# 配置任务列实现计划

规格：`docs/task-columns-spec.md`。用户已选择保留 ecommerce 原结构，只追加空状态列。

任务跟踪使用本项目 `TODO.md` 与当前会话任务表，不创建第二份任务清单。

## 顺序与交付

1. **core 纯变更与来源写入**：新增列配置模块，读取标题、受控生成方案、预览 diff、确认指纹、历史和原子写入。先运行失败回归；保留所有原正文与任务字节。
2. **传输薄适配**：HTTP 和 Tauri 共用 ProjectBackend 方法。以契约测试覆盖当前项目/来源隔离、只读和冲突。
3. **UI 完整流程**：无列菜单入口、默认/已有标题选择、预览、确认、取消、冲突后重新预览。来源和项目绑定；不使用客户端 Markdown 作为写入载荷。
4. **实战验证**：脱敏 ecommerce 结构作稳定 fixture；真实 ecommerce/TODO.md 临时副本进行保真预览/确认及取消/冲突验收，原文件不写入。
5. **门禁与审查**：Rust workspace tests/format/clippy；build/typecheck；UI E2E 和真实 serve E2E；可运行桌面检查并明确无法直接验证的边界。

## 共享接口（新增，不修改既有数据契约）

Rust 类型（从 todo_core 导出），JSON 均为 snake_case：

```text
ColumnSetupSelection =
  { mode: "defaults" }
  | { mode: "existing", headings: [{ name: string, done: boolean }] }
  | { mode: "defaults_and_start", task_id: string }

ColumnSetupOptions = {
  source_path: string,
  base_hash: string,
  headings: [{ name: string, line: number, selectable: boolean, reason: string|null }],
  configured: boolean
}

ColumnSetupPreview = {
  source_path: string,
  base_hash: string,
  selection: ColumnSetupSelection,
  columns: [{ id: string, name: string, done: boolean, created: boolean }],
  proposed: string,
  diff: TextDiff
}
```

`ProjectBackend`：

- `column_setup_options(source: &str) -> Result<ColumnSetupOptions>`
- `preview_columns(source: &str, selection: &ColumnSetupSelection) -> Result<ColumnSetupPreview>`
- `apply_columns(source: &str, base_hash: &str, selection: &ColumnSetupSelection) -> Result<ProjectSnapshot>`

HTTP：

- `GET /api/v1/sources/columns?source=...`
- `POST /api/v1/sources/columns/preview {source_path, selection}`
- `POST /api/v1/sources/columns {source_path, base_hash, selection}`

Tauri 命令（camelCase invoke 参数，对应既有约定）：

- `column_setup_options {sourcePath, expectedProjectPath}`
- `preview_columns {sourcePath, selection, expectedProjectPath}`
- `apply_columns {sourcePath, baseHash, selection, expectedProjectPath}`

`DesktopOps` 对应方法带 `expected_root: &str` 且强制验证。读/预览也验证项目绑定。

TS `desktop` 新方法：

- `columnSetupOptions(sourcePath: string, expectedProjectPath: string)`
- `previewColumns(sourcePath: string, selection: ColumnSetupSelection, expectedProjectPath: string)`
- `applyColumns(preview: ColumnSetupPreview, expectedProjectPath: string)`

前端确认发送 typed selection，不发送 proposed；预览的 selection 在 UI 输入变化后作废。用户更换项目/来源关闭旧对话框；所有异步回调以生命周期/请求编号保护。

## 本轮范围

- 初始配置适用于 `frontmatter.columns` 为空的管理文件；已有非空列定义明确拒绝修改，保留旧 ID 与定义。
- 无 columns key 时局部追加；显式空 columns 仅对可证明的简单空列表位置作局部替换；复杂 YAML/重复标题/多行标题拒绝并说明。
- 默认名称和 ID 为「待办」/todo、「进行中」/doing、「已完成」/done。唯一同名 H2 可复用；缺失的在 EOF 追加空标题。现有标题方式 ID 按稳定选择顺序生成，不依据任务内容猜测完成。
- 普通配置不自动分配任何任务；选择现有标题会使其原有任务归属于该列，但不改变 checkbox。
- 用户明确拖入推进中或选择「开始处理」时，使用 `defaults_and_start`：预览同时创建默认列并移动指定的未归列未完成任务，确认时一个还原点、一次原子写入。复制解析器识别的完整 Markdown 列表项，保留普通续行、子任务和锚定备注。已有同来源活动列则直接沿用既有移动流程。

## 并行所有权

- 主代理：`crates/todo-core`、规格、计划、ecommerce 副本验证。
- 传输代理：`crates/todo-cli/src/serve.rs` 与对应 tests、`apps/desktop/src-tauri` 与对应 tests。
- UI 代理：`apps/desktop/src` 和 UI E2E（不得覆盖已有菜单修复）。

不提交、覆盖或清理 ecommerce 的工作树。共享编译输出需串行或复用既有后台任务，测试预览端口必须隔离。
