mod claude_cli;
mod claude_log;
mod config;
mod db;
mod modules;
mod platform;
mod scan_service;
mod scheduler;
mod settings;
mod shared_format;
mod state;
mod tray;
mod window;

use tauri::{Manager, WindowEvent};

use crate::{
    modules::{insights, quota, sessions, usage},
    state::AppState,
};

/// 开机自启时附带的参数：启动后只驻留托盘，不弹出主窗口
const ARG_MINIMIZED: &str = "--minimized";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // 单实例插件必须最先注册：重复启动时激活已有窗口
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            window::show_main(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![ARG_MINIMIZED]),
        ))
        .setup(|app| {
            let handle = app.handle().clone();
            let state = AppState::init(&handle)?;
            let saved_quota = quota::load_saved(&state.conn());
            app.manage(state);
            app.manage(quota::QuotaState::new(saved_quota));
            app.manage(insights::job::InsightsJob::default());
            tray::init(&handle)?;
            // 主窗口初始隐藏（tauri.conf.json visible=false），非自启场景才显示，避免闪一下
            if !std::env::args().any(|a| a == ARG_MINIMIZED) {
                window::show_main(&handle);
            }
            scan_service::spawn_startup_scan(handle.clone());
            scheduler::spawn(handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            scan_service::refresh,
            scan_service::get_scan_status,
            usage::commands::get_daily_usage,
            usage::commands::get_summary,
            usage::commands::list_projects,
            usage::commands::list_models,
            usage::commands::export_csv,
            sessions::commands::list_sessions,
            sessions::commands::list_recent_projects,
            sessions::commands::get_session,
            sessions::commands::get_subagent_session,
            sessions::commands::open_project_dir,
            settings::get_settings,
            settings::set_config,
            settings::list_timezones,
            settings::get_pricing,
            settings::check_claude_dir,
            insights::commands::list_insights,
            insights::commands::sync_insights,
            insights::commands::get_insight_html,
            insights::commands::open_insight_external,
            insights::commands::delete_insight,
            insights::commands::clear_insights,
            insights::commands::generate_insights,
            insights::commands::translate_insight,
            insights::commands::get_insights_job,
            insights::commands::detect_claude,
            quota::commands::get_quota,
            quota::commands::refresh_quota,
        ])
        .on_window_event(|win, event| {
            // 显示缩放变化：托盘图标换成对应尺寸
            if let WindowEvent::ScaleFactorChanged { .. } = event {
                tray::refresh_icon(win.app_handle());
            }
            // 关闭主窗口：开启「最小化到托盘」时隐藏（macOS 仍留在程序坞，点击可恢复），否则正常关闭（最后一个窗口关闭即退出）
            if let WindowEvent::CloseRequested { api, .. } = event {
                let to_tray = win.app_handle().try_state::<AppState>().is_none_or(|s| s.config().close_to_tray);
                if win.label() == window::MAIN && to_tray {
                    api.prevent_close();
                    let _ = win.hide();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, _event| {
            // macOS：窗口隐藏到菜单栏后，点击程序坞图标重新显示主窗口
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = _event {
                window::show_main(_app);
            }
        });
}
