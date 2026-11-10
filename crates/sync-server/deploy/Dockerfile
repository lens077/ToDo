# virgo-sync 镜像：Rust workspace 内构建 sync-server，运行时只带二进制。
# 构建上下文是仓库根目录（workspace 清单 + crates/ + src-tauri 成员声明）：
#   docker buildx build --platform linux/amd64 \
#     -f crates/sync-server/deploy/Dockerfile -t <image> .
FROM rust:1-bookworm AS build
WORKDIR /build
# 先拷清单与 lockfile，依赖层可缓存；rust-toolchain.toml 固定工具链版本。
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
# workspace 成员必须全部在位。src-tauri 的 lib path 在 manifest 里声明，
# 只拷清单会导致解析失败，所以要整个目录（源码小，不构建它）。
COPY crates/todo-core crates/todo-core
COPY crates/todo-cli crates/todo-cli
COPY crates/sync-server crates/sync-server
COPY apps/desktop/src-tauri apps/desktop/src-tauri
RUN cargo build --release --locked -p sync-server

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
# 与 sponsor-api 一致的非 root 约定（uid/gid 1000）。不能用 `sync`：
# 那是 Debian 自带系统账号。
RUN useradd --uid 1000 --create-home virgo && mkdir -p /data && chown virgo:virgo /data
COPY --from=build /build/target/release/virgo-sync /usr/local/bin/virgo-sync
USER virgo:virgo
WORKDIR /data
ENV VIRGO_SYNC_BIND=0.0.0.0:8090 \
    VIRGO_SYNC_DATA_DIR=/data
VOLUME ["/data"]
EXPOSE 8090
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD curl -fsS http://127.0.0.1:8090/healthz || exit 1
ENTRYPOINT ["/usr/local/bin/virgo-sync"]
