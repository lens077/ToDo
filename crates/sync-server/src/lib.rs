//! virgo-sync — self-hosted sync/parse service for the personal reading
//! library. The server is "a ReadingStore without a desktop": every capability
//! (X/WeChat/readability parsing, context notes, progress) is reused from
//! `todo-core`, this crate only adds the HTTP surface (see
//! `docs/sync-server-spec.md`).

use std::sync::Arc;

use axum::{
    extract::{Path, Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use todo_core::{ReadingArticleDetail, ReadingContextInput, ReadingStore};
use tokio_stream::{wrappers::BroadcastStream, Stream, StreamExt};
use tower_http::services::{ServeDir, ServeFile};

/// Shared server state: the reading library plus the change broadcaster that
/// SSE subscribers listen on.
pub struct SyncState {
    store: Arc<ReadingStore>,
    events: tokio::sync::broadcast::Sender<()>,
}

impl SyncState {
    pub fn new(root: impl Into<std::path::PathBuf>) -> Self {
        Self {
            store: Arc::new(ReadingStore::new(root)),
            events: tokio::sync::broadcast::channel(64).0,
        }
    }

    fn broadcast(&self) {
        // A send error only means no subscriber is listening; that is fine.
        let _ = self.events.send(());
    }
}

pub fn build_router(
    state: Arc<SyncState>,
    token: &str,
    web_dir: Option<&std::path::Path>,
) -> Router {
    let token = token.to_owned();
    let images = state.store.root().join("images");
    // The API is token-gated; the static web client is not (it ships no
    // data — the page itself asks for the token and calls the API).
    let api = Router::new()
        .route("/healthz", get(healthz))
        .route("/articles", get(list_articles))
        .route("/articles/{id}", get(get_article).delete(remove_article))
        .route("/articles/{id}/context", post(put_context))
        .route("/articles/{id}/progress", post(put_progress))
        .route("/articles/{id}/read", post(put_read))
        .route("/articles/{id}/refetch", post(refetch_article))
        .route("/links", post(save_link))
        .route("/stream", get(stream))
        .layer(middleware::from_fn(
            move |headers: HeaderMap, req: Request, next: Next| {
                authorize(headers, req, next, token.clone())
            },
        ))
        .with_state(state);
    let mut app = Router::new()
        .merge(api)
        .nest_service("/images", ServeDir::new(images));
    if let Some(dir) = web_dir {
        let index = dir.join("index.html");
        // SPA fallback: unknown paths render the client, which routes
        // client-side.
        app = app.fallback_service(ServeDir::new(dir).fallback(ServeFile::new(index)));
    }
    app
}

/// Bearer-token gate. `/healthz` stays open for k8s probes.
async fn authorize(headers: HeaderMap, req: Request, next: Next, token: String) -> Response {
    if req.uri().path() == "/healthz" {
        return next.run(req).await;
    }
    let authorized = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(|value| value == format!("Bearer {token}"))
        .unwrap_or(false);
    if authorized {
        next.run(req).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

/// Maps `todo-core` errors onto HTTP: missing articles are 404, client
/// mistakes are 422, everything else is a 500 with the error text (single
/// user, no error hiding).
enum ApiError {
    Core(todo_core::Error),
    BadRequest(String),
    Join(String),
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest(message.into())
    }
}

impl From<todo_core::Error> for ApiError {
    fn from(error: todo_core::Error) -> Self {
        Self::Core(error)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            Self::Core(error) => {
                let status = match &error {
                    todo_core::Error::ReadingArticleNotFound(_) => StatusCode::NOT_FOUND,
                    _ => StatusCode::INTERNAL_SERVER_ERROR,
                };
                (status, error.to_string()).into_response()
            }
            Self::BadRequest(message) => {
                (StatusCode::UNPROCESSABLE_ENTITY, message).into_response()
            }
            Self::Join(message) => (StatusCode::INTERNAL_SERVER_ERROR, message).into_response(),
        }
    }
}

/// Runs a blocking `ReadingStore` call on the blocking pool.
async fn blocking<T, F>(f: F) -> Result<T, ApiError>
where
    F: FnOnce() -> todo_core::Result<T> + Send + 'static,
    T: Send + 'static,
{
    let joined = tokio::task::spawn_blocking(f)
        .await
        .map_err(|error| ApiError::Join(error.to_string()))?;
    Ok(joined?)
}

async fn healthz() -> &'static str {
    "ok\n"
}

async fn list_articles(
    State(state): State<Arc<SyncState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let store = state.store.clone();
    let articles = blocking(move || store.list()).await?;
    Ok(Json(json!({ "articles": articles })))
}

async fn get_article(
    State(state): State<Arc<SyncState>>,
    Path(id): Path<String>,
) -> Result<Json<ReadingArticleDetail>, ApiError> {
    let store = state.store.clone();
    Ok(Json(blocking(move || store.get(&id)).await?))
}

#[derive(Deserialize)]
struct SaveLinkInput {
    url: String,
    #[serde(default)]
    context: Option<ContextInput>,
}

/// Partial context: clients only send what they have. Missing fields are
/// stored as empty, which is what `ReadingContextInput` treats as unset.
#[derive(Deserialize, Default)]
struct ContextInput {
    saved_reason: Option<String>,
    focus_questions: Option<String>,
    revisit_condition: Option<String>,
    personal_notes: Option<String>,
    revisit_type: Option<String>,
}

impl From<ContextInput> for ReadingContextInput {
    fn from(input: ContextInput) -> Self {
        let empty = String::new;
        Self {
            saved_reason: input.saved_reason.unwrap_or_else(empty),
            focus_questions: input.focus_questions.unwrap_or_else(empty),
            revisit_condition: input.revisit_condition.unwrap_or_else(empty),
            personal_notes: input.personal_notes.unwrap_or_else(empty),
            revisit_type: input.revisit_type.unwrap_or_else(empty),
        }
    }
}

/// Quick save: enter the library as a stub immediately (收藏优先), parse in
/// the background. The client gets a millisecond response; the SSE ping tells
/// it to refresh once parsing lands.
async fn save_link(
    State(state): State<Arc<SyncState>>,
    Json(input): Json<SaveLinkInput>,
) -> Result<(StatusCode, Json<ReadingArticleDetail>), ApiError> {
    let url = input.url.trim().to_string();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(ApiError::bad_request("只接受 http/https 链接"));
    }
    let store = state.store.clone();
    let mut detail = blocking(move || store.add_failed(&url)).await?;
    if let Some(context) = input.context {
        let store = state.store.clone();
        let id = detail.article.id.clone();
        detail = blocking(move || store.update_context(&id, &context.into())).await?;
    }
    state.broadcast();
    // Background parse: the stub already answered the client; refetch fills
    // in title/content asynchronously and pings subscribers when it settles
    // (success or failure both change state — a failed parse keeps the URL
    // openable, per the 收藏优先 rule).
    let background = state.clone();
    let store = state.store.clone();
    let id = detail.article.id.clone();
    tokio::spawn(async move {
        let _ = tokio::task::spawn_blocking(move || store.refetch(&id)).await;
        background.broadcast();
    });
    Ok((StatusCode::CREATED, Json(detail)))
}

