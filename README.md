# ToDo

**Sumery Virgo**（室女座）——给 AI 协作开发者的本地任务对账器，Markdown 是唯一事实源。

**Sumery** 是品牌，**Virgo** 是桌面产品名（`Sumery Virgo` / 菜单栏 `Virgo` / `com.sumery.virgo`）；仓库 `ToDo`、CLI `todo`、`TODO.md` 格式是工程名，不改。规则见 Sumery 品牌系统（Libra 仓库 `docs/brand/DESIGN.md`），`apps/desktop/src/identity.test.ts` 把字段钉死。

给 AI 协作开发者的本地任务对账器。任务状态与代码现实发生漂移时，人和 agent 可能被过期任务误导，重复已经完成的工作。

ToDo 把每个仓库的 Project Task 保存在该仓库的 `TODO.md` 中，以可审计的 Git 物证辅助对账；回写只修改工具识别的字节区间，未识别内容逐字节保留；对账结论只报告，不擅自改写完成状态。CLI 是标准接口，桌面看板是同一事实源的视图和编辑入口。

当前是实验性 MVP。Project Task 随仓库管理；个人 Capture 另以 `~/.todo/inbox.md` 为事实源，不进入 Git，也不受项目 `.todo/history/` 还原点保护，需要单独备份。语义对账默认关闭，需自备 LLM provider 并显式启用；Playwright 与真实服务 E2E 已接入 CI，桌面原生路径仍有人工验收待完成。首次用于真实项目之前，先提交或备份现有任务文件，并审查每次写入的 diff。

**许可未定，仅供授权评估。** 本仓库尚未选定许可证，保留一切权利；未经书面授权，不得使用、复制、修改、分发或衍生。本声明约束的是**源码的使用与分发**，与下方「授权与定价」说明的产品销售方式是两件事：商业模型已定（`DECISIONS.md` 决策④），开源边界与许可证文本仍未定（决策②）。许可证选定前本声明有效。

- CLI 不需要登录，也不需要网络。桌面端需要周期性联网校验授权，离线可继续使用 30 天。
- SQLite 只是派生投影，可随时从 Markdown 重建。

术语定义见 `CONTEXT.md`；格式规范见 `docs/format-spec-v1.md`；已完成验证与待验收项见 `docs/verification-log.md`。

## 授权与定价（计划）

当前是实验性 MVP，尚未发售。本节说明已经定下的方向，便于提前判断，**不构成销售要约**；正式发售时以销售页为准。

- **本体买断，永久可用。** 一次性付费，含自购买日起 **12 个月**的更新权。窗口到期后软件继续正常工作，只是不再收到此后发布的新版本。到期前后的产品完全一致：不降级、不弹窗、不加水印、不减少任何已有能力。
- **起步价 $19 / ¥88，续费半价。** 这是按当前功能范围定的**早鸟价**，随能力增加上调；续费按续费当时标价的一半计。**断档不设惩罚**：停多久都可以按当时续费价续上，拿到当时的最新版并重新计 12 个月；也可以在首购时选择自动续费，随时取消，已拿到的版本永远保留。**涨价不影响任何已购用户的永久使用权，也不会缩短已经买到的窗口。**
- **更新窗口按时间计，不按版本号计。** 购买后 12 个月内发布的版本永久归你，因此版本号如何排列都不影响你的权利。购买后 30 天内发布的大版本同样视为窗口内。
- **安全修复与数据修复永久免费**，覆盖所有版本，不受更新窗口限制。
- **未来的云同步、移动推送、团队空间是独立可选订阅**，不影响本体任何功能。不购买订阅，本体照常完整可用。
- 一份授权覆盖个人的全部设备与操作系统。商业与团队使用另走年度订阅。
- **桌面端可免费试用 30 天**，试用期内功能与付费版完全相同。到期后桌面端转为只读，可以继续查看，不能再写入；**CLI 与你的 `TODO.md` 不受任何影响**，购买后立即恢复。试用需要用邮箱激活，试用截止日由服务端签发。
- **桌面端在启动时联网校验授权**，校验通过后离线可继续完整使用 **30 天**；离线超过 30 天转为只读，恢复联网后立即解除。启动时连不上服务器**不会被拒之门外**：只要本机有有效的校验记录就正常进入，后台重试。校验只发送授权 key、版本号和一个随机安装标识，**不发送项目路径、任务内容或机器指纹**。
- **服务停止保障。** 如果授权服务停止运营，我们承诺在停止前发布一个移除校验的最终版本，或向所有持有者签发永久有效的离线凭据——二者至少做到其一。买断的「永久可用」不以我们的服务器还活着为前提。

