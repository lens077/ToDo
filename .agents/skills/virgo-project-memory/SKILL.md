---
name: virgo-project-memory
description: 读取并维护 Sumery Virgo 的 Project Contract：工作前读取 .virgo/project.yaml 与映射文档，交接前更新受影响文档并运行 todo contract check。
---

# Virgo Project Memory

## 工作开始时

1. 读取 `.virgo/project.yaml`（Project Contract schema v1，见 `docs/project-contract-spec.md`）。
2. 按 `documents` 映射读取适用的权威文档：任务（TODO.md）、进度（CONTEXT.md）、决策（DECISIONS.md）、Agent 指令（AGENTS.md）。
3. 确定任务范围与验收条件。

## 交接前

1. 更新受影响的权威文档，保留既有路径与格式。
2. 记录真实验证结果（命令与输出），不把 AI 自述当作证据。
3. 列出未解决问题与下一步。
4. 运行 `todo contract check`，退出码 6 表示存在错误级发现，须先修复。
5. 报告变更文件清单与后续步骤。

## 边界

- 写入受管理文件前先审阅 diff；保留用户自定义内容。
- 核心检查是确定性的，不依赖模型运行时。
