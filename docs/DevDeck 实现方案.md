# DevDeck 实现方案（v0.1 版本）

文档修订：2026-11-06 初稿（ClaudeMeter，基于 ccusage）→ 2026-11-09 修订（改名 DevDeck、去掉 ccusage、新增会话浏览）→ 2026-11-11 增补第 12 节 v0.2 Insights 模块 · @byzer

## 1. 背景与目标

DevDeck 是运行在 Windows 本机的桌面工具箱，托盘常驻，以模块承载功能。v0.1 交付「日常开发」主线下的两个模块，都基于本机 Claude Code 日志（`~/.claude/projects/**/*.jsonl`）：

1. **每日用量**：按天展示 token 用量（input / output / cache 写入 / cache 读取 / 合计）与估算费用（USD），支持按模型拆分、按项目筛选、自定义日期范围
2. **会话浏览**：查看 Claude Code 历史会话列表，以及会话内的对话记录

另外：托盘常驻，悬停显示今日用量；完全离线可用。

**非目标（v0.1 不做）**

- Claude.ai 网页 / 桌面聊天的用量（本地无数据）
- 组织级 Admin API 用量、多机汇总、局域网访问
- 超额告警、预算管理（v2 候选）
- 运行时依赖 ccusage 或 Node（ccusage 只在开发期作为 token 数的对照工具）
- 会话全文搜索（v0.2，SQLite FTS5）；会话的编辑、删除、导出
- 人民币显示

## 2. 技术选型

| 层 | 选型 | 说明 |
| --- | --- | --- |
| 桌面框架 | Tauri 2.x | 原生窗口 + 托盘 + 安装包；依赖 WebView2 |
| 前端 | Vue 3 + TypeScript + Vite | 组合式 API；状态管理 Pinia；路由 vue-router（hash 模式） |
| UI 组件 | Naive UI | 暗色主题开箱即用 |
| 图表 | ECharts 5（vue-echarts） | 堆叠柱状图 + 折线（M3 引入） |
| Markdown | markdown-it + highlight.js（M4 引入） | 渲染对话内容 |
| 后端 | Rust（Tauri commands） | 解析 jsonl、建索引、读写缓存、系统集成 |
| 本地缓存 | SQLite（rusqlite，bundled 特性） | 用量明细、会话索引、文件读取游标 |
| Tauri 插件 | single-instance、autostart、dialog | 官方插件；配置由 Rust 读写 config.json，不用 store 插件 |

## 3. 总体架构与数据流

```text
~/.claude/projects/**/*.jsonl
        │  （增量读取：按文件记录已读字节偏移）
        ▼
  Scanner ──► JsonlParser ──► UsageRecord（去重后）──► usage_event 表 ──► 按日聚合查询
                         └──► SessionMeta ────────────► sessions 表
                                                        ▲
  前端 ◄── Tauri commands（只读 SQLite）                 │
  会话详情 ◄── 按需流式读取单个 jsonl，构建消息树 ─────────┘（正文不入库）
```

- 前端只读 SQLite；扫描由调度器在后台执行，完成后 emit 事件通知前端
- 一次扫描同时产出用量记录和会话索引，避免重复读取日志（本机实测 736 个文件、1.1GB）
- 会话正文不入库：打开会话时按需读取对应文件，保护隐私也控制数据库体积

## 4. 日志格式要点（2026-11-09 本机实测）

- 文件位置：`projects/<编码路径>/<sessionId>.jsonl`；子代理在 `<sessionId>/subagents/*.jsonl`；超长工具输出在 `<sessionId>/tool-results/`
- **目录名不可还原为路径**（中文等字符被替换为 `-`），项目真实路径取记录中的 `cwd` 字段
- 记录 `type` 约 20 种：`user`、`assistant`、`system`、`attachment`、`ai-title`、`last-prompt`、`api-request*`、`file-history-*`、`queue-operation`、`cost-state` 等；格式无官方文档，可能随版本变化
- 消息通过 `uuid` / `parentUuid` 组成树，回退、编辑会产生分叉；`isSidechain`、`isCompactSummary`、`isMeta` 标记特殊消息
- **用量去重**：一次 API 响应会拆成多行写入，每行携带相同 `usage`。实测 60,193 行去重后 27,833 条，必须按 `(message.id, requestId)` 去重
- `usage.cache_creation` 区分 `ephemeral_5m_input_tokens` 与 `ephemeral_1h_input_tokens`，两者单价不同
- 存在 `speed` 字段（疑似 fast mode）与 `<synthetic>` 模型（非真实请求，不计费）

