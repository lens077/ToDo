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

- [x] init 与 adopt 改用原子创建，消除 TOCTOU 窗口 <!--t:kq3n8w--> #core #safety !1 ^1000 🔗08171d57b008cf32d33455f9b91efc61b84c3c7c..8673510ea9dea7ff61b7a7f948c0f5c9f5a57a21
- [ ] 人工验收原生文件选择器打开项目 <!--t:p7v2ld--> #desktop #manual !1 ^2000
- [ ] 人工验收外部编辑后界面刷新 <!--t:w8s5rt--> #desktop #manual !1 ^4000
- [ ] 移除 research 文档中的本机绝对路径 <!--t:c2j6xb--> #docs !2 ^5000
- [x] 为 CLI 增加 release 构建与安装说明 <!--t:n9f4pk--> #cli #docs !2 ^6000
      ↳ 0a84a18 已交付 CLI release 构建、源码安装说明与 macOS 发布文档 <!--n:n9f4pk-->
- [ ] hooks context 改为跨平台进程树终止 <!--t:z5t7gq--> #cli #agents !2 ^7000
- [ ] 按既定触发条件让 todo serve 与桌面端使用 SQLite 投影 <!--t:b6r1vy--> #cli #perf !3 ^8000
      ↳ 评估已完成：约 10000 任务或常驻进程需要投影；当前 serve 与桌面端仍只调用内存 project_document，尚未打开 Projection 数据库 <!--n:b6r1vy-->
- [ ] 产出可分发的桌面端 release 构建 <!--t:d3l8hn--> #desktop #release !3 ^9000 ◐80
      ↳ release workflow、打包脚本与发布文档已完成；待真实 Developer ID 签名与 Apple 公证凭据验收 <!--n:d3l8hn-->
- [x] 补前端 Playwright 交互层回归并纳入 CI <!--t:3zvv9u--> #desktop #test !2 🔗15ef0bb0a98a9c8c2db3ec80b9fe50e194b92c34..b45e0740b0211d5cb71f1f24a865102ae8ec7be0
      ↳ 交互层 98 项通过、web 全链路 14 项已接入 CI 两个 job；2026-09-04 的 1654316 误将本项改回未完成（完成说明行被保留即为误覆盖佐证），此处恢复 <!--n:3zvv9u-->
- [ ] 拍板商业模型剩余决策：涨价时机、边界条款、授权服务、隐私政策与收款渠道 <!--t:aiw1hs--> #product #decision !1
      ↳ 组合双轨、价格、续费、试用、更新窗口与联网校验均已定；剩余项以 DECISIONS.md 决策④为准 <!--n:aiw1hs-->
- [ ] 拍板首次运行 onboarding 方案（见 docs/onboarding-spec.md） <!--t:jrnlos--> #product #onboarding !2
- [ ] progress 标记无法清除：--note --clear 不清 ◐，progress 0 被拒，帮助文本承诺的 unset 无法表达 <!--t:krr4qq--> #cli #bug !2
- [x] web.spec.ts 串行共享真实 TODO.md 无逐例重置，前序写入导致 quadrant 分组用例失败并中断其余 11 项 <!--t:1bcg1t--> #desktop #test #bug !2 🔗8e51f852281dcb7b27df2a29707853e3149d923c..ad1dd82e79f1f49c52df8ad60edad9fda5125945
      ↳ 实因非串行状态泄漏：多来源落地后任务标识变为 [来源,ID] 复合键，web 规格仍按裸 ID 选择；且快速新增/扫描/历史在聚合浏览下禁用。已改为经 helper 构造复合键并显式选择来源，14/14 通过 <!--n:1bcg1t-->

## Doing
- [ ] 人工验收看板任务写回 TODO.md <!--t:h4m9zc--> #desktop #manual !1 ^3000
- [ ] 用 todo 自身管理本项目后续开发 <!--t:1pevv2--> #process !2 🔗2e112404a57360d0d054543cbbdff4ace9862afd..9142b5b6dfed7ee34d7991c60a882c398496a43a
- [x] 配置任务列：保真预览、确认写入与 ecommerce 实战适配 <!--t:57wlfi--> !1
      ↳ 已实现配置任务列入口、默认/现有标题选择、保真预览与指纹确认；Rust门禁、126项前端、15项真实服务、37项WebKit通过；ecommerce真实副本验收原文件SHA未变，边界见docs/verification-log.md <!--n:57wlfi-->
- [x] 未归列任务可直接拖入推进中，并在无列时一次确认创建列和推进 <!--t:angi74--> !1
      ↳ 已支持直接拖入/开始处理；无列时合并预览后一个历史点原子配置并推进，保留完整Markdown列表项；Rust门禁、132项前端、43项WebKit、15项真实服务及ecommerce副本拖放通过 <!--n:angi74-->

## Done
