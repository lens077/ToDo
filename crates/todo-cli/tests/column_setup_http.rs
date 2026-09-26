//! Column setup crosses HTTP as a typed, source-bound preview/confirm flow.
//! Every fixture lives in a temporary directory; repository task files are never used.

use serde_json::{json, Value};
use std::{fs, path::Path, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const DOC: &str = "---\ntodo-spec: 1\nvendor-note: 'keep: this'\n---\n\n# Project tasks\n\n## Domain\n- [ ] **Keep** `original` task <!--t:aaaaaa--> !1\n  ↳ existing note\n";
const BODY: &str = "# Project tasks\n\n## Domain\n- [ ] **Keep** `original` task <!--t:aaaaaa--> !1\n  ↳ existing note\n";
const START_BLOCK: &str = "- [ ] **Start** `only this` <!--t:start01--> #api !2\n  ↳ preserve note <!--n:start01-->\n\n  Keep [linked detail](https://example.invalid/detail) with the task.\n  - [ ] nested work <!--t:child01-->\n  - [x] completed child <!--t:child02-->\n";

fn start_document() -> String {
    format!(
        "---\ntodo-spec: 1\nvendor-note: 'keep: this'\n---\n\n# Project tasks\n\n## Domain\n{START_BLOCK}\n- [ ] sibling stays <!--t:other01-->\n\n## Archive\n- [x] finished stays <!--t:finish1-->\n"
    )
}

async fn start(root: &Path, read_only: bool) -> String {
    let app = todo_cli::serve::router(root.to_path_buf(), Duration::from_secs(2), read_only, None);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr.to_string()
}

async fn request(base: &str, method: &str, route: &str, body: Value) -> (u16, Value) {
    let mut stream = tokio::net::TcpStream::connect(base).await.unwrap();
    let payload = body.to_string();
    let message = format!(
        "{method} /api/v1{route} HTTP/1.1\r\nHost: {base}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{payload}",
        payload.len()
    );
    stream.write_all(message.as_bytes()).await.unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).await.unwrap();
    let status = raw.split_whitespace().nth(1).unwrap().parse().unwrap();
    let body = raw.split_once("\r\n\r\n").unwrap().1;
    (status, serde_json::from_str(body).unwrap_or(Value::Null))
}

fn query_value(value: &str) -> String {
    value.bytes().map(|byte| format!("%{byte:02X}")).collect()
}