## 5. 数据层设计

### 5.1 解析器

- 逐行宽松解析（`serde_json::Value`），单行失败只记日志不中断；未知 `type` 与字段一律忽略
- 用量记录：`type == "assistant"` 且有 `message.usage`；跳过 `<synthetic>`；按 `(message.id, requestId)` 去重（跨文件也去重，主键约束保证）
- 日期：按配置时区（默认系统时区）把 `timestamp` 换算成本地日期
- 会话元数据：sessionId、cwd（取首个非空值）、gitBranch、标题（最后一条 `ai-title`）、首条用户提示、最后提示（`last-prompt`）、起止时间、消息数、模型集合、是否含子代理
- 测试：从本机日志抽取样本并**脱敏**后存入 `src-tauri/tests/fixtures/`，覆盖去重、分叉、压缩、子代理、坏行

### 5.2 价格表

- 内置 `pricing.json`：按模型给出 input、output、cache 5m 写、cache 1h 写、cache 读单价（USD / 百万 token），记录来源 URL 与核对日期
- 实现时从 Anthropic 官方价格页核对，**不凭记忆填写**；未知模型费用记为 0，界面标注「价格未知」
- 设置页可查看，并按模型覆盖单价；界面所有费用均标注「估算」

### 5.3 SQLite 表

```sql
CREATE TABLE file_cursor (            -- 增量读取游标
  path TEXT PRIMARY KEY, size INTEGER, mtime INTEGER, offset INTEGER, session_id TEXT
);
CREATE TABLE usage_event (            -- 去重后的单次请求用量
  message_id TEXT NOT NULL, request_id TEXT NOT NULL,
  session_id TEXT NOT NULL, project TEXT NOT NULL, model TEXT NOT NULL,
  ts TEXT NOT NULL, date TEXT NOT NULL,           -- date 为配置时区下的 YYYY-MM-DD
  input_tokens INTEGER, output_tokens INTEGER,
  cache_write_5m_tokens INTEGER, cache_write_1h_tokens INTEGER, cache_read_tokens INTEGER,
  speed TEXT,
  PRIMARY KEY (message_id, request_id)
);
CREATE INDEX idx_usage_date ON usage_event(date);
CREATE TABLE sessions (
  session_id TEXT PRIMARY KEY, file_path TEXT, project TEXT, git_branch TEXT,
  title TEXT, first_prompt TEXT, last_prompt TEXT,
  started_at TEXT, ended_at TEXT, message_count INTEGER, models TEXT, has_subagents INTEGER
);
CREATE TABLE scan_log (id INTEGER PRIMARY KEY, started_at TEXT, finished_at TEXT,
  files INTEGER, ok INTEGER, error TEXT);
```

- 费用在查询时用价格表计算（价格变更无需重扫）
- 数据库位置：`%APPDATA%\com.byzer.devdeck\devdeck.db`（Tauri app_data_dir）

### 5.4 刷新策略

| 场景 | 做法 |
| --- | --- |
| 首次启动 / 手动「全量重建」 | 后台全量扫描，界面显示进度；在事务中写入新数据，成功后替换，失败保留旧数据 |
| 定时刷新（默认 5 分钟） | 对比文件 size/mtime，只读新增字节；文件变小视为被重写，重读该文件 |
| 修改时区 / 日志目录 | 全量重建 |

## 6. 功能需求

### 6.1 每日用量页

