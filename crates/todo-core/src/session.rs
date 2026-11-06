//! Development sessions (Phase 4): the unit of cross-day context recovery.
//!
//! A session is a Markdown file at `<project>/.virgo/sessions/` — the file is
//! the source of truth, Git carries its history, and the app only renders and
//! validates. `begin` opens one, `finish` persists the summary with
//! deterministically collected evidence (git status file list; the agent/user
//! supplies test commands and results — the app never runs tests), and
//! `resume` reconstructs what the next session needs.

use std::{fs, path::Path, process::Command};

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

pub const SESSIONS_DIR: &str = ".virgo/sessions";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionSummary {
    pub id: String,
    pub goal: String,
    /// `active` | `completed` | `interrupted`
    pub status: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub next_actions: Vec<String>,
    pub file_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationEntry {
    pub command: String,
    pub result: String,
    pub evidence: String,
}

/// Everything `finish` writes. Natural-language fields come from the agent or
/// user; changed files are collected from git when available.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionFinishInput {
    pub completed: Vec<String>,
    pub decisions: Vec<String>,
    pub validation: Vec<ValidationEntry>,
    pub unresolved: Vec<String>,
    pub next_actions: Vec<String>,
    pub context_for_next: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionResume {
    pub latest: Option<SessionSummary>,
    /// Full rendered summary of the latest session, ready to paste into an
    /// agent's context.
    pub summary_markdown: String,
    /// Files changed during the latest session (git-derived).
    pub changed_files: Vec<String>,
    /// The unfinished work: next actions of the latest session, or all
    /// sections when it is still active.
    pub outstanding: Vec<String>,
}

/// Opens a session. Only one active session per project: `begin` while one is
/// open is refused, so "resume" always has a single unambiguous point.
pub fn begin(project_root: &Path, goal: &str) -> Result<SessionSummary> {
    let goal = goal.trim();
    if goal.is_empty() {
        return Err(Error::WorkspaceInvalid("会话目标不能为空".into()));
    }
    let sessions = sessions_dir(project_root)?;
    if active_session(&sessions)?.is_some() {
        return Err(Error::WorkspaceInvalid(
            "已有进行中的会话；先完成或中断它再开始新会话".into(),
        ));
    }
    let now = now_ms();
    let id = format!("s-{}", now);
    let file_name = format!("{id}.md");
    let summary = SessionSummary {
        id: id.clone(),
        goal: goal.to_string(),
        status: "active".into(),
        started_at: now,
        finished_at: None,
        next_actions: Vec::new(),
        file_name: file_name.clone(),
    };
    fs::create_dir_all(&sessions).map_err(|source| Error::Io {
        path: sessions.clone(),
        source,
    })?;
    fs::write(sessions.join(&file_name), render_active(&summary)).map_err(|source| Error::Io {
        path: sessions.join(file_name),
        source,
    })?;
    Ok(summary)
}

/// Completes the active session and returns the stored summary.
pub fn finish(project_root: &Path, input: &SessionFinishInput) -> Result<SessionSummary> {
    let sessions = sessions_dir(project_root)?;
    let (file_name, started_at, goal) = active_session(&sessions)?
        .ok_or_else(|| Error::WorkspaceInvalid("没有进行中的会话".into()))?;
    let changed_files = collect_git_changed_files(project_root);
    let now = now_ms();
    let mut markdown = format!(
        "---\nsession_id: \"{}\"\nstatus: \"completed\"\nstarted_at: \"{}\"\nfinished_at: \"{}\"\n---\n\n",
        started_at, iso_or_epoch(started_at), iso_or_epoch(now),
    );
    markdown.push_str("# Session Summary\n\n");
    markdown.push_str(&format!("## Goal\n\n{}\n\n", heading_value(&goal)));
    push_list(&mut markdown, "Completed", &input.completed);
    markdown.push_str("## Files Changed\n\n");
    if changed_files.is_empty() {
        markdown.push_str("- （无 git 变更或非 git 仓库）\n");
    } else {
        for file in &changed_files {
            markdown.push_str(&format!("- {file}\n"));
        }
    }
    markdown.push('\n');
    push_list(&mut markdown, "Decisions", &input.decisions);
    markdown.push_str("## Validation\n\n");
    if input.validation.is_empty() {
        markdown.push_str("| Command | Result | Evidence |\n|---|---|---|\n| （未记录） | | |\n\n");
    } else {
        markdown.push_str("| Command | Result | Evidence |\n|---|---|---|\n");
        for entry in &input.validation {
            markdown.push_str(&format!(
                "| {} | {} | {} |\n",
                escape_cell(&entry.command),
                escape_cell(&entry.result),
                escape_cell(&entry.evidence)
            ));
        }
        markdown.push('\n');
    }
    push_list(&mut markdown, "Unresolved", &input.unresolved);
    push_list(&mut markdown, "Next Actions", &input.next_actions);
    markdown.push_str(&format!(
        "## Context for Next Session\n\n{}\n",
        if input.context_for_next.trim().is_empty() {
            "（未填写）".to_string()
        } else {
            input.context_for_next.trim().to_string()
        }
    ));
    let path = sessions.join(&file_name);
    fs::write(&path, &markdown).map_err(|source| Error::Io { path, source })?;
    Ok(SessionSummary {
        id: started_at.to_string(),
        goal,
        status: "completed".into(),
        started_at,
        finished_at: Some(now),
        next_actions: input.next_actions.clone(),
        file_name,
    })
}

/// Marks the active session interrupted so a new one can begin.
pub fn interrupt(project_root: &Path) -> Result<SessionSummary> {
    let sessions = sessions_dir(project_root)?;
    let (file_name, started_at, goal) = active_session(&sessions)?
        .ok_or_else(|| Error::WorkspaceInvalid("没有进行中的会话".into()))?;
    let mut markdown = format!(
        "---\nsession_id: \"{}\"\nstatus: \"interrupted\"\nstarted_at: \"{}\"\nfinished_at: \"{}\"\n---\n\n",
        started_at, iso_or_epoch(started_at), iso_or_epoch(now_ms()),
    );
    markdown.push_str("# Session Summary\n\n");
    markdown.push_str(&format!("## Goal\n\n{}\n\n", heading_value(&goal)));
    markdown.push_str("会话被中断，未产出总结。\n");
    let path = sessions.join(&file_name);
    fs::write(&path, &markdown).map_err(|source| Error::Io { path, source })?;
    Ok(SessionSummary {
        id: started_at.to_string(),
        goal,
        status: "interrupted".into(),
        started_at,
        finished_at: Some(now_ms()),
        next_actions: Vec::new(),
        file_name,
    })
}

pub fn list(project_root: &Path) -> Result<Vec<SessionSummary>> {
    let sessions = sessions_dir(project_root)?;
    let mut summaries = Vec::new();
    let entries = match fs::read_dir(&sessions) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(summaries),
        Err(source) => {
            return Err(Error::Io {
                path: sessions,
                source,
            })
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|extension| extension != "md") {
            continue;
        }
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(_) => continue,
        };
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        summaries.push(parse_summary(&file_name, &content));
    }
    summaries.sort_by_key(|summary| std::cmp::Reverse(summary.started_at));
    Ok(summaries)
}

