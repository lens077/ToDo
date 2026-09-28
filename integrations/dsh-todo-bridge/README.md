# dsh-todo-bridge

DeepSeek Harness（DSH）扩展包，含宿主与 Web 客户端两半：宿主侧把汇总面板里每个会话的状态发布给 ToDo CLI，客户端侧在 Web GUI 里解析 `?session=<id>` 深链。它是 `docs/ai-panel-spec.md` §7 的「路径 B」实现，只依赖 `docs/ai-feed-spec.md`（`ai-feed-spec: 1`）；宿主侧唯一的输出动作是运行 `todo ai upsert --stdin`，stdin 为一条 §1.1 的 JSON 记录。

适用对象：在本机同时运行 `dsh web` 与 ToDo 的开发者。安装后无需改动 DSH 仓库，也不改动 ToDo 仓库的构建。

## 做什么

- 订阅 `ctx.sessionProjections.onChanged`（`sessionDigest`、`title`、`sessionListMetadata`）、`session-inbox/changed`、`agent/status`、`approval/request` 与 `user-questions/request` 两条 waterfall、工作区归档集合变化，以及 `session/created` / `session/disposed` 生命周期变化；这保证 DSH 释放 live session 后不会把 ToDo 留在旧的「运行中」。
- 按 DSH 客户端 `ui-digest/src/client/select.ts` 的规则在宿主侧复刻分类（`unread | seen | running | needsYou | failed | handled | snoozed`），再映射为契约 `state`：`running → running`、`needsYou → waiting`、`unread → unread`、`seen → seen`、`failed → failed`、`handled → handled`；`snoozed`、已归档、空白、子代理会话不发布。
- 按会话去抖（默认 300 ms）并合并为最新一条；同一会话串行发送，旧记录不会追过新记录；内容（`event_at` 以外）与上次成功发布相同的记录不再发送，避免无谓推进 `event_at` 而清掉 ToDo 侧的 `acked_at`。
- `event_at` 取来源侧事件时间的最大值：摘要的 `questionAt` / `repliedAt`、收件箱行的 `updatedAt`、宿主运行时信号（agent 状态翻转、审批或提问开始与结束、归档切换）的观测时间；不使用发送时刻。
- 插件启动时先发布一次快照：所有已挂载会话，以及 `sessionQuery` 列出、投影缓存中有 `sessionDigest` 行的冷会话。
- CLI 不存在或返回非零：写日志、计数；连续失败达到阈值后暂停发布并告警一次，之后每 60 秒最多探测一次，探测成功即恢复并清零计数。
- 卸载时取消订阅、清除定时器，并等待进行中的 CLI 进程结束。
- 客户端半（`lib/client.js`）：Web GUI 启动时读取 `location.search` 的 `session` 参数，等到会话列表里出现该 id 后调用 `ctx.sessions.open(id)`，再用 `history.replaceState` 去掉参数，刷新页面不会再次跳转。列表首次拉取已完成但没有该 id 时补发一次列表刷新；15 秒内仍未出现则写一条 warn 放弃。

## 会话深链

`open_url` 默认为 `{base}/?session={sessionId}`，例如 `http://127.0.0.1:3080/?session=session-69ebcc9b-…`。`sessionId` 即宿主 `session.header.id`（`session-<uuid>`），与 Web 客户端会话列表行的 `sessionId` 相同。

ToDo 面板的「打开会话」用系统浏览器打开这条链接。DSH Web 壳没有前端路由，同一标签页被导航到新的 `?session=` 地址时是整页加载，所以启动时执行一次即可覆盖 ToDo 的两种设置：「打开会话」→ 复用同一窗口（同一标签页重新加载到新地址）与每次新开标签页。

未安装客户端半（例如 DSH 版本不支持 `dsh.client`）时，链接仍然打开 GUI 根页并恢复上次选中的会话，`?session` 参数被忽略。

## 安装

前置条件：

- `dsh` 可用，且 `web` 配置档已初始化（`~/.dsh/profiles/web`）。
- `todo` 在 DSH 进程的 PATH 中，或在配置里写绝对路径。`todo ai` 子命令由 ToDo 阶段 1 提供。
- Node.js `^22.19.0 || >=24`，pnpm。

在本目录构建：

