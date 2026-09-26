//! Native column setup uses the same serialized contract as HTTP, with a
//! mandatory project binding even for options and preview reads.

use serde_json::json;
use std::{fs, time::Duration};
use todo_core::{CaptureStore, ColumnSetupSelection, DesktopBackend};
use todo_desktop_lib::ops::{wire, DesktopOps};

const DOC: &str = "---\ntodo-spec: 1\nvendor-note: 'keep: this'\n---\n\n# Project tasks\n\n## Domain\n- [ ] **Keep** `original` task <!--t:aaaaaa--> !1\n  ↳ existing note\n";
const START_BLOCK: &str = "- [ ] **Start** `only this` <!--t:start01--> #api !2\n  ↳ preserve note <!--n:start01-->\n\n  Keep [linked detail](https://example.invalid/detail) with the task.\n  - [ ] nested work <!--t:child01-->\n  - [x] completed child <!--t:child02-->\n";

fn start_document() -> String {
    format!(
        "---\ntodo-spec: 1\nvendor-note: 'keep: this'\n---\n\n# Project tasks\n\n## Domain\n{START_BLOCK}\n- [ ] sibling stays <!--t:other01-->\n\n## Archive\n- [x] finished stays <!--t:finish1-->\n"
    )
}

fn ops(inbox: &std::path::Path) -> DesktopOps {
    DesktopOps::new(
        DesktopBackend::default(),
        CaptureStore::new(inbox, Duration::from_secs(2)),
    )
}

#[test]
fn options_preview_and_apply_reject_a_stale_project_even_when_the_sources_are_identical() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    for root in [first.path(), second.path()] {
        fs::write(root.join("TODO.md"), DOC).unwrap();
    }
    let ops = ops(&first.path().join("inbox.md"));
    let original = ops.open_project(first.path().to_str().unwrap()).unwrap();
    let selection: ColumnSetupSelection =
        serde_json::from_value(json!({"mode":"defaults"})).unwrap();
    let preview = ops
        .preview_columns("TODO.md", &selection, &original.project_path)
        .unwrap();
    ops.open_project(second.path().to_str().unwrap()).unwrap();

    assert!(ops
        .column_setup_options("TODO.md", &original.project_path)
        .unwrap_err()
        .contains("project changed"));
    assert!(ops
        .preview_columns("TODO.md", &selection, &original.project_path)
        .unwrap_err()
        .contains("project changed"));
    assert!(ops
        .apply_columns(
            "TODO.md",
            &preview.base_hash,
            &selection,
            &original.project_path,
        )
        .unwrap_err()
        .contains("project changed"));
    for root in [first.path(), second.path()] {
        assert_eq!(fs::read_to_string(root.join("TODO.md")).unwrap(), DOC);
        assert!(!root.join(".todo").exists());
        assert!(!root.join(".TODO.md.lock").exists());
    }
}

