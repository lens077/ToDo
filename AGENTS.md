# AGENTS.md

<!-- BEGIN VIRGO MANAGED RULES -->
## Virgo Project Memory

Before work:
1. Read `.virgo/project.yaml`.
2. Read the mapped source documents.
3. Identify task scope and acceptance criteria.

Before handoff:
1. Update affected source documents.
2. Record actual verification results.
3. List unresolved issues.
4. Run `todo contract check`.
5. Report changed files and next steps.

Review diffs before writing managed files.
Keep user-owned instructions intact.
<!-- END VIRGO MANAGED RULES -->

## 本仓库约定

- Markdown 是唯一事实源；SQLite 只是可重建的派生投影。
- TODO.md 格式唯一依据是 `docs/format-spec-v1.md`（`todo-spec: 1`），与 `.virgo/project.yaml` 的 contract schema 相互独立。
- 验证命令：`make check-quick`（快速门禁）、`make check`（完整门禁）。

## 构建矩阵（发版必须遵守）

只构建下列目标；其余平台/架构/格式不再产出，也不要重新加回来：

| 平台 | 保留 | 去掉 |
| --- | --- | --- |
| macOS | arm64，仅 `.app`（`bundle.targets: ["app"]`） | Intel（x86_64）、universal、`.dmg` |
| Windows | x64，仅 NSIS 安装包（`tauri.windows.conf.json`） | arm64、MSI |
| Linux | x64，仅 `deb` 与 `rpm`（`tauri.linux.conf.json`） | AppImage 及其他格式 |
| Android | 仅 `x86_64`（`tauri android build --target x86_64`） | arm64-v8a、armeabi-v7a、x86 |

原因：多架构/多格式让安装包体积翻倍、发布验证与签名成本成倍增加，而这些目标没有实际用户。收紧后每个平台只留一个架构、一个（或两个）安装格式。

落地位置：
- `apps/desktop/src-tauri/tauri.conf.json` + `tauri.windows.conf.json` + `tauri.linux.conf.json`：默认打包格式。
- `scripts/build-target.sh`、`Makefile`（`comm-build` / `build-*`）：只保留 `mac-arm64`、`win-x64`、`linux-x64`。
- `.github/workflows/release.yml`：macOS arm64 单目标。
- 架构不在 `tauri.conf.json` 里表达：macOS 构建在 Apple Silicon 上执行（`bundle-macos` 与 `scripts/package-macos.sh` 会拒绝非 arm64 宿主）。
