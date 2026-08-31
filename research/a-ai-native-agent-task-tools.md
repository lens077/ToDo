# AI 原生 / 面向 AI coding agent 的任务管理工具:类目调研

> 调研日期:**2026-08-30**。所有 star 数、下载量、活跃度均为该日通过 GitHub API / npm registry 查询所得,引用时请连同日期一起使用。
> 一手资料优先:官方 README、仓库源码、docs、release notes、CHANGELOG、官方博客;社区评价取自 Hacker News 原始评论树。
> 无法核实之处一律标注「未验证」。委托方产品在下文简称「本产品」。

---

## 0. 给赶时间的人:七条最重要的结论

1. **Backlog.md 与本产品构想重合度最高(高)**,但不是全等。它=「markdown 任务文件进 git + CLI + 终端/Web 看板 + MCP」,6,573★、月下载 5.5 万、每周发版。**它没有的:移动端、局域网访问、推送通知、中文、代码注释扫描导入、以及最关键的「同步事实」。**
2. **「同步事实」在整个类目里是真空。** 我逐个核查了 Backlog.md、Task Master、beads、Shrimp、ccpm 的命令面与源码,**没有任何一个工具提供「扫描代码库实际状态、反向校正任务」的能力**。Task Master 官方文档把这件事明确推给用户纪律(「Update as you go」)。这是本产品最硬的差异化。
3. **Vibe Kanban(27,953★)已于 2026-04-10 停止商业运营**,母公司 bloop 关停。它**已经上线了「个人免费 + 团队 $30/人/月 + 企业 SSO」的完整定价**,创始人原话仍是:「绝大多数是免费用户,我们找不到一个让自己兴奋的商业模式」。**这是本类目最重要的商业化警报——「加个团队版收 seat 费」这条路已被走过并失败。**
   - **更值得记取的是死法**:真正的社区决裂发生在**更早的 2026 年 2 月**——把看板搬上云并**强制登录**,摧毁了用户最珍视的「本地、离线、无需账号」(issue #2687,33👍)。关停公告在 HN 只有 4 分 0 评论,而首发时是 195 分 132 评论,社区早已散尽。**委托方的 HTTP API / 推送 / 商业化必须是可选增量,不能替换本地路径。**
4. **Steve Yegge 的 beads(26,718★)是对本产品核心假设的正面挑战**:他明确论证「markdown 计划对 agent 是只写内存」。但 beads 自身已从 jsonl+SQLite **迁移到 Dolt 数据库,且数据库被 gitignore**——它实际上已经放弃了「任务文件进 git」。而社区对它的反弹催生了 `beans`(909★)与 `ticket`(881★)两个「回归 markdown」项目。
5. **平台吞噬风险真实存在但已出现反转**:Anthropic 在 Claude Code v2.1.16(2026-01-22)推出受 beads 启发的 Tasks 系统;但在最新版本中,**Todo/Task 工具已在 Opus 4.8、Sonnet 5 等新模型上默认关闭**(官方 CHANGELOG)。平台自己在收缩这块能力。
6. **「把 TODO.md 包一层 MCP」没有护城河**(`todo-md-mcp` 6★、单日写完)。护城河只可能来自:同步事实、三端体验、以及中文与商业化组合。
7. **中文是真实空位**:Backlog.md 的 i18n(含 zh-CN/zh-TW)PR 被关闭未合并,至今英文 only。

---

## 1. Backlog.md —— 重合度最高的对手

**仓库**:[MrLesk/Backlog.md](https://github.com/MrLesk/Backlog.md) · 官网 [backlog.md](https://backlog.md) · npm `backlog.md`

### 1.1 基本数据(2026-08-30 查询)

| 指标 | 数值 |
| --- | --- |
| Star | **6,573** |
| Fork | 396 |
| License | **MIT**(纯 OSI 开源) |
| 主语言 | TypeScript(Bun 构建) |
| 创建 / 最后 push | 2025-06-04 / 2026-08-29 |
| 最新版本 | v1.50.1(2026-08-10) |
| npm 近一月下载 | **55,183** |
| 开放 issue | 30 |
| 近 8 周提交数 | 78, 42, 12, 41, 112, 30, 21, 34 |

**活跃度判断:非常健康。**每周有版本、提交稳定、issue 积压少(30 个),与 Task Master(212)和 beads(587)形成鲜明对比。

### 1.2 是什么 / 目标用户 / 核心工作流

自述为「Markdown-native Task Manager & Kanban visualizer for any Git repository」,标语是「AI agents write the code. You review the tasks: before, during, and after.」

它的产品论证很清晰(README):AI 一小时产出的代码超过人一天能仔细读完的量,**瓶颈不再是写代码而是人的注意力**。因此把 agent 工作组织成**三个评审检查点**:

1. **评审规格** —— agent 先把想法拆成带描述与验收标准的任务,代码尚未存在时就能纠偏;
2. **评审计划** —— agent 调研代码库并把实现计划写进任务,批准后才动手;
3. **评审代码** —— 一个任务 = 一个上下文窗口 = 一个 PR,diff 保持在人类可读的尺寸。

**自举**:Backlog.md 自身几乎全部代码由 AI agent 通过 Backlog.md 完成,任务账本就在仓库 [backlog/tasks](https://github.com/MrLesk/Backlog.md/tree/main/backlog/tasks) 里公开可查。这是很强的可信度信号。

### 1.3 任务数据存哪(实测)

存在项目内 `backlog/`(或 `.backlog/`、或 `backlog.config.yml` 指定的自定义目录),**每个任务一个 `.md` 文件**,文件名形如 `back-208 - Add-paste-as-markdown-support-in-Web-UI.md`。

实测一个真实任务文件的格式(YAML frontmatter + markdown 正文):

```markdown
---
id: BACK-208
title: Add paste-as-markdown support in Web UI
status: To Do
assignee: []
created_date: '2025-07-26'
labels:
  - web-ui
  - enhancement
dependencies: []
priority: medium
---

## Description
...

## Acceptance Criteria
- [ ] Rich text content pasted into task edit fields is automatically converted to markdown
- [ ] ...
```

目录结构:`backlog/` 下有 `tasks/`、`drafts/`、`completed/`、`archive/`、`docs/`、`decisions/`、`milestones/`、`config.yml`。

**进 git:是**,这是默认且核心的设计。`git` 可选(`backlog init --no-git` 支持纯文件系统模式)。配置项 `auto_commit` 默认 `false`——即**默认不自动提交**,由用户自己决定何时提交,这一点与本产品「随代码提交进仓库」的设想一致但更保守。

> 与本产品的关键差异:本产品是**单一 TODO.md** 作为事实源;Backlog.md 是**一任务一文件**。这是个重要的架构分歧,详见 §9.2。

### 1.4 与 AI agent 的集成

**两条路线,且官方明确偏好 CLI:**

- **CLI instructions(默认、推荐)**:`backlog init` 生成指令文件,让 agent 运行 `backlog instructions overview` 获取工作流。
- **MCP(可选)**:`backlog mcp start`,官方给出 Claude Code / Codex / Gemini CLI / Kiro / Cursor 的接入命令。

**MCP 工具全清单(源码 `src/mcp/tools/*/index.ts` 实测,共 20 个)**:

| 分类 | 工具 |
| --- | --- |
| 任务(7) | `task_create`、`task_list`、`task_search`、`task_edit`、`task_view`、`task_archive`、`task_complete` |
| 里程碑(5) | `milestone_list`、`milestone_add`、`milestone_rename`、`milestone_remove`、`milestone_archive` |
| 文档(5) | `document_list`、`document_view`、`document_create`、`document_update`、`document_search` |
| DoD(2) | `definition_of_done_defaults_get`、`definition_of_done_defaults_upsert` |
| 工作流(1) | `get_backlog_instructions` |

agent 还可读取资源 `backlog://workflow/overview`、`/task-creation`、`/task-execution`、`/task-finalization`。

> ⚠️ **对本产品最重要的一条情报**:项目的 [MANIFESTO.md](https://github.com/MrLesk/Backlog.md/blob/main/MANIFESTO.md) 明写「**MCP is a legacy, optional adapter.**」「**The CLI is canonical.**」并要求「features must not be designed MCP-first or exist only through MCP」。
>
> 也就是说,这个赛道跑得最快的玩家判断:**MCP 不是最优 agent 入口,CLI 才是。**理由不难推测——CLI 无需常驻进程、不吃工具描述的 token 预算、任何 agent 都能用。本产品「MCP server + skills」的技术路线应当认真对待这个反面判断,至少要同时提供一等公民的 CLI。

**hooks**:配置项 `onStatusChange` 可在任务状态变化时执行 shell 命令,官方示例甚至是「状态变为 In Progress 时自动拉起 `claude` 去实现这个任务」——一个很轻量但很强的自动化钩子,可供本产品借鉴。

### 1.5 是否解决「todo 与代码现实漂移」

**部分解决,但不是本产品设想的那种解决方式。**

- ✅ **它解决的是「跨分支的任务状态漂移」**。源码 [`src/core/cross-branch-tasks.ts`](https://github.com/MrLesk/Backlog.md/blob/main/src/core/cross-branch-tasks.ts) 实现了跨分支状态解析:遍历近期活跃分支(`activeBranchDays` 默认 30 天),用 `git` 直接读取各分支树中的任务文件,以**最近修改**的那份为准。作者在 HN 上亲自解释:「Git allows to fetch certain files from other branches including remote ones without checking out those branches. The state is always up to date no matter if you are running backlog.md from main branch or a feature branch.」([HN](https://news.ycombinator.com/item?id=44483530))
  这解决的是「任务在 feature 分支被标记完成,主分支看不到」的问题——**是本产品文档里没有提到、但真实存在的一类漂移**,值得纳入设计。
- ❌ **它不解决「代码改了但任务没更新」**。我检索了全部 CLI 命令与文档,**不存在任何扫描代码库、对齐任务与代码现实的功能**。

**代价**:跨分支检查在分支多的大仓库上有性能开销,官方文档明确提示可关掉 `checkActiveBranches` 换速度。

### 1.6 技术栈 / 性能 / GUI 形态

- **技术栈**:TypeScript + Bun,编译为各平台原生二进制(`backlog.md-darwin-arm64` 等 6 个 optionalDependencies),npm 主包零运行时依赖。
- **界面**:①终端 Kanban(`backlog board`);②Web UI(`backlog browser`,拖拽 + 表单编辑);③纯 CLI;④社区 VS Code 扩展。
- **⚠️ Web 服务绑定 `127.0.0.1` 硬编码**(源码 `src/server/index.ts`:`const BROWSER_HOST = "127.0.0.1"`),**明确不可从局域网/VPN 的其他设备访问**。
- **移动端**:无原生应用。MANIFESTO 定位为「desktop-first with best-effort mobile behavior」,并写明「Full desktop/mobile feature and visual parity is not a default commitment.」
- **HTTP API**:Web UI 背后**确实有一套 REST API**(实测路由约 28 条:`/api/tasks`、`/api/task/:id`、`/api/tasks/:id/complete`、`/api/search`、`/api/statistics` 等)。但它是 Web UI 的内部接口、绑定 localhost、**未作为公开集成契约对外文档化**。
- **国际化**:**无**。i18n 需求 issue [#386](https://github.com/MrLesk/Backlog.md/issues/386) 已关闭;一位中文贡献者提交的「Web UI i18n: en, ja, zh-CN, zh-TW」PR [#677](https://github.com/MrLesk/Backlog.md/pull/677) 状态为 **CLOSED 未合并**,源码中也无 locale 文件。**英文 only 是现状。**

### 1.7 商业化

**无。**纯 MIT,无付费版、无云服务、无 sponsor 墙。作者 Alex Gavrilescu 供职于 Funstage GmbH,这是个人开源项目。

### 1.8 社区评价(HN 254 分 / 62 评论,[原帖](https://news.ycombinator.com/item?id=44483530))

**正面**:普遍认可「markdown + AI agent」的方向;有用户表示自己独立收敛到几乎相同的方案(`tasks.md` + 依赖 + 状态)。第三方评测称它是「第一个把 AI coding agent 当作一等公民而非附属品的工具」。

**批评与暴露的真实痛点**(对本产品极有价值):

- **任务多了以后 agent 上下文会爆**。tptacek 直接发问:「你截图里任务这么多,agent 怎么在不炸掉上下文预算的情况下导航这些 markdown?」作者回答靠「任务切到一个 PR 的大小」,并承认**真正吃上下文的是 docs 和 decisions 文件**。
- **单个任务文件不能太大**,否则 agent 编辑困难。
- **命名困惑**:`Backlog.md` 是目录名不是文件名,作者不得不专门给 Codex 写指令,「它老想 `cat Backlog.md` 而不是 `cd`」。**对本产品的直接启示:`TODO.md` 这个名字反而没有这个歧义,是个小优势。**
- 作者本人建议:需要接 Jira 的场景去用 Task Master,「Backlog 更适合本来就不会上项目管理工具的小项目」——**这是它自己划定的能力边界**。

### 1.9 与本产品的重合度:**高**

| 维度 | Backlog.md | 本产品 | 判定 |
| --- | --- | --- | --- |
| markdown 任务进 git | ✅ 一任务一文件 | ✅ 单一 TODO.md | 同向,粒度不同 |
| 看板 UI 回写文件 | ✅ Web 拖拽 | ✅ | **重合** |
| MCP 接入 agent | ✅ 20 工具(但定位为 legacy) | ✅ MCP + skills | **重合** |
| CLI | ✅ 且为 canonical | 未强调 | **对方更强** |
| 桌面/移动/Web 三端 | ❌ 仅本地 Web + 终端 | ✅ Tauri 三端 | **本产品差异化** |
| 局域网/远程访问 | ❌ 硬编码 127.0.0.1 | ✅ HTTP API | **本产品差异化** |
| 同步事实 | ❌ | ✅ | **本产品差异化** |
| 代码注释 TODO 导入 | ❌ | ✅ | **本产品差异化** |
| 仓库导入 | ❌ | ✅ | **本产品差异化** |
| 推送通知(ntfy) | ❌ | ✅ | **本产品差异化** |
| 中文 | ❌ i18n PR 被关 | ✅ 默认中文 | **本产品差异化** |
| 商业化 | ❌ 无意图 | ✅ 要商业化 | 生态位不冲突 |
| 跨分支状态解析 | ✅ 已实现 | 未提及 | **对方更强,建议补** |

**结论**:方向高度雷同,但 Backlog.md 把自己限定在「单机 + 终端/本地 Web + 英文 + 不商业化」。本产品的三端、远程、中文、同步事实构成清晰的差异空间。**最需要警惕的不是功能差距,而是它每周发版的迭代速度和已建立的 6.5k★ 心智。**

---

## 2. Vibe Kanban —— 27,953★,但公司已关停

**仓库**:[BloopAI/vibe-kanban](https://github.com/BloopAI/vibe-kanban) · 官网 [vibekanban.com](https://www.vibekanban.com/)

### 2.1 头号情报:项目正在日落

README 顶部现有横幅:「**Vibe Kanban is sunsetting.**」

官方公告 [Goodbye bloop](https://www.vibekanban.com/blog/shutdown)(Louis Knight-Webb,2026-04-10)原文要点:

> 「Today we're shutting down bloop, the company behind Vibe Kanban. The Vibe Kanban project will live on, open source and community maintained.」
>
> 「**Thousands of software engineers use Vibe Kanban every day to ship more with coding agents, but the vast majority are free users and we couldn't find a business model that we could get excited about.**」

处置方式:提供数据导出;远程服务(云端 kanban issues、comments、projects、organisations)保留 30 天后下线,**产品回退为纯本地架构**;近 30 天发票全额退款、订阅终止;本地 workspace 继续可用。

**它当初的定价模型**([pricing 页](https://www.vibekanban.com/pricing),2026-08-30 仍可访问):

| 档位 | 价格 | 内容 |
| --- | --- | --- |
| FREE | $0 | 单人、核心功能、社区支持 |
| **PRO** | **$30 / 人 / 月** | 2–49 人、99.5% SLA、Discord 支持 |
| ENTERPRISE | 定制 | 50+ 人、SSO/SAML、99.9% SLA |

> **这是本次调研对委托方最有商业价值的一条发现。**一个拿了 YC 背景、28k★、每天数千工程师在用、功能领先(首个做多 agent、diff 评论、实时预览、远程访问)的产品,**采用了看起来最合理的「个人免费 + 团队按人月付费」模型,依然死于找不到商业模式**。
>
> 尤其值得注意:它把付费点放在**团队协作**上——而这恰恰是本报告 §9.4 会建议的方向之一。**这说明「团队付费」并非万灵药**,委托方需要比「加个团队版」更具体的变现假设。详见 §9.4。

### 2.1.1 真正的死亡时点是 2026 年 2 月,不是 4 月(关键教训)

这是本次调研最反直觉、也对委托方最有指导意义的一条:**社区决裂发生在关停前两个月,起因是产品方向而非资金。**

2026 年 2 月,团队把看板搬上云端并**强制登录**,触发 issue [#2687「Bring back old UI」](https://github.com/BloopAI/vibe-kanban/issues/2687)(实测 **33 个 👍、39 条评论**,现已关闭)。用户原话包括「classic rug pull」「You've ruined a very good project」;评论区可见的核心诉求非常朴素:

> 「the old ui is better for me too,and **it's support offline mode**」
> 「**Why can't users use kanban board offline (not logined)?** Just want to use it by my own, not working with team.」
> 「Time to look for new ones.」

CEO 当时的辩护是「Preparing for cloud ensures the maximum chance of survival」——两个月后公司关停。

**铁证**:关停公告在 HN 上只拿到 **4 分、0 条评论**;而 2025 年首发时是 **195 分、132 条评论**。社区在 2 月就已散尽。

> ⚠️ **对委托方最直接的教训**:这个项目最受欢迎的特性之一是**本地、离线、无需登录**;把它改成云端强制登录,直接摧毁了社区信任。**委托方规划的「HTTP API + 推送 + 商业化」如果以牺牲『本地优先、离线可用、无需账号』为代价,极可能重蹈覆辙。**正确做法是:云端能力必须是**可选的增量**,而不是对本地路径的替换。

### 2.1.2 「社区维护」是空头承诺(已核实)

官方称项目将「continue as open source and community maintained」,但实测:

- **`commits?since=2026-04-25` 返回 0 条**——关停后零提交。
- **2,998 个 fork 无一接盘**(唯一还在发版的是 `dexloom/vibe-kanban-indie`,63★)。
- 用户已迁移到新产品:Emdash(5,542★)、Nimbalyst(1,608★)、Agor(1,380★)、Chorus(1,151★)。
- ⚠️ 存在**自 2026-06 未修复的 OAuth 账号接管漏洞(#3429)**——**不建议在生产环境使用**。

> **另一条可引用的判断**:社区对其死因最精辟的总结是——「The company couldn't make money because **the free local product was too complete**.」这句话对任何「开源本地工具 + 想商业化」的团队都值得贴在墙上。

### 2.2 基本数据(2026-08-30 查询)

| 指标 | 数值 |
| --- | --- |
| Star | **27,953** |
| Fork | 2,998 |
| License | Apache-2.0 |
| 主语言 | Rust(3.7 MB)+ TypeScript(3.4 MB) |
| 最新 release | v0.1.44(2026-04-24) |
| 最后 push | 2026-04-24 |

### 2.3 是什么 / 核心工作流

定位:「Get 10X more out of Claude Code, Gemini CLI, Codex, Amp and other coding agents」。它**不是任务文件管理器,而是 coding agent 的编排器 + 评审台**。

README 描述的能力:
- 用 kanban issue 规划工作
- **在 workspace 中运行 agent**——每个 workspace 给 agent 一个分支、一个终端、一个 dev server
- 评审 diff 并留行内评论,反馈直接送回 agent
- 内置浏览器预览(带 devtools、inspect、设备模拟)
- 切换 **10+ agent**:Claude Code、Codex、Gemini CLI、GitHub Copilot、Amp、Cursor、OpenCode、Droid、CCR、Qwen Code
- 生成 PR(AI 写描述)并合并

启动方式:`npx vibe-kanban`。

### 2.4 任务数据存哪 —— 与本产品的根本分歧

**存在本地 SQLite 数据库,不在用户的 git 仓库里。**数据库文件名硬编码于源码 [`crates/db/src/lib.rs`](https://github.com/BloopAI/vibe-kanban/blob/main/crates/db/src/lib.rs):**`db.v2.sqlite`**(注意是 v2)。

```rust
let database_url = format!("sqlite://{}", asset_dir().join("db.v2.sqlite").to_string_lossy());
```

路径(经官方 [troubleshooting.mdx](https://github.com/BloopAI/vibe-kanban/blob/main/docs/troubleshooting.mdx) 确认):

| 平台 | 路径 |
| --- | --- |
| macOS | `~/Library/Application Support/ai.bloop.vibe-kanban/` |
| Linux | `~/.local/share/vibe-kanban/` |
| Windows | `%APPDATA%\bloop\vibe-kanban\` |

表结构(由 70+ 迁移文件还原):`projects → tasks → workspaces`(原 `task_attempts`)`→ sessions → execution_processes → coding_agent_turns`,另有 `merges`、`pull_requests`、`repos`、`workspace_repo`。`TaskStatus = Todo / InProgress / InReview / Done / Cancelled`。

**这是它与 Backlog.md / 本产品最本质的路线差异**:任务是「agent 运行记录」而非「仓库文档」。`git clone` 拿不到任何任务状态。

> ⚠️ **一条极有说服力的情报**:任务一度被搬到**云端 Postgres**(迁移史可见 `shared_tasks` 表 → ElectricSQL 同步 → 最后删表)。**结果是公司关停后,看板功能随云端一起废掉**。
>
> **这正是「markdown-in-repo」路线可以主打的结构性优势**:任务随仓库天然存活,不依赖任何厂商的服务器继续运行。建议委托方把这一点作为明确的产品叙事——**「你的任务不会因为我们倒闭而消失」**,这在 Vibe Kanban 之后是有真实说服力的卖点。

### 2.5 与 agent 的集成

- **作为编排器**:spawn 各家 agent 的本地 CLI 子进程,用 `--output-format=stream-json --input-format=stream-json` 双向通信,`--resume` 续跑;用 **git worktree** 给每个任务隔离工作区(`crates/worktree-manager`、`workspace-manager`)。分支命名 `vk/<id>-<task>`,支持 Direct merge 与 PR,支持 GitHub + Azure Repos。
- **agent 数量需校正**:官方文档列 10 个,但源码 `CodingAgent` 枚举实为 **9 个 executor**——**CCR 其实是 Claude Code executor 的一个开关,不是独立 executor**。
- **双向 MCP**:既自带 MCP server(`npx vibe-kanban --mcp`,工具含 `create_workspace`,可让 agent 自己开工作区),**同时也是 MCP client**。联合创始人 ggordonhall 在 HN 说明:加上 Vibe Kanban 的 MCP 后,可以起一个「planning」ticket 让 agent 反过来往看板里批量建卡([HN](https://news.ycombinator.com/item?id=44533004))。

### 2.6 是否解决漂移

**用「隔离 + 给每条任务绑定物证」从源头预防,而非事后对账。**具体机制值得本产品借鉴:

- 每任务独立 **worktree + 分支**;
- 记录 **before / after 的 HEAD commit sha**——即任务与「代码到底改了什么」有可验证的硬绑定;
- 关联 **PR 状态**;
- **agent 跑完强制进入 `InReview` 而不是 `Done`**——完成与否由人裁定。

> **这套「任务 ↔ 物证」的思路可以与 markdown 方案叠加**:本产品的「同步事实」若能在 TODO.md 中记录任务对应的 commit 范围/文件路径,对账就从「让 agent 重新理解代码」变成「比对已记录的物证与当前 HEAD」,**可信度会高一个量级**。这是本次调研中最具可操作性的技术借鉴。

**但它并未真正消除漂移**:任务与代码**不同源**(SQLite vs. 仓库)是更根本的风险,且有用户报告过最坏形态的 bug:「**It says it is merged, but changes are not committed**」。

本产品的定位不同:允许 agent 在任意地方改代码,事后再对账。二者是**预防 vs. 事后校正**的互补关系,不是替代关系。

### 2.7 三端与远程

值得本产品注意:仓库里有 **`crates/tauri-app`**(Tauri 桌面壳)和一整组 `relay-*` crates(隧道/远程访问),还有 [mobile-testing.md](https://github.com/BloopAI/vibe-kanban/blob/main/mobile-testing.md) 指导用 **Tailscale + Caddy** 从手机访问 Web 前端。

> 即:**技术栈选型(Rust + Tauri + SQLite + 远程访问)与本产品高度一致**。但它没有做原生移动端,手机访问靠 Tailscale 组网——本产品若真做出好用的移动端,是有真实空位的。

### 2.8 社区评价(HN 195 分 / 132 评论)

- **正面**:「用了一周,非常好——像第一次用 Cursor 那种生产力跃升」。有用户把它与 git worktree 组合作为并行 agent 的标准答案。
- **质疑(很尖锐)**:
  - deepdarkforest:并行 agent 实际会互相打架、改同一批文件、重写测试,「是混乱……对不重要的 demo 无所谓,但我很怀疑」。
  - codingdave:「这类产品建立在『AI 已经成熟到能完成所有任务』的假设上,但事实并非如此。」
  - swalsh:规模化后出现「复合式的假阳性」——模型加兜底逻辑让测试过了,实际是埋雷。
  - csomar:**「为什么需要 GitHub 授权?这要求对我的仓库有无限私有访问权限。这对我是明确的 No。」**——权限顾虑是真实的获客阻力,本产品若做仓库导入需正面处理。
  - **worktree 不是银弹**:「you're still sitting around waiting for Claude to merge conflicts」。
  - **评审瓶颈**(对本产品的问题定义很有价值):「AI writes 1 PR, we now review 11」——**并行 agent 把瓶颈从「写」搬到了「审」**,这正是 Backlog.md 三检查点与本产品「同步事实」共同想缓解的问题。
- Backlog.md 作者 mrlesk 也在该帖友善留言,两个项目互相知晓。

**两处需要公道记录的更正**(避免打稻草人):

1. **「token 烧钱」并非主要槽点**——Vibe Kanban 用的是用户自己的 agent 订阅,真实短板是成本**不可见**,而非昂贵。
2. **遥测默认开启曾引发众怒,但官方在 Show HN 当天数小时内即改为 opt-in**(PR #146)——属教科书式危机响应,应予肯定。

### 2.9 与本产品的重合度:**中**

技术栈(Rust/Tauri/SQLite)、看板形态、agent 集成高度重合;但**事实源哲学相反**(DB vs. 文件)、**工作流更重**(必须在其 workspace 内执行)。本产品可视其为「同栈不同路」的参照,并从其关停中吸取商业化教训。

---

## 3. claude-task-master / Task Master AI

**仓库**:[eyaltoledano/claude-task-master](https://github.com/eyaltoledano/claude-task-master) · 商业站 [tryhamster.com](https://tryhamster.com)

### 3.1 基本数据(2026-08-30 查询)

| 指标 | 数值 |
| --- | --- |
| Star | **28,037**(类目内最高) |
| Fork | 2,628 |
| License | **MIT + Commons Clause**(非 OSI 开源,禁止售卖与托管) |
| npm 近一月下载 | **80,001** |
| 最新正式版 | 0.43.1(**2026-03-31**,约 5 个月前) |
| 最后 commit | 2026-04-23(仅替换失效文档链接) |
| 开放 issue + PR | 212 |

> **活跃度警报**:main 分支约 4 个月无实质更新,`1.0.0-rc.1` 自 2026-02-18 未转正,而 issue 持续累积。合理推断团队重心已转向商业产品 Hamster(**未验证**)。

### 3.2 核心工作流

**PRD 驱动**:写 `.taskmaster/docs/prd.txt` → `parse-prd` 生成任务 → `analyze-complexity`(1–10 打分)→ `expand` 拆子任务 → `next`/`show` 取任务 → `set-status`/`update-subtask` 回写。

### 3.3 任务数据存哪

**`.taskmaster/tasks/tasks.json`**,单一大 JSON,**提交进 git**(官方仓库自己就提交了)。0.16.2 起改为 **Tagged Task Lists**:`{"master": {"tasks": [...]}, "feature-x": {...}}`,每个 tag 内 ID 独立编号。

另生成每任务 `task_<id>_<tag>.txt`(**是 `.txt` 不是 `.md`**),且是**派生产物,需手动 `generate` 重新生成**——本身就是第二份会过期的副本。

**实测官方自己提交的 `tasks.json`(硬证据)**:

| 指标 | 数值 |
| --- | --- |
| 文件大小 | **1,558,493 字节(约 1.49 MB)** |
| tag 数 / 任务数 / 子任务数 | 9 / 182 / 914 |
| 平均每任务 | 7,805 字节 |
| 单任务最大 | **127,870 字节**(含 45 个子任务) |
| 已完成仍留存 | 97 个 |

> **对本产品的直接教训**:单文件承载全部任务 + 提交 git = 每次状态变更重写整个文件 → diff 噪音大、极易冲突。**本产品的「单一 TODO.md」有完全相同的结构性风险**,详见 §9.2。

### 3.4 与 agent 的集成

**MCP 工具实测 44 个**(源码 `mcp-server/src/tools/tool-registry.js`;README 仍写 36 个,已过时)。分类:项目配置 4、PRD 与生成 5、范围调整 2、查询 4、增删改 9、移动与依赖 5、tag 管理 6、调研 1、Autopilot/TDD 8。

**支持分级加载**(`TASK_MASTER_TOOLS=all|standard|core`),因为**全开约吃 21,000 tokens** 的上下文,core 也要约 5,000。

Claude Code 侧提供官方 plugin:**46 个 `/tm:` slash commands + 3 个 subagent**(orchestrator/executor/checker)。**无官方 Skills**;hooks 是 Kiro 格式而非 Claude Code 格式。支持 15 个编辑器 profile。

### 3.5 是否解决漂移:**不解决**(逐条核实)

| 命令 | 源码描述 | 是否针对代码现实 |
| --- | --- | --- |
| `update` / `update_task` | 「based on new context or changes provided in **the prompt**」 | ❌ 依据 prompt 文字 |
| `update_subtask` | 追加带时间戳的信息 | ❌ 内容由调用方给出 |
| `analyze_project_complexity` | 对**任务文本**打分 | ❌ |
| `validate/fix_dependencies` | 校验任务图自洽 | ❌ |
| `sync-readme` | 任务 → README 导出 | ❌ 名字极易误解 |
| `research` | 可带 `filePaths`/`includeProjectTree`,结果可 `saveTo` 回任务 | ⚠️ 唯一能摄入代码,但需**显式指定路径**且是一次性 |

**不存在 `sync` 命令。**官方 `docs/task-structure.md` 把问题交还用户:「**Update as you go**: If your implementation diverges from the plan, use the update command...」

最接近的是 Kiro hook `tm-code-change-task-tracker`:保存源码文件时,给「当前 in-progress」的那一个任务追加笔记,**是否标记完成还要问用户**,且仅 Kiro 可用。

Autopilot(8 工具)是 TDD + git 分支/commit 绑定的状态机,属**预防**而非事后校正。

> 讽刺的是,它自己的 MCP 层就有 drift:issue [#1683](https://github.com/eyaltoledano/claude-task-master/issues/1683) 标题即「**set_task_status can drift from tasks.json**」,报告者结论是「This makes the MCP unsafe to trust without re-reading the on-disk tasks.json」。

### 3.6 技术栈 / GUI / 商业化

- TypeScript+JS monorepo(turbo/pnpm),Node ≥20,Vercel AI SDK v5 + fastmcp + zod。
- **GUI:VS Code Kanban 扩展**(publisher 即 `Hamster`)。无开源 Web/移动端。TUI 未实装(issue [#1725](https://github.com/eyaltoledano/claude-task-master/issues/1725) 崩溃)。
- **商业化:Hamster 是同团队的托管产品**。分界为「本地文件 = 免费开源;briefs / API storage / 团队同步 / 托管 AI = 付费」。CLI 的 `auth`/`login`/`briefs`/`export` 即云端入口。
- ⚠️ Hamster 产品页称「Taskmaster is MIT licensed」,但仓库 LICENSE 实为 **MIT + Commons Clause**(明确禁止 hosting 与售卖),**营销表述与实际许可不符**。

### 3.7 主要痛点

- **Token 成本高**:44 工具全开 21K tokens;有用户实测 Cursor rules 再吃约 27K([#1616](https://github.com/eyaltoledano/claude-task-master/issues/1616))。无内置花费上限。
- **数据完整性是最密集的批评**,且集中在 tag 机制 + 写路径:[#1708](https://github.com/eyaltoledano/claude-task-master/issues/1708) schema 腐化(`id` number→string、注入 `parentId:"undefined"`,**在官方自己的 tasks.json 中实测印证**);[#1636](https://github.com/eyaltoledano/claude-task-master/issues/1636) `fix_dependencies` 会抹掉除 master 外所有 tag;[#1637](https://github.com/eyaltoledano/claude-task-master/issues/1637)/[#1638](https://github.com/eyaltoledano/claude-task-master/issues/1638) 跨 tag 数据损坏。
- **隐私**:[#1681](https://github.com/eyaltoledano/claude-task-master/issues/1681) 默认 Sentry 遥测 `sendDefaultPii:true`,**记录完整 prompt 与响应**。
- **长期未修**:[#1601](https://github.com/eyaltoledano/claude-task-master/issues/1601) `parse-prd` 在 Codex CLI/OpenRouter 上完全不可用,1 月报告 4 月仍有用户追问(含中文用户「现在他修复了吗,codex完全无法使用」)。

### 3.8 与本产品的重合度:**中**

同为「任务文件进 git + MCP」,但它是 **PRD 驱动的生成器**,重心在「把需求拆成任务」;本产品重心在「看板 + 事实对齐」。它的 1.5 MB 单文件与 tag 写路径 bug 是本产品最该规避的具体教训。

---

## 4. beads —— 对本产品核心假设的正面挑战

**仓库**:[gastownhall/beads](https://github.com/gastownhall/beads)(原 `steveyegge/beads`,已迁移)· 文档 [beads.gascity.com](https://beads.gascity.com/)

### 4.1 基本数据(2026-08-30 查询)

| 指标 | 数值 |
| --- | --- |
| Star | **26,718** |
| Fork | 1,803 |
| License | MIT |
| 主语言 | Go(28.9 MB) |
| 创建 | 2025-10-12(不到一年) |
| 最新版本 / 最后 push | v1.2.2(2026-08-15) / 2026-08-30 |
| npm `@beads/bd` 近一月 | 22,861 |
| **开放 issue** | **587** |
| 近 8 周提交 | 201, 208, 144, 282, 252, 117, 58, 18 |

**极高强度开发**(周均百余次提交),但 **587 个未决 issue** 也说明质量债务同样在累积。

### 4.2 设计理念:为什么说 markdown todo 对 agent 不够

这是本产品**必须正面回应**的论证。Yegge 在 [Introducing Beads](https://steve-yegge.medium.com/introducing-beads-a-coding-agent-memory-system-637d7d92514a) 与 [The Beads Revolution](https://steve-yegge.medium.com/the-beads-revolution-how-i-built-the-todo-system-that-ai-agents-actually-want-to-use-228a5f9be2a9) 中的核心主张:

> 「**The core problem with markdown plans: They're write-only memory for agents.**」
>
> 「Markdown plans are text, not structured data, and need to be parsed and interpreted — this places a high cognitive load on the model, which steals GPU cycles from your actual problem.」
>
> 「They're **not queryable** ... **Agents rarely update the plans as they work, so the plans bit-rot very fast.**」
>
> 「With markdown, two agents working on the same project means **conflicting TODO lists and duplicated work**.」
>
> 「With markdown, I'd need to parse `git blame`, and even then I only see line-level changes, not semantic 『status changed from open to in_progress』.」

他自述曾累积 **605 个** 处于不同腐烂阶段的 markdown 计划文件。

**四条具体优势主张**:①依赖是一等公民,`bd ready --json` 直接给出可做的工作,无需解释文本;②执行中发现的工作用 `discovered-from` 链接,保留来源;③跨会话持久,不必重新粘贴;④多 agent 可查询同一逻辑库、看到 `in_progress` 认领状态。

> **对本产品的意义**:「TODO.md 会 bit-rot」正是委托方「同步事实」按钮想解决的问题——**Yegge 的诊断与委托方一致,只是开的药方不同**(他换存储,委托方加对账)。这实际上**强化**了本产品的问题定义,同时也提醒:如果不解决可查询性与依赖建模,单靠 markdown 会遇到他描述的天花板。

### 4.3 存储机制 —— 重要更正

**任务书中的「jsonl+sqlite」已过时。**当前(v1.x)beads **以 [Dolt](https://github.com/dolthub/dolt)(版本控制的 SQL 数据库)为存储后端**:

- 嵌入模式(默认):数据在 `.beads/embeddeddolt/`,单写者;
- Server 模式:外部 `dolt sql-server`,多写者。

**与 git 的关系(关键)**:

> 官方文档架构图明写:「Dolt DB (`.beads/embeddeddolt/` ... ; **gitignored**)」

跨机同步走 **Dolt remote**:`bd dolt push` / `bd dolt pull`,历史存放在 git remote 的 **`refs/dolt/data`**(与 `refs/heads/main` 分离)。`.beads/issues.jsonl` **仅是导出物**,官方明确:「It is **not the canonical cross-machine sync channel**」「not the source of truth or a backup」。

> **这是一个极有说服力的观察**:beads 一路从「jsonl 进 git」演化到「数据库 + 独立 ref」,**等于承认了『把结构化任务塞进 git 工作树』的路线在多 agent 高频写入下不成立**。本产品需要明确自己不会走到那个规模,或者提前设计好冲突策略(§9.2)。

### 4.4 与 agent 的集成

`bd init` 自动写 `AGENTS.md`;`bd setup <target>` 支持 codex / claude / factory / cursor / mux 等(装 skill、hooks、指令)。核心命令:`bd ready`、`bd create`、`bd update --claim`(原子认领)、`bd dep add`、`bd show`、`bd prime`(注入工作流上下文与记忆)、`bd remember`。有独立 MCP server(PyPI `beads-mcp` 1.2.2)。

**hash 化 ID**(`bd-a1b2`)专为避免多 agent/多分支下的 ID 冲突——这是 markdown 方案(递增 ID)确实薄弱的一点,**本产品应认真考虑 ID 生成策略**。

### 4.5 GUI 生态 —— 对本产品的直接警示

beads 官方不做 GUI,但社区已建成庞大生态([community-tools.md](https://github.com/gastownhall/beads/blob/main/docs/community-tools.md)),其中**多个是 Tauri 桌面应用**:

- **Beadbox**(Tauri/Next.js,macOS 原生仪表盘,brew cask 分发)
- **BeadSpec**(Tauri/React,跨平台,依赖图 + 工作区分页)
- **Beads Task-Issue Tracker**(Tauri/Vue,跨平台)
- **Beadazzle**(原生 SwiftUI macOS)
- 另有多个 Web UI、VS Code / JetBrains / Neovim 插件

> **对本产品的警示**:「Tauri 桌面看板」本身**不是壁垒**——已经有至少 3 个团队为 beads 做出来了。本产品的护城河必须落在**同步事实 + 移动端 + 中文 + 完整产品化**的组合上,而不是「用 Tauri 做了个看板」。

### 4.6 社区反响:两极分化(重要)

**支持面**:HN 上有用户表示「它不与 gh issues 竞争,它与 markdown specs 竞争」「对跨多个上下文窗口的任务很有用」;Anthropic 官方受其启发(见 §5)。

**批评面(相当猛烈,且集中)**:

- 「Beads is a **good idea with a bad implementation**. It's not a designed product in the sense we are used to, it's more like a **stream of consciousness converted directly into code**.」([HN](https://news.ycombinator.com/item?id=46463757))
- ac29:「我真的很喜欢这个想法……但它就是不太好用。**bug 很多,而且这些 bug 会让 agent 困惑**,因为它被告知要按某种方式做事,而那种方式并不稳定生效。」
- laxori666:「**一直出现 merge conflict,agent 就直接保留其中一边而不是智能合并**,把我做任务的工作全毁了。到现在也没看到 beads 怎么解决这个问题。」
- azeirah:「作者的开发方式相当无常……beads 对一个本质上很简单的想法来说,是个**极难跟上的烂摊子**。」
- nmfisher:「读到『landing the plane』这个说法时我就开始找更简单的替代品了。」
- 文档与 README 被指为 AI 生成、自相矛盾(themgt 详述了 README 说 `beads.jsonl` 是 source of truth、但 main 分支上该文件已消失的困惑)。

**最有价值的市场信号:反弹催生了两个「回归 markdown」项目**

| 项目 | Star | 说明 |
| --- | --- | --- |
| [wedow/ticket](https://github.com/wedow/ticket)(`tk`) | **881** | 单个 bash 脚本,markdown + YAML frontmatter 存 `.tickets/`。自述「**written as a full replacement for beads**」「无需保持 SQLite 同步,也没有流氓后台守护进程搅乱你的改动」,并提供 `migrate-beads`。理由:「markdown 让 agent 容易检索,**而不是把上万字符的 JSONL 行灌进上下文窗口**」 |
| [hmans/beans](https://github.com/hmans/beans) | **909** | Go CLI,「Plain old Markdown files」存 `.beans/`,带 TUI 与 **GraphQL 查询引擎**(让 agent 精确取数、省 token)。HN 评价:「比 beads **侵入性小得多**……用起来是种享受」 |

> **这对本产品是强力的正面信号**:市场对「beads 太重太复杂」有真实不满,**「简单的 markdown 方案」有明确需求**。但注意 `ticket` 和 `beans` 都保留了**依赖图**和**结构化查询**——说明简单不等于放弃结构。

### 4.7 与本产品的重合度:**中**(理念对立,问题域相同)

同样为 agent 解决长程记忆与任务漂移,但**结论相反**:beads 认为要抛弃 markdown 换结构化数据库;本产品认为 markdown 够用、缺的是对账机制。二者是同一问题的两种答案。

---

## 5. Shrimp Task Manager

**仓库**:[cjo4m06/mcp-shrimp-task-manager](https://github.com/cjo4m06/mcp-shrimp-task-manager)

**数据(2026-08-30)**:2,150★、256 fork、MIT、最后 push **2025-08-21**(约 12 个月前)、46 个未决 issue/PR。

**是什么**:一个 MCP server,把任务管理以工具形式给 agent,强调思维链、反思与风格一致。工作流:`plan_task` → `analyze_task` → **`reflect_task`(反思)** → `split_tasks` → `execute_task` → `verify_task`。**共 15 个 MCP 工具**(源码 `src/index.ts` 逐一核对):`plan_task`、`analyze_task`、`reflect_task`、`split_tasks`、`list_tasks`、`execute_task`、`verify_task`、`delete_task`、`clear_all_tasks`、`update_task`、`query_task`、`get_task_detail`、`process_thought`、`init_project_rules`、`research_mode`。

> 注意其在 `analyze_task` 与 `split_tasks` 之间**显式插入 `reflect_task`(先分析 → 自我批判 → 再拆分)**,这个「反思步骤」的工作流设计本身值得借鉴,即便项目本身已停更。

**任务存哪(关键差异)**:`<DATA_DIR>/tasks.json` 单文件 + `<DATA_DIR>/memory/tasks_memory_<ts>.json` 历史备份。`DATA_DIR` 默认指向独立目录,**且项目 `.gitignore` 明确排除 `data/*`**。特别之处:它会在 `DATA_DIR` 下**另建一个私有 git 仓库**自动提交任务变更。

> 即:**任务有版本历史,但与用户代码仓库是两条互不相通的时间线**——不进 PR、不进 code review、不随分支切换。与本产品「随代码进仓库」的理念相反。

**GUI**:有两套——`ENABLE_GUI=true` 的内置轻量 WebGUI,以及 `tools/task-viewer/` 的独立 React 界面。

**是否解决漂移**:❌ 无代码扫描或对账机制。

**状态判断:事实上已停止维护(静默停更)**。12 个月无提交;2026-03 报告的构建失败 issue [#117](https://github.com/cjo4m06/mcp-shrimp-task-manager/issues/117) 挂了约 5 个月无回应;2026-08 新 issue 零回复;**未 archive 也无停更公告**,对使用者更具迷惑性。256 个 fork 中无人接盘。

**重合度:低**。纯 MCP、无看板事实源理念、已停更。**主要价值是反面教材:15 个工具的「工作流状态机」很难长期维护,且 star 数不代表当前健康度。**

---

## 6. Agent 内置 todo 机制的局限 —— 及一个重要反转

> ⚠️ **本节推翻了一个常见但已过时的前提。**「agent 内置 todo 都是易失的、扁平无依赖的」这一说法在 2025 年成立,但**对当前的 Claude Code 已不成立**——它现在会落盘、有依赖图、跨压缩与重启存活、可多 agent 共享。该说法目前**仅对 Codex 与旧版 `TodoWrite` 成立**。

### 6.1 Claude Code:TodoWrite → Tasks → 又在新模型上收回

**演进(引自官方 [CHANGELOG.md](https://github.com/anthropics/claude-code/blob/main/CHANGELOG.md) 与 [tools-reference](https://code.claude.com/docs/en/tools-reference))**:

1. **早期**:`TodoWrite`——**无状态整表覆写**,每次提交完整列表替换旧列表,状态仅 `pending`/`in_progress`/`completed`。
2. **v2.1.16(2026-01-22)**:「**Added new task management system, including new capabilities like dependency tracking**」。即 `TaskCreate`/`TaskGet`/`TaskUpdate`/`TaskList`,是**带任务 ID 的增量 CRUD**。Anthropic 工程师公开表示「We took inspiration from projects like **Beads** by Steve Yegge」([报道](https://paddo.dev/blog/from-beads-to-tasks/))。
3. **`TodoWrite` 现已默认关闭**:官方工具参考原文「Disabled by default in favor of `TaskCreate`, `TaskGet`, `TaskList`, and `TaskUpdate`」。

**存储位置(与流传说法不同,需更正)**:

- **`~/.claude/todos/` 是官方标注的遗留目录,当前版本不再写入**。[claude-directory](https://code.claude.com/docs/en/claude-directory) 原文:「`todos/`, `statsig/`, `logs/` — **Legacy directories from older versions. No longer written.**」
- **当前实际存储为 `~/.claude/tasks/{list-id}/{task-id}.json`**,单个任务 JSON 结构含 **`blocks` / `blockedBy`**——即**落在磁盘上的真实依赖图**。
- 流传的 `~/.claude/todos/<session-id>-agent-<agent-id>.json` 格式未找到一手依据,应视为描述旧版本,标注「未验证」。

**存活性与共享**(引自 [interactive-mode](https://code.claude.com/docs/en/interactive-mode#task-list) 与 [agent-teams](https://code.claude.com/docs/en/agent-teams)):

- 用户可见(`Ctrl+T`);**跨上下文压缩存活**(「Tasks persist across context compactions」);跨会话重启可恢复;
- `CLAUDE_CODE_TASK_LIST_ID=my-project` 可跨会话共享;**多 agent 共享采用文件锁认领**,且「a pending task with unresolved dependencies cannot be claimed until those dependencies are completed」,依赖完成后自动解除阻塞;
- 任务「persists locally and is **never uploaded**」,但受 `cleanupPeriodDays`(默认 **30 天**)清理。

**专用 hook 事件(值得本产品借鉴)**:`TaskCreated`(可阻止任务创建)与 `TaskCompleted`——官方说明「Use this to enforce completion criteria like **passing tests or lint checks** before a task can close」。二者均在 CHANGELOG 中独立可查(第 3504、4430 行)。

> **这是一个真正的差异化能力**:让「完成」可被外部规则校验,而不只是模型自称完成。**本产品的「同步事实」本质上是同一思路的加强版**——可考虑借鉴这种「完成需通过校验」的机制设计。

**关键反转**:CHANGELOG 明写:

> 「**Todo/task-tracking tools (TaskCreate/Get/Update/List, TodoWrite) are no longer available on Opus 4.8, Sonnet 5, Fable 5, Mythos 5, and newer models**; set `CLAUDE_CODE_ENABLE_TODO_TOOLS=1` to bring them back」

官方给出的理由(文档原文):「Those models keep track of multi-step work without a written checklist, and **the tools' definitions and reminders take up context**, so Claude Code leaves them out」。

> **含义有二**:①「被平台吞噬」的风险在**收缩**而非扩大;②Anthropic 自己认为**前沿模型不再需要「会话内」的书面 checklist**。这恰恰说明——**内置 todo 要解决的「会话内记忆」正在被模型能力吸收,而外部任务系统真正的价值在另一个层面:跨会话、跨人、跨仓库、人类可见、可评审的项目事实来源。**这正是本产品应当占据的位置。

### 6.2 Codex CLI:`update_plan`(结论与 Claude Code 相反)

源码核实([openai/codex](https://github.com/openai/codex)):

- **schema 硬性无依赖**:`PlanItemArg { step, status }`,配 `#[serde(deny_unknown_fields)]` + JSON Schema `additionalProperties: false`——**模型即使想塞入依赖字段也会解析报错**;且**无 per-item ID**,外部系统无法稳定引用某一步。
- **永不落盘**:`rollout/src/policy.rs` 将 `EventMsg::PlanUpdate` 归入注释为「**Transient, non-durable events**」的分支,返回 `false`。handler 一次成功调用的全部效果就是发事件并返回字符串 `"Plan updated"`,无任何文件 I/O。
- **只能靠考古恢复**:工具调用本身作为对话记录落在 `~/.codex/sessions/**/rollout-*.jsonl` 里,所以理论上可回放 transcript 重建最后一次调用参数——但**这是考古,不是任务存储**,不存在 plan 文件、任务表或任何「查询当前未完成步骤」的 API。
- ⚠️ 易踩的坑:`TurnItem::Plan` 确实持久化,但它是 `{id, text}` 的不透明文本(Plan Mode 的叙述),**不是** step/status checklist。
- 官方 `docs/` 中检索 `update_plan` **零命中**,是纯内部实现细节。

### 6.3 局限清单(按产品分列)

| 局限 | Claude Code Tasks | Codex `update_plan` |
| --- | --- | --- |
| 落盘持久化 | ✅ `~/.claude/tasks/` | ❌ 永不落盘 |
| 依赖图 | ✅ `blocks`/`blockedBy` | ❌ schema 硬性禁止 |
| 跨会话共享 | ✅ 需设 list ID | ❌ |
| 多 agent 协作 | ✅ 文件锁认领 | ❌ |
| 人类可见 | ✅ `Ctrl+T` | 仅当轮显示 |

**但对二者均成立的结构性局限**(这才是本产品的机会所在):

| 局限 | 说明 |
| --- | --- |
| **不在仓库、不在 git** | 存在家目录;`git clone` 拿不到;不随分支/PR 移动;**无法 code review、无法 diff、无法 revert** |
| **受自动清理** | Claude Code 默认 30 天 `cleanupPeriodDays` |
| **无人类写入路径** | 没有看板、没有表单,人只能看不能好好编辑 |
| **不与代码现实对账** | 完全依赖模型自觉更新 |
| **无跨项目视图** | 无法回答「我所有项目里现在最该做什么」;同一 list ID 跨仓库使用还会串台 |
| **厂商割裂** | Claude Code、Codex、Gemini CLI 各有各的机制,互不相通 |
| **新模型默认关闭** | 不可依赖 |

一条很切题的一手引证:[openai/codex#33890](https://github.com/openai/codex/issues/33890) 指出内置 plan 工具与外部任务系统会「creates **two competing planning surfaces**」。

> **建议采用的定性表述**:内置 todo 是 **agent 的工作记忆(working memory)**,不是**项目的事实来源(system of record)**。本产品要做的是后者。

---

## 7. 其他有代表性的项目

### 7.1 automazeio/ccpm(Claude Code PM)—— 8,357★

[automazeio/ccpm](https://github.com/automazeio/ccpm),MIT,Shell,最后 push 2026-03-18(约 5 个月,活跃度下降),仅 4 个 open issue。

规格驱动开发的 skill 系统,原则是「**No Vibe Coding** —— 每一行代码都必须能追溯到一份规格」,五阶段:Brainstorm → Document → Plan → Execute → Track。用 **GitHub Issues 做协作层 + git worktree 做并行执行**,本地 `.claude/prds/`、`.claude/epics/<feature>/` 存 PRD 与任务 markdown,`sync` 命令对齐二者。任务 frontmatter 带 `depends_on`、`parallel`、`conflicts_with`。用 **14 个 bash 脚本**做状态统计,理由是「即时输出,无 LLM 开销」——**这个「确定性脚本优先于 LLM」的取舍值得本产品借鉴**。

> ⚠️ 注意其 README **两处「source of truth」自相矛盾**:一处有整节叫「Why GitHub Issues」强调「**Issue state is project state**」、GitHub Issues 是 single source of truth;另一处又写「**Files are the source of truth** — plain markdown that lives in your repo, no external services」。实际是双轨制:本地 markdown 承载规格与上下文,GitHub Issues 承载协作与可见性,`sync` 对齐二者。
>
> **对本产品的直接启示**:「唯一事实源」是一个**极易在实现中被稀释**的承诺。一旦引入「仓库导入」「HTTP API」「移动端缓存」,就会出现多份状态。**本产品的表述必须比 ccpm 更严谨,并在架构上明确写死:谁是权威、冲突时以谁为准、其余各方何时失效。**这一点建议直接写进产品的设计文档首页。

### 7.2 wedow/ticket(`tk`)—— 881★ 与 hmans/beans —— 909★

见 §4.6。二者是「beads 太复杂」的直接反弹产物,均为 markdown/flat-file + 依赖图,分别用单个 bash 脚本与 Go 实现。**是本产品「简单 markdown」路线最有力的市场佐证。**

### 7.3 Priivacy-ai/spec-kitty —— 1,574★

[Priivacy-ai/spec-kitty](https://github.com/Priivacy-ai/spec-kitty),MIT,Python,**2026-08-30 当日仍在提交**。主张「**the repository remains the source of truth**」:规格、计划、工作包、验收标准全部以文件留在 `kitty-specs/`,每个 agent 分配隔离 worktree,提供 `next → review → accept → merge` 闭环与**可选本地看板**。⚠️ 679 个 open issue 相对 1,574★ 属偏高积压,稳定性「未验证」。

### 7.4 danjdewhurst/todo-md-mcp —— 6★(反面证据)

[todo-md-mcp](https://github.com/danjdewhurst/todo-md-mcp),MIT,TypeScript,**2025-07-14 当天创建当天完成,此后无更新**。4 个工具(`list/add/update/delete_todos`),以单个 `todo.md` 为唯一后端。存储手法值得借鉴——**用 HTML 注释藏持久 UUID**,兼顾人类可读与机器寻址:

```markdown
- [ ] First incomplete task <!-- id:550e8400-e29b-41d4-a716-446655440000 -->
- [x] Completed task <!-- id:6ba7b810-9dad-11d1-80b4-00c04fd430c8 -->
```

> **最重要的结论**:「把 TODO.md 包一层 MCP」6★、一天写完。**技术门槛极低,不构成任何护城河。**本产品的价值必须来自别处。

### 7.5 其他(备查)

| 项目 | Star | 说明 |
| --- | --- | --- |
| [williammb/cadenza](https://github.com/williammb/cadenza) | 2 | **Tauri + Rust 桌面任务板 + CLI**,与本产品技术栈几乎相同。agent 通过 Unix socket/命名管道 NDJSON 通信,**不开 TCP**;agent 只能「提议」,由人接受/拒绝。体量极小,但证明该形态已有人尝试 |
| [dayeggpi/KamKan](https://github.com/dayeggpi/kamkan) | 6 | Tauri 2 + React 的 markdown 看板,配套独立 MCP/Skill 仓库,纯 `.md` 无数据库 |
| [endlessblink/master-plan](https://github.com/endlessblink/master-plan) | 27 | 单个 `MASTER_PLAN.md`,**不走 MCP**,纯 Claude Code slash 命令。代表「skills 成熟后,MCP 不再是唯一入口」的趋势 |
| [ysamlan/vscode-backlog-md](https://github.com/ysamlan/vscode-backlog-md) | — | Backlog.md 的 VS Code 扩展(官方 README 推荐) |

---

## 8. 云端 / SaaS 路线(对照组,简述)

### 8.1 Linear MCP + Agent

Linear 的 MCP server **纯远程托管、无自托管选项**,端点 `https://mcp.linear.app/mcp`,OAuth 2.1 认证。官方文档不列工具名,第三方抓取显示约 **23–25 个**(具体数量「未验证」)。

「把 issue 指派给 agent」演进清晰:2025-05-20 [Linear for Agents](https://linear.app/changelog/2025-05-20-linear-for-agents)(agent 成为 first-class user)→ 2025-07-30 Agent 交互规范(**委派时用户仍是主 assignee,agent 是代其工作的 contributor**)→ 2026-06-11 [Coding sessions](https://linear.app/changelog/2026-06-11-coding-sessions):委派 issue 后 Linear 在云沙箱中调用 **Claude Code 或 Codex**,起草 PR 并把 diff 附回 issue,按 AI credits 计费。截至 2026-08-30,Agent API 仍为 Developer Preview。

### 8.2 GitHub Issues + Copilot cloud agent

官方文档已将「Copilot coding agent」更名为 **「Copilot cloud agent」**([docs](https://docs.github.com/en/copilot/concepts/agents/cloud-agent/about-cloud-agent)),2025-09-25 GA。流程:issue assignee 选 Copilot → 在 GitHub Actions 驱动的临时环境中改代码 → 开 PR 请你 review。

官方明确的限制(对本产品有利):**单次只能改一个仓库**、一次一个分支一个 PR、**每会话硬上限 59 分钟**、不兼容的 ruleset 会直接阻断、消耗 Actions 分钟 + AI credits,以及最关键的:「**Copilot cloud agent only works with repositories hosted on GitHub.**」

### 8.3 本地文件路线 vs. 云端路线

| 维度 | 云端 SaaS | 仓库内 markdown |
| --- | --- | --- |
| 任务存储 | 厂商数据库,API 访问 | 用户仓库内文件 |
| 网络与认证 | 强制 | 不需要 |
| 与代码同版本 | ❌ `git clone` 拿不到任务 | ✅ 随分支/PR 原子移动 |
| 离线 | ❌ | ✅ |
| 计费 | AI credits / Actions 分钟 | 零边际成本 |
| 锁定 | 强(Copilot 仅限 GitHub 托管) | 无,纯文本可迁移 |
| 可 diff / review / revert 任务 | ❌ | ✅ |
| **厂商生存风险** | ❌ **厂商关停 = 任务数据失效** | ✅ 随仓库天然存活 |

> **最后一行不是假设,而是已发生的事实**:Vibe Kanban 把任务搬上云端 Postgres 后,公司关停 → 看板功能随云端一起废掉(§2.1.1、§2.4)。**建议委托方把「厂商生存风险」作为产品叙事的正式卖点**——在 2026 年的市场环境下,这句话有真实说服力。

**但必须公允**:云端在「人与人的协作」上确有实质优势(session 属于组织、团队共享可见)。有意思的是,GitHub 官方论证云端 agent 优于 IDE 助手的理由是——本地会话中的决策「**untracked**,除非提交否则终将遗失」。**这句话恰恰是「把任务以 markdown 提交进仓库」最好的论据。**

正确定位是**互补**:面向人类团队的协调(SaaS 占优)vs. agent 本地化、git 原生、可移植的任务状态(markdown 占优)。

---

## 9. 与委托方构想的雷同度总表

### 9.1 总表

| 工具 | 事实源 | 进 git | agent 入口 | 看板 | 三端 | 解决漂移 | Star(08-30) | 活跃度 | 商业化 | **雷同度** |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **Backlog.md** | 一任务一 `.md` | ✅ | CLI(主)+MCP 20 工具 | 终端+本地 Web | ❌ 仅 localhost | 仅跨分支 | 6,573 | **很高**(周更) | 无 | **高** |
| **Vibe Kanban** | 本地 SQLite | ❌ | 编排 CLI + 自带 MCP | Web+Tauri | 桌面+Tailscale | 预防(worktree) | 27,953 | **已停运** | **失败退出** | **中** |
| **Task Master** | `tasks.json` 单文件 | ✅ | MCP 44 工具 + 46 命令 | VS Code 扩展 | ❌ | ❌ | 28,037 | 低(4 月停滞) | Hamster 付费 | **中** |
| **beads** | **Dolt DB(gitignore)** | ❌(走 `refs/dolt/data`) | CLI + MCP + skills | 社区(含多个 Tauri) | 社区补齐 | ❌ | 26,718 | **极高**(债也高) | 无 | **中**(理念对立) |
| **Shrimp** | 独立 `tasks.json` | ❌(独立私有 git) | MCP 15 工具 | 两套 WebGUI | ❌ | ❌ | 2,150 | **已停更** | 无 | **低** |
| **ccpm** | `.claude/` + GH Issues | ✅ 双轨 | Skills + bash | ❌ | ❌ | ❌ | 8,357 | 中(5 月未更) | 无 | **中低** |
| **ticket / beans** | `.tickets/`、`.beans/` markdown | ✅ | CLI(+GraphQL) | TUI | ❌ | ❌ | 881 / 909 | 中 | 无 | **中**(理念同盟) |
| **todo-md-mcp** | 单个 `todo.md` | ✅ | MCP 4 工具 | ❌ | ❌ | ❌ | 6 | 停更 | 无 | 低(但**理念最接近**) |
| **Claude Code Tasks** | `~/.claude/tasks/` | ❌ | 原生 | ❌ | ❌ | ❌ | — | 新模型默认关闭 | 平台内置 | **低** |
| **Linear / GH Copilot** | 厂商云 DB | ❌ | 官方 MCP / 指派 | ✅ SaaS | ✅ | ❌ | — | 高 | 成熟订阅 | **低** |

### 9.2 逐项差异点(本产品 vs. 全类目)

| 本产品设计 | 类目现状 | 判定 |
| --- | --- | --- |
| **TODO.md 唯一事实源** | Backlog.md 一任务一文件;Task Master 单 JSON(1.5 MB 已膨胀);todo-md-mcp 单文件但仅 6★ | ⚠️ **单文件是最大架构风险**。Task Master 的 1.5 MB 实测与 beads 的迁移史都指向同一结论:单文件在任务量增长 + 多 agent 并发写入下会出现 diff 噪音与合并冲突。**建议:单文件作为「视图/索引」,底层可分片;或明确限定目标用户为中小项目(Backlog.md 作者也是这么划边界的)** |
| **看板回写文件** | Backlog.md 已实现 | 重合,非差异点 |
| **Tauri 三端** | Vibe Kanban 有 Tauri 桌面(已停运);beads 社区有 ≥3 个 Tauri GUI;cadenza/KamKan 同栈 | ⚠️ **桌面端不是壁垒**;**移动端是真空**(无一家有原生移动应用) |
| **启动 <2s / 零卡顿 / 离线** | Backlog.md 是编译二进制,已很快;Task Master 被批 MCP 内存高 | 合理但难以作为卖点宣传 |
| **MCP + skills** | Backlog.md 明确判定 **MCP 是 legacy,CLI 才 canonical**;Task Master 44 工具吃 21K token;master-plan 证明 skills 可完全替代 MCP | ⚠️ **强烈建议同时提供一等公民 CLI**,并严格控制工具数量与描述长度 |
| **「同步事实」按钮** | **全类目无人实现** | ✅ **最硬的差异化**。最接近的只有 Task Master 的 Kiro hook(仅追加笔记、仅当前任务、仅 Kiro) |
| **仓库导入 TODO.md** | 无 | ✅ 差异化(注意 Vibe Kanban 因索要 GitHub 权限被用户明确抵制) |
| **代码注释 TODO/FIXME 扫描** | 仅 beads 生态的 [stringer](https://github.com/davetashner/stringer) 做类似事(挖 TODO 输出 JSONL 供 `bd init` 导入) | ✅ 基本差异化,已有可参考先例 |
| **curl 可用 HTTP API** | Backlog.md 有内部 REST 但硬编码 localhost、未文档化 | ✅ 差异化 |
| **ntfy 推送** | 全类目无 | ✅ 差异化(但需求强度「未验证」) |
| **默认中文** | Backlog.md i18n PR 被关闭;全类目英文为主 | ✅ **真实空位** |
| **商业化** | Backlog.md/beads 无意图;Task Master 靠 Commons Clause 保留云端;**Vibe Kanban 尝试后失败退出** | ⚠️ 见 §9.4 |
| (未提及)跨分支状态解析 | Backlog.md 已实现 | ❗ **建议补上**,否则多分支场景会被直接比下去 |
| (未提及)任务依赖图 | Backlog.md/beads/ticket/beans/ccpm **全都有** | ❗ **建议补上**,这是 agent 问「下一步做什么」的基础 |
| (未提及)ID 冲突策略 | beads 用 hash ID 专门解决多 agent 并发 | ❗ 单一 TODO.md + 多 agent 必然遇到,**需前置设计** |

### 9.3 这一类目的空白点 / 机会

**A. 真正的空白(无人做)**

1. **「同步事实」/ 代码 ↔ 任务对账** —— 类目内零实现,而 drift 是被反复承认的痛点(Yegge:「plans bit-rot very fast」;Task Master 官方:「Update as you go」)。**这是最值得押注的差异化。**
   - 但要注意:这个功能的**可信度**是产品成败关键。让 agent 扫代码判断任务是否完成,天然会有误判。建议设计成**「提议 + 人确认」**(cadenza 的 propose/accept 模型、ccpm 的确定性脚本优先,都是可借鉴的先例),而不是自动改状态。
2. **原生移动端** —— 全类目无一家有。Vibe Kanban 只能靠 Tailscale 访问 Web,Backlog.md 明确不承诺移动端对等。
3. **中文优先** —— Backlog.md 的 i18n PR 被关闭,是明确的空位。
4. **跨项目 / 跨仓库统一视图** —— HN 上有用户明确表达此需求(「我有 15 分钟,现在最该做什么?」),`backlog.md-hub` 这类小项目的存在也是佐证。类目内基本空白。

**B. 有先例但做得不好(可超越)**

5. **agent 上下文预算** —— Task Master 44 工具吃 21K token 被广泛诟病;beans 专门做 GraphQL 让 agent 精确取数省 token。**工具设计应「少而深」,并提供结构化查询而非让 agent 读整个文件。**
6. **多 agent 并发写入的冲突处理** —— beads 用户抱怨「agent 直接保留一边而不是智能合并」;这是 markdown 路线的固有难点,**谁先做好谁赢**。
7. **非 GitHub 场景** —— Copilot cloud agent 明确只支持 GitHub 托管仓库;本地文件路线天然覆盖 GitLab、自建、乃至非 git VCS。

**C. 需要警惕的风险**

8. **平台吞噬**:Anthropic 已做过 Tasks(受 beads 启发),虽在新模型上默认关闭,但随时可能回来。**对策:做平台不会做的部分——人的看板、跨会话跨项目、移动端、团队协作、中文。**
9. **「Tauri 看板」不是壁垒**:beads 社区已有至少 3 个 Tauri GUI,cadenza/KamKan 与本产品同栈。
10. **简单方案易被复制**:`todo-md-mcp` 一天写完。

### 9.4 关于商业化的专门提示

这是本次调研中**最需要委托方正视**的一点。类目内的商业化事实:

| 项目 | 商业化结果 |
| --- | --- |
| **Vibe Kanban** | 28k★、每天数千工程师使用、功能领先、**已上线「个人免费 + 团队 $30/人/月 + 企业版 SSO」的完整定价** → **仍因「绝大多数是免费用户,找不到让自己兴奋的商业模式」而关停公司**(2026-04-10) |
| **Task Master** | 用 **MIT + Commons Clause**(禁止他人做托管服务)为自家 Hamster 云保留空间;但 main 分支已停滞 4 个月,重心转向商业产品 |
| **Backlog.md** | 6.5k★、月下载 5.5 万,**明确不商业化**,纯 MIT 个人项目 |
| **beads** | 26.7k★,MIT,无商业化 |

**可提炼的结论**:

- **单机开发者工具 + 本地文件 + 开源**这个组合,**付费意愿极低**——Vibe Kanban 是用一整家公司验证出来的结论。
- ⚠️ **「做个团队版收 seat 费」这条最顺理成章的路,已经被 Vibe Kanban 走过并失败**($30/人/月,含 SLA 与 SSO)。原因推测(**未验证**):这类工具的使用是**个体开发者行为**,团队采购决策链条长,而个人用户完全可以停在免费档;同时开源本身让「自建」永远是零成本替代。
- 因此,若委托方坚持商业化,需要的是**比「团队协作」更硬的付费理由**。基于类目观察的候选方向(**均属推断,未验证**):
  1. **托管即价值**:让付费档提供**用户自己搭不起来**的东西——例如跨设备实时同步 + 移动推送的托管服务(ntfy 自建虽可行,但多端一致性与可靠投递有真实运维成本)。
  2. **不做开源,或延迟开源**:Vibe Kanban 与 Backlog.md 的对照说明,完全开源 + 本地优先几乎等于放弃对软件本身收费。Task Master 的 **Commons Clause** 是折中先例(保留自家托管权,代价是失去 OSI 开源身份与部分社区好感)。
  3. **中文市场 + 本地化服务**:这是类目内唯一无人占据的位置,且中文用户对「开箱即用的中文体验」的支付意愿与英文市场未必相同(**需自行验证**)。
  4. **面向企业的合规/私有化部署**:本地文件 + 离线 + 不上传,天然契合对代码保密敏感的组织——这与 §8 中云端路线的强制联网形成对比,是可收费的差异。
- **许可证选择需前置决策**,且应在写第一行代码前定下来:纯 MIT 意味着商业化只能靠服务;Commons Clause 保留托管权但不再是 OSI 开源。

> **一句总结**:本类目里,**开源 + 高 star 与「能赚钱」之间没有已被验证的通路**。委托方若把商业化当作硬目标,应当把它作为**产品设计的前置约束**(决定开源策略、付费点、目标客户),而不是产品做完之后再想。

---

## 10. 给委托方的八条具体建议

1. **保留「同步事实」作为第一卖点**,但实现为**「agent 提议 → 人确认」**而非自动改写。可参考 cadenza 的 propose/accept 与 ccpm 的「确定性脚本优先于 LLM」。
2. **重新评估「单一 TODO.md」**。Task Master 的 1.5 MB 实测、beads 的存储迁移史、Backlog.md 的一任务一文件选择,三条独立证据都指向单文件的扩展性风险。可考虑:TODO.md 作为人类可读的索引/视图,底层按任务分片;或明确把目标用户限定在中小项目。
3. **补上依赖图与跨分支状态解析**。前者是全类目标配(agent 回答「下一步做什么」的基础),后者是 Backlog.md 已有而本产品未提及的能力。
4. **CLI 与 MCP 并重,不要 MCP-first**。Backlog.md 的 MANIFESTO 明确把 MCP 降级为 legacy adapter,理由充分(token 预算、无需常驻、通用性)。同时严控工具数量。
5. **移动端是最干净的差异化**,全类目空白;桌面 Tauri 看板则不是壁垒,不要作为核心卖点宣传。
6. **商业化路径要在动工前想清楚**。Vibe Kanban 的关停说明「本地开发者工具」本身难以变现;把付费点放在**移动推送、托管同步**这类「必须有服务端才成立」的能力上,比对本地功能收费更可行——但注意它的团队版($30/人/月)也失败了,见 §9.4。

7. **⚠️ 云端能力必须是可选增量,绝不能替换本地路径。**这是 Vibe Kanban 用一家公司换来的教训:它在 2026 年 2 月把看板搬上云并**强制登录**,用户最珍视的「本地、离线、无需账号」被摧毁,社区当场决裂(issue #2687,33👍),两个月后公司关停。**委托方规划的 HTTP API、ntfy 推送、商业化,都必须建立在「不登录也能完整使用本地功能」的前提之上。**

8. **借鉴「任务 ↔ 物证」绑定**。Vibe Kanban 给每个任务记录 before/after HEAD commit sha + PR 状态,并强制 agent 完成后进 `InReview` 而非 `Done`。**若本产品在 TODO.md 中记录任务对应的 commit 范围与文件路径,「同步事实」就从「让 agent 重新理解代码」降级为「比对物证与当前 HEAD」——可信度和成本都会好一个量级。**

---

## 11. 「未验证」清单

以下内容未能以一手资料完全核实,引用时请注明:

- **Claude Code 旧版 `~/.claude/todos/<session-id>-agent-<agent-id>.json` 文件名格式**:广为流传但未找到一手来源,且与当前布局不符。
- **Codex 官方文档对 `update_plan` 的记载**:仓库 `docs/` 检索零命中;developers.openai.com/codex 未穷尽爬取,该部分「部分未验证」。
- **Linear MCP 工具的确切数量**:官方不公开清单,第三方抓取为 23–25 个,互相矛盾。
- **Task Master 团队重心转向 Hamster**:基于提交与发布节奏的推断,无官方声明。
- **Task Master 官方宣称的 200 万累计下载**:官网自述,未独立核算(npm 近一月实测 80,001)。
- **spec-kitty 的实际稳定性**:679 个 open issue,未实测。
- **ntfy 类推送在本类目的真实需求强度**:未找到直接证据。
- **本产品商业化建议中的付费点判断**:基于类目观察的推断,非经验证的市场结论。
- **beads Dolt 迁移的确切版本节点**:确认 v1.x 文档已全面以 Dolt 为准,但未逐版本追溯迁移时间线。
- **Vibe Kanban 的 X/Twitter 相关讨论**:本次调研环境无 X 登录凭据,该部分为二手信息;Discord 社群不可公开检索。
- **Vibe Kanban 迁移目标产品的排名**(Emdash / Nimbalyst / Agor / Chorus 的相对热度):来源可能带商业倾向,star 数为实测但「用户主要迁移去向」属推断。

---

## 附:主要一手来源

- Backlog.md:[README](https://github.com/MrLesk/Backlog.md)、[MANIFESTO.md](https://github.com/MrLesk/Backlog.md/blob/main/MANIFESTO.md)、[ADVANCED-CONFIG.md](https://github.com/MrLesk/Backlog.md/blob/main/ADVANCED-CONFIG.md)、[cross-branch-tasks.ts](https://github.com/MrLesk/Backlog.md/blob/main/src/core/cross-branch-tasks.ts)、[HN 44483530](https://news.ycombinator.com/item?id=44483530)
- Vibe Kanban:[README](https://github.com/BloopAI/vibe-kanban)、[停运公告](https://www.vibekanban.com/blog/shutdown)、[HN 44533004](https://news.ycombinator.com/item?id=44533004)
- Task Master:[README](https://github.com/eyaltoledano/claude-task-master)、[docs/task-structure.md](https://github.com/eyaltoledano/claude-task-master/blob/main/docs/task-structure.md)、[tool-registry.js](https://github.com/eyaltoledano/claude-task-master/blob/main/mcp-server/src/tools/tool-registry.js)、[LICENSE](https://github.com/eyaltoledano/claude-task-master/blob/main/LICENSE)
- beads:[README](https://github.com/gastownhall/beads)、[docs/index.md](https://beads.gascity.com/)、[sync-concepts.md](https://github.com/gastownhall/beads/blob/main/docs/core-concepts/sync-concepts.md)、[Yegge: Introducing Beads](https://steve-yegge.medium.com/introducing-beads-a-coding-agent-memory-system-637d7d92514a)、[HN 46463757](https://news.ycombinator.com/item?id=46463757)、[HN 46487580](https://news.ycombinator.com/item?id=46487580)
- Claude Code:[CHANGELOG.md](https://github.com/anthropics/claude-code/blob/main/CHANGELOG.md)
- Shrimp:[README](https://github.com/cjo4m06/mcp-shrimp-task-manager)
- 云端:[Linear MCP](https://linear.app/docs/mcp)、[Linear Coding sessions](https://linear.app/changelog/2026-06-11-coding-sessions)、[GitHub Copilot cloud agent](https://docs.github.com/en/copilot/concepts/agents/cloud-agent/about-cloud-agent)
