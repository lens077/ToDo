//! Workspace registry commands: a thin layer over `todo_core::WorkspaceStore`.
//!
//! Opening a project by registry ID lives in `lib.rs` because it needs the
//! watcher and preferences plumbing of `AppState`.

use tauri::State;
use todo_core::{
    AddedProject, EnvironmentProfile, ProjectProfile, WorkspaceStore, WorkspaceSummary,
};

/// `TODO_WORKSPACE_STORE` overrides the global directory so tests and the
/// desktop can share a fixture, the same convention the AI feed uses.
pub fn runtime_workspace_store() -> Option<WorkspaceStore> {
    if let Some(path) = std::env::var_os("TODO_WORKSPACE_STORE").filter(|value| !value.is_empty()) {
        return Some(WorkspaceStore::new(std::path::PathBuf::from(path)));
    }
    match WorkspaceStore::global() {
        Ok(store) => Some(store),
        Err(error) => {
            eprintln!("workspace store unavailable: {error}");
            None
        }
    }
}

pub struct WorkspaceState {
    pub store: Option<WorkspaceStore>,
}

impl WorkspaceState {
    fn store(&self) -> Result<WorkspaceStore, String> {
        self.store
            .clone()
            .ok_or_else(|| "工作空间注册表不可用：无法确定用户主目录".to_string())
    }
}

