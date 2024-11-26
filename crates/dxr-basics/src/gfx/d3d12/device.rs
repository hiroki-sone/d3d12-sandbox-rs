use windows::core::Interface;
use windows::Win32::Foundation::{self, E_FAIL, HWND};
use windows::Win32::Graphics::{
    Direct3D::*,
    Direct3D12::*,
    Dxgi::{Common::*, *},
};

use super::command::Context;
use super::view::*;
use super::{
    command::{self, FenceValue},
    util,
};
pub struct Device {
    // D3D12 Device: considered as a memory context that tracks allocations in GPU memory
    device: ID3D12Device5,

    gfx_queue: command::Queue,
    copy_queue: command::Queue,

    swap_chain: IDXGISwapChain4,
    frame_buffers: [ID3D12Resource; FRAME_BUFFER_COUNT],
    frame_buffer_rtvs: [Rtv; FRAME_BUFFER_COUNT],

    back_buffer_index: usize,
    frame_fences: [FenceValue; FRAME_BUFFER_COUNT],

    view_heap: CbvSrvUavHeap,
    rtv_heap: RtvHeap,
    dsv_heap: DsvHeap,

    // controls whether the swap chain's present method should wait for the next vertical fresh before presenting the rendered image
    vsync_enabled: bool,
    tearing_supported: bool,
}

impl Device {
    pub fn build(hwnd: HWND, screen_width: u32, screen_height: u32) -> windows::core::Result<Self> {
        let factory = create_factory(true, true)?;

        let device = create_device(&factory)?;

        let gfx_queue = command::Queue::build(
            &device,
            D3D12_COMMAND_LIST_TYPE_DIRECT,
            "Device::gfx_queue".into(),
        )
        .unwrap();

        let swap_chain = create_swap_chain(
            &factory,
            gfx_queue.get(),
            hwnd,
            screen_width,
            screen_height,
            FRAME_BUFFER_COUNT as u32,
        )
        .unwrap();

        let frame_buffers = create_frame_buffers_from_swap_chain(&swap_chain);

        let back_buffer_index = unsafe { swap_chain.GetCurrentBackBufferIndex() } as usize;

        //TODO make the capacity configurable
        let view_heap = CbvSrvUavHeap::build(&device, 100, "Device::view_heap")?;

        let mut rtv_heap =
            RtvHeap::build(&device, FRAME_BUFFER_COUNT as u32 + 1, "Device::rtv_heap")?;

        let frame_buffer_rtvs = frame_buffers
            .each_ref()
            .map(|buf| rtv_heap.create_rtv(&device, buf));

        let dsv_heap = DsvHeap::build(&device, 1, "Device::dsv_heap")?;

        let copy_queue = command::Queue::build(
            &device,
            D3D12_COMMAND_LIST_TYPE_COPY,
            "Device::copy_queue".into(),
        )
        .unwrap();

        let tearing_supported = check_tearing_support(&factory);

        let inline_raytracing_supported = check_inline_raytracing_support(&device);
        if !inline_raytracing_supported {
            return Err(windows::core::Error::new(
                E_FAIL,
                "This device does not support the Inline Ray Tracing feature",
            ));
        }

        if !check_bindless_support(&device) {
            return Err(windows::core::Error::new(
                E_FAIL,
                "This device does not support the Bindless Resource feature",
            ));
        }

        Ok(Self {
            device,

            gfx_queue,
            copy_queue,

            swap_chain,
            frame_buffers,
            frame_buffer_rtvs,

            back_buffer_index,

            frame_fences: Default::default(),

            view_heap,
            rtv_heap,
            dsv_heap,

            tearing_supported,
            vsync_enabled: true,
        })
    }

    pub fn present_frame(&mut self, ctx: Context) -> windows::core::Result<()> {
        self.frame_fences[self.back_buffer_index] = self.gfx_queue.execute_commands(ctx)?;

        let sync_interval = if self.vsync_enabled { 1 } else { 0 };
        let present_flags = if self.tearing_supported && !self.vsync_enabled {
            DXGI_PRESENT_ALLOW_TEARING
        } else {
            DXGI_PRESENT(0)
        };

        let present_result = unsafe { self.swap_chain.Present(sync_interval, present_flags) };
        if present_result.is_err() {
            return Err(present_result.into());
        }

        // insert fence for the current frame
        self.frame_fences[self.back_buffer_index] = self.gfx_queue.signal();

        // wait for the previous frame
        let i = unsafe { self.swap_chain.GetCurrentBackBufferIndex() } as usize;
        self.gfx_queue.wait_fence(self.frame_fences[i]);

        self.back_buffer_index = i;

        Ok(())
    }

