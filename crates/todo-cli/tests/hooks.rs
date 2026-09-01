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
            &[
                "hooks",
                "preview",
                "--agent",
                "claude",
                "--json",
                "--show-content",
            ],
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
            &[
                "hooks",
                "install",
                "--agent",
                "claude",
                "--json",
                "--show-content",
            ],
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
fn installed_shell_command_uses_absolute_binary_without_path() {
    let home = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    let value: serde_json::Value = serde_json::from_slice(
        &todo(
            home.path(),
            cwd.path(),
            &[
                "hooks",
                "preview",
                "--agent",
                "claude",
                "--json",
                "--show-content",
            ],
        )
        .stdout,
    )
    .unwrap();
    let command = value["data"]["targets"][0]["content"].as_str().unwrap();
    let settings: serde_json::Value = serde_json::from_str(command).unwrap();
    let shell = settings["hooks"]["SessionStart"][0]["hooks"][0]["command"]
        .as_str()
        .unwrap();
    let executable = std::fs::canonicalize(env!("CARGO_BIN_EXE_todo")).unwrap();
    assert!(shell.starts_with("TODO_HOOK_ID=todo-session-context '/"));
    assert!(shell.contains(&executable.display().to_string()));
    let status = Command::new("sh")
        .args(["-c", shell])
        .env("PATH", "/usr/bin:/bin")
        .current_dir(cwd.path())
        .status()
        .unwrap();
    assert!(status.success());
    assert!(!shell.contains("timeout"));
    assert!(!shell.contains("|| true"));
}

#[test]
fn hooks_doctor_reports_resolved_executable() {
    let home = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_todo"))
        .env("HOME", home.path())
        .env("PATH", "/usr/bin:/bin")
        .current_dir(cwd.path())
        .args(["hooks", "doctor", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["ok"], true);
    assert_eq!(value["data"]["available"], true);
    let executable = value["data"]["executable"].as_str().unwrap();
    assert!(std::path::Path::new(executable).is_absolute());
    assert!(value["data"]["command"]
        .as_str()
        .unwrap()
        .contains(executable));
    assert!(!value["data"]["message"].as_str().unwrap().is_empty());
}

/// `preview`, `install` and `uninstall` all read the same settings.json. Warning
/// about only one of them implies the other two are safe to paste into an issue,
/// which is how the key leaks. None of them may print secrets by default.
#[test]
fn hook_commands_never_print_config_secrets_without_show_content() {
    let secret = "sk-ant-LEAKED-TEST-9999";
    for command in ["preview", "install", "uninstall"] {
        let home = tempdir().unwrap();
        let cwd = tempdir().unwrap();
        let path = home.path().join(".claude/settings.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            format!("{{\n  \"env\": {{\"ANTHROPIC_API_KEY\": \"{secret}\"}}\n}}\n"),
        )
        .unwrap();

        for args in [
            vec!["hooks", command, "--agent", "claude"],
            vec!["hooks", command, "--agent", "claude", "--json"],
        ] {
            let output = todo(home.path(), cwd.path(), &args);
            let text = String::from_utf8_lossy(&output.stdout);
            assert!(
                !text.contains(secret),
                "`todo {}` leaked the API key by default: {text}",
                args.join(" ")
            );
            assert!(
                text.contains(path.to_str().unwrap()),
                "`todo {}` must still report the target path: {text}",
                args.join(" ")
            );
        }
    }
}

#[test]
fn show_content_still_prints_the_full_config_when_asked() {
    let secret = "sk-ant-LEAKED-TEST-9999";
    let home = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    let path = home.path().join(".claude/settings.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        format!("{{\n  \"env\": {{\"ANTHROPIC_API_KEY\": \"{secret}\"}}\n}}\n"),
    )
    .unwrap();

    let output = todo(
        home.path(),
        cwd.path(),
        &["hooks", "preview", "--agent", "claude", "--show-content"],
    );
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains(secret), "opt-in must print the body: {text}");
}
