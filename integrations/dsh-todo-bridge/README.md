# dsh-todo-bridge

DeepSeek Harness（DSH）宿主侧扩展，把汇总面板里每个会话的状态发布给 ToDo CLI。它是 `docs/ai-panel-spec.md` §7 的「路径 B」实现，只依赖 `docs/ai-feed-spec.md`（`ai-feed-spec: 1`）；唯一的输出动作是运行 `todo ai upsert --stdin`，stdin 为一条 §1.1 的 JSON 记录。

适用对象：在本机同时运行 `dsh web` 与 ToDo 的开发者。安装后无需改动 DSH 仓库，也不改动 ToDo 仓库的构建。

## 做什么

- 订阅 `ctx.sessionProjections.onChanged`（`sessionDigest`、`title`、`sessionListMetadata`）、`session-inbox/changed`、`agent/status`、`approval/request` 与 `user-questions/request` 两条 waterfall，以及工作区归档集合的变化。
- 按 DSH 客户端 `ui-digest/src/client/select.ts` 的规则在宿主侧复刻分类（`unread | seen | running | needsYou | failed | handled | snoozed`），再映射为契约 `state`：`running → running`、`needsYou → waiting`、`unread → unread`、`seen → seen`、`failed → failed`、`handled → handled`；`snoozed`、已归档、空白、子代理会话不发布。
- 按会话去抖（默认 300 ms）并合并为最新一条；同一会话串行发送，旧记录不会追过新记录；内容（`event_at` 以外）与上次成功发布相同的记录不再发送，避免无谓推进 `event_at` 而清掉 ToDo 侧的 `acked_at`。
- `event_at` 取来源侧事件时间的最大值：摘要的 `questionAt` / `repliedAt`、收件箱行的 `updatedAt`、宿主运行时信号（agent 状态翻转、审批或提问开始与结束、归档切换）的观测时间；不使用发送时刻。
- 插件启动时先发布一次快照：所有已挂载会话，以及 `sessionQuery` 列出、投影缓存中有 `sessionDigest` 行的冷会话。
- CLI 不存在或返回非零：写日志、计数；连续失败达到阈值后暂停发布并告警一次，之后每 60 秒最多探测一次，探测成功即恢复并清零计数。
- 卸载时取消订阅、清除定时器，并等待进行中的 CLI 进程结束。

## 安装

前置条件：

- `dsh` 可用，且 `web` 配置档已初始化（`~/.dsh/profiles/web`）。
- `todo` 在 DSH 进程的 PATH 中，或在配置里写绝对路径。`todo ai` 子命令由 ToDo 阶段 1 提供。
- Node.js `^22.19.0 || >=24`，pnpm。

在本目录构建：

```bash
cd /Users/lens/lens077/ToDo/integrations/dsh-todo-bridge
pnpm install
pnpm build          # 产出 lib/index.js（ESM）与 lib/index.d.ts
```

安装到 `web` 配置档（二选一）：

```bash
# 方式一：从本地目录安装（pnpm 会复制 lib、cordis.patch.yml、package.json）
dsh plugin --profile web add file:/Users/lens/lens077/ToDo/integrations/dsh-todo-bridge

# 方式二：从 tarball 安装
pnpm pack --out /tmp/dsh-todo-bridge.tgz
dsh plugin --profile web add file:/tmp/dsh-todo-bridge.tgz
```

`dsh plugin` 在配置档目录执行 pnpm，并因 `package.json` 的 `dsh.bundle` 声明把 `dsh-todo-bridge` 追加到 `dsh.profile.bundles`。之后重启 `dsh web` 使新的 bundle 层生效；配置档的补丁文件是实时重载的，但新增依赖不是。

卸载：

```bash
dsh plugin --profile web remove dsh-todo-bridge
```

说明：本目录不属于 ToDo 根 `pnpm-workspace.yaml`（只包含 `apps/*`）；目录内自带 `pnpm-workspace.yaml`，`pnpm install` 只作用于本目录。开发期依赖通过 `link:` 指向 DSH 源码检出 `/Users/lens/lens077/deepseek-harness`，只用于类型检查与测试；运行时只导入 `@deepseek-ai/schemastery`，由 DSH 安装的依赖闭包提供。

## 配置

