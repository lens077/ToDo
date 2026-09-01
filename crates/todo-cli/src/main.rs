mod hooks;

use todo_cli::serve;

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
    time::Duration,
};

use clap::{Parser, Subcommand};
use serde::Serialize;
use todo_core::{
    atomic_write, generate_task_id, parse_file, Change, Error, History, InsertPosition,
    LockedWriter, ReconcileOptions, ReconcileReport, Reconciler, TokenKind,
};

const TODO_TEMPLATE: &str = "---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n  - {id: doing, name: Doing}\n  - {id: done, name: Done, type: done}\n---\n\n# Project tasks\n\n## Todo\n\n## Doing\n\n## Done\n";
const ARCHIVE_TEMPLATE: &str = "---\ntodo-spec: 1\ncolumns:\n  - {id: archived, name: Archived, type: done}\n---\n\n# Archived project tasks\n\n## Archived\n";

#[derive(Parser)]
#[command(
    name = "todo",
    version,
    about = "TODO.md canonical CLI",
    after_help = "Exit codes: 0 success; 1 general error; 2 usage error; 3 not initialized; 4 lock timeout; 5 optimistic conflict; 6 lint issues"
)]
struct Cli {
    #[arg(long, global = true, default_value_t = 2000)]
    lock_timeout_ms: u64,
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Init {
        #[arg(long)]
        adopt: bool,
    },
    List {
        #[arg(long)]
        column: Option<String>,
        #[arg(long)]
        all: bool,
    },
    Add {
        body: String,
        #[arg(long)]
        column: Option<String>,
        #[arg(long = "tag")]
        tags: Vec<String>,
        #[arg(long)]
        due: Option<String>,
        #[arg(long)]
        priority: Option<u8>,
    },
    Done {
        id: String,
    },
    Undone {
        id: String,
    },
    /// Record partial completion: `todo progress <id> 60 --note "…"`.
    Progress {
        id: String,
        /// Percent complete, 1-99. Use `todo done` for 100 and leave it unset for 0.
        percent: u8,
        #[arg(long)]
        note: Option<String>,
    },
    /// Record that an attempt failed, without moving the task backwards.
    Fail {
        id: String,
        #[arg(long)]
        note: Option<String>,
    },
    /// Write or clear the `↳` note line under a task.
    Note {
        id: String,
        text: Option<String>,
        #[arg(long)]
        clear: bool,
    },
    Evidence {
        id: String,
        #[arg(long)]
        from: String,
        #[arg(long, default_value = "HEAD")]
        to: String,
    },
    Next,
    Lint,
    /// Move completed top-level task subtrees to TODO.archive.md.
    Archive,
    Reconcile {
        #[arg(long)]
        apply: bool,
        #[arg(long)]
        no_semantic: bool,
    },
    Hooks {
        #[command(subcommand)]
        command: HooksCommand,
    },
    /// List restore points recorded before each write.
    History {
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// Delete all but the newest N restore points.
        #[arg(long)]
        prune_to: Option<usize>,
    },
    /// Show the left/right diff between a restore point and the current file.
    Diff {
        version: u64,
    },
    /// Restore a previous version. Prints the diff and does nothing without --yes.
    Restore {
        version: u64,
        #[arg(long)]
        yes: bool,
    },
    /// Find untracked work across the repository. Reports only unless --import.
    Scan {
        /// Restrict to loose checkboxes in other Markdown files.
        #[arg(long)]
        markdown_only: bool,
        /// Restrict to TODO/FIXME/HACK/XXX markers in source comments.
        #[arg(long)]
        comments_only: bool,
        /// Show the diff of importing the findings as tasks.
        #[arg(long)]
        import: bool,
        /// Column that imported tasks land in.
        #[arg(long, default_value = "todo")]
        column: String,
        /// Apply the import. Without it, --import only previews.
        #[arg(long)]
        yes: bool,
    },
    /// Print the repository-alignment prompt for an agent to run.
    Align {
        /// Validate an agent's reply instead of printing the prompt.
        /// Pass a file path, or `-` to read stdin.
        #[arg(long)]
        review: Option<String>,
    },
    /// Serve the HTTP API, and optionally the Web UI, for this project.
    Serve {
        #[arg(long, default_value_t = 3017)]
        port: u16,
        /// Interface to bind. Loopback by default: a task file is repository
        /// working state, and exposing it on every interface should be deliberate.
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// Refuse every mutating endpoint. Intended for mobile clients.
        #[arg(long)]
        read_only: bool,
        /// Directory of built Web UI assets to serve at `/`.
        #[arg(long)]
        web_root: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum HooksCommand {
    Preview {
        #[arg(long, value_enum, default_value = "all")]
        agent: hooks::Agent,
    },
    Install {
        #[arg(long, value_enum, default_value = "all")]
        agent: hooks::Agent,
    },
    Uninstall {
        #[arg(long, value_enum, default_value = "all")]
        agent: hooks::Agent,
    },
    Instructions {
        #[arg(long, value_enum, default_value = "all")]
        agent: hooks::Agent,
        #[arg(long)]
        write: bool,
    },
    #[command(hide = true)]
    Context {
        #[arg(long, value_enum, default_value = "claude")]
        agent: hooks::Agent,
    },
}

#[derive(Serialize)]
struct Envelope<T: Serialize> {
    schema: &'static str,
    ok: bool,
    data: T,
}
#[derive(Serialize)]
struct Message {
    message: String,
}
#[derive(Serialize, Clone)]
struct TaskView {
    id: Option<String>,
    body: String,
    completed: bool,
    column: Option<String>,
    indent_level: usize,
    parent_id: Option<String>,
    due: Option<String>,
    tags: Vec<String>,
    assignees: Vec<String>,
    priority: Option<u8>,
    order_key: Option<i64>,
    dependencies: Vec<String>,
    progress: Option<u8>,
    failed_on: Option<String>,
    note: Option<String>,
    line: usize,
}
#[derive(Serialize)]
struct ListData {
    tasks: Vec<TaskView>,
}
#[derive(Serialize)]
struct MutationData {
    task: TaskView,
}
#[derive(Serialize)]
struct NextData {
    task: Option<TaskView>,
}
#[derive(Serialize)]
struct ReconcileData {
    report: ReconcileReport,
    applied: usize,
    /// Findings that exist but have no safe automatic fix. Distinct from
    /// `report.checks_skipped`, which is about checks that never ran.
    findings_requiring_manual_action: usize,
}
#[derive(Serialize)]
struct LintData {
    issues: Vec<LintIssue>,
}
#[derive(Serialize)]
struct ArchiveData {
    archived: usize,
    path: String,
}
#[derive(Clone)]
struct ArchiveBlock {
    root_id: String,
    ids: Vec<String>,
    raw: String,
}
#[derive(Serialize)]
struct LintIssue {
    severity: &'static str,
    code: &'static str,
    line: Option<usize>,
    message: String,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            emit_error(&cli, &error);
            ExitCode::from(error_code(&error))
        }
    }
}

