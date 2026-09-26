# 验证日志(captain 独立验证记录)

> 分工:codex-engineer(openai/gpt-5.6-sol)实现,captain(claude-opus-5)独立验证。
> 原则:**不采信实现方的自测结果** —— 每项都用实现方未见过的样本/场景复验;验证方自身的错误也如实记录。

## T1 — 解析 + 保真回写(通过)

| 验证项 | 方法 | 结果 |
|---|---|---|
| 测试复现 | 独立 `cargo test` | 8 passed |
| 无 AST 重序列化 | 逐行审 `write.rs` | ✅ 确认是字节区间 `replace_range`,无序列化路径 |
| 保真性 | **captain 自造恶意样本**(BOM+CRLF / LF,含代码围栏假任务、行内代码假任务、表格、图片、mermaid、制表符缩进、变体选择符、未知 frontmatter 键) | ✅ 空变更逐字节相同;切换状态只改 1 行且总字节不变;6 个真任务全识别、假任务零混入 |
| 属性测试是否摆设 | 读测试源码 | ✅ 多重集不变式断言属实,覆盖 CRLF/LF 与脏内容;toggle 对合断言逐字节还原 |

**超出要求的设计**:`validate_changed_lines` 逐段比对 edit 之间的保留区必须逐字节相同,能在写盘前抓住「区间被撑大」;写盘前重新 parse 输出,避免写出自己都解析不了的文件。

**因本轮暴露而修订的 spec**(根因在 spec,非实现):
1. §1.3 非标准 checkbox:pulldown 不把 `[-]`/`[/]` 当 task item,实测 5 任务文件只解析出 3 个,**两行完全不可见**。改为:必须记录进 `unrecognized_task_like_lines` 并由 lint 警告。理由:**静默不可见比不支持更危险**。
2. §1.2 列归属:原写「非列标题下的任务归属最近上游列」,实测导致用户章节若位于「已完成」列之后,其下未完成任务被判为 `done` 列 —— **列归属与 checkbox 自相矛盾**。改为:一律 `column: null`。

## T2 — SQLite 投影(通过)

首次尝试因 4 个 rusqlite 生命周期错误(E0700 + E0597×3)编译失败中断;captain 提供精确诊断与修法后恢复,未推倒重来。

| 验证项 | 方法 | 结果 |
|---|---|---|
| 测试复现 | `cargo test` + `clippy -D warnings` | 13 passed / 干净 |
| **不成为第二事实源** | 审查全部 pub API | ✅ 所有函数要么吃 `&Document`、要么往外读;**无任何反向写文件路径** |
| 增量 == 全量 | **captain 自造场景**:跨两文件的改名+完成翻转+新增+删除 | ✅ 两条路径快照完全相同 |
| 增量是真增量 | 检查未受影响文件的行 | ✅ 更新 a.md 时 b.md 行未被动 |
| 幂等性 | 重复全量重建 | ✅ 快照一致 |
| 性能 | captain 独立复现 benchmark | ✅ median **8.99ms** / p95 9.54ms(预算 100ms,~11 倍余量) |
| spec 修订是否真修 | 读 `parser_revisions.rs` 断言 | ✅ 两项均真实断言,且正确区分「真非标准 checkbox」与「代码块内的假货」 |

## T4 — 子树移动 + advisory lock(通过)

| 验证项 | 方法 | 结果 |
|---|---|---|
| 测试复现 | `cargo test` | 17 passed |
| **真·多进程并发** | captain 编译独立二进制,**16 个进程同时写同一文件**(实现方只测了 12 线程,语义不等价) | ✅ 16 进程全成功;文件恰好 17 任务(1 seed+16);**零丢失更新**;无重复/损坏 ID;写后仍可解析 |
| 锁超时 | 真进程持锁下写入 | ✅ 2.005s 后返回明确错误(点名锁路径与超时值),**未永久挂死** |
| 崩溃恢复 | `kill -9` 持锁进程 | ✅ 下一个写入者 **12ms** 获锁(OS 自动释放 flock);残留 0 字节锁文件无害 |

**关键设计**:`update()` 把 read 放在锁内,使排队写入者不会基于陈旧快照操作 —— advisory lock(防并发踩踏)与乐观锁(防陈旧快照)**正确组合而非冗余**。

## T3 — CLI 骨架(通过,带 3 项已知缺口)

| 验证项 | 方法 | 结果 |
|---|---|---|
| 测试复现 | `cargo test` | 21 passed |
| 基本流程 | 实跑 init/add/list/done/lint | ✅ 正常 |
| JSON envelope | 消费 `--json` 输出 | ✅ `schema`/`ok`/`data` 三段式,schema 串带版本(如 `todo.list.v1`) |
| **exit code 守约** | 逐个制造对应错误 | ✅ 0 成功 / 1 一般错误 / 2 用法 / 3 未初始化 / 4 锁超时(真进程持锁下测)/ 6 lint 发现问题 —— **全部正确** |
| lint 有效性 | 注入重复 ID | ✅ 正确检出 |
| init 数据安全 | 重复 init、16 并发 init、init+add 混合并发 | ✅ 重复 init 拒绝且不覆盖(重要任务存活);16 并发产出唯一合法文件;混合并发文件合法且 lint 通过 |
| 性能 | 1000 任务实测 | `list` ~31ms、`add` ~38ms(**含进程启动开销**) |

### 已知缺口(裁决:v1 接受,记录待办)

