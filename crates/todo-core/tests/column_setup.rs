use std::{fs, path::Path};

use tempfile::{tempdir, TempDir};
use todo_core::{
    parse, ColumnHeadingSelection, ColumnSetupSelection, DesktopIntent, Error, History, MoveTarget,
    ProjectBackend,
};

const ECOMMERCE: &str = include_str!("fixtures/ecommerce-tasks.txt");

fn project(text: &str) -> (TempDir, ProjectBackend) {
    let temp = tempdir().unwrap();
    fs::write(temp.path().join("TODO.md"), text).unwrap();
    let mut backend = ProjectBackend::default();
    backend.open_project(temp.path()).unwrap();
    (temp, backend)
}

fn bytes(root: &Path) -> String {
    fs::read_to_string(root.join("TODO.md")).unwrap()
}

#[test]
fn direct_start_previews_one_atomic_move_without_losing_multiline_task_content() {
    let text = "---\ntodo-spec: 1\nowner: ecommerce\n---\n# 项目\n\n## 分类明细\n### 交易\n- [ ] **库存预占正确性**：保留长任务 <!--t:parent1--> !2\n  普通续行：不会留在原地。\n    - [ ] 子任务 <!--t:child01-->\n      ↳ 子任务说明 <!--n:child01-->\n\n  仍属于父任务的段落。\n\n- [ ] 其他任务 <!--t:other01-->\n\n### 下一领域\n正文和标题不要跟着移动。\n";
    let (temp, backend) = project(text);
    let selection = ColumnSetupSelection::DefaultsAndStart {
        task_id: "parent1".into(),
    };
    let preview = backend.preview_columns("TODO.md", &selection).unwrap();
    assert_eq!(bytes(temp.path()), text);
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    let parsed = parse(preview.proposed.clone()).unwrap();
    assert_eq!(parsed.tasks.len(), 3);
    for id in ["parent1", "child01"] {
        let task = parsed.task(id).unwrap();
        assert_eq!(task.column.as_deref(), Some("doing"));
        assert!(!task.completed);
    }
    assert_eq!(parsed.task("other01").unwrap().column, None);
    let doing = parsed.columns.iter().find(|c| c.id == "doing").unwrap();
    let moved = &parsed.source[doing.content_range.clone()];
    assert!(moved.contains("普通续行：不会留在原地。"));
    assert!(moved.contains("仍属于父任务的段落。"));
    assert!(moved.contains("↳ 子任务说明 <!--n:child01-->"));
    assert!(!moved.contains("其他任务"));
    assert!(!moved.contains("下一领域"));
    assert!(preview
        .proposed
        .contains("### 下一领域\n正文和标题不要跟着移动。"));
    let written = backend
        .apply_columns("TODO.md", &preview.base_hash, &selection)
        .unwrap();
    assert_eq!(bytes(temp.path()), preview.proposed);
    assert_eq!(
        written
            .tasks
            .iter()
            .find(|t| t.id.as_deref() == Some("parent1"))
            .unwrap()
            .column
            .as_deref(),
        Some("doing")
    );
    let history = History::for_file(temp.path().join("TODO.md"));
    assert_eq!(history.entries().unwrap().len(), 1);
    assert_eq!(
        history.read(history.entries().unwrap()[0].version).unwrap(),
        text
    );
    assert!(matches!(
        backend.apply_columns("TODO.md", &preview.base_hash, &selection),
        Err(Error::ConcurrentModification)
    ));
}

#[test]
fn direct_start_preserves_crlf_nested_item_and_unrelated_tasks() {
    let text = "\u{feff}---\r\ntodo-spec: 1\r\n---\r\n## 分类\r\n- [ ] 父任务 <!--t:parent1-->\r\n    - [ ] 待推进 <!--t:child01--> !3\r\n      详细说明保留。\r\n    - [x] 已完成同级 <!--t:child02-->\r\n\r\n- [ ] 其他任务也保持原样 <!--t:other01-->\r\n";
    let (temp, backend) = project(text);
    let selection = ColumnSetupSelection::DefaultsAndStart {
        task_id: "child01".into(),
    };
    let preview = backend.preview_columns("TODO.md", &selection).unwrap();
    assert_eq!(bytes(temp.path()), text);
    assert!(preview
        .proposed
        .contains("## 进行中\r\n- [ ] 待推进 <!--t:child01--> !3\r\n  详细说明保留。\r\n"));
    assert!(preview
        .proposed
        .contains("    - [x] 已完成同级 <!--t:child02-->\r\n"));
    assert!(preview
        .proposed
        .contains("- [ ] 其他任务也保持原样 <!--t:other01-->\r\n"));
    assert!(!preview.proposed.replace("\r\n", "").contains('\n'));
}

