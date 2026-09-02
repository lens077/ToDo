//! Guards the cost model of `DesktopSnapshot` derivation against git history.
//!
//! Deriving a snapshot needs, for every task without explicit evidence, the
//! commit that introduced its stable ID. The obvious implementation asks git
//! once per task (`git log -S<id>`), which is O(history × tasks) subprocesses
//! and makes a board of 40 tasks over a year of history take seconds to open.
//! The implementation instead reads the history once and matches every wanted
//! ID against that single stream.
//!
//! These tests pin that shape by *counting subprocesses* rather than by timing
//! the call. A wall-clock budget would be flaky on loaded CI machines and would
//! silently pass on a fast one; an invocation count is exact, so a regression to
//! per-task git calls fails the same way on every machine.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Mutex, MutexGuard, OnceLock},
};
use tempfile::TempDir;

/// Serialises every test in this file.
///
/// `PATH` and `GIT_SPY_LOG` are process-global, and `cargo test` runs the
/// tests in this file on parallel threads, so a snapshot taken by one test
/// would be recorded by another's spy and the counts would be nondeterministic.
/// Spies take this lock for their lifetime; tests without a spy take it too, so
/// none of their git calls can land inside a recording window.
fn path_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Runs a real git command and asserts it succeeded.
///
/// Fixture calls must never be counted as snapshot cost, even when another
/// test's spy currently owns `PATH`: the shim only records when `GIT_SPY_LOG`
/// is set, and this helper strips it.
fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(dir)
        .env_remove("GIT_SPY_LOG")
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {}: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// A stable ID for task `index`, using only the characters valid in `<!--t:…-->`.
fn task_id(index: usize) -> String {
    // Six lowercase letters keeps every ID the same width and unique per index.
    let mut id = String::new();
    let mut value = index;
    for _ in 0..6 {
        id.insert(0, char::from(b'a' + u8::try_from(value % 26).unwrap()));
        value /= 26;
    }
    id
}

fn todo_document(task_count: usize) -> String {
    let mut text =
        String::from("---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n---\n\n## Todo\n");
    for index in 0..task_count {
        text.push_str(&format!(
            "- [ ] task {index} <!--t:{}--> !2\n",
            task_id(index)
        ));
    }
    text
}

/// Builds a repository whose TODO.md grew one task per commit.
///
/// This is the realistic shape: the file has a long per-line history, so a
/// per-task `git log` pays for the whole history once per task.
fn repository_with_history(task_count: usize) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["config", "user.email", "test@example.com"]);
    git(root, &["config", "user.name", "Test"]);
    // Keep commits deterministic: fixed identity and fixed timestamps mean the
    // same input always produces the same history.
    git(root, &["config", "commit.gpgsign", "false"]);

    for count in 1..=task_count {
        fs::write(root.join("TODO.md"), todo_document(count)).unwrap();
        git(root, &["add", "TODO.md"]);
        git(
            root,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-q",
                "--date=2026-01-01T00:00:00Z",
                "-m",
                &format!("add task {count}"),
            ],
        );
    }
    dir
}

/// Installs a `git` shim earlier on `PATH` that appends each invocation to a log
/// and then delegates to the real git, so behaviour is unchanged and only
/// observation is added.
///
/// Owns the `PATH` lock for its whole lifetime: the shim lives in a `TempDir`
/// that is deleted on drop, so a concurrently installed spy must not be able to
/// publish a `PATH` entry that this one then removes from under it.
struct GitSpy {
    _guard: MutexGuard<'static, ()>,
    _dir: TempDir,
    bin: PathBuf,
    log: PathBuf,
}

