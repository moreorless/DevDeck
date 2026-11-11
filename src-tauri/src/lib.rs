mod tray;
mod window;

use tauri::WindowEvent;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // 单实例插件必须最先注册：重复启动时激活已有窗口
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            window::show_main(app);
        }))
        .setup(|app| {
            tray::init(app.handle())?;
            Ok(())
        })
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
