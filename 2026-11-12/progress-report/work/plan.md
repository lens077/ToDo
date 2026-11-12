# 进度汇报 HTML · 素材与规划

- 类型：技术进度汇报（非幻灯片）
- 读者：项目 owner，浏览器阅读 + 打印
- 语言：中文
- 核心结论：onboarding 已落地并提交（未 push）；体积优化与构建矩阵改动已实测但未提交；人工验收与两个提交收口待决策。

## 证据来源

| 事实 | 来源 |
| --- | --- |
| onboarding 提交 7993894（15 文件，+500/−12） | `git show --stat 7993894` |
| check-quick 245s（提交时）、273s（本轮复跑） | `docs/verification-log.md`；本轮实测 |
| 体积 26.73 → 20.68 → 18.31 MiB；二进制 26,803,456 → 20,460,672 → 17,979,200 B | 本轮 `make bundle-macos` + `du -sh` / `stat` |
| 剩余构成：二进制 17.15 MiB + icon.icns 1.2 MiB；dist 1.3 MB（PWA 图标 600 KB） | 本轮拆解 |
| panic=abort 上限：`__eh_frame` 1.59 MB + `__gcc_except_tab` 0.74 MB | 本轮 `size`/段分析 |
| 构建矩阵改动 6 文件 | `git status --short` |
| 清理 gen/android/app/build：3.3 GB → 10 MB | `du -sh` |
| debug APK 491 MB / universal release APK 28 MB | `du -sh` |
| CI 实况：release.yml(macos-14)、deploy.yml(站点)、rust-check.yml；无 .gitlab-ci.yml | 目录与文件读取 |
| 交接 §5 九步 | `docs/handoff-2026-11-12-onboarding-wip.md` §5 |
| P0/P1/P2 路线图 | `docs/audits/2026-11-03-audit.md` §6 |
| TODO 条目 aiw1hs / 6h0jqz / xfayi6 | `TODO.md` |

## 结构

1. 概览（4 个 KPI + 结论）
2. 本轮工作：onboarding / 体积优化 / 构建矩阵
3. 计划完成状态：九步 + P0/P1/P2 路线图
4. 验收方法：自动命令 + 人工项
5. 待决策（5 项）
6. 风险与未完成

## 视觉

- 单文件、无外部依赖、系统字体；内联 SVG 三柱对比图（体积三步）
- 表格全部包 `.table-wrap`；打印时避免分页切断表格行
