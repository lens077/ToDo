//! End-to-end contract tests for the desktop command layer.
//!
//! These drive the same `DesktopOps` the `#[tauri::command]` handlers call, against
//! real Markdown files, and assert the **serialized JSON** rather than Rust values.
//! That is deliberate: every desktop defect found so far in this project was a
//! shape mismatch between the Rust response and the TypeScript decoder in
//! `apps/desktop/src/lib/desktop.ts`, not a logic error. Asserting Rust structs
//! would not have caught a single one of them.
//!
//! Each assertion below names the exact field the bridge reads.

use std::{fs, time::Duration};

use serde_json::json;
use tempfile::tempdir;
use todo_core::{CaptureStore, DesktopBackend, DesktopIntent, MoveTarget};
use todo_desktop_lib::ops::{wire, DesktopOps};

const FIXTURE: &str = "---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n  - {id: doing, name: Doing}\n  - {id: done, name: Done, type: done}\n---\n\n# Project tasks\n\n## Todo\n- [ ] first task <!--t:aaa111--> #core !1\n- [ ] second task <!--t:bbb222-->\n\n## Doing\n\n## Done\n";

struct Fixture {
    _dir: tempfile::TempDir,
    ops: DesktopOps,
    project: std::path::PathBuf,
    inbox: std::path::PathBuf,
}

fn fixture() -> Fixture {
    let dir = tempdir().unwrap();
    let project = dir.path().join("TODO.md");
    fs::write(&project, FIXTURE).unwrap();
    let inbox = dir.path().join("inbox.md");
    // The Capture store is pointed at a temp path so tests never touch the
    // user's real ~/.todo/inbox.md.
    let ops = DesktopOps::new(
        DesktopBackend::new(Duration::from_secs(2)),
        CaptureStore::new(&inbox, Duration::from_secs(2)),
    );
    Fixture {
        _dir: dir,
        ops,
        project,
        inbox,
    }
}

#[test]
fn snapshot_wire_shape_matches_the_typescript_decoder() {
    let fx = fixture();
    let snapshot = fx.ops.open_project(fx.project.to_str().unwrap()).unwrap();
    let value = wire(&snapshot);

    // desktop.ts normalizeSnapshot() rejects the payload unless these are strings.
    assert!(
        value["project_path"].is_string(),
        "project_path must be a string"
    );
    assert!(value["revision"].is_string(), "revision must be a string");

    // Columns feed the board and the triage picker.
    let columns = value["columns"].as_array().unwrap();
    assert_eq!(columns.len(), 3);
    assert_eq!(columns[0]["id"], "todo");
    assert_eq!(columns[0]["name"], "Todo");
    assert_eq!(columns[2]["done"], true, "done column must be flagged");

    // TaskCard reads exactly these keys.
    let task = &value["tasks"][0];
    for key in [
        "id",
        "body",
        "completed",
        "column",
        "due",
        "priority",
        "tags",
    ] {
        assert!(task.get(key).is_some(), "task is missing `{key}`");
    }
    assert_eq!(task["id"], "aaa111");
    assert_eq!(task["completed"], false);
    assert_eq!(task["priority"], 1);
    assert_eq!(task["tags"][0], "core");

    // TodayView always ships all three buckets, even when empty, so the UI can
    // render group headers without null checks.
    for bucket in ["overdue", "today", "upcoming"] {
        assert!(
            value["today"][bucket].is_array(),
            "today.{bucket} must be an array"
        );
    }
}

