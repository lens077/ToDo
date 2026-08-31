use std::{fs, path::Path, process::Command};
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
