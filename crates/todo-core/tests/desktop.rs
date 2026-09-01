use chrono::{DateTime, NaiveDate, Utc};
use std::{
    fs,
    sync::{Arc, Barrier},
    thread,
    time::{Duration, Instant},
};
use tempfile::tempdir;
use todo_core::{
    CaptureProject, CaptureStore, Clock, DesktopBackend, DesktopIntent, MoveTarget, TriageResult,
};

#[derive(Debug)]
struct FakeClock {
    today: NaiveDate,
    now: DateTime<Utc>,
}

impl FakeClock {
    fn fixed() -> Arc<Self> {
        Arc::new(Self {
            today: NaiveDate::from_ymd_opt(2026, 1, 2).unwrap(),
            now: DateTime::parse_from_rfc3339("2026-01-02T03:04:05Z")
                .unwrap()
                .with_timezone(&Utc),
        })
    }
}

impl Clock for FakeClock {
    fn local_today(&self) -> NaiveDate {
        self.today
    }

    fn now_utc(&self) -> DateTime<Utc> {
        self.now
    }
}

fn project() -> (tempfile::TempDir, DesktopBackend) {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("TODO.md"),
        "---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n  - {id: done, name: Done, type: done}\n---\n\n## Todo\n- [ ] overdue <!--t:aaaaaa--> 📅2026-01-01\n- [ ] today <!--t:bbbbbb--> 📅2026-01-02\n- [ ] day seven <!--t:cccccc--> 📅2026-01-09\n- [ ] day eight <!--t:eeeeee--> 📅2026-01-10\n- [x] hidden <!--t:dddddd--> 📅2026-01-02\n\n## Done\n",
    )
    .unwrap();
    let mut backend = DesktopBackend::with_clock(Duration::from_secs(2), FakeClock::fixed());
    backend.open_project(dir.path()).unwrap();
    (dir, backend)
}

#[test]
fn backend_derives_local_today_and_only_the_next_seven_days() {
    let (dir, backend) = project();
    let snapshot = backend.snapshot().unwrap();
    assert_eq!(snapshot.today.overdue.len(), 1);
    assert_eq!(snapshot.today.today.len(), 1);
    assert_eq!(snapshot.today.upcoming.len(), 1);
    assert_eq!(snapshot.today.upcoming[0].body, "day seven");
    assert_eq!(snapshot.tasks.len(), 5);

    let mut text = fs::read_to_string(dir.path().join("TODO.md")).unwrap();
    text = text.replace("today", "externally changed");
    fs::write(dir.path().join("TODO.md"), text).unwrap();
    assert!(backend
        .snapshot()
        .unwrap()
        .tasks
        .iter()
        .any(|task| task.body == "externally changed"));
}

#[test]
fn backend_mutations_return_authoritative_snapshot() {
    let (_dir, backend) = project();
    let added = backend
        .mutate(DesktopIntent::AddTask {
            body: "new".into(),
            column: Some("todo".into()),
        })
        .unwrap();
    let id = added
        .tasks
        .iter()
        .find(|task| task.body == "new")
        .unwrap()
        .id
        .clone()
        .unwrap();
    let done = backend
        .mutate(DesktopIntent::SetCompleted {
            id,
            completed: true,
            note: None,
            evidence: None,
        })
        .unwrap();
    assert!(
        done.tasks
            .iter()
            .find(|task| task.body == "new")
            .unwrap()
            .completed
    );
}

#[test]
fn partial_and_failed_outcomes_are_atomic_and_require_notes() {
    let (dir, backend) = project();
    let path = dir.path().join("TODO.md");

    let partial = backend
        .mutate(DesktopIntent::SetPartial {
            id: "aaaaaa".into(),
            percent: 60,
            note: "core complete; integration remains".into(),
        })
        .unwrap();
    let task = partial
        .tasks
        .iter()
        .find(|task| task.id.as_deref() == Some("aaaaaa"))
        .unwrap();
    assert_eq!(task.progress, Some(60));
    assert_eq!(
        task.note.as_deref(),
        Some("core complete; integration remains")
    );

    let failed = backend
        .mutate(DesktopIntent::SetFailed {
            id: "aaaaaa".into(),
            note: "load test failed".into(),
        })
        .unwrap();
    let task = failed
        .tasks
        .iter()
        .find(|task| task.id.as_deref() == Some("aaaaaa"))
        .unwrap();
    assert_eq!(task.progress, Some(60));
    assert_eq!(task.failed_on.as_deref(), Some("2026-01-02"));
    assert_eq!(task.note.as_deref(), Some("load test failed"));

    let source = fs::read_to_string(path).unwrap();
    assert!(source.contains("◐60 ⚠2026-01-02"));
    assert!(source.contains("↳ load test failed <!--n:aaaaaa-->"));
}

