#!/usr/bin/env bash
# Exercise column setup against a private copy. Never serves or writes the source checkout.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE="${1:-$ROOT/crates/todo-core/tests/fixtures/ecommerce-tasks.txt}"
SOURCE="$(python3 - "$SOURCE" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1]).resolve(strict=True)
if not p.is_file():
    raise SystemExit('source must be a regular Markdown file')
print(p)
PY
)"
ORIGINAL_HASH="$(shasum -a 256 "$SOURCE")"
WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/virgo-columns.XXXXXX")"
WORKDIR="$(python3 - "$WORKDIR" <<'PY'
from pathlib import Path
import sys
print(Path(sys.argv[1]).resolve(strict=True))
PY
)"
SERVER_PID=""
cleanup() {
  local code=$?
  if [[ -n "$SERVER_PID" ]]; then
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
  if [[ "$(shasum -a 256 "$SOURCE")" != "$ORIGINAL_HASH" ]]; then
    echo 'Source file changed during verification; do not claim it remained unchanged.' >&2
    code=1
  fi
  rm -rf "$WORKDIR"
  exit "$code"
}
trap cleanup EXIT
cp "$SOURCE" "$WORKDIR/TODO.md"

cd "$ROOT"
echo '==> build current CLI and frontend'
cargo build -p todo-cli --locked --quiet
pnpm --filter @todo/desktop build
PORT="$(python3 - <<'PY'
import socket
with socket.socket() as sock:
    sock.bind(('127.0.0.1', 0))
    print(sock.getsockname()[1])
PY
)"
(
  cd "$WORKDIR"
  exec env TODO_AI_STORE="$WORKDIR/ai.json" "$ROOT/target/debug/todo" serve \
    --port "$PORT" --web-root "$ROOT/apps/desktop/dist"
) >"$WORKDIR/server.log" 2>&1 &
SERVER_PID=$!
URL="http://127.0.0.1:$PORT"
python3 - "$URL" "$WORKDIR" <<'PY'
import json, sys, time, urllib.request
url, root = sys.argv[1:]
for _ in range(100):
    try:
        with urllib.request.urlopen(url + '/api/v1/health', timeout=1) as response:
            health = json.load(response)
        if health.get('project') != root:
            raise SystemExit('Refusing a server bound to a different project')
        break
    except (OSError, ValueError):
        time.sleep(0.1)
else:
    raise SystemExit('Isolated column-test server did not become ready')
PY

echo '==> verify UI against isolated real task file'
cd "$ROOT/apps/desktop"
BROWSER="${E2E_COLUMNS_BROWSER:-chromium}"
case "$BROWSER" in
  chromium) CONFIG=playwright.config.ts ;;
  webkit) CONFIG=playwright.webkit.config.ts ;;
  *) echo 'E2E_COLUMNS_BROWSER must be chromium or webkit' >&2; exit 1 ;;
esac
E2E_COLUMNS_URL="$URL" \
E2E_COLUMNS_PATH="$WORKDIR/TODO.md" \
PLAYWRIGHT_PORT="${PLAYWRIGHT_PORT:-4296}" \
  "$ROOT/node_modules/.bin/playwright" test e2e/column-setup-live.spec.ts --config "$CONFIG" --project "$BROWSER" --workers=1 --output=test-results-columns-live
printf 'Verified isolated copy; original source hash is unchanged.\n'
