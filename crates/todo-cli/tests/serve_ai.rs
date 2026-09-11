//! AI feed endpoints of `todo serve` (`docs/ai-feed-spec.md`).
//!
//! The store is seeded through `todo_core::AiStore` exactly as `todo ai upsert`
//! would; these tests cover what the HTTP boundary adds: the workspace and
//! `all` filters, the local marks, the task created by `to-task`, and that
//! read-only really refuses the marks too.

use std::{net::SocketAddr, path::PathBuf, time::Duration};

use todo_core::{AiOutcome, AiState, AiStore, AiUpsert};
use tokio::net::TcpListener;

const DOC: &str = "---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n  - {id: done, name: Done, type: done}\n---\n\n## Todo\n\n- [ ] 给登录接口加限流 <!--t:aaaaaa--> !1\n\n## Done\n";

struct Server {
    base: String,
    store: PathBuf,
}

async fn start(dir: &std::path::Path, read_only: bool) -> Server {
    let store = dir.join("ai").join("sessions.json");
    let app = todo_cli::serve::router_with_ai_store(
        dir.join("TODO.md"),
        Duration::from_millis(2000),
        read_only,
        None,
        Some(store.clone()),
    );
    let listener = TcpListener::bind("127.0.0.1:0".parse::<SocketAddr>().unwrap())
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    Server {
        base: format!("http://{addr}/api/v1"),
        store,
    }
}

async fn request(method: &str, url: &str, body: Option<&str>) -> (u16, serde_json::Value) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let stripped = url.strip_prefix("http://").unwrap();
    let (authority, path) = stripped.split_once('/').unwrap();
    let mut stream = tokio::net::TcpStream::connect(authority).await.unwrap();
    let payload = body.unwrap_or_default();
    let request = format!(
        "{method} /{path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{payload}",
        payload.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).await.unwrap();
    let status = raw
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or(0);
    let body = raw
        .split_once("\r\n\r\n")
        .map(|(_, rest)| rest.trim())
        .unwrap_or("");
    let json = if body.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_str(body)
            .unwrap_or_else(|error| panic!("expected JSON, got {body:?}: {error}"))
    };
    (status, json)
}

async fn get(url: &str) -> (u16, serde_json::Value) {
    request("GET", url, None).await
}

async fn post(url: &str, body: &str) -> (u16, serde_json::Value) {
    request("POST", url, Some(body)).await
}

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("TODO.md"), DOC).unwrap();
    dir
}

fn upsert(store: &std::path::Path, session: &str, workspace: &str, state: AiState, event_at: i64) {
    AiStore::new(store, Duration::from_secs(2))
        .upsert(AiUpsert {
            spec: 1,
            source: "dsh".into(),
            session_id: session.into(),
            workspace: workspace.into(),
            state,
            outcome: matches!(state, AiState::Unread | AiState::Seen | AiState::Handled)
                .then_some(AiOutcome::Completed),
            title: Some(format!("会话 {session}")),
            question: Some("补一段 cargo install 的说明".into()),
            reply: Some("已在 README 加入说明".into()),
            reply_truncated: false,
            changed_files: vec!["README.md".into()],
            changed_file_count: None,
            open_url: Some(format!("http://127.0.0.1:3080/?session={session}")),
            event_at: Some(event_at),
        })
        .unwrap();
}

#[tokio::test]
async fn sessions_are_listed_in_contract_order_and_filtered_by_workspace_and_all() {
    let dir = fixture();
    let server = start(dir.path(), false).await;
    let root = dir.path().to_str().unwrap().to_string();
    upsert(&server.store, "s_run", &root, AiState::Running, 10);
    upsert(
        &server.store,
        "s_wait",
        &format!("{root}/apps"),
        AiState::Waiting,
        20,
    );
    upsert(&server.store, "s_done", &root, AiState::Handled, 30);
    upsert(
        &server.store,
        "s_other",
        "/elsewhere/repo",
        AiState::Unread,
        40,
    );

    let (status, body) = get(&format!("{}/ai/sessions", server.base)).await;
    assert_eq!(status, 200);
    assert_eq!(body["connected"], true);
    let ids: Vec<&str> = body["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|record| record["session_id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        ["s_wait", "s_other", "s_run"],
        "handled is excluded by default"
    );

    let (_, body) = get(&format!("{}/ai/sessions?all=1", server.base)).await;
    let ids: Vec<&str> = body["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|record| record["session_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["s_wait", "s_other", "s_run", "s_done"]);

    let encoded = root.replace('/', "%2F");
    let (_, body) = get(&format!(
        "{}/ai/sessions?workspace={encoded}&all=1",
        server.base
    ))
    .await;
    let ids: Vec<&str> = body["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|record| record["session_id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        ["s_wait", "s_run", "s_done"],
        "workspace matches the root and paths under it"
    );
    let record = &body["records"][0];
    for key in [
        "source",
        "session_id",
        "workspace",
        "state",
        "outcome",
        "title",
        "question",
        "reply",
        "reply_truncated",
        "changed_files",
        "changed_file_count",
        "open_url",
        "event_at",
        "received_at",
        "acked_at",
        "hidden",
        "task_id",
    ] {
        assert!(record.get(key).is_some(), "record is missing `{key}`");
    }
    assert_eq!(record["state"], "waiting");
}

