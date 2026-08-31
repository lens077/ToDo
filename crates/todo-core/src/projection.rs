use std::{collections::HashMap, path::Path};

use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::{Document, Result, TokenKind};

const SCHEMA: &str = r#"
CREATE TABLE metadata(key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE files(path TEXT PRIMARY KEY, mtime INTEGER, hash TEXT NOT NULL);
CREATE TABLE columns(file_path TEXT NOT NULL, id TEXT NOT NULL, name TEXT NOT NULL, kind TEXT, position INTEGER NOT NULL, PRIMARY KEY(file_path,id));
CREATE TABLE tasks(
  id TEXT PRIMARY KEY, body TEXT NOT NULL, completed INTEGER NOT NULL, column_id TEXT,
  indent_level INTEGER NOT NULL, parent_id TEXT, due TEXT, tags TEXT NOT NULL,
  assignees TEXT NOT NULL, priority INTEGER, order_key INTEGER, evidence TEXT NOT NULL,
  deps TEXT NOT NULL, file_path TEXT NOT NULL, line INTEGER NOT NULL, source_hash TEXT NOT NULL
);
CREATE INDEX tasks_file_line ON tasks(file_path,line);
"#;

pub const SCHEMA_HASH: &str = "todo-core-schema-v1-9326a2c1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedTask {
    pub id: String,
    pub body: String,
    pub completed: bool,
    pub column: Option<String>,
    pub indent_level: usize,
    pub parent_id: Option<String>,
    pub due: Option<String>,
    pub tags: Vec<String>,
    pub assignees: Vec<String>,
    pub priority: Option<u8>,
    pub order_key: Option<i64>,
    pub evidence: Vec<String>,
    pub deps: Vec<String>,
    pub file_path: String,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionSnapshot {
    pub tasks: Vec<ProjectedTask>,
    pub columns: Vec<(String, String, String, Option<String>, usize)>,
    pub files: Vec<(String, Option<i64>, String)>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UpdateStats {
    pub inserted: usize,
    pub updated: usize,
    pub deleted: usize,
}

pub struct Projection {
    connection: Connection,
}

impl Projection {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let connection = Connection::open(path)?;
        connection.pragma_update(None, "foreign_keys", true)?;
        let mut projection = Self { connection };
        projection.ensure_schema()?;
        Ok(projection)
    }

    pub fn open_in_memory() -> Result<Self> {
        let connection = Connection::open_in_memory()?;
        let mut projection = Self { connection };
        projection.ensure_schema()?;
        Ok(projection)
    }

    pub fn rebuild(&mut self, documents: &[(&Path, &Document)]) -> Result<()> {
        let transaction = self.connection.transaction()?;
        clear_derived(&transaction)?;
        for (path, document) in documents {
            project_file(&transaction, path, document, true)?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn update_file(&mut self, path: &Path, document: &Document) -> Result<UpdateStats> {
        let transaction = self.connection.transaction()?;
        let path_text = path.to_string_lossy();
        let mut statement =
            transaction.prepare("SELECT id,source_hash FROM tasks WHERE file_path=?1")?;
        let old: HashMap<String, String> = statement
            .query_map([path_text.as_ref()], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<std::result::Result<_, _>>()?;
        drop(statement);
        let new = task_rows(document, path);
        let mut stats = UpdateStats::default();
        for id in old.keys() {
            if !new.iter().any(|row| &row.id == id) {
                transaction.execute("DELETE FROM tasks WHERE id=?1", [id])?;
                stats.deleted += 1;
            }
        }
        for row in &new {
            match old.get(&row.id) {
                None => {
                    insert_task(&transaction, row)?;
                    stats.inserted += 1;
                }
                Some(hash) if hash != &row.source_hash => {
                    update_task(&transaction, row)?;
                    stats.updated += 1;
                }
                _ => {}
            }
        }
        transaction.execute(
            "DELETE FROM columns WHERE file_path=?1",
            [path_text.as_ref()],
        )?;
        project_columns(&transaction, path, document)?;
        upsert_file(&transaction, path, document)?;
        transaction.commit()?;
        Ok(stats)
    }

    pub fn tasks(&self) -> Result<Vec<ProjectedTask>> {
        let mut statement = self.connection.prepare(
            "SELECT id,body,completed,column_id,indent_level,parent_id,due,tags,assignees,priority,order_key,evidence,deps,file_path,line FROM tasks ORDER BY file_path,line",
        )?;
        let values = statement
            .query_map([], |row| {
                Ok(ProjectedTask {
                    id: row.get(0)?,
                    body: row.get(1)?,
                    completed: row.get::<_, i64>(2)? != 0,
                    column: row.get(3)?,
                    indent_level: row.get::<_, i64>(4)? as usize,
                    parent_id: row.get(5)?,
                    due: row.get(6)?,
                    tags: decode_list(&row.get::<_, String>(7)?),
                    assignees: decode_list(&row.get::<_, String>(8)?),
                    priority: row.get::<_, Option<i64>>(9)?.map(|v| v as u8),
                    order_key: row.get(10)?,
                    evidence: decode_list(&row.get::<_, String>(11)?),
                    deps: decode_list(&row.get::<_, String>(12)?),
                    file_path: row.get(13)?,
                    line: row.get::<_, i64>(14)? as usize,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        Ok(values)
    }

    pub fn snapshot(&self) -> Result<ProjectionSnapshot> {
        let tasks = self.tasks()?;
        let mut column_statement = self.connection.prepare(
            "SELECT file_path,id,name,kind,position FROM columns ORDER BY file_path,position",
        )?;
        let columns = column_statement
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get::<_, i64>(4)? as usize,
                ))
            })?
            .collect::<std::result::Result<_, _>>()?;
        drop(column_statement);
        let mut file_statement = self
            .connection
            .prepare("SELECT path,mtime,hash FROM files ORDER BY path")?;
        let files = file_statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<std::result::Result<_, _>>()?;
        Ok(ProjectionSnapshot {
            tasks,
            columns,
            files,
        })
    }

    pub fn task_rowid(&self, id: &str) -> Result<Option<i64>> {
        Ok(self
            .connection
            .query_row("SELECT rowid FROM tasks WHERE id=?1", [id], |row| {
                row.get(0)
            })
            .optional()?)
    }

    pub fn schema_hash(&self) -> Result<String> {
        Ok(self.connection.query_row(
            "SELECT value FROM metadata WHERE key='schema_hash'",
            [],
            |row| row.get(0),
        )?)
    }

    #[doc(hidden)]
    pub fn set_schema_hash_for_test(&self, value: &str) -> Result<()> {
        self.connection.execute(
            "UPDATE metadata SET value=?1 WHERE key='schema_hash'",
            [value],
        )?;
        Ok(())
    }

    fn ensure_schema(&mut self) -> Result<()> {
        let metadata_exists: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='metadata')",
            [],
            |row| row.get(0),
        )?;
        let matches = metadata_exists
            && self
                .connection
                .query_row(
                    "SELECT value FROM metadata WHERE key='schema_hash'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .as_deref()
                == Some(SCHEMA_HASH);
        if !matches {
            let transaction = self.connection.transaction()?;
            for table in ["tasks", "columns", "files", "metadata"] {
                transaction.execute(&format!("DROP TABLE IF EXISTS {table}"), [])?;
            }
            transaction.execute_batch(SCHEMA)?;
            transaction.execute(
                "INSERT INTO metadata(key,value) VALUES('schema_hash',?1)",
                [SCHEMA_HASH],
            )?;
            transaction.commit()?;
        }
        Ok(())
    }
}

#[derive(Debug)]
struct TaskRow {
    id: String,
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
    evidence: Vec<String>,
    deps: Vec<String>,
    file_path: String,
    line: usize,
    source_hash: String,
}

fn task_rows(document: &Document, path: &Path) -> Vec<TaskRow> {
    document
        .tasks
        .iter()
        .filter_map(|task| {
            let id = task.id.clone()?;
            let mut due = None;
            let mut tags = Vec::new();
            let mut assignees = Vec::new();
            let mut priority = None;
            let mut order_key = None;
            let mut evidence = Vec::new();
            let mut deps = Vec::new();
            for token in &task.metadata {
                match &token.kind {
                    TokenKind::Tag(value) => tags.push(value.clone()),
                    TokenKind::Assignee(value) => assignees.push(value.clone()),
                    TokenKind::DueDate(value) => due = Some(value.clone()),
                    TokenKind::Priority(value) => priority = Some(*value),
                    TokenKind::Sort(value) => order_key = Some(*value),
                    TokenKind::Evidence { from, to } => evidence.push(format!("{from}..{to}")),
                    TokenKind::Dependency(value) => deps.push(value.clone()),
                    _ => {}
                }
            }
            let parent_id = task
                .parent
                .and_then(|parent| document.tasks[parent].id.clone());
            let raw = &document.source[task.line_range.clone()];
            Some(TaskRow {
                id,
                body: task.body.clone(),
                completed: task.completed,
                column: task.column.clone(),
                indent_level: task.indent_level,
                parent_id,
                due,
                tags,
                assignees,
                priority,
                order_key,
                evidence,
                deps,
                file_path: path.to_string_lossy().into_owned(),
                line: document.source.as_bytes()[..task.line_range.start]
                    .iter()
                    .filter(|byte| **byte == b'\n')
                    .count()
                    + 1,
                source_hash: format!("{:016x}", crate::parse::hash_bytes(raw.as_bytes())),
            })
        })
        .collect()
}

fn clear_derived(transaction: &Transaction<'_>) -> Result<()> {
    transaction.execute("DELETE FROM tasks", [])?;
    transaction.execute("DELETE FROM columns", [])?;
    transaction.execute("DELETE FROM files", [])?;
    Ok(())
}

fn project_file(
    transaction: &Transaction<'_>,
    path: &Path,
    document: &Document,
    insert: bool,
) -> Result<()> {
    project_columns(transaction, path, document)?;
    upsert_file(transaction, path, document)?;
    if insert {
        for row in task_rows(document, path) {
            insert_task(transaction, &row)?;
        }
    }
    Ok(())
}

fn project_columns(transaction: &Transaction<'_>, path: &Path, document: &Document) -> Result<()> {
    for (position, column) in document.columns.iter().enumerate() {
        transaction.execute(
            "INSERT INTO columns(file_path,id,name,kind,position) VALUES(?1,?2,?3,?4,?5)",
            params![
                path.to_string_lossy(),
                column.id,
                column.name,
                column.kind,
                position as i64
            ],
        )?;
    }
    Ok(())
}

fn upsert_file(transaction: &Transaction<'_>, path: &Path, document: &Document) -> Result<()> {
    let mtime = document
        .snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.modified)
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos().min(i64::MAX as u128) as i64);
    transaction.execute("INSERT INTO files(path,mtime,hash) VALUES(?1,?2,?3) ON CONFLICT(path) DO UPDATE SET mtime=excluded.mtime,hash=excluded.hash",
        params![path.to_string_lossy(), mtime, format!("{:016x}", crate::parse::hash_bytes(document.source.as_bytes()))])?;
    Ok(())
}

fn insert_task(transaction: &Transaction<'_>, row: &TaskRow) -> Result<()> {
    execute_task(transaction, "INSERT INTO tasks(id,body,completed,column_id,indent_level,parent_id,due,tags,assignees,priority,order_key,evidence,deps,file_path,line,source_hash) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)", row)
}
fn update_task(transaction: &Transaction<'_>, row: &TaskRow) -> Result<()> {
    execute_task(transaction, "UPDATE tasks SET body=?2,completed=?3,column_id=?4,indent_level=?5,parent_id=?6,due=?7,tags=?8,assignees=?9,priority=?10,order_key=?11,evidence=?12,deps=?13,file_path=?14,line=?15,source_hash=?16 WHERE id=?1", row)
}
fn execute_task(transaction: &Transaction<'_>, sql: &str, row: &TaskRow) -> Result<()> {
    let tags = encode_list(&row.tags);
    let assignees = encode_list(&row.assignees);
    let evidence = encode_list(&row.evidence);
    let deps = encode_list(&row.deps);
    transaction.execute(
        sql,
        params![
            row.id,
            row.body,
            row.completed as i64,
            row.column,
            row.indent_level as i64,
            row.parent_id,
            row.due,
            tags,
            assignees,
            row.priority.map(i64::from),
            row.order_key,
            evidence,
            deps,
            row.file_path,
            row.line as i64,
            row.source_hash
        ],
    )?;
    Ok(())
}

fn encode_list(values: &[String]) -> String {
    values.join("\u{1f}")
}
fn decode_list(value: &str) -> Vec<String> {
    if value.is_empty() {
        Vec::new()
    } else {
        value.split('\u{1f}').map(str::to_owned).collect()
    }
}
