//! Format v2: partial completion (`◐`), failed attempts (`⚠`), and note lines.
//!
//! These cover the three properties the format has to hold: the new tokens parse,
//! the note line binds to exactly one task, and every write keeps untouched bytes
//! byte-identical.

use todo_core::{apply_changes, parse, Change, TokenKind};

const DOC: &str = "---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n---\n\n## Todo\n\n- [ ] 给登录接口加限流 <!--t:p5pxad--> ◐60 ⚠2026-09-01 !1\n- [ ] 写文档 <!--t:aa11bb--> #docs\n";

fn doc_with(body: &str) -> String {
    format!("---\ntodo-spec: 1\ncolumns:\n  - {{id: todo, name: Todo}}\n---\n\n## Todo\n\n{body}")
}

#[test]
fn progress_and_failure_tokens_parse() {
    let document = parse(DOC).unwrap();
    let task = document.task("p5pxad").unwrap();

    assert_eq!(task.progress(), Some(60));
    assert_eq!(task.failure(), Some("2026-09-01"));
    assert!(task.partial());
    assert!(!task.completed);
    assert_eq!(task.body, "给登录接口加限流");

    let plain = document.task("aa11bb").unwrap();
    assert_eq!(plain.progress(), None);
    assert!(!plain.partial());
}

#[test]
fn a_failure_alone_still_counts_as_work_in_flight() {
    let document = parse(doc_with("- [ ] 压测 <!--t:cc22dd--> ⚠2026-09-01\n")).unwrap();
    let task = document.task("cc22dd").unwrap();

    assert_eq!(task.progress(), None);
    assert_eq!(task.failure(), Some("2026-09-01"));
    assert!(task.partial());
}

#[test]
fn progress_outside_one_to_ninety_nine_is_not_a_progress_token() {
    // 0 cannot be told apart from "not started" and 100 would be a second way to
    // say done, which the checkbox alone is allowed to express.
    for raw in ["◐0", "◐100", "◐101", "◐abc"] {
        let document = parse(doc_with(&format!("- [ ] 任务 <!--t:cc22dd--> {raw}\n"))).unwrap();
        let task = document.task("cc22dd").unwrap();
        assert_eq!(task.progress(), None, "{raw} must not parse as progress");
        assert!(
            task.metadata
                .iter()
                .chain(task.unknown_tokens.iter())
                .any(|token| token.raw == raw),
            "{raw} must still be preserved verbatim"
        );
    }
}

#[test]
fn completed_task_is_never_partial() {
    let document = parse(doc_with("- [x] 任务 <!--t:cc22dd--> ◐60\n")).unwrap();
    let task = document.task("cc22dd").unwrap();

    assert!(task.completed);
    assert!(!task.partial(), "a checked task is done regardless of ◐");
}

#[test]
fn note_line_binds_to_its_anchored_task() {
    let source = doc_with(
        "- [ ] 加限流 <!--t:p5pxad--> ◐60\n      ↳ 令牌桶已实现；压测未过 <!--n:p5pxad-->\n- [ ] 写文档 <!--t:aa11bb-->\n",
    );
    let document = parse(&source).unwrap();

    let note = document.task("p5pxad").unwrap().note.as_ref().unwrap();
    assert_eq!(note.text, "令牌桶已实现；压测未过");
    assert_eq!(note.indent, "      ");
    assert!(document.task("aa11bb").unwrap().note.is_none());
}

#[test]
fn note_with_a_foreign_anchor_is_not_adopted() {
    // Without this the note would silently re-home itself onto whichever task it
    // drifted next to, and the wrong task would start reporting someone else's state.
    let source = doc_with("- [ ] 加限流 <!--t:p5pxad-->\n      ↳ 属于别的任务 <!--n:aa11bb-->\n");
    let document = parse(&source).unwrap();

    assert!(document.task("p5pxad").unwrap().note.is_none());
}

#[test]
fn setting_a_note_leaves_every_other_byte_untouched() {
    let document = parse(DOC).unwrap();
    let index = document
        .tasks
        .iter()
        .position(|task| task.id.as_deref() == Some("p5pxad"))
        .unwrap();

    let updated = apply_changes(
        &document,
        &[Change::SetNote {
            task: index,
            text: "令牌桶已实现；压测 500QPS 未过".into(),
        }],
    )
    .unwrap();

    assert!(updated.contains("      ↳ 令牌桶已实现；压测 500QPS 未过 <!--n:p5pxad-->\n"));
    // The note is inserted as a whole new line, so removing that line has to restore
    // the original file exactly.
    let without_note: String = updated
        .lines()
        .filter(|line| !line.contains("<!--n:p5pxad-->"))
        .map(|line| format!("{line}\n"))
        .collect();
    assert_eq!(without_note, DOC);

    let reparsed = parse(&updated).unwrap();
    assert_eq!(
        reparsed.task("p5pxad").unwrap().note.as_ref().unwrap().text,
        "令牌桶已实现；压测 500QPS 未过"
    );
}