fn run(cli: &Cli) -> Result<u8, Error> {
    let path = PathBuf::from("TODO.md");
    match &cli.command {
        Command::Init { adopt } => init(cli, &path, *adopt),
        Command::List { column, all } => {
            let doc = load(&path)?;
            let mut tasks: Vec<_> = projected_views(&path, &doc)?
                .into_iter()
                .filter(|task| *all || (!task.completed && task.column.is_some()))
                .filter(|task| {
                    column
                        .as_ref()
                        .is_none_or(|column| task.column.as_ref() == Some(column))
                })
                .collect();
            tasks.sort_by_key(|task| task.line);
            emit(cli, "todo.list.v1", ListData { tasks }, |data| {
                data.tasks
                    .iter()
                    .map(human_task)
                    .collect::<Vec<_>>()
                    .join("\n")
            });
            Ok(0)
        }
        Command::Add {
            body,
            column,
            tags,
            due,
            priority,
        } => add(cli, &path, body, column, tags, due, *priority),
        Command::Done { id } => toggle(cli, &path, id, true),
        Command::Undone { id } => toggle(cli, &path, id, false),
        Command::Progress { id, percent, note } => {
            progress(cli, &path, id, *percent, note.as_deref())
        }
        Command::Fail { id, note } => fail(cli, &path, id, note.as_deref()),
        Command::Note { id, text, clear } => set_note(cli, &path, id, text.as_deref(), *clear),
        Command::Evidence { id, from, to } => evidence(cli, &path, id, from, to),
        Command::Next => next(cli, &path),
        Command::Lint => lint(cli, &path),
        Command::Archive => archive(cli, &path),
        Command::Reconcile { apply, no_semantic } => reconcile(cli, &path, *apply, *no_semantic),
        Command::Hooks { command } => run_hooks(cli, command),
        Command::History { limit, prune_to } => history_list(cli, &path, *limit, *prune_to),
        Command::Diff { version } => show_diff(cli, &path, *version),
        Command::Restore { version, yes } => restore(cli, &path, *version, *yes),
        Command::Scan {
            markdown_only,
            comments_only,
            import,
            column,
            yes,
        } => run_scan(
            cli,
            &path,
            *markdown_only,
            *comments_only,
            *import,
            column,
            *yes,
        ),
        Command::Align { review } => align(cli, &path, review.as_deref()),
        Command::Serve {
            port,
            host,
            read_only,
            web_root,
        } => {
            let addr: std::net::SocketAddr = format!("{host}:{port}")
                .parse()
                .map_err(|_| Error::InvalidAlignment(format!("invalid host/port {host}:{port}")))?;
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .map_err(|error| Error::InvalidAlignment(error.to_string()))?;
            runtime
                .block_on(serve::run(
                    path.clone(),
                    addr,
                    Duration::from_millis(cli.lock_timeout_ms),
                    *read_only,
                    web_root.clone(),
                ))
                .map_err(Error::InvalidAlignment)?;
            Ok(0)
        }
    }
}

fn align(cli: &Cli, path: &Path, review: Option<&str>) -> Result<u8, Error> {
    let Some(source) = review else {
        print!("{}", todo_core::ALIGNMENT_PROMPT);
        return Ok(0);
    };
    let raw = if source == "-" {
        use std::io::Read;
        let mut buffer = String::new();
        std::io::stdin()
            .read_to_string(&mut buffer)
            .map_err(|error| Error::InvalidAlignment(error.to_string()))?;
        buffer
    } else {
        std::fs::read_to_string(source)
            .map_err(|error| Error::InvalidAlignment(format!("{source}: {error}")))?
    };
    let document = load(path)?;
    let result = todo_core::review_alignment(&document, &raw)?;
    // Rejections and gaps are findings in their own right: an agent that skipped
    // half the list has not aligned anything.
    let code = if !result.rejected.is_empty() || !result.missing.is_empty() {
        6
    } else {
        0
    };
    emit(cli, "todo.alignment.v1", result, |data| {
        let mut lines = Vec::new();
        for verdict in data.actionable() {
            lines.push(format!(
                "{:<16} {}  {}  [{}]",
                format!("{:?}", verdict.verdict).to_lowercase(),
                verdict.task_id,
                verdict.reason,
                verdict.citations.join(", ")
            ));
        }
        for rejected in &data.rejected {
            lines.push(format!(
                "REJECTED {} — {}",
                rejected.task_id, rejected.reason
            ));
        }
        if !data.missing.is_empty() {
            lines.push(format!(
                "NOT REVIEWED: {} task(s) got no verdict: {}",
                data.missing.len(),
                data.missing.join(", ")
            ));
        }
        if data.truncated {
            lines.push("the agent reported truncating its input; re-run for the rest".into());
        }
        lines.push(format!(
            "{} accepted, {} rejected, {} not reviewed — nothing was written",
            data.accepted.len(),
            data.rejected.len(),
            data.missing.len()
        ));
        lines.join("\n")
    });
    Ok(code)
}

#[derive(Serialize)]
struct ScanData {
    report: todo_core::ScanReport,
    diff: Option<todo_core::TextDiff>,
    applied: bool,
}

