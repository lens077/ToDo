# 以本地纯文本 / Markdown 为事实源的开发者 todo 工具 — 品类调研

> 调研日期：2026-08-30
> 方法：以一手资料为主（官方文档、README、源码、GitHub issue、格式 spec、npm/GitHub API 指标），关键结论附来源 URL。源码级结论均在正文注明具体文件与行为。
> 目的：为「本地 TODO.md 为唯一事实源 + 看板视图双向回写 + git 提交 + MCP/agent 读写」的产品构想提供竞品与坑位参照。

---

## 0. 一页纸结论

**这个品类没有赢家，但坑已经被踩得很清楚。**

1. **没有事实标准。** `todomd/todo.md` 是这个品类里唯一像样的「TODO.md 规范」尝试，但它 2023-08 后就没再动过，watcher 只有 4 个（见 §4）。真正有事实影响力的是 **GFM task list（`- [ ]`）**——它只定义「勾选框」，不定义状态机、优先级、截止日期、列。委托方要么自己定义扩展，要么寄生在已有的语法习惯上。
2. **双向回写的头号杀手是「全文件重写」。** Obsidian Kanban 的 `boardToMd()` 是从内存 AST 完整重新序列化整个文件（源码实证，见 §5.1.2），后果是用户写在看板文件里的表格、图片嵌入、非规范标题会被**静默删除**（issue #1082）。**同一生态里的对照组极其干净**：Obsidian Tasks 采用「行级就地改写 + 三级定位 + 不一致就放弃并告警」，结果是**下载量高出 58%、原作者成功交接给团队、至今活跃**；而 Kanban 已事实停摆、作者公开找接盘且无人应征。**这是委托方最需要用架构手段规避的风险，也是本报告最强的一条经验证据。**
3. **「纯文本」和「可靠同步」在历史上是对立的，两条路都有人走到头了。** Taskwarrior 3.0 为了同步可靠性把纯文本 `.data` 换成了 SQLite，代价是老用户流失和「数据消失」事故；org-mode 守住纯文本，代价是移动端只能 Force Load / Force Save 二选一覆盖，没有 merge（见 §7）。委托方主张「TODO.md 是唯一事实源 + SQLite 只做缓存」，方向上正好绕开这个对立——**前提是缓存永远可从文件重建，且冲突交给 git 而不是自研同步**。
   - **破局的关键单点是「稳定 ID」**：orgzly 作者亲口承认「真正的 merge 需要给每个 note 分配 ID，而我一直回避这么做」，这正是 org 移动端只能整文件覆盖的根因；反过来 Taskwarrior 靠 uuid + `tag_<x>` 拆分实现了自动合并。**两个方向的独立证据指向同一条强制性设计要求。**
4. **注释扫描是只读的、是入口不是主线。** VS Code 的 Todo Tree 等插件都是 ripgrep 全量扫描 + 树视图，没有回写能力（见 §2）。imdone 是唯一真正做了「拖卡片 → 改写源文件里的 token」的产品（见 §1）。
5. **商业化极其艰难，全品类只有一个成功样本。** imdone 做了十几年，npm 月下载约 2 千，最终在 2025 年把主叙事从「markdown 看板」改成了「Jira issue 落地成仓库里的 markdown，给 AI 保留上下文」，定价 $9/月（个人）、$18/用户/月（团队）、$1500 的 30 天试点（见 §1.5）。
   - 其余全部是**零商业化**：git-bug ★10k 无收入、Taskwarrior 仅靠捐赠（维护者自述「I've run out of steam」，而这正是 3.0 数据事故的深层原因）、todo.txt 生态**完全没有商业层**（SwiftoDo 已免费、macOS 版 2021 年后停更、Simpletask 已归档、Sleek 靠 Liberapay 交商店年费）。
   - **唯一走通的是 beorg（iOS org-mode 客户端）：开放数据格式 + 闭源精品体验 + 订阅制**，主打「Your data. Your choice.」。**这为委托方「要商业化」给出了唯一被验证过的姿态。**

---

## 1. imdone — 唯一真正做过「看板改动回写源文件」的老牌产品

- 核心库：<https://github.com/imdone/imdone-core>（★151，MIT，2014 年起，仍在提交，最后 push 2026-08）
- 官网 / 文档：<https://imdone.io/docs/imdone-kanban/>
- 已归档的 Atom 时代前身：<https://github.com/imdone/imdone-atom>（★221，2019 年停更）

### 1.1 数据格式：三种任务语法

imdone 的关键设计是**「列名即 token」**——任务写在哪一列，是由文本里的 token 本身决定的，而不是由它在文件中的位置决定的。这是它跟 Obsidian Kanban 最根本的分野。

