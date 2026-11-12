# DevDeck（开发台）

DevDeck 是一款运行在本机（Windows / macOS）的桌面工具箱，面向个人开发者的日常工作。它以托盘常驻的方式运行，用模块化的"卡片"承载功能，分为两条主线：

- **日常开发**：围绕开发过程的度量与辅助。
  - **每日用量**：按天展示 Claude Code 的 input / output / cache 用量和估算费用，支持按模型、按项目筛选；显示套餐额度（当前会话、本周、各模型的已用百分比与重置时间）
  - **会话浏览**：查看 Claude Code 历史会话与会话内的对话记录
- **个人任务**：围绕个人工作的规划与跟踪（规划中）。

**特点**

- 本地优先：无需账号，不联网，只读取本机 Claude Code 日志
- 轻量常驻：基于 Tauri 2 + Vue 3，安装包小、内存占用低
- 模块化：每项功能是一个独立模块，按需扩展

**当前状态**：v0.1，进度见 [docs/progress.md](docs/progress.md)，方案见 [docs/DevDeck 实现方案.md](docs/DevDeck%20实现方案.md)。

## 安装

从发布产物中任选其一：

- **安装包** `DevDeck_<版本>_x64-setup.exe`：按当前用户安装，不需要管理员权限；缺少 WebView2 时会自动安装（需联网）。
- **便携版** `DevDeck_<版本>_x64_portable.zip`：解压后直接运行 `DevDeck.exe`。

系统要求：Windows 10/11 x64，Microsoft Edge WebView2 运行时（Windows 11 自带，Windows 10 一般已随系统更新安装）。

> 安装包和程序未做代码签名，首次运行时 Windows SmartScreen 可能提示「已保护你的电脑」，点击「更多信息 → 仍要运行」即可。

**macOS**（代码已适配，尚未在 Mac 上实测）：`DevDeck_<版本>_<架构>.dmg` 或 `.app.zip`，拖入「应用程序」即可。要求 macOS 11 及以上。
应用未签名，首次打开若提示「无法验证开发者」，在访达中右键 → 打开，或执行 `xattr -cr /Applications/DevDeck.app`。

## 使用

- 启动后自动查找 Claude Code 日志目录（`CLAUDE_CONFIG_DIR` → `~/.claude` → `~/.config/claude`；Windows 上 `~` 即 `%USERPROFILE%`），找不到时可在「设置 → 数据源」手动添加。
- 关闭窗口默认最小化到托盘；托盘图标悬停显示今日用量，菜单可立即刷新、开关开机自启、退出。Windows 上左键显示/隐藏窗口、右键打开菜单；macOS 上点击菜单栏图标打开菜单，点击程序坞图标恢复窗口。
- 费用为按官方价格表计算的**估算值**，价格可在「设置 → 价格表」中自定义。
- 配置、缓存与 Insights 报告归档位于 `%APPDATA%\com.byzer.devdeck\`（macOS：`~/Library/Application Support/com.byzer.devdeck/`），包括 `config.json`、`devdeck.db`、`insights/`；删除该目录即清除全部数据。会话正文不入库，查看时直接读取日志。

## 开发

环境：Rust stable、Node 20+、pnpm。Windows 需 MSVC 工具链与 WebView2；macOS 需 Xcode Command Line Tools。Mac 应用只能在 macOS 上构建（Tauri 不支持从 Windows 交叉编译）。

```bash
pnpm install
pnpm tauri dev      # 开发运行
cargo test --manifest-path src-tauri/Cargo.toml   # Rust 测试
pnpm release        # Windows 打包：NSIS 安装包 + 便携版 zip，输出到 release/
pnpm release:mac    # macOS 打包：.dmg + .app.zip（加 --universal 生成 Apple 芯片 + Intel 通用包）
```

**云端构建**：GitHub Actions（`.github/workflows/build.yml`）在 Windows 与 macOS 机器上分别测试并打包。在 Actions 页面手动运行即可下载产物；推送版本标签（如 `git tag v0.2.0 && git push origin v0.2.0`）会在两个平台都成功后创建 Release 草稿，附上全部安装包。
