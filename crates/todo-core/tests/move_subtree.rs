use todo_core::{apply_changes, parse, Change, InsertPosition};

const SOURCE: &str = "---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n  - {id: doing, name: Doing}\n  - {id: done, name: Done, type: done}\n---\n\n## Todo\n- [ ] parent <!--t:aaaaaa-->\n    - [ ] child <!--t:bbbbbb-->\n        - [ ] grandchild <!--t:cccccc-->\n- [ ] sibling <!--t:dddddd-->\n\n## Doing\n- [ ] target parent <!--t:eeeeee-->\n    - [ ] nested target <!--t:111111-->\n\n## Done\n- [x] existing <!--t:ffffff-->\n";

#[test]
fn moving_parent_moves_entire_subtree_and_rebases_root_indent() {
    let document = parse(SOURCE).unwrap();
    let moved = apply_changes(
        &document,
        &[Change::MoveTask {
            task: 0,
            column: "doing".into(),
            position: InsertPosition::After(5),
            completion_note: None,
            completion_evidence: None,
        }],
    )
    .unwrap();
    let reparsed = parse(moved.clone()).unwrap();
    let parent = reparsed.task("aaaaaa").unwrap();
    let child = reparsed.task("bbbbbb").unwrap();
    let grandchild = reparsed.task("cccccc").unwrap();
    assert_eq!(parent.column.as_deref(), Some("doing"));
    assert_eq!(parent.indent_level, 1);
    assert_eq!(
        parent.parent,
        Some(
            reparsed
                .tasks
                .iter()
                .position(|task| task.id.as_deref() == Some("eeeeee"))
                .unwrap()
        )
    );
    assert_eq!(
        child.parent,
        Some(
            reparsed
                .tasks
                .iter()
                .position(|task| task.id.as_deref() == Some("aaaaaa"))
                .unwrap()
        )
    );
    assert_eq!(
        grandchild.parent,
        Some(
            reparsed
                .tasks
                .iter()
                .position(|task| task.id.as_deref() == Some("bbbbbb"))
                .unwrap()
        )
    );
    assert_eq!(moved.matches("<!--t:bbbbbb-->").count(), 1);
    assert_eq!(moved.matches("<!--t:cccccc-->").count(), 1);
}

#[test]
fn a_move_cannot_target_its_own_descendant() {
    let document = parse(SOURCE).unwrap();
    let result = apply_changes(
        &document,
        &[Change::MoveTask {
            task: 0,
            column: "todo".into(),
            position: InsertPosition::Before(1),
            completion_note: None,
            completion_evidence: None,
        }],
    );
    assert!(result.is_err());
}

#[test]
fn active_reorders_preserve_descendant_completion_state() {
    let source = SOURCE.replace("- [ ] child <!--t:bbbbbb-->", "- [x] child <!--t:bbbbbb-->");
    let document = parse(source).unwrap();
    let moved = apply_changes(
        &document,
        &[Change::MoveTask {
            task: 0,
            column: "doing".into(),
            position: InsertPosition::Start,
            completion_note: None,
            completion_evidence: None,
        }],
    )
    .unwrap();
    let reparsed = parse(moved).unwrap();
    assert!(!reparsed.task("aaaaaa").unwrap().completed);
    assert!(reparsed.task("bbbbbb").unwrap().completed);
    assert!(!reparsed.task("cccccc").unwrap().completed);
}

#[test]
fn moving_a_parent_into_done_resolves_descendant_outcome_tokens() {
    let source = SOURCE.replace(
        "- [ ] child <!--t:bbbbbb-->",
        "- [ ] child <!--t:bbbbbb--> ◐50\n          ↳ half complete <!--n:bbbbbb-->",
    );
    let document = parse(source).unwrap();
    let moved = apply_changes(
        &document,
        &[Change::MoveTask {
            task: 0,
            column: "done".into(),
            position: InsertPosition::Start,
            completion_note: Some("parent accepted".into()),
            completion_evidence: None,
        }],
    )
    .unwrap();
    let reparsed = parse(moved).unwrap();
    let child = reparsed.task("bbbbbb").unwrap();
    assert!(child.completed);
    assert_eq!(child.progress(), None);
    assert_eq!(
        child.note.as_ref().map(|note| note.text.as_str()),
        Some("half complete")
    );
}

