pub mod ops;
mod preferences;
mod watch;

use serde::Serialize;
use std::{
    fmt::Display,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager, State};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use todo_core::{
    AiRecord, AiStore, Capture, CaptureStore, DesktopBackend, DesktopIntent, ProjectSnapshot,
    TriageResult,
};
use watch::{TargetFileWatcher, WatchSignal};

struct AppState {
    ops: ops::DesktopOps,
    watcher: Mutex<Option<TargetFileWatcher>>,
    /// Watches the AI feed store so the AI view refreshes when the CLI writes.
    /// Independent of the project watcher: the store is global.
    ai_watcher: Mutex<Option<TargetFileWatcher>>,
    status: Mutex<DesktopStatus>,
    /// `None` when the home directory is unavailable; the desktop then simply
    /// forgets the project between launches.
    preferences_path: Option<PathBuf>,
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
    pub snapshot: CommandValue<ProjectSnapshot>,
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
) -> Result<ProjectSnapshot, String> {
    let snapshot = state.ops.open_project(&path)?;
    let project = PathBuf::from(&snapshot.project_path);
    start_watcher(&app, &state, project.clone())?;
    preferences::remember_project(state.preferences_path.as_deref(), &project);
    Ok(snapshot)
}

/// Reopens the project from the previous launch, if it is still a task file.
///
/// Failure is deliberately silent: a moved repository or a file that no longer
/// parses must not block startup. The user sees the ordinary "choose a project"
/// state and can pick again; the stale entry is replaced on the next open.
fn reopen_last_project(app: &tauri::AppHandle, state: &State<'_, AppState>) {
    let Some(path) = state.preferences_path.as_deref() else {
        return;
    };
    let Some(last) = preferences::read(path).last_project else {
        return;
    };
    let Some(last) = last.to_str() else { return };
    match state.ops.open_project(last) {
        Ok(snapshot) => {
            let _ = start_watcher(app, state, PathBuf::from(&snapshot.project_path));
        }
        #[cfg(debug_assertions)]
        Err(error) => eprintln!("last project {last} not reopened: {error}"),
        #[cfg(not(debug_assertions))]
        Err(_) => {}
    }
}

#[tauri::command]
fn snapshot(state: State<'_, AppState>) -> Result<ProjectSnapshot, String> {
    state.ops.snapshot()
}

#[tauri::command]
fn mutate(state: State<'_, AppState>, intent: DesktopIntent) -> Result<ProjectSnapshot, String> {
    state.ops.mutate(intent)
}

#[tauri::command]
fn project_snapshot(
    state: State<'_, AppState>,
    source_path: Option<String>,
    loose_sources: Option<Vec<String>>,
) -> Result<ProjectSnapshot, String> {
    state
        .ops
        .project_snapshot_loose(source_path.as_deref(), &loose_sources.unwrap_or_default())
}

#[tauri::command]
fn project_mutate(
    state: State<'_, AppState>,
    source_path: Option<String>,
    intent: DesktopIntent,
    project_path: Option<String>,
) -> Result<ProjectSnapshot, String> {
    state
        .ops
        .project_mutate_for(source_path.as_deref(), intent, project_path.as_deref())
}

#[tauri::command]
fn preview_adopt(
    state: State<'_, AppState>,
    source_path: String,
    loose: Option<bool>,
) -> Result<todo_core::AdoptionPreview, String> {
    state
        .ops
        .preview_adopt_with(&source_path, loose.unwrap_or(false))
}

#[tauri::command]
fn apply_adopt(
    state: State<'_, AppState>,
    source_path: String,
    base_hash: String,
    project_path: Option<String>,
    loose: Option<bool>,
) -> Result<ProjectSnapshot, String> {
    state.ops.apply_adopt_for(
        &source_path,
        &base_hash,
        project_path.as_deref(),
        loose.unwrap_or(false),
    )
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
    source_path: Option<String>,
    project_path: Option<String>,
) -> Result<CaptureTriageResponse, String> {
    state.ops.capture_triage_for(
        &id,
        column.as_deref(),
        source_path.as_deref(),
        project_path.as_deref(),
    )
}

