//! Repository-wide discovery of work that is not yet tracked in `TODO.md`.
//!
//! Two deterministic layers live here:
//!
//! 1. loose GFM checkboxes in other Markdown files, and
//! 2. `TODO:` / `FIXME:` / `HACK:` / `XXX:` markers in source comments.
//!
//! A third layer — deciding whether an existing task matches what the code now
//! does — is a judgment call and belongs to an agent; this module only prepares
//! its input and validates its output (see [`crate::scan_prompt`]).
//!
//! Nothing here writes. Community experience with comment importers is blunt:
//! importing everything at once is what kills trust, because a first run that
//! dumps hundreds of stale markers is indistinguishable from noise. So the scan
//! produces a proposal, deduplicates it against tasks that already exist, and
//! leaves the decision to a human reviewing a diff.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};

use crate::{parse, Document, Result};

/// Markers recognised in source comments, in priority order.
const MARKERS: [&str; 4] = ["TODO", "FIXME", "HACK", "XXX"];

/// Files larger than this are skipped: a multi-megabyte generated file has no
/// hand-written task markers worth importing, and reading it stalls the scan.
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LooseCheckbox {
    pub path: String,
    pub line: usize,
    pub text: String,
    pub completed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeComment {
    pub path: String,
    pub line: usize,
    /// `TODO`, `FIXME`, `HACK` or `XXX`.
    pub marker: String,
    pub text: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ScanStats {
    pub files_scanned: usize,
    pub files_skipped: usize,
    /// Candidates dropped because an equivalent task already exists.
    pub duplicates_filtered: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanReport {
    pub loose_checkboxes: Vec<LooseCheckbox>,
    pub code_comments: Vec<CodeComment>,
    pub stats: ScanStats,
}

impl ScanReport {
    pub fn is_empty(&self) -> bool {
        self.loose_checkboxes.is_empty() && self.code_comments.is_empty()
    }

    pub fn total(&self) -> usize {
        self.loose_checkboxes.len() + self.code_comments.len()
    }
}

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub markdown: bool,
    pub comments: bool,
    /// Task file that owns the repository; never reported as loose.
    pub task_file: PathBuf,
}

impl ScanOptions {
    pub fn all(task_file: impl Into<PathBuf>) -> Self {
        Self {
            markdown: true,
            comments: true,
            task_file: task_file.into(),
        }
    }
}

/// Walks the repository and reports untracked work.
///
/// `existing` is the current task document; its task bodies are used to drop
/// candidates that are already tracked, so re-running a scan converges instead of
/// re-proposing the same items.
pub fn scan(
    root: impl AsRef<Path>,
    existing: &Document,
    options: &ScanOptions,
) -> Result<ScanReport> {
    let root = root.as_ref();
    let known = known_bodies(existing);
    let task_file = options
        .task_file
        .canonicalize()
        .unwrap_or_else(|_| options.task_file.clone());

    let mut report = ScanReport {
        loose_checkboxes: Vec::new(),
        code_comments: Vec::new(),
        stats: ScanStats::default(),
    };

    // WalkBuilder honours .gitignore, .ignore and global excludes, so generated
    // and vendored trees never reach the report.
    // `require_git(false)` matters: by default ignore rules apply only inside a git
    // repository, so scanning a plain directory would walk straight into vendored
    // and generated trees that .gitignore already excludes.
    let walk = WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .require_git(false)
        .parents(true)
        .build();

    for entry in walk.flatten() {
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let path = entry.path();
        if path
            .canonicalize()
            .is_ok_and(|resolved| resolved == task_file)
        {
            continue;
        }
        if entry
            .metadata()
            .ok()
            .is_some_and(|meta| meta.len() > MAX_FILE_BYTES)
        {
            report.stats.files_skipped += 1;
            continue;
        }
        let Ok(content) = std::fs::read_to_string(path) else {
            // Binary or non-UTF-8; not a source of task markers.
            report.stats.files_skipped += 1;
            continue;
        };
        report.stats.files_scanned += 1;
        let display = relative(root, path);

        if options.markdown && is_markdown(path) {
            collect_loose_checkboxes(&display, &content, &known, &mut report);
        } else if options.comments && is_source(path) {
            collect_code_comments(&display, &content, &known, &mut report, openers_for(path));
        }
    }

    report
        .loose_checkboxes
        .sort_by(|a, b| (&a.path, a.line).cmp(&(&b.path, b.line)));
    report
        .code_comments
        .sort_by(|a, b| (&a.path, a.line).cmp(&(&b.path, b.line)));
    Ok(report)
}

/// Normalised bodies of tasks that already exist, used for deduplication.
fn known_bodies(document: &Document) -> HashSet<String> {
    document
        .tasks
        .iter()
        .map(|task| normalize(&task.body))
        .collect()
}

/// Lowercased, whitespace-collapsed, punctuation-trimmed comparison key.
///
/// Deliberately loose: `// TODO: add retry` and a task reading `Add retry.`
/// are the same piece of work, and proposing it twice is what makes an importer
/// feel like noise.
fn normalize(value: &str) -> String {
    let lowered = value.to_lowercase();
    let cleaned: String = lowered
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn collect_loose_checkboxes(
    path: &str,
    content: &str,
    known: &HashSet<String>,
    report: &mut ScanReport,
) {
    // A file that is already a managed task file is not "loose"; parsing succeeds
    // only when it carries the todo-spec frontmatter.
    if parse(content.to_string()).is_ok() {
        return;
    }
    let mut fenced = false;
    for (index, line) in content.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let Some(rest) = checkbox_body(trimmed) else {
            continue;
        };
        // Already carries a stable id: some other managed file owns it.
        if rest.contains("<!--t:") {
            continue;
        }
        let text = rest.trim().to_string();
        if text.is_empty() {
            continue;
        }
        if known.contains(&normalize(&text)) {
            report.stats.duplicates_filtered += 1;
            continue;
        }
        report.loose_checkboxes.push(LooseCheckbox {
            path: path.to_string(),
            line: index + 1,
            completed: trimmed.starts_with("- [x]") || trimmed.starts_with("- [X]"),
            text,
        });
    }
}

fn checkbox_body(trimmed: &str) -> Option<&str> {
    for prefix in ["- [ ] ", "- [x] ", "- [X] ", "* [ ] ", "* [x] ", "* [X] "] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            return Some(rest);
        }
    }
    None
}