来源：[imdone-core README](https://github.com/imdone/imdone-core/blob/master/README.md)

**Code Style（代码注释，仅在注释中识别）**
```
// TODO This is a task
// TODO: This is a task
// TODO:5 This is a task
// TODO: A task with a description looks like this.
// Every line after the task is part of the description until we find another
// task, a blank comment line, or a line of code
```

**Hash Style（markdown / 任意文本）**
```
#TODO: This is a task
#TODO:0 This is a task
#to-do:0 This is a task
```

**Markdown Style（链接形态，避免渲染成正文）**
```
[This is a task](#todo:)
[This is a task](#todo:10)
```

冒号后的数字是**排序权重**（sort order），不是优先级。这是 imdone 解决「看板内拖动排序如何持久化到纯文本」的核心手段。

### 1.2 元数据：直接复用 todo.txt 语法

imdone 明确声明「使用 todo.txt 的 +project/@context 和 meta:data」（README）：

```
#DOING:20 This task was created on 2018-02-09 created:2018-02-09
#DOING:20 This task was completed on 2018-02-09 completed:2018-02-09
#doing:20  This task is due on 2015-02-09 due:2015-02-09
#doing:20  This task has a *madjs* tag +madjs
#doing:20  This task has a *madjs* context @madjs
#doing:20  This task has profile metadata profile:piascikj
```

`meta:value` 还能在 `.imdone/config.yml` 里配 `urlTemplate` 变成可点击外链（例如 `user:piascikj` → GitHub 主页）。**这是一个很值得借鉴的设计：用一个通用 `key:value` 槽位 + 配置化的渲染规则，替代硬编码字段。**

### 1.3 解析实现（源码级）

文件：`lib/adapters/parsers/task/CardContentParser.js`

```js
const CODE_STYLE_PATTERN = '([A-Z]+[A-Z-_]+?)((:)(-?[\\d.]+(?:e-?\\d+)?)?)?[ \\t]+(.+)$'
export const LIST_NAME_PATTERN = '[\\p{L}0-9-_]{2,}'
export const CHECK_REGEX = /^(\s*- )\[([x ])\]/
```

值得注意的工程细节：
- **列名规则不对称**：code style 的列名必须全大写（`[A-Z]+[A-Z-_]+?`），hash/markdown style 则允许任意大小写字母（`\p{L}`，支持 Unicode）。
- **行内代码排除**：`hasTaskInText()` 里有 `isWithinInlineCode()`，会把落在反引号 `` `...` `` 里的 `#TODO` 排除掉——这是扫描器必须处理的真实噪音来源。
- **描述的边界**：代码文件里描述在「遇到另一个任务 / 空注释行 / 一行代码」时结束；非代码文件里在「遇到另一个任务或空行」时结束。
- **HTML 注释包裹**：markdown 里为避免任务被渲染，官方推荐写在 `<!-- ... -->` 里，并且**必须在描述和注释结束标记之间留空行**，否则结束标记会被吃进描述（README 明确警告）。这类「格式陷阱」正是纯文本方案的典型税负。

### 1.4 双向回写：拖动 = 改 token + 重排 order

官方文档明说：「When a card is moved to a new list or position, imdone changes the token (e.g. TODO) in the file.」（<https://imdone.io/docs/imdone-kanban/>）

排序的实现在 `lib/get-tasks-to-modify.js`，逻辑是**分数式排序（fractional ordering）**：找到目标位置前后两个「order 不同」的任务，取其区间，用 `(endingOrder - startingOrder) / changes` 算增量插入；若后面没有任务则直接 `+10`。

```js
const increment = isNumber(endingOrder) ? (endingOrder - startingOrder) / changes : 10
```

**这是委托方可以直接抄的方案**：它保证了「拖动一张卡片，通常只需要改写一行」，而不是把整列的序号全部重排。对 git diff 友好度极高——这正是「todo 随 git 提交」场景的刚需。

相关能力：
- `moveTasksBetweenLists()`：重命名列 = 批量改写所有相关文件的 token。
- 文件监听：「While imdone is running it listens for file changes in your project and updates your board.」
- 事件模型完善（`task.found` / `file.modified` / `tasks.moved` / `config.update` 等 19 个事件），说明它内部就是一个 file-watch → 增量 re-extract 的管线。

### 1.5 商业化与现状：**本调研最有价值的市场信号**

imdone 的商业化轨迹可以完整还原：

| 阶段 | 定价 / 形态 | 来源 |
|---|---|---|
| 早期 | LTD 终身买断 $14.99（2 设备，v1.x） | [ziligma 评测](https://ziligma.com/imdone-review/)、[ltdguides](https://ltdguides.com/products/imdone-lifetime-subscription/) |
| 2025-11 | 订阅 $18/月，**转化率为零** | [DEV: Early Access Pricing](https://dev.to/imdone/early-access-pricing-for-imdone-cli-desktop-developer-pricing-experiment-4eic) |
| 2025-12 | 降到 $9/月做一周实验，24 小时内拿到 2 个付费用户，比之前整月都多 | [DEV: Lab Notes](https://dev.to/imdone/lab-notes-what-i-learned-dropping-imdones-price-for-a-week-543l) |
| 现在 | 个人 $9/月、团队 $18/用户/月固定；主打「AI Story Memory」，30 天试点 $1,500 | [imdone.io](https://imdone.io/) |

作者本人的复盘结论（几乎是给委托方写的）：

> "Price is rarely the final boss. Dropping from $18 to $9 helped solo devs, but it didn't magically fix adoption. It just revealed the next constraint."
>
> 真正的问题是：「People don't fully understand what the product actually does」「First-time setup feels heavy or risky」「Time-to-first-value is too long」。

**产品叙事的迁移**同样关键。imdone 官网首页现在的主语已经不是看板，而是：

> "Imdone keeps the decisions, blockers, evidence, and next steps behind each Jira story with the work, so the next person **or agent** can continue without reconstructing it."

即：**从「markdown 看板」转向「给 AI/agent 保留工程上下文」**，并把入口从「扫描你的 TODO 注释」改成「`imdone pull` 把 Jira issue 拉成仓库里的 markdown」。官网自己也把桌面版降级成了次要产品（"Looking for imdone desktop? Imdone desktop is still here."）。

**规模线索（真实但残酷）**：
- `imdone-core` npm 月下载 **2,111**；`imdone-cli` **2,164**（npm API，2026-07-31 ~ 08-29）
- 核心库 ★151，主仓 `imdone/imdone` 仅 ★26，Obsidian 插件 ★18
- 整个 imdone org 是**单人项目**（提交几乎全是 piascikj）

### 1.6 对委托方的意义

**可借鉴**：token 即列名（改状态 = 改一个词，天然 git 友好）；分数式 order；`key:value` + urlTemplate 的可扩展元数据；行内代码排除。

**警示**：
- imdone 用十二年验证了「开发者向 markdown 看板」作为独立产品**很难卖**。
- 它最终找到的付费理由不是「看板好看」，而是**「上下文不丢失，人和 agent 都能接着干」**——这跟委托方的「同步事实」按钮、MCP/skills 是同一个方向。这是个正面信号，但也意味着委托方会在这个赛道上正面遇到 imdone。
- 委托方的差异化必须落在 imdone 明确没做好的地方：**启动速度、开箱即用（time-to-first-value）、agent 原生集成的深度**。

---

## 2. VS Code 注释扫描插件：只读视图，不是事实源

### 2.1 Todo Tree

- 仓库：<https://github.com/Gruntfuggly/todo-tree>（★1,816，fork 188）
- **最后 push：2024-04-13**，open issues **195** — 事实上已停止维护

**默认正则（从 `package.json` 的 `todo-tree.regex.regex` 直接读出）**：
```
(//|#|<!--|;|/\*|^|^[ \t]*(-|\d+.))\s*($TAGS)
```
默认 tags：`["BUG", "HACK", "FIXME", "TODO", "XXX", "[ ]", "[x]"]`，`regexCaseSensitive: true`。

`$TAGS` 是占位符，运行时被替换成 tag 列表的 alternation。底层用 **ripgrep** 全量扫描。

**这个正则暴露了纯正则扫描的根本局限**：
- 它只匹配「注释起始符 + TAG」，`(//|#|<!--|;|/\*|^|^[ \t]*(-|\d+.))` 这一段是在**猜**注释语法，而不是真正解析语言。因此它无法区分「真注释里的 TODO」和「字符串字面量里的 TODO」，也无法处理块注释的续行。
- 对比 imdone 的做法：imdone 维护了一份 `lib/languages.js` 的语言→注释符映射表，且**只在注释内**识别 code style 任务。这是更正确但成本更高的路线。

**能力边界**：Todo Tree 是纯粹的**只读树视图**。它没有状态模型、没有优先级/截止日期、不能勾选完成、不能回写。它的价值是「发现」，不是「管理」。

### 2.2 Todo+（fabiospampinato/vscode-todo-plus）

这个更接近委托方的形态，因为它有**自己的纯文本格式**（`.todo` 文件）而不只是扫描：项目用行尾冒号表示（`Project:`），任务用 `☐` / `✔` / `✘`（或 `[ ]` / `[x]`），并用 `@started` / `@done` / `@critical` / `@high` / `@low` 等标签承载状态和时间追踪，`@lasted` 自动计算耗时。

它证明了一件事：**在纯文本里用「行首符号 + @tag」承载结构是可行的且体验不差**，但代价是这套格式只有它自己认识——不是 GFM，GitHub 上渲染不出来，其他工具读不懂。委托方选择 GFM `- [ ]` 是更保守也更正确的决定。

### 2.3 TODO Highlight

只做语法着色，无任何结构化模型。不构成竞品，只说明「注释里的 TODO」是一个被反复验证的**开发者习惯**——这是委托方「扫描代码注释导入」功能的需求基础。

### 2.4 小结

这一层工具的共性：**扫描是单向的**。它们把代码当只读输入，从不写回。委托方要做「扫描 TODO/FIXME 导入」，需要明确回答一个它们都回避了的问题：

> **导入之后，源码里那行 `// TODO:` 是留着、删掉，还是改写成带 ID 的引用？**

三种选择各有代价（留着 = 双份事实源会漂移；删掉 = 破坏性且 diff 噪音大；改写成 `// TODO(#id):` = 最稳但侵入代码）。imdone 的答案是「不导入，注释本身就是卡片」——这是最自洽的，但要求用户接受 token 被工具改写。

---

## 3. todo.txt 生态：规范最扎实，但天花板明确

- 规范：<https://github.com/todotxt/todo.txt>（★3,384）
- CLI：<https://github.com/todotxt/todo.txt-cli>（★6,166，GPL-3.0，仍在维护，最后 push 2026-08）
- Sleek（Electron GUI）：<https://github.com/ransome1/sleek>（★2,027，MIT，活跃）
- topydo（TUI）：<https://github.com/topydo/topydo>（★933）

### 3.1 格式规范（一手，来自官方 README）

核心公理：**「A single line in your todo.txt text file represents a single task.」**

规范的两条哲学目标非常值得抄：
> - 文件内容在没有任何工具、只有纯文本查看器时也应可读。
> - 用户可以在纯文本编辑器里以符合直觉的方式操作它。例如，一个能按字母排序行的编辑器，应该能对任务列表做出有意义的排序。

这解释了格式里所有看起来奇怪的决定：**优先级和日期放在行首，是为了让 `sort` 命令直接可用；完成标记用 `x`，既像填好的复选框，又能在字母排序中沉到底部。**

未完成任务的三条规则：
1. 优先级若存在必须在**最前面**，格式为 `(A)` — 大写字母 + 括号 + 空格。
2. 创建日期紧跟优先级之后，`YYYY-MM-DD`。
3. `+Project` 表示项目，`@context` 表示上下文，可出现在行内任意位置。

```
(A) Thank Mom for the meatballs @phone
(B) Schedule Goodwill pickup +GarageSale @phone
Post signs around the neighborhood +GarageSale
@GroceryStore pies
```

完成任务：行首 `x ` + 完成日期（+ 可选创建日期）。扩展元数据用 `key:value`（社区约定：`due:`、`rec:`、`t:` 阈值日期、`pri:` 保留原优先级）。完成项通常归档到 `done.txt`。

### 3.2 为何小众（分析）

todo.txt 的规范质量其实高于本调研中的任何一个 markdown 方案，但它有三个结构性问题：

1. **`key:value` 扩展是「社区约定」而非规范。** `due:` / `rec:` / `t:` 都不在官方 README 的强制部分，导致各客户端支持度不一致——A 客户端写的 `rec:1w`，B 客户端可能只当成普通文本。**这正是「没有中央规范就会碎片化」的活标本，委托方若要定义 TODO.md 扩展语法必须引以为戒。**
2. **没有子任务、没有层级。** 「一行一个任务」的公理是它简洁的来源，也是它的天花板。开发者的真实任务是有层级的。
3. **同步靠外部文件同步（Dropbox / Nextcloud / Syncthing）**，多设备并发编辑时是整文件冲突，没有行级合并。

另有两条更硬的证据：

- **规范本身已被官方宣告实质无维护**：todo.txt 仓库 issue #100 是一则 PSA，说明该规范 "effectively unmaintained"。
- **`key:value` 是个「没有键名清单的容器」**：issue #8 中官方原话——*"there is no full list since each person can use their own set of special tags"*。这是碎片化的官方认证。

### 3.2.1 商业层：**整个生态没有商业层**（对原始假设的重要修正）

调研中发现原始简报对 todo.txt 移动端商业化的判断是**反的**，实际情况是：

| 项目 | 实际状态 | 证据 |
|---|---|---|
| **SwiftoDo (iOS)** | 作者是 **Michael Descy**，**免费**，v3.3.5，最后更新 **2024-05-01** | Apple iTunes Lookup API（双路径交叉验证）|
| SwiftoDo Desktop (macOS) | $9.99，**2021-03-21 后再未更新** | App Store |
| **Simpletask (Android)** | GitHub 仓库**已 archived**，最后 push 2023-11-25，339 个 open issue 冻结，release 停在 6.2.0 @ 2015-11-04 | GitHub |
| Sleek | 靠 GitHub Sponsors / Liberapay 覆盖 Apple 和 Microsoft 商店年费；作者已明确**排除做移动端**（issue #669） | README / issues |

Sleek 的 README 里作者公开求助：「sleek's backlog is becoming increasingly populated, yet our capacity is limited」。**一个 2000 star 的 FOSS todo 工具，连应用商店年费都要众筹。**

> **这条修正很重要**：todo.txt 生态**没有任何一个在运转的商业层**。这比任何格式层面的抱怨都更能解释它为什么停滞——**没有钱，就没有移动端；没有移动端，就没有增长。** 这与 §7 中 Taskwarrior 的结论完全一致，也与 beorg 作为 org 生态唯一商业化成功案例形成对照。

（注：SwiftoDo 的 App Store 页面未显示内购，但未能正面证实「无内购」，此处「免费」应读作「下载免费」。）

### 3.3 对委托方的意义

**可借鉴**：「让文件在 `sort`/`grep` 下有意义」这条设计原则；`key:value` 通用槽位；`done.txt` 归档分离（避免主文件无限膨胀——委托方的 TODO.md 也会遇到这个问题）。

**警示**：不要把扩展语法留给「社区约定」。要么写进规范并提供校验器，要么不做。

---

## 4. TODO.md 格式约定：**没有事实标准**

- <https://github.com/todomd/todo.md>（★284，fork 446，**最后 push 2023-08-07**，无 license）

规范全文很短，核心如下：

```
# Project Name
Project Description

### Column Name
- [ ] Task title ~3d #type @name yyyy-mm-dd  
  - [ ] Sub-task or description  

### Completed Column ✓
- [x] Completed task title  
```

规则：
- 列 = `###` 标题；任务 = `- [ ]` 或裸 `- `
- **完成列的列名必须含 `✓` 或 `[x]`**
- 每个任务标题**结尾有两个空格**（为了在 GitHub Pages 上换行）
- `~3d` 估时、`#type` 标签、`@name` 负责人、`yyyy-mm-dd` 日期，**统统追加在标题末尾**
- 缩进 2 空格 = 子任务或描述

### 4.1 判断：这不是标准，是一个被遗弃的提案

证据链：
- **watcher 只有 4 个**，而 star 284 / fork 446。fork 数远高于 star 数是个强信号：**人们 fork 它是为了复制那份 TODO.md 模板文件，不是为了参与规范。**
- 2023-08 之后零提交，7 个 open issue 无人处理。
- 无 license（法律上其实无法安全采用）。
- `gh search code "todomd/todo.md"` 返回空 — 几乎没有代码库引用这个规范。

### 4.2 规范本身的技术缺陷

即使采用，这套规范也有硬伤，委托方应逐条规避：

| 缺陷 | 后果 |
|---|---|
| 「完成列名必须含 ✓ 或 [x]」 | 用中文列名（如「已完成」）就得写成 `### 已完成 ✓`，把**呈现语义塞进了标识符** |
| 行尾两空格 | 不可见字符承载语义，几乎所有编辑器的「删除行尾空白」和 prettier 都会破坏它 |
| 元数据全部裸追加在标题末尾 | `~3d #type @name 2020-03-20` 与任务正文没有分隔符，解析歧义大（任务标题里本来就可能有 `#` 或 `@`） |
| 无 ID | 任务无法被稳定引用，跨文件依赖、agent 精确改写都做不到 |

### 4.3 结论

**委托方几乎必然要自己定义格式。** 唯一真正稳固的地基是 GFM task list（`- [ ]` / `- [x]`），因为 GitHub、GitLab、VS Code 预览、几乎所有 markdown 渲染器都认它。在此之上的一切（列、优先级、截止日期、ID）都是自定义扩展，**没有先例可以「兼容」，只有先例可以「避坑」**。

这既是坏消息（没有现成的网络效应可蹭），也是好消息（没有历史包袱，可以把 ID 这类关键设计一次做对）。

---

## 5. Obsidian 双插件：markdown 双向看板最成熟的实现（也是最大的坑源）

### 5.1 Obsidian Kanban — **最值得逐行研究的对象**

- 仓库：<https://github.com/mgmeyers/obsidian-kanban>（现重定向至 `community-archive/obsidian-kanban`）
- ★4,483，fork 451，GPL-3.0，**open issues 600**

#### 5.1.1 文件格式（源码实证：`src/parsers/common.ts`）

```ts
export const frontmatterKey = 'kanban-plugin';
export const completeString = `**${t('Complete')}**`;
export const archiveString = '***';
export const basicFrontmatter = ['---', '', `${frontmatterKey}: board`, '', '---', '', ''].join('\n');
```

一个 Kanban 文件长这样：

```markdown
---

kanban-plugin: board

---

## 待办

- [ ] 任务一
- [ ] 任务二

## 进行中

**Complete**
- [ ] 任务三


***

## Archive

- [x] 已归档的任务

%% kanban:settings
```
{"kanban-plugin":"board"}
```
%%
```

要点：
- **列 = `## ` 二级标题**（`laneToMd()`：`lines.push('## ' + ...)`）
- **卡片 = `- [ ] ` 列表项**（`itemToMd()`：`` `- [${item.data.checkChar}] ${...}` ``）
- **frontmatter `kanban-plugin: board`** 是文件的「我是看板」标记，Obsidian 靠 metadataCache 的 frontmatter 判定（`src/main.ts` 中 `cache.frontmatter[frontmatterKey]`）
- **`**Complete**`** 紧跟列标题时，表示「移入此列的卡片自动标记完成」（`lane.data.shouldMarkItemsComplete`）
- **`***`（水平线）+ `## Archive`** 是归档区分隔（`archiveToMd()`）
- **`%% kanban:settings` + 代码块 + `%%`** 把整个看板的**每板设置以 JSON 塞进 markdown 注释**（`settingsToCodeblock()`）。Obsidian 里 `%% ... %%` 是不渲染的注释——**这是「把配置藏进纯文本文件而不污染阅读体验」的经典手法，值得借鉴**。

#### 5.1.2 致命缺陷：`boardToMd()` 全文件重写

这是本次调研最重要的一条源码发现（`src/parsers/formats/list.ts`）：

```js
export function boardToMd(board: Board) {
  const lanes = board.children.reduce((md, lane) => {
    return md + laneToMd(lane);
  }, '');
  const frontmatter = ['---', '', stringifyYaml(board.data.frontmatter), '---', '', ''].join('\n');
  return frontmatter + lanes + archiveToMd(board.data.archive) + settingsToCodeblock(board);
}
```

**保存 = 从内存里的 Board 对象把整个文件重新拼出来。** 任何不属于「frontmatter / `## 列` / `- [ ] 卡片` / archive / settings」结构的内容，在往返一次之后就消失了。

用户实证 —— [issue #1082「Deletes anything not formatted for Kanban board」](https://github.com/mgmeyers/obsidian-kanban/issues/1082)（2024-12 提交，**至今 OPEN，零回复**）：

> 我只是想试用一下这个插件。在一个已经有任务、表格和图片的笔记的 frontmatter 里加了 `kanban-plugin: board`，选择 "Open as kanban board"……当我切回 markdown 时，**所有标题都被降级成了 h2，所有表格和图片嵌入都被删除了。**

相关的 [issue #1042](https://github.com/mgmeyers/obsidian-kanban/issues/1042) 是同一问题的功能请求版本：「希望能在笔记里放与看板无关的内容」。社区有人提了 PR #1073（把看板数据放在特定 H1 下，实现看板与正文共存），**未被合并**。

> **对委托方的直接启示**：委托方的 TODO.md 是「唯一事实源」，用户一定会往里写标题说明、链接、代码块、注释。如果采用「解析成模型 → 序列化回文件」的架构，就会重演 #1082。
>
> **必须采用的替代架构是「保守写入 / surgical edit」**：解析时记录每个任务的**原始行号与原始文本**，写回时只替换发生变化的那些行（甚至只替换行内变化的片段），其余字节原样保留。imdone 的「改 token + 改 order 数字」本质上就是这个思路，因此它没有这个问题。

#### 5.1.3 同步与冲突

[issue #439「Syncing issue (between laptop and mobile?)」](https://github.com/mgmeyers/obsidian-kanban/issues/439)（2022-01 提交，**至今 OPEN**）：

- 原始报告：笔记本上的改动没同步到手机，随后**手机上的旧版本反向覆盖了新版本**，且 Obsidian 的版本历史里只剩很旧的版本可恢复 → 实质数据丢失。
- 2024-10 仍有用户跟帖（iOS + Mac + Obsidian Sync）：「usually duplicated bullet points tasks (cards in kanban)……but so far no data loss encountered, only duplicated notes」——**同一张卡片被复制成两份**。

**根因分析**：因为保存是全文件重写，两端各自重写整个文件，同步层看到的是「两个完全不同的文件版本」，无论是 Obsidian Sync 还是 git，都只能整文件二选一或产生大面积冲突。**行级最小化改动不仅是 diff 美观问题，更是同步可合并性的前提。**

#### 5.1.4 性能

维护者与核心贡献者 pjeby 在 [issue #178](https://github.com/mgmeyers/obsidian-kanban/issues/178) 和 [#938](https://github.com/mgmeyers/obsidian-kanban/issues/938) 中留下了非常详实的性能剖析记录，对委托方极有参考价值：

- 「the parser is insanely slow, taking >1ms per line, which means that it takes **a quarter second just to parse a board with a few hundred items**」
- 加了解析缓存后：「under 1ms to parse a changed version of a file that's already been loaded, because it **only processes lane headers and changed lines**」— **增量解析是有效的**
- 但真正的瓶颈是渲染：约 220 张卡片的看板，初次加载 1.5~2 秒；维护者自陈瓶颈在 Obsidian 的 markdown 渲染器，最终靠**三阶段渲染**（解析 → 预渲染卡片 markdown → 渲染 React 树）+ 内容不变则不重渲染的缓存才缓解
- pjeby 还发现了状态回环 bug：外部改文件 → setExternal → React 重渲染 → useEffect 误判 → setInternal **把外部改动改回去**。这是「文件监听 + UI 状态」双向绑定的经典陷阱，委托方做 file watch 时必然会遇到。
- [issue #1201](https://github.com/mgmeyers/obsidian-kanban/issues/1201)（2026-03，OPEN）：拖拽时 O(n) 遍历所有列表项做碰撞检测。

#### 5.1.5 维护状态：**已实质停摆**

- 仓库已迁到 `community-archive` org
- 提交历史：**最后一次真正的代码修复是 2024-06-03（"Fix #973"）**；2026-01 加了 `MAINTAINERS.md`，2026-03 的最后一次提交是 **"Remove funding"**
- open issues **600**，watcher 仅 18
- **官方下载量 2,607,917**（[Obsidian 官方 community-plugin-stats.json](https://raw.githubusercontent.com/obsidianmd/obsidian-releases/master/community-plugin-stats.json)）

作者在 [`MAINTAINERS.md`](https://github.com/community-archive/obsidian-kanban/blob/main/MAINTAINERS.md)（2026-01-12）中亲笔公开征集接手人，原文值得完整引用：

> **January 12, 2026** — Hello! It's me! The developer of the Kanban plugin.
> I no longer have the bandwidth to maintain the Kanban plugin (**and haven't for a while**) and I'm looking for new maintainers to take over the project.
> - The plugin's code is **kind of a mess in spots**. I built this while I was a stressed out grad student and often "good enough" was good enough.
> - The plugin's code is **fairly complex, and LLMs may struggle with parts of it**. You should be able to code without the use of LLMs to reduce the amount of bugs...

**至今无人接手**：按 star 排序的最大 fork 仅 `geetduggal/obsidian-kanban`（31★）、`kepano/obsidian-kanban`（20★，2024-07 后未动），相对上游 4,483★ 完全不成量级。

**一个 260 万下载量、被广泛推荐为「Obsidian 看板首选」的插件，实质上已经无人维护。** 这既是警示（这个品类留不住维护者），也是机会（大量用户正在寻找替代品）。

#### 5.1.6 补充：更多源码级坑位（对委托方直接适用）

以下细节均来自源码，每一条委托方都会在自己的实现中遇到同样的抉择：

**（a）UI 状态被写进事实源 —— 冲突的隐形放大器**

`%% kanban:settings %%` 里存的不只是配置，还包括 **`list-collapse`（哪几列被折叠）**。也就是说**用户只是折叠了一列这样一个纯 UI 操作，文件就被改写了**。多设备场景下，两端只是「各自看了看板、折叠了不同的列」，就会产生互相冲突的整文件写入。

> **对委托方的规则**：**纯视图状态（折叠、滚动位置、当前筛选）绝不能进 TODO.md。** 应放在 SQLite 缓存或 `.gitignore` 的本地状态文件里。

**（b）frontmatter 被无条件重排**

`boardToMd()` 用 `stringifyYaml(board.data.frontmatter)` 重写整个 frontmatter → 用户自定义的 YAML 排版、注释、键顺序全部丢失。实证：[#787「Plugin reformats yml front matter undesirably」](https://github.com/community-archive/obsidian-kanban/issues/787)（2023-06，至今 OPEN）。这对「TODO.md 同时被其他工具当数据源」的场景是硬伤。

**（c）魔法字符串 + 本地化 = 双重陷阱**

`**Complete**` 和 `## Archive` 都是 `t()` 本地化字符串。后果：**切换 Obsidian 语言后，归档区可能不再被识别**；而卡片正文里恰好出现 `**Complete**` 就会造成误判。归档区的识别条件还很脆——必须是紧跟 `***`（thematicBreak）之后的、标题恰为 `Archive` 的 heading；手写 `## Archive` 忘了上面的 `***`，它会被当成**普通列**渲染。

> **对委托方的规则**：语义标记必须是**语言无关的稳定标识符**（如 HTML 注释里的 `<!-- todo:done-column -->` 或 frontmatter 字段），绝不能依赖会随 UI 语言变化的显示文本。

**（d）列标题里的 `(数字)` 会被吞掉**

```ts
// src/parsers/helpers/parser.ts:60
const match = str.match(/^(.*?)\s*\((\d+)\)$/);
```
这是 WIP 上限语法。副作用：一个正常命名为 `## Sprint (2)` 的列，`(2)` 会被当成 WIP 限制**从标题里剥离**。这是「在自然语言标题里塞语法」的典型代价。

**（e）纯文本编辑后视图不刷新 → 反向覆盖**

实证 issue：[#380](https://github.com/community-archive/obsidian-kanban/issues/380)（在 markdown 视图加卡片，切回看板看不到）、[#399](https://github.com/community-archive/obsidian-kanban/issues/399)、[#644](https://github.com/community-archive/obsidian-kanban/issues/644)（切换视图后看板损坏直到重启）。风险模型很明确：**视图内存态与磁盘态不一致时，下一次 UI 操作会用内存里的旧板覆写磁盘 → 外部编辑丢失。**

这正是委托方最危险的场景：用户/agent 在编辑器里改了 TODO.md，GUI 还持有旧状态，此时用户在 GUI 拖了一张卡 → 覆盖掉刚才所有外部改动。

**（f）卡片重复的实证**

[#981「Random duplication of every cards in Kanban after moving cards」](https://github.com/community-archive/obsidian-kanban/issues/981)（2024-06，OPEN）——移动卡片后所有卡片随机重复。另见 [#568](https://github.com/community-archive/obsidian-kanban/issues/568)、[#943](https://github.com/community-archive/obsidian-kanban/issues/943)（多个看板并排打开导致空视图/重复）。

**（g）日期格式是「配置」而不是「规范」**

Kanban 的日期不是独立字段，而是写在卡片标题文本里，靠触发符识别（默认 `@`，时间用 `@@`），支持 `@{2026-09-01}`、`@[[2026-09-01]]`、`@[2026-09-01](path)` 三种包裹。而**日期格式本身由用户设置的 moment 格式串决定**（`date-format`，默认 `YYYY-MM-DD`）。

> **对委托方的规则**：**日期格式必须写死在 spec 里（ISO 8601），不能做成用户可配置项。** 否则同一个 TODO.md 在两台机器上会被解析成不同结果——这在 agent 读写场景下是灾难。

**（h）一个正面事实：Kanban 内置了对 Obsidian Tasks emoji 的解析**

`src/parsers/helpers/inlineMetadata.ts` 明确定义了：
```ts
export const taskFields = new Set([
  'priority','start','created','scheduled','due','completion','cancelled','id','dependsOn','repeat',
]);
```
并配有完整的 `EMOJI_REGEXES`。这说明 **Obsidian Tasks 的 emoji 行内格式已经在事实上成为被其他工具消费的社区标准**——这对委托方的格式选型是一条强证据（见 §10.1）。

### 5.2 Obsidian Tasks — 行内元数据语法的事实参考

- 仓库：<https://github.com/obsidian-tasks-group/obsidian-tasks>（★3,973，MIT，**活跃**，最后 push 2026-08-28）
- 文档：<https://publish.obsidian.md/tasks/>

#### 5.2.1 完整 emoji 语法表（源码实证：`src/TaskSerializer/DefaultTaskSerializer.ts`）

| 字段 | 符号 | 兼容别名（正则内） | 值格式 |
|---|---|---|---|
| 优先级 最高 | `🔺` | — | — |
| 优先级 高 | `⏫` | — | — |
| 优先级 中 | `🔼` | — | — |
| 优先级 低 | `🔽` | — | — |
| 优先级 最低 | `⏬` | — | — |
| 开始日期 | `🛫` | — | `YYYY-MM-DD` |
| 创建日期 | `➕` | — | `YYYY-MM-DD` |
| 计划日期 | `⏳` | `⌛` | `YYYY-MM-DD` |
| 截止日期 | `📅` | `📆` `🗓` | `YYYY-MM-DD` |
| 完成日期 | `✅` | — | `YYYY-MM-DD` |
| 取消日期 | `❌` | — | `YYYY-MM-DD` |
| 重复规则 | `🔁` | — | `[a-zA-Z0-9, !]+`（如 `every week`） |
| 完成时动作 | `🏁` | — | `[a-zA-Z]+` |
| 依赖于 | `⛔` | — | 逗号分隔的 id 序列 |
| 任务 ID | `🆔` | — | `[a-zA-Z0-9-_]+` |

示例：
```markdown
- [ ] 修复登录超时 🔺 🆔 abc123 ⛔ xyz789 ➕ 2026-08-01 🛫 2026-08-05 ⏳ 2026-08-10 📅 2026-08-15 🔁 every week
```

#### 5.2.2 值得偷师的工程细节

从源码里能读出几个很务实的决定：

```js
// \uFE0F? allows an optional Variant Selector 16 on emojis.
let source = symbols + '\uFE0F?';
// The regexes end with `$` because they will be matched and
// removed from the end until none are left.
source += '$';
```

1. **`\uFE0F?` 处理 emoji 变体选择符** — 用户从不同平台复制 emoji 时会带隐藏的 VS16 字符，不处理就会解析失败。**这是所有用 emoji 做标记的方案必踩的坑。**
2. **从行尾开始逐个匹配并剥离** — 每个字段正则都以 `$` 结尾，反复从末尾剥离已识别的字段，剩下的就是任务正文。这优雅地解决了「元数据与正文的分隔」问题，**恰好是 §4.2 中 todomd 规范做不到的那点**。
3. **注释里写着「Remove the 'u' flag, to fix parsing on iPadOS/iOS 18.6」** — 跨平台正则的真实伤疤。
4. **`🆔` + `⛔` 提供了任务 ID 与依赖** — 这是纯文本任务系统里少见的、把「稳定引用」做进格式的设计。**委托方若要支持 agent 精确改写、任务依赖，ID 是必需品，这里有现成范式。**

#### 5.2.3 写回定位算法：**委托方最应该直接复刻的一段代码**

源码：`src/Obsidian/File.ts` 的 `findLineNumberOfTaskToToggle()`。当用户在**查询结果**（可能渲染在另一个文件里）中勾选一个任务时，插件必须回到原文件找到那一行。它用了**三级回退**：

```ts
export function findLineNumberOfTaskToToggle(originalTask, fileLines, listItemsCache, errorLoggingFunction) {
  let result = tryFindingExactMatchAtOriginalLineNumber(originalTask, fileLines);
  if (result !== undefined) return result;
  result = tryFindingIdenticalUniqueMarkdownLineInFile(originalTask, fileLines);
  if (result !== undefined) return result;
  return tryFindingLineNumberFromTaskSectionInfo(originalTask, fileLines, listItemsCache, errorLoggingFunction);
}
```

1. **原行号处精确匹配**：`fileLines[n] === originalTask.originalMarkdown`。最快路径。
2. **全文唯一匹配**：若该行文本在整个文件中只出现一次，就用它。（**两条一模一样的任务会让这级失效**——这正是为什么需要 ID。）
3. **回退到 section + listItemsCache** 逐个数 list item；若缓存行号越界则返回 undefined。

**最关键的设计是第四点**：函数签名里有一个 `errorLoggingFunction`，注释说明「which will be called if the found line differs from the original markdown」——即**发现不一致时弹出警告并放弃写入，而不是盲写覆盖**。

> **这与 Obsidian Kanban 的「全文件覆写」形成完全对立的工程哲学，也是委托方应当采用的默认姿态：**
> **宁可拒绝写入并告诉用户，也不要在状态不确定时覆盖用户数据。**

#### 5.2.4 状态机模型（比 checkbox 字符更抽象一层）

Tasks 的状态是一个四元组：**(Status Symbol, Next Status Symbol, Status Name, Status Type)**。

| Symbol | Next Symbol | Name | Type |
|---|---|---|---|
| `space` | `/` | Todo | `TODO` |
| `/` | `x` | In Progress | `IN_PROGRESS` |
| `x` | `space` | Done | `DONE` |
| `-` | `space` | Cancelled | `CANCELLED` |

六种 Status Type：`TODO`、`IN_PROGRESS`、`ON_HOLD`、`DONE`、`CANCELLED`、`NON_TASK`。

**关键抽象**（官方文档原文）：`DONE` 类型决定两件事——「是否在切换时添加/移除完成日期」以及「是否生成递归任务的下一个实例」。

> **这是本调研中最优雅的一个设计决定**：**语义挂在「类型」上，而不是挂在「字符」上。** 用户可以自定义任意 checkbox 字符（`- [/]`、`- [!]`、`- [?]`），只要声明它属于哪个类型，所有行为自动正确。`Next Status Symbol` 则定义了点击时的状态环。
>
> 委托方的「TODO/DOING/DONE 列」应当采用同样的两层结构：**显示名（可自定义、可中文）→ 语义类型（固定枚举）**。这同时解决了 §5.1.6(c) 的本地化陷阱。

另一个细节：`CANCELLED` 是后来才独立出来的，官方说明原文——之前只能标成 DONE，"but this was misleading. It could also have added a Done date to them, which was even more misleading."

#### 5.2.5 递归任务的写回行为

```markdown
- [ ] take out the trash 🔁 every Sunday 📅 2021-04-25
```
勾选后，文件变成**两行**：
```markdown
- [ ] take out the trash 🔁 every Sunday 📅 2021-05-02
- [x] take out the trash 🔁 every Sunday 📅 2021-04-25 ✅ 2021-04-24
```
新实例默认插在**原任务的上一行**。规则必须以 `every` 开头。已知限制（官方明说）：递归任务必须至少有 Due / Scheduled / Start 之一，否则不参与日期查询，「has been seen to confuse a number of users」。

#### 5.2.6 性能：一个已被解决的教训

[#697「Investigate whether Tasks plugin affects editing speed in large vaults — **Answer: it does**」](https://github.com/obsidian-tasks-group/obsidian-tasks/issues/697)（94 条评论，已关闭）。根因原文：「the way that Tasks tracks edits to Obsidian notes is to **reload all files**」；解决方向：「Refactor the code so that when a note is edited, **only that note is re-read, and not the whole vault**.」

现在 `src/Obsidian/Cache.ts` 通过订阅 `metadataCache.on('changed', ...)` 做**按文件增量重解析**。但查询求值仍是「每个 ```tasks 块渲染时对全量任务过滤」，因此宽泛查询在几万条任务下仍会慢；官方提供 `limit`、`explain`、Presets 收敛开销。

#### 5.2.7 规模与治理：与 Kanban 的决定性差异

- **官方下载量 4,130,627** —— 比 Kanban（2,607,917）高约 **58%**，尽管 star 数略低。（**评估 Obsidian 插件不能只看 star**。）
- manifest 的 author 字段是 `"Clare Macrae and Ilyas Landikov (created by Martin Schenck)"` —— **原作者已成功移交给团队**，这与 Kanban 找不到接手人形成直接对照。
- 提供公开 API：`app.plugins.plugins['obsidian-tasks-plugin'].apiV1`。
- 支持 Dataview 风格替代语法（`[due:: 2024-01-01]`），在设置里切换。注意 done 对应的 key 是 **`completion::`** 而非 `done::`。
- 无通知能力（相关 PR #1925 已关闭未合入）。

#### 5.2.8 与 Kanban 的关键对比

Obsidian Tasks **不重写文件结构**——它的模型是「任务是行，行属于任意文件的任意位置」，勾选完成时只改写那一行，且不一致就放弃并告警。这就是它没有 #1082 这类事故、且能保持活跃维护的结构性原因。

> **两个插件的对照本身就是本调研最强的一条结论**：
> **「行级模型 + 就地改写」的 Tasks 活得好好的；「文档级模型 + 全文件序列化」的 Kanban 事故不断且已停摆。**
> 委托方要做的恰恰是 Kanban 形态（看板视图），因此必须借用 Tasks 的写入哲学，才能避开 Kanban 的命运。

---

## 6. Tasks.md — 用「目录即泳道」绕开格式问题

- 仓库：<https://github.com/BaldissaraMatheus/Tasks.md>（★2,187，MIT，活跃，最新版本 3.3.0 / 2026-03）
- 技术栈：SolidJS + Koa，Docker 单镜像自托管

### 6.1 数据模型：文件系统即结构

README 说得很直白：

> "Every lane you add within the app is a **directory** in your filesystem and every task is **file**."

```
/tasks/
├── Backlog/
│   └── 某个任务.md
├── Sprint/
└── Done/
    └── Something something.md
```

**这是一个非常聪明的规避策略**：卡片在哪一列，由它所在的目录决定，**不需要任何 markdown 内的元数据**，也就完全不存在「解析/序列化破坏文件」的问题。拖动卡片 = `mv` 一个文件。排序、标签、日期则存在 `/config` 目录里。

### 6.2 优缺点

**优点**：格式零侵入（每个 .md 就是纯粹的卡片正文，可以被任何编辑器/Obsidian 直接打开）；写回操作是原子的文件移动；PWA 可安装；子目录可作为独立项目。

**缺点（对委托方而言是致命的）**：
- **一个任务一个文件** → 一个 200 任务的项目就是 200 个文件，git log 噪音极大，无法「一眼看完整个 TODO」，`grep` 也不再是单文件操作。
- 列的顺序、卡片在列内的顺序无法由文件系统表达，只能存在 side-car 配置里 → **事实源又分裂了**。
- 无 API、无通知、无移动端原生应用。
- 作者明确声明这是「low maintenance project」，「scope of features and support are purposefully kept narrow」，且**拒绝显著扩大范围的功能请求**。商业化仅为一次性赞助。

### 6.3 对委托方的意义

它验证了「目录即列」这条路可行且实现简单，但也暴露了代价：**单文件 TODO.md 的可读性/可 grep 性/单次 diff 完整性，是委托方相对它的核心优势，不应放弃。**

---

## 7. Taskwarrior 与 org-mode：两条走到尽头的路

> 本节要点由并行调研补充核实，结论已交叉验证。

### 7.1 Taskwarrior — 为了同步，放弃了纯文本

- <https://github.com/GothenburgBitFactory/taskwarrior>（★6,026，MIT，活跃）

**最关键的事实：Taskwarrior 3.0.0（2024-03-24）把存储从纯文本 FF4 格式的 `pending.data` / `completed.data` 换成了 SQLite（`taskchampion.sqlite3`，由 Rust 写的 TaskChampion 核心提供）。**

后果与争议（均有一手证据）：
- **没有自动迁移**：必须在 2.x 下 `task export` 再到 3.x `task import`。滚动发行版（如 Arch）用户在无感升级后遭遇「数据消失」。
- 官方 Discussion 中有用户直言**纯文本正是他们当初选择 Taskwarrior 的首要理由**（GitHub Discussions #3291 / #3208）。
- 官方文档在 3.0 后**明令禁止**再用 Syncthing / rsync / git 同步数据目录——因为 SQLite 文件按字节同步会损坏。
- 同步机制是 **operational transformation（OT，不是 CRDT）**：Create / Delete / Update 三类 operation + 服务端线性版本链，客户端只能 rebase 不能 force push。
- 老的 `taskd`（taskserver）仓库**已 archived**，由 TaskChampion sync server 取代。

**争议的一手证据**（[Discussion #3291](https://github.com/GothenburgBitFactory/taskwarrior/discussions/3291) / [#3208](https://github.com/GothenburgBitFactory/taskwarrior/discussions/3208)）：

> "I really would like that TW3 can use plain text files instead of binary data… **plain text files was, by far, the most important point for using taskwarrior instead of other to-do programs**."

> "But using Git to sync and version control a binary database file is almost pointless, so I would prefer to keep my Taskwarrior data in plaintext/JSON that Git works well with."

维护者 djmitche 的回应（承认这是方向性转变，且道出了根因）：

> "I agree that 3.0 represents a change in direction for the project… **this project has been very quiet for about a decade**, with some serious deficiencies especially in the sync support… **I've run out of steam on this project after a few years' work.**"

**任务模型**（值得借鉴的部分）：description / project / tags / priority(H,M,L) / due / scheduled / wait / until / depends / annotations / **UDA（用户自定义属性）** / **virtual tags**（如 `+OVERDUE` `+READY`，由规则自动推导而非手工标注），以及一套可配置系数的 **urgency 计算公式**，用于自动排序「接下来该做什么」。

**一个必须偷师的数据模型设计**（[TaskChampion Task Representation](https://taskwarrior.org/docs/task/)）：任务是 string→string 的 key/value map。关键在于 key 的设计**刻意规避 read-modify-write 冲突**——tag 不存成 `tags: "a,b,c"`，而是拆成 `tag_<tag>` 独立 key。官方给的理由：

```
Update(uuid, "tags", "oldtag,newtag1", "2020-11-23T14:21:22Z")
Update(uuid, "tags", "oldtag,newtag2", "2020-11-23T15:08:57Z")
→ 结果是 oldtag,newtag2，但用户本意是 oldtag,newtag1,newtag2
```

同理 `dep_<uuid>`、`annotation_<timestamp>` 也是独立 key。

> **对委托方的直接启示**：在 TODO.md 里，**一个任务的多值字段（标签、依赖）如果写成一个逗号分隔的串，两端并发修改时 git 会合并出错误结果**。若能拆成可独立增删的行内 token（`#tag1 #tag2` 而非 `tags:tag1,tag2`），git 的行内合并成功率会显著提高。这是「格式设计直接决定合并质量」的最佳范例。

**urgency 公式**（[官方文档](https://taskwarrior.org/docs/urgency/)）是多项式求和，每项一个可配系数：

```
urgency.due.coefficient          12.0  # 临近/超期
urgency.blocking.coefficient      8.0  # 阻塞了其他任务
urgency.uda.priority.H.coefficient 6.0
urgency.age.coefficient           2.0
urgency.waiting.coefficient      -3.0
urgency.blocked.coefficient      -5.0
```

官方对它的自我定位很克制：「这是**近似**，如同 Apgar 评分或 PageRank，不可能完美表达每个人心中的重要性。」tags/annotations 项还带递减因子（避免堆 tag 刷分）。

**hooks**（[官方文档](https://taskwarrior.org/docs/hooks/)）：采用「Git 模型」——**无需配置，放对位置 + 加执行位即生效**（`~/.task/hooks/`，命名 `on-add`、`on-add.01` 等，按文件名排序执行；去掉执行权限即禁用）。

| 事件 | 时机 | 能力 |
|---|---|---|
| `on-launch` | 初始化后、处理前 | 可阻止启动 |
| `on-add` | 新任务处理后、保存前 | 批准 / 拒绝 / 修改 |
| `on-modify` | 修改处理后、保存前 | 批准 / 拒绝 / 再修改（**完成与删除也算 modify**） |
| `on-exit` | 全部处理后、输出前 | 只能补 footnote，不能改任务 |

I/O 约定极简：**每行一个 JSON 对象**（同 `task export` 格式），任务 JSON 必须占且仅占一行。退出码 0 = 放行，非 0 = 拒绝且反馈文本必填。`on-modify` 的 stdin 是两行（原任务 + 修改后任务）。

> **这是一个极简且好用的扩展点范式，对委托方的「agent 集成」很有参考价值**——比插件 API 轻量一个数量级，且天然适合 AI agent 和 shell 脚本调用。委托方在 MCP 之外值得提供这一层。

**移动端是最大短板**：官方 Android 客户端早已 DISCONTINUED；社区主力 `bgregos/foreground`（★258）2024-06 后停更；iOS 侧只剩 `taskchamp`（★77）还活着。

**商业化**：无。仅 GitHub Sponsors / 捐赠。

**生态 UI**：`kdheepak/taskwarrior-tui`（★2,114，非常活跃，最后 push 2026-08-30）是最健康的一环 —— **说明「优秀的 TUI/GUI 前端」确实有需求，只是没人靠它赚钱。**

### 7.2 org-mode — 守住了纯文本，把冲突成本转嫁给用户

**语法（纯文本任务管理的能力上限）**：

```org
#+TODO: TODO NEXT WAITING(w@/!) | DONE CANCELLED(c@)

* TODO [#A] 修复登录超时                                    :bug:backend:
  SCHEDULED: <2026-08-10 Mon> DEADLINE: <2026-08-15 Sat>
  :PROPERTIES:
  :ID:       abc-123
  :Effort:   3h
  :END:
  :LOGBOOK:
  - State "TODO"  from "NEXT"  [2026-08-09 Sun 10:00]
  :END:
```

- **自定义状态机**：`#+TODO:` 序列，`|` 左边是「未完成」态，右边是「完成」态。`(w@/!)` 这类标记控制状态切换时是否要求记笔记 / 记时间戳 → **状态转换本身被记录进 `:LOGBOOK:`**。这是本调研中唯一把「状态变更历史」写进纯文本的方案，对委托方的「同步事实」「审计 agent 改了什么」有直接参考价值。
- 优先级 `[#A]`、标签 `:tag:`、`:PROPERTIES:` drawer 存任意 key-value（含稳定 `:ID:`）。
- **repeater 三种语义**——本调研中最值得直接抄走的单点设计（[官方 Repeated tasks](https://orgmode.org/manual/Repeated-tasks.html)）：

| 形式 | 语义 | 场景 |
|---|---|---|
| `+1w` | 严格按周期从**原日期**推移 | 交房租：漏了 3 个月仍欠 3 次，必须补 |
| `++1w` | 推移至少一个周期，并继续推到**未来**，且**保持原有星期几** | 给父亲打电话：漏了 3 周不该一天打 3 次 |
| `.+1w` | 从**完成时刻**起算一个周期 | 换烟感电池：应该在上次做完后固定间隔 |

官方原文示例：

```org
** TODO Call Father
   DEADLINE: <2008-02-10 Sun ++1w>
   Marking this as DONE shifts the date by at least one week, but also
   by as many weeks as it takes to get this date into the future.
   However, it stays on a Sunday, even if you called and marked it
   done on Saturday.

** TODO Check the batteries in the smoke detectors
   DEADLINE: <2005-11-01 Tue .+1m>
   Marking this as DONE shifts the date to one month after today.
```

**Taskwarrior 和绝大多数主流待办应用都没有这么完整的重复任务建模。** 委托方若要支持重复任务，这三分语义是现成的正确抽象。

- **`SCHEDULED` 的含义是高频误解点**，官方专门警告：

> "Scheduling an item in Org mode should **not** be understood in the same way that we understand scheduling a meeting… In Org mode, scheduling means setting a date when you want to **start working on** an action item."

即 `SCHEDULED` ≈ 开始处理日（未完成会自动顺延到今天），`DEADLINE` ≈ 截止日。约会应该用普通时间戳。

- **优先级的关键限制**：官方明说「Priorities make a difference only for sorting in the agenda. **Outside the agenda, they have no inherent meaning to Org mode.**」即 org 只有一维人工优先级，没有 Taskwarrior urgency 那样的多因子自动计算。**理想方案可能是「TW 的自动计算 + org 的可见语法」。**

**性能**：agenda 视图需要 visit 每一个 agenda file 并在其中启用 org-mode，文件数上千时可达数分钟级。这是「聚合视图必须扫描全部源文件」的固有代价——**委托方用 SQLite 做缓存正是对这个问题的正确答案。**

**移动端与冲突（本调研最重要的一条警示）**：

`orgzly-revived/orgzly-android-revived`（★1,116，2026-08 仍在每周构建）是当前 Android 事实标准。但它的冲突处理是硬伤，[官方文档](https://www.orgzlyrevived.com/docs)原文：

> "If you modify a notebook in Orgzly and at the same time (before performing a sync) you modify its linked remote notebook, next sync will leave the notebook in a conflicted state."
> - `Force Load` to import remote notebook and overwrite the local version
> - `Force Save` to export local notebook and overwrite the remote version
>
> **"There is currently no other way to resolve a conflict within Orgzly itself."**

即**只有二选一的整文件覆盖，没有 diff、没有 merge**。维护者明确拒绝内置 diff（issue #292：「This is a task management app. There are specialized tools for pretty much anything else.」）。另一个结构性限制：「**something must tell Orgzly to sync**」——不监听远端变化，Syncthing 改了文件 Orgzly 也不知道。

**根因（决定性的一条）**：orgzly 原作者在 [issue #35](https://github.com/orgzly/orgzly-android/issues/35) 中解释，真正的 merge 需要**给每个 note 分配 ID**（像 MobileOrg 那样），而他一直刻意回避引入 ID。

> **这与 Taskwarrior 用 uuid + `tag_<x>` 拆分来支撑自动合并形成完美对照，共同指向同一条结论：**
> **稳定标识符是「纯文本能否自动合并」的分水岭。想要纯文本 + 自动合并，就必须在文本里嵌入稳定 ID。**
>
> 这对委托方是一条强制性设计要求，而不是可选项。

真实用户痛点（[HN 讨论](https://news.ycombinator.com/item?id=39393049)）：

> "I used orgzly+syncthing for a while but **the overhead of manually resolving conflicts between my laptop and phone were not worth it in the end.**"

社区流行的规避性工作流很说明问题：**手机端只写一个 inbox 文件，之后在电脑上 refile**——用「限制单一写入点」人为规避冲突。

**用户规模的真实锚点**（维护者在 issue #104 中透露的罕见数据）：Play Store 约 **1,100 个**唯一账号安装，其中约 320 个链接了 Dropbox。**这个量级说明整个 org-mode 移动端生态规模相当小。**（注意 revived 仓库 1,116★ 低于原版 2,813★，因为 star 沉淀在老仓库——**评估活跃度不能只看 star**。）

**beorg（iOS）—— 本调研中唯一商业化成功的样本**：
- 闭源商业产品，[beorg.app](https://www.beorg.app/)，活跃（文档更新至 2025-11）
- 同步：iCloud（唯一支持推送更新）、Dropbox、WebDAV、Box、以及「选择其他 App 的文件夹」（可对接 Working Copy 走 git）
- 功能：周 agenda、Apple Watch app + 复杂功能、Siri、通知、模板、PDF 导出、**内置 Scheme 脚本语言**、Shortcuts URL scheme
- **商业模式：Premium 订阅（7 天试用，月/年）+ 单项内购**（Saved Searches、加密、Box 同步等）
- 官网主打「Your data. Your choice.」「Future proof. **View and edit beorg files in any text editor.**」

> **beorg 证明了纯文本任务管理可以做成付费产品，前提是把「文件格式开放」和「体验封闭精致」分开——数据用开放格式，体验做闭源精品。** 这与 Taskwarrior「全开源、零收入、维护枯竭」形成直接对照，**也正是委托方要商业化时应当采取的姿态**。

**为何没破圈**：绑定 Emacs（用户上限≈Emacs 用户上限）、语法复杂度高且无渐进路径、移动端每家都要重新实现一遍解析器和语义、同步冲突转嫁用户、性能不随规模扩展。

**看板视图在 org 生态是边缘需求**：`org-super-agenda`（★1,535）是 agenda 增强的事实标准，而 `org-kanban` 仅 ★267。**org 用户的心智模型是 agenda（时间/上下文驱动）而非 kanban（流程驱动）。** 这提示委托方：看板不是唯一正确的视图，「今天该做什么」的时间视图可能同样重要。

### 7.3 两者对照给委托方的结论

> **Taskwarrior 为了可靠同步牺牲了纯文本，流失了它最初的用户；org-mode 守住了纯文本，把冲突解决的成本全部转嫁给了用户。**
>
> 两者「没破圈」的共同根因是**移动端与同步**，而不是功能不足。

委托方的架构（TODO.md 为唯一事实源 + SQLite 仅作缓存 + 用 git 承载同步与冲突）在理论上同时避开了这两个陷阱：
- 事实源是文本 → 不会有 Taskwarrior 3.0 的信任崩塌；
- 同步用 git → 有成熟的行级三方合并，不必自研 OT/CRDT，也不会退化成 orgzly 的整文件覆盖。

**但这个优势能否成立，完全取决于写回是否是行级最小改动。**（见 §5.1.2、§9）

---

## 8. git 原生任务与终端看板（简述）

### 8.1 git-bug

- <https://github.com/git-bug/git-bug>（★10,018，GPL-3.0，活跃）
- 本调研中 star 最高的项目。

核心设计（README）：把 issue、评论、用户**作为 git object 直接嵌入仓库**——注意是 objects，**不是文件**（"embeds issues, comments, and more as objects in a git repository (_not files!_)"）。因此工作区里看不到任何东西，靠 `git push` / `git pull` 分发，天然离线优先、天然分布式。

- 特性：CLI / TUI / Web 三种界面；毫秒级查询；有 GitHub / GitLab 双向 bridge。
- **与委托方构想的根本分歧**：git-bug 的数据**不在工作区**，因此既不可 `grep`、也不能被 AI agent 直接读到、更不会出现在 code review 的 diff 里。委托方「todo 随 git 提交、且是仓库里一个能被人和 agent 直接读写的文件」的主张，恰恰是 git-bug 主动放弃的路线。
- **可借鉴**：它的 bridge 模型（本地为主、与 GitHub/GitLab 双向同步）非常成熟，且证明了 ★10k 的关注度依然可以完全没有商业化——**这个品类的用户对「免费开源」的期待极强。**

### 8.2 终端看板

| 项目 | 数据 | 状态 |
|---|---|---|
| [dooit](https://github.com/dooit-org/dooit) ★2,941 | **SQLite**（v3 起），Python 配置文件 | 活跃 |
| [taskell](https://github.com/smallhadroncollider/taskell) ★1,783 | markdown / JSON，可连 Trello / GitHub Projects | **已 archived（2023-10）** |
| [kanban.bash](https://github.com/coderofsalvation/kanban.bash) ★956 | **CSV**，单文件 shell 脚本 | 活跃 |
| [taskwarrior-tui](https://github.com/kdheepak/taskwarrior-tui) ★2,114 | Taskwarrior 后端 | 非常活跃 |

一个反直觉但重要的观察：**这一层里没有一个用 markdown 做主存储。** dooit 用 SQLite，kanban.bash 用 CSV，taskell 用 markdown 但已归档。原因不难理解——**markdown 对机器来说是最难可靠往返（round-trip）的格式**，CSV 和 SQLite 都没有「保留用户原始排版」的负担。

> 这条观察对委托方是个提醒：**选择 markdown 作为事实源，是一个明确的「用工程复杂度换用户体验与生态兼容」的交易。** 这个交易可以是对的，但必须清楚地知道自己买的是什么、付的是什么。

---

## 9. TODO.md 作为事实源的已知坑清单

按「会不会毁掉用户数据」和「会不会毁掉产品口碑」排序。

### 坑 1（最致命）：格式保真 —— 全文件重写会静默吞掉用户内容

- **实证**：Obsidian Kanban `boardToMd()` 从 AST 重建整个文件；用户的表格、图片嵌入被删除、标题层级被强制降为 h2（[#1082](https://github.com/mgmeyers/obsidian-kanban/issues/1082)，OPEN 无回复）。
- **为什么委托方一定会撞上**：TODO.md 是「唯一事实源」，用户必然往里写说明段落、链接、代码块、`<!-- -->` 注释、甚至 mermaid 图。
- **对策**：
  1. **保守写入（surgical edit）**：解析结果保留每个任务的 `line` / `col` / `rawText`；写回时按行替换，未变更的字节**原样保留**（包括行尾空白、缩进风格、换行符类型）。
  2. **往返测试作为 CI 红线**：`parse(serialize(parse(x))) === parse(x)` 不够，必须断言 **`serialize_unchanged(x) === x` 逐字节相等**。用大量真实脏数据（含 HTML 注释、表格、嵌套列表、代码块内的假任务、CRLF、BOM、全角字符）做 fuzz。
  3. **代码块与行内代码必须排除**：imdone 专门实现了 `isWithinInlineCode()`；markdown 的 ``` 围栏里出现 `- [ ]` 是极常见的（写文档时）。
  4. **写入前校验 + 失败即中止**：若序列化结果与原文件的差异超出「预期变更的行集合」，直接拒绝写入并报错，而不是覆盖。

### 坑 2：双向同步冲突 —— 全文件重写会让 git 合并失效

- **实证**：Obsidian Kanban [#439](https://github.com/mgmeyers/obsidian-kanban/issues/439)，手机端旧版本反向覆盖桌面端新版本导致数据丢失；2024 年仍有用户报告卡片被复制成两份。[#981](https://github.com/community-archive/obsidian-kanban/issues/981)：移动卡片后所有卡片随机重复。
- **机理**：两端各自全文件重写 → 每一行的 diff 都被标记为变更 → git 三方合并要么整块冲突、要么错误自动合并成重复条目。
- **对策**：
  1. 行级最小改动（同坑 1 的对策），让 git 的三方合并真正能工作。
  2. **稳定 ID 是合并的前提**——这一条有两个独立方向的强证据：**Taskwarrior** 用 uuid 且把 tags 拆成 `tag_<x>` 独立 key 来支撑自动合并；**orgzly 作者明确承认**「真正的 merge 需要给每个 note 分配 ID，而我一直回避这么做」，这正是 org 移动端只能整文件覆盖的根因。参考 Obsidian Tasks 的 `🆔` 作为语法载体。
  3. **多值字段不要写成逗号分隔串**。Taskwarrior 的 `tag_<tag>` 拆分设计证明：`tags:a,b` 这种写法在并发修改时会丢数据；`#a #b` 这种可独立增删的 token 序列，git 行内合并成功率高得多。
  4. **拖动排序不要重排整列**。用 imdone 的分数式 order（`(end - start) / n`），保证移动一张卡片只改一行。
  5. **纯 UI 状态绝不入文件**。Obsidian Kanban 把 `list-collapse`（列折叠状态）写进 `%% kanban:settings %%`，导致**用户只是折叠一列就改写了文件**，多端各自折叠不同列即产生冲突。视图状态应放 SQLite 缓存或 gitignore 的本地文件。
  6. **提供冲突后的修复入口**：git 留下的 `<<<<<<<` 冲突标记会让解析器失败。必须优雅降级（检测到冲突标记时进入只读模式并明确提示），而不是崩溃或把标记当成任务解析。

### 坑 3：多设备 / 多进程并发写

- **实证 A**：pjeby 在 Kanban [#178](https://github.com/mgmeyers/obsidian-kanban/issues/178) 中定位到的状态回环——外部文件变更触发 `setExternal` → React 重渲染 → `useEffect` 误触发 `setInternal` → **把外部的改动又改回去了**。
- **实证 B（更常见）**：Kanban [#380](https://github.com/community-archive/obsidian-kanban/issues/380) / [#399](https://github.com/community-archive/obsidian-kanban/issues/399) / [#644](https://github.com/community-archive/obsidian-kanban/issues/644)——在纯文本视图改了文件，看板视图不刷新；此时在看板上做任何操作，**内存里的旧状态会覆写磁盘上的新内容**。
- **这正是委托方最危险的场景**：用户或 agent 在编辑器里改了 TODO.md，GUI 仍持有旧状态，用户随手拖一张卡 → 刚才的所有外部改动被抹掉。
- **场景**：委托方的架构里同时存在 GUI、file watcher、curl API、MCP server、AI agent 五个潜在写入者。这比 Obsidian 的情况复杂得多。
- **对策**：
  1. **单一写入串行化**：所有写入走同一个队列，禁止并发写同一文件。
  2. **写前校验 mtime + 内容 hash**：若磁盘上的文件已不是上次读取的那份，必须重新解析并 rebase 本次变更，而不是直接覆盖（乐观锁）。
  3. **原子写**：写临时文件 + `rename()`，避免 watcher 读到半截文件。
  4. **自写忽略**：watcher 必须能识别「这次变更是我自己造成的」，否则会触发无谓的重解析甚至回环。
  5. **debounce 但不能丢事件**：编辑器保存常触发多次 fs 事件。

### 坑 4：不可见字符与编码

- **实证**：todomd 规范要求任务行尾**两个空格**（§4.2）——prettier、`.editorconfig` 的 `trim_trailing_whitespace`、绝大多数编辑器的保存钩子都会把它删掉。
- **实证**：Obsidian Tasks 源码里的 `\uFE0F?`（emoji 变体选择符）和「移除正则 `u` flag 以修复 iPadOS 解析」的注释。
- **对策**：**语义绝不承载在不可见字符上**。emoji 方案要处理 VS16。要处理 CRLF / LF 混排（imdone 用了 `eol` 库）、BOM、全角标点。

### 坑 5：单文件的可扩展性

- **实证**：todo.txt 生态用 `done.txt` 把完成项分离出去，正是为了防止主文件无限膨胀。
- **问题**：一个活跃项目跑一年，TODO.md 里的已完成任务会淹没未完成任务，且每次 git diff 都要加载全文。
- **对策**：设计之初就想好归档策略（`TODO.md` + `TODO.archive.md`，或 Kanban 式的 `## Archive` 区段），并让归档动作本身是一次干净的、可审阅的 diff。

### 坑 6：格式规范的碎片化

- **实证**：todo.txt 的 `due:` / `rec:` / `t:` 是社区约定而非规范，导致客户端间支持不一致（§3.2）。
- **对策**：委托方既然要做 MCP/skills 让多个 agent 读写，**必须发布一份带版本号的形式化 spec + 一个官方校验器（lint）**，否则 Claude Code 和 Codex 会各写各的方言。这一点在 AI 场景下比在人类场景下严重得多——**LLM 会自信地发明看起来合理的语法。**

### 坑 7：TODO 注释导入的双事实源问题

- 见 §2.4。导入后源码里那行注释的归宿必须有明确答案，否则「代码里的 TODO」和「TODO.md 里的条目」会成为两个互相漂移的事实源——**这与「唯一事实源」的产品主张直接冲突。**

### 坑 8：把语义挂在「可变的显示文本」上

- **实证**：Obsidian Kanban 的 `**Complete**`（完成列标记）和 `## Archive`（归档区）都是 `t()` 本地化字符串 —— **切换 UI 语言可能导致归档区不再被识别**；同时卡片正文里恰好出现 `**Complete**` 就会误判。归档区还必须紧跟 `***` 之后，手写时极易出错。
- **实证**：Kanban 的列标题 `## Sprint (2)` 中的 `(2)` 会被当成 WIP 上限**从标题里剥离**。
- **对策**：语义标记必须是**语言无关的稳定标识符**。列的「显示名」（可中文、可随便改）与「语义类型」（固定枚举 `todo`/`doing`/`done`）必须分离——**这正是 Obsidian Tasks 的 Status Type 设计**（见 §5.2.4），它让用户能自定义任意 checkbox 字符而行为始终正确。

### 坑 9：把格式做成「用户可配置项」

- **实证**：Obsidian Kanban 的日期格式由用户设置的 moment 格式串决定（`date-format`）。后果是**同一个文件在两台配置不同的机器上被解析成不同结果**。
- **对策**：**日期一律 ISO 8601，格式写死在 spec 里，不做成配置。** 在多设备 + 多 agent 读写的场景下，可配置的格式等于没有格式。

---

## 10. 可借鉴的格式 / 交互设计

### 10.1 格式层

| 设计 | 来源 | 为什么值得抄 |
|---|---|---|
| **GFM `- [ ]` 作为地基** | 通用 | 唯一有真实网络效应的语法；GitHub 上直接可读可勾选 |
| **从行尾逐个剥离元数据字段** | Obsidian Tasks（每个字段正则以 `$` 结尾，匹配后剥离，剩余即正文） | 优雅解决「元数据 vs 正文」的分隔歧义，无需分隔符 |
| **稳定任务 ID + 依赖** | Obsidian Tasks `🆔` / `⛔` | 合并、跨文件引用、agent 精确改写的前提。**orgzly 与 Taskwarrior 从正反两面共同证明这是分水岭** |
| **状态语义挂「类型」不挂「字符」** | Obsidian Tasks 的 (Symbol, Next Symbol, Name, **Type**) 四元组 | 用户可自定义显示名与字符，行为由固定枚举 Type 决定；同时解决本地化陷阱 |
| **多值字段用可独立增删的 token** | Taskwarrior `tag_<x>` 拆分 | `#a #b` 比 `tags:a,b` 的 git 行内合并成功率高得多 |
| **重复任务三分语义** | org-mode `+1w` / `++1w` / `.+1w` | 严格周期 / 追到未来但保持对齐 / 从完成时刻起算，是经长期打磨的正确抽象 |
| **分数式排序 order** | imdone `(end - start) / n` | 拖动一张卡只改一行，git diff 极小 |
| **通用 `key:value` 槽位 + 配置化渲染** | imdone `meta` + `urlTemplate`；todo.txt `key:value` | 不必为每个新字段改格式；`pr:123` 可渲染成链接 |
| **配置藏进 markdown 注释** | Obsidian Kanban `%% kanban:settings ... %%` | 每个文件自带看板配置（列顺序、WIP 上限），不需要 side-car 文件，且不污染渲染 |
| **状态变更写进 LOGBOOK** | org-mode `:LOGBOOK:` + `(w@/!)` | 「同步事实」按钮和 agent 审计的天然载体：谁在什么时候把它从 X 改成了 Y |
| **完成项归档分离** | todo.txt `done.txt`；Kanban `## Archive` | 控制主文件体积 |
| **`sort`/`grep` 友好** | todo.txt 的设计哲学 | 开发者会用命令行操作这个文件，格式应当尊重这一点 |

### 10.2 交互层

1. **「token 即状态」——改状态就是改一个词**（imdone）。用户在编辑器里手改 `TODO` → `DOING` 与在看板里拖动是完全等价的操作。这让「文件是事实源」不只是技术声明，而是**可感知的用户体验**。

2. **列 = `## 标题`，卡片 = 列表项**（Obsidian Kanban / todomd 一致）。这是唯一被两个独立方案收敛到的约定，用户直觉成本最低，且在 GitHub 上渲染出来就是可读的分组清单。委托方应沿用。

3. **`**Complete**` 式的列行为标记**（Obsidian Kanban）。用一个 markdown 内的标记声明「移入此列的卡片自动标记完成」——把**列的行为**也写进纯文本，而不是藏在应用配置里。这对「AI agent 需要理解看板语义」尤其重要：agent 读文件就能知道规则。

4. **三阶段渲染 + 内容 hash 缓存**（Obsidian Kanban 的性能教训）。解析 → 预渲染 → 挂载，且内容未变则不重渲染。委托方要求「启动 < 2 秒」，应直接采用：**SQLite 缓存供首屏秒开，后台增量校验文件**。

5. **增量解析**（pjeby：只处理列标题和变更行，可从 250ms 降到 <1ms）。file watch 触发时不要全量重解析。

6. **hooks 的极简接口**（Taskwarrior：stdin/stdout 传 JSON 行）。相比插件 API，这个形态对 AI agent 和脚本极其友好，实现成本也低。委托方的 MCP 集成之外，值得提供这一层给不写 MCP 的用户。

7. **virtual tags**（Taskwarrior：`+OVERDUE` `+READY` 由规则自动推导）。不写进文件、由引擎实时计算的标签，可以在不污染事实源的前提下提供丰富的筛选能力。

8. **imdone 的血泪教训：缩短 time-to-first-value**。作者自己的结论是产品失败的主因不是价格而是「首次配置感觉沉重且有风险」。委托方的「打开一个仓库，如果有 TODO.md 立刻看到看板；没有就一键生成」必须做到零配置。

### 10.3 定位建议（基于市场证据）

- **不要把「markdown 看板」当作卖点**——imdone 十二年、Obsidian Kanban 4.5k star 都证明了这件事本身不构成付费理由。
- **付费理由在「上下文不丢失，人和 agent 能接着干」**——这是 imdone 在 2025 年靠付费实验找到的答案，也正是委托方「同步事实」按钮 + MCP/skills 的价值主张。委托方与 imdone 会正面相遇，但 imdone 已把重心移向 Jira 团队场景，**本地优先 + agent 原生的单机开发者场景仍有空档**。
- **Obsidian Kanban 的停摆（600 open issues、最后一次代码修复 2024-06）留下了一批正在找替代品的用户**，且他们已经被教育过「markdown 看板」这个概念——这是一个现成的冷启动人群。
- **警惕这个品类对「免费开源」的强预期**：git-bug ★10k 零商业化、Sleek ★2k 靠众筹交商店年费、Tasks.md 作者主动限制范围、Taskwarrior 靠捐赠维持到维护者力竭。**商业化必须建立在「团队/AI 工作流」而非「个人任务管理」上。**

- **唯一被验证的商业化姿态来自 beorg**：**数据格式完全开放（可用任何文本编辑器打开），产品体验闭源且精致，按订阅收费。** 这与委托方「TODO.md 是纯文本、随 git 走、无锁定」+「Tauri 客户端体验 + agent 集成」的组合完全同构。**开放格式不是商业化的障碍，反而是这个品类里唯一能建立信任的前提**——因为这批用户购买的正是「我的数据永远不会被你绑架」。

- **一条来自 org 生态的战术提醒**：`org-super-agenda`（★1,535）远比 `org-kanban`（★267）流行，说明**看板不是这个人群唯一甚至首要的视图需求**。「今天/本周该做什么」的时间聚合视图值得与看板同等对待。

- **Taskwarrior 的教训值得写在墙上**：一个纯靠捐赠的基础设施型开源工具，最终「以用户数据事故的形式支付了欠债」——十年停滞 → 一次仓促重写 → 无迁移路径 → 用户数据丢失 → 信任崩塌。**可持续收入不是贪心，是对用户数据负责的前提。**

---

## 附：核心数据速查

| 项目 | Star | License | 最后更新 | 存储 | 双向回写 | 商业化 |
|---|---|---|---|---|---|---|
| imdone-core | 151 | MIT | 2026-08 | md / 代码注释 | ✅ 改 token + order | $9/月个人，$18/用户团队 |
| Obsidian Kanban | 4,483 | GPL-3.0 | 2024-06（实质停摆，作者公开找接盘） | md（`## ` 列 + `- [ ]`） | ⚠️ 全文件重写，有数据丢失 | 无（已 Remove funding） |
| Obsidian Tasks | 3,973 | MIT | 2026-08（活跃，已团队化） | md 行内 emoji | ✅ 行级 + 三级定位 + 不一致即告警 | 无 |
| Tasks.md | 2,187 | MIT | 2026-03 | 目录=列，文件=卡 | ✅ 文件移动 | 一次性赞助 |
| todo.txt-cli | 6,166 | GPL-3.0 | 2026-08 | todo.txt | ✅ 行级 | 无 |
| Sleek | 2,027 | MIT | 2026-08 | todo.txt | ✅ | 赞助/众筹 |
| Taskwarrior | 6,026 | MIT | 2026-08 | **SQLite（3.0 起）** | — | 无 |
| git-bug | 10,018 | GPL-3.0 | 2026-07 | git objects（非文件） | ✅ | 无 |
| Todo Tree | 1,816 | — | 2024-04（停更） | 无（只读扫描） | ❌ | 无 |
| todomd/todo.md | 284 | 无 | 2023-08（遗弃） | 规范文档 | — | — |
| dooit | 2,941 | MIT | 2026-08 | SQLite | — | 无 |
| taskwarrior-tui | 2,114 | MIT | 2026-08 | TW 后端 | ✅ | 无 |
| taskell | 1,783 | BSD-3 | **已归档** | md / JSON | ✅ | 无 |
| orgzly-revived | 1,116 | GPL | 2026-08 | .org 文件 | ⚠️ 仅整文件覆盖 | 无 |

> 数据来源：GitHub API（2026-08-30 查询）、npm registry API、Obsidian 官方 community-plugin-stats.json、Apple iTunes Lookup API。

**Obsidian 插件下载量（官方统计，比 star 更能反映真实使用）**：

| 插件 | Star | **官方下载量** | 维护状态 |
|---|---|---|---|
| Obsidian Tasks | 3,973 | **4,130,627** | 活跃，已交接给团队 |
| Obsidian Kanban | 4,483 | **2,607,917** | 事实停摆 |

注意 star 高的那个下载量反而低 58% —— **评估这个品类的项目不能只看 star**。

---

## 附二：证据强度说明

本报告绝大部分结论基于一手材料（源码常量、官方文档原文、GitHub API 实测、issue 原文）。以下两条**证据强度较弱，按推断对待**：

1. **Obsidian Kanban 与 Obsidian Sync / iCloud / git 的冲突**：#439 有明确的跨设备数据丢失与卡片重复实证，但未检索到标题级的 iCloud/git 专项 issue。「全文件重写 + UI 状态入文件 → 冲突粒度为整文件」这一**机理判断是基于源码的推断**。
2. **Obsidian Tasks 的错行写回 bug**：检索未命中当前 open 的明确实例；只能确认源码层面已有三级回退 + 不一致告警的防御设计，以及历史上曾有相关重构（PR #1728）。

另有两个未闭合的小缺口：SwiftoDo 是否含内购（仅确认「下载免费」）、Simpletask 的 Google Play 现状（仅确认 GitHub 仓库已归档）。均不影响任何实质结论。
