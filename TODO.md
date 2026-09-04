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
- [x] 人工验收原生文件选择器打开项目 <!--t:p7v2ld--> #desktop #manual !1 ^2000
- [ ] 人工验收看板任务写回 TODO.md <!--t:h4m9zc--> #desktop #manual !1 ^3000
- [ ] 人工验收外部编辑后界面刷新 <!--t:w8s5rt--> #desktop #manual !1 ^4000
- [ ] 移除 research 文档中的本机绝对路径 <!--t:c2j6xb--> #docs !2 ^5000
- [ ] 为 CLI 增加 release 构建与安装说明 <!--t:n9f4pk--> #cli #docs !2 ^6000
- [ ] hooks context 改为跨平台进程树终止 <!--t:z5t7gq--> #cli #agents !2 ^7000
- [ ] 评估 list 接入 SQLite 投影的触发条件 <!--t:b6r1vy--> #cli #perf !3 ^8000
- [ ] 产出可分发的桌面端 release 构建 <!--t:d3l8hn--> #desktop #release !3 ^9000
- [ ] 补前端 Playwright 交互层回归并纳入 CI <!--t:3zvv9u--> #desktop #test !2 🔗15ef0bb0a98a9c8c2db3ec80b9fe50e194b92c34..b45e0740b0211d5cb71f1f24a865102ae8ec7be0
      ↳ Playwright 交互层与 web 全链路回归已纳入 CI：pnpm e2e 14 项、e2e-web.sh 5 项，ci.yml 两步骤均已接入 <!--n:3zvv9u-->
- [ ] 拍板商业模型三选项（见 docs/business-model-options.md） <!--t:aiw1hs--> #product #decision !1
- [ ] 拍板首次运行 onboarding 方案（见 docs/onboarding-spec.md） <!--t:jrnlos--> #product #onboarding !2
- [ ] progress 标记无法清除：--note --clear 不清 ◐，progress 0 被拒，帮助文本承诺的 unset 无法表达 <!--t:krr4qq--> #cli #bug !2

## Doing
- [ ] 用 todo 自身管理本项目后续开发 <!--t:1pevv2--> #process !2 🔗2e112404a57360d0d054543cbbdff4ace9862afd..9142b5b6dfed7ee34d7991c60a882c398496a43a

## Done
