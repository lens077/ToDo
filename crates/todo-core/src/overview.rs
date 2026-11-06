//! Cross-project overview (Phase 7): one read-only aggregate that answers
//! "what is each project's status and next step" without opening any of them.
//!
//! Everything is derived from sources that already exist — the workspace
//! registry, project profiles, `.virgo/sessions/` files, and the knowledge
//! index. Nothing here mutates; anything missing degrades to an explicit
//! placeholder so the dashboard can distinguish "none" from "not recorded".

use serde::{Deserialize, Serialize};

use crate::{session_list, KnowledgeDocument, ProjectSummary, Result, WorkspaceStore};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectOverview {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub path_status: String,
    pub lifecycle_status: String,
    pub current_milestone: String,
    pub next_action: String,
    pub blockers: String,
    /// Latest session status (`active`/`completed`/`interrupted`), if any.
    pub session_status: Option<String>,
    pub session_goal: Option<String>,
    /// Outstanding work from the latest session (active → goal, finished → next actions).
    pub outstanding: Vec<String>,
    pub knowledge_documents: u32,
    /// Documents still on the suggested classification; the dashboard hints
    /// at confirming them.
    pub unclassified_documents: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OverviewReport {
    pub projects: Vec<ProjectOverview>,
    pub total_projects: usize,
    pub missing_projects: usize,
    pub active_sessions: usize,
    pub unclassified_documents: u32,
}

impl WorkspaceStore {
    /// Read-only cross-project aggregate. Per-project session reads touch the
    /// filesystem; a missing or unreadable `.virgo/sessions/` simply means
    /// "no session recorded".
    pub fn overview(&self) -> Result<OverviewReport> {
        let projects = self.list_projects()?;
        let mut overviews = Vec::with_capacity(projects.len());
        let mut missing = 0usize;
        let mut active_sessions = 0usize;
        let mut unclassified_total: u32 = 0;
        for project in &projects {
            overviews.push(self.project_overview(project)?);
            if project.path_status != "reachable" {
                missing += 1;
            }
        }
        for overview in &overviews {
            if overview.session_status == Some("active".into()) {
                active_sessions += 1;
            }
            unclassified_total += overview.unclassified_documents;
        }
        Ok(OverviewReport {
            total_projects: overviews.len(),
            missing_projects: missing,
            active_sessions,
            unclassified_documents: unclassified_total,
            projects: overviews,
        })
    }

    fn project_overview(&self, project: &ProjectSummary) -> Result<ProjectOverview> {
        let profile = self.project_profile(&project.id)?;
        let sessions = session_list(std::path::Path::new(&project.root_path)).unwrap_or_default();
        let latest = sessions.first();
        let (session_status, session_goal, outstanding) = match latest {
            Some(session) if session.status == "active" => (
                Some(session.status.clone()),
                Some(session.goal.clone()),
                vec![session.goal.clone()],
            ),
            Some(session) => (
                Some(session.status.clone()),
                Some(session.goal.clone()),
                session.next_actions.clone(),
            ),
            None => (None, None, Vec::new()),
        };
        let knowledge = self.knowledge_list(&project.id)?;
        let unclassified = knowledge
            .iter()
            .filter(|document: &&KnowledgeDocument| {
                !document.tombstone
                    && document.classification_source != "confirmed"
                    && document.logical_type == "general"
            })
            .count() as u32;
        Ok(ProjectOverview {
            id: project.id.clone(),
            name: project.name.clone(),
            root_path: project.root_path.clone(),
            path_status: project.path_status.clone(),
            lifecycle_status: if profile.lifecycle_status.is_empty() {
                "active".into()
            } else {
                profile.lifecycle_status
            },
            current_milestone: profile.current_milestone,
            next_action: profile.next_action,
            blockers: profile.blockers,
            session_status,
            session_goal,
            outstanding,
            knowledge_documents: knowledge
                .iter()
                .filter(|document| !document.tombstone)
                .count() as u32,
            unclassified_documents: unclassified,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::{session_begin, WorkspaceStore};

    fn store() -> (tempfile::TempDir, WorkspaceStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = WorkspaceStore::new(dir.path().join("ws"));
        (dir, store)
    }

    fn project_at(store: &WorkspaceStore, name: &str) -> (tempfile::TempDir, String) {
        let workspace = store.create_workspace(name, "").unwrap();
        let dir = tempfile::tempdir().unwrap();
        let project = store
            .add_project(&workspace.id, dir.path())
            .unwrap()
            .project;
        (dir, project.id)
    }

    #[test]
    fn empty_registry_reports_zeroes() {
        let (_d, store) = store();
        let report = store.overview().unwrap();
        assert_eq!(report.total_projects, 0);
        assert_eq!(report.missing_projects, 0);
        assert_eq!(report.active_sessions, 0);
    }

    #[test]
    fn overview_answers_status_and_next_step_per_project() {
        let (_d, store) = store();
        let (dir, project) = project_at(&store, "Main");
        store
            .set_project_profile(
                &project,
                &crate::ProjectProfile {
                    lifecycle_status: "active".into(),
                    current_milestone: "v1.2 发布".into(),
                    next_action: "接入 watcher 增量".into(),
                    blockers: "等待设计稿".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        session_begin(dir.path(), "补 Overview").unwrap();

        let report = store.overview().unwrap();
        assert_eq!(report.total_projects, 1);
        let overview = &report.projects[0];
        assert_eq!(overview.lifecycle_status, "active");
        assert_eq!(overview.current_milestone, "v1.2 发布");
        assert_eq!(overview.next_action, "接入 watcher 增量");
        assert_eq!(overview.blockers, "等待设计稿");
        assert_eq!(overview.session_status.as_deref(), Some("active"));
        // Active session: the outstanding work is its goal.
        assert_eq!(overview.outstanding, vec!["补 Overview".to_string()]);
        assert_eq!(report.active_sessions, 1);
    }

    #[test]
    fn finished_session_outstanding_is_next_actions() {
        let (_d, store) = store();
        let (dir, project) = project_at(&store, "Main");
        let _ = project;
        session_begin(dir.path(), "阶段收尾").unwrap();
        crate::session_finish(
            dir.path(),
            &crate::SessionFinishInput {
                next_actions: vec!["接 Phase 7".into()],
                ..Default::default()
            },
        )
        .unwrap();
        let report = store.overview().unwrap();
        let overview = &report.projects[0];
        assert_eq!(overview.session_status.as_deref(), Some("completed"));
        assert_eq!(overview.outstanding, vec!["接 Phase 7".to_string()]);
        assert_eq!(report.active_sessions, 0);
    }

    #[test]
    fn missing_path_and_unclassified_documents_are_counted() {
        let (_d, store) = store();
        let (dir, project) = project_at(&store, "Main");
        // README→brief、ADR→decision 均确认；notes.md 保持 general 未确认 → 1 unclassified。
        std::fs::write(dir.path().join("README.md"), "# 简介\n").unwrap();
        std::fs::create_dir_all(dir.path().join("docs/adr")).unwrap();
        std::fs::write(dir.path().join("docs/adr/0001-x.md"), "# 决策\n").unwrap();
        std::fs::write(dir.path().join("docs/notes.md"), "# 随手记\n").unwrap();
        store.knowledge_scan(&project).unwrap();
        let documents = store.knowledge_list(&project).unwrap();
        for document in documents
            .iter()
            .filter(|document| document.logical_type != "general")
        {
            store
                .knowledge_confirm(&document.id, document.logical_type.as_str())
                .unwrap();
        }

        // Missing path: drop the project directory.
        let project_path = dir.path().to_path_buf();
        drop(dir);
        let report = store.overview().unwrap();
        let overview = &report.projects[0];
        assert_eq!(overview.path_status, "missing");
        assert!(!project_path.is_dir());
        assert_eq!(report.missing_projects, 1);
        assert_eq!(overview.unclassified_documents, 1);
        assert_eq!(overview.knowledge_documents, 3);
        assert_eq!(report.unclassified_documents, 1);
    }
}
