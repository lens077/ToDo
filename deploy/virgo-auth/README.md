# virgo-auth：登录、Pro 判定、设备绑定与短期令牌

`virgo-img.apikv.com` 上的授权服务。它只做四件事，每件事都在服务端完成，
客户端拿不到也改不了：

1. **登录**：Casdoor OIDC 授权码流程。`client_secret` 只在服务器上，永不下发。
2. **判定 Pro**：读 Casdoor userinfo 的 `groups` 与 `roles`，命中
   `PRO_GROUP`（`lens/virgo-pro`）或 `PRO_ROLE`（`lens/virgo_pro`）任一即 Pro。
   两种字段的写法不同：groups 是 `owner/name` 全名，roles 只有短名；比较时
   允许一边全名一边短名，但两个全名必须同 owner（`proClaimMatches`）。
3. **绑定设备**：账号 ↔ 设备公钥（P-256），一个账号只允许一台活跃设备。
4. **签发短期令牌**：Ed25519 签的 12 小时令牌，imgproxy 用公钥验。

## 为什么是「设备公钥」而不是「设备指纹」

设备指纹（Android ID / 型号串）可以被伪造，也能被同型号环境复现。这里用
**密钥对**：私钥在 Android Keystore / macOS 钥匙串里生成，永不出设备；服务端
只存公钥。所以「把令牌拷到另一台手机」拿不到私钥就签不出可用请求——这是你
要求的「绑定单设备、避免多设备共用」的唯一可靠实现。

证明格式（`X-Virgo-Proof` 对应的签名原文）：

```
{sub}\n{method}\n{path}\n{unix秒}\n{nonce}
```

服务端校验四件事：签名有效、时间在 ±5 分钟内、同一 nonce 只用一次、账号有绑定。

## 接口

| 方法 | 路径 | 用途 |
| --- | --- | --- |
| GET | `/auth/login?client=desktop` | 302 到 Casdoor；登录后跳 `virgo://auth?ticket=`（Android） |
| GET | `/auth/login?client=loopback&port=&state=` | 302 到 Casdoor；登录后跳 `http://127.0.0.1:<port>/auth?ticket=&state=`（桌面端，RFC 8252） |
| GET | `/auth/callback?code=&state=` | code 换 userinfo → 判 Pro → 写会话 / 跳客户端 |
| POST | `/v1/activate` | `{publicKey, deviceName, ticket}` → 绑设备 + 发令牌 |
| POST | `/v1/refresh` | 设备证明 → 新令牌 |
| POST | `/v1/unbind` | 解绑当前设备（24 小时冷却；换机不再必需） |
| GET | `/v1/entitlements` | 当前账号 `{pro, deviceBound}` |
| GET | `/healthz` | 存活探针 |

错误一律 `{error, message}`。`429` 表示解绑冷却；激活不再返回「账号已绑别的设备」——新设备会直接挤掉旧设备（last login wins）。

## 环境变量

| 变量 | 必填 | 说明 |
| --- | --- | --- |
| `CASDOOR_CLIENT_ID` | 是 | 应用 `virgo-todo` 的 Client ID |
| `CASDOOR_CLIENT_SECRET` | 是 | 同一应用 Secret（只在服务端） |
| `CASDOOR_URL` | 否 | 默认 `https://casdoor.apikv.com` |
| `CASDOOR_REDIRECT_URL` | 是 | `https://virgo-auth.apikv.com/auth/callback` |
| `AUTH_STATE_SECRET` | 是 | ≥16 字节随机串，签 state / 会话票据 |
| `PRO_GROUP` 或 `PRO_ROLE` | 是 | 命中其一即 Pro，如 `lens/virgo-pro` |
| `TOKEN_KEY_FILE` | 否 | Ed25519 私钥路径，默认 `/srv/virgo-auth/token.ed25519`（首次自动生成 0600） |
| `TOKEN_TTL` | 否 | 默认 `12h` |
| `CLIENT_SCHEME` | 否 | 移动端回跳 scheme，如 `virgo://auth`（留空则不移交票据） |
| `LISTEN` | 否 | 默认 `127.0.0.1:8766` |

## 部署（k8s，当前形态）

授权服务跑在 k8s 的 `virgo` 命名空间；**绑定表落 `postgresql` 命名空间的
`pg-main`（库 `virgo_auth`）**。imgproxy 仍留在 node4——集群节点回不了
`pbs.twimg.com`（实测 HTTP 000），图片必须由境外节点抓。