#[test]
fn native_default_setup_serializes_the_agreed_wire_contract_and_only_writes_the_selected_source() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("docs")).unwrap();
    fs::write(dir.path().join("TODO.md"), DOC).unwrap();
    let target = dir.path().join("docs/TODO.md");
    fs::write(&target, DOC).unwrap();
    let ops = ops(&dir.path().join("inbox.md"));
    let project = ops.open_project(dir.path().to_str().unwrap()).unwrap();
    let root = &project.project_path;
    let options = wire(&ops.column_setup_options("docs/TODO.md", root).unwrap());
    assert_eq!(options["source_path"], "docs/TODO.md");
    assert_eq!(options["configured"], false);
    assert!(!options["base_hash"].as_str().unwrap().is_empty());
    assert_eq!(
        options["headings"],
        json!([{"name":"Domain", "line":8, "selectable":true, "reason":null}])
    );
    let selection: ColumnSetupSelection =
        serde_json::from_value(json!({"mode":"defaults"})).unwrap();
    let preview = ops
        .preview_columns("docs/TODO.md", &selection, root)
        .unwrap();
    let value = wire(&preview);
    assert_eq!(value["source_path"], "docs/TODO.md");
    assert_eq!(value["base_hash"], options["base_hash"]);
    assert_eq!(value["selection"], json!({"mode":"defaults"}));
    assert_eq!(
        value["columns"],
        json!([
            {"id":"todo", "name":"待办", "done":false, "created":true},
            {"id":"doing", "name":"进行中", "done":false, "created":true},
            {"id":"done", "name":"已完成", "done":true, "created":true}
        ])
    );
    assert!(value["diff"]["added"].as_u64().unwrap() > 0);
    assert_eq!(fs::read_to_string(&target).unwrap(), DOC);
    assert_eq!(fs::read_to_string(dir.path().join("TODO.md")).unwrap(), DOC);
    assert!(!dir.path().join("docs/.todo").exists());
    assert!(!dir.path().join("docs/.TODO.md.lock").exists());

    let snapshot = wire(
        &ops.apply_columns("docs/TODO.md", &preview.base_hash, &selection, root)
            .unwrap(),
    );
    assert_eq!(snapshot["project_path"], project.project_path);
    let columns = snapshot["columns"].as_array().unwrap();
    assert_eq!(columns.len(), 3);
    assert!(columns
        .iter()
        .all(|column| column["source_path"] == "docs/TODO.md"));
    assert_eq!(snapshot["tasks"].as_array().unwrap().len(), 2);
    assert!(snapshot["tasks"].as_array().unwrap().iter().all(|task| {
        task["id"] == "aaaaaa" && task["completed"] == false && task["column"].is_null()
    }));
    assert_eq!(fs::read_to_string(&target).unwrap(), preview.proposed);
    assert_eq!(fs::read_to_string(dir.path().join("TODO.md")).unwrap(), DOC);
    assert_eq!(
        ops.history_list_source(Some("docs/TODO.md")).unwrap().len(),
        1
    );
    assert!(ops.history_list_source(Some("TODO.md")).unwrap().is_empty());
    assert!(
        ops.column_setup_options("docs/TODO.md", root)
            .unwrap()
            .configured
    );
}

#[test]
fn native_columns_require_the_exact_nonempty_current_project_root() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("TODO.md"), DOC).unwrap();
    let ops = ops(&dir.path().join("inbox.md"));
    let project = ops.open_project(dir.path().to_str().unwrap()).unwrap();
    let selection: ColumnSetupSelection =
        serde_json::from_value(json!({"mode":"defaults"})).unwrap();
    let preview = ops
        .preview_columns("TODO.md", &selection, &project.project_path)
        .unwrap();
    for expected in ["", ".", &format!("{}/.", project.project_path)] {
        assert!(ops.column_setup_options("TODO.md", expected).is_err());
        assert!(ops
            .preview_columns("TODO.md", &selection, expected)
            .is_err());
        assert!(ops
            .apply_columns("TODO.md", &preview.base_hash, &selection, expected)
            .is_err());
    }
    assert_eq!(fs::read_to_string(dir.path().join("TODO.md")).unwrap(), DOC);
    assert!(!dir.path().join(".todo").exists());
    assert!(!dir.path().join(".TODO.md.lock").exists());
}

#[test]
fn native_existing_selection_survives_the_wire_and_stale_hashes_preserve_external_edits() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("TODO.md");
    fs::write(&target, DOC).unwrap();
    let ops = ops(&dir.path().join("inbox.md"));
    let project = ops.open_project(dir.path().to_str().unwrap()).unwrap();
    let root = &project.project_path;
    let selection: ColumnSetupSelection = serde_json::from_value(json!({
        "mode":"existing", "headings":[{"name":"Domain", "done":true}]
    }))
    .unwrap();
    let preview = ops.preview_columns("TODO.md", &selection, root).unwrap();
    assert_eq!(wire(&preview)["selection"], wire(&selection));
    assert_eq!(wire(&preview)["columns"][0]["name"], "Domain");
    assert_eq!(wire(&preview)["columns"][0]["created"], false);
    let external = format!("{DOC}\nAn external edit.\n");
    fs::write(&target, &external).unwrap();
    assert!(ops
        .apply_columns("TODO.md", &preview.base_hash, &selection, root)
        .unwrap_err()
        .contains("changed"));
    assert_eq!(fs::read_to_string(&target).unwrap(), external);
    assert!(ops.history_list_source(Some("TODO.md")).unwrap().is_empty());

    let refreshed = ops.preview_columns("TODO.md", &selection, root).unwrap();
    let snapshot = wire(
        &ops.apply_columns("TODO.md", &refreshed.base_hash, &selection, root)
            .unwrap(),
    );
    assert_eq!(snapshot["columns"][0]["name"], "Domain");
    assert_eq!(snapshot["columns"][0]["done"], true);
    assert_eq!(snapshot["tasks"][0]["completed"], false);
    assert_eq!(fs::read_to_string(&target).unwrap(), refreshed.proposed);
    assert!(ops
        .apply_columns("TODO.md", &refreshed.base_hash, &selection, root)
        .unwrap_err()
        .contains("changed"));
    assert_eq!(ops.history_list_source(Some("TODO.md")).unwrap().len(), 1);
}

