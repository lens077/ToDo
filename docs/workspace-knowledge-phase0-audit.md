# Workspace / Project / Knowledge — Phase 0 仓库审计

日期：2026-10-31
依据：`docs/workspace-project-knowledge-spec.html`（v1.0，2026-10-31）
范围：只读审计，未修改业务代码。按规范 §16，Phase 0 完成后停止，等待用户批准。

## 1. 当前架构图

```text
Rust workspace
├── crates/todo-core        解析/投影/阅读/contract 库
│   ├── project.rs          ProjectBackend：单 root 的目录发现 + snapshot
│   ├── projection.rs       tasks SQLite 投影（SCHEMA_HASH 重建式更新）
│   ├── reading.rs          阅读库 ~/.todo/reading/reading.sqlite
│   ├── scan.rs             全仓库 Markdown checkbox/TODO 注释扫描
│   └── contract.rs         .virgo/project.yaml contract 校验
├── crates/todo-cli         CLI（todo serve --root <path>，单 root）
└── apps/desktop
    ├── src-tauri           Tauri host：DesktopOps 单例 + notify watcher
    └── src                 React 前端：useDesktop 单 snapshot model，无路由库
```

- 技术栈：Rust + Tauri 2 + React + rusqlite（WAL）。Markdown 唯一事实源，SQLite 派生。
- 门禁：`make check`（verify.sh：fmt/clippy/test/build/Playwright E2E）；`make check-quick`（pre-push）。

## 2. 当前数据模型图

| 存储 | 位置 | 机制 |
|---|---|---|
| 任务事实源 | 各项目 TODO.md + `<!--t:id-->` token | GFM checkbox（docs/format-spec-v1.md） |
| 任务投影 | `.todo/`（项目目录内）SQLite | SCHEMA_HASH 变更即重建，无迁移文件 |
| 阅读库 | `~/.todo/reading/reading.sqlite` + images/ | 内联 `CREATE TABLE IF NOT EXISTS`，无迁移框架 |
| Capture | `~/.todo/inbox.md` | Markdown 文本 |
| AI 会话 feed | `~/.todo/ai/sessions.json` | JSON |
| 应用偏好 | `~/.todo/desktop.json`（`last_project: Option<PathBuf>`） | JSON |
| 前端偏好 | localStorage（按 projectPath 分 key） | — |

## 3. 功能 → 新模型映射表

| 蓝图对象 | 当前状态 | 映射与差距 |
|---|---|---|
| Workspace | 无 | 全新：`workspace.rs` + `~/.todo/workspace.sqlite`（仿 ReadingStore 模式） |
| Project（稳定身份） | 身份 = canonicalized 目录绝对路径（`project.rs:147-185`），无持久 UUID | ProjectBackend 为单 root 结构（`project.rs:100-106`）；多 project 可用多实例并排，注册表需新建 |
| Project Profile | 无档案；有 `.virgo/project.yaml`（仅 contract 用）与 CONTEXT.md/DECISIONS.md 手工惯例 | Profile 需新建；lifecycle/milestone/blockers 为新字段 |
| Environment | 无 | 全新（只读扫描可仿 scan.rs 的 WalkBuilder 模式） |
| Knowledge 文档索引 | 无索引；有 `discover()`（`project.rs:449-559`，找 todo 源文件，256 sources/10000 entries/16MiB 预算） | 索引器可仿 discover + scan.rs 风格；增量扫描需新写（watch.rs 事件 → hash 对账） |
| Markdown 阅读器 | Rust 端 pulldown-cmark + ammonia 消毒（`reading.rs`），前端 `ReaderView.tsx:218` innerHTML 注入 | **直接复用**该链路做 Knowledge 阅读器；前端 inlineMarkdown 只支持行内语法 |
| Session | 无机制；仅 CONTEXT.md/AGENTS.md 手工惯例 + AI 会话 feed | 全新 |
| Task | todo.md checkbox，投影完备，contract 完成校验 | 保留为事实源；project_id 归属由"项目目录"天然满足，无需改格式 |
| Reading Context | articles/annotations 表 + X/微信/网页解析完备 | 上一轮已审计（docs/reading-context-audit.md），即蓝图 Phase 6 |
| Overview/Dashboard | 无跨项目聚合 | 新视图，复用 quadrantGroups 聚合多 snapshot |