/// What the next session needs: the latest summary and the outstanding work.
pub fn resume(project_root: &Path) -> Result<SessionResume> {
    let sessions = sessions_dir(project_root)?;
    let summaries = list(project_root)?;
    let latest = summaries.first().cloned();
    let (summary_markdown, changed_files, outstanding) = match &latest {
        None => ("（项目还没有会话记录）".into(), Vec::new(), Vec::new()),
        Some(summary) => {
            let path = sessions.join(&summary.file_name);
            let markdown =
                fs::read_to_string(&path).map_err(|source| Error::Io { path, source })?;
            let changed = extract_section_list(&markdown, "Files Changed");
            let outstanding = if summary.status == "active" {
                vec![summary.goal.clone()]
                    .into_iter()
                    .filter(|goal| !goal.is_empty())
                    .collect()
            } else {
                extract_section_list(&markdown, "Next Actions")
            };
            (markdown.clone(), changed, outstanding)
        }
    };
    Ok(SessionResume {
        latest,
        summary_markdown,
        changed_files,
        outstanding,
    })
}

// ── Internals ────────────────────────────────────────────────────────────────

fn sessions_dir(project_root: &Path) -> Result<std::path::PathBuf> {
    if !project_root.is_dir() {
        return Err(Error::WorkspaceInvalid(format!(
            "{} 不是目录",
            project_root.to_string_lossy()
        )));
    }
    Ok(project_root.join(SESSIONS_DIR))
}

fn active_session(sessions_dir: &Path) -> Result<Option<(String, i64, String)>> {
    let entries = match fs::read_dir(sessions_dir) {
        Ok(entries) => entries,
        // No sessions directory yet: nothing is active.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(Error::Io {
                path: sessions_dir.to_path_buf(),
                source,
            })
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|extension| extension != "md") {
            continue;
        }
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(_) => continue,
        };
        if content.contains("status: \"active\"") {
            let file_name = path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
            let id = file_name
                .trim_end_matches(".md")
                .trim_start_matches("s-")
                .to_string();
            let started_at = id.parse::<i64>().unwrap_or(0);
            let goal = extract_section(&content, "Goal").unwrap_or_default();
            return Ok(Some((file_name, started_at, goal)));
        }
    }
    Ok(None)
}

