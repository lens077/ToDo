# Workspace Agent 集成与验证协议（Phase 5）

Session 与文档校验的 Agent 接入方式、CLI 契约、CI 模板。Session 文件位于各项目 `.virgo/sessions/`，Markdown 是事实源。

## CLI 契约（Agent 接入方式 2：纯 CLI）

所有 JSON 输出遵循稳定信封 `{"schema": "...", "ok": true|false, "data"| "error": ...}`。

| 命令 | 作用 | Schema |
|---|---|---|
| `todo session begin --goal "…"` | 开始会话；已有活动会话时报错 | `todo.session.v1` |
| `todo session finish --completed … --decision … --validation "cmd\|result\|evidence" --unresolved … --next … --task <id> --context "…"` | 完成会话并写入总结（含 git 变更文件清单、任务链接、Virgo 确定性验证行） | `todo.session.v1` |
| `todo session interrupt` | 中断活动会话，解锁新会话 | `todo.session.v1` |
| `todo session list` | 会话列表（新→旧） | `todo.session-list.v1` |
| `todo session resume [--json]` | 跨天恢复：最新摘要 + outstanding（active 取目标，completed 取下一步） | `todo.session-resume.v1` |
| `todo docs validate` | 会话卫生校验：ERROR 阻断（exit 6）、WARNING/INFO 提示 | `todo.docs.v1` |

退出码沿用既有约定：0 成功、1 错误、2 用法、6 findings（阻断）。

## Hooks 接入（Agent 接入方式 1：宿主注入）

`todo hooks instructions --write` 把会话工作流写进 AGENTS.md/CLAUDE.md 的托管块（`todo hooks install` 安装 SessionStart 注入）。托管块之外的章节永远是用户自有内容。

## 验证规则（`todo docs validate`）

| 规则 | 级别 | 条件 |
|---|---|---|
| VDS001 | ERROR | 会话文件缺 frontmatter `status`；resume 无法判断生命周期 |
| VDS002 | ERROR | 活动会话 Goal 为空；恢复产出空上下文 |
| VDS003 | WARNING | 最近完成的会话无 Next Actions |
| VDS004 | INFO | 项目还没有会话记录 |

## CI 模板（阻断 ERROR）

```yaml
# .github/workflows/docs-validate.yml
name: docs-validate
on: [push, pull_request]
jobs:
  validate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo install --path crates/todo-cli
      - name: Session & docs hygiene (blocks on ERROR)
        run: todo docs validate
```

本地提示用 git hook（可绕过）；分支保护上的 CI 才是强制门禁。

## 两种接入方式的验证证据

- 方式 1（hooks 注入）：`agent_integration_block_and_skill_document_both_advertise_sessions`（crates/todo-cli/tests/cli.rs）验证 instructions 落盘并包含会话工作流。
- 方式 2（纯 CLI）：`session_begin_finish_resume_round_trip_over_json` 验证 JSON 信封、错误信封、resume 数据；`docs_validate_blocks_on_error_and_passes_clean_projects` 验证 exit 6 阻断。
