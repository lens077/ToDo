---
todo-spec: 1
columns:
  - {id: todo, name: Todo}
  - {id: doing, name: Doing}
  - {id: done, name: Done, type: done}
---

# Project tasks

本文件由本工具自身管理。修改任务状态请用 `todo` 命令，不要手改这些行。
说明段落、标题和本段文字都会被工具原样保留。

已完成的历史工作记录在 `docs/verification-log.md`，不重复列在这里。

## Todo

- [ ] 人工验收原生文件选择器打开项目 <!--t:p7v2ld--> #desktop #manual !1 ^2000
- [ ] 人工验收外部编辑后界面刷新 <!--t:w8s5rt--> #desktop #manual !1 ^4000
- [ ] 按既定触发条件让 todo serve 与桌面端使用 SQLite 投影 <!--t:b6r1vy--> #cli #perf !3 ^8000
      ↳ 2026-11-07 决策⑥：benchmark_projection_10k_tasks 实测（release）parse 59ms / 投影全量重建 28ms / 增量更新 132ms；内存快照以 10 倍余量满足交互阈值，暂不接入。重开触发：活跃任务 >20k、常驻 serve 轮询 CPU 热点、快照 p95 >500ms。详见 DECISIONS.md 决策⑥ <!--n:b6r1vy-->
- [x] 产出可分发的桌面端 release 构建 <!--t:d3l8hn--> #desktop #release !3 ^9000 🔗9142b5b6dfed7ee34d7991c60a882c398496a43a..f771581925deb34870a4e4092cb9819373474a04
      ↳ 2026-11-19 决策⑦拍板：不等签名公证，ad-hoc 构建直接公开分发。v0.1.0（app.zip 9.1M + CLI 4.9M + SHA256SUMS）随官网 /downloads/ 上线（CF Pages 已验证 200）。签名路径移至新任务 b1gxqf <!--n:d3l8hn-->
- [ ] 落地「目前免费 + 赞助」：赞助屏与收款入口 <!--t:aiw1hs--> #product #site !1
      ↳ 2026-10-26 已定：下载与使用免费、功能不阉割、不做授权校验与试用；赞助 ¥1/¥6/¥18/¥30/¥68/¥88 + 自定义，国际 Ko-fi/GitHub Sponsors 只入计划不实施。2026-11-12 支付宝入口已上线（生产网关 + ¥1 端到端实测，6 档位按钮三端生效，见 verification-log）。剩：赞助屏文案打磨（docs/site-copy.md 第 5 屏）、致谢区、隐私政策（只覆盖 Umami 统计）、爱发电入口（7gi5ls，前置已满足） <!--n:aiw1hs-->
- [x] 拍板首次运行 onboarding 方案（见 docs/onboarding-spec.md） <!--t:jrnlos--> #product #onboarding !2
      ↳ 2026-11-12 拍板：默认无向导 + 设置显眼位置放可选「引导向导」按钮（点击进入传统向导，默认不启用）；主推 todo scan；对账延迟到首条 finding；agent 提示阈值 3 个任务。详见 DECISIONS.md 决策⑤与 docs/onboarding-spec.md §7 <!--n:jrnlos-->
- [ ] 接入爱发电赞助入口（支付宝完成后再做） <!--t:7gi5ls--> #product #site !2
- [x] 签约通过后切生产：切网关 + 回写镜像 tag + make sponsor-k8s-deploy + 生产实测异步通知（等用户指令执行） <!--t:3nd6da--> #sponsor #alipay #release !2
      ↳ 2026-11-12 全链路完成：正式凭据入 OpenBao（PKCS#8→PKCS#1 转换+配对校验）、helm bootstrap + ArgoCD Synced Healthy、公网 healthz 200 sandbox:false、¥1 真实支付验签/金额/trade_no/账本全绿、官网三端 sponsor-api meta 已注入。退款无 API（产品决策保持最小），测试款商家平台手动退。详见 docs/verification-log.md 2026-11-12 条目 <!--n:3nd6da-->
- [ ] 调研 twitter-text 库在阅读功能中的用途 <!--t:6h0jqz--> #research #reading !3
      ↳ 已拉取 github.com/sayrer/twitter-text 到 ./twitter-text/（未进 git）。结论：它是推文正文文本处理库（长度计算、提取 @/#/URL、autolink、校验），不解决链接解析本身；X 状态链接解析已由 syndication 方案实现（见 reading.rs）。候选增强：用它的 autolinker/extractor 把保存的推文正文里的 @提及、#话题、链接渲染成可点击元素，且 unicode/emoji 边界处理比正则严谨；代价是 cxx（C++ 工具链）+ pest 依赖较重。先试轻量正则，不够再上此库。 <!--n:6h0jqz-->
- [x] 移动端底部导航栏：默认5项+固定「我的」+3~7自定义+拖拽排序（规格见 docs/design/mobile-bottom-nav-spec.md） <!--t:c947n8--> #desktop #mobile !2
      ↳ M1-M4 全部完成并真机验收：默认 5 槽、3-7 槽可配、长按拖拽排序、更多视图 9 项可达；6 槽标签改单行省略，见 docs/verification-log.md <!--n:c947n8-->
