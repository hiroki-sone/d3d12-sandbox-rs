use windows::Win32::Graphics::{Direct3D12::*, Dxgi::Common::*};

use super::{
    barrier,
    device::{self, Device},
    resource,
    view::Srv,
};

use crate::gfx::{math, mesh::Mesh};

pub struct RaytracingScene {
    blas_list: Vec<Blas>,

    tlas: Tlas,
    srv: Option<Srv>,

    mesh_data: [MeshData; MAX_MESH_DATA_COUNT],

    name: String,
}

impl RaytracingScene {
    pub fn new(
        tlas_flags: D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAGS,
        name: String,
    ) -> Self {
        let tlas = Tlas::new(TlasId { v: 0 }, tlas_flags);
        Self {
            blas_list: Vec::new(),
            tlas,
            srv: None,
            mesh_data: Default::default(),
            name,
        }
    }

    pub fn add_blas(
        &mut self,
        build_flags: D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAGS,
    ) -> BlasId {
        let id = BlasId {
            v: self.blas_list.len(),
        };
        self.blas_list.push(Blas::new(id, build_flags));
        id
    }

    pub fn add_mesh(&mut self, blas_id: BlasId, mesh: &Mesh, transform_address: Option<u64>) {
        let _total_geometry_count = self
            .blas_list
            .iter()
            .fold(0, |acc, g| acc + g.geometries.len());
        debug_assert!(_total_geometry_count < MAX_MESH_DATA_COUNT);

        let geometry = D3D12_RAYTRACING_GEOMETRY_DESC {
            Type: D3D12_RAYTRACING_GEOMETRY_TYPE_TRIANGLES,
            Flags: D3D12_RAYTRACING_GEOMETRY_FLAG_OPAQUE,
            Anonymous: D3D12_RAYTRACING_GEOMETRY_DESC_0 {
                Triangles: D3D12_RAYTRACING_GEOMETRY_TRIANGLES_DESC {
                    Transform3x4: transform_address.unwrap_or(0),
                    IndexFormat: mesh.index_buffer_view().Format,
                    VertexFormat: mesh.position_format(),
                    IndexCount: mesh.index_count() as u32,
                    VertexCount: mesh.vertex_count() as u32,
                    IndexBuffer: mesh.index_buffer_view().BufferLocation,
                    VertexBuffer: D3D12_GPU_VIRTUAL_ADDRESS_AND_STRIDE {
                        StartAddress: mesh.position_buffer_view().BufferLocation,
                        StrideInBytes: mesh.position_buffer_view().StrideInBytes.into(),
                    },
                },
            },
        };
        self.blas_list[blas_id.v].add_geometry(geometry);

        self.mesh_data[blas_id.v] = MeshData {
            index_buffer_handle: mesh.index_srv().handle(),
            color_buffer_handle: mesh.color_srv().handle(),
            ..Default::default()
        };
    }

    pub fn build(&mut self, device: &mut Device) -> windows::core::Result<()> {
        let ctx = device.request_gfx_command_ctx()?;
        let command_list = ctx.command_list();

        let mut blas_barriers = Vec::with_capacity(self.blas_list.len());

        for blas in &mut self.blas_list {
            let name = format!("{}::blas[{}]", self.name, blas.id.v);
            blas.allocate_buffers(device, &name)?;

            let buf = blas.scratch_buffer.as_ref().unwrap();
            let barrier = barrier::transition(
                buf,
                D3D12_RESOURCE_STATE_COMMON,
                D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
            );
            blas_barriers.push(barrier);
        }

        unsafe {
            command_list.ResourceBarrier(&blas_barriers);
        };

        blas_barriers.clear();

        let tlas = &mut self.tlas;

        for blas in &mut self.blas_list {
            blas.build(command_list);
            let buf = blas.buffer.as_ref().unwrap();
            blas_barriers.push(barrier::uav(buf));

            tlas.add_instance(blas, None);
        }

        unsafe { command_list.ResourceBarrier(&blas_barriers) };

        let name = format!("{}::tlas[{}]::instance_buffer", self.name, tlas.id.v);
        tlas.init_instance_buffer(device, &name)?;

        let name = format!("{}::tlas[{}]", self.name, tlas.id.v);
        let created_srv = tlas.allocate_buffers(device, &name);

        tlas.build(command_list)?;

        if created_srv {
            self.init_srv(device);
        }

        let command_queue = device.gfx_queue_mut();
        let fence_value = command_queue.execute_commands(ctx).unwrap();
        command_queue.wait_fence(fence_value);

        Ok(())
    }

