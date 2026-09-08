//! Bounded, read-only project discovery and explicitly source-directed writes.
use crate::{
    atomic_write, diff_lines, parse, CompletionPolicy, DesktopBackend, DesktopColumn,
    DesktopIntent, DesktopSnapshot, DesktopTask, Document, Error, History, LockedWriter, Result,
    TextDiff,
};
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    time::Duration,
};

const MAX_SOURCES: usize = 256;
const MAX_FILE: usize = 2 * 1024 * 1024;
const MAX_TOTAL: usize = 16 * 1024 * 1024;
const MAX_ENTRIES: usize = 10_000;
const MAX_PREVIEW: usize = 16 * 1024;
/// Bytes read from the head of a Markdown file to look for a `todo-spec`
/// frontmatter declaration. Frontmatter that opts a file in is short by
/// design; a longer preamble simply is not an opt-in.
const MAX_DECLARATION_HEAD: usize = 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskSource {
    pub path: String,
    pub status: String,
    pub writable: bool,
    /// Raw text shown for unmanaged or unreadable sources; empty once managed.
    pub preview: String,
    /// The preview stops at `MAX_PREVIEW` bytes; writes always use the whole file.
    #[serde(default)]
    pub preview_truncated: bool,
    /// Bullet lines (`- text`) that the loose list reader would treat as
    /// tasks. A hint for offering the Beta reader; never a task count.
    #[serde(default)]
    pub loose_candidates: usize,
    /// True when this snapshot read the source with the loose list reader.
    /// Such a source is always read-only until it is adopted.
    #[serde(default)]
    pub loose: bool,
    pub error: Option<String>,
    pub revision: String,
    pub task_count: usize,
    pub completion_policy: CompletionPolicy,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectTask {
    #[serde(flatten)]
    pub task: DesktopTask,
    pub source_path: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectColumn {
    #[serde(flatten)]
    pub column: DesktopColumn,
    pub source_path: String,
}
impl std::ops::Deref for ProjectTask {
    type Target = DesktopTask;
    fn deref(&self) -> &Self::Target {
        &self.task
    }
}
impl std::ops::Deref for ProjectColumn {
    type Target = DesktopColumn;
    fn deref(&self) -> &Self::Target {
        &self.column
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectTodayView {
    pub overdue: Vec<ProjectTask>,
    pub today: Vec<ProjectTask>,
    pub upcoming: Vec<ProjectTask>,
    pub next: Vec<ProjectTask>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectSnapshot {
    pub project_path: String,
    pub revision: String,
    pub completion_policy: CompletionPolicy,
    pub columns: Vec<ProjectColumn>,
    pub tasks: Vec<ProjectTask>,
    pub today: ProjectTodayView,
    pub sources: Vec<TaskSource>,
    pub selected_source: Option<String>,
    pub warnings: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct AdoptionPreview {
    pub source_path: String,
    pub base_hash: String,
    pub diff: TextDiff,
    pub proposed: String,
}
#[derive(Debug, Clone)]
pub struct ProjectBackend {
    root: Option<PathBuf>,
    single: Option<String>,
    lock_timeout: Duration,
    base: DesktopBackend,
}
struct Source {
    info: TaskSource,
    content: Option<String>,
    document: Option<Document>,
}
impl Default for ProjectBackend {
    fn default() -> Self {
        Self::new(Duration::from_secs(2))
    }
}
impl ProjectBackend {
    pub fn new(lock_timeout: Duration) -> Self {
        Self {
            root: None,
            single: None,
            lock_timeout,
            base: DesktopBackend::new(lock_timeout),
        }
    }
    pub fn from_backend(backend: DesktopBackend) -> Self {
        let file = backend.project_path().map(Path::to_path_buf);
        let root = file
            .as_ref()
            .and_then(|p| p.parent())
            .map(Path::to_path_buf);
        let single = file
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(str::to_owned);
        Self {
            root,
            single,
            lock_timeout: Duration::from_secs(2),
            base: backend,
        }
    }
    pub fn project_path(&self) -> Option<&Path> {
        self.root.as_deref()
    }
    pub fn open_project(&mut self, path: impl AsRef<Path>) -> Result<ProjectSnapshot> {
        let path = path.as_ref();
        // The chosen root may live beneath system aliases such as macOS /var.
        // Resolve those ancestors once; source validation remains rooted at the
        // canonical project and rejects every symlink component inside it.
        let metadata = fs::symlink_metadata(path).map_err(|e| crate::error::io_error(path, e))?;
        if metadata.file_type().is_symlink() {
            return Err(invalid("selected project path must not be a symlink"));
        }
        let canonical = path
            .canonicalize()
            .map_err(|e| crate::error::io_error(path, e))?;
        let (root, single) = if canonical.is_dir() {
            (canonical, None)
        } else if canonical.is_file() {
            let name = canonical
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| invalid("invalid source filename"))?
                .to_owned();
            if !["todo.md", "todo", "todo.txt"].contains(&name.to_ascii_lowercase().as_str()) {
                // Preserve the legacy explicit-file contract for unrelated notes.
                let mut budget = MAX_FILE + 1;
                parse(bounded_read(&canonical, &mut budget)?)?;
            }
            (
                canonical
                    .parent()
                    .ok_or_else(|| invalid("missing parent"))?
                    .to_path_buf(),
                Some(name),
            )
        } else {
            return Err(invalid("project must be a directory or regular file"));
        };
        self.root = Some(root);
        self.single = single;
        self.snapshot()
    }
    pub fn snapshot(&self) -> Result<ProjectSnapshot> {
        self.build_snapshot(None, &[])
    }
    pub fn snapshot_source(&self, source: &str) -> Result<ProjectSnapshot> {
        self.build_snapshot(Some(source), &[])
    }
    /// Like `snapshot` / `snapshot_source`, but reads the named sources with
    /// the loose list reader (Beta): plain `- text` bullets become read-only
    /// tasks in memory. Nothing is written; the file stays as it is.
    pub fn snapshot_with_loose(
        &self,
        selected: Option<&str>,
        loose: &[String],
    ) -> Result<ProjectSnapshot> {
        self.build_snapshot(selected, loose)
    }
    fn build_snapshot(&self, selected: Option<&str>, loose: &[String]) -> Result<ProjectSnapshot> {
        let root = self.root.as_ref().ok_or(Error::NoProject)?;
        let (mut sources, mut warnings) = self.discover()?;
        for source in sources.iter_mut() {
            if loose.iter().any(|path| path == &source.info.path) {
                loosen_source(source);
            }
        }
        if let Some(source) = selected {
            self.validate(source)?;
            if !sources.iter().any(|s| s.info.path == source) {
                return Err(invalid("source is not in the discovered project"));
            }
        }
        let mut result = ProjectSnapshot {
            project_path: root.to_string_lossy().into_owned(),
            revision: String::new(),
            completion_policy: CompletionPolicy::Normal,
            columns: vec![],
            tasks: vec![],
            today: ProjectTodayView::default(),
            sources: vec![],
            selected_source: selected.map(str::to_owned),
            warnings: vec![],
        };
        for mut source in sources {
            if selected.is_none_or(|s| s == source.info.path) {
                if let Some(document) = source.document.as_ref() {
                    let mut backend = self.base.clone();
                    match backend.snapshot_document(&root.join(&source.info.path), document) {
                        Ok(snapshot) => append_snapshot(&mut result, &source.info.path, snapshot),
                        Err(e) => {
                            source.info.status = "error".into();
                            source.info.writable = false;
                            source.info.error = Some(e.to_string());
                            warnings.push(format!("{}: {e}", source.info.path));
                        }
                    }
                }
            }
            result.sources.push(source.info);
        }
        result.revision =
            revision(format!("{selected:?}{:?}{warnings:?}", result.sources).as_bytes());
        result.warnings = warnings;
        Ok(result)
    }
    pub fn source_backend(&self, source: Option<&str>) -> Result<DesktopBackend> {
        let (sources, _) = self.discover()?;
        let chosen = match source {
            Some(name) => sources
                .iter()
                .find(|s| s.info.path == name)
                .ok_or_else(|| invalid("source is not in the discovered project"))?,
            None => {
                let managed: Vec<_> = sources
                    .iter()
                    .filter(|s| s.info.status == "managed")
                    .collect();
                if sources.len() != 1 || managed.len() != 1 {
                    return Err(invalid("请先选择来源文件"));
                }
                managed[0]
            }
        };
        if chosen.info.status != "managed" || !chosen.info.writable {
            return Err(invalid("source is read-only; adopt it before writing"));
        }
        let path = self.validate(&chosen.info.path)?;
        validate_write_paths(&path)?;
        let mut backend = self.base.clone();
        backend.snapshot_document(&path, chosen.document.as_ref().ok_or(Error::NotTaskFile)?)?;
        Ok(backend)
    }
    pub fn mutate(&self, source: Option<&str>, intent: DesktopIntent) -> Result<ProjectSnapshot> {
        let backend = self.source_backend(source)?;
        // Revalidate immediately before the legacy backend acquires its advisory lock.
        // This narrows races, but is not an OS-level no-follow/openat guarantee.
        let path = backend.project_path().ok_or(Error::NoProject)?;
        let relative = relative(self.root.as_ref().ok_or(Error::NoProject)?, path)?;
        self.validate(&relative)?;
        backend.mutate(intent)?;
        self.snapshot()
    }
    pub fn preview_adopt(&self, source: &str) -> Result<AdoptionPreview> {
        self.preview_adopt_with(source, false)
    }
    /// `loose` also converts plain bullets into checkboxes, so a list written
    /// as `- text` becomes a managed task list in one reviewed rewrite.
    pub fn preview_adopt_with(&self, source: &str, loose: bool) -> Result<AdoptionPreview> {
        let (mut sources, _) = self.discover()?;
        let found = sources
            .iter_mut()
            .find(|s| s.info.path == source)
            .ok_or_else(|| invalid("source is not in the discovered project"))?;
        self.validate(source)?;
        if loose {
            loosen_source(found);
        }
        if found.info.status != "unmanaged" {
            return Err(invalid("only an unmanaged valid source can be adopted"));
        }
        let content = found.content.as_ref().ok_or(Error::NotTaskFile)?;
        let proposed = adoption(content, loose)?;
        Ok(AdoptionPreview {
            source_path: source.into(),
            base_hash: revision(content.as_bytes()),
            diff: diff_lines(content, &proposed),
            proposed,
        })
    }
    pub fn apply_adopt(&self, source: &str, base_hash: &str) -> Result<ProjectSnapshot> {
        self.apply_adopt_with(source, base_hash, false)
    }
    pub fn apply_adopt_with(
        &self,
        source: &str,
        base_hash: &str,
        loose: bool,
    ) -> Result<ProjectSnapshot> {
        self.preview_adopt_with(source, loose)?;
        let path = self.validate(source)?;
        if !writable(&path) {
            return Err(invalid("source is read-only"));
        }
        let writer = LockedWriter::new(&path, self.lock_timeout);
        let _guard = writer.lock()?;
        self.validate(source)?;
        let mut budget = MAX_TOTAL;
        let content = bounded_read(&path, &mut budget)?;
        if revision(content.as_bytes()) != base_hash {
            return Err(Error::ConcurrentModification);
        }
        let proposed = adoption(&content, loose)?;
        parse(proposed.clone())?;
        self.validate(source)?;
        if !writable(&path) {
            return Err(invalid("source is read-only"));
        }
        let current = bounded_read(&path, &mut budget)?;
        if current != content {
            return Err(Error::ConcurrentModification);
        }
        History::for_file(&path).record(&path, "adopt")?;
        self.validate(source)?;
        if bounded_read(&path, &mut budget)? != content {
            return Err(Error::ConcurrentModification);
        }
        atomic_write(&path, proposed.as_bytes())?;
        self.snapshot()
    }
    fn validate(&self, source: &str) -> Result<PathBuf> {
        if source.is_empty()
            || source.contains('\\')
            || source
                .split('/')
                .any(|c| c.is_empty() || c == "." || c == "..")
            || !Path::new(source)
                .components()
                .all(|c| matches!(c, Component::Normal(_)))
        {
            return Err(invalid("invalid relative source path"));
        }
        let root = self.root.as_ref().ok_or(Error::NoProject)?;
        let path = root.join(source);
        reject_symlinks(&path)?;
        let canonical = path
            .canonicalize()
            .map_err(|e| crate::error::io_error(&path, e))?;
        if !canonical.starts_with(root) || !canonical.is_file() {
            return Err(invalid("source escapes project or is not a regular file"));
        }
        Ok(canonical)
    }
    fn discover(&self) -> Result<(Vec<Source>, Vec<String>)> {
        let root = self.root.as_ref().ok_or(Error::NoProject)?;
        reject_symlinks(root)?;
        let mut warnings = vec![];
        let mut paths = vec![];
        if let Some(single) = &self.single {
            paths.push(single.clone());
        } else {
            let walk = WalkBuilder::new(root)
                .hidden(true)
                .git_ignore(true)
                .git_global(true)
                .require_git(false)
                .parents(true)
                .follow_links(false)
                .filter_entry(|e| {
                    e.depth() == 0
                        || !e.file_name().to_str().is_some_and(|n| {
                            matches!(n, ".git" | "node_modules" | "build" | "dist" | "target")
                        })
                })
                .sort_by_file_path(|a, b| a.cmp(b))
                .build();
            for (index, entry) in walk.enumerate() {
                if index >= MAX_ENTRIES {
                    warnings.push("Traversal truncated at 10000 entries".into());
                    break;
                }
                match entry {
                    Err(e) => warnings.push(format!("Discovery: {e}")),
                    // Two ways in: a conventional file name, or any Markdown
                    // file whose frontmatter declares `todo-spec`. The second
                    // lets a project keep TODO.md as an index and split the
                    // real lists into e.g. docs/todo/*.md, each opting in.
                    Ok(e)
                        if e.file_type().is_some_and(|t| t.is_file())
                            && (is_task_file_name(&e.file_name().to_string_lossy())
                                || (is_markdown_name(&e.file_name().to_string_lossy())
                                    && declares_todo_spec(e.path()))) =>
                    {
                        if paths.len() == MAX_SOURCES {
                            warnings.push("Discovery truncated at 256 sources".into());
                            break;
                        }
                        match relative(root, e.path()) {
                            Ok(p) => paths.push(p),
                            Err(e) => warnings.push(e.to_string()),
                        }
                    }
                    _ => {}
                }
            }
        }
        paths.sort();
        let mut budget = MAX_TOTAL;
        let mut sources = vec![];
        for path in paths {
            let mut source = Source {
                info: TaskSource {
                    path: path.clone(),
                    status: "error".into(),
                    writable: false,
                    preview: String::new(),
                    preview_truncated: false,
                    loose_candidates: 0,
                    loose: false,
                    error: None,
                    revision: String::new(),
                    task_count: 0,
                    completion_policy: CompletionPolicy::Normal,
                },
                content: None,
                document: None,
            };
            let loaded = (|| -> Result<()> {
                let full = self.validate(&path)?;
                let content = bounded_read(&full, &mut budget)?;
                source.info.revision = revision(content.as_bytes());
                let mut end = content.len().min(MAX_PREVIEW);
                while !content.is_char_boundary(end) {
                    end -= 1;
                }
                // A bounded preview is only a reading aid for sources the
                // board cannot show as tasks; a long file is not a finding.
                source.info.preview = content[..end].into();
                source.info.preview_truncated = end < content.len();
                let (document, managed) = classify(&content)?;
                if let Some(id) = document.duplicate_ids().first() {
                    return Err(Error::DuplicateTaskId(id.clone()));
                }
                if managed {
                    source.info.preview = String::new();
                    source.info.preview_truncated = false;
                }
                source.info.loose_candidates = loose_candidates(&content);
                source.info.status = if managed { "managed" } else { "unmanaged" }.into();
                source.info.writable = managed && writable(&full);
                source.info.task_count = document.tasks.len();
                source.info.completion_policy = document.completion_policy();
                source.content = Some(content);
                source.document = Some(document);
                Ok(())
            })();
            if let Err(e) = loaded {
                source.info.error = Some(e.to_string());
                warnings.push(format!("{path}: {e}"));
            }
            sources.push(source);
        }
        Ok((sources, warnings))
    }
}
fn invalid(message: &str) -> Error {
    Error::InvalidTaskLine(message.into())
}
fn is_task_file_name(name: &str) -> bool {
    ["todo.md", "todo", "todo.txt"].contains(&name.to_ascii_lowercase().as_str())
}
fn is_markdown_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".md") || lower.ends_with(".markdown")
}
/// True when the file opens with a YAML frontmatter block that contains a
/// `todo-spec` key. Reads at most `MAX_DECLARATION_HEAD` bytes and never
/// follows symlinks; anything unreadable is simply not a source.
fn declares_todo_spec(path: &Path) -> bool {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    let Ok(file) = fs::File::open(path) else {
        return false;
    };
    let mut bytes = Vec::with_capacity(MAX_DECLARATION_HEAD);
    if (&file)
        .take(MAX_DECLARATION_HEAD as u64)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return false;
    }
    let head = String::from_utf8_lossy(&bytes);
    let body = head.strip_prefix('\u{feff}').unwrap_or(&head);
    let mut lines = body.lines();
    if lines.next().map(|l| l.trim_end_matches('\r')) != Some("---") {
        return false;
    }
    for line in lines {
        let line = line.trim_end_matches('\r');
        if line == "---" {
            return false;
        }
        if line
            .split_once(':')
            .is_some_and(|(key, _)| key.trim() == "todo-spec")
        {
            return true;
        }
    }
    false
}
fn revision(bytes: &[u8]) -> String {
    format!("{:016x}", crate::parse::hash_bytes(bytes))
}
fn relative(root: &Path, path: &Path) -> Result<String> {
    path.strip_prefix(root)
        .map_err(|_| invalid("source escapes root"))?
        .components()
        .map(|c| {
            c.as_os_str()
                .to_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid("non-UTF-8 source path"))
        })
        .collect::<Result<Vec<_>>>()
        .map(|parts| parts.join("/"))
}
fn reject_symlinks(path: &Path) -> Result<()> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| crate::error::io_error(path, e))?
            .join(path)
    };
    let mut current = PathBuf::new();
    for component in absolute.components() {
        current.push(component);
        let meta =
            fs::symlink_metadata(&current).map_err(|e| crate::error::io_error(&current, e))?;
        if meta.file_type().is_symlink() {
            return Err(invalid("symlink components are not allowed"));
        }
    }
    Ok(())
}
fn writable(path: &Path) -> bool {
    validate_write_paths(path).is_ok()
        && [Some(path), path.parent()]
            .into_iter()
            .flatten()
            .all(|p| fs::metadata(p).is_ok_and(|m| !m.permissions().readonly()))
}
fn validate_write_paths(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| invalid("missing source parent"))?;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| invalid("invalid filename"))?;
    let mut pending = vec![parent.join(".todo"), parent.join(format!(".{name}.lock"))];
    let mut entries = 0;
    while let Some(candidate) = pending.pop() {
        entries += 1;
        if entries > MAX_ENTRIES {
            return Err(invalid("write metadata traversal limit exceeded"));
        }
        let metadata = match fs::symlink_metadata(&candidate) {
            Ok(metadata) => metadata,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(crate::error::io_error(&candidate, e)),
        };
        if metadata.file_type().is_symlink() {
            return Err(invalid("symlink in write metadata path"));
        }
        if metadata.is_dir() {
            for entry in
                fs::read_dir(&candidate).map_err(|e| crate::error::io_error(&candidate, e))?
            {
                let entry = entry.map_err(|e| crate::error::io_error(&candidate, e))?;
                pending.push(entry.path());
                if entries + pending.len() > MAX_ENTRIES {
                    return Err(invalid("write metadata traversal limit exceeded"));
                }
            }
        }
    }
    Ok(())
}
fn bounded_read(path: &Path, remaining: &mut usize) -> Result<String> {
    let file = fs::File::open(path).map_err(|e| crate::error::io_error(path, e))?;
    let size = file
        .metadata()
        .map_err(|e| crate::error::io_error(path, e))?
        .len();
    if size > MAX_FILE as u64 {
        return Err(invalid("source exceeds 2 MiB read limit"));
    }
    if size > *remaining as u64 {
        return Err(invalid("project exceeds 16 MiB read budget"));
    }
    let limit = (MAX_FILE + 1).min(*remaining);
    let mut bytes = vec![];
    (&file)
        .take(limit as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| crate::error::io_error(path, e))?;
    *remaining -= bytes.len();
    let final_size = file
        .metadata()
        .map_err(|e| crate::error::io_error(path, e))?
        .len();
    if bytes.len() > MAX_FILE || final_size > bytes.len() as u64 {
        return Err(invalid("source or project read limit exceeded"));
    }
    String::from_utf8(bytes).map_err(|_| Error::InvalidUtf8)
}
// Insert only frontmatter bytes; preserve BOM, line endings and all original prose.
fn prepare(content: &str) -> Result<String> {
    let start = if content.starts_with('\u{feff}') {
        3
    } else {
        0
    };
    let body = &content[start..];
    let newline = if body.contains("\r\n") { "\r\n" } else { "\n" };
    if body.lines().next() == Some("---") {
        let mut offset = start;
        let mut closing = None;
        for (i, line) in body.split_inclusive('\n').enumerate() {
            if i > 0 && line.trim_end_matches(['\r', '\n']) == "---" {
                closing = Some(offset);
                break;
            }
            offset += line.len();
        }
        let closing =
            closing.ok_or_else(|| Error::InvalidFrontmatter("unterminated frontmatter".into()))?;
        let first_end = start + body.find('\n').ok_or(Error::NotTaskFile)? + 1;
        let value: serde_yaml::Value = serde_yaml::from_str(&content[first_end..closing])
            .map_err(|e| Error::InvalidFrontmatter(e.to_string()))?;
        let mapping = value
            .as_mapping()
            .ok_or_else(|| Error::InvalidFrontmatter("frontmatter must be a mapping".into()))?;
        if mapping.contains_key(serde_yaml::Value::String("todo-spec".into())) {
            return Ok(content.into());
        }
        let mut result = content.to_owned();
        result.insert_str(first_end, &format!("todo-spec: 1{newline}"));
        Ok(result)
    } else {
        Ok(format!(
            "{}---{newline}todo-spec: 1{newline}---{newline}{}",
            &content[..start],
            body
        ))
    }
}
/// A source is managed when nothing needs to be written before the board can
/// address its tasks: the `todo-spec` declaration is present and every task
/// already carries a stable ID. A file that only declares the spec (for
/// example a split list opted in by hand) is discovered but stays unmanaged
/// until adoption assigns the missing IDs through the usual diff preview.
fn classify(content: &str) -> Result<(Document, bool)> {
    let prepared = prepare(content)?;
    let declared = prepared == content;
    let document = parse(prepared)?;
    let managed = declared && document.tasks.iter().all(|task| task.id.is_some());
    Ok((document, managed))
}
fn adoption(content: &str, loose: bool) -> Result<String> {
    let working = if loose { loosen(content) } else { content.to_owned() };
    let (document, managed) = classify(&working)?;
    if managed {
        return Err(invalid("source is already managed"));
    }
    if let Some(id) = document.duplicate_ids().first() {
        return Err(Error::DuplicateTaskId(id.clone()));
    }
    // Reserve every existing ID before generating any, including later tasks.
    // `document.source` is the working text: with `loose` it already carries
    // the inserted checkboxes, so IDs land on the converted lines too.
    let mut ids: HashSet<String> = document.tasks.iter().filter_map(|t| t.id.clone()).collect();
    let mut output = document.source.clone();
    for task in document.tasks.iter().rev().filter(|t| t.id.is_none()) {
        let mut nonce = 0u64;
        let id = loop {
            let candidate = revision(
                format!(
                    "{}:{}:{nonce}",
                    revision(content.as_bytes()),
                    task.line_range.start
                )
                .as_bytes(),
            );
            if ids.insert(candidate.clone()) {
                break candidate;
            }
            nonce += 1;
        };
        output.insert_str(task.line_range.end, &format!(" <!--t:{id}-->"));
    }
    parse(output.clone())?;
    Ok(output)
}
/// Reads a source with the loose list reader: bullets become checkboxes in
/// memory, the document is re-classified, and the source is forced read-only
/// because the tasks' byte ranges no longer describe the file on disk.
fn loosen_source(source: &mut Source) {
    let Some(content) = source.content.as_ref() else {
        return;
    };
    let working = loosen(content);
    match classify(&working) {
        Ok((document, _)) => {
            source.info.task_count = document.tasks.len();
            source.info.completion_policy = document.completion_policy();
            source.info.status = "unmanaged".into();
            source.document = Some(document);
        }
        Err(e) => {
            source.info.status = "error".into();
            source.info.error = Some(e.to_string());
            source.document = None;
        }
    }
    source.info.writable = false;
    source.info.loose = true;
}
/// Number of lines `loosen` would change.
fn loose_candidates(content: &str) -> usize {
    loosen(content)
        .lines()
        .zip(content.lines())
        .filter(|(a, b)| a != b)
        .count()
}
/// The loose list reader (Beta). Every unordered bullet outside frontmatter
/// and fenced code that is not already a checkbox becomes `- [ ] text`.
/// Bullet characters, indentation, line endings, BOM and every other line are
/// kept byte for byte, so the diff against the original shows exactly the
/// inserted `[ ] ` on each converted line and nothing else.
pub fn loosen(content: &str) -> String {
    let mut out = String::with_capacity(content.len() + 64);
    let mut in_frontmatter = false;
    let mut fence: Option<(u8, usize)> = None;
    for (index, line) in content.split_inclusive('\n').enumerate() {
        let body = line.trim_end_matches(['\r', '\n']);
        let logical = if index == 0 { body.trim_start_matches('\u{feff}') } else { body };
        if index == 0 && logical == "---" {
            in_frontmatter = true;
            out.push_str(line);
            continue;
        }
        if in_frontmatter {
            if logical == "---" {
                in_frontmatter = false;
            }
            out.push_str(line);
            continue;
        }
        let trimmed = logical.trim_start_matches([' ', '\t']);
        let fence_byte = trimmed
            .as_bytes()
            .first()
            .copied()
            .filter(|byte| matches!(byte, b'`' | b'~'));
        let fence_len = trimmed
            .bytes()
            .take_while(|byte| Some(*byte) == fence_byte)
            .count();
        if let Some((byte, minimum)) = fence {
            if Some(byte) == fence_byte && fence_len >= minimum && trimmed[fence_len..].trim().is_empty() {
                fence = None;
            }
            out.push_str(line);
            continue;
        }
        if fence_byte.is_some() && fence_len >= 3 {
            fence = Some((fence_byte.unwrap(), fence_len));
            out.push_str(line);
            continue;
        }
        let indent_len = logical.len() - trimmed.len();
        let mut bytes = trimmed.bytes();
        let is_bullet = matches!(bytes.next(), Some(b'-' | b'*' | b'+'))
            && matches!(bytes.next(), Some(b' ' | b'\t'));
        if is_bullet {
            let after = trimmed[1..].trim_start_matches([' ', '\t']);
            let already = after.starts_with('[') && after.len() >= 3 && after.as_bytes()[2] == b']';
            if !after.is_empty() && !already && !after.starts_with("---") && !after.starts_with("***") {
                // BOM + indent + bullet char, then the checkbox, then the
                // text and the original line ending.
                let ending_len = line.len() - body.len();
                let bom_len = body.len() - logical.len();
                out.push_str(&line[..bom_len + indent_len + 1]);
                out.push_str(" [ ] ");
                out.push_str(after);
                out.push_str(&line[line.len() - ending_len..]);
                continue;
            }
        }
        out.push_str(line);
    }
    out
}
fn append_snapshot(result: &mut ProjectSnapshot, source: &str, snapshot: DesktopSnapshot) {
    let wrap = |task| ProjectTask {
        task,
        source_path: source.into(),
    };
    if snapshot.completion_policy == CompletionPolicy::Strict {
        result.completion_policy = CompletionPolicy::Strict;
    }
    result
        .columns
        .extend(snapshot.columns.into_iter().map(|column| ProjectColumn {
            column,
            source_path: source.into(),
        }));
    result.tasks.extend(snapshot.tasks.into_iter().map(wrap));
    result
        .today
        .overdue
        .extend(snapshot.today.overdue.into_iter().map(wrap));
    result
        .today
        .today
        .extend(snapshot.today.today.into_iter().map(wrap));
    result
        .today
        .upcoming
        .extend(snapshot.today.upcoming.into_iter().map(wrap));
    result
        .today
        .next
        .extend(snapshot.today.next.into_iter().map(wrap));
}
