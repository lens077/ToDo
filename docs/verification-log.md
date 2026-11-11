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

## 2026-09-27 至 2026-09-30 功能交付补记

本节由 2026-10-25 的任务对账补记：以下功能已进入代码与自动化测试，但此前未写入本日志。门禁在补记当天的修复后工作树上重新执行，数字是复验结果，不是原提交当天的记录。

### 任务正文 Markdown 与原地编辑（d140ad9、aebcfee）

- core 新增受控 `set_body` intent：只重写解析器识别的任务正文，保留稳定 ID、token、续行与文件其余字节；外部修改经锁与指纹拒绝。
- 桌面端按 Markdown 渲染卡片正文，编辑在居中浮层内完成；取消、空正文与冲突路径都有交互回归。
- `task-body-markdown.spec.ts` 与 Rust workspace 回归通过。

### 本地阅读库（f8afcbc、fb7975a、2ef9e9c）

- 阅读库归档到 `~/.todo/reading/`（SQLite + 图片），属于个人数据，不进入项目文件或 Git；ADR 见 `docs/adr/0003-reading-library.md`。
- 抽取用 `readabilityrs`，抓取在 Rust 端完成以绕开跨域与 Referer 限制；微信懒加载图片就地补丁，正文经 `ammonia` 清洗后进入 webview。
- 笔记使用 W3C 文本锚点（quote + 字符偏移），阅读位置同时存百分比与块序号；0 与 100 不存锚点。
- 浏览器可导入当前页面；`todo serve` 的 HTTP 传输暂不开放阅读库，桌面端通过 Tauri asset 协议读取 `$HOME/.todo/reading/**`。
- `reading.spec.ts` 与 reading 相关 Rust 回归通过（10 通过、1 忽略）。

### AI 会话与 DSH 桥（3ab4f11、48ff807）

- `todo ai` 的会话状态以 `(source, session_id)` 为主键，存于 `~/.todo/ai/sessions.json`，不进入 `TODO.md` 与 Git；状态机、排序与退出码见 `docs/ai-feed-spec.md`。
- DSH 桥接按宿主生命周期发布 `running` / `waiting` / `unread` / `seen` / `failed` / `handled`，并监听 `session/created` 与 `session/disposed`，避免保留已释放会话。
- `ai.spec.ts`、`crates/todo-cli/tests/ai.rs`、`serve_ai.rs` 通过；桥接目录自带独立依赖，本次未重新安装或重跑其测试。

### 匿名使用统计（0ad565f）

- 桌面端默认向自托管 Umami 发送视图名、动作名、版本、平台与屏幕尺寸；不发送任务内容、文件路径或项目名，不设 Cookie、不记录身份，可在设置关闭；CLI 不发送任何统计。
- 实现审查通过；该功能没有新增自动化测试，属于本节的验证边界。

### 双语官网与图标（ab7f200、f9e5f4b）

- `web/` 生成 `/en/`、`/zh/` 两份完整静态页与根路径语言路由，构建时校验两种语言结构一致；`npm run build` 通过。
- 图标改为图像优先的浅色/深色两版，favicon 跟随系统外观；`pnpm --filter @todo/desktop test:identity` 8 项通过。站点部署结果与图标视觉未在本轮浏览器中人工复核。

### 2026-10-25 对账与修复

- `todo evidence` 为 `n9f4pk`、`57wlfi`、`angi74` 补写了 Git 区间；`todo archive` 把 6 个已完成顶层任务移入 `TODO.archive.md`。本轮修复完成的 `krr4qq`、`z5t7gq`、`c2j6xb` 也随后归档（累计 9 项）。`todo lint` 无问题，`todo reconcile` 从 3 条 suspected 恢复为 0 finding（exit 0）。
- `todo progress <id> --clear` 删除 `◐` token，不再用 `◐0` 表达清除；`--clear` 与百分比互斥，严格策略下清除不要求说明。新增 `progress_can_be_cleared_without_claiming_completion` 与 `progress_requires_a_percent_or_clear` 回归。
- `hooks context` 的子进程在 Unix 放入独立进程组，超时用 `killpg` 终止整棵树；Windows 走 `taskkill /T /F`。新增 Unix 回归 `terminate_tree_reaches_descendants`，用 shell 派生的 `sleep` 验证后代确实被终止。Windows 分支未在 Windows 实机执行。
- 移除 README、DSH 桥文档、规格与调研文档中的本机绝对路径；验收清单里的错误家目录改为 `/path/to/ToDo`。
- 严格 Clippy 此前被 `crates/todo-core/src/reading.rs` 的 3 个既有 lint 阻塞：两处多余引用与一处相同分支。已修复，未改变行为。
- 同一批阅读库提交也未通过 rustfmt；`cargo fmt --all` 一并格式化了 `crates/todo-core/src/reading.rs` 与 `apps/desktop/src-tauri/src/reading.rs`，均为行为中性改动。

### 本轮门禁

| 门禁 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | ✅ clean |
| `cargo test --workspace --locked` | ✅ 0 failed |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | ✅ 0 error |
| `pnpm typecheck` / `pnpm build` | ✅ pass |
| `pnpm e2e` | ✅ 145 passed, 16 skipped（真实服务用例按设计跳过） |
| `./scripts/e2e-web.sh` | ✅ 15 passed |
| `pnpm --filter @todo/desktop test:identity` | ✅ 8 passed |
| `web/npm run build` | ✅ pass |
| `git diff --check` | ✅ clean |

### 验证边界

- 本轮没有可用的 Developer ID 证书与 Apple 公证凭据，未产出可分发桌面端 release。
- 三条 `#manual` 验收（原生文件选择器、看板写回、外部编辑刷新）仍未由真人执行；WebKit 原生窗口与 macOS 菜单栏未在本次复核。
- Windows 进程树终止只通过编译分支审查，未在 Windows 实机运行。
- 阅读库的网页抓取依赖真实网络；自动化测试覆盖解析与交互层，不覆盖特定站点的反爬变化。

## 2026-10-31 Workspace / Project Registry（Phase 1）

依据 `docs/workspace-project-knowledge-spec.html` 与 Phase 0 审计（`docs/workspace-knowledge-phase0-audit.md`）。实现多项目虚拟工作空间的数据层、Tauri 命令与工作空间视图。未触碰 TODO.md、reading.sqlite、inbox.md；`~/.todo/workspace/workspace.sqlite` 为独立新文件，删即回滚。

### 修改文件

- 新增 `crates/todo-core/src/workspace.rs`：Workspace/Project/成员关系注册表；`meta.schema_version` 显式迁移（旧库拒绝打开并报错，新库静默建表）；canonical path 去重；稳定项目 ID 与重关联；10 个单测。
- 新增错误变体（`error.rs`）与模块导出（`lib.rs`）。
- 新增 `apps/desktop/src-tauri/src/workspace.rs`：9 个命令薄封装；`lib.rs` 注册 + `workspace_open_project`（按 ID 打开，缺路径时拒绝并提示重关联）。
- 前端：新增 `lib/workspace.ts`（API + 内存 mock）、`components/WorkspaceView.tsx`；`Sidebar.tsx` 增加"工作空间"导航（阅读仍第一）；`App.tsx` 接入视图；`useDesktop.ts` 增加 `openRegisteredProject`（按 ID 换项目快照）；`desktop.ts` 增加 `openRegisteredProject`；`style.css` 增加 ws-* 样式。
- 新增 E2E `e2e/workspaces.spec.ts`（3 用例）。

