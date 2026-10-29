//! Testable command operations for a project root and its task sources.
use crate::{CaptureAppendResponse, CaptureTriageResponse, CommandValue};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use todo_core::{
    AiListFilter, AiRecord, AiStore, Capture, CaptureProject, CaptureStore, Change,
    ColumnSetupOptions, ColumnSetupPreview, ColumnSetupSelection, DesktopBackend, DesktopIntent,
    LockedWriter, ProjectBackend, ProjectSnapshot,
};

pub struct DesktopOps {
    pub backend: Arc<Mutex<ProjectBackend>>,
    pub capture_store: CaptureStore,
    /// The AI feed store (`docs/ai-feed-spec.md`). `None` when the home
    /// directory is unavailable; the AI view then reports that instead of
    /// pretending the feed is empty.
    ai_store: Option<AiStore>,
}
impl DesktopOps {
    pub fn new(backend: DesktopBackend, capture_store: CaptureStore) -> Self {
        Self {
            backend: Arc::new(Mutex::new(ProjectBackend::from_backend(backend))),
            capture_store,
            ai_store: None,
        }
    }
    pub fn with_ai_store(mut self, ai_store: AiStore) -> Self {
        self.ai_store = Some(ai_store);
        self
    }
    fn backend(&self) -> Result<std::sync::MutexGuard<'_, ProjectBackend>, String> {
        self.backend
            .lock()
            .map_err(|_| "desktop backend lock is poisoned".to_string())
    }
    fn ai_store(&self) -> Result<&AiStore, String> {
        self.ai_store
            .as_ref()
            .ok_or_else(|| "AI 会话存储不可用：无法确定主目录".to_string())
    }
    /// Every record the AI view may show, including `handled` and hidden ones;
    /// the view applies its own pills and drops hidden records. `connected`
    /// tells the "no host yet" empty state apart from "nothing right now".
    pub fn ai_list(&self, workspace: Option<&str>) -> Result<AiListResponse, String> {
        let store = self.ai_store()?;
        let records = store
            .list(&AiListFilter {
                workspace: workspace.map(PathBuf::from),
                states: Vec::new(),
                include_all: true,
            })
            .map_err(stringify)?;
        Ok(AiListResponse {
            records,
            connected: store.path().exists(),
        })
    }
    pub fn ai_ack(&self, source: &str, session: &str) -> Result<AiRecord, String> {
        self.ai_store()?.ack(source, session).map_err(stringify)
    }
    pub fn ai_remove(&self, source: &str, session: &str) -> Result<AiRecord, String> {
        self.ai_store()?.remove(source, session).map_err(stringify)
    }
    pub fn ai_link(&self, source: &str, session: &str, task_id: &str) -> Result<AiRecord, String> {
        self.ai_store()?
            .link(source, session, task_id)
            .map_err(stringify)
    }
    /// Creates a Project Task from a session and links the record to it.
    ///
    /// Three writes, in the order that leaves the least confusing state if one
    /// fails: the task line first (the user's intent), then the link (so a
    /// retry does not create a second task), then the `↳` note.
    pub fn ai_to_task(
        &self,
        source: &str,
        session: &str,
        column: Option<&str>,
        source_path: Option<&str>,
        expected_root: Option<&str>,
    ) -> Result<AiToTaskResponse, String> {
        let store = self.ai_store()?;
        let backend = self.backend()?;
        if expected_root
            .is_some_and(|root| backend.project_path() != Some(std::path::Path::new(root)))
        {
            return Err("project changed before this write; reload and retry".into());
        }
        let record = store.get(source, session).map_err(stringify)?;
        let mut target = backend.source_backend(source_path).map_err(stringify)?;
        let task_id = target
            .create_task(&ai_task_body(&record), column)
            .map_err(stringify)?;
        let record = store.link(source, session, &task_id).map_err(stringify)?;
        let path = target
            .project_path()
            .ok_or_else(|| "no project is open".to_string())?;
        write_ai_task_note(path, &task_id, &ai_task_note(&record))
            .map_err(|error| format!("任务 {task_id} 已创建并关联，但说明写入失败：{error}"))?;
        Ok(AiToTaskResponse {
            task_id,
            record,
            snapshot: CommandValue::from_result(backend.snapshot()),
        })
    }
    pub fn open_project(&self, path: &str) -> Result<ProjectSnapshot, String> {
        self.backend()?.open_project(path).map_err(stringify)
    }
    pub fn snapshot(&self) -> Result<ProjectSnapshot, String> {
        self.backend()?.snapshot().map_err(stringify)
    }
    pub fn project_snapshot(&self, source: Option<&str>) -> Result<ProjectSnapshot, String> {
        self.project_snapshot_loose(source, &[])
    }
    pub fn project_snapshot_loose(
        &self,
        source: Option<&str>,
        loose: &[String],
    ) -> Result<ProjectSnapshot, String> {
        self.backend()?
            .snapshot_with_loose(source, loose)
            .map_err(stringify)
    }
    pub fn mutate(&self, intent: DesktopIntent) -> Result<ProjectSnapshot, String> {
        self.project_mutate(None, intent)
    }
    pub fn project_mutate(
        &self,
        source: Option<&str>,
        intent: DesktopIntent,
    ) -> Result<ProjectSnapshot, String> {
        self.project_mutate_for(source, intent, None)
    }
    pub fn project_mutate_for(
        &self,
        source: Option<&str>,
        intent: DesktopIntent,
        expected_root: Option<&str>,
    ) -> Result<ProjectSnapshot, String> {
        let backend = self.backend()?;
        if expected_root
            .is_some_and(|root| backend.project_path() != Some(std::path::Path::new(root)))
        {
            return Err("project changed before this write; reload and retry".into());
        }
        backend.mutate(source, intent).map_err(stringify)
    }
    pub fn preview_adopt(&self, source: &str) -> Result<todo_core::AdoptionPreview, String> {
        self.preview_adopt_with(source, false)
    }
    pub fn preview_adopt_with(
        &self,
        source: &str,
        loose: bool,
    ) -> Result<todo_core::AdoptionPreview, String> {
        self.backend()?
            .preview_adopt_with(source, loose)
            .map_err(stringify)
    }
    pub fn apply_adopt(&self, source: &str, base_hash: &str) -> Result<ProjectSnapshot, String> {
        self.apply_adopt_for(source, base_hash, None, false)
    }
    pub fn apply_adopt_for(
        &self,
        source: &str,
        base_hash: &str,
        expected_root: Option<&str>,
        loose: bool,
    ) -> Result<ProjectSnapshot, String> {
        let backend = self.backend()?;
        if expected_root
            .is_some_and(|root| backend.project_path() != Some(std::path::Path::new(root)))
        {
            return Err("project changed before adoption; review the current source again".into());
        }
        backend
            .apply_adopt_with(source, base_hash, loose)
            .map_err(stringify)
    }
    /// Keep the project lock through the core operation: opening another root
    /// must not race between checking the dialog's binding and using its source.
    fn column_backend(
        &self,
        expected_root: &str,
    ) -> Result<std::sync::MutexGuard<'_, ProjectBackend>, String> {
        let backend = self.backend()?;
        if backend.project_path().and_then(|path| path.to_str()) != Some(expected_root) {
            return Err(
                "project changed before column setup; review the current source again".into(),
            );
        }
        Ok(backend)
    }
    pub fn column_setup_options(
        &self,
        source: &str,
        expected_root: &str,
    ) -> Result<ColumnSetupOptions, String> {
        self.column_backend(expected_root)?
            .column_setup_options(source)
            .map_err(stringify)
    }
    pub fn preview_columns(
        &self,
        source: &str,
        selection: &ColumnSetupSelection,
        expected_root: &str,
    ) -> Result<ColumnSetupPreview, String> {
        self.column_backend(expected_root)?
            .preview_columns(source, selection)
            .map_err(stringify)
    }
    pub fn apply_columns(
        &self,
        source: &str,
        base_hash: &str,
        selection: &ColumnSetupSelection,
        expected_root: &str,
    ) -> Result<ProjectSnapshot, String> {
        self.column_backend(expected_root)?
            .apply_columns(source, base_hash, selection)
            .map_err(stringify)
    }
    pub fn capture_list(&self) -> Result<Vec<Capture>, String> {
        self.capture_store.list().map_err(stringify)
    }
    pub fn capture_append(&self, text: &str) -> Result<CaptureAppendResponse, String> {
        let (capture, captures) = self
            .capture_store
            .append_with_snapshot(text)
            .map_err(stringify)?;
        Ok(CaptureAppendResponse { capture, captures })
    }
    pub fn capture_delete(&self, id: &str) -> Result<Vec<Capture>, String> {
        self.capture_store.delete(id).map_err(stringify)
    }
    pub fn capture_triage(
        &self,
        id: &str,
        column: Option<&str>,
    ) -> Result<CaptureTriageResponse, String> {
        self.capture_triage_source(id, column, None)
    }
    pub fn capture_triage_source(
        &self,
        id: &str,
        column: Option<&str>,
        source: Option<&str>,
    ) -> Result<CaptureTriageResponse, String> {
        self.capture_triage_for(id, column, source, None)
    }
    pub fn capture_triage_for(
        &self,
        id: &str,
        column: Option<&str>,
        source: Option<&str>,
        expected_root: Option<&str>,
    ) -> Result<CaptureTriageResponse, String> {
        let backend = self.backend()?;
        if expected_root
            .is_some_and(|root| backend.project_path() != Some(std::path::Path::new(root)))
        {
            return Err("project changed before triage; select the source again".into());
        }
        let mut target = backend.source_backend(source).map_err(stringify)?;
        let result = self
            .capture_store
            .triage_to_task(id, &mut target, column)
            .map_err(stringify)?;
        let snapshot = CommandValue::from_result(backend.snapshot());
        let captures = CommandValue::from_result(self.capture_store.list());
        Ok(CaptureTriageResponse {
            result,
            snapshot,
            captures,
        })
    }

    fn source_path(&self, source: Option<&str>) -> Result<PathBuf, String> {
        self.backend()?
            .source_backend(source)
            .map_err(stringify)?
            .project_path()
            .map(ToOwned::to_owned)
            .ok_or_else(|| "no project is open".into())
    }
    fn document(&self, source: Option<&str>) -> Result<(PathBuf, todo_core::Document), String> {
        let path = self.source_path(source)?;
        let document = todo_core::parse_file(&path).map_err(stringify)?;
        Ok((path, document))
    }
    /// Validates `.virgo/project.yaml` for the open project. Read-only and
    /// offline; the report carries the stable VPC rule IDs the UI shows.
    pub fn contract_check(&self) -> Result<todo_core::ContractCheckReport, String> {
        let root = self
            .backend()?
            .project_path()
            .map(ToOwned::to_owned)
            .ok_or("no project is open")?;
        match todo_core::load_contract(&root).map_err(stringify)? {
            Some((path, contract)) => Ok(todo_core::check_contract(
                Some(&path),
                Some(&contract),
                &root,
            )),
            None => Ok(todo_core::check_contract(None, None, &root)),
        }
    }
    pub fn scan_repository(
        &self,
        markdown: bool,
        comments: bool,
        import_into: Option<&str>,
    ) -> Result<ScanPreview, String> {
        self.scan_repository_source(markdown, comments, import_into, None)
    }
    pub fn scan_repository_source(
        &self,
        markdown: bool,
        comments: bool,
        import_into: Option<&str>,
        source: Option<&str>,
    ) -> Result<ScanPreview, String> {
        let (path, document) = self.document(source)?;
        let root = self
            .backend()?
            .project_path()
            .map(ToOwned::to_owned)
            .ok_or("no project is open")?;
        let options = todo_core::ScanOptions {
            markdown,
            comments,
            task_file: path,
        };
        let report = todo_core::scan(&root, &document, &options).map_err(stringify)?;
        let rewrite = match import_into {
            Some(column) if !report.is_empty() => {
                let proposed =
                    todo_core::import_text(&document, &report, column).map_err(stringify)?;
                Some(ProposedRewrite {
                    diff: todo_core::diff_lines(&document.source, &proposed),
                    base_hash: content_hash(&document.source),
                    proposed,
                })
            }
            _ => None,
        };
        Ok(ScanPreview { report, rewrite })
    }
    pub fn apply_rewrite(
        &self,
        proposed: &str,
        base_hash: &str,
        op: &str,
    ) -> Result<ProjectSnapshot, String> {
        self.apply_rewrite_source(proposed, base_hash, op, None)
    }
    pub fn apply_rewrite_source(
        &self,
        proposed: &str,
        base_hash: &str,
        op: &str,
        source: Option<&str>,
    ) -> Result<ProjectSnapshot, String> {
        self.apply_rewrite_for(proposed, base_hash, op, source, None)
    }
    pub fn apply_rewrite_for(
        &self,
        proposed: &str,
        base_hash: &str,
        op: &str,
        source: Option<&str>,
        expected_root: Option<&str>,
    ) -> Result<ProjectSnapshot, String> {
        let backend = self.backend()?;
        if expected_root
            .is_some_and(|root| backend.project_path() != Some(std::path::Path::new(root)))
        {
            return Err("project changed since rewrite preview; review the source again".into());
        }
        let target = backend.source_backend(source).map_err(stringify)?;
        let path = target.project_path().ok_or("no source selected")?;
        todo_core::LockedWriter::new(path, Duration::from_secs(2))
            .transform_labeled(op, |current| {
                if content_hash(&current.source) != base_hash {
                    return Err(todo_core::Error::ConcurrentModification);
                }
                Ok(proposed.to_string())
            })
            .map_err(stringify)?;
        backend.snapshot().map_err(stringify)
    }
    pub fn history_list(&self) -> Result<Vec<todo_core::HistoryEntry>, String> {
        self.history_list_source(None)
    }
    pub fn history_list_source(
        &self,
        source: Option<&str>,
    ) -> Result<Vec<todo_core::HistoryEntry>, String> {
        let path = self.source_path(source)?;
        let mut entries = todo_core::History::for_file(&path)
            .entries_for(&path)
            .map_err(stringify)?;
        entries.reverse();
        Ok(entries)
    }
    pub fn history_preview(&self, version: u64) -> Result<ProposedRewrite, String> {
        self.history_preview_source(version, None)
    }
    pub fn history_preview_source(
        &self,
        version: u64,
        source: Option<&str>,
    ) -> Result<ProposedRewrite, String> {
        let (path, document) = self.document(source)?;
        let previous = todo_core::History::for_file(&path)
            .read_for(&path, version)
            .map_err(stringify)?;
        Ok(ProposedRewrite {
            diff: todo_core::diff_lines(&previous, &document.source),
            base_hash: content_hash(&document.source),
            proposed: previous,
        })
    }
    pub fn alignment_prompt(&self) -> String {
        todo_core::ALIGNMENT_PROMPT.to_string()
    }
    pub fn alignment_review(&self, raw: &str) -> Result<todo_core::AlignmentReview, String> {
        self.alignment_review_source(raw, None)
    }
    pub fn alignment_review_source(
        &self,
        raw: &str,
        source: Option<&str>,
    ) -> Result<todo_core::AlignmentReview, String> {
        let (_, document) = self.document(source)?;
        todo_core::review_alignment(&document, raw).map_err(stringify)
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct ProposedRewrite {
    pub diff: todo_core::TextDiff,
    pub proposed: String,
    pub base_hash: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct ScanPreview {
    pub report: todo_core::ScanReport,
    pub rewrite: Option<ProposedRewrite>,
}
#[derive(Debug, Clone, Serialize)]
pub struct AiListResponse {
    pub records: Vec<AiRecord>,
    /// Whether a host has ever published: the store file exists on disk.
    pub connected: bool,
}
#[derive(Debug, Serialize)]
pub struct AiToTaskResponse {
    pub task_id: String,
    pub record: AiRecord,
    pub snapshot: CommandValue<ProjectSnapshot>,
}
/// One-line task body: the title, else the question, else the session ID.
/// A task line cannot hold newlines or comment markers, so both are removed.
pub fn ai_task_body(record: &AiRecord) -> String {
    let raw = record
        .title
        .as_deref()
        .filter(|text| !text.trim().is_empty())
        .or(record
            .question
            .as_deref()
            .filter(|text| !text.trim().is_empty()))
        .map(str::to_owned)
        .unwrap_or_else(|| format!("会话 {}", record.session_id));
    let mut body = raw
        .replace("<!--", "")
        .replace("-->", "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    const MAX_CHARS: usize = 200;
    if body.chars().count() > MAX_CHARS {
        body = body.chars().take(MAX_CHARS).collect::<String>() + "…";
    }
    body
}
/// The `↳` note that keeps the way back to the session.
pub fn ai_task_note(record: &AiRecord) -> String {
    let text = match record.open_url.as_deref() {
        Some(url) => format!("来自 {} 会话 {}：{url}", record.source, record.session_id),
        None => format!("来自 {} 会话 {}", record.source, record.session_id),
    };
    text.replace("<!--", "").replace("-->", "")
}
fn write_ai_task_note(path: &std::path::Path, task_id: &str, note: &str) -> todo_core::Result<()> {
    LockedWriter::new(path, Duration::from_secs(2)).update_labeled("ai-to-task", |document| {
        let task = document
            .tasks
            .iter()
            .position(|task| task.id.as_deref() == Some(task_id))
            .ok_or_else(|| todo_core::Error::TaskNotFound(task_id.into()))?;
        Ok(vec![Change::SetNote {
            task,
            text: note.to_string(),
        }])
    })?;
    Ok(())
}
fn content_hash(source: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
fn stringify<E: std::fmt::Display>(error: E) -> String {
    error.to_string()
}
pub fn wire<T: Serialize>(value: &T) -> serde_json::Value {
    serde_json::to_value(value).expect("command responses must serialize")
}