```
virgo-auth.apikv.com  ──▶ k8s cilium-gateway (10.10.31.240:443) ──▶ virgo-auth (virgo ns)
virgo-img.apikv.com   ──▶ node4 imgproxy (127.0.0.1:8765)
```

**双域名方案**（定稿）：授权与图片各占一个域名，每个资源只有一个 target、不用
path 分流——与 `sponsor-api`、`virgo` 等既有资源完全同构。代价是 Casdoor 的
`redirectUris` 必须是 `https://virgo-auth.apikv.com/auth/callback`。

（早期曾用单域名 + 三个 path target；Pangolin 1.27 确实支持 `path`/`pathMatchType`，
但本部署所有资源都是 1:1，没必要引入未验证的能力。）

### 一次性准备

```bash
# 1. 建库（CNPG Database 资源，operator 以管理身份执行）
kubectl apply -f - <<'EOF'
apiVersion: postgresql.cnpg.io/v1
kind: Database
metadata: { name: virgo-auth, namespace: postgresql }
spec: { name: virgo_auth, owner: app, cluster: { name: pg-main } }
EOF

# 2. 三个 Secret（密钥不进 Git）：
#    virgo-auth-env   client-id / client-secret / state-secret
#    virgo-auth-token token.ed25519（Ed25519 私钥，64 字节二进制）
#    virgo-auth-db    url=postgres://app:<pw>@pg-main-rw.postgresql:5432/virgo_auth?sslmode=verify-full&sslrootcert=/etc/virgo-pg/ca.crt
#    virgo-pg-ca      ca.crt（从 postgresql/pg-main-ca 复制，Secret 不能跨命名空间挂载）

# 3. 构建镜像并推 TCR（node4 有 docker；FROM alpine + 静态二进制）
GOOS=linux GOARCH=amd64 CGO_ENABLED=0 go build -trimpath -ldflags "-s -w" -o /tmp/imgbuild/virgo-auth ./cmd/virgo-auth
scp /tmp/imgbuild/virgo-auth node4:/tmp/imgbuild/     # 与 Dockerfile 同目录
ssh node4 'cd /tmp/imgbuild && docker build -t ccr.ccs.tencentyun.com/sumery/virgo-auth:<tag> . && docker push ...'

# 4. 部署
helm upgrade --install virgo-auth deploy/virgo-auth/chart/virgo-auth -n virgo -f .../values-prod.yaml
```

> 当前状态（2026-11-26 晚）：含 `dpk` 签发的新镜像 `20261126234900`
> （digest `sha256:76d04b98ee4bc631859be27b57a8f52b6b65b34aef924b59a11c69620eff1a20`）
> **已上线**（helm revision 8，Pod `virgo-auth-7c49ffd499-ktqnq`），`/auth/login` 302 正常。
> 用受控票据激活实测：令牌 claims 带 `dpk` 且与设备公钥一致，拿该令牌对 node4
> 新版 imgproxy 取图 200/12302 B。**待客户端发版**：旧客户端发 `Authorization: Bearer`，
> 会被 imgproxy 401（Pro 取图本来就没通过）；新 APK 已构建（`app-universal-release-signed.apk`，
> 20.4 MB），等真机接回后安装。

### 导出验签公钥给 node4

```bash
./scripts/export-pubkey.sh /tmp/virgo-auth.key       # 读 64 字节私钥，输出后 32 字节公钥
scp pub.b64 node4:/srv/virgo-imgproxy/pro.pub
ssh node4 'systemctl restart virgo-imgproxy'
```

> node4 上早先的 systemd `virgo-auth` 实例已 `disable --now`：它用另一把密钥，
> 留着会签出 imgproxy 验不过的令牌。

## 与 imgproxy 的衔接

imgproxy 有两条鉴权路径，任一通过即可：

1. **Pro 短期令牌**：`X-Virgo-Token: <base64url(payload)>.<base64url(sig)>`。
   验签用 `/srv/virgo-imgproxy/pro.pub`。令牌里的 `hosts` 字段可收窄可回源域名。
2. **静态 Bearer**：`tokens.json` 里的手工令牌，给开发与内部排障用。

两条都校验白名单（只放行 `pbs/abs.twimg.com`）。令牌过期（`exp`）直接 401。

## 客户端接入（2026-11-25）

实现在 `apps/desktop/src-tauri/src/auth.rs`（Rust）与
`apps/desktop/src/{lib/auth.ts,hooks/useAuth.ts,components/SettingsDialog.tsx}`（前端）。

