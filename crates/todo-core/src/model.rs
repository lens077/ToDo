use std::{ops::Range, path::PathBuf, time::SystemTime};

#[derive(Debug, Clone)]
pub struct Document {
    pub source: String,
    pub frontmatter: Frontmatter,
    pub columns: Vec<Column>,
    pub tasks: Vec<Task>,
    pub unrecognized_task_like_lines: Vec<UnrecognizedTaskLikeLine>,
    pub bom: bool,
    pub line_ending: LineEnding,
    pub final_newline: bool,
    pub snapshot: Option<FileSnapshot>,
}

impl Document {
    pub fn duplicate_ids(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut duplicates = std::collections::HashSet::new();
        for id in self.tasks.iter().filter_map(|task| task.id.as_ref()) {
            if !seen.insert(id) {
                duplicates.insert(id.clone());
            }
        }
        let mut values: Vec<_> = duplicates.into_iter().collect();
        values.sort();
        values
    }

    pub fn task(&self, id: &str) -> Option<&Task> {
        self.tasks
            .iter()
            .find(|task| task.id.as_deref() == Some(id))
    }

    pub fn write_unchanged(&self) -> &[u8] {
        self.source.as_bytes()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnrecognizedTaskLikeLine {
    pub line: usize,
    pub raw: String,
    pub range: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct FileSnapshot {
    pub path: PathBuf,
    pub modified: Option<SystemTime>,
    pub hash: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    CrLf,
    Mixed,
    None,
}

impl LineEnding {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CrLf => "\r\n",
            _ => "\n",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Frontmatter {
    pub range: Range<usize>,
    pub raw: String,
    pub todo_spec: u64,
    pub columns: Vec<ColumnDefinition>,
    pub value: serde_yaml::Value,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ColumnDefinition {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Column {
    pub id: String,
    pub name: String,
    pub kind: Option<String>,
    pub heading_range: Range<usize>,
    pub content_range: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct Task {
    pub id: Option<String>,
    pub body: String,
    pub completed: bool,
    pub checkbox: char,
    pub indent: String,
    pub indent_level: usize,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub column: Option<String>,
    pub metadata: Vec<MetadataToken>,
    pub unknown_tokens: Vec<MetadataToken>,
    /// Bytes occupied by the line excluding its newline.
    pub line_range: Range<usize>,
    /// Bytes occupied by the complete line including its newline when present.
    pub full_line_range: Range<usize>,
    pub checkbox_range: Range<usize>,
    pub body_range: Range<usize>,
    pub metadata_range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataToken {
    pub kind: TokenKind,
    pub raw: String,
    pub range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    StableId(String),
    Tag(String),
    Assignee(String),
    DueDate(String),
    CompletionDate(String),
    Priority(u8),
    Sort(i64),
    Evidence { from: String, to: String },
    Dependency(String),
    Reference { path: String, id: String },
    Unknown,
}
