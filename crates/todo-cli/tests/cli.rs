use std::{fs, process::Command};
use tempfile::tempdir;

fn todo(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_todo"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

fn git(dir: &std::path::Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(dir)
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
fn priority_sets_clears_and_updates_a_subtree() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("TODO.md"),
        "---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n---\n\n## Todo\n- [ ] parent <!--t:abc123--> !1\n    - [ ] child <!--t:def456--> !3\n",
    )
    .unwrap();

    let changed = todo(
        dir.path(),
        &["priority", "abc123", "4", "--subtree", "--json"],
    );
    assert!(
        changed.status.success(),
        "{}",
        String::from_utf8_lossy(&changed.stderr)
    );
    let listed: serde_json::Value =
        serde_json::from_slice(&todo(dir.path(), &["list", "--json"]).stdout).unwrap();
    assert!(listed["data"]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .all(|task| task["priority"] == 4));

    let cleared = todo(dir.path(), &["priority", "abc123", "--clear", "--json"]);
    assert!(cleared.status.success());
    let listed: serde_json::Value =
        serde_json::from_slice(&todo(dir.path(), &["list", "--json"]).stdout).unwrap();
    assert_eq!(
        listed["data"]["tasks"][0]["priority"],
        serde_json::Value::Null
    );
    assert_eq!(listed["data"]["tasks"][1]["priority"], 4);

    assert!(!todo(dir.path(), &["priority", "abc123", "5"])
        .status
        .success());
}

