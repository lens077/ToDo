//! Project knowledge: discover existing Markdown, suggest a logical type,
//! let the user confirm the mapping, and render a safe reading view.
//!
//! Boundaries (blueprint §4): existing documents are indexed in place — never
//! moved, rewritten, or renamed. The index stores metadata + content hash so
//! unchanged files skip re-parsing and deleted files keep a tombstone. The
//! reading view is rebuilt from the file on every read, so the HTML is always
//! a disposable projection of the Markdown source.

use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::Path,
};

use pulldown_cmark::{html, Options, Parser};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{reading::sanitize_html, Error, Result, WorkspaceStore};

const MAX_SCAN_FILES: usize = 2000;
const MAX_SCAN_FILE_BYTES: u64 = 512 * 1024;
/// Directories that are never knowledge: build output, VCS internals, and
/// app-private state.
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".todo",
    ".virgo",
    ".venv",
];

/// Logical document types (blueprint §4.2).
pub const LOGICAL_TYPES: &[&str] = &[
    "general",
    "brief",
    "tasks",
    "progress",
    "architecture",
    "environment",
    "runbook",
    "agent_instructions",
    "decision",
    "implementation",
    "reference",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KnowledgeDocument {
    pub id: String,
    pub project_id: String,
    /// Path relative to the project root, `/`-separated.
    pub relative_path: String,
    pub logical_type: String,
    pub title: String,
    /// `suggested` (rule-based) or `confirmed` (user accepted/overrode).
    pub classification_source: String,
    pub modified_at: i64,
    pub indexed_at: i64,
    /// Deleted files keep their index row (and history references) behind a
    /// tombstone instead of vanishing.
    pub tombstone: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KnowledgeScanReport {
    pub scanned: usize,
    pub indexed: usize,
    pub unchanged: usize,
    pub tombstoned: usize,
    pub documents: Vec<KnowledgeDocument>,
}

/// One full-text search hit: the document plus a highlighted body snippet.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KnowledgeSearchHit {
    pub document: KnowledgeDocument,
    /// Body fragment around the match; terms wrapped in 【】.
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeDocumentContent {
    #[serde(flatten)]
    pub document: KnowledgeDocument,
    /// Sanitized HTML rendered fresh from the file. Never persisted.
    pub content_html: String,
}

impl WorkspaceStore {
    /// Walks the project root and refreshes the index. Files whose content
    /// hash is unchanged keep their row untouched (classification and history
    /// included); new files are suggested a type; missing files are tombstoned.
    pub fn knowledge_scan(&self, project_id: &str) -> Result<KnowledgeScanReport> {
        let connection = self.connect()?;
        self.project(&connection, project_id)?;
        let root = self.project_root(&connection, project_id)?;
        let found = walk_markdown(&root)?;
        let known_hashes: std::collections::HashSet<String> = {
            let mut statement = connection
                .prepare("SELECT content_hash FROM knowledge_documents WHERE project_id = ?1")?;
            let rows = statement.query_map(params![project_id], |row| row.get::<_, String>(0))?;
            rows.flatten().collect()
        };

        let now = now_ms();
        let mut indexed = 0usize;
        let mut unchanged = 0usize;
        for file in &found {
            let hash = content_hash(&file.content);
            if known_hashes.contains(&hash) {
                unchanged += 1;
            } else {
                indexed += 1;
            }
            // Same relative path always maps to the same row; the content hash
            // only decides whether we re-parse title and classification.
            let id = {
                let mut hasher = DefaultHasher::new();
                project_id.hash(&mut hasher);
                file.relative_path.hash(&mut hasher);
                format!("kdoc{:016x}", hasher.finish())
            };
            let suggested = suggest_type(&file.relative_path, &file.content);
            connection.execute(
                "INSERT INTO knowledge_documents (id, project_id, relative_path, logical_type, title,
                                                 classification_source, content_hash, modified_at, indexed_at, tombstone)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'suggested', ?6, ?7, ?8, 0)
                 ON CONFLICT(project_id, relative_path) DO UPDATE SET
                   content_hash = excluded.content_hash,
                   modified_at = excluded.modified_at,
                   indexed_at = excluded.indexed_at,
                   tombstone = 0,
                   logical_type = CASE WHEN knowledge_documents.classification_source = 'confirmed'
                                       THEN knowledge_documents.logical_type ELSE excluded.logical_type END,
                   title = CASE WHEN knowledge_documents.classification_source = 'confirmed'
                                THEN knowledge_documents.title ELSE excluded.title END,
                   classification_source = knowledge_documents.classification_source",
                params![id, project_id, file.relative_path, suggested.0, suggested.1, hash, file.modified_at, now],
            )?;
            // Keep the full-text index in step with the row. The stored title
            // wins (a confirmed classification may have renamed it); the body
            // is the file content read moments ago. Rescans re-read every
            // file anyway, so re-indexing is the cheap and always-correct path.
            let stored_title: String = connection.query_row(
                "SELECT title FROM knowledge_documents WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )?;
            connection.execute(
                "DELETE FROM knowledge_fts WHERE doc_id = ?1",
                params![id],
            )?;
            connection.execute(
                "INSERT INTO knowledge_fts (doc_id, title, body) VALUES (?1, ?2, ?3)",
                params![id, cjk_spaced(&stored_title), cjk_spaced(&file.content)],
            )?;
        }
        // Tombstone rows whose path no longer exists on disk.
        let live_paths: Vec<String> = found.iter().map(|f| f.relative_path.clone()).collect();
        let mut tombstoned = 0usize;
        {
            let mut statement = connection.prepare(
                "SELECT relative_path FROM knowledge_documents
                 WHERE project_id = ?1 AND tombstone = 0",
            )?;
            let rows = statement.query_map(params![project_id], |row| row.get::<_, String>(0))?;
            for path in rows.flatten() {
                if !live_paths.contains(&path) {
                    connection.execute(
                        "UPDATE knowledge_documents SET tombstone = 1 WHERE project_id = ?1 AND relative_path = ?2",
                        params![project_id, path],
                    )?;
                    connection.execute(
                        "DELETE FROM knowledge_fts WHERE doc_id IN
                           (SELECT id FROM knowledge_documents WHERE project_id = ?1 AND relative_path = ?2)",
                        params![project_id, path],
                    )?;
                    tombstoned += 1;
                }
            }
        }
        let documents = self.knowledge_list(project_id)?;
        Ok(KnowledgeScanReport {
            scanned: found.len(),
            indexed,
            unchanged,
            tombstoned,
            documents,
        })
    }

    pub fn knowledge_list(&self, project_id: &str) -> Result<Vec<KnowledgeDocument>> {
        let connection = self.connect()?;
        self.project(&connection, project_id)?;
        let mut statement = connection.prepare(
            "SELECT id, project_id, relative_path, logical_type, title, classification_source,
                    modified_at, indexed_at, tombstone
             FROM knowledge_documents WHERE project_id = ?1
             ORDER BY tombstone, relative_path",
        )?;
        let rows = statement.query_map(params![project_id], document_from_row)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Error::from)
    }

    /// User confirms (or overrides) a document's logical type. Confirmations
    /// survive re-scans even when content changes.
    pub fn knowledge_confirm(
        &self,
        document_id: &str,
        logical_type: &str,
    ) -> Result<KnowledgeDocument> {
        if !LOGICAL_TYPES.contains(&logical_type) {
            return Err(Error::WorkspaceInvalid(format!(
                "未知文档类型: {logical_type}"
            )));
        }
        let connection = self.connect()?;
        let changed = connection.execute(
            "UPDATE knowledge_documents
             SET logical_type = ?2, classification_source = 'confirmed', indexed_at = ?3
             WHERE id = ?1",
            params![document_id, logical_type, now_ms()],
        )?;
        if changed == 0 {
            return Err(Error::WorkspaceInvalid(format!(
                "文档不存在: {document_id}"
            )));
        }
        connection
            .query_row(
                "SELECT id, project_id, relative_path, logical_type, title, classification_source,
                        modified_at, indexed_at, tombstone
                 FROM knowledge_documents WHERE id = ?1",
                params![document_id],
                document_from_row,
            )
            .map_err(Error::from)
    }

    /// Renders the document from its file on disk. The file is the source of
    /// truth: nothing cached can drift from what the user edits externally.
    pub fn knowledge_read(&self, document_id: &str) -> Result<KnowledgeDocumentContent> {
        let connection = self.connect()?;
        let (project_id, relative_path) = connection
            .query_row(
                "SELECT project_id, relative_path FROM knowledge_documents WHERE id = ?1",
                params![document_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?
            .ok_or_else(|| Error::WorkspaceInvalid(format!("文档不存在: {document_id}")))?;
        let root = self.project_root(&connection, &project_id)?;
        let path = root.join(&relative_path);
        let content = fs::read_to_string(&path).map_err(|source| Error::Io { path, source })?;
        let document = connection
            .query_row(
                "SELECT id, project_id, relative_path, logical_type, title, classification_source,
                        modified_at, indexed_at, tombstone
                 FROM knowledge_documents WHERE id = ?1",
                params![document_id],
                document_from_row,
            )
            .map_err(Error::from)?;
        Ok(KnowledgeDocumentContent {
            document,
            content_html: render_markdown(&content),
        })
    }

    /// Full-text search over a project's knowledge documents (M4). User input
    /// is quoted term by term so it can never inject FTS5 syntax; hits are
    /// ranked by FTS5 relevance with a highlighted body snippet. Deleted
    /// (tombstoned) documents are excluded, and an index that has not been
    /// refreshed by a scan simply returns fewer hits — never stale rows.
    pub fn knowledge_search(&self, project_id: &str, query: &str) -> Result<Vec<KnowledgeSearchHit>> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let connection = self.connect()?;
        self.project(&connection, project_id)?;
        let terms: Vec<String> = query
            .split_whitespace()
            .map(|term| format!("\"{}\"", cjk_spaced(term).replace('"', "\"\"")))
            .collect();
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let mut statement = connection.prepare(
            "SELECT d.id, d.project_id, d.relative_path, d.logical_type, d.title, d.classification_source,
                    d.modified_at, d.indexed_at, d.tombstone,
                    snippet(knowledge_fts, 2, '【', '】', '…', 12)
             FROM knowledge_fts
             JOIN knowledge_documents d ON d.id = knowledge_fts.doc_id
             WHERE knowledge_fts MATCH ?1 AND d.project_id = ?2 AND d.tombstone = 0
             ORDER BY rank
             LIMIT 20",
        )?;
        let rows = statement.query_map(params![terms.join(" "), project_id], |row| {
            Ok(KnowledgeSearchHit {
                document: document_from_row(row)?,
                snippet: cjk_despace(&row.get::<_, String>(9)?),
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Error::from)
    }

    fn project_root(
        &self,
        connection: &rusqlite::Connection,
        project_id: &str,
    ) -> Result<std::path::PathBuf> {
        Ok(std::path::PathBuf::from(
            self.project(connection, project_id)?.root_path,
        ))
    }
}

struct MarkdownFile {
    relative_path: String,
    content: String,
    modified_at: i64,
}

fn walk_markdown(root: &Path) -> Result<Vec<MarkdownFile>> {
    let mut files = Vec::new();
    let walker = ignore::WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(false)
        .build();
    for entry in walker.flatten() {
        if files.len() >= MAX_SCAN_FILES {
            break;
        }
        let path = entry.path();
        if !path.is_file() {
            // Directory pruning: `ignore` respects .gitignore by default but
            // not our fixed skip list.
            continue;
        }
        let relative = match path.strip_prefix(root) {
            Ok(relative) => relative,
            Err(_) => continue,
        };
        if relative
            .components()
            .any(|component| SKIP_DIRS.contains(&component.as_os_str().to_string_lossy().as_ref()))
        {
            continue;
        }
        let is_markdown = path
            .extension()
            .is_some_and(|extension| extension == "md" || extension == "markdown");
        if !is_markdown {
            continue;
        }
        let metadata = match fs::metadata(path) {
            Ok(metadata) if metadata.len() <= MAX_SCAN_FILE_BYTES => metadata,
            _ => continue,
        };
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(_) => continue, // Binary or unreadable: not knowledge.
        };
        let modified_at = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis() as i64)
            .unwrap_or(0);
        files.push(MarkdownFile {
            relative_path: relative.to_string_lossy().replace('\\', "/"),
            content,
            modified_at,
        });
    }
    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(files)
}

/// Whether `ch` is CJK ideograph/kana/hangul/fullwidth punctuation. The FTS5
/// default tokenizer never splits CJK runs, so "普通正文" would be one token
/// and a two-character query would never match; indexing with single spaces
/// between CJK chars turns every char into a token and quoted queries into
/// phrase queries, which is the standard CJK workaround.
fn is_cjk(ch: char) -> bool {
    matches!(ch as u32,
        0x3040..=0x30FF // kana
        | 0x3400..=0x4DBF // CJK ext A
        | 0x4E00..=0x9FFF // CJK unified
        | 0xF900..=0xFAFF // compatibility ideographs
        | 0xAC00..=0xD7AF // hangul
        | 0x3000..=0x303F // CJK punctuation
    )
}

fn cjk_spaced(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let mut previous_cjk = false;
    for ch in text.chars() {
        let cjk = is_cjk(ch);
        if cjk && previous_cjk {
            out.push(' ');
        }
        out.push(ch);
        previous_cjk = cjk;
    }
    out
}

/// Inverse of [`cjk_spaced`] for display: removes a space only when both
/// neighbours are CJK, leaving latin spacing untouched.
fn cjk_despace(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (index, &ch) in chars.iter().enumerate() {
        if ch == ' '
            && index > 0
            && index + 1 < chars.len()
            && is_cjk(chars[index - 1])
            && is_cjk(chars[index + 1])
        {
            continue;
        }
        out.push(ch);
    }
    out
}

/// Rule-based suggestion from file/dir names and content shape. Returns
/// (logical_type, title).
fn suggest_type(relative_path: &str, content: &str) -> (String, String) {
    let lower = relative_path.to_lowercase();
    let file_name = lower.rsplit('/').next().unwrap_or(&lower);
    let stem = file_name
        .trim_end_matches(".md")
        .trim_end_matches(".markdown");
    let logical_type = |value: &str| value.to_string();

    let kind = if stem == "todo" || stem == "tasks" || stem == "待办" {
        logical_type("tasks")
    } else if stem == "readme" || stem == "project" || stem == "项目介绍" {
        logical_type("brief")
    } else if stem == "progress"
        || stem == "进度"
        || stem == "changelog"
        || stem.starts_with("changelog")
    {
        logical_type("progress")
    } else if stem == "architecture" || stem == "设计方案" || lower.contains("architecture") {
        logical_type("architecture")
    } else if stem == "environment" || stem == "开发环境" {
        logical_type("environment")
    } else if stem == "runbook" || stem == "部署手册" {
        logical_type("runbook")
    } else if stem == "agents" || stem == "claude" {
        logical_type("agent_instructions")
    } else if lower.contains("adr") || lower.contains("decision") || lower.contains("决策") {
        logical_type("decision")
    } else if lower.contains("implementation")
        || lower.contains("plan")
        || lower.contains("实施方案")
    {
        logical_type("implementation")
    } else if lower.contains("reference") || lower.contains("research") || lower.contains("资料")
    {
        logical_type("reference")
    } else {
        logical_type("general")
    };

    let title = content
        .lines()
        .find_map(|line| {
            let line = line.trim();
            line.strip_prefix("# ")
                .map(|title| title.trim().to_string())
                .filter(|title| !title.is_empty())
        })
        .unwrap_or_else(|| stem.to_string());
    (kind, title)
}

fn content_hash(content: &str) -> String {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Markdown → sanitized HTML. Rebuilds from scratch every call: the HTML is a
/// disposable projection (blueprint §4.4).
pub fn render_markdown(markdown: &str) -> String {
    let parser = Parser::new_ext(
        markdown,
        Options::ENABLE_TABLES | Options::ENABLE_FOOTNOTES | Options::ENABLE_STRIKETHROUGH,
    );
    let mut html = String::with_capacity(markdown.len() * 2);
    html::push_html(&mut html, parser);
    sanitize_html(&html)
}

fn document_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeDocument> {
    Ok(KnowledgeDocument {
        id: row.get(0)?,
        project_id: row.get(1)?,
        relative_path: row.get(2)?,
        logical_type: row.get(3)?,
        title: row.get(4)?,
        classification_source: row.get(5)?,
        modified_at: row.get(6)?,
        indexed_at: row.get(7)?,
        tombstone: row.get::<_, i64>(8)? != 0,
    })
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, WorkspaceStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = WorkspaceStore::new(dir.path().join("ws"));
        (dir, store)
    }

    /// Returns both tempdirs: dropping the store dir would delete the
    /// registry database itself, so callers must hold both.
    fn project_with(
        files: &[(&str, &str)],
    ) -> (tempfile::TempDir, tempfile::TempDir, WorkspaceStore, String) {
        let (store_dir, store) = store();
        let workspace = store.create_workspace("Main", "").unwrap();
        let project_dir = tempfile::tempdir().unwrap();
        for (name, content) in files {
            let path = project_dir.path().join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
        let project = store
            .add_project(&workspace.id, project_dir.path())
            .unwrap()
            .project;
        (store_dir, project_dir, store, project.id)
    }

    #[test]
    fn scan_suggests_types_from_names_and_keeps_files_in_place() {
        let (_s, _p, store, project) = project_with(&[
            ("README.md", "# My Project\n\n目标\n"),
            ("TODO.md", "# Tasks\n- [ ] x\n"),
            ("docs/adr/0003-choice.md", "# ADR 3\n"),
            ("docs/notes.md", "# 随手记\n"),
        ]);
        let report = store.knowledge_scan(&project).unwrap();
        assert_eq!(report.scanned, 4);
        assert_eq!(report.indexed, 4);
        let by_path = |path: &str| {
            report
                .documents
                .iter()
                .find(|doc| doc.relative_path == path)
                .unwrap()
        };
        assert_eq!(by_path("README.md").logical_type, "brief");
        assert_eq!(by_path("TODO.md").logical_type, "tasks");
        assert_eq!(by_path("docs/adr/0003-choice.md").logical_type, "decision");
        assert_eq!(by_path("docs/notes.md").logical_type, "general");
        assert_eq!(by_path("README.md").title, "My Project");
        // Files untouched on disk: the scan only reads.
        assert!(by_path("README.md").relative_path == "README.md");
    }

    #[test]
    fn rescan_skips_unchanged_and_tombstones_deleted() {
        let (_s, project_dir, store, project) =
            project_with(&[("README.md", "# Title\n"), ("old.md", "# Old\n")]);
        store.knowledge_scan(&project).unwrap();
        let report = store.knowledge_scan(&project).unwrap();
        assert_eq!(report.unchanged, 2);
        assert_eq!(report.indexed, 0);

        fs::remove_file(project_dir.path().join("old.md")).unwrap();
        let report = store.knowledge_scan(&project).unwrap();
        assert_eq!(report.tombstoned, 1);
        let old = report
            .documents
            .iter()
            .find(|doc| doc.relative_path == "old.md")
            .unwrap();
        assert!(old.tombstone);
        // The tombstone row survives; history references keep resolving.
        assert_eq!(store.knowledge_list(&project).unwrap().len(), 2);
    }

    #[test]
    fn confirm_overrides_suggestion_and_survives_rescan_and_content_change() {
        let (_s, project_dir, store, project) = project_with(&[("docs/notes.md", "# 随手记\n")]);
        let report = store.knowledge_scan(&project).unwrap();
        let doc = &report.documents[0];
        assert_eq!(doc.logical_type, "general");
        assert_eq!(doc.classification_source, "suggested");

        let confirmed = store.knowledge_confirm(&doc.id, "reference").unwrap();
        assert_eq!(confirmed.classification_source, "confirmed");

        // Rescan: confirmed mapping wins over a fresh suggestion.
        store.knowledge_scan(&project).unwrap();
        let listed = store.knowledge_list(&project).unwrap();
        assert_eq!(listed[0].logical_type, "reference");

        // Content change: still confirmed, title stays user-confirmed era.
        fs::write(project_dir.path().join("docs/notes.md"), "# 改名了\n").unwrap();
        store.knowledge_scan(&project).unwrap();
        let listed = store.knowledge_list(&project).unwrap();
        assert_eq!(listed[0].logical_type, "reference");
    }

    #[test]
    fn confirm_rejects_unknown_type() {
        let (_s, _p, store, project) = project_with(&[("README.md", "# T\n")]);
        store.knowledge_scan(&project).unwrap();
        let doc = &store.knowledge_list(&project).unwrap()[0];
        assert!(store.knowledge_confirm(&doc.id, "not-a-type").is_err());
    }

    #[test]
    fn read_renders_sanitized_html_from_disk() {
        let (_s, _p, store, project) = project_with(&[(
            "docs/design.md",
            "# 设计\n\n<script>alert(1)</script>\n\n**bold**\n",
        )]);
        store.knowledge_scan(&project).unwrap();
        let doc = &store.knowledge_list(&project).unwrap()[0];
        let content = store.knowledge_read(&doc.id).unwrap();
        assert!(content.content_html.contains("<h1>设计</h1>"));
        assert!(content.content_html.contains("<strong>bold</strong>"));
        assert!(
            !content.content_html.contains("<script>"),
            "XSS survived: {}",
            content.content_html
        );
    }

    #[test]
    fn read_reflects_external_edits_without_rescan() {
        let (_s, project_dir, store, project) = project_with(&[("README.md", "# v1\n")]);
        store.knowledge_scan(&project).unwrap();
        let doc = &store.knowledge_list(&project).unwrap()[0];
        fs::write(project_dir.path().join("README.md"), "# v2 外部修改\n").unwrap();
        let content = store.knowledge_read(&doc.id).unwrap();
        assert!(content.content_html.contains("v2 外部修改"));
    }

    #[test]
    fn read_missing_file_reports_io_error() {
        let (_s, project_dir, store, project) = project_with(&[("README.md", "# T\n")]);
        store.knowledge_scan(&project).unwrap();
        let doc = &store.knowledge_list(&project).unwrap()[0];
        fs::remove_file(project_dir.path().join("README.md")).unwrap();
        assert!(store.knowledge_read(&doc.id).is_err());
    }

    #[test]
    fn skips_build_and_private_directories() {
        let (_s, _p, store, project) = project_with(&[
            ("target/debug/notes.md", "# build\n"),
            ("node_modules/pkg/readme.md", "# dep\n"),
            (".virgo/agent/SKILL.md", "# agent\n"),
            ("docs/real.md", "# real\n"),
        ]);
        let report = store.knowledge_scan(&project).unwrap();
        assert_eq!(report.scanned, 1);
        assert_eq!(report.documents[0].relative_path, "docs/real.md");
    }

    #[test]
    fn render_markdown_is_standalone_and_safe() {
        let html = render_markdown("# A\n\n<img src=\"x\" onerror=\"evil()\">\n");
        assert!(html.contains("<h1>A</h1>"));
        assert!(!html.contains("onerror"));
    }
    #[test]
    fn scan_indexes_full_text_and_search_finds_body_terms() {
        let (store_dir, project_dir, store, project) = project_with(&[
            ("README.md", "# 部署手册\n\nArgo 回滚使用 `argocd app rollback`，先查历史。\n"),
            ("docs/notes.md", "# 随手记\n\n午餐吃什么。\n"),
        ]);
        let _ = store_dir;
        store.knowledge_scan(&project).unwrap();
        let hits = store.knowledge_search(&project, "Argo").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].document.relative_path, "README.md");
        assert!(hits[0].snippet.contains("Argo"), "{}", hits[0].snippet);
        assert!(hits[0].snippet.contains("【"));
        // Unrelated term finds nothing; empty query returns nothing.
        assert!(store.knowledge_search(&project, "午餐").is_ok());
        assert!(store.knowledge_search(&project, "  ").unwrap().is_empty());

        // Deleting the file tombstones the row and removes it from search.
        std::fs::remove_file(project_dir.path().join("README.md")).unwrap();
        let report = store.knowledge_scan(&project).unwrap();
        assert_eq!(report.tombstoned, 1);
        assert!(store.knowledge_search(&project, "Argo").unwrap().is_empty());
    }

    #[test]
    fn search_quotes_user_input_instead_of_erroring() {
        let (store_dir, _project_dir, store, project) = project_with(&[
            ("README.md", "# 手册\n\n普通正文。\n"),
        ]);
        let _ = store_dir;
        store.knowledge_scan(&project).unwrap();
        // FTS5 syntax characters must be treated as literals, not operators.
        for query in ["\"quoted\"", "AND OR NOT", "a*b", "NEAR(", "\'"] {
            assert!(
                store.knowledge_search(&project, query).is_ok(),
                "query {query} must not error"
            );
        }
        assert!(store
            .knowledge_search(&project, "普通")
            .unwrap()
            .len()
            == 1);
    }

}
