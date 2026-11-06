# Reading Context 代码审计（Phase 0）

日期：2026-10-31
范围：只读审计，未修改业务代码。目标：为 Reading Context（收藏原因 / 阅读状态 / 项目任务关联 / 个人笔记 / 搜索）确定可复用组件、数据模型与兼容性风险。

## 1. 当前架构

- 产品定位：本地任务对账器。Markdown（各仓库 `TODO.md`、`~/.todo/inbox.md`）是唯一事实源，SQLite 是可重建投影（`crates/todo-core/src/projection.rs:8-19`，`SCHEMA_HASH = "todo-core-schema-v2-4b8d19e7"`）。
- 技术栈：Rust workspace（`todo-core` 解析/投影/阅读库，`todo-cli` CLI）+ Tauri 2 桌面端（React 前端 `apps/desktop/src`，host 直接调 todo-core）+ 落地站 `web/`。
- 数据库：rusqlite + WAL。**没有迁移框架**：阅读库用内联 `CREATE TABLE IF NOT EXISTS`（`crates/todo-core/src/reading.rs:121-153`）；tasks 投影用 SCHEMA_HASH 重建式更新。
- 门禁：`make check` = `scripts/verify.sh`（fmt/clippy/test/build/Playwright E2E）；`make check-quick` = `scripts/verify-quick.sh`（pre-push）。任务完成受 contract 检查约束（`require_task_reference_for_completion: true`）。
- `.virgo/project.yaml` 映射：tasks→TODO.md、progress→CONTEXT.md、architecture→docs/desktop-mvp-spec.md、decisions→DECISIONS.md。

## 2. 现有阅读功能（可复用主体）

存储：`~/.todo/reading/reading.sqlite`，图片按文章落盘 `~/.todo/reading/images/<id>/`。

### 2.1 数据层（`crates/todo-core/src/reading.rs`）

- 表：`articles(id, url UNIQUE, title, site_name, excerpt, byline, cover_file, content_html, markdown, word_count, reading_minutes, added_at, last_opened_at, progress_percent, progress_anchor, read_at)`；`annotations(id, article_id FK CASCADE, quote, start, end, note, created_at, updated_at)`。
- API：`add`(160)、`add_browser_html`(173)、`list`(224)、`get`(238)、`touch`(284)、`set_progress`(298)、`set_read`(334)、`remove`(356)、`upsert_annotation`(367)、`delete_annotation`(421)。
- 单测内嵌于源文件（1680 行起）。

### 2.2 解析能力（全部可复用，Reading Context 不动它们）

- X：`x_status_id`(583)、`fetch_x_tweet`(610)（fxtwitter → syndication 兜底）、X Articles 渲染 `fx_article_html`(835)、quoted tweets blockquote。
- 微信：`is_wechat_error_shell`(1303)、`wechat_content_fallback`(1308)、`data-src` 懒加载补丁、`mmbiz.qpic.cn` 图片本地化。
- 普通网页：`fetch_page`(542)、`extract`(1212, readabilityrs)、`sanitize_html`(1365, ammonia)。

### 2.3 Tauri 层与前端

- 命令：`apps/desktop/src-tauri/src/reading.rs`（`reading_list/add/get/progress/set_read/remove/annotation_upsert/annotation_delete/import_browser`），注册于 `src-tauri/src/lib.rs:671-679`；asset scope 限 `$HOME/.todo/reading/**`。
- 前端：`components/ReadingView.tsx`（列表+筛选 chips）、`components/ReaderView.tsx`（正文+圈选标注，`@recogito/text-annotator`）、`hooks/useReading.ts`（`useReading`:36）、`lib/reading.ts`（类型与 `readingState()` 派生：unread/reading/read，单一真相）。视图切换 `App.tsx:306-310`。
- 设计文档：`docs/adr/0003-reading-library.md`、`docs/design/reading/`。

## 3. Project / Task 实体

- Task：`crates/todo-core/src/model.rs:140`（`TaskNote`:236）。事实源是 TODO.md checkbox 行 + `<!--t:id-->` token；SQLite 投影 `tasks(id, body, completed, column_id, parent_id, due, tags, priority, …)`。
- Project：`crates/todo-core/src/project.rs`（`ProjectTask`:51、`ProjectSnapshot`:82、`ProjectBackend`:101）。
- 前端 CRUD：`apps/desktop/src/lib/desktop.ts` invoke 封装、`TaskCard.tsx` 等。

## 4. Markdown 与搜索现状

- 无富文本 Markdown 编辑器。最近的是 `TaskBodyEditor.tsx`（单行 textarea）+ `lib/inlineMarkdown.tsx` 行内渲染。ReaderView 的 `markdown` 字段由后端生成，正文目前展示 `content_html`。
- 无全文搜索：只有 `lib/reading.ts` 的 `filterArticles` 状态筛选与前端 chips；SQLite 无 FTS 表。

