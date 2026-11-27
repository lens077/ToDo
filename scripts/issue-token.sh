#!/usr/bin/env bash
# 发放一个图片代理令牌：生成随机串，打印可直接粘贴的 JSON 片段与设置页要填的值。
#
#   ./scripts/issue-token.sh user@example.com [每日请求数] [每日字节数] [有效天数]
#
# 默认：2000 次/日、512 MiB/日、365 天有效。0 表示不限。
set -euo pipefail

note="${1:?用法: issue-token.sh <备注/邮箱> [每日请求数] [每日字节数] [有效天数]}"
reqs="${2:-2000}"
bytes="${3:-536870912}"
days="${4:-365}"

token="$(head -c 24 /dev/urandom | base64 | tr -d '/+=' | head -c 32)"
if [[ "$days" == "0" ]]; then
  expires="null"
else
  expires="$(( $(date +%s) + days * 86400 ))"
fi

cat <<JSON
{
  "token": "$token",
  "plan": "pro",
  "daily_requests": $reqs,
  "daily_bytes": $bytes,
  "expires_at": $expires,
  "note": "$note"
}
JSON

cat <<EOF

—— 客户端「设置 → 图片代理」要填 ——
代理地址：https://img.example.com/img?url=
访问令牌：$token
（有效期至：$(date -r "$(( ${expires/ null/0} ))" '+%Y-%m-%d' 2>/dev/null || echo '不限')）
EOF
