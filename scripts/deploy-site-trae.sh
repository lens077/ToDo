#!/usr/bin/env bash
# 把 web/dist 发布到 Trae 部署平台（deployment 插件的 trae-deploy CLI）。
#
#   用法：make trae-deploy
#
# 必须在 Trae 会话的终端里运行：CLI 的凭据、项目与会话上下文来自会话环境，
# 普通终端会在预检阶段被拦下。首次运行会创建站点「Virgo 官网」，之后按名复用；
# 已有进行中的部署时只提示，不会并发创建第二个。
set -euo pipefail

CLI="${TRAE_DEPLOY:-}"
SITE_NAME="${TRAE_SITE_NAME:-Virgo 官网}"
SITE_ID="${TRAE_SITE_ID:-}"
ACCESS="${TRAE_ACCESS:-public}"
APP_TAGS="${TRAE_APP_TAGS:-123,289}"
DIST="web/dist"
SITE_DESC="Source: web/dist（由 web/build.mjs 构建）。Sumery Virgo 官方落地页，中英双语静态站。Deployment type: StaticWeb."

die() { printf '%s\n' "$*" >&2; exit 1; }

cli() { sh "$CLI" "$@"; }

# 从 stdin 的 CLI JSON 里取字段（支持 a.b.c），取不到就输出空行。
field() {
  node -e '
    let s = "";
    process.stdin.on("data", (d) => (s += d)).on("end", () => {
      let v;
      try { v = JSON.parse(s); } catch { process.exit(0); }
      for (const k of process.argv[1].split(".")) v = v == null ? undefined : v[k];
      if (v === undefined || v === null) process.exit(0);
      console.log(typeof v === "object" ? JSON.stringify(v) : v);
    });
  ' "$1"
}

[[ -n "$CLI" && -f "$CLI" ]] || die "找不到 Trae 部署 CLI：请确认已安装 deployment 插件，或用 TRAE_DEPLOY=<路径>/trae-deploy 指定。"
command -v node >/dev/null 2>&1 || die "需要 node 来解析 CLI 输出。"
[[ -f "$DIST/index.html" ]] || die "缺少 $DIST/index.html，先执行 make site-build。"

# 1. 预检：凭据、项目与会话上下文。会话外的 shell 会在这里失败。
echo "==> 检查部署权限与会话上下文"
check_json=""
if ! check_json="$(cli check 2>/dev/null)"; then
  echo "部署预检失败：请在 Trae 会话的终端里运行 make trae-deploy（会话外的 shell 没有部署权限与运行时上下文）。" >&2
  if [[ -n "$check_json" ]]; then printf '%s\n' "$check_json" >&2; fi
  exit 1
fi
if [[ "$(printf '%s' "$check_json" | field data.credential_valid)" != "true" ]]; then
  die "凭据无效：请在 Trae 里重新登录后重试。"
fi

# 2. 确定站点：显式 ID → 同名复用 → 新建。
site_id="$SITE_ID"
if [[ -n "$site_id" ]] && ! cli sites get --site-id "$site_id" >/dev/null 2>&1; then
  echo "站点 $site_id 不存在，改为按名称匹配。" >&2
  site_id=""
fi

if [[ -z "$site_id" ]]; then
  echo "==> 查找站点：$SITE_NAME"
  page=1
  total=1
  while (( page <= total )); do
    list="$(cli sites list --page-no "$page")" || die "读取站点列表失败。"
    total="$(printf '%s' "$list" | field data.total_pages)"
    total="${total:-1}"
    found="$(printf '%s' "$list" | node -e '
      let s = "";
      process.stdin.on("data", (d) => (s += d)).on("end", () => {
        let items = [];
        try { items = (JSON.parse(s).data || {}).items || []; } catch { process.exit(0); }
        const hit = items.find((i) => i.display_name === process.argv[1]);
        if (hit) console.log(hit.deploy_target_id);
      });
    ' "$SITE_NAME")"
    if [[ -n "$found" ]]; then site_id="$found"; break; fi
    page=$((page + 1))
  done
fi

if [[ -z "$site_id" ]]; then
  echo "==> 新建站点：$SITE_NAME（$ACCESS）"
  created="$(cli sites create --name "$SITE_NAME" --description "$SITE_DESC" --access-policy "$ACCESS")" \
    || die "创建站点失败，请查看上面的错误。"
  site_id="$(printf '%s' "$created" | field data.deploy_target_id)"
  [[ -n "$site_id" ]] || die "创建站点未返回站点 ID，请重试。"
  echo "    站点 ID：$site_id（之后可显式传入：make trae-deploy TRAE_SITE_ID=$site_id）"
else
  echo "==> 复用站点：$site_id"
fi

# 3. 不允许并发部署：终态是 9 成功 / 10 失败 / 11 取消。
active="$(cli deployments list --site-id "$site_id" | node -e '
  let s = "";
  process.stdin.on("data", (d) => (s += d)).on("end", () => {
    let items = [];
    try { items = (JSON.parse(s).data || {}).items || []; } catch { process.exit(0); }
    const terminal = new Set([9, 10, 11]);
    const running = items.find((i) => !terminal.has(i.status));
    if (running) console.log(running.deployment_id + "（状态 " + running.status + "）");
  });
')"
[[ -z "$active" ]] || die "站点上已有进行中的部署 $active，等它结束再重试。"

# 4. 准备上传目录：必须是系统临时目录下的新目录，不走项目目录。
SRC="$(mktemp -d "${TMPDIR:-/tmp}/trae-deploy.XXXXXX")"
trap 'rm -rf "$SRC"' EXIT
mkdir -p "$SRC/public"
cp -R "$DIST/." "$SRC/public/"
printf '{}' > "$SRC/execution-config.json"

commit="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
echo "==> 上传并创建部署（$commit）"
deploy_json="$(cli deployments create --site-id "$site_id" \
  --title "Virgo 官网 · $commit" \
  --description "从 web/dist（web/build.mjs 构建产物，$commit）发布 Sumery Virgo 官方落地页：语言路由页、/zh/、/en/、/privacy/。Deploy type: static-web." \
  --type static-web --source-dir "$SRC" \
  --execution-config "$SRC/execution-config.json" --app-tags "$APP_TAGS")" \
  || die "创建部署失败，请查看上面的错误。"
deployment_id="$(printf '%s' "$deploy_json" | field data.deployment_id)"
[[ -n "$deployment_id" ]] || die "创建部署未返回部署 ID。"
echo "    部署 ID：$deployment_id"

# 5. 等待终态；成功后再读出访问地址。
echo "==> 等待部署完成"
if ! wait_json="$(cli deployments wait --deployment-id "$deployment_id" --timeout-seconds 600)"; then
  printf '%s\n' "$wait_json" >&2
  die "部署失败或被取消：用 logs build / logs deploy 查看原因。"
fi
status="$(printf '%s' "$wait_json" | field data.deployment.status)"
if [[ "$status" != "9" ]]; then
  die "部署尚未结束（状态 ${status:-unknown}），可稍后在 Trae 里用 deployments wait 继续查看。"
fi

site_json="$(cli sites get --site-id "$site_id")" || die "部署成功，但读取站点信息失败。"
host="$(printf '%s' "$site_json" | field data.site_config.hostname)"
echo "==> 完成：https://$host"
echo "    站点 $site_id · 部署 $deployment_id"