#[tokio::test]
async fn an_absent_store_is_reported_as_not_connected() {
    let dir = fixture();
    let server = start(dir.path(), false).await;
    let (status, body) = get(&format!("{}/ai/sessions", server.base)).await;
    assert_eq!(status, 200);
    assert_eq!(body["connected"], false);
    assert_eq!(body["records"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn ack_remove_and_link_write_local_marks_only() {
    let dir = fixture();
    let server = start(dir.path(), false).await;
    let root = dir.path().to_str().unwrap();
    upsert(&server.store, "s_1", root, AiState::Unread, 10);

    let (status, body) = post(&format!("{}/ai/sessions/dsh/s_1/ack", server.base), "").await;
    assert_eq!(status, 200);
    assert!(body["acked_at"].is_number());
    assert_eq!(
        body["state"], "unread",
        "ack does not change the host's state"
    );

    let (status, body) = post(
        &format!("{}/ai/sessions/dsh/s_1/link", server.base),
        r#"{"task_id":"aaaaaa"}"#,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["task_id"], "aaaaaa");

    let (status, _) = post(
        &format!("{}/ai/sessions/dsh/s_1/link", server.base),
        r#"{"task_id":"  "}"#,
    )
    .await;
    assert_eq!(status, 400);

    let (status, body) = post(&format!("{}/ai/sessions/dsh/s_1/remove", server.base), "").await;
    assert_eq!(status, 200);
    assert_eq!(body["hidden"], true);
    let (_, body) = get(&format!("{}/ai/sessions", server.base)).await;
    assert_eq!(
        body["records"].as_array().unwrap().len(),
        0,
        "removed records leave the default list"
    );
    let (_, body) = get(&format!("{}/ai/sessions?all=1", server.base)).await;
    assert_eq!(body["records"][0]["hidden"], true);

    let (status, _) = post(&format!("{}/ai/sessions/dsh/missing/ack", server.base), "").await;
    assert_eq!(status, 404, "an unknown session is not a server fault");
}

#[tokio::test]
async fn to_task_creates_a_linked_task_with_a_session_note() {
    let dir = fixture();
    let server = start(dir.path(), false).await;
    let root = dir.path().to_str().unwrap();
    upsert(&server.store, "s_8f2a", root, AiState::Unread, 10);

    let (status, body) = post(
        &format!("{}/ai/sessions/dsh/s_8f2a/to-task", server.base),
        r#"{"column":"todo"}"#,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let task_id = body["task_id"].as_str().unwrap().to_string();
    assert_eq!(body["record"]["task_id"], task_id);
    let created = body["snapshot"]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|task| task["id"] == task_id)
        .expect("the snapshot includes the new task");
    assert_eq!(created["body"], "会话 s_8f2a");
    assert_eq!(created["column"], "todo");
    assert_eq!(
        created["note"],
        "来自 dsh 会话 s_8f2a：http://127.0.0.1:3080/?session=s_8f2a"
    );
    let file = std::fs::read_to_string(dir.path().join("TODO.md")).unwrap();
    assert!(
        file.contains(&format!("- [ ] 会话 s_8f2a <!--t:{task_id}-->")),
        "{file}"
    );
    assert!(
        file.contains("↳ 来自 dsh 会话 s_8f2a：http://127.0.0.1:3080/?session=s_8f2a <!--n:"),
        "{file}"
    );
}

#[tokio::test]
async fn read_only_serves_the_feed_but_refuses_every_mark() {
    let dir = fixture();
    let server = start(dir.path(), true).await;
    let root = dir.path().to_str().unwrap();
    upsert(&server.store, "s_1", root, AiState::Unread, 10);

    let (status, body) = get(&format!("{}/ai/sessions", server.base)).await;
    assert_eq!(status, 200);
    assert_eq!(body["records"].as_array().unwrap().len(), 1);

    for (action, payload) in [
        ("ack", ""),
        ("remove", ""),
        ("link", r#"{"task_id":"aaaaaa"}"#),
        ("to-task", "{}"),
    ] {
        let (status, body) = post(
            &format!("{}/ai/sessions/dsh/s_1/{action}", server.base),
            payload,
        )
        .await;
        assert_eq!(status, 403, "{action}: {body}");
    }
    let (_, body) = get(&format!("{}/ai/sessions?all=1", server.base)).await;
    let record = &body["records"][0];
    assert!(record["acked_at"].is_null());
    assert_eq!(record["hidden"], false);
    assert!(record["task_id"].is_null());
    let file = std::fs::read_to_string(dir.path().join("TODO.md")).unwrap();
    assert_eq!(file, DOC, "read-only wrote nothing to the task file");
}
