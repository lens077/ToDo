//! 阅读: a personal read-later library.
//!
//! An article is fetched once, reduced to its readable body, and stored with
//! its images under `~/.todo/reading/`. After that the original site is never
//! contacted again: the copy the user annotates is the copy they saw.
//!
//! Reading position follows the Readeck / Karakeep convention: a percentage
//! for the list and an anchor (the block the viewport was on) for restoring
//! the exact place. 100% is "read" and sticky; 0 and 100 carry no anchor.
//! Annotations use the W3C text-quote shape (`quote` + character offsets)
//! against the stored body, which never changes, so they re-anchor reliably.

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

use regex::Regex;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Error, Result};

const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.4 Safari/605.1.15";
/// 微信正文页对「桌面 Safari」这类指纹会返回「参数错误」壳（尤其移动网络出口）。
/// 用微信内置浏览器的 UA 再试一次，能过风控的概率明显更高。
const WECHAT_USER_AGENT: &str = "Mozilla/5.0 (Linux; Android 14; Pixel 7 Build/UQ1A.240205.004) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/122.0.6261.120 Mobile Safari/537.36 XWEB/1220117 MMWEBSDK/20240404 MMWEBID/1234 MicroMessenger/8.0.49.2600(0x28003133) WeChat/arm64 Weixin NetType/WIFI Language/zh_CN ABI/arm64";
const MAX_IMAGES: usize = 60;
const MAX_IMAGE_BYTES: usize = 12 * 1024 * 1024;
const MAX_PAGE_BYTES: usize = 24 * 1024 * 1024;
/// 一次失败解析最多尝试几次（含第一次）。到达上限后只保留链接，不再自动重试；
/// 手动「重试解析」会把计数清零。
pub const MAX_PARSE_ATTEMPTS: u32 = 3;
/// 每次失败后的等待时间。只有前两次失败会安排重试，第三次即终态。
const PARSE_RETRY_BACKOFF_MS: [i64; 2] = [60_000, 300_000];
/// 从服务器拉取正文时并发下载图片的线程数。图片已经在服务器本地，并发能把
/// 一篇多图文章的等待从「张数 × 往返」压到接近一次往返。
const MAX_IMAGE_WORKERS: usize = 6;

/// One saved link as the list shows it: metadata and progress, no body.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReadingArticle {
    pub id: String,
    pub url: String,
    pub title: String,
    pub site_name: String,
    pub excerpt: String,
    pub byline: Option<String>,
    /// Absolute path of the localized cover image, if the page had one.
    pub cover_path: Option<String>,
    pub word_count: u32,
    pub reading_minutes: u32,
    /// Epoch milliseconds.
    pub added_at: i64,
    pub last_opened_at: Option<i64>,
    /// 0–100. 100 means read.
    pub progress_percent: u32,
    pub progress_anchor: Option<String>,
    pub read_at: Option<i64>,
    pub annotation_count: u32,
    /// `pending` while the body is still being fetched, `ok` when it is in the
    /// library, `failed` when the last parse attempt did not produce content.
    #[serde(default = "default_parse_status")]
    pub parse_status: String,
    /// Why the last parse failed; empty when it succeeded.
    #[serde(default)]
    pub parse_error: String,
}

fn default_parse_status() -> String {
    "ok".to_string()
}

/// The article with its body, as the reader needs it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReadingArticleDetail {
    #[serde(flatten)]
    pub article: ReadingArticle,
    /// Sanitized HTML. Image `src` values are `images/<file>` relative to `image_dir`.
    pub content_html: String,
    pub markdown: Option<String>,
    /// Absolute directory holding this article's localized images.
    pub image_dir: String,
    pub annotations: Vec<ReadingAnnotation>,
    /// User-authored context. Only `update_context` writes these fields;
    /// re-fetching an article can never touch them.
    #[serde(default = "ReadingContext::empty")]
    pub context: ReadingContext,
}

/// Why an article was saved and what to do with it later. Stored on the
/// article row but written exclusively through [`ReadingStore::update_context`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ReadingContext {
    pub saved_reason: String,
    pub focus_questions: String,
    pub revisit_condition: String,
    pub personal_notes: String,
    /// `NONE` | `DATE` | `TASK_COMPLETED` | `PROJECT_COMPLETED` | `MANUAL` (stored only in v1).
    pub revisit_type: String,
    /// `ok` | `failed` — failed records keep the URL and accept context edits.
    pub parse_status: String,
    #[serde(default)]
    pub project_links: Vec<ReadingLink>,
    #[serde(default)]
    pub task_links: Vec<ReadingLink>,
}