明确不做：**本体订阅制**、**lifetime deal 促销**、**任何形式的数据扣押**。任务数据始终是你仓库里的纯文本，随时可以迁出，与是否付费无关。

完整论证见 `docs/business-model-options.md`，结论与待定项见 `DECISIONS.md` 决策④。

## 环境要求

| 工具 | 版本 | 说明 |
|---|---|---|
| Rust | 开发工具链 1.98.1；MSRV 1.88 | `rust-toolchain.toml` 固定开发、Clippy 和发布工具链；独立 MSRV job 从 `Cargo.toml` 的 `rust-version` 读取下限，显式选用该版本运行锁定依赖测试 |
| Node.js | 26.8.1 | `.node-version` 固定本地与 CI 版本；`package.json` 要求 Node 26.x |
| pnpm | 12.0.0 | 见 `package.json` 的 `packageManager` |

`.node-version` 是版本声明，不会自行切换全局 Node；运行 pnpm 命令前，先用本地 Node 版本管理器选择该版本。rustup 会自动读取 `rust-toolchain.toml`；手动复验 MSRV 可运行 `cargo +1.88.0 test --workspace --locked`，不修改全局默认工具链。

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

从仓库源码安装 CLI：

```bash
cargo install --path crates/todo-cli --locked
todo --version
```

`cargo install` 会以 release 模式构建，并把 `todo` 安装到 Cargo 的二进制目录（默认是 `~/.cargo/bin`）。

在任意项目目录中初始化并使用 CLI：

```bash
cd /path/to/your/project
todo init
todo add "实现解析器" --column todo --tag core --due 2026-09-10 --priority 1
todo list
todo next
```

CLI 在当前工作目录查找 `TODO.md`，不会向上级目录搜索。

开发 Web 端或桌面端时，使用仓库根目录的 `Makefile` 安装依赖并构建：

```bash
make install
make build
make dev
```

`make help` 列出完整命令。`make bundle-macos` 只生成 `Todo.app`；`make package-macos` 生成本地验收用的 CLI、应用压缩包和 `SHA256SUMS`。本地验收包未经 Developer ID 签名和 Apple 公证，不得分发。

已有普通 GFM 清单时，用 `todo init --adopt` 接管现有文件并补齐稳定 ID。不带 `--adopt` 时遇到已存在的文件会拒绝写入。

## CLI 命令

| 命令 | 用途 |
|---|---|
| `todo init [--adopt]` | 创建 `TODO.md`，或接管已有清单 |
| `todo list [--column <id>] [--all]` | 列出任务；`--all` 包含已完成与游离任务 |
| `todo add <正文> [--column <id>] [--tag <值>] [--due <日期>] [--priority <1-4>]` | 新增任务 |
| `todo done <id> [--note <文本>] [--evidence]` / `todo undone <id>` | 切换完成状态；`--evidence` 附带建议的 Git 区间，严格模式下 `done` 必须提供说明 |
| `todo archive` | 把已完成的顶层任务子树移入 `TODO.archive.md` |
| `todo progress <id> <1-99> [--note <文本>]` | 记录部分完成 |
| `todo fail <id> [--note <文本>]` | 记录一次失败的尝试；不改变完成状态 |
| `todo note <id> <文本>` / `todo note <id> --clear` | 写入或清除 `↳` 说明行 |
| `todo next` | 给出下一个可执行任务 |
| `todo lint` | 检查纯文档结构：解析错误、重复稳定 ID（error）、非标准 checkbox（warning）。发现问题时退出码为 6；依赖与 Git 物证问题要跑 `todo reconcile` |
| `todo evidence <id> --from <sha> [--to <sha>]` | 写入 Git 物证，`--to` 默认 `HEAD` |
| `todo reconcile [--apply] [--semantic]` | 对账；默认只报告。结构层与 Git 物证层默认运行；**语义层默认关闭**，需 `--semantic` 并自备 provider |
| `todo hooks preview\|install\|uninstall\|instructions [--agent claude\|codex\|deepseek-harness\|all]` | 管理 agent 接入 |
| `todo align [--review <文件\|->]` | 打印全仓对齐提示词，或校验 agent 的回复 |
| `todo serve [--port <n>] [--host <ip>] [--read-only] [--web-root <目录>]` | 启动 HTTP API 与 Web 端 |
| `todo scan [--markdown-only\|--comments-only] [--import [--yes]] [--column <id>]` | 扫描全仓未跟踪的工作 |
| `todo ai upsert --stdin` / `todo ai upsert --source <s> --session <id> --workspace <路径> --state <状态> [...]` | 由 agent 宿主发布一条会话状态；契约见 `docs/ai-feed-spec.md` |
| `todo ai list [--workspace <路径>] [--state <状态>]... [--all]` | 列出 AI 会话；默认隐藏 `handled` 与已移除的记录 |
| `todo ai get\|ack\|remove <source> <session>` / `todo ai link <source> <session> <task-id>` | 查看、标记已读、移除，或关联到某个任务的稳定 ID |
| `todo ai prune [--older-than <天>]` | 清理早于 N 天（默认 30）的 `handled` 或已移除记录 |
| `todo history [--limit <n>] [--prune-to <n>]` | 列出还原点 |
| `todo diff <版本>` | 还原点与当前文件的左右对比 |
| `todo restore <版本> [--yes]` | 恢复到某个版本；不带 `--yes` 只预览 |

