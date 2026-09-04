//! HTTP surface of `todo serve`.
//!
//! The server is a face over the same core, so these tests check the properties
//! the HTTP boundary adds rather than re-testing core behaviour: the confirm gate
//! survives being split across two requests, read-only really is read-only, and
//! retryable failures are not reported as server faults.

use std::{net::SocketAddr, path::PathBuf, time::Duration};

use tokio::net::TcpListener;

const DOC: &str = "---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n  - {id: done, name: Done, type: done}\n---\n\n## Todo\n\n- [ ] 给登录接口加限流 <!--t:aaaaaa--> !1\n\n## Done\n";

/// Boots a server on an ephemeral port and returns its base URL.
async fn start(dir: &std::path::Path, read_only: bool) -> String {
    let path: PathBuf = dir.join("TODO.md");
    let app = todo_cli::serve::router(path, Duration::from_millis(2000), read_only, None);
    let listener = TcpListener::bind("127.0.0.1:0".parse::<SocketAddr>().unwrap())
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}/api/v1")
}

async fn get(url: &str) -> (u16, String) {
    let response = reqwest_get(url).await;
    response
}

// Minimal HTTP client so the test suite does not pull in a full client stack.
async fn reqwest_get(url: &str) -> (u16, String) {
    request("GET", url, None).await
}

async fn post(url: &str, body: &str) -> (u16, String) {
    request("POST", url, Some(body)).await
}

async fn request(method: &str, url: &str, body: Option<&str>) -> (u16, String) {
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
        .map(|(_, rest)| rest)
        .unwrap_or("");
    (status, body.to_string())
}

fn json(body: &str) -> serde_json::Value {
    // Responses use Connection: close, so the body is the remainder verbatim.
    serde_json::from_str(body.trim()).unwrap_or_else(|error| {
        panic!("expected JSON, got {body:?}: {error}");
    })
}

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("TODO.md"), DOC).unwrap();
    dir
}

#[tokio::test]
async fn snapshot_is_served_over_http() {
    let dir = fixture();
    let base = start(dir.path(), false).await;

    let (status, body) = get(&format!("{base}/snapshot")).await;

    assert_eq!(status, 200);
    let value = json(&body);
    assert_eq!(value["tasks"][0]["id"], "aaaaaa");
    assert_eq!(value["tasks"][0]["priority"], 1);
}

