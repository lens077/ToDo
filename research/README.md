# AI 时代 todo 工具:深度调研与构想评审报告

> 调研日期:2026-08-30(各子报告标注为准)。方法:5 个并行调研 agent + 主 agent 一手核查,共 400+ 条来源 URL,一手资料(官方文档、源码、issue 原文、实测)优先。
> 委托方构想:本地 TODO.md 为唯一事实源 + Tauri 看板三端 + MCP/skills 供 Claude Code/Codex 读写 + 「同步事实」按钮 + git 导入 + 注释扫描 + curl CRUD API + ntfy 通知 + 中文默认 + 高性能(<2s 启动)+ 商业化。

## 报告导读

| 文件 | 内容 | 规模 |
|---|---|---|
| 本文 | 综合裁决、构想评审、定位/架构/商业化建议 | 主报告 |
| [a-ai-native-agent-task-tools.md](a-ai-native-agent-task-tools.md) | Backlog.md、Vibe Kanban、Task Master、beads、Shrimp、内置 todo、雷同度总表 | 812 行 / 66 来源 |
| [b-markdown-local-first-tools.md](b-markdown-local-first-tools.md) | imdone、Obsidian 双插件、todo.txt、org-mode、Taskwarrior;TODO.md 九大坑;可借鉴设计 | 1023 行(含源码级分析) |
| [c-commercial-mainstream.md](c-commercial-mainstream.md) | Linear/Height/Todoist/滴答清单/Things/monday 等;MCP 生态;定价与商业化 | 723 行 / 196 来源 |
| [d-selfhosted-ntfy-tauri.md](d-selfhosted-ntfy-tauri.md) | 自托管看板、ntfy 机制与安全、Tauri 可行性实测、推荐架构草案 | 1229 行 / 95 来源 + 2 附录 |
| [e-community-voices.md](e-community-voices.md) | HN/Reddit/V2EX 真实声音;痛点量化;逐条裁决;12 条未满足需求 | 1177 行 / 94 来源 |
| [f-firsthand-monday-syncfacts-cat.md](f-firsthand-monday-syncfacts-cat.md) | monday.com 公开资料分析;「同步事实」竞品;cat 底座评估 | 主 agent 一手 |
| [g-decisions-addendum.md](g-decisions-addendum.md) | **决策补遗**:去 MCP 用 CLI、Obsidian/VS Code/IDEA 插件策略、部署形态、付费功能评审、修订版 MVP | 委托方决策后新增 |

---

## 1. 执行摘要:十条核心结论