1. **`list` 未走 SQLite 投影**(实现方主动申报)。裁决:**v1 接受**。依据:实测 1000 任务 `list` 约 31ms,而 T2 全量投影重建仅 9ms —— 说明**解析不是瓶颈,进程启动才是**,接 SQLite 不会显著改善单次 CLI 调用。**触发条件**:当任务量达到 ~10000,或出现常驻进程(GUI / `todo serve`)时必须接入,那时投影的价值才真正兑现。
2. **`init/adopt` 未走 LockedWriter**(主动申报,存在 TOCTOU 窗口)。裁决:**v1 接受**。依据:实测三种并发场景均未造成数据损失或文件损坏(合法 v1 文件尚不存在时无法用现有 API 加锁,这是鸡生蛋问题)。**待办**:改用 `create_new` 原子创建消除窗口。
3. **ID 为「时间+PID」base36,非真随机**(主动申报)。裁决:**v1 接受,但必须在多分支场景前修**。依据:24 进程并发实测无碰撞;但实测确认 ID **单调可预测**(`5ddh`→`5e89`→`5e8c`→`5e8f`),与 spec §2「随机 base36」的初衷不符 —— 该设计的目的不只是防碰撞,更是防**跨分支合并时的相邻/易混淆 ID**(beads 用 hash ID 正是为此)。**待办**:改为随机源。

### 其他缺陷(captain 发现)

- **错误信息张冠李戴**:重复 `init` 时报「operation would create an invalid task line: TODO.md already exists」—— 复用了错误的 Error variant,前缀误导。exit code 与 `--json` envelope 均正确,仅文案问题。

## 验证方自身的错误(如实记录)

1. 首次写验证程序时误用 `todo_core::projection::Projection`,实际类型已在 crate 根导出 —— **captain 未先读接口**,非实现缺陷。
2. 猜测 `LockedWriter` 方法名为 `write`,实际是 `update` —— 同上。
3. 崩溃恢复首测用了 macOS 不存在的 `flock`/`timeout` 命令,持锁脚本根本没拿到锁,测试无效 —— 已用 Rust 重写后重测。
4. 误判重复 `init` 返回 exit 0,实为 `echo $?` 捕获了管道中 `head` 的退出码 —— 实际是 1(正确)。**险些据此误报产品缺陷。**
5. t7 验收时 JSON 解析报 KeyError,一度以为是回归;实为 §6.5.6 落地后 schema 新增 `data.report` 层级,**我的脚本用了旧结构**。
6. 首次 shallow clone 测试的克隆源提交里根本没有 TODO.md(创建时序错误),引擎正确返回 exit 3,被我误读为异常 —— 重新构造后才是有效测试。

---

## T5 — 随机 ID(通过)

| 验证项 | 方法 | 结果 |
|---|---|---|
| 真随机而非仅无碰撞 | 生成 200 个 ID 做**单调性统计** | ✅ 相邻递增对 **103/199 ≈ 51.8%**(真随机应 ≈50%,旧时间戳方案 ≈100%)。首字符覆盖 36/36,前 2 位 189/200 种 |
| 碰撞加长 | 读测试源码 | ✅ 用**注入式假随机源**强制制造碰撞并断言加长到 7 位,非靠撞运气 |
| 错误文案 | 重复 init | ✅ 已改为专门的 `FileAlreadyExists`,文案说人话 |

## T6 / T7 — 对账引擎(通过,经一轮打回)

### 首轮(t6)通过项
干净仓库零输出、非 git 优雅降级(含 `skipped` 说明)、物证三态(`evidence-reverted` 与 `evidence-missing` 区分)、状态只升不降、语义层只吃收窄候选集(`SemanticCandidate` 不含仓库全量内容,mock 用 `received()` 使收窄可测)。

### 首轮发现的真缺陷
**shallow clone 未识别 → 大规模误报**。实测 `git clone --depth=1` 后 `is-shallow-repository` 为 `true`,但引擎报 4 条 suspected、`skipped` 为空,把截断历史外的物证全判为失效。**CI 默认即 `--depth=1`,首次运行就会满屏误报。** 注:实现方 t6 output 自称已处理 shallow,实测证明未兜住 —— 这正是独立验证的价值。

### 打回原因(严重)
`--apply` **删除了 `🔗` 物证与 `⛔` 依赖 token**。一个「工作被 revert」的任务在对账后退化为看不出任何问题的普通未完成任务,**下次对账再也发现不了**,漂移被永久掩埋。等于发现火灾后拆掉火警。

根因一半在 captain:该裁决当时只存在于聊天消息,且消息晚于实现方完工。**已将规则写入 spec §6.5「对账引擎的不可协商约束」**,不再依赖对话传递。

### 修补后复验(全部通过)
| 验收项 | 结果 |
|---|---|
| `--apply` 后 `🔗`/`⛔`/`→` 是否保留 | ✅ 三类全部原样保留,无可自动修项时文件**逐字节未变** |
| §6.5.6 未处理项提示 | ✅ 输出 `applied 0, skipped 3 (manual judgment required)` |
| 干净仓库零输出(回归) | ✅ 保持 |
| shallow clone 降级 | ✅ 完整克隆 0 findings 无跳过;shallow 克隆 0 物证 findings + 明确 skip 原因。**两者对比证明是真识别 shallow,而非碰巧没检查** |
| 重复 ID 分层 | ✅ 无引用者自动修、有引用者只报不修(`applied 1, skipped 1`) |

### 由实现方发现、captain 采纳的 spec 缺陷
实现方指出 §6.5.2 与 §6.5.3 自相矛盾:前者称重复 ID 属 confirmed,后者称有外部引用时降级 suspected。
**裁决:错在 §6.5.3,已更正。** 依据 §6.5.2 判据 —— `confirmed` 表示**事实确定**,`suspected` 表示完成判定或**事实存疑**。重复 ID 是确定事实,不确定的只是修法,而修法不唯一已由 `suggested_action == null` 表达,不应再用层级重复表达一次。实际后果亦支持:重复 ID 会静默破坏一切基于 ID 的操作,埋进 suspected 更危险。

## T10 — Tauri 后端骨架与 DesktopBackend(通过)

原实现成员长时间无文件或构建活动后被移除。`desktop-sol-engineer` 使用同一路由 `openai/gpt-5.6-sol`、推理档位 `xhigh` 接管现有磁盘成果；未重建或丢弃既有文件。

