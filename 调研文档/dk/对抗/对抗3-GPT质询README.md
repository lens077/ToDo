# 对抗 3：以《评审报告与当前实现对照》质询 README

> 质询对象：仓库根 `README.md`，共 128 行。
> 立场依据：`调研文档/dk/我的理解-调研方案与当前实现对照3.md`（下称「立场文档」）。
> 核验基线：commit `1669194`。除通读立场文档与 README 外，已核查 `CONTEXT.md`、`DECISIONS.md`、`TODO.md`、格式与桌面规格、验证日志、决策补遗、Cargo 元数据以及 CLI、core、hooks、桌面后端源码；实跑了已构建 CLI 的帮助命令并检查了 Cargo workspace 和锁定依赖。
> 攻击标准：不要求 README 复述深度文档。只有当错误、歧义或缺失会让 README 的真实受众误判能力、风险、安装条件、数据边界或使用权限时，才列为攻击点。

## C1 — 核心对账被写成「核对实际状态」，实装没有语义完成度判断

- **README 行号与原文**：第 7 行：「对账引擎用 Git 物证核对任务与代码的实际状态」；第 66 行列出 `todo reconcile [--apply] [--no-semantic]`。
- **质询**：CLI 到底在哪里核对了任务语义与「代码的实际状态」？当前可执行路径会做文档结构检查和 Git 物证有效性检查，但没有接入语义实现；把这两层检查写成「任务与代码实际对齐」是在把核心卖点的未来态写成现在时。
- **立场证据**：立场文档 §5「剩余风险」第 1 项明确判定：`SemanticReconciler` 只有 trait 与 mock，真实 LLM 未接入，这是差异化叙事与实现之间最大的缺口。§5 同时把物证、无损回写、安全底线的组合定义为护城河，而不是声称物证本身已经证明任务完成。
- **仓库核验**：
  - `crates/todo-core/src/reconcile.rs:72` 至 `crates/todo-core/src/reconcile.rs:97` 只有 `SemanticReconciler` trait 和 `MockSemanticReconciler`。
  - `crates/todo-cli/src/main.rs:529` 至 `crates/todo-cli/src/main.rs:535` 直接调用 `Reconciler::new(".")`，没有 `.with_semantic(...)`；因此 CLI 中默认 `reconcile` 与 `--no-semantic` 没有语义能力差异。
  - `crates/todo-core/src/reconcile.rs:188` 至 `crates/todo-core/src/reconcile.rs:307` 另有重复 ID、非标准 checkbox、依赖与死引用等文档结构检查；`crates/todo-core/src/reconcile.rs:348` 至 `crates/todo-core/src/reconcile.rs:395` 的 Git 部分检查 commit 是否存在、提交信息是否含精确的 revert 标记，并收集 changed files。changed files 只进入未接线的语义候选；这些检查都不会判断任务正文是否已由代码实现。
  - `CONTEXT.md:19` 至 `CONTEXT.md:25` 还明确说 Evidence 只证明代码改动存在，不单独证明任务完成；Finding 是提议或确定事实，不会自行改写状态。
- **对 README 受众的伤害**：这是产品最硬的差异化声明。用户可能把一次绿色 `reconcile` 当成「代码与任务已核对」，据此信任陈旧完成状态；产品评估者则会误判 MVP 已完成最关键的语义闭环。
- **严重度**：高。
- **建议修改方向**：把第 7 行收窄为「结构层校验稳定 ID、依赖和 Git 物证的存在/撤销状态」；紧邻 `reconcile` 命令明确写「语义对账尚未接真实实现，当前不会判断任务正文是否已由代码完成；`--no-semantic` 在 CLI 当前版本无行为差异」。若不愿公开这一限制，就先接入真实 semantic provider，再保留现有措辞。

## C2 — Git 检查被跳过时仍返回绿色，README 没有披露这个假阴性路径

