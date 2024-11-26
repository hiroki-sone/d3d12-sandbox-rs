pub use glam::*;

macro_rules! align {
    ($value:expr, $alignment:expr) => {
        ($value + $alignment - 1) & (!($alignment - 1))
    };
}

pub(crate) use align;

pub fn divide_and_round_up(x: u32, y: u32) -> u32 {
    (x + y - 1) / y
}

pub fn mat4_to_row_marjor_float3x4(m: &Mat4) -> [f32; 12] {
    // `D3D12_RAYTRACING_INSTANCE_DESC::Transform` is a row-major 3x4 matrix,
    // but unfortunately glam does not provide `to_rows_array`
    let m = m.transpose().to_cols_array();
    let mut float3x4 = [0.0; 12];
    float3x4.copy_from_slice(&m[..12]);
    float3x4
}
