# 对抗 1：以《我的理解——调研方案与当前实现对照》质询 README

> 质询对象：仓库根 `README.md`，共 128 行。
> 立场文档：`调研文档/dk/我的理解-调研方案与当前实现对照.md`。
> 核查方式：通读两份文档，并对照 `CONTEXT.md`、格式与桌面规格、验证日志、Cargo/npm 清单、CLI/核心/桌面源码；另执行 `target/debug/todo --help`、各子命令 `--help`、`cargo metadata --offline`、`cargo tree --offline` 等只读命令。

## 质询口径

这里不以「README 没有复述全部深度文档」凑数。以下每一项都至少满足一项：README 的明确陈述可被当前仓库证伪；README 给出的主路径按原文执行会失效；README 隐去了会让真实受众误判能力、安全边界或自动化契约的关键条件。

## A1 对账被写成「核对代码实际状态」，实际 CLI 只接上了结构层

- **README 位置与原文**：`README.md:7`：「对账引擎用 Git 物证核对任务与代码的实际状态，只报告，不擅自改写完成状态。」
- **质询**：当前 CLI 凭什么声称能核对「任务与代码的实际状态」？它能核对的是重复 ID、孤儿依赖、死引用、commit 对象是否存在，以及提交信息是否含标准 revert 文案；真正判断「代码是否实现了任务」的语义层并未接入。顶部卖点把「物证卫生检查」扩大成「代码实际状态核对」，超出了当前实现。
- **立场证据**：立场文档 §3「核心机制」第 3 点明确区分结构层与语义层；§5 第 5 点明确写出当前只有 `MockSemanticReconciler`，没有真实模型，也没有桌面确认 UI，并将「AI 提议」称为仍然空缺的半边。
- **仓库核实**：`CONTEXT.md:19` 明定 Evidence 只证明代码改动存在，「不单独证明任务已经完成」；`crates/todo-cli/src/main.rs:529` 用 `Reconciler::new(".")` 直接执行，没有调用 `with_semantic`；`crates/todo-core/src/reconcile.rs:119` 创建的 `semantic` 默认为 `None`，`crates/todo-core/src/reconcile.rs:152` 只有存在实现时才运行语义层。Git 层在 `crates/todo-core/src/reconcile.rs:450` 只检查 commit 对象存在，在 `crates/todo-core/src/reconcile.rs:457` 只按提交信息中的 `This reverts commit <sha>` 字符串识别 revert；它没有核查当前文件内容是否满足任务正文。
- **严重度**：高。
- **建议修改方向**：把顶部陈述收窄为「核对任务结构与 Git 物证的完整性；语义完成判定尚未接入」。单列「已实现 / 未实现」两层，并保留「Evidence 不等于完成证明」这一句。等真实 `SemanticReconciler` 与人工确认闭环接通后，再使用「代码实际状态」表述。

## A2 快速开始绕过了产品核心证据闭环

- **README 位置与原文**：`README.md:41`：「在任意项目目录中初始化并使用 CLI」；随后的 `README.md:45` 至 `README.md:49` 只演示 `init`、`add`、`list`、`next`。`README.md:65` 至 `README.md:67` 才把 `evidence`、`reconcile`、`hooks` 各列成一行。
- **质询**：README 首页最重要的差异化是「对账」，但唯一的首次使用路径创建出来的任务没有 `🔗` 物证，也不安装 hook，更不运行对账。读者逐字照做，得到的只是普通 Markdown Todo；最核心能力没有被激活，也没有任何提示说明「无 Finding」可能只是「从未记录物证」。这不是要求 README 抄设计文档，而是主路径没有交付首页承诺。
- **立场证据**：立场文档 §5 第 2 点把「从强同步到对账」称为全项目最重要的概念跃迁；§3 第 3、4 点分别说明 Evidence → reconcile 与 CLI + hooks 才构成实际闭环。§5 第 5 点又提醒闭环目前只有骨架，更要求 README 精确展示已经能跑通的那一半。
- **仓库核实**：`crates/todo-core/src/reconcile.rs:354` 只遍历带 `TokenKind::Evidence` 的任务来生成代码候选；没有物证的普通任务不会产生「未检查」或「缺物证」Finding。完整工作流其实只藏在 `crates/todo-cli/src/hooks.rs:211` 的受管指令块中：开工记录 `START`、提交后写 `evidence`、再 `done`、交接前 `reconcile --no-semantic`，README 没有把这条流程交给人类用户。
- **真实受众伤害**：首次使用者会把「no reconciliation findings」误读为代码与任务一致；评估者也无法从快速开始验证产品唯一差异化，容易把项目判断成又一个文件型 Todo。
- **严重度**：高。
- **建议修改方向**：在快速开始之后增加一条最短的「证据闭环」：记录起点 commit → 创建/选定任务 → 提交代码 → `todo evidence` → 人工确认后 `todo done` → `todo reconcile --no-semantic`。同时把输出状态区分为「已检查且无异常」与「没有可检查的物证」。

