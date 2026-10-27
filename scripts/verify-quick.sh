#!/usr/bin/env bash
# verify-quick.sh — push 前的最便宜验证，目标 2 分钟内。
#
#   scripts/verify-quick.sh            # 按 RUN_RUST / RUN_WEB（默认都跑）
#   scripts/verify-quick.sh rust       # 只跑 Rust
#   scripts/verify-quick.sh web        # 只跑前端
#
# Rust 与前端并行；绿只打一行摘要，红只打失败日志尾部。
# 完整日志落在临时目录，摘要里给出路径。
set -uo pipefail

root=$(git rev-parse --show-toplevel) || { echo "verify-quick: 不在 git 仓库内" >&2; exit 2; }
cd "$root" || exit 2

want=${1:-all}
run_rust=${RUN_RUST:-1}
run_web=${RUN_WEB:-1}
case "$want" in
  rust) run_web=0 ;;
  web) run_rust=0 ;;
  all) ;;
  *) echo "用法: $0 [rust|web]" >&2; exit 2 ;;
esac

tail_lines=${VERIFY_QUICK_TAIL:-40}
logdir=$(mktemp -d "${TMPDIR:-/tmp}/verify-quick.XXXXXX")
start=$SECONDS

run_rust() {
  cargo fmt --all -- --check &&
  cargo clippy --workspace --all-targets --locked -- -D warnings &&
  cargo test --workspace --locked
}

run_web() {
  pnpm build &&
  pnpm --dir web run build
}

rust_pid=""
web_pid=""
if [ "$run_rust" = 1 ]; then
  run_rust >"$logdir/rust.log" 2>&1 &
  rust_pid=$!
fi
if [ "$run_web" = 1 ]; then
  run_web >"$logdir/web.log" 2>&1 &
  web_pid=$!
fi

report() { # report <名字> <rc> <log>
  local name=$1 rc=$2 log=$3
  if [ "$rc" = 0 ]; then
    echo "✅ $name 绿"
  else
    echo "❌ $name 红（rc=$rc）——日志尾部 ${tail_lines} 行（完整：$log）："
    tail -n "$tail_lines" "$log" | sed 's/^/  | /'
  fi
}

overall=0
if [ -n "$rust_pid" ]; then
  rc=0; wait "$rust_pid" || rc=$?
  report "rust(fmt+clippy+test)" "$rc" "$logdir/rust.log"
  [ "$rc" = 0 ] || overall=1
fi
if [ -n "$web_pid" ]; then
  rc=0; wait "$web_pid" || rc=$?
  report "web(desktop build + static site)" "$rc" "$logdir/web.log"
  [ "$rc" = 0 ] || overall=1
fi

check_base=$(git merge-base HEAD origin/main 2>/dev/null || true)
if [ -n "$check_base" ]; then
  git diff --check "$check_base" HEAD || overall=1
else
  git diff --check || overall=1
fi

if [ "$overall" = 0 ]; then
  echo "verify-quick: 绿（$((SECONDS - start))s）"
else
  echo "verify-quick: 红（$((SECONDS - start))s）"
fi
exit "$overall"
