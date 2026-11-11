//! 托盘：左键显示/隐藏主窗口；右键菜单；悬停显示今日用量

use anyhow::Result;
use tauri::{
    AppHandle, Manager, Wry,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_autostart::ManagerExt;

use crate::{
    claude_log::scanner::ScanMode,
    modules::usage::{commands::today, query},
    scan_service,
    shared_format::{fmt_tokens, fmt_usd},
    state::AppState,
    window,
};

const TRAY_ID: &str = "main";
const MENU_SHOW: &str = "show";
const MENU_REFRESH: &str = "refresh";
const MENU_AUTOSTART: &str = "autostart";
const MENU_QUIT: &str = "quit";

/// 需要在运行中更新状态的菜单项
pub struct TrayItems {
    autostart: CheckMenuItem<Wry>,
}

pub fn init(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, MENU_SHOW, "打开主窗口", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, MENU_REFRESH, "立即刷新", true, None::<&str>)?;
    let enabled = app.autolaunch().is_enabled().unwrap_or(false);
    let autostart = CheckMenuItem::with_id(app, MENU_AUTOSTART, "开机自启", true, enabled, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, MENU_QUIT, "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &refresh, &autostart, &sep, &quit])?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("DevDeck")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            MENU_SHOW => window::show_main(app),
            MENU_REFRESH => scan_service::spawn_scan(app.clone(), ScanMode::Incremental),
            MENU_AUTOSTART => {
                let want = !app.autolaunch().is_enabled().unwrap_or(false);
                if let Err(e) = set_autostart(app, want) {
                    eprintln!("切换开机自启失败: {e:#}");
                }
            }
            MENU_QUIT => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                window::toggle_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    app.manage(TrayItems { autostart });
    refresh_tooltip(app);
    Ok(())
}

/// 开启/关闭开机自启，返回系统中的实际状态，并同步托盘勾选
pub fn set_autostart(app: &AppHandle, enable: bool) -> Result<bool> {
    let al = app.autolaunch();
    if enable {
        al.enable()?;
    } else {
        al.disable()?;
    }
    let actual = al.is_enabled()?;
    if let Some(items) = app.try_state::<TrayItems>() {
        let _ = items.autostart.set_checked(actual);
    }
    Ok(actual)
}

/// 悬停提示：`DevDeck · 今日 1.23M tokens · $4.56`
pub fn refresh_tooltip(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else { return };
    let text = match query::summary(&state.conn(), &state.pricing(), today(&state)) {
        Ok(s) => format!("DevDeck · 今日 {} tokens · {}", fmt_tokens(s.today.tokens), fmt_usd(s.today.cost_usd)),
        Err(_) => "DevDeck".to_string(),
    };
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(text));
    }
}
