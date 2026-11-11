//! 应用配置：保存在 `<app_data_dir>/config.json`，由 Rust 侧统一持有（调度、扫描、托盘都要读）

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

use crate::{claude_log::locate, modules::usage::pricing::Rates};

/// 允许的自动刷新间隔（分钟），0 表示关闭
pub const REFRESH_CHOICES: [u32; 5] = [0, 1, 5, 15, 30];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppConfig {
    /// 在自动检测之外追加的 Claude 配置目录
    pub claude_dirs: Vec<String>,
    /// IANA 时区名；None 表示跟随系统
    pub timezone: Option<String>,
    pub refresh_interval_min: u32,
    /// 按模型覆盖单价（键为归一化后的模型名）
    pub pricing_overrides: BTreeMap<String, Rates>,
    /// 开机自启：以系统实际注册状态为准，这里只做展示
    pub autostart: bool,
    pub close_to_tray: bool,
    /// system | light | dark
    pub theme: String,
    pub insights: InsightsConfig,
}

/// Insights 模块配置；默认关闭
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InsightsConfig {
    pub enabled: bool,
    /// 手动指定的 claude 可执行文件；为空时自动查找
    pub claude_path: Option<String>,
    /// 生成后自动翻译为简体中文
    pub translate: bool,
}

impl Default for InsightsConfig {
    fn default() -> Self {
        Self { enabled: false, claude_path: None, translate: true }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            claude_dirs: Vec::new(),
            timezone: None,
            refresh_interval_min: 5,
            pricing_overrides: BTreeMap::new(),
            autostart: false,
            close_to_tray: true,
            theme: "system".into(),
            insights: InsightsConfig::default(),
        }
    }
}

impl AppConfig {
    /// 读取配置；文件不存在用默认值，内容损坏时报错（不静默覆盖用户配置）
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(s) => {
                let cfg: Self = serde_json::from_str(&s).with_context(|| format!("配置文件格式错误：{}", path.display()))?;
                Ok(cfg.normalized())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("无法读取配置 {}", path.display())),
        }
    }

    /// 先写临时文件再改名，避免写一半时崩溃损坏配置
    pub fn save(&self, path: &Path) -> Result<()> {
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// 校验用户提交的配置
    pub fn validate(&self) -> Result<()> {
        if !REFRESH_CHOICES.contains(&self.refresh_interval_min) {
            return Err(anyhow!("不支持的刷新间隔：{} 分钟", self.refresh_interval_min));
        }
        if let Some(tz) = &self.timezone {
            tz.parse::<chrono_tz::Tz>().map_err(|_| anyhow!("无法识别的时区：{tz}"))?;
        }
        if !matches!(self.theme.as_str(), "system" | "light" | "dark") {
            return Err(anyhow!("不支持的主题：{}", self.theme));
        }
        for (model, r) in &self.pricing_overrides {
            let vals = [r.input, r.output, r.cache_write_5m, r.cache_write_1h, r.cache_read];
            if vals.iter().any(|v| !v.is_finite() || *v < 0.0) {
                return Err(anyhow!("{model} 的单价必须是非负数"));
            }
        }
        Ok(())
    }

    /// 去掉空白目录、重复目录与空时区
    fn normalized(mut self) -> Self {
        let mut seen = Vec::new();
        for d in self.claude_dirs.drain(..).map(|d| d.trim().to_string()).filter(|d| !d.is_empty()) {
            if !seen.contains(&d) {
                seen.push(d);
            }
        }
        self.claude_dirs = seen;
        self.timezone = self.timezone.filter(|t| !t.trim().is_empty());
        self.insights.claude_path =
            self.insights.claude_path.map(|p| p.trim().to_string()).filter(|p| !p.is_empty());
        self
    }

    pub fn normalize(self) -> Self {
        self.normalized()
    }

    /// 要检查的 Claude 目录：自动检测的候选 + 用户追加的，去重
    pub fn candidate_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = locate::candidate_dirs();
        for d in &self.claude_dirs {
            let p = PathBuf::from(d);
            if !dirs.contains(&p) {
                dirs.push(p);
            }
        }
        dirs
    }

    pub fn tz(&self) -> AppTz {
        match self.timezone.as_deref().and_then(|t| t.parse::<chrono_tz::Tz>().ok()) {
            Some(tz) => AppTz::Named(tz),
            None => AppTz::Local,
        }
    }
}

