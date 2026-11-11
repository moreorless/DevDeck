//! 全局状态：数据库、价格表、扫描标志

use std::{
    path::PathBuf,
    sync::{Mutex, MutexGuard, atomic::AtomicBool},
};

use anyhow::{Context, Result};
use rusqlite::Connection;
use tauri::{AppHandle, Manager};

use crate::{db, modules::usage::pricing::PricingTable};

pub struct AppState {
    pub db_path: PathBuf,
    /// 查询用连接；扫描使用单独的连接（WAL 模式下读写互不阻塞）
    conn: Mutex<Connection>,
    pub pricing: PricingTable,
    pub scanning: AtomicBool,
}

impl AppState {
    pub fn init(app: &AppHandle) -> Result<Self> {
        let dir = app.path().app_data_dir().context("无法获取应用数据目录")?;
        std::fs::create_dir_all(&dir).with_context(|| format!("无法创建目录 {}", dir.display()))?;
        let db_path = dir.join("devdeck.db");
        let conn = db::open(&db_path)?;
        Ok(Self { db_path, conn: Mutex::new(conn), pricing: PricingTable::builtin(), scanning: AtomicBool::new(false) })
    }

    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// command 统一的错误格式：带上下文链的中文描述
pub fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}
