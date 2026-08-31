//! Version history for task files.
//!
//! Every write snapshots the file's previous contents first, so any change can be
//! undone — including a change made by a bulk operation such as a repository scan.
//! History is local recovery state, not project truth: it lives in `.todo/` beside
//! the task file and ignores itself out of git.
//!
//! Snapshots are numbered, not content-addressed. A hash collision in a content
//! addressed store silently returns the wrong file contents, which is the one
//! failure a recovery mechanism must never have.

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    error::{io_error, Error, Result},
    Document,
};

/// Directory that holds all local recovery state, relative to the task file.
pub const STATE_DIR: &str = ".todo";
const HISTORY_DIR: &str = "history";
const INDEX_FILE: &str = "index.jsonl";

/// How many snapshots per file to keep by default.
pub const DEFAULT_KEEP: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryEntry {
    pub version: u64,
    /// RFC 3339 timestamp of when the snapshot was taken.
    pub at: String,
    /// Task file this snapshot belongs to, relative to the state directory's parent.
    pub file: String,
    /// Operation that was about to overwrite the file, e.g. `progress` or `scan-apply`.
    pub op: String,
    pub bytes: usize,
    /// Snapshot file name inside the history directory.
    pub blob: String,
}

pub struct History {
    root: PathBuf,
}

impl History {
    /// Opens the history store that governs `path`, creating nothing until a write.
    pub fn for_file(path: impl AsRef<Path>) -> Self {
        let parent = path
            .as_ref()
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        Self {
            root: parent.join(STATE_DIR).join(HISTORY_DIR),
        }
    }

    pub fn directory(&self) -> &Path {
        &self.root
    }

    fn index_path(&self) -> PathBuf {
        self.root.join(INDEX_FILE)
    }

    /// Records the current contents of `path` as the newest snapshot.
    ///
    /// Called before a write, so the returned entry always describes the state the
    /// caller is about to replace. A missing file records nothing: there is no prior
    /// state to restore.
    pub fn record(&self, path: impl AsRef<Path>, op: &str) -> Result<Option<HistoryEntry>> {
        let path = path.as_ref();
        let Ok(content) = fs::read(path) else {
            return Ok(None);
        };
        self.ensure_layout()?;
        let version = self.next_version()?;
        let blob = format!("v{version:06}.snap");
        let blob_path = self.root.join(&blob);
        crate::atomic_write(&blob_path, &content)?;

        let entry = HistoryEntry {
            version,
            at: chrono::Local::now().to_rfc3339(),
            file: path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("TODO.md")
                .to_string(),
            op: op.to_string(),
            bytes: content.len(),
            blob,
        };
        self.append_index(&entry)?;
        Ok(Some(entry))
    }

    /// All snapshots, oldest first.
    pub fn entries(&self) -> Result<Vec<HistoryEntry>> {
        let index = self.index_path();
        let Ok(text) = fs::read_to_string(&index) else {
            return Ok(Vec::new());
        };
        let mut entries = Vec::new();
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            // A truncated final line (power loss mid-append) must not make the whole
            // history unreadable; skip what cannot be parsed and keep the rest.
            if let Ok(entry) = serde_json::from_str::<HistoryEntry>(line) {
                entries.push(entry);
            }
        }
        entries.sort_by_key(|entry| entry.version);
        Ok(entries)
    }

    pub fn entry(&self, version: u64) -> Result<HistoryEntry> {
        self.entries()?
            .into_iter()
            .find(|entry| entry.version == version)
            .ok_or_else(|| Error::HistoryVersionNotFound(version))
    }

    /// Contents captured by a snapshot.
    pub fn read(&self, version: u64) -> Result<String> {
        let entry = self.entry(version)?;
        let blob = self.root.join(&entry.blob);
        let bytes = fs::read(&blob).map_err(|source| io_error(&blob, source))?;
        String::from_utf8(bytes).map_err(|_| Error::InvalidUtf8)
    }

    /// Deletes all but the newest `keep` snapshots, returning how many were removed.
    pub fn prune(&self, keep: usize) -> Result<usize> {
        let entries = self.entries()?;
        if entries.len() <= keep {
            return Ok(0);
        }
        let cutoff = entries.len() - keep;
        let (drop, keep_entries) = entries.split_at(cutoff);
        for entry in drop {
            let _ = fs::remove_file(self.root.join(&entry.blob));
        }
        let mut rewritten = String::new();
        for entry in keep_entries {
            rewritten.push_str(&serde_json::to_string(entry).map_err(Error::history_encode)?);
            rewritten.push('\n');
        }
        crate::atomic_write(self.index_path(), rewritten.as_bytes())?;
        Ok(drop.len())
    }

    fn ensure_layout(&self) -> Result<()> {
        fs::create_dir_all(&self.root).map_err(|source| io_error(&self.root, source))?;
        // History is machine-local recovery state. Self-ignoring keeps it out of the
        // user's commits without editing the user's own .gitignore.
        let marker = self.root.parent().unwrap_or(&self.root).join(".gitignore");
        if !marker.exists() {
            crate::atomic_write(&marker, b"*\n")?;
        }
        Ok(())
    }

    fn next_version(&self) -> Result<u64> {
        Ok(self
            .entries()?
            .last()
            .map_or(1, |entry| entry.version.saturating_add(1)))
    }

    fn append_index(&self, entry: &HistoryEntry) -> Result<()> {
        use std::io::Write;
        let path = self.index_path();
        let mut line = serde_json::to_string(entry).map_err(Error::history_encode)?;
        line.push('\n');
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|source| io_error(&path, source))?;
        file.write_all(line.as_bytes())
            .map_err(|source| io_error(&path, source))?;
        file.sync_all().map_err(|source| io_error(&path, source))?;
        Ok(())
    }
}

