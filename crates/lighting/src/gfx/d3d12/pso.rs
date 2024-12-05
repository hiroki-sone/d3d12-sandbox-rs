use windows::Win32::{Foundation::E_FAIL, Graphics::Direct3D12::*};

use super::{device::Device, util::*};

pub fn create_root_signature(
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

pub fn create_gfx_pso(
    device: &Device,
    desc: &D3D12_GRAPHICS_PIPELINE_STATE_DESC,
    name: &str,
) -> windows::core::Result<ID3D12PipelineState> {
    let pso: ID3D12PipelineState = unsafe { device.get().CreateGraphicsPipelineState(desc) }?;
    set_name_str(&pso, name)?;
    Ok(pso)
}
