# Product

<!-- impeccable:product-schema 1 -->

> 来源说明：本文件由仓库文档（`README.md`、`CONTEXT.md`、`DECISIONS.md`、`docs/desktop-mvp-spec.md`、`docs/priority-quadrant-spec.md`、`docs/onboarding-spec.md`、`docs/macos-release.md`）与代码现状整理而成。2026-09-06 产品负责人确认：本文件是 CLI、桌面端、Web 端共享的唯一产品记录（`apps/desktop` 继承此文件，不单独建档）；「Users」按原样确认，「Platform」按其答复修正；`DECISIONS.md` 五项决策全部仍未拍板，本文件只如实记录未决，不替它们下结论。其余条目直接引自仓库；标注「待裁定」的条目等产品负责人决定。

## Platform

web

- 桌面端是 Tauri 2 壳（bundle `productName` `Todo`，标识 `dev.todo.desktop`，主窗口 1180×780，另有预创建的隐藏速记窗口）内的同一份 React 构建；同一构建也由 `todo serve --web-root` 在浏览器中运行，并带 PWA manifest（`apps/desktop/public/manifest.webmanifest`，`display: standalone`，描述文案「本地优先的 Markdown 任务看板」）。字段取 `web` 的原因：渲染的是 HTML/CSS，Impeccable 的 web 工具链（live、检测器、audit）适用。
- **绑定约束（2026-09-06，产品负责人）：桌面端遵循 macOS 原生惯例。** 原话记录，不在此展开。
- **已裁定（2026-09-06）：** 产品负责人确认改为遵循 macOS 惯例，替换 `docs/desktop-mvp-spec.md` §2 原「工程笔记与编辑台」视觉方向（其中明确禁止系统字体，并未禁止原生控件）。旧外观只作证据与反参照；本轮选择「市政简报」`civic-bureau-prospectus-late-modern`，新的视觉权威为 `apps/desktop/DESIGN.md`，覆盖整个应用（四个视图、对话框、菜单）。
- 移动端不是当前平台：`todo serve --read-only` 是「在手机上看」的既定路径（所有写入端点返回 403，前端探测到只读后直接拒绝写操作）；Tauri 移动端列为首版不做，形态见 `DECISIONS.md` 决策③（未决）。

## Users

- **主要用户**：与 AI coding agent（Claude Code、Codex、DeepSeek Harness 等）协作开发的个人开发者或小团队。场景是在一个 Git 仓库里长时间推进工作，任务状态与代码现实会漂移，人和 agent 都可能被过期任务误导而重做已完成的工作。
- **使用姿态**：开发者在编辑器/终端与桌面端之间切换；agent 通过 CLI 与 hooks 读写同一份 `TODO.md`。
- **首版按单人使用设计**（2026-09-06 确认）；「团队空间」明确列为首版不做。
- 典型的高频时刻（引自规格）：会话开始前或 agent 批量新增一批任务之后，用约 30 秒决定「哪些是 `!1`、哪些先放着」（`docs/priority-quadrant-spec.md` §1）；一项工作告一段落时，在原地一行写下结局（完成 / 部分完成 / 失败）。

## Product Purpose

给 AI 协作开发者的本地任务对账器。项目目录内各 Task Source 保存各自的 Project Task，界面汇总展示并写回原文件，以可审计的 Git 物证辅助对账。成功的标准是 **time-to-first-value**：安装后立刻能用，第一次就感受到「任务视图与代码现实被对齐」的价值（`docs/onboarding-spec.md` §0）。

## Positioning

- 文件即事实源：SQLite 只是派生投影，可随时从 Markdown 重建；回写只修改工具识别的字节区间，未识别内容逐字节保留。
- 对账只报告、不擅自改写完成状态；Evidence 证明代码改动存在，不单独证明任务完成。
- 差异化在对账引擎，而不是又一个看板（`DECISIONS.md` 决策②）。语义对账是 BYOK：用户自带 key，请求不经过我方服务器。

## Operating Context

