use std::mem;
use windows::Win32::Graphics::{Direct3D12::*, Dxgi::Common::*};

use super::d3d12::{device::*, raytracing::*, resource, view::*};
use super::light::SpotLight;
use super::mesh::{Material, Mesh, MeshResource};
use super::{math::*, mesh};

pub struct Scene {
    timer: std::time::Instant,

    camera: Camera,
    camera_buffer: ID3D12Resource,
    camera_cbv: Cbv,

    meshes: Vec<Mesh>,

    light: SpotLight,

    raytracing_scene: RaytracingScene,

    transform_buffer: ID3D12Resource,
    transform_srv: Srv,

    #[allow(unused)]
    material_buffer: ID3D12Resource,
    material_srv: Srv,
}

impl Scene {
    pub fn build(
        device: &mut Device,
        viewport_width: u32,
        viewport_height: u32,
    ) -> windows::core::Result<Self> {
        let mesh_resources = mesh::obj::load("assets/bunny.obj").expect("Cannot find the obj file");
        let mesh_material = Material {
            base_color: Vec3::new(1.0, 0.97, 0.73),
            metallic: 0.75,
            specular_reflectance: Vec3::new(0.95, 0.73, 0.37),
            roughness: 0.5,
            specular_tint: Vec3::new(1.0, 0.97, 0.73),
            pad: Default::default(),
        };
        let meshes: windows::core::Result<Vec<Mesh>> = mesh_resources
            .iter()
            .map(|mesh| Mesh::load(device, mesh, mesh_material.clone(), Box::new(rotate)))
            .collect();
        let mut meshes = meshes?;

        let plane = MeshResource::new(
            &PLANE_INDICES,
            &PLANE_VERTEX_POSITIONS,
            &PLANE_VERTEX_NORMALS,
            "plane".to_string(),
        );
        let mesh_material = Material {
            base_color: Vec3::new(0.75, 0.75, 0.75),
            metallic: 0.0,
            specular_reflectance: Vec3::ZERO,
            roughness: 1.0,
            specular_tint: Vec3::ZERO,
            pad: Default::default(),
        };
        meshes.push(Mesh::load(device, &plane, mesh_material, Box::new(stay))?);

        let transforms: Vec<_> = meshes
            .iter()
            .flat_map(|mesh| [mesh.transform(), mesh.transposed_inv_transform()])
            .cloned()
            .collect();

        let transform_buffer = resource::create_buffer_with_data(
            device,
            D3D12_HEAP_TYPE_UPLOAD,
            D3D12_RESOURCE_FLAG_NONE,
            D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE,
            &transforms,
            "Scene::transform_buffer",
        )?;

        let transform_srv_desc = D3D12_SHADER_RESOURCE_VIEW_DESC {
            Format: DXGI_FORMAT_UNKNOWN,
            ViewDimension: D3D12_SRV_DIMENSION_BUFFER,
            Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
            Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                Buffer: D3D12_BUFFER_SRV {
                    FirstElement: 0,
                    NumElements: transforms.len() as u32,
                    StructureByteStride: std::mem::size_of_val(&transforms[0]) as u32,
                    Flags: D3D12_BUFFER_SRV_FLAG_NONE,
                },
            },
        };
        let transform_srv = device.create_srv(Some(&transform_buffer), Some(&transform_srv_desc));

        let materials: Vec<_> = meshes.iter().map(|mesh| mesh.material()).cloned().collect();
        let material_buffer = resource::create_buffer_with_data(
            device,
            D3D12_HEAP_TYPE_UPLOAD,
            D3D12_RESOURCE_FLAG_NONE,
            D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE,
            &materials,
            "Scene::material_buffer",
        )?;

