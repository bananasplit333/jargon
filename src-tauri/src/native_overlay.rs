#[cfg(windows)]
mod platform {
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::atomic::AtomicU32;
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
    const CORNER_RADIUS: i32 = 3;
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
    static LEVEL_MILLIS: AtomicU32 = AtomicU32::new(0);
    static LEVEL_TICK: AtomicU64 = AtomicU64::new(0);
    static FORCE_HOVER: AtomicBool = AtomicBool::new(false);
    static LAST_POINTER_INSIDE: AtomicBool = AtomicBool::new(false);

    fn storage() -> &'static Mutex<Option<SharedHwnd>> {
        OVERLAY_HWND.get_or_init(|| Mutex::new(None))
    }

    fn metrics_storage() -> &'static Mutex<OverlayMetrics> {
        METRICS.get_or_init(|| Mutex::new(OverlayMetrics::new()))
    }

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

                let (hover, width, height) = {
                    let guard = metrics_storage().lock().unwrap();
                    (guard.hover, guard.current.width.max(1), guard.current.height.max(1))
                };

                if hover && height >= 12 {
                    let level = (LEVEL_MILLIS.load(Ordering::Relaxed) as f32 / 1000.0)
                        .clamp(0.0, 1.0);
                    let tick = LEVEL_TICK.load(Ordering::Relaxed);
                    draw_level_bars(hdc, width, height, level, tick);
                }

                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            winmsg::WM_MOUSEMOVE => {
                let (x, y) = decode_mouse_coords(l_param);
                let inside = pointer_inside_current(x, y);
                LAST_POINTER_INSIDE.store(inside, Ordering::Relaxed);
                if !FORCE_HOVER.load(Ordering::Relaxed) {
                    let _ = handle_hover_change(inside);
                }
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
                LAST_POINTER_INSIDE.store(false, Ordering::Relaxed);
                if !FORCE_HOVER.load(Ordering::Relaxed) {
                    let _ = handle_hover_change(false);
                }
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

    fn draw_level_bars(hdc: windows::Win32::Graphics::Gdi::HDC, width: i32, height: i32, level: f32, tick: u64) {
        let bar_count: i32 = 9;
        let gap: i32 = 2;
        let bar_width: i32 = 3;
        let padding_y: i32 = 3;

        let available_height = (height - padding_y * 2).max(1);
        let min_bar_height = 2.min(available_height);
        let max_bar_height = available_height.max(min_bar_height);

        let total_width = bar_count * bar_width + (bar_count - 1) * gap;
        let start_x = (((width - total_width) as f32) / 2.0).round() as i32;
        let center_y = (height as f32 / 2.0).round() as i32;

        let weights: [f32; 9] = [0.35, 0.55, 0.75, 0.95, 1.0, 0.95, 0.75, 0.55, 0.35];
        let base_level = level.clamp(0.0, 1.0).powf(0.65);
        let brush = unsafe { CreateSolidBrush(COLORREF(0x00FFFFFF)) };
        for i in 0..bar_count {
            let weight = weights.get(i as usize).copied().unwrap_or(1.0);
            let phase = (tick as f32 * 0.22) + (i as f32 * 0.85);
            let wobble = 0.75 + 0.25 * phase.sin();
            let bar_level = (base_level * wobble * weight).clamp(0.0, 1.0);
            let h = (min_bar_height as f32
                + (max_bar_height - min_bar_height) as f32 * bar_level)
                .round() as i32;
            let left = start_x + i * (bar_width + gap);
            let top = (center_y - h / 2).max(0);
            let bottom = (center_y + (h - h / 2)).min(height);
            let rect = RECT {
                left,
                top,
                right: left + bar_width,
                bottom,
            };
            let _ = unsafe { FillRect(hdc, &rect, brush) };
        }
        let _ = unsafe { DeleteObject(brush.into()) };
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

    pub fn set_hover_platform(active: bool) -> Result<(), Error> {
        FORCE_HOVER.store(active, Ordering::SeqCst);
        if active {
            handle_hover_change(true)
        } else {
            handle_hover_change(LAST_POINTER_INSIDE.load(Ordering::Relaxed))
        }
    }

    pub fn set_level_platform(level: f32) -> Result<(), Error> {
        let clamped = level.clamp(0.0, 1.0);
        LEVEL_MILLIS.store((clamped * 1000.0).round() as u32, Ordering::Relaxed);
        LEVEL_TICK.fetch_add(1, Ordering::Relaxed);
        let hwnd = ensure_window()?;
        unsafe {
            let _ = InvalidateRect(hwnd, core::ptr::null(), 1);
        }
        Ok(())
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
        FORCE_HOVER.store(false, Ordering::SeqCst);
        LAST_POINTER_INSIDE.store(false, Ordering::SeqCst);
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

#[cfg(windows)]
pub fn set_hover(active: bool) -> Result<(), String> {
    platform::set_hover_platform(active).map_err(|e: windows::core::Error| e.to_string())
}

#[cfg(windows)]
pub fn set_level(level: f32) -> Result<(), String> {
    platform::set_level_platform(level).map_err(|e: windows::core::Error| e.to_string())
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

#[cfg(not(windows))]
pub fn set_hover(_active: bool) -> Result<(), String> {
    Ok(())
}

#[cfg(not(windows))]
pub fn set_level(_level: f32) -> Result<(), String> {
    Ok(())
}
