use tauri::{AppHandle, Manager, Runtime};

/// 主窗口标签，与 tauri.conf.json 中的 label 一致
pub const MAIN: &str = "main";

/// 显示并聚焦主窗口（包括从最小化恢复）
pub fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window(MAIN) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// 主窗口可见则隐藏，否则显示
pub fn toggle_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window(MAIN) {
        let visible = w.is_visible().unwrap_or(false);
        let minimized = w.is_minimized().unwrap_or(false);
        if visible && !minimized {
            let _ = w.hide();
        } else {
            show_main(app);
        }
    }
}
