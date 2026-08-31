pub mod ops;
mod watch;

use serde::Serialize;
use std::{
    fmt::Display,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager, State};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use todo_core::{
    Capture, CaptureStore, DesktopBackend, DesktopIntent, DesktopSnapshot, TriageResult,
};
use watch::{TargetFileWatcher, WatchSignal};

struct AppState {
    ops: ops::DesktopOps,
    watcher: Mutex<Option<TargetFileWatcher>>,
    status: Mutex<DesktopStatus>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DesktopStatus {
    pub startup_ms: Option<u64>,
    pub shortcut_status: ShortcutStatus,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ShortcutStatus {
    Pending,
    Registered,
    Error { message: String },
}

#[derive(Debug, Serialize)]
pub struct CaptureAppendResponse {
    pub capture: Capture,
    pub captures: Vec<Capture>,
}

#[derive(Debug, Serialize)]
pub struct CaptureTriageResponse {
    pub result: TriageResult,
    pub snapshot: CommandValue<DesktopSnapshot>,
    pub captures: CommandValue<Vec<Capture>>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CommandValue<T> {
    Available { value: T },
    Unavailable { message: String },
}

impl<T> CommandValue<T> {
    pub fn from_result<E: Display>(result: Result<T, E>) -> Self {
        match result {
            Ok(value) => Self::Available { value },
            Err(error) => Self::Unavailable {
                message: error.to_string(),
            },
        }
    }
}

#[tauri::command]
fn open_project(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<DesktopSnapshot, String> {
    let snapshot = state.ops.open_project(&path)?;
    start_watcher(&app, &state, PathBuf::from(&snapshot.project_path))?;
    Ok(snapshot)
}

#[tauri::command]
fn snapshot(state: State<'_, AppState>) -> Result<DesktopSnapshot, String> {
    state.ops.snapshot()
}

#[tauri::command]
fn mutate(state: State<'_, AppState>, intent: DesktopIntent) -> Result<DesktopSnapshot, String> {
    state.ops.mutate(intent)
}

#[tauri::command]
fn capture_list(state: State<'_, AppState>) -> Result<Vec<Capture>, String> {
    state.ops.capture_list()
}

#[tauri::command]
fn capture_append(
    state: State<'_, AppState>,
    text: String,
) -> Result<CaptureAppendResponse, String> {
    state.ops.capture_append(&text)
}

#[tauri::command]
fn capture_delete(state: State<'_, AppState>, id: String) -> Result<Vec<Capture>, String> {
    state.ops.capture_delete(&id)
}

#[tauri::command]
fn capture_triage(
    state: State<'_, AppState>,
    id: String,
    column: Option<String>,
) -> Result<CaptureTriageResponse, String> {
    state.ops.capture_triage(&id, column.as_deref())
}

#[tauri::command]
fn scan_repository(
    state: State<'_, AppState>,
    markdown: bool,
    comments: bool,
    import_into: Option<String>,
) -> Result<ops::ScanPreview, String> {
    state
        .ops
        .scan_repository(markdown, comments, import_into.as_deref())
}

#[tauri::command]
fn apply_rewrite(
    state: State<'_, AppState>,
    proposed: String,
    base_hash: String,
    op: String,
) -> Result<DesktopSnapshot, String> {
    state.ops.apply_rewrite(&proposed, &base_hash, &op)
}

#[tauri::command]
fn history_list(state: State<'_, AppState>) -> Result<Vec<todo_core::HistoryEntry>, String> {
    state.ops.history_list()
}

#[tauri::command]
fn history_preview(
    state: State<'_, AppState>,
    version: u64,
) -> Result<ops::ProposedRewrite, String> {
    state.ops.history_preview(version)
}

#[tauri::command]
fn alignment_prompt(state: State<'_, AppState>) -> String {
    state.ops.alignment_prompt()
}

#[tauri::command]
fn alignment_review(
    state: State<'_, AppState>,
    raw: String,
) -> Result<todo_core::AlignmentReview, String> {
    state.ops.alignment_review(&raw)
}

#[tauri::command]
fn desktop_status(state: State<'_, AppState>) -> Result<DesktopStatus, String> {
    let status = state
        .status
        .lock()
        .map(|status| status.clone())
        .map_err(|_| "desktop status lock is poisoned".to_string())?;
    #[cfg(debug_assertions)]
    eprintln!(
        "desktop_status startup_ms={:?} shortcut_status={:?}",
        status.startup_ms, status.shortcut_status
    );
    Ok(status)
}

fn show_capture_window(app: &tauri::AppHandle) -> Result<u64, String> {
    let started = Instant::now();
    let window = app
        .get_webview_window("capture")
        .ok_or_else(|| "capture window is unavailable".to_string())?;
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())?;
    Ok(started.elapsed().as_micros().min(u64::MAX as u128) as u64)
}

fn start_watcher(
    app: &tauri::AppHandle,
    state: &State<'_, AppState>,
    path: PathBuf,
) -> Result<(), String> {
    let backend = state.ops.backend.clone();
    let handle = app.clone();
    let watcher =
        TargetFileWatcher::spawn(
            path,
            Duration::from_millis(100),
            move |signal| match signal {
                WatchSignal::Reload => match backend
                    .lock()
                    .map_err(|_| "desktop backend lock is poisoned".to_string())
                    .and_then(|backend| backend.snapshot().map_err(|error| error.to_string()))
                {
                    Ok(snapshot) => {
                        let _ = handle.emit("desktop://snapshot-changed", snapshot);
                    }
                    Err(error) => {
                        let _ = handle.emit("desktop://snapshot-error", error);
                    }
                },
                WatchSignal::Error(error) => {
                    let _ = handle.emit(
                        "desktop://snapshot-error",
                        format!("TODO.md watcher error: {error}"),
                    );
                }
            },
        )
        .map_err(|error| format!("failed to watch TODO.md parent directory: {error}"))?;
    *state
        .watcher
        .lock()
        .map_err(|_| "desktop watcher lock is poisoned".to_string())? = Some(watcher);
    Ok(())
}

fn status_after_setup(elapsed: Duration, shortcut_result: Result<(), String>) -> DesktopStatus {
    DesktopStatus {
        startup_ms: Some(elapsed.as_millis().min(u64::MAX as u128) as u64),
        shortcut_status: match shortcut_result {
            Ok(()) => ShortcutStatus::Registered,
            Err(message) => ShortcutStatus::Error { message },
        },
    }
}

fn runtime_capture_store(lock_timeout: Duration) -> todo_core::Result<CaptureStore> {
    #[cfg(debug_assertions)]
    if let Some(path) = std::env::var_os("TODO_DESKTOP_TEST_CAPTURE_PATH") {
        return Ok(CaptureStore::new(PathBuf::from(path), lock_timeout));
    }
    CaptureStore::global(lock_timeout)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let started = Instant::now();
    let capture_store = runtime_capture_store(Duration::from_secs(2))
        .expect("could not resolve the global Capture inbox path");
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        match show_capture_window(app) {
                            Ok(elapsed_us) => {
                                #[cfg(debug_assertions)]
                                eprintln!("capture_shortcut_dispatch_us={elapsed_us}");
                            }
                            Err(error) => {
                                let _ = app.emit("desktop://shortcut-error", error);
                            }
                        }
                    }
                })
                .build(),
        )
        .manage(AppState {
            ops: ops::DesktopOps::new(DesktopBackend::default(), capture_store),
            watcher: Mutex::new(None),
            status: Mutex::new(DesktopStatus {
                startup_ms: None,
                shortcut_status: ShortcutStatus::Pending,
            }),
        })
        .invoke_handler(tauri::generate_handler![
            open_project,
            snapshot,
            mutate,
            capture_list,
            capture_append,
            capture_delete,
            capture_triage,
            scan_repository,
            apply_rewrite,
            history_list,
            history_preview,
            alignment_prompt,
            alignment_review,
            desktop_status
        ])
        .setup(move |app| {
            let shortcut = if cfg!(target_os = "macos") {
                Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::Space)
            } else {
                Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space)
            };
            let registration = app
                .global_shortcut()
                .register(shortcut)
                .map_err(|error| error.to_string());
            if let Err(error) = &registration {
                let _ = app.emit("desktop://shortcut-error", error.clone());
            }
            let status = status_after_setup(started.elapsed(), registration);
            let startup_ms = status.startup_ms.unwrap_or_default();
            *app.state::<AppState>()
                .status
                .lock()
                .map_err(|_| std::io::Error::other("desktop status lock is poisoned"))? = status;
            let _ = app.emit("desktop://ready", startup_ms);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("desktop runtime failed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_status_persists_success_and_error_details() {
        assert_eq!(
            status_after_setup(Duration::from_millis(37), Ok(())),
            DesktopStatus {
                startup_ms: Some(37),
                shortcut_status: ShortcutStatus::Registered,
            }
        );
        let failed = status_after_setup(
            Duration::from_millis(41),
            Err("shortcut already registered".into()),
        );
        assert_eq!(failed.startup_ms, Some(41));
        assert_eq!(
            failed.shortcut_status,
            ShortcutStatus::Error {
                message: "shortcut already registered".into()
            }
        );
        let serialized = serde_json::to_value(failed).unwrap();
        assert_eq!(serialized["startup_ms"], 41);
        assert_eq!(serialized["shortcut_status"]["status"], "error");
    }
}
