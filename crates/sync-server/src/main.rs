//! virgo-sync entry point: config from the environment, then serve.
//!
//! `VIRGO_SYNC_DATA_DIR` and `VIRGO_SYNC_TOKEN` are required — the server
//! refuses to start with an implicit empty token or an implicit data path.
//! `VIRGO_SYNC_BIND` defaults to loopback:8090; the k8s Service exposes it.

use std::{path::PathBuf, sync::Arc};

#[tokio::main]
async fn main() {
    let data_dir = std::env::var("VIRGO_SYNC_DATA_DIR").expect("VIRGO_SYNC_DATA_DIR 未设置");
    let token = std::env::var("VIRGO_SYNC_TOKEN").expect("VIRGO_SYNC_TOKEN 未设置");
    if token.is_empty() {
        panic!("VIRGO_SYNC_TOKEN 不能为空");
    }
    let bind = std::env::var("VIRGO_SYNC_BIND").unwrap_or_else(|_| "127.0.0.1:8090".to_string());

    let state = Arc::new(sync_server::SyncState::new(PathBuf::from(data_dir)));
    let app = sync_server::build_router(state, &token);
    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .unwrap_or_else(|error| panic!("无法绑定 {bind}：{error}"));
    eprintln!("virgo-sync listening on {bind}");
    axum::serve(listener, app)
        .await
        .expect("server runtime failed");
}