#[test]
fn native_column_operations_refuse_invalid_unmanaged_and_outside_sources() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("TODO.md"), DOC).unwrap();
    fs::write(dir.path().join("TODO.txt"), "- [ ] not adopted\n").unwrap();
    fs::write(outside.path().join("TODO.md"), DOC).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), dir.path().join("linked")).unwrap();
    let ops = ops(&dir.path().join("inbox.md"));
    let project = ops.open_project(dir.path().to_str().unwrap()).unwrap();
    let root = &project.project_path;
    let selection = ColumnSetupSelection::Defaults;
    let absolute = outside.path().join("TODO.md");
    for source in [
        "",
        "../TODO.md",
        "docs/../TODO.md",
        "docs\\TODO.md",
        "./TODO.md",
        "missing.md",
        "TODO.txt",
        "linked/TODO.md",
        absolute.to_str().unwrap(),
    ] {
        assert!(
            ops.column_setup_options(source, root).is_err(),
            "{source:?}"
        );
        assert!(
            ops.preview_columns(source, &selection, root).is_err(),
            "{source:?}"
        );
        assert!(
            ops.apply_columns(source, "unused", &selection, root)
                .is_err(),
            "{source:?}"
        );
    }
    assert_eq!(fs::read_to_string(dir.path().join("TODO.md")).unwrap(), DOC);
    assert_eq!(
        fs::read_to_string(dir.path().join("TODO.txt")).unwrap(),
        "- [ ] not adopted\n"
    );
    assert_eq!(fs::read_to_string(&absolute).unwrap(), DOC);
    assert!(!dir.path().join(".todo").exists());
    assert!(!dir.path().join(".TODO.md.lock").exists());
}

#[test]
fn native_readonly_source_allows_options_but_refuses_preview_and_apply_without_side_effects() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("TODO.md");
    fs::write(&target, DOC).unwrap();
    let ops = ops(&dir.path().join("inbox.md"));
    let project = ops.open_project(dir.path().to_str().unwrap()).unwrap();
    let original_permissions = fs::metadata(&target).unwrap().permissions();
    let mut readonly = original_permissions.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&target, readonly).unwrap();
    let root = &project.project_path;
    let options = ops.column_setup_options("TODO.md", root);
    let preview = ops.preview_columns("TODO.md", &ColumnSetupSelection::Defaults, root);
    let apply = ops.apply_columns("TODO.md", "unused", &ColumnSetupSelection::Defaults, root);
    fs::set_permissions(&target, original_permissions).unwrap();

    assert!(!options.unwrap().configured);
    assert!(preview.unwrap_err().contains("read-only"));
    assert!(apply.unwrap_err().contains("read-only"));
    assert_eq!(fs::read_to_string(&target).unwrap(), DOC);
    assert!(!dir.path().join(".todo").exists());
    assert!(!dir.path().join(".TODO.md.lock").exists());
}