- **README 行号与原文**：第 41 行：「在任意项目目录中初始化并使用 CLI」；第 66 行：「`todo reconcile`……默认只报告」；第 71 行把退出码 `0` 定义为成功。
- **质询**：在非 Git 目录、shallow clone 或 partial clone 中，Git 物证检查根本没有执行，为什么默认人类输出仍可显示 `no reconciliation findings` 并以 0 退出？这不是「降级」，而是对核心检查是否发生保持沉默。
- **立场证据**：立场文档 §5 把安全克制定义为三要素护城河之一，并指出信任崩塌一次就不会再开启对账；§5 风险第 2 项又说明证据门尚未跑完。README 更不应把「未检查」呈现成「检查后无发现」。
- **仓库核验**：
  - `docs/verification-log.md:87` 至 `docs/verification-log.md:106` 记录过 shallow clone 大规模误报的严重缺陷，修复策略是跳过 Git 物证并给出明确原因。
  - `crates/todo-core/src/reconcile.rs:142` 至 `crates/todo-core/src/reconcile.rs:150` 确实把 shallow/partial/non-Git 原因放进 `report.skipped`。
  - 但 `crates/todo-cli/src/main.rs:555` 至 `crates/todo-cli/src/main.rs:559` 的退出码只看 `confirmed` 与 `suspected`，完全不看 `report.skipped`；两者为空时返回 0。
  - `crates/todo-cli/src/main.rs:573` 至 `crates/todo-cli/src/main.rs:597` 的人类输出也完全不打印 `report.skipped`，会输出 `no reconciliation findings`。
  - `crates/todo-cli/src/hooks.rs:242` 至 `crates/todo-cli/src/hooks.rs:255` 的 SessionStart 注入同样只筛 `confirmed:` / `suspected:`，跳过原因不会进入 agent 上下文。
- **对 README 受众的伤害**：许多 CI checkout 会使用浅克隆（具体默认行为取决于平台与配置），非 Git 目录又被「任意项目目录」覆盖。用户和 agent 都可能收到假绿色结果，并继续相信对账已经完成；这直接击中产品要消灭的「错误事实感」。
- **严重度**：高。
- **建议修改方向**：README 把 Git 与完整历史列为 Evidence/Reconcile 的前置条件，说明 shallow/partial clone 会跳过检查；实现侧必须在人类输出和 hooks 中显式打印 `report.skipped`，并让「核心检查未执行」使用非 0 或专门的 incomplete 状态，不能与完整成功混为一类。

## C3 — Rust 1.74 的环境承诺与锁定依赖的真实 MSRV 冲突

- **README 行号与原文**：第 15 行：「Rust | 1.74 或更高 | `Cargo.toml` 中的 `rust-version`」。
- **质询**：`Cargo.toml` 写了 1.74 不等于锁定依赖能在 1.74 编译。README 为什么只抄本地 package 声明，不核对实际依赖图？
- **立场证据**：立场文档 §3 强调当前实现状态应以独立验证为准，§4 又把 CLI 与桌面构建列为已实现 P0。一个写在快速开始前的最低工具链版本必须能复现这些构建，而不能只是 manifest 中未被验证的愿望值。
- **仓库核验**：
  - `Cargo.toml:7` 声明 `rust-version = "1.74"`，但 `Cargo.toml:18` 对 `clap` 使用宽松的 `4.5` 范围；`Cargo.lock:375` 至 `Cargo.lock:376` 实际锁到 `clap 4.6.6`。
  - 实跑 `cargo metadata --locked --format-version 1` 并检查各 package 的 `rust_version`：CLI 的直接依赖 `clap 4.6.6` 声明 MSRV 1.85；桌面锁定图中的 `time 0.3.55`、`plist 1.10.0`、`serde_with 3.22.0` 等声明 MSRV 1.88。`Cargo.lock:2531`、`Cargo.lock:3178`、`Cargo.lock:3871` 可回查相应锁定包。
  - 仓库没有 `rust-toolchain.toml` 固定已验证工具链；本轮核验环境为 Rust 1.97.1，不能反证 1.74 可用。
- **对 README 受众的伤害**：严格按 README 安装 Rust 1.74 的新用户会在依赖解析/编译阶段失败，而且失败发生在第一条构建命令；这是可复现的 onboarding 阻断，不是文案瑕疵。
- **严重度**：高。
- **建议修改方向**：要么把 README 最低版本提高到锁定图实际要求并在 CI 用该版本验证，要么收紧/降级依赖使 `cargo +1.74 build --locked` 真正通过。最低版本必须由 CI matrix 证明，不能只引用 workspace manifest。

## C4 — CLI「快速开始」实际会构建整个桌面工作区

- **README 行号与原文**：第 19 行：「桌面端另需各平台的 Tauri 2 系统依赖」；第 34 至 39 行的通用构建步骤是 `pnpm install` 后运行裸 `cargo build`；第 41 至 49 行紧接着只演示 CLI。
- **质询**：既然桌面依赖被写成「另需」，为什么 CLI 快速开始先无必要地要求 `pnpm install`，随后又用会包含 `todo-desktop` 的裸 `cargo build`？前者是多余的 Node/pnpm 步骤，后者会把桌面 host 的 Rust 与平台系统依赖带入 CLI 路径。
- **立场证据**：立场文档 §3、§4 把 todo-core、todo-cli、桌面端分成独立层，并指出 CLI 已可单独工作。README 应利用这个分层提供最短可执行路径，而不是把完整 workspace 构建伪装成 CLI 前置条件。
- **仓库核验**：
  - `Cargo.toml:1` 至 `Cargo.toml:3` 是虚拟 workspace，members 包括 `apps/desktop/src-tauri`。
  - 实跑 `cargo metadata --locked --no-deps --format-version 1`，`workspace_default_members` 同时包含 `todo-core`、`todo-cli`、`todo-desktop`；因此在根目录执行裸 `cargo build` 会构建桌面 host。
  - `crates/todo-cli/Cargo.toml` 只依赖 core、clap、serde 等 Rust crate；CLI 本身不需要 `pnpm install`。
