use chrono::Local;
use tauri::State;

use super::query::{self, SessionPage, SessionQuery};
use crate::state::{AppState, err};

#[tauri::command(async)]
pub fn list_sessions(state: State<'_, AppState>, query: SessionQuery) -> Result<SessionPage, String> {
    query::list_sessions(&state.conn(), &state.pricing, &query, &Local).map_err(err)
}
