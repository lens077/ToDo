use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
use tempfile::TempDir;
use todo_core::{DesktopIntent, ProjectBackend};

fn project_root() -> (TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().canonicalize().unwrap();
    (temp, path)
}
fn put(root: &Path, name: &str, text: &str) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}
fn managed(body: &str) -> String {
    format!("---\ntodo-spec: 1\n---\n- [ ] {body} <!--t:aaaaaa-->\n")
}
fn backend(root: &Path) -> ProjectBackend {
    let mut backend = ProjectBackend::new(Duration::from_millis(100));
    backend.open_project(root).unwrap();
    backend
}
fn add() -> DesktopIntent {
    DesktopIntent::AddTask {
        body: "new task".into(),
        column: None,
    }
}

#[test]
fn root_selection_accepts_system_ancestor_aliases_and_rejects_non_task_files() {
    let temp = tempfile::tempdir().unwrap();
    put(temp.path(), "TODO.md", &managed("task"));
    let mut backend = ProjectBackend::default();
    assert_eq!(
        backend
            .open_project(temp.path().join("TODO.md"))
            .unwrap()
            .tasks
            .len(),
        1
    );
    assert_eq!(backend.open_project(temp.path()).unwrap().tasks.len(), 1);
    put(temp.path(), "notes.md", "# just notes\n");
    assert!(matches!(
        backend.open_project(temp.path().join("notes.md")),
        Err(todo_core::Error::NotTaskFile)
    ));
}

#[test]
fn empty_directory_is_a_project() {
    let (_temp, root) = project_root();
    let backend = backend(&root);
    let snapshot = backend.snapshot().unwrap();
    assert_eq!(snapshot.project_path, root.to_str().unwrap());
    assert!(snapshot.sources.is_empty());
    assert!(snapshot.tasks.is_empty());
    assert!(backend.source_backend(None).is_err());
    assert_eq!(fs::read_dir(root).unwrap().count(), 0);
}

#[test]
fn nested_same_ids_are_local_and_write_to_exact_source() {
    let (_temp, root) = project_root();
    put(&root, "TODO.md", &managed("root task"));
    put(&root, "nested/tOdO.TxT", &managed("nested task"));
    let backend = backend(&root);
    let snapshot = backend.snapshot().unwrap();
    assert_eq!(snapshot.tasks.len(), 2);
    assert_eq!(snapshot.tasks[0].id.as_deref(), Some("aaaaaa"));
    assert_eq!(snapshot.tasks[1].id.as_deref(), Some("aaaaaa"));
    assert!(backend
        .mutate(None, add())
        .unwrap_err()
        .to_string()
        .contains("请先选择来源文件"));
    backend
        .mutate(
            Some("nested/tOdO.TxT"),
            DesktopIntent::SetCompleted {
                id: "aaaaaa".into(),
                completed: true,
                note: None,
                evidence: None,
            },
        )
        .unwrap();
    assert_eq!(
        fs::read_to_string(root.join("TODO.md")).unwrap(),
        managed("root task")
    );
    assert!(fs::read_to_string(root.join("nested/tOdO.TxT"))
        .unwrap()
        .contains("- [x]"));
    let selected = backend.snapshot_source("nested/tOdO.TxT").unwrap();
    assert_eq!(selected.tasks.len(), 1);
    assert_eq!(selected.selected_source.as_deref(), Some("nested/tOdO.TxT"));
    assert_eq!(selected.sources.len(), 2);
    let json = serde_json::to_value(&selected.tasks[0]).unwrap();
    assert_eq!(json["id"], "aaaaaa");
    assert_eq!(json["source_path"], "nested/tOdO.TxT");
    assert!(json.get("task").is_none());
}

#[test]
fn unmanaged_discovery_is_read_only_and_only_checkboxes_are_tasks() {
    let (_temp, root) = project_root();
    let text = "plain prose\n- ordinary bullet\n- [ ] actual task\n```\n- [ ] not a task\n```\n";
    put(&root, "TODO", text);
    let backend = backend(&root);
    let snapshot = backend.snapshot().unwrap();
    assert_eq!(snapshot.sources[0].status, "unmanaged");
    assert!(!snapshot.sources[0].writable);
    assert_eq!(snapshot.tasks.len(), 1);
    assert_eq!(snapshot.tasks[0].body, "actual task");
    assert!(backend.mutate(Some("TODO"), add()).is_err());
    assert_eq!(fs::read_to_string(root.join("TODO")).unwrap(), text);
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
    put(&root, "nested/TODO.md", &managed("managed"));
    assert!(backend.source_backend(None).is_err());
}

