use std::{fs, path::Path, process::Command};

use tempfile::tempdir;
use todo_core::{parse, MockSemanticReconciler, ReconcileOptions, Reconciler, SemanticProposal};

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
fn repo() -> tempfile::TempDir {
    let dir = tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    git(dir.path(), &["config", "user.name", "Test"]);
    dir
}
fn document(evidence: &str, completed: bool) -> todo_core::Document {
    parse(format!("---\ntodo-spec: 1\ncolumns:\n  - {{id: todo, name: Todo}}\n  - {{id: done, name: Done, type: done}}\n---\n\n## {}\n- [{}] work <!--t:aaaaaa--> {evidence}\n",if completed{"Done"}else{"Todo"},if completed{"x"}else{" "})).unwrap()
}

#[test]
fn clean_repository_produces_empty_structural_report() {
    let dir = repo();
    fs::write(dir.path().join("code.rs"), "fn done() {}\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "implement"]);
    let sha = git(dir.path(), &["rev-parse", "HEAD"]);
    let doc = document(&format!("🔗{sha}..{sha}"), true);
    let report = Reconciler::new(dir.path())
        .reconcile(&doc, &ReconcileOptions::structural_only())
        .unwrap();
    assert!(report.confirmed.is_empty());
    assert!(report.suspected.is_empty());
}

#[test]
fn evidence_missing_and_standard_revert_are_confirmed_hygiene_only() {
    let dir = repo();
    fs::write(dir.path().join("code.rs"), "one\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "implement"]);
    let sha = git(dir.path(), &["rev-parse", "HEAD"]);
    fs::write(dir.path().join("code.rs"), "base\n").unwrap();
    git(
        dir.path(),
        &["commit", "-qam", "This reverts commit placeholder"],
    );
    git(
        dir.path(),
        &[
            "commit",
            "--allow-empty",
            "-qm",
            &format!("This reverts commit {sha}."),
        ],
    );
    let reverted = Reconciler::new(dir.path())
        .reconcile(
            &document(&format!("🔗{sha}..{sha}"), false),
            &ReconcileOptions::structural_only(),
        )
        .unwrap();
    assert_eq!(reverted.confirmed[0].judgment, "evidence-reverted");
    assert!(reverted.suspected.is_empty());
    assert!(!matches!(
        reverted.confirmed[0].suggested_action,
        Some(todo_core::SuggestedAction::SetCompleted { .. })
    ));
    let missing = Reconciler::new(dir.path())
        .reconcile(
            &document("🔗deadbeef..deadbeef", false),
            &ReconcileOptions::structural_only(),
        )
        .unwrap();
    assert_eq!(missing.confirmed[0].judgment, "evidence-missing");
    assert!(missing.suspected.is_empty());
}

#[test]
fn shallow_and_partial_repositories_skip_evidence_checks() {
    let origin = repo();
    fs::write(origin.path().join("code.rs"), "one\n").unwrap();
    git(origin.path(), &["add", "."]);
    git(origin.path(), &["commit", "-qm", "old"]);
    let old = git(origin.path(), &["rev-parse", "HEAD"]);
    fs::write(origin.path().join("code.rs"), "two\n").unwrap();
    git(origin.path(), &["commit", "-qam", "new"]);
    let parent = tempdir().unwrap();
    let clone_path = parent.path().join("shallow");
    git(
        parent.path(),
        &[
            "clone",
            "-q",
            "--depth=1",
            &format!("file://{}", origin.path().display()),
            clone_path.to_str().unwrap(),
        ],
    );
    let report = Reconciler::new(&clone_path)
        .reconcile(
            &document(&format!("🔗{old}..{old}"), false),
            &ReconcileOptions::structural_only(),
        )
        .unwrap();
    assert!(report.confirmed.is_empty());
    assert!(report.suspected.is_empty());
    assert!(!report.checks_skipped.is_empty());

    git(&clone_path, &["config", "remote.origin.promisor", "true"]);
    let partial = Reconciler::new(&clone_path)
        .reconcile(
            &document("🔗deadbeef..deadbeef", false),
            &ReconcileOptions::structural_only(),
        )
        .unwrap();
    assert!(partial.confirmed.is_empty());
    assert!(partial.suspected.is_empty());
    assert!(!partial.checks_skipped.is_empty());
}

#[test]
fn completed_status_never_moves_backward() {
    let dir = tempdir().unwrap();
    let doc = document("", true);
    let semantic =
        MockSemanticReconciler::new(vec![SemanticProposal::uncomplete("aaaaaa", "code missing")]);
    let report = Reconciler::new(dir.path())
        .with_semantic(&semantic)
        .reconcile(&doc, &ReconcileOptions::default())
        .unwrap();
    assert!(report.confirmed.is_empty());
    assert!(report
        .suspected
        .iter()
        .all(|item| item.suggested_change.is_none()));
}

#[test]
fn nongit_repository_degrades_without_evidence_claims() {
    let dir = tempdir().unwrap();
    let doc = document("🔗deadbeef..deadbeef", false);
    let report = Reconciler::new(dir.path())
        .reconcile(&doc, &ReconcileOptions::structural_only())
        .unwrap();
    assert!(report.confirmed.is_empty());
    assert!(report.suspected.is_empty());
    assert!(report
        .checks_skipped
        .iter()
        .any(|value| value.contains("git")));
}

#[test]
fn semantic_layer_receives_only_structural_candidates() {
    let dir = tempdir().unwrap();
    let doc = document("", false);
    let semantic = MockSemanticReconciler::default();
    Reconciler::new(dir.path())
        .with_semantic(&semantic)
        .reconcile(&doc, &ReconcileOptions::default())
        .unwrap();
    assert!(semantic.received().is_empty());
}