impl GitSpy {
    fn install() -> Self {
        let guard = path_lock();
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        let log = dir.path().join("calls.log");
        let real = String::from_utf8_lossy(
            &Command::new("sh")
                .args(["-c", "command -v git"])
                .output()
                .unwrap()
                .stdout,
        )
        .trim()
        .to_string();
        assert!(!real.is_empty(), "git must be installed to run this test");
        let shim = bin.join("git");
        fs::write(
            &shim,
            format!(
                "#!/bin/sh\n[ -n \"$GIT_SPY_LOG\" ] && printf '%s\\n' \"$*\" >> \"$GIT_SPY_LOG\"\nexec {real} \"$@\"\n",
                real = shell_quote(Path::new(&real)),
            ),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&shim, fs::Permissions::from_mode(0o755)).unwrap();
        }
        fs::write(&log, "").unwrap();
        Self {
            _guard: guard,
            _dir: dir,
            bin,
            log,
        }
    }

    /// Runs `body` with the shim ahead of the real git on `PATH` and returns the
    /// git command lines it observed.
    ///
    /// The spy already holds the `PATH` lock, so the log contains the
    /// subprocesses of `body` alone even though other tests run concurrently.
    /// Build any fixture repository *before* calling this: setup commits made
    /// inside the window would be counted as snapshot cost.
    fn record<T>(&self, body: impl FnOnce() -> T) -> (T, Vec<String>) {
        fs::write(&self.log, "").unwrap();
        let previous = std::env::var_os("PATH");
        let mut entries = vec![self.bin.clone()];
        if let Some(previous) = &previous {
            entries.extend(std::env::split_paths(previous));
        }
        let patched = std::env::join_paths(entries).unwrap();
        // Safety: the whole suite for this file is single-threaded (see the
        // `#[test]` bodies below, which never spawn threads), so no other thread
        // observes the environment mid-swap.
        std::env::set_var("PATH", &patched);
        std::env::set_var("GIT_SPY_LOG", &self.log);
        let value = body();
        std::env::remove_var("GIT_SPY_LOG");
        match previous {
            Some(previous) => std::env::set_var("PATH", previous),
            None => std::env::remove_var("PATH"),
        }
        let calls = fs::read_to_string(&self.log)
            .unwrap()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(str::to_string)
            .collect();
        (value, calls)
    }
}

fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', r"'\''"))
}

/// The number of git subprocesses a snapshot spends must not depend on how many
/// tasks the board holds.
///
/// A per-task `git log -S<id>` regression turns 2 calls into 2 + N and shows up
/// here as an exact count mismatch, with no timing involved.
#[test]
fn snapshot_git_cost_is_constant_in_the_number_of_tasks() {
    let spy = GitSpy::install();

    // Build both repositories up front: creating history spends git calls of its
    // own, and only the snapshot's calls may land inside a recording window.
    let small = repository_with_history(3);
    let large = repository_with_history(24);

    let (small_snapshot, small_calls) = spy.record(|| {
        let mut backend = todo_core::DesktopBackend::new(std::time::Duration::from_secs(2));
        backend.open_project(small.path()).unwrap()
    });

    let (large_snapshot, large_calls) = spy.record(|| {
        let mut backend = todo_core::DesktopBackend::new(std::time::Duration::from_secs(2));
        backend.open_project(large.path()).unwrap()
    });

    assert_eq!(small_snapshot.tasks.len(), 3);
    assert_eq!(large_snapshot.tasks.len(), 24);

    assert_eq!(
        small_calls.len(),
        large_calls.len(),
        "git subprocess count must not grow with task count; \
         3 tasks ran {small_calls:?} and 24 tasks ran {large_calls:?}"
    );

    // Pin the absolute shape too: one HEAD lookup plus one history read. This
    // catches a regression that scans history twice even when both sizes agree.
    assert_eq!(
        large_calls.len(),
        2,
        "expected exactly one HEAD resolve and one history read, got {large_calls:?}"
    );
    assert_eq!(
        large_calls
            .iter()
            .filter(|call| call.contains("log"))
            .count(),
        1,
        "history must be read exactly once for the whole board, got {large_calls:?}"
    );
}