fn render_active(summary: &SessionSummary) -> String {
    format!(
        "---\nsession_id: \"{}\"\nstatus: \"active\"\nstarted_at: \"{}\"\n---\n\n# Session Summary\n\n## Goal\n\n{}\n\n## Completed\n\n## Files Changed\n\n## Decisions\n\n## Validation\n\n| Command | Result | Evidence |\n|---|---|---|\n\n## Unresolved\n\n## Next Actions\n\n## Context for Next Session\n",
        summary.id,
        iso_or_epoch(summary.started_at),
        heading_value(&summary.goal),
    )
}

fn parse_summary(file_name: &str, content: &str) -> SessionSummary {
    let frontmatter = content
        .strip_prefix("---\n")
        .map(|rest| rest.split("\n---").next().unwrap_or("").to_string())
        .unwrap_or_default();
    let value = |key: &str| {
        frontmatter
            .lines()
            .find_map(|line| line.trim().strip_prefix(&format!("{key}: ")))
            .map(|value| value.trim().trim_matches('"').to_string())
            .unwrap_or_default()
    };
    // Unparseable timestamps fall back to 0 for ordering; the file name
    // remains the authoritative identity.
    let started_at = value("started_at").parse::<i64>().unwrap_or(0);
    let status = if value("status").is_empty() {
        "active".into()
    } else {
        value("status")
    };
    let finished_at = frontmatter
        .lines()
        .find_map(|line| line.trim().strip_prefix("finished_at: "))
        .map(|value| value.trim().trim_matches('"').to_string())
        .filter(|value| !value.is_empty() && value != "null")
        .and_then(|value| value.parse::<i64>().ok());
    SessionSummary {
        id: file_name.trim_end_matches(".md").to_string(),
        goal: extract_section(content, "Goal").unwrap_or_default(),
        status,
        started_at,
        finished_at,
        next_actions: extract_section_list(content, "Next Actions"),
        file_name: file_name.to_string(),
    }
}

