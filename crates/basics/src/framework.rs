use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    System::LibraryLoader::GetModuleHandleA,
    UI::WindowsAndMessaging::*,
};

use super::renderer::Renderer;

pub fn run(config: &crate::Config) -> windows::core::Result<()> {
    let name = windows::core::s!("window");

    let instance = unsafe { GetModuleHandleA(None)? };

    let wnd_class = WNDCLASSEXA {
        cbSize: std::mem::size_of::<WNDCLASSEXA>() as u32,
        style: CS_HREDRAW | CS_VREDRAW, // redraw when window size or position changes, horizontally and vertically
        lpfnWndProc: Some(wnd_proc),
        hInstance: instance.into(),
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW)? },
        lpszClassName: name,
        ..Default::default()
    };
    debug_assert_ne!(unsafe { RegisterClassExA(&wnd_class) }, 0);

    let mut rect = RECT {
        left: 0,
        top: 0,
        right: config.client_width().try_into().unwrap(),
        bottom: config.client_height().try_into().unwrap(),
    };

    let mut _renderer: Option<Renderer> = None;

    let hwnd = unsafe {
        AdjustWindowRect(&mut rect, WS_OVERLAPPEDWINDOW, false)?;

        CreateWindowExA(
            WINDOW_EX_STYLE::default(),
            name,
            windows::core::s!("Hello cube"),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            rect.right - rect.left,
            rect.bottom - rect.top,
            None,
            None,
            instance,
            Some(&mut _renderer as *mut Option<Renderer> as *mut std::ffi::c_void),
        )?
    };

    _renderer = Some(Renderer::new(
        hwnd,
        config.client_width(),
        config.client_height(),
    ));

    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
    }

    let mut msg = MSG::default();
    while msg.message != WM_QUIT {
        if unsafe { PeekMessageA(&mut msg, None, 0, 0, PM_REMOVE) }.into() {
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageA(&msg);
            }
        }
    }

    Ok(())
}

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            unsafe {
                let data: &CREATESTRUCTA = std::mem::transmute(lparam);
                SetWindowLongPtrA(hwnd, GWLP_USERDATA, data.lpCreateParams as _);
            }
            return LRESULT::default();
        }
        WM_PAINT => {
            let user_data = unsafe { GetWindowLongPtrA(hwnd, GWLP_USERDATA) };
            if let Some(mut renderer) = std::ptr::NonNull::<Option<Renderer>>::new(user_data as _) {
                if let Some(renderer) = unsafe { renderer.as_mut() } {
                    renderer.update();
                    renderer.render().unwrap();
                }
            }
            return LRESULT::default();
        }
        WM_KEYUP => {
            if wparam.0 == 32 {
                println!("Space key has been pressed");
            }
            return LRESULT::default();
        }
        WM_DESTROY => {
            unsafe {
                PostQuitMessage(0);
            }
            return LRESULT::default();
        }
        _ => unsafe { DefWindowProcA(hwnd, msg, wparam, lparam) },
    }
}
