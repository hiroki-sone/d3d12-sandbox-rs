use super::util::*;
use std::collections::VecDeque;
use windows::{
    core::Interface,
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        Graphics::Direct3D12::*,
        System::Threading::{CreateEventA, WaitForSingleObject, INFINITE},
    },
};

pub struct CommandQueue {
    command_list_type: D3D12_COMMAND_LIST_TYPE,
    command_queue: ID3D12CommandQueue,

    // backing memory for recording the GPU commands into a command list
    // cannot be reset or reused until the GPU finishes executing all commands
    allocators: VecDeque<CommandAllocator>,

    // GPU commands are recorded into this
    command_lists: VecDeque<ID3D12GraphicsCommandList7>,

    device: ID3D12Device2, // is it possible to use reference (&ID3D12Device2) here?

    // sync objects
    fence: ID3D12Fence,
    fence_event: HANDLE,
    fence_value: FenceValue,
}

impl CommandQueue {
    pub fn build(
        device: &ID3D12Device2,
        command_list_type: D3D12_COMMAND_LIST_TYPE,
    ) -> windows::core::Result<Self> {
        let desc = D3D12_COMMAND_QUEUE_DESC {
            Type: command_list_type,
            Priority: D3D12_COMMAND_QUEUE_PRIORITY_NORMAL.0,
            Flags: D3D12_COMMAND_QUEUE_FLAG_NONE,
            NodeMask: 0,
        };
        let command_queue: ID3D12CommandQueue = unsafe { device.CreateCommandQueue(&desc) }?;
        set_name(&command_queue, windows::core::w!("command_queue"))?;

        let fence_value = 0;
        let fence = unsafe { device.CreateFence(fence_value, D3D12_FENCE_FLAG_NONE) }?;
        let fence_event = unsafe {
            CreateEventA(
                None,
                false,
                false,
                windows::core::s!("CommandQueue::event_handle"),
            )
        }?;

        Ok(Self {
            command_list_type,
            command_queue,
            allocators: Default::default(),
            command_lists: Default::default(),
            device: device.clone(),
            fence,
            fence_event,
            fence_value: FenceValue { v: fence_value },
        })
    }

    pub fn request_command_ctx(&mut self) -> windows::core::Result<CommandContext> {
        let allocator = self
            .allocators
            .pop_front()
            .and_then(|e| {
                if self.is_fence_completed(e.fence_value) {
                    Some(e.allocator)
                } else {
                    self.allocators.push_front(e);
                    None
                }
            })
            .unwrap_or_else(|| {
                create_command_allocator(&self.device, self.command_list_type, "command_allocator")
                    .unwrap()
            });

        let command_list = self
            .command_lists
            .pop_front()
            .map(|e| {
                unsafe { e.Reset(&allocator, None) }.unwrap();
                e
            })
            .unwrap_or_else(|| {
                create_command_list(
                    &self.device,
                    &allocator,
                    self.command_list_type,
                    None,
                    "command_list",
                )
                .unwrap()
            });

        Ok(CommandContext {
            command_list,
            allocator,
        })
    }

    #[must_use]
    pub fn get(&self) -> &ID3D12CommandQueue {
        &self.command_queue
    }

    pub fn execute_commands(
        &mut self,
        context: CommandContext,
    ) -> windows::core::Result<FenceValue> {
        let command_list = context.command_list;
        unsafe {
            command_list.Close()?;

            let command_lists = [Some(command_list.cast()?)];
            self.command_queue.ExecuteCommandLists(&command_lists);
        }
        self.command_lists.push_back(command_list);

        let fence_value = self.signal();

        self.allocators.push_back(CommandAllocator {
            allocator: context.allocator,
            fence_value,
        });

        Ok(fence_value)
    }

    pub fn signal(&mut self) -> FenceValue {
        self.fence_value.v += 1;

        unsafe { self.command_queue.Signal(&self.fence, self.fence_value.v) }.unwrap();
        self.fence_value
    }

    pub fn is_fence_completed(&self, fence_value: FenceValue) -> bool {
        let completed_value = unsafe { self.fence.GetCompletedValue() };
        completed_value >= fence_value.v
    }

    pub fn wait_fence(&self, fence_value: FenceValue) {
        if self.is_fence_completed(fence_value) {
            return;
        }

        unsafe {
            self.fence
                .SetEventOnCompletion(fence_value.v, self.fence_event)
                .unwrap();

            WaitForSingleObject(self.fence_event, INFINITE);
        }
    }

    pub fn flush(&mut self) {
        let v = self.signal();
        self.wait_fence(v);
    }
}

impl Drop for CommandQueue {
    fn drop(&mut self) {
        self.flush();
        unsafe { CloseHandle(self.fence_event) }.unwrap();
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
#[must_use]
pub struct FenceValue {
    v: u64,
}

pub struct CommandContext {
    command_list: ID3D12GraphicsCommandList7,
    allocator: ID3D12CommandAllocator,
}

impl CommandContext {
    pub fn command_list(&self) -> &ID3D12GraphicsCommandList7 {
        &self.command_list
    }
}

struct CommandAllocator {
    allocator: ID3D12CommandAllocator,
    fence_value: FenceValue,
}

fn create_command_allocator(
    device: &ID3D12Device2,
    cmd_list_type: D3D12_COMMAND_LIST_TYPE,
    name: &str,
) -> windows::core::Result<ID3D12CommandAllocator> {
    let allocator: ID3D12CommandAllocator =
        unsafe { device.CreateCommandAllocator(cmd_list_type) }?;
    set_name_str(&allocator, name)?;
    Ok(allocator)
}

fn create_command_list(
    device: &ID3D12Device2,
    allocator: &ID3D12CommandAllocator,
    command_list_type: D3D12_COMMAND_LIST_TYPE,
    initial_state: Option<&ID3D12PipelineState>,
    name: &str,
) -> windows::core::Result<ID3D12GraphicsCommandList7> {
    let command_list: ID3D12GraphicsCommandList7 =
        unsafe { device.CreateCommandList(0, command_list_type, allocator, initial_state) }?;
    set_name_str(&command_list, name)?;

    Ok(command_list)
}