### 验证

| 门禁 | 结果 |
|---|---|
| `cargo test -p todo-core --locked --lib workspace` | ✅ 10 passed |
| `cargo test --workspace --locked`（verify-quick 内） | ✅ 0 failed |
| `cargo fmt/clippy -D warnings` | ✅ clean |
| `tsc --noEmit` | ✅ pass |
| `playwright test`（全部） | ✅ 146 passed（含 3 个新增工作空间用例） |
| `make check-quick` | ✅ 绿（24s） |

### 已知边界 / 未完成

- 旧"选择目录"入口（CommandBar）仍独立存在，尚未合并为"添加项目到工作空间"；`desktop.json` 的 `last_project` 保持原语义（双读兼容，未迁移写入注册表）。
- 打开多项目后 `DesktopOps` 仍是单活动项目（backend 池化未做）；工作空间内"打开"通过切换活动项目实现，不并行持有多个 watcher。
- Phase 2（Project Profile / Environment）及之后的 Knowledge、Session、Reading 融合未开始。
- `todo contract check` 子命令仍不存在（AGENTS.md 与 CLI 不一致，遗留环境问题）。

## 2026-10-31 Project Profile 与 Environment（Phase 2）

依据同一蓝图。实现项目档案（用户确认字段）与 Declared/Observed 环境分离；扫描为只读、秘密不落库。schema v1→v2 增量迁移。

### 修改文件

- 新增 `crates/todo-core/src/profile.rs`：`scan_environment`（package.json/Makefile/compose/锁文件/`.env.example`，只记录环境变量名，绝不读 `.env` 或密钥文件）+ `ProjectProfile`/`EnvironmentProfile` 持久化；10 个单测。
- `workspace.rs`：schema_version 升至 2，新增 `project_profiles`、`environment_profiles` 表（声明与观测分列，扫描结构上不可能覆盖声明）；v1→v2 前向迁移测试。
- `apps/desktop/src-tauri/src/workspace.rs`：+5 命令（profile_get/set、environment_get/scan/set_declared）；`lib.rs` 注册。
- 前端：新增 `components/ProjectDetailPanel.tsx`（档案编辑 + 声明/观测双栏）；`WorkspaceView` 卡片增加"档案"展开；`lib/workspace.ts` 增加 API 与 mock；样式追加。
- E2E：`e2e/workspaces.spec.ts` +2 用例（档案保存、扫描保留声明）。

### 验证

| 门禁 | 结果 |
|---|---|
| `cargo test -p todo-core --locked --lib` | ✅ 73 passed（含 10 个 profile、11 个 workspace、v1→v2 迁移） |
| `playwright test`（全部） | ✅ 149 passed, 16 skipped（新增 2 用例：扫描后声明 JSON 保持 `{"node":"22"}` 不变） |
| `make check-quick`（fmt/clippy/test/build/契约） | ✅ 绿（127s） |

### 秘密保护证据

- `scan_records_env_names_but_never_values_or_dotenv`：`.env` 中 `SECRET_TOKEN=supersecret` 不出现在扫描输出序列化结果中；`source_files` 不含 `.env`。
- 扫描文件名单为硬编码白名单；无 glob 读取。

### 已知边界 / 未完成

- 观测扫描未覆盖：运行时版本号（未执行版本命令）、部署文件（Helm/K8s）、服务依赖的深度识别。
- Declared 目前是自由 JSON，无结构校验；Mismatch（声明 vs 观测差异）对比视图未做。
- Phase 3（Knowledge 文档发现与阅读器）未开始。

## 2026-10-31 Knowledge 文档发现与阅读器（Phase 3）

实现项目内 Markdown 的只读索引：规则建议逻辑类型 → 用户确认（确认结果在重扫描和内容变更后保留）→ 消毒 HTML 阅读视图（每次从磁盘重建，不落盘）。schema v2→v3。

### 修改文件

- 新增 `crates/todo-core/src/knowledge.rs`：WalkBuilder 扫描（跳过 .git/node_modules/target/dist/.todo/.virgo/.venv，≤2000 文件、512KiB/文件）、文件名/目录规则建议 11 种逻辑类型、content_hash 增量（未变跳过）、删除文件墓碑化、`render_markdown`（pulldown-cmark + ammonia，复用 reading 的 sanitize）。9 个单测。
- `workspace.rs` schema v3：`knowledge_documents` 表（UNIQUE(project_id, relative_path)，FK 级联）。
- Tauri：+4 命令（knowledge_scan/list/confirm/read）。
- 前端：新增 `lib/knowledge.ts`（API + mock，mock 渲染器做转义）；`ProjectDetailPanel` 增加"知识文档"区（扫描、类型下拉确认、建议/已确认徽章、阅读视图）。
- E2E：+1 用例（建议→确认覆盖→阅读渲染、断言无脚本执行）。

### 验证

| 门禁 | 结果 |
|---|---|
| `cargo test -p todo-core --locked --lib` | ✅ 82 passed（含 XSS 不通过、外部修改即读即得、墓碑保留、确认跨重扫描） |
| `playwright test`（全部） | ✅ 150 passed, 16 skipped |
| `make check-quick` | ✅ 绿（148s） |

### 边界与门禁对应

- 「现有文档不搬迁、不覆盖」：扫描只读，无任何写回文件的代码路径（单测断言扫描后文件原样、阅读即时反映外部编辑）。
- 「HTML 可由 Markdown 重建」：HTML 不持久化，`knowledge_read` 每次从磁盘渲染。
- 「恶意 Markdown」：`render_markdown_is_standalone_and_safe` 断言 `onerror` 属性被剥离。

### 已知边界 / 未完成

- notify watcher 增量扫描未接：目前是手动触发全量扫描（hash 对账后增量入库），自动触发留待后续。
- 无全文搜索（Phase 5 范围）；阅读器无目录大纲/锚点。
- Phase 4（Session 与上下文恢复）未开始。

## 2026-10-31 Session 与上下文恢复（Phase 4）

Session 存为项目内 Markdown（`<project>/.virgo/sessions/s-<ts>.md`），文件是事实源、Git 管历史；应用只渲染/校验。无数据库表——跨天恢复 = 读文件。

### 修改文件

- 新增 `crates/todo-core/src/session.rs`：begin（单活动会话约束）/ finish（写入 Goal/Completed/Files Changed/Decisions/Validation 表格/Unresolved/Next Actions/Context；**确定性收集** `git status --porcelain` 文件清单，非 git 或失败为空不报错；测试命令与结果由调用方提供，应用绝不运行测试）/ interrupt / list / resume（最新会话 + outstanding：active 取目标、completed 取下一步）。9 个单测。
- Tauri：+5 命令（session_begin/finish/interrupt/list/resume，按注册项目 ID 解析根目录）。
- 前端：新增 `lib/session.ts`（API + mock，同生命周期契约）；`ProjectDetailPanel` 增加"开发会话"区（恢复摘要、历史列表、开始/完成/中断表单）。
- E2E：+1 用例（begin→finish→resume，切视图后历史与待续保留；断言拒绝第二个活动会话）。

### 验证

| 门禁 | 结果 |
|---|---|
| `cargo test -p todo-core --locked --lib` | ✅ 91 passed（含 git 收集用临时仓库实证、finish 后 resume 取 Next Actions、interrupt 解锁） |
| `playwright test`（全部） | ✅ 151 passed, 16 skipped |
| `make check-quick` | ✅ 绿（137s） |

### Gate 对应

