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

## M2 日志解析与索引（已完成，2026-11-10）

**已完成**

- `claude_log/`：日志目录定位、文件枚举（含子代理）、宽松单行解析、按字节偏移的增量扫描
- SQLite（WAL）：`file_cursor` / `usage_event` / `sessions` / `scan_log`；全量重建在单个事务中完成
- 价格表 `resources/pricing.json`：来自官方价格页（2026-11-10 核对），支持 5m/1h cache 写、Haiku 5.5 长上下文分档、fast mode、US 地域倍率；费用在查询时计算
- commands：`refresh`、`get_scan_status`、`get_daily_usage`、`get_summary`、`list_projects`、`list_models`、`list_sessions`；事件 `data://updated|error|progress`
- 启动时后台扫描（库为空则全量，否则增量）
- 前端：`shared/data.ts`、各模块 `api.ts`；关于页增加「数据状态」（目录、会话数、用量记录、上次扫描、立即刷新、全量重建）

**验收记录**

- 28 个单元测试通过：解析、价格、扫描（去重、子代理、时区、增量、不完整行、文件重写、文件删除、幂等）、用量与会话查询
- 本机真实日志（805 个文件、1.26GB、36 万行）：release 全量扫描 8.2 秒，无变化增量 29ms；坏行 0
- 与 `ccusage claude daily --json --offline --timezone America/Denver`（ccusage 20.0.48）对照：42 天中 41 天 input / output / cache 写 / cache 读四项完全一致；唯一差异为当天（本会话持续写入，两次扫描时间不同）
- 应用内：启动自动全量扫描（debug 构建 27 秒），关于页「立即刷新」触发增量扫描正常

**偏差与决策**

- 去重：同一 `(message.id, requestId)` 保留 output_tokens 最大的一行（实测有 263 组重复行 usage 不同，个别为全 0）；与 ccusage 对照未出现差异
- 会话以文件为单位（文件名即 sessionId）；文件内少量记录的 sessionId 与文件名不同（续接、转移场景），仍归入该文件的会话
- 标题优先级：`custom-title` > 最后一条 `ai-title`
- 提示数 `prompt_count` 只统计用户真正输入的内容（不含工具结果、meta、系统注入）；列表只显示提示数 > 0 的会话
- 日志文件被删除时：删除会话索引，保留用量（Claude Code 会定期清理旧日志）；但「全量重建」会丢失这部分历史用量
- fast mode 判定为 `usage.speed == "fast"`、US 地域判定为 `inference_geo == "us"`：本机日志只出现过 `standard` / `global` / `not_available`，实际取值待遇到后核实
- 费用尚未与 ccusage 对照（ccusage 离线价格表可能不含新模型），以官方价格页为准

## M2 补充（2026-11-10）

- 全量重建改为保留历史用量（用户确认）：只删除现存日志对应会话的用量后重读，日志已被清理的用量保留，并按当前时区重算日期；新增 2 个测试

## M3 每日用量页（2026-11-10，待人工验收部分交互）

**已完成**

- 指标卡：今日 tokens、今日费用、近 7 天、近 30 天，与上一周期对比；价格未知的模型单独提示
- 筛选栏：今天 / 7 天 / 30 天 / 本月 / 自定义日期、项目多选、模型多选、刷新、最后刷新时间
- 趋势图：每日 token 堆叠柱（按 token 类型 / 按模型切换）+ 每日估算费用柱图
- 明细表：每天一行，展开看模型拆分；日期倒序；≥100 万显示 M、≥10 亿显示 B，悬停显示精确值
- CSV 导出：系统保存对话框 + Rust `export_csv`（每日 × 模型一行，UTF-8 BOM，跟随筛选）；单元测试覆盖
- 空态与异常：未找到日志目录引导去设置、首次扫描进度、刷新失败黄色横幅（保留旧数据）

**验收记录**