fn extract_section(content: &str, heading: &str) -> Option<String> {
    let marker = format!("## {heading}");
    let mut lines = content.lines().skip_while(|line| line.trim() != marker);
    lines.next()?;
    let mut collected = String::new();
    for line in lines {
        if line.starts_with("## ") {
            break;
        }
        collected.push_str(line);
        collected.push('\n');
    }
    let trimmed = collected.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// List items of a section, `- ` prefixes stripped.
fn extract_section_list(content: &str, heading: &str) -> Vec<String> {
    extract_section(content, heading)
        .map(|section| {
            section
                .lines()
                .filter_map(|line| line.trim().strip_prefix("- "))
                .filter(|item| !item.starts_with('（'))
                .map(|item| item.to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn push_list(markdown: &mut String, heading: &str, items: &[String]) {
    markdown.push_str(&format!("## {heading}\n\n"));
    if items.is_empty() {
        markdown.push_str("- （未记录）\n\n");
        return;
    }
    for item in items {
        markdown.push_str(&format!("- {}\n", item.trim()));
    }
    markdown.push('\n');
}

fn heading_value(value: &str) -> &str {
    value.trim()
}

fn escape_cell(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

fn iso_or_epoch(ms: i64) -> String {
    // Deterministic and sortable: epoch millis stay parseable by the reader.
    ms.to_string()
}

/// Deterministic git evidence: `git status --porcelain` paths. Any failure
/// (not a repo, git missing) yields an empty list, never an error — sessions
/// must work outside git too.
fn collect_git_changed_files(project_root: &Path) -> Vec<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(project_root)
        .args(["status", "--porcelain"])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.get(3..))
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty())
        .collect()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn begin_active(root: &Path, goal: &str) -> SessionSummary {
        begin(root, goal).unwrap()
    }

    #[test]
    fn begin_creates_active_session_and_refuses_second() {
        let root = project();
        let summary = begin_active(root.path(), "实现 Workspace 注册表");
        assert_eq!(summary.status, "active");
        let file = root.path().join(SESSIONS_DIR).join(&summary.file_name);
        assert!(file.exists());
        let content = fs::read_to_string(file).unwrap();
        assert!(content.contains("status: \"active\""));
        assert!(content.contains("## Goal"));

        assert!(begin(root.path(), "第二个会话").is_err());
    }

    #[test]
    fn begin_requires_goal() {
        let root = project();
        assert!(begin(root.path(), "   ").is_err());
    }

    #[test]
    fn finish_writes_summary_and_collects_git_files() {
        let root = project();
        begin_active(root.path(), "对账器 Phase 4");
        // Deterministic git evidence from a throwaway repo.
        let run = |args: &[&str]| {
            Command::new("git")
                .arg("-C")
                .arg(root.path())
                .args(args)
                .output()
                .unwrap();
        };
        run(&["init", "-q"]);
        run(&["-c", "user.email=t@t", "-c", "user.name=t", "add", "-A"]);
        run(&[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "init",
        ]);
        fs::write(root.path().join("new-file.rs"), "fn main() {}\n").unwrap();

        let finished = finish(
            root.path(),
            &SessionFinishInput {
                completed: vec!["注册表落库".into()],
                decisions: vec!["Session 存 Markdown".into()],
                validation: vec![ValidationEntry {
                    command: "make check-quick".into(),
                    result: "绿".into(),
                    evidence: "verify-quick 148s".into(),
                }],
                unresolved: vec![],
                next_actions: vec!["接入 watcher".into()],
                context_for_next: "watcher 在 src-tauri/watch.rs".into(),
            },
        )
        .unwrap();
        assert_eq!(finished.status, "completed");
        assert!(finished.finished_at.is_some());

        let content =
            fs::read_to_string(root.path().join(SESSIONS_DIR).join(&finished.file_name)).unwrap();
        assert!(
            content.contains("- new-file.rs"),
            "git file missing: {content}"
        );
        assert!(content.contains("| make check-quick | 绿 | verify-quick 148s |"));
        assert!(content.contains("- 注册表落库"));
        assert!(content.contains("## Context for Next Session"));
    }

    #[test]
    fn finish_without_session_is_refused() {
        let root = project();
        assert!(finish(root.path(), &SessionFinishInput::default()).is_err());
    }

    #[test]
    fn interrupt_unblocks_new_session() {
        let root = project();
        begin_active(root.path(), "被中断的工作");
        let interrupted = interrupt(root.path()).unwrap();
        assert_eq!(interrupted.status, "interrupted");
        let second = begin_active(root.path(), "重新开始");
        assert_ne!(second.file_name, interrupted.file_name);
    }

    #[test]
    fn resume_reports_outstanding_next_actions_after_finish() {
        let root = project();
        begin_active(root.path(), "Phase 4");
        finish(
            root.path(),
            &SessionFinishInput {
                next_actions: vec!["接入 watcher 增量".into(), "补 E2E".into()],
                ..Default::default()
            },
        )
        .unwrap();
        let resumed = resume(root.path()).unwrap();
        let latest = resumed.latest.unwrap();
        assert_eq!(latest.status, "completed");
        assert_eq!(
            resumed.outstanding,
            vec!["接入 watcher 增量".to_string(), "补 E2E".to_string()]
        );
        assert!(resumed.summary_markdown.contains("## Validation"));
    }

    #[test]
    fn resume_of_active_session_outstanding_is_the_goal() {
        let root = project();
        begin_active(root.path(), "还没做完的目标");
        let resumed = resume(root.path()).unwrap();
        assert_eq!(resumed.latest.unwrap().status, "active");
        assert_eq!(resumed.outstanding, vec!["还没做完的目标".to_string()]);
    }

    #[test]
    fn resume_on_empty_project_is_empty_not_error() {
        let root = project();
        let resumed = resume(root.path()).unwrap();
        assert!(resumed.latest.is_none());
        assert!(resumed.outstanding.is_empty());
    }

    #[test]
    fn list_orders_newest_first_and_reports_status() {
        let root = project();
        begin_active(root.path(), "第一个");
        finish(root.path(), &SessionFinishInput::default()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        begin_active(root.path(), "第二个");
        let summaries = list(root.path()).unwrap();
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].status, "active");
        assert_eq!(summaries[1].status, "completed");
        assert!(summaries[0].started_at >= summaries[1].started_at);
    }
}

// ── Docs validator (Phase 5) ────────────────────────────────────────────────

/// Severity mirrors the contract validator; `docs validate` blocks CI on Error.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DocsSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocsFinding {
    pub rule: String,
    pub severity: DocsSeverity,
    pub message: String,
    /// Session file the finding refers to, when applicable.
    pub file: Option<String>,
}

/// Deterministic, offline validation of the session records under
/// `.virgo/sessions/`. Complements the contract check: the contract validates
/// document mapping, this validates session hygiene — the fields `resume`
/// depends on. Errors are things that break context recovery; warnings are
/// quality gaps; info is orientation.
pub fn docs_validate(project_root: &Path) -> Result<Vec<DocsFinding>> {
    let mut findings = Vec::new();
    let sessions = sessions_dir(project_root)?;
    let entries = match fs::read_dir(&sessions) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            findings.push(DocsFinding {
                rule: "VDS004".into(),
                severity: DocsSeverity::Info,
                message: "项目还没有会话记录；`todo session begin --goal \"…\"` 开始第一个会话"
                    .into(),
                file: None,
            });
            return Ok(findings);
        }
        Err(source) => {
            return Err(Error::Io {
                path: sessions,
                source,
            })
        }
    };

    let mut summaries = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|extension| extension != "md") {
            continue;
        }
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(_) => continue,
        };
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        // VDS001: frontmatter must declare a status the resume logic can trust.
        if !content.starts_with("---\n") || !content.contains("status: \"") {
            findings.push(DocsFinding {
                rule: "VDS001".into(),
                severity: DocsSeverity::Error,
                message: "会话文件缺少可解析的 frontmatter `status` 字段；resume 无法判断生命周期"
                    .into(),
                file: Some(file_name.clone()),
            });
            continue;
        }
        let summary = parse_summary(&file_name, &content);
        // VDS002: an active session without a goal cannot restore context.
        if summary.status == "active" && summary.goal.trim().is_empty() {
            findings.push(DocsFinding {
                rule: "VDS002".into(),
                severity: DocsSeverity::Error,
                message:
                    "进行中的会话缺少目标（Goal 为空）；`session finish` / `resume` 会产出空上下文"
                        .into(),
                file: Some(file_name.clone()),
            });
        }
        summaries.push(summary);
    }
    summaries.sort_by_key(|summary| std::cmp::Reverse(summary.started_at));

    // VDS003: a completed session without next actions defeats the recovery flow.
    if let Some(latest) = summaries
        .iter()
        .find(|summary| summary.status == "completed")
    {
        if latest.next_actions.is_empty() {
            findings.push(DocsFinding {
                rule: "VDS003".into(),
                severity: DocsSeverity::Warning,
                message: "最近完成的会话没有 Next Actions；下次会话没有可续接的入口".into(),
                file: Some(latest.file_name.clone()),
            });
        }
    }
    Ok(findings)
}

