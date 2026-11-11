use chrono::Utc;
use tauri::State;

use super::query::{self, DailyModelUsage, Summary, UsageFilter};
use crate::{
    state::{AppState, err},
    with_tz,
};

#[tauri::command(async)]
pub fn get_daily_usage(state: State<'_, AppState>, filter: UsageFilter) -> Result<Vec<DailyModelUsage>, String> {
    query::daily_usage(&state.conn(), &state.pricing(), &filter).map_err(err)
}

/// 按配置时区计算「今天」
pub fn today(state: &AppState) -> chrono::NaiveDate {
    with_tz!(state.tz(), |tz| Utc::now().with_timezone(tz).date_naive())
}

#[tauri::command(async)]
pub fn get_summary(state: State<'_, AppState>) -> Result<Summary, String> {
    query::summary(&state.conn(), &state.pricing(), today(&state)).map_err(err)
}

#[tauri::command(async)]
pub fn list_projects(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    query::list_projects(&state.conn()).map_err(err)
}

#[tauri::command(async)]
pub fn list_models(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    query::list_models(&state.conn()).map_err(err)
}

/// 按筛选条件导出 CSV 到 `path`（路径由前端保存对话框给出），返回写入的行数
#[tauri::command(async)]
pub fn export_csv(state: State<'_, AppState>, path: String, filter: UsageFilter) -> Result<usize, String> {
    let rows = query::daily_usage(&state.conn(), &state.pricing(), &filter).map_err(err)?;
    std::fs::write(&path, query::to_csv(&rows)).map_err(|e| format!("写入 {path} 失败：{e}"))?;
    Ok(rows.len())
}