- `pnpm build`、`cargo test`（31 通过）
- 本机数据：近 30 天页面正常渲染，指标卡、筛选栏、按类型堆叠图、悬停提示、费用图、明细表截图核对；10-29 与 10-15 的费用与数据库直接计算一致（$276.22、$260.40）
- 调色板验证脚本：浅色 / 深色均 ALL PASS；浅色两色对比度 < 3:1，由明细表与悬停提示补偿

**偏差**

- **不使用右轴费用折线**：dataviz 规范禁止双 y 轴，改为 token 图下方单独的费用图（x 轴对齐）
- ECharts 使用 6.1（vue-echarts 8 要求），方案原写 5
- 数字格式增加 B（≥10 亿），否则 cache 读合计会显示成数千 M
- 按模型堆叠：颜色按模型全量用量排序固定分配，前 7 个独立着色，其余归入「其他」（灰）
- 「按模型」切换时曾观察到柱子悬空、高度异常，已改为整体替换 option（notMerge）并缩短更新动画；截图验证被桌面遮挡中断，需人工确认

**待人工确认**

- 「按模型」堆叠显示是否正常
- 明细表展开、深色主题下的图表、CSV 导出对话框

M3 人工验收：用户确认「按模型」堆叠、明细展开、深色主题、CSV 导出均正常（2026-11-11）。

## M4 会话浏览（2026-11-11）

**已完成**

- `claude_log/transcript.rs`：读取单个会话文件，取主线并转换为展示条目（用户、助手、思考、工具调用 + 结果、压缩点、系统注入）
  - 主线：文件中最后一条非 sidechain 消息沿 `parentUuid` 回溯；压缩边界处改走 `logicalParentUuid` 接上压缩前历史；被回退的分支不展示
  - 工具结果按 `tool_use_id` 挂到对应工具调用；Agent 工具通过 `toolUseResult.agentId` 关联子代理日志
  - 长文本截断（工具输出 2 万字符、输入 1 万、系统内容 5 千），图片只计数不传输
- commands：`get_session`、`get_subagent_session`（agentId 只允许安全字符）、`open_project_dir`（只打开索引中已知会话的目录）
- 前端：左栏会话列表（关键字、项目、日期筛选，分页加载）；右栏详情（Markdown + 代码高亮，原始 HTML 一律转义；思考 / 工具调用 / 系统内容折叠；子代理面包屑导航；长会话分批渲染）
- 复制 `claude --resume <id>`、在资源管理器中打开项目目录
- 首条提示优先取文字提示（开头的斜杠命令信息量低）
- 新增索引版本（`meta` 表）：解析规则变化时启动自动全量重建；本次升到 2

**验收记录**

- `cargo test` 36 通过（新增主线 / 分叉 / 压缩 / 子代理 / 截断 / 首条提示 / 索引版本）
- 本机最大的 3 个会话文件（29 / 24 / 18 MB）：release 构建读取主线 0.15～0.29 秒，传给前端的 JSON 1.2～3.4 MB
- 应用内截图核对：列表、详情、工具调用摘要、系统内容计数正常；索引版本升级后启动自动全量重建

**偏差**

- 用户消息按纯文本显示（保留原样换行），助手消息按 Markdown 渲染
- Markdown 中的链接不可点击跳转（WebView 内跳转会离开应用；外部打开需引入 opener 插件，暂不做）
- 「复制 resume 命令」只复制 `claude --resume <id>`，需在项目目录执行（提示中说明）

**待人工确认**

- 主线消息顺序与 `claude --resume` 看到的一致（方案验收项）
- 子代理查看、压缩点显示、打开项目目录

M4 人工验收：用户确认正常（2026-11-11）。

**M4 之后的增量需求（2026-11-11，已验收）**

- 会话详情「仅显示用户请求」开关；消息时间带月日
- 项目筛选：最近活跃项目置顶分组 + 快捷标签（`list_recent_projects`）

## M5 设置与自动刷新（2026-11-11）

**已完成**