#[test]
fn mutation_intents_deserialize_from_the_exact_frontend_payloads() {
    let fx = fixture();
    fx.ops.open_project(fx.project.to_str().unwrap()).unwrap();

    // These literals are copied from the DesktopMutation union in desktop.ts.
    // If either side renames a field or a tag, this test fails.
    let add: DesktopIntent =
        serde_json::from_value(json!({"type": "add_task", "body": "from ui", "column": "todo"}))
            .expect("add_task payload must deserialize");
    let snapshot = fx.ops.mutate(add).unwrap();
    assert!(
        snapshot.tasks.iter().any(|task| task.body == "from ui"),
        "add_task must return an authoritative snapshot containing the new task"
    );

    let complete: DesktopIntent =
        serde_json::from_value(json!({"type": "set_completed", "id": "aaa111", "completed": true}))
            .expect("set_completed payload must deserialize");
    let snapshot = fx.ops.mutate(complete).unwrap();
    assert!(
        snapshot
            .tasks
            .iter()
            .find(|task| task.id.as_deref() == Some("aaa111"))
            .unwrap()
            .completed
    );

    let edited: DesktopIntent = serde_json::from_value(
        json!({"type": "set_body", "id": "aaa111", "body": "**edited** in `ui`"}),
    )
    .expect("set_body payload must deserialize");
    let snapshot = fx.ops.mutate(edited).unwrap();
    assert_eq!(
        snapshot
            .tasks
            .iter()
            .find(|task| task.id.as_deref() == Some("aaa111"))
            .unwrap()
            .body,
        "**edited** in `ui`"
    );

    // The move seam uses stable task IDs, never array indexes: a stale index
    // would move the wrong task after an external edit.
    let moved: DesktopIntent = serde_json::from_value(json!({
        "type": "move_task",
        "id": "bbb222",
        "column": "doing",
        "target": {"type": "column_end"}
    }))
    .expect("move_task payload must deserialize");
    let snapshot = fx.ops.mutate(moved).unwrap();
    assert_eq!(
        snapshot
            .tasks
            .iter()
            .find(|task| task.id.as_deref() == Some("bbb222"))
            .unwrap()
            .column
            .as_deref(),
        Some("doing")
    );

    // All four MoveTarget variants must round-trip; the UI can emit any of them.
    for (payload, expected) in [
        (json!({"type": "column_start"}), MoveTarget::ColumnStart),
        (json!({"type": "column_end"}), MoveTarget::ColumnEnd),
        (
            json!({"type": "before", "task_id": "aaa111"}),
            MoveTarget::Before {
                task_id: "aaa111".into(),
            },
        ),
        (
            json!({"type": "after", "task_id": "aaa111"}),
            MoveTarget::After {
                task_id: "aaa111".into(),
            },
        ),
    ] {
        let parsed: MoveTarget = serde_json::from_value(payload).unwrap();
        assert_eq!(parsed, expected);
    }
}

#[test]
fn mutations_are_written_through_to_markdown() {
    let fx = fixture();
    fx.ops.open_project(fx.project.to_str().unwrap()).unwrap();
    let before = fs::read_to_string(&fx.project).unwrap();

    fx.ops
        .mutate(DesktopIntent::SetCompleted {
            id: "aaa111".into(),
            completed: true,
            note: None,
            evidence: None,
        })
        .unwrap();

    let after = fs::read_to_string(&fx.project).unwrap();
    assert!(
        after.contains("- [x] first task"),
        "completion must persist"
    );

    // Markdown is the single source of truth, so an unrelated line must survive
    // byte for byte. This is the product's hard rule, not a nicety.
    assert!(
        after.contains("- [ ] second task <!--t:bbb222-->"),
        "untouched task line must be preserved verbatim"
    );
    assert_eq!(
        before.lines().count(),
        after.lines().count(),
        "a status toggle must not add or remove lines"
    );
}

#[test]
fn capture_append_and_delete_return_authoritative_lists() {
    let fx = fixture();

    let appended = fx.ops.capture_append("remember this").unwrap();
    let value = wire(&appended);
    // desktop.ts throws unless BOTH keys are present: it applies `captures` as
    // the authoritative list rather than patching local state.
    assert!(value["capture"]["id"].is_string());
    assert_eq!(value["capture"]["text"], "remember this");
    assert!(
        value["capture"]["created_at"]
            .as_str()
            .unwrap()
            .ends_with('Z'),
        "created_at must be RFC 3339 UTC"
    );
    assert_eq!(value["captures"].as_array().unwrap().len(), 1);

    let id = appended.capture.id.clone();
    let remaining = fx.ops.capture_delete(&id).unwrap();
    assert!(
        wire(&remaining).as_array().unwrap().is_empty(),
        "capture_delete must return the authoritative remaining list"
    );
}

#[test]
fn capture_survives_marker_like_and_multiline_text() {
    let fx = fixture();
    // A Capture that contains the store's own terminator must not corrupt the
    // file; the format is length-prefixed precisely to survive this.
    let hostile = "line one\n<!--/capture-->\nline two 🧪";
    let appended = fx.ops.capture_append(hostile).unwrap();
    assert_eq!(appended.capture.text, hostile);

    let listed = fx.ops.capture_list().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].text, hostile, "round trip must be lossless");
}

#[test]
fn corrupt_capture_file_is_rejected_instead_of_silently_dropping_records() {
    let fx = fixture();
    fx.ops.capture_append("real note").unwrap();
    let raw = fs::read_to_string(&fx.inbox).unwrap();
    // Truncate mid-record: a lenient parser would silently lose the user's note.
    fs::write(&fx.inbox, &raw[..raw.len() - 5]).unwrap();

    assert!(
        fx.ops.capture_list().is_err(),
        "a truncated inbox must surface an error, never silently drop captures"
    );
}

