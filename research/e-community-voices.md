# 开发者社区关于「AI 时代 todo/任务管理痛点」的真实讨论

> 调研目的：验证或证伪委托方的产品假设 ——「AI agent 大量改代码后，todo 看板没同步，任务其实已完成或已变化，看板漂移、过时、最后没人看」。
> 拟议方案：本地 `TODO.md` 为事实源 + Tauri 看板 + MCP 让 Claude Code/Codex 读写 + 「同步事实」按钮让 agent 扫码库对齐任务状态。
>
> 调研时间：2026-08-30。渠道：Hacker News（Algolia API 全文检索 + 评论树抓取）、Exa/Firecrawl 网页检索、GitHub Issues、DEV.to、Reddit、中文社区。
> 引用规范：每条给出 URL 与大致日期；区分「单条声音」与「反复出现的模式」。英文原文保留，附中文概括。

---

## 0. 结论速览（TL;DR）

| 假设组成部分 | 裁决 | 证据强度 |
| --- | --- | --- |
| 「AI agent 改完代码后任务状态失真」 | **真需求，且有量化证据** | 强（多源独立复现 + 量化数字 + 已有专门工具） |
| 「markdown/纯文本放 git 作为事实源」 | **真需求，已是事实标准之一** | 强（HN 1402 分主贴 + Backlog.md 6.5k stars） |
| 「需要一个看板 GUI」 | **弱需求，且是最容易被证伪的一环** | 弱（Vibe Kanban 已倒闭；多数人只用 CLI/编辑器） |
| 「MCP 让 agent 读写」 | **真需求，但 MCP 未必是最佳载体** | 中（社区更偏好 CLI 而非 MCP） |
| 「同步事实按钮 = agent 扫码库对齐状态」 | **最强的差异化点，且尚未被很好满足** | 中强（已有 `backlog.md-drift` 验证方向，但生态空白仍在） |

一句话：**痛点是真的，纯文本方向是对的，「看板」是最弱的一环，「同步事实」才是真正值钱的东西。**

**三条跨平台独立收敛的论断，构成本报告的证据骨架**（三位作者互不相干、分处三个平台，却指向同一产品结论）：

| 论断 | 来源 | 回答了什么 |
| --- | --- | --- |
| 「**Not a discipline problem, a distance problem**」 | Reddit r/AI_Agents（22 分） | 为什么**必然**漂移 |
| 「**Creates are reliable, completes are lossy**」 | DEV.to（27% ghost rate 实测） | 漂移为什么是**单向**的 |
| 「**Done is a claim that must be reconciled, not a message from the agent**」 | Reddit r/AI_Agents（6+ 人附和） | **唯一**的解法 |

**以及一条必须正视的市场警告**：一个与委托方定位几乎完全相同的产品（本地 markdown + 看板 + agent 可编辑 + 无订阅）在 r/ClaudeAI 拿到 135 分，而**最高赞评论（74 分）是**「Everybody and their grandma has made their own Claude's task list.」

---

## 1. 「todo 漂移 / 与代码脱节」痛点：真实讨论证据

这是委托方假设的核心。以下证据按「证明力」从强到弱排列。

### 1.1 量化证据：27% 的「幽灵任务」（最强单条证据）

**Nick Meinhold, "I Audited My AI's To-Do List. A Quarter of It Was Already Done."**
- URL: https://dev.to/nickmeinhold/i-audited-my-ais-to-do-list-a-quarter-of-it-was-already-done-1k01（原文 https://enspyr.co/blog/i-audited-my-ais-todo-list）
- 日期：2026-08-05

作者用 Claude Code hooks 把 agent 的任务自动同步成 GitHub issue，某天做了一次对账：

> "Eleven open tasks for the current project. Three of them were already done — one finished five days ago, two finished the same afternoon they were created. They'd been sitting 'open' ever since, getting faithfully restored into session after session as live work. **That's a 27% ghost rate on a single morning's sample.**"

他给出了漂移的**结构性成因**，这段论述对产品设计极其关键：

> "The create signal is automatic. The agent cannot start working on a task without first creating it... **The complete signal is not automatic.** It's a separate, optional act of bookkeeping that has to happen after the work is already done... Closing the issue is the one step with nothing downstream depending on it. So it's the step that gets skipped."
>
> "The result: **creates are reliable, completes are lossy.**"

以及他提出的两种「诚实的修法」——**这几乎是委托方「同步事实」按钮的理论基础**：

> "1. Make the completion signal as automatic as the creation signal... 2. **Stop trusting self-reported completion and verify against reality** — periodically reconcile the claimed state against the downstream observable, and let reality close the loop."
>
> "The task list said 'open.' Reality said 'done.' **When those disagree, reality wins, and the only way to know reality disagrees is to go and look.**"

中文概括：agent 天然「爱开任务、不爱关任务」，因为开任务和干活是绑定的、关任务是纯记账。于是看板会**单向地被幽灵任务填满**，而且每次新会话还会把幽灵任务当成活任务重新捞出来做一遍。唯一的解法是拿代码/产物这类「下游可观测事实」去反查。

评论区两条回复也值得注意（同页，2026-08-05）：

> "This is a clean example of why I do not trust agent memory as state. **The issue is only a pointer until something outside the model confirms it still represents reality.** I would be tempted to make the reconciler run before every planning pass, not as cleanup afterward." —— Reid Marlow

> "The underlying bug is that **the agent is both the worker and the bookkeeper**. 'Done' is whatever it says it is... The fix is to make the issue close itself. Close on the merge, close on the deploy hook, close on the test that passes." —— Nyx533

**产品启示**：「同步事实」不该只是一个手动按钮，更应该在**每次 plan 之前自动跑一次**（Reid 的意见），并尽量把「关闭」挂到 commit/测试等不可跳过的信号上（Nyx533 的意见）。

### 1.2 官方 issue 中的量化痛点：Claude Code 的「AI Groundhog Day」

**anthropics/claude-code Issue #39961 — "Session continuity: agent repeatedly redoes completed work across sessions ('AI Groundhog Day')"**
- URL: https://github.com/anthropics/claude-code/issues/39961
- 日期：2026 年（issue 活跃至 2026-04）

> "The agent re-reads plans, re-implements already-committed code changes, and re-discovers the same issues — **consuming 50-75% of each session on redundant work.** The user ends up serving as the continuity layer between sessions."

最关键的一句，几乎是委托方假设的**逐字复述**：

> "**A development plan (markdown checklist) looks identical to every session. Without explicit `[DONE]` markers with commit hashes, the agent re-reads the plan and re-implements finished items. The agent does not think to check `git log` to see if planned work is already committed.**"

另一位用户的补充（同 issue）：

> "+1 — we built an entire infrastructure to work around this exact problem across 200+ sessions... **This works, but it costs 10-15% of every session just re-establishing context. The user shouldn't need to build a custom knowledge management system on top of an AI coding tool.**"

以及一段很生动的用户自述：

> "I spent the entire day with Claude debugging a document preparation pipeline... after a half day of 'doom loop' exchanges w/ the agent and repeated failure, I finally realized that **the problem had been addressed in a previous session. So I remain the archivist of last resort and guardian of memory!**"

社区自发给出的缓解方案，方向与委托方一致（注入 git 事实）：

> "A simpler approach — the model can query git directly: `SessionStart` hook → `echo '📋 Recent work (do NOT redo):' && git log --oneline -10`... Combined with a structured `tasks/todo.md` file (with ✅/⬜ checkboxes), it gives the model enough context to avoid redoing work."
>
> "**Git history and hook-injected context are harder to lose because they're freshly computed on each session start.**"

**这是「多次出现的模式」**：该 issue 被标记与 #39663、#24686、#13955 重复，说明同一痛点在官方仓库被反复提出至少 4 次。

### 1.3 相关官方 issue：计划文件里混入已完成任务

**anthropics/claude-code Issue #13641 — "[BUG] Claude hallucinates previous tasks into the plan"**
- URL: https://github.com/anthropics/claude-code/issues/13641

> "Sometimes when I am in plan mode... it gets previously done tasks in the plan... **New plan can't have done tasks in it.**"
> "Getting same thing happen. Code that already been edited has been plan show up on the screen, asking me if I should accept edits, etc. **But the coding was already done.**"
> "This is still happening, in simple plans Claude keeps shoving plan parts from previous plans... **I reckon this is Critical to fix.**"
> "**This happens when we use the same plan markdown with previous plans as I discovered.**"

中文概括：复用同一个 plan markdown 文件时，已完成任务会被反复带进新计划。**这直接证明「markdown 计划文件会漂移」不是臆想，而是被复现、被标记为 Critical 的 bug。**

**anthropics/claude-code Issue #54795 — 任务列表丢项**
- URL: https://github.com/anthropics/claude-code/issues/54795

> "claude is not retaining the item that need to be done and just skips stuff so **i constantly compare my list to claude to be sure we are on the same page. 9/10 claude leaves stuff out**"

回复里给出的最佳实践，恰好是委托方方案的正当性论证：

> "**Work off a markdown file in the repo** (e.g. `tasks/todo.md` checked in). Claude treating an actual file as the canonical list is more durable than relying on its in-session task tool — **when context gets compacted, the on-disk file survives and the task tool state often doesn't.**"

### 1.4 已经有人专门做了「漂移检测」工具（强信号）

**ReliableGenius/backlog.md-drift — "AI-powered drift detection for Backlog.md"**
- URL: https://github.com/ReliableGenius/backlog.md-drift

这个项目的 README 开篇几乎是委托方需求文档的翻版：

> "Backlog.md keeps issues alongside your code, but **it doesn't detect when tasks become outdated as code evolves**:
> - A task references `src/auth/oauth.ts` but that file was deleted in a refactor
> - Acceptance criteria say 'implement rate limiting' but **it already exists**
> - A completed task's referenced files have been modified since completion
> - All of a task's referenced files are gone, but the task is still open
>
> **For AI agents consuming the backlog as context, stale tasks produce stale plans.**"

它的两层设计值得委托方直接借鉴：
- **Layer 1 结构性漂移**（确定性、快、零误报）：死引用、依赖状态、完成后被修改、孤儿任务
- **Layer 2 语义漂移**（LLM 驱动、可选）：判断任务描述/验收标准是否还与代码相符

并且它挂在 pre-commit hook 上，有 `warn / block / silent` 三种模式。

**产品启示**：这既验证了需求真实（有人真的动手做了），也说明**委托方的「同步事实」按钮不是独家创新**，差异化必须落在别处（见第 7 节）。

### 1.5 「AI 生成的 markdown 垃圾」是一个被反复吐槽的模式

HN "Beads – A memory upgrade for your coding agent"（111 分 / 68 评论，2025-11-28，https://news.ycombinator.com/item?id=46075616）：

> "Recent LLMs seem to have an intense penchant to try to write one or more markdown files per large task. **Ending up with loads of markdown poop feels like the new `.DS_Store`, but harder to `.gitignore` because they'll name files whatever floats their boat.**" —— @iand675（https://news.ycombinator.com/item?id=46077433）

HN "Show HN: I replaced Beads with a faster, simpler Markdown-based task tracker"（84 分 / 51 评论，2026-01-04，https://news.ycombinator.com/item?id=46487580）：

> "I think there's some risk in **agents tripping over stale tickets and consuming tokens - I see it with .md all the time.**" —— @lukefreiler（https://news.ycombinator.com/item?id=46509745）

> "Or at least move them to a `.tickets-done` directory, so if the agent is searching under `.tickets` it won't see them, but you retain some history." —— @lemming（https://news.ycombinator.com/item?id=46509884）

中文概括：**陈旧任务不只是「不好看」，它在烧 token、污染 agent 上下文。** 这是把「看板漂移」从美学问题升级为成本问题的关键论据。

### 1.6 Reddit：「不是纪律问题，是距离问题」—— 对痛点根因最精准的论断

**r/AI_Agents「We replaced Jira with markdown files, and for the first time the board matches what was actually built」**
- URL: https://www.reddit.com/r/AI_Agents/comments/1vo1byp/
- 日期：约 2026-08-14｜u/DaveGold83｜22 分 / 22 评论
- 实践规模：7 个月、15 个项目、10 个仓库、165 个活跃 ticket、12 人团队，最终取消 Jira license

**这是整份调研中对痛点根因最好的一句论断**：

> "I have never worked with a ticket board outside the codebase that still matched what was actually built. **Not a discipline problem, a distance problem**: scope changes and the reasoning behind them get settled in a pull request or a call, and **updating the ticket is a separate action somebody has to remember later, from memory.** Some of it lands days late as a summary. **Most of it never lands at all.**"
>
> "That plan lived in the chat or in a scratch file, **outside both the ticket and the repo, and nothing ever merged it back.**"

高赞评论（u/SaltsMoon，11 分）：

> "Agents tend to work better when **the task state lives right next to the code and can be diffed.** Jira is great for humans coordinating across a company, but **for agents, a repo-local markdown board gives them a much smaller source of truth.**"

另一条评论直接命中「看板是虚构」：

