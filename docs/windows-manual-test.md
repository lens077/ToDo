# Windows 手工测试单（Virgo 0.1.1 / x64）

两套产物，建议先跑 NSIS 装包确认能启动，再用 MSIX 套件测打包形态。

## 产物

| 文件 | 说明 |
| --- | --- |
| `dist/windows/Sumery Virgo_0.1.1_x64-setup.exe`（7,162,076 字节，SHA-256 `245a258b…7fc8`） | NSIS 安装包，未签名，首次运行会弹 SmartScreen |
| `dist/windows/virgo-msix-kit.zip`（9,948,901 字节，SHA-256 `a819e7a8…d1b1`） | MSIX 打包套件（payload 二进制 + 清单 + 磁贴 + 打签脚本） |

MSIX 套件需要在 Windows 上打包：macOS 没有 `makeappx`/`signtool`，仓库发布矩阵也只用 NSIS。payload 的 exe 是用 `cargo-xwin` 在 macOS 上交叉编译出来的 x64 二进制（GUI 子系统，不会弹控制台窗口）。

## A. NSIS 安装包

1. 双击 `Sumery Virgo_0.1.1_x64-setup.exe`；SmartScreen 提示时选「更多信息 → 仍要运行」。
2. 安装完成，从开始菜单启动「Sumery Virgo」，确认窗口能打开、任务清单能加载（首次会要求选择项目目录）。
3. 登录 Casdoor（设置 → 登录，浏览器回跳 `virgo://auth`），确认「订阅 Pro 有效 / 代理状态 已启用」，点一次「刷新令牌」。
   - 这一步同时覆盖 `k9dpae` 的 Windows 侧：设备私钥应写进 Windows Credential Manager（`cmdkey /list | findstr /i virgo` 能看到条目），而不是明文文件。
4. 关掉应用重开，再点「刷新令牌」仍成功 —— 证明私钥在重启后能从 Keyring 解出。
5. X 文章（需要能访问 X 正文接口的网络）：收件箱粘贴一条 status 链接 → 添加文章 → 打开确认图片可见 → 断网重开再看一次。

## B. MSIX 打包与安装

前置：Windows 10/11 SDK（`winget install Microsoft.WindowsSDK`），PowerShell 5.1+。

```powershell
Expand-Archive virgo-msix-kit.zip -DestinationPath .\msix
cd .\msix\kit
pwsh -File package-msix.ps1 -Sign
```

脚本会：生成/复用自签名证书（主题 `CN=Virgo Test`）→ 用 `makeappx` 打包 → 用 `signtool` 签名 → 输出 `out\virgo-0.1.1.0-x64.msix` 与 `out\virgo-test.cer`。

安装（管理员 PowerShell）：

```powershell
Import-Certificate -FilePath out\virgo-test.cer -CertStoreLocation Cert:\LocalMachine\TrustedPeople
Add-AppxPackage -Path out\virgo-0.1.1.0-x64.msix
```

然后从开始菜单启动，重点看：

1. 应用能启动，`%USERPROFILE%\.todo` 能正常创建/读写（MSIX 是全信任桌面应用，不受包容器重定向限制）。
2. 项目目录能否选择、任务清单能否加载。
3. 登录 + 刷新令牌（同 A 的第 3、4 步）。
4. 卸载：设置 → 应用 → 已安装的应用 → Sumery Virgo → 卸载；确认 `%USERPROFILE%\.todo` 是否保留（按当前实现应该保留）。

## 需要反馈

1. 每步截图（安装/启动、Pro 状态、刷新令牌、文章含图、MSIX 安装后的开始菜单条目）。
2. MSIX 相关的具体报错：`makeappx`/`signtool` 的完整输出、`Add-AppxPackage` 的错误码（HRESULT）。
3. 若 NSIS 包启动就崩：Windows 事件查看器里应用崩溃记录（Application log）。
4. 证书信任环节如果用企业签名，把 pfx 与主题告诉我，我改脚本参数。

## 已知边界

- 两个包都未签名（NSIS 无签名；MSIX 用自签名证书，需手动信任）。
- MSIX 不在仓库发布矩阵里，属于这次测试的临时形态；正式发版仍只产出 NSIS。
- 应用市场分发需要有效的代码签名证书与商店身份，这里的包不满足。
