# Android 真机手工测试单（Virgo 0.1.1 / arm64）

## 安装包

- 文件：`dist/virgo-0.1.1-arm64-release-signed.apk`（20,438,341 字节）
- SHA-256：`178ea706efaae6eae2d21f37e4da9d600a46bc1dc9e60ce161fd8e8849932176`
- 签名：Android debug keystore（`CN=Android Debug`），不是 Play 签名；自编译包，安装时系统会提示「未知来源」，允许即可。
- ABI：arm64-v8a（只支持 64 位真机）
- 版本：`versionName=0.1.1`，包名 `com.sumery.virgo`
- 含本次修复：`UriPlugin`（SAF `content://` 文件导入）、移动端工作空间目录、Pro 图片代理 + P-256 设备证明。

## 前置条件

1. Android 8+、arm64 真机，能安装自编译 APK。
2. 一台**能访问 X 正文接口的网络**：手机浏览器能打开 `https://api.fxtwitter.com/i/status/1`（返回 JSON）或 `https://x.com`。图片 CDN `pbs.twimg.com` 不需要能直连——那正是要被代理接管的部分。
3. Casdoor 账号已开通 Pro（这台账号的有效期到 2026-12-26）。

### 设备绑定（2026-11-27 起：新设备登录自动挤掉旧设备）

`virgo-auth` 仍然是**单设备绑定**，但改成 last login wins：同一账号在新设备登录并激活时，服务端直接把绑定换成新设备，旧设备的设备证明立刻失效（下一次刷新令牌/校验会返回 `bad_proof`），**不再需要先去旧设备解绑**。所以：

- 直接在另一台手机登录即可，旧设备会自动被挤掉。
- 旧设备手里的 Pro 令牌在到期前仍能取图（令牌是无状态的，最短 12 小时）；回到旧设备重新登录就能抢回来。
- 如果测试账号还没有 Pro，请在 Casdoor 管理台把它加进 `virgo_pro` 组；免费档也能测导入与阅读，只是图片不走代理。

## 安装

```bash
adb install -r dist/virgo-0.1.1-arm64-release-signed.apk
```

或把 APK 传到手机，用文件管理器点开安装（允许未知来源）。

## 测试步骤

### 1. 登录与 Pro 状态

1. 打开应用 → 底部「我的」→ 登录入口，走浏览器 Casdoor 登录并回跳 `virgo://auth`。
2. 确认显示：账号 `ca8ceec3-…`、设备「Android」、`订阅 Pro 有效`、`代理状态 已启用`。
3. 点一次「刷新令牌」，确认没有报错（这一步会走设备证明）。

期望：登录后 `/v1/activate` 或 `/v1/refresh` 返回 200，令牌剩余时间正常刷新。

### 2. X 长文导入 + 图片落盘（核心用例）

1. 「收件箱」→ 粘贴框输入一条 X 状态链接，例如
   `https://x.com/NASA/status/2051361508181856362` → 点「添加文章」。
2. 等待状态从「同步中…」变为「已同步」，收件箱数量 +1。
3. 打开这篇文章：标题应为该推文正文（例如 “Start your week with some new photos from Artemis II!”），正文中应能看到图片。
4. 断网验证：开飞行模式 → 强杀应用重启 → 打开同一篇文章 → 图片仍能显示。

期望：图片可见且断网可见（说明已落盘）；node4 imgproxy 的当日计数会增加，可在服务端用
`ssh node4 'python3 -c "import json;d=json.load(open(\"/srv/virgo-imgproxy/state.json\"));print([(k,v) for k,v in d.items() if v.get(\"day\")==\"$(date +%F)\"])"'` 交叉确认。

### 3. 本地文件导入（content:// 修复回归）

1. 往手机 `Download/` 放一个 `.html` 或 `.md`（正文里可以带一张 `pbs.twimg.com` 图片地址）。
2. 「收件箱」→「打开文件」→ 系统文件选择器里选「下载」→ 选中该文件。
3. 等待「已同步」，确认文章进入收件箱且正文完整。

期望：不再出现 `I/O error for content://…: No such file or directory`。

### 4. 已知限制（不是本次要修的）

- Android 上没有目录选择器：点「选择项目 / 添加项目」会内联报 `Folder picker is not implemented on mobile`。这是 `a2pick` 未完成的一半，测试时请记录但不算失败。
- 手机网络必须能访问 X 正文接口；若只通代理域名，导入会失败在正文抓取那一步。

## 需要反馈的内容

1. 每一步的截图（Pro 状态页、文章正文含图、飞行模式下同一篇文章）。
2. 文章标题与导入耗时（「刚刚 / N 分钟」）。
3. 若有失败：完整报错文字 + 步骤 + 是否已登录 Pro。
4. 结束后我可以按时间点在 node4 上核对代理计数与 device 绑定记录。
