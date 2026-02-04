use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tauri::image::Image;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State};

#[cfg(not(windows))]
use tauri::{LogicalPosition, WebviewUrl, WebviewWindowBuilder};

mod native_overlay;
mod system_audio;

#[cfg(windows)]
use std::os::windows::process::{CommandExt, ExitStatusExt};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SttConfig {
    hotkey: String,
    run_in_background: bool,
    type_into_active_app: bool,
}

impl Default for SttConfig {
    fn default() -> Self {
        Self {
            hotkey: "Ctrl+Shift".to_string(),
            run_in_background: true,
            type_into_active_app: true,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SttStatus {
    running: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TranscriptEvent {
    text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LogEvent {
    stream: String,
    line: String,
}

struct InnerState {
    config: SttConfig,
    child: Option<Child>,
}

#[derive(Clone)]
struct AppState(Arc<Mutex<InnerState>>);

impl AppState {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(InnerState {
            config: SttConfig::default(),
            child: None,
        })))
    }
}

const OVERLAY_WIDTH_PX: i32 = 90;
const OVERLAY_HEIGHT_PX: i32 = 5;
const OVERLAY_HORIZONTAL_OFFSET_PX: i32 = 0;
const OVERLAY_VERTICAL_MARGIN_PX: i32 = 16;

const OVERLAY_HOVER_SCALE_X: f32 = 1.15;
const OVERLAY_HOVER_SCALE_Y: f32 = 5.0;

// Track overlay visibility and debounce sequence for hover collapse dwell
static OVERLAY_VISIBLE: OnceLock<AtomicBool> = OnceLock::new();
static HOVER_DWELL_SEQ: OnceLock<AtomicU64> = OnceLock::new();
static SOUND_EFFECTS_ENABLED: OnceLock<AtomicBool> = OnceLock::new();
static DICTATION_ACTIVE: OnceLock<AtomicBool> = OnceLock::new();
static DICTATION_LAST_START_MS: OnceLock<AtomicU64> = OnceLock::new();

fn overlay_visible_flag() -> &'static AtomicBool {
    OVERLAY_VISIBLE.get_or_init(|| AtomicBool::new(false))
}

fn hover_dwell_seq() -> &'static AtomicU64 {
    HOVER_DWELL_SEQ.get_or_init(|| AtomicU64::new(0))
}

fn sound_effects_enabled_flag() -> &'static AtomicBool {
    SOUND_EFFECTS_ENABLED.get_or_init(|| AtomicBool::new(true))
}

fn dictation_active_flag() -> &'static AtomicBool {
    DICTATION_ACTIVE.get_or_init(|| AtomicBool::new(false))
}

fn dictation_last_start_ms() -> &'static AtomicU64 {
    DICTATION_LAST_START_MS.get_or_init(|| AtomicU64::new(0))
}

fn now_millis() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn emit_dictation_start(app: &AppHandle) {
    let now = now_millis();
    let last = dictation_last_start_ms().load(Ordering::Relaxed);
    if now.saturating_sub(last) < 200 {
        return;
    }
    if dictation_active_flag()
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        dictation_last_start_ms().store(now, Ordering::SeqCst);
        let _ = app.emit("stt:dictation-start", ());
    }
}

fn emit_dictation_stop(app: &AppHandle) {
    if dictation_active_flag().swap(false, Ordering::SeqCst) {
        let _ = app.emit("stt:dictation-stop", ());
    }
}

#[cfg_attr(not(windows), allow(unused_variables))]
fn configure_overlay(app: &AppHandle) -> Result<(), String> {
    #[cfg(windows)]
    {
        let (x, y) = match app.primary_monitor() {
            Ok(Some(monitor)) => {
                let size = monitor.size();
                let position = monitor.position();
                let width = size.width as i32;
                let mut computed_x =
                    position.x + (width - OVERLAY_WIDTH_PX) / 2 - OVERLAY_HORIZONTAL_OFFSET_PX;
                if computed_x < position.x {
                    computed_x = position.x;
                }
                let computed_y = position.y + OVERLAY_VERTICAL_MARGIN_PX;
                (computed_x, computed_y)
            }
            _ => (0, OVERLAY_VERTICAL_MARGIN_PX),
        };

        return native_overlay::configure(
            OVERLAY_WIDTH_PX.max(1),
            OVERLAY_HEIGHT_PX.max(1),
            x,
            y,
            OVERLAY_HOVER_SCALE_X,
            OVERLAY_HOVER_SCALE_Y,
        );
    }

    #[cfg(not(windows))]
    {
        let _ = app;
        Ok(())
    }
}