fn collect_code_comments(
    path: &str,
    content: &str,
    known: &HashSet<String>,
    report: &mut ScanReport,
    openers: &[&str],
) {
    for (index, line) in content.lines().enumerate() {
        let Some((marker, text)) = comment_marker(line, openers) else {
            continue;
        };
        if text.is_empty() {
            continue;
        }
        if known.contains(&normalize(&text)) {
            report.stats.duplicates_filtered += 1;
            continue;
        }
        report.code_comments.push(CodeComment {
            path: path.to_string(),
            line: index + 1,
            marker: marker.to_string(),
            text,
        });
    }
}

/// Comment openers that apply to a given file, chosen by extension.
///
/// Language-specific rather than a union of every syntax: treating `#` as a
/// comment in Rust would flag every attribute, and `--` in TypeScript would flag
/// every decrement.
fn openers_for(path: &Path) -> &'static [&'static str] {
    const C_LIKE: &[&str] = &["//", "/*"];
    const HASH: &[&str] = &["#"];
    const DASH: &[&str] = &["--", "/*"];

    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "py" | "rb" | "sh" | "bash" | "zsh" => HASH,
        "sql" | "lua" => DASH,
        _ => C_LIKE,
    }
}

/// Extracts `MARKER: text` from a line, but only when the marker sits in a real
/// comment.
///
/// String state is tracked first, because the dominant false positive is a marker
/// inside a string literal — test fixtures and generated templates are full of
/// them. A first run that is mostly false positives does not get a second run.
fn comment_marker(line: &str, openers: &[&str]) -> Option<(&'static str, String)> {
    let start = comment_start(line, openers)?;
    let trimmed = line[start..].trim_start();

    for marker in MARKERS {
        let Some(rest) = strip_marker(trimmed, marker) else {
            continue;
        };
        let text = rest
            .trim_start_matches([':', '(', ')', '-', ' ', '\t'])
            .trim_end_matches(['*', '/', '-', '>', ' ', '\t'])
            .trim();
        return Some((marker, text.to_string()));
    }
    None
}

/// Byte offset just past the first comment opener that is not inside a string.
///
/// Also accepts a leading `*`, which continues a block comment.
fn comment_start(line: &str, openers: &[&str]) -> Option<usize> {
    let trimmed = line.trim_start();
    if trimmed.starts_with('*') && !trimmed.starts_with("*/") {
        return Some(line.len() - trimmed.len() + 1);
    }

    // Iterate by character, not by byte: a byte index can land inside a multi-byte
    // character, and slicing there panics on any line containing non-ASCII text.
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match quote {
            Some(open) => {
                if character == '\\' {
                    escaped = true;
                } else if character == open {
                    quote = None;
                }
            }
            None => {
                if matches!(character, '"' | '\'' | '`') {
                    quote = Some(character);
                } else {
                    for opener in openers {
                        if line[index..].starts_with(opener) {
                            return Some(index + opener.len());
                        }
                    }
                }
            }
        }
    }
    None
}

