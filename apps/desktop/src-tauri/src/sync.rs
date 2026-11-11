//! 桌面阅读库 ↔ todo-sync 服务器全量对账（v1）。
//!
//! 同步模型与 `docs/sync-server-spec.md` 一致：URL 为唯一键的全量对账——
//! 本地有而服务器没有的上行（stub + context，服务器后台解析），服务器有
//! 而本地没有的下行（stub + context + 本地重解析，每轮上限内）。已有条目
//! 的备注/进度互不覆盖（v1 冲突策略：创建时随行，之后各自独立）。

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use todo_core::{ReadingContextInput, ReadingStore};

/// 每轮同步最多内联重解析的下载数量；超出的保持 stub，下一轮继续。
const REFETCH_BUDGET: usize = 10;

#[derive(Clone, Debug, Default, Serialize)]
pub struct SyncReport {
    pub pulled: usize,
    pub pushed: usize,
    pub refetched: usize,
    pub pending_parse: usize,
    pub errors: Vec<String>,
}

#[derive(Deserialize)]
struct ServerList {
    articles: Vec<ServerArticle>,
}

#[derive(Deserialize)]
struct ServerArticle {
    id: String,
    url: String,
}

#[derive(Deserialize)]
struct ServerDetail {
    context: ServerContext,
}

#[derive(Deserialize)]
struct ServerContext {
    saved_reason: String,
    focus_questions: String,
    revisit_condition: String,
    personal_notes: String,
    revisit_type: String,
}

impl From<ServerContext> for ReadingContextInput {
    fn from(context: ServerContext) -> Self {
        Self {
            saved_reason: context.saved_reason,
            focus_questions: context.focus_questions,
            revisit_condition: context.revisit_condition,
            personal_notes: context.personal_notes,
            revisit_type: context.revisit_type,
        }
    }
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
    let base = base.trim().trim_end_matches('/');
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?;
    let mut report = SyncReport::default();

    let server_list: ServerList = client
        .get(format!("{base}/articles"))
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .map_err(|error| format!("连接服务器失败：{error}"))?
        .error_for_status()
        .map_err(|error| format!("服务器拒绝访问（检查 token）：{error}"))?
        .json()
        .map_err(|error| error.to_string())?;
    let server: HashMap<String, String> = server_list
        .articles
        .into_iter()
        .map(|article| (article.url, article.id))
        .collect();

    let local_list = store.list().map_err(|error| error.to_string())?;
    let mut local: HashMap<String, String> = HashMap::new();
    let mut stubs: Vec<String> = Vec::new();
    for article in &local_list {
        local.insert(article.url.clone(), article.id.clone());
        if article.word_count == 0 {
            stubs.push(article.id.clone());
        }
    }

    // 上行：本地有、服务器没有 → stub + context（服务器后台解析）。
    for (url, id) in &local {
        if server.contains_key(url) {
            continue;
        }
        let detail = match store.get(id) {
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

    // 下行：服务器有、本地没有 → stub + context + 本地重解析。
    let mut refetch_budget = REFETCH_BUDGET;
    for (url, server_id) in &server {
        if local.contains_key(url) {
            continue;
        }
        let detail: ServerDetail = match client
            .get(format!("{base}/articles/{server_id}"))
            .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
            .send()
        {
            Ok(response) => match response.error_for_status() {
                Ok(response) => match response.json() {
                    Ok(detail) => detail,
                    Err(error) => {
                        report.errors.push(format!("下行 {url}：{error}"));
                        continue;
                    }
                },
                Err(error) => {
                    report.errors.push(format!(
                        "下行 {url}：{}",
                        error.status().unwrap_or_default()
                    ));
                    continue;
                }
            },
            Err(error) => {
                report.errors.push(format!("下行 {url}：{error}"));
                continue;
            }
        };
        let created = match store.add_failed(url) {
            Ok(created) => created,
            Err(error) => {
                report.errors.push(format!("入库 {url}：{error}"));
                continue;
            }
        };
        let local_id = created.article.id.clone();
        if let Err(error) = store.update_context(&local_id, &detail.context.into()) {
            report.errors.push(format!("备注 {url}：{error}"));
        }
        report.pulled += 1;
        if refetch_budget > 0 {
            refetch_budget -= 1;
            match store.refetch(&local_id) {
                Ok(_) => report.refetched += 1,
                Err(_) => report.pending_parse += 1,
            }
        } else {
            report.pending_parse += 1;
        }
    }

    // 重试：本地解析失败的 stub，本轮预算内继续重解析。
    for id in stubs.iter().take(refetch_budget) {
        if store.refetch(id).is_ok() {
            report.refetched += 1;
        }
    }

    if report.errors.len() > 5 {
        report.errors.truncate(5);
        report.errors.push("……（其余错误已省略）".to_string());
    }
    Ok(report)
}
