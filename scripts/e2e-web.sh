#!/usr/bin/env bash
# End-to-end check of the Web端 against a live `todo serve`.
#
# Builds the UI, creates a throwaway project, serves both from one binary, and
# runs the web specs against it. Everything is torn down on exit, including on
# failure, so a crashed run does not leave a server holding a port.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${PORT:-3021}"
TODO_BIN="$ROOT/target/debug/todo"

cleanup() {
  [[ -n "${SERVER_PID:-}" ]] && kill "$SERVER_PID" 2>/dev/null || true
  [[ -n "${WORKDIR:-}" ]] && rm -rf "$WORKDIR" || true
}
trap cleanup EXIT

echo "==> building"
cargo build -p todo-cli --locked --quiet
pnpm --filter @todo/desktop build >/dev/null

echo "==> preparing fixture project"
WORKDIR="$(mktemp -d)"
cd "$WORKDIR"
git init -q .
git config user.email e2e@example.com
git config user.name "e2e"
"$TODO_BIN" init >/dev/null
"$TODO_BIN" add "给登录接口加限流" --priority 1 >/dev/null
mkdir -p docs
printf '# Plan\n\n- [ ] 写迁移脚本\n' > docs/plan.md

echo "==> serving on :$PORT"
"$TODO_BIN" serve --port "$PORT" --web-root "$ROOT/apps/desktop/dist" >"$WORKDIR/serve.log" 2>&1 &
SERVER_PID=$!

for _ in $(seq 1 40); do
  if curl -fsS "http://127.0.0.1:$PORT/api/v1/health" >/dev/null 2>&1; then break; fi
  sleep 0.25
done
curl -fsS "http://127.0.0.1:$PORT/api/v1/health" >/dev/null || {
  echo "server failed to start:"; cat "$WORKDIR/serve.log"; exit 1;
}

echo "==> running web specs"
cd "$ROOT/apps/desktop"
E2E_WEB_URL="http://127.0.0.1:$PORT" \
  "$ROOT/node_modules/.bin/playwright" test e2e/web.spec.ts --reporter=list
