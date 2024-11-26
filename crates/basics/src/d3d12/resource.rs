use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;

use super::device::Device;

pub fn heap_properties(heap_type: D3D12_HEAP_TYPE) -> D3D12_HEAP_PROPERTIES {
    D3D12_HEAP_PROPERTIES {
        Type: heap_type,
        CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
        MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
        CreationNodeMask: 1,
        VisibleNodeMask: 1,
    }
}

pub fn buffer_desc(buffer_size: u64, flags: D3D12_RESOURCE_FLAGS) -> D3D12_RESOURCE_DESC {
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

pub fn texture2d_desc(
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

pub fn create_buffer<T>(
    device: &Device,
    size: u64,
    init_data: Option<&[T]>,
    heap_type: D3D12_HEAP_TYPE,
    flags: D3D12_RESOURCE_FLAGS,
    name: &str,
) -> windows::core::Result<ID3D12Resource> {
    //TODO: replace with default heap
    let properties = heap_properties(heap_type);
    let desc = buffer_desc(size, flags);
    let mut dst: Option<ID3D12Resource> = None;
    unsafe {
        device.get().CreateCommittedResource(
            &properties,
            D3D12_HEAP_FLAG_NONE,
            &desc,
            D3D12_RESOURCE_STATE_COMMON,
            None,
            &mut dst,
        )
    }?;
    let dst = dst.expect("Failed to create a buffer");

    if let Some(init_data) = init_data {
        let mut data = std::ptr::null_mut();
        unsafe {
            dst.Map(0, None, Some(&mut data))?;
            std::ptr::copy_nonoverlapping(init_data.as_ptr(), data as *mut T, init_data.len());
            dst.Unmap(0, None);
        }
    }

    super::util::set_name_str(&dst, name)?;

    Ok(dst)
}
