use core::f32;
use std::mem;
use std::path::PathBuf;

use super::{
    d3d12::{
        barrier,
        device::*,
        pix::*,
        raytracing,
        resource::*,
        shader::*,
        util::*,
        view::{Dsv, Srv, Uav},
    },
    scene::Scene,
};

use super::math::*;

use windows::Win32::Foundation::{E_FAIL, FALSE, HWND};
use windows::Win32::Graphics::{Direct3D::*, Direct3D12::*, Dxgi::Common::*};

pub enum RenderingMode {
    Raytracing,
    Rasterization,
}

pub struct Renderer {
    device: Device,

    color_buffer: ID3D12Resource,
    color_srv: Srv,
    color_uav: Uav,

    #[allow(dead_code)]
    depth_buffer: ID3D12Resource,
    depth_dsv: Dsv,

    viewport_width: u32,
    viewport_height: u32,

    pix: Option<Pix>,

    mode: RenderingMode,

    draw_mesh_root_signature: ID3D12RootSignature,
    draw_mesh_pso: ID3D12PipelineState,

    raytracing_root_signature: ID3D12RootSignature,
    raytracing_pso: ID3D12PipelineState,

    copy_root_signature: ID3D12RootSignature,
    copy_pso: ID3D12PipelineState,
}

impl Renderer {
    pub fn new(hwnd: HWND, viewport_width: u32, viewport_height: u32) -> Self {
        let mut device = Device::build(hwnd, viewport_width, viewport_height).unwrap();

        let color_buffer_format = DXGI_FORMAT_R32G32B32A32_FLOAT;
        let color_buffer = create_texture2d(
            &device,
            (viewport_width, viewport_height),
            color_buffer_format,
            D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET | D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
            D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
            None,
            "Renderer::color_buffer",
        )
        .unwrap();

        let color_srv_desc = D3D12_SHADER_RESOURCE_VIEW_DESC {
            Format: color_buffer_format,
            ViewDimension: D3D12_SRV_DIMENSION_TEXTURE2D,
            Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
            Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                Texture2D: D3D12_TEX2D_SRV {
                    MostDetailedMip: 0,
                    MipLevels: 1,
                    PlaneSlice: 0,
                    ResourceMinLODClamp: 0.0,
                },
            },
        };

        let color_srv = device.create_srv(Some(&color_buffer), Some(&color_srv_desc));

        let color_uav_desc = D3D12_UNORDERED_ACCESS_VIEW_DESC {
            Format: color_buffer_format,
            ViewDimension: D3D12_UAV_DIMENSION_TEXTURE2D,
            Anonymous: D3D12_UNORDERED_ACCESS_VIEW_DESC_0 {
                Texture2D: {
                    D3D12_TEX2D_UAV {
                        MipSlice: 0,
                        PlaneSlice: 0,
                    }
                },
            },
        };

        let color_uav = device.create_uav(&color_buffer, Some(&color_uav_desc));

        let (depth_buffer, depth_dsv) = create_depth_buffer(
            &mut device,
            viewport_width,
            viewport_height,
            "Renderer::depth_buffer",
        )
        .unwrap();

        let pix = Pix::build()
            .inspect_err(|e| eprintln!("Failed to load PIX module: {e}"))
            .ok();

        let draw_mesh_root_signature = create_draw_mesh_root_signature(&device).unwrap();

        let draw_mesh_pso = create_draw_mesh_pso(&device, &draw_mesh_root_signature).unwrap();

        let cs = ShaderConfig {
            path: "shaders/raytracing.hlsl".into(),
            entry_point: "main".into(),
            target: "cs_6_6".into(),
        };

        let raytracing_root_signature = create_raytracing_root_signature(&device).unwrap();
        let raytracing_pso =
            create_compute_pso(&device, &cs, &raytracing_root_signature, "raytracing_pso").unwrap();

        let copy_root_signature = copy_texture_root_signature(&device).unwrap();
        let copy_pso = create_copy_texture_pso(&device, &copy_root_signature).unwrap();

