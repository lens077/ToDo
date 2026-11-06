//! Multi-project workspace registry (`docs/workspace-knowledge-phase0-audit.md`).
//!
//! The registry is the single source of truth for workspace membership and the
//! stable project identity: a project keeps its UUID when its directory moves
//! and is re-linked. It lives in `~/.todo/workspace/workspace.sqlite`, follows
//! the [`crate::reading`] store pattern (per-call connection, WAL), and adds
//! what the reading store lacks: a versioned `meta` table with explicit,
//! fail-loud migrations so a schema gap can never silently degrade into empty
//! data.

use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

const SCHEMA_VERSION: i64 = 4;

/// Serializable views for the desktop commands.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub projects: Vec<ProjectSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectSummary {
    pub id: String,
    /// Current real directory; empty when the project is missing and the
    /// recorded path is used as the re-link hint.
    pub root_path: String,
    /// User-facing name; defaults to the directory name.
    pub name: String,
    /// `reachable` / `missing` — checked against the filesystem at read time.
    pub path_status: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_opened_at: Option<i64>,
    /// Workspaces this project belongs to; empty in per-workspace listings.
    pub workspace_ids: Vec<String>,
}

/// Result of adding a directory: dedup by canonical path never duplicates.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AddedProject {
    pub project: ProjectSummary,
    pub created: bool,
}

#[derive(Clone)]
pub struct WorkspaceStore {
    root: PathBuf,
}

impl WorkspaceStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn global() -> Result<Self> {
        let home = dirs::home_dir().ok_or(Error::HomeDirectoryUnavailable)?;
        Ok(Self::new(home.join(".todo").join("workspace")))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn connect(&self) -> Result<Connection> {
        fs::create_dir_all(&self.root).map_err(|source| Error::Io {
            path: self.root.clone(),
            source,
        })?;
        let connection = Connection::open(self.root.join("workspace.sqlite"))?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
        migrate(&connection)?;
        Ok(connection)
    }

    /// Creates the default workspace on first launch. The name is stable so
    /// repeated calls are no-ops.
    pub fn ensure_default_workspace(&self) -> Result<String> {
        let connection = self.connect()?;
        let existing: Option<String> = connection
            .query_row(
                "SELECT id FROM workspaces ORDER BY created_at LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(id) = existing {
            return Ok(id);
        }
        let now = now_ms();
        let id = new_id();
        connection.execute(
            "INSERT INTO workspaces (id, name, description, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, "默认工作空间", "由单目录模式迁移而来", now, now],
        )?;
        Ok(id)
    }