- **对 README 受众的伤害**：README 先让 CLI 用户安装无关的 JavaScript 依赖；随后，没有 WebKitGTK/GTK 等 Tauri 系统依赖的 Linux 用户还可能在裸 `cargo build` 中失败。即使能构建，也要下载并编译无关的桌面依赖，显著拉长 time-to-first-value。
- **严重度**：高。
- **建议修改方向**：把 CLI 快速开始改为 `cargo build --locked -p todo-cli`，并把 Node、pnpm、Tauri 系统依赖整体移到桌面端章节。完整贡献者构建另列 `cargo build --workspace`，不要混入首条用户路径。

## C5 — `init --adopt` 会改写假任务，还可能让真任务从默认视图消失

- **README 行号与原文**：第 53 行：「已有普通 GFM 清单时，用 `todo init --adopt` 接管现有文件并补齐稳定 ID」；第 122 行：「工具只修改自己认识的行，其余字节原样保留」。
- **质询**：普通 Markdown 经常包含展示任务语法的代码围栏示例。为什么 `--adopt` 接管逻辑不复用正式解析器的围栏排除规则，而是逐行匹配字符串并给示例代码插 ID？这正是 README 承诺不会碰的字节。
- **立场证据**：立场文档 §5 把无损回写列为三要素护城河之一，并指出它是评审报告认定的生死线；§3 还把恶意样本中的代码围栏假任务零混入列为验证成果。该验证覆盖正式 parser，并不自动覆盖另一套 adoption 扫描器。
- **仓库核验**：
  - `docs/format-spec-v1.md:73` 至 `docs/format-spec-v1.md:75` 明确规定代码围栏内与行内代码中的内容都不是任务。
  - `crates/todo-cli/src/main.rs:707` 至 `crates/todo-cli/src/main.rs:724` 的 `adopt_source` 不调用 parser，也不维护 fence 状态；任何去掉缩进后以 `- [ ]`、`- [x]` 或 `- [X]` 开头的行都会被追加 `<!--t:...-->`。
  - `crates/todo-cli/src/main.rs:290` 至 `crates/todo-cli/src/main.rs:297` 还会拒绝所有以 `---\n` 开头的普通 Markdown，即使 YAML frontmatter 与 todo 无关；README 没披露这一限制。
  - `adopt_source` 会原样保留既有二级标题；按 `docs/format-spec-v1.md:50` 至 `docs/format-spec-v1.md:55`，标题名若不匹配新 frontmatter 的 Todo/Doing/Done，标题下任务会变成 `column: null`。`crates/todo-cli/src/main.rs:184` 至 `crates/todo-cli/src/main.rs:190` 又让默认 `todo list` 过滤掉这些任务，看板也不渲染游离任务。
  - `crates/todo-cli/tests/cli.rs:52` 至 `crates/todo-cli/tests/cli.rs:64` 的 adoption 测试只有单条真实任务，没有围栏、既有二级标题、行内示例或普通 frontmatter。
- **对 README 受众的伤害**：这是 README 主动建议用户对既有文件执行的破坏性迁移入口。它可能污染文档示例、生成无意义 ID diff，还可能让原本位于自定义二级标题下的真任务从默认列表与看板消失，直接破坏用户对「接管」和字节保真的信任。
- **严重度**：高。
- **建议修改方向**：实现侧让 `--adopt` 基于与正式 parser 相同的代码围栏/行内代码识别，只给真实 task node 补 ID，安全合并普通 frontmatter，并为既有标题建立明确的列映射或迁移报告；修复前 README 必须限制适用输入、要求先提交/备份并审查 diff，不能笼统称「普通 GFM 清单」。

## C6 — Claude hook 依赖持久 PATH 却吞错，Codex 写入选项又被省略