### 完成策略、结局与物证

项目 frontmatter 的 `completion-policy` 决定结局说明是否必填，缺省为 `normal`。可用 `todo policy` 查看，用 `todo policy normal|strict` 修改。

| 策略 | 完成 `done` | 部分完成 `progress` | 失败 `fail` |
|---|---|---|---|
| `normal` | 说明可选；未传 `--note` 时清除旧说明 | `--note` 必填 | `--note` 必填 |
| `strict` | `--note` 必填 | `--note` 必填 | `--note` 必填 |

桌面端与 Web 端把跨列移动视为状态转换，并把任务放在目标列顶部：移入 `type: done` 列改为完成，移回活动列则恢复为未完成。普通任务通过拖放或右键菜单移入完成列时直接写入；严格模式或带有 `◐`/`⚠` 的任务先填写结局说明。确认后，列、勾选状态、说明、物证以及冲突 token 的清理通过一次原子写入完成。移动父任务时整个子树一起移动；跨越完成边界会同步后代状态，进入完成列会清除后代的 `◐`/`⚠`，同时保留其锚定说明。

`↳` 说明行记录三个结局的结果、剩余工作或失败原因。`done` 总会清除互相冲突的 `◐`、`⚠`；普通模式未给新说明时也清除旧 `↳`，严格模式则把本次必填说明写回。部分完成与失败的语法见 `docs/format-spec-v1.md` §3.1。

`todo done <id> --note "验收通过" --evidence` 会尝试附带建议物证区间：已有 `🔗from..to` 时沿用其 `from`，否则从 Git 历史中查找稳定 ID 首次出现的 commit，并以当前 `HEAD` 为 `to`。Git 不可用、稳定 ID 尚未进入历史、commit 无法解析或区间会成为空的 `HEAD..HEAD` 时，命令明确报告 `evidence unavailable` 并拒绝写入；绝不会生成 `HEAD..HEAD`。不需要物证时省略 `--evidence`。

桌面端与 `todo serve` 为每个任务展示同样的建议区间，但不会每次刷新都重读历史：进程内缓存「稳定 ID → 首次出现的 commit」，只在 `HEAD` 前进时增量扫描新 commit；任务文件名沿用打开的文件，不限于 `TODO.md`。已有 `🔗from` 会在同一次 `git cat-file` 里批量解析成完整 commit；解析不到的 `from` 直接报告不可用，而不是等到写入时才失败。

完成状态、说明与物证是一次复合写入：同一个 `LockedWriter` 持锁校验并原子替换，只建立一个 restore point，不会出现任务已完成但说明或物证未写入的中间状态。写入前的原文保存在 `.todo/history/`，可用 `todo history` 找到版本，再用 `todo diff <版本>` 查看。

### 归档已完成任务