/// Reading history once must still be *correct*: every task gets the commit that
/// actually introduced it, not merely the first or the newest commit.
///
/// Without this the cheap implementation could "pass" the cost test by
/// returning a single shared answer for every task.
#[test]
fn one_history_read_still_attributes_each_task_to_its_own_commit() {
    let _serial = path_lock();
    let repository = repository_with_history(5);
    let root = repository.path();
    let commits: Vec<String> = git(root, &["log", "--reverse", "--format=%H", "--", "TODO.md"])
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(commits.len(), 5, "one commit per added task");

    let mut backend = todo_core::DesktopBackend::new(std::time::Duration::from_secs(2));
    let snapshot = backend.open_project(root).unwrap();
    let head = git(root, &["rev-parse", "HEAD"]);

    for (index, task) in snapshot.tasks.iter().enumerate() {
        let expected_from = &commits[index];
        match &task.evidence_availability {
            todo_core::EvidenceAvailability::Available { from, to } => {
                assert_eq!(
                    from, expected_from,
                    "task {index} ({:?}) must point at the commit that introduced it",
                    task.id
                );
                assert_eq!(to, &head, "evidence must end at HEAD");
            }
            // The last task was introduced by HEAD itself, so HEAD..HEAD is empty
            // and correctly reported as unavailable rather than as a bogus range.
            todo_core::EvidenceAvailability::Unavailable { reason } => {
                assert_eq!(
                    expected_from, &head,
                    "task {index} ({:?}) was unavailable for the wrong reason: {reason}",
                    task.id
                );
            }
        }
    }
}

fn commit_task(root: &Path, file: &str, count: usize) {
    fs::write(root.join(file), todo_document(count)).unwrap();
    git(root, &["add", file]);
    git(
        root,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-q",
            "-m",
            &format!("add task {count}"),
        ],
    );
}

fn from_of(task: &todo_core::DesktopTask) -> Option<&str> {
    match &task.evidence_availability {
        todo_core::EvidenceAvailability::Available { from, .. } => Some(from.as_str()),
        todo_core::EvidenceAvailability::Unavailable { .. } => None,
    }
}

/// Every refresh of the board asks for a snapshot; the history must not be
/// re-read while HEAD has not moved.
#[test]
fn snapshot_reuses_cached_introductions_while_head_is_unchanged() {
    let spy = GitSpy::install();
    let repository = repository_with_history(4);
    let mut backend = todo_core::DesktopBackend::new(std::time::Duration::from_secs(2));

    let (first, _) = spy.record(|| backend.open_project(repository.path()).unwrap());
    let (second, calls) = spy.record(|| backend.snapshot().unwrap());

    assert!(
        !calls.iter().any(|call| call.contains("log")),
        "HEAD did not move, so history must come from the cache: {calls:?}"
    );
    assert_eq!(
        calls.len(),
        1,
        "only the HEAD lookup may run on a repeat snapshot: {calls:?}"
    );
    for (before, after) in first.tasks.iter().zip(&second.tasks) {
        assert_eq!(from_of(before), from_of(after), "{:?}", before.id);
    }
}