- **README 行号与原文**：第 51 行：「把 `target/debug` 加入 `PATH` 后可直接使用 `todo`」；第 67 行把 `todo hooks preview|install|uninstall|instructions [--agent claude|codex|all]` 统称为「管理 agent 接入」。
- **质询**：README 第 51 行提到可把 `target/debug` 加入 PATH，却没有说明 Claude SessionStart 必须看见这条持久 PATH，也没有给验证步骤；生成 hook 又硬编码裸 `todo` 并吞掉错误。Codex install 的输出虽会提示没有 SessionStart hook，但真正写入 `AGENTS.md`/`CLAUDE.md` 的 `instructions --write` 选项仍从 README 命令表消失。
- **立场证据**：立场文档 §1 与 §3 说明 v1 主动去 MCP，CLI + hooks 是替代后的核心接入层；§3 特别称 SessionStart 注入与指引块已经落地。`research/g-decisions-addendum.md` §1 D1 更把 hooks 定义为保证任务系统「不被忽略」的关键。
- **仓库核验**：
  - `crates/todo-cli/src/hooks.rs:12` 至 `crates/todo-cli/src/hooks.rs:14` 生成命令 `todo hooks context ... 2>/dev/null || true`。如果 `todo` 不在 hook 进程的持久 PATH 中，失败会被完全吞掉。
  - `crates/todo-cli/src/hooks.rs:70` 至 `crates/todo-cli/src/hooks.rs:75` 对 Codex install/uninstall 只返回「no config change」，真正接入要走 instructions。
  - `crates/todo-cli/src/main.rs:100` 至 `crates/todo-cli/src/main.rs:105` 定义了 `instructions --write`；`crates/todo-cli/src/hooks.rs:195` 至 `crates/todo-cli/src/hooks.rs:202` 证明不带 `--write` 只预览、不落盘。
- **对 README 受众的伤害**：CLI + hooks 是取消 MCP 后的 P0 接入路线，不是边缘功能。Claude 用户若只在交互 shell 临时调整 PATH，SessionStart 可能长期静默无输出；Codex 用户还必须从命令输出而非 README 才能发现要另走 instructions 写入。接入失败后，产品重新退化为「靠人记得同步」。
- **严重度**：高。
- **建议修改方向**：README 增加按 agent 分开的完整配方与验证步骤：先将二进制安装进 hook 可见的持久 PATH，再执行 Claude install；Codex 明示没有 SessionStart hook，必须执行 `todo hooks instructions --agent codex --write`。实现侧优先把当前可执行文件的绝对路径写入 hook，且提供 `todo hooks doctor`，不要以 `|| true` 隐藏永久配置错误。

## C7 — 密钥泄露警告漏掉了 `install` 与 `uninstall`

- **README 行号与原文**：第 75 行：「`todo hooks preview` 会打印本地 agent 配置文件的完整内容，其中可能包含 API 密钥。不要把该输出贴到公开渠道。」
- **质询**：为什么只警告 preview？Claude 的 install 和 uninstall 也会把完整的 `~/.claude/settings.json` 内容原样打印到 stdout，同样可能泄露 API 密钥。
- **立场证据**：立场文档 §5 把安全底线的克制列为护城河，并强调信任一旦崩塌就难以恢复。README 已经识别了密钥风险，却只覆盖一个命令，属于危险的半提示。
- **仓库核验**：
  - `crates/todo-cli/src/main.rs:231` 至 `crates/todo-cli/src/main.rs:249` 对 preview、install、uninstall 都调用同一个 `emit(..., hooks_human)`。
  - `crates/todo-cli/src/main.rs:254` 至 `crates/todo-cli/src/main.rs:260` 的 `hooks_human` 会打印每个 target 的完整 `content`。
  - `crates/todo-cli/src/hooks.rs:80` 至 `crates/todo-cli/src/hooks.rs:164` 读取、重序列化并把整个 `settings.json` 放入 `HookTarget.content`，不是只输出受管 hook diff。
- **对 README 受众的伤害**：用户可能谨慎地不分享 preview，却把「安装结果」贴进 issue、CI 日志或聊天求助，暴露同一批密钥。README 的限定措辞反而制造了 install/uninstall 输出更安全的错误暗示。
- **严重度**：高。
- **建议修改方向**：立刻把警告扩展到 preview/install/uninstall 的所有 Claude 输出；更根本的修复是默认只打印目标路径、changed 状态与脱敏 diff，完整内容必须显式 `--show-content`，并对常见 secret 字段做递归遮蔽。

## C8 — 仓库没有许可证声明，复制、修改、分发与贡献边界不明