#[cfg_attr(windows, allow(unused_variables))]
fn set_overlay_visibility(app: &AppHandle, visible: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        // Avoid redundant show/hide operations
        let was = overlay_visible_flag().swap(visible, Ordering::SeqCst);
        if was == visible {
            return Ok(());
        }
        if visible {
            configure_overlay(app)?;
            native_overlay::show()
        } else {
            native_overlay::hide()
        }
    }

    #[cfg(not(windows))]
    {
        if let Some(window) = app.get_webview_window("overlay") {
            if visible {
                let _: tauri::Result<()> = window.show();
                let _: tauri::Result<()> = window.set_focus();
            } else {
                let _: tauri::Result<()> = window.hide();
            }
        }
        Ok(())
    }
}

fn dev_workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR points to src-tauri; go up one level to workspace root
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri should have a parent directory")
        .to_path_buf()
}

fn resolve_script_path(app: &AppHandle) -> PathBuf {
    // In dev mode, always use workspace root; in production, use Resource directory
    let resource_path = app
        .path()
        .resolve("python/main.py", tauri::path::BaseDirectory::Resource);

    match resource_path {
        Ok(path) if path.exists() => path,
        _ => dev_workspace_root().join("python").join("main.py"),
    }
}

fn resolve_model_dir(app: &AppHandle) -> PathBuf {
    let resource_path = app
        .path()
        .resolve("data/parakeet_model", tauri::path::BaseDirectory::Resource);

    match resource_path {
        Ok(path) if path.exists() => path,
        _ => dev_workspace_root().join("data").join("parakeet_model"),
    }
}

fn emit_status(app: &AppHandle, running: bool) {
    let _ = app.emit("stt:status", SttStatus { running });
}

fn emit_log(app: &AppHandle, stream: &str, line: &str) {
    let _ = app.emit(
        "stt:log",
        LogEvent {
            stream: stream.to_string(),
            line: line.to_string(),
        },
    );
}

fn emit_transcript(app: &AppHandle, text: &str) {
    let _ = app.emit(
        "stt:transcript",
        TranscriptEvent {
            text: text.to_string(),
        },
    );
}

fn log_to_file(message: &str) {
    let log_path = dev_workspace_root().join("jargon_engine.log");
    if let Some(parent) = log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_path) {
        let _ = writeln!(file, "{}", message);
    }
}

fn spawn_reader_thread<R: std::io::Read + Send + 'static>(
    app: AppHandle,
    stream_name: &'static str,
    reader: R,
) {
    std::thread::spawn(move || {
        let buf = BufReader::new(reader);
        for line in buf.lines().flatten() {
            log_to_file(&format!("[python:{stream_name}] {line}"));
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                if value.get("type").and_then(|v| v.as_str()) == Some("overlay") {
                    if let Some(hover) = value.get("hover").and_then(|v| v.as_bool()) {
                        if hover {
                            let _ = set_overlay_visibility(&app, true);
                            hover_dwell_seq().fetch_add(1, Ordering::SeqCst);
                            let _ = crate::native_overlay::set_hover(true);
                        } else {
                            // Dwell for 30ms before collapsing; cancel if another event arrives
                            let seq = hover_dwell_seq().fetch_add(1, Ordering::SeqCst) + 1;
                            std::thread::spawn(move || {
                                std::thread::sleep(std::time::Duration::from_millis(30));
                                if hover_dwell_seq().load(Ordering::SeqCst) == seq {
                                    let _ = crate::native_overlay::set_hover(false);
                                }
                            });
                        }
                        continue;
                    }
                } else if value.get("type").and_then(|v| v.as_str()) == Some("dictation_start") {
                    // Emit event first so the frontend can play the sound effect
                    emit_dictation_start(&app);
                    // Pause any playing media
                    if let Err(err) = system_audio::set_music_muted(true) {
                        emit_log(&app, "audio", &format!("failed to pause media: {err}"));
                    }
                    continue;
                } else if value.get("type").and_then(|v| v.as_str()) == Some("dictation_stop") {
                    if let Err(err) = system_audio::set_music_muted(false) {
                        emit_log(
                            &app,
                            "audio",
                            &format!("failed to restore audio mute state: {err}"),
                        );
                    }
                    emit_dictation_stop(&app);
                    continue;
                } else if value.get("type").and_then(|v| v.as_str()) == Some("overlay_level") {
                    if let Some(level) = value.get("level").and_then(|v| v.as_f64()) {
                        let _ = crate::native_overlay::set_level(level as f32);
                        continue;
                    }
                } else if value.get("type").and_then(|v| v.as_str()) == Some("transcript") {
                    if let Some(text) = value.get("text").and_then(|v| v.as_str()) {
                        emit_transcript(&app, text);
                        continue;
                    }
                }
            }

            emit_log(&app, stream_name, &line);
        }
    });
}

