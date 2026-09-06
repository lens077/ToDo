---
version: 1
slug: "src-app-tsx"
primary_target: "src/App.tsx"
related_targets: ["src/style.css","src/components/GoalView.tsx","src/components/Sidebar.tsx","src/components/CommandBar.tsx","src/components/TaskCard.tsx","src/components/EmptyState.tsx","src/components/QuadrantView.tsx","src/components/CaptureView.tsx","src/components/ReviewView.tsx"]
---

# 桌面端主界面（整应用）— 表面简报

## Scope & mode

- 范围：Tauri 桌面端 / Web 端共用的整份 React 界面——侧栏、工具栏、四个一级视图（目标 / 象限 / 随记 / 扫描与历史）、所有对话框与菜单、速记窗口。
- 访客模式：**Operate**。可扫读、状态清晰、macOS 惯例与键盘操作优先于表现力。
- 本轮性质：替换视觉世界（redesign）。产品事实、文案、功能、ARIA 结构、e2e 依赖的类名全部保留；旧「工程笔记与编辑台」只作反参照。

## Audience, job, constraints

- 用户：与 AI coding agent 协作的个人开发者，在编辑器 / 终端与本应用间切换；高频动作是看「现在该做什么」、拖动改列、在原地一行写结局、给一批新任务定级。
- 用户钉住的约束（2026-09-06）：遵循 macOS 惯例；外观跟随系统（浅色 + 深色）；工艺标尺 Linear / Raycast（Mac 版）。
- 品牌承诺：名称 ToDo；品牌标记 `FIELD / TODO · LOCAL WORK MEMORY`（FT）；界面简体中文；ID / 日期 / 物证 / 快捷键用等宽字体。
- 不得虚构：价格、许可、同步服务、用户评价。
- 必须保留：`.goal-region--*`、`.task-card--focus`、`.move-menu`、`.workspace__content`、`.sidebar`、`.review-*` 等类名与全部 ARIA 名称；Playwright `pnpm e2e` 全绿。

## Chosen direction

- 决策页第 1 轮重掷手中，用户选择被判为 declined 的目录挑战者 **「市政简报」**（`civic-bureau-prospectus-late-modern`），强行采用。裁决只供参考；此世界独占页面。
- 记忆点：骨白纸面上唯一的深色漆面对象是「正在推进」的任务卡；一条青→丁香→紫的丝带在标题右侧空域随光标轻轻弯曲。
- 与钉住约束的碰撞翻译：世界的「几何无衬线」→ 系统字体 SF Pro（macOS 惯例），保留「一族两字重（400/500）」纪律；世界的单一浅色 → 同一世界的夜版（墨蓝地、骨白字、丝带发光）以跟随系统；世界的营销页拓扑 → macOS 源列表侧栏 + 工具栏 + 分带工作区，丝带与漆面对象作为材料保留。

## Direction contract

THESIS: 一份市政简报做成的工作台——骨白纸面、发丝分带、唯一的深色漆面对象是正在推进的任务。拒绝品类默认的「深色侧栏 + 纸底网格 + 彩色徽标看板」。

OWN-WORLD: 地 #F7F7FA→#FFFFFF，字 #1B1547 永不纯黑，紫 #6B5CE7 只给主操作 / 选中 / 进度 / 今天，危险文字浅色 #C93E33、深色 #F3857B；失败实心操作固定 #C93E33 配白字；青 #7FD8D0→丁香 #C9B8F0→紫 #5B4FD6 丝带是唯一多彩物；发丝 #E6E7EE 分带，面板 12px、按钮与输入 8px，以发丝分带，漆面焦点、浮层和对话框使用柔和偏移阴影；SF Pro 400/500 两字重，SF Mono 给 ID / 物证 / 快捷键；深色 = 夜版：地 #0F0D22，字 #ECEAF7，紫提亮 #8C80F0；焦点卡保持深色（#25213F→#211D38→#1E1A33），以略亮面板、细边框、浅色文字和少量紫色维持层级，不使用整块白底。实际令牌与跨组件规则以 `apps/desktop/DESIGN.md` 为准。

STORY: 打开即看见一件深色漆面卡——当下的工作；其余是纸上分带的白卡清单。拖动即改列，结局在原地一行写完，写入成功卡片才落位。

FIRST VIEWPORT: 1180×780。左 216px 骨白源列表（品牌标记、项目切换、四视图、同步状态、设置）。顶 56px 白色工具栏：项目 · 快速新增输入 + 实心紫「添加」· 模型 / 刷新。工作区标题「目标」28px/500，副行紫色计数；丝带在标题右侧空域低透明度掠过，随光标弯曲 12px。其下：推进中带（漆面深色焦点卡，操作按钮在卡内）→ 待办带（各列并排白卡，发丝边）；右侧 20% 已完成栏，可收起成细条。

FORM: 目录挑战者「市政简报」，第 1 轮重掷手，用户强行采用；seed key 34e0386a。签名交互：丝带随光标弯曲；项目载入时工作区一次 24px / 0.8s 上升。

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance.

## Unresolved

- 象限、随记、扫描与历史三个视图在同一世界内的具体构图由构建阶段决定，须与目标视图共享卡片、按钮、分带语法。
- 速记窗口（520×180）沿用同一令牌，不放丝带。
