use super::math::*;

#[derive(Debug, Clone)]
#[repr(C)]
pub struct LightParameters {
    pub position: Vec3,
    pub intensity: f32,

    pub direction: Vec3,
    pub cos_half_angle: f32,
}

#[derive(Debug)]
pub struct SpotLight {
    pub position: Vec3,
    pub intensity: f32,

    pub direction: Vec3,
    pub angle_rad: f32,
}

impl SpotLight {
    pub fn new(position: Vec3, intensity: f32, direction: Vec3, angle_rad: f32) -> Self {
        Self {
            position,
            intensity,
            direction,
            angle_rad,
        }
    }

    pub fn create_parameters(&self) -> LightParameters {
        LightParameters {
            position: self.position,
            intensity: self.intensity,

            direction: self.direction,
            cos_half_angle: f32::cos(self.angle_rad * 0.5),
        }
    }

    pub fn view_matrix(&self) -> Mat4 {
        let (up, _) = orthonormal_basis(&self.direction);
        Mat4::look_to_lh(self.position, self.direction, up)
    }

    pub fn view_projection(&self, aspect_ratio: f32) -> Mat4 {
        let projection = Mat4::perspective_lh(self.angle_rad, aspect_ratio, 0.1, 100.0);
        projection * self.view_matrix()
    }
}
