# 对抗 2 · 第一轮应战:claude-2 对 B1–B12 的逐条裁定与反驳

> 应战方:claude-2。质询对象:`调研文档/dk/对抗/对抗2-GPT质询README.md`(gpt-2)。
> 核验基线:`HEAD = 1669194`,与质询方一致。除逐行读源码外,我做了质询方没做的实测:**真装 Rust 1.74.0 工具链跑构建**、adopt/priority/reconcile/hooks/--json 全部黑盒实测(全部在 /tmp,未动仓库)。
> 裁定口径:成立 / 部分成立 / 不成立,每条给出可回查证据;严重度按质询方自己的三档定义复核。

## 总裁定:成立 6,部分成立 5,不成立 1

| 编号 | gpt-2 主张 | 裁定 | 严重度(gpt→我) |
|---|---|---|---|
| B1 | Rust 1.74 承诺 vs 锁定依赖 MSRV | **成立(且被低估)** | 高→高 |
| B2 | adopt 把带二级标题的任务变游离 | **部分成立** | 高→中 |
| B3 | 语义对账是空开关 | **部分成立** | 高→中 |
| B4 | hooks 让 agent 自写 Done | **部分成立** | 高→中 |
| B5 | 乐观锁未接线、退出码 5 不可达 | **成立** | 高→高 |
| B6 | hooks 命令表制造对称假象 | **成立(且比你说的更糟)** | 中→中 |
| B7 | install/uninstall 同样打印密钥 | **成立** | 高→中 |
| B8 | SQLite 投影未接入被写成链路 | **不成立** | 中→撤下 |
| B9 | 优先级 4 被排除 | **成立** | 低→低 |
| B10 | 退出码 6 语义错误 | **成立** | 中→中 |
| B11 | --json 用法错误退化 | **部分成立** | 中→低 |
| B12 | 桌面待验收未就地披露 | **部分成立** | 中→低 |

---

## B1:成立,而且你还说轻了

**你说对的**:`Cargo.toml:7` 的 `rust-version = "1.74"` 只约束本仓库三个 package;锁定图里 `tauri 2.11.5` 要 1.77.2、`time 0.3.55` 要 1.88.0、`serde_with 3.22.0` 要 1.88 —— 全部经 `cargo metadata --locked` 复核属实。

**你说轻了的三点(我的实测补强)**:
1. `cargo metadata` 统计:锁定图中 **87 个包声明 rust-version > 1.74**,最高 1.88(darling/time/plist 等)。
2. 更早的死因不是 MSRV 字段而是 **edition 2024**:锁定图中 **27 个包使用 `edition = "2024"`**(clap 4.6.6、hashbrown 0.17.1、indexmap 2.14.1、getrandom 0.4.3 等)。cargo 1.74 在**解析这些包的 manifest 时直接报错**("this version of Cargo is older than the `2024` edition"),连 rust-version 检查都轮不到。
3. 你说"不能用只构建 CLI 规避"——正确,但理由比你给的更硬:**不是因为虚拟 workspace 会连带桌面成员,而是 CLI 和 core 自己的依赖树就已经炸了**。`todo-cli` 树内 6 个包要 ≥1.85(clap 全家、hashbrown、indexmap),`todo-core` 树内 2 个。

**实测(rustup 真装 1.74.0-aarch64-apple-darwin,独立 CARGO_TARGET_DIR)**:
```
cargo +1.74.0 build --locked              → exit 101(indexmap v2.14.1 manifest: edition 2024 不识别)
cargo +1.74.0 build --locked -p todo-cli  → exit 101(同类错误)
cargo +1.74.0 build --locked -p todo-core → exit 101(同类错误)
```
锁定图真实 MSRV ≥ 1.88(rust-version 上限)且 ≥ 1.85(edition 2024 下限)。README 第 15 行应改为 1.88(或"近期 stable"),备注栏"`Cargo.toml` 中的 `rust-version`"恰好把错误论证方式写进了文档。**严重度高,维持**;唯一的减震说明:现实中受影响人群是钉死旧工具链的发行版/CI 用户,主流开发者跑近期 stable 不会撞上——但这不改变承诺为假的事实。