#[allow(clippy::too_many_arguments)]
fn run_scan(
    cli: &Cli,
    path: &Path,
    markdown_only: bool,
    comments_only: bool,
    import: bool,
    column: &str,
    yes: bool,
) -> Result<u8, Error> {
    let document = load(path)?;
    let options = todo_core::ScanOptions {
        markdown: !comments_only,
        comments: !markdown_only,
        task_file: path.to_path_buf(),
    };
    let root = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let report = todo_core::scan(root, &document, &options)?;

    if !import || report.is_empty() {
        emit(
            cli,
            "todo.scan.v1",
            ScanData {
                report,
                diff: None,
                applied: false,
            },
            scan_human,
        );
        return Ok(0);
    }

    let proposed = todo_core::import_text(&document, &report, column)?;
    let diff = todo_core::diff_lines(&document.source, &proposed);
    // An import can add hundreds of lines. It goes through the same confirm gate as
    // a restore: preview by default, write only when explicitly asked.
    if !yes {
        emit(
            cli,
            "todo.scan.v1",
            ScanData {
                report,
                diff: Some(diff),
                applied: false,
            },
            scan_human,
        );
        return Ok(0);
    }
    let writer = LockedWriter::new(path, Duration::from_millis(cli.lock_timeout_ms));
    writer.replace_all("scan-import", &proposed)?;
    emit(
        cli,
        "todo.scan.v1",
        ScanData {
            report,
            diff: Some(diff),
            applied: true,
        },
        scan_human,
    );
    Ok(0)
}

fn scan_human(data: &ScanData) -> String {
    let report = &data.report;
    if report.is_empty() {
        return format!(
            "no untracked work found ({} file(s) scanned, {} already tracked)",
            report.stats.files_scanned, report.stats.duplicates_filtered
        );
    }
    let mut lines = Vec::new();
    for item in &report.loose_checkboxes {
        lines.push(format!(
            "md      {}:{}  {}",
            item.path, item.line, item.text
        ));
    }
    for item in &report.code_comments {
        lines.push(format!(
            "{:<7} {}:{}  {}",
            item.marker.to_lowercase(),
            item.path,
            item.line,
            item.text
        ));
    }
    lines.push(format!(
        "{} candidate(s); {} file(s) scanned, {} skipped, {} already tracked",
        report.total(),
        report.stats.files_scanned,
        report.stats.files_skipped,
        report.stats.duplicates_filtered
    ));
    if let Some(diff) = &data.diff {
        lines.push(format!(
            "import would add {} line(s) and remove {}",
            diff.added, diff.removed
        ));
        lines.push(if data.applied {
            "imported; run `todo history` to find the restore point".into()
        } else {
            "nothing was written; re-run with --import --yes to apply".into()
        });
    } else {
        lines.push("re-run with --import to preview importing these as tasks".into());
    }
    lines.join("\n")
}

#[derive(Serialize)]
struct HistoryData {
    entries: Vec<todo_core::HistoryEntry>,
    pruned: usize,
}

#[derive(Serialize)]
struct DiffData {
    version: u64,
    diff: todo_core::TextDiff,
    applied: bool,
}

fn history_list(
    cli: &Cli,
    path: &Path,
    limit: usize,
    prune_to: Option<usize>,
) -> Result<u8, Error> {
    let history = History::for_file(path);
    let pruned = match prune_to {
        Some(keep) => history.prune(keep)?,
        None => 0,
    };
    let mut entries = history.entries()?;
    entries.reverse();
    entries.truncate(limit);
    emit(
        cli,
        "todo.history.v1",
        HistoryData { entries, pruned },
        |data| {
            if data.entries.is_empty() {
                return "no restore points yet".into();
            }
            let mut lines: Vec<String> = data
                .entries
                .iter()
                .map(|entry| {
                    format!(
                        "v{:<6} {}  {:<14} {} bytes",
                        entry.version, entry.at, entry.op, entry.bytes
                    )
                })
                .collect();
            if data.pruned > 0 {
                lines.push(format!("pruned {} older restore point(s)", data.pruned));
            }
            lines.join("\n")
        },
    );
    Ok(0)
}

/// Renders a diff for review. `version` is the left column, the current file the right.
fn build_diff(path: &Path, version: u64) -> Result<todo_core::TextDiff, Error> {
    let previous = History::for_file(path).read(version)?;
    let current = std::fs::read_to_string(path).unwrap_or_default();
    Ok(todo_core::diff_lines(&previous, &current))
}

fn diff_human(data: &DiffData) -> String {
    let mut lines = vec![format!(
        "left = v{} (restore point), right = current file",
        data.version
    )];
    for hunk in &data.diff.hunks {
        let left = hunk
            .left_line
            .map(|n| n.to_string())
            .unwrap_or_else(|| "    ".into());
        let right = hunk
            .right_line
            .map(|n| n.to_string())
            .unwrap_or_else(|| "    ".into());
        let marker = match hunk.kind {
            todo_core::DiffKind::Context => ' ',
            todo_core::DiffKind::Added => '+',
            todo_core::DiffKind::Removed => '-',
        };
        lines.push(format!("{left:>5} {right:>5} {marker} {}", hunk.text));
    }
    lines.push(format!(
        "{} line(s) only on the right, {} only on the left",
        data.diff.added, data.diff.removed
    ));
    if !data.applied {
        lines.push("nothing was written; re-run `todo restore <version> --yes` to apply".into());
    }
    lines.join("\n")
}

fn show_diff(cli: &Cli, path: &Path, version: u64) -> Result<u8, Error> {
    let diff = build_diff(path, version)?;
    emit(
        cli,
        "todo.diff.v1",
        DiffData {
            version,
            diff,
            applied: false,
        },
        diff_human,
    );
    Ok(0)
}

fn restore(cli: &Cli, path: &Path, version: u64, yes: bool) -> Result<u8, Error> {
    let diff = build_diff(path, version)?;
    // Without --yes this is a preview: a restore rewrites the whole file, so the
    // caller sees exactly what changes before anything is written.
    if !yes {
        emit(
            cli,
            "todo.diff.v1",
            DiffData {
                version,
                diff,
                applied: false,
            },
            diff_human,
        );
        return Ok(0);
    }
    let content = History::for_file(path).read(version)?;
    let writer = LockedWriter::new(path, Duration::from_millis(cli.lock_timeout_ms));
    writer.replace_all("restore", &content)?;
    emit(
        cli,
        "todo.diff.v1",
        DiffData {
            version,
            diff,
            applied: true,
        },
        |data| {
            format!(
                "restored v{}; the replaced state is itself a new restore point",
                data.version
            )
        },
    );
    Ok(0)
}