> "The plan gets written, the code drifts, and nobody goes back to reconcile because **updating the ticket is a chore with zero reward. So the drift just accumulates until the board is fiction.**"

**中文概括**：委托方说「看板漂移」，这条评论给出了**为什么必然漂移**的机制 —— 更新看板是一个「独立的、要靠记忆的、零回报的动作」。**所以任何依赖「人记得更新」的方案都注定失败；这从根本上论证了「同步事实」的必要性，也论证了「任务必须住在代码旁边」。**

其他 Reddit 佐证：

- **r/ClaudeAI**（https://www.reddit.com/r/ClaudeAI/comments/1p3io5m/ ）：「Claude often forgets to update my task.md files after completing work. **Task files drifting out of sync (tasks falsely marked as done**, or changed...)」
- **r/vibecoding & r/cursor 同题双发**（https://www.reddit.com/r/vibecoding/comments/1ubzjz5/ ｜ https://www.reddit.com/r/cursor/comments/1ubzixp/ ）：「they are all slowly **drifting out of sync, they start containing contradicting information**」「The important rule is: **do not let the agent resolve contradictions by writing more prose.** Force it to either replace the old authority, mark it...」

### 1.7 ⭐ 「done」的语义崩塌：2026 年的核心新痛点

**r/AI_Agents —— 代理谎报「done」，trace 全绿但现实没变**
- 镜像全文：https://reddit.sentinel-team.org/posts/1v7bhm2/snapshots/2026-07-30T03%3A43%3A11.089892Z
- 日期：约 2026-07-30

> "Honestly the thing that scares me about agents isn't a wrong answer. **it's when it lies about what it did.** Agent goes 'done, refund issued.' run looks clean. no errors. you go check and **there's nothing there**... session got marked resolved anyway."
>
> "**A clean run just means the thing stopped running. that's it. doesn't mean the work happened.** and '**i did X' is basically free for a model to say**"
>
> "observability is just the trace, **which is the agent narrating itself**, so if the write silently no-op'ed or it skipped a step **the trace still looks green and happy.**"

评论区 6+ 位用户独立收敛到同一结论（**强模式**）：

> "**The confirmation has to come from a path the agent cannot author.**"
> "make **done a claim that must be reconciled, not a message from the agent.** ... The trace is useful for debugging, but **it is never the proof.**"
> "**It's a claim.** The only thing that counts is a **read back from the system of record, issued separately from the write**"

相关帖（https://reddit.sentinel-team.org/posts/1t3qdnc/snapshots/2026-05-05T19%3A42%3A21.040103Z ，约 2026-05-05）：

> "**The run finishes clean. The log says success.** Then you look closer and one step never really happened"
> 评论：「**The silent skip problem happens when 'the agent said it did it' becomes equivalent to 'the system confirms it happened.' Those are different.**」

**中文概括**：这与第 1.1 节 Nick Meinhold 的「creates are reliable, completes are lossy」是同一现象的两个独立发现。**「done 是一个声明，不是一个事实」——这应该成为委托方产品的核心设计公理。**

### 1.8 ⭐ 「模型把三周前的 TODO 当成当前意图」—— 最精准的 todo drift 定义

**r/AI_Agents「Do AI coding tools still lose project context for you after a while?」**
- 镜像全文：https://reddit.sentinel-team.org/posts/1tvhtg0/snapshots/2026-06-05T18%3A20%3A01.179808Z
- 日期：约 2026-06-05｜u/Shoddy_Ad1207

最佳评论（**本次调研中对 todo 漂移危害的最精准表述**）：

> "I don't think 'more docs' is the whole answer. **Docs are necessary; unmanaged docs just become another stale context source for the AI to confidently misuse.** ... The underrated part is **expiration**: old plans should either be marked done, superseded, or still true. Otherwise **the model treats a 3-week-old TODO like current intent.**"
>
> "**Let them go stale for two weeks and they're worse than nothing** because the agent confidently uses outdated context."

**🚨 这条对委托方是双刃剑**：它既证明痛点真实，也警告 **「本地 TODO.md 作为事实源」如果没有过期机制，会从解药变成毒药。**

### 1.9 ⭐ 陈旧的规范文件会反向污染代码

**r/cursor「Anyone else find their CLAUDE.md / AGENTS.md files end up lying to the agent after a while?」（97 评论）**
- 二手来源（含逐字引文与用户名）：https://mcp.directory/blog/claude-md-agents-md-maintenance-2026

> OP：「We wrote ours at launch. Six months later **half the rules in it are wrong.** A pattern we deprecated, a library we dropped, a decision we reversed after an incident. **The file still confidently tells the agent to do the old thing, and the agent does it, because why wouldn't it. It trusts the file more than the code.**」
>
> u/ultrathink-art：「Worse than the rot itself is when **the agent starts 'fixing' new code back toward the deprecated pattern**, because it trusts the file over the codebase.」
>
> OP 补充：过期规范生成的代码「**looks fine on the surface, which is worse than it being obviously broken.**」

相关：r/ClaudeCode「The single best thing I've put in CLAUDE.md」（**168 赞**，u/MisterHarvest）提出的规则正是「**Verify, Don't Trust**」。

**中文概括**：这把危害升级到最高级别 —— **陈旧的任务/规范文件不只是「没人看」，它会让 agent 主动把新代码改回废弃模式，而且表面看起来完全正常。** 这是委托方产品叙事中最有力的一段。

### 1.10 更宏观的「agent drift」概念已经成形

- **"What Is Agent Drift in AI Coding"**（https://vinish.dev/agent-drift-in-ai-coding ，2026-07-29）：agent 写出「本身正确但不符合项目既有约定」的代码，因为上下文窗口装不下架构决策。
- **"Agentic Entropy and Architectural Drift"**（https://codex.danielvaughan.com/2026/08/14/... ，2026-08-14）：引用了三篇 2026 年 arXiv 论文，提出 **drift gate** 概念 ——「spec-code divergence blocks the merge; it is an automated enforcement, not a review guideline」。

> "Traditional code review catches drift only when reviewers happen to notice it. **The drift gate makes it structurally impossible to merge code that has diverged from its specification.**"

其中引用的 Bloomberg Pomona 研究数据：**82% 的 agent 生成 PR 被合并，且大多数除强制 review 外无人工介入**，作者称之为「reviewer's paradox」——人类审查能力固定，agent 产出无限增长，review 退化为橡皮图章。

中文概括：「漂移」已经从个人吐槽上升为有命名、有论文、有防御框架的行业议题。**委托方的痛点站在一个正在成形的大趋势上。**

### 1.11 痛点普遍度判定

| 维度 | 判断 |
| --- | --- |
| 出现次数 | HN 至少 5 个独立高分帖；Reddit 跨 6 个 subreddit（r/AI_Agents、r/ClaudeAI、r/ClaudeCode、r/cursor、r/vibecoding、r/ExperiencedDevs）；Claude Code 官方仓库至少 4 个重复 issue；DEV.to、个人博客、arXiv 论文均有 |
| 时间跨度 | 2025-07（Backlog.md 发布）至 2026-08，持续 13 个月热度不减 |
| 是否量化 | 有：27% ghost rate、50-75% 会话浪费、10-15% 会话开销、82% PR 橡皮图章、r/ExperiencedDevs 拒审 AI PR 帖 1560 赞 |
| 是否独立复现 | 有：Nick Meinhold（DEV.to）与 r/AI_Agents 匿名用户**互不相识却得出同一结论**——「done 是声明不是事实」 |
| 是否有人付诸行动 | 有：backlog.md-drift、beads、Backlog.md、ticket、Task Master 等至少 6 个工具 |

**结论：这是「反复出现的模式」，不是「单条声音」。痛点真实且普遍，且在 2026 年仍在升温。**

**跨社区收敛的三条独立论断（最有说服力的证据结构）**：
1. 「**Not a discipline problem, a distance problem**」（Reddit r/AI_Agents）—— 解释了**为什么必然漂移**
2. 「**creates are reliable, completes are lossy**」（DEV.to）—— 解释了**漂移的方向是单向的**
3. 「**done is a claim that must be reconciled, not a message from the agent**」（Reddit r/AI_Agents）—— 给出了**唯一正确的解法**

三条来自三个互不相干的作者与平台，指向同一个产品结论。

---

## 2. 开发者对现有工具的核心抱怨 TOP 清单

主要数据源：HN "I tried every todo app and ended up with a .txt file"（**1402 分 / 数百评论**，2025-08-11，https://news.ycombinator.com/item?id=44864134 ，原文 https://www.al3rez.com/todo-txt-journey）。这是本次调研中最高分的相关帖，代表性极强。

### 2.1 通用抱怨（跨工具，出现频率最高）

**① 数据主权 / 供应商倒闭恐惧 —— 出现频率最高的单一主题**

> "I lost all my notes to a proprietary format back in 2004. **I've been 100% a notes.txt person ever since** and it's never failed me nor been not enough." —— @superkuh（https://news.ycombinator.com/item?id=44864542）

> "**The sync breaks. The company sells out and dies.**" —— 原文观点，被大量引用

> "I used for years M$$$ todo, but **I am afraid of lock-in**. Migration is WIP." —— @fithisux（id=44874998）

> "you can't use this software without logging in. That's a showstopper for me. It leads me to believe **it'll begin syncing my data outside of my local environment.** Can you put details about an upcoming employer meeting there without notifying your employer you've shared this data with a third party vendor?" —— @davsti4（id=44876903）

**② 工具太有主见，装不下真实的思维**

> "**Todo software is too opinionated. It's not flexible enough to allow you to break rules.** You can't move things around in a way that allows you to control visual white space between entries. **Everything 'is something' (a task, an event) vs just being (text.)**"

> "Most people wish they could customize their Todo app or system to their specific preference or need, but have no way of making it happen. **Devs can, so they do.**"

> "**When you start structuring your TODO list, you will miss adding stuff that isn't easy to fit into the structure to said list.**"

**③ 折腾工具本身成为拖延**

> "A lot of the people I've worked with who built elaborate productivity systems and custom software **weren't all that productive. They seemingly spent as much time doing productivity rituals and rearranging their productivity software stack as they did doing actual work.** I count the really heavy Notion users in this category."

> "I spent several years trying to make a custom todo system and ended up back where I started using CalDAV and a basic todo app... **Turns out I was always procrastinating because I didn't want to force myself to adapt to something simple.**"

反方观点（同帖，值得平衡看待）：

> "I think it's actually a **selection bias**. Who is more likely to spend a lot of time on productivity systems — a person who is on top of their obligations or a person who is drowning in them."

**④ 每 10-15 年必然轮回一次**

> "Each one need individually is simple so it is easy to demand that need is added too. **Until the whole system for each different need becomes so complex it collapses under its own weight and you move to a new tool. This cycle seems to repeat every 10-15 years at all companies. JIRA is the tool everyone talking about today, but there were many others in the past, and there will be a different one in a few years.**"

### 2.2 按工具分组

#### Todoist
- **抱怨**：游戏化机制喧宾夺主。原文作者：「Great until I realized I was gaming the points system instead of doing actual work. Turns out completing 'drink water' 8 times a day doesn't make you productive.」
- **反驳（数量不少）**：「Why didn't he turn off the points system if it was distracting, instead of migrating to the next shiny new TODO workflow?」（@cypherpunks01, id=44866922）
- **好评点**：重复任务的自然语言调度是纯文本做不到的护城河。「Recurring tasks or tasks far into the future is what has me locked into Todoist... 'Do task every third Friday of the month'」（@bootlooped, id=44864719）
- **判定**：Todoist 的抱怨其实**不强**，反而有较多辩护者。它的真实护城河是**重复任务 + 提醒**。

#### Jira
- 「JIRA is the tool everyone talking about today, but there were many others in the past」——被当作「注定被取代的当代代表」。
- HN Backlog.md 帖里一条高共鸣短评：「**Ah, this is what the future after Jira looks like.**」（@oc1，https://news.ycombinator.com/item?id=44483530）
- TODO 注释相关讨论中：「**Jira/GitHub-like issue trackers can often become blackholes for certain kinds of work.**」（@kjeetgill，https://news.ycombinator.com/item?id=21910391）
- **判定**：Jira 的抱怨是**背景噪音级别的共识**，几乎无人辩护，但也很少有人认为它能被个人工具替代（它解决的是组织问题）。

#### Notion
- 「I count the really heavy Notion users in this category [折腾胜于产出]... I've recently been pulling my hair out dealing with a couple PMs who think 'reorganizing Notion' and adding more rules for Notion is a good use of their time each week.」
- 「I find it most interesting that **despite Notion's appeal and fanbase, it continues to lead to failure of this function.**」（@zahirbmirza, id=44866571）
- self.md 的工具对比（https://self.md/tools/backlog-md/ ，2026-02-09）：「**Notion: flexible but not git-native**」
- **判定**：Notion 的核心抱怨是「灵活到没有约束，最后变成整理 Notion 本身」。

