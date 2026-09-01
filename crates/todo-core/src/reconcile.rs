use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    process::Command,
};

use crate::{generate_task_id, parse_file, Change, Document, Result, TokenKind};

#[derive(Debug, Clone, serde::Serialize)]
pub struct ReconcileReport {
    pub confirmed: Vec<ReconcileFinding>,
    pub suspected: Vec<ReconcileFinding>,
    /// Checks that could not run at all, e.g. the git evidence layer in a shallow
    /// clone. Serialized as `checks_skipped` to keep it distinct from the count of
    /// findings awaiting manual judgment, which is a different thing entirely.
    #[serde(rename = "checks_skipped")]
    pub checks_skipped: Vec<String>,
}

impl ReconcileReport {
    /// True when at least one layer did not run, so "no findings" is not a clean bill
    /// of health but an unfinished inspection.
    pub fn incomplete(&self) -> bool {
        !self.checks_skipped.is_empty()
    }

    pub fn has_findings(&self) -> bool {
        !self.confirmed.is_empty() || !self.suspected.is_empty()
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ReconcileFinding {
    pub task_id: Option<String>,
    pub judgment: String,
    pub evidence: Vec<Evidence>,
    #[serde(skip)]
    pub suggested_change: Option<Change>,
    pub suggested_action: Option<SuggestedAction>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Evidence {
    pub kind: String,
    pub value: String,
    pub path: Option<String>,
    pub line: Option<usize>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SuggestedAction {
    SetCompleted { completed: bool },
    ReplaceDuplicateId { new_id: String },
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SemanticCandidate {
    pub task_id: String,
    pub body: String,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone)]
pub struct SemanticProposal {
    pub task_id: String,
    pub judgment: String,
    pub evidence: Vec<Evidence>,
    pub completed: Option<bool>,
}
impl SemanticProposal {
    pub fn uncomplete(id: &str, reason: &str) -> Self {
        Self {
            task_id: id.into(),
            judgment: "semantic-uncomplete".into(),
            evidence: vec![Evidence {
                kind: "semantic".into(),
                value: reason.into(),
                path: None,
                line: None,
            }],
            completed: Some(false),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SemanticCheck {
    pub proposals: Vec<SemanticProposal>,
    pub checks_skipped: Vec<String>,
}

impl SemanticCheck {
    pub fn completed(proposals: Vec<SemanticProposal>) -> Self {
        Self {
            proposals,
            checks_skipped: Vec::new(),
        }
    }

    pub fn skipped(reason: impl Into<String>) -> Self {
        Self {
            proposals: Vec::new(),
            checks_skipped: vec![reason.into()],
        }
    }
}

pub trait SemanticReconciler {
    fn reconcile(&self, document: &Document, candidates: &[SemanticCandidate]) -> SemanticCheck;
}

#[derive(Default)]
pub struct MockSemanticReconciler {
    proposals: Vec<SemanticProposal>,
    received: RefCell<Vec<SemanticCandidate>>,
}
impl MockSemanticReconciler {
    pub fn new(proposals: Vec<SemanticProposal>) -> Self {
        Self {
            proposals,
            received: RefCell::new(Vec::new()),
        }
    }
    pub fn received(&self) -> Vec<SemanticCandidate> {
        self.received.borrow().clone()
    }
}
impl SemanticReconciler for MockSemanticReconciler {
    fn reconcile(&self, _document: &Document, candidates: &[SemanticCandidate]) -> SemanticCheck {
        *self.received.borrow_mut() = candidates.to_vec();
        SemanticCheck::completed(self.proposals.clone())
    }
}

#[derive(Debug, Clone)]
pub struct ReconcileOptions {
    pub semantic: bool,
}
impl Default for ReconcileOptions {
    fn default() -> Self {
        Self { semantic: true }
    }
}
impl ReconcileOptions {
    pub fn structural_only() -> Self {
        Self { semantic: false }
    }
}

pub struct Reconciler<'a> {
    root: PathBuf,
    semantic: Option<&'a dyn SemanticReconciler>,
}
impl<'a> Reconciler<'a> {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            semantic: None,
        }
    }
    pub fn with_semantic(mut self, semantic: &'a dyn SemanticReconciler) -> Self {
        self.semantic = Some(semantic);
        self
    }
    pub fn reconcile(
        &self,
        document: &Document,
        options: &ReconcileOptions,
    ) -> Result<ReconcileReport> {
        let mut report = ReconcileReport {
            confirmed: Vec::new(),
            suspected: Vec::new(),
            checks_skipped: Vec::new(),
        };
        structural_document(document, &mut report);
        let git = GitEvidence::open(&self.root);
        let mut candidates = Vec::new();
        match git {
            Some(git) if git.incomplete_history => report.checks_skipped.push(
                "git evidence check skipped: shallow or partial clone has incomplete history"
                    .into(),
            ),
            Some(git) => evidence_checks(document, &git, &mut report, &mut candidates),
            None => report
                .checks_skipped
                .push("git evidence check skipped: git unavailable or not a repository".into()),
        }
        if options.semantic && self.semantic.is_none() {
            // Asking for the semantic layer and silently getting nothing is the same
            // failure as the shallow-clone case: the caller believes a layer ran.
            report
                .checks_skipped
                .push("semantic check skipped: no semantic reconciler is configured".into());
        }

        if options.semantic {
            if let Some(semantic) = self.semantic {
                let check = semantic.reconcile(document, &candidates);
                report.checks_skipped.extend(
                    check
                        .checks_skipped
                        .into_iter()
                        .map(|reason| format!("semantic check skipped: {reason}")),
                );
                for proposal in check.proposals {
                    let completed = document
                        .task(&proposal.task_id)
                        .is_some_and(|task| task.completed);
                    let upward = proposal.completed == Some(true) && !completed;
                    report.suspected.push(ReconcileFinding {
                        task_id: Some(proposal.task_id.clone()),
                        judgment: proposal.judgment,
                        evidence: proposal.evidence,
                        suggested_change: if upward {
                            document
                                .tasks
                                .iter()
                                .position(|task| task.id.as_deref() == Some(&proposal.task_id))
                                .map(|task| Change::SetCompleted {
                                    task,
                                    completed: true,
                                })
                        } else {
                            None
                        },
                        suggested_action: if upward {
                            Some(SuggestedAction::SetCompleted { completed: true })
                        } else {
                            None
                        },
                    });
                }
            }
        }
        Ok(report)
    }
}

fn structural_document(document: &Document, report: &mut ReconcileReport) {
    let mut existing_ids: HashSet<_> = document
        .tasks
        .iter()
        .filter_map(|task| task.id.clone())
        .collect();
    let mut first_occurrence = HashSet::new();
    for (task_index, task) in document.tasks.iter().enumerate() {
        let Some(id) = &task.id else { continue };
        if !first_occurrence.insert(id.clone()) {
            if let Ok(new_id) = generate_task_id(&existing_ids) {
                existing_ids.insert(new_id.clone());
                if let Some((token_index, token)) = task
                    .metadata
                    .iter()
                    .enumerate()
                    .find(|(_, token)| matches!(token.kind, TokenKind::StableId(_)))
                {
                    report.confirmed.push(ReconcileFinding {
                        task_id: Some(id.clone()),
                        judgment: "duplicate-id".into(),
                        evidence: vec![Evidence {
                            kind: "stable-id".into(),
                            value: id.clone(),
                            path: None,
                            line: Some(line_number(document, task.line_range.start)),
                        }],
                        suggested_change: if id_is_referenced(document, id) {
                            None
                        } else {
                            Some(Change::SetMetadata {
                                task: task_index,
                                kind: TokenKind::StableId(new_id.clone()),
                                raw: format!("<!--t:{new_id}-->"),
                            })
                        },
                        suggested_action: if id_is_referenced(document, id) {
                            None
                        } else {
                            Some(SuggestedAction::ReplaceDuplicateId { new_id })
                        },
                    });
                    let _ = token_index;
                    let _ = token;
                }
            }
        }
    }
    let ids: HashSet<_> = document
        .tasks
        .iter()
        .filter_map(|task| task.id.clone())
        .collect();
    for item in &document.unrecognized_task_like_lines {
        report.suspected.push(ReconcileFinding {
            task_id: None,
            judgment: "nonstandard-checkbox".into(),
            evidence: vec![Evidence {
                kind: "source-line".into(),
                value: item.raw.clone(),
                path: None,
                line: Some(item.line),
            }],
            suggested_change: None,
            suggested_action: None,
        });
    }
    for task in &document.tasks {
        let Some(id) = task.id.clone() else { continue };
        for token in &task.metadata {
            if let TokenKind::Dependency(dep) = &token.kind {
                if !ids.contains(dep) {
                    report.confirmed.push(report_only_finding(
                        &id,
                        "orphan-dependency",
                        Evidence {
                            kind: "dependency".into(),
                            value: dep.clone(),
                            path: None,
                            line: Some(line_number(document, task.line_range.start)),
                        },
                    ));
                } else if task.completed
                    && document
                        .task(dep)
                        .is_some_and(|dependency| !dependency.completed)
                {
                    report.suspected.push(finding(
                        &id,
                        "completed-with-open-dependency",
                        Evidence {
                            kind: "dependency".into(),
                            value: dep.clone(),
                            path: None,
                            line: None,
                        },
                    ));
                }
            }
            if let TokenKind::Reference { path, id: target } = &token.kind {
                let target_path = Path::new(path);
                let dead = !target_path.exists()
                    || parse_file(target_path)
                        .map(|target_document| target_document.task(target).is_none())
                        .unwrap_or(false);
                if dead {
                    report.confirmed.push(report_only_finding(
                        &id,
                        "dead-reference",
                        Evidence {
                            kind: "reference".into(),
                            value: target.clone(),
                            path: Some(path.clone()),
                            line: Some(line_number(document, task.line_range.start)),
                        },
                    ));
                }
            }
        }
    }
}
fn report_only_finding(id: &str, judgment: &str, evidence: Evidence) -> ReconcileFinding {
    ReconcileFinding {
        task_id: Some(id.into()),
        judgment: judgment.into(),
        evidence: vec![evidence],
        suggested_change: None,
        suggested_action: None,
    }
}

fn id_is_referenced(document: &Document, id: &str) -> bool {
    document
        .tasks
        .iter()
        .flat_map(|task| &task.metadata)
        .any(|token| {
            matches!(&token.kind, TokenKind::Dependency(target) if target == id)
                || matches!(&token.kind, TokenKind::Reference { id: target, .. } if target == id)
        })
}

fn line_number(document: &Document, offset: usize) -> usize {
    document.source.as_bytes()[..offset]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
        + 1
}

fn finding(id: &str, judgment: &str, evidence: Evidence) -> ReconcileFinding {
    ReconcileFinding {
        task_id: Some(id.into()),
        judgment: judgment.into(),
        evidence: vec![evidence],
        suggested_change: None,
        suggested_action: None,
    }
}

fn evidence_checks(
    document: &Document,
    git: &GitEvidence,
    report: &mut ReconcileReport,
    candidates: &mut Vec<SemanticCandidate>,
) {
    for task in &document.tasks {
        let Some(id) = task.id.clone() else { continue };
        for token in &task.metadata {
            if let TokenKind::Evidence { from, to } = &token.kind {
                let mut evidence = vec![Evidence {
                    kind: "commit-range".into(),
                    value: format!("{from}..{to}"),
                    path: None,
                    line: None,
                }];
                if !git.exists(from) || !git.exists(to) {
                    report.confirmed.push(report_only_finding(
                        &id,
                        "evidence-missing",
                        evidence.remove(0),
                    ));
                } else if git.reverted(from) || git.reverted(to) {
                    report.confirmed.push(report_only_finding(
                        &id,
                        "evidence-reverted",
                        evidence.remove(0),
                    ));
                } else if !task.completed {
                    evidence.extend(
                        git.changed_files(from, to)
                            .into_iter()
                            .map(|path| Evidence {
                                kind: "changed-file".into(),
                                value: path.clone(),
                                path: Some(path),
                                line: None,
                            }),
                    );
                    candidates.push(SemanticCandidate {
                        task_id: id.clone(),
                        body: task.body.clone(),
                        evidence,
                    });
                }
            }
        }
    }
}

struct GitEvidence {
    root: PathBuf,
    messages: HashMap<String, String>,
    incomplete_history: bool,
}
impl GitEvidence {
    fn open(root: &Path) -> Option<Self> {
        if Command::new("git")
            .current_dir(root)
            .args(["rev-parse", "--is-inside-work-tree"])
            .output()
            .ok()
            .is_none_or(|output| !output.status.success())
        {
            return None;
        }
        let shallow = Command::new("git")
            .current_dir(root)
            .args(["rev-parse", "--is-shallow-repository"])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .is_some_and(|output| String::from_utf8_lossy(&output.stdout).trim() == "true");
        let partial = Command::new("git")
            .current_dir(root)
            .args(["config", "--get", "remote.origin.promisor"])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .is_some_and(|output| String::from_utf8_lossy(&output.stdout).trim() == "true");
        let output = Command::new("git")
            .current_dir(root)
            .args(["log", "--format=%H%x00%B%x00%x1e"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let mut messages = HashMap::new();
        for record in text.split('\x1e') {
            let mut parts = record.trim().splitn(2, '\0');
            if let (Some(sha), Some(message)) = (parts.next(), parts.next()) {
                messages.insert(sha.trim().into(), message.trim_matches('\0').trim().into());
            }
        }
        Some(Self {
            root: root.into(),
            messages,
            incomplete_history: shallow || partial,
        })
    }
    fn exists(&self, sha: &str) -> bool {
        Command::new("git")
            .current_dir(&self.root)
            .args(["cat-file", "-e", &format!("{sha}^{{commit}}")])
            .output()
            .is_ok_and(|o| o.status.success())
    }
    fn reverted(&self, sha: &str) -> bool {
        self.messages
            .values()
            .any(|message| message.contains(&format!("This reverts commit {sha}")))
    }
    fn changed_files(&self, from: &str, to: &str) -> Vec<String> {
        Command::new("git")
            .current_dir(&self.root)
            .args(["diff", "--name-only", &format!("{from}^..{to}")])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    }
}