#[test]
fn native_defaults_and_start_moves_only_the_named_source_block_in_one_history_entry() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("docs")).unwrap();
    let original = start_document();
    let target = dir.path().join("docs/TODO.md");
    fs::write(&target, &original).unwrap();
    fs::write(dir.path().join("TODO.md"), &original).unwrap();
    let ops = ops(&dir.path().join("inbox.md"));
    let project = ops.open_project(dir.path().to_str().unwrap()).unwrap();
    let root = &project.project_path;
    let selection: ColumnSetupSelection = serde_json::from_value(json!({
        "mode":"defaults_and_start", "task_id":"start01"
    }))
    .unwrap();
    let preview = ops
        .preview_columns("docs/TODO.md", &selection, root)
        .unwrap();
    let value = wire(&preview);
    assert_eq!(
        value["selection"],
        json!({"mode":"defaults_and_start", "task_id":"start01"})
    );
    assert_eq!(value["source_path"], "docs/TODO.md");
    assert_eq!(
        value["columns"],
        json!([
            {"id":"todo", "name":"待办", "done":false, "created":true},
            {"id":"doing", "name":"进行中", "done":false, "created":true},
            {"id":"done", "name":"已完成", "done":true, "created":true}
        ])
    );
    assert_eq!(preview.proposed.matches(START_BLOCK).count(), 1);
    let doing = preview.proposed.split_once("## 进行中\n").unwrap().1;
    assert!(doing
        .split_once("## 已完成\n")
        .unwrap()
        .0
        .contains(START_BLOCK));
    assert!(preview.proposed.contains("## Domain\n"));
    assert!(preview
        .proposed
        .contains("- [ ] sibling stays <!--t:other01-->\n"));
    assert!(preview
        .proposed
        .contains("## Archive\n- [x] finished stays <!--t:finish1-->\n"));
    assert!(preview.proposed.contains("vendor-note: 'keep: this'\n"));
    assert!(value["diff"]["added"].as_u64().unwrap() > 0);
    assert!(value["diff"]["removed"].as_u64().unwrap() > 0);
    // Cancel is simply not applying: preview creates no file or metadata writes.
    assert_eq!(fs::read_to_string(&target).unwrap(), original);
    assert_eq!(
        fs::read_to_string(dir.path().join("TODO.md")).unwrap(),
        original
    );
    assert!(!dir.path().join("docs/.todo").exists());
    assert!(!dir.path().join("docs/.TODO.md.lock").exists());

    let snapshot = wire(
        &ops.apply_columns("docs/TODO.md", &preview.base_hash, &selection, root)
            .unwrap(),
    );
    assert_eq!(fs::read_to_string(&target).unwrap(), preview.proposed);
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
            if moving {
                json!("doing")
            } else {
                serde_json::Value::Null
            }
        );
        assert_eq!(
            task["completed"],
            matches!(task["id"].as_str(), Some("child02" | "finish1"))
        );
    }
    let history = ops.history_list_source(Some("docs/TODO.md")).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(
        ops.history_preview_source(history[0].version, Some("docs/TODO.md"))
            .unwrap()
            .proposed,
        original
    );
    assert!(ops.history_list_source(Some("TODO.md")).unwrap().is_empty());
}

#[test]
fn native_defaults_and_start_refuses_stale_hash_and_project_without_partial_setup() {
    let dir = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let original = start_document();
    for root in [dir.path(), other.path()] {
        fs::write(root.join("TODO.md"), &original).unwrap();
    }
    let ops = ops(&dir.path().join("inbox.md"));
    let project = ops.open_project(dir.path().to_str().unwrap()).unwrap();
    let selection: ColumnSetupSelection = serde_json::from_value(json!({
        "mode":"defaults_and_start", "task_id":"start01"
    }))
    .unwrap();
    let preview = ops
        .preview_columns("TODO.md", &selection, &project.project_path)
        .unwrap();
    let external = format!("{original}\nExternal edit wins.\n");
    fs::write(dir.path().join("TODO.md"), &external).unwrap();
    assert!(ops
        .apply_columns(
            "TODO.md",
            &preview.base_hash,
            &selection,
            &project.project_path
        )
        .unwrap_err()
        .contains("changed"));
    assert_eq!(
        fs::read_to_string(dir.path().join("TODO.md")).unwrap(),
        external
    );
    assert!(
        !ops.column_setup_options("TODO.md", &project.project_path)
            .unwrap()
            .configured
    );
    assert!(ops
        .snapshot()
        .unwrap()
        .tasks
        .iter()
        .all(|task| task.column.is_none()));
    assert!(ops.history_list_source(Some("TODO.md")).unwrap().is_empty());

    ops.open_project(other.path().to_str().unwrap()).unwrap();
    assert!(ops
        .preview_columns("TODO.md", &selection, &project.project_path)
        .unwrap_err()
        .contains("project changed"));
    assert!(ops
        .apply_columns(
            "TODO.md",
            &preview.base_hash,
            &selection,
            &project.project_path
        )
        .unwrap_err()
        .contains("project changed"));
    assert_eq!(
        fs::read_to_string(other.path().join("TODO.md")).unwrap(),
        original
    );
    assert!(!other.path().join(".todo").exists());
    assert!(!other.path().join(".TODO.md.lock").exists());
}

