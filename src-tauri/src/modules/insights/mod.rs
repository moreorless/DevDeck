//! Insights 模块：归档并查看 Claude Code 原生 `/insights` 报告（默认关闭）

pub mod archive;
pub mod cli;
pub mod commands;
pub mod job;
pub mod translate;

use anyhow::Result;
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

/// 有新报告归档、或报告被删除时通知前端
pub const EVT_INSIGHTS_UPDATED: &str = "insights://updated";

/// 模块开启时归档新报告，返回新增份数；关闭时不读取 usage-data/
pub fn sync_if_enabled(app: &AppHandle) -> Result<usize> {
    let state = app.state::<AppState>();
    let cfg = state.config();
    if !cfg.insights.enabled {
        return Ok(0);
    }
    let added = archive::sync(&state.conn(), &cfg.candidate_dirs(), &state.insights_dir)?.len();
    if added > 0 {
        let _ = app.emit(EVT_INSIGHTS_UPDATED, added);
    }
    Ok(added)
}

/// 后台归档（开启模块时、扫描完成后调用）
pub fn spawn_sync(app: AppHandle) {
    std::thread::spawn(move || {
        if let Err(e) = sync_if_enabled(&app) {
            eprintln!("归档 insights 报告失败: {e:#}");
        }
    });
}