## A3 `init --adopt` 不是 README 所承诺的安全接管

- **README 位置与原文**：`README.md:53`：「已有普通 GFM 清单时，用 `todo init --adopt` 接管现有文件并补齐稳定 ID。」；`README.md:122`：「工具只修改自己认识的行，其余字节原样保留。」
- **质询**：`--adopt` 当前不是 Markdown-aware 迁移，而是逐行前缀扫描。它至少有两个可证伪后果：会给代码围栏或缩进代码里的示例 checkbox 添加 ID；会让原文件自带二级标题下的真实任务变成游离任务，默认 `todo list` 看不见。对「普通 GFM 清单」作无条件接管承诺，会直接触碰用户数据与可见性安全线。
- **立场证据**：立场文档 §3 第 1、2 点把「人写内容原样保留」与「字节区间外科手术」列为核心机制；§4 对照项 7 也只认可安全编辑成功后回写的路径。`adopt` 绕开解析器的行为与这条立场正面冲突。
- **仓库核实**：`docs/format-spec-v1.md:74` 明确规定代码围栏和行内代码中的 checkbox 不是任务；但 `crates/todo-cli/src/main.rs:707` 的 `adopt_source` 只对每行做 `trim_start` 和 `starts_with("- [ ]")` 判断，没有围栏状态，随后直接追加 `<!--t:...-->`。它还先插入 `## Todo`，再原样接上旧文档；按 `docs/format-spec-v1.md:52`，旧文档中不匹配列名的 `## Tasks`、`## 工作` 等标题会使其下任务成为 `column: null`。`crates/todo-cli/src/main.rs:188` 的默认 `list` 又过滤掉 `column.is_none()` 的任务。
- **可证伪样本**：一个包含 ` ```markdown` / `- [ ] 示例` / ` ``` ` 的说明文件会被改写示例；一个包含 `## Tasks` 后真实清单的文件，接管后 `todo list` 默认可能返回空列表，只有 `--all` 才能看见游离任务。
- **严重度**：高。
- **建议修改方向**：在修复前把 README 收窄为只支持无围栏、无自定义二级标题的简单清单，并要求先备份。实现上应复用 Markdown-aware 扫描，提供 `--dry-run`/迁移预览，明确列映射，写前备份，并为上述两类样本增加回归测试。

## A4 「Rust 1.74 或更高」与当前锁定依赖不相容

- **README 位置与原文**：`README.md:15`：「Rust 1.74 或更高」，依据是「`Cargo.toml` 中的 `rust-version`」。
- **质询**：声明自己的 `rust-version` 不等于依赖图真的支持该版本。按当前 `Cargo.lock`，README 的构建命令无法在 Rust 1.74 上完成；这是环境要求的事实错误，不是措辞偏好。
- **立场证据**：立场文档 §3 将 `todo-core`、`todo-cli` 和 Tauri host 都列为当前实现，§5 第 5 点又指出尚无 release 分发。因此源码构建是目前唯一入口，错误的 MSRV 会直接封死首次使用，而不是影响边缘贡献者。
- **仓库核实**：根 `Cargo.toml:7` 确实只声明了 `rust-version = "1.74"`。但只读命令 `cargo metadata --offline --format-version 1` 显示当前 CLI 依赖 `clap 4.6.6` 的 `rust_version` 为 1.85，桌面依赖中的 `time 0.3.55`、`plist 1.10.0`、`serde_with 3.22.0` 等要求 1.88。`cargo tree --offline -i clap@4.6.6` 将其追溯到 `todo-cli`；`cargo tree --offline -i time@0.3.55` 将其追溯到 Tauri/`todo-desktop`。锁文件版本可在 `Cargo.lock:375`、`Cargo.lock:2531`、`Cargo.lock:3178`、`Cargo.lock:3871` 回查。
- **严重度**：高。
- **建议修改方向**：二选一：把 README 与 workspace MSRV 提升到经验证的实际值（当前整仓至少 1.88），或锁定/降级依赖并在 CI 中真实执行 `cargo +1.74 test --workspace --locked`。在 CI 证明之前，不应把清单字段当作验证结果。

