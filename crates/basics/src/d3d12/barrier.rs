use windows::Win32::Graphics::Direct3D12::*;

pub fn transition_barrier(
    resource: &ID3D12Resource,
    old_state: D3D12_RESOURCE_STATES,
    new_state: D3D12_RESOURCE_STATES,
) -> D3D12_RESOURCE_BARRIER {
    // https://github.com/microsoft/windows-rs/blob/master/crates/samples/windows/direct3d12/src/main.rs#L486
    D3D12_RESOURCE_BARRIER {
        Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
        Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
        Anonymous: D3D12_RESOURCE_BARRIER_0 {
            Transition: std::mem::ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                pResource: unsafe { std::mem::transmute_copy(resource) },
                Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                StateBefore: old_state,
                StateAfter: new_state,
            }),
        },
    }
}