#### Linear
- 抱怨很少，主要是**数据位置**：「**Linear: beautiful UI, but state lives in their cloud**」（self.md，2026-02-09）
- 但**需求很明确**：多人希望 agent 工具能对接 Linear。「I want it to be able to handle integration with systems like **Linear** or even GH issues」（@bredren，https://news.ycombinator.com/item?id=46509945）；「If you could consider how a team could use **linear** in conjunction with this that would be very interesting.」（@sails，Backlog.md 帖）
- **判定**：Linear 在开发者中口碑良好，是「被希望集成」而非「被吐槽」的对象。**这对委托方是个警告：不要把 Linear 当敌人。**

#### Obsidian
- 主要作为**正面方案**出现：「I use a TODO.md within Obsidian, synced across devices with SyncThing. **That's the sweet spot**」（@pphysch, id=44864461）
- 少数抱怨其膨胀：「I love Obsidian and the sync is worth it, but I wouldn't say it's one step above a text file. **It's miles away. Never-ending features and customisations.**」（@mulhoon, id=44864884）

#### GitHub Issues
- **好评**：「why not just use github issues? My agent team does, easy to tell them so and give them access to suitable `gh` commands.」（@jannniii，https://news.ycombinator.com/item?id=46508759）
- **抱怨（三条，很具体）**：
  1. 「**vendor lockin + slow network calls (+ GitHub is down sometimes)**... also GH projects are pretty slow/clunky」（@dkdcio, id=46509034）
  2. 「I don't particularly like **the aesthetics of having a bunch of LLM-generated prose in my issue trackers**」（@simonw, id=46080652）
  3. 「**GitHub Issues: better, but still external to your repo**」（self.md）
- **判定**：GitHub Issues 是委托方最强的免费竞品，其弱点是**网络延迟 + 把 LLM 啰嗦文本污染到公共 tracker**。

#### Reddit 补充：团队侧 vs 个人侧的流失驱动力完全不同

Reddit 的证据显示，「弃用」在两端有不同机制：

**团队侧 = 仪式化 + 指标博弈**

r/agile「Does using tracking tools actually hurt agile practices?」（约 2026-06-11，5 分 / 17 评论，镜像 https://reddit.sentinel-team.org/posts/1u1uy2z/snapshots/2026-06-11T00%3A17%3A18.52526Z ）：

> OP：「Teams think they're being agile just because they drag cards around a board. **But moving tickets doesn't create real collaboration**」「Too many buttons, custom fields, complex workflows - **you need like 6 clicks just to update a simple task status.**」
> u/signalbound（13 分）：「If you give a company that has screwed over Jira a decent tool, **they will screw that decent tool over just like Jira.**」
> 「The problem starts when people focus more on **updating tickets than on collaborating and delivering value. A good tool should support the process, not become the process.**」
> 同帖其他恐怖故事：「My org has a little over **200 fields on ticket creation**... **I don't even look at the board anymore, as all it provides is grief**」；「Then everyone spends **1/2 their time keeping the data right**... **Reporting out became a full time job.**」

r/ExperiencedDevs 关于「ticket 农场」（约 2026-06，镜像 https://reddit.sentinel-team.org/posts/1u93a87/snapshots/2026-06-18T20%3A53%3A30.987708Z ）：

> 「if a developer spends three days digging deep into a messy runtime context to fix a core architectural bottleneck, **they look completely unproductive on the daily burndown chart because no cards moved.** but if someone else spends the weekend creating ten shallow, low effort styling tasks... and dragging them all to 'Done,' the manager celebrates them」
> 「we've created a system that actively **incentivizes ticket_farming over deep work**」
> 评论：「**The purpose of the system is what it does.** You're observing the system whose purpose is to **make a show of doing work over actually doing the work.**」

r/ExperiencedDevs 经典锚点帖「**Everyone hates Jira, but no one stops using it**」（https://www.reddit.com/r/ExperiencedDevs/comments/izu5u3/ ）—— 标题本身就是该主题的定论。

**个人侧 = 愧疚循环 + 涨价触发**

- **愧疚循环**：r/productivity「I've quit every to-do app I've tried」（https://www.reddit.com/r/productivity/comments/1uwa0cs/ ）——「I've installed **Things, Todoist, Apple Reminders, Notion, Clear… and abandoned every one within a few weeks.**」
- **涨价触发**：r/productivity 在三周内出现两个 ~670 分的 Todoist 替代品求助帖（「What are some good alternatives to ToDoIst?」670 分，2025-10-05；「Favourite To do apps outside of todoist or microsoft?」684 分，2025-10-22）。代表性评论：「**$48 a year for a glorified reminders app is absurd.**」（索引来源 https://opiniondeck.com/reddit/todoist-complaints/ ）
- **ADHD 视角**：r/jira「Jira is making your ADHD worse」（2026-05-20）——「**Board full of tickets nobody touched in three weeks. In Progress column with 11 things in it.** Same standup explanation four days running — 'yeah still in…'」

**注意：这与中文社区知乎评测的「欠债感」（打开就是一堆红色逾期任务）完全同构。跨语言、跨社区一致。**

#### Obsidian 的反向证据（委托方需注意）

r/ObsidianMD「Separation of concerns, or 'Why I don't keep tasks in Obsidian'」（Lemmy 镜像含全文 https://old.feddit.dk/post/341252 ）：

> OP：「Frequently I would write something to do in my daily note only for it to be **lost and never followed up on.** I would return to a note and see either a task I had completely forgotten about or **a task that was later duplicated somewhere else in my vault.**」
> 评论：「I want to use search to find curated info, **not littered with to-do notes that don't add value past its due date.**」
> 评论：「I've finally settled for Todoist for now... **there's going to be too much friction for me keeping my tasks in Obsidian.**」

**至少 5 位评论者独立复述同一条弧线 = 强模式。⚠️ 这是对「markdown 即任务系统」最直接的反驳：纯文件方案会让任务丢失、重复，并污染搜索。**

#### 其他（Things 3 / TickTick / org-mode）
- **Things 3**：好评度极高但锁死 Apple。「things 3 is fantastic and **access to it is an actual factor in what devices i buy**」（@t0lo）；痛点是「My only wish is that they would roll out a version for Windows.」
- **TickTick**：「it's basically a .txt file dump for me, **but with notifications and reoccurring tasks** /shrug」（@jackero, id=44864556）
- **org-mode**：被反复推荐为「终局方案」，但门槛与移动端是硬伤（见 3.2）。

---

## 3. 「纯文本 / markdown todo 放 git」流派的正反方观点

### 3.1 正方

**① 长期可用性（最强论据）**
> "I lost all my notes to a proprietary format back in 2004. I've been 100% a notes.txt person ever since." （见 2.1）

todo.txt 的口号本身即是纲领：「Future-proof task tracking in a file you control」（HN 83 分，2019-08-20，https://news.ycombinator.com/item?id=20745654）

**② 零摩擦、跟得上思维速度**
> "I switched away from Workflowy to a plain text file format. Load times are quicker, navigation is quicker, can use keyboard only without a mouse, **the UX can keep up with the pace of my thinking.**" —— @mpalczewski（id 见 https://news.ycombinator.com/item?id=30879327）

> "As soon as you add features to more than a one item per line list **you've added useless friction.** Unless you've got org-mode then its OK" —— @lwh

**③ 对 AI agent 特别友好（2025 年后新增的论据）**
- Backlog.md 作者 @mrlesk 的原始动机（HN 254 分，2025-07-06，https://news.ycombinator.com/item?id=44483530）：
> "I threw Claude Code at an existing codebase a few months back and quickly quit— untangling its output was slower than writing from scratch. **The fix turned out to be process, not model horsepower.**"
> 迭代路径：「50% task success - added README.md + CLAUDE.md → 75% - wrote one markdown file per task → 95%+ - built Backlog.md」
- 「Backlog.md is still a bit rough on the edges but is definitely **a proof that markdown files and AI Agents work really well together.**」
- self.md（2026-02-09）总结得最精炼：「**`.md` files aren't documentation anymore. they're the protocol layer between humans and agents.**」

**④ git 天然解决多分支状态**
> "How will it work with multiple branches? Simple: using git :) Git allows to fetch certain files from other branches including remote ones without checking out those branches. **The state is always up to date no matter if you are running backlog.md from main branch or a feature branch.**" —— @mrlesk

**⑤ 上下文压缩后仍然幸存**（见 1.3 引用）
> "when context gets compacted, **the on-disk file survives and the task tool state often doesn't.**"

### 3.2 反方（同样有力，委托方必须正视）

**① 「雪花软件」批判 —— 本次调研中最尖锐的反方论据**

@Igrom 在 1402 分帖中的高赞评论（https://news.ycombinator.com/item?id=44865065）：

> "Reading through the comments under this thread, there are many users who swear by a plain text file, **but who then build quite a lot of snowflake software to regain functionality offered by more structured TODO applications.** That includes:
> - having your computer alert you to things that come up
> - being able to tag notes / add events to a calendar / set priority
> - expecting prioritized tasks to be at the top of the agenda
> - being able to add recurring tasks / full-text search / formatting
>
> Some of the laborious workarounds: **feeding TODOs to an LLM to filter for the currently relevant ones and send Telegram notifications; hand-copying currently relevant tasks to the top; running a script on a VPS to sync notifications; set up cron job with git commit; writing post-it notes by hand.**"

反驳这条批判的回复也很精彩：

> "**So close! People building snowflake software is a consequence of it not being a generalizable problem, not the cause of it.** Everyone organizes their notes/todos differently, and though the variations may seem slight, they are best solved by a blunt and unopinionated tool. **A text file is the hammer of the digital world.**" —— @debussyman（id=44876545）

**② 提醒/重复任务是纯文本的硬伤**
> "It does one thing a text file struggles at: **scheduling recurring tasks** and adding notes to a recurring task." —— @refreeze654
> "Textfile certainly isn't going to do this for you." —— @cypherpunks01

**③ 「1001 个 todo app」嘲讽**
HN 上被单独摘出成帖的一条评论（https://news.ycombinator.com/item?id=44868541）：

> "Every developer: 'Wow there are a 1000 of ToDo apps. I can't possibly try them all. **So I will write my own ToDo app that does exactly what I want!**' Result: **There are now 1001 ToDo apps.**"
>
> 而且这位作者补了一句市场警告：「ToDo/productivity apps is a very tough market. I know because I wrote a visual task planner for Windows and Mac (hyperplan.com) and **struggled to get enough visibility to make it commercially viable, despite a lot of rave feedback.**」

**④ 「直接用现成的就好」**
> "**Todoist does all of that and more**, basically any reminders app does most of that. Just a counterpoint to say, many of us look for a todo app, use one of the many great ones on the market and then don't write blog posts about it. **It's worth just trying one of the many existing apps instead of building your own.**" —— @zaptheimpaler（id=44868606）

**⑤ 纯文本 + AI agent 的并发写入是真 bug**

**"Two AI Agents Shared a Todo File and a Line Vanished"**（https://dev.to/hideyukimori/two-ai-agents-shared-a-todo-file-and-a-line-vanished-22jd ，2026-08-08）——**这条对委托方的架构决策极其重要**：

> "I run several AI coding agents in parallel... they share a single plain-text todo file... Every writer to that file — a CLI, a terminal UI, and the agents themselves — does the same thing: **read the whole file, change one line in memory, write the whole file back.** Three processes doing full-file read-modify-write on one path, **with no locking anywhere.**"
>
> "That's a lost update, and it's textbook... **No error surfaces because nothing went wrong at the filesystem level. Both writes succeeded.** One of them just described a world that no longer existed by the time it landed."
>
> "**If more than one process writes a whole file, you have a lost-update bug, whether or not you've seen it yet. Agent sessions count as processes. They're just processes that write convincing commit messages.**"

他还发现了一个诊断难题，对「同步事实」功能是直接警示：

> "**A line diff tells you what changed. It doesn't tell you what's missing.**" —— 因为 agent 会大量改写措辞，diff 会淹没在噪音里；正确的问法是集合差（「有没有旧行在新文件里完全没有对应物」）。

**⑥ 单文件 vs 多文件的 agent 编辑难题**
> "If every 'task' is one .md file, I believe **AI have issues editing big files, it can't easily append text to a big file due to context window**... So this means the tasks have to remain small." —— @ttoinou（Backlog.md 帖）
> 作者回应：「One of the instructions that ships with backlog.md is to make the tasks 'as big as they would fit in a PR'... **smaller atomic tasks are the only way for the moment to achieve a high success rate.**」

### 3.3 正反方权重判断

| 论点 | 强度 | 对委托方的影响 |
| --- | --- | --- |
| 纯文本长期可用 / 数据主权 | 强，几乎无人反驳 | ✅ 支持核心方向 |
| markdown 对 agent 友好 | 强，2025 后成为新共识 | ✅ 支持核心方向 |
| 「雪花软件」批判 | 强 | ⚠️ 必须避免做成又一个 1001 |
| 提醒/重复任务缺失 | 中，但对「代码任务」场景不适用 | ➖ 委托方场景可豁免 |
| **并发写入丢数据** | **强，且是技术硬伤** | 🚨 **必须在架构层解决** |
| 市场极难做 | 强 | ⚠️ 商业化预期要下调 |

