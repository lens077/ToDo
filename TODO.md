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
      ↳ 评估已完成：约 10000 任务或常驻进程需要投影；当前 serve 与桌面端仍只调用内存 project_document，尚未打开 Projection 数据库 <!--n:b6r1vy-->
- [ ] 产出可分发的桌面端 release 构建 <!--t:d3l8hn--> #desktop #release !3 ^9000 ◐80
      ↳ release workflow、打包脚本与发布文档已完成；待真实 Developer ID 签名与 Apple 公证凭据验收 <!--n:d3l8hn-->
- [ ] 落地「目前免费 + 赞助」：赞助屏与收款入口 <!--t:aiw1hs--> #product #site !1
      ↳ 2026-10-26 已定：下载与使用免费、功能不阉割、不做授权校验与试用；赞助 ¥1/¥6/¥18/¥30/¥68/¥88 + 自定义，国内支付宝与爱发电，国际 Ko-fi/GitHub Sponsors 只入计划不实施；回报仅致谢与自愿名单。原买断方案（$19/¥88、半价续费、12 个月窗口、30 天试用、联网校验、停服保障）全部作废，代码里从未实现，无需回退。待办：赞助屏文案（见 docs/site-copy.md 第 5 屏）、爱发电与支付宝入口、致谢区、隐私政策（只覆盖 Umami 统计） <!--n:aiw1hs-->
- [ ] 拍板首次运行 onboarding 方案（见 docs/onboarding-spec.md） <!--t:jrnlos--> #product #onboarding !2
      ↳ 待定（4）：不做向导方向、todo scan 作为首次价值证明、对账揭示时机、agent 接入提示阈值 <!--n:jrnlos-->

## Doing
- [ ] 人工验收看板任务写回 TODO.md <!--t:h4m9zc--> #desktop #manual !1 ^3000
- [ ] 用 todo 自身管理本项目后续开发 <!--t:1pevv2--> #process !2 🔗2e112404a57360d0d054543cbbdff4ace9862afd..9142b5b6dfed7ee34d7991c60a882c398496a43a

## Done
