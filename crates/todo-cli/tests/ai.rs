//! `todo ai` against a temporary store. Every test points `TODO_AI_STORE` at a
//! tempdir so nothing here reads or writes the developer's `~/.todo/ai/`.

use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};

use tempfile::tempdir;

const SPEC_EXAMPLE: &str = r#"{
  "ai-feed-spec": 1,
  "source": "dsh",
  "session_id": "s_8f2a",
  "workspace": "/Users/lens/lens077/ToDo",
  "title": "为 CLI 增加 release 构建说明",
  "state": "unread",
  "outcome": "completed",
  "question": "补一段 cargo install 的说明",
  "reply": "已在 README 快速开始加入 cargo install --path …",
  "changed_files": ["README.md"],
  "open_url": "http://127.0.0.1:3080/?session=s_8f2a",
  "event_at": 1757550000000
}"#;

struct Harness {
    store: std::path::PathBuf,
    cwd: tempfile::TempDir,
}

impl Harness {
    fn new() -> Self {
        let cwd = tempdir().unwrap();
        Self {
            store: cwd.path().join("ai-store").join("sessions.json"),
            cwd,
        }
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_todo"));
        command
            .env("TODO_AI_STORE", &self.store)
            .current_dir(self.cwd.path())
            .args(args);
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }

    fn run_stdin(&self, args: &[&str], input: &str) -> Output {
        let mut child = self
            .command(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }

    fn json(&self, args: &[&str]) -> serde_json::Value {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "todo {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }

    fn upsert_flags(&self, session: &str, workspace: &str, state: &str, event_at: i64) {
        let event_at = event_at.to_string();
        let output = self.run(&[
            "ai",
            "upsert",
            "--source",
            "dsh",
            "--session",
            session,
            "--workspace",
            workspace,
            "--state",
            state,
            "--event-at",
            &event_at,
        ]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn ids(value: &serde_json::Value) -> Vec<String> {
    value["data"]["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|record| record["session_id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn stdin_upsert_round_trips_through_json_envelope() {
    let h = Harness::new();
    let output = h.run_stdin(&["ai", "upsert", "--stdin", "--json"], SPEC_EXAMPLE);
    assert!(output.status.success());
    let body: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(body["schema"], "todo.ai.upsert.v1");
    assert_eq!(body["ok"], true);
    assert_eq!(body["data"]["unchanged"], false);
    let record = &body["data"]["record"];
    assert_eq!(record["source"], "dsh");
    assert_eq!(record["session_id"], "s_8f2a");
    assert_eq!(record["state"], "unread");
    assert_eq!(record["outcome"], "completed");
    assert_eq!(record["changed_file_count"], 1);
    assert_eq!(record["event_at"], 1_757_550_000_000_i64);
    assert!(record["received_at"].as_i64().unwrap() > 0);
    assert_eq!(record["acked_at"], serde_json::Value::Null);
    assert_eq!(record["hidden"], false);

    let stored: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&h.store).unwrap()).unwrap();
    assert_eq!(stored["ai-feed-spec"], 1);
    assert_eq!(stored["sessions"].as_array().unwrap().len(), 1);

    let got = h.json(&["ai", "get", "dsh", "s_8f2a", "--json"]);
    assert_eq!(got["schema"], "todo.ai.record.v1");
    assert_eq!(got["data"]["record"], *record);

    let human = h.run(&["ai", "list"]);
    let text = String::from_utf8(human.stdout).unwrap();
    assert!(
        text.starts_with("[unread] dsh/s_8f2a  为 CLI 增加 release 构建说明  (ToDo)  1 file"),
        "{text}"
    );
    assert!(text.contains("↳ 补一段 cargo install 的说明"), "{text}");
}

#[test]
fn stale_event_is_reported_unchanged_and_equal_event_overwrites() {
    let h = Harness::new();
    h.upsert_flags("s1", "/work/project", "unread", 200);
    let stale = h.json(&[
        "ai",
        "upsert",
        "--source",
        "dsh",
        "--session",
        "s1",
        "--workspace",
        "/work/project",
        "--state",
        "running",
        "--event-at",
        "100",
        "--json",
    ]);
    assert_eq!(stale["data"]["unchanged"], true);
    assert_eq!(stale["data"]["record"]["state"], "unread");

    let human = h.run(&[
        "ai",
        "upsert",
        "--source",
        "dsh",
        "--session",
        "s1",
        "--workspace",
        "/work/project",
        "--state",
        "running",
        "--event-at",
        "100",
    ]);
    assert_eq!(
        String::from_utf8_lossy(&human.stdout).trim(),
        "unchanged dsh/s1 (stored event is newer)"
    );

    let same = h.json(&[
        "ai",
        "upsert",
        "--source",
        "dsh",
        "--session",
        "s1",
        "--workspace",
        "/work/project",
        "--state",
        "seen",
        "--event-at",
        "200",
        "--json",
    ]);
    assert_eq!(same["data"]["unchanged"], false);
    assert_eq!(same["data"]["record"]["state"], "seen");
}

#[test]
fn contract_violations_exit_2_and_write_nothing() {
    let h = Harness::new();
    let bad_spec = SPEC_EXAMPLE.replace("\"ai-feed-spec\": 1", "\"ai-feed-spec\": 2");
    let output = h.run_stdin(&["ai", "upsert", "--stdin"], &bad_spec);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("ai-feed-spec"));

    let output = h.run_stdin(&["ai", "upsert", "--stdin", "--json"], "{not json");
    assert_eq!(output.status.code(), Some(2));
    let body: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"]["code"], 2);

    let unknown_state = SPEC_EXAMPLE.replace("\"unread\"", "\"snoozed\"");
    let output = h.run_stdin(&["ai", "upsert", "--stdin"], &unknown_state);
    assert_eq!(output.status.code(), Some(2));

    let relative_workspace = SPEC_EXAMPLE.replace("/Users/lens/lens077/ToDo", "relative/dir");
    let output = h.run_stdin(&["ai", "upsert", "--stdin"], &relative_workspace);
    assert_eq!(output.status.code(), Some(2));

    let missing_required = r#"{"ai-feed-spec":1,"source":"dsh","state":"unread"}"#;
    let output = h.run_stdin(&["ai", "upsert", "--stdin"], missing_required);
    assert_eq!(output.status.code(), Some(2));

    assert!(
        !h.store.exists(),
        "a rejected record must not create the store"
    );
    assert!(!h.store.parent().unwrap().exists());
}

#[test]
fn flag_form_upsert_validates_enums_and_requires_fields() {
    let h = Harness::new();
    let output = h.run(&[
        "ai",
        "upsert",
        "--source",
        "dsh",
        "--session",
        "s1",
        "--workspace",
        "/work/project",
        "--state",
        "failed",
        "--outcome",
        "max-tokens",
        "--title",
        "迁移脚本",
        "--question",
        "为什么失败？",
        "--reply",
        "上下文超出上限",
        "--reply-truncated",
        "--changed-file",
        "a.rs",
        "--changed-file-count",
        "5",
        "--open-url",
        "https://example.test/s1",
        "--json",
    ]);
    assert!(output.status.success());
    let body: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let record = &body["data"]["record"];
    assert_eq!(record["state"], "failed");
    assert_eq!(record["outcome"], "max-tokens");
    assert_eq!(record["reply_truncated"], true);
    assert_eq!(record["changed_files"], serde_json::json!(["a.rs"]));
    assert_eq!(record["changed_file_count"], 5);
    assert_eq!(record["open_url"], "https://example.test/s1");
    // --event-at omitted: defaults to now, and now is after the example's 2025 timestamp.
    assert!(record["event_at"].as_i64().unwrap() > 1_757_550_000_000);

    let human = h.run(&[
        "ai",
        "upsert",
        "--source",
        "dsh",
        "--session",
        "s2",
        "--workspace",
        "/work/project",
        "--state",
        "running",
    ]);
    assert_eq!(
        String::from_utf8_lossy(&human.stdout).trim(),
        "upserted dsh/s2 → running"
    );

    let bad_state = h.run(&[
        "ai",
        "upsert",
        "--source",
        "dsh",
        "--session",
        "s3",
        "--workspace",
        "/work/project",
        "--state",
        "snoozed",
    ]);
    assert_eq!(bad_state.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&bad_state.stderr);
    for state in ["waiting", "failed", "unread", "running", "seen", "handled"] {
        assert!(stderr.contains(state), "{stderr}");
    }

    let bad_outcome = h.run(&[
        "ai",
        "upsert",
        "--source",
        "dsh",
        "--session",
        "s3",
        "--workspace",
        "/work/project",
        "--state",
        "failed",
        "--outcome",
        "exploded",
    ]);
    assert_eq!(bad_outcome.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&bad_outcome.stderr).contains("max-tokens"));

    let missing = h.run(&["ai", "upsert", "--source", "dsh"]);
    assert_eq!(missing.status.code(), Some(2));

    let conflict = h.run(&["ai", "upsert", "--stdin", "--source", "dsh"]);
    assert_eq!(conflict.status.code(), Some(2));

    assert!(h.run(&["ai", "get", "dsh", "s3"]).status.code() == Some(1));
}

#[test]
fn list_filters_by_workspace_state_and_all() {
    let h = Harness::new();
    h.upsert_flags("a", "/work/project", "running", 500);
    h.upsert_flags("b", "/work/project", "unread", 100);
    h.upsert_flags("c", "/work/project", "unread", 300);
    h.upsert_flags("d", "/work/project", "waiting", 50);
    h.upsert_flags("e", "/work/other", "waiting", 999);
    h.upsert_flags("f", "/work/project/sub", "seen", 10);
    h.upsert_flags("g", "/work/project", "handled", 400);

    let all_visible = h.json(&["ai", "list", "--json"]);
    assert_eq!(all_visible["schema"], "todo.ai.list.v1");
    assert_eq!(ids(&all_visible), ["e", "d", "c", "b", "a", "f"]);

    let project = h.json(&["ai", "list", "--workspace", "/work/project", "--json"]);
    assert_eq!(ids(&project), ["d", "c", "b", "a", "f"]);

    let unread = h.json(&["ai", "list", "--state", "unread", "--json"]);
    assert_eq!(ids(&unread), ["c", "b"]);

    let two_states = h.json(&[
        "ai", "list", "--state", "unread", "--state", "waiting", "--json",
    ]);
    assert_eq!(ids(&two_states), ["e", "d", "c", "b"]);

    let everything = h.json(&["ai", "list", "--all", "--json"]);
    assert_eq!(ids(&everything), ["e", "d", "c", "b", "a", "f", "g"]);

    let handled_only = h.json(&["ai", "list", "--all", "--state", "handled", "--json"]);
    assert_eq!(ids(&handled_only), ["g"]);

    let bad_state = h.run(&["ai", "list", "--state", "snoozed"]);
    assert_eq!(bad_state.status.code(), Some(2));
}

#[test]
fn ack_remove_and_link_write_local_marks_that_survive_upserts() {
    let h = Harness::new();
    h.upsert_flags("s1", "/work/project", "unread", 100);

    let acked = h.json(&["ai", "ack", "dsh", "s1", "--json"]);
    assert_eq!(acked["schema"], "todo.ai.record.v1");
    assert!(acked["data"]["record"]["acked_at"].as_i64().unwrap() > 0);
    assert_eq!(
        String::from_utf8_lossy(&h.run(&["ai", "ack", "dsh", "s1"]).stdout).trim(),
        "acked dsh/s1"
    );

    let linked = h.json(&["ai", "link", "dsh", "s1", "kq3n8w", "--json"]);
    assert_eq!(linked["data"]["record"]["task_id"], "kq3n8w");
    assert_eq!(
        String::from_utf8_lossy(&h.run(&["ai", "link", "dsh", "s1", "kq3n8w"]).stdout).trim(),
        "linked dsh/s1 → kq3n8w"
    );

    // Newer activity clears the ack but keeps the link.
    h.upsert_flags("s1", "/work/project", "unread", 200);
    let got = h.json(&["ai", "get", "dsh", "s1", "--json"]);
    assert_eq!(got["data"]["record"]["acked_at"], serde_json::Value::Null);
    assert_eq!(got["data"]["record"]["task_id"], "kq3n8w");

    let removed = h.json(&["ai", "remove", "dsh", "s1", "--json"]);
    assert_eq!(removed["data"]["record"]["hidden"], true);
    assert_eq!(
        String::from_utf8_lossy(&h.run(&["ai", "remove", "dsh", "s1"]).stdout).trim(),
        "removed dsh/s1"
    );
    assert!(ids(&h.json(&["ai", "list", "--json"])).is_empty());
    h.upsert_flags("s1", "/work/project", "failed", 300);
    assert!(ids(&h.json(&["ai", "list", "--json"])).is_empty());
    assert_eq!(ids(&h.json(&["ai", "list", "--all", "--json"])), ["s1"]);

    for verb in ["get", "ack", "remove"] {
        let missing = h.run(&["ai", verb, "dsh", "nope"]);
        assert_eq!(missing.status.code(), Some(1), "{verb}");
        assert!(String::from_utf8_lossy(&missing.stderr).contains("dsh/nope"));
    }
    assert_eq!(
        h.run(&["ai", "link", "dsh", "nope", "kq3n8w"])
            .status
            .code(),
        Some(1)
    );
}

#[test]
fn prune_drops_only_old_handled_or_hidden_records() {
    let h = Harness::new();
    let day = 86_400_000_i64;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    h.upsert_flags("old-handled", "/work/project", "handled", now - 40 * day);
    h.upsert_flags("new-handled", "/work/project", "handled", now - 5 * day);
    h.upsert_flags("old-open", "/work/project", "unread", now - 40 * day);
    h.upsert_flags("old-hidden", "/work/project", "unread", now - 40 * day);
    assert!(h
        .run(&["ai", "remove", "dsh", "old-hidden"])
        .status
        .success());

    let pruned = h.json(&["ai", "prune", "--json"]);
    assert_eq!(pruned["schema"], "todo.ai.prune.v1");
    assert_eq!(pruned["data"]["removed"], 2);
    assert_eq!(pruned["data"]["remaining"], 2);
    assert!(h.run(&["ai", "get", "dsh", "old-open"]).status.success());
    assert!(h.run(&["ai", "get", "dsh", "new-handled"]).status.success());
    assert_eq!(
        h.run(&["ai", "get", "dsh", "old-handled"]).status.code(),
        Some(1)
    );

    let again = h.run(&["ai", "prune", "--older-than", "1"]);
    assert_eq!(
        String::from_utf8_lossy(&again.stdout).trim(),
        "pruned 1, 1 remaining"
    );
}

#[test]
fn ai_commands_do_not_need_a_task_file() {
    let h = Harness::new();
    assert!(!Path::new(h.cwd.path()).join("TODO.md").exists());
    // Control: a project command in the same directory is "not initialized".
    assert_eq!(h.run(&["list"]).status.code(), Some(3));

    let listed = h.run(&["ai", "list"]);
    assert!(listed.status.success());
    assert_eq!(
        String::from_utf8_lossy(&listed.stdout).trim(),
        "no ai sessions"
    );
    let listed = h.json(&["ai", "list", "--json"]);
    assert_eq!(listed["data"]["sessions"], serde_json::json!([]));

    h.upsert_flags("s1", "/work/project", "unread", 1);
    assert!(h.run(&["ai", "prune"]).status.success());
    assert!(h.run(&["ai", "get", "dsh", "s1"]).status.success());
    assert!(!Path::new(h.cwd.path()).join("TODO.md").exists());
}