#[tokio::test]
async fn priority_endpoint_sets_clears_and_validates_the_value() {
    let dir = fixture();
    let base = start(dir.path(), false).await;

    let (status, body) = post(
        &format!("{base}/tasks/aaaaaa/priority"),
        "{\"priority\":4,\"scope\":\"task\"}",
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(json(&body)["tasks"][0]["priority"], 4);

    let (status, body) = post(
        &format!("{base}/tasks/aaaaaa/priority"),
        "{\"priority\":null,\"scope\":\"task\"}",
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(json(&body)["tasks"][0]["priority"], serde_json::Value::Null);

    let (status, _) = post(&format!("{base}/tasks/aaaaaa/priority"), "{\"priority\":5}").await;
    assert_eq!(status, 400);
}

#[tokio::test]
async fn progress_rejects_zero_and_one_hundred_exactly_like_the_cli() {
    // One rule, one implementation: 0 and 100 would become second spellings of
    // "not started" and "done" no matter which face of the product wrote them.
    let dir = fixture();
    let base = start(dir.path(), false).await;

    for percent in [0, 100] {
        let (status, _) = post(
            &format!("{base}/tasks/aaaaaa/progress"),
            &format!("{{\"percent\":{percent}}}"),
        )
        .await;
        assert_eq!(status, 400, "percent {percent} must be rejected");
    }

    let (status, body) = post(
        &format!("{base}/tasks/aaaaaa/progress"),
        "{\"percent\":60,\"note\":\"压测未过\"}",
    )
    .await;
    assert_eq!(status, 200);
    let value = json(&body);
    assert_eq!(value["tasks"][0]["progress"], 60);
    assert_eq!(value["tasks"][0]["note"], "压测未过");
}

#[tokio::test]
async fn progress_and_failure_require_notes_and_share_the_desktop_mutation_contract() {
    let dir = fixture();
    let base = start(dir.path(), false).await;

    let (missing_note, _) =
        post(&format!("{base}/tasks/aaaaaa/progress"), "{\"percent\":60}").await;
    assert_eq!(missing_note, 400);

    let (progress, body) = post(
        &format!("{base}/tasks/aaaaaa/progress"),
        "{\"percent\":60,\"note\":\"core complete\"}",
    )
    .await;
    assert_eq!(progress, 200);
    assert_eq!(json(&body)["tasks"][0]["progress"], 60);

    let (missing_note, _) = post(&format!("{base}/tasks/aaaaaa/fail"), "{}").await;
    assert_eq!(missing_note, 400);

    let (failed, body) = post(
        &format!("{base}/tasks/aaaaaa/fail"),
        "{\"note\":\"load test failed\"}",
    )
    .await;
    assert_eq!(failed, 200);
    assert_eq!(json(&body)["tasks"][0]["note"], "load test failed");
}

#[tokio::test]
async fn move_endpoint_changes_state_atomically_and_accepts_strict_completion_details() {
    let dir = fixture();
    let path = dir.path().join("TODO.md");
    let strict = std::fs::read_to_string(&path).unwrap().replace(
        "todo-spec: 1\n",
        "todo-spec: 1\ncompletion-policy: strict\n",
    );
    std::fs::write(&path, strict).unwrap();
    let base = start(dir.path(), false).await;

    let (rejected, _) = post(&format!("{base}/tasks/aaaaaa/move"), r#"{"column":"done"}"#).await;
    assert_eq!(rejected, 400);

    let (completed, body) = post(
        &format!("{base}/tasks/aaaaaa/move"),
        r#"{"column":"done","note":"严格验收通过"}"#,
    )
    .await;
    assert_eq!(completed, 200);
    let value = json(&body);
    assert_eq!(value["tasks"][0]["column"], "done");
    assert_eq!(value["tasks"][0]["completed"], true);
    assert_eq!(value["tasks"][0]["note"], "严格验收通过");

    let (restored, body) = post(&format!("{base}/tasks/aaaaaa/move"), r#"{"column":"todo"}"#).await;
    assert_eq!(restored, 200);
    let value = json(&body);
    assert_eq!(value["tasks"][0]["column"], "todo");
    assert_eq!(value["tasks"][0]["completed"], false);
    assert_eq!(value["tasks"][0]["note"], "严格验收通过");
}

#[tokio::test]
async fn an_import_preview_writes_nothing() {
    let dir = fixture();
    std::fs::write(dir.path().join("plan.md"), "- [ ] 写迁移脚本\n").unwrap();
    let base = start(dir.path(), false).await;
    let before = std::fs::read_to_string(dir.path().join("TODO.md")).unwrap();

    let (status, body) = post(&format!("{base}/scan/import"), "{\"column\":\"todo\"}").await;

    assert_eq!(status, 200);
    let value = json(&body);
    assert!(value["diff"]["added"].as_u64().unwrap() > 0);
    assert!(value["base_hash"].as_str().is_some());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("TODO.md")).unwrap(),
        before,
        "a preview must not write"
    );
}

#[tokio::test]
async fn confirming_an_import_with_a_stale_hash_is_refused() {
    // The confirm gate has to survive being split across two requests: without the
    // hash check a client could confirm one diff and have a different one applied.
    let dir = fixture();
    std::fs::write(dir.path().join("plan.md"), "- [ ] 写迁移脚本\n").unwrap();
    let base = start(dir.path(), false).await;
    let before = std::fs::read_to_string(dir.path().join("TODO.md")).unwrap();

    let (status, body) = post(
        &format!("{base}/scan/import"),
        "{\"column\":\"todo\",\"confirm_base_hash\":\"deadbeefdeadbeef\"}",
    )
    .await;

    assert_eq!(status, 409);
    assert!(json(&body)["error"].as_str().unwrap().contains("changed"));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("TODO.md")).unwrap(),
        before
    );
}

