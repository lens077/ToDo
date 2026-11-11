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
- [ ] 产出可分发的桌面端 release 构建 <!--t:d3l8hn--> #desktop #release !3 ^9000 ◐80
      ↳ release workflow、打包脚本与发布文档已完成；待真实 Developer ID 签名与 Apple 公证凭据验收 <!--n:d3l8hn-->
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
- [x] 清理 sync-server 测试脏数据（example.com/*、/s/quick-1..3、clip-a/b、dbg-one、rust-lang/cargo） <!--t:t4gfbx--> #sync #backend !2
- [x] Android APK 重打并真机复验同步 v2（进度/取消/热同步） <!--t:0x16fl--> #mobile #sync !2
      ↳ 2026-11-12 三端复验通过：手机首次同步拉 14 条（含进度）、PC 重启 dev 后热同步零变更；服务器/PC/手机 URL 集合 diff 为空。手机 3 条 failed 系服务器端 wc=0 的 3 条按设计转本机退避解析（save_pulled 第 4 分支），非缺陷。「取消」按钮未单独验证 <!--n:0x16fl-->
- [ ] 定位 make check-quick 耗时超标（实测 245s，目标 <120s） <!--t:xfayi6--> #perf #ci !3
      ↳ 2026-11-12 实测：check-quick 245s（rust fmt+clippy+workspace test+contract check 约 190s，desktop web build + 静态站点约 40s）。目标 <120s；待定位耗时来源（疑似 clippy 全量 + 未命中增量缓存的 web 构建），不阻塞 onboarding。 <!--n:xfayi6-->

## Doing
- [ ] 人工验收看板任务写回 TODO.md <!--t:h4m9zc--> #desktop #manual !1 ^3000
- [ ] 用 todo 自身管理本项目后续开发 <!--t:1pevv2--> #process !2 🔗2e112404a57360d0d054543cbbdff4ace9862afd..9142b5b6dfed7ee34d7991c60a882c398496a43a
- [x] sync-server B1：crates/sync-server API+鉴权+SSE+单测（规格见 docs/sync-server-spec.md） <!--t:5uhqjl--> #sync #backend !1
- [x] sync-web C1：网页端小前端 MVP——快速保存/列表搜索/阅读详情/SSE 近实时（部署走 virgo-sync 静态托管） <!--t:kot9n4--> #sync #frontend !1

## Done
