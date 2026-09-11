# AI 面板与 DSH 对接方案

状态：已拍板（2026-09-11），实施中。决策：存储全局 `~/.todo/ai/`；v1 单向；DSH 桥接作为外部扩展分发；一个会话一条记录；阶段 1、2、3b 并行。字段与写入规则以 `docs/ai-feed-spec.md` 为准。本文规划三件事：`todo ai` CLI 子命令、桌面端「AI」一级视图、以及 DeepSeek Harness（DSH）向 ToDo 发布会话状态的桥接。三者只通过一份版本化的 JSON 契约耦合，可以分开开发。

## 1. 问题

DSH 的汇总面板已经按会话汇总了「运行中 / 等你回复 / 完成未读 / 已完成 / 失败」。这些状态只在 DSH 自己的 GUI 里可见。ToDo 是开发者的任务工作台，需要在同一个窗口里看到「agent 在替我做什么、做完了没有、哪些出错要我处理」，并把有价值的结果转成 Project Task。

## 2. 边界

- **AI 条目不是 Project Task。** 它描述一个 agent 会话的当前状态，不进 `TODO.md`、不进 Git、不受 `.todo/history/` 还原点保护。混进 `TODO.md` 会让每次 agent 回合都产生仓库 diff，违反「只修改工具识别的字节区间」的承诺。
- **单向发布。** v1 只做 DSH → ToDo。ToDo 侧的「已读 / 已处理」是本地标记，不回写 DSH。双向同步列为待定项（§9）。
- **CLI 是唯一写入口。** DSH 只调用 `todo ai ...`，不直接写存储文件。桌面端与 `todo serve` 只读该存储，不接受来自 DSH 的 HTTP 调用；`todo serve` 不一定在运行。
- **来源无关。** 契约字段用 `source` 区分来源，DSH 是第一个来源；Claude Code、Codex 以后可以用同一 CLI 发布。

## 3. 数据模型

一个 AI 条目对应一个 agent 会话（DSH 的 Session），以 `(source, session_id)` 为主键。每次 upsert 覆盖整条记录；条目不保留回合历史。

```jsonc
{
  "ai-feed-spec": 1,
  "source": "dsh",
  "session_id": "s_8f2a…",           // 来源内的会话 ID
  "workspace": "/Users/lens/lens077/ToDo",   // 会话工作目录，绝对路径；用于匹配当前项目
  "title": "为 CLI 增加 release 构建说明",   // 会话标题，可空
  "state": "unread",                  // running | waiting | unread | seen | failed | handled
  "outcome": "completed",             // completed | error | aborted | blocked | max-tokens | interrupted | null
  "question": "…",                    // 最新一个人类提问，来源已截断
  "reply": "…",                       // 该提问的收尾回答，来源已截断
  "reply_truncated": false,
  "changed_files": ["crates/todo-cli/src/main.rs"],
  "changed_file_count": 3,
  "open_url": "http://127.0.0.1:3080/?session=s_8f2a…",   // 打开会话的链接，由来源提供
  "task_id": null,                    // 关联的 Project Task 稳定 ID，可空
  "event_at": 1757550000000,          // 来源侧状态变化时间，epoch ms
  "received_at": 1757550000100        // CLI 写入时间，由 ToDo 填写
}
```

状态与 DSH 汇总面板分类的对应：

| DSH 分类 | `state` | 含义 |
|---|---|---|
| `running` | `running` | 回合进行中，`outcome` 为 `null` |
| `needsYou` | `waiting` | agent 等待用户批准或回答 |
| `unread` | `unread` | 回合已结束且用户未在 DSH 中查看 |
| `seen` | `seen` | 已查看但未标记处理 |
| `failed` | `failed` | `outcome` 不是 `completed` |
| `handled` | `handled` | 用户已在 DSH 中标记处理 |
| `snoozed` | 不发布 | v1 不同步稍后再看 |

ToDo 本地另有两个字段，不由来源写入：`acked_at`（本地已读）和 `hidden`（本地移除）。来源再次 upsert 且 `event_at` 前进时，`acked_at` 清空，`hidden` 保持。

## 4. 存储

