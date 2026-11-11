//! 桌面阅读库 ↔ todo-sync 服务器全量对账（v2）。
//!
//! 同步只搬运数据，不再解析：服务器已经解析好的正文和图片由
//! `GET /articles/{id}` 一次性取回，经 `ReadingStore::save_pulled` 直接落库，
//! 客户端不会为了「同步」再去抓原站。解析只发生在本机新添加的链接上，失败后
//! 按退避重试（见 `todo_core::ReadingStore::mark_failed`）。
//!
//! 对账仍以 URL 为唯一键：本地有、服务器没有的上行（只传 URL + 上下文，正文
//! 由服务器解析）；服务器有、本地没有的下行；本地已有正文的条目不再重复拉取。
//! 已有条目的备注/进度互不覆盖（v1 冲突策略：创建时随行，之后各自独立）。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, TryLockError};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use todo_core::{ReadingArticle, ReadingArticleDetail, ReadingStore};

/// 单实例闸门：SSE 对账、手动对账、启动补推共用同一条路径。先到先跑，后来者
/// 直接跳过，避免同一批文章被并发拉取。
static SYNC_LOCK: Mutex<()> = Mutex::new(());
/// 取消标记：界面上的「取消」让当前这一轮在下一个安全点停下。
static CANCEL: AtomicBool = AtomicBool::new(false);
/// 下行详情的并发数。图片复制在 `save_pulled` 内部另有自己的并发池
/// （`MAX_IMAGE_WORKERS`），这里只把「请求-等正文」的网络往返重叠起来。
const MAX_PULL_WORKERS: usize = 4;

pub fn cancel() {
    CANCEL.store(true, Ordering::SeqCst);
}

