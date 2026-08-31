# ToDo 接入 QQ 机器人开放平台

Status: wontfix

> 勘察与撰写：2026-09-01，AgentTeams `qqbot-integration` / project-scout。
> 平台事实来自队长核实的 <https://bot.q.qq.com/wiki/> 结论，本文不重复联网核对。
>
> **`wontfix` 的确切范围**：不建议**在 ToDo 仓内直接接入** QQ 机器人。
> 「手机提醒 + 通知上直接完成/推迟」这个**需求本身是成立的**，而且已经在
> `DECISIONS.md` 决策③ 里排了期——只是它的正确实现不是在本仓写一个 QQ 客户端。
> 详见 §5。

## 1. 这个项目是什么

面向开发者的**本地优先**任务工具。仓库里的 `TODO.md` 是唯一事实源，CLI 是标准接口，
桌面端只是视图和安全编辑入口。

| 结论 | 依据文件 |
|---|---|
| Markdown 是唯一事实源，SQLite 只是可重建的派生投影 | `README.md` 首段与「设计约束」 |
| **本地模式不需要登录，也不需要网络** | `README.md` 第 5 行 |
| **v1 不包含 MCP、账号、云同步和团队空间** | `README.md`「设计约束」末条 |
| 交付物是 CLI（`crates/todo-cli`，二进制 `todo`）+ Tauri 2 桌面端 | `README.md` 目录结构 |
| 核心差异化是**对账引擎**：用 Git 物证核对任务与代码状态，只报告不改写 | `README.md` 第 7 行；`CONTEXT.md` Evidence / Reconciliation Finding |
| Capture（个人收件箱）在 `~/.todo/inbox.md`，不属于任何项目、无完成状态 | `README.md`「桌面端」末段；`CONTEXT.md` Capture |
| 当前进度：10 条待办，含 3 条人工验收、1 条 Doing（用自身管理开发） | `TODO.md` |

**决定性的勘察事实**——`Cargo.toml` 的 `[workspace.dependencies]` 全表如下：

```
pulldown-cmark, serde, serde_yaml, thiserror, proptest, tempfile,
rusqlite, fs2, clap, serde_json, getrandom, chrono, dirs
```

没有 `tokio`、没有 `axum`/`hyper`、**没有 `reqwest`**。对 `crates/`、`apps/` 下所有 `.rs`
grep `axum|hyper|tokio::net|TcpListener|reqwest`，**零命中**。

这不是「还没加」，这是产品定义：一个没有网络栈的本地工具。

## 2. QQ 机器人对它有没有价值

**需求有价值，但在本仓直接接入没有价值，且与产品定义冲突。** 三条理由：

### 2.1 没有常驻进程可以承载机器人

QQ 机器人的两种事件接入方式都要求一个**长期在线的进程**：

- WebSocket：纯出站长连接，`op=1` 心跳、断线靠 `op=6 Resume` 补发；
- Webhook：一个持续监听 80/443/8080/8443 的 HTTPS 服务。

而 ToDo 的两个形态都不满足：

- **CLI 是一次性进程**——`todo add` 跑完就退出。`README.md` 明确 CLI 在当前工作目录查找
  `TODO.md`、不向上搜索，它连「有哪些项目」这个全局视图都没有。
- **桌面端只在你打开它时运行**，且 `README.md` 定位它是「视图和安全编辑入口」。
  任务到期提醒恰恰要在你**没打开**它的时候送达。

要接机器人就得先造一个常驻守护进程。那是一个新组件，不是一次集成。

### 2.2 会正面推翻两条写死的设计约束

`README.md`「设计约束」最后一条是 v1 不包含**账号、云同步**；第 5 行是本地模式
**不需要网络**。QQ 机器人要求 AppID + AppSecret 鉴权、要求出站连腾讯服务器、
要求把任务标题送到第三方服务器上——三条全撞。

`DECISIONS.md` 决策② 更把这件事的商业含义写明了：信任的最低要求是
「格式 spec 公开 + 数据永远是本地纯文本、可随时迁出」，并把
**同步服务、推送**明确划进 L3 商业层。也就是说推送不是「顺手加的功能」，
它是收费点，得按产品节奏走，不该由一次技术集成偷跑。

### 2.3 这件事已经有指定的承担者，就在隔壁

`DECISIONS.md` 决策③「移动端形态」的推荐方案，P0 阶段原文：

> **P0（随 MVP）** | 无 App：ntfy/Bark 推送 + 通知按钮回传 |
> 到期提醒、任务完成通知、通知上直接「完成/推迟」 | **极低（cat 链路直接复用）**

同一份文件的「硬事实」里还写着：

> cat 已验证：PWA + Tauri Android 薄壳（原生 HTTPS transport）+ ntfy/Bark 推送 +
> HMAC 一次性 token 按钮回传，APK 已产出。