- 本地模式不需要登录，也不需要网络。语义对账默认关闭，需自备 LLM provider（`TODO_LLM_PROVIDER` / `TODO_LLM_API_KEY`）并显式 `--semantic` 启用。
- CLI（`todo`）是标准接口；单文件命令继续操作当前目录 `TODO.md`。桌面端选择项目目录，`todo serve` 绑定启动目录，二者聚合发现目录内任务来源，并按来源写回。同一份 React 构建跑在三处：Tauri 桌面壳、浏览器（对接 `todo serve`，默认只绑回环地址）、无后端 mock（演示与 e2e 用）；浏览器演示不会获得本机目录访问权限。
- 项目任务随仓库管理；个人 Capture 以 `~/.todo/inbox.md` 为事实源，不进入 Git，也不受项目还原点保护。桌面端记住上次打开的项目于 `~/.todo/desktop.json`，项目移走或无法解析时静默回到「选择项目」状态。
- 每次写入建立还原点（`.todo/history/`，默认保留 200 个）；整文件改写（扫描导入、版本恢复）必须先看左右 diff 再确认，预览锁定文件指纹，期间文件被改动则拒绝写入。
- agent 接入：`todo hooks install --agent claude|codex|deepseek-harness|all` 写本地 agent 配置；`todo align` 产出全仓对齐提示词并校验回复，`--review` 永不写文件。
- 发布：macOS 正式发布由 `.github/workflows/release.yml` 构建、Developer ID 签名、Apple 公证（`docs/macos-release.md`），无未签名降级路径；**仓库当前没有可用的发布证书与公证凭据**，本地 `make package-macos` 只产出 `local-unnotarized` 验收包，不得分发。可分发的桌面端 release 尚未产出（`TODO.md` `d3l8hn`）。
- 开发入口：`make dev`（Tauri + Vite，端口 1420）；纯前端 `pnpm --filter @todo/desktop dev` 走 mock 桥接；Web 端 `pnpm build && todo serve --web-root apps/desktop/dist`（端口 3017）。

## Capabilities and Constraints

- 术语见 `CONTEXT.md`：Project Root、Task Source、Project Task、Adoption、Capture、Triage、Evidence、Reconciliation Finding、Today View。避免用 todo item / card / ticket / note / sync / error 等替代词。
- 项目目录递归发现大小写不敏感的 `TODO.md`、`TODO`、`TODO.txt`，以及 frontmatter 声明了 `todo-spec` 的任意 `.md` 文件（拆分清单的显式加入方式），显示来源相对路径；任务身份由来源路径与文件内 ID 共同确定。普通文本和未接管清单只读预览，检查 diff 并确认后才接管；源码注释 TODO 不自动导入。忽略规则、大小限制、来源隔离与 API 详见 `docs/project-root-spec.md`。
- 来源错误隔离，不因根目录缺少单个 `TODO.md` 而拒绝打开目录。新增和修改必须定位来源；跨来源移动不提供伪事务。历史、扫描、对齐审阅需选来源。HTTP 不开放个人 Capture 存储；该视图提供不可用说明，桌面版保留个人随记与按来源分拣。
- 格式规范 `docs/format-spec-v1.md`：列由 frontmatter `columns[]` 定义，`type: done` 是唯一列类型；`!1`–`!4` 优先级（中文档位名 最高 / 高 / 中 / 低，2026-09-04 拍板）、`◐` 部分完成、`⚠` 失败、`↳` 说明、`🔗from..to` 物证是文件内的既有 token，界面按原样显示；稳定 ID 是 HTML 注释里的随机 base36。
- 完成策略 `normal | strict`：严格模式下完成必须留说明并决定物证；部分完成与失败在任何策略下都必须留说明。完成状态、说明与物证是一次复合原子写入，只建立一个还原点。
- 桌面端四个一级视图（`apps/desktop/src/components/Sidebar.tsx`）：
  - **目标**：推进中 / 待办 / 已完成在同一工作区；推进中默认展开 1 项、最多 5 项，其余排队可「提到最前」；已完成栏与待办区可收起成细条；逾期 / 今天 / 7 天内只筛选、不复制任务。
  - **象限**：按 `!` 档位 2×2 定级，无 `!` 的任务留在底部「未定级」托盘；拖入格子即写回该行的 `!` token；不写 `^`、不整格上色、不做自动定级；有子任务时每次让用户选「仅修改这一项」或「同时修改 N 个子任务」，批量修改在一次 Rust 原子写入中完成（`docs/priority-quadrant-spec.md`，已实现）。
  - **随记**：个人 Capture 收件箱与分拣；速记窗口 `Cmd+Shift+Space`（macOS）/ `Ctrl+Shift+Space`（其他），`Cmd/Ctrl+Enter` 保存，`Esc` 隐藏。
  - **扫描与历史**：全仓扫描未跟踪工作、导入 diff 预览、查看与恢复还原点。
