use std::{collections::HashMap, fs, ops::Range, path::Path};

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde::Deserialize;

use crate::{
    error::{io_error, Error, Result},
    Column, ColumnDefinition, Document, FileSnapshot, Frontmatter, LineEnding, MetadataToken, Task,
    TaskNote, TokenKind, UnrecognizedTaskLikeLine,
};

#[derive(Deserialize)]
struct FrontmatterData {
    #[serde(rename = "todo-spec")]
    todo_spec: serde_yaml::Value,
    #[serde(default)]
    columns: Vec<ColumnDefinition>,
}

#[derive(Debug)]
struct Line {
    start: usize,
    content_end: usize,
    end: usize,
}

#[derive(Debug)]
struct Marker {
    range: Range<usize>,
    checked: bool,
}

pub fn parse(source: impl Into<String>) -> Result<Document> {
    parse_inner(source.into(), None)
}

pub fn parse_file(path: impl AsRef<Path>) -> Result<Document> {
    let path = path.as_ref();
    let bytes = fs::read(path).map_err(|source| io_error(path, source))?;
    let source = String::from_utf8(bytes).map_err(|_| Error::InvalidUtf8)?;
    let metadata = fs::metadata(path).map_err(|source| io_error(path, source))?;
    let snapshot = FileSnapshot {
        path: path.to_path_buf(),
        modified: metadata.modified().ok(),
        hash: hash_bytes(source.as_bytes()),
    };
    parse_inner(source, Some(snapshot))
}

fn parse_inner(source: String, snapshot: Option<FileSnapshot>) -> Result<Document> {
    reject_conflicts(&source)?;
    let bom = source.starts_with('\u{feff}');
    let lines = lines(&source);
    let frontmatter = parse_frontmatter(&source, &lines, bom)?;
    if frontmatter.todo_spec != 1 {
        return Err(Error::UnsupportedVersion {
            version: frontmatter.todo_spec.to_string(),
        });
    }

    // OffsetIter is the authoritative Markdown-aware task marker source. It excludes
    // fenced/indented code and inline code before line-level extraction is attempted.
    let mut markers = Vec::new();
    let mut heading_events = Vec::new();
    let mut current_heading: Option<(HeadingLevel, Range<usize>, String)> = None;
    let parser = Parser::new_ext(&source, Options::ENABLE_TASKLISTS).into_offset_iter();
    for (event, range) in parser {
        match event {
            Event::TaskListMarker(checked) => markers.push(Marker { range, checked }),
            Event::Start(Tag::Heading { level, .. }) => {
                current_heading = Some((level, range, String::new()));
            }
            Event::Text(text) | Event::Code(text) => {
                if let Some((_, _, value)) = &mut current_heading {
                    value.push_str(&text);
                }
            }
            Event::End(TagEnd::Heading(level)) => {
                if let Some((start_level, start_range, value)) = current_heading.take() {
                    if level == start_level {
                        heading_events.push((level, start_range.start..range.end, value));
                    }
                }
            }
            _ => {}
        }
    }

    let h2_boundaries: Vec<_> = heading_events
        .iter()
        .filter(|(level, _, _)| *level == HeadingLevel::H2)
        .map(|(_, range, name)| (range.start, name.clone()))
        .collect();
    let unrecognized_task_like_lines =
        scan_unrecognized_task_like_lines(&source, &lines, &frontmatter.range);
    let mut columns = build_columns(&source, &lines, &frontmatter.columns, heading_events);
    columns.sort_by_key(|column| column.heading_range.start);
    for index in 0..columns.len() {
        columns[index].content_range.end = columns
            .get(index + 1)
            .map_or(source.len(), |column| column.heading_range.start);
    }

    let mut tasks = Vec::new();
    let mut ids = HashMap::new();
    for marker in markers {
        let line_index = line_index_containing(&lines, marker.range.start)
            .ok_or_else(|| Error::InvalidTaskLine("marker outside a source line".into()))?;
        let line = &lines[line_index];
        let raw = &source[line.start..line.content_end];
        let marker_offset = marker.range.start - line.start;
        if !valid_task_prefix(raw, marker_offset) || in_inline_code(raw, marker_offset) {
            continue;
        }
        let mut task = parse_task_line(
            &source,
            line,
            marker,
            &columns,
            &h2_boundaries,
            &frontmatter.columns,
        )?;
        task.indent_level = indent_level(&task.indent, &tasks);
        let task_index = tasks.len();
        task.parent = (0..task_index)
            .rev()
            .find(|&index| tasks[index].indent_level < task.indent_level);
        if let Some(parent) = task.parent {
            tasks[parent].children.push(task_index);
        }
        if let Some(id) = &task.id {
            ids.entry(id.clone()).or_insert(task_index);
        }
        task.note = parse_note_line(&source, &lines, line_index, task.id.as_deref());
        tasks.push(task);
    }

    Ok(Document {
        line_ending: detect_line_ending(&source),
        final_newline: source.ends_with('\n') || source.ends_with('\r'),
        source,
        frontmatter,
        columns,
        tasks,
        unrecognized_task_like_lines,
        bom,
        snapshot,
    })
}