    pub fn update(
        &mut self,
        device: &mut Device,
        cmd_list: &ID3D12GraphicsCommandList7,
    ) -> windows::core::Result<()> {
        let mut barriers = Vec::with_capacity(self.blas_list.len());

        for blas in &mut self.blas_list {
            blas.update(cmd_list);
            barriers.push(barrier::uav(blas.buffer.as_ref().unwrap()));
        }

        unsafe { cmd_list.ResourceBarrier(&barriers) };

        let name = format!("{}::tlas[{}]", self.name, self.tlas.id.v);
        let create_srv = self.tlas.allocate_buffers(device, &name);

        self.tlas.build(cmd_list)?;

        if create_srv {
            self.init_srv(device);
        }

        let barrier = barrier::uav(self.tlas.buffer.as_ref().unwrap());
        unsafe { cmd_list.ResourceBarrier(&[barrier]) };

        Ok(())
    }

    pub fn init_srv(&mut self, device: &mut Device) {
        let desc = D3D12_SHADER_RESOURCE_VIEW_DESC {
            Format: DXGI_FORMAT_UNKNOWN,
            ViewDimension: D3D12_SRV_DIMENSION_RAYTRACING_ACCELERATION_STRUCTURE,
            Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
            Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                RaytracingAccelerationStructure: D3D12_RAYTRACING_ACCELERATION_STRUCTURE_SRV {
                    Location: unsafe { self.tlas.buffer.as_ref().unwrap().GetGPUVirtualAddress() },
                },
            },
        };

        // The first arg must be None: passing the actual buffer would cause an error:
        // ID3D12Device::CreateShaderResourceView: When ViewDimension is D3D12_SRV_DIMENSION_RAYTRACING_ACCELERATION_STRUCTURE,
        // pResource must be NULL, since the resource location comes from a GPUVA in pDesc.
        self.srv = Some(device.create_srv(None, Some(&desc)));
    }

    pub fn srv(&self) -> Option<&Srv> {
        self.srv.as_ref()
    }

    pub fn mesh_data(&self) -> &[MeshData; MAX_MESH_DATA_COUNT] {
        &self.mesh_data
    }
}

#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct BlasId {
    v: usize,
}

#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct TlasId {
    v: usize,
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct MeshData {
    index_buffer_handle: u32,
    color_buffer_handle: u32,
    _reserved: [u32; 2],
}

// must match MAX_MESH_DATA_COUNT in raytracing.hlsl
pub const MAX_MESH_DATA_COUNT: usize = 1;

#[derive(Debug, PartialEq, Eq)]
enum BuildMode {
    FullBuild,
    Update,
}

struct Blas {
    id: BlasId,

    buffer: Option<ID3D12Resource>,
    scratch_buffer: Option<ID3D12Resource>,

    build_flags: D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAGS,

    geometries: Vec<D3D12_RAYTRACING_GEOMETRY_DESC>,
}

impl Blas {
    fn new(id: BlasId, build_flags: D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAGS) -> Self {
        Blas {
            id,
            buffer: None,
            scratch_buffer: None,
            build_flags,
            geometries: Vec::new(),
        }
    }

    fn add_geometry(&mut self, geometry: D3D12_RAYTRACING_GEOMETRY_DESC) {
        self.geometries.push(geometry);
    }

    fn allocate_buffers(
        &mut self,
        device: &device::Device,
        name: &str,
    ) -> windows::core::Result<()> {
        let inputs = self.inputs(BuildMode::FullBuild);

        let mut info = Default::default();
        unsafe {
            device
                .get()
                .GetRaytracingAccelerationStructurePrebuildInfo(&inputs, &mut info)
        };

        let scratch_buffer_name = "Scratch buffer for ".to_string() + name;
        let scratch_buffer = resource::create_buffer(
            device,
            info.ScratchDataSizeInBytes,
            D3D12_HEAP_TYPE_DEFAULT,
            D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
            D3D12_RESOURCE_STATE_COMMON,
            &scratch_buffer_name,
        )?;

        let buffer = resource::create_buffer(
            device,
            info.ResultDataMaxSizeInBytes,
            D3D12_HEAP_TYPE_DEFAULT,
            D3D12_RESOURCE_FLAG_RAYTRACING_ACCELERATION_STRUCTURE
                | D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
            D3D12_RESOURCE_STATE_RAYTRACING_ACCELERATION_STRUCTURE,
            name,
        )?;

        // note that the state of scratch buffer is COMMON, and needs to change to UNORDERED_ACCESS
        // before buidling
        self.buffer = Some(buffer);
        self.scratch_buffer = Some(scratch_buffer);

        Ok(())
    }

