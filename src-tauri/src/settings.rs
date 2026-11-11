//! 设置相关 commands

use std::collections::BTreeSet;

use anyhow::anyhow;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_autostart::ManagerExt;

use crate::{
    claude_log::scanner::{ScanMode, ScanStats},
    config::{AppConfig, timezone_names},
    modules::{
        insights,
        usage::{
            pricing::{LongContext, Rates, normalize_model},
            query,
        },
    },
    scan_service::{self, EVT_UPDATED},
    state::{AppState, err},
    platform, tray,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirStatus {
    pub path: String,
    pub exists: bool,
    /// 用户追加的（可删除）还是自动检测的
    pub custom: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub config: AppConfig,
    /// 配置文件损坏时的错误；此时使用默认配置
    pub config_error: Option<String>,
    pub dirs: Vec<DirStatus>,
    /// 系统时区名（用于「跟随系统」的说明）
    pub system_timezone: String,
    /// 当前系统：windows | macos | linux（界面中的路径写法等）
    pub platform: String,
    /// 界面展示用的主目录写法：`%USERPROFILE%` 或 `~`
    pub home_label: String,
}

#[tauri::command(async)]
pub fn get_settings(app: AppHandle, state: State<'_, AppState>) -> Result<SettingsView, String> {
    let mut config = state.config();
    config.autostart = app.autolaunch().is_enabled().unwrap_or(false);
    let dirs = config
        .candidate_dirs()
        .into_iter()
        .map(|p| {
            let s = p.display().to_string();
            DirStatus { exists: p.join("projects").is_dir(), custom: config.claude_dirs.contains(&s), path: s }
        })
        .collect();
    Ok(SettingsView {
        config,
        config_error: state.config_error.clone(),
        dirs,
        system_timezone: chrono::Local::now().format("%:z").to_string(),
        platform: platform::OS.to_string(),
        home_label: platform::home_label().to_string(),
    })
}

/// 保存配置并生效：时区变化 → 全量重建；日志目录变化 → 增量扫描；价格变化 → 通知前端重新读取
#[tauri::command(async)]
pub fn set_config(app: AppHandle, state: State<'_, AppState>, config: AppConfig) -> Result<AppConfig, String> {
    (|| {
        let mut new = config.normalize();
        new.validate()?;
        let old = state.config();

        let actual = app.autolaunch().is_enabled().unwrap_or(false);
        new.autostart = if new.autostart != actual { tray::set_autostart(&app, new.autostart)? } else { actual };

        state.apply_config(new.clone())?;

        if old.timezone != new.timezone {
            scan_service::spawn_scan(app.clone(), ScanMode::Full);
        } else if old.claude_dirs != new.claude_dirs {
            scan_service::spawn_scan(app.clone(), ScanMode::Incremental);
        } else if old.pricing_overrides != new.pricing_overrides {
            let _ = app.emit(EVT_UPDATED, ScanStats::default());
            tray::refresh_tooltip(&app);
        }
        // 开启 Insights 后立即归档已有报告
        if new.insights.enabled && !old.insights.enabled {
            insights::spawn_sync(app.clone());
        }
        Ok::<_, anyhow::Error>(new)
    })()
    .map_err(err)
}

#[tauri::command]
pub fn list_timezones() -> Vec<String> {
    timezone_names()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceRow {
    pub model: String,
    pub builtin: Option<Rates>,
    #[serde(rename = "override")]
    pub override_rates: Option<Rates>,
    /// 内置的长上下文分档（覆盖后不再生效）
    pub long_context: Option<LongContext>,
    /// 本机日志中出现过
    pub in_use: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PricingView {
    pub source: String,
    pub checked_at: String,
    pub rows: Vec<PriceRow>,
}

#[tauri::command(async)]
pub fn get_pricing(state: State<'_, AppState>) -> Result<PricingView, String> {
    let builtin = state.builtin_pricing();
    let overrides = state.config().pricing_overrides;
    let used: BTreeSet<String> =
        query::list_models(&state.conn()).map_err(err)?.iter().map(|m| normalize_model(m)).collect();

    let mut names: BTreeSet<String> = builtin.models.keys().cloned().collect();
    names.extend(overrides.keys().map(|m| normalize_model(m)));
    names.extend(used.iter().filter(|m| m.as_str() != "<synthetic>").cloned());

    let mut rows: Vec<PriceRow> = names
        .into_iter()
        .map(|m| {
            let b = builtin.models.get(&m);
            PriceRow {
                builtin: b.map(|p| p.base),
                long_context: b.and_then(|p| p.long_context.clone()),
                override_rates: overrides.iter().find(|(k, _)| normalize_model(k) == m).map(|(_, r)| *r),
                in_use: used.contains(&m),
                model: m,
            }
        })
        .collect();
    // 本机用到的模型排前面
    rows.sort_by(|a, b| b.in_use.cmp(&a.in_use).then_with(|| a.model.cmp(&b.model)));
    Ok(PricingView { source: builtin.source.clone(), checked_at: builtin.checked_at.clone(), rows })
}

/// 校验目录是否像 Claude 配置目录（含 projects/）
#[tauri::command]
pub fn check_claude_dir(path: String) -> Result<bool, String> {
    let p = std::path::PathBuf::from(&path);
    if !p.is_dir() {
        return Err(err(anyhow!("目录不存在：{path}")));
    }
    Ok(p.join("projects").is_dir())
}
