//! Testable command operations for a project root and its task sources.
use crate::{CaptureAppendResponse, CaptureTriageResponse, CommandValue};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use todo_core::{
    Capture, CaptureStore, DesktopBackend, DesktopIntent, ProjectBackend, ProjectSnapshot,
};

pub struct DesktopOps {
    pub backend: Arc<Mutex<ProjectBackend>>,
    pub capture_store: CaptureStore,
}
impl DesktopOps {
    pub fn new(backend: DesktopBackend, capture_store: CaptureStore) -> Self {
        Self {
            backend: Arc::new(Mutex::new(ProjectBackend::from_backend(backend))),
            capture_store,
        }
    }
    fn backend(&self) -> Result<std::sync::MutexGuard<'_, ProjectBackend>, String> {
        self.backend
            .lock()
            .map_err(|_| "desktop backend lock is poisoned".to_string())
    }
    pub fn open_project(&self, path: &str) -> Result<ProjectSnapshot, String> {
        self.backend()?.open_project(path).map_err(stringify)
    }
    pub fn snapshot(&self) -> Result<ProjectSnapshot, String> {
        self.backend()?.snapshot().map_err(stringify)
    }
    pub fn project_snapshot(&self, source: Option<&str>) -> Result<ProjectSnapshot, String> {
        let backend = self.backend()?;
        match source {
            Some(source) => backend.snapshot_source(source),
            None => backend.snapshot(),
        }
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
        self.backend()?.preview_adopt(source).map_err(stringify)
    }
    pub fn apply_adopt(&self, source: &str, base_hash: &str) -> Result<ProjectSnapshot, String> {
        self.apply_adopt_for(source, base_hash, None)
    }
    pub fn apply_adopt_for(
        &self,
        source: &str,
        base_hash: &str,
        expected_root: Option<&str>,
    ) -> Result<ProjectSnapshot, String> {
        let backend = self.backend()?;
        if expected_root
            .is_some_and(|root| backend.project_path() != Some(std::path::Path::new(root)))
        {
            return Err("project changed before adoption; review the current source again".into());
        }
        backend.apply_adopt(source, base_hash).map_err(stringify)
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
