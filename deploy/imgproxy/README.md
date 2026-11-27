# Virgo 图片代理（Pro 回源）

## 当前链路（2026-11-27，v4imgd 修复后的生产拓扑）

```
浏览器 / App
  └─ https://virgo-img.apikv.com/img?url=pbs.twimg.com%2F...
       │  （DNS → 193.112.22.183 = node3；TLS 由 node3 上的 Pangolin Traefik 终结）
       ▼
  Pangolin resource 92  target: site `node3-local`(siteId 15) / 127.0.0.1:8765
                        internalPort 47041，authToken 开启，mode http
       ▼
  node3 本机 newt（site node3-local，exitNode）把流量投递到 node3 的 127.0.0.1:8765
       ▼
  反向 SSH 隧道（node4 主动连 node3 的映射端口 34125）
       systemd: node4 上的 virgo-img-tunnel
       ssh -N -R 127.0.0.1:8765:127.0.0.1:8765 root@193.112.22.183 -p 34125
       ▼
  node4 的 imgproxy 127.0.0.1:8765（systemd virgo-imgproxy，旧版备份 imgproxy.py.bak-20261126）
       ▼
  pbs.twimg.com（只有 node4 能直连；node1/3/5/6 实测都取不到 X CDN）
```

为什么要绕这一跳：

- **node4 在 NAT 后面，只映射了 SSH（TCP 34126）**。它的 newt 与 Pangolin 的 WireGuard 数据面走 UDP 51820/21820，回包到不了 node4（在 node4 抓包只有出站 initiate；node3 侧 tcpdump 能看到 init 与回包）。因此 site node4（siteId 16）上的 resource 永远超时。
- **node3 是公网入口但取不到 X CDN**（`pbs.twimg.com` 直连 000），所以 imgproxy 不能落在 node3。
- 组合起来：`公网 → node3（Pangolin + node3-local newt）→ 反向 SSH → node4（能访问 X 的 imgproxy）`。

替换方案时，**只需要替换「节点 → 隧道」这一段**：让 virgo-img 的 resource target 指向一个能访问 `127.0.0.1:8765` 的 site（或直接在 k8s/公网主机上跑 imgproxy 并把 target 指向该 site）。
不要同时把 `deploy/imgproxy/imgproxy.py` 的验签逻辑（Ed25519 令牌 + `dpk` 设备证明）换掉，那是与客户端约定好的。

**2026-11-27 实测**（公网链路，不是本机）：`https://virgo-img.apikv.com/healthz` → 200（连测 3 次）；
无令牌取图 → 401 `{"error": "unknown token"}`；用受控票据激活的服务端令牌 + P-256 设备证明
从公网取 `pbs.twimg.com/profile_images/1683325380441128960/yRsRRjGO.jpg` → **200 / 12302 B JPEG**。
测试绑定与 nonce 已从 PG 清零。

X（推特）文章的图片 CDN（`pbs.twimg.com`）在部分网络下不可路由，正文却能通过
FxTwitter API 正常抓取，结果就是「文章存下来了、`img/` 是空的」。这个服务是
Pro 订阅的回源节点：客户端直连失败后，把图片地址交给它代取。

## 拓扑

```
Virgo 客户端 ──直连失败──▶ img.example.com/img?url=pbs.twimg.com/media/x.jpg
                              │  （node4，普通境外 VPS）
                              └──▶ pbs.twimg.com  （2.3 MB/s，50ms）
```

未订阅（设置里未登录 Pro）：**完全不走代理**，图片保留下载失败的原始链接，
正文照常可读。不把图片地址交给任何第三方。

订阅后（设置 → 图片代理 → 登录 Pro 账号）：直连 → 自建代理 → weserv → 保留原 URL。
客户端自己拿地址（`https://virgo-img.apikv.com/img?url=`）与短期令牌，用户不再手填。

## 服务端

单文件 Python 标准库实现（`imgproxy.py`），无第三方依赖、无 nginx、无 redis。

| 约束 | 值 |
| --- | --- |
| 监听 | `127.0.0.1:8765`（对外由 nginx/Caddy 反代，或 SSH 隧道） |
| 白名单 | `pbs.twimg.com`、`abs.twimg.com` 两域，其余 403 |
| 鉴权 | ①`X-Virgo-Token`：virgo-auth 签发的 Ed25519 短期令牌（主路径）②`Authorization: Bearer <token>` 或 `?token=`：tokens.json 静态令牌（内部排障） |
| 配额 | 每令牌「每日请求数 + 每日字节数」，UTC 日切 |
| 缓存 | 磁盘 LRU，默认 4 GB，命中不计请求配额 |
| 单图上限 | 12 MB |

### 首次部署

```bash
scp deploy/imgproxy/imgproxy.py root@<host>:/root/imgproxy.py
scp deploy/imgproxy/virgo-imgproxy.service root@<host>:/etc/systemd/system/
ssh root@<host> '
  mkdir -p /srv/virgo-imgproxy
  printf "VIRGO_IMGPROXY_ADMIN_TOKEN=%s\n" "$(head -c 24 /dev/urandom | base64 | tr -d "/+=")" \
    > /etc/virgo-imgproxy.env
  chmod 600 /etc/virgo-imgproxy.env
  systemctl daemon-reload
  systemctl enable --now virgo-imgproxy
  systemctl is-active virgo-imgproxy
'
```

### 对外暴露（三选一）

1. **SSH 隧道**（最省事，仅自用）：`ssh -N -L 8765:127.0.0.1:8765 <host>`，
   设置里填 `http://127.0.0.1:8765/img?url=`。
2. **nginx 反代 + TLS**（推荐给付费用户）：`proxy_pass http://127.0.0.1:8765;`，
   证书用 certbot。
