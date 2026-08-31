use std::{
    fs,
    sync::{Arc, Barrier},
    thread,
    time::Duration,
};

use tempfile::tempdir;
use todo_core::{parse_file, Change, Error, InsertPosition, LockedWriter};

const BASE: &str = "---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n---\n\n## Todo\n";

#[test]
fn concurrent_writers_serialize_read_modify_write_cycles() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("TODO.md");
    fs::write(&path, BASE).unwrap();
    let writers = 12;
    let barrier = Arc::new(Barrier::new(writers));
    let mut handles = Vec::new();
    for index in 0..writers {
        let path = path.clone();
        let barrier = barrier.clone();
        handles.push(thread::spawn(move || {
            barrier.wait();
            LockedWriter::new(&path, Duration::from_secs(3))
                .update(|_document| {
                    Ok(vec![Change::InsertTask {
                        column: Some("todo".into()),
                        position: InsertPosition::End,
                        line: format!("- [ ] concurrent-{index} <!--t:{:06}-->", index + 100),
                    }])
                })
                .unwrap();
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }
    let document = parse_file(&path).unwrap();
    assert_eq!(document.tasks.len(), writers);
    for index in 0..writers {
        assert!(document.source.contains(&format!("concurrent-{index}")));
    }
}

#[test]
fn lock_timeout_is_explicit_and_crash_safe_lock_file_can_remain() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("TODO.md");
    fs::write(&path, BASE).unwrap();
    let first = LockedWriter::new(&path, Duration::from_secs(1));
    let guard = first.lock().unwrap();
    let second = LockedWriter::new(&path, Duration::from_millis(30));
    assert!(matches!(second.lock(), Err(Error::LockTimeout { .. })));
    drop(guard);
    assert!(second.lock().is_ok());
}
