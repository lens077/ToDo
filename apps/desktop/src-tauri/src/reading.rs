//! 阅读 commands: a thin layer over `todo_core::ReadingStore`.
//!
//! Fetching a page blocks on the network, so `reading_add` runs on the
//! blocking pool; every other call is a local SQLite read or write.

use crate::sync;
use crate::workspace::WorkspaceState;
use serde::{Deserialize, Serialize};
use std::sync::{Condvar, Mutex, OnceLock};
use tauri::{AppHandle, Emitter, State};
use todo_core::{
    ReadingAnnotation, ReadingAnnotationInput, ReadingArticle, ReadingArticleDetail,
    ReadingExportFormat, ReadingStore,
};

/// `TODO_READING_STORE` overrides the global directory so tests and the
/// desktop can share a fixture, the same convention the AI feed uses.
///
/// `dirs::home_dir()` is `None` inside the Android sandbox, so on mobile we
/// fall back to the app-private data dir: saving a link needs no project or
/// user-chosen folder, it just lands in the app's own store.
pub fn runtime_reading_store() -> Option<ReadingStore> {
    if let Some(path) = std::env::var_os("TODO_READING_STORE").filter(|value| !value.is_empty()) {
        return Some(ReadingStore::new(std::path::PathBuf::from(path)));
    }
    match ReadingStore::global() {
        Ok(store) => Some(store),
        Err(error) => {
            #[cfg(mobile)]
            if let Some(dir) = crate::mobile_data_dir() {
                return Some(ReadingStore::new(dir.join(".todo").join("reading")));
            }
            eprintln!("reading store unavailable: {error}");
            None
        }
    }
}

pub struct ReadingState {
    pub store: Option<ReadingStore>,
}

impl ReadingState {
    fn store(&self) -> Result<ReadingStore, String> {
        self.store
            .clone()
            .ok_or_else(|| "阅读库不可用：无法确定用户主目录".to_string())
    }
}

#[tauri::command]
pub fn reading_list(state: State<'_, ReadingState>) -> Result<Vec<ReadingArticle>, String> {
    state.store()?.list().map_err(|error| error.to_string())
}

/// How many pages are fetched and extracted at the same time. Batch imports
/// queue everything at once; this keeps the phone and the sites from being
/// hammered while every row already shows as「解析中」.
const MAX_PARSE_WORKERS: usize = 3;

static PARSE_GATE: OnceLock<(Mutex<usize>, Condvar)> = OnceLock::new();

fn parse_gate() -> &'static (Mutex<usize>, Condvar) {
    PARSE_GATE.get_or_init(|| (Mutex::new(0), Condvar::new()))
}

fn acquire_slot() {
    let (lock, cond) = parse_gate();
    let mut active = match lock.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    while *active >= MAX_PARSE_WORKERS {
        active = match cond.wait(active) {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
    }
    *active += 1;
}

fn release_slot() {
    let (lock, cond) = parse_gate();
    let mut active = match lock.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    *active = active.saturating_sub(1);
    cond.notify_one();
}

#[derive(Clone, Serialize)]
pub struct ParsedEvent {
    pub id: String,
    /// `ok` or `failed`.
    pub status: &'static str,
    pub error: Option<String>,
}

/// Fetches the body off the UI thread and reports the outcome. The row is
/// already in the library, so a failure only flips its badge — the link stays.
fn spawn_parse(app: AppHandle, store: ReadingStore, id: String) {
    tauri::async_runtime::spawn_blocking(move || {
        acquire_slot();
        let payload = match store.refetch(&id) {
            Ok(_) => ParsedEvent {
                id: id.clone(),
                status: "ok",
                error: None,
            },
            Err(error) => {
                let message = error.to_string();
                eprintln!("reading parse failed: {id}: {message}");
                let _ = store.mark_failed(&id, &message);
                ParsedEvent {
                    id: id.clone(),
                    status: "failed",
                    error: Some(message),
                }
            }
        };
        release_slot();
        let _ = app.emit("reading://parsed", payload);
    });
}

/// 读取系统剪贴板纯文本。Android 只允许前台应用读剪贴板（我们正好在前台），
/// 权限被拒或内容不是文本时返回 None，不打断阅读视图。
#[tauri::command]
pub fn reading_clipboard_text(app: AppHandle) -> Option<String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    app.clipboard().read_text().ok()
}