## 4. 可复用模块清单

1. **ProjectBackend 多实例**：结构上"每 root 一个实例"，多 project 并排不需改 discover/写入逻辑（`project.rs`）。锁与 history 在各项目 `.todo/` 内，天然隔离。
2. **notify watcher**：`notify = "8.2.0"` + `TargetFileWatcher`（`src-tauri/src/watch.rs:1-72`，递归+debounce），多项目 watch 直接多实例化。
3. **Markdown→HTML 链路**：pulldown-cmark + ammonia + 图片本地化 + asset 协议 scope（reading.rs / ReaderView.tsx），Knowledge 阅读器现成。
4. **扫描基建**：`ignore::WalkBuilder`（discover、scan.rs 均在用）。
5. **ReadingStore 模式**：独立目录 + 独立 sqlite + `CREATE TABLE IF NOT EXISTS`（`reading.rs:97-119`）——workspace.sqlite 照此建。
6. **Reading 全套解析**（X/微信/网页）与上一轮 Reading Context 审计结论。
7. **前端模式**：MainView 字面量联合 + Sidebar navItems + App.tsx 渲染分支；localStorage 已按 projectPath 分 key。
8. **mock 传输层**（`lib/desktop.ts` `?mock=`）支撑 E2E。

## 5. 迁移风险与回滚方案

| # | 风险 | 缓解 |
|---|---|---|
| R1 | **单例全局状态渗透（最大）**：DesktopOps/ProjectBackend/watcher/preferences 全链路假设单 project；`~/.todo/inbox.md`、AI store、Capture triage 是全局单例，triage 写入目标 project 的选择逻辑是复杂度集中点 | Phase 1 先做 backend 池 + per-project 命令参数；triage 保持全局 inbox，归属选择延后 |
| R2 | **事实来源三处分叉**：workspace.sqlite 与 desktop.json `last_project`、前端 localStorage 各存一份 | workspace.sqlite 为唯一注册表；desktop.json 迁移字段保留兼容读；localStorage 只存 UI 偏好 |
| R3 | **无迁移框架**：第三个 SQLite + "CREATE TABLE IF NOT EXISTS" 模式无版本管理，出错静默降级 | workspace.sqlite 引入 `schema_version` 表 + 轻量迁移例程（PRAGMA 探测 + ALTER），失败显式报错 |
| R4 | **扫描预算放大**：16MiB/10000 entries 单项目预算 ×N 项目；notify 递归 watch 无 node_modules 过滤，可能事件风暴 | watch 加忽略规则；文档索引 hash 对账增量；Overview 聚合用懒加载 |
| R5 | **路径身份无 UUID**：Project ID 稳定性依赖注册表落库 + 重关联流程 | Phase 1 即登记 UUID ↔ canonical path，缺失路径显示 Missing |
| R6 | **mock 路径**：浏览器构建无 Tauri，多 project 模型必须同步进 mock | 每个 Phase 更新 `lib/desktop.ts` mock |

回滚：workspace.sqlite 独立文件，删除即回滚；不触碰现有 TODO.md、reading.sqlite、inbox.md；旧"选择目录"入口保留为"添加 Project"。

## 6. 按真实代码修订的分阶段实施计划

（对蓝图 §14 的修订，Gate 不变）

