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
    diff_lines, import_text, parse_file, review_alignment, scan, DesktopBackend, DesktopIntent,
    History, LockedWriter, MoveTarget, PriorityScope, ProjectBackend, ProjectSnapshot, ScanOptions,
};
use tower_http::{cors::CorsLayer, services::ServeDir};

#[derive(Clone)]
struct ServeState {
    path: PathBuf,
    lock_timeout: Duration,
    read_only: bool,
}
impl ServeState {
    fn backend(&self) -> Result<ProjectBackend, ApiError> {
        let mut backend = ProjectBackend::new(self.lock_timeout);
        backend.open_project(&self.path)?;
        Ok(backend)
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
            todo_core::Error::TaskNotFound(_) | todo_core::Error::HistoryVersionNotFound(_) => {
                StatusCode::NOT_FOUND
            }
            todo_core::Error::NotTaskFile
            | todo_core::Error::NoProject
            | todo_core::Error::InvalidTaskLine(_)
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

pub fn router(
    path: PathBuf,
    lock_timeout: Duration,
    read_only: bool,
    web_root: Option<PathBuf>,
) -> Router {
    let state = ServeState {
        path: path.canonicalize().unwrap_or(path),
        lock_timeout,
        read_only,
    };
    let api = Router::new()
        .route("/health", get(health))
        .route("/snapshot", get(snapshot))
        .route("/mutate", post(project_mutate))
        .route("/sources/adopt/preview", post(preview_adopt))
        .route("/sources/adopt", post(apply_adopt))
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
fn content_hash(source: &str) -> String {
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