/// Read the `↳ <text> <!--n:ID-->` line directly beneath a task line.
///
/// The note must sit on the very next physical line and its `n:` anchor must name
/// the task that owns it. Both conditions are required: without the anchor a note
/// would silently reattach itself to whichever task it drifted next to, and
/// scanning further than one line would let an unrelated paragraph be adopted.
fn parse_note_line(
    source: &str,
    lines: &[Line],
    task_line_index: usize,
    task_id: Option<&str>,
) -> Option<TaskNote> {
    let id = task_id?;
    let line = lines.get(task_line_index + 1)?;
    let raw = &source[line.start..line.content_end];
    let indent_len = raw.len() - raw.trim_start_matches([' ', '\t']).len();
    let body = raw[indent_len..].strip_prefix('↳')?.trim_start();
    let anchor = format!("<!--n:{id}-->");
    let text = body.strip_suffix(&anchor)?.trim_end();
    Some(TaskNote {
        text: text.to_string(),
        indent: raw[..indent_len].to_string(),
        line_range: line.start..line.content_end,
        full_line_range: line.start..line.end,
    })
}

fn reject_conflicts(source: &str) -> Result<()> {
    for (index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start_matches([' ', '\t']);
        if trimmed.starts_with("<<<<<<<")
            || trimmed.starts_with("=======")
            || trimmed.starts_with(">>>>>>>")
        {
            return Err(Error::ConflictMarker { line: index + 1 });
        }
    }
    Ok(())
}

fn parse_frontmatter(source: &str, lines: &[Line], bom: bool) -> Result<Frontmatter> {
    let start = usize::from(bom) * '\u{feff}'.len_utf8();
    let first = lines.first().ok_or(Error::NotTaskFile)?;
    if first.start != 0 || source[start..first.content_end] != *"---" {
        return Err(Error::NotTaskFile);
    }
    let closing_index = lines
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, line)| &source[line.start..line.content_end] == "---")
        .map(|(index, _)| index)
        .ok_or(Error::NotTaskFile)?;
    let closing = &lines[closing_index];
    let body_start = first.end;
    let body_end = closing.start;
    let raw = source[body_start..body_end].to_string();
    let value: serde_yaml::Value =
        serde_yaml::from_str(&raw).map_err(|error| Error::InvalidFrontmatter(error.to_string()))?;
    let data: FrontmatterData = serde_yaml::from_value(value.clone())
        .map_err(|error| Error::InvalidFrontmatter(error.to_string()))?;
    let version = match data.todo_spec {
        serde_yaml::Value::Number(number) => number.as_u64().ok_or_else(|| {
            Error::InvalidFrontmatter("todo-spec must be a positive integer".into())
        })?,
        other => {
            return Err(Error::UnsupportedVersion {
                version: yaml_scalar(&other),
            })
        }
    };
    Ok(Frontmatter {
        range: 0..closing.end,
        raw,
        todo_spec: version,
        columns: data.columns,
        value,
    })
}

fn build_columns(
    source: &str,
    lines: &[Line],
    definitions: &[ColumnDefinition],
    events: Vec<(HeadingLevel, Range<usize>, String)>,
) -> Vec<Column> {
    events
        .into_iter()
        .filter_map(|(level, range, name)| {
            if level != HeadingLevel::H2 {
                return None;
            }
            let definition = definitions.iter().find(|column| column.name == name)?;
            let line = lines
                .iter()
                .find(|line| line.start <= range.start && range.start < line.end)?;
            Some(Column {
                id: definition.id.clone(),
                name: definition.name.clone(),
                kind: definition.kind.clone(),
                heading_range: line.start..line.end,
                content_range: line.end..source.len(),
            })
        })
        .collect()
}