#[test]
fn adoption_is_previewed_conflict_checked_and_lossless() {
    let (_temp, root) = project_root();
    let text = "\u{feff}---\r\ntitle: My project\r\n---\r\nProse  \r\n- [ ] First  \r\n- [x] Existing <!--t:zzzzzz-->\r\nlast line without newline";
    put(&root, "TODO.txt", text);
    let backend = backend(&root);
    let preview = backend.preview_adopt("TODO.txt").unwrap();
    assert_eq!(fs::read_to_string(root.join("TODO.txt")).unwrap(), text);
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
    assert!(preview.proposed.contains("title: My project\r\n"));
    assert!(preview.proposed.ends_with("last line without newline"));
    assert!(preview.proposed.contains("<!--t:zzzzzz-->"));
    let mut stripped = preview.proposed.replace("todo-spec: 1\r\n", "");
    let parsed = todo_core::parse(preview.proposed.clone()).unwrap();
    let new_id = parsed.tasks[0].id.as_ref().unwrap();
    stripped = stripped.replace(&format!(" <!--t:{new_id}-->"), "");
    assert_eq!(stripped, text);
    put(&root, "TODO.txt", &(text.to_owned() + " changed"));
    assert!(matches!(
        backend.apply_adopt("TODO.txt", &preview.base_hash),
        Err(todo_core::Error::ConcurrentModification)
    ));
    assert!(!root.join(".todo").exists());
    let preview = backend.preview_adopt("TODO.txt").unwrap();
    let snapshot = backend.apply_adopt("TODO.txt", &preview.base_hash).unwrap();
    assert_eq!(snapshot.sources[0].status, "managed");
    assert_eq!(snapshot.tasks.len(), 2);
    let stored = fs::read_to_string(root.join("TODO.txt")).unwrap();
    assert!(stored.contains("<!--t:zzzzzz-->"));
    assert_eq!(stored, preview.proposed);
    let history = todo_core::History::for_file(root.join("TODO.txt"));
    assert_eq!(history.entries().unwrap().len(), 1);
}

#[test]
fn ignore_rules_and_errors_are_isolated() {
    let (_temp, root) = project_root();
    put(&root, ".gitignore", "ignored/\n");
    for name in [
        "ignored/TODO.md",
        ".hidden/TODO.md",
        "node_modules/TODO.md",
        "build/TODO",
        "dist/TODO.txt",
        "target/TODO.md",
        ".git/TODO",
    ] {
        put(&root, name, &managed("ignored"));
    }
    put(&root, "ok/TODO.MD", &managed("good"));
    put(&root, "bad/TODO", "---\ntodo-spec: [\n---\n- [ ] broken\n");
    put(
        &root,
        "unterminated/TODO.txt",
        "---\nwrong: true\n- [ ] broken\n",
    );
    fs::write(root.join("TODO.txt"), [0xff, 0xfe]).unwrap();
    let backend = backend(&root);
    let snapshot = backend.snapshot().unwrap();
    assert_eq!(snapshot.sources.len(), 4);
    assert_eq!(snapshot.tasks.len(), 1);
    assert_eq!(
        snapshot
            .sources
            .iter()
            .filter(|s| s.status == "error")
            .count(),
        3
    );
    assert_eq!(snapshot.warnings.len(), 3);
    assert!(backend.preview_adopt("bad/TODO").is_err());
    assert!(backend.preview_adopt("unterminated/TODO.txt").is_err());
}

