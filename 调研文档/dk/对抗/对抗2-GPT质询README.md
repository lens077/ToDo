# 对抗 2：以实现对照立场质询 README

> 质询对象：仓库根 `README.md`（128 行）。
> 立场依据：`调研文档/dk/我的理解-调研方案与当前实现对照2.md`。
> 核验基线：当前 `HEAD` 为 `1669194`；除通读文档与源码外，使用现有 `target/debug/todo` 做了只读命令探测。

## 判定口径

- 只攻击会影响安装、数据安全、自动化契约、核心定位或能力判断的错误与缺位。
- 「README 没有复述深度文档」本身不构成问题；只有当缺失会让 README 的真实受众作出错误操作或错误能力判断时才列入。
- 严重度分为：高（会阻断快速开始、危及数据或扭曲核心卖点）、中（会破坏集成或形成显著错误预期）、低（局部接口事实错误，但有其他路径可纠正）。

## 逐条质询

### B1：宣称支持 Rust 1.74，但锁定依赖要求 Rust 1.88

- **README 原文（第 15 行）**：`| Rust | 1.74 或更高 | Cargo.toml 中的 rust-version |`
- **证据**：
  - 立场文档 §4.1 把三个 Rust 成员视为同一个 workspace，§4.8 只证明当前环境下门禁通过，没有做 MSRV 验证。
  - 根 `Cargo.toml:7` 的确声明 `rust-version = "1.74"`，但这只约束本仓库 package，不能替锁定的传递依赖背书。
  - `Cargo.lock:3523` 锁定 `tauri 2.11.5`；该 crate 自身声明 `rust-version = "1.77.2"`。更直接的是，`Cargo.lock:3595` 中的 `tauri-codegen` 依赖 `time`，当前锁定的 `time 0.3.55`（`Cargo.lock:3871`）声明 `rust-version = "1.88.0"`；`tauri-utils` 依赖的 `serde_with 3.22.0` 也声明 `rust-version = "1.88"`。
  - README 的快速开始运行裸 `cargo build`，虚拟 workspace 默认会构建桌面成员，因此不能用「只构建 CLI」规避上述依赖。当前实测环境是 `rustc 1.97.1`，也不能反证 1.74 可用。
- **可证伪结论**：在 README 明示支持的 Rust 1.74 环境中按第 36–39 行运行 `cargo build --locked`，Cargo 会因依赖 MSRV 高于编译器而拒绝构建。README 把 package 声明误当成了完整依赖图的最低版本证明。
- **严重度：高**。这是快速开始的硬阻断，而且发生在 README 明确承诺支持的环境中。
- **建议修改方向**：要么把最低版本改为当前锁定图真实要求的 1.88；要么固定兼容 1.74 的依赖并增加 `cargo +1.74 build --workspace --locked` CI。没有真实 MSRV 门禁前，不要以根 manifest 的单个字段作证明。

### B2：`init --adopt` 会把常见带二级标题的 GFM 任务变成默认不可见的游离任务

- **README 原文（第 53 行）**：`已有普通 GFM 清单时，用 todo init --adopt 接管现有文件并补齐稳定 ID。`
- **证据**：
  - 立场文档 §4.2 明确：无法匹配 `columns[].name` 的 `##` 标题下，任务必须成为 `column: null` 的游离任务；§6 又把 `init --adopt` 描述成已经解决的接管路径。
  - `crates/todo-cli/src/main.rs:707` 的 `adopt_source` 先写入默认的 `## Todo`，随后原样复制旧文件，最后追加 `## Doing` 与 `## Done`。它不会把旧文件中的 `## Backlog`、`## Tasks` 等标题映射为列。
  - `crates/todo-core/src/parse.rs:273` 按最近的二级标题决定列，只有标题名匹配 frontmatter 列定义才返回列 ID；`crates/todo-cli/src/main.rs:188` 的默认 `list` 又过滤掉 `column.is_none()` 的任务。
  - 桌面端同样不是兜底：`apps/desktop/src/components/BoardView.tsx:55` 只把 `task.column === column.id` 的任务放入列。