#[test]
fn moving_into_done_completes_subtree_and_moving_out_reopens_it() {
    let document = parse(SOURCE).unwrap();
    let moved = apply_changes(
        &document,
        &[Change::MoveTask {
            task: 0,
            column: "done".into(),
            position: InsertPosition::End,
            completion_note: None,
            completion_evidence: None,
        }],
    )
    .unwrap();
    let done = parse(moved).unwrap();
    for id in ["aaaaaa", "bbbbbb", "cccccc"] {
        assert!(done.task(id).unwrap().completed, "{id} should be completed");
    }
    let parent_index = done
        .tasks
        .iter()
        .position(|task| task.id.as_deref() == Some("aaaaaa"))
        .unwrap();
    let moved_back = apply_changes(
        &done,
        &[Change::MoveTask {
            task: parent_index,
            column: "todo".into(),
            position: InsertPosition::Start,
            completion_note: None,
            completion_evidence: None,
        }],
    )
    .unwrap();
    let todo = parse(moved_back).unwrap();
    for id in ["aaaaaa", "bbbbbb", "cccccc"] {
        assert!(!todo.task(id).unwrap().completed, "{id} should be reopened");
    }
}

#[test]
fn move_completion_rejects_multiline_notes() {
    let document = parse(SOURCE).unwrap();
    let result = apply_changes(
        &document,
        &[Change::MoveTask {
            task: 0,
            column: "done".into(),
            position: InsertPosition::Start,
            completion_note: Some("accepted\n- [ ] injected".into()),
            completion_evidence: None,
        }],
    );
    assert!(result.is_err());
}

#[test]
fn reopening_a_task_moves_its_note_and_preserves_evidence() {
    let source = SOURCE.replace(
        "- [x] existing <!--t:ffffff-->",
        "- [x] existing <!--t:ffffff--> 🔗abc..def\n        ↳  accepted earlier   <!--n:ffffff-->",
    );
    let document = parse(source).unwrap();
    let task = document
        .tasks
        .iter()
        .position(|task| task.id.as_deref() == Some("ffffff"))
        .unwrap();
    let moved = apply_changes(
        &document,
        &[Change::MoveTask {
            task,
            column: "todo".into(),
            position: InsertPosition::Start,
            completion_note: None,
            completion_evidence: None,
        }],
    )
    .unwrap();
    let reopened = parse(moved.clone()).unwrap();
    let task = reopened.task("ffffff").unwrap();
    assert!(!task.completed);
    assert_eq!(
        task.note.as_ref().map(|note| note.text.as_str()),
        Some("accepted earlier")
    );
    assert_eq!(task.evidence(), Some(("abc", "def")));
    assert_eq!(moved.matches("<!--n:ffffff-->").count(), 1);
    assert!(moved.contains("        ↳  accepted earlier   <!--n:ffffff-->"));
}

#[test]
fn matching_same_column_move_preserves_note_bytes_and_eof() {
    let source = "---\ntodo-spec: 1\ncolumns:\n  - {id: done, name: Done, type: done}\n---\n\n## Done\n- [x] existing <!--t:ffffff-->\n        ↳  accepted earlier   <!--n:ffffff-->";
    let document = parse(source).unwrap();
    let moved = apply_changes(
        &document,
        &[Change::MoveTask {
            task: 0,
            column: "done".into(),
            position: InsertPosition::Start,
            completion_note: None,
            completion_evidence: None,
        }],
    )
    .unwrap();
    assert_eq!(moved, source);
}

#[test]
fn moving_after_the_last_task_is_a_byte_exact_noop() {
    let document = parse(SOURCE).unwrap();
    let moved = apply_changes(
        &document,
        &[Change::MoveTask {
            task: 3,
            column: "todo".into(),
            position: InsertPosition::After(3),
            completion_note: None,
            completion_evidence: None,
        }],
    )
    .unwrap();
    assert_eq!(moved, SOURCE);
}

#[test]
fn moving_after_a_parent_keeps_its_children_attached() {
    let source = "---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n---\n\n## Todo\n- [ ] mover <!--t:aaaaaa-->\n- [ ] parent <!--t:bbbbbb-->\n    - [ ] child <!--t:cccccc-->\n";
    let document = parse(source).unwrap();
    let moved = apply_changes(
        &document,
        &[Change::MoveTask {
            task: 0,
            column: "todo".into(),
            position: InsertPosition::After(1),
            completion_note: None,
            completion_evidence: None,
        }],
    )
    .unwrap();
    let reparsed = parse(moved).unwrap();
    let parent = reparsed
        .tasks
        .iter()
        .position(|task| task.id.as_deref() == Some("bbbbbb"))
        .unwrap();
    assert_eq!(reparsed.task("cccccc").unwrap().parent, Some(parent));
    assert_eq!(reparsed.task("aaaaaa").unwrap().parent, None);
}
