use tauri::{AppHandle, State};

use super::{QuotaSnapshot, QuotaState};
use crate::state::{AppState, err};

/// 当前缓存的额度读数（不触发读取）
#[tauri::command]
pub fn get_quota(q: State<'_, QuotaState>) -> QuotaSnapshot {
    q.snapshot()
}

/// 立即读取一次（运行 `claude -p /usage`，约 5 秒）
#[tauri::command(async)]
pub fn refresh_quota(app: AppHandle, state: State<'_, AppState>) -> Result<QuotaSnapshot, String> {
    super::ensure_enabled(&state).map_err(err)?;
    Ok(super::refresh(&app, true))
}