- **指标卡**（4 张，始终为全局数据，不受筛选影响）：今日 tokens、今日费用、近 7 天费用、近 30 天费用；与上一周期对比涨跌百分比
- **筛选栏**：日期范围（今天 / 7 天 / 30 天 / 本月 / 自定义）、项目多选、模型多选、刷新按钮、最后刷新时间
- **趋势图**：每日堆叠柱状图（input / output / cache 写入 / cache 读取），右轴折线为每日费用；可切换「按 token 类型」/「按模型」堆叠
- **明细表**：每天一行汇总，「模型」列显示模型数；行展开显示按模型拆分；日期倒序；导出 CSV（跟随当前筛选）
- 数字格式：千分位；≥100 万显示为 `1.23M`，悬停显示精确值

### 6.2 会话浏览页

- **会话列表**：按项目筛选、日期范围、标题/首条提示关键字搜索；时间倒序；显示标题、首条提示、起止时间、消息数、主要模型、分支、会话 token 与估算费用
- **对话详情**：
  - 显示主线：从最后的叶子节点沿 `parentUuid` 回溯，不展示其他分支
  - 用户与助手文本渲染 Markdown 与代码高亮
  - thinking、工具调用、工具结果默认折叠，长输出截断并可展开
  - attachment、system、meta 默认隐藏，提供开关显示
  - 压缩点显示分隔标记与摘要
  - 子代理：在对应工具调用处提供入口，单独查看其对话
- **操作**（只读）：复制 `claude --resume <sessionId>`、在资源管理器中打开项目目录

### 6.3 设置页

- Claude 日志目录（默认自动检测，可追加多个）
- 时区（默认系统时区）
- 自动刷新间隔（1 / 5 / 15 / 30 分钟、关闭）
- 价格表查看与覆盖
- 开机自启、关闭窗口时最小化到托盘
- 主题：跟随系统 / 浅色 / 深色
- 「全量重建缓存」按钮（二次确认）

### 6.4 托盘

- 悬停提示：`今日 1.23M tokens · $4.56`
- 右键菜单：打开主窗口、立即刷新、开机自启（勾选）、退出
- 左键单击：显示/隐藏主窗口

### 6.5 异常状态

| 情况 | 表现 |
| --- | --- |
| 未找到 Claude 日志目录 | 空态提示，展示已检查的路径列表，引导去设置页 |
| 单行解析失败 | 跳过并计数，关于页 / 日志中可见 |
| 扫描整体失败 | 保留旧数据，顶部黄色横幅显示错误摘要 |
| 会话文件已被删除 | 详情页提示文件不存在，列表下次扫描时移除 |

## 7. 工程结构与接口

### 7.1 目录结构

```text
DevDeck/
├─ src/                          # Vue 3 前端
│  ├─ app/                       # 路由、布局、主题
│  ├─ modules/
│  │  ├─ usage/                  # 每日用量模块
│  │  └─ sessions/               # 会话浏览模块
│  ├─ pages/                     # Settings.vue / About.vue
│  └─ shared/                    # 格式化、通用组件
├─ src-tauri/
│  ├─ src/
│  │  ├─ lib.rs / main.rs
│  │  ├─ tray.rs / window.rs
│  │  ├─ claude_log/             # 日志定位、扫描、jsonl 解析（两个模块共用）
│  │  ├─ modules/usage/          # 用量查询、价格计算、commands
│  │  ├─ modules/sessions/       # 会话索引查询、详情读取、commands
│  │  ├─ db/                     # SQLite 连接与迁移
│  │  ├─ scheduler.rs
│  │  └─ config.rs
│  ├─ resources/pricing.json
│  ├─ tests/fixtures/            # 脱敏后的 jsonl 样本
│  ├─ capabilities/default.json
│  └─ tauri.conf.json
└─ docs/
```

### 7.2 Tauri commands

| 命令 | 入参 | 返回 | 说明 |
| --- | --- | --- | --- |
| `get_daily_usage` | since, until, projects?, models? | `DailyUsage[]` | 按日按模型聚合 |
| `get_summary` | — | 今日 / 7 天 / 30 天汇总 | 指标卡与托盘 |
| `list_projects` / `list_models` | — | `string[]` | 筛选项 |
| `export_csv` | since, until, projects?, models? | 文件路径 | 弹出保存对话框 |
| `list_sessions` | project?, since?, until?, keyword?, offset, limit | `SessionMeta[]` + 总数 | 分页 |
| `get_session` | sessionId | `SessionDetail` | 主线消息列表 |
| `get_subagent_session` | sessionId, agentFile | `SessionDetail` | 子代理对话 |
| `refresh` | mode: `incremental` \| `full` | `ScanResult` | 触发扫描 |
| `get_config` / `set_config` | `AppConfig` | `AppConfig` | 持久化到 store |