/// 生效的时区：跟随系统或指定 IANA 时区
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppTz {
    Local,
    Named(chrono_tz::Tz),
}

/// 按生效时区执行：`with_tz!(cfg.tz(), |tz| scan(..., tz))`，tz 为 `&impl TimeZone`
#[macro_export]
macro_rules! with_tz {
    ($tz:expr, |$t:ident| $body:expr) => {
        match $tz {
            $crate::config::AppTz::Local => {
                let $t = &chrono::Local;
                $body
            }
            $crate::config::AppTz::Named(z) => {
                let $t = &z;
                $body
            }
        }
    };
}

/// 常用时区放在前面，其余按字母序
pub fn timezone_names() -> Vec<String> {
    const COMMON: [&str; 8] = [
        "Asia/Shanghai",
        "Asia/Tokyo",
        "Asia/Singapore",
        "Europe/London",
        "Europe/Berlin",
        "America/New_York",
        "America/Denver",
        "America/Los_Angeles",
    ];
    let mut out: Vec<String> = COMMON.iter().map(|s| s.to_string()).collect();
    out.extend(chrono_tz::TZ_VARIANTS.iter().map(|t| t.name().to_string()).filter(|n| !COMMON.contains(&n.as_str())));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_uses_defaults_and_roundtrips() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.json");
        let cfg = AppConfig::load(&path).unwrap();
        assert_eq!(cfg, AppConfig::default());

        let mut c = cfg.clone();
        c.timezone = Some("Asia/Shanghai".into());
        c.refresh_interval_min = 15;
        c.pricing_overrides.insert(
            "my-model".into(),
            Rates { input: 1.0, cache_write_5m: 1.25, cache_write_1h: 2.0, cache_read: 0.1, output: 5.0 },
        );
        c.save(&path).unwrap();
        assert_eq!(AppConfig::load(&path).unwrap(), c);
    }

    #[test]
    fn partial_file_fills_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.json");
        std::fs::write(&path, r#"{"theme":"dark","claudeDirs":[" D:\\a ","","D:\\a"]}"#).unwrap();
        let c = AppConfig::load(&path).unwrap();
        assert_eq!(c.theme, "dark");
        assert_eq!(c.refresh_interval_min, 5);
        assert_eq!(c.claude_dirs, vec![r"D:\a".to_string()], "去空白、去空、去重");
    }

    #[test]
    fn corrupt_file_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.json");
        std::fs::write(&path, "{not json").unwrap();
        assert!(AppConfig::load(&path).is_err());
    }

    #[test]
    fn validation() {
        let ok = AppConfig::default();
        assert!(ok.validate().is_ok());
        assert!(AppConfig { refresh_interval_min: 7, ..ok.clone() }.validate().is_err());
        assert!(AppConfig { timezone: Some("Mars/Base".into()), ..ok.clone() }.validate().is_err());
        assert!(AppConfig { theme: "pink".into(), ..ok.clone() }.validate().is_err());
        let mut neg = ok.clone();
        neg.pricing_overrides.insert(
            "m".into(),
            Rates { input: -1.0, cache_write_5m: 0.0, cache_write_1h: 0.0, cache_read: 0.0, output: 0.0 },
        );
        assert!(neg.validate().is_err());
    }

    #[test]
    fn timezone_resolution() {
        assert_eq!(AppConfig::default().tz(), AppTz::Local);
        let c = AppConfig { timezone: Some("Asia/Shanghai".into()), ..Default::default() };
        assert_eq!(c.tz(), AppTz::Named(chrono_tz::Asia::Shanghai));
        let names = timezone_names();
        assert_eq!(names[0], "Asia/Shanghai");
        assert!(names.len() > 300);
        assert_eq!(names.iter().filter(|n| *n == "Asia/Shanghai").count(), 1);
    }
}
