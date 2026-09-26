//! HTTP view of a bound project root. Every write resolves a discovered source;
//! this server never accepts an arbitrary disk path or switches its root remotely.
use axum::{
    extract::{Path as AxumPath, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use todo_core::{
    diff_lines, import_text, parse_file, review_alignment, scan, AiListFilter, AiRecord, AiStore,
    CaptureProject, Change, ColumnSetupOptions, ColumnSetupPreview, ColumnSetupSelection,
    DesktopBackend, DesktopIntent, History, LockedWriter, MoveTarget, PriorityScope,
    ProjectBackend, ProjectSnapshot, ScanOptions,
};
use tower_http::{cors::CorsLayer, services::ServeDir};

#[derive(Clone)]
struct ServeState {
    path: PathBuf,
    lock_timeout: Duration,
    read_only: bool,
    /// The AI feed store (`docs/ai-feed-spec.md`); `None` when no home
    /// directory could be found, in which case the AI endpoints say so.
    ai_store: Option<AiStore>,
}
impl ServeState {
    fn backend(&self) -> Result<ProjectBackend, ApiError> {
        let mut backend = ProjectBackend::new(self.lock_timeout);
        backend.open_project(&self.path)?;
        Ok(backend)
    }
    fn ai_store(&self) -> Result<&AiStore, ApiError> {
        self.ai_store.as_ref().ok_or_else(|| ApiError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "AI feed store is unavailable: no home directory".into(),
        })
    }
    fn source(&self, source: Option<&str>) -> Result<DesktopBackend, ApiError> {
        Ok(self.backend()?.source_backend(source)?)
    }
    fn source_path(&self, source: Option<&str>) -> Result<PathBuf, ApiError> {
        self.source(source)?
            .project_path()
            .map(ToOwned::to_owned)
            .ok_or_else(|| todo_core::Error::NoProject.into())
    }
    fn writable(&self) -> Result<(), ApiError> {
        if self.read_only {
            return Err(ApiError {
                status: StatusCode::FORBIDDEN,
                message: "server is running in --read-only mode".into(),
            });
        }
        Ok(())
    }
}
struct ApiError {
    status: StatusCode,
    message: String,
}
impl From<todo_core::Error> for ApiError {
    fn from(error: todo_core::Error) -> Self {
        let status = match error {
            todo_core::Error::LockTimeout { .. } => StatusCode::SERVICE_UNAVAILABLE,
            todo_core::Error::ConcurrentModification => StatusCode::CONFLICT,
            todo_core::Error::TaskNotFound(_)
            | todo_core::Error::HistoryVersionNotFound(_)
            | todo_core::Error::AiSessionNotFound(_) => StatusCode::NOT_FOUND,
            todo_core::Error::NotTaskFile
            | todo_core::Error::NoProject
            | todo_core::Error::InvalidTaskLine(_)
            | todo_core::Error::InvalidTaskId(_)
            | todo_core::Error::InvalidAlignment(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Self {
            status,
            message: error.to_string(),
        }
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (
            self.status,
            Json(serde_json::json!({"ok":false,"error":self.message})),
        )
            .into_response()
    }
}
type ApiResult<T> = Result<Json<T>, ApiError>;
#[derive(Default, Deserialize)]
struct SourceQuery {
    source: Option<String>,
}

/// Where the AI feed lives: `TODO_AI_STORE` overrides the global path, the
/// same convention the CLI uses so tests never touch `~/.todo/ai/`.
fn runtime_ai_store(lock_timeout: Duration) -> Option<AiStore> {
    if let Some(path) = std::env::var_os("TODO_AI_STORE").filter(|value| !value.is_empty()) {
        return Some(AiStore::new(PathBuf::from(path), lock_timeout));
    }
    AiStore::global(lock_timeout).ok()
}

pub fn router(
    path: PathBuf,
    lock_timeout: Duration,
    read_only: bool,
    web_root: Option<PathBuf>,
) -> Router {
    router_with_ai_store(path, lock_timeout, read_only, web_root, None)
}

/// `router` with an explicit AI store path; `None` resolves it from the
/// environment. Tests use this to point the feed at a fixture file.
pub fn router_with_ai_store(
    path: PathBuf,
    lock_timeout: Duration,
    read_only: bool,
    web_root: Option<PathBuf>,
    ai_store: Option<PathBuf>,
) -> Router {
    let state = ServeState {
        path: path.canonicalize().unwrap_or(path),
        lock_timeout,
        read_only,
        ai_store: match ai_store {
            Some(path) => Some(AiStore::new(path, lock_timeout)),
            None => runtime_ai_store(lock_timeout),
        },
    };
    let api = Router::new()
        .route("/health", get(health))
        .route("/snapshot", get(snapshot))
        .route("/mutate", post(project_mutate))
        .route("/sources/adopt/preview", post(preview_adopt))
        .route("/sources/adopt", post(apply_adopt))
        .route(
            "/sources/columns",
            get(column_setup_options).post(apply_columns),
        )
        .route("/sources/columns/preview", post(preview_columns))
        .route("/tasks", post(create_task))
        .route("/tasks/{id}/completed", post(set_completed))
        .route("/tasks/{id}/progress", post(set_progress))
        .route("/tasks/{id}/fail", post(set_failed))
        .route("/tasks/{id}/priority", post(set_priority))
        .route("/tasks/{id}/move", post(move_task))
        .route("/scan", get(run_scan))
        .route("/scan/import", post(import_scan))
        .route("/history", get(history))
        .route("/history/{version}/diff", get(history_diff))
        .route("/history/{version}/restore", post(history_restore))
        .route("/align/prompt", get(align_prompt))
        .route("/align/review", post(align_review))
        .route("/ai/sessions", get(ai_sessions))
        .route("/ai/sessions/{source}/{id}/ack", post(ai_ack))
        .route("/ai/sessions/{source}/{id}/remove", post(ai_remove))
        .route("/ai/sessions/{source}/{id}/link", post(ai_link))
        .route("/ai/sessions/{source}/{id}/to-task", post(ai_to_task))
        .with_state(Arc::new(state))
        .layer(CorsLayer::permissive());
    match web_root {
        Some(root) => Router::new()
            .nest("/api/v1", api)
            .fallback_service(ServeDir::new(root)),
        None => Router::new().nest("/api/v1", api),
    }
}
pub async fn run(
    path: PathBuf,
    addr: SocketAddr,
    lock_timeout: Duration,
    read_only: bool,
    web_root: Option<PathBuf>,
) -> Result<(), String> {
    let app = router(path.clone(), lock_timeout, read_only, web_root);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("could not bind {addr}: {e}"))?;
    let bound = listener.local_addr().map_err(|e| e.to_string())?;
    eprintln!(
        "todo serve → http://{bound}/api/v1  (project root: {})",
        path.display()
    );
    if read_only {
        eprintln!("read-only: mutating endpoints return 403");
    }
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .map_err(|e| e.to_string())
}
async fn health(State(state): State<Arc<ServeState>>) -> Json<serde_json::Value> {
    Json(
        serde_json::json!({"ok":true,"read_only":state.read_only,"project":state.path.display().to_string(),"project_sources":true,"captures":false}),
    )
}
#[derive(Deserialize)]
struct SnapshotQuery {
    source: Option<String>,
    /// Sources to read with the loose list reader (Beta); newline-separated
    /// so paths may contain any other character. Read-only in memory.
    loose: Option<String>,
}
async fn snapshot(
    State(state): State<Arc<ServeState>>,
    Query(query): Query<SnapshotQuery>,
) -> ApiResult<ProjectSnapshot> {
    let backend = state.backend()?;
    let loose: Vec<String> = query
        .loose
        .as_deref()
        .unwrap_or("")
        .split('\n')
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect();
    Ok(Json(
        backend.snapshot_with_loose(query.source.as_deref(), &loose)?,
    ))
}
#[derive(Deserialize)]
struct ProjectMutation {
    source_path: Option<String>,
    intent: DesktopIntent,
}
async fn project_mutate(
    State(state): State<Arc<ServeState>>,
    Json(input): Json<ProjectMutation>,
) -> ApiResult<ProjectSnapshot> {
    mutate(&state, input.source_path.as_deref(), input.intent)
}
fn mutate(
    state: &ServeState,
    source: Option<&str>,
    intent: DesktopIntent,
) -> ApiResult<ProjectSnapshot> {
    state.writable()?;
    Ok(Json(state.backend()?.mutate(source, intent)?))
}
#[derive(Deserialize)]
struct AdoptInput {
    source_path: String,
    base_hash: Option<String>,
    #[serde(default)]
    loose: bool,
}
async fn preview_adopt(
    State(state): State<Arc<ServeState>>,
    Json(input): Json<AdoptInput>,
) -> ApiResult<todo_core::AdoptionPreview> {
    state.writable()?;
    Ok(Json(
        state
            .backend()?
            .preview_adopt_with(&input.source_path, input.loose)?,
    ))
}
async fn apply_adopt(
    State(state): State<Arc<ServeState>>,
    Json(input): Json<AdoptInput>,
) -> ApiResult<ProjectSnapshot> {
    state.writable()?;
    let hash = input.base_hash.ok_or_else(|| ApiError {
        status: StatusCode::BAD_REQUEST,
        message: "review an adoption diff and supply base_hash before confirming".into(),
    })?;
    Ok(Json(state.backend()?.apply_adopt_with(
        &input.source_path,
        &hash,
        input.loose,
    )?))
}

#[derive(Deserialize)]
struct ColumnSetupQuery {
    source: String,
    expected_project_path: Option<String>,
}
#[derive(Deserialize)]
struct ColumnPreviewInput {
    source_path: String,
    selection: ColumnSetupSelection,
    expected_project_path: Option<String>,
}
#[derive(Deserialize)]
struct ColumnApplyInput {
    source_path: String,
    base_hash: String,
    selection: ColumnSetupSelection,
    expected_project_path: Option<String>,
}
/// The HTTP server cannot switch projects, but a client may have changed servers
/// since opening its dialog. When supplied, bind even read/preview to that root.
fn column_backend(
    state: &ServeState,
    expected_root: Option<&str>,
) -> Result<ProjectBackend, ApiError> {
    let backend = state.backend()?;
    if expected_root
        .is_some_and(|root| backend.project_path().and_then(|path| path.to_str()) != Some(root))
    {
        return Err(ApiError {
            status: StatusCode::CONFLICT,
            message: "project changed before column setup; review the current source again".into(),
        });
    }
    Ok(backend)
}
async fn column_setup_options(
    State(state): State<Arc<ServeState>>,
    Query(query): Query<ColumnSetupQuery>,
) -> ApiResult<ColumnSetupOptions> {
    Ok(Json(
        column_backend(&state, query.expected_project_path.as_deref())?
            .column_setup_options(&query.source)?,
    ))
}
async fn preview_columns(
    State(state): State<Arc<ServeState>>,
    Json(input): Json<ColumnPreviewInput>,
) -> ApiResult<ColumnSetupPreview> {
    state.writable()?;
    Ok(Json(
        column_backend(&state, input.expected_project_path.as_deref())?
            .preview_columns(&input.source_path, &input.selection)?,
    ))
}
async fn apply_columns(
    State(state): State<Arc<ServeState>>,
    Json(input): Json<ColumnApplyInput>,
) -> ApiResult<ProjectSnapshot> {
    state.writable()?;
    Ok(Json(
        column_backend(&state, input.expected_project_path.as_deref())?.apply_columns(
            &input.source_path,
            &input.base_hash,
            &input.selection,
        )?,
    ))
}

// Legacy per-task endpoints retain their single-source contract. Multi-source
// callers must name a source; an ID is never searched globally to guess a file.
#[derive(Deserialize)]
struct NewTask {
    body: String,
    column: Option<String>,
    source_path: Option<String>,
}
async fn create_task(
    State(state): State<Arc<ServeState>>,
    Json(input): Json<NewTask>,
) -> ApiResult<ProjectSnapshot> {
    mutate(
        &state,
        input.source_path.as_deref(),
        DesktopIntent::AddTask {
            body: input.body,
            column: input.column,
        },
    )
}
#[derive(Deserialize)]
struct Completed {
    completed: bool,
    note: Option<String>,
    evidence: Option<todo_core::EvidenceRange>,
    source_path: Option<String>,
}
async fn set_completed(
    State(state): State<Arc<ServeState>>,
    AxumPath(id): AxumPath<String>,
    Json(input): Json<Completed>,
) -> ApiResult<ProjectSnapshot> {
    mutate(
        &state,
        input.source_path.as_deref(),
        DesktopIntent::SetCompleted {
            id,
            completed: input.completed,
            note: input.note,
            evidence: input.evidence,
        },
    )
}
#[derive(Deserialize)]
struct MoveInput {
    column: String,
    note: Option<String>,
    evidence: Option<todo_core::EvidenceRange>,
    source_path: Option<String>,
}
async fn move_task(
    State(state): State<Arc<ServeState>>,
    AxumPath(id): AxumPath<String>,
    Json(input): Json<MoveInput>,
) -> ApiResult<ProjectSnapshot> {
    mutate(
        &state,
        input.source_path.as_deref(),
        DesktopIntent::MoveTask {
            id,
            column: input.column,
            target: MoveTarget::ColumnStart,
            note: input.note,
            evidence: input.evidence,
        },
    )
}
#[derive(Deserialize)]
struct PriorityInput {
    #[serde(default)]
    priority: PriorityValue,
    #[serde(default)]
    scope: PriorityScope,
    source_path: Option<String>,
}
#[derive(Default)]
enum PriorityValue {
    #[default]
    Missing,
    Value(Option<u8>),
}
impl<'de> Deserialize<'de> for PriorityValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Option::<u8>::deserialize(deserializer).map(Self::Value)
    }
}
async fn set_priority(
    State(state): State<Arc<ServeState>>,
    AxumPath(id): AxumPath<String>,
    Json(input): Json<PriorityInput>,
) -> ApiResult<ProjectSnapshot> {
    let PriorityValue::Value(priority) = input.priority else {
        return Err(ApiError {
            status: StatusCode::BAD_REQUEST,
            message: "priority must be 1-4 or null".into(),
        });
    };
    mutate(
        &state,
        input.source_path.as_deref(),
        DesktopIntent::SetPriority {
            id,
            priority,
            scope: input.scope,
        },
    )
}
#[derive(Deserialize)]
struct ProgressInput {
    percent: u8,
    note: Option<String>,
    source_path: Option<String>,
}
async fn set_progress(
    State(state): State<Arc<ServeState>>,
    AxumPath(id): AxumPath<String>,
    Json(input): Json<ProgressInput>,
) -> ApiResult<ProjectSnapshot> {
    state.writable()?;
    let note = input.note.ok_or_else(|| ApiError {
        status: StatusCode::BAD_REQUEST,
        message: "partial completion requires a note".into(),
    })?;
    mutate(
        &state,
        input.source_path.as_deref(),
        DesktopIntent::SetPartial {
            id,
            percent: input.percent,
            note,
        },
    )
}
#[derive(Deserialize)]
struct FailInput {
    note: Option<String>,
    source_path: Option<String>,
}
async fn set_failed(
    State(state): State<Arc<ServeState>>,
    AxumPath(id): AxumPath<String>,
    Json(input): Json<FailInput>,
) -> ApiResult<ProjectSnapshot> {
    state.writable()?;
    let note = input.note.ok_or_else(|| ApiError {
        status: StatusCode::BAD_REQUEST,
        message: "failed attempt requires a note".into(),
    })?;
    mutate(
        &state,
        input.source_path.as_deref(),
        DesktopIntent::SetFailed { id, note },
    )
}
#[derive(Deserialize)]
struct ScanQuery {
    #[serde(default = "yes")]
    markdown: bool,
    #[serde(default = "yes")]
    comments: bool,
    source: Option<String>,
}
fn yes() -> bool {
    true
}
async fn run_scan(
    State(state): State<Arc<ServeState>>,
    Query(query): Query<ScanQuery>,
) -> ApiResult<todo_core::ScanReport> {
    let backend = state.backend()?;
    let path = state.source_path(query.source.as_deref())?;
    let document = parse_file(&path)?;
    let options = ScanOptions {
        markdown: query.markdown,
        comments: query.comments,
        task_file: path,
    };
    Ok(Json(scan(
        backend.project_path().ok_or(todo_core::Error::NoProject)?,
        &document,
        &options,
    )?))
}
#[derive(Serialize)]
struct RewritePreview {
    diff: todo_core::TextDiff,
    proposed: String,
    base_hash: String,
}
#[derive(Deserialize)]
struct ImportInput {
    #[serde(default = "todo_column")]
    column: String,
    #[serde(default = "yes")]
    markdown: bool,
    #[serde(default = "yes")]
    comments: bool,
    confirm_base_hash: Option<String>,
    source_path: Option<String>,
}
fn todo_column() -> String {
    "todo".into()
}
async fn import_scan(
    State(state): State<Arc<ServeState>>,
    Json(input): Json<ImportInput>,
) -> Result<axum::response::Response, ApiError> {
    let backend = state.backend()?;
    let path = state.source_path(input.source_path.as_deref())?;
    let document = parse_file(&path)?;
    let options = ScanOptions {
        markdown: input.markdown,
        comments: input.comments,
        task_file: path.clone(),
    };
    let report = scan(
        backend.project_path().ok_or(todo_core::Error::NoProject)?,
        &document,
        &options,
    )?;
    let proposed = import_text(&document, &report, &input.column)?;
    let base_hash = content_hash(&document.source);
    match input.confirm_base_hash {
        None => Ok(Json(RewritePreview {
            diff: diff_lines(&document.source, &proposed),
            proposed,
            base_hash,
        })
        .into_response()),
        Some(confirmed) => {
            state.writable()?;
            LockedWriter::new(&path, state.lock_timeout).transform_labeled(
                "scan-import",
                |current| {
                    if content_hash(&current.source) != confirmed || confirmed != base_hash {
                        return Err(todo_core::Error::ConcurrentModification);
                    }
                    Ok(proposed)
                },
            )?;
            Ok(Json(backend.snapshot()?).into_response())
        }
    }
}
async fn history(
    State(state): State<Arc<ServeState>>,
    Query(query): Query<SourceQuery>,
) -> ApiResult<Vec<todo_core::HistoryEntry>> {
    let path = state.source_path(query.source.as_deref())?;
    let mut entries = History::for_file(&path).entries_for(&path)?;
    entries.reverse();
    Ok(Json(entries))
}
async fn history_diff(
    State(state): State<Arc<ServeState>>,
    AxumPath(version): AxumPath<u64>,
    Query(query): Query<SourceQuery>,
) -> ApiResult<RewritePreview> {
    let path = state.source_path(query.source.as_deref())?;
    let previous = History::for_file(&path).read_for(&path, version)?;
    let document = parse_file(&path)?;
    Ok(Json(RewritePreview {
        diff: diff_lines(&previous, &document.source),
        base_hash: content_hash(&document.source),
        proposed: previous,
    }))
}
#[derive(Deserialize)]
struct RestoreInput {
    confirm_base_hash: String,
    source_path: Option<String>,
}
async fn history_restore(
    State(state): State<Arc<ServeState>>,
    AxumPath(version): AxumPath<u64>,
    Json(input): Json<RestoreInput>,
) -> ApiResult<ProjectSnapshot> {
    state.writable()?;
    let path = state.source_path(input.source_path.as_deref())?;
    let content = History::for_file(&path).read_for(&path, version)?;
    LockedWriter::new(&path, state.lock_timeout).transform_labeled("restore", |current| {
        if content_hash(&current.source) != input.confirm_base_hash {
            return Err(todo_core::Error::ConcurrentModification);
        }
        Ok(content)
    })?;
    Ok(Json(state.backend()?.snapshot()?))
}
async fn align_prompt() -> String {
    todo_core::ALIGNMENT_PROMPT.to_string()
}
#[derive(Deserialize)]
struct ReviewInput {
    raw: String,
    source_path: Option<String>,
}
async fn align_review(
    State(state): State<Arc<ServeState>>,
    Json(input): Json<ReviewInput>,
) -> ApiResult<todo_core::AlignmentReview> {
    let path = state.source_path(input.source_path.as_deref())?;
    Ok(Json(review_alignment(&parse_file(&path)?, &input.raw)?))
}
// ── AI feed ────────────────────────────────────────────────────────────────
// Read-mostly: the host writes through `todo ai upsert`; this server only
// reads the store and records the local marks (ack, remove, link).
#[derive(Deserialize)]
struct AiSessionsQuery {
    workspace: Option<String>,
    /// `1`/`true` includes `handled` and hidden records, like `todo ai list --all`.
    all: Option<String>,
}
#[derive(Serialize)]
struct AiSessionsResponse {
    records: Vec<AiRecord>,
    /// Whether a host has ever published: the store file exists on disk.
    connected: bool,
}
async fn ai_sessions(
    State(state): State<Arc<ServeState>>,
    Query(query): Query<AiSessionsQuery>,
) -> ApiResult<AiSessionsResponse> {
    let store = state.ai_store()?;
    let include_all = matches!(query.all.as_deref(), Some("1" | "true"));
    let records = store.list(&AiListFilter {
        workspace: query.workspace.map(PathBuf::from),
        states: Vec::new(),
        include_all,
    })?;
    Ok(Json(AiSessionsResponse {
        records,
        connected: store.path().exists(),
    }))
}
async fn ai_ack(
    State(state): State<Arc<ServeState>>,
    AxumPath((source, id)): AxumPath<(String, String)>,
) -> ApiResult<AiRecord> {
    state.writable()?;
    Ok(Json(state.ai_store()?.ack(&source, &id)?))
}
async fn ai_remove(
    State(state): State<Arc<ServeState>>,
    AxumPath((source, id)): AxumPath<(String, String)>,
) -> ApiResult<AiRecord> {
    state.writable()?;
    Ok(Json(state.ai_store()?.remove(&source, &id)?))
}
#[derive(Deserialize)]
struct AiLinkInput {
    task_id: String,
}
async fn ai_link(
    State(state): State<Arc<ServeState>>,
    AxumPath((source, id)): AxumPath<(String, String)>,
    Json(input): Json<AiLinkInput>,
) -> ApiResult<AiRecord> {
    state.writable()?;
    Ok(Json(state.ai_store()?.link(
        &source,
        &id,
        &input.task_id,
    )?))
}
#[derive(Deserialize, Default)]
struct AiToTaskInput {
    column: Option<String>,
    source_path: Option<String>,
}
#[derive(Serialize)]
struct AiToTaskResponse {
    task_id: String,
    record: AiRecord,
    snapshot: ProjectSnapshot,
}
/// Creates a Project Task from a session and links the record to it. Task
/// line first, then the link (a retry must not create a second task), then
/// the `↳` note that keeps the way back to the session.
async fn ai_to_task(
    State(state): State<Arc<ServeState>>,
    AxumPath((source, id)): AxumPath<(String, String)>,
    Json(input): Json<AiToTaskInput>,
) -> ApiResult<AiToTaskResponse> {
    state.writable()?;
    let store = state.ai_store()?;
    let record = store.get(&source, &id)?;
    let backend = state.backend()?;
    let mut target = backend.source_backend(input.source_path.as_deref())?;
    let task_id = target.create_task(&ai_task_body(&record), input.column.as_deref())?;
    let record = store.link(&source, &id, &task_id)?;
    let path = target.project_path().ok_or(todo_core::Error::NoProject)?;
    let note = ai_task_note(&record);
    LockedWriter::new(path, state.lock_timeout)
        .update_labeled("ai-to-task", |document| {
            let task = document
                .tasks
                .iter()
                .position(|task| task.id.as_deref() == Some(task_id.as_str()))
                .ok_or_else(|| todo_core::Error::TaskNotFound(task_id.clone()))?;
            Ok(vec![Change::SetNote { task, text: note }])
        })
        .map_err(|error| {
            let api = ApiError::from(error);
            ApiError {
                status: api.status,
                message: format!(
                    "task {task_id} was created and linked, but its note was not written: {}",
                    api.message
                ),
            }
        })?;
    Ok(Json(AiToTaskResponse {
        task_id,
        record,
        snapshot: backend.snapshot()?,
    }))
}
/// One-line task body: the title, else the question, else the session ID.
/// A task line cannot hold newlines or comment markers, so both are removed.
fn ai_task_body(record: &AiRecord) -> String {
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
fn ai_task_note(record: &AiRecord) -> String {
    let text = match record.open_url.as_deref() {
        Some(url) => format!("来自 {} 会话 {}：{url}", record.source, record.session_id),
        None => format!("来自 {} 会话 {}", record.source, record.session_id),
    };
    text.replace("<!--", "").replace("-->", "")
}

fn content_hash(source: &str) -> String {
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
