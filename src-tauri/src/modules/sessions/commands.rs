use std::path::PathBuf;

use anyhow::{Result, anyhow};
use chrono::Local;
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use tauri::State;

use super::query::{self, SessionPage, SessionQuery};
use crate::{
    claude_log::transcript::{Item, read_transcript},
    state::{AppState, err},
};

#[tauri::command(async)]
pub fn list_sessions(state: State<'_, AppState>, query: SessionQuery) -> Result<SessionPage, String> {
    query::list_sessions(&state.conn(), &state.pricing, &query, &Local).map_err(err)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDetail {
    pub session_id: String,
    /// 子代理时为 agentId
    pub agent_id: Option<String>,
    pub project: Option<String>,
    pub file_path: String,
    pub items: Vec<Item>,
    pub bad_lines: usize,
}

struct SessionFile {
    file_path: PathBuf,
    project: Option<String>,
}

fn session_file(conn: &Connection, session_id: &str) -> Result<SessionFile> {
    let row: Option<(Option<String>, Option<String>)> = conn
        .query_row("SELECT file_path, project FROM sessions WHERE session_id = ?1", [session_id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()?;
    let (path, project) = row.ok_or_else(|| anyhow!("会话不存在：{session_id}"))?;
    let file_path = PathBuf::from(path.ok_or_else(|| anyhow!("会话缺少日志文件路径"))?);
    if !file_path.is_file() {
        return Err(anyhow!("会话日志文件已不存在：{}", file_path.display()));
    }
    Ok(SessionFile { file_path, project })
}

/// `<项目目录>/<sessionId>/subagents`
fn subagent_dir(main_file: &std::path::Path, session_id: &str) -> Option<PathBuf> {
    main_file.parent().map(|p| p.join(session_id).join("subagents"))
}

#[tauri::command(async)]
pub fn get_session(state: State<'_, AppState>, session_id: String) -> Result<SessionDetail, String> {
    (|| {
        let f = session_file(&state.conn(), &session_id)?;
        let sub = subagent_dir(&f.file_path, &session_id);
        let t = read_transcript(&f.file_path, sub.as_deref(), false)?;
        Ok(SessionDetail {
            session_id: session_id.clone(),
            agent_id: None,
            project: f.project,
            file_path: f.file_path.display().to_string(),
            items: t.items,
            bad_lines: t.bad_lines,
        })
    })()
    .map_err(err)
}

#[tauri::command(async)]
pub fn get_subagent_session(
    state: State<'_, AppState>,
    session_id: String,
    agent_id: String,
) -> Result<SessionDetail, String> {
    (|| {
        // agentId 拼进路径，只允许安全字符
        if agent_id.is_empty() || !agent_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err(anyhow!("非法的子代理 id：{agent_id}"));
        }
        let f = session_file(&state.conn(), &session_id)?;
        let dir = subagent_dir(&f.file_path, &session_id).ok_or_else(|| anyhow!("无法定位子代理目录"))?;
        let path = dir.join(format!("agent-{agent_id}.jsonl"));
        if !path.is_file() {
            return Err(anyhow!("子代理日志不存在：{}", path.display()));
        }
        let t = read_transcript(&path, Some(&dir), true)?;
        Ok(SessionDetail {
            session_id: session_id.clone(),
            agent_id: Some(agent_id.clone()),
            project: f.project,
            file_path: path.display().to_string(),
            items: t.items,
            bad_lines: t.bad_lines,
        })
    })()
    .map_err(err)
}

/// 在资源管理器中打开会话的项目目录。只接受索引中已知的会话，不接受任意路径
#[tauri::command(async)]
pub fn open_project_dir(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    (|| {
        let project: Option<String> = state
            .conn()
            .query_row("SELECT project FROM sessions WHERE session_id = ?1", [&session_id], |r| r.get(0))
            .optional()?
            .flatten();
        let dir = PathBuf::from(project.ok_or_else(|| anyhow!("会话没有记录项目目录"))?);
        if !dir.is_dir() {
            return Err(anyhow!("项目目录已不存在：{}", dir.display()));
        }
        std::process::Command::new("explorer").arg(&dir).spawn()?;
        Ok(())
    })()
    .map_err(err)
}
