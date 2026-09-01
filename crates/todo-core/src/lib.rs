//! Lossless parser and surgical writer for TODO.md format v1.
//! Parsed values are a view over the original bytes; writes never serialize Markdown.

mod desktop;
mod error;
mod history;
mod id;
mod lock;
mod model;
mod parse;
mod projection;
mod prompt;
mod reconcile;
mod scan;
mod write;

pub use desktop::*;
pub use error::{Error, Result};
pub use history::{
    diff_document, diff_lines, DiffHunk, DiffKind, History, HistoryEntry, TextDiff, DEFAULT_KEEP,
    STATE_DIR,
};
pub use id::{generate_task_id, DEFAULT_ID_LENGTH};
pub use lock::{FileLockGuard, LockedWriter};
pub use model::*;
pub use parse::{parse, parse_file};
pub use projection::{
    project_document, ProjectedTask, Projection, ProjectionSnapshot, UpdateStats, SCHEMA_HASH,
};
pub use prompt::{
    review_alignment, AlignmentResponse, AlignmentReview, AlignmentVerdict, Confidence,
    RejectedVerdict, Verdict, ALIGNMENT_PROMPT,
};
pub use reconcile::*;
pub use scan::{import_text, scan, CodeComment, LooseCheckbox, ScanOptions, ScanReport, ScanStats};
pub use write::{apply_changes, atomic_write, Change, Edit, InsertPosition, OptimisticLock};