fn run_hooks(cli: &Cli, command: &HooksCommand) -> Result<u8, Error> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    match command {
        HooksCommand::Context { .. } => {
            print!("{}", hooks::context());
            Ok(0)
        }
        HooksCommand::Preview { agent } => {
            let result = hooks::preview(&home, *agent)?;
            emit(cli, "todo.hooks.v1", result, hooks_human);
            Ok(0)
        }
        HooksCommand::Install { agent } => {
            let result = hooks::install(&home, *agent)?;
            emit(cli, "todo.hooks.v1", result, hooks_human);
            Ok(0)
        }
        HooksCommand::Uninstall { agent } => {
            let result = hooks::uninstall(&home, *agent)?;
            emit(cli, "todo.hooks.v1", result, hooks_human);
            Ok(0)
        }
        HooksCommand::Instructions { agent, write } => {
            let result = hooks::instructions(&home, *agent, *write)?;
            emit(cli, "todo.hooks.instructions.v1", result, hooks_human);
            Ok(0)
        }
    }
}

fn hooks_human(result: &hooks::HookResult) -> String {
    result
        .targets
        .iter()
        .map(|target| format!("target: {}\n{}", target.path, target.content))
        .collect::<Vec<_>>()
        .join("\n")
}

fn init(cli: &Cli, path: &Path, adopt: bool) -> Result<u8, Error> {
    // A legal v1 file does not exist yet, so `LockedWriter::update` cannot be used
    // for creation. `create_new` closes the create window in the kernel instead:
    // exactly one racing caller creates the file, every other one sees AlreadyExists.
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => {
            use std::io::Write;
            file.write_all(TODO_TEMPLATE.as_bytes())
                .map_err(|source| todo_io(path, source))?;
            file.sync_all().map_err(|source| todo_io(path, source))?;
        }
        Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => {
            if !adopt {
                return Err(Error::FileAlreadyExists {
                    path: path.to_path_buf(),
                });
            }
            // Adoption rewrites an existing file, so it takes the same advisory lock
            // every other writer uses and re-reads under that lock. Reading before
            // locking would let a concurrent adopt clobber the user's tasks.
            let writer = LockedWriter::new(path, Duration::from_millis(cli.lock_timeout_ms));
            let _guard = writer.lock()?;
            let original = fs::read_to_string(path).map_err(|source| todo_io(path, source))?;
            if original.starts_with("---\n") {
                return Err(Error::InvalidTaskLine(
                    "TODO.md already has frontmatter".into(),
                ));
            }
            let adopted = adopt_source(&original)?;
            atomic_write(path, adopted.as_bytes())?;
        }
        Err(source) => return Err(todo_io(path, source)),
    }
    emit(
        cli,
        "todo.init.v1",
        Message {
            message: "initialized TODO.md".into(),
        },
        |data| data.message.clone(),
    );
    Ok(0)
}

fn add(
    cli: &Cli,
    path: &Path,
    body: &str,
    column: &Option<String>,
    tags: &[String],
    due: &Option<String>,
    priority: Option<u8>,
) -> Result<u8, Error> {
    if priority.is_some_and(|value| !(1..=4).contains(&value)) {
        return Err(Error::InvalidTaskLine("priority must be 1..4".into()));
    }
    if due.as_ref().is_some_and(|date| !valid_date(date)) {
        return Err(Error::InvalidTaskLine("due date must be YYYY-MM-DD".into()));
    }
    let existing = load(path)?
        .tasks
        .into_iter()
        .filter_map(|task| task.id)
        .collect();
    let id = generate_task_id(&existing)?;
    let column = column.clone().unwrap_or_else(|| "todo".into());
    let mut line = format!("- [ ] {body} <!--t:{id}-->");
    for tag in tags {
        line.push_str(&format!(" #{tag}"));
    }
    if let Some(due) = due {
        line.push_str(&format!(" 📅{due}"));
    }
    if let Some(priority) = priority {
        line.push_str(&format!(" !{priority}"));
    }
    let writer = LockedWriter::new(path, Duration::from_millis(cli.lock_timeout_ms));
    let result = writer.update_labeled("add", |_| {
        Ok(vec![Change::InsertTask {
            column: Some(column.clone()),
            position: InsertPosition::End,
            line,
        }])
    })?;
    let view = views(&result)
        .into_iter()
        .find(|task| task.id.as_deref() == Some(&id))
        .ok_or(Error::TaskNotFound(id))?;
    emit(
        cli,
        "todo.mutation.v1",
        MutationData { task: view },
        |data| format!("added {}", human_task(&data.task)),
    );
    Ok(0)
}

fn toggle(cli: &Cli, path: &Path, id: &str, completed: bool) -> Result<u8, Error> {
    let writer = LockedWriter::new(path, Duration::from_millis(cli.lock_timeout_ms));
    let id_owned = id.to_string();
    let result = writer.update_labeled(if completed { "done" } else { "undone" }, |document| {
        let index = index_of(document, &id_owned)?;
        let mut changes = vec![Change::SetCompleted {
            task: index,
            completed,
        }];
        if completed {
            // A finished task carries no partial-progress claim. Leaving ◐/⚠ behind
            // would leave the line asserting two different states at once.
            for (token, _) in
                document.tasks[index]
                    .metadata
                    .iter()
                    .enumerate()
                    .filter(|(_, token)| {
                        matches!(token.kind, TokenKind::Progress(_) | TokenKind::Failure(_))
                    })
            {
                changes.push(Change::RemoveMetadata { task: index, token });
            }
            changes.push(Change::RemoveNote { task: index });
        }
        Ok(changes)
    })?;
    let view = views(&result)
        .into_iter()
        .find(|task| task.id.as_deref() == Some(id))
        .ok_or_else(|| Error::TaskNotFound(id.into()))?;
    emit(
        cli,
        "todo.mutation.v1",
        MutationData { task: view },
        |data| human_task(&data.task),
    );
    Ok(0)
}

/// Locate a task by stable id, or fail with the id the caller asked for.
fn index_of(document: &todo_core::Document, id: &str) -> Result<usize, Error> {
    document
        .tasks
        .iter()
        .position(|task| task.id.as_deref() == Some(id))
        .ok_or_else(|| Error::TaskNotFound(id.into()))
}

fn emit_task(cli: &Cli, document: &todo_core::Document, id: &str) -> Result<u8, Error> {
    let view = views(document)
        .into_iter()
        .find(|task| task.id.as_deref() == Some(id))
        .ok_or_else(|| Error::TaskNotFound(id.into()))?;
    emit(
        cli,
        "todo.mutation.v1",
        MutationData { task: view },
        |data| human_task(&data.task),
    );
    Ok(0)
}