- 「应用重启后能恢复最近 Session、未完成事项和下一步动作」：Session 落盘项目内 Markdown，resume 输出最新摘要 + outstanding；桌面端档案面板载入时自动调 session_resume。
- 「确定性收集」：git 变更文件来自 `git status --porcelain`（单测用临时 git 仓库验证）；验证证据表格由用户/Agent 填写，无任何自动执行。

### 已知边界 / 未完成

- Session 与 Task 的关联（task_ids）未做 UI（模板字段已预留于规范，实现待 Phase 5/6 一并接入任务引用）。
- 会话文件时间戳为 epoch 毫秒（可排序），未格式化为 ISO-8601 字符串。
- Phase 5（Agent Contract/Skill/CLI/Validator）未开始。

## 2026-10-31 Agent Contract、CLI 与 Validator（Phase 5）

在既有 contract/hooks 基建上扩展：Session CLI（稳定 JSON 信封）、docs 验证器（ERROR 阻断）、Agent 接入文档与 CI 模板。未新造框架。

### 修改文件

- `crates/todo-core/src/session.rs`：+`docs_validate`（VDS001/002 ERROR、VDS003 WARNING、VDS004 INFO）+ `docs_validate_blocking`；5 个单测。
- `crates/todo-cli/src/main.rs`：+`todo session {begin,finish,interrupt,list,resume}`（JSON 信封 `todo.session.v1` / `todo.session-list.v1` / `todo.session-resume.v1`；validation 参数 `command|result|evidence`）、+`todo docs validate`（ERROR → exit 6）。
- `crates/todo-cli/src/hooks.rs`：托管 instructions 块增加会话工作流 3 行（begin/finish/resume/docs validate）。
- 新增 `docs/workspace-agent-integration.md`：CLI 契约表、验证规则表、CI 模板（GitHub Actions，ERROR 阻断）、两种接入方式。
- `crates/todo-cli/tests/cli.rs`：+3 集成测试。

### 验证

| 门禁 | 结果 |
|---|---|
| `cargo test --workspace --locked` | ✅ 409 passed, 0 failed（含 5 个验证器单测、3 个 CLI 集成测试） |
| `make check-quick` | ✅ 绿（136s） |

### Gate 对应

- 「CI 能阻断 ERROR」：`docs_validate_blocks_on_error_and_passes_clean_projects` 实测 exit 6；CI 模板见 docs/workspace-agent-integration.md。
- 「至少两种 Agent 接入方式」：① hooks 注入（instructions 块含会话工作流，测试验证落盘）；② 纯 CLI JSON 契约（begin/finish/resume 全链路 JSON 信封测试）。

### 已知边界 / 未完成

- CI 模板为文档形式，本仓库无 GitHub Actions 环境可实测（本地 `todo docs validate` 已实测 exit 6）。
- `todo serve` HTTP 未暴露 session/docs 端点（桌面端 Tauri 命令已覆盖）。
- Phase 6（Reading Context 融合，即早前审计的方案归位）与 Phase 7（Overview 汇总面板）未开始。

## 2026-10-31 Reading Context 融合（Phase 6）

阅读库升级为上下文阅读收件箱：收藏原因/关注问题/复访条件/个人笔记 + 项目任务多对多关联 + Markdown 引用导出。X/微信/网页解析器零改动。schema 以 `ensure_columns`（PRAGMA table_info 探测 + ALTER）原地升级，旧库无损。

### 修改文件

- `crates/todo-core/src/reading.rs`：articles 加 6 列（saved_reason/focus_questions/revisit_condition/personal_notes/revisit_type/parse_status）；新表 article_project_links、article_task_links（FK 级联、幂等）；`update_context`（这些字段的**唯一写路径**）、`add_failed`（解析失败仍保存 URL，parse_status=failed，可立即编辑上下文）、`refetch`/`replace_content`（只更新来源列——结构上不可能覆盖上下文）、link/unlink、`export_reference` 引用卡片、`export_project_index` 项目索引（按已读分组）。7 个单测。
- Tauri：+8 命令（update_context/refetch/link×4/export×2）。
- 前端：`lib/reading.ts` 类型与 API+mock；新增 `components/ReadingContextPanel.tsx`（阅读器内嵌面板：四字段、关联 chips、导出剪贴板、重新解析）；`useReading` 暴露 `applyDetail`。
- E2E：reading.spec.ts +1 用例。

### 验证

| 门禁 | 结果 |
|---|---|
| `cargo test -p todo-core --locked --lib` | ✅ 103 passed（含 legacy 库原地升级无损、refetch 后上下文/进度逐字段断言、失败收藏去重、关联级联清理） |
| `playwright test`（全部） | ✅ 152 passed, 16 skipped（含重解析后上下文/关联逐项断言、导出剪贴板内容断言） |
| `make check-quick` | ✅ 绿（144s） |

### Gate 对应

- 「解析失败仍能保存」：`add_failed` 落库 failed 记录 + E2E/单测覆盖 URL 保留与上下文编辑。
- 「重新解析不会覆盖个人上下文」：`replace_content` 的 UPDATE 语句不提及上下文列（结构性保证）；单测逐字段断言 + E2E 断言。
- 「不写入项目文件」（ADR-0003）：关联只存 reading 侧引用（注册项目 ID / 任务 ID），导出为剪贴板 Markdown，无任何项目文件写路径。

### 已知边界 / 未完成

- 导出为复制到剪贴板；写入项目 `docs/references/` 的文件写入流（预览+确认）延后（需 Phase 3 文档写入基建）。
- 复访条件仅存储（revisit_type），日期/任务完成触发属 Phase 6 原规范的"重新激活"，蓝图未列入必做。
- Phase 7（Overview 汇总面板与 UX 收敛）未开始。

## 2026-10-31 Overview 汇总面板（Phase 7，蓝图最后阶段）

跨项目汇总面板：每个项目的状态、里程碑、下一步/待续、阻塞、文档分类提示，一屏可答。纯只读聚合，不新增写路径。

### 修改文件

- 新增 `crates/todo-core/src/overview.rs`：`WorkspaceStore::overview()` 聚合注册表 + Profile + `.virgo/sessions/`（active→目标、completed→下一步）+ knowledge 未分类计数；缺失占位符显式区分"无"与"未记录"。4 个单测。
- Tauri：+1 命令 `overview_summary`。
- 前端：新增 `lib/overview.ts`（mock 复用 workspace/session/knowledge fixtures）与 `components/OverviewView.tsx`（汇总行、状态筛选、项目卡片）；Sidebar 增加"总览"（第一项），落点视图保持"目标"不变。
- E2E：新增 `e2e/overview.spec.ts`（2 用例：汇总行+卡片+筛选、空态指引）；reading.spec 首项断言更新为 总览→阅读。

### 验证

| 门禁 | 结果 |
|---|---|
| `cargo test -p todo-core --locked --lib` | ✅ 107 passed（含 finished 会话取 Next Actions、缺失路径计数、未分类文档计数） |
| `playwright test`（全部） | ✅ 154 passed, 16 skipped |
| `make check-quick` | ✅ 绿（178s） |

### Gate 对应

- 「用户可以从汇总面板判断每个项目的状态和下一步」：卡片展示 路径状态/生命周期/会话徽章/里程碑/下一步（优先 outstanding）/阻塞/文档分类提示；E2E 断言占位符显式（"—"）而非空白。

### 已知边界 / 未完成