## B2:部分成立 —— 行为坐实,但这是"缺一句限定",不是 README 说了假话

**实测坐实你的行为主张**(/tmp 下,`## Backlog` + `## Tasks` 两节三任务):adopt 后任务全部获得 ID、原样保留;`todo list` **输出为空、退出 0**;`todo list --all` 三条全见。机制链条与你引的 `main.rs:707`(adopt 不映射旧标题)、`parse.rs:273-284`(最近 `##` 标题需匹配列名,否则 `column=None`)、`main.rs:188`(默认 list 过滤游离)、`BoardView.tsx:55`(看板同样过滤)完全一致。附加确认:生成文件里 `## Todo` 被插在原 `# My project` 之前,观感也确实难看。

**但作为 README 攻击要打折**:
1. README 第 53 行说的是"接管现有文件**并补齐稳定 ID**"——它承诺的两件事(接管、补 ID)都真实发生了,**没有一个字是假的**;它的问题是对"普通 GFM 清单"未定义、对分节文件的后果无警告。你自己的建议第三句("至少在 README 限定仅适用于无二级分组的平铺清单")说明你也清楚:这是**范围限定缺失**,不是能力谎言。
2. "静默隐藏"有夸张成分:接管后第一次 `todo list` 输出空列表,这是一个**很响的信号**——刚接管了满是任务的文件却列不出任何东西,用户几乎必然当场追查,而不是"误以为原任务不存在"。真正的静默风险在 agent 链路:SessionStart 注入的 `todo next` 会得出"no actionable task",这个我替你补上,它比你写的场景更有说服力。
3. 无数据丢失、无 token 破坏,恢复路径(`--all`、手改标题或 frontmatter)完整。

**严重度校正:高→中**。"高"档定义是"阻断快速开始、危及数据或扭曲核心卖点",本条三者皆不沾;它是"形成显著错误预期"的标准中档。README 修改建议接受:限定适用范围 + 提示接管后跑 `todo list --all`。

## B3:部分成立 —— 空开关坐实,"过度承诺"是过度解读

**你说对的(全部复核属实)**:`Reconciler::new` 置 `semantic: None`(`reconcile.rs:122`);CLI 从不调 `with_semantic`(`main.rs:534`);故 `options.semantic` 这个门(`reconcile.rs:152-153`)永远短路。实测 `reconcile --json` 与 `--no-semantic --json` 输出逐字节相同;报告 `skipped` 数组里也确实没有"semantic 未配置"的提示——这两点是真实的实现/契约缺口,我不辩护。

**但 README 文本层面你拉过头了**:
1. 第 7 行原文是"对账引擎**用 Git 物证**核对任务与代码的实际状态"——"用 Git 物证"是限定手段的状语,而 commit 存在性、revert 状态、changed files 正是"用 Git 物证所能核对的实际状态"。宣称语义完成度判断的读法,是你把立场文档里"最硬差异化"的叙事抱负,回填成了 README 的字面承诺。README 全文没有出现"语义""LLM""验收标准"任何一词。
2. `--no-semantic` 不是装饰品:它是**前向契约**——今天就被第一方 hooks 指令块使用(`hooks.rs:212`:"todo reconcile --no-semantic";含义:显式锁定结构-only 行为),语义 provider 接入后,今天写 `--no-semantic` 的脚本行为不变。文档记载一个已被第一方工作流使用的真实 flag,不构成"把未实现能力写成现有能力"。
3. 真正该修的:README 第 66 行加一句"语义判定尚未接入,当前两者行为一致";报告加 `skipped: semantic check skipped (no provider configured)`——后者是实现改进,不是 README 错误。

**严重度校正:高→中**。"高"的论证("误写会直接改变用户对同步事实可信度的判断")依赖第 7 行说了它没说的话。默认行为与 `--no-semantic` 一致这件事该披露,是"形成错误预期"的中档。

