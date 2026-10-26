//! 阅读 commands: a thin layer over `todo_core::ReadingStore`.
//!
//! Fetching a page blocks on the network, so `reading_add` runs on the
//! blocking pool; every other call is a local SQLite read or write.

use serde::Deserialize;
use tauri::State;
use todo_core::{
    ReadingAnnotation, ReadingAnnotationInput, ReadingArticle, ReadingArticleDetail, ReadingStore,
};

/// `TODO_READING_STORE` overrides the global directory so tests and the
/// desktop can share a fixture, the same convention the AI feed uses.
pub fn runtime_reading_store() -> Option<ReadingStore> {
    if let Some(path) = std::env::var_os("TODO_READING_STORE").filter(|value| !value.is_empty()) {
        return Some(ReadingStore::new(std::path::PathBuf::from(path)));
    }
    match ReadingStore::global() {
        Ok(store) => Some(store),
        Err(error) => {
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
