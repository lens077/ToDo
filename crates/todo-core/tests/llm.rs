use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::Path,
    process::Command,
    thread,
    time::Duration,
};

use tempfile::tempdir;
use todo_core::{
    parse, LlmConfig, LlmProvider, LlmSemanticReconciler, ReconcileOptions, Reconciler,
    ALIGNMENT_PROMPT,
};

struct MockResponse {
    status: u16,
    body: String,
    delay: Duration,
}

fn serve_once(response: MockResponse) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = Vec::new();
        let mut buffer = [0u8; 4096];
        let header_end = loop {
            let read = stream.read(&mut buffer).unwrap();
            assert!(read > 0, "client closed before sending headers");
            request.extend_from_slice(&buffer[..read]);
            if let Some(index) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                break index + 4;
            }
        };
        let headers = String::from_utf8_lossy(&request[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().unwrap())
            })
            .unwrap_or(0);
        while request.len() < header_end + content_length {
            let read = stream.read(&mut buffer).unwrap();
            assert!(read > 0, "client closed before sending its body");
            request.extend_from_slice(&buffer[..read]);
        }

        thread::sleep(response.delay);
        let reason = match response.status {
            200 => "OK",
            401 => "Unauthorized",
            429 => "Too Many Requests",
            _ => "Internal Server Error",
        };
        let reply = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response.status,
            reason,
            response.body.len(),
            response.body
        );
        let _ = stream.write_all(reply.as_bytes());
        String::from_utf8(request).unwrap()
    });
    (format!("http://{address}"), handle)
}

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn evidence_project() -> (tempfile::TempDir, todo_core::Document) {
    let directory = tempdir().unwrap();
    git(directory.path(), &["init", "-q"]);
    git(
        directory.path(),
        &["config", "user.email", "test@example.com"],
    );
    git(directory.path(), &["config", "user.name", "Test"]);
    fs::write(directory.path().join("README"), "base\n").unwrap();
    git(directory.path(), &["add", "."]);
    git(directory.path(), &["commit", "-qm", "base"]);
    fs::create_dir(directory.path().join("src")).unwrap();
    fs::write(
        directory.path().join("src/lib.rs"),
        "pub fn feature() -> bool { true }\n",
    )
    .unwrap();
    git(directory.path(), &["add", "."]);
    git(directory.path(), &["commit", "-qm", "implement feature"]);
    let from = git(directory.path(), &["rev-parse", "HEAD"]);
    fs::write(
        directory.path().join("src/lib.rs"),
        "pub fn feature() -> bool { true }\npub fn wired() {}\n",
    )
    .unwrap();
    git(directory.path(), &["add", "."]);
    git(directory.path(), &["commit", "-qm", "wire feature"]);
    let to = git(directory.path(), &["rev-parse", "HEAD"]);
    let document = parse(format!(
        "---\ntodo-spec: 1\ncolumns:\n  - {{id: todo, name: Todo}}\n---\n\n## Todo\n- [ ] feature <!--t:aaaaaa--> 🔗{from}..{to}\n"
    ))
    .unwrap();
    (directory, document)
}

fn config(provider: LlmProvider, base_url: String) -> LlmConfig {
    LlmConfig {
        provider,
        api_key: "top-secret-key".into(),
        base_url,
        model: "test-model".into(),
    }
}

#[test]
fn openai_uses_alignment_prompt_and_validated_response() {
    let alignment = serde_json::json!({
        "schema": "todo.alignment.v1",
        "truncated": false,
        "verdicts": [{
            "task_id": "aaaaaa",
            "verdict": "done",
            "confidence": "high",
            "reason": "implementation is wired",
            "citations": ["src/lib.rs:1"],
            "percent": null
        }]
    })
    .to_string();
    let body = serde_json::json!({
        "choices": [{"message": {"content": alignment}}]
    })
    .to_string();
    let (base_url, server) = serve_once(MockResponse {
        status: 200,
        body,
        delay: Duration::ZERO,
    });
    let semantic = LlmSemanticReconciler::new(config(LlmProvider::OpenAi, base_url)).unwrap();
    let (directory, document) = evidence_project();

    let report = Reconciler::new(directory.path())
        .with_semantic(&semantic)
        .reconcile(&document, &ReconcileOptions { semantic: true })
        .unwrap();

    assert!(report.checks_skipped.is_empty(), "{report:?}");
    assert_eq!(report.suspected.len(), 1);
    assert_eq!(report.suspected[0].judgment, "semantic-done");
    let request = server.join().unwrap();
    assert!(request.starts_with("POST /v1/chat/completions HTTP/1.1"));
    assert!(request
        .to_ascii_lowercase()
        .contains("authorization: bearer top-secret-key"));
    let request_body = request.split("\r\n\r\n").nth(1).unwrap();
    let json: serde_json::Value = serde_json::from_str(request_body).unwrap();
    assert_eq!(json["messages"][0]["content"], ALIGNMENT_PROMPT);
}

