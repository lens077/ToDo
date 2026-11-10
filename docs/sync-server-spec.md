# virgo-sync：链接库自托管同步/解析服务规格

日期：2026-11-11
状态：MVP 实施中
关联：决策③（移动端是同步副本消费者）、链接保存需求设计（2026-11-11 会话）、`docs/reading-context-audit.md`

## 1. 定位

单用户自托管服务，跑在用户自己的海外服务器（k1 集群），是链接保存需求的 B 层：

1. **同步**：桌面端保存的链接/备注/进度，多端（Android 壳、网页端）近实时可见；
2. **解析**：复用 `todo-core` 的 `ReadingStore` 全套解析（X/微信/readability），为没有本地解析能力的端兜底；
3. **不做**：多用户、TODO.md 项目数据同步（链接库与项目文件严格分离，同 ADR-0003 边界）、离线冲突合并（后置）。

拍板记录：数据库用 **SQLite**（单用户足够，`ReadingStore` 原生支持自定义根目录）；网页端走**独立小前端**（不进 `web/` 落地站构建）。

## 2. 核心复用决策

`ReadingStore::new(root)` 把整个阅读库（SQLite + 图片目录）落到任意根目录——服务端就是「一个没有桌面的 ReadingStore 持有者」，解析/存储/备注/进度逻辑零复制：

| 服务端能力 | 复用的 todo-core API |
| --- | --- |
| 快速保存（先收藏后解析） | `add_failed(url)` + 后台 `refetch(id)` |
| 完整解析 | `add(url)` / `refetch(id)`（X → fxtwitter 兜底、微信 fallback、readability） |
| 备注/收藏原因 | `update_context(id, ReadingContextInput)` |
| 进度/已读 | `set_progress` / `set_read` |
| 删除 | `remove(id)`（含图片清理） |
| 图片 | `root/images/<id>/` 直接静态服务 |

## 3. API（v1，Bearer token 鉴权）

| 方法/路径 | 语义 |
| --- | --- |
| `GET /healthz` | 存活探针（免鉴权，k8s probe 用） |
| `GET /articles` | **全量**列表（单用户个人库规模下，全量即增量） |
| `GET /articles/{id}` | 详情（含 content_html / annotations / context） |
| `POST /links` | 快速保存 `{url, context?}` → 立即返回 stub，后台解析（收藏优先语义） |
| `POST /articles/{id}/refetch` | 重新解析 |
| `POST /articles/{id}/context` | 写备注/收藏原因/重读安排（解析不覆盖的唯一写路径） |
| `POST /articles/{id}/progress` | `{percent, anchor?}` |
| `POST /articles/{id}/read` | `{read}` |
| `DELETE /articles/{id}` | 删除 |
| `GET /stream` | SSE `event: change`（任何变更广播一次 ping） |
| `GET /images/...` | 文章图片静态服务 |

**同步模型（v1 = 全量对账 + SSE 触发）**：客户端连上/收到 SSE ping → `GET /articles` 全量拉取 → 以服务端为准按 id 集合 diff 出增删改。删除靠 id 集合差集推断，不需要墓碑表。个人库数百条时 payload 可控；>5k 条再考虑 cursor 增量（触发条件记录于此）。

**离线写入（v1 不做）**：客户端离线期间的保存/备注暂不入队；Android 分享接收在无网时提示重试。

## 4. 鉴权与部署

- `Authorization: Bearer <VIRGO_SYNC_TOKEN>`，环境变量注入（k8s Secret），缺失直接拒启；`/healthz` 豁免。
- 环境变量：`VIRGO_SYNC_DATA_DIR`（必填）、`VIRGO_SYNC_TOKEN`（必填）、`VIRGO_SYNC_BIND`（默认 `127.0.0.1:8090`，k8s 里由 Service 暴露）。
- 部署（2026-11-11 拍板）：`todo-sync.apikv.com`，k1 集群 `todo` namespace，PVC 落库，随机 token 走 `todo-sync-token` Secret。chart 在 `apps/sync-server/deploy/chart/todo-sync`（沿用 sponsor-api 模式：HTTPRoute→cilium-gateway、NetworkPolicy、非 root）；Makefile 目标 `sync-secret` / `sync-image` / `sync-k8s-deploy`。ArgoCD 注册待首跑稳定后补。

## 5. 实施切分

| 阶段 | 内容 | 状态 |
| --- | --- | --- |
| B1 | `crates/sync-server`：API + 鉴权 + SSE + 单测 | 本次 |
| B2 | k8s 部署（镜像 + helm values + Secret） | 待用户给部署窗口 |
| C1 | 桌面端同步模块（指向服务器 + SSE 监听） | 待 B2 |
| C2 | 网页端独立小前端（列表/搜索/快速保存/阅读） | 待 B2 |
| D2 | Android 壳接服务器 API + 分享接收 | 待 C2 |

## 6. 已知边界

- `add_failed` 进库即标记 `failed`，后台 `refetch` 成功后转正常——保存响应永远是毫秒级 stub，正文异步补齐（客户端刷新即得）。
- token 比较非常数时间（单用户低风险，记录在案；后续可换 `subtle`）。
- SSE 断线重连靠客户端全量重拉，无需 cursor 恢复语义。
- 微信风控：海外服务器解析 `mp.weixin.qq.com` 偶发受限，失败记录保持可打开原文链接，桌面端本地解析不受影响。
