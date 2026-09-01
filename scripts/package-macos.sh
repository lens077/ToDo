#!/usr/bin/env bash
# Build reproducible macOS CLI and application archives from one checkout.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTPUT_DIR="${OUTPUT_DIR:-$ROOT/target/release-artifacts}"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: macOS release artifacts must be built on macOS" >&2
  exit 1
fi

if [[ "$OUTPUT_DIR" != /* ]]; then
  OUTPUT_DIR="$ROOT/$OUTPUT_DIR"
fi

cd "$ROOT"
VERSION="$(node -e 'const fs = require("fs"); const config = JSON.parse(fs.readFileSync("apps/desktop/src-tauri/tauri.conf.json", "utf8")); process.stdout.write(config.version);')"
HOST_TRIPLE="$(rustc -vV | awk '/^host:/ { print $2; exit }')"
CLI_BIN="$ROOT/target/release/todo"
APP_BUNDLE="$ROOT/target/release/bundle/macos/Todo.app"
CLI_ARCHIVE="todo-${VERSION}-${HOST_TRIPLE}.tar.gz"
APP_ARCHIVE="Todo-${VERSION}-${HOST_TRIPLE}.app.zip"
STAGING_DIR="$(mktemp -d "${TMPDIR:-/tmp}/todo-release.XXXXXX")"

cleanup() {
  rm -rf "$STAGING_DIR"
}
trap cleanup EXIT

mkdir -p "$OUTPUT_DIR"
rm -f \
  "$OUTPUT_DIR/$CLI_ARCHIVE" \
  "$OUTPUT_DIR/$APP_ARCHIVE" \
  "$OUTPUT_DIR/SHA256SUMS"

echo "==> building todo CLI $VERSION for $HOST_TRIPLE"
cargo build --locked --release -p todo-cli

echo "==> building Todo.app"
pnpm --filter @todo/desktop tauri build --bundles app

if [[ ! -x "$CLI_BIN" ]]; then
  echo "error: expected CLI binary at $CLI_BIN" >&2
  exit 1
fi
if [[ ! -d "$APP_BUNDLE" ]]; then
  echo "error: expected application bundle at $APP_BUNDLE" >&2
  exit 1
fi

install -m 0755 "$CLI_BIN" "$STAGING_DIR/todo"
tar -C "$STAGING_DIR" -czf "$OUTPUT_DIR/$CLI_ARCHIVE" todo
ditto -c -k --sequesterRsrc --keepParent "$APP_BUNDLE" "$OUTPUT_DIR/$APP_ARCHIVE"

cd "$OUTPUT_DIR"
shasum -a 256 "$CLI_ARCHIVE" "$APP_ARCHIVE" > SHA256SUMS

printf 'Created release artifacts:\n  %s\n  %s\n  %s\n' \
  "$OUTPUT_DIR/$CLI_ARCHIVE" \
  "$OUTPUT_DIR/$APP_ARCHIVE" \
  "$OUTPUT_DIR/SHA256SUMS"
