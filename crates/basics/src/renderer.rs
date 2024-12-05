use core::f32;
use std::mem;
use std::path::PathBuf;

use crate::d3d12::{device::Device, shader::ShaderCompiler};

use super::{
    d3d12::{
        barrier, device,
        pix::{pix_color, Pix},
        resource,
        shader::ShaderConfig,
    },
    math,
};

use windows::Win32::Foundation::{E_FAIL, FALSE, HWND};
use windows::Win32::Graphics::{Direct3D::*, Direct3D12::*, Dxgi::Common::*};

use glam::Vec3;

pub struct Renderer {
    device: device::Device,

    #[allow(dead_code)]
    depth_buffer: ID3D12Resource,

    screen_width: u32,
    screen_height: u32,

    pix: Option<Pix>,

    timer: std::time::Instant,
    model_view_proj: math::Mat4,
    mesh: Mesh,
    root_signature: ID3D12RootSignature,
    pso: ID3D12PipelineState,
}

impl Renderer {
    pub fn new(hwnd: HWND, screen_width: u32, screen_height: u32) -> Self {
        let mut device = device::Device::build(hwnd, screen_width, screen_height).unwrap();

        create_frame_buffer_rtvs(&device);

        let depth_buffer = create_depth_buffer(&device, screen_width, screen_height).unwrap();

        let pix = Pix::build()
            .inspect_err(|e| eprintln!("Failed to load PIX module: {e}"))
            .ok();

        let mesh = load_mesh(&mut device).unwrap();

        let root_signature = create_root_signature(&device).unwrap();

        let pso = create_pso(&device, &root_signature).unwrap();

        Self {
            device,

            depth_buffer,

            screen_width,
            screen_height,

            timer: std::time::Instant::now(),

            pix,

            model_view_proj: math::Mat4::IDENTITY,
            mesh,
            root_signature,
            pso,
        }
    }

    pub fn update(&mut self) {
        let total_time = self.timer.elapsed().as_secs_f64();

        let angle_deg = (total_time * 90.0) % 360.0;
        let angle = angle_deg.to_radians();
        let rotation_axis = math::Vec3::new(0.0, 1.0, 1.0).normalize();
        let model = math::Mat4::from_axis_angle(rotation_axis, angle as f32);

        let eye = Vec3::new(0.0, 0.0, -10.0);
        let center: Vec3 = Vec3::ZERO;
        let up = Vec3::new(0.0, 1.0, 0.0);
        // let view = math::Mat4::look_at_rh(eye, center, up);
        let view = math::Mat4::look_at_lh(eye, center, up);

        let fov = 45.0 * f32::consts::PI / 180.0;
        let aspect_ratio = (self.screen_width as f32) / (self.screen_height as f32);
        // let projection = math::Mat4::perspective_rh(fov, aspect_ratio, 0.1, 100.0);
        let projection = math::Mat4::perspective_lh(fov, aspect_ratio, 0.1, 100.0);

        self.model_view_proj = projection * view * model;
    }

