mod hooks;

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
    time::Duration,
};

use clap::{Parser, Subcommand};
use serde::Serialize;
use todo_core::{
    generate_task_id, parse_file, Change, Error, InsertPosition, LockedWriter, ReconcileOptions,
    ReconcileReport, Reconciler, TokenKind,
};

const TODO_TEMPLATE: &str = "---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n  - {id: doing, name: Doing}\n  - {id: done, name: Done, type: done}\n---\n\n# Project tasks\n\n## Todo\n\n## Doing\n\n## Done\n";

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
    Evidence {
        id: String,
        #[arg(long)]
        from: String,
        #[arg(long, default_value = "HEAD")]
        to: String,
    },
    Next,
    Lint,
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
    skipped_manual: usize,
}
#[derive(Serialize)]
struct LintData {
    issues: Vec<LintIssue>,
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
            let mut tasks: Vec<_> = views(&doc)
                .into_iter()
                .filter(|task| *all || (!task.completed && task.column.is_some()))
                .filter(|task| {
                    column
                        .as_ref()
                        .map_or(true, |column| task.column.as_ref() == Some(column))
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
        Command::Evidence { id, from, to } => evidence(cli, &path, id, from, to),
        Command::Next => next(cli, &path),
        Command::Lint => lint(cli, &path),
        Command::Reconcile { apply, no_semantic } => reconcile(cli, &path, *apply, *no_semantic),
        Command::Hooks { command } => run_hooks(cli, command),
    }
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
    if path.exists() {
        if !adopt {
            return Err(Error::FileAlreadyExists {
                path: path.to_path_buf(),
            });
        }
        let original = fs::read_to_string(path).map_err(|source| todo_io(path, source))?;
        if original.starts_with("---\n") {
            return Err(Error::InvalidTaskLine(
                "TODO.md already has frontmatter".into(),
            ));
        }
        let adopted = adopt_source(&original)?;
        fs::write(path, adopted).map_err(|source| todo_io(path, source))?;
    } else {
        fs::write(path, TODO_TEMPLATE).map_err(|source| todo_io(path, source))?;
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
    let result = writer.update(|_| {
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
    let result = writer.update(|document| {
        let index = document
            .tasks
            .iter()
            .position(|task| task.id.as_deref() == Some(&id_owned))
            .ok_or_else(|| Error::TaskNotFound(id_owned.clone()))?;
        Ok(vec![Change::SetCompleted {
            task: index,
            completed,
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
        |data| human_task(&data.task),
    );
    Ok(0)
}

fn evidence(cli: &Cli, path: &Path, id: &str, from: &str, to: &str) -> Result<u8, Error> {
    let from = resolve_commit(from)?;
    let to = resolve_commit(to)?;
    let raw = format!("🔗{from}..{to}");
    let writer = LockedWriter::new(path, Duration::from_millis(cli.lock_timeout_ms));
    let id_owned = id.to_string();
    let result = writer.update(|document| {
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

fn lint(cli: &Cli, path: &Path) -> Result<u8, Error> {
    if !path.exists() {
        return Err(Error::NotTaskFile);
    }
    let mut issues = Vec::new();
    let raw = fs::read_to_string(path).map_err(|source| todo_io(path, source))?;
    match todo_core::parse(raw) {
        Ok(document) => {
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
    let skipped_manual = report.confirmed.len() + report.suspected.len() - applicable;
    let mut applied = 0;
    if apply && applicable > 0 {
        let writer = LockedWriter::new(path, Duration::from_millis(cli.lock_timeout_ms));
        writer.update(|current| {
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
    let code = if report.confirmed.is_empty() && report.suspected.is_empty() {
        0
    } else {
        6
    };
    emit(
        cli,
        "todo.reconcile.v1",
        ReconcileData {
            report,
            applied,
            skipped_manual,
        },
        reconcile_human,
    );
    Ok(code)
}

fn reconcile_human(data: &ReconcileData) -> String {
    let report = &data.report;
    if report.confirmed.is_empty() && report.suspected.is_empty() {
        return format!(
            "no reconciliation findings\napplied {}, skipped {} (manual judgment required)",
            data.applied, data.skipped_manual
        );
    }
    let findings = report
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
        .join("\n");
    format!(
        "{findings}\napplied {}, skipped {} (manual judgment required)",
        data.applied, data.skipped_manual
    )
}

fn load(path: &Path) -> Result<todo_core::Document, Error> {
    if !path.exists() {
        Err(Error::NotTaskFile)
    } else {
        parse_file(path)
    }
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
    format!(
        "{} [{}] {}",
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
    let mut out=String::from("---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n  - {id: doing, name: Doing}\n  - {id: done, name: Done, type: done}\n---\n\n## Todo\n");
    let mut existing = HashSet::new();
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start_matches([' ', '\t']);
        if (trimmed.starts_with("- [ ]")
            || trimmed.starts_with("- [x]")
            || trimmed.starts_with("- [X]"))
            && !line.contains("<!--t:")
        {
            let ending = if line.ends_with("\n") { "\n" } else { "" };
            out.push_str(line.trim_end_matches(['\r', '\n']));
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
