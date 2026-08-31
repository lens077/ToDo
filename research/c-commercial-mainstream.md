# 主流商业 todo / 任务管理产品调研

> 调研日期:2026-08-30。所有定价均以该日官方定价页或官方帮助中心显示为准;AI 类产品调价频繁,引用前请复核。
> 阅读对象:正在做「开发者向 AI 时代 todo 工具」的委托方(本地 `TODO.md` 为事实源 + 看板双向同步、Tauri 三端、SQLite、启动 < 2 秒、MCP/skills 接入、GitHub/GitLab 导入、curl CRUD API、ntfy 通知、离线、默认中文、要商业化)。
> 标注约定:官方一手来源直接给链接;社区口碑一律标注「口碑,非官方」;无法核实的写「未能核实」,不做推测。

---

## 0. 结论摘要(给赶时间的人)

1. **MCP 已从差异化卖点退化为合格线**。到 2026 年 8 月,Linear、Todoist、**TickTick/滴答清单**、Notion、Asana、Trello、monday.com、ClickUp、Sunsama、Amie 都已交付**官方一方** MCP server,且几乎都是 OAuth + 远程 HTTP 形态。委托方若把「有 MCP」当核心卖点,已经晚了;真正的差异化只能落在**本地事实源**与**agent 可直接改文件**上。
2. **「AI 全自动维护任务」这条路已有一个失败先例**:Height 2.0 以「autonomous project management」为全部叙事,2024 年 10 月全量发布,2025 年 3 月即宣布关停,2025-09-24 正式下线,域名今日已无法建立 TLS 连接(本次实测)。这不证明方向错,但证明**仅靠 AI 自动化不足以支撑一家公司**。
3. **「快」来自架构,不是运行时**。**Linear 和 Notion 都是 Electron**(Linear 官方 changelog 自述),一个被认为快、一个公认慢 —— 决定性变量是**权威数据在本地还是在云端**。这既证明委托方的 Tauri + SQLite 站在正确一侧,也意味着**不能靠「用了 Tauri」就自动变快**。另需注意:HN 上大量工程师并不认同 Linear「快」,并指出同步引擎的真实代价是**静默数据丢失**。
4. **最直接的竞品不是 Todoist,而是 Backlog.md**(6,573★,MIT 免费):Markdown 原生、local-first、终端看板 + Web 看板、面向 AI agent。**且它主动选择「不做 issue 同步」,把 Git 本身当同步层** —— 这是对委托方「双向同步」设想最强的反向信号。委托方必须正面回答「为什么用户要付费买你,而不是用免费的它」。
5. **收费模式建议**:个人开发者 + 开发者用户群 + 本地优先架构 = **一次性买断 + 有期限的更新窗口(fair perpetual)** 最稳,而非订阅。理由、前提条件与例外见第 13 节。

---

## 1. 横向速查表

### 1.1 官方 MCP server 现状(截至 2026-08)

