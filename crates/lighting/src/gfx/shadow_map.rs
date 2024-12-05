use std::{mem, path::PathBuf};

use windows::core as winapi;
use windows::Win32::Foundation::FALSE;
use windows::Win32::Graphics::{Direct3D::*, Direct3D12::*, Dxgi::Common::*};

use super::d3d12::{
    device::*,
    pix::*,
    pso,
    resource::*,
    shader::*,
    view,
    view::{Dsv, Srv},
};
use super::{math::*, scene::Scene};

pub struct ShadowMap {
    texture: ID3D12Resource,
    width: u32,
    height: u32,

    dsv: Dsv,
    srv: Srv,
}

impl ShadowMap {
    pub fn build(
        device: &mut Device,
        width: u32,
        height: u32,
        format: DXGI_FORMAT,
        name: &str,
    ) -> winapi::Result<Self> {
        let clear_value: D3D12_CLEAR_VALUE = D3D12_CLEAR_VALUE {
            Format: format,
            Anonymous: D3D12_CLEAR_VALUE_0 {
                DepthStencil: D3D12_DEPTH_STENCIL_VALUE {
                    Depth: 1.0,
                    Stencil: 0,
                },
            },
        };

        let texture = create_texture2d(
            device,
            (width, height),
            format,
            D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL,
            D3D12_RESOURCE_STATE_DEPTH_WRITE,
            Some(&clear_value),
            name,
        )?;

        let dsv_desc = D3D12_DEPTH_STENCIL_VIEW_DESC {
            Format: format,
            ViewDimension: D3D12_DSV_DIMENSION_TEXTURE2D,
            Flags: D3D12_DSV_FLAG_NONE,
            Anonymous: D3D12_DEPTH_STENCIL_VIEW_DESC_0 {
                Texture2D: D3D12_TEX2D_DSV { MipSlice: 0 },
            },
        };

        let dsv = device.create_dsv(&texture, Some(&dsv_desc));

        let srv_format = match format {
            DXGI_FORMAT_D16_UNORM => DXGI_FORMAT_R16_UNORM,
            DXGI_FORMAT_D32_FLOAT => DXGI_FORMAT_R32_FLOAT,
            _ => panic!("Invalid format: {}", format.0),
        };

        let srv_desc = D3D12_SHADER_RESOURCE_VIEW_DESC {
            Format: srv_format,
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

        let srv = device.create_srv(Some(&texture), Some(&srv_desc));

        Ok(Self {
            texture,
            width,
            height,
            dsv,
            srv,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn texture(&self) -> &ID3D12Resource {
        &self.texture
    }

    pub fn srv(&self) -> &Srv {
        &self.srv
    }
}

pub struct ShadowMapPass {
    root_signature: ID3D12RootSignature,
    pso: ID3D12PipelineState,
}

impl ShadowMapPass {
    pub fn build(device: &mut Device, format: DXGI_FORMAT, name: &str) -> winapi::Result<Self> {
        let root_signature = create_root_signature(device, &format!("{name}::root_signature"))?;

        let pso = create_pso(device, &root_signature, format, &format!("{name}::pso"))?;

        Ok(Self {
            root_signature,
            pso,
        })
    }

    pub fn clear(&self, command_list: &ID3D12GraphicsCommandList7, shadow_map: &ShadowMap) {
        let rects = [];
        unsafe {
            command_list.ClearDepthStencilView(
                shadow_map.dsv.cpu_handle(),
                D3D12_CLEAR_FLAG_DEPTH,
                1.0,
                0,
                &rects,
            )
        };
    }

    pub fn draw(
        &self,
        command_list: &ID3D12GraphicsCommandList7,
        scene: &Scene,
        shadow_map: &ShadowMap,
        pix: Option<&Pix>,
    ) {
        let color = pix_color(0, 255, 0);
        let _event = pix.map(|p| p.begin_event(command_list, color, "Draw shadow maps"));

        self.clear(command_list, shadow_map);

        let light = scene.light();

        let aspect_ratio = (shadow_map.width as f32) / (shadow_map.height as f32);
        let view_projection = light.view_projection(aspect_ratio);

        let rect = windows::Win32::Foundation::RECT {
            left: 0,
            top: 0,
            right: shadow_map.width.try_into().unwrap(),
            bottom: shadow_map.height.try_into().unwrap(),
        };

        let viewport = D3D12_VIEWPORT {
            TopLeftX: 0.0,
            TopLeftY: 0.0,
            Width: shadow_map.width as f32,
            Height: shadow_map.height as f32,
            MinDepth: D3D12_MIN_DEPTH,
            MaxDepth: D3D12_MAX_DEPTH,
        };

        unsafe {
            command_list.SetPipelineState(&self.pso);
            command_list.SetGraphicsRootSignature(&self.root_signature);

            command_list.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);

            command_list.RSSetViewports(&[viewport]);
            command_list.RSSetScissorRects(&[rect]);

            command_list.OMSetRenderTargets(0, None, FALSE, Some(&shadow_map.dsv.cpu_handle()));

            command_list.SetGraphicsRoot32BitConstants(
                1,
                (mem::size_of_val(&view_projection) / mem::size_of::<f32>()) as u32,
                &view_projection as *const _ as _,
                0,
            );
        }

        for (i, mesh) in scene.meshes().iter().enumerate() {
            unsafe {
                let vbv = mesh.position_buffer_view();
                command_list.IASetVertexBuffers(0, Some(&[*vbv]));

                command_list.IASetIndexBuffer(Some(mesh.index_buffer_view()));

                let resources = ResourceHandles {
                    mesh_transform: scene.transform_srv().handle(),
                    mesh_id: i as u32,
                };
                command_list.SetGraphicsRoot32BitConstants(
                    0,
                    ResourceHandles::COUNT,
                    resources.as_ptr(),
                    0,
                );

                command_list.DrawIndexedInstanced(mesh.index_count() as u32, 1, 0, 0, 0);
            }
        }
    }
}

pub fn create_root_signature(
    device: &Device,
    name: &str,
) -> windows::core::Result<ID3D12RootSignature> {
    let flags = D3D12_ROOT_SIGNATURE_FLAG_CBV_SRV_UAV_HEAP_DIRECTLY_INDEXED
        | D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT
        | D3D12_ROOT_SIGNATURE_FLAG_DENY_HULL_SHADER_ROOT_ACCESS
        | D3D12_ROOT_SIGNATURE_FLAG_DENY_DOMAIN_SHADER_ROOT_ACCESS
        | D3D12_ROOT_SIGNATURE_FLAG_DENY_GEOMETRY_SHADER_ROOT_ACCESS
        | D3D12_ROOT_SIGNATURE_FLAG_DENY_PIXEL_SHADER_ROOT_ACCESS;

    let params = [
        D3D12_ROOT_PARAMETER1 {
            ParameterType: D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS,
            Anonymous: D3D12_ROOT_PARAMETER1_0 {
                Constants: D3D12_ROOT_CONSTANTS {
                    ShaderRegister: 0,
                    RegisterSpace: 0,
                    Num32BitValues: ResourceHandles::COUNT,
                },
            },
            ShaderVisibility: D3D12_SHADER_VISIBILITY_VERTEX,
        },
        D3D12_ROOT_PARAMETER1 {
            ParameterType: D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS,
            Anonymous: D3D12_ROOT_PARAMETER1_0 {
                Constants: D3D12_ROOT_CONSTANTS {
                    ShaderRegister: 1,
                    RegisterSpace: 0,
                    Num32BitValues: (mem::size_of::<Mat4>() / mem::size_of::<f32>()) as u32,
                },
            },
            ShaderVisibility: D3D12_SHADER_VISIBILITY_VERTEX,
        },
    ];

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

    pso::create_root_signature(device, &desc, name)
}

fn create_pso(
    device: &Device,
    root_signature: &ID3D12RootSignature,
    dsv_format: DXGI_FORMAT,
    name: &str,
) -> windows::core::Result<ID3D12PipelineState> {
    let shader_compiler = ShaderCompiler::build(true)?;

    let hlsl: PathBuf = "shaders/lighting/shadow_map.hlsl".into();
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

    let input_layout = [D3D12_INPUT_ELEMENT_DESC {
        SemanticName: windows::core::s!("POSITION"),
        SemanticIndex: 0,
        Format: DXGI_FORMAT_R32G32B32_FLOAT,
        InputSlot: 0,
        AlignedByteOffset: D3D12_APPEND_ALIGNED_ELEMENT,
        InputSlotClass: D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA,
        InstanceDataStepRate: 0,
    }];

    let desc = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
        pRootSignature: unsafe { mem::transmute_copy(root_signature) },

        VS: D3D12_SHADER_BYTECODE {
            pShaderBytecode: unsafe { vertex_shader.GetBufferPointer() },
            BytecodeLength: unsafe { vertex_shader.GetBufferSize() },
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
        NumRenderTargets: 0,
        DSVFormat: dsv_format,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },

        ..Default::default()
    };

    pso::create_gfx_pso(device, &desc, name)
}

#[repr(C)]
struct ResourceHandles {
    mesh_transform: u32,
    mesh_id: u32,
}

view::impl_resource_handles!(ResourceHandles);
