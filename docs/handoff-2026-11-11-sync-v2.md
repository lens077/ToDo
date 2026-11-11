# 交接文档：同步 v2（A/B/C）——复用服务器正文 + 解析退避 + 进度/取消

> 日期：2026-11-11
> 关联：`docs/sync-server-spec.md`（§3 同步模型 v2）、`docs/verification-log.md`（2026-11-11 同步 v2 条目）、`docs/handoff-2026-11-11.md`（整体交接）
> 状态：代码完成、真实服务器验收通过、**尚未提交**

## 1. 一句话现状

同步不再解析：下行直接复用服务器已解析的正文与图片，失败解析有退避和次数上限，同步带
进度、取消与进程内单实例闸门。23 条 / 79 张图的库：首次 11.7s，无变更热同步 168ms
（旧实现空转 60–67s）。

## 2. 解决的问题与根因

| 现象 | 根因 | 处理 |
| --- | --- | --- |
| 一次「无变更」同步 60–67s | 每轮对 `word_count == 0` 的失败条目重新抓整页（30s 超时、最多 60 图、串行），没有退避/上限 | 同步不再解析；失败改为 1min/5min 退避 + 3 次终态 |
| 同一篇文章解析两次 | 服务器已解析，下行又 `refetch` 原站 | 下行取 `GET /articles/{id}` 的 `content_html` 直接落库 |
| PC 23 条、移动端 16 条 | 后台对账在别的视图拉到新条目，阅读视图未刷新，不是上传不完整 | `useReading` 订阅 `reading://synced` + 切回阅读强制 `refresh()` |
| 手动与 SSE 对账并发抓同一批 | 没有互斥 | `SYNC_LOCK.try_lock()`，后到的一轮返回 `skipped` |
| 同步期间没有反馈、不能停 | 无进度事件/取消入口 | `reading://sync-progress` + `reading_sync_cancel` |

## 3. 同步逻辑（v2）

入口 `apps/desktop/src-tauri/src/sync.rs`：

1. `SYNC_LOCK.try_lock()`；已有同步在跑 → 返回 `SyncReport { skipped: true }`，不碰网络。
2. 清空取消标记，`GET /articles` 拿全量清单（URL → `ReadingArticle`）。
3. 读本地 `store.list()`，得到 URL → 本地行。
4. **上行**：本地有、服务器没有 → `POST /links`，只传 `url` + 上下文，服务器后台解析。
5. **下行**：满足任一条件才拉详情——
   - 本地没有该 URL；
   - 本地 `word_count == 0` 且服务器 `word_count > 0`（空壳补正文）。
   → `GET /articles/{id}` → `store.save_pulled(&detail, base)`。
6. 每个阶段/每篇前回调进度；每篇前检查取消；错误最多保留 5 条。下行详情请求 4 路并发
   （`MAX_PULL_WORKERS`），进度里的 `done` 是已完成数；入库由 SQLite WAL + busy_timeout 串行兜底。

**不变量**：已有正文的本地行永不重拉；已有行的备注/进度不被服务器覆盖（v1 冲突策略）。

## 4. 数据与状态模型

### 新列（`ensure_columns` 自动迁移旧库）

| 列 | 含义 |
| --- | --- |
| `parse_attempts INTEGER NOT NULL DEFAULT 0` | 已失败次数 |
| `parse_retry_at INTEGER` | 下次自动重试时间（epoch ms）；`NULL` = 没安排 |

### 常量（`crates/todo-core/src/reading.rs`）

- `MAX_PARSE_ATTEMPTS = 3`（含第一次）
- `PARSE_RETRY_BACKOFF_MS = [60_000, 300_000]`：第 1、2 次失败后等待；第 3 次终态
- `MAX_IMAGE_WORKERS = 6`：从服务器复制图片的并发数

### `mark_failed` / 重试

- `mark_failed(id, reason)`：`attempts += 1`；未到上限则写 `parse_retry_at = now + backoff`，
  到上限写 `NULL`（终态）。
- `mark_pending(id)`：**手动**重试，清零计数，重新获得完整预算。
- `mark_retrying(id)`：**自动**重试，保留计数。
- `due_retry_ids(limit)`：`status='failed' AND attempts < 3 AND (retry_at IS NULL OR retry_at <= now)`。
- `replace_content` 成功时把计数与 `parse_retry_at` 清零。

### `save_pulled` 三种情况

