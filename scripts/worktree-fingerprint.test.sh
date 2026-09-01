#!/usr/bin/env bash
# Tests for the worktree content fingerprint used by scripts/verify.sh.
#
# Each case builds a throwaway repository, takes a fingerprint, makes the kind
# of concurrent edit a release gate must notice, and takes another. The old
# `git status --porcelain=v1` gate is measured alongside, so the tests document
# exactly which cases it missed rather than merely asserting the new behaviour.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/worktree-fingerprint.sh
source "$ROOT/scripts/worktree-fingerprint.sh"

PASS=0
FAIL=0
# Each case runs in a subshell (so a `cd` cannot leak between cases), and a
# subshell cannot update the parent's counters. Results are therefore appended
# to a file and tallied at the end: a case that fails, or dies early, cannot be
# lost by the harness.
RESULTS="$(mktemp)"
trap 'rm -f "$RESULTS"' EXIT

ok() {
  printf 'ok\n' >>"$RESULTS"
  printf 'ok   %s\n' "$1"
}

no() {
  printf 'fail\n' >>"$RESULTS"
  printf 'FAIL %s\n     %s\n' "$1" "${2:-}" >&2
}

# Asserts the fingerprint differs across a change the gate must catch.
assert_detects() {
  local name="$1" before="$2" after="$3"
  if [[ "$before" != "$after" ]]; then
    ok "$name"
  else
    no "$name" "fingerprint did not change; a concurrent edit would slip through"
  fi
}

# Asserts the fingerprint is stable when nothing relevant changed, so the gate
# does not cry wolf and train people to ignore it.
assert_stable() {
  local name="$1" before="$2" after="$3"
  if [[ "$before" == "$after" ]]; then
    ok "$name"
  else
    no "$name" "fingerprint changed with no edit:
$(diff <(printf '%s\n' "$before") <(printf '%s\n' "$after") || true)"
  fi
}

new_repo() {
  local dir
  dir="$(mktemp -d)"
  git -C "$dir" init -q -b main
  git -C "$dir" config user.email test@example.com
  git -C "$dir" config user.name Test
  printf 'v1\n' >"$dir/tracked.txt"
  git -C "$dir" add .
  git -C "$dir" -c user.name=Test -c user.email=test@example.com commit -qm base
  printf '%s' "$dir"
}

# --- Case 1: an already-dirty tracked file changes again -----------------------
# The motivating bug. Porcelain prints " M tracked.txt" both times.
(
  repo="$(new_repo)"
  cd "$repo" || exit 1
  printf 'v2-dirty\n' >tracked.txt

  porcelain_before="$(git status --porcelain=v1)"
  before="$(worktree_fingerprint)"

  printf 'v3-concurrent\n' >tracked.txt

  porcelain_after="$(git status --porcelain=v1)"
  after="$(worktree_fingerprint)"

  assert_detects "already-dirty tracked file edited again is detected" "$before" "$after"

  if [[ "$porcelain_before" == "$porcelain_after" ]]; then
    ok "regression documented: porcelain alone misses this edit"
  else
    no "regression documented: porcelain alone misses this edit" \
      "porcelain changed, so this case no longer demonstrates the gap"
  fi
  rm -rf "$repo"
)

# --- Case 2: an already-untracked file changes again ---------------------------
# Porcelain prints "?? untracked.txt" both times.
(
  repo="$(new_repo)"
  cd "$repo" || exit 1
  printf 'u1\n' >untracked.txt

  porcelain_before="$(git status --porcelain=v1)"
  before="$(worktree_fingerprint)"

  printf 'u2-concurrent\n' >untracked.txt

  porcelain_after="$(git status --porcelain=v1)"
  after="$(worktree_fingerprint)"

  assert_detects "already-untracked file edited again is detected" "$before" "$after"

  if [[ "$porcelain_before" == "$porcelain_after" ]]; then
    ok "regression documented: porcelain alone misses untracked content edits"
  else
    no "regression documented: porcelain alone misses untracked content edits" \
      "porcelain changed, so this case no longer demonstrates the gap"
  fi
  rm -rf "$repo"
)

