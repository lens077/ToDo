//! Adoption wizard planning (spec: `docs/project-contract-spec.md`, phase 2).
//!
//! Scanning and planning are pure functions over the filesystem; the CLI turns
//! an [`AdoptPlan`] into writes. Files are only ever proposed for creation —
//! existing user documents are mapped by role, never migrated or rewritten
//! (except the managed agent-rules block, handled by the installer).

use std::{collections::BTreeMap, fs, path::Path};

use serde::Serialize;

use crate::{
    contract::{DocumentRef, DocumentRole},
    error::Error,
    Result,
};

/// One discovered file that could play a document role. Filename is only a
/// candidate hint; the final role comes from this scan's precedence or the
/// user's explicit contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AdoptionCandidate {
    pub role: DocumentRole,
    pub path: String,
}

/// What `todo project adopt` plans to do. Every write is a new file or a
/// backup-then-replace of a file Virgo manages; user files are never touched.
#[derive(Debug, Clone, Serialize)]
pub struct AdoptPlan {
    /// Files discovered and the roles proposed for them.
    pub candidates: Vec<AdoptionCandidate>,
    /// Roles whose candidates conflicted (two files, one role). The first
    /// candidate per precedence wins; the rest are listed here for review.
    pub conflicts: Vec<String>,
    /// Paths the plan would create or replace.
    pub writes: Vec<String>,
    /// True when a contract already exists; adoption then only proposes the
    /// missing integrations (agent block, skill).
    pub already_adopted: bool,
    /// The proposed `.virgo/project.yaml` body (empty when already adopted).
    pub proposed_contract: String,
}

/// Deterministic candidate table: first match per role wins, scanned in this
/// fixed order so repeated runs propose the same mapping.
const CANDIDATES: &[(DocumentRole, &[&str])] = &[
    (DocumentRole::Tasks, &["TODO.md", "TASKS.md"]),
    (DocumentRole::Plan, &["PLAN.md", "ROADMAP.md"]),
    (
        DocumentRole::Architecture,
        &[
            "ARCHITECTURE.md",
            "DESIGN.md",
            "docs/architecture.md",
            "docs/design.md",
        ],
    ),
    (DocumentRole::Progress, &["CONTEXT.md", "STATUS.md"]),
    (DocumentRole::Runbook, &["RUNBOOK.md", "docs/runbook.md"]),
    (
        DocumentRole::Decisions,
        &["DECISIONS.md", "docs/decisions.md"],
    ),
    (DocumentRole::AgentInstructions, &["AGENTS.md", "CLAUDE.md"]),
];

/// Scans `root` for candidate documents and builds the adoption proposal.
/// `project_name` seeds the generated contract; the ID is derived from it.
pub fn plan_adoption(root: &Path, project_name: &str) -> Result<AdoptPlan> {
    let existing = crate::contract::load_contract(root)?;
    let already_adopted = existing.is_some();
    let mut candidates = Vec::new();
    let mut conflicts = Vec::new();
    let mut documents: BTreeMap<DocumentRole, DocumentRef> = BTreeMap::new();
    for (role, names) in CANDIDATES {
        let mut found: Vec<String> = names
            .iter()
            .filter(|name| root.join(name).exists())
            .map(|name| name.to_string())
            .collect();
        if found.is_empty() {
            continue;
        }
        let primary = found.remove(0);
        for extra in found {
            conflicts.push(format!(
                "role `{}`: `{primary}` wins over `{extra}`",
                role.as_str()
            ));
        }
        documents.insert(
            *role,
            DocumentRef {
                path: primary,
                auxiliary: false,
            },
        );
        candidates.push(AdoptionCandidate {
            role: *role,
            path: documents[role].path.clone(),
        });
    }

    if already_adopted {
        return Ok(AdoptPlan {
            candidates,
            conflicts,
            writes: Vec::new(),
            already_adopted,
            proposed_contract: String::new(),
        });
    }

    let proposed_contract = render_contract(project_name, &documents);
    let mut writes = vec![format!(
        "{}/{}",
        crate::contract::CONTRACT_DIR,
        crate::contract::CONTRACT_FILE
    )];
    if !documents.contains_key(&DocumentRole::AgentInstructions) {
        writes.push("AGENTS.md".into());
    }
    Ok(AdoptPlan {
        candidates,
        conflicts,
        writes,
        already_adopted,
        proposed_contract,
    })
}

fn render_contract(project_name: &str, documents: &BTreeMap<DocumentRole, DocumentRef>) -> String {
    let id = project_id_from_name(project_name);
    let mut out = String::new();
    out.push_str("schema_version: 1\n\nproject:\n");
    out.push_str(&format!("  id: \"{id}\"\n"));
    out.push_str(&format!("  name: \"{}\"\n", yaml_escape(project_name)));
    out.push_str("  status: active\n\n");
    out.push_str("sources:\n  - id: \"main\"\n    path: \".\"\n    role: code\n\n");
    out.push_str("documents:\n");
    for (role, doc) in documents {
        out.push_str(&format!(
            "  {}:\n    path: \"{}\"\n",
            role.as_str(),
            doc.path
        ));
    }
    out.push_str(
        "\npolicy:\n  agent_instructions: \"AGENTS.md\"\n  enforcement: guided\n  checks:\n    require_task_reference_for_completion: true\n",
    );
    out
}

