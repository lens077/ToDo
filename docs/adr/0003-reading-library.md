# 阅读库是个人级本地归档，不是项目任务源

「阅读」把任意 http/https 链接抓成可读正文并保存到 `~/.todo/reading/`（SQLite + 图片文件），与随记、AI 会话一样属于用户个人，不跟随项目，也不写入任何项目文件。之后不再请求原站：用户圈选、写笔记、记录进度的都是这份本地副本，所以锚点不会漂移，原文删除也能继续读。

抽取用 `readabilityrs`（Mozilla Readability 移植，同时产出 Markdown），抓取在 Rust 端完成以绕开跨域与 Referer 限制；微信文章的 `data-src` 懒加载与 `visibility:hidden` 在送入抽取前就地补丁，`mmbiz.qpic.cn` 图片带页面 Referer 下载后落盘，正文里的 `src` 改写为相对路径，桌面端通过 Tauri asset 协议（scope 仅 `$HOME/.todo/reading/**`）读取。正文经 `ammonia` 白名单清洗后才进入 webview。

圈选标记采用 `@recogito/text-annotator` 的 W3C 文本锚点（`quote` + 字符偏移），偏移失效时回退到引文查找。阅读位置沿用 Readeck / Karakeep 的语义：同时存百分比与锚点（视口顶部第一个块的序号），恢复时锚点优先、百分比兜底；0 与 100 不存锚点，100 即已读且不回退，手动「标为未读」保留位置。进度只在离开阅读器、窗口失焦或滚动停顿后写入，程序性滚动不算阅读。

`todo serve` 的 HTTP 传输暂不开放阅读库；浏览器构建用内存 mock 覆盖交互层测试。调研与设计稿见 `../design/reading/`。