#[test]
fn source_and_byte_limits_warn_without_writing() {
    let (_temp, root) = project_root();
    put(&root, "TODO", &"x".repeat(2 * 1024 * 1024 + 1));
    let snapshot = backend(&root).snapshot().unwrap();
    assert_eq!(snapshot.sources[0].status, "error");
    assert!(!snapshot.warnings.is_empty());
    fs::remove_file(root.join("TODO")).unwrap();
    for i in 0..257 {
        put(&root, &format!("{i:03}/TODO"), "prose");
    }
    let snapshot = backend(&root).snapshot().unwrap();
    assert_eq!(snapshot.sources.len(), 256);
    assert!(snapshot.warnings.iter().any(|w| w.contains("256")));
}

#[test]
fn paths_must_be_exact_discovered_relative_paths() {
    let (_temp, root) = project_root();
    put(&root, "TODO.md", &managed("task"));
    put(&root, "other.md", &managed("not discovered"));
    let backend = backend(&root);
    for path in [
        "../TODO.md",
        "./TODO.md",
        "/TODO.md",
        "a/../TODO.md",
        "TODO.md/",
        "other.md",
        "todo.md",
        "a\\TODO.md",
    ] {
        assert!(backend.source_backend(Some(path)).is_err(), "{path}");
        assert!(backend.mutate(Some(path), add()).is_err(), "{path}");
    }
}

#[test]
fn total_read_budget_allows_exact_limit_and_isolates_excess() {
    let (_temp, root) = project_root();
    let text = "x".repeat(2 * 1024 * 1024);
    for i in 0..9 {
        put(&root, &format!("{i}/TODO"), &text);
    }
    let mut backend = ProjectBackend::default();
    let snapshot = backend.open_project(&root).unwrap();
    assert_eq!(snapshot.sources.len(), 9);
    assert_eq!(
        snapshot
            .sources
            .iter()
            .filter(|s| s.status == "unmanaged")
            .count(),
        8
    );
    assert_eq!(snapshot.sources[8].status, "error");
    assert!(snapshot.warnings.iter().any(|w| w.contains("16 MiB")));
}

#[test]
fn entry_limit_warns_and_legacy_file_input_is_limited() {
    let (_temp, root) = project_root();
    put(&root, "TODO.md", &managed("root task"));
    put(&root, "nested/TODO", &managed("nested task"));
    let mut backend = ProjectBackend::default();
    let snapshot = backend.open_project(root.join("TODO.md")).unwrap();
    assert_eq!(snapshot.sources.len(), 1);
    assert_eq!(snapshot.tasks.len(), 1);
    assert!(backend.source_backend(None).is_ok());
    for i in 0..10_001 {
        put(&root, &format!("entries/{i:05}.txt"), "");
    }
    let snapshot = backend.open_project(&root).unwrap();
    assert!(snapshot.warnings.iter().any(|w| w.contains("10000")));
}

#[cfg(unix)]
#[test]
fn symlinks_and_readonly_sources_cannot_write() {
    use std::os::unix::{fs::symlink, fs::PermissionsExt};
    let (_temp, root) = project_root();
    let (_outside, outside) = project_root();
    put(&outside, "TODO.md", &managed("outside"));
    symlink(outside.join("TODO.md"), root.join("TODO.md")).unwrap();
    symlink(&outside, root.join("linked")).unwrap();
    let backend = backend(&root);
    assert!(backend.snapshot().unwrap().sources.is_empty());
    assert!(backend.mutate(Some("linked/TODO.md"), add()).is_err());
    fs::remove_file(root.join("TODO.md")).unwrap();
    put(&root, "TODO.md", &managed("inside"));
    symlink(&outside, root.join(".todo")).unwrap();
    assert!(!backend.snapshot().unwrap().sources[0].writable);
    assert!(backend.mutate(Some("TODO.md"), add()).is_err());
    fs::remove_file(root.join(".todo")).unwrap();
    symlink(outside.join("TODO.md"), root.join(".TODO.md.lock")).unwrap();
    assert!(backend.mutate(Some("TODO.md"), add()).is_err());
    fs::remove_file(root.join(".TODO.md.lock")).unwrap();
    fs::set_permissions(root.join("TODO.md"), fs::Permissions::from_mode(0o444)).unwrap();
    assert!(!backend.snapshot().unwrap().sources[0].writable);
    assert!(backend.mutate(Some("TODO.md"), add()).is_err());
    assert_eq!(
        fs::read_to_string(outside.join("TODO.md")).unwrap(),
        managed("outside")
    );
}
