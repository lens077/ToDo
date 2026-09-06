use serde_json::{json, Value};
use std::{fs, path::Path, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
const DOC: &str = "---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n  - {id: done, name: Done, type: done}\n---\n\n## Todo\n- [ ] task <!--t:aaaaaa--> !1\n\n## Done\n";
async fn start(root: &Path, read_only: bool) -> String {
    let app = todo_cli::serve::router(root.to_path_buf(), Duration::from_secs(2), read_only, None);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("{addr}")
}
async fn req(base: &str, method: &str, route: &str, body: Value) -> (u16, Value) {
    let mut stream = tokio::net::TcpStream::connect(base).await.unwrap();
    let payload = body.to_string();
    let message = format!("{method} /api/v1{route} HTTP/1.1\r\nHost: {base}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{payload}",payload.len());
    stream.write_all(message.as_bytes()).await.unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).await.unwrap();
    let status = raw.split_whitespace().nth(1).unwrap().parse().unwrap();
    let body = raw.split_once("\r\n\r\n").unwrap().1;
    (status, serde_json::from_str(body).unwrap_or(Value::Null))
}
#[tokio::test]
async fn directory_without_top_level_file_discovers_nested_sources() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("docs")).unwrap();
    fs::write(dir.path().join("docs/todo.md"), DOC).unwrap();
    let base = start(dir.path(), false).await;
    let (code, snapshot) = req(&base, "GET", "/snapshot", Value::Null).await;
    assert_eq!(code, 200, "{snapshot}");
    assert_eq!(snapshot["sources"][0]["path"], "docs/todo.md");
    assert_eq!(snapshot["tasks"][0]["source_path"], "docs/todo.md");
    fs::write(
        dir.path().join("TODO.txt"),
        "ordinary text\n- [ ] discovered only\n",
    )
    .unwrap();
    let (_, snapshot) = req(&base, "GET", "/snapshot", Value::Null).await;
    assert_eq!(snapshot["sources"].as_array().unwrap().len(), 2);
}
#[tokio::test]
async fn same_ids_require_source_and_only_target_file_changes() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("TODO.md"), DOC).unwrap();
    fs::write(dir.path().join("TODO.txt"), DOC).unwrap();
    let base = start(dir.path(), false).await;
    let intent = json!({"type":"set_completed","id":"aaaaaa","completed":true});
    let (code, _) = req(&base, "POST", "/mutate", json!({"intent":intent})).await;
    assert_eq!(code, 400);
    let (code, after) = req(
        &base,
        "POST",
        "/mutate",
        json!({"source_path":"TODO.txt","intent":intent}),
    )
    .await;
    assert_eq!(code, 200, "{after}");
    assert_eq!(after["tasks"].as_array().unwrap().len(), 2);
    assert_eq!(fs::read_to_string(dir.path().join("TODO.md")).unwrap(), DOC);
    assert!(fs::read_to_string(dir.path().join("TODO.txt"))
        .unwrap()
        .contains("- [x]"));
    let (_, history) = req(&base, "GET", "/history?source=TODO.txt", Value::Null).await;
    let version = history[0]["version"].as_u64().unwrap();
    let (_, other) = req(&base, "GET", "/history?source=TODO.md", Value::Null).await;
    assert!(other.as_array().unwrap().is_empty());
    let (code, _) = req(
        &base,
        "GET",
        &format!("/history/{version}/diff?source=TODO.md"),
        Value::Null,
    )
    .await;
    assert_eq!(code, 404);
    let (code, _) = req(
        &base,
        "POST",
        &format!("/history/{version}/restore"),
        json!({"source_path":"TODO.md","confirm_base_hash":"any"}),
    )
    .await;
    assert_eq!(code, 404);
}
#[tokio::test]
async fn adoption_is_explicit_preview_bound_and_readonly_enforced() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("TODO");
    let original = "Notes remain untouched\n- [ ] first\n";
    fs::write(&file, original).unwrap();
    let base = start(dir.path(), false).await;
    let (code, _) = req(
        &base,
        "POST",
        "/mutate",
        json!({"source_path":"TODO","intent":{"type":"add_task","body":"bad","column":null}}),
    )
    .await;
    assert_eq!(code, 400);
    assert_eq!(fs::read_to_string(&file).unwrap(), original);
    let (code, preview) = req(
        &base,
        "POST",
        "/sources/adopt/preview",
        json!({"source_path":"TODO"}),
    )
    .await;
    assert_eq!(code, 200, "{preview}");
    assert_eq!(fs::read_to_string(&file).unwrap(), original);
    fs::write(&file, format!("{original}external edit\n")).unwrap();
    let (code, _) = req(
        &base,
        "POST",
        "/sources/adopt",
        json!({"source_path":"TODO","base_hash":preview["base_hash"]}),
    )
    .await;
    assert_eq!(code, 409);
    let (_, preview) = req(
        &base,
        "POST",
        "/sources/adopt/preview",
        json!({"source_path":"TODO"}),
    )
    .await;
    let (code, _) = req(
        &base,
        "POST",
        "/sources/adopt",
        json!({"source_path":"TODO","base_hash":preview["base_hash"]}),
    )
    .await;
    assert_eq!(code, 200);
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        preview["proposed"].as_str().unwrap()
    );
    let readonly = start(dir.path(), true).await;
    let (code, _) = req(
        &readonly,
        "POST",
        "/sources/adopt/preview",
        json!({"source_path":"TODO"}),
    )
    .await;
    assert_eq!(code, 403);
}
#[tokio::test]
async fn traversal_and_symlinks_cannot_address_sources_or_global_captures() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("TODO.md"), DOC).unwrap();
    let base = start(dir.path(), false).await;
    for source in ["../TODO.md", "/tmp/TODO.md", "docs/../TODO.md"] {
        let (code, _) = req(
            &base,
            "POST",
            "/mutate",
            json!({"source_path":source,"intent":{"type":"add_task","body":"bad","column":"todo"}}),
        )
        .await;
        assert_eq!(code, 400, "source {source}");
    }
    #[cfg(unix)]
    {
        let external = tempfile::tempdir().unwrap();
        fs::write(external.path().join("TODO.txt"), DOC).unwrap();
        std::os::unix::fs::symlink(
            external.path().join("TODO.txt"),
            dir.path().join("TODO.txt"),
        )
        .unwrap();
        let (code,_)=req(&base,"POST","/mutate",json!({"source_path":"TODO.txt","intent":{"type":"add_task","body":"bad","column":"todo"}})).await;
        assert_eq!(code, 400);
        assert_eq!(
            fs::read_to_string(external.path().join("TODO.txt")).unwrap(),
            DOC
        );
    }
    let (code, _) = req(&base, "GET", "/captures", Value::Null).await;
    assert_eq!(code, 404);
}