    pub fn render(&mut self) -> windows::core::Result<()> {
        let ctx = self.device.request_gfx_command_ctx()?;
        let cmd_list = ctx.command_list();

        let pix = self.pix.as_ref();

        let back_buffer = self.device.back_buffer();

        {
            let color = pix_color(0, 255, 0);
            let _event = pix.map(|p| p.begin_event(cmd_list, color, "Render"));

            let rect = windows::Win32::Foundation::RECT {
                left: 0,
                top: 0,
                right: self.screen_width.try_into().unwrap(),
                bottom: self.screen_height.try_into().unwrap(),
            };

            let viewport = D3D12_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: self.screen_width as f32,
                Height: self.screen_height as f32,
                MinDepth: D3D12_MIN_DEPTH,
                MaxDepth: D3D12_MAX_DEPTH,
            };

            let mut rtv = unsafe { self.device.rtv_heap().GetCPUDescriptorHandleForHeapStart() };
            rtv.ptr += self.device.back_buffer_index() * (self.device.rtv_size() as usize);
            let dsv = unsafe { self.device.dsv_heap().GetCPUDescriptorHandleForHeapStart() };

            {
                let _event =
                    pix.map(|p| p.begin_event(cmd_list, color, "Clear frame buffer"));

                let barriers = [barrier::transition_barrier(
                    back_buffer,
                    D3D12_RESOURCE_STATE_PRESENT,
                    D3D12_RESOURCE_STATE_RENDER_TARGET,
                )];
                unsafe { cmd_list.ResourceBarrier(&barriers) };

                const CLEAR_COLOR: [f32; 4] = [0.4, 0.6, 0.9, 1.0];

                unsafe {
                    cmd_list
                        .ClearRenderTargetView(rtv, &CLEAR_COLOR, None)
                };

                let rects = [];
                unsafe {
                    cmd_list.ClearDepthStencilView(
                        dsv,
                        D3D12_CLEAR_FLAG_DEPTH,
                        1.0,
                        0,
                        &rects,
                    )
                };
            }

            {
                let color = pix_color(0, 255, 0);
                let _event = pix.map(|p| p.begin_event(cmd_list, color, "Draw Cube"));

                let matrix = self.model_view_proj.as_ref().as_ptr() as *const std::ffi::c_void;
                let constant_count =
                    mem::size_of_val(&self.model_view_proj) / mem::size_of::<f32>();

                unsafe {
                    cmd_list.RSSetViewports(&[viewport]);
                    cmd_list.RSSetScissorRects(&[rect]);
                    cmd_list
                        .OMSetRenderTargets(1, Some(&rtv), FALSE, Some(&dsv));

                    cmd_list.SetPipelineState(&self.pso);
                    cmd_list
                        .IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                    cmd_list
                        .IASetVertexBuffers(0, Some(&[self.mesh.vbv]));

                    cmd_list.IASetIndexBuffer(Some(&self.mesh.ibv));

                    cmd_list
                        .SetGraphicsRootSignature(&self.root_signature);
                    cmd_list.SetGraphicsRoot32BitConstants(
                        0,
                        constant_count as u32,
                        matrix,
                        0,
                    );

                    cmd_list
                        .DrawIndexedInstanced(INDICES.len() as u32, 1, 0, 0, 0);
                }
            }
        }

        {
            let barriers = [barrier::transition_barrier(
                back_buffer,
                D3D12_RESOURCE_STATE_RENDER_TARGET,
                D3D12_RESOURCE_STATE_PRESENT,
            )];

            unsafe { cmd_list.ResourceBarrier(&barriers) }

            self.device.present_frame(ctx)?;
        }

        Ok(())
    }
}

fn create_frame_buffer_rtvs(device: &Device) {
    let mut rtv = unsafe { device.rtv_heap().GetCPUDescriptorHandleForHeapStart() };

    for buffer in device.frame_buffers() {
        unsafe { device.get().CreateRenderTargetView(buffer, None, rtv) };
        rtv.ptr += device.rtv_size() as usize;
    }
}