- 位置：`~/.todo/ai/sessions.json`，全局单文件，与 `~/.todo/inbox.md` 同级。
- 写入：文件锁 `~/.todo/ai/.lock` + 临时文件原子替换，复用 `todo-core` 的 `lock.rs` 与 `write.rs` 的原子替换路径。
- 幂等：`event_at` 小于已存记录的 upsert 被忽略并返回 `unchanged: true`，避免乱序的 DSH 事件回退状态。
- 容量：默认保留每个 `workspace` 最近 200 条，`handled` 与 `hidden` 条目超过 30 天自动清理；`todo ai prune` 可手动执行。
- 不用 SQLite：条目数量小，JSON 快照直接读取更简单；SQLite 投影原则不变。

选全局而不是 `<project>/.todo/ai/` 的理由：DSH 工作区不一定是 ToDo 项目根，也不一定有 `TODO.md`；CLI 不向上级目录搜索，按 cwd 写入会把状态写进不相干的目录。全局存储由桌面端按 `workspace` 前缀匹配当前项目。

## 5. CLI：`todo ai`

| 命令 | 用途 |
|---|---|
| `todo ai upsert --stdin` | 从 stdin 读取一条 §3 的 JSON 并写入；DSH 桥接的主路径 |
| `todo ai upsert --source dsh --session <id> --workspace <路径> --state <状态> [--outcome <值>] [--title …] [--question …] [--reply …] [--open-url …] [--event-at <ms>]` | 参数式写入；供 shell hook 使用 |
| `todo ai list [--workspace <路径>] [--state <状态>] [--all]` | 列出条目；默认过滤 `handled` 与 `hidden`，`--all` 包含 |
| `todo ai get <source> <session>` | 查看单条 |
| `todo ai ack <source> <session>` | 本地标记已读 |
| `todo ai remove <source> <session>` | 本地隐藏 |
| `todo ai link <source> <session> <task-id>` | 关联 Project Task |
| `todo ai prune [--older-than <天>]` | 清理过期条目 |

约束：

- 所有命令支持全局 `--json` 与 `--lock-timeout-ms`，退出码沿用现有约定（`0` 成功、`2` 用法错误、`4` 锁超时）。
- `upsert` 校验 `ai-feed-spec` 版本；未知版本退出码 `2`，不写入。
- `upsert` 是快路径：DSH 每个状态变化都会调用一次，必须避免读取 `TODO.md` 或 Git。目标是单次调用低于 50 ms〔待实测〕。
- 不新增 `todo ai` 对 `TODO.md` 的任何写入；「转为任务」走现有 `todo add`。

## 6. 桌面端：「AI」视图

侧栏「工作视图」新增一级入口「AI」，位于「随记」之后。入口右侧显示需要关注的数量（`waiting + failed + unread`）。

布局：

- 顶部状态胶囊：运行中 / 等你回复 / 完成未读 / 失败 / 已处理，点击筛选。默认隐藏 `handled`。
- 范围切换：「当前项目」（`workspace` 等于或位于当前项目根之下）与「所有工作区」。
- 分区顺序与 DSH 一致：等你回复、失败、完成未读、运行中、已读、已处理。
- 卡片：状态标记、标题、工作区相对名、提问、回答预览、改动文件、更新时间。
- 卡片动作：**打开会话**（用系统默认浏览器打开 `open_url`）、**转为任务**（用当前项目的 `todo add` 语义创建 Project Task，`↳` 说明写入会话链接与来源，并回写 `task_id`）、**标记已读**、**移除**。
- 空状态区分「尚未接入 DSH」（给出 `todo hooks instructions --agent deepseek-harness` 的提示）与「当前项目没有会话」。

数据通道：

- Tauri：新增命令 `ai_list` / `ai_ack` / `ai_remove` / `ai_link` / `ai_to_task`，直接调用 `todo-core`；`watch.rs` 增加对 `~/.todo/ai/` 目录的监听，变化时向前端推送刷新信号，与 `TODO.md` 监听共用一条通道。
- `todo serve`：新增 `GET /api/v1/ai/sessions`、`POST /api/v1/ai/sessions/{source}/{id}/ack|remove|link|to-task`；只读模式下写入端点返回 403。前端在 Web 传输下每 5 秒轮询〔间隔待定〕。
- mock 传输：提供固定夹具，覆盖 E2E。

## 7. DSH 侧桥接

两条路径，按保真度递进；契约相同，可以先上路径 A。

### 路径 A：hooks（无需改 DSH）

通过现有 `dsh-hooks-claude-code` 桥接，把 `UserPromptSubmit` 映射为 `state=running`，`Stop` 映射为 `state=unread` 或 `failed`。`todo hooks install --agent deepseek-harness` 负责写入 hooks 配置。