#[tauri::command]
fn scan_repository(
    state: State<'_, AppState>,
    markdown: bool,
    comments: bool,
    import_into: Option<String>,
    source_path: Option<String>,
) -> Result<ops::ScanPreview, String> {
    state.ops.scan_repository_source(
        markdown,
        comments,
        import_into.as_deref(),
        source_path.as_deref(),
    )
}

#[tauri::command]
fn apply_rewrite(
    state: State<'_, AppState>,
    proposed: String,
    base_hash: String,
    op: String,
    source_path: Option<String>,
    project_path: Option<String>,
) -> Result<ProjectSnapshot, String> {
    state.ops.apply_rewrite_for(
        &proposed,
        &base_hash,
        &op,
        source_path.as_deref(),
        project_path.as_deref(),
    )
}

#[tauri::command]
fn history_list(
    state: State<'_, AppState>,
    source_path: Option<String>,
) -> Result<Vec<todo_core::HistoryEntry>, String> {
    state.ops.history_list_source(source_path.as_deref())
}

#[tauri::command]
fn history_preview(
    state: State<'_, AppState>,
    version: u64,
    source_path: Option<String>,
) -> Result<ops::ProposedRewrite, String> {
    state
        .ops
        .history_preview_source(version, source_path.as_deref())
}

#[tauri::command]
fn alignment_prompt(state: State<'_, AppState>) -> String {
    state.ops.alignment_prompt()
}

#[tauri::command]
fn alignment_review(
    state: State<'_, AppState>,
    raw: String,
    source_path: Option<String>,
) -> Result<todo_core::AlignmentReview, String> {
    state
        .ops
        .alignment_review_source(&raw, source_path.as_deref())
}

#[tauri::command]
fn ai_list(
    state: State<'_, AppState>,
    workspace: Option<String>,
) -> Result<ops::AiListResponse, String> {
    state.ops.ai_list(workspace.as_deref())
}

#[tauri::command]
fn ai_ack(state: State<'_, AppState>, source: String, session: String) -> Result<AiRecord, String> {
    state.ops.ai_ack(&source, &session)
}

#[tauri::command]
fn ai_remove(
    state: State<'_, AppState>,
    source: String,
    session: String,
) -> Result<AiRecord, String> {
    state.ops.ai_remove(&source, &session)
}

#[tauri::command]
fn ai_link(
    state: State<'_, AppState>,
    source: String,
    session: String,
    task_id: String,
) -> Result<AiRecord, String> {
    state.ops.ai_link(&source, &session, &task_id)
}

#[tauri::command]
fn ai_to_task(
    state: State<'_, AppState>,
    source: String,
    session: String,
    column: Option<String>,
    source_path: Option<String>,
    project_path: Option<String>,
) -> Result<ops::AiToTaskResponse, String> {
    state.ops.ai_to_task(
        &source,
        &session,
        column.as_deref(),
        source_path.as_deref(),
        project_path.as_deref(),
    )
}

/// Opens an `http`/`https` URL in the system browser.
///
/// Only web URLs are accepted: the AI feed contract restricts `open_url` to
/// those, and a `file:` or custom scheme handed to the OS opener would run
/// whatever is registered for it.
#[tauri::command]
fn open_external(url: String) -> Result<(), String> {
    open_web_url(&url)
}

fn open_web_url(url: &str) -> Result<(), String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("只能打开 http/https 链接".into());
    }
    let mut command = if cfg!(target_os = "macos") {
        let mut command = std::process::Command::new("open");
        command.arg(url);
        command
    } else if cfg!(target_os = "windows") {
        let mut command = std::process::Command::new("cmd");
        command.args(["/C", "start", "", url]);
        command
    } else {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(url);
        command
    };
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法打开浏览器：{error}"))
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

