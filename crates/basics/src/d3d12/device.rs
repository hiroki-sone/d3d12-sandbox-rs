use windows::core::Interface;
use windows::Win32::Foundation::{self, HWND};
use windows::Win32::Graphics::{
    Direct3D::*,
    Direct3D12::*,
    Dxgi::{Common::*, *},
};

use super::command_queue::CommandContext;
use super::{
    command_queue::{CommandQueue, FenceValue},
    util,
};
pub struct Device {
    // D3D12 Device: considered as a memory context that tracks allocations in GPU memory
    device: ID3D12Device2,

    gfx_command_queue: CommandQueue,
    copy_command_queue: CommandQueue,

    swap_chain: IDXGISwapChain4,
    frame_buffers: [ID3D12Resource; FRAME_BUFFER_COUNT],
    back_buffer_index: usize,
    frame_fences: [FenceValue; FRAME_BUFFER_COUNT],

    rtv_heap: ID3D12DescriptorHeap,
    rtv_size: u32,

    dsv_heap: ID3D12DescriptorHeap,

    // controls whether the swap chain's present method should wait for the next vertical fresh before presenting the rendered image
    vsync_enabled: bool,
    tearing_supported: bool,
}

impl Device {
    pub fn build(hwnd: HWND, screen_width: u32, screen_height: u32) -> windows::core::Result<Self> {
        let factory = create_factory(true, true)?;

        let device = create_device(&factory)?;

        let gfx_command_queue =
            CommandQueue::build(&device, D3D12_COMMAND_LIST_TYPE_DIRECT).unwrap();

        let swap_chain = create_swap_chain(
            &factory,
            gfx_command_queue.get(),
            hwnd,
            screen_width,
            screen_height,
            FRAME_BUFFER_COUNT as u32,
        )
        .unwrap();

        let frame_buffers = create_frame_buffers_from_swap_chain(&swap_chain);

        let back_buffer_index = unsafe { swap_chain.GetCurrentBackBufferIndex() } as usize;

        let rtv_heap = create_descriptor_heap(
            &device,
            D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
            FRAME_BUFFER_COUNT as u32,
        )?;
        let rtv_size =
            unsafe { device.GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_RTV) };

        let dsv_heap = create_descriptor_heap(&device, D3D12_DESCRIPTOR_HEAP_TYPE_DSV, 1)?;

        let copy_command_queue =
            CommandQueue::build(&device, D3D12_COMMAND_LIST_TYPE_COPY).unwrap();

        let tearing_supported = check_tearing_support(&factory);

        Ok(Self {
            device,

            gfx_command_queue,
            copy_command_queue,

            swap_chain,
            frame_buffers,
            back_buffer_index,

            frame_fences: Default::default(),

            rtv_heap,
            rtv_size,
            dsv_heap,

            tearing_supported,
            vsync_enabled: true,
        })
    }

    pub fn present_frame(&mut self, ctx: CommandContext) -> windows::core::Result<()> {
        self.frame_fences[self.back_buffer_index] = self.gfx_command_queue.execute_commands(ctx)?;

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
        self.frame_fences[self.back_buffer_index] = self.gfx_command_queue.signal();

        // wait for the previous frame
        let i = unsafe { self.swap_chain.GetCurrentBackBufferIndex() } as usize;
        self.gfx_command_queue.wait_fence(self.frame_fences[i]);

        self.back_buffer_index = i;

        Ok(())
    }

    pub fn request_gfx_command_ctx(&mut self) -> windows::core::Result<CommandContext> {
        self.gfx_command_queue.request_command_ctx()
    }

    pub fn request_copy_command_ctx(&mut self) -> windows::core::Result<CommandContext> {
        self.copy_command_queue.request_command_ctx()
    }

    pub fn get(&self) -> &ID3D12Device2 {
        &self.device
    }

    pub fn gfx_command_queue(&self) -> &CommandQueue {
        &self.gfx_command_queue
    }

    pub fn copy_command_queue(&self) -> &CommandQueue {
        &self.copy_command_queue
    }

    pub fn copy_command_queue_mut(&mut self) -> &mut CommandQueue {
        &mut self.copy_command_queue
    }

    pub fn frame_buffers(&self) -> &[ID3D12Resource; FRAME_BUFFER_COUNT] {
        &self.frame_buffers
    }

    pub fn back_buffer_index(&self) -> usize {
        self.back_buffer_index
    }

    pub fn back_buffer(&self) -> &ID3D12Resource {
        &self.frame_buffers[self.back_buffer_index]
    }

    pub fn rtv_heap(&self) -> &ID3D12DescriptorHeap {
        &self.rtv_heap
    }

    pub fn rtv_size(&self) -> u32 {
        self.rtv_size
    }

    pub fn dsv_heap(&self) -> &ID3D12DescriptorHeap {
        &self.dsv_heap
    }

    pub fn is_tearing_supported(&self) -> bool {
        self.tearing_supported
    }
}

pub const FRAME_BUFFER_COUNT: usize = 3;

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

fn create_device(factory: &IDXGIFactory6) -> windows::core::Result<ID3D12Device2> {
    let mut index = 0;
    let mut device: Option<ID3D12Device2> = None;
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
            D3D12_MESSAGE_ID_CLEARRENDERTARGETVIEW_MISMATCHINGCLEARVALUE,
            // issued when capturing the frame using graphics debugger?
            D3D12_MESSAGE_ID_MAP_INVALID_NULLRANGE,
            D3D12_MESSAGE_ID_UNMAP_INVALID_NULLRANGE,
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
        Format: DXGI_FORMAT_R8G8B8A8_UNORM,
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
            let name = format!("frame_buffer[{i}]");
            util::set_name_str(&buffer, &name)
                .unwrap_or_else(|e| panic!("Failed to name {name}: {e}"));

            buffer
        })
        .collect::<Vec<_>>()
        .try_into()
        .unwrap()
}

fn create_descriptor_heap(
    device: &ID3D12Device2,
    heap_type: D3D12_DESCRIPTOR_HEAP_TYPE,
    max_descriptor_count: u32,
) -> windows::core::Result<ID3D12DescriptorHeap> {
    let is_shader_visible = (heap_type == D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV)
        || (heap_type == D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER);

    let flags = if is_shader_visible {
        D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE
    } else {
        D3D12_DESCRIPTOR_HEAP_FLAG_NONE
    };

    let desc = D3D12_DESCRIPTOR_HEAP_DESC {
        NumDescriptors: max_descriptor_count,
        Type: heap_type,
        Flags: flags,
        ..Default::default()
    };

    unsafe { device.CreateDescriptorHeap(&desc) }
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

extern "system" fn capture_message(
    _category: D3D12_MESSAGE_CATEGORY,
    _severity: D3D12_MESSAGE_SEVERITY,
    _id: D3D12_MESSAGE_ID,
    description: windows::core::PCSTR,
    _context: *mut core::ffi::c_void,
) {
    // DO NOT CALL D3D FUNCTIONS IN THIS FUNCTION
    match unsafe { description.to_string() } {
        Ok(msg) => eprintln!("[D3D] {msg}"),
        Err(e) => eprintln!("A message from D3D is corrupted: {e}"),
    }
}
