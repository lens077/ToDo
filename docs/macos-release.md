# macOS 发布、签名与公证

本文档供发布维护者使用，说明如何生成 macOS CLI 与 `Todo.app`，以及如何用 Developer ID 完成签名和 Apple 公证。

> 本仓库没有可用于发布的 Developer ID 证书或 Apple 公证凭据。所有依赖真实凭据的验收项均标记为 `〔待验证〕`。本地 ad-hoc 签名不能替代 Developer ID 签名。

## 发布约束

- `.github/workflows/release.yml` 只在 `macos-release` 环境的全部密钥存在时继续执行。
- 发布工作流不提供未签名发布的降级路径。签名、公证或 Gatekeeper 检查失败时，工作流在上传产物前停止。
- 构建与签名 job 只有 `contents: read` 权限。单独的发布 job 只有 `contents: write` 权限，且不接收 Apple 凭据。
- 发布工作流的第三方 Actions 固定到完整 commit SHA。更新 action 时，先审查新 commit，再更新 SHA 后的版本注释。
- `workflow_dispatch` 生成受保护的工作流产物，但不创建 GitHub Release。
- 推送与应用版本匹配的 `v*` 标签时，工作流在全部检查通过后创建 GitHub Release。
- 本地未提供发布凭据时，`scripts/package-macos.sh` 只生成带 `local-unnotarized` 后缀的验收包。不要分发这些验收包。

## Bundle 配置

`apps/desktop/src-tauri/tauri.conf.json` 包含以下发布骨架：

- `bundle.active` 为 `true`；
- `bundle.targets` 只包含 `app`；
- `bundle.macOS.hardenedRuntime` 为 `true`；
- `bundle.macOS.signingIdentity` 为 `null`。

`signingIdentity` 保持为 `null`，避免把组织身份写入仓库。发布工作流通过 `APPLE_SIGNING_IDENTITY` 注入实际的 Developer ID Application 身份。

## 前置条件

发布维护者需要：

1. 有效的 Apple Developer Program 会员资格。
2. 包含私钥的 `Developer ID Application` 证书，并导出为密码保护的 `.p12` 文件。
3. 有权提交公证的 App Store Connect 团队 API key，包括 `.p8` 私钥、Key ID 与 Issuer ID。
4. 与证书一致的 Apple Team ID。
5. 对 GitHub 仓库 `macos-release` Environment 和 Release 的管理权限。

## 配置 GitHub Environment

在 GitHub 仓库中创建名为 `macos-release` 的 Environment。建议为该 Environment 配置人工审批，并添加以下 secrets：

| Secret | 内容 |
|---|---|
| `APPLE_CERTIFICATE` | `.p12` 文件的单行 Base64 内容 |
| `APPLE_CERTIFICATE_PASSWORD` | 导出 `.p12` 时设置的密码 |
| `APPLE_SIGNING_IDENTITY` | 完整身份，例如 `Developer ID Application: Example Name (TEAMID)` |
| `APPLE_TEAM_ID` | 证书所属的 10 位 Team ID |
| `APPLE_API_KEY_ID` | App Store Connect API key 的 Key ID |
| `APPLE_API_ISSUER` | App Store Connect API key 的 Issuer ID |
| `APPLE_API_PRIVATE_KEY` | `AuthKey_*.p8` 文件的单行 Base64 内容 |

生成 `APPLE_CERTIFICATE`：

```bash
openssl base64 -A -in DeveloperIDApplication.p12 | pbcopy
```

生成 `APPLE_API_PRIVATE_KEY`：

```bash
openssl base64 -A -in AuthKey_EXAMPLE.p8 | pbcopy
```

发布脚本只把 API 私钥解码到权限为 `0600` 的临时文件。退出时，脚本删除包含该私钥的临时目录。公证命令通过私钥文件认证，不把私钥或账户密码放进进程参数。

查询可用的签名身份：

```bash
security find-identity -v -p codesigning
```

把输出中完整的 `Developer ID Application: ... (TEAMID)` 文本保存为 `APPLE_SIGNING_IDENTITY`。不要把 `.p12`、密码或 secret 值提交到仓库，也不要粘贴到 Issue、Pull Request 或构建日志中。

## 本地构建验收包

没有发布凭据时，运行：

```bash
OUTPUT_DIR=target/release-local ./scripts/package-macos.sh
```

脚本执行以下操作：