运行 `todo archive` 会把已完成的顶层任务子树移到同目录的 `TODO.archive.md`。归档沿用文件锁和版本历史；任务的 `🔗`、`⛔`、`→` 等物证 token 会随整个子树一起搬走，不会在归档时删除。实现先复制到归档文件，再从 `TODO.md` 删除；遇到崩溃或并发冲突时，宁可留下可检查的重复副本，也不会丢失唯一副本。

## Web 端与 HTTP API

```bash
pnpm build
todo serve --web-root apps/desktop/dist
# → http://127.0.0.1:3017/  界面
# → http://127.0.0.1:3017/api/v1/  接口
```

同一份 React 构建同时跑在三处：Tauri 桌面壳、浏览器（对接 `todo serve`）、以及无后端的 mock（e2e 用）。选择在 `src/lib/transport.ts` 里一次性完成，不在每个调用点分叉——否则很快会出现第二份逐渐走样的界面。

严格模式下，Web/桌面端勾选完成不会直接写盘，而是打开三结局确认对话框。完成、部分完成与失败都要求说明；部分完成还要求 `1–99` 的完成比例。Git 物证只适用于完成结局：可用时默认勾选，不可用时展示具体原因并禁用。每个结局都通过一次请求和一次 `LockedWriter` 事务写入，成功后才关闭对话框。

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

## AI 会话状态

agent 宿主（如 DeepSeek Harness）通过 `todo ai upsert` 发布每个会话的当前状态：`running`、`waiting`、`unread`、`seen`、`failed`、`handled`，以及最近一次提问、收尾回答和改动文件。记录以 `(source, session_id)` 为主键，存放在全局的 `~/.todo/ai/sessions.json`，**不进 `TODO.md`，也不进 Git**；`event_at` 早于已存记录的写入会被忽略并报告 `unchanged`。`todo ai list` 在任何目录下都可运行，不要求当前目录有 `TODO.md`；`ack`、`remove`、`link` 只改 ToDo 本地标记，宿主的后续更新不会覆盖「已移除」和「已关联」。字段、状态语义、排序与退出码见 `docs/ai-feed-spec.md`；测试或脚本可用环境变量 `TODO_AI_STORE` 指向另一份存储文件。

### 集成 DeepSeek Harness（DSH）

ToDo 通过 `integrations/dsh-todo-bridge/` 连接 DSH。桥接插件订阅 DSH 的会话、投影、收件箱和交互状态，并把实时快照写入 ToDo 的 AI 会话存储。首次安装或升级时按以下步骤执行：

1. **准备 CLI 和依赖**

   确认 `todo` 已安装，并确保 DSH 进程可以在 PATH 中找到它。也可以在 DSH 配置中填写 `todoBinary` 的绝对路径：

   ```bash
   cargo install --path crates/todo-cli --locked
   todo --version
   dsh --version
   ```

2. **构建桥接插件**

   ```bash
   cd /Users/lens/lens077/ToDo/integrations/dsh-todo-bridge
   pnpm install
   pnpm typecheck
   pnpm build
   pnpm test
   ```

   构建会生成宿主端 `lib/index.js`、Web 客户端端 `lib/client.js` 及类型声明。

3. **安装到 DSH 的 web 配置档**

   推荐直接从本地目录安装：

   ```bash
   dsh plugin --profile web add file:/Users/lens/lens077/ToDo/integrations/dsh-todo-bridge
   ```

   也可以先打包再安装：

   ```bash
   pnpm pack --out /tmp/dsh-todo-bridge.tgz
   dsh plugin --profile web add file:/tmp/dsh-todo-bridge.tgz
   ```

   `file:` 安装会复制构建产物，不是符号链接。之后重新执行 `pnpm build` 后，必须再次执行 `dsh plugin ... add` 才会把新产物复制进 DSH 配置档。

4. **配置工作区和 CLI 路径**

   编辑 `~/.dsh/profiles/web/cordis.patch.yml`，保留插件 id，并按实际路径修改配置：

   ```yaml
   - id: todo-bridge
     name: 'dsh-todo-bridge'
     config:
       enabled: true
       todoBinary: /Users/lens/.cargo/bin/todo
       workspaces:
         - /Users/lens/lens077/ToDo
       debounceMs: 300
       maxConsecutiveFailures: 5
       openUrlTemplate: '{base}/?session={sessionId}'
   ```

   `workspaces` 为空数组表示发布所有工作区；填写后只发布指定目录及其子目录中的 DSH 会话。`todoBinary` 如果已在 PATH 中可写成 `todo`。

