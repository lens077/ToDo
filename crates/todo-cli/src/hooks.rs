use serde::Serialize;
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use todo_core::{atomic_write, Error};

pub const MARKER: &str = "TODO_HOOK_ID=todo-session-context";
pub const COMMAND: &str =
    "TODO_HOOK_ID=todo-session-context todo hooks context --agent claude 2>/dev/null || true";
pub const BEGIN: &str = "<!-- todo:begin -->";
pub const END: &str = "<!-- todo:end -->";

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum Agent {
    Claude,
    Codex,
    All,
}
#[derive(Serialize)]
pub struct HookResult {
    pub targets: Vec<HookTarget>,
}
#[derive(Serialize)]
pub struct HookTarget {
    pub agent: &'static str,
    pub path: String,
    pub content: String,
    pub changed: bool,
    pub backup: Option<String>,
}

pub fn preview(home: &Path, agent: Agent) -> Result<HookResult, Error> {
    plan(home, agent, false, Action::Install)
}
pub fn install(home: &Path, agent: Agent) -> Result<HookResult, Error> {
    plan(home, agent, true, Action::Install)
}
pub fn uninstall(home: &Path, agent: Agent) -> Result<HookResult, Error> {
    plan(home, agent, true, Action::Uninstall)
}
pub fn instructions(home: &Path, agent: Agent, write: bool) -> Result<HookResult, Error> {
    plan(home, agent, write, Action::Instructions)
}

#[derive(Clone, Copy)]
enum Action {
    Install,
    Uninstall,
    Instructions,
}
fn plan(home: &Path, agent: Agent, write: bool, action: Action) -> Result<HookResult, Error> {
    let mut targets = Vec::new();
    if matches!(agent, Agent::Claude | Agent::All) {
        match action {
            Action::Install | Action::Uninstall => {
                targets.push(settings_target(home, write, action)?)
            }
            Action::Instructions => targets.push(instructions_target(
                Path::new("CLAUDE.md"),
                "claude",
                write,
            )?),
        }
    }
    if matches!(agent, Agent::Codex | Agent::All) {
        if matches!(action, Action::Instructions) {
            targets.push(instructions_target(Path::new("AGENTS.md"), "codex", write)?)
        } else {
            targets.push(HookTarget{agent:"codex",path:home.join(".codex/config.toml").display().to_string(),content:"Codex has no SessionStart context hook; no config change. Use `todo hooks instructions --agent codex`.".into(),changed:false,backup:None})
        }
    }
    Ok(HookResult { targets })
}

fn settings_target(home: &Path, write: bool, action: Action) -> Result<HookTarget, Error> {
    let path = home.join(".claude/settings.json");
    let original = fs::read_to_string(&path).unwrap_or_default();
    let mut settings: Value = if original.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(&original).map_err(|e| Error::InvalidConfiguration {
            path: path.clone(),
            message: e.to_string(),
        })?
    };
    if !settings.is_object() {
        return Err(Error::InvalidConfiguration {
            path,
            message: "root must be a JSON object".into(),
        });
    }
    let hooks = settings
        .as_object_mut()
        .unwrap()
        .entry("hooks")
        .or_insert_with(|| json!({}));
    if !hooks.is_object() {
        return Err(Error::InvalidConfiguration {
            path,
            message: "hooks must be a JSON object".into(),
        });
    }
    let groups = hooks
        .as_object_mut()
        .unwrap()
        .entry("SessionStart")
        .or_insert_with(|| json!([]));
    let Some(array) = groups.as_array_mut() else {
        return Err(Error::InvalidConfiguration {
            path,
            message: "hooks.SessionStart must be an array".into(),
        });
    };
    strip_managed(array);
    if matches!(action, Action::Install) {
        array.push(json!({"hooks":[{"type":"command","command":COMMAND}]}));
    }
    if array.is_empty() {
        hooks.as_object_mut().unwrap().remove("SessionStart");
    }
    let indent = detect_indent(&original);
    let newline = if original.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut output = serde_json::to_string_pretty(&settings).unwrap();
    if indent != "  " {
        output = reindent(&output, indent)
    }
    output = output.replace('\n', newline);
    output.push_str(newline);
    let reparsed: Value =
        serde_json::from_str(&output).map_err(|error| Error::InvalidConfiguration {
            path: path.clone(),
            message: format!("generated settings failed validation: {error}"),
        })?;
    validate_preserved_settings(&settings, &reparsed, &path)?;
    let changed = output != original;
    let mut backup = None;
    if write && changed {
        if path.exists() {
            let backup_path =
                PathBuf::from(format!("{}.todo-backup-{}", path.display(), timestamp()));
            fs::copy(&path, &backup_path).map_err(|source| io_error(&backup_path, source))?;
            backup = Some(backup_path.display().to_string());
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| io_error(parent, source))?;
        }
        atomic_write(&path, output.as_bytes())?;
    }
    Ok(HookTarget {
        agent: "claude",
        path: path.display().to_string(),
        content: output,
        changed,
        backup,
    })
}
fn validate_preserved_settings(expected: &Value, actual: &Value, path: &Path) -> Result<(), Error> {
    if expected != actual {
        return Err(Error::InvalidConfiguration {
            path: path.to_path_buf(),
            message: "post-write semantic validation failed".into(),
        });
    }
    Ok(())
}