- 任务操作：部分完成与失败在卡片内一行写完，不弹对话框；普通模式勾选完成默认先一步确认（可在「设置」关闭）；跨列拖放与右键菜单「移动到…」把任务放到目标列顶部并同步勾选状态（移入 `type: done` 列即完成，移回活动列即恢复未完成，父任务带整个子树）；右键菜单「优先级…」子菜单（快捷键 `1`–`4`、`0` 清除）在所有视图可用；严格模式或带 `◐`/`⚠` 的任务进入完成列时仍填写结局说明。拖放只发送 intent，Rust 写入成功并返回新 Snapshot 后界面才落位；冲突时恢复最新 Snapshot 并提示「文件已在外部更新，请重试」。
- CLI 对等：`todo priority <id> <1-4|--clear> [--subtree]` 与桌面端共用写入函数，agent 也能定级。
- 本机偏好不进文件：完成确认开关、模型提供方配置、推进中展开数与收起状态存 localStorage（`todo.app-settings.v1`、`todo.provider-settings.v1`、`todo.goal-layout.v1`）。
- 首版不做：多仓聚合、周/月回顾、泳道与依赖图、富文本随记与自动 AI 分拣、账号与云同步、团队空间、MCP、Tauri 移动端。
- **未决（2026-09-06 确认全部仍未拍板，见 `DECISIONS.md` 决策看板）**：① 活跃任务上限承诺值（建议 200）与详情溢出语法；② 许可证与开源边界（当前「保留一切权利」占位，见 `NOTICE.md`）；③ 移动端形态（P0 推送 → P1 PWA/薄壳 → P2 spike）；④ 商业模型（买断 / Obsidian 式 / 组合，`docs/business-model-options.md`，`TODO.md` `aiw1hs`）；⑤ 首次运行 onboarding（不做向导、`todo scan` 作首次价值证明、对账延迟揭示，`docs/onboarding-spec.md`，`TODO.md` `jrnlos`）。拍板前，后续设计不得在界面或文案里虚构价格、许可、同步服务等承诺。

## Brand Commitments

- 名称：产品叫 **ToDo**，桌面端窗口标题与 bundle `productName` 为 `Todo`，界面品牌标记 `FIELD / TODO · LOCAL WORK MEMORY`（缩写标记 `FT`）。
- **视觉承诺（2026-09-06 产品负责人确认）：** 桌面端遵循 macOS 原生惯例；外观跟随系统，浅色与深色两套都做；工艺标尺是 Linear 与 Raycast 的 Mac 版（键盘优先、命令面板、紧凑密度）。本文件不记录视觉配方；系统化记录归 `apps/desktop/DESIGN.md`。
- 已作废的旧约定（仅作证据与反参照）：`docs/desktop-mvp-spec.md` §2「工程笔记与编辑台」——暖纸色底、墨蓝黑正文、钴蓝强调、Bricolage Grotesque 与 JetBrains Mono、「不用系统字体、不默认暗色」。现有界面截图存于 `apps/desktop/.impeccable/review/incumbent/`。
- 语言：界面与文档为简体中文；ID、日期、物证、快捷键用等宽字体。
- 语气（引自现有文案）：直接、说明性、不做营销修辞；错误与只读状态给出具体原因和下一步（「文件已在外部更新，请重试」「只读浏览」）。

## Evidence on Hand

- 真实任务数据：本仓库自身的 `TODO.md`（用 todo 管理本项目后续开发；2026-09-04 时 12 项未完成，分布 `!1`×4、`!2`×7、`!3`×2）。
- 规格与决策：`docs/desktop-mvp-spec.md`（桌面端 MVP）、`docs/priority-quadrant-spec.md`（象限，含 §12 已拍板项）、`docs/onboarding-spec.md`、`docs/business-model-options.md`、`docs/macos-release.md`、`docs/format-spec-v1.md`、`docs/adr/`。
- 验证记录与性能实测：`docs/verification-log.md`；人工验收步骤：`docs/manual-acceptance.md`；Playwright 回归 `apps/desktop/e2e/*.spec.ts`（completion、mobile、quadrant、review、task-movement、web）。
- 选型调研：`research/`、`调研文档/`。
- 品牌资产：`apps/desktop/icon/`（图标源与渲染脚本）、`apps/desktop/public/icon-*.png`、`apps/desktop/src-tauri/icons/`。
- 没有用户评价、案例、客户 logo、价格或许可证；不得虚构。没有可分发的签名版本。

## Product Principles

1. 文件是事实，界面是视图：项目任务视图从各 Task Source 重建，写入失败绝不保留虚假的乐观状态；个人随记和本机偏好有各自存储，不由任务文件重建。
2. 只报告，不代替判断：对账、扫描、导入都先预览再确认，完成状态永远由人决定。
3. 记录进度是工作的一部分，不是打断：部分完成、失败、定级都要在原地一步写完。
4. 先看到价值，再学习：不做向导与巡览，靠真实数据说话。
5. 可审计优先于漂亮：短 ID、物证区间、还原点始终可见可查。

## Accessibility & Inclusion

- 规格要求全部操作可用键盘完成：右键菜单 `Shift+F10` / Menu 键、方向键、Enter、`Esc`；象限视图 `Tab` 在格与托盘间移动；触屏用卡片上可见的菜单按钮完成同样操作（`docs/desktop-mvp-spec.md` §3、`docs/priority-quadrant-spec.md` §6.2）。
- 状态变化通过 `aria-live="polite"` 播报（开始 / 成功 / 失败三句式）。
- 响应式下限：< 720px 时象限四格纵向堆叠并关闭拖动，改档只走菜单与快捷键；`mobile.spec.ts` 覆盖。
- 动效只表达状态变化，禁止无意义的持续浮动与全局弹跳（§2）。尚未确立正式的 WCAG 等级承诺。