fn create_depth_buffer(
    device: &device::Device,
    width: u32,
    height: u32,
) -> windows::core::Result<ID3D12Resource> {
    if width == 0 || height == 0 {
        return Err(windows::core::Error::new(
            E_FAIL,
            "The width and the hright must be grater than zero",
        ));
    }

    const CLEAR_VALUE: D3D12_CLEAR_VALUE = D3D12_CLEAR_VALUE {
        Format: DXGI_FORMAT_D32_FLOAT,
        Anonymous: D3D12_CLEAR_VALUE_0 {
            DepthStencil: D3D12_DEPTH_STENCIL_VALUE {
                Depth: 1.0,
                Stencil: 0,
            },
        },
    };

    let properties = resource::heap_properties(D3D12_HEAP_TYPE_DEFAULT);

    let desc = resource::texture2d_desc(
        DXGI_FORMAT_D32_FLOAT,
        width.into(),
        height,
        D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL,
    );

    let mut depth_buffer: Option<ID3D12Resource> = None;
    unsafe {
        device.get().CreateCommittedResource(
            &properties,
            D3D12_HEAP_FLAG_NONE,
            &desc,
            D3D12_RESOURCE_STATE_DEPTH_WRITE,
            Some(&CLEAR_VALUE),
            &mut depth_buffer,
        )
    }?;
    let depth_buffer = depth_buffer.unwrap();

    let dsv_desc = D3D12_DEPTH_STENCIL_VIEW_DESC {
        Format: DXGI_FORMAT_D32_FLOAT,
        ViewDimension: D3D12_DSV_DIMENSION_TEXTURE2D,
        Flags: D3D12_DSV_FLAG_NONE,
        Anonymous: D3D12_DEPTH_STENCIL_VIEW_DESC_0 {
            Texture2D: D3D12_TEX2D_DSV { MipSlice: 0 },
        },
    };

    unsafe {
        device.get().CreateDepthStencilView(
            &depth_buffer,
            Some(&dsv_desc),
            device.dsv_heap().GetCPUDescriptorHandleForHeapStart(),
        )
    };
    Ok(depth_buffer)
}

#[derive(Debug)]
#[repr(C)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 3],
}

#[rustfmt::skip]
const VERTICES: [Vertex; 8] = [
    Vertex { position: [-1.0, -1.0, -1.0], color: [0.0, 0.0, 0.0] },
    Vertex { position: [-1.0,  1.0, -1.0], color: [0.0, 1.0, 0.0] },
    Vertex { position: [ 1.0,  1.0, -1.0], color: [1.0, 1.0, 0.0] },
    Vertex { position: [ 1.0, -1.0, -1.0], color: [1.0, 0.0, 0.0] },
    Vertex { position: [-1.0, -1.0,  1.0], color: [0.0, 0.0, 1.0] },
    Vertex { position: [-1.0,  1.0,  1.0], color: [0.0, 1.0, 1.0] },
    Vertex { position: [ 1.0,  1.0,  1.0], color: [1.0, 1.0, 1.0] },
    Vertex { position: [ 1.0, -1.0,  1.0], color: [1.0, 0.0, 1.0] } 
];

// winding order is clockwise
#[rustfmt::skip]
const INDICES: [u16; 36] = [
    0, 1, 2, 0, 2, 3,
    4, 6, 5, 4, 7, 6,
    4, 5, 1, 4, 1, 0,
    3, 2, 6, 3, 6, 7,
    1, 5, 6, 1, 6, 2,
    4, 0, 3, 4, 3, 7,
];

struct Mesh {
    #[allow(unused)]
    vertex_buffer: ID3D12Resource,
    vbv: D3D12_VERTEX_BUFFER_VIEW,
    #[allow(unused)]
    index_buffer: ID3D12Resource,
    ibv: D3D12_INDEX_BUFFER_VIEW,
}