#[tokio::test]
async fn defaults_preview_writes_nothing_and_confirmation_only_configures_the_named_source() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("docs")).unwrap();
    fs::write(dir.path().join("TODO.md"), DOC).unwrap();
    let target = dir.path().join("docs/TODO.md");
    fs::write(&target, DOC).unwrap();
    let base = start(dir.path(), false).await;

    let (status, options) = request(
        &base,
        "GET",
        "/sources/columns?source=docs%2FTODO.md",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{options}");
    assert_eq!(options["source_path"], "docs/TODO.md");
    assert_eq!(options["configured"], false);
    assert!(!options["base_hash"].as_str().unwrap().is_empty());
    assert_eq!(
        options["headings"],
        json!([{"name":"Domain", "line":8, "selectable":true, "reason":null}])
    );

    let selection = json!({"mode":"defaults"});
    let (status, preview) = request(
        &base,
        "POST",
        "/sources/columns/preview",
        json!({"source_path":"docs/TODO.md", "selection":selection}),
    )
    .await;
    assert_eq!(status, 200, "{preview}");
    assert_eq!(preview["source_path"], "docs/TODO.md");
    assert_eq!(preview["base_hash"], options["base_hash"]);
    assert_eq!(preview["selection"], selection);
    assert_eq!(
        preview["columns"],
        json!([
            {"id":"todo", "name":"待办", "done":false, "created":true},
            {"id":"doing", "name":"进行中", "done":false, "created":true},
            {"id":"done", "name":"已完成", "done":true, "created":true}
        ])
    );
    assert!(preview["diff"]["added"].as_u64().unwrap() > 0);
    assert_eq!(fs::read_to_string(&target).unwrap(), DOC);
    assert_eq!(fs::read_to_string(dir.path().join("TODO.md")).unwrap(), DOC);
    assert!(!dir.path().join(".todo").exists());
    assert!(!dir.path().join("docs/.todo").exists());
    assert!(!dir.path().join("docs/.TODO.md.lock").exists());

    let (status, snapshot) = request(
        &base,
        "POST",
        "/sources/columns",
        json!({
            "source_path":"docs/TODO.md",
            "base_hash":preview["base_hash"],
            "selection":selection
        }),
    )
    .await;
    assert_eq!(status, 200, "{snapshot}");
    assert_eq!(
        snapshot["project_path"],
        dir.path().canonicalize().unwrap().to_str().unwrap()
    );
    let columns = snapshot["columns"].as_array().unwrap();
    assert_eq!(columns.len(), 3);
    assert!(columns
        .iter()
        .all(|column| column["source_path"] == "docs/TODO.md"));
    assert_eq!(snapshot["tasks"].as_array().unwrap().len(), 2);
    assert!(snapshot["tasks"].as_array().unwrap().iter().all(|task| {
        task["id"] == "aaaaaa" && task["completed"] == false && task["column"].is_null()
    }));
    let written = fs::read_to_string(&target).unwrap();
    assert_eq!(written, preview["proposed"].as_str().unwrap());
    assert!(written.contains("vendor-note: 'keep: this'\n"));
    assert!(written.contains(BODY));
    assert_eq!(fs::read_to_string(dir.path().join("TODO.md")).unwrap(), DOC);

    let (status, history) =
        request(&base, "GET", "/history?source=docs%2FTODO.md", Value::Null).await;
    assert_eq!(status, 200, "{history}");
    assert_eq!(history.as_array().unwrap().len(), 1);
    let (status, options) = request(
        &base,
        "GET",
        "/sources/columns?source=docs%2FTODO.md",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{options}");
    assert_eq!(options["configured"], true);
}

#[tokio::test]
async fn a_stale_hash_and_a_repeated_confirmation_return_conflict_without_overwriting() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("TODO.md");
    fs::write(&target, DOC).unwrap();
    let base = start(dir.path(), false).await;
    let input = json!({"source_path":"TODO.md", "selection":{"mode":"defaults"}});
    let (status, preview) = request(&base, "POST", "/sources/columns/preview", input.clone()).await;
    assert_eq!(status, 200, "{preview}");
    let external = format!("{DOC}\nExternal edit must remain.\n");
    fs::write(&target, &external).unwrap();
    let confirmation = |preview: &Value| {
        json!({
            "source_path":"TODO.md",
            "base_hash":preview["base_hash"],
            "selection":preview["selection"]
        })
    };

    let (status, error) = request(&base, "POST", "/sources/columns", confirmation(&preview)).await;
    assert_eq!(status, 409, "{error}");
    assert_eq!(error["ok"], false);
    assert!(error["error"].as_str().unwrap().contains("changed"));
    assert_eq!(fs::read_to_string(&target).unwrap(), external);
    let (_, history) = request(&base, "GET", "/history?source=TODO.md", Value::Null).await;
    assert!(history.as_array().unwrap().is_empty());

    let (status, refreshed) = request(&base, "POST", "/sources/columns/preview", input).await;
    assert_eq!(status, 200, "{refreshed}");
    let (status, snapshot) =
        request(&base, "POST", "/sources/columns", confirmation(&refreshed)).await;
    assert_eq!(status, 200, "{snapshot}");
    let written = fs::read_to_string(&target).unwrap();
    let (status, error) =
        request(&base, "POST", "/sources/columns", confirmation(&refreshed)).await;
    assert_eq!(status, 409, "{error}");
    assert_eq!(fs::read_to_string(&target).unwrap(), written);
    let (_, history) = request(&base, "GET", "/history?source=TODO.md", Value::Null).await;
    assert_eq!(history.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn read_only_server_serves_options_but_refuses_preview_and_apply_with_403() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("TODO.md"), DOC).unwrap();
    let base = start(dir.path(), true).await;
    let (status, options) =
        request(&base, "GET", "/sources/columns?source=TODO.md", Value::Null).await;
    assert_eq!(status, 200, "{options}");
    for route in ["/sources/columns/preview", "/sources/columns"] {
        let (status, error) = request(
            &base,
            "POST",
            route,
            json!({
                "source_path":"TODO.md",
                "base_hash":options["base_hash"],
                "selection":{"mode":"defaults"}
            }),
        )
        .await;
        assert_eq!(status, 403, "{route}: {error}");
        assert_eq!(error["ok"], false);
        assert!(error["error"].as_str().unwrap().contains("read-only"));
    }
    assert_eq!(fs::read_to_string(dir.path().join("TODO.md")).unwrap(), DOC);
    assert!(!dir.path().join(".todo").exists());
    assert!(!dir.path().join(".TODO.md.lock").exists());
}