## A5 CLI 快速开始实际上会构建桌面端

- **README 位置与原文**：`README.md:19`：「桌面端另需各平台的 Tauri 2 系统依赖。」；`README.md:34` 至 `README.md:39` 的统一构建步骤是 `pnpm install` 后直接 `cargo build`；随后 `README.md:41` 才进入 CLI 使用。
- **质询**：「桌面端另需」暗示只用 CLI 可以不装桌面系统依赖，但根目录 `cargo build` 会构建全部默认 workspace member，包括 `todo-desktop`。README 没有给 CLI 用户一条真正独立的构建路径。
- **立场证据**：立场文档 §3 把 core、CLI、desktop 明确分成三个模块，且强调 CLI 是 canonical interface。这种分层若不能反映到快速开始，CLI 作为低门槛标准接口的价值就被桌面工具链绑架。
- **仓库核实**：根 `Cargo.toml:2` 把 `apps/desktop/src-tauri` 与两个 crate 并列为 workspace member；未设置 `default-members`。只读 `cargo metadata --no-deps --format-version 1` 返回的 `workspace_default_members` 包含 `todo-core`、`todo-cli`、`todo-desktop` 三者。桌面 crate 在 `apps/desktop/src-tauri/Cargo.toml:11` 至 `apps/desktop/src-tauri/Cargo.toml:19` 引入 Tauri build/runtime 与平台插件。因此文档中的 `cargo build` 不是 CLI-only 构建。
- **真实受众伤害**：只想试 CLI 的 Linux/CI 用户可能先因 WebKit/GTK 等 Tauri 系统库缺失而失败，误以为 CLI 本身不可构建；`pnpm install` 对 CLI 也属不必要前置。
- **严重度**：中。
- **建议修改方向**：拆成两条：CLI 快速开始使用 `cargo build -p todo-cli`，只需 Rust；完整 workspace/桌面构建再要求 Node、pnpm 与 Tauri 系统依赖。把二进制路径与对应命令放在各自步骤下。

## A6 密钥泄露警告错误地只点名 `preview`

- **README 位置与原文**：`README.md:75`：「`todo hooks preview` 会打印本地 agent 配置文件的完整内容，其中可能包含 API 密钥。不要把该输出贴到公开渠道。」
- **质询**：为什么只警告 `preview`？当前 `install` 和 `uninstall` 同样把修改后的完整 `~/.claude/settings.json` 打到 stdout，其中同样可能保留 API 密钥。README 的限定会让用户误以为安装/卸载输出适合贴到 issue 或 CI 日志。
- **立场证据**：立场文档 §3 第 4 点把 hooks 定为替代 MCP 的正式接入层，而非调试玩具。既然接入层是核心路径，其秘密处理必须符合 §3 第 2、3 点反复强调的「可审计且不销毁信息」安全纪律，不能只给一个不完整警告。
- **仓库核实**：`crates/todo-cli/src/main.rs:231` 至 `crates/todo-cli/src/main.rs:249` 对 `preview`、`install`、`uninstall` 都调用同一个 `emit`；`crates/todo-cli/src/main.rs:254` 的 `hooks_human` 无条件打印每个 target 的 `content`。`crates/todo-cli/src/hooks.rs:80` 读取完整 settings，`crates/todo-cli/src/hooks.rs:158` 又把完整输出放入 `HookTarget.content`。因此三条命令的泄密面相同，JSON 模式也包含该字段。
- **严重度**：高。
- **建议修改方向**：实现优先：`install`/`uninstall` 默认只输出目标、是否修改和备份路径；完整 diff 必须显式 `--show-content`，并对已知 secret 键脱敏。文档至少应立即把警告扩展到所有会返回完整配置内容的 hooks 命令及 `--json` 输出。

