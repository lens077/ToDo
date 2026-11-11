# 交接文档：onboarding 实施（进行中）+ 本日已收尾工作

> 日期：2026-11-12
> 关联：`docs/onboarding-spec.md`（§7 已更新为拍板结论）、`DECISIONS.md` 决策⑤（已拍板）、
> `docs/verification-log.md`（2026-11-12 三条目：同步 v2 三端复验、脏数据清理、支付宝切生产）
> 状态：**onboarding 实施进行到一半，工作区未提交**；此前所有工作已提交并推送双远端

## 1. 一句话现状

支付宝切生产、同步 v2 三端闭环、脏数据清理均已完结并落档；onboarding 按 2026-11-12 拍板
结论实施中：**CLI 三处已完成并人工验收，桌面端改了 6 个文件（未编译验证），向导组件、
对账揭示、agent 提示尚未开始**。

## 2. 本日已完结（全部已提交推送，commit 见 git log）

1. **同步 v2 收尾**：并发优化（11.7s→9.15s）、脏数据清理 9 条（服务器/PC/手机三端 URL
   集合 diff 为空）、APK 重装、三端复验通过（TODO：t4gfbx、0x16fl 已关闭）。
2. **支付宝切生产**（3nd6da 关闭）：正式凭据入 OpenBao（PKCS#8→PKCS#1 转换+配对校验）、
   helm bootstrap + ArgoCD Synced Healthy、公网入口两层修复（Pangolin 手工路由 +
   NetworkPolicy→CiliumNetworkPolicy）、¥1 真实支付验签/金额/trade_no/账本全绿、
   官网三端 sponsor-api meta 已注入、支付按钮已上线。
3. **Makefile 修复**：`sponsor-k8s-deploy` 去掉镜像重建依赖（digest tag 可用）；核实
   GitLab 镜像由 deploy.yml gitops job 自动完成（GITLAB_TOKEN 已配置），无需改 CI。
4. **`.env` 已删除**（凭据在 OpenBao，应用公钥可从开放平台查看）。
5. **onboarding 拍板**（jrnlos 关闭，commit 395bbb5）：见 §3。

## 3. onboarding 拍板结论（实施依据）

1. **引导方向**：默认不打扰（无开机向导）；**设置的显眼位置放「引导向导」按钮**，点击
   进入传统分步向导，默认不启用、可随时跳过。
2. **首次价值证明**：主推 `todo scan`（CLI 提示第二位、桌面首屏主行动）。
3. **对账引擎揭示**：延迟到首条 finding，第一句强调「只报告，不改写」。
4. **agent 接入提示**：完成 3 个任务后一次性提示。

规格：`docs/onboarding-spec.md` §2–§4 是实施清单；§0 反目标 1 已按拍板改写。

## 4. 已完成的实施改动（**未提交，7 个文件**）

### CLI（已完成 + 人工验收 ✓，cargo test -p todo-cli -p todo-core 全绿）

| 文件 | 改动 |
|---|---|
| `crates/todo-cli/src/main.rs` | 新增 `NEXT_STEPS_HINT` 常量；`init` 成功输出追加「下一步」（只进人类可读输出，`--json` envelope 不变）；空清单 `list` 打印同一段提示（§2.3「用户在哪里迷路，就在哪里给路标」） |
| `crates/todo-core/src/error.rs` | `FileAlreadyExists` 文案改为含「接管现有清单并补齐稳定 ID（会先建还原点）」 |

人工验收输出（temp 目录实测）：init 提示、空 list 提示、重复 init 的 adopt 提示、
`--json` 稳定性——全部符合规格 §2.1–§2.3。

### 桌面端（代码已写，**尚未 cargo check / tsc 验证**）

| 文件 | 改动 |
|---|---|
| `apps/desktop/src-tauri/src/lib.rs` | 新增 `create_todo_md` 命令（显式创建 TODO.md：目录已有则报错引导 `--adopt`；模板 `TODO_MD_TEMPLATE` 与 CLI `todo init` 逐字一致）+ generate_handler 注册（`lib.rs:769` 附近） |
| `apps/desktop/src/lib/desktop.ts` | 新增 `chooseAndCreateProject()`（目录对话框标题明示「将写入 TODO.md」→ `create_todo_md`） |
| `apps/desktop/src/hooks/useDesktop.ts` | 新增 `createProject` 回调（镜像 `chooseProject` 的 epoch/context 模式，失败留在 no-project 态由 banner 说明）；已在 model 返回对象导出 |
| `apps/desktop/src/components/EmptyState.tsx` | 次级动作「还没有 TODO.md？」展开说明（推荐先 `todo scan` / `todo init`；桌面端创建需显式确认、写明将创建哪个文件）+「选择目录并创建 TODO.md」按钮 |
| `apps/desktop/src/components/GoalView.tsx` | 接口加 `onOpenScan?`；无任务空态加主行动「扫描这个仓库」（§3.2）；今日筛选空文案改「给任务加上截止日期，它们会出现在这里。」 |

