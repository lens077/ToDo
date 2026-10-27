#!/usr/bin/env bash
# 构建单个桌面端目标。Tauri 的安装包必须在对应宿主上构建：
#   mac-*   → macOS
#   win-*   → Windows
#   linux-* → Linux
# 在错误宿主上会直接报错，不会假装成功。
#
# 用法：scripts/build-target.sh <mac-arm64|mac-x64|win-x64|win-arm64|linux-x64>
# 预检：scripts/build-target.sh <target> --check
set -euo pipefail

target=${1:-}
check_only=${2:-}
[ -n "$target" ] || { echo "用法: $0 <mac-arm64|mac-x64|win-x64|win-arm64|linux-x64> [--check]" >&2; exit 2; }

case "$target" in
  mac-arm64)  want=Darwin; triple=aarch64-apple-darwin;        bundles=app ;;
  mac-x64)    want=Darwin; triple=x86_64-apple-darwin;         bundles=app ;;
  win-x64)    want=Windows; triple=x86_64-pc-windows-msvc;     bundles=nsis ;;
  win-arm64)  want=Windows; triple=aarch64-pc-windows-msvc;    bundles=nsis ;;
  linux-x64)  want=Linux;  triple=x86_64-unknown-linux-gnu;    bundles=appimage,deb ;;
  *) echo "未知目标: $target" >&2; exit 2 ;;
esac

host=$(uname -s)
case "$host" in
  Darwin) host_kind=Darwin ;;
  Linux) host_kind=Linux ;;
  MINGW*|MSYS*|CYGWIN*|Windows_NT) host_kind=Windows ;;
  *) host_kind=$host ;;
esac

if [ "$host_kind" != "$want" ]; then
  echo "error: $target 需要在 $want 上构建，当前是 $host_kind" >&2
  echo "提示：Windows / Linux 安装包请在对应机器或发布 runner 上跑；macOS 只能在 macOS 上打包。" >&2
  exit 1
fi

if command -v rustup >/dev/null 2>&1; then
  if ! rustup target list --installed | grep -qx "$triple"; then
    echo "error: Rust target 未安装：$triple" >&2
    echo "运行：rustup target add $triple" >&2
    exit 1
  fi
fi

if [ "$check_only" = "--check" ]; then
  echo "OK: $target 可在 $host_kind 上构建（$triple, bundles=$bundles）"
  exit 0
fi

echo "构建 $target → $triple（bundles=$bundles）"
pnpm --filter @todo/desktop tauri build --target "$triple" --bundles "$bundles" --ci
echo "产物在 target/$triple/release/bundle/"
