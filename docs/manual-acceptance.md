# 桌面端人工验收清单

对应 `TODO.md` 的三条 `#manual` 任务：`p7v2ld`（原生文件选择器）、`h4m9zc`（看板写回）、`w8s5rt`（外部编辑刷新）。

## 为什么必须人工做

自动化跑不了，不是因为没写测试，而是**权限边界挡在那里**：当前 macOS 会话拒绝向 System Events 发送 Apple Events（错误 `-1743`，`docs/verification-log.md` §未自动化的真机路径已记录，本次复验仍然如此）。原生文件选择器、WKWebView 内的键盘输入都在这条边界之外。

已有三组独立证据覆盖了同一批逻辑（browser mock 的完整交互路径、Rust 侧 authoritative mutation 与并发测试、TS bridge 与 Rust command 的结构对照），**唯独「真实 webview + 真实原生控件」这一层只能由人确认**。这份清单就是把那一层补上。

## 准备

```bash
pnpm --filter @todo/desktop tauri build --debug
open target/debug/bundle/macos/Todo.app
```

用一个**可以随便改的测试仓库**，不要用正在工作的项目：

```bash
mkdir -p /tmp/acceptance && cd /tmp/acceptance && git init -q
todo init && todo add "验收样本任务" --column todo
git add -A && git commit -qm "seed"
```

> ⚠️ 本地 `--debug` 包没有 Developer ID 签名与公证，只能本机验收，不得作为发布产物。

---

## p7v2ld — 原生文件选择器打开项目

| 步骤 | 操作 | 期望 |
|---|---|---|
| 1 | 启动 `.app`，首屏应是「选择一份真实的 TODO.md」空状态 | 显示 `LOCAL FIRST / NO PROJECT`，不自动接管任何目录 |
| 2 | 点「选择项目」 | 弹出的是 **macOS 原生**选择器，不是网页模拟的对话框 |
| 3 | 选 `/tmp/acceptance` | 看板载入，出现「验收样本任务」 |
| 4 | 再次打开选择器，选一个**没有 TODO.md** 的目录 | **不静默创建文件**；显式提示或回到空状态 |
| 5 | 检查步骤 4 的目录 | `ls` 确认目录里**没有**被偷偷写入 `TODO.md` |

步骤 4–5 是这条任务的重点：`README.md` 承诺「桌面端不会自动接管」，这一条只能靠真实选择器验证。

## h4m9zc — 看板任务写回 TODO.md

全程用**两个窗口并排**：左边 `.app`，右边终端跑 `watch -n1 cat /tmp/acceptance/TODO.md`（或反复 `cat`）。

| 步骤 | 操作 | 期望 |
|---|---|---|
| 1 | 在看板新增一个任务 | `TODO.md` 立即出现该行，且带 `<!--t:...-->` 稳定 ID |
| 2 | 把任务拖到另一列 | 文件里该行移动到对应小节；**其余字节不变** |
| 3 | 勾选完成（`normal` 策略） | `- [ ]` 变 `- [x]` |
| 4 | `todo policy strict` 后再勾选一个 | 弹出三结局确认对话框，要求填说明 |
| 5 | 在对话框选「部分完成」，填 60 与说明 | 文件出现 `◐60` 与 `↳` 说明行 |
| 6 | 选「失败」，填原因 | 出现 `⚠` 与 `↳`；**完成状态不变** |
| 7 | 全程结束后 `git diff` | 每次改动只触及相关行，没有整文件重排或换行符变化 |

步骤 7 是核心承诺（「只修改自己认识的行，其余字节原样保留」）的最终检验。

## w8s5rt — 外部编辑后界面刷新

| 步骤 | 操作 | 期望 |
|---|---|---|
| 1 | `.app` 开着项目，在终端 `todo add "外部新增" ` | 界面**自动**出现该任务，不需要手动刷新 |
| 2 | 用编辑器直接改 `TODO.md` 某任务的文字并保存 | 界面同步更新 |
| 3 | 界面上打开确认对话框，**先不确认**；此时在终端改同一文件 | 确认时应被拒绝并提示「文件已在外部更新，请重试」，**不得**用陈旧内容覆盖 |
| 4 | 终端 `todo done <id>`（或 `restore`） | 界面状态跟随，不出现虚假的乐观状态 |

步骤 3 是确认闸门的真实检验：预览锁定文件指纹，指纹不符必须拒写。

## Quick Capture（顺带确认）

| 步骤 | 操作 | 期望 |
|---|---|---|
| 1 | 按 `Cmd+Shift+Space` | 速记窗口出现并**获得前台焦点** |
| 2 | 输入文字，`Cmd+Enter` | 存入 `~/.todo/inbox.md`；**不进入项目仓库** |
| 3 | `Esc` | 窗口隐藏 |

> 注意步骤 2 会写你真实的 `~/.todo/inbox.md`。想隔离可用 debug-only 的 `TODO_DESKTOP_TEST_CAPTURE_PATH` 指到 `/tmp`。

---

## 验收结论怎么记

三条都通过后，用工具自身记录（保持任务状态与物证一致）：

```bash
cd /Users/sumery/lens077/ToDo
todo done p7v2ld --note "原生选择器验收通过：可打开项目，无 TODO.md 时不静默创建"
todo done h4m9zc --note "看板写回验收通过：新增/拖动/三结局均正确落盘，diff 最小"
todo done w8s5rt --note "外部编辑刷新验收通过：自动同步，且陈旧 diff 被确认闸门拒绝"
```

任何一条不通过，用 `todo fail <id> --note "<现象>"` 记录**现象而非猜测的原因**，并把复现步骤补进 `docs/verification-log.md`。

最后把结果并入 `docs/verification-log.md`，那里是验证结论的权威归档。