| 环节 | 做法 |
| --- | --- |
| 唤起登录 | 桌面端 `client=loopback&port=<随机端口>&state=<随机串>`：应用先在 `127.0.0.1` 起一个短命监听，再打开浏览器；服务端回调跳到本机，应用校验 state、回一页静态提示、再用票据激活。Android 用 `client=desktop`，回跳 `virgo://auth?ticket=`，由 `tauri-plugin-deep-link` 接住。 |
| 设备密钥 | P-256 ECDSA。macOS 走私钥进系统钥匙串（`security` CLI，进程身份固定为 `/usr/bin/security`，未签名开发构建反复重建也不会反复弹授权框）；Windows / Linux 走 `keyring` 3（Windows Credential Manager / Linux Secret Service），没有可用后端（无 DBus 或 keyring 守护）时自动回退到应用目录 0600 文件；Android 由 Kotlin `KeystorePlugin` 把 PKCS#8 用 Keystore 里不可导出的 AES-256-GCM 密钥加密后落盘。`auth.json` 只存 `sub`/公钥/设备名，不含任何秘密。 |
| 令牌 | **只在内存**，不写 `localStorage`、不写 `todo.app-settings.v1`；重启后由后台线程用设备证明重新签发（启动即补一次，之后每 10 分钟检查）。 |
| 抓图 | `reading_add` / `reading_quick_add` / `reading_retry_parse` 在 Rust 里取令牌，前端拿不到；代理地址是常量 `https://virgo-img.apikv.com/img?url=`，令牌只发给这个 host。显式 `imageProxyBase/Token` 参数仍可用，优先于登录态（排障与测试）。 |
| 设置页 | 只有登录态：未登录给「登录 Pro 账号」，已登录列账号/设备/绑定时间/令牌剩余，并提供刷新与解绑（解绑需二次确认，提示 24 小时冷却）。 |

## 已知边界

- **令牌可被拷走**：imgproxy 只验签与到期，不校验请求来自哪台设备。12 小时内
  把令牌拷到另一台机器也能取图；设备绑定只在激活时约束「一个账号一台设备」。
  要堵住需要让 imgproxy 验设备证明（Python 侧实现 P-256 验签）或把 TTL 缩短
  到分钟级，见 `TODO.md` `f8tokn`。
- **钥匙串写入有毫秒级的 argv 暴露**：macOS 侧用 `security add-generic-password -w <base64>`
  写私钥，写入瞬间同一用户的其他进程可在 `ps` 里看到这段 base64（窗口只有几毫秒，
  但严格说不是零）。彻底的做法是改用 Security framework 或签名的 helper；在
  改为 Tauri 原生 Keychain 前，这条记在这里，别当成「钥匙串 = 绝对不可读」。
- **Windows / Linux 的私钥保护依赖 `keyring` 3 后端**：Windows 走 Credential
  Manager、Linux 走 Secret Service（DBus）；无桌面/无守护进程时回退到应用目录
  0600 文件。代码已按同一份 API 写，Windows 的调用方式在最小 crate 里
  `cargo check --target x86_64-pc-windows-msvc` 通过；Linux 与两个平台的运行时
  尚未实测（本机无对应环境），发版前需在真机各跑一遍「登录 → 重启 → 刷新 → 解绑」。
- **Pro 判定的时效**：Casdoor 订阅到期后由它的 cron（每小时）回收组，所以
  最坏情况多给一小时。严格计费应在 `/v1/activate` 里额外查订阅 `endTime`。
- **令牌里的设备公钥（`dpk`）**：activate / refresh 会把请求带的设备 P-256
  公钥（SPKI base64）写进令牌 claims；imgproxy 用它给每个取图请求验设备证明
  （见 `deploy/imgproxy/README.md` 的「设备证明」一节）。这里只存公钥，私钥
  永远不出客户端。
- **Pro 判定依赖 userinfo 的 groups/roles**：`LoginURL` 已请求
  `scope=openid profile email`，实测 Casdoor 的 userinfo 返回
  `groups: null` 与 `roles: ["admin","customer","virgo_pro"]`；只读 groups 会把
  有 `virgo_pro` 角色的账号误判成非 Pro（2026-11-26 已修，见下）。
## 实测记录（2026-11-24，node4）