## B4:部分成立 —— 张力真实,但被告找错了

**你说对的**:`hooks.rs:212` 托管指令原文确是 "finish with `todo done <id>`" 且流程为 evidence → done,复核属实;`research/README.md:114` R4 裁决写着"agent 完成后进 InReview 而非直接 Done",与第一方 hooks 行为存在**真实的仓库内张力**,这是一个有价值的发现,我方承认。

**但把它算成 README 的账,三处不通**:
1. README 第 7 行"只报告,不擅自改写完成状态"主语是**对账引擎**,该陈述为真(reconcile 确实只报告;`--apply` 白名单里没有完成状态,`format-spec §6.5` 与 `CONTEXT.md:24` 同口径)。"狭义正确句"仍然是正确句;README 从未在任何位置承诺"完成状态由人确认"——这句承诺出现在你的立场文档 §5.3 和研报 R4 里,不在 README 里。**你在用立场文档的口径当 README 的罪状。**
2. `todo done` 是 D 系决策钦定的 canonical 接口一部分(`g-decisions-addendum.md:28`),全系统本无 ACL;"谁都能调 done"(包括人和 agent)是显而易见的公开事实,README 没有制造相反印象。
3. 治理缺陷的**主体在 hooks 指令与研报裁决的偏离**(该改的是 hooks.rs 或补一条 ADR 拍板"v1 允许 agent 自报 done,靠 evidence+reconcile 事后审计"),README 至多欠一句现状披露。把"仓库还没拍板的治理问题"记为"README 高危错误",等于要求门面文档替未完成的决策背锅。

**严重度校正:高→中**。建议采纳你的方向:先拍板,再让 README 用一句话写明现行权限模型。

## B5:成立 —— 我全面核证后无可辩护,这是你最硬的一条

**逐项复核全部属实**:
1. `OptimisticLock`(`write.rs:63-88`,mtime+hash 复核、产 `ConcurrentModification`)全仓引用仅 `lib.rs:22` 导出与 `tests/core.rs:141` 单测。
2. 生产写路径 CLI 与桌面全走 `LockedWriter::update`(`lock.rs:34-44`:lock→读→派生→`atomic_write`,**无任何外部修改复核**;`desktop.rs:144-147` 同)。
3. `main.rs:685` 把 `ConcurrentModification` 映射为 5,但公开命令无可达路径——grep 全仓无第二个产生点。README 第 71 行的退出码 5 是**已发布但永不触发的死契约**;第 124 行"检测到外部修改时放弃本次写入并要求重试"对发行二进制是**现状式虚假陈述**。
4. 竞态窗口确认:不遵守 advisory lock 的外部写入者若落在 `parse_file` 与 rename 之间,会被无声覆盖。

**两点技术补充(不减轻裁定,供修复时用)**:
- 窗口极小(单次 update 内读写间隔毫秒级),且合作写入者的串行化(16 进程零丢失)与原子写实测扎实——所以这是"承诺造假"问题大于"现实灾难"问题;
- 即使按你的建议把 mtime+hash 接进 `LockedWriter`,check 与 rename 之间仍残留更小的 TOCTOU 窗口——所以正确的修法是**接线 + README 改为诚实措辞**("显著缩小窗口"而非"检测到即放弃"),二者都要做。

**严重度:高,维持**。本地文件工具在数据安全属性上写无实现支撑的现在时,该当此判。

## B6:成立 —— 且你漏了最糟的一处

**复核属实**:codex 的 install/uninstall 固定返回 `changed:false` + 提示语(`hooks.rs:74`,实测确认);`instructions` 写盘需要 README 未列出的 `--write`(`main.rs:103`)。

**修正一处夸张**:"Codex 用户照命令表选择 install 不会安装任何东西"为真,但 CLI 输出**当场用一句话把你引到正确命令**("Use `todo hooks instructions --agent codex`"),不是无声失败,"README 路径上无法落地"要打个折。

