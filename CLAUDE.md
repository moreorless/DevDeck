# DevDeck

本机桌面工具箱（Windows 为主，兼容 macOS），Tauri 2 + Vue 3 + TypeScript。完整方案见 `docs/DevDeck 实现方案.md`，进度见 `docs/progress.md`。

## 技术栈

- 前端：Vue 3（组合式 API，`<script setup lang="ts">`）、Vite、Pinia、vue-router（hash 模式）、Naive UI
- 后端：Rust（edition 2024），Tauri 2 commands；SQLite 用 rusqlite（bundled）
- 包管理：pnpm

## 常用命令

```bash
pnpm install
pnpm build                                        # 前端类型检查 + 构建
pnpm tauri dev                                    # 开发运行
cargo test --manifest-path src-tauri/Cargo.toml   # Rust 测试
pnpm tauri build                                  # 打包 NSIS
```

## 目录约定

- `src/app/`：路由、布局、主题；新页面在 `src/app/router.ts` 注册，侧边栏菜单按 `meta.group` 自动生成
- `src/modules/<模块>/`：每个功能模块自带 pages / components / store.ts / api.ts
- `src-tauri/src/claude_log/`：Claude Code 日志定位、扫描、解析，用量和会话两个模块共用
- `src-tauri/src/modules/<模块>/`：模块的查询逻辑和 Tauri commands

## 关键约束

- 不依赖 ccusage 或 Node 运行；ccusage 只在开发期用于核对 token 数
- 日志格式无官方文档：宽松解析，未知 `type`/字段忽略，单行失败不中断；以真实日志（脱敏后存 `src-tauri/tests/fixtures/`）为准，不凭记忆假设字段
- 用量必须按 `(message.id, requestId)` 去重；跳过 `<synthetic>` 模型；cache 写入区分 5m / 1h
- 项目路径取记录里的 `cwd`，不要从 `projects/` 目录名反推
- 价格表来自官方价格页并注明核对日期，不凭记忆填写
- 会话正文不入库、不缓存、不联网
- 前端只读 SQLite（通过 commands），扫描在后台执行，完成后 emit 事件
- 不实现方案第 1 节的非目标；不引入方案第 2 节以外的重量级依赖
- 外部进程调用（如有）必须带超时、`CREATE_NO_WINDOW`（Windows）、错误上报
- 平台差异集中在 `src-tauri/src/platform.rs`（主目录、打开路径等）；不要直接写 `explorer`、`USERPROFILE`、`.exe`

## 工作方式

- 按 M1 → M6 顺序推进；每个里程碑完成后运行测试、提交 commit，并更新 `docs/progress.md`
- 方案未覆盖的决策：选最简单、可回退的做法，记入 `docs/progress.md` 的「待确认」，不阻塞
- 用中文写注释和文档
