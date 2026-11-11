# 交接文档：同步 v2 收尾——并发优化 + 脏数据清理 + APK 重装（2026-11-12）

> 日期：2026-11-12
> 关联：`docs/handoff-2026-11-11-sync-v2.md`（同步 v2 主交接）、`docs/sync-server-spec.md` §3、
> `docs/verification-log.md`（2026-11-12 三条目）
> 状态：全部完成并已提交；同步 v2 在三端（服务器/PC/手机）闭环验收通过

## 1. 一句话现状

同步 v2 已提交并在三端闭环：冷同步并发优化后 11.7s→9.15s，测试脏数据 9 条已从
三端彻底清除（服务器/PC/手机各剩 14 条真实收藏，URL 集合 diff 为空），新 debug APK
（含 v2）已装回手机并完成首次同步复验。

## 2. 本轮提交（本地 main，未 push）

| commit | 内容 |
| --- | --- |
| `b7f1b48` | feat(reading): 同步 v2 主体（服务器正文复用 + 退避 + 进度/取消） |
| `613f74c` | docs: 同步 v2 交接文档与 spec/verification-log |
| `8f33325` | docs(todo): 登记后续任务（t4gfbx 脏数据清理、0x16fl APK 重打） |
| `b4a14bd` | perf(reading): 下行详情 4 路并发（11.7s→9.15s） |
| `d8a3315` | docs: 脏数据清理与 APK 重装记录 |

另：CI 的 Tauri Linux 依赖改动此前已由另一会话以 `3820f46` 提交，不在本轮。

## 3. 并发优化（b4a14bd）

- `sync.rs` 下行从逐篇串行改为 `MAX_PULL_WORKERS = 4` 的 worker 池（`AtomicUsize`
  取号 + `thread::scope`，与 todo-core 图片复制池同一模式）。
- `run_with_progress` 增加 `F: Sync`；进度 `done` 语义改为**已完成数**；取消语义不变
  （worker 每篇前检查 `CANCEL`）。入库靠 SQLite WAL + `busy_timeout(5s)` 兜底，无需应用层写锁。
- 真实服务器 23 条/79 图：**11.7s → 9.15s**。提升有限的原因：图片复制仍在
  `save_pulled` 内逐篇 6 路执行，且篇数基数小；再快需要把图片复制也做跨篇合并，未做。
- 新单测 `concurrent_pull_fetches_each_detail_exactly_once`（本地 mock HTTP 服务器，
  断言每篇详情拉且只拉一次、第二轮零请求）。

## 4. 脏数据清理（t4gfbx，已完成）

**删除无墓碑下的安全顺序**（违反会复活）：

1. 停所有同步源（PC `tauri dev`、手机清数据）——持有行会推回服务器；
2. 删设备本地行；
3. 删服务器行；
4. 设备再上线，只拉到剩余的干净集合。

**实际执行**：

- 服务器 `DELETE /articles/{id}` ×9 全部 204：交接清单 8 条
  （`example.com/`、`clip-a`、`clip-b`、`dbg-one`、`rust-lang/cargo`、`quick-1..3`）
  + `x.com/jack/status/20`（清单外但明显测试链接，用户确认一并删）。
- PC 本地按 URL 删 9 行 + `images/<id>` 目录（App 进程残留先 kill 再操作）。
- 手机 `adb shell pm clear com.sumery.virgo` 清数据，装回新 APK。
- **保留未删**：`learnharness.git`、`diagram-design`（不在清单内且 PC 本地有真实正文）、
  `r92ya8bg9nbrm`（微信真实文章）。

**结果**：服务器 14 条无残留；PC 14 行（0 空壳 / 0 失败）；三端 URL 集合 diff 为空。

## 5. APK 重装与三端复验（0x16fl，已完成）

- 构建命令必须带 `.zshrc` 里的环境：`ANDROID_HOME=$HOME/android/sdk`、
  `NDK_HOME=$ANDROID_HOME/ndk/30.0.16138531`、`JAVA_HOME`=Android Studio JBR。
  裸跑报 "Skipping Android Studio command line tools installation"，且 **管道会把
  exit code 吞成假绿**（第一次构建实际失败却 exit 0）——用 `set -o pipefail`。
- vivo 安装需手机屏幕确认 USB 安装弹窗；`adb install` 被拒时重试并在手机上点允许。
- 三端复验（2026-11-12）：
  - 服务器 14 条；PC 14 行（0 空壳 / 0 失败）；手机 14 行，三端 URL 集合 diff 为空。
  - 手机首次同步拉 14 条（用户已见进度），PC 重启 dev 后热同步零变更。
  - 手机 3 条 `failed`（`learnharness.git`、`diagram-design`、微信 `XDF7S4hJPJMM3Y37wVtRig`）
    **是设计行为**：服务器端这 3 条 `word_count=0`，`save_pulled` 第 4 分支以 failed 入库、
    `retry_at = now`，交给手机本机退避解析（最多 3 次）。PC 本地对这 3 条已有真实正文，
    解析语义独立，无需处理。如想省事可在服务器 `/articles/{id}/refetch` 单独重推。
  - 「取消」按钮本轮未单独验证（v2 验收时验过）。

## 6. 剩余 TODO

1. 手机端 3 条 failed 会在本机退避解析中自动补齐（1min/5min，最多 3 次）；若终态仍失败，
   卡片上有「重试解析」。
2. 可选优化未做：删除加墓碑、进度上行、同步失败条目统计进报告、图片复制跨篇并发。
3. 旧待办：`d3l8hn` 桌面 release 签名、CI 回写 image tag、`p7v2ld`/`w8s5rt`/`h4m9zc` 人工验收。
4. 本轮 5 个 commit 在本地 main **未 push**；推送前确认远端无新提交。

## 7. 不要重踩

（继承 v2 交接 §12 全部条目，另有新增：）

- 不要在设备活跃时删服务器数据：无墓碑，持有方会推回（本轮清理顺序见 §4）。
- `pnpm tauri android build` 裸跑必失败，环境变量见 §5；**管道 tail 会吞 exit code**，
  判断成败必须 `set -o pipefail` 或直接看产物与日志。
- 判断脏数据是否清干净，用 URL 集合 diff，不要只看条数。
- vivo 手机 adb install 需要手机端确认弹窗，「User rejected permissions」不是命令失败。