- [x] sync-server B2：k8s 部署（镜像+helm+Secret），等 B1 本地跑通后执行 <!--t:yfj0ol--> #sync #release !2
- [x] 链接库 C 层：桌面同步模块 + 网页端独立小前端（待 B2） <!--t:u9lj4a--> #sync #desktop !2
- [x] desktop-sync C2：桌面阅读库与 todo-sync 服务器双向对账（全量对账 + SSE 触发） <!--t:dj9w2g--> #sync #desktop !2
- [x] Android 分享接收真机验收（装机后 am start 模拟 SEND intent，验服务器收到） <!--t:51swhk--> #mobile #manual !2
      ↳ 2026-11-11 真机验收通过：V2425A 分享唯一 URL 后服务器 /articles 4→5，见 docs/verification-log.md <!--n:51swhk-->
- [x] todo-sync 纳入 ArgoCD + 桌面自动对账（SSE/离线重试） <!--t:se88bl--> #sync #backend #desktop
      ↳ todo-sync 已由 ArgoCD todo-sync-prod 纳管（Synced/Healthy）；桌面端改为 SSE 常驻自动对账 + 5→60s 退避，离线期间写入在重连首跑补推；见 docs/verification-log.md <!--n:se88bl-->
- [x] 阅读：秒回入列 + 后台解析状态 + 剪贴板导入 <!--t:n0fz2h--> #sync #desktop
      ↳ 粘贴链接秒回入列，正文后台解析（解析中/失败/重试），剪贴板单条/多条提示并按内容指纹去重；真机验收见 docs/verification-log.md <!--n:n0fz2h-->
- [x] 阅读：修复多链接粘贴与无正文文章解析 <!--t:3571gu--> #sync #desktop
      ↳ 根因是多行粘贴折行拼成畸形 URL；已拆条入队并在两端拒绝脏链接，无正文文章改为仅链接收藏，失败原因落库并显示；真机三条微信链接均 ok，见 docs/verification-log.md <!--n:3571gu-->
- [x] 阅读：解析保留微信内联样式（卡片/装饰标题不再丢失） <!--t:wst7k4--> #desktop #reading !2
      ↳ 根因是 readabilityrs clean_styles 默认删光 style 属性 + ammonia 白名单不放行；微信文章改为 clean_styles(false)，sanitize_html 放行 style 并做声明级过滤（url()/expression()/position:fixed 等整条丢弃）；存量文章点「重新解析」即按新管线重生。cargo test 40 通过，check-quick 绿 <!--n:wst7k4-->
- [x] 阅读：另存为 HTML / Markdown（任务2） <!--t:sv2md9--> #desktop #reading !2
      ↳ 阅读上下文面板新增「另存为 HTML / Markdown」：系统目录选择器选目录，正文存为「标题.html/.md」（重名自动 -2 序号），图片复制/尽力下载进 img/ 子目录并重写引用；Markdown 优先用已存 markdown，缺失时 htmd 现转。命令 reading_export_article，单测覆盖文件名净化/图片落地/远程兜底保留，check-quick 绿。2026-11-19 补右键入口（阅读页正文 + 列表卡片，ReaderContextMenu 复用任务菜单骨架并放宽呼吸感）+ 导出结果提示条（面板收起也可见），导出逻辑提为 useArticleExport 共用 <!--n:sv2md9-->