- 落点视图保持"目标"（改为总览会破坏既有键盘/看板交互假设，留待 UX 收敛专项）。
- 键盘导航仅沿用既有全局行为，未为总览新增快捷键；"上下文过期（stale）提示"需要 Profile 的 confirmed_at 时间策略，未实现。
- 蓝图 Phase 0–7 全部交付；Reading Context 规范（原始文档）与蓝图 Phase 6 融合完成。

## 2026-11-06 M0-A 收口与门禁恢复（2026-11-03 审计 P0-1/P0-4/P0-5）

按 `docs/audits/2026-11-03-audit.md` 的 P0 项执行收口第一阶段（M0-A）：修复完整门禁红、消除文档阶段矛盾、忽略本地目录。本轮未执行任何 git commit / push（等用户批准 M0-B 提交计划）。

### 修改文件

- `apps/desktop/e2e/mobile.spec.ts`：导航断言由固定 `toHaveCount(6)` 改为断言 `Sidebar.tsx` navItems 的 9 个标签集合与顺序（按钮有显式 `aria-label`，徽标计数不干扰可访问名），数量由集合长度派生；新增/删除视图时测试会明确指出差异而非报莫名的数字差。
- `docs/workspace-knowledge-phase0-audit.md`：结尾"Phase 0 完毕，等待批准"加删除线并附 2026-11-06 更正——Phase 1–7 已全部交付（逐阶段证据见本日志 2026-10-31 各节），消除该文档与实际实现状态的矛盾。
- `CONTEXT.md`：新增 Progress 节，使 `.virgo/project.yaml` 的 `progress: CONTEXT.md` 映射指向真实进度内容（摘要 + 指向 `TODO.md` 与本日志）。
- `.gitignore`：忽略 `/x-downloader/`（17MB）与 `/.trae-html-share-packages/`（3.9MB）本地研究/工具导出，不再污染 `git status`。

### 验证

| 门禁 | 结果 |
|---|---|
| `cargo test --workspace --locked` | ✅ 420 passed / 0 failed（退出码 0） |
| `make check`（verify.sh 全量） | ✅ 退出码 0；mock E2E **155 passed / 16 skipped**（mobile 修复后由 154 passed + 1 failed 转绿），真实后端 web E2E **15 passed**（21.2s） |
| `make check-quick` | ✅ 绿（128s，退出码 0） |
| `todo contract check --root .` | ✅ `contract ok: ./.virgo/project.yaml (guided)`（退出码 0） |
| `todo lint` | ✅ `no issues`（退出码 0） |
| `playwright test mobile.spec.ts`（单 spec 复核） | ✅ 5 passed（2.3s） |

### Gate 对应

- 「mobile.spec.ts 不再依赖过时的固定导航数量」：`expectedNavLabels` 标签集合断言 + 数量派生，9 项与 `Sidebar.tsx:27-37` 的 navItems 一致。
- 「Phase 0 审计文档与实际实现状态一致」：文末更正块指向 Phase 1–7 的交付证据。
- 「CONTEXT.md 的进度映射指向真实进度内容」：Progress 节落地，`contract check` 复验通过。
- 「.gitignore 排除指定生成目录」：两目录已从 `git status` 消失（工作区条目 49→50：−2 已忽略目录，+3 本轮修改文件）。
- 「原有用户修改全部保留」：本轮仅新增 4 处修改（上列文件），26 处既有已修改文件与 19 处既有未跟踪文件逐字节未动（`git diff` 仅含本轮 4 文件）。

### 已知边界 / 未完成

- Phase 1–7 成果仍未提交 Git（等用户批准 M0-B 提交计划后执行）；本地 `main` 仍落后 `origin/main` 1 个提交（`3613d96`，只改 `web/deploy/chart/virgo/values-prod.yaml`，与本轮全部改动无文件交集）。
- `make check-quick` 128s 仍超脚本 2 分钟目标（沿用 2026-11-03 审计 P3 项，本轮未处理）。

## 2026-11-07 M0-B 提交执行（6 个提交，工作区清零，未 push）

用户批准 M0-B 计划（`docs/audits/2026-11-06-m0b-commit-plan.html`）后逐个执行；51 个文件全部入库，`git status` 清零。**未执行 push；与 `origin/main` 的合并留待 push 授权时处理。**

### 提交序列

| # | 提交 | 内容 |
|---|---|---|
| C1 | `77cf6ab` feat(core) | workspace/profile/knowledge/session/overview 五模块 + lib/error（+3,062） |
| C2 | `f33a9ff` feat(cli) | session/docs 子命令 + agent 会话指引 + 集成测试（+399） |
| F | `f5f3ed3` fix(core) | **计划外修复提交**：reading.rs 落库（+818），见下「执行偏差」 |
| C3′ | `ccd662e` feat(desktop) | 原计划 C3+C4 合并：九视图 + 23 命令 + 阅读上下文 UI + e2e（22 文件，+2,930） |
| C4′ | `3f243f1` chore(deps) | Tauri 客户端版本固定（用户自主变更独立成提交） |
| C5′ | `93d7965` docs | 阶段文档对齐 + 验证日志 + M0-A 杂项（17 文件，+2,142） |

### 执行偏差（两点，均有代码证据）

1. **原计划 C3+C4 合并为 C3′**：预检发现 `lib.rs` 的 diff 同时包含 workspace 命令注册与 C4 的 6 个 reading 命令注册（`reading::reading_update_context` 等，diff 行 69–76）——`generate_handler!` 引用尚不存在的函数会使 C3 单独无法编译，而 C4 又依赖 C3 的 `crate::workspace`，构成循环耦合。按计划排除的 `add -p` 宏内拆行不采用，合并为一个提交；总提交数仍为 6。
2. **计划外 fix 提交 `f5f3ed3`**：C1 提交后符号核查发现其快照不可编译——C1 的 `lib.rs` 再导出 `ReadingContext/ReadingContextInput/ReadingLink`，但三个类型在未提交的 `reading.rs` 中（`cargo test` 编译的是工作区而非提交快照，属验证盲区）。因 `git reset` 属禁令需单独批准，采用修复性提交落库 `reading.rs`；**`77cf6ab` 与 `f33a9ff` 两个快照保留该编译瑕疵**，push 前如需完全可二分的历史，可将 `f5f3ed3` 折叠进 `77cf6ab`（需届时批准 rebase/reset）。

### 逐步验证

| 时点 | 命令 | 结果 |
|---|---|---|
| C1 后 | `cargo test -p todo-core --locked` | ✅ 278 passed / 0 failed（注：编译对象为工作区，快照缺陷由后继符号核查暴露） |
| C2 后 | `cargo test -p todo-cli --locked` | ✅ 103 passed / 0 failed |
| fix 后 | `git show HEAD` 符号复核 | ✅ 三个类型齐备，HEAD 起每个提交可编译 |
| C3′ 前 | `pnpm typecheck` + `pnpm build` | ✅ tsc 0 错误、vite built |
| C4′ 前 | `pnpm install --frozen-lockfile` | ✅ 锁文件一致（275ms） |
| C5′ 后 | `make check` 全量 | ✅ 退出码 0；mock E2E 155 passed、web E2E 15 passed |

### 已知边界 / 未完成

- **未 push**：本地 `main` 现领先 `origin/main` 6 个提交、落后 1 个（`3613d96`）。push 前需合并该提交（零文件交集，预期无冲突），合并与 push 均待单独授权。
- C1/C2 快照编译瑕疵（上述偏差 2）；`f5f3ed3` 提交信息中已写明折叠方案。

