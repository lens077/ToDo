use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::Write,
    ops::Range,
    path::Path,
};

use crate::{
    error::io_error, parse::hash_bytes, parse::valid_id, Document, Error, Result, TokenKind,
};

#[derive(Debug, Clone)]
pub enum Change {
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
    MoveTask {
        task: usize,
        column: String,
        position: InsertPosition,
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

    pub fn write(&self, changes: &[Change]) -> Result<Document> {
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
            Ok(vec![edit(
                document,
                task.full_line_range.clone(),
                String::new(),
            )])
        }
        Change::MoveTask {
            task,
            column,
            position,
        } => move_task_edits(document, *task, column, *position),
    }
}

fn move_task_edits(
    document: &Document,
    task_index: usize,
    column_id: &str,
    position: InsertPosition,
) -> Result<Vec<Edit>> {
    let root = task_at(document, task_index)?;
    let subtree_end_index = document
        .tasks
        .iter()
        .enumerate()
        .skip(task_index + 1)
        .find(|(_, task)| task.indent_level <= root.indent_level)
        .map_or(document.tasks.len(), |(index, _)| index);
    let block_range =
        root.full_line_range.start..document.tasks[subtree_end_index - 1].full_line_range.end;
    let destination = match position {
        InsertPosition::After(index) => {
            let target = task_at(document, index)?;
            let next_same_column = document
                .tasks
                .iter()
                .skip(index + 1)
                .find(|candidate| candidate.column == target.column)
                .map(|candidate| candidate.full_line_range.start);
            next_same_column
                .or_else(|| {
                    document
                        .columns
                        .iter()
                        .find(|column| column.id == column_id)
                        .map(|column| column.content_range.end)
                })
                .unwrap_or(target.full_line_range.end)
        }
        _ => insertion_offset(document, Some(column_id), position)?,
    };
    if block_range.contains(&destination) || destination == block_range.end {
        return Ok(Vec::new());
    }
    let target_indent = match position {
        InsertPosition::Before(index) => task_at(document, index)?.indent.clone(),
        InsertPosition::After(index) => task_at(document, index)?.indent.clone(),
        InsertPosition::Start | InsertPosition::End => String::new(),
    };
    let target_column = document
        .columns
        .iter()
        .find(|column| column.id == column_id)
        .ok_or_else(|| Error::ColumnNotFound(column_id.into()))?;
    let complete = target_column.kind.as_deref() == Some("done");
    let newline = local_newline(document, destination);
    let mut moved = String::new();
    for task in &document.tasks[task_index..subtree_end_index] {
        let relative_indent = task
            .indent
            .strip_prefix(&root.indent)
            .unwrap_or(&task.indent);
        let mut raw = document.source[task.line_range.clone()].to_string();
        raw.replace_range(
            0..task.indent.len(),
            &format!("{target_indent}{relative_indent}"),
        );
        if complete && !task.completed {
            let local_start = task.checkbox_range.start - task.line_range.start
                + target_indent.len()
                + relative_indent.len()
                - task.indent.len();
            raw.replace_range(local_start..local_start + 3, "[x]");
        }
        moved.push_str(&raw);
        moved.push_str(newline);
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

fn insertion_offset(
    document: &Document,
    column: Option<&str>,
    position: InsertPosition,
) -> Result<usize> {
    match position {
        InsertPosition::Before(index) => Ok(task_at(document, index)?.full_line_range.start),
        InsertPosition::After(index) => {
            let task = task_at(document, index)?;
            let subtree_end = document
                .tasks
                .iter()
                .skip(index + 1)
                .find(|candidate| {
                    candidate.indent_level <= task.indent_level && candidate.column == task.column
                })
                .map(|candidate| candidate.full_line_range.start)
                .or_else(|| {
                    task.column.as_deref().and_then(|column_id| {
                        document
                            .columns
                            .iter()
                            .find(|column| column.id == column_id)
                            .map(|column| column.content_range.end)
                    })
                })
                .unwrap_or(task.full_line_range.end);
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
                        Ok(task.full_line_range.end)
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