限制：hook 事件不包含 DSH 的「已查看 / 已处理」标记，也不包含「等你回复」；`unread` 会一直停留到用户在 ToDo 里本地标记。开发成本最低，作为保底。

### 路径 B：DSH 宿主插件 `dsh-todo-bridge`（推荐）

在 DSH 仓库新增一个宿主侧 Cordis 插件：

- 订阅 `ctx.sessionProjections.onChanged`（key 为 `sessionDigest`）与 `session-inbox/changed`。
- 复用 `ui-digest/src/client/select.ts` 的分类规则。该规则目前在客户端包内，需要提取到宿主可用的共享位置，或在插件内实现同一规则并用同一套测试夹具校验。
- 按会话去抖（约 300 ms〔待定〕）后，通过 `ctx.shell` 执行 `todo ai upsert --stdin`，stdin 为 §3 JSON；`open_url` 由插件根据 Web 服务地址生成。
- 配置项：`enabled`、`todoBinary`（默认 `todo`，PATH 中查找）、`workspaces`（可选白名单）。
- 失败处理：CLI 不存在或非零退出只记日志，不影响会话；连续失败 N 次后暂停并提示一次。
- 排除子代理会话、空白会话与已归档会话，与汇总面板一致。

路径 B 是「DSH 单独开发」的部分，只依赖 §3 契约和 §5 的 `upsert` 命令。

实现位置：`integrations/dsh-todo-bridge/`（独立 npm 包，不在根 `pnpm-workspace.yaml` 内），以 DSH 外部扩展形式分发，安装命令为 `dsh plugin --profile web add file:<绝对路径>`。分类规则在 `src/classify.ts` 复刻并用夹具校验；「等你回复」以宿主侧 `approval/request` 与 `user-questions/request` 两条 waterfall 的进入与结束近似；CLI 经 `ctx.subprocess` 启动（无该服务时退回 `node:child_process`）；`open_url` 目前只是 Web 服务基地址，因为 DSH Web 客户端尚无按会话直达的 URL。配置、信号对应表与限制见该目录的 `README.md`。

## 8. 分期

| 阶段 | 交付 | 验收 |
|---|---|---|
| 0 | 本文定稿；契约 JSON Schema 落到 `docs/ai-feed-spec.md` | 双方确认字段与状态映射 |
| 1 | `todo-core` 新增 `ai` 模块（模型、存储、锁、幂等）；`todo ai` 全部子命令 | `cargo test`：并发 upsert、乱序 `event_at`、prune；`--json` 契约测试 |
| 2 | 桌面端「AI」视图；Tauri 命令与目录监听；`todo serve` 端点；mock 夹具与 Playwright 用例 | `pnpm e2e` 覆盖筛选、动作、空状态；人工验收目录监听刷新 |
| 3a | `todo hooks install --agent deepseek-harness` 写入路径 A 的 hooks | 真实 DSH 会话结束后 ToDo 出现条目 |
| 3b | DSH 仓库 `dsh-todo-bridge` 插件 | DSH 中标记已读 / 已处理后 ToDo 状态同步变化 |
| 4 | 「转为任务」与 `task_id` 关联；README 与 `docs/desktop-mvp-spec.md` 更新 | 人工验收 |

阶段 1 与 2 可以并行；阶段 3b 与 1 可以并行，只要契约先定。

## 9. 决策记录

| 问题 | 决定 |
|---|---|
| 存储位置 | 全局 `~/.todo/ai/sessions.json` |
| ToDo 侧「已读 / 已处理」回写 DSH | v1 不回写；双向同步列为 v2 |
| DSH 桥接位置 | 外部扩展 `dsh-todo-bridge`，独立于 DSH 仓库；分类规则在扩展内复刻并用夹具校验 |
| 条目粒度 | 一个会话一条 |
| 分期 | 阶段 1（core + CLI）、阶段 2（桌面视图）、阶段 3b（DSH 扩展）并行，只共享 `docs/ai-feed-spec.md` |

已确认：DSH Web 客户端（`packages/client/*`）目前不解析会话 ID 的 URL 参数或 hash，不支持按会话直达。v1 的 `open_url` 为 DSH GUI 根地址（默认 `http://127.0.0.1:3080`）；卡片同时展示 `session_id`，用户在 DSH 汇总面板中定位。DSH 后续提供深链接时，桥接只需改 `open_url` 的拼接，契约不变。