| 产品 | 官方 MCP | 端点 | 来源 |
|---|---|---|---|
| **Linear** | ✅ | `https://mcp.linear.app/mcp`(另有 `/mcp/readonly`) | [linear.app/docs/mcp](https://linear.app/docs/mcp)(本次实测抓取) |
| **Todoist** | ✅ | `https://ai.todoist.net/mcp` | [Doist/todoist-mcp](https://github.com/Doist/todoist-mcp)、[developer.todoist.com](https://developer.todoist.com/) |
| **Notion** | ✅ | 托管远程服务器 | [developers.notion.com/docs/mcp](https://developers.notion.com/docs/mcp)、[makenotion/notion-mcp-server](https://github.com/makenotion/notion-mcp-server) |
| **Asana** | ✅ v2 | `https://mcp.asana.com/v2/mcp` | [developers.asana.com](https://developers.asana.com/docs/using-asanas-mcp-server);旧 `/sse` 端点 2026-05-11 关停 |
| **Trello** | ✅(独立于 Rovo) | `https://mcp.trello.com/v1` | [support.atlassian.com/trello](https://support.atlassian.com/trello/docs/connect-trello-to-ai-assistants-with-trello-mcp/) |
| **monday.com** | ✅ | `https://mcp.monday.com/mcp`,或 `npx @mondaydotcomorg/monday-api-mcp` | [mondaycom/mcp](https://github.com/mondaycom/mcp)(MIT) |
| **ClickUp** | ✅(公测) | `https://mcp.clickup.com/mcp` | [developer.clickup.com](https://developer.clickup.com/docs/connect-an-ai-assistant-to-clickups-mcp-server) |
| **Sunsama** | ✅ | `https://api.sunsama.com/mcp` | [help.sunsama.com](https://help.sunsama.com/docs/integrations/mcp/) |
| **Amie** | ✅ | `https://mcp.amie.so` | [amie.so/mcp](https://amie.so/mcp) |
| *Atlassian Rovo(参照)* | ✅ 但**不含 Trello** | `https://mcp.atlassian.com/v1/mcp/authv2` | [支持工具清单](https://support.atlassian.com/atlassian-rovo-mcp-server/docs/supported-tools/)仅 Jira/Confluence/JSM/Bitbucket/Compass |
| **TickTick / 滴答清单** | ✅(国内外双端点) | `https://mcp.ticktick.com` / `https://mcp.dida365.com` | [TickTick 帮助页](https://help.ticktick.com/articles/7438129581631995904)、[滴答清单帮助页](https://help.dida365.com/articles/7438132116019216384) |
| **Things 3** | ❌ | — | 无云端 API,社区 MCP 靠本地 AppleScript/URL scheme |
| **Microsoft To Do** | ❌ | — | 微软官方 MCP 仅覆盖 **Entra 目录、只读、预览版**([learn.microsoft.com](https://learn.microsoft.com/en-us/graph/mcp-server/overview)),官方明示 M365 API 归 Agent 365,**不含 To Do/Planner** |
| **Motion** | ❌ | — | 仅 REST API + Zapier,MCP 为社区实现 |
| **Apple Reminders** | ❌ | — | 仅本地 EventKit,无云端 API |
| **Google Tasks** | ❌ | — | 有官方 REST API,无官方 MCP |
| **Height** | — | — | 产品已于 2025-09-24 关停 |

> 对委托方的含义:**MCP 不再是卖点,而是入场券**。但注意一个结构性空隙 —— 上述所有 MCP 都是「云端 SaaS 暴露 API 给 agent」,没有一家是「agent 直接读写你磁盘上的文件」。这正是 `TODO.md` 事实源路线的差异化所在。

### 1.2 定价模型对比(2026-08-30 核查)

| 产品 | 免费层 | 个人订阅 | 团队(按席位) | 买断 | AI 计费 |
|---|---|---|---|---|---|
| Todoist | 5 个项目 / 3 过滤器 / 7 天历史 | Pro $7/月 或 $60/年 | Business $10,年付 $8 | ❌ | 含在订阅内(Ramble 免费层限 10 次/月) |
| TickTick | 9 清单 / 每清单 99 任务 / 5 习惯 | 国际 $35.99/年;**国内 ¥139/年** | 无分级团队版 | ❌ | 含 |
| **Things 3** | ❌(Mac 15 天试用) | ❌ | ❌ | ✅ **各平台分开买断**,合计 $64.97 | 无 AI |
| Microsoft To Do | 全免费 | — | 随 M365 | ❌ | 走 Copilot 许可 |
| **Linear** | 250 issues / 2 teams | — | Basic $10、Business $16(年付) | ❌ | Agent 不计费席位 |
| monday.com | 最多 2 席 | — | Basic/Standard/Pro/Enterprise,**3 席起** | ❌ | AI credits |
| Notion | 有 | Plus $10 | Business $20 | ❌ | 已并入 Business;Agents 按 $10/1000 credits |
| ClickUp | 有(60MB) | Unlimited $7 | Business $12 | ❌ | **独立加购** $9 或 $28 |
| Asana | **上限 2 人** | — | Starter $10.99、Advanced $24.99 | ❌ | credits 配额 |
| Trello | ≤10 人 / ≤10 板 | — | Standard $5、Premium $10 | ❌ | 含(Standard 起) |
| Motion | ❌(7 天试用需绑卡) | Pro AI $19(年付) | Business AI $29 | ❌ | 含 credits,超额另计 |
| Sunsama | ❌(14 天试用) | $17/月(年付)、$22 月付 | — | ❌ | 含 |
| Amie | ❌(7 天试用) | Pro $20 | Business $40 | ❌ | 含 |
| Apple Reminders / Google Tasks | 全免费 | — | — | — | — |

**唯一的买断制幸存者是 Things 3** —— 这个事实本身就是第 9 节商业化建议的核心论据。

---

## 2. Linear —— 开发者项目管理标杆(重点)

Linear 是委托方最该逐条拆解的对象:它同时做到了「开发者认可」「快」「MCP 与 agent 原生」「能收钱」。

### 2.1 Linear Sync Engine(LSE):它是怎么做到「快」的

Linear 官方未完整公开实现,以下要点来自社区对生产代码的逆向工程报告 [wzhudev/reverse-linear-sync-engine](https://github.com/wzhudev/reverse-linear-sync-engine/blob/main/SUMMARY.md)(**二手来源,但极为详尽,且与 Linear 公开演讲口径一致**),以及作者的 [DEV 长文](https://dev.to/wzhudev/i-reversed-linears-sync-engine-to-see-how-it-works-3cj)。

**核心结构**

- **内存对象池 + MobX 可观察模型**:`Issue`、`Team`、`Comment` 等实体称为 model,以 UUID 存入 Object Pool;属性用装饰器注册元数据到 `ModelRegistry`,变更自动驱动视图更新。**读操作永远不过网络** —— 这是「零延迟」的根源。
- **IndexedDB 作本地持久层**:每个工作区一个数据库。`ModelRegistry` 维护 `__schemaHash`(全部模型元数据的哈希),用于判断本地库是否需要迁移 —— 这是一个非常实用的工程细节,**委托方用 SQLite 也需要等价机制**。
- **事务(Transaction)与乐观更新**:属性赋值即刻改内存;`save()` 生成 `UpdateTransaction`,进请求队列**同时写入 IndexedDB 的 `__transactions` 表**;队列批量发服务端,成功后清除缓存。断线时事务留存,重连后重发。事务同时支撑 undo/redo,失败可在客户端回滚。
- **`lastSyncId` 全局单调递增**:服务端每成功执行一个事务,全局 `lastSyncId` +1,**充当整个数据库的版本号**。客户端据此发现丢包。
- **重要的架构判断**:报告明确指出,LSE **更接近 OT 而非 CRDT** —— 因为它依赖中心服务端为所有事务定序(全序),而 CRDT 只需偏序。冲突解决策略是**last-writer-wins**。
- **Delta packets over WebSocket**:服务端广播增量包(含多个 sync action,各带 sync id)给所有客户端**包括发起方**;增量包可能与原事务不同,因为服务端会产生副作用(如生成历史记录)。
- **SyncGroup 做权限与数据切片**:`subscribedSyncGroups` 包含用户 ID、所属团队、预定义角色的 UUID,决定你能收到哪些增量包。
- **Lazy hydration 与五种加载策略**:`instant`(启动即载,默认)、`lazy`、`partial`、`explicitlyRequested`、`local`。配合 partial indexes,避免把整个工作区灌进内存。

**给委托方的可迁移结论**

1. **本地事实源 + 乐观写 + 后台推送,是「快」的通用配方**,与具体存储无关。委托方用 SQLite 取代 IndexedDB,性质相同且查询能力更强。
2. **必须有 schema 版本哈希与迁移路径**。本地缓存一旦与新版结构不符,要能安全重建。
3. **必须有持久化的待发送队列(outbox)**。Linear 把事务写进 IndexedDB 再发,这正是离线可用的关键;委托方的 `TODO.md` + SQLite 需要等价设计。
4. **单调递增序号比时间戳可靠**。用于发现丢失的更新。
5. **懒加载分级是规模化的前提**,但委托方场景(个人 / 单仓库 todo)数据量小,**可以大幅简化,不必照搬**。
6. **代价要认清**:内存占用高、超大工作区首次 bootstrap 慢、整体复杂度极高。

### 2.2 冷静看待「Linear 很快」这个口碑

这是本次调研中最值得委托方警惕的一节。HN 讨论串 [How's Linear so fast?](https://news.ycombinator.com/item?id=48437609) 里,大量工程师并不买账(**口碑,非官方**):

- 「I've actually found Linear to be quite slow? There was a week where it would also spin at 100% CPU if I left a tab open」
- 「It appears to use a lot of memory too, at least in Firefox. I can't have more than a few Linear tabs open at a time.」
- 「'Fast' is not a word I'd use for it... Linear pages often hang or take many many seconds for first load.」
- 最尖锐的一条批评直指同步引擎的本质代价:「The app is fast because it is not correct. The user has no way to know if their view is consistent with any other user's view. The user has no way to know if the app silently discarded one of their inputs because of a conflict.」
- 还有具体的数据丢失场景:「A common problem I have with Linear is that repeated writes sometimes overwrite themselves.」

同串中亦有反面观点认为许多应用**根本不需要同步引擎**:「you can easily get this speed down to like 100ms which is near enough to instant to not matter. You definitely don't need to go to client side rendering for most things to feel fast.」

> **对委托方的启示**:LWW + 乐观更新的组合会带来**静默覆盖**风险。委托方的场景是单用户 + 本地文件,冲突面本就远小于多人协作,但 `TODO.md` 与看板双向同步、以及 agent 并发改写同一文件,恰恰会重新引入这个问题。**建议:写入前做基于内容哈希的乐观锁,冲突时显式提示而非静默覆盖** —— 这可以直接成为对 Linear 的差异化宣传点。

### 2.3 官方 MCP server(本次实测确认)

抓取 [linear.app/docs/mcp](https://linear.app/docs/mcp) 原文确认:

- 主端点 `https://mcp.linear.app/mcp`,**Streamable HTTP 为主传输**;交互式安装用 **OAuth 2.1 + 动态客户端注册**,也可用 bearer token 或 API key。
- 只读端点 `https://mcp.linear.app/mcp/readonly`,或在标准端点上只申请 read scope —— 「the underlying token can't reach write APIs」。这个**读写分离设计值得委托方直接抄**:给 agent 只读权限是很多用户的第一诉求。
- 官方文档给出 Claude Code 一行接入:`claude mcp add --transport http linear-server https://mcp.linear.app/mcp`,并为 Claude、Cursor、VS Code、Windsurf、Zed 提供专页。
- 支持 Okta 企业托管授权。

### 2.4 Linear for Agents

来自官方开发者文档 [linear.app/developers/agents](https://linear.app/developers/agents)(标注为 Developer Preview):

- **Agent 表现得像工作区里的另一个用户**:可被 @mention、**通过 assign 委派 issue**、创建和回复评论、参与项目与文档。
- 通过 OAuth 应用创建,需启用 webhook 中的 **Agent session events**。
- 两条商业上极重要的事实:**「It does not cost anything to develop agents in Linear」**,以及 **「agents installed in your workspace do not count as billable users」**。
- 官方提供基于 TypeScript SDK + Cloudflare 的示例 agent。

> Linear 把 agent 排除在计费席位外,是一个刻意的生态策略:降低 Cursor/Devin/Claude Code 等接入摩擦,让 Linear 成为 agent 时代的任务中枢。**委托方若要做「把任务派给 agent」,应参考这个「agent 不占席位」的定价姿态。**

### 2.5 定价(2026-08-30 实测抓取 [linear.app/pricing](https://linear.app/pricing))

| | Free | Basic | Business | Enterprise |
|---|---|---|---|---|
| 价格 | $0 | **$10**/人/月(年付) | **$16**/人/月(年付) | 询价,仅年付 |
| Issues | **250** | 无限 | 无限 | 无限 |
| Teams | 2 | 5 | 无限 | 无限 |
| 文件上传 | 10MB | 无限 | 无限 | 无限 |
| **MCP access** | ✅ | ✅ | ✅ | ✅ |
| **Agent platform / Linear Agent** | ✅ | ✅ | ✅ | ✅ |
| Coding sessions / Loops / Code Intelligence | — | — | ✅(需 AI credits) | ✅ |

注:免费层即可用 MCP 与 Agent 平台,**AI 深度功能(Loops、Code Intelligence、Coding sessions)锁在 Business 档且消耗 AI credits**。页面标注「Trusted by more than 40,000 companies」。

### 2.6 GitHub 集成:如何解决「任务与真实工作脱节」

据官方文档 [linear.app/docs/github](https://linear.app/docs/github),集成有两大功能:**关联 PR** 与 **同步 GitHub issues**。通过把 PR / commit 关联到 issue 自动推进工作流状态;可将 Linear team 与指定 repo 双向同步 issue 并生成双向同步的评论线程。历史 issue 需用 GitHub Issues Importer 导入。需要 Linear 与 Linear Code 两个应用以获得完整能力。

> 这正是委托方「GitHub/GitLab 导入」与「同步事实」按钮的成熟参照:**状态自动跟随代码活动流转**,而不是让用户手动勾选。

### 2.7 口碑总结

- **优点**:速度(尽管有争议)、键盘驱动、克制而有主见的设计、cycle 模型契合工程节奏、GitHub 集成扎实。第三方对比中常见「Linear is the fastest project management tool I've used」「issue creation takes about two seconds」(**口碑,非官方**,[Cotera 对比文](https://cotera.co/articles/linear-vs-clickup-comparison))。
- **缺点**:无原生文档系统(需搭配 Notion);对非工程团队(市场、运营)不适配;有用户认为它「已变成它想取代的复杂之物」;UX 被部分用户批评为图标含义不清、状态变化缺乏指示。

---

## 3. Height —— 「AI 自动维护任务」的重要失败先例

**这是本次调研对委托方战略价值最高的一节。**

### 3.1 它曾经主张什么

Height 2.0 自称 **「the first ever autonomous project collaboration tool」**。官方博客 [Height 2.0 is now launched](https://height.app/blog/introducing-height-2-0) 的原话:

> 「Over the past year, we didn't just add AI onto the product; we designed the entire experience around it from the ground up. Height is now a reasoning engine and collaboration tool fused together.」

它要解决的问题,与委托方的「同步事实」按钮**高度同构**:

> 「product builders are expending more energy and brainpower on project upkeep than they are on building products.」

官方举的自家例子:22 人公司、近期 2000+ 活跃任务、「almost a 100 statuses, attributes, task descriptions to update every week per team member」。具体自主能力包括:bug 自动分类归档(只写标题,tagging/优先级/指派全自动)、**backlog 自动修剪**、**spec 自动更新**、基于团队真实对话写进度更新、依赖关系自动映射、重复与过期任务自动消除。

### 3.2 它怎么了:已关停

- 2025 年 3 月 20 日前后,创始人兼 CEO Michaël Villar 宣布关停,给出六个月过渡期。原文:「After three and a half years of being publicly available, we've made the difficult decision to discontinue operations six months from now, with the final day of service being **September 24th, 2025**.」(经 [AlternativeTo 报道](https://alternativeto.net/news/2025/3/height-project-management-tool-to-shut-down-by-september-2025/)与镜像文本佐证)
- **本次实测(2026-08-30)**:`height.app` 与 `www.height.app` 的 DNS 仍解析到 `35.190.76.117`,但 HTTPS 握手直接失败(`curl: (35) LibreSSL SSL_connect: SSL_ERROR_SYSCALL`),两个独立抓取器均无法建立连接 —— **服务确已下线**。
- 背景数据(**二手来源**):成立于 2018 年,累计融资约 1830 万美元,含 2021 年 Redpoint 领投的 1400 万美元 A 轮;[Creativerly 的报道](https://www.creativerly.com/height-app-is-shutting-down/)指出关停公告「incredibly generic」,未说明真实原因。
- 时间线上最刺眼的一点:**Height 2.0 于 2024 年 10 月全量发布,五个月后即宣布关停**。

### 3.3 对委托方的四条教训

1. **「AI 自动维护任务」作为唯一叙事,已被验证不足以支撑公司**。它是很好的功能,但难以独立成为产品的全部理由。
2. **自动化与信任的矛盾**:自动改动任务状态,一旦出错,用户失去的是对系统的信任。委托方的「同步事实」按钮设计上**优于** Height —— 它是**用户显式触发**的,而非后台自主运行。**建议保留这个「人在回路」的定位,并明确宣传它。**(对照 Microsoft 的做法:Planner Agent 的 AI 生成内容现在**默认进入 draft 模式**待用户批准,见 [官方公告](https://techcommunity.microsoft.com/blog/plannerblog/planner-agent-in-microsoft-365-copilot-is-now-generally-available/4527629) —— 业界正在集体回撤「全自动」。)
3. **风投节奏与产品节奏错配是致命的**。委托方作为个人开发者、不烧钱,反而没有「必须高速增长否则关停」的压力 —— 这是结构性优势,应转化为「我们不会消失」的信任叙事。
4. **数据可携带性是卖点**。Height 关停后用户数据被删除,无迁移路径。委托方的 `TODO.md` 纯文本事实源,天然免疫这个问题 ——「**你的数据永远是你仓库里的纯文本文件**」应当是首页第一句话。

---

## 4. Todoist(Doist)

### 4.1 API:注意,REST API v2 已不是主线

**这是全文最容易过时出错的技术点。** Doist 已迁移到**统一 API v1**,官方原文:「The Todoist API v1 is a new API that **unifies the Sync API v9 and the REST API v2**」([developer.todoist.com/api/v1](https://developer.todoist.com/api/v1/))。

- 基址 `https://api.todoist.com/api/v1/`,有完整 [OpenAPI 规范](https://developer.todoist.com/openapi.json)。
- 迁移的真实成本:**ID 格式变更**,旧 v2 调用可能「silently fail or loop」(二手:[dashy issue](https://github.com/lissy93/dashy/issues/2025))。
- 旧 API 关停确切日期**未能核实**(仅第三方论坛提到「2026 年初」)。
- 认证:Personal token(Bearer)或 OAuth2。
- **速率限制**(摘自官方 OpenAPI 规范):增量 sync **1000 次 / 15 分钟 / 用户**;全量 sync **100 次 / 15 分钟**;单请求批量最多 **100** 条命令;POST body 1 MiB;标准请求超时 15 秒。

> 对委托方的直接参考:这套限额说明**即使是成熟商业产品,同步也是「批量 + 限频」而非逐条实时**。委托方的 curl CRUD API 设计可以更慷慨(本地无网络成本),这本身就是卖点。

### 4.2 官方开发者工具链(四件套最全)

Doist 是本次调研中**开发者生态最完整**的一家:

- **官方托管 MCP**:`https://ai.todoist.net/mcp`,OAuth 一分钟接入(本次实测抓取 [Doist/todoist-mcp](https://github.com/Doist/todoist-mcp) README 确认)。亦可 `npx @doist/todoist-mcp` 本地运行,或作为 npm 库直接引入自建 AI 界面。
- **官方 CLI**:`td` 命令(`npm i -g @doist/todoist-cli`),并支持 `td skill install claude-code|codex|copilot|cursor|gemini` —— **直接给编码 agent 装 skill**,这与委托方的「MCP/skills 供 Claude Code、Codex 接入」思路完全一致,值得研究其 skill 组织方式([Doist/todoist-cli](https://github.com/Doist/todoist-cli))。
- **官方 SDK**:[Python](https://github.com/Doist/todoist-api-python)、[TypeScript](https://github.com/Doist/todoist-sdk-typescript)。
- **官方 Webhooks**:四家个人向产品中唯一齐全的事件推送。

### 4.3 AI 能力:Todoist Assist

官方产品名 **Todoist Assist**([产品页](https://www.todoist.com/todoist-assist)):

| 功能 | 说明 | 层级 |
|---|---|---|
| **Filter Assist** | 自然语言生成过滤器 | 全部用户,**仅英语/西班牙语** |
| **Task Assist** | 任务拆解为子任务、改写更可执行 | Pro / Business |
| **Email Assist** | 邮件转任务 | Pro / Business |
| **Ramble** | 语音口述转结构化任务,**支持 40 种语言** | 全部计划,免费版 10 次/月 |

经典的 **Smart Quick Add**(自然语言解析日期)是非 LLM 老功能,仍是同类标杆。隐私上官方声明输入「not shared with third parties **or used for training AI models**」。

> 注意:**Filter Assist 不支持中文**。这对委托方「默认中文」的定位是一个可打的差异点。

### 4.4 定价(2026-08-30 核对官方[计费 FAQ](https://www.todoist.com/help/articles/todoist-plans-pricing-and-billing-faq-Vq2z0HWL6))

免费版已更名 **Beginner**;Pro 于 **2025-12-10 涨价约 40%**(月付 $5→$7,年付 $48→$60,见[官方涨价公告](https://www.todoist.com/help/articles/todoist-pro-pricing-update-in-2025-bxBvHZuJZ))。

| | Beginner | Pro | Business |
|---|---|---|---|
| 价格 | $0 | **$7/月 或 $60/年** | **$10/人/月,年付 $8** |
| 个人项目 | **5** | 300 | 每人 300 |
| 过滤器视图 | **3** | 150 | 每人 150 |
| 活动历史 | **1 周** | 完整 | 完整 |
| 文件上传 | 5 MB | 25–100 MB | 100 MB |

另有**全账户硬限制**(所有档位一致,[官方说明](https://www.todoist.com/help/articles/usage-limits-in-todoist-e5rcSY)):每项目 **300** 个活跃任务、总计 **500** 个项目、每账户 500 标签。官方解释原因是「Having too many active tasks in a single project can **slow syncing down**」并坦承「we're working to make our sync engine faster, so we can increase these limits」。

> **这是委托方的机会**:Todoist 因云同步架构不得不设 300 任务/项目的上限。本地 SQLite 方案**没有这个限制**,可直接作为宣传点。

### 4.5 离线与性能口碑

- **离线**:客户端可离线增删改,回网同步([官方文档](https://www.todoist.com/help/articles/use-todoist-while-offline-4rbaZw))。但官方明确警告:「**Don't close Todoist or shut down your computer before your offline changes have finished syncing** ... may lead to data loss」—— 说明它是「乐观写入 + 待同步队列」,而非强一致本地数据库。
- **性能(一手证据,很有价值)**:官方 CLI 仓库 issue [todoist-cli#57 "td API slow"](https://github.com/Doist/todoist-cli/issues/57)(2026-02)实测 `td today` 耗时 **17–23 秒**(Node 启动占 11–17s,`/api/v1/projects` 取 50 个项目耗 2.7–3.2s),`td today --all` 曾达 **1 分 50 秒**。Doist 员工回复承认「there's a lot to improve in our own code here in the CLI」。
- 关于 2023–2024 改版的集中差评,**未能核实**到有代表性的可引用讨论,不做断言。

**优点**:开发者生态最全、自然语言输入标杆、全平台、AI 克制可选、SOC2 Type II。
**缺点**:API 迁移有静默失败风险、免费版收紧、2025 年底涨价 40%、离线有丢数据警告、Filter Assist 无中文、无原生番茄钟/习惯。

---

## 5. TickTick / 滴答清单(Appest)—— 中国团队出海标杆

### 5.1 双品牌双后端:账号不互通

官方明确说明(一手,[滴答清单数据迁移页](https://dida365.com/public/guide/migration/data-center)):「**滴答清单账户和 TickTick 账户是不通用的**」。官方给出的国内版差异理由包括农历等本地化、Google 地图/语音替换,以及「在中国大陆访问 TickTick 网络较慢,如果使用中国区网络**同步速度将提升 5 倍以上**」。国内版额外支持 QQ/微博/微信登录。

> **对委托方极有参考价值**:这是「中国团队做全球化产品」的成熟范式 —— **双后端、双品牌、数据分区、支付本地化**,而非一套系统硬撑两个市场。委托方若要商业化且默认中文,应及早决定是走这条路,还是只做单一市场。

### 5.2 功能矩阵:一个 app 装下别人四个 app

番茄钟、习惯追踪、日历视图与双向同步、艾森豪威尔四象限、自然语言输入、看板、智能清单、时间块、地理围栏提醒 —— **全部内置**。这是它相对 Todoist 的核心竞争力。

### 5.3 API 与 MCP:一个罕见的反转

**官方 Open API (V1) 能力严重不足**。OAuth2,**scope 只有 `tasks:read` 和 `tasks:write`**。V1 **无法访问**:Inbox(收件箱不作为 project 暴露)、标签、**习惯与专注记录**、文件夹、**已完成任务**、过滤器/智能清单(代码级佐证:[go-ticktick](https://github.com/slavkluev/go-ticktick))。

正因如此,社区库普遍被迫混用**未公开的 V2 网页 API**(cookie 模拟登录,甚至用 Selenium 抓 cookie)—— 不稳定且有账号安全风险。

**但官方 MCP 的能力反而超过官方 REST API**:

- 端点:`https://mcp.ticktick.com`(国际)/ `https://mcp.dida365.com`(国内)
- **仅支持 Streamable HTTP,不支持 SSE**(官方明说)
- 覆盖范围(官方原文):「basic task, list, **habit, focus records and countdown** operations」—— **能碰 Open API 碰不到的习惯与专注记录**
- 官方文档罗列 Claude / Claude Code / ChatGPT / Cursor / Codex / TRAE / **豆包 / 千问办公** 等大量客户端配置 —— 对国产 AI 客户端的适配明显比 Todoist 更用心

> **不要用「API 弱」推断「MCP 弱」**。Appest 显然把 MCP 当作新的一等公民接口在做,而 Open API 已多年停滞。这也印证了 MCP 正在成为比 REST 更重要的对外接口。

**硬伤:无 webhook**。事件驱动自动化只能轮询。

### 5.4 定价(2026-08-30)

- **国内 滴答清单:¥139/年**(官方升级页实抓,「一年高级会员只需￥139(每月仅￥11.6)」,[ticktick.com/upgrade?language=zh_cn](https://ticktick.com/upgrade?language=zh_cn))
- **国际 TickTick**:官方英文升级页显示 **US$49.99/年**([ticktick.com/upgrade](https://ticktick.com/upgrade) 实抓);多个二手源称实际常年以 **$35.99/年** 的「限时优惠」售卖([ellieplanner](https://ellieplanner.com/productivity-copilot/ticktick-pricing) 等)。**两个数字并存,引用时需同时注明。**
- 无终身买断,无分级团队套餐。

> **注意这个价差**:¥139 ≈ $19,而国际版标价 $49.99。**中国市场的支付意愿定价明显更低**,这是委托方做中文付费产品必须面对的现实。

**优点**:功能密度最高、性价比最好、国内体验专门优化、官方 MCP 覆盖面超出其 REST API。
**缺点**:官方 Open API 严重不足、无 webhook、双账号体系摩擦、无官方 SDK/CLI。性能口碑普遍正面但**未能核实**到可引用的一手实测。

---

## 6. Things 3(Cultured Code)—— 极致体验 + 买断制的唯一幸存者

### 6.1 定价(经 Apple iTunes Lookup API 实测,美区)

**一次性买断,无订阅**,每平台单独购买:

| 平台 | 美区价格 | 备注 |
|---|---|---|
| **Mac** | **$34.99** | 有 15 天试用 |
| **iPhone + Watch** | **$9.99** | Watch 版免费包含 |
| **iPad** | **$19.99** | |
| **Vision** | 未能核实 | |

合计约 **$64.97**(不含 Vision)。

> ⚠️ **纠错**:不少二手对比文章写「Things 3 $49.99 一次性」,与 App Store 实价不符。以 App Store 为准。

官方立场(一手,[Getting Things](https://culturedcode.com/things/support/articles/2803552/)):「Things 3 is a **one-time purchase** for each platform; there are **no subscriptions**」;**Things Cloud 同步免费** —— 「at no extra charge」;同类设备不重复收费;**无捆绑包、无折扣、无学生优惠、无升级折扣**;仅通过 App Store 销售。

**Things 4 或订阅制:截至 2026-08-30 无任何官方公告(未能核实)。** 版本号仍在 3.23.x 且 2026-08-28 刚更新,说明 3 代仍在积极维护 —— **一个买断制产品能持续维护近十年,这是第 9 节商业化建议的关键实证。**

### 6.2 交互设计精髓(值得委托方逐条借鉴)

- **Magic Plus 按钮**:可拖拽的浮动「+」,**拖到列表不同位置决定新条目的插入点与类型**(待办/标题/项目),把「创建」与「定位」合成一个手势。
- **四大时间视图**:Today / Upcoming / Anytime / Someday —— 用「**什么时候做**」而非「优先级数字」来组织,是对 GTD 的克制翻译。
- **Areas(持续责任范围)vs Projects(有终点的事)** 的二分。
- **Headings**:项目内轻量分组,不占用任务语义。
- **键盘优先** + 全局 Quick Entry 热键(带 Autofill 可抓取当前 app 的链接)。

设计哲学:**不做功能堆叠,做减法和手感**。没有番茄钟、习惯、看板、AI。

### 6.3 自动化:本地能力其实很强

- **URL Scheme**([官方文档](https://culturedcode.com/things/support/articles/2803573/)):`things:///add?title=...`;最强的是 **JSON 命令**,可一次性创建整个项目连同 notes、headings、to-dos 的嵌套结构。
- **AppleScript**([官方文档](https://culturedcode.com/things/support/articles/2803572/)):**可读可写** —— 能读内置列表(`to dos of list "Inbox"`)、按属性创建待办、定位到 area/project。这是它与纯 URL scheme(基本只写)的关键区别。
- **Shortcuts** 支持。

### 6.4 限制:对 AI 集成是硬约束

- ❌ **没有公开 Web/Cloud API**,Things Cloud 是私有同步协议。
- ❌ **Apple 独占**:无 Windows / Android / Web。
- ❌ 无协作。
- ❌ **不可能有官方 MCP**:数据只在本地设备与私有云,**任何 Things MCP 都必须是跑在用户 Mac 上的本地 shim**。社区方案(如 [drjforrest/mcp-things3](https://github.com/drjforrest/mcp-things3),★73)star 数普遍很低,成熟度与维护风险要打折看待。

### 6.5 性能

原生 Swift/Objective-C(非 Electron)+ 本地数据库,交互无网络往返。**坦白说明**:未能核实到可引用的一手 benchmark,「极快」属架构推断与业界共识。

> **对委托方的双重启示**:(1) Things 证明**买断制在本地优先工具上可长期存活**;(2) 它的致命短板恰是委托方的机会 —— **Apple 独占 + 无 API + 无 AI**。委托方的 Tauri 三端 + MCP + 中文,正好补上这三个洞。

---

## 7. Microsoft To Do —— 免费 + 生态绑定

- **完全免费,无付费层**。商业模式是生态绑定:官方定位「To Do is **the single destination for personal tasks in Microsoft 365**」([Graph 文档](https://learn.microsoft.com/en-us/graph/todo-concept-overview))。
- **API**:标准 Microsoft Graph,`GET/POST/PATCH/DELETE https://graph.microsoft.com/v1.0/me/todo/lists/{id}/tasks`,文档质量高,Entra OAuth2 认证,支持变更通知订阅。
- **一个实务大坑**:To Do 与 Planner 在 UI 上正在合并,**但 API 层仍是两套割裂的东西**。Planner API 拿不到个人私有任务;且[官方文档](https://learn.microsoft.com/en-us/graph/api/resources/planner-overview?view=graph-rest-1.0)明说「**Premium plans and tasks aren't available** on the Planner API」。
- **产品整合**:新 Planner 官方定义为「brings together the simplicity of **Microsoft To Do**, the collaboration of Planner, the power of Microsoft Project, and the intelligence of Microsoft Copilot」([微软博客](https://techcommunity.microsoft.com/blog/plannerblog/transitioning-to-microsoft-planner-and-retiring-microsoft-project-for-the-web/4410149))。To Do 作为独立品牌正在被稀释,但**官方未宣布关停独立 app**(存续时间表未能核实)。
- **AI**:To Do 自身无 AI,全部走 Copilot / Planner Agent。值得注意的是 [Planner Agent 已 GA](https://techcommunity.microsoft.com/blog/plannerblog/planner-agent-in-microsoft-365-copilot-is-now-generally-available/4527629),且官方明确「All AI-generated plans, tasks, and updates are now created in **draft mode by default**」——**业界正在从「全自动」回撤到「人工确认」**。
- **MCP**:❌。微软官方 MCP 仅覆盖 Entra 目录、只读、预览版,M365 API 被官方划归 Agent 365。
- **口碑**:接替 Wunderlist 的历史包袱很重(2017–2020 年大量同步与功能缺失吐槽);**近两年是否改善未能核实**,引用旧证据须注明时间背景。

---

## 8. monday.com —— 平台化 Work OS

### 8.1 数据模型:与普通 todo 的本质区别

`workspace → board → group → item → subitem`,每个 item 的属性由**列(column)**定义,列类型丰富(status、person、date、formula、mirror、connect boards 等)。**它本质是一个「带视图的可编程数据库」,而非任务列表** —— 这解释了它为什么能同时卖给市场、HR、销售、工程团队。

### 8.2 API 与 MCP

- **GraphQL API**:`https://api.monday.com/v2`,带日期化版本(如 `2024-10`),有**复杂度预算(complexity budget)**式的限流 —— 查询按字段复杂度计费,而非单纯按请求数。
- **官方 MCP**:✅ [mondaycom/mcp](https://github.com/mondaycom/mcp),MIT 许可,npm 包 `@mondaydotcomorg/monday-api-mcp`,亦有托管端点。可管理 boards、items、columns、groups、docs。
- 有成熟的 apps framework 与 marketplace。

### 8.3 定价与商业表现

按席位计价,**最低 3 席起**(这是常被吐槽的一点:单人用户无法购买最低档)。分 Free / Basic / Standard / Pro / Enterprise。

**为什么赚钱(一手财报)**:据 [monday.com 投资者关系](https://ir.monday.com/financials-and-filings/quarterly-results/default.aspx),FY2025 全年营收 **$1.232B**(同比约 +26%),Q1 2026 营收 **$351M**;ARR > $50K 的客户占总 ARR 的 **41%**,> $100K 的占 **28%**。

> **对委托方最重要的一课**:monday.com 的钱**不来自个人用户,而来自不断向上扩张的企业客户**(大客户占比逐年提升)。这条路径需要销售团队、合规认证、企业级功能 —— **个人开发者不可能复制**。不要把 monday.com 当作商业模式参照,它只应作为「数据模型设计」的参照。

### 8.4 口碑

功能强大但**重**。第三方对比中常被指出加载慢、配置复杂、按席位成本随团队增长快速累积。(**口碑,非官方**)

---

## 9. Notion / ClickUp / Asana / Trello 简述

### 9.1 Notion

- **AI 计费口径已变**:Notion AI **不再是独立加购**,已并入 Business($20/人/月);新的计量项是 **Custom Agents 与 Workers**,「Free to try, then **$10 per 1,000 monthly Notion credits**」([官方定价页](https://www.notion.com/pricing))。
- **官方 MCP**:✅ 托管远程服务器 + 开源仓库 [makenotion/notion-mcp-server](https://github.com/makenotion/notion-mcp-server),[官方文档](https://developers.notion.com/docs/mcp)明确「a remote MCP server hosted by Notion」,管理员可在 Settings → Connections 管控。
- **离线**:有真离线模式但**需逐页显式开启**「Available offline」;数据库默认**只下载前 50 行**,embeds/forms/buttons 离线不可用([官方指南](https://www.notion.com/help/guides/working-offline-in-notion-everything-you-need-to-know))。
- **性能口碑(二手)**:[3perf 工程剖析](https://3perf.com/blog/notion/)指出桌面端是「a React web app embedded into a native shell」,**冷启动约 6.2 秒**,中端手机可达 12.6 秒。Reddit 上的典型抱怨:「anything takes 2-3 seconds, I cannot scroll down a database without it hanging」、「It has become a hog... Notion is just a web app」。

### 9.2 ClickUp

- **AI 是唯一走独立加购路线的**:Brain AI **$9/人/月** 或 Everything AI **$28/人/月**,叠加 Business($12)后**最高可达 $40/人/月**([Brain 定价页](https://clickup.com/brain/pricing))。
- **官方 MCP**:✅ 公测,`https://mcp.clickup.com/mcp`,全套餐可用,**仅 OAuth**,且出于安全「haven't added any deletion tools」([开发者文档](https://developer.clickup.com/docs/connect-an-ai-assistant-to-clickups-mcp-server))。
- **性能口碑(二手,最严重)**:Reddit 对照实测「a simple Kanban board still takes about **8 seconds** to load fully. For comparison, **Trello loads in about 1 second** on the same setup」,官方支持承认「the initial load time is around 6[s]」([原帖](https://www.reddit.com/r/clickup/comments/1n2kqar/why_is_clickup_so_slow_compared_to_other_tools/))。更值得警惕的是伴随的数据可靠性投诉:「text disappears for no apparent reason」。ClickUp 官方反馈站甚至有专门的 [SLOWNESS 议题](https://feedback.clickup.com/public-api/p/slowness)。

### 9.3 Asana

- **AI 走 credit 配额制**(非独立席位加购):Starter 5 万 / Advanced 7.5 万 / Enterprise 20 万 credits 每月,**按 billing account 而非按人**([官方定价页](https://asana.com/pricing))。
- **官方 MCP**:✅ **v2 正式版**,`https://mcp.asana.com/v2/mcp`,Streamable HTTP + OAuth;旧 beta 端点 `/sse` 将于 **2026-05-11 关停**([开发者文档](https://developers.asana.com/docs/using-asanas-mcp-server))。
- **开发者体验最规范**:提供 `llms.txt`,且任意文档 URL 追加 `.md` 即得 Markdown 版 —— **这个 AI 友好的文档设计值得委托方直接抄。**
- 定价:Starter $10.99、Advanced $24.99(年付)。免费版**上限仅 2 人**,同类中最苛刻。

### 9.4 Trello

- **需更正一个常见误解**:Trello 有**自己独立的官方 MCP**(`https://mcp.trello.com/v1`,OAuth 2.0,**全套餐含免费版可用**,[官方文档](https://support.atlassian.com/trello/docs/connect-trello-to-ai-assistants-with-trello-mcp/));而 [Atlassian Rovo MCP 的工具清单](https://support.atlassian.com/atlassian-rovo-mcp-server/docs/supported-tools/)里**完全没有 Trello**,只有 Jira/Confluence/JSM/Bitbucket/Compass。两者是并行关系,不是包含关系。Trello MCP 不提供破坏性删除(只能归档)。
- **Butler 自动化**按 command runs 配额计费:Free 250 次/月,Standard 1000 次,Premium 无限。
- 定价:Standard $5、Premium $10(年付)。免费版 ≤10 人 / ≤10 板。
- **性能是它的优势**:在多个对照中作为「1 秒加载」的基准出现。官方确认「Trello is proudly a **cloud-only** product」,无离线编辑保证。

---

## 10. AI 排程新秀:Motion / Sunsama / Amie

| | 卖点 | 定价(2026-08-30) | 官方 MCP |
|---|---|---|---|
| **Motion** | AI 把任务按截止日/时长/优先级**自动排进日历**,变动时自动重排 | Pro AI **$19**/席/月(年付,含 7500 credits);Business AI **$29**。**无免费层**,7 天试用需绑卡。超额 credits $0.25/100 | ❌ 仅 REST API + Zapier |
| **Sunsama** | 反其道行之:强调**每日规划仪式**,人工逐条拖入并承诺时长;核心是聚合器(拉 Todoist/Linear/GitHub/Jira/Apple Reminders) | 单一 Pro **$17/月**(年付)或 $22 月付 | ✅ `https://api.sunsama.com/mcp` |
| **Amie** | **已转型**:主叙事从「日历+todo」变为 **AI 会议记录**(官网主标题已是「AI Note Taker」),日历与 todo 仍在但非主线 | Pro **$20**/人/月、Business $40(年付) | ✅ `https://mcp.amie.so`,OAuth 2.1 + PKCE,支持多 Google 账号 |

**Motion 的口碑警示(二手)**:贵、激进、流失率高。典型用户原话:「Trial ended and I got hit with a $34 charge... **$34/month for a task manager is a hard sell when Todoist is $5**」;AI 重排「moves tasks in ways that feel arbitrary」,因为系统不掌握用户脑中的紧急度与精力状态;留存用户普遍需 3–4 周才建立信任([SaaS Flags 汇总](https://saasflags.com/products/motion-app))。

> **对委托方的启示**:Motion 的问题与 Height 同源 —— **AI 替用户做决定,一旦不符预期就摧毁信任**。委托方的「同步事实」按钮是**用户触发、结果可见、可回滚**的,这个定位更稳,应当坚持。

---

## 11. 系统自带(一句话)

- **Apple Reminders**:完全免费、深度嵌入 iOS/macOS(Siri、Shortcuts、锁屏),第三方本地 app 可经 **EventKit** 读写;**是本组唯一真正的本地优先产品**(飞行模式功能完整),但无云端 REST API,云端 agent 无法直连。
- **Google Tasks / Android**:完全免费、内嵌 Gmail/Calendar,有[公开 REST API](https://developers.google.com/workspace/tasks/limits)(每日 5 万次配额);功能极简 —— 无优先级、无标签、无附件。

---

## 12. 横向专题

### 12.1 MCP 生态时间线(一手来源)

- **2024-11-25**:Anthropic 开源发布 MCP([官方公告](https://www.anthropic.com/news/model-context-protocol))。
- **2025-03**:OpenAI 正式采纳,覆盖 ChatGPT 桌面端、Agents SDK、Responses API;同期发布首个 MCP 授权规范(OAuth 2.1)。
- **2025-09-08**:官方 **MCP Registry** 预览发布(`registry.modelcontextprotocol.io`,[官方博客](https://blog.modelcontextprotocol.io/posts/2025-09-08-mcp-registry-preview/)),定位为「single source of truth for MCP servers」,采用元注册表(metaregistry)设计,指向 npm/PyPI/Docker Hub 等实际制品。**注意其自述仍为 preview,不保证数据持久性。**
- 此后 MCP 转入 **Linux Foundation** 治理。

- **2025-12-09**:MCP **捐赠给 Linux Foundation 旗下 Agentic AI Foundation**,与 Block 的 goose、OpenAI 的 AGENTS.md 并列创始项目([Anthropic 公告](https://www.anthropic.com/news/donating-the-model-context-protocol-and-establishing-of-the-agentic-ai-foundation))。**MCP 已不再是 Anthropic 专有资产,而是中立开放标准** —— 这对「要不要押注 MCP」是决定性的一条。

**形态演进**:早期以本地 stdio server 为主,现已明显转向**远程 HTTP(Streamable HTTP)+ OAuth 2.1** —— 本文调研的 Linear、Todoist、TickTick、Asana、Trello、ClickUp、Sunsama、Amie 全部采用远程托管形态。规范 `2025-06-18` 起,MCP server 被定义为 **OAuth 2.1 resource server**,**MUST** 实现 RFC9728 受保护资源元数据([授权规范](https://modelcontextprotocol.io/specification/2025-06-18/basic/authorization))。

**但要警惕「有 MCP」与「有能用的 MCP」的差距**(二手,但基于对注册表的大规模实测):一份对 9,326 个远程 server 的探测报告称 **17.2% 宣称有端点却根本不响应**,55.8% 完全无需鉴权([MCP Queen](https://mcpqueen.com/reports/state-of-mcp-2026-07));另一份统计称 top 0.1% 的包吃掉 84% 安装量([MCP HQ](https://mcphq.ai/state-of-mcp))。**上文那些官方端点的价值,恰恰在于它们是少数经得起验证的。**

> **对委托方的判断**:MCP 已是合格线。但**所有现有 MCP 都是「云端 SaaS 把 API 暴露给 agent」**,没有一家做「agent 直接读写本地文件事实源」。委托方的机会不在「也有 MCP」,而在 **MCP 背后是本地文件而非云端数据库** —— agent 改完,`git diff` 就能看见。

### 12.2 「Agent 可被指派任务」正在成为新范式

- **Linear for Agents**:agent 作为工作区用户,可被 @mention 与 **assign issue**;**agent 不计费席位,开发免费**([官方文档](https://linear.app/developers/agents))。
- **GitHub Copilot coding agent**:可被指派 issue 并自动开 PR。
- **Microsoft Planner Agent**:已 GA,但 AI 生成内容**默认 draft 待批准**。

**共同趋势:从「AI 自动做」回撤到「AI 提议 + 人确认」。** Height 的失败与 Motion 的口碑问题都指向同一结论。委托方的「同步事实」按钮天然符合这个方向。

### 12.3 本地优先(local-first)技术地景

理论原点是 Ink & Switch 的 [Local-first software](https://www.inkandswitch.com/local-first/)(七项理想:快、多设备、离线可用、协作、长期保存、隐私默认、用户最终控制)。

可选同步方案(委托方是 **Tauri + SQLite**,故重点看能配 SQLite 的):

| 方案 | 性质 | 与委托方的匹配度 |
|---|---|---|
| **Linear Sync Engine** | 自研,不开源 | 仅供借鉴思路 |
| **PowerSync** | Postgres ↔ 客户端 SQLite,自定义冲突解决 | 移动端最成熟;但需 Postgres 后端 |
| **ElectricSQL** | Postgres → SQLite/PGlite,shape 订阅,LWW | 开源 Apache 2.0;偏 React |
| **Zero (Rocicorp)** | 客户端响应式缓存,IndexedDB | Web-only,不适合 Tauri 移动端 |
| **Automerge / Yjs** | CRDT | 适合协作富文本,**结构化任务不必要**;注意墓碑(tombstone)无限增长需 GC |
| **cr-sqlite** | SQLite + CRDT 扩展 | 概念上最贴近,但生态小 |
| **PowerSync** | 后端 DB ⇄ **应用内 SQLite** 双向部分同步 | **第一梯队**;唯一把「应用内 SQLite 离线优先」当第一性目标且覆盖移动端的成熟方案。商业分发前务必先读其 licensing 条款(source-available) |
| **cr-sqlite** | SQLite 扩展,把表升级为 CRDT(`crsql_as_crr`) | **第一梯队**;MIT,schema 几乎不用改,Rust 侧可静态链接。**实测未停止维护**(2026-08-10 仍有提交,含 Android 16KB page size 适配) |
| **Turso / libSQL** | Rust 重写 SQLite + 变更同步 | 方向最正确且与 Rust/Tauri 同源,**但官方声明未到 1.0**(README:「we have not yet reached 1.0」)。务实策略:**现在用标准 SQLite,保持数据访问层可替换** |
| **baresync** | **专为 Tauri 设计的 SQLite 同步栈**(Rust 引擎 + Tauri 插件 + outbox) | 值得先看,[GitHub](https://github.com/sakti-dev/baresync) |

**三个必须纠正的流行印象(经实测)**:

1. **ElectricSQL 的变化远超「转型」**:创始人在 [Electric is joining Databricks](https://electric.ax/blog/2026/08/11/electric-joining-databricks)(2026-08-11,本次实测抓取确认)中亲述早期「We were building Postgres to SQLite sync」后转向 Postgres→Postgres;**公司已并入 Databricks,域名迁至 `electric.ax`,且 Electric Cloud 正在关停**。→ **不要作为新项目的 SQLite 同步方案。**
2. **Replicache 已进入维护模式**,官网明示「Existing users should migrate to Zero」。
3. **Convex 官方承认无完整离线同步**:「Convex **doesn't currently provide a full offline sync mechanism**」。→ 与本地优先诉求直接冲突。

**明确不推荐**:Convex(无离线)、Electric(已弃 SQLite 路线 + 云关停)、Zero(强制 Postgres,拿不到本地 SQLite 文件)。

**一个重要的架构判断**([FlowVerify 综述](https://www.flowverify.co/blog/crdt-sync-boundary-local-first-2026),二手但论证清晰):选型的关键不是 CRDT vs OT,而是**「同步的边界是行、文档,还是事件日志」**。行级同步(委托方场景的自然选择)的代价会在**schema 迁移**时显现 —— 老客户端可能长期离线持有旧结构。

> **给委托方的务实建议**:单用户 + 本地文件事实源,**先不要引入任何通用同步引擎**。`TODO.md` ↔ SQLite 的双向同步本质是**单机内的两个表示之间的一致性问题**,用「文件 mtime + 内容哈希 + outbox」即可解决。跨设备同步交给 Git 或用户自己的网盘,是最省力且最符合开发者直觉的选择。

### 12.4 「Markdown 为事实源」的既有工具 —— 委托方最直接的竞品

**这是本次调研必须提醒委托方的一点:该赛道已有人在做,且免费开源。**

**[Backlog.md](https://github.com/MrLesk/Backlog.md)**(MIT,npm `backlog.md`)自我描述为「**Markdown‑native Task Manager & Kanban visualizer for any Git repository**」,标语是「AI agents write the code. You review the tasks」。其特性与委托方的设想高度重叠:

- 📝 每个任务是仓库里的一个 `.md` 文件
- 📊 `backlog board` 终端看板;`backlog browser` 提供**本地 Web 看板,支持拖拽编辑**
- 🔒 **Local-first** —— 「no server, no account, no telemetry; tasks are plain files in your repo」
- 🤖 「works with Claude Code, Gemini CLI, Codex, Kiro & any other **MCP or CLI** compatible AI assistant」
- ✅ 验收标准(acceptance criteria)与 Definition of Done
- 跨平台(macOS/Linux/Windows),**MIT 开源免费**
- 已在 Devoxx Belgium 2025 与 AI Engineer Code Summit 2025 演讲

其产品论述也很有说服力:「AI agents can now produce more plausible code in an hour than you can carefully read in a day. **The bottleneck is no longer writing code. It's your attention.**」并设计了三个审查检查点(审 spec → 审 plan → 审 code)。

其他相关工具:Obsidian Tasks 插件、Logseq、todo.txt、TaskPaper、org-mode、taskwarrior;以及代码扫描类:[todo-to-issue-action](https://github.com/marketplace/actions/todo-to-issue)(把提交中的 TODO 注释自动建成 GitHub issue,并可回写 issue URL)、tickgit、imdone。

> **委托方必须回答的问题**:Backlog.md 免费、开源、已有社区与会议曝光。你的付费理由是什么?
> 本次调研能看到的差异化空间:**(1) 图形界面与移动端**(Backlog.md 是 CLI + 本地 Web,无原生桌面/移动 app);**(2) 中文优先**;**(3) 通知(ntfy)与跨设备**;**(4) 双向同步的可靠性保证**(冲突显式提示而非静默覆盖);**(5) 非工程师也能用的看板**。建议把这几点作为产品定义的锚点,而不是重复「Markdown + 看板 + AI」这个已被占据的叙事。

### 12.5 性能口碑:谁快谁慢

| 产品 | 评价 | 证据 |
|---|---|---|
| **Things 3** | 快(原生) | 架构推断,无可引用 benchmark |
| **Trello** | 快 | 对照实测约 1 秒加载 |
| **Linear** | **有争议** | 官方叙事与部分用户体感一致,但 HN 上大量工程师反驳(见 2.2) |
| **Todoist** | 中等,CLI 明显慢 | 官方 issue 实测 `td today` 17–23 秒 |
| **Notion** | 慢 | 3perf:桌面端冷启动 **6.2 秒** |
| **ClickUp** | **最慢** | 用户实测看板 8 秒,官方承认约 6 秒 |
| **monday.com** | 重 | 口碑,无实测 |

**必须纠正一个流行误解:「Linear 快是因为原生」是错的。** [Linear 官方 changelog](https://linear.app/changelog/2019-04-25-linear-desktop-app) 自述其桌面端是「with the **Electron wrapper**」。**Linear 与 Notion 都是 Electron,一个被认为快、一个被公认慢** —— 决定性变量不是运行时,而是**权威数据在哪里**:Linear 的 UI 直读本地 IndexedDB,Notion 则一切走网络。

> **这条纠正对委托方极其重要**:它证明「快」来自**本地优先架构**而非语言/运行时选择。委托方的 Tauri + SQLite 天然站在正确的一侧,**但也意味着不能靠「用了 Tauri」就自动获得快** —— WebView 首屏渲染与本地查询路径仍需认真优化。「启动 < 2 秒」是可兑现且可宣传的,但要靠架构纪律,不能想当然。

**一个架构选择外溢的实例**:2025-09 起 Electron 应用因覆写私有 AppKit API,导致 **macOS 26 Tahoe 全系统卡顿**([Michael Tsai 汇总](https://mjtsai.com/blog/2025/09/30/electron-apps-causing-system-wide-lag-on-tahoe/))。上表除 Things 3 外几乎全是 Electron。

**硬数据(安装包体积,反映是否捆绑完整 Chromium)**:Things 3 **21.0 MB**(原生)、TickTick 94 MB、Todoist 112 MB、Notion 115 MB、ClickUp 154 MB、Obsidian 218 MB —— 原生与 ClickUp 相差约 **7.3 倍**。⚠️ 体积不等于启动时间或内存占用,它只反映架构事实。

### 12.6 「任务与真实工作脱节」的现有解法

1. **Linear / Jira 的 GitHub 集成**:PR/commit 关联 issue 后**自动推进状态**;分支名自动关联;PR 状态双向同步([Linear 官方文档](https://linear.app/docs/github))。
2. **GitHub Projects + Issues** 本身作为任务系统,Copilot coding agent 可被指派 issue。
3. **自动化规则**:monday、ClickUp、Asana 的 rules;Trello Butler(按 command runs 计费)。
4. **代码扫描类**(与委托方「同步事实」最相关):[todo-to-issue-action](https://github.com/marketplace/actions/todo-to-issue) 在 push 时扫描 diff 中新增的 TODO 注释建 issue,TODO 被删除时关闭 issue,还能把 issue URL 回写进源码注释。

**Linear 的 magic words 分三类**(比通常认知丰富,[官方文档](https://linear.app/docs/github#magic-words)),值得委托方直接借鉴其语义分层:

| 类别 | 词表 | 行为 |
|---|---|---|
| **Closing** | `close/fix/resolve/complete/implement` 及其变位 | 合并时置为终态 |
| **Non-closing** | `ref, refs, references, part of, contributes to, toward(s)` | 推进状态但**不置终态** |
| **Relation** | `relates to, related to` | 仅标记相关,**完全不改状态** |

三个值得抄的细节:**逃生舱** `skip ENG-123` / `ignore ENG-123` 可阻止自动关联;一个 PR 可关多个 issue;**多个 PR 关联同一 issue 时,只在最后一个达标时才变更状态**。

**对照组 Jira Smart Commits** 只有 `#comment` / `#time` / `#transition` 三个命令,但约束暴露了命令式设计的代价:不能跨行、提交者 email 必须精确匹配、**必填字段会让 transition 静默失败**。→ Jira 把表达力给用户,代价是静默失败;Linear 收窄成固定词表,换来「几乎不会失败」。

### 12.7 ⚠️ 一个必须正视的反向信号

**该赛道最活跃的 Markdown 任务工具 Backlog.md(6,573★)主动选择了「不做 issue 同步」**,而是把 **Git 本身当作同步层**(「remote Git operations are **optional**」)。

这对委托方的「看板双向同步」与「GitHub/GitLab 导入」是最强的反向信号,必须认真对待:

1. **不要把「同步」定义成「双向镜像」**。做得最认真的 Linear 仍带着一串刺眼限制(只同步新建的、双向同时只能配一个 repo、线程外评论不同步、还要专门做错误 banner)。**双向同步是成本极高、永远漏水的抽象。**
2. **务必抄 [todo-to-issue-action](https://github.com/alstr/todo-to-issue-action) 的「Issue URL 回写」**:创建远端对象后把 URL 写回本地文件(如 `<!-- gh:#123 -->`)—— **映射表就是文件本身**,不怕重命名、不怕重排序、`git diff` 可见、冲突可人工解决。这是整份调研中最值得直接落地的设计。
3. **单向 + 幂等**:`TODO.md → 外部` 做单向投影,只留一条窄回流(PR 合并 → 勾掉某行)。
4. **留逃生舱**:没有 opt-out 的自动化会被用户整个关掉。
5. **提前定义边界规则**:TODO 被删除等于「完成」还是「放弃」?(有工具选择「移除即关闭」,但重构挪动代码会误关 —— 很危险。)
6. **失败必须可见**:同步失败应写回 `TODO.md` 本身或显眼位置,不要静默。

> **顺着 agent 时代重新定位价值**:Linear 的 delegate≠assignee(官方原则:「**an agent cannot be held accountable**」)与 Backlog.md 的三个审查检查点指向同一结论 —— **AI 时代 `TODO.md` 的核心价值不是「记住要做什么」,而是在代码生成之前提供一个人类能一屏读完的审查界面,并在之后成为「做过什么、为什么」的永久 Git 记录。「同步事实」的真正卖点,是让这份记录不会因为 agent 干完活而失真。**

---

## 13. 商业化模式对比与建议

### 13.1 先看清三个事实

1. **除 Things 3 外,本次调研的商业产品几乎全部是订阅制**。买断制在这个品类是少数派。
2. **但 Things 3 活了下来,而且活得很好**:近十年持续维护(2026-08-28 仍在更新)、免费云同步、无折扣无促销、只在 App Store 卖。它证明**买断制在「本地优先 + 单机价值」的工具上是可持续的**。
3. **反面教材同样清晰**:Height(VC 支持、AI 叙事、5 个月内从全量发布到宣布关停)与 monday.com(靠企业大客户扩张,FY2025 营收 $1.232B)代表的两条路,**个人开发者都走不了**。

### 13.2 各模式对委托方的适配度

| 模式 | 适配度 | 理由 |
|---|---|---|
| **一次性买断 + 有限更新窗口(fair perpetual)** | ⭐⭐⭐⭐⭐ **推荐** | 与「本地优先、无服务器成本」的架构完全对齐;开发者用户群明确反感订阅;无 churn 运维负担 |
| 纯订阅 | ⭐⭐ | 除非你真的提供持续的服务端价值(跨设备同步、AI 推理),否则「为本地运行的软件按月收租」难以自洽 |
| 开源核心 + 付费增值(open-core) | ⭐⭐⭐⭐ | 面对 Backlog.md 这类免费竞品时,这可能是**必要的防守**:开源 CLI/核心引擎,收费卖 GUI、移动端、同步与通知 |
| 按席位团队版 | ⭐⭐ | 起步期不要碰;需要销售、权限、合规,个人开发者负担不起 |
| Lifetime deal(买断制促销) | ⭐ | 多篇 indie 复盘警告这是陷阱:一次性收入换来无限期支持义务 |
| 完全免费 + 捐赠 | ⭐ | 委托方明确要商业化,不适用 |

### 13.3 为什么推荐「买断 + 更新窗口」

来自 indie 开发者的实证与复盘(**二手,但论证一致且相互印证**):

- **本地运行 = 边际成本近零**,这是买断制成立的前提。一位开发者的总结很直接:「I only use lifetime pricing for local-first apps. **If it runs on their machine, there's no marginal cost per user.**」([DEV](https://dev.to/godnick/why-i-chose-lifetime-pricing-for-my-dev-tools-and-would-do-it-again-25mb))
- **省掉整套订阅运维**:「I don't need Stripe webhooks for failed payments, dunning emails, grace periods, or a 'win-back' campaign... **For a solo dev, that operational simplicity is worth more than the theoretical revenue gap.**」
- **「fair perpetual」是业界成熟折中**:永久授权 + 一年更新窗口,到期后软件**继续可用**,新版本另购。「The customer's app never stops working — only the 'free updates' entitlement lapses. You get a renewal motion **without the rented-software resentment**.」([DEV](https://dev.to/nicodemanez/one-time-vs-subscription-licensing-which-to-use-6oi))。JetBrains 的 perpetual fallback license、Sublime Text 均属此类。
- **定价不要锚太低**:一篇复盘直言其教训是「**I underpriced the lifetime anchor**... Default psychological anchors for one-time software skew low because of App Store conditioning. **Price the value, not the feel.**」

**两个可直接照抄的成熟范式(官方页面实测确认)**:

- **Sublime Text**([官方 store 页](https://www.sublimehq.com/store/text) 实测原文):「Personal licenses are a **once off purchase, and come with 3 years of updates**. After 3 years, an upgrade will be required to receive further updates. **One license key is all you need for all your computers and operating systems**」。个人 $99;**商业则走订阅**。→ 这正是「个人买断 + 商业订阅」双轨。
- **JetBrains perpetual fallback license**([官方 FAQ](https://sales.jetbrains.com/hc/en-gb/articles/207240845-What-is-a-perpetual-fallback-license-and-how-do-I-use-one)):买的是订阅,但**连续满 12 个月**后获得永久授权(覆盖订阅开始时的最新大版本);中途取消不满 12 个月即丧失资格。

> **为什么这类模式对 local-first 特别有效**:它同时消除双方恐惧 —— 开发者得到可预测收入,用户得到「**停订不变砖**」。**对本地运行、无服务器依赖的桌面工具,这个承诺几乎零成本**,却能极大降低付费阻力。这是订阅模式里唯一不与本地优先哲学冲突的变体。

**三个必须避开的坑(均有一手复盘)**:

1. **从买断转订阅会引发「背叛」叙事,而非普通涨价反弹**。Kaleidoscope 2023 年转订阅后,[MacRumors 论坛](https://forums.macrumors.com/threads/how-to-diff-kaleidoscope-is-going-subscription.2390027/)典型反应:「I love the app, but **not $150 love it**」,且用户**立刻迁移到** Beyond Compare / BBEdit / FileMerge。→ **开发者用户极度价格敏感,且技术上完全有能力自建替代方案。所以定价模式一开始就要选对,不要指望以后再改。**
2. **Lifetime Deal 是陷阱**。一份复盘:售出约 1,300 份净得约 $34,000 一次性,今天仍有约 1,150 个活跃账户产生 **$0/月**却持续消耗成本 —— 「**A lifetime deal is not revenue. It is a loan you repay in support tickets and server bills for as long as the user lives.**」更反直觉的一条:「**Active ones almost never upgrade.** People who buy lifetime deals are **deal hunters by identity**」。→ 对委托方而言「服务器成本」那条罪状因无服务器架构而不成立,**但「支持成本永久化」与「最省钱人群的逆向选择」两条完全成立**。
3. **捐赠不是商业模式**:横评显示单人中型开源库通常 $100–500/月,**超 75% 维护者拿到零资金**。

**另一个值得单独研究的原型:Obsidian**。本体 $0 且[官方 license](https://obsidian.md/license) 明确允许商业使用,收入来自 **Sync / Publish 这两件「本地优先做不到的事」** —— 同步与托管天然需要服务器、天然适合订阅,**且不把本体功能扣为人质**。这与委托方「本地优先 + 要商业化」的结构高度契合,是比 Things 3 更值得对标的收入模型。

**但必须诚实指出买断制的三个前提条件**,任何一条不满足就应重新考虑订阅:
1. 边际成本必须近零 —— **委托方若自建云同步服务,这条就破了**。建议同步走 Git / 用户自有网盘,把服务器成本留给用户。
2. 必须有可持续的自然获客渠道 —— 买断制是「top-of-funnel game」,每月都要有新客。
3. 支持负担必须可控 —— 功能范围要克制。

### 13.4 具体建议(可执行)

**定价结构**

- **核心产品:一次性买断 + 12–36 个月更新窗口**(抄 Sublime 的「once off purchase, comes with 3 years of updates」)。到期后软件永久可用,继续更新需升级付费。
- **可选的第二收入曲线(抄 Obsidian)**:若日后要做云同步/托管看板,**把它做成独立的可选订阅**,而非把本体功能扣为人质。本体买断 + 服务订阅,是本地优先产品唯一自洽的双轨。
- **双轨授权(抄 Sublime)**:个人买断,商业/团队走年度订阅。
- **参考价位**:对标 Things Mac 的 $34.99 与国内 ¥139/年 的滴答清单。建议**国内 ¥128–198 买断**、**国际 $29–39 买断**。注意 TickTick 国内外 ¥139 vs $49.99 的定价差说明:**中国市场支付意愿显著更低,必须做区域定价**。
- **免费层**:不要做功能阉割版,做**规模阉割**(如单仓库 / 单看板免费),让个人开发者能完整体验核心闭环再付费。这比 Todoist 的「5 个项目」更符合开发者心理。
- **不要做 lifetime deal 促销**。

**渠道与收款**

- **不要只依赖 App Store**(Apple 抽成 30%,小企业计划 15%;且 Things 因只在 App Store 卖而无法自主定价与促销)。Tauri 应用可自主分发。
- **国际**:用 **Merchant of Record** 规避全球税务合规负担。**Lemon Squeezy**(现属 Stripe)对独立开发者最友好 —— **内建 license key 生成、激活、验证 API**,正是桌面应用所需;费率 5% + $0.50。Paddle 更适合规模化 SaaS 且**不原生支持 license key**。
  - **关键概念区分**([Stripe 官方](https://stripe.com/resources/more/merchant-of-record-vs-payment-facilitator)):MoR **成为 seller of record**,代你在各国注册并申报 VAT/GST;而 **Stripe Tax 只帮你计算收取,登记申报的法律责任仍在你身上**。→ 多付的约 5% 买的不是支付通道,而是「**不用在几十个国家注册 VAT**」。卖数字产品 + 进入 EU/UK 市场,两条全中,**强烈建议用 MoR**。
- **国内(纠正一个流行误解)**:**不需要营业执照也能收款**。[微信支付官方文档](https://pay.weixin.qq.com/doc/v3/partner/4012713017)的主体类型含**小微商户**(仅需个人身份证)。但有额度限制([官方](https://pay.weixin.qq.com/doc/v3/partner/4012086921)):小微商户日收款 10 万元,**信用卡单日 1 千、单月 1 万**。→ **起步走小微商户,长期注册个体工商户最省心。** 个人出海结汇的合规路径**未能核实**。
- **⚠️ 不要把「美国区外链零佣金」当既定前提**:2025-04 地方法院曾禁止 Apple 对 off-app 购买征佣,但**第九巡回法院于 2025-12-11 认为该禁令过宽,予以撤销并发回重审**。截至 2026-08-30,**最终佣金率处于未定状态**。
- **分发合规成本**:macOS 必须签名+公证,需 Apple Developer **$99/年**;Windows 方面[微软官方文档](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options)已明确 **EV 证书自 2024 年起不再即时绕过 SmartScreen**,推荐 Azure Artifact Signing(约 $9.99/月)。**[tauri-plugin-updater](https://v2.tauri.app/plugin/updater/) 的更新签名强制且不可关闭 —— 私钥丢失 = 永远无法再推送更新**,务必做好密钥备份。年度合规成本约 $219(约 3 份 $99 销售即可覆盖)。

**防守 Backlog.md 这类免费开源竞品**

建议采用 **open-core**:**开源 CLI 与 `TODO.md` 解析/同步内核**(建立信任与生态,让 agent 生态愿意接入),**收费卖 GUI 桌面端 + 移动端 + 通知 + 跨设备体验**。理由:开发者会为「省时间的界面与移动端」付费,但很难为「一个能解析 Markdown 的 CLI」付费 —— 而后者恰恰是竞品已经免费提供的部分。

**叙事建议(基于本次调研的独有发现)**

1. 「**你的数据永远是你仓库里的纯文本文件**」—— 对照 Height 关停后数据被删除。
2. 「**AI 提议,你确认**」—— 对照 Height 的全自动失败与 Microsoft 回撤到 draft 模式。
3. 「**没有 300 条任务上限**」—— 对照 Todoist 因云同步不得不设的硬限制。
4. 「**买断,不订阅**」—— 对照全行业订阅化,且 Things 3 证明这条路可持续。

---

## 14. 明确「未能核实」的事项

以下条目本次调研未能取得可靠一手证据,**请勿当作已证实事实引用**:

1. Things for Vision 的美元价格(iTunes lookup 未返回独立条目)。
2. Things 4 或 Cultured Code 订阅制计划 —— 无任何官方公告。
3. Todoist 旧 API(REST v2 / Sync v9)的确切关停日期 —— 仅第三方论坛提到「2026 年初」。
4. TickTick 国际版美元定价:官方英文升级页显示 **$49.99/年**,二手来源普遍称实际售 **$35.99/年「限时优惠」**;两者并存,未能取得权威口径。
5. TickTick 免费版各项配额的官方出处(数据来自二手评测站)。
6. TickTick 的一手性能实测。
7. Things 3「极快」的 benchmark —— 属架构推断与业界共识。
8. Microsoft To Do 在 2025–2026 年的同步可靠性现状(可引用吐槽多为 2017–2020 年)。
9. 独立 Microsoft To Do app 的存续/退役时间表 —— 官方未宣布关停。
10. Todoist 2023–2024 改版的集中舆论评价 —— 未找到有代表性的可引用讨论。
11. ClickUp 与 Asana 是否具备正式的离线编辑能力。
12. Motion 月付的确切金额(官方页面抓取时仅呈现年付视图;二手来源在 $29–$34 之间不一致)。
13. monday.com 各档位的**精确席位单价**(定价页为 JS 渲染,本次未能直接抓取;仅确认「3 席起」与财报数据)。
14. **美国区 link-out 的最终佣金率** —— 第九巡回法院 2025-12-11 撤销原禁令并发回重审,截至调研日未定。
15. Linear 的官方离线读写边界、AI credit 单价、默认分支名模板。
16. Trello 是否被 Atlassian Rovo MCP 覆盖(Rovo 工具清单中未见 Trello,但两者同属 Atlassian)。
17. 中国个人开发者出海结汇的合规路径。
18. Stripe Managed Payments 的具体费率。

> **另需说明**:本次调研主动排除了三个疑似 AI 内容农场的来源(含「640ms」「TTI 5.1s→1.3s」「n=172, p<0.01」这类精确却无法溯源的数据)。采信它们会让性能一节看起来更完整,**但那是错的**。

---

## 15. 主要来源清单

**本次实测抓取(一手)**
- [linear.app/docs/mcp](https://linear.app/docs/mcp) — Linear MCP 端点、OAuth 2.1、只读端点
- [linear.app/pricing](https://linear.app/pricing) — Linear 各档价格与 250 issues 限制
- [linear.app/developers/agents](https://linear.app/developers/agents) — Agent 不计费席位、开发免费
- [linear.app/docs/github](https://linear.app/docs/github) — GitHub 集成机制
- [developer.todoist.com](https://developer.todoist.com/) — 统一 API v1、官方 MCP/CLI/SDK
- [Doist/todoist-mcp](https://github.com/Doist/todoist-mcp) — 官方 MCP 端点 `ai.todoist.net/mcp`
- [ticktick.com/upgrade](https://ticktick.com/upgrade) 与 [?language=zh_cn](https://ticktick.com/upgrade?language=zh_cn) — $49.99/年 vs ¥139/年
- [ir.monday.com](https://ir.monday.com/financials-and-filings/quarterly-results/default.aspx) — FY2025 营收 $1.232B
- [MrLesk/Backlog.md](https://github.com/MrLesk/Backlog.md) — 竞品特性与产品论述
- [wzhudev/reverse-linear-sync-engine](https://github.com/wzhudev/reverse-linear-sync-engine/blob/main/SUMMARY.md) — LSE 架构细节
- `height.app` DNS/TLS 探测 —— 确认服务已下线

**官方文档与定价页**
- Todoist:[计费 FAQ](https://www.todoist.com/help/articles/todoist-plans-pricing-and-billing-faq-Vq2z0HWL6)、[涨价公告](https://www.todoist.com/help/articles/todoist-pro-pricing-update-in-2025-bxBvHZuJZ)、[使用限制](https://www.todoist.com/help/articles/usage-limits-in-todoist-e5rcSY)、[离线](https://www.todoist.com/help/articles/use-todoist-while-offline-4rbaZw)
- TickTick MCP:[国际](https://help.ticktick.com/articles/7438129581631995904)、[国内](https://help.dida365.com/articles/7438132116019216384)、[账号不通用说明](https://dida365.com/public/guide/migration/data-center)
- Things:[定价](https://culturedcode.com/things/pricing/)、[Getting Things](https://culturedcode.com/things/support/articles/2803552/)、[URL Scheme](https://culturedcode.com/things/support/articles/2803573/)、[AppleScript](https://culturedcode.com/things/support/articles/2803572/)
- Microsoft:[To Do Graph API](https://learn.microsoft.com/en-us/graph/todo-concept-overview)、[企业 MCP](https://learn.microsoft.com/en-us/graph/mcp-server/overview)、[Planner 整合](https://techcommunity.microsoft.com/blog/plannerblog/transitioning-to-microsoft-planner-and-retiring-microsoft-project-for-the-web/4410149)、[Planner Agent GA](https://techcommunity.microsoft.com/blog/plannerblog/planner-agent-in-microsoft-365-copilot-is-now-generally-available/4527629)
- Notion:[定价](https://www.notion.com/pricing)、[MCP](https://developers.notion.com/docs/mcp)、[离线指南](https://www.notion.com/help/guides/working-offline-in-notion-everything-you-need-to-know)
- ClickUp:[定价](https://clickup.com/pricing)、[Brain 定价](https://clickup.com/brain/pricing)、[MCP](https://developer.clickup.com/docs/connect-an-ai-assistant-to-clickups-mcp-server)
- Asana:[定价](https://asana.com/pricing)、[MCP](https://developers.asana.com/docs/using-asanas-mcp-server)
- Trello:[MCP](https://support.atlassian.com/trello/docs/connect-trello-to-ai-assistants-with-trello-mcp/)、[Rovo 工具清单](https://support.atlassian.com/atlassian-rovo-mcp-server/docs/supported-tools/)
- monday.com MCP:[mondaycom/mcp](https://github.com/mondaycom/mcp)
- Motion:[定价](https://www.usemotion.com/pricing);Sunsama:[MCP](https://help.sunsama.com/docs/integrations/mcp/);Amie:[MCP](https://amie.so/mcp)
- MCP 生态:[Anthropic 发布公告](https://www.anthropic.com/news/model-context-protocol)、[Registry 预览](https://blog.modelcontextprotocol.io/posts/2025-09-08-mcp-registry-preview/)
- 本地优先:[Ink & Switch](https://www.inkandswitch.com/local-first/)、[baresync](https://github.com/sakti-dev/baresync)

**二手来源(口碑与旁证,正文均已标注)**
- [HN: How's Linear so fast?](https://news.ycombinator.com/item?id=48437609) — 对 Linear「快」的反驳与同步引擎风险
- [Todoist CLI 性能 issue](https://github.com/Doist/todoist-cli/issues/57) — 官方仓库内实测(严格说属一手)
- [3perf: Notion 性能剖析](https://3perf.com/blog/notion/)、[Reddit: ClickUp 慢](https://www.reddit.com/r/clickup/comments/1n2kqar/why_is_clickup_so_slow_compared_to_other_tools/)
- [AlternativeTo: Height 关停报道](https://alternativeto.net/news/2025/3/height-project-management-tool-to-shut-down-by-september-2025/)、[Creativerly 评论](https://www.creativerly.com/height-app-is-shutting-down/)
- [SaaS Flags: Motion 投诉汇总](https://saasflags.com/products/motion-app)
- 商业模式:[DEV: 为何选择 lifetime 定价](https://dev.to/godnick/why-i-chose-lifetime-pricing-for-my-dev-tools-and-would-do-it-again-25mb)、[DEV: 买断 vs 订阅](https://dev.to/nicodemanez/one-time-vs-subscription-licensing-which-to-use-6oi)
- [TickTick V1 API 限制的代码级佐证](https://github.com/slavkluev/go-ticktick)
- [todo-to-issue-action](https://github.com/alstr/todo-to-issue-action) — Issue URL 回写设计
- [Sublime Text 定价](https://www.sublimehq.com/store/text)(实测)、[JetBrains perpetual fallback](https://sales.jetbrains.com/hc/en-gb/articles/207240845-What-is-a-perpetual-fallback-license-and-how-do-I-use-one)、[Obsidian license](https://obsidian.md/license)
- [Kaleidoscope 转订阅的用户反应](https://forums.macrumors.com/threads/how-to-diff-kaleidoscope-is-going-subscription.2390027/)
- [Electric 并入 Databricks](https://electric.ax/blog/2026/08/11/electric-joining-databricks)(实测)、[Replicache 维护模式](https://replicache.dev/)、[Convex 无离线同步](https://www.convex.dev/sync)
- [MCP 捐赠 Linux Foundation](https://www.anthropic.com/news/donating-the-model-context-protocol-and-establishing-of-the-agentic-ai-foundation)、[MCP 授权规范](https://modelcontextprotocol.io/specification/2025-06-18/basic/authorization)
- [Linear magic words](https://linear.app/docs/github#magic-words)、[Linear 桌面端 Electron changelog](https://linear.app/changelog/2019-04-25-linear-desktop-app)、[Linear Agent 开发者文档](https://linear.app/developers/agents)
- [微信支付主体类型](https://pay.weixin.qq.com/doc/v3/partner/4012713017)、[Stripe MoR vs PayFac](https://stripe.com/resources/more/merchant-of-record-vs-payment-facilitator)、[Tauri updater](https://v2.tauri.app/plugin/updater/)
- [Electron 致 macOS Tahoe 卡顿](https://mjtsai.com/blog/2025/09/30/electron-apps-causing-system-wide-lag-on-tahoe/)