## A7 README 把 Claude 与 Codex 写成同一种安装，实际 Codex `install` 是 no-op

- **README 位置与原文**：`README.md:67`：「`todo hooks preview|install|uninstall|instructions [--agent claude|codex|all]`｜管理 agent 接入。」
- **质询**：这张表掩盖了两套完全不同的行为：Claude 的 `install` 写全局 SessionStart；Codex 的 `install` 明确不改任何配置；Codex 真正可写入的路径是 `hooks instructions --agent codex --write`，而 README 连 `--write` 都没有列出。一个名为 `--agent all` 的成功命令并没有把「all」都接上。
- **立场证据**：立场文档 §3 第 4 点已经准确写明「Codex 无 SessionStart 等价物，仅提供指令块」；§5 第 5 点又把这一点列为当前缺口。README 抹平这个差异，反而比立场文档更不准确。
- **仓库核实**：`crates/todo-cli/src/hooks.rs:70` 至 `crates/todo-cli/src/hooks.rs:75` 对 Codex install 只返回「no config change」说明；`crates/todo-cli/src/main.rs:100` 定义了 `instructions --write`，`crates/todo-cli/src/hooks.rs:195` 只有 `write == true` 才改当前目录的 `AGENTS.md`/`CLAUDE.md`。实跑 `target/debug/todo hooks instructions --help` 也显示 `--write`，README 的命令表却省略了唯一产生写入的参数。
- **真实受众伤害**：Codex 用户执行表面上最自然的 `todo hooks install --agent codex` 后，agent 仍不会获得任务规则；若只执行 README 展示的 `instructions`，也只是把内容打印出来，不会安装。
- **严重度**：中。
- **建议修改方向**：不要用一行合并四个子命令。分别给出 Claude：`preview` → `install` → 验证；Codex：`instructions --agent codex --write` → 检查受管块。明确 Claude 修改 `~/.claude/settings.json`，Codex 修改当前项目 `AGENTS.md`，并把 Codex 无自动 SessionStart 注入列为限制。

## A8 Claude hook 依赖裸 `todo` 在 `PATH`，失败却被静默吞掉

- **README 位置与原文**：`README.md:45` 至 `README.md:48` 全部用绝对路径调用二进制；`README.md:51` 仅说把 `target/debug` 加入 `PATH` 后「可直接使用 `todo`」；`README.md:67` 又把 hooks 当成可用的 agent 接入。
- **质询**：按快速开始原样使用绝对路径、但不修改 `PATH` 的用户，可以正常运行所有示例，却会安装一个永远找不到 `todo` 的 SessionStart hook。更糟的是，hook 用 `|| true` 吞掉错误，用户看到的是成功而不是接入失败。README 没有把 `PATH` 从「调用便利」提升为 hooks 的硬前置条件。
- **立场证据**：立场文档 §3 第 4 点称 hooks 的价值在于会话开始时主动注入，而不是等 agent 记得调用；§5 第 5 点承认 Codex 一侧尚有缺口。Claude 一侧若因路径静默失效，README 所依赖的「不被忽略」机制同样落空。
- **仓库核实**：`crates/todo-cli/src/hooks.rs:13` 的安装命令硬编码为裸 `todo hooks context ... || true`；`crates/todo-cli/src/hooks.rs:266` 的内部子进程也再次用裸 `todo`，spawn 失败即返回空文本。`crates/todo-cli/tests/hooks.rs:144` 甚至把「PATH 中没有 todo 时 shell 仍成功」锁成了测试行为，证明这是确定的静默降级，不是推测。
- **严重度**：中。
- **建议修改方向**：安装时用 `current_exe()` 解析并正确 shell-escape 绝对路径，或在写配置前做 PATH preflight；安装结果必须给出可运行的验证命令和「本次注入是否产生内容」。如果保留 fail-open，也要区分「不阻断 agent」与「悄悄宣称安装成功」。

## A9 `lint` 不检查最关键的重复 ID

