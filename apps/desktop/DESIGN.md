---
name: Virgo / 市政简报
brand: ../../../work/docs/brand/DESIGN.md
description: 骨白纸面、发丝分带与丝带线焦点组成的本地工作台，三档密度共用一套语法。
colors:
  paper: "#f7f7fa"
  panel: "#ffffff"
  ink: "#1b1547"
  virgo: "#ffb347"
  virgo-light: "#ffe08a"
  accent: "#b45309"
  accent-wash: "#fdf1dc"
  primary-fill: "#f5a524"
  primary-ink: "#1b1547"
  danger: "#c93e33"
  danger-fill: "#c93e33"
  night: "#0f0d22"
  panel-dark: "#171433"
  ink-dark: "#eceaf7"
  accent-dark: "#fbbf24"
  danger-dark: "#f3857b"
typography:
  body:
    fontFamily: "-apple-system, BlinkMacSystemFont, 'SF Pro Text', 'SF Pro Display', system-ui, 'PingFang SC', 'Hiragino Sans GB', 'Microsoft YaHei', sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.45
  identifier:
    fontFamily: "ui-monospace, 'SF Mono', Menlo, Consolas, monospace"
    fontSize: "11px"
    fontWeight: 400
    letterSpacing: "0.02em"
rounded:
  panel: "12px"
  control: "8px"
  chip: "5px"
spacing:
  card-gap: "12px"
  region-gap: "14px"
  workspace: "28px"
components:
  button-primary:
    backgroundColor: "{colors.primary-fill}"
    textColor: "{colors.primary-ink}"
    rounded: "{rounded.control}"
    padding: "0 16px"
  input:
    backgroundColor: "{colors.paper}"
    textColor: "{colors.ink}"
    rounded: "{rounded.control}"
    padding: "0 11px"
  nav-selected:
    backgroundColor: "{colors.accent-wash}"
    textColor: "{colors.accent}"
    rounded: "{rounded.control}"
    padding: "6px 9px"
  tag:
    backgroundColor: "{colors.accent-wash}"
    textColor: "{colors.accent}"
    rounded: "{rounded.chip}"
    padding: "2px 7px"
  task-card:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.ink}"
    rounded: "{rounded.panel}"
    padding: "12px 14px 12px"
---
# Design System: Virgo / 市政简报

品牌、命名、图标外壳、页脚与定价原则见希曼 Sumery 品牌系统（Libra 仓库 `docs/brand/DESIGN.md`）。本文件只定义 Virgo 的强调色、界面语言与组件。

## 品牌关联

Virgo 是 Sumery 的「对账与校准」产品：品牌母体的安静小桌面在这里变成一张市政简报式工作台，纸面、分带、丝带线和琥珀标记共同表达「把代码现实与任务记录对齐」。品牌不应把 Virgo 做成泛效率看板，也不应把 AI 写成主角；主角是 Markdown 事实源、Git 物证和用户对写入动作的控制。

后续官网或介绍页可沿用 Scorpius 的「一个具体工作场景」方法，但场景应是审阅桌、文件夹和校准线，而不是剪贴板小房间。文案保持直接、说明性、可审计：每个能力同时写清它读取什么、写回什么、不会替用户决定什么。

## 色相

