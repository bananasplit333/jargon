use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State};

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

fn dev_workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")))
        .to_path_buf()
}

fn resolve_script_path(app: &AppHandle) -> PathBuf {
    app.path()
        .resolve("python/main.py", tauri::path::BaseDirectory::Resource)
        .unwrap_or_else(|_| dev_workspace_root().join("python").join("main.py"))
}

fn resolve_model_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .resolve("data/parakeet_model", tauri::path::BaseDirectory::Resource)
        .unwrap_or_else(|_| dev_workspace_root().join("data").join("parakeet_model"))
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

fn spawn_reader_thread<R: std::io::Read + Send + 'static>(
    app: AppHandle,
    stream_name: &'static str,
    reader: R,
) {
    std::thread::spawn(move || {
        let buf = BufReader::new(reader);
        for line in buf.lines().flatten() {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                if value.get("type").and_then(|v| v.as_str()) == Some("transcript") {
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
    if !script_path.exists() {
        return Err(format!(
            "Python script not found at {}",
            script_path.display()
        ));
    }

    let model_dir = resolve_model_dir(app);

    let mut command = Command::new("python");
    command
        .arg(script_path)
        .arg("--hotkey")
        .arg(config.hotkey)
        .arg("--model-dir")
        .arg(model_dir)
        .arg("--type-into-active-app")
        .arg(if config.type_into_active_app {
            "true"
        } else {
            "false"
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn().map_err(|err| format!("{err}"))?;

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
            emit_log(&app_for_monitor, "engine", &format!("python exited: {status}"));
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

    TrayIconBuilder::new()
        .menu(&menu)
        .on_tray_icon_event(|tray, event| {
            if matches!(event, TrayIconEvent::Click { .. }) {
                if let Some(window) = tray.app_handle().get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .on_menu_event(|app_handle, event| match event.id().as_ref() {
            "show" => {
                if let Some(window) = app_handle.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "hide" => {
                if let Some(window) = app_handle.get_webview_window("main") {
                    let _ = window.hide();
                }
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
        })
        .build(app)?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::new())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            setup_tray(app)?;

            if let Some(window) = app.get_webview_window("main") {
                let app_handle = app.handle().clone();
                let state = app_handle.state::<AppState>().clone();
                let window_for_event = window.clone();

                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        let run_in_background = state
                            .0
                            .lock()
                            .map(|g| g.config.run_in_background)
                            .unwrap_or(true);
                        if run_in_background {
                            api.prevent_close();
                            let _ = window_for_event.hide();
                        }
                    }
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
            stt_restart
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
