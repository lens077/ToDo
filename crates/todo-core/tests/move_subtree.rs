use todo_core::{apply_changes, parse, Change, InsertPosition};

const SOURCE: &str = "---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n  - {id: doing, name: Doing}\n  - {id: done, name: Done, type: done}\n---\n\n## Todo\n- [ ] parent <!--t:aaaaaa-->\n    - [ ] child <!--t:bbbbbb-->\n        - [ ] grandchild <!--t:cccccc-->\n- [ ] sibling <!--t:dddddd-->\n\n## Doing\n    - [ ] nested target <!--t:eeeeee-->\n\n## Done\n- [x] existing <!--t:ffffff-->\n";

#[test]
fn moving_parent_moves_entire_subtree_and_rebases_root_indent() {
    let document = parse(SOURCE).unwrap();
    let moved = apply_changes(
        &document,
        &[Change::MoveTask {
            task: 0,
            column: "doing".into(),
            position: InsertPosition::After(4),
        }],
    )
    .unwrap();
    let reparsed = parse(moved.clone()).unwrap();
    let parent = reparsed.task("aaaaaa").unwrap();
    let child = reparsed.task("bbbbbb").unwrap();
    let grandchild = reparsed.task("cccccc").unwrap();
    assert_eq!(parent.column.as_deref(), Some("doing"));
    assert_eq!(parent.indent_level, 0);
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
fn moving_into_done_completes_subtree_but_moving_out_does_not_uncomplete() {
    let document = parse(SOURCE).unwrap();
    let moved = apply_changes(
        &document,
        &[Change::MoveTask {
            task: 0,
            column: "done".into(),
            position: InsertPosition::End,
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
        }],
    )
    .unwrap();
    let todo = parse(moved_back).unwrap();
    for id in ["aaaaaa", "bbbbbb", "cccccc"] {
        assert!(todo.task(id).unwrap().completed, "{id} must stay completed");
    }
}