bundle 补丁 `cordis.patch.yml` 插入 `id: todo-bridge` 一行。在 `~/.dsh/profiles/web/cordis.patch.yml` 里按 id 覆盖（id 定向补丁整键替换 config，需要把保留的字段一起写出）：

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
```

| 字段 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `enabled` | boolean | `true` | 为 `false` 时插件只写一条日志，不订阅任何事件 |
| `todoBinary` | string | `"todo"` | PATH 中的可执行名或绝对路径 |
| `workspaces` | string[] | `[]` | 绝对路径白名单；会话 `cwd` 等于或位于其下才发布；空表示全部 |
| `debounceMs` | integer ≥ 0 | `300` | 每会话合并窗口，毫秒 |
| `maxConsecutiveFailures` | integer ≥ 1 | `5` | 连续失败达到该值后暂停发布 |
| `openUrlTemplate` | string | `"{base}"` | `open_url` 模板；`{base}` 为 Web 服务回环地址，`{sessionId}` 为会话 ID |

禁用整行：`- id: todo-bridge` 加 `disabled: true`。

## 宿主侧信号与分类对应

| 契约 `state` | 面板分类 | 宿主侧信号 |
|---|---|---|
| `waiting` | `needsYou` | `approval/request`、`user-questions/request` waterfall 进入到结束之间（计划评审是一种 `user-questions` 请求） |
| `running` | `running` | `ctx.agents.get(sessionId).status === 'running'`，变化经 `agent/status` |
| `unread` | `unread` | 摘要 `replySeq ?? questionSeq` 大于收件箱 `lastSeenSeq` |
| `seen` | `seen` | 上式不成立，且未标记处理 |
| `failed` | `failed` | 摘要 `outcome !== 'completed'`（含回合未收尾但无 agent 运行的 `null`） |
| `handled` | `handled` | 收件箱 `handledAt != null` |
| 不发布 | `snoozed` | 收件箱 `snoozedUntil` 在未来 |
| 不发布 | — | `sessionListMetadata.blank`（无 `turn/start`）、`header.origin === 'subagent'`、`workspaceRegistry.archivedSessionIds` 含该会话 |

其他字段：`workspace` 取 `session.header.cwd`；`title` 取 `title` 投影；`question` / `reply` / `changed_files` / `outcome` 取 `sessionDigest` 投影，`changed_files` 相对 `workspace` 表示并截到 32 项；`open_url` 由 `ctx.webServer.port` 组成 `http://127.0.0.1:<port>`，无 Web 服务时为 `http://127.0.0.1:3080`。

## 限制

- 「等你回复」在客户端来自 `uiSession.pendingInteractions`，宿主侧没有同等对象。本插件以两条 waterfall 的进入与结束近似；若另有先于本插件注册且直接应答的 answerer（例如按策略自动放行的审批），则观察不到，且 300 ms 内即结束的请求不会发布 `waiting`。
- DSH Web 客户端目前没有按会话 ID 直达的 URL，`open_url` 默认只是基地址；`openUrlTemplate` 为将来支持深链预留。
- 不发布即不撤回：会话被归档、稍后再看或删除后，ToDo 里保留最后一条记录，直到下一次发布或本地移除。
- 冷会话（未挂载）只在投影缓存有 `sessionDigest` 行时才出现在启动快照里，与会话列表的行为一致。
- 收件箱行被整体清除时没有 `updatedAt` 可用，插件以观测时间作为 `event_at`；宿主重启后启动快照中的会话没有运行时信号，`event_at` 只来自摘要与收件箱时间。
- 宿主的 `ctx.subprocess` 会剔除凭据类环境变量与 `DSH_*` 变量后再启动 `todo`；PATH 保留。

## 验证

```bash
pnpm typecheck && pnpm build && pnpm test
```

测试覆盖：分类规则（对照 `select.ts` 用例）、记录构建与截断、发布器的去抖 / 串行 / 失败暂停 / 探测恢复、插件对宿主事件的接线（假 Context），以及在存在 `../../target/debug/todo` 时通过真实 CLI 写入隔离 `HOME` 的端到端用例。

安装到配置档后：

```bash
dsh --profile web --dump-config | grep -A 8 'id: todo-bridge'   # 确认层已合成
todo ai list                                                   # 重启 dsh web 后应看到会话
todo ai list --workspace /Users/lens/lens077/ToDo --json
```

不想影响正在运行的 `dsh web` 时，可以用独立的 Harness 主目录做演练：

```bash
export DSH_HOME=/tmp/dsh-home-test
dsh --profile todo-bridge-test --from-default-profile web --dump-config >/dev/null
dsh plugin --profile todo-bridge-test add file:/tmp/dsh-todo-bridge.tgz
HOME=/tmp/todo-home-test dsh --profile todo-bridge-test --port 0 --no-open --no-browser-auth
HOME=/tmp/todo-home-test todo ai list --all
```

## 目录

```
integrations/dsh-todo-bridge/
├── cordis.patch.yml      bundle 补丁：插入 id: todo-bridge 行
├── package.json          dsh.bundle 声明、peer 依赖、脚本
├── src/
│   ├── index.ts          Cordis 插件：订阅、初始快照、open_url、卸载
│   ├── classify.ts       select.ts 规则的宿主侧复刻 + 契约映射
│   ├── record.ts         ai-feed-spec 记录构建与截断
│   ├── publisher.ts      去抖、串行、失败暂停与探测恢复
│   ├── spawn.ts          ctx.subprocess 与 node:child_process 两种启动器
│   └── types.ts          插件内部输入输出类型
└── tests/                vitest 用例与夹具
```
