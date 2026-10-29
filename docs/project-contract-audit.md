# Virgo Project Contract · 阶段 0 仓库审计报告

审计时间：2026-10-30。审计方式：对仓库源码、测试与文档的只读扫描。结论均附真实文件路径与符号。

## 可复用模块

| 能力 | 位置 | 结论 |
|---|---|---|
| CLI 框架 | clap 4.5 derive；`crates/todo-cli/src/main.rs` 的 `Cli` / `enum Command` / `fn run()` | 新命令加变体即可；`--json` 是全局参数 |
| JSON 输出 | `Envelope<T>` + `emit()`（main.rs），schema 命名 `todo.<cmd>.v1` | 直接复用 |
| 退出码 | `fn error_code`（main.rs）；0/1/2/3/4/5/6/7 已分配，6=findings、7=check 无法运行 | contract check 沿用 6 |
| YAML | `serde_yaml 0.9` 已在 workspace deps，todo-core 已依赖 | 无需新增依赖 |
| 错误类型 | `crates/todo-core/src/error.rs` `Error::InvalidConfiguration { path, message }` | contract 解析错误直接复用 |
| 报告结构先例 | `todo lint`（main.rs `fn lint`，JSON schema `todo.lint.v1`，退出码 6） | contract check 照此范式 |
| SQLite 投影 | `crates/todo-core/src/projection.rs`，`SCHEMA_HASH`、`rebuild` | 本次只读校验，不涉及 |
| 写回/备份 | `write.rs` `LockedWriter`、`history.rs` `History` | 本次只读校验，不涉及 |
| Git Hook | `Makefile` `hooks` 目标 → `core.hooksPath .githooks`；仅 `pre-push`（调 `scripts/verify-quick.sh`） | 阶段 4 集成点 |
| 测试 | 内联 `#[cfg(test)]` + `tests/cli.rs` 的 `fn todo(dir, args)` 助手，`tempfile` 已是 dev-dep | 直接沿用 |

## 需要新增的模块

- `crates/todo-core/src/contract.rs`：schema v1、loader、校验器（本次已新增）。
- CLI `Command::Contract` 变体（本次已新增）。
- 项目级配置文件 `.virgo/project.yaml`：此前无项目级可提交配置机制，全新面，无冲突。
- `docs/project-contract-spec.md`、本审计报告。
- 尚未实现（按 spec 分阶段）：接管向导、Skill 安装器、Hook/CI 集成、桌面 findings 视图（可复用 `apps/desktop/src/components/DiffView.tsx` 与 tauri command 范式）。

## 已确认的约定与冲突

1. **`todo hooks` 名称已被占用**（`crates/todo-cli/src/hooks.rs`：向 agent settings.json 注入命令），contract 的 git-hook 集成不得复用该名称。
2. **`docs/format-spec-v1.md` 自声明为 TODO.md 格式唯一依据**：contract 文档必须明确两者是独立版本面，已在 `docs/project-contract-spec.md` 划清。
3. **CONTEXT.md 术语**（Project Root、Task Source 等）：contract 文档沿用这些词汇。
4. **无 Rust CI 工作流**：`.github/workflows/` 只有 deploy.yml 与 release.yml，质量门禁全靠 pre-push hook；spec 的「CI 独立验证强制规则」在阶段 4 需新增 workflow。
5. **spec 草案退出码 0-4 与仓库 0-7 冲突**：按仓库惯例执行，差异已记录在 spec 文档。

## 阶段 1 落地验证

- `cargo test -p todo-core contract`：15 通过（schema 拒绝未知字段/角色、VPC001-007 各场景、loader 行为）。
- `cargo test -p todo-cli --test cli contract`：5 通过（missing→VPC001+退出码 6、最小 contract 通过、VPC002、未知 schema_version、`--root` 显式根）。