#[test]
fn native_defaults_and_start_rejects_readonly_source_for_both_preview_and_apply() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("TODO.md");
    let original = start_document();
    fs::write(&target, &original).unwrap();
    let ops = ops(&dir.path().join("inbox.md"));
    let project = ops.open_project(dir.path().to_str().unwrap()).unwrap();
    let selection: ColumnSetupSelection = serde_json::from_value(json!({
        "mode":"defaults_and_start", "task_id":"start01"
    }))
    .unwrap();
    let hash = ops
        .column_setup_options("TODO.md", &project.project_path)
        .unwrap()
        .base_hash;
    let permissions = fs::metadata(&target).unwrap().permissions();
    let mut readonly = permissions.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&target, readonly).unwrap();
    let preview = ops.preview_columns("TODO.md", &selection, &project.project_path);
    let apply = ops.apply_columns("TODO.md", &hash, &selection, &project.project_path);
    fs::set_permissions(&target, permissions).unwrap();
    assert!(preview.unwrap_err().contains("read-only"));
    assert!(apply.unwrap_err().contains("read-only"));
    assert_eq!(fs::read_to_string(&target).unwrap(), original);
    assert!(!dir.path().join(".todo").exists());
    assert!(!dir.path().join(".TODO.md.lock").exists());
}

#[test]
fn native_defaults_and_start_refuses_malformed_completed_assigned_and_cross_source_ids() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("docs")).unwrap();
    let original = start_document();
    let assigned = "---\ntodo-spec: 1\ncolumns:\n  - {id: custom, name: Existing}\n---\n## Existing\n- [ ] assigned task <!--t:start01-->\n";
    fs::write(dir.path().join("docs/TODO.md"), &original).unwrap();
    fs::write(dir.path().join("TODO.md"), DOC).unwrap();
    fs::write(dir.path().join("TODO.txt"), assigned).unwrap();
    let ops = ops(&dir.path().join("inbox.md"));
    let project = ops.open_project(dir.path().to_str().unwrap()).unwrap();
    for (source, task_id) in [
        ("docs/TODO.md", ""),
        ("docs/TODO.md", "short"),
        ("docs/TODO.md", "START01"),
        ("docs/TODO.md", "../start01"),
        ("docs/TODO.md", "start01\n"),
        ("docs/TODO.md", "[\"docs/TODO.md\",\"start01\"]"),
        ("docs/TODO.md", "absent1"),
        ("docs/TODO.md", "finish1"),
        ("TODO.txt", "start01"),
        ("TODO.md", "start01"),
    ] {
        let selection: ColumnSetupSelection = serde_json::from_value(json!({
            "mode":"defaults_and_start", "task_id":task_id
        }))
        .unwrap();
        let hash = ops
            .column_setup_options(source, &project.project_path)
            .unwrap()
            .base_hash;
        assert!(
            ops.preview_columns(source, &selection, &project.project_path)
                .is_err(),
            "{source}: {task_id:?}"
        );
        assert!(
            ops.apply_columns(source, &hash, &selection, &project.project_path)
                .is_err(),
            "{source}: {task_id:?}"
        );
    }
    for payload in [
        json!({"mode":"defaults_and_start"}),
        json!({"mode":"defaults_and_start", "taskId":"start01"}),
        json!({"mode":"defaults_and_start", "task_id":7}),
    ] {
        assert!(serde_json::from_value::<ColumnSetupSelection>(payload).is_err());
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