#[test]
fn direct_start_reuses_an_undeclared_doing_heading_when_task_is_already_first() {
    let text =
        "---\ntodo-spec: 1\n---\n## 进行中\n- [ ] 即将归列 <!--t:start01-->\n  保留任务续行。\n";
    let (temp, backend) = project(text);
    let selection = ColumnSetupSelection::DefaultsAndStart {
        task_id: "start01".into(),
    };
    let preview = backend.preview_columns("TODO.md", &selection).unwrap();
    let configured = parse(preview.proposed.clone()).unwrap();
    assert_eq!(
        configured.task("start01").unwrap().column.as_deref(),
        Some("doing")
    );
    assert_eq!(preview.proposed.matches("## 进行中").count(), 1);
    assert!(preview
        .proposed
        .contains("## 进行中\n- [ ] 即将归列 <!--t:start01-->\n  保留任务续行。"));
    assert_eq!(bytes(temp.path()), text);
    backend
        .apply_columns("TODO.md", &preview.base_hash, &selection)
        .unwrap();
    assert_eq!(bytes(temp.path()), preview.proposed);
}

#[test]
fn direct_start_missing_or_completed_task_never_creates_columns() {
    let text = "---\ntodo-spec: 1\n---\n## 分类\n- [x] 完成任务 <!--t:closed1-->\n- [ ] 保留任务 <!--t:other01-->\n";
    let (temp, backend) = project(text);
    for id in ["missing1", "closed1", "[\"TODO.md\",\"other01\"]"] {
        assert!(backend
            .preview_columns(
                "TODO.md",
                &ColumnSetupSelection::DefaultsAndStart { task_id: id.into() }
            )
            .is_err());
        assert_eq!(bytes(temp.path()), text);
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    }
}

#[test]
fn ecommerce_default_columns_are_previewed_without_touching_existing_content() {
    let (temp, backend) = project(ECOMMERCE);
    let options = backend.column_setup_options("TODO.md").unwrap();
    assert!(!options.configured);
    assert_eq!(
        options
            .headings
            .iter()
            .map(|h| h.name.as_str())
            .collect::<Vec<_>>(),
        [
            "一、优先级",
            "二、领域状态表",
            "三、推进顺序",
            "四、分类明细"
        ]
    );
    let preview = backend
        .preview_columns("TODO.md", &ColumnSetupSelection::Defaults)
        .unwrap();
    assert_eq!(preview.base_hash, options.base_hash);
    assert_eq!(bytes(temp.path()), ECOMMERCE);
    assert_eq!(
        fs::read_dir(temp.path()).unwrap().count(),
        1,
        "preview creates no locks or history"
    );
    assert_eq!(preview.diff.removed, 0);
    assert_eq!(
        preview
            .columns
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        ["待办", "进行中", "已完成"]
    );
    assert!(preview.columns.iter().all(|c| c.created));

    let before = parse(ECOMMERCE).unwrap();
    let after = parse(preview.proposed.clone()).unwrap();
    assert_eq!(after.tasks.len(), before.tasks.len());
    assert!(after.source[after.frontmatter.range.end..]
        .starts_with(&before.source[before.frontmatter.range.end..]));
    for (old, new) in before.tasks.iter().zip(&after.tasks) {
        assert_eq!(
            &before.source[old.full_line_range.clone()],
            &after.source[new.full_line_range.clone()]
        );
        assert_eq!(old.id, new.id);
        assert_eq!(old.completed, new.completed);
        assert_eq!(old.column, new.column);
    }
    let snapshot = backend
        .apply_columns("TODO.md", &preview.base_hash, &preview.selection)
        .unwrap();
    assert_eq!(bytes(temp.path()), preview.proposed);
    assert_eq!(snapshot.columns.len(), 3);
    assert!(snapshot.tasks.iter().all(|task| task.column.is_none()));
    let history = History::for_file(temp.path().join("TODO.md"));
    let entries = history.entries().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].op, "configure-columns");
    assert_eq!(history.read(entries[0].version).unwrap(), ECOMMERCE);
    assert!(backend.column_setup_options("TODO.md").unwrap().configured);

    // Configuration enables explicit moves; it never moves on the user's behalf.
    let moved = backend
        .mutate(
            Some("TODO.md"),
            DesktopIntent::MoveTask {
                id: "87fd6689f21a3030".into(),
                column: "doing".into(),
                target: MoveTarget::ColumnStart,
                note: None,
                evidence: None,
            },
        )
        .unwrap();
    assert_eq!(
        moved
            .tasks
            .iter()
            .find(|t| t.id.as_deref() == Some("87fd6689f21a3030"))
            .unwrap()
            .column
            .as_deref(),
        Some("doing")
    );
}