| 验证项 | 方法 | 结果 |
|---|---|---|
| Rust 回归 | captain 独立运行 `cargo test --workspace` | ✅ workspace 全部测试通过；Desktop 专项 12 项通过 |
| 严格静态检查 | `cargo clippy --workspace --all-targets -- -D warnings` | ✅ 通过 |
| 稳定 ID 移动 | 审查接口并复跑「旧 Snapshot 后外部插入」用例 | ✅ 前端只提交 `MoveTarget` 稳定 ID；索引在 `LockedWriter::update` 的新鲜 `Document` 内解析 |
| Today 边界 | 注入 `FakeClock` | ✅ 使用本地日期；`upcoming` 仅包含严格未来 7 天；第 8 天被排除 |
| Capture 数据安全 | marker-like Unicode、多行、截断、错误长度、首次并发 append | ✅ length-prefixed 格式可无损往返任意 UTF-8；损坏文件被拒绝；首次创建无丢失更新 |
| Triage 部分成功 | 人为破坏 Capture 删除阶段 | ✅ 返回已创建的 `task_id`，不谎报完整成功 |
| 文件 watcher | 连续 5 轮真实测试；每轮两次 temp-file + rename | ✅ 监听父目录后两次替换均收到最新内容；`notes.md`、锁文件与临时文件噪音不触发 |
| 1000 任务性能 | captain 独立 `--nocapture` 复测 | ✅ **16.42 ms**，低于 100 ms 预算 |
| Tauri 编译 | 项目本地 CLI：`tauri build --debug --no-bundle` | ✅ 生成 `target/debug/todo-desktop`；图标为合法 128×128 RGBA PNG |

后端命令包含 `open_project`、`snapshot`、`mutate`、`capture_list`、`capture_append`、`capture_delete`、`capture_triage` 与 `desktop_status`。Tauri host 直接调用 `todo-core`，未启动 CLI 子进程。

## T11 — 看板、今日视图与随记 UI(通过浏览器验收，Tauri 真机见 T12)

| 验证项 | 方法 | 结果 |
|---|---|---|
| 前端门禁 | captain 独立运行 `pnpm typecheck && pnpm build` | ✅ 最终 typed bridge 收紧后复跑通过；生产 JS gzip 约 75.58 kB，CSS gzip 约 9.82 kB |
| 离线字体 | 审查源码与构建资产 | ✅ Bricolage Grotesque 与 JetBrains Mono 均打包为本地 `.woff2`；源码无 Google Fonts 或其他外部字体 URL |
| 看板主路径 | Playwright browser mock | ✅ 新增任务；键盘「移动到列」菜单；authoritative Snapshot 返回后落位；横向列滚动 |
| 今日视图 | Playwright 切换并检查分组 | ✅ 逾期、今天、未来 7 天三组均可访问 |
| 随记与 Triage | 新增包含 `<!--/capture-->` 的 marker-like 文本，再分拣 | ✅ Capture 出现后可分拣；分拣后从 ledger 移除并进入 Project Task mock |
| 全局速记窗口 | 520×180 viewport；`Ctrl+Enter` | ✅ textarea 自动聚焦；保存后清空；成功状态仅出现一次 |
| 响应式 | 1180×780 与 720×780 截图和可访问性树 | ✅ 主窗口与窄窗口均可使用；720 宽保留横向看板与紧凑导航 |
| 控制台与网络 | Playwright console/requests | ✅ 无应用错误；仅加载本地静态资产 |
| 真实 Tauri bridge | 对照 `src-tauri/src/lib.rs` 逐项审查 | ✅ `MoveTarget`、`desktop_status`、Capture authoritative list、Triage `CommandValue` 已按真实返回结构解包；启动主动尝试 `snapshot`，不依赖早期事件 |

视觉采用工程笔记/编辑台方向：暖纸网格、墨色侧栏、cobalt/朱红/苔绿状态色。没有引入 UI kit 或全局状态框架。

## T12 — macOS 真机、性能与打包(通过，保留一项人工验收)

| 验证项 | 方法 | 结果 |
|---|---|---|
| 最终 Rust 门禁 | captain 独立运行 `cargo test --workspace` 与严格 Clippy | ✅ 全部通过；Desktop 专项增至 13 项 |
| 并发 Triage | 两个线程、两个 `DesktopBackend` 同时分拣同一 Capture | ✅ Capture lock 覆盖读取、创建任务与删除；仅 1 次成功、项目中仅 1 个同正文任务 |
| 测试数据隔离 | debug-only `TODO_DESKTOP_TEST_CAPTURE_PATH` | ✅ 真机测试可使用 `/tmp` inbox；release 仍固定使用 `~/.todo/inbox.md`；验收结束后真实 `~/.todo` 仍不存在 |
| 主窗口可见 | CoreGraphics `optionOnScreenOnly`，确认 layer 0 的 1180×780 窗口 | ✅ standalone 连续 5 次为 **330.5–370.0 ms**；低于 2 s 预算 |
| setup 指标 | 原生 `desktop_status` debug log | ✅ 本轮实测 **274 ms / 334 ms**，`shortcut_status=registered` |
| 全局快捷键 | 系统级 `CGEvent` 发送 `Cmd+Shift+Space`，再用 CoreGraphics 查窗；人工物理操作复核 | ✅ 出现 520×180、layer 0 的预创建窗口；handler dispatch log 为 **2–4 ms**；正式 `.app` 能获得前台焦点 |
| Escape | 增加 window-level `keydown`，不再依赖 textarea 焦点；人工操作正式 `.app` | ✅ Esc 能正常隐藏速记窗口；按钮焦点也走同一 handler |
| `.app` bundle | `tauri build --debug --bundles app` | ✅ 生成 `target/debug/bundle/macos/Todo.app`，36 MB，arm64 |
| bundle 签名 | 本地 ad-hoc 深签名后 `codesign --verify --deep --strict` | ✅ 通过；这是开发签名，不是 Developer ID 签名，也未 notarize |
| bundle 启动 | 直接启动 bundle 内二进制并等待 layer 0 主窗 | ✅ 首次 **1204.9 ms**，签名后复核 **590.5 ms**，均低于 2 s |
| 进程清理 | 检查 `todo-desktop` 与 TCP 1420 | ✅ 无残留原生进程或 Vite listener |