impl ReadingContext {
    pub fn empty() -> Self {
        Self {
            revisit_type: "NONE".into(),
            parse_status: "ok".into(),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReadingLink {
    /// Registry project ID or task ID on the reading side; the reading store
    /// never writes into project files (ADR-0003 boundary).
    pub reference: String,
    pub label: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReadingContextInput {
    pub saved_reason: String,
    pub focus_questions: String,
    pub revisit_condition: String,
    pub personal_notes: String,
    #[serde(default = "default_revisit_type")]
    pub revisit_type: String,
}

fn default_revisit_type() -> String {
    "NONE".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReadingAnnotation {
    pub id: String,
    pub article_id: String,
    pub quote: String,
    pub start: u32,
    pub end: u32,
    pub note: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 另存为的输出格式；前端以 `html` / `markdown` 小写字符串传参。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReadingExportFormat {
    Html,
    Markdown,
}

impl ReadingExportFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Markdown => "md",
        }
    }
}

/// 一次「另存为」的产物：主文件绝对路径与保存进 `img/` 的图片数。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReadingExportResult {
    pub path: String,
    pub images: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadingAnnotationInput {
    pub id: String,
    pub article_id: String,
    pub quote: String,
    pub start: u32,
    pub end: u32,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone)]
pub struct ReadingStore {
    root: PathBuf,
}

impl ReadingStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn global() -> Result<Self> {
        let home = dirs::home_dir().ok_or(Error::HomeDirectoryUnavailable)?;
        Ok(Self::new(home.join(".todo").join("reading")))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn image_dir(&self, article_id: &str) -> PathBuf {
        self.root.join("images").join(article_id)
    }

    fn connect(&self) -> Result<Connection> {
        fs::create_dir_all(&self.root).map_err(|source| Error::Io {
            path: self.root.clone(),
            source,
        })?;
        let connection = Connection::open(self.root.join("reading.sqlite"))?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS articles (
               id TEXT PRIMARY KEY,
               url TEXT NOT NULL,
               title TEXT NOT NULL,
               site_name TEXT NOT NULL DEFAULT '',
               excerpt TEXT NOT NULL DEFAULT '',
               byline TEXT,
               cover_file TEXT,
               content_html TEXT NOT NULL,
               markdown TEXT,
               word_count INTEGER NOT NULL DEFAULT 0,
               reading_minutes INTEGER NOT NULL DEFAULT 0,
               added_at INTEGER NOT NULL,
               last_opened_at INTEGER,
               progress_percent INTEGER NOT NULL DEFAULT 0,
               progress_anchor TEXT,
               read_at INTEGER
             );
             CREATE UNIQUE INDEX IF NOT EXISTS articles_url ON articles(url);
             CREATE TABLE IF NOT EXISTS annotations (
               id TEXT PRIMARY KEY,
               article_id TEXT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
               quote TEXT NOT NULL,
               start INTEGER NOT NULL,
               end INTEGER NOT NULL,
               note TEXT NOT NULL DEFAULT '',
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS annotations_article ON annotations(article_id, start);
             CREATE TABLE IF NOT EXISTS article_project_links (
               article_id TEXT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
               project_ref TEXT NOT NULL,
               label TEXT NOT NULL DEFAULT '',
               created_at INTEGER NOT NULL,
               PRIMARY KEY (article_id, project_ref)
             );
             CREATE TABLE IF NOT EXISTS article_task_links (
               article_id TEXT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
               task_ref TEXT NOT NULL,
               label TEXT NOT NULL DEFAULT '',
               created_at INTEGER NOT NULL,
               PRIMARY KEY (article_id, task_ref)
             );",
        )?;
        ensure_columns(
            &connection,
            "articles",
            &[
                ("saved_reason", "TEXT NOT NULL DEFAULT ''"),
                ("focus_questions", "TEXT NOT NULL DEFAULT ''"),
                ("revisit_condition", "TEXT NOT NULL DEFAULT ''"),
                ("personal_notes", "TEXT NOT NULL DEFAULT ''"),
                ("revisit_type", "TEXT NOT NULL DEFAULT 'NONE'"),
                ("parse_status", "TEXT NOT NULL DEFAULT 'ok'"),
                ("parse_error", "TEXT NOT NULL DEFAULT ''"),
                ("parse_attempts", "INTEGER NOT NULL DEFAULT 0"),
                // 到期时间（epoch ms）；NULL 表示没有安排自动重试。
                ("parse_retry_at", "INTEGER"),
            ],
        )?;
        connection.execute_batch("PRAGMA foreign_keys = ON;")?;
        Self::normalize_wechat_site_names(&connection);
        Ok(connection)
    }

    /// 一次性修复旧数据：早期的微信解析把作者名（byline）写进了 site_name，
    /// 公众号名整条丢失。摘要（og:description）是「原创 作者 公众号」连排，
    /// 能认出公众号名就把 site_name 纠正回来。修好的行不再命中 WHERE，
    /// 重复执行是空转。
    fn normalize_wechat_site_names(connection: &Connection) {
        let Ok(mut statement) = connection.prepare(
            "SELECT id, byline, excerpt FROM articles
             WHERE url LIKE '%mp.weixin.qq.com%' AND byline != '' AND site_name = byline",
        ) else {
            return;
        };
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .ok();
        let Some(rows) = rows else {
            return;
        };
        for row in rows.flatten() {
            let (id, byline, excerpt) = row;
            let Ok(pattern) = Regex::new(&format!(
                r#"^原创[，,]?\s*{}\s+(\S+)\s*$"#,
                regex::escape(byline.trim())
            )) else {
                continue;
            };
            let Some(account) = pattern
                .captures(excerpt.trim())
                .and_then(|captures| captures.get(1))
                .map(|matched| matched.as_str().to_string())
            else {
                continue;
            };
            let _ = connection.execute(
                "UPDATE articles SET site_name = ?2 WHERE id = ?1",
                params![id, account],
            );
        }
    }

    /// Fetches, extracts, localizes images, and stores the article. Adding a
    /// URL that is already saved returns the existing record untouched.
    pub fn add(&self, url: &str) -> Result<ReadingArticleDetail> {
        let url = normalize_url(url)?;
        if let Some(existing) = self.find_by_url(&url)? {
            return self.get(&existing);
        }
        let fetched = fetch_page(&url)?;
        self.add_fetched(&url, fetched)
    }

    /// Imports the HTML currently visible in a browser. This is the escape
    /// hatch for pages that require a browser session (notably WeChat): the
    /// browser supplies the authenticated/JS-rendered HTML, while the local
    /// store still owns sanitizing, extracting, and persisting it.
    pub fn add_browser_html(&self, url: &str, html: &str) -> Result<ReadingArticleDetail> {
        let url = normalize_url(url)?;
        if html.trim().is_empty() || html.len() > MAX_PAGE_BYTES {
            return Err(Error::ReadingFetch("浏览器页面内容为空或过大".into()));
        }
        if let Some(existing) = self.find_by_url(&url)? {
            return self.get(&existing);
        }
        self.add_fetched(
            &url,
            FetchedPage {
                html: html.to_string(),
                final_url: url.clone(),
            },
        )
    }

    fn add_fetched(&self, url: &str, fetched: FetchedPage) -> Result<ReadingArticleDetail> {
        let extracted = extract(&fetched, url)?;
        let id = new_id();
        let image_dir = self.image_dir(&id);
        let localized = localize_images(&extracted.content_html, url, &image_dir);
        let cover_file = extracted
            .cover_url
            .as_deref()
            .and_then(|cover| localize_cover(cover, url, &image_dir));
        let now = now_ms();
        let connection = self.connect()?;
        connection.execute(
            "INSERT INTO articles (id, url, title, site_name, excerpt, byline, cover_file, content_html, markdown,
                                   word_count, reading_minutes, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                id,
                url,
                extracted.title,
                extracted.site_name,
                extracted.excerpt,
                extracted.byline,
                cover_file,
                localized,
                extracted.markdown,
                extracted.word_count,
                extracted.reading_minutes,
                now,
            ],
        )?;
        drop(connection);
        self.get(&id)
    }

    pub fn list(&self) -> Result<Vec<ReadingArticle>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT a.id, a.url, a.title, a.site_name, a.excerpt, a.byline, a.cover_file, a.word_count,
                    a.reading_minutes, a.added_at, a.last_opened_at, a.progress_percent, a.progress_anchor,
                    a.read_at, (SELECT COUNT(*) FROM annotations n WHERE n.article_id = a.id),
                    a.parse_status, a.parse_error
             FROM articles a
             ORDER BY a.added_at DESC",
        )?;
        let rows = statement.query_map([], |row| self.article_from_row(row))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Error::from)
    }

    pub fn get(&self, id: &str) -> Result<ReadingArticleDetail> {
        let connection = self.connect()?;
        let article = connection
            .query_row(
                "SELECT a.id, a.url, a.title, a.site_name, a.excerpt, a.byline, a.cover_file, a.word_count,
                        a.reading_minutes, a.added_at, a.last_opened_at, a.progress_percent, a.progress_anchor,
                        a.read_at, (SELECT COUNT(*) FROM annotations n WHERE n.article_id = a.id),
                        a.parse_status, a.parse_error
                 FROM articles a WHERE a.id = ?1",
                params![id],
                |row| self.article_from_row(row),
            )
            .optional()?
            .ok_or_else(|| Error::ReadingArticleNotFound(id.to_string()))?;
        let (content_html, markdown): (String, Option<String>) = connection.query_row(
            "SELECT content_html, markdown FROM articles WHERE id = ?1",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let mut statement = connection.prepare(
            "SELECT id, article_id, quote, start, end, note, created_at, updated_at
             FROM annotations WHERE article_id = ?1 ORDER BY start ASC, created_at ASC",
        )?;
        let annotations = statement
            .query_map(params![id], |row| {
                Ok(ReadingAnnotation {
                    id: row.get(0)?,
                    article_id: row.get(1)?,
                    quote: row.get(2)?,
                    start: row.get(3)?,
                    end: row.get(4)?,
                    note: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let context = self.context(&connection, &article.id)?;
        Ok(ReadingArticleDetail {
            image_dir: self.image_dir(&article.id).to_string_lossy().into_owned(),
            article,
            content_html,
            markdown,
            annotations,
            context,
        })
    }

    /// Loads the user-authored context plus the project/task links. Plain
    /// column reads: nothing here can resurrect lost context because re-parse
    /// paths never write these columns.
    fn context(&self, connection: &Connection, article_id: &str) -> Result<ReadingContext> {
        let mut context = connection
            .query_row(
                "SELECT saved_reason, focus_questions, revisit_condition, personal_notes,
                        revisit_type, parse_status
                 FROM articles WHERE id = ?1",
                params![article_id],
                |row| {
                    Ok(ReadingContext {
                        saved_reason: row.get(0)?,
                        focus_questions: row.get(1)?,
                        revisit_condition: row.get(2)?,
                        personal_notes: row.get(3)?,
                        revisit_type: row.get(4)?,
                        parse_status: row.get(5)?,
                        project_links: Vec::new(),
                        task_links: Vec::new(),
                    })
                },
            )
            .optional()?
            .unwrap_or_else(ReadingContext::empty);
        let links = |table: &str, column: &str| -> Result<Vec<ReadingLink>> {
            let mut statement = connection.prepare(&format!(
                "SELECT {column} AS reference, label, created_at FROM {table} WHERE article_id = ?1 ORDER BY created_at"
            ))?;
            let rows = statement.query_map(params![article_id], |row| {
                Ok(ReadingLink {
                    reference: row.get(0)?,
                    label: row.get(1)?,
                    created_at: row.get(2)?,
                })
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(Error::from)
        };
        context.project_links = links("article_project_links", "project_ref")?;
        context.task_links = links("article_task_links", "task_ref")?;
        Ok(context)
    }

    /// Saves the user-authored context. This is the only write path for these
    /// fields — the structural guarantee behind "re-parse never overwrites".
    pub fn update_context(
        &self,
        id: &str,
        input: &ReadingContextInput,
    ) -> Result<ReadingArticleDetail> {
        let connection = self.connect()?;
        let changed = connection.execute(
            "UPDATE articles SET saved_reason = ?2, focus_questions = ?3, revisit_condition = ?4,
                    personal_notes = ?5, revisit_type = ?6
             WHERE id = ?1",
            params![
                id,
                input.saved_reason.trim(),
                input.focus_questions.trim(),
                input.revisit_condition.trim(),
                input.personal_notes.trim(),
                if input.revisit_type.trim().is_empty() {
                    "NONE"
                } else {
                    input.revisit_type.trim()
                },
            ],
        )?;
        if changed == 0 {
            return Err(Error::ReadingArticleNotFound(id.to_string()));
        }
        drop(connection);
        self.get(id)
    }

    /// Saves a URL that could not be parsed. The record keeps the URL, enters
    /// the library as `failed`, and accepts context edits immediately
    /// (收藏优先：解析失败不能阻止收藏).
    pub fn add_failed(&self, url: &str) -> Result<ReadingArticleDetail> {
        let url = normalize_url(url)?;
        if let Some(existing) = self.find_by_url(&url)? {
            return self.get(&existing);
        }
        let id = new_id();
        let connection = self.connect()?;
        connection.execute(
            "INSERT INTO articles (id, url, title, content_html, added_at, parse_status)
             VALUES (?1, ?2, ?3, '', ?4, 'failed')",
            params![id, url, host_of(&url), now_ms()],
        )?;
        drop(connection);
        self.get(&id)
    }

    /// 收藏优先：先以 `pending` 入列，返回毫秒级；正文由调用方在后台线程抓取，
    /// 成功后用 `refetch` 写入正文，失败则 `mark_failed`。保存链接本身不该等网络。
    pub fn add_pending(&self, url: &str) -> Result<ReadingArticleDetail> {
        let url = normalize_url(url)?;
        if let Some(existing) = self.find_by_url(&url)? {
            return self.get(&existing);
        }
        let id = new_id();
        let connection = self.connect()?;
        connection.execute(
            "INSERT INTO articles (id, url, title, content_html, added_at, parse_status)
             VALUES (?1, ?2, ?3, '', ?4, 'pending')",
            params![id, url, host_of(&url), now_ms()],
        )?;
        drop(connection);
        self.get(&id)
    }

    /// Records that the background parse did not produce content and schedules
    /// the next attempt with a backoff. The record stays in the library so the
    /// person can still keep the link; the reason rides along so the card can
    /// explain itself. After [`MAX_PARSE_ATTEMPTS`] the row is terminal: the
    /// link stays, but no automatic retry is scheduled any more.
    pub fn mark_failed(&self, id: &str, reason: &str) -> Result<()> {
        let connection = self.connect()?;
        let previous: i64 = connection
            .query_row(
                "SELECT parse_attempts FROM articles WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| Error::ReadingArticleNotFound(id.to_string()))?;
        let attempts = previous + 1;
        let retry_at = if attempts as u32 >= MAX_PARSE_ATTEMPTS {
            None
        } else {
            let wait = PARSE_RETRY_BACKOFF_MS
                [(attempts as usize - 1).min(PARSE_RETRY_BACKOFF_MS.len() - 1)];
            Some(now_ms() + wait)
        };
        connection.execute(
            "UPDATE articles SET parse_status = 'failed', parse_error = ?2,
                    parse_attempts = ?3, parse_retry_at = ?4
             WHERE id = ?1",
            params![id, reason, attempts, retry_at],
        )?;
        Ok(())
    }

    /// Puts a record back into the queue for a manual retry: the attempt count
    /// is reset, so a person asking again gets the full automatic budget.
    pub fn mark_pending(&self, id: &str) -> Result<()> {
        let connection = self.connect()?;
        let changed = connection.execute(
            "UPDATE articles SET parse_status = 'pending', parse_error = '',
                    parse_attempts = 0, parse_retry_at = NULL
             WHERE id = ?1",
            params![id],
        )?;
        if changed == 0 {
            return Err(Error::ReadingArticleNotFound(id.to_string()));
        }
        Ok(())
    }

    /// Puts a record back into the queue for an automatic retry. The attempt
    /// count is kept, so repeated failures still reach the terminal cap.
    pub fn mark_retrying(&self, id: &str) -> Result<()> {
        let connection = self.connect()?;
        let changed = connection.execute(
            "UPDATE articles SET parse_status = 'pending', parse_error = '',
                    parse_retry_at = NULL
             WHERE id = ?1",
            params![id],
        )?;
        if changed == 0 {
            return Err(Error::ReadingArticleNotFound(id.to_string()));
        }
        Ok(())
    }

    /// Every article still waiting for its body, oldest first. Used to resume
    /// parses that were cut short by an app restart.
    pub fn pending_ids(&self) -> Result<Vec<String>> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare("SELECT id FROM articles WHERE parse_status = 'pending' ORDER BY added_at")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Error::from)
    }

    /// Failed rows whose backoff has elapsed and which still have attempts
    /// left. This is what keeps a dead link from being fetched on every sync:
    /// terminal rows never show up here, and live ones only when due.
    pub fn due_retry_ids(&self, limit: usize) -> Result<Vec<String>> {
        self.due_retry_ids_at(now_ms(), limit)
    }

    pub(crate) fn due_retry_ids_at(&self, now: i64, limit: usize) -> Result<Vec<String>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id FROM articles
             WHERE parse_status = 'failed' AND parse_attempts < ?1
               AND (parse_retry_at IS NULL OR parse_retry_at <= ?2)
             ORDER BY added_at LIMIT ?3",
        )?;
        let rows = statement.query_map(params![MAX_PARSE_ATTEMPTS, now, limit as i64], |row| {
            row.get::<_, String>(0)
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Error::from)
    }

    /// Re-fetches the article and replaces only the source metadata and body.
    /// Saved reason, questions, notes, links and reading state are untouched:
    /// the UPDATE simply does not mention those columns.
    pub fn refetch(&self, id: &str) -> Result<ReadingArticleDetail> {
        let url = {
            let connection = self.connect()?;
            connection
                .query_row(
                    "SELECT url FROM articles WHERE id = ?1",
                    params![id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .ok_or_else(|| Error::ReadingArticleNotFound(id.to_string()))?
        };
        let fetched = fetch_page(&url)?;
        self.replace_content(id, &fetched)
    }

    /// Applies fetched content to an existing record (shared by `refetch` and
    /// the retry of a failed capture). Testable without the network.
    #[allow(private_interfaces)]
    pub(crate) fn replace_content(
        &self,
        id: &str,
        fetched: &FetchedPage,
    ) -> Result<ReadingArticleDetail> {
        let url = {
            let connection = self.connect()?;
            connection
                .query_row(
                    "SELECT url FROM articles WHERE id = ?1",
                    params![id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .ok_or_else(|| Error::ReadingArticleNotFound(id.to_string()))?
        };
        let extracted = extract(fetched, &url)?;
        let image_dir = self.image_dir(id);
        let localized = localize_images(&extracted.content_html, &url, &image_dir);
        let cover_file = extracted
            .cover_url
            .as_deref()
            .and_then(|cover| localize_cover(cover, &url, &image_dir));
        let connection = self.connect()?;
        connection.execute(
            "UPDATE articles SET title = ?2, site_name = ?3, excerpt = ?4, byline = ?5, cover_file = ?6,
                    content_html = ?7, markdown = ?8, word_count = ?9, reading_minutes = ?10,
                    parse_status = 'ok', parse_error = '', parse_attempts = 0, parse_retry_at = NULL
             WHERE id = ?1",
            params![
                id,
                extracted.title,
                extracted.site_name,
                extracted.excerpt,
                extracted.byline,
                cover_file,
                localized,
                extracted.markdown,
                extracted.word_count,
                extracted.reading_minutes,
            ],
        )?;
        drop(connection);
        self.get(id)
    }

    /// Stores an article another device already parsed. The body and its
    /// images come from the sync server, so the original site is not touched
    /// again — this is what keeps a reconciliation to a couple of requests per
    /// new article instead of a full re-parse.
    ///
    /// A URL already in the library keeps its context and reading state. If it
    /// was still a bodyless stub when it was first pulled, the body is filled
    /// in here once the server has one.
    pub fn save_pulled(
        &self,
        detail: &ReadingArticleDetail,
        base: &str,
    ) -> Result<ReadingArticleDetail> {
        let url = normalize_url(&detail.article.url)?;
        let base = base.trim().trim_end_matches('/');
        if let Some(existing_id) = self.find_by_url(&url)? {
            let existing = self.get(&existing_id)?;
            if existing.article.word_count > 0 || detail.article.word_count == 0 {
                return Ok(existing);
            }
            let image_dir = self.image_dir(&existing_id);
            let content_html =
                localize_remote_images(&detail.content_html, base, &detail.article.id, &image_dir);
            let cover_file = remote_cover_file(detail, base, &image_dir);
            let connection = self.connect()?;
            connection.execute(
                "UPDATE articles SET title = ?2, site_name = ?3, excerpt = ?4, byline = ?5,
                        cover_file = ?6, content_html = ?7, markdown = ?8, word_count = ?9,
                        reading_minutes = ?10, parse_status = 'ok', parse_error = '',
                        parse_attempts = 0, parse_retry_at = NULL
                 WHERE id = ?1",
                params![
                    existing_id,
                    detail.article.title,
                    detail.article.site_name,
                    detail.article.excerpt,
                    detail.article.byline,
                    cover_file,
                    content_html,
                    detail.markdown,
                    detail.article.word_count,
                    detail.article.reading_minutes,
                ],
            )?;
            drop(connection);
            return self.get(&existing_id);
        }

        // Keep the server id when it is free: image references stay stable
        // across devices and re-pulls are idempotent.
        let mut id = detail.article.id.clone();
        if self.get(&id).is_ok() {
            id = new_id();
        }
        let image_dir = self.image_dir(&id);
        let content_html =
            localize_remote_images(&detail.content_html, base, &detail.article.id, &image_dir);
        let cover_file = remote_cover_file(detail, base, &image_dir);
        let has_body = detail.article.word_count > 0 || !detail.content_html.trim().is_empty();
        let parse_error = if has_body {
            String::new()
        } else if detail.article.parse_error.is_empty() {
            "服务器还没有解析出正文".to_string()
        } else {
            detail.article.parse_error.clone()
        };
        // A bodyless pull is due immediately: this device may reach a page the
        // server could not (WeChat is sensitive to the network's region).
        let retry_at = if has_body { None } else { Some(now_ms()) };
        let connection = self.connect()?;
        connection.execute(
            "INSERT INTO articles (id, url, title, site_name, excerpt, byline, cover_file, content_html,
                                   markdown, word_count, reading_minutes, added_at, last_opened_at,
                                   progress_percent, progress_anchor, read_at, saved_reason,
                                   focus_questions, revisit_condition, personal_notes, revisit_type,
                                   parse_status, parse_error, parse_attempts, parse_retry_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                     ?18, ?19, ?20, ?21, ?22, ?23, 0, ?24)",
            params![
                id,
                url,
                detail.article.title,
                detail.article.site_name,
                detail.article.excerpt,
                detail.article.byline,
                cover_file,
                content_html,
                detail.markdown,
                detail.article.word_count,
                detail.article.reading_minutes,
                detail.article.added_at,
                detail.article.last_opened_at,
                detail.article.progress_percent,
                detail.article.progress_anchor,
                detail.article.read_at,
                detail.context.saved_reason,
                detail.context.focus_questions,
                detail.context.revisit_condition,
                detail.context.personal_notes,
                detail.context.revisit_type,
                if has_body { "ok" } else { "failed" },
                parse_error,
                retry_at,
            ],
        )?;
        drop(connection);
        self.get(&id)
    }

    /// Links the article to a registered project (by registry ID). Idempotent.
    pub fn link_project(
        &self,
        id: &str,
        project_ref: &str,
        label: &str,
    ) -> Result<ReadingArticleDetail> {
        self.link(
            id,
            "article_project_links",
            "project_ref",
            project_ref,
            label,
        )?;
        self.get(id)
    }

    pub fn unlink_project(&self, id: &str, project_ref: &str) -> Result<ReadingArticleDetail> {
        self.unlink(id, "article_project_links", "project_ref", project_ref)?;
        self.get(id)
    }

    /// Links the article to a task (by stable task ID). Idempotent.
    pub fn link_task(&self, id: &str, task_ref: &str, label: &str) -> Result<ReadingArticleDetail> {
        self.link(id, "article_task_links", "task_ref", task_ref, label)?;
        self.get(id)
    }

    pub fn unlink_task(&self, id: &str, task_ref: &str) -> Result<ReadingArticleDetail> {
        self.unlink(id, "article_task_links", "task_ref", task_ref)?;
        self.get(id)
    }

    fn link(
        &self,
        id: &str,
        table: &str,
        column: &str,
        reference: &str,
        label: &str,
    ) -> Result<()> {
        let reference = reference.trim();
        if reference.is_empty() {
            return Err(Error::InvalidReadingUrl("关联引用不能为空".into()));
        }
        let connection = self.connect()?;
        let exists: Option<String> = connection
            .query_row(
                "SELECT id FROM articles WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_none() {
            return Err(Error::ReadingArticleNotFound(id.to_string()));
        }
        connection.execute(
            &format!(
                "INSERT OR IGNORE INTO {table} (article_id, {column}, label, created_at) VALUES (?1, ?2, ?3, ?4)"
            ),
            params![id, reference, label.trim(), now_ms()],
        )?;
        Ok(())
    }

    fn unlink(&self, id: &str, table: &str, column: &str, reference: &str) -> Result<()> {
        let connection = self.connect()?;
        let changed = connection.execute(
            &format!("DELETE FROM {table} WHERE article_id = ?1 AND {column} = ?2"),
            params![id, reference],
        )?;
        if changed == 0 {
            return Err(Error::ReadingArticleNotFound(id.to_string()));
        }
        Ok(())
    }

    /// Markdown 引用卡片：标题、作者、来源、保存原因、关联、笔记。
    pub fn export_reference(&self, id: &str) -> Result<String> {
        let detail = self.get(id)?;
        let context = &detail.context;
        let mut markdown = String::new();
        markdown.push_str(
            "---
type: reading
",
        );
        markdown.push_str(&format!(
            "title: {}
",
            detail.article.title
        ));
        markdown.push_str(&format!(
            "url: {}
",
            detail.article.url
        ));
        markdown.push_str(&format!(
            "parse_status: {}
",
            context.parse_status
        ));
        markdown.push_str(&format!(
            "captured_at: {}
---

",
            detail.article.added_at
        ));
        markdown.push_str(&format!(
            "# {}

",
            detail.article.title
        ));
        if let Some(byline) = &detail.article.byline {
            markdown.push_str(&format!(
                "- 作者：{byline}
"
            ));
        }
        markdown.push_str(&format!(
            "- 来源：{}
",
            detail.article.site_name
        ));
        markdown.push_str(&format!(
            "- 原始链接：{}
",
            detail.article.url
        ));
        markdown.push_str(
            "
# 收藏原因

",
        );
        markdown.push_str(if context.saved_reason.is_empty() {
            "（未填写）"
        } else {
            &context.saved_reason
        });
        markdown.push_str(
            "

# 未来关注点

",
        );
        markdown.push_str(if context.focus_questions.is_empty() {
            "（未填写）"
        } else {
            &context.focus_questions
        });
        markdown.push_str(
            "

# 复访条件

",
        );
        markdown.push_str(if context.revisit_condition.is_empty() {
            "（未填写）"
        } else {
            &context.revisit_condition
        });
        markdown.push_str(
            "

# 阅读笔记

",
        );
        markdown.push_str(if context.personal_notes.is_empty() {
            "（未填写）"
        } else {
            &context.personal_notes
        });
        if !context.project_links.is_empty() {
            markdown.push_str(
                "
# 关联项目

",
            );
            for link in &context.project_links {
                markdown.push_str(&format!(
                    "- {}{}
",
                    link.reference,
                    if link.label.is_empty() {
                        String::new()
                    } else {
                        format!("（{}）", link.label)
                    }
                ));
            }
        }
        if !context.task_links.is_empty() {
            markdown.push_str(
                "
# 关联任务

",
            );
            for link in &context.task_links {
                markdown.push_str(&format!(
                    "- {}
",
                    link.reference
                ));
            }
        }
        Ok(markdown)
    }

    /// 项目阅读索引：一个注册项目的全部关联文章，按阅读状态分组。
    pub fn export_project_index(&self, project_ref: &str) -> Result<String> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT a.id, a.title, a.url, a.progress_percent
             FROM article_project_links l
             JOIN articles a ON a.id = l.article_id
             WHERE l.project_ref = ?1
             ORDER BY a.added_at DESC",
        )?;
        let rows = statement.query_map(params![project_ref], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, u32>(3)?,
            ))
        })?;
        let mut unread = Vec::new();
        let mut read = Vec::new();
        for row in rows {
            let (id, title, url, percent) = row?;
            let detail = self.get(&id)?;
            let reason = detail.context.saved_reason;
            let entry = format!(
                "### {title}

- URL: {url}
- 收藏原因：{}
- Virgo: reading://{id}
",
                if reason.is_empty() {
                    "（未填写）"
                } else {
                    &reason
                }
            );
            if percent >= 100 {
                read.push(entry)
            } else {
                unread.push(entry)
            }
        }
        let mut markdown = String::from(
            "# Reading References

",
        );
        markdown.push_str(
            "## 待阅读

",
        );
        if unread.is_empty() {
            markdown.push_str(
                "（无）

",
            );
        } else {
            for entry in &unread {
                markdown.push_str(entry);
                markdown.push('\n');
            }
        }
        markdown.push_str(
            "## 已阅读

",
        );
        if read.is_empty() {
            markdown.push_str(
                "（无）
",
            );
        } else {
            for entry in &read {
                markdown.push_str(entry);
                markdown.push('\n');
            }
        }
        Ok(markdown)
    }

    /// 另存为：把正文与图片写进用户选择的目录，图片放 `img/`，文件名用
    /// 文章标题（重名自动加序号）。返回主文件路径。
    pub fn export_article(
        &self,
        id: &str,
        format: ReadingExportFormat,
        dir: &Path,
    ) -> Result<ReadingExportResult> {
        let detail = self.get(id)?;
        fs::create_dir_all(dir).map_err(|source| Error::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let img_dir = dir.join("img");
        fs::create_dir_all(&img_dir).map_err(|source| Error::Io {
            path: img_dir.clone(),
            source,
        })?;
        let (html, markdown, images) = export_with_local_images(
            &detail.content_html,
            detail.markdown.as_deref(),
            &self.image_dir(id),
            &img_dir,
            &detail.article.url,
        );
        let stem = export_stem(&detail.article.title, id);
        let (path, body) = match format {
            ReadingExportFormat::Html => (
                unique_export_path(dir, &stem, format.extension()),
                export_html_document(&detail, &html),
            ),
            ReadingExportFormat::Markdown => (
                unique_export_path(dir, &stem, format.extension()),
                export_markdown_document(&detail, markdown.as_deref()),
            ),
        };
        fs::write(&path, body).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        Ok(ReadingExportResult {
            path: path.to_string_lossy().into_owned(),
            images,
        })
    }

    /// Records that the reader opened the article now.
    pub fn touch(&self, id: &str) -> Result<()> {
        let connection = self.connect()?;
        let changed = connection.execute(
            "UPDATE articles SET last_opened_at = ?2 WHERE id = ?1",
            params![id, now_ms()],
        )?;
        if changed == 0 {
            return Err(Error::ReadingArticleNotFound(id.to_string()));
        }
        Ok(())
    }

    /// Saves a reading position. Progress never moves backwards once the
    /// article is read; reaching 100 marks it read. 0 and 100 drop the anchor.
    pub fn set_progress(
        &self,
        id: &str,
        percent: u32,
        anchor: Option<&str>,
    ) -> Result<ReadingArticle> {
        let percent = percent.min(100);
        let connection = self.connect()?;
        let current: Option<(u32, Option<i64>)> = connection
            .query_row(
                "SELECT progress_percent, read_at FROM articles WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let (current_percent, read_at) =
            current.ok_or_else(|| Error::ReadingArticleNotFound(id.to_string()))?;
        if read_at.is_some() && current_percent == 100 {
            // Sticky completion: re-reading does not un-read.
            return self.summary(id);
        }
        let anchor = if percent == 0 || percent == 100 {
            None
        } else {
            anchor.map(str::trim).filter(|value| !value.is_empty())
        };
        let read_at = if percent == 100 { Some(now_ms()) } else { None };
        connection.execute(
            "UPDATE articles SET progress_percent = ?2, progress_anchor = ?3, read_at = ?4 WHERE id = ?1",
            params![id, percent, anchor, read_at],
        )?;
        drop(connection);
        self.summary(id)
    }

    /// Manual read / unread. Unread keeps the position so 继续阅读 still works.
    pub fn set_read(&self, id: &str, read: bool) -> Result<ReadingArticle> {
        let connection = self.connect()?;
        let changed = if read {
            connection.execute(
                "UPDATE articles SET progress_percent = 100, progress_anchor = NULL, read_at = ?2 WHERE id = ?1",
                params![id, now_ms()],
            )?
        } else {
            connection.execute(
                "UPDATE articles SET read_at = NULL,
                        progress_percent = CASE WHEN progress_percent = 100 THEN 0 ELSE progress_percent END
                 WHERE id = ?1",
                params![id],
            )?
        };
        if changed == 0 {
            return Err(Error::ReadingArticleNotFound(id.to_string()));
        }
        drop(connection);
        self.summary(id)
    }

    pub fn remove(&self, id: &str) -> Result<()> {
        let connection = self.connect()?;
        connection.execute("DELETE FROM annotations WHERE article_id = ?1", params![id])?;
        let changed = connection.execute("DELETE FROM articles WHERE id = ?1", params![id])?;
        if changed == 0 {
            return Err(Error::ReadingArticleNotFound(id.to_string()));
        }
        let _ = fs::remove_dir_all(self.image_dir(id));
        Ok(())
    }

    pub fn upsert_annotation(&self, input: ReadingAnnotationInput) -> Result<ReadingAnnotation> {
        if input.id.trim().is_empty() {
            return Err(Error::InvalidReadingAnnotation("缺少标记 ID".into()));
        }
        if input.end <= input.start {
            return Err(Error::InvalidReadingAnnotation("标记范围为空".into()));
        }
        if input.quote.trim().is_empty() {
            return Err(Error::InvalidReadingAnnotation("标记没有引用文字".into()));
        }
        let connection = self.connect()?;
        let exists: bool = connection.query_row(
            "SELECT COUNT(*) FROM articles WHERE id = ?1",
            params![input.article_id],
            |row| row.get::<_, i64>(0).map(|count| count > 0),
        )?;
        if !exists {
            return Err(Error::ReadingArticleNotFound(input.article_id));
        }
        let now = now_ms();
        connection.execute(
            "INSERT INTO annotations (id, article_id, quote, start, end, note, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
             ON CONFLICT(id) DO UPDATE SET quote = excluded.quote, start = excluded.start,
                 end = excluded.end, note = excluded.note, updated_at = excluded.updated_at",
            params![
                input.id,
                input.article_id,
                input.quote,
                input.start,
                input.end,
                input.note,
                now
            ],
        )?;
        connection.query_row(
            "SELECT id, article_id, quote, start, end, note, created_at, updated_at FROM annotations WHERE id = ?1",
            params![input.id],
            |row| {
                Ok(ReadingAnnotation {
                    id: row.get(0)?,
                    article_id: row.get(1)?,
                    quote: row.get(2)?,
                    start: row.get(3)?,
                    end: row.get(4)?,
                    note: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            },
        )
        .map_err(Error::from)
    }

    pub fn delete_annotation(&self, id: &str) -> Result<()> {
        let connection = self.connect()?;
        connection.execute("DELETE FROM annotations WHERE id = ?1", params![id])?;
        Ok(())
    }

    fn summary(&self, id: &str) -> Result<ReadingArticle> {
        Ok(self.get(id)?.article)
    }

    fn find_by_url(&self, url: &str) -> Result<Option<String>> {
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT id FROM articles WHERE url = ?1",
                params![url],
                |row| row.get(0),
            )
            .optional()
            .map_err(Error::from)
    }

    fn article_from_row(&self, row: &rusqlite::Row<'_>) -> rusqlite::Result<ReadingArticle> {
        let id: String = row.get(0)?;
        let cover_file: Option<String> = row.get(6)?;
        let cover_path = cover_file.map(|file| {
            self.image_dir(&id)
                .join(file)
                .to_string_lossy()
                .into_owned()
        });
        Ok(ReadingArticle {
            id,
            url: row.get(1)?,
            title: row.get(2)?,
            site_name: row.get(3)?,
            excerpt: row.get(4)?,
            byline: row.get(5)?,
            cover_path,
            word_count: row.get(7)?,
            reading_minutes: row.get(8)?,
            added_at: row.get(9)?,
            last_opened_at: row.get(10)?,
            progress_percent: row.get(11)?,
            progress_anchor: row.get(12)?,
            read_at: row.get(13)?,
            annotation_count: row.get::<_, i64>(14)? as u32,
            parse_status: row.get(15)?,
            parse_error: row.get(16)?,
        })
    }
}

// ── Fetching ────────────────────────────────────────────────────────────────

struct FetchedPage {
    html: String,
    final_url: String,
}

/// Only absolute web URLs are accepted; anything else could reach local files.
/// Best-effort display name for a URL that failed to parse.
fn host_of(url: &str) -> String {
    url.split("://")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or(url)
        .to_string()
}

/// Adds columns that postdate a user's existing database. `CREATE TABLE IF
/// NOT EXISTS` cannot evolve a live library; this checks `PRAGMA table_info`
/// and alters only what is missing, so upgrades are idempotent.
fn ensure_columns(connection: &Connection, table: &str, columns: &[(&str, &str)]) -> Result<()> {
    let existing: std::collections::HashSet<String> = {
        let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
        let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
        rows.flatten().collect()
    };
    for (name, ddl) in columns {
        if !existing.contains(*name) {
            connection.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {name} {ddl};"))?;
        }
    }
    Ok(())
}

fn normalize_url(url: &str) -> Result<String> {
    let trimmed = url.trim();
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return Err(Error::InvalidReadingUrl("只能添加 http/https 链接".into()));
    }
    // 多行粘贴被折叠成空格后，`Url::parse` 会把空格编码进路径，拼出一个永远
    // 抓不到的地址；这里直接拒掉，避免脏链接进库。
    if trimmed.chars().any(char::is_whitespace) {
        return Err(Error::InvalidReadingUrl(
            "链接里含有空格，可能是多条链接拼在了一起".into(),
        ));
    }
    let parsed = reqwest::Url::parse(trimmed)
        .map_err(|error| Error::InvalidReadingUrl(format!("链接无效：{error}")))?;
    if parsed.host_str().is_none() {
        return Err(Error::InvalidReadingUrl("链接缺少主机名".into()));
    }
    if parsed.path().contains("http://") || parsed.path().contains("https://") {
        return Err(Error::InvalidReadingUrl(
            "链接里含有多个地址，请分开添加".into(),
        ));
    }
    if url_is_private(&parsed) {
        return Err(Error::InvalidReadingUrl("不能抓取本机或内网地址".into()));
    }
    let mut normalized = parsed;
    normalized.set_fragment(None);
    Ok(normalized.to_string())
}

fn url_is_private(url: &reqwest::Url) -> bool {
    let host = match url.host_str() {
        Some(host) => host.trim_matches(['[', ']']),
        None => return true,
    };
    if host.eq_ignore_ascii_case("localhost")
        || host.ends_with(".localhost")
        || host.ends_with(".local")
    {
        return true;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => {
            ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.octets()[0] == 100 && (ip.octets()[1] & 0b1100_0000) == 0b0100_0000
        }
        Ok(std::net::IpAddr::V6(ip)) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
        }
        Err(_) => false,
    }
}

fn http_client() -> Result<reqwest::blocking::Client> {
    crate::http_client_builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 || url_is_private(attempt.url()) {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|error| Error::ReadingFetch(error.to_string()))
}

fn fetch_page(url: &str) -> Result<FetchedPage> {
    // X serves a JS-only shell to plain HTTP clients; the syndication API
    // returns the tweet as structured JSON without login.
    if let Some(tweet_id) = x_status_id(url) {
        if let Some(page) = fetch_x_tweet(&tweet_id, url) {
            return Ok(page);
        }
    }
    let client = http_client()?;
    let mut request = client
        .get(url)
        .header("Accept", "text/html,application/xhtml+xml,*/*;q=0.8")
        .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8");
    if is_wechat_article(url) {
        request = request
            .header(reqwest::header::USER_AGENT, WECHAT_USER_AGENT)
            .header(reqwest::header::REFERER, "https://mp.weixin.qq.com/");
    }
    let response = request
        .send()
        .map_err(|error| Error::ReadingFetch(fetch_failure_message(url, error)))?;
    let status = response.status();
    if !status.is_success() {
        return Err(Error::ReadingFetch(format!(
            "网页返回 HTTP {}",
            status.as_u16()
        )));
    }
    let final_url = response.url().to_string();
    let header_charset = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(charset_from_content_type);
    let bytes = response
        .bytes()
        .map_err(|error| Error::ReadingFetch(fetch_failure_message(url, error)))?;
    if bytes.len() > MAX_PAGE_BYTES {
        return Err(Error::ReadingFetch("网页过大，无法保存".into()));
    }
    let html = decode_html(&bytes, header_charset.as_deref());
    Ok(FetchedPage { html, final_url })
}

// ── X (Twitter) ─────────────────────────────────────────────────────────────

/// Tweet id when the URL points at a status on x.com / twitter.com.
fn x_status_id(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    let host_ok = matches!(
        host.as_str(),
        "x.com"
            | "www.x.com"
            | "mobile.x.com"
            | "twitter.com"
            | "www.twitter.com"
            | "mobile.twitter.com"
    );
    if !host_ok {
        return None;
    }
    let segments: Vec<&str> = parsed.path().split('/').filter(|s| !s.is_empty()).collect();
    match segments.as_slice() {
        [_, "status", id, ..] if id.chars().all(|c| c.is_ascii_digit()) => Some(id.to_string()),
        _ => None,
    }
}

/// Fetches a tweet as structured JSON and wraps it in a small self-describing
/// HTML document that `x_tweet_extract` parses back. FxTwitter goes first (it
/// also carries X Articles and quoted tweets); the syndication API is the
/// fallback. Returns `None` so the caller falls through to the ordinary page
/// fetch.
fn fetch_x_tweet(tweet_id: &str, page_url: &str) -> Option<FetchedPage> {
    let client = http_client().ok()?;
    let fetch_json = |url: &str| -> Option<serde_json::Value> {
        let response = client
            .get(url)
            .header("Accept", "application/json")
            .send()
            .ok()?;
        if !response.status().is_success() {
            return None;
        }
        response.json().ok()
    };
    let html = fetch_json(&format!("https://api.fxtwitter.com/i/status/{tweet_id}"))
        .and_then(|value| fx_tweet_html(&value))
        .or_else(|| {
            let value = fetch_json(&format!(
                "https://cdn.syndication.twimg.com/tweet-result?id={tweet_id}&token=x&lang=zh"
            ))?;
            x_tweet_html(&value)
        })?;
    Some(FetchedPage {
        html,
        final_url: page_url.to_string(),
    })
}

/// Builds the synthetic document from the syndication JSON. The marker
/// `id="x-tweet"` plus data attributes carry the metadata that readability
/// cannot see in a shell page.
fn x_tweet_html(value: &serde_json::Value) -> Option<String> {
    let author = value.get("user")?.get("name")?.as_str()?;
    let handle = value.get("user")?.get("screen_name")?.as_str()?;
    let mut text = value.get("text")?.as_str()?.to_string();
    if text.trim().is_empty() {
        return None;
    }
    // Media links are t.co placeholders pointing at the images below.
    if let Some(media) = value.pointer("/entities/media").and_then(|m| m.as_array()) {
        for item in media {
            if let Some(short) = item.get("url").and_then(|u| u.as_str()) {
                text = text.replace(short, "").trim_end().to_string();
            }
        }
    }
    let photos: Vec<String> = value
        .get("mediaDetails")
        .and_then(|m| m.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("media_url_https").and_then(|u| u.as_str()))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let mut body = text
        .split('\n')
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| format!("<p>{}</p>", escape_html(line)))
        .collect::<String>();
    body.push_str(&figures_html(&photos));
    let title = first_line(&text);
    Some(x_tweet_document(
        author,
        handle,
        &title,
        &body,
        photos.first().map(String::as_str),
    ))
}

/// Builds the synthetic document from the FxTwitter JSON, ported from the
/// x-downloader script: plain tweets render text plus photos, X Articles
/// render their Draft.js blocks, quoted tweets become a blockquote.
fn fx_tweet_html(value: &serde_json::Value) -> Option<String> {
    if value.get("code").and_then(serde_json::Value::as_i64) != Some(200) {
        return None;
    }
    let tweet = value.get("tweet")?;
    let author = tweet.pointer("/author/name").and_then(Value::as_str)?;
    let handle = tweet
        .pointer("/author/screen_name")
        .and_then(Value::as_str)?;
    if author.trim().is_empty() || handle.trim().is_empty() {
        return None;
    }
    let text = tweet.get("text").and_then(Value::as_str).unwrap_or("");
    let source_url = tweet
        .get("url")
        .and_then(Value::as_str)
        .filter(|url| url.starts_with("http"));
    let mut body = String::new();
    let mut cover: Option<String> = None;
    let mut title = first_line(text);

    let article = tweet.get("article").filter(|article| {
        article
            .pointer("/content/blocks")
            .and_then(Value::as_array)
            .is_some_and(|blocks| !blocks.is_empty())
    });
    if let Some(article) = article {
        title = article
            .get("title")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| first_line(text));
        cover = article
            .pointer("/cover_media/media_info/original_img_url")
            .and_then(Value::as_str)
            .map(str::to_string);
        body.push_str(&fx_article_html(article));
    } else {
        let photos = fx_photos(tweet);
        body.push_str(
            &text
                .split('\n')
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(|line| format!("<p>{}</p>", escape_html(line)))
                .collect::<String>(),
        );
        body.push_str(&figures_html(&photos));
        if cover.is_none() {
            cover = photos.first().cloned();
        }
        if fx_has_video(tweet) {
            body.push_str("<p>（该推文包含视频，请访问原文查看）</p>");
        }
    }

    if let Some(quote) = tweet.get("quote").filter(|q| q.is_object()) {
        let quote_handle = quote
            .pointer("/author/screen_name")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let quote_text = quote.get("text").and_then(Value::as_str).unwrap_or("");
        if !quote_text.trim().is_empty() {
            body.push_str(&format!(
                "<blockquote><p><a href=\"https://x.com/{quote_handle}\">@{quote_handle}</a>: {}</p></blockquote>",
                escape_html(quote_text)
            ));
        }
    }
    if let Some(url) = source_url {
        body.push_str(&format!(
            "<p><a href=\"{}\">查看原推</a></p>",
            escape_html(url)
        ));
    }
    Some(x_tweet_document(
        author,
        handle,
        &title,
        &body,
        cover.as_deref(),
    ))
}

fn fx_photos(tweet: &serde_json::Value) -> Vec<String> {
    tweet
        .pointer("/media/all")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| item.get("type").and_then(Value::as_str) == Some("photo"))
                .filter_map(|item| item.get("url").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn fx_has_video(tweet: &serde_json::Value) -> bool {
    tweet
        .pointer("/media/all")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items
                .iter()
                .any(|item| item.get("type").and_then(Value::as_str) == Some("video"))
        })
}

