//! 平台差异集中在这里：Windows / macOS（Linux 按 xdg 约定兜底，未专门测试）

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// 当前系统：windows | macos | linux
pub const OS: &str = std::env::consts::OS;

/// 用户主目录：Windows 取 USERPROFILE，其他系统取 HOME
pub fn home_dir() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var).or_else(|| std::env::var_os("HOME")).map(PathBuf::from)
}

/// 用系统默认方式打开：目录在文件管理器中打开，文件用默认程序（.html 即浏览器）打开
pub fn open_path(path: &Path) -> Result<()> {
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    // 只是把路径交给系统打开，立即返回，不等待
    std::process::Command::new(program).arg(path).spawn().with_context(|| format!("无法打开 {}", path.display()))?;
    Ok(())
}

/// 界面展示用的主目录写法
pub fn home_label() -> &'static str {
    if cfg!(windows) { "%USERPROFILE%" } else { "~" }
}