#[tauri::command]
pub fn workspace_list(state: State<'_, WorkspaceState>) -> Result<Vec<WorkspaceSummary>, String> {
    state
        .store()?
        .list_workspaces()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn workspace_create(
    state: State<'_, WorkspaceState>,
    name: String,
    description: String,
) -> Result<WorkspaceSummary, String> {
    state
        .store()?
        .create_workspace(&name, &description)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn workspace_rename(
    state: State<'_, WorkspaceState>,
    id: String,
    name: String,
) -> Result<WorkspaceSummary, String> {
    state
        .store()?
        .rename_workspace(&id, &name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn workspace_delete(state: State<'_, WorkspaceState>, id: String) -> Result<(), String> {
    state
        .store()?
        .delete_workspace(&id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn workspace_add_project(
    state: State<'_, WorkspaceState>,
    workspace_id: String,
    path: String,
) -> Result<AddedProject, String> {
    state
        .store()?
        .add_project(&workspace_id, std::path::Path::new(&path))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn workspace_remove_project(
    state: State<'_, WorkspaceState>,
    workspace_id: String,
    project_id: String,
) -> Result<(), String> {
    state
        .store()?
        .remove_project_from_workspace(&workspace_id, &project_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn workspace_relink_project(
    state: State<'_, WorkspaceState>,
    project_id: String,
    path: String,
) -> Result<todo_core::ProjectSummary, String> {
    state
        .store()?
        .relink_project(&project_id, std::path::Path::new(&path))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn workspace_delete_project(
    state: State<'_, WorkspaceState>,
    project_id: String,
) -> Result<(), String> {
    state
        .store()?
        .delete_project(&project_id)
        .map_err(|error| error.to_string())
}

// ── Project profile & environment (Phase 2) ─────────────────────────────────

#[tauri::command]
pub fn project_profile_get(
    state: State<'_, WorkspaceState>,
    project_id: String,
) -> Result<ProjectProfile, String> {
    state
        .store()?
        .project_profile(&project_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn project_profile_set(
    state: State<'_, WorkspaceState>,
    project_id: String,
    profile: ProjectProfile,
) -> Result<ProjectProfile, String> {
    state
        .store()?
        .set_project_profile(&project_id, &profile)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn environment_get(
    state: State<'_, WorkspaceState>,
    project_id: String,
) -> Result<EnvironmentProfile, String> {
    state
        .store()?
        .environment_profile(&project_id)
        .map_err(|error| error.to_string())
}

/// Runs the read-only environment scan at the project's registered root. The
/// scan writes only the observed column; declared values are untouched.
#[tauri::command]
pub fn environment_scan(
    state: State<'_, WorkspaceState>,
    project_id: String,
) -> Result<EnvironmentProfile, String> {
    let store = state.store()?;
    let root = store
        .list_projects()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|project| project.id == project_id)
        .map(|project| project.root_path)
        .ok_or_else(|| format!("注册项目不存在: {project_id}"))?;
    let observed = todo_core::scan_environment(std::path::Path::new(&root));
    store
        .record_environment_scan(&project_id, &observed)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn environment_set_declared(
    state: State<'_, WorkspaceState>,
    project_id: String,
    declared_json: String,
) -> Result<EnvironmentProfile, String> {
    state
        .store()?
        .set_declared_environment(&project_id, &declared_json)
        .map_err(|error| error.to_string())
}

// ── Knowledge documents (Phase 3) ────────────────────────────────────────────

#[tauri::command]
pub fn knowledge_scan(
    state: State<'_, WorkspaceState>,
    project_id: String,
) -> Result<todo_core::KnowledgeScanReport, String> {
    state
        .store()?
        .knowledge_scan(&project_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn knowledge_list(
    state: State<'_, WorkspaceState>,
    project_id: String,
) -> Result<Vec<todo_core::KnowledgeDocument>, String> {
    state
        .store()?
        .knowledge_list(&project_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn knowledge_confirm(
    state: State<'_, WorkspaceState>,
    document_id: String,
    logical_type: String,
) -> Result<todo_core::KnowledgeDocument, String> {
    state
        .store()?
        .knowledge_confirm(&document_id, &logical_type)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn knowledge_read(
    state: State<'_, WorkspaceState>,
    document_id: String,
) -> Result<todo_core::KnowledgeDocumentContent, String> {
    state
        .store()?
        .knowledge_read(&document_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn knowledge_search(
    state: State<'_, WorkspaceState>,
    project_id: String,
    query: String,
) -> Result<Vec<todo_core::KnowledgeSearchHit>, String> {
    state
        .store()?
        .knowledge_search(&project_id, &query)
        .map_err(|error| error.to_string())
}

// ── Sessions (Phase 4) ───────────────────────────────────────────────────────

/// Resolves the registered root for a project ID, shared by session commands.
fn session_root(store: &WorkspaceStore, project_id: &str) -> Result<std::path::PathBuf, String> {
    store
        .list_projects()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|project| project.id == project_id)
        .map(|project| std::path::PathBuf::from(project.root_path))
        .ok_or_else(|| format!("注册项目不存在: {project_id}"))
}

#[tauri::command]
pub fn session_begin(
    state: State<'_, WorkspaceState>,
    project_id: String,
    goal: String,
) -> Result<todo_core::SessionSummary, String> {
    let root = session_root(&state.store()?, &project_id)?;
    todo_core::session_begin(&root, &goal).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn session_finish(
    state: State<'_, WorkspaceState>,
    project_id: String,
    input: todo_core::SessionFinishInput,
) -> Result<todo_core::SessionSummary, String> {
    let root = session_root(&state.store()?, &project_id)?;
    todo_core::session_finish(&root, &input).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn session_interrupt(
    state: State<'_, WorkspaceState>,
    project_id: String,
) -> Result<todo_core::SessionSummary, String> {
    let root = session_root(&state.store()?, &project_id)?;
    todo_core::session_interrupt(&root).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn session_list(
    state: State<'_, WorkspaceState>,
    project_id: String,
) -> Result<Vec<todo_core::SessionSummary>, String> {
    let root = session_root(&state.store()?, &project_id)?;
    todo_core::session_list(&root).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn session_resume(
    state: State<'_, WorkspaceState>,
    project_id: String,
) -> Result<todo_core::SessionResume, String> {
    let root = session_root(&state.store()?, &project_id)?;
    todo_core::session_resume(&root).map_err(|error| error.to_string())
}

// ── Overview (Phase 7) ───────────────────────────────────────────────────────

#[tauri::command]
pub fn overview_summary(
    state: State<'_, WorkspaceState>,
) -> Result<todo_core::OverviewReport, String> {
    state.store()?.overview().map_err(|error| error.to_string())
}