/// Watches the AI store's directory and tells the webview when it changes.
///
/// The directory may not exist before the first `todo ai upsert`; it is created
/// here so the watch can be registered now rather than polled for later. A
/// failure is reported once and the view then refreshes only on its own reads.
fn start_ai_watcher(app: &tauri::AppHandle, state: &State<'_, AppState>, store_path: &Path) {
    let Some(parent) = store_path.parent() else {
        return;
    };
    let Some(file_name) = store_path.file_name() else {
        return;
    };
    let watched = std::fs::create_dir_all(parent)
        .and_then(|_| std::fs::canonicalize(parent))
        .map(|parent| parent.join(file_name));
    let target = match watched {
        Ok(target) => target,
        Err(error) => {
            eprintln!(
                "AI store directory {} not watched: {error}",
                parent.display()
            );
            return;
        }
    };
    let handle = app.clone();
    let watcher =
        TargetFileWatcher::spawn(
            target,
            Duration::from_millis(150),
            move |signal| match signal {
                WatchSignal::Reload => {
                    let _ = handle.emit("desktop://ai-changed", ());
                }
                WatchSignal::Error(error) => eprintln!("AI store watcher error: {error}"),
            },
        );
    match watcher {
        Ok(watcher) => {
            if let Ok(mut slot) = state.ai_watcher.lock() {
                *slot = Some(watcher);
            }
        }
        Err(error) => eprintln!("AI store not watched: {error}"),
    }
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

/// The AI feed store: `TODO_AI_STORE` overrides the global path, the same
/// convention the CLI uses so tests and the desktop can share a fixture.
fn runtime_ai_store(lock_timeout: Duration) -> Option<AiStore> {
    if let Some(path) = std::env::var_os("TODO_AI_STORE").filter(|value| !value.is_empty()) {
        return Some(AiStore::new(PathBuf::from(path), lock_timeout));
    }
    match AiStore::global(lock_timeout) {
        Ok(store) => Some(store),
        Err(error) => {
            eprintln!("AI feed store unavailable: {error}");
            None
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let started = Instant::now();
    let capture_store = runtime_capture_store(Duration::from_secs(2))
        .expect("could not resolve the global Capture inbox path");
    let ai_store = runtime_ai_store(Duration::from_secs(2));
    let ai_store_path = ai_store.as_ref().map(|store| store.path().to_path_buf());
    let mut ops = ops::DesktopOps::new(DesktopBackend::default(), capture_store);
    if let Some(store) = ai_store {
        ops = ops.with_ai_store(store);
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        match show_capture_window(app) {
                            #[cfg(debug_assertions)]
                            Ok(elapsed_us) => {
                                eprintln!("capture_shortcut_dispatch_us={elapsed_us}");
                            }
                            #[cfg(not(debug_assertions))]
                            Ok(_) => {}
                            Err(error) => {
                                let _ = app.emit("desktop://shortcut-error", error);
                            }
                        }
                    }
                })
                .build(),
        )
        .manage(AppState {
            ops,
            watcher: Mutex::new(None),
            ai_watcher: Mutex::new(None),
            status: Mutex::new(DesktopStatus {
                startup_ms: None,
                shortcut_status: ShortcutStatus::Pending,
            }),
            preferences_path: preferences::default_path(),
        })
        .invoke_handler(tauri::generate_handler![
            open_project,
            snapshot,
            mutate,
            project_snapshot,
            project_mutate,
            preview_adopt,
            apply_adopt,
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
            ai_list,
            ai_ack,
            ai_remove,
            ai_link,
            ai_to_task,
            open_external,
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
            let handle = app.handle().clone();
            reopen_last_project(&handle, &app.state::<AppState>());
            if let Some(path) = &ai_store_path {
                start_ai_watcher(&handle, &app.state::<AppState>(), path);
            }
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
    fn main_webview_uses_html_drag_and_drop() {
        let config: tauri::Config =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let main = config
            .app
            .windows
            .iter()
            .find(|window| window.label == "main")
            .expect("main window must be configured");
        assert!(
            !main.drag_drop_enabled,
            "task cards use HTML drag-and-drop, not Tauri's native file-drop handler"
        );
    }

    #[test]
    fn only_web_urls_reach_the_system_opener() {
        for url in ["file:///etc/passwd", "javascript:alert(1)", "todo://x", ""] {
            assert!(open_web_url(url).is_err(), "{url:?} must be refused");
        }
    }

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