#[test]
fn selecting_existing_headings_only_adds_declarations_and_preserves_checkboxes() {
    let text = "---\ntodo-spec: 1\nextra: 'keep this' # comment\n---\n## **开发**\n- [ ] work <!--t:aaaaaa-->\n## 验收\n- [ ] not completed yet <!--t:bbbbbb-->\n";
    let (temp, backend) = project(text);
    let selection = ColumnSetupSelection::Existing {
        headings: vec![
            ColumnHeadingSelection {
                name: "开发".into(),
                done: false,
            },
            ColumnHeadingSelection {
                name: "验收".into(),
                done: true,
            },
        ],
    };
    let preview = backend.preview_columns("TODO.md", &selection).unwrap();
    assert_eq!(bytes(temp.path()), text);
    assert!(preview.columns.iter().all(|c| !c.created));
    let before = parse(text).unwrap();
    let after = parse(preview.proposed).unwrap();
    assert_eq!(
        &before.source[before.frontmatter.range.end..],
        &after.source[after.frontmatter.range.end..]
    );
    assert_eq!(after.columns.len(), 2);
    assert_eq!(after.columns[1].kind.as_deref(), Some("done"));
    assert!(after.tasks.iter().all(|t| !t.completed));
    assert!(after
        .frontmatter
        .raw
        .contains("extra: 'keep this' # comment"));
}

#[test]
fn stale_preview_and_invalid_selection_cannot_overwrite_ecommerce_work() {
    let (temp, backend) = project(ECOMMERCE);
    let preview = backend
        .preview_columns("TODO.md", &ColumnSetupSelection::Defaults)
        .unwrap();
    let edited = format!("{ECOMMERCE}\n外部编辑的证据\n");
    fs::write(temp.path().join("TODO.md"), &edited).unwrap();
    assert!(matches!(
        backend.apply_columns("TODO.md", &preview.base_hash, &preview.selection),
        Err(Error::ConcurrentModification)
    ));
    assert_eq!(bytes(temp.path()), edited);
    assert!(!temp.path().join(".todo").exists());
    assert!(backend
        .preview_columns(
            "TODO.md",
            &ColumnSetupSelection::Existing { headings: vec![] }
        )
        .is_err());
    assert!(backend
        .preview_columns(
            "TODO.md",
            &ColumnSetupSelection::Existing {
                headings: vec![ColumnHeadingSelection {
                    name: "not a heading".into(),
                    done: false
                }]
            }
        )
        .is_err());
    assert_eq!(bytes(temp.path()), edited);
}

#[test]
fn bom_crlf_empty_columns_and_existing_default_heading_are_preserved() {
    let text = "\u{feff}---\r\ntodo-spec: 1\r\ncolumns: [] # keep this comment\r\ncustom: { a: 'b' }\r\n---\r\n## 待办\r\n- [ ] original <!--t:aaaaaa-->\r\n## 原文\r\nno final newline";
    let (temp, backend) = project(text);
    let preview = backend
        .preview_columns("TODO.md", &ColumnSetupSelection::Defaults)
        .unwrap();
    assert!(!preview.columns[0].created);
    assert!(preview.columns[1].created);
    assert_eq!(preview.proposed.matches("## 待办").count(), 1);
    assert!(preview.proposed.starts_with('\u{feff}'));
    assert!(preview.proposed.contains("# keep this comment\r\n"));
    assert!(preview.proposed.contains("custom: { a: 'b' }\r\n"));
    assert!(!preview.proposed.replace("\r\n", "").contains('\n'));
    assert!(preview.proposed.contains("## 原文\r\nno final newline\r\n"));
    assert_eq!(bytes(temp.path()), text);
}