1. **你的核心痛点被强验证,且比你描述的更严重。** 「todo 漂移」不只是「没人看」——agent 在读且被误导:实测 27% 的任务是「幽灵任务」(代码已完成、状态未更新),Claude Code 官方 issue 实测 50–75% 的会话浪费在重做已完成的工作。漂移有结构性成因(「create 可靠、complete 有损」「不是纪律问题,是距离问题」),靠自觉更新注定失败,必须外部对账。(证据:e §1)
2. **存在雷同工具,重合度最高的是 [Backlog.md](https://github.com/MrLesk/Backlog.md)**(6,573★、月下载 5.5 万、周更、MIT 免费):markdown 任务进 git + 终端/Web 看板 + 20 个 MCP 工具。但它没有:移动端、推送、中文(i18n PR 被关闭)、代码注释导入、跨仓库视图、**同步事实**。(a §1、§9)
3. **「同步事实」是全类目真空,是你最硬的差异化。** 逐条核查 Backlog.md/Task Master/beads/Shrimp/ccpm 的命令面与源码,无一实现「扫代码反向校正任务」;只有零散 CLI/技能级先例(KEEL、backlog.md-drift、reconcile 设计稿)。(a §9.3、f §2)
4. **但「同步事实」不能只是按钮。** 按钮=「用户要记得点」,是原痛点的翻版。社区共识与中文社区的失败经验(「变成新的形式主义」「总是要这里改一下那里改一下太耗精力」)都指向:挂 SessionStart/plan 前/pre-commit 自动触发,结构性检查零误报、语义检查 LLM 提议 + 人确认,按钮只作兜底。(e §7.2)
5. **看板是你构想中最弱的一环。** 最接近的明星竞品 Vibe Kanban(27,953★,已上线「个人免费 + 团队 $30/人/月」定价)于 2026-04-10 关停,创始人:「绝大多数是免费用户,找不到商业模式」;r/ClaudeAI 上同定位产品的最高赞评论是「Everybody and their grandma has made their own Claude's task list」。看板应是附赠视图,不是主打卖点。**更关键的死因细节:真正的社区决裂发生在 2026-02 —— 团队把看板搬上云并强制登录,摧毁了用户最珍视的「本地、离线、无需账号」(issue #2687,33👍),两个月后关停;关停公告在 HN 仅 4 分 0 评论,社区早已散尽。教训:云端能力只能是可选增量,绝不能替换本地路径。**(a §2、§10;e §4.3、§4.5)
6. **MCP 已是入场券而非卖点,且不该是唯一入口。** Linear/Todoist/滴答清单/Notion/Asana/Trello/monday/ClickUp 均已有官方 MCP;但全部是「云端 SaaS 暴露 API」,**无一家是「agent 直接读写本地文件事实源」**——这才是你的空隙。同时社区实证 CLI > hooks > MCP(Backlog.md MANIFESTO 明写「MCP is a legacy, optional adapter」「The CLI is canonical」)。(c §1.1、§12.1;a §9.2)
7. **「TODO.md 为事实源」成立,但工程上有九个坑,且单文件是最大架构风险。** Obsidian Kanban(全文件重序列化→静默毁内容→作者弃坑)vs Obsidian Tasks(行级就地改写→下载量高 58%、健康存活)是同生态对照实验;Task Master 单 JSON 实测已膨胀到 1.5MB。保真回写唯一正解:pulldown-cmark 取字节偏移、外科手术式改写、绝不重新序列化(实测 cmark/comrak 均会破坏内容)。(b §9;a §9.2;d Part 3)
8. **Tauri 桌面可行且有生产级先例;移动端「证据不支持」,需先 spike。** sidecar 在移动端完全不可用(MCP 必须用 rmcp 编译进主二进制);iOS 沙盒里没有用户的 git 仓库,「本地 TODO.md 为事实源」在手机上不成立——移动端只能定位为同步副本消费者。启动 <2s 桌面端现实,但须在低端 Android 真机实测。(d Part 3)
9. **「AI 全自动维护任务」已有失败先例,「AI 提议 + 人确认」是行业共识方向。** Height 2.0(autonomous project management)从全量发布到宣布关停仅 5 个月;Microsoft Planner Agent 回撤到默认 draft 待批准。你的用户触发+确认定位是对的,应坚持并作为叙事。(c §3、§12.2)
10. **商业化必须是前置约束,不是事后附加。** 反例:Vibe Kanban 连团队 seat 费都试过并失败;正例:Things 3(买断制唯一幸存者)、beorg(开放格式+闭源体验+订阅,品类内唯一验证样本)。中文社区存在「本地数据+跨平台+不订阅=不可能三角」的明确未满足需求,心理价位「买断不超过 ¥100」。推荐:买断 + 12 个月更新窗口 + open-core + 区域定价。(c §13;a §9.4;b §10.3;e §5)

---

## 2. 你的构想还原(评审对象)

| # | 需求 | 原话要点 |
|---|---|---|
| R1 | 本地 TODO.md 为事实源 | 「以本地 md 为主,看板只是方便修改」,todo 随 git 提交 |
| R2 | 看板读取/导入/渲染 TODO.md,改动回写 | 「在 todo 工具里写 todo 时同步回写 TODO.md」 |
| R3 | MCP + skills 让 Codex/Claude 读取接入 | 「通过 mcp、项目约束和 agent 对应的 mcp、skills 实现强同步」 |
| R4 | 「同步事实」按钮 | 「让 AI agent 去查询实际情况和代码是否对齐和同步」 |
| R5 | GitHub/GitLab 导入 TODO.md;规则/注释扫描导入 | 「自动搜索包含 todo 的文件……注释里写着 TODO 的」 |
| R6 | 网站 + curl 的 CRUD | — |
| R7 | ntfy 第三方通知 | — |
| R8 | 国际化后置,中文默认 | — |
| R9 | 性能:启动 <2s,加载/修改零卡顿 | 「我对性能非常敏感」 |
| R10 | 在线 + 离线,SQLite | — |
| R11 | Web + PC + 移动,Tauri | — |
| R12 | 商业化 + 个性化功能吸引用户 | — |
| 参考 | cat 项目代码、monday.com 看板 | — |

---

## 3. 市场全景:四大流派与它们教你的事

### 3.1 AI 原生 / agent 任务工具(与你同赛道,详见 a)

| 工具 | Star(08-30) | 事实源 | 状态 | 与你的雷同度 |
|---|---|---|---|---|
| Backlog.md | 6,573 | 一任务一 .md,进 git | 周更,最活跃 | **高** |
| Vibe Kanban | 27,953 | 本地 SQLite | **公司已关停(2026-04-10)** | 中 |
| Task Master | 28,037 | tasks.json 单文件(实测 1.5MB) | 4 个月停滞,重心转商业版 | 中 |
| beads | 26,718 | **Dolt DB(已放弃文件进 git)** | 极活跃 | 中(理念对立) |
| ticket / beans | 881 / 909 | markdown 目录 | 中 | 中(「回归 markdown」同盟) |
| Shrimp | 2,150 | tasks.json | 已停更 | 低 |

教训:① 依赖图是全类目标配(agent 问「下一步做什么」的基础),你未提及,必须补;② beads 的复杂性引发反弹催生 beans/ticket,证明「简单 markdown」有真实需求,但它们都保留了依赖图;③ Anthropic 已内置过受 beads 启发的 Tasks 工具,但在新模型上默认关闭(上下文成本)——平台会吸收「会话内记忆」,**跨会话/进 git/可评审的项目事实源仍是第三方空间**;④ 多 agent ID 冲突要前置设计(beads 用 hash ID)。

### 3.2 Markdown/纯文本本地派(你的技术祖先,详见 b)

- **imdone**:12 年、唯一真正做过「看板改动回写源文件+注释扫描」的老牌产品,npm 月下载仅约 2 千;2025 年转向「给 AI 保留上下文」叙事,$18→$9 降价后 24 小时内拿到 2 个付费用户,作者复盘「Price is rarely the final boss」,真问题是 time-to-first-value。**它与你的「同步事实+MCP」同方向,会正面相遇,但已把重心移向 Jira 团队场景。**
- **Obsidian Kanban vs Tasks**:全文件重序列化 vs 行级就地改写的对照实验(见摘要第 7 条)。Kanban 已进 community-archive,**其 261 万被教育过「markdown 看板」概念的用户正在找替代品——现成冷启动人群**。
- **org-mode / Taskwarrior**:纯文本天花板与 CLI 标杆,共同证明「稳定 ID 是纯文本自动合并的分水岭」(orgzly 作者亲口承认回避 ID 导致移动端只能整文件覆盖;Taskwarrior 靠 uuid + tags 拆 key 实现自动合并)。Taskwarrior 靠捐赠维持到维护者力竭、3.0 以数据丢失事故收场——**可持续收入是对用户数据负责的前提**。
- **todo.txt**:规范最扎实但生态无商业层;TODO.md 没有事实标准(todomd/todo.md 是遗弃提案),**地基只有 GFM `- [ ]`,格式必须自定义并发布带版本号的 spec + lint**(否则 Claude 和 Codex 会各写各的方言——LLM 会自信地发明语法)。

### 3.3 商业主流(体验与商业模式的参照,详见 c)

- **Linear**:同步引擎(内存对象图+IndexedDB+事务队列,偏 OT)换来「读操作永不过网络」的零延迟,但 HN 工程师指出其代价是静默丢弃冲突输入。**你的机会:冲突显式提示而非静默覆盖。** Linear for Agents 让 agent 可被指派 issue 且不占席位——生态姿态值得学。注意一个常见误解:Linear 桌面端也是 Electron(官方 changelog 自述「with the Electron wrapper」),它与 Notion 同栈却一快一慢——**快慢的决定变量是「权威数据在本地还是云端」,不是框架**。
- **Height**:AI 全自动项目管理的失败先例(5 个月从发布到宣布关停,关停后用户数据删除)。
- **Todoist**:API 成熟+官方 MCP+官方 CLI,但云同步架构迫使它设「每项目 300 任务」硬上限——**本地 SQLite 无此限制,可直接当宣传点**。
- **滴答清单**:国内 ¥139/年 vs 国际 $49.99/年,区域定价是必修课;国内外双 MCP 端点已上线。
- **Things 3**:买断制唯一幸存者($64.97 三平台合计),证明「本地优先+单机价值」上买断可持续。
- **monday.com**(你给的参考对象,详见 f):Board→Group→Item→Subitem + Column 即 schema,Kanban 只是结构化数据上的投影视图(列=单一 Status 字段);多视图/自动化/AI Blocks(8 credits/动作)是平台化打法;性能是其软肋(官方开过 speed issues 专页;2025 年用户报告 <10 items 的视图吃 1.5GB 内存)。**可抄:视图即投影、列=状态字段、WIP 上限、divide-by;不可抄:重平台路线。**

### 3.4 自托管看板(反面参照,详见 d)

Vikunja/Planka/WeKan/Kanboard/Focalboard(已停维护)无一以文件为事实源——多人协作形态必然中心化 DB,商业模式建立在托管上。**它们解决不了「todo 与代码同仓、同分支、在 PR diff 里可见」——这正是你的位置。** Planka 的 fair-code(Sustainable Use)许可是「允许自托管自用、禁止他人做竞品 SaaS」的现成模板。

---

## 4. 痛点验证:「todo 漂移」证据链

来自互不相干作者、三个平台的独立论断构成骨架(详见 e §1):

| 论断 | 回答 | 来源 |
|---|---|---|
| 「Not a discipline problem, **a distance problem**」 | 为什么必然漂移 | Reddit r/AI_Agents |
| 「Creates are reliable, **completes are lossy**」(实测 27% ghost rate) | 为什么漂移是单向的 | DEV.to 实测,2026-08 |
| 「**Done is a claim** that must be reconciled, not a message from the agent」 | 唯一解法 | Reddit,6+ 人独立附和 |

量化:Claude Code 官方 issue #39961 实测 50–75% 会话浪费在重做已完成工作;r/cursor 97 评论帖:「agent 会把新代码改回废弃模式,因为它信文件胜过信代码」;「模型把三周前的 TODO 当成当前意图」。**叙事升级:这不是体验问题,是成本问题(烧 token)和正确性问题(改坏代码)。**

---

## 5. 构想逐项评审

| # | 构想 | 裁决 | 关键修改 |
|---|---|---|---|
| R1 | TODO.md 唯一事实源 | ✅ 成立(★★★★☆) | 必须:稳定 ID、行级外科手术回写、原子写+advisory lock、归档策略;单文件先限定中小项目,预留分片路径;发布格式 spec+lint |
| R2 | 看板双向同步 | ⚠️ 降级为「附赠视图」 | 不作为主卖点;必须补「今天/现在该做什么」时间视图(org-super-agenda ★1,535 > org-kanban ★267);实时 watch 不许要求手动刷新 |
| R3 | MCP + skills | ⚠️ 重排优先级 | **CLI(canonical)> hooks(保证不被忽略)> MCP(补充)+ skills**;工具少而深,严控 token(Task Master 44 工具吃 21K token 是反例) |
| R4 | 同步事实按钮 | ✅✅ 升级为核心卖点 | 改为自动触发(SessionStart/plan 前/pre-commit),两层设计:结构性检查(确定性、零误报、毫秒级)+ 语义检查(LLM 提议+人确认);状态只升不降;按钮保留作兜底。**降本增信的关键设计——任务↔物证绑定**(借鉴 Vibe Kanban):任务记录对应的 commit sha 范围+文件路径+PR 状态,对账就从「让 agent 重新理解代码」降级为「比对物证与当前 HEAD」,可信度与成本好一个量级;agent 完成后进 InReview 而非直接 Done |
| R5 | git 导入 + 注释扫描 | ✅ 差异化成立 | 注释 TODO 导入必须回答「双事实源」问题:借鉴 todo-to-issue 闭环(注释→任务→回写 ID→注释删除→任务关闭);GitHub/GitLab issue 集成**不做双向镜像,做单向投影+一条窄回流**(Backlog.md 主动不做 issue 同步、把 git 当同步层,是强反向信号),URL 回写进文件让映射表就是文件本身;谨慎索要 GitHub 权限(Vibe Kanban 被抵制先例) |
| R6 | 网站 + curl CRUD | ✅ 差异化成立 | OpenAPI 3.1 文档化;Web 端做瘦客户端(不做 wasm SQLite 第三真源) |
| R7 | ntfy 通知 | ⚠️ 修正范围 | ntfy 作一等可选而非唯一:Delay 上限 3 天(不能当提醒调度器,自建调度);topic 读写同权(泄漏=可伪造注入),**topic 绝不能写进项目目录**(git push 即公开);iOS 正文不过 ntfy.sh(源码已验证,可安抚隐私顾虑);建议自建 APNs/FCM 为主 + ntfy/Bark 可选(Bark 覆盖中国 iOS 用户) |
| R8 | 中文默认 | ✅ 真空位 | Backlog.md i18n PR 被关闭、全类目英文;中文社区诉求明确(本地数据/买断/微信生态) |
| R9 | 启动 <2s、零卡顿 | ✅ 可行且可营销 | 竞品背景:Notion 冷启动 6.2s、ClickUp 看板 8s、monday 1.5GB 内存;但 Linear(Electron)靠本地权威数据做到零延迟——**「用了 Tauri」不会自动变快,兑现 <2s 靠架构纪律**:SQLite 投影首屏秒开+后台增量校验;增量解析(250ms→<1ms 先例);低端 Android 真机实测进 CI |
| R10 | 在线+离线,SQLite | ✅ 定位明确 | SQLite=纯投影、可随时重建,不是第二事实源;单机不引入同步引擎(文件 mtime+hash+outbox 足够);多设备再上 Loro CRDT;git 定位为历史/备份而非同步机制 |
| R11 | Tauri 三端 | ⚠️ 分阶段 | 桌面 Phase 1(低风险,GitButler/Yaak/Hoppscotch 先例);Web Phase 2(瘦客户端);移动 Phase 3 先 spike(Spacedrive 最终选 RN;sidecar 不可用→MCP 用 rmcp 编译进主二进制;移动端=同步副本消费者,不是事实源持有者) |
| R12 | 商业化 + 个性化 | ⚠️ 前置约束 | 见 §7;个性化见 §8 |

### 必须正视的四条风险(均有实证)

1. **并发写入丢数据**:GUI、watcher、curl API、MCP、agent 五个写入者并存,比 Obsidian 场景更复杂。对策:单一写入队列串行化、写前 mtime+hash 乐观锁、原子写、自写忽略、冲突标记优雅降级。(b §9 坑 3)
2. **误报即死刑**:beads 用户「I've banned my agents from running `bd doctor`, because it reports false positives」。同步事实的结构层必须零误报,语义层必须可关。(e)
3. **维护成本必须低于收益**:「Spec 文档与代码脱节,变成新的形式主义」(中文社区)。产品要主动声明边界:小 bug 修复、两文件以内的改动不值得走流程。(e §7.2)
4. **上下文预算**:Spec Kit 752 行指令=每会话 33K token 的教训;你的 agent 指令面要按 token 预算设计。(e)

---

## 6. 推荐产品定位

> 从「本地 TODO.md + 看板 + MCP,附带同步」
> 调整为「**让 AI agent 的任务状态与代码事实保持一致的对账引擎,附带看板视图**」。

一句话定位(建议):**给 AI 协作开发者的任务对账器 —— todo 永远在你的仓库里,永远和代码对得上。**

三层价值主张:
1. **事实源在你手里**:任务是仓库里的纯文本,git diff 可见、PR 可审、无锁定,**厂商死了数据还在**(对照已发生的事实:Height 关停删数据、Vibe Kanban 云端看板随公司一起失效;markdown-in-repo 随仓库天然存活)。
2. **agent 读写不漂移**:CLI/hooks/MCP 三入口 + 稳定 ID + 结构化增量查询,陈旧任务不再误导 agent、不再烧 token。
3. **对账代替自觉**:自动漂移检测 + AI 提议 + 一键确认(「AI 提议,你确认」是行业已验证的正确姿态)。

与关键竞品的一句话差异:
- vs **Backlog.md**:它是 CLI+本地 Web、英文、无移动端无推送无对账;你是 GUI+移动+推送+中文+对账引擎。
- vs **beads**:它把数据搬出文件进 Dolt;你坚持「人可读、进 git、PR 可审」,用 SQLite 投影补查询能力。
- vs **手写 TODO.md + 让 AI 读**(零成本真对手):你提供对账、提醒、多端、结构化查询、可视化——「省下的 token 和重做的会话」就是价格锚。

---

## 7. 商业化建议(综合 a/b/c/e)

- **模式**:一次性买断 + 12 个月更新窗口(fair perpetual,到期软件永久可用)。可照抄的成熟范式:Sublime(一次性 + 3 年更新 + 一个 key 通用所有设备与系统)、JetBrains(订阅满 12 个月得永久 fallback 授权)。国内 ¥98–148(社区心理价位「不超过 100」)、国际 $29–39;续更 6 折。**不做订阅**(除非提供真实服务端价值),**不做 lifetime deal 促销**;**定价模式一开始就要选对**——Kaleidoscope 买断转订阅引发的是「背叛」叙事,开发者用户当场迁移。
- **另一个可能更贴切的对标是 Obsidian 模型**:本体免费(含商业用途)建立最大装机量,收入来自 Sync/Publish 这类「本地优先自己做不到的事」。对应到你:GUI+CLI 免费拉规模,收「跨设备同步中继 + 移动推送 + 团队空间」的订阅,与买断制二选一或组合(买断客户端 + 订阅同步服务)。
- **免费层做规模阉割而非功能阉割**:单仓库/单看板免费、完整体验核心闭环(缩短 time-to-first-value 是 imdone 用 12 年换来的教训)。
- **open-core 防守免费竞品**:开源 CLI + md 解析/对账内核(建立 agent 生态信任),收费卖 GUI 桌面+移动+推送+跨设备同步。许可优先研究 fair-code/Sustainable Use(Planka/n8n 模板)而非纯 MIT 或 AGPL。
- **付费点必须落在「必须有服务端/精品体验才成立」的能力上**:跨设备同步中继、移动推送、精品 GUI。**不要指望团队 seat 费**(Vibe Kanban 用一家公司证伪)。beorg 是唯一验证样本:开放格式 + 闭源精品体验收费。
- **一条铁律:不登录也能完整使用本地功能。** HTTP API、ntfy、云同步、商业化全部必须是可选增量——Vibe Kanban 2026-02 强制登录上云,社区当场决裂,两个月后关停。
- **渠道**:自主分发为主(Tauri 可自更新,桌面端不依赖商店);国际走 Lemon Squeezy(内建 license key API,5%+$0.50);国内微信/支付宝 + 爱发电起步;移动端走商店(updater 插件移动端不可用)。
- **叙事四板斧**:「数据永远是你仓库里的纯文本」/「AI 提议,你确认」/「没有 300 条任务上限」/「买断,不订阅」。

---

## 8. 个性化与增长建议

1. **cat 桌宠联动(独有资产)**:宠物状态反映任务/专注状态、到期举手提醒、完成庆祝动画。游戏化生产力有真实市场(Forest 2025 年 60M+ 用户;桌宠番茄钟在社交媒体自传播)。这是竞品没有的辨识度,且 cat 的事件网关/ntfy/hook 桥全部现成。
2. **「我现在有 15 分钟,该做什么」**:被社区明确表达但无人满足的需求(e N5),结合依赖图+预估粒度做推荐,差异化成本低。
3. **跨仓库统一视图**(e N4):早期就把数据模型设计成多仓,Backlog.md 被直接问倒的点。
4. **每日站会卡片**:cat 已有早报聚合(天气+TODO+git log+番茄),迁移过来即「AI 生成的今日作战图」,天然的分享传播物。
5. **冷启动人群**:Obsidian Kanban 弃坑留下的 261 万下载存量用户;HN/Reddit 纯文本派;中文社区「不可能三角」人群。

---

## 9. 技术架构要点(详见 d 架构草案 + b 坑清单)

```
用户 Git 仓库:TODO.md(唯一事实源)
      │ notify + debounce + hash 防回环
Rust Core Crate(一份代码四处复用)
  ├─ md-engine:pulldown-cmark 取字节偏移 → 外科手术改写 → 原子写 + advisory lock
  ├─ index:SQLite(rusqlite)纯投影,可随时重建 —— 首屏秒开靠它
  ├─ reconcile:同步事实引擎(结构层确定性检查 + 语义层 LLM 提议)
  └─ notify-out:APNs/FCM(主)· ntfy · Bark
入口:Tauri 桌面(全功能)/ Tauri 移动(同步副本消费者)/ MCP(rmcp 内嵌,非 sidecar)/ CLI(canonical,--json)/ 可选自托管服务端(REST + Web 瘦客户端)
```

十条硬约束(每条都有实测/实证支撑,出处见 d/b):
1. 绝不重新序列化整个文件(cmark 726→702、comrak 注入合成注释,实测);
2. 每任务稳定 ID(`<!-- id:xxx -->` 或行内标记),这是合并与 agent 精确改写的分水岭;
3. 语义挂稳定枚举不挂显示文本;日期写死 ISO 8601,不做配置;
4. 多值字段用可独立增删 token(`#a #b`),拖动排序用分数式 order(一卡只改一行);
5. 纯 UI 状态(列折叠等)绝不入文件;
6. 所有写入者(GUI/CLI/MCP/API/agent)共用同一写队列 + advisory lock;
7. 往返测试进 CI:「未触碰行多重集不变」断言 + 真实脏语料 fuzz;
8. 冲突标记(`<<<<<<<`)检测→只读模式+明确提示,不崩溃不误解析;
9. MCP 用 rmcp 编译进主二进制(sidecar 移动端不可用、影响签名);
10. ntfy topic 存 `~/.config/`,绝不进项目目录;
11. Tauri updater 更新签名强制且不可关闭,**私钥丢失 = 永远无法再推送更新**——私钥备份策略第一天就要定;Windows 分发注意 EV 证书自 2024 起不再即时绕过 SmartScreen。

分阶段:Phase 1 桌面三端 + CLI + MCP + 对账引擎(低风险)→ Phase 2 Web 瘦客户端 → Phase 3 移动端先 spike 再定框架(Tauri Mobile 证据不支持,Spacedrive 选了 RN)。
必做 spike:低端 Android 冷启动实测、移动端产品形态、保真回写属性测试、单文件团队伪冲突率、Linux WebKitGTK、国产 ROM WebView。

cat 可复用:Tauri2+Vue3 壳与常驻网关模式、/todos CRUD 原型、ntfy 部署与 HMAC 一次性 token 回程、claude-hook/codex-notify 安装器、多端结构(desktop/pwa/mobile/site)。注意:cat 以 SQLite 为事实源,与新工具方向相反,存储层不能照抄。

---

## 10. MVP 建议(验证核心假设的最小闭环)

1. `todo init`:任意仓库零配置生成 TODO.md(约定格式 v1 + lint);已有 TODO.md 立即渲染(time-to-first-value < 30 秒)。
2. Rust core:解析 + 保真回写 + SQLite 投影 + watch。
3. CLI(canonical):`todo list/add/done/next --json` + hooks 注入指引(CLAUDE.md/AGENTS.md 一行接入)。
4. **对账引擎 v1**:`todo reconcile` — git log/diff + 代码检索取证 → 分级报告(确凿自动改 / 疑似列表人确认)→ 桌面端一键确认 UI。
5. Tauri 桌面看板 + 「今天」视图(读 SQLite 投影,写走同一队列)。
6. MCP(rmcp 内嵌,≤8 个工具)+ Claude Code skill。
7. ntfy/Bark 到期提醒(复用 cat 链路)。
先发这七件,验证「对账」留存,再谈 Web/移动/同步服务。

---

## 11. 调研方法与局限

- 方法:5 个并行子调研(a–e)+ 主 agent 一手核查(f);一手资料优先(官方文档/源码/issue/实测);Exa 免费额度中途耗尽后改用 web_search/curl/gh CLI/存档镜像。
- monday.com 实机看板需登录,经委托方确认改用公开资料。
- X/Twitter 无凭据,未采信任何 X 一手证据;Reddit 部分引文来自存档镜像(已分级标注)。
- 各子报告均含「未能核实」清单(合计 40+ 条),引用时请注意标注;商业化建议属推断,非经验证的市场结论。
