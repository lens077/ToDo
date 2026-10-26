# 一手调研笔记:monday.com 看板、「同步事实」竞品、cat 底座评估

> 调研日期:2026-04-30。本文由主 agent 亲自调研撰写,与 research/ 下其他分组调研笔记并列,供主报告引用。
> 说明:monday.com 实机看板需要登录,经确认改用公开资料(官方支持文档、官方公告、社区帖)分析。

## 1. monday.com 看板做了什么(公开资料)

### 1.1 数据模型:五层结构

据官方支持文档[《Understanding monday.com's structural hierarchy》](https://support.monday.com/hc/en-us/articles/7278527605906-Understanding-monday-com-s-structural-hierarchy)与[《The basics of a board》](https://support.monday.com/hc/en-us/articles/115005317249-The-basics-of-a-board):

```
Workspace → Folder(可嵌套) → Board(看板)/Dashboard/Workdoc
Board 内部:Group(彩色分组) → Item(行) → Subitem(子项)
横向:Column(列)定义每个 Item 的字段,官方提供数十种列类型
```

要点:
- Board 是「表格优先」的:主视图是 Main Table,Kanban 只是附加视图之一。
- Column 即 schema:Status、People、Date、Timeline、Files、Connect Boards(跨板关联)等,用户按需拼装。
- 同一 Board 可挂多个 View(Table/Kanban/Gantt/Calendar/Chart/Form 等),视图可锁定、置顶、隐藏主表、公开分享、导出、定时导出 PDF(见[《The board views》](https://support.monday.com/hc/en-us/articles/360001267945-The-board-views))。

### 1.2 Kanban 视图机制

据官方[《The Kanban View》](https://support.monday.com/hc/en-us/articles/360000661379-The-Kanban-View):
- 卡片列**只能由 Status 列的标签生成**(截至文档更新时);可选 Cover 列(图片封面)、卡片上直接展示的字段、按 Group/Status/Connect 列「Divide by」分泳道。
- 支持:卡片内联编辑、拖拽换列换序、子项展示、每列 WIP 上限(task limit)、导航概览小部件(battery 形式显示各列分布)。
- 对本项目的启示:monday 的 Kanban 是「结构化数据库上的一层投影」,列映射 = 单一 Status 字段。TODO.md 看板同样应把「列」映射为任务的单一状态字段,其余信息(优先级/标签/截止)作为卡片字段展示,不参与列布局。

### 1.3 AI 能力与计费

- 2025-02 官方公告[《monday.com Announces AI Vision》](https://ir.monday.com/news-and-events/news-releases/news-details/2025/monday.com-Announces-AI-Vision-to-Empower-Businesses-to-Scale/default.aspx):三支柱 = AI Blocks(Categorize/Extract 等模块化 AI 动作)+ Product Power-ups(嵌入各产品)+ Digital Workforce(数字员工 agent,首个为 monday Expert)。
- 计费:AI 按 credit 池计量。每计划每月含 500 免费 credits;AI Block 动作 8 credits/次;credit 桶从 2,500 到 250,000 分档扩容(见 [AI Feature Catalog](https://support.monday.com/hc/en-us/articles/24047211522194-AI-Feature-Catalog))。
- **官方已提供 monday MCP**:AI Feature Catalog 明确「monday MCP 集成不消耗 AI credits,走 API 调用并计入 API 限额」。
- Sidekick 可用自然语言生成视图/仪表盘(Text to board filters、Prompt to board 等不消耗 credits)。

### 1.4 性能口碑(反面教材)

- monday 官方曾专门开设[「speed issues」进展页](https://monday.com/cs/speed-issues-updates)处理板卡顿投诉(历史事件,最终以「Improved Board Performance」功能收尾)。
- 2025-04 官方社区帖[《Board feel so heavy, feel lagging and sometimes freeze》](https://community.monday.com/ask-the-com/post/board-feel-so-heavy-feel-lagging-and-sometimes-freeze-n7vxNJyDp039amV):过滤后不足 10 条 item 的视图,浏览器标签页内存占用约 1.5–1.6 GB。
- 结论:重 Web 平台在大板/复杂列场景的性能是长期痛点,「快」本身就是可营销的差异点(Linear 即以此立身)。

## 2. 「同步事实」按钮:已有先例与空白

用户核心差异化设想:一键让 AI agent 扫描代码库现状,对齐 todo 与代码现实。调研发现该问题已被命名、已有零散实现,但**无产品化 GUI 一键方案**:

| 项目 | 形态 | 机制 | 状态 |
|---|---|---|---|
| [TDReminder 论文](https://zpgao.github.io/papers/Just-In-Time_TODO-Missed_Commits_Detection.pdf) | 学术 | 检测「本该加 TODO 却没加」的提交(TODO-missed commits),CodeBERT 编码 diff+commit message 二分类 | 论文 |
| [since](https://github.com/LNSHRIVAS/since) | MCP 工具 | 指纹化 agent 读过的文件,后续每次工具调用报告哪些文件已变化(「stale world model」问题) | 可用 |
| [KEEL](https://github.com/vzwjustin/KEEL) | CLI+companion | 实时反漂移:记录 agent 声明的计划,2 秒轮询仓库,检测 scope/plan/goal 漂移;`keel done` 门禁「现实与意图一致才算完成」 | early alpha |
| [dev-agent-backlog reconcile 设计](https://github.com/farra/dev-agent-backlog/blob/819e662d/docs/design/013-reconciliation-commands.org) | org-mode 生态设计文档 | `/reconcile-backlog`:交叉取证 git log、CHANGELOG、源码,把任务分类为 confirmed-todo / likely-done / unclear;自动修清晰项、交互确认模糊项;**状态只升不降**(TODO→DONE 需证据,DONE→TODO 永不自动) | 设计文档 |
| [harness-sync 技能](https://www.claudepluginhub.com/skills/maxwell2732-claudecode-research-harness-workflow/harness-sync) | Claude Code 技能 | Plans.md 对账:commit 历史 vs 任务标记(cc:TODO/cc:WIP/cc:done),提出更新建议表,人工确认后写回 | 可用 |
| AI stale bots(google/adk-java 样例、dosu better-stale-bot) | GitHub bot | LLM 判断 issue 是否真的失活/已解决,再打标签或关闭 | 可用 |

可直接借鉴的设计共识:
1. **证据驱动**:git log、CHANGELOG、代码检索三路取证,而不是让模型凭空判断。
2. **分级处置**:自动修「证据确凿」项;「疑似完成」项列表交人确认;绝不自动把完成改回未完成。
3. **产出对账报告**:变更建议以 diff/表格呈现,人一键接受。
4. 空白点:以上全是 CLI/技能/设计稿,没有一个做成「看板上的一个按钮 + 可视化对账流」。

## 3. cat 项目底座评估(可复用资产)

cat 是同作者的 Tauri 2 桌宠 + 本地事件网关项目（原调研在本机路径下进行，路径已脱敏）,其架构与本 todo 工具高度同构,可复用:

| cat 资产 | 内容 | 对 todo 工具的复用价值 |
|---|---|---|
| 技术栈 | Tauri 2 + Vue 3 + TS + Vite + UnoCSS + Pinia;Rust 侧 axum(127.0.0.1:9527)+ tokio + rusqlite + reqwest + tokio-cron-scheduler | 整套壳与网关模式直接套用;「网关在 Rust 侧常驻,WebView 休眠不影响服务」正是 todo 工具需要的 |
| HTTP API | 已实现 `GET/POST /todos`、`PATCH/DELETE /todos/{id}`,以及带 Bearer 认证的远端面 `/api/todos` | curl CRUD 需求已有原型;数据模型 `Todo = {id,title,due_at,done,notified_at,created_at,completed_at}` 可扩展 |
| SQLite | rusqlite 单文件,`todos` 表已建 | 作为 md 的缓存/索引层的起点 |
| ntfy | `deploy/ntfy` 自托管配置;主题设计 `cat-evt-<rand32>` 下行 + `cat-cmd-<rand32>` 回程;一次性 HMAC token 防伪造动作 | 通知与「手机上点按钮回传」链路完整可搬 |
| agent 桥 | `bridges/claude-hook`(写 ~/.claude/settings.json 的 hooks 安装器)、`bridges/codex-notify`(~/.codex/config.toml notify) | Claude Code / Codex 事件接入模式现成;todo 工具的 MCP/skills 可与该 hook 体系并存 |
| 已有能力 | cat 会在会话正常结束时,从正文提取未完成的 Markdown 清单项(`- [ ]`)写入 TODO,并去重 | 「从 agent 输出捕获待办」已有实现雏形(protocol.md 第 531 行) |
| 多端 | apps/desktop + apps/pwa + apps/mobile(Tauri Android)+ apps/site | 桌面/Web/移动三端复用同一 Vue 前端的路径已验证,Android APK 已产出 |

注意差异:cat 的 todo 存 SQLite 为唯一事实源;新工具要求「TODO.md 为事实源、SQLite 只做索引」,同步方向相反,不能照抄存储层。

## 4. 个性化/游戏化市场信号(简查)

- [Forest 到 2025 年用户超 6000 万](https://yukaichou.com/lifestyle-gamification/best-gamified-productivity-apps/);Habitica 长期是游戏化习惯管理标杆(免费 + 订阅)。
- 桌宠陪伴 + 番茄钟类产品在 Reddit/Instagram 有自传播案例(如 [r/SideProject 的宠物番茄钟](https://www.reddit.com/r/SideProject/comments/1uhk3z9/i_made_a_free_cozy_pomodoro_app_where_you_focus/)、桌面像素猫 reel)。
- 启示:作者已有 cat 桌宠,todo 工具与桌宠联动(宠物状态反映任务/专注状态)是低成本高辨识度的个性化路线。