- **可证伪结论**：对含 `## Backlog` 与任务的普通 GFM 文件执行接管后，任务仍在文件中、也可能获得 ID，但会从默认 `todo list` 和看板消失；只有 `todo list --all` 才看得到。这不是数据丢失，却是危险的「接管成功但主视图静默隐藏」。
- **严重度：高**。迁移入口会制造错误的完成感，用户可能在看板中误以为原任务不存在。
- **建议修改方向**：接管前扫描并展示标题映射方案；不能无歧义映射时拒绝接管或要求显式 `--map-column`。至少在 README 限定「仅适用于无二级分组的平铺 GFM 清单」，并要求接管后运行 `todo list --all` 检查游离任务。

### B3：把「核对代码实际状态」写成现有能力，但默认语义对账实际上是空开关

- **README 原文（第 7 行）**：`对账引擎用 Git 物证核对任务与代码的实际状态，只报告，不擅自改写完成状态。`
- **README 原文（第 66 行）**：`todo reconcile [--apply] [--no-semantic]`
- **证据**：
  - 立场文档 §4.4 已明确写出：`SemanticReconciler` trait 存在，但当前只有 `MockSemanticReconciler`，真实 LLM 判定尚未接入；§7 再次把它列为未兑现项。
  - `crates/todo-core/src/reconcile.rs:119` 的 `Reconciler::new` 把 `semantic` 设为 `None`；只有显式调用 `with_semantic` 才会安装语义实现。CLI 在 `crates/todo-cli/src/main.rs:534` 只调用 `Reconciler::new(".").reconcile(...)`，从未调用 `with_semantic`。
  - 当前二进制只读探测中，`todo reconcile --json` 与 `todo reconcile --no-semantic --json` 输出逐字段相同；默认结果甚至没有 `skipped` 信息说明语义层未配置。
  - 当前 Git 层只验证 commit 是否存在、是否被 revert，并提取 changed files（`reconcile.rs:348`）；它不读取代码内容，也不能判断任务验收标准是否已满足。
- **可证伪结论**：`--no-semantic` 在公开 CLI 中不改变任何行为。README 的「任务与代码的实际状态」把已经实现的物证完整性检查扩大成尚未实现的语义完成度判断。
- **严重度：高**。这是产品最硬差异化的能力边界；误写会直接改变用户对「同步事实」可信度的判断。
- **建议修改方向**：改为「当前核验结构问题、commit 存在性与 revert 状态；代码语义判定尚未接入」。在语义 provider 接通前移除或标注 `--no-semantic` 为预留，并在报告中明确 `semantic check skipped: no provider configured`。

### B4：「对账不改状态」掩盖了第一方 agent 指令仍让 agent 自己写 Done

- **README 原文（第 7 行）**：`只报告，不擅自改写完成状态。`
- **README 原文（第 62、67 行）**：README 同时公开 `todo done <id>` 和 agent hooks，却没有说明谁有权调用状态写命令。
- **证据**：
  - 立场文档 §5.3 把最关键分歧定义为「谁有权写完成状态」，结论是 agent 记录 Evidence、对账提出报告、完成状态由人确认；它还引用「creates 可靠、completes 有损」说明不能信任 agent 自报完成。
  - 仓库自研调研 `research/README.md:114` 的裁决更明确：语义检查应「LLM 提议 + 人确认」，agent 完成后进入 InReview，而不是直接 Done。
  - 实际生成的托管指令 `crates/todo-cli/src/hooks.rs:212` 却写着 `finish with todo done <id>`，并在下一条要求 agent 提交后执行 `todo evidence ...`，随后立即 `todo done <id>`。
- **可证伪结论**：README 的句子只限制 `reconcile` 本身，并没有限制被 hooks 管理的 agent。按第一方生成的工作流，造成漂移风险的同一个 agent 仍可在没有人工确认的情况下写完成状态。README 用狭义正确句遮住了治理层面的相反行为。
- **严重度：高**。这不是 README 少写一段原理，而是核心信任模型与实际 agent 接入相冲突。
- **建议修改方向**：先拍板并写死权限模型。若坚持人确认，托管指令应止于 Evidence 和「待确认」报告，不得调用 `todo done`；若允许 agent 直接 Done，README 必须明确条件、风险和恢复路径，并停止宣称「人确认」是现行闭环。

### B5：外部修改检测与退出码 5 没有接入 CLI 或桌面写入主链路

