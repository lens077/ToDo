use std::{collections::BTreeMap, fs};

use proptest::prelude::*;
use tempfile::tempdir;
use todo_core::{
    apply_changes, parse, parse_file, Change, Error, InsertPosition, LineEnding, OptimisticLock,
    TokenKind,
};

const DIRTY: &str = include_str!("fixtures/dirty.md");
const CRLF: &[u8] = include_bytes!("fixtures/dirty-crlf.md");

#[test]
fn parses_complete_model_and_excludes_fake_tasks() {
    let document = parse(DIRTY).unwrap();
    assert_eq!(document.frontmatter.todo_spec, 1);
    assert!(document.frontmatter.value.get("unknown-key").is_some());
    assert_eq!(document.columns.len(), 3);
    assert_eq!(document.tasks.len(), 5);
    assert_eq!(document.tasks[0].id.as_deref(), Some("a1b2c3"));
    assert_eq!(document.tasks[0].body, "全角：ＡＢＣ 与 emoji ✈️");
    assert_eq!(document.tasks[1].parent, Some(0));
    assert_eq!(document.tasks[0].children, vec![1]);
    assert_eq!(document.tasks[2].column, None);
    assert_eq!(document.tasks[3].checkbox, ' ');
    assert!(!document.tasks[3].completed);
    assert_eq!(document.tasks[0].unknown_tokens.len(), 1);
    assert_eq!(document.tasks[0].unknown_tokens[0].raw, "future:value");
    for task in &document.tasks {
        assert_eq!(
            &document.source[task.line_range.clone()],
            document.source[task.full_line_range.clone()].trim_end_matches(['\r', '\n'])
        );
    }
}

#[test]
fn round_trip_is_byte_identical_for_dirty_inputs() {
    let plain = parse(DIRTY).unwrap();
    assert_eq!(plain.write_unchanged(), DIRTY.as_bytes());
    let crlf_text = String::from_utf8(CRLF.to_vec()).unwrap();
    let crlf = parse(crlf_text.clone()).unwrap();
    assert_eq!(crlf.write_unchanged(), CRLF);
    assert!(crlf.bom);
    assert_eq!(crlf.line_ending, LineEnding::CrLf);
}

#[test]
fn rejects_non_task_versions_and_conflicts() {
    assert!(matches!(
        parse("# note\n- [ ] ordinary\n"),
        Err(Error::NotTaskFile)
    ));
    assert!(matches!(
        parse("---\ntodo-spec: 2\ncolumns: []\n---\n"),
        Err(Error::UnsupportedVersion { .. })
    ));
    assert!(matches!(
        parse("---\ntodo-spec: 1\ncolumns: []\n---\n<<<<<<< ours\n"),
        Err(Error::ConflictMarker { .. })
    ));
}

#[test]
fn surgical_operations_preserve_unrelated_content() {
    let document = parse(DIRTY).unwrap();
    let changed = apply_changes(
        &document,
        &[
            Change::SetCompleted {
                task: 0,
                completed: true,
            },
            Change::SetBody {
                task: 3,
                body: "新正文 ✨".into(),
            },
            Change::SetMetadata {
                task: 3,
                kind: TokenKind::Sort(2500),
                raw: "^2500".into(),
            },
            Change::RemoveMetadata { task: 0, token: 2 },
        ],
    )
    .unwrap();
    assert!(changed.contains("- [x] 全角"));
    assert!(changed.contains("+ [ ] 新正文 ✨"));
    assert!(changed.contains("!1 ^2500"));
    assert!(changed.contains("future:value"));
    assert!(changed.contains("```markdown\n- [ ] 围栏内假任务"));
    assert!(changed.contains("| checkbox | `- [ ] fake` |"));
}

