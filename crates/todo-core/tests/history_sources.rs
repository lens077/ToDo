use std::{
    fs,
    sync::{Arc, Barrier},
    thread,
};
use todo_core::History;
#[test]
fn sibling_sources_have_distinct_concurrent_versions_and_cannot_cross_read() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("TODO.md");
    let b = dir.path().join("TODO.txt");
    fs::write(&a, "source A").unwrap();
    fs::write(&b, "source B").unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let jobs = [a.clone(), b.clone()]
        .into_iter()
        .map(|path| {
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                History::for_file(&path)
                    .record(&path, "test")
                    .unwrap()
                    .unwrap()
            })
        })
        .collect::<Vec<_>>();
    let entries = jobs
        .into_iter()
        .map(|job| job.join().unwrap())
        .collect::<Vec<_>>();
    assert_ne!(entries[0].version, entries[1].version);
    let h = History::for_file(&a);
    assert_eq!(h.entries_for(&a).unwrap().len(), 1);
    assert_eq!(h.entries_for(&b).unwrap().len(), 1);
    assert_eq!(h.read_for(&a, entries[0].version).unwrap(), "source A");
    assert!(h.read_for(&a, entries[1].version).is_err());
    assert_eq!(h.read_for(&b, entries[1].version).unwrap(), "source B");
}