/// A single-file left/right comparison, ready for a confirmation UI.
#[derive(Debug, Clone, Serialize)]
pub struct TextDiff {
    pub hunks: Vec<DiffHunk>,
    pub added: usize,
    pub removed: usize,
}

impl TextDiff {
    pub fn is_empty(&self) -> bool {
        self.added == 0 && self.removed == 0
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffHunk {
    /// 1-based line number in the original text, when the line exists there.
    pub left_line: Option<usize>,
    /// 1-based line number in the proposed text, when the line exists there.
    pub right_line: Option<usize>,
    pub kind: DiffKind,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiffKind {
    Context,
    Added,
    Removed,
}

/// Line diff between the current file and a proposed replacement.
///
/// Produced in Rust rather than the UI so the rows a user confirms are computed
/// from the same bytes the writer will use.
pub fn diff_lines(before: &str, after: &str) -> TextDiff {
    let left: Vec<&str> = before.lines().collect();
    let right: Vec<&str> = after.lines().collect();
    let table = lcs_table(&left, &right);

    let mut hunks = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < left.len() && j < right.len() {
        if left[i] == right[j] {
            hunks.push(DiffHunk {
                left_line: Some(i + 1),
                right_line: Some(j + 1),
                kind: DiffKind::Context,
                text: left[i].to_string(),
            });
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            hunks.push(DiffHunk {
                left_line: Some(i + 1),
                right_line: None,
                kind: DiffKind::Removed,
                text: left[i].to_string(),
            });
            i += 1;
        } else {
            hunks.push(DiffHunk {
                left_line: None,
                right_line: Some(j + 1),
                kind: DiffKind::Added,
                text: right[j].to_string(),
            });
            j += 1;
        }
    }
    while i < left.len() {
        hunks.push(DiffHunk {
            left_line: Some(i + 1),
            right_line: None,
            kind: DiffKind::Removed,
            text: left[i].to_string(),
        });
        i += 1;
    }
    while j < right.len() {
        hunks.push(DiffHunk {
            left_line: None,
            right_line: Some(j + 1),
            kind: DiffKind::Added,
            text: right[j].to_string(),
        });
        j += 1;
    }

    let added = hunks
        .iter()
        .filter(|hunk| hunk.kind == DiffKind::Added)
        .count();
    let removed = hunks
        .iter()
        .filter(|hunk| hunk.kind == DiffKind::Removed)
        .count();
    TextDiff {
        hunks,
        added,
        removed,
    }
}

/// Diff a parsed document against a proposed replacement.
pub fn diff_document(document: &Document, proposed: &str) -> TextDiff {
    diff_lines(&document.source, proposed)
}

fn lcs_table(left: &[&str], right: &[&str]) -> Vec<Vec<usize>> {
    let mut table = vec![vec![0usize; right.len() + 1]; left.len() + 1];
    for i in (0..left.len()).rev() {
        for j in (0..right.len()).rev() {
            table[i][j] = if left[i] == right[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }
    table
}