## 2026-11-07 M1–M5 路线图交付（代决策模式，一次性验收）

授权方式：无需询问、逐里程碑代决策并记录到 `docs/audits/2026-11-07-m1-m5-decisions.html`（18 条，每条含「如何修改」），全部完成后统一验收。验收报告：`docs/audits/2026-11-07-m1-m5-acceptance.html`。

### 交付内容与提交

| 里程碑 | 提交 | 内容 |
|---|---|---|
| M1 | `01cdbb9` feat(session) | Session–Task 关联：finish `--task` 写 `## Task Links`（Markdown 事实源），resume 解析任务当前状态（found/completed/column/body，仅根 TODO.md，缺失降级不报错）；桌面表单+状态列表；mock 同契约 |
| M2 | `fa6171a` feat(ai) + `dbf0493` fix(ai) | 审批确认对象（ai-feed-spec v2）：`action`/`params` 发布、ToDo 侧规范化 JSON SHA-256 摘要、ack 门禁（盲/错摘要 exit 1，锁内校验）、参数重发自动作废旧审批、v1 发布方与存量文件双向兼容（真实 store 实测）、桌面「确认执行」卡片 |
| M3 | `5949fd6` feat(session) | 确定性验证器：finish 自动追加 lint / reconcile 结构层 / docs validate 三行 virgo 验证（agent 行在前、来源标注「virgo 自动（确定性检查，非 agent 自报）」），失败不阻断，仍不执行项目任意测试 |
| M4 | `03950f5` feat(knowledge) | 全文搜索：schema v4 FTS5（doc_id/title/body）、CJK 字间空格短语方案（两字中文可查）、逐词引号防注入、snippet 高亮展示、桌面搜索框；前端订阅既有 watcher 事件 5s 去抖自动重建（仅扫描过才触发） |
| M5 | `81fb857` docs(project) | 投影接入决策：新建 `benchmark_projection_10k_tasks`（`#[ignore]`）实测 release 10k 任务 parse **59ms** / 投影全量重建 **28ms** / 增量更新 **132ms** → **暂不接入**（`DECISIONS.md` 决策⑥，三触发条件重开）；`todo note b6r1vy` 更新备注 |

### 逐里程碑验证

| 门禁 | 结果 |
|---|---|
| `cargo test -p todo-core --locked`（session/ai/knowledge/workspace 分模块及全量） | ✅ 全绿（全量 284 passed 起步，逐里程碑递增） |
| `cargo test -p todo-cli --locked`（全量，含 serve_ai、ai 集成） | ✅ 104 passed / 0 failed |
| `cargo test -p todo-desktop`（src-tauri 测试，ai_feed 修复后） | ✅ 含于终验 |
| `pnpm typecheck` + `pnpm build`（各里程碑） | ✅ 0 错误 |
| `playwright test workspaces.spec.ts`（M1/M4） | ✅ 7 passed |
| `playwright test ai.spec.ts`（M2，徽标计数随演示确认记录更新） | ✅ 13 passed |
| `cargo test --release --test projection -- --ignored benchmark`（M5） | ✅ 数字见上表 |
| `todo lint` / `todo contract check --root .`（M5 后） | ✅ no issues / contract ok |
| **终验 `make check`（verify.sh 全量）** | ✅ **退出码 0**：cargo 429 passed / 0 failed，mock E2E 155 passed / web E2E 15 passed |

### 执行记录

- 过程中一次 `make check` 红被如实处理：① `cargo fmt` 对 M1/M3 测试代码的格式意见；② `todo-desktop` 的 `tests/ai_feed.rs` 仍在用 M2 之前的 `AiUpsert`/`ai_ack` 旧签名（此前仅 `cargo check` 未编译该 crate 测试）；③ AI 演示确认记录使 mock 徽标 +1，`ai.spec.ts` 四处计数断言随之更新。均修复后终验全绿。
- 全程未 push；本地 `main` 现领先 `origin/main` 13 个提交、落后 1 个（`3613d96`，零文件交集）。

### 已知边界 / 未完成

- 见决策文档各条「已知边界」：M1 多来源解析（D-M1-2）、M2 DSH 桥未升级（D-M2-3）、M4 无拼音/语义搜索（D-M4-1）、M5 `b6r1vy` 复选框状态待用户裁决（D-M5-1）。
- push、历史整理（`f5f3ed3` 折叠）、CSP 收窄三项均待用户验收时单独授权（D-M0-1/2/3）。
- 执行报告：`docs/audits/2026-11-07-m0b-execution.html`。

## 2026-11-07 远端合并与 push（D-M0-2 执行）

用户授权「push 与远端合并」后执行：

1. `git fetch origin`：远端仍在 `3613d96`，本地领先 15 个提交。
2. `git merge origin/main` → 干净合并 `5b44806`（远端提交只改 `web/deploy/chart/virgo/values-prod.yaml`，与本地零文件交集，预期零冲突兑现）。
3. `make check-quick`：合并后首次以 `merge-base(HEAD, origin/main)=3613d96` 为 diff 基准，暴露 `docs/workspace-project-knowledge-spec.html` EOF 多一空行（`93d7965` 提交时即存在，此前 check-quick 的基准未覆盖该文件）→ `9022267` 修复 → **verify-quick 绿（25s）**。
4. 决策文档 D-M0-2 标记为已执行。
5. `git push origin main`：见下方结果行。

### 已知边界 / 未完成

- D-M0-1（`f5f3ed3` 折叠）按决策保留未折叠历史，已随本次 push 进入远端；如需可二分历史需在远端做强制操作，不建议。
- D-M0-3（CSP 收窄）仍挂起到发布前。

## 2026-11-11 Android APK 构建打通（移动端可行性验证）

背景：用户提出「快速保存链接 + 备注 + 多端同步」需求，移动端是其中一端；本次按决策③ P1 路线先验证 APK 产出链路（DESKTOP-MVP 未扩范围）。

1. `apps/desktop/src-tauri/src/lib.rs` 桌面专属 API 用 `#[cfg(desktop)]` 隔离（4 处）：global-shortcut 插件导入与注册（移动端为空壳 crate）、setup 内快捷键注册（移动端置说明性 `shortcut_status`）、`unminimize()` 调用、`show_capture_window`。语义不变：桌面路径逐字节等价。
2. `pnpm tauri android init` 生成 `src-tauri/gen/android`；Gradle 仓库全部换成阿里云镜像（`settings.gradle`、`build.gradle.kts`、`buildSrc/build.gradle.kts`、新建 `buildSrc/settings.gradle.kts`）——原报错为 `plugins.gradle.org` 的 `kotlin-build-statistics:2.3.21` TLS 握手被断（国内网络），镜像后配置阶段通过。
3. 验证实际执行结果：
   - `cargo check --target aarch64-linux-android`：通过（1m05s）；
   - `cargo check`（桌面目标）：通过，无回归；
   - `./gradlew help`：通过（镜像源解析 kotlin-dsl 插件成功）；
   - `pnpm tauri android build --apk --target aarch64`：**产出 `app-universal-release-unsigned.apk`**（Gradle 9.6.1 + AGP 9.3.1，警告均为 AGP 9 过时 API 提示，不影响产物）。

### 已知边界 / 未完成

- 未真机安装运行验收；release APK 未签名（装机先走 `--debug` 包或自行配 keystore）。
- 移动端 Rust host 仍直读本地路径：Android 沙盒无 `~/.todo`，数据通道待链接保存需求的 sync-server（服务端 API）落地，这正是决策③「移动端只能是同步副本消费者」的印证。
- `gen/android` 依赖本机 SDK/NDK 路径（`local.properties` 已被生成 .gitignore 忽略）；JNI 符号链接每次构建自动重建。

