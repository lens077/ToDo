use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::Write,
    ops::Range,
    path::Path,
};

use crate::{
    error::io_error, parse::hash_bytes, parse::valid_id, CompletionPolicy, Document, Error, Result,
    TokenKind,
};

#[derive(Debug, Clone)]
pub enum Change {
    SetCompletionPolicy {
        policy: CompletionPolicy,
    },
    SetCompleted {
        task: usize,
        completed: bool,
    },
    SetBody {
        task: usize,
        body: String,
    },
    SetMetadata {
        task: usize,
        kind: TokenKind,
        raw: String,
    },
    RemoveMetadata {
        task: usize,
        token: usize,
    },
    InsertTask {
        column: Option<String>,
        position: InsertPosition,
        line: String,
    },
    DeleteTask {
        task: usize,
    },
    DeleteSubtree {
        task: usize,
    },
    MoveTask {
        task: usize,
        column: String,
        position: InsertPosition,
        /// A fresh completion note when this move enters a done column.
        completion_note: Option<String>,
        /// A validated Git range written when this move enters a done column.
        completion_evidence: Option<(String, String)>,
    },
    /// Write the `↳ … <!--n:ID-->` line under a task, replacing any existing note.
    SetNote {
        task: usize,
        text: String,
    },
    RemoveNote {
        task: usize,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum InsertPosition {
    Start,
    End,
    Before(usize),
    After(usize),
}

#[derive(Debug, Clone)]
pub struct Edit {
    pub range: Range<usize>,
    pub replacement: String,
    /// Original 1-based lines this edit may alter. Insertions use the line at their boundary.
    pub expected_lines: BTreeSet<usize>,
}

pub struct OptimisticLock<'a> {
    document: &'a Document,
}

impl<'a> OptimisticLock<'a> {
    pub fn new(document: &'a Document) -> Self {
        Self { document }
    }

    pub fn validate(&self) -> Result<()> {
        let snapshot = self
            .document
            .snapshot
            .as_ref()
            .ok_or(Error::ConcurrentModification)?;
        let bytes = fs::read(&snapshot.path).map_err(|source| io_error(&snapshot.path, source))?;
        let metadata =
            fs::metadata(&snapshot.path).map_err(|source| io_error(&snapshot.path, source))?;
        if hash_bytes(&bytes) != snapshot.hash || metadata.modified().ok() != snapshot.modified {
            return Err(Error::ConcurrentModification);
        }
        Ok(())
    }