fn figures_html(urls: &[String]) -> String {
    urls.iter()
        .map(|url| format!("<figure><img src=\"{}\"></figure>", escape_html(url)))
        .collect()
}

/// Wraps a parsed tweet in the synthetic page: `<article id="x-tweet">` with
/// data attributes, plus `og:image` for the cover the reader can localize.
fn x_tweet_document(
    author: &str,
    handle: &str,
    title: &str,
    body: &str,
    cover: Option<&str>,
) -> String {
    let cover = cover
        .map(|url| {
            format!(
                "<meta property=\"og:image\" content=\"{}\">",
                escape_html(url)
            )
        })
        .unwrap_or_default();
    format!(
        "<html><head><title>{} (@{handle}) on X</title>{cover}</head>\
         <body><article id=\"x-tweet\" data-author=\"{}\" data-handle=\"{handle}\" data-title=\"{}\">\
         {body}</article></body></html>",
        escape_html(title),
        escape_html_attr(author),
        escape_html_attr(title),
    )
}

/// Converts an X Article's Draft.js blocks to HTML (the x-downloader
/// translation, targeting HTML instead of Markdown). Offsets are applied on
/// `char`s, so astral-plane characters (rare emoji) may shift styling by one.
fn fx_article_html(article: &serde_json::Value) -> String {
    let mut entities = std::collections::HashMap::new();
    if let Some(list) = article
        .pointer("/content/entityMap")
        .and_then(Value::as_array)
    {
        for item in list {
            let key = match item.get("key") {
                Some(serde_json::Value::String(key)) => key.clone(),
                Some(other) => other.to_string(),
                None => continue,
            };
            if let Some(entity) = item.get("value") {
                entities.insert(key, entity.clone());
            }
        }
    }
    // media_id -> image URL, used by MEDIA entities inside articles.
    let mut media_urls = std::collections::HashMap::new();
    if let Some(list) = article.get("media_entities").and_then(Value::as_array) {
        for item in list {
            if let (Some(id), Some(url)) = (
                item.get("media_id").map(json_key),
                item.pointer("/media_info/original_img_url")
                    .and_then(Value::as_str),
            ) {
                media_urls.insert(id, url.to_string());
            }
        }
    }
    let mut out = String::new();
    let mut list_tag: Option<&str> = None;
    let blocks = article
        .pointer("/content/blocks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for block in &blocks {
        let block_type = block
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("unstyled");
        let text = block.get("text").and_then(Value::as_str).unwrap_or("");
        let styled = styled_block_html(
            text,
            block.get("entityRanges").and_then(Value::as_array),
            block.get("inlineStyleRanges").and_then(Value::as_array),
            &entities,
        );
        let is_list = matches!(block_type, "ordered-list-item" | "unordered-list-item");
        if !is_list {
            if let Some(tag) = list_tag.take() {
                out.push_str(&format!("</{tag}>"));
            }
        }
        match block_type {
            "header-one" => out.push_str(&format!("<h1>{styled}</h1>")),
            "header-two" => out.push_str(&format!("<h2>{styled}</h2>")),
            "header-three" => out.push_str(&format!("<h3>{styled}</h3>")),
            "ordered-list-item" => {
                if list_tag != Some("ol") {
                    if let Some(tag) = list_tag.take() {
                        out.push_str(&format!("</{tag}>"));
                    }
                    out.push_str("<ol>");
                    list_tag = Some("ol");
                }
                out.push_str(&format!("<li>{styled}</li>"));
            }
            "unordered-list-item" => {
                if list_tag != Some("ul") {
                    if let Some(tag) = list_tag.take() {
                        out.push_str(&format!("</{tag}>"));
                    }
                    out.push_str("<ul>");
                    list_tag = Some("ul");
                }
                out.push_str(&format!("<li>{styled}</li>"));
            }
            "blockquote" => out.push_str(&format!("<blockquote><p>{styled}</p></blockquote>")),
            "atomic" => {
                if let Some(fragment) = fx_atomic_html(block, &entities, &media_urls) {
                    out.push_str(&fragment);
                }
            }
            _ => {
                if !styled.trim().is_empty() {
                    out.push_str(&format!("<p>{styled}</p>"));
                }
            }
        }
    }
    if let Some(tag) = list_tag.take() {
        out.push_str(&format!("</{tag}>"));
    }
    out
}

