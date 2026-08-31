//! Version history and the diff that feeds the confirmation UI.

use std::{fs, time::Duration};

use tempfile::tempdir;
use todo_core::{diff_lines, parse, Change, DiffKind, History, LockedWriter, STATE_DIR};

const DOC: &str = "---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n---\n\n## Todo\n\n- [ ] 第一件事 <!--t:aaaaaa-->\n- [ ] 第二件事 <!--t:bbbbbb-->\n";

fn writer(path: &std::path::Path) -> LockedWriter {
    LockedWriter::new(path, Duration::from_millis(2000))
}

#[test]
fn every_write_leaves_a_restorable_snapshot() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("TODO.md");
    fs::write(&path, DOC).unwrap();

    writer(&path)
        .update_labeled("done", |_| {
            Ok(vec![Change::SetCompleted {
                task: 0,
                completed: true,
            }])
        })
        .unwrap();

    let history = History::for_file(&path);
    let entries = history.entries().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].op, "done");
    assert_eq!(entries[0].file, "TODO.md");
    // The snapshot holds the state that was replaced, not the new one.
    assert_eq!(history.read(entries[0].version).unwrap(), DOC);
    assert!(fs::read_to_string(&path)
        .unwrap()
        .contains("- [x] 第一件事"));
}

#[test]
fn history_ignores_itself_out_of_git() {
    // Recovery state is machine-local. Landing in the user's commits would turn an
    // undo buffer into repository content and leak it into every clone.
    let dir = tempdir().unwrap();
    let path = dir.path().join("TODO.md");
    fs::write(&path, DOC).unwrap();

    writer(&path)
        .update_labeled("done", |_| {
            Ok(vec![Change::SetCompleted {
                task: 0,
                completed: true,
            }])
        })
        .unwrap();

    let ignore = dir.path().join(STATE_DIR).join(".gitignore");
    assert_eq!(fs::read_to_string(ignore).unwrap(), "*\n");
}

#[test]
fn a_write_that_changes_nothing_records_nothing() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("TODO.md");
    fs::write(&path, DOC).unwrap();

    writer(&path)
        .update_labeled("noop", |_| Ok(Vec::new()))
        .unwrap();

    assert!(History::for_file(&path).entries().unwrap().is_empty());
}

#[test]
fn restoring_an_old_version_is_itself_undoable() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("TODO.md");
    fs::write(&path, DOC).unwrap();
    let w = writer(&path);

    w.update_labeled("done", |_| {
        Ok(vec![Change::SetCompleted {
            task: 0,
            completed: true,
        }])
    })
    .unwrap();
    let after_first = fs::read_to_string(&path).unwrap();

    let history = History::for_file(&path);
    let original = history.read(1).unwrap();
    w.replace_all("restore", &original).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), DOC);

    // The restore itself is a write, so the state it replaced is recoverable too.
    let entries = history.entries().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[1].op, "restore");
    assert_eq!(history.read(entries[1].version).unwrap(), after_first);
}

#[test]
fn a_replacement_that_would_not_parse_never_reaches_disk() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("TODO.md");
    fs::write(&path, DOC).unwrap();

    let result = writer(&path).replace_all("scan-apply", "no frontmatter here\n");

    assert!(result.is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), DOC);
    assert!(History::for_file(&path).entries().unwrap().is_empty());
}

#[test]
fn prune_keeps_the_newest_versions_and_drops_their_blobs() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("TODO.md");
    fs::write(&path, DOC).unwrap();
    let w = writer(&path);

    for index in 0..5 {
        let completed = index % 2 == 0;
        w.update_labeled("toggle", |_| {
            Ok(vec![Change::SetCompleted { task: 0, completed }])
        })
        .unwrap();
    }

    let history = History::for_file(&path);
    let before = history.entries().unwrap();
    assert_eq!(before.len(), 5);

    assert_eq!(history.prune(2).unwrap(), 3);
    let after = history.entries().unwrap();
    assert_eq!(after.len(), 2);
    assert_eq!(after[0].version, 4);
    // Pruned snapshots are gone from disk, not merely unlisted.
    for entry in &before[..3] {
        assert!(!history.directory().join(&entry.blob).exists());
    }
    assert!(history.read(after[1].version).is_ok());
}

#[test]
fn a_truncated_index_line_does_not_destroy_the_rest_of_the_history() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("TODO.md");
    fs::write(&path, DOC).unwrap();
    let w = writer(&path);
    w.update_labeled("done", |_| {
        Ok(vec![Change::SetCompleted {
            task: 0,
            completed: true,
        }])
    })
    .unwrap();

    let history = History::for_file(&path);
    let index = history.directory().join("index.jsonl");
    let mut text = fs::read_to_string(&index).unwrap();
    text.push_str("{\"version\":2,\"at\":\"trunc");
    fs::write(&index, text).unwrap();

    let entries = history.entries().unwrap();
    assert_eq!(entries.len(), 1, "the intact entry must survive");
    assert!(history.read(1).is_ok());
}

#[test]
fn diff_reports_replaced_lines_as_a_removal_and_an_addition() {
    let before = "alpha\nbravo\ncharlie\n";
    let after = "alpha\nBRAVO\ncharlie\n";
    let diff = diff_lines(before, after);

    assert_eq!(diff.added, 1);
    assert_eq!(diff.removed, 1);
    let removed = diff
        .hunks
        .iter()
        .find(|hunk| hunk.kind == DiffKind::Removed)
        .unwrap();
    assert_eq!(removed.text, "bravo");
    assert_eq!(removed.left_line, Some(2));
    assert_eq!(removed.right_line, None);

    let added = diff
        .hunks
        .iter()
        .find(|hunk| hunk.kind == DiffKind::Added)
        .unwrap();
    assert_eq!(added.text, "BRAVO");
    assert_eq!(added.right_line, Some(2));
}

#[test]
fn identical_text_produces_an_empty_diff() {
    let diff = diff_lines(DOC, DOC);
    assert!(diff.is_empty());
    assert!(diff.hunks.iter().all(|hunk| hunk.kind == DiffKind::Context));
}

#[test]
fn diff_line_numbers_stay_aligned_after_an_insertion() {
    let before = "one\ntwo\n";
    let after = "one\ninserted\ntwo\n";
    let diff = diff_lines(before, after);

    assert_eq!(diff.added, 1);
    assert_eq!(diff.removed, 0);
    let context: Vec<_> = diff
        .hunks
        .iter()
        .filter(|hunk| hunk.kind == DiffKind::Context)
        .collect();
    assert_eq!(context[1].text, "two");
    // "two" is line 2 on the left and line 3 on the right; a UI that shows both
    // columns needs each side numbered independently.
    assert_eq!(context[1].left_line, Some(2));
    assert_eq!(context[1].right_line, Some(3));
}

#[test]
fn diff_against_a_parsed_document_uses_its_original_bytes() {
    let document = parse(DOC).unwrap();
    let proposed = DOC.replace("- [ ] 第一件事", "- [x] 第一件事");
    let diff = todo_core::diff_document(&document, &proposed);

    assert_eq!(diff.added, 1);
    assert_eq!(diff.removed, 1);
}
