use crate::{
    generate_task_id, parse_file, project_document, Change, CompletionOutcome, CompletionPolicy,
    Document, Error, InsertPosition, LockedWriter, ProjectedTask, Result, TokenKind,
};
use chrono::{DateTime, Days, Local, NaiveDate, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

pub trait Clock: Debug + Send + Sync {
    fn local_today(&self) -> NaiveDate;
    fn now_utc(&self) -> DateTime<Utc>;
}

#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn local_today(&self) -> NaiveDate {
        Local::now().date_naive()
    }

    fn now_utc(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopSnapshot {
    pub project_path: String,
    pub revision: String,
    pub completion_policy: CompletionPolicy,
    pub columns: Vec<DesktopColumn>,
    pub tasks: Vec<DesktopTask>,
    pub today: TodayView,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopColumn {
    pub id: String,
    pub name: String,
    pub done: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopTask {
    pub id: Option<String>,
    pub body: String,
    pub completed: bool,
    pub column: Option<String>,
    pub indent_level: usize,
    pub parent_id: Option<String>,
    pub due: Option<String>,
    pub priority: Option<u8>,
    pub tags: Vec<String>,
    /// Partial completion percentage, when the task carries a `◐` token.
    pub progress: Option<u8>,
    /// Date of the last failed attempt, when the task carries a `⚠` token.
    pub failed_on: Option<String>,
    /// Text of the `↳` note line, when present.
    pub note: Option<String>,
    pub evidence: Option<String>,
    pub evidence_availability: EvidenceAvailability,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum EvidenceAvailability {
    Available { from: String, to: String },
    Unavailable { reason: String },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TodayView {
    pub overdue: Vec<DesktopTask>,
    pub today: Vec<DesktopTask>,
    pub upcoming: Vec<DesktopTask>,
    /// Actionable work carrying no due date, ordered by priority.
    ///
    /// Without this band a P1 task with no date appears nowhere on the landing
    /// view, so a page that claims to answer "what should I do now" hides the
    /// highest-priority thing in the project.
    pub next: Vec<DesktopTask>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MoveTarget {
    ColumnStart,
    ColumnEnd,
    Before { task_id: String },
    After { task_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceRange {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PriorityScope {
    #[default]
    Task,
    Subtree,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DesktopIntent {
    AddTask {
        body: String,
        column: Option<String>,
    },
    SetCompleted {
        id: String,
        completed: bool,
        #[serde(default)]
        note: Option<String>,
        #[serde(default)]
        evidence: Option<EvidenceRange>,
    },
    SetPartial {
        id: String,
        percent: u8,
        note: String,
    },
    SetFailed {
        id: String,
        note: String,
    },
    /// Replaces the Markdown text of a task line, leaving checkbox, metadata,
    /// and the `↳` note untouched.
    SetBody {
        id: String,
        body: String,
    },
    SetPriority {
        id: String,
        priority: Option<u8>,
        #[serde(default)]
        scope: PriorityScope,
    },
    MoveTask {
        id: String,
        column: String,
        target: MoveTarget,
        /// Completion details used only when the destination is a done column.
        /// The column determines the checkbox state; callers never set it twice.
        #[serde(default)]
        note: Option<String>,
        #[serde(default)]
        evidence: Option<EvidenceRange>,
    },
}

impl DesktopIntent {
    /// Short operation name recorded in version history.
    pub fn label(&self) -> &'static str {
        match self {
            Self::AddTask { .. } => "board-add",
            Self::SetCompleted {
                completed: true, ..
            } => "board-done",
            Self::SetCompleted { .. } => "board-undone",
            Self::SetPartial { .. } => "board-partial",
            Self::SetFailed { .. } => "board-fail",
            Self::SetBody { .. } => "board-edit",
            Self::SetPriority { .. } => "board-priority",
            Self::MoveTask { .. } => "board-move",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DesktopBackend {
    project: Option<PathBuf>,
    lock_timeout: Duration,
    clock: Arc<dyn Clock>,
    ranker: Arc<dyn TaskRanker + Send + Sync>,
    /// Shared between clones so one long-lived server process pays for a
    /// history scan once per HEAD, not once per request.
    evidence: Arc<Mutex<EvidenceCache>>,
}

impl Default for DesktopBackend {
    fn default() -> Self {
        Self::new(Duration::from_secs(2))
    }
}

impl DesktopBackend {
    pub fn new(lock_timeout: Duration) -> Self {
        Self::with_clock(lock_timeout, Arc::new(SystemClock))
    }

    pub fn with_clock(lock_timeout: Duration, clock: Arc<dyn Clock>) -> Self {
        Self {
            project: None,
            lock_timeout,
            clock,
            ranker: Arc::new(DefaultRanker),
            evidence: Arc::new(Mutex::new(EvidenceCache::default())),
        }
    }

    /// Swaps in an alternative ordering, such as a hosted ranker.
    pub fn with_ranker(mut self, ranker: Arc<dyn TaskRanker + Send + Sync>) -> Self {
        self.ranker = ranker;
        self
    }

    pub fn open_project(&mut self, path: impl AsRef<Path>) -> Result<DesktopSnapshot> {
        let todo = resolve_project(path.as_ref());
        let document = parse_file(&todo)?;
        self.project = Some(todo);
        self.derive(&document)
    }

    pub fn snapshot(&self) -> Result<DesktopSnapshot> {
        let path = self.project.as_ref().ok_or(Error::NoProject)?;
        self.derive(&parse_file(path)?)
    }

    pub(crate) fn snapshot_document(
        &mut self,
        path: &Path,
        document: &Document,
    ) -> Result<DesktopSnapshot> {
        self.project = Some(path.to_path_buf());
        self.derive(document)
    }

    pub fn mutate(&self, intent: DesktopIntent) -> Result<DesktopSnapshot> {
        let (document, _) = self.apply_intent(intent)?;
        self.derive(&document)
    }

    pub fn project_path(&self) -> Option<&Path> {
        self.project.as_deref()
    }

    fn apply_intent(&self, intent: DesktopIntent) -> Result<(Document, Option<String>)> {
        let path = self.project.as_ref().ok_or(Error::NoProject)?;
        let writer = LockedWriter::new(path, self.lock_timeout);
        let mut created_task_id = None;
        // Label the restore point with the desktop action that caused it, so a user
        // scanning history sees "board-move" rather than an anonymous "update".
        let label = intent.label();
        let document = writer.update_labeled(label, |document| {
            intent_changes(
                document,
                &intent,
                &mut created_task_id,
                self.clock.local_today(),
                path,
            )
        })?;
        Ok((document, created_task_id))
    }

    fn derive(&self, document: &Document) -> Result<DesktopSnapshot> {
        let path = self.project.as_ref().ok_or(Error::NoProject)?;
        let head = git_output(
            repository_of(path),
            &["rev-parse", "--verify", "HEAD^{commit}"],
        );
        let lookup = {
            let mut cache = self
                .evidence
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            cache.refresh(path, head.as_deref().ok(), document)
        };
        let mut projected: HashMap<_, _> = project_document(path, document)?
            .into_iter()
            .map(|task| (task.id.clone(), task))
            .collect();
        let tasks: Vec<_> = document
            .tasks
            .iter()
            .map(|task| {
                task.id
                    .as_ref()
                    .and_then(|id| projected.remove(id))
                    .map_or_else(
                        || task_view(head.as_ref(), &lookup, document, task),
                        |projected| projected_task_view(head.as_ref(), &lookup, projected),
                    )
            })
            .collect();
        let today = self.clock.local_today();
        let upcoming_end = today.checked_add_days(Days::new(7)).unwrap_or(today);
        let active: Vec<_> = tasks
            .iter()
            .filter(|task| !task.completed)
            .cloned()
            .collect();
        let mut view = TodayView {
            overdue: active
                .iter()
                .filter(|task| task_due_date(task).is_some_and(|due| due < today))
                .cloned()
                .collect(),
            today: active
                .iter()
                .filter(|task| task_due_date(task) == Some(today))
                .cloned()
                .collect(),
            upcoming: active
                .iter()
                .filter(|task| {
                    task_due_date(task).is_some_and(|due| due > today && due <= upcoming_end)
                })
                .cloned()
                .collect(),
            next: active
                .iter()
                .filter(|task| task_due_date(task).is_none())
                .cloned()
                .collect(),
        };
        // The landing view answers "what should I do now", so each band is ordered
        // by urgency rather than by position in the file.
        for band in [
            &mut view.overdue,
            &mut view.today,
            &mut view.upcoming,
            &mut view.next,
        ] {
            self.ranker.rank(band);
        }
        let hash = document
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.hash)
            .unwrap_or_else(|| hash(document.source.as_bytes()));
        Ok(DesktopSnapshot {
            project_path: path.display().to_string(),
            revision: format!("{hash:016x}"),
            completion_policy: document.completion_policy(),
            columns: document
                .columns
                .iter()
                .map(|column| DesktopColumn {
                    id: column.id.clone(),
                    name: column.name.clone(),
                    done: column.kind.as_deref() == Some("done"),
                })
                .collect(),
            tasks,
            today: view,
        })
    }
}

fn resolve_project(path: &Path) -> PathBuf {
    let todo = if path.is_dir() {
        path.join("TODO.md")
    } else {
        path.to_path_buf()
    };
    std::fs::canonicalize(&todo).unwrap_or(todo)
}

fn intent_changes(
    document: &Document,
    intent: &DesktopIntent,
    created_task_id: &mut Option<String>,
    today: NaiveDate,
    path: &Path,
) -> Result<Vec<Change>> {
    match intent {
        DesktopIntent::AddTask { body, column } => {
            if body.trim().is_empty() {
                return Err(Error::InvalidTaskLine("task body cannot be empty".into()));
            }
            let existing: HashSet<_> = document
                .tasks
                .iter()
                .filter_map(|task| task.id.clone())
                .collect();
            let id = generate_task_id(&existing)?;
            *created_task_id = Some(id.clone());
            Ok(vec![Change::InsertTask {
                column: column.clone(),
                position: InsertPosition::End,
                line: format!("- [ ] {body} <!--t:{id}-->"),
            }])
        }
        DesktopIntent::SetCompleted {
            id,
            completed,
            note,
            evidence,
        } => {
            let task = find_task(document, id)?;
            if *completed
                && document
                    .completion_policy()
                    .requires_note(CompletionOutcome::Done)
                && note.as_deref().is_none_or(|text| text.trim().is_empty())
            {
                return Err(Error::InvalidTaskLine(
                    "strict completion requires a note for done".into(),
                ));
            }
            let mut changes = vec![Change::SetCompleted {
                task,
                completed: *completed,
            }];
            if *completed {
                for (token, metadata) in document.tasks[task].metadata.iter().enumerate() {
                    if matches!(
                        metadata.kind,
                        TokenKind::Progress(_) | TokenKind::Failure(_)
                    ) {
                        changes.push(Change::RemoveMetadata { task, token });
                    }
                }
                if note.is_none() && document.tasks[task].note.is_some() {
                    changes.push(Change::RemoveNote { task });
                }
            }
            if let Some(note) = note {
                changes.push(Change::SetNote {
                    task,
                    text: note.clone(),
                });
            }
            if let Some(evidence) = evidence {
                validate_evidence(path, evidence)?;
                changes.push(Change::SetMetadata {
                    task,
                    kind: TokenKind::Evidence {
                        from: evidence.from.clone(),
                        to: evidence.to.clone(),
                    },
                    raw: format!("🔗{}..{}", evidence.from, evidence.to),
                });
            }
            Ok(changes)
        }
        DesktopIntent::SetPartial { id, percent, note } => {
            if !(1..=99).contains(percent) {
                return Err(Error::InvalidTaskLine(
                    "progress must be between 1 and 99".into(),
                ));
            }
            if note.trim().is_empty() {
                return Err(Error::InvalidTaskLine(
                    "partial completion requires a note".into(),
                ));
            }
            let task = find_task(document, id)?;
            Ok(vec![
                Change::SetMetadata {
                    task,
                    kind: TokenKind::Progress(*percent),
                    raw: format!("◐{percent}"),
                },
                Change::SetNote {
                    task,
                    text: note.clone(),
                },
            ])
        }
        DesktopIntent::SetPriority {
            id,
            priority,
            scope,
        } => set_priority_changes(document, id, *priority, *scope),
        DesktopIntent::SetFailed { id, note } => {
            if note.trim().is_empty() {
                return Err(Error::InvalidTaskLine(
                    "failed attempt requires a note".into(),
                ));
            }
            let task = find_task(document, id)?;
            let today = today.to_string();
            Ok(vec![
                Change::SetMetadata {
                    task,
                    kind: TokenKind::Failure(today.clone()),
                    raw: format!("⚠{today}"),
                },
                Change::SetNote {
                    task,
                    text: note.clone(),
                },
            ])
        }
        DesktopIntent::SetBody { id, body } => set_body_changes(document, id, body),
        DesktopIntent::MoveTask {
            id,
            column,
            target,
            note,
            evidence,
        } => {
            let task = find_task(document, id)?;
            let into_done = document.columns.iter().any(|candidate| {
                candidate.id == *column && candidate.kind.as_deref() == Some("done")
            });
            if note
                .as_deref()
                .is_some_and(|text| text.contains(['\n', '\r']))
            {
                return Err(Error::InvalidTaskLine(
                    "completion note must remain on one line".into(),
                ));
            }
            let completion_note = note.as_deref().filter(|text| !text.trim().is_empty());
            if !into_done && (completion_note.is_some() || evidence.is_some()) {
                return Err(Error::InvalidTaskLine(
                    "completion details require a done destination".into(),
                ));
            }
            if into_done && !document.tasks[task].completed {
                // A move into Done is one completion transaction. Strict and
                // in-progress tasks supply their outcome here so the checkbox,
                // metadata cleanup, note, evidence, and placement write atomically.
                let missing_note = completion_note.is_none();
                if document
                    .completion_policy()
                    .requires_note(CompletionOutcome::Done)
                    && missing_note
                {
                    return Err(Error::InvalidTaskLine(
                        "strict completion requires a note for done; record the outcome instead of moving into Done".into(),
                    ));
                }
                if document.tasks[task].partial() && missing_note {
                    return Err(Error::InvalidTaskLine(
                        "task has partial or failed state; record the outcome instead of moving into Done".into(),
                    ));
                }
            }
            if let Some(evidence) = evidence {
                validate_evidence(path, evidence)?;
            }
            Ok(vec![Change::MoveTask {
                task,
                column: column.clone(),
                position: resolve_move_target(document, column, target)?,
                completion_note: completion_note.map(str::to_owned),
                completion_evidence: evidence
                    .as_ref()
                    .map(|range| (range.from.clone(), range.to.clone())),
            }])
        }
    }
}

/// Rewrites only the body span of one task line.
///
/// The edited text must read back as exactly the same body: text that ends in
/// something the parser would peel off as metadata (`#tag`, `!1`, a date, …) or
/// that adds a stable-id comment would silently change the task's meaning, so
/// it is refused instead of written.
fn set_body_changes(document: &Document, id: &str, body: &str) -> Result<Vec<Change>> {
    let body = body.trim();
    if body.is_empty() {
        return Err(Error::InvalidTaskLine("task body cannot be empty".into()));
    }
    if body.contains(['\n', '\r']) {
        return Err(Error::InvalidTaskLine(
            "task body must remain on one line".into(),
        ));
    }
    if body.contains("<!--") || body.contains("-->") {
        return Err(Error::InvalidTaskLine(
            "task body must not contain HTML comment markers".into(),
        ));
    }
    let task = find_task(document, id)?;
    if document.tasks[task].body == body {
        return Ok(Vec::new());
    }
    let changes = vec![Change::SetBody {
        task,
        body: body.to_owned(),
    }];
    let output = crate::write::apply_changes(document, &changes)?;
    let reparsed = crate::parse(output)?;
    let original = &document.tasks[task];
    let unchanged_shape = reparsed.tasks.len() == document.tasks.len()
        && reparsed.tasks.get(task).is_some_and(|written| {
            written.id == original.id
                && written.body == body
                && written.metadata.len() == original.metadata.len()
        });
    if !unchanged_shape {
        return Err(Error::InvalidTaskLine(
            "task body would be read back as metadata or another task; rephrase the ending".into(),
        ));
    }
    Ok(changes)
}

pub fn set_priority_changes(
    document: &Document,
    id: &str,
    priority: Option<u8>,
    scope: PriorityScope,
) -> Result<Vec<Change>> {
    if priority.is_some_and(|value| !(1..=4).contains(&value)) {
        return Err(Error::InvalidTaskLine(
            "priority must be between 1 and 4".into(),
        ));
    }
    let root = find_task(document, id)?;
    let end = if scope == PriorityScope::Subtree {
        document.tasks[root + 1..]
            .iter()
            .position(|task| {
                task.indent_level <= document.tasks[root].indent_level
                    || task.column != document.tasks[root].column
            })
            .map_or(document.tasks.len(), |offset| root + 1 + offset)
    } else {
        root + 1
    };
    let mut changes = Vec::new();
    for task_index in root..end {
        let tokens: Vec<_> = document.tasks[task_index]
            .metadata
            .iter()
            .enumerate()
            .filter_map(|(index, token)| {
                matches!(token.kind, TokenKind::Priority(_)).then_some(index)
            })
            .collect();
        if let Some(value) = priority {
            let already_normalized = tokens.len() == 1
                && matches!(document.tasks[task_index].metadata[tokens[0]].kind, TokenKind::Priority(current) if current == value);
            if already_normalized {
                continue;
            }
            if tokens.len() > 1 {
                for token in tokens.iter().skip(1).rev() {
                    changes.push(Change::RemoveMetadata {
                        task: task_index,
                        token: *token,
                    });
                }
            }
            changes.push(Change::SetMetadata {
                task: task_index,
                kind: TokenKind::Priority(value),
                raw: format!("!{value}"),
            });
        } else {
            for token in tokens.into_iter().rev() {
                changes.push(Change::RemoveMetadata {
                    task: task_index,
                    token,
                });
            }
        }
    }
    Ok(changes)
}

fn resolve_move_target(
    document: &Document,
    column: &str,
    target: &MoveTarget,
) -> Result<InsertPosition> {
    match target {
        MoveTarget::ColumnStart => Ok(InsertPosition::Start),
        MoveTarget::ColumnEnd => Ok(InsertPosition::End),
        MoveTarget::Before { task_id } => {
            let index = find_task(document, task_id)?;
            validate_target_column(document, index, task_id, column)?;
            Ok(InsertPosition::Before(index))
        }
        MoveTarget::After { task_id } => {
            let index = find_task(document, task_id)?;
            validate_target_column(document, index, task_id, column)?;
            Ok(InsertPosition::After(index))
        }
    }
}

fn validate_target_column(
    document: &Document,
    target_index: usize,
    task_id: &str,
    column: &str,
) -> Result<()> {
    if document.tasks[target_index].column.as_deref() == Some(column) {
        Ok(())
    } else {
        Err(Error::InvalidTaskLine(format!(
            "move target {task_id} does not belong to column {column}"
        )))
    }
}

fn find_task(document: &Document, id: &str) -> Result<usize> {
    document
        .tasks
        .iter()
        .position(|task| task.id.as_deref() == Some(id))
        .ok_or_else(|| Error::TaskNotFound(id.into()))
}

fn projected_task_view(
    head: std::result::Result<&String, &String>,
    introductions: &EvidenceLookup,
    task: ProjectedTask,
) -> DesktopTask {
    let evidence_availability =
        evidence_availability(head, introductions, &task.id, task.evidence.first());
    DesktopTask {
        id: Some(task.id),
        body: task.body,
        completed: task.completed,
        column: task.column,
        indent_level: task.indent_level,
        parent_id: task.parent_id,
        due: task.due,
        priority: task.priority,
        tags: task.tags,
        progress: task.progress,
        failed_on: task.failed_on,
        note: task.note,
        evidence: task.evidence.into_iter().next(),
        evidence_availability,
    }
}

fn task_view(
    head: std::result::Result<&String, &String>,
    introductions: &EvidenceLookup,
    document: &Document,
    task: &crate::Task,
) -> DesktopTask {
    let mut due = None;
    let mut priority = None;
    let mut tags = Vec::new();
    for token in &task.metadata {
        match &token.kind {
            TokenKind::DueDate(value) => due = Some(value.clone()),
            TokenKind::Priority(value) => priority = Some(*value),
            TokenKind::Tag(value) => tags.push(value.clone()),
            _ => {}
        }
    }
    let evidence = task.evidence().map(|(from, to)| format!("{from}..{to}"));
    let evidence_availability = task.id.as_deref().map_or_else(
        || EvidenceAvailability::Unavailable {
            reason: "task has no stable ID".into(),
        },
        |id| evidence_availability(head, introductions, id, evidence.as_ref()),
    );
    DesktopTask {
        id: task.id.clone(),
        body: task.body.clone(),
        completed: task.completed,
        column: task.column.clone(),
        indent_level: task.indent_level,
        parent_id: task
            .parent
            .and_then(|index| document.tasks.get(index)?.id.clone()),
        due,
        priority,
        tags,
        progress: task.progress(),
        failed_on: task.failure().map(str::to_string),
        note: task.note.as_ref().map(|note| note.text.clone()),
        evidence,
        evidence_availability,
    }
}

/// What one snapshot needs from git, resolved against the cache.
#[derive(Debug, Default)]
struct EvidenceLookup {
    /// Stable ID → commit that first added its `<!--t:ID-->` anchor.
    introductions: HashMap<String, String>,
    /// Raw `from` of an existing `🔗from..to` → full commit hash, or `None`
    /// when the repository has no such commit.
    resolved: HashMap<String, Option<String>>,
}

/// Cross-snapshot memory of the task file's git history.
///
/// The first scan reads the whole history of the task file; later scans read
/// only `scanned_head..HEAD`, so a busy server pays per commit rather than per
/// request. An evidence start is resolved once and remembered by its raw text.
#[derive(Debug, Default)]
struct EvidenceCache {
    file: Option<PathBuf>,
    scanned_head: Option<String>,
    introductions: HashMap<String, String>,
    resolved: HashMap<String, Option<String>>,
}

impl EvidenceCache {
    fn refresh(&mut self, path: &Path, head: Option<&str>, document: &Document) -> EvidenceLookup {
        if self.file.as_deref() != Some(path) {
            *self = Self {
                file: Some(path.to_path_buf()),
                ..Self::default()
            };
        }
        let Some(head) = head else {
            return EvidenceLookup::default();
        };
        let repository = repository_of(path);
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("TODO.md");

        let needs_scan = self.scanned_head.as_deref() != Some(head)
            && document
                .tasks
                .iter()
                .filter(|task| task.evidence().is_none())
                .filter_map(|task| task.id.as_deref())
                .any(|id| !self.introductions.contains_key(id));
        if needs_scan {
            let range = self
                .scanned_head
                .as_ref()
                .map(|old| format!("{old}..{head}"));
            let history = match scan_history(repository, file_name, range.as_deref()) {
                Ok(history) => Some(history),
                // The previous HEAD may have been rewritten away; the history
                // we remembered is then suspect, so start over from the root.
                Err(_) if range.is_some() => {
                    self.introductions.clear();
                    scan_history(repository, file_name, None).ok()
                }
                Err(_) => None,
            };
            if let Some(history) = history {
                for (id, commit) in history {
                    self.introductions.entry(id).or_insert(commit);
                }
                self.scanned_head = Some(head.to_string());
            }
        }

        let unresolved: Vec<String> = document
            .tasks
            .iter()
            .filter_map(|task| task.evidence())
            .map(|(from, _)| from.to_string())
            .filter(|from| !self.resolved.contains_key(from))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        if !unresolved.is_empty() {
            for (from, commit) in resolve_commits(repository, &unresolved) {
                self.resolved.insert(from, commit);
            }
        }

        EvidenceLookup {
            introductions: self.introductions.clone(),
            resolved: self.resolved.clone(),
        }
    }
}

/// Reads the task file's history once (or only `range` when given) and maps
/// every `<!--t:ID-->` anchor to the commit whose diff first added it.
///
/// Anchors are matched whole: a six-character base36 ID is short enough to
/// occur inside prose or a commit hash, which would misattribute the task.
fn scan_history(
    repository: &Path,
    file_name: &str,
    range: Option<&str>,
) -> std::result::Result<Vec<(String, String)>, String> {
    let mut args = vec!["log", "--reverse", "--format=commit:%H", "-p"];
    if let Some(range) = range {
        args.push(range);
    }
    args.extend(["--", file_name]);
    let history = git_output(repository, &args)?;
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    let mut commit: Option<String> = None;
    for line in history.lines() {
        if let Some(hash) = line.strip_prefix("commit:") {
            commit = Some(hash.to_string());
            continue;
        }
        let Some(added) = line.strip_prefix('+').filter(|_| !line.starts_with("+++")) else {
            continue;
        };
        let Some(hash) = &commit else { continue };
        let mut rest = added;
        while let Some(start) = rest.find("<!--t:") {
            let after = &rest[start + "<!--t:".len()..];
            let Some(end) = after.find("-->") else { break };
            let id = &after[..end];
            if !id.is_empty() && seen.insert(id.to_string()) {
                found.push((id.to_string(), hash.clone()));
            }
            rest = &after[end + "-->".len()..];
        }
    }
    Ok(found)
}

/// Resolves every candidate to a full commit hash in one `git cat-file`
/// process, so a board full of linked tasks still costs one subprocess.
fn resolve_commits(repository: &Path, candidates: &[String]) -> Vec<(String, Option<String>)> {
    use std::io::Write;
    let input: String = candidates
        .iter()
        .map(|candidate| format!("{candidate}^{{commit}}\n"))
        .collect();
    let output = std::process::Command::new("git")
        .args(["cat-file", "--batch-check"])
        .current_dir(repository)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .and_then(|mut child| {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(input.as_bytes())?;
            }
            child.wait_with_output()
        });
    let Ok(output) = output else {
        return candidates.iter().map(|c| (c.clone(), None)).collect();
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    candidates
        .iter()
        .zip(stdout.lines())
        .map(|(candidate, line)| {
            let mut fields = line.split_whitespace();
            let resolved = match (fields.next(), fields.next()) {
                (Some(hash), Some("commit")) => Some(hash.to_string()),
                _ => None,
            };
            (candidate.clone(), resolved)
        })
        .collect()
}

fn evidence_availability(
    head: std::result::Result<&String, &String>,
    lookup: &EvidenceLookup,
    id: &str,
    existing: Option<&String>,
) -> EvidenceAvailability {
    let head = match head {
        Ok(head) => head.clone(),
        Err(reason) => {
            return EvidenceAvailability::Unavailable {
                reason: (*reason).clone(),
            }
        }
    };
    let from = match existing.and_then(|existing| existing.split_once("..")) {
        Some((from, _)) => match lookup.resolved.get(from) {
            Some(Some(resolved)) => Some(resolved.clone()),
            _ => {
                return EvidenceAvailability::Unavailable {
                    reason: format!(
                        "existing evidence start {from} is not a commit in this repository"
                    ),
                }
            }
        },
        None => lookup.introductions.get(id).cloned(),
    };
    match from {
        Some(from) if !from.is_empty() && from != head => {
            EvidenceAvailability::Available { from, to: head }
        }
        Some(_) => EvidenceAvailability::Unavailable {
            reason: "evidence would be an empty HEAD..HEAD range".into(),
        },
        None => EvidenceAvailability::Unavailable {
            reason: format!("stable ID {id} was not found in the task file's git history"),
        },
    }
}

fn repository_of(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn validate_evidence(path: &Path, evidence: &EvidenceRange) -> Result<()> {
    let repository = repository_of(path);
    let head = git_output(repository, &["rev-parse", "--verify", "HEAD^{commit}"])
        .map_err(Error::InvalidTaskLine)?;
    let resolve = |value: &str| {
        git_output(
            repository,
            &["rev-parse", "--verify", &format!("{value}^{{commit}}")],
        )
        .map_err(Error::InvalidTaskLine)
    };
    let from = resolve(&evidence.from)?;
    let to = resolve(&evidence.to)?;
    if from == to {
        return Err(Error::InvalidTaskLine(
            "evidence range must not be empty".into(),
        ));
    }
    if to != head {
        return Err(Error::InvalidTaskLine(
            "evidence range must end at current HEAD".into(),
        ));
    }
    Ok(())
}

fn git_output(repository: &Path, args: &[&str]) -> std::result::Result<String, String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(repository)
        .output()
        .map_err(|error| format!("git is unavailable: {error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if detail.is_empty() {
            format!("git {} failed", args.join(" "))
        } else {
            detail
        });
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() {
        Err(format!("git {} returned no commit", args.join(" ")))
    } else {
        Ok(value)
    }
}

/// Supplies an ordering for a Today band.
///
/// The built-in implementation is deterministic and free. A hosted ranker that
/// weighs dependencies, estimated effort and recent activity is the paid seam:
/// it plugs in here rather than replacing the view, so the free tier keeps a
/// complete ordering and never degrades into an unsorted list when no ranker is
/// configured.
pub trait TaskRanker: std::fmt::Debug {
    /// Reorders `tasks` in place. Must be a total order and must neither add nor
    /// drop entries — the caller renders exactly what it passed in.
    fn rank(&self, tasks: &mut [DesktopTask]);
}

/// Priority, then due date, then work already started. Always available.
#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultRanker;

impl TaskRanker for DefaultRanker {
    fn rank(&self, tasks: &mut [DesktopTask]) {
        sort_by_urgency(tasks);
    }
}

/// Orders a Today band: priority first, then due date, then work already started.
///
/// Started work is surfaced above untouched work at equal urgency, because
/// finishing something at 60% is usually cheaper than starting something new.
/// The order is total and deterministic, so the list does not reshuffle between
/// renders of the same data.
fn sort_by_urgency(tasks: &mut [DesktopTask]) {
    tasks.sort_by(|left, right| {
        // Missing priority sorts last, not as 0.
        let priority = left
            .priority
            .unwrap_or(u8::MAX)
            .cmp(&right.priority.unwrap_or(u8::MAX));
        let due = task_due_date(left)
            .map(|date| date.to_string())
            .unwrap_or_else(|| "9999-99-99".into())
            .cmp(
                &task_due_date(right)
                    .map(|date| date.to_string())
                    .unwrap_or_else(|| "9999-99-99".into()),
            );
        let started = right.progress.is_some().cmp(&left.progress.is_some());
        priority
            .then(due)
            .then(started)
            .then_with(|| left.body.cmp(&right.body))
    });
}

fn task_due_date(task: &DesktopTask) -> Option<NaiveDate> {
    task.due
        .as_deref()
        .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
}

fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |value, byte| {
        (value ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

pub trait CaptureProject {
    fn create_task(&mut self, body: &str, column: Option<&str>) -> Result<String>;
}

impl CaptureProject for DesktopBackend {
    fn create_task(&mut self, body: &str, column: Option<&str>) -> Result<String> {
        let (_, task_id) = self.apply_intent(DesktopIntent::AddTask {
            body: body.into(),
            column: column.map(str::to_string),
        })?;
        task_id.ok_or_else(|| Error::InvalidTaskLine("task creation returned no ID".into()))
    }
}

pub const MAX_CAPTURE_BYTES: usize = 64 * 1024;
const CAPTURE_PREAMBLE: &str = "# Captures\n\n";
const CAPTURE_HEADER_PREFIX: &str = "<!--capture:";
const CAPTURE_HEADER_END: &str = "-->\n";
const CAPTURE_FOOTER: &str = "\n<!--/capture-->\n\n";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Capture {
    pub id: String,
    pub created_at: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TriageResult {
    Completed { task_id: String },
    TaskCreatedCaptureRetained { task_id: String, message: String },
}

#[derive(Debug, Clone)]
pub struct CaptureStore {
    path: PathBuf,
    lock_timeout: Duration,
    clock: Arc<dyn Clock>,
}

impl CaptureStore {
    pub fn new(path: impl Into<PathBuf>, lock_timeout: Duration) -> Self {
        Self::with_clock(path, lock_timeout, Arc::new(SystemClock))
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
        Ok(Self::new(global_capture_path()?, lock_timeout))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn list(&self) -> Result<Vec<Capture>> {
        read_capture_items(&self.path)
    }

    pub fn append(&self, text: &str) -> Result<Capture> {
        self.append_with_snapshot(text)
            .map(|(capture, _items)| capture)
    }

    pub fn append_with_snapshot(&self, text: &str) -> Result<(Capture, Vec<Capture>)> {
        validate_capture_text(text)?;
        self.update(|items| {
            let existing = items.iter().map(|capture| capture.id.clone()).collect();
            let capture = Capture {
                id: generate_task_id(&existing)?,
                created_at: self
                    .clock
                    .now_utc()
                    .to_rfc3339_opts(SecondsFormat::Secs, true),
                text: text.into(),
            };
            items.push(capture.clone());
            Ok((capture, items.clone()))
        })
    }

    pub fn delete(&self, id: &str) -> Result<Vec<Capture>> {
        self.update(|items| {
            let before = items.len();
            items.retain(|item| item.id != id);
            if items.len() == before {
                return Err(Error::TaskNotFound(id.into()));
            }
            Ok(items.clone())
        })
    }

    pub fn triage_to_task<P: CaptureProject>(
        &self,
        id: &str,
        project: &mut P,
        column: Option<&str>,
    ) -> Result<TriageResult> {
        self.ensure_parent()?;
        let writer = LockedWriter::new(&self.path, self.lock_timeout);
        let _guard = writer.lock()?;
        let mut items = read_capture_items(&self.path)?;
        let index = items
            .iter()
            .position(|item| item.id == id)
            .ok_or_else(|| Error::TaskNotFound(id.into()))?;
        let task_id = project.create_task(&items[index].text, column)?;
        items.remove(index);
        match crate::atomic_write(&self.path, serialize_captures(&items).as_bytes()) {
            Ok(()) => Ok(TriageResult::Completed { task_id }),
            Err(error) => Ok(TriageResult::TaskCreatedCaptureRetained {
                task_id,
                message: error.to_string(),
            }),
        }
    }

    fn update<T, F>(&self, change: F) -> Result<T>
    where
        F: FnOnce(&mut Vec<Capture>) -> Result<T>,
    {
        self.ensure_parent()?;
        let writer = LockedWriter::new(&self.path, self.lock_timeout);
        let _guard = writer.lock()?;
        let mut items = read_capture_items(&self.path)?;
        let result = change(&mut items)?;
        crate::atomic_write(&self.path, serialize_captures(&items).as_bytes())?;
        Ok(result)
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

fn validate_capture_text(text: &str) -> Result<()> {
    if text.trim().is_empty() {
        return Err(Error::InvalidTaskLine("capture cannot be empty".into()));
    }
    if text.len() > MAX_CAPTURE_BYTES {
        return Err(Error::InvalidTaskLine(format!(
            "capture exceeds {MAX_CAPTURE_BYTES} bytes"
        )));
    }
    Ok(())
}

pub fn global_capture_path() -> Result<PathBuf> {
    dirs::home_dir()
        .map(|home| home.join(".todo").join("inbox.md"))
        .ok_or(Error::HomeDirectoryUnavailable)
}

fn read_capture_items(path: &Path) -> Result<Vec<Capture>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(Error::Io {
                path: path.to_path_buf(),
                source,
            })
        }
    };
    let source = String::from_utf8(bytes).map_err(|_| Error::InvalidCaptureFile {
        path: path.to_path_buf(),
        message: "file is not valid UTF-8".into(),
    })?;
    parse_captures(&source, path)
}

fn parse_captures(source: &str, path: &Path) -> Result<Vec<Capture>> {
    if !source.starts_with(CAPTURE_PREAMBLE) {
        return Err(capture_error(path, "missing '# Captures' preamble"));
    }
    let mut cursor = CAPTURE_PREAMBLE.len();
    let mut items = Vec::new();
    let mut seen_ids = HashSet::new();
    while cursor < source.len() {
        let record_number = items.len() + 1;
        let tail = &source[cursor..];
        if !tail.starts_with(CAPTURE_HEADER_PREFIX) {
            return Err(capture_error(
                path,
                format!("unexpected content at byte {cursor}"),
            ));
        }
        let header_end_relative = tail.find(CAPTURE_HEADER_END).ok_or_else(|| {
            capture_error(
                path,
                format!("record {record_number} has an unclosed header"),
            )
        })?;
        let header_start = cursor + CAPTURE_HEADER_PREFIX.len();
        let header_end = cursor + header_end_relative;
        let fields: Vec<_> = source[header_start..header_end]
            .split_whitespace()
            .collect();
        if fields.len() != 3 {
            return Err(capture_error(
                path,
                format!("record {record_number} has an invalid header"),
            ));
        }
        let id = fields[0];
        if !crate::parse::valid_id(id) {
            return Err(capture_error(
                path,
                format!("record {record_number} has invalid ID '{id}'"),
            ));
        }
        if !valid_created_at(fields[1]) {
            return Err(capture_error(
                path,
                format!("record {record_number} has an invalid RFC3339 timestamp"),
            ));
        }
        let byte_length = fields[2]
            .strip_prefix("bytes=")
            .and_then(|value| value.parse::<usize>().ok())
            .ok_or_else(|| {
                capture_error(
                    path,
                    format!("record {record_number} has an invalid byte length"),
                )
            })?;
        if byte_length > MAX_CAPTURE_BYTES {
            return Err(capture_error(
                path,
                format!("record {record_number} exceeds {MAX_CAPTURE_BYTES} bytes"),
            ));
        }
        let text_start = header_end + CAPTURE_HEADER_END.len();
        let text_end = text_start.checked_add(byte_length).ok_or_else(|| {
            capture_error(
                path,
                format!("record {record_number} byte length overflowed"),
            )
        })?;
        if text_end > source.len() || !source.is_char_boundary(text_end) {
            return Err(capture_error(
                path,
                format!("record {record_number} has a truncated UTF-8 payload"),
            ));
        }
        if !source[text_end..].starts_with(CAPTURE_FOOTER) {
            return Err(capture_error(
                path,
                format!("record {record_number} byte length does not end at its closing marker"),
            ));
        }
        if !seen_ids.insert(id.to_string()) {
            return Err(capture_error(
                path,
                format!("record {record_number} repeats capture ID '{id}'"),
            ));
        }
        items.push(Capture {
            id: id.into(),
            created_at: fields[1].into(),
            text: source[text_start..text_end].into(),
        });
        cursor = text_end + CAPTURE_FOOTER.len();
    }
    Ok(items)
}

fn serialize_captures(items: &[Capture]) -> String {
    let mut output = CAPTURE_PREAMBLE.to_string();
    for item in items {
        output.push_str(&format!(
            "{CAPTURE_HEADER_PREFIX}{} {} bytes={}{}",
            item.id,
            item.created_at,
            item.text.len(),
            CAPTURE_HEADER_END
        ));
        output.push_str(&item.text);
        output.push_str(CAPTURE_FOOTER);
    }
    output
}

fn valid_created_at(value: &str) -> bool {
    DateTime::parse_from_rfc3339(value)
        .map(|parsed| {
            parsed
                .with_timezone(&Utc)
                .to_rfc3339_opts(SecondsFormat::Secs, true)
                == value
        })
        .unwrap_or(false)
}

fn capture_error(path: &Path, message: impl Into<String>) -> Error {
    Error::InvalidCaptureFile {
        path: path.to_path_buf(),
        message: message.into(),
    }
}