收尾时收到一组晚于首个 bundle 的 bridge 类型收紧：Rust `DesktopStatus` 与 UI 派生 `WorkspaceStatus` 被分离，Triage 改为强制非空 `task_id` 的 tagged union，全局 `Cmd/Ctrl+Enter` 也移到 window handler。captain 发现同一批晚到文件还留下了引用缺失 `native_test.js` 的 debug harness，导致 bundle 编译失败；该 harness 从未接入 setup，已完整移除。随后重新通过前端门禁、桌面 crate 测试、严格 Clippy、`.app` 构建、ad-hoc 深签名和 LaunchServices 双窗口验收。

### 未自动化的真机路径

当前 macOS 会话拒绝向 System Events 发送 Apple Events，错误为 `-1743`。因此，没有把「点击原生文件选择器、在 WKWebView textarea 输入文字、观察界面刷新」写成自动化通过项。用户已在正式 `.app` 上人工确认速记窗口获得前台焦点且 Esc 可正常隐藏。其余逻辑由三组独立证据覆盖：

1. browser mock 的完整交互路径；
2. Rust 的 authoritative mutation、Capture、watcher 与并发测试；
3. TypeScript bridge 与 Rust command 的参数和返回结构逐项对照。

正式对外发布前仍应由具备 Accessibility 权限的人工作一次原生文件选择、任务写回、Quick Capture 文本保存与外部编辑刷新。此限制不影响当前开发 `.app` 的可运行性，也不改变已测得的启动与快捷键窗口性能。

## T3 缺口 2 修复 — init/adopt 原子创建(通过)

T3 验收时记录的「`init/adopt` 未走 `LockedWriter`，存在 TOCTOU 窗口」已修复。

| 验证项 | 方法 | 结果 |
|---|---|---|
| 缺陷可复现 | 并发 `init`，统计同时成功的调用者 | ✅ 修复前 **5/40 轮**出现多个成功者，最多 2 个同时成功 |
| 真实数据损失 | 预置两条用户任务后并发 `init --adopt` | ✅ 修复前 **2/40 轮**用户任务被覆盖丢失 |
| 修复后 init | 120 轮 × 32 进程，共 3,840 次并发创建 | ✅ **0** 轮多成功者；`max-simultaneous-winners=1` |
| 修复后 adopt | 120 轮 × 6 进程并发接管 | ✅ **0** 轮丢失用户任务 |
| 回归测试有效性 | 把新测试跑在**旧代码**上 | ✅ 确实失败(4 个成功者而非 1 个)，证明不是永远为绿的空测试 |
| 行为未回归 | 重复 `init`、已有 frontmatter 的 `adopt` | ✅ 均退出码 1；重复 `init` 后文件逐字节未变 |
| 门禁 | `cargo test -p todo-cli`、`clippy -D warnings`、`cargo fmt --check` | ✅ 16 项测试通过，静态检查与格式干净 |

实现要点：`init` 用 `create_new` 把创建窗口交给内核裁决；`adopt` 因为是重写既有文件，改为先取 advisory lock 再读，写入走 `atomic_write`。回归测试使用真实多进程，因为该竞态只存在于进程之间，进程内测试会给出虚假信心。

验证方自身的错误：首个探针让 24 个进程并发 `echo >>` 同一文件统计成功数，实测会丢行，导致高并发下反而「测不出问题」。改为每个胜出者写独立标记文件后结果才可信。**这个假阴性差点让我判定竞态不存在。**

## 桌面端命令层 e2e 契约测试(通过)

此前桌面端没有任何自动化测试，Rust 与 TypeScript 之间的契约只靠人工逐项审查。而已发现的桌面缺陷**全部是 shape mismatch，没有一个是逻辑错误**，因此测试断言序列化后的 JSON，而不是 Rust 值——断言 Rust 结构体一个历史缺陷都抓不到。

测试位于 `apps/desktop/src-tauri/tests/command_contract.rs`，共 11 项，覆盖 snapshot、mutation、Capture、Triage 与 status 全部命令，并直接使用 `apps/desktop/src/lib/desktop.ts` 中的真实 payload 字面量。

为避免出现「测试路径」与「生产路径」两份实现，命令体已抽到 `ops::DesktopOps`，`#[tauri::command]` 退化为薄适配层。测试驱动的是生产同一份代码。

### 反证：注入缺陷确认测试会变红

绿色测试本身不构成证据。以下每类缺陷都单独注入、确认变红后还原：

| 注入的缺陷 | 结果 |
|---|---|
| `MoveTarget`/`DesktopIntent` 的 serde tag 由 `type` 改为 `kind` | ✅ 变红 |
| `snapshot.project_path` 改为 camelCase | ✅ 变红(2 项) |
| `TriageResult` 的 `task_id` 改名 | ✅ 变红 |
| `CommandValue` 去掉 `status` tag 改为 untagged | ✅ 变红(2 项) |
| `ShortcutStatus` 不再是 tagged union | ✅ 变红 |

这五类正是此前实际发生过的 bridge mismatch。

### 覆盖的产品红线

除 shape 外，测试还锁住三条行为红线：完成状态写回后未触碰行逐字节保留且行数不变；marker-like 与多行 Capture 无损往返；截断的 inbox 必须报错而不是静默丢记录。

| 门禁 | 结果 |
|---|---|
| `cargo test --workspace` | ✅ 70 项通过 |
| `cargo clippy --workspace --all-targets -- -D warnings` | ✅ 干净 |
| `cargo fmt --all -- --check` | ✅ 干净 |
| `pnpm typecheck && pnpm build` | ✅ 通过 |