#[test]
fn completion_policy_enforces_the_note_matrix() {
    for (policy, command, succeeds) in [
        ("normal", vec!["done", "abc123"], true),
        ("normal", vec!["progress", "abc123", "50"], false),
        ("normal", vec!["fail", "abc123"], false),
        ("strict", vec!["done", "abc123"], false),
        ("strict", vec!["progress", "abc123", "50"], false),
        ("strict", vec!["fail", "abc123"], false),
    ] {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("TODO.md"),
            format!("---\ntodo-spec: 1\ncompletion-policy: {policy}\ncolumns:\n  - {{id: todo, name: Todo}}\n---\n\n## Todo\n- [ ] work <!--t:abc123-->\n"),
        )
        .unwrap();
        let output = todo(dir.path(), &command);
        assert_eq!(output.status.success(), succeeds, "{policy} {command:?}");
        if !succeeds {
            assert!(String::from_utf8_lossy(&output.stderr).contains("requires a note"));
        }
    }

    for (policy, command) in [
        (
            "normal",
            vec!["progress", "abc123", "50", "--note", "half done"],
        ),
        ("normal", vec!["fail", "abc123", "--note", "attempt failed"]),
        (
            "strict",
            vec!["done", "abc123", "--note", "verified complete"],
        ),
        (
            "strict",
            vec!["progress", "abc123", "50", "--note", "half done"],
        ),
        ("strict", vec!["fail", "abc123", "--note", "attempt failed"]),
    ] {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("TODO.md"),
            format!("---\ntodo-spec: 1\ncompletion-policy: {policy}\ncolumns:\n  - {{id: todo, name: Todo}}\n---\n\n## Todo\n- [ ] work <!--t:abc123-->\n"),
        )
        .unwrap();
        let output = todo(dir.path(), &command);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn strict_done_requires_the_current_command_to_supply_a_note() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("TODO.md");
    let original = "---\ntodo-spec: 1\ncompletion-policy: strict\ncolumns:\n  - {id: todo, name: Todo}\n---\n\n## Todo\n- [ ] work <!--t:abc123-->\n      ↳ stale progress context <!--n:abc123-->\n";
    fs::write(&path, original).unwrap();

    let output = todo(dir.path(), &["done", "abc123"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("requires a note"));
    assert_eq!(fs::read_to_string(path).unwrap(), original);
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
fn progress_can_be_cleared_without_claiming_completion() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("TODO.md");
    let original = "---\ntodo-spec: 1\ncompletion-policy: strict\ncolumns:\n  - {id: todo, name: Todo}\n---\n\n## Todo\n- [ ] ship <!--t:abc123--> ◐80\n      ↳ load test still failing <!--n:abc123-->\n";
    fs::write(&path, original).unwrap();

    let output = todo(dir.path(), &["progress", "abc123", "--clear"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(path).unwrap();

    assert!(!updated.contains("◐80"));
    assert!(updated.contains("- [ ] ship <!--t:abc123-->"));
    assert!(updated.contains("load test still failing"));
}

#[test]
fn progress_requires_a_percent_or_clear() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("TODO.md");
    let original = "---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n---\n\n## Todo\n- [ ] ship <!--t:abc123-->\n";
    fs::write(&path, original).unwrap();

    for args in [
        vec!["progress", "abc123"],
        vec!["progress", "abc123", "80", "--clear"],
    ] {
        let output = todo(dir.path(), &args);
        assert!(!output.status.success(), "{args:?} should be rejected");
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }
}

#[test]
fn archive_moves_completed_tasks_with_evidence_and_records_history() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("TODO.md");
    let archived_block = "- [x] shipped <!--t:done01--> 🔗abc1234..def5678 ⛔dep123 ✅2026-09-01\n      ↳ keep this audit context <!--n:done01-->\n";
    fs::write(
        &path,
        format!(
            "---\ntodo-spec: 1\ncolumns:\n  - {{id: todo, name: Todo}}\n  - {{id: done, name: Done, type: done}}\n---\n\n## Todo\n- [ ] active <!--t:open01-->\n\n## Done\n{archived_block}"
        ),
    )
    .unwrap();

    let output = todo(dir.path(), &["archive", "--json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], "todo.archive.v1");
    assert_eq!(value["data"]["archived"], 1);

    let active = fs::read_to_string(&path).unwrap();
    assert!(active.contains("active <!--t:open01-->"));
    assert!(!active.contains("done01"));
    assert!(!active.contains("🔗abc1234..def5678"));

    let archive_path = dir.path().join("TODO.archive.md");
    let archive = fs::read_to_string(&archive_path).unwrap();
    assert!(archive.contains(archived_block));
    assert!(archive.contains("🔗abc1234..def5678"));
    assert!(archive.contains("⛔dep123"));

    let history: serde_json::Value =
        serde_json::from_slice(&todo(dir.path(), &["history", "--json"]).stdout).unwrap();
    let entries = history["data"]["entries"].as_array().unwrap();
    assert!(entries
        .iter()
        .any(|entry| entry["op"] == "archive" && entry["file"] == "TODO.md"));
    assert!(entries
        .iter()
        .any(|entry| entry["op"] == "archive" && entry["file"] == "TODO.archive.md"));

    let repeated = todo(dir.path(), &["archive", "--json"]);
    assert!(repeated.status.success());
    let repeated_value: serde_json::Value = serde_json::from_slice(&repeated.stdout).unwrap();
    assert_eq!(repeated_value["data"]["archived"], 0);
    assert_eq!(fs::read_to_string(archive_path).unwrap(), archive);
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
fn adopt_maps_custom_headings_to_todo_so_tasks_remain_visible() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("TODO.md");
    fs::write(
        &path,
        "# Project\n\n## Backlog\n- [ ] visible after adoption\n\n## Someday\n- [x] completed item\n",
    )
    .unwrap();

    assert!(todo(dir.path(), &["init", "--adopt"]).status.success());
    let listed = todo(dir.path(), &["list"]);
    assert!(listed.status.success());
    assert!(String::from_utf8_lossy(&listed.stdout).contains("visible after adoption"));

    let adopted = fs::read_to_string(path).unwrap();
    assert!(adopted.contains("### Backlog"));
    assert!(adopted.contains("### Someday"));
    assert_eq!(adopted.matches("<!--t:").count(), 2);
}

#[test]
fn adopt_merges_plain_frontmatter_without_losing_custom_bytes() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("TODO.md");
    let custom = "title: Project board\nowner: alice\n# keep this comment\n";
    fs::write(
        &path,
        format!("---\n{custom}---\n\n## Backlog\n- [ ] existing\n"),
    )
    .unwrap();

    let output = todo(dir.path(), &["init", "--adopt"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let adopted = fs::read_to_string(path).unwrap();
    assert!(adopted.contains(custom));
    assert!(adopted.contains("todo-spec: 1"));
    assert_eq!(adopted.matches("---\n").count(), 2);
    assert!(String::from_utf8_lossy(&todo(dir.path(), &["list"]).stdout).contains("existing"));
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
fn reconcile_defaults_to_structural_only_in_a_complete_git_repository() {
    let dir = tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    git(dir.path(), &["config", "user.name", "Test"]);
    assert!(todo(dir.path(), &["init"]).status.success());
    git(dir.path(), &["add", "TODO.md"]);
    git(dir.path(), &["commit", "-qm", "add task file"]);

    let output = todo(dir.path(), &["reconcile", "--json"]);

    assert_eq!(output.status.code(), Some(0));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(value["data"]["report"]["checks_skipped"]
        .as_array()
        .unwrap()
        .is_empty());
}

/// A configured provider that never runs must not report a clean check. The
/// operator set TODO_LLM_* expecting semantic judgement, so a bare exit 0 would
/// read as "the task text was compared against the code".
#[test]
fn configured_semantic_provider_that_never_runs_is_reported_as_incomplete() {
    let dir = tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    git(dir.path(), &["config", "user.name", "Test"]);
    assert!(todo(dir.path(), &["init"]).status.success());
    git(dir.path(), &["add", "TODO.md"]);
    git(dir.path(), &["commit", "-qm", "add task file"]);

    let configured = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_todo"))
            .current_dir(dir.path())
            .env("TODO_LLM_PROVIDER", "anthropic")
            .env("TODO_LLM_API_KEY", "test-key")
            .args(args)
            .output()
            .unwrap()
    };

    // Provider configured but --semantic not passed: the layer silently did not run.
    let output = configured(&["reconcile", "--json"]);
    assert_eq!(output.status.code(), Some(7));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let skipped = value["data"]["report"]["checks_skipped"]
        .as_array()
        .unwrap();
    assert!(
        skipped
            .iter()
            .any(|reason| reason.as_str().unwrap_or_default().contains("semantic")),
        "must name the semantic layer: {skipped:?}"
    );

    // Explicitly opting out is an informed choice, and agents are told to run it
    // this way, so it must stay a clean exit rather than become permanent noise.
    assert_eq!(
        configured(&["reconcile", "--no-semantic"]).status.code(),
        Some(0)
    );

    // With no provider configured at all the layer is simply off, not skipped.
    assert_eq!(
        todo(dir.path(), &["reconcile", "--json"]).status.code(),
        Some(0)
    );
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
fn semantic_authentication_failure_exits_incomplete_instead_of_clean() {
    let dir = tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    git(dir.path(), &["config", "user.name", "Test"]);
    fs::write(dir.path().join("base.txt"), "base\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "base"]);
    fs::write(dir.path().join("feature.rs"), "fn feature() {}\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "feature"]);
    let from = git(dir.path(), &["rev-parse", "HEAD"]);
    fs::write(
        dir.path().join("feature.rs"),
        "fn feature() { println!(\"wired\"); }\n",
    )
    .unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "wire feature"]);
    let to = git(dir.path(), &["rev-parse", "HEAD"]);
    fs::write(
        dir.path().join("TODO.md"),
        format!(
            "---\ntodo-spec: 1\ncolumns:\n  - {{id: todo, name: Todo}}\n---\n\n## Todo\n- [ ] feature <!--t:aaaaaa--> 🔗{from}..{to}\n"
        ),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_todo"))
        .current_dir(dir.path())
        .args(["reconcile", "--semantic", "--json"])
        .env("TODO_LLM_PROVIDER", "openai")
        .env("TODO_LLM_MODEL", "test-model")
        .env_remove("TODO_LLM_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(7));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let skipped = value["data"]["report"]["checks_skipped"]
        .as_array()
        .unwrap();
    assert!(skipped
        .iter()
        .any(|reason| reason.as_str().unwrap().contains("authentication")));
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
    assert!(todo(
        dir.path(),
        &[
            "progress",
            &id,
            "60",
            "--note",
            "implementation remains incomplete",
        ],
    )
    .status
    .success());

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

#[test]
fn contract_check_reports_missing_contract_as_vpc001_with_exit_six() {
    let dir = tempdir().unwrap();
    let output = todo(dir.path(), &["contract", "check", "--json"]);
    assert_eq!(output.status.code(), Some(6));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["schema"], "todo.contract.v1");
    assert_eq!(json["data"]["findings"][0]["rule"], "VPC001");
}

#[test]
fn contract_check_passes_a_minimal_contract_and_exit_zero() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".virgo")).unwrap();
    fs::write(
        dir.path().join(".virgo/project.yaml"),
        "schema_version: 1\nproject:\n  id: test-project\n  name: Test\n",
    )
    .unwrap();
    let output = todo(dir.path(), &["contract", "check", "--json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["data"]["findings"].as_array().unwrap().len(), 0);
    assert_eq!(json["data"]["enforcement"], "guided");
}

#[test]
fn contract_check_reports_missing_mapped_document_as_vpc002() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".virgo")).unwrap();
    fs::write(
        dir.path().join(".virgo/project.yaml"),
        "schema_version: 1\nproject:\n  id: p\n  name: P\ndocuments:\n  tasks:\n    path: TODO.md\n",
    )
    .unwrap();
    let output = todo(dir.path(), &["contract", "check"]);
    assert_eq!(output.status.code(), Some(6));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("panic"), "unexpected panic: {stderr}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("VPC002"), "stdout: {stdout}");
}

#[test]
fn contract_check_rejects_unknown_schema_version_with_locatable_error() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".virgo")).unwrap();
    fs::write(
        dir.path().join(".virgo/project.yaml"),
        "schema_version: 9\nproject:\n  id: p\n  name: P\n",
    )
    .unwrap();
    let output = todo(dir.path(), &["contract", "check", "--json"]);
    assert_eq!(output.status.code(), Some(6));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["data"]["findings"][0]["rule"], "VPC001");
    assert!(json["data"]["findings"][0]["message"]
        .as_str()
        .unwrap()
        .contains("schema_version 9"));
}

#[test]
fn contract_check_supports_explicit_root_outside_cwd() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".virgo")).unwrap();
    fs::write(
        dir.path().join(".virgo/project.yaml"),
        "schema_version: 1\nproject:\n  id: p\n  name: P\n",
    )
    .unwrap();
    let cwd = tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_todo"))
        .current_dir(cwd.path())
        .args(["contract", "check", "--root", dir.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
