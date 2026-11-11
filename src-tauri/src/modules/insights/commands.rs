use anyhow::anyhow;
use tauri::{AppHandle, Emitter, State};

use super::{EVT_INSIGHTS_UPDATED, archive};
use crate::state::{AppState, err};

#[tauri::command(async)]
pub fn list_insights(state: State<'_, AppState>) -> Result<Vec<archive::InsightsReport>, String> {
    archive::list(&state.conn()).map_err(err)
}

/// 立即检查新报告（页面上的「检查新报告」）
#[tauri::command(async)]
pub fn sync_insights(app: AppHandle, state: State<'_, AppState>) -> Result<usize, String> {
    if !state.config().insights.enabled {
        return Err(err(anyhow!("Insights 模块未开启")));
    }
    super::sync_if_enabled(&app).map_err(err)
}

/// 用于 DevDeck 内显示的报告 HTML（已去掉外链并加 CSP）
#[tauri::command(async)]
pub fn get_insight_html(state: State<'_, AppState>, id: i64) -> Result<String, String> {
    (|| {
        let path = archive::archived_path(&state.conn(), &state.insights_dir, id)?;
        let bytes = std::fs::read(&path)?;
        Ok(archive::prepare_for_view(&String::from_utf8_lossy(&bytes)))
    })()
    .map_err(err)
}

/// 用系统默认浏览器打开归档的原文件
#[tauri::command(async)]
pub fn open_insight_external(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    (|| {
        let path = archive::archived_path(&state.conn(), &state.insights_dir, id)?;
        std::process::Command::new("explorer").arg(&path).spawn()?;
        Ok(())
    })()
    .map_err(err)
}

#[tauri::command(async)]
pub fn delete_insight(app: AppHandle, state: State<'_, AppState>, id: i64) -> Result<(), String> {
    archive::delete(&state.conn(), &state.insights_dir, id).map_err(err)?;
    let _ = app.emit(EVT_INSIGHTS_UPDATED, 0);
    Ok(())
}

/// 全部清除；模块关闭时也可用
#[tauri::command(async)]
pub fn clear_insights(app: AppHandle, state: State<'_, AppState>) -> Result<usize, String> {
    let n = archive::clear(&state.conn(), &state.insights_dir).map_err(err)?;
    let _ = app.emit(EVT_INSIGHTS_UPDATED, 0);
    Ok(n)
}