未重建 `.app` bundle，人工验收对象保持不变。

遗留：前端交互层(Playwright)自动化尚未纳入 CI，Tauri IPC 的实际序列化由本测试的 `wire()` 以同一 serde 路径模拟，而非经过真实 webview。

## MSRV 下限实测(对抗 3 C3)

对抗 3 C3 判定:README 的 Rust 版本承诺不能只抄 `Cargo.toml` 的 `rust-version`,必须由锁定依赖图证明,否则严格照做的新用户会在第一条构建命令上失败。此处记录双向实测,使该字段不再是未经验证的愿望值。

| 验证项 | 方法 | 结果 |
|---|---|---|
| 锁定图真实 MSRV | `cargo metadata --locked` 汇总各 package 的 `rust_version` 取最大值 | 1.88.0(`globset 0.4.20`、`icu_* 2.3.x`、`time 0.3.55`、`plist 1.10.0`、`darling 0.23.0`) |
| 声明下限可构建 | `rustup run 1.88.0 cargo build --locked -p todo-cli` | ✅ 通过 |
| 低一版必失败 | `rustup run 1.87.0 cargo build --locked -p todo-cli` | ✅ 如期失败(exit 101,`rustc 1.87.0 is not supported by the following packages`) |

反向用例是这组验证的关键:它证明 1.88 是**精确下限**而非保守高报 —— 低于它会在依赖解析阶段直接失败,失败早于任何编译。

CI 不硬编码版本号,而是由 `.github/workflows/ci.yml` 与 `release.yml` 用 `awk` 从 `Cargo.toml` 的 `rust-version` 读出并安装同一工具链,因此 README、manifest、CI 三者不会再各自漂移。日后抬升依赖导致 MSRV 变化时,只需改 `Cargo.toml` 一处,CI 自动跟随;但**必须重跑上表的双向实测并更新此节**,否则 C3 会以新版本号原样复发。

## lint 重复 ID 复验(对抗 3 C14)

对抗 3 C14 判定第 54 行「lint 有效性｜注入重复 ID｜正确检出」已与源码分叉,因为当时的 `lint` 不调用 `duplicate_ids()`。本轮重新核对:该结论**当前成立**,不是失真记录 —— `crates/todo-cli/src/main.rs` 的 `lint()` 已经遍历 `document.duplicate_ids()` 并产出 `duplicate-id` 问题。故保留第 54 行原状。

| 验证项 | 方法 | 结果 |
|---|---|---|
| 重复 ID 检出 | captain 自造含重复 `t:aaa111` 与 `[/]` 非标准 checkbox 的文件,实跑 `todo lint` | ✅ 同时报 `duplicate-id`(error)与 `nonstandard-checkbox`(warning),exit 6 |
| JSON 契约 | 消费 `todo lint --json` | ✅ `severity`/`code`/`line`/`message` 齐备;重复 ID 定位到**第二次出现**的行 |
| 回归锁定 | 读 `crates/todo-cli/tests/cli.rs:313` `lint_reports_duplicate_stable_ids` | ✅ 断言 exit 6、`severity=error`、精确行号 9、message 含 ID,非占位测试 |

仍需注意 C14 的另一半:lint 只覆盖**纯文档结构**(解析错误、重复 ID、非标准 checkbox)。依赖、死引用与 Git 物证问题由 `reconcile` 检出,跑 lint 绿色不等于这些层也干净。README 命令表已据此写明范围与分工,不再使用「检查格式问题」这一泛称。

## 对抗 3 全条目复核与 C7/C1/C15 修复

对 16 条逐条重新取证(每条独立核查 + captain 抽验 C7/C16/C6/C8)。结论:**C2、C3、C4、C10、C14、C16 已失效**(后续提交已修);**C1、C5、C12、C15 半修**;**C6、C7、C8、C9、C11、C13 仍成立**。本轮修掉其中三条无需产品决策的。

### C7 — hooks 输出默认不再打印配置正文

修复前实测:造含假密钥的 `settings.json` 后,`preview` / `install` / `uninstall` 三条命令**全部**原样打印密钥(各命中 1 次),而 README 只警告 `preview`。这个限定措辞比不警告更糟——它暗示写入类命令的输出是安全的。

| 验证项 | 方法 | 结果 |
|---|---|---|
| 默认不泄漏 | 三条命令 × 人类/`--json` 两种输出 | ✅ 密钥命中 0;仍打印路径、`changed`、备份位置 |
| opt-in 仍可见 | `--show-content` | ✅ 完整正文照常输出 |
| 回归锁定 | `hook_commands_never_print_config_secrets_without_show_content`、`show_content_still_prints_the_full_config_when_asked` | ✅ 通过 |

`--json` 的 `content` 字段同样替换为占位符 —— 只改人类输出会留下等价的泄漏面。原有三个断言正文的测试改为显式 `--show-content`,保留其覆盖而非削弱。

### C1 — 已配置 provider 却静默跳过语义层

修复前实测:`TODO_LLM_PROVIDER` 已配置时,不带 `--semantic` 的 `todo reconcile` 返回 **exit 0 且 `checks_skipped` 为空**,与 C2 刚建立的「7 才表示没查完」契约直接冲突。

关键分寸在于**区分三种情况**,而不是一律报跳过:

| 场景 | 期望 | 实测 |
|---|---|---|
| 配了 provider,未带 `--semantic` | 7 + 具名原因 | ✅ `semantic check skipped: a provider is configured but --semantic was not passed` |
| 显式 `--no-semantic` | 0 | ✅ 0 |
| 未配 provider,默认运行 | 0 | ✅ 0 |

