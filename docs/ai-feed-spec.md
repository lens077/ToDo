# AI Feed 契约 v1

`ai-feed-spec: 1`。本文是 DSH（或其他 agent 宿主）向 ToDo 发布会话状态的唯一接口契约。三方实现——`todo ai` CLI、桌面端「AI」视图、DSH 外部扩展 `dsh-todo-bridge`——只依赖本文，不依赖彼此的源码。

已定决策（2026-09-11）：存储全局；v1 单向（宿主 → ToDo）；DSH 桥接作为外部扩展分发；一个会话一条记录。

## 1. 记录

一条记录描述一个 agent 会话的当前状态。主键是 `(source, session_id)`。每次 `upsert` 用新记录整条替换旧记录；ToDo 本地字段（§1.2）按 §3 规则保留。

### 1.1 宿主写入字段

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `ai-feed-spec` | `1` | 是 | 契约版本。其他值被拒绝，退出码 `2` |
| `source` | string | 是 | 来源标识，小写字母、数字、`-`，1–32 字节。DSH 使用 `dsh` |
| `session_id` | string | 是 | 来源内唯一的会话 ID，1–128 字节 |
| `workspace` | string | 是 | 会话工作目录的绝对路径。桌面端据此匹配当前项目 |
| `state` | string | 是 | `running` / `waiting` / `unread` / `seen` / `failed` / `handled` |
| `outcome` | string \| null | 否 | `completed` / `error` / `aborted` / `blocked` / `max-tokens` / `interrupted`；回合未结束时为 `null` |
| `title` | string \| null | 否 | 会话标题，≤ 200 字节 |
| `question` | string \| null | 否 | 最新一个人类提问，≤ 2000 字节；来源负责截断 |
| `reply` | string \| null | 否 | 该提问的收尾回答，≤ 4000 字节；来源负责截断 |
| `reply_truncated` | bool | 否 | 回答是否被来源截断；缺省 `false` |
| `changed_files` | string[] | 否 | 本回合改动的文件路径（相对 `workspace`），≤ 32 项 |
| `changed_file_count` | integer | 否 | 改动文件总数，可大于 `changed_files.len()`；缺省等于列表长度 |
| `open_url` | string \| null | 否 | 打开该会话的链接；`http`/`https` |
| `event_at` | integer | 是 | 来源侧状态变化时间，Unix epoch 毫秒 |

超过长度上限的字符串字段由 CLI 截断到上限，不拒绝；`changed_files` 超过 32 项时截断列表并保留 `changed_file_count`。

### 1.2 ToDo 本地字段

由 ToDo 写入，宿主传入时忽略：

| 字段 | 类型 | 说明 |
|---|---|---|
| `received_at` | integer | CLI 写入时间，epoch 毫秒 |
| `acked_at` | integer \| null | 用户在 ToDo 中标记已读的时间 |
| `hidden` | bool | 用户在 ToDo 中移除；`list` 默认不显示 |
| `task_id` | string \| null | 关联的 Project Task 稳定 ID |

## 2. 状态

| `state` | 含义 | DSH 汇总面板分类 |
|---|---|---|
| `running` | 回合进行中；`outcome` 应为 `null` | `running` |
| `waiting` | agent 等待用户批准、选择或回答 | `needsYou` |
| `unread` | 回合已结束，用户未在宿主中查看 | `unread` |
| `seen` | 用户已查看，未标记处理 | `seen` |
| `failed` | 回合以非 `completed` 结局结束 | `failed` |
| `handled` | 用户已在宿主中标记处理 | `handled` |

宿主的「稍后再看」（snoozed）不发布；已归档、空白、子代理会话不发布。

排序：`waiting` → `failed` → `unread` → `running` → `seen` → `handled`。同一状态内按 `event_at` 降序。

## 3. 写入规则

1. `event_at` 小于已存记录 `event_at` 的 `upsert` 忽略，返回 `unchanged: true`，退出码 `0`。相等时按新记录覆盖（允许同一时刻的字段修正）。
2. `event_at` 大于已存记录时，`acked_at` 清空，`hidden` 与 `task_id` 保留。
3. 记录以外的顶层字段忽略，不报错，便于宿主先行扩展。
4. 缺少必填字段、类型错误、枚举值未知：退出码 `2`，不写入。

## 4. 存储

- 路径：`~/.todo/ai/sessions.json`。
- 文件锁：`~/.todo/ai/.sessions.json.lock`，超时遵循 `--lock-timeout-ms`（默认 2000 ms）。
- 写入：临时文件 + 原子替换。
- 文件格式：

```json
{
  "ai-feed-spec": 1,
  "sessions": [ { …记录… } ]
}
```

- 保留：每个 `workspace` 最多 200 条，超出按 `event_at` 升序丢弃；`handled` 或 `hidden` 且 `event_at` 早于 30 天的记录在 `prune` 时清理。

## 5. CLI 接口

```
todo ai upsert --stdin
todo ai upsert --source <s> --session <id> --workspace <路径> --state <状态>
              [--outcome <值>] [--title <文本>] [--question <文本>] [--reply <文本>]
              [--reply-truncated] [--changed-file <路径>]... [--changed-file-count <n>]
              [--open-url <url>] [--event-at <ms>]
todo ai list [--workspace <路径>] [--state <状态>]... [--all]
todo ai get <source> <session>
todo ai ack <source> <session>
todo ai remove <source> <session>
todo ai link <source> <session> <task-id>
todo ai prune [--older-than <天>]
```

- `--stdin` 读取一个 JSON 对象（§1.1）。`--event-at` 缺省为当前时间。
- `list --workspace` 匹配 `workspace` 等于该路径或位于其下；默认排除 `handled` 与 `hidden`，`--all` 包含。
- `--json` 输出与现有 envelope 一致：`{"ok":true,"data":…}`；`upsert` 的 `data` 为 `{ "record": …, "unchanged": bool }`。
- 退出码：`0` 成功、`1` 一般错误、`2` 用法或契约错误、`4` 锁超时。

## 6. 宿主实现要求

- 只调用 `todo ai upsert`，不直接读写 `sessions.json`。
- 同一会话的连续变化应去抖（建议 300 ms），避免每个流式 token 触发写入。
- `todo` 不在 PATH 或返回非零：记录日志，不影响会话；连续失败后暂停发布并提示一次。
- `event_at` 使用来源侧事件时间，不用发送时间，保证乱序保护有效。

## 7. 示例

```bash
printf '%s' '{
  "ai-feed-spec": 1,
  "source": "dsh",
  "session_id": "s_8f2a",
  "workspace": "/Users/lens/lens077/ToDo",
  "title": "为 CLI 增加 release 构建说明",
  "state": "unread",
  "outcome": "completed",
  "question": "补一段 cargo install 的说明",
  "reply": "已在 README 快速开始加入 cargo install --path …",
  "changed_files": ["README.md"],
  "open_url": "http://127.0.0.1:3080/?session=s_8f2a",
  "event_at": 1757550000000
}' | todo ai upsert --stdin --json
```