- **README 原文（第 124 行）**：`写入走文件锁与原子替换；检测到外部修改时放弃本次写入并要求重试。`
- **README 原文（第 71 行）**：`5 乐观锁冲突`
- **证据**：
  - 立场文档 §4.3 把 advisory lock、mtime + hash 乐观锁和原子写列为已经组合落地的三层防护。
  - `crates/todo-core/src/write.rs:63` 的确实现了独立 `OptimisticLock`，但仓库全局引用只出现在导出和 `crates/todo-core/tests/core.rs:135` 的单元测试中；CLI、`DesktopBackend` 和 `LockedWriter` 均未调用它。
  - 生产写入实际走 `crates/todo-core/src/lock.rs:34`：取得 advisory lock 后读文件、推导改动、`apply_changes`，随后直接 `atomic_write`。这能串行化遵守同一锁的写入者，却不会检测不遵守该锁的编辑器在「读后、rename 前」写入的内容。
  - `crates/todo-cli/src/main.rs:685` 虽把 `ConcurrentModification` 映射为退出码 5，但公开命令没有可达路径产生该错误。
- **可证伪结论**：退出码 5 是已声明但当前不可达的 CLI 契约；README 所说的外部修改检测也不是 CLI/桌面主链路的真实行为。外部编辑若恰好落入读写窗口，存在被后续 rename 覆盖的风险。
- **严重度：高**。README 在本地文件工具最关键的数据安全属性上作了超出现实的保证。
- **建议修改方向**：把 mtime + hash 校验接入所有 `LockedWriter` 写入路径，并增加「不遵守 advisory lock 的外部进程并发写」黑盒测试；在接线前删除「检测到外部修改」与退出码 5 的现状式表述，或明确标为保留契约。

### B6：hooks 命令表制造了 Claude 与 Codex 对称接入的假象

- **README 原文（第 67 行）**：`todo hooks preview|install|uninstall|instructions [--agent claude|codex|all] | 管理 agent 接入`
- **证据**：
  - 立场文档 §4.5 已说明真实差异：只有 Claude 有 SessionStart hook；Codex 只能写指令块。§7 也把「hooks 自动注入目前只覆盖 Claude」列为未兑现项。
  - `crates/todo-cli/src/hooks.rs:56` 只有 Claude 的 install/uninstall 会生成 `~/.claude/settings.json` 目标；Codex 分支在非 instructions 操作中固定返回「no config change」。
  - 只读实测 `todo hooks install --agent codex --json` 返回 `changed:false`，并明确要求改用 `todo hooks instructions --agent codex`。
  - 但 `instructions` 默认也只预览；真正写入 `AGENTS.md` 还必须加 README 未列出的 `--write`（`crates/todo-cli/src/main.rs:103`）。
- **可证伪结论**：Codex 用户照命令表选择 `install` 不会安装任何东西；改跑 `instructions` 若没有自行发现 `--write`，仍不会写文件。`--agent claude|codex|all` 是参数对称，不是能力对称。
- **严重度：中**。不会损坏数据，但会让核心 agent 接入在 README 路径上无法落地，并造成「已安装」的错误预期。
- **建议修改方向**：按 agent 拆成两条可复制流程：Claude 使用 `hooks install` + 可选 instructions，Codex 使用 `hooks instructions --agent codex --write`；显式写明 Codex 无 SessionStart 自动注入，`all` 也不会改变这一限制。

### B7：密钥泄露警告只点名 preview，但 install/uninstall 同样输出完整配置

- **README 原文（第 75 行）**：`todo hooks preview 会打印本地 agent 配置文件的完整内容，其中可能包含 API 密钥。不要把该输出贴到公开渠道。`
- **证据**：
  - 立场文档 §4.5 指向 `crates/todo-cli/src/hooks.rs` 作为 agent 接入实现。
  - `crates/todo-cli/src/main.rs:231`、`:236`、`:241` 对 preview、install、uninstall 都调用相同的 `emit(..., hooks_human)`；`hooks_human` 在 `main.rs:254` 无差别打印每个 target 的 `content`。
  - Claude 的 `settings_target` 会把整个现有 `~/.claude/settings.json` 解析、合并后放入 `HookTarget.content`（`hooks.rs:80`、`:158`），并非只返回本工具新增的 diff。`--json` 同样序列化该完整字段。