第一版改法把「只要没跑语义层就报 skipped」写进 core,结果 `reconcile_defaults_to_structural_only_in_a_complete_git_repository` 立刻变红。该测试是对的:`hooks.rs:235` 注入给 agent 的指令要求收工前跑 `todo reconcile --no-semantic`,若那条路径恒返回 7,退出码 7 会退化成永久噪声,反而摧毁 C2 建立的信号。故改为只在 CLI 层针对「配了却没用」这一真实危险情形报跳过。回归测试 `configured_semantic_provider_that_never_runs_is_reported_as_incomplete` 三种场景全覆盖。

### C15 — `--help` 退出码文案

`main.rs` 的 `after_help` 仍写「6 lint issues」且完全没有 7,而 `hooks.rs:235` 给 agent 的口径已是「6 findings, 7 checks incomplete」——同一仓库内自相矛盾。已统一为「6 findings (lint issues or reconcile findings); 7 checks incomplete (a check could not run — unverified, not clean)」。7 的缺失比 6 的措辞更危险:`--help` 读者根本看不到 7 的存在,容易把它当未知错误或误判为通过。

### 门禁

| 门禁 | 结果 |
|---|---|
| `cargo test --workspace` | ✅ 全绿,0 failed(新增 3 项回归测试) |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | ✅ 干净 |
| `cargo fmt --all -- --check` | ✅ 干净 |
| `pnpm typecheck` | ✅ 通过 |

未修的高危项与其阻塞原因:**C6**(hook 裸 `todo` + `|| true` 吞错,需改 `current_exe` 并加 `hooks doctor`)、**C8**(仍无 LICENSE,`DECISIONS.md:58-62` 四个选项未拍板,属产品决策)、**C5**(adopt 遇普通 frontmatter 报错、既有标题致真任务变 `column:null` 从默认视图消失)、**C9/C11/C13**(README 措辞层,可合并处理)。

## C6 / C5 / C9+C11+C13 修复(三 agent 并行)

由三个 openai/gpt-5.6-sol 成员并行处理,captain 对每项**独立复验**(实跑,不采信成员的自测报告)。

### C6 — hook 不再吞错,且不再依赖 PATH

两层缺陷。成员先修了「吞错」一层:去掉 `|| true`(保留 `2>/dev/null` 以免 SessionStart 输出 shell 噪音,但非零退出码得以传播),并新增 `todo hooks doctor`。captain 复验时发现**主因未修** —— 落盘的 hook 仍是裸 `todo`,`current_exe` 全仓零引用,`run_capture` 仍是 `Command::new("todo")`。

关键区分:去掉 `|| true` 只是把失败从**静默地永远失败**变成**响亮地永远失败**,并未移除导致失败的 PATH 依赖。成员用受限 PATH 下 exit 127 作为修复证据,但那恰恰是**残留 bug 的复现**,不是修复的证明。据此开 t4 要求补齐。

| 验证项 | 方法 | 结果 |
|---|---|---|
| 落盘命令含绝对路径 | 装入临时 HOME 后读 `settings.json` | ✅ `TODO_HOOK_ID=... '/Users/.../todo' hooks context --agent claude 2>/dev/null` |
| 受限 PATH 下真能注入 | 真实项目内 `env -i PATH=/usr/bin:/bin sh -c "$CMD"` | ✅ exit 0,输出 `TODO next: [m38737] visible task` |
| 旧 hook 不被孤儿化 | 预置裸 `todo` 旧 hook 后重装 | ✅ SessionStart 条目数 1(替换而非重复);卸载后残留 0 |
| `run_capture` | 读 `hooks.rs` | ✅ 改用解析出的 executable |

复验中一个自身失误值得记录:首次受限 PATH 实跑得到 exit 0 且**零输出**,形似静默失败;实为当时目录没有 `TODO.md`,空输出正确。改到真实项目内重跑才得到正确结论 —— 若采信首次结果,会误报一个不存在的 bug。

**测试覆盖的一处倒退**:doctor 集成测试被改为新的健康契约(`available=true`、exit 0)是对的,但这样一来 exit 1 分支**完全失去覆盖**。该分支只有 `current_exe()` 失败才可达,集成测试无法触发,故 captain 在 `hook_command` 接缝处补了单元测试:未解析出可执行文件时回退裸 `todo`、含空格路径的 shell 引用、以及**路径内含单引号**的转义(否则引号提前闭合,路径会裂成多个 shell 词)。captain 自己最初写的一条断言也是错的(断言回退命令不含 `/`,但 `2>/dev/null` 就含 `/`),已改为剥掉 marker 前缀后再判断。

### C5 — adopt 不再让真任务消失

| 验证项 | 方法 | 结果 |
|---|---|---|
| 普通 frontmatter 可接管 | 含 `title:`/`author:` 的文件跑 `init --adopt` | ✅ exit 0(此前 exit 1 报 "already has frontmatter") |
| 真任务保持可见 | 自定义 `## Backlog`/`## In Progress` 下各一条真任务,跑 `todo list` | ✅ 两条都在默认视图(此前默认列表为空,仅 `--all` 可见) |
| frontmatter 无损合并 | 读接管后文件 | ✅ 注入 todo-spec/columns,用户 `title`/`author` 逐字保留 |
| 围栏假任务不被插 ID | ```markdown 围栏内假任务 | ✅ 全文恰好 2 个 `<!--t:`(仅两条真任务) |

列映射规则:既有 H2 标题保守降级到生成的 Todo 列之下(`## Backlog` → `### Backlog`),不猜测工作流语义。满足「真任务绝不从默认视图消失」这一硬性不变量,同时不发明工具无法验证的语义映射。

### C9/C11/C13 — README 措辞

C13 开头重写为「问题→方法→当前边界」,三要素护城河(Git 物证 / 字节区间无损回写 / 只报告不擅改)首次同时出现在首屏 —— 其中**无损回写此前在开头完全缺席**。C11 事实源限定到 per-repo Project Task,并写明 Capture 的 `~/.todo/inbox.md` 不进 Git、不受 `.todo/history/` 还原点保护、需单独备份。C9 新增实验性 MVP 与待验收项披露,以及首次使用前提交/备份并审查 diff 的建议。

