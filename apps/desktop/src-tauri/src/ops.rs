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

fn stringify<E: std::fmt::Display>(error: E) -> String {
    error.to_string()
}

/// Serializes exactly as the Tauri IPC boundary does, so tests assert the JSON
/// the TypeScript bridge actually receives rather than Rust-side field names.
pub fn wire<T: Serialize>(value: &T) -> serde_json::Value {
    serde_json::to_value(value).expect("command responses must serialize")
}
