use std::path::Path;

use tempfile::tempdir;
use todo_core::{parse, project_document, Projection};

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
fn mainline_projection_preserves_partial_failure_and_note_fields() {
    let document = parse(source(
        "- [ ] active <!--t:aaaaaa--> ◐70 ⚠2026-09-01\n      ↳ load test context <!--n:aaaaaa-->",
    ))
    .unwrap();

    let tasks = project_document(Path::new("TODO.md"), &document).unwrap();
    let active = tasks.iter().find(|task| task.id == "aaaaaa").unwrap();
    assert_eq!(active.progress, Some(70));
    assert_eq!(active.failed_on.as_deref(), Some("2026-09-01"));
    assert_eq!(active.note.as_deref(), Some("load test context"));
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
fn note_only_incremental_changes_refresh_the_projected_row() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("index.db");
    let original = parse(source(
        "- [ ] active <!--t:aaaaaa-->\n      ↳ old context <!--n:aaaaaa-->",
    ))
    .unwrap();
    let changed = parse(source(
        "- [ ] active <!--t:aaaaaa-->\n      ↳ fresh context <!--n:aaaaaa-->",
    ))
    .unwrap();
    let mut projection = Projection::open(path).unwrap();
    projection
        .rebuild(&[(Path::new("TODO.md"), &original)])
        .unwrap();

    let stats = projection
        .update_file(Path::new("TODO.md"), &changed)
        .unwrap();

    assert_eq!(stats.updated, 1);
    let task = projection
        .tasks()
        .unwrap()
        .into_iter()
        .find(|task| task.id == "aaaaaa")
        .unwrap();
    assert_eq!(task.note.as_deref(), Some("fresh context"));
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

/// M5 decision input (TODO `b6r1vy`): how the in-memory path and the SQLite
/// projection scale at 10k tasks. `#[ignore]`d so the default suite stays
/// fast; run with
/// `cargo test -p todo-core --release --test projection -- --ignored --nocapture benchmark`.
#[test]
#[ignore]
fn benchmark_projection_10k_tasks() {
    const TASKS: usize = 10_000;
    let body: String = (0..TASKS)
        .map(|index| {
            format!("- [ ] 任务 {index} <!--t:bench{index:06}--> #bench !2 📅2026-09-10\n")
        })
        .collect();
    let markdown = source(&body);

    let started = std::time::Instant::now();
    let document = parse(&markdown).unwrap();
    let parse_elapsed = started.elapsed();

    let directory = tempdir().unwrap();
    let mut projection = Projection::open(directory.path().join("index.db")).unwrap();
    let started = std::time::Instant::now();
    projection
        .rebuild(&[(Path::new("TODO.md"), &document)])
        .unwrap();
    let rebuild_elapsed = started.elapsed();

    // The edit the projection exists for: one checkbox flips, everything else
    // must survive untouched.
    let changed_markdown = markdown.replacen("- [ ] 任务 42", "- [x] 任务 42", 1);
    let changed = parse(&changed_markdown).unwrap();
    let started = std::time::Instant::now();
    let stats = projection
        .update_file(Path::new("TODO.md"), &changed)
        .unwrap();
    let update_elapsed = started.elapsed();
    assert_eq!(stats.updated, 1);
    assert_eq!(stats.deleted, 0);
    assert_eq!(projection.tasks().unwrap().len(), TASKS + 1);

    eprintln!(
        "10k tasks: parse {parse_elapsed:?}, projection rebuild {rebuild_elapsed:?}, incremental update {update_elapsed:?}"
    );
    // Generous ceilings keep this a regression tripwire, not a benchmark harness.
    assert!(parse_elapsed < std::time::Duration::from_secs(10));
    assert!(rebuild_elapsed < std::time::Duration::from_secs(10));
}
