//! Explicit, additive column setup, optionally moving one selected Markdown item.
//! Configuration alone never moves tasks; starting work preserves the item's full text.
use std::collections::{HashMap, HashSet};

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};

use crate::{diff_lines, parse, Document, Error, Result, TextDiff};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum ColumnSetupSelection {
    Defaults,
    DefaultsAndStart {
        task_id: String,
    },
    Existing {
        headings: Vec<ColumnHeadingSelection>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ColumnHeadingSelection {
    pub name: String,
    pub done: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnHeadingOption {
    pub name: String,
    pub line: usize,
    pub selectable: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnSetupOptions {
    pub source_path: String,
    pub base_hash: String,
    pub headings: Vec<ColumnHeadingOption>,
    pub configured: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedColumn {
    pub id: String,
    pub name: String,
    pub done: bool,
    pub created: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ColumnSetupPreview {
    pub source_path: String,
    pub base_hash: String,
    pub selection: ColumnSetupSelection,
    pub columns: Vec<PlannedColumn>,
    pub proposed: String,
    pub diff: TextDiff,
}

pub(crate) fn revision(document: &Document) -> String {
    format!(
        "{:016x}",
        crate::parse::hash_bytes(document.source.as_bytes())
    )
}

pub(crate) fn options(source_path: &str, document: &Document) -> ColumnSetupOptions {
    ColumnSetupOptions {
        source_path: source_path.into(),
        base_hash: revision(document),
        headings: heading_options(document),
        configured: !document.frontmatter.columns.is_empty(),
    }
}

pub(crate) fn preview(
    source_path: &str,
    document: &Document,
    selection: &ColumnSetupSelection,
) -> Result<ColumnSetupPreview> {
    if !document.frontmatter.columns.is_empty() {
        return Err(invalid(
            "当前来源已有列定义；此入口只初始化任务列，不覆盖已有配置。",
        ));
    }
    let start_id = match selection {
        ColumnSetupSelection::DefaultsAndStart { task_id } => {
            validate_start_task(document, task_id)?;
            Some(task_id.as_str())
        }
        _ => None,
    };
    let headings = heading_options(document);
    let columns = match selection {
        ColumnSetupSelection::Defaults | ColumnSetupSelection::DefaultsAndStart { .. } => [
            ("todo", "待办", false),
            ("doing", "进行中", false),
            ("done", "已完成", true),
        ]
        .into_iter()
        .map(|(id, name, done)| {
            let found = headings.iter().find(|heading| heading.name == name);
            if found.is_some_and(|heading| !heading.selectable) {
                return Err(invalid(
                    "默认列名称与歧义标题冲突，请先选择唯一的现有二级标题。",
                ));
            }
            Ok(PlannedColumn {
                id: id.into(),
                name: name.into(),
                done,
                created: found.is_none(),
            })
        })
        .collect::<Result<Vec<_>>>()?,
        ColumnSetupSelection::Existing { headings: selected } => {
            if selected.is_empty() || selected.len() > 64 {
                return Err(invalid("请选择 1 至 64 个现有二级标题。"));
            }
            let mut seen = HashSet::new();
            selected
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    if !seen.insert(&item.name) {
                        return Err(invalid("不能重复选择同一个标题。"));
                    }
                    if !headings.iter().any(|h| h.name == item.name && h.selectable) {
                        return Err(invalid("所选标题不存在或无法唯一定位，请重新读取来源。"));
                    }
                    Ok(PlannedColumn {
                        id: format!("column{}", index + 1),
                        name: item.name.clone(),
                        done: item.done,
                        created: false,
                    })
                })
                .collect::<Result<Vec<_>>>()?
        }
    };
    let newline = document.line_ending.as_str();
    let entries: String = columns
        .iter()
        .map(|column| {
            // JSON string literals are YAML-compatible, so names cannot inject YAML.
            let name =
                serde_json::to_string(&column.name).expect("string serialization cannot fail");
            format!(
                "  - {{id: {}, name: {}{}}}{newline}",
                column.id,
                name,
                if column.done { ", type: done" } else { "" }
            )
        })
        .collect();
    let mut proposed = insert_declarations(document, &entries)?;
    let created: Vec<_> = columns.iter().filter(|column| column.created).collect();
    if !created.is_empty() {
        if !proposed.ends_with('\n') {
            proposed.push_str(newline);
        }
        proposed.push_str(newline);
        for column in created {
            proposed.push_str(&format!("## {}{newline}{newline}", column.name));
        }
    }
    let parsed = parse(proposed.clone())?;
    // An unterminated fenced/HTML block at EOF must not swallow the new headings.
    if parsed.columns.len() != columns.len()
        || columns.iter().any(|column| {
            !parsed.columns.iter().any(|c| {
                c.id == column.id
                    && c.name == column.name
                    && (c.kind.as_deref() == Some("done")) == column.done
            })
        })
    {
        return Err(invalid(
            "新列标题无法被识别；请检查未闭合代码块或重复标题。",
        ));
    }
    let mut old_metadata = document.frontmatter.value.clone();
    let mut new_metadata = parsed.frontmatter.value.clone();
    let key = serde_yaml::Value::String("columns".into());
    if let Some(map) = old_metadata.as_mapping_mut() {
        map.remove(&key);
    }
    if let Some(map) = new_metadata.as_mapping_mut() {
        map.remove(&key);
    }
    if old_metadata != new_metadata || parsed.tasks.len() != document.tasks.len() {
        return Err(invalid("无法保真添加列配置，未写入文件。"));
    }
    if let Some(task_id) = start_id {
        proposed = move_started_block(&parsed, task_id)?;
    }
    // The shared line diff uses an LCS matrix. Bound its work before allocation;
    // a byte cap alone still permits millions of short lines in user input.
    let cells = (document.source.lines().count() + 1).saturating_mul(proposed.lines().count() + 1);
    if cells > 4_000_000 {
        return Err(invalid(
            "当前文件超过安全差异预览预算，请先缩小来源文件后重试。",
        ));
    }
    Ok(ColumnSetupPreview {
        source_path: source_path.into(),
        base_hash: revision(document),
        selection: selection.clone(),
        columns,
        diff: diff_lines(&document.source, &proposed),
        proposed,
    })
}

fn invalid(message: &str) -> Error {
    Error::InvalidTaskLine(message.into())
}

fn validate_start_task(document: &Document, id: &str) -> Result<()> {
    if !crate::parse::valid_id(id) {
        return Err(Error::InvalidTaskId(id.into()));
    }
    if document
        .duplicate_ids()
        .iter()
        .any(|duplicate| duplicate == id)
    {
        return Err(Error::DuplicateTaskId(id.into()));
    }
    let task = document
        .task(id)
        .ok_or_else(|| Error::TaskNotFound(id.into()))?;
    if task.completed || task.column.is_some() {
        return Err(invalid(
            "只能通过此入口推进未归列且未完成的任务，请重新读取当前状态。",
        ));
    }
    Ok(())
}

/// Move the actual Markdown list item, not a reconstruction of known task lines.
/// Ecommerce task descriptions can continue as plain paragraphs without ↳ anchors;
/// copying their parser-owned item range keeps those bytes and child items together.
fn move_started_block(document: &Document, id: &str) -> Result<String> {
    let task = document
        .task(id)
        .ok_or_else(|| Error::TaskNotFound(id.into()))?;
    let target = document
        .columns
        .iter()
        .find(|column| column.id == "doing")
        .ok_or_else(|| Error::ColumnNotFound("doing".into()))?;
    let mut items = Vec::new();
    let mut item_start = None;
    for (event, range) in
        Parser::new_ext(&document.source, Options::ENABLE_TASKLISTS).into_offset_iter()
    {
        match event {
            Event::Start(Tag::Item) => items.push(range.start),
            Event::TaskListMarker(_) if range.start == task.checkbox_range.start => {
                item_start = items.last().copied();
            }
            Event::End(TagEnd::Item) => {
                let start = items.pop();
                if item_start.is_some() && start == item_start {
                    let block_start = task.full_line_range.start;
                    let mut block_end = range.end;
                    if block_end < document.source.len()
                        && !document.source[..block_end].ends_with('\n')
                    {
                        block_end = document.source[block_end..]
                            .find('\n')
                            .map_or(document.source.len(), |end| block_end + end + 1);
                    }
                    let insertion = target.content_range.start;
                    if block_start < insertion && insertion < block_end {
                        return Err(invalid("目标列位于任务自身内容中，无法安全移动。"));
                    }
                    let mut block = String::new();
                    for line in document.source[block_start..block_end].split_inclusive('\n') {
                        if line.trim().is_empty() {
                            block.push_str(line);
                        } else {
                            block.push_str(
                                line.strip_prefix(&task.indent)
                                    .ok_or_else(|| invalid("任务续行缩进无法保真移动。"))?,
                            );
                        }
                    }
                    if !block.ends_with('\n') {
                        block.push_str(document.line_ending.as_str());
                    }
                    let mut proposed = document.source.clone();
                    proposed.replace_range(block_start..block_end, "");
                    let adjusted = if insertion >= block_end {
                        insertion - (block_end - block_start)
                    } else {
                        insertion
                    };
                    proposed.insert_str(adjusted, &block);
                    let moved = parse(proposed.clone())?;
                    if moved.task(id).and_then(|t| t.column.as_deref()) != Some("doing")
                        || moved.tasks.len() != document.tasks.len()
                    {
                        return Err(invalid("任务移动后结构无法校验，未写入文件。"));
                    }
                    for old in &document.tasks {
                        let Some(old_id) = old.id.as_deref() else {
                            return Err(Error::MissingTaskId);
                        };
                        let new = moved.task(old_id).ok_or(Error::MissingTaskId)?;
                        if old.completed != new.completed
                            || old.body != new.body
                            || old.metadata.iter().map(|t| &t.raw).collect::<Vec<_>>()
                                != new.metadata.iter().map(|t| &t.raw).collect::<Vec<_>>()
                            || old.note.as_ref().map(|n| &n.text)
                                != new.note.as_ref().map(|n| &n.text)
                        {
                            return Err(invalid("任务内容校验失败，未写入文件。"));
                        }
                    }
                    return Ok(proposed);
                }
            }
            _ => {}
        }
    }
    Err(invalid("无法定位任务完整 Markdown 列表项，未写入文件。"))
}

/// Mirror the parser's Markdown-visible heading names, excluding code fences.
/// Only independent ATX H2 headings are safe insertion points for the v1 writer.
fn heading_options(document: &Document) -> Vec<ColumnHeadingOption> {
    let offset = document.frontmatter.range.end;
    let body = &document.source[offset..];
    let mut current: Option<(usize, String)> = None;
    let mut headings = Vec::new();
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(
            document
                .source
                .match_indices('\n')
                .map(|(index, _)| index + 1),
        )
        .collect();
    for (event, range) in Parser::new_ext(body, Options::ENABLE_TASKLISTS).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading {
                level: HeadingLevel::H2,
                ..
            }) => current = Some((range.start + offset, String::new())),
            Event::Text(text) | Event::Code(text) => {
                if let Some((_, name)) = &mut current {
                    name.push_str(&text);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some((_, name)) = &mut current {
                    name.push('\n');
                }
            }
            Event::End(TagEnd::Heading(HeadingLevel::H2)) => {
                if let Some((start, name)) = current.take() {
                    let line_number = line_starts.partition_point(|offset| *offset <= start);
                    let line_start = line_starts[line_number - 1];
                    let line = document.source[line_start..]
                        .split('\n')
                        .next()
                        .unwrap_or("");
                    let atx = line.starts_with("## ") || line.starts_with("##\t");
                    let selectable = atx
                        && !name.trim().is_empty()
                        && !name.contains(['\r', '\n'])
                        && name.len() <= 512;
                    headings.push(ColumnHeadingOption {
                        name,
                        line: line_number,
                        selectable,
                        reason: (!selectable)
                            .then(|| "仅支持独立、单行且非空的 ## 二级标题。".into()),
                    });
                }
            }
            _ => {}
        }
    }
    let mut counts = HashMap::new();
    for heading in &headings {
        *counts.entry(heading.name.clone()).or_insert(0usize) += 1;
    }
    for heading in &mut headings {
        if counts[&heading.name] > 1 {
            heading.selectable = false;
            heading.reason = Some("标题重名，无法唯一定位；请先重命名。".into());
        }
    }
    headings
}

/// Touch only a missing key or the literal empty sequence in a simple block mapping.
/// Reject aliases, flow mappings and multi-line empty values rather than reserialize YAML.
fn insert_declarations(document: &Document, entries: &str) -> Result<String> {
    let raw = &document.frontmatter.raw;
    if raw
        .lines()
        .find(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        .is_some_and(|line| line.trim_start().starts_with('{'))
    {
        return Err(invalid(
            "暂不支持保真修改行内 YAML 映射；请使用块式 frontmatter。",
        ));
    }
    let mut output = document.source.clone();
    if document.frontmatter.value.get("columns").is_none() {
        output.insert_str(
            document.frontmatter.body_range.start,
            &format!("columns:{}{entries}", document.line_ending.as_str()),
        );
        return Ok(output);
    }
    let mut candidates = Vec::new();
    let mut offset = document.frontmatter.body_range.start;
    for line in raw.split_inclusive('\n') {
        let key_len = ["columns:", "'columns':", "\"columns\":"]
            .iter()
            .find_map(|key| line.starts_with(key).then_some(key.len()));
        if let Some(key_len) = key_len {
            let tail = &line[key_len..];
            let trimmed = tail.trim_start_matches([' ', '\t']);
            if trimmed.starts_with('[') {
                if let Some(end) = trimmed.find(']') {
                    let remainder = trimmed[end + 1..].trim();
                    if trimmed[1..end].trim().is_empty()
                        && (remainder.is_empty() || remainder.starts_with('#'))
                    {
                        let start = offset + key_len + tail.len() - trimmed.len();
                        candidates.push((start..start + end + 1, offset + line.len()));
                    }
                }
            }
        }
        offset += line.len();
    }
    if candidates.len() != 1 {
        return Err(invalid(
            "暂不支持保真修改此 columns 写法；空列请使用 columns: []。",
        ));
    }
    let (empty_value, after_line) = candidates.pop().unwrap();
    output.insert_str(after_line, entries);
    output.replace_range(empty_value, "");
    Ok(output)
}
