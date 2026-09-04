#!/usr/bin/env bash
# End-to-end check of the Web端 against a live `todo serve`.
#
# Builds the UI, creates a throwaway project, serves both from one binary, and
# runs the web specs against it. Everything is torn down on exit, including on
# failure, so a crashed run does not leave a server holding a port.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ -n "${PORT:-}" ]]; then
  PORT="$PORT"
else
  PORT="$(python3 - <<'PYPORT'
import socket
with socket.socket() as sock:
    sock.bind(('127.0.0.1', 0))
    print(sock.getsockname()[1])
PYPORT
)"
fi
TODO_BIN="$ROOT/target/debug/todo"
RESTART_SIGNAL=""
RESTART_READY=""

cleanup() {
  if [[ -n "${SUPERVISOR_PID:-}" ]]; then
    kill "$SUPERVISOR_PID" 2>/dev/null || true
    wait "$SUPERVISOR_PID" 2>/dev/null || true
  fi
  if [[ -n "${WORKDIR:-}" && -f "$WORKDIR/server.pid" ]]; then
    kill "$(cat "$WORKDIR/server.pid")" 2>/dev/null || true
  fi
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
"$TODO_BIN" policy strict >/dev/null
git add TODO.md
git commit -qm "initialize project"
"$TODO_BIN" add "给登录接口加限流" --priority 1 >/dev/null
# Seed a real parent subtree plus a low-priority task for quadrant persistence.
python3 - <<'PY'
from pathlib import Path
path = Path('TODO.md')
text = path.read_text()
needle = '## Todo\n'
fixture = '''## Todo
- [ ] 发布桌面版 <!--t:parent1--> !2 #release
    - [ ] 构建 macOS 安装包 <!--t:child01--> !3 #release
    - [ ] 编写安装说明 <!--t:child02--> !4 #docs
- [ ] 清理发布记录 <!--t:low001--> !4 #release
'''
path.write_text(text.replace(needle, fixture, 1))
PY
git add TODO.md
git commit -qm "seed project task"
# The desktop history scanner follows TODO.md specifically; advance that history
# while preserving the task's introduction commit as the evidence start.
printf '\n' >> TODO.md
git add TODO.md
git commit -qm "advance task ledger"
# Evidence starts at the commit that introduced the task and ends at the current
# HEAD, so commit an unrelated implementation file without rewriting TODO.md.
printf 'export const rateLimit = true;\n' > rate-limit.ts
git add rate-limit.ts
git commit -qm "implement rate limiting"
mkdir -p docs
printf '# Plan\n\n- [ ] 写迁移脚本\n' > docs/plan.md
git add docs/plan.md
git commit -qm "add implementation plan"
# Start from a clean tree so the evidence range the browser sees is exactly
# introduction..HEAD; the CLI leaves a lock file behind that git would report.

rm -f .TODO.md.lock
git status --porcelain | grep -q . && { git status --short; exit 1; }

echo "==> serving on :$PORT"
RESTART_SIGNAL="$WORKDIR/restart.signal"
RESTART_READY="$WORKDIR/restart.ready"
(
  while true; do
    rm -f "$RESTART_SIGNAL" "$RESTART_READY"
    "$TODO_BIN" serve --port "$PORT" --web-root "$ROOT/apps/desktop/dist" >>"$WORKDIR/serve.log" 2>&1 &
    child=$!
    trap 'kill "$child" 2>/dev/null || true; wait "$child" 2>/dev/null || true; exit 0' TERM INT
    printf '%s\n' "$child" > "$WORKDIR/server.pid"
    touch "$RESTART_READY"
    while kill -0 "$child" 2>/dev/null; do
      if [[ -f "$RESTART_SIGNAL" ]]; then
        kill "$child" 2>/dev/null || true
        wait "$child" 2>/dev/null || true
        break
      fi
      sleep 0.1
    done
    [[ -f "$RESTART_SIGNAL" ]] || exit 0
  done
) &
SUPERVISOR_PID=$!

for _ in $(seq 1 80); do
  if curl -fsS "http://127.0.0.1:$PORT/api/v1/health" >/dev/null 2>&1; then break; fi
  sleep 0.25
done
curl -fsS "http://127.0.0.1:$PORT/api/v1/health" >/dev/null || {
  echo "server failed to start:"; cat "$WORKDIR/serve.log"; exit 1;
}

echo "==> running web specs"
cd "$ROOT/apps/desktop"
E2E_WEB_URL="http://127.0.0.1:$PORT" \
E2E_TODO_PATH="$WORKDIR/TODO.md" \
E2E_RESTART_SIGNAL="$RESTART_SIGNAL" \
E2E_RESTART_READY="$RESTART_READY" \
  "$ROOT/node_modules/.bin/playwright" test e2e/web.spec.ts --reporter=list
