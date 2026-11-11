//! 全局状态：数据库、配置、价格表、扫描标志

use std::{
    path::PathBuf,
    sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, atomic::AtomicBool},
};

use anyhow::{Context, Result};
use rusqlite::Connection;
use tauri::{AppHandle, Manager};

use crate::{
    config::{AppConfig, AppTz},
    db,
    modules::usage::pricing::PricingTable,
};

pub struct AppState {
    pub db_path: PathBuf,
    config_path: PathBuf,
    /// 查询用连接；扫描使用单独的连接（WAL 模式下读写互不阻塞）
    conn: Mutex<Connection>,
    builtin_pricing: PricingTable,
    /// 内置价格 + 用户覆盖
    pricing: RwLock<PricingTable>,
    config: RwLock<AppConfig>,
    pub scanning: AtomicBool,
    /// 配置文件损坏时的错误（此时使用默认配置运行，设置页提示）
    pub config_error: Option<String>,
}

impl AppState {
    pub fn init(app: &AppHandle) -> Result<Self> {
        let dir = app.path().app_data_dir().context("无法获取应用数据目录")?;
        std::fs::create_dir_all(&dir).with_context(|| format!("无法创建目录 {}", dir.display()))?;
        let db_path = dir.join("devdeck.db");
        let config_path = dir.join("config.json");
        let conn = db::open(&db_path)?;

        let (config, config_error) = match AppConfig::load(&config_path) {
            Ok(c) => (c, None),
            Err(e) => (AppConfig::default(), Some(format!("{e:#}"))),
        };
        let builtin_pricing = PricingTable::builtin();
        let pricing = builtin_pricing.with_overrides(&config.pricing_overrides);
        Ok(Self {
            db_path,
            config_path,
            conn: Mutex::new(conn),
            builtin_pricing,
            pricing: RwLock::new(pricing),
            config: RwLock::new(config),
            scanning: AtomicBool::new(false),
            config_error,
        })
    }

    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn pricing(&self) -> RwLockReadGuard<'_, PricingTable> {
        self.pricing.read().unwrap_or_else(|e| e.into_inner())
    }

    pub fn builtin_pricing(&self) -> &PricingTable {
        &self.builtin_pricing
    }

    pub fn config(&self) -> AppConfig {
        self.config.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn tz(&self) -> AppTz {
        self.config.read().unwrap_or_else(|e| e.into_inner()).tz()
    }

    /// 保存并生效新配置（调用方负责校验）
    pub fn apply_config(&self, cfg: AppConfig) -> Result<()> {
        cfg.save(&self.config_path)?;
        *self.pricing.write().unwrap_or_else(|e| e.into_inner()) =
            self.builtin_pricing.with_overrides(&cfg.pricing_overrides);
        *self.config.write().unwrap_or_else(|e| e.into_inner()) = cfg;
        Ok(())
    }
}

/// command 统一的错误格式：带上下文链的中文描述
pub fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}