/// Errors block CI (`docs validate` exits non-zero), the rest are advisory.
pub fn docs_validate_blocking(findings: &[DocsFinding]) -> bool {
    findings
        .iter()
        .any(|finding| finding.severity == DocsSeverity::Error)
}

#[cfg(test)]
mod docs_validate_tests {
    use super::*;

    #[test]
    fn empty_project_is_info_not_error() {
        let root = tempfile::tempdir().unwrap();
        let findings = docs_validate(root.path()).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, DocsSeverity::Info);
        assert!(!docs_validate_blocking(&findings));
    }

    #[test]
    fn well_kept_sessions_produce_no_findings() {
        let root = tempfile::tempdir().unwrap();
        begin(root.path(), "有目标的工作").unwrap();
        finish(
            root.path(),
            &SessionFinishInput {
                next_actions: vec!["继续下一步".into()],
                ..Default::default()
            },
        )
        .unwrap();
        let findings = docs_validate(root.path()).unwrap();
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn completed_session_without_next_actions_is_warning() {
        let root = tempfile::tempdir().unwrap();
        begin(root.path(), "工作").unwrap();
        finish(root.path(), &SessionFinishInput::default()).unwrap();
        let findings = docs_validate(root.path()).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, "VDS003");
        assert_eq!(findings[0].severity, DocsSeverity::Warning);
        assert!(!docs_validate_blocking(&findings));
    }

    #[test]
    fn malformed_frontmatter_is_blocking_error() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join(SESSIONS_DIR);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("s-1.md"), "# 手写的会话\n\n没有 frontmatter\n").unwrap();
        let findings = docs_validate(root.path()).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, "VDS001");
        assert_eq!(findings[0].severity, DocsSeverity::Error);
        assert!(docs_validate_blocking(&findings));
    }

    #[test]
    fn active_session_without_goal_is_blocking_error() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join(SESSIONS_DIR);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("s-2.md"),
            "---\nsession_id: \"2\"\nstatus: \"active\"\n---\n\n## Goal\n\n\n",
        )
        .unwrap();
        let findings = docs_validate(root.path()).unwrap();
        assert!(findings
            .iter()
            .any(|finding| finding.rule == "VDS002" && finding.severity == DocsSeverity::Error));
        assert!(docs_validate_blocking(&findings));
    }
}
