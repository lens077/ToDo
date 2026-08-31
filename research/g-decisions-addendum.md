# 决策补遗:去 MCP、Obsidian/IDE 插件、部署形态、付费功能评审

> 日期:2026-08-30。性质:委托方读完部分主报告后的方向决策,与对应的评审/实施建议。证据引用自 a–f 各子报告与本轮补充核实。

## 0. 本轮决策记录(委托方)

| # | 决策/新需求 | 评审结论 |
|---|---|---|
| D1 | v1 去掉 MCP,用 CLI 代替 | ✅ 赞同,留一个低成本后门 |
| D2 | 做 Obsidian 插件,覆盖不用 AI 的纯记录用户 | ✅ 强赞同,恰好接住 261 万存量真空 |
| D3 | 考虑 VS Code / JetBrains(IDEA)插件 | ✅ 可行,实现成本可控(插件皮 + 同一内核) |
| D4 | 部署:单二进制 / docker compose / k8s(后续) | ✅ 架构天然支持,注意许可证防蚕食 |
| D5 | 付费:托管 LLM/API 识别整理 TODO | ✅ 成立(「收本地做不到的事」),有红线 |
| D6 | 付费:随想(快捷键 + 空白画布 + 快速记录) | ✅ 好方向,建议 v1 做纯文本速记,画布后置 |
| D7 | 付费:日/周/月/年 + todo 看板视图 | ⚠️ 方向对,免费/付费线已按边际成本重画(§5.3) |
| D8 | 存储:A 起步;瘦身时按分类拆分 `todos/` + 链接引用 + 链接校验 | ✅ 已定稿(§7.1) |
| D9 | 闭源、私有仓库;插件市场分发 | ✅ 四渠道全部可行,Obsidian 有官方闭源路径(§7.2) |
| D10 | 移动端:Android 主力、PC 主战场、局域网免费/云中继付费 | ✅ 已定稿,P1 可提前(§7.3) |
| D11 | URL 收藏 + HTTP 预览 + 浏览器插件 | ✅ 已评审入档(§7.4) |

---

## 1. D1:去 MCP、CLI 代替

**赞同。** 依据已在主报告 §5 R3:社区实证 CLI > hooks > MCP;Backlog.md 作者「No mcp, just custom instructions」达到 95%+ 成功率;Task Master 44 个 MCP 工具吃 21K token 是反面教材。你不熟 MCP 也不构成损失——对 Claude Code/Codex,CLI + 一行接入指引已是被验证的最优形态。

实施要点:
1. **CLI 即产品的 canonical 接口**:`todo list/add/done/next/reconcile`,全部支持 `--json` 稳定输出 + 稳定 exit code(agent 与脚本共用)。
2. **agent 接入 = 文档而非协议**:`todo init` 时生成/追加 CLAUDE.md、AGENTS.md 中的一段接入指引(≤30 行,控制上下文预算);另发布 Claude Code skill 版本。
3. **hooks 保证「不被忽略」**:提供 `todo hooks install`(参考 cat 的 `bridges/claude-hook` 安装器),在 SessionStart 注入当前任务快照与对账摘要——这是解决「agent 无视任务系统」(e §N10)的关键,比 MCP 更可靠。
4. **后门零成本保留**:CLI 的 `--json` 面本身就是未来 MCP/HTTP 的 schema。若某天某个生态只认 MCP,用 rmcp 给 CLI 套一层薄壳即可(d 报告已论证 rmcp 内嵌路线),不需要现在做。

---

## 2. D2:Obsidian 生态答疑与集成策略

### 2.1 你问的「Obsidian 是否有 TODO 类插件或内置」——直接回答

