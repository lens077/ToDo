use todo_core::parse;

#[test]
fn unmatched_h2_makes_following_tasks_orphans() {
    let source = "---\ntodo-spec: 1\ncolumns:\n  - {id: done, name: Done, type: done}\n---\n\n## Done\n- [x] managed <!--t:aaaaaa-->\n\n## Notes\n- [ ] orphan <!--t:bbbbbb-->\n";
    let document = parse(source).unwrap();
    assert_eq!(
        document.task("aaaaaa").unwrap().column.as_deref(),
        Some("done")
    );
    assert_eq!(document.task("bbbbbb").unwrap().column, None);
}

#[test]
fn records_nonstandard_checkbox_lines_but_excludes_code() {
    let source = "---\ntodo-spec: 1\ncolumns: []\n---\n- [-] migrated\n- [/] waiting\n`- [-] inline`\n```markdown\n- [/] fenced\n```\n- [ ] managed <!--t:aaaaaa-->\n";
    let document = parse(source).unwrap();
    assert_eq!(document.tasks.len(), 1);
    assert_eq!(document.unrecognized_task_like_lines.len(), 2);
    assert_eq!(document.unrecognized_task_like_lines[0].line, 5);
    assert_eq!(
        document.unrecognized_task_like_lines[0].raw,
        "- [-] migrated"
    );
    assert_eq!(document.unrecognized_task_like_lines[1].line, 6);
    assert_eq!(
        document.unrecognized_task_like_lines[1].raw,
        "- [/] waiting"
    );
}
