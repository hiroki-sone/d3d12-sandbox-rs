use std::mem;
use windows::Win32::Graphics::{Direct3D12::*, Dxgi::Common::*};

use super::d3d12::{device::*, resource, view::Srv};
use super::math::*;

type OnUpdate = dyn FnMut(f64) -> Mat4;

pub struct Mesh {
    vertex_count: usize,
    #[allow(unused)]
    position_buffer: ID3D12Resource,
    position_format: DXGI_FORMAT,
    position_vbv: D3D12_VERTEX_BUFFER_VIEW,
    position_srv: Srv,

    #[allow(unused)]
    normal_buffer: ID3D12Resource,
    normal_vbv: D3D12_VERTEX_BUFFER_VIEW,
    normal_srv: Srv,

    index_count: usize,
    #[allow(unused)]
    index_buffer: ID3D12Resource,
    ibv: D3D12_INDEX_BUFFER_VIEW,
    index_srv: Srv,

    transform: [f32; 12],
    transposed_inv_transform: [f32; 12],

    material: Material,

    on_update: Box<OnUpdate>,
}

impl Mesh {
    pub fn load(
        device: &mut Device,
        mesh: &MeshResource,
        material: Material,
        on_update: Box<OnUpdate>,
    ) -> windows::core::Result<Self> {
        let ctx = device.request_copy_command_ctx()?;
        let command_list = ctx.command_list();

        let position_buffer_size = mem::size_of_val(&mesh.positions[0]) * mesh.positions.len();

        let positions = resource::create_buffer_with_data(
            device,
            D3D12_HEAP_TYPE_UPLOAD,
            D3D12_RESOURCE_FLAG_NONE,
            D3D12_RESOURCE_STATE_COMMON,
            &mesh.positions,
            "Intermediate vertex buffer",
        )?;

        let position_buffer = resource::create_buffer(
            device,
            position_buffer_size as u64,
            D3D12_HEAP_TYPE_DEFAULT,
            D3D12_RESOURCE_FLAG_NONE,
            D3D12_RESOURCE_STATE_COMMON,
            &format!("{}::position_buffer", mesh.name),
        )?;

        unsafe { command_list.CopyResource(&position_buffer, &positions) };

        let position_vbv = D3D12_VERTEX_BUFFER_VIEW {
            BufferLocation: unsafe { position_buffer.GetGPUVirtualAddress() },
            SizeInBytes: position_buffer_size as u32,
            StrideInBytes: mem::size_of_val(&mesh.positions[0]) as u32,
        };

        let position_srv_desc = D3D12_SHADER_RESOURCE_VIEW_DESC {
            Format: DXGI_FORMAT_UNKNOWN,
            ViewDimension: D3D12_SRV_DIMENSION_BUFFER,
            Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
            Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                Buffer: D3D12_BUFFER_SRV {
                    FirstElement: 0,
                    NumElements: mesh.positions.len() as u32,
                    StructureByteStride: mem::size_of_val(&mesh.positions[0]) as u32,
                    Flags: D3D12_BUFFER_SRV_FLAG_NONE,
                },
            },
        };
        let position_srv = device.create_srv(Some(&position_buffer), Some(&position_srv_desc));

        let normal_buffer_size = mem::size_of_val(&mesh.normals[0]) * mesh.normals.len();

        let normals = resource::create_buffer_with_data(
            device,
            D3D12_HEAP_TYPE_UPLOAD,
            D3D12_RESOURCE_FLAG_NONE,
            D3D12_RESOURCE_STATE_COMMON,
            &mesh.normals,
            "Intermediate normal buffer",
        )?;

        let normal_buffer = resource::create_buffer(
            device,
            normal_buffer_size as u64,
            D3D12_HEAP_TYPE_DEFAULT,
            D3D12_RESOURCE_FLAG_NONE,
            D3D12_RESOURCE_STATE_COMMON,
            &format!("{}::normal_buffer", mesh.name),
        )?;

        unsafe { command_list.CopyResource(&normal_buffer, &normals) };

        let normal_vbv = D3D12_VERTEX_BUFFER_VIEW {
            BufferLocation: unsafe { normal_buffer.GetGPUVirtualAddress() } as u64,
            SizeInBytes: normal_buffer_size as u32,
            StrideInBytes: mem::size_of_val(&mesh.normals[0]) as u32,
        };

