//! 定位 Claude Code 日志目录与 jsonl 文件

use std::path::{Path, PathBuf};

/// 一个待扫描的日志文件
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogFile {
    pub path: PathBuf,
    /// 所属会话：主文件取文件名，子代理文件取上级会话目录名
    pub session_id: String,
    pub is_subagent: bool,
    /// `projects/` 下的目录名，cwd 缺失时作为项目名兜底
    pub project_dir: String,
    /// 仅主文件有意义：是否存在 `<sessionId>/subagents/`
    pub has_subagents: bool,
}

/// 默认的 Claude 配置目录候选：`CLAUDE_CONFIG_DIR`（逗号分隔）→ `~/.claude` → `~/.config/claude`
pub fn candidate_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(v) = std::env::var("CLAUDE_CONFIG_DIR") {
        dirs.extend(v.split(',').map(str::trim).filter(|s| !s.is_empty()).map(PathBuf::from));
    }
    if dirs.is_empty() {
        if let Some(home) = crate::platform::home_dir() {
            dirs.push(home.join(".claude"));
            dirs.push(home.join(".config").join("claude"));
        }
    }
    dirs
}

/// 候选目录中实际存在 `projects/` 的那些
pub fn existing_dirs(candidates: &[PathBuf]) -> Vec<PathBuf> {
    candidates.iter().filter(|d| d.join("projects").is_dir()).cloned().collect()
}

/// 列出所有会话文件：`projects/*/*.jsonl` 与 `projects/*/<sid>/subagents/*.jsonl`
pub fn list_log_files(claude_dirs: &[PathBuf]) -> Vec<LogFile> {
    let mut out = Vec::new();
    for root in claude_dirs {
        for project in read_dirs(&root.join("projects")) {
            let project_dir = file_name(&project);
            for entry in read_entries(&project) {
                if entry.is_file() && is_jsonl(&entry) {
                    let sid = stem(&entry);
                    let has_subagents = project.join(&sid).join("subagents").is_dir();
                    out.push(LogFile {
                        path: entry,
                        session_id: sid,
                        is_subagent: false,
                        project_dir: project_dir.clone(),
                        has_subagents,
                    });
                } else if entry.is_dir() {
                    let sid = file_name(&entry);
                    for sub in read_entries(&entry.join("subagents")) {
                        if sub.is_file() && is_jsonl(&sub) {
                            out.push(LogFile {
                                path: sub,
                                session_id: sid.clone(),
                                is_subagent: true,
                                project_dir: project_dir.clone(),
                                has_subagents: false,
                            });
                        }
                    }
                }
            }
        }
    }
    out
}

fn read_entries(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    v.sort();
    v
}

fn read_dirs(dir: &Path) -> Vec<PathBuf> {
    read_entries(dir).into_iter().filter(|p| p.is_dir()).collect()
}

fn is_jsonl(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("jsonl"))
}

fn stem(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}
