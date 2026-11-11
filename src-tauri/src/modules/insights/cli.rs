//! 调用 Claude Code CLI（`claude -p`）：查找可执行文件、带超时运行、解析 JSON 结果
//!
//! 约束：不弹控制台窗口（CREATE_NO_WINDOW）、超时强制结束、退出码与 stderr 上报。

use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;

/// 后台调用时关闭用户配置的 hooks，避免触发用户的自动化（通知、记忆等）
pub const NO_HOOKS_SETTINGS: &str = r#"{"disableAllHooks":true}"#;

/// 查找 `claude`：设置中指定 → PATH → `%USERPROFILE%\.local\bin\claude.exe`
pub fn find_claude(configured: Option<&str>) -> Result<PathBuf> {
    if let Some(p) = configured.map(str::trim).filter(|p| !p.is_empty()) {
        let p = PathBuf::from(p);
        return if p.is_file() { Ok(p) } else { Err(anyhow!("设置中指定的 claude 不存在：{}", p.display())) };
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            for name in ["claude.exe", "claude.cmd"] {
                let p = dir.join(name);
                if p.is_file() {
                    return Ok(p);
                }
            }
        }
    }
    if let Some(home) = std::env::var_os("USERPROFILE") {
        let p = PathBuf::from(home).join(".local").join("bin").join("claude.exe");
        if p.is_file() {
            return Ok(p);
        }
    }
    Err(anyhow!("找不到 claude 可执行文件：请确认已安装 Claude Code，或在设置中手动指定路径"))
}

#[derive(Debug, Deserialize)]
struct JsonResult {
    #[serde(default)]
    subtype: Option<String>,
    #[serde(default)]
    is_error: bool,
    #[serde(default)]
    result: Option<String>,
    #[serde(default)]
    total_cost_usd: Option<f64>,
}

#[derive(Debug)]
pub struct CliOutput {
    pub result: String,
    /// CLI 按 API 价格估算的费用
    pub cost_usd: Option<f64>,
}

/// 运行 `claude <args> --output-format json`，可选写入 stdin；超时强制结束
pub fn run_json(claude: &Path, args: &[&str], stdin: Option<&str>, cwd: &Path, timeout: Duration) -> Result<CliOutput> {
    let mut cmd = Command::new(claude);
    cmd.args(args)
        .args(["--output-format", "json"])
        .current_dir(cwd)
        .stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().with_context(|| format!("无法启动 {}", claude.display()))?;

    // stdin / stdout / stderr 各用一个线程，避免管道写满互相阻塞
    let writer = match (stdin, child.stdin.take()) {
        (Some(text), Some(mut pipe)) => {
            let text = text.to_string();
            Some(std::thread::spawn(move || {
                let _ = pipe.write_all(text.as_bytes());
            }))
        }
        _ => None,
    };
    let read_all = |mut r: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = r.read_to_end(&mut buf);
            String::from_utf8_lossy(&buf).into_owned()
        })
    };
    let out = read_all(Box::new(child.stdout.take().context("无法读取 stdout")?));
    let err = read_all(Box::new(child.stderr.take().context("无法读取 stderr")?));

    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(anyhow!("claude 运行超过 {} 分钟，已强制结束", timeout.as_secs() / 60));
        }
        std::thread::sleep(Duration::from_millis(300));
    };
    if let Some(w) = writer {
        let _ = w.join();
    }
    let stdout = out.join().unwrap_or_default();
    let stderr = err.join().unwrap_or_default();
    parse_output(status.code(), &stdout, &stderr)
}

fn parse_output(code: Option<i32>, stdout: &str, stderr: &str) -> Result<CliOutput> {
    let parsed: Option<JsonResult> = serde_json::from_str(stdout.trim()).ok();
    match parsed {
        Some(j) if code == Some(0) && !j.is_error && j.subtype.as_deref().is_none_or(|s| s == "success") => {
            Ok(CliOutput { result: j.result.unwrap_or_default(), cost_usd: j.total_cost_usd })
        }
        Some(j) => Err(anyhow!(
            "claude 执行失败（退出码 {}，{}）：{}",
            code.map_or("无".into(), |c| c.to_string()),
            j.subtype.unwrap_or_else(|| "error".into()),
            tail(j.result.as_deref().filter(|s| !s.is_empty()).unwrap_or(stderr))
        )),
        None => Err(anyhow!(
            "claude 执行失败（退出码 {}）：{}",
            code.map_or("无".into(), |c| c.to_string()),
            tail(if stderr.trim().is_empty() { stdout } else { stderr })
        )),
    }
}

/// 错误信息只保留末尾 600 字符
fn tail(s: &str) -> String {
    let s = s.trim();
    let n = s.chars().count();
    if n <= 600 { s.to_string() } else { format!("…{}", s.chars().skip(n - 600).collect::<String>()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_success_and_errors() {
        let ok = parse_output(
            Some(0),
            r#"{"type":"result","subtype":"success","is_error":false,"result":"done","total_cost_usd":1.5}"#,
            "",
        )
        .unwrap();
        assert_eq!(ok.result, "done");
        assert_eq!(ok.cost_usd, Some(1.5));

        let e =
            parse_output(Some(1), r#"{"subtype":"error_max_turns","is_error":true,"result":"boom"}"#, "").unwrap_err();
        assert!(format!("{e}").contains("boom"));

        let e = parse_output(Some(2), "not json", "auth failed").unwrap_err();
        assert!(format!("{e}").contains("auth failed") && format!("{e}").contains("退出码 2"));
    }

    #[test]
    fn configured_path_must_exist() {
        assert!(find_claude(Some(r"Z:\no\such\claude.exe")).is_err());
    }
}
