//! 定时任务：按配置间隔增量刷新；每分钟更新一次托盘提示（跨零点后「今日」随之切换），并检查是否该读取套餐额度

use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

use tauri::{AppHandle, Manager};

use crate::{claude_log::scanner::ScanMode, modules::quota, scan_service, state::AppState, tray};

const TICK: Duration = Duration::from_secs(10);
const TOOLTIP_EVERY: Duration = Duration::from_secs(60);

/// 距上次刷新是否已到间隔；间隔 0 表示关闭
pub fn refresh_due(interval_min: u32, since_last: Duration) -> bool {
    interval_min > 0 && since_last >= Duration::from_secs(u64::from(interval_min) * 60)
}

pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        // 启动时已有一次扫描，从现在开始计时
        let mut last_refresh = Instant::now();
        let mut last_tooltip = Instant::now();
        loop {
            std::thread::sleep(TICK);
            let state = app.state::<AppState>();
            if refresh_due(state.config().refresh_interval_min, last_refresh.elapsed()) {
                last_refresh = Instant::now();
                // 手动刷新正在进行时跳过本轮
                if !state.scanning.load(Ordering::SeqCst) {
                    if let Err(e) = scan_service::run_scan(&app, Some(ScanMode::Incremental)) {
                        eprintln!("定时刷新失败: {e:#}");
                    }
                }
            }
            if last_tooltip.elapsed() >= TOOLTIP_EVERY {
                last_tooltip = Instant::now();
                tray::refresh_tooltip(&app);
                quota::maybe_refresh_in_background(&app);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn due_logic() {
        assert!(!refresh_due(0, Duration::from_secs(99_999)), "0 表示关闭");
        assert!(!refresh_due(5, Duration::from_secs(299)));
        assert!(refresh_due(5, Duration::from_secs(300)));
        assert!(refresh_due(1, Duration::from_secs(61)));
    }
}