/// 收藏优先：先把链接放进列表（毫秒级，`pending`），正文在后台抓取。
/// 连续添加不会被上一次的抓取阻塞。
#[tauri::command]
pub fn reading_quick_add(
    state: State<'_, ReadingState>,
    app: AppHandle,
    url: String,
) -> Result<ReadingArticle, String> {
    let store = state.store()?;
    let detail = store.add_pending(&url).map_err(|error| error.to_string())?;
    if detail.article.parse_status == "pending" {
        spawn_parse(app, store, detail.article.id.clone());
    }
    Ok(detail.article)
}

/// Retries a failed parse: the row goes back to「解析中」immediately.
#[tauri::command]
pub fn reading_retry_parse(
    state: State<'_, ReadingState>,
    app: AppHandle,
    id: String,
) -> Result<ReadingArticle, String> {
    let store = state.store()?;
    store.mark_pending(&id).map_err(|error| error.to_string())?;
    let article = store.get(&id).map_err(|error| error.to_string())?.article;
    spawn_parse(app, store, id);
    Ok(article)
}

/// Resumes parses that an app restart cut short, plus failed ones whose
/// backoff has elapsed. Called when the reading view mounts, so「解析中」never
/// sticks forever and a dead link is retried at most [`todo_core::MAX_PARSE_ATTEMPTS`]
/// times instead of on every sync.
#[tauri::command]
pub fn reading_resume_pending(
    state: State<'_, ReadingState>,
    app: AppHandle,
) -> Result<u32, String> {
    /// 启动时最多补跑几条失败解析：退避到期不等于要立刻全部重来。
    const RETRY_BUDGET: usize = 3;

    let store = state.store()?;
    let mut ids = store.pending_ids().map_err(|error| error.to_string())?;
    let due = store
        .due_retry_ids(RETRY_BUDGET)
        .map_err(|error| error.to_string())?;
    for id in due {
        match store.mark_retrying(&id) {
            Ok(()) => ids.push(id),
            Err(error) => eprintln!("reading retry mark failed: {id}: {error}"),
        }
    }
    let count = ids.len() as u32;
    for id in ids {
        spawn_parse(app.clone(), store.clone(), id);
    }
    Ok(count)
}

