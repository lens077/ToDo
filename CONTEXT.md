# Developer Work Context

本产品把仓库里的可提交工作与个人临时想法分开管理，同时用代码物证持续校正任务视图，避免 agent 重做已经完成的工作。

## Language

**Project Root**:
用户选择的项目目录，是任务来源发现和项目内操作的边界；不要求它本身是 Git 仓库根。
_Avoid_: Task file, Git root

**Task Source**:
项目目录内承载任务的原始文件。不同来源各自保有任务身份、完成策略和内容；汇总视图不取代原文件。
_Avoid_: Import copy, merged task file

**Project Task**:
属于项目中某个 Task Source 的可执行工作，由该原文件记录。任务身份由来源相对路径与文件内稳定 ID 共同确定，同名 ID 不跨来源合并。
_Avoid_: Todo item, card, ticket

**Adoption**:
用户检查预览差异并明确确认，将发现的只读来源转为可管理任务文件的过程。发现文件本身不意味着同意接管。
_Avoid_: Automatic import, background migration

**Capture**:
个人快速记录的原始文本，尚未承诺执行，也尚未归属任何项目。Capture 没有完成状态。
_Avoid_: Task, memo, note

**Triage**:
人工审阅 Capture，并选择丢弃、保留或转化为某个仓库 Project Task 的过程。
_Avoid_: Sync, import, move

**Evidence**:
把 Project Task 与一段 Git commit 历史关联起来的可审计事实。Evidence 证明代码改动存在，不单独证明任务已经完成。
_Avoid_: Completion proof, status

**Reconciliation Finding**:
对 Project Task、Evidence 与当前代码状态之间不一致的报告。Finding 是提议或确定事实，不会自行改写任务完成状态。
_Avoid_: Error, automatic fix

**Today View**:
从 Project Task 的状态与截止日期派生出的当日工作视图，不拥有任务数据。
_Avoid_: Daily task list, separate inbox

## Progress

进度只在这里做摘要；逐条任务见 `TODO.md`，逐阶段验证证据见 `docs/verification-log.md`。

- Workspace / Project Profile / Knowledge / Session / Overview（蓝图 Phase 1–7）已实现并通过测试；截至 2026-11-06 成果仍在工作区待提交（现状与风险见 `docs/audits/2026-11-03-audit.md`）。
- 进行中：M0 收口——恢复完整门禁（`make check`）并把上述成果按模块提交进 Git。
- 阻塞项：可分发的桌面端 release（缺 Developer ID 签名与 Apple 公证凭据，`TODO.md` `d3l8hn`）；赞助生产切换（等支付宝签约，`TODO.md` `3nd6da`）。
- 2026-11-13：阅读功能两项落地——微信内联样式保留（readabilityrs `clean_styles(false)` + ammonia 声明级 style 过滤，存量文章需重新解析）与另存为 HTML/Markdown（目录选择器 + `img/` 图片目录 + 标题文件名，任务2）；自动门禁全绿，真机验收待办（`TODO.md` `wst7k4` / `sv2md9`）。
- 2026-11-14：现代 Web 特性优化（分支 `modern-web-optimization`）——桌面端三处 Enter 提交加 IME 组字防护（isComposing + keyCode 229，覆盖 WebKit 乱序 bug），站点 hero stagger 换 sibling-index() 带 @supports 回退；tsc / vite build / e2e 162 通过全绿，详见 `docs/verification-log.md` 2026-11-14（`TODO.md` `mg31je`）。