fn progress(
    cli: &Cli,
    path: &Path,
    id: &str,
    percent: u8,
    note: Option<&str>,
) -> Result<u8, Error> {
    if !(1..=99).contains(&percent) {
        // Routing 0 and 100 to the commands that own those states keeps a single way
        // to express "not started" and "done".
        return Err(Error::InvalidTaskLine(format!(
            "progress must be 1-99; got {percent}. Use `todo done` for complete, or leave progress unset for not started"
        )));
    }
    let writer = LockedWriter::new(path, Duration::from_millis(cli.lock_timeout_ms));
    let id_owned = id.to_string();
    let note_owned = note.map(str::to_string);
    let result = writer.update_labeled("progress", |document| {
        let index = index_of(document, &id_owned)?;
        let mut changes = vec![Change::SetMetadata {
            task: index,
            kind: TokenKind::Progress(percent),
            raw: format!("◐{percent}"),
        }];
        if let Some(text) = &note_owned {
            changes.push(Change::SetNote {
                task: index,
                text: text.clone(),
            });
        }
        Ok(changes)
    })?;
    emit_task(cli, &result, id)
}

fn fail(cli: &Cli, path: &Path, id: &str, note: Option<&str>) -> Result<u8, Error> {
    let today = chrono::Local::now().date_naive().to_string();
    let writer = LockedWriter::new(path, Duration::from_millis(cli.lock_timeout_ms));
    let id_owned = id.to_string();
    let note_owned = note.map(str::to_string);
    let result = writer.update_labeled("fail", |document| {
        let index = index_of(document, &id_owned)?;
        // Recording a failure never unchecks the task: status only moves forward
        // (spec §6.5.4). A failure on a done task is a signal for reconciliation,
        // not a licence to rewrite the completion claim.
        let mut changes = vec![Change::SetMetadata {
            task: index,
            kind: TokenKind::Failure(today.clone()),
            raw: format!("⚠{today}"),
        }];
        if let Some(text) = &note_owned {
            changes.push(Change::SetNote {
                task: index,
                text: text.clone(),
            });
        }
        Ok(changes)
    })?;
    emit_task(cli, &result, id)
}

fn set_note(
    cli: &Cli,
    path: &Path,
    id: &str,
    text: Option<&str>,
    clear: bool,
) -> Result<u8, Error> {
    if clear == text.is_some() {
        return Err(Error::InvalidTaskLine(
            "provide note text, or --clear to remove it".into(),
        ));
    }
    let writer = LockedWriter::new(path, Duration::from_millis(cli.lock_timeout_ms));
    let id_owned = id.to_string();
    let text_owned = text.map(str::to_string);
    let result = writer.update_labeled("note", |document| {
        let index = index_of(document, &id_owned)?;
        Ok(vec![match &text_owned {
            Some(text) => Change::SetNote {
                task: index,
                text: text.clone(),
            },
            None => Change::RemoveNote { task: index },
        }])
    })?;
    emit_task(cli, &result, id)
}

fn evidence(cli: &Cli, path: &Path, id: &str, from: &str, to: &str) -> Result<u8, Error> {
    let from = resolve_commit(from)?;
    let to = resolve_commit(to)?;
    let raw = format!("🔗{from}..{to}");
    let writer = LockedWriter::new(path, Duration::from_millis(cli.lock_timeout_ms));
    let id_owned = id.to_string();
    let result = writer.update_labeled("evidence", |document| {
        let index = document
            .tasks
            .iter()
            .position(|task| task.id.as_deref() == Some(&id_owned))
            .ok_or_else(|| Error::TaskNotFound(id_owned.clone()))?;
        Ok(vec![Change::SetMetadata {
            task: index,
            kind: TokenKind::Evidence {
                from: from.clone(),
                to: to.clone(),
            },
            raw: raw.clone(),
        }])
    })?;
    let view = views(&result)
        .into_iter()
        .find(|task| task.id.as_deref() == Some(id))
        .ok_or_else(|| Error::TaskNotFound(id.into()))?;
    emit(
        cli,
        "todo.mutation.v1",
        MutationData { task: view },
        |data| {
            format!(
                "recorded {raw} for {}",
                data.task.id.as_deref().unwrap_or("no-id")
            )
        },
    );
    Ok(0)
}

fn resolve_commit(value: &str) -> Result<String, Error> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--verify", &format!("{value}^{{commit}}")])
        .output()
        .map_err(|source| todo_io(Path::new("git"), source))?;
    if !output.status.success() {
        return Err(Error::InvalidTaskLine(format!(
            "git commit `{value}` does not exist"
        )));
    }
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if sha.len() != 40 || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(Error::InvalidTaskLine(format!(
            "git returned invalid commit sha for `{value}`"
        )));
    }
    Ok(sha)
}

fn next(cli: &Cli, path: &Path) -> Result<u8, Error> {
    let document = load(path)?;
    let completed: HashSet<_> = document
        .tasks
        .iter()
        .filter(|task| task.completed)
        .filter_map(|task| task.id.clone())
        .collect();
    let mut tasks: Vec<_> = views(&document)
        .into_iter()
        .filter(|task| {
            !task.completed
                && task.column.is_some()
                && task.dependencies.iter().all(|dep| completed.contains(dep))
        })
        .collect();
    tasks.sort_by(|a, b| {
        a.priority
            .unwrap_or(5)
            .cmp(&b.priority.unwrap_or(5))
            .then_with(|| {
                a.due
                    .as_deref()
                    .unwrap_or("9999-99-99")
                    .cmp(b.due.as_deref().unwrap_or("9999-99-99"))
            })
            .then(a.line.cmp(&b.line))
    });
    let task = tasks.into_iter().next();
    emit(cli, "todo.next.v1", NextData { task }, |data| {
        data.task
            .as_ref()
            .map(human_task)
            .unwrap_or_else(|| "no actionable task".into())
    });
    Ok(0)
}