/// When HEAD moves, only the commits since the last scan are read, and the
/// answers for tasks introduced earlier are kept.
#[test]
fn snapshot_scans_only_the_new_commits_after_head_moves() {
    let spy = GitSpy::install();
    let repository = repository_with_history(3);
    let root = repository.path();
    let mut backend = todo_core::DesktopBackend::new(std::time::Duration::from_secs(2));
    let first = backend.open_project(root).unwrap();
    let old_head = git(root, &["rev-parse", "HEAD"]);
    commit_task(root, "TODO.md", 4);
    let new_head = git(root, &["rev-parse", "HEAD"]);

    let (second, calls) = spy.record(|| backend.snapshot().unwrap());

    let logs: Vec<_> = calls.iter().filter(|call| call.contains("log")).collect();
    assert_eq!(
        logs.len(),
        1,
        "exactly one incremental history read: {calls:?}"
    );
    assert!(
        logs[0].contains(&format!("{old_head}..")),
        "the read must start where the previous scan stopped: {calls:?}"
    );
    // Tasks 0..2 keep their original commits; task 2 moved from "introduced by
    // HEAD" (unavailable) to a real range now that HEAD advanced past it.
    assert_eq!(from_of(&second.tasks[0]), from_of(&first.tasks[0]));
    assert_eq!(from_of(&second.tasks[1]), from_of(&first.tasks[1]));
    assert_eq!(from_of(&second.tasks[2]), Some(old_head.as_str()));
    assert_eq!(
        from_of(&second.tasks[3]),
        None,
        "task 3 is introduced by HEAD"
    );
    match &second.tasks[3].evidence_availability {
        todo_core::EvidenceAvailability::Unavailable { reason } => {
            assert!(reason.contains("HEAD..HEAD"), "{reason}")
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(new_head, git(root, &["rev-parse", "HEAD"]));
}

/// The task file is whatever the project opened, not a hard-coded `TODO.md`.
#[test]
fn evidence_follows_the_opened_task_file_name() {
    let _serial = path_lock();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["config", "user.email", "test@example.com"]);
    git(root, &["config", "user.name", "Test"]);
    for count in 1..=3 {
        commit_task(root, "tasks.md", count);
    }
    let commits: Vec<String> = git(root, &["log", "--reverse", "--format=%H", "--", "tasks.md"])
        .lines()
        .map(str::to_string)
        .collect();

    let mut backend = todo_core::DesktopBackend::new(std::time::Duration::from_secs(2));
    let snapshot = backend.open_project(root.join("tasks.md")).unwrap();

    assert_eq!(from_of(&snapshot.tasks[0]), Some(commits[0].as_str()));
    assert_eq!(from_of(&snapshot.tasks[1]), Some(commits[1].as_str()));
}

/// An existing `🔗from..to` is only offered as a suggestion when `from` is a
/// commit in this repository, and all such starts are checked in one process.
#[test]
fn existing_evidence_starts_are_verified_in_one_batch_and_cached() {
    let repository = repository_with_history(3);
    let root = repository.path();
    let commits: Vec<String> = git(root, &["log", "--reverse", "--format=%H", "--", "TODO.md"])
        .lines()
        .map(str::to_string)
        .collect();
    let short = &commits[0][..10];
    let mut text =
        String::from("---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n---\n\n## Todo\n");
    text.push_str(&format!(
        "- [ ] linked <!--t:aaaaaa--> 🔗{short}..{}\n",
        commits[1]
    ));
    text.push_str(&format!(
        "- [ ] linked too <!--t:bbbbbb--> 🔗{}..{}\n",
        commits[1], commits[2]
    ));
    text.push_str("- [ ] broken <!--t:cccccc--> 🔗deadbeefdead..HEAD\n");
    fs::write(root.join("TODO.md"), text).unwrap();

    let spy = GitSpy::install();
    let mut backend = todo_core::DesktopBackend::new(std::time::Duration::from_secs(2));
    let (snapshot, calls) = spy.record(|| backend.open_project(root).unwrap());

    assert!(!calls.iter().any(|call| call.contains("log")), "{calls:?}");
    assert_eq!(
        calls.len(),
        2,
        "one HEAD lookup plus one batched commit check, got {calls:?}"
    );
    // The abbreviated start is offered in its full form so the write validates.
    assert_eq!(from_of(&snapshot.tasks[0]), Some(commits[0].as_str()));
    assert_eq!(from_of(&snapshot.tasks[1]), Some(commits[1].as_str()));
    match &snapshot.tasks[2].evidence_availability {
        todo_core::EvidenceAvailability::Unavailable { reason } => {
            assert!(reason.contains("deadbeefdead"), "{reason}")
        }
        other => panic!("{other:?}"),
    }

    let (_, again) = spy.record(|| backend.snapshot().unwrap());
    assert_eq!(again.len(), 1, "resolutions are cached: {again:?}");
}

/// A board whose tasks all carry explicit evidence needs no history read at all.
///
/// This pins the early-out that keeps the common "everything is already linked"
/// case free, and would catch a regression that always shells out to `git log`.
#[test]
fn snapshot_skips_the_history_read_when_no_task_needs_an_introduction() {
    // All git setup happens before the spy records, so the window holds only the
    // snapshot's own subprocesses.
    let repository = repository_with_history(3);
    let root = repository.path();
    let commits: Vec<String> = git(root, &["log", "--reverse", "--format=%H", "--", "TODO.md"])
        .lines()
        .map(str::to_string)
        .collect();

    let mut text =
        String::from("---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n---\n\n## Todo\n");
    for index in 0..3 {
        text.push_str(&format!(
            "- [ ] task {index} <!--t:{}--> 🔗{}..{}\n",
            task_id(index),
            commits[0],
            commits[2],
        ));
    }
    fs::write(root.join("TODO.md"), text).unwrap();

    let spy = GitSpy::install();
    let (_snapshot, calls) = spy.record(|| {
        let mut backend = todo_core::DesktopBackend::new(std::time::Duration::from_secs(2));
        backend.open_project(root).unwrap()
    });

    assert!(
        !calls.iter().any(|call| call.contains("log")),
        "no task needs an introduction, so history must not be read: {calls:?}"
    );
}
