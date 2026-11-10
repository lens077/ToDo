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

# 网页端小前端：pnpm workspace 过滤安装后构建，产物由运行时静态托管。
FROM node:26-alpine AS web
WORKDIR /repo
COPY pnpm-workspace.yaml pnpm-lock.yaml package.json ./
# 过滤安装与 frozen lockfile 校验要求全部 workspace 成员的 manifest 在位。
COPY apps/sync-web/package.json apps/sync-web/
COPY apps/desktop/package.json apps/desktop/
COPY apps/sponsor-api/package.json apps/sponsor-api/
COPY web/package.json web/
# Node 25+ 不再内置 corepack，显式装 pnpm（版本与 packageManager 对齐）。
RUN npm i -g pnpm@12.15.0 \
    && pnpm install --frozen-lockfile --filter @sumery/sync-web
COPY apps/sync-web apps/sync-web
RUN pnpm --filter @sumery/sync-web build

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
# 与 sponsor-api 一致的非 root 约定（uid/gid 1000）。不能用 `sync`：
# 那是 Debian 自带系统账号。
RUN useradd --uid 1000 --create-home virgo && mkdir -p /data && chown virgo:virgo /data
COPY --from=build /build/target/release/virgo-sync /usr/local/bin/virgo-sync
COPY --from=web /repo/apps/sync-web/dist /app/web
USER virgo:virgo
WORKDIR /data
ENV VIRGO_SYNC_BIND=0.0.0.0:8090 \
    VIRGO_SYNC_DATA_DIR=/data \
    VIRGO_SYNC_WEB_DIR=/app/web
VOLUME ["/data"]
EXPOSE 8090
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD curl -fsS http://127.0.0.1:8090/healthz || exit 1
ENTRYPOINT ["/usr/local/bin/virgo-sync"]
