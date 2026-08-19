#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo '用法：package-macos-dmg.sh <应用.app> <输出.dmg>' >&2
  exit 2
fi

app_path=$1
output_path=$2
if [[ ! -d "$app_path" || "$app_path" != *.app ]]; then
  echo "macOS应用包无效：$app_path" >&2
  exit 2
fi
if [[ "$output_path" != *.dmg ]]; then
  echo "DMG输出路径必须以.dmg结尾：$output_path" >&2
  exit 2
fi

output_directory=$(dirname "$output_path")
mkdir -p "$output_directory"
temporary_root=$(mktemp -d "${TMPDIR:-/tmp}/dsh-desktop-dmg.XXXXXX")
cleanup() {
  rm -rf "$temporary_root"
}
trap cleanup EXIT

staging_directory="$temporary_root/DSH Desktop"
mkdir -p "$staging_directory"
ditto "$app_path" "$staging_directory/$(basename "$app_path")"
ln -s /Applications "$staging_directory/Applications"

# 单步创建只读压缩镜像，避开托管Intel Runner不稳定的临时镜像卸载流程。
hdiutil create \
  -volname 'DSH Desktop' \
  -srcfolder "$staging_directory" \
  -ov \
  -format UDZO \
  "$output_path"

test -s "$output_path"
echo "macOS DMG已生成：$output_path"