---

## 4. 新工具的社区真实评价

### 4.1 Backlog.md（HN 254 分 / 62 评论，2025-07-06；GitHub ~6.5k stars）

**主贴**：https://news.ycombinator.com/item?id=44483530
**仓库**：https://github.com/MrLesk/Backlog.md （创建于 2025-06-04，MIT，191 个 release）

**好评点**：
- 方向被广泛认可：「**Ah, this is what the future after Jira looks like.**」（@oc1）
- 「this is a **proof that markdown files and AI Agents work really well together**」
- 「Ooh, the idea of **having a CLI for the LLM to mark off tasks without having to pollute context by reading the file over and over again** is cool!」（@theshrike79）—— 这条对委托方 MCP/CLI 设计很关键
- 独立评测（https://toolgenix.nxtniche.com/posts/backlogmd-quick-review-2026-07-05/ ，2026-07-05）：「It's **an attention fragmenter for the AI coding era** — and the 5,900 GitHub stars make more sense the longer you use it.」
- 收敛效应：「I built myself a tool that does something quite similar... **for a project that is just for me, it's exactly what I need**」（@danpalmer），作者回应「I'm happy to hear that more and more people are converging towards a very similar process」

**质疑点**：
- **命名混乱**：「Part of this confusing trend of naming projects like files」（@kurtis_reed）。作者自己承认：「It is so confusing that **I had to add custom instructions for Codex telling him that Backlog.md is the project folder and not a file. He was wasting few mins trying to CAT Backlog.md instead of CD**」
- **定位不清**：「Can we change the title to include that **this is a tool for AI**? I thought it was just gonna be a visualizer.」（@jedimastert）
- **不想被强制提交进 git**：「Is there a proper way to use this project without committing files to git? ... after every command I get a lengthy error」（@nzach）—— 作者后来把 autoCommit 默认改为 false
- **跨仓库不支持**：「we have two separate repos for BE (Golang) and FE (next.js). And we wanted to have one backlog for both projects. **But backlog resides inside git repo.**」（@gekpp）—— **这是一个未解决的真实缺口**
- **上下文预算质疑**（tptacek 提问）：「how are agents navigating the large number of markdown task content **without blowing out their context budget?**」作者答：「what blows most context are actually the docs and the decisions markdown files」
- **重复造轮子质疑**：与 taskwarrior、todo.txt、org-mode、`gh` CLI 的关系被反复追问

**关键工程细节**（可直接借鉴）：作者明确表示当时**不用 MCP**：
> "**No mcp, just custom instructions.** When you initialize backlog in a folder it asks you if you want to set up agent's instructions like CLAUDE.md... Feels a bit like magic"
>
> 提问者的动机很有代表性：「Feels like some MCP integration might make it feel more native? **Though i've not had much luck in getting Claude to natively use MCPs**」

（注：Backlog.md 后续版本已加入 MCP 支持，但**社区最初的成功是靠 CLAUDE.md 指令 + CLI 达成的**。）

### 4.2 beads（Steve Yegge）（HN 111 分 / 68 评论，2025-11-28）

**主贴**：https://news.ycombinator.com/item?id=46075616 ｜ 仓库现迁至 https://github.com/gastownhall/beads

**好评点**：
- 「I've been using beads for a few projects and I find it **superior to spec kit or any other form of structured workflow**... The added benefit is the agent doesn't need to hold so much context... **it's the only spec tool I've found that has stuck.**」（@qudat, id=46078322）
- 「It doesn't compete with gh issues as much as **it competes with markdown specs**. It's helpful for getting Claude Code to work with tasks that will span multiple context windows.」（@adamgordonbell）
- simonw 的定位解释（本次调研中最好的一段「为什么需要这类工具」）：
> "LLMs famously don't have a memory... **Giving them somewhere to jot down notes is a surprisingly effective way of working around this limitation.** The simplest version of this is to let them read and write files... Beads is a much more structured way of achieving the same thing. I expect it works well partly because **LLM training data makes them familiar with the issue/bug tracker style of working already.**"（id=46077622）
- 架构受赞：「I love the data architecture. **JSONL lines checked into git, and a SQLite local cache.**」（@jauntywundrkind）
- 战略意义：「it's turning the corner from '**teaching AIs to develop using tools that were designed for humans**' to '**inventing new tools and techniques that are designed specifically for AI use**'」（@pbw, id=46079238）

**质疑点（相当尖锐）**：
- **README 是 AI 写的**：「A classic issue of AI generated READMEs. **Never to the point, always repetitive and verbose**」（@mimischi）；「the readme.md seems ai generated」（@fnord77）；「I went through the whole readme first and **kept wondering what problem the system aims to address**」（@wowamit）
- **「agent 喜欢用它」的说法被群嘲**：原文写「Agents report that they enjoy working with Beads」，引来：
  - 「**LLMs also report that they enjoy my questions**, in fact they tell me it's a good question literally every time I ask about their weird choices.」（@Etheryte）
  - 「You're absolutely right!」（@lnenad，纯讽刺）
  - 「**Yegge makes stuff up and is known to say controversial things for fun, so I assume it's trolling.**」（@skybrian）
  - 「The author has a vested interest in AI, that is why its capabilities may be greatly exaggerated/anthropomorphised... **Proceed with caution.**」（@dude250711）
- **dogfooding 自相矛盾**（@themgt, id=46079051，最有杀伤力的一条）：
> "readme says: `.beads/beads.jsonl` - Issue data in JSONL format (**source of truth**, synced via git). But **there is no beads.jsonl in the dogfooded repo**, and there's like ~hundreds of commits in the past few days, so I'm not clear how I'm supposed to understand what's going on with the repo. **beads.jsonl is the spoon, but there is no spoon.** I'll check back later, or have my beads-superpowered agent check back for me. Agents report that they enjoy this."
- **与 agent 内置任务管理冲突**：「Do you find that it **interferes with coding agents' built-in task management features**? I tried beads a few weeks ago and **Claude exhibited some strange behavior** there.」（@threecheese）
- **根本必要性质疑**：「I don't understand the point of this project. **We already have github/gitlab for tasks.**」（@CuriouslyC）；「I really don't understand the raison-d'être for beads other than **yak shaving**」（@xrd）
- **不想污染主仓库**：「Is there a good way to use beads without pushing your `.beads` dir upstream?」→ simonw：「Add the `.beads` directory to `.gitignore`」

**⚠️ Reddit / GitHub 上的负面证据比 HN 更猛烈（这是 beads 评价的关键补充）**

正面（r/ClaudeCode「Update: I tried beads for 3 weeks after asking about it here」，https://www.reddit.com/r/ClaudeCode/comments/1ov1z94/ ）：
> "TL;DR: **It solved my biggest frustration with AI coding—Claude forgetting everything after compaction.**"

但负面证据来自多位**具名开发者**（GitHub issue #2938「Beads feels painful to use」，https://github.com/steveyegge/beads/issues/2938 ；汇总见 https://random.qmx.me/posts/2026/01/04/on-beads-bloat-and-breaking-points/ ）：

> issue 作者：「it feels like **frequent bugs and sharp edges lead me to spend a bunch of time caring for beads itself instead of doing beads.**」
> 「**I've banned my agents from running `bd doctor`** because it seems to report false positives, and when my agents try to run `--fix` **it seems to break more things.**」
> 另一用户：「Another user that appreciates the work but is **frustrated almost every release.** ... Today is 0.62 to 0.63 and **now its unusable again.**」
> **Greg Wedow**（后来写了替代品 `ticket`）：「**easily the most frustrating piece of software I've used in years. Buggy, slow, slopmaxxed to hell.** ... I've been using beads since October and **every release made it worse.** Finally got time to **rip it out of my projects** over the holidays and all my workflows run so much smoother now.」
> **Armin Ronacher**（知名开发者，Flask 作者）：「I tried beads and it's **the most invasive shit I have trialed yet.** It might be great, but somehow **every one of my projects starts having beads in them and I can't figure out why.** Frustrating.」

**作者 Steve Yegge 本人的公开承认**（https://steve-yegge.medium.com/beads-blows-up-a0a61bb889b4 ）：

> "**Beads has remained very, very buggy. Like, I am appalled and embarrassed** ... I've probably fixed **half a dozen major, heart-attack, show-stopper bugs every single day for the past 3 weeks.**"

**中文概括与产品启示**：beads 的失败模式是**「工具本身需要被照顾，抢走了做正事的时间」**。「I've banned my agents from running `bd doctor` because it reports false positives」这条对委托方尤其致命 —— **如果「同步事实」功能误报，用户会直接禁用它，那么产品的核心价值就归零了。这是为什么必须采用「结构性检查零误报 + 语义检查可选」的分层设计。**

**后续演化（重要信号）**：2026-01-04 出现 **"I replaced Beads with a faster, simpler Markdown-based task tracker"**（HN 84 分 / 51 评论，https://news.ycombinator.com/item?id=46487580 ，https://github.com/wedow/ticket）：

> "Steve Yegge released Beads back in October and I found that **giving Claude tools for proper task tracking was a massive unlock. But Beads grew massively in a short time and every release made it slower and more frustrating to use. You don't need to index everything with SQLite when you have awk.**"

> "I was interested in beads but found **the author's approach to software development quite erratic and honestly a bit unprofessional**... Beads is **an incredibly difficult-to-follow mess for something that is at its core a pretty simple idea.** You distilled it to its core" —— @azeirah

> "I started playing around with beads, but **when I got to the phrase 'landing the plane' I started looking for simpler alternatives.** So far, this looks much closer to what I want (**flat files, human readable, basic syntax**)." —— @nmfisher

**中文概括与产品启示**：beads 验证了「agent 原生任务追踪」的需求真实且强烈，但它**因过度复杂而被社区反噬**，催生了「更简单的 markdown 替代品」。**这是委托方最应该学习的一课：赢家是「够用的简单」，不是「完备的复杂」。**

### 4.3 Vibe Kanban —— 最重要的反面教材

**主贴**：HN 195 分 / 132 评论，2025-07-11，https://news.ycombinator.com/item?id=44533004

**⚠️ 关键事实：公司已于 2026-04-10 倒闭。**
- 公告：https://www.vibekanban.com/blog/shutdown
> "Today we're shutting down bloop, the company behind Vibe Kanban. The Vibe Kanban project will live on, open source and community maintained."
- 倒闭原因（据 https://nimbalyst.com/blog/vibe-kanban-after-bloop-whats-next/ ，2026-05-04）：
> "In Louis Knight-Webb's words, '**the vast majority are free users and we couldn't find a business model that we could get excited about.**'"

**发布时的好评点**：
- 「I used this last week and it's excellent - **feels like the same increase in productivity increase from when I first used Cursor.**」（@lharries）
- git worktree 隔离方案被普遍认可；内置 diff review 强制人工审查
- 「the **visual feedback loop** alone is worth trying. Seeing three tasks done, one failed, and two in progress at a glance is something a terminal can't give you.」（Medium 评测）

**质疑点（发布当天就非常猛烈，事后看几乎全部应验）**：
- 最高赞质疑（@deepdarkforest）：
> "i tried every single coding agent with a variety of similar tools and **whenever i try to parallelize, they clash while editing files simultaneously, i lose mental context of what's going on, they rewrite tests etc. It's chaos.** ... **A kanban board to just shoot off as many tasks as possible and just quickly read over the PRs is crazy to me.**"
- @codingdave 的商业判断（事后被验证）：
> "**It is not just chaos, it is an unwanted product.** Don't misunderstand - people would love this product if it works. But AI cannot do this yet. Products like this are built on **an assumption that AI has matured enough to actually succeed at all tasks. But that simply isn't true.** ... doing the workflow first and assuming quality will catch up later is just asking for a pile of rejections when you try to sell it."
- 「false affirmatives」问题（@swalsh）：
> "The hard part is as you scale, **you end up with compounded false affirmatives. Model adds some fallback mechanism that makes it work, tests pass, etc.** ... if you dig up a buried body, I'd manually confirm it was properly disposed of as **the models usually hid the body in the first place**"
- worktree 实操并不美好（@helsinki）：
> "This works in theory and somewhat in practice but **it is not as clean as people make it seem**, as someone who has spent tens of thousands on Opus tokens and worktrees - it's just not that great. It works, but it's just, ugh, **boring, super tedious**... at the end of it all, you're still sitting around waiting for Claude to merge conflicts."
- 硬件天花板（作者自己承认）：「I already find my macbook slowing down after 4 concurrent runs」