- 配置 `config.json`（`<app_data_dir>`）：Rust 侧统一持有；先写临时文件再改名；文件损坏时用默认配置运行并在设置页提示
- 设置页：
  - 数据源：自动检测目录 + 手动添加（目录选择对话框，校验含 `projects/`）/ 移除；显示每个目录是否存在
  - 时区：跟随系统或 IANA 时区（chrono-tz）；修改需确认，后台全量重建
  - 自动刷新：1 / 5 / 15 / 30 分钟 / 关闭
  - 价格表：内置价格 + 按模型自定义（可新增价格未知的模型）、恢复内置；默认只看本机用到的模型
  - 开机自启、关闭窗口行为（最小化到托盘 / 退出）、主题（写入配置，localStorage 仅做首屏缓存）
  - 全量重建缓存
- 定时增量刷新（`scheduler.rs`，10 秒检查一次是否到期；手动刷新进行中则跳过本轮）
- 设置变更时的扫描排队：正在扫描时等待其结束再执行，保证时区 / 目录修改一定生效
- 托盘：悬停显示 `DevDeck · 今日 1.23M tokens · $4.56`（扫描完成后及每分钟更新，跨零点自动切换）；右键菜单增加「立即刷新」「开机自启（勾选）」
- 开机自启（tauri-plugin-autostart）带 `--minimized` 参数：启动后只驻留托盘；主窗口改为初始隐藏、非自启时再显示，避免闪烁
- 今日、会话日期筛选、用量归日均按配置时区

**验收记录**

- `cargo test` 45 通过（新增配置读写/校验/时区、价格覆盖、调度到期判断、托盘数字格式）；`pnpm build` 通过
- 应用启动后主窗口正常显示（窗口枚举确认，未截图）

**偏差**

- 配置未用 tauri store 插件，改为 Rust 读写 `config.json`：调度、扫描、托盘都在 Rust 侧读取配置，统一持有更简单，也便于测试
- 修改日志目录只做增量扫描（新增目录的文件会被读入；移除目录后会话索引删除、用量保留），不做全量重建
- 自定义价格对分档模型统一按一档计价

**待人工确认**

- 设置页各项保存与生效；托盘悬停文字与「立即刷新」「开机自启」菜单
- 开机自启：开发模式下注册的是 debug 版 exe，测试后请关闭；正式验证放到 M6 安装包
- 断网可用、日志目录配置错误时显示引导（方案验收项）

## M6 打包分发（2026-11-11）

**已完成**

- `tauri.conf.json`：NSIS 简体中文界面、按当前用户安装（无需管理员）、WebView2 内嵌引导程序（`embedBootstrapper`，静默）；补充发布者、版权、描述
- `pnpm release`（`scripts/release.ps1`）：构建并输出到 `release/`（已忽略提交）
  - `DevDeck_<版本>_x64-setup.exe`：NSIS 安装包
  - `DevDeck_<版本>_x64_portable.zip`：单个 exe（前端资源已内嵌）+ 使用说明
- README：安装方式、系统要求、SmartScreen 提示、使用说明、数据位置、开发命令

**验收记录**

- release 构建（LTO）4 分 30 秒；安装包 4.6 MB、便携版 zip 3.5 MB、exe 8.3 MB
- 本机运行便携版：主窗口正常显示，内存 38 MB；启动后自动增量扫描 817 个文件 0.2 秒，坏行 0，无错误

**偏差**

