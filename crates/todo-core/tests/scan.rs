//! Repository scan: what it must find, and more importantly what it must not.
//!
//! The failure mode that kills comment importers is a noisy first run. Most of
//! these cases are therefore about suppression: fenced examples, prose that
//! merely mentions "todo", words like TODOS, ignored trees, and work that is
//! already tracked.

use std::fs;

use tempfile::tempdir;
use todo_core::{parse, scan, ScanOptions};

const MANAGED: &str = "---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n---\n\n## Todo\n\n- [ ] 已经跟踪的活 <!--t:aaaaaa-->\n";

fn write(dir: &std::path::Path, name: &str, content: &str) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

fn run(dir: &std::path::Path) -> todo_core::ScanReport {
    let document = parse(MANAGED).unwrap();
    let options = ScanOptions::all(dir.join("TODO.md"));
    scan(dir, &document, &options).unwrap()
}

#[test]
fn finds_loose_checkboxes_in_other_markdown_files() {
    let dir = tempdir().unwrap();
    write(dir.path(), "TODO.md", MANAGED);
    write(
        dir.path(),
        "docs/plan.md",
        "# Plan\n\n- [ ] 写迁移脚本\n- [x] 建表\n",
    );

    let report = run(dir.path());

    assert_eq!(report.loose_checkboxes.len(), 2);
    assert_eq!(report.loose_checkboxes[0].path, "docs/plan.md");
    assert_eq!(report.loose_checkboxes[0].line, 3);
    assert_eq!(report.loose_checkboxes[0].text, "写迁移脚本");
    assert!(!report.loose_checkboxes[0].completed);
    assert!(report.loose_checkboxes[1].completed);
}

#[test]
fn the_managed_task_file_is_never_reported_as_loose() {
    let dir = tempdir().unwrap();
    write(dir.path(), "TODO.md", MANAGED);

    let report = run(dir.path());

    assert!(report.loose_checkboxes.is_empty());
}

#[test]
fn checkboxes_inside_fenced_code_are_examples_not_tasks() {
    let dir = tempdir().unwrap();
    write(dir.path(), "TODO.md", MANAGED);
    write(
        dir.path(),
        "README.md",
        "# Doc\n\n```markdown\n- [ ] 这是文档里的示例\n```\n\n- [ ] 这是真任务\n",
    );

    let report = run(dir.path());

    assert_eq!(report.loose_checkboxes.len(), 1);
    assert_eq!(report.loose_checkboxes[0].text, "这是真任务");
}

#[test]
fn checkboxes_that_already_carry_a_stable_id_belong_to_another_managed_file() {
    let dir = tempdir().unwrap();
    write(dir.path(), "TODO.md", MANAGED);
    write(
        dir.path(),
        "todos/auth.md",
        "- [ ] 有主的任务 <!--t:bbbbbb-->\n",
    );

    let report = run(dir.path());

    assert!(report.loose_checkboxes.is_empty());
}

#[test]
fn finds_markers_in_source_comments_with_their_location() {
    let dir = tempdir().unwrap();
    write(dir.path(), "TODO.md", MANAGED);
    write(
        dir.path(),
        "src/main.rs",
        "fn main() {\n    // TODO: 加上重试\n    let x = 1;\n    /* FIXME: 这里会溢出 */\n}\n",
    );

    let report = run(dir.path());

    assert_eq!(report.code_comments.len(), 2);
    assert_eq!(report.code_comments[0].marker, "TODO");
    assert_eq!(report.code_comments[0].text, "加上重试");
    assert_eq!(report.code_comments[0].line, 2);
    assert_eq!(report.code_comments[0].path, "src/main.rs");
    assert_eq!(report.code_comments[1].marker, "FIXME");
    assert_eq!(report.code_comments[1].text, "这里会溢出");
}

#[test]
fn a_marker_must_sit_in_a_comment_not_in_a_string_or_identifier() {
    // Matching the bare word would flag every mention of "todo" in ordinary code,
    // and a scan whose first run is mostly false positives does not get a second.
    let dir = tempdir().unwrap();
    write(dir.path(), "TODO.md", MANAGED);
    write(
        dir.path(),
        "src/app.ts",
        "const label = \"TODO: not a task\";\nconst TODOS = [];\nfunction todoList() {}\n// TODO: 真的待办\n",
    );

    let report = run(dir.path());

    assert_eq!(
        report.code_comments.len(),
        1,
        "got {:?}",
        report.code_comments
    );
    assert_eq!(report.code_comments[0].text, "真的待办");
    assert_eq!(report.code_comments[0].line, 4);
}

#[test]
fn work_already_tracked_is_filtered_instead_of_proposed_again() {
    // Re-running a scan must converge. Proposing the same item every time is what
    // makes an importer feel like noise.
    let dir = tempdir().unwrap();
    write(dir.path(), "TODO.md", MANAGED);
    write(dir.path(), "notes.md", "- [ ] 已经跟踪的活\n");
    write(dir.path(), "src/lib.rs", "// TODO: 已经跟踪的活!\n");

    let report = run(dir.path());

    assert!(report.is_empty(), "got {report:?}");
    assert_eq!(report.stats.duplicates_filtered, 2);
}

