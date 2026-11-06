//! Project profile and environment (Phase 2 of the workspace blueprint).
//!
//! Two boundaries matter here:
//! - **Declared vs observed.** The declared environment is user/agent-authored
//!   truth; the observed environment is a read-only scan. They live in
//!   separate columns, so a scan can never overwrite what the user confirmed.
//! - **Secrets.** The scanner reads an allowlist of config files only. `.env`
//!   values, key material, and credentials never enter the database, logs, or
//!   this module's output — environment variables are recorded by name alone,
//!   from `.env.example`-style templates.

use std::{collections::BTreeMap, fs, path::Path};

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{Error, Result, WorkspaceStore};

const MAX_SCAN_FILE_BYTES: u64 = 512 * 1024;

/// User-confirmed project archive. All fields optional except status.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectProfile {
    pub description: String,
    pub project_type: String,
    pub lifecycle_status: String,
    /// Tech-stack tags, e.g. `["rust", "tauri", "sqlite"]`.
    pub tech_stack: Vec<String>,
    pub current_milestone: String,
    pub progress_summary: String,
    pub blockers: String,
    pub next_action: String,
    pub updated_at: i64,
}

/// What the scanner found on disk. Plain data: serializes straight to the
/// `observed_json` column.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObservedEnvironment {
    pub package_manager: Option<String>,
    /// Runtimes/languages detected from config presence (`rust`, `node`, `python`, `go`).
    pub runtimes: Vec<String>,
    /// Runnable commands: name + the file that declares them.
    pub commands: Vec<ObservedCommand>,
    /// Service names from compose files.
    pub services: Vec<String>,
    /// Host ports mentioned by compose `ports:` entries.
    pub ports: Vec<String>,
    /// Environment variable NAMES from `.env.example`-style templates. Never values.
    pub env_var_names: Vec<String>,
    /// Files the scan actually read.
    pub source_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObservedCommand {
    pub name: String,
    pub source: String,
}

/// Persisted environment state for one project.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnvironmentProfile {
    /// User/agent-confirmed declaration (free-form JSON). Never touched by scans.
    pub declared_json: String,
    pub observed: Option<ObservedEnvironment>,
    pub scanned_at: Option<i64>,
    pub confirmed_at: Option<i64>,
    pub status: String,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn read_capped(path: &Path) -> Option<String> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_SCAN_FILE_BYTES {
        return None;
    }
    fs::read_to_string(path).ok()
}

