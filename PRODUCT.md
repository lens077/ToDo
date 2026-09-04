# Product

<!-- impeccable:product-schema 1 -->

> 来源说明：本文件由仓库文档（`README.md`、`CONTEXT.md`、`DECISIONS.md`、`docs/desktop-mvp-spec.md`、`docs/onboarding-spec.md`）与 2026-09-03 的界面需求整理而成，尚未经产品负责人逐条确认。标注「推断」的条目需要拍板；其余条目直接引自仓库。

## Platform

web

桌面端是 Tauri 2 壳内的同一份 React 构建，也以 `todo serve` 在浏览器中运行；设计语言是 web 的，不模仿 macOS/Windows 原生控件。

## Users

- **主要用户**：与 AI coding agent（Claude Code、Codex、DeepSeek Harness 等）协作开发的个人开发者或小团队。场景是在一个 Git 仓库里长时间推进工作，任务状态与代码现实会漂移，人和 agent 都可能被过期任务误导而重做已完成的工作。
- **使用姿态**：开发者在编辑器/终端与桌面端之间切换；agent 通过 CLI 与 hooks 读写同一份 `TODO.md`。
- 推断：单人使用为主，「团队空间」明确列为首版不做。

## Product Purpose

给 AI 协作开发者的本地任务对账器。每个仓库的 Project Task 保存在该仓库的 `TODO.md`，以可审计的 Git 物证辅助对账。成功的标准是 **time-to-first-value**：安装后立刻能用，第一次就感受到「任务视图与代码现实被对齐」的价值（`docs/onboarding-spec.md` §0）。

## Positioning

- 文件即事实源：SQLite 只是派生投影，可随时从 Markdown 重建；回写只修改工具识别的字节区间，未识别内容逐字节保留。
- 对账只报告、不擅自改写完成状态；Evidence 证明代码改动存在，不单独证明任务完成。
- 差异化在对账引擎，而不是又一个看板（`DECISIONS.md` 决策②）。

## Operating Context

- 本地模式不需要登录，也不需要网络。语义对账默认关闭，需自备 LLM provider 并显式启用。
- CLI（`todo`）是标准接口；桌面端与 Web 端是同一事实源的视图和编辑入口。
- 项目任务随仓库管理；个人 Capture 以 `~/.todo/inbox.md` 为事实源，不进入 Git。桌面端记住上次打开的项目于 `~/.todo/desktop.json`。
- 每次写入建立还原点（`.todo/history/`）；整文件改写必须先看 diff 再确认。

## Capabilities and Constraints

- 术语见 `CONTEXT.md`：Project Task、Capture、Triage、Evidence、Reconciliation Finding。避免用 todo item / card / ticket / note / sync / error 等替代词。
- 格式规范 `docs/format-spec-v1.md`：列由 frontmatter `columns[]` 定义，`type: done` 是唯一列类型；`◐` 部分完成、`⚠` 失败、`↳` 说明、`🔗from..to` 物证是文件内的既有 token，界面按原样显示。
- 完成策略 `normal | strict`：严格模式下完成必须留说明并决定物证；部分完成与失败在任何策略下都必须留说明。
- 桌面端首页「目标」把推进中、待办、已完成放在一个工作区；推进中默认展开 1 项、最多 5 项；已完成栏与待办区可收起；部分完成与失败在卡片内记录，不弹对话框（2026-09-03 需求）。
- 首版不做：多仓聚合、周/月回顾、泳道与依赖图、富文本随记、账号与云同步、Tauri 移动端。
- 未决：许可证与商业模型（`DECISIONS.md` 决策②④）；首次运行 onboarding 方案（决策⑤）；活跃任务上限承诺值（建议 200）。

## Brand Commitments

- 名称：产品叫 **ToDo**，桌面端窗口标题 `Todo`，界面品牌标记 `FIELD / TODO · LOCAL WORK MEMORY`。
- 视觉方向已在 `docs/desktop-mvp-spec.md` §2 约定为「工程笔记与编辑台」：暖纸色底、墨蓝黑正文、钴蓝强调、朱红警示、苔绿完成；Bricolage Grotesque Variable 与 JetBrains Mono Variable；不用紫色主视觉、不默认暗色、不用 Inter/Roboto/Arial/系统字体。
- 语言：界面与文档为简体中文；ID、日期、物证、快捷键用等宽字体。

## Evidence on Hand

- 真实任务数据：本仓库自身的 `TODO.md`（用 todo 管理本项目后续开发）。
- 验证记录与性能实测：`docs/verification-log.md`；人工验收步骤：`docs/manual-acceptance.md`。
- 选型调研：`research/`。
- 没有用户评价、案例或客户 logo；不得虚构。

## Product Principles

1. 文件是事实，界面是视图：任何界面状态都能从 `TODO.md` 重建，写入失败绝不保留虚假的乐观状态。
2. 只报告，不代替判断：对账、扫描、导入都先预览再确认，完成状态永远由人决定。
3. 记录进度是工作的一部分，不是打断：部分完成与失败要在原地一行写完。
4. 先看到价值，再学习：不做向导与巡览，靠真实数据说话。
5. 可审计优先于漂亮：短 ID、物证区间、还原点始终可见可查。