    pub fn request_gfx_command_ctx(&mut self) -> windows::core::Result<Context> {
        self.gfx_queue.request_command_ctx()
    }

    pub fn request_copy_command_ctx(&mut self) -> windows::core::Result<Context> {
        self.copy_queue.request_command_ctx()
    }

    pub fn get(&self) -> &ID3D12Device5 {
        &self.device
    }

    pub fn gfx_queue(&self) -> &command::Queue {
        &self.gfx_queue
    }

    pub fn gfx_queue_mut(&mut self) -> &mut command::Queue {
        &mut self.gfx_queue
    }

    pub fn copy_queue(&self) -> &command::Queue {
        &self.copy_queue
    }

    pub fn copy_queue_mut(&mut self) -> &mut command::Queue {
        &mut self.copy_queue
    }

    pub fn view_heap(&self) -> &ID3D12DescriptorHeap {
        self.view_heap.get()
    }

    pub fn create_cbv(&mut self, desc: Option<*const D3D12_CONSTANT_BUFFER_VIEW_DESC>) -> Cbv {
        self.view_heap.create_cbv(&self.device, desc)
    }

    pub fn create_srv(
        &mut self,
        resource: Option<&ID3D12Resource>,
        desc: Option<*const D3D12_SHADER_RESOURCE_VIEW_DESC>,
    ) -> Srv {
        self.view_heap.create_srv(&self.device, resource, desc)
    }

    pub fn create_uav(
        &mut self,
        resource: &ID3D12Resource,
        desc: Option<*const D3D12_UNORDERED_ACCESS_VIEW_DESC>,
    ) -> Uav {
        self.view_heap.create_uav(&self.device, resource, desc)
    }

    pub fn create_rtv(&mut self, resource: &ID3D12Resource) -> Rtv {
        self.rtv_heap.create_rtv(&self.device, resource)
    }

    pub fn create_dsv(
        &mut self,
        resource: &ID3D12Resource,
        desc: Option<*const D3D12_DEPTH_STENCIL_VIEW_DESC>,
    ) -> Dsv {
        self.dsv_heap.create_dsv(&self.device, resource, desc)
    }

    pub fn frame_buffers(&self) -> [&ID3D12Resource; FRAME_BUFFER_COUNT] {
        self.frame_buffers.each_ref()
    }

    pub fn back_buffer_index(&self) -> usize {
        self.back_buffer_index
    }

    pub fn back_buffer(&self) -> &ID3D12Resource {
        &self.frame_buffers[self.back_buffer_index]
    }

    pub fn back_buffer_rtv(&self) -> &Rtv {
        &self.frame_buffer_rtvs[self.back_buffer_index]
    }

    pub fn is_tearing_supported(&self) -> bool {
        self.tearing_supported
    }
}

pub const FRAME_BUFFER_COUNT: usize = 3;

pub const FRAME_BUFFER_FORMAT: DXGI_FORMAT = DXGI_FORMAT_R8G8B8A8_UNORM;

pub fn report_live_objects() -> windows::core::Result<()> {
    unsafe {
        let debug: IDXGIDebug1 = DXGIGetDebugInterface1(0)?;
        debug.ReportLiveObjects(
            DXGI_DEBUG_ALL,
            DXGI_DEBUG_RLO_DETAIL | DXGI_DEBUG_RLO_IGNORE_INTERNAL,
        )
    }
}

fn create_factory(
    enable_debug_layer: bool,
    enable_gpu_based_validation: bool,
) -> windows::core::Result<IDXGIFactory6> {
    let enable_debug_layer = enable_debug_layer || enable_gpu_based_validation;
    if enable_debug_layer {
        let mut debug: Option<ID3D12Debug1> = None;
        if let Err(e) = unsafe { D3D12GetDebugInterface(&mut debug) } {
            eprintln!("Failed to enable debug layer: {e}");
        } else {
            let debug = debug.unwrap();
            unsafe {
                debug.EnableDebugLayer();
                debug.SetEnableGPUBasedValidation(enable_gpu_based_validation);
            }
        }
    }

    let flags = if enable_debug_layer {
        DXGI_CREATE_FACTORY_DEBUG
    } else {
        DXGI_CREATE_FACTORY_FLAGS(0)
    };

    unsafe { CreateDXGIFactory2(flags) }
}

