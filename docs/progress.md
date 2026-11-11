# 进度记录

## M1 工程骨架（已完成，2026-11-10）

**已完成**

- 用官方 `create-tauri-app`（vue-ts 模板）生成骨架，去掉示例代码与 opener 插件
- 前端：Pinia、vue-router（hash 模式）、Naive UI；侧边栏按路由 `meta.group` 分组生成（日常开发 / 系统）
- 页面占位：每日用量、会话浏览、设置（主题切换）、关于（显示版本号）
- 主题：跟随系统 / 浅色 / 深色，暂存 localStorage
- Rust：托盘（左键显示/隐藏，右键菜单「打开主窗口 / 退出」）、单实例（重复启动激活已有窗口）、关闭主窗口隐藏到托盘
- `pnpm build` 通过；`cargo build` 无警告

**验收记录**

- 开发环境：Rust 1.99.1、VS Build Tools 2022 17.14（C++ 工作负载）、Windows SDK 10.0.26100、Node 24、pnpm 12
- `pnpm tauri dev` 启动出窗口，界面正常渲染，内存约 39MB
- 脚本验证：向主窗口发送 WM_CLOSE 后进程存活、窗口隐藏；再次启动 exe 时新进程自动退出，原窗口被重新显示
- 托盘左键显示/隐藏、右键菜单「打开主窗口 / 退出」：需人工点击验证

**偏差**

- 主题设置暂存 localStorage，M5 接入 tauri store 统一配置后迁移
- 打包目标暂设为 `nsis`，便携版 zip 在 M6 处理

## 待确认

- 正式图标（暂用 Tauri 默认图标）
