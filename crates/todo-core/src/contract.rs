//! Project Contract: `.virgo/project.yaml` schema v1 loader and deterministic
//! validator (spec: `docs/project-contract-spec.md`, rule IDs VPC001-VPC007).
//!
//! The contract is a machine-readable project declaration; it is not a second
//! TODO.md format. Validation is offline, pure and evidence-carrying: every
//! finding names a stable rule ID, a severity and the offending path.

use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::error::{io_error, Error};

/// Only schema v1 exists; anything else is rejected with a locatable error.
pub const CONTRACT_SCHEMA_VERSION: u32 = 1;
pub const CONTRACT_DIR: &str = ".virgo";
pub const CONTRACT_FILE: &str = "project.yaml";

pub const MANAGED_BEGIN: &str = "<!-- BEGIN VIRGO MANAGED RULES -->";
pub const MANAGED_END: &str = "<!-- END VIRGO MANAGED RULES -->";

/// The canonical Virgo agent-instructions block. Installer updates converge on
/// this text; anything between the markers is replaced wholesale.
pub const MANAGED_RULES: &str = "\
<!-- BEGIN VIRGO MANAGED RULES -->
## Virgo Project Memory

Before work:
1. Read `.virgo/project.yaml`.
2. Read the mapped source documents.
3. Identify task scope and acceptance criteria.

Before handoff:
1. Update affected source documents.
2. Record actual verification results.
3. List unresolved issues.
4. Run `todo contract check`.
5. Report changed files and next steps.

Review diffs before writing managed files.
Keep user-owned instructions intact.
<!-- END VIRGO MANAGED RULES -->";

/// What an install run did to a file. `Unchanged` means the file already
/// matched the canonical output — a second install must not append anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallOutcome {
    /// File did not exist; the initial content was generated.
    Created,
    /// File existed and was rewritten (block added or block text updated).
    Updated,
    /// File already matched; nothing was written.
    Unchanged,
}

/// Idempotently installs the managed rules block into `content` (the current
/// file body, `None` when the file does not exist). User-owned text outside
/// the markers is preserved byte-for-byte.
pub fn install_agent_rules(content: Option<&str>) -> (String, InstallOutcome) {
    match content {
        None => (
            format!("# AGENTS.md\n\n{MANAGED_RULES}\n"),
            InstallOutcome::Created,
        ),
        Some(existing) => {
            if let Some(begin) = existing.find(MANAGED_BEGIN) {
                let end = existing[begin..]
                    .find(MANAGED_END)
                    .map(|end| begin + end + MANAGED_END.len());
                let Some(end) = end else {
                    // Unterminated block: treat the whole tail as the block and
                    // replace it, keeping everything before the opening marker.
                    let replaced = format!("{}{}", &existing[..begin], MANAGED_RULES);
                    return (replaced, InstallOutcome::Updated);
                };
                let updated = format!(
                    "{}{}{}",
                    &existing[..begin],
                    MANAGED_RULES,
                    &existing[end..]
                );
                if updated == existing {
                    (existing.to_string(), InstallOutcome::Unchanged)
                } else {
                    (updated, InstallOutcome::Updated)
                }
            } else {
                let mut out = existing.to_string();
                if !out.is_empty() && !out.ends_with("\n\n") {
                    if out.ends_with('\n') {
                        out.push('\n');
                    } else {
                        out.push_str("\n\n");
                    }
                }
                out.push_str(MANAGED_RULES);
                out.push('\n');
                (out, InstallOutcome::Updated)
            }
        }
    }
}

/// Removes the managed rules block, returning `None` when there is nothing to
/// remove. Only bytes between the markers (inclusive) are deleted; trailing
/// blank lines left behind are collapsed.
pub fn uninstall_agent_rules(content: &str) -> Option<String> {
    let begin = content.find(MANAGED_BEGIN)?;
    let end = content[begin..]
        .find(MANAGED_END)
        .map(|end| begin + end + MANAGED_END.len())?;
    let mut out = String::new();
    out.push_str(&content[..begin]);
    out.push_str(&content[end..]);
    while out.ends_with("\n\n") {
        out.pop();
    }
    Some(out)
}