- **内置**:Obsidian 原生支持 GFM checkbox(`- [ ]` 渲染为可勾选框),但仅此而已——无日期、重复、查询、聚合。官方新核心功能 Bases(数据库视图)截至调研日**尚不支持任务聚合**([社区正在请求](https://forum.obsidian.md/t/bases-support-for-tasks/103074)),任务管理仍是插件领地。
- **Obsidian Tasks 插件**(约 413 万下载,活跃):事实上的任务标准插件。行内 emoji 元数据(📅 截止、🔁 重复、🆔 ID、⛔ 依赖)+ `tasks` 查询块生成动态视图;写入哲学是「行级就地改写 + 定位失败就放弃并告警」——正是 b 报告推荐你采用的那套。
- **Obsidian Kanban 插件**(约 261 万下载,**已弃坑**):md↔看板双向同步,但全文件重序列化毁数据(issue #1082)、600 open issues、作者公开找接盘无人应征、仓库已进 community-archive。
- 其余:Day Planner、Checklist、Task Board 等小插件,无一统治级。

**结论:Obsidian 有强烈且被验证的 todo 需求(两插件合计 670 万+ 下载),看板位置正空着。你的插件方案不是「挤进红海」,是「接盘一个弃坑的头部插件」。**(b §5、§10.3)

### 2.2 集成策略建议

- **格式兼容优先**:任务语法尽量兼容/映射 Obsidian Tasks 的行内元数据(它是存量用户的肌肉记忆);看板文件约定(列=`## 标题`、卡片=`- [ ]`、配置藏 `%% %%` 注释)直接沿用 Kanban 插件的约定——迁移零成本,还能反向吸收其存量用户。
- **技术路线:Rust 内核编译 WASM 进插件**(方案 A,推荐):Obsidian 插件是 TS/JS,把 md-engine(解析+保真回写)用 wasm-pack 编译进插件,保证「同一份回写逻辑」跨 App/CLI/插件——避免出现两套解析器方言(b §坑 6 警告过 LLM 和多实现都会发明方言)。桌面插件可另探测本机 CLI 以调用对账等重功能(方案 B 作增强)。
- **附带红利:Obsidian mobile 即你的过渡移动端。** 移动端原生 app 被 d 报告判为「先 spike」,而 Obsidian 手机版 + 你的插件(WASM 无进程依赖)+ 用户自己的 Obsidian Sync/iCloud,就是零成本的移动查看/轻编辑方案——正好填 Phase 3 之前的空窗。
- **商业化合规**:Obsidian [允许付费插件](https://www.reddit.com/r/ObsidianMD/comments/1rdkf5k/can_you_publish_a_paid_obsidian_plugin/)(license key / API key 是常见方式),须遵守[官方开发者政策](https://docs.obsidian.md/Developer+policies)(明确披露付费与网络行为)。建议:插件本体免费(看板+任务视图),付费解锁 AI 整理/同步/多仓聚合——与整体 open-core 策略一致。

### 2.3 产品分层顺势成立

同一格式、同一内核,三类人群三个入口:
1. **AI coding 用户**:CLI + hooks + skill(强同步、对账);
2. **开发者(不深用 AI)**:桌面 App + VS Code/IDEA 插件(看板、提醒、注释扫描);
3. **纯记录用户(不接触 AI)**:Obsidian 插件 + 随想速记(最原本的 todo 需求)。

这是把 TAM 从「AI 开发者」扩到「所有 markdown 任务用户」的正确姿势,且不增加第二事实源。

---

## 3. D3:VS Code / JetBrains 插件

- **VS Code**:现有插件(Todo Tree 等)只做注释扫描只读视图(b §2),没有「TODO.md 看板 + 状态回写 + 对账」。实现:TS 插件,复用 WASM 内核或 spawn 本机 CLI;TreeView(任务面板)起步,webview 看板后置。商店无原生付费,走 license key(GitLens Pro 模式)。
- **JetBrains**:[Marketplace 原生支持付费插件](https://plugins.jetbrains.com/docs/marketplace/revenue-sharing-and-fees.html),官方提供 license 验证代码——**是三个插件渠道里唯一自带收款基础设施的**,适合放付费能力。实现:Kotlin 插件 spawn CLI(`--json`),不必移植内核。
- **优先级建议**:Obsidian(接盘真空+非 AI 人群)> VS Code(开发者密度最高)> JetBrains(付费渠道,后置)。三者都只是「皮」,厚度全在 Rust 内核与 CLI——这决定了插件维护成本可控。

---

## 4. D4:部署形态(后续)

- **单二进制**:Rust core + axum 本就是一个二进制(cat 网关模式);`todo serve` = REST API + Web 瘦客户端。这是 self-host 的第一形态,也是「curl CRUD」需求的载体。
- **docker compose**:server + 可选 ntfy 一键起(cat 的 `deploy/ntfy` 配置可直接复用)。
- **k8s/helm**:仅当团队/多租户版立项后再做,个人场景是负资产。
- **许可证防蚕食**:自托管版与付费云同步并存时,用 fair-code/Sustainable Use(Planka/n8n 模板,d 报告)——允许自托管自用,禁止他人拿你的 server 做竞品 SaaS。
- **铁律不变**:server/云/API 全部是可选增量,**不登录也能完整使用本地功能**(Vibe Kanban 之死,a §10)。

---

## 5. D5–D7:三个付费功能评审

### 5.1 D5 托管 LLM/API:识别、整理、分析项目 TODO ✅

- **定位正确**:这正是主报告 §7 的 Obsidian 模型——「收本地优先自己做不到的事」。对不折腾的用户,「开箱即用的 AI 整理」是真付费点;monday 的 AI credits(8 credits/动作、月送 500)是现成计费参照(f §1.3)。
- **必须 BYOK 并存**:开发者用户强烈偏好自带 API key;不允许会被喷「圈钱」,允许则托管档卖「零配置+更便宜的批量价+移动端可用」。BYOK 免费、托管计费,是这个人群的标准姿势。
- **三条红线**(证据在 e):① AI 整理排版必须「提议 + diff 预览 + 人确认」,绝不静默重排(agent 大量改写措辞会让 diff 被噪音淹没,信任一次崩塌就再也不开);② 误报即死刑——结构性识别(注释扫描、物证比对)零误报打底,LLM 只做语义增强且可关;③ 说清边界:两文件以内的小改动不值得走 AI 流程。

### 5.2 D6 随想:快捷键 + 空白画布 + 快速记录 ✅(v1 收窄)

- **价值判断**:这是高频入口(每天 N 次)带动低频资产(看板每天 1–2 次)的正确结构;「响应快」与你的性能定位同构。中文市场先例 [flomo](https://flomoapp.com/)(「持续不断记录,意义自然浮现」,订阅制,自述百万级用户)验证了「快速记录+回顾」的付费意愿;英文侧 Drafts(「先写后想」)长年是 iOS 付费榜常客。
- **v1 收窄建议**:「空白画布」先做**纯文本速记**——全局快捷键唤起 <100ms、失焦自动保存、Esc 即走;无限画布(excalidraw 类)复杂度高一个量级,后置。cat 已有全局快捷键与托盘基础设施可复用。
- **数据模型必须先想清楚(重要)**:随想是**个人层**(全局 inbox,存本地 SQLite/`inbox.md`),不进项目仓库;经「triage(整理分拣)」后才流入某个项目的 TODO.md。这一步分拣恰好是 D5 托管 AI 的天然入口:「把这堆随想整理成任务并分到项目」——免费手动分拣,AI 分拣付费。不要让随想直接写项目仓库,否则污染 git 历史(e §N8)。

### 5.3 D7 日/周/月/年视图付费 ⚠️(方向对,分界线要改)

- **方向被证据支持**:org-super-agenda(★1,535)远比 org-kanban(★267)流行——时间聚合视图 ≥ 看板(b §10.3);「我现在有 15 分钟该做什么」是被明确表达而无人满足的需求(e §N5)。
- **但不能把「视图」整体划进付费**:c 报告的结论是免费层做**规模阉割**而非功能阉割;imdone 12 年的教训是 time-to-first-value 决定生死。核心闭环(单仓看板 + 今日视图)必须免费,否则免费用户体验不到「灵」,转化无从谈起。
- **定界原则(已确认)**:免费层做规模阉割(单仓)而非功能阉割,核心闭环完整可用;付费按**边际成本**拆两档——零边际成本的功能进**买断**,有边际成本的服务进**订阅**(c §13:边际成本近零是买断制成立前提,自建云服务即破)。

| 免费 Free(单仓完整可用,永不强制登录) | Pro 买断(一次付费 + 12 个月更新窗) | 云服务订阅(有边际成本) |
|---|---|---|
| 看板 + 今日视图 + 随想速记(手动分拣) | 周/月/年回顾、热力图、周报卡片(cat 周报渲染可复用) | 托管 AI:识别/整理/分拣/对账语义层,含额度(monday credits 模式参照) |
| CLI/hooks 全量、注释扫描、`--json`(agent 生态信任基石) | 多仓聚合视图、「我有 15 分钟该做什么」推荐 | 跨设备同步中继 |
| 对账结构层(零误报)+ 语义层 BYOK(自带 API key) | 高级看板:泳道、WIP 上限、依赖图视图 | 托管推送(未来官方移动 app 的 APNs/FCM 通道) |
| 单仓库、系统本地通知 | ntfy/Bark 推送(用户自己的 topic/自托管,零边际成本) | 团队空间(远期,谨慎立项——Vibe Kanban 教训) |
| Obsidian / VS Code 插件本体 | JetBrains 插件(走其原生付费市场) | 企业私有化部署支持(按合同) |

要点:① **BYOK 免费是刻意的**——开发者自带 key 跑全部 AI 功能,买断/订阅卖的是「零配置、批量价、移动端可用」,避免「圈钱」骂名(社区对这类工具的强预期,见 a §9.4);② ntfy/Bark 放买断档,因为通道是用户自己的,我们零成本;③ 免费层永远「不登录完整可用」,登录只在购买/同步时出现。

---

## 6. 修订后的 MVP 顺序(替代主报告 §10)

1. Rust core:解析 + 保真回写 + SQLite 投影 + watch(不变)。
2. **CLI(canonical)**:`init/list/add/done/next/reconcile --json` + hooks 安装器 + CLAUDE.md/AGENTS.md 接入段 + skill。**(MCP 移出 v1)**
3. 对账引擎 v1:物证绑定(commit sha 范围+文件路径)+ 结构层检查 + 语义层提议(BYOK)。
4. Tauri 桌面:看板 + 今日视图 + **随想速记(全局快捷键)**。
5. **Obsidian 插件(WASM 内核)**:任务视图 + 看板,兼容 Tasks 语法与 Kanban 文件约定。
6. 通知:本地 + ntfy/Bark(复用 cat 链路)。
7. 之后:VS Code 插件 → `todo serve`(单二进制 self-host)→ compose → 托管 AI 计费 → JetBrains → 移动端 spike。

## 7. 第二轮决策(2026-08-30 晚):存储定稿、闭源分发、移动端定稿、收藏与浏览器插件

### 7.1 D8 任务存储定稿:A 起步 + 「分类拆分」演进路径

**定稿**:先做方案 A(单一 TODO.md + 五条硬约束:稳定 ID、行级回写、写队列+锁、归档命令、lint)。

**演进路径(委托方提出的 C 变体,记录以免遗忘)**:文件膨胀需要瘦身时,**按任务分类拆分**到 `todos/<分类>.md`,TODO.md 保留链接引用,继续充当唯一入口与索引。配套要求:

1. **权威归属写死**:任务数据以「任务行所在文件」为准;TODO.md 中指向分类文件的引用行是**指针不是副本**,不携带可漂移的状态。
2. **链接校验器**(`todo lint` 扩展):死链(目标文件不存在/目标 ID 不存在)、孤儿文件(`todos/` 下未被引用)、重复 ID、引用↔目标一致性;校验属结构层,零误报、进 CI 与对账引擎。
3. `todo slim`(或 `archive`)命令执行拆分,拆分本身必须是一次干净可审的 diff。
4. 待确认默认值(建议值,发布前拍板):行内详情超过约 500 字符提示溢出到分类文件;文案中的单文件适用边界写「活跃任务 ≤200 的中小项目」〔上限数值待确认〕。

### 7.2 D9 闭源与分发:四渠道政策调研结论(本轮核实)

**结论:「代码闭源 + 私有仓库」在四个目标渠道全部可行,不阻碍插件生态对接。**

| 渠道 | 闭源 | 分发形态 | 依据与注意 |
|---|---|---|---|
| **Obsidian 社区目录** | ✅ 允许,须 README 披露,逐案审查 | 公开 GitHub 仓库的 **release 二进制附件**(main.js/manifest.json/styles.css),用户安装时直接下载这些文件 | 官方政策原文:「Close sourced code. This will be handled on a case by case basis」([Developer policies](https://docs.obsidian.md/community-directory/developer-policies));官方论坛给出的实操路径:**公开仓库只放编译产物,真实源码私仓邀请审查员**([论坛答复](https://forum.obsidian.md/t/how-do-i-publish-a-closed-source-plugin/82588))。硬约束:**禁代码混淆**(minify 允许);付费、需账号、联网、遥测均须 README 披露;须带 LICENSE 文件 |
| **VS Code Marketplace** | ✅ 允许 | VSIX 包,无源码要求 | 微软自家 [Pylance 即闭源商业扩展](https://marketplace.visualstudio.com/items/ms-python.vscode-pylance/license);收费走自己的 license key(GitLens 模式) |
| **JetBrains Marketplace** | ✅ 允许 | 插件包,官方付费/license 验证基础设施 | 付费插件本身就是闭源商业形态([revenue sharing](https://plugins.jetbrains.com/docs/marketplace/revenue-sharing-and-fees.html)) |
| **Chrome Web Store** | ✅ 允许(不要求开源) | 打包上传 | [Code Readability Requirements](https://developer.chrome.com/docs/webstore/program-policies/code-readability):「must not obfuscate code」「**Minification is allowed**」 |

三条补充判断:
1. **闭源产品 + 开放 L1 不矛盾且必须并存**:格式 spec、lint、解析库仍开源(MIT/Apache),换 agent 生态信任与事实标准地位——beorg/Obsidian 均为「闭源产品 + 开放格式」的成功样本(b §10.3、c §13)。
2. **Obsidian 插件里的 WASM 内核需提交前确认**:政策只写了禁混淆,未明确 WASM;闭源本就逐案审查,提交时一并沟通〔待与 Obsidian 审查团队确认〕。保底方案:插件层 TS 明文可审,WASM 仅承载解析/回写算法并附说明。
3. **自有渠道不受任何政策约束**:官网直接分发 Tauri 安装包与 CLI 二进制(Homebrew/Scoop/npm),Obsidian 侧还有 BRAT/手动安装的旁路。商店只是入口,不是命脉。

### 7.3 D10 移动端定稿

- **主力机 Android**;委托方 Mac 已具备 Android + Tauri 全套环境,cat 已产出可安装 APK → **P1(PWA + Android Tauri 薄壳)风险显著低于一般判断,可提前**;iOS 顺延至 P2 spike。
- **PC 是主战场**;移动端功能收敛为四件:**看**(同步只读 + 勾选/推迟)、**写随记**、**收藏 URL**、**收通知**。不做移动端完整编辑〔P2 再评估〕。
- **数据通道定稿**:免费档 = 局域网直连桌面端(桌面 Rust 网关暴露局域网 API + 配对令牌,复用 cat 的 9527/9528 双面模式;这是 Backlog.md 硬编码 localhost 留下的空隙);付费档 = 云中继同步(收费点)。
- **Android 分享面板(share target)是随记/收藏的关键入口**:任何 app 里「分享到本应用」即完成收藏,成本低、频率高。

### 7.4 D11 新功能:URL 收藏 + HTTP 预览 + 浏览器插件

- **数据模型**:收藏是随想的一种(个人层 inbox,不进项目仓库),字段:url、标题、favicon、摘要、来源、标签;可后续 triage 成任务或沉淀为资料。
- **HTTP 预览(unfurl)**:桌面端 Rust(reqwest)抓取 og:title/og:description/og:image + favicon,本地缓存,移动端消费缓存;必须有隐私开关(仅存 URL 不抓取)、超时与响应大小上限;付费云档可由服务端代抓(移动端独立收藏时可用)。
- **浏览器插件**(Chrome MV3,后续 Edge/Firefox):快捷键/右键收藏当前页(URL + 标题 + 选中文本)、快速添加 todo;数据优先发**本机桌面网关**(localhost,免费档),桌面不在线时本地排队;付费档走云中继(手机可同步收到)。闭源允许(见 7.2)。
- **市场叙事素材**:Pocket——「稍后读」品类代名词——已于 2025-07-08 被 Mozilla 关停并删除账号数据([社区确认](https://www.reddit.com/r/kobo/comments/1ksvsli/mozilla_pocket_is_shutting_down_in_july_export/))。「收藏也应该本地化、纯文本化」与主线叙事完全同构,又一个「厂商死了数据还在」的现成论据。
- 免费/付费归位:本地收藏 + 本地预览 + 浏览器插件本体免费;云代抓、跨设备收藏同步、AI 整理收藏(自动打标/摘要)进付费档(与 §5.3 三档一致)。

### 7.5 对 MVP 顺序的微调(替代 §6 第 7 条之后的排序)

1–6 不变(core → CLI/hooks → 对账 → 桌面 App[看板+今日+随记] → Obsidian 插件 → 通知)。
7. **随记扩展 URL 收藏 + 本地 unfurl**(数据模型与随记同层,增量小);
8. **P1 移动端:PWA + Android Tauri 薄壳**(局域网直连;看/勾选/随记/收藏);
9. 浏览器插件(收藏与快速添加入口);
10. `todo serve` 单二进制 → compose;
11. 云中继同步 + 托管 AI 计费(付费档开闸);
12. VS Code 插件 → JetBrains 付费插件 → iOS spike。

---

## 8. 来源补充(两轮合并)

第二轮新增:
- Obsidian 开发者政策(闭源逐案允许、禁混淆、披露要求):https://docs.obsidian.md/community-directory/developer-policies
- Obsidian 闭源插件实操路径(官方论坛):https://forum.obsidian.md/t/how-do-i-publish-a-closed-source-plugin/82588
- Obsidian 插件提交机制(release 二进制附件):https://docs.obsidian.md/plugins/releasing/submit-plugin
- Pylance 闭源商业扩展先例:https://marketplace.visualstudio.com/items/ms-python.vscode-pylance/license
- Chrome Web Store 代码可读性政策:https://developer.chrome.com/docs/webstore/program-policies/code-readability
- Pocket 关停(2025-07-08):https://www.reddit.com/r/kobo/comments/1ksvsli/mozilla_pocket_is_shutting_down_in_july_export/

第一轮:

- Obsidian 付费插件讨论:https://www.reddit.com/r/ObsidianMD/comments/1rdkf5k/can_you_publish_a_paid_obsidian_plugin/
- Obsidian 开发者政策:https://docs.obsidian.md/Developer+policies
- Obsidian Bases 任务支持请求:https://forum.obsidian.md/t/bases-support-for-tasks/103074
- JetBrains Marketplace 付费与分成:https://plugins.jetbrains.com/docs/marketplace/revenue-sharing-and-fees.html 、license 验证:https://plugins.jetbrains.com/docs/marketplace/technical-implementation.html
- flomo:https://flomoapp.com/ (用户规模为官方自述,未独立核实)
- 其余引用见 a/b/c/d/e/f 子报告对应章节。
