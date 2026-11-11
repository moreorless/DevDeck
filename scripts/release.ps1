# 打包发布：NSIS 安装包 + 便携版 zip，产物统一放到 release/
# 用法：pnpm release
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$version = (Get-Content src-tauri/tauri.conf.json -Raw -Encoding UTF8 | ConvertFrom-Json).version
$out = Join-Path $root "release"
New-Item -ItemType Directory -Force $out | Out-Null

Write-Host "==> 构建 DevDeck $version"
pnpm tauri build
if ($LASTEXITCODE -ne 0) { throw "tauri build 失败（退出码 $LASTEXITCODE）" }

$target = Join-Path $root "src-tauri/target/release"

# 安装包
$setup = Get-ChildItem "$target/bundle/nsis/*_${version}_x64-setup.exe" | Select-Object -First 1
if (-not $setup) { throw "未找到 NSIS 安装包" }
Copy-Item $setup.FullName (Join-Path $out $setup.Name) -Force

# 便携版：单个 exe（前端资源已内嵌）+ 说明
$stage = Join-Path $env:TEMP "devdeck-portable-$version"
Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item (Join-Path $target "devdeck.exe") (Join-Path $stage "DevDeck.exe")
@"
DevDeck $version 便携版

直接运行 DevDeck.exe 即可，无需安装。

- 需要 Microsoft Edge WebView2 运行时（Windows 11 自带；Windows 10 通常已随系统更新安装）。
  若启动无反应，请安装 WebView2：https://developer.microsoft.com/microsoft-edge/webview2/
- 配置与缓存保存在 %APPDATA%\com.byzer.devdeck\，删除该目录即可清除全部数据。
- 只读取本机 Claude Code 日志，不联网。
- 未做代码签名，首次运行时 Windows SmartScreen 可能提示，选择「更多信息 → 仍要运行」。
"@ | Set-Content -Encoding utf8 (Join-Path $stage "使用说明.txt")
$zip = Join-Path $out "DevDeck_${version}_x64_portable.zip"
Remove-Item $zip -Force -ErrorAction SilentlyContinue
Compress-Archive -Path "$stage/*" -DestinationPath $zip
Remove-Item $stage -Recurse -Force

Write-Host "==> 产物："
Get-ChildItem $out | ForEach-Object { "{0,-40} {1,8:N1} MB" -f $_.Name, ($_.Length / 1MB) }
