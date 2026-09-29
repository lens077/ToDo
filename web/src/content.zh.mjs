// 中文页。文案出自 docs/site-copy.md（六屏骨架见 work/docs/brand/LANDING.md），
// 事实以 PRODUCT.md / CONTEXT.md 为准。改这里时同步改 content.en.mjs——
// 构建会检查两份结构一致。

const notify = "mailto:support@looke.cloud?subject=" + encodeURIComponent("Virgo 上线时通知我");

export default {
  hreflang: "zh-Hans",
  htmlLang: "zh-Hans",
  ogLocale: "zh_CN",
  langName: "中文",
  meta: {
    title: "Sumery Virgo · 任务与代码现实对账 · 给 AI 协作开发者的本地任务对账器",
    description:
      "Sumery Virgo 把项目里的 TODO.md 汇总成一个视图，用 Git 里的改动给每一项找物证，只报告差异，完成与否由你决定。本地运行，不需要登录。macOS 版即将推出。",
  },
  ui: {
    skip: "跳到正文",
    home: "Virgo 首页",
    nav: "页面导航",
    readIn: "阅读中文版",
    artCaption: "演示数据：一份 TODO.md 与它的对账结果",
  },
  brand: { kicker: "SUMERY · VIRGO", name: "Virgo", sub: "本地 · Markdown 是唯一事实源" },
  status: {
    label: "当前状态",
    text: "尚未发布。macOS 版在签名与公证收尾。留一封邮件，上线时我们回信。",
    cta: { label: "写信给我们", href: notify, event: { name: "notify", data: { from: "status" } } },
  },
  nav: [
    { href: "#how", label: "怎么用" },
    { href: "#bounds", label: "边界" },
    { href: "#views", label: "视图" },
    { href: "#get", label: "上线通知" },
  ],
  hero: {
    title: ["任务与代码，", "对一次账。"],
    lede:
      "和 agent 一起推进一个仓库，任务清单很快就不是代码的样子了：做完的没勾，勾了的其实没做完，agent 读到过期的一行，把已经完成的工作再做一遍。Virgo 把项目里的 <code>TODO.md</code> 汇总成一个视图，用 Git 里的改动给每一项找物证，只报告差异——<strong>完成与否，还是你来定。</strong>",
    actions: [
      { kind: "primary", label: "上线时通知我", href: notify, event: { name: "notify", data: { from: "hero" } } },
      { kind: "ghost", label: "看它怎么对账", href: "#how" },
    ],
    fine: "上线通知是一封邮件，不是订阅。",
  },
  how: {
    id: "how",
    title: "三十秒，把清单对回代码。",
    sub: null,
    steps: [
      {
        title: "打开项目目录。",
        body: "Virgo 找出目录里所有的 <code>TODO.md</code>（以及声明了 <code>todo-spec</code> 的 Markdown），按来源分开列出，不需要登录，也不需要网络。",
      },
      {
        title: "看对账结果。",
        body: "每一项旁边是 Git 里能找到的物证：哪些改动、哪个区间。没有物证的、物证和状态对不上的，标出来。",
      },
      {
        title: "在原地写结局。",
        body: "完成、部分完成（<code>◐</code>）、失败（<code>⚠</code>）都在那一行写完，附一句说明。写回去的只有 Virgo 认识的那几个字节，文件里其他内容逐字节不动。",
      },
      {
        title: "agent 读同一份文件。",
        body: "<code>todo hooks install --agent claude|codex|deepseek-harness</code>，它在会话开始时读到的就是你刚对过账的清单。",
      },
    ],
    note: null,
  },
  bounds: {
    id: "bounds",
    title: "它只报告，不替你改。",
    sub: null,
    items: [
      {
        title: "文件是事实，界面是视图。",
        body: "SQLite 只是派生的索引，随时可以从 Markdown 重建。删掉 Virgo，你的 <code>TODO.md</code> 还是一份普通的 Markdown，任何编辑器都能开。",
      },
      {
        title: "物证证明改动存在，不证明任务完成。",
        body: "对账结果是「这一项在 Git 里有 / 没有对应的改动」，勾不勾永远是人的决定。",
      },
      {
        title: "每次写入之前先建还原点。",
        body: "<code>.todo/history/</code> 默认保留 200 个。整文件改写（扫描导入、版本恢复）必须先看左右 diff 再确认；预览期间文件被别人改了，写入会被拒绝，提示你重试。",
      },
      {
        title: "不需要网络，不需要登录。",
        body: "本地模式打开目录就能用。语义对账默认关闭；打开它需要你自己的 LLM key，请求从你的机器直接发到你选的服务商，不经过我们的服务器。",
      },
      {
        title: "匿名使用统计可以关。",
        body: "桌面端默认发送打开了哪个视图、用了哪些功能，以及版本号和系统；不发送任务内容、文件路径或项目名。在「设置 → 使用统计」关闭。",
      },
      {
        title: "个人随记不进仓库。",
        body: "速记窗口（<kbd>⌘⇧Space</kbd>）记下的东西存在 <code>~/.todo/inbox.md</code>，不受项目还原点保护，也不会被提交。",
      },
    ],
  },
  catalog: {
    id: "views",
    title: "四个视图，一份文件。",
    sub: null,
    items: [
      { name: "目标", body: "推进中、待办、已完成在同一个工作区；推进中默认展开 1 项，最多 5 项。" },
      { name: "象限", body: "按 <code>!1</code>–<code>!4</code> 拖进四格，写回的只是那一行的 <code>!</code> 标记；没定级的留在托盘。" },
      { name: "随记", body: "个人收件箱与分拣；<kbd>⌘⇧Space</kbd> 随时记一条，<kbd>⌘⏎</kbd> 保存。" },
      { name: "扫描与历史", body: "全仓扫描未跟踪的工作，导入前看 diff；查看与恢复还原点。" },
    ],
    after:
      "<code>- [ ] 任务 !2 ◐ ↳ 说明 🔗 a1b2c3..d4e5f6 &lt;!-- id --&gt;</code>——优先级、部分完成、说明、物证、稳定 ID 都是文件里的普通字符。CLI <code>todo</code> 和桌面端读写同一套。",
  },
  get: {
    id: "get",
    title: "还没到发布那一天。",
    sub: null,
    body: [
      "macOS 版在做 Developer ID 签名与公证；签好之前不放安装包，也不放「先用着」的未签名版本。其他平台等 macOS 之后再说，现在不承诺。",
      "留一封邮件，上线那天我们回信告诉你。不需要注册，也没有邮件列表——就是一封信，正文空着也行。",
    ],
    cta: { label: "上线时通知我", href: notify, event: { name: "notify", data: { from: "get" } } },
    tableCaption: "各平台发布状态",
    columns: ["平台", "状态"],
    rows: [["macOS · Apple Silicon", "即将推出"]],
  },
  footer: {
    sub: "Sumery Virgo · 本地 · Markdown 是唯一事实源",
    note: "本页用自托管的 Umami 统计访问量与按钮点击，不设 Cookie，不记录身份。",
  },
  art: {
    file: "TODO.md",
    lines: [
      { box: "x", text: "阅读页导入浏览器页面", evidence: "2ef9e9c", flag: "ok" },
      { box: "x", text: "加固文章链接", marks: "!2", flag: "warn" },
      { box: " ", text: "标题优先级", marks: "!1 ◐", note: "规格已起草" },
      { box: " ", text: "写 DSH 桥接文档", evidence: "48ff807", flag: "warn" },
    ],
    findings: {
      title: "对账 · 2 处差异",
      items: [
        { task: "加固文章链接", text: "已勾选，Git 里没有对应改动" },
        { task: "写 DSH 桥接文档", text: "有改动 48ff807，仍未勾选" },
      ],
      foot: "只报告，不改写",
    },
  },
};
