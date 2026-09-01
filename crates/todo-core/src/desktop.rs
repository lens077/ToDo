use crate::{
    generate_task_id, parse_file, project_document, Change, Document, Error, InsertPosition,
    LockedWriter, ProjectedTask, Result, TokenKind,
};
use chrono::{DateTime, Days, Local, NaiveDate, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
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
    pub due: Option<String>,
    pub priority: Option<u8>,
    pub tags: Vec<String>,
    /// Partial completion percentage, when the task carries a `◐` token.
    pub progress: Option<u8>,
    /// Date of the last failed attempt, when the task carries a `⚠` token.
    pub failed_on: Option<String>,
    /// Text of the `↳` note line, when present.
    pub note: Option<String>,
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
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DesktopIntent {
    AddTask {
        body: String,
        column: Option<String>,
    },
    SetCompleted {
        id: String,
        completed: bool,
    },
    MoveTask {
        id: String,
        column: String,
        target: MoveTarget,
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
        let path = self.project.as_ref().ok_or(Error::NotTaskFile)?;
        self.derive(&parse_file(path)?)
    }

    pub fn mutate(&self, intent: DesktopIntent) -> Result<DesktopSnapshot> {
        let (document, _) = self.apply_intent(intent)?;
        self.derive(&document)
    }

    pub fn project_path(&self) -> Option<&Path> {
        self.project.as_deref()
    }

    fn apply_intent(&self, intent: DesktopIntent) -> Result<(Document, Option<String>)> {
        let path = self.project.as_ref().ok_or(Error::NotTaskFile)?;
        let writer = LockedWriter::new(path, self.lock_timeout);
        let mut created_task_id = None;
        // Label the restore point with the desktop action that caused it, so a user
        // scanning history sees "board-move" rather than an anonymous "update".
        let label = intent.label();
        let document = writer.update_labeled(label, |document| {
            intent_changes(document, &intent, &mut created_task_id)
        })?;
        Ok((document, created_task_id))
    }

    fn derive(&self, document: &Document) -> Result<DesktopSnapshot> {
        let path = self.project.as_ref().ok_or(Error::NotTaskFile)?;
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
                    .map_or_else(|| task_view(task), projected_task_view)
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
    if path.is_dir() {
        path.join("TODO.md")
    } else {
        path.to_path_buf()
    }
}

fn intent_changes(
    document: &Document,
    intent: &DesktopIntent,
    created_task_id: &mut Option<String>,
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
        DesktopIntent::SetCompleted { id, completed } => Ok(vec![Change::SetCompleted {
            task: find_task(document, id)?,
            completed: *completed,
        }]),
        DesktopIntent::MoveTask { id, column, target } => Ok(vec![Change::MoveTask {
            task: find_task(document, id)?,
            column: column.clone(),
            position: resolve_move_target(document, column, target)?,
        }]),
    }
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

fn projected_task_view(task: ProjectedTask) -> DesktopTask {
    DesktopTask {
        id: Some(task.id),
        body: task.body,
        completed: task.completed,
        column: task.column,
        due: task.due,
        priority: task.priority,
        tags: task.tags,
        progress: task.progress,
        failed_on: task.failed_on,
        note: task.note,
    }
}

fn task_view(task: &crate::Task) -> DesktopTask {
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
    DesktopTask {
        id: task.id.clone(),
        body: task.body.clone(),
        completed: task.completed,
        column: task.column.clone(),
        due,
        priority,
        tags,
        progress: task.progress(),
        failed_on: task.failure().map(str::to_string),
        note: task.note.as_ref().map(|note| note.text.clone()),
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
