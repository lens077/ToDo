//! `todo serve` — the HTTP face of the same core the CLI and desktop use.
//!
//! This exists so the Web UI and `curl` reach the product without a second
//! implementation of anything. Handlers are thin: they parse, call `todo-core`,
//! and serialise. Every safety property therefore holds here for free — writes
//! go through the same locked writer, land in the same version history, and a
//! bulk rewrite is still refused unless the caller echoes the base hash it was
//! shown.
//!
//! Binding is loopback by default. A task file is a repository's working state,
//! and exposing it on every interface by accident is not a mistake the user
//! should have to notice; `--host` makes that an explicit decision.

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use axum::{
    extract::{Path as AxumPath, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use todo_core::{
    diff_lines, import_text, parse_file, review_alignment, scan, DesktopBackend, DesktopIntent,
    DesktopSnapshot, History, LockedWriter, MoveTarget, ScanOptions,
};
use tower_http::{cors::CorsLayer, services::ServeDir};

#[derive(Clone)]
struct ServeState {
    path: PathBuf,
    lock_timeout: Duration,
    /// Read-only mode: every mutating route is refused. Mobile clients are
    /// expected to run against this, so "view on the phone" cannot become
    /// "accidentally rewrite the repository from the phone".
    read_only: bool,
}

impl ServeState {
    fn backend(&self) -> Result<DesktopBackend, ApiError> {
        let mut backend = DesktopBackend::new(self.lock_timeout);
        backend
            .open_project(self.path.to_string_lossy().as_ref())
            .map_err(ApiError::from)?;
        Ok(backend)
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
        // Lock contention and optimistic-lock failures are retryable and must not
        // be reported as server faults, or a client cannot tell "try again" from
        // "this is broken".
        let status = match error {
            todo_core::Error::LockTimeout { .. } => StatusCode::SERVICE_UNAVAILABLE,
            todo_core::Error::ConcurrentModification => StatusCode::CONFLICT,
            todo_core::Error::TaskNotFound(_) | todo_core::Error::HistoryVersionNotFound(_) => {
                StatusCode::NOT_FOUND
            }
            todo_core::Error::NotTaskFile | todo_core::Error::InvalidTaskLine(_) => {
                StatusCode::BAD_REQUEST
            }
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
            Json(serde_json::json!({ "ok": false, "error": self.message })),
        )
            .into_response()
    }
}

type ApiResult<T> = Result<Json<T>, ApiError>;

pub fn router(
    path: PathBuf,
    lock_timeout: Duration,
    read_only: bool,
    web_root: Option<PathBuf>,
) -> Router {
    let state = ServeState {
        path,
        lock_timeout,
        read_only,
    };
    let api = Router::new()
        .route("/health", get(health))
        .route("/snapshot", get(snapshot))
        .route("/tasks", post(create_task))
        .route("/tasks/{id}/completed", post(set_completed))
        .route("/tasks/{id}/progress", post(set_progress))
        .route("/tasks/{id}/fail", post(set_failed))
        .route("/tasks/{id}/move", post(move_task))
        .route("/scan", get(run_scan))
        .route("/scan/import", post(import_scan))
        .route("/history", get(history))
        .route("/history/{version}/diff", get(history_diff))
        .route("/history/{version}/restore", post(history_restore))
        .route("/align/prompt", get(align_prompt))
        .route("/align/review", post(align_review))
        .with_state(Arc::new(state))
        // The Web UI may be served from a different origin during development.
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
        .map_err(|error| format!("could not bind {addr}: {error}"))?;
    let bound = listener.local_addr().map_err(|error| error.to_string())?;
    eprintln!(
        "todo serve → http://{bound}/api/v1  (project: {})",
        path.display()
    );
    if read_only {
        eprintln!("read-only: mutating endpoints return 403");
    }
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown())
        .await
        .map_err(|error| error.to_string())
}

async fn shutdown() {
    let _ = tokio::signal::ctrl_c().await;
}

#[derive(Serialize)]
struct Health {
    ok: bool,
    read_only: bool,
    project: String,
}

async fn health(State(state): State<Arc<ServeState>>) -> Json<Health> {
    Json(Health {
        ok: true,
        read_only: state.read_only,
        project: state.path.display().to_string(),
    })
}

async fn snapshot(State(state): State<Arc<ServeState>>) -> ApiResult<DesktopSnapshot> {
    Ok(Json(state.backend()?.snapshot().map_err(ApiError::from)?))
}

fn mutate(state: &ServeState, intent: DesktopIntent) -> ApiResult<DesktopSnapshot> {
    state.writable()?;
    let backend = state.backend()?;
    Ok(Json(backend.mutate(intent).map_err(ApiError::from)?))
}

#[derive(Deserialize)]
struct NewTask {
    body: String,
    #[serde(default)]
    column: Option<String>,
}

async fn create_task(
    State(state): State<Arc<ServeState>>,
    Json(input): Json<NewTask>,
) -> ApiResult<DesktopSnapshot> {
    mutate(
        &state,
        DesktopIntent::AddTask {
            body: input.body,
            column: input.column,
        },
    )
}

#[derive(Deserialize)]
struct Completed {
    completed: bool,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    evidence: Option<todo_core::EvidenceRange>,
}

async fn set_completed(
    State(state): State<Arc<ServeState>>,
    AxumPath(id): AxumPath<String>,
    Json(input): Json<Completed>,
) -> ApiResult<DesktopSnapshot> {
    mutate(
        &state,
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
}

async fn move_task(
    State(state): State<Arc<ServeState>>,
    AxumPath(id): AxumPath<String>,
    Json(input): Json<MoveInput>,
) -> ApiResult<DesktopSnapshot> {
    mutate(
        &state,
        DesktopIntent::MoveTask {
            id,
            column: input.column,
            target: MoveTarget::ColumnEnd,
        },
    )
}

#[derive(Deserialize)]
struct ProgressInput {
    percent: u8,
    #[serde(default)]
    note: Option<String>,
}

/// Applies the same 1-99 rule the CLI enforces, for the same reason: 0 and 100
/// would become second spellings of "not started" and "done".
async fn set_progress(
    State(state): State<Arc<ServeState>>,
    AxumPath(id): AxumPath<String>,
    Json(input): Json<ProgressInput>,
) -> ApiResult<DesktopSnapshot> {
    state.writable()?;
    let note = input.note.ok_or_else(|| ApiError {
        status: StatusCode::BAD_REQUEST,
        message: "partial completion requires a note".into(),
    })?;
    mutate(
        &state,
        DesktopIntent::SetPartial {
            id,
            percent: input.percent,
            note,
        },
    )
}

#[derive(Deserialize)]
struct FailInput {
    #[serde(default)]
    note: Option<String>,
}

async fn set_failed(
    State(state): State<Arc<ServeState>>,
    AxumPath(id): AxumPath<String>,
    Json(input): Json<FailInput>,
) -> ApiResult<DesktopSnapshot> {
    state.writable()?;
    let note = input.note.ok_or_else(|| ApiError {
        status: StatusCode::BAD_REQUEST,
        message: "failed attempt requires a note".into(),
    })?;
    mutate(&state, DesktopIntent::SetFailed { id, note })
}

#[derive(Deserialize)]
struct ScanQuery {
    #[serde(default = "yes")]
    markdown: bool,
    #[serde(default = "yes")]
    comments: bool,
}

fn yes() -> bool {
    true
}

async fn run_scan(
    State(state): State<Arc<ServeState>>,
    Query(query): Query<ScanQuery>,
) -> ApiResult<todo_core::ScanReport> {
    let document = parse_file(&state.path).map_err(ApiError::from)?;
    let root = state
        .path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    let options = ScanOptions {
        markdown: query.markdown,
        comments: query.comments,
        task_file: state.path.clone(),
    };
    Ok(Json(
        scan(root, &document, &options).map_err(ApiError::from)?,
    ))
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
    /// Echo of the hash from a previous preview. Absent means "preview only".
    #[serde(default)]
    confirm_base_hash: Option<String>,
}

fn todo_column() -> String {
    "todo".into()
}

/// Preview, then apply — never both in one unconfirmed call.
///
/// Without `confirm_base_hash` this returns the diff and writes nothing. With it,
/// the hash must still match the file on disk, so a client cannot confirm one
/// rewrite and have a different one applied.
async fn import_scan(
    State(state): State<Arc<ServeState>>,
    Json(input): Json<ImportInput>,
) -> Result<axum::response::Response, ApiError> {
    let document = parse_file(&state.path).map_err(ApiError::from)?;
    let root = state
        .path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    let options = ScanOptions {
        markdown: input.markdown,
        comments: input.comments,
        task_file: state.path.clone(),
    };
    let report = scan(root, &document, &options).map_err(ApiError::from)?;
    let proposed = import_text(&document, &report, &input.column).map_err(ApiError::from)?;
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
            if confirmed != base_hash {
                return Err(ApiError {
                    status: StatusCode::CONFLICT,
                    message: "the file changed since the preview; review the new diff and retry"
                        .into(),
                });
            }
            let writer = LockedWriter::new(&state.path, state.lock_timeout);
            writer
                .replace_all("scan-import", &proposed)
                .map_err(ApiError::from)?;
            Ok(Json(state.backend()?.snapshot().map_err(ApiError::from)?).into_response())
        }
    }
}

