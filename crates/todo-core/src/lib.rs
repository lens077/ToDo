//! Lossless parser and surgical writer for TODO.md format v1.
//! Parsed values are a view over the original bytes; writes never serialize Markdown.

mod desktop;
mod error;
mod id;
mod lock;
mod model;
mod parse;
mod projection;
mod reconcile;
mod write;

pub use desktop::*;
pub use error::{Error, Result};
pub use id::{generate_task_id, DEFAULT_ID_LENGTH};
pub use lock::{FileLockGuard, LockedWriter};
pub use model::*;
pub use parse::{parse, parse_file};
pub use projection::{ProjectedTask, Projection, ProjectionSnapshot, UpdateStats, SCHEMA_HASH};
pub use reconcile::*;
pub use write::{apply_changes, atomic_write, Change, Edit, InsertPosition, OptimisticLock};
