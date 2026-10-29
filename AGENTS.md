# AGENTS.md

<!-- BEGIN VIRGO MANAGED RULES -->
## Virgo Project Memory

Before work:
1. Read `.virgo/project.yaml`.
2. Read the mapped source documents.
3. Identify task scope and acceptance criteria.

Before handoff:
1. Update affected source documents.
2. Record actual verification results.
3. List unresolved issues.
4. Run `todo contract check`.
5. Report changed files and next steps.

Review diffs before writing managed files.
Keep user-owned instructions intact.
<!-- END VIRGO MANAGED RULES -->

## 本仓库约定

- Markdown 是唯一事实源；SQLite 只是可重建的派生投影。
- TODO.md 格式唯一依据是 `docs/format-spec-v1.md`（`todo-spec: 1`），与 `.virgo/project.yaml` 的 contract schema 相互独立。
- 验证命令：`make check-quick`（快速门禁）、`make check`（完整门禁）。
