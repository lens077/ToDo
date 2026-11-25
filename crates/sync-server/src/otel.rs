//! OTel 接入：stdout（JSON）+ OTLP/HTTP 的 logs 与 traces。
//!
//! 环境变量：
//! - `OTEL_EXPORTER_OTLP_ENDPOINT`：collector 基址（如 `http://...:4318`）。未设置时只写 stdout。
//! - `OTEL_RESOURCE_ATTRIBUTES`：标准 resource 属性，由 SDK 自动读取并与下面的属性合并。
//! - `RUST_LOG`：EnvFilter 指令，默认 `info`。
//! - `DEPLOYMENT_ENVIRONMENT`：`deployment.environment.name`，默认 `dev`。

use opentelemetry::trace::TracerProvider as _;
use opentelemetry::KeyValue;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{LogExporter, SpanExporter};
use opentelemetry_sdk::{logs::SdkLoggerProvider, trace::SdkTracerProvider, Resource};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Registry};

/// 持有 provider，退出时 flush 批处理队列；返回值由 main 持有到进程结束。
pub struct OtelGuard {
    logger_provider: SdkLoggerProvider,
    tracer_provider: SdkTracerProvider,
}

impl Drop for OtelGuard {
    fn drop(&mut self) {
        let _ = self.logger_provider.shutdown();
        let _ = self.tracer_provider.shutdown();
    }
}

fn env_filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
}

fn stdout_only() {
    Registry::default()
        .with(env_filter())
        .with(tracing_subscriber::fmt::layer().json().with_target(false))
        .init();
}

/// 安装全局 subscriber。未配置 OTLP 端点或导出器构建失败时退化为纯 stdout。
pub fn init(service_name: &'static str, service_namespace: &'static str) -> Option<OtelGuard> {
    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .ok()
        .filter(|value| !value.is_empty());
    if endpoint.is_none() {
        stdout_only();
        return None;
    }

    let environment = std::env::var("DEPLOYMENT_ENVIRONMENT")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "dev".to_owned());
    let resource = Resource::builder()
        .with_service_name(service_name)
        .with_attributes([
            KeyValue::new("service.namespace", service_namespace),
            KeyValue::new("deployment.environment.name", environment),
        ])
        .build();

    let log_exporter = match LogExporter::builder().with_http().build() {
        Ok(exporter) => exporter,
        Err(error) => {
            eprintln!("virgo-sync: OTLP 日志导出器初始化失败，退回 stdout：{error}");
            stdout_only();
            return None;
        }
    };
    let span_exporter = match SpanExporter::builder().with_http().build() {
        Ok(exporter) => exporter,
        Err(error) => {
            eprintln!("virgo-sync: OTLP traces 导出器初始化失败，退回 stdout：{error}");
            stdout_only();
            return None;
        }
    };

    let logger_provider = SdkLoggerProvider::builder()
        .with_resource(resource.clone())
        .with_batch_exporter(log_exporter)
        .build();
    let tracer_provider = SdkTracerProvider::builder()
        .with_resource(resource)
        .with_batch_exporter(span_exporter)
        .build();

    Registry::default()
        .with(env_filter())
        .with(tracing_subscriber::fmt::layer().json().with_target(false))
        .with(OpenTelemetryTracingBridge::new(&logger_provider))
        .with(tracing_opentelemetry::layer().with_tracer(tracer_provider.tracer(service_name)))
        .init();

    Some(OtelGuard {
        logger_provider,
        tracer_provider,
    })
}