/// Stable, filesystem-safe project ID: lowercase ASCII slug of the name plus a
/// short random suffix so two projects with the same name never collide. The
/// suffix is generated once at adoption time and then never changes.
fn project_id_from_name(name: &str) -> String {
    let slug: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-').to_string();
    let slug = if slug.is_empty() {
        "project".to_string()
    } else {
        slug
    };
    let mut id = slug;
    // Collapse runs of dashes produced by the mapping above.
    while id.contains("--") {
        id = id.replace("--", "-");
    }
    let suffix = crate::generate_task_id(&std::collections::HashSet::new())
        .unwrap_or_else(|_| "000000".to_string());
    format!("{id}-{suffix}")
}

fn yaml_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Re-reads `path` and fails if it no longer matches `expected` — the
/// concurrency check required before every adoption write.
pub fn ensure_unmodified(path: &Path, expected: &str) -> Result<()> {
    let current =
        fs::read_to_string(path).map_err(|source| crate::error::io_error(path, source))?;
    if current != expected {
        return Err(Error::ConcurrentModification);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::ProjectContract;

    fn write(root: &Path, files: &[&str]) {
        for file in files {
            let target = root.join(file);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(target, "x").unwrap();
        }
    }

    #[test]
    fn scan_maps_existing_files_and_skips_missing() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), &["TODO.md", "CONTEXT.md"]);
        let plan = plan_adoption(dir.path(), "My Project").unwrap();
        assert!(plan
            .candidates
            .iter()
            .any(|c| c.role == DocumentRole::Tasks && c.path == "TODO.md"));
        assert!(plan
            .candidates
            .iter()
            .any(|c| c.role == DocumentRole::Progress && c.path == "CONTEXT.md"));
        assert!(!plan.candidates.iter().any(|c| c.role == DocumentRole::Plan));
        assert_eq!(plan.writes[0], ".virgo/project.yaml");
        assert!(plan.proposed_contract.contains("TODO.md"));
    }

    #[test]
    fn precedence_resolves_conflicts_and_records_them() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), &["TODO.md", "TASKS.md"]);
        let plan = plan_adoption(dir.path(), "P").unwrap();
        assert!(plan
            .candidates
            .iter()
            .any(|c| c.role == DocumentRole::Tasks && c.path == "TODO.md"));
        assert!(!plan.candidates.iter().any(|c| c.path == "TASKS.md"));
        assert!(plan
            .conflicts
            .iter()
            .any(|c| c.contains("TODO.md") && c.contains("TASKS.md")));
    }

    #[test]
    fn planning_is_deterministic_across_runs() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), &["TODO.md", "README.md", "DECISIONS.md"]);
        let a = plan_adoption(dir.path(), "P").unwrap();
        let b = plan_adoption(dir.path(), "P").unwrap();
        assert_eq!(
            serde_json::to_string(&a.candidates).unwrap(),
            serde_json::to_string(&b.candidates).unwrap()
        );
        // The proposed contract embeds a random ID suffix, so only compare the
        // mapping section: everything after `documents:`.
        let tail = |text: &str| text.split("documents:").nth(1).unwrap().to_string();
        assert_eq!(tail(&a.proposed_contract), tail(&b.proposed_contract));
    }

    #[test]
    fn already_adopted_projects_get_no_writes() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), &["TODO.md"]);
        fs::create_dir_all(dir.path().join(".virgo")).unwrap();
        fs::write(
            dir.path().join(".virgo/project.yaml"),
            "schema_version: 1\nproject:\n  id: p\n  name: P\n",
        )
        .unwrap();
        let plan = plan_adoption(dir.path(), "P").unwrap();
        assert!(plan.already_adopted);
        assert!(plan.writes.is_empty());
        assert!(plan.proposed_contract.is_empty());
    }

    #[test]
    fn generated_contract_passes_the_validator() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), &["TODO.md", "CONTEXT.md"]);
        let plan = plan_adoption(dir.path(), "My Project").unwrap();
        let contract: ProjectContract = serde_yaml::from_str(&plan.proposed_contract)
            .expect("generated contract must be valid yaml");
        let report = crate::contract::check_contract(None, Some(&contract), dir.path());
        assert!(!report.blocking(), "findings: {:?}", report.findings);
    }

    #[test]
    fn ensure_unmodified_detects_concurrent_edits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.md");
        fs::write(&path, "v1").unwrap();
        ensure_unmodified(&path, "v1").unwrap();
        fs::write(&path, "v2").unwrap();
        assert!(matches!(
            ensure_unmodified(&path, "v1"),
            Err(Error::ConcurrentModification)
        ));
    }
}
