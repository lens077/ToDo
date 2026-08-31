use std::{fs, process::Command};
use tempfile::tempdir;
fn todo(home: &std::path::Path, cwd: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_todo"))
        .env("HOME", home)
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn preview_does_not_write_and_matches_install_content() {
    let home = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    let path = home.path().join(".claude/settings.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let original = "{\n    \"theme\": \"dark\"\n}\n";
    fs::write(&path, original).unwrap();
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    let preview: serde_json::Value = serde_json::from_slice(
        &todo(
            home.path(),
            cwd.path(),
            &["hooks", "preview", "--agent", "claude", "--json"],
        )
        .stdout,
    )
    .unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
    let install: serde_json::Value = serde_json::from_slice(
        &todo(
            home.path(),
            cwd.path(),
            &["hooks", "install", "--agent", "claude", "--json"],
        )
        .stdout,
    )
    .unwrap();
    assert_eq!(
        preview["data"]["targets"][0]["content"],
        install["data"]["targets"][0]["content"]
    );
    assert_eq!(
        fs::read_to_string(path).unwrap(),
        install["data"]["targets"][0]["content"].as_str().unwrap()
    );
}

#[test]
fn install_is_idempotent_preserves_user_hook_and_uninstall_removes_only_ours() {
    let home = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    let path = home.path().join(".claude/settings.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let original="{\n  \"theme\": \"dark\",\n  \"hooks\": {\n    \"SessionStart\": [{\"hooks\": [{\"type\": \"command\", \"command\": \"user-hook --keep\"}]}]\n  }\n}\n";
    fs::write(&path, original).unwrap();
    assert!(todo(
        home.path(),
        cwd.path(),
        &["hooks", "install", "--agent", "claude"]
    )
    .status
    .success());
    let first = fs::read_to_string(&path).unwrap();
    assert!(first.contains("user-hook --keep"));
    assert_eq!(
        first.matches("TODO_HOOK_ID=todo-session-context").count(),
        1
    );
    assert!(todo(
        home.path(),
        cwd.path(),
        &["hooks", "install", "--agent", "claude"]
    )
    .status
    .success());
    assert_eq!(fs::read_to_string(&path).unwrap(), first);
    assert!(fs::read_dir(path.parent().unwrap())
        .unwrap()
        .any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("todo-backup")));
    assert!(todo(
        home.path(),
        cwd.path(),
        &["hooks", "uninstall", "--agent", "claude"]
    )
    .status
    .success());
    let after = fs::read_to_string(path).unwrap();
    assert!(after.contains("user-hook --keep"));
    assert!(!after.contains("TODO_HOOK_ID"));
}

#[test]
fn invalid_json_is_rejected_without_overwrite() {
    let home = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    let path = home.path().join(".claude/settings.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "{broken").unwrap();
    let output = todo(
        home.path(),
        cwd.path(),
        &["hooks", "install", "--agent", "claude"],
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(fs::read_to_string(path).unwrap(), "{broken");
}

#[test]
fn instructions_replace_managed_block_and_preserve_user_content() {
    let home = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    let path = cwd.path().join("AGENTS.md");
    fs::write(&path, "# User rules\n\nkeep this\n").unwrap();
    assert!(todo(
        home.path(),
        cwd.path(),
        &["hooks", "instructions", "--agent", "codex", "--write"]
    )
    .status
    .success());
    let first = fs::read_to_string(&path).unwrap();
    assert!(first.contains("keep this"));
    assert!(first.contains("todo evidence <id> --from \"$START\""));
    assert!(first.contains("Never hand-edit evidence tokens"));
    assert!(first.lines().count() <= 30);
    assert!(todo(
        home.path(),
        cwd.path(),
        &["hooks", "instructions", "--agent", "codex", "--write"]
    )
    .status
    .success());
    assert_eq!(fs::read_to_string(path).unwrap(), first);
}

#[test]
fn installed_shell_command_succeeds_when_todo_missing() {
    let home = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    let value: serde_json::Value = serde_json::from_slice(
        &todo(
            home.path(),
            cwd.path(),
            &["hooks", "preview", "--agent", "claude", "--json"],
        )
        .stdout,
    )
    .unwrap();
    let command = value["data"]["targets"][0]["content"].as_str().unwrap();
    let settings: serde_json::Value = serde_json::from_str(command).unwrap();
    let shell = settings["hooks"]["SessionStart"][0]["hooks"][0]["command"]
        .as_str()
        .unwrap();
    let status = Command::new("sh")
        .args(["-c", shell])
        .env("PATH", "/usr/bin:/bin")
        .status()
        .unwrap();
    assert!(status.success());
    assert!(!shell.contains("timeout"));
    assert!(shell.ends_with("|| true"));
}
