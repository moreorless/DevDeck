use anyhow::anyhow;
use tauri::{AppHandle, Emitter, State};

use super::{
    EVT_INSIGHTS_UPDATED, archive, cli,
    job::{self, InsightsJob, JobKind, JobStatus},
};
use crate::state::{AppState, err};

fn ensure_enabled(state: &AppState) -> Result<(), String> {
    if state.config().insights.enabled { Ok(()) } else { Err(err(anyhow!("Insights 模块未开启"))) }
}

#[tauri::command(async)]
pub fn list_insights(state: State<'_, AppState>) -> Result<Vec<archive::InsightsReport>, String> {
    archive::list(&state.conn()).map_err(err)
}

/// 立即检查新报告（页面上的「检查新报告」）
#[tauri::command(async)]
pub fn sync_insights(app: AppHandle, state: State<'_, AppState>) -> Result<usize, String> {
    ensure_enabled(&state)?;
    super::sync_if_enabled(&app).map_err(err)
}

/// 用于 DevDeck 内显示的报告 HTML（已去掉外链并加 CSP）；`zh` 为真时取中文译文
#[tauri::command(async)]
pub fn get_insight_html(state: State<'_, AppState>, id: i64, zh: bool) -> Result<String, String> {
    (|| {
        let path = archive::archived_path(&state.conn(), &state.insights_dir, id, zh)?;
        let bytes = std::fs::read(&path)?;
        Ok(archive::prepare_for_view(&String::from_utf8_lossy(&bytes)))
    })()
    .map_err(err)
}

/// 用系统默认浏览器打开归档文件（原文或译文）
#[tauri::command(async)]
pub fn open_insight_external(state: State<'_, AppState>, id: i64, zh: bool) -> Result<(), String> {
    (|| {
        let path = archive::archived_path(&state.conn(), &state.insights_dir, id, zh)?;
        crate::platform::open_path(&path)?;
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

/// 后台生成新报告（按设置决定是否随后翻译）；立即返回，进度通过 `insights://job` 事件通知
#[tauri::command(async)]
pub fn generate_insights(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    ensure_enabled(&state)?;
    let translate = state.config().insights.translate;
    job::start(&app, JobKind::Generate { translate }).map_err(err)
}

/// 后台把已归档的报告翻译为中文
#[tauri::command(async)]
pub fn translate_insight(app: AppHandle, state: State<'_, AppState>, id: i64) -> Result<(), String> {
    ensure_enabled(&state)?;
    job::start(&app, JobKind::Translate { id }).map_err(err)
}

#[tauri::command]
pub fn get_insights_job(job: State<'_, InsightsJob>) -> JobStatus {
    job.status()
}

/// 当前会使用的 claude 可执行文件路径（设置页展示）
#[tauri::command(async)]
pub fn detect_claude(state: State<'_, AppState>) -> Result<String, String> {
    cli::find_claude(state.config().insights.claude_path.as_deref()).map(|p| p.display().to_string()).map_err(err)
}