        let material_srv_desc = D3D12_SHADER_RESOURCE_VIEW_DESC {
            Format: DXGI_FORMAT_UNKNOWN,
            ViewDimension: D3D12_SRV_DIMENSION_BUFFER,
            Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
            Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                Buffer: D3D12_BUFFER_SRV {
                    FirstElement: 0,
                    NumElements: materials.len() as u32,
                    StructureByteStride: std::mem::size_of::<Material>() as u32,
                    Flags: D3D12_BUFFER_SRV_FLAG_NONE,
                },
            },
        };
        let material_srv = device.create_srv(Some(&material_buffer), Some(&material_srv_desc));

        let camera = Camera {
            viewport_size: [viewport_width, viewport_height],
            ..Default::default()
        };

        let camera_buf_size = align!(std::mem::size_of_val(&camera), 256);
        let camera_buffer = resource::create_buffer(
            device,
            camera_buf_size as u64,
            D3D12_HEAP_TYPE_UPLOAD,
            D3D12_RESOURCE_FLAG_NONE,
            D3D12_RESOURCE_STATE_COMMON,
            "Scene::camera",
        )?;

        let camera_cbv_desc = D3D12_CONSTANT_BUFFER_VIEW_DESC {
            BufferLocation: unsafe { camera_buffer.GetGPUVirtualAddress() },
            SizeInBytes: camera_buf_size as u32,
        };
        let camera_cbv = device.create_cbv(Some(&camera_cbv_desc));

        let mut raytracing_scene = RaytracingScene::new(
            D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_PREFER_FAST_TRACE,
            "Scene::raytracing_scene".into(),
        );

        let blas_flags = D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_PREFER_FAST_TRACE
            | D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_ALLOW_UPDATE;

        let blas_id = raytracing_scene.add_blas(blas_flags);
        let transform_address = unsafe { transform_buffer.GetGPUVirtualAddress() };

        for (i, mesh) in meshes.iter().enumerate() {
            // multiplied by 2 because odd number indices are for invertransposed inverse matrices
            let transform = transform_address + (2 * mem::size_of::<[f32; 12]>() * i) as u64;
            raytracing_scene.add_mesh(blas_id, mesh, Some(transform));
        }

        let light_position = Vec3::new(3.0, 10.0, -3.0);
        let light_dir = (vec3(0.0, 0.0, 0.0) - light_position).normalize();
        let light_angle = 60.0_f32.to_radians();
        let light = SpotLight::new(light_position, 500.0, light_dir, light_angle);

        raytracing_scene.build(device)?;

        Ok(Scene {
            timer: std::time::Instant::now(),
            camera,
            camera_buffer,
            camera_cbv,

            meshes,
            light,

            raytracing_scene,

            transform_buffer,
            transform_srv,

            material_buffer,
            material_srv,
        })
    }

    pub fn update(&mut self) {
        let total_time = self.timer.elapsed().as_secs_f64();

        for mesh in &mut self.meshes {
            mesh.update(total_time);
        }

        let eye = Vec3::new(0.0, 5.0, -10.0);
        let center: Vec3 = Vec3::ZERO;
        let up = Vec3::new(0.0, 1.0, 0.0);
        let view = Mat4::look_at_lh(eye, center, up);

        let fov = 20.0_f32.to_radians();
        let viewport_size = &self.camera.viewport_size;
        let aspect_ratio = (viewport_size[0] as f32) / (viewport_size[1] as f32);

        let projection = Mat4::perspective_lh(fov, aspect_ratio, 0.1, 100.0);

        let view_proj = projection * view;

        self.camera.position = eye;
        self.camera.view_proj = view_proj;
        self.camera.inv_view_proj = view_proj.inverse();
    }

    pub fn update_buffers(
        &mut self,
        device: &mut Device,
        cmd_list: &ID3D12GraphicsCommandList7,
    ) -> windows::core::Result<()> {
        self.update_camera_buffer()?;
        self.update_transform()?;
        self.raytracing_scene.update(device, cmd_list)?;

        Ok(())
    }

    fn update_camera_buffer(&self) -> windows::core::Result<()> {
        let mut data = std::ptr::null_mut();
        unsafe {
            self.camera_buffer.Map(0, None, Some(&mut data))?;
            std::ptr::copy_nonoverlapping(&self.camera, data as *mut _, 1);
            self.camera_buffer.Unmap(0, None);
        }

        Ok(())
    }

    pub fn update_transform(&self) -> windows::core::Result<()> {
        let transforms: Vec<[f32; 12]> = self
            .meshes
            .iter()
            .flat_map(|mesh| [mesh.transform(), mesh.transposed_inv_transform()])
            .cloned()
            .collect();

        let mut data = std::ptr::null_mut();
        unsafe {
            self.transform_buffer.Map(0, None, Some(&mut data))?;
            std::ptr::copy_nonoverlapping(transforms.as_ptr(), data as *mut _, transforms.len());
            self.transform_buffer.Unmap(0, None);
        }

        Ok(())
    }

    pub fn light(&self) -> &SpotLight {
        &self.light
    }

    pub fn camera_cbv(&self) -> &Cbv {
        &self.camera_cbv
    }

    pub fn transform_srv(&self) -> &Srv {
        &self.transform_srv
    }

    pub fn raytracing_scene(&self) -> &RaytracingScene {
        &self.raytracing_scene
    }

    pub fn meshes(&self) -> &[Mesh] {
        &self.meshes
    }

    pub fn material_srv(&self) -> &Srv {
        &self.material_srv
    }
}

#[derive(Debug, Default)]
#[repr(C, align(16))]
pub struct Camera {
    view_proj: Mat4,

    inv_view_proj: Mat4,

    position: Vec3,
    pad: u32,

    viewport_size: [u32; 2],
}

#[rustfmt::skip]
const PLANE_VERTEX_POSITIONS: [Vec3; 4] = [
    Vec3{x: -10.0, y: -2.0, z: -10.0},
    Vec3{x:  10.0, y: -2.0, z: -10.0},
    Vec3{x: -10.0, y: -2.0, z:  10.0},
    Vec3{x:  10.0, y: -2.0, z:  10.0},
];

#[rustfmt::skip]
const PLANE_VERTEX_NORMALS: [Vec3; 4] = [
    Vec3{x: 0.0, y: 1.0, z:  0.0},
    Vec3{x: 0.0, y: 1.0, z:  0.0},
    Vec3{x: 0.0, y: 1.0, z:  0.0},
    Vec3{x: 0.0, y: 1.0, z:  0.0},
];

#[rustfmt::skip]
const PLANE_INDICES: [u32; 6] = [
    0, 2, 3, 0, 3, 1
];

fn rotate(time: f64) -> Mat4 {
    let angle_deg = (time * 90.0) % 360.0;
    let angle = angle_deg.to_radians();
    Mat4::from_rotation_y(angle as f32)
}

fn stay(_time: f64) -> Mat4 {
    Mat4::IDENTITY
}