- **可证伪结论**：不仅 preview，install 与 uninstall 的标准输出也可能包含原配置中的 API 密钥。README 的命令特指会让用户误以为其他两个操作的日志可以安全贴出或留在 CI。
- **严重度：高**。这是可导致凭据进入终端录屏、CI 日志或支持工单的直接安全风险。
- **建议修改方向**：实现上只输出目标路径、changed、backup 和本工具管理块的 diff，默认脱敏其他字段；在修复前把警告扩展为 preview/install/uninstall 以及 JSON 模式全部适用。

### B8：README 把「已实现的 SQLite 组件」写成了当前产品链路的一部分，却不说 CLI 和桌面完全未使用它

- **README 原文（第 6 行）**：`SQLite 只是派生投影，可随时从 Markdown 重建。`
- **README 原文（第 24、112–116 行）**：目录说明把 SQLite 投影列入 core，并提供投影性能基准。
- **证据**：
  - 立场文档 §4.7 明确区分「已实现」与「已接入」：投影具备全量/增量能力，但当前未接入 `list` 与桌面主链路；§7 把这一点列为尚未兑现项。
  - 仓库全局检索 `Projection` 的生产用法，只出现于 `todo-core` 导出；构造和 `rebuild` 调用只在 projection 测试与 benchmark example 中。
  - CLI 的 `load` 在 `crates/todo-cli/src/main.rs:599` 每次直接 `parse_file("TODO.md")`；桌面 `open_project`、`snapshot` 在 `crates/todo-core/src/desktop.rs:121`、`:128` 同样直接解析 Markdown。
  - 自举清单 `TODO.md:25` 仍有「评估 list 接入 SQLite 投影的触发条件」未完成任务。
- **可证伪结论**：9 ms 的投影 benchmark 是孤立组件数字，不是当前 `todo list` 或桌面 Snapshot 的执行路径。README 的现在时架构描述会让读者合理推断运行时已经维护派生库。
- **严重度：中**。它会误导性能分析、故障排查和二次开发者对数据流的判断，但不会直接造成数据损失。
- **建议修改方向**：补一句「投影已实现并通过独立 benchmark，但 v1 的 CLI/桌面目前直接解析 Markdown，尚不创建或读取索引库」；同时写明约 10,000 任务或常驻进程的接入触发条件。

### B9：README 把合法优先级 4 从 CLI 契约中删掉了

- **README 原文（第 61 行）**：`--priority <1-3>`
- **证据**：
  - 立场文档 §4.2 的格式还原明确写 `!1`–`!4`，§4.5 把该格式作为 CLI 能力基础。
  - 权威规范 `docs/format-spec-v1.md:98` 定义优先级为 `!1`..`!4`。
  - CLI 在 `crates/todo-cli/src/main.rs:320` 只拒绝不在 `1..=4` 的值，解析器在 `crates/todo-core/src/parse.rs:518` 也接受 4。
- **可证伪结论**：`todo add ... --priority 4` 是有效命令，但 README 明确告诉人和 agent 该值不在范围内。
- **严重度：低**。功能仍可用，但 README 与格式规范、实现三方不一致，会让生成客户端或 agent 指令错误限缩取值。
- **建议修改方向**：把命令表改为 `<1-4>`，并在 CLI 参数层添加 `value_parser`，让 `--help` 也显示和强制同一范围。

### B10：退出码 6 不只表示 lint 问题，`reconcile` 有任何 finding 也会返回 6

- **README 原文（第 64、71 行）**：`todo lint ... 发现问题时退出码为 6`；退出码表进一步写成 `6 lint 发现问题`。
- **证据**：
  - 立场文档 §4.5 说退出码 0–6 各有定义，但没有揭示 README 与实际命令共享该码的差异。
  - `crates/todo-cli/src/main.rs:555` 对 `reconcile` 的 `confirmed` 或 `suspected` 只要任一非空就返回 6，不论是否运行 lint。
  - 同一仓库生成的 agent 指令反而把 6 写成通用 `findings`（`crates/todo-cli/src/hooks.rs:212`），与 README、顶层 `--help` 的 `lint issues` 口径冲突。
- **可证伪结论**：自动化不能依据 README 把退出码 6 解释为「执行的是 lint 或仅有格式错误」；对账发现失效物证、孤儿依赖或疑似完成也会得到同一码。
- **严重度：中**。hooks、CI 与 agent 编排通常按退出码分支，错误语义会导致误报、错误恢复或把有效 Finding 当成未知失败。
- **建议修改方向**：统一写成「6：检查发现问题（`lint` issue 或 `reconcile` finding）」，并要求调用方通过 JSON `schema` 区分命令结果；同步修正 Clap `after_help`。