fn parse_task_line(
    source: &str,
    line: &Line,
    marker: Marker,
    columns: &[Column],
    h2_boundaries: &[(usize, String)],
    definitions: &[ColumnDefinition],
) -> Result<Task> {
    let raw = &source[line.start..line.content_end];
    let local_marker = marker.range.start - line.start;
    let indent_end = raw
        .char_indices()
        .take_while(|(_, character)| *character == ' ' || *character == '\t')
        .map(|(index, character)| index + character.len_utf8())
        .last()
        .unwrap_or(0);
    let after_checkbox = marker.range.end - line.start;
    let content_start = raw[after_checkbox..]
        .char_indices()
        .find(|(_, character)| !character.is_whitespace())
        .map_or(raw.len(), |(index, _)| after_checkbox + index);
    let (body_end, mut metadata) = peel_metadata(raw, content_start, line.start);
    let body = raw[content_start..body_end].trim_end_matches([' ', '\t']);
    let trimmed_body_end = content_start + body.len();
    let id_tokens: Vec<_> = metadata
        .iter()
        .filter_map(|token| match &token.kind {
            TokenKind::StableId(id) => Some(id.clone()),
            _ => None,
        })
        .collect();
    if id_tokens.len() > 1 {
        return Err(Error::InvalidTaskLine(
            "multiple stable IDs on one task".into(),
        ));
    }
    let id = id_tokens.into_iter().next();
    let unknown_tokens = metadata
        .iter()
        .filter(|token| token.kind == TokenKind::Unknown)
        .cloned()
        .collect();
    metadata.sort_by_key(|token| token.range.start);
    let active_h2 = h2_boundaries
        .iter()
        .filter(|(start, _)| *start < line.start)
        .max_by_key(|(start, _)| *start)
        .map(|(_, name)| name);
    let column = active_h2.and_then(|name| {
        let definition = definitions.iter().find(|column| &column.name == name)?;
        columns
            .iter()
            .find(|column| column.id == definition.id)
            .map(|column| column.id.clone())
    });
    let checkbox = raw[local_marker + 1..]
        .chars()
        .next()
        .ok_or_else(|| Error::InvalidTaskLine(raw.into()))?;
    Ok(Task {
        id,
        body: body.to_string(),
        completed: marker.checked,
        checkbox,
        indent: raw[..indent_end].to_string(),
        indent_level: 0,
        parent: None,
        children: Vec::new(),
        column,
        metadata,
        unknown_tokens,
        note: None,
        line_range: line.start..line.content_end,
        full_line_range: line.start..line.end,
        checkbox_range: marker.range,
        body_range: line.start + content_start..line.start + trimmed_body_end,
        metadata_range: line.start + body_end..line.content_end,
    })
}

fn scan_unrecognized_task_like_lines(
    source: &str,
    lines: &[Line],
    frontmatter_range: &Range<usize>,
) -> Vec<UnrecognizedTaskLikeLine> {
    let mut result = Vec::new();
    let mut fence: Option<(u8, usize)> = None;
    for (index, line) in lines.iter().enumerate() {
        if line.start < frontmatter_range.end {
            continue;
        }
        let raw = &source[line.start..line.content_end];
        let trimmed = raw.trim_start_matches([' ', '\t']);
        let fence_byte = trimmed.as_bytes().first().copied();
        let fence_len = trimmed
            .bytes()
            .take_while(|byte| Some(*byte) == fence_byte && matches!(*byte, b'`' | b'~'))
            .count();
        if fence_len >= 3 {
            match fence {
                Some((byte, minimum)) if Some(byte) == fence_byte && fence_len >= minimum => {
                    fence = None;
                }
                None => fence = Some((fence_byte.unwrap(), fence_len)),
                _ => {}
            }
            continue;
        }
        if fence.is_some() {
            continue;
        }
        if let Some(offset) = nonstandard_checkbox_offset(raw) {
            if !in_inline_code(raw, offset) {
                result.push(UnrecognizedTaskLikeLine {
                    line: index + 1,
                    raw: raw.to_string(),
                    range: line.start..line.content_end,
                });
            }
        }
    }
    result
}