#[tokio::test]
async fn every_column_endpoint_rejects_invalid_traversal_and_undiscovered_sources() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("TODO.md"), DOC).unwrap();
    fs::write(dir.path().join("notes.md"), "ordinary notes\n").unwrap();
    fs::write(dir.path().join("TODO.txt"), "- [ ] not adopted\n").unwrap();
    fs::write(outside.path().join("TODO.md"), DOC).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), dir.path().join("linked")).unwrap();
    let absolute = outside.path().join("TODO.md");
    let base = start(dir.path(), false).await;
    for source in [
        "",
        "../TODO.md",
        "docs/../TODO.md",
        "./TODO.md",
        "docs\\TODO.md",
        "missing.md",
        "notes.md",
        "TODO.txt",
        "linked/TODO.md",
        absolute.to_str().unwrap(),
    ] {
        let (status, error) = request(
            &base,
            "GET",
            &format!("/sources/columns?source={}", query_value(source)),
            Value::Null,
        )
        .await;
        assert_eq!(status, 400, "GET source {source:?}: {error}");
        for route in ["/sources/columns/preview", "/sources/columns"] {
            let (status, error) = request(
                &base,
                "POST",
                route,
                json!({"source_path":source, "base_hash":"unused", "selection":{"mode":"defaults"}}),
            )
            .await;
            assert_eq!(status, 400, "{route} source {source:?}: {error}");
        }
    }
    assert_eq!(fs::read_to_string(dir.path().join("TODO.md")).unwrap(), DOC);
    assert_eq!(fs::read_to_string(&absolute).unwrap(), DOC);
    assert_eq!(
        fs::read_to_string(dir.path().join("notes.md")).unwrap(),
        "ordinary notes\n"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("TODO.txt")).unwrap(),
        "- [ ] not adopted\n"
    );
    assert!(!dir.path().join(".todo").exists());
}

#[tokio::test]
async fn supplied_project_binding_is_checked_on_options_preview_and_apply() {
    let dir = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    for root in [dir.path(), other.path()] {
        fs::write(root.join("TODO.md"), DOC).unwrap();
    }
    // Explicit-file serve binds to its parent Project Root, not the file itself.
    let base = start(&dir.path().join("TODO.md"), false).await;
    let root = dir
        .path()
        .canonicalize()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let other_root = other
        .path()
        .canonicalize()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let route = |expected: &str| {
        format!(
            "/sources/columns?source=TODO.md&expected_project_path={}",
            query_value(expected)
        )
    };
    let (status, options) = request(&base, "GET", &route(&root), Value::Null).await;
    assert_eq!(status, 200, "{options}");
    for expected in [other_root.as_str(), &format!("{root}/."), ""] {
        let (status, error) = request(&base, "GET", &route(expected), Value::Null).await;
        assert_eq!(status, 409, "expected root {expected:?}: {error}");
        for route in ["/sources/columns/preview", "/sources/columns"] {
            let (status, error) = request(
                &base,
                "POST",
                route,
                json!({
                    "source_path":"TODO.md",
                    "base_hash":options["base_hash"],
                    "selection":{"mode":"defaults"},
                    "expected_project_path":expected
                }),
            )
            .await;
            assert_eq!(status, 409, "{route}: {error}");
            assert!(error["error"].as_str().unwrap().contains("project changed"));
        }
    }
    let (status, preview) = request(
        &base,
        "POST",
        "/sources/columns/preview",
        json!({"source_path":"TODO.md", "selection":{"mode":"defaults"}, "expected_project_path":root}),
    )
    .await;
    assert_eq!(status, 200, "{preview}");
    let (status, snapshot) = request(
        &base,
        "POST",
        "/sources/columns",
        json!({
            "source_path":"TODO.md",
            "base_hash":preview["base_hash"],
            "selection":preview["selection"],
            "expected_project_path":root
        }),
    )
    .await;
    assert_eq!(status, 200, "{snapshot}");
    assert_eq!(snapshot["project_path"], root);
    assert_eq!(
        fs::read_to_string(other.path().join("TODO.md")).unwrap(),
        DOC
    );
}

