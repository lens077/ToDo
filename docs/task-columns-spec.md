# 配置任务列：预览后确认写入

## 目标与范围

从任务菜单的「当前来源未定义列」提示进入「配置任务列…」，无需手改 YAML 即可为当前来源配置可移动的目标列。支持桌面原生桥接与 HTTP 服务；两种入口使用同一 Rust 实现。

本轮优先解决无列来源的初始化。不会重排现有任务，不会根据任务正文推断状态，也不提供删除已有列、重命名已有列或跨来源搬迁。

## 实战依据：ecommerce

已检查 `/Users/lens/lens077/ecommerce/TODO.md`：

- 文件已声明 `todo-spec: 1`，但没有 `columns`。
- 二级标题为「一、优先级」「二、领域状态表」「三、推进顺序」「四、分类明细」。这些是文档结构，不是待办/进行中/已完成状态。
- 任务保留在领域三级标题及优先级四级标题下，带稳定 ID、Markdown 强调、行内代码、链接和续行说明。
- 仓库及此任务文件已有未提交改动。验证默认使用临时副本，不能以模板覆盖原文件，也不提交该仓库的其他改动。

适配必须保持当前文件所有已有任务行、任务 ID、备注、状态表与章节的原始字节。创建默认列只增加声明和空标题；后续显式「移动到」才移动某个任务。

## 交互与验收

1. 在无列任务菜单里提供「配置任务列…」，打开绑定到任务来源的对话框，而不是绑定到当前聚合视图。
2. 明确显示来源路径。加载失败可重试；来源不可写或服务只读时不得提供可提交的操作。
3. 两种方式：
   - **创建默认状态列**：新增「待办」「进行中」「已完成」三个空二级标题；不把现有任务移入。已存在同名唯一标题时复用，不重复创建。
   - **使用现有标题**：列出解析器识别的现有二级标题，明确普通文档标题不等于状态；由用户勾选。允许显式指定完成列，默认不推断。重名或无法唯一定位的标题不能选择。
4. 「预览差异」只读取文件。显示复用/创建的列与完整左右 diff；取消、返回、Esc 均不写文件。修改选择后旧预览失效。
5. 「确认并写入」提交来源、选择、预览指纹。后端在锁内重算受控变更，拒绝陈旧预览、越界路径和只读文件；不能执行客户端传入的任意 Markdown。
6. 写入前记录已有历史还原点，成功后返回权威项目快照，立即更新目标列。重新打开同一任务菜单能显示目标列并可按既有规则移动。
7. 文件在预览后被外部修改时拒写，保留外部修改，要求重新预览。切换项目/来源后不把旧对话框的写入施加到新项目。
8. 小窗口中对话框与 diff 可滚动，确认/取消可通过键盘访问；写入期间避免重复提交。

## 数据与接口

沿用项目现有 `snake_case` wire 字段、错误映射和 `diff_lines` 格式。读取方案、预览、确认写入三步均显式绑定来源。HTTP 读取使用 `source` 查询参数；预览与确认使用 `source_path` 请求字段。原生三步都要求并校验 `expectedProjectPath`（Rust 参数为 `expected_project_path`）；HTTP 同样支持可选的 `expected_project_path`，界面始终发送，防止对错误项目应用旧选择。

- 读取：返回当前来源指纹、已有列定义、可选择二级标题及歧义提示。
- 预览：接收类型化选择（默认列或现有标题），返回 `source_path`、`base_hash`、受控方案和 `diff`。
- 确认：接收同一选择与 `base_hash`，返回权威 `ProjectSnapshot`。重复提交旧指纹应冲突，不覆盖当前文件。

核心约束：已定义的列 ID 与未知 frontmatter 内容必须保留；遇到无法保真修改的 YAML 结构或歧义标题，明确拒绝而不是重新序列化整份文档。沿用来源 2 MiB 大小上限；现有 diff 算法使用 LCS 矩阵，本入口在分配前限制为 4,000,000 个单元，超限明确拒绝预览和写入，不截断用户内容。

## 技术栈与代码位置