        Self {
            device,

            color_buffer,
            color_srv,
            color_uav,

            depth_buffer,
            depth_dsv,

            viewport_width,
            viewport_height,

            mode: RenderingMode::Raytracing,

            pix,

            draw_mesh_root_signature,
            draw_mesh_pso,

            raytracing_root_signature,
            raytracing_pso,

            copy_root_signature,
            copy_pso,
        }
    }

    pub fn render(&mut self, scene: &mut Scene) -> windows::core::Result<()> {
        let ctx = self.device.request_gfx_command_ctx()?;
        let cmd_list = ctx.command_list();

        let pix = self.pix.as_ref();

        let rect = windows::Win32::Foundation::RECT {
            left: 0,
            top: 0,
            right: self.viewport_width.try_into().unwrap(),
            bottom: self.viewport_height.try_into().unwrap(),
        };

        let viewport = D3D12_VIEWPORT {
            TopLeftX: 0.0,
            TopLeftY: 0.0,
            Width: self.viewport_width as f32,
            Height: self.viewport_height as f32,
            MinDepth: D3D12_MIN_DEPTH,
            MaxDepth: D3D12_MAX_DEPTH,
        };

        unsafe { cmd_list.SetDescriptorHeaps(&[Some(self.device.view_heap().clone())]) };

        {
            let color = pix_color(0, 255, 0);
            let _event = pix.map(|p| p.begin_event(cmd_list, color, "Update scene"));
            scene.update_buffers(&mut self.device, cmd_list)?;
        }

        let back_buffer = self.device.back_buffer();

        {
            let color = pix_color(0, 255, 0);
            let _event = pix.map(|p| p.begin_event(cmd_list, color, "Render"));

            let barriers = [barrier::transition(
                back_buffer,
                D3D12_RESOURCE_STATE_PRESENT,
                D3D12_RESOURCE_STATE_RENDER_TARGET,
            )];
            unsafe { cmd_list.ResourceBarrier(&barriers) };

            self.clear_buffers(cmd_list, pix);

            unsafe {
                cmd_list.RSSetViewports(&[viewport]);
                cmd_list.RSSetScissorRects(&[rect]);
                cmd_list.OMSetRenderTargets(
                    1,
                    Some(&self.device.back_buffer_rtv().cpu_handle()),
                    FALSE,
                    Some(&self.depth_dsv.cpu_handle()),
                );
            }

            match &self.mode {
                RenderingMode::Rasterization => {
                    self.draw_mesh(cmd_list, scene, pix);
                }
                RenderingMode::Raytracing => {
                    self.raytrace(cmd_list, scene, pix);
                }
            }
        }

        let barriers = [barrier::transition(
            back_buffer,
            D3D12_RESOURCE_STATE_RENDER_TARGET,
            D3D12_RESOURCE_STATE_PRESENT,
        )];

        unsafe { cmd_list.ResourceBarrier(&barriers) }

        self.device.present_frame(ctx)?;

        Ok(())
    }

    pub fn device(&self) -> &Device {
        &self.device
    }

    pub fn device_mut(&mut self) -> &mut Device {
        &mut self.device
    }

    fn clear_buffers(&self, cmd_list: &ID3D12GraphicsCommandList7, pix: Option<&Pix>) {
        const CLEAR_COLOR: [f32; 4] = [0.4, 0.6, 0.9, 1.0];

        let color = pix_color(0, 255, 0);
        let _event = pix.map(|p| p.begin_event(cmd_list, color, "Clear buffers"));

        unsafe {
            cmd_list.ClearRenderTargetView(
                self.device.back_buffer_rtv().cpu_handle(),
                &CLEAR_COLOR,
                None,
            )
        };

        let rects = [];
        unsafe {
            cmd_list.ClearDepthStencilView(
                self.depth_dsv.cpu_handle(),
                D3D12_CLEAR_FLAG_DEPTH,
                1.0,
                0,
                &rects,
            )
        };
    }

    fn draw_mesh(&self, cmd_list: &ID3D12GraphicsCommandList7, scene: &Scene, pix: Option<&Pix>) {
        let color = pix_color(0, 255, 0);
        let _event = pix.map(|p| p.begin_event(cmd_list, color, "Draw Cube"));

        let mesh = scene.mesh();

        unsafe {
            cmd_list.SetPipelineState(&self.draw_mesh_pso);
            cmd_list.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);

            let vbvs = mesh.vertex_buffer_views();
            cmd_list.IASetVertexBuffers(0, Some(&vbvs));

            cmd_list.IASetIndexBuffer(Some(mesh.index_buffer_view()));

            cmd_list.SetGraphicsRootSignature(&self.draw_mesh_root_signature);

            let resources = DrawMeshResourceHandles {
                camera: scene.camera_cbv().handle(),
                transform: scene.transform_srv().handle(),
            };
            cmd_list.SetGraphicsRoot32BitConstants(
                0,
                DrawMeshResourceHandles::COUNT,
                resources.as_ptr(),
                0,
            );

            cmd_list.DrawIndexedInstanced(mesh.index_count() as u32, 1, 0, 0, 0);
        }
    }

    fn raytrace(&self, cmd_list: &ID3D12GraphicsCommandList7, scene: &Scene, pix: Option<&Pix>) {
        let color = pix_color(0, 255, 0);
        let _event = pix.map(|p| p.begin_event(cmd_list, color, "Raytrace Cube"));

        unsafe {
            cmd_list.SetPipelineState(&self.raytracing_pso);

            cmd_list.SetComputeRootSignature(&self.raytracing_root_signature);

            let resources = RaytracingResourceHandles {
                mesh_data: scene.raytracing_scene().mesh_data().clone(),
                camera: scene.camera_cbv().handle(),
                output: self.color_uav.handle(),
                raytracing_scene: scene.raytracing_scene().srv().unwrap().handle(),
            };
            cmd_list.SetComputeRoot32BitConstants(
                0,
                RaytracingResourceHandles::COUNT,
                resources.as_ptr(),
                0,
            );

            const NUM_THREAD_X: u32 = 8;
            const NUM_THREAD_Y: u32 = 8;
            let x = divide_and_round_up(self.viewport_width, NUM_THREAD_X);
            let y = divide_and_round_up(self.viewport_height, NUM_THREAD_Y);
            cmd_list.Dispatch(x, y, 1);
        }

        let barriers = [
            barrier::uav(&self.color_buffer),
            barrier::transition(
                &self.color_buffer,
                D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
                D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
            ),
        ];

        unsafe {
            cmd_list.ResourceBarrier(&barriers);

            // copy the color buffer to the frame buffer
            cmd_list.SetPipelineState(&self.copy_pso);
            cmd_list.SetGraphicsRootSignature(&self.copy_root_signature);

            let resources = CopyResourceHandles {
                camera: scene.camera_cbv().handle(),
                src_texture: self.color_srv.handle(),
            };
            cmd_list.SetGraphicsRoot32BitConstants(
                0,
                CopyResourceHandles::COUNT,
                resources.as_ptr(),
                0,
            );

            cmd_list.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);

            cmd_list.IASetVertexBuffers(0, None);

            cmd_list.DrawInstanced(3, 1, 0, 0);
        }

        let barriers = [barrier::transition(
            &self.color_buffer,
            D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
            D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
        )];

        unsafe { cmd_list.ResourceBarrier(&barriers) };
    }

    pub fn toggle_rendering_mode(&mut self) {
        self.mode = match self.mode {
            RenderingMode::Rasterization => {
                println!("Switched to Raytracing");
                RenderingMode::Raytracing
            }
            RenderingMode::Raytracing => {
                println!("Switched to Rasterization");
                RenderingMode::Rasterization
            }
        };
    }
}