5. **重启 DSH Web 并验证**

   新增依赖和客户端 bundle 只在 DSH 重启后生效：

   ```bash
   dsh --profile web --dump-config | grep -A 8 'id: todo-bridge'
   dsh web --profile web
   todo ai list --all
   todo ai list --workspace /Users/lens/lens077/ToDo --json
   ```

   DSH Web 运行后，在 ToDo 的 AI 页面应看到当前会话状态。点击「打开会话」会通过 `open_url` 定位到 DSH 对应 session；桥接版本为 `dsh-todo-bridge/0.2.0`，并监听 `session/created` / `session/disposed`，避免 ToDo 保留已释放会话的旧「运行中」状态。

6. **升级或卸载**

   升级桥接插件时，重新构建、重新执行安装命令并重启 DSH Web：

   ```bash
   cd /Users/lens/lens077/ToDo/integrations/dsh-todo-bridge
   pnpm build
   dsh plugin --profile web add file:/Users/lens/lens077/ToDo/integrations/dsh-todo-bridge
   ```

   卸载：

   ```bash
   dsh plugin --profile web remove dsh-todo-bridge
   ```

   更完整的配置字段、状态映射、限制和独立测试方法见 `integrations/dsh-todo-bridge/README.md`。

## 版本历史

每次写入前自动把改动前的文件存为还原点，放在 `TODO.md` 同级的 `.todo/history/`。该目录用自带的 `.gitignore` 自我屏蔽，不进用户的 git 历史。

- 还原点按序号编号，不做内容寻址：哈希碰撞会让恢复静默返回错误内容，这是恢复机制唯一不能有的故障。
- 无实际改动的写入不记录，避免重复快照把真正的还原点挤出保留窗口。
- `todo restore` 本身也是一次写入，被它替换掉的状态同样成为新的还原点，所以恢复可以再撤销。
- 默认保留 200 个，用 `todo history --prune-to <n>` 清理。

全局选项：`--json` 输出稳定 envelope，`--lock-timeout-ms` 设置文件锁超时，默认 2000 毫秒。

退出码：`0` 成功、`1` 一般错误、`2` 用法错误、`3` 未初始化、`4` 锁超时、`5` 乐观锁冲突、`6` lint 或对账发现问题、`7` 有检查未能执行。

**退出码 `7` 的含义是「没查完」，不是「没问题」。** 浅克隆、非 git 目录或未配置语义对账器时，对应检查整层跳过；此时 `reconcile` 不会返回 `0`，人类输出会打印 `NOT CHECKED:` 行，JSON 的 `report.checks_skipped` 列出原因。不要把 `7` 当作通过。

### 语义对账（可选，默认关闭）

结构层与 Git 物证层只判断「稳定 ID、依赖、commit 区间是否有效」，**不判断任务正文是否已由代码实现**。后者属于语义层，需要自备 LLM provider 并显式 `--semantic`：

```bash
export TODO_LLM_PROVIDER=anthropic   # 或 openai
export TODO_LLM_API_KEY=<your-key>
todo reconcile --semantic
```

可选 `TODO_LLM_MODEL` 与 `TODO_LLM_BASE_URL`。语义结论一律进 `suspected`，只报告、绝不自动改写完成状态。

已配置 provider 却没带 `--semantic` 时，`reconcile` 会把「语义层未运行」计入 `checks_skipped` 并返回 `7`——避免把一次只跑了结构层的运行误读成「任务与代码已核对」。明确不需要语义层时用 `--no-semantic`，它是一次知情的选择，仍返回 `0`。

`todo reconcile --apply` 只执行确定且无外部引用的清理，绝不删除 `🔗`、`⛔`、`→` 等物证 token，也不改写完成状态。规则见 `docs/format-spec-v1.md` §6.5。

`todo hooks preview|install|uninstall` 读写的是本地 agent 配置文件（如 `~/.claude/settings.json`），其中可能包含 API 密钥。这三条命令默认只打印目标路径、`changed` 与备份位置，**不输出文件正文**（`--json` 的 `content` 字段同样被替换为占位符）。确需查看完整内容时显式加 `--show-content`，并注意该输出不可贴到 issue、CI 日志或聊天中。

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

构建 macOS 本地验收包：

```bash
pnpm --filter @todo/desktop tauri build --debug
```

