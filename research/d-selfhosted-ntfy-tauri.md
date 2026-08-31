# 开源自托管任务看板 + ntfy 通知生态 + Tauri 技术可行性调研

> 调研日期：2026-08-30
> 调研对象：为「开发者向 AI 时代 todo 工具」做选型与可行性论证
> 委托方产品设定：本地 `TODO.md` 为事实源 + 看板双向同步；Tauri(Rust) 桌面/移动/Web；SQLite 缓存索引；启动 < 2s；MCP 接入 Claude Code/Codex；curl CRUD API；ntfy 推送；在线+离线；要商业化
>
> 除特别标注外，本文数据均取自一手来源（官方 docs、GitHub API、仓库源码、release notes），并附 URL。GitHub 星数/时间戳取自 `gh api` 实时查询。

---

## 摘要：四个最重要的结论

先给最影响决策的四条，细节见正文。

1. **自托管看板阵营没有一个把「文件」当事实源**，它们全部是「数据库为事实源 + Web UI」。这既是委托方的机会，也解释了为什么 Vikunja/Planka 们解决不了「todo 在 git 里」——不是他们做不到，是这个假设与他们的多人协作产品形态冲突（见 [Part 1 §6](#6-结论自托管看板为什么没解决开发者todo-在-git-里的需求)）。**但真正的竞争压力不在它们身上，而在 `walm/todomd`、`amterp/kan` 这类 2026 年新出现的「markdown 为源 + agent CLI」小工具**，它们已经把委托方设想的核心机制实现了一遍（见 [Part 1 §5](#5-真正的竞品md-为事实源的-agent-友好工具)）。

2. **ntfy 自托管 + iOS 的隐私顾虑比传言小得多，但依赖是真实的。** 经**服务端源码验证**（`forwardPollRequest()` 的 POST body 为 `strings.NewReader("")`）：转发给 ntfy.sh 的只有 message ID 与 topic URL 的 SHA256，**消息正文不经过 ntfy.sh**，设备回自托管服务器取正文。不配 `upstream-base-url` 也能收到，但延迟「几十分钟到数小时」。
   ⚠️ 但有两个必须知道的约束：**① ntfy 定时投递最大只有 3 天**，无法作为长期提醒调度器；**② topic 同时授予读和写**，泄漏即可被伪造注入假提醒（见 [Part 2](#part-2ntfy-通知生态)）。

3. **Tauri 2 桌面端可放心用，移动端「证据不支持」，且有三个硬约束必须现在写进架构：**
   - **`updater` 插件在 iOS/Android 明确不支持**（官方矩阵 ❌），移动端更新只能走应用商店；
   - **sidecar 在移动端完全不可用**——维护者原话「sidecars are simply not supported on mobile」，且**二进制根本不会被打进 APK/AAB**。→ **推荐改用 `rmcp` crate 把 MCP server 编译进 Rust 主二进制**，五端统一且更快启动；
   - **移动端 `fs` 默认限制在应用沙盒目录**，且 iOS 文件监听用 Kqueue 有已知性能问题。**「本地 TODO.md 为事实源」在移动端不成立**，移动端只能是同步副本的消费者。

   **移动端的最强反面证据**：GitButler / Yaak / Hoppscotch / Spacedrive **全部没有移动端生成目录**；而 Spacedrive 这个真心想要移动端的旗舰 Tauri 项目，**最终选择了 React Native/Expo**。

4. **「保真回写」有了实测结论：所有 AST→序列化方案都不无损，唯一正解是字节区间外科手术。**
   实测 726 字节样本：`cmark()` 与 `cmark_with_source_range()` **均掉到 702**，comrak 涨到 727 且**主动注入合成注释**。**唯一可行方案是用 pulldown-cmark 只取字节偏移、绝不重新序列化**（见 [Part 3 §4](#4-文件监听与-markdown-解析放-rust-侧还是前端侧)）。

---

## Part 1：开源自托管任务/看板工具

### 0. 总览（GitHub 实时数据，2026-08-30 由 `gh api` 查询）

| 项目 | 技术栈 | Star | 最近 push | 最新 release | License | 数据存储 |
|---|---|---:|---|---|---|---|
| **Vikunja** | Go + Vue 3 | 5,205 | 2026-08-30 | v2.5.0 (2026-08-04) | AGPL-3.0 | SQLite/MySQL/Postgres |
| **Planka** | JS (React + Sails.js) | 12,463 | 2026-08-28 | v2.2.1 (2026-08-10) | ⚠️ fair-code 自定义 | PostgreSQL |
| **WeKan** | JS (Meteor) | 21,067 | 2026-08-30 | v11.29 (2026-08-30) | MIT | MongoDB |
| **Kanboard** | PHP | 9,835 | 2026-08-29 | v1.2.54 (2026-08-29) | MIT | SQLite/MySQL/Postgres |
| **Focalboard** | TypeScript + Go | 26,441 | 2026-05-18 | v8.0.0 (2024-06-13) | ⚠️ 已停维护 | SQLite/Postgres |
| **Leantime** | PHP | 11,474 | 2026-08-15 | — | AGPL-3.0 | MySQL |
| **OpenProject** | Ruby on Rails | 15,970 | 2026-08-30 | — | GPL-3.0 | PostgreSQL |
| **Memos** | Go + React | 62,642 | 2026-08-30 | v0.31.0-rc.1 | MIT | SQLite/MySQL/Postgres |
| **SilverBullet** | TypeScript (Deno) | 5,953 | 2026-08-30 | 2.10.0 (2026-07-28) | MIT | **Markdown 文件** |

一个立刻可见的模式：**除 SilverBullet 外，全部以数据库为事实源。**

---

### 1. Vikunja —— 最值得抄的 API 与通知设计

- 仓库已迁至 [`go-vikunja/vikunja`](https://github.com/go-vikunja/vikunja)（原 `go-vikunja/api`），Go + Vue 3，AGPL-3.0，5.2k star，活跃（v2.5.0，2026-08-04）。

**API 设计（对委托方「curl CRUD API」直接可借鉴）**

- 官方文档明确 v1/v2 双轨：「Starting with Vikunja 2.4.0 there is a second API version with standard REST verbs and an OpenAPI 3.1 spec…v1 stays supported, but new endpoints only land on v2; Vikunja 3.0 will deprecate v1 and 4.0 will remove it.」——[API Documentation](https://vikunja.io/docs/api-documentation)
- 认证推荐 **API token**：`Authorization: Bearer <token>`，在 Settings > API Tokens 生成。自托管实例可用 `/api/v1/login` 换 JWT；**Vikunja Cloud 不支持用户名密码，只能用 API token**。
- Swagger 自动生成，规范托管在 `/api/v1/docs.json`，公开实例 [try.vikunja.io/api/v1/docs](https://try.vikunja.io/api/v1/docs)。
- **可借鉴点**：`docs.json` 可被任何 OpenAPI 工具消费 —— 对委托方「MCP + curl API」双通道是天然契合的，MCP server 可以直接由 OpenAPI 规范生成。

**Webhook 机制（源码级验证，`pkg/models/webhooks.go`）**

结构体字段直接说明了设计意图：

```go
type Webhook struct {
    TargetURL string   // POST 目标
    Events    []string // 订阅事件，可用事件从 /api/v1/webhooks/events 获取
    ProjectID int64    // 项目级
    UserID    int64    // 用户级（与 ProjectID 互斥）
    Secret    string   // HMAC 签名密钥，writeOnly，响应中永不返回
    BasicAuthUser/BasicAuthPassword string // writeOnly
}
```

- **载荷格式**：`{"event_name": "task.created", "time": "...", "data": {"task": {...}, "doer": {...}}}`
- **签名**：设了 secret 就用 **HMAC-SHA256** 签原始 body，放在 `X-Vikunja-Signature` 头。
- **投递语义**：**只投一次，失败（HTTP ≥ 400 或超时）不重试**，默认超时 30s（`webhooks.timeoutseconds`）。
- **SSRF 防护**：多人实例上 webhook 可被用来探测内网，Vikunja 支持用 [mole](https://github.com/frain-dev/mole) 作出口代理（`webhooks.proxyurl`）。
- 来源：[Webhooks API](https://vikunja.io/docs/webhooks/)

> **给委托方的直接启示**：`writeOnly` 的 secret 字段、HMAC 签名头、出口代理防 SSRF，这三点是成熟度标志。委托方若做 webhook，建议**至少加重试 + 指数退避**——Vikunja 的「不重试」是公认的弱点。

**提醒/通知机制（`pkg/models/task_reminder.go`）**

源码里的相对提醒设计值得抄：

```go
const (
    ReminderRelationDueDate   ReminderRelation = `due_date`
    ReminderRelationStartDate ReminderRelation = `start_date`
    ReminderRelationEndDate   ReminderRelation = `end_date`
)
```

即提醒可以是绝对时间，也可以是「相对于截止日/开始日/结束日的偏移」，由 cron 定时扫描（`pkg/models/task_overdue_reminder.go`、`pkg/notifications/`）。通知出口包含站内 + 邮件（`pkg/notifications/mail.go`）。

- **原生不支持 ntfy**，但**用户级 webhook + ntfy 的 HTTP API 可以两行 curl 打通**，这是最常见的社区做法。

**商业化模式（对委托方最有参考价值的一点）**

README 明示：AGPL-3.0 开源 + [Vikunja Cloud](https://vikunja.cloud/) 托管 + [Vikunja Pro](https://vikunja.io/pro/)（admin panel、audit logs、time tracking 为 Pro 功能）。典型 **open-core + hosted** 三段式。

**优缺点**

- ✅ API 设计规范（OpenAPI 3.1）、webhook 有签名、支持 CalDAV（可被 Apple 提醒事项/Thunderbird 直连）、多视图（列表/看板/甘特/日历）、自托管友好（单二进制）。
- ✅ 有桌面端（`desktop/` 目录，GPL-3.0，本质是 Electron 壳）。
- ❌ 事实源是数据库，**没有任何「文件即数据」路径**；导出是一次性快照而非双向同步。
- ❌ AGPL-3.0 对想闭源商业化的团队是强约束（若委托方参考其代码需格外注意）。
- ⚠️ README 主动声明大量使用 LLM 辅助编码（`@tink-bot`），代码质量需自行判断。

---

### 2. Planka / WeKan / Kanboard / Focalboard

#### Focalboard —— **已明确停止维护**（重要结论）

这是本次调研需要纠正的一个常见认知。三条硬证据：

1. 仓库已从 `mattermost/focalboard` **迁移到 [`mattermost-community/focalboard`](https://github.com/mattermost-community/focalboard)**（社区托管区）。
2. README 顶部官方警告：
   > `[!WARNING]` This repository is currently not maintained. If you're interested in becoming a maintainer please [let us know here](https://github.com/mattermost-community/focalboard/issues/5038).
3. 最新 release **v8.0.0 停在 2024-06-13**，之后无新版本（`gh api` 实时查询）。

README 同时指明：独立版 Focalboard 已被冻结，Mattermost 插件形态迁到 [`mattermost/mattermost-plugin-boards`](https://github.com/mattermost/mattermost-plugin-boards)。

- 技术栈：TypeScript(React) 前端 + Go 后端，SQLite/Postgres，有 Swagger API。
- 26k star 但**不可作为选型对象**——这正是「star 数不等于活跃度」的典型案例。委托方若做竞品对比图，Focalboard 应标注为「已停维护」。

#### Planka —— 注意它已经**换成了非 OSI 许可证**

- 技术栈：React + Redux 前端，**Sails.js**（Node.js）后端，**PostgreSQL**。12.5k star，活跃（v2.2.1，2026-08-10）。
- ⚠️ **许可证已从 AGPL-3.0 改为 fair-code 自定义许可**。`gh api` 返回 `spdx_id: NOASSERTION`。官方 [License Guide](https://github.com/plankanban/planka/blob/master/LICENSES/PLANKA%20License%20Guide%20EN.md) 说明：
  - 采用 **Fair Use License + PLANKA Pro/Enterprise License** 双轨，「based on and extends the **Sustainable Use License** introduced by **n8n**」；
  - 文件名含 `.pe.` 的属 Pro/Enterprise，**不在社区许可范围内**；
  - **非主分支的内容不授权**；
  - 「For all other PLANKA-based hosting services or shared use of PLANKA accounts across organizations, you need to buy a commercial PLANKA license.」
  - 要求贡献者签 CLA，以便**将来可单方面改许可证**。
- **对委托方的意义**：这是一个「开源看板项目为了商业化而收紧许可」的最新案例（与 n8n 同源）。如果委托方要商业化，**fair-code / Sustainable Use License 是比 AGPL 更值得研究的模板**——它允许自托管自用，但禁止他人拿去做竞品 SaaS。
- ✅ 实时协作（Sails.js WebSocket）、UI 完成度高。❌ 依赖 PostgreSQL，部署比单二进制重；许可证不再是纯开源。

#### WeKan

- 技术栈：**Meteor**（JS），**MongoDB**。21k star，MIT，release 极其频繁（v11.29，2026-08-30，一天多版）。
- ⚠️ 维护风格特殊：README 明确「GitHub issues/PRs are only for FLOSS Developers, not for support, support is at wekan.fi/commercial-support/」——**社区支持被导流到商业支持**，这本身也是一种商业化模式（付费支持型）。
- ✅ 历史久、功能全、REST API 完整、MIT 宽松。
- ❌ Meteor 技术栈老旧、资源占用高（MongoDB + Meteor），前端性能被长期诟病；版本号飞涨但多为小修。对追求「零卡顿」的委托方无参考价值。

#### Kanboard

- 技术栈：**PHP**（无重框架），SQLite/MySQL/Postgres。9.8k star，MIT，活跃（v1.2.54，2026-08-29）。
- **API 是 JSON-RPC 2.0，不是 REST**（[API Reference](https://docs.kanboard.org/v1/api/)）。分两类：
  - **Application API**：用户 `jsonrpc` + settings 页 token，**不检查权限**，无 session，适合系统间集成；
  - **User API**：用户凭据或 personal access token，**检查应用角色与项目权限**，适合原生移动/桌面客户端。
  - 支持 **batch requests**——文档明说「particularly useful for mobile clients with higher network latency」。
  - 安全约定：三次认证失败需从登录表单解锁；v1.2.8 起启用 2FA 的用户必须用 API key。
- ✅ 极轻（PHP + SQLite 即可跑）、插件生态丰富、自动化动作（Automatic Actions）设计好。
- ❌ JSON-RPC 对现代前端/AI 工具链不如 REST/OpenAPI 友好（MCP 生成、curl 手搓都更别扭）；UI 老派。
- **给委托方的启示**：Kanboard 的「Application API vs User API」分层值得借鉴——**给 AI agent/脚本用的 token 与给人用的凭据应当分离且权限模型不同**。这正是 MCP 场景需要的。

---

### 3. Leantime / OpenProject（简述）

**Leantime**
- PHP 8.2+，MySQL，AGPL-3.0，11.5k star（[系统要求](https://leantime.io/all-about-open-source-project-management/)）。
- 定位特殊：「project management for the non-project manager」，**明确以 ADHD / 自闭症 / 阅读障碍用户为设计目标**（README），强调目标导向与认知负担。
- 商业化：[定价页](https://leantime.io/pricing/) 说明「Our open source version operates under AGPL licensing and requires code updates to be submitted back to the core code」，Pro 版提供 Whiteboard 等插件包 —— 又一个 **AGPL + Pro 插件** 的 open-core 案例。
- 与委托方相关性低（重团队协作、非开发者向、非文件驱动）。

**OpenProject**
- Ruby on Rails，PostgreSQL，GPL-3.0，16k star，非常活跃。
- 企业级 PM（甘特、预算、工时、BIM），有 Community / Enterprise 双版本（又一个 open-core）。
- 有完整 REST API v3（HAL+JSON）。
- 与委托方相关性最低：**重量级企业软件，与「开发者本地 markdown」是两个宇宙**。列出仅为完整性。

---

### 4. Memos / SilverBullet（md 与任务混合方案）

**Memos**（62.6k star，Go + React，MIT，极活跃）
- 定位是「short-form thinking / 快速捕获」，Markdown 时间线，不是任务系统。
- ⚠️ **关键区分**：README 称 "Markdown-native"，但**存储在 SQLite/MySQL/Postgres 数据库里，不是 `.md` 文件**。它的 "markdown" 指内容格式，不是存储介质。Docker 挂载 `~/.memos` 存的是数据库。
- 对委托方的价值：**它是「Markdown 格式 ≠ Markdown 文件为源」这一混淆的最佳反例**，可用于向用户解释差异化。
- 有 gRPC/REST API，MIT 许可宽松。

**SilverBullet**（5.9k star，TypeScript/Deno，MIT，活跃，v2.10.0）
- **本次调研中唯一真正以 Markdown 文件为事实源的项目**，也因此最值得委托方细读。
- 架构：纯 `.md` 文件构成 "space"，加**内置数据库与查询语言**，再加 **Space Lua** 脚本引擎（[官方介绍](https://silverbullet.md/SilverBullet)：「combines a clean live-preview editor with wiki-style linking, a built-in database and query language, and a fully integrated Lua scripting environment」）。
- v2 已发布（[公告](https://community.silverbullet.md/t/silverbullet-v2-released/3100)）。
- **它证明了委托方架构的可行性**：文件为源 + 索引数据库 + 查询层，正是委托方设想的「TODO.md 为源 + SQLite 索引」的同构方案。
- ❌ 它是笔记应用，任务只是 query 出来的视图；无移动原生端；无推送；非看板产品。

---

### 5. 真正的竞品：md 为事实源的 agent 友好工具

**这是本次调研最重要的发现。** 委托方设想的「TODO.md 为源 + 看板 + AI agent 接入」，在 2026 年已经有多个独立实现，且解决方案高度趋同。委托方必须知道它们的存在与设计。

#### `todomd/todo.md` —— 事实上的格式规范（284 star，2020 年起）

[TODO.md 格式规范](https://github.com/todomd/todo.md)（todomd.org）：

```markdown
# Project Name

### Column Name
- [ ] Task title ~3d #type @name yyyy-mm-dd
  - [ ] Sub-task or description

### Completed Column ✓
- [x] Completed task title
```

规则：基于 **GFM Task Lists**；section 即看板列；完成列名需含 `✓` 或 `[x]`；标签/负责人/估时/日期写在标题末尾。
- **对委托方的意义**：**不要另发明格式**。这是已有社区共识的先例，兼容它能直接获得 GitHub 页面可渲染、其他工具可读的红利。

#### `walm/todomd` —— 与委托方设想几乎重合（Go，MIT，2026-07 创建）

[README](https://github.com/walm/todomd) 自述：「A Kanban TUI and agent-friendly CLI over a plain markdown TODO.md. The markdown file is the single source of truth... Humans use the TUI; AI agents drive the CLI (`--json` everywhere, stable task IDs, meaningful exit codes).」

它已经解决了委托方必然会遇到的几个硬问题，**方案可直接借鉴**：

| 问题 | todomd 的解法 |
|---|---|
| **任务稳定 ID** | `<!-- id:3f2a -->` HTML 注释，渲染时不可见；4 字符 base36；手写任务下次写入时自动补 ID |
| **AI 不应看到自己的改动** | `todomd changes --as claude --ignore-author claude`：每个消费者一个游标，**自己的写入折叠进自己的游标**，不会回报给自己，但别人仍能看到 |
| **捕获一切来源的变更** | 对文件快照做 diff（存 `$XDG_STATE_HOME/todomd`），因此 CLI/TUI/`$EDITOR`/手改/formatter/`git pull` 全部能捕获 |
| **并发写安全** | 每次写取 advisory lock（锁放在 state 目录，**不放用户文件旁边**）+ 原子替换文件 |
| **外部编辑检测** | TUI 空闲时每 2s `stat` 文件，变了就重载并给变更卡片打徽标 |
| **描述保真** | 「Everything up to `#### Comments` / the next heading is the description, **preserved verbatim**」 |
| **归档** | 「Archive and history are **git's job**」；`archive` 命令拒绝执行除非文件已提交 git |

> ⚠️ **最值得注意的一句**（README 中记录的 commit 注释）：
> 「Considered goldmark; **hand-rolled round-trips better**.」
> 即：该项目评估过成熟 Markdown 解析器后，**为了保真回写而选择手写解析器**。这与 Part 3 关于 lossless markdown 的结论完全一致，是独立的交叉验证。

- 现状：**1 star，v0.9.0，单人项目**——威胁度低，但设计参考价值极高。
- 配套 [`walm/todomd-web`](https://github.com/walm/todomd-web)：「one Go binary, no database, the same TODO.md as the source of truth」，即 Web 看板也不引入数据库。

#### `amterp/kan` —— 另一种存储取舍（Go，Apache-2.0）

[SPEC.md](https://github.com/amterp/kan/blob/main/SPEC.md)：「Kanban boards that live in your repository... no database, no server — just files. Works with any VCS (or none).」

**关键架构决策与理由（直接关系到委托方的冲突设计）**：
> **Decision**: Each card is stored as a separate JSON file.
> **Rationale**: VCS tools handle merges at the file level. With one file per card:
> - Adding cards rarely conflicts (different files)
> - Conflicts only occur when two people edit the *same* card — a real conflict worth surfacing
> - The alternative (JSONL with one card per line) causes spurious conflicts on concurrent additions

**这是对委托方「单一 TODO.md」方案最尖锐的技术质疑**：单文件在 git 协作下会产生大量伪冲突。委托方需要有明确回应（见架构草案的取舍章节）。

#### 反面教材：`voku/agent-kanban` 的架构反思

[docs/architecture.md](https://github.com/voku/agent-kanban/blob/HEAD/docs/architecture.md) 记录了一次重构，把「Markdown 作为内部数据库」定性为**反模式**：
> This is the "Markdown as internal database" anti-pattern: two representations of the same facts (the array and the rendered text) that can drift apart, and a verifier that is really checking "did my own renderer's template not change" rather than "is this board healthy."

**教训**：如果 md 是渲染产物、内存对象是真源，两者必然漂移。委托方必须坚持**「文件是唯一真源，内存/SQLite 只是投影」**，且投影可随时从文件重建——方向不能反。

#### Obsidian 生态（旁证市场需求）

- [`obsidian-tasks`](https://github.com/obsidian-tasks-group/obsidian-tasks)：3,974 star，活跃 —— 在 md 文件里做任务查询。
- `obsidian-kanban`：4,483 star，但仓库已迁至 **`community-archive/obsidian-kanban`**，最后 push 2026-03，**原作者已退出**。

两者合计近 8.5k star，**证明「Markdown 文件里管任务」有真实且规模不小的需求**；后者的归档也说明**这个位置目前空出来了**。

---

### 6. 结论：自托管看板为什么没解决开发者「todo 在 git 里」的需求？

综合以上，原因有五层，从表层到根本：

**① 产品形态决定存储形态：它们都是多人协作工具。**
Vikunja/Planka/WeKan/OpenProject 的核心用例是「团队共享一个看板」。多人并发、权限（Vikunja 有 `notifications_permissions.go`、Kanboard 有项目级权限检查）、实时同步、审计——**这些需求天然指向中心化数据库**。一个躺在某人 git 仓库里的 `TODO.md` 无法表达「谁能看哪张卡」。

**② 「文件为源」与「服务器为源」在一致性模型上不可调和。**
一旦以文件为源，服务器就失去了权威性：用户可能 `git checkout` 到三个月前，可能在 vim 里整体重写，可能 rebase。服务器必须接受「事实源随时可能被外部整体替换」。这是**分布式系统难题**，而不是功能缺失。Part 3 §4 会说明这正是 Loro 的 `update()` 这类 diff 导入 API 要解决的问题。

**③ 目标用户不是开发者。**
Leantime 明确面向「non-project manager」，OpenProject 面向企业 PMO，Planka/WeKan 对标 Trello。**它们的用户不用 git，也不在终端里工作**。「todo 跟着代码走、跟着分支走、在 PR diff 里可见」对这些用户是无意义的；对开发者却是全部意义。

**④ 商业模式不鼓励。**
Vikunja Cloud、Planka Pro/Enterprise、Leantime Pro、WeKan 商业支持——**它们的收入都建立在「托管服务器」上**。若数据在用户的 git 仓库里，托管的价值大幅缩水。这是**结构性的动机缺失**：让数据离开服务器，等于削弱自己的商业模式。

**⑤ 真正尝试的人另有其人，且刚刚开始。**
如上节，`walm/todomd`（2026-07）、`amterp/kan`（2026-01）都是**2026 年新出现的**，且都是单人小项目、star 个位数。SilverBullet 做到了文件为源但不是看板。**这说明：需求被验证（Obsidian 生态 8.5k star），供给刚萌芽，窗口是真实存在的。**

> **给委托方的战略判断**：
> 差异化不应主打「自托管看板的替代品」——那是在别人的主场比拼多人协作。
> 应主打**「开发者 + AI agent 的本地任务事实源」**：todo 与代码同仓、同分支、同 PR，人用 GUI、AI 用 MCP，两者看到同一个文件。
> 真正的对手不是 Vikunja，而是「用户直接手写 TODO.md + 让 Claude Code 自己读」这个零成本方案。**产品必须证明自己比一个纯文本文件更有价值**——看板 UI、移动端提醒、跨设备同步就是那个增量。

---

## Part 2：ntfy 通知生态

### 1. iOS 推送的真相：自托管必须经 ntfy.sh 中转吗？

**这是本部分最重要、也最容易被二手资料讲错的一点。** 以下直接引自 ntfy 官方 config 文档源文件（`docs/config.md`，[GitHub raw](https://raw.githubusercontent.com/binwiederhier/ntfy/main/docs/config.md)）。

**结论：要「即时」推送，是的；但正文不经过 ntfy.sh。**

原文：
> To still support instant notifications on iOS through your self-hosted ntfy server, you have to forward so called `poll_request` messages to the main ntfy.sh server (or any upstream server that's APNS/Firebase connected, if you build your own iOS app), which will then forward it to Firebase/APNS.

配置：
```yaml
upstream-base-url: "https://ntfy.sh"
upstream-access-token: "..."  # 可选，超限或上游受保护时用
```

**完整数据流（官方原文列举）**：
1. iOS app 订阅 `https://ntfy.example.com/mytopic`
2. app 实际订阅 Firebase topic `6de73be8dfb7d69e...`（**即 topic URL 的 SHA256**）
3. 你发消息到自托管服务器 → 自托管服务器向 `https://ntfy.sh/<sha256>` 发 poll request，**该请求只含 message ID（`X-Poll-ID` 头）与 topic URL 的 SHA256**
4. ntfy.sh → Firebase → APNS → 你的设备
5. **设备收到触发后，回你的自托管服务器取真正的消息内容**并显示

官方给出的等价 curl：
```
curl -X POST -H "X-Poll-ID: s4PdJozxM8na" https://ntfy.sh/6de73be8dfb7d69e32fb2c00c23fe7adbd8b5504406e3068c273aa24cef4055b
{"id":"4HsClFEuCIcs","time":1654087955,"event":"poll_request","topic":"6de73...","message":"New message","poll_id":"s4PdJozxM8na"}
```

且官方明说：
> Note that the self-hosted server **literally sends the message `New message` for every message**, even if your message may be `Some other message`. This is so that if iOS cannot talk to the self-hosted server (in time, or at all), it'll show `New message` as a popup.

**服务端源码验证**（`server/server.go` 的 `forwardPollRequest()`）——不止看文档，实读实现：

```go
func (s *Server) forwardPollRequest(v *visitor, m *model.Message) {
	topicURL  := fmt.Sprintf("%s/%s", s.config.BaseURL, m.Topic)
	topicHash := fmt.Sprintf("%x", sha256.Sum256([]byte(topicURL)))
	forwardURL := fmt.Sprintf("%s/%s", s.config.UpstreamBaseURL, topicHash)
	req, err := http.NewRequest("POST", forwardURL, strings.NewReader(""))  // ← 请求体确为空
	req.Header.Set("X-Poll-ID", m.ID)
	...
```

`strings.NewReader("")` 证实 **POST body 是空的**；`"New message"` 是 ntfy.sh 侧为 poll_request 生成的占位文案，并非你的服务器上传。

**因此，泄露给 ntfy.sh 的信息仅为**：

| 数据 | 是否流经 ntfy.sh |
|---|---|
| 消息正文 / 标题 / tags / priority / actions / 附件 | ❌ **否**（由 iOS NSE 回源自取） |
| 消息 ID（`X-Poll-ID`，随机 12 字符） | ✅ 是 |
| `SHA256(base-url + "/" + topic)` | ✅ 是 |
| 你服务器的出口 IP | ✅ 是 |
| **消息发送的时间点与频率** | ✅ 是（最实质的元数据泄漏） |

> ⚠️ **一个文档未提及、但真实存在的风险**：`SHA256(topicURL)` **可被离线爆破**。域名通常可枚举，若 topic 名可猜（`alerts`/`todo`/`reminders`），第三方可通过字典攻击反推出 topic 名。**这把「topic 即密码」问题原样带到了 iOS 场景**——防御手段同样是高熵随机 topic 名。

**iOS 端已知故障模式**（[known-issues](https://docs.ntfy.sh/known-issues/)）：若通知**只显示 `New message` 而无正文**，说明 iPhone 联系不上你的自建服务器（NSE 回源失败）。**NSE 只有 25 秒执行窗口**——对「服务器在内网 / 走 VPN / 域名解析慢」的部署是常见故障。另需注意：`upstream-base-url` 要填 `https://ntfy.sh` 而**非你自己的域名**，且 `base-url` 必须与 iOS 端「Default Server」完全一致，这是高频踩坑点。

**不配 `upstream-base-url` 会怎样？** 官方：
> If `upstream-base-url` is not set, notifications will still eventually get to your device, but delivery can take **hours**, depending on the state of the phone. If you are using your phone, it shouldn't take more than **20-30 minutes** though.

对「todo 提醒」这种时效敏感场景，**这个延迟不可接受**——必须配 upstream。

**根因**：APNs 证书绑定 app bundle ID。ntfy.sh 官方 iOS app 用的是他们自己的证书，第三方服务器无法直接向该 app 推送。除非**自己发一个 iOS app 并用自己的 APNs 证书**（官方原文括号里也承认了这条路：「or any upstream server that's APNS/Firebase connected, **if you build your own iOS app**」）。

> 🎯 **对委托方的直接结论**：委托方自己要发 Tauri iOS app，**因此可以走「自己的 APNs 证书」这条路**，完全绕开 ntfy.sh 中转。ntfy 更适合定位为「给已有 ntfy 用户的可选集成」，而非委托方 iOS 推送的主通道。详见架构草案。

**Android 差异：完全不需要中转，明显更好。** 官方原文：
> "The ntfy Android app uses Firebase **only for the main host `ntfy.sh`**, and **only in the Google Play flavor**. **It won't use Firebase for any self-hosted servers**, and not at all in the F-Droid flavor."

- 开启 instant delivery 后靠前台服务维持长连接，**即使 doze 模式也即时送达**，代价是一条常驻通知；官方称耗电约「17 小时 0-1%」。
- 不开则走 FCM，延迟可达 15 分钟以上。

| | Google Play 版 | F-Droid 版 |
|---|---|---|
| 含 Firebase | ✅（仅用于 ntfy.sh） | ❌ 完全不含 |
| 自建服务器 | 走长连接，不走 FCM | 走长连接 |
| instant delivery | 可开关 | **强制常开，无法关闭** |

此外 ntfy 是 **UnifiedPush 的参考实现**（Android 去 Google 化推送的开放标准），Android 端还支持自定义请求头（适配 Cloudflare Access 等）与导入自签/mTLS 证书。

> **架构含义**：Android 可做到完全自主可控，iOS 做不到。**若要向用户承诺隐私，必须分平台如实说明**，不能笼统宣称「自建即完全私有」。

### 2. 机制核心：topic 即频道，也即密码

官方 [Publishing 文档](https://docs.ntfy.sh/publish/) 开宗明义：
> Topics are created on the fly by subscribing or publishing to them. Because there is no sign-up, **the topic is essentially a password**, so pick something that's not easily guessable.

- topic 命名限制：`[-_A-Za-z0-9]`，最长 **64 字符**。
- 发布即一条 HTTP PUT/POST：`curl -d "Backup successful" ntfy.sh/mytopic`

**优先级**（`X-Priority` / `Priority` / `prio` / `p`）：

| ID | 名称 | 行为 |
|---|---|---|
| 5 | `max`/`urgent` | 超长震动 + 弹出通知 |
| 4 | `high` | 长震动 + 弹出通知 |
| 3 | `default` | 默认 |
| 2 | `low` | 无声无震，不主动显示 |
| 1 | `min` | 折叠到「其他通知」 |

**标签/emoji**（`X-Tags`/`Tags`）：匹配 emoji short code 则渲染为 emoji 前缀，否则列在通知下方。适合做 `#bug`/`#feature` 或项目名标注。

**Action 按钮**（`Actions` 头）——**对 todo 场景价值最大的特性**：
```
-H "Actions: http, Open door, https://api.nest.com/open/yAxkasd, clear=true"
```
支持 `view`（开 URL）、`http`（直接发 HTTP 请求）、`broadcast`（Android 广播）。
> 🎯 **委托方可直接用它做「通知上直接完成任务」**：推送提醒时挂一个 `http` action 指向自家 API 的「标记完成」端点，用户不必打开 app。这是 ntfy 相对 Bark/Gotify 的显著优势。

**其他**：`Title`、`Click`（点击跳转）、`Attach`（附件 URL）、`Email`（转发邮件）。
UTF-8 标题若出现乱码，可用 RFC 2047 编码（`=?UTF-8?B?...?=`）。

#### 🚨 定时投递（`Delay`）有 3 天硬上限 —— 对本产品是关键约束

Header `X-Delay`（别名 `Delay`/`X-At`/`At`/`X-In`/`In`），支持 Unix 时间戳、时长（`30m`）、**自然语言**（`tomorrow, 3pm`、`Tuesday, 7am`）。

但 [官方文档](https://docs.ntfy.sh/publish/#scheduled-delivery) 明确：
> "the **minimum delay** you can set is **10 seconds** and the **maximum delay is 3 days**. This can be configured with the `message-delay-limit` option."

> ⚠️ **产品含义**：**绝不能把 ntfy 当作长期提醒调度器。** 一个 todo 应用要提醒「下个月的任务」，ntfy 直接做不到（ntfy.sh 官方实例固定 3 天；自建可调 `message-delay-limit` 但用户的自建实例不受你控制）。
> **必须自建调度器**（本地 cron / 服务端任务队列），在临近到期时才 publish；或在 3 天窗口内滚动续投。**这条应直接写进架构设计。**

#### Token 的一个重要限制

官方明示 token **无法降权**：
> "As of today, access tokens grant users **full access to the user account**... Granular access tokens are on the roadmap, but not yet implemented."

即：发给第三方应用的 token = 该账号全部权限。要做最小权限，只能**为每个用途单独建 user + 用 ACL 限制 + 再签 token**。每用户 token 上限 60 个。

### 3. 服务端关键参数（自托管要点，源自 `docs/config.md` 配置表）

| 配置项 | 默认值 | 说明 |
|---|---|---|
| `cache-duration` | **12h** | 消息缓存时长，**支撑 `since=` 与 `poll=1`**；设 0 关闭缓存 |
| `message-size-limit` | **4K** | 正文上限；官方警告：「FCM/APNS have limits around 4KB. If you increase this size limit, **FCM and APNS will NOT work** for large messages」 |
| `attachment-file-size-limit` | 15M | 单附件上限 |
| `auth-default-access` | **`read-write`** | ⚠️ **默认全开**，可选 `read-only`/`write-only`/`deny-all` |
| `enable-signup` | `false` | 默认禁止注册 |
| `visitor-request-limit-burst` | 60 | 每访客请求桶初始容量 |
| `visitor-request-limit-replenish` | 5s | 桶补充速率 |
| `visitor-message-daily-limit` | 未设 | 每访客每日消息上限 |
| `web-push-public-key` / `web-push-private-key` | — | 浏览器 Web Push（RFC8030），`ntfy webpush keys` 生成 |
| `web-push-expiry-duration` | 60d | 订阅过期清理 |

**部署形态**：官方描述为 "statically linked binary"，提供 tarball / deb / rpm / Docker，支持 **amd64、armv7、arm64**（树莓派可跑）。

**数据库是可选的**：默认 SQLite 分三个独立文件——`cache-file`（消息缓存）、`auth-file`（用户与 ACL，**设置即启用鉴权**）、`web-push-file`。三者**全部不设也能跑**（纯内存，重启丢缓存）。**最小可用部署无需任何数据库文件。** 也可用 `database-url` 切到 PostgreSQL（此时三个 file 配置必须留空）。

⚠️ **两个易踩的坑**：
1. **走反代必须设 `behind-proxy: true`**，否则所有访客被当成同一个 IP 共享限流；
2. **`keepalive-interval` 默认 45s，而 Android app 有硬编码的 77s 超时**——反代必须允许长连接、关闭响应缓冲（nginx `proxy_buffering off;`）、读超时远大于 keepalive，否则订阅流被反复掐断。

⚠️ **PostgreSQL 的隐式陷阱**：设 `database-url` 会隐式启用鉴权，但默认 `auth-default-access` 仍是 `read-write`，**匿名用户依然能读写所有 topic**。私有实例**必须显式设 `auth-default-access: deny-all`**。

- **Web Push 支持**（RFC 8030 / VAPID，`ntfy webpush keys` 生成密钥）：浏览器关闭也能收通知，对委托方 Web 端很关键。
  ⚠️ 但注意：推送端点由浏览器厂商提供（Chrome 即 `fcm.googleapis.com`），**自建 + 浏览器后台推送 ≠ 完全不经第三方**；且密钥对**不可轻易更换**（换了用户需手动清除通知权限才能重新订阅）。
- ⚠️ **附件 URL 无鉴权**：即使 topic 私有，附件链接也仅靠「不可猜的随机 message ID」保护（维护者原话：与 Google 的 secret URL 同理），默认仅存 3 小时。**敏感文件不要走 ntfy 附件。**
- 资源占用：官方文档未给内存基线（只说受 `nofile` 限制），社区常引用的「约 50MB」**未能从官方来源证实**。

**订阅侧 API（客户端实现要点）**

四种订阅格式：`<topic>/json`（NDJSON，**多数语言首选**）、`/sse`（浏览器 `EventSource`）、`/raw`（仅正文，无元数据）、`/ws`（WebSocket）。多 topic 用逗号：`ntfy.sh/t1,t2/json`。
事件类型含 `open`/`keepalive`/`message`/`message_delete`/`poll_request`——**客户端应只处理 `message` 但必须容忍其余类型**。

🚨 **v2.28.0 新增的 replay 限制（会影响现有客户端）**：
- 单 topic 重放响应上限 **10 MB**，被截断时带 `X-Messages-Truncated: 1` 头；
- **重放流量计入每日带宽预算**，超出返回 **HTTP 429**（ntfy 错误码 `42905`）。

> ✅ **最佳实践**：持久化最后一条消息的 `id`，轮询时用 `?poll=1&since=<lastId>`，**不要用裸 `poll=1`**（会重放整个缓存）。官方原话："If you are polling repeatedly, pass `since=<last message ID>` rather than re-fetching everything."

同版本还新增：title 上限 **1 KB**、tags 合计 **512 bytes**，超出直接 HTTP 400——**对把长文本塞进 title 的老客户端是破坏性变更**。

**认证与 ACL**（`docs/config.md` §Access control）：
> The access control list (ACL) manages access to topics for non-admin users, and for anonymous access (`everyone`/`*`).

```
ntfy access                            # 查看 ACL
ntfy access USERNAME                   # 查看某用户
ntfy access USERNAME TOPIC PERMISSION  # 授予/拒绝
```
- 支持 CLI 与 `server.yml` 的 `auth-access` 两种方式；权限含 read/write/read-write/deny，支持 topic 通配。
- token 认证：`Authorization: Bearer tk_...`；浏览器/EventSource 无法设头时可用 `?auth=` 查询参数（Base64 编码）。

### 4. ntfy Pro 定价

`ntfy.sh/pricing` 是 React SPA，HTML 中无价格文本。**数据改取自官方 live API `https://ntfy.sh/v1/tiers`**（价格单位为美分，比页面更权威；注意正确路径是 `/v1/tiers`，`/api/v1/tiers` 返回 404）：

| | **Free** | **Supporter** | **Pro** | **Business** |
|---|---|---|---|---|
| 月付 | $0 | **$6/mo** | **$12/mo** | **$25/mo** |
| 年付 | $0 | $60/yr | $120/yr | $240/yr |
| 每日消息 | **250** | 2,500 | 20,000 | 50,000 |
| 消息保留 | 12 小时 | 5 天 | 7 天 | 10 天 |
| 每日邮件 | 5 | 50 | 250 | 500 |
| 每日电话 | 0 | 3 | 20 | 50 |
| **🔑 预留 topic** | **0** | **3** | **10** | **50** |
| 附件单文件 / 总量 | 2MB / 20MB | 25MB / 500MB | 250MB / 2GB | 1GB / 10GB |

限流基准也有差别：免费档按 **IP**（`basis: "ip"`，与同 NAT/同 IPv6 /64 的人共享配额），付费档按**账号**（`basis: "tier"`）。

**Reserved topics 的真正意义**：这是在 ntfy.sh 公共实例上**唯一能获得真正访问控制的手段**。三种模式：「只有我能发布和订阅」/「我发布，所有人可订阅」/「我订阅，所有人可发布」。

| | 普通 topic（免费） | 预留 topic（付费） |
|---|---|---|
| 谁能订阅/发布 | **任何知道名字的人** | 仅你（或你授权者） |
| 名字被抢占 | 会（先到先得，无归属） | 归你所有 |
| 安全模型 | 靠名字保密（security by obscurity） | **真正的 ACL** |

- ntfy 服务端 **Apache-2.0**，自托管完全免费无限制（自建可用 `enable-reservations` 开启预留）。
- ⚠️ 已知问题：[#785](https://github.com/binwiederhier/ntfy/issues/785)（OPEN）——预留 topic 在 iOS 上有未解决的接收问题，若方案依赖此特性需先验证。

### 5. 第三方 app 集成 ntfy 的最佳实践

综合官方文档语义与生态实践，给委托方的具体建议：

**① 默认 topic 设计——绝不能可猜**
- ntfy 官方主题生成器的做法是「用户输入名字 + 追加随机后缀」（文档中示例生成 `wcK9OOx2Io`）。
- 委托方应：`todo-<user_id_hash>-<128bit 随机>`，**随机部分至少 16 字符**，在客户端本地生成，永不用邮箱/用户名等可枚举信息拼接。
- topic 上限 64 字符，空间充足。

**② 防止 topic 泄露**
- **视 topic 为密钥**：存进系统 keychain（Tauri 可用 `stronghold` 插件或 OS keyring），**不要写进 `TODO.md`、不要写进会被 git 提交的配置文件**——这对本产品尤其危险，因为产品的核心就是把文件放进 git 仓库！
  > ⚠️ **这是委托方的一个真实风险点**：如果把 ntfy topic 写进项目内的 `.todo/config.yml` 并被用户 `git push` 到公开仓库，等于公开了推送频道（任何人可发垃圾提醒，也可订阅窥探）。**配置必须存在用户级目录（如 `~/.config/`）而非项目目录**，或至少默认写入 `.gitignore`。
- **能用认证就用认证**：自托管时设 `auth-default-access: deny-all` + 为客户端发 token，比裸 topic 安全一个数量级。
- **托管场景用 reserved topics**（Pro 功能）防抢占。
- 敏感内容不放正文：**ntfy 服务端本身无 E2E 加密**（[#69](https://github.com/binwiederhier/ntfy/issues/69) 长期 OPEN），ntfy.sh 会缓存 12h。建议只推「你有 3 个任务到期」，详情回 app 看。

> 🚨 **必须注意 topic 的读写对称性**：topic 这把「密码」**同时授予读和写**。泄漏 = 攻击者既能**窃听你全部的任务提醒**，又能**伪造注入假提醒**。
> 对 todo 产品这是很实在的攻击面——想象攻击者注入「会议改到 3 点」或「该任务已取消」。
> **唯一根治手段是自建 + ACL + 读写分离 token**（`write-only` 给发送端，`read-only` 给客户端）。

官方 FAQ 对暴力破解的立场（供参考）：随机 10 位 topic 有 `64^10` 种可能，配合限流与 fail2ban，爆破不可行——**前提是你真的用了随机名**。社区建议熵值：≥11 字符约 64 bits，**16 字符约 96 bits 为推荐值**。

**③ 让用户完全可配置**
生态通用做法（Uptime Kuma、Home Assistant、Grafana、Healthchecks 等均如此）：暴露三个字段 —— **server URL（默认 `https://ntfy.sh`）、topic、可选 token/用户名密码**。委托方应照此设计，让自托管用户填自己的域名。

**④ 已有集成案例**（经源码级核验，非道听途说）

| 项目 | 状态 | 认证支持 | 备注 |
|---|---|---|---|
| **Home Assistant** | ✅ **核心集成，`quality_scale: platinum`**（最高等级） | token / 密码**互斥**建模 | 读 `manifest.json` 确认；含 `notify/sensor/event/update`，**能发布也能订阅** |
| **Uptime Kuma** | ✅ 配置面最完整 | 三选一：none / user+pass / token | 有 `ntfyPriority` 与 `ntfyPriorityDown` **分事件优先级** |
| **Healthchecks** | ✅ | **仅 token** | `priority=0` 兼作「静音」开关，设计巧妙 |
| **Apprise** | ✅ | 全支持 | 显式 `NtfyMode: CLOUD/PRIVATE`，**默认 PRIVATE** |
| **Shoutrrr** | ⚠️ **不支持 token**（仅 Basic） | — | Watchtower 委托它，故继承此缺口 |
| **Vikunja** | ❌ **无原生支持** | — | 全量 grep 仓库树 `ntfy` **零匹配** |
| **Grafana** | ❌ 仍 OPEN | — | 走通用 webhook |

- **Vikunja 确认无原生 ntfy**（[#481](https://github.com/go-vikunja/vikunja/issues/481) 以「通用 webhook」结案；[#8 Apprise Integration](https://github.com/go-vikunja/vikunja/issues/8) 仍 OPEN）。**这是 todo 品类的真实空白，是委托方的差异化机会。**
- ⚠️ 但需正视 Vikunja 维护者 kolaente 的反对理由：
  > "I still think **reminders should be mostly a responsibility of the client application**. If I need to be online (as would be the case with gotify) that's not an ideal solution."

  **他说得有道理**：服务端推送要求设备在线，本地调度不需要。**最佳方案是两者结合**——本地通知（`tauri-plugin-notification`）负责常规到期提醒，ntfy/APNs 负责跨设备与服务端触发场景。
- **Apprise**（MIT，Python）作为「通知路由器」支持 100+ 目标。**支持 Apprise URL 格式可一次拿到上百个渠道**，性价比极高。

**⑤ 集成配置面的行业共性**（所有被核验项目都收敛到同一形状）：`server URL` + `topic` + `auth` + `priority` + `title` + `tags`。
**核验的项目中没有一个硬编码 ntfy.sh 而阻止自建**——这应视为事实标准，务必遵守。

> 🔑 **来自 [issue #143](https://github.com/binwiederhier/ntfy/issues/143) 的警告，直接针对「分发型应用」**：
> "**If a developer publishes an application that contains an hardcoded topic anybody can extract it and use it to send notifications.** (…receiving a large number of spam messages would consume battery and create a **DoS vulnerability**)"
>
> 即：**绝不可在客户端硬编码 topic**——它会被逆向提取，导致所有用户被垃圾推送 DoS。topic 必须每用户随机生成。

### 6. 对比 Gotify / Pushover / Bark，以及「todo 提醒」选型建议

| 维度 | **ntfy** | **Gotify** | **Pushover** | **Bark** |
|---|---|---|---|---|
| 形态 | 自托管 + 托管 | 自托管 | 纯 SaaS | 自托管 + 官方服务 |
| Star / 活跃 | 33.8k，v2.28.0 (2026-08) | 15.8k，v3.1.0 (2026-08) | 闭源 | App 9.0k / server 3.6k |
| License | Apache-2.0 | Apache-2.0(server) | 闭源 | MIT |
| 后端 | Go + SQLite/PG | Go + SQLite/PG | — | Go |
| **iOS 原生 app** | ✅（自托管需 upstream 中转触发） | ❌ **无官方 iOS app** | ✅ 成熟 | ✅ **专为 iOS 而生** |
| Android app | ✅（+UnifiedPush 参考实现） | ✅ | ✅ | ❌ |
| 模型 | **topic pub/sub** | app token | user/app key | key |
| 优先级 | 1–5 服务端语义 | 0–10 客户端解释 | 5 级含 **Emergency 需确认** | `level` 含 **timeSensitive / critical** |
| Action 按钮 | ✅ | ❌ | 有限 | 有限（`url` 跳转） |
| 定时投递 | ✅ `Delay` | ❌ | ✅ | 需第三方 |
| E2E 加密 | ✅ 客户端 | ❌ | ✅ | ✅ `ciphertext` |
| 成本 | 自托管免费 / 托管付费 | 免费 | **$4.99 一次性/平台**；Teams $5/user/月；发送方 1 万条/月免费 | 免费 |

（Pushover 价格源自 [官方定价页](https://pushover.net/pricing) 与 [支持文档](https://support.pushover.net/i8-how-much-does-pushover-cost-is-there-a-subscription)；Bark 能力源自 [Finb/Bark README](https://github.com/Finb/Bark)。）

**各家要点补充**
- **Gotify**：致命短板是**没有官方 iOS app**（[gotify/android#238](https://github.com/gotify/android/issues/238)、[server#87](https://github.com/gotify/server/issues/87)、[#262](https://github.com/gotify/server/issues/262) 均已关闭而无 app），公认阻碍是 $99/年 Apple 开发者账号。社区 iOS 客户端用轮询而非 APNs。**颇具讽刺意味的是**：Gotify 用户的 iOS 变通方案最后常常绕回 ntfy。对需要 iOS 提醒的 todo 产品**基本出局**。
  但它有两点真实优势值得记下：**per-app token 卫生**（可单独吊销某个来源而不影响其他，这点比 ntfy 的「token 即全权」更干净）、以及**投递路径中完全没有 Google/FCM 依赖**。
- **Pushover**：最省心、可靠了十余年，**Emergency 优先级需用户确认否则反复响**——这是逾期任务的理想能力。但闭源 SaaS、不可自托管、终端用户要付费（对推广是摩擦）。
- **Bark**：**中国 iOS 开发者的事实标准**。MIT，可自托管（`bark-server`，Go），支持通知分组、自定义图标铃声、**`level=timeSensitive`（专注模式下仍可显示）与 `level=critical`（无视静音与勿扰）**、`ciphertext` 加密推送、`call=1` 持续响铃 30 秒。URL 极简：`https://api.day.app/:key/:title/:body`。**缺点：仅 iOS，无 Android/桌面**。
- **ntfy 的短板**（需诚实告知）：默认无认证的 topic 模型是「脚枪」；ACL 相对后加，配置体验不如 token-first 设计优雅；topic 容易膨胀。

#### 🎯 「todo 提醒」场景选型建议

todo 提醒的特殊性：**时效敏感、频率中等、内容含私密项目信息、用户跨 iOS/Android/桌面、且用户是开发者（愿意自托管、在意隐私）**。

**推荐分层策略（而非单选）**：

1. **主通道 —— 自建推送（APNs + FCM），不依赖任何第三方。**
   委托方本就要发布自己的 iOS/Android app，拥有自己的 APNs 证书与 FCM 项目。**这是唯一能做到「低延迟 + 正文不出自家服务器 + 用户零配置」的方案**，也是商业产品应有的样子。前两家（ntfy/Gotify）之所以有中转问题，正是因为它们没有你的 app——而你有。

2. **可选通道 —— ntfy（首选的第三方集成）。**
   面向「已经在用 ntfy 的自托管开发者」这一与目标用户高度重合的群体。理由：Apache-2.0、可完全自托管、**有 action 按钮（能在通知上直接完成任务）**、有定时投递、跨 iOS/Android/Web/桌面/CLI。
   实现成本极低：一个 HTTP POST。让用户填 server URL + topic + token。

3. **可选通道 —— Bark（面向中国 iOS 用户）。**
   若目标市场含中国开发者，Bark 的接入成本同样是一个 GET/POST，且 `timeSensitive`/`critical` 对「逾期任务」语义精准。**性价比很高，建议与 ntfy 一并支持。**

4. **可选通道 —— Apprise 兼容 URL。**
   一次实现，覆盖 Pushover/Discord/Slack/Telegram/邮件等上百目标，避免逐个适配。

5. **不推荐 Gotify** 作为一等公民（无 iOS 官方 app），但因其 API 简单，若走 Apprise 可顺带覆盖。

> **一句话结论**：**自建 APNs/FCM 做默认，ntfy + Bark 作为一等公民的可选集成，其余经 Apprise 兜底。** 不要把 ntfy 当作唯一推送通道——那会让 iOS 用户的提醒延迟取决于第三方，或让你被迫依赖 ntfy.sh 中转。
>
> 另外无论选哪家：**提醒调度必须由你自己掌握**（ntfy 的 `Delay` 上限 3 天，Bark 需第三方，Gotify 无此功能）。本地用 `tauri-plugin-notification` 做设备内提醒，服务端用任务队列在临近到期时才推送。

#### ntfy 速查表（供实现时参考）

| 关切点 | 结论 |
|---|---|
| 自建 + iOS 是否必经 ntfy.sh | **是**（要即时性）；但只传 message ID + SHA256(topicURL)，**POST body 为空**（源码验证） |
| 不设 upstream 的代价 | 仍能收到，延迟 20-30 分钟至数小时 |
| Android | **不需要中转**；F-Droid 版完全无 Firebase |
| Token | `tk_` 前缀；**无法降权，等同账号全权**；每用户上限 60 |
| ACL | read-write / read-only / write-only / deny，支持 `*` 通配，`everyone` 代表匿名 |
| **定时投递** | 最小 10 秒，**最大 3 天** → 长期提醒必须自建调度器 |
| 消息大小 | 正文 4K（FCM/APNS 硬限约 4KB）；v2.28.0 起 title ≤1KB、tags ≤512B |
| 免费档 | 250 条/天、缓存 12h、带宽 200MB/天、**预留 topic 0 个** |
| 付费 | Supporter $6/mo、Pro $12/mo、Business $25/mo |
| 反代 | **必设 `behind-proxy: true`**；keepalive 45s < Android 硬超时 77s；关闭缓冲 |
| Topic 安全 | **topic 名 = 密码，且同时授予读+写**；建议 ≥16 字符随机；真安全需自建 ACL 或付费预留 |
| 最新版本 | **v2.28.0**（2026-08-27），是一次限流「硬化」发布 |

---

## Part 3：Tauri 技术可行性

### 1. Tauri 2.x 现状与移动端成熟度

- **Tauri 2.0 稳定版发布于 2024-10-02**（`gh api` 查 `tauri-v2.0.0` tag 的 `published_at`），距今约两年。
- 当前 2.11.x（`tauri-v2.11.5`，2026-07-01）。主仓 **110,670 star**，Apache-2.0/MIT 双许可。
- 移动端（iOS/Android）是 **Tauri 2 的官方一等特性**（v1 时代不支持），这是 v2 的最大卖点之一。

**官方插件平台支持矩阵**（直接摘自 [plugins-workspace README](https://github.com/tauri-apps/plugins-workspace)，✅=（部分）支持，❌=不支持，?=未知/未测试/计划中）：

| 插件 | Win | Mac | Lin | **iOS** | **And** |
|---|:-:|:-:|:-:|:-:|:-:|
| **sql** | ✅ | ✅ | ✅ | **✅** | **✅** |
| **notification** | ✅ | ✅ | ✅ | **✅** | **✅** |
| **deep-link** | ✅ | ✅ | ✅ | **✅** | **✅** |
| **fs** | ✅ | ✅ | ✅ | **?** | **?** |
| **shell** | ✅ | ✅ | ✅ | **?** | **?** |
| **updater** | ✅ | ✅ | ✅ | **❌** | **❌** |
| store | ✅ | ✅ | ✅ | ✅ | ✅ |
| http | ✅ | ✅ | ✅ | ✅ | ✅ |
| clipboard-manager | ✅ | ✅ | ✅ | ✅ | ✅ |
| dialog | ✅ | ✅ | ✅ | ✅ | ✅ |
| log / os / opener / upload | ✅ | ✅ | ✅ | ✅ | ✅ |
| autostart / cli / positioner / single-instance / window-state | ✅ | ✅ | ✅ | ❌ | ❌ |
| stronghold / process / websocket / global-shortcut / persisted-scope / localhost | ✅ | ✅ | ✅ | ? | ? |
| biometric / nfc / haptics / geolocation / barcode-scanner | — | — | — | ✅ | ✅ |

**对委托方的三条硬结论**：

1. ✅ **`sql`（SQLite）全平台支持**，含 iOS/Android。这是好消息，SQLite 缓存方案在移动端可行。
2. ❌ **`updater` 在 iOS/Android 明确不支持**。移动端只能走 App Store / Google Play 更新。桌面端可用 updater 做自动更新（商业产品必备）。**这必须写进发布计划**。
3. ⚠️ **`fs` 与 `shell` 在移动端标注为 `?`（未知/未测试）**。这两个恰恰是本产品的命脉（文件为事实源 + MCP 子进程）。详见下文。

**最低系统版本（一手核实，纠正流传的错误说法）**

从 `crates/tauri-cli/schema.json` 默认值直接读出：`android.minSdkVersion = 24`（**Android 7.0**）、`iOS.minimumSystemVersion = "13.0"`。
> ⚠️ 网上流传的「Tauri 最低 Android 10 / iOS 16」是错误的（源自 AI 生成的 SEO 内容）。

**官方对移动端 DX 的自我评价**（[Tauri 2.0 发布公告](https://v2.tauri.app/blog/tauri-20/) 原文，值得原样引用）：
> "**We are not completely happy about the developer experience at the moment** but are actively improving to bring it up to par with the desktop experience."
> "On mobile **not all of the official plugins are supported**."

**已知坑（需要提前规划）**

- **移动端文件系统语义完全不同**：iOS 应用沙盒内没有「用户的 git 仓库」这一概念（且 `fs` 插件默认限制在 Application folder）。**「本地 TODO.md 为事实源」在 iOS 上根本不成立**——用户的代码仓库不在手机上。移动端必须重新定义为「远端/同步副本的消费者」。这是产品设计问题，不是技术问题，但必须现在想清楚。
- **iOS 工具链与 Xcode 版本强耦合，构建期问题多**（均为 OPEN issue）：[#15066](https://github.com/tauri-apps/tauri/issues/15066)（Xcode 26 linker error）、[#9292](https://github.com/tauri-apps/tauri/issues/9292)（2024-03 至今无法构建 iOS）、[#15931](https://github.com/tauri-apps/tauri/issues/15931)（NVM 环境下找不到 Node）。
- **签名/上架摩擦点集中**：[#15741](https://github.com/tauri-apps/tauri/issues/15741)（export options 漏 `provisioningProfiles`）、[#15663](https://github.com/tauri-apps/tauri/issues/15663)（app extension 丢 entitlements）、[#13952](https://github.com/tauri-apps/tauri/issues/13952)（externalBin 导致 Mac App Store 上传失败）。
- **移动端基础适配能力仍缺失**：SafeArea 控制（[#11475](https://github.com/tauri-apps/tauri/issues/11475)，2024-10 至今 OPEN）、屏幕方向控制（[#13408](https://github.com/tauri-apps/tauri/issues/13408)）。
- **Google Play 发布无法自动化**（官方 [文档](https://v2.tauri.app/distribute/google-play/) 承认）：「**Tauri currently does not offer a way to automate the process of creating Android releases**」，首次上传必须手动。
- ⚠️ **国产 ROM 风险**：官方明确不支持华为 WebView / HarmonyOS（[#7287](https://github.com/tauri-apps/tauri/issues/7287)，2023-06 至今 OPEN）。**对面向中国开发者的产品这是实质风险。**

> ✅ **关于「App Store 会不会因为用了 Tauri 而拒审」**：检索 tauri 仓库**未发现**任何因框架本身导致的**审核政策性拒绝**；检索到的均为构建/签名/上传的**技术性失败**。**不应夸大此风险。**
- **WebView 碎片化**：Android 用系统 WebView（版本随设备/厂商差异极大，低端机与国产 ROM 尤甚），iOS 用 WKWebView，Windows 用 WebView2，Linux 用 WebKitGTK。**Linux 的 WebKitGTK 是公认最容易出渲染/性能问题的一环**。跨端 CSS/JS 兼容性成本高于 Electron（Electron 全平台同一个 Chromium）。
- **iOS 上架**：需 Apple 开发者账号（$99/年）；Tauri 生成标准 Xcode 项目，可正常提交，但需注意隐私清单（Privacy Manifest）等 Apple 近年新要求。

### 2. 启动性能：能否稳定 < 2 秒？

**桌面端：能，且通常远优于 2 秒。**
Tauri 不打包 Chromium，用系统 WebView，二进制体积（几 MB～十几 MB）与内存占用显著低于 Electron（后者通常 100MB+ 安装体积、冷启动常在 1–3 秒）。社区普遍报告 Tauri 桌面冷启动在**几百毫秒级**。

**启动耗时的实际构成（决定优化方向）**：
1. **WebView 初始化**——通常是大头，且**不受你控制**。Windows 上 WebView2 若未常驻，首次唤起成本最高。
2. **前端 bundle 解析与首屏渲染**——**这是你唯一能大幅优化的部分**。
3. Rust 侧初始化（打开 SQLite、建立文件监听、读配置）——若同步阻塞会直接加到启动路径上。

**给委托方的可执行建议（用于守住 < 2s）**：
- 前端 bundle 严格控制体积，首屏只渲染看板骨架；
- **Rust 侧启动时不要同步扫描/解析整个仓库**：先从 SQLite 缓存读上次的投影渲染 UI，再异步校验文件 mtime/hash 并增量更新；
- 文件监听、索引重建、网络同步全部放到首帧之后；
- 用 Tauri 的 splashscreen 仅作兜底，不要用它掩盖真实的慢启动。

#### 官方基准数据（唯一可信的一手来源）

Tauri 官方持续运行基准 [tauri-apps/benchmark_results](https://github.com/tauri-apps/benchmark_results)（gh-pages，2026-08-27 更新），指标为「进程启动到 `DOMContentLoaded`」，hyperfine 3 次预热 + 10 次取平均。`hello_world` 的 `exec_time.mean`：

| 平台 | Tauri hello_world | 数据时间 |
|---|---|---|
| Linux | **0.741 s** | 2026-08-26 |
| macOS | **1.405 s** | 2026-08-26 |
| Windows | **0.591 s** | 2026-08-26 |
| Electron hello_world | 0.475 s | ⚠️ **2023-09-24（已过期约 3 年）** |

**必须诚实指出**：在 Tauri **自己的**基准里，Tauri hello world（0.59–1.41s）**并不快于** Electron（0.475s）。但该对比不成立——Electron 数据停更于 2023-09，且官方 README 明确警告不同 OS 的 runner 速度不同、结果不可直接横比。且这些是 **CI 虚拟机上的 hello world**，非真实应用。**不应据此承诺任何数字。**

Tauri 真正确定且可信的优势是**体积**：同一数据集中 Tauri 二进制约 **2.7–3.0 MB**，Electron 约 **166 MB**（约 55 倍）。

#### ⚠️ Windows WebView2 的长尾陷阱

微软官方 [WebView2 performance 文档](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/performance) 明确：
> "A common performance bottleneck is the time to create a WebView2 control and navigate to a webpage for the first time. **On a cold launch, WebView2 must spin up its processes and disk caches, which can introduce a noticeable delay.**"

微软还特别建议：**优先静态 HTML 而非 SPA 框架**（SPA 必须先下载/解析/执行整个框架代码才能显示任何内容）。

**实证病态案例**：[tauri#13727](https://github.com/tauri-apps/tauri/issues/13727) 报告 Windows 启动超 **20 秒**。排查确认**与 Tauri 无关**——以**管理员权限**运行时 WebView2 的 `EnsureCoreWebView2Async` 本身即阻塞 20 秒（纯 WinForms + WebView2 可复现），微软侧对应 [WebView2Feedback#5300](https://github.com/MicrosoftEdge/WebView2Feedback/issues/5300)。
> 👉 **切勿让应用以管理员权限运行。**

#### 🚨 重要警示：网上流传的 Tauri 基准数据大量造假

搜索会大量返回诸如「Tauri 2.0 冷启动 320ms vs Electron 1800ms」「Tauri iOS 110ms / Android 90ms，1000 次运行 95% 置信区间 <2%」这类**看似精确**的数字。

**经核实为伪造**：某文声称的基准仓库 `github.com/cross-platform-benchmarks/2026-hybrid-test-harness` 经 `gh api` 查询返回 **404（仓库不存在）**；同批文章还称「Tauri 最低 Android 10 / iOS 16」，与一手配置（**API 24 / iOS 13**，见下）直接矛盾。**判定为 AI 生成的 SEO 内容，请勿引用。**

> ⚠️ **移动端无任何可信公开冷启动数据**——官方 benchmark 只覆盖桌面 CI。移动端还需叠加 Rust `.so` 加载、JNI 初始化、系统 WebView 进程拉起（低端机显著更慢）。**必须在真机上做 spike 实测，把 < 2s 作为可回归的 CI 指标而非假设。**

### 3. 关键插件可行性

- **`tauri-plugin-sql`**：✅ 全平台（含 iOS/Android）。基于 **sqlx 0.8**（`sqlite = ["sqlx/sqlite", "sqlx/runtime-tokio"]`）。

  ⚠️ **需澄清一处官方文档自相矛盾**：插件**子 README 写 iOS = `x`（不支持）**，但那张表出自 2024-10-02 的 PR #1857 且此后未更新；**根 README 矩阵与文档站均标注 iOS "Full support"**，且源码 `plugins/sql/src/lib.rs` 中**不存在任何 `target_os = "ios"` 门禁**。**结论：iOS 支持成立**，子 README 的 `x` 是历史遗留错误。
  （备注：`Cargo.toml` 有 `# TODO: bundled-cipher` —— **若日后需要 SQLCipher 加密，官方尚未提供**。）

  **注意**：它是「在 JS 侧执行 SQL 字符串」的模型；若要复杂逻辑与类型安全，**更推荐在 Rust 侧直接用 `rusqlite` 或 `sqlx` 写业务命令，通过 `invoke` 暴露**——性能与可维护性都更好，也便于 Web 端复用同一套后端逻辑。
- **`tauri-plugin-fs`**：桌面 ✅、移动受限。**v1 的 `tauri-plugin-fs-watch` 在 v2 已并入 `fs`，成为可选 feature**（源码 `plugins/fs/Cargo.toml` 实证）：
  ```toml
  notify = { version = "8", optional = true, features = ["serde", "serialization-compat-6"] }
  notify-debouncer-full = { version = "0.6", optional = true }
  [features]
  watch = ["notify", "notify-debouncer-full"]
  ```
  即需显式启用 `features = ["watch"]`，API 为 `watch()`（防抖，可设 `delayMs`）与 `watchImmediate()`。**底层确为 `notify` v8 + `notify-debouncer-full`**。

  🚨 **iOS 文件监听有已知性能问题，且根因已定位**：[plugins-workspace#1823](https://github.com/tauri-apps/plugins-workspace/issues/1823)「fs watch() and watchImmediate() in iOS so slow」（2024-09，**至今 OPEN**）。
  原因是 `watcher.rs` 直接用 `notify::RecommendedWatcher`，无平台特化，而 `notify` 的平台映射为：Linux/**Android** → inotify（高效）、macOS → FSEvents（高效）、Windows → ReadDirectoryChanges（高效）、**iOS → `KqueueWatcher`**。Kqueue **需为每个被监视文件持有一个 fd**，开销大且易触及上限——这正是「so slow」的根因。

  🚨 **移动端 fs 作用域限制（对本产品是根本性约束）**：官方文档对 android/ios 均标注 *"Access is restricted to Application folder by default"*。**即移动端无法监听用户任意位置的 `TODO.md`，只能在应用沙盒目录内。**
  iOS 另需自建 `PrivacyInfo.xcprivacy`（`NSPrivacyAccessedAPICategoryFileTimestamp` + 理由码 `C617.1`），否则上架会被 Apple 隐私清单检查拦截。
- **`tauri-plugin-notification`**：✅ 全平台。本地通知可用于「到期提醒」，移动端需运行时权限。**但它是本地通知，不是远程推送**——app 未运行时的推送仍需 APNs/FCM 或 ntfy。
- **`tauri-plugin-deep-link`**：✅ 全平台。可用于 `todoapp://task/abc123` 从通知/浏览器唤起并定位任务。
- **`tauri-plugin-updater`**：桌面 ✅ / **移动 ❌**（见上）。
- **sidecar（`externalBin`）—— 能否打包 MCP server 二进制？**

  官方 [Embedding External Binaries](https://v2.tauri.app/develop/sidecar/) 说明：在 `tauri.conf.json` 的 `bundle.externalBin` 列出二进制，文件名须带 `-$TARGET_TRIPLE` 后缀（如 `my-sidecar-aarch64-apple-darwin`）；运行时经 `tauri_plugin_shell::ShellExt` 的 `app.shell().sidecar("my-sidecar").spawn()`，并需在 capabilities 里授予 `shell:allow-execute` / `shell:allow-spawn`，还可用正则约束允许的参数。

  **关键判断：sidecar 是桌面能力，移动端不可用。** 依据三点：
  1. sidecar 完全建立在 **`shell` 插件**之上，而 `shell` 在官方矩阵中对 iOS/Android 标为 `?`；
  2. 官方文档通篇只给桌面 target triple 示例，无任何移动端说明；
  3. **iOS 在系统层面禁止应用 `fork`/`exec` 派生子进程**——这是 App Store 审核红线，非 Tauri 能绕过。

  **维护者原话确认**（[tauri#9774](https://github.com/tauri-apps/tauri/issues/9774)，FabianLars）：
  > "**As far as i know, sidecars are simply not supported on mobile.**"
  > "**I don't think the binary is even included in the apk/aab so no.**"
  > "It's not planned right now but it's not a definite 'no, never' yet."

  即 **二进制根本不会被打进 APK/AAB**。该 issue 至今 OPEN，而官方 sidecar 文档页**仍未加注移动端不支持**——这是文档缺陷，极易误导选型。

  ⚠️ **即使只在桌面用 sidecar 也有代价**：[tauri#13952](https://github.com/tauri-apps/tauri/issues/13952) 显示使用 `externalBin` 后**上传 Mac App Store 失败**（签名被 Tauri 覆盖），维护者称目前只能自行重新完整签名。

  ##### ✅ 更好的解法：把 MCP server 编译进主二进制，而非作为 sidecar 进程

  **MCP 官方 Rust SDK 存在且活跃**：[modelcontextprotocol/rust-sdk](https://github.com/modelcontextprotocol/rust-sdk)（3,855 star，2026-08-29 仍在推送），crate 名 **`rmcp`**，最新 **3.1.4**（2026-08-20），累计下载 2,298 万。

  用 `rmcp` 作为 **library 链接进 Rust 主二进制**，收益显著：
  - **绕开 sidecar 的移动端禁令**，五端同一份实现；
  - 无需为每个 target triple 交叉编译并打包外部二进制；
  - **规避上述 App Store 签名问题**；
  - 省掉拉起子进程的开销，**直接服务 < 2s 冷启动目标**。

  代价：MCP server 需用 Rust 实现。**若现有 MCP server 是 Node/Python 且必须复用**，则只能「桌面 sidecar + 移动端不提供 MCP（或连远程）」做功能分级。**这个决策必须前置**，它会反向影响整体架构。

##### MCP 传输方式的选择

MCP 规范定义两种标准传输：**stdio**（子进程，由客户端拉起）与 **Streamable HTTP**。对本产品：

| 形态 | 传输 | 适用 |
|---|---|---|
| **桌面（主力）** | **stdio** | Claude Code / Codex 的默认与最省心方式；用户在配置里指向你的二进制即可 |
| **app 运行中** | **Streamable HTTP**（本地回环端口） | app 已运行时，MCP 可复用进程内索引与文件锁，**无需重复解析、无并发写冲突** |

> 💡 **建议同时提供两者**：`your-app mcp` 子命令走 stdio（覆盖「app 没开」的场景），app 运行时额外监听本地端口走 Streamable HTTP。两者共用同一份 `rmcp` 实现与同一个 core crate，边际成本很低。
>
> ⚠️ **关键并发风险**：stdio 模式下 **MCP server 进程与 app 可能同时写 `TODO.md`**。两者必须共用前文所述的 **advisory lock + 原子替换**，否则会写坏用户文件。这也是 §4 建议「解析与写入逻辑收敛到同一个 Rust core crate」的另一个理由。

### 4. 文件监听与 Markdown 解析：放 Rust 侧还是前端侧？

#### 明确建议：**全部放 Rust 侧。**

理由：
1. **前端不可信也不持久**：WebView 可能被系统回收、窗口可能关闭；而文件监听与冲突处理必须持续可靠。
2. **Web 端根本没有文件系统**：解析逻辑若写在前端，Web 端也用不上；写在 Rust 侧则可编译进服务端，Web 走 API 复用同一套逻辑。**一份解析器，三处复用（桌面/服务端/CLI）**。
3. **性能与原子性**：读-改-写 + 文件锁 + 原子替换必须在一处完成，跨 IPC 做这件事极易出错。
4. Rust 侧可直接复用为**独立 CLI 与 MCP server 二进制**（同一个 crate 出三个产物）。

#### 文件监听：`notify` crate 的现实

- Rust 事实标准是 [`notify`](https://github.com/notify-rs/notify)，macOS 用 FSEvents、Linux 用 inotify、Windows 用 ReadDirectoryChangesW。
- **必须使用 `notify-debouncer-full`**：编辑器保存文件通常是「写临时文件 → rename 覆盖」，会产生 `Remove`+`Create` 而非 `Modify`，且常有重复/乱序事件。不去抖必然出现漏事件与抖动。
- ⚠️ **自写自触发的反馈环**：你的 app 写 `TODO.md` 会触发自己的监听器。标准解法有三种，建议叠加使用：
  1. 写入前后短暂**暂停监听**（有竞态风险，不能单独依赖）；
  2. 记录**自己写入内容的 hash**，事件回来时比对，一致则丢弃（**最可靠**）；
  3. 按 `walm/todomd` 的思路，给每个变更来源一个**游标/作者标记**，自己的写入不回报给自己。
- **移动端**：iOS/Android 上 `notify` 不适用（沙盒无外部文件可监听），移动端应改为「监听远端同步事件」。

#### Markdown 保真回写：本次调研的技术核心

#### 🔬 实测结论（非推断）：本次调研实际构建了 Rust 测试工程逐一验证

用一份 **726 字节的「恶意」`TODO.md`** 样本实测——包含 YAML frontmatter、HTML 注释、混用 `*`/`+`/`-` 列表符、`_`/`__` 强调、4 空格与 Tab 混合缩进、行尾双空格硬换行、`1.` 与 `2)` 两种有序列表、链接引用定义、裸 HTML 块、GFM 表格：

| 方案 | 726 字节样本往返结果 |
|---|---|
| pulldown-cmark + `cmark()` | ❌ **726 → 702** |
| pulldown-cmark + `cmark_with_source_range()` | ❌ **726 → 702**（source_range 变体**也救不回来**） |
| comrak `format_commonmark` | ❌ **726 → 727** |
| markdown-rs | ❌ **根本没有序列化器** |
| panache-parser（rowan CST） | ✅ 726 → 726（但见下方否决理由） |
| **✅ 字节区间外科手术（pulldown-cmark 偏移）** | ✅ **完全一致** |

**具体被破坏的内容（实测）**：`_`→`*`、`__`→`**`、所有列表符号（`+`/`-`）→`*`、Tab 缩进→2 空格、空白被折叠。

**comrak 更糟——它会主动「发明」内容**：把行尾双空格硬换行改成 `\`、**注入合成的 `<!-- end list -->` 注释**、并**把链接引用定义内联展开**。对「保留用户原文」需求是灾难性的。

> 📌 **关键更正**：`cmark_with_source_range()` 常被认为是保真回写的救命稻草（它确实能保住**转义字符**原貌）。**实测证明它同样丢失 24 字节**，不能用于整篇往返。

**库状态核实**：

| 库 | Star | 状态 | 位置信息 | lossless? |
|---|---:|---|---|---|
| **pulldown-cmark** | 2,699 | 活跃，v0.13.4，下载 **1.43 亿**（事实标准） | ✅ `into_offset_iter()` 给 **`Range<usize>` 字节区间** | ❌（但**不需要它无损**，见方案） |
| **comrak** | 1,687 | 活跃，v0.54.0 | ⚠️ `sourcepos` 是**行/列非字节**，需自建行首索引换算 | ❌ 且会发明内容 |
| **markdown-rs** | 1,564 | ❌ **最后 commit 2025-04-23**，约 16 个月休眠 | ✅ mdast+position | ❌ **公开 API 仅 `to_html`/`to_html_with_options`/`to_mdast`，无序列化器** → **应直接剔除** |

- comrak 另有实证 round-trip 缺陷 issue [#685](https://github.com/kivikakk/comrak/issues/685)。
- **独立交叉验证**：`walm/todomd` 的结论是「Considered goldmark; **hand-rolled round-trips better**」——不同生态、不同团队、同一结论。

#### 📌 存在一个真正无损的 Rust CST 库，但现在不能用

**`panache-parser`** 确实做到字节级无损（实测 726→726），底层是 **rowan**（与 rust-analyzer 同一套 CST 库，技术路线正确），显式建模 `YAML_METADATA`/`BLANK_LINE`/`HTML_BLOCK_RAW`，甚至有 `TASK_CHECKBOX` 节点。

**但仍否决，理由（均已核实）**：
1. **方言不对**——它解析 **Pandoc/Quarto/RMarkdown**，不是 CommonMark/GFM；
2. **实测漏解析**——样本中 9 个 checkbox 只找到 8 个，**漏掉 `2) [x]`**。对 TODO 应用，**漏掉一个任务是不可接受的正确性缺陷**；
3. **成熟度不足**——累计下载仅 **1,971 次**（对比 pulldown-cmark 1.43 亿），**12 个月发了 42 个版本**（仅 2026-08 就 0.23→0.28），API 剧烈变动，pre-1.0。

> **建议：记录在案并持续观察**。若未来 1.0 且支持 GFM，是理想解。

##### 🎯 推荐方案：**按字节区间做「外科手术式」修改，而非「解析 → 重新序列化」**

这是本报告对委托方最重要的技术建议：

**不要**：`parse(md) -> AST -> mutate -> render(AST) -> write`（必然丢格式）
**而要**：
1. 用 **pulldown-cmark 的 `OffsetIter`** 解析，拿到每个任务项在原文中的**精确 byte range**（它提供 `Event` 与 `Range<usize>` 配对）；
2. 建立「任务 ID → byte range + 结构化字段」的索引，存入 SQLite；
3. 修改时**只替换需要改动的最小 byte 区间**（如把 `- [ ]` 改成 `- [x]`，或在某行末尾追加 `@due(2026-09-01)`），**文件其余部分逐字节原样保留**；
4. 移动卡片（跨列）= 剪切一个区间 + 插入到另一位置，同样是字节操作；
5. 写入用**原子替换**（写临时文件 + `rename`）+ **advisory lock**。

**这样能天然保住**：用户的注释、缩进风格、非任务段落（前言、说明、代码块）、行尾空格、Windows CRLF、YAML front matter、以及任何解析器不认识的内容。

##### ✅ 已实测通过的安全边界

- **勾选切换绝对安全**：`TaskListMarker` 的区间**恰好是 `[ ]` 这 3 个字节**；全部切换再切回 = **字节完全一致（726）**。
- **CRLF 被保留**；**无末尾换行的文件**正常处理；**Unicode/emoji 安全**（区间落在合法字符边界）；marker 后多余空格不计入区间。

##### 🔥 实测踩到的真实 bug：`Tag::Item` 区间会嵌套重叠，会吞掉后续标题

这是本次调研对工程落地最有价值的一条——实现「跨列移动任务」时**静默损坏了文件**：用 `Tag::Item` 区间做整行展开时，某个 item 区间**吞掉了紧随其后的 `## In Progress` 标题**，产出 `"+ [ ] Item...\n\n## In Progress\n"`。

两个反直觉事实（实测）：
1. **`Tag::Item` 的区间会嵌套/重叠** —— 实测 `263..381` 完全包含 `298..341`。**朴素地把 item 区间当作互不相交的「行块」是错的。**
2. **某些 item 区间以 `\n` 开头**（前导换行被算进区间）。

**修复方案（已验证有效）**：不要直接用 `Tag::Item` 原始区间。应把每个任务块**钳制（clamp）到它自己的 marker 所在行**——结束位置取「下一个 marker 行的行首」与「下一个 `Heading`/`Rule`」中**较早者**，再**向前回退跳过空行**。

##### 必须处理的其他陷阱

1. **偏移失效**：一次 UI 操作的所有编辑**收集成批，按 `range.start` 降序（从后往前）应用**，前面的偏移才不受影响。切忌逐条编辑逐条重解析。**并断言各区间互不重叠。**
2. **换行符**：检测并沿用 CRLF/LF，否则一次保存改掉全文件行尾，git diff 爆炸。
3. **末尾换行 / BOM**：记录原状并保持。
4. **YAML frontmatter**：pulldown-cmark 需显式启用 `Options::ENABLE_YAML_STYLE_METADATA_BLOCKS`，否则会被当普通内容甚至解析成 thematic break。**建议 frontmatter 区间整体跳过，永不改写。**
5. **代码块内的伪任务**：``` 块里的 `- [ ]` **绝不能**被识别为任务。
6. **跨 section 移动**：必须显式重新缩进并明确定义列表符号策略（因 item 区间嵌套且目标层级缩进不同）。
7. **文件仅 KB 级**：每次写盘后直接重新解析即可，**不要过早优化**做增量。

##### 四条必测断言（建议写进 CI）

1. **零编辑恒等** —— 解析→零编辑→写回，字节**完全相同**；
2. **切换对合性** —— toggle 两次回到原状；
3. **Golden 文件** —— 含 **CRLF 变体**与**无末尾换行变体**；
4. **未触碰行的多重集不变** ← **正是这条断言捕获了上面的吞标题 bug**，务必实现。

**配套约定**：
- **稳定 ID 用 HTML 注释**：`<!-- id:3f2a -->`（GitHub 渲染时不可见），与 `walm/todomd` 一致；这样任务改名/移动都不丢身份。
- **兼容 `todomd/todo.md` 社区格式**（`### 列名` + `- [ ]` + 行尾元数据），不另造格式。
- 只在用户显式要求时才做「格式规范化」（canonicalize），默认绝不重排全文。

### 5. Web 端复用与移动端结论

**同一前端跑 Tauri + Web 的标准做法**：定义一个平台抽象层（如 `TodoRepo` 接口），提供两个实现——Tauri 下走 `invoke()` 调 Rust 命令，Web 下走 `fetch()` 调远端 API；构建时用 Vite 环境变量或运行时检测（`window.__TAURI__` / `isTauri()`）选择注入。业务组件对此无感知。

要点与坑：
- **能力差异必须在 UI 上显式降级**：Web 端没有本地文件访问（除非用 File System Access API，且 Safari 支持差），「打开本地仓库」这类入口应隐藏而非报错；
- 认证模型不同（桌面可存 keychain，Web 靠 cookie/token）；
- Web 端有 CORS，Tauri 侧没有——API 设计需兼顾。

**移动端 Tauri 2 vs RN/Flutter，一句话结论**：
**若团队核心资产是 Rust 业务逻辑（解析器/同步引擎）且要求三端共用一套前端，Tauri 2 移动端是合理选择；但若追求极致原生交互体验、成熟的生态与招聘面，RN/Flutter 仍明显更稳妥——Tauri 移动端在 2026 年已可用但生态与踩坑资料远不及前两者。**

### 6. SQLite 方案与建议架构

- **桌面/移动**：Rust 侧用 **`rusqlite`**（同步、轻、直接）或 **`sqlx`**（异步、编译期校验）。建议 `rusqlite` + 连接池，业务逻辑写在 Rust，用 `invoke` 暴露给前端。`tauri-plugin-sql` 仅在需要前端直接写 SQL 时才用。
- **Web 端**：有两条路
  - **(A) 浏览器内 SQLite**：官方 sqlite-wasm + **OPFS**。关键在于选对 VFS——各 VFS 的能力差异很大（数据源：[sqlite.org 持久化文档](https://sqlite.org/wasm/doc/trunk/persistence.md) 与 [PowerSync 2026-05 综述](https://powersync.com/blog/sqlite-persistence-on-the-web)）：

    | VFS | 需 COOP/COEP 头？ | 并发 | Chrome | Safari | Firefox |
    |---|---|---|---|---|---|
    | sqlite-wasm `opfs` | **是**（需 `SharedArrayBuffer`） | 多连接并发 | 102+ | **17+** | 111+ |
    | **sqlite-wasm `opfs-sahpool`** | **否** ✅ | **仅单连接**（独占锁） | 108+ | **16.4+** | 111+ |
    | wa-sqlite `OPFSCoopSyncVFS` | 否 | 多连接并发 | 108+ | 16.4+ | 111+ |

    **要点**：`opfs-sahpool`（SQLite 3.43.0 起）**不需要 COOP/COEP 头、不需要 SharedArrayBuffer**，且 Safari 支持下探到 16.4——这是浏览器内 SQLite 目前最省事的选择。**代价是同一时刻只能有一个连接**（多标签页会冲突）。
    ⚠️ 其余代价：OPFS 存储**可能被浏览器在存储压力下回收**（非持久化保证，需 `navigator.storage.persist()` 且不保证成功）、调试复杂度陡增、所有 OPFS VFS **都必须跑在 Web Worker 里**（不能在主页面）。
  - **(B) Web 端做瘦客户端**，不落本地 SQLite，直接调服务端 API。

  🎯 **建议选 (B)**。理由：本产品的事实源是**用户机器上的文件**，而 Web 端本来就摸不到那个文件——它天然是「远端投影的查看器」。在浏览器里再维护一份 SQLite 只会引入第三个真源，与 §5 的「Markdown 作内部数据库」反模式同类。让 Web 端保持无状态（至多用 IndexedDB 做离线只读缓存），复杂度收益比最高。

### 7. 本地优先同步：CRDT vs Git

**候选**（GitHub 实时数据）：Yjs 22.7k star；Automerge 6.5k；**Loro 6.1k，Rust 原生，活跃（loro-crdt@1.15.1，2026-08-29）**。

**性能与取舍**（社区实测，[HN 讨论](https://news.ycombinator.com/item?id=41012895)，作者为自研 CRDT 库的 josephg）：
- **Automerge 保存完整历史**（删除项永久保留），带来 time-travel 能力，但**明显更慢更耗内存**——加载 100 页文档约「10 秒 / 200MB 内存」；同一测试中 Yrs 9.5ms、Yjs 74ms、**Automerge 1100ms**。
- **Yjs 不存完整历史**（但支持任意多 checkpoint），Rust 移植 **Yrs 现已非常快**（早期比 JS 慢的说法已过时）。
- **Loro** 使用 **Fugue**（减少并发编辑的交错异常）与 **Peritext-like** 富文本算法，支持完整历史与 time travel，且是 **Rust 原生**。

> 对本产品：`TODO.md` 通常是 KB 级小文件，**性能不是瓶颈**；决定性因素是「**能否把外部整体重写的文本转成 diff 导入**」——这正是 Loro `update()`/`update_by_line()` 的能力。

**「md 为事实源」带来的特殊难题**：外部编辑器（vim/VSCode/`git pull`）会**整体重写文件**。CRDT 需要的是「操作序列」，但你只拿到「新旧两个完整文本」。若简单地「清空再插入」，CRDT 历史会爆炸且并发合并语义全失。

**Loro 恰好提供了对症的原语**（源码核实，`crates/loro/src/lib.rs`）：
```rust
pub fn update(&self, text: &str, options: UpdateOptions) -> Result<(), UpdateTimeoutError>
pub fn update_by_line(&self, ...)  // 行级计算，更高效但精度略低
```
即**「给我新的完整文本，我自己算出最小 diff 并转成 CRDT 操作」**——这正是「外部整体重写文件」场景所需。`update_by_line` 对行结构化的 `TODO.md` 尤其合适。Loro 用 Rust 编写，可直接嵌入 Tauri 后端，并有 wasm binding 供 Web 复用。

**Git 即同步的评估**
- ✅ 零额外基础设施；开发者本就把 `TODO.md` 放在 git 里；历史/回滚/blame 全免费（`walm/todomd` 明确把归档交给 `git log -p`）。
- ❌ 但**不能作为唯一同步机制**：需要用户手动 commit/pull；冲突以文本冲突标记形式暴露（`<<<<<<<`）对非 git 场景的移动端用户是灾难；单个 `TODO.md` 在多人/多设备并发下会产生大量伪冲突（`amterp/kan` 正因此选择「一卡一文件」）。

#### 🎯 推荐：分层混合，**不要让 CRDT 成为事实源**

```
文件（TODO.md）= 唯一事实源
      ↓ 解析/投影（可随时重建）
SQLite 索引 = 查询与 UI 加速（可删可重建）
      ↓ 仅用于跨设备
CRDT (Loro) = 在线同步层，处理多设备并发
      ↓ 冲突兜底
3-way merge（以最近公共祖先为基）
```

具体策略：
- **单设备/本地**：完全不动 CRDT，纯文件读写 + SQLite 索引。绝大多数使用场景到此为止，简单可靠。
- **多设备在线同步**：用 Loro 作传输与合并层；本地文件被外部改动时，用 `update()`/`update_by_line()` 把新文本喂进去转成操作。
- **git 作为「用户可见的历史与备份」**，而非同步机制——顺应开发者习惯，但不承担正确性责任。
- **兜底**：无法自动合并时，做 3-way merge 并在 UI 上呈现冲突，**永不静默丢弃用户数据**。

### 8. 已有 Tauri 生产力应用（可行性佐证）

⚠️ **首先纠正一个前提：Tauri 官方 showcase 页面已不存在。** `https://tauri.app/showcase/` 返回 **404**；[awesome-tauri](https://github.com/tauri-apps/awesome-tauri) 也**已移除 Applications 章节**（README 仅剩 Guides / Articles / Templates / Plugins / Integrations）。**选型时无法引用官方背书清单。**

**逐一核实结果**（方法：`gh api` 读语言/描述 + 检查仓库内是否存在 `src-tauri/`）：

| 应用 | 是否 Tauri | 实际栈 | 证据 |
|---|---|---|---|
| **GitButler** | ✅ **是** | **Tauri + Rust + Svelte** | 仓库描述原文「powered by Tauri/Rust/Svelte」；21,588 star，活跃 |
| **Spacedrive** | ✅ 是（**仅桌面**） | 桌面 Tauri；**移动端 React Native/Expo** | `apps/tauri` 与 `apps/mobile`（含 `expo-router`）并存 |
| **Pake** | ✅ 是 | Tauri | 根目录含 `src-tauri/`；61,154 star |
| **Yaak**（API 客户端） | ✅ 是 | Tauri + Rust | 含 `crates-tauri/`；19,134 star |
| **Hoppscotch Desktop** | ✅ 是 | Tauri | `packages/hoppscotch-desktop/src-tauri/` |
| **Jan**（本地 LLM 客户端） | ✅ 是（**含移动端**） | Tauri v2 + iOS/Android | PR [janhq/jan#6657](https://github.com/janhq/jan/pull/6657) 已合并 |
| Obsidian | ❌ 否 | Electron | — |
| Logseq | ❌ 否 | **Clojure + Electron** | `package.json` 含 electron 依赖 |
| AppFlowy | ❌ 否 | **Flutter/Dart** | 主语言 Dart |
| Zed | ❌ 否 | 原生 Rust（GPUI） | — |
| **Kando** | ❌ **否（纠正常见误传）** | **Electron Forge** | `package.json` 含 `@electron-forge/cli` |
| **Clash Verge Rev** | ✅ 是 | `tauri-build 2.6.3` | **141,076 star**，Tauri 生态最大体量之一 |
| **lencx/ChatGPT** | ✅ 是 | Rust | 54,468 star |
| **Kunkun**（可扩展启动器） | ✅ 是 | — | **品类最接近开发者向工具** |
| Payload / Noor / Clippy | ⚠️ 名称歧义，无法确认 | — | Payload→JS CMS；Clippy→Rust linter |

**技术栈精确化**：GitButler = Tauri **2.11.1** + Svelte；Spacedrive = Tauri **2.1** + React 19；Pake = Tauri **2.10.2**；Yaak = React；Hoppscotch = Vue 3。

**两个可直接借鉴的工程细节**：
- **GitButler 不用默认的 `src-tauri/`**，而把 Tauri 宿主放在 `crates/gitbutler-tauri/`，处于约 **60 个 crate 的 Rust workspace** 中——**证明 Tauri 能承载严肃的大型 Rust 代码库**，正是本产品「core crate 四处复用」所需的组织方式。
- **Yaak 的发布工程可直接抄**：完整签名安装包矩阵（dmg/exe/deb/rpm/AppImage × x64/arm64），每个产物带 `.sig` + `latest.json` 自动更新——桌面端发布流程的现成模板。

#### 🚨 两个最关键的信号

**① GitButler 是本项目最佳参照物**：Tauri + Rust + Svelte，21.6k star，活跃。定位（开发者向工具 + 高频交互 + 本地文件为真相）与本产品高度吻合，**证明 Tauri 桌面端足以支撑复杂、性能敏感的开发者工具**。但它**没有移动端**。

**② Spacedrive 是最值得警惕的反面证据**：38.9k star，桌面用 Tauri，**但移动端主动选择 React Native + Expo 而非 Tauri Mobile**。仓库中还出现 `apps/gpui-photo-grid`（GPUI 是 Zed 的原生 GPU 框架），暗示其在探索脱离 WebView 的路径。
> **一个资源充足、技术能力强的团队，在需要移动端时放弃了 Tauri Mobile。这对「用 Tauri 覆盖五端」的设想是直接的反面证据。**

**「有没有知名 Tauri 应用真上了移动端？」——有，但极少，且都不重量级。**
最有力的一手证据是 **Jan** 的 mobile MVP（2025-10-01 合并），但 PR 自称 "MVP"，且**为兼容移动端移除了 updater**（反证了 §3 的 updater 结论）。

**更强的证伪**：直接探测各仓库是否存在移动端生成目录——**GitButler / Yaak / Hoppscotch / Spacedrive 全部没有 `gen/apple` 或 `gen/android`（均 404）**。Spacedrive 的 Tauri schema **仅含 desktop/linux/macOS**。

补充证据：
- [tauri#14675](https://github.com/tauri-apps/tauri/issues/14675) —— **iOS 在真机上崩溃（模拟器正常）**，2025-12 至 2026-04 仍未解决，官方建议的绕过方式是**降级**。约 **45 个 open 的 `platform: iOS` issue**。
- 一位**支持 Tauri 的**实践者做了 4 个 Tauri 移动应用，**一个都没上架**（iOS 甚至没尝试）。
- 此前搜到的「上架成功」案例（Shadow Dash、flow-like）**均无法验证**——无商店链接，flow-like 找不到仓库。

> **判断**：移动端结论应从「高风险」上调为**「证据不支持」**。Tauri Mobile 已越过「能不能上架」的门槛（Jan 证明），但**尚未越过「知名生产力产品敢押注」的门槛**。截至 2026-08，未发现任何大型知名生产力应用把 Tauri Mobile 用于正式移动端产品。

> ⚠️ **措辞纪律**：awesome-tauri 的应用列表是**被删除的**（PR [#876](https://github.com/tauri-apps/awesome-tauri/pull/876)，2026-08-20，-426 行），但该 **PR body 为空——官方未给任何理由**。**不可解读为「Tauri 生态在萎缩」**（更可能是列表维护成本）；只能陈述客观事实：目前没有官方精选应用列表。删除前的完整列表仍可从父提交 [c625de2](https://github.com/tauri-apps/awesome-tauri/blob/c625de266743519d82105ff42ea46528769ec3df/README.md) 读取。

> 🚨 **情报卫生警告**：本次调研发现 Tauri 相关二手内容**污染严重**——除 §2.3 的伪造基准数据（不存在的 `2026-hybrid-test-harness` 仓库）外，另有站点谎称「AppFlowy 运行在 Tauri 上」（**实为 Flutter**）。**涉及 Tauri 的任何数字与案例都必须一手核实。**

---

## 推荐技术架构草案

### 组件图（文字版）

```
                        ┌───────────────────────────────────────┐
                        │   用户的 Git 仓库 / 本地磁盘           │
                        │   ★ TODO.md  ← 唯一事实源（Source of  │
                        │                  Truth）              │
                        └──────────────┬────────────────────────┘
                                       │ notify + debouncer-full
                                       │ （hash 比对防自触发回环）
        ┌──────────────────────────────▼──────────────────────────────┐
        │            Rust Core Crate（一份代码，四处复用）             │
        │  ┌────────────────────────────────────────────────────────┐ │
        │  │ md-engine：pulldown-cmark OffsetIter 取 byte range      │ │
        │  │           → 按字节区间外科手术式改写（保真回写）        │ │
        │  │           → 原子写 + advisory lock                      │ │
        │  ├────────────────────────────────────────────────────────┤ │
        │  │ index：SQLite (rusqlite) — 纯投影，可随时重建           │ │
        │  ├────────────────────────────────────────────────────────┤ │
        │  │ sync：Loro CRDT（仅多设备在线时启用）                   │ │
        │  │       外部整体改写 → update_by_line() 转 diff           │ │
        │  ├────────────────────────────────────────────────────────┤ │
        │  │ notify-out：APNs/FCM（主）· ntfy · Bark · Apprise       │ │
        │  └────────────────────────────────────────────────────────┘ │
        └───┬──────────────┬───────────────┬───────────────┬──────────┘
            │              │               │               │
   ┌────────▼───────┐ ┌────▼─────────┐ ┌───▼──────────┐ ┌──▼───────────────┐
   │ Tauri 桌面     │ │ Tauri 移动   │ │ MCP (rmcp)   │ │ CLI (curl 友好)  │
   │ mac/win/linux  │ │ iOS/Android  │ │ 内嵌非 sidecar│ │ 同一 crate       │
   │ 全功能         │ │ ⚠ 无本地仓库 │ │ stdio +      │ │ --json 输出      │
   │ updater ✅     │ │ updater ❌   │ │ 本地 HTTP    │ │ 稳定 exit code   │
   └────────────────┘ └──────┬───────┘ └───┬──────────┘ └──────────────────┘
                             │             │ Claude Code / Codex
            ┌────────────────▼─────────────▼────────────┐
            │  可选自托管服务端（Rust，复用同一 core）   │
            │  REST API（OpenAPI 3.1）+ Web 瘦客户端     │
            │  推送出口 · 多设备同步中继                 │
            └────────────────────────────────────────────┘
                             │
                    ┌────────▼─────────┐
                    │ Web（同一前端）  │
                    │ 瘦客户端，无本地 │
                    │ SQLite，走 API   │
                    └──────────────────┘
```

### 关键取舍（每条都是有代价的决定）

| 决策 | 选择 | 放弃了什么 | 理由 |
|---|---|---|---|
| **Markdown 回写** | 字节区间外科手术（pulldown-cmark offsets），**绝不重新序列化** | AST 的简洁与优雅 | **实测**：`cmark()`/`cmark_with_source_range()` 726→702，comrak 726→727 且注入合成注释；`walm/todomd` 独立得出「手写更好」的相同结论 |
| **MCP 实现语言** | Rust（`rmcp` crate 编译进主二进制） | 复用现有 Node/Python MCP 实现 | sidecar 移动端不可用且影响 Mac App Store 签名；内嵌可五端统一并加速冷启动。**此决策必须前置** |
| **移动端框架** | 建议先 spike，不排除 RN/Flutter | 「一套 Tauri 走五端」的统一性 | Spacedrive 等旗舰项目均放弃 Tauri Mobile；45 个 open iOS issue；真机崩溃问题官方建议降级绕过 |
| **事实源** | 文件，单向投影到 SQLite | 查询灵活性、写入性能 | 避免 `voku/agent-kanban` 记录的「Markdown 作内部数据库」双真源漂移反模式 |
| **Web 端存储** | 瘦客户端，不用 wasm SQLite | 离线可写能力 | Web 端本就摸不到用户文件；再加一份本地库＝第三个真源 |
| **同步** | 本地纯文件；多设备才启用 Loro | 「处处 CRDT」的架构统一性 | 单机场景占绝大多数，不应为少数场景付出复杂度；Loro 的 `update_by_line` 恰好解决外部整体重写 |
| **git** | 定位为历史/备份，非同步机制 | 「零基础设施同步」的美好设想 | 需手动 commit/pull；冲突标记对移动端用户不可用；单文件并发产生伪冲突（`amterp/kan` 的反证） |
| **MCP 分发** | `rmcp` 内嵌 + `app mcp` 子命令走 stdio；app 运行时另开本地 Streamable HTTP | 移动端 MCP（可接受） | 二进制不会被打进 APK/AAB；**而 Claude Code/Codex 本就在桌面**。⚠️ stdio 进程与 app 可能并发写文件，**必须共用 advisory lock** |
| **推送** | 自建 APNs/FCM 为主，ntfy/Bark 为一等可选 | 「只接 ntfy」的实现简单性 | 自托管 ntfy 的 iOS 即时推送必须经 ntfy.sh 中转触发；自己有 app 就该用自己的证书 |
| **移动端更新** | 应用商店 | 自动静默更新 | `updater` 插件 iOS/Android 明确 ❌ |
| **文件格式** | 兼容 `todomd/todo.md` + `<!-- id:xxx -->` | 自定义格式的表达力 | 复用社区共识，GitHub 可渲染，降低用户迁移成本 |
| **移动端定位** | 远端副本的消费者，非事实源持有者 | 「三端完全对等」 | iOS 沙盒里没有用户的 git 仓库——这是产品事实而非技术限制 |

### 建议的分阶段推进（不要一次押注五端）

| 阶段 | 范围 | 风险 | 依据 |
|---|---|---|---|
| **Phase 1** | **桌面三端**：Tauri 2 + 轻量前端 + `rmcp` 内嵌 MCP + pulldown-cmark 字节回写 | **低**，有充分先例 | GitButler/Yaak/Hoppscotch 生产级验证 |
| **Phase 2** | **Web 版**：复用前端 + 换存储抽象层（瘦客户端） | 中 | Tauri 不产出 Web 版，需独立实现 |
| **Phase 3** | **移动端**：**先做技术 spike 再定框架** | **高** | 见移动端「证据不支持」结论 |

### 必须尽早验证的风险点（建议做 spike）

1. **低端 Android 真机冷启动**是否 < 2s —— 官方 benchmark 只覆盖桌面 CI，网上数字已证伪，**必须实测并做成 CI 指标**。
2. **移动端的产品形态重新定义** —— 由于 `fs` 默认限制在沙盒目录，「本地 TODO.md 为事实源」在手机上无法照搬。移动端应定位为**同步客户端**（经 Git/云端取得 TODO.md），而非直接编辑用户任意路径的文件。
3. **MCP 语言决策前置** —— 若现有 MCP server 是 Node/Python，需立刻决定：移植到 Rust（`rmcp`，换来五端统一），还是接受「桌面内嵌 / 移动端不支持」的功能分级。**此决定会反向影响整体架构，不能拖到后期。**
4. **保真回写用属性测试锁死** —— 把上文四条断言（尤其「未触碰行多重集不变」）写进 CI，这是防止回归的唯一可靠手段；建议在**真实 TODO.md 语料库**上重跑（本次实测只有单份样本）。
5. **ntfy topic 泄露风险** —— 配置必须写入 `~/.config/` 而非项目目录，否则用户 `git push` 会公开推送频道。
6. **单文件 `TODO.md` 在团队 git 协作下的伪冲突率** —— 若严重，需评估 `amterp/kan` 的「一卡一文件」方案（牺牲产品简洁性）。
7. **Linux WebKitGTK** 渲染与性能（历来 Tauri 最脆弱环节）；**国产 ROM / 鸿蒙 WebView** 兼容性（官方明确不支持）。
8. **Windows 上不要请求管理员权限**（WebView2 会阻塞 20 秒）。

### 与竞品的差异化落点

- 对 **Vikunja/Planka 等自托管看板**：你不与其比多人协作，你比的是「**todo 与代码同仓、同分支、在 PR diff 里可见**」。
- 对 **`walm/todomd`/`amterp/kan`**：它们是单人 CLI/TUI 项目（star 个位数），你的增量是 **GUI + 移动端 + 推送 + 跨设备同步 + 商业化支撑**。
- 对 **「用户直接手写 TODO.md 让 AI 读」这个零成本方案**：这才是真正的对手。产品必须靠看板可视化、到期提醒、多设备、以及 MCP 的结构化能力（稳定 ID、增量变更查询）证明自身价值。
- **商业化许可参考**：Planka 与 n8n 的 **Sustainable Use / fair-code** 模式比 AGPL 更适合「允许自托管自用、禁止他人做竞品 SaaS」的诉求，值得优先研究。

---

## 附：主要一手来源

**Part 1**
- Vikunja 仓库与源码：https://github.com/go-vikunja/vikunja （`pkg/models/webhooks.go`、`pkg/models/task_reminder.go`、`pkg/notifications/`）
- Vikunja Webhooks 文档：https://vikunja.io/docs/webhooks/
- Vikunja API 文档：https://vikunja.io/docs/api-documentation
- Focalboard（停维护声明）：https://github.com/mattermost-community/focalboard
- Planka 许可证指南：https://github.com/plankanban/planka/blob/master/LICENSES/PLANKA%20License%20Guide%20EN.md
- Kanboard API：https://docs.kanboard.org/v1/api/
- Leantime 定价/许可：https://leantime.io/pricing/
- SilverBullet：https://silverbullet.md/SilverBullet ；v2 公告 https://community.silverbullet.md/t/silverbullet-v2-released/3100
- TODO.md 格式规范：https://github.com/todomd/todo.md
- walm/todomd：https://github.com/walm/todomd
- amterp/kan SPEC：https://github.com/amterp/kan/blob/main/SPEC.md
- voku/agent-kanban 架构反思：https://github.com/voku/agent-kanban/blob/HEAD/docs/architecture.md

**Part 2**
- ntfy 发布文档：https://docs.ntfy.sh/publish/
- ntfy 配置文档（iOS 中转、rate limit、ACL）：https://raw.githubusercontent.com/binwiederhier/ntfy/main/docs/config.md
- ntfy 订阅 API：https://docs.ntfy.sh/subscribe/api/
- ntfy 已知问题：https://docs.ntfy.sh/known-issues/
- ntfy 定价（live API，非 SPA 页面）：https://ntfy.sh/v1/tiers
- ntfy iOS 架构说明（维护者）：https://github.com/binwiederhier/ntfy/issues/1680
- ntfy 硬编码 topic 的 DoS 风险：https://github.com/binwiederhier/ntfy/issues/143
- ntfy topic 安全（维护者答复）：https://github.com/binwiederhier/ntfy/issues/1787
- ntfy E2E 加密（长期 OPEN）：https://github.com/binwiederhier/ntfy/issues/69
- Vikunja 无原生 ntfy 的佐证：https://github.com/go-vikunja/vikunja/issues/481 ；https://github.com/go-vikunja/vikunja/issues/8
- Home Assistant ntfy 核心集成（platinum）：https://github.com/home-assistant/core/pull/135152
- Uptime Kuma ntfy provider：https://github.com/louislam/uptime-kuma/blob/master/server/notification-providers/ntfy.js
- Healthchecks ntfy forms：https://github.com/healthchecks/healthchecks/blob/master/hc/integrations/ntfy/forms.py
- Apprise ntfy 插件：https://github.com/caronc/apprise/blob/master/apprise/plugins/ntfy.py
- Pushover 定价：https://pushover.net/pricing ；https://support.pushover.net/i8-how-much-does-pushover-cost-is-there-a-subscription
- Bark：https://github.com/Finb/Bark ；文档 https://bark.day.app/
- Gotify：https://github.com/gotify/server
- 横向对比（二手，供参考）：https://selfhosting.sh/compare/ntfy-vs-gotify/ ；https://www.bigiron.cc/guides/gotify-vs-ntfy-vs-apprise-vs-pushover

**Part 3**
- Tauri 官方插件平台矩阵：https://github.com/tauri-apps/plugins-workspace
- Tauri sidecar 文档：https://v2.tauri.app/develop/sidecar/
- Tauri 2.0.0 stable tag（2024-10-02）：https://github.com/tauri-apps/tauri/releases/tag/tauri-v2.0.0
- Tauri 官方基准数据：https://github.com/tauri-apps/benchmark_results
- Tauri 2.0 发布公告（含官方对移动端 DX 的自评）：https://v2.tauri.app/blog/tauri-20/
- sidecar 移动端不支持（维护者表态）：https://github.com/tauri-apps/tauri/issues/9774
- externalBin 导致 Mac App Store 上传失败：https://github.com/tauri-apps/tauri/issues/13952
- iOS 文件监听性能问题（Kqueue 根因）：https://github.com/tauri-apps/plugins-workspace/issues/1823
- iOS 真机崩溃：https://github.com/tauri-apps/tauri/issues/14675
- 华为 WebView / HarmonyOS 不支持：https://github.com/tauri-apps/tauri/issues/7287
- WebView2 启动开销（微软官方）：https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/performance
- WebView2 管理员权限 20 秒阻塞：https://github.com/tauri-apps/tauri/issues/13727
- Google Play 发布无法自动化（官方承认）：https://v2.tauri.app/distribute/google-play/
- MCP Rust SDK（crate `rmcp`）：https://github.com/modelcontextprotocol/rust-sdk
- Jan 移动端 MVP（移除 updater）：https://github.com/janhq/jan/pull/6657
- awesome-tauri 移除应用列表 PR：https://github.com/tauri-apps/awesome-tauri/pull/876
- pulldown-cmark：https://github.com/pulldown-cmark/pulldown-cmark
- pulldown-cmark-to-cmark（#94 空白不保留，OPEN）：https://github.com/Byron/pulldown-cmark-to-cmark/issues/94
- comrak round-trip 缺陷 issue：https://github.com/kivikakk/comrak/issues/685
- markdown-rs（2025-04 后停更）：https://github.com/wooorm/markdown-rs
- panache-parser（rowan CST，真无损但方言/成熟度不匹配）：https://github.com/jolars/panache
- Loro（`update` / `update_by_line` 源码）：https://github.com/loro-dev/loro/blob/main/crates/loro/src/lib.rs
- notify crate：https://github.com/notify-rs/notify
- SQLite WASM 持久化与 VFS 选择（官方）：https://sqlite.org/wasm/doc/trunk/persistence.md
- Web 端 SQLite 持久化现状综述（2026-05）：https://powersync.com/blog/sqlite-persistence-on-the-web
- CRDT 性能实测讨论（Yjs/Yrs/Automerge）：https://news.ycombinator.com/item?id=41012895

---

### 配套附录（本次调研的深度支撑材料，同目录）

- `appendix-lossless-markdown-rust.md` —— Rust markdown 保真回写的**完整实测报告**（含测试样本构造、逐库往返字节数、`Tag::Item` 区间重叠 bug 的复现与修复）
- `appendix-tauri-viability.md` —— Tauri 可行性的**逐条一手核实记录**（含各应用仓库探测证据、issue 清单）

---

## 未能核实 / 存疑事项（诚实清单）

1. **ntfy 内存占用无官方数字** —— 官方只说受 `nofile` 限制；社区常引用的「约 50MB」未能从官方来源证实。
2. **ntfy `upstream-base-url` 默认值表述存在文档与代码的歧义** —— 配置表列默认 `https://ntfy.sh`，但代码是 `if != ""` 才转发。**若不希望任何数据到达 ntfy.sh，请显式置空并实测验证。**
3. **Gotify 横向对比部分来自二手来源**，未做源码级核验。
4. **markdown 实测只有一份样本**（726 字节恶意用例），非语料库。**建议在真实 TODO.md 语料上重跑。**
5. **tree-sitter-markdown 的 Rust binding 未实际构建测试**，其「适合增量重解析」属推断。
6. **comrak 的 inline sourcepos 精度未系统测试。**
7. **移动端冷启动无任何可信公开数据** —— 必须自行实测。
8. **Payload / Noor / Clippy 三个名称无法定位**，疑为名称混淆；**Cursor / Warp 未逐条验证**（闭源，属业界共识）。
9. **未发现「因使用 Tauri 而被 App Store 拒审」的证据** —— 检索到的均为构建/签名的技术性失败，**不应夸大此风险**。
10. **awesome-tauri 清空应用列表的原因官方未说明** —— 不可解读为生态萎缩。

---

*本报告所有 GitHub 星数、push 时间、release 版本均为 2026-08-30 经 `gh api` 实时查询所得。ntfy 定价取自 live API `ntfy.sh/v1/tiers`。markdown 往返数据为实际构建 Rust 工程实测所得。标注「未确证」处请勿直接对外引用。*
