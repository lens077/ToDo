//! Lossless parser and surgical writer for TODO.md format v1.
//! Parsed values are a view over the original bytes; writes never serialize Markdown.

mod ai;
mod column_setup;
mod contract;
mod desktop;
mod error;
mod heading_priority;
mod history;
mod id;
mod llm;
mod lock;
mod model;
mod parse;
mod project;
mod projection;
mod prompt;
mod reading;
mod reconcile;
mod scan;
mod write;

pub use ai::*;
pub use column_setup::{
    ColumnHeadingOption, ColumnHeadingSelection, ColumnSetupOptions, ColumnSetupPreview,
    ColumnSetupSelection, PlannedColumn,
};
pub use contract::{
    check_contract, load_contract, ContractCheckReport, ContractFinding, DocumentRef, DocumentRole,
    Enforcement, Policy, ProjectContract, CONTRACT_SCHEMA_VERSION,
};
pub use desktop::*;
pub use error::{Error, Result};
pub use heading_priority::{
    effective_priorities, heading_priorities, parse_priority_label, EffectivePriority,
    HeadingPriority, PriorityLabel, PriorityScale, PrioritySource,
};
pub use history::{
    diff_document, diff_lines, DiffHunk, DiffKind, History, HistoryEntry, TextDiff, DEFAULT_KEEP,
    STATE_DIR,
};
pub use id::{generate_task_id, DEFAULT_ID_LENGTH};
pub use llm::{LlmConfig, LlmProvider, LlmSemanticReconciler};
pub use lock::{FileLockGuard, LockedWriter};
pub use model::*;
pub use parse::{parse, parse_file};
pub use project::*;
pub use projection::{
    project_document, ProjectedTask, Projection, ProjectionSnapshot, UpdateStats, SCHEMA_HASH,
};
pub use prompt::{
    review_alignment, AlignmentResponse, AlignmentReview, AlignmentVerdict, Confidence,
    RejectedVerdict, Verdict, ALIGNMENT_PROMPT,
};
pub use reading::{
    reading_stats, ReadingAnnotation, ReadingAnnotationInput, ReadingArticle, ReadingArticleDetail,
    ReadingStore,
};
pub use reconcile::*;
pub use scan::{import_text, scan, CodeComment, LooseCheckbox, ScanOptions, ScanReport, ScanStats};
pub use write::{apply_changes, atomic_write, Change, Edit, InsertPosition, OptimisticLock};