fn create_device(factory: &IDXGIFactory6) -> windows::core::Result<ID3D12Device5> {
    let mut index = 0;
    let mut device: Option<ID3D12Device5> = None;
    let mut adapter_desc = DXGI_ADAPTER_DESC1::default();

    while let Ok(adapter) = unsafe {
        factory.EnumAdapterByGpuPreference::<IDXGIAdapter1>(
            index,
            DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE,
        )
    } {
        adapter_desc = unsafe { adapter.GetDesc1() }?;
        let adapter_flag = DXGI_ADAPTER_FLAG(adapter_desc.Flags as i32);
        if (adapter_flag & DXGI_ADAPTER_FLAG_SOFTWARE) != DXGI_ADAPTER_FLAG_NONE {
            // reject WARP
            continue;
        }

        if unsafe { D3D12CreateDevice(&adapter, D3D_FEATURE_LEVEL_11_0, &mut device) }.is_ok() {
            break;
        } else {
            device = None;
        }

        index += 1;
    }

    let Some(device) = device else {
        return Err(windows::core::Error::new(
            windows::Win32::Foundation::E_FAIL,
            "Failed to create a device: the GPU does not support required features.",
        ));
    };

    let name = windows::core::PCWSTR(adapter_desc.Description.as_ptr());
    util::set_name(&device, name)?;

    let info_queue = device.cast::<ID3D12InfoQueue1>()?;
    let mut _callback_cookie = 0;
    unsafe {
        info_queue.SetBreakOnSeverity(D3D12_MESSAGE_SEVERITY_CORRUPTION, true)?;
        info_queue.SetBreakOnSeverity(D3D12_MESSAGE_SEVERITY_ERROR, true)?;
        info_queue.SetBreakOnSeverity(D3D12_MESSAGE_SEVERITY_WARNING, true)?;

        let mut denied_severities = [D3D12_MESSAGE_SEVERITY_INFO];
        let mut _denied_messages = [
            // issued when the initial state of a resource is not D3D12_RESOURCE_STATE_COMMON
            D3D12_MESSAGE_ID_CREATERESOURCE_STATE_IGNORED,
        ];

        let deny_list = D3D12_INFO_QUEUE_FILTER_DESC {
            NumSeverities: denied_severities.len() as u32,
            pSeverityList: denied_severities.as_mut_ptr(),
            // NumIDs: denied_messages.len() as u32,
            // pIDList: denied_messages.as_mut_ptr(),
            ..Default::default()
        };
        let filter = D3D12_INFO_QUEUE_FILTER {
            DenyList: deny_list,
            ..Default::default()
        };
        info_queue.PushStorageFilter(&filter)?;

        // set a callback to capture D3D debug messages as LLDB does not support Windows debug output
        // https://github.com/microsoft/windows-rs/issues/3031
        info_queue.RegisterMessageCallback(
            Some(capture_message),
            D3D12_MESSAGE_CALLBACK_FLAG_NONE,
            std::ptr::null_mut(),
            &mut _callback_cookie,
        )?;
    }

    Ok(device)
}

fn create_swap_chain(
    factory: &IDXGIFactory6,
    command_queue: &ID3D12CommandQueue,
    hwnd: HWND,
    width: u32,
    height: u32,
    count: u32,
) -> windows::core::Result<IDXGISwapChain4> {
    let flags = if check_tearing_support(factory) {
        DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING
    } else {
        DXGI_SWAP_CHAIN_FLAG(0)
    };

    let desc = DXGI_SWAP_CHAIN_DESC1 {
        Width: width,
        Height: height,
        Format: FRAME_BUFFER_FORMAT,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Stereo: false.into(),
        BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
        BufferCount: count,
        // behavior when resizing window
        Scaling: DXGI_SCALING_STRETCH,
        SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
        AlphaMode: DXGI_ALPHA_MODE_UNSPECIFIED,
        Flags: flags.0 as u32,
    };

    let swap_chain =
        unsafe { factory.CreateSwapChainForHwnd(command_queue, hwnd, &desc, None, None) }?;

    // Disable Alt+Enter fullscreen toggle
    unsafe { factory.MakeWindowAssociation(hwnd, DXGI_MWA_NO_ALT_ENTER) }?;

    swap_chain.cast::<IDXGISwapChain4>()
}