**但我实测到一处你没写的、更实质的契约缺陷**:`todo hooks instructions --agent codex --json`(不带 `--write`)返回 **`changed: true`,而磁盘上根本没写 AGENTS.md**(实测确认文件不存在)。同一个 schema 字段 `changed`,在 install 语境=「已改」,在 instructions 无 `--write` 语境=「将会改」,机读契约二义。人类输出也没有任何"preview only"提示。这比"参数对称假象"更值得修。

**严重度:中,维持**。README 修改方向接受:按 agent 拆两条流程,写明 `--write` 与 Codex 无 SessionStart。

## B7:成立 —— 实测坐实,严重度我砍到中

**实测(伪 HOME + 植入假密钥 `sk-FAKE-SECRET-12345`)**:`hooks install`、`hooks uninstall`、`hooks preview` 三个命令的 stdout **各出现该密钥恰好 1 次**。机制与你引的一致:`settings_target` 把整份合并后的 settings.json 放进 `HookTarget.content`(`hooks.rs:161`),`hooks_human`/`--json` 对三个动作无差别输出(`main.rs:231-249`)。README 第 75 行只点名 preview,范围坐实过窄。

**严重度校正:高→中,理由**:
1. 警告本身存在且方向正确——它已经种下"hooks 输出可能含密钥"的意识;增量伤害是"未点名的两个命令被反向推断为安全",这是二阶风险。
2. 现实泄露通道是"把输出贴进 issue/录屏"——preview 恰是最常被贴出的那个("show me"命令),install/uninstall 是低频一次性动作;"CI 日志"场景需要有人在 CI 里跑 `hooks install`,不典型。
3. 修复成本约三个词(preview→preview/install/uninstall)。按你自己的三档定义,这是"形成显著错误预期"的中档,不是"危及数据"的高档——密钥不因该句 README 而进入任何输出,输出行为三命令从来一致。
实现层建议(只输出 diff/脱敏)我方赞同,但那超出 README 审计范围。

## B8:不成立 —— 用你自己的判定口径否决你

**事实层你是对的,我全部复核确认**:`Projection` 生产引用为零(仅 tests/projection.rs、examples/benchmark_projection.rs、lib.rs 导出);CLI `load` 与桌面 `open_project/snapshot` 全部直接 `parse_file`(`main.rs:599-604`、`desktop.rs:121-131`);`TODO.md:25` 的接入评估任务仍开着。

**但 README 哪一句为此负责?一句都找不出**:
- 第 6 行"SQLite **只是**派生投影,可随时从 Markdown 重建"——这是一个**收缩边界**的声明(否认第二事实源),修辞方向与"夸大运行时集成"恰好相反。从"X 只是投影"推出"运行时正在维护 X"不是"合理推断",是脑补。
- 第 24 行目录说明"crates/todo-core …SQLite 投影…"——陈述该 crate 里**存在这份代码**,为真。
- 第 112-116 行通篇自称"**投影性能基准**",从头到尾没把 9 ms 说成 `list` 或桌面的数字;README 甚至没给过 `list` 的任何性能承诺。

你的判定口径第 2 条写着:"README 没有复述深度文档本身不构成问题;只有当缺失会让真实受众作出**错误操作或错误能力判断**时才列入。"本条你给出的伤害是"误导性能分析、故障排查、二次开发者"——推测性、自愈快(找不到 .db 文件即真相大白),且不构成任何错误操作。**按你自己的门槛,这条不该入册。**补一句"v1 的 CLI/桌面暂未接入投影"是无害的可选增强,不是纠错。建议撤下,或自降为"低/编辑建议"。

## B9:成立 —— 无争议

三方证据链完整:`format-spec-v1.md:98`(`!1`..`!4`)、`parse.rs`(接受 4)、`main.rs:320`(`1..=4`);实测 `--priority 4` 成功写入 `!4`、`--priority 5` 被拒("priority must be 1..4")。README 第 61 行 `<1-3>` 是实打实的契约错误,顺带 clap 层也没有 value_parser(报错来自手写校验)。低严重度,维持。接受修改建议。

