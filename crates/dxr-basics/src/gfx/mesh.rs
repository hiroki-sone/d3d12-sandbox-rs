use windows::Win32::Graphics::{Direct3D12::*, Dxgi::Common::*};

use super::d3d12::{device::*, resource, view::Srv};
use super::math::*;

pub struct Mesh {
    vertex_count: usize,
    #[allow(unused)]
    vertex_buffer: ID3D12Resource,
    position_vbv: D3D12_VERTEX_BUFFER_VIEW,

    vertex_format: DXGI_FORMAT,
    color_vbv: D3D12_VERTEX_BUFFER_VIEW,
    color_srv: Srv,

    index_count: usize,
    #[allow(unused)]
    index_buffer: ID3D12Resource,
    ibv: D3D12_INDEX_BUFFER_VIEW,
    index_srv: Srv,
}

impl Mesh {
    pub fn load(device: &mut Device) -> windows::core::Result<Self> {
        const VERTEX_SIZE: usize = std::mem::size_of::<f32>() * 3;
        let vertex_buffer_size = std::mem::size_of_val(&MESH_VERTICES);

        let vertices = resource::create_buffer_with_data(
            device,
            D3D12_HEAP_TYPE_UPLOAD,
            D3D12_RESOURCE_FLAG_NONE,
            D3D12_RESOURCE_STATE_COMMON,
            &MESH_VERTICES,
            "Intermediate vertex buffer",
        )?;

        let vertex_buffer = resource::create_buffer(
            device,
            vertex_buffer_size as u64,
            D3D12_HEAP_TYPE_DEFAULT,
            D3D12_RESOURCE_FLAG_NONE,
            D3D12_RESOURCE_STATE_COMMON,
            "Vertex buffer",
        )?;

        let ctx = device.request_copy_command_ctx()?;
        let command_list = ctx.command_list();

        unsafe { command_list.CopyResource(&vertex_buffer, &vertices) };

        let vertex_buffer_address = unsafe { vertex_buffer.GetGPUVirtualAddress() };
        let position_vbv = D3D12_VERTEX_BUFFER_VIEW {
            BufferLocation: vertex_buffer_address,
            SizeInBytes: (VERTEX_SIZE * MESH_VERTEX_COUNT) as u32,
            StrideInBytes: VERTEX_SIZE as u32,
        };

        let color_vbv = D3D12_VERTEX_BUFFER_VIEW {
            BufferLocation: vertex_buffer_address + (VERTEX_SIZE * MESH_VERTEX_COUNT) as u64,
            SizeInBytes: (VERTEX_SIZE * MESH_VERTEX_COUNT) as u32,
            StrideInBytes: VERTEX_SIZE as u32,
        };

        let color_srv_desc = D3D12_SHADER_RESOURCE_VIEW_DESC {
            Format: DXGI_FORMAT_UNKNOWN,
            ViewDimension: D3D12_SRV_DIMENSION_BUFFER,
            Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
            Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                Buffer: D3D12_BUFFER_SRV {
                    FirstElement: MESH_VERTEX_COUNT as u64,
                    NumElements: MESH_VERTEX_COUNT as u32,
                    StructureByteStride: (std::mem::size_of::<f32>() * 3) as u32,
                    Flags: D3D12_BUFFER_SRV_FLAG_NONE,
                },
            },
        };
        let color_srv = device.create_srv(Some(&vertex_buffer), Some(&color_srv_desc));

        let index_buffer_size = std::mem::size_of_val(&MESH_INDICES);
        let index_size = std::mem::size_of_val(&MESH_INDICES[0]);
        let indices = resource::create_buffer_with_data(
            device,
            D3D12_HEAP_TYPE_UPLOAD,
            D3D12_RESOURCE_FLAG_NONE,
            D3D12_RESOURCE_STATE_COMMON,
            &MESH_INDICES,
            "intermediate index buffer",
        )?;

        let index_buffer = resource::create_buffer(
            device,
            index_buffer_size as u64,
            D3D12_HEAP_TYPE_DEFAULT,
            D3D12_RESOURCE_FLAG_NONE,
            D3D12_RESOURCE_STATE_COMMON,
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

        let index_srv_desc = D3D12_SHADER_RESOURCE_VIEW_DESC {
            Format: DXGI_FORMAT_UNKNOWN,
            ViewDimension: D3D12_SRV_DIMENSION_BUFFER,
            Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
            Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                Buffer: D3D12_BUFFER_SRV {
                    FirstElement: 0,
                    NumElements: MESH_INDICES.len() as u32,
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
            vertex_count: MESH_VERTEX_COUNT,
            vertex_format: DXGI_FORMAT_R32G32B32_FLOAT,
            vertex_buffer,
            position_vbv,
            color_vbv,
            color_srv,

            index_count: MESH_INDICES.len(),
            index_buffer,
            ibv,
            index_srv,
        })
    }

    pub fn vertex_buffer_views(&self) -> [D3D12_VERTEX_BUFFER_VIEW; 2] {
        [self.position_vbv, self.color_vbv]
    }

    pub fn position_buffer_view(&self) -> &D3D12_VERTEX_BUFFER_VIEW {
        &self.position_vbv
    }

    pub fn vertex_count(&self) -> usize {
        self.vertex_count
    }

    pub fn position_format(&self) -> DXGI_FORMAT {
        self.vertex_format
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

    pub fn color_srv(&self) -> &Srv {
        &self.color_srv
    }
}

const MESH_VERTEX_COUNT: usize = 8;
#[rustfmt::skip]
const MESH_VERTICES: [[f32; 3]; MESH_VERTEX_COUNT * 2] = [
    // position
    [-1.0, -1.0, -1.0],
    [-1.0,  1.0, -1.0],
    [ 1.0,  1.0, -1.0],
    [ 1.0, -1.0, -1.0],
    [-1.0, -1.0,  1.0],
    [-1.0,  1.0,  1.0],
    [ 1.0,  1.0,  1.0],
    [ 1.0, -1.0,  1.0],
    // color
    [0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0],
    [1.0, 1.0, 0.0],
    [1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0],
    [0.0, 1.0, 1.0],
    [1.0, 1.0, 1.0],
    [1.0, 0.0, 1.0] 
];

// winding order is clockwise
#[rustfmt::skip]
const MESH_INDICES: [u32; 36] = [
    0, 1, 2, 0, 2, 3,
    4, 6, 5, 4, 7, 6,
    4, 5, 1, 4, 1, 0,
    3, 2, 6, 3, 6, 7,
    1, 5, 6, 1, 6, 2,
    4, 0, 3, 4, 3, 7,
];