## 5. 测试结构

- Rust：`cargo test --workspace --locked`；单测内嵌 + `crates/todo-core/tests/`、`crates/todo-cli/tests/`。
- 前端：Playwright E2E（`apps/desktop/e2e/*.spec.ts`，含 `reading.spec.ts`）；浏览器构建走内存 mock（`lib/desktop.ts` `?mock=`）。
- 近期阅读提交（49dede5、3fe9d89、2d0379e 等）集中在上述 6 个文件 + `style.css`。

## 6. 可复用组件清单

1. `ReadingStore` 及内联建表模式——新表/新列在此扩展。
2. `articles.progress_percent` / `read_at` + `readingState()` 派生 = 阅读状态雏形。
3. `annotations` 表（含 `note` 字段）+ ReaderView 圈选 = 个人笔记基础。
4. 任务/项目 invoke 封装与 TaskCard（关联 UI）。
5. `inlineMarkdown` 渲染、`useReading` hook、批量导入面板（进度/取消交互模式）。
6. mock 传输层（E2E 必须继续可用）。

## 7. 建议的数据模型变更

- `articles` 加列：`saved_reason`（收藏原因）、`focus_notes`（关注重点）、`revisit_type`/`revisit_at`（重读安排，第一阶段只存储）。
- 新表：`reading_project_relations(reading_id, project_ref, created_at)`、`reading_task_relations(reading_id, task_ref, created_at)`、`reading_contexts`（或直接加列，取决于是否与 Source 严格分离——倾向加列以降低复杂度，见风险 2）。
- 个人笔记：复用/扩展 `annotations`，或加 `article_notes`（多段自由笔记）。
- URL 规范化 + 重复检测：`url UNIQUE` 已存在，补规范化函数与重复提示。
- 搜索：第一版用 `title/site_name/excerpt/saved_reason/note` LIKE；不部署外部搜索引擎。

## 8. 兼容性风险与决策建议

1. **无迁移框架（最高风险）**：改 `articles` 表必须兼容已有 `~/.todo/reading/reading.sqlite`。需要 `PRAGMA table_info` 探测 + `ALTER TABLE ADD COLUMN` 式的轻量迁移例程（或引 schema 版本号），不能只改 `CREATE TABLE`。
2. **ADR-0003 边界**：阅读库是"个人级、不写入任何项目文件"。任务/项目关联只在 reading 侧存引用（如 project 路径/task token），不回写 TODO.md，否则侵入 format-spec-v1。任务删除后按规范保留文章并清理失效关联。
3. **状态语义冲突**：现有 `readingState()` 由 `progress_percent`/`read_at` 派生且 sticky（100% 即已读）。新增 INBOX/UNREAD/READING/READ/ARCHIVED 状态机必须与该派生逻辑协调，避免两套真相——建议 `reading.ts` 仍为单一派生点，显式状态只作为用户覆盖。
4. **mock 路径**：`todo serve`/浏览器构建无阅读库，新字段必须同步进 mock，否则 E2E 全挂。
5. **contract 门禁**：新代码需过 clippy/test；不加 AI、不重建侧边栏。

## 9. 推荐实施方案

- Phase 1（数据模型）：reading.rs 内加轻量迁移例程 + 新列/新表 + URL 规范化 + 重复检测 + 数据层单测。
- Phase 2（收藏交互）：快速备注抽屉（收藏原因/项目/任务/关注点/重读安排，全可选），复用批量导入的进度交互模式。
- Phase 3（详情）：ReaderView 上方或侧边加上下文区（上下文编辑 + Markdown 笔记 + 状态/收藏操作），保持 asset/进度逻辑不变。
- Phase 4（项目关联）：项目详情加 Reading References 面板；关联只存 reading 侧。
- Phase 5（搜索导出）：LIKE 搜索 + Markdown 单篇导出/项目索引（带预览确认，不覆盖用户文档）。
- Phase 6（重新激活）：日期触发在打开应用时检查；任务/项目完成触发在状态变更时检查；无后台定时任务。

## 10. 本阶段验证记录

- 审计为只读：未改任何业务代码；`git status` 无新增业务改动。
- 未运行测试（无代码变更）；后续 Phase 1 起每阶段跑 `make check-quick` 并记录到 `docs/verification-log.md`。

## 11. 未决问题

1. 阅读上下文字段存 `articles` 加列 vs 独立 `reading_contexts` 表（规范倾向分离，实施上加列更简单；建议 Phase 1 定夺）。
2. 个人笔记复用 `annotations` 还是新表。
3. 项目关联引用格式：project 路径（跟随 .virgo/project.yaml 的 project id？）待与 project.rs 的标识方式对齐。
