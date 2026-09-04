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
    pub fn completion_policy(&self) -> CompletionPolicy {
        self.frontmatter.completion_policy
    }

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
    pub body_range: Range<usize>,
    pub raw: String,
    pub todo_spec: u64,
    pub columns: Vec<ColumnDefinition>,
    pub completion_policy: CompletionPolicy,
    pub value: serde_yaml::Value,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompletionPolicy {
    #[default]
    Normal,
    Strict,
}

impl CompletionPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Strict => "strict",
        }
    }

    /// Whether an explanation is mandatory for this terminal/partial outcome.
    pub fn requires_note(self, outcome: CompletionOutcome) -> bool {
        self == Self::Strict || outcome != CompletionOutcome::Done
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionOutcome {
    Done,
    Partial,
    Failed,
}

impl std::fmt::Display for CompletionPolicy {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
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
    pub note: Option<TaskNote>,
    /// Bytes occupied by the line excluding its newline.
    pub line_range: Range<usize>,
    /// Bytes occupied by the complete line including its newline when present.
    pub full_line_range: Range<usize>,
    pub checkbox_range: Range<usize>,
    pub body_range: Range<usize>,
    pub metadata_range: Range<usize>,
}

impl Task {
    /// Partial completion percentage, when the task carries a `◐` token.
    pub fn progress(&self) -> Option<u8> {
        self.metadata.iter().find_map(|token| match token.kind {
            TokenKind::Progress(value) => Some(value),
            _ => None,
        })
    }

    /// Date of the last failed attempt, when the task carries a `⚠` token.
    pub fn failure(&self) -> Option<&str> {
        self.metadata.iter().find_map(|token| match &token.kind {
            TokenKind::Failure(date) => Some(date.as_str()),
            _ => None,
        })
    }

    /// True when work has started but the task is neither complete nor merely queued.
    ///
    /// A failure alone counts: an attempt that failed is work in flight, even when
    /// no percentage was recorded.
    pub fn partial(&self) -> bool {
        !self.completed && (self.progress().is_some() || self.failure().is_some())
    }

    pub fn evidence(&self) -> Option<(&str, &str)> {
        self.metadata.iter().find_map(|token| match &token.kind {
            TokenKind::Evidence { from, to } => Some((from.as_str(), to.as_str())),
            _ => None,
        })
    }
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
    Evidence {
        from: String,
        to: String,
    },
    Dependency(String),
    Reference {
        path: String,
        id: String,
    },
    /// `◐<0-100>` — partial completion percentage. Its presence means the task is
    /// underway; it never implies completion (spec §6.5.4 keeps automatic reconciliation monotonic).
    Progress(u8),
    /// `⚠<YYYY-MM-DD>` — the date an attempt on this task last failed. Independent
    /// of progress: a task can be 80% done and still carry an unresolved failure.
    Failure(String),
    Unknown,
}

/// A `↳ <text> <!--n:ID-->` line directly beneath a task line.
///
/// The note is a lazy paragraph continuation of the task's list item, so every
/// CommonMark renderer folds it into that item and none of them turn it into an
/// indented code block. It carries the prose a line-tail token cannot hold: what
/// got finished, what is left, and why an attempt failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskNote {
    pub text: String,
    pub indent: String,
    /// Bytes occupied by the line excluding its newline.
    pub line_range: Range<usize>,
    /// Bytes occupied by the complete line including its newline when present.
    pub full_line_range: Range<usize>,
}