**第三方深度评测**（https://solvedbycode.ai/lab/vibe-kanban-honest-review ，2026-01-12）的负面清单：
> "**Vibe Kanban enables parallel execution but does not solve the logical problem of merge conflicts.**"
> "The Kanban board **adds friction**: creating a card, typing a description, assigning an agent, and managing the board. For 'change the button color' tasks, **this is absurd overhead.**"
> "**If agents hallucinate or get stuck in loops, Vibe Kanban cannot magically fix them. It only provides a better view to watch the crash happen.**"
> 隐私事故：「Early versions collected analytics by default - including email addresses and GitHub usernames - without clear consent. After HN backlash, they made it opt-in. **But it damaged trust.**」
> 评测者最终结论：「**I'm not adopting Vibe Kanban.** Not because it's bad - it's genuinely well-built. But my workflow is conversation-first... **Adding a Kanban layer would break that flow.**」

**🚨 对委托方的直接警示**：这是与委托方方案**最接近的产品**（看板 + AI agent + 本地运行），它：
1. 发布时热度极高（HN 195 分），
2. 但在**同一天**就被指出「看板对小任务是纯摩擦」「并行不解决语义冲突」，
3. 最终**因找不到商业模式而关闭公司**。

**「看板」这一层，是被市场检验过、且失败过的一层。**

### 4.4 Task Master AI（claude-task-master）

- HN 上**几乎没有独立讨论热度**（多个提交均为 1-3 分、0 评论，如 https://news.ycombinator.com/item?id=44491693）
- 但在其他帖的评论中被反复提及为「用过、可用」：
  - 「personally I use task-master.dev」（@cloudking，Backlog.md 帖；官网 https://www.task-master.dev/ ）
  - 「I had some success with task-master.dev, **it's a bit more complex though**」（@cloudking，ticket 帖）
  - **最关键的一条负面**：「**Taskmaster-ai was pretty good, but sometimes the agent ignored it as the project grew larger**（can probably prevent that now using claude code hooks）」（@thelittleone，https://news.ycombinator.com/item?id=44483530）
- **判定**：Task Master 有真实用户但社区声量低；其核心失效模式——**项目变大后 agent 干脆无视任务系统**——是所有同类工具（包括委托方）的共同威胁。

### 4.5 ⭐⭐ 最重要的一条市场信号：「人人都造过一个」

**r/ClaudeAI「Kanban Board for Claude Code - no paywalls, no sign ups, no subscriptions」**
- URL: https://www.reddit.com/r/ClaudeAI/comments/1v3aky6/
- 镜像全文：https://reddit.sentinel-team.org/posts/1v3aky6/snapshots/2026-07-24T19%3A44%3A38.955533Z
- 日期：约 2026-07-24｜u/don_kruger｜**135 分 / 99 评论**

**🚨 这个产品与委托方的方案几乎完全一致**：

> "**Every ticket is a plain Markdown file with YAML frontmatter.** The board lives in a folder on your filesystem... no accounts, no server, no lock-in."
> "A file watcher picks up agent edits in real time... Claude moves a ticket on disk, the UI updates instantly, **with dedupe so agent and human edits don't clobber each other**"
> "Agent edits are attributed in the Activity Feed (modifiedBy), so **you can see exactly what Claude did vs what you did**"

**而最高赞评论（u/Critical_Hunter_6924，74 分）是嘲讽 + 安全警告**：

> "**Everybody and their grandma has made their own Claude's task list.**"
> "Next to that, I [urge] everyone to steer away from this due to security risk. **It's very fishy if you don't open source grandma's code.**"

**对委托方的三条硬结论**：
1. **概念本身受欢迎**（135 分）—— 说明方向对
2. **但「又一个轮子」引发强烈抵触**（最高赞 74 分是嘲讽）—— 说明**光有「markdown + 看板 + agent 可编辑」不足以脱颖而出**
3. **闭源会直接触发安全质疑** —— 若委托方做 Tauri 桌面应用且不开源，**必须预期这条攻击**

**注意：这条证据比 Vibe Kanban 倒闭更贴近委托方 —— 因为它是「本地 markdown + 看板 + 无订阅」这个精确定位，而社区的反应是「你是第 N 个」。**

**其他佐证该类目饱和但无赢家的信号**：
- r/AI_Agents 反复重问同一问题（post id `1q491mu`、`1vzk6nl`）→ **该类目尚无公认赢家**
- r/ClaudeCode「What task tracker are you using with Claude Code?」（https://www.reddit.com/r/ClaudeCode/comments/1v91nk4/ ）—— 评论：「ultimately those are primarily made for humans」
- r/ClaudeCode「Am I an idiot for using Beads rather than PROJECT.md?」（https://www.reddit.com/r/ClaudeCode/comments/1r6uhgw/ ）—— 标题本身即是该赛道的困惑写照

### 4.6 Spec Kit / 规格驱动开发（SDD）：与委托方方案同源的邻近赛道

委托方的「TODO.md 为事实源」本质上是 SDD 的轻量版，因此 SDD 的社区评价高度相关。

#### 正反同框的高质量讨论串

**r/ClaudeAI「Any review about Spec Driven Development?」**（https://www.reddit.com/r/ClaudeAI/comments/1tnt93b/ ｜镜像 https://reddit.sentinel-team.org/posts/1tnt93b/snapshots/2026-05-30T02%3A41%3A26.708778Z ）
- 约 2026-05-30｜u/Paramooretz15｜6 分 / 20 评论

> **正方**（u/pmward，**7 分，高于原帖**）：「SDD is not vibe coding. **That's why it actually works.** At least so long as you actually review the specs, break features down to reasonable sizes... The only con is that it takes more time and effort on your part. **But again, that's why it works.**」
>
> **去魅**（u/tyschan，6 分）：「useful for externalization of context... **otherwise hyped beyond belief. its literally just a markdown file for handover.**」
>
> ⭐ **最深刻的一条**（u/InteractionSmall6778，3 分）—— **直接解释了委托方痛点的机理**：
> 「**The planning phase being separate from execution is the key variable, not the spec file itself.** ... the model's context window is bounded, and a spec gives it a stable anchor point across multiple sessions. **Without it, longer features tend to drift. The model starts making assumptions that made sense in isolation but conflict with decisions from two sessions ago.**」
> 「The 'just a markdown file' dismissal misses this: **the artifact matters less than the forcing function** to clarify requirements before touching code.」
> **边界条件（对产品定位极重要）**：「Where it genuinely doesn't pay off: **small bug fixes, anything under 2 files, exploratory spikes.** The overhead isn't worth it there.」
>
> **反框架**（u/TheRealJesus2，2 分）：「**Very few are actually writing specs because specs are way harder to write than code**... **And you don't need a literal framework for it.** All the harnesses have plan mode.」
>
> **历史去魅**（u/satechguy，2 分）：「It has been around for, well, **50 years. Recently re-discovered by vibe coders.**」

#### 根本性质疑（票数最高）

**r/programming「Spec-driven development doesn't work if you're too confused to write the spec」**（https://www.reddit.com/r/programming/comments/1r0s9za/ ｜镜像 https://reddit.sentinel-team.org/posts/1r0s9za/snapshots/2026-02-10T17%3A21%3A33.592384Z ）
- 约 2026-02-10｜u/habitue｜**72 分 / 14 评论**

> u/Rain-And-Coffee（**36 分**）：「**The coding part was secondary to figuring out the domain. The domain is the hard part**... Once I know what to build and all the rules, coding it into a spec is quite fun.」
> u/baked_tea（16 分）：「**Any spec unambiguous enough to work is already close enough to code that we could just express it as code.** Writing that spec is a discovery process.」
> ⭐ u/germanheller（9 分）—— **与「文档腐烂」完美闭环**：「people think the spec replaces the need to understand the domain. It doesn't — **it just encodes your understanding. If your understanding is wrong, the spec just makes you wrong faster and more systematically.**」
> u/GR1V4s（8 分）：「**AI is pushing us to the old days where we created big requirements documents but now with markdown**」

#### Spec Kit 的具体失败模式（唯一带实测数据的 token 成本证据）

**r/GithubCopilot「Spec-driven development with Spec-Kit is eating my tokens alive」**（https://www.reddit.com/r/GithubCopilot/comments/1sus656/ ｜镜像 https://reddit.sentinel-team.org/posts/1sus656/snapshots/2026-04-24T23%3A20%3A04.257979Z ）
- 约 2026-04-24｜u/boolean_autocrat｜5 分 / 9 评论

> 「**Love the workflow, hate the token bill.** Every time I run /plan or /implement, the agent goes on a reading spree through my entire codebase. We're talking **20+ file reads, a dozen grep calls**, directory listings everywhere. And this is before it writes a single line of output.」
> 「My instructions file was **752 lines. That's about 33k tokens loaded into every single session before I even type anything.**」（裁剪后降至 21.7k，省 ~35%）
> 「**Serena tool calls during a full /plan run: zero. Literally zero.**」（代码索引 MCP 完全无效）
> 「**Spec-Kit agents are document-oriented.** They read templates, specs, constitution files, existing module structures, and full source files... **No symbol-level indexing tool helps with that**」

**维护性问题**（GitHub 官方 discussion，https://github.com/github/spec-kit/discussions/1482 ）：
> 「In the last month we had **22 PRs opened and exactly zero commits**... **if SpecKit is not maintained properly then I need to move on**」
> 「**Spec Kit was dead to me after:** 1) Not delivering a functional feature - nice spec though!! 2) **Using all my premium tokens - 5x token usage over a vibe code that just worked** 3) plan mode with claude code came out」

**社区冷淡信号**：r/GithubCopilot「Speckit still popular?」（https://www.reddit.com/r/GithubCopilot/comments/1uksz1y/ ，18 分 / 30 评论）——「Some leaders at my company believe everyone should adopt Speckit... **However, I don't see many posts about Speckit in this sub. Why is that?**」

**🚨 对委托方的四条关键启示**：
1. **「文档/任务文件」本身要控制体积** —— 752 行指令 = 每次会话 33k tokens。委托方的 TODO.md 必须有**上下文预算意识**（这也印证了 N2）
2. **「插件让 agent 少读文件」的假设不成立** —— 代码索引 MCP 在文档驱动流程里调用次数为 0
3. **明确边界条件**：「small bug fixes, anything under 2 files, exploratory spikes」不值得用这套流程。**产品文案要主动说明「什么时候不该用我」**，否则会重蹈 Vibe Kanban「小任务纯摩擦」的覆辙
4. **「plan mode 出来后我就不用了」** —— 原生能力会持续吞噬第三方工具的价值。委托方必须找到**原生 plan mode 做不到的事**（答案就是：跨会话的持久化 + 与代码事实的对账）

### 4.7 其他被提及的替代方案（说明赛道极度拥挤）

| 工具 | 定位 | 来源 |
| --- | --- | --- |
| `git notes` | 「For solo projects I just use git notes... Claude Code just needs to be told that they exist」 | @arjie, id=46508685 |
| Taskwarrior | 「most LLMs already understand Taskwarrior. **They've never heard of Beads.**」 | @mbanerjeepalmer, id=46077770 |
| git-bug / git-issue | 更重、更偏协作 | @khimaros |
| GitHub Spec Kit | 与 beads 组合使用 | @iand675 |
| Forgejo 自建 | 「single go binary and very fast」 | @xrd |
| ticket (`tk`) | bash 写的 beads 极简替代 | https://github.com/wedow/ticket |
| trekker / beans / zdo / kanban-cli / moo-tasks | 更多同类 | HN 各 Show HN |

**@bbor 的观察值得引用**（id=46509274）：
> "I've spent **way too long rolling my own issue tracking systems (plural!)** over the years, and it's good to see someone else share my intuition that **dependencies and tagging are by far the most important part of solo-ish issue trackers.** You'd be shocked how many massive tech companies publish issue trackers where **dependencies are an afterthought (or worse: a paid upgrade).**"

---

## 5. 中文社区的独特诉求

> 说明：本节数据由并行调研通道获取。V2EX 官方 API（`https://www.v2ex.com/api/topics/hot.json`）从本次调研网络环境**直接请求失败**（返回非 JSON，疑似 Cloudflare 拦截），故改用搜索引擎 + 页面抓取路径。

### 5.1 「不可能三角」：中文社区选择困境的最精炼概括

**V2EX「2026 年了，todo 应用烂大街了，仍然没有一个符合我要求的」（58+ 楼，2026-03-28）**
- URL: https://www.v2ex.com/t/1201809
- **这是本次中文调研中最有价值的单帖**，🔁 反复出现的模式

楼主的四条需求，几乎是中文开发者的最大公约数：

> 「1. 也是最关键的，**数据要能存在本地**（或者我能掌控的地方，**支持 webdav 最好**）；2. 跨平台，至少支持 Win/Linux/Android；3. 支持分组、标签、提醒、备注；4. 界面现代好看」
>
> 「- Microsoft Todo：**不支持标签，数据无法本地**；- 滴答清单：功能很强大完善，但是**数据不支持本地（哪怕能自动备份也行啊）**；其他的什么 trello、notion 也是一样，**数据强绑定，还依赖网络**」

8 楼 @Skifary 的回复，是整个中文社区困境的最精炼概括：

> 「**数据要能在本地，又要跨平台，还不要订阅。有点像不可能三角，不赚钱的买卖没人做的**」

