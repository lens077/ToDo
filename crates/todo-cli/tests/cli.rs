use std::{fs, process::Command};
use tempfile::tempdir;

fn todo(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_todo"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn init_add_list_done_next_and_json_are_scriptable() {
    let dir = tempdir().unwrap();
    assert!(todo(dir.path(), &["init", "--json"]).status.success());
    let added = todo(
        dir.path(),
        &[
            "add",
            "ship it",
            "--tag",
            "core",
            "--due",
            "2026-09-10",
            "--priority",
            "1",
            "--json",
        ],
    );
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let added_json: serde_json::Value = serde_json::from_slice(&added.stdout).unwrap();
    assert_eq!(added_json["schema"], "todo.mutation.v1");
    let id = added_json["data"]["task"]["id"].as_str().unwrap();
    let listed: serde_json::Value =
        serde_json::from_slice(&todo(dir.path(), &["list", "--json"]).stdout).unwrap();
    assert_eq!(listed["schema"], "todo.list.v1");
    assert_eq!(listed["data"]["tasks"].as_array().unwrap().len(), 1);
    let next: serde_json::Value =
        serde_json::from_slice(&todo(dir.path(), &["next", "--json"]).stdout).unwrap();
    assert_eq!(next["data"]["task"]["id"], id);
    assert!(todo(dir.path(), &["done", id, "--json"]).status.success());
    let listed: serde_json::Value =
        serde_json::from_slice(&todo(dir.path(), &["list", "--json"]).stdout).unwrap();
    assert!(listed["data"]["tasks"].as_array().unwrap().is_empty());
    assert!(todo(dir.path(), &["undone", id, "--json"]).status.success());
}

#[test]
fn done_removes_stale_partial_note_but_preserves_evidence_and_dependencies() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("TODO.md");
    fs::write(
        &path,
        "---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n---\n\n## Todo\n- [ ] ship <!--t:abc123--> 🔗abc1234..def5678 ⛔dep123 ◐80 ⚠2026-09-01\n      ↳ load test still failing <!--n:abc123-->\n",
    )
    .unwrap();

    let output = todo(dir.path(), &["done", "abc123"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(path).unwrap();

    assert!(updated.contains("- [x] ship <!--t:abc123--> 🔗abc1234..def5678 ⛔dep123"));
    assert!(!updated.contains("◐80"));
    assert!(!updated.contains("⚠2026-09-01"));
    assert!(!updated.contains("load test still failing"));
    assert!(!updated.contains("<!--n:abc123-->"));
}

#[test]
fn init_never_overwrites_without_explicit_adopt() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("TODO.md");
    fs::write(&path, "- [ ] existing\n").unwrap();
    let refused = todo(dir.path(), &["init"]);
    assert_eq!(refused.status.code(), Some(1));
    assert_eq!(fs::read_to_string(&path).unwrap(), "- [ ] existing\n");
    assert!(todo(dir.path(), &["init", "--adopt"]).status.success());
    let adopted = fs::read_to_string(path).unwrap();
    assert!(adopted.contains("todo-spec: 1"));
    assert!(adopted.contains("<!--t:"));
}

#[test]
fn adopt_preserves_fenced_task_examples_byte_for_byte() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("TODO.md");
    let backtick_fence = "```markdown\n- [ ] example with trailing spaces  \n```\n";
    let tilde_fence = "~~~~text\r\n- [x] another example\r\n~~~~\r\n";
    let source = format!("- [ ] real task\n{backtick_fence}{tilde_fence}");
    fs::write(&path, &source).unwrap();

    let output = todo(dir.path(), &["init", "--adopt"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let adopted = fs::read_to_string(path).unwrap();

    assert!(adopted.contains(backtick_fence));
    assert!(adopted.contains(tilde_fence));
    assert_eq!(adopted.matches("<!--t:").count(), 1);
}

/// Regression: `init` used to check `path.exists()` and then `fs::write`, so two
/// processes could both pass the check and both write. Measured at 5/40 rounds
/// before the fix. The invariant is that exactly one caller may succeed.
#[test]
fn concurrent_init_has_exactly_one_winner() {
    for _ in 0..12 {
        let dir = tempdir().unwrap();
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let path = dir.path().to_path_buf();
                std::thread::spawn(move || {
                    Command::new(env!("CARGO_BIN_EXE_todo"))
                        .current_dir(&path)
                        .arg("init")
                        .output()
                        .unwrap()
                        .status
                        .success()
                })
            })
            .collect();
        let winners = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .filter(|succeeded| *succeeded)
            .count();
        assert_eq!(winners, 1, "exactly one init must create the file");
        let created = fs::read_to_string(dir.path().join("TODO.md")).unwrap();
        assert_eq!(created, TODO_TEMPLATE_EXPECTED);
    }
}