- **README 位置与原文**：`README.md:64`：「`todo lint`｜检查格式问题，发现问题时退出码为 6。」
- **质询**：「检查格式问题」没有范围限定，但当前 lint 连格式中最重要的全局唯一 ID 都不检查。CI 得到 exit 0，不代表任务身份没有损坏。README 又把验证日志称作独立证据，而该日志声称「注入重复 ID，正确检出」，与当前源码无法同时成立。
- **立场证据**：立场文档 §3 第 1 点把稳定 ID 称为格式核心，§3 第 3 点说明外部引用与对账都依赖它；重复 ID 会破坏的正是项目最重要的审计链，而不是次要样式。
- **仓库核实**：`docs/format-spec-v1.md:83` 要求 ID 全局唯一。当前 `lint` 在 `crates/todo-cli/src/main.rs:487` 至 `crates/todo-cli/src/main.rs:526` 只收集解析错误与非标准 checkbox；解析器在 `crates/todo-core/src/parse.rs:104` 至 `crates/todo-core/src/parse.rs:134` 遇到重复 ID 只保留第一次映射，不报错。重复 ID 是在 `crates/todo-core/src/reconcile.rs:188` 才被发现。与此同时，`docs/verification-log.md:54` 仍写着 lint 注入重复 ID「正确检出」，说明 README 指向的验证结论已与当前命令职责漂移。
- **严重度**：高。
- **建议修改方向**：优先让 `todo lint` 覆盖稳定 ID 唯一性、非法元数据和列定义等纯格式不变量；增加 CLI 回归测试，确保重复 ID 返回 6。在此之前，README 必须列出 lint 的实际检查范围，并明确重复 ID 需由 `reconcile` 检查；验证日志应标注适用 commit 或更新结论。

## A10 `--json` 在用法错误时没有所谓稳定 envelope

- **README 位置与原文**：`README.md:69`：「全局选项：`--json` 输出稳定 envelope」；`README.md:71` 又定义了用法错误退出码 2。
- **质询**：这句话没有成功路径限定，agent/脚本自然会期待错误也机器可读。但参数缺失、未知子命令等最常见的用法错误绕过了 JSON emitter：exit 2，stdout 为空，stderr 是 Clap 人类文本。
- **立场证据**：立场文档 §3 的 CLI 模块把「`--json` 稳定 envelope + 6 档退出码」并列为 canonical CLI 的工程能力。既然这是给 agent 和脚本的标准接口，就不能只在进入业务逻辑后才稳定。
- **仓库核实**：`crates/todo-cli/src/main.rs:169` 使用 `Cli::parse()`；Clap 在解析失败时直接退出，后面的 `emit_error` 不会运行。只读实跑 `target/debug/todo --json nope` 与 `target/debug/todo --json add`：两者均 exit 2、stdout 为空，stderr 分别是 `unrecognized subcommand` 与缺少 `<BODY>` 的人类帮助文本。
- **严重度**：中。
- **建议修改方向**：改用 `try_parse()`，检测 `--json` 后把 usage error 包成 `todo.error.v1`，并把帮助文本作为字段或单独选项；若暂不实现，README 必须限定为「命令成功及运行期错误使用 envelope；参数解析错误仍为 stderr 文本」。

## A11 退出码 6 不只表示 lint 问题

- **README 位置与原文**：`README.md:64` 说 lint 发现问题时退出 6；`README.md:71` 总表把 `6` 定义为「lint 发现问题」。
- **质询**：`todo reconcile` 只要存在 confirmed 或 suspected Finding 也返回 6。把 6 写成 lint 专用，会让调用方把正常的对账结果误分类，也使 README 自己的命令表内部不完整。
- **立场证据**：立场文档 §3 的 CLI 描述强调六档退出码是稳定契约；§3 第 3 点又把 Finding 分成 confirmed/suspected。稳定契约必须覆盖对账这一核心命令，而不是沿用早期 lint-only 文案。
- **仓库核实**：`crates/todo-cli/src/main.rs:555` 至 `crates/todo-cli/src/main.rs:559` 明确在任何 confirmed/suspected 非空时返回 6。项目自己的受管指令块在 `crates/todo-cli/src/hooks.rs:211` 已把 6 更准确地写成 `findings`，与 README 和 CLI `--help` 的 `lint issues` 文案构成三方不一致。
- **严重度**：中。
- **建议修改方向**：将 6 定义为「命令完成，但发现需要关注的质量/对账问题」，再逐命令解释 `lint issues` 与 `reconciliation findings`。如果自动化需要区分，稳定 JSON error/finding code 才是分类依据，不应只靠进程码。