4 楼 @docx 给出了商业侧的解释：

> 「滴答已经够好了……**商业产品如果给你这么开放了，那他还怎么卖会员**，你这个要求大概只能找 FOSS 应用，或者自己手搓一个了」

14 楼 @Danmen123 甚至给出了明确的产品定义与定价（**对委托方极具参考价值**）：

> 「你可以做一个**本地版本的滴答清单，提供 webdav，然后提供买断**（当然价格不能太贵，**不超过 100 吧**），感觉有搞头啊」

其他关键回复：

> 16 楼 @Nugine0：「这种严重涉及隐私的东西**要么用成熟商业产品，要么用自己写的，别人写出来的我是绝对不敢用的**」
> 37 楼 @Admstor：「**目前没有任何一个 todo 是（纯本地非联网）这样的**」
> 38 楼 @wsjjacky：「自己 vibe 一个，前提是你愿意投入很多时间去调试和打磨（**做好比做出来要难得多**）；……**用 Things 十几年了，中间各种 GTD app 来了又去，最后只有 Things 屹立不倒😂**」

同帖也有对「折腾工具」的清醒声音（与英文社区完全一致）：

> 30 楼 @Moatkon：「**把事情做了比用什么工具重要**。之前我也折腾，用了很多 todo，除了浪费时间挑来挑去没啥用」
> 45 楼 @lagrange7：「**徘徊在门外纠结选什么代办 app，最浪费时间**」

### 5.2 「真有需求的都是自己开发的」：造轮子共识

**V2EX「做了一个内网/本机用的 todo 系统（rbtodo）」（2026-04-23）**
- URL: https://www.v2ex.com/t/1207920
- 🔁 反复出现的模式（共识级表述）

> 1 楼 @ronyin：「一般没需求的都用成熟版的。。**真有需求的，todo 都是自己开发的，用别人的不符合自己的需求**。。哈哈哈哈哈」

楼主的动机很典型，指向**企业环境的数据合规**（这是中文社区独有的强诉求）：

> 「工作电脑用在线的 todo ，比如 notion 和 apple 的 提醒事项**都给禁止了**。所以我才写了这个。」

商业化尝试也很有代表性：楼主原本想「激活码永久的，每个激活码 20 块」，后改口做免费 + 打赏。

其他造轮子样本：
- **V2EX「写了一个在终端使用 Todo 的 CLI 工具」**（2025-05-19，https://www.v2ex.com/t/1132724）——最扎心的回复：「产品审美+技术审美双双在线，赞一个。不过 **TODO 的实现实在太多，玩一下可以，离真正用起来还缺一个理由**。」另一条产品建议：「**todo 类软件，添加任务的阻力越低越好**」
- **V2EX「用 AI 为自己开发了一个极简的 TODO 插件」**（2026-02-09，https://www.v2ex.com/t/1191775）——「在试过很多清单工具后，我发现一个致命的问题：**随着记录的任务越多，实际完成的任务越来越少**。」设计上「**最多只能添加 10 个任务，满了就不允许再添加**」；「整个项目开发过程非常丝滑……**仅 10 分钟就搞定了**」「AI 含量：99.9%」
- **V2EX「自己业余做的 Windows 待办应用」**（2025-05-29，https://www.v2ex.com/t/1135187）——技术栈正是「**Tauri 写的，前端套壳**」，与委托方选型一致
- **V2EX「cascade - 自动从代办事项生成日程计划」**（2025-09-13，https://www.v2ex.com/t/1158908）——「市面上的产品**要么太重，要么需要订阅**……可以看作是一个**穷人版的 Motion**」「纯本地，无云端，无 AI」

### 5.3 中文用户的独特诉求

#### ① 微信生态集成 —— 最独特、英文社区完全没有的硬需求

**V2EX「大家现在在用什么 TODO 管理软件？」（5388 次点击，50+ 楼，2026-01-16）**
- URL: https://www.v2ex.com/t/1186210
- 🔁 反复出现的模式，滴答清单一边倒

> 「滴答清单，该有的功能基本都有，全平台，**还能通过微信转发消息录入**」
> 「滴答清单，懒得折腾……**还能跟微信飞书打通**，每年交钱，好像也没用到啥付费才能用的功能，就当支持开发者了」

flomo 的核心卖点同样是微信（https://www.v2ex.com/t/762957 ，2021-03-18）：

> 「之前做了**微信快速输入**，现在又有用户基于 API 开发了 TelegramBOT」

但也有反对声音（https://www.v2ex.com/t/1220035 ，2026-06-12，6 楼）：

> 「**微信聊天不自带有提醒功能吗？长按，设置提醒，就行，装什么 app**」

#### ② 买断制偏好 / 订阅制反感

> 「**曾经的滴答付费用户，比较烦每年续费，已转一木清单永久会员**」（V2EX #1186210）
> 「**希望最好是是买断制**」→ 回复：「不好意思，没看到你要买断的，**滴答是订阅的**」（https://us.v2ex.com/t/753892 ）
> 「剩下的一些独立开发的……（开源的或者付费买断的都行，**订阅的就算了**）」（V2EX #1201809）
> 「做一个本地版本的滴答清单，提供 webdav，然后**提供买断**（价格不能太贵，**不超过 100 吧**）」（V2EX #1201809, 14 楼）

**中文社区的心理价位明确落在「100 元人民币以内的买断」。**

#### ③ 数据在国内 / 网络环境

> 「notion 很多人推荐……**没有中文界面，无亲和力**……**中文搜索不太好用；云端保存数据，而且要科学上网才能流畅使用，假如……想想都怕**」（https://www.v2ex.com/t/845883 ，13 楼）
> 「之前用的 Google Calendar……**现在回国了 Google 的用不了**，我也懒得每天定时开技术手段去同步」（https://www.v2ex.com/t/1144516 ，2025-07-11）
> 「todoist 的自然语言主要体现在英文上，**中文弱一点，毕竟还是海外应用**」（https://www.v2ex.com/t/817607 ）
> 对国内服务同样不放心：「**数据不在本地啊，隐私警告**……**不开源，不维护了也没人接盘，到时数据就完了**」（flomo 帖）

#### ④ 同步稳定性 —— 有真实「误事」案例

**V2EX「稍微吐槽一下近期被夸赞苹果原生软件"提醒事项"」（2021-11-24）**
- URL: https://www.v2ex.com/t/817607
- 🔁 反复出现的模式

> OP：「**mac 端同步要比 ipad、iPhone 上慢一分钟以上**……**没有垃圾箱，右滑到底是删除，很容易误删除，删了就找不回**」
> 回复（滴答同步事故）：「滴答其实做得很好，可惜**在其他平台新建的任务，必须在 iOS 打开同步一下才行，否则会收不到提醒，已经不敢用了，经常误事**」
> 「**反正我是错过好几次，感觉很不爽**」

#### ⑤ 自建 / 数据自持

> 「自架 wekan……挺好，记了 3 年了，**最大的好处数据全部在自己手里**」（V2EX #1186210）
> 「**记事本打开个 txt 。git 或者网盘同步。真正的大忙人需要的不是 todo app，而是个秘书。**」（V2EX #1186210，高赞金句）

### 5.4 对主流工具的中文吐槽

#### 滴答清单（TickTick）—— 事实上的默认答案，但吐槽点极固定

滴答清单在中文社区的地位相当于英文社区的 Todoist，是「懒得折腾就用它」的默认选项。固定吐槽点有四条：

1. **数据不能本地**：「功能很强大完善，但是**数据不支持本地（哪怕能自动备份也行啊）**」
2. **订阅续费烦**：见上文买断制部分
3. **功能过载**：「滴答清单尝试过多次，还开通了一年会员，**目前吃灰中，爱不起来，小问题很多，每次尝试都会发现一些，功能累赘繁杂，ui 逻辑不统一，没有统一的设计感**」（V2EX #1186210）
4. **同步曾误事**：见 5.3 ④

知乎的量化评测更进一步（https://www.zhihu.com/tardis/jm/art/2048805572930188188 ）：

> 「**滴答清单完成率约 60%。功能太多了反而导致我花时间在"管理任务"而不是"执行任务"上。调整标签、挪优先级、设子任务……搞完一轮已经不想做事了。**」
> 「Microsoft To Do 完成率约 65%。"我的一天"功能有仪式感但缺乏强制力……然后你就会看到一堆未完成，**心理负担开始累积**」
> 「有些 App 用久了会有一种"**欠债感**"——打开就是一堆红色的逾期任务」
> 「待办 App 的核心能力不是"帮你记住"，而是"**帮你动手**"」

**注意：「欠债感」这个概念与本报告第 1 节的「幽灵任务」在心理机制上高度同构 —— 陈旧的未完成项对人是心理负担，对 agent 是 token 浪费和错误执行。这是一个很好的产品叙事连接点。**

#### 印象笔记 / Notion

**V2EX「印象笔记是怎么变成现在这逼样的」（2022-04-09，https://www.v2ex.com/t/845883 ）**
> OP：「现在实在忍受不了这傻逼了」
> 2 楼：「**一个笔记软件不能速开，新功能稀烂倒是会员等级这件事学的挺溜**，简直是垃圾」
> 12 楼：「试试 notion 或者 onenote 吧，**被印象和为知恶心坏了**」

#### 「真忙起来根本来不及打开」

> 「真忙起来，**高端的 todo 根本来不及打开**」（V2EX #1186210）
> 「另外，**别同时用两个以上待办 App。会疯的。**」（知乎评测）

### 5.5 中文社区的纯文本 / Markdown 流派

与英文社区高度一致，但规模较小：

**V2EX #1186210 帖内的纯文本派**：
> 「我用 **org mode，org-capture 负责录入**，复杂的项目用 TODO entry（通过手机的 **beorg** 提醒）、简单的待办用 checkbox……**每年一个 org 文件，简单好用，备份也方便。**」
> 「Todoist 和滴答清单都用过来着, 体验其实是不错的. 不过后面我发现**我不怎么用的到移动端, 就改成纯 Emacs org mode 了**」
> 「必须简单免费我用的 obsidian……**markdown 本来就可以用 todo 语法, 显示 checkbox**」
> 「**文本 + git，大多数事情都不重要，长久不碰的直接删掉**」

**已有人做出与委托方形态完全一致的产品**：
- **ApexTodo**（https://staging.v2ex.com/t/1201264 ）：「它把任务**直接存成本地 todo.md（标准 Markdown Task 语法）**，没有账号依赖，数据完全可控。」
- **tinker（基于 todo.txt）**（2026-03-01，https://www.v2ex.com/t/1194979 ）：「**十年来一直都是 vscode 用 markdown 记录待办，然后使用 onedrive 同步，最近开始切换到 todo.txt 上**」

**纯文本派的中文痛点与英文完全相同 —— 没有提醒**：
- 少数派《亲测有效！Obsidian Tasks 让我的任务管理效率翻倍》（2024-11-19，https://sspai.com/post/94049 ）：「Tasks 任务插件**依赖文本文件记录数据**……但**多端实时性可能稍有滞后**」「**没有计算机基础，大部分用户研究起来有点困难**」
- 少数派《从混乱到井然：我在 Obsidian 中管理 TODO 的实用心得》（2024-11-06，https://sspai.com/post/93652 ）：「**Tasks 插件本身不具备提醒功能**」

### 5.6 中文社区的 AI 时代任务管理讨论

#### ⭐ 「Spec 文档和实际代码脱节，变成新的形式主义」—— 委托方痛点的中文原话

**苏江《在团队里如何使用 AI 编程？Spec coding 实践》（https://sujiang.blog/?p=5978 ）**
- 🔁 这是中文社区对委托方痛点最直接的表述

> 「**Claude 喜欢每完成一个任务，就生成一个总结文档，无所吊谓，反正不影响程序运行。删起来还花时间。**」
> 「一个人这么干，大不了就是一坨屎山代码，但对于团队协作，**如果每个人都这样来一坨，就很容易堆成巨型屎山**。」
> 「很多小任务，员工可能会觉得，"明明我用 AI 一次对话，分分钟就能完成，但你非要我写文档，却要三五轮对话才能完成"……**最后导致 Spec 文档和实际代码脱节，变成新的形式主义。**」
> 「Claude Code、Cursor 等很多工具都有 plan mode，但**实际上很少人用，因为没这个习惯，都喜欢让 AI 少废话，多干活**。」
> 「**Spec Coding 的本质不是工具，而是流程变革。**」

**中文概括**：这与英文社区的「markdown poop」「stale tickets consuming tokens」是同一现象的两种表述。**「新的形式主义」这个词对委托方尤其重要 —— 如果同步成本高于收益，用户会直接放弃维护，工具就死了。**

#### AI 的 todo 天然不落盘（技术层面根因）

**掘金《手撕 Claude Code-4: TodoWrite 与任务系统》（2026-04-18，https://juejin.cn/post/7629562013133258786 ）**