```bash
cd /Users/lens/lens077/ToDo/integrations/dsh-todo-bridge
pnpm install
pnpm build          # 产出 lib/index.js（宿主，ESM）、lib/client.js（客户端）与 .d.ts
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

`file:` 安装是复制而非符号链接（`node_modules/dsh-todo-bridge` 是普通目录）：本目录重新 `pnpm build` 后，需要再执行一次同样的 `add` 命令把新的 `lib/` 复制进配置档，然后重启 `dsh web`。客户端半由宿主启动时扫描 `dsh.client` 声明后合成进页面，同样只在重启后生效。

卸载：

```bash
dsh plugin --profile web remove dsh-todo-bridge
```

说明：本目录不属于 ToDo 根 `pnpm-workspace.yaml`（只包含 `apps/*`）；目录内自带 `pnpm-workspace.yaml`，`pnpm install` 只作用于本目录。开发期依赖通过 `link:` 指向 DSH 源码检出 `/Users/lens/lens077/deepseek-harness`，只用于类型检查与测试；宿主半运行时只导入 `@deepseek-ai/schemastery`，由 DSH 安装的依赖闭包提供；客户端半只有类型导入，不从浏览器模块表 `require` 任何模块。

客户端半的声明方式与 `@zseven-w/dsh-openpencil` 相同：`package.json` 的 `dsh.client`（`platform: web`，`inject` 列出 `@deepseek-ai/dsh-api-session-controller`）加 `exports["./client"]` 指向 `lib/client.js`；产物是 `window.__ModuleLoader__.load({ id: 'dsh-todo-bridge', factory })` 形式的 CJS 工厂，由 `tsdown.config.ts` 的第二个条目生成。宿主启动时扫描到该声明即把它并入 Web 页面的插件图，`cordis.patch.yml` 不需要为它加行。

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
    openUrlTemplate: '{base}/?session={sessionId}'
```

| 字段 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `enabled` | boolean | `true` | 为 `false` 时插件只写一条日志，不订阅任何事件 |
| `todoBinary` | string | `"todo"` | PATH 中的可执行名或绝对路径 |
| `workspaces` | string[] | `[]` | 绝对路径白名单；会话 `cwd` 等于或位于其下才发布；空表示全部 |
| `debounceMs` | integer ≥ 0 | `300` | 每会话合并窗口，毫秒 |
| `maxConsecutiveFailures` | integer ≥ 1 | `5` | 连续失败达到该值后暂停发布 |
| `openUrlTemplate` | string | `"{base}/?session={sessionId}"` | `open_url` 模板；`{base}` 为 Web 服务回环地址，`{sessionId}` 为 URL 编码后的会话 ID。写成 `"{base}"` 可退回只打开 GUI 根页 |

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

其他字段：`workspace` 取 `session.header.cwd`；`title` 取 `title` 投影；`question` / `reply` / `changed_files` / `outcome` 取 `sessionDigest` 投影，`changed_files` 相对 `workspace` 表示并截到 32 项；`open_url` 由 `ctx.webServer.port` 组成 `http://127.0.0.1:<port>` 代入 `{base}`，无 Web 服务时为 `http://127.0.0.1:3080`。

`changed_files` 的相对化按三步进行：路径位于 `workspace` 或其 realpath 之下时去掉该前缀；否则在路径里查找 `workspace` 末尾若干段（先长后短，至少含末段目录名）首次出现的位置，去掉到该处为止的部分（例如工作区 `/Users/lens/lens077/kubernetes`，会话日志里记录的是 `/Users/sumery/lens077/kubernetes/components/gateway/README.md`，得到 `components/gateway/README.md`）；两者都不成立则原样保留。

## 限制

- 「等你回复」在客户端来自 `uiSession.pendingInteractions`，宿主侧没有同等对象。本插件以两条 waterfall 的进入与结束近似；若另有先于本插件注册且直接应答的 answerer（例如按策略自动放行的审批），则观察不到，且 300 ms 内即结束的请求不会发布 `waiting`。
- 深链依赖本包的客户端半被 Web GUI 加载；未加载时 `?session` 被忽略，只打开上次选中的会话。链接指向的会话已被删除时，客户端在 15 秒后放弃并写一条 warn。
- 不发布即不撤回：会话被归档、稍后再看或删除后，ToDo 里保留最后一条记录，直到下一次发布或本地移除。
- 冷会话（未挂载）只在投影缓存有 `sessionDigest` 行时才出现在启动快照里，与会话列表的行为一致。
- 收件箱行被整体清除时没有 `updatedAt` 可用，插件以观测时间作为 `event_at`；宿主重启后启动快照中的会话没有运行时信号，`event_at` 只来自摘要与收件箱时间。
- 宿主的 `ctx.subprocess` 会剔除凭据类环境变量与 `DSH_*` 变量后再启动 `todo`；PATH 保留。

## 验证

```bash
pnpm typecheck && pnpm build && pnpm test
```

测试覆盖：分类规则（对照 `select.ts` 用例）、记录构建与截断、`changed_files` 相对化的三种分支、发布器的去抖 / 串行 / 失败暂停 / 探测恢复、插件对宿主事件的接线（假 Context）、客户端半对假 `ctx.sessions` 的行为（立即命中、稍后命中、超时放弃、参数剥离），以及在存在 `../../target/debug/todo` 时通过真实 CLI 写入隔离 `HOME` 的端到端用例。客户端半没有浏览器内的自动化验证；安装后可用 `dsh --profile web --dump-config` 确认行存在，再在浏览器里打开一条 `open_url` 检查是否落到对应会话。

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
├── package.json          dsh.bundle / dsh.client 声明、peer 依赖、脚本
├── tsdown.config.ts      两个产物：lib/index.js（宿主 ESM）与 lib/client.js（客户端 CJS 工厂）
├── src/
│   ├── index.ts          Cordis 宿主插件：订阅、初始快照、open_url、卸载
│   ├── client/index.ts   Cordis 客户端插件：?session 深链
│   ├── classify.ts       select.ts 规则的宿主侧复刻 + 契约映射
│   ├── record.ts         ai-feed-spec 记录构建与截断
│   ├── publisher.ts      去抖、串行、失败暂停与探测恢复
│   ├── spawn.ts          ctx.subprocess 与 node:child_process 两种启动器
│   └── types.ts          插件内部输入输出类型
└── tests/                vitest 用例与夹具
```