    pub fn write(&self, changes: &[Change]) -> Result<Document> {
        self.validate()?;
        let snapshot = self
            .document
            .snapshot
            .as_ref()
            .ok_or(Error::ConcurrentModification)?;
        let output = apply_changes(self.document, changes)?;
        atomic_write(&snapshot.path, output.as_bytes())?;
        crate::parse_file(&snapshot.path)
    }
}

pub fn apply_changes(document: &Document, changes: &[Change]) -> Result<String> {
    let mut edits = Vec::new();
    for change in changes {
        edits.extend(change_to_edits(document, change)?);
    }
    validate_edits(&document.source, &edits)?;
    let mut output = document.source.clone();
    let mut ordered = edits.clone();
    ordered.sort_by_key(|edit| edit.range.start);
    for edit in ordered.iter().rev() {
        output.replace_range(edit.range.clone(), &edit.replacement);
    }
    validate_changed_lines(&document.source, &output, &edits)?;
    // A successful write must remain a valid v1 task file and conflict-free.
    crate::parse(output.clone())?;
    Ok(output)
}

pub fn atomic_write(path: impl AsRef<Path>, bytes: &[u8]) -> Result<()> {
    let path = path.as_ref();
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("TODO.md");
    let mut counter = 0u32;
    let (temp_path, mut file) = loop {
        let candidate = parent.join(format!(".{name}.todo-tmp-{}-{counter}", std::process::id()));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => break (candidate, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => counter += 1,
            Err(source) => return Err(io_error(&candidate, source)),
        }
    };
    let result = (|| {
        file.write_all(bytes)
            .map_err(|source| io_error(&temp_path, source))?;
        file.sync_all()
            .map_err(|source| io_error(&temp_path, source))?;
        if let Ok(metadata) = fs::metadata(path) {
            fs::set_permissions(&temp_path, metadata.permissions())
                .map_err(|source| io_error(&temp_path, source))?;
        }
        fs::rename(&temp_path, path).map_err(|source| io_error(path, source))?;
        if let Ok(directory) = OpenOptions::new().read(true).open(parent) {
            let _ = directory.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn change_to_edits(document: &Document, change: &Change) -> Result<Vec<Edit>> {
    match change {
        Change::SetCompletionPolicy { policy } => {
            Ok(vec![completion_policy_edit(document, *policy)?])
        }
        Change::SetCompleted { task, completed } => {
            let task = task_at(document, *task)?;
            let replacement = if *completed { "[x]" } else { "[ ]" };
            Ok(vec![edit(
                document,
                task.checkbox_range.clone(),
                replacement.into(),
            )])
        }
        Change::SetBody { task, body } => {
            reject_newline(body)?;
            let task = task_at(document, *task)?;
            Ok(vec![edit(document, task.body_range.clone(), body.clone())])
        }
        Change::SetMetadata { task, kind, raw } => {
            reject_newline(raw)?;
            validate_metadata(kind, raw)?;
            let task = task_at(document, *task)?;
            if let Some(existing) = task
                .metadata
                .iter()
                .find(|token| same_kind(&token.kind, kind))
            {
                return Ok(vec![edit(document, existing.range.clone(), raw.clone())]);
            }
            let insertion = task.line_range.end;
            let separator = if insertion > task.body_range.start
                && !document.source.as_bytes()[insertion - 1].is_ascii_whitespace()
            {
                " "
            } else {
                ""
            };
            Ok(vec![edit(
                document,
                insertion..insertion,
                format!("{separator}{raw}"),
            )])
        }
        Change::RemoveMetadata { task, token } => {
            let task = task_at(document, *task)?;
            let token = task.metadata.get(*token).ok_or(Error::InvalidRange)?;
            let mut start = token.range.start;
            if start > task.body_range.end
                && document.source.as_bytes()[start - 1].is_ascii_whitespace()
            {
                start -= 1;
            }
            Ok(vec![edit(document, start..token.range.end, String::new())])
        }
        Change::InsertTask {
            column,
            position,
            line,
        } => {
            validate_new_task_line(line)?;
            let offset = insertion_offset(document, column.as_deref(), *position)?;
            let newline = local_newline(document, offset);
            let prefix = if offset > 0 && !document.source[..offset].ends_with('\n') {
                newline
            } else {
                ""
            };
            Ok(vec![edit(
                document,
                offset..offset,
                format!("{prefix}{line}{newline}"),
            )])
        }
        Change::DeleteTask { task } => {
            let task = task_at(document, *task)?;
            // A note belongs to its task; leaving it behind would orphan a line whose
            // anchor names an id that no longer exists.
            let end = task
                .note
                .as_ref()
                .map_or(task.full_line_range.end, |note| note.full_line_range.end);
            Ok(vec![edit(
                document,
                task.full_line_range.start..end,
                String::new(),
            )])
        }
        Change::DeleteSubtree { task } => {
            let root = task_at(document, *task)?;
            let subtree_end = document
                .tasks
                .iter()
                .enumerate()
                .skip(*task + 1)
                .find(|(_, candidate)| candidate.indent_level <= root.indent_level)
                .map_or(document.tasks.len(), |(index, _)| index);
            let last = &document.tasks[subtree_end - 1];
            let end = last
                .note
                .as_ref()
                .map_or(last.full_line_range.end, |note| note.full_line_range.end);
            Ok(vec![edit(
                document,
                root.full_line_range.start..end,
                String::new(),
            )])
        }
        Change::MoveTask {
            task,
            column,
            position,
            completion_note,
            completion_evidence,
        } => move_task_edits(
            document,
            *task,
            column,
            *position,
            completion_note.as_deref(),
            completion_evidence.as_ref(),
        ),
        Change::SetNote { task, text } => {
            reject_newline(text)?;
            validate_note_text(text)?;
            let task = task_at(document, *task)?;
            let id = task.id.as_deref().ok_or(Error::MissingTaskId)?;
            let rendered = format!("{}↳ {text} <!--n:{id}-->", note_indent(task));
            match &task.note {
                Some(note) => Ok(vec![edit(document, note.line_range.clone(), rendered)]),
                None => {
                    let offset = task.full_line_range.end;
                    let newline = local_newline(document, offset);
                    // Without a trailing newline on the task line the note would be
                    // appended to it; add the break first, then the note line.
                    let (prefix, suffix) = if document.source[..offset].ends_with('\n') {
                        ("", newline)
                    } else {
                        (newline, "")
                    };
                    Ok(vec![edit(
                        document,
                        offset..offset,
                        format!("{prefix}{rendered}{suffix}"),
                    )])
                }
            }
        }
        Change::RemoveNote { task } => {
            let task = task_at(document, *task)?;
            match &task.note {
                Some(note) => Ok(vec![edit(
                    document,
                    note.full_line_range.clone(),
                    String::new(),
                )]),
                None => Ok(Vec::new()),
            }
        }
    }
}

fn completion_policy_edit(document: &Document, policy: CompletionPolicy) -> Result<Edit> {
    let body = &document.source[document.frontmatter.body_range.clone()];
    let mut offset = document.frontmatter.body_range.start;
    for line in body.split_inclusive('\n') {
        let content = line.trim_end_matches(['\r', '\n']);
        if content.starts_with([' ', '\t']) {
            offset += line.len();
            continue;
        }
        let Some(colon) = content.find(':') else {
            offset += line.len();
            continue;
        };
        let key = content[..colon].trim().trim_matches(['\'', '"']);
        if key != "completion-policy" {
            offset += line.len();
            continue;
        }
        let suffix = &content[colon + 1..];
        let leading = suffix.len() - suffix.trim_start_matches([' ', '\t']).len();
        let value = &suffix[leading..];
        let value_len = yaml_scalar_len(value);
        if value_len == 0 {
            return Err(Error::InvalidFrontmatter(
                "completion-policy must be strict or normal".into(),
            ));
        }
        let start = offset + colon + 1 + leading;
        return Ok(edit(
            document,
            start..start + value_len,
            policy.as_str().into(),
        ));
    }

    let offset = document.frontmatter.body_range.end;
    let newline = local_newline(document, offset);
    Ok(edit(
        document,
        offset..offset,
        format!("completion-policy: {}{newline}", policy.as_str()),
    ))
}

fn yaml_scalar_len(value: &str) -> usize {
    match value.as_bytes().first().copied() {
        Some(quote @ (b'\'' | b'"')) => value
            .as_bytes()
            .iter()
            .enumerate()
            .skip(1)
            .find(|(_, byte)| **byte == quote)
            .map_or(value.len(), |(index, _)| index + 1),
        Some(_) => {
            let end = value
                .char_indices()
                .find(|(index, character)| {
                    *character == '#'
                        && (*index == 0 || value.as_bytes()[*index - 1].is_ascii_whitespace())
                })
                .map_or(value.len(), |(index, _)| index);
            value[..end].trim_end_matches([' ', '\t']).len()
        }
        None => 0,
    }
}

/// Indent that keeps a note line inside its task's list item.
///
/// Six spaces past the task's own indent aligns the note under the task body. An
/// indented code block cannot interrupt a paragraph, so this stays a lazy
/// continuation line rather than becoming a code block.
fn note_indent(task: &crate::Task) -> String {
    format!("{}      ", task.indent)
}

fn validate_note_text(text: &str) -> Result<()> {
    if text.trim().is_empty() {
        return Err(Error::InvalidTaskLine("note text is empty".into()));
    }
    // The anchor is the only HTML comment allowed on a note line; comment syntax in
    // the prose would let a note swallow or fake its own anchor.
    if text.contains("<!--") || text.contains("-->") {
        return Err(Error::InvalidTaskLine(
            "note text must not contain HTML comment markers".into(),
        ));
    }
    Ok(())
}

fn move_task_edits(
    document: &Document,
    task_index: usize,
    column_id: &str,
    position: InsertPosition,
    completion_note: Option<&str>,
    completion_evidence: Option<&(String, String)>,
) -> Result<Vec<Edit>> {
    let root = task_at(document, task_index)?;
    let target_column = document
        .columns
        .iter()
        .find(|column| column.id == column_id)
        .ok_or_else(|| Error::ColumnNotFound(column_id.into()))?;
    let complete = target_column.kind.as_deref() == Some("done");
    if !complete && (completion_note.is_some() || completion_evidence.is_some()) {
        return Err(Error::InvalidTaskLine(
            "completion details require a done destination".into(),
        ));
    }
    if let Some(note) = completion_note {
        reject_newline(note)?;
        validate_note_text(note)?;
    }

    let source_done = root.column.as_deref().is_some_and(|source_column| {
        document
            .columns
            .iter()
            .any(|column| column.id == source_column && column.kind.as_deref() == Some("done"))
    });
    let subtree_end_index = document
        .tasks
        .iter()
        .enumerate()
        .skip(task_index + 1)
        .find(|(_, task)| task.indent_level <= root.indent_level)
        .map_or(document.tasks.len(), |(index, _)| index);
    if let InsertPosition::Before(target) | InsertPosition::After(target) = position {
        if target > task_index && target < subtree_end_index {
            return Err(Error::InvalidTaskLine(
                "move target belongs to the task subtree".into(),
            ));
        }
    }
    let last = &document.tasks[subtree_end_index - 1];
    let block_end = last
        .note
        .as_ref()
        .map_or(last.full_line_range.end, |note| note.full_line_range.end);
    let block_range = root.full_line_range.start..block_end;
    let destination = insertion_offset(document, Some(column_id), position)?;
    let target_indent = match position {
        InsertPosition::Before(index) => task_at(document, index)?.indent.clone(),
        InsertPosition::After(index) => task_at(document, index)?.indent.clone(),
        InsertPosition::Start | InsertPosition::End => String::new(),
    };
    let newline = local_newline(document, destination);
    let mut moved = String::new();
    for (offset, task) in document.tasks[task_index..subtree_end_index]
        .iter()
        .enumerate()
    {
        let relative_indent = task
            .indent
            .strip_prefix(&root.indent)
            .unwrap_or(&task.indent);
        let new_indent = format!("{target_indent}{relative_indent}");
        let destination_completed = if offset == 0 {
            complete
        } else if complete && !source_done {
            true
        } else if !complete && source_done {
            false
        } else {
            task.completed
        };
        let entering_done = destination_completed && !task.completed;
        let clear_outcome = destination_completed
            && (entering_done || task.progress().is_some() || task.failure().is_some());
        let evidence = (offset == 0).then_some(completion_evidence).flatten();
        moved.push_str(&render_moved_task_line(
            document,
            task,
            &new_indent,
            destination_completed,
            clear_outcome,
            evidence,
        )?);
        moved.push_str(newline);

        let fresh_note = (offset == 0 && complete)
            .then_some(completion_note)
            .flatten();
        if let Some(note) = fresh_note {
            let id = task.id.as_deref().ok_or(Error::MissingTaskId)?;
            moved.push_str(&format!("{new_indent}      ↳ {note} <!--n:{id}-->"));
            moved.push_str(newline);
        } else if !(offset == 0 && complete && entering_done) {
            if let Some(note) = &task.note {
                let relative_indent = note
                    .indent
                    .strip_prefix(&task.indent)
                    .unwrap_or(&note.indent);
                let mut raw = document.source[note.line_range.clone()].to_string();
                raw.replace_range(
                    0..note.indent.len(),
                    &format!("{new_indent}{relative_indent}"),
                );
                moved.push_str(&raw);
                moved.push_str(newline);
            }
        }
    }

    // A same-column move can already point inside its own block. It still has
    // work to do when the column state must repair the checkbox, so replace the
    // block in place instead of treating it as a no-op.
    if block_range.contains(&destination) || destination == block_range.end {
        let original = &document.source[block_range.clone()];
        if !original.ends_with(['\n', '\r']) && moved.ends_with(newline) {
            moved.truncate(moved.len() - newline.len());
        }
        return if original == moved {
            Ok(Vec::new())
        } else {
            Ok(vec![edit(document, block_range, moved)])
        };
    }

    let prefix = if destination > 0 && !document.source[..destination].ends_with('\n') {
        newline
    } else {
        ""
    };
    Ok(vec![
        edit(document, block_range, String::new()),
        edit(
            document,
            destination..destination,
            format!("{prefix}{moved}"),
        ),
    ])
}

fn render_moved_task_line(
    document: &Document,
    task: &crate::Task,
    new_indent: &str,
    completed: bool,
    clear_outcome: bool,
    completion_evidence: Option<&(String, String)>,
) -> Result<String> {
    let line_start = task.line_range.start;
    let mut raw = document.source[task.line_range.clone()].to_string();
    let mut replacements: Vec<(Range<usize>, String)> = Vec::new();

    if task.indent != new_indent {
        replacements.push((0..task.indent.len(), new_indent.into()));
    }
    if task.completed != completed {
        replacements.push((
            task.checkbox_range.start - line_start..task.checkbox_range.end - line_start,
            if completed { "[x]" } else { "[ ]" }.into(),
        ));
    }
    if clear_outcome {
        for metadata in &task.metadata {
            if matches!(
                metadata.kind,
                TokenKind::Progress(_) | TokenKind::Failure(_)
            ) {
                let mut start = metadata.range.start - line_start;
                if start > task.body_range.end - line_start
                    && raw.as_bytes()[start - 1].is_ascii_whitespace()
                {
                    start -= 1;
                }
                replacements.push((start..metadata.range.end - line_start, String::new()));
            }
        }
    }
    if let Some((from, to)) = completion_evidence {
        let kind = TokenKind::Evidence {
            from: from.clone(),
            to: to.clone(),
        };
        let rendered = format!("🔗{from}..{to}");
        validate_metadata(&kind, &rendered)?;
        if let Some(existing) = task
            .metadata
            .iter()
            .find(|metadata| matches!(metadata.kind, TokenKind::Evidence { .. }))
        {
            replacements.push((
                existing.range.start - line_start..existing.range.end - line_start,
                rendered,
            ));
        } else {
            let separator = if raw.as_bytes().last().is_some_and(u8::is_ascii_whitespace) {
                ""
            } else {
                " "
            };
            replacements.push((raw.len()..raw.len(), format!("{separator}{rendered}")));
        }
    }

    replacements.sort_by_key(|(range, _)| range.start);
    for (range, replacement) in replacements.into_iter().rev() {
        raw.replace_range(range, &replacement);
    }
    Ok(raw)
}

fn insertion_offset(
    document: &Document,
    column: Option<&str>,
    position: InsertPosition,
) -> Result<usize> {
    match position {
        InsertPosition::Before(index) => Ok(task_at(document, index)?.full_line_range.start),
        InsertPosition::After(index) => {
            let task = task_at(document, index)?;
            let next_sibling = document.tasks.iter().skip(index + 1).find(|candidate| {
                candidate.indent_level <= task.indent_level && candidate.column == task.column
            });
            let subtree_end = if let Some(candidate) = next_sibling {
                candidate.full_line_range.start
            } else {
                let last = document
                    .tasks
                    .iter()
                    .skip(index + 1)
                    .take_while(|candidate| {
                        candidate.column == task.column
                            && candidate.indent_level > task.indent_level
                    })
                    .last()
                    .unwrap_or(task);
                last.note
                    .as_ref()
                    .map_or(last.full_line_range.end, |note| note.full_line_range.end)
            };
            Ok(subtree_end)
        }
        InsertPosition::Start | InsertPosition::End => {
            let tasks: Vec<_> = document
                .tasks
                .iter()
                .filter(|task| task.column.as_deref() == column)
                .collect();
            match position {
                InsertPosition::Start => {
                    if let Some(task) = tasks.first() {
                        Ok(task.full_line_range.start)
                    } else if let Some(id) = column {
                        let column = document
                            .columns
                            .iter()
                            .find(|item| item.id == id)
                            .ok_or_else(|| Error::ColumnNotFound(id.into()))?;
                        Ok(column.heading_range.end)
                    } else {
                        Ok(document.frontmatter.range.end)
                    }
                }
                InsertPosition::End => {
                    if let Some(task) = tasks.last() {
                        Ok(task
                            .note
                            .as_ref()
                            .map_or(task.full_line_range.end, |note| note.full_line_range.end))
                    } else if let Some(id) = column {
                        let column = document
                            .columns
                            .iter()
                            .find(|item| item.id == id)
                            .ok_or_else(|| Error::ColumnNotFound(id.into()))?;
                        Ok(column.heading_range.end)
                    } else {
                        Ok(document.frontmatter.range.end)
                    }
                }
                _ => unreachable!(),
            }
        }
    }
}

fn validate_edits(source: &str, edits: &[Edit]) -> Result<()> {
    let mut sorted: Vec<_> = edits.iter().collect();
    sorted.sort_by_key(|edit| (edit.range.start, edit.range.end));
    for edit in &sorted {
        if edit.range.start > edit.range.end
            || edit.range.end > source.len()
            || !source.is_char_boundary(edit.range.start)
            || !source.is_char_boundary(edit.range.end)
        {
            return Err(Error::InvalidRange);
        }
    }
    for pair in sorted.windows(2) {
        let left = pair[0];
        let right = pair[1];
        let overlapping = left.range.end > right.range.start
            || (left.range.is_empty()
                && right.range.is_empty()
                && left.range.start == right.range.start);
        if overlapping {
            return Err(Error::OverlappingEdits);
        }
    }
    Ok(())
}

fn validate_changed_lines(before: &str, after: &str, edits: &[Edit]) -> Result<()> {
    let expected: BTreeSet<_> = edits
        .iter()
        .flat_map(|edit| edit.expected_lines.iter().copied())
        .collect();
    // The output must equal exactly the declared surgical splices. Comparing every
    // preserved segment catches an accidentally widened range before bytes reach disk.
    let mut ordered = edits.to_vec();
    ordered.sort_by_key(|edit| edit.range.start);
    let mut before_cursor = 0;
    let mut after_cursor = 0;
    for edit in &ordered {
        let preserved_len = edit.range.start - before_cursor;
        let after_preserved_end = after_cursor + preserved_len;
        if after_preserved_end > after.len()
            || before[before_cursor..edit.range.start] != after[after_cursor..after_preserved_end]
        {
            return Err(Error::UnexpectedLineChanges {
                expected: expected.iter().copied().collect(),
                actual: expected.iter().copied().collect(),
            });
        }
        after_cursor = after_preserved_end + edit.replacement.len();
        before_cursor = edit.range.end;
    }
    if before[before_cursor..] != after[after_cursor..] {
        return Err(Error::UnexpectedLineChanges {
            expected: expected.iter().copied().collect(),
            actual: expected.iter().copied().collect(),
        });
    }
    Ok(())
}

fn edit(document: &Document, range: Range<usize>, replacement: String) -> Edit {
    let start = line_number(&document.source, range.start);
    let end = line_number(
        &document.source,
        range.end.saturating_sub(1).max(range.start),
    );
    Edit {
        range,
        replacement,
        expected_lines: (start..=end).collect(),
    }
}

fn line_number(source: &str, offset: usize) -> usize {
    source.as_bytes()[..offset.min(source.len())]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
        + 1
}

fn local_newline(document: &Document, offset: usize) -> &'static str {
    if offset >= 2 && &document.source.as_bytes()[offset - 2..offset] == b"\r\n" {
        "\r\n"
    } else {
        document.line_ending.as_str()
    }
}

fn task_at(document: &Document, index: usize) -> Result<&crate::Task> {
    document.tasks.get(index).ok_or(Error::InvalidRange)
}

fn reject_newline(value: &str) -> Result<()> {
    if value.contains(['\n', '\r']) {
        Err(Error::InvalidTaskLine(
            "task fields must remain on one line".into(),
        ))
    } else {
        Ok(())
    }
}

fn validate_new_task_line(line: &str) -> Result<()> {
    reject_newline(line)?;
    let trimmed = line.trim_start_matches([' ', '\t']);
    if !(trimmed.starts_with("- [") || trimmed.starts_with("+ [") || trimmed.starts_with("* [")) {
        return Err(Error::InvalidTaskLine(line.into()));
    }
    Ok(())
}

fn validate_metadata(kind: &TokenKind, raw: &str) -> Result<()> {
    match kind {
        TokenKind::StableId(id) => {
            if !valid_id(id) || raw != format!("<!--t:{id}-->") {
                return Err(Error::InvalidTaskId(id.clone()));
            }
        }
        TokenKind::Unknown => {}
        _ if raw.split_whitespace().count() != 1 => {
            return Err(Error::InvalidTaskLine(
                "metadata token contains whitespace".into(),
            ))
        }
        _ => {}
    }
    Ok(())
}

fn same_kind(left: &TokenKind, right: &TokenKind) -> bool {
    std::mem::discriminant(left) == std::mem::discriminant(right)
}
