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

**当前状态**：v0.1 开发中，进度见 [docs/progress.md](docs/progress.md)，方案见 [docs/DevDeck 实现方案.md](docs/DevDeck%20实现方案.md)。

## 开发

环境：Windows 10/11 x64、Rust stable（MSVC 工具链）、Node 20+、pnpm、WebView2。

```bash
pnpm install
pnpm tauri dev      # 开发运行
pnpm tauri build    # 打包
```