## 5. 当前计划（剩余步骤，按序执行）

1. **编译验证**：`cargo check -p todo-desktop` + `pnpm --filter @todo/desktop tsc --noEmit`（web build），先让已完成改动变绿。
2. **OnboardingWizard 组件**（新建 `apps/desktop/src/components/OnboardingWizard.tsx`）：
   传统分步向导，约 5 步——① 欢迎与本地优先一句话 ② 项目（有 snapshot 显示当前路径，
   无则调 `chooseAndOpenProject`）③ 扫描（说明 + 跳 review 视图，避免重复 scan 逻辑）
   ④ agent 接入（展示 `todo hooks install`，说明自动检测 claude/codex 等）⑤ 对账
   （「只报告，不改写」+ BYOK「请求不经过我们的服务器」）。进度「第 n 步，共 5 步」，
   上一步/下一步/跳过。**默认不启用**：只从设置按钮打开，无需持久化状态。
3. **SettingsDialog**：顶部（`settings-body` 第一个 group 前）加显眼 section「首次使用？」
   +「打开引导向导」按钮 → App 传 `onOpenWizard`（App.tsx `:508` 附近已有
   ProviderSettings 的打开先例，同样模式加 wizard 状态）。
4. **App.tsx 接线**：EmptyState 传 `onCreateTodoMd={model.createProject}`；GoalView 传
   `onOpenScan={() => setActiveView('review')}`。
5. **对账首次揭示**（§3.3）：`ContractView`（App.tsx:363 渲染，目前只有 `readOnly`
   prop）加 `hasCompletedTask` prop（App 里 `model.snapshot.tasks.some(t => t.completed)`）；
   findings ≥1 且已完成 ≥1 任务且 localStorage 旗标未设 → 视图内一次性说明条
   （「对账发现 n 处不一致……只报告，不改写你的完成状态」），关闭写
   `todo.onboarding.reconcile-revealed.v1`（localStorage 先例：`lib/clipboardLinks.ts`、
   `lib/sourceEnrollment.ts`）。
6. **agent 接入提示**（§4）：完成 ≥3 任务（`model.snapshot.tasks.filter(t => t.completed).length`）
   且 localStorage 旗标未设 → App.tsx 在 CommandBar 下方渲染一次性提示条
   「让 agent 自动读到这份清单？」+ `todo hooks install` 命令展示，关闭写
   `todo.onboarding.agent-hint.v1`。⚠️ 不得展示 `todo hooks preview` 输出（可能含 API key）。
7. **门禁**：`make check-quick`（rust fmt+clippy+test / tsc / web build）。
8. **提交**：`feat(onboarding): …` 一个 commit（代码）+ docs 更新（verification-log 条目、
   onboarding-spec 验收清单勾选状态按实际）→ 推 origin + gitlab。
9. **人工验收**（需要用户）：`pnpm --filter @todo/desktop tauri dev`——改 Rust 需重启；
   验收 §6 清单（≤3 命令、≤2 点击、零非预期写盘、第二次运行不再出现提示）。

## 6. 实施注意（不要重踩）

- **快照/竞态模式**：useDesktop 的新回调必须照 `chooseProject` 的 `contextSequence` /
  `snapshotEpoch` / `withBusy('project')` 三件套写，否则项目切换竞态会把旧快照写回来。
- **一次性状态放 localStorage**，不放用户仓库（规格 §5.1；先例 `clipboardLinks.ts:16`）。
- **模板一致性**：`TODO_MD_TEMPLATE`（lib.rs）与 CLI `TODO_TEMPLATE`（todo-cli main.rs:27）
  必须逐字相同；改其一要同步另一。
- **CommandBar 不是命令面板**（是顶栏），无命令注册机制——agent 提示做成独立提示条，
  不要试图加「命令」。
- **ContractView 目前无 snapshot prop**，加 props 时 App.tsx:363 处同步传。
- 桌面改 Rust 后 `tauri dev` 需重启（HMR 只覆盖前端）。
- 推送惯例：origin（GitHub）与 gitlab（ArgoCD 源）双推；CI 的 gitops job 会自动镜像
  main 到 GitLab，若 push 遇 non-fast-forward 先 `git fetch origin && git rebase origin/main`
  （bot 回写提交 `chore(site): release … [skip ci]` 是正常现象）。
- 门禁一律 `make check-quick`（约 1-4 分钟，rust+web 全绿才算过）。

## 7. 常用命令

```bash
cargo check -p todo-desktop            # Rust 侧快速验证
pnpm --filter @todo/desktop exec tsc --noEmit   # 前端类型
make check-quick                       # 完整快速门禁
pnpm --filter @todo/desktop tauri dev  # 桌面联调（改 Rust 需重启）
cargo run -q -p todo-cli -- contract check      # Virgo contract
```
