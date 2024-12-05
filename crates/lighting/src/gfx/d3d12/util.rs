use windows::{core::HSTRING, Win32::Graphics::Direct3D12::*};

pub fn set_name(object: &ID3D12Object, name: windows::core::PCWSTR) -> windows::core::Result<()> {
    unsafe { object.SetName(name) }
}

pub fn set_name_str(object: &ID3D12Object, name: &str) -> windows::core::Result<()> {
    // https://github.com/microsoft/windows-rs/issues/973
    let name: HSTRING = name.into();
    set_name(object, windows::core::PCWSTR(name.as_ptr()))
}
