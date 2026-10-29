# Project Contract 规范 v1

状态：已实现（阶段 1）。需求来源：`virgo-project-contract-spec/index.html`（Sumery Virgo · Project Contract Agent 实施规范 v1.0 draft）。本文描述已落地的接口，与 spec 草案冲突之处以本文为准。

## 定位

Project Contract 是机器可读的项目声明（`.virgo/project.yaml`），记录项目身份、文档角色映射与检查策略。它与 TODO.md 的 `todo-spec: 1` 是两个独立版本面：`todo-spec` 约束任务文件格式（唯一格式依据见 `docs/format-spec-v1.md`），`contract` schema 约束项目声明本身。Contract 不保存项目知识，只保存指向知识所在位置的映射。

## Schema v1

```yaml
schema_version: 1

project:
  id: "stable-project-uuid"   # 必填，非空
  name: "Example Project"     # 必填，非空
  description: "…"            # 可选
  status: active              # 默认 active

sources:                      # 可选
  - id: "main"                # 唯一
    path: "."                 # 项目相对路径
    role: code

documents:                    # 角色 -> 路径映射，角色集合封闭
  tasks:        { path: "TODO.md" }
  progress:     { path: "CONTEXT.md" }
  architecture: { path: "docs/architecture.md" }
  runbook:      { path: "docs/runbook.md" }
  decisions:    { path: "DECISIONS.md" }
  agent_instructions: { path: "AGENTS.md" }

policy:
  agent_instructions: "AGENTS.md"
  skill: ".agents/skills/virgo-project-memory/SKILL.md"
  enforcement: guided         # observe | guided | enforced，默认 guided
  checks:
    require_task_reference_for_completion: false
    require_test_evidence: false
    require_session_handoff: false
```

- 文档角色（封闭集合）：`tasks`、`plan`、`architecture`、`progress`、`runbook`、`decisions`、`environment`、`handoff`、`agent_instructions`。未知角色是 schema 错误。
- 所有路径必须是项目相对路径，拒绝绝对路径与越过根目录的 `..` 组件。
- 未知字段拒绝（`deny_unknown_fields`），保证校验器严格可定位。

## 检查规则（稳定规则 ID）

| 规则 | 内容 | 默认级别 |
|---|---|---|
| VPC001 | schema 无效：解析失败、未知字段/角色、`schema_version` 非 1、空字段、重复 source id、路径不安全 | 错误 |
| VPC002 | 映射的文档路径不存在 | 错误（tasks/agent_instructions）或警告，enforced 模式下全部为错误 |
| VPC003 | 启用的 policy checks 要求的角色缺少映射（require_task_reference→tasks、require_session_handoff→handoff） | 警告，enforced 模式下为错误 |
| VPC004 | 多个 source 指向同一路径 | 警告 |
| VPC005 | `policy.agent_instructions` / `policy.skill` 引用的文件不存在 | 警告 |
| VPC007 | enforced 模式缺少 `handoff` 映射 | 错误 |

规则 ID 稳定；单元测试覆盖每条规则的通过与失败场景（`crates/todo-core/src/contract.rs`）。

## CLI

```bash
todo contract check            # 检查当前目录
todo contract check --root .   # 显式指定项目根
todo contract check --json     # schema "todo.contract.v1"
```

退出码沿用仓库既有门禁语义（见 CLI `--help` after_help），不使用 spec 草案中的 0-4 编码：

- `0` 无阻断性发现；
- `6` 存在错误级发现（与 lint/reconcile 的 findings 语义一致）；
- 其他错误码（1/2/3…）按 `error_code` 既有映射。

## 核心实现

- `crates/todo-core/src/contract.rs`：schema、loader（`load_contract`）、确定性校验器（`check_contract`）。纯离线，无模型调用。
- `crates/todo-cli/src/main.rs`：`Command::Contract` 子命令与 `contract_check` 处理函数。
- 本仓库自己的 contract：`.virgo/project.yaml`。

## 与 spec 草案的差异记录

1. 退出码采用仓库惯例 0/1/6/7，而非草案的 0-4。
2. 阶段 0-1 已落地；接管向导（阶段 2）、Skill 安装器（阶段 3）、Hook/CI 集成（阶段 4）、桌面视图（阶段 5）未实现，见 spec 的分阶段计划。
3. 阶段 0 审计报告：`docs/project-contract-audit.md`。