/// Regression: `init --adopt` read the file outside any lock, so a concurrent
/// adopt could overwrite it with a snapshot taken before the other write.
/// Measured losing real user tasks in 2/40 rounds before the fix.
#[test]
fn concurrent_adopt_never_loses_user_tasks() {
    for _ in 0..12 {
        let dir = tempdir().unwrap();
        let path = dir.path().join("TODO.md");
        fs::write(&path, "- [ ] keep A\n- [ ] keep B\n").unwrap();
        let workers: Vec<_> = (0..6)
            .map(|_| {
                let dir_path = dir.path().to_path_buf();
                std::thread::spawn(move || {
                    Command::new(env!("CARGO_BIN_EXE_todo"))
                        .current_dir(&dir_path)
                        .args(["init", "--adopt"])
                        .output()
                        .unwrap();
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
        let adopted = fs::read_to_string(&path).unwrap();
        assert!(adopted.contains("keep A"), "lost user task A: {adopted}");
        assert!(adopted.contains("keep B"), "lost user task B: {adopted}");
        assert!(adopted.contains("todo-spec: 1"));
    }
}

const TODO_TEMPLATE_EXPECTED: &str = "---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n  - {id: doing, name: Doing}\n  - {id: done, name: Done, type: done}\n---\n\n# Project tasks\n\n## Todo\n\n## Doing\n\n## Done\n";

#[test]
fn lint_has_stable_exit_six_and_json_issues() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("TODO.md"),
        "---\ntodo-spec: 1\ncolumns: []\n---\n- [/] waiting\n",
    )
    .unwrap();
    let output = todo(dir.path(), &["lint", "--json"]);
    assert_eq!(output.status.code(), Some(6));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], "todo.lint.v1");
    assert_eq!(value["data"]["issues"][0]["code"], "nonstandard-checkbox");
}

#[test]
fn lint_reports_duplicate_stable_ids() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("TODO.md"),
        "---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n---\n\n## Todo\n- [ ] first <!--t:abc123-->\n- [ ] second <!--t:abc123-->\n",
    )
    .unwrap();

    let output = todo(dir.path(), &["lint", "--json"]);
    assert_eq!(output.status.code(), Some(6));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let issue = value["data"]["issues"]
        .as_array()
        .unwrap()
        .iter()
        .find(|issue| issue["code"] == "duplicate-id")
        .expect("duplicate ID must be a lint issue");
    assert_eq!(issue["severity"], "error");
    assert_eq!(issue["line"], 9);
    assert!(issue["message"].as_str().unwrap().contains("abc123"));
}

#[test]
fn reconcile_reports_a_skipped_evidence_layer_instead_of_claiming_success() {
    // Outside a git repository the evidence layer cannot run. Exiting 0 here would
    // tell CI the repository was checked and clean, which is the one thing this run
    // is not entitled to say.
    let dir = tempdir().unwrap();
    assert!(todo(dir.path(), &["init"]).status.success());
    let before = fs::read(dir.path().join("TODO.md")).unwrap();
    let output = todo(dir.path(), &["reconcile", "--json", "--no-semantic"]);

    assert_eq!(
        output.status.code(),
        Some(7),
        "incomplete run must not exit 0"
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], "todo.reconcile.v2");
    assert!(value["data"]["report"]["confirmed"]
        .as_array()
        .unwrap()
        .is_empty());
    let skipped = value["data"]["report"]["checks_skipped"]
        .as_array()
        .unwrap();
    assert!(
        skipped
            .iter()
            .any(|line| line.as_str().unwrap().contains("git")),
        "the skipped git evidence check must be named: {skipped:?}"
    );
    // Report-only: the file is never touched.
    assert_eq!(fs::read(dir.path().join("TODO.md")).unwrap(), before);
}