fn start_engine_inner(app: &AppHandle, state: &AppState) -> Result<(), String> {
    let config = {
        let guard = state.0.lock().map_err(|_| "State lock poisoned")?;
        if guard.child.is_some() {
            emit_status(app, true);
            return Ok(());
        }
        guard.config.clone()
    };

    let script_path = resolve_script_path(app);
    log_to_file(&format!("[setup] resolved Python script path: {}", script_path.display()));
    eprintln!(
        "[setup] resolved Python script path: {}",
        script_path.display()
    );
    if !script_path.exists() {
        let msg = format!("Python script not found at {}", script_path.display());
        log_to_file(&format!("[error] {msg}"));
        return Err(msg);
    }

    let model_dir = resolve_model_dir(app);
    let python_dir = script_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| dev_workspace_root().join("python"));
    log_to_file(&format!("[setup] python cwd: {}", python_dir.display()));
    log_to_file(&format!("[setup] model dir: {}", model_dir.display()));

    // Build common args: run unbuffered for immediate stdout
    let mut args: Vec<std::ffi::OsString> = Vec::new();
    args.push("-u".into());
    // Run in module mode from the python directory, matching manual run
    args.push("-m".into());
    args.push("main".into());
    args.push("--hotkey".into());
    args.push(config.hotkey.clone().into());
    args.push("--model-dir".into());
    args.push(model_dir.as_os_str().to_owned());
    args.push("--type-into-active-app".into());
    args.push(if config.type_into_active_app {
        "true".into()
    } else {
        "false".into()
    });

    // On Windows prefer pyw (launcher) to avoid console window; fallback to pythonw/python
    #[cfg(windows)]
    let mut child = {
        let mut pyw_cmd = Command::new("pyw");
        let mut pyw_args = Vec::with_capacity(args.len() + 1);
        pyw_args.push("-3".into());
        pyw_args.extend(args.iter().cloned());
        eprintln!("[engine] spawn cwd: {}", python_dir.display());
        eprintln!("[engine] spawn cmd: pyw {:?}", pyw_args);
        pyw_cmd
            .args(&pyw_args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(python_dir.clone())
            .creation_flags(CREATE_NO_WINDOW);
        match pyw_cmd.spawn() {
            Ok(ch) => {
                eprintln!("[engine] started with 'pyw -3 -m main'");
                log_to_file("[engine] started with 'pyw -3 -m main'");
                ch
            }
            Err(pyw_err) => {
                log_to_file(&format!("[error] pyw spawn failed: {pyw_err}"));
                let mut command = Command::new("pythonw");
                eprintln!("[engine] fallback spawn cmd: pythonw {:?}", args);
                command
                    .args(&args)
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .current_dir(python_dir.clone())
                    .creation_flags(CREATE_NO_WINDOW);
                match command.spawn() {
                    Ok(ch) => {
                        eprintln!("[engine] started with 'pythonw -m main'");
                        log_to_file("[engine] started with 'pythonw -m main'");
                        ch
                    }
                    Err(py_err) => {
                        log_to_file(&format!("[error] pythonw spawn failed: {py_err}"));
                        let mut fallback = Command::new("python");
                        fallback
                            .args(&args)
                            .stdin(Stdio::null())
                            .stdout(Stdio::piped())
                            .stderr(Stdio::piped())
                            .current_dir(python_dir.clone())
                            .creation_flags(CREATE_NO_WINDOW);
                        match fallback.spawn() {
                            Ok(ch) => {
                                eprintln!("[engine] started with 'python -m main'");
                                log_to_file("[engine] started with 'python -m main'");
                                ch
                            }
                            Err(err) => {
                                let msg = format!(
                                    "Failed to start Python: pyw error: {pyw_err}; pythonw error: {py_err}; python error: {err}"
                                );
                                log_to_file(&format!("[error] {msg}"));
                                return Err(msg);
                            }
                        }
                    }
                }
            }
        }
    };

    #[cfg(not(windows))]
    let mut child = {
        let mut command = Command::new("python");
        eprintln!("[engine] spawn cwd: {}", python_dir.display());
        eprintln!("[engine] spawn cmd: python {:?}", args);
        command
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(python_dir.clone());
        match command.spawn() {
            Ok(ch) => ch,
            Err(err) => return Err(format!("Failed to start Python: {err}")),
        }
    };

    if let Some(stdout) = child.stdout.take() {
        spawn_reader_thread(app.clone(), "stdout", stdout);
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_reader_thread(app.clone(), "stderr", stderr);
    }

    {
        let mut guard = state.0.lock().map_err(|_| "State lock poisoned")?;
        guard.child = Some(child);
    }

    emit_status(app, true);

    let app_for_monitor = app.clone();
    let state_for_monitor = state.clone();
    std::thread::spawn(move || loop {
        let exit_status = {
            let mut guard = match state_for_monitor.0.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            let Some(child) = guard.child.as_mut() else {
                return;
            };

            match child.try_wait() {
                Ok(Some(status)) => Some(status),
                Ok(None) => None,
                Err(_) => Some(std::process::ExitStatus::from_raw(1)),
            }
        };

        if let Some(status) = exit_status {
            {
                let mut guard = match state_for_monitor.0.lock() {
                    Ok(g) => g,
                    Err(_) => return,
                };
                guard.child = None;
            }
            emit_status(&app_for_monitor, false);
            emit_log(
                &app_for_monitor,
                "engine",
                &format!("python exited: {status}"),
            );
            if let Err(err) = system_audio::set_music_muted(false) {
                emit_log(
                    &app_for_monitor,
                    "audio",
                    &format!("failed to restore audio mute state: {err}"),
                );
            }
            return;
        }

        std::thread::sleep(Duration::from_millis(250));
    });

    Ok(())
}

