#!/usr/bin/env bash
# macOS 打包：生成 .dmg 与 .app 压缩包，输出到 release/
# 用法：pnpm release:mac             # 本机架构
#       pnpm release:mac --universal # Apple 芯片 + Intel 通用包（需先 rustup target add aarch64-apple-darwin x86_64-apple-darwin）
set -euo pipefail
cd "$(dirname "$0")/.."

if [[ "$(uname)" != "Darwin" ]]; then
  echo "只能在 macOS 上打包 Mac 应用" >&2
  exit 1
fi

version=$(node -p "require('./src-tauri/tauri.conf.json').version")
target_args=()
bundle_dir="src-tauri/target/release/bundle"
arch=$(uname -m)
if [[ "${1:-}" == "--universal" ]]; then
  target_args=(--target universal-apple-darwin)
  bundle_dir="src-tauri/target/universal-apple-darwin/release/bundle"
  arch="universal"
fi

echo "==> 构建 DevDeck ${version}（${arch}）"
# bash 3.2（macOS 自带）在 set -u 下展开空数组会报错，用 ${arr[@]+...} 写法
pnpm tauri build ${target_args[@]+"${target_args[@]}"}

mkdir -p release
dmg=$(ls "${bundle_dir}"/dmg/*.dmg | head -n 1)
cp "$dmg" "release/DevDeck_${version}_${arch}.dmg"
# .app 打成 zip（ditto 保留扩展属性与符号链接）
ditto -c -k --keepParent "${bundle_dir}/macos/DevDeck.app" "release/DevDeck_${version}_${arch}.app.zip"

echo "==> 完成"
ls -lh release/DevDeck_"${version}"_"${arch}".*
echo
echo "未签名：首次打开若提示「无法验证开发者」，右键 → 打开，或执行 xattr -cr /Applications/DevDeck.app"