fn create_depth_buffer(
    device: &mut Device,
    width: u32,
    height: u32,
    name: &str,
) -> windows::core::Result<(ID3D12Resource, Dsv)> {
    const CLEAR_VALUE: D3D12_CLEAR_VALUE = D3D12_CLEAR_VALUE {
        Format: DXGI_FORMAT_D32_FLOAT,
        Anonymous: D3D12_CLEAR_VALUE_0 {
            DepthStencil: D3D12_DEPTH_STENCIL_VALUE {
                Depth: 1.0,
                Stencil: 0,
            },
        },
    };

    let depth_buffer = create_texture2d(
        device,
        (width, height),
        DXGI_FORMAT_D32_FLOAT,
        D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL,
        D3D12_RESOURCE_STATE_DEPTH_WRITE,
        Some(&CLEAR_VALUE),
        name,
    )?;

    let dsv_desc = D3D12_DEPTH_STENCIL_VIEW_DESC {
        Format: DXGI_FORMAT_D32_FLOAT,
        ViewDimension: D3D12_DSV_DIMENSION_TEXTURE2D,
        Flags: D3D12_DSV_FLAG_NONE,
        Anonymous: D3D12_DEPTH_STENCIL_VIEW_DESC_0 {
            Texture2D: D3D12_TEX2D_DSV { MipSlice: 0 },
        },
    };

    let dsv = device.create_dsv(&depth_buffer, Some(&dsv_desc));

    Ok((depth_buffer, dsv))
}

