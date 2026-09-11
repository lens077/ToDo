//! AI feed: the current state of agent sessions published by an external host
//! (DeepSeek Harness first). Contract: `docs/ai-feed-spec.md`.
//!
//! Records are not Project Tasks. They live in a global JSON store under
//! `~/.todo/ai/`, never in `TODO.md`, never in Git, never in `.todo/history/`.
//! The host only ever calls `todo ai upsert`; the desktop and `todo serve` read
//! this store and write the local marks (`acked_at`, `hidden`, `task_id`).

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use serde::{Deserialize, Serialize};

use crate::{desktop::Clock, Error, LockedWriter, Result};

/// Contract version this build reads and writes.
pub const AI_FEED_SPEC: u32 = 1;
/// Records kept per workspace; older ones by `event_at` are dropped on write.
pub const AI_KEEP_PER_WORKSPACE: usize = 200;
/// Default age (days) after which handled or hidden records are pruned.
pub const AI_PRUNE_DAYS: u64 = 30;

pub const AI_MAX_SOURCE_BYTES: usize = 32;
pub const AI_MAX_SESSION_BYTES: usize = 128;
pub const AI_MAX_TITLE_BYTES: usize = 200;
pub const AI_MAX_QUESTION_BYTES: usize = 2000;
pub const AI_MAX_REPLY_BYTES: usize = 4000;
pub const AI_MAX_CHANGED_FILES: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiState {
    Running,
    Waiting,
    Unread,
    Seen,
    Failed,
    Handled,
}

impl AiState {
    pub const ALL: [AiState; 6] = [
        AiState::Waiting,
        AiState::Failed,
        AiState::Unread,
        AiState::Running,
        AiState::Seen,
        AiState::Handled,
    ];

    /// Display order from the contract: what needs the user first.
    pub fn rank(self) -> u8 {
        match self {
            AiState::Waiting => 0,
            AiState::Failed => 1,
            AiState::Unread => 2,
            AiState::Running => 3,
            AiState::Seen => 4,
            AiState::Handled => 5,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            AiState::Running => "running",
            AiState::Waiting => "waiting",
            AiState::Unread => "unread",
            AiState::Seen => "seen",
            AiState::Failed => "failed",
            AiState::Handled => "handled",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        AiState::ALL
            .into_iter()
            .find(|state| state.as_str() == value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiOutcome {
    Completed,
    Error,
    Aborted,
    Blocked,
    MaxTokens,
    Interrupted,
}

impl AiOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            AiOutcome::Completed => "completed",
            AiOutcome::Error => "error",
            AiOutcome::Aborted => "aborted",
            AiOutcome::Blocked => "blocked",
            AiOutcome::MaxTokens => "max-tokens",
            AiOutcome::Interrupted => "interrupted",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "completed" => Some(AiOutcome::Completed),
            "error" => Some(AiOutcome::Error),
            "aborted" => Some(AiOutcome::Aborted),
            "blocked" => Some(AiOutcome::Blocked),
            "max-tokens" => Some(AiOutcome::MaxTokens),
            "interrupted" => Some(AiOutcome::Interrupted),
            _ => None,
        }
    }
}

/// What a host sends. Mirrors `docs/ai-feed-spec.md` §1.1; unknown top-level
/// fields are ignored so a host can extend ahead of this build.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiUpsert {
    #[serde(rename = "ai-feed-spec")]
    pub spec: u32,
    pub source: String,
    pub session_id: String,
    pub workspace: String,
    pub state: AiState,
    #[serde(default)]
    pub outcome: Option<AiOutcome>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub question: Option<String>,
    #[serde(default)]
    pub reply: Option<String>,
    #[serde(default)]
    pub reply_truncated: bool,
    #[serde(default)]
    pub changed_files: Vec<String>,
    #[serde(default)]
    pub changed_file_count: Option<u32>,
    #[serde(default)]
    pub open_url: Option<String>,
    #[serde(default)]
    pub event_at: Option<i64>,
}