Virgo 的色相是琥珀：图标里的大勾用 `#f5a524`（清单卡 + 勾尖三颗星点，`icon/icon-{light,dark}-1024.png`，浅色纸底 / 深色板岩底两版跟随系统外观，Dock 用深色版）。琥珀本身在纸面上不可读（对比度不足 4.5:1），所以界面分两层：文字、线条、选中态用 `--accent #b45309`（夜版 `#fbbf24`），填充控件用 `--primary-fill #f5a524` 配深色字 `--primary-ink #1b1547`。原「工作紫」`#6b5ce7` 已退役，下文凡说「紫」处按此映射读：文字紫 → `--accent`，紫洗底 → `--accent-wash`，实心按钮紫 → `--primary-fill`。
## Overview
**Creative North Star: "市政简报"**
一份市政简报做成的工作台：骨白纸面、发丝分带、丝带线焦点。采用用户选定的 `civic-bureau-prospectus-late-modern`；系统字体、macOS 惯例与 Linear / Raycast 的 Mac 版工艺标尺优先，不照搬营销页构图。外观跟随系统。**2026-09-07 用户裁定：推进中任务卡不再是深色漆面**（反差过大、体量过重），改为「衬色纸面」——比页面高一档的琥珀洗底、同一主墨、卡片顶边一条 2px 青→淡琥珀→琥珀丝带线；夜版为比面板略亮一档的深底，同样带丝带线。
**Key Characteristics:**
- 纸面分带、系统字体两字重；页面上没有任何深色色块。
- 推进中任务以衬色纸面与顶边丝带线区分，丝带仍是唯一多彩物。
- 三档布局密度（正常 / 中等 / 效率）共用同一语法，由本机设置切换。
范围是 Tauri / Web 共用的整个 React UI。产品事实继承根 `PRODUCT.md`，Operate 模式与页面策略留在 `.impeccable/surfaces/src-app-tsx.md`。依据完整 `src/style.css`、`Ribbon.tsx`、主要组件及最新 desktop/mobile/dark 截图抽取，不是静态稿还原。`finish-verdict.md` 的 ship 仅覆盖待办列扁平化修复，不代表整应用可访问性认证。
## Colors
Primary 为琥珀：主操作、选中、今天与部分完成。CSS `--primary-fill #f5a524` 两种外观相同，配深字 `#1b1547`；悬停浅色为 `#e3941a`、夜版为 `#fbbf24`。文字与线条用可读的 `--accent #b45309`（夜版 `#fbbf24`）。
Secondary 为语义朱红与证据青：危险文字采用 frontmatter 的浅深配对；失败实心提交固定 danger-fill。浅色成功文字 `#177a71`，夜版 `#7fd8d0`。丝带端点为 `#7fd8d0`、`#ffe08a`、`#f5a524`，焦点进度条可复用。
Neutral 为纸地、面板与主墨。次级面浅/深为 `#f1f1f6` / `#1e1a40`，次墨为 `#4b5165` / `#b4b2cc`，注释墨为 `#6c7189` / `#8b89a8`；发丝为 `#e6e7ee` / `#2a2652`，加强发丝为 `#d3d4e0` / `#3a3568`。主墨不用纯黑。复用核心令牌在 frontmatter；其余精确语义洗色与材料以 CSS 为准，不制造平行全量令牌库。
**The Semantic Fill Rule.** 危险文字与危险实心底色是两个角色；夜版提亮文字，不提亮失败按钮底色。
焦点卡令牌 `--focus-*`：浅色底 `#fff8ec → #fdf1dc`、主墨 `#1b1547`、次墨 `#6b5a3a`、发丝 `#f1dfbd`；夜版底 `#2a2340 → #241d36`、主墨 `#eceaf7`、次墨 `#cdbfa6`、发丝 `rgba(251, 191, 36, 0.22)`。## Typography
系统 sans 的 400 / 500 两字重承担内容与层级；系统 mono 承担 ID、日期、物证和快捷键，计数采用 tabular-nums。不引入外部字体，也不沿用旧系统字体禁令。
无项目入口标题为 `clamp(34px, 4.4vw, 54px)` / 500 / 1.04；常规视图标题 28px / 1.05，目标实际为 26px。焦点任务 16px / 500 / 1.4（中等 15px、效率 14px、移动 15px）；普通任务 14px / 1.45（中等 13.5px、效率 13px、移动 13.5px / 1.4），完成栏紧凑卡 13px。对话框标题 18–20px，按钮 12.5–13.5px，辅助信息 10.5–12.5px。没有统一数学字号比例。
## Layout
桌面采用 216px 源列表侧栏、最小 56px 工具栏及可滚动工作区；横边距 28px，壳最小高度 560px。目标左侧推进中/待办分带，右侧完成栏 `clamp(232px, 20%, 340px)`，收起为 44px；首行 `fit-content(62%)`。默认展开一项推进中任务，本机设置允许 1–5 项，不能把材料角色误写成硬性单卡限制。
来源栏（多来源项目）：一行放「任务来源」选择器、录入计数、可截断的聚合提示，右侧工具区放「发现 N 个新来源」药丸、「调整录入…」与「发现提示 N」折叠按钮；提示展开为发丝分隔的等宽列表，不再用原生 `details`。录入来源是本机偏好（`todo.source-enrollment.v1:<project>`），首次打开多来源项目弹出录入表；未录入的来源不进看板、不进任何选择器。
**密度**：`.app-shell[data-density]` 三档，只在 ≥720px 生效。正常为本文件默认值；中等保留全部元素，间距、标题、卡片各收一档（面板 10px 圆角、标题 22px、区域头 38px）；效率去掉标题丝带与入场上升动画、空态改左对齐、面板 8px / 控件 6px 圆角、侧栏 196px、工具栏 44px、区域头 32px、卡片 7–8px 内边距、无悬停阴影，横边距 14px。
象限为一个面板内的 2×2 发丝分格，下面接未定级托盘；随记为输入纸面加横向账本；扫描与历史用分隔列表和 diff。待办列现在只有标题、数量、间距与下方发丝，没有中间有色圆角包围层。
不超过 1000px 托盘改单列；980px 侧栏为 180px、目标丝带及工具栏项目按钮隐藏；760px 侧栏为 68px 图标栏、目标区域纵向堆叠；720px 象限纵排、菜单项目最小 44px；719px 源列表变为含文字的底部导航（最小 52px）、筛选另起一行、横边距约 14px 加 safe-area。断点不混同。短桌面高度不超过 620px 时首行改为 `fit-content(55%)`。页面最小宽度 320px，速记按 520×180 紧凑纸面设计。手机截图是响应式 Web，不是 Tauri 移动端承诺。
## Elevation & Depth
纸面常态以色阶和发丝分隔，任务交互才抬升；菜单、状态浮层与对话框有柔和偏移阴影。焦点卡用衬色纸面加柔和偏移阴影（效率密度下无阴影，只留发丝与丝带线）；夜版比面板略亮一档，保持低亮度，不整块反白。精确阴影、材料、动效与键盘焦点只在 sidecar 扩展及片段中记录。
**The Paper Bands Rule.** 区域和任务可以有边界；中间待办列只用标题、数量、间距与发丝线分带。
## Shapes
面板/任务 12px，常规输入/按钮 8px，标签 5px；小按钮 6px，菜单/内部纸面 10px，对话框 14px。药丸限日期筛选、数量及局部状态。常规边线 1px，任务勾选 1.5px；空列用虚线。图标为内联 SVG，通常 14–16px、移动底栏 19px。文件中的部分完成、失败和物证标记是数据，不当装饰删除。
## Components
按钮分实心主操作、轮廓、工具文字与失败实心提交；通常最小高度分别为 36px、36px、32px、30px。禁用主按钮转次级面与注释墨。快速新增是组合输入，模型字段最小 38px；使用标准 HTML 表单语义和自绘开关/勾选按钮，不声称它们是 AppKit 控件。
导航选中为淡紫底与紫字。任务菜单是应用管理的 240px 浮层，支持按钮、右键、键盘与触摸子菜单；辅助提示多为 title，没有独立自绘 tooltip 系统。日期筛选药丸与任务小标签不混用。
任务有 card/compact/focus/quadrant 四变体；focus 变体的操作按钮为 28px 轮廓按钮（效率 26px）；设置对话框用分段控件选密度；录入表为勾选列表，路径目录淡、文件名实，状态与任务数在右。批量接管表复用同一列表语法，宽 760px，评审阶段每行可展开成内嵌 diff（最高 32vh）。Beta 功能用 `.beta-badge`（紫洗底药丸）标注；宽松解析的邀请是虚线边框 `.loose-offer` 便签，说明只读、不改文件、可关闭。完成任务降到地色并加删除线。普通卡工具在悬停或内部键盘焦点时出现，触摸保持可见；忙碌态阻止重复操作。部分完成与失败在卡内一行提交，写入成功才关闭或落位。只读禁止写操作。
丝带仅在目标标题和无项目入口，aria-hidden，不承载信息；速记不放丝带。完整 WebGL 行为见 sidecar 描述，预览片段不冒充着色器。漆面任务焦点与键盘焦点是两个概念。
完成/设置/模型面板最大宽度为 590/560/720px，轻量确认与定级范围为 460px；取消、Escape 与现有表单语义保留，不宣称完整焦点陷阱已经独立验证。diff 以等宽、洗色加侧线区分增删；随记部分分拣成功明确提示任务已创建，避免重复创建。无项目、读取中、失败、任务为空与只读分别说明原因和下一步。
## Do's and Don'ts
- Do 保留系统字体、两字重与系统浅深外观。
- Do 让三档密度改的是间距、字号与装饰，不改层级、文案与 ARIA。
- Do 用发丝分带，保留 ID、物证、键盘入口与真实写入状态。
- Don't 恢复中间待办列的有色圆角盒，或把所有任务与象限染成重点。
- Don't 把推进中卡片做回深色漆面；焦点靠衬色纸面与丝带线，不靠反差。
- Don't 声称旧规格禁止原生控件，也不要恢复旧系统字体禁令。
- Don't 把截图或局部 ship 当作全面交互与可访问性认证。
扩展文件为 `.impeccable/design.json`（schemaVersion 2）；色阶仅供面板展示，不是新增生产令牌。此轮只写文档，不运行浏览器、服务、检测器或测试。
