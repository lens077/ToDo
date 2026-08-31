use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use fs2::FileExt;

use crate::{
    apply_changes, atomic_write, error::io_error, parse_file, Change, Document, Error, History,
    Result,
};

pub struct LockedWriter {
    path: PathBuf,
    timeout: Duration,
}

pub struct FileLockGuard {
    file: File,
    path: PathBuf,
}

impl LockedWriter {
    pub fn new(path: impl AsRef<Path>, timeout: Duration) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            timeout,
        }
    }

    /// Locks, then reads the current file, derives changes, validates, and atomically writes.
    /// The read occurs inside the lock so queued writers never operate on stale snapshots.
    pub fn update<F>(&self, derive: F) -> Result<Document>
    where
        F: FnOnce(&Document) -> Result<Vec<Change>>,
    {
        self.update_labeled("update", derive)
    }

    /// As [`LockedWriter::update`], recording `op` as the reason in version history.
    ///
    /// The snapshot is taken inside the lock and only once the write is known to be
    /// needed, so history records exactly the state each write replaced.
    pub fn update_labeled<F>(&self, op: &str, derive: F) -> Result<Document>
    where
        F: FnOnce(&Document) -> Result<Vec<Change>>,
    {
        let _guard = self.lock()?;
        let document = parse_file(&self.path)?;
        let changes = derive(&document)?;
        let output = apply_changes(&document, &changes)?;
        if output != document.source {
            // A no-op write would otherwise fill history with identical snapshots and
            // push real restore points out of the retention window.
            History::for_file(&self.path).record(&self.path, op)?;
        }
        atomic_write(&self.path, output.as_bytes())?;
        parse_file(&self.path)
    }

    /// Replaces the whole file with `content` after snapshotting the previous state.
    ///
    /// Used by bulk operations whose result is computed as text (repository scan,
    /// history restore) rather than as a list of [`Change`] values.
    pub fn replace_all(&self, op: &str, content: &str) -> Result<Document> {
        let _guard = self.lock()?;
        let current = std::fs::read_to_string(&self.path).unwrap_or_default();
        if current == content {
            return parse_file(&self.path);
        }
        // Reject a replacement that would not parse, before it reaches disk.
        crate::parse(content.to_string())?;
        History::for_file(&self.path).record(&self.path, op)?;
        atomic_write(&self.path, content.as_bytes())?;
        parse_file(&self.path)
    }

    pub fn lock(&self) -> Result<FileLockGuard> {
        let lock_path = lock_path(&self.path);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|source| io_error(&lock_path, source))?;
        let started = Instant::now();
        loop {
            match file.try_lock_exclusive() {
                Ok(()) => {
                    return Ok(FileLockGuard {
                        file,
                        path: lock_path,
                    })
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if started.elapsed() >= self.timeout {
                        return Err(Error::LockTimeout {
                            path: lock_path,
                            timeout_ms: self.timeout.as_millis().min(u64::MAX as u128) as u64,
                        });
                    }
                    thread::sleep(Duration::from_millis(5));
                }
                Err(source) => return Err(io_error(&lock_path, source)),
            }
        }
    }
}

impl Drop for FileLockGuard {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.file);
    }
}

impl FileLockGuard {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn lock_path(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("TODO.md");
    parent.join(format!(".{name}.lock"))
}