fn create_frame_buffers_from_swap_chain(
    swap_chain: &IDXGISwapChain4,
) -> [ID3D12Resource; FRAME_BUFFER_COUNT] {
    (0..FRAME_BUFFER_COUNT)
        .map(|i| -> ID3D12Resource {
            let buffer: ID3D12Resource = unsafe { swap_chain.GetBuffer(i as u32) }.unwrap();
            let name = format!("Device::frame_buffer[{i}]");
            util::set_name_str(&buffer, &name)
                .unwrap_or_else(|e| panic!("Failed to name {name}: {e}"));

            buffer
        })
        .collect::<Vec<_>>()
        .try_into()
        .unwrap()
}

fn check_tearing_support(factory: &IDXGIFactory6) -> bool {
    let mut allow_tearing = Foundation::FALSE;

    unsafe {
        factory.CheckFeatureSupport(
            DXGI_FEATURE_PRESENT_ALLOW_TEARING,
            &mut allow_tearing.0 as *mut std::ffi::c_int as *mut std::ffi::c_void,
            core::mem::size_of_val(&allow_tearing) as u32,
        )
    }
    .is_ok_and(|_| allow_tearing.as_bool())
}

fn check_inline_raytracing_support(device: &ID3D12Device5) -> bool {
    let mut options = D3D12_FEATURE_DATA_D3D12_OPTIONS5::default();
    unsafe {
        device.CheckFeatureSupport(
            D3D12_FEATURE_D3D12_OPTIONS5,
            &mut options as *mut _ as _,
            std::mem::size_of_val(&options) as u32,
        )
    }
    .is_ok_and(|_| options.RaytracingTier.0 >= D3D12_RAYTRACING_TIER_1_1.0)
}

fn check_bindless_support(device: &ID3D12Device5) -> bool {
    let mut options = D3D12_FEATURE_DATA_D3D12_OPTIONS::default();
    let resource_binding = unsafe {
        device.CheckFeatureSupport(
            D3D12_FEATURE_D3D12_OPTIONS,
            &mut options as *mut _ as _,
            std::mem::size_of_val(&options) as u32,
        )
    }
    .is_ok_and(|_| options.ResourceBindingTier.0 >= D3D12_RESOURCE_BINDING_TIER_3.0);

    let mut shader_model_data = D3D12_FEATURE_DATA_SHADER_MODEL {
        HighestShaderModel: D3D_SHADER_MODEL_6_6,
    };
    let shader_model = unsafe {
        device.CheckFeatureSupport(
            D3D12_FEATURE_SHADER_MODEL,
            &mut shader_model_data as *mut _ as _,
            std::mem::size_of_val(&shader_model_data) as u32,
        )
    }
    .is_ok_and(|_| shader_model_data.HighestShaderModel.0 >= D3D_SHADER_MODEL_6_6.0);

    resource_binding && shader_model
}

extern "system" fn capture_message(
    _category: D3D12_MESSAGE_CATEGORY,
    severity: D3D12_MESSAGE_SEVERITY,
    id: D3D12_MESSAGE_ID,
    description: windows::core::PCSTR,
    _context: *mut core::ffi::c_void,
) {
    // DO NOT CALL D3D FUNCTIONS IN THIS FUNCTION
    let severity = match severity {
        D3D12_MESSAGE_SEVERITY_CORRUPTION => "CORRUPTION",
        D3D12_MESSAGE_SEVERITY_ERROR => "ERROR",
        D3D12_MESSAGE_SEVERITY_WARNING => "WARNING",
        D3D12_MESSAGE_SEVERITY_INFO => "Info",
        D3D12_MESSAGE_SEVERITY_MESSAGE => "Message",
        _ => panic!("Invalid D3D12_MESSAGE_SEVERITY: {}", severity.0),
    };

    match unsafe { description.to_string() } {
        Ok(msg) => eprintln!("[D3D {severity}] {msg} ({})", id.0),
        Err(e) => eprintln!("A message from D3D is corrupted: {e}"),
    }
}