captain 复验:五处近期修好的段落(MSRV 1.88 行、lint 范围行、退出码 7、语义对账节、hooks 密钥警告)全部完好;零许可证声明;零不可核实断言。

**成员的一处正确克制值得记录**:任务描述里提供了 dogfooding 起始日、两周基线、5 人试用等具体事实,成员**主动不写**,理由是这些只出现在调研/对抗материал中,无法从权威项目状态文件独立核实。这是对的 —— 把无法核实的具体数字写进成熟度披露,正是 C9 所指控的同一类缺陷。

### 最终合并门禁

成员各自的绿色门禁**不构成合并态证据**(两名工程师并发编辑同一 `main.rs`,期间确实出现过一次瞬时编译中断,由成员间直接沟通解决)。以下为 captain 在全部工作落地后运行的权威结果:

| 门禁 | 结果 |
|---|---|
| `cargo test --workspace` | ✅ 0 failed |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | ✅ 0 error |
| `cargo fmt --all -- --check` | ✅ clean |
| `pnpm typecheck` | ✅ pass |
| `git diff --check` | ✅ clean |

### 已知残留(记录而非扩大范围)

`run_capture` 在 spawn 失败时仍返回 `String::new()`,而 `context() -> String` 没有位置区分「无内容可注入」与「子进程根本没启动」。要传播这一差异需改 `context()`/`context_with_deadline()` 的返回契约,超出 C6 范围。

实际风险已显著下降:接入 `current_exe()` 后自调用不再依赖 PATH ——

```
$ env -i PATH=/nonexistent HOME=/tmp/spawn todo hooks context --agent claude
TODO next: [gl7ocv] a task     (exit 0)
```

即 PATH 完全不可用时自调用仍成功,C6 场景已无法触达该分支。剩下的是真正罕见的情形(绝对路径已解析但 exec 仍失败:二进制被中途删除、权限变更、进程表耗尽),且 `todo hooks doctor` 已为运维方提供检查手段。若日后要闭合,应连同 `context()` 的错误契约一并设计。

对抗 3 剩余未闭合项:**C8**(无 LICENSE)—— 属产品决策,`DECISIONS.md:58-62` 四个选项仍未拍板,不在工程范围内。

## 证据门:已跑与不可跑

证据门的权威定义在 `调研文档/dk/我的理解-调研方案与当前实现对照3.md:71`:「漂移报告能否让看板重新被信任、round-trip 无损率基线,都要等 2 周 dogfooding 与外部试用数据」。本节把其中**当下可测的部分**跑出实测数字,并明确区分**结构上无法补的部分**。

### 一、无法补的部分(先说清楚,避免伪造成熟度)

| 门项 | 现状 | 为何补不了 |
|---|---|---|
| 2 周 dogfooding 基线 | ❌ 不可得 | 仓库首个 commit 为 2026-08-31,最新为 2026-09-01,**全部生命周期 2 天**。任何「两周基线」都是伪造。 |
| 5 名外部开发者试用 | ❌ 不可得 | 无外部用户;本机无法产生他人试用数据。 |

`TODO.md` 的真实自举足迹:`git log --follow -- TODO.md` 共 **6 个 commit,全部集中在 2026-08-31**。当前 13 条任务、1 条完成、2 个真实 `🔗` 物证区间。这是「自举已启动」的证据,**不是**「已持续运行两周」的证据。

因此 README 的成熟度披露继续**不写**具体天数与试用人数(见前节 readme-writer 的克制),只写可核实的状态。

### 二、round-trip 无损率基线(可测,已跑)

样本为含中文、emoji、制表符、变体选择符、代码围栏假任务、行内代码假任务、非标准 checkbox `[-]`、tag/priority/order_key 的文件。

| 验证项 | 方法 | 结果 |
|---|---|---|
| 反复 mutate/undo 后逐字节还原 | `done`/`undone` 循环 **100 次**,每次与原文 diff | ✅ **drifted 0 / 100**,末态逐字节相同(441 → 441 bytes) |
| 围栏假任务不被动 | 同上样本 | ✅ 围栏内容原样,全文 ID 数恰为 2(仅两条真任务) |
| 非标准 checkbox 不被吞 | `[-]` 行 | ✅ 保留,由 lint 以 warning 暴露 |

**基线结论**:在上述样本与 100 次循环下,round-trip 无损率 **100%**。这是本轮能给出的最强 round-trip 数字,但**样本由我构造**,不等于真实用户文件分布。

### 三、漂移报告是否可信(可测,已跑)

「看板能否重新被信任」的可测代理是:reconcile 在**健康**与**被破坏**两种输入下是否给出不同且正确的判断。

| 场景 | 结果 |
|---|---|
| 本仓库真实历史 | ✅ `no reconciliation findings (all checks ran)`,exit 0,**0 check did not run** |
| 构造仓库:物证区间有效 | ✅ 同上,exit 0 |
| 构造仓库:注入死 commit 引用 | ✅ `confirmed: evidence-missing`,exit 6 |

反向用例是关键:仅有「健康时为绿」不能证明报告有判别力,必须证明**被破坏时会变红**。两者齐备,该门项在结构层成立。语义层仍未接真实 provider(见 C1),故不能声称「任务正文与代码已核对」。

### 四、本轮暴露的真实缺陷:progress 标记无法清除

跑 round-trip 时首次得到 **DRIFTED**,根因不是回写失真,而是一条缺失的 CLI 能力:

- `todo progress <id> 50 --note "..."` 写入 `◐50`;
- `todo note <id> --clear` 只清 `↳` 说明行,**`◐50` 残留**;
- `todo progress <id> 0` 被拒:`progress must be 1-99; got 0. Use 'todo done' for complete, or leave progress unset for not started`;
- 但 `percent` 是**必填位置参数**,「leave progress unset」在 CLI 上**根本无法表达**;
- 目前唯一清除路径是 `done` 再 `undone`,即「先谎报完成再撤回」。

