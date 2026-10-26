---
todo-spec: 1
columns:
  - {id: archived, name: Archived, type: done}
---

# Archived project tasks

## Archived
- [x] init 与 adopt 改用原子创建，消除 TOCTOU 窗口 <!--t:kq3n8w--> #core #safety !1 ^1000 🔗08171d57b008cf32d33455f9b91efc61b84c3c7c..8673510ea9dea7ff61b7a7f948c0f5c9f5a57a21
- [x] 为 CLI 增加 release 构建与安装说明 <!--t:n9f4pk--> #cli #docs !2 ^6000 🔗2e112404a57360d0d054543cbbdff4ace9862afd..0a84a18c54fa4856c9cceefe1ca7e26f4108e130
      ↳ 0a84a18 已交付 CLI release 构建、源码安装说明与 macOS 发布文档 <!--n:n9f4pk-->
- [x] 补前端 Playwright 交互层回归并纳入 CI <!--t:3zvv9u--> #desktop #test !2 🔗15ef0bb0a98a9c8c2db3ec80b9fe50e194b92c34..b45e0740b0211d5cb71f1f24a865102ae8ec7be0
      ↳ 交互层 98 项通过、web 全链路 14 项已接入 CI 两个 job；2026-09-04 的 1654316 误将本项改回未完成（完成说明行被保留即为误覆盖佐证），此处恢复 <!--n:3zvv9u-->
- [x] web.spec.ts 串行共享真实 TODO.md 无逐例重置，前序写入导致 quadrant 分组用例失败并中断其余 11 项 <!--t:1bcg1t--> #desktop #test #bug !2 🔗8e51f852281dcb7b27df2a29707853e3149d923c..ad1dd82e79f1f49c52df8ad60edad9fda5125945
      ↳ 实因非串行状态泄漏：多来源落地后任务标识变为 [来源,ID] 复合键，web 规格仍按裸 ID 选择；且快速新增/扫描/历史在聚合浏览下禁用。已改为经 helper 构造复合键并显式选择来源，14/14 通过 <!--n:1bcg1t-->
- [x] 配置任务列：保真预览、确认写入与 ecommerce 实战适配 <!--t:57wlfi--> !1 🔗21084d2e501bfb2d71c1d9634767297e11f89760..0ba1514fa431bdc4334086440b4fe11d730ecf1a
      ↳ 已实现配置任务列入口、默认/现有标题选择、保真预览与指纹确认；Rust门禁、126项前端、15项真实服务、37项WebKit通过；ecommerce真实副本验收原文件SHA未变，边界见docs/verification-log.md <!--n:57wlfi-->
- [x] 未归列任务可直接拖入推进中，并在无列时一次确认创建列和推进 <!--t:angi74--> !1 🔗21084d2e501bfb2d71c1d9634767297e11f89760..ec771b67bb4bb29ad6ebb96f50b6ccd661be8e14
      ↳ 已支持直接拖入/开始处理；无列时合并预览后一个历史点原子配置并推进，保留完整Markdown列表项；Rust门禁、132项前端、43项WebKit、15项真实服务及ecommerce副本拖放通过 <!--n:angi74-->
- [x] 移除 research 文档中的本机绝对路径 <!--t:c2j6xb--> #docs !2 ^5000 🔗2997c42963d8d3be02a02303d7be0dabd925de50..6f39be1bd6ddb4e50ce2ad0eacc3438c7beaeb57
      ↳ README、DSH 桥、规格、调研文档与验收清单的本机绝对路径已替换为占位路径 <!--n:c2j6xb-->
- [x] hooks context 改为跨平台进程树终止 <!--t:z5t7gq--> #cli #agents !2 ^7000 🔗820fa14b50979e4d3a3fcbc272814c96deba222c..faab1760e045fc89f414ba24b8047cef1167bc6f
      ↳ hooks 子进程进入独立进程组；Unix killpg、Windows taskkill /T，Unix 后代终止回归通过 <!--n:z5t7gq-->
- [x] progress 标记无法清除：--note --clear 不清 ◐，progress 0 被拒，帮助文本承诺的 unset 无法表达 <!--t:krr4qq--> #cli #bug !2 🔗820fa14b50979e4d3a3fcbc272814c96deba222c..faab1760e045fc89f414ba24b8047cef1167bc6f
      ↳ progress --clear 删除 ◐，不再用 0/100 伪造状态；百分比与 --clear 互斥，严格策略下清除不要求说明 <!--n:krr4qq-->