## 2026-11-11 Android 真机验收（启动修复后）

1. 首次真机启动闪退：logcat 实锤 `lib.rs:648` panic —— `runtime_capture_store().expect()` 在 Android 沙盒触发（`dirs::home_dir()` 返回 `None`）。
2. 修复：`runtime_capture_store` 移动端回退到可写私有目录（`$HOME`，兜底 `TMPDIR`/应用缓存目录）；`tauri.android.conf.json` 只保留 main 窗口（capture 速记窗是桌面快捷键专属，此前 Android 把它当主窗口显示了）。ai/reading/workspace/preferences 原本就走 `Option` 降级，logcat 可见优雅降级日志。
3. 真机（vivo V2425A，USB）实测：安装 → 启动无 panic（`desktop_status startup_ms=92`，shortcut_status 如实报移动端说明）→ 截屏确认主界面完整渲染（底部导航、onboarding、顶栏）。
4. 门禁插曲：verify-quick 的 `git diff --check <merge-base> HEAD` 三次报 `buildSrc/build.gradle.kts:24 blank line at EOF`——0f6bace 把带 EOF 空行的生成文件提交了；552f47a 修复后区间 diff 干净。另：AGENTS.md 的 `todo contract check` 实为 `cargo run -p todo-cli -- contract check --root .`（全局安装的 todo 二进制太旧没有该子命令）。

### 已知边界 / 未完成

- 移动端本地 inbox 落在应用缓存目录（TMPDIR 兜底），系统清缓存即丢——真数据通道待 sync-server（决策③：移动端是副本消费者）。
- reading/workspace 沙盒里不可用，阅读视图在移动端为空属预期。
- release 签名、图标（`pnpm icon` 的 android 删除段）待发布前处理。

## 2026-11-11 sync-server B1（crates/sync-server MVP）

规格：`docs/sync-server-spec.md`。任务：5uhqjl（B1）、yfj0ol（B2 部署）、u9lj4a（C 层）。

1. 新 crate `crates/sync-server`（bin `virgo-sync`）：axum 0.8 + `ReadingStore` 复用（root 可指定，解析/备注/进度零复制）。API：`POST /links`（收藏优先，stub 毫秒级返回 + 后台 refetch + SSE ping）、`GET /articles`（全量对账模型）、detail/context/progress/read/delete/refetch、`GET /stream`（SSE change 事件）、`GET /images` 静态、`GET /healthz`（免鉴权）。鉴权 Bearer token（env 注入，缺失拒启），blocking 调用走 spawn_blocking。
2. 单测 7 个全绿（鉴权 401/健康检查豁免/保存-列表-备注-删除全流程/非法 URL 422/未知 id 404/SSE 广播）。
3. 真实进程冒烟（127.0.0.1:18099）：healthz ok；无 token 401；保存 201；备注持久化验证；SSE 收到 `event: change`；后台解析实测 `example.com` → title 'Example Domain'、word_count 25（fetch→readability→落库全链路通）。
4. X 链接冒烟失败定位：测试用推文 ID 为编造（fxtwitter 对不存在推文正确返回 404），且沙盒网络不可达 `cdn.syndication.twimg.com`；解析代码为桌面端已验证原逻辑未改动，待 B2 部署海外服务器后真实推文验证。
5. `make check-quick` 绿（34s）。

### 已知边界 / 未完成

- 全量对账模型：个人库 >5k 条时需换 cursor 增量（触发条件见规格 §3）。
- token 比较非常数时间；离线写入队列未做（规格 §3/§6）。
- B2 部署（镜像/helm/Secret）待用户给部署窗口。

## 2026-11-11 sync-server B2（todo-sync 部署与真机测试）

1. Dockerfile 两处修复：src-tauri 需整目录 COPY（manifest 声明 lib path，只拷清单解析失败）；runtime 用户 `sync` 与 Debian 自带系统账号冲突改名 `virgo`。镜像 `todo-sync-api:715a8ca` 已推 TCR（构建上下文仓库根 + `.dockerignore`）。
2. 部署踩坑与结论：
   - `virgo-tcr` imagePullSecret 是 namespace 级，已复制到 `todo`；
   - 按 **OCI index digest**（buildx 默认产物）拉取在 k3 containerd 报 NotFound，按 **平台 manifest digest**（`9913426b…`）或 tag 正常——镜像 tag 钉平台 manifest digest；
   - openebs-lvm RWO 卷在 k3 有 kubelet 残留挂载（verifyMount），删 pod 等周期清理后恢复；
   - PVC 绑定 k3 节点亲和，nodeSelector 绕不开（chart 已加 `api.nodeSelector` 通道备用）。
3. 部署成功（helm revision 4，pod Running on k3）。端到端实测（集群内 Service 路径）：healthz 200；无 token 401；`POST /links` 201（stub + 备注「部署验证」）；8s 后 GitHub 链接解析完成（title 'GitHub - rust-lang/rust…'，wc=236）；真实推文 `x.com/jack/status/20` 解析成功（title 'just setting up my twttr'，wc=9，fxtwitter 200 可达）。
4. `make check-quick` 绿。

### 已知边界 / 未完成

- **外网 `https://todo-sync.apikv.com/healthz` 返回 Pangolin 的 401**（响应体 'Unauthorized'、content-type text/plain，非本服务格式）——Pangolin 资源默认开了认证，需用户在 Pangolin 控制台对该资源关闭认证（纯 API 服务，客户端只带 Bearer）。集群内路径已全部验证通过。
- `cdn.syndication.twimg.com` 集群内 SSL 握手失败（exit 35），fxtwitter 主路径可用、兜底路径不可用。
- ArgoCD Application 注册待路由全通后补（当前 helm bootstrap revision 4 手工管理）。

## 2026-11-11 sync-server 外网链路打通（503 定位与修复）

1. 用户关闭 Pangolin 资源认证后 401 变 503；逐层定位（集群内经网关 IP 直打）发现 **503 来自 Cilium 网关本身**（对照 virgo 200），与 Pangolin 无关——外网链路是 client→Pangolin(TLS)→cilium-gateway→Service。
2. 根因：`networkpolicy.yaml` 沿用 sponsor-api 图的 k8s NetworkPolicy `namespaceSelector: io.cilium/k8s=default`，但 default ns 没有该标签（该图从未上线验证过），Cilium Gateway 的流量身份是 **ingress entity**，规则什么都放不进来 → no healthy upstream。正确模式照 virgo 图改写为 `CiliumNetworkPolicy`（fromEntities: ingress/host/remote-node；egress DNS + toCIDRSet 全网段除内网——CNP 语法是 toCIDRSet 而非 ipBlock/except）。
3. 修复后全绿实测（外网 https://todo-sync.apikv.com）：healthz `ok`；无 token 401；带 token 保存 201；列表返回已解析文章（新增 axum 仓库链接 wc=581）；CNP egress 下 fxtwitter 仍 200。

## 2026-11-11 sync-web C1（网页端小前端 MVP + 服务器静态托管）

