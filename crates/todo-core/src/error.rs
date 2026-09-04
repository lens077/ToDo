use std::{io, path::PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("file is not valid UTF-8")]
    InvalidUtf8,
    #[error("git conflict marker found on line {line}; file is read-only")]
    ConflictMarker { line: usize },
    #[error("not a TODO.md task file: missing frontmatter with todo-spec: 1")]
    NotTaskFile,
    /// The desktop backend has no project open. Distinct from `NotTaskFile` so
    /// a fresh launch is never reported as a broken file.
    #[error("no project is open")]
    NoProject,
    #[error("unsupported todo-spec version {version}; upgrade required")]
    UnsupportedVersion { version: String },
    #[error("invalid frontmatter: {0}")]
    InvalidFrontmatter(String),
    #[error("task not found: {0}")]
    TaskNotFound(String),
    #[error("task has no stable ID; select it by index or assign an ID first")]
    MissingTaskId,
    #[error("column not found: {0}")]
    ColumnNotFound(String),
    #[error("invalid task ID `{0}`; expected at least 6 lowercase base36 characters")]
    InvalidTaskId(String),
    #[error("duplicate task ID: {0}")]
    DuplicateTaskId(String),
    #[error("edit ranges overlap")]
    OverlappingEdits,
    #[error("invalid edit range")]
    InvalidRange,
    #[error("optimistic lock failed: file changed since it was read; rebase required")]
    ConcurrentModification,
    #[error(
        "write validation failed: changed lines {actual:?} exceeded expected lines {expected:?}"
    )]
    UnexpectedLineChanges {
        expected: Vec<usize>,
        actual: Vec<usize>,
    },
    #[error("operation would create an invalid task line: {0}")]
    InvalidTaskLine(String),
    #[error("SQLite projection error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("timed out after {timeout_ms}ms waiting for advisory lock {path}")]
    LockTimeout { path: PathBuf, timeout_ms: u64 },
    #[error("{path} already exists; use `todo init --adopt` to explicitly adopt it")]
    FileAlreadyExists { path: PathBuf },
    #[error("operating system random source failed: {0}")]
    RandomSource(String),
    #[error("invalid configuration {path}: {message}")]
    InvalidConfiguration { path: PathBuf, message: String },
    #[error("capture inbox {path} is corrupt: {message}")]
    InvalidCaptureFile { path: PathBuf, message: String },
    #[error("could not resolve the current user's home directory")]
    HomeDirectoryUnavailable,
    #[error("history version {0} not found")]
    HistoryVersionNotFound(u64),
    #[error("could not encode history entry: {0}")]
    HistoryEncode(String),
    #[error("invalid alignment response: {0}")]
    InvalidAlignment(String),
    #[error("invalid LLM configuration: {0}")]
    InvalidLlmConfiguration(String),
}

impl Error {
    pub(crate) fn history_encode(source: serde_json::Error) -> Self {
        Self::HistoryEncode(source.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn io_error(path: impl Into<PathBuf>, source: io::Error) -> Error {
    Error::Io {
        path: path.into(),
        source,
    }
}
