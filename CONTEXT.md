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