1. 使用 `cargo build --locked --release -p todo-cli` 构建 CLI。
2. 使用 Tauri 构建 `Todo.app`。
3. 对本地 `Todo.app` 应用 ad-hoc 深签名，并用 `codesign --verify --deep --strict` 检查 bundle 完整性。
4. 生成 CLI 压缩包、应用压缩包与 `SHA256SUMS`。
5. 给文件名添加 `local-unnotarized` 后缀，并打印不可分发警告。

这条路径只验证构建、bundle 完整性与打包。Ad-hoc 签名不验证发布者身份，也不验证 Developer ID、Apple 公证或 Gatekeeper 放行。

## 创建正式发布

1. 确认 `apps/desktop/src-tauri/tauri.conf.json` 中的 `version` 是待发布版本。
2. 创建同版本标签。标签必须是 `v` 加应用版本，例如应用版本 `0.1.0` 对应 `v0.1.0`。
3. 推送标签，等待 `Release macOS` 工作流完成。

```bash
git tag -s v0.1.0 -m "Release v0.1.0"
git push origin v0.1.0
```

发布工作流按以下顺序执行：

1. 从 `Cargo.toml` 读取 `workspace.package.rust-version`，安装对应 Rust 工具链。
2. 校验标签与应用版本一致。
3. 校验所有发布 secrets 都已配置。
4. 把 `.p12` 证书导入临时 keychain。
5. 用 Developer ID 签名 CLI，并由 Tauri 签名 `Todo.app`。
6. 用 `codesign --verify --deep --strict` 检查签名，并校验证书类型与 Team ID。
7. 使用临时 `.p8` API key 文件，分别把 `Todo.app` 和 CLI 提交给 `xcrun notarytool`。只有状态为 `Accepted` 才继续。
8. 把公证票据 staple 到 `Todo.app`，运行 `stapler validate`，并对应用与 CLI 分别运行 `spctl --assess`。独立 CLI 不能 staple 公证票据，Gatekeeper 会在线查询它的公证记录。
9. 生成压缩包和 `SHA256SUMS`。
10. 构建 job 上传工作流产物；标签触发时，无 Apple 凭据的发布 job 下载已验证产物并附加到 GitHub Release。

任一步骤失败时，不要手动上传同一次运行留下的文件。修复证书、密钥、版本或代码问题后，重新执行完整工作流。

## 验证下载产物

下载同一次工作流生成的两个压缩包和 `SHA256SUMS`，在 macOS 上执行。`SHA256SUMS` 用于发现传输损坏；它与产物来自同一发布路径，不能单独证明发布者身份。

```bash
shasum -a 256 -c SHA256SUMS
mkdir -p /tmp/todo-release-check
ditto -x -k Todo-0.1.0-*-apple-darwin.app.zip /tmp/todo-release-check
codesign --verify --deep --strict --verbose=2 /tmp/todo-release-check/Todo.app
xcrun stapler validate /tmp/todo-release-check/Todo.app
spctl --assess --type execute --verbose=2 /tmp/todo-release-check/Todo.app
```

验证 CLI 签名：

```bash
tar -xzf todo-0.1.0-*-apple-darwin.tar.gz -C /tmp/todo-release-check
codesign --verify --strict --verbose=2 /tmp/todo-release-check/todo
codesign --display --verbose=4 /tmp/todo-release-check/todo
spctl --assess --type execute --verbose=2 /tmp/todo-release-check/todo
```

把示例中的 `0.1.0` 替换为实际版本。`codesign --display` 的 `Authority` 必须是 `Developer ID Application`，`TeamIdentifier` 必须与 `APPLE_TEAM_ID` 一致。

## 待验证项

以下步骤需要真实证书或 Apple 公证账户，本次实现无法在本地诚实复现：

1. `〔待验证〕` GitHub Actions 能从 `macos-release` Environment 导入真实 `.p12` 证书，并解码真实 App Store Connect `.p8` 私钥。
2. `〔待验证〕` Tauri 生成的 `Todo.app` 和独立 CLI 均显示正确的 Developer ID Application authority 与 Team ID。
3. `〔待验证〕` Apple 公证服务使用团队 API key 对两个产物返回 `Accepted`，且 `Todo.app` 能成功 staple 公证票据。
4. `〔待验证〕` 从 GitHub Release 重新下载的 `Todo.app` 与 CLI 均通过 `spctl --assess`，且 CLI 通过 `codesign --verify`。

完成这些验证后，把工作流 URL、产物 SHA-256 和验证命令输出记录到发布记录中。不要只记录「工作流为绿色」。