#[test]
fn inserts_deletes_and_moves_only_whole_task_lines() {
    let document = parse(DIRTY).unwrap();
    let inserted = apply_changes(
        &document,
        &[Change::InsertTask {
            column: Some("doing".into()),
            position: InsertPosition::Start,
            line: "- [ ] 新任务 <!--t:z9y8x7-->".into(),
        }],
    )
    .unwrap();
    assert_eq!(inserted.matches("新任务 <!--t:z9y8x7-->").count(), 1);
    let inserted_document = parse(inserted.clone()).unwrap();
    assert_eq!(
        inserted_document.task("z9y8x7").unwrap().column.as_deref(),
        Some("doing")
    );

    let document = parse(DIRTY).unwrap();
    let deleted = apply_changes(&document, &[Change::DeleteTask { task: 1 }]).unwrap();
    assert!(!deleted.contains("嵌套任务"));
    assert!(deleted.contains("`示例：- [ ] 行内代码不算任务`"));

    let document = parse(DIRTY).unwrap();
    let moved = apply_changes(
        &document,
        &[Change::MoveTask {
            task: 2,
            column: "done".into(),
            position: InsertPosition::End,
        }],
    )
    .unwrap();
    assert_eq!(moved.matches("仍归上游待办").count(), 1);
    assert!(moved.contains("完成项 <!--t:m3n4o5--> ✅️2026-08-31\n- [x] 仍归上游待办"));
    assert!(moved.contains("## 普通章节\n\n说明不会改变。\n\n## 进行中"));
}

#[test]
fn optimistic_lock_and_atomic_write_detect_external_changes() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("TODO.md");
    fs::write(&path, DIRTY).unwrap();
    let document = parse_file(&path).unwrap();
    fs::write(&path, format!("{DIRTY}\nexternal\n")).unwrap();
    let result = OptimisticLock::new(&document).write(&[Change::SetCompleted {
        task: 0,
        completed: true,
    }]);
    assert!(matches!(result, Err(Error::ConcurrentModification)));
    assert!(fs::read_to_string(path).unwrap().ends_with("external\n"));
}

fn line_multiset_without(
    lines: impl Iterator<Item = String>,
    omitted: usize,
) -> BTreeMap<String, usize> {
    let mut values = BTreeMap::new();
    for (index, line) in lines.enumerate() {
        if index != omitted {
            *values.entry(line).or_default() += 1;
        }
    }
    values
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn arbitrary_single_task_edit_preserves_untouched_line_multiset(
        task_count in 1usize..30,
        chosen_seed in any::<usize>(),
        new_body in "[A-Za-z0-9一二三四五六七八九 ✈️_-]{1,32}",
        completed in any::<bool>(),
        crlf in any::<bool>(),
    ) {
        let newline = if crlf { "\r\n" } else { "\n" };
        let mut source = format!("---{newline}todo-spec: 1{newline}columns:{newline}  - {{id: todo, name: Todo}}{newline}---{newline}{newline}## Todo{newline}{newline}");
        for index in 0..task_count {
            source.push_str(&format!("- [ ] original-{index} <!--t:{index:06}--> unknown:{index}{newline}"));
        }
        source.push_str(&format!("{newline}| preserve | table |{newline}|---|---|{newline}| ![](/图.png) | `- [ ] fake` |{newline}"));
        let document = parse(source.clone()).unwrap();
        let chosen = chosen_seed % task_count;
        let changed = apply_changes(&document, &[
            Change::SetBody { task: chosen, body: new_body.clone() },
            Change::SetCompleted { task: chosen, completed },
        ]).unwrap();
        let before_lines: Vec<String> = source.split_inclusive('\n').map(str::to_owned).collect();
        let after_lines: Vec<String> = changed.split_inclusive('\n').map(str::to_owned).collect();
        prop_assert_eq!(before_lines.len(), after_lines.len());
        prop_assert_eq!(
            line_multiset_without(before_lines.into_iter(), 8 + chosen),
            line_multiset_without(after_lines.into_iter(), 8 + chosen),
        );
    }

    #[test]
    fn arbitrary_toggle_is_an_involution(chosen_seed in any::<usize>()) {
        let document = parse(DIRTY).unwrap();
        let chosen = chosen_seed % document.tasks.len();
        let toggled = apply_changes(&document, &[Change::SetCompleted {
            task: chosen,
            completed: !document.tasks[chosen].completed,
        }]).unwrap();
        let reparsed = parse(toggled).unwrap();
        let restored = apply_changes(&reparsed, &[Change::SetCompleted {
            task: chosen,
            completed: document.tasks[chosen].completed,
        }]).unwrap();
        prop_assert_eq!(restored.as_bytes(), DIRTY.as_bytes());
    }
}
