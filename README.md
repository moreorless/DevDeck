# DevDeck（开发台）

DevDeck 是一款运行在 Windows 本机的桌面工具箱，面向个人开发者的日常工作。它以托盘常驻的方式运行，用模块化的"卡片"承载功能，分为两条主线：

- **日常开发**：围绕开发过程的度量与辅助。
  - **每日用量**：按天展示 Claude Code 的 input / output / cache 用量和估算费用，支持按模型、按项目筛选
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

## 使用

- 启动后自动查找 Claude Code 日志目录（`CLAUDE_CONFIG_DIR` → `%USERPROFILE%\.claude` → `%USERPROFILE%\.config\claude`），找不到时可在「设置 → 数据源」手动添加。
- 关闭窗口默认最小化到托盘；托盘图标悬停显示今日用量，左键显示/隐藏窗口，右键菜单可立即刷新、开关开机自启、退出。
- 费用为按官方价格表计算的**估算值**，价格可在「设置 → 价格表」中自定义。
- 配置与缓存位于 `%APPDATA%\com.byzer.devdeck\`（`config.json`、`devdeck.db`）；卸载或删除该目录即清除全部数据。会话正文不入库，查看时直接读取日志。

## 开发

环境：Windows 10/11 x64、Rust stable（MSVC 工具链）、Node 20+、pnpm、WebView2。

```bash
pnpm install
pnpm tauri dev      # 开发运行
cargo test --manifest-path src-tauri/Cargo.toml   # Rust 测试
pnpm release        # 打包：NSIS 安装包 + 便携版 zip，输出到 release/
```
