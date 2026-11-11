//! 托盘：Windows 左键显示/隐藏主窗口、右键菜单；macOS 菜单栏图标点击弹出菜单（系统惯例）；悬停显示今日用量

use anyhow::Result;
use tauri::{
    AppHandle, Manager, Wry,
    image::Image,
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
        .show_menu_on_left_click(cfg!(target_os = "macos"))
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
            if cfg!(target_os = "macos") {
                return;
            }
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                window::toggle_main(tray.app_handle());
            }
        });
    if let Some(icon) = tray_icon(app) {
        // macOS 用模板图标：系统按菜单栏的浅色 / 深色自动着色
        builder = builder.icon(icon).icon_as_template(cfg!(target_os = "macos"));
    } else if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    app.manage(TrayItems { autostart });
    refresh_tooltip(app);
    Ok(())
}

/// 托盘简化图标（源文件 icons/tray/source.svg），按 16px 网格绘制，预渲染多个尺寸
const TRAY_ICONS: [(u32, &[u8]); 6] = [
    (16, include_bytes!("../icons/tray/16x16.png")),
    (20, include_bytes!("../icons/tray/20x20.png")),
    (24, include_bytes!("../icons/tray/24x24.png")),
    (32, include_bytes!("../icons/tray/32x32.png")),
    (40, include_bytes!("../icons/tray/40x40.png")),
    (48, include_bytes!("../icons/tray/48x48.png")),
];

/// 托盘图标逻辑尺寸 16px × 缩放比例，取不小于它的最小预渲染尺寸（避免系统放大发糊）
pub fn pick_tray_size(scale: f64) -> u32 {
    let want = (16.0 * scale).round() as u32;
    TRAY_ICONS.iter().map(|(s, _)| *s).find(|s| *s >= want).unwrap_or(48)
}

/// macOS 菜单栏模板图标（源文件 icons/tray/macos/source.svg，18pt 的 2x 渲染）
const MACOS_TEMPLATE: &[u8] = include_bytes!("../icons/tray/macos/template@2x.png");

fn tray_icon(app: &AppHandle) -> Option<Image<'static>> {
    if cfg!(target_os = "macos") {
        return Image::from_bytes(MACOS_TEMPLATE).ok();
    }
    let scale = app.primary_monitor().ok().flatten().map(|m| m.scale_factor()).unwrap_or(1.0);
    let size = pick_tray_size(scale);
    let bytes = TRAY_ICONS.iter().find(|(s, _)| *s == size)?.1;
    Image::from_bytes(bytes).ok()
}

/// 显示缩放变化时换成对应尺寸的托盘图标（macOS 模板图标由系统缩放，无需处理）
pub fn refresh_icon(app: &AppHandle) {
    if cfg!(target_os = "macos") {
        return;
    }
    if let (Some(tray), Some(icon)) = (app.tray_by_id(TRAY_ID), tray_icon(app)) {
        let _ = tray.set_icon(Some(icon));
    }
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

#[cfg(test)]
mod tests {
    use super::pick_tray_size;

    #[test]
    fn tray_size_follows_scale() {
        assert_eq!(pick_tray_size(1.0), 16);
        assert_eq!(pick_tray_size(1.25), 20);
        assert_eq!(pick_tray_size(1.5), 24);
        assert_eq!(pick_tray_size(1.75), 32);
        assert_eq!(pick_tray_size(2.0), 32);
        assert_eq!(pick_tray_size(2.5), 40);
        assert_eq!(pick_tray_size(4.0), 48);
    }
}