/// Roles a document can play. Unknown roles are a schema error, so the set of
/// roles is closed and stable across versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentRole {
    Tasks,
    Plan,
    Architecture,
    Progress,
    Runbook,
    Decisions,
    Environment,
    Handoff,
    AgentInstructions,
}

impl DocumentRole {
    pub fn as_str(self) -> &'static str {
        match self {
            DocumentRole::Tasks => "tasks",
            DocumentRole::Plan => "plan",
            DocumentRole::Architecture => "architecture",
            DocumentRole::Progress => "progress",
            DocumentRole::Runbook => "runbook",
            DocumentRole::Decisions => "decisions",
            DocumentRole::Environment => "environment",
            DocumentRole::Handoff => "handoff",
            DocumentRole::AgentInstructions => "agent_instructions",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Enforcement {
    Observe,
    #[default]
    Guided,
    Enforced,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectContract {
    pub schema_version: u32,
    pub project: ProjectMeta,
    #[serde(default)]
    pub sources: Vec<SourceDecl>,
    #[serde(default)]
    pub documents: BTreeMap<DocumentRole, DocumentRef>,
    #[serde(default)]
    pub policy: Policy,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectMeta {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_status")]
    pub status: String,
}

fn default_status() -> String {
    "active".into()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceDecl {
    pub id: String,
    pub path: String,
    #[serde(default)]
    pub role: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentRef {
    pub path: String,
    #[serde(default)]
    pub auxiliary: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    #[serde(default)]
    pub agent_instructions: Option<String>,
    #[serde(default)]
    pub skill: Option<String>,
    #[serde(default)]
    pub enforcement: Enforcement,
    #[serde(default)]
    pub checks: Checks,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checks {
    #[serde(default)]
    pub require_task_reference_for_completion: bool,
    #[serde(default)]
    pub require_test_evidence: bool,
    #[serde(default)]
    pub require_session_handoff: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// A stable, evidence-carrying check result. Rule IDs (VPC001…) must not be
/// renamed; tests and CI gate on them.
#[derive(Debug, Clone, Serialize)]
pub struct ContractFinding {
    pub rule: &'static str,
    pub severity: Severity,
    pub message: String,
    /// Path the finding is about, relative to the project root when possible.
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContractCheckReport {
    pub contract_path: Option<String>,
    pub enforcement: Enforcement,
    pub findings: Vec<ContractFinding>,
}

impl ContractCheckReport {
    pub fn blocking(&self) -> bool {
        self.findings
            .iter()
            .any(|finding| finding.severity == Severity::Error)
    }
}

/// Loads `.virgo/project.yaml` under `root`. Missing file is `Ok(None)` — the
/// caller turns that into a VPC001 finding, not an Err, so `contract check`
/// reports evidence instead of failing opaque.
pub fn load_contract(
    root: &Path,
) -> std::result::Result<Option<(PathBuf, ProjectContract)>, Error> {
    let path = root.join(CONTRACT_DIR).join(CONTRACT_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&path).map_err(|source| io_error(&path, source))?;
    let contract: ProjectContract =
        serde_yaml::from_str(&raw).map_err(|error| Error::InvalidConfiguration {
            path: path.clone(),
            message: error.to_string(),
        })?;
    Ok(Some((path, contract)))
}

/// Validates a loaded contract against the project root. Deterministic: same
/// files in, same findings out.
pub fn check_contract(
    contract_path: Option<&Path>,
    contract: Option<&ProjectContract>,
    root: &Path,
) -> ContractCheckReport {
    let mut findings = Vec::new();
    let Some(contract) = contract else {
        findings.push(ContractFinding {
            rule: "VPC001",
            severity: Severity::Error,
            message: format!(
                "no project contract found; expected {CONTRACT_DIR}/{CONTRACT_FILE} under the project root"
            ),
            path: Some(format!("{CONTRACT_DIR}/{CONTRACT_FILE}")),
        });
        return ContractCheckReport {
            contract_path: contract_path.map(|path| display_path(Some(path))),
            enforcement: Enforcement::default(),
            findings,
        };
    };
    let enforcement = contract.policy.enforcement;

    // VPC001: schema-level validity.
    if contract.schema_version != CONTRACT_SCHEMA_VERSION {
        findings.push(ContractFinding {
            rule: "VPC001",
            severity: Severity::Error,
            message: format!(
                "unsupported contract schema_version {}; this build supports {}",
                contract.schema_version, CONTRACT_SCHEMA_VERSION
            ),
            path: None,
        });
    }
    if contract.project.id.trim().is_empty() {
        findings.push(empty_field("project.id"));
    }
    if contract.project.name.trim().is_empty() {
        findings.push(empty_field("project.name"));
    }
    let mut seen_sources = Vec::new();
    for source in &contract.sources {
        if source.id.trim().is_empty() {
            findings.push(empty_field("sources[].id"));
        }
        if seen_sources.contains(&source.id.as_str()) {
            findings.push(ContractFinding {
                rule: "VPC001",
                severity: Severity::Error,
                message: format!("duplicate source id `{}`", source.id),
                path: Some(display_path(contract_path)),
            });
        }
        seen_sources.push(source.id.as_str());
        if let Err(bad) = safe_relative_path(&source.path) {
            findings.push(unsafe_path("sources[].path", &source.path, bad));
        }
    }

    // VPC002: mapped paths exist; VPC003: roles missing a source.
    for (role, doc) in &contract.documents {
        if let Err(bad) = safe_relative_path(&doc.path) {
            findings.push(unsafe_path("documents", &doc.path, bad));
            continue;
        }
        let target = root.join(&doc.path);
        if !target.exists() {
            findings.push(ContractFinding {
                rule: "VPC002",
                severity: missing_role_severity(*role, enforcement),
                message: format!("document mapped as `{}` does not exist", role.as_str()),
                path: Some(doc.path.clone()),
            });
        }
    }
    for required in required_roles(&contract.policy.checks) {
        if !contract.documents.contains_key(&required) {
            findings.push(ContractFinding {
                rule: "VPC003",
                severity: missing_role_severity(required, enforcement),
                message: format!(
                    "role `{}` is required by the enabled policy checks but has no mapped document",
                    required.as_str()
                ),
                path: None,
            });
        }
    }

    // VPC004: two sources pointing at the same directory is a likely mistake.
    let mut seen_paths: Vec<&str> = Vec::new();
    for source in &contract.sources {
        if seen_paths.contains(&source.path.as_str()) {
            findings.push(ContractFinding {
                rule: "VPC004",
                severity: Severity::Warning,
                message: format!("sources `{}` and this entry share path `{}`; one should be removed or made auxiliary", seen_paths.iter().find(|p| **p == source.path).copied().unwrap_or("?"), source.path),
                path: Some(source.path.clone()),
            });
        }
        seen_paths.push(&source.path);
    }

    // VPC005: referenced agent instructions / skill files must exist.
    for (field, value) in [
        (
            "policy.agent_instructions",
            &contract.policy.agent_instructions,
        ),
        ("policy.skill", &contract.policy.skill),
    ] {
        let Some(value) = value else { continue };
        if safe_relative_path(value).is_err() {
            findings.push(unsafe_path(field, value, PathSafetyError::Unsafe));
            continue;
        }
        if !root.join(value).exists() {
            findings.push(ContractFinding {
                rule: "VPC005",
                severity: Severity::Warning,
                message: format!("{field} references a file that does not exist"),
                path: Some(value.clone()),
            });
        }
    }

    // VPC007: strict mode requires a handoff document.
    if enforcement == Enforcement::Enforced
        && !contract.documents.contains_key(&DocumentRole::Handoff)
    {
        findings.push(ContractFinding {
            rule: "VPC007",
            severity: Severity::Error,
            message: "enforced mode requires a `handoff` document mapping".into(),
            path: None,
        });
    }

    ContractCheckReport {
        contract_path: contract_path.map(|path| display_path(Some(path))),
        enforcement,
        findings,
    }
}

fn empty_field(field: &str) -> ContractFinding {
    ContractFinding {
        rule: "VPC001",
        severity: Severity::Error,
        message: format!("{field} must be a non-empty string"),
        path: None,
    }
}

fn unsafe_path(field: &str, value: &str, bad: PathSafetyError) -> ContractFinding {
    let message = match bad {
        PathSafetyError::Empty => format!("{field} is an empty path"),
        PathSafetyError::Absolute => {
            format!("{field} `{value}` must be a relative path")
        }
        PathSafetyError::Unsafe => {
            format!("{field} `{value}` escapes the project root or contains unsafe components")
        }
    };
    ContractFinding {
        rule: "VPC001",
        severity: Severity::Error,
        message,
        path: Some(value.to_string()),
    }
}

fn missing_role_severity(role: DocumentRole, enforcement: Enforcement) -> Severity {
    match (role, enforcement) {
        (DocumentRole::Tasks | DocumentRole::AgentInstructions, _) => Severity::Error,
        (_, Enforcement::Enforced) => Severity::Error,
        _ => Severity::Warning,
    }
}

fn required_roles(checks: &Checks) -> Vec<DocumentRole> {
    let mut roles = Vec::new();
    if checks.require_task_reference_for_completion {
        roles.push(DocumentRole::Tasks);
    }
    if checks.require_session_handoff {
        roles.push(DocumentRole::Handoff);
    }
    roles
}

/// Public boolean form of the path-safety rule, for installers that validate
/// contract-referenced paths before writing.
pub fn path_is_safe_relative(value: &str) -> bool {
    safe_relative_path(value).is_ok()
}

enum PathSafetyError {
    Empty,
    Absolute,
    Unsafe,
}

/// Paths in the contract are project-relative: no absolute paths, no `..`,
/// no empty segments that could resolve outside the authorized root.
fn safe_relative_path(value: &str) -> Result<(), PathSafetyError> {
    if value.trim().is_empty() {
        return Err(PathSafetyError::Empty);
    }
    let path = Path::new(value);
    if path.is_absolute() {
        return Err(PathSafetyError::Absolute);
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir => {}
            _ => return Err(PathSafetyError::Unsafe),
        }
    }
    Ok(())
}

fn display_path(path: Option<&Path>) -> String {
    path.map(|p| p.display().to_string())
        .unwrap_or_else(|| format!("{CONTRACT_DIR}/{CONTRACT_FILE}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root_with(files: &[(&str, &str)]) -> tempdir::TempDir {
        let dir = tempdir::tempdir().expect("tempdir");
        for (path, content) in files {
            let target = dir.path().join(path);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).expect("mkdir");
            }
            fs::write(target, content).expect("write");
        }
        dir
    }

    fn contract_yaml(yaml: &str) -> ProjectContract {
        serde_yaml::from_str(yaml).expect("valid contract")
    }

    const MINIMAL: &str = "schema_version: 1\nproject:\n  id: test-project\n  name: Test\nsources:\n  - id: main\n    path: .\n    role: code\n";

    #[test]
    fn minimal_contract_passes() {
        let dir = root_with(&[("TODO.md", "x"), ("AGENTS.md", "x")]);
        let contract = contract_yaml(MINIMAL);
        let report = check_contract(None, Some(&contract), dir.path());
        assert!(!report.blocking(), "findings: {:?}", report.findings);
    }

    #[test]
    fn missing_contract_is_vpc001() {
        let dir = root_with(&[]);
        let report = check_contract(None, None, dir.path());
        assert_eq!(report.findings[0].rule, "VPC001");
        assert!(report.blocking());
    }

    #[test]
    fn unknown_schema_version_is_vpc001() {
        let dir = root_with(&[]);
        let contract = contract_yaml("schema_version: 2\nproject:\n  id: a\n  name: b\n");
        let report = check_contract(None, Some(&contract), dir.path());
        assert!(report
            .findings
            .iter()
            .any(|f| f.rule == "VPC001" && f.message.contains("schema_version 2")));
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let error = serde_yaml::from_str::<ProjectContract>(
            "schema_version: 1\nproject:\n  id: a\n  name: b\nbogus: true\n",
        )
        .expect_err("unknown field must fail");
        assert!(error.to_string().contains("bogus"));
    }

    #[test]
    fn unknown_document_role_is_rejected() {
        let error = serde_yaml::from_str::<ProjectContract>(
            "schema_version: 1\nproject:\n  id: a\n  name: b\ndocuments:\n  nonsense:\n    path: x.md\n",
        )
        .expect_err("unknown role must fail");
        assert!(error.to_string().contains("nonsense"));
    }

    #[test]
    fn missing_mapped_document_is_vpc002() {
        let dir = root_with(&[("TODO.md", "x")]);
        let contract = contract_yaml(&format!(
            "{MINIMAL}documents:\n  tasks:\n    path: TODO.md\n  runbook:\n    path: docs/runbook.md\n"
        ));
        let report = check_contract(None, Some(&contract), dir.path());
        assert!(report
            .findings
            .iter()
            .any(|f| f.rule == "VPC002" && f.path.as_deref() == Some("docs/runbook.md")));
        // tasks exists, so only the runbook finding fires.
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn path_escaping_root_is_vpc001() {
        let dir = root_with(&[]);
        let contract = contract_yaml(
            "schema_version: 1\nproject:\n  id: test-project\n  name: Test\nsources:\n  - id: main\n    path: .\n  - id: escape\n    path: ../elsewhere\n",
        );
        let report = check_contract(None, Some(&contract), dir.path());
        assert!(report
            .findings
            .iter()
            .any(|f| f.rule == "VPC001" && f.message.contains("escapes")));
    }

    #[test]
    fn absolute_path_is_vpc001() {
        let dir = root_with(&[]);
        let contract = contract_yaml(&format!(
            "{MINIMAL}documents:\n  tasks:\n    path: /etc/passwd\n"
        ));
        let report = check_contract(None, Some(&contract), dir.path());
        assert!(report
            .findings
            .iter()
            .any(|f| f.rule == "VPC001" && f.message.contains("relative")));
    }

    #[test]
    fn duplicate_source_id_is_vpc001() {
        let dir = root_with(&[]);
        let contract = contract_yaml(
            "schema_version: 1\nproject:\n  id: test-project\n  name: Test\nsources:\n  - id: main\n    path: .\n  - id: main\n    path: .\n",
        );
        let report = check_contract(None, Some(&contract), dir.path());
        assert!(report
            .findings
            .iter()
            .any(|f| f.rule == "VPC001" && f.message.contains("duplicate source id")));
    }

    #[test]
    fn missing_required_role_is_vpc003() {
        let dir = root_with(&[("TODO.md", "x"), ("handoff.md", "x")]);
        let contract = contract_yaml(&format!(
            "{MINIMAL}documents:\n  tasks:\n    path: TODO.md\npolicy:\n  checks:\n    require_session_handoff: true\n"
        ));
        let report = check_contract(None, Some(&contract), dir.path());
        assert!(report
            .findings
            .iter()
            .any(|f| f.rule == "VPC003" && f.message.contains("handoff")));
    }

    #[test]
    fn missing_skill_reference_is_vpc005() {
        let dir = root_with(&[]);
        let contract = contract_yaml(&format!(
            "{MINIMAL}policy:\n  skill: .agents/skills/nope/SKILL.md\n"
        ));
        let report = check_contract(None, Some(&contract), dir.path());
        assert!(report.findings.iter().any(
            |f| f.rule == "VPC005" && f.path.as_deref() == Some(".agents/skills/nope/SKILL.md")
        ));
    }

    #[test]
    fn enforced_mode_without_handoff_is_vpc007() {
        let dir = root_with(&[("TODO.md", "x")]);
        let contract = contract_yaml(&format!(
            "{MINIMAL}documents:\n  tasks:\n    path: TODO.md\npolicy:\n  enforcement: enforced\n"
        ));
        let report = check_contract(None, Some(&contract), dir.path());
        assert!(report.findings.iter().any(|f| f.rule == "VPC007"));
    }

    #[test]
    fn install_creates_initial_file_with_one_block() {
        let (content, outcome) = install_agent_rules(None);
        assert_eq!(outcome, InstallOutcome::Created);
        assert_eq!(content.matches(MANAGED_BEGIN).count(), 1);
        assert!(content.starts_with("# AGENTS.md"));
    }

    #[test]
    fn repeated_install_is_idempotent() {
        let (first, outcome1) = install_agent_rules(None);
        let (second, outcome2) = install_agent_rules(Some(&first));
        assert_eq!(outcome1, InstallOutcome::Created);
        assert_eq!(outcome2, InstallOutcome::Unchanged);
        assert_eq!(first, second);
    }

    #[test]
    fn install_preserves_user_content_and_appends_block() {
        let user = "# My rules\n\n1. Keep it simple.\n";
        let (installed, outcome) = install_agent_rules(Some(user));
        assert_eq!(outcome, InstallOutcome::Updated);
        assert!(installed.starts_with(user));
        assert_eq!(installed.matches(MANAGED_BEGIN).count(), 1);
        // Idempotent on the second run.
        let (again, outcome2) = install_agent_rules(Some(&installed));
        assert_eq!(outcome2, InstallOutcome::Unchanged);
        assert_eq!(again, installed);
    }

    #[test]
    fn install_replaces_stale_block_in_place() {
        let stale = format!("{MANAGED_BEGIN}\nstale content\n{MANAGED_END}\n\nuser text\n");
        let (updated, outcome) = install_agent_rules(Some(&stale));
        assert_eq!(outcome, InstallOutcome::Updated);
        assert!(updated.contains("user text"));
        assert!(!updated.contains("stale content"));
        assert_eq!(updated.matches(MANAGED_BEGIN).count(), 1);
    }

    #[test]
    fn install_recovers_from_unterminated_block() {
        let broken = "user text\n\n<!-- BEGIN VIRGO MANAGED RULES -->\nno end marker\n";
        let (updated, outcome) = install_agent_rules(Some(broken));
        assert_eq!(outcome, InstallOutcome::Updated);
        assert!(updated.contains(MANAGED_END));
        assert!(!updated.contains("no end marker"));
        assert!(updated.starts_with("user text"));
    }

    #[test]
    fn uninstall_removes_only_the_block() {
        let user = "# My rules\n";
        let (installed, _) = install_agent_rules(Some(user));
        let removed = uninstall_agent_rules(&installed).expect("block present");
        assert_eq!(removed, user);
        assert!(uninstall_agent_rules(user).is_none());
    }

    #[test]
    fn load_contract_returns_none_when_absent_and_some_when_present() {
        let dir = root_with(&[]);
        assert!(load_contract(dir.path()).expect("load").is_none());
        fs::create_dir_all(dir.path().join(CONTRACT_DIR)).expect("mkdir");
        fs::write(dir.path().join(CONTRACT_DIR).join(CONTRACT_FILE), MINIMAL).expect("write");
        let (path, contract) = load_contract(dir.path()).expect("load").expect("some");
        assert!(path.ends_with("project.yaml"));
        assert_eq!(contract.project.id, "test-project");
    }

    #[test]
    fn invalid_yaml_is_invalid_configuration_error() {
        let dir = root_with(&[]);
        fs::create_dir_all(dir.path().join(CONTRACT_DIR)).expect("mkdir");
        fs::write(
            dir.path().join(CONTRACT_DIR).join(CONTRACT_FILE),
            "schema_version: [unclosed\n",
        )
        .expect("write");
        assert!(matches!(
            load_contract(dir.path()),
            Err(Error::InvalidConfiguration { .. })
        ));
    }

    // A tiny local tempdir helper: the workspace does not depend on the
    // `tempfile` crate and adding one for two call sites is not worth it.
    mod tempdir {
        use std::{
            env, fs,
            path::PathBuf,
            sync::atomic::{AtomicU64, Ordering},
        };

        static COUNTER: AtomicU64 = AtomicU64::new(0);

        pub struct TempDir(PathBuf);

        impl TempDir {
            pub fn path(&self) -> &std::path::Path {
                &self.0
            }
        }

        impl Drop for TempDir {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }

        pub fn tempdir() -> std::io::Result<TempDir> {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let pid = std::process::id();
            let dir = env::temp_dir().join(format!("todo-contract-test-{pid}-{n}"));
            fs::create_dir_all(&dir)?;
            Ok(TempDir(dir))
        }
    }
}