/// Read-only environment scan of a project root. Failure of any single file is
/// not an error: the scan reports what it could see.
pub fn scan_environment(root: &Path) -> ObservedEnvironment {
    let mut observed = ObservedEnvironment::default();
    let seen = |name: &str| root.join(name);

    // package.json: scripts, packageManager, engines, dependency hints.
    if let Some(text) = read_capped(&seen("package.json")) {
        observed.source_files.push("package.json".into());
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(scripts) = value.get("scripts").and_then(|s| s.as_object()) {
                for key in scripts.keys() {
                    observed.commands.push(ObservedCommand {
                        name: key.clone(),
                        source: "package.json".into(),
                    });
                }
            }
            if let Some(manager) = value.get("packageManager").and_then(|m| m.as_str()) {
                observed.package_manager =
                    Some(manager.split('@').next().unwrap_or(manager).to_string());
            }
            let mut hints = Vec::new();
            for key in ["dependencies", "devDependencies"] {
                if let Some(deps) = value.get(key).and_then(|d| d.as_object()) {
                    hints.extend(deps.keys().take(50).cloned());
                }
            }
            for (dependency, runtime) in [
                ("typescript", "typescript"),
                ("vite", "vite"),
                ("react", "react"),
                ("electron", "electron"),
                ("tauri", "tauri"),
            ] {
                if hints.iter().any(|name| name == dependency)
                    && !observed.runtimes.contains(&runtime.to_string())
                {
                    observed.runtimes.push(runtime.into());
                }
            }
        }
    }

    // Lockfiles pin the package manager even without a package.json field.
    if observed.package_manager.is_none() {
        for (lock, manager) in [
            ("pnpm-lock.yaml", "pnpm"),
            ("package-lock.json", "npm"),
            ("yarn.lock", "yarn"),
            ("bun.lockb", "bun"),
        ] {
            if seen(lock).exists() {
                observed.package_manager = Some(manager.into());
                observed.source_files.push(lock.into());
                break;
            }
        }
    }
    if seen("package.json").exists() && !observed.runtimes.iter().any(|r| r == "node") {
        observed.runtimes.push("node".into());
    }

    // Rust: presence, not parsing. The crate list would need a toml parser.
    if seen("Cargo.toml").exists() {
        observed.runtimes.push("rust".into());
        observed.source_files.push("Cargo.toml".into());
    }
    if seen("Cargo.lock").exists() && observed.package_manager.is_none() {
        observed.package_manager = Some("cargo".into());
    }

    // Python / Go: presence only.
    if seen("pyproject.toml").exists() || seen("requirements.txt").exists() {
        observed.runtimes.push("python".into());
        observed.source_files.push("pyproject.toml".into());
    }
    if seen("go.mod").exists() {
        observed.runtimes.push("go".into());
        observed.source_files.push("go.mod".into());
    }

    // Makefile targets: top-level `name:` lines.
    if let Some(text) = read_capped(&seen("Makefile")) {
        observed.source_files.push("Makefile".into());
        for line in text.lines() {
            if line.starts_with('\t') || line.starts_with('#') || line.contains('=') {
                continue;
            }
            if let Some((name, rest)) = line.split_once(':') {
                let name = name.trim();
                if name.is_empty()
                    || name.contains(' ')
                    || name.starts_with('.')
                    || rest.trim().starts_with('=')
                {
                    continue;
                }
                observed.commands.push(ObservedCommand {
                    name: name.to_string(),
                    source: "Makefile".into(),
                });
            }
        }
    }

    // Compose files: service names and host ports. YAML is read as text with
    // serde_yaml for structure; port strings stay verbatim.
    for compose in [
        "docker-compose.yml",
        "docker-compose.yaml",
        "compose.yml",
        "compose.yaml",
    ] {
        let path = seen(compose);
        if !path.exists() {
            continue;
        }
        observed.source_files.push(compose.into());
        if let Some(text) = read_capped(&path) {
            if let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(&text) {
                if let Some(services) = value.get("services").and_then(|s| s.as_mapping()) {
                    for key in services.keys() {
                        if let Some(name) = key.as_str() {
                            observed.services.push(name.to_string());
                        }
                    }
                }
            }
            let mut in_ports = false;
            for line in text.lines() {
                let trimmed = line.trim_start();
                if line.starts_with("  ") && trimmed.starts_with("ports:") {
                    in_ports = true;
                    continue;
                }
                if in_ports {
                    if trimmed.starts_with("- ") {
                        let entry = trimmed
                            .trim_start_matches("- ")
                            .trim()
                            .trim_matches('"')
                            .to_string();
                        observed.ports.push(entry);
                    } else if !line.starts_with(' ')
                        || (trimmed.contains(':') && !trimmed.starts_with('-'))
                    {
                        in_ports = false;
                    }
                }
            }
        }
    }

    // Environment variable NAMES only, from example templates. `.env` itself
    // is deliberately never read.
    for template in [".env.example", ".env.sample", ".env.template"] {
        let path = seen(template);
        if !path.exists() {
            continue;
        }
        observed.source_files.push(template.into());
        if let Some(text) = read_capped(&path) {
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((name, _)) = line.split_once('=') {
                    let name = name.trim();
                    if !name.is_empty() && !observed.env_var_names.contains(&name.to_string()) {
                        observed.env_var_names.push(name.to_string());
                    }
                }
            }
        }
    }

    observed
}