扫描完成后 emit `data://updated`；失败 emit `data://error`；全量扫描期间 emit `data://progress`。

### 7.3 配置结构

```json
{
  "claudeDirs": [],
  "timezone": null,
  "refreshIntervalMin": 5,
  "pricingOverrides": {},
  "autostart": false,
  "closeToTray": true,
  "theme": "system"
}
```

`timezone` 为 null 表示跟随系统。

## 8. Windows 细节与打包

- 日志目录自动检测顺序：`CLAUDE_CONFIG_DIR` → `%USERPROFILE%\.claude` → `%USERPROFILE%\.config\claude`
- 路径含中文、空格时正常工作；文件按 UTF-8 读取，非法字节替换而非报错
- 读取日志时不加独占锁，允许 Claude Code 同时写入；末尾不完整的行留到下次读取
- 单实例：重复启动时激活已有窗口
- WebView2：安装包用 `embedBootstrapper` 或 `offlineInstaller`，保证离线机器可装
- 代码签名：v0.1 不签名，README 说明 SmartScreen 提示
- 产物：NSIS 安装包与便携版 zip
- 构建环境：Windows 10/11 x64、Rust stable（MSVC 工具链）、Node 20+、pnpm；`pnpm tauri build`

## 9. 里程碑与验收标准

1. **M1 工程骨架**：Tauri 2 + Vue 3 + TS 初始化，路由与布局、主题切换、单实例、托盘最小可用
   - 验收：`pnpm tauri dev` 启动出窗口；托盘可显示/隐藏窗口、退出；关闭窗口隐藏到托盘；重复启动激活已有窗口
2. **M2 日志解析与索引**：日志定位、增量扫描、解析器、去重、SQLite、会话索引、价格表
   - 验收：fixture 测试覆盖去重、分叉、压缩、子代理、坏行；同一日期范围的每日 token 数与 `npx ccusage daily --json` 一致
3. **M3 每日用量页**：指标卡、筛选栏、趋势图、明细表、CSV 导出
   - 验收：近 30 天数据 1 秒内渲染；筛选后合计正确
4. **M4 会话浏览**：会话列表、对话详情、子代理查看、复制 resume 命令
   - 验收：本机最大的会话文件打开时间 ≤ 2 秒；主线消息顺序与 Claude Code `--resume` 时看到的一致
5. **M5 设置与自动刷新**：设置页全部配置项、定时增量刷新、开机自启、托盘今日用量、异常状态
   - 验收：断网可用；日志目录配置错误时显示引导而不崩溃
6. **M6 打包分发**：NSIS 安装包、便携版、README
   - 验收：在一台未装 Node、无网络的干净 Windows 10/11 上安装后正常显示数据

**整体验收清单**

- [ ] 冷启动到看到数据 ≤ 2 秒（有缓存时）
- [ ] 增量刷新期间界面不卡顿
- [ ] 内存占用常驻 ≤ 150MB
- [ ] 所有 Rust 单元测试与 fixture 测试通过
- [ ] 中文路径、带空格路径下正常工作

## 10. 风险

| 风险 | 影响 | 应对 |
| --- | --- | --- |
| Claude Code 日志格式变化 | 统计或会话展示偏差 | 宽松解析；未知类型忽略；fixture 测试；关于页显示解析失败行数 |
| 价格表过期 | 费用估算不准 | 标注「估算」；记录核对日期；设置页可覆盖 |
| 去重口径与 ccusage 不一致 | token 数对不上 | M2 用 ccusage 对照验证，差异记录在 progress.md |
| 日志量大时首次全量扫描慢 | 首次启动等待 | 后台扫描并显示进度；之后只增量读取 |
| 会话正文含敏感信息 | 泄露 | 正文不入库、不缓存、不联网 |