fn strip_marker<'a>(text: &'a str, marker: &str) -> Option<&'a str> {
    let candidate = text.get(..marker.len())?;
    if !candidate.eq_ignore_ascii_case(marker) {
        return None;
    }
    let rest = &text[marker.len()..];
    // `TODOS` or `TODOist` is not a marker; require a delimiter after it.
    match rest.chars().next() {
        None => Some(rest),
        Some(next) if !next.is_alphanumeric() && next != '_' => Some(rest),
        Some(_) => None,
    }
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| {
            value.eq_ignore_ascii_case("md") || value.eq_ignore_ascii_case("markdown")
        })
}

fn is_source(path: &Path) -> bool {
    const EXTENSIONS: [&str; 26] = [
        "rs", "ts", "tsx", "js", "jsx", "go", "py", "java", "kt", "swift", "c", "h", "cc", "cpp",
        "hpp", "cs", "rb", "php", "scala", "sh", "bash", "zsh", "sql", "vue", "svelte", "lua",
    ];
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| EXTENSIONS.iter().any(|ext| value.eq_ignore_ascii_case(ext)))
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Builds the document text that would result from importing a scan report.
///
/// Returns the proposed file contents; it never writes. Callers are expected to
/// show the result as a diff and let a human confirm, because an import can add
/// hundreds of lines at once and a wrong bulk edit is the costliest kind.
///
/// Each imported task records where it came from in its note line, so a reviewer
/// can jump to the source and the provenance survives in the file itself.
pub fn import_text(document: &Document, report: &ScanReport, column: &str) -> Result<String> {
    if report.is_empty() {
        return Ok(document.source.clone());
    }
    let mut taken: HashSet<String> = document
        .tasks
        .iter()
        .filter_map(|task| task.id.clone())
        .collect();

    let mut lines = Vec::new();
    for item in &report.loose_checkboxes {
        let id = fresh_id(&mut taken)?;
        let state = if item.completed { "x" } else { " " };
        lines.push(format!(
            "- [{state}] {} <!--t:{id}--> #scan-md\n      ↳ 来源 {}:{} <!--n:{id}-->",
            item.text, item.path, item.line
        ));
    }
    for item in &report.code_comments {
        let id = fresh_id(&mut taken)?;
        let tag = item.marker.to_lowercase();
        lines.push(format!(
            "- [ ] {} <!--t:{id}--> #scan-{tag}\n      ↳ 来源 {}:{} <!--n:{id}-->",
            item.text, item.path, item.line
        ));
    }

    let block = lines.join("\n");
    let newline = document.line_ending.as_str();
    let block = block.replace('\n', newline);

    // Append inside the target column when it exists, otherwise at end of file, so
    // an import never silently lands in the wrong place.
    match document
        .columns
        .iter()
        .find(|candidate| candidate.id == column)
    {
        Some(target) => {
            let insert_at = trimmed_column_end(&document.source, target.content_range.end);
            let mut output = String::with_capacity(document.source.len() + block.len() + 8);
            output.push_str(&document.source[..insert_at]);
            if !output.is_empty() && !output.ends_with('\n') {
                output.push_str(newline);
            }
            output.push_str(&block);
            output.push_str(newline);
            output.push_str(&document.source[insert_at..]);
            Ok(output)
        }
        None => {
            let mut output = document.source.clone();
            if !output.is_empty() && !output.ends_with('\n') {
                output.push_str(newline);
            }
            output.push_str(&block);
            output.push_str(newline);
            Ok(output)
        }
    }
}

/// Offset of the end of a column's content, ignoring trailing blank lines so an
/// import does not leave a gap between the existing tasks and the new ones.
fn trimmed_column_end(source: &str, end: usize) -> usize {
    let mut at = end.min(source.len());
    while at > 0 {
        let candidate = source[..at].trim_end_matches(['\n', '\r']);
        if candidate.len() == at {
            break;
        }
        at = candidate.len();
    }
    at
}

fn fresh_id(taken: &mut HashSet<String>) -> Result<String> {
    let id = crate::generate_task_id(taken)?;
    taken.insert(id.clone());
    Ok(id)
}