# --- Case 3: a file inside an already-untracked directory changes --------------
# Porcelain collapses an untracked directory to one "?? dir/" line.
(
  repo="$(new_repo)"
  cd "$repo" || exit 1
  mkdir -p nested/deep
  printf 'd1\n' >nested/deep/file.txt

  before="$(worktree_fingerprint)"
  printf 'd2-concurrent\n' >nested/deep/file.txt
  after="$(worktree_fingerprint)"

  assert_detects "edit inside an untracked directory is detected" "$before" "$after"

  before_add="$(worktree_fingerprint)"
  printf 'new\n' >nested/deep/added.txt
  after_add="$(worktree_fingerprint)"
  assert_detects "new file in an untracked directory is detected" "$before_add" "$after_add"
  rm -rf "$repo"
)

# --- Case 4: staging-only and commit-only changes ------------------------------
(
  repo="$(new_repo)"
  cd "$repo" || exit 1
  printf 'v2\n' >tracked.txt

  before="$(worktree_fingerprint)"
  git add tracked.txt
  after_stage="$(worktree_fingerprint)"
  assert_detects "staging a dirty file is detected" "$before" "$after_stage"

  git -c user.name=Test -c user.email=test@example.com commit -qm concurrent
  after_commit="$(worktree_fingerprint)"
  assert_detects "a concurrent commit is detected" "$after_stage" "$after_commit"
  rm -rf "$repo"
)

# --- Case 5: paths with spaces and quotes survive ------------------------------
# porcelain=v1 without -z quotes and escapes such paths; -z keeps them literal.
(
  repo="$(new_repo)"
  cd "$repo" || exit 1
  printf 'a\n' >"has space and 'quote'.txt"

  before="$(worktree_fingerprint)"
  printf 'b-concurrent\n' >"has space and 'quote'.txt"
  after="$(worktree_fingerprint)"

  assert_detects "edit to a path with spaces and quotes is detected" "$before" "$after"
  rm -rf "$repo"
)

# --- Case 6: deletions -------------------------------------------------------
(
  repo="$(new_repo)"
  cd "$repo" || exit 1
  before="$(worktree_fingerprint)"
  rm tracked.txt
  after="$(worktree_fingerprint)"
  assert_detects "deleting a tracked file is detected" "$before" "$after"
  rm -rf "$repo"
)

# --- Case 7: no false alarms -------------------------------------------------
# A gate that fires on an unchanged tree gets ignored, so stability matters as
# much as sensitivity. Reading files and re-running must not change the answer.
(
  repo="$(new_repo)"
  cd "$repo" || exit 1
  printf 'v2\n' >tracked.txt
  printf 'u1\n' >untracked.txt
  mkdir -p nested
  printf 'n1\n' >nested/file.txt

  before="$(worktree_fingerprint)"
  cat tracked.txt untracked.txt nested/file.txt >/dev/null
  sleep 1
  touch tracked.txt untracked.txt
  after="$(worktree_fingerprint)"

  assert_stable "unchanged content produces a stable fingerprint" "$before" "$after"
  rm -rf "$repo"
)

# --- Case 8: ignored build output must not trip the gate ----------------------
# target/ and node_modules/ churn during a build; including them would make the
# gate fire on every run and be disabled within a day.
(
  repo="$(new_repo)"
  cd "$repo" || exit 1
  printf 'target/\nnode_modules/\n' >.gitignore
  git add .gitignore
  git -c user.name=Test -c user.email=test@example.com commit -qm ignore
  mkdir -p target node_modules
  printf 'build1\n' >target/artifact

  before="$(worktree_fingerprint)"
  printf 'build2-churn\n' >target/artifact
  printf 'dep\n' >node_modules/lib.js
  after="$(worktree_fingerprint)"

  assert_stable "ignored build output does not trip the gate" "$before" "$after"
  rm -rf "$repo"
)

PASS="$(grep -c '^ok$' "$RESULTS" || true)"
FAIL="$(grep -c '^fail$' "$RESULTS" || true)"
printf '\n%d passed, %d failed\n' "$PASS" "$FAIL"

# Guard against a case dying before it reported: the expected count is fixed, so
# a silently truncated run fails loudly instead of looking green.
EXPECTED=12
if [[ $((PASS + FAIL)) -ne "$EXPECTED" ]]; then
  printf 'expected %d assertions, saw %d; a case exited early\n' \
    "$EXPECTED" "$((PASS + FAIL))" >&2
  exit 1
fi
[[ "$FAIL" -eq 0 ]]