    fn build(&mut self, command_list: &ID3D12GraphicsCommandList7) {
        let dst = self
            .buffer
            .as_ref()
            .expect("allocate_buffers needs to be called first");

        let scratch_buffer = self
            .scratch_buffer
            .as_ref()
            .expect("allocate_buffers needs to be called first");

        let inputs = self.inputs(BuildMode::FullBuild);

        let desc = D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_DESC {
            Inputs: inputs,
            DestAccelerationStructureData: unsafe { dst.GetGPUVirtualAddress() },
            ScratchAccelerationStructureData: unsafe { scratch_buffer.GetGPUVirtualAddress() },
            SourceAccelerationStructureData: 0,
        };

        unsafe { command_list.BuildRaytracingAccelerationStructure(&desc, None) }
    }

    fn update(&mut self, command_list: &ID3D12GraphicsCommandList7) {
        let dst = self
            .buffer
            .as_ref()
            .expect("allocate_buffers needs to be called first");

        let scratch_buffer = self
            .scratch_buffer
            .as_ref()
            .expect("allocate_buffers needs to be called first");

        let inputs = self.inputs(BuildMode::Update);

        let desc = D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_DESC {
            Inputs: inputs,
            DestAccelerationStructureData: unsafe { dst.GetGPUVirtualAddress() },
            ScratchAccelerationStructureData: unsafe { scratch_buffer.GetGPUVirtualAddress() },
            SourceAccelerationStructureData: unsafe { dst.GetGPUVirtualAddress() },
        };

        unsafe { command_list.BuildRaytracingAccelerationStructure(&desc, None) }
    }

    fn inputs(&self, mode: BuildMode) -> D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS {
        let mut build_flags = self.build_flags;

        if mode == BuildMode::Update {
            assert_ne!(
                self.build_flags & D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_ALLOW_UPDATE,
                D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_NONE
            );
            build_flags |= D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_PERFORM_UPDATE;
        }

        D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS {
            Type: D3D12_RAYTRACING_ACCELERATION_STRUCTURE_TYPE_BOTTOM_LEVEL,
            Flags: build_flags,
            NumDescs: self.geometries.len() as u32,
            DescsLayout: D3D12_ELEMENTS_LAYOUT_ARRAY,
            Anonymous: D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS_0 {
                pGeometryDescs: self.geometries.as_ptr(),
            },
        }
    }
}

struct Tlas {
    id: TlasId,

    buffer: Option<ID3D12Resource>,
    scratch_buffer: Option<ID3D12Resource>,

    build_flags: D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAGS,

    instances: Vec<D3D12_RAYTRACING_INSTANCE_DESC>,
    instance_buffer: Option<ID3D12Resource>,
}

impl Tlas {
    fn new(id: TlasId, build_flags: D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAGS) -> Self {
        Tlas {
            instances: Vec::new(),
            instance_buffer: None,

            buffer: None,
            scratch_buffer: None,

            build_flags,

            id,
        }
    }

    fn add_instance(&mut self, blas: &Blas, transform: Option<math::Affine3A>) {
        let transform = math::mat4_to_row_marjor_float3x4(
            &transform.unwrap_or(math::Affine3A::IDENTITY).into(),
        );

        let instance_mask = 0xFF; // 8 bits
        assert!(instance_mask <= u8::MAX.into());

        const U24_MAX: u32 = 0xFF_FFFF;
        let instance_id = blas.id.v as u32; // 24 bits
        assert!(instance_id <= U24_MAX);

        let instance_flags = D3D12_RAYTRACING_INSTANCE_FLAG_NONE; // 8 bits

        let contribution_to_hit_group_index = 0;
        assert!(contribution_to_hit_group_index <= U24_MAX);

        let blas = blas.buffer.as_ref().unwrap();

        let instance = D3D12_RAYTRACING_INSTANCE_DESC {
            Transform: transform,
            AccelerationStructure: unsafe { blas.GetGPUVirtualAddress() },
            _bitfield1: (instance_mask << 24) | instance_id,
            _bitfield2: ((instance_flags.0 as u32) << 24) | contribution_to_hit_group_index,
        };

        self.instances.push(instance);
    }

