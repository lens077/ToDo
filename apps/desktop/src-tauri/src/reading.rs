//! 阅读 commands: a thin layer over `todo_core::ReadingStore`.
//!
//! Fetching a page blocks on the network, so `reading_add` runs on the
//! blocking pool; every other call is a local SQLite read or write.

use crate::sync;
use crate::workspace::WorkspaceState;
use serde::{Deserialize, Serialize};
use std::sync::{Condvar, Mutex, OnceLock};
use tauri::{AppHandle, Emitter, State};
use todo_core::Capture;
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

/// 新增随记（统一收件箱 A′）：Markdown 直存即渲染，标题从正文派生，
/// 不走解析管线。删除复用 `reading_remove`。
#[tauri::command]
pub async fn reading_note_append(
    state: State<'_, ReadingState>,
    markdown: String,
) -> Result<ReadingArticleDetail, String> {
    let store = state.store()?;
    tauri::async_runtime::spawn_blocking(move || store.note_append(&markdown))
        .await
        .map_err(|error| format!("随记保存任务中断：{error}"))?
        .map_err(|error| error.to_string())
}

/// 编辑随记：整篇重渲染并刷新 updated_at（同步冲突裁决用）。
#[tauri::command]
pub fn reading_note_update(
    state: State<'_, ReadingState>,
    id: String,
    markdown: String,
) -> Result<ReadingArticleDetail, String> {
    state
        .store()?
        .note_update(&id, &markdown)
        .map_err(|error| error.to_string())
}

/// 应用内「打开文件」：dialog 选中的本地文件导入统一收件箱。
/// html/md → 文章（readability 提取，拒收则整页净化直通），txt → 随记。
#[tauri::command]
pub async fn reading_import_file(
    state: State<'_, ReadingState>,
    path: String,
) -> Result<ReadingArticleDetail, String> {
    let store = state.store()?;
    tauri::async_runtime::spawn_blocking(move || store.import_local_file(&path))
        .await
        .map_err(|error| format!("文件导入任务中断：{error}"))?
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

/// 另存为 HTML / Markdown：正文与图片写进用户选的目录下一个「包裹目录」
/// （正文 + img/ 同居一处）。`prefix` / `suffix` 是前端从设置解析出的命名
/// 前后缀（产品后缀 / 仅标题 / 自定义）；远程兜底图的下载会走网络，放在
/// blocking pool，避免卡住 UI 线程。
#[tauri::command]
pub async fn reading_export_article(
    state: State<'_, ReadingState>,
    id: String,
    format: ReadingExportFormat,
    dir: String,
    prefix: Option<String>,
    suffix: Option<String>,
) -> Result<todo_core::ReadingExportResult, String> {
    if dir.trim().is_empty() {
        return Err("保存目录为空".to_string());
    }
    let store = state.store()?;
    let dir = std::path::PathBuf::from(dir);
    let naming = todo_core::ReadingExportNaming {
        prefix: prefix.unwrap_or_default(),
        suffix: suffix.unwrap_or_default(),
    };
    tauri::async_runtime::spawn_blocking(move || store.export_article(&id, format, &dir, &naming))
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

// ── 随记分拣桥（统一收件箱 A′）────────────────────────────────────────
// CaptureView 分拣面板与全局速记窗的数据源从 inbox.md 切到阅读库随记
// （kind='note'）；Capture 结构保持不变，前端桥（desktop.ts）无需改动。

/// 收件箱随记 → 旧 Capture 形状（id/created_at/text 契约不变）。
fn note_to_capture(detail: &ReadingArticleDetail) -> Capture {
    let created_at = chrono::DateTime::from_timestamp_millis(detail.article.added_at)
        .unwrap_or_default()
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    Capture {
        id: detail.article.id.clone(),
        created_at,
        text: detail.markdown.clone().unwrap_or_default(),
    }
}

/// 分拣列表：只含未分拣的随记（saved_reason 带 triaged 前缀的留档条目隐藏）。
pub fn capture_notes(state: &ReadingState) -> Result<Vec<Capture>, String> {
    let notes = state
        .store()?
        .list_notes()
        .map_err(|error| error.to_string())?;
    Ok(notes
        .iter()
        .filter(|detail| !detail.context.saved_reason.starts_with("triaged"))
        .map(note_to_capture)
        .collect())
}

/// 全局速记 / 分拣面板新增：文本直存为阅读库随记。
pub fn capture_append_note(
    state: &ReadingState,
    text: &str,
) -> Result<(Capture, Vec<Capture>), String> {
    let store = state.store()?;
    let detail = store.note_append(text).map_err(|error| error.to_string())?;
    let capture = note_to_capture(&detail);
    let captures = capture_notes(state)?;
    Ok((capture, captures))
}

/// 删除随记（分拣面板删除按钮）。
pub fn capture_delete_note(state: &ReadingState, id: &str) -> Result<Vec<Capture>, String> {
    state
        .store()?
        .remove(id)
        .map_err(|error| error.to_string())?;
    capture_notes(state)
}

/// 分拣取稿：Markdown 即任务正文；非随记条目明确报错。
pub fn capture_note_text(state: &ReadingState, id: &str) -> Result<String, String> {
    let detail = state.store()?.get(id).map_err(|error| error.to_string())?;
    if detail.article.kind != "note" {
        return Err(format!("该条目不是随记: {id}"));
    }
    Ok(detail.markdown.unwrap_or_default())
}

pub fn mark_note_triaged(state: &ReadingState, id: &str, destination: &str) -> Result<(), String> {
    state
        .store()?
        .mark_note_triaged(id, destination)
        .map_err(|error| error.to_string())
}

/// inbox.md 一次性迁移（设计稿 §2）：逐条转为阅读库随记，成功后原文件
/// 改名 `inbox.md.migrated-<时间戳>` 留档。幂等：文件不存在返回 0 条。
pub fn migrate_capture_inbox(state: &ReadingState) -> Result<(usize, String), String> {
    let path = todo_core::global_capture_path().map_err(|error| error.to_string())?;
    migrate_capture_inbox_at(state, &path)
}

/// 指定路径的迁移主体：测试用临时目录，命令走全局路径。
pub fn migrate_capture_inbox_at(
    state: &ReadingState,
    path: &std::path::Path,
) -> Result<(usize, String), String> {
    if !path.exists() {
        return Ok((0, String::new()));
    }
    let items = todo_core::parse_capture_file(path).map_err(|error| error.to_string())?;
    let store = state.store()?;
    for item in &items {
        store
            .note_append_with_title(&item.text, None)
            .map_err(|error| error.to_string())?;
    }
    let archived = path.with_extension(format!(
        "md.migrated-{}",
        chrono::Utc::now().format("%Y%m%d-%H%M%S")
    ));
    std::fs::rename(path, &archived).map_err(|error| format!("迁移后改名失败：{error}"))?;
    Ok((items.len(), archived.to_string_lossy().into_owned()))
}
