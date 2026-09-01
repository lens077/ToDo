#!/usr/bin/env bash
# Content fingerprint of the working tree, for detecting concurrent edits.
#
# `git status --porcelain=v1` names which files are dirty but says nothing about
# what is *in* them. A file that was already modified before verification and is
# modified again during it produces byte-identical porcelain output, so a gate
# built on porcelain alone silently blesses a tree nobody verified. The same
# blindness applies to untracked files: rewriting an already-untracked file
# leaves the `??` line unchanged.
#
# This fingerprint therefore hashes file *content*, so any concurrent edit to a
# tracked or untracked file changes the output.
#
# Usage: worktree_fingerprint  (prints "<mode> <hash> <path>" lines, sorted)
set -euo pipefail

# Prefer a real hasher; fall back across the common names so the gate works on
# both macOS and Linux CI images.
_hash_stdin() {
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 | cut -d' ' -f1
  elif command -v sha256sum >/dev/null 2>&1; then
    sha256sum | cut -d' ' -f1
  else
    echo "no sha256 implementation (shasum/sha256sum) available" >&2
    return 1
  fi
}

worktree_fingerprint() {
  # HEAD pins the commit, so a concurrent commit is caught even when it leaves
  # the tree clean.
  printf 'head %s\n' "$(git rev-parse HEAD 2>/dev/null || echo none)"
  # The index digest catches staging-only changes, which alter neither file
  # content nor the untracked set.
  printf 'index %s\n' "$(git write-tree 2>/dev/null || echo none)"

  # Every file git considers dirty or untracked, NUL-delimited so paths with
  # spaces, quotes or newlines survive. -z gives unquoted, literal paths.
  #
  # Deliberately includes ignored-file exclusion (no --ignored): build output
  # such as target/ churns constantly and would make the gate useless.
  local status_line path staged unstaged
  while IFS= read -r -d '' status_line; do
    staged="${status_line:0:1}"
    unstaged="${status_line:1:1}"
    path="${status_line:3}"

    # Renames and copies encode "new\0old"; the old path follows as its own
    # NUL-delimited record, which we consume and fold into the fingerprint.
    if [[ "$staged" == "R" || "$staged" == "C" ]]; then
      local origin=""
      IFS= read -r -d '' origin || true
      printf 'rename %s -> %s\n' "$origin" "$path"
    fi

    if [[ -f "$path" ]]; then
      printf '%s%s %s %s\n' "$staged" "$unstaged" "$(_hash_stdin <"$path")" "$path"
    elif [[ -L "$path" ]]; then
      # A symlink's target is its content; hashing the link avoids following it.
      printf '%s%s link:%s %s\n' "$staged" "$unstaged" "$(readlink "$path")" "$path"
    elif [[ -d "$path" ]]; then
      # An untracked directory: hash the files beneath it, since porcelain
      # collapses the whole tree into a single trailing-slash entry.
      local file
      while IFS= read -r -d '' file; do
        printf '%s%s %s %s\n' "$staged" "$unstaged" "$(_hash_stdin <"$file")" "$file"
      done < <(find "$path" -type f -print0 | LC_ALL=C sort -z)
    else
      # Deleted, or a special file we cannot read: the status letters alone
      # still distinguish it from a present file.
      printf '%s%s absent %s\n' "$staged" "$unstaged" "$path"
    fi
  done < <(git status --porcelain=v1 -z)
}

# Allow both `source`ing the function and running the script directly.
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  worktree_fingerprint
fi