    pub fn create_workspace(&self, name: &str, description: &str) -> Result<WorkspaceSummary> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::WorkspaceInvalid("工作空间名称不能为空".into()));
        }
        let connection = self.connect()?;
        let now = now_ms();
        let id = new_id();
        connection.execute(
            "INSERT INTO workspaces (id, name, description, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, name, description.trim(), now, now],
        )?;
        self.workspace(&connection, &id)
    }

    pub fn rename_workspace(&self, id: &str, name: &str) -> Result<WorkspaceSummary> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::WorkspaceInvalid("工作空间名称不能为空".into()));
        }
        let connection = self.connect()?;
        let changed = connection.execute(
            "UPDATE workspaces SET name = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, name, now_ms()],
        )?;
        if changed == 0 {
            return Err(Error::WorkspaceNotFound(id.into()));
        }
        self.workspace(&connection, id)
    }

    /// Deletes a workspace and its membership rows. Projects survive: the
    /// workspace is only a container.
    pub fn delete_workspace(&self, id: &str) -> Result<()> {
        let connection = self.connect()?;
        let changed = connection.execute("DELETE FROM workspaces WHERE id = ?1", params![id])?;
        if changed == 0 {
            return Err(Error::WorkspaceNotFound(id.into()));
        }
        Ok(())
    }

    pub fn list_workspaces(&self) -> Result<Vec<WorkspaceSummary>> {
        let connection = self.connect()?;
        let ids: Vec<String> = {
            let mut statement =
                connection.prepare("SELECT id FROM workspaces ORDER BY created_at")?;
            let rows = statement.query_map([], |row| row.get(0))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };
        ids.iter()
            .map(|id| self.workspace(&connection, id))
            .collect()
    }

    fn workspace(&self, connection: &Connection, id: &str) -> Result<WorkspaceSummary> {
        connection
            .query_row(
                "SELECT id, name, description, created_at, updated_at
                 FROM workspaces WHERE id = ?1",
                params![id],
                |row| {
                    Ok(WorkspaceSummary {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        description: row.get(2)?,
                        created_at: row.get(3)?,
                        updated_at: row.get(4)?,
                        projects: Vec::new(),
                    })
                },
            )
            .optional()?
            .ok_or_else(|| Error::WorkspaceNotFound(id.into()))
            .map(|mut summary| {
                summary.projects = self.workspace_projects(connection, id);
                summary
            })
    }

    fn workspace_projects(
        &self,
        connection: &Connection,
        workspace_id: &str,
    ) -> Vec<ProjectSummary> {
        let mut statement = connection
            .prepare(
                "SELECT p.id, p.root_path, p.name, p.created_at, p.updated_at, p.last_opened_at
                 FROM workspace_projects wp
                 JOIN projects p ON p.id = wp.project_id
                 WHERE wp.workspace_id = ?1
                 ORDER BY wp.sort_order, wp.added_at",
            )
            .expect("workspace_projects query is static");
        let rows = statement
            .query_map(params![workspace_id], project_from_row)
            .expect("workspace_projects query is static");
        rows.map(|row| row.map(finalize_project))
            .collect::<std::result::Result<Vec<_>, _>>()
            .expect("workspace_projects rows are plain data")
    }

    /// Registers a directory as a project and attaches it to the workspace.
    ///
    /// Dedup is by canonicalized absolute path: re-adding a known directory
    /// returns the existing project (and repairs a missing membership) instead
    /// of creating a second identity.
    pub fn add_project(&self, workspace_id: &str, path: &Path) -> Result<AddedProject> {
        let canonical = canonical_root(path)?;
        let connection = self.connect()?;
        let existing: Option<String> = connection
            .query_row(
                "SELECT id FROM projects WHERE canonical_path = ?1",
                params![canonical.to_string_lossy()],
                |row| row.get(0),
            )
            .optional()?;
        let (project_id, created) = match existing {
            Some(id) => (id, false),
            None => {
                let id = new_id();
                let now = now_ms();
                let name = canonical
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| canonical.to_string_lossy().to_string());
                connection.execute(
                    "INSERT INTO projects (id, canonical_path, root_path, name, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        id,
                        canonical.to_string_lossy(),
                        canonical.to_string_lossy(),
                        name,
                        now,
                        now
                    ],
                )?;
                (id, true)
            }
        };
        // Repair the membership even for an existing project: a prior removal
        // (or an interrupted run) must not make re-adding silently do nothing.
        let now = now_ms();
        let member: Option<i64> = connection
            .query_row(
                "SELECT 1 FROM workspace_projects WHERE workspace_id = ?1 AND project_id = ?2",
                params![workspace_id, project_id],
                |row| row.get(0),
            )
            .optional()?;
        if member.is_none() {
            connection.execute(
                "INSERT INTO workspace_projects (workspace_id, project_id, sort_order, added_at)
                 VALUES (?1, ?2,
                         COALESCE((SELECT MAX(sort_order) + 1 FROM workspace_projects
                                   WHERE workspace_id = ?1), 0),
                         ?3)",
                params![workspace_id, project_id, now],
            )?;
        }
        let project = self.project(&connection, &project_id)?;
        Ok(AddedProject { project, created })
    }

    /// Moves a project to a new directory. The stable ID, membership, and every
    /// future association (sessions, reading links) survive the move.
    pub fn relink_project(&self, project_id: &str, path: &Path) -> Result<ProjectSummary> {
        let canonical = canonical_root(path)?;
        let connection = self.connect()?;
        let duplicate: Option<String> = connection
            .query_row(
                "SELECT id FROM projects WHERE canonical_path = ?1 AND id != ?2",
                params![canonical.to_string_lossy(), project_id],
                |row| row.get(0),
            )
            .optional()?;
        if duplicate.is_some() {
            return Err(Error::WorkspacePathAlreadyRegistered(
                canonical.to_string_lossy().to_string(),
            ));
        }
        let changed = connection.execute(
            "UPDATE projects SET canonical_path = ?2, root_path = ?3, updated_at = ?4
             WHERE id = ?1",
            params![
                project_id,
                canonical.to_string_lossy(),
                canonical.to_string_lossy(),
                now_ms()
            ],
        )?;
        if changed == 0 {
            return Err(Error::WorkspaceProjectNotFound(project_id.into()));
        }
        self.project(&connection, project_id)
    }

    /// Removes membership only. The project record, its ID, and its directory
    /// are untouched.
    pub fn remove_project_from_workspace(
        &self,
        workspace_id: &str,
        project_id: &str,
    ) -> Result<()> {
        let connection = self.connect()?;
        let changed = connection.execute(
            "DELETE FROM workspace_projects WHERE workspace_id = ?1 AND project_id = ?2",
            params![workspace_id, project_id],
        )?;
        if changed == 0 {
            return Err(Error::WorkspaceProjectNotFound(project_id.into()));
        }
        Ok(())
    }

    /// Removes the project from the registry entirely. Membership rows go with
    /// it (FK cascade); nothing on disk is touched.
    pub fn delete_project(&self, project_id: &str) -> Result<()> {
        let connection = self.connect()?;
        let changed =
            connection.execute("DELETE FROM projects WHERE id = ?1", params![project_id])?;
        if changed == 0 {
            return Err(Error::WorkspaceProjectNotFound(project_id.into()));
        }
        Ok(())
    }

    pub fn project(&self, connection: &Connection, project_id: &str) -> Result<ProjectSummary> {
        connection
            .query_row(
                "SELECT p.id, p.root_path, p.name, p.created_at, p.updated_at, p.last_opened_at
                 FROM projects p WHERE p.id = ?1",
                params![project_id],
                project_from_row,
            )
            .optional()?
            .map(finalize_project)
            .ok_or_else(|| Error::WorkspaceProjectNotFound(project_id.into()))
    }

    /// All projects in the registry, with their workspace memberships.
    pub fn list_projects(&self) -> Result<Vec<ProjectSummary>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT p.id, p.root_path, p.name, p.created_at, p.updated_at, p.last_opened_at
             FROM projects p ORDER BY p.created_at",
        )?;
        let rows = statement.query_map([], project_from_row)?;
        let mut projects = rows
            .map(|row| row.map(finalize_project))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for project in &mut projects {
            let mut ids = connection.prepare(
                "SELECT workspace_id FROM workspace_projects WHERE project_id = ?1 ORDER BY added_at",
            )?;
            let mapped = ids.query_map(params![project.id], |row| row.get(0))?;
            project.workspace_ids = mapped.collect::<std::result::Result<Vec<_>, _>>()?;
        }
        Ok(projects)
    }

    /// Finds a project by canonicalized path (used for open-from-directory
    /// flows that must reuse an existing identity).
    pub fn find_project_by_path(&self, path: &Path) -> Result<Option<ProjectSummary>> {
        let canonical = match path.canonicalize() {
            Ok(canonical) => canonical,
            Err(_) => return Ok(None),
        };
        let connection = self.connect()?;
        let existing: Option<String> = connection
            .query_row(
                "SELECT id FROM projects WHERE canonical_path = ?1",
                params![canonical.to_string_lossy()],
                |row| row.get(0),
            )
            .optional()?;
        match existing {
            Some(id) => self.project(&connection, &id).map(Some),
            None => Ok(None),
        }
    }

    pub fn touch_project(&self, project_id: &str) -> Result<()> {
        let connection = self.connect()?;
        connection.execute(
            "UPDATE projects SET last_opened_at = ?2 WHERE id = ?1",
            params![project_id, now_ms()],
        )?;
        Ok(())
    }
}