/// One stored record: the host fields plus ToDo's local marks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiRecord {
    pub source: String,
    pub session_id: String,
    pub workspace: String,
    pub state: AiState,
    pub outcome: Option<AiOutcome>,
    pub title: Option<String>,
    pub question: Option<String>,
    pub reply: Option<String>,
    pub reply_truncated: bool,
    pub changed_files: Vec<String>,
    pub changed_file_count: u32,
    pub open_url: Option<String>,
    pub event_at: i64,
    pub received_at: i64,
    pub acked_at: Option<i64>,
    pub hidden: bool,
    pub task_id: Option<String>,
}

impl AiRecord {
    pub fn key(&self) -> (&str, &str) {
        (&self.source, &self.session_id)
    }

    /// Whether the record belongs to `workspace` or a directory under it.
    pub fn in_workspace(&self, workspace: &Path) -> bool {
        Path::new(&self.workspace).starts_with(workspace)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiUpsertOutcome {
    pub record: AiRecord,
    /// The stored record's `event_at` was newer; nothing was written.
    pub unchanged: bool,
}

#[derive(Debug, Clone, Default)]
pub struct AiListFilter {
    /// Keep records whose `workspace` equals or lies under this path.
    pub workspace: Option<PathBuf>,
    /// Keep only these states. Empty means every state.
    pub states: Vec<AiState>,
    /// Include `handled` and `hidden` records.
    pub include_all: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiPruneOutcome {
    pub removed: usize,
    pub remaining: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AiFile {
    #[serde(rename = "ai-feed-spec")]
    spec: u32,
    sessions: Vec<AiRecord>,
}

#[derive(Debug, Clone)]
pub struct AiStore {
    path: PathBuf,
    lock_timeout: Duration,
    clock: Arc<dyn Clock>,
}

impl AiStore {
    pub fn new(path: impl Into<PathBuf>, lock_timeout: Duration) -> Self {
        Self::with_clock(path, lock_timeout, Arc::new(crate::desktop::SystemClock))
    }

    pub fn with_clock(
        path: impl Into<PathBuf>,
        lock_timeout: Duration,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            path: path.into(),
            lock_timeout,
            clock,
        }
    }

    pub fn global(lock_timeout: Duration) -> Result<Self> {
        Ok(Self::new(global_ai_path()?, lock_timeout))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Every stored record, in contract display order.
    pub fn list(&self, filter: &AiListFilter) -> Result<Vec<AiRecord>> {
        let mut records = read_ai_file(&self.path)?.sessions;
        records.retain(|record| {
            if let Some(workspace) = &filter.workspace {
                if !record.in_workspace(workspace) {
                    return false;
                }
            }
            if !filter.include_all && (record.hidden || record.state == AiState::Handled) {
                return false;
            }
            filter.states.is_empty() || filter.states.contains(&record.state)
        });
        sort_records(&mut records);
        Ok(records)
    }

    pub fn get(&self, source: &str, session_id: &str) -> Result<AiRecord> {
        read_ai_file(&self.path)?
            .sessions
            .into_iter()
            .find(|record| record.source == source && record.session_id == session_id)
            .ok_or_else(|| Error::AiSessionNotFound(format!("{source}/{session_id}")))
    }

    /// The host's fast path. Validates the contract, applies the `event_at`
    /// rule, keeps local marks, and trims per-workspace retention.
    pub fn upsert(&self, input: AiUpsert) -> Result<AiUpsertOutcome> {
        let now = self.now_ms();
        let incoming = normalize(input, now)?;
        self.update(|records| {
            let position = records
                .iter()
                .position(|record| record.key() == incoming.key());
            let Some(index) = position else {
                records.push(incoming.clone());
                trim_workspace(records, &incoming.workspace);
                return Ok((
                    AiUpsertOutcome {
                        record: incoming,
                        unchanged: false,
                    },
                    true,
                ));
            };
            let existing = &records[index];
            if incoming.event_at < existing.event_at {
                return Ok((
                    AiUpsertOutcome {
                        record: existing.clone(),
                        unchanged: true,
                    },
                    false,
                ));
            }
            let mut merged = incoming;
            merged.hidden = existing.hidden;
            merged.task_id = existing.task_id.clone();
            // Same instant is a field correction, not new activity: keep the ack.
            merged.acked_at = if merged.event_at == existing.event_at {
                existing.acked_at
            } else {
                None
            };
            records[index] = merged.clone();
            Ok((
                AiUpsertOutcome {
                    record: merged,
                    unchanged: false,
                },
                true,
            ))
        })
    }

    pub fn ack(&self, source: &str, session_id: &str) -> Result<AiRecord> {
        let now = self.now_ms();
        self.modify(source, session_id, |record| {
            record.acked_at = Some(now);
        })
    }

    pub fn remove(&self, source: &str, session_id: &str) -> Result<AiRecord> {
        self.modify(source, session_id, |record| {
            record.hidden = true;
        })
    }

    pub fn link(&self, source: &str, session_id: &str, task_id: &str) -> Result<AiRecord> {
        let task_id = task_id.trim();
        if task_id.is_empty() {
            return Err(Error::InvalidTaskId(task_id.into()));
        }
        let task_id = task_id.to_string();
        self.modify(source, session_id, move |record| {
            record.task_id = Some(task_id);
        })
    }

    /// Drops handled or hidden records older than `older_than_days`.
    pub fn prune(&self, older_than_days: u64) -> Result<AiPruneOutcome> {
        let cutoff = self.now_ms() - (older_than_days as i64).saturating_mul(86_400_000);
        self.update(|records| {
            let before = records.len();
            records.retain(|record| {
                let stale = record.hidden || record.state == AiState::Handled;
                !(stale && record.event_at < cutoff)
            });
            let removed = before - records.len();
            Ok((
                AiPruneOutcome {
                    removed,
                    remaining: records.len(),
                },
                removed > 0,
            ))
        })
    }

    fn modify<F>(&self, source: &str, session_id: &str, change: F) -> Result<AiRecord>
    where
        F: FnOnce(&mut AiRecord),
    {
        self.update(|records| {
            let record = records
                .iter_mut()
                .find(|record| record.source == source && record.session_id == session_id)
                .ok_or_else(|| Error::AiSessionNotFound(format!("{source}/{session_id}")))?;
            change(record);
            Ok((record.clone(), true))
        })
    }

    /// Locks, reads, applies `change`, and writes back only when it reports a change.
    fn update<T, F>(&self, change: F) -> Result<T>
    where
        F: FnOnce(&mut Vec<AiRecord>) -> Result<(T, bool)>,
    {
        self.ensure_parent()?;
        let writer = LockedWriter::new(&self.path, self.lock_timeout);
        let _guard = writer.lock()?;
        let mut file = read_ai_file(&self.path)?;
        let (result, changed) = change(&mut file.sessions)?;
        if changed {
            let bytes = serde_json::to_vec_pretty(&file).map_err(Error::history_encode)?;
            crate::atomic_write(&self.path, &bytes)?;
        }
        Ok(result)
    }

    fn now_ms(&self) -> i64 {
        self.clock.now_utc().timestamp_millis()
    }

    fn ensure_parent(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        Ok(())
    }
}

pub fn global_ai_path() -> Result<PathBuf> {
    dirs::home_dir()
        .map(|home| home.join(".todo").join("ai").join("sessions.json"))
        .ok_or(Error::HomeDirectoryUnavailable)
}

/// Contract order: state rank, then newest activity first.
pub fn sort_records(records: &mut [AiRecord]) {
    records.sort_by(|a, b| {
        a.state
            .rank()
            .cmp(&b.state.rank())
            .then_with(|| b.event_at.cmp(&a.event_at))
            .then_with(|| a.key().cmp(&b.key()))
    });
}

fn normalize(input: AiUpsert, now: i64) -> Result<AiRecord> {
    if input.spec != AI_FEED_SPEC {
        return Err(Error::InvalidAiFeed(format!(
            "unsupported ai-feed-spec {}; this build accepts {AI_FEED_SPEC}",
            input.spec
        )));
    }
    let source = input.source.trim();
    if source.is_empty()
        || source.len() > AI_MAX_SOURCE_BYTES
        || !source
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(Error::InvalidAiFeed(
            "source must be 1-32 bytes of lowercase letters, digits, or `-`".into(),
        ));
    }
    let session_id = input.session_id.trim();
    if session_id.is_empty() || session_id.len() > AI_MAX_SESSION_BYTES {
        return Err(Error::InvalidAiFeed(
            "session_id must be 1-128 bytes".into(),
        ));
    }
    let workspace = input.workspace.trim();
    if workspace.is_empty() || !Path::new(workspace).is_absolute() {
        return Err(Error::InvalidAiFeed(
            "workspace must be an absolute path".into(),
        ));
    }
    if let Some(url) = input.open_url.as_deref() {
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(Error::InvalidAiFeed(
                "open_url must start with http:// or https://".into(),
            ));
        }
    }
    let event_at = input.event_at.unwrap_or(now);
    if event_at < 0 {
        return Err(Error::InvalidAiFeed("event_at must be non-negative".into()));
    }
    let mut changed_files = input.changed_files;
    let listed = changed_files.len();
    changed_files.truncate(AI_MAX_CHANGED_FILES);
    let changed_file_count = input
        .changed_file_count
        .unwrap_or(listed as u32)
        .max(changed_files.len() as u32);
    Ok(AiRecord {
        source: source.to_string(),
        session_id: session_id.to_string(),
        workspace: workspace.to_string(),
        state: input.state,
        outcome: input.outcome,
        title: input.title.map(|s| truncate_bytes(s, AI_MAX_TITLE_BYTES)),
        question: input
            .question
            .map(|s| truncate_bytes(s, AI_MAX_QUESTION_BYTES)),
        reply: input.reply.map(|s| truncate_bytes(s, AI_MAX_REPLY_BYTES)),
        reply_truncated: input.reply_truncated,
        changed_files,
        changed_file_count,
        open_url: input.open_url,
        event_at,
        received_at: now,
        acked_at: None,
        hidden: false,
        task_id: None,
    })
}

/// Cuts on a char boundary so a truncated field stays valid UTF-8.
fn truncate_bytes(mut text: String, max: usize) -> String {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text
}

/// Keeps the newest `AI_KEEP_PER_WORKSPACE` records for `workspace`.
fn trim_workspace(records: &mut Vec<AiRecord>, workspace: &str) {
    let mut indices: Vec<usize> = records
        .iter()
        .enumerate()
        .filter(|(_, record)| record.workspace == workspace)
        .map(|(index, _)| index)
        .collect();
    if indices.len() <= AI_KEEP_PER_WORKSPACE {
        return;
    }
    indices.sort_by_key(|&index| records[index].event_at);
    let excess = indices.len() - AI_KEEP_PER_WORKSPACE;
    let mut doomed: Vec<usize> = indices.into_iter().take(excess).collect();
    doomed.sort_unstable_by(|a, b| b.cmp(a));
    for index in doomed {
        records.remove(index);
    }
}

fn read_ai_file(path: &Path) -> Result<AiFile> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            return Ok(AiFile {
                spec: AI_FEED_SPEC,
                sessions: Vec::new(),
            })
        }
        Err(source) => {
            return Err(Error::Io {
                path: path.to_path_buf(),
                source,
            })
        }
    };
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(AiFile {
            spec: AI_FEED_SPEC,
            sessions: Vec::new(),
        });
    }
    let file: AiFile = serde_json::from_slice(&bytes).map_err(|error| Error::InvalidAiFile {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    if file.spec != AI_FEED_SPEC {
        return Err(Error::InvalidAiFile {
            path: path.to_path_buf(),
            message: format!("unsupported ai-feed-spec {}", file.spec),
        });
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, NaiveDate, TimeZone, Utc};
    use std::sync::Mutex;

    #[derive(Debug)]
    struct FixedClock(Mutex<i64>);

    impl Clock for FixedClock {
        fn local_today(&self) -> NaiveDate {
            self.now_utc().date_naive()
        }
        fn now_utc(&self) -> DateTime<Utc> {
            Utc.timestamp_millis_opt(*self.0.lock().unwrap()).unwrap()
        }
    }

    fn store(dir: &tempfile::TempDir, now: i64) -> (AiStore, Arc<FixedClock>) {
        let clock = Arc::new(FixedClock(Mutex::new(now)));
        let store = AiStore::with_clock(
            dir.path().join("ai").join("sessions.json"),
            Duration::from_millis(500),
            clock.clone(),
        );
        (store, clock)
    }

    fn upsert(session: &str, state: AiState, event_at: i64) -> AiUpsert {
        AiUpsert {
            spec: AI_FEED_SPEC,
            source: "dsh".into(),
            session_id: session.into(),
            workspace: "/work/project".into(),
            state,
            outcome: None,
            title: Some("t".into()),
            question: None,
            reply: None,
            reply_truncated: false,
            changed_files: vec![],
            changed_file_count: None,
            open_url: None,
            event_at: Some(event_at),
        }
    }

    #[test]
    fn upsert_creates_then_replaces_and_clears_ack_on_newer_event() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = store(&dir, 1_000);
        let first = store.upsert(upsert("s1", AiState::Running, 100)).unwrap();
        assert!(!first.unchanged);
        assert_eq!(first.record.received_at, 1_000);

        store.ack("dsh", "s1").unwrap();
        store.link("dsh", "s1", "kq3n8w").unwrap();
        assert!(store.get("dsh", "s1").unwrap().acked_at.is_some());

        let second = store.upsert(upsert("s1", AiState::Unread, 200)).unwrap();
        assert!(!second.unchanged);
        assert_eq!(second.record.state, AiState::Unread);
        assert_eq!(
            second.record.acked_at, None,
            "newer activity clears the ack"
        );
        assert_eq!(second.record.task_id.as_deref(), Some("kq3n8w"));
    }

    #[test]
    fn stale_event_is_ignored_and_equal_event_keeps_ack() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = store(&dir, 1_000);
        store.upsert(upsert("s1", AiState::Unread, 200)).unwrap();
        store.ack("dsh", "s1").unwrap();

        let stale = store.upsert(upsert("s1", AiState::Running, 100)).unwrap();
        assert!(stale.unchanged);
        assert_eq!(stale.record.state, AiState::Unread);

        let same = store.upsert(upsert("s1", AiState::Seen, 200)).unwrap();
        assert!(!same.unchanged);
        assert_eq!(same.record.state, AiState::Seen);
        assert!(same.record.acked_at.is_some());
    }

    #[test]
    fn hidden_survives_upsert_and_list_hides_by_default() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = store(&dir, 1_000);
        store.upsert(upsert("s1", AiState::Unread, 100)).unwrap();
        store.upsert(upsert("s2", AiState::Handled, 100)).unwrap();
        store.remove("dsh", "s1").unwrap();
        store.upsert(upsert("s1", AiState::Failed, 300)).unwrap();

        let visible = store.list(&AiListFilter::default()).unwrap();
        assert!(visible.is_empty());
        let all = store
            .list(&AiListFilter {
                include_all: true,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(all.len(), 2);
        assert!(all.iter().any(|r| r.session_id == "s1" && r.hidden));
    }

    #[test]
    fn list_orders_by_state_rank_then_newest_and_filters_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = store(&dir, 1_000);
        store.upsert(upsert("a", AiState::Running, 500)).unwrap();
        store.upsert(upsert("b", AiState::Unread, 100)).unwrap();
        store.upsert(upsert("c", AiState::Unread, 300)).unwrap();
        store.upsert(upsert("d", AiState::Waiting, 50)).unwrap();
        let mut other = upsert("e", AiState::Waiting, 999);
        other.workspace = "/work/other".into();
        store.upsert(other).unwrap();
        let mut nested = upsert("f", AiState::Seen, 10);
        nested.workspace = "/work/project/sub".into();
        store.upsert(nested).unwrap();

        let ids: Vec<String> = store
            .list(&AiListFilter {
                workspace: Some(PathBuf::from("/work/project")),
                ..Default::default()
            })
            .unwrap()
            .into_iter()
            .map(|r| r.session_id)
            .collect();
        assert_eq!(ids, ["d", "c", "b", "a", "f"]);

        let unread_only = store
            .list(&AiListFilter {
                states: vec![AiState::Unread],
                ..Default::default()
            })
            .unwrap();
        assert_eq!(unread_only.len(), 2);
    }

    #[test]
    fn prune_removes_old_handled_and_hidden_only() {
        let dir = tempfile::tempdir().unwrap();
        let day = 86_400_000;
        let (store, _) = store(&dir, 100 * day);
        store
            .upsert(upsert("old-handled", AiState::Handled, 10 * day))
            .unwrap();
        store
            .upsert(upsert("new-handled", AiState::Handled, 95 * day))
            .unwrap();
        store
            .upsert(upsert("old-open", AiState::Unread, 10 * day))
            .unwrap();
        store
            .upsert(upsert("old-hidden", AiState::Unread, 10 * day))
            .unwrap();
        store.remove("dsh", "old-hidden").unwrap();

        let outcome = store.prune(AI_PRUNE_DAYS).unwrap();
        assert_eq!(
            outcome,
            AiPruneOutcome {
                removed: 2,
                remaining: 2
            }
        );
        assert!(store.get("dsh", "old-open").is_ok());
        assert!(store.get("dsh", "new-handled").is_ok());
        assert!(store.get("dsh", "old-handled").is_err());
    }

    #[test]
    fn retention_keeps_newest_per_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = store(&dir, 1_000);
        for i in 0..(AI_KEEP_PER_WORKSPACE as i64 + 5) {
            store
                .upsert(upsert(&format!("s{i}"), AiState::Seen, i))
                .unwrap();
        }
        let all = store
            .list(&AiListFilter {
                include_all: true,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(all.len(), AI_KEEP_PER_WORKSPACE);
        assert!(all.iter().all(|r| r.event_at >= 5));
    }

    #[test]
    fn contract_violations_are_rejected_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = store(&dir, 1_000);
        let mut bad_spec = upsert("s", AiState::Running, 1);
        bad_spec.spec = 2;
        assert!(matches!(
            store.upsert(bad_spec),
            Err(Error::InvalidAiFeed(_))
        ));
        let mut bad_source = upsert("s", AiState::Running, 1);
        bad_source.source = "DSH!".into();
        assert!(matches!(
            store.upsert(bad_source),
            Err(Error::InvalidAiFeed(_))
        ));
        let mut bad_ws = upsert("s", AiState::Running, 1);
        bad_ws.workspace = "relative/path".into();
        assert!(matches!(store.upsert(bad_ws), Err(Error::InvalidAiFeed(_))));
        let mut bad_url = upsert("s", AiState::Running, 1);
        bad_url.open_url = Some("javascript:alert(1)".into());
        assert!(matches!(
            store.upsert(bad_url),
            Err(Error::InvalidAiFeed(_))
        ));
        assert!(!store.path().exists());
    }

    #[test]
    fn long_fields_are_truncated_on_char_boundaries() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = store(&dir, 1_000);
        let mut input = upsert("s", AiState::Running, 1);
        input.title = Some("汉".repeat(200));
        input.changed_files = (0..40).map(|i| format!("f{i}")).collect();
        let record = store.upsert(input).unwrap().record;
        assert!(record.title.as_ref().unwrap().len() <= AI_MAX_TITLE_BYTES);
        assert_eq!(record.changed_files.len(), AI_MAX_CHANGED_FILES);
        assert_eq!(record.changed_file_count, 40);
    }

    #[test]
    fn unknown_top_level_fields_are_ignored_and_json_round_trips() {
        let json = r#"{"ai-feed-spec":1,"source":"dsh","session_id":"x","workspace":"/w",
            "state":"failed","outcome":"max-tokens","future_field":{"a":1},"event_at":5}"#;
        let input: AiUpsert = serde_json::from_str(json).unwrap();
        assert_eq!(input.outcome, Some(AiOutcome::MaxTokens));
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = store(&dir, 1_000);
        let record = store.upsert(input).unwrap().record;
        let text = serde_json::to_string(&record).unwrap();
        assert!(text.contains("\"state\":\"failed\""));
        assert!(text.contains("\"outcome\":\"max-tokens\""));
        let back: AiRecord = serde_json::from_str(&text).unwrap();
        assert_eq!(back, record);
    }

    #[test]
    fn corrupt_file_is_reported_not_silently_reset() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = store(&dir, 1_000);
        fs::create_dir_all(store.path().parent().unwrap()).unwrap();
        fs::write(store.path(), b"{not json").unwrap();
        assert!(matches!(
            store.list(&AiListFilter::default()),
            Err(Error::InvalidAiFile { .. })
        ));
    }
}
