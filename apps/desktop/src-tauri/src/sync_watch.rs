//! SSE 驱动的自动对账与离线重试（v1.1）。
//!
//! 服务器 `GET /stream` 在每次变更后推一个 `change` 事件。这里保持一条长连接：
//! 连上（或重连上）先跑一次全量对账——离线期间本地写入的条目就是在这个时刻
//! 被推上服务器的，所以不需要额外的写入队列；之后每个事件触发一次对账。
//!
//! 连不上时按 5s → 10s → … → 60s 退避重试，并把状态推给界面。对账本身仍是
//! `sync::run` 的全量模型，重复执行是幂等的。

use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::sync;

/// 重连退避的起点与上限。
const BACKOFF_START: Duration = Duration::from_secs(5);
const BACKOFF_MAX: Duration = Duration::from_secs(60);
/// 两次对账之间的最小间隔：服务器「保存 + 后台解析完成」会在毫秒级连推两个
/// 事件，合并成一次对账。
const MIN_SYNC_GAP: Duration = Duration::from_secs(1);

/// 自动同步的连接状态，事件名 `reading://link`。
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkState {
    /// 没有配置服务器 / token。
    Idle,
    Connecting,
    Online,
    Offline,
}

#[derive(Clone, Debug, Serialize)]
pub struct LinkStatus {
    pub state: LinkState,
    /// 离线时距离下次重试的秒数；在线时为 0。
    pub retry_in: u64,
    pub error: Option<String>,
}

impl Default for LinkStatus {
    fn default() -> Self {
        Self {
            state: LinkState::Idle,
            retry_in: 0,
            error: None,
        }
    }
}

#[derive(Clone)]
struct Config {
    base: String,
    token: String,
}

/// 自动同步的共享状态：代次用来让旧线程在新配置生效后自行退出。
pub struct SyncWatchState {
    generation: Arc<AtomicU64>,
    status: Arc<Mutex<LinkStatus>>,
}

impl Default for SyncWatchState {
    fn default() -> Self {
        Self {
            generation: Arc::new(AtomicU64::new(0)),
            status: Arc::new(Mutex::new(LinkStatus::default())),
        }
    }
}

fn backoff_next(current: Duration) -> Duration {
    (current * 2).min(BACKOFF_MAX)
}

fn is_change_event(line: &str) -> bool {
    line.starts_with("data:")
}

/// 启动（或按新配置重启）自动同步。前端在配置完整时调用；重复调用是安全的，
/// 旧线程会在下一次检查代次时退出。
#[tauri::command]
pub fn reading_sync_watch_start(
    state: State<'_, SyncWatchState>,
    app: AppHandle,
    base: String,
    token: String,
) -> Result<(), String> {
    let base = base.trim().trim_end_matches('/').to_string();
    let token = token.trim().to_string();
    if base.is_empty() || token.is_empty() {
        return Err("需要服务器地址和 token".to_string());
    }

    let mine = state.generation.fetch_add(1, Ordering::SeqCst) + 1;
    {
        let mut status = state
            .status
            .lock()
            .map_err(|_| "sync status lock is poisoned".to_string())?;
        *status = LinkStatus {
            state: LinkState::Connecting,
            retry_in: 0,
            error: None,
        };
    }
    let _ = app.emit(
        "reading://link",
        LinkStatus {
            state: LinkState::Connecting,
            retry_in: 0,
            error: None,
        },
    );

    let generation = state.generation.clone();
    let status = state.status.clone();
    let config = Config { base, token };
    std::thread::Builder::new()
        .name("todo-sync-watch".to_string())
        .spawn(move || run(app, config, generation, mine, status))
        .map_err(|error| format!("无法启动同步线程：{error}"))?;
    Ok(())
}

/// 停止自动同步（界面断开配置时调用）。
#[tauri::command]
pub fn reading_sync_watch_stop(state: State<'_, SyncWatchState>) -> Result<(), String> {
    state.generation.fetch_add(1, Ordering::SeqCst);
    let mut status = state
        .status
        .lock()
        .map_err(|_| "sync status lock is poisoned".to_string())?;
    *status = LinkStatus::default();
    Ok(())
}

#[tauri::command]
pub fn reading_sync_watch_status(state: State<'_, SyncWatchState>) -> Result<LinkStatus, String> {
    state
        .status
        .lock()
        .map(|status| status.clone())
        .map_err(|_| "sync status lock is poisoned".to_string())
}

