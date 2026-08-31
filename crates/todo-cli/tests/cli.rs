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
fn reconcile_defaults_to_report_only_and_non_git_degrades_cleanly() {
    let dir = tempdir().unwrap();
    assert!(todo(dir.path(), &["init"]).status.success());
    let before = fs::read(dir.path().join("TODO.md")).unwrap();
    let output = todo(dir.path(), &["reconcile", "--json", "--no-semantic"]);
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], "todo.reconcile.v1");
    assert!(value["data"]["report"]["confirmed"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(value["data"]["report"]["suspected"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(fs::read(dir.path().join("TODO.md")).unwrap(), before);
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
    assert_eq!(value["data"]["skipped_manual"], 2);
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
