use windows::Win32::Graphics::Direct3D12::*;

use super::util::set_name_str;

pub const TYPE_CBV_SRV_UAV: i32 = D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV.0;
pub const TYPE_SAMPLER: i32 = D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER.0;
pub const TYPE_RTV: i32 = D3D12_DESCRIPTOR_HEAP_TYPE_RTV.0;
pub const TYPE_DSV: i32 = D3D12_DESCRIPTOR_HEAP_TYPE_DSV.0;

/// Constant Buffer View
pub struct Cbv {
    handle: u32,
}

impl Cbv {
    pub fn handle(&self) -> u32 {
        self.handle
    }
}

/// Shader Resource View
pub struct Srv {
    handle: u32,
}

impl Srv {
    pub fn handle(&self) -> u32 {
        self.handle
    }
}

/// Unordered Access View
pub struct Uav {
    handle: u32,
}

impl Uav {
    pub fn handle(&self) -> u32 {
        self.handle
    }
}

/// Render Target View
pub struct Rtv {
    cpu_handle: D3D12_CPU_DESCRIPTOR_HANDLE,
}

impl Rtv {
    pub fn cpu_handle(&self) -> D3D12_CPU_DESCRIPTOR_HANDLE {
        self.cpu_handle
    }
}

/// Depth Stencil View
pub struct Dsv {
    cpu_handle: D3D12_CPU_DESCRIPTOR_HANDLE,
}

impl Dsv {
    pub fn cpu_handle(&self) -> D3D12_CPU_DESCRIPTOR_HANDLE {
        self.cpu_handle
    }
}

pub struct DesciptorHeap<const T: i32> {
    heap: ID3D12DescriptorHeap,
    view_size: u32,
    view_count: u32,
    capacity: u32,
}

impl<const T: i32> DesciptorHeap<T> {
    pub fn build(device: &ID3D12Device5, capacity: u32, name: &str) -> windows::core::Result<Self> {
        assert!(capacity > 0);

        let heap_type: D3D12_DESCRIPTOR_HEAP_TYPE = D3D12_DESCRIPTOR_HEAP_TYPE(T);

        let is_shader_visible: bool = (T == TYPE_CBV_SRV_UAV) || (T == TYPE_SAMPLER);

        let flags = if is_shader_visible {
            D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE
        } else {
            D3D12_DESCRIPTOR_HEAP_FLAG_NONE
        };

        let desc = D3D12_DESCRIPTOR_HEAP_DESC {
            NumDescriptors: capacity,
            Type: heap_type,
            Flags: flags,
            ..Default::default()
        };

        let heap: ID3D12DescriptorHeap = unsafe { device.CreateDescriptorHeap(&desc) }?;

        let view_size = unsafe { device.GetDescriptorHandleIncrementSize(heap_type) };

        set_name_str(&heap, name)?;

        Ok(Self {
            heap,
            view_size,
            view_count: 0,
            capacity,
        })
    }

    pub fn get(&self) -> &ID3D12DescriptorHeap {
        &self.heap
    }
}

impl DesciptorHeap<TYPE_CBV_SRV_UAV> {
    pub fn create_cbv(
        &mut self,
        device: &ID3D12Device5,
        desc: Option<*const D3D12_CONSTANT_BUFFER_VIEW_DESC>,
    ) -> Cbv {
        assert!(self.view_count < self.capacity);

        let mut cpu_handle = unsafe { self.heap.GetCPUDescriptorHandleForHeapStart() };
        cpu_handle.ptr += (self.view_size as usize) * (self.view_count as usize);

        unsafe { device.CreateConstantBufferView(desc, cpu_handle) };

        let handle = self.view_count;

        self.view_count += 1;

        Cbv { handle }
    }

    pub fn create_srv(
        &mut self,
        device: &ID3D12Device5,
        resource: Option<&ID3D12Resource>,
        desc: Option<*const D3D12_SHADER_RESOURCE_VIEW_DESC>,
    ) -> Srv {
        assert!(self.view_count < self.capacity);

        let mut cpu_handle = unsafe { self.heap.GetCPUDescriptorHandleForHeapStart() };
        cpu_handle.ptr += (self.view_size as usize) * (self.view_count as usize);

        unsafe { device.CreateShaderResourceView(resource, desc, cpu_handle) };

        let handle = self.view_count;

        self.view_count += 1;

        Srv { handle }
    }

    pub fn create_uav(
        &mut self,
        device: &ID3D12Device5,
        resource: &ID3D12Resource,
        desc: Option<*const D3D12_UNORDERED_ACCESS_VIEW_DESC>,
    ) -> Uav {
        assert!(self.view_count < self.capacity);

        let mut cpu_handle = unsafe { self.heap.GetCPUDescriptorHandleForHeapStart() };
        cpu_handle.ptr += (self.view_size as usize) * (self.view_count as usize);

        unsafe { device.CreateUnorderedAccessView(resource, None, desc, cpu_handle) };

        let handle = self.view_count;

        self.view_count += 1;

        Uav { handle }
    }
}

impl DesciptorHeap<TYPE_RTV> {
    pub fn create_rtv(&mut self, device: &ID3D12Device5, resource: &ID3D12Resource) -> Rtv {
        assert!(self.view_count < self.capacity);

        let mut cpu_handle = unsafe { self.heap.GetCPUDescriptorHandleForHeapStart() };
        cpu_handle.ptr += (self.view_size as usize) * (self.view_count as usize);

        unsafe { device.CreateRenderTargetView(resource, None, cpu_handle) };

        self.view_count += 1;

        Rtv { cpu_handle }
    }
}

impl DesciptorHeap<TYPE_DSV> {
    pub fn create_dsv(
        &mut self,
        device: &ID3D12Device5,
        resource: &ID3D12Resource,
        desc: Option<*const D3D12_DEPTH_STENCIL_VIEW_DESC>,
    ) -> Dsv {
        assert!(self.view_count < self.capacity);

        let mut cpu_handle = unsafe { self.heap.GetCPUDescriptorHandleForHeapStart() };
        cpu_handle.ptr += (self.view_size as usize) * (self.view_count as usize);

        unsafe { device.CreateDepthStencilView(resource, desc, cpu_handle) };

        self.view_count += 1;

        Dsv { cpu_handle }
    }
}

pub type CbvSrvUavHeap = DesciptorHeap<TYPE_CBV_SRV_UAV>;
pub type RtvHeap = DesciptorHeap<TYPE_RTV>;
pub type DsvHeap = DesciptorHeap<TYPE_DSV>;
