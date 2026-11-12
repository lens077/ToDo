#!/usr/bin/env bash
# 构建 Android 包。构建矩阵只保留 arm64-v8a 真机（见 AGENTS.md）：
# 包装脚本强制 `--target aarch64`，其余参数（--debug / --apk / --aab 等）原样
# 透传给 `tauri android build`。
#
# 用法：scripts/build-android.sh [--target aarch64] [tauri android build 参数...]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

target=aarch64
forward=()

while [ $# -gt 0 ]; do
  case "$1" in
    --target|-t)
      if [ $# -lt 2 ]; then
        echo "error: --target 需要一个值；只支持 aarch64（arm64-v8a 真机）" >&2
        exit 2
      fi
      target=$2
      shift 2
      ;;
    --target=*)
      target=${1#*=}
      shift
      ;;
    -h|--help)
      sed -n '2,6p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      forward+=("$1")
      shift
      ;;
  esac
done

if [ "$target" != "aarch64" ]; then
  echo "error: 只支持 arm64-v8a 真机（--target aarch64），收到 --target $target" >&2
  echo "原因：构建矩阵只为真机发布一个 ABI；armv7 / i686 / x86_64 不在发布范围，" >&2
  echo "多 ABI 会让 APK 体积与真机验证成本翻倍。" >&2
  echo "如需模拟器调试，请直接用 pnpm --filter @todo/desktop tauri android build，不要走发布脚本。" >&2
  exit 2
fi

if command -v rustup >/dev/null 2>&1; then
  if ! rustup target list --installed | grep -qx "aarch64-linux-android"; then
    echo "error: Rust target 未安装：aarch64-linux-android" >&2
    echo "运行：rustup target add aarch64-linux-android" >&2
    exit 1
  fi
fi

cd "$ROOT"
echo "构建 Android arm64-v8a（aarch64）"
cmd=(pnpm --filter @todo/desktop tauri android build --target aarch64)
if [ ${#forward[@]} -gt 0 ]; then
  cmd+=("${forward[@]}")
fi
"${cmd[@]}"