fn nonstandard_checkbox_offset(line: &str) -> Option<usize> {
    let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
    let rest = &line[indent..];
    let marker_end = if rest.starts_with(['-', '+', '*']) {
        1
    } else {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 || !matches!(rest.as_bytes().get(digits), Some(b'.' | b')')) {
            return None;
        }
        digits + 1
    };
    let whitespace = rest[marker_end..]
        .bytes()
        .take_while(u8::is_ascii_whitespace)
        .count();
    if whitespace == 0 {
        return None;
    }
    let checkbox = indent + marker_end + whitespace;
    let bytes = line.as_bytes();
    if bytes.get(checkbox) == Some(&b'[')
        && bytes.get(checkbox + 2) == Some(&b']')
        && !matches!(bytes.get(checkbox + 1), Some(b' ' | b'x' | b'X'))
    {
        Some(checkbox)
    } else {
        None
    }
}

fn valid_task_prefix(line: &str, marker_start: usize) -> bool {
    let before = &line[..marker_start];
    let trimmed = before.trim_start_matches([' ', '\t']);
    let marker = trimmed.as_bytes();
    if marker.len() < 2 {
        return false;
    }
    let unordered = matches!(marker[0], b'-' | b'+' | b'*')
        && marker[1..].iter().all(|byte| byte.is_ascii_whitespace());
    let ordered = marker
        .iter()
        .position(|byte| !byte.is_ascii_digit())
        .is_some_and(|index| {
            index > 0
                && matches!(marker[index], b'.' | b')')
                && marker[index + 1..]
                    .iter()
                    .all(|byte| byte.is_ascii_whitespace())
        });
    unordered || ordered
}

fn in_inline_code(line: &str, offset: usize) -> bool {
    let mut index = 0;
    let mut active_ticks = 0;
    while index < offset {
        if line.as_bytes()[index] == b'`' {
            let start = index;
            while index < line.len() && line.as_bytes()[index] == b'`' {
                index += 1;
            }
            let run = index - start;
            if active_ticks == 0 {
                active_ticks = run;
            } else if active_ticks == run {
                active_ticks = 0;
            }
        } else {
            index += 1;
        }
    }
    active_ticks != 0
}

fn peel_metadata(
    line: &str,
    content_start: usize,
    absolute_start: usize,
) -> (usize, Vec<MetadataToken>) {
    let mut end = line.len();
    let mut tokens = Vec::new();
    loop {
        let token_end = end;
        let trimmed_end = line[..token_end].trim_end_matches([' ', '\t']).len();
        if trimmed_end <= content_start {
            break;
        }
        if line[..trimmed_end].ends_with("-->") {
            if let Some(comment_start) = line[..trimmed_end].rfind("<!--") {
                if comment_start >= content_start
                    && line[comment_start..trimmed_end].split_whitespace().count() == 1
                {
                    let raw = &line[comment_start..trimmed_end];
                    let kind = classify_token(raw);
                    if !matches!(kind, TokenKind::Unknown) {
                        tokens.push(MetadataToken {
                            kind,
                            raw: raw.to_string(),
                            range: absolute_start + comment_start..absolute_start + trimmed_end,
                        });
                        end = comment_start;
                        continue;
                    }
                }
            }
        }
        let start = line[..trimmed_end]
            .rfind([' ', '\t'])
            .map_or(content_start, |index| index + 1);
        if start < content_start {
            break;
        }
        let raw = &line[start..trimmed_end];
        let kind = classify_token(raw);
        // Unknown suffix tokens are preserved and exposed, but peeling stops when a
        // whitespace-delimited word is ordinary prose rather than token-shaped.
        if matches!(kind, TokenKind::Unknown) && !looks_like_unknown_token(raw) {
            break;
        }
        tokens.push(MetadataToken {
            kind,
            raw: raw.to_string(),
            range: absolute_start + start..absolute_start + trimmed_end,
        });
        end = start;
    }
    tokens.reverse();
    (end, tokens)
}