- **README 行号与原文**：第 32 行标题为「快速开始」；第 41 行：「在任意项目目录中初始化并使用 CLI」；第 104 行还给出完整测试命令。README 全文没有许可证或「仅内部评估」说明。
- **质询**：一个准备商业化、并明确讨论开源边界的仓库，在未拍板许可证时为什么只给构建与运行路径，却不告诉读者复制、修改、再分发和贡献代码的许可边界？
- **立场证据**：立场文档 §5 风险第 3 项明确说「对账引擎是否开源、买断还是 Obsidian 模型尚未拍板」。这不是 README 可以忽略的深度战略细节，而是外部读者当前能否合法使用仓库的前置条件。
- **仓库核验**：
  - `DECISIONS.md:36` 至 `DECISIONS.md:60` 的开源边界与许可证选项仍全部待拍板。
  - `research/g-decisions-addendum.md:137` 至 `research/g-decisions-addendum.md:151` 又记录了闭源分发方向以及开放 L1 的建议，说明仓库不是默认 MIT 的简单情况。
  - `git ls-files` 核验不到任何根级或已跟踪的 `LICENSE`/`LICENCE` 文件；README 也没有版权/许可段落。
- **对 README 受众的伤害**：默认版权状态不会自动授予复制、修改和再分发权。拿到仓库的评估者、潜在贡献者与打包者无法判断合法边界；这会阻断贡献和分发，也可能在未来改许可证时制造预期冲突。
- **严重度**：高。
- **建议修改方向**：先拍板并添加 LICENSE；若当前必须保持闭源/内部状态，README 顶部明确写「仅供授权评估，未授予复制、修改或再分发许可」，并说明格式规范、CLI、对账、GUI 各层的许可边界。不要以沉默代替政策。

## C9 — README 没有披露「刚开始 dogfooding」的实验性成熟度

- **README 行号与原文**：第 32 行直接进入「快速开始」；第 98 行只把 `.app` 限定为「适合本机验收」；第 118 行强调「独立验证记录」，但全文没有项目状态、数据安全试用边界或 dogfooding 时长。
- **质询**：强内部验证不等于真实工作流已经跑过证据门。为什么 README 展示大量通过项，却不同时告诉用户 round-trip 基线、两周 dogfooding 和外部试用都还没有结果？
- **立场证据**：立场文档 §4 最后一段明确说 dogfooding 自 08-31 刚开始，报告要求的两周真实项目基线与 5 名外部开发者试用尚未完成；§5 风险第 2 项把「证据门未跑」列为第二大剩余风险，并明确要求在此之前不扩功能面。
- **仓库核验**：
  - `TODO.md:19` 至 `TODO.md:27` 仍有原生文件选择器、看板写回、外部编辑刷新三项人工验收，以及 release 构建和 Playwright 进 CI 等待办。
  - `TODO.md:30` 只有一条「用 todo 自身管理本项目后续开发」的 Doing 任务，印证自举刚开始而不是长期运行证据。
  - `apps/desktop/package.json` 与 Rust crates 版本均为 `0.1.0`；README 没有 CLI release 安装说明，桌面仅给出本机验收用 debug `.app`，`TODO.md:23` 还把「CLI release 构建与安装说明」列为待办。
- **对 README 受众的伤害**：这是会写入项目任务文件的数据工具。没有成熟度声明，读者容易把「测试通过」等同于「已适合长期托管真实 backlog」，无法据此决定是否只在副本/已提交文件上试用。
- **严重度**：中。
- **建议修改方向**：在开头增加「项目状态」：实验性 MVP、dogfooding 起始日期、尚缺的 2 周基线/外部试用、未自动化真机路径、无 release 分发；建议首次使用前提交 `TODO.md` 并审查 diff。验证日志仍可保留，但要与尚未验证的证据门并列呈现。

## C10 — 优先级范围在 README、规范和 CLI 之间三方不一致

- **README 行号与原文**：第 46 行示例使用 `--priority 1`；第 61 行把选项写成 `--priority <1-3>`。
- **质询**：优先级 4 到底是合法还是非法？README 说只有 1–3，格式规范和 CLI 都接受 1–4。
- **立场证据**：立场文档 §3 把 `docs/format-spec-v1.md` 定义为格式层依据，并强调 CLI 参数/稳定 JSON/退出码已经验证。README 作为入口不能再发明第三套范围。
- **仓库核验**：
  - `docs/format-spec-v1.md:98` 明确规定优先级 token 为 `!1` 到 `!4`。
  - `crates/todo-cli/src/main.rs:320` 至 `crates/todo-cli/src/main.rs:321` 只在值不属于 `1..=4` 时拒绝，因此 `--priority 4` 是正式可执行行为。
- **对 README 受众的伤害**：用户会误以为 4 非法，集成方可能按 README 生成错误校验 schema；同一仓库的规范、CLI 和门面文档出现三种认知，削弱所谓 canonical 接口的可信度。
- **严重度**：中。
- **建议修改方向**：把两处命令签名统一为 `<1-4>`，并说明 1 最高、4 最低；若产品只想暴露三级，则应先修改 spec 与 CLI，再改 README，不能只靠文档隐藏第 4 级。

