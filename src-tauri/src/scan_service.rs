//! 扫描调度：后台执行扫描、写 scan_log、向前端发事件

use std::sync::atomic::Ordering;

use anyhow::{Result, anyhow};
use chrono::Utc;
use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{
    claude_log::{
        locate::{existing_dirs, list_log_files},
        scanner::{self, ScanMode, ScanStats, fmt_ts},
    },
    db,
    modules::insights,
    state::{AppState, err},
    tray, with_tz,
};

pub const EVT_UPDATED: &str = "data://updated";
pub const EVT_ERROR: &str = "data://error";
pub const EVT_PROGRESS: &str = "data://progress";

#[derive(Clone, Serialize)]
struct Progress {
    done: usize,
    total: usize,
}

/// 扫描标志的守卫：离开作用域时复位
struct ScanGuard<'a>(&'a AppState);
impl Drop for ScanGuard<'_> {
    fn drop(&mut self) {
        self.0.scanning.store(false, Ordering::SeqCst);
    }
}

/// 阻塞执行一次扫描。`mode` 为 None 时：库为空则全量，否则增量
pub fn run_scan(app: &AppHandle, mode: Option<ScanMode>) -> Result<ScanStats> {
    let state = app.state::<AppState>();
    if state.scanning.swap(true, Ordering::SeqCst) {
        return Err(anyhow!("正在扫描中，请稍后再试"));
    }
    let _guard = ScanGuard(&state);

    let result = scan_inner(app, &state, mode);
    match &result {
        Ok(stats) => {
            let _ = app.emit(EVT_UPDATED, stats);
            tray::refresh_tooltip(app);
            // 顺带归档新的 insights 报告（模块关闭时不读取）；失败不影响扫描结果
            if let Err(e) = insights::sync_if_enabled(app) {
                eprintln!("归档 insights 报告失败: {e:#}");
            }
        }
        Err(e) => {
            let _ = app.emit(EVT_ERROR, err(anyhow!("{e:#}")));
        }
    }
    result
}

fn scan_inner(app: &AppHandle, state: &AppState, mode: Option<ScanMode>) -> Result<ScanStats> {
    let mut conn = db::open(&state.db_path)?;
    // 未指定时：库为空或索引规则已升级则全量，否则增量
    let mode = mode.unwrap_or(
        if scanner::has_any_cursor(&conn)? && scanner::stored_index_version(&conn)? == scanner::INDEX_VERSION {
            ScanMode::Incremental
        } else {
            ScanMode::Full
        },
    );

    let mode_str = match mode {
        ScanMode::Incremental => "incremental",
        ScanMode::Full => "full",
    };
    conn.execute(
        "INSERT INTO scan_log (started_at, mode) VALUES (?1, ?2)",
        params![fmt_ts(&Utc::now()), mode_str],
    )?;
    let log_id = conn.last_insert_rowid();

    let result = (|| {
        let cfg = state.config();
        let candidates = cfg.candidate_dirs();
        let dirs = existing_dirs(&candidates);
        if dirs.is_empty() {
            let checked: Vec<String> = candidates.iter().map(|d| d.display().to_string()).collect();
            return Err(anyhow!("未找到 Claude 日志目录，已检查：{}", checked.join("；")));
        }
        let files = list_log_files(&dirs);
        let mut last_emit = 0;
        let mut progress = |done: usize, total: usize| {
            // 全量扫描才发进度；每 20 个文件发一次
            if mode == ScanMode::Full && (done - last_emit >= 20 || done == total) {
                last_emit = done;
                let _ = app.emit(EVT_PROGRESS, Progress { done, total });
            }
        };
        let stats = with_tz!(cfg.tz(), |tz| scanner::scan(&mut conn, &files, mode, tz, &mut progress))?;
        if mode == ScanMode::Full {
            scanner::save_index_version(&conn)?;
        }
        Ok(stats)
    })();

    match &result {
        Ok(s) => conn.execute(
            "UPDATE scan_log SET finished_at = ?1, files_total = ?2, files_read = ?3, bad_lines = ?4, ok = 1 WHERE id = ?5",
            params![fmt_ts(&Utc::now()), s.files_total as i64, s.files_read as i64, s.bad_lines as i64, log_id],
        )?,
        Err(e) => conn.execute(
            "UPDATE scan_log SET finished_at = ?1, ok = 0, error = ?2 WHERE id = ?3",
            params![fmt_ts(&Utc::now()), format!("{e:#}"), log_id],
        )?,
    };
    result
}

/// 启动时在后台执行一次扫描
pub fn spawn_startup_scan(app: AppHandle) {
    std::thread::spawn(move || {
        if let Err(e) = run_scan(&app, None) {
            eprintln!("启动扫描失败: {e:#}");
        }
    });
}

/// 后台执行一次扫描；若正在扫描则等它结束后再执行（最多等 10 分钟），保证本次请求不丢
pub fn spawn_scan(app: AppHandle, mode: ScanMode) {
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        for _ in 0..1200 {
            if !state.scanning.load(Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        if let Err(e) = run_scan(&app, Some(mode)) {
            eprintln!("后台扫描失败: {e:#}");
        }
    });
}

#[tauri::command]
pub async fn refresh(app: AppHandle, mode: ScanMode) -> Result<ScanStats, String> {
    tauri::async_runtime::spawn_blocking(move || run_scan(&app, Some(mode)))
        .await
        .map_err(|e| e.to_string())?
        .map_err(err)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LastScan {
    pub started_at: String,
    pub finished_at: Option<String>,
    pub mode: String,
    pub files_total: Option<i64>,
    pub bad_lines: Option<i64>,
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStatus {
    pub scanning: bool,
    pub last_scan: Option<LastScan>,
    pub checked_dirs: Vec<String>,
    pub found_dirs: Vec<String>,
    pub session_count: i64,
    pub usage_count: i64,
    pub pricing_checked_at: String,
}

#[tauri::command(async)]
pub fn get_scan_status(state: State<'_, AppState>) -> Result<ScanStatus, String> {
    let conn = state.conn();
    let last_scan = conn
        .query_row(
            "SELECT started_at, finished_at, mode, files_total, bad_lines, ok, error FROM scan_log ORDER BY id DESC LIMIT 1",
            [],
            |r| {
                Ok(LastScan {
                    started_at: r.get(0)?,
                    finished_at: r.get(1)?,
                    mode: r.get(2)?,
                    files_total: r.get(3)?,
                    bad_lines: r.get(4)?,
                    ok: r.get(5)?,
                    error: r.get(6)?,
                })
            },
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let count = |sql: &str| conn.query_row(sql, [], |r| r.get::<_, i64>(0)).map_err(|e| e.to_string());
    let candidates = state.config().candidate_dirs();
    Ok(ScanStatus {
        scanning: state.scanning.load(Ordering::SeqCst),
        last_scan,
        found_dirs: existing_dirs(&candidates).iter().map(|d| d.display().to_string()).collect(),
        checked_dirs: candidates.iter().map(|d| d.display().to_string()).collect(),
        session_count: count("SELECT COUNT(*) FROM sessions WHERE prompt_count > 0")?,
        usage_count: count("SELECT COUNT(*) FROM usage_event")?,
        pricing_checked_at: state.pricing().checked_at.clone(),
    })
}