/// Renders one atomic block: MARKDOWN entities become code blocks, IMAGE and
/// MEDIA entities become figures pointing at the image URL (localization
/// happens later in the shared image pipeline).
fn fx_atomic_html(
    block: &serde_json::Value,
    entities: &std::collections::HashMap<String, serde_json::Value>,
    media_urls: &std::collections::HashMap<String, String>,
) -> Option<String> {
    let key = block.pointer("/entityRanges/0/key").map(json_key)?;
    let entity = entities.get(&key)?;
    let entity_type = entity.get("type").and_then(Value::as_str)?;
    let data = entity.get("data")?;
    match entity_type {
        "MARKDOWN" => {
            let markdown = data.get("markdown").and_then(Value::as_str)?;
            Some(format!(
                "<pre><code>{}</code></pre>",
                escape_html(markdown.trim())
            ))
        }
        "IMAGE" => {
            let url = data
                .get("src")
                .or_else(|| data.get("url"))
                .and_then(Value::as_str)?;
            Some(format!(
                "<figure><img src=\"{}\"></figure>",
                escape_html(url)
            ))
        }
        "MEDIA" => {
            let url = data
                .pointer("/mediaItems/0/mediaId")
                .and_then(Value::as_str)
                .and_then(|id| media_urls.get(id))
                .map(String::as_str)
                .or_else(|| {
                    data.pointer("/media_info/original_img_url")
                        .and_then(Value::as_str)
                })?;
            Some(format!(
                "<figure><img src=\"{}\"></figure>",
                escape_html(url)
            ))
        }
        _ => None,
    }
}

/// Applies Draft.js LINK entities and Bold/Italic inline styles to a block.
/// Ranges may overlap (bold inside a link); they nest by position, so the
/// walk opens at each index before emitting the escaped character.
fn styled_block_html(
    text: &str,
    entity_ranges: Option<&Vec<serde_json::Value>>,
    inline_styles: Option<&Vec<serde_json::Value>>,
    entities: &std::collections::HashMap<String, serde_json::Value>,
) -> String {
    enum Span {
        Bold,
        Italic,
        Link(String),
    }
    let chars: Vec<char> = text.chars().collect();
    let mut spans: Vec<(usize, usize, Span)> = Vec::new();
    for range in entity_ranges.into_iter().flatten() {
        let (offset, length) = span_bounds(range, chars.len());
        if length == 0 {
            continue;
        }
        let url = range
            .get("key")
            .map(json_key)
            .and_then(|key| entities.get(&key))
            .and_then(|entity| entity.get("data"))
            .and_then(|data| data.get("url"))
            .and_then(Value::as_str)
            .unwrap_or("");
        if url.is_empty() {
            continue;
        }
        spans.push((offset, offset + length, Span::Link(url.to_string())));
    }
    for style in inline_styles.into_iter().flatten() {
        let (offset, length) = span_bounds(style, chars.len());
        if length == 0 {
            continue;
        }
        match style.get("style").and_then(Value::as_str) {
            Some("Bold") => spans.push((offset, offset + length, Span::Bold)),
            Some("Italic") => spans.push((offset, offset + length, Span::Italic)),
            _ => {}
        }
    }
    spans.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
    let mut open: Vec<(usize, &Span)> = Vec::new();
    let mut out = String::new();
    let mut next_span = 0;
    for index in 0..=chars.len() {
        while let Some((end, _)) = open.last() {
            if *end == index {
                let (_, span) = open.pop().unwrap();
                out.push_str(match span {
                    Span::Bold => "</strong>",
                    Span::Italic => "</em>",
                    Span::Link(_) => "</a>",
                });
            } else {
                break;
            }
        }
        while next_span < spans.len() && spans[next_span].0 == index {
            let (_, end, span) = &spans[next_span];
            let open_tag = match span {
                Span::Bold => "<strong>".to_string(),
                Span::Italic => "<em>".to_string(),
                Span::Link(url) => format!("<a href=\"{}\">", escape_html_attr(url)),
            };
            out.push_str(&open_tag);
            open.push((*end, span));
            next_span += 1;
        }
        if index < chars.len() {
            out.push_str(&escape_char(chars[index]));
        }
    }
    out
}

fn span_bounds(value: &serde_json::Value, max: usize) -> (usize, usize) {
    let offset = value.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
    let length = value.get("length").and_then(Value::as_u64).unwrap_or(0) as usize;
    let length = length.min(max.saturating_sub(offset));
    (offset, length)
}

fn escape_char(ch: char) -> String {
    match ch {
        '&' => "&amp;".into(),
        '<' => "&lt;".into(),
        '>' => "&gt;".into(),
        other => other.to_string(),
    }
}

/// Draft.js entity keys appear both as JSON strings and numbers; this maps
/// either shape to the same string (`Value::to_string` would quote strings).
fn json_key(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(key) => key.clone(),
        other => other.to_string(),
    }
}

/// Parses the synthetic tweet document back into an `Extracted`, bypassing
/// readability (which has nothing to score in it). The body is generic HTML
/// (paragraphs, headings, lists, blockquotes, code blocks, figures) emitted by
/// either tweet builder, so it only needs sanitizing.
fn x_tweet_extract(html: &str) -> Option<Extracted> {
    let marker = Regex::new(r#"(?is)<article\s+id="x-tweet"\s+([^>]*?)>(.*?)</article>"#).ok()?;
    let captures = marker.captures(html)?;
    let attributes = &captures[1];
    let body = &captures[2];
    let attribute = |name: &str| -> Option<String> {
        Regex::new(&format!(r#"{name}="([^"]*)""#))
            .ok()?
            .captures(attributes)
            .map(|found| decode_entities(&found[1]))
    };
    let author = attribute("data-author").unwrap_or_default();
    let handle = attribute("data-handle").unwrap_or_default();
    if body.trim().is_empty() {
        return None;
    }
    let content_html = sanitize_html(body);
    let text = collapse_whitespace(&html_to_text(body));
    let (word_count, reading_minutes) = reading_stats(&text);
    let title = attribute("data-title")
        .filter(|title| !title.trim().is_empty())
        .or_else(|| Some(first_line(&text)))
        .unwrap_or_else(|| format!("@{handle}"));
    Some(Extracted {
        title: truncate_chars(&title, 60),
        // Like WeChat, the reader recognises the author, not the platform.
        site_name: author,
        excerpt: truncate_chars(&text, 160),
        byline: Some(format!("@{handle}")),
        cover_url: og_image(html),
        content_html,
        markdown: None,
        word_count,
        reading_minutes,
    })
}

fn first_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or(text)
        .to_string()
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_html_attr(value: &str) -> String {
    escape_html(value).replace('"', "&quot;")
}

fn classify_fetch_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "抓取超时".into()
    } else if error.is_connect() {
        "无法连接到该网站".into()
    } else {
        error.to_string()
    }
}

/// Adds a network-environment hint for sites that usually need a proxy,
/// instead of leaving the user staring at a bare "抓取超时".
fn fetch_failure_message(url: &str, error: reqwest::Error) -> String {
    let message = classify_fetch_error(error);
    if x_status_id(url).is_some() {
        return format!(
            "解析 X（推特）链接失败：{message}。x.com 在部分网络下无法直连，请检查当前网络环境，例如开启全局代理后重试。"
        );
    }
    message
}

fn charset_from_content_type(value: &str) -> Option<String> {
    value
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix("charset="))
        .map(|charset| charset.trim_matches('"').to_ascii_lowercase())
}

/// Decodes with the declared charset, sniffing `<meta charset>` when the
/// header says nothing. Chinese sites still ship GBK often enough to matter.
fn decode_html(bytes: &[u8], header_charset: Option<&str>) -> String {
    let sniffed = if header_charset.is_none() {
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).to_ascii_lowercase();
        Regex::new(r#"charset\s*=\s*["']?\s*([a-z0-9_\-]+)"#)
            .ok()
            .and_then(|pattern| pattern.captures(&head))
            .and_then(|captures| captures.get(1).map(|m| m.as_str().to_string()))
    } else {
        None
    };
    let label = header_charset.map(str::to_string).or(sniffed);
    let encoding = label
        .as_deref()
        .and_then(|name| encoding_rs::Encoding::for_label(name.as_bytes()))
        .unwrap_or(encoding_rs::UTF_8);
    let (decoded, _, _) = encoding.decode(bytes);
    decoded.into_owned()
}

// ── Extraction ──────────────────────────────────────────────────────────────

struct Extracted {
    title: String,
    site_name: String,
    excerpt: String,
    byline: Option<String>,
    cover_url: Option<String>,
    content_html: String,
    markdown: Option<String>,
    word_count: u32,
    reading_minutes: u32,
}

fn extract(page: &FetchedPage, requested_url: &str) -> Result<Extracted> {
    let html = prepare_html(&page.html);
    let base_url = if page.final_url.is_empty() {
        requested_url
    } else {
        &page.final_url
    };
    let mut builder = readabilityrs::ReadabilityOptions::builder()
        .output_markdown(true)
        .sanitize_content(true);
    if is_wechat_article(requested_url) || is_wechat_article(&page.final_url) {
        // 微信的视觉设计（卡片边框、装饰标题等）全部写在 style 属性上；
        // readability 默认的 clean_styles 会把它删光。sanitize_html 会再做
        // 一次声明级过滤，这里放心保留。
        builder = builder.clean_styles(false);
    }
    let options = builder.build();
    if let Some(tweet) = x_tweet_extract(&html) {
        return Ok(tweet);
    }
    if is_wechat_error_shell(&html) {
        return Err(Error::ReadingFetch(
            "微信返回了参数错误页，请在微信或浏览器中打开后使用“保存到阅读”".into(),
        ));
    }
    let article = readabilityrs::Readability::new(&html, Some(base_url), Some(options))
        .ok()
        .and_then(|parser| parser.parse());
    let fallback = wechat_content_fallback(&html);
    // 微信正文容器是固定的 #js_content，直接取容器：readability 的候选评分
    // 在微信这种 section 重嵌套 + 图多的排版上会选错根节点（丢整块图片、
    // 或选到作者卡片区）。fallback 缺失时（视频/图片消息页）退回原链路。
    let wechat = is_wechat_article(requested_url) || is_wechat_article(&page.final_url);
    let used_fallback = wechat && fallback.is_some();
    let raw_content = if used_fallback {
        fallback.clone()
    } else {
        article
            .as_ref()
            .and_then(|item| item.content.clone())
            .filter(|content| !content.trim().is_empty())
            .or_else(|| fallback.clone())
            // 只有元数据的页面（微信视频/图片消息等）也保留下来，而不是判失败。
            .or_else(|| link_only_body(&html))
    }
    .ok_or_else(|| Error::ReadingExtract("没有找到可阅读的正文".into()))?;
    let title_from_article = article.as_ref().and_then(|item| item.title.clone());
    let standardized =
        readabilityrs::elements::standardize_all(&raw_content, title_from_article.as_deref());
    let mut content_html = sanitize_html(&standardized);
    if used_fallback {
        // 作者头（原创/作者/公众号/发布时间）插在正文最前。
        if let Some(header) = wechat_article_header(&html) {
            content_html = format!("{header}{content_html}");
        }
    }
    // 走 js_content 容器时，readability 的 text/markdown 属于它自己选的节点，
    // 与本文不对应，直接从容器内容推导。
    let text = if used_fallback {
        html_to_text(&raw_content)
    } else {
        article
            .as_ref()
            .and_then(|item| item.text_content.clone())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| html_to_text(&raw_content))
    };
    let (word_count, reading_minutes) = reading_stats(&text);
    let host = reqwest::Url::parse(base_url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_string))
        .unwrap_or_default();
    let byline = article
        .as_ref()
        .and_then(|item| item.byline.clone())
        .map(|byline| collapse_whitespace(&byline))
        .filter(|value| !value.is_empty());
    // WeChat labels every article "微信公众平台"; the account name is what
    // the reader recognises.
    let mut site_name = if host == "mp.weixin.qq.com" && byline.is_some() {
        byline.clone().unwrap_or_default()
    } else {
        article
            .as_ref()
            .and_then(|item| item.site_name.clone())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| host.clone())
    };
    let mut byline = byline;
    if used_fallback {
        // 页面变量里是准确的作者（var author）与公众号名（var nickname）。
        // 旧逻辑把 byline（作者名）当成了 site_name，公众号名整个丢失。
        if let Some(author) = wechat_page_var(&html, "author") {
            byline = Some(author);
        }
        if let Some(nickname) = wechat_nickname(&html) {
            site_name = nickname;
        }
    }
    let title = article
        .as_ref()
        .and_then(|item| item.title.clone())
        .map(|title| title.trim().to_string())
        .filter(|title| !title.is_empty())
        .or_else(|| meta_content(&html, "og:title"))
        .or_else(|| html_title(&html))
        .unwrap_or_else(|| host.clone());
    let excerpt = article
        .as_ref()
        .and_then(|item| item.excerpt.clone())
        .map(|excerpt| collapse_whitespace(&excerpt))
        .filter(|excerpt| !excerpt.is_empty())
        .or_else(|| meta_content(&html, "og:description"))
        .unwrap_or_else(|| first_sentence(&text));
    Ok(Extracted {
        title,
        site_name,
        excerpt: truncate_chars(&excerpt, 160),
        byline,
        cover_url: article
            .as_ref()
            .and_then(|item| item.image.clone())
            .or_else(|| og_image(&html)),
        content_html,
        markdown: if used_fallback {
            None
        } else {
            article
                .as_ref()
                .and_then(|item| item.markdown_content.clone())
                .filter(|value| !value.trim().is_empty())
        },
        word_count,
        reading_minutes,
    })
}

/// 微信正文页（`mp.weixin.qq.com/s/...`）走一套更像内置浏览器的请求头。
fn is_wechat_article(url: &str) -> bool {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(|host| host == "mp.weixin.qq.com"))
        .unwrap_or(false)
}

fn is_wechat_error_shell(html: &str) -> bool {
    html.contains("weui-msg__title warn")
        && (html.contains("参数错误") || html.contains("链接已过期") || html.contains("内容不存在"))
}

fn wechat_content_fallback(html: &str) -> Option<String> {
    if !html.contains("mp.weixin.qq.com")
        && !html.contains("id=\"js_content\"")
        && !html.contains("id='js_content'")
    {
        return None;
    }
    // 逐个扫描 <div 开始标签，找到正文容器（js_content / rich_media_content），
    // 再按嵌套深度配对取内层——非贪婪正则会在第一个 </div> 处截断，
    // 而微信正文里嵌着大量 div/section。
    let mut cursor = 0;
    while let Some(offset) = html[cursor..].find("<div") {
        let open_start = cursor + offset;
        let Some(open_end) = html[open_start..].find('>') else {
            break;
        };
        let open_end = open_start + open_end;
        let open_tag = &html[open_start..=open_end];
        let is_container = open_tag.contains("id=\"js_content\"")
            || open_tag.contains("id='js_content'")
            || open_tag.contains("rich_media_content");
        cursor = open_end + 1;
        if !is_container {
            continue;
        }
        let inner = balanced_div_inner(html, open_end + 1)?;
        let body = inner.trim();
        return (html_to_text(body).chars().count() >= 20).then(|| body.to_string());
    }
    None
}