#[tauri::command]
pub async fn reading_import_browser(
    state: State<'_, ReadingState>,
    url: String,
    html: String,
) -> Result<ReadingArticleDetail, String> {
    let store = state.store()?;
    tauri::async_runtime::spawn_blocking(move || store.add_browser_html(&url, &html))
        .await
        .map_err(|error| format!("浏览器导入任务中断：{error}"))?
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn reading_add(
    state: State<'_, ReadingState>,
    url: String,
) -> Result<ReadingArticleDetail, String> {
    let store = state.store()?;
    tauri::async_runtime::spawn_blocking(move || store.add(&url))
        .await
        .map_err(|error| format!("抓取任务中断：{error}"))?
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn reading_get(
    state: State<'_, ReadingState>,
    id: String,
) -> Result<ReadingArticleDetail, String> {
    let store = state.store()?;
    store.touch(&id).map_err(|error| error.to_string())?;
    store.get(&id).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn reading_progress(
    state: State<'_, ReadingState>,
    id: String,
    percent: u32,
    anchor: Option<String>,
) -> Result<ReadingArticle, String> {
    state
        .store()?
        .set_progress(&id, percent, anchor.as_deref())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn reading_set_read(
    state: State<'_, ReadingState>,
    id: String,
    read: bool,
) -> Result<ReadingArticle, String> {
    state
        .store()?
        .set_read(&id, read)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn reading_remove(state: State<'_, ReadingState>, id: String) -> Result<(), String> {
    state
        .store()?
        .remove(&id)
        .map_err(|error| error.to_string())
}

#[derive(Debug, Deserialize)]
pub struct AnnotationPayload {
    pub id: String,
    pub article_id: String,
    pub quote: String,
    pub start: u32,
    pub end: u32,
    #[serde(default)]
    pub note: String,
}

#[tauri::command]
pub fn reading_annotation_upsert(
    state: State<'_, ReadingState>,
    annotation: AnnotationPayload,
) -> Result<ReadingAnnotation, String> {
    state
        .store()?
        .upsert_annotation(ReadingAnnotationInput {
            id: annotation.id,
            article_id: annotation.article_id,
            quote: annotation.quote,
            start: annotation.start,
            end: annotation.end,
            note: annotation.note,
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn reading_annotation_delete(state: State<'_, ReadingState>, id: String) -> Result<(), String> {
    state
        .store()?
        .delete_annotation(&id)
        .map_err(|error| error.to_string())
}

// ── Reading Context (Phase 6) ────────────────────────────────────────────────

#[tauri::command]
pub fn reading_update_context(
    state: State<'_, ReadingState>,
    id: String,
    context: todo_core::ReadingContextInput,
) -> Result<ReadingArticleDetail, String> {
    state
        .store()?
        .update_context(&id, &context)
        .map_err(|error| error.to_string())
}

/// Re-fetches source metadata and body. User context is structurally
/// untouched: the Rust UPDATE never mentions those columns.
#[tauri::command]
pub async fn reading_refetch(
    state: State<'_, ReadingState>,
    id: String,
) -> Result<ReadingArticleDetail, String> {
    let store = state.store()?;
    tauri::async_runtime::spawn_blocking(move || store.refetch(&id))
        .await
        .map_err(|error| format!("重新解析任务中断：{error}"))?
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn reading_link_project(
    state: State<'_, ReadingState>,
    id: String,
    project_ref: String,
    label: String,
) -> Result<ReadingArticleDetail, String> {
    state
        .store()?
        .link_project(&id, &project_ref, &label)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn reading_unlink_project(
    state: State<'_, ReadingState>,
    id: String,
    project_ref: String,
) -> Result<ReadingArticleDetail, String> {
    state
        .store()?
        .unlink_project(&id, &project_ref)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn reading_link_task(
    state: State<'_, ReadingState>,
    id: String,
    task_ref: String,
    label: String,
) -> Result<ReadingArticleDetail, String> {
    state
        .store()?
        .link_task(&id, &task_ref, &label)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn reading_unlink_task(
    state: State<'_, ReadingState>,
    id: String,
    task_ref: String,
) -> Result<ReadingArticleDetail, String> {
    state
        .store()?
        .unlink_task(&id, &task_ref)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn reading_export_reference(
    state: State<'_, ReadingState>,
    id: String,
) -> Result<String, String> {
    state
        .store()?
        .export_reference(&id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn reading_export_project_index(
    _state: State<'_, WorkspaceState>,
    project_ref: String,
) -> Result<String, String> {
    // The index projection lives in the reading store; the project_ref is the
    // workspace registry ID, kept as an opaque string on the reading side.
    let reading = crate::reading::runtime_reading_store()
        .ok_or_else(|| "阅读库不可用：无法确定用户主目录".to_string())?;
    reading
        .export_project_index(&project_ref)
        .map_err(|error| error.to_string())
}

/// 另存为 HTML / Markdown：正文与图片写进用户选择的目录。远程兜底图的
/// 下载会走网络，放在 blocking pool，避免卡住 UI 线程。
#[tauri::command]
pub async fn reading_export_article(
    state: State<'_, ReadingState>,
    id: String,
    format: ReadingExportFormat,
    dir: String,
) -> Result<todo_core::ReadingExportResult, String> {
    if dir.trim().is_empty() {
        return Err("保存目录为空".to_string());
    }
    let store = state.store()?;
    let dir = std::path::PathBuf::from(dir);
    tauri::async_runtime::spawn_blocking(move || store.export_article(&id, format, &dir))
        .await
        .map_err(|error| format!("另存为任务中断：{error}"))?
        .map_err(|error| error.to_string())
}

/// 全量对账：本地阅读库 ↔ todo-sync 服务器。网络往返阻塞，走 blocking pool。
/// 进度经 `reading://sync-progress` 推给界面；同一时刻只允许一轮。
#[tauri::command]
pub async fn reading_sync_now(
    state: State<'_, ReadingState>,
    app: AppHandle,
    base: String,
    token: String,
) -> Result<sync::SyncReport, String> {
    let store = state.store()?;
    tauri::async_runtime::spawn_blocking(move || {
        sync::run_with_progress(&store, &base, &token, &|progress| {
            let _ = app.emit("reading://sync-progress", progress);
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

/// 请求取消正在进行的同步：当前这一篇处理完后停下，已完成的条目保留。
#[tauri::command]
pub fn reading_sync_cancel() -> bool {
    sync::cancel();
    true
}