fn stop_engine_inner(app: &AppHandle, state: &AppState) -> Result<(), String> {
    let mut child = {
        let mut guard = state.0.lock().map_err(|_| "State lock poisoned")?;
        guard.child.take()
    };

    if let Some(child) = child.as_mut() {
        let _ = child.kill();
        let _ = child.wait();
    }

    emit_status(app, false);
    if let Err(err) = system_audio::set_music_muted(false) {
        emit_log(
            app,
            "audio",
            &format!("failed to restore audio mute state: {err}"),
        );
    }
    Ok(())
}

#[tauri::command]
fn stt_get_config(state: State<'_, AppState>) -> Result<SttConfig, String> {
    let guard = state.0.lock().map_err(|_| "State lock poisoned")?;
    Ok(guard.config.clone())
}

#[tauri::command]
fn stt_set_config(state: State<'_, AppState>, config: SttConfig) -> Result<(), String> {
    let mut guard = state.0.lock().map_err(|_| "State lock poisoned")?;
    guard.config = config;
    Ok(())
}

#[tauri::command]
fn stt_get_status(app: AppHandle, state: State<'_, AppState>) -> Result<SttStatus, String> {
    let running = state
        .0
        .lock()
        .map_err(|_| "State lock poisoned")?
        .child
        .is_some();
    emit_status(&app, running);
    Ok(SttStatus { running })
}

#[tauri::command]
fn stt_start(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    start_engine_inner(&app, &state)
}

#[tauri::command]
fn stt_stop(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    stop_engine_inner(&app, &state)
}

#[tauri::command]
fn stt_restart(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    stop_engine_inner(&app, &state)?;
    start_engine_inner(&app, &state)?;
    Ok(())
}

#[tauri::command]
fn sound_get_enabled() -> Result<bool, String> {
    Ok(sound_effects_enabled_flag().load(Ordering::SeqCst))
}