#[test]
fn move_resolves_target_id_after_external_index_shift() {
    let (dir, backend) = project();
    let _old_snapshot = backend.snapshot().unwrap();
    let path = dir.path().join("TODO.md");
    let source = fs::read_to_string(&path)
        .unwrap()
        .replace("## Todo\n", "## Todo\n- [ ] external <!--t:ffffff-->\n");
    fs::write(&path, source).unwrap();

    let moved = backend
        .mutate(DesktopIntent::MoveTask {
            id: "aaaaaa".into(),
            column: "todo".into(),
            target: MoveTarget::After {
                task_id: "cccccc".into(),
            },
        })
        .unwrap();
    let ids: Vec<_> = moved
        .tasks
        .iter()
        .filter(|task| task.column.as_deref() == Some("todo"))
        .filter_map(|task| task.id.as_deref())
        .collect();
    let moved_index = ids.iter().position(|id| *id == "aaaaaa").unwrap();
    let target_index = ids.iter().position(|id| *id == "cccccc").unwrap();
    assert_eq!(moved_index, target_index + 1);
}

#[test]
fn move_rejects_a_target_from_another_column_without_writing() {
    let (dir, backend) = project();
    let path = dir.path().join("TODO.md");
    let source = fs::read_to_string(&path)
        .unwrap()
        .replace("## Done\n", "## Done\n- [x] shipped <!--t:gggggg-->\n");
    fs::write(&path, source).unwrap();
    let before = fs::read(&path).unwrap();
    let result = backend.mutate(DesktopIntent::MoveTask {
        id: "aaaaaa".into(),
        column: "todo".into(),
        target: MoveTarget::Before {
            task_id: "gggggg".into(),
        },
    });
    assert!(result.is_err());
    assert_eq!(fs::read(path).unwrap(), before);
}

#[test]
fn thousand_task_snapshot_is_under_budget() {
    let dir = tempdir().unwrap();
    let mut source =
        "---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n---\n## Todo\n".to_string();
    for i in 0..1000 {
        source.push_str(&format!("- [ ] task {i} <!--t:{i:06}-->\n"));
    }
    fs::write(dir.path().join("TODO.md"), source).unwrap();
    let mut backend = DesktopBackend::with_clock(Duration::from_secs(2), FakeClock::fixed());
    let snapshot = backend.open_project(dir.path()).unwrap();
    assert_eq!(snapshot.tasks.len(), 1000);

    // Warm once, then take the best of three samples so shared-CI scheduling
    // noise cannot hide the actual regression boundary (the old path took 14s+).
    let best = (0..3)
        .map(|_| {
            let started = Instant::now();
            backend.snapshot().unwrap();
            started.elapsed()
        })
        .min()
        .unwrap();
    eprintln!("1000-task desktop snapshot best-of-three: {best:?}");
    assert!(best < Duration::from_millis(100), "{best:?}");
}

#[test]
fn capture_round_trips_marker_like_unicode_and_multiline_text() {
    let dir = tempdir().unwrap();
    let store = CaptureStore::with_clock(
        dir.path().join("inbox.md"),
        Duration::from_secs(1),
        FakeClock::fixed(),
    );
    let text = "想法\n<!--/capture-->\nsecond line\n";
    let capture = store.append(text).unwrap();
    assert_eq!(capture.created_at, "2026-01-02T03:04:05Z");
    assert!(DateTime::parse_from_rfc3339(&capture.created_at).is_ok());
    assert_eq!(store.list().unwrap()[0].text, text);
    assert!(store.append(&"x".repeat(64 * 1024 + 1)).is_err());
}

#[test]
fn capture_parser_rejects_truncated_or_length_corruption() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("inbox.md");
    let store = CaptureStore::with_clock(&path, Duration::from_secs(1), FakeClock::fixed());
    store.append("safe").unwrap();

    let valid = fs::read_to_string(&path).unwrap();
    fs::write(&path, valid.replace("bytes=4", "bytes=3")).unwrap();
    assert!(store.list().is_err());

    fs::write(&path, valid.trim_end_matches("<!--/capture-->\n\n")).unwrap();
    assert!(store.list().is_err());
}