产物为 `target/debug/bundle/macos/Todo.app`。本地构建没有 Developer ID 签名或 Apple 公证，只适合本机验收，不得作为正式发布产物。

正式发布由 `.github/workflows/release.yml` 构建、签名、公证并上传 release 产物。证书配置、失败条件与下载后验证步骤见 `docs/macos-release.md`。

桌面端首页是**目标视图**，把推进中、待办、已完成放在同一个工作区：推进中的列在最前，默认只展开 1 项、最多同时展开 5 项，卡片内容完整可见，其余排队并可「提到最前」；待办列在其下；已完成栏在右侧纵贯整个工作区。已完成栏和待办区都可以像 VS Code 侧栏一样收起成细条，让推进中占满；展开数与收起状态记在本机。顶部的「逾期 / 今天 / 7 天内」筛选沿用今日视图的派生规则，只筛选、不复制任务。推进中的列按列 `id`/名称匹配 `doing`、`正在`、`进行` 等词，匹配不到时取第二个活动列。

部分完成与失败直接在卡片里记录：点 ◐ 或 ⚠ 展开进度条与一行说明，回车写入，`Esc` 取消，不弹对话框。勾选完成默认先弹一步确认（回车完成、`Esc` 取消），防止误点直接写盘；可在左下角「设置」里关闭。拖放或右键菜单移动任务时不使用这层安全确认；任务始终进入目标列顶部，移出完成列会恢复为未完成。严格模式以及带 `◐`/`⚠` 的任务进入完成列时仍填写结局说明。随记、扫描与历史各占一个一级视图，行为与边界见 `docs/desktop-mvp-spec.md`。

普通 Markdown 接管后不一定有任务列。此时「移动到…」禁用，菜单内提供 **「配置任务列…」**：可创建「待办 / 进行中 / 已完成」空列，或勾选文件内已有的唯一二级标题。先检查完整左右 diff，再确认写入；取消不改文件，外部修改会使旧预览失效。默认仅新增声明和空标题，**不会移动现有任务或改写领域分类**。已有列定义不通过此入口覆盖。适用于 ecommerce 这类按领域和优先级组织的长任务文件，详见 `docs/task-columns-spec.md`。

**未归列任务可直接拖入「推进中」**，也可从任务菜单选择「开始处理」。已有同来源活动列时直接移动；无列时自动展示「创建列并推进任务」差异，确认后一次创建默认列并推进指定任务，不必配置后再拖一次。取消不创建列也不移动任务；任务的普通续行、子任务与备注随完整 Markdown 列表项保留，其他任务不动。

桌面端会记住上次打开的项目（`~/.todo/desktop.json`），下次启动直接载入；项目已移走或文件无法解析时静默回到「选择项目」状态，而不是把它报告成解析错误。

任何整文件改写（扫描导入、版本恢复）都必须先看左右两栏 diff 再确认：预览会锁定它所基于的文件指纹，文件在此期间被改动则拒绝写入，避免「确认的是一份 diff、写进去的是另一份」。

桌面端提供目标视图和随记账本，行为与边界见 `docs/desktop-mvp-spec.md`。全局速记快捷键在 macOS 上是 `Cmd+Shift+Space`，其他平台是 `Ctrl+Shift+Space`；速记窗口中 `Cmd/Ctrl+Enter` 保存，`Esc` 隐藏。

Capture 是个人收件箱，存放在 `~/.todo/inbox.md`，不属于任何项目，也没有完成状态。分拣后才会成为当前项目的 Project Task。该文件不进入项目 Git，也不受项目 `.todo/history/` 还原点保护，需要单独备份。

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

- 每个仓库的 Project Task 以该仓库的 Markdown 为唯一事实源；个人 Capture 另以 `~/.todo/inbox.md` 为事实源。工具只修改自己认识的字节区间，其余字节原样保留。
- 稳定 ID 使用随机 base36，写在 HTML 注释中，不影响渲染。
- 写入走文件锁与原子替换；检测到外部修改时放弃本次写入并要求重试。
- 桌面端直接复用 `todo-core`，不启动 CLI 子进程。
- v1 不包含 MCP、云同步和团队空间。不做密码账号：桌面端以授权 key（试用为邮箱激活）作为身份，仅用于授权校验。

决策记录见 `DECISIONS.md` 与 `docs/adr/`。
