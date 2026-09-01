# 商业模型三选项决策对比

> 状态：**待拍板**。对应 `DECISIONS.md` 决策② 最后一条待办。
> 证据基线：`research/c-commercial-mainstream.md` §13、`research/b-markdown-local-first-tools.md`、`research/e-community-voices.md`（调研日期 2026-08-30）。
> 拍板后把「已选」写回 `DECISIONS.md`，并按 ADR 惯例在 `docs/adr/` 留一条最终记录。

## 0. 先确认四个已经成立的事实

这四条不是假设，是当前代码库的既成事实，它们直接决定哪个选项可行。

| 事实 | 出处 | 对定价的含义 |
|---|---|---|
| **无服务器架构**：本地模式不需要登录，也不需要网络 | `README.md` 开头；`v1 不包含 MCP、账号、云同步和团队空间` | 边际成本近零 —— 这是买断制成立的**第一前提条件**，目前**满足** |
| **语义对账是 BYOK**：用户自带 `TODO_LLM_API_KEY`，走用户自己的 Anthropic/OpenAI 账号 | `crates/todo-core/src/llm.rs:88-125` | 最贵的 AI 能力**不产生你的推理成本**。对比 Notion「$10 per 1,000 credits」、ClickUp 最高 $40/人/月，你没有这条成本曲线 |
| **对账引擎已实装且体量最大**：529 行 + 独立的 LLM 语义层、`unknown` 一等判定、提议不写盘 | `crates/todo-core/src/reconcile.rs`、`llm.rs`、`prompt.rs` | 唯一真实的差异化资产。`DECISIONS.md:42`：白送它等于送掉付费理由 |
| **数据永远是用户的纯文本**：Markdown 唯一事实源，SQLite 只是可重建投影 | `README.md` 设计约束 | 信任叙事的地基。**任何选项都不得破坏它**，否则丢掉核心卖点 |

**第 4 条是硬约束，不是变量。** 三个选项都必须保证：不付费 / 停止付费 / 公司消失，用户的 `TODO.md` 都照常可读可编辑。这一条不参与权衡。

---

## 1. 三个选项的完整定义

### 选项 A：一次性买断 + 更新窗口（Things 3 / Sublime 式）

- 核心产品（桌面端 + 对账引擎 + 移动端）一次性买断，含 12–36 个月更新窗口。
- 窗口到期后**软件永久可用**，只是不再获得新版本；续费才继续更新。
- CLI + 格式 spec + 解析库保持开源免费。
- 个人买断，商业/团队走年度订阅（Sublime 双轨）。

### 选项 B：Obsidian 式 —— 客户端免费 + 服务订阅

- 桌面端、对账引擎、CLI **全部免费**（含商业使用）。
- 只对「本地优先做不到的事」收订阅：云同步中继、移动端推送、团队空间。
- 收入完全依赖后期才存在的服务端。

### 选项 C：组合双轨 —— 本体买断 + 可选服务订阅

- 本体（桌面端 + 对账引擎）买断，同选项 A。
- 云同步 / 移动推送 / 团队空间作为**独立可选订阅**叠加，同选项 B。
- 关键约束：订阅**不把本体功能扣为人质**，不买订阅本体照常完整可用。

---

## 2. 逐维度对比

| 维度 | A 买断 | B Obsidian 式 | C 组合 |
|---|---|---|---|
| **与无服务器架构的契合** | ⭐⭐⭐⭐⭐ 完全对齐 | ⭐⭐ 必须自建同步服务，**主动破坏买断前提条件①** | ⭐⭐⭐⭐ 本体对齐；订阅部分自负成本且成本随收入走 |
| **首日可收入** | ✅ 桌面端可发布即可收 | ❌ 必须先建同步服务（P1–P2，数月后） | ✅ 同 A |
| **开发者反感度** | 低。「买断，不订阅」是对照全行业订阅化的**独有叙事** | 最低（本体免费） | 低。订阅是可选加购，不是入场券 |
| **对账引擎的变现** | ✅ 直接变现最硬资产 | ❌ **白送最硬资产**，违背 `DECISIONS.md:42` | ✅ 同 A |
| **防守 Backlog.md 等免费竞品** | ⭐⭐⭐⭐ open-core：开源 CLI 建生态，收费卖 GUI/移动端 | ⭐⭐⭐⭐⭐ 本体免费，正面压制 | ⭐⭐⭐⭐ 同 A |
| **收入天花板** | 中。top-of-funnel game，每月都要有新客 | 高（若同步真做成） | 高。买断打底 + 订阅提供经常性收入 |
| **收入可预测性** | 低（一次性） | 高（MRR） | 中高 |
| **支持负担** | 永久化（卖出即永久支持义务） | 随订阅走，可持续 | 本体永久 + 订阅可持续 |
| **实现复杂度** | 低：license key 校验即可 | 高：账号体系 + 同步服务 + 冲突解决 | 中→高：分阶段，先 A 后加 B |
| **可逆性** | ⚠️ **单向**。转订阅会引发「背叛」叙事 | 较易加买断档 | ⭐⭐⭐⭐⭐ 两条腿都已就位，后续只调比重 |

---

## 3. 决定性证据

### 3.1 支持选项 A / C：买断在这个品类活得下来

