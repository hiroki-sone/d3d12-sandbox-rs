use windows::Win32::Graphics::{Direct3D12::*, Dxgi::Common::*};

use super::d3d12::{device::*, raytracing::*, resource, view::*};
use super::math::*;
use super::mesh::Mesh;

pub struct Scene {
    timer: std::time::Instant,

    camera: Camera,
    camera_buffer: ID3D12Resource,
    camera_cbv: Cbv,

    mesh: Mesh,

    raytracing_scene: RaytracingScene,

    model_transform: [f32; 12],
    transform_buffer: ID3D12Resource,
    transform_srv: Srv,
}

impl Scene {
    pub fn build(
        device: &mut Device,
        viewport_width: u32,
        viewport_height: u32,
    ) -> windows::core::Result<Self> {
        let model_transform = mat4_to_row_marjor_float3x4(&Mat4::IDENTITY);
        let transform_buffer = resource::create_buffer_with_data(
            device,
            D3D12_HEAP_TYPE_UPLOAD,
            D3D12_RESOURCE_FLAG_NONE,
            D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE,
            &[model_transform],
            "Scene::transform_buffer",
        )?;

        let transform_srv_desc = D3D12_SHADER_RESOURCE_VIEW_DESC {
            Format: DXGI_FORMAT_UNKNOWN,
            ViewDimension: D3D12_SRV_DIMENSION_BUFFER,
            Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
            Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                Buffer: D3D12_BUFFER_SRV {
                    FirstElement: 0,
                    NumElements: 1,
                    StructureByteStride: std::mem::size_of_val(&model_transform) as u32,
                    Flags: D3D12_BUFFER_SRV_FLAG_NONE,
                },
            },
        };
        let transform_srv = device.create_srv(Some(&transform_buffer), Some(&transform_srv_desc));

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

        let mesh = Mesh::load(device)?;

        let mut raytracing_scene = RaytracingScene::new(
            D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_PREFER_FAST_TRACE,
            "Scene::raytracing_scene".into(),
        );

        let blas_flags = D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_PREFER_FAST_TRACE
            | D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_ALLOW_UPDATE;

        let blas_id = raytracing_scene.add_blas(blas_flags);
        let transform_address = unsafe { transform_buffer.GetGPUVirtualAddress() };

        raytracing_scene.add_mesh(blas_id, &mesh, Some(transform_address));

        raytracing_scene.build(device)?;

        Ok(Scene {
            timer: std::time::Instant::now(),
            camera,
            camera_buffer,
            camera_cbv,

            mesh,
            raytracing_scene,

            model_transform,
            transform_buffer,
            transform_srv,
        })
    }

    pub fn update(&mut self) {
        let total_time = self.timer.elapsed().as_secs_f64();

        let angle_deg = (total_time * 90.0) % 360.0;
        let angle = angle_deg.to_radians();
        let rotation_axis = Vec3::new(0.0, 1.0, 1.0).normalize();
        let model = Mat4::from_axis_angle(rotation_axis, angle as f32);

        self.model_transform = mat4_to_row_marjor_float3x4(&model);

        let eye = Vec3::new(0.0, 0.0, -10.0);
        let center: Vec3 = Vec3::ZERO;
        let up = Vec3::new(0.0, 1.0, 0.0);
        // let view = math::Mat4::look_at_rh(eye, center, up);
        let view = Mat4::look_at_lh(eye, center, up);

        let fov = 45.0 * core::f32::consts::PI / 180.0;
        let viewport_size = &self.camera.viewport_size;
        let aspect_ratio = (viewport_size[0] as f32) / (viewport_size[1] as f32);

        // let projection = math::Mat4::perspective_rh(fov, aspect_ratio, 0.1, 100.0);
        let projection = Mat4::perspective_lh(fov, aspect_ratio, 0.1, 100.0);

        let view_proj = projection * view;

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
        let mut data = std::ptr::null_mut();

        unsafe {
            self.transform_buffer.Map(0, None, Some(&mut data))?;
            std::ptr::copy_nonoverlapping(&self.model_transform, data as *mut _, 1);
            self.transform_buffer.Unmap(0, None);
        }

        Ok(())
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

    pub fn mesh(&self) -> &Mesh {
        &self.mesh
    }
}

#[derive(Debug, Default)]
#[repr(C, align(16))]
pub struct Camera {
    view_proj: Mat4,

    inv_view_proj: Mat4,

    viewport_size: [u32; 2],
}