1. URL 已存在且已有正文 → 原样返回（不重拉）。
2. URL 已存在但为空壳、服务器已有正文 → 只更新正文/元数据/状态，保留本地上下文与进度。
3. URL 不存在 → 用**服务器 id** 建行（id 冲突时才生成新 id），正文、图片、服务器上下文与进度
   一并落库。服务器也没有正文 → 以 `failed` 入库，`retry_at = now`，交给本机退避重试。

图片：服务器正文里的引用是 `images/<file>`，从 `{base}/images/{server_id}/{file}` 复制到
本地 `images/<local_id>/`；复制失败的引用改写成服务器绝对 URL（在线可看，不出现碎图）。
封面同一目录、同一逻辑。

## 5. 代码地图

| 位置 | 内容 |
| --- | --- |
| `crates/todo-core/src/reading.rs` | 新列、退避/上限、`save_pulled`、`due_retry_ids`、远端图片复制 |
| `apps/desktop/src-tauri/src/sync.rs` | v2 对账、`run_with_progress`、详情 4 路并发（`MAX_PULL_WORKERS`）、`SYNC_LOCK`、`cancel()`、`SyncReport`/`SyncProgress` |
| `apps/desktop/src-tauri/src/sync_watch.rs` | SSE 常驻；对账时转发 `reading://sync-progress` |
| `apps/desktop/src-tauri/src/reading.rs` | `reading_sync_now`（带 AppHandle 发进度）、`reading_sync_cancel`、`reading_resume_pending`（pending + 每次最多 3 条到期失败） |
| `apps/desktop/src-tauri/src/lib.rs` | 注册 `reading_sync_cancel` |
| `apps/desktop/src/lib/readingSync.ts` | `SyncReport{pulled,pushed,skipped,cancelled,errors}`、`SyncProgress`、`cancelSync`、`onProgress`、单行格式化 |
| `apps/desktop/src/components/ReadingView.tsx` | 面板进度、忙时「取消」、报告摘要 |
| `apps/desktop/src/hooks/useReading.ts` | `pulled > 0` 时刷新；订阅 `reading://synced` |
| `apps/desktop/src/App.tsx` | 切回阅读时强制刷新 |

命令与事件：

- `reading_sync_now({ base, token })`：前端参数不变；AppHandle 由 Tauri 注入。
- `reading_sync_cancel()`。
- `reading://sync-start` / `reading://sync-progress` / `reading://synced` / `reading://sync-error` / `reading://link`。

## 6. 验收证据

- 真实服务器（`https://todo-sync.apikv.com`，空目录，23 条 / 79 张图）：
  - 详情串行时首次下行 **11.7s**（2026-11-11）；改 4 路并发后 **9.15s**（2026-11-12），
    `pulled=23 pushed=0 errors=[]`
  - 紧接着无变更再跑 `pulled=0 pushed=0 errors=[]`，秒级内（当时 168ms，后续复测受网络
    波动约 1–2s）
- 单测：
  - `pulled_articles_reuse_the_server_body_without_refetching`
  - `a_bodyless_pull_is_filled_in_when_the_server_finishes`
  - `failed_parses_back_off_and_stop_after_the_cap`
  - `pulled_images_are_copied_from_the_sync_server`
  - `pulled_images_fall_back_to_the_server_url_when_the_copy_fails`
  - `a_second_sync_is_skipped_while_one_is_running`
  - `concurrent_pull_fetches_each_detail_exactly_once`（mock 服务器验证并发下拉拉且只拉一次）
- `cargo test -p todo-core --lib reading`：32 passed；`cargo test -p todo-desktop --lib`：15 passed
- `make check-quick`：绿（rust fmt+clippy+test / web build，228s）
- 用户已完成界面验收。

## 7. 人工验收步骤（复现用）

1. 启动 `pnpm --filter @todo/desktop tauri dev`（改 Rust 后需重启，HMR 只覆盖前端）。
2. 阅读 → 同步 → 立即对账：无变更应在 1 秒内返回「下行 0 · 上行 0」。
3. 空库/新设备：进度显示「正在取 n/23：标题」，中途点「取消」，已落库条目保留。
4. 必然失败的链接：自动重试最多 3 次后不再重试；「重试解析」清零后可再试。
5. 移动端需重新打 APK 才会带上本批逻辑：
   `pnpm tauri android build --apk --debug --target aarch64`，再 `adb install`。

## 8. 已知边界（重要）