- **Things 3 是唯一的买断制幸存者**，且活得很好：近十年持续维护（2026-08-28 仍在更新）、免费云同步、无折扣促销。各平台分开买断合计 $64.97，官方明确「one-time purchase... there are no subscriptions」（[Getting Things](https://culturedcode.com/things/support/articles/2803552/)）。
- **Sublime Text 双轨**：个人 $99「once off purchase, and come with 3 years of updates」，商业走订阅（[官方 store](https://www.sublimehq.com/store/text)）。这正是「个人买断 + 商业订阅」的成熟模板。
- **JetBrains perpetual fallback**：买订阅但连续满 12 个月获永久授权（[官方 FAQ](https://sales.jetbrains.com/hc/en-gb/articles/207240845-What-is-a-perpetual-fallback-license-and-how-do-I-use-one)）。
- 核心逻辑，一位开发者的原话：「I only use lifetime pricing for local-first apps. **If it runs on their machine, there's no marginal cost per user.**」（[DEV](https://dev.to/godnick/why-i-chose-lifetime-pricing-for-my-dev-tools-and-would-do-it-again-25mb)）——**你的 BYOK 架构让这条比一般本地应用还更成立。**

### 3.2 反对选项 B：它会主动打破自己的前提

调研文档对买断三前提的表述是：「边际成本必须近零 —— **委托方若自建云同步服务，这条就破了**。建议同步走 Git / 用户自有网盘，把服务器成本留给用户。」

选项 B 的**全部收入**都来自那个会打破前提的东西。更要命的是它与 `DECISIONS.md:42` 直接冲突：对账引擎是「全类目真空」的唯一差异化，选项 B 把它免费送出，然后指望用一个尚不存在、且要长期烧钱的同步服务赚回来。

**Obsidian 能这么做，是因为它先有了千万级装机量。** 你现在没有。B 是规模换收入的赌注，适合已有分发势能的产品。

### 3.3 三个必须避开的坑

1. **买断转订阅 = 背叛叙事**。Kaleidoscope 2023 年转订阅后典型反应：「I love the app, but **not $150 love it**」，用户立刻迁移到 Beyond Compare / BBEdit / FileMerge。→ **开发者极度价格敏感且完全有能力自建替代方案，定价模式一开始就要选对。**
2. **Lifetime Deal 是陷阱**。某复盘：售出约 1,300 份净得 $34,000，今天仍有约 1,150 个活跃账户产生 **$0/月**却持续消耗成本 ——「A lifetime deal is not revenue. It is a **loan you repay in support tickets**」。且「Active ones almost never upgrade. People who buy lifetime deals are **deal hunters by identity**」。→ 服务器成本那条对你不成立，但**支持成本永久化 + 最省钱人群的逆向选择完全成立**。
3. **捐赠不是商业模式**：单人中型开源库通常 $100–500/月，**超 75% 维护者拿到零资金**。

### 3.4 定价锚点

- 一条一手教训：「**I underpriced the lifetime anchor**... Default psychological anchors for one-time software skew low because of App Store conditioning. **Price the value, not the feel.**」
- 建议区间：**国内 ¥128–198 买断、国际 $29–39 买断**。
- **必须做区域定价**：TickTick 国内 ¥139/年 vs 国际标价 $49.99/年，「中国市场支付意愿显著更低」。

---

## 4. 推荐：选 C，但按 A 的形态发布

**推荐 C（组合双轨），且第一年在外观上与 A 完全一致。**

理由是 C 在当下**就是 A**——同步服务要到 P2 才存在（`DECISIONS.md` 决策③）。选 C 而非 A，唯一区别是**现在就把话说对**，避免日后加订阅时被指责「转订阅」：

> 发布日文案就写明：**本体买断、永久可用；未来的云同步是可选加购，不影响本体任何功能。**

这一句提前说，日后推同步订阅就是兑现承诺；不说，就是 Kaleidoscope 剧本。**这是选 C 相对选 A 的全部价值，成本为零。**

### 建议的具体形态

| 项 | 建议 |
|---|---|
| 免费层 | **规模阉割，不做功能阉割**：单仓库免费，完整包含对账引擎。让人跑完整闭环再付费——比 Todoist「5 个项目」更符合开发者心理 |
| 核心产品 | 买断 + **24 个月**更新窗口（Sublime 是 36，Things 是无限；24 是折中）。到期软件永久可用 |
| 价格 | 国际 $39、国内 ¥168。**取建议区间上沿**，因为 anchor 偏低是有记录的教训 |
| 双轨 | 个人买断；商业/团队年度订阅 |
| 未来订阅 | 云同步 / 移动推送 / 团队空间，独立可选，**永不扣本体为人质** |
| 明确不做 | ❌ Lifetime deal 促销 ❌ 把对账引擎降级为免费 ❌ 任何形式的数据扣押 |

### 渠道

- **不要只依赖 App Store**（抽成 30% / 小企业 15%；Things 因只在 App Store 卖而无法自主定价促销）。Tauri 可自主分发。
- **国际用 Merchant of Record**：Lemon Squeezy（现属 Stripe）**内建 license key 生成/激活/验证 API**，正是桌面应用所需，费率 5% + $0.50。多付的 5% 买的不是支付通道，而是「不用在几十个国家注册 VAT」。
- **国内不需要营业执照也能收款**：[微信支付](https://pay.weixin.qq.com/doc/v3/partner/4012713017)主体类型含小微商户（仅需身份证），但有额度限制（日收款 10 万，信用卡单日 1 千 / 单月 1 万）。起步小微商户，长期注册个体工商户。
- **合规成本约 $219/年**：Apple Developer $99/年 + Azure Artifact Signing 约 $9.99/月。约 3 份 $39 销售即可覆盖。
- ⚠️ **`tauri-plugin-updater` 的更新签名强制且不可关闭 —— 私钥丢失 = 永远无法再推送更新**，务必备份。

---

## 5. 待你拍板

**待拍板项已汇总到 `DECISIONS.md` 决策④，那里是唯一权威清单。** 本文只做论证，不重复维护勾选状态——同一份清单存两处，必然有一处过期。

共 7 项：选项 A/B/C、免费层形态、价格锚点、更新窗口长度、**README 承诺文案的时机（最紧急）**、国际收款渠道、国内收款主体。