fn create_root_signature(
    device: &Device,
    desc: &D3D12_VERSIONED_ROOT_SIGNATURE_DESC,
    name: &str,
) -> windows::core::Result<ID3D12RootSignature> {
    let mut blob = None;
    let mut error = None;
    unsafe { D3D12SerializeVersionedRootSignature(desc, &mut blob, Some(&mut error)) }?;

    if let Some(e) = error {
        let message = unsafe { std::ffi::CStr::from_ptr(e.GetBufferPointer() as _) };
        return Err(windows::core::Error::new(E_FAIL, message.to_str().unwrap()));
    }

    let blob = blob.unwrap();
    let root_signature: ID3D12RootSignature = unsafe {
        let data =
            std::slice::from_raw_parts(blob.GetBufferPointer() as *const u8, blob.GetBufferSize());
        device.get().CreateRootSignature(0, data)
    }?;

    set_name_str(&root_signature, name)?;

    Ok(root_signature)
}

fn create_gfx_pso(
    device: &Device,
    desc: &D3D12_GRAPHICS_PIPELINE_STATE_DESC,
    name: &str,
) -> windows::core::Result<ID3D12PipelineState> {
    let pso: ID3D12PipelineState = unsafe { device.get().CreateGraphicsPipelineState(desc) }?;
    set_name_str(&pso, name)?;
    Ok(pso)
}

fn create_compute_pso(
    device: &Device,
    shader: &ShaderConfig,
    root_signature: &ID3D12RootSignature,
    name: &str,
) -> windows::core::Result<ID3D12PipelineState> {
    let shader_compiler = ShaderCompiler::build(true)?;

    let shader = shader_compiler.compile_file(shader)?;

    let desc = D3D12_COMPUTE_PIPELINE_STATE_DESC {
        CS: D3D12_SHADER_BYTECODE {
            pShaderBytecode: unsafe { shader.GetBufferPointer() },
            BytecodeLength: unsafe { shader.GetBufferSize() },
        },
        pRootSignature: unsafe { mem::transmute_copy(root_signature) },
        Flags: D3D12_PIPELINE_STATE_FLAG_NONE,
        ..Default::default()
    };

    let pso: ID3D12PipelineState = unsafe { device.get().CreateComputePipelineState(&desc) }?;
    set_name_str(&pso, name)?;
    Ok(pso)
}

macro_rules! impl_resource_handles {
    ($struct_name:ident) => {
        impl $struct_name {
            const COUNT: u32 =
                (std::mem::size_of::<$struct_name>() / std::mem::size_of::<u32>()) as u32;

            fn as_ptr(&self) -> *const std::ffi::c_void {
                self as *const $struct_name as *const std::ffi::c_void
            }
        }
    };
}

#[repr(C)]
struct DrawMeshResourceHandles {
    camera: u32,
    transform: u32,
}
impl_resource_handles!(DrawMeshResourceHandles);

fn create_draw_mesh_root_signature(device: &Device) -> windows::core::Result<ID3D12RootSignature> {
    let flags = D3D12_ROOT_SIGNATURE_FLAG_CBV_SRV_UAV_HEAP_DIRECTLY_INDEXED
        | D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT
        | D3D12_ROOT_SIGNATURE_FLAG_DENY_HULL_SHADER_ROOT_ACCESS
        | D3D12_ROOT_SIGNATURE_FLAG_DENY_DOMAIN_SHADER_ROOT_ACCESS
        | D3D12_ROOT_SIGNATURE_FLAG_DENY_GEOMETRY_SHADER_ROOT_ACCESS
        | D3D12_ROOT_SIGNATURE_FLAG_DENY_PIXEL_SHADER_ROOT_ACCESS;

    let params = [D3D12_ROOT_PARAMETER1 {
        ParameterType: D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS,
        Anonymous: D3D12_ROOT_PARAMETER1_0 {
            Constants: D3D12_ROOT_CONSTANTS {
                ShaderRegister: 0,
                RegisterSpace: 0,
                Num32BitValues: DrawMeshResourceHandles::COUNT,
            },
        },
        ShaderVisibility: D3D12_SHADER_VISIBILITY_VERTEX,
    }];

    let desc = D3D12_VERSIONED_ROOT_SIGNATURE_DESC {
        Version: D3D_ROOT_SIGNATURE_VERSION_1_1,
        Anonymous: D3D12_VERSIONED_ROOT_SIGNATURE_DESC_0 {
            Desc_1_1: D3D12_ROOT_SIGNATURE_DESC1 {
                NumParameters: params.len() as u32,
                pParameters: params.as_ptr(),
                NumStaticSamplers: 0,
                pStaticSamplers: std::ptr::null(),
                Flags: flags,
            },
        },
    };

    create_root_signature(device, &desc, "draw_mesh_root_signature")
}