- WebView2 采用内嵌引导程序而非离线安装包：安装包从 ~130 MB 降到 4.6 MB；代价是「没有 WebView2 且无网络」的机器无法安装。Windows 11 自带 WebView2，Windows 10 一般已随系统更新安装
- 便携版的配置与缓存仍在 `%APPDATA%\com.byzer.devdeck\`（与安装版共用），未做「数据跟随 exe 目录」
- 打包脚本需兼容 Windows PowerShell 5：脚本文件带 UTF-8 BOM，读取 JSON 显式指定 UTF-8

**待人工确认**

- 安装包安装 / 卸载流程（开始菜单、卸载后程序目录清理）；未在本机执行安装，以免改动当前环境
- 方案验收项「未装 Node、无网络的干净 Windows 10/11 上安装后正常显示数据」：需在干净机器或虚拟机上验证
- 安装版开机自启（注册的是安装目录的 exe）

**正式图标（2026-11-11）**：源文件 `src-tauri/icons/source.svg`（绿色渐变底 + 两张叠放卡片 + 三根递增柱，对应 Deck 与用量统计），`pnpm tauri icon src-tauri/icons/source.svg` 生成各尺寸，删除 android / ios 目录。16px 托盘下柱形不够清晰，仅保留「绿底白卡」轮廓，可接受。`build.rs` 增加 `rerun-if-changed=icons`：否则增量构建不会重新嵌入 exe 图标资源。

**托盘简化图标（2026-11-11）**：`icons/tray/source.svg` 按 16px 网格绘制（绿底 + 三根白柱，去掉卡片），预渲染 16/20/24/32/40/48，启动时按主显示器缩放选不小于 16×缩放 的尺寸，缩放变化时更新；开启 tauri `image-png` 特性。生成命令：`pnpm tauri icon src-tauri/icons/tray/source.svg -o src-tauri/icons/tray -p 16 -p 20 -p 24 -p 32 -p 40 -p 48`。

## v0.2 规划（2026-11-11）

- 新增 Insights 模块方案（`docs/DevDeck 实现方案.md` 第 12 节）：归档并查看 Claude Code 原生 `/insights` 报告，再支持从 DevDeck 触发；默认关闭
- I0 实测完成（2026-11-11）：`usage-data/` 结构为 `report.html` + `report-<时间戳>.html` + `facets/` + `session-meta/`；每次运行新增约 50 个会话的分析；报告自带内联脚本（折叠、复制）并外链 Google Fonts
- `claude -p "/insights" --output-format json` 可后台运行：78 秒，退出码 0，`result` 中含报告路径，本次 $1.69；Git Bash 下参数会被 MSYS 改写，须由 PowerShell / CreateProcess 调用
- 据此修订方案 12 节：触发改为后台方式，放弃打开终端方式；iframe 改为允许脚本、不给同源

### I1 归档与查看（2026-11-11）

- 后端 `modules/insights/`：
  - `archive.rs`：扫描各 Claude 目录的 `usage-data/report*.html`（带时间戳的副本优先于 `report.html`），按 sha256 去重复制到 `<app_data_dir>/insights/<时间>-<哈希前 12 位>.html`；解析头部「已分析 / 总会话数、时间范围」，失败留空
  - 删除 / 清除：删文件，记录只留哈希（`deleted_at`），之后不会再次归档同一份
  - 显示前去掉外链 `<link>`（Google Fonts）并注入 `default-src 'none'` 的 CSP；归档原文件不改
  - commands：`list_insights` / `sync_insights` / `get_insight_html` / `open_insight_external` / `delete_insight` / `clear_insights`；事件 `insights://updated`
- 归档时机：每次扫描成功后（模块开启时）；开启模块时立即执行一次；页面「检查新报告」
- 数据库迁移到 user_version 3（新表 `insights_report`）；配置新增 `insights.enabled`（默认 false）；新依赖 `sha2`（Tauri 已间接依赖）
- 前端：`/insights` 页面（模块开启才出现在侧边栏），左侧报告列表、右侧 `sandbox="allow-scripts"` 的 srcdoc iframe，可「用浏览器打开」「删除」；设置页新增 Insights 卡片（开启需确认，显示已归档份数，清除全部）
- 测试：新增 7 个单元测试（头部解析、去重、源目录清理后仍可读、删除后不再归档、清除、非法文件名）+ 1 个本机真实报告测试（`cargo test real_report -- --ignored`，3 份报告均解析成功）；共 53 个通过

