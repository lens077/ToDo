//! `sync::run` 集成测试：起一个真实的 virgo-sync 子进程做全量对账。
//! 不依赖外网——解析失败只产生 stub，不影响对账断言。

use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use todo_core::{ReadingContextInput, ReadingStore};

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// virgo-sync 在另一个 workspace 成员里；`CARGO_BIN_EXE` 只覆盖本包 bin，
/// 这里按 workspace target 目录定位，缺失时兜底构建一次。
fn server_bin() -> std::path::PathBuf {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.ancestors().nth(3).expect("workspace root");
    let bin = root.join("target/debug/virgo-sync");
    if !bin.exists() {
        let status = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
            .args(["build", "-p", "sync-server", "--locked", "--quiet"])
            .status()
            .expect("cargo build sync-server");
        assert!(status.success(), "failed to build virgo-sync for the test");
    }
    bin
}

fn spawn_server(data_dir: &std::path::Path, port: u16) -> Server {
    let mut child = Command::new(server_bin())
        .env("VIRGO_SYNC_DATA_DIR", data_dir)
        .env("VIRGO_SYNC_TOKEN", "test-token")
        .env("VIRGO_SYNC_BIND", format!("127.0.0.1:{port}"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn virgo-sync");
    let client = reqwest::blocking::Client::new();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("server did not come up on port {port}");
        }
        if client
            .get(format!("http://127.0.0.1:{port}/healthz"))
            .timeout(Duration::from_millis(500))
            .send()
            .map(|response| response.status().is_success())
            .unwrap_or(false)
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Server(child)
}

fn post_json(
    port: u16,
    path: &str,
    token: &str,
    body: serde_json::Value,
) -> reqwest::blocking::Response {
    reqwest::blocking::Client::new()
        .post(format!("http://127.0.0.1:{port}{path}"))
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
        .json(&body)
        .send()
        .expect("post")
}

fn server_urls(port: u16, token: &str) -> HashMap<String, String> {
    let text = reqwest::blocking::Client::new()
        .get(format!("http://127.0.0.1:{port}/articles"))
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .expect("list")
        .text()
        .expect("body");
    let value: serde_json::Value = serde_json::from_str(&text).expect("json");
    value["articles"]
        .as_array()
        .expect("articles")
        .iter()
        .map(|article| {
            (
                article["url"].as_str().expect("url").to_string(),
                article["id"].as_str().expect("id").to_string(),
            )
        })
        .collect()
}

fn context(notes: &str) -> ReadingContextInput {
    ReadingContextInput {
        saved_reason: String::new(),
        focus_questions: String::new(),
        revisit_condition: String::new(),
        personal_notes: notes.to_string(),
        revisit_type: String::new(),
    }
}

#[test]
fn sync_roundtrip_between_local_store_and_server() {
    let server_dir = tempfile::tempdir().expect("server dir");
    let local_dir = tempfile::tempdir().expect("local dir");
    let port = 18127;
    let _server = spawn_server(server_dir.path(), port);
    let base = format!("http://127.0.0.1:{port}");

    // 服务器侧已有一篇（解析会失败，stub 不影响对账）。
    let response = post_json(
        port,
        "/links",
        "test-token",
        serde_json::json!({
            "url": "https://server-only.example/b",
            "context": { "personal_notes": "服务器侧备注" }
        }),
    );
    assert!(response.status().is_success(), "server seed failed");

    // 本地侧已有一篇。
    let store = ReadingStore::new(local_dir.path());
    let local = store
        .add_failed("https://local-only.example/a")
        .expect("local add");
    store
        .update_context(&local.article.id, &context("本地侧备注"))
        .expect("local context");

    let report = todo_desktop_lib::sync::run(&store, &base, "test-token").expect("sync run");
    assert!(report.pushed >= 1, "expected a push: {report:?}");
    assert!(report.pulled >= 1, "expected a pull: {report:?}");

    // 下行：服务器独有的 URL 出现在本地库，且备注随行。
    let pulled = store
        .list()
        .expect("list")
        .into_iter()
        .find(|article| article.url == "https://server-only.example/b")
        .expect("pulled article");
    let detail = store.get(&pulled.id).expect("detail");
    assert_eq!(detail.context.personal_notes, "服务器侧备注");

    // 上行：本地独有的 URL 出现在服务器，备注随行。
    let urls = server_urls(port, "test-token");
    assert!(urls.contains_key("https://local-only.example/a"));
    let pushed: serde_json::Value = reqwest::blocking::Client::new()
        .get(format!(
            "http://127.0.0.1:{port}/articles/{}",
            urls["https://local-only.example/a"]
        ))
        .header(reqwest::header::AUTHORIZATION, "Bearer test-token")
        .send()
        .expect("detail")
        .json()
        .expect("json");
    assert_eq!(pushed["context"]["personal_notes"], "本地侧备注");

    // 二次对账幂等：不再有新的上行/下行。
    let second = todo_desktop_lib::sync::run(&store, &base, "test-token").expect("second sync");
    assert_eq!(second.pushed, 0, "no push on second sync");
    assert_eq!(second.pulled, 0, "no pull on second sync");
}