#[tokio::test]
async fn existing_selection_round_trips_and_client_markdown_cannot_control_the_write() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("TODO.md");
    fs::write(&target, DOC).unwrap();
    let base = start(dir.path(), false).await;
    let selection = json!({"mode":"existing", "headings":[{"name":"Domain", "done":true}]});
    let (status, preview) = request(
        &base,
        "POST",
        "/sources/columns/preview",
        json!({"source_path":"TODO.md", "selection":selection, "proposed":"malicious replacement"}),
    )
    .await;
    assert_eq!(status, 200, "{preview}");
    assert_eq!(preview["selection"], selection);
    assert_eq!(preview["columns"].as_array().unwrap().len(), 1);
    assert_eq!(preview["columns"][0]["name"], "Domain");
    assert_eq!(preview["columns"][0]["done"], true);
    assert_eq!(preview["columns"][0]["created"], false);
    assert_eq!(fs::read_to_string(&target).unwrap(), DOC);
    let (status, snapshot) = request(
        &base,
        "POST",
        "/sources/columns",
        json!({
            "source_path":"TODO.md",
            "selection":selection,
            "base_hash":preview["base_hash"],
            "proposed":"malicious replacement"
        }),
    )
    .await;
    assert_eq!(status, 200, "{snapshot}");
    assert_eq!(snapshot["columns"][0]["done"], true);
    assert_eq!(snapshot["tasks"][0]["completed"], false);
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        preview["proposed"].as_str().unwrap()
    );
    assert!(fs::read_to_string(&target).unwrap().contains(BODY));
}

#[tokio::test]
async fn setup_requires_explicit_source_selection_and_confirmation_hash() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("TODO.md");
    fs::write(&target, DOC).unwrap();
    let base = start(dir.path(), false).await;
    let (status, _) = request(&base, "GET", "/sources/columns", Value::Null).await;
    assert_eq!(status, 400, "even a single-source project requires source");
    for (route, body) in [
        (
            "/sources/columns/preview",
            json!({"selection":{"mode":"defaults"}}),
        ),
        ("/sources/columns/preview", json!({"source_path":"TODO.md"})),
        (
            "/sources/columns/preview",
            json!({"source_path":"TODO.md", "selection":{"mode":"automatic"}}),
        ),
        (
            "/sources/columns/preview",
            json!({"source_path":"TODO.md", "selection":{"mode":"existing", "headings":[{"name":"Domain"}]}}),
        ),
        (
            "/sources/columns",
            json!({"source_path":"TODO.md", "selection":{"mode":"defaults"}}),
        ),
        (
            "/sources/columns",
            json!({"source_path":"TODO.md", "base_hash":"anything", "proposed":"not a selection"}),
        ),
    ] {
        let (status, _) = request(&base, "POST", route, body.clone()).await;
        assert_eq!(status, 422, "{route}: {body}");
    }
    let (status, error) = request(
        &base,
        "POST",
        "/sources/columns/preview",
        json!({"source_path":"TODO.md", "selection":{"mode":"existing", "headings":[]}}),
    )
    .await;
    assert_eq!(status, 400, "{error}");
    assert_eq!(error["ok"], false);
    assert_eq!(fs::read_to_string(&target).unwrap(), DOC);
    assert!(!dir.path().join(".todo").exists());
    assert!(!dir.path().join(".TODO.md.lock").exists());
}