- [x] 清理 sync-server 测试脏数据（example.com/*、/s/quick-1..3、clip-a/b、dbg-one、rust-lang/cargo） <!--t:t4gfbx--> #sync #backend !2
- [x] Android APK 重打并真机复验同步 v2（进度/取消/热同步） <!--t:0x16fl--> #mobile #sync !2
      ↳ 2026-11-12 三端复验通过：手机首次同步拉 14 条（含进度）、PC 重启 dev 后热同步零变更；服务器/PC/手机 URL 集合 diff 为空。手机 3 条 failed 系服务器端 wc=0 的 3 条按设计转本机退避解析（save_pulled 第 4 分支），非缺陷。「取消」按钮未单独验证 <!--n:0x16fl-->
- [ ] 定位 make check-quick 耗时超标（实测 245s，目标 <120s） <!--t:xfayi6--> #perf #ci !3
      ↳ 2026-11-12 实测：check-quick 245s（rust fmt+clippy+workspace test+contract check 约 190s，desktop web build + 静态站点约 40s）。目标 <120s；待定位耗时来源（疑似 clippy 全量 + 未命中增量缓存的 web 构建），不阻塞 onboarding。 <!--n:xfayi6-->
- [x] 现代 Web 特性优化：IME 回车防护 + 站点 stagger 动画现代化 <!--t:mg31je--> #desktop #frontend !2
      ↳ 2026-11-14 modern-web-guidance 扫描驱动：知识搜索/会话目标输入加 isComposing+keyCode229 防护（WebKit bug 165004 乱序场景）、ConfirmCompleteDialog window 级 Enter 加防护、web 站点 hero stagger 改 sibling-index()（@supports 回退）。验证：tsc 绿、desktop build 绿、e2e 162 通过 0 失败、legacy-scan web 清零、desktop 余 1 条为 TaskCard 文章级快捷键（指南 dropWhen 排除的误报）。已合入 main 并推送（commits 945ae8b/e1c29d5，rebase 前为 a55de99/7d454c4），见 docs/verification-log.md 2026-11-14 <!--n:mg31je-->
- [x] 阅读：窄屏（≤1000px）上下布局下「标记与笔记」只剩标题且无法展开 <!--t:zj3fjt--> #desktop #reading !2
      ↳ 已改为贴底抽屉（方案 B）。≤1000px：.reader__columns 只剩一行，正文独占高度；.reader-notes 变 position:absolute 贴底的抽屉（max-height 72%、圆角、shadow-dialog、z-index 3），收起时 translateY 移出可视区并 visibility:hidden，打开时淡入 scrim（z-index 2）+ 0.28s 弹入；标题右侧新增只在窄列渲染的关闭按钮，右下角常驻药丸开关（highlighter 图标 + 「标记与笔记」+ 琥珀计数），Escape 收起抽屉且编辑中的草稿不被取消。组件用 matchMedia('(max-width:1000px)') 决定是否渲染抽屉外壳，宽列仍是常驻右栏、不出现开关。实测 1000/760/390：抽屉开合后笔记卡完整（189px 高，卡 103px 全在框内），正文高度 600~670px 不再被挤。验证：tsc 绿、vite build 绿、playwright chromium 155 passed/16 skipped（新增「narrow widths fold the notes into a bottom sheet」覆盖开关/抽屉可见/Escape/关闭按钮）、mobile 11 passed；浅色与深色两种外观均截图确认。已知遗留：抽屉不能拖拽调整高度；抽屉打开时正文被 scrim 遮住，不能边读边写。 <!--n:zj3fjt-->
- [ ] 接入 Developer ID 签名与公证并把站点下载区切回签名产物（决策⑦回开） <!--t:b1gxqf--> #desktop #release !3
      ↳ Apple Developer 会员 + .p12 + App Store Connect API key 到位后，走 docs/macos-release.md「创建正式发布」签名工作流；站点下载区切签名产物、下架未签名包（决策⑦回开条件） <!--n:b1gxqf-->
- [ ] 修复 watch 测试簇 FSEvents 延迟投递导致的间歇性失败（verify-quick 红/绿漂移） <!--t:onfv6x--> #desktop #test !2
      ↳ 根因：macOS FSEvents 把建立期写入事件延迟批量重放，污染 250ms 静默断言，失败在 reading_target_without_a_write_does_not_reload 与 parent_watch_survives_two_atomic_replacements 等同族测试间游走；notify 依赖未变，疑与 2026-11-19 nightly 切换相关。reading_target 已加沉降排空（本日提交），其余同族测试待同样处理 <!--n:onfv6x-->

## Doing
- [ ] 人工验收看板任务写回 TODO.md <!--t:h4m9zc--> #desktop #manual !1 ^3000
- [ ] 用 todo 自身管理本项目后续开发 <!--t:1pevv2--> #process !2 🔗2e112404a57360d0d054543cbbdff4ace9862afd..9142b5b6dfed7ee34d7991c60a882c398496a43a
- [x] sync-server B1：crates/sync-server API+鉴权+SSE+单测（规格见 docs/sync-server-spec.md） <!--t:5uhqjl--> #sync #backend !1
- [x] sync-web C1：网页端小前端 MVP——快速保存/列表搜索/阅读详情/SSE 近实时（部署走 virgo-sync 静态托管） <!--t:kot9n4--> #sync #frontend !1
- [ ] Trae 部署平台接入：Makefile trae-deploy + scripts/deploy-site-trae.sh（发布 web/dist 静态站） <!--t:ovy2ij--> #site #deploy !3 ◐80
      ↳ 2026-11-18 立项并随本轮提交入库：Makefile 新增 trae-deploy 与 TRAE_* 变量、新增 scripts/deploy-site-trae.sh（权限预检/站点按名复用/禁并发/临时目录上传/等终态读地址），发布 web/dist 静态站。代码就绪；剩余：在 Trae 会话终端真实跑一次 make trae-deploy（普通 shell 预检会被拦）。 <!--n:ovy2ij-->
- [ ] 多包依赖升级：Tauri 2.12.5 / vite 8.3.8 / express 5.3 / Playwright 1.65.1 等 <!--t:e9zhyn--> #deps #ci !3 ◐80
      ↳ 2026-11-19 补：cargo update 对齐 Rust 侧 tauri 2.12.7 / plugin-dialog 2.8.4 / plugin-global-shortcut 2.4.1（此前 tauri build 因版本失配被拦，v0.1.0 发布构建已用此组合）。verify-quick 多次通过；watch 测试簇间歇失败另立 onfv6x。剩余：完整 make check-quick + e2e <!--n:e9zhyn-->

## Done