async fn history(State(state): State<Arc<ServeState>>) -> ApiResult<Vec<todo_core::HistoryEntry>> {
    let mut entries = History::for_file(&state.path)
        .entries()
        .map_err(ApiError::from)?;
    entries.reverse();
    Ok(Json(entries))
}

async fn history_diff(
    State(state): State<Arc<ServeState>>,
    AxumPath(version): AxumPath<u64>,
) -> ApiResult<RewritePreview> {
    let previous = History::for_file(&state.path)
        .read(version)
        .map_err(ApiError::from)?;
    let document = parse_file(&state.path).map_err(ApiError::from)?;
    Ok(Json(RewritePreview {
        diff: diff_lines(&previous, &document.source),
        base_hash: content_hash(&document.source),
        proposed: previous,
    }))
}

#[derive(Deserialize)]
struct RestoreInput {
    confirm_base_hash: String,
}

async fn history_restore(
    State(state): State<Arc<ServeState>>,
    AxumPath(version): AxumPath<u64>,
    Json(input): Json<RestoreInput>,
) -> ApiResult<DesktopSnapshot> {
    state.writable()?;
    let document = parse_file(&state.path).map_err(ApiError::from)?;
    if content_hash(&document.source) != input.confirm_base_hash {
        return Err(ApiError {
            status: StatusCode::CONFLICT,
            message: "the file changed since the preview; review the new diff and retry".into(),
        });
    }
    let content = History::for_file(&state.path)
        .read(version)
        .map_err(ApiError::from)?;
    let writer = LockedWriter::new(&state.path, state.lock_timeout);
    writer
        .replace_all("restore", &content)
        .map_err(ApiError::from)?;
    Ok(Json(state.backend()?.snapshot().map_err(ApiError::from)?))
}

async fn align_prompt() -> String {
    todo_core::ALIGNMENT_PROMPT.to_string()
}

#[derive(Deserialize)]
struct ReviewInput {
    raw: String,
}

async fn align_review(
    State(state): State<Arc<ServeState>>,
    Json(input): Json<ReviewInput>,
) -> ApiResult<todo_core::AlignmentReview> {
    let document = parse_file(&state.path).map_err(ApiError::from)?;
    Ok(Json(
        review_alignment(&document, &input.raw).map_err(ApiError::from)?,
    ))
}

fn content_hash(source: &str) -> String {
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