#[tokio::test]
async fn defaults_and_start_previews_and_applies_one_source_bound_block_with_one_history() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("docs")).unwrap();
    let original = start_document();
    let target = dir.path().join("docs/TODO.md");
    fs::write(&target, &original).unwrap();
    fs::write(dir.path().join("TODO.md"), &original).unwrap();
    let base = start(dir.path(), false).await;
    let selection = json!({"mode":"defaults_and_start", "task_id":"start01"});
    let (status, preview) = request(
        &base,
        "POST",
        "/sources/columns/preview",
        json!({"source_path":"docs/TODO.md", "selection":selection}),
    )
    .await;
    assert_eq!(status, 200, "{preview}");
    assert_eq!(preview["selection"], selection);
    assert_eq!(preview["source_path"], "docs/TODO.md");
    assert_eq!(
        preview["columns"],
        json!([
            {"id":"todo", "name":"待办", "done":false, "created":true},
            {"id":"doing", "name":"进行中", "done":false, "created":true},
            {"id":"done", "name":"已完成", "done":true, "created":true}
        ])
    );
    let proposed = preview["proposed"].as_str().unwrap();
    assert!(proposed.contains(START_BLOCK));
    assert_eq!(proposed.matches(START_BLOCK).count(), 1);
    let doing = proposed.split_once("## 进行中\n").unwrap().1;
    assert!(doing
        .split_once("## 已完成\n")
        .unwrap()
        .0
        .contains(START_BLOCK));
    assert!(proposed.contains("## Domain\n"));
    assert!(proposed.contains("- [ ] sibling stays <!--t:other01-->\n"));
    assert!(proposed.contains("## Archive\n- [x] finished stays <!--t:finish1-->\n"));
    assert!(proposed.contains("vendor-note: 'keep: this'\n"));
    assert!(preview["diff"]["added"].as_u64().unwrap() > 0);
    assert!(preview["diff"]["removed"].as_u64().unwrap() > 0);
    // Abandoning this preview is cancel: no task, column, lock, or history was written.
    assert_eq!(fs::read_to_string(&target).unwrap(), original);
    assert_eq!(
        fs::read_to_string(dir.path().join("TODO.md")).unwrap(),
        original
    );
    assert!(!dir.path().join("docs/.todo").exists());
    assert!(!dir.path().join("docs/.TODO.md.lock").exists());

    let (status, snapshot) = request(
        &base,
        "POST",
        "/sources/columns",
        json!({
            "source_path":"docs/TODO.md", "base_hash":preview["base_hash"],
            "selection":selection, "proposed":"never trust client Markdown"
        }),
    )
    .await;
    assert_eq!(status, 200, "{snapshot}");
    assert_eq!(fs::read_to_string(&target).unwrap(), proposed);
    assert_eq!(
        fs::read_to_string(dir.path().join("TODO.md")).unwrap(),
        original
    );
    assert_eq!(snapshot["columns"].as_array().unwrap().len(), 3);
    assert!(snapshot["columns"]
        .as_array()
        .unwrap()
        .iter()
        .all(|column| column["source_path"] == "docs/TODO.md"));
    let tasks = snapshot["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 10);
    for task in tasks {
        let moving = task["source_path"] == "docs/TODO.md"
            && matches!(task["id"].as_str(), Some("start01" | "child01" | "child02"));
        assert_eq!(
            task["column"],
            if moving { json!("doing") } else { Value::Null }
        );
        assert_eq!(
            task["completed"],
            matches!(task["id"].as_str(), Some("child02" | "finish1"))
        );
    }
    let (_, history) = request(&base, "GET", "/history?source=docs%2FTODO.md", Value::Null).await;
    assert_eq!(history.as_array().unwrap().len(), 1);
    let version = history[0]["version"].as_u64().unwrap();
    let (status, restore) = request(
        &base,
        "GET",
        &format!("/history/{version}/diff?source=docs%2FTODO.md"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{restore}");
    assert_eq!(restore["proposed"], original);
    let (_, untouched) = request(&base, "GET", "/history?source=TODO.md", Value::Null).await;
    assert!(untouched.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn defaults_and_start_stale_hash_refuses_both_setup_and_move() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("TODO.md");
    let original = start_document();
    fs::write(&target, &original).unwrap();
    let base = start(dir.path(), false).await;
    let selection = json!({"mode":"defaults_and_start", "task_id":"start01"});
    let (status, preview) = request(
        &base,
        "POST",
        "/sources/columns/preview",
        json!({"source_path":"TODO.md", "selection":selection}),
    )
    .await;
    assert_eq!(status, 200, "{preview}");
    let external = format!("{original}\nExternal edit wins.\n");
    fs::write(&target, &external).unwrap();
    let (status, error) = request(
        &base,
        "POST",
        "/sources/columns",
        json!({"source_path":"TODO.md", "base_hash":preview["base_hash"], "selection":selection}),
    )
    .await;
    assert_eq!(status, 409, "{error}");
    assert_eq!(error["ok"], false);
    assert_eq!(fs::read_to_string(&target).unwrap(), external);
    let (_, options) = request(&base, "GET", "/sources/columns?source=TODO.md", Value::Null).await;
    assert_eq!(options["configured"], false);
    let (_, snapshot) = request(&base, "GET", "/snapshot", Value::Null).await;
    assert!(snapshot["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .all(|task| task["column"].is_null()));
    let (_, history) = request(&base, "GET", "/history?source=TODO.md", Value::Null).await;
    assert!(history.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn defaults_and_start_readonly_server_refuses_preview_and_apply() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("TODO.md");
    let original = start_document();
    fs::write(&target, &original).unwrap();
    let base = start(dir.path(), true).await;
    let (_, options) = request(&base, "GET", "/sources/columns?source=TODO.md", Value::Null).await;
    for route in ["/sources/columns/preview", "/sources/columns"] {
        let (status, error) = request(
            &base,
            "POST",
            route,
            json!({
                "source_path":"TODO.md", "base_hash":options["base_hash"],
                "selection":{"mode":"defaults_and_start", "task_id":"start01"}
            }),
        )
        .await;
        assert_eq!(status, 403, "{route}: {error}");
        assert!(error["error"].as_str().unwrap().contains("read-only"));
    }
    assert_eq!(fs::read_to_string(&target).unwrap(), original);
    assert!(!dir.path().join(".todo").exists());
    assert!(!dir.path().join(".TODO.md.lock").exists());
}

#[tokio::test]
async fn defaults_and_start_rejects_invalid_completed_assigned_and_cross_source_task_ids() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("docs")).unwrap();
    let original = start_document();
    let assigned = "---\ntodo-spec: 1\ncolumns:\n  - {id: custom, name: Existing}\n---\n## Existing\n- [ ] assigned task <!--t:start01-->\n";
    fs::write(dir.path().join("docs/TODO.md"), &original).unwrap();
    fs::write(dir.path().join("TODO.md"), DOC).unwrap();
    fs::write(dir.path().join("TODO.txt"), assigned).unwrap();
    let base = start(dir.path(), false).await;
    for (source, task_id, expected) in [
        ("docs/TODO.md", "", 400),
        ("docs/TODO.md", "short", 400),
        ("docs/TODO.md", "START01", 400),
        ("docs/TODO.md", "../start01", 400),
        ("docs/TODO.md", "start01\n", 400),
        ("docs/TODO.md", "[\"docs/TODO.md\",\"start01\"]", 400),
        ("docs/TODO.md", "absent1", 404),
        ("docs/TODO.md", "finish1", 400),
        ("TODO.txt", "start01", 400),
        ("TODO.md", "start01", 404),
    ] {
        let (_, options) = request(
            &base,
            "GET",
            &format!("/sources/columns?source={}", query_value(source)),
            Value::Null,
        )
        .await;
        for route in ["/sources/columns/preview", "/sources/columns"] {
            let (status, error) = request(
                &base,
                "POST",
                route,
                json!({
                    "source_path":source, "base_hash":options["base_hash"],
                    "selection":{"mode":"defaults_and_start", "task_id":task_id}
                }),
            )
            .await;
            assert_eq!(status, expected, "{route}, {source}, {task_id:?}: {error}");
            assert_eq!(error["ok"], false);
        }
    }
    for selection in [
        json!({"mode":"defaults_and_start"}),
        json!({"mode":"defaults_and_start", "taskId":"start01"}),
        json!({"mode":"defaults_and_start", "task_id":7}),
    ] {
        for route in ["/sources/columns/preview", "/sources/columns"] {
            let (status, _) = request(
                &base,
                "POST",
                route,
                json!({"source_path":"docs/TODO.md", "base_hash":"unused", "selection":selection}),
            )
            .await;
            assert_eq!(status, 422, "{route}: {selection}");
        }
    }
    assert_eq!(
        fs::read_to_string(dir.path().join("docs/TODO.md")).unwrap(),
        original
    );
    assert_eq!(fs::read_to_string(dir.path().join("TODO.md")).unwrap(), DOC);
    assert_eq!(
        fs::read_to_string(dir.path().join("TODO.txt")).unwrap(),
        assigned
    );
    assert!(!dir.path().join("docs/.todo").exists());
    assert!(!dir.path().join(".todo").exists());
}