fn load_mesh(device: &mut device::Device) -> windows::core::Result<Mesh> {
    const VERTEX_SIZE: usize = std::mem::size_of::<Vertex>();
    let vertex_buffer_size = std::mem::size_of_val(&VERTICES);

    let vertices = resource::create_buffer(
        device,
        vertex_buffer_size as u64,
        Some(&VERTICES),
        D3D12_HEAP_TYPE_UPLOAD,
        D3D12_RESOURCE_FLAG_NONE,
        "Intermediate vertex buffer",
    )?;

    let vertex_buffer = resource::create_buffer::<()>(
        device,
        vertex_buffer_size as u64,
        None,
        D3D12_HEAP_TYPE_DEFAULT,
        D3D12_RESOURCE_FLAG_NONE,
        "Vertex buffer",
    )?;

    let ctx = device.request_copy_command_ctx()?;
    let command_list = ctx.command_list();

    unsafe { command_list.CopyResource(&vertex_buffer, &vertices) };

    let vbv = D3D12_VERTEX_BUFFER_VIEW {
        BufferLocation: unsafe { vertex_buffer.GetGPUVirtualAddress() },
        SizeInBytes: vertex_buffer_size as u32,
        StrideInBytes: VERTEX_SIZE as u32,
    };

    let index_buffer_size = std::mem::size_of_val(&INDICES);
    let index_size = std::mem::size_of_val(&INDICES[0]);
    let indices = resource::create_buffer(
        device,
        index_buffer_size as u64,
        Some(&INDICES),
        D3D12_HEAP_TYPE_UPLOAD,
        D3D12_RESOURCE_FLAG_NONE,
        "intermediate index buffer",
    )?;

    let index_buffer = resource::create_buffer::<()>(
        device,
        index_buffer_size as u64,
        None,
        D3D12_HEAP_TYPE_DEFAULT,
        D3D12_RESOURCE_FLAG_NONE,
        "Index buffer",
    )?;

    unsafe { command_list.CopyResource(&index_buffer, &indices) };

    let index_format = if index_size == std::mem::size_of::<u16>() {
        DXGI_FORMAT_R16_UINT
    } else {
        DXGI_FORMAT_R32_UINT
    };
    let ibv = D3D12_INDEX_BUFFER_VIEW {
        BufferLocation: unsafe { index_buffer.GetGPUVirtualAddress() },
        Format: index_format,
        SizeInBytes: index_buffer_size as u32,
    };

    // make sure vertex and index buffers are uploaded to the GPU memory
    let command_queue = device.copy_command_queue_mut();
    let fence_value = command_queue.execute_commands(ctx).unwrap();
    command_queue.wait_fence(fence_value);

    Ok(Mesh {
        vertex_buffer,
        vbv,
        index_buffer,
        ibv,
    })
}

fn create_root_signature(device: &device::Device) -> windows::core::Result<ID3D12RootSignature> {
    let flags = D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT
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
                Num32BitValues: (std::mem::size_of::<math::Mat4>() / std::mem::size_of::<f32>())
                    as u32,
            },
        },
        ShaderVisibility: D3D12_SHADER_VISIBILITY_VERTEX,
    }];

    let root_signature_desc = D3D12_VERSIONED_ROOT_SIGNATURE_DESC {
        Version: D3D_ROOT_SIGNATURE_VERSION_1_1,
        Anonymous: D3D12_VERSIONED_ROOT_SIGNATURE_DESC_0 {
            Desc_1_1: D3D12_ROOT_SIGNATURE_DESC1 {
                NumParameters: 1,
                pParameters: params.as_ptr(),
                NumStaticSamplers: 0,
                pStaticSamplers: std::ptr::null(),
                Flags: flags,
            },
        },
    };

    let mut blob = None;
    let mut error = None;
    unsafe {
        D3D12SerializeVersionedRootSignature(&root_signature_desc, &mut blob, Some(&mut error))
    }?;
    if let Some(e) = error {
        let message = unsafe { std::ffi::CStr::from_ptr(e.GetBufferPointer() as _) };
        return Err(windows::core::Error::new(E_FAIL, message.to_str().unwrap()));
    }

    let blob = blob.unwrap();
    unsafe {
        let root_signature_blob =
            std::slice::from_raw_parts(blob.GetBufferPointer() as *const u8, blob.GetBufferSize());
        device.get().CreateRootSignature(0, root_signature_blob)
    }
}

fn create_pso(
    device: &Device,
    root_signature: &ID3D12RootSignature,
) -> windows::core::Result<ID3D12PipelineState> {
    let shader_compiler = ShaderCompiler::build(true)?;

    let hlsl: PathBuf = "shaders/basics/basics.hlsl".into();
    let vs_config = ShaderConfig {
        path: hlsl.clone(),
        entry_point: "vs_main".into(),
        target: "vs_6_0".into(),
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
        target: "ps_6_0".into(),
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
            InputSlot: 0,
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
    desc.RTVFormats[0] = DXGI_FORMAT_R8G8B8A8_UNORM;

    unsafe { device.get().CreateGraphicsPipelineState(&desc) }
}