1. `apps/sync-web`（React 19 + Vite 8，pnpm workspace 成员）：Token 配置（localStorage）、快速保存（URL+备注，收藏优先语义）、列表（搜索标题/链接/摘要/站点 + 全部/未读/已读筛选）、阅读详情（正文渲染/备注编辑失焦保存/标记已读/重新解析/删除/打开原文）、SSE 订阅（fetch 流解析——EventSource 无法带 Authorization 头，断线 3s 重连）。构建产物 228KB JS（gzip 72KB）。
2. sync-server 增加 `VIRGO_SYNC_WEB_DIR` 静态托管：API 路由继续走 token 门禁，静态资源与 SPA fallback 免鉴权（页面本身不含数据）。坑：`ServeDir::not_found_service` 对 `/anything` 不生效（404 直通），改用 `ServeDir::fallback(ServeFile(index))` 后 SPA fallback 正常。
3. 单测 8 个全绿（新增：静态页免鉴权 + SPA fallback + API 仍 401）；本地真实进程冒烟：`/` 出 HTML、assets 200、API 401。`make check-quick` 绿。
4. 镜像 Dockerfile 增加 node:26-alpine 阶段（pnpm 过滤安装 + 构建），产物入 `/app/web` 并默认启用 `VIRGO_SYNC_WEB_DIR`。

## 2026-11-11 sync-web C1 部署验收（外网全通）

1. 镜像重建（Node 26 无 corepack，Dockerfile 改 `npm i -g pnpm@12.15.0`）。
2. 部署插曲两件：① digest 拉取再次 NotFound（TCR 行为不稳定，此前同方式成功过）——**降级为按 tag 钉版本**，digest 钉版待 CI 阶段重验；② `virgo-tcr` imagePullSecret 在 todo ns 被回收——根因：virgo 的该 Secret 由 ESO 管理（ownerRef 指向 ExternalSecret），手工复制把 ownerRef 一起带走，指向 todo ns 不存在的 ExternalSecret 被 GC 清理。正确做法：照 virgo 建 ExternalSecret `virgo-tcr`（ClusterSecretStore openbao），Synced True。
3. 外网最终验收全绿：healthz `ok`；`/` 返回 Virgo Sync 页面；SPA fallback `/anything` 200；API 无 token 401；带 token 保存 201；tauri 仓库文章解析 521 词（首次瞬时失败，`refetch` 恢复——失败→重解路径如设计工作）。

### 已知边界 / 未完成

- 镜像版本钉 tag 不钉 digest（TCR digest 解析在 k3 上不稳定），CI 化时重验。
- C2（桌面双向对账）未开始：dj9w2g。

## 2026-11-11 desktop-sync C2（桌面阅读库双向对账）

1. `apps/desktop/src-tauri/src/sync.rs`：`sync::run(store, base, token)`——URL 为唯一键的全量对账。上行：本地有/服务器无 → `POST /links`（stub+context，服务器后台解析）；下行：服务器有/本地无 → `GET detail` → `add_failed`+`update_context`+本地 `refetch`（每轮预算 10，超出留 stub 下轮续）；重试：本地 wc=0 的 stub 在预算内继续 `refetch`。已有条目的备注/进度互不覆盖（v1 冲突策略：随创建走，之后各自独立）。
2. Tauri 命令 `reading_sync_now`（spawn_blocking）；前端 ReadingView 加「同步」按钮 + 配置面板（base/token 存 localStorage，键与 sync-web 一致）；浏览器 mock 构建下按钮不渲染（E2E 不受影响）。
3. 集成测试 `tests/sync.rs`：起真实 `virgo-sync` 子进程（按 workspace target 定位，缺失时兜底构建）做双向往返——上行/下行/备注随行/二次幂等全部断言通过。
4. `make check-quick` 绿。

## 2026-11-11 Android 分享接收（D 层 v1）

1. 新增 `ShareReceiverActivity`：注册 ACTION_SEND text/plain；从分享文本提取第一个 http(s) 链接，直接 `POST {base}/links`（不进 WebView 主界面）；token 存 SharedPreferences（首次分享出极简表单填一次，默认服务器 todo-sync.apikv.com）；成功/失败 Toast 并退出。
2. `--debug --target aarch64` APK 构建通过（含新 activity）。真机验收待设备连接（本轮手机未连接）——验收步骤：安装 → 模拟 `am start -a android.intent.action.SEND --es android.intent.extra.TEXT "<url>" -n com.sumery.virgo/.ShareReceiverActivity` → 首次填 token → 服务器列表出现该 URL。

## 2026-11-11 Android 分享接收真机验收（51swhk，通过）

1. 设备：vivo V2425A（序列号 `10AEB72BCN000EJ`，Android 17）。覆盖安装 04:17 构建的 universal debug APK；`aapt2 dump xmltree` 确认清单含 `com.sumery.virgo.ShareReceiverActivity` 且 `exported=true`。
2. 首次分享：`am start -n com.sumery.virgo/.ShareReceiverActivity -a android.intent.action.SEND -t text/plain --es android.intent.extra.TEXT "<url>"` 后出现极简表单（服务器默认 `https://todo-sync.apikv.com` + Token 输入 + 保存按钮），首次配置路径成立。
3. 填 token 的坑：`adb shell input text` 会把 hex 打进 vivo 中文输入法的候选流程，实际落成中文字符；改用 `run-as` 写入 `shared_prefs/virgo-sync.xml`（键 `base`/`token`，与表单同源）绕过 IME。
4. 验收结果：分享 `https://example.com/virgo-android-acceptance-20261111` 后 Activity 自动关闭回桌面，`ShareReceiver` 无 W/E 日志；`GET /articles` 条目数 4 → 5，新增记录的 url/title 与预期一致（stub 先回、后台解析），证明真机 → 公网 → 服务器写入链路通。
5. 去重路径顺带验证：`https://github.com/rust-lang/rust` 已在库中，分享后未新增重复条目。
6. 遗留：首次表单需手工输入 64 位 hex token，体验差；后续可考虑粘贴框或二维码导入，不在本卡范围。

## 2026-11-11 移动端底部导航 M1（c947n8）

1. 新增 `MobileTabBar`：窄屏（≤719px）专用底栏，4 个可自定义视图（目标 / 阅读 / 随记 / 总览）+ 最右固定「我的」；断点内 `.sidebar { display: none }`，桌面断点外组件默认 `display: none`，侧栏结构与行为不变。
2. 「我的」打开现有 SettingsDialog，并新增「更多视图」分区（仅窄屏可见）：列出未入栏的 5 个视图，保证任何视图可达。该分区属于 M4 的「完整入口」，但 M1 若不提前落地，工作空间 / 象限 / AI / 扫描与历史 / 规范 会在移动端变为不可达。
3. 测试：`mobile.spec` 重写为断言 5 个槽位与顺序、按钮高度 ≥44px、底栏贴底、`.sidebar` 隐藏、经「我的」进入工作空间；`ai.spec` 的 640px 场景改走「我的」→ AI。identity 8 项通过；全量 E2E 154 通过（16 skipped，1 项 AI 窄窗用例修复后通过）。
4. `make check-quick` 绿。

## 2026-11-11 移动端底部导航 M2（c947n8）