#[tokio::test]
async fn confirming_an_import_with_the_matching_hash_applies_it() {
    let dir = fixture();
    std::fs::write(dir.path().join("plan.md"), "- [ ] 写迁移脚本\n").unwrap();
    let base = start(dir.path(), false).await;

    let (_, preview) = post(&format!("{base}/scan/import"), "{\"column\":\"todo\"}").await;
    let hash = json(&preview)["base_hash"].as_str().unwrap().to_string();

    let (status, body) = post(
        &format!("{base}/scan/import"),
        &format!("{{\"column\":\"todo\",\"confirm_base_hash\":\"{hash}\"}}"),
    )
    .await;

    assert_eq!(status, 200);
    assert_eq!(json(&body)["tasks"].as_array().unwrap().len(), 2);
    let written = std::fs::read_to_string(dir.path().join("TODO.md")).unwrap();
    assert!(written.contains("写迁移脚本"));
    assert!(written.contains("#scan-md"));
}

#[tokio::test]
async fn read_only_refuses_writes_but_still_serves_reads() {
    // Mobile runs against this. "View on the phone" must not be able to become
    // "rewrite the repository from the phone".
    let dir = fixture();
    let base = start(dir.path(), true).await;

    let (read, _) = get(&format!("{base}/snapshot")).await;
    assert_eq!(read, 200);

    for (url, payload) in [
        (format!("{base}/tasks"), "{\"body\":\"新任务\"}"),
        (
            format!("{base}/tasks/aaaaaa/completed"),
            "{\"completed\":true}",
        ),
        (
            format!("{base}/tasks/aaaaaa/progress"),
            "{\"percent\":50,\"note\":\"still working\"}",
        ),
        (
            format!("{base}/tasks/aaaaaa/fail"),
            "{\"note\":\"attempt failed\"}",
        ),
        (format!("{base}/tasks/aaaaaa/move"), "{\"column\":\"done\"}"),
    ] {
        let (status, _) = post(&url, payload).await;
        assert_eq!(status, 403, "{url} must be refused in read-only mode");
    }

    assert_eq!(
        std::fs::read_to_string(dir.path().join("TODO.md")).unwrap(),
        DOC
    );
}

fn git(dir: &std::path::Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .status()
        .unwrap();
    assert!(status.success());
}

#[tokio::test]
async fn completed_rejects_invalid_evidence_without_writing() {
    // A real repository, so the rejection comes from the range rules rather than
    // from git being absent.
    let dir = fixture();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["add", "TODO.md"]);
    git(dir.path(), &["commit", "-qm", "seed"]);
    let before = std::fs::read(dir.path().join("TODO.md")).unwrap();
    let base = start(dir.path(), false).await;

    let (status, body) = post(
        &format!("{base}/tasks/aaaaaa/completed"),
        "{\"completed\":true,\"evidence\":{\"from\":\"HEAD\",\"to\":\"HEAD\"}}",
    )
    .await;

    assert_eq!(status, 400, "{body}");
    assert!(body.contains("must not be empty"), "{body}");
    assert_eq!(std::fs::read(dir.path().join("TODO.md")).unwrap(), before);
}

#[tokio::test]
async fn a_missing_task_is_a_404_not_a_server_error() {
    // Clients need to tell "you asked for something that is not there" from
    // "the server is broken"; collapsing both into 500 makes retry logic wrong.
    let dir = fixture();
    let base = start(dir.path(), false).await;

    let (status, _) = post(
        &format!("{base}/tasks/zzzzzz/progress"),
        "{\"percent\":50,\"note\":\"still working\"}",
    )
    .await;

    assert_eq!(status, 404);
}

#[tokio::test]
async fn history_is_listed_newest_first_after_writes() {
    let dir = fixture();
    let base = start(dir.path(), false).await;

    post(
        &format!("{base}/tasks/aaaaaa/progress"),
        "{\"percent\":40,\"note\":\"in progress\"}",
    )
    .await;
    post(
        &format!("{base}/tasks/aaaaaa/fail"),
        "{\"note\":\"attempt failed\"}",
    )
    .await;

    let (status, body) = get(&format!("{base}/history")).await;

    assert_eq!(status, 200);
    let entries = json(&body);
    let ops: Vec<&str> = entries
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["op"].as_str().unwrap())
        .collect();
    assert_eq!(ops, vec!["board-fail", "board-partial"]);
}

#[tokio::test]
async fn the_alignment_prompt_is_reachable() {
    let dir = fixture();
    let base = start(dir.path(), false).await;

    let (status, body) = get(&format!("{base}/align/prompt")).await;

    assert_eq!(status, 200);
    assert!(body.contains("核对 TODO 与代码现实"));
}