/// Triggers a reparse. The network work happens on the blocking pool; the
/// response arrives when the (re)parse finished.
async fn refetch_article(
    State(state): State<Arc<SyncState>>,
    Path(id): Path<String>,
) -> Result<Json<ReadingArticleDetail>, ApiError> {
    let store = state.store.clone();
    let detail = blocking(move || store.refetch(&id)).await?;
    state.broadcast();
    Ok(Json(detail))
}

async fn put_context(
    State(state): State<Arc<SyncState>>,
    Path(id): Path<String>,
    Json(context): Json<ContextInput>,
) -> Result<Json<ReadingArticleDetail>, ApiError> {
    let store = state.store.clone();
    let detail = blocking(move || store.update_context(&id, &context.into())).await?;
    state.broadcast();
    Ok(Json(detail))
}

#[derive(Deserialize)]
struct ProgressInput {
    percent: u32,
    #[serde(default)]
    anchor: Option<String>,
}

async fn put_progress(
    State(state): State<Arc<SyncState>>,
    Path(id): Path<String>,
    Json(input): Json<ProgressInput>,
) -> Result<Json<todo_core::ReadingArticle>, ApiError> {
    let store = state.store.clone();
    let article =
        blocking(move || store.set_progress(&id, input.percent, input.anchor.as_deref())).await?;
    state.broadcast();
    Ok(Json(article))
}

#[derive(Deserialize)]
struct ReadInput {
    read: bool,
}

async fn put_read(
    State(state): State<Arc<SyncState>>,
    Path(id): Path<String>,
    Json(input): Json<ReadInput>,
) -> Result<Json<todo_core::ReadingArticle>, ApiError> {
    let store = state.store.clone();
    let article = blocking(move || store.set_read(&id, input.read)).await?;
    state.broadcast();
    Ok(Json(article))
}

async fn remove_article(
    State(state): State<Arc<SyncState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let store = state.store.clone();
    blocking(move || store.remove(&id)).await?;
    state.broadcast();
    Ok(StatusCode::NO_CONTENT)
}

