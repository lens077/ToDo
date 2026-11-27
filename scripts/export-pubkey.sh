#!/usr/bin/env bash
# 从 virgo-auth 的 Ed25519 私钥导出验签公钥（base64url），交给 imgproxy。
#
#   ./scripts/export-pubkey.sh /srv/virgo-auth/token.ed25519
#
# 需要在能读到私钥的地方执行；输出即 imgproxy 的 pro.pub 内容。
set -euo pipefail

key="${1:-/srv/virgo-auth/token.ed25519}"
if [[ ! -f "$key" ]]; then
  echo "找不到私钥：$key" >&2
  exit 1
fi

python3 - "$key" <<'PY'
import base64, sys
raw = open(sys.argv[1], "rb").read()
if len(raw) != 64:
    sys.exit(f"私钥长度应为 64 字节（Ed25519），实际 {len(raw)}")
# Ed25519 私钥 = 32 字节种子 + 32 字节公钥；后 32 字节就是公钥。
print(base64.urlsafe_b64encode(raw[32:]).decode().rstrip("="))
PY