## A12 CLI、格式规范与桌面对元数据契约各说各话

- **README 位置与原文**：`README.md:61` 把 CLI 参数写成 `--due <日期>` 与 `--priority <1-3>`；`README.md:3` 又称 CLI 是标准接口、桌面端是视图。
- **质询**：canonical CLI、格式 spec 和桌面视图至少有两处确定漂移。其一，规范与 CLI 都支持优先级 4，README 却只写 1–3，桌面前端还会把合法的 4 静默转成 `null`。其二，README 没写日期格式，CLI 所谓校验只检查 `YYYY-MM-DD` 的字符形状，会接受 `2026-99-99`；桌面 Today 再用真实日历解析并把它静默排除。标准接口产生的数据不应在标准视图中消失。
- **立场证据**：立场文档 §3 第 1 点把元数据 token 视为 canonical 文件模型，模块表又声明桌面端直接复用 core。该架构的价值就是跨入口同一语义；当前差异证明 README 所称「标准接口 / 视图」还没有端到端守约。
- **仓库核实**：优先级方面，`docs/format-spec-v1.md:98` 定义 `!1` 至 `!4`，`crates/todo-cli/src/main.rs:320` 接受 1..=4；但 `apps/desktop/src/lib/desktop.ts:6` 的类型只有 1、2、3，`apps/desktop/src/lib/desktop.ts:212` 至 `apps/desktop/src/lib/desktop.ts:223` 会把 4 归一化为 `null`，卡片因此不显示。日期方面，`docs/format-spec-v1.md:12` 要求 ISO 8601；CLI 的 `valid_date` 在 `crates/todo-cli/src/main.rs:697` 只检查长度、连字符位置和数字，解析器在 `crates/todo-core/src/parse.rs:568` 也相同；桌面端却在 `crates/todo-core/src/desktop.rs:313` 用 `NaiveDate::parse_from_str` 做真实日历校验，失败即当作无日期。
- **严重度**：中。
- **建议修改方向**：建立 core 层共享的 `Priority` 与 `DueDate` 验证类型，CLI、parser、desktop 共用；README 明写 `YYYY-MM-DD` 和 1–4。加入契约测试：CLI 创建 priority 4 和非法日历日期后，桌面 Snapshot 的行为必须与 CLI 结果一致，非法输入应在写盘前拒绝。

## 总体裁决

README 已经讲清了「Markdown 唯一事实源」「派生投影」「默认只报告」「物证 token 不可删除」这些正确边界；问题不在于篇幅短，而在于它把尚未接通的语义对账写成当前能力，同时没有给出能激活结构对账的首用流程。更危险的是，数据接管、hooks 密钥输出和构建门槛三处存在可直接伤害用户的事实偏差。作为项目门面，当前版本足以介绍仓库结构，却还不能充当可信的安装、迁移与 agent 接入手册。

## 总攻击清单

| 编号 | 一句话标题 | 严重度 |
|---|---|---|
| A1 | 对账被写成「核对代码实际状态」，实际 CLI 只接上了结构层 | 高 |
| A2 | 快速开始绕过了产品核心证据闭环 | 高 |
| A3 | `init --adopt` 会改写假任务并让现有任务隐身 | 高 |
| A4 | Rust 1.74 环境门槛与当前锁定依赖不相容 | 高 |
| A5 | CLI 快速开始实际上会构建桌面端 | 中 |
| A6 | `install`/`uninstall` 同样会回显含密钥的完整配置 | 高 |
| A7 | Codex 的 `install` 是 no-op，真正写入参数却缺席 | 中 |
| A8 | Claude hook 的 PATH 前置条件失败时被静默吞掉 | 中 |
| A9 | `lint` 不检查最关键的重复 ID | 高 |
| A10 | `--json` 在用法错误时没有稳定 envelope | 中 |
| A11 | 退出码 6 不只表示 lint 问题 | 中 |
| A12 | CLI、规范与桌面对元数据契约各说各话 | 中 |