/// SSE stream: one `change` event per mutation. Clients treat any event as
/// "refetch everything" — the sync model is full-state reconciliation.
async fn stream(
    State(state): State<Arc<SyncState>>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let receiver = state.events.subscribe();
    let events =
        BroadcastStream::new(receiver).map(|_| Ok(Event::default().event("change").data("1")));
    Sse::new(events).keep_alive(KeepAlive::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    type TestRequest = axum::http::Request<Body>;

    fn test_state() -> Arc<SyncState> {
        let root = tempfile::tempdir().expect("tempdir");
        Arc::new(SyncState::new(root.path()))
    }

    async fn send(
        app: Router,
        method: &'static str,
        uri: &str,
        token: Option<&str>,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, serde_json::Value) {
        let mut request = axum::http::Request::builder().method(method).uri(uri);
        if let Some(token) = token {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        if body.is_some() {
            request = request.header(header::CONTENT_TYPE, "application/json");
        }
        let request: TestRequest = request
            .body(Body::from(
                body.map(|value| value.to_string()).unwrap_or_default(),
            ))
            .expect("request");
        let response = app.oneshot(request).await.expect("response");
        let status = response.status();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let value = if bytes.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
        };
        (status, value)
    }

    #[tokio::test]
    async fn rejects_requests_without_bearer_token() {
        let app = build_router(test_state(), "secret", None);
        let (status, _) = send(app, "GET", "/articles", None, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn healthz_is_open() {
        let app = build_router(test_state(), "secret", None);
        let (status, _) = send(app, "GET", "/healthz", None, None).await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn wrong_token_is_unauthorized() {
        let app = build_router(test_state(), "secret", None);
        let (status, _) = send(app, "GET", "/articles", Some("nope"), None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn save_list_update_delete_flow() {
        let state = test_state();
        let app = build_router(state.clone(), "secret", None);

        // Quick save with a note: stub created, no parsing, context saved.
        let (status, saved) = send(
            app.clone(),
            "POST",
            "/links",
            Some("secret"),
            Some(serde_json::json!({
                "url": "https://example.com/article",
                "context": { "personal_notes": "读一下第 3 节" }
            })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "save failed: {saved}");
        let id = saved["id"].as_str().expect("id").to_string();

        let (_, listed) = send(app.clone(), "GET", "/articles", Some("secret"), None).await;
        let articles = listed["articles"].as_array().expect("articles");
        assert_eq!(articles.len(), 1);
        assert_eq!(articles[0]["url"], "https://example.com/article");

        // Update the note through the context endpoint.
        let (status, updated) = send(
            app.clone(),
            "POST",
            &format!("/articles/{id}/context"),
            Some("secret"),
            Some(serde_json::json!({ "personal_notes": "改备注" })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(updated["context"]["personal_notes"], "改备注");

        // Delete, then the library is empty again.
        let request = axum::http::Request::builder()
            .method("DELETE")
            .uri(format!("/articles/{id}"))
            .header(header::AUTHORIZATION, "Bearer secret")
            .body(Body::empty())
            .expect("request");
        let response = app.clone().oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let (_, listed) = send(app, "GET", "/articles", Some("secret"), None).await;
        assert_eq!(listed["articles"].as_array().expect("articles").len(), 0);
    }

    #[tokio::test]
    async fn save_rejects_non_http_urls() {
        let app = build_router(test_state(), "secret", None);
        let (status, _) = send(
            app,
            "POST",
            "/links",
            Some("secret"),
            Some(serde_json::json!({ "url": "file:///etc/passwd" })),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn unknown_article_is_404() {
        let app = build_router(test_state(), "secret", None);
        let (status, _) = send(app, "GET", "/articles/nope", Some("secret"), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn mutations_notify_sse_subscribers() {
        let state = test_state();
        let mut receiver = state.events.subscribe();
        let app = build_router(state, "secret", None);
        let (_, _) = send(
            app,
            "POST",
            "/links",
            Some("secret"),
            Some(serde_json::json!({ "url": "https://example.com/b" })),
        )
        .await;
        assert!(receiver.try_recv().is_ok(), "expected a change broadcast");
    }

    #[tokio::test]
    async fn web_client_serves_without_token_and_api_stays_gated() {
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::write(root.path().join("index.html"), "<html>sync-web</html>").expect("write");
        let app = build_router(test_state(), "secret", Some(root.path()));

        // Static client loads without a token.
        let (status, page) = send(app.clone(), "GET", "/", None, None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(page, serde_json::Value::Null); // body is html, json parse yields null

        // SPA fallback: an unknown path renders the client too.
        let (status, _) = send(app.clone(), "GET", "/anything", None, None).await;
        assert_eq!(status, StatusCode::OK);

        // The API behind it stays token-gated.
        let (status, _) = send(app, "GET", "/articles", None, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}