fn classify_token(raw: &str) -> TokenKind {
    if let Some(id) = raw
        .strip_prefix("<!--t:")
        .and_then(|value| value.strip_suffix("-->"))
    {
        return if valid_id(id) {
            TokenKind::StableId(id.into())
        } else {
            TokenKind::Unknown
        };
    }
    if let Some(value) = raw.strip_prefix('#').filter(|value| !value.is_empty()) {
        return TokenKind::Tag(value.into());
    }
    if let Some(value) = raw.strip_prefix('@').filter(|value| !value.is_empty()) {
        return TokenKind::Assignee(value.into());
    }
    let normalized = raw.replace('\u{fe0f}', "");
    if let Some(value) = normalized
        .strip_prefix("📅")
        .filter(|value| valid_date(value))
    {
        return TokenKind::DueDate(value.into());
    }
    if let Some(value) = normalized
        .strip_prefix('✅')
        .filter(|value| valid_date(value))
    {
        return TokenKind::CompletionDate(value.into());
    }
    if let Some(value) = raw
        .strip_prefix('!')
        .and_then(|value| value.parse::<u8>().ok())
    {
        if (1..=4).contains(&value) {
            return TokenKind::Priority(value);
        }
    }
    if let Some(value) = raw
        .strip_prefix('^')
        .and_then(|value| value.parse::<i64>().ok())
    {
        return TokenKind::Sort(value);
    }
    if let Some(value) = normalized.strip_prefix("🔗") {
        if let Some((from, to)) = value.split_once("..") {
            if !from.is_empty() && !to.is_empty() {
                return TokenKind::Evidence {
                    from: from.into(),
                    to: to.into(),
                };
            }
        }
    }
    if let Some(value) = normalized
        .strip_prefix('⛔')
        .filter(|value| valid_id(value))
    {
        return TokenKind::Dependency(value.into());
    }
    if let Some(value) = raw.strip_prefix('→') {
        if let Some((path, id)) = value.rsplit_once('#') {
            if !path.is_empty() && valid_id(id) {
                return TokenKind::Reference {
                    path: path.into(),
                    id: id.into(),
                };
            }
        }
    }
    if let Some(value) = normalized
        .strip_prefix('◐')
        .and_then(|value| value.parse::<u8>().ok())
    {
        // 0 and 100 are rejected: 0 is indistinguishable from "not started" and 100
        // would be a second way to say "done", which §6.5.4 reserves for the checkbox.
        if (1..=99).contains(&value) {
            return TokenKind::Progress(value);
        }
    }
    if let Some(value) = normalized
        .strip_prefix('⚠')
        .filter(|value| valid_date(value))
    {
        return TokenKind::Failure(value.into());
    }
    TokenKind::Unknown
}

fn looks_like_unknown_token(raw: &str) -> bool {
    raw.starts_with("<!--")
        || raw.starts_with(['#', '@', '!', '^', '→', '◐', '⚠'])
        || raw.contains(':')
}

pub(crate) fn valid_id(id: &str) -> bool {
    id.len() >= 6
        && id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte.is_ascii_lowercase())
}

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
}

fn indent_level(indent: &str, existing: &[Task]) -> usize {
    if indent.is_empty() {
        return 0;
    }
    let unit = existing
        .iter()
        .find(|task| !task.indent.is_empty())
        .map(|task| task.indent.as_str())
        .unwrap_or(indent);
    if !unit.is_empty() && indent.len() % unit.len() == 0 {
        indent.len() / unit.len()
    } else {
        existing
            .iter()
            .rev()
            .find(|task| task.indent.len() < indent.len())
            .map_or(1, |task| task.indent_level + 1)
    }
}

fn lines(source: &str) -> Vec<Line> {
    if source.is_empty() {
        return Vec::new();
    }
    let mut result = Vec::new();
    let mut start = 0;
    for (index, byte) in source.bytes().enumerate() {
        if byte == b'\n' {
            let content_end = if index > start && source.as_bytes()[index - 1] == b'\r' {
                index - 1
            } else {
                index
            };
            result.push(Line {
                start,
                content_end,
                end: index + 1,
            });
            start = index + 1;
        }
    }
    if start < source.len() {
        result.push(Line {
            start,
            content_end: source.len(),
            end: source.len(),
        });
    }
    result
}

fn line_index_containing(lines: &[Line], offset: usize) -> Option<usize> {
    lines
        .binary_search_by(|line| {
            if offset < line.start {
                std::cmp::Ordering::Greater
            } else if offset >= line.end && line.end != line.start {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .ok()
}

fn detect_line_ending(source: &str) -> LineEnding {
    let crlf = source.match_indices("\r\n").count();
    let lf = source.bytes().filter(|byte| *byte == b'\n').count();
    match (crlf, lf) {
        (0, 0) => LineEnding::None,
        (0, _) => LineEnding::Lf,
        (a, b) if a == b => LineEnding::CrLf,
        _ => LineEnding::Mixed,
    }
}

fn yaml_scalar(value: &serde_yaml::Value) -> String {
    serde_yaml::to_string(value)
        .unwrap_or_else(|_| "unknown".into())
        .trim()
        .into()
}

pub(crate) fn hash_bytes(bytes: &[u8]) -> u64 {
    // Stable dependency-free FNV-1a hash. This is a change detector, not a security primitive.
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}