## C11 — 「唯一事实源」没有限定 Project Task，与 Capture 的第二权威文件形成歧义

- **README 行号与原文**：第 3 行：「仓库里的 `TODO.md` 是唯一事实源」；第 102 行：「Capture 是个人收件箱，存放在 `~/.todo/inbox.md`」。
- **质询**：唯一的是所有产品数据，还是只限 Project Task？README 开场使用绝对表述，直到 99 行后才披露另一个具有权威数据的 Markdown 文件。
- **立场证据**：立场文档 §4 明确把 Capture/Triage 作为报告之外新增的个人收件箱能力，并强调它不污染仓库；这套双边界本来是实现的优点，README 却在第一句话把作用域省掉了。
- **仓库核验**：
  - `CONTEXT.md:7` 至 `CONTEXT.md:17` 严格区分 Project Task、Capture 与 Triage。
  - `docs/adr/0001-desktop-seams-and-capture-storage.md:3` 的准确表述是：Project Tasks 以仓库 Markdown 为权威，personal Captures 以全局 `inbox.md` 为权威。
  - `crates/todo-core/src/desktop.rs:489` 至 `crates/todo-core/src/desktop.rs:492` 固定全局 Capture 路径；这不是 SQLite 临时缓存，而是需备份的持久数据。
- **对 README 受众的伤害**：用户可能把「仓库随 Git 走」理解为所有数据都天然备份，忽略 `~/.todo/inbox.md`；贡献者也可能把 Capture 错当成派生数据。该歧义直接影响备份、迁移和数据丢失预期。
- **严重度**：中。
- **建议修改方向**：把第 3 行改成「每个仓库的 Project Task 以该仓库 `TODO.md` 为唯一事实源；个人 Capture 另以 `~/.todo/inbox.md` 为事实源」。在开头就给出数据边界和备份范围，不要让术语定义承担 99 行后的补救。

## C12 — SQLite 被呈现为现行产品架构，但没有生产调用者

- **README 行号与原文**：第 6 行：「SQLite 只是派生投影，可随时从 Markdown 重建」；第 24 行把「SQLite 投影」列入 todo-core；第 112 至 118 行提供「投影性能基准」。
- **质询**：README 是否在暗示 CLI 或桌面端当前使用 SQLite 加速？仓库里 Projection 只有测试和 benchmark 调用，产品执行路径仍直接解析 Markdown。
- **立场证据**：立场文档 §4 P0 对照已经如实写出「`list` 暂未接投影」；§5 风险排序要求先过证据门再扩面。README 省略接线状态，会把一个已实现但未采用的组件写成当前运行架构。
- **仓库核验**：
  - 全仓 Rust 调用检索显示 `Projection::open` / `Projection::open_in_memory` 只出现在 `crates/todo-core/tests/projection.rs` 和 `crates/todo-core/examples/benchmark_projection.rs`。
  - `crates/todo-cli/src/main.rs:599` 至 `crates/todo-cli/src/main.rs:605` 每次直接 `parse_file("TODO.md")`。
  - `crates/todo-core/src/desktop.rs:121` 至 `crates/todo-core/src/desktop.rs:131` 的 desktop open/snapshot 同样直接 `parse_file`；桌面派生视图也没有查询 Projection。
- **对 README 受众的伤害**：用户会把 benchmark 数字当成实际 CLI/桌面数据路径的性能证据；贡献者会错误假设已有缓存一致性与重建生命周期，增加调试和架构理解成本。
- **严重度**：中。
- **建议修改方向**：把第 6 行改成状态明确的表述，例如「todo-core 已实现可重建的 SQLite Projection；当前 CLI 与桌面仍直接解析 Markdown，Projection 仅用于测试/基准，接入触发条件见验证日志/TODO」。接线后再把它升级为现行架构声明。

## C13 — README 没有讲清三要素护城河，反而把普通品类名当定位

