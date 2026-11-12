//! 套餐额度：当前 5 小时会话、本周全部模型、本周各模型（如 Fable）的已用百分比与重置时间
//!
//! 数据全部来自 Claude Code 自带的 `/usage`（`claude -p /usage`，不调用模型、不消耗额度），
//! DevDeck 不读取凭据、不直接请求 Anthropic（思路参考 magpie）。
//! 结果缓存在内存并存入 `meta` 表，重启后先显示上次的读数。

pub mod commands;
pub mod parse;

use std::{
    path::Path,
    sync::Mutex,
    time::{Duration, Instant},
};

use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::{claude_cli as cli, claude_log::scanner::fmt_ts, state::AppState, tray};
use parse::{Parsed, QuotaWindow};

pub const EVT_QUOTA_UPDATED: &str = "quota://updated";
const META_KEY: &str = "quota.snapshot";
const RUN_TIMEOUT: Duration = Duration::from_secs(60);
/// 两次读取的最小间隔（手动刷新也受限，避免连点）
const MIN_GAP: Duration = Duration::from_secs(30);
/// 后台读取：距上次读取至少这么久，且这段时间里 Claude Code 有新的用量
pub const BACKGROUND_GAP: Duration = Duration::from_secs(10 * 60);
/// 即使没有新用量，读数超过这么久也重读一次（窗口可能已重置）
pub const STALE_AFTER: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaSnapshot {
    pub windows: Vec<QuotaWindow>,
    /// 最近一次成功读取的时间（UTC）
    pub fetched_at: Option<String>,
    /// 没有额度信息时的说明（如 API Key 计费）
    pub note: Option<String>,
    /// 最近一次读取失败的原因；成功后清空
    #[serde(skip_deserializing)]
    pub error: Option<String>,
    #[serde(skip_deserializing)]
    pub refreshing: bool,
}

#[derive(Default)]
pub struct QuotaState {
    snap: Mutex<QuotaSnapshot>,
    last_try: Mutex<Option<Instant>>,
}

impl QuotaState {
    /// 以上次保存的读数初始化
    pub fn new(saved: QuotaSnapshot) -> Self {
        Self { snap: Mutex::new(saved), last_try: Mutex::new(None) }
    }
    pub fn snapshot(&self) -> QuotaSnapshot {
        self.snap.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    fn update(&self, f: impl FnOnce(&mut QuotaSnapshot)) -> QuotaSnapshot {
        let mut s = self.snap.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut s);
        s.clone()
    }
    /// 距上次尝试读取的时间；从未读取为 None
    pub fn since_last_try(&self) -> Option<Duration> {
        self.last_try.lock().unwrap_or_else(|e| e.into_inner()).map(|t| t.elapsed())
    }
}

/// 启动时载入上次保存的读数
pub fn load_saved(conn: &Connection) -> QuotaSnapshot {
    conn.query_row("SELECT value FROM meta WHERE key = ?1", [META_KEY], |r| r.get::<_, String>(0))
        .optional()
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save(conn: &Connection, snap: &QuotaSnapshot) -> Result<()> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![META_KEY, serde_json::to_string(snap)?],
    )?;
    Ok(())
}

/// 运行 `claude -p /usage` 并解析
fn read_usage(claude: &Path, cwd: &Path) -> Result<Parsed> {
    let out = cli::run_json(
        claude,
        &[
            "-p",
            "/usage",
            "--tools",
            "",
            "--strict-mcp-config",
            // 不加载用户 / 项目设置：不触发 hooks、不加载插件
            "--setting-sources",
            "",
            "--no-session-persistence",
        ],
        None,
        cwd,
        RUN_TIMEOUT,
    )?;
    Ok(parse::parse(&out.result, Utc::now()))
}

/// 读取一次额度并更新缓存、通知前端与托盘。`force` 为假时遵守最小间隔；正在读取时直接返回当前缓存
pub fn refresh(app: &AppHandle, force: bool) -> QuotaSnapshot {
    let q = app.state::<QuotaState>();
    {
        let mut last = q.last_try.lock().unwrap_or_else(|e| e.into_inner());
        let snap = q.snapshot();
        if snap.refreshing || (!force && last.is_some_and(|t| t.elapsed() < MIN_GAP)) {
            return snap;
        }
        if force && last.is_some_and(|t| t.elapsed() < Duration::from_secs(3)) {
            return snap;
        }
        *last = Some(Instant::now());
    }
    let _ = app.emit(EVT_QUOTA_UPDATED, q.update(|s| s.refreshing = true));

    let state = app.state::<AppState>();
    let result = (|| {
        let cfg = state.config();
        let claude = cli::find_claude(cfg.insights.claude_path.as_deref())?;
        let cwd = state.db_path.parent().map(Path::to_path_buf).unwrap_or_else(std::env::temp_dir);
        read_usage(&claude, &cwd)
    })();

    let now = fmt_ts(&Utc::now());
    let snap = q.update(|s| {
        s.refreshing = false;
        match result {
            Ok(Parsed::Windows(w)) => {
                s.windows = w;
                s.fetched_at = Some(now);
                s.note = None;
                s.error = None;
            }
            Ok(Parsed::NoQuota(msg)) => {
                s.windows.clear();
                s.fetched_at = Some(now);
                s.note = Some(if msg.is_empty() { "Claude Code 没有返回额度信息".into() } else { msg });
                s.error = None;
            }
            // 读取失败时保留上次的读数，界面同时显示错误
            Ok(Parsed::Denied(msg)) => s.error = Some(format!("Claude Code 账号不可用：{msg}")),
            Err(e) => s.error = Some(format!("{e:#}")),
        }
    });
    if snap.error.is_none()
        && let Err(e) = save(&state.conn(), &snap)
    {
        eprintln!("保存额度读数失败: {e:#}");
    }
    let _ = app.emit(EVT_QUOTA_UPDATED, &snap);
    tray::refresh_tooltip(app);
    snap
}

