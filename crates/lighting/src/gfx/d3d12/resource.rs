use windows::Win32::Foundation::E_FAIL;
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;

use super::device::Device;
use super::util::set_name_str;

pub fn create_buffer(
    device: &Device,
    size: u64,
    heap_type: D3D12_HEAP_TYPE,
    flags: D3D12_RESOURCE_FLAGS,
    init_state: D3D12_RESOURCE_STATES,
    name: &str,
) -> windows::core::Result<ID3D12Resource> {
    let properties = heap_properties(heap_type);
    let desc = buffer_desc(size, flags);
    let mut buffer: Option<ID3D12Resource> = None;
    unsafe {
        device.get().CreateCommittedResource(
            &properties,
            D3D12_HEAP_FLAG_NONE,
            &desc,
            init_state,
            None,
            &mut buffer,
        )
    }?;

    let buffer: ID3D12Resource = buffer.expect("Failed to create a buffer");

    super::util::set_name_str(&buffer, name)?;

    Ok(buffer)
}

pub fn create_buffer_with_data<T>(
    device: &Device,
    heap_type: D3D12_HEAP_TYPE,
    flags: D3D12_RESOURCE_FLAGS,
    init_state: D3D12_RESOURCE_STATES,
    init_data: &[T],
    name: &str,
) -> windows::core::Result<ID3D12Resource> {
    let size = std::mem::size_of_val(init_data);
    let buffer = create_buffer(device, size as u64, heap_type, flags, init_state, name)?;

    let mut data = std::ptr::null_mut();
    unsafe {
        buffer.Map(0, None, Some(&mut data))?;
        std::ptr::copy_nonoverlapping(init_data.as_ptr(), data as *mut T, init_data.len());
        buffer.Unmap(0, None);
    }

    Ok(buffer)
}

pub fn create_texture2d(
    device: &Device,
    size: (u32, u32),
    format: DXGI_FORMAT,
    resource_flags: D3D12_RESOURCE_FLAGS,
    init_state: D3D12_RESOURCE_STATES,
    clear_value: Option<*const D3D12_CLEAR_VALUE>,
    name: &str,
) -> windows::core::Result<ID3D12Resource> {
    let (width, height) = size;

    if width == 0 || height == 0 {
        return Err(windows::core::Error::new(
            E_FAIL,
            "The width and the height must be grater than zero",
        ));
    }

    let properties = heap_properties(D3D12_HEAP_TYPE_DEFAULT);

    let desc = texture2d_desc(format, width.into(), height, resource_flags);

    let mut texture: Option<ID3D12Resource> = None;
    unsafe {
        device.get().CreateCommittedResource(
            &properties,
            D3D12_HEAP_FLAG_NONE,
            &desc,
            init_state,
            clear_value,
            &mut texture,
        )?;
    };
    let texture = texture.unwrap();

    set_name_str(&texture, name)?;

    Ok(texture)
}

fn heap_properties(heap_type: D3D12_HEAP_TYPE) -> D3D12_HEAP_PROPERTIES {
    D3D12_HEAP_PROPERTIES {
        Type: heap_type,
        CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
        MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
        CreationNodeMask: 1,
        VisibleNodeMask: 1,
    }
}

fn buffer_desc(buffer_size: u64, flags: D3D12_RESOURCE_FLAGS) -> D3D12_RESOURCE_DESC {
    D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
        Alignment: 0,
        Width: buffer_size,
        Height: 1,
        DepthOrArraySize: 1,
        MipLevels: 1,
        Format: DXGI_FORMAT_UNKNOWN,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
        Flags: flags,
    }
}

fn texture2d_desc(
    format: DXGI_FORMAT,
    width: u64,
    height: u32,
    flags: D3D12_RESOURCE_FLAGS,
) -> D3D12_RESOURCE_DESC {
    D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
        Alignment: 0,
        Width: width,
        Height: height,
        DepthOrArraySize: 1,
        MipLevels: 1,
        Format: format,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
        Flags: flags,
    }
}