#[test]
fn gitignored_trees_are_not_scanned() {
    let dir = tempdir().unwrap();
    write(dir.path(), "TODO.md", MANAGED);
    write(dir.path(), ".gitignore", "vendor/\nbuild/\n");
    write(dir.path(), "vendor/dep.rs", "// TODO: 第三方代码里的待办\n");
    write(dir.path(), "build/out.md", "- [ ] 构建产物\n");
    write(dir.path(), "src/real.rs", "// TODO: 我们自己的待办\n");

    let report = run(dir.path());

    assert_eq!(report.code_comments.len(), 1);
    assert_eq!(report.code_comments[0].text, "我们自己的待办");
    assert!(report.loose_checkboxes.is_empty());
}

#[test]
fn scanning_can_be_restricted_to_one_layer() {
    let dir = tempdir().unwrap();
    write(dir.path(), "TODO.md", MANAGED);
    write(dir.path(), "plan.md", "- [ ] markdown 项\n");
    write(dir.path(), "src/x.rs", "// TODO: 注释项\n");
    let document = parse(MANAGED).unwrap();

    let only_markdown = scan(
        dir.path(),
        &document,
        &ScanOptions {
            markdown: true,
            comments: false,
            task_file: dir.path().join("TODO.md"),
        },
    )
    .unwrap();
    assert_eq!(only_markdown.loose_checkboxes.len(), 1);
    assert!(only_markdown.code_comments.is_empty());

    let only_comments = scan(
        dir.path(),
        &document,
        &ScanOptions {
            markdown: false,
            comments: true,
            task_file: dir.path().join("TODO.md"),
        },
    )
    .unwrap();
    assert!(only_comments.loose_checkboxes.is_empty());
    assert_eq!(only_comments.code_comments.len(), 1);
}

#[test]
fn results_are_ordered_by_path_and_line_so_review_is_stable() {
    let dir = tempdir().unwrap();
    write(dir.path(), "TODO.md", MANAGED);
    write(dir.path(), "src/b.rs", "// TODO: 第二\n");
    write(dir.path(), "src/a.rs", "// TODO: 甲\n// FIXME: 乙\n");

    let report = run(dir.path());

    let keys: Vec<_> = report
        .code_comments
        .iter()
        .map(|c| (c.path.as_str(), c.line))
        .collect();
    assert_eq!(
        keys,
        vec![("src/a.rs", 1), ("src/a.rs", 2), ("src/b.rs", 1)]
    );
}

#[test]
fn markers_are_recognised_across_comment_syntaxes() {
    let dir = tempdir().unwrap();
    write(dir.path(), "TODO.md", MANAGED);
    write(dir.path(), "s.py", "# TODO: python 风格\n");
    write(dir.path(), "s.sql", "-- FIXME: sql 风格\n");
    write(dir.path(), "s.rb", "# HACK: ruby 风格\n");

    let report = run(dir.path());

    let mut found: Vec<_> = report
        .code_comments
        .iter()
        .map(|c| c.marker.as_str())
        .collect();
    found.sort_unstable();
    assert_eq!(found, vec!["FIXME", "HACK", "TODO"]);
}

#[test]
fn a_comment_opener_inside_a_string_literal_is_not_a_comment() {
    // Regression: scanning this repository reported 13 candidates, every one of them
    // a marker inside a test fixture string such as "// TODO: ...". String state has
    // to be tracked before an opener counts.
    let dir = tempdir().unwrap();
    write(dir.path(), "TODO.md", MANAGED);
    write(
        dir.path(),
        "src/fixture.rs",
        concat!(
            "let sample = \"// TODO: 这是测试夹具里的字符串\";\n",
            "let html = \"<!-- todo:begin -->\";\n",
            "let tpl = `/* FIXME: 模板字符串 */`;\n",
            "// TODO: 只有这一条是真的\n",
        ),
    );

    let report = run(dir.path());

    assert_eq!(
        report.code_comments.len(),
        1,
        "string literals must not produce markers: {:?}",
        report.code_comments
    );
    assert_eq!(report.code_comments[0].text, "只有这一条是真的");
    assert_eq!(report.code_comments[0].line, 4);
}

#[test]
fn non_ascii_source_lines_do_not_panic() {
    // Regression: the opener search indexed by byte, so any line with Chinese text
    // could slice inside a character and abort the whole scan.
    let dir = tempdir().unwrap();
    write(dir.path(), "TODO.md", MANAGED);
    write(
        dir.path(),
        "src/zh.rs",
        "let 变量 = \"中文字符串里的内容\";\nlet another = 中文标识符;\n// TODO: 中文待办\n",
    );

    let report = run(dir.path());

    assert_eq!(report.code_comments.len(), 1);
    assert_eq!(report.code_comments[0].text, "中文待办");
}

#[test]
fn comment_syntax_is_chosen_per_language() {
    // `#` opens a comment in Python but is an attribute in Rust; `--` is a comment in
    // SQL but a decrement in TypeScript. A shared union of openers would flag both.
    let dir = tempdir().unwrap();
    write(dir.path(), "TODO.md", MANAGED);
    write(
        dir.path(),
        "attr.rs",
        "#[derive(Debug)] // TODO: 真待办\nstruct S;\n",
    );
    write(
        dir.path(),
        "count.ts",
        "let n = 0;\nn--; // FIXME: 真待办二\n",
    );

    let report = run(dir.path());

    let texts: Vec<_> = report
        .code_comments
        .iter()
        .map(|c| c.text.as_str())
        .collect();
    assert_eq!(texts.len(), 2, "got {texts:?}");
    assert!(texts.contains(&"真待办"));
    assert!(texts.contains(&"真待办二"));
}
