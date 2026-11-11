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

**桌面端自动对账（2026-11-11）**：桌面常驻线程保持 `/stream` 长连接（`Authorization: Bearer`），连上/重连上先跑一次全量对账，之后每个 `data:` 事件触发一次；1s 内的事件合并成一次，避免「保存 + 解析完成」连推导致重复对账。断线按 5s→10s→20s→40s→60s 退避，状态经 `reading://link` 推给界面；手动「立即对账」仍保留。

**同步模型 v2（2026-11-11）：同步不再解析。** 下行对「本地没有」或「本地仍是空壳而服务器已有正文」的条目取 `GET /articles/{id}`，把 `content_html` 和 `server/images/<id>/` 直接落库（`ReadingStore::save_pulled`，图片 6 路并发），不再为了同步重抓原站。解析只发生在本机新添加的链接上：

- 失败写 `parse_attempts` / `parse_retry_at`，退避 1 分钟、5 分钟，第 3 次失败即终态；
- 终态行不再自动重试，手动「重试解析」把计数清零；
- `reading_resume_pending` 只补跑到期的失败条目（每次最多 3 条）。

同步本身带单实例闸门（SSE 与手动对账不会并发抓同一批）、`reading://sync-progress` 进度（`reconcile` / `push` / `pull` / `done`，含「第 n/共 m」）与 `reading_sync_cancel` 取消（当前这一篇结束后停下，已完成的保留）。详情请求 4 路并发（2026-11-12，图片复制在 `save_pulled` 内部仍为 6 路）。实测 23 条 / 79 张图：串行详情首次 11.7s，4 路并发 9.2s；无变更再跑秒级内（网络波动时 1–2s，旧实现空转 60–67s）。

**离线写入**：v1 不另建写入队列——本地 SQLite 就是队列，全部本地写入先落盘；对账是全量幂等的，所以离线期间的保存/备注会在下一次连上（重连首跑）时自动补推。界面用「离线，Ns 后重试」表达这个状态，而不是提示失败。注意：离线期间 Android 分享接收仍要求网络（服务端负责抓正文），这一点未变。

## 4. 鉴权与部署

- `Authorization: Bearer <VIRGO_SYNC_TOKEN>`，环境变量注入（k8s Secret），缺失直接拒启；`/healthz` 豁免。
- 环境变量：`VIRGO_SYNC_DATA_DIR`（必填）、`VIRGO_SYNC_TOKEN`（必填）、`VIRGO_SYNC_BIND`（默认 `127.0.0.1:8090`，k8s 里由 Service 暴露）。
- 部署（2026-11-11 拍板）：`todo-sync.apikv.com`，k1 集群 `todo` namespace，PVC 落库，随机 token 走 `todo-sync-token` Secret。chart 在 `apps/sync-server/deploy/chart/todo-sync`（沿用 sponsor-api 模式：HTTPRoute→cilium-gateway、NetworkPolicy、非 root）；Makefile 目标 `sync-secret` / `sync-image` / `sync-k8s-deploy`。
- GitOps（2026-11-11）：已注册 ArgoCD Application `todo-sync-prod`（`deploy/argocd/application-todo-sync.yaml`，project `virgo`，目标命名空间 `todo`），`make sync-argo-apply` 一次性注册。之后只改 `values-prod.yaml` 的 `api.image.tag` 并推到 GitLab main，ArgoCD 自动同步；**不要再跑 `make sync-k8s-deploy`**，否则 helm 与 Argo 会互相覆盖。

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