- **README 行号与原文**：第 3 行只把产品定义为「面向开发者的本地优先任务工具」；第 5 至 7 行按存储、SQLite、对账列机制。
- **质询**：目标用户为什么不用手写 TODO.md、Backlog.md 或任意本地看板？README 没有先讲「agent 被漂移任务误导、重复已完成工作」这个核心问题，也没有把物证、无损回写、安全裁决连接成一套可信对账系统。
- **立场证据**：立场文档 §1 把「TODO 漂移是结构性问题」列为仍然成立的首要判断；§5 明确判定护城河是三件事组合：可审计 Git 物证、难以速成复刻的无损回写、§6.5 的安全底线。`research/README.md:133` 至 `research/README.md:148` 也把推荐定位写成「给 AI 协作开发者的任务对账器」，并解释看板只是附赠视图。
- **仓库核验**：README 确实在第 7 行提 Evidence、在第 73 行提安全约束、到第 122 行才提未触碰字节保留；三者分散在首屏、CLI 尾部和文档尾部，且第 7 行还因 C1 把结构层夸成了实际状态核对。当前首段无法准确传达「为什么可信、目前做到哪一层」。
- **对 README 受众的伤害**：潜在用户会把项目归类成又一个 Markdown 看板，错过真正差异；同时被「实际状态」的过强措辞吸引的用户又会在能力边界上受骗。这既欠承诺又过承诺。
- **严重度**：中。
- **建议修改方向**：重写开头为「问题—方法—当前边界」三句：agent 任务漂移造成重做；以 Git Evidence + 字节保真回写 + 只报告不擅改的裁决建立信任；当前语义对账尚未接线。桌面看板放在其后，明确是视图，不是定位。

## C14 — `todo lint` 的「格式检查」背书超过实际覆盖

- **README 行号与原文**：第 64 行：「`todo lint` | 检查格式问题，发现问题时退出码为 6」。
- **质询**：「格式问题」至少是否包括会破坏稳定 ID 操作的重复 ID？验证日志说包括，当前 `lint` 实现却不检查。README 用一个宽泛动词掩盖了实际极窄的检查面。
- **立场证据**：立场文档 §3 把格式规范、稳定 ID、lint 与验证结果作为实现可信度的一部分；§5 又把安全底线列为护城河。对于以稳定 ID 为语义锚点的工具，漏掉重复 ID 不是无关紧要的 lint 风格差异。
- **仓库核验**：
  - `crates/todo-cli/src/main.rs:487` 至 `crates/todo-cli/src/main.rs:527` 的 `lint` 只收集 parser error 与 `unrecognized_task_like_lines`，没有调用 `Document::duplicate_ids()`、没有依赖/引用检查。
  - `crates/todo-core/src/model.rs:16` 至 `crates/todo-core/src/model.rs:28` 虽提供 `duplicate_ids()`，但 lint 路径没有使用。
  - 重复 ID 实际由 `crates/todo-core/src/reconcile.rs:188` 至 `crates/todo-core/src/reconcile.rs:230` 检出；用户只跑 README 推荐的 lint 不会得到该发现。
  - `docs/verification-log.md:54` 却仍写「lint 有效性｜注入重复 ID｜正确检出」，已与当前源码分叉；README 第 118 行把这份日志作为独立验证背书而未提示漂移。
- **对 README 受众的伤害**：CI 使用者可能把 `todo lint` 绿色当成任务文件结构安全，重复 ID 却会让按 ID 的 done/evidence 操作命中错误任务。链接的验证记录还会增强这种错误信心。
- **严重度**：中。
- **建议修改方向**：README 枚举当前 lint 范围，并明确重复 ID/依赖/物证问题需跑 `reconcile`；更合理的是让 lint 实现覆盖所有纯文档结构不变量，并更新验证日志。不能同时保留「格式检查」泛称和已失真的重复 ID 背书。

## C15 — 退出码 6 并非仅表示 lint 问题

- **README 行号与原文**：第 69 行称 `--json` 输出稳定 envelope；第 71 行把退出码写成「`6` lint 发现问题」。
- **质询**：`todo reconcile` 有 confirmed 或 suspected finding 时也返回 6。脚本看到 6 应判断「lint 失败」还是「对账发现」？README 的全局定义与实际 canonical CLI 不一致。
- **立场证据**：立场文档 §3 把退出码 0–6 的逐项验证当成 CLI 可脚本化证据；`research/g-decisions-addendum.md` §1 D1 又要求 CLI 以稳定 exit code 供 agent 与脚本共用。既然把它作为协议，就不能用错误的单义标签描述复用码。
- **仓库核验**：
  - `crates/todo-cli/src/main.rs:555` 至 `crates/todo-cli/src/main.rs:559` 明确在 reconcile 有任一 finding 时返回 6。
  - `crates/todo-cli/src/hooks.rs:275` 至 `crates/todo-cli/src/hooks.rs:284` 也把子命令退出 6 当成可读取输出的正常 finding 路径，说明 6 已经是跨 lint/reconcile 的通用「发现」码。
  - 已构建 `target/debug/todo --help` 同样打印 `6 lint issues`，README 复制了实现帮助文本中的错误，而不是核对命令行为。
