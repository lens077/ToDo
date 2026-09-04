use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
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
        let parent = target
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let (sender, receiver) = mpsc::channel();
        let event_sender = sender.clone();
        let mut watcher = notify::recommended_watcher(move |event| {
            let _ = event_sender.send(WatchMessage::Event(event));
        })?;
        watcher.watch(&parent, RecursiveMode::NonRecursive)?;
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
    event.need_rescan() || event.paths.iter().any(|path| path == target)
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

        atomic_replace(&target, "second", 2);
        assert_eventually_reads(&receiver, "second");
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

    fn atomic_replace(target: &Path, contents: &str, sequence: usize) {
        let temporary = target
            .parent()
            .unwrap()
            .join(format!(".TODO.md.test-tmp-{sequence}"));
        fs::write(&temporary, contents).unwrap();
        fs::rename(temporary, target).unwrap();
    }
}
