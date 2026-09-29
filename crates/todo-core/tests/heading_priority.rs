use todo_core::{
    effective_priorities, heading_priorities, parse, parse_priority_label, PriorityLabel,
    PriorityScale, PrioritySource,
};

fn label(text: &str) -> Option<(PriorityScale, u8)> {
    parse_priority_label(text).map(|PriorityLabel { scale, rank, .. }| (scale, rank))
}

#[test]
fn recognizes_priority_labels_at_the_head_of_a_heading() {
    use PriorityScale::*;
    for (text, expected) in [
        ("P0", (PNumber, 0)),
        ("p1", (PNumber, 1)),
        ("P1 · 线 B：领域事件", (PNumber, 1)),
        ("P2 · DuckDB 试点 D0–D3", (PNumber, 2)),
        ("P0：正确性与安全阻断", (PNumber, 0)),
        ("P0级", (PNumber, 0)),
        ("[P0] 库存", (PNumber, 0)),
        ("【P1】订单", (PNumber, 1)),
        ("（P2）搜索", (PNumber, 2)),
        ("Ｐ０", (PNumber, 0)),
        ("🔴 P0", (PNumber, 0)),
        ("1. P0", (PNumber, 0)),
        ("三、P2", (PNumber, 2)),
        ("Priority 1", (PNumber, 1)),
        ("优先级：P0", (PNumber, 0)),
        ("优先级 2", (PNumber, 2)),
        ("!1", (Native, 1)),
        ("High", (Word, 1)),
        ("High Priority", (Word, 1)),
        ("Priority: Low", (Word, 3)),
        ("Critical: 数据丢失", (Word, 0)),
        ("Medium (next sprint)", (Word, 2)),
        ("紧急", (Word, 0)),
        ("高优先级", (Word, 1)),
        ("中优先级", (Word, 2)),
        ("低优", (Word, 3)),
        ("优先级：高", (Word, 1)),
    ] {
        assert_eq!(label(text), Some(expected), "{text:?}");
    }
}

#[test]
fn recognizes_a_trailing_label() {
    use PriorityScale::*;
    assert_eq!(label("库存正确性 (P0)"), Some((PNumber, 0)));
    assert_eq!(label("鉴权 — P1"), Some((PNumber, 1)));
    assert_eq!(label("鉴权 P1"), Some((PNumber, 1)));
    assert_eq!(label("鉴权：高优先级"), Some((Word, 1)));
}

#[test]
fn does_not_mistake_ordinary_headings_for_priorities() {
    for text in [
        "一、优先级",
        "优先级",
        "Postgres 迁移",
        "P2P 网络",
        "HTTP2 升级",
        "High availability",
        "Low-latency search",
        "高级功能",
        "高",
        "中文文档",
        "Normal form",
        "微服务与交易闭环",
        "Phase 1",
        "Priority queue 实现",
        "!5",
        "P",
        "",
    ] {
        assert_eq!(label(text), None, "{text:?}");
    }
}

const ECOMMERCE: &str = "---
todo-spec: 1
---
# 项目待办

## 一、优先级

- **P0：正确性与安全阻断**。先修库存。
- **P1：生产闭环**。
- **P2：增强与条件项**。

## 四、分类明细

### 微服务与交易闭环

#### P0

- [ ] 库存预占 <!--t:aaaaaa-->
    - [ ] 子任务 <!--t:aaaaab-->
- [ ] 已手工定级 <!--t:aaaaac--> !3

#### P1

- [ ] 建单联动 <!--t:bbbbbb-->

### 数据一致性与事件驱动

- [ ] 领域说明之下、优先级标题之前 <!--t:cccccc-->

#### P1 · 线 B：领域事件

- [ ] 事务生产者 <!--t:dddddd-->

#### P2 · DuckDB 试点

- [x] 已完成仍保留来源 <!--t:eeeeee-->

```md
#### P0
- [ ] fenced <!--t:ffffff-->
```

## 五、不再按原目标推进

- [ ] 取消项 <!--t:gggggg-->
";

fn by_id<T: Clone>(document: &todo_core::Document, values: &[T], id: &str) -> T {
    let index = document
        .tasks
        .iter()
        .position(|task| task.id.as_deref() == Some(id))
        .unwrap();
    values[index].clone()
}