#[test]
fn reconcile_human_output_never_says_clean_when_a_check_was_skipped() {
    let dir = tempdir().unwrap();
    assert!(todo(dir.path(), &["init"]).status.success());
    let output = todo(dir.path(), &["reconcile", "--no-semantic"]);
    let text = String::from_utf8(output.stdout).unwrap();

    assert!(text.contains("INCOMPLETE"), "got: {text}");
    assert!(text.contains("NOT CHECKED"), "got: {text}");
    assert!(
        !text.contains("no reconciliation findings (all checks ran)"),
        "must not claim a complete run: {text}"
    );
}

#[test]
fn reconcile_apply_does_not_apply_suspected_findings() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("TODO.md"),
        "---\ntodo-spec: 1\ncolumns: []\n---\n- [/] waiting\n",
    )
    .unwrap();
    let before = fs::read(dir.path().join("TODO.md")).unwrap();
    let output = todo(dir.path(), &["reconcile", "--apply", "--json"]);
    assert_eq!(output.status.code(), Some(6));
    assert_eq!(fs::read(dir.path().join("TODO.md")).unwrap(), before);
}

#[test]
fn reconcile_apply_preserves_confirmed_evidence_without_action() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("TODO.md"),
        "---\ntodo-spec: 1\ncolumns: []\n---\n- [ ] task <!--t:aaaaaa--> ⛔missing\n- [/] suspected\n",
    )
    .unwrap();
    let output = todo(
        dir.path(),
        &["reconcile", "--apply", "--json", "--no-semantic"],
    );
    assert_eq!(output.status.code(), Some(6));
    let after = fs::read_to_string(dir.path().join("TODO.md")).unwrap();
    assert!(after.contains("⛔missing"));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["data"]["applied"], 0);
    assert_eq!(value["data"]["findings_requiring_manual_action"], 2);
    assert_eq!(
        value["data"]["report"]["confirmed"][0]["suggested_action"],
        serde_json::Value::Null
    );
    assert!(after.contains("- [/] suspected"));
    assert!(after.contains("- [ ] task"));
}

#[test]
fn missing_file_is_exit_three_and_usage_is_exit_two() {
    let dir = tempdir().unwrap();
    assert_eq!(todo(dir.path(), &["list", "--json"]).status.code(), Some(3));
    assert_eq!(todo(dir.path(), &["nope"]).status.code(), Some(2));
}

#[test]
fn restore_without_yes_previews_the_diff_and_writes_nothing() {
    // The confirmation step is the whole safety mechanism for bulk rewrites: a
    // restore replaces the entire file, so a preview must never touch disk.
    let dir = tempdir().unwrap();
    assert!(todo(dir.path(), &["init"]).status.success());
    assert!(todo(dir.path(), &["add", "first"]).status.success());
    let after_add = fs::read_to_string(dir.path().join("TODO.md")).unwrap();

    let preview = todo(dir.path(), &["restore", "1", "--json"]);
    assert!(preview.status.success());
    let value: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert_eq!(value["schema"], "todo.diff.v1");
    assert_eq!(value["data"]["applied"], false);
    assert!(
        value["data"]["diff"]["added"].as_u64().unwrap() > 0,
        "the preview must describe a real change"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("TODO.md")).unwrap(),
        after_add,
        "a preview must not write"
    );

    let applied = todo(dir.path(), &["restore", "1", "--yes", "--json"]);
    assert!(applied.status.success());
    assert_ne!(
        fs::read_to_string(dir.path().join("TODO.md")).unwrap(),
        after_add
    );
}

#[test]
fn history_records_the_operation_behind_each_restore_point() {
    let dir = tempdir().unwrap();
    assert!(todo(dir.path(), &["init"]).status.success());
    assert!(todo(dir.path(), &["add", "first"]).status.success());
    let listed = todo(dir.path(), &["list", "--json"]);
    let value: serde_json::Value = serde_json::from_slice(&listed.stdout).unwrap();
    let id = value["data"]["tasks"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(todo(dir.path(), &["progress", &id, "60"]).status.success());

    let output = todo(dir.path(), &["history", "--json"]);
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], "todo.history.v1");
    let ops: Vec<&str> = value["data"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["op"].as_str().unwrap())
        .collect();
    assert!(ops.contains(&"progress"), "got {ops:?}");
    assert!(ops.contains(&"add"), "got {ops:?}");
}