/// 公众号资料卡：正文中内嵌的 `<mp-common-profile>` 组件由 JS 渲染，抓取时
/// 只是空标签，但属性里带了全部数据（昵称/头像/简介/原创数/认证状态）。
/// 原地转换成纯展示卡（无 `<a>`，不受阅读器链接样式影响），位置与原文一致；
/// 内联样式里写全 margin，避免被阅读器的图片居中规则挪位。
fn render_wechat_profile_cards(html: &str) -> String {
    let Ok(tag) = Regex::new(r#"(?is)<mp-common-profile\b[^>]*>\s*</mp-common-profile>"#) else {
        return html.to_string();
    };
    let attribute = |source: &str, name: &str| -> Option<String> {
        Regex::new(&format!(r#"data-{name}="([^"]*)""#))
            .ok()?
            .captures(source)
            .and_then(|captures| captures.get(1))
            .map(|matched| decode_entities(matched.as_str()))
            .filter(|value| !value.trim().is_empty())
    };
    tag.replace_all(html, |captures: &regex::Captures<'_>| {
        let source = &captures[0];
        let Some(nickname) = attribute(source, "nickname") else {
            // 数据不全的组件没有可展示的内容，直接丢弃。
            return String::new();
        };
        let avatar = attribute(source, "headimg").map(|url| {
            format!(
                r#"<img src="{}" alt="" style="width: 48px; height: 48px; border-radius: 50%; margin: 2px 14px 0 0; flex: none">"#,
                html_escape(&url)
            )
        });
        let verified = attribute(source, "verify_status")
            .map(|status| status != "0")
            .unwrap_or(false);
        let badge = if verified {
            r#"<span style="display: inline-block; margin-left: 4px; width: 15px; height: 15px; line-height: 15px; border-radius: 50%; background: #4874ff; color: #fff; font-size: 11px; text-align: center; vertical-align: 1px">✓</span>"#
        } else {
            ""
        };
        let signature = attribute(source, "signature")
            .map(|value| {
                format!(
                    r#"<div style="color: #444; font-size: 14px; line-height: 1.6; margin-top: 6px">{}</div>"#,
                    html_escape(&value)
                )
            })
            .unwrap_or_default();
        let origin = attribute(source, "origin_num").and_then(|value| value.parse::<u32>().ok())
            .filter(|count| *count > 0)
            .map(|count| {
                format!(
                    r#"<div style="color: #b2b2b2; font-size: 13px; margin-top: 6px">{count}篇原创内容</div>"#
                )
            })
            .unwrap_or_default();
        format!(
            r#"<div style="margin: 0 0 8px; padding: 16px 20px; border: 1px solid #e7e7e7; border-radius: 8px; background: #fff; max-width: 100%; box-sizing: border-box"><div style="display: flex; align-items: flex-start">{}<div style="min-width: 0; flex: 1"><div style="font-weight: 600; color: #1a1a1a; font-size: 17px; line-height: 1.4">{}{badge}</div>{signature}{origin}</div></div><div style="border-top: 1px solid #f0f0f0; margin-top: 14px; padding-top: 10px; color: #b2b2b2; font-size: 13px">公众号</div></div>"#,
            avatar.unwrap_or_default(),
            html_escape(&nickname),
        )
    })
    .into_owned()
}

/// 微信页面变量（`var author = "徐泓镔";` / `var createTime = '...'`）。
fn wechat_page_var(html: &str, name: &str) -> Option<String> {
    Regex::new(&format!(r#"var\s+{name}\s*=\s*['"]([^'"]*)['"]\s*;"#))
        .ok()?
        .captures(html)
        .and_then(|captures| captures.get(1))
        .map(|matched| decode_entities(matched.as_str()))
        .filter(|value| !value.trim().is_empty())
}

/// 公众号名：新版页面写 `var nickname = htmlDecode("...")`，旧版直接赋值。
fn wechat_nickname(html: &str) -> Option<String> {
    wechat_page_var(html, "nickname").or_else(|| {
        Regex::new(r#"var\s+nickname\s*=\s*htmlDecode\("([^"]*)"\)"#)
            .ok()?
            .captures(html)
            .and_then(|captures| captures.get(1))
            .map(|matched| collapse_whitespace(&decode_entities(matched.as_str())))
            .filter(|value| !value.is_empty())
    })
}

/// 作者头：原版文章顶部的一行——原创标记、作者、公众号、发布时间。阅读数与
/// 地区由 JS 实时取，静态页没有，不显示。数据全无时返回 None。
fn wechat_article_header(html: &str) -> Option<String> {
    let nickname = wechat_nickname(html);
    let author = wechat_page_var(html, "author");
    let create_time = wechat_page_var(html, "createTime");
    let is_original = Regex::new(r#"copyright_stat[^0-9]*'(\d)'"#)
        .ok()?
        .captures(html)
        .and_then(|captures| captures.get(1))
        .map(|matched| matched.as_str() == "1")
        .unwrap_or(false);
    if author.is_none() && nickname.is_none() && create_time.is_none() {
        return None;
    }
    let original_badge = if is_original {
        r#"<span style="background: #f0a032; color: #fff; font-size: 12px; padding: 1px 6px; border-radius: 3px; flex: none">原创</span>"#
    } else {
        ""
    };
    let part = |value: &Option<String>, color: &str| {
        value
            .as_deref()
            .map(|text| {
                format!(
                    r#"<span style="color: {color}; overflow: hidden; text-overflow: ellipsis; white-space: nowrap">{}</span>"#,
                    html_escape(text)
                )
            })
            .unwrap_or_default()
    };
    Some(format!(
        r#"<div style="margin: 0 8px 20px; display: flex; align-items: center; flex-wrap: wrap; gap: 10px; font-size: 15px">{original_badge}{}{}{}</div>"#,
        part(&author, "#333"),
        part(&nickname, "#999"),
        part(&create_time, "#999"),
    ))
}

/// 从 `open_end`（`<div ...>` 的 `>` 之后）开始，按 `<div` / `</div>` 的嵌套
/// 深度找到配对的闭合标签，返回两者之间的 HTML。未闭合时取到文档末尾兜底。
fn balanced_div_inner(html: &str, open_end: usize) -> Option<String> {
    let mut depth = 1usize;
    let mut cursor = open_end;
    let boundary = |byte: u8| byte.is_ascii_whitespace() || byte == b'>' || byte == b'/';
    while depth > 0 {
        let rest = &html[cursor..];
        let next_open = rest.find("<div").filter(|offset| {
            rest[*offset + 4..]
                .bytes()
                .next()
                .map(boundary)
                .unwrap_or(false)
        });
        let next_close = rest.find("</div").filter(|offset| {
            rest[*offset + 5..]
                .bytes()
                .next()
                .map(boundary)
                .unwrap_or(false)
        });
        match (next_open, next_close) {
            // 有未配对的 <div 就先吃掉它的开始标签；否则消耗一个闭合标签。
            (Some(open), None) => {
                let tag_end = rest[open..].find('>')? + open + 1;
                depth += 1;
                cursor += tag_end;
            }
            (Some(open), Some(close)) if open < close => {
                let tag_end = rest[open..].find('>')? + open + 1;
                depth += 1;
                cursor += tag_end;
            }
            (_, Some(close)) => {
                depth -= 1;
                if depth == 0 {
                    return Some(html[open_end..cursor + close].to_string());
                }
                cursor += rest[close..].find('>')? + close + 1;
            }
            (None, None) => return Some(html[open_end..].to_string()),
        }
    }
    None
}

fn html_to_text(html: &str) -> String {
    let without_tags = Regex::new(r"(?is)<[^>]+>")
        .map(|pattern| pattern.replace_all(html, " ").into_owned())
        .unwrap_or_else(|_| html.to_string());
    collapse_whitespace(&decode_entities(&without_tags))
}

/// Site quirks that Readability cannot know about. WeChat hides the body
/// behind `visibility:hidden` and lazy-loads every image through `data-src`;
/// lifting those here keeps the extractor generic.
fn prepare_html(html: &str) -> String {
    let mut result = html.to_string();
    if let (Ok(tag), Ok(lazy), Ok(src)) = (
        Regex::new(r#"(?is)<img\b[^>]*>"#),
        Regex::new(r#"(?i)\bdata-src\s*=\s*("[^"]*"|'[^']*')"#),
        Regex::new(r#"(?i)\ssrc\s*=\s*("[^"]*"|'[^']*'|[^\s>]+)"#),
    ) {
        result = tag
            .replace_all(&result, |captures: &regex::Captures<'_>| {
                let element = &captures[0];
                let Some(value) = lazy.captures(element).map(|found| found[1].to_string()) else {
                    return element.to_string();
                };
                // Drop any placeholder src so the real one wins.
                let without_src = src.replace_all(element, "").into_owned();
                without_src.replacen("<img", &format!("<img src={value}"), 1)
            })
            .into_owned();
    }
    // 隐藏样式的兜底：微信正文在 JS 淡入前靠 `visibility:hidden` / `opacity:0`
    // 藏着，抓取时 JS 不会执行。边界（\s;"' 或结尾）只匹配不吞掉，因此
    // `opacity: 0.5` 这类半透明装饰不会被误伤。
    if let Ok(hidden) =
        Regex::new(r#"(?i)(visibility\s*:\s*hidden|opacity\s*:\s*0+(\.0*)?)([\s;"']|$)"#)
    {
        result = hidden
            .replace_all(&result, |captures: &regex::Captures<'_>| {
                captures[3].to_string()
            })
            .into_owned();
    }
    render_wechat_profile_cards(&result)
}

pub(crate) fn sanitize_html(html: &str) -> String {
    let mut builder = ammonia::Builder::default();
    builder
        .add_tags([
            "figure",
            "figcaption",
            "section",
            "article",
            "mark",
            "video",
            "source",
            "audio",
        ])
        .add_tag_attributes("img", ["width", "height", "loading"])
        .add_tag_attributes("video", ["src", "poster", "controls"])
        .add_tag_attributes("audio", ["src", "controls"])
        .add_tag_attributes("source", ["src", "type"])
        .rm_tag_attributes("a", ["target"])
        // 微信等站点的视觉设计全靠内联 style；放行后逐条声明过滤，
        // 危险构造（url()/expression()/position:fixed 等）不会出现在结果里。
        .add_generic_attributes(["style"])
        .attribute_filter(|_element, attribute, value| {
            if attribute.eq_ignore_ascii_case("style") {
                Some(std::borrow::Cow::Owned(sanitize_style(value)))
            } else {
                Some(std::borrow::Cow::Borrowed(value))
            }
        })
        .link_rel(Some("noopener noreferrer"))
        .url_relative(ammonia::UrlRelative::PassThrough);
    builder.clean(html).to_string()
}

/// 过滤内联 style 中的单条声明。ammonia 4 的 CSS 过滤器（`style::filter_style_attribute`）
/// 是私有的，这里做一个等价的声明级白名单：`url()` / `expression()` / `javascript:`
/// 是 XSS 与外链通道，`position:fixed` 可做覆盖层钓鱼，含这些构造的声明整条丢弃，
/// 其余（边框、背景色、字号等纯样式）保留。
fn sanitize_style(style: &str) -> String {
    style
        .split(';')
        .map(|declaration| declaration.trim())
        .filter(|declaration| {
            if declaration.is_empty() {
                return false;
            }
            let lower = declaration.to_ascii_lowercase();
            if lower.starts_with('@')
                || lower.contains("url(")
                || lower.contains("expression(")
                || lower.contains("javascript:")
                || lower.contains("-moz-binding")
            {
                return false;
            }
            // `position: fixed` 无论空格怎么写都拦掉。
            let mut parts = lower.splitn(2, ':');
            let property = parts.next().unwrap_or_default().trim();
            let value = parts.next().unwrap_or_default().trim();
            !(property == "position" && value.split_whitespace().any(|word| word == "fixed"))
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn html_title(html: &str) -> Option<String> {
    Regex::new(r"(?is)<title[^>]*>(.*?)</title>")
        .ok()?
        .captures(html)
        .and_then(|captures| captures.get(1))
        .map(|m| collapse_whitespace(&decode_entities(m.as_str())))
        .filter(|title| !title.is_empty())
}

/// Reads `og:*` / `twitter:*` meta content in either attribute order.
fn meta_content(html: &str, key: &str) -> Option<String> {
    let first = Regex::new(&format!(
        r#"(?is)<meta\b[^>]*?(?:property|name)\s*=\s*["']{}["'][^>]*?content\s*=\s*["']([^"']*)["']"#,
        regex::escape(key)
    ))
    .ok()?;
    let second = Regex::new(&format!(
        r#"(?is)<meta\b[^>]*?content\s*=\s*["']([^"']*)["'][^>]*?(?:property|name)\s*=\s*["']{}["']"#,
        regex::escape(key)
    ))
    .ok()?;
    first
        .captures(html)
        .or_else(|| second.captures(html))
        .and_then(|captures| captures.get(1))
        .map(|m| collapse_whitespace(&decode_entities(m.as_str())))
        .filter(|value| !value.is_empty())
}

/// 只有元数据的页面（微信视频/图片消息、部分分享页）没有可提取正文。
/// 只要页面给了 og:title 就当作「已收藏」保存，正文位置放一段说明，
/// 而不是把整条链接标成解析失败。
fn link_only_body(html: &str) -> Option<String> {
    meta_content(html, "og:title")?;
    let mut body = String::from(
        "<p>这篇内容没有可提取的正文（可能是视频、图片消息或分享页）。可点「原文」继续阅读。</p>",
    );
    if let Some(description) = meta_content(html, "og:description") {
        body.push_str(&format!("<p>{}</p>", escape_html(&description)));
    }
    Some(body)
}

fn og_image(html: &str) -> Option<String> {
    let pattern = Regex::new(
        r#"(?is)<meta\b[^>]*?(?:property|name)\s*=\s*["'](?:og:image|twitter:image)["'][^>]*?content\s*=\s*["']([^"']+)["']"#,
    )
    .ok()?;
    pattern
        .captures(html)
        .or_else(|| {
            Regex::new(
                r#"(?is)<meta\b[^>]*?content\s*=\s*["']([^"']+)["'][^>]*?(?:property|name)\s*=\s*["'](?:og:image|twitter:image)["']"#,
            )
            .ok()?
            .captures(html)
        })
        .and_then(|captures| captures.get(1))
        .map(|m| decode_entities(m.as_str()))
}

fn decode_entities(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn first_sentence(text: &str) -> String {
    let collapsed = collapse_whitespace(text);
    let end = collapsed
        .char_indices()
        .find(|(_, ch)| matches!(ch, '。' | '！' | '？' | '.' | '!' | '?'))
        .map(|(index, ch)| index + ch.len_utf8())
        .unwrap_or(collapsed.len());
    collapsed[..end].to_string()
}

fn truncate_chars(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    let mut result: String = value.chars().take(max).collect();
    result.push('…');
    result
}

/// CJK reads by character, everything else by word. 400 字/分钟 and 230 wpm
/// are the usual figures; the sum is rounded up so a short note is 1 minute.
pub fn reading_stats(text: &str) -> (u32, u32) {
    let mut cjk = 0u32;
    let mut latin_words = 0u32;
    let mut in_word = false;
    for ch in text.chars() {
        let is_cjk = matches!(ch as u32,
            0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0xAC00..=0xD7AF | 0x20000..=0x2FA1F);
        if is_cjk {
            cjk += 1;
            in_word = false;
        } else if ch.is_alphanumeric() {
            if !in_word {
                latin_words += 1;
                in_word = true;
            }
        } else {
            in_word = false;
        }
    }
    let words = cjk + latin_words;
    let minutes = (f64::from(cjk) / 400.0 + f64::from(latin_words) / 230.0).ceil() as u32;
    (words, minutes.max(if words > 0 { 1 } else { 0 }))
}

// ── Export as HTML / Markdown ───────────────────────────────────────────────

const MAX_EXPORT_STEM_CHARS: usize = 80;

/// 把 HTML 与 Markdown 里的图片引用统一落地到 `img_dir`：本地 `images/<file>`
/// 直接从 `image_dir` 复制；本地化失败时残留的远程 http(s) URL 尽力下载，
/// 失败保留原 URL。返回处理后的正文、Markdown 与保存的图片数。
fn export_with_local_images(
    html: &str,
    markdown: Option<&str>,
    image_dir: &Path,
    img_dir: &Path,
    page_url: &str,
) -> (String, Option<String>, u32) {
    let mut mapping: HashMap<String, String> = HashMap::new();
    let mut next_index = 1u32;
    let client = http_client().ok();
    let mut resolve = |source: &str| {
        resolve_export_image(
            source,
            image_dir,
            img_dir,
            page_url,
            client.as_ref(),
            &mut mapping,
            &mut next_index,
        )
    };
    let exported_html = rewrite_export_html_images(html, &mut resolve);
    let exported_markdown = match markdown {
        Some(markdown) => Some(rewrite_export_markdown_images(markdown, &mut resolve)),
        // 存储层没有 Markdown（微信 fallback 路径）时从重写后的 HTML 现转，
        // 图片引用已经是 img/ 相对路径，不用再处理。
        None => {
            Some(htmd::convert(&exported_html).unwrap_or_else(|_| html_to_text(&exported_html)))
        }
    };
    (exported_html, exported_markdown, mapping.len() as u32)
}

type ExportResolve<'a> = dyn FnMut(&str) -> Option<String> + 'a;

fn rewrite_export_html_images(html: &str, resolve: &mut ExportResolve<'_>) -> String {
    let Ok(pattern) = Regex::new(r#"(?i)src="([^"]+)""#) else {
        return html.to_string();
    };
    pattern
        .replace_all(html, |captures: &regex::Captures<'_>| {
            resolve(captures[1].trim()).map_or_else(
                || captures[0].to_string(),
                |local| format!("src=\"{local}\""),
            )
        })
        .into_owned()
}

fn rewrite_export_markdown_images(markdown: &str, resolve: &mut ExportResolve<'_>) -> String {
    let Ok(pattern) = Regex::new(r"(!\[[^\]]*\]\()([^)]+)(\))") else {
        return markdown.to_string();
    };
    pattern
        .replace_all(markdown, |captures: &regex::Captures<'_>| {
            resolve(captures[2].trim()).map_or_else(
                || captures[0].to_string(),
                |local| format!("{}img/{local}{}", &captures[1], &captures[3]),
            )
        })
        .into_owned()
}

/// 一个图片引用的导出处理：返回 `img/<file>` 形式的相对引用，或 None（保持原样）。
#[allow(clippy::too_many_arguments)]
fn resolve_export_image(
    source: &str,
    image_dir: &Path,
    img_dir: &Path,
    page_url: &str,
    client: Option<&reqwest::blocking::Client>,
    mapping: &mut HashMap<String, String>,
    next_index: &mut u32,
) -> Option<String> {
    if let Some(existing) = mapping.get(source) {
        return Some(existing.clone());
    }
    let file = if let Some(rest) = source.strip_prefix("images/") {
        // 已本地化的图片：store 的 images/ 目录里直接复制。
        let origin = image_dir.join(rest);
        if !origin.exists() {
            return None;
        }
        fs::create_dir_all(parent_dir(img_dir, rest)).ok()?;
        fs::copy(&origin, img_dir.join(rest)).ok()?;
        rest.to_string()
    } else {
        if !(source.starts_with("http://") || source.starts_with("https://")) {
            return None;
        }
        let client = client?;
        let stem = next_index.to_string();
        *next_index += 1;
        download_image(client, source, page_url, img_dir, &stem)?
    };
    let reference = format!("img/{file}");
    mapping.insert(source.to_string(), reference.clone());
    Some(reference)
}

/// `img/<file>` 可能带子目录（文件名里没有，但防御一下），复制前补齐父目录。
fn parent_dir(img_dir: &Path, file: &str) -> PathBuf {
    match Path::new(file).parent() {
        Some(parent) if !parent.as_os_str().is_empty() => img_dir.join(parent),
        _ => img_dir.to_path_buf(),
    }
}

/// 导出文件名取自文章标题：去掉各平台非法字符与控制符，限长，空标题回退到 id。
fn export_stem(title: &str, id: &str) -> String {
    let cleaned: String = title
        .chars()
        .filter(|c| {
            !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') && !c.is_control()
        })
        .collect();
    let mut stem = collapse_whitespace(&cleaned).trim().to_string();
    while stem.ends_with('.') {
        stem.pop();
    }
    if stem.chars().count() > MAX_EXPORT_STEM_CHARS {
        stem = stem.chars().take(MAX_EXPORT_STEM_CHARS).collect();
        while stem.ends_with('.') {
            stem.pop();
        }
    }
    if stem.is_empty() {
        stem = format!("article-{id}");
    }
    stem
}

/// 目标目录里已有同名导出时追加序号（-2、-3……），不覆盖旧文件。
fn unique_export_path(dir: &Path, stem: &str, extension: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.{extension}"));
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|index| dir.join(format!("{stem}-{index}.{extension}")))
        .find(|candidate| !candidate.exists())
        .unwrap_or(first)
}

fn export_html_document(detail: &ReadingArticleDetail, html: &str) -> String {
    let title = html_escape(&detail.article.title);
    let url = html_escape(&detail.article.url);
    let site = html_escape(&detail.article.site_name);
    let byline = detail
        .article
        .byline
        .as_deref()
        .map(html_escape)
        .unwrap_or_default();
    let date = chrono::DateTime::from_timestamp_millis(detail.article.added_at)
        .map(|time| {
            time.with_timezone(&chrono::Local)
                .format("%Y-%m-%d")
                .to_string()
        })
        .unwrap_or_default();
    format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>
body {{ max-width: 720px; margin: 0 auto; padding: 24px 16px; font: 16px/1.7 -apple-system, "PingFang SC", "Microsoft YaHei", sans-serif; color: #222; }}
header {{ border-bottom: 1px solid #eee; margin-bottom: 24px; padding-bottom: 12px; }}
header p {{ color: #888; font-size: 14px; }}
img {{ max-width: 100%; }}
blockquote {{ color: #666; border-left: 3px solid #ddd; margin-left: 0; padding-left: 12px; }}
</style>
</head>
<body>
<header>
<h1>{title}</h1>
<p>{byline} · {site} · {date} · <a href="{url}">原文链接</a></p>
</header>
<main>
{html}
</main>
</body>
</html>
"#
    )
}

fn export_markdown_document(detail: &ReadingArticleDetail, markdown: Option<&str>) -> String {
    let mut out = format!("# {}\n\n", detail.article.title);
    if let Some(byline) = &detail.article.byline {
        out.push_str(&format!("- 作者：{byline}\n"));
    }
    out.push_str(&format!("- 来源：{}\n", detail.article.site_name));
    out.push_str(&format!("- 原始链接：{}\n\n", detail.article.url));
    if let Some(markdown) = markdown.filter(|value| !value.trim().is_empty()) {
        out.push_str(markdown.trim());
        out.push('\n');
    }
    out
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ── Image localization ──────────────────────────────────────────────────────

/// Downloads every `<img src>` into `image_dir` and rewrites the reference to
/// `images/<n>.<ext>`. Failures leave the original URL in place: a missing
/// picture is better than a missing article.
fn localize_images(content_html: &str, page_url: &str, image_dir: &Path) -> String {
    let Ok(pattern) = Regex::new(r#"(?i)<img\b[^>]*?\bsrc\s*=\s*"([^"]*)""#) else {
        return content_html.to_string();
    };
    let client = match http_client() {
        Ok(client) => client,
        Err(_) => return content_html.to_string(),
    };
    let mut seen: HashSet<String> = HashSet::new();
    let mut downloads: Vec<(String, String)> = Vec::new();
    for captures in pattern.captures_iter(content_html) {
        let source = captures[1].to_string();
        if source.is_empty() || !seen.insert(source.clone()) {
            continue;
        }
        if downloads.len() >= MAX_IMAGES {
            break;
        }
        let Some(absolute) = absolute_url(page_url, &source) else {
            continue;
        };
        let index = downloads.len() + 1;
        if let Some(file) =
            download_image(&client, &absolute, page_url, image_dir, &format!("{index}"))
        {
            downloads.push((source, format!("images/{file}")));
        }
    }
    let mut result = content_html.to_string();
    for (source, local) in downloads {
        result = result.replace(&format!("src=\"{source}\""), &format!("src=\"{local}\""));
    }
    result
}

fn localize_cover(cover_url: &str, page_url: &str, image_dir: &Path) -> Option<String> {
    let client = http_client().ok()?;
    let absolute = absolute_url(page_url, cover_url)?;
    download_image(&client, &absolute, page_url, image_dir, "cover")
}

/// Downloads a pulled article's images from the sync server instead of the
/// original site. Anything that cannot be fetched keeps an absolute server URL
/// in the body: a network-backed picture beats a broken one.
fn localize_remote_images(
    content_html: &str,
    base: &str,
    server_id: &str,
    image_dir: &Path,
) -> String {
    let Ok(pattern) = Regex::new(r#"src="images/([^"]+)""#) else {
        return content_html.to_string();
    };
    let mut files: Vec<String> = Vec::new();
    for captures in pattern.captures_iter(content_html) {
        let file = captures[1].to_string();
        if !files.contains(&file) {
            files.push(file);
        }
    }
    if files.is_empty() {
        return content_html.to_string();
    }
    let missing: Vec<String> = files
        .iter()
        .filter(|file| !image_dir.join(file).exists())
        .cloned()
        .collect();
    if !missing.is_empty() {
        if let Some(client) = remote_image_client() {
            download_remote_images(&client, base, server_id, image_dir, &missing);
        }
    }
    let mut result = content_html.to_string();
    for file in files {
        if image_dir.join(&file).exists() {
            continue;
        }
        let remote = format!("{base}/images/{server_id}/{file}");
        result = result.replace(
            &format!("src=\"images/{file}\""),
            &format!("src=\"{remote}\""),
        );
    }
    result
}

/// The cover is served from the same directory as the body images.
fn remote_cover_file(
    detail: &ReadingArticleDetail,
    base: &str,
    image_dir: &Path,
) -> Option<String> {
    let file = Path::new(detail.article.cover_path.as_deref()?)
        .file_name()?
        .to_str()?
        .to_string();
    if !image_dir.join(&file).exists() {
        let client = remote_image_client()?;
        download_remote_images(
            &client,
            base,
            &detail.article.id,
            image_dir,
            std::slice::from_ref(&file),
        );
    }
    image_dir.join(&file).exists().then_some(file)
}

/// Images are a straight copy from our own server: a short timeout keeps a
/// dead connection from stalling a reconciliation.
fn remote_image_client() -> Option<reqwest::blocking::Client> {
    crate::http_client_builder()
        .timeout(Duration::from_secs(10))
        .build()
        .ok()
}

/// Fetches `images/<file>` from the sync server with a small worker pool. The
/// server already holds these files, so this is a straight copy; failures are
/// silent and leave the reference for the caller to rewrite.
fn download_remote_images(
    client: &reqwest::blocking::Client,
    base: &str,
    server_id: &str,
    image_dir: &Path,
    files: &[String],
) {
    if files.is_empty() {
        return;
    }
    let _ = fs::create_dir_all(image_dir);
    let next = AtomicUsize::new(0);
    let workers = files.len().min(MAX_IMAGE_WORKERS);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(file) = files.get(index) else {
                    return;
                };
                let url = format!("{base}/images/{server_id}/{file}");
                let Ok(response) = client.get(&url).send() else {
                    continue;
                };
                if !response.status().is_success() {
                    continue;
                }
                let Ok(bytes) = response.bytes() else {
                    continue;
                };
                if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES {
                    continue;
                }
                let _ = fs::write(image_dir.join(file), &bytes);
            });
        }
    });
}

fn absolute_url(base: &str, candidate: &str) -> Option<String> {
    let candidate = decode_entities(candidate);
    if candidate.starts_with("data:") {
        return None;
    }
    let base = reqwest::Url::parse(base).ok()?;
    let joined = base.join(candidate.trim()).ok()?;
    if !matches!(joined.scheme(), "http" | "https") || url_is_private(&joined) {
        return None;
    }
    Some(joined.to_string())
}

/// Fetches one image. The page URL goes out as `Referer` because CDNs such as
/// `mmbiz.qpic.cn` reject anything else; the response's content type picks
/// the extension.
fn download_image(
    client: &reqwest::blocking::Client,
    url: &str,
    referer: &str,
    image_dir: &Path,
    stem: &str,
) -> Option<String> {
    let response = client
        .get(url)
        .header("Referer", referer)
        .header(
            "Accept",
            "image/avif,image/webp,image/apng,image/*,*/*;q=0.8",
        )
        .send()
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            value
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase()
        })
        .unwrap_or_default();
    let bytes = response.bytes().ok()?;
    if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES {
        return None;
    }
    let extension = match content_type.as_str() {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "image/avif" => "avif",
        "image/bmp" => "bmp",
        _ => sniff_extension(&bytes)?,
    };
    fs::create_dir_all(image_dir).ok()?;
    let file = format!("{stem}.{extension}");
    fs::write(image_dir.join(&file), &bytes).ok()?;
    Some(file)
}

fn sniff_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("png")
    } else if bytes.starts_with(b"GIF8") {
        Some("gif")
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn new_id() -> String {
    let mut bytes = [0u8; 8];
    if getrandom::fill(&mut bytes).is_err() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        bytes.copy_from_slice(&(nanos as u64).to_le_bytes());
    }
    let mut value = u64::from_le_bytes(bytes);
    let alphabet = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut id = String::new();
    for _ in 0..12 {
        id.push(alphabet[(value % 36) as usize] as char);
        value /= 36;
    }
    format!("r{id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wechat_lazy_images_are_promoted_and_hidden_body_revealed() {
        let html = r#"<div id="js_content" style="visibility: hidden;"><p>正文</p><img class="rich_pages" data-src="https://mmbiz.qpic.cn/a.jpg" src="data:image/gif;base64,R0lGOD" /></div>"#;
        let prepared = prepare_html(html);
        assert!(prepared.contains(r#"src="https://mmbiz.qpic.cn/a.jpg""#));
        assert!(!prepared.contains("visibility"));
        assert!(!prepared.contains("data:image/gif"));
    }

    #[test]
    fn fx_tweet_renders_plain_text_photos_quote_and_source() {
        let value = serde_json::json!({
            "code": 200,
            "tweet": {
                "text": "第一行\n\n第二行",
                "url": "https://x.com/a/status/1",
                "author": {"name": "作者", "screen_name": "author"},
                "media": {"all": [
                    {"type": "photo", "url": "https://pbs.twimg.com/media/1.jpg"},
                    {"type": "video", "url": "https://video.twimg.com/x.mp4"}
                ]},
                "quote": {
                    "text": "被引用的话",
                    "author": {"screen_name": "quoted"}
                }
            }
        });
        let html = fx_tweet_html(&value).unwrap();
        let extracted = x_tweet_extract(&html).unwrap();
        assert_eq!(extracted.title, "第一行");
        assert_eq!(extracted.site_name, "作者");
        assert_eq!(extracted.byline.as_deref(), Some("@author"));
        assert!(extracted.content_html.contains("<p>第二行</p>"));
        assert!(extracted
            .content_html
            .contains(r#"<img src="https://pbs.twimg.com/media/1.jpg">"#));
        assert!(!extracted.content_html.contains("video.twimg.com"));
        assert!(extracted.content_html.contains("该推文包含视频"));
        assert!(extracted.content_html.contains("@quoted</a>: 被引用的话"));
        assert!(extracted
            .content_html
            .contains(r#"href="https://x.com/a/status/1""#));
        assert_eq!(
            extracted.cover_url.as_deref(),
            Some("https://pbs.twimg.com/media/1.jpg")
        );
    }

    #[test]
    fn fx_article_renders_draftjs_blocks_with_links_and_code() {
        let value = serde_json::json!({
            "code": 200,
            "tweet": {
                "text": "长文预告 https://t.co/xyz",
                "url": "https://x.com/a/status/2",
                "author": {"name": "作者", "screen_name": "author"},
                "article": {
                    "title": "教程文章",
                    "cover_media": {"media_info": {"original_img_url": "https://pbs.twimg.com/media/cover.jpg"}},
                    "media_entities": [
                        {"media_id": "m1", "media_info": {"original_img_url": "https://pbs.twimg.com/media/inline.jpg"}}
                    ],
                    "content": {
                        "entityMap": [
                            {"key": 0, "value": {"type": "LINK", "data": {"url": "https://example.com/x?a=1&b=2"}}},
                            {"key": 1, "value": {"type": "MARKDOWN", "data": {"markdown": "cargo build --release"}}},
                            {"key": 2, "value": {"type": "MEDIA", "data": {"mediaItems": [{"mediaId": "m1"}]}}}
                        ],
                        "blocks": [
                            {"type": "header-two", "text": "标题二"},
                            {"type": "unstyled", "text": "带链接和加粗的一段",
                             "entityRanges": [{"offset": 3, "length": 2, "key": 0}],
                             "inlineStyleRanges": [{"offset": 0, "length": 3, "style": "Bold"}]},
                            {"type": "ordered-list-item", "text": "第一步"},
                            {"type": "ordered-list-item", "text": "第二步"},
                            {"type": "unordered-list-item", "text": "要点"},
                            {"type": "blockquote", "text": "引用一句"},
                            {"type": "atomic", "text": " ", "entityRanges": [{"offset": 0, "length": 1, "key": 1}]},
                            {"type": "atomic", "text": " ", "entityRanges": [{"offset": 0, "length": 1, "key": 2}]},
                            {"type": "unstyled", "text": "<script>alert(1)</script>"}
                        ]
                    }
                }
            }
        });
        let html = fx_tweet_html(&value).unwrap();
        let extracted = x_tweet_extract(&html).unwrap();
        assert_eq!(extracted.title, "教程文章");
        assert_eq!(
            extracted.cover_url.as_deref(),
            Some("https://pbs.twimg.com/media/cover.jpg")
        );
        let content = &extracted.content_html;
        assert!(content.contains("<h2>标题二</h2>"));
        assert!(content.contains(r#"<a href="https://example.com/x?a=1&amp;b=2""#));
        assert!(content.contains("<strong>带链接</strong>"));
        assert!(content.contains(">和加</a>粗的一段"));
        assert!(content.contains("<ol><li>第一步</li><li>第二步</li></ol>"));
        assert!(content.contains("<ul><li>要点</li></ul>"));
        assert!(content.contains("<blockquote><p>引用一句</p></blockquote>"));
        assert!(content.contains("<pre><code>cargo build --release</code></pre>"));
        assert!(content.contains(r#"<img src="https://pbs.twimg.com/media/inline.jpg">"#));
        assert!(!content.contains("<script>"));
        // URL 里的 & 与实体分开转义，没有双重转义。
        assert!(!content.contains("&amp;amp;"));
    }

    #[test]
    fn fx_rejects_error_payloads_and_empty_authors() {
        assert!(fx_tweet_html(&serde_json::json!({"code": 404, "tweet": {}})).is_none());
        assert!(fx_tweet_html(
            &serde_json::json!({"code": 200, "tweet": {"author": {"name": "", "screen_name": ""}}})
        )
        .is_none());
    }

    #[test]
    fn x_status_urls_are_recognised_and_other_hosts_rejected() {
        assert_eq!(
            x_status_id(
                "https://x.com/zhang_baoqing/status/2115643540248445072?a=mIxGQIcCFC0Zk2Q5wjo&s=19"
            ),
            Some("2115643540248445072".into())
        );
        assert_eq!(
            x_status_id("https://twitter.com/user/status/1234567890123456789"),
            Some("1234567890123456789".into())
        );
        assert_eq!(x_status_id("https://x.com/zhang_baoqing"), None);
        assert_eq!(x_status_id("https://example.com/a/status/123"), None);
        assert_eq!(
            x_status_id("https://x.com/zhang_baoqing/status/notanumber"),
            None
        );
    }

    #[test]
    fn x_tweet_html_round_trips_through_extract() {
        let value = serde_json::json!({
            "text": "最近发现的网站\n\n会实时更新 https://t.co/tBzMtou8IT",
            "user": {"name": "一地鸡毛", "screen_name": "zhang_baoqing"},
            "entities": {"media": [{"url": "https://t.co/tBzMtou8IT"}]},
            "mediaDetails": [{"media_url_https": "https://pbs.twimg.com/media/HVxGNglaUAAjoIW.jpg"}]
        });
        let html = x_tweet_html(&value).unwrap();
        let extracted = x_tweet_extract(&html).unwrap();
        assert_eq!(extracted.title, "最近发现的网站");
        assert_eq!(extracted.site_name, "一地鸡毛");
        assert_eq!(extracted.byline.as_deref(), Some("@zhang_baoqing"));
        assert!(extracted.content_html.contains("会实时更新"));
        assert!(!extracted.content_html.contains("t.co"));
        assert!(extracted
            .content_html
            .contains(r#"src="https://pbs.twimg.com/media/HVxGNglaUAAjoIW.jpg""#));
        assert_eq!(
            extracted.cover_url.as_deref(),
            Some("https://pbs.twimg.com/media/HVxGNglaUAAjoIW.jpg")
        );
    }

    #[test]
    fn x_tweet_html_escapes_hostile_text_and_rejects_empty() {
        assert!(x_tweet_html(
            &serde_json::json!({"text": "", "user": {"name": "a", "screen_name": "b"}})
        )
        .is_none());
        let value = serde_json::json!({
            "text": "<script>alert(1)</script>",
            "user": {"name": "Evil \"Quote\" & <Co>", "screen_name": "e"}
        });
        let html = x_tweet_html(&value).unwrap();
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("Evil &quot;Quote&quot; &amp; &lt;Co&gt;"));
        let extracted = x_tweet_extract(&html).unwrap();
        assert!(!extracted.content_html.contains("<script>"));
        assert_eq!(extracted.site_name, "Evil \"Quote\" & <Co>");
    }

    #[test]
    fn reading_stats_count_cjk_by_character_and_latin_by_word() {
        let (words, minutes) = reading_stats("你好世界 hello world");
        assert_eq!(words, 6);
        assert_eq!(minutes, 1);
        let long: String = "字".repeat(1200);
        assert_eq!(reading_stats(&long).1, 3);
        assert_eq!(reading_stats(""), (0, 0));
    }

    #[test]
    fn sanitizer_keeps_figures_and_relative_image_paths_but_drops_scripts() {
        let html = r#"<figure><img src="images/1.jpg" alt="x"><figcaption>c</figcaption></figure><script>alert(1)</script><a href="javascript:alert(1)">bad</a><p onclick="x()">t</p>"#;
        let clean = sanitize_html(html);
        assert!(clean.contains(r#"<img src="images/1.jpg" alt="x">"#));
        assert!(clean.contains("<figcaption>c</figcaption>"));
        assert!(!clean.contains("script"));
        assert!(!clean.contains("javascript:"));
        assert!(!clean.contains("onclick"));
    }

    #[test]
    fn prepare_html_reveals_opacity_zero_but_keeps_partial_opacity() {
        let html = r#"<div style="opacity: 0"><p>正文</p></div><div style="opacity:0;"><p>正文二</p></div><div style="opacity: 0.5"><p>装饰</p></div>"#;
        let prepared = prepare_html(html);
        assert!(prepared.contains("<p>正文</p>"));
        assert!(prepared.contains("<p>正文二</p>"));
        assert!(prepared.contains(r#"style=""#));
        // 半透明装饰与其它属性原样保留。
        assert!(prepared.contains("opacity: 0.5"));
        assert!(!prepared.contains(r#"opacity: 0""#));
        assert!(!prepared.contains("opacity:0;"));
    }

    #[test]
    fn style_filter_keeps_decorations_and_drops_dangerous_declarations() {
        assert_eq!(
            sanitize_style("border: 1px solid green; font-size: 16px; color:#07c160"),
            "border: 1px solid green; font-size: 16px; color:#07c160"
        );
        let filtered = sanitize_style(
            "background-image: url(https://evil.example/track.png); position: fixed; top: 0; behavior: url(#default#time2); @import url(evil.css);",
        );
        assert_eq!(filtered, "top: 0");
        assert!(sanitize_style("").is_empty());
    }

    #[test]
    fn sanitizer_keeps_safe_inline_styles_but_strips_dangerous_ones() {
        let html = r#"<section style="border: 1px solid green; padding: 12px"><p style="position: fixed; z-index: 9999">覆盖层</p></section>"#;
        let clean = sanitize_html(html);
        assert!(clean.contains(r#"style="border: 1px solid green; padding: 12px""#));
        assert!(!clean.contains("position"));
        assert!(clean.contains(">覆盖层</p>"));
    }

    #[test]
    fn wechat_styled_cards_keep_their_inline_styles() {
        let html = r#"<html><head><title>样式测试</title></head><body><div id="js_content" style="visibility: hidden;"><section style="border: 1px solid green; padding: 12px"><h2>01 背景</h2><p>按团队分工，我们的代码和文档分布在四个仓库，一个业务需求通常会同时牵动这四方。</p></section></div></body></html>"#;
        let page = FetchedPage {
            html: html.into(),
            final_url: "https://mp.weixin.qq.com/s/test".into(),
        };
        let extracted = extract(&page, &page.final_url).unwrap();
        assert!(extracted.content_html.contains("border: 1px solid green"));
    }

    #[test]
    fn urls_are_validated_and_fragments_dropped() {
        assert!(normalize_url("file:///etc/passwd").is_err());
        assert!(normalize_url("ftp://example.com/a").is_err());
        assert!(normalize_url("http://127.0.0.1:8080/private").is_err());
        assert!(normalize_url("http://169.254.169.254/latest/meta-data").is_err());
        assert!(absolute_url("https://example.com/a", "http://10.0.0.1/private").is_none());
        assert_eq!(
            normalize_url("https://example.com/a?b=1#frag").unwrap(),
            "https://example.com/a?b=1"
        );
    }

    #[test]
    fn wechat_fallback_preserves_article_link_lists() {
        let html = r#"<html><head><meta property="og:title" content="还没放假吗?"></head><body><div id="js_content"><p>各位彦祖亦菲们，祝大家国庆中秋双节快乐！</p><h2>AI技术</h2><p><a href="https://mp.weixin.qq.com/s/other">JEV：一个不会写代码的模型，为什么拿下4000万美元？</a></p></div></body></html>"#;
        let page = FetchedPage {
            html: html.into(),
            final_url: "https://mp.weixin.qq.com/s/test".into(),
        };
        let extracted = extract(&page, &page.final_url).unwrap();
        assert!(extracted.content_html.contains("JEV"));
        assert!(extracted.content_html.contains("mp.weixin.qq.com/s/other"));
        assert_eq!(extracted.title, "还没放假吗?");
    }

    #[test]
    fn wechat_profile_component_renders_as_linkless_card() {
        let html = r#"<section><mp-common-profile data-pluginname="mpprofile" data-nickname="腾讯云开发者" data-headimg="http://mmbiz.qpic.cn/a.png?wx_fmt=png&amp;wxfrom=19" data-signature="腾讯云官方社区公众号，汇聚技术开发者群体。" data-origin_num="1049" data-verify_status="2"></mp-common-profile></section>"#;
        let prepared = prepare_html(html);
        assert!(prepared.contains("腾讯云开发者"));
        assert!(prepared.contains("腾讯云官方社区公众号，汇聚技术开发者群体。"));
        assert!(prepared.contains("1049篇原创内容"));
        assert!(prepared.contains("公众号"));
        // URL 属性转义保真；下载时 absolute_url 里的 decode_entities 会还原 &。
        assert!(prepared.contains(r#"src="http://mmbiz.qpic.cn/a.png?wx_fmt=png&amp;wxfrom=19""#));
        assert!(!prepared.contains("mp-common-profile"));
        // 纯展示卡：没有链接标签，阅读器的 a 样式不会作用到它。
        assert!(!prepared.contains("<a"));
        // 头像 margin 写全，不被阅读器图片居中规则挪位。
        assert!(prepared.contains("margin: 2px 14px 0 0"));
        // 数据残缺的组件直接丢弃。
        assert_eq!(
            prepare_html("<p>x</p><mp-common-profile data-nickname=\"\"></mp-common-profile>"),
            "<p>x</p>"
        );
    }

    #[test]
    fn wechat_error_shell_is_not_misreported_as_empty_article() {
        let html = r#"<div class="weui-msg__title warn">参数错误</div>"#;
        assert!(is_wechat_error_shell(html));
    }

    #[test]
    fn wechat_fallback_extracts_author_and_account_from_page_vars() {
        let html = r#"<html><head><title>长文标题</title><script>var author = "徐泓镔";
var nickname = htmlDecode("腾讯云开发者");
var createTime = '2026-11-12 09:09';
copyright_stat: '1' * 1,</script></head><body><div id="js_content"><p>正文第一段，长度足以通过可读性阈值，满足提取要求。</p></div></body></html>"#;
        let page = FetchedPage {
            html: html.into(),
            final_url: "https://mp.weixin.qq.com/s/test".into(),
        };
        let extracted = extract(&page, &page.final_url).unwrap();
        // 主作者是人（byline），公众号名是 site_name——两者不再混用。
        assert_eq!(extracted.byline.as_deref(), Some("徐泓镔"));
        assert_eq!(extracted.site_name, "腾讯云开发者");
    }

    #[test]
    fn wechat_article_header_shows_author_and_publish_time() {
        let html = r#"<script>var author = "徐泓镔";
var createTime = '2026-11-12 09:09';
copyright_stat: '1' * 1,
var nickname = htmlDecode("腾讯云开发者");</script>"#;
        let header = wechat_article_header(html).unwrap();
        assert!(header.contains(">原创</span>"));
        assert!(header.contains("徐泓镔"));
        assert!(header.contains("腾讯云开发者"));
        assert!(header.contains("2026-11-12 09:09"));
        assert!(!header.contains("<a"));
        // 非原创（copyright_stat=0）不出原创标记；全无数据时不生成。
        let plain = r#"<script>var author = "某某";
copyright_stat: '0' * 1,</script>"#;
        let header = wechat_article_header(plain).unwrap();
        assert!(!header.contains(">原创<"));
        assert!(wechat_article_header("<p>无变量</p>").is_none());
    }

    #[test]
    fn wechat_fallback_uses_balanced_div_and_keeps_images() {
        // js_content 里的嵌套 div 不应把提取截断在第一个 </div>。
        let html = r#"<html><head><title>图集</title></head><body><div id="js_content"><div style="border: 1px solid green"><p>外层第一段，足以达到可读性阈值的内容。</p><img src="https://mmbiz.qpic.cn/a.jpg"></div><p>嵌套结束后的段落。</p></div><div id="js_content_footer"></div></body></html>"#;
        let page = FetchedPage {
            html: html.into(),
            final_url: "https://mp.weixin.qq.com/s/test".into(),
        };
        let extracted = extract(&page, &page.final_url).unwrap();
        assert!(extracted.content_html.contains("外层第一段"));
        assert!(extracted.content_html.contains("嵌套结束后的段落"));
        assert!(extracted
            .content_html
            .contains(r#"src="https://mmbiz.qpic.cn/a.jpg""#));
        assert!(extracted.content_html.contains("border: 1px solid green"));
        // 容器外的骨架（如 js_content_footer）不会混进正文。
        assert!(!extracted.content_html.contains("footer"));
        // 图片进不了 readability 时字数仍按容器全文统计。
        assert!(extracted.word_count > 0);
        assert_eq!(extracted.markdown, None);
    }

    #[test]
    fn decode_html_honours_gbk_meta_charset() {
        let (gbk, _, _) = encoding_rs::GBK
            .encode("<html><head><meta charset=\"gbk\"></head><body>中文</body></html>");
        assert!(decode_html(&gbk, None).contains("中文"));
        assert!(decode_html("<p>直接</p>".as_bytes(), Some("utf-8")).contains("直接"));
    }

    #[test]
    fn og_image_is_found_in_either_attribute_order() {
        let a = r#"<meta property="og:image" content="https://x/a.jpg">"#;
        let b = r#"<meta content="https://x/b.jpg" property="og:image">"#;
        assert_eq!(og_image(a).as_deref(), Some("https://x/a.jpg"));
        assert_eq!(og_image(b).as_deref(), Some("https://x/b.jpg"));
    }

    #[test]
    fn pasted_multi_link_inputs_are_rejected() {
        // 单行输入框把换行折成空格后的样子。
        let glued = "https://mp.weixin.qq.com/s/Lc1qmdP9PdjkH-YIVYM98g https://mp.weixin.qq.com/s/aQIH8KKcVuOfztlIBQDAOA";
        assert!(normalize_url(glued).is_err());
        let encoded = "https://mp.weixin.qq.com/s/Lc1qmdP9PdjkH-YIVYM98g%20https://";
        assert!(normalize_url(encoded).is_err());
        assert_eq!(
            normalize_url("https://mp.weixin.qq.com/s/Lc1qmdP9PdjkH-YIVYM98g").unwrap(),
            "https://mp.weixin.qq.com/s/Lc1qmdP9PdjkH-YIVYM98g"
        );
    }

    #[test]
    fn pages_without_a_body_fall_back_to_their_meta_description() {
        let video_article = r#"<html><head>
            <meta property="og:title" content="办税小知识：如何使用数电票？">
            <meta property="og:description" content="点击视频了解，来源：国家税务总局">
          </head><body><div id="js_content" class="share_notice_wrp">分享</div></body></html>"#;
        // 没有 og:title 的页面仍然算失败，避免把 404 页收进库里。
        let broken = r#"<html><head><title>404</title></head><body>Not Found</body></html>"#;
        assert!(link_only_body(broken).is_none());
        let body = link_only_body(video_article).expect("meta-only page keeps a placeholder body");
        assert!(body.contains("没有可提取的正文"));
        assert!(body.contains("点击视频了解"));
        let page = FetchedPage {
            html: video_article.into(),
            final_url: "https://mp.weixin.qq.com/s/x".into(),
        };
        let extracted = extract(&page, "https://mp.weixin.qq.com/s/x").unwrap();
        assert_eq!(extracted.title, "办税小知识：如何使用数电票？");
        assert_eq!(extracted.excerpt, "点击视频了解，来源：国家税务总局");
    }

    fn temp_store() -> (tempfile::TempDir, ReadingStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = ReadingStore::new(dir.path().join("reading"));
        (dir, store)
    }

    fn insert_fixture(store: &ReadingStore, id: &str) {
        let connection = store.connect().unwrap();
        connection
            .execute(
                "INSERT INTO articles (id, url, title, content_html, added_at) VALUES (?1, ?2, '标题', '<p>正文</p>', 1)",
                params![id, format!("https://example.com/{id}")],
            )
            .unwrap();
    }

    #[test]
    fn progress_is_sticky_at_100_and_drops_anchor_at_the_ends() {
        let (_dir, store) = temp_store();
        insert_fixture(&store, "r1");
        let mid = store.set_progress("r1", 42, Some("idx:7")).unwrap();
        assert_eq!(mid.progress_percent, 42);
        assert_eq!(mid.progress_anchor.as_deref(), Some("idx:7"));
        assert!(mid.read_at.is_none());

        let done = store.set_progress("r1", 100, Some("idx:99")).unwrap();
        assert_eq!(done.progress_percent, 100);
        assert!(done.progress_anchor.is_none());
        assert!(done.read_at.is_some());

        let again = store.set_progress("r1", 10, Some("idx:1")).unwrap();
        assert_eq!(again.progress_percent, 100, "read stays read");

        let unread = store.set_read("r1", false).unwrap();
        assert_eq!(unread.progress_percent, 0);
        assert!(unread.read_at.is_none());
        let zero = store.set_progress("r1", 0, Some("idx:0")).unwrap();
        assert!(zero.progress_anchor.is_none());
    }

    #[test]
    fn annotations_round_trip_and_cascade_on_remove() {
        let (_dir, store) = temp_store();
        insert_fixture(&store, "r2");
        let created = store
            .upsert_annotation(ReadingAnnotationInput {
                id: "a1".into(),
                article_id: "r2".into(),
                quote: "正文".into(),
                start: 0,
                end: 2,
                note: String::new(),
            })
            .unwrap();
        assert_eq!(created.note, "");
        let updated = store
            .upsert_annotation(ReadingAnnotationInput {
                id: "a1".into(),
                article_id: "r2".into(),
                quote: "正文".into(),
                start: 0,
                end: 2,
                note: "想法".into(),
            })
            .unwrap();
        assert_eq!(updated.note, "想法");
        assert_eq!(updated.created_at, created.created_at);
        let detail = store.get("r2").unwrap();
        assert_eq!(detail.annotations.len(), 1);
        assert_eq!(detail.article.annotation_count, 1);
        assert!(store
            .upsert_annotation(ReadingAnnotationInput {
                id: "a2".into(),
                article_id: "r2".into(),
                quote: "x".into(),
                start: 3,
                end: 3,
                note: String::new(),
            })
            .is_err());
        store.remove("r2").unwrap();
        assert!(store.get("r2").is_err());
        assert!(store.list().unwrap().is_empty());
    }
}

#[cfg(test)]
mod live_tests {
    //! Network tests, run by hand: `cargo test -p todo-core live_ -- --ignored --nocapture`.
    use super::*;

    fn probe(url: &str) {
        let dir = tempfile::tempdir().unwrap();
        let store = ReadingStore::new(dir.path());
        let detail = store.add(url).expect("add");
        let images = fs::read_dir(&detail.image_dir)
            .map(|d| d.count())
            .unwrap_or(0);
        println!(
            "byline={:?}\nHTML>>>\n{}\n<<<",
            detail.article.byline,
            detail.content_html.chars().take(1200).collect::<String>()
        );
        println!(
            "title={:?}\nsite={:?} words={} minutes={} excerpt={:?}\ncover={:?}\nimages_on_disk={} html_len={} md_len={}",
            detail.article.title,
            detail.article.site_name,
            detail.article.word_count,
            detail.article.reading_minutes,
            detail.article.excerpt,
            detail.article.cover_path,
            images,
            detail.content_html.len(),
            detail.markdown.as_ref().map(|m| m.len()).unwrap_or(0)
        );
        assert!(detail.content_html.len() > 200);
    }

    #[test]
    #[ignore]
    fn live_x_tweet() {
        probe(
            &std::env::var("READING_URL").unwrap_or_else(|_| {
                "https://x.com/zhang_baoqing/status/2115643540248445072".into()
            }),
        );
    }

    #[test]
    #[ignore]
    fn live_wechat() {
        probe(
            &std::env::var("READING_URL")
                .unwrap_or_else(|_| "https://mp.weixin.qq.com/s/Vo0hZqbCzsKdH3RgLB4R1A".into()),
        );
    }

    /// Ordinary web articles go straight through readability; this probe
    /// guards the batch-import promise that "simple sites just work".
    #[test]
    #[ignore]
    fn live_generic_site() {
        probe(&std::env::var("READING_URL").unwrap_or_else(|_| {
            "https://blog.rust-lang.org/2026/10/26/how-to-fund-rust-contributors/".into()
        }));
    }
}

#[cfg(test)]
mod reading_context_tests {
    use super::*;

    fn store() -> (tempfile::TempDir, ReadingStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = ReadingStore::new(dir.path().join("reading"));
        (dir, store)
    }

    #[test]
    fn legacy_wechat_rows_get_the_account_name_back() {
        let (_d, store) = store();
        let _ = store.list(); // 建库（此时还没有旧数据）。
        let connection = rusqlite::Connection::open(store.root().join("reading.sqlite")).unwrap();
        // 旧解析把作者名同时写进了 byline 和 site_name。
        connection
            .execute(
                "INSERT INTO articles (id, url, title, site_name, byline, excerpt, content_html, word_count, added_at)
                 VALUES ('wxold', 'https://mp.weixin.qq.com/s/old', '旧文', '徐泓镔', '徐泓镔', '原创 徐泓镔 腾讯云开发者', '<p>x</p>', 10, ?1),
                        ('wxok',  'https://mp.weixin.qq.com/s/ok',  '新文', '腾讯云开发者', '徐泓镔', '原创 徐泓镔 腾讯云开发者', '<p>x</p>', 10, ?1)",
                rusqlite::params![now_ms()],
            )
            .unwrap();
        // 下一次连接触发规范化：旧行被纠正，正确行不动。
        let article = store.get("wxold").unwrap().article;
        assert_eq!(article.site_name, "腾讯云开发者");
        assert_eq!(article.byline.as_deref(), Some("徐泓镔"));
        let article = store.get("wxok").unwrap().article;
        assert_eq!(article.site_name, "腾讯云开发者");
        assert_eq!(article.byline.as_deref(), Some("徐泓镔"));
        // 摘要不含公众号名的行（认不出次作者）保持原样。
        connection
            .execute(
                "INSERT INTO articles (id, url, title, site_name, byline, excerpt, content_html, word_count, added_at)
                 VALUES ('wxnone', 'https://mp.weixin.qq.com/s/none', '无摘要', '某人', '某人', '正文摘要，没有作者连排。', '<p>x</p>', 10, ?1)",
                rusqlite::params![now_ms()],
            )
            .unwrap();
        let article = store.get("wxnone").unwrap().article;
        assert_eq!(article.site_name, "某人");
    }

    fn fixture_page(title: &str, body: &str) -> FetchedPage {
        // Long enough to pass extract()'s readability threshold.
        let paragraph = format!("{body}，并且这一段足够长，可以被可读性算法识别为正文。");
        let article = (0..12)
            .map(|_| format!("<p>{paragraph}</p>"))
            .collect::<String>();
        FetchedPage {
            html: format!("<html><head><title>{title}</title></head><body><article>{article}</article></body></html>"),
            final_url: "https://example.com/a".into(),
        }
    }

    fn seed(store: &ReadingStore) -> String {
        // Direct insert keeps the fixture free of extract()'s readability
        // requirements; these tests cover the context layer, not extraction.
        let _ = store.list(); // Ensure the database file exists.
        let connection = rusqlite::Connection::open(store.root().join("reading.sqlite")).unwrap();
        let id = new_id();
        connection
            .execute(
                "INSERT INTO articles (id, url, title, site_name, byline, content_html, word_count, added_at)
                 VALUES (?1, ?2, ?3, 'example.com', '作者', '<p>原正文</p>', 10, ?4)",
                rusqlite::params![id, "https://example.com/a", "原标题", now_ms()],
            )
            .unwrap();
        id
    }

    #[test]
    fn context_round_trips_and_defaults_are_sane() {
        let (_d, store) = store();
        let id = seed(&store);
        let detail = store.get(&id).unwrap();
        assert_eq!(detail.context, ReadingContext::empty());

        let saved = store
            .update_context(
                &id,
                &ReadingContextInput {
                    saved_reason: "研究 E2E 定位方式".into(),
                    focus_questions: "能否兼容 Playwright？".into(),
                    revisit_condition: "E2E 重构进入 In Progress".into(),
                    personal_notes: "可访问性树方案可行".into(),
                    revisit_type: "TASK_COMPLETED".into(),
                },
            )
            .unwrap();
        assert_eq!(saved.context.saved_reason, "研究 E2E 定位方式");
        assert_eq!(saved.context.revisit_type, "TASK_COMPLETED");
        let reloaded = store.get(&id).unwrap();
        assert_eq!(reloaded.context, saved.context);
    }

    #[test]
    fn failed_capture_keeps_url_and_accepts_context() {
        let (_d, store) = store();
        let detail = store
            .add_failed("https://unreachable.invalid/post")
            .unwrap();
        assert_eq!(detail.context.parse_status, "failed");
        assert_eq!(detail.article.url, "https://unreachable.invalid/post");
        // Context edits work immediately on the failed record.
        let updated = store
            .update_context(
                &detail.article.id,
                &ReadingContextInput {
                    saved_reason: "先收藏，稍后再试".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(updated.context.saved_reason, "先收藏，稍后再试");
        // Re-adding the same URL dedups instead of duplicating.
        let again = store
            .add_failed("https://unreachable.invalid/post")
            .unwrap();
        assert_eq!(again.article.id, detail.article.id);
    }

    #[test]
    fn pending_capture_enters_the_list_before_the_body_arrives() {
        let (_d, store) = store();
        let detail = store.add_pending("https://example.com/later").unwrap();
        assert_eq!(detail.article.parse_status, "pending");
        // The list shows it right away, titled by host, with no body.
        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].title, "example.com");
        assert_eq!(listed[0].parse_status, "pending");
        assert_eq!(
            store.pending_ids().unwrap(),
            vec![detail.article.id.clone()]
        );

        // A failed attempt keeps the row and flips the badge.
        store.mark_failed(&detail.article.id, "抓取超时").unwrap();
        let listed = store.list().unwrap();
        assert_eq!(listed[0].parse_status, "failed");
        assert_eq!(listed[0].parse_error, "抓取超时");
        assert!(store.pending_ids().unwrap().is_empty());

        // Retry puts it back in the queue; success clears the badge and the reason.
        store.mark_pending(&detail.article.id).unwrap();
        let retrying = store.list().unwrap();
        assert_eq!(retrying[0].parse_status, "pending");
        assert_eq!(retrying[0].parse_error, "");
        let done = store
            .replace_content(&detail.article.id, &fixture_page("补好了", "正文"))
            .unwrap();
        assert_eq!(done.article.parse_status, "ok");
        assert_eq!(done.article.title, "补好了");
    }

    #[test]
    fn adding_a_pending_url_twice_returns_the_first_row() {
        let (_d, store) = store();
        let first = store.add_pending("https://example.com/once").unwrap();
        let second = store.add_pending("https://example.com/once").unwrap();
        assert_eq!(first.article.id, second.article.id);
        assert_eq!(store.list().unwrap().len(), 1);
    }

    #[test]
    fn refetch_updates_source_but_never_touches_context() {
        let (_d, store) = store();
        let id = seed(&store);
        store
            .update_context(
                &id,
                &ReadingContextInput {
                    saved_reason: "珍贵的原因".into(),
                    personal_notes: "我的笔记".into(),
                    focus_questions: "我的关注".into(),
                    revisit_condition: "我的条件".into(),
                    revisit_type: "DATE".into(),
                },
            )
            .unwrap();
        let before = store.get(&id).unwrap();
        let before_progress = store.set_progress(&id, 40, Some("anchor")).unwrap();

        // Simulate a successful re-parse (no network): source columns change.
        let replaced = store
            .replace_content(&id, &fixture_page("新标题", "新正文"))
            .unwrap();
        assert_eq!(replaced.article.title, "新标题");
        assert!(replaced.article.title.starts_with("新标题"));

        let after = store.get(&id).unwrap();
        assert_eq!(after.context.saved_reason, "珍贵的原因");
        assert_eq!(after.context.personal_notes, "我的笔记");
        assert_eq!(after.context.focus_questions, "我的关注");
        assert_eq!(after.context.revisit_condition, "我的条件");
        assert_eq!(after.context.revisit_type, "DATE");
        assert_eq!(after.context.parse_status, "ok");
        // Reading state survives too.
        assert_eq!(
            after.article.progress_percent,
            before_progress.progress_percent
        );
        assert_eq!(before.article.title, "原标题");
    }

    #[test]
    fn project_and_task_links_are_deduped_and_removable() {
        let (_d, store) = store();
        let id = seed(&store);
        let linked = store.link_project(&id, "proj-1", "Desktop App").unwrap();
        assert_eq!(linked.context.project_links.len(), 1);
        // Duplicate insert is ignored.
        let again = store.link_project(&id, "proj-1", "Desktop App").unwrap();
        assert_eq!(again.context.project_links.len(), 1);
        store.link_project(&id, "proj-2", "").unwrap();
        store.link_task(&id, "a1b2c3", "降低 E2E 耦合").unwrap();

        let multi = store.get(&id).unwrap();
        assert_eq!(multi.context.project_links.len(), 2);
        assert_eq!(multi.context.task_links.len(), 1);

        // Removing a membership keeps the article and the other link.
        let unlinked = store.unlink_project(&id, "proj-1").unwrap();
        assert_eq!(unlinked.context.project_links.len(), 1);
        assert_eq!(unlinked.context.task_links.len(), 1);
        // Unknown link removal is an error, not a silent no-op.
        assert!(store.unlink_task(&id, "zzzzzz").is_err());
    }

    #[test]
    fn removing_article_cascades_links() {
        let (_d, store) = store();
        let id = seed(&store);
        store.link_project(&id, "proj-1", "").unwrap();
        store.link_task(&id, "a1b2c3", "").unwrap();
        store.remove(&id).unwrap();
        let connection = rusqlite::Connection::open(store.root().join("reading.sqlite")).unwrap();
        let projects: i64 = connection
            .query_row("SELECT COUNT(*) FROM article_project_links", [], |row| {
                row.get(0)
            })
            .unwrap();
        let tasks: i64 = connection
            .query_row("SELECT COUNT(*) FROM article_task_links", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!((projects, tasks), (0, 0));
    }

    #[test]
    fn exports_reference_card_and_project_index() {
        let (_d, store) = store();
        let id = seed(&store);
        store
            .update_context(
                &id,
                &ReadingContextInput {
                    saved_reason: "研究定位".into(),
                    focus_questions: "Q1".into(),
                    revisit_condition: "条件".into(),
                    personal_notes: "结论".into(),
                    revisit_type: "NONE".into(),
                },
            )
            .unwrap();
        store.link_project(&id, "proj-1", "Desktop App").unwrap();
        store.link_task(&id, "a1b2c3", "").unwrap();

        let card = store.export_reference(&id).unwrap();
        assert!(card.contains("type: reading"));
        assert!(card.contains("# 收藏原因"));
        assert!(card.contains("研究定位"));
        assert!(card.contains("- proj-1（Desktop App）"));
        assert!(card.contains("- a1b2c3"));

        let index = store.export_project_index("proj-1").unwrap();
        assert!(index.contains("# Reading References"));
        assert!(index.contains("## 待阅读"));
        assert!(index.contains("- 收藏原因：研究定位"));
        assert!(index.contains("reading://"));
        // Mark read → moves to 已阅读 section.
        store.set_read(&id, true).unwrap();
        let index = store.export_project_index("proj-1").unwrap();
        assert!(index.contains("## 已阅读"));
    }

    #[test]
    fn export_html_uses_title_filename_and_lands_images_in_img() {
        let (_d, store) = store();
        let id = seed(&store);
        let image_dir = store.root().join("images").join(&id);
        fs::create_dir_all(&image_dir).unwrap();
        fs::write(image_dir.join("1.jpg"), b"fake").unwrap();
        let connection = rusqlite::Connection::open(store.root().join("reading.sqlite")).unwrap();
        connection
            .execute(
                "UPDATE articles SET title = ?2, content_html = ?3, markdown = NULL WHERE id = ?1",
                rusqlite::params![
                    id,
                    "给AI写了800行规则",
                    r#"<section style="border: 1px solid green"><p>正文</p><img src="images/1.jpg" alt="a"><img src="https://invalid.example/x.png"></section>"#,
                ],
            )
            .unwrap();
        let out_dir = tempfile::tempdir().unwrap();
        let result = store
            .export_article(&id, ReadingExportFormat::Html, out_dir.path())
            .unwrap();
        // 已本地化的 1.jpg 复制进 img/；下载失败的远程图保留原 URL。
        assert_eq!(result.images, 1);
        assert!(out_dir.path().join("img").join("1.jpg").exists());
        let html = fs::read_to_string(&result.path).unwrap();
        assert!(html.contains(r#"src="img/1.jpg""#));
        assert!(html.contains("https://invalid.example/x.png"));
        assert!(html.contains("border: 1px solid green"));
        assert!(html.contains("原文链接"));
        assert!(result
            .path
            .ends_with(&format!("{}.html", export_stem("给AI写了800行规则", &id))));
    }

    #[test]
    fn export_reuses_title_filename_with_sequence_when_repeated() {
        let (_d, store) = store();
        let id = seed(&store);
        let out_dir = tempfile::tempdir().unwrap();
        let first = store
            .export_article(&id, ReadingExportFormat::Html, out_dir.path())
            .unwrap();
        let second = store
            .export_article(&id, ReadingExportFormat::Html, out_dir.path())
            .unwrap();
        assert!(first.path.ends_with(".html"));
        assert!(second.path.ends_with("-2.html"));
        assert_ne!(first.path, second.path);
    }

    #[test]
    fn export_markdown_converts_html_when_stored_markdown_is_missing() {
        let (_d, store) = store();
        let id = seed(&store);
        let image_dir = store.root().join("images").join(&id);
        fs::create_dir_all(&image_dir).unwrap();
        fs::write(image_dir.join("1.jpg"), b"fake").unwrap();
        let connection = rusqlite::Connection::open(store.root().join("reading.sqlite")).unwrap();
        connection
            .execute(
                "UPDATE articles SET title = ?2, content_html = ?3, markdown = NULL WHERE id = ?1",
                rusqlite::params![
                    id,
                    "流程：x-flow 让 AI 可控",
                    "<h2>01 背景</h2><ul><li>要点一</li><li>要点二</li></ul><p>第一段。</p><img src=\"images/1.jpg\" alt=\"a\">",
                ],
            )
            .unwrap();
        let out_dir = tempfile::tempdir().unwrap();
        let result = store
            .export_article(&id, ReadingExportFormat::Markdown, out_dir.path())
            .unwrap();
        assert!(result.path.ends_with(".md"));
        let markdown = fs::read_to_string(&result.path).unwrap();
        assert!(markdown.contains("# 流程：x-flow 让 AI 可控"));
        assert!(markdown.contains("- 来源：example.com"));
        assert!(markdown.contains("## 01 背景"));
        assert!(markdown.contains("*   要点一"));
        assert!(markdown.contains("![a](img/1.jpg)"));
    }

    #[test]
    fn export_markdown_uses_stored_markdown_and_keeps_remote_images() {
        let (_d, store) = store();
        let id = seed(&store);
        let connection = rusqlite::Connection::open(store.root().join("reading.sqlite")).unwrap();
        connection
            .execute(
                "UPDATE articles SET markdown = ?2 WHERE id = ?1",
                rusqlite::params![
                    id,
                    "![远程图](https://invalid.example/lost.png)\n\n正文第一段。"
                ],
            )
            .unwrap();
        let out_dir = tempfile::tempdir().unwrap();
        let result = store
            .export_article(&id, ReadingExportFormat::Markdown, out_dir.path())
            .unwrap();
        let markdown = fs::read_to_string(&result.path).unwrap();
        // 下载失败的远程图保留原 URL，而不是被静默丢掉。
        assert!(markdown.contains("https://invalid.example/lost.png"));
        assert!(markdown.contains("正文第一段。"));
    }

    #[test]
    fn export_stem_strips_forbidden_characters_and_truncates() {
        assert_eq!(export_stem(r#"a/b\c:d*e?f"g<h>i|j"#, "id1"), "abcdefghij");
        assert_eq!(export_stem("标题\n带  换行", "id2"), "标题带 换行");
        assert_eq!(export_stem(&"长".repeat(100), "id3").chars().count(), 80);
        assert_eq!(export_stem("...", "id4"), "article-id4");
        assert_eq!(export_stem("", "id5"), "article-id5");
    }

    #[test]
    fn legacy_database_is_upgraded_without_loss() {
        let (_dir, store) = store();
        let id = seed(&store);
        // Simulate a v1 library (no context columns/tables).
        {
            let connection =
                rusqlite::Connection::open(store.root().join("reading.sqlite")).unwrap();
            connection.execute_batch(
                "CREATE TABLE legacy AS SELECT id, url, title FROM articles;
                 DROP TABLE article_project_links;
                 DROP TABLE article_task_links;
                 ALTER TABLE articles DROP COLUMN saved_reason;
                 ALTER TABLE articles DROP COLUMN parse_status;
                 ALTER TABLE articles ADD COLUMN saved_reason TEXT NOT NULL DEFAULT '';
                 ALTER TABLE articles ADD COLUMN parse_status TEXT NOT NULL DEFAULT 'ok';
                 INSERT INTO articles (id, url, title, content_html, added_at) VALUES ('legacy1', 'https://example.com/old', '旧文章', '', 0);
                 ",
            )
            .unwrap();
        }
        // Reconnecting upgrades in place and keeps every row readable.
        let detail = store.get(&id).unwrap();
        assert_eq!(detail.context.saved_reason, "");
        let legacy = store.get("legacy1").unwrap();
        assert_eq!(legacy.article.title, "旧文章");
        assert_eq!(legacy.context.parse_status, "ok");
    }

    /// A server-side article as `GET /articles/{id}` returns it.
    fn remote_detail(id: &str, url: &str, body: &str) -> ReadingArticleDetail {
        ReadingArticleDetail {
            article: ReadingArticle {
                id: id.into(),
                url: url.into(),
                title: "服务器标题".into(),
                site_name: "example.com".into(),
                excerpt: "服务器摘要".into(),
                byline: Some("作者".into()),
                cover_path: None,
                word_count: if body.is_empty() { 0 } else { 100 },
                reading_minutes: if body.is_empty() { 0 } else { 1 },
                added_at: now_ms() - 1_000,
                last_opened_at: None,
                progress_percent: 42,
                progress_anchor: Some("idx:3".into()),
                read_at: None,
                annotation_count: 0,
                parse_status: if body.is_empty() { "failed" } else { "ok" }.into(),
                parse_error: String::new(),
            },
            content_html: body.into(),
            markdown: if body.is_empty() {
                None
            } else {
                Some("# 正文".into())
            },
            image_dir: "/srv/reading/images/x".into(),
            annotations: Vec::new(),
            context: ReadingContext {
                saved_reason: "服务器上的原因".into(),
                ..ReadingContext::empty()
            },
        }
    }

    #[test]
    fn pulled_articles_reuse_the_server_body_without_refetching() {
        let (_d, store) = store();
        let detail = remote_detail(
            "srv00001",
            "https://example.com/pulled",
            "<p>服务器正文</p>",
        );
        let saved = store.save_pulled(&detail, "http://127.0.0.1:9").unwrap();
        assert_eq!(saved.article.id, "srv00001");
        assert_eq!(saved.content_html, "<p>服务器正文</p>");
        assert_eq!(saved.article.parse_status, "ok");
        assert_eq!(saved.context.saved_reason, "服务器上的原因");
        assert_eq!(saved.article.progress_percent, 42);

        // 再拉一次是幂等的：不新增行，也不覆盖本机改过的上下文。
        store
            .update_context(
                &saved.article.id,
                &ReadingContextInput {
                    saved_reason: "本地改过".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let again = store.save_pulled(&detail, "http://127.0.0.1:9").unwrap();
        assert_eq!(again.article.id, "srv00001");
        assert_eq!(again.context.saved_reason, "本地改过");
        assert_eq!(store.list().unwrap().len(), 1);
    }

    #[test]
    fn a_bodyless_pull_is_filled_in_when_the_server_finishes() {
        let (_d, store) = store();
        let mut stub = remote_detail("srv00002", "https://example.com/later", "");
        stub.article.parse_error = "参数错误".into();
        let stub = store.save_pulled(&stub, "http://127.0.0.1:9").unwrap();
        assert_eq!(stub.article.parse_status, "failed");
        assert_eq!(stub.article.parse_error, "参数错误");
        assert_eq!(stub.article.word_count, 0);
        // 空壳立即进入本机重试队列，而不是等下一次同步。
        assert_eq!(
            store.due_retry_ids(10).unwrap(),
            vec![stub.article.id.clone()]
        );

        let full = remote_detail("srv00002", "https://example.com/later", "<p>补好的正文</p>");
        let filled = store.save_pulled(&full, "http://127.0.0.1:9").unwrap();
        assert_eq!(filled.article.id, stub.article.id);
        assert_eq!(filled.article.parse_status, "ok");
        assert_eq!(filled.content_html, "<p>补好的正文</p>");
        assert_eq!(filled.article.word_count, 100);
    }

    #[test]
    fn failed_parses_back_off_and_stop_after_the_cap() {
        let (_d, store) = store();
        let detail = store.add_pending("https://example.com/broken").unwrap();
        let id = detail.article.id.clone();

        store.mark_failed(&id, "第一次失败").unwrap();
        // 刚失败时不到期；一分钟后才进入重试队列。
        assert!(store.due_retry_ids(10).unwrap().is_empty());
        assert_eq!(
            store.due_retry_ids_at(now_ms() + 60_001, 10).unwrap(),
            vec![id.clone()]
        );

        store.mark_failed(&id, "第二次失败").unwrap();
        // 第二次退避 5 分钟。
        assert!(store
            .due_retry_ids_at(now_ms() + 60_001, 10)
            .unwrap()
            .is_empty());

        store.mark_failed(&id, "第三次失败").unwrap();
        // 到达上限即终态：再过一小时也不会被自动重试。
        assert!(store
            .due_retry_ids_at(now_ms() + 60 * 60_000, 10)
            .unwrap()
            .is_empty());
        assert_eq!(store.list().unwrap()[0].parse_status, "failed");

        // 手动重试清零计数，重新获得完整预算。
        store.mark_pending(&id).unwrap();
        assert_eq!(store.list().unwrap()[0].parse_status, "pending");
        store.mark_failed(&id, "手动重试后又失败").unwrap();
        assert_eq!(
            store.due_retry_ids_at(now_ms() + 60_001, 10).unwrap(),
            vec![id]
        );
    }

    #[test]
    fn pulled_images_are_copied_from_the_sync_server() {
        let png = b"\x89PNG\r\n\x1a\nfake-image";
        let base = serve_once(png, "image/png");
        let (_d, store) = store();
        let detail = remote_detail(
            "srv00004",
            "https://example.com/img",
            r#"<p>带图正文</p><img src="images/1.jpg">"#,
        );
        let saved = store.save_pulled(&detail, &base).unwrap();
        assert!(saved.content_html.contains(r#"src="images/1.jpg""#));
        let file = Path::new(&saved.image_dir).join("1.jpg");
        assert_eq!(fs::read(file).unwrap(), png);
    }

    #[test]
    fn pulled_images_fall_back_to_the_server_url_when_the_copy_fails() {
        let (_d, store) = store();
        let detail = remote_detail(
            "srv00003",
            "https://example.com/img2",
            r#"<p>带图正文</p><img src="images/1.jpg">"#,
        );
        let saved = store.save_pulled(&detail, "http://127.0.0.1:9").unwrap();
        assert!(saved
            .content_html
            .contains(r#"src="http://127.0.0.1:9/images/srv00003/1.jpg""#));
    }

    /// Minimal one-shot-per-connection HTTP server; returns its base URL.
    fn serve_once(body: &'static [u8], content_type: &'static str) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut buffer = [0u8; 1024];
                let _ = stream.read(&mut buffer);
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.write_all(body);
            }
        });
        format!("http://{address}")
    }
}
