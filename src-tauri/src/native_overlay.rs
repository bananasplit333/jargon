#[cfg(windows)]
mod platform {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Mutex, OnceLock};
    use std::thread;
    use std::time::Duration;

    use core::ffi::c_void;

    use windows::core::{w, Error, PCWSTR};
    use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        BeginPaint, CreateRoundRectRgn, CreateSolidBrush, DeleteObject, EndPaint, FillRect,
        HRGN, PAINTSTRUCT,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{LoadCursorW, SetCursor, IDC_ARROW};
    use windows::Win32::UI::WindowsAndMessaging::HCURSOR;
    use windows::Win32::UI::WindowsAndMessaging::{
        self as winmsg, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW,
        SetLayeredWindowAttributes, SetWindowPos, ShowWindow, TranslateMessage, MSG, WINDOW_EX_STYLE, WINDOW_STYLE,
        WNDCLASSW,
    };

    #[repr(C)]
    #[allow(non_snake_case)]
    struct TRACKMOUSEEVENT {
        cbSize: u32,
        dwFlags: u32,
        hwndTrack: HWND,
        dwHoverTime: u32,
    }

    const TME_LEAVE: u32 = 0x00000002;
    const WM_MOUSELEAVE: u32 = 0x02A3;
    // No custom messages for wave/animation

    #[link(name = "user32")]
    extern "system" {
        fn TrackMouseEvent(lpEventTrack: *mut TRACKMOUSEEVENT) -> i32;
        fn InvalidateRect(hWnd: HWND, lpRect: *const RECT, bErase: i32) -> i32;
        fn SetWindowRgn(hWnd: HWND, hRgn: HRGN, bRedraw: i32) -> i32;
        // No timer APIs needed
    }

    const CLASS_NAME: PCWSTR = w!("JargonNativeOverlayClass");
    const WINDOW_NAME: PCWSTR = w!("JargonNativeOverlayWindow");
    const WINDOW_STYLE_FLAGS: WINDOW_STYLE = winmsg::WS_POPUP;
    const ANIMATION_STEPS: u32 = 8;
    const ANIMATION_FRAME_MS: u64 = 14;
    const CORNER_RADIUS: i32 = 8;
    // No wave/line animation constants; keep overlay minimal
    fn ensure_class_registered() -> Result<(), Error> {
        CLASS_REGISTERED
            .get_or_init(|| unsafe {
                let h_instance = GetModuleHandleW(None)?;
                let class = WNDCLASSW {
                    style: winmsg::CS_HREDRAW | winmsg::CS_VREDRAW,
                    lpfnWndProc: Some(wnd_proc),
                    hInstance: h_instance.into(),
                    lpszClassName: CLASS_NAME,
                    ..Default::default()
                };

                if RegisterClassW(&class) == 0 {
                    Err(Error::from_win32())
                } else {
                    Ok(())
                }
            })
            .clone()
    }

    #[derive(Copy, Clone)]
    struct SharedHwnd(isize);

    impl SharedHwnd {
        fn new(hwnd: HWND) -> Self {
            Self(hwnd.0 as isize)
        }

        fn hwnd(self) -> HWND {
            HWND(self.0 as *mut c_void)
        }
    }

    unsafe impl Send for SharedHwnd {}
    unsafe impl Sync for SharedHwnd {}

    #[derive(Clone, Copy, Default, PartialEq, Eq)]
    struct Geometry {
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    }

    impl Geometry {
        fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
            Self { x, y, width, height }
        }

        fn lerp(self, other: Geometry, t: f32) -> Self {
            fn lerp_i32(start: i32, end: i32, t: f32) -> i32 {
                (start as f32 + (end - start) as f32 * t).round() as i32
            }

            Geometry {
                x: lerp_i32(self.x, other.x, t),
                y: lerp_i32(self.y, other.y, t),
                width: lerp_i32(self.width, other.width, t).max(1),
                height: lerp_i32(self.height, other.height, t).max(1),
            }
        }
    }

    struct OverlayMetrics {
        base: Geometry,
        expanded: Geometry,
        current: Geometry,
        hover: bool,
    }

    impl OverlayMetrics {
        fn new() -> Self {
            Self {
                base: Geometry::default(),
                expanded: Geometry::default(),
                current: Geometry::default(),
                hover: false,
            }
        }
    }

    static OVERLAY_HWND: OnceLock<Mutex<Option<SharedHwnd>>> = OnceLock::new();
    static CLASS_REGISTERED: OnceLock<Result<(), Error>> = OnceLock::new();
    static METRICS: OnceLock<Mutex<OverlayMetrics>> = OnceLock::new();
    static ANIMATION_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn storage() -> &'static Mutex<Option<SharedHwnd>> {
        OVERLAY_HWND.get_or_init(|| Mutex::new(None))
    }

    fn metrics_storage() -> &'static Mutex<OverlayMetrics> {
        METRICS.get_or_init(|| Mutex::new(OverlayMetrics::new()))
    }

    // We won't rely on WM_MOUSELEAVE or mouse capture; instead, we
    // toggle hover based on whether the current pointer position is
    // inside the overlay bounds during WM_MOUSEMOVE.

    fn decode_mouse_coords(l_param: LPARAM) -> (i32, i32) {
        let raw = l_param.0 as u32;
        let x = (raw & 0xFFFF) as u16 as i16 as i32;
        let y = (raw >> 16) as u16 as i16 as i32;
        (x, y)
    }

    fn pointer_inside_current(x: i32, y: i32) -> bool {
        let metrics = metrics_storage();
        let guard = metrics.lock().unwrap();
        let width = guard.current.width.max(1);
        let height = guard.current.height.max(1);
        x >= 0 && y >= 0 && x < width && y < height
    }

    unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, _w_param: WPARAM, l_param: LPARAM) -> LRESULT {
        match msg {
            winmsg::WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                let brush = CreateSolidBrush(COLORREF(0x000000));
                let _ = FillRect(hdc, &RECT::from(ps.rcPaint), brush);
                let _ = DeleteObject(brush.into());
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            winmsg::WM_MOUSEMOVE => {
                let (x, y) = decode_mouse_coords(l_param);
                let inside = pointer_inside_current(x, y);
                let _ = handle_hover_change(inside);
                if inside {
                    let mut tme = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    let _ = unsafe { TrackMouseEvent(&mut tme) };
                }
                LRESULT(0)
            }
            winmsg::WM_SETCURSOR => {
                // Force normal arrow cursor to avoid busy spinner
                match unsafe { LoadCursorW(None, IDC_ARROW) } {
                    Ok(hcur) => {
                        let _prev = unsafe { SetCursor(Some(hcur)) };
                        LRESULT(1)
                    }
                    Err(_) => unsafe { DefWindowProcW(hwnd, msg, _w_param, l_param) },
                }
            }
            WM_MOUSELEAVE => {
                let _ = handle_hover_change(false);
                LRESULT(0)
            }
            winmsg::WM_DESTROY => {
                if let Some(mutex) = OVERLAY_HWND.get() {
                    let mut guard = mutex.lock().unwrap();
                    *guard = None;
                }
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, _w_param, l_param) },
        }
    }

    fn overlay_ex_style_flags() -> WINDOW_EX_STYLE {
        WINDOW_EX_STYLE(winmsg::WS_EX_LAYERED.0 | winmsg::WS_EX_TOOLWINDOW.0 | winmsg::WS_EX_TOPMOST.0)
    }

    fn spawn_overlay_thread_and_get_hwnd() -> Result<HWND, Error> {
        use std::sync::mpsc;
        ensure_class_registered()?;
        let (tx, rx) = mpsc::sync_channel::<isize>(1);
        thread::spawn(move || {
            unsafe {
                let h_instance = match GetModuleHandleW(None) {
                    Ok(h) => h,
                    Err(_) => return,
                };
                let hwnd = match CreateWindowExW(
                    overlay_ex_style_flags(),
                    CLASS_NAME,
                    WINDOW_NAME,
                    WINDOW_STYLE_FLAGS,
                    0,
                    0,
                    1,
                    1,
                    None,
                    None,
                    Some(h_instance.into()),
                    None,
                ) {
                    Ok(h) => h,
                    Err(_) => return,
                };
                // Configure layering and start hidden
                let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, winmsg::LWA_ALPHA);
                let _ = ShowWindow(hwnd, winmsg::SW_HIDE);
                let _ = tx.send(hwnd.0 as isize);

                // Message pump
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).into() {
                    let _ = TranslateMessage(&msg);
                    let _ = DispatchMessageW(&msg);
                }
            }
        });

        // Wait for window creation
        let hwnd_isize = rx.recv().map_err(|_| Error::from_win32())?;
        Ok(HWND(hwnd_isize as *mut core::ffi::c_void))
    }

    fn ensure_window() -> Result<HWND, Error> {
        let mut guard = storage().lock().unwrap();
        if let Some(shared) = *guard {
            let hwnd = shared.hwnd();
            if unsafe { winmsg::IsWindow(Some(hwnd)).as_bool() } {
                return Ok(hwnd);
            }
        }
        let hwnd = spawn_overlay_thread_and_get_hwnd()?;
        *guard = Some(SharedHwnd::new(hwnd));
        Ok(hwnd)
    }

    fn apply_geometry(hwnd: HWND, geom: Geometry) -> Result<(), Error> {
        let width = geom.width.max(1);
        let height = geom.height.max(1);
        unsafe {
            SetWindowPos(
                hwnd,
                Some(winmsg::HWND_TOPMOST),
                geom.x,
                geom.y,
                width,
                height,
                winmsg::SWP_NOACTIVATE,
            )?;

            // Update rounded window region to maintain rounded borders on resize
            let hrgn = CreateRoundRectRgn(0, 0, width, height, CORNER_RADIUS * 2, CORNER_RADIUS * 2);
            let _ = SetWindowRgn(hwnd, hrgn, 1);

            // Request a repaint after geometry changes
            let _ = InvalidateRect(hwnd, core::ptr::null(), 1);
        }
        Ok(())
    }

    fn handle_hover_change(hover: bool) -> Result<(), Error> {
        let target = {
            let metrics = metrics_storage();
            let mut guard = metrics.lock().unwrap();
            if guard.hover == hover {
                return Ok(());
            }
            guard.hover = hover;
            if hover {
                guard.expanded
            } else {
                guard.base
            }
        };
        let hwnd = ensure_window()?;
        unsafe { let _ = InvalidateRect(hwnd, core::ptr::null(), 1); }
        animate_to(target)
    }

    fn animate_to(target: Geometry) -> Result<(), Error> {
        let hwnd = ensure_window()?;
        let shared = SharedHwnd::new(hwnd);
        let start = {
            let metrics = metrics_storage();
            metrics.lock().unwrap().current
        };

        if start == target {
            return Ok(());
        }

        let sequence = ANIMATION_SEQUENCE.fetch_add(1, Ordering::SeqCst) + 1;

        thread::spawn(move || {
            let step_count = ANIMATION_STEPS.max(1);
            for step in 1..=step_count {
                if ANIMATION_SEQUENCE.load(Ordering::SeqCst) != sequence {
                    return;
                }

                let t = step as f32 / step_count as f32;
                let next = start.lerp(target, t);
                if apply_geometry(shared.hwnd(), next).is_ok() {
                    let metrics = metrics_storage();
                    let mut guard = metrics.lock().unwrap();
                    guard.current = next;
                } else {
                    return;
                }

                thread::sleep(Duration::from_millis(ANIMATION_FRAME_MS));
            }

            if ANIMATION_SEQUENCE.load(Ordering::SeqCst) == sequence {
                if apply_geometry(shared.hwnd(), target).is_ok() {
                    let metrics = metrics_storage();
                    let mut guard = metrics.lock().unwrap();
                    guard.current = target;
                }
            }
        });

        Ok(())
    }

    // No wave-related functions; overlay remains minimal

    pub fn configure(width: i32, height: i32, x: i32, y: i32, hover_scale_x: f32, hover_scale_y: f32) -> Result<(), Error> {
        let hwnd = ensure_window()?;

        let scale_x = hover_scale_x.max(1.0);
        let scale_y = hover_scale_y.max(1.0);
        let expanded_width = ((width as f32) * scale_x).round() as i32;
        let expanded_height = ((height as f32) * scale_y).round() as i32;
        let expanded_width = expanded_width.max(width);
        let expanded_height = expanded_height.max(height);

        let center_x = x as f32 + width as f32 / 2.0;
        let center_y = y as f32 + height as f32 / 2.0;
        let expanded_x = (center_x - expanded_width as f32 / 2.0).round() as i32;
        let expanded_y = (center_y - expanded_height as f32 / 2.0).round() as i32;

        let base_geom = Geometry::new(x, y, width, height);
        let expanded_geom = Geometry::new(expanded_x, expanded_y, expanded_width, expanded_height);

        let target = {
            let metrics = metrics_storage();
            let mut guard = metrics.lock().unwrap();
            guard.base = base_geom;
            guard.expanded = expanded_geom;
            let target = if guard.hover { expanded_geom } else { base_geom };
            guard.current = target;
            target
        };

        ANIMATION_SEQUENCE.fetch_add(1, Ordering::SeqCst);
        apply_geometry(hwnd, target)
    }

    pub fn show() -> Result<(), Error> {
        let hwnd = ensure_window()?;
        unsafe {
            let _ = ShowWindow(hwnd, winmsg::SW_SHOWNA);
        }
        Ok(())
    }

    pub fn hide() -> Result<(), Error> {
        let hwnd = ensure_window()?;
        ANIMATION_SEQUENCE.fetch_add(1, Ordering::SeqCst);
        if let Some(metrics) = METRICS.get() {
            let mut guard = metrics.lock().unwrap();
            guard.hover = false;
            guard.current = guard.base;
        }
        unsafe {
            let _ = ShowWindow(hwnd, winmsg::SW_HIDE);
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn configure(_width: i32, _height: i32, _x: i32, _y: i32, _hover_scale_x: f32, _hover_scale_y: f32) -> Result<(), String> {
        Ok(())
    }

    pub fn show() -> Result<(), String> {
        Ok(())
    }

    pub fn hide() -> Result<(), String> {
        Ok(())
    }
}

#[cfg(windows)]
pub fn configure(width: i32, height: i32, x: i32, y: i32, hover_scale_x: f32, hover_scale_y: f32) -> Result<(), String> {
    platform::configure(width, height, x, y, hover_scale_x, hover_scale_y)
        .map_err(|e: windows::core::Error| e.to_string())
}

#[cfg(windows)]
pub fn show() -> Result<(), String> {
    platform::show().map_err(|e: windows::core::Error| e.to_string())
}

#[cfg(windows)]
pub fn hide() -> Result<(), String> {
    platform::hide().map_err(|e: windows::core::Error| e.to_string())
}

#[cfg(not(windows))]
pub fn configure(width: i32, height: i32, x: i32, y: i32, hover_scale_x: f32, hover_scale_y: f32) -> Result<(), String> {
    platform::configure(width, height, x, y, hover_scale_x, hover_scale_y)
}

#[cfg(not(windows))]
pub fn show() -> Result<(), String> {
    platform::show()
}

#[cfg(not(windows))]
pub fn hide() -> Result<(), String> {
    platform::hide()
}