### B11：`--json` 并不保证所有错误都有 JSON envelope

- **README 原文（第 69 行）**：`全局选项：--json 输出稳定 envelope`
- **证据**：
  - 立场文档 §4.5 把稳定版本化 envelope 视为 CLI/未来 MCP 薄壳的机器契约。
  - `crates/todo-cli/src/main.rs:169` 使用 `Cli::parse()`；Clap 在参数或子命令错误时直接打印并退出，控制流不会进入 `emit_error`。
  - 当前二进制只读实测 `target/debug/todo --json nope`：退出码为 2，输出是人类可读的 `error: unrecognized subcommand 'nope'` 与 Usage 文本，而不是 JSON。
- **可证伪结论**：脚本不能无条件把 `--json` 的输出送入 JSON parser；恰恰在最需要稳定错误契约的用法错误上，README 的保证失效。
- **严重度：中**。这会直接破坏 agent、编辑器插件或未来 MCP 薄壳的健壮性。
- **建议修改方向**：若只承诺运行期错误，README 应明确「参数解析成功后输出 JSON envelope，Clap 用法错误除外」；更好的修复是使用 `try_parse` 捕获 Clap 错误，并在 `--json` 下输出版本化 `todo.error.v1`。

### B12：「安全编辑入口」没有在桌面章节就地披露三条关键原生路径仍待验收

- **README 原文（第 3 行）**：`桌面端只是视图和安全编辑入口。`
- **README 原文（第 100 行）**：`桌面端提供看板、今日视图和随记账本。`
- **证据**：
  - 立场文档 §7 明确列出三项尚未完成的桌面人工验收：原生文件选择器、看板写回、外部编辑刷新；前端 Playwright 也尚未纳入 CI。
  - 自举清单 `TODO.md:19`、`:20`、`:21`、`:27` 仍把这四项列为未完成。
  - `docs/verification-log.md:167` 明确说明自动化因 macOS Accessibility 权限不足未覆盖这些真机路径，`:173` 要求正式分发前人工走一次；`:228` 再次声明 Playwright 未纳入 CI。
  - README 第 118 行虽链接验证日志，但桌面章节本身没有任何「开发 MVP」「原生路径待验收」限定。
- **可证伪结论**：现有证据证明底层 core、browser mock 和 IPC 契约受到测试，不证明 README 所称的完整原生「安全编辑入口」已经端到端验收。这里攻击的是成熟度表述，不是断言功能必然失效。
- **严重度：中**。README 的受众会按入口文案拿真实仓库试用；对文件选择、写回和外部刷新状态的错误信心具有直接数据安全成本。
- **建议修改方向**：在桌面章节就地标注「开发 MVP」，列出三条待人工验收路径与 Playwright CI 缺口；完成并记录真机验收后再恢复无条件的「安全编辑入口」表述。

## 总攻击清单

| 编号 | 一句话标题 | 严重度 |
|---|---|---|
| B1 | Rust 1.74 承诺与锁定依赖的 1.88 MSRV 冲突 | 高 |
| B2 | `init --adopt` 会把带二级标题的旧任务静默变成游离任务 | 高 |
| B3 | 「核对代码实际状态」过度承诺，默认语义对账是空开关 | 高 |
| B4 | README 避谈完成状态权限，第一方 hooks 仍让 agent 自写 Done | 高 |
| B5 | 乐观锁未接入生产写入路径，退出码 5 当前不可达 | 高 |
| B6 | hooks 参数表掩盖 Claude/Codex 能力不对称与 `--write` 前提 | 中 |
| B7 | install/uninstall 也会打印完整配置，密钥警告范围错误 | 高 |
| B8 | SQLite 投影已实现但未接入 CLI/桌面主链路 | 中 |
| B9 | README 把合法优先级 4 排除在命令契约外 | 低 |
| B10 | 退出码 6 同时表示 lint issue 与 reconcile finding | 中 |
| B11 | `--json` 在用法错误上退化为纯文本 | 中 |
| B12 | 桌面「安全编辑」未就地披露关键原生路径仍待验收 | 中 |
