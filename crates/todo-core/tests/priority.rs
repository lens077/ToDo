use todo_core::{apply_changes, parse, set_priority_changes, PriorityScope, TokenKind};

const SOURCE: &str = "---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n  - {id: done, name: Done, type: done}\n---\n\n## Todo\n- [ ] parent <!--t:aaaaaa--> !2 #core\n    - [ ] child <!--t:bbbbbb-->\n        - [ ] grandchild <!--t:cccccc--> !3\n- [ ] sibling <!--t:dddddd--> !4\n\n## Done\n- [x] finished <!--t:eeeeee--> !1\n";

fn priority(source: &str, id: &str) -> Option<u8> {
    parse(source)
        .unwrap()
        .task(id)
        .unwrap()
        .metadata
        .iter()
        .find_map(|token| {
            if let TokenKind::Priority(value) = token.kind {
                Some(value)
            } else {
                None
            }
        })
}

#[test]
fn sets_one_priority_without_touching_neighbors() {
    let document = parse(SOURCE).unwrap();
    let output = apply_changes(
        &document,
        &set_priority_changes(&document, "aaaaaa", Some(1), PriorityScope::Task).unwrap(),
    )
    .unwrap();
    assert_eq!(priority(&output, "aaaaaa"), Some(1));
    assert_eq!(priority(&output, "bbbbbb"), None);
    assert_eq!(priority(&output, "cccccc"), Some(3));
    assert!(output.contains("!1 #core"));
}

#[test]
fn applies_priority_to_whole_subtree_but_not_sibling() {
    let document = parse(SOURCE).unwrap();
    let output = apply_changes(
        &document,
        &set_priority_changes(&document, "aaaaaa", Some(4), PriorityScope::Subtree).unwrap(),
    )
    .unwrap();
    for id in ["aaaaaa", "bbbbbb", "cccccc"] {
        assert_eq!(priority(&output, id), Some(4));
    }
    assert_eq!(priority(&output, "dddddd"), Some(4));
    assert_eq!(priority(&output, "eeeeee"), Some(1));
}

#[test]
fn clears_subtree_priorities_and_preserves_unknown_content() {
    let document = parse(SOURCE).unwrap();
    let output = apply_changes(
        &document,
        &set_priority_changes(&document, "aaaaaa", None, PriorityScope::Subtree).unwrap(),
    )
    .unwrap();
    for id in ["aaaaaa", "bbbbbb", "cccccc"] {
        assert_eq!(priority(&output, id), None);
    }
    assert!(output.contains("<!--t:aaaaaa--> #core"));
}

#[test]
fn rejects_values_outside_format_range() {
    let document = parse(SOURCE).unwrap();
    assert!(set_priority_changes(&document, "aaaaaa", Some(5), PriorityScope::Task).is_err());
}

#[test]
fn same_priority_is_a_byte_identical_noop() {
    let document = parse(SOURCE).unwrap();
    let changes = set_priority_changes(&document, "dddddd", Some(4), PriorityScope::Task).unwrap();
    assert!(changes.is_empty());
    assert_eq!(apply_changes(&document, &changes).unwrap(), SOURCE);
}

#[test]
fn duplicate_priorities_are_normalized_without_overlapping_edits() {
    let source = "---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n---\n\n## Todo\n- [ ] work <!--t:aaaaaa--> !1 !2\n";
    let document = parse(source).unwrap();
    let output = apply_changes(
        &document,
        &set_priority_changes(&document, "aaaaaa", Some(4), PriorityScope::Task).unwrap(),
    )
    .unwrap();
    assert_eq!(output.matches("!4").count(), 1);
    assert!(!output.contains("!1"));
    assert!(!output.contains("!2"));
}

#[test]
fn subtree_stops_when_column_changes_even_if_next_task_is_indented() {
    let source = "---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n  - {id: other, name: Other}\n---\n\n## Todo\n- [ ] parent <!--t:aaaaaa--> !1\n    - [ ] child <!--t:bbbbbb--> !1\n## Other\n    - [ ] malformed indent <!--t:cccccc--> !3\n";
    let document = parse(source).unwrap();
    let output = apply_changes(
        &document,
        &set_priority_changes(&document, "aaaaaa", Some(2), PriorityScope::Subtree).unwrap(),
    )
    .unwrap();
    assert_eq!(priority(&output, "aaaaaa"), Some(2));
    assert_eq!(priority(&output, "bbbbbb"), Some(2));
    // The malformed indented task is orphaned by the parser, but its bytes still
    // prove the previous subtree did not cross the column boundary.
    assert!(output.contains("<!--t:cccccc--> !3"));
}