## 11. 已确认的决策（2026-11-09）

- 产品名 DevDeck，工程、标识（`com.byzer.devdeck`）、数据目录统一
- 默认时区跟随系统
- UI 组件库 Naive UI
- 费用只显示 USD
- 不依赖 ccusage，自行解析 jsonl
- 会话浏览纳入 v0.1（M4），只读
- 明细表每天一行，展开看模型拆分；指标卡显示全局数据，导出跟随筛选
- 全量重建在事务中完成

正式图标已完成（2026-11-11），见 `docs/progress.md`。

## 12. v0.2 规划：Insights 模块（2026-11-11 确认）

### 12.1 目标

在 DevDeck 中使用 Claude Code 原生的 `/insights`（分析本机近期会话，指出工作中的问题与摩擦点，给出使用建议），并长期保存每次生成的报告。

- **不自行实现分析**：只启动原生命令、读取其产物；DevDeck 不调用模型、不需要账号
- **先做记录、后做触发**：归档与查看不依赖调用方式，风险低，先交付

### 12.2 已确认的事实（官方文档，2026-11-11 核对）

- `/insights` 生成 HTML 报告：分析本机近期会话（不含其他设备、claude.ai、云端会话），每次最多分析 200 个此前未分析过的会话，跳过过短会话；报告头部显示「已分析数（总数）」
- 产物位于 `~/.claude/usage-data/`：最新报告 `report.html`、每次运行的带时间戳副本、按会话缓存的分析数据
- 该目录受 `cleanupPeriodDays` 控制（默认 30 天，启动时清理）→ DevDeck 需要自行归档
- 分析通过用户自己的 provider / 账号运行，消耗套餐额度或 API token
- 文档来源：costs.md「Analyze your usage patterns」、commands.md、claude-directory.md、headless.md

**I0 本机实测（2026-11-11，Claude Code 2.1.322）**

- `usage-data/` 结构：
  - `report.html`：最新报告；`report-YYYY-MM-DD-HHMMSS.html`：每次运行的时间戳副本，内容与当次的 `report.html` 相同
  - `facets/<session_id>.json`：按会话的模型分析结果（目标、结果、摩擦点、摘要等字段）
  - `session-meta/<session_id>.json`：按会话的本地统计（时长、消息数、工具次数、token、代码行数等）
- 每次运行新增约 50 个会话的模型分析（两次运行 facets 各 +50），报告按累计结果重新汇总
- 报告头部格式：`<p class="subtitle">1,842 messages across 228 sessions (526 total) | 2026-07-27 to 2026-11-11</p>`
- 报告 HTML 约 60–70KB，自带一段内联脚本（折叠面板、复制按钮、时段直方图切换），外链 Google Fonts；不加载其他外部资源、不发请求
- **`claude -p "/insights" --output-format json` 可以后台运行**：78 秒完成，退出码 0，`subtype=success`；`result` 文本中包含报告路径（`file://...report-<时间戳>.html`）；本次 `total_cost_usd` 为 1.69
- 注意：从 Git Bash 调用时，MSYS 会把参数 `/insights` 改写成 `C:/Program Files/Git/insights`，导致命令没有被执行（这次也消耗了一次调用）；DevDeck 用 CreateProcess 直接传参，不受影响
- 后台运行本身会产生一条新的会话日志（会被用量统计计入，属正常）
- 未测：`claude "/insights"` 交互启动（方式 A 已可用，不再需要）

### 12.3 功能

1. **归档**（I1）
   - 每次扫描时检查 `<Claude 目录>/usage-data/`，把新出现的报告 HTML 复制到 `<app_data_dir>/insights/`（按内容哈希去重），源文件不改动
   - `facets/`、`session-meta/` 不读取、不归档（格式未公开，且 facets 是逐会话摘要）
