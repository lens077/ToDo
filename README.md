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
| `todo done <id>` / `todo undone <id>` | 切换完成状态；`done` 会清除 `◐`、`⚠` |
| `todo progress <id> <1-99> [--note <文本>]` | 记录部分完成 |
| `todo fail <id> [--note <文本>]` | 记录一次失败的尝试；不改变完成状态 |
| `todo note <id> <文本>` / `todo note <id> --clear` | 写入或清除 `↳` 说明行 |
| `todo next` | 给出下一个可执行任务 |
| `todo lint` | 检查格式问题，发现问题时退出码为 6 |
| `todo evidence <id> --from <sha> [--to <sha>]` | 写入 Git 物证，`--to` 默认 `HEAD` |
| `todo reconcile [--apply] [--no-semantic]` | 对账；默认只报告 |
| `todo hooks preview\|install\|uninstall\|instructions [--agent claude\|codex\|all]` | 管理 agent 接入 |
| `todo scan [--markdown-only\|--comments-only] [--import [--yes]] [--column <id>]` | 扫描全仓未跟踪的工作 |
| `todo history [--limit <n>] [--prune-to <n>]` | 列出还原点 |
| `todo diff <版本>` | 还原点与当前文件的左右对比 |
| `todo restore <版本> [--yes]` | 恢复到某个版本；不带 `--yes` 只预览 |

部分完成与部分失败的语法见 `docs/format-spec-v1.md` §3.1。

## 全仓扫描

`todo scan` 找出仓库里还没被跟踪的工作，两层都是确定性的，不依赖模型：

- **散落的 Markdown checkbox**：其他 `.md` 文件里的 `- [ ]`。代码围栏内的示例、已带 `<!--t:-->` 的行、以及被管理的任务文件本身都会排除。
- **源码注释标记**：`TODO` / `FIXME` / `HACK` / `XXX`。注释符按语言选择，字符串字面量内的标记不算——`"// TODO: ..."` 这类测试夹具是最主要的误报来源。

扫描尊重 `.gitignore`（在非 git 目录下同样生效），跳过大于 2 MB 的文件，并按正文归一化后与现有任务去重，因此重复运行会收敛而不是反复提议同一批。

`--import` 只生成提案并打印 diff，**不写盘**；加 `--yes` 才回写，且回写前自动建还原点。导入的任务带 `#scan-md` / `#scan-todo` 等标签，出处记在 `↳` 说明行里。

## 版本历史

每次写入前自动把改动前的文件存为还原点，放在 `TODO.md` 同级的 `.todo/history/`。该目录用自带的 `.gitignore` 自我屏蔽，不进用户的 git 历史。

- 还原点按序号编号，不做内容寻址：哈希碰撞会让恢复静默返回错误内容，这是恢复机制唯一不能有的故障。
- 无实际改动的写入不记录，避免重复快照把真正的还原点挤出保留窗口。
- `todo restore` 本身也是一次写入，被它替换掉的状态同样成为新的还原点，所以恢复可以再撤销。
- 默认保留 200 个，用 `todo history --prune-to <n>` 清理。

全局选项：`--json` 输出稳定 envelope，`--lock-timeout-ms` 设置文件锁超时，默认 2000 毫秒。

退出码：`0` 成功、`1` 一般错误、`2` 用法错误、`3` 未初始化、`4` 锁超时、`5` 乐观锁冲突、`6` lint 或对账发现问题、`7` 有检查未能执行。

**退出码 `7` 的含义是「没查完」，不是「没问题」。** 浅克隆、非 git 目录或未配置语义对账器时，对应检查整层跳过；此时 `reconcile` 不会返回 `0`，人类输出会打印 `NOT CHECKED:` 行，JSON 的 `report.checks_skipped` 列出原因。不要把 `7` 当作通过。

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