1. 偏好（localStorage `todo.app-settings.v1`）新增 device-local 字段 `mobileTabBar: { order, version: 1 }`，默认 `['goal','reading','capture','overview']`，与 M1 观感一致；不进 sync-server（决策③：移动端是副本消费者）。
2. 规范化：未知 id 静默丢弃、重复去重、最多 6 项；不足 2 项回退默认 4 项，不弹恢复提示。首次订阅时 `repairStoredSettings` 把规范化结果回写存储（幂等，只跑一次）。
3. 视图 id 的唯一事实源移到 `lib/views.ts`（`MAIN_VIEW_IDS` / `isMainView`），`Sidebar` 重新导出 `MainView`，偏好层不再依赖 React 组件。
4. 底栏列数改由 `--mobile-tab-count` 驱动；「更多视图」列表随偏好变化，只列未入栏视图。
5. E2E 新增 2 条：脏数据（未知 id / 重复 / 超 6 项）渲染 6 项并回写；仅 1 项时回退默认 4 项。移动端 7 项通过，全量 157 passed（16 skipped），`make check-quick` 绿。

## 2026-11-11 移动端底部导航 M3（c947n8）

1. 编辑入口两处：底栏非固定项长按 400ms（长按后的 click 被抑制，不会误切视图），或「我的」→ 设置 → 底部导航栏。
2. 编辑器 `MobileTabBarEditor`：上半区当前栏预览（长按 300ms 拖动重排、− 移除），下半区未入栏视图池（＋ 加入）；每次操作即写偏好，无保存按钮。
3. 拖拽走 Pointer Events（Android WebView 的 HTML5 DnD 不可用）：拖动开始时快照槽位坐标做命中判定，避免 chip 换位后命中漂移导致来回抖动；换位用 FLIP（Web Animations API）补间；拖动期间 `body.is-tab-dragging` 锁定滚动与选择。
4. 数量约束：2~6；到下限时 − 禁用并提示「至少保留 2 个视图」，到上限时 ＋ 禁用并提示「最多 6 个视图，其余收纳在『我的』」。
5. 长按入口打开设置时自动滚到「底部导航栏」分区（`initialSection`）。
6. E2E 新增 3 条：长按开编辑器且不切视图；增删到 6 / 2 的 clamp 与按钮禁用；拖拽换位后偏好落盘。移动端 10 项通过，全量 160 passed（16 skipped），`make check-quick` 绿。

## 2026-11-11 移动端底部导航 M4（c947n8，真机部分待补）

1. 「更多视图」分区已在 M1 提前落地，M2 起随偏好动态过滤，只列未入栏视图。
2. 角标：`MobileTabBar` 接收与侧栏一致的计数（阅读 / 目标 / 随记 / 象限 / AI），只有对应视图入栏时才渲染；「我的」固定槽永不显示角标。
3. E2E 新增：AI 入栏后出现数字角标、移除后角标随视图一起消失。移动端 11 项通过，全量 161 passed（16 skipped）。
4. `make check-quick` 绿；`pnpm tauri android build --apk --debug --target aarch64` 构建通过（486MB）。
5. 真机项见下一条记录。设备在首次构建后断开，重连后补测完成。

## 2026-11-11 移动端底部导航 M4 真机验收（c947n8，通过）

1. 设备：vivo V2425A（序列号 `10AEB72BCN000EJ`，Android 17）。覆盖安装 universal debug APK 后通过 `adb forward tcp:9222 localabstract:webview_devtools_remote_<pid>` 接 CDP，用 Playwright `connectOverCDP` 驱动真实 WebView（含 `Input.dispatchTouchEvent` 触摸事件），截图用 `adb exec-out screencap`。
2. 默认五槽：目标 / 阅读 / 随记 / 总览 / 我的，`grid-template-columns` 为 `45px ×4 56px`；角标在无计数时不渲染。
3. 加到上限：编辑器逐项「＋」到 6 个视图后底栏 7 槽（`45px ×6 56px`），视图池 3 个按钮全部禁用，提示「最多 6 个视图，其余收纳在『我的』」；「更多视图」同步只剩 AI / 扫描与历史 / 规范。
4. 首次真机截图发现并修复：6 槽时 10.5px 标签在 45px 列宽内折成两行（「工作空间」），把底栏撑高。改为标签 `white-space: nowrap` + 省略号，并按 `data-count` 把 5 / 6 槽字号降到 10 / 9px；重建安装后 6 槽标签单行、底栏高度与 4 槽一致。
5. 减到下限：逐项「−」到 2 个视图后底栏 3 槽（`141px ×2 56px`），两个移除按钮禁用，提示「至少保留 2 个视图」，池中 7 个视图可加入。
6. 触摸拖拽：CDP `touchStart` → 450ms → 8 次 `touchMove` → `touchEnd`，把「目标」从第 1 槽拖到第 4 槽。拖动期间 `body.is-tab-dragging` 为真，`window.scrollY` 与 `.settings-body.scrollTop` 全程 0（Android WebView 内拖拽不带动页面滚动），松手后偏好落盘为 `["reading","overview","workspaces","goal"]`，底栏顺序与之一致。
7. 长按入口：底栏「目标」按住 600ms 后设置打开并停在「底部导航栏」分区，当前视图未被切换（仍是 AI）。
8. 全部 9 个视图可达：「我的」→「更多视图」列出未入栏的 5 个视图，点 AI 后设置关闭、AI 视图渲染。
9. 观察（不阻塞）：AI 视图在未选项目时顶部显示红色「AI 会话存储不可用：无法确定主目录」，是既有行为，与本次改动无关。
10. 收尾：验收后把设备偏好复位为默认 4 视图，便于人工复看。

## 2026-11-11 todo-sync 纳入 ArgoCD + 桌面自动对账（通过）

### ArgoCD（集群实测）

1. 新增 `deploy/argocd/application-todo-sync.yaml`（Application `todo-sync-prod`，project `virgo`，路径 `apps/sync-server/deploy/chart/todo-sync`，目标 `todo` ns），`virgo` AppProject 放开 `todo` 命名空间与 PersistentVolumeClaim / NetworkPolicy 两类资源。
2. `values-prod.yaml` 的 `api.image.tag` 先钉到当时在跑的 `6cd8c5c@sha256:6e6566…`，避免纳管瞬间换镜像。
3. `make sync-argo-apply` 后：`Synced Healthy`，纳管 7 个资源（Deployment / Service / PVC / 2×HTTPRoute / CiliumNetworkPolicy），PVC `todo-sync-data` 保留、数据未动。
4. 纳管的首次同步触发了一次 rev 7→8 滚动（chart 渲染与 helm 手工部署的注解有差），镜像 digest 不变；`kubectl rollout status` 通过，单副本先杀后起（RWO）。
5. 服务实测：`/healthz` 200；`Authorization: Bearer` 后 `/articles` 200；`/stream` 200 长连接；无 token 的 `/stream` 401。

### 桌面自动对账

6. 新增 `apps/desktop/src-tauri/src/sync_watch.rs`：常驻线程保持 `/stream`，连上先全量对账（补推离线期间写入），之后每个 `data:` 事件触发一次；1s 内事件合并；断线 5s→10s→20s→40s→60s 退避。命令 `reading_sync_watch_start/stop/status`，事件 `reading://link` `reading://sync-start` `reading://synced` `reading://sync-error`。
7. 前端 `lib/readingSync.ts` + `ReadingView`：有配置时 App 启动即拉起常驻连接，面板显示「自动同步已连接 / 离线，Ns 后重试」，保留「立即对账」与「断开自动同步」。
8. 单元测试：`backoff_doubles_until_the_cap`、`only_sse_data_lines_trigger_a_sync` 通过；`cargo test -p todo-desktop` 全绿。
9. 门禁：`make check-quick` 绿（rust fmt+clippy+test / web build+静态站）。
10. 未在本次自动化的部分：桌面 GUI 内实际观察到「服务器变更 → 列表自动刷新」需要 App 运行，留作人工验收第 3 条。