fn cancelled() -> bool {
    CANCEL.load(Ordering::SeqCst)
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct SyncReport {
    pub pulled: usize,
    pub pushed: usize,
    /// 已有一轮同步在跑，本轮没有做事。
    pub skipped: bool,
    /// 本轮在完成前被取消。
    pub cancelled: bool,
    pub errors: Vec<String>,
}

/// 进度：界面只显示当前阶段和“第几篇/共几篇”，不做百分比承诺。
#[derive(Clone, Debug, Default, Serialize)]
pub struct SyncProgress {
    /// `reconcile` | `push` | `pull` | `done`
    pub stage: &'static str,
    /// 已处理的条数（从 0 开始）。
    pub done: usize,
    pub total: usize,
    /// 正在处理的条目标题或地址。
    pub label: String,
}

#[derive(Deserialize)]
struct ServerList {
    articles: Vec<ReadingArticle>,
}

fn context_json(input: &todo_core::ReadingContext) -> serde_json::Value {
    serde_json::json!({
        "saved_reason": input.saved_reason,
        "focus_questions": input.focus_questions,
        "revisit_condition": input.revisit_condition,
        "personal_notes": input.personal_notes,
        "revisit_type": input.revisit_type,
    })
}

pub fn run(store: &ReadingStore, base: &str, token: &str) -> Result<SyncReport, String> {
    run_with_progress(store, base, token, &|_| {})
}

/// 与 [`run`] 相同，但在阶段切换和每篇下行前后回调进度；下行是并发的，
/// 回调可能从多个 worker 线程触发（`F` 需为 `Sync`）。
pub fn run_with_progress<F>(
    store: &ReadingStore,
    base: &str,
    token: &str,
    on_progress: &F,
) -> Result<SyncReport, String>
where
    F: Fn(SyncProgress) + Sync + ?Sized,
{
    let _guard = match SYNC_LOCK.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::WouldBlock) => {
            return Ok(SyncReport {
                skipped: true,
                ..SyncReport::default()
            })
        }
        Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
    };
    CANCEL.store(false, Ordering::SeqCst);

    let base = base.trim().trim_end_matches('/');
    let client = todo_core::http_client_builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?;
    let mut report = SyncReport::default();

    on_progress(SyncProgress {
        stage: "reconcile",
        done: 0,
        total: 0,
        label: "读取服务器清单".into(),
    });
    let server_list: ServerList = client
        .get(format!("{base}/articles"))
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .map_err(|error| format!("连接服务器失败：{error}"))?
        .error_for_status()
        .map_err(|error| format!("服务器拒绝访问（检查 token）：{error}"))?
        .json()
        .map_err(|error| error.to_string())?;
    let server: HashMap<String, ReadingArticle> = server_list
        .articles
        .into_iter()
        .map(|article| (article.url.clone(), article))
        .collect();

    let local_list = store.list().map_err(|error| error.to_string())?;
    let local: HashMap<String, ReadingArticle> = local_list
        .into_iter()
        .map(|article| (article.url.clone(), article))
        .collect();

    // 上行：本地有、服务器没有 → stub + context（服务器后台解析）。
    let mut local_only: Vec<&ReadingArticle> = local
        .values()
        .filter(|article| !server.contains_key(&article.url))
        .collect();
    local_only.sort_by_key(|article| std::cmp::Reverse(article.added_at));
    for (index, article) in local_only.iter().enumerate() {
        if cancelled() {
            report.cancelled = true;
            break;
        }
        on_progress(SyncProgress {
            stage: "push",
            done: index,
            total: local_only.len(),
            label: article.url.clone(),
        });
        let url = &article.url;
        let detail = match store.get(&article.id) {
            Ok(detail) => detail,
            Err(error) => {
                report.errors.push(format!("读取本地 {url}：{error}"));
                continue;
            }
        };
        let body = serde_json::json!({ "url": url, "context": context_json(&detail.context) });
        let outcome = client
            .post(format!("{base}/links"))
            .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
            .json(&body)
            .send();
        match outcome {
            Ok(response) if response.status().is_success() => report.pushed += 1,
            Ok(response) => {
                report
                    .errors
                    .push(format!("上行 {url}：{}", response.status()));
            }
            Err(error) => report.errors.push(format!("上行 {url}：{error}")),
        }
    }

    // 下行：服务器有、本地没有；或本地只有空壳而服务器已经解析出正文。
    // 正文与图片直接取自服务器，本地不再重抓原站。详情请求用一个小
    // worker 池并发跑（冷同步时大头是网络往返），入库各自开连接、由
    // SQLite 的 WAL + busy_timeout 兜底串行化。
    let mut pending: Vec<&ReadingArticle> = server
        .values()
        .filter(|article| match local.get(&article.url) {
            None => true,
            Some(existing) => existing.word_count == 0 && article.word_count > 0,
        })
        .collect();
    pending.sort_by_key(|article| std::cmp::Reverse(article.added_at));

    let pulled = AtomicUsize::new(0);
    let completed = AtomicUsize::new(0);
    let next = AtomicUsize::new(0);
    let stopped = AtomicBool::new(false);
    let pull_errors = Mutex::new(Vec::new());
    let workers = pending.len().min(MAX_PULL_WORKERS);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                if cancelled() {
                    stopped.store(true, Ordering::SeqCst);
                    return;
                }
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(article) = pending.get(index) else {
                    return;
                };
                let url = &article.url;
                on_progress(SyncProgress {
                    stage: "pull",
                    done: completed.load(Ordering::Relaxed),
                    total: pending.len(),
                    label: if article.title.is_empty() {
                        article.url.clone()
                    } else {
                        article.title.clone()
                    },
                });
                let detail: ReadingArticleDetail = match client
                    .get(format!("{base}/articles/{}", article.id))
                    .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
                    .send()
                {
                    Ok(response) => match response.error_for_status() {
                        Ok(response) => match response.json() {
                            Ok(detail) => detail,
                            Err(error) => {
                                pull_errors
                                    .lock()
                                    .unwrap()
                                    .push(format!("下行 {url}：{error}"));
                                continue;
                            }
                        },
                        Err(error) => {
                            pull_errors.lock().unwrap().push(format!(
                                "下行 {url}：{}",
                                error.status().unwrap_or_default()
                            ));
                            continue;
                        }
                    },
                    Err(error) => {
                        pull_errors
                            .lock()
                            .unwrap()
                            .push(format!("下行 {url}：{error}"));
                        continue;
                    }
                };
                if let Err(error) = store.save_pulled(&detail, base) {
                    pull_errors
                        .lock()
                        .unwrap()
                        .push(format!("入库 {url}：{error}"));
                    continue;
                }
                pulled.fetch_add(1, Ordering::Relaxed);
                completed.fetch_add(1, Ordering::Relaxed);
            });
        }
    });
    report.pulled = pulled.load(Ordering::Relaxed);
    report.cancelled = stopped.load(Ordering::SeqCst);
    report.errors.extend(pull_errors.into_inner().unwrap());

    if !report.cancelled {
        on_progress(SyncProgress {
            stage: "done",
            done: pending.len(),
            total: pending.len(),
            label: String::new(),
        });
    }

    if report.errors.len() > 5 {
        report.errors.truncate(5);
        report.errors.push("……（其余错误已省略）".to_string());
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_sync_is_skipped_while_one_is_running() {
        let guard = SYNC_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = ReadingStore::new(dir.path().join("reading"));
        // 先到的那轮还占着闸门：这一轮必须直接返回，不碰网络。
        let report = run(&store, "http://127.0.0.1:9", "token").unwrap();
        assert!(report.skipped);
        assert_eq!(report.pulled, 0);
        assert_eq!(report.pushed, 0);
        drop(guard);
    }

    fn server_article_json(index: usize) -> serde_json::Value {
        serde_json::json!({
            "id": format!("srv{index:05}"),
            "url": format!("https://example.com/pull/{index}"),
            "title": format!("并发 {index}"),
            "site_name": "example",
            "excerpt": "",
            "byline": null,
            "cover_path": null,
            "word_count": 100,
            "reading_minutes": 1,
            "added_at": 1_700_000_000_000i64 + index as i64,
            "last_opened_at": null,
            "progress_percent": 0,
            "progress_anchor": null,
            "read_at": null,
            "annotation_count": 0,
            "parse_status": "ok",
            "parse_error": ""
        })
    }

    fn server_detail_json(index: usize) -> serde_json::Value {
        serde_json::json!({
            "id": format!("srv{index:05}"),
            "url": format!("https://example.com/pull/{index}"),
            "title": format!("并发 {index}"),
            "site_name": "example",
            "excerpt": "",
            "byline": null,
            "cover_path": null,
            "word_count": 100,
            "reading_minutes": 1,
            "added_at": 1_700_000_000_000i64 + index as i64,
            "last_opened_at": null,
            "progress_percent": 0,
            "progress_anchor": null,
            "read_at": null,
            "annotation_count": 0,
            "parse_status": "ok",
            "parse_error": "",
            "content_html": format!("<p>正文 {index}</p>"),
            "markdown": null,
            "image_dir": "",
            "annotations": []
        })
    }

    /// 并发下拉：worker 池必须把每篇详情拉且只拉一次，第二轮零变更。
    #[test]
    fn concurrent_pull_fetches_each_detail_exactly_once() {
        use std::io::{Read, Write};

        const COUNT: usize = 6;
        let detail_hits: &'static AtomicUsize = Box::leak(Box::new(AtomicUsize::new(0)));

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut buffer = [0u8; 2048];
                let Ok(read) = stream.read(&mut buffer) else {
                    return;
                };
                let request = String::from_utf8_lossy(&buffer[..read]);
                let path = request.split_whitespace().nth(1).unwrap_or_default();
                let (status, body) = if path == "/articles" {
                    let items: Vec<String> = (0..COUNT)
                        .map(|index| server_article_json(index).to_string())
                        .collect();
                    ("200 OK", format!(r#"{{"articles":[{}]}}"#, items.join(",")))
                } else if let Some(id) = path.strip_prefix("/articles/") {
                    match id.trim_start_matches("srv").parse::<usize>() {
                        Ok(index) => {
                            detail_hits.fetch_add(1, Ordering::Relaxed);
                            ("200 OK", server_detail_json(index).to_string())
                        }
                        Err(_) => ("404 Not Found", r#"{"error":"not found"}"#.to_string()),
                    }
                } else {
                    ("404 Not Found", r#"{"error":"not found"}"#.to_string())
                };
                let header = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.write_all(body.as_bytes());
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let store = ReadingStore::new(dir.path().join("reading"));
        let base = format!("http://{address}");

        let cold = run(&store, &base, "token").unwrap();
        assert_eq!(cold.pulled, COUNT, "冷同步应拉完全部：{:?}", cold.errors);
        assert_eq!(cold.errors.len(), 0);
        assert_eq!(
            detail_hits.load(Ordering::Relaxed),
            COUNT,
            "每篇详情拉且只拉一次"
        );
        let rows = store.list().unwrap();
        assert_eq!(rows.len(), COUNT);
        assert!(rows
            .iter()
            .all(|row| row.word_count > 0 && row.parse_status == "ok"));

        let warm = run(&store, &base, "token").unwrap();
        assert_eq!(warm.pulled, 0, "第二轮不该再下行");
        assert_eq!(
            detail_hits.load(Ordering::Relaxed),
            COUNT,
            "第二轮零详情请求"
        );
    }

    /// 对着真实服务器跑两轮：第一轮把远端库拉进空目录，第二轮应该零变更且
    /// 在秒级完成。默认忽略，验收时用环境变量打开：
    ///
    /// ```text
    /// TODO_SYNC_BASE=https://todo-sync.apikv.com TODO_SYNC_TOKEN=*** \
    ///   cargo test -p todo-desktop live_sync_reuses_server_bodies -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn live_sync_reuses_server_bodies() {
        let (Ok(base), Ok(token)) = (
            std::env::var("TODO_SYNC_BASE"),
            std::env::var("TODO_SYNC_TOKEN"),
        ) else {
            eprintln!("TODO_SYNC_BASE / TODO_SYNC_TOKEN 未设置，跳过");
            return;
        };
        if base.trim().is_empty() || token.trim().is_empty() {
            eprintln!("TODO_SYNC_BASE / TODO_SYNC_TOKEN 为空，跳过");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let store = ReadingStore::new(dir.path().join("reading"));

        let started = std::time::Instant::now();
        let cold = run(&store, &base, &token).unwrap();
        let cold_elapsed = started.elapsed();

        let started = std::time::Instant::now();
        let warm = run(&store, &base, &token).unwrap();
        let warm_elapsed = started.elapsed();
        println!(
            "cold: {cold_elapsed:?} pulled={} pushed={} errors={:?}",
            cold.pulled, cold.pushed, cold.errors
        );
        println!(
            "warm: {warm_elapsed:?} pulled={} pushed={} errors={:?}",
            warm.pulled, warm.pushed, warm.errors
        );

        assert_eq!(warm.pulled, 0, "第二轮不该再下行");
        assert_eq!(warm.pushed, 0, "第二轮不该再上行");
        assert!(
            warm_elapsed < Duration::from_secs(5),
            "无变更的同步耗时 {warm_elapsed:?}，应在一秒级"
        );
        assert_eq!(warm.errors.len(), 0, "第二轮不该有错误");
    }
}