- Rust `todo-core`：解析、保真列配置变更、锁、历史与指纹校验。
- Rust `todo-cli/src/serve.rs`：HTTP 薄适配；沿用只读及来源边界。
- Tauri `apps/desktop/src-tauri`：原生命令薄适配。
- React/TypeScript `apps/desktop/src`：来源绑定的配置对话框、预览、确认及菜单入口。
- Rust tests 与 `apps/desktop/e2e`：核心、契约、真实服务与 UI 测试。

代码风格沿用现有项目，边界逻辑放 core，传输层不复制算法。例如：

```rust
if revision(content.as_bytes()) != base_hash {
    return Err(Error::ConcurrentModification);
}
```

## 实现顺序与验证节点

1. 核心预览/写入及保真测试：默认列、现有标题、取消零写入、冲突、只读与来源隔离。
2. HTTP/Tauri 契约与前端 bridge：两种 transport 复用核心，同样的请求/返回结构。
3. 配置对话框与菜单入口：选择 → 预览 → 确认完整链路。
4. ecommerce 脱敏结构回归与真实文件临时副本验证；确认原 ecommerce 文件未被测试改变。
5. 完整门禁、运行中应用检查与局限说明。

## 验证命令

```bash
cargo test -p todo-core --locked
cargo test -p todo-cli --locked
cargo test -p todo-desktop --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
pnpm build
PLAYWRIGHT_PORT=4291 pnpm --filter @todo/desktop exec playwright test --workers=1
bash scripts/e2e-web.sh
# 默认使用已入库的 ecommerce 脱敏结构样本，服务只绑定临时副本
bash scripts/e2e-columns.sh
# 可指定当前实战文件；仍先复制到临时目录，测试后核对原文件 SHA-256 未变
bash scripts/e2e-columns.sh ../ecommerce/TODO.md
# 可选 WebKit 引擎回归，独立于默认 Chromium/移动端门禁
pnpm --filter @todo/desktop exec playwright install webkit
PLAYWRIGHT_PORT=4297 pnpm --filter @todo/desktop exec playwright test --config playwright.webkit.config.ts --workers=1
E2E_COLUMNS_BROWSER=webkit bash scripts/e2e-columns.sh ../ecommerce/TODO.md
```

## 边界

- 始终：先写能证明当前缺口的测试；只操作精确来源；保留未知字节；预览不写盘；确认前校验指纹；保护已有用户改动。
- 需单独选择：将 ecommerce 原文件的哪些现有标题当作列；是否重新组织领域/优先级章节；是否落盘到正在修改的原文件。
- 不做：静默把项目改成默认模板、推断正文完成度、修改 ecommerce 业务代码/部署配置、访问生产系统或凭据、提交 ecommerce 现有工作树。

## 已确认方案

用户已选择「保留 ecommerce 分类结构，只追加空默认列」，先在隔离副本上验收。真实文件落盘通过产品内预览与确认完成。

## 增补：未归列任务直接推进

用户进一步要求把未归列卡片直接拖进「推进中」，不必先配置再拖一次。

- 来源已有活动推进列：按同来源规则直接移动到该列顶部，保留既有完成策略。
- 来源尚无列：拖放或「开始处理」动作打开一次「创建列并推进任务」预览。默认列配置与该任务的移动合并为一个受控修改；确认后只建立一个历史还原点、一次原子写入。取消或过期指纹不创建列、不移动任务。
- 新增选择变体 `{ mode: "defaults_and_start", task_id: "<来源内ID>" }`，复用同一预览/确认接口。界面使用任务的 local_id，不把复合 UI ID 当文件 ID。
- 此变体仅用于原本未归列、未完成、可写的任务。保留任务 ID、checkbox、元数据、子任务与所属 Markdown 续行；不改其他任务或原分类标题。
- 原文件已有列定义但无可识别活动列时不覆盖旧配置，也不跨源借用目标；明确报告不能开始处理的原因。
- 普通「配置任务列」动作保持原语义：仅配置列，不移动任何任务。
- 拖入期间按实际来源可写性反馈落点；切源/切项目、重复拖放、取消、外部编辑均遵循已有生命周期与确认保护。