fn publish(app: &AppHandle, status: &Arc<Mutex<LinkStatus>>, next: LinkStatus) {
    let _ = app.emit("reading://link", next.clone());
    if let Ok(mut current) = status.lock() {
        *current = next;
    }
}

fn stale(generation: &Arc<AtomicU64>, mine: u64) -> bool {
    generation.load(Ordering::SeqCst) != mine
}

fn run(
    app: AppHandle,
    config: Config,
    generation: Arc<AtomicU64>,
    mine: u64,
    status: Arc<Mutex<LinkStatus>>,
) {
    let client = match todo_core::http_client_builder()
        .connect_timeout(Duration::from_secs(10))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            publish(
                &app,
                &status,
                LinkStatus {
                    state: LinkState::Offline,
                    retry_in: BACKOFF_MAX.as_secs(),
                    error: Some(error.to_string()),
                },
            );
            return;
        }
    };
    let mut backoff = BACKOFF_START;

    loop {
        if stale(&generation, mine) {
            return;
        }
        let response = client
            .get(format!("{}/stream", config.base))
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", config.token),
            )
            .send();

        match response {
            Ok(response) if response.status().is_success() => {
                backoff = BACKOFF_START;
                publish(
                    &app,
                    &status,
                    LinkStatus {
                        state: LinkState::Online,
                        retry_in: 0,
                        error: None,
                    },
                );
                // 连上的第一件事：把离线期间攒下的本地条目推上去。
                reconcile(&app, &config);

                let mut last = std::time::Instant::now();
                for line in BufReader::new(response).lines() {
                    if stale(&generation, mine) {
                        return;
                    }
                    match line {
                        Ok(line) if is_change_event(&line) => {
                            if last.elapsed() >= MIN_SYNC_GAP {
                                reconcile(&app, &config);
                                last = std::time::Instant::now();
                            }
                        }
                        Ok(_) => {}
                        // 服务器关掉长连接（滚动更新、网关超时）：直接重连。
                        Err(_) => break,
                    }
                }
            }
            Ok(response) => {
                let error = format!("服务器返回 {}", response.status());
                publish(
                    &app,
                    &status,
                    LinkStatus {
                        state: LinkState::Offline,
                        retry_in: backoff.as_secs(),
                        error: Some(error),
                    },
                );
            }
            Err(error) => {
                publish(
                    &app,
                    &status,
                    LinkStatus {
                        state: LinkState::Offline,
                        retry_in: backoff.as_secs(),
                        error: Some(error.to_string()),
                    },
                );
            }
        }

        // 退避等待，期间有新的 start/stop 就立刻退出。
        let slept = std::time::Instant::now();
        while slept.elapsed() < backoff {
            if stale(&generation, mine) {
                return;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        backoff = backoff_next(backoff);
    }
}

fn reconcile(app: &AppHandle, config: &Config) {
    let Some(store) = crate::reading::runtime_reading_store() else {
        let _ = app.emit(
            "reading://sync-error",
            "阅读库不可用：无法确定用户主目录".to_string(),
        );
        return;
    };
    let _ = app.emit("reading://sync-start", ());
    match sync::run(&store, &config.base, &config.token) {
        Ok(report) => {
            let _ = app.emit("reading://synced", report);
        }
        Err(error) => {
            let _ = app.emit("reading://sync-error", error);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_until_the_cap() {
        let mut step = BACKOFF_START;
        assert_eq!(step, Duration::from_secs(5));
        step = backoff_next(step);
        assert_eq!(step, Duration::from_secs(10));
        step = backoff_next(step);
        assert_eq!(step, Duration::from_secs(20));
        step = backoff_next(step);
        assert_eq!(step, Duration::from_secs(40));
        step = backoff_next(step);
        assert_eq!(step, Duration::from_secs(60));
        assert_eq!(backoff_next(step), Duration::from_secs(60));
    }

    #[test]
    fn only_sse_data_lines_trigger_a_sync() {
        assert!(is_change_event("data:1"));
        assert!(is_change_event("data: "));
        // 事件名、keep-alive 注释、空行都不该触发。
        assert!(!is_change_event("event: change"));
        assert!(!is_change_event(": keep-alive"));
        assert!(!is_change_event(""));
    }
}