| 项 | 结果 |
| --- | --- |
| `GET /auth/login` | 302，带 client_id / redirect_uri / scope / 已签名 state ✅ |
| 未登录调 `/v1/activate` | 401 `not_signed_in` ✅ |
| 伪造 ticket | 401 `not_signed_in` ✅ |
| 未绑定的 `/v1/refresh` | 401 `bad_proof` ✅ |
| 有效 Pro 令牌取图 | 200，122884 字节有效 JPEG ✅ |
| 过期 Pro 令牌 | 401 ✅ |
| 静态 Bearer 令牌并存 | 200 ✅ |

Ed25519 交叉验证：Go 的 `crypto/ed25519` 产出签名，Python 标准库实现验签
通过；篡改消息 / 篡改签名 / 换公钥 / 空签名全部拒绝。公钥导出脚本的输出与
Go 独立算出的公钥逐字节一致。

## 待办

1. **Casdoor 对象**：`virgo_pro` 角色已存在（admin_01 名下可见，display
   "Virgo Pro"），`virgo-pro` 组与 `virgo-todo` 定价页还没建；`PRO_GROUP`
   目前是空命中，实际靠角色判定。跑 `casdoor-setup.sh` 时注意 admin token
   只有 15 分钟有效期。
2. **`redirectUris` 清理**：`admin/virgo-todo` 当前注册了
   `http://localhost:9000/callback`（旧开发入口）与
   `https://virgo-auth.apikv.com/auth/callback`；建议删掉前者。
3. **`virgo-img.apikv.com` 回源中断（2026-11-26 发现，未修）**：TLS 握手正常、
   HTTP/2 流能建立，但 Pangolin 不再把请求转给 node4（无响应直到超时）；
   node4 本机 `127.0.0.1:8765/healthz` 返回 `{"ok":true}`，newt 的 websocket
   已连上（siteId 16）。逐项排查后判断是 Pangolin 侧 resource → site 的关联
   掉线或站点被标记异常，需要 Pangolin 控制台或 node1 上的 DB 才能确认。
   这一条不修，Pro 图片回源在公网不可用。

## 集群部署实测（2026-11-25）

| 项 | 结果 |
| --- | --- |
| 库 `virgo_auth` | CNPG `Database` 资源创建，operator 报 `applied=true` |
| Pod | `virgo-auth` 1/1 Running，日志「绑定表落 PostgreSQL」 |
| 表 | 服务启动自动建：`device_bindings` / `device_unbinds` / `proof_nonces` |
| `/healthz` | `{"ok":true}` |
| `/auth/login` | 302，带 client_id、`redirect_uri=https://virgo-img.apikv.com/auth/callback`、已签名 state |
| `/v1/activate`（无票据） | 401 `not_signed_in` |
| `/v1/entitlements`（无会话） | 401 `not_signed_in` |
| HTTPRoute | `cilium-gateway: accepted=True`，hostname `virgo-img.apikv.com`，path `/auth` + `/v1` |
| 真实密钥令牌取图（node4 imgproxy） | 200，88167 字节有效 JPEG |
| k8s 网关直连（Host 正确） | `/auth/login` 302、`/v1` 401（**需要 CiliumNetworkPolicy**，见下） |
| 伪造令牌 | 401 |
| 静态 Bearer 令牌 | 200（保留给内部排障） |

## 两个坑（已定位）

**① NetworkPolicy 类型错了会让公网 503/401。** 首版用了 `networking.k8s.io` 的
NetworkPolicy + namespaceSelector；Cilium Gateway 的流量身份是 **ingress entity**
（envoy 跑在 cilium-agent 内），namespace 选择器拦不到它 → 网关对后端返回 503，
Pangolin 再把 503 转成 401。必须用 `cilium.io/v2` 的 **CiliumNetworkPolicy** +
`fromEntities: [ingress, host, remote-node]`（sponsor-api 图的注释里记着同一个坑）。

**② Pangolin 资源的默认策略可能开了 SSO。** 实测 `virgo-img.apikv.com` 对应的
`resourcePolicies.sso = 1`，于是 Pangolin 在路由前就返回 401 `Unauthorized`（纯文本、
`nosniff`，与后端的 JSON 错误明显不同）；工作的 `virgo` 资源是 `sso = 0`。
修法：控制台把该资源的认证关掉（Set authentication → None / 关 SSO）。

## Pangolin 双域名（已定稿）