1. **删除没有墓碑**：全量对账只做「有/无」对比。在服务器删掉一条后，任何仍持有该行的
   设备会在下一轮通过 `POST /links` 把它推回去（测试脏数据就是这样复活的）。要真正删除，
   需要删除所有设备，或后续加 tombstone。
2. **桌面只上行 URL + 上下文**：进度、标注、项目/任务关联仍不上传（v1 语义未变）。
   新下拉的条目会带服务器上的进度（初次创建时随行）。
3. **图片失败回退服务器 URL**：这部分图片离线不可见；正文本身始终本地。
4. **取消粒度是「一篇」**：正在下载的图片批次（最多 10s 超时）不会被打断。
5. **互斥是进程内**：同时开两个 App 实例仍可能并发；桌面目前按单实例使用。
6. **服务器 id 复用**：本地沿用服务器 id 以保持图片目录稳定；仅在同 id 已被别的 URL 占用时
   才生成新 id（正文里的图片引用是相对路径，换 id 也成立）。
7. 空壳下拉会触发本机最多 3 次退避重试（这是有意为之：服务器可能因地域风控失败，本机可能成功）。

## 9. 后续 TODO

1. ~~清理测试脏数据~~（已完成，2026-11-12）：服务器 `DELETE` ×9（清单 8 条 + `x.com/jack/status/20`，
   用户确认）、PC 本地 9 行与图片目录已删、手机已 `pm clear` 并装回新 APK；服务器与 PC 均
   剩 14 条且集合一致，见 `docs/verification-log.md` 2026-11-12 条目。剩人工步骤：手机打开
   App 首次同步、重启 `tauri dev`，三端计数应一致。
2. Android APK 已重新构建并安装（2026-11-12，含同步 v2 + 详情并发）；**真机复验**（进度/取消/
   热同步）待用户打开 App 完成。
3. 可选优化：~~冷启动详情并发拉取~~（已完成，11.7s → 9.15s）；删除加墓碑；进度上行；同步失败条目统计进报告。
4. 旧待办：`d3l8hn` 桌面 release 签名、CI 回写 image tag、`p7v2ld`/`w8s5rt`/`h4m9zc` 人工验收。

## 10. 常用命令

```bash
# 真实服务器两轮验收（默认 #[ignore]，不配环境变量会跳过）
TOKEN=$(kubectl -n todo get secret todo-sync-token -o jsonpath='{.data.VIRGO_SYNC_TOKEN}' | base64 -d)
TODO_SYNC_BASE=https://todo-sync.apikv.com TODO_SYNC_TOKEN="$TOKEN" \
  cargo test -p todo-desktop --lib live_sync_reuses_server_bodies -- --ignored --nocapture

# 门禁
make check-quick

# 本地库概况
sqlite3 ~/.todo/reading/reading.sqlite \
  "select count(*) 总数, sum(word_count=0) 空壳, sum(parse_status='failed') 失败 from articles;"

# Android 调试包
cd apps/desktop
pnpm tauri android build --apk --debug --target aarch64
adb install -r src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
```

## 11. 提交状态（交接时）

工作区未提交，涉及：

```
apps/desktop/src-tauri/src/{lib.rs,reading.rs,sync.rs,sync_watch.rs}
apps/desktop/src/{App.tsx,components/ReadingView.tsx,hooks/useReading.ts,lib/readingSync.ts}
crates/todo-core/src/reading.rs
docs/{sync-server-spec.md,verification-log.md}
```

另：`.github/workflows/rust-check.yml` 有一处 Tauri Linux 系统依赖的改动，不是本批同步工作，
提交时请确认是否一并带上。

## 12. 不要重踩

- 不要把 `store.refetch` 加回同步路径——那正是 60 秒空转的来源。
- 服务器正文的图片引用必须保持 `images/<file>` 形状；若 `todo-core` 的 `localize_images`
  命名变化，同步的 `localize_remote_images` 要同步改。
- `parse_retry_at IS NULL` 不代表「终态」：终态看 `parse_attempts >= MAX_PARSE_ATTEMPTS`；
  未到期的失败行也可能是 NULL（例如刚被 `mark_retrying`）。
- 新增 HTTP 客户端继续用 `todo_core::http_client_builder()`（Android 需要 webpki 根证书）。
- 同步详情请求 30s 超时，图片复制单独 10s 超时；不要再把两者混用同一个客户端。