/// 后台是否该读取：从未读过；或距上次已过 BACKGROUND_GAP 且有新用量；或读数超过 STALE_AFTER；
/// 或某个窗口的重置时间已过
pub fn background_due(
    since_last_try: Option<Duration>,
    fetched_at: Option<&str>,
    latest_usage: Option<&str>,
    windows: &[QuotaWindow],
    now: DateTime<Utc>,
) -> bool {
    let Some(since) = since_last_try else { return true };
    if since < BACKGROUND_GAP {
        return false;
    }
    let Some(fetched) = fetched_at else { return true };
    // 时间均为 RFC 3339 UTC 毫秒格式，可直接按字符串比较
    let new_usage = latest_usage.is_some_and(|u| u > fetched);
    let reset_passed = windows.iter().filter_map(|w| w.resets_at.as_deref()).any(|r| r < fmt_ts(&now).as_str());
    new_usage || reset_passed || since >= STALE_AFTER
}

/// 定时任务调用：模块开启且到期时在后台读取
pub fn maybe_refresh_in_background(app: &AppHandle) {
    let state = app.state::<AppState>();
    if !state.config().quota.enabled {
        return;
    }
    let q = app.state::<QuotaState>();
    let snap = q.snapshot();
    let latest: Option<String> =
        state.conn().query_row("SELECT MAX(ts) FROM usage_event", [], |r| r.get(0)).ok().flatten();
    if background_due(q.since_last_try(), snap.fetched_at.as_deref(), latest.as_deref(), &snap.windows, Utc::now()) {
        let app = app.clone();
        std::thread::spawn(move || {
            refresh(&app, false);
        });
    }
}

/// 托盘提示中的简短额度：`5h 42% · 周 91% · Fable 95%`
pub fn tooltip_text(snap: &QuotaSnapshot) -> Option<String> {
    let parts: Vec<String> = snap
        .windows
        .iter()
        .map(|w| {
            let name = match (w.kind.as_str(), w.scope.as_deref()) {
                ("session", _) => "5h".to_string(),
                (_, Some(s)) => s.to_string(),
                _ => "周".to_string(),
            };
            format!("{name} {:.0}%", w.used_percent)
        })
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// 设置中关闭时拒绝手动读取
pub fn ensure_enabled(state: &AppState) -> Result<()> {
    if state.config().quota.enabled { Ok(()) } else { Err(anyhow!("额度显示已在设置中关闭")) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(resets_at: Option<&str>) -> QuotaWindow {
        QuotaWindow {
            kind: "session".into(),
            scope: None,
            used_percent: 10.0,
            resets_at: resets_at.map(str::to_string),
            resets_text: None,
        }
    }

    #[test]
    fn background_schedule() {
        let now = DateTime::parse_from_rfc3339("2026-11-12T08:00:00Z").unwrap().with_timezone(&Utc);
        let fetched = Some("2026-11-12T07:40:00.000Z");
        let future = [w(Some("2026-11-12T09:00:00Z"))];
        let m = |n: u64| Some(Duration::from_secs(n * 60));
        assert!(background_due(None, None, None, &[], now), "从未读取");
        assert!(!background_due(m(5), fetched, Some("2026-11-12T07:59:00.000Z"), &future, now), "间隔未到");
        assert!(background_due(m(20), fetched, Some("2026-11-12T07:59:00.000Z"), &future, now), "有新用量");
        assert!(!background_due(m(20), fetched, Some("2026-11-12T07:00:00.000Z"), &future, now), "没有新用量");
        assert!(background_due(m(20), fetched, None, &[w(Some("2026-11-12T07:50:00Z"))], now), "重置时间已过");
        assert!(background_due(m(61), fetched, None, &future, now), "读数过旧");
    }

    #[test]
    fn tooltip() {
        let mut s = QuotaSnapshot::default();
        assert_eq!(tooltip_text(&s), None);
        s.windows = vec![
            w(None),
            QuotaWindow { kind: "week".into(), scope: None, used_percent: 91.0, resets_at: None, resets_text: None },
            QuotaWindow {
                kind: "week".into(),
                scope: Some("Fable".into()),
                used_percent: 95.4,
                resets_at: None,
                resets_text: None,
            },
        ];
        assert_eq!(tooltip_text(&s).as_deref(), Some("5h 10% · 周 91% · Fable 95%"));
    }

    #[test]
    fn saved_snapshot_roundtrip() {
        let conn = crate::db::open_in_memory().unwrap();
        assert!(load_saved(&conn).windows.is_empty());
        let mut s = QuotaSnapshot { fetched_at: Some("t".into()), ..Default::default() };
        s.windows.push(w(Some("2026-11-12T09:00:00Z")));
        s.error = Some("不保存".into());
        save(&conn, &s).unwrap();
        let back = load_saved(&conn);
        assert_eq!(back.windows, s.windows);
        assert_eq!(back.error, None);
    }

    /// 本机实际运行 `claude -p /usage`（不调用模型）：`cargo test real_usage -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_usage() {
        let claude = cli::find_claude(None).unwrap();
        let start = Instant::now();
        let parsed = read_usage(&claude, &std::env::temp_dir()).unwrap();
        println!("耗时 {:?}：{parsed:#?}", start.elapsed());
        assert!(matches!(parsed, Parsed::Windows(ref w) if !w.is_empty()));
    }
}