#[tauri::command]
fn sound_set_enabled(enabled: bool) -> Result<(), String> {
    sound_effects_enabled_flag().store(enabled, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
fn overlay_show(app: AppHandle, show: bool) -> Result<(), String> {
    set_overlay_visibility(&app, show)
}

// Removed: wave activation command; overlay remains minimal

fn setup_tray(app: &tauri::App) -> Result<(), tauri::Error> {
    let show = MenuItemBuilder::with_id("show", "Show").build(app)?;
    let hide = MenuItemBuilder::with_id("hide", "Hide").build(app)?;
    let start = MenuItemBuilder::with_id("start", "Start").build(app)?;
    let stop = MenuItemBuilder::with_id("stop", "Stop").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
    let menu = MenuBuilder::new(app)
        .item(&show)
        .item(&hide)
        .separator()
        .item(&start)
        .item(&stop)
        .separator()
        .item(&quit)
        .build()?;

    let tray_icon = Image::from_bytes(include_bytes!("../icons/icon.png"))
        .expect("failed to load tray icon");

    TrayIconBuilder::new()
        .icon(tray_icon)
        .menu(&menu)
        .on_menu_event(
            |app_handle: &tauri::AppHandle, event: tauri::menu::MenuEvent| match event.id().as_ref()
            {
                "show" => {
                    if let Some(window) = app_handle.get_webview_window("main") {
                        let _: tauri::Result<()> = window.show();
                        let _ = window.set_focus();
                    }
                    let _ = set_overlay_visibility(app_handle, false);
                }
                "hide" => {
                    if let Some(window) = app_handle.get_webview_window("main") {
                        let _: tauri::Result<()> = window.hide();
                    }
                    let _ = set_overlay_visibility(app_handle, true);
                }
                "start" => {
                    let state = app_handle.state::<AppState>();
                    let _ = start_engine_inner(app_handle, &state);
                }
                "stop" => {
                    let state = app_handle.state::<AppState>();
                    let _ = stop_engine_inner(app_handle, &state);
                }
                "quit" => app_handle.exit(0),
                _ => {}
            },
        )
        .build(app)?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::new())
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _: tauri::Result<()> = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            setup_tray(app)?;

            #[cfg(not(windows))]
            {
                let default_width = OVERLAY_WIDTH_PX as f64;
                let default_height = OVERLAY_HEIGHT_PX as f64;

                let overlay = WebviewWindowBuilder::new(
                    app,
                    "overlay",
                    WebviewUrl::App("overlay.html".into()),
                )
                .decorations(false)
                .transparent(true)
                .always_on_top(true)
                .skip_taskbar(true)
                .resizable(false)
                .inner_size(default_width, default_height)
                .min_inner_size(0.0, 0.0)
                .build()?;

                if let Ok(Some(monitor)) = app.primary_monitor() {
                    let size = monitor.size();
                    let position = monitor.position();
                    let mut x = position.x as f64 + (size.width as f64 - default_width) / 2.0
                        - OVERLAY_HORIZONTAL_OFFSET_PX as f64;
                    if x < position.x as f64 {
                        x = position.x as f64;
                    }
                    let y = position.y as f64 + OVERLAY_VERTICAL_MARGIN_PX as f64;
                    let _ = overlay.set_position(LogicalPosition::new(x, y));
                }
                let _: tauri::Result<()> = overlay.hide();
            }

            let handle_for_overlay = app.handle().clone();
            let _ = configure_overlay(&handle_for_overlay);
            let _ = set_overlay_visibility(&handle_for_overlay, false);

            // Auto-start the Python engine on app launch
            eprintln!("[setup] auto-starting Python engine...");
            let state_for_engine = app.state::<AppState>();
            let handle_for_engine = app.handle().clone();
            if let Err(e) = start_engine_inner(&handle_for_engine, &state_for_engine) {
                eprintln!("[setup] failed to start Python engine: {}", e);
            }

            if let Some(window) = app.get_webview_window("main") {
                let state = {
                    let state_ref = app.state::<AppState>();
                    state_ref.inner().clone()
                };
                let window_for_event = window.clone();
                let overlay_event_handle = app.handle().clone();
                let overlay_poll_handle = app.handle().clone();

                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        let run_in_background = state
                            .0
                            .lock()
                            .map(|g| g.config.run_in_background)
                            .unwrap_or(true);
                        if run_in_background {
                            api.prevent_close();
                            let _: tauri::Result<()> = window_for_event.hide();
                            let _ = set_overlay_visibility(&overlay_event_handle, true);
                        }
                    }
                });

                // Keep overlay always visible regardless of window focus/visibility
                let _main_handle = window.clone();
                std::thread::spawn(move || loop {
                    let show_overlay = true;

                    let _ = set_overlay_visibility(&overlay_poll_handle, show_overlay);

                    std::thread::sleep(Duration::from_millis(250));
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            stt_get_config,
            stt_set_config,
            stt_get_status,
            stt_start,
            stt_stop,
            stt_restart,
            sound_get_enabled,
            sound_set_enabled,
            overlay_show
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