**待手动验证（`pnpm tauri dev`）**

- 设置中开启 Insights → 侧边栏出现 Insights，列表中有 2 份报告（01:16、02:15）
- 报告内折叠面板可用；深色主题下报告为白底；「用浏览器打开」正常
- 删除一份 → 列表减少，「检查新报告」不会再归档回来；设置页「清除全部归档」后列表为空
- 关闭模块 → 侧边栏不再显示


### I2 一键生成与中文翻译（2026-11-11）

- 补充需求：报告使用中文。核查 claude 程序源码：`/insights` 各部分调用使用固定空系统提示、HTML 模板写死英文，`language` 设置（本机已设为简体中文）和 `--append-system-prompt` 都无效 → 改为生成后翻译
- 后端 `modules/insights/`：
  - `cli.rs`：查找 claude（设置 → PATH 的 claude.exe / claude.cmd → `%USERPROFILE%\.local\bin\claude.exe`）；`run_json` 带超时、`CREATE_NO_WINDOW`，stdin / stdout / stderr 各一个线程，解析 `--output-format json` 结果，失败时上报退出码与 `result` / stderr 末尾
  - `translate.rs`：抽取文本节点 → 去重分批 → 并行 `claude -p --model sonnet`（自定义翻译系统提示、无工具、无 MCP、无斜杠命令、不保存会话、关闭 hooks）→ 按位置填回；脚本中的固定界面文字按对照表替换
  - `job.rs`：后台任务（生成 → 归档 → 翻译，或单独翻译），同一时刻一个，`insights://job` 推送状态；记录估算费用
  - 迁移 user_version 4：`translated_file`、`cost_usd`；配置新增 `claudePath`、`translate`（默认开启）
- 前端：Insights 页「生成新报告」（确认框说明费用与发送内容）、运行中显示阶段与耗时、错误提示；报告「中文 / 原文」切换、「翻译为中文」；列表显示「中文」标记与 DevDeck 生成费用。设置页新增「翻译为中文」开关与 claude 路径（显示自动检测结果，可手动选择）
- 测试：新增 8 个单元测试（CLI 结果解析、文本节点抽取、去重分批、填回保结构、实体解码、译文数组校验、脚本文字替换、译文保存与删除），共 60 个通过；2 个本机测试（`real_report_segments`、`real_translate`，后者会调用 claude）
- 实测翻译 3 次（调整提示词，其中 1 次因脚本编码问题提示词未生效，白跑 $0.105）：最终版 29 秒、$0.16；建议写入 CLAUDE.md 的规则段落也已翻译，命令与路径保持原样，标签与脚本和原文一致
- 未实测：带 `--no-session-persistence` / `--strict-mcp-config` / `disableAllHooks` 的生成命令（I0 实测的是不带这些参数的版本），由第一次点击「生成新报告」验证

**修复（2026-11-11）**：首次一键生成成功（带新参数的生成命令已验证），但翻译失败「翻译结果条数不符：期望 76，实际 77」——模型把一条拆成两条。改为「编号 → 文本」JSON 对象按编号对齐，漏译编号重试一次，仍缺失的保留原文并在完成提示中注明段数。在新报告（237 会话）上实测：33 秒、$0.167、0 段保留原文

**待手动验证（`pnpm tauri dev`）**

- 已有的两份英文报告：「翻译为中文」→ 约 30 秒后出现「中文」标记，默认显示中文，可切换原文；图表时段与复制按钮为中文
- 「生成新报告」→ 确认 → 运行中显示「正在生成 / 正在翻译 · 已用 m:ss」→ 完成后自动选中新报告并显示中文，列表显示费用
- 设置页 claude 程序显示 `%USERPROFILE%\.local\bin\claude.exe（自动检测）`；关闭「翻译为中文」后生成的报告只有原文


**默认开启（2026-11-11）**：按用户要求，`insights.enabled` 默认改为 true。已有 config.json 中保存的值不变；新安装启动后自动归档本机已有报告，生成新报告仍需点击并确认。