- **对 README 受众的伤害**：自动化会错误分类失败原因，可能把 reconcile finding 当格式损坏，或只在 lint 场景允许 6 而让 hooks/CI 中断。所谓稳定契约因此只稳定了数字，没有稳定语义。
- **严重度**：中。
- **建议修改方向**：若保留复用码，把 README/CLI help 改为「6：lint 或 reconciliation findings（命令内语义见 JSON schema）」；若消费者必须无歧义，则给 reconcile 单独退出码。无论选择哪种，都补对应契约测试。

## C16 — README 承诺的外部修改检测没有接入 CLI/桌面写入主链路

- **README 行号与原文**：第 124 行：「写入走文件锁与原子替换；检测到外部修改时放弃本次写入并要求重试」；第 71 行还为「乐观锁冲突」保留退出码 5。
- **质询**：CLI 和桌面端的实际 mutation 都走 `LockedWriter::update`，它在读取后不复核 mtime/hash 就原子替换。普通编辑器不会遵守 `.TODO.md.lock`，外部修改若恰好发生在读取与 rename 之间，哪里会被检测并放弃？
- **立场证据**：立场文档 §3 把「advisory lock + 乐观锁 + 原子写」列为已验证的数据安全组合；§5 又把安全底线定义为护城河。README 第 124 行正是在向用户承诺这套组合，而不是只描述一个未使用的库类型。
- **仓库核验**：
  - `crates/todo-core/src/lock.rs:32` 至 `crates/todo-core/src/lock.rs:44` 的 `LockedWriter::update` 流程是加 advisory lock、`parse_file`、派生 change、`apply_changes`、`atomic_write`；写前没有重读文件，也没有比较 snapshot hash/mtime。
  - `crates/todo-core/src/write.rs:63` 至 `crates/todo-core/src/write.rs:87` 的 `OptimisticLock` 确实会在写前复核 hash 与 mtime，但全仓调用检索显示它只在 `crates/todo-core/tests/core.rs` 使用；生产 CLI、desktop 与 Capture 路径都没有调用。
  - CLI 的 add/toggle/evidence/reconcile apply 走 `LockedWriter`，例如 `crates/todo-cli/src/main.rs:343` 至 `crates/todo-cli/src/main.rs:377`；桌面 mutation 走 `crates/todo-core/src/desktop.rs:142` 至 `crates/todo-core/src/desktop.rs:148` 的同一路径。
  - `docs/verification-log.md:44` 还声称 advisory lock 与乐观锁已「正确组合」，该验证结论已经与当前生产调用图不一致。
- **对 README 受众的伤害**：外部编辑是本产品主动支持的核心场景。原子 rename 只能防半写文件，advisory lock 只能协调守约写入者；两者都不能防 VS Code/Vim 在竞态窗口内的更新被旧 snapshot 覆盖。README 把一个未接线的安全机制写成现行保证，风险是直接丢用户刚写的任务内容。
- **严重度**：高。
- **建议修改方向**：生产写入主链路必须在持有 advisory lock 的同时，于最终 rename 前复核读取时 snapshot；发现外部变化后基于新文档重新派生稳定 ID intent，无法安全 rebase 时返回 `ConcurrentModification`/退出码 5。补一个不遵守 advisory lock 的外部进程竞态测试；修复前 README 必须撤回无条件保证并披露窗口。

## 总攻击清单

| 编号 | 一句话标题 | 严重度 |
|---|---|---|
| C1 | 核心对账被写成「核对实际状态」，实装没有语义完成度判断 | 高 |
| C2 | Git 检查被跳过时仍返回绿色且不提示 | 高 |
| C3 | Rust 1.74 门槛与锁定依赖的 MSRV 冲突 | 高 |
| C4 | CLI 快速开始误建整个桌面工作区 | 高 |
| C5 | `init --adopt` 会改写假任务，还可能隐藏真任务 | 高 |
| C6 | Claude hook 依赖持久 PATH 却吞错，Codex 写入选项被省略 | 高 |
| C7 | 密钥泄露警告漏掉 install/uninstall | 高 |
| C8 | 无许可证声明，复制、修改、分发与贡献边界不明 | 高 |
| C9 | 未披露仅是刚开始 dogfooding 的实验性 MVP | 中 |
| C10 | 优先级范围 README/规范/CLI 三方不一致 | 中 |
| C11 | 「唯一事实源」未限定 Project Task，与 Capture 边界歧义 | 中 |
| C12 | SQLite 被呈现为现行架构但没有生产调用者 | 中 |
| C13 | README 没讲清物证、无损回写、安全底线三要素护城河 | 中 |
| C14 | `todo lint` 的格式检查背书超过实际覆盖 | 中 |
| C15 | 退出码 6 并非仅表示 lint 问题 | 中 |
| C16 | 外部修改检测未接入 CLI/桌面写入主链路 | 高 |