fn archive(cli: &Cli, path: &Path) -> Result<u8, Error> {
    let mut document = load(path)?;
    let mut roots = archivable_roots(&document);
    if roots.is_empty() {
        return emit_archive(cli, 0, &archive_path(path));
    }

    let needs_ids = roots
        .iter()
        .flat_map(|(_, tasks)| tasks.clone())
        .any(|index| document.tasks[index].id.is_none());
    if needs_ids {
        let writer = LockedWriter::new(path, Duration::from_millis(cli.lock_timeout_ms));
        document = writer.update_labeled("archive", |current| {
            let mut existing: HashSet<String> = current
                .tasks
                .iter()
                .filter_map(|task| task.id.clone())
                .collect();
            let mut changes = Vec::new();
            for (_, tasks) in archivable_roots(current) {
                for index in tasks {
                    if current.tasks[index].id.is_none() {
                        let id = generate_task_id(&existing)?;
                        existing.insert(id.clone());
                        changes.push(Change::SetMetadata {
                            task: index,
                            kind: TokenKind::StableId(id.clone()),
                            raw: format!("<!--t:{id}-->"),
                        });
                    }
                }
            }
            Ok(changes)
        })?;
        roots = archivable_roots(&document);
    }

    let blocks = archive_blocks(&document, &roots)?;
    let archive_path = archive_path(path);
    ensure_archive_file(&archive_path)?;
    let archive_writer =
        LockedWriter::new(&archive_path, Duration::from_millis(cli.lock_timeout_ms));
    archive_writer.transform_labeled("archive", |current| merge_archive(current, &blocks))?;

    // Copy first, then delete only if the source bytes still match the copied blocks.
    // A crash or conflict may leave a duplicate, but can never lose the sole copy.
    let source_writer = LockedWriter::new(path, Duration::from_millis(cli.lock_timeout_ms));
    source_writer.update_labeled("archive", |current| {
        if let Some(id) = current.duplicate_ids().into_iter().next() {
            return Err(Error::DuplicateTaskId(id));
        }
        let mut changes = Vec::new();
        for block in &blocks {
            let index = current
                .tasks
                .iter()
                .position(|task| task.id.as_deref() == Some(block.root_id.as_str()))
                .ok_or(Error::ConcurrentModification)?;
            let tasks = task_subtree(current, index);
            if current.tasks[index].indent_level != 0
                || current.tasks[tasks.clone()]
                    .iter()
                    .any(|task| !task.completed)
                || current.source[subtree_bytes(current, tasks)].as_bytes() != block.raw.as_bytes()
            {
                return Err(Error::ConcurrentModification);
            }
            changes.push(Change::DeleteSubtree { task: index });
        }
        Ok(changes)
    })?;

    emit_archive(cli, blocks.len(), &archive_path)
}

fn emit_archive(cli: &Cli, archived: usize, path: &Path) -> Result<u8, Error> {
    let path = path.display().to_string();
    emit(
        cli,
        "todo.archive.v1",
        ArchiveData { archived, path },
        |data| {
            format!(
                "archived {} completed task(s) to {}",
                data.archived, data.path
            )
        },
    );
    Ok(0)
}

fn archive_path(path: &Path) -> PathBuf {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .join("TODO.archive.md")
}

fn ensure_archive_file(path: &Path) -> Result<(), Error> {
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => {
            use std::io::Write;
            file.write_all(ARCHIVE_TEMPLATE.as_bytes())
                .map_err(|source| todo_io(path, source))?;
            file.sync_all().map_err(|source| todo_io(path, source))?;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(source) => Err(todo_io(path, source)),
    }
}

fn archivable_roots(document: &todo_core::Document) -> Vec<(usize, std::ops::Range<usize>)> {
    document
        .tasks
        .iter()
        .enumerate()
        .filter(|(_, task)| task.indent_level == 0 && task.completed)
        .filter_map(|(index, _)| {
            let tasks = task_subtree(document, index);
            document.tasks[tasks.clone()]
                .iter()
                .all(|task| task.completed)
                .then_some((index, tasks))
        })
        .collect()
}

fn task_subtree(document: &todo_core::Document, root: usize) -> std::ops::Range<usize> {
    let level = document.tasks[root].indent_level;
    let end = document
        .tasks
        .iter()
        .enumerate()
        .skip(root + 1)
        .find(|(_, task)| task.indent_level <= level)
        .map_or(document.tasks.len(), |(index, _)| index);
    root..end
}

fn subtree_bytes(
    document: &todo_core::Document,
    tasks: std::ops::Range<usize>,
) -> std::ops::Range<usize> {
    let first = &document.tasks[tasks.start];
    let last = &document.tasks[tasks.end - 1];
    let end = last
        .note
        .as_ref()
        .map_or(last.full_line_range.end, |note| note.full_line_range.end);
    first.full_line_range.start..end
}

fn archive_blocks(
    document: &todo_core::Document,
    roots: &[(usize, std::ops::Range<usize>)],
) -> Result<Vec<ArchiveBlock>, Error> {
    if let Some(id) = document.duplicate_ids().into_iter().next() {
        return Err(Error::DuplicateTaskId(id));
    }
    roots
        .iter()
        .map(|(root, tasks)| {
            let root_id = document.tasks[*root]
                .id
                .clone()
                .ok_or(Error::MissingTaskId)?;
            let ids = document.tasks[tasks.clone()]
                .iter()
                .map(|task| task.id.clone().ok_or(Error::MissingTaskId))
                .collect::<Result<Vec<_>, _>>()?;
            let raw = document.source[subtree_bytes(document, tasks.clone())].to_string();
            Ok(ArchiveBlock { root_id, ids, raw })
        })
        .collect()
}

