#!/usr/bin/env python3
"""
Virgo 图片代理（Pro 订阅侧的回源节点）。

只做一件事：把被阻断的 twimg CDN 地址取回来，原样返回字节。设计约束：

1. **白名单**：只放行 `pbs.twimg.com` / `abs.twimg.com`，其余一律 403。
   不做通用代理——开放代理会被拿去当翻墙出口，轻则打满带宽，重则封 IP。
2. **令牌**：`Authorization: Bearer <token>` 或 `?token=`。令牌表放在
   `tokens.json`，每个令牌带配额与到期时间，热加载（改文件即生效）。
3. **配额**：按令牌计「字节数 + 请求数」双配额，按 UTC 自然日重置。
   计数落盘（`state.json`），重启不清零。
4. **磁盘缓存**：图片 URL 内容不可变，按键缓存到 `cache/`，命中直接回盘，
   省掉回源与内存；缓存总量上限可配，超了按最旧淘汰。
5. **零第三方依赖**：node4 上只有 python3，不装 nginx / go / redis。

运维接口（只监听本机，公网不暴露）：
  GET /healthz     存活探针
  GET /_stats      各令牌当日用量（需要 admin 令牌）
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import re
import threading
import time
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, unquote, urlparse

try:
    import ed25519_verify  # 同目录：Pro 短期令牌的验签实现
except ImportError:  # 部署时应与 imgproxy.py 同目录
    ed25519_verify = None

# ---- 设备证明（Pro 令牌的第二个因子） ----
# virgo-auth 签发的令牌里带 `dpk`（设备 P-256 公钥，SPKI DER base64）。带 dpk 的
# 令牌，每个取图请求都必须再附 `X-Virgo-Ts` / `X-Virgo-Nonce` / `X-Virgo-Sig`，
# 签的是 `GET\n{path}\n{ts}\n{nonce}`。这样令牌被拷到别的机器也签不出来。
PROOF_TS_SKEW = 300
_PROOF_NONCES: dict[str, float] = {}
_PROOF_NONCE_LOCK = threading.Lock()


def _b64decode_any(value: str) -> bytes:
    value = (value or "").strip()
    if not value:
        raise ValueError("empty base64")
    padded = value + "=" * (-len(value) % 4)
    try:
        return base64.urlsafe_b64decode(padded)
    except Exception:
        return base64.b64decode(padded)


def _nonce_seen(nonce: str) -> bool:
    """一次性 nonce：记到过期为止；重复出现直接判重放。"""
    now = time.time()
    with _PROOF_NONCE_LOCK:
        for key, expires in list(_PROOF_NONCES.items()):
            if expires <= now:
                del _PROOF_NONCES[key]
        if nonce in _PROOF_NONCES:
            return True
        _PROOF_NONCES[nonce] = now + PROOF_TS_SKEW * 2
        if len(_PROOF_NONCES) > 4096:
            oldest = sorted(_PROOF_NONCES, key=_PROOF_NONCES.get)[: len(_PROOF_NONCES) // 4]
            for key in oldest:
                del _PROOF_NONCES[key]
        return False


def verify_device_proof(claims: dict, method: str, path: str, headers) -> str | None:
    """带 dpk 的令牌必须附设备证明；返回 None 表示通过，否则返回原因。"""
    device_pub = (claims.get("dpk") or "").strip()
    if not device_pub:
        return None
    ts = (headers.get("X-Virgo-Ts") or "").strip()
    nonce = (headers.get("X-Virgo-Nonce") or "").strip()
    sig = (headers.get("X-Virgo-Sig") or "").strip()
    if not (ts and nonce and sig):
        return "缺少设备证明"
    try:
        ts_int = int(ts)
    except ValueError:
        return "设备证明时间戳无效"
    if abs(int(time.time()) - ts_int) > PROOF_TS_SKEW:
        return "设备证明已过期"
    if not re.fullmatch(r"[A-Za-z0-9_-]{4,64}", nonce):
        return "设备证明 nonce 无效"
    if _nonce_seen(nonce):
        return "设备证明重复使用"
    try:
        from cryptography.exceptions import InvalidSignature
        from cryptography.hazmat.primitives import hashes, serialization
        from cryptography.hazmat.primitives.asymmetric import ec
    except Exception:
        return "imgproxy 没有 cryptography，无法校验设备证明"
    try:
        public_key = serialization.load_der_public_key(_b64decode_any(device_pub))
    except Exception:
        return "设备公钥无效"
    message = f"{method}\n{path}\n{ts_int}\n{nonce}".encode()
    try:
        public_key.verify(_b64decode_any(sig), message, ec.ECDSA(hashes.SHA256()))
    except InvalidSignature:
        return "设备证明签名不对"
    except Exception as exc:
        return f"设备证明校验失败：{exc}"
    return None

ALLOWED_HOSTS = ("pbs.twimg.com", "abs.twimg.com")
# 目标地址只接受「host/path?query」形式，不允许再嵌 scheme（防 SSRF 绕过）。
TARGET_PATTERN = re.compile(r"^(?:(?:pbs|abs)\.twimg\.com)(?:/[^\s]*)?$")
USER_AGENT = (
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) "
    "AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
)
MAX_IMAGE_BYTES = 12 * 1024 * 1024


class Store:
    """令牌表与用量计数：两份 JSON 文件，改动后自动重新载入。"""

    def __init__(self, root: Path) -> None:
        self.root = root
        self.tokens_path = root / "tokens.json"
        self.state_path = root / "state.json"
        self.cache_dir = root / "cache"
        self.cache_dir.mkdir(parents=True, exist_ok=True)
        self._lock = threading.Lock()
        self._tokens: dict[str, dict] = {}
        self._tokens_mtime = 0.0
        self._state: dict[str, dict] = {}
        self._load_tokens()
        self._load_state()

    # ── 令牌 ────────────────────────────────────────────────────────────
    def _load_tokens(self) -> None:
        try:
            mtime = self.tokens_path.stat().st_mtime
        except OSError:
            self._tokens = {}
            return
        if mtime == self._tokens_mtime:
            return
        try:
            raw = json.loads(self.tokens_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            return
        tokens = {}
        for entry in raw.get("tokens", []):
            token = entry.get("token")
            if isinstance(token, str) and token:
                tokens[token] = {
                    "plan": entry.get("plan", "pro"),
                    # 0 表示不限；按 UTC 日重置。
                    "daily_requests": int(entry.get("daily_requests", 0)),
                    "daily_bytes": int(entry.get("daily_bytes", 0)),
                    "expires_at": entry.get("expires_at"),  # epoch 秒，null 表示不过期
                    "note": entry.get("note", ""),
                }
        self._tokens = tokens
        self._tokens_mtime = mtime

    def authenticate(self, token: str) -> tuple[dict | None, str]:
        self._load_tokens()
        entry = self._tokens.get(token)
        if not entry:
            return None, "unknown token"
        expires = entry.get("expires_at")
        if isinstance(expires, (int, float)) and time.time() > expires:
            return None, "token expired"
        return entry, ""

    # ── 用量 ────────────────────────────────────────────────────────────
    def _load_state(self) -> None:
        try:
            self._state = json.loads(self.state_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            self._state = {}

    def _save_state(self) -> None:
        tmp = self.state_path.with_suffix(".tmp")
        tmp.write_text(json.dumps(self._state, ensure_ascii=False), encoding="utf-8")
        os.replace(tmp, self.state_path)

    @staticmethod
    def _today() -> str:
        return time.strftime("%Y-%m-%d", time.gmtime())

    def usage(self, token: str) -> dict:
        with self._lock:
            day = self._today()
            record = self._state.get(token) or {}
            if record.get("day") != day:
                record = {"day": day, "requests": 0, "bytes": 0}
            return dict(record)

    def charge(self, token: str, amount: int, *, counted_request: bool = True) -> bool:
        """记账；超出日配额返回 False（本次请求应当拒绝）。

        `counted_request=False` 用于缓存命中：只累加字节、不占请求次数，
        这样重看同一篇文章不会把请求配额吃掉。
        """
        with self._lock:
            day = self._today()
            record = self._state.get(token) or {}
            if record.get("day") != day:
                record = {"day": day, "requests": 0, "bytes": 0}
            entry = self._tokens.get(token, {})
            limit_req = int(entry.get("daily_requests", 0))
            limit_bytes = int(entry.get("daily_bytes", 0))
            if counted_request and limit_req and record["requests"] + 1 > limit_req:
                return False
            if limit_bytes and record["bytes"] + amount > limit_bytes:
                return False
            if counted_request:
                record["requests"] += 1
            record["bytes"] += amount
            self._state[token] = record
            self._save_state()
            return True

    def snapshot(self) -> dict:
        with self._lock:
            day = self._today()
            out = {}
            for token, entry in self._tokens.items():
                record = self._state.get(token) or {}
                if record.get("day") != day:
                    record = {"day": day, "requests": 0, "bytes": 0}
                out[entry.get("note") or token[:8]] = {
                    "plan": entry.get("plan"),
                    "requests": record["requests"],
                    "bytes": record["bytes"],
                }
            return out

    # ── 缓存 ────────────────────────────────────────────────────────────
    def cache_key(self, target: str) -> str:
        return hashlib.sha256(target.encode("utf-8")).hexdigest()[:40]

    def cache_get(self, key: str) -> tuple[bytes, str] | None:
        path = self.cache_dir / key
        meta = self.cache_dir / f"{key}.type"
        try:
            return path.read_bytes(), meta.read_text(encoding="utf-8").strip()
        except OSError:
            return None

    def cache_put(self, key: str, body: bytes, content_type: str, limit_bytes: int) -> None:
        try:
            (self.cache_dir / key).write_bytes(body)
            (self.cache_dir / f"{key}.type").write_text(content_type, encoding="utf-8")
        except OSError:
            return
        self._evict(limit_bytes)

    def _evict(self, limit_bytes: int) -> None:
        try:
            entries = [
                (p.stat().st_mtime, p)
                for p in self.cache_dir.iterdir()
                if p.is_file() and p.suffix != ".type"
            ]
        except OSError:
            return
        total = sum(size for _, p in entries for size in [p.stat().st_size])
        if total <= limit_bytes:
            return
        for _, path in sorted(entries):
            try:
                size = path.stat().st_size
                path.unlink()
                sidecar = path.with_suffix(".type")
                if sidecar.exists():
                    sidecar.unlink()
                total -= size
            except OSError:
                continue
            if total <= limit_bytes:
                break


def fetch_upstream(target: str, timeout: float) -> tuple[bytes, str]:
    request = urllib.request.Request(
        f"https://{target}",
        headers={"User-Agent": USER_AGENT, "Accept": "image/*,*/*;q=0.8"},
    )
    with urllib.request.urlopen(request, timeout=timeout) as response:
        status = getattr(response, "status", 200)
        if status != 200:
            raise urllib.error.HTTPError(target, status, "upstream", response.headers, None)
        content_type = response.headers.get("Content-Type", "application/octet-stream")
        body = response.read(MAX_IMAGE_BYTES + 1)
    if not body:
        raise ValueError("empty body")
    if len(body) > MAX_IMAGE_BYTES:
        raise ValueError("image too large")
    return body, content_type


def _decode_key(raw: str) -> bytes:
    """解 base64 / base64url（两种都接受，缺填充自动补）。"""
    text = raw.strip()
    try:
        return base64.b64decode(text + "=" * (-len(text) % 4))
    except Exception:
        return base64.urlsafe_b64decode(text + "=" * (-len(text) % 4))


class TokenVerifier:
    """验 Pro 短期令牌：`base64url(payload).base64url(sig)`，Ed25519 签名。

    与 virgo-auth 的 token 包对应：payload 是 JSON claims，签名字符串不含
    点号以外的分隔。公钥由 virgo-auth 导出后用环境变量注入。
    """

    def __init__(self, public_key_path: str | None) -> None:
        self.public_key = None
        if not public_key_path:
            return
        try:
            raw = Path(public_key_path).read_text(encoding="utf-8").strip()
            # virgo-auth 导出的是 base64url 无填充（Ed25519 公钥 32 字节），
            # 两种解码都试一下，避免「auth 导出的格式 imgproxy 解不了」。
            self.public_key = _decode_key(raw)
        except OSError:
            self.public_key = None

    @property
    def enabled(self) -> bool:
        return self.public_key is not None and ed25519_verify is not None

    def verify(self, raw_token: str) -> dict | None:
        if not self.enabled or not raw_token:
            return None
        parts = raw_token.split(".")
        if len(parts) != 2:
            return None
        try:
            payload = base64.urlsafe_b64decode(parts[0] + "=" * (-len(parts[0]) % 4))
            signature = base64.urlsafe_b64decode(parts[1] + "=" * (-len(parts[1]) % 4))
        except Exception:
            return None
        if not ed25519_verify.verify(self.public_key, payload, signature):
            return None
        try:
            claims = json.loads(payload)
        except json.JSONDecodeError:
            return None
        if int(claims.get("exp", 0)) < time.time():
            return None
        return claims


class Handler(BaseHTTPRequestHandler):
    server_version = "VirgoImgProxy/1"
    protocol_version = "HTTP/1.1"

    store: Store
    verifier: TokenVerifier
    admin_token: str
    cache_limit: int
    upstream_timeout: float

    def log_message(self, fmt: str, *args) -> None:  # 静默：由 journald 收正式日志
        return

    def _json(self, status: int, payload: dict) -> None:
        body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _pro_claims(self) -> dict | None:
        """从 `X-Virgo-Token` 头或 `?vgt=` 取出并验签；无效返回 None。"""
        raw = self.headers.get("X-Virgo-Token", "").strip()
        if not raw:
            query = parse_qs(urlparse(self.path).query)
            raw = (query.get("vgt") or [""])[0]
        if not raw:
            return None
        claims = self.verifier.verify(raw)
        if claims is None:
            return None
        proof_error = verify_device_proof(claims, "GET", self.path, self.headers)
        if proof_error is not None:
            # 标记具体原因，do_GET 会把它作为 401 的正文，方便客户端排错。
            self.auth_error = proof_error
            return None
        return claims

    def _host_allowed_for_pro(self, claims: dict, target: str) -> bool:
        """令牌可选地收窄可回源主机（`hosts` 字段，逗号分隔）。"""
        allowed = (claims.get("hosts") or "").strip()
        if not allowed:
            return True
        host = target.split("/", 1)[0].lower()
        return host in {h.strip().lower() for h in allowed.split(",") if h.strip()}

    def _bearer(self, query: dict[str, list[str]]) -> str:
        header = self.headers.get("Authorization", "")
        if header.lower().startswith("bearer "):
            return header[7:].strip()
        if query.get("token"):
            return query["token"][0]
        return ""

    def do_GET(self) -> None:  # noqa: N802
        parsed = urlparse(self.path)
        query = parse_qs(parsed.query)

        if parsed.path == "/healthz":
            self._json(200, {"ok": True})
            return

        if parsed.path == "/_stats":
            if self._bearer(query) != self.admin_token:
                self._json(403, {"error": "forbidden"})
                return
            self._json(200, {"usage": self.store.snapshot()})
            return

        if parsed.path != "/img":
            self._json(404, {"error": "not found"})
            return

        target = unquote(query.get("url", [""])[0]).lstrip().lstrip("/")
        if not target or not TARGET_PATTERN.match(target):
            # 明确拒绝不在白名单内的目标，避免变成开放代理。
            self._json(403, {"error": "host not allowed"})
            return

        # 两条鉴权路径，任一通过即可：
        #   1) Pro 短期令牌（Ed25519，virgo-auth 签发，绑定设备）——推荐
        #   2) 静态 Bearer 令牌（tokens.json，手工发放，用于开发/内部）
        self.auth_error = ""
        pro_claims = self._pro_claims()
        if pro_claims is not None:
            if not self._host_allowed_for_pro(pro_claims, target):
                self._json(403, {"error": "host not allowed for this token"})
                return
            token = "pro:" + pro_claims.get("sub", "")
        else:
            token = self._bearer(query)
            entry, reason = self.store.authenticate(token)
            if entry is None:
                self._json(401, {"error": self.auth_error or reason or "unauthorized"})
                return

        key = self.store.cache_key(target)
        cached = self.store.cache_get(key)
        if cached is not None:
            # 缓存命中的流量实际不经过回源，但**仍然占用你的出口带宽**，
            # 所以按字节计费、不计请求数：重看同一篇文章不会被重复扣次。
            body, content_type = cached
            if not self.store.charge(token, len(body), counted_request=False):
                self._json(429, {"error": "daily byte quota exceeded"})
                return
            self._send_image(body, content_type, hit=True)
            return

        try:
            body, content_type = fetch_upstream(target, self.upstream_timeout)
        except urllib.error.HTTPError as exc:
            self._json(502, {"error": f"upstream {exc.code}"})
            return
        except Exception as exc:  # 超时/DNS/证书等
            self._json(504, {"error": f"upstream failed: {exc}"})
            return

        if not self.store.charge(token, len(body)):
            self._json(429, {"error": "daily quota exceeded"})
            return
        self.store.cache_put(key, body, content_type, self.cache_limit)
        self._send_image(body, content_type, hit=False)

    def _send_image(self, body: bytes, content_type: str, hit: bool) -> None:
        self.send_response(200)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "public, max-age=604800")
        self.send_header("X-Virgo-Cache", "hit" if hit else "miss")
        self.end_headers()
        self.wfile.write(body)


def main() -> None:
    parser = argparse.ArgumentParser(description="Virgo image proxy")
    parser.add_argument("--root", default=os.environ.get("VIRGO_IMGPROXY_ROOT", "/srv/virgo-imgproxy"))
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8765)
    parser.add_argument("--cache-gb", type=float, default=4.0)
    parser.add_argument("--upstream-timeout", type=float, default=20.0)
    parser.add_argument("--pro-pubkey", default="", help="Pro 令牌验签公钥（base64 文本文件）")
    args = parser.parse_args()

    root = Path(args.root)
    root.mkdir(parents=True, exist_ok=True)
    Handler.store = Store(root)
    Handler.verifier = TokenVerifier(args.pro_pubkey or os.environ.get("VIRGO_PRO_PUBKEY_FILE", ""))
    Handler.admin_token = os.environ.get("VIRGO_IMGPROXY_ADMIN_TOKEN", "")
    Handler.cache_limit = int(args.cache_gb * 1024**3)
    Handler.upstream_timeout = args.upstream_timeout

    server = ThreadingHTTPServer((args.host, args.port), Handler)
    server.daemon_threads = True
    print(f"virgo-imgproxy listening on {args.host}:{args.port}", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