## macOS 兼容（2026-11-11）

- 用户要求代码兼容 Windows 与 Mac。新增 `platform.rs`（主目录、打开路径、主目录写法），替换两处 `explorer` 调用与 `USERPROFILE` 读取
- 查找 claude 按系统区分文件名与安装目录；macOS 应用 PATH 很短，额外检查 Homebrew、`/usr/local/bin`、npm / bun 全局目录，并为子进程补全 PATH
- 托盘：macOS 使用单色模板图标（`icons/tray/macos/source.svg` → `template@2x.png`，36px），点击弹出菜单；`RunEvent::Reopen` 时显示主窗口
- 设置页：路径提示按系统显示（`SettingsView` 新增 `platform`、`homeLabel`）；选择 claude 时只在 Windows 按 exe/cmd 过滤；等宽字体补 SF Mono / Menlo
- 打包：`tauri.macos.conf.json`、`scripts/release-mac.sh`（`pnpm release:mac [--universal]`，兼容 macOS 自带 bash 3.2）；Windows 打包配置不变
- 验证：Windows 上 60 个测试通过、clippy 无新增告警、前端构建通过。尝试 `cargo check --target aarch64-apple-darwin` 失败（objc2 等 C 依赖需要 macOS 工具链），macOS 专用代码改为对照 Tauri 2.12.5 源码核对（`RunEvent::Reopen`、`icon_as_template`）
- GitHub Actions（`.github/workflows/build-macos.yml`，2026-11-12）：macos-latest 上手动触发或推送 `v*` 标签时构建通用包（Apple 芯片 + Intel），先跑 Rust 测试再打包，产物上传为 Artifact；标签触发时另建 Release 草稿。首次运行 #43163623605 约 16 分钟成功：macOS 上 60 个测试通过，产出 `DevDeck_0.1.0_universal.dmg`（7.7MB）与 `.app.zip`（7.6MB）
- 工作流改为 `.github/workflows/build.yml`（2026-11-12）：Windows（`pnpm release`：NSIS 安装包 + 便携版）与 macOS（通用包）矩阵并行，`fail-fast: false`；各平台先跑 Rust 测试再打包，产物分别上传为 `DevDeck-windows` / `DevDeck-macos`；推送 `v*` 标签时由单独的 release 任务在两个平台都成功后创建 Release 草稿（避免两个任务同时创建）
- 待 Mac 上运行验证（编译与测试已在 CI 通过）：菜单栏图标深浅色、点击图标弹出菜单、程序坞恢复窗口、开机自启、日志目录与 claude 自动检测、Insights 生成与翻译、访达打开项目目录

## 套餐额度显示（2026-11-12）

- 需求：显示当前会话、本周、Fable 的已用百分比与重置时间。核查后采用 magpie 的方案：运行 `claude -p /usage`（不调用模型，本机实测约 5 秒、费用 0），解析文本输出；详见方案第 13 节
- 后端：新增 `modules/quota/`（解析、缓存、后台定时、commands `get_quota` / `refresh_quota`、事件 `quota://updated`）；`insights/cli.rs` 移为公共的 `claude_cli.rs`；配置新增 `quota.enabled`（默认开启）；托盘提示增加额度行
- 前端：每日用量页顶部「套餐额度」卡片；设置页「统计与刷新」新增开关
- 测试：新增 7 个单元测试（真实输出解析、重置时间格式与跨年、ANSI 与无重置时间、无额度与账号错误、后台调度条件、托盘文字、读数保存），共 67 个通过；本机测试 `real_usage`（实际运行 /usage，4.8 秒解析出 3 个窗口）
- 待手动验证：每日用量页卡片显示与刷新、托盘提示第二行、关闭开关后卡片隐藏

## 待确认

- 离线且无 WebView2 的机器：如有需要，另出 `offlineInstaller` 版本安装包

- fast mode 与 US 地域在日志中的实际字段取值