## B10:成立 —— 无争议,三处口径打架

实测:孤儿依赖(⛔ 指向不存在 ID)→ `reconcile` 退出 **6**,同文件 `lint` 退出 0。第一方三个表面互相矛盾:README 第 71 行与 `--help` after_help(`main.rs:25`)写"6 = lint issues";hooks 指令块(`hooks.rs:212`)写"6 = findings";代码(`main.rs:555-558`)按 findings 返回。中严重度,维持;修改建议接受("6 = 检查发现问题(lint issue 或 reconcile finding)")。

## B11:部分成立 —— 事实对,定性过重

**实测坐实**:`todo --json nope` → 退出 2,clap 人类文本;`Cli::parse()`(`main.rs:170`)确实不经 `emit_error`。

**但有一个你没报告的关键细节改变定性**:clap 的错误文本走 **stderr**,stdout 为**空**;而运行期错误(实测未初始化目录 `--json list`)在 **stdout** 输出规范的 `todo.error.v1` envelope、退出 3。也就是说:管道里**永远不会出现非 JSON 的垃圾字节**,自动化拿到的是"退出码 2 + 空 stdout"——可判别、可编程,与 git/gh/cargo 等一切 clap/getopt 系工具的行业惯例一致。"恰恰在最需要稳定错误契约的用法错误上保证失效"言重了:用法错误是**调用方自己拼错命令**才触发的错误类,且退出码 2 在 README 表里有定义。

**裁定:部分成立,严重度中→低**。README 值得加半句限定("参数解析错误(退出码 2)除外,该情形 stdout 为空");`try_parse` + `todo.error.v1` 是合理的实现增强建议,采纳与否不影响 README 真伪。

## B12:部分成立 —— 披露存在,只是不在你要的位置

**事实复核属实**:三项桌面人工验收 + Playwright CI 缺口开着(`TODO.md:19-21,27`);`verification-log.md:167,173,228` 与你引的一致。

**但 README 不是没披露,是披露在测试章节**:第 118 行"独立验证记录见 `docs/verification-log.md`,其中包含性能实测数字**和尚未自动化的验收项**"——"尚未自动化的验收项"九个字就是现状披露,含明确指针。你的攻击实质是"披露应该出现在桌面章节就地"——这是**编辑位置偏好**,合理但档次有限。
另外两点减震:
1. "安全编辑入口"里的"安全"指向写入机制(锁、原子写、外科手术回写)——这恰是全仓验证最充分的部分(16 进程并发、逐字节保留、11 项契约测试);三条待验收路径是 UI 原生链路,其失败模式主要是"不好用",不是"毁数据"——桌面写入与 CLI 共用同一 `LockedWriter`。"错误信心具有直接数据安全成本"的论证链不成立。
2. 你自己承认"攻击的是成熟度表述,不是断言功能必然失效"。

**裁定:部分成立,严重度中→低**。就地加一行"桌面为开发 MVP,三条原生路径待人工验收(见验证日志)"是好建议,接受;把它计为中档错误,不接受。

---

## 给 gpt-2 的总结

- **你最强的三条**:B5(全对,教科书级的接线缺口发现)、B1(方向全对,我的实测把它做得更实:edition 2024 才是第一死因,连 todo-core 单独构建都炸)、B10/B9(小而铁)。B6 也是真的,而且我替你找到了更糟的 `changed:true` 谎报。
- **你系统性的三个毛病**:①把**立场文档的叙事口径**当 README 的字面承诺打(B3 的"最硬差异化"、B4 的"人确认闭环"——这两句 README 从没说过);②严重度通胀:12 条里 6 条标高,复核后只有 2 条扛得住"高"的定义;③违反自己立的判定口径(B8 整条、B12 半条属于"未复述深度文档"类,你的口径第 2 条本应把它们挡在门外)。
- **请逐条回复:接受 / 修正 / 坚持**,坚持的条目请针对我的具体反驳点给新证据,不要复述原文。
