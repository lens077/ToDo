use notify::{
    event::{AccessKind, AccessMode},
    Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::mpsc::{self, RecvTimeoutError},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Debug)]
pub(crate) enum WatchSignal {
    Reload,
    Error(String),
}

enum WatchMessage {
    Event(notify::Result<Event>),
    Stop,
}

pub(crate) struct TargetFileWatcher {
    watcher: Option<RecommendedWatcher>,
    sender: mpsc::Sender<WatchMessage>,
    worker: Option<JoinHandle<()>>,
}

impl TargetFileWatcher {
    pub(crate) fn spawn<F>(
        target: impl AsRef<Path>,
        debounce: Duration,
        mut on_signal: F,
    ) -> notify::Result<Self>
    where
        F: FnMut(WatchSignal) + Send + 'static,
    {
        let target = canonical_target(target.as_ref());
        let recursive = target.is_dir();
        let parent = if recursive {
            target.clone()
        } else {
            target
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf()
        };
        let (sender, receiver) = mpsc::channel();
        let event_sender = sender.clone();
        let mut watcher = notify::recommended_watcher(move |event| {
            let _ = event_sender.send(WatchMessage::Event(event));
        })?;
        watcher.watch(
            &parent,
            if recursive {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            },
        )?;
        let worker = thread::spawn(move || {
            run_event_loop(receiver, &target, debounce, &mut on_signal);
        });
        Ok(Self {
            watcher: Some(watcher),
            sender,
            worker: Some(worker),
        })
    }
}

