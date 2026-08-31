use std::path::Path;

use tempfile::tempdir;
use todo_core::{parse, Projection};

fn source(body: &str) -> String {
    format!("---\ntodo-spec: 1\ncolumns:\n  - {{id: todo, name: Todo}}\n  - {{id: done, name: Done, type: done}}\n---\n\n## Todo\n{body}\n## Done\n- [x] shipped <!--t:cccccc--> ✅2026-08-30\n")
}

#[test]
fn projects_all_task_fields_and_relations() {
    let directory = tempdir().unwrap();
    let mut projection = Projection::open(directory.path().join("index.db")).unwrap();
    let document = parse(source("- [ ] parent <!--t:aaaaaa--> #core #rust @me 📅2026-09-10 !2 ^1500 🔗abc..def ⛔cccccc\n    - [ ] child <!--t:bbbbbb-->" )).unwrap();
    projection
        .rebuild(&[(Path::new("TODO.md"), &document)])
        .unwrap();

    let tasks = projection.tasks().unwrap();
    assert_eq!(tasks.len(), 3);
    let parent = tasks.iter().find(|task| task.id == "aaaaaa").unwrap();
    assert_eq!(parent.column.as_deref(), Some("todo"));
    assert_eq!(parent.due.as_deref(), Some("2026-09-10"));
    assert_eq!(parent.tags, vec!["core", "rust"]);
    assert_eq!(parent.assignees, vec!["me"]);
    assert_eq!(parent.priority, Some(2));
    assert_eq!(parent.order_key, Some(1500));
    assert_eq!(parent.evidence, vec!["abc..def"]);
    assert_eq!(parent.deps, vec!["cccccc"]);
    let child = tasks.iter().find(|task| task.id == "bbbbbb").unwrap();
    assert_eq!(child.parent_id.as_deref(), Some("aaaaaa"));
    assert_eq!(child.line, 10);
}

#[test]
fn incremental_update_matches_full_rebuild_and_preserves_unaffected_rows() {
    let directory = tempdir().unwrap();
    let incremental_path = directory.path().join("incremental.db");
    let rebuilt_path = directory.path().join("rebuilt.db");
    let original = parse(source(
        "- [ ] alpha <!--t:aaaaaa--> #old\n- [ ] beta <!--t:bbbbbb-->",
    ))
    .unwrap();
    let changed = parse(source(
        "- [x] alpha renamed <!--t:aaaaaa--> #new !1\n- [ ] beta <!--t:bbbbbb-->",
    ))
    .unwrap();

    let mut incremental = Projection::open(&incremental_path).unwrap();
    incremental
        .rebuild(&[(Path::new("TODO.md"), &original)])
        .unwrap();
    let beta_rowid_before = incremental.task_rowid("bbbbbb").unwrap().unwrap();
    let stats = incremental
        .update_file(Path::new("TODO.md"), &changed)
        .unwrap();
    assert_eq!(stats.inserted, 0);
    assert_eq!(stats.updated, 1);
    assert_eq!(stats.deleted, 0);
    assert_eq!(
        incremental.task_rowid("bbbbbb").unwrap(),
        Some(beta_rowid_before)
    );

    let mut rebuilt = Projection::open(&rebuilt_path).unwrap();
    rebuilt
        .rebuild(&[(Path::new("TODO.md"), &changed)])
        .unwrap();
    assert_eq!(incremental.snapshot().unwrap(), rebuilt.snapshot().unwrap());
}

#[test]
fn schema_hash_mismatch_rebuilds_derived_tables() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("index.db");
    let document = parse(source("- [ ] alpha <!--t:aaaaaa-->")).unwrap();
    {
        let mut projection = Projection::open(&path).unwrap();
        projection
            .rebuild(&[(Path::new("TODO.md"), &document)])
            .unwrap();
        projection.set_schema_hash_for_test("obsolete").unwrap();
    }
    let projection = Projection::open(&path).unwrap();
    assert!(projection.tasks().unwrap().is_empty());
    assert_eq!(projection.schema_hash().unwrap(), todo_core::SCHEMA_HASH);
}