fn merge_archive(document: &todo_core::Document, blocks: &[ArchiveBlock]) -> Result<String, Error> {
    if let Some(id) = document.duplicate_ids().into_iter().next() {
        return Err(Error::DuplicateTaskId(id));
    }
    let insertion = document
        .columns
        .iter()
        .find(|column| column.id == "archived")
        .map(|column| column.content_range.end)
        .ok_or_else(|| Error::ColumnNotFound("archived".into()))?;
    let mut splices: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    let mut appended = String::new();

    for block in blocks {
        let existing_root = document.tasks.iter().position(|task| {
            task.indent_level == 0 && task.id.as_deref() == Some(block.root_id.as_str())
        });
        let existing_tasks = existing_root.map(|root| task_subtree(document, root));
        for id in &block.ids {
            if let Some(index) = document
                .tasks
                .iter()
                .position(|task| task.id.as_deref() == Some(id.as_str()))
            {
                if existing_tasks
                    .as_ref()
                    .is_none_or(|tasks| !tasks.contains(&index))
                {
                    return Err(Error::DuplicateTaskId(id.clone()));
                }
            }
        }

        if let Some(tasks) = existing_tasks {
            let range = subtree_bytes(document, tasks);
            let replacement = rendered_archive_block(&block.raw, document.line_ending.as_str());
            if document.source[range.clone()].as_bytes() != replacement.as_bytes() {
                splices.push((range, replacement));
            }
        } else {
            push_archive_block(&mut appended, &block.raw, document.line_ending.as_str());
        }
    }

    if !appended.is_empty() {
        let prefix = if insertion > 0 && !document.source[..insertion].ends_with('\n') {
            document.line_ending.as_str()
        } else {
            ""
        };
        splices.push((insertion..insertion, format!("{prefix}{appended}")));
    }
    if splices.is_empty() {
        return Ok(document.source.clone());
    }

    splices.sort_by_key(|(range, _)| range.start);
    let mut output = document.source.clone();
    for (range, replacement) in splices.into_iter().rev() {
        output.replace_range(range, &replacement);
    }
    let parsed = todo_core::parse(output.clone())?;
    if let Some(id) = parsed.duplicate_ids().into_iter().next() {
        return Err(Error::DuplicateTaskId(id));
    }
    Ok(output)
}

fn rendered_archive_block(raw: &str, newline: &str) -> String {
    let mut output = String::new();
    push_archive_block(&mut output, raw, newline);
    output
}

fn push_archive_block(output: &mut String, raw: &str, newline: &str) {
    if !output.is_empty() && !output.ends_with('\n') {
        output.push_str(newline);
    }
    output.push_str(raw);
    if !output.ends_with('\n') {
        output.push_str(newline);
    }
}

fn lint(cli: &Cli, path: &Path) -> Result<u8, Error> {
    if !path.exists() {
        return Err(Error::NotTaskFile);
    }
    let mut issues = Vec::new();
    let raw = fs::read_to_string(path).map_err(|source| todo_io(path, source))?;
    match todo_core::parse(raw) {
        Ok(document) => {
            for id in document.duplicate_ids() {
                let line = document
                    .tasks
                    .iter()
                    .filter(|task| task.id.as_deref() == Some(id.as_str()))
                    .nth(1)
                    .map(|task| {
                        document.source.as_bytes()[..task.line_range.start]
                            .iter()
                            .filter(|byte| **byte == b'\n')
                            .count()
                            + 1
                    });
                issues.push(LintIssue {
                    severity: "error",
                    code: "duplicate-id",
                    line,
                    message: format!("duplicate task ID `{id}` violates global uniqueness"),
                });
            }
            for item in document.unrecognized_task_like_lines {
                issues.push(LintIssue {
                    severity: "warning",
                    code: "nonstandard-checkbox",
                    line: Some(item.line),
                    message: format!(
                        "line {} looks like a task but uses a nonstandard checkbox",
                        item.line
                    ),
                });
            }
        }
        Err(error) => issues.push(LintIssue {
            severity: "error",
            code: "invalid-document",
            line: None,
            message: error.to_string(),
        }),
    }
    let code = if issues.is_empty() { 0 } else { 6 };
    emit(cli, "todo.lint.v1", LintData { issues }, |data| {
        if data.issues.is_empty() {
            "no issues".into()
        } else {
            data.issues
                .iter()
                .map(|issue| format!("{}: {}", issue.code, issue.message))
                .collect::<Vec<_>>()
                .join("\n")
        }
    });
    Ok(code)
}

fn reconcile(cli: &Cli, path: &Path, apply: bool, no_semantic: bool) -> Result<u8, Error> {
    let document = load(path)?;
    let options = ReconcileOptions {
        semantic: !no_semantic,
    };
    let report = Reconciler::new(".").reconcile(&document, &options)?;
    let applicable = report
        .confirmed
        .iter()
        .filter(|finding| finding.suggested_change.is_some())
        .count();
    let findings_requiring_manual_action =
        report.confirmed.len() + report.suspected.len() - applicable;
    let mut applied = 0;
    if apply && applicable > 0 {
        let writer = LockedWriter::new(path, Duration::from_millis(cli.lock_timeout_ms));
        writer.update_labeled("reconcile-apply", |current| {
            let fresh = Reconciler::new(".").reconcile(current, &options)?;
            let changes: Vec<_> = fresh
                .confirmed
                .into_iter()
                .filter_map(|finding| finding.suggested_change)
                .collect();
            applied = changes.len();
            Ok(changes)
        })?;
    }
    // Findings outrank an incomplete run: if something concrete was found the caller
    // already has work to do. But "nothing found" while a layer never ran must not
    // report success, or a shallow-clone CI job reads an unfinished inspection as a
    // clean bill of health.
    let code = if report.has_findings() {
        6
    } else if report.incomplete() {
        7
    } else {
        0
    };
    emit(
        cli,
        "todo.reconcile.v2",
        ReconcileData {
            report,
            applied,
            findings_requiring_manual_action,
        },
        reconcile_human,
    );
    Ok(code)
}

fn reconcile_human(data: &ReconcileData) -> String {
    let report = &data.report;
    let mut sections = Vec::new();

    if report.has_findings() {
        sections.push(
            report
                .confirmed
                .iter()
                .map(|finding| format!("confirmed: {}", finding.judgment))
                .chain(
                    report
                        .suspected
                        .iter()
                        .map(|finding| format!("suspected: {}", finding.judgment)),
                )
                .collect::<Vec<_>>()
                .join("\n"),
        );
    } else if report.incomplete() {
        // Never the bare "no findings" line when a layer was skipped: that sentence
        // reads as "checked and clean" and is exactly the claim we cannot make.
        sections.push("no findings from the checks that ran — this run was INCOMPLETE".into());
    } else {
        sections.push("no reconciliation findings (all checks ran)".into());
    }

    for reason in &report.checks_skipped {
        sections.push(format!("NOT CHECKED: {reason}"));
    }

    sections.push(format!(
        "applied {}, {} finding(s) need manual judgment, {} check(s) did not run",
        data.applied,
        data.findings_requiring_manual_action,
        report.checks_skipped.len()
    ));
    sections.join("\n")
}

