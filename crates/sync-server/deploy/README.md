# virgo-sync 可观测性环境变量

`virgo-sync` 使用 tracing-subscriber + OTel OTLP/HTTP 同时输出本地 JSON 日志与 traces/logs。

| 变量 | 说明 |
| :--- | :--- |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | collector 基址，例如 `http://otel-opentelemetry-collector.opentelemetry.svc.cluster.local:4318`。未设置时只输出 stdout，不尝试导出。 exporter 会自动追加 `/v1/logs` 与 `/v1/traces`。 |
| `OTEL_RESOURCE_ATTRIBUTES` | 标准 OTel resource 属性（如 `service.namespace=...`），由 SDK 自动读取；代码内置的 `service.name` / `service.namespace=todo` / `deployment.environment.name` 会覆盖同名项。 |
| `RUST_LOG` | EnvFilter 指令，默认 `info`（例：`virgo_sync=debug,tower_http=info`）。 |
| `DEPLOYMENT_ENVIRONMENT` | `deployment.environment.name`，默认 `dev`。 |

已设置的 resource 属性：

- `service.name = virgo-sync`
- `service.namespace = todo`
- `deployment.environment.name = <DEPLOYMENT_ENVIRONMENT 或 dev>`

启动仍然由 `VIRGO_SYNC_DATA_DIR` / `VIRGO_SYNC_TOKEN` / `VIRGO_SYNC_BIND` 控制，与可观测性配置无关。
