#!/usr/bin/env bash
# Run only after all concurrent editors stop; this is the authoritative release gate.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# Fingerprint file *content*, not just which files are dirty: a file that was
# already modified before verification and modified again during it leaves
# `git status --porcelain=v1` byte-identical, so a porcelain-only gate would
# bless a tree nobody verified. See scripts/worktree-fingerprint.test.sh.
# shellcheck source=scripts/worktree-fingerprint.sh
source "$ROOT/scripts/worktree-fingerprint.sh"

before="$(worktree_fingerprint)"
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
pnpm typecheck
pnpm --filter @todo/desktop test:identity
pnpm build
pnpm e2e
./scripts/e2e-web.sh
git diff --check

after="$(worktree_fingerprint)"
if [[ "$before" != "$after" ]]; then
  echo "verification changed the worktree; stop concurrent editors and rerun" >&2
  # The fingerprint hashes content, so this diff names the files whose content
  # moved even when the porcelain status letters stayed the same.
  diff -u <(printf '%s\n' "$before") <(printf '%s\n' "$after") || true
  exit 1
fi