fn project_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProjectSummary> {
    Ok(ProjectSummary {
        id: row.get(0)?,
        root_path: row.get(1)?,
        name: row.get(2)?,
        path_status: String::new(),
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
        last_opened_at: row.get(5)?,
        workspace_ids: Vec::new(),
    })
}

/// `path_status` is derived at read time, never stored: a stale cached status
/// would lie about the filesystem.
fn finalize_project(mut project: ProjectSummary) -> ProjectSummary {
    project.path_status = if Path::new(&project.root_path).is_dir() {
        "reachable".into()
    } else {
        "missing".into()
    };
    project
}

fn canonical_root(path: &Path) -> Result<PathBuf> {
    let canonical = path.canonicalize().map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if !canonical.is_dir() {
        return Err(Error::WorkspaceInvalid(format!(
            "{} 不是目录",
            canonical.to_string_lossy()
        )));
    }
    Ok(canonical)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn new_id() -> String {
    let mut bytes = [0u8; 8];
    if getrandom::fill(&mut bytes).is_err() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        bytes.copy_from_slice(&nanos.to_le_bytes());
    }
    let value = u64::from_le_bytes(bytes);
    let alphabet = b"0123456789abcdefghijklmnopqrstuvwxyz";
    // 5 bits per char keep the index within the 36-char alphabet (no modulo
    // bias); 12 chars × 5 = 60 bits fit in the u64.
    (0..12)
        .map(|i| alphabet[(value >> (i * 5)) as usize & 0b11111] as char)
        .collect()
}

