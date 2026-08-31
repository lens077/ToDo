//! Command operations, free of Tauri's `State` wrapper.
//!
//! The `#[tauri::command]` functions in `lib.rs` are thin adapters over these,
//! so integration tests can drive the exact code the desktop runs without
//! standing up a webview. Keeping one implementation here is what makes the
//! contract test meaningful: there is no second code path that could drift.

use std::sync::{Arc, Mutex};

use serde::Serialize;
use todo_core::{Capture, CaptureStore, DesktopBackend, DesktopIntent, DesktopSnapshot};

use crate::{CaptureAppendResponse, CaptureTriageResponse, CommandValue};

/// The shared, lockable desktop state. `AppState` in `lib.rs` wraps this.
pub struct DesktopOps {
    pub backend: Arc<Mutex<DesktopBackend>>,
    pub capture_store: CaptureStore,
}

impl DesktopOps {
    pub fn new(backend: DesktopBackend, capture_store: CaptureStore) -> Self {
        Self {
            backend: Arc::new(Mutex::new(backend)),
            capture_store,
        }
    }

    fn backend(&self) -> std::result::Result<std::sync::MutexGuard<'_, DesktopBackend>, String> {
        self.backend
            .lock()
            .map_err(|_| "desktop backend lock is poisoned".to_string())
    }

    pub fn open_project(&self, path: &str) -> std::result::Result<DesktopSnapshot, String> {
        self.backend()?
            .open_project(path)
            .map_err(|error| error.to_string())
    }

    pub fn snapshot(&self) -> std::result::Result<DesktopSnapshot, String> {
        self.backend()?.snapshot().map_err(stringify)
    }

    pub fn mutate(&self, intent: DesktopIntent) -> std::result::Result<DesktopSnapshot, String> {
        self.backend()?.mutate(intent).map_err(stringify)
    }

    pub fn capture_list(&self) -> std::result::Result<Vec<Capture>, String> {
        self.capture_store.list().map_err(stringify)
    }

    pub fn capture_append(&self, text: &str) -> std::result::Result<CaptureAppendResponse, String> {
        let (capture, captures) = self
            .capture_store
            .append_with_snapshot(text)
            .map_err(stringify)?;
        Ok(CaptureAppendResponse { capture, captures })
    }

    pub fn capture_delete(&self, id: &str) -> std::result::Result<Vec<Capture>, String> {
        self.capture_store.delete(id).map_err(stringify)
    }

    /// Triage is deliberately not atomic across the project file and the personal
    /// inbox: the task is created first, the Capture deleted second. A partial
    /// success must still report the created `task_id` so the user does not
    /// create the task twice.
    pub fn capture_triage(
        &self,
        id: &str,
        column: Option<&str>,
    ) -> std::result::Result<CaptureTriageResponse, String> {
        let (result, snapshot) = {
            let mut backend = self.backend()?;
            let result = self
                .capture_store
                .triage_to_task(id, &mut *backend, column)
                .map_err(stringify)?;
            let snapshot = CommandValue::from_result(backend.snapshot());
            (result, snapshot)
        };
        let captures = CommandValue::from_result(self.capture_store.list());
        Ok(CaptureTriageResponse {
            result,
            snapshot,
            captures,
        })
    }
}

/// A proposed whole-file rewrite, awaiting confirmation.
///
/// `base_hash` pins the document the diff was computed against. Applying a
/// proposal built from a file that has since changed would write a rewrite the
/// user never actually saw, so the hash is checked again at apply time.
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

impl DesktopOps {
    fn project(&self) -> std::result::Result<std::path::PathBuf, String> {
        self.backend()?
            .project_path()
            .map(std::path::Path::to_path_buf)
            .ok_or_else(|| "no project is open".to_string())
    }

    fn document(&self) -> std::result::Result<(std::path::PathBuf, todo_core::Document), String> {
        let path = self.project()?;
        let document = todo_core::parse_file(&path).map_err(stringify)?;
        Ok((path, document))
    }

    /// Scans the repository and, when `import_into` is set, also prepares the
    /// rewrite that importing would produce. Nothing is written either way.
    pub fn scan_repository(
        &self,
        markdown: bool,
        comments: bool,
        import_into: Option<&str>,
    ) -> std::result::Result<ScanPreview, String> {
        let (path, document) = self.document()?;
        let root = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| std::path::Path::new("."));
        let options = todo_core::ScanOptions {
            markdown,
            comments,
            task_file: path.clone(),
        };
        let report = todo_core::scan(root, &document, &options).map_err(stringify)?;

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

    /// Writes a previously previewed rewrite, refusing if the file moved on.
    pub fn apply_rewrite(
        &self,
        proposed: &str,
        base_hash: &str,
        op: &str,
    ) -> std::result::Result<DesktopSnapshot, String> {
        let (path, document) = self.document()?;
        if content_hash(&document.source) != base_hash {
            return Err(
                "the file changed since this preview was generated; review the new diff and retry"
                    .into(),
            );
        }
        let writer = todo_core::LockedWriter::new(&path, std::time::Duration::from_millis(2000));
        writer.replace_all(op, proposed).map_err(stringify)?;
        self.snapshot()
    }

    pub fn history_list(&self) -> std::result::Result<Vec<todo_core::HistoryEntry>, String> {
        let path = self.project()?;
        let mut entries = todo_core::History::for_file(&path)
            .entries()
            .map_err(stringify)?;
        entries.reverse();
        Ok(entries)
    }

    /// Diff of a restore point (left) against the current file (right).
    pub fn history_preview(&self, version: u64) -> std::result::Result<ProposedRewrite, String> {
        let (path, document) = self.document()?;
        let previous = todo_core::History::for_file(&path)
            .read(version)
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

    pub fn alignment_review(
        &self,
        raw: &str,
    ) -> std::result::Result<todo_core::AlignmentReview, String> {
        let (_, document) = self.document()?;
        todo_core::review_alignment(&document, raw).map_err(stringify)
    }
}

/// Stable content fingerprint used to detect that a previewed base has moved on.
fn content_hash(source: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn stringify<E: std::fmt::Display>(error: E) -> String {
    error.to_string()
}

/// Serializes exactly as the Tauri IPC boundary does, so tests assert the JSON
/// the TypeScript bridge actually receives rather than Rust-side field names.
pub fn wire<T: Serialize>(value: &T) -> serde_json::Value {
    serde_json::to_value(value).expect("command responses must serialize")
}