impl Drop for TargetFileWatcher {
    fn drop(&mut self) {
        let _ = self.sender.send(WatchMessage::Stop);
        self.watcher.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn canonical_target(target: &Path) -> PathBuf {
    fs::canonicalize(target).unwrap_or_else(|_| {
        if target.is_absolute() {
            target.to_path_buf()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(target)
        }
    })
}

fn run_event_loop<F>(
    receiver: mpsc::Receiver<WatchMessage>,
    target: &Path,
    debounce: Duration,
    on_signal: &mut F,
) where
    F: FnMut(WatchSignal),
{
    while let Ok(message) = receiver.recv() {
        match message {
            WatchMessage::Stop => return,
            WatchMessage::Event(Err(error)) => on_signal(WatchSignal::Error(error.to_string())),
            WatchMessage::Event(Ok(event)) if event_affects_target(&event, target) => {
                if !debounce_relevant_events(&receiver, target, debounce, on_signal) {
                    return;
                }
            }
            WatchMessage::Event(Ok(_)) => {}
        }
    }
}

fn debounce_relevant_events<F>(
    receiver: &mpsc::Receiver<WatchMessage>,
    target: &Path,
    debounce: Duration,
    on_signal: &mut F,
) -> bool
where
    F: FnMut(WatchSignal),
{
    let mut deadline = Instant::now() + debounce;
    loop {
        // A permanently non-empty queue makes recv_timeout return immediately.
        // Check the wall clock ourselves so unrelated parent-directory noise
        // cannot starve a relevant target event forever.
        if Instant::now() >= deadline {
            on_signal(WatchSignal::Reload);
            return true;
        }
        let timeout = deadline.saturating_duration_since(Instant::now());
        match receiver.recv_timeout(timeout) {
            Ok(WatchMessage::Stop) => return false,
            Ok(WatchMessage::Event(Err(error))) => {
                on_signal(WatchSignal::Error(error.to_string()));
            }
            Ok(WatchMessage::Event(Ok(event))) if event_affects_target(&event, target) => {
                deadline = Instant::now() + debounce;
            }
            Ok(WatchMessage::Event(Ok(_))) => {}
            Err(RecvTimeoutError::Timeout) => {
                on_signal(WatchSignal::Reload);
                return true;
            }
            Err(RecvTimeoutError::Disconnected) => return false,
        }
    }
}

fn event_affects_target(event: &Event, target: &Path) -> bool {
    if event.need_rescan() {
        return true;
    }
    // Reload callbacks read the watched file. Linux reports those reads too;
    // treating them as changes would make each reload schedule the next one.
    if matches!(event.kind, EventKind::Access(_))
        && !matches!(
            event.kind,
            EventKind::Access(AccessKind::Close(AccessMode::Write))
        )
    {
        return false;
    }
    event.paths.iter().any(|path| {
        if path == target {
            return true;
        }
        if !target.is_dir() {
            return false;
        }
        let Ok(relative) = path.strip_prefix(target) else {
            return false;
        };
        let mut parts = relative.components().peekable();
        while let Some(part) = parts.next() {
            let name = part.as_os_str().to_string_lossy();
            if parts.peek().is_some()
                && (name.starts_with('.')
                    || matches!(name.as_ref(), "node_modules" | "target" | "build" | "dist"))
            {
                return false;
            }
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let name = name.to_ascii_lowercase();
        matches!(
            name.as_str(),
            "todo" | "todo.md" | "todo.txt" | ".gitignore" | ".ignore"
        ) || (!name.starts_with('.')
            && (path.is_dir()
                || matches!(
                    event.kind,
                    EventKind::Create(notify::event::CreateKind::Folder)
                        | EventKind::Remove(notify::event::RemoveKind::Folder)
                        | EventKind::Modify(notify::event::ModifyKind::Name(_))
                )))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::mpsc,
        thread,
        time::{Duration, Instant},
    };
    use tempfile::tempdir;

    #[test]
    fn root_watch_discovers_nested_creation_and_deletion() {
        let directory = tempdir().unwrap();
        let nested = directory.path().join("docs");
        fs::create_dir(&nested).unwrap();
        let source = nested.join("todo.TXT");
        let (sender, receiver) = mpsc::channel();
        let observed = source.clone();
        let _watcher =
            TargetFileWatcher::spawn(directory.path(), Duration::from_millis(60), move |signal| {
                if let WatchSignal::Reload = signal {
                    let _ = sender.send(observed.exists());
                }
            })
            .unwrap();
        fs::write(&source, "- [ ] discovered").unwrap();
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            if receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap()
            {
                break;
            }
        }
        fs::remove_file(&source).unwrap();
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            if !receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap()
            {
                break;
            }
        }
    }

    #[test]
    fn recursive_events_ignore_build_outputs_and_detect_source_names() {
        use notify::event::{CreateKind, RemoveKind};
        let directory = tempdir().unwrap();
        let root = directory.path();
        for path in [
            "docs/todo.md",
            "docs/TODO",
            "docs/ToDo.txt",
            "docs/.gitignore",
        ] {
            let event = Event::new(EventKind::Create(CreateKind::File)).add_path(root.join(path));
            assert!(event_affects_target(&event, root), "{path}");
        }
        for path in [
            "node_modules/pkg/TODO.md",
            ".git/TODO",
            "target/TODO.txt",
            "src/main.rs",
        ] {
            let event = Event::new(EventKind::Create(CreateKind::File)).add_path(root.join(path));
            assert!(!event_affects_target(&event, root), "{path}");
        }
        let deleted =
            Event::new(EventKind::Remove(RemoveKind::Folder)).add_path(root.join("removed-folder"));
        assert!(event_affects_target(&deleted, root));
    }

    #[test]
    fn parent_watch_survives_two_atomic_replacements_and_ignores_noise() {
        let directory = tempdir().unwrap();
        let target = directory.path().join("TODO.md");
        fs::write(&target, "initial").unwrap();
        let (sender, receiver) = mpsc::channel();
        let callback_target = target.clone();
        let _watcher = TargetFileWatcher::spawn(
            &target,
            Duration::from_millis(100),
            move |signal| match signal {
                WatchSignal::Reload => {
                    sender
                        .send(fs::read_to_string(&callback_target).unwrap())
                        .unwrap();
                }
                WatchSignal::Error(error) => panic!("watch error: {error}"),
            },
        )
        .unwrap();

        fs::write(directory.path().join("notes.md"), "noise").unwrap();
        fs::write(directory.path().join(".TODO.md.lock"), "noise").unwrap();
        assert!(receiver.recv_timeout(Duration::from_millis(250)).is_err());

        atomic_replace(&target, "first", 1);
        assert_eventually_reads(&receiver, "first");
        assert_eventually_quiet(&receiver);

        atomic_replace(&target, "second", 2);
        assert_eventually_reads(&receiver, "second");
        assert_eventually_quiet(&receiver);
    }

    #[test]
    fn read_only_events_are_ignored_but_mutations_and_rescans_are_kept() {
        use notify::event::{CreateKind, DataChange, Flag, ModifyKind, RemoveKind, RenameMode};

        let target = Path::new("/tmp/TODO.md");
        for access in [
            AccessKind::Any,
            AccessKind::Read,
            AccessKind::Open(AccessMode::Read),
            AccessKind::Open(AccessMode::Any),
            AccessKind::Close(AccessMode::Read),
            AccessKind::Close(AccessMode::Any),
            AccessKind::Other,
        ] {
            let event = Event::new(EventKind::Access(access)).add_path(target.to_path_buf());
            assert!(!event_affects_target(&event, target), "{access:?}");
        }
        for kind in [
            EventKind::Any,
            EventKind::Create(CreateKind::File),
            EventKind::Modify(ModifyKind::Data(DataChange::Content)),
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
            EventKind::Remove(RemoveKind::File),
            EventKind::Access(AccessKind::Close(AccessMode::Write)),
        ] {
            let event = Event::new(kind).add_path(target.to_path_buf());
            assert!(event_affects_target(&event, target), "{kind:?}");
            let unrelated = Event::new(kind).add_path(PathBuf::from("/tmp/notes.md"));
            assert!(!event_affects_target(&unrelated, target), "{kind:?}");
        }
        let rescan = Event::new(EventKind::Access(AccessKind::Read)).set_flag(Flag::Rescan);
        assert!(event_affects_target(&rescan, target));
    }

    #[test]
    fn reading_target_without_a_write_does_not_reload() {
        let directory = tempdir().unwrap();
        let target = directory.path().join("TODO.md");
        fs::write(&target, "unchanged").unwrap();
        let (sender, receiver) = mpsc::channel();
        let _watcher =
            TargetFileWatcher::spawn(&target, Duration::from_millis(20), move |signal| {
                sender.send(signal).unwrap()
            })
            .unwrap();
        for _ in 0..8 {
            assert_eq!(fs::read_to_string(&target).unwrap(), "unchanged");
            thread::sleep(Duration::from_millis(15));
        }
        assert!(matches!(
            receiver.recv_timeout(Duration::from_millis(250)),
            Err(RecvTimeoutError::Timeout)
        ));
    }

    #[test]
    fn reload_callback_reads_do_not_create_a_feedback_loop() {
        use notify::event::{AccessKind, AccessMode, DataChange, ModifyKind};

        let directory = tempdir().unwrap();
        let target = directory.path().join("TODO.md");
        fs::write(&target, "unchanged").unwrap();
        let (sender, receiver) = mpsc::channel();
        let read_sender = sender.clone();
        let (signal_sender, signal_receiver) = mpsc::channel();
        sender
            .send(WatchMessage::Event(Ok(Event::new(
                notify::EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            )
            .add_path(target.clone()))))
            .unwrap();

        let worker = thread::spawn(move || {
            run_event_loop(
                receiver,
                &target,
                Duration::from_millis(20),
                &mut |signal| {
                    if matches!(signal, WatchSignal::Reload) {
                        assert_eq!(fs::read_to_string(&target).unwrap(), "unchanged");
                        // notify's Linux backend watches OPEN and reports it as
                        // Open(Any). Replay that read feedback on every OS.
                        read_sender
                            .send(WatchMessage::Event(Ok(Event::new(EventKind::Access(
                                AccessKind::Open(AccessMode::Any),
                            ))
                            .add_path(target.clone()))))
                            .unwrap();
                    }
                    signal_sender.send(signal).unwrap();
                },
            );
        });

        let first = signal_receiver.recv_timeout(Duration::from_secs(2));
        let second = signal_receiver.recv_timeout(Duration::from_millis(250));
        sender.send(WatchMessage::Stop).unwrap();
        worker.join().unwrap();
        assert!(matches!(first, Ok(WatchSignal::Reload)));
        assert!(
            matches!(second, Err(RecvTimeoutError::Timeout)),
            "a reload's own read must not schedule another reload: {second:?}"
        );
    }

    #[test]
    fn relevant_events_are_not_starved_by_continuous_parent_noise() {
        let target = PathBuf::from("/tmp/TODO.md");
        let (sender, receiver) = mpsc::channel();
        let noise_sender = sender.clone();
        let noise = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(2);
            while Instant::now() < deadline {
                if noise_sender
                    .send(WatchMessage::Event(Ok(Event::new(notify::EventKind::Any))))
                    .is_err()
                {
                    break;
                }
            }
        });
        sender
            .send(WatchMessage::Event(Ok(
                Event::new(notify::EventKind::Any).add_path(target.clone())
            )))
            .unwrap();

        let (signal_sender, signal_receiver) = mpsc::channel();
        let worker = thread::spawn(move || {
            run_event_loop(
                receiver,
                &target,
                Duration::from_millis(10),
                &mut |signal| signal_sender.send(signal).unwrap(),
            );
        });

        assert!(matches!(
            signal_receiver.recv_timeout(Duration::from_millis(250)),
            Ok(WatchSignal::Reload)
        ));
        sender.send(WatchMessage::Stop).unwrap();
        worker.join().unwrap();
        noise.join().unwrap();
    }

    fn assert_eventually_reads(receiver: &mpsc::Receiver<String>, expected: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let value = receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap();
            if value == expected {
                return;
            }
            assert!(Instant::now() < deadline, "never observed {expected:?}");
        }
    }

    fn assert_eventually_quiet(receiver: &mpsc::Receiver<String>) {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match receiver.recv_timeout(Duration::from_millis(250)) {
                Err(RecvTimeoutError::Timeout) => return,
                Ok(value) => assert!(
                    Instant::now() < deadline,
                    "watcher never became quiet after reading {value:?}"
                ),
                Err(error) => panic!("watcher disconnected: {error}"),
            }
        }
    }

    fn atomic_replace(target: &Path, contents: &str, sequence: usize) {
        let temporary = target
            .parent()
            .unwrap()
            .join(format!(".TODO.md.test-tmp-{sequence}"));
        fs::write(&temporary, contents).unwrap();
        fs::rename(temporary, target).unwrap();
    }
}