#[test]
fn triage_wire_shape_carries_task_id_and_command_values() {
    let fx = fixture();
    fx.ops.open_project(fx.project.to_str().unwrap()).unwrap();
    let appended = fx.ops.capture_append("promote me").unwrap();

    let response = fx
        .ops
        .capture_triage(&appended.capture.id, Some("todo"))
        .unwrap();
    let value = wire(&response);

    // desktop.ts reads `result` as a tagged union and REQUIRES a non-empty
    // task_id on both outcomes, so a partial success can never leave the user
    // unable to find the task that was already created.
    assert_eq!(value["result"]["status"], "completed");
    assert!(
        value["result"]["task_id"]
            .as_str()
            .is_some_and(|id| !id.is_empty()),
        "triage result must always carry a task_id"
    );

    // snapshot and captures are CommandValue-wrapped, not bare values.
    assert_eq!(value["snapshot"]["status"], "available");
    assert!(value["snapshot"]["value"]["project_path"].is_string());
    assert_eq!(value["captures"]["status"], "available");
    assert!(
        value["captures"]["value"].as_array().unwrap().is_empty(),
        "the triaged Capture must be gone from the authoritative list"
    );

    let task_id = value["result"]["task_id"].as_str().unwrap();
    let markdown = fs::read_to_string(&fx.project).unwrap();
    assert!(
        markdown.contains(task_id),
        "the created task must exist in the project file"
    );
    assert!(markdown.contains("promote me"));
}

#[test]
fn command_value_reports_unavailable_without_failing_the_whole_call() {
    // CommandValue exists so a follow-up read can fail without discarding the
    // primary result. The UI shows a warning banner instead of an error.
    let unavailable: todo_desktop_lib::CommandValue<i32> =
        todo_desktop_lib::CommandValue::from_result(Err::<i32, _>(std::io::Error::other("boom")));
    let value = wire(&unavailable);
    assert_eq!(value["status"], "unavailable");
    assert_eq!(value["message"], "boom");
    assert!(
        value.get("value").is_none(),
        "unavailable must not carry a value key"
    );
}

#[test]
fn desktop_status_wire_shape_is_a_tagged_union() {
    // The UI derives its own project/sync state; the backend only reports these
    // two facts. Inventing extra fields here previously broke the decoder.
    let value = wire(&todo_desktop_lib::DesktopStatus {
        startup_ms: Some(274),
        shortcut_status: todo_desktop_lib::ShortcutStatus::Registered,
    });
    assert_eq!(value["startup_ms"], 274);
    assert_eq!(value["shortcut_status"]["status"], "registered");

    let failed = wire(&todo_desktop_lib::DesktopStatus {
        startup_ms: None,
        shortcut_status: todo_desktop_lib::ShortcutStatus::Error {
            message: "already registered".into(),
        },
    });
    assert!(failed["startup_ms"].is_null());
    assert_eq!(failed["shortcut_status"]["status"], "error");
    assert_eq!(failed["shortcut_status"]["message"], "already registered");
}

#[test]
fn opening_a_non_task_file_is_reported_as_a_no_project_error() {
    let fx = fixture();
    let plain = fx.project.parent().unwrap().join("notes.md");
    fs::write(&plain, "# just notes\n").unwrap();

    let error = fx.ops.open_project(plain.to_str().unwrap()).unwrap_err();
    // desktop.ts classifyError() maps this text to `no_project`, which drives the
    // "choose a project" empty state rather than a scary error banner.
    assert!(
        error.to_lowercase().contains("not a todo.md task file"),
        "unexpected error text for the no-project path: {error}"
    );
}

#[test]
fn snapshot_before_opening_a_project_fails_cleanly() {
    let fx = fixture();
    assert!(
        fx.ops.snapshot().is_err(),
        "snapshot without an open project must error, not panic or return empty data"
    );
}

#[test]
fn contract_check_wire_shape_matches_the_typescript_decoder() {
    let fx = fixture();
    fx.ops.open_project(fx.project.to_str().unwrap()).unwrap();

    // No contract yet: the report is the VPC001 finding, not an error, so the
    // view can render evidence instead of a blank screen.
    let missing = wire(&fx.ops.contract_check().unwrap());
    assert_eq!(missing["contract_path"], serde_json::Value::Null);
    assert_eq!(missing["findings"][0]["rule"], "VPC001");
    assert_eq!(missing["findings"][0]["severity"], "error");
    assert!(missing["findings"][0]["message"].is_string());

    // Adopt the project (writes .virgo/project.yaml + integrations), then the
    // report must be clean and carry the contract path for the UI header.
    let dir = fx.project.parent().unwrap().to_path_buf();
    let plan = todo_core::plan_adoption(&dir, "fixture").unwrap();
    std::fs::create_dir_all(dir.join(".virgo")).unwrap();
    std::fs::write(dir.join(".virgo/project.yaml"), plan.proposed_contract).unwrap();
    std::fs::write(dir.join("AGENTS.md"), "# rules\n").unwrap();
    let adopted = wire(&fx.ops.contract_check().unwrap());
    assert!(
        adopted["contract_path"].is_string(),
        "contract_path must be a string for the adopted case"
    );
    assert_eq!(
        adopted["findings"].as_array().unwrap().len(),
        0,
        "adopted fixture should have no findings: {adopted}"
    );
}
