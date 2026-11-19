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

进度只在这里做摘要；逐条任务见 `TODO.md`，逐阶段验证证据见 `docs/verification-log.md`。换机/交接入口见 `docs/handoff-2026-11-18.md`。

- Workspace / Project Profile / Knowledge / Session / Overview（蓝图 Phase 1–7）已实现、通过测试，并在 M0-B 按模块提交（6 个提交 / 51 文件，2026-11-07）后 push 到 `origin/main`；审计结论见 `docs/audits/2026-11-03-audit.md`。
- M0 收口已完成：完整门禁恢复，`git status` 曾清零，本地与 `origin/main` 同步（M0-A 2026-11-06、M0-B 2026-11-07、D-M0-2 push 2026-11-07）。
- 桌面端 release 已上线：2026-11-19 决策⑦拍板不等签名公证，v0.1.0 未签名构建随官网 `/downloads/` 三端公开分发（`TODO.md` `d3l8hn` 已关闭）；签名路径回开条件见 `TODO.md` `b1gxqf`。
- 赞助与收款已上线：支付宝生产网关全链路 2026-11-12 实测通过（`TODO.md` `3nd6da` 已关闭）；剩余赞助屏文案、致谢区、隐私政策与爱发电入口见 `TODO.md` `aiw1hs` / `7gi5ls`。
- 2026-11-13：阅读功能两项落地——微信内联样式保留（readabilityrs `clean_styles(false)` + ammonia 声明级 style 过滤，存量文章需重新解析）与另存为 HTML/Markdown（目录选择器 + `img/` 图片目录 + 标题文件名，任务2）；自动门禁全绿，真机验收待办（`TODO.md` `wst7k4` / `sv2md9`）。
- 2026-11-14：现代 Web 特性优化（分支 `modern-web-optimization`）——桌面端三处 Enter 提交加 IME 组字防护（isComposing + keyCode 229，覆盖 WebKit 乱序 bug），站点 hero stagger 换 sibling-index() 带 @supports 回退；tsc / vite build / e2e 162 通过全绿，详见 `docs/verification-log.md` 2026-11-14（`TODO.md` `mg31je`）。
- 2026-11-15：阅读交互三处落地——剪贴板读取可在设置中关闭并明示用途（commit `87cb1f3`，无独立任务，见 `docs/verification-log.md`）、笔记卡动作区统一（`4818259`）、窄屏「标记与笔记」改贴底抽屉（`TODO.md` `zj3fjt`，`d9c95a0`）；tsc / vite build / Playwright（chromium + mobile）全绿。
- 2026-11-19：阅读另存为补右键入口——阅读页正文与列表卡片右键均可弹「另存为 HTML / Markdown」（`ReaderContextMenu`，放宽呼吸感 + 入场动画），导出逻辑提为 `useArticleExport` 共用，结果提示条收起面板也可见；sv2md9 收尾，真机验收清单见 `docs/verification-log.md`（`TODO.md` `sv2md9` / `wst7k4`）。
- 2026-11-19：v0.1.0 未签名先行上线（决策⑦）——官网状态条 / hero / 下载区改版（zh/en），产物入库 `web/public/downloads/` 并随三端部署；`package-macos.sh` 新增 `RELEASE_QUALIFIER`；Cargo.lock 对齐 tauri 2.12.7（修复 tauri build 版本失配）；watch 测试簇间歇失败另立 `onfv6x`（reading_target 已加沉降排空）。验证：verify-quick 绿（27–32s 多次）、CF Pages 横幅与下载 200、EO 下载 200；详见 `docs/verification-log.md` 2026-11-19。
- Trae 部署与依赖升级（2026-11-18 立项 `ovy2ij` / `e9zhyn`，各 ◐80）：代码已随 `084944f` / `0cbdb99` 入库（`.idea/`、`.zcode/` 已加入 `.gitignore`）；剩余分别是「在 Trae 会话终端实测 `make trae-deploy`」与「升级后跑完整 `make check-quick` + e2e 验无回归」，未实测前不算完成。