#[test]
fn setting_a_note_twice_replaces_rather_than_stacks() {
    let document = parse(DOC).unwrap();
    let index = 0;
    let once = apply_changes(
        &document,
        &[Change::SetNote {
            task: index,
            text: "第一版".into(),
        }],
    )
    .unwrap();

    let document = parse(&once).unwrap();
    let twice = apply_changes(
        &document,
        &[Change::SetNote {
            task: index,
            text: "第二版".into(),
        }],
    )
    .unwrap();

    assert_eq!(twice.matches("<!--n:p5pxad-->").count(), 1);
    assert!(twice.contains("第二版"));
    assert!(!twice.contains("第一版"));
}

#[test]
fn removing_a_note_restores_the_original_document() {
    let document = parse(DOC).unwrap();
    let with_note = apply_changes(
        &document,
        &[Change::SetNote {
            task: 0,
            text: "临时说明".into(),
        }],
    )
    .unwrap();

    let document = parse(&with_note).unwrap();
    let removed = apply_changes(&document, &[Change::RemoveNote { task: 0 }]).unwrap();

    assert_eq!(removed, DOC);
}

#[test]
fn deleting_a_task_takes_its_note_with_it() {
    // An orphaned note line would keep an anchor pointing at an id that no longer
    // exists, which every later scan would have to re-diagnose.
    let document = parse(DOC).unwrap();
    let with_note = apply_changes(
        &document,
        &[Change::SetNote {
            task: 0,
            text: "说明".into(),
        }],
    )
    .unwrap();

    let document = parse(&with_note).unwrap();
    let deleted = apply_changes(&document, &[Change::DeleteTask { task: 0 }]).unwrap();

    assert!(!deleted.contains("<!--n:p5pxad-->"));
    assert!(!deleted.contains("加限流"));
    assert!(deleted.contains("写文档"));
}

#[test]
fn note_text_may_not_smuggle_comment_syntax() {
    let document = parse(DOC).unwrap();
    let result = apply_changes(
        &document,
        &[Change::SetNote {
            task: 0,
            text: "伪造 <!--n:aa11bb--> 锚点".into(),
        }],
    );

    assert!(
        result.is_err(),
        "comment markers in note prose must be rejected"
    );
}

#[test]
fn progress_token_can_be_set_and_cleared_through_metadata_changes() {
    let document = parse(doc_with("- [ ] 任务 <!--t:cc22dd--> !1\n")).unwrap();

    let updated = apply_changes(
        &document,
        &[Change::SetMetadata {
            task: 0,
            kind: TokenKind::Progress(40),
            raw: "◐40".into(),
        }],
    )
    .unwrap();
    assert!(updated.contains("- [ ] 任务 <!--t:cc22dd--> !1 ◐40"));

    let document = parse(&updated).unwrap();
    assert_eq!(document.task("cc22dd").unwrap().progress(), Some(40));

    // Replacing an existing progress token must not append a second one.
    let bumped = apply_changes(
        &document,
        &[Change::SetMetadata {
            task: 0,
            kind: TokenKind::Progress(90),
            raw: "◐90".into(),
        }],
    )
    .unwrap();
    assert_eq!(bumped.matches('◐').count(), 1);
    assert_eq!(
        parse(&bumped).unwrap().task("cc22dd").unwrap().progress(),
        Some(90)
    );
}

#[test]
fn crlf_documents_keep_their_line_endings_when_a_note_is_added() {
    let source = DOC.replace('\n', "\r\n");
    let document = parse(&source).unwrap();
    let updated = apply_changes(
        &document,
        &[Change::SetNote {
            task: 0,
            text: "说明".into(),
        }],
    )
    .unwrap();

    assert!(updated.contains("<!--n:p5pxad-->\r\n"));
    assert!(!updated.contains("<!--n:p5pxad-->\n\r"));
    assert_eq!(
        parse(&updated)
            .unwrap()
            .task("p5pxad")
            .unwrap()
            .note
            .as_ref()
            .unwrap()
            .text,
        "说明"
    );
}