fn create_draw_mesh_pso(
    device: &Device,
    root_signature: &ID3D12RootSignature,
) -> windows::core::Result<ID3D12PipelineState> {
    let shader_compiler = ShaderCompiler::build(true)?;

    let hlsl: PathBuf = "shaders/rasterization.hlsl".into();
    let vs_config = ShaderConfig {
        path: hlsl.clone(),
        entry_point: "vs_main".into(),
        target: "vs_6_6".into(),
    };
    let vertex_shader = match shader_compiler.compile_file(&vs_config) {
        Ok(shader) => shader,
        Err(e) => panic!(
            "Failed to compile {} {}: {e}",
            hlsl.as_os_str().to_str().unwrap(),
            vs_config.entry_point
        ),
    };

    let ps_config = ShaderConfig {
        path: hlsl.clone(),
        entry_point: "ps_main".into(),
        target: "ps_6_6".into(),
    };
    let pixel_shader = match shader_compiler.compile_file(&ps_config) {
        Ok(shader) => shader,
        Err(e) => panic!(
            "Failed to compile {} {}: {e}",
            hlsl.as_os_str().to_str().unwrap(),
            ps_config.entry_point
        ),
    };

    let input_layout = [
        D3D12_INPUT_ELEMENT_DESC {
            SemanticName: windows::core::s!("POSITION"),
            SemanticIndex: 0,
            Format: DXGI_FORMAT_R32G32B32_FLOAT,
            InputSlot: 0,
            AlignedByteOffset: D3D12_APPEND_ALIGNED_ELEMENT,
            InputSlotClass: D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA,
            InstanceDataStepRate: 0,
        },
        D3D12_INPUT_ELEMENT_DESC {
            SemanticName: windows::core::s!("COLOR"),
            SemanticIndex: 0,
            Format: DXGI_FORMAT_R32G32B32_FLOAT,
            InputSlot: 1,
            AlignedByteOffset: D3D12_APPEND_ALIGNED_ELEMENT,
            InputSlotClass: D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA,
            InstanceDataStepRate: 0,
        },
    ];

    let mut desc = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
        pRootSignature: unsafe { mem::transmute_copy(root_signature) },

        VS: D3D12_SHADER_BYTECODE {
            pShaderBytecode: unsafe { vertex_shader.GetBufferPointer() },
            BytecodeLength: unsafe { vertex_shader.GetBufferSize() },
        },

        PS: D3D12_SHADER_BYTECODE {
            pShaderBytecode: unsafe { pixel_shader.GetBufferPointer() },
            BytecodeLength: unsafe { pixel_shader.GetBufferSize() },
        },

        BlendState: D3D12_BLEND_DESC {
            AlphaToCoverageEnable: false.into(),
            IndependentBlendEnable: false.into(),
            RenderTarget: [Default::default(); 8],
        },
        SampleMask: u32::MAX,
        RasterizerState: D3D12_RASTERIZER_DESC {
            FillMode: D3D12_FILL_MODE_SOLID,
            CullMode: D3D12_CULL_MODE_BACK,
            ..Default::default()
        },
        DepthStencilState: D3D12_DEPTH_STENCIL_DESC {
            DepthEnable: true.into(),
            DepthWriteMask: D3D12_DEPTH_WRITE_MASK_ALL,
            DepthFunc: D3D12_COMPARISON_FUNC_LESS,
            StencilEnable: false.into(),
            ..Default::default()
        },

        InputLayout: D3D12_INPUT_LAYOUT_DESC {
            pInputElementDescs: input_layout.as_ptr(),
            NumElements: input_layout.len() as u32,
        },

        PrimitiveTopologyType: D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
        NumRenderTargets: 1,
        DSVFormat: DXGI_FORMAT_D32_FLOAT,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },

        ..Default::default()
    };

    desc.BlendState.RenderTarget[0] = D3D12_RENDER_TARGET_BLEND_DESC {
        BlendEnable: false.into(),
        LogicOpEnable: false.into(),
        SrcBlend: D3D12_BLEND_ONE,
        DestBlend: D3D12_BLEND_ZERO,
        BlendOp: D3D12_BLEND_OP_ADD,
        SrcBlendAlpha: D3D12_BLEND_ONE,
        DestBlendAlpha: D3D12_BLEND_ZERO,
        BlendOpAlpha: D3D12_BLEND_OP_ADD,
        LogicOp: D3D12_LOGIC_OP_NOOP,
        RenderTargetWriteMask: D3D12_COLOR_WRITE_ENABLE_ALL.0 as u8,
    };
    desc.RTVFormats[0] = FRAME_BUFFER_FORMAT;

    create_gfx_pso(device, &desc, "draw_mesh_pso")
}