/// Explicit, fail-loud migrations. `CREATE TABLE IF NOT EXISTS` alone cannot
/// evolve a schema; a database newer than this binary refuses to open instead
/// of silently degrading.
fn migrate(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS meta (
           key TEXT PRIMARY KEY,
           value TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS workspaces (
           id TEXT PRIMARY KEY,
           name TEXT NOT NULL,
           description TEXT NOT NULL DEFAULT '',
           created_at INTEGER NOT NULL,
           updated_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS projects (
           id TEXT PRIMARY KEY,
           canonical_path TEXT NOT NULL UNIQUE,
           root_path TEXT NOT NULL,
           name TEXT NOT NULL DEFAULT '',
           created_at INTEGER NOT NULL,
           updated_at INTEGER NOT NULL,
           last_opened_at INTEGER
         );
         CREATE TABLE IF NOT EXISTS workspace_projects (
           workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
           project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
           sort_order INTEGER NOT NULL DEFAULT 0,
           added_at INTEGER NOT NULL,
           PRIMARY KEY (workspace_id, project_id)
         );
         -- v2: project archive and environment (declared vs observed are
         -- separate columns so a scan can never overwrite a user declaration).
         CREATE TABLE IF NOT EXISTS project_profiles (
           project_id TEXT PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
           description TEXT NOT NULL DEFAULT '',
           project_type TEXT NOT NULL DEFAULT '',
           lifecycle_status TEXT NOT NULL DEFAULT '',
           tech_stack TEXT NOT NULL DEFAULT '[]',
           current_milestone TEXT NOT NULL DEFAULT '',
           progress_summary TEXT NOT NULL DEFAULT '',
           blockers TEXT NOT NULL DEFAULT '',
           next_action TEXT NOT NULL DEFAULT '',
           updated_at INTEGER NOT NULL DEFAULT 0
         );
         CREATE TABLE IF NOT EXISTS environment_profiles (
           project_id TEXT PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
           declared_json TEXT NOT NULL DEFAULT '{}',
           observed_json TEXT,
           scanned_at INTEGER,
           confirmed_at INTEGER,
           status TEXT NOT NULL DEFAULT 'unscanned'
         );
         -- v3: knowledge document index. Metadata only; the Markdown file is
         -- always the source of truth and deleted files become tombstones.
         CREATE TABLE IF NOT EXISTS knowledge_documents (
           id TEXT PRIMARY KEY,
           project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
           relative_path TEXT NOT NULL,
           logical_type TEXT NOT NULL DEFAULT 'general',
           title TEXT NOT NULL DEFAULT '',
           classification_source TEXT NOT NULL DEFAULT 'suggested',
           content_hash TEXT NOT NULL DEFAULT '',
           modified_at INTEGER NOT NULL DEFAULT 0,
           indexed_at INTEGER NOT NULL DEFAULT 0,
           tombstone INTEGER NOT NULL DEFAULT 0,
           UNIQUE (project_id, relative_path)
         );
         -- v4: full-text search over knowledge documents. Standalone FTS5
         -- table (doc_id UNINDEXED) rebuilt by every knowledge_scan; rows
         -- vanish from search when their document is tombstoned.
         CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_fts USING fts5(doc_id UNINDEXED, title, body);",
    )?;
    let version: Option<i64> = connection
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .and_then(|value| value.parse().ok());
    match version {
        None => {
            connection.execute(
                "INSERT INTO meta (key, value) VALUES ('schema_version', ?1)",
                params![SCHEMA_VERSION],
            )?;
        }
        Some(stored) if stored == SCHEMA_VERSION => {}
        Some(stored) if stored > SCHEMA_VERSION => {
            return Err(Error::WorkspaceInvalid(format!(
                "workspace.sqlite schema v{stored} 比当前应用（v{SCHEMA_VERSION}）更新；请升级应用"
            )));
        }
        // Older schemas migrate forward: this DDL is additive and idempotent,
        // so v1 tables keep every row while the new tables appear.
        Some(stored) => {
            connection.execute(
                "UPDATE meta SET value = ?2 WHERE key = 'schema_version'",
                params![stored, SCHEMA_VERSION],
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, WorkspaceStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = WorkspaceStore::new(dir.path().join("ws"));
        (dir, store)
    }

    #[test]
    fn default_workspace_is_created_once() {
        let (_dir, store) = store();
        let first = store.ensure_default_workspace().unwrap();
        let second = store.ensure_default_workspace().unwrap();
        assert_eq!(first, second);
        assert_eq!(store.list_workspaces().unwrap().len(), 1);
    }

    #[test]
    fn add_project_is_deduped_by_canonical_path() {
        let (_dir, store) = store();
        let workspace = store.create_workspace("Main", "").unwrap();
        let project_dir = tempfile::tempdir().unwrap();
        let added = store
            .add_project(&workspace.id, project_dir.path())
            .unwrap();
        assert!(added.created);
        let again = store
            .add_project(&workspace.id, project_dir.path())
            .unwrap();
        assert!(!again.created);
        assert_eq!(again.project.id, added.project.id);
        // One membership, not two.
        assert_eq!(store.list_projects().unwrap().len(), 1);
    }

    #[test]
    fn relink_keeps_identity_and_rejects_duplicate_paths() {
        let (_dir, store) = store();
        let workspace = store.create_workspace("Main", "").unwrap();
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let project = store
            .add_project(&workspace.id, first.path())
            .unwrap()
            .project;
        store.add_project(&workspace.id, second.path()).unwrap();

        // Relink to an occupied path is rejected.
        assert!(store.relink_project(&project.id, second.path()).is_err());

        let third = tempfile::tempdir().unwrap();
        let moved = store.relink_project(&project.id, third.path()).unwrap();
        assert_eq!(moved.id, project.id);
        assert_eq!(
            moved.root_path,
            third.path().canonicalize().unwrap().to_string_lossy()
        );
        assert_eq!(moved.path_status, "reachable");
        // Membership survives the move.
        let workspaces = store.list_workspaces().unwrap();
        assert_eq!(workspaces[0].projects.len(), 2);
        assert!(workspaces[0].projects.iter().any(|p| p.id == project.id));
    }

    #[test]
    fn missing_projects_show_missing_status() {
        let (_dir, store) = store();
        let workspace = store.create_workspace("Main", "").unwrap();
        let project_dir = tempfile::tempdir().unwrap();
        let project = store
            .add_project(&workspace.id, project_dir.path())
            .unwrap()
            .project;
        let project_path = project_dir.path().to_path_buf();
        drop(project_dir);
        let workspaces = store.list_workspaces().unwrap();
        let listed = workspaces[0]
            .projects
            .iter()
            .find(|p| p.id == project.id)
            .unwrap();
        assert!(!project_path.is_dir());
        assert_eq!(listed.path_status, "missing");
    }

    #[test]
    fn removing_membership_keeps_project_and_registry_delete_cascades() {
        let (_dir, store) = store();
        let workspace = store.create_workspace("Main", "").unwrap();
        let other = store.create_workspace("Other", "").unwrap();
        let project_dir = tempfile::tempdir().unwrap();
        let project = store
            .add_project(&workspace.id, project_dir.path())
            .unwrap()
            .project;
        store.add_project(&other.id, project_dir.path()).unwrap();

        store
            .remove_project_from_workspace(&workspace.id, &project.id)
            .unwrap();
        assert!(store
            .project(&store.connect().unwrap(), &project.id)
            .is_ok());
        let ids: Vec<_> = store
            .list_projects()
            .unwrap()
            .into_iter()
            .find(|p| p.id == project.id)
            .unwrap()
            .workspace_ids;
        assert_eq!(ids, vec![other.id.clone()]);

        store.delete_project(&project.id).unwrap();
        assert!(store
            .list_projects()
            .unwrap()
            .iter()
            .all(|p| p.id != project.id));
        assert_eq!(store.list_workspaces().unwrap().len(), 2);
    }

    #[test]
    fn deleting_workspace_keeps_projects() {
        let (_dir, store) = store();
        let workspace = store.create_workspace("Main", "").unwrap();
        let project_dir = tempfile::tempdir().unwrap();
        let project = store
            .add_project(&workspace.id, project_dir.path())
            .unwrap()
            .project;
        store.delete_workspace(&workspace.id).unwrap();
        assert!(store.list_workspaces().unwrap().is_empty());
        assert_eq!(store.list_projects().unwrap().len(), 1);
        assert_eq!(store.list_projects().unwrap()[0].id, project.id);
        assert!(store.list_projects().unwrap()[0].workspace_ids.is_empty());
    }

    #[test]
    fn future_schema_refuses_to_open() {
        let (_dir, store) = store();
        store.ensure_default_workspace().unwrap();
        let connection = Connection::open(store.root().join("workspace.sqlite")).unwrap();
        connection
            .execute(
                "UPDATE meta SET value = '99' WHERE key = 'schema_version'",
                [],
            )
            .unwrap();
        let err = store.list_workspaces().unwrap_err();
        assert!(err.to_string().contains("v99"), "{err}");
    }

    #[test]
    fn v1_database_migrates_forward_and_keeps_rows() {
        let (_dir, store) = store();
        let workspace = store.create_workspace("Main", "").unwrap();
        let project_dir = tempfile::tempdir().unwrap();
        let project = store
            .add_project(&workspace.id, project_dir.path())
            .unwrap()
            .project;
        // Simulate a database written by the v1 binary (no profile tables).
        {
            let connection = Connection::open(store.root().join("workspace.sqlite")).unwrap();
            connection
                .execute("DROP TABLE project_profiles", [])
                .unwrap();
            connection
                .execute("DROP TABLE environment_profiles", [])
                .unwrap();
            connection
                .execute(
                    "UPDATE meta SET value = '1' WHERE key = 'schema_version'",
                    [],
                )
                .unwrap();
        }
        // Reopening migrates to v2 without losing v1 rows.
        let workspaces = store.list_workspaces().unwrap();
        assert_eq!(workspaces.len(), 1);
        assert_eq!(workspaces[0].projects[0].id, project.id);
        let connection = Connection::open(store.root().join("workspace.sqlite")).unwrap();
        let version: String = connection
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION.to_string());
    }

    #[test]
    fn find_project_by_path_uses_existing_identity() {
        let (_dir, store) = store();
        let workspace = store.create_workspace("Main", "").unwrap();
        let project_dir = tempfile::tempdir().unwrap();
        let project = store
            .add_project(&workspace.id, project_dir.path())
            .unwrap()
            .project;
        let found = store
            .find_project_by_path(project_dir.path())
            .unwrap()
            .unwrap();
        assert_eq!(found.id, project.id);
        assert!(store
            .find_project_by_path(Path::new("/definitely/not/here"))
            .unwrap()
            .is_none());
    }
}
