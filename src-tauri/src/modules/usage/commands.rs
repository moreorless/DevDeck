use chrono::Local;
use tauri::State;

use super::query::{self, DailyModelUsage, Summary, UsageFilter};
use crate::state::{AppState, err};

#[tauri::command(async)]
pub fn get_daily_usage(state: State<'_, AppState>, filter: UsageFilter) -> Result<Vec<DailyModelUsage>, String> {
    query::daily_usage(&state.conn(), &state.pricing, &filter).map_err(err)
}

#[tauri::command(async)]
pub fn get_summary(state: State<'_, AppState>) -> Result<Summary, String> {
    query::summary(&state.conn(), &state.pricing, Local::now().date_naive()).map_err(err)
}

#[tauri::command(async)]
pub fn list_projects(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    query::list_projects(&state.conn()).map_err(err)
}

#[tauri::command(async)]
pub fn list_models(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    query::list_models(&state.conn()).map_err(err)
}
