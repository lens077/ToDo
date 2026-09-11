//! The AI feed commands (`ai_list`, `ai_ack`, `ai_remove`, `ai_link`,
//! `ai_to_task`) as `desktop.ts` sees them: serialized shapes and the marks
//! they leave in the store and in the task file.

use std::{fs, time::Duration};

use todo_core::{AiOutcome, AiState, AiStore, AiUpsert, CaptureStore, DesktopBackend};
use todo_desktop_lib::ops::{wire, DesktopOps};

const FIXTURE: &str = "---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n  - {id: doing, name: Doing}\n  - {id: done, name: Done, type: done}\n---\n\n## Todo\n- [ ] first task <!--t:aaa111-->\n\n## Doing\n\n## Done\n";

struct Fixture {
    dir: tempfile::TempDir,
    ops: DesktopOps,
    store: AiStore,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("TODO.md"), FIXTURE).unwrap();
    let store_path = dir.path().join("ai").join("sessions.json");
    let store = AiStore::new(&store_path, Duration::from_secs(2));
    let ops = DesktopOps::new(
        DesktopBackend::new(Duration::from_secs(2)),
        CaptureStore::new(dir.path().join("inbox.md"), Duration::from_secs(2)),
    )
    .with_ai_store(AiStore::new(&store_path, Duration::from_secs(2)));
    Fixture { dir, ops, store }
}

fn record(
    session: &str,
    workspace: &str,
    state: AiState,
    title: Option<&str>,
    event_at: i64,
) -> AiUpsert {
    AiUpsert {
        spec: 1,
        source: "dsh".into(),
        session_id: session.into(),
        workspace: workspace.into(),
        state,
        outcome: matches!(state, AiState::Unread).then_some(AiOutcome::Completed),
        title: title.map(str::to_owned),
        question: Some("补一段 cargo install 的说明\n第二行".into()),
        reply: None,
        reply_truncated: false,
        changed_files: vec![],
        changed_file_count: None,
        open_url: Some(format!("http://127.0.0.1:3080/?session={session}")),
        event_at: Some(event_at),
    }
}

#[test]
fn ai_list_reports_connection_and_includes_handled_and_hidden_records() {
    let fx = fixture();
    let value = wire(&fx.ops.ai_list(None).unwrap());
    assert_eq!(value["connected"], false, "no host has published yet");
    assert_eq!(value["records"].as_array().unwrap().len(), 0);

    let root = fx.dir.path().to_str().unwrap();
    fx.store
        .upsert(record("s_a", root, AiState::Handled, Some("done"), 1))
        .unwrap();
    fx.store
        .upsert(record(
            "s_b",
            &format!("{root}/sub"),
            AiState::Waiting,
            None,
            2,
        ))
        .unwrap();
    fx.store
        .upsert(record("s_c", "/elsewhere", AiState::Unread, None, 3))
        .unwrap();
    fx.ops.ai_remove("dsh", "s_c").unwrap();

    let value = wire(&fx.ops.ai_list(None).unwrap());
    assert_eq!(value["connected"], true);
    let ids: Vec<&str> = value["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["session_id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        ["s_b", "s_c", "s_a"],
        "the view filters pills and hidden itself"
    );
    assert_eq!(value["records"][1]["hidden"], true);
    assert_eq!(value["records"][0]["state"], "waiting");
    assert_eq!(value["records"][2]["outcome"], serde_json::Value::Null);

    let scoped = wire(&fx.ops.ai_list(Some(root)).unwrap());
    let ids: Vec<&str> = scoped["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["session_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["s_b", "s_a"]);
}

#[test]
fn ack_and_link_leave_local_marks_and_keep_the_host_state() {
    let fx = fixture();
    let root = fx.dir.path().to_str().unwrap();
    fx.store
        .upsert(record("s_a", root, AiState::Unread, None, 1))
        .unwrap();
    let acked = fx.ops.ai_ack("dsh", "s_a").unwrap();
    assert!(acked.acked_at.is_some());
    assert_eq!(acked.state, AiState::Unread);
    let linked = fx.ops.ai_link("dsh", "s_a", "aaa111").unwrap();
    assert_eq!(linked.task_id.as_deref(), Some("aaa111"));
    assert!(fx.ops.ai_link("dsh", "s_a", " ").is_err());
    assert!(fx.ops.ai_ack("dsh", "missing").is_err());
}

#[test]
fn to_task_creates_a_task_from_the_title_or_question_and_writes_the_session_note() {
    let fx = fixture();
    let root = fx.dir.path().to_str().unwrap();
    fx.ops.open_project(root).unwrap();
    fx.store
        .upsert(record(
            "s_t",
            root,
            AiState::Unread,
            Some("为 CLI 增加 release 构建说明"),
            1,
        ))
        .unwrap();
    fx.store
        .upsert(record("s_q", root, AiState::Unread, None, 2))
        .unwrap();

    let value = wire(
        &fx.ops
            .ai_to_task("dsh", "s_t", Some("doing"), None, None)
            .unwrap(),
    );
    let task_id = value["task_id"].as_str().unwrap().to_string();
    assert_eq!(value["record"]["task_id"], task_id);
    assert_eq!(value["snapshot"]["status"], "available");
    let created = value["snapshot"]["value"]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == task_id)
        .unwrap();
    assert_eq!(created["body"], "为 CLI 增加 release 构建说明");
    assert_eq!(created["column"], "doing");
    assert_eq!(
        created["note"],
        "来自 dsh 会话 s_t：http://127.0.0.1:3080/?session=s_t"
    );

    // Without a title the question becomes the body, folded onto one line.
    let value = wire(&fx.ops.ai_to_task("dsh", "s_q", None, None, None).unwrap());
    let task_id = value["task_id"].as_str().unwrap();
    let created = value["snapshot"]["value"]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == task_id)
        .unwrap();
    assert_eq!(created["body"], "补一段 cargo install 的说明 第二行");

    let file = fs::read_to_string(fx.dir.path().join("TODO.md")).unwrap();
    assert!(
        file.contains("↳ 来自 dsh 会话 s_t：http://127.0.0.1:3080/?session=s_t <!--n:"),
        "{file}"
    );

    // A stale project root is refused before anything is written.
    assert!(fx
        .ops
        .ai_to_task("dsh", "s_t", None, None, Some("/somewhere/else"))
        .is_err());
    assert!(fx
        .ops
        .ai_to_task("dsh", "missing", None, None, None)
        .is_err());
}