#[test]
fn duplicate_headings_and_existing_configuration_are_not_guessed_or_replaced() {
    let text = "---\ntodo-spec: 1\n---\n## 待办\n## 待办\n";
    let (_temp, backend) = project(text);
    let options = backend.column_setup_options("TODO.md").unwrap();
    assert!(options
        .headings
        .iter()
        .all(|h| !h.selectable && h.reason.is_some()));
    assert!(backend
        .preview_columns("TODO.md", &ColumnSetupSelection::Defaults)
        .is_err());
    let text = "---\ntodo-spec: 1\ncolumns:\n  - {id: custom, name: Custom, extra: preserved}\n---\n## Other\n";
    let (temp, backend) = project(text);
    assert!(backend.column_setup_options("TODO.md").unwrap().configured);
    assert!(backend
        .preview_columns("TODO.md", &ColumnSetupSelection::Defaults)
        .is_err());
    assert_eq!(bytes(temp.path()), text);
}

#[test]
fn large_line_count_cannot_allocate_an_unbounded_diff_matrix() {
    let text = format!("---\ntodo-spec: 1\n---\n{}", "prose\n\n".repeat(1_250));
    let (temp, backend) = project(&text);
    let error = backend
        .preview_columns("TODO.md", &ColumnSetupSelection::Defaults)
        .unwrap_err();
    assert!(error.to_string().contains("差异预览"));
    assert_eq!(bytes(temp.path()), text);
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
}

#[test]
fn setup_will_not_write_a_source_that_exceeds_the_project_read_limit() {
    let mut text = "---\ntodo-spec: 1\n---\n".to_string();
    text.push_str(&"x".repeat(2 * 1024 * 1024 - text.len() - 16));
    let (temp, backend) = project(&text);
    assert!(backend
        .preview_columns("TODO.md", &ColumnSetupSelection::Defaults)
        .is_err());
    assert_eq!(bytes(temp.path()), text);
}

#[test]
fn no_eof_headings_are_written_inside_unclosed_fences() {
    let text = "---\ntodo-spec: 1\n---\n- [ ] keep <!--t:aaaaaa-->\n```markdown\nexample without closing fence\n";
    let (temp, backend) = project(text);
    assert!(backend
        .preview_columns("TODO.md", &ColumnSetupSelection::Defaults)
        .is_err());
    assert_eq!(bytes(temp.path()), text);
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
}

#[test]
fn heading_names_cannot_inject_yaml_and_nested_headings_are_not_selectable() {
    let text = "---\ntodo-spec: 1\n---\n## \"Work\": [not YAML] # note\n- [ ] keep <!--t:aaaaaa-->\n\n> ## Quoted heading\n\n- nested list:\n  ## Nested heading\n\nSetext heading\n--------------\n";
    let (_temp, backend) = project(text);
    let options = backend.column_setup_options("TODO.md").unwrap();
    assert!(options
        .headings
        .iter()
        .filter(|h| h.selectable)
        .all(|h| h.name == "\"Work\": [not YAML] # note"));
    let selection = ColumnSetupSelection::Existing {
        headings: vec![ColumnHeadingSelection {
            name: "\"Work\": [not YAML] # note".into(),
            done: false,
        }],
    };
    let preview = backend.preview_columns("TODO.md", &selection).unwrap();
    let parsed = parse(preview.proposed).unwrap();
    assert_eq!(parsed.columns.len(), 1);
    assert_eq!(parsed.columns[0].name, "\"Work\": [not YAML] # note");
}

#[test]
fn unsupported_yaml_is_rejected_instead_of_reserialized() {
    for text in [
        "---\n{todo-spec: 1, other: x}\n---\n## Work\n",
        "---\ntodo-spec: 1\nempty: &empty []\ncolumns: *empty\n---\n## Work\n",
    ] {
        let (temp, backend) = project(text);
        assert!(backend
            .preview_columns("TODO.md", &ColumnSetupSelection::Defaults)
            .is_err());
        assert_eq!(bytes(temp.path()), text);
        assert!(!temp.path().join(".todo").exists());
    }
}
