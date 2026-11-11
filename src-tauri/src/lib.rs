mod claude_log;
mod db;
mod modules;
mod scan_service;
mod state;
mod tray;
mod window;

use tauri::{Manager, WindowEvent};

use crate::{
    modules::{sessions, usage},
    state::AppState,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // 单实例插件必须最先注册：重复启动时激活已有窗口
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            window::show_main(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();
            app.manage(AppState::init(&handle)?);
            tray::init(&handle)?;
            scan_service::spawn_startup_scan(handle);
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
        ])
        .on_window_event(|win, event| {
            // 关闭主窗口时隐藏到托盘，真正退出走托盘菜单。M5 接入 closeToTray 配置项
            if let WindowEvent::CloseRequested { api, .. } = event {
                if win.label() == window::MAIN {
                    api.prevent_close();
                    let _ = win.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
