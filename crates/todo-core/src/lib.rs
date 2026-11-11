//! Lossless parser and surgical writer for TODO.md format v1.
//! Parsed values are a view over the original bytes; writes never serialize Markdown.

mod adoption;
mod ai;
mod column_setup;
mod contract;
mod desktop;
mod error;
mod heading_priority;
mod history;
mod id;
mod knowledge;
mod llm;
mod lock;
mod model;
mod overview;
mod parse;
mod profile;
mod project;
mod projection;
mod prompt;
mod reading;
mod reconcile;
mod scan;
mod session;
mod tls;
mod workspace;
mod write;

pub use adoption::{ensure_unmodified, plan_adoption, AdoptPlan};
pub use ai::*;
pub use column_setup::{
    ColumnHeadingOption, ColumnHeadingSelection, ColumnSetupOptions, ColumnSetupPreview,
    ColumnSetupSelection, PlannedColumn,
};
pub use contract::{
    check_contract, install_agent_rules, load_contract, path_is_safe_relative,
    uninstall_agent_rules, ContractCheckReport, ContractFinding, DocumentRef, DocumentRole,
    Enforcement, InstallOutcome, Policy, ProjectContract, CONTRACT_SCHEMA_VERSION, MANAGED_RULES,
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
pub use knowledge::{
    render_markdown, KnowledgeDocument, KnowledgeDocumentContent, KnowledgeScanReport,
    KnowledgeSearchHit, LOGICAL_TYPES,
};
pub use llm::{LlmConfig, LlmProvider, LlmSemanticReconciler};
pub use lock::{FileLockGuard, LockedWriter};
pub use model::*;
pub use overview::{OverviewReport, ProjectOverview};
pub use parse::{parse, parse_file};
pub use profile::{
    scan_environment, EnvironmentProfile, ObservedCommand, ObservedEnvironment, ProjectProfile,
    ScanSummary,
};
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
    ReadingContext, ReadingContextInput, ReadingLink, ReadingStore,
};
pub use reconcile::*;
pub use scan::{import_text, scan, CodeComment, LooseCheckbox, ScanOptions, ScanReport, ScanStats};
pub use session::{
    begin as session_begin, docs_validate, docs_validate_blocking, finish as session_finish,
    interrupt as session_interrupt, list as session_list, resume as session_resume, DocsFinding,
    DocsSeverity, SessionFinishInput, SessionResume, SessionSummary, ValidationEntry,
};
pub use tls::http_client_builder;
pub use workspace::{AddedProject, ProjectSummary, WorkspaceStore, WorkspaceSummary};
pub use write::{apply_changes, atomic_write, Change, Edit, InsertPosition, OptimisticLock};