fn load(path: &Path) -> Result<todo_core::Document, Error> {
    if !path.exists() {
        Err(Error::NotTaskFile)
    } else {
        parse_file(path)
    }
}
fn projected_views(path: &Path, document: &todo_core::Document) -> Result<Vec<TaskView>, Error> {
    let mut projected: HashMap<_, _> = todo_core::project_document(path, document)?
        .into_iter()
        .map(|task| (task.id.clone(), task))
        .collect();
    Ok(views(document)
        .into_iter()
        .map(|view| {
            let Some(task) = view.id.as_ref().and_then(|id| projected.remove(id)) else {
                return view;
            };
            TaskView {
                id: Some(task.id),
                body: task.body,
                completed: task.completed,
                column: task.column,
                indent_level: task.indent_level,
                parent_id: task.parent_id,
                due: task.due,
                tags: task.tags,
                assignees: task.assignees,
                priority: task.priority,
                order_key: task.order_key,
                dependencies: task.deps,
                progress: task.progress,
                failed_on: task.failed_on,
                note: task.note,
                line: task.line,
            }
        })
        .collect())
}

fn views(document: &todo_core::Document) -> Vec<TaskView> {
    document
        .tasks
        .iter()
        .enumerate()
        .map(|(index, task)| {
            let mut due = None;
            let mut tags = Vec::new();
            let mut assignees = Vec::new();
            let mut priority = None;
            let mut order_key = None;
            let mut dependencies = Vec::new();
            for token in &task.metadata {
                match &token.kind {
                    TokenKind::DueDate(v) => due = Some(v.clone()),
                    TokenKind::Tag(v) => tags.push(v.clone()),
                    TokenKind::Assignee(v) => assignees.push(v.clone()),
                    TokenKind::Priority(v) => priority = Some(*v),
                    TokenKind::Sort(v) => order_key = Some(*v),
                    TokenKind::Dependency(v) => dependencies.push(v.clone()),
                    _ => {}
                }
            }
            TaskView {
                id: task.id.clone(),
                body: task.body.clone(),
                completed: task.completed,
                column: task.column.clone(),
                indent_level: task.indent_level,
                parent_id: task
                    .parent
                    .and_then(|parent| document.tasks[parent].id.clone()),
                due,
                tags,
                assignees,
                priority,
                order_key,
                dependencies,
                progress: task.progress(),
                failed_on: task.failure().map(str::to_string),
                note: task.note.as_ref().map(|note| note.text.clone()),
                line: document.source.as_bytes()[..document.tasks[index].line_range.start]
                    .iter()
                    .filter(|byte| **byte == b'\n')
                    .count()
                    + 1,
            }
        })
        .collect()
}
fn emit<T: Serialize>(cli: &Cli, schema: &'static str, data: T, human: impl FnOnce(&T) -> String) {
    if cli.json {
        println!(
            "{}",
            serde_json::to_string(&Envelope {
                schema,
                ok: true,
                data
            })
            .unwrap()
        );
    } else {
        let value = human(&data);
        if !value.is_empty() {
            println!("{value}");
        }
    }
}
fn emit_error(cli: &Cli, error: &Error) {
    if cli.json {
        println!(
            "{}",
            serde_json::json!({"schema":"todo.error.v1","ok":false,"error":{"code":error_code(error),"message":error.to_string()}})
        );
    } else {
        eprintln!("error: {error}");
    }
}
fn error_code(error: &Error) -> u8 {
    match error {
        Error::NotTaskFile => 3,
        Error::LockTimeout { .. } => 4,
        Error::ConcurrentModification => 5,
        _ => 1,
    }
}
fn human_task(task: &TaskView) -> String {
    // Partial state is shown inline: a human reading `[ ] body` alone would read a
    // 60%-done task with a failed attempt as untouched work.
    let mut suffix = String::new();
    if let Some(percent) = task.progress {
        suffix.push_str(&format!(" ◐{percent}%"));
    }
    if let Some(date) = &task.failed_on {
        suffix.push_str(&format!(" ⚠failed {date}"));
    }
    let note = task
        .note
        .as_deref()
        .map(|text| format!("\n      ↳ {text}"))
        .unwrap_or_default();
    format!(
        "{} [{}] {}{suffix}{note}",
        if task.completed { "x" } else { " " },
        task.id.as_deref().unwrap_or("no-id"),
        task.body
    )
}
fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
}
fn adopt_source(source: &str) -> Result<String, Error> {
    let mut out = String::from(
        "---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n  - {id: doing, name: Doing}\n  - {id: done, name: Done, type: done}\n---\n\n## Todo\n",
    );
    let mut existing = HashSet::new();
    let mut fence: Option<(u8, usize)> = None;
    for line in source.split_inclusive('\n') {
        let content = line.trim_end_matches(['\r', '\n']);
        let trimmed = content.trim_start_matches([' ', '\t']);
        let fence_byte = trimmed
            .as_bytes()
            .first()
            .copied()
            .filter(|byte| matches!(byte, b'`' | b'~'));
        let fence_len = trimmed
            .bytes()
            .take_while(|byte| Some(*byte) == fence_byte)
            .count();

        if let Some((byte, minimum)) = fence {
            if Some(byte) == fence_byte
                && fence_len >= minimum
                && trimmed[fence_len..].trim().is_empty()
            {
                fence = None;
            }
            out.push_str(line);
            continue;
        }
        if fence_len >= 3 {
            fence = Some((
                fence_byte.expect("a non-zero fence has a delimiter"),
                fence_len,
            ));
            out.push_str(line);
            continue;
        }

        if (trimmed.starts_with("- [ ]")
            || trimmed.starts_with("- [x]")
            || trimmed.starts_with("- [X]"))
            && !line.contains("<!--t:")
        {
            let ending = if line.ends_with('\n') { "\n" } else { "" };
            out.push_str(content);
            let id = generate_task_id(&existing)?;
            existing.insert(id.clone());
            out.push_str(&format!(" <!--t:{id}-->{ending}"));
        } else {
            out.push_str(line);
        }
    }
    out.push_str("\n## Doing\n\n## Done\n");
    Ok(out)
}
fn todo_io(path: &Path, source: std::io::Error) -> Error {
    Error::Io {
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_modification_uses_exit_code_five() {
        assert_eq!(error_code(&Error::ConcurrentModification), 5);
    }
}