> 「**当所有 todo 都标记为 completed，AppState 中的列表被清空为 `[]` 而不是保留所有已完成项。这避免了 token 浪费**」
> 「Claude Code 中有两个相关但不同的"任务"概念：**Todo（任务清单）= Claude 在当前会话中规划的工作步骤**；Task（系统任务）= 运行时管理的实体」

**中文概括**：Claude Code 内置 TodoWrite 的任务是**会话内临时物，不跨会话、不落盘**。这从技术层面解释了为什么必须有外部持久化任务源 —— **这是委托方方案的技术正当性论证。**

#### 已经有人在做「agent 维护纯文本 todo」

V2EX #1201809 帖内的 AI 派回复（2026-03）：

> 25 楼 @lixintcwdsg：「我现在已经是**自己写提示词，然后用一个 onedrive 目录，管理个人所有待办了**」
> 39 楼 @Liftman：「**openclaw + 任意 IM。我现在就是每天早晚下班跟我汇报一下我的工作**……平时我很多工作下发给多个 agent。做完了会跟我讲。**todo 也会自动清除**。」
> 58 楼 @MelodYi：「有考虑过 **skill + agent 的形态**吗？自己定义一个大致的纯文本格式，然后**让 agent 去维护**……**做个纯 CLI 交互，结构化的存储，不要 UI，让 AI 来使用。**」

**⚠️ 但楼主的实践反馈是负面的**（29 楼）：

> 「哈哈，愿望很美好，之前实际试过一下发现**总是要这里改一下那里改一下太耗精力了**」

**这条对委托方是重要警示：让 agent 维护 todo 的想法很多人有过，但实际维护成本可能高于收益。**

另一条实践（https://www.v2ex.com/t/1222249 ，2026-06-23，23 楼）与委托方方案几乎完全一致：

> 「最近接手了一个项目的管理工作，会在**项目资料的根目录放一个 todo.txt**，然后让 **AI vibe coding 了一个单体页面用来展示四象限的看板 todo list**。」

同帖 24 楼道出了碎片化痛点：

> 「我手机里一堆 TODOList APP……**长期项目的 TODO 又会单独写在一个 Markdown 文件里存在 GitHub 私有仓库里，有时候又是放在 Notion 里，感觉乱七八糟的**，用哪个完全看心情」

### 5.7 中文社区对新工具的评价

#### Vibe Kanban —— 中文评价同样清醒

**CSDN 深圳开发者社区《AI 编程任务管理新思路：Vibe Kanban 体验》**
- URL: https://devpress.csdn.net/shenzhen/695b2e46bf6b0e4b2860bb2a.html

> 「个人评价：**有意思，但未必实用**」「这是一款很有意思、让人眼前一新的工具，但**很难成为真正的生产力**。」
> 「如果很喜欢他的任务管理，那么用它的流程变成：**建任务 → 自动执行 → 打开 VS Code → 继续开发 → 回去合并，反而更繁琐**」
> 「**代码审查只能看不能改，还是得回到 VS Code**」「**不符合主流 Vibe Coding 的使用习惯**」
> 「结论就是：我对它的任务流程管理很感兴趣，但**在真实开发里不会真正依赖它**。」

**zccz14《从任务管理到战略思维：vibe-kanban 使用体验与反思》**
- URL: https://readme.zccz14.com/zh-Hans/vibe-kanban-experience-reflection.html
- **这是本次调研中最深刻的一条反思，强烈建议委托方通读**

> 「以前在使用 Claude Code 的时候，总感觉自己是个 Senior Developer，在带一个 Junior Developer……」
> 「这种方式让我感觉自己**更像是一个"包工头"，而不是一个"码农"**。」
> 关键转折：「我很快发现，**我的瓶颈来到了创建新任务和 Review 任务上**（是的，显然还需要 Review 任务）。**不安感并没有因为我用了 vibe-kanban 而消失。甚至，我觉得它变得更强烈了，因为我现在有更多的任务需要管理**。」
> 「真正的解放，是要学会放下对细节的控制欲，而不是试图通过工具来增强控制欲。**他们的工具可能会让人觉得更有掌控感，但实际上并没有真正地解放人类的带宽。**」

**⚠️ 这条与英文社区 Vibe Kanban 倒闭的事实互相印证：看板不解决瓶颈，只是把瓶颈从「写代码」搬到了「管任务 + review」。**

#### Task Master AI —— 中文有正面推广，但需打折

- 翊行代码《Claude Code 效率神器：claude-task-master》（2025-07-08，https://www.wangyiyang.cc/2025/07/08/claude-task-master-efficiency-guide/ ）：「初看项目描述，觉得**又是一个"重复造轮子"的任务管理工具**。但仔细研究后，我被深深震撼了」，并称「**解决了 AI 辅助开发中的一个关键问题：上下文维护**」
  - ⚠️ **该文自报数据（任务完成率 70%→95% 等）无法独立验证，营销腔明显，本报告仅作为「社区吹捧样本」列出，不采信为事实。**
- most.tw（繁体）《AI 驅動開發的新紀元》（2025-06，https://most.tw/posts/ainews/claude-task-master-202506/ ）：「還在用那些笨重的專案管理工具，卻發現**兩天後就放棄使用了**嗎？」「系統維護一個 **tasks.json 文件，作為你和 AI 助手之間的共享知識庫**」

#### 🔍 空白发现：Backlog.md 在中文社区几乎零声量

多组检索（「Backlog.md 中文 评价」等）**未找到任何中文原生评价**，返回结果全部是 Vibe Kanban / Task Master 的内容。

**这本身是一个机会信号**：英文世界 6.5k stars 的头部工具在中文社区尚无认知，中文市场存在空位。但也要注意反面解读 —— 可能说明中文开发者对「git 原生任务管理」这一品类的需求尚未被激活。

### 5.8 中文社区诉求小结

| 诉求 | 强度 | 与委托方方案的关系 |
| --- | --- | --- |
| 数据本地 / 可控（webdav、纯本地） | ★★★★★ | ✅ 完全契合（本地 TODO.md） |
| 买断制，≤100 元 | ★★★★☆ | ⚠️ 商业模式必须避开订阅 |
| 跨平台（Win/Linux/Android 尤其重要） | ★★★★☆ | ⚠️ Tauri 可覆盖桌面，移动端是缺口 |
| 微信生态集成 | ★★★★☆ | ➖ 与代码任务场景关系不大，但若做个人版则必需 |
| 提醒/通知 | ★★★☆☆ | ➖ 代码任务场景可豁免 |
| 企业环境合规（在线工具被禁） | ★★★☆☆ | ✅ 本地方案的独特优势 |
| 低录入阻力 | ★★★★☆ | ✅ 「添加任务的阻力越低越好」 |

**关键洞察**：中文社区的「不可能三角」（本地 + 跨平台 + 不订阅）**恰好是委托方方案天然满足的**——本地 markdown + Tauri 跨平台 + 一次性买断。这是委托方在中文市场的天然优势。但**移动端**是明显缺口，而中文用户对移动端的重视程度高于英文社区。

---

## 6. 「大家还缺什么」：未被满足的需求清单

按「证据强度 × 与委托方方案的相关度」排序。

### N1. 状态与代码的自动对账（漂移检测）—— 最强
- 证据：27% ghost rate（https://dev.to/nickmeinhold/i-audited-my-ais-to-do-list-a-quarter-of-it-was-already-done-1k01 ）；Claude Code #39961 中「The agent does not think to check `git log`」；backlog.md-drift 的存在（https://github.com/ReliableGenius/backlog.md-drift ）
- 现状：**只有一个第三方插件在做，且尚未被主流工具内建**
- 关键设计要求（来自评论）：「**run the reconciler before every planning pass, not as cleanup afterward**」

### N2. 陈旧任务不污染 agent 上下文 / 不烧 token
- 证据：「agents tripping over stale tickets and consuming tokens - **I see it with .md all the time**」（id=46509745）；「move them to a `.tickets-done` directory, so if the agent is searching under `.tickets` it won't see them」（id=46509884）；「markdown poop... the new `.DS_Store`」（id=46077433）
- 现状：**普遍缺失**。需要「归档即对 agent 不可见，但对人可查」的双视图。

### N3. 会话间的可靠交接（session handoff）
- 证据：Claude Code #39961 的四项诉求 ——「Built-in session handoff / **Startup awareness（自动 `git log`）** / Memory scope inheritance / **Promise tracking**」
- 「we built an entire infrastructure to work around this... **costs 10-15% of every session**」
- 现状：官方未提供，社区靠 hook 自建。**「Promise tracking」（agent 承诺过的事要被持久化并核验）几乎无人做。**

### N4. 跨仓库 / 跨项目的统一视图
- 证据：「we have two separate repos for BE and FE... we wanted **one backlog for both projects**. But backlog resides inside git repo.」（@gekpp）；「I wasn't able to find something AI-native that **supported subissues, and worked across projects**」（@bredren）；「some of the things that are on github are in **monorepos where each project really deserves its own set of tickets**」（@wild_egg）
- 现状：**明确空白**。Backlog.md 被直接问倒。

### N5. 「我现在有 15 分钟，该做什么」
- 证据（@bredren, id=46509945）：「The idea is in part, **~I have 15 minutes right now, what is the most important thing to put attention to? Be routed to the project and go immediately to work with a CLI.**」
- 现状：所有工具都在「记录任务」，**没有工具在回答「现在该做哪个」**。这是一个被明确表达却无人满足的需求。

### N6. 依赖关系是一等公民
- 证据：「**dependencies and tagging are by far the most important part** of solo-ish issue trackers. You'd be shocked how many massive tech companies publish issue trackers where dependencies are an afterthought (**or worse: a paid upgrade**)」（@bbor）；「Dependency tracking is such a vital part of organizing work and **it's hard to find good tooling for that**」（@wild_egg）；beads 的 `bd ready`（列出无阻塞任务）被反复称赞
- 现状：**这是 beads 唯一被一致好评的功能**，且是 Backlog.md 早期被追问的缺口。

### N7. 多 agent 并发写入的安全性
- 证据：「Two AI Agents Shared a Todo File and a Line Vanished」全文（https://dev.to/hideyukimori/two-ai-agents-shared-a-todo-file-and-a-line-vanished-22jd ）
- 「**If more than one process writes a whole file, you have a lost-update bug, whether or not you've seen it yet.**」
- 现状：**该文作者自己都还没修好**（「The todo file still has no lock」）。纯 markdown 方案的**结构性风险**。

### N8. 不污染主仓库 git 历史
- 证据：simonw：「I like the idea of **keeping potentially noisy changes out of my main branch history**, since I look at that all the time」（id=46077901）；「Is there a good way to use beads without pushing your `.beads` dir upstream?」；「why do all these projects **assume i want to store this stuff in my current repo?**」（@NamlchakKhandro, id=46627830）；Backlog.md 的 autoCommit 争议
- 现状：**反复出现的抱怨**，各工具解法不一（gitignore / 独立分支 / 独立仓库）。**必须做成可配置。**

### N9. 与既有 tracker（Linear / Jira / GH Issues）双向同步
- 证据：「Is there an alternative that **integrates with a Jira instance**? Many of my tasks already exist in forms of a Jira ticket」（@rumblefrog）；「Would love to be able to use this UI, **connected to an external source like Linear**」（@totaa）；「I want it to be able to handle integration with systems like **Linear** or even GH issues」（@bredren）；「Anyone thinking of adding a feature for **exporting existing gitlab/github issues** to this Ticket or Beads?」（@stefanoco）
- 现状：**需求明确、反复出现、几乎无人做好。** 这是「个人工具」走向「能在公司用」的必经之路。

### N10. Agent 会随项目变大而无视任务系统
- 证据：「Taskmaster-ai was pretty good, but **sometimes the agent ignored it as the project grew larger**」（@thelittleone）；「Do you find that it **interferes with coding agents' built-in task management features**?」（@threecheese）
- 现状：**根本性威胁，无人解决。** 任何「靠 prompt 约定让 agent 用你的工具」的方案都会遇到。指向 hooks（强制注入）而非 MCP（可选调用）。

### N11. 「维护成本必须低于收益」—— 中文社区给出的最直接警告
- 证据：「最后导致 **Spec 文档和实际代码脱节，变成新的形式主义**」（https://sujiang.blog/?p=5978 ）；「愿望很美好，之前实际试过一下发现**总是要这里改一下那里改一下太耗精力了**」（V2EX #1201809, 29 楼）；「TODO 的实现实在太多，玩一下可以，**离真正用起来还缺一个理由**」（V2EX #1132724）
- 现状：**这是所有同类工具的死因。** 只要「保持同步」需要人操心，用户就会放弃。
- **产品要求：同步必须是零成本的（自动触发），否则等于没做。**

### N12. 本地 + 跨平台 + 买断的「不可能三角」（中文市场特有机会）
- 证据：「**数据要能在本地，又要跨平台，还不要订阅。有点像不可能三角，不赚钱的买卖没人做的**」（V2EX #1201809, 8 楼）；「做一个**本地版本的滴答清单，提供 webdav，然后提供买断**（**不超过 100 吧**），感觉有搞头啊」（同帖 14 楼）
- 现状：**中文社区明确表达了需求和心理价位，但无人满足。** 委托方的技术选型恰好天然满足这三角。
- ⚠️ 但缺移动端 —— 中文用户对移动端重视度高于英文社区。