2. **查看**（I1）
   - 新页面「Insights」（日常开发分组）：左侧报告列表（生成时间、已分析 / 总会话数），右侧显示报告
   - 报告放在沙箱 iframe（`srcdoc`，`sandbox="allow-scripts"`，不给 `allow-same-origin`）中显示：报告自带的折叠等脚本可运行，但无法访问 DevDeck 页面与 Tauri 接口；复制按钮在沙箱中可能无效
   - 提供「用系统浏览器打开」（打开归档文件）
   - 单份删除（报告页）、全部清除（设置页，模块关闭时也可用）；删除后只保留哈希，同一份报告不会再次归档
3. **触发**（I2）
   - 后台执行 `claude -p "/insights" --output-format json`（`CREATE_NO_WINDOW`，工作目录为 app_data_dir），完成后立即归档并通知；页面显示运行中状态与耗时
   - 结果判断：退出码 0 且 `subtype=success` 为成功；否则显示 `result` / stderr 摘要
   - 触发前确认对话框：说明会消耗额度（本机实测一次约 $1.7，按 API 价格估算）、会把会话内容发送给模型
   - `claude` 可执行文件：设置中手动指定 → PATH → `%USERPROFILE%\.local\bin\claude.exe`
   - 已放弃「打开终端执行」方式：后台方式已验证可用，不再需要
4. **结构化提取**（v0.2 之后候选）：若确认会话日志或缓存中有稳定的结构化结果，再做「问题 / 建议」条目的跨报告对比

### 12.4 约束与隐私

- 模块**默认关闭**，在设置中由用户主动开启；关闭后不扫描 `usage-data/`、不显示页面
- 第 10 节「正文不入库、不缓存、不联网」调整为：**DevDeck 自身不联网**；insights 报告（模型对会话的总结）是唯一被归档的会话衍生内容，可随时一键清除；会话正文仍不入库
- 外部进程：带超时（15 分钟）、`CREATE_NO_WINDOW`、退出码与 stderr 上报；同一时刻只允许一个 insights 任务
- 解析宽松：只依赖「是 HTML 文件 + 文件时间」；「已分析 / 总会话数」解析失败时留空，不影响归档

### 12.5 数据

```sql
CREATE TABLE insights_report (
  id INTEGER PRIMARY KEY,
  sha256 TEXT UNIQUE NOT NULL,       -- 去重；删除后仍保留，避免再次归档
  source_path TEXT NOT NULL,         -- 原始路径（可能已被 Claude Code 清理）
  archived_file TEXT,                -- <app_data_dir>/insights/ 下的文件名；删除后为空
  generated_at TEXT NOT NULL,        -- 源文件修改时间（UTC）
  subtitle TEXT,                     -- 报告头部原文，以下统计从中解析，可空
  sessions_analyzed INTEGER,
  sessions_total INTEGER,
  period_start TEXT,
  period_end TEXT,
  bytes INTEGER NOT NULL,
  archived_at TEXT NOT NULL,
  deleted_at TEXT                    -- 删除时间；删除后清空文件名与统计，只留哈希
);
```

配置新增：`insights.enabled`（默认 false，I1）、`insights.claudePath`（可空，I2）。

### 12.6 里程碑与验收

- **I0 实测**（已完成 2026-11-11）：结论见 12.2
- **I1 归档与查看**：验收——开启模块后，已有报告被归档并可查看；删除 `usage-data/` 后归档仍可查看；关闭模块后不再扫描；一键清除后归档目录为空、表中只剩哈希，再次扫描不会重新归档
- **I2 触发**：验收——从 DevDeck 触发后，新报告在完成后 1 分钟内出现在列表中；`claude` 不存在、超时、非零退出时显示明确错误，不崩溃

### 12.7 风险

| 风险 | 影响 | 应对 |
| --- | --- | --- |
| 后续版本 `-p` 模式不再支持 `/insights` | 无法后台一键生成 | 错误提示中引导用户在 Claude Code 中手动运行，归档照常工作 |
| 报告格式 / 文件命名随版本变化 | 解析失败 | 只归档原文件，不依赖内部结构 |
| 报告含项目名与工作内容摘要 | 隐私 | 默认关闭、可一键清除、不联网 |
| 每次运行消耗额度 | 费用 | 触发前确认；只在用户操作时触发，不做定时触发 |

