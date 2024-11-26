use windows::core::{s, Interface, Result, PCSTR};
use windows::Win32::Graphics::Direct3D12::ID3D12GraphicsCommandList;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};

// WinPixEventRuntime
// https://devblogs.microsoft.com/pix/winpixeventruntime/
pub struct Pix {
    begin_event: BeginEventOnCommandList,
    end_event: EndEventOnCommandList,
    _set_marker: SetMarkerOnCommandList,
}

impl Pix {
    pub fn build() -> Result<Self> {
        let module = unsafe { LoadLibraryA(s!("WinPixEventRuntime.dll")) }?;

        let Some(begin_event) =
            (unsafe { GetProcAddress(module, s!("PIXBeginEventOnCommandList")) })
        else {
            return Err(windows::core::Error::new(
                windows::Win32::Foundation::E_FAIL,
                "Failed to get `PIXBeginEventOnCommandList`",
            ));
        };

        let Some(end_event) = (unsafe { GetProcAddress(module, s!("PIXEndEventOnCommandList")) })
        else {
            return Err(windows::core::Error::new(
                windows::Win32::Foundation::E_FAIL,
                "Failed to get `PIXEndEventOnCommandList`",
            ));
        };

        let Some(set_marker) = (unsafe { GetProcAddress(module, s!("PIXSetMarkerOnCommandList")) })
        else {
            return Err(windows::core::Error::new(
                windows::Win32::Foundation::E_FAIL,
                "Failed to get `PIXSetMarkerOnCommandList`",
            ));
        };

        Ok(Self {
            begin_event: unsafe {
                std::mem::transmute::<ProcAddress, BeginEventOnCommandList>(begin_event)
            },
            end_event: unsafe {
                std::mem::transmute::<ProcAddress, EndEventOnCommandList>(end_event)
            },
            _set_marker: unsafe {
                std::mem::transmute::<ProcAddress, SetMarkerOnCommandList>(set_marker)
            },
        })
    }

    pub fn begin_event<'a>(
        &'a self,
        command_list: &'a ID3D12GraphicsCommandList,
        color: u64,
        name: &'a str,
    ) -> PixEvent<'a> {
        let begin_event = self.begin_event;
        let name: String = name.chars().chain(std::iter::once('\0')).collect();

        // seems like the command list must be cloned, or BeginEventOnCommandList crashes
        // https://www.polymonster.co.uk/blog/bulding-new-engine-in-rust-2
        unsafe { begin_event(command_list.clone().as_raw(), color, PCSTR(name.as_ptr())) };
        PixEvent {
            pix: self,
            command_list,
        }
    }

    pub fn _set_marker(&self, command_list: &ID3D12GraphicsCommandList, color: u64, name: &str) {
        let set_marker = self._set_marker;
        let name: String = name.chars().chain(std::iter::once('\0')).collect();
        unsafe { set_marker(command_list.clone().as_raw(), color, PCSTR(name.as_ptr())) };
    }
}

pub struct PixEvent<'a> {
    pix: &'a Pix,
    command_list: &'a ID3D12GraphicsCommandList,
}

impl<'a> Drop for PixEvent<'a> {
    fn drop(&mut self) {
        let end_event = self.pix.end_event;
        unsafe { end_event(self.command_list.as_raw()) };
    }
}

pub fn pix_color(red: u8, green: u8, blue: u8) -> u64 {
    // The format is ARGB and the alpha channel value must be 0xff
    // https://devblogs.microsoft.com/pix/winpixeventruntime/
    let red: u64 = red.into();
    let green: u64 = green.into();
    let blue: u64 = blue.into();
    0xff_00_00_00 | (red << 16) | (green << 8) | blue
}

type ProcAddress = unsafe extern "system" fn() -> isize;

// Specify "system" for WinAPI
// https://doc.rust-lang.org/nightly/reference/items/external-blocks.html#r-items.extern.abi.stdcall
type BeginEventOnCommandList = unsafe extern "system" fn(*mut std::ffi::c_void, u64, PCSTR);
type EndEventOnCommandList = unsafe extern "system" fn(*mut std::ffi::c_void);
type SetMarkerOnCommandList = unsafe extern "system" fn(*mut std::ffi::c_void, u64, PCSTR);