        let normal_srv_desc = D3D12_SHADER_RESOURCE_VIEW_DESC {
            Format: DXGI_FORMAT_UNKNOWN,
            ViewDimension: D3D12_SRV_DIMENSION_BUFFER,
            Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
            Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                Buffer: D3D12_BUFFER_SRV {
                    FirstElement: 0,
                    NumElements: mesh.normals.len() as u32,
                    StructureByteStride: mem::size_of_val(&mesh.normals[0]) as u32,
                    Flags: D3D12_BUFFER_SRV_FLAG_NONE,
                },
            },
        };
        let normal_srv = device.create_srv(Some(&normal_buffer), Some(&normal_srv_desc));

        let index_buffer_size = mem::size_of_val(&mesh.indices[0]) * mesh.indices.len();
        let index_size = mem::size_of_val(&mesh.indices[0]);
        let indices = resource::create_buffer_with_data(
            device,
            D3D12_HEAP_TYPE_UPLOAD,
            D3D12_RESOURCE_FLAG_NONE,
            D3D12_RESOURCE_STATE_COMMON,
            &mesh.indices,
            "intermediate index buffer",
        )?;

        let index_buffer = resource::create_buffer(
            device,
            index_buffer_size as u64,
            D3D12_HEAP_TYPE_DEFAULT,
            D3D12_RESOURCE_FLAG_NONE,
            D3D12_RESOURCE_STATE_COMMON,
            &format!("{}::_index_buffer", mesh.name),
        )?;

        unsafe { command_list.CopyResource(&index_buffer, &indices) };

        let index_format = if index_size == mem::size_of::<u16>() {
            DXGI_FORMAT_R16_UINT
        } else {
            DXGI_FORMAT_R32_UINT
        };
        let ibv = D3D12_INDEX_BUFFER_VIEW {
            BufferLocation: unsafe { index_buffer.GetGPUVirtualAddress() },
            Format: index_format,
            SizeInBytes: index_buffer_size as u32,
        };

        let index_srv_desc = D3D12_SHADER_RESOURCE_VIEW_DESC {
            Format: DXGI_FORMAT_UNKNOWN,
            ViewDimension: D3D12_SRV_DIMENSION_BUFFER,
            Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
            Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                Buffer: D3D12_BUFFER_SRV {
                    FirstElement: 0,
                    NumElements: mesh.indices.len() as u32,
                    StructureByteStride: index_size as u32,
                    Flags: D3D12_BUFFER_SRV_FLAG_NONE,
                },
            },
        };
        let index_srv = device.create_srv(Some(&index_buffer), Some(&index_srv_desc));

        // make sure vertex and index buffers are uploaded to the GPU memory
        let command_queue = device.copy_queue_mut();
        let fence_value = command_queue.execute_commands(ctx).unwrap();
        command_queue.wait_fence(fence_value);

        Ok(Mesh {
            vertex_count: mesh.positions.len(),
            position_format: DXGI_FORMAT_R32G32B32_FLOAT,
            position_buffer,
            position_vbv,
            position_srv,

            normal_buffer,
            normal_vbv,
            normal_srv,

            index_count: mesh.indices.len(),
            index_buffer,
            ibv,
            index_srv,

            transform: mat4_to_row_marjor_float3x4(&Mat4::IDENTITY),
            transposed_inv_transform: mat4_to_row_marjor_float3x4(&Mat4::IDENTITY),

            material,

            on_update,
        })
    }

    pub fn update(&mut self, time: f64) {
        let transform = (self.on_update)(time);
        self.transform = mat4_to_row_marjor_float3x4(&transform);

        let transposed_inv_transform = transform.inverse().transpose();
        self.transposed_inv_transform = mat4_to_row_marjor_float3x4(&transposed_inv_transform);
    }

    pub fn vertex_buffer_views(&self) -> [D3D12_VERTEX_BUFFER_VIEW; 2] {
        [self.position_vbv, self.normal_vbv]
    }

    pub fn position_buffer_view(&self) -> &D3D12_VERTEX_BUFFER_VIEW {
        &self.position_vbv
    }

    pub fn position_srv(&self) -> &Srv {
        &self.position_srv
    }

    pub fn vertex_count(&self) -> usize {
        self.vertex_count
    }

    pub fn position_format(&self) -> DXGI_FORMAT {
        self.position_format
    }

    pub fn index_buffer_view(&self) -> &D3D12_INDEX_BUFFER_VIEW {
        &self.ibv
    }

    pub fn index_count(&self) -> usize {
        self.index_count
    }

    pub fn index_srv(&self) -> &Srv {
        &self.index_srv
    }

    pub fn normal_srv(&self) -> &Srv {
        &self.normal_srv
    }

    pub fn transform(&self) -> &[f32; 12] {
        &self.transform
    }

    pub fn transposed_inv_transform(&self) -> &[f32; 12] {
        &self.transposed_inv_transform
    }

    pub fn material(&self) -> &Material {
        &self.material
    }
}

pub struct MeshResource {
    indices: Vec<u32>,
    positions: Vec<Vec3>,
    normals: Vec<Vec3>,
    name: String,
}

impl MeshResource {
    pub fn new(indices: &[u32], positions: &[Vec3], normals: &[Vec3], name: String) -> Self {
        MeshResource {
            indices: Vec::from(indices),
            positions: Vec::from(positions),
            normals: Vec::from(normals),
            name,
        }
    }
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct Material {
    pub base_color: Vec3,
    pub metallic: f32,
    pub specular_reflectance: Vec3,
    pub roughness: f32,
    pub specular_tint: Vec3,
    pub pad: u32,
}

pub mod obj {
    use glam::Vec3;

    use super::MeshResource;

    pub fn load(path: &str) -> Result<Vec<super::MeshResource>, tobj::LoadError> {
        let (models, _materials) = tobj::load_obj(path, &tobj::GPU_LOAD_OPTIONS)?;

        let resources = models
            .iter()
            .map(|model| {
                let name = if model.name.is_empty() {
                    path.to_string()
                } else {
                    format!("{path}:{}", model.name)
                };

                let mesh = &model.mesh;
                let mut positions = Vec::new();
                for i in (0..mesh.positions.len()).step_by(3) {
                    let p = Vec3::new(
                        mesh.positions[i],
                        mesh.positions[i + 1],
                        mesh.positions[i + 2],
                    );
                    positions.push(p);
                }

                let mut normals = Vec::new();
                for i in (0..mesh.normals.len()).step_by(3) {
                    let n = Vec3::new(mesh.normals[i], mesh.normals[i + 1], mesh.normals[i + 2]);
                    normals.push(n);
                }

                MeshResource {
                    indices: model.mesh.indices.clone(),
                    positions,
                    normals,
                    name,
                }
            })
            .collect();

        Ok(resources)
    }
}
