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
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use regex::Regex;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Error, Result};

const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.4 Safari/605.1.15";
const MAX_IMAGES: usize = 60;
const MAX_IMAGE_BYTES: usize = 12 * 1024 * 1024;
const MAX_PAGE_BYTES: usize = 24 * 1024 * 1024;

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
             CREATE INDEX IF NOT EXISTS annotations_article ON annotations(article_id, start);",
        )?;
        connection.execute_batch("PRAGMA foreign_keys = ON;")?;
        Ok(connection)
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
                    a.read_at, (SELECT COUNT(*) FROM annotations n WHERE n.article_id = a.id)
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
                        a.read_at, (SELECT COUNT(*) FROM annotations n WHERE n.article_id = a.id)
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
        Ok(ReadingArticleDetail {
            image_dir: self.image_dir(&article.id).to_string_lossy().into_owned(),
            article,
            content_html,
            markdown,
            annotations,
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
        })
    }
}

// ── Fetching ────────────────────────────────────────────────────────────────

struct FetchedPage {
    html: String,
    final_url: String,
}

/// Only absolute web URLs are accepted; anything else could reach local files.
fn normalize_url(url: &str) -> Result<String> {
    let trimmed = url.trim();
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return Err(Error::InvalidReadingUrl("只能添加 http/https 链接".into()));
    }
    let parsed = reqwest::Url::parse(trimmed)
        .map_err(|error| Error::InvalidReadingUrl(format!("链接无效：{error}")))?;
    if parsed.host_str().is_none() {
        return Err(Error::InvalidReadingUrl("链接缺少主机名".into()));
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
    reqwest::blocking::Client::builder()
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
    let response = client
        .get(url)
        .header("Accept", "text/html,application/xhtml+xml,*/*;q=0.8")
        .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
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
    let options = readabilityrs::ReadabilityOptions::builder()
        .output_markdown(true)
        .sanitize_content(true)
        .build();
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
    let raw_content = article
        .as_ref()
        .and_then(|item| item.content.clone())
        .filter(|content| !content.trim().is_empty())
        .or_else(|| fallback.clone())
        .ok_or_else(|| Error::ReadingExtract("没有找到可阅读的正文".into()))?;
    let title_from_article = article.as_ref().and_then(|item| item.title.clone());
    let standardized =
        readabilityrs::elements::standardize_all(&raw_content, title_from_article.as_deref());
    let content_html = sanitize_html(&standardized);
    let text = article
        .as_ref()
        .and_then(|item| item.text_content.clone())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| html_to_text(&raw_content));
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
    let site_name = if host == "mp.weixin.qq.com" && byline.is_some() {
        byline.clone().unwrap_or_default()
    } else {
        article
            .as_ref()
            .and_then(|item| item.site_name.clone())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| host.clone())
    };
    let title = article
        .as_ref()
        .and_then(|item| item.title.clone())
        .map(|title| title.trim().to_string())
        .filter(|title| !title.is_empty())
        .or_else(|| html_title(&html))
        .unwrap_or_else(|| host.clone());
    let excerpt = article
        .as_ref()
        .and_then(|item| item.excerpt.clone())
        .map(|excerpt| collapse_whitespace(&excerpt))
        .filter(|excerpt| !excerpt.is_empty())
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
        markdown: article
            .as_ref()
            .and_then(|item| item.markdown_content.clone())
            .filter(|value| !value.trim().is_empty()),
        word_count,
        reading_minutes,
    })
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
    let patterns = [
        r#"(?is)<div[^>]*id=["']js_content["'][^>]*>(.*?)</div>"#,
        r#"(?is)<div[^>]*class=["'][^"']*rich_media_content[^"']*["'][^>]*>(.*?)</div>"#,
    ];
    patterns.iter().find_map(|pattern| {
        Regex::new(pattern)
            .ok()?
            .captures(html)
            .and_then(|captures| {
                let body = captures.get(1)?.as_str().trim();
                (html_to_text(body).chars().count() >= 20).then(|| body.to_string())
            })
    })
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
    if let Ok(hidden) = Regex::new(r#"(?i)visibility\s*:\s*hidden\s*;?"#) {
        result = hidden.replace_all(&result, "").into_owned();
    }
    result
}

fn sanitize_html(html: &str) -> String {
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
        .link_rel(Some("noopener noreferrer"))
        .url_relative(ammonia::UrlRelative::PassThrough);
    builder.clean(html).to_string()
}

fn html_title(html: &str) -> Option<String> {
    Regex::new(r"(?is)<title[^>]*>(.*?)</title>")
        .ok()?
        .captures(html)
        .and_then(|captures| captures.get(1))
        .map(|m| collapse_whitespace(&decode_entities(m.as_str())))
        .filter(|title| !title.is_empty())
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
    fn wechat_error_shell_is_not_misreported_as_empty_article() {
        let html = r#"<div class="weui-msg__title warn">参数错误</div>"#;
        assert!(is_wechat_error_shell(html));
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
}