#[test]
fn tasks_inherit_the_nearest_labelled_heading_within_its_section() {
    let document = parse(ECOMMERCE).unwrap();
    let inferred = heading_priorities(&document);
    let level = |id| by_id(&document, &inferred, id).map(|found| found.level);

    assert_eq!(
        level("aaaaaa"),
        Some(1),
        "P0 is the top of a zero-based scale"
    );
    assert_eq!(level("aaaaab"), Some(1), "nested tasks share their heading");
    assert_eq!(level("bbbbbb"), Some(2));
    assert_eq!(
        level("cccccc"),
        None,
        "a new H3 closes the previous P1 section"
    );
    assert_eq!(level("dddddd"), Some(2));
    assert_eq!(level("eeeeee"), Some(3));
    assert_eq!(
        level("gggggg"),
        None,
        "a later H2 closes every nested label"
    );
    assert!(
        document
            .tasks
            .iter()
            .all(|task| task.id.as_deref() != Some("ffffff")),
        "fenced examples are neither tasks nor headings"
    );

    let found = by_id(&document, &inferred, "dddddd").unwrap();
    assert_eq!(found.label, "P1");
    assert_eq!(found.heading, "P1 · 线 B：领域事件");
    assert_eq!(found.heading_line, 30);
    assert!(!found.clamped);
}

#[test]
fn explicit_task_priority_overrides_the_heading() {
    let document = parse(ECOMMERCE).unwrap();
    let effective = effective_priorities(&document);

    let manual = by_id(&document, &effective, "aaaaac").unwrap();
    assert_eq!(manual.level, 3);
    assert_eq!(manual.source, PrioritySource::Explicit);

    let inherited = by_id(&document, &effective, "aaaaaa").unwrap();
    assert_eq!(inherited.level, 1);
    assert!(matches!(inherited.source, PrioritySource::Heading(ref h) if h.label == "P0"));

    assert!(by_id(&document, &effective, "gggggg").is_none());
}

#[test]
fn one_based_scales_are_detected_when_p0_never_appears() {
    let source = "---\ntodo-spec: 1\n---\n## P1\n- [ ] a <!--t:aaaaaa-->\n## P2\n- [ ] b <!--t:bbbbbb-->\n## P5\n- [ ] c <!--t:cccccc-->\n";
    let document = parse(source).unwrap();
    let inferred = heading_priorities(&document);
    let level = |id| by_id(&document, &inferred, id).map(|found| (found.level, found.clamped));
    assert_eq!(
        level("aaaaaa"),
        Some((1, false)),
        "P1 is the top when P0 is absent"
    );
    assert_eq!(level("bbbbbb"), Some((2, false)));
    assert_eq!(
        level("cccccc"),
        Some((4, true)),
        "deeper tiers collapse into !4"
    );
}

#[test]
fn a_p0_mention_in_prose_makes_the_scale_zero_based() {
    let source = "---\ntodo-spec: 1\n---\nP0 已全部清空。\n\n## P1\n- [ ] a <!--t:aaaaaa-->\n";
    let document = parse(source).unwrap();
    let inferred = heading_priorities(&document);
    assert_eq!(by_id(&document, &inferred, "aaaaaa").unwrap().level, 2);

    // Inline code is not prose: `P0` inside backticks must not shift the scale.
    let source = "---\ntodo-spec: 1\n---\n见 `P0` 字段。\n\n## P1\n- [ ] a <!--t:aaaaaa-->\n";
    let document = parse(source).unwrap();
    let inferred = heading_priorities(&document);
    assert_eq!(by_id(&document, &inferred, "aaaaaa").unwrap().level, 1);
}

#[test]
fn word_labels_map_onto_the_native_four_tiers() {
    let source = "---\ntodo-spec: 1\n---\n## High\n- [ ] a <!--t:aaaaaa-->\n## 中优先级\n- [ ] b <!--t:bbbbbb-->\n## Low\n- [ ] c <!--t:cccccc-->\n## 紧急\n- [ ] d <!--t:dddddd-->\n";
    let document = parse(source).unwrap();
    let inferred = heading_priorities(&document);
    let level = |id| by_id(&document, &inferred, id).unwrap().level;
    assert_eq!(
        [
            level("aaaaaa"),
            level("bbbbbb"),
            level("cccccc"),
            level("dddddd")
        ],
        [2, 3, 4, 1]
    );
}

#[test]
fn an_unlabelled_subheading_keeps_the_labelled_parent() {
    let source = "---\ntodo-spec: 1\n---\n## P0\n### 库存\n- [ ] a <!--t:aaaaaa-->\n### P2 延后\n- [ ] b <!--t:bbbbbb-->\n### 订单\n- [ ] c <!--t:cccccc-->\n";
    let document = parse(source).unwrap();
    let inferred = heading_priorities(&document);
    let level = |id| by_id(&document, &inferred, id).unwrap().level;
    assert_eq!(
        [level("aaaaaa"), level("bbbbbb"), level("cccccc")],
        [1, 3, 1]
    );
}

#[test]
fn frontmatter_is_never_read_as_a_setext_heading() {
    // `---` after `todo-spec: P1`-like YAML would form a setext H2 if the
    // frontmatter were fed to the Markdown parser.
    let source = "---\ntodo-spec: 1\nnote: P0\n---\n- [ ] a <!--t:aaaaaa-->\n";
    let document = parse(source).unwrap();
    assert_eq!(heading_priorities(&document), vec![None]);
}
