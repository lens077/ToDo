use std::{fs, path::Path};
use tempfile::tempdir;
use todo_core::{apply_changes, parse, ReconcileOptions, Reconciler};

#[test]
fn only_unreferenced_duplicate_ids_are_auto_fixed() {
    let dir = tempdir().unwrap();
    let source="---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n---\n\n## Todo\n- [ ] first <!--t:aaaaaa--> ⛔missing\n- [ ] duplicate <!--t:aaaaaa-->\n- [ ] ref <!--t:bbbbbb--> →missing.md#cccccc\n";
    let document = parse(source).unwrap();
    let report = Reconciler::new(dir.path())
        .reconcile(&document, &ReconcileOptions::structural_only())
        .unwrap();
    assert_eq!(report.confirmed.len(), 3);
    assert!(report.suspected.is_empty());
    let changes: Vec<_> = report
        .confirmed
        .into_iter()
        .filter_map(|finding| finding.suggested_change)
        .collect();
    let fixed = apply_changes(&document, &changes).unwrap();
    let reparsed = parse(fixed).unwrap();
    assert!(reparsed.duplicate_ids().is_empty());
    assert!(reparsed.source.contains("⛔missing"));
    assert!(reparsed.source.contains("→missing.md#cccccc"));
    assert!(reparsed.tasks.iter().all(|task| !task.completed));
}

#[test]
fn referenced_duplicate_id_is_suspected_and_has_no_action() {
    let source = "---\ntodo-spec: 1\ncolumns: []\n---\n- [ ] first <!--t:aaaaaa-->\n- [ ] second <!--t:aaaaaa-->\n- [ ] waits <!--t:bbbbbb--> ⛔aaaaaa\n";
    let document = parse(source).unwrap();
    let report = Reconciler::new(Path::new("."))
        .reconcile(&document, &ReconcileOptions::structural_only())
        .unwrap();
    let duplicate = report
        .confirmed
        .iter()
        .find(|finding| finding.judgment == "duplicate-id")
        .unwrap();
    assert!(duplicate.suggested_change.is_none());
    assert!(duplicate.suggested_action.is_none());
}

#[test]
fn existing_reference_file_with_missing_target_id_is_confirmed() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("target.md");
    fs::write(
        &target,
        "---\ntodo-spec: 1\ncolumns: []\n---\n- [ ] other <!--t:dddddd-->\n",
    )
    .unwrap();
    let source = format!(
        "---\ntodo-spec: 1\ncolumns: []\n---\n- [ ] ref <!--t:aaaaaa--> →{}#cccccc\n",
        target.display()
    );
    let document = parse(source).unwrap();
    let report = Reconciler::new(Path::new("."))
        .reconcile(&document, &ReconcileOptions::structural_only())
        .unwrap();
    assert_eq!(report.confirmed.len(), 1);
    assert_eq!(report.confirmed[0].judgment, "dead-reference");
}
