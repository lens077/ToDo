#!/usr/bin/env bash
# Build macOS CLI and application archives from one checkout.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTPUT_DIR="${OUTPUT_DIR:-$ROOT/target/release-artifacts}"
SIGN_AND_NOTARIZE="${SIGN_AND_NOTARIZE:-0}"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: macOS release artifacts must be built on macOS" >&2
  exit 1
fi
if [[ "$SIGN_AND_NOTARIZE" != "0" && "$SIGN_AND_NOTARIZE" != "1" ]]; then
  echo "error: SIGN_AND_NOTARIZE must be 0 or 1" >&2
  exit 1
fi
if [[ "$OUTPUT_DIR" != /* ]]; then
  OUTPUT_DIR="$ROOT/$OUTPUT_DIR"
fi

require_env() {
  local name="$1"
  if [[ -z "${!name:-}" ]]; then
    echo "error: $name is required for a signed release" >&2
    exit 1
  fi
}

verify_developer_id_signature() {
  local path="$1"
  local label="$2"
  local details

  codesign --verify --deep --strict --verbose=2 "$path"
  details="$(codesign --display --verbose=4 "$path" 2>&1)"
  if ! grep -Fq 'Authority=Developer ID Application:' <<<"$details"; then
    echo "error: $label is not signed with a Developer ID Application certificate" >&2
    exit 1
  fi
  if ! grep -Fq "TeamIdentifier=$APPLE_TEAM_ID" <<<"$details"; then
    echo "error: $label signature does not use team $APPLE_TEAM_ID" >&2
    exit 1
  fi
}

submit_for_notarization() {
  local archive="$1"
  local label="$2"
  local result_file="$3"
  local status

  xcrun notarytool submit "$archive" \
    --key "$NOTARY_KEY_PATH" \
    --key-id "$APPLE_API_KEY_ID" \
    --issuer "$APPLE_API_ISSUER" \
    --wait \
    --output-format json > "$result_file"

  status="$(node -e 'const fs = require("fs"); const result = JSON.parse(fs.readFileSync(process.argv[1], "utf8")); process.stdout.write(result.status || "");' "$result_file")"
  if [[ "$status" != "Accepted" ]]; then
    echo "error: Apple notarization did not accept $label (status: ${status:-missing})" >&2
    cat "$result_file" >&2
    exit 1
  fi
}

cd "$ROOT"
VERSION="$(node -e 'const fs = require("fs"); const config = JSON.parse(fs.readFileSync("apps/desktop/src-tauri/tauri.conf.json", "utf8")); process.stdout.write(config.version);')"
HOST_TRIPLE="$(rustc -vV | awk '/^host:/ { host = $2 } END { print host }')"
CLI_BIN="$ROOT/target/release/todo"
# The bundle is named after tauri.conf.json productName ("Sumery Virgo").
APP_BUNDLE="$ROOT/target/release/bundle/macos/Sumery Virgo.app"
ARTIFACT_QUALIFIER=""
if [[ "$SIGN_AND_NOTARIZE" == "0" ]]; then
  ARTIFACT_QUALIFIER="-local-unnotarized"
fi
CLI_ARCHIVE="todo-${VERSION}-${HOST_TRIPLE}${ARTIFACT_QUALIFIER}.tar.gz"
APP_ARCHIVE="Sumery-Virgo-${VERSION}-${HOST_TRIPLE}${ARTIFACT_QUALIFIER}.app.zip"
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

if [[ "$SIGN_AND_NOTARIZE" == "1" ]]; then
  for name in \
    APPLE_SIGNING_IDENTITY \
    APPLE_TEAM_ID \
    APPLE_API_KEY_ID \
    APPLE_API_ISSUER \
    APPLE_API_PRIVATE_KEY
  do
    require_env "$name"
  done

  NOTARY_KEY_PATH="$STAGING_DIR/AuthKey_${APPLE_API_KEY_ID}.p8"
  install -m 0600 /dev/null "$NOTARY_KEY_PATH"
  if ! printf '%s' "$APPLE_API_PRIVATE_KEY" | openssl base64 -d -A > "$NOTARY_KEY_PATH"; then
    echo "error: APPLE_API_PRIVATE_KEY is not valid Base64" >&2
    exit 1
  fi
  if ! grep -Fq -- '-----BEGIN PRIVATE KEY-----' "$NOTARY_KEY_PATH"; then
    echo "error: APPLE_API_PRIVATE_KEY does not decode to an App Store Connect .p8 key" >&2
    exit 1
  fi

  identities="$(security find-identity -v -p codesigning)"
  if ! grep -Fq "$APPLE_SIGNING_IDENTITY" <<<"$identities"; then
    echo "error: APPLE_SIGNING_IDENTITY is not available in the build keychain" >&2
    exit 1
  fi
fi

echo "==> building todo CLI $VERSION for $HOST_TRIPLE"
cargo build --locked --release -p todo-cli

if [[ "$SIGN_AND_NOTARIZE" == "1" ]]; then
  echo "==> signing todo CLI with Developer ID"
  codesign --force --options runtime --timestamp \
    --sign "$APPLE_SIGNING_IDENTITY" "$CLI_BIN"

  echo "==> building and signing Sumery Virgo.app"
  env \
    -u APPLE_ID \
    -u APPLE_PASSWORD \
    -u APPLE_TEAM_ID \
    -u APPLE_API_ISSUER \
    -u APPLE_API_KEY \
    -u APPLE_API_KEY_PATH \
    pnpm --filter @todo/desktop tauri build --bundles app
else
  echo "==> building Sumery Virgo.app for local validation"
  env \
    -u APPLE_SIGNING_IDENTITY \
    -u APPLE_ID \
    -u APPLE_PASSWORD \
    -u APPLE_TEAM_ID \
    -u APPLE_API_ISSUER \
    -u APPLE_API_KEY \
    -u APPLE_API_KEY_PATH \
    pnpm --filter @todo/desktop tauri build --bundles app
fi

if [[ ! -x "$CLI_BIN" ]]; then
  echo "error: expected CLI binary at $CLI_BIN" >&2
  exit 1
fi
if [[ ! -d "$APP_BUNDLE" ]]; then
  echo "error: expected application bundle at $APP_BUNDLE" >&2
  exit 1
fi

if [[ "$SIGN_AND_NOTARIZE" == "1" ]]; then
  echo "==> verifying Developer ID signatures"
  verify_developer_id_signature "$CLI_BIN" "todo CLI"
  verify_developer_id_signature "$APP_BUNDLE" "Sumery Virgo.app"

  echo "==> submitting Sumery Virgo.app for notarization"
  APP_NOTARY_ARCHIVE="$STAGING_DIR/Virgo-notarization.zip"
  APP_NOTARY_RESULT="$STAGING_DIR/Virgo-notarization.json"
  ditto -c -k --sequesterRsrc --keepParent "$APP_BUNDLE" "$APP_NOTARY_ARCHIVE"
  submit_for_notarization "$APP_NOTARY_ARCHIVE" "Sumery Virgo.app" "$APP_NOTARY_RESULT"
  xcrun stapler staple "$APP_BUNDLE"
  xcrun stapler validate "$APP_BUNDLE"
  spctl --assess --type execute --verbose=2 "$APP_BUNDLE"

  echo "==> submitting todo CLI for notarization"
  CLI_NOTARY_ARCHIVE="$STAGING_DIR/todo-notarization.zip"
  CLI_NOTARY_RESULT="$STAGING_DIR/todo-notarization.json"
  ditto -c -k --keepParent "$CLI_BIN" "$CLI_NOTARY_ARCHIVE"
  submit_for_notarization "$CLI_NOTARY_ARCHIVE" "todo CLI" "$CLI_NOTARY_RESULT"
  spctl --assess --type execute --verbose=2 "$CLI_BIN"
else
  echo "==> applying an ad-hoc signature for local bundle validation"
  codesign --force --deep --sign - "$APP_BUNDLE"
  codesign --verify --deep --strict --verbose=2 "$APP_BUNDLE"
  echo "warning: local archives are not Developer ID signed or notarized; do not distribute them" >&2
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