#[test]
fn concurrent_first_appends_are_serialized_by_the_capture_lock() {
    let dir = tempdir().unwrap();
    let store = CaptureStore::with_clock(
        dir.path().join("new").join("inbox.md"),
        Duration::from_secs(2),
        FakeClock::fixed(),
    );
    let barrier = Arc::new(Barrier::new(5));
    let mut workers = Vec::new();
    for index in 0..4 {
        let worker_store = store.clone();
        let worker_barrier = barrier.clone();
        workers.push(thread::spawn(move || {
            worker_barrier.wait();
            worker_store.append(&format!("capture {index}")).unwrap()
        }));
    }
    barrier.wait();
    let created: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    let captures = store.list().unwrap();
    assert_eq!(captures.len(), 4);
    assert_eq!(
        captures
            .iter()
            .map(|capture| &capture.id)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        4
    );
    assert_eq!(created.len(), 4);
}

#[test]
fn capture_triage_returns_the_created_task_id() {
    let dir = tempdir().unwrap();
    let store = CaptureStore::with_clock(
        dir.path().join("inbox.md"),
        Duration::from_secs(1),
        FakeClock::fixed(),
    );
    let capture = store.append("keep exact").unwrap();
    struct Fake {
        tasks: Vec<String>,
    }
    impl CaptureProject for Fake {
        fn create_task(&mut self, body: &str, _: Option<&str>) -> todo_core::Result<String> {
            self.tasks.push(body.into());
            Ok("task01".into())
        }
    }
    let mut fake = Fake { tasks: Vec::new() };
    assert_eq!(
        store.triage_to_task(&capture.id, &mut fake, None).unwrap(),
        TriageResult::Completed {
            task_id: "task01".into()
        }
    );
    assert!(store.list().unwrap().is_empty());
    assert_eq!(fake.tasks, vec!["keep exact"]);
}

#[test]
fn capture_triage_with_desktop_backend_returns_an_existing_task_id() {
    let (dir, mut backend) = project();
    let store = CaptureStore::with_clock(
        dir.path().join("inbox.md"),
        Duration::from_secs(1),
        FakeClock::fixed(),
    );
    let capture = store.append("turn this into a Project Task").unwrap();
    let result = store
        .triage_to_task(&capture.id, &mut backend, Some("todo"))
        .unwrap();
    let task_id = match result {
        TriageResult::Completed { task_id } => task_id,
        TriageResult::TaskCreatedCaptureRetained { .. } => panic!("capture deletion failed"),
    };
    assert!(backend
        .snapshot()
        .unwrap()
        .tasks
        .iter()
        .any(|task| task.id.as_deref() == Some(&task_id)));
    assert!(store.list().unwrap().is_empty());
}

#[test]
fn concurrent_triage_creates_exactly_one_project_task() {
    let (dir, first_backend) = project();
    let mut second_backend = DesktopBackend::with_clock(Duration::from_secs(2), FakeClock::fixed());
    second_backend.open_project(dir.path()).unwrap();
    let store = CaptureStore::with_clock(
        dir.path().join("inbox.md"),
        Duration::from_secs(2),
        FakeClock::fixed(),
    );
    let capture = store.append("triage exactly once").unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let mut workers = Vec::new();
    for mut backend in [first_backend, second_backend] {
        let worker_store = store.clone();
        let worker_barrier = barrier.clone();
        let capture_id = capture.id.clone();
        workers.push(thread::spawn(move || {
            worker_barrier.wait();
            worker_store.triage_to_task(&capture_id, &mut backend, Some("todo"))
        }));
    }
    barrier.wait();
    let results: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Ok(TriageResult::Completed { .. })))
            .count(),
        1
    );
    assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);

    let mut verifier = DesktopBackend::with_clock(Duration::from_secs(2), FakeClock::fixed());
    let snapshot = verifier.open_project(dir.path()).unwrap();
    assert_eq!(
        snapshot
            .tasks
            .iter()
            .filter(|task| task.body == "triage exactly once")
            .count(),
        1
    );
    assert!(store.list().unwrap().is_empty());
}

#[test]
fn triage_partial_success_includes_the_created_task_id() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("inbox.md");
    let store = CaptureStore::with_clock(&path, Duration::from_secs(1), FakeClock::fixed());
    let capture = store.append("keep visible").unwrap();
    struct BreaksInbox {
        path: std::path::PathBuf,
    }
    impl CaptureProject for BreaksInbox {
        fn create_task(&mut self, _: &str, _: Option<&str>) -> todo_core::Result<String> {
            fs::remove_file(&self.path).unwrap();
            fs::create_dir(&self.path).unwrap();
            Ok("task02".into())
        }
    }
    let mut project = BreaksInbox { path };
    assert!(matches!(
        store
            .triage_to_task(&capture.id, &mut project, None)
            .unwrap(),
        TriageResult::TaskCreatedCaptureRetained { task_id, .. } if task_id == "task02"
    ));
}

