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
| `todo hooks preview\|install\|uninstall\|instructions [--agent claude\|codex\|deepseek-harness\|all]` | 管理 agent 接入 |
| `todo align [--review <文件\|->]` | 打印全仓对齐提示词，或校验 agent 的回复 |
| `todo serve [--port <n>] [--host <ip>] [--read-only] [--web-root <目录>]` | 启动 HTTP API 与 Web 端 |
| `todo scan [--markdown-only\|--comments-only] [--import [--yes]] [--column <id>]` | 扫描全仓未跟踪的工作 |
| `todo history [--limit <n>] [--prune-to <n>]` | 列出还原点 |
| `todo diff <版本>` | 还原点与当前文件的左右对比 |
| `todo restore <版本> [--yes]` | 恢复到某个版本；不带 `--yes` 只预览 |

部分完成与部分失败的语法见 `docs/format-spec-v1.md` §3.1。

## Web 端与 HTTP API

```bash
pnpm build
todo serve --web-root apps/desktop/dist
# → http://127.0.0.1:3017/  界面
# → http://127.0.0.1:3017/api/v1/  接口
```

同一份 React 构建同时跑在三处：Tauri 桌面壳、浏览器（对接 `todo serve`）、以及无后端的 mock（e2e 用）。选择在 `src/lib/transport.ts` 里一次性完成，不在每个调用点分叉——否则很快会出现第二份逐渐走样的界面。

**默认只绑定回环地址。** 任务文件是仓库的工作状态，不该因为疏忽就暴露在所有网卡上；要对外提供必须显式 `--host`。

**移动端用 `--read-only`。** 该模式下所有写入端点返回 403，「在手机上看」不会变成「在手机上误改仓库」。前端探测到只读后会直接拒绝写操作并给出提示，而不是让每次点击各自失败一次。

curl 示例：

```bash
curl -s localhost:3017/api/v1/snapshot
curl -X POST localhost:3017/api/v1/tasks/<id>/progress \
  -H 'Content-Type: application/json' -d '{"percent":60,"note":"压测未过"}'
```

整文件改写走两步：先 POST 拿到 `base_hash` 与 diff（不写盘），再带上 `confirm_base_hash` 确认。哈希对不上返回 409——确认闸门跨请求依然成立，避免「确认的是一份 diff、写进去的是另一份」。

## 全仓扫描

`todo scan` 找出仓库里还没被跟踪的工作，两层都是确定性的，不依赖模型：

- **散落的 Markdown checkbox**：其他 `.md` 文件里的 `- [ ]`。代码围栏内的示例、已带 `<!--t:-->` 的行、以及被管理的任务文件本身都会排除。
- **源码注释标记**：`TODO` / `FIXME` / `HACK` / `XXX`。注释符按语言选择，字符串字面量内的标记不算——`"// TODO: ..."` 这类测试夹具是最主要的误报来源。

扫描尊重 `.gitignore`（在非 git 目录下同样生效），跳过大于 2 MB 的文件，并按正文归一化后与现有任务去重，因此重复运行会收敛而不是反复提议同一批。

`--import` 只生成提案并打印 diff，**不写盘**；加 `--yes` 才回写，且回写前自动建还原点。导入的任务带 `#scan-md` / `#scan-todo` 等标签，出处记在 `↳` 说明行里。

### 第三层：与代码现实对齐

前两层是确定性的，这一层是判断题，交给 agent：

```bash
todo align > prompt.md          # 把提示词交给 agent 执行
todo align --review reply.json  # 校验它的回复
```

提示词按「操作规程」而不是「请求」来写：固定步骤、显式停止规则、单一输出格式。三条约束对应三种已知的失败：

- **误报即死刑**，所以 `unknown` 是一等判定，且提示词明说不确定时它就是正确答案。
- **覆盖要可证明**，所以 agent 必须对给定清单里每个 id 都给结论，漏掉的会被列进 `NOT REVIEWED`。
- **模型没有决定权**，所有判定都是提议。任何 `done` 缺少 `path:line` 引用会被程序拒绝，不会出现在确认界面上。

`--review` 只校验和汇报，**永远不写文件**；有拒绝项或漏查项时退出码为 6。

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

桌面端首页是**今日视图**，分逾期、今天、未来 7 天、后续可做四段。最后一段是没有截止日期的活，按优先级排——否则一个 P1 但没写日期的任务会在首页上完全消失。每段内部按优先级、截止日期、是否已开工排序——同等紧急时，把做到一半的排在没动过的前面。看板、随记、扫描与历史各占一个一级视图，行为与边界见 `docs/desktop-mvp-spec.md`。

任何整文件改写（扫描导入、版本恢复）都必须先看左右两栏 diff 再确认：预览会锁定它所基于的文件指纹，文件在此期间被改动则拒绝写入，避免「确认的是一份 diff、写进去的是另一份」。

桌面端提供看板、今日视图和随记账本，行为与边界见 `docs/desktop-mvp-spec.md`。全局速记快捷键在 macOS 上是 `Cmd+Shift+Space`，其他平台是 `Ctrl+Shift+Space`；速记窗口中 `Cmd/Ctrl+Enter` 保存，`Esc` 隐藏。

Capture 是个人收件箱，存放在 `~/.todo/inbox.md`，不属于任何项目，也没有完成状态。分拣后才会成为当前项目的 Project Task。

## 测试

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
pnpm typecheck && pnpm build
pnpm e2e
```

`pnpm e2e` 跑 Playwright 交互层回归；`./scripts/e2e-web.sh` 另起一个真实的 `todo serve`，用同一份构建跑 Web 端全链路（读写真实 TODO.md）。前端在非 Tauri 环境下走 mock 桥接，因此这套测试不需要 webview 或真实项目，覆盖视图路由、确认闸门与 diff 渲染；Rust 侧的契约由 `cargo test` 保证，两者刻意分工。

注意 `vite preview` 默认只监听 `::1`，配置里显式绑定 `127.0.0.1`，否则健康检查永远超时。

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
