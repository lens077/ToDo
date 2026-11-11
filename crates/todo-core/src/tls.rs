//! HTTP 客户端的统一入口。
//!
//! reqwest 0.13 的 `rustls` 特性固定使用 `rustls-platform-verifier`。桌面端它
//! 走系统信任库，正合需要；Android 端则需要随包附带的 Kotlin 组件，本应用没有
//! 集成，于是第一次 HTTPS 请求就会在后台线程 panic（表现为「抓取任务中断：
//! event loop thread panicked」）。移动端改用随包的 webpki 公共根证书：连接
//! `todo-sync.apikv.com`、公众号原文等公网站点足够，且不依赖 JVM。
pub fn http_client_builder() -> reqwest::blocking::ClientBuilder {
    #[cfg(target_os = "android")]
    {
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        return reqwest::blocking::Client::builder().use_preconfigured_tls(config);
    }
    #[cfg(not(target_os = "android"))]
    reqwest::blocking::Client::builder()
}