    fn init_instance_buffer(&mut self, device: &Device, name: &str) -> windows::core::Result<()> {
        let instance_buffer = resource::create_buffer_with_data(
            device,
            D3D12_HEAP_TYPE_UPLOAD,
            D3D12_RESOURCE_FLAG_NONE,
            D3D12_RESOURCE_STATE_GENERIC_READ,
            &self.instances,
            name,
        )?;

        self.instance_buffer = Some(instance_buffer);

        Ok(())
    }

    #[must_use]
    fn allocate_buffers(&mut self, device: &Device, name: &str) -> bool {
        let inputs = self.inputs();

        let mut info = Default::default();
        unsafe {
            device
                .get()
                .GetRaytracingAccelerationStructurePrebuildInfo(&inputs, &mut info)
        };

        let scratch_buffer = self
            .scratch_buffer
            .take_if(|buf| unsafe { buf.GetDesc() }.Width >= info.ScratchDataSizeInBytes)
            .unwrap_or_else(|| {
                let scratch_buffer_name = "Scratch buffer for ".to_string() + name;
                resource::create_buffer(
                    device,
                    info.ScratchDataSizeInBytes,
                    D3D12_HEAP_TYPE_DEFAULT,
                    D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
                    D3D12_RESOURCE_STATE_COMMON,
                    &scratch_buffer_name,
                )
                .unwrap()
            });
        self.scratch_buffer = Some(scratch_buffer);

        let prev_address = self
            .buffer
            .as_ref()
            .map_or(0, |buf| unsafe { buf.GetGPUVirtualAddress() });

        let buffer = self
            .buffer
            .take_if(|buf| unsafe { buf.GetDesc() }.Width >= info.ResultDataMaxSizeInBytes)
            .unwrap_or_else(|| {
                resource::create_buffer(
                    device,
                    info.ResultDataMaxSizeInBytes,
                    D3D12_HEAP_TYPE_DEFAULT,
                    D3D12_RESOURCE_FLAG_RAYTRACING_ACCELERATION_STRUCTURE
                        | D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
                    D3D12_RESOURCE_STATE_RAYTRACING_ACCELERATION_STRUCTURE,
                    name,
                )
                .unwrap()
            });
        self.buffer = Some(buffer);

        prev_address != unsafe { self.buffer.as_ref().unwrap().GetGPUVirtualAddress() }
    }

    fn build(&mut self, command_list: &ID3D12GraphicsCommandList7) -> windows::core::Result<()> {
        assert!(
            self.instance_buffer.is_some(),
            "init_instance_buffer must be called first."
        );

        let scratch_buffer = self
            .scratch_buffer
            .as_ref()
            .expect("allocate_buffers must be called first");

        let buffer = self
            .buffer
            .as_ref()
            .expect("allocate_buffers must be called first");

        let inputs = self.inputs();

        let desc = D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_DESC {
            Inputs: inputs,
            DestAccelerationStructureData: unsafe { buffer.GetGPUVirtualAddress() },
            ScratchAccelerationStructureData: unsafe { scratch_buffer.GetGPUVirtualAddress() },
            SourceAccelerationStructureData: 0,
        };

        unsafe { command_list.BuildRaytracingAccelerationStructure(&desc, None) }

        Ok(())
    }

    fn inputs(&self) -> D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS {
        let instance_buf_address = self
            .instance_buffer
            .as_ref()
            .map_or(0, |buf| unsafe { buf.GetGPUVirtualAddress() });

        D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS {
            Type: D3D12_RAYTRACING_ACCELERATION_STRUCTURE_TYPE_TOP_LEVEL,
            Flags: self.build_flags,
            NumDescs: self.instances.len() as u32,
            DescsLayout: D3D12_ELEMENTS_LAYOUT_ARRAY,
            Anonymous: D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS_0 {
                InstanceDescs: instance_buf_address,
            },
        }
    }
}

impl Default for MeshData {
    fn default() -> Self {
        MeshData {
            index_buffer_handle: u32::MAX,
            color_buffer_handle: u32::MAX,
            _reserved: [u32::MAX; 2],
        }
    }
}