也就是说：**ToDo 早就决定了推送能力靠复用 cat，而不是自己实现。**
既然如此，「ToDo 要不要接 QQ」这个问题的正确形式是「**cat 要不要接 QQ**」——
接了，ToDo 免费继承；没接，ToDo 自己接也是重复造轮子。

cat 侧的评估见 `/Users/sumery/lens077/cat/.scratch/qqbot-integration/spec.md`（结论：有价值，P2）。

## 3. 若假设要做：场景与频控（备查，不建议执行）

留档以便将来 L3 商业层真的要做推送时有个起点。

| 场景 | 消息形态 | 主动/被动 | intent |
|---|---|---|---|
| 任务到期提醒（`due` 字段到点） | msg_type 0 + keyboard（完成／推迟 1 天） | **主动** | 下行不需要 intent |
| 通知上直接「完成/推迟」 | 按钮回调 → `todo done <id>` | 被动回执 | `INTERACTION`（1<<26） |
| 手机快速 Capture（发一句话进 `~/.todo/inbox.md`） | 单聊文本 | **被动回复** | `GROUP_AND_C2C_EVENT`（1<<25） |
| 对账发现不一致时提醒 | msg_type 0 | **主动** | — |

**频控**：任务提醒天然低频，主动消息每日 1000 条/用户、单关系 20/qpm 都够用。
真正的坑是**被动回复窗口**：单聊 60 分钟 / 4 次。手机 Capture 场景下用户可能连发多条想法，
第 5 条就回不了执了——只能靠「不逐条回执，攒一条汇总」绕开，体验会打折。

其中「手机快速 Capture」是唯一一个**只有 IM 才做得好**的场景（`CONTEXT.md` 定义 Capture
是「个人快速记录的原始文本，尚未承诺执行」，而 QQ 恰好是随手可达的输入口）。
若将来重新评估，这条是最值得先试的。

## 4. 接入代价评估

| 维度 | 结论 |
|---|---|
| **技术栈匹配** | ❌ 官方 SDK `botgo` 是 Go；本仓是 Rust + TS，用不上。 |
| **需新增依赖** | ❌ 代价最重的一项：要从零加 `tokio` + HTTP 客户端 + WS 客户端（+ Webhook 方案还要 ed25519）。当前 13 个 workspace 依赖里一个网络库都没有。 |
| **需新增组件** | ❌ 一个常驻守护进程（见 §2.1），含进程生命周期、开机自启、单实例互斥。 |
| **公网出口** | ⚠️ 需要稳定公网出口 IP 报白名单（新机器人默认启用，仅正式环境）。开发者的家用宽带 IP 通常是动态的。 |
| **备案域名** | 仅在消息带链接时需要；纯文本 + 按钮可规避。 |
| **与产品定位冲突** | ❌ 见 §2.2。这一项不是成本，是否决理由。 |

## 5. 优先级建议与替代路径

**优先级：不排期。** 本仓不实现 QQ 机器人。

推荐的替代路径，按依赖顺序：

1. **先等 cat 决定是否接 QQ**（cat 侧评估为 P2）。cat 已经是 ToDo 决策③ 指定的推送承担者。
2. **ToDo 侧真正要做的一件事**是：让 `todo` 能把「任务到期」这类事件**发给 cat**，
   而不是自己发给腾讯。cat 的本地事件网关收 `POST http://127.0.0.1:9527/event`，
   统一 envelope（`cat/docs/design/protocol.md` §1）。
   代价评估：一条 `curl`，**零新增 Rust 依赖**——这与 ToDo 现有的 hooks 机制同构
   （`README.md` 的 `todo hooks install`，以及 cat 那边 hook 就是一行 `curl … || true`）。
   *注意*：cat 的 `source` 枚举当前是 `claude-code | codex | dsh | pet-run | desktop |
   mobile | system | ci`，没有 `todo`。要么走 `pet-run`，要么请 cat 加一个枚举值——
   这属于 cat 的协议变更，需与 cat 侧确认，**本文不替它决定**。
3. **推送作为商业能力的定位不变**：`DECISIONS.md` 决策② 把推送划在 L3；
   决策② 和决策③ 的待拍板项目前都还没勾选（`DECISIONS.md` 里全是 `- [ ]`）。
   **在这些拍板之前，任何推送通道的选型都是早熟的。**

如果将来要推翻本文结论，触发条件写在这里，省得后人靠直觉改回去：

- ToDo 已经有了常驻守护进程（因其他需求而生，不是为了 QQ）；且
- 决策② 的商业模型已拍板，推送确定属于要自建的能力；且
- cat 明确不承担 ToDo 的推送（或两者已解耦为不同产品）。

三条同时成立才值得重开这张单。
