#!/usr/bin/env bash
# 幂等配置 Casdoor：Virgo Todo 的 Pro 角色 / Plan / Pricing，以及应用回调与签名证书。
#
#   CASDOOR_URL=https://casdoor.apikv.com \
#   CASDOOR_TOKEN='<JWT>' \
#   ./casdoor-setup.sh
#
# token 只传 JWT（`Authorization: Bearer <JWT>`）。Casdoor 的「用户名,令牌」形式在
# role/group 端点会返回 Unauthorized operation（2026-11-26 实测）。只读环境变量，不落盘。
set -euo pipefail

CASDOOR_URL="${CASDOOR_URL:?需要 CASDOOR_URL}"
CASDOOR_TOKEN="${CASDOOR_TOKEN:?需要 CASDOOR_TOKEN（用户名,令牌）}"
OWNER="${OWNER:-lens}"
APP="${APP:-virgo-todo}"
ROLE="${ROLE:-virgo_pro}"
PLAN="${PLAN:-virgo-pro}"
PRICING="${PRICING:-virgo-todo}"
REDIRECT_URI="${REDIRECT_URI:-https://virgo-auth.apikv.com/auth/callback}"
CERT="${CERT:-virgo-todo}"
PRODUCT="${PRODUCT:-}"

api() { # api <method> <path> [json]
  local method="$1" path="$2" body="${3:-}"
  if [[ -n "$body" ]]; then
    curl -sS --max-time 20 -X "$method" "$CASDOOR_URL$path" \
      -H "Authorization: Bearer $CASDOOR_TOKEN" -H "Content-Type: application/json" -d "$body"
  else
    curl -sS --max-time 20 -X "$method" "$CASDOOR_URL$path" -H "Authorization: Bearer $CASDOOR_TOKEN"
  fi
}

ok() { python3 -c 'import sys,json; d=json.load(sys.stdin); print("ok" if d.get("status")=="ok" else "ERR: "+str(d.get("msg")))'; }

echo "== 1. 角色 $OWNER/$ROLE"
api GET "/api/get-role?id=$OWNER%2F$ROLE" | ok
api PUT /api/add-role "{\"owner\":\"$OWNER\",\"name\":\"$ROLE\",\"displayName\":\"Virgo Pro\",\"description\":\"Virgo Todo 的 Pro 订阅权限组\",\"isEnabled\":true}" >/dev/null 2>&1 || true
api GET "/api/get-role?id=$OWNER%2F$ROLE" | ok

echo "== 2. 套餐 $OWNER/$PLAN（role 绑到上面的角色）"
api PUT /api/add-plan "{\"owner\":\"$OWNER\",\"name\":\"$PLAN\",\"displayName\":\"Virgo Pro\",\"role\":\"$ROLE\",\"period\":\"1 month\",\"price\":29,\"currency\":\"CNY\",\"isEnabled\":true}" >/dev/null 2>&1 || true
api GET "/api/get-plan?id=$OWNER%2F$PLAN" | ok

GROUP="${GROUP:-virgo-pro}"
echo "== 2b. 组 $OWNER/$GROUP（与角色同名的用户组，成员由订阅流程写入）"
api GET "/api/get-group?id=$OWNER%2F$GROUP" | ok
api PUT /api/add-group "{\"owner\":\"$OWNER\",\"name\":\"$GROUP\",\"displayName\":\"Virgo Pro\",\"type\":\"Physical\",\"isEnabled\":true}" >/dev/null 2>&1 || true
api GET "/api/get-group?id=$OWNER%2F$GROUP" | ok

echo "== 3. 定价页 $OWNER/$PRICING"
api PUT /api/add-pricing "{\"owner\":\"$OWNER\",\"name\":\"$PRICING\",\"displayName\":\"Virgo Todo\",\"plans\":[\"$PLAN\"],\"application\":\"$APP\",\"isEnabled\":true}" >/dev/null 2>&1 || true
api GET "/api/get-pricing?id=$OWNER%2F$PRICING" | ok

echo "== 4. 应用回调与签名证书"
app_json=$(api GET "/api/get-application?id=admin%2F$APP" | python3 -c '
import sys,json
d=json.load(sys.stdin)["data"]
print(json.dumps(d))')
updated=$(printf '%s' "$app_json" | python3 -c "
import sys,json
d=json.load(sys.stdin)
uris=set(d.get('redirectUris') or [])
uris.add('$REDIRECT_URI')
# 旧的 localhost:9000 开发入口不再保留：回调只认 virgo-auth 域名。
uris.discard('http://localhost:9000/callback')
d['redirectUris']=sorted(uris)
d['cert']='$CERT'
print(json.dumps(d))")
api POST "/api/update-application?id=admin%2F$APP" "$updated" | ok
echo
echo "完成。应用回调：$(api GET "/api/get-application?id=admin%2F$APP" | python3 -c 'import sys,json;d=json.load(sys.stdin)["data"];print(d.get("redirectUris"),"cert="+str(d.get("cert")))')"
