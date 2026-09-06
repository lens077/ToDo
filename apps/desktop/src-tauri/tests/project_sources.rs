use std::{fs, time::Duration};
use todo_core::{CaptureStore, DesktopBackend, DesktopIntent};
use todo_desktop_lib::ops::DesktopOps;
const DOC: &str = "---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n---\n\n## Todo\n- [ ] task <!--t:aaaaaa-->\n";
fn ops(inbox: &std::path::Path) -> DesktopOps {
    DesktopOps::new(
        DesktopBackend::default(),
        CaptureStore::new(inbox, Duration::from_secs(2)),
    )
}
#[test]
fn native_sources_history_and_stale_root_writes_are_isolated() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    for root in [first.path(), second.path()] {
        fs::write(root.join("TODO.md"), DOC).unwrap();
        fs::write(root.join("TODO.txt"), DOC).unwrap();
    }
    let ops = ops(&first.path().join("inbox.md"));
    let snapshot = ops.open_project(first.path().to_str().unwrap()).unwrap();
    let priority = || DesktopIntent::SetPriority {
        id: "aaaaaa".into(),
        priority: Some(3),
        scope: todo_core::PriorityScope::Task,
    };
    assert!(ops.project_mutate(None, priority()).is_err());
    let changed = ops
        .project_mutate_for(Some("TODO.txt"), priority(), Some(&snapshot.project_path))
        .unwrap();
    assert_eq!(changed.tasks.len(), 2);
    assert_eq!(
        fs::read_to_string(first.path().join("TODO.md")).unwrap(),
        DOC
    );
    let history = ops.history_list_source(Some("TODO.txt")).unwrap();
    assert_eq!(history.len(), 1);
    assert!(ops.history_list_source(Some("TODO.md")).unwrap().is_empty());
    assert!(ops
        .history_preview_source(history[0].version, Some("TODO.md"))
        .is_err());
    ops.open_project(second.path().to_str().unwrap()).unwrap();
    assert!(ops
        .project_mutate_for(Some("TODO.txt"), priority(), Some(&snapshot.project_path))
        .is_err());
    assert_eq!(
        fs::read_to_string(second.path().join("TODO.txt")).unwrap(),
        DOC
    );
    assert!(ops
        .apply_rewrite_for(
            DOC,
            "unused",
            "restore",
            Some("TODO.txt"),
            Some(&snapshot.project_path)
        )
        .unwrap_err()
        .contains("project changed"));
    assert!(ops
        .capture_triage_for(
            "nonexistent",
            Some("todo"),
            Some("TODO.txt"),
            Some(&snapshot.project_path)
        )
        .unwrap_err()
        .contains("project changed"));
}
#[test]
fn native_source_adoption_requires_same_root_and_preview_hash() {
    let dir = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    for root in [dir.path(), other.path()] {
        fs::write(root.join("TODO"), "plan\n- [ ] work\n").unwrap();
    }
    let ops = ops(&dir.path().join("inbox.md"));
    let snapshot = ops.open_project(dir.path().to_str().unwrap()).unwrap();
    let preview = ops.preview_adopt("TODO").unwrap();
    ops.open_project(other.path().to_str().unwrap()).unwrap();
    assert!(ops
        .apply_adopt_for("TODO", &preview.base_hash, Some(&snapshot.project_path))
        .is_err());
    assert_eq!(
        fs::read_to_string(other.path().join("TODO")).unwrap(),
        "plan\n- [ ] work\n"
    );
}