#[test]
fn global_capture_path_uses_the_product_home_location() {
    let path = todo_core::global_capture_path().unwrap();
    assert!(path.ends_with(std::path::Path::new(".todo").join("inbox.md")));
}

#[test]
fn today_bands_are_ordered_by_priority_then_due_then_started() {
    // The landing view answers "what should I do now", so file order is not good
    // enough: P1 must outrank P3 regardless of where each line sits.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("TODO.md");
    std::fs::write(
        &path,
        "---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n---\n\n## Todo\n\n\
- [ ] 低优先 <!--t:aaaaaa--> !3 📅2026-08-20\n\
- [ ] 无优先级 <!--t:bbbbbb--> 📅2026-08-20\n\
- [ ] 高优先 <!--t:cccccc--> !1 📅2026-08-20\n\
- [ ] 高优先已开工 <!--t:dddddd--> !1 📅2026-08-20 ◐60\n",
    )
    .unwrap();

    let backend = todo_core::DesktopBackend::new(std::time::Duration::from_millis(2000));
    let mut backend = backend;
    let snapshot = backend.open_project(path.to_str().unwrap()).unwrap();

    let ids: Vec<_> = snapshot
        .today
        .overdue
        .iter()
        .map(|task| task.id.clone().unwrap())
        .collect();

    assert_eq!(
        ids,
        vec!["dddddd", "cccccc", "aaaaaa", "bbbbbb"],
        "priority first; started work ahead of untouched at equal priority; \
         missing priority last"
    );
}

#[test]
fn a_custom_ranker_replaces_the_default_ordering() {
    // The paid seam: a hosted ranker plugs in here instead of replacing the view,
    // so the free tier always has a complete ordering rather than an unsorted list.
    #[derive(Debug)]
    struct ReverseAlphabetical;
    impl todo_core::TaskRanker for ReverseAlphabetical {
        fn rank(&self, tasks: &mut [todo_core::DesktopTask]) {
            tasks.sort_by(|a, b| b.body.cmp(&a.body));
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("TODO.md");
    std::fs::write(
        &path,
        "---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n---\n\n## Todo\n\n\
- [ ] alpha <!--t:aaaaaa--> !1 📅2026-08-20\n\
- [ ] zulu <!--t:bbbbbb--> !3 📅2026-08-20\n",
    )
    .unwrap();

    let mut backend = todo_core::DesktopBackend::new(std::time::Duration::from_millis(2000))
        .with_ranker(std::sync::Arc::new(ReverseAlphabetical));
    let snapshot = backend.open_project(path.to_str().unwrap()).unwrap();

    let bodies: Vec<_> = snapshot
        .today
        .overdue
        .iter()
        .map(|task| task.body.clone())
        .collect();
    // Default ordering would put the P1 "alpha" first; the custom ranker wins.
    assert_eq!(bodies, vec!["zulu", "alpha"]);
}

#[test]
fn undated_actionable_work_still_appears_on_the_landing_view() {
    // Regression: a P1 task with no due date matched no band, so the page that
    // claims to answer "what should I do now" hid the highest-priority item in
    // the project. Found by driving the real Web UI, not by a unit test.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("TODO.md");
    std::fs::write(
        &path,
        "---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n---\n\n## Todo\n\n\
- [ ] 低优先无日期 <!--t:aaaaaa--> !3\n\
- [ ] 高优先无日期 <!--t:bbbbbb--> !1\n\
- [x] 已完成无日期 <!--t:cccccc--> !1\n",
    )
    .unwrap();

    let mut backend = todo_core::DesktopBackend::new(std::time::Duration::from_millis(2000));
    let snapshot = backend.open_project(path.to_str().unwrap()).unwrap();

    let ids: Vec<_> = snapshot
        .today
        .next
        .iter()
        .map(|task| task.id.clone().unwrap())
        .collect();
    assert_eq!(ids, vec!["bbbbbb", "aaaaaa"], "ordered by priority");
    assert!(
        snapshot.today.overdue.is_empty() && snapshot.today.today.is_empty(),
        "undated work must not be forced into a dated band"
    );
}
