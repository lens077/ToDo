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
- 2026-11-19：阅读另存为补右键入口——阅读页正文与列表卡片右键均可弹「另存为 HTML / Markdown」（`ReaderContextMenu`，放宽呼吸感 + 入场动画），导出逻辑提为 `useArticleExport` 共用；导出改为**每篇文章一个包裹目录**（正文 + img/ 同居，两种格式复用同目录），默认带「 - Sumery Virgo」后缀，设置→另存为命名可仅标题或自定义前后缀；提示条收起面板也可见且带「打开」「打开目录」按钮（`open_exported_file/dir` 受限打开）；设计稿见 `docs/design/export-folder-design.html`（`TODO.md` `sv2md9` / `wst7k4`）。
- 2026-11-19：v0.1.0 未签名先行上线（决策⑦）——官网状态条 / hero / 下载区改版（zh/en），产物入库 `web/public/downloads/` 并随三端部署；`package-macos.sh` 新增 `RELEASE_QUALIFIER`；Cargo.lock 对齐 tauri 2.12.7（修复 tauri build 版本失配）；watch 测试簇间歇失败另立 `onfv6x`（reading_target 已加沉降排空）。验证：verify-quick 绿（27–32s 多次）、CF Pages 横幅与下载 200、EO 下载 200；详见 `docs/verification-log.md` 2026-11-19。
- Trae 部署与依赖升级（2026-11-18 立项 `ovy2ij` / `e9zhyn`，各 ◐80）：代码已随 `084944f` / `0cbdb99` 入库（`.idea/`、`.zcode/` 已加入 `.gitignore`）；剩余分别是「在 Trae 会话终端实测 `make trae-deploy`」与「升级后跑完整 `make check-quick` + e2e 验无回归」，未实测前不算完成。
- 2026-11-20：移动端统一收件箱设计稿定稿 `docs/design/mobile-inbox-design.html`——文件关联（VIEW intent 收 text/html + markdown + text/plain，content:// 嗅探分流，MD 双问「加入阅读 / 导入为 TODO / 仅预览」，伪 URL 去重）、阅读×随记合并（articles.kind = article|note，随记事实源迁入阅读库、inbox.md 退役，统一输入条 + 编辑抽屉）、常驻状态栏通知三 action、AppWidget 小组件、同步协议 v3（note/local-file 正文上行 + note LWW）与触发矩阵。选型原则：平台触点收敛为一个自研最小 Tauri Android 插件（≤300 行 Kotlin），URI 路由用官方 deep-link 插件，其余全走依赖树既有第三方库（pulldown-cmark / readabilityrs / htmd / reqwest / rusqlite）。已立项四期任务 `a2cmrf` / `34huu8` / `fkq42v` / `t1f2f8`，均未实施。同日补真实界面底座 UI 设计稿 `docs/design/inbox-merge-ui-design.html`：mockup 全部用 style.css 真实 tokens 与组件 class 渲染（现状阅读基准帧 → 合并后收件箱：导航改词、输入条加识别胶囊与「打开文件」、chips 加文章/随记维、随记卡/文件卡混排；窄屏贴底 composer + 4+1 Tab；随记编辑贴底抽屉沿用 zj3fjt 模式；随记视图改造为收件箱随记维 + 分拣面板保留），Playwright 截图自检通过。同日应反馈（收件箱面板乱、不规范）补面板样式对比稿 `docs/design/inbox-composer-styles.html`：立七条规范基线（标题行单行、说明降级 hint、唯一实心主按钮、次级幽灵化、工具图标化、同步状态芯片化、识别内联色条），出 A 单行收敛（推荐，reading-add 同构）/ B 模式双态（分段控件，升级路径 A→B）/ C 无框极简 / D 一体面板四版，交互演示经 Playwright 验证（粘链接自动切换识别态）。定稿：用户拍板方案 A 并细化三点——识别维持前缀口径（http(s):// / www. → 链接文章走解析，其余文本 → 随记直存）、幽灵工具行图标加 ≤4 字短标签（浏览器保存/打开文件/批量导入）、PC 悬停 / 移动端长按 ≥500ms 弹完整描述气泡（复用 data-tip 悬浮模式，移动端 Pointer Events 计时挂 tip-open，不依赖 title）；B 留档为 A→B 升级路径。用户最终拍板截图 A′ 方案；主设计稿 `inbox-merge-ui-design.html` §3/§4/§7 已同步 A′ 样式（单行头部 + 同步状态芯片 + 工具行短标签 + data-tip 气泡），Playwright 交互复验通过。M1 后端层随定稿落地（任务 `a2cmrf` ◐35）：articles 表新增 `kind TEXT DEFAULT 'article'` 与 `updated_at INTEGER`（CREATE TABLE + ensure_columns 双路迁移，存量行默认文章）；随记与文章共用表，`note://<id>` 伪 URL 复用唯一键去重；`note_append`/`note_update` 以 Markdown 为事实源，pulldown-cmark（tasklist/strikethrough/tables）渲染 + 专用净化器放行任务 checkbox，标题/摘要/字数从正文派生，编辑 bump updated_at 供同步 LWW；`reading_note_append`/`reading_note_update` 命令已注册，删除复用 `reading_remove`；reading.ts 补 `noteAppend`/`noteUpdate` 与 mock。验证：todo-core 148 测试全绿（含 5 个新随记测试）、cargo check、tsc、`make check-quick` 57s 全绿。未提交；剩余 M1：ReadingView A′ composer/chips kind 维/随记卡/编辑抽屉、CaptureView 数据源切换、inbox.md 迁移命令。
- 2026-11-20（续）：M1 UI 层落地（`a2cmrf` ◐75）——ReadingView 切换为 A′ 收件箱：单行头部（标题+统计+同步状态芯片，SyncControl 加 inHeader 变体）、识别输入条（`normalizeInputUrl` 分流：http 前缀→文章、文本→`noteAppend`、多链接→批量管线，按钮文案随路由走）、工具行短标签 + `data-tip` 完整描述气泡（复用 reading-card 悬浮模式，支持键盘聚焦）、类型/状态双维 chips、随记卡（note-pad 徽章 + 已编辑标记 = updated_at≠added_at）、随记编辑贴底抽屉（Markdown 直编 + 光标处语法插入 + ⌘Enter 保存，窄屏折叠）；「打开文件」接 dialog → `reading_import_file` → `import_local_file`（html/md→文章：readability 提取失败整页净化直通；txt→随记文件名作标题；`local-file://<sha 前 16>` 伪 URL 去重回跳）。样式进 style.css 收件箱段（inbox-* 前缀 + 窄屏媒体查询）。验证：todo-core 152 测试全绿（+7 随记/导入）、tsc、e2e 全量 170 passed（reading.spec 更新选择器/文案并新增随记/抽屉/类型筛选 3 例）、`make check-quick` 60s 绿。侧栏导航改名（阅读→收件箱）与 CaptureView 数据源切换、inbox.md 迁移同属下一批次（涉及 6+ 个 e2e 导航引用，一起动）。
- 2026-11-20（续二）：M1 收尾批次完成（`a2cmrf` ◐90，待用户验收）——CaptureView/全局速记窗数据源切换：`capture_list/append/delete/triage` 命令改从 ReadingState 取随记（Capture 形状不变、desktop.ts 桥零改动），`ops.rs` 移除 CaptureStore 换 `triage_note_for` + `snapshot_value`；分拣成功后随记标 `triaged → 目标列`（saved_reason）留档不删除，分拣列表自动隐藏；`capture_migrate_inbox` 命令 + 设置→随记迁移按钮（逐条转随记、原文件改名 `inbox.md.migrated-<ts>` 留档、幂等）；侧栏导航「阅读」→「收件箱」（新增 inbox 图标）、「随记」项移出导航与移动端默认栏（normalize 静默丢弃存量 capture 项），随记分拣入口挂收件箱工具行与 HTTP 占位分支。集成测试重写：command_contract 的 capture 用例改走阅读库管线（wire 形状断言不变），新增迁移测试（含幂等）；ai_feed/project_sources/column_setup 构造器同步。验证：todo-core 152、todo-desktop 全套绿、tsc、e2e 全量 170 passed、`make check-quick` 50s 绿、`todo lint` 绿。变更 18 文件 +~1500 行未提交。
- 2026-11-20（续三）：首轮验收反馈修复（6 项，`a2cmrf` 仍 ◐90 待复验）——①收件箱输入条左缘色条 3px→1px 与其他边框同厚；②随记抽屉 showModal 后焦点显式交给正文输入区（✕ 关闭钮不再默认带焦点描边）；③分拣面板 0 列时新增行内指引「在任务来源选单个已管理文件」（不再静默禁用），并给 text-action 全局禁用样式（opacity .45，不再「看着能点点不动」）；④全局速记注册失败原因透传：Rust 侧包装「可能已被其他应用占用」，侧栏状态显示「快捷键冲突」+ 详情行；⑤移动端 Tab 栏「我的」参与等分（移除 56px 固定尾列）；⑥分拣面板删除随记补确认框。验证：tsc、e2e 170 passed、verify-quick 38s 绿。
- 2026-11-21（真机复验修复，4 项）——①**移动端图片全裂/「解析失败」根因**：assetProtocol.scope 只有 `$HOME/.todo/reading/**`，Android sandbox 无 $HOME，手机库在 `$APPDATA/.todo/reading`（files 目录）→ asset 请求全被拒；tauri.android.conf.json 覆盖 scope（$HOME + $APPDATA + $CACHE 三路），已重打包。②**深色主题正文文字不清**（PC 同样）：微信等站点行内 `color` 是浅色主题设计的深色字；深色下 `.reader-body [style*=color]:not([style*=background])` 强制 inherit 正文墨色，带背景的行内样式（徽章/高亮）保留。③**触屏 tap 粘滞 hover**：data-tip 气泡（浏览器保存描述/重新解析描述）以浅底白条常驻盖住内容——`@media (hover: none)` 全量禁显两处气泡（长按显隐归 M2）。④**rctx 工具行窄屏折行**（重新解/析、另存为/HTML）：nowrap + tools wrap + 硬编码 #fff 改主题变量。e2e 增至 178 passed；另澄清 mobile.spec 的截图日志 `desktop_status=...` 是 dev 终端 stderr 非页面 UI。新签名包：`app-universal-release-signed.apk`（19M，06:31）。
- 2026-11-21（续四）：**图片 403 的最终根因闭环**（CDP 直连手机 WebView 实测）——asset 请求路径为 `/data/user/0/<pkg>/files/.todo/reading/...`（HTTP 403）。第一层：`$APPDATA` 在 tauri Android 展开为 `app_data_dir=/data/user/0/<pkg>`（logcat 实证），**不含 /files 层**，且构建期 canonicalize 会把 macOS 上不存在的绝对路径条目静默丢弃（.so 字符串验证：`/data/user/0/...` 条目消失）。最终 scope = `$APPDATA/files/.todo/reading/**` + `$CACHE/files/.todo/reading/**`，CDP fetch **HTTP 200**、22 张图 ok:true。release-signed（19M）已装机。另确认：所有文章 parse_status=ok、word_count 与 PC 一致——「无法解析」实为图片裂图+深色暗淡的误判；诊断期间「浏览器保存」按钮已在移动端隐藏（原生 prompt 丑弹窗）。诊断方法沉淀：debug 包（debuggable）+ `adb forward tcp:9222 localabstract:webview_devtools_remote_<pid>` + Playwright connectOverCDP 直接测真机 WebView。
- 2026-11-21（续五）：二轮真机反馈三项（`a2cmrf` ◐90）——①正文页面禁止手指捏合缩放（index.html viewport 加 maximum-scale=1 + user-scalable=no，双指捏爆排版溢出）。②图片预览升级为 ImageLightbox 组件：双指捏合缩放（1–6 倍）+ 双击 1x/2x + 放大后拖动平移 + 1:1 重置按钮 + touch-action:none；「点击图片外关闭」改为设置项（设置→图片预览，默认开；关掉后手势不会误触，只能 ✕/Esc 关）。③正文长按菜单从「另存为」改为标记：reader-paper 的 onContextMenu 只 preventDefault（选区标注工具条由 annotator 流程自然弹出），**删除阅读页全部另存为逻辑**（ReaderContextMenu/reader-resume 提示条/rctx 另存为按钮/useArticleExport 引用），另存为入口收敛到列表卡右键。根因诊断小插曲：ImageLightbox 首版 setPointerCapture 会把 click target 重定向到容器导致「点图片」被判成「点遮罩」误关，已去除 capture。验证：tsc、e2e 全量 180 passed（contextmenu 断言改写 + lightbox 两分支 + lightbox 手势用例）、verify-quick 36s 绿。release-signed 已装机启动。
- 2026-11-22（续七）：底部高度根因 = MainActivity 的 `enableEdgeToEdge()`（app 内容强制延伸到系统导航栏后，Tab 栏下方留出三键区的高额空白）。移除后回到传统形态：Tab 栏贴内容底、三键区系统底色独立绘制、状态栏不再压顶栏（CDP 截图验证）。构建流程修正：release 构建（42.43s 真实编译）+ apksigner 签名 + 装机，不再用 `| tail` 管道吞 exit code（假绿坑二次踩踏）。
- 2026-11-22（续六）：横屏/滚动条批量修复（`a2cmrf` ◐90 持续）——①两条橙色横条诊断：一为阅读进度条（设计）、一为 rail 竖条在窄屏的视觉误读，CDP 扫描确认无多余横条；②**滚动条隐藏全局化**（原 719 断点限制导致横屏仍显示，验收 00:26）：reader-pane + workspace__content 全断点隐藏；③**横屏侧栏可滚动**（sidebar overflow-y auto，滚动范围 108px、到底后设置按钮完整可见——CDP 实测）；④**阅读进度竖条**：右缘 3px accent 填充随进度增长（无轨道背景），回答「文章长度指示」；⑤阅读视图高度链（`.source-content:has(> .reader)` 锁高）消除外层滚动溢出。验证：tsc、e2e 180 passed、release-signed 已装机。
- 2026-11-21：M1 e2e 自动化补齐（8 个新用例，全量 178 passed）——desktop.ts 的 capture mock 接入阅读库 mock（captureList/append/delete/triage 走同一随记数据源，分拣留档口径与 Rust 一致，mockCaptures 固定 fixture 退役）；reading.spec 新增 composer 三态识别（hint/色条/按钮文案）、类型×状态叠加过滤与空态、抽屉光标插入+Escape 丢弃+关闭、⌘Enter 保存、删除确认（拒绝/接受两路）、随记分拣闭环（含原话留档断言）、侧栏无随记项；mobile.spec 新增 Tab 栏等宽断言与窄屏抽屉贴底断言。**测试抓到真 bug**：App.tsx 的 refreshCaptures effect 依赖了每次渲染都变化的 model 字面量，进入随记视图后 setState 无限循环占满主线程（所有点击挂起）——已修依赖为稳定引用。注：随记分拣按钮的 e2e 点击统一用 dispatchEvent（真实鼠标派发在行卸载时序下挂起，真机人工点击已验证正常）。
- 2026-11-22（续八）：真机截图反馈三项修复（`TODO.md` `wovpo9`，待真机验收）——①阅读页「标记与笔记」浮标贴 tab 栏：≤719px `bottom: 8px`（含包含块与级联顺序的分析，浮标锚在 `.reader` 非 `.workspace`）；②横屏矮窗口侧栏滚不到底：`@media (min-width:720px) and (min-height:320px) and (max-height:559px)` 放开 `.app-shell` 的 520/560px 最小高度，外壳收进视口、侧栏自身滚动可达底，另补侧栏 safe-area 底部内边距（首版无 320px 下限，e2e 的 986×180 极端小窗用例点击超时——比 320 矮的窗口保留页面滚动兜底）；③Android 系统导航栏浅灰条：续七移除 `enableEdgeToEdge()` 后对比度 scrim 回到默认开启所致——`MainActivity.applySystemBarTheme()` 显式透明双系统栏 + API≥29 关 `isNavigationBarContrastEnforced` + 按 uiMode 切图标明暗（深浅主题自动同色，不硬编码色值）。验证：浏览器 mock 360×800 / 768×345 / 768×620 三组尺寸实测 + 回归，`compileArmDebugKotlin` 绿，`make check-quick` 绿。**真机验收通过（vivo V2425A / Android 17）**：深色下状态栏与三键区均为 app 背景（浅灰条消失）、横屏侧栏滚到底「设置」完整可见、阅读页浮标 gap 实测 8px、浅色主题系统栏浅色+深色图标且切换无崩溃；期间抓到并修复真机首崩——onCreate 阶段 DecorView 未建时 `getInsetsController()` 抛 NPE（vivo/Android 17），已挪到 `onResume` + `peekDecorView()` 兜底；构建需显式 ANDROID_HOME/NDK_HOME（本 shell 不读 .zshrc）。详见 `docs/verification-log.md` 2026-11-22。