impl WorkspaceStore {
    pub fn project_profile(&self, project_id: &str) -> Result<ProjectProfile> {
        let connection = self.connect()?;
        self.project(&connection, project_id)?;
        let (
            description,
            project_type,
            lifecycle_status,
            tech_stack,
            current_milestone,
            progress_summary,
            blockers,
            next_action,
            updated_at,
        ) = connection
            .query_row(
                "SELECT description, project_type, lifecycle_status, tech_stack,
                        current_milestone, progress_summary, blockers, next_action, updated_at
                 FROM project_profiles WHERE project_id = ?1",
                params![project_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get::<_, String>(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                        row.get(8)?,
                    ))
                },
            )
            .optional()?
            .unwrap_or_default();
        Ok(ProjectProfile {
            description,
            project_type,
            lifecycle_status,
            tech_stack: serde_json::from_str(&tech_stack).unwrap_or_default(),
            current_milestone,
            progress_summary,
            blockers,
            next_action,
            updated_at,
        })
    }

    /// Saves the user-confirmed profile. The project must exist in the
    /// registry; scanning and confirming are separate concerns.
    pub fn set_project_profile(
        &self,
        project_id: &str,
        profile: &ProjectProfile,
    ) -> Result<ProjectProfile> {
        let mut confirmed = profile.clone();
        confirmed.updated_at = now_ms();
        confirmed.lifecycle_status = if confirmed.lifecycle_status.trim().is_empty() {
            "active".into()
        } else {
            confirmed.lifecycle_status.trim().into()
        };
        let tech_stack = serde_json::to_string(&confirmed.tech_stack)
            .map_err(|error| Error::WorkspaceInvalid(error.to_string()))?;
        let connection = self.connect()?;
        self.project(&connection, project_id)?;
        connection.execute(
            "INSERT INTO project_profiles (project_id, description, project_type, lifecycle_status, tech_stack,
                                           current_milestone, progress_summary, blockers, next_action, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(project_id) DO UPDATE SET
               description = excluded.description,
               project_type = excluded.project_type,
               lifecycle_status = excluded.lifecycle_status,
               tech_stack = excluded.tech_stack,
               current_milestone = excluded.current_milestone,
               progress_summary = excluded.progress_summary,
               blockers = excluded.blockers,
               next_action = excluded.next_action,
               updated_at = excluded.updated_at",
            params![
                project_id,
                confirmed.description,
                confirmed.project_type,
                confirmed.lifecycle_status,
                tech_stack,
                confirmed.current_milestone,
                confirmed.progress_summary,
                confirmed.blockers,
                confirmed.next_action,
                confirmed.updated_at,
            ],
        )?;
        Ok(confirmed)
    }

    pub fn environment_profile(&self, project_id: &str) -> Result<EnvironmentProfile> {
        let connection = self.connect()?;
        self.project(&connection, project_id)?;
        let (declared_json, observed_json, scanned_at, confirmed_at, status) = connection
            .query_row(
                "SELECT declared_json, observed_json, scanned_at, confirmed_at, status
                 FROM environment_profiles WHERE project_id = ?1",
                params![project_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<i64>>(2)?,
                        row.get::<_, Option<i64>>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()?
            .unwrap_or_else(|| ("{}".into(), None, None, None, "unscanned".into()));
        Ok(EnvironmentProfile {
            declared_json,
            observed: observed_json.and_then(|json| serde_json::from_str(&json).ok()),
            scanned_at,
            confirmed_at,
            status,
        })
    }

    /// Records a fresh scan. Writes only the observed column: the declared
    /// JSON is structurally untouched, which is the "scan never overwrites
    /// user declarations" gate.
    pub fn record_environment_scan(
        &self,
        project_id: &str,
        observed: &ObservedEnvironment,
    ) -> Result<EnvironmentProfile> {
        let json = serde_json::to_string(observed)
            .map_err(|error| Error::WorkspaceInvalid(error.to_string()))?;
        let connection = self.connect()?;
        self.project(&connection, project_id)?;
        connection.execute(
            "INSERT INTO environment_profiles (project_id, declared_json, observed_json, scanned_at, status)
             VALUES (?1, '{}', ?2, ?3, 'scanned')
             ON CONFLICT(project_id) DO UPDATE SET
               observed_json = excluded.observed_json,
               scanned_at = excluded.scanned_at,
               status = 'scanned'",
            params![project_id, json, now_ms()],
        )?;
        self.environment_profile(project_id)
    }

    /// Saves the user-confirmed declaration and marks the record confirmed.
    pub fn set_declared_environment(
        &self,
        project_id: &str,
        declared_json: &str,
    ) -> Result<EnvironmentProfile> {
        if serde_json::from_str::<serde_json::Value>(declared_json).is_err() {
            return Err(Error::WorkspaceInvalid("声明环境必须是合法 JSON".into()));
        }
        let connection = self.connect()?;
        self.project(&connection, project_id)?;
        connection.execute(
            "INSERT INTO environment_profiles (project_id, declared_json, confirmed_at, status)
             VALUES (?1, ?2, ?3, 'confirmed')
             ON CONFLICT(project_id) DO UPDATE SET
               declared_json = excluded.declared_json,
               confirmed_at = excluded.confirmed_at,
               status = 'confirmed'",
            params![project_id, declared_json, now_ms()],
        )?;
        self.environment_profile(project_id)
    }
}

/// `BTreeMap` keeps imports honest for tests that compare scan output.
pub type ScanSummary = BTreeMap<String, Vec<String>>;

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, WorkspaceStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = WorkspaceStore::new(dir.path().join("ws"));
        (dir, store)
    }

    fn fixture(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (name, content) in files {
            fs::write(dir.path().join(name), content).unwrap();
        }
        dir
    }

    #[test]
    fn scan_reads_scripts_lockfiles_and_compose() {
        let dir = fixture(&[
            ("package.json", r#"{"name":"app","scripts":{"dev":"vite","build":"vite build"},"packageManager":"pnpm@9","dependencies":{"react":"^18"}}"#),
            ("docker-compose.yml", "services:\n  web:\n    image: nginx\n    ports:\n      - \"8080:80\"\n  db:\n    image: postgres\n"),
            ("Makefile", ".PHONY: all\nall: build\n\ndev:\n\techo hi\n"),
        ]);
        let observed = scan_environment(dir.path());
        assert_eq!(observed.package_manager.as_deref(), Some("pnpm"));
        assert!(observed
            .commands
            .iter()
            .any(|c| c.name == "dev" && c.source == "package.json"));
        assert!(observed
            .commands
            .iter()
            .any(|c| c.name == "all" && c.source == "Makefile"));
        assert!(!observed.commands.iter().any(|c| c.name == ".PHONY"));
        assert_eq!(observed.services, vec!["web", "db"]);
        assert_eq!(observed.ports, vec!["8080:80"]);
        assert!(observed.runtimes.contains(&"node".to_string()));
        assert!(observed.runtimes.contains(&"react".to_string()));
    }

    #[test]
    fn scan_records_env_names_but_never_values_or_dotenv() {
        let dir = fixture(&[
            (".env", "SECRET_TOKEN=supersecret\n"),
            (
                ".env.example",
                "DATABASE_URL=\n# comment\nAPI_KEY=placeholder\n",
            ),
        ]);
        let observed = scan_environment(dir.path());
        assert_eq!(observed.env_var_names, vec!["DATABASE_URL", "API_KEY"]);
        assert!(!observed.source_files.iter().any(|f| f == ".env"));
        let serialized = serde_json::to_string(&observed).unwrap();
        assert!(
            !serialized.contains("supersecret"),
            "secret value leaked: {serialized}"
        );
    }

    #[test]
    fn scan_detects_rust_go_python_presence() {
        let dir = fixture(&[
            ("Cargo.toml", "[package]\nname = \"x\"\n"),
            ("go.mod", "module x\n"),
        ]);
        let observed = scan_environment(dir.path());
        assert!(observed.runtimes.contains(&"rust".to_string()));
        assert!(observed.runtimes.contains(&"go".to_string()));
        assert_eq!(observed.package_manager.as_deref(), None);
    }

    #[test]
    fn scan_of_missing_root_reports_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let observed = scan_environment(dir.path());
        assert_eq!(observed, ObservedEnvironment::default());
    }

    #[test]
    fn profile_round_trips_with_defaults() {
        let (_dir, store) = store();
        let workspace = store.create_workspace("Main", "").unwrap();
        let project_dir = tempfile::tempdir().unwrap();
        let project = store
            .add_project(&workspace.id, project_dir.path())
            .unwrap()
            .project;

        let empty = store.project_profile(&project.id).unwrap();
        assert_eq!(empty.lifecycle_status, "");
        assert!(empty.tech_stack.is_empty());

        let profile = ProjectProfile {
            description: "本地任务对账器".into(),
            project_type: "desktop".into(),
            lifecycle_status: "active".into(),
            tech_stack: vec!["rust".into(), "tauri".into()],
            next_action: "Phase 2 收尾".into(),
            ..Default::default()
        };
        let saved = store.set_project_profile(&project.id, &profile).unwrap();
        assert!(saved.updated_at > 0);
        let loaded = store.project_profile(&project.id).unwrap();
        assert_eq!(loaded, saved);
        assert_eq!(
            loaded.tech_stack,
            vec!["rust".to_string(), "tauri".to_string()]
        );
    }

    #[test]
    fn profile_requires_registered_project() {
        let (_dir, store) = store();
        assert!(store.project_profile("nope").is_err());
    }

    #[test]
    fn scan_never_overwrites_declared_environment() {
        let (_dir, store) = store();
        let workspace = store.create_workspace("Main", "").unwrap();
        let project_dir = tempfile::tempdir().unwrap();
        let project = store
            .add_project(&workspace.id, project_dir.path())
            .unwrap()
            .project;

        store
            .set_declared_environment(&project.id, r#"{"node":"22","ports":[3000]}"#)
            .unwrap();
        let project_dir2 = fixture(&[("package.json", r#"{"scripts":{"dev":"vite"}}"#)]);
        let observed = scan_environment(project_dir2.path());
        let after_scan = store
            .record_environment_scan(&project.id, &observed)
            .unwrap();
        assert_eq!(after_scan.declared_json, r#"{"node":"22","ports":[3000]}"#);
        assert_eq!(after_scan.status, "scanned");
        assert!(after_scan
            .observed
            .as_ref()
            .unwrap()
            .commands
            .iter()
            .any(|c| c.name == "dev"));
    }

    #[test]
    fn declared_environment_must_be_json() {
        let (_dir, store) = store();
        let workspace = store.create_workspace("Main", "").unwrap();
        let project_dir = tempfile::tempdir().unwrap();
        let project = store
            .add_project(&workspace.id, project_dir.path())
            .unwrap()
            .project;
        assert!(store
            .set_declared_environment(&project.id, "not json")
            .is_err());
        let confirmed = store.set_declared_environment(&project.id, "{}").unwrap();
        assert_eq!(confirmed.status, "confirmed");
        assert!(confirmed.confirmed_at.is_some());
    }

    #[test]
    fn environment_defaults_to_unscanned() {
        let (_dir, store) = store();
        let workspace = store.create_workspace("Main", "").unwrap();
        let project_dir = tempfile::tempdir().unwrap();
        let project = store
            .add_project(&workspace.id, project_dir.path())
            .unwrap()
            .project;
        let environment = store.environment_profile(&project.id).unwrap();
        assert_eq!(environment.status, "unscanned");
        assert!(environment.observed.is_none());
        assert_eq!(environment.declared_json, "{}");
    }

    #[test]
    fn deleting_project_cascades_profile_and_environment() {
        let (_dir, store) = store();
        let workspace = store.create_workspace("Main", "").unwrap();
        let project_dir = tempfile::tempdir().unwrap();
        let project = store
            .add_project(&workspace.id, project_dir.path())
            .unwrap()
            .project;
        store
            .set_project_profile(&project.id, &ProjectProfile::default())
            .unwrap();
        store
            .record_environment_scan(&project.id, &ObservedEnvironment::default())
            .unwrap();
        store.delete_project(&project.id).unwrap();
        // Recreating a project at the same path starts with a clean archive.
        let recreated = store
            .add_project(&workspace.id, project_dir.path())
            .unwrap()
            .project;
        assert_ne!(recreated.id, project.id);
        assert_eq!(
            store.project_profile(&recreated.id).unwrap(),
            ProjectProfile::default()
        );
        assert_eq!(
            store.environment_profile(&recreated.id).unwrap().status,
            "unscanned"
        );
    }
}
