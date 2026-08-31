# ToDo

面向开发者的本地优先任务工具。仓库里的 `TODO.md` 是唯一事实源，CLI 是标准接口，桌面端只是视图和安全编辑入口。

- 本地模式不需要登录，也不需要网络。
- SQLite 只是派生投影，可随时从 Markdown 重建。
- 对账引擎用 Git 物证核对任务与代码的实际状态，只报告，不擅自改写完成状态。

术语定义见 `CONTEXT.md`；格式规范见 `docs/format-spec-v1.md`。

## 环境要求

| 工具 | 版本 | 说明 |
|---|---|---|
| Rust | 1.74 或更高 | `Cargo.toml` 中的 `rust-version` |
| Node.js | `^20.19.0` 或 `>=22.12.0` | Vite 7 的要求；本仓库在 24.20.0 上验证 |
| pnpm | 10.17.1 | 见 `package.json` 的 `packageManager` |

桌面端另需各平台的 Tauri 2 系统依赖。macOS 需要 Xcode 命令行工具。

## 目录结构

```text
crates/todo-core      解析、保真回写、文件锁、SQLite 投影、对账、桌面后端
crates/todo-cli       canonical CLI，二进制名为 todo
apps/desktop          React 前端
apps/desktop/src-tauri Tauri 2 host，直接调用 todo-core
docs/                 格式规范、桌面 MVP 规格、ADR、验证日志
research/             选型调研与决策依据
```

## 快速开始

安装依赖并构建：

```bash
pnpm install
cargo build
```

在任意项目目录中初始化并使用 CLI：

```bash
cd /path/to/your/project
/path/to/ToDo/target/debug/todo init
/path/to/ToDo/target/debug/todo add "实现解析器" --column todo --tag core --due 2026-09-10 --priority 1
/path/to/ToDo/target/debug/todo list
/path/to/ToDo/target/debug/todo next
```

CLI 在当前工作目录查找 `TODO.md`，不会向上级目录搜索。把 `target/debug` 加入 `PATH` 后可直接使用 `todo`。

已有普通 GFM 清单时，用 `todo init --adopt` 接管现有文件并补齐稳定 ID。不带 `--adopt` 时遇到已存在的文件会拒绝写入。

## CLI 命令

| 命令 | 用途 |
|---|---|
| `todo init [--adopt]` | 创建 `TODO.md`，或接管已有清单 |
| `todo list [--column <id>] [--all]` | 列出任务；`--all` 包含已完成与游离任务 |
| `todo add <正文> [--column <id>] [--tag <值>] [--due <日期>] [--priority <1-3>]` | 新增任务 |
| `todo done <id>` / `todo undone <id>` | 切换完成状态 |
| `todo next` | 给出下一个可执行任务 |
| `todo lint` | 检查格式问题，发现问题时退出码为 6 |
| `todo evidence <id> --from <sha> [--to <sha>]` | 写入 Git 物证，`--to` 默认 `HEAD` |
| `todo reconcile [--apply] [--no-semantic]` | 对账；默认只报告 |
| `todo hooks preview\|install\|uninstall\|instructions [--agent claude\|codex\|all]` | 管理 agent 接入 |

全局选项：`--json` 输出稳定 envelope，`--lock-timeout-ms` 设置文件锁超时，默认 2000 毫秒。

退出码：`0` 成功、`1` 一般错误、`2` 用法错误、`3` 未初始化、`4` 锁超时、`5` 乐观锁冲突、`6` lint 发现问题。

`todo reconcile --apply` 只执行确定且无外部引用的清理，绝不删除 `🔗`、`⛔`、`→` 等物证 token，也不改写完成状态。规则见 `docs/format-spec-v1.md` §6.5。

`todo hooks preview` 会打印本地 agent 配置文件的完整内容，其中可能包含 API 密钥。不要把该输出贴到公开渠道。

## 桌面端

开发模式：

```bash
pnpm --filter @todo/desktop tauri dev
```

生产前端构建与类型检查：

```bash
pnpm typecheck
pnpm build
```

构建 macOS 开发包：

```bash
pnpm --filter @todo/desktop tauri build --debug --bundles app
```

产物为 `target/debug/bundle/macos/Todo.app`。`tauri.conf.json` 中 `bundle.active` 为 `false`，因此需要显式传入 `--bundles app`。该包只有本地 ad-hoc 签名，适合本机验收；对外分发仍需 Developer ID 签名与公证。

桌面端提供看板、今日视图和随记账本，行为与边界见 `docs/desktop-mvp-spec.md`。全局速记快捷键在 macOS 上是 `Cmd+Shift+Space`，其他平台是 `Ctrl+Shift+Space`；速记窗口中 `Cmd/Ctrl+Enter` 保存，`Esc` 隐藏。

Capture 是个人收件箱，存放在 `~/.todo/inbox.md`，不属于任何项目，也没有完成状态。分拣后才会成为当前项目的 Project Task。

## 测试

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
pnpm typecheck && pnpm build
```

投影性能基准。必须使用 `--release`，debug 构建未经优化，数字没有参考价值：

```bash
cargo run --release -p todo-core --example benchmark_projection
```

独立验证记录见 `docs/verification-log.md`，其中包含性能实测数字和尚未自动化的验收项。

## 设计约束

- Markdown 是唯一事实源，工具只修改自己认识的行，其余字节原样保留。
- 稳定 ID 使用随机 base36，写在 HTML 注释中，不影响渲染。
- 写入走文件锁与原子替换；检测到外部修改时放弃本次写入并要求重试。
- 桌面端直接复用 `todo-core`，不启动 CLI 子进程。
- v1 不包含 MCP、账号、云同步和团队空间。

决策记录见 `DECISIONS.md` 与 `docs/adr/`。