- **Phase 0**：本审计。✅
- **Phase 1 — Workspace & Project Registry**：新建 `crates/todo-core/src/workspace.rs`（workspace/projects/成员关系 + schema_version 迁移例程）；src-tauri backend 池化（HashMap<path, ProjectBackend>，命令带 project_path）；preferences 迁移；前端 Sidebar 加 Workspaces 视图。旧目录入口迁移为默认 Workspace + 默认 Project。
- **Phase 2 — Project Profile & Environment**：profile 表 + 只读扫描（Declared/Observed 分离，秘密不落库）。
- **Phase 3 — Knowledge 文档索引与阅读器**：`knowledge.rs` 索引（logical_type 建议 + 用户确认）+ 复用 pulldown/ammonia 链路阅读器 + watch 增量扫描。
- **Phase 4 — Session**：session 存储（第一版存项目 `.virgo/sessions/` Markdown，符合"Markdown 事实源"）+ begin/finish/resume CLI。
- **Phase 5 — Agent Contract/Skill/CLI/Validator**：与现有 contract.rs、AGENTS.md、make 门禁融合，不另起炉灶。
- **Phase 6 — Reading Context 融合**：即已审计的 Reading Context 方案，此处归位。
- **Phase 7 — Overview 汇总面板**：跨项目聚合视图（懒加载 snapshot）。

每阶段跑 `make check-quick`，Phase 完成后跑 `make check`，证据记入 `docs/verification-log.md`。

## 7. 结论 → 证据对照表

| 结论 | 证据 |
|---|---|
| 目录选择 = Tauri dialog | `apps/desktop/src/lib/desktop.ts:4`、`hooks/useDesktop.ts:392-407` |
| last_project 存 desktop.json | `src-tauri/src/preferences.rs:16-27`、`lib.rs:96-113` |
| 单 backend 单 root | `project.rs:100-106`、`lib.rs:78-89` |
| serve 从不切 root | `serve.rs:1-2, 39, 117-135`、`todo-cli/src/main.rs:51-52` |
| watcher 可多实例 | `src-tauri/src/watch.rs:1-72`、`lib.rs:476-514` |
| Markdown 消毒链路现成 | `reading.rs` sanitize_html、`ReaderView.tsx:218` |
| 无迁移框架、Store 模式 | `reading.rs:97-153`、`projection.rs:8-21` |
| 无 Session 机制 | `~/.todo/` 实测仅 inbox/ai/reading/desktop.json |
| discover 扫描预算 | `project.rs:17-21, 449-559` |
| 前端无路由库，MainView 联合类型 | `App.tsx:49, 306-374`、`Sidebar.tsx:5, 27` |

## 8. 验证记录

- 审计只读：`git status` 无业务改动（仅新增两份 docs 文档）。
- `make check-quick` 绿（38s，上一轮记录；本轮无代码变更未重跑）。
- ~~`todo contract check` 子命令不存在~~；更正（2026-11-03）：该命令已存在，`cargo run --quiet --locked -p todo-cli -- contract check --root .` 实测输出 `contract ok: ./.virgo/project.yaml (guided)`、退出码 0。

## 9. 待确认决策（进入 Phase 1 前）

1. Project 稳定 ID 的生成方式：注册表内 UUID（建议）vs 项目内 `.virgo/identity` 文件。
2. 旧"选择目录"入口保留为"添加 Project"，是否同意。
3. Session 存储位置：项目内 `.virgo/sessions/*.md`（建议）vs 全局 `~/.todo/sessions/`。
4. Phase 1 是否同时迁移 desktop.json `last_project`，还是先双读兼容。

~~**Phase 0 完毕。按规范 §16 停止，等待批准后进入 Phase 1。**~~

> 更正（2026-11-06）：批准已给出，Phase 1–7 已全部实施并通过测试，逐阶段交付证据见 `docs/verification-log.md`（2026-10-31 起的 Phase 1–7 各节）。本文上文 §3–§6 记录的是 Phase 0 时点的判断，不反映现状：当时判为"无"的 Workspace 注册表、Profile/Environment、Knowledge 索引、Session、Overview 均已按本文件的映射建议落地（`crates/todo-core/src/{workspace,profile,knowledge,session,overview}.rs`）。Phase 1–7 成果尚未提交 Git 的问题与处理见 `docs/audits/2026-11-03-audit.md` §1.2 与 M0 收口计划。