#[repr(C)]
struct RaytracingResourceHandles {
    mesh_data: [raytracing::MeshData; raytracing::MAX_MESH_DATA_COUNT],
    camera: u32,
    output: u32,
    raytracing_scene: u32,
}
impl_resource_handles!(RaytracingResourceHandles);

fn create_raytracing_root_signature(device: &Device) -> windows::core::Result<ID3D12RootSignature> {
    let flags = D3D12_ROOT_SIGNATURE_FLAG_CBV_SRV_UAV_HEAP_DIRECTLY_INDEXED
        | D3D12_ROOT_SIGNATURE_FLAG_DENY_VERTEX_SHADER_ROOT_ACCESS
        | D3D12_ROOT_SIGNATURE_FLAG_DENY_HULL_SHADER_ROOT_ACCESS
        | D3D12_ROOT_SIGNATURE_FLAG_DENY_DOMAIN_SHADER_ROOT_ACCESS
        | D3D12_ROOT_SIGNATURE_FLAG_DENY_GEOMETRY_SHADER_ROOT_ACCESS
        | D3D12_ROOT_SIGNATURE_FLAG_DENY_PIXEL_SHADER_ROOT_ACCESS;

    let params = [D3D12_ROOT_PARAMETER1 {
        ParameterType: D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS,
        Anonymous: D3D12_ROOT_PARAMETER1_0 {
            Constants: D3D12_ROOT_CONSTANTS {
                ShaderRegister: 0,
                RegisterSpace: 0,
                Num32BitValues: RaytracingResourceHandles::COUNT,
            },
        },
        ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
    }];

    let desc = D3D12_VERSIONED_ROOT_SIGNATURE_DESC {
        Version: D3D_ROOT_SIGNATURE_VERSION_1_1,
        Anonymous: D3D12_VERSIONED_ROOT_SIGNATURE_DESC_0 {
            Desc_1_1: D3D12_ROOT_SIGNATURE_DESC1 {
                NumParameters: params.len() as u32,
                pParameters: params.as_ptr(),
                NumStaticSamplers: 0,
                pStaticSamplers: std::ptr::null(),
                Flags: flags,
            },
        },
    };

    create_root_signature(device, &desc, "raytracing_root_signature")
}

#[repr(C)]
struct CopyResourceHandles {
    camera: u32,
    src_texture: u32,
}
impl_resource_handles!(CopyResourceHandles);

fn copy_texture_root_signature(device: &Device) -> windows::core::Result<ID3D12RootSignature> {
    let flags = D3D12_ROOT_SIGNATURE_FLAG_CBV_SRV_UAV_HEAP_DIRECTLY_INDEXED
        | D3D12_ROOT_SIGNATURE_FLAG_DENY_VERTEX_SHADER_ROOT_ACCESS
        | D3D12_ROOT_SIGNATURE_FLAG_DENY_HULL_SHADER_ROOT_ACCESS
        | D3D12_ROOT_SIGNATURE_FLAG_DENY_DOMAIN_SHADER_ROOT_ACCESS
        | D3D12_ROOT_SIGNATURE_FLAG_DENY_GEOMETRY_SHADER_ROOT_ACCESS;

    let params = [D3D12_ROOT_PARAMETER1 {
        ParameterType: D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS,
        Anonymous: D3D12_ROOT_PARAMETER1_0 {
            Constants: D3D12_ROOT_CONSTANTS {
                ShaderRegister: 0,
                RegisterSpace: 0,
                Num32BitValues: CopyResourceHandles::COUNT,
            },
        },
        ShaderVisibility: D3D12_SHADER_VISIBILITY_PIXEL,
    }];

    let desc = D3D12_VERSIONED_ROOT_SIGNATURE_DESC {
        Version: D3D_ROOT_SIGNATURE_VERSION_1_1,
        Anonymous: D3D12_VERSIONED_ROOT_SIGNATURE_DESC_0 {
            Desc_1_1: D3D12_ROOT_SIGNATURE_DESC1 {
                NumParameters: params.len() as u32,
                pParameters: params.as_ptr(),
                NumStaticSamplers: 0,
                pStaticSamplers: std::ptr::null(),
                Flags: flags,
            },
        },
    };

    create_root_signature(device, &desc, "copy_root_signature")
}

