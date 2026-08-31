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