#[test]
fn anthropic_uses_messages_api_and_system_prompt() {
    let alignment = serde_json::json!({
        "schema": "todo.alignment.v1",
        "truncated": false,
        "verdicts": [{
            "task_id": "aaaaaa",
            "verdict": "unknown",
            "confidence": "low",
            "reason": "insufficient context",
            "citations": [],
            "percent": null
        }]
    })
    .to_string();
    let body = serde_json::json!({
        "content": [{"type": "text", "text": alignment}]
    })
    .to_string();
    let (base_url, server) = serve_once(MockResponse {
        status: 200,
        body,
        delay: Duration::ZERO,
    });
    let semantic = LlmSemanticReconciler::new(config(LlmProvider::Anthropic, base_url)).unwrap();
    let (directory, document) = evidence_project();

    let report = Reconciler::new(directory.path())
        .with_semantic(&semantic)
        .reconcile(&document, &ReconcileOptions { semantic: true })
        .unwrap();

    assert!(!report.incomplete(), "{report:?}");
    assert!(report.suspected.is_empty());
    let request = server.join().unwrap();
    assert!(request.starts_with("POST /v1/messages HTTP/1.1"));
    assert!(request
        .to_ascii_lowercase()
        .contains("x-api-key: top-secret-key"));
    assert!(request
        .to_ascii_lowercase()
        .contains("anthropic-version: 2023-06-01"));
    let request_body = request.split("\r\n\r\n").nth(1).unwrap();
    let json: serde_json::Value = serde_json::from_str(request_body).unwrap();
    assert_eq!(json["system"], ALIGNMENT_PROMPT);
}

#[test]
fn authentication_failure_is_reported_as_a_skipped_check_without_secrets() {
    let (base_url, server) = serve_once(MockResponse {
        status: 401,
        body: r#"{"error":"provider-secret-body"}"#.into(),
        delay: Duration::ZERO,
    });
    let semantic = LlmSemanticReconciler::new(config(LlmProvider::OpenAi, base_url)).unwrap();
    let (directory, document) = evidence_project();

    let report = Reconciler::new(directory.path())
        .with_semantic(&semantic)
        .reconcile(&document, &ReconcileOptions { semantic: true })
        .unwrap();
    server.join().unwrap();

    assert!(report.incomplete());
    let reason = report.checks_skipped.join("\n");
    assert!(reason.contains("authentication"), "{reason}");
    assert!(!reason.contains("top-secret-key"));
    assert!(!reason.contains("provider-secret-body"));
}

#[test]
fn connection_failure_is_reported_as_a_skipped_check() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    // A developer shell often exports http_proxy. A loopback endpoint (a local
    // model, or this closed port) must be contacted directly regardless, or the
    // refused connection turns into a slow proxy error and is classified wrongly.
    // The client reads the proxy environment when it is built, so the variable
    // only needs to exist for that call.
    std::env::set_var("http_proxy", "http://192.0.2.1:9");
    let semantic = LlmSemanticReconciler::new(config(LlmProvider::OpenAi, base_url));
    std::env::remove_var("http_proxy");
    let semantic = semantic.unwrap();
    let (directory, document) = evidence_project();

    let report = Reconciler::new(directory.path())
        .with_semantic(&semantic)
        .reconcile(&document, &ReconcileOptions { semantic: true })
        .unwrap();

    assert!(report.incomplete());
    assert!(report
        .checks_skipped
        .iter()
        .any(|reason| reason.contains("network connection failed")));
}

#[test]
fn timeout_is_reported_as_a_skipped_check() {
    let (base_url, server) = serve_once(MockResponse {
        status: 200,
        body: "{}".into(),
        delay: Duration::from_millis(150),
    });
    let semantic = LlmSemanticReconciler::with_timeout(
        config(LlmProvider::OpenAi, base_url),
        Duration::from_millis(25),
    )
    .unwrap();
    let (directory, document) = evidence_project();

    let report = Reconciler::new(directory.path())
        .with_semantic(&semantic)
        .reconcile(&document, &ReconcileOptions { semantic: true })
        .unwrap();
    server.join().unwrap();

    assert!(report.incomplete());
    assert!(report
        .checks_skipped
        .iter()
        .any(|reason| reason.contains("timed out")));
}