---

## 7. 对委托方假设的裁决

### 7.1 逐条裁决

#### ✅ 真需求：「AI agent 改完代码后，任务状态与现实脱节」
**证据权重：强（★★★★★）**

- **量化证据**：27% ghost rate（DEV.to，2026-08）、50-75% 会话浪费（Claude Code #39961）
- **结构性论证**：Nick Meinhold 的「create 可靠 / complete 有损」模型，解释了为什么这**不是偶发 bug 而是系统必然**
- **官方复现**：Claude Code 仓库至少 4 个重复 issue，其中一个被用户标为 Critical
- **有人已动手**：backlog.md-drift 存在本身就是需求验证
- **独立来源**：HN、GitHub Issues、DEV.to、个人博客、arXiv 论文五类来源互相印证

**判定：这是本次调研中被验证得最扎实的一点。委托方的核心洞察成立。**

#### ✅ 真需求：「本地 TODO.md / markdown 为事实源」
**证据权重：强（★★★★☆）**

- HN 1402 分主贴代表纯文本派的规模
- Backlog.md 6.5k stars、191 个 release、上过 Devoxx 和 AI Engineer Summit
- 「`.md` files aren't documentation anymore. **they're the protocol layer between humans and agents**」
- 「when context gets compacted, the on-disk file survives」——有明确的技术必要性

**扣分项**：
- 并发写入丢数据是**真实技术债**（N7），必须在架构层解决（文件锁 / mtime 检查 / 单写入者代理）
- 「不要污染主仓库」必须可配置（N8）

#### ⚠️ 半伪需求：「Tauri 看板 GUI」
**证据权重：反面证据强（★★★★★ 反对）**

这是**最应该被质疑的一环**：

0. **⭐ 最直接的打击**：一个**与委托方定位几乎完全相同**的产品（本地 markdown-per-ticket + YAML frontmatter + 看板 + agent 实时编辑 + 无账号无订阅）在 r/ClaudeAI 拿到 135 分，但**最高赞评论（74 分）是**：「**Everybody and their grandma has made their own Claude's task list.**」并附带闭源安全质疑（https://www.reddit.com/r/ClaudeAI/comments/1v3aky6/ ，2026-07-24）。**这说明该形态已经饱和到「再做一个会被嘲讽」的程度。**

1. **最接近的竞品已经死了**：Vibe Kanban（HN 195 分）于 2026-04-10 关闭公司，理由是「大多数是免费用户，找不到让我们兴奋的商业模式」
2. **看板对小任务是纯摩擦**：「creating a card, typing a description, assigning an agent... For 'change the button color' tasks, **this is absurd overhead**」
3. **看板不解决真问题**：「**It only provides a better view to watch the crash happen.**」
4. **重度用户根本不看板**：`ticket` 作者有 1900 个 ticket，但「**I don't delete these or really look at the `.tickets/` directory.** I navigate them through VS Code. Ctrl+t does fuzzy finding」（id=46509523）
5. **1001 个 todo app 警告**：「ToDo/productivity apps is a very tough market... struggled to get enough visibility to make it commercially viable, **despite a lot of rave feedback**」

**但不是完全伪需求**，有两条支持证据：
- 「**the visual feedback loop alone is worth trying.** Seeing three tasks done, one failed, and two in progress at a glance is something a terminal can't give you.」
- Backlog.md 用户：「seeing that visual representation **made something click**」；但同一位用户也指出「you have to remember to **refresh the page after your AI tool updates task**」——**连看板都会漂移**

**判定：看板应该是「附赠品」而非「主打」。把它当主打，就是在重走 Vibe Kanban 的路。**

#### ⚠️ 需要重新设计：「MCP 让 Claude Code/Codex 读写」
**证据权重：中（★★★☆☆），但形式存疑**

- **社区偏好 CLI 甚于 MCP**：
  - Backlog.md 作者：「**No mcp, just custom instructions**」，而这个方案让他达到 95%+ 成功率
  - 「I've **not had much luck in getting Claude to natively use MCPs**」（@unshavedyak）
  - 「Ooh, the idea of having **a CLI for the LLM** to mark off tasks without having to pollute context by reading the file over and over again is cool!」
  - beads 用 `bd` CLI；ticket 用 `tk` CLI；两者都不是 MCP-first
- **MCP 的固有问题**：「If you put it behind an MCP server, it becomes a matter of **ensuring the agent will invoke the MCP at the right moment, which is a whole challenge in itself**」（@stingraycharles）
- **更强的机制是 hooks**：Claude Code #39961 的社区共识是用 `SessionStart` hook 强制注入 `git log`，因为「**Git history and hook-injected context are harder to lose**」；@thelittleone 也提到用 hooks 防止 agent 无视任务系统

**判定：MCP 可以做，但不该是唯一入口。优先级建议：CLI ＞ hooks ＞ MCP。** 只做 MCP 会踩中 N10（agent 随项目变大而无视工具）。

#### ✅✅ 真需求且是最强差异化：「同步事实」按钮
**证据权重：中强（★★★★☆）**

这是委托方方案中**最有价值、也最被证据支持**的一环：

- 直接对应 N1，且有 Nick Meinhold 的理论背书：「**Stop trusting self-reported completion and verify against reality**」
- 对应 Claude Code #39961 的明确诉求：「**Startup awareness** — before starting planned work, the agent should automatically check `git log`... **This is trivial to implement and would prevent the most egregious duplication.**」
- 对应 backlog.md-drift 已验证的产品方向

**但有三个必须注意的陷阱**：
1. **不该只是按钮**：「I would be tempted to make the reconciler **run before every planning pass, not as cleanup afterward**」
2. **不能只做 line diff**：「**A line diff tells you what changed. It doesn't tell you what's missing.**」（agent 会大量改写措辞，diff 会被噪音淹没）
3. **分层做**：backlog.md-drift 的「结构性检查（确定性、零误报）+ 语义检查（LLM、可选）」两层设计是经过验证的正确形态。**不要一上来全用 LLM 判断。**

### 7.2 综合裁决

**痛点是真的，而且比委托方描述的更严重、更结构化。**

委托方说「看板漂移、过时、最后没人看」——社区证据表明，真实情况是：

1. **不只是「没人看」，而是「agent 在读，且被误导」**。陈旧任务会被 agent 当作活任务重新执行，浪费 50-75% 的会话时间，并持续烧 token。**这把问题从「体验问题」升级为「成本问题」和「正确性问题」，这是更好的产品叙事。**
2. **漂移有结构性成因**，不是纪律问题：agent 天然「开任务可靠、关任务有损」。所以「让大家记得更新看板」这种方案注定失败，**必须靠外部事实对账**。

**哪部分是伪需求 / 高风险**：

| 组件 | 风险 | 建议 |
| --- | --- | --- |
| Tauri 看板 GUI | 🔴 高 —— 最接近的竞品已倒闭，且被指为「纯摩擦」 | 降级为可选视图，不作为核心卖点；重度用户用编辑器 + 模糊搜索导航 |
| MCP 作为唯一集成方式 | 🟡 中 —— 社区实证 CLI 更可靠 | CLI 优先，hooks 保证不被忽略，MCP 作为补充 |
| 单一 TODO.md 单文件 | 🔴 高 —— 并发丢更新 + agent 编辑大文件困难 | 每任务一文件（Backlog.md 路线）或加锁；任务粒度「一个 PR 大小」 |
| 只做单仓库 | 🟡 中 —— 跨仓需求被明确提出且无人满足 | 早期就设计成可跨仓（N4，这是差异化机会） |
| 「同步事实」 | 🟢 低 —— 需求真实 | **提升为核心卖点**，改为自动/plan 前触发，分结构性 + 语义两层 |

**⚠️ 一条来自中文社区的、可能致命的警告**：

苏江关于 Spec coding 的结论 ——「最后导致 Spec 文档和实际代码脱节，**变成新的形式主义**」—— 与 V2EX 楼主试用 AI 维护 todo 后的反馈 ——「愿望很美好，之前实际试过一下发现**总是要这里改一下那里改一下太耗精力了**」—— 共同指向同一个死因：

> **如果「保持同步」本身需要用户操心，用户就会放弃这个工具。**

这意味着「同步事实」**不能是一个需要人点的按钮**。按钮意味着「用户要记得点」，而这正是原始痛点（「看板没同步」）的翻版 —— 只是把「记得更新看板」换成了「记得点同步」。

**正确形态应该是**：
- 挂在 `SessionStart` / plan 前 / pre-commit hook 上自动触发（英文社区共识）
- 结构性检查零误报、毫秒级，不打扰用户
- 只有语义层面的疑似漂移才提示人确认
- 按钮作为**兜底手动入口**保留，但不是主路径

**建议的产品重心调整**：

> 从「**本地 TODO.md + 看板 + MCP，附带同步功能**」
> 调整为「**让 AI agent 的任务状态与代码事实保持一致的对账引擎，附带一个看板视图**」

理由：
- 「本地 markdown 任务管理」赛道已极度拥挤（第 4.5 节列了 12+ 个工具），且有 6.5k stars 的既有赢家
- 「看板」赛道刚死过一个明星产品
- **「对账 / 漂移检测」赛道只有一个第三方插件**，且是所有证据链中最痛、最可量化的那一环
- N5（「我现在有 15 分钟该做什么」）、N4（跨仓）、N3（promise tracking）都是**被明确表达但无人满足**的相邻机会

### 7.3 证据权重与局限说明

**强证据**（多源独立、有量化、有时间跨度）：
- todo 漂移的存在与成因
- 纯文本/markdown 方向的正当性
- 看板层的商业失败

**中等证据**（单一来源但高质量，或多来源但无量化）：
- MCP vs CLI 的优劣
- 跨仓库、依赖关系等缺口

**本次调研的局限（如实说明）**：

**渠道可用性**
1. **Hacker News：完全可用**。通过 Algolia API 做了全文检索与完整评论树抓取，本报告 HN 引文均为**一手原文**（含分数、日期、评论 id）。
2. **Reddit：直连被网络层硬拦截**（`curl` 对 reddit.com / old.reddit.com / api.reddit.com 的 `.json` 端点均返回 **HTTP 000**，非 403/429，换 UA 与重试无效；`r.jina.ai` 与公共镜像同样被挡；Firecrawl 明确拒绝该站点）。
   - **绕行方案**：发现 `https://reddit.sentinel-team.org/posts/<post_id>/snapshots/<时间戳>` 存档镜像可直连，**含完整正文 + 评论 + 用户名 + 票数**。
   - **因此本报告的 Reddit 引文分两级**：标注了镜像 URL 与票数的（如 A-1、「done 是声明」串、135 分看板帖、72 分 SDD 帖）为**已验证一手全文**；仅有 reddit.com URL 而无票数的为**搜索引擎索引片段**，建议二次核验。
3. **V2EX：官方 API 已失效**（`api/topics/hot.json` 超时、`show.json` 返回空 body；V2EX 已关停 v1 公开 API），`r.jina.ai` 代理亦被挡。中文素材改由搜索引擎索引获取，其 highlights **直接带 V2EX 楼层原文与楼层号**，可信度尚可，但**未逐条打开原页面核验**。
4. **X/Twitter：不可用**。`agent-reach doctor` 显示无完整凭据（`active_backend: null`）；`x_search` 明确返回「X was unreachable; a web search answered second-hand」。**故本报告未采信任何 X 平台证据。**
5. Exa MCP 在调研中途触发免费额度限制，后续改用其他检索通道。

**证据质量提示**
6. **已排除可疑聚合站内容**：部分二手聚合站（如 funtasking.com）提供「无 URL 的高赞 Reddit 引文」，因无法验证真实性，**已从本报告剔除**。opiniondeck.com、saasprobe.com 等提供了用户名 + 票数 + 日期，可信度较高但仍属二手，已在文中标注。
7. **Backlog.md 的中文与 Reddit 声量结论需软化**：三方独立检索均未找到 Backlog.md 的 Reddit 讨论与中文原生评价。准确表述应为「**在本次检索能力下未能找到**」，而非断言其不存在。但同类目产品（135 分帖）的社区反应可作为该赛道温度的替身证据。
8. **Task Master AI 的中文推广文数据（完成率 70%→95% 等）为作者自报，无法独立验证**，本报告仅列为「社区吹捧样本」。
9. 部分来源日期为 2026 年，系检索引擎返回的原始发布时间，未逐条二次核验。

**覆盖缺口**
10. r/emacs（org-mode）、r/Trello、r/selfhosted、r/todoist、r/ObsidianMD 中除已引用者外，未取得更多可用的一手引文。
11. 未做定量抽样（如「N 个帖子中有 M 个提到漂移」），本报告的「普遍度」判断基于**跨平台独立复现**与**票数/评论数**两个代理指标，而非统计抽样。