3. **Caddy**：`reverse_proxy 127.0.0.1:8765`，自动签证书。

无论哪种，**不要**把 8765 直接裸露在公网：服务本身靠令牌防护，但没有任何
DDoS 缓冲。

### 令牌管理

主路径是 Pro 短期令牌（见 `deploy/virgo-auth`）：客户端登录后拿 12 小时令牌，
令牌里带 `sub` 与 `hosts` 白名单，imgproxy 用 `/srv/virgo-imgproxy/pro.pub` 验签。

静态令牌只留给内部排障。**2026-11-25 已把不限配额的 `devtoken` 从 node4
移除**（`tokens.json` 现为 `{"tokens": []}`），之后要发内部令牌请走
`scripts/issue-token.sh`（默认 2000 次 / 512 MiB / 365 天，不要再用 0=不限）。

`/srv/virgo-imgproxy/tokens.json`，改完**无需重启**（按 mtime 热加载）：

```json
{
  "tokens": [
    {
      "token": "发给用户的随机串",
      "plan": "pro",
      "daily_requests": 2000,
      "daily_bytes": 536870912,
      "expires_at": 1798761600,
      "note": "user@example.com"
    }
  ]
}
```

- `daily_requests` / `daily_bytes` 为 `0` 表示不限。
- `expires_at` 是 epoch 秒，`null` 表示不过期。
- `note` 用于 `/_stats` 里辨认是谁。

新增令牌用 `scripts/issue-token.sh`（生成随机串并打印 JSON 片段）。

### 观测

```bash
# 各令牌当日用量（需要 admin 令牌，见 /etc/virgo-imgproxy.env）
curl -s -H "Authorization: Bearer $ADMIN" http://127.0.0.1:8765/_stats

journalctl -u virgo-imgproxy -f      # 进程日志
du -sh /srv/virgo-imgproxy/cache     # 缓存占用
```

### 容量

2C / 2G 的机器上，瓶颈是**带宽**而不是 CPU：回源 2.3 MB/s，单张 120 KB 的图
约 50ms。真正限制并发用户数的是出口带宽——按每篇文章 6 张图、每张 120 KB 估算，
一篇文章约 0.7 MB。每天 500 篇文章 ≈ 350 MB 出口。

## 客户端

设置 → 图片代理（Pro）只显示登录态，没有可手填的地址与令牌：

- 未登录：不代理，失败的图片保留原始链接。
- 已登录 Pro：`apps/desktop/src-tauri/src/auth.rs` 在 Rust 里持有令牌，抓图命令
  直接取用——令牌不进 `localStorage`、不进 `todo.app-settings.v1`、也不经过
  前端。地址前缀是常量 `https://virgo-img.apikv.com/img?url=`。
- 排障仍可显式传参：`reading_add` / `reading_quick_add` / `reading_retry_parse`
  保留 `imageProxyBase` / `imageProxyToken` 可选参数，显式参数优先于登录态。

### 实现要点

- `todo_core::ImageProxyConfig`：`base` + `token`，`is_enabled()` 只看 base。
- `download_image`：候选序列 `[直连, 自建代理, weserv]`，第一个 2xx 胜出；
  代理轮次带 1 秒间隔重试（上游偶发 5xx）。
- 令牌只发给**以配置的 base 开头**的地址，不会泄漏给 weserv。
- `image_client()` 单独 10 秒超时：被墙的 CDN 不会把整篇解析拖住。

## 已知限制

- 客户端目前是**串行**下载图片：一条 6 图文章在 71 KB/s 的链路上约 10 秒。
  改成并发能压到 3~4 秒，但会影响 `localize_images` 的错误处理语义，暂未做。
- 同一张图在缓存命中时仍消耗出口带宽（只是不占请求配额）。
- **设备证明已落地（f8tokn）**：带 `dpk`（设备 P-256 公钥，SPKI base64）的令牌，
  imgproxy 要求每个请求再附 `X-Virgo-Ts` / `X-Virgo-Nonce` / `X-Virgo-Sig`，
  签名对象 `GET\n{path}\n{ts}\n{nonce}`，±300s 时间窗 + nonce 一次性；令牌被
  拷到别的机器签不出证明。无 `dpk` 的老令牌仍按原样放行。
  部署顺序：先发客户端（多发的证明头被旧代理忽略），再上线带 `dpk` 的
  virgo-auth 与新 imgproxy。验签依赖 `cryptography`（node4 已装 41.0.7；
  缺失时带 `dpk` 的请求会 401 并写明原因）。
- 本机 macOS 未启动时，设置的「登录 Pro 账号」依赖浏览器回跳；企业代理拦
  `127.0.0.1` 时需放行。
- 没有内存级热点缓存：4 GB 磁盘缓存对 2G 内存的机器更安全。

## 设备证明（2026-11-26，f8tokn）

请求头三件套：

| 头 | 内容 |
| --- | --- |
| `X-Virgo-Token` | virgo-auth 签的短期令牌（`base64(payload).base64(sig)`，Ed25519） |
| `X-Virgo-Ts` | 签名时的 unix 秒；与本机相差超过 300s 拒绝 |
| `X-Virgo-Nonce` | `[A-Za-z0-9_-]{4,64}`，同一 nonce 只放行一次（TTL 表约 4096 条） |
| `X-Virgo-Sig` | 设备 P-256 私钥对 `GET\n{path}\n{ts}\n{nonce}` 的 DER 签名（base64） |

`path` 必须与 HTTP 请求行一致（`/img?url=pbs.twimg.com%2F...`）。实测：
有效证明 200 / 12302 B JPEG；重放 401「设备证明重复使用」；篡改签名
401「设备证明签名不对」；无 `dpk` 令牌不带证明 200；无令牌 401。
单测在 `tests/test_device_proof.py`（7 例，需 `cryptography`）。
