use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    System::LibraryLoader::GetModuleHandleA,
    UI::Input::KeyboardAndMouse::*,
    UI::WindowsAndMessaging::*,
};

use super::gfx::{renderer::Renderer, scene::Scene};

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

    let mut _framework: Option<Framework> = None;

    let hwnd = unsafe {
        AdjustWindowRect(&mut rect, WS_OVERLAPPEDWINDOW, false)?;

        CreateWindowExA(
            WINDOW_EX_STYLE::default(),
            name,
            windows::core::s!("Raytracing basics"),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            rect.right - rect.left,
            rect.bottom - rect.top,
            None,
            None,
            instance,
            Some(&mut _framework as *mut Option<Framework> as *mut std::ffi::c_void),
        )?
    };

    _framework = Some(Framework::new(
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
            let mut framework = get_framework_ptr(hwnd);
            if let Some(framework) = unsafe { framework.as_mut() } {
                framework.update();
                framework.render().unwrap();
            }
            return LRESULT::default();
        }
        WM_KEYUP => {
            if wparam.0 == VK_SPACE.0.into() {
                let mut framework = get_framework_ptr(hwnd);
                if let Some(framework) = unsafe { framework.as_mut() } {
                    framework.renderer.toggle_rendering_mode();
                }
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

fn get_framework_ptr(hwnd: HWND) -> std::ptr::NonNull<Option<Framework>> {
    let user_data = unsafe { GetWindowLongPtrA(hwnd, GWLP_USERDATA) };
    std::ptr::NonNull::<Option<Framework>>::new(user_data as _).unwrap()
}

struct Framework {
    scene: Scene,
    renderer: Renderer,
}

impl Framework {
    fn new(hwnd: HWND, screen_width: u32, screen_height: u32) -> Self {
        let mut renderer = Renderer::new(hwnd, screen_width, screen_height);
        let scene = Scene::build(renderer.device_mut(), screen_width, screen_height).unwrap();
        Self { scene, renderer }
    }

    fn update(&mut self) {
        self.scene.update();
    }

    fn render(&mut self) -> windows::core::Result<()> {
        self.renderer.render(&mut self.scene)
    }
}