fn create_copy_texture_pso(
    device: &Device,
    root_signature: &ID3D12RootSignature,
) -> windows::core::Result<ID3D12PipelineState> {
    let shader_compiler = ShaderCompiler::build(true)?;

    let hlsl: PathBuf = "shaders/fullscreen.hlsl".into();
    let vs_config = ShaderConfig {
        path: hlsl.clone(),
        entry_point: "vs_main".into(),
        target: "vs_6_6".into(),
    };
    let vertex_shader = match shader_compiler.compile_file(&vs_config) {
        Ok(shader) => shader,
        Err(e) => panic!(
            "Failed to compile {} {}: {e}",
            hlsl.as_os_str().to_str().unwrap(),
            vs_config.entry_point
        ),
    };

    let ps_config = ShaderConfig {
        path: hlsl.clone(),
        entry_point: "copy_ps".into(),
        target: "ps_6_6".into(),
    };
    let pixel_shader = match shader_compiler.compile_file(&ps_config) {
        Ok(shader) => shader,
        Err(e) => panic!(
            "Failed to compile {} {}: {e}",
            hlsl.as_os_str().to_str().unwrap(),
            ps_config.entry_point
        ),
    };

    let mut desc = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
        pRootSignature: unsafe { mem::transmute_copy(root_signature) },

        VS: D3D12_SHADER_BYTECODE {
            pShaderBytecode: unsafe { vertex_shader.GetBufferPointer() },
            BytecodeLength: unsafe { vertex_shader.GetBufferSize() },
        },

        PS: D3D12_SHADER_BYTECODE {
            pShaderBytecode: unsafe { pixel_shader.GetBufferPointer() },
            BytecodeLength: unsafe { pixel_shader.GetBufferSize() },
        },

        BlendState: D3D12_BLEND_DESC {
            AlphaToCoverageEnable: false.into(),
            IndependentBlendEnable: false.into(),
            RenderTarget: [Default::default(); 8],
        },
        SampleMask: u32::MAX,
        RasterizerState: D3D12_RASTERIZER_DESC {
            FillMode: D3D12_FILL_MODE_SOLID,
            CullMode: D3D12_CULL_MODE_BACK,
            ..Default::default()
        },
        DepthStencilState: D3D12_DEPTH_STENCIL_DESC {
            DepthEnable: false.into(),
            StencilEnable: false.into(),
            ..Default::default()
        },

        InputLayout: D3D12_INPUT_LAYOUT_DESC {
            pInputElementDescs: std::ptr::null(),
            NumElements: 0,
        },

        PrimitiveTopologyType: D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
        NumRenderTargets: 1,

        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },

        ..Default::default()
    };

    desc.BlendState.RenderTarget[0] = D3D12_RENDER_TARGET_BLEND_DESC {
        BlendEnable: false.into(),
        LogicOpEnable: false.into(),
        SrcBlend: D3D12_BLEND_ONE,
        DestBlend: D3D12_BLEND_ZERO,
        BlendOp: D3D12_BLEND_OP_ADD,
        SrcBlendAlpha: D3D12_BLEND_ONE,
        DestBlendAlpha: D3D12_BLEND_ZERO,
        BlendOpAlpha: D3D12_BLEND_OP_ADD,
        LogicOp: D3D12_LOGIC_OP_NOOP,
        RenderTargetWriteMask: D3D12_COLOR_WRITE_ENABLE_ALL.0 as u8,
    };
    desc.RTVFormats[0] = FRAME_BUFFER_FORMAT;

    create_gfx_pso(device, &desc, "copy_texture_pso")
}
