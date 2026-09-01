use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use tempfile::tempdir;
fn cmd(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_todo"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}
fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}
#[test]
fn evidence_validates_commits_defaults_to_head_and_writes_canonical_token() {
    let dir = tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    git(dir.path(), &["config", "user.name", "Test"]);
    fs::write(dir.path().join("file"), "one").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "one"]);
    let from = git(dir.path(), &["rev-parse", "HEAD"]);
    fs::write(dir.path().join("file"), "two").unwrap();
    git(dir.path(), &["commit", "-qam", "two"]);
    let head = git(dir.path(), &["rev-parse", "HEAD"]);
    assert!(cmd(dir.path(), &["init"]).status.success());
    let added: serde_json::Value =
        serde_json::from_slice(&cmd(dir.path(), &["add", "work", "--json"]).stdout).unwrap();
    let id = added["data"]["task"]["id"].as_str().unwrap();
    let output = cmd(dir.path(), &["evidence", id, "--from", &from, "--json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = fs::read_to_string(dir.path().join("TODO.md")).unwrap();
    assert!(text.contains(&format!("🔗{from}..{head}")));
}
#[test]
fn done_with_note_and_suggested_evidence_is_one_restore_point() {
    let dir = tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    git(dir.path(), &["config", "user.name", "Test"]);
    fs::write(
        dir.path().join("TODO.md"),
        "---\ntodo-spec: 1\ncompletion-policy: strict\ncolumns:\n  - {id: todo, name: Todo}\n---\n\n## Todo\n- [ ] work <!--t:abc123-->\n",
    )
    .unwrap();
    git(dir.path(), &["add", "TODO.md"]);
    git(dir.path(), &["commit", "-qm", "introduce task"]);
    let from = git(dir.path(), &["rev-parse", "HEAD"]);
    fs::write(dir.path().join("implementation"), "done").unwrap();
    git(dir.path(), &["add", "implementation"]);
    git(dir.path(), &["commit", "-qm", "implement task"]);
    let head = git(dir.path(), &["rev-parse", "HEAD"]);

    let output = cmd(
        dir.path(),
        &["done", "abc123", "--note", "verified", "--evidence"],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = fs::read_to_string(dir.path().join("TODO.md")).unwrap();
    assert!(text.contains("- [x] work"));
    assert!(text.contains("↳ verified <!--n:abc123-->"));
    assert!(text.contains(&format!("🔗{from}..{head}")));

    let history = cmd(dir.path(), &["history", "--json"]);
    let value: serde_json::Value = serde_json::from_slice(&history.stdout).unwrap();
    let entries = value["data"]["entries"].as_array().unwrap();
    assert_eq!(
        entries.len(),
        1,
        "compound completion must create one restore point"
    );
    assert_eq!(entries[0]["op"], "done");
}

#[test]
fn done_suggests_evidence_from_the_locked_todo_snapshot_and_current_head() {
    let dir = tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    git(dir.path(), &["config", "user.name", "Test"]);
    fs::write(
        dir.path().join("TODO.md"),
        "---\ntodo-spec: 1\ncompletion-policy: strict\ncolumns:\n  - {id: todo, name: Todo}\n---\n\n## Todo\n- [ ] work <!--t:abc123-->\n",
    )
    .unwrap();
    git(dir.path(), &["add", "TODO.md"]);
    git(dir.path(), &["commit", "-qm", "introduce task"]);
    let original_from = git(dir.path(), &["rev-parse", "HEAD"]);
    fs::write(dir.path().join("implementation"), "done").unwrap();
    git(dir.path(), &["add", "implementation"]);
    git(dir.path(), &["commit", "-qm", "implement task"]);

    let original_git = String::from_utf8(
        Command::new("sh")
            .args(["-c", "command -v git"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let shim_dir = dir.path().join("shim");
    fs::create_dir(&shim_dir).unwrap();
    fs::write(
        shim_dir.join("git"),
        format!(
            "#!/bin/sh\nif [ \"$1\" = rev-parse ] && [ \"$3\" = 'HEAD^{{commit}}' ] && [ ! -e \"$TODO_GIT_ENTERED\" ]; then\n  : > \"$TODO_GIT_ENTERED\"\n  while [ -e \"$TODO_GIT_BLOCK\" ]; do sleep 0.01; done\nfi\nexec \"{}\" \"$@\"\n",
            original_git.trim()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(shim_dir.join("git"), fs::Permissions::from_mode(0o755)).unwrap();
    let entered = dir.path().join("git-entered");
    let block = dir.path().join("git-block");
    fs::write(&block, "blocked").unwrap();

    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.path().join(".TODO.md.lock"))
        .unwrap();
    fs2::FileExt::lock_exclusive(&lock).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_todo"));
    child
        .current_dir(dir.path())
        .env(
            "PATH",
            format!(
                "{}:{}",
                shim_dir.display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .env("TODO_GIT_ENTERED", &entered)
        .env("TODO_GIT_BLOCK", &block)
        .args([
            "--lock-timeout-ms",
            "5000",
            "done",
            "abc123",
            "--note",
            "verified",
            "--evidence",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = child.spawn().unwrap();
    thread::sleep(Duration::from_millis(100));
    assert!(
        child.try_wait().unwrap().is_none(),
        "done must wait for LockedWriter"
    );

    let concurrent_from = git(dir.path(), &["rev-parse", "HEAD"]);
    let path = dir.path().join("TODO.md");
    let current = fs::read_to_string(&path).unwrap();
    fs::write(
        &path,
        current.replace(
            "<!--t:abc123-->",
            &format!("<!--t:abc123--> 🔗{concurrent_from}..{original_from}"),
        ),
    )
    .unwrap();
    fs::write(dir.path().join("concurrent"), "new head").unwrap();
    git(dir.path(), &["add", "TODO.md", "concurrent"]);
    git(dir.path(), &["commit", "-qm", "concurrent task update"]);
    let concurrent_head = git(dir.path(), &["rev-parse", "HEAD"]);

    fs2::FileExt::unlock(&lock).unwrap();
    fs::remove_file(&block).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        assert!(Instant::now() < deadline, "done command did not finish");
        thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = fs::read_to_string(path).unwrap();
    assert!(text.contains("- [x] work"));
    assert!(text.contains(&format!("🔗{concurrent_from}..{concurrent_head}")));
    assert!(!text.contains(&format!("🔗{original_from}..")));
}

#[test]
fn done_evidence_rejects_empty_head_range_without_writing() {
    let dir = tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    git(dir.path(), &["config", "user.name", "Test"]);
    fs::write(
        dir.path().join("TODO.md"),
        "---\ntodo-spec: 1\ncompletion-policy: strict\ncolumns: []\n---\n- [ ] work <!--t:abc123-->\n",
    )
    .unwrap();
    git(dir.path(), &["add", "TODO.md"]);
    git(dir.path(), &["commit", "-qm", "introduce task"]);
    let before = fs::read(dir.path().join("TODO.md")).unwrap();
    let output = cmd(
        dir.path(),
        &["done", "abc123", "--note", "verified", "--evidence"],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("HEAD..HEAD"));
    assert_eq!(fs::read(dir.path().join("TODO.md")).unwrap(), before);
}

#[test]
fn evidence_rejects_missing_commit_without_writing() {
    let dir = tempdir().unwrap();
    assert!(cmd(dir.path(), &["init"]).status.success());
    let added: serde_json::Value =
        serde_json::from_slice(&cmd(dir.path(), &["add", "work", "--json"]).stdout).unwrap();
    let id = added["data"]["task"]["id"].as_str().unwrap();
    let before = fs::read(dir.path().join("TODO.md")).unwrap();
    let output = cmd(dir.path(), &["evidence", id, "--from", "deadbeef"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(fs::read(dir.path().join("TODO.md")).unwrap(), before);
}