即帮助文本承诺了一条不存在的操作。改用 `done`/`undone` 清除后,100 次循环 drifted 0 —— 故**无损回写本身没问题**,缺的是清除 progress 的正当入口。已记入 `TODO.md` 待办,不在本轮扩范围修复。

## 配置任务列与 ecommerce 实战适配

对应任务 `57wlfi`；需求、接口与边界见 `task-columns-spec.md`。

问题并非空子菜单的 CSS 颜色：ecommerce/TODO.md 只有 `todo-spec: 1`，没有列声明；其二级标题是文档组织、三级标题是领域、四级标题是优先级。将它接管为任务来源不能等同于把分类章节改成状态列。

用户明确选择保留原结构，只追加空默认状态列。本轮新增任务菜单入口「配置任务列…」，支持默认列或显式勾选现有唯一二级标题。预览只读；确认绑定项目、来源、typed selection 和文件指纹，后台重算受控修改，写前创建历史。不会执行客户端的任意 proposed Markdown。

### 已取得的证据

- Rust 核心回归覆盖 ecommerce 脱敏结构、原正文逐字节前缀、任务 ID/顺序/checkbox、BOM/CRLF、未知 YAML、重复标题、未闭合围栏、陈旧指纹、只读/越界来源、2 MiB 大小与 diff 内存预算。workspace tests 与严格 Clippy 通过。
- HTTP 和 Tauri 命令层契约测试验证精确来源、项目切换拒绝、取消零写入、重复确认冲突与只读保护。HTTP red 测试在新增路由前返回 404，新增能力后的同一路径通过。
- API-shaped UI 与原生 invoke/event 桩覆盖两种配置、显式完成列、取消/返回/改选择、重复提交、权限撤销、迟到预览以及写入后迟到快照。审查发现的旧快照覆盖较新 watcher 权限结果已由先红后绿的回归锁定。
- WebKit 的 Tab 初始焦点落到 body 问题已复现并修复；320×568 与 986×320 对话框可访问，支持双向键盘循环，差异两侧与底部操作可滚动访问。WebKit 配置/菜单专项 37 项通过；此入口跳过的真实服务用例另行执行。
- `E2E_COLUMNS_BROWSER=webkit bash scripts/e2e-columns.sh ../ecommerce/TODO.md` 在真实文件的临时副本上通过：预览和取消后字节/历史未变；确认后的正文仍以完整原正文为前缀，所有任务的来源、ID、正文、完成状态、层级与说明保持；新增三个空列；随后显式移动才更改任务位置。测试前后原 ecommerce 文件 SHA-256 相同。Chromium 的真实副本流程也已通过。

### 最终门禁

- `cargo fmt --all -- --check`、`cargo test --workspace --locked`、严格 Clippy：通过。
- Chromium/移动端完整串行回归：126 项通过；该入口的 16 项 live 用例按设计跳过，分别在真实服务门禁运行。
- 真实 `todo serve` 全链路：15 项通过，包含配置后移动。
- WebKit 配置/菜单回归：37 项通过；真实 ecommerce 副本 WebKit 链路另 1 项通过。
- `pnpm build`（含 TypeScript 检查）与 `git diff --check`：通过。

完整 Chromium 第一次运行曾在页面初始化时失败一项，当时另一套 live 脚本正重建同一 `dist`；未把该次运行记为成功。停止并行构建后对同一最终产物完整重跑通过。测试采用独立端口/输出目录，既有失败断言未被删除或放宽。

### 验证边界

没有直接修改或提交 ecommerce 工作树，也没有把已有领域、状态表和说明重排到默认模板。没有访问 ecommerce 生产服务或凭据。WebKit 浏览器与 Tauri invoke/event 桩是自动化证据，但不等同于已自动操作用户那一个原生 WKWebView 窗口；最终原生视觉仍需区分。开发进程已编译注册新的原生命令。

## 未归列任务直接推进

对应任务 `angi74`。无列来源的卡片可以直接拖入「推进中」，或通过菜单「开始处理」打开固定的 `defaults_and_start` 预览；确认创建默认列并移动指定任务，不需要配置后再拖一次。已有同来源推进列时使用原有移动接口，不借用其他文件的列。

- 新变体在 core 中验证来源内 ID、未完成/未归列状态，并复制 Markdown 解析器识别的完整列表项。普通续行、嵌套子任务、已完成子任务与锚定说明均保留；只在嵌套任务脱离父项时按需减去原缩进。
- 预览零写入；确认经过同一锁、指纹与历史机制，一个还原点、一次原子写入。重复确认、陈旧指纹、来源只读、任务已变化或跨来源 ID 都拒绝。
- Rust 回归复现了「已有未声明的进行中标题且任务位于首项」的零距离移动误拒绝，修复后与 CRLF、长任务、子任务及续行保真回归一起通过。完整 Rust tests、格式、Clippy 通过。
- ecommerce 真实文件临时副本已在 WebKit 完成实际拖放：取消保留全部原文与历史，确认后的文件等于后端预览，指定任务出现在推进中，仅新增一个历史条目；原 ecommerce 文件 SHA-256 未变。
- 全量门禁曾再次发现旧的折叠待办栏拖放用例不稳定。临时事件探针复现 8 次中 1 次失败：动画未结束时落点为 y=579 且真实事件命中 Doing；正常落点为 y=676 命中折叠 Todo。测试改为等待实际 grid 动画结束，再拖动；不改变任务状态/排序断言，8 次复验均通过。探针已移除。
- 最终验证：Chromium/移动端完整串行 132 项通过，16 项真实服务用例按设计在默认入口跳过；WebKit 配置/菜单 43 项通过；真实服务 15 项通过；ecommerce 真实副本拖放与取消/确认另行通过。构建/类型检查、Rust 全量测试、格式与严格 Clippy 通过。
- 按用户要求，开发应用与 1420 热更新服务在测试期间关闭，最终门禁通过前不重新启动。