fn strip_managed(groups: &mut Vec<Value>) {
    for group in groups.iter_mut() {
        if let Some(hooks) = group.get_mut("hooks").and_then(Value::as_array_mut) {
            hooks.retain(|hook| {
                !hook
                    .get("command")
                    .and_then(Value::as_str)
                    .is_some_and(|command| command.contains(MARKER))
            });
        }
    }
    groups.retain(|group| {
        group
            .get("hooks")
            .and_then(Value::as_array)
            .map_or(true, |hooks| !hooks.is_empty())
    });
}

fn instructions_target(path: &Path, agent: &'static str, write: bool) -> Result<HookTarget, Error> {
    let original = fs::read_to_string(path).unwrap_or_default();
    let block = instruction_block();
    let content = replace_block(&original, &block);
    let changed = content != original;
    if write && changed {
        atomic_write(path, content.as_bytes())?;
    }
    Ok(HookTarget {
        agent,
        path: path.display().to_string(),
        content,
        changed,
        backup: None,
    })
}
fn instruction_block() -> String {
    format!("{BEGIN}\n## TODO.md workflow\n- TODO.md is the sole task source; use the `todo` CLI rather than editing task state indirectly.\n- Inspect work with `todo list --json`, `todo next --json`, and `todo reconcile --no-semantic --json`.\n- Create work with `todo add \"body\" --tag name`; finish with `todo done <id>`.\n- JSON responses use stable versioned `schema`, `ok`, and `data` fields.\n- Exit codes: 0 success, 1 error, 2 usage, 3 uninitialized, 4 lock timeout, 5 retry conflict, 6 findings.\n- Before coding, record the starting commit: `START=$(git rev-parse HEAD)`.\n- After committing, run `todo evidence <id> --from \"$START\"` (`--to` defaults to HEAD), then `todo done <id>`.\n- Never hand-edit evidence tokens; every task write must use the locked CLI.\n- Evidence recording is mandatory: reconciliation relies on commit ranges instead of guessing from the whole codebase.\n- Run `todo reconcile --no-semantic` before handing work back; never auto-resolve report-only evidence findings.\n{END}\n")
}
fn replace_block(original: &str, block: &str) -> String {
    if let Some(start) = original.find(BEGIN) {
        if let Some(relative) = original[start..].find(END) {
            let end = start + relative + END.len();
            let mut out = String::new();
            out.push_str(&original[..start]);
            out.push_str(block.trim_end());
            out.push_str(&original[end..]);
            return out;
        }
    }
    let mut out = original.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n')
    }
    if !out.is_empty() {
        out.push('\n')
    }
    out.push_str(block);
    out
}

pub fn context() -> String {
    context_with_deadline(Duration::from_secs(2))
}

pub fn context_with_deadline(timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    let next = run_capture(&["next"], deadline);
    let reconcile = run_capture(&["reconcile", "--no-semantic"], deadline);
    let mut lines = Vec::new();
    if !next.trim().is_empty() && !next.contains("no actionable task") {
        lines.push(format!("TODO next: {}", next.trim()))
    }
    for line in reconcile
        .lines()
        .filter(|line| line.starts_with("confirmed:") || line.starts_with("suspected:"))
        .take(37)
    {
        lines.push(format!("TODO reconcile: {line}"))
    }
    lines.truncate(40);
    if lines.is_empty() {
        String::new()
    } else {
        lines.join("\n") + "\n"
    }
}
fn run_capture(args: &[&str], deadline: Instant) -> String {
    if Instant::now() >= deadline {
        return String::new();
    }
    let mut child = match Command::new("todo")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return String::new(),
    };
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return if matches!(status.code(), Some(0 | 6)) {
                    child
                        .wait_with_output()
                        .ok()
                        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
                        .unwrap_or_default()
                } else {
                    String::new()
                };
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return String::new();
            }
        }
    }
}
fn detect_indent(raw: &str) -> &str {
    raw.lines()
        .find_map(|line| {
            let spaces = line.len() - line.trim_start().len();
            (spaces > 0).then_some(&line[..spaces])
        })
        .unwrap_or("  ")
}
fn reindent(pretty: &str, indent: &str) -> String {
    pretty
        .lines()
        .map(|line| {
            let spaces = line.len() - line.trim_start().len();
            format!("{}{}", indent.repeat(spaces / 2), line.trim_start())
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn io_error(path: &Path, source: std::io::Error) -> Error {
    Error::Io {
        path: path.to_path_buf(),
        source,
    }
}