| 域名 | Site | IP | Port | Method | Path | 认证 |
| --- | --- | --- | --- | --- | --- | --- |
| `virgo-auth.apikv.com` | `k8s-cluster`（site 11） | `10.10.31.240` | 443 | https | **不填** | **关**（SSO off） |
| `virgo-img.apikv.com` | `node4`（site 16） | `127.0.0.1` | 8765 | http | **不填** | **关**（SSO off） |

`10.10.31.240` 是 cilium-gateway 的 VIP，与 `sponsor-api.apikv.com` 用的同一个。
认证必须关：实测该资源的默认策略 `sso=1` 会让 Pangolin 在路由前直接返回纯文本
`Unauthorized`（401），与后端的 JSON 错误形态不同。

两条自检：

```bash
curl -sI https://virgo-auth.apikv.com/auth/login | head -2   # 期望 302 → casdoor
curl -s  https://virgo-auth.apikv.com/v1/entitlements        # 期望 401 not_signed_in
curl -s -o /dev/null -w '%{http_code}\n' \
  'https://virgo-img.apikv.com/img?url=pbs.twimg.com/media/HW0oDBlbIAE-B5P.jpg'  # 期望 401（无令牌）
```

## 剩余事项

1. **Casdoor 回调校验**（人工）：浏览器打开 `/auth/login?client=loopback&port=53001&state=abc`
   并完成一次真实登录，确认 Casdoor 不报 `invalid redirect_uri`，且回跳落在
   `127.0.0.1:53001`。需要应用里已注册 `https://virgo-auth.apikv.com/auth/callback`。
2. **node4 静态令牌**：`devtoken` 已于 2026-11-25 从 `tokens.json` 移除
   （`tokens.json` 现为空）；之后内部令牌一律走 `scripts/issue-token.sh` 的有限配额。

## 客户端接入实测（2026-11-25）

| 项 | 结果 |
| --- | --- |
| `GET /auth/login?client=loopback&port=53001&state=abc_123` | 302 → Casdoor，`Set-Cookie: virgo_login_client=loopback\|53001\|abc_123`（`HttpOnly; Secure; SameSite=Lax`） |
| `client=loopback&port=80` | 400 `bad_client`（端口必须在 1024–65535） |
| `client=desktop` | 302 → Casdoor，cookie=`desktop`（`virgo://auth` 回跳保留） |
| 受控激活（服务端签好的合法票据，跳过 Casdoor） | Rust 客户端 `activate` 200 → 令牌可用 → 设备证明 `refresh` 200 → `unbind` 200，绑定表清理后 0 行 |
| Rust 签名 ↔ Go 验签 | Rust（p256）产出的 SPKI 公钥与 ASN.1 DER 签名，Go `x509.ParsePKIXPublicKey` + `ecdsa.VerifyASN1` 通过 |
| 错误路径 | 无票据/伪造票据 401、非 Pro 403、解绑冷却 429、被挤掉设备的证明 401 `bad_proof`（2026-11-27 实测） |
| Pro 令牌取图 | 200，140459 字节 JPEG（1200×480），1.69s |
| 静态令牌 `devtoken`（已移除） | 401 `unknown token`（移除后立即失效，热加载生效） |

## 真机登录实测（2026-11-26，vivo V2425A / Android 17）

| 项 | 结果 |
| --- | --- |
| 真人 Casdoor 登录 → 回跳 | 浏览器完成登录 → `virgo://auth?ticket=…` 拉起应用 → 激活 200；`device_bindings` 写入 `ca8ceec3-3345-48ce-b2db-40afe710eb69` / `Android` |
| 设置页登录态 | 账号 / 设备 / 「Pro 有效」/ 绑定时间 / 「已启用 · 令牌剩余约 12 小时」全部正确 |
| 刷新令牌 | 点击后 `proof_nonces` 新增一行（Keystore `unseal` + P-256 签名 + HTTPS 全通） |
| 非 Pro 账号 | 403 `not_pro`；界面显示「这个账号不是 Pro 订阅」，不写绑定行 |
| 重启后持久化 | force-stop 重启：账号恢复、启动自动 refresh 成功（新 nonce），无「私钥不在本机」 |
| 解绑 | 两步确认 → 绑定删除 + `device_unbinds` 一行 + 本机私钥清除，界面回到未登录 |
| release 包三处修复 | ①R8 keep（`seal`/`unseal` 原被改名成 `a`/`d`）②`reqwest` 改走随包 webpki 根证书（`rustls-platform-verifier` 未初始化会 panic）③私钥惰性读取（Android 插件晚于 `.manage()` 登记，构造期 `unseal` 必失败） |
