//! Insights 报告归档：把 `<Claude 目录>/usage-data/report*.html` 复制到 `<app_data_dir>/insights/`
//!
//! 只依赖「是 HTML 文件 + 文件时间」；报告头部统计解析失败时留空，不影响归档。
//! `facets/`、`session-meta/` 不读取（格式未公开，且 facets 是逐会话摘要）。

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::claude_log::scanner::fmt_ts;

/// 超过这个大小的文件不当作报告（实测约 60–70KB）
const MAX_REPORT_BYTES: u64 = 20 * 1024 * 1024;

/// 报告头部统计：`1,842 messages across 228 sessions (526 total) | 2026-07-27 to 2026-11-11`
#[derive(Debug, Default, PartialEq)]
pub struct ReportHeader {
    pub subtitle: Option<String>,
    pub sessions_analyzed: Option<i64>,
    pub sessions_total: Option<i64>,
    pub period_start: Option<String>,
    pub period_end: Option<String>,
}

pub fn parse_header(html: &str) -> ReportHeader {
    let Some(subtitle) = subtitle_text(html) else {
        return ReportHeader::default();
    };
    let num = |s: &str| s.trim().replace(',', "").parse::<i64>().ok();
    let words: Vec<&str> = subtitle.split_whitespace().collect();
    let sessions_analyzed =
        words.iter().position(|w| *w == "sessions").and_then(|i| i.checked_sub(1)).and_then(|i| num(words[i]));
    let sessions_total =
        subtitle.split_once('(').and_then(|(_, rest)| rest.split_once("total")).and_then(|(n, _)| num(n));
    let (period_start, period_end) = match subtitle.split_once('|').and_then(|(_, p)| p.split_once(" to ")) {
        Some((a, b)) => (Some(a.trim().to_string()), Some(b.trim().to_string())),
        None => (None, None),
    };
    ReportHeader { subtitle: Some(subtitle), sessions_analyzed, sessions_total, period_start, period_end }
}

fn subtitle_text(html: &str) -> Option<String> {
    let start = html.find("class=\"subtitle\"")?;
    let rest = &html[start..];
    let text = &rest[rest.find('>')? + 1..];
    let text = text[..text.find('<')?].split_whitespace().collect::<Vec<_>>().join(" ");
    (!text.is_empty()).then_some(text)
}

/// `<Claude 目录>/usage-data/` 下的报告文件；带时间戳的副本排在 `report.html` 前面，
/// 内容相同时以副本的路径记录来源
pub fn list_report_files(claude_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in claude_dirs {
        let Ok(rd) = std::fs::read_dir(root.join("usage-data")) else {
            continue;
        };
        let mut files: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_ascii_lowercase();
                name.starts_with("report") && name.ends_with(".html") && p.is_file()
            })
            .collect();
        files.sort_by_key(|p| (p.file_name().is_some_and(|n| n.eq_ignore_ascii_case("report.html")), p.clone()));
        out.extend(files);
    }
    out
}

/// 归档新报告，返回新增报告的 id。已删除的报告留有哈希记录，不会再次归档
pub fn sync(conn: &Connection, claude_dirs: &[PathBuf], archive_dir: &Path) -> Result<Vec<i64>> {
    let files = list_report_files(claude_dirs);
    if files.is_empty() {
        return Ok(Vec::new());
    }
    std::fs::create_dir_all(archive_dir).with_context(|| format!("无法创建目录 {}", archive_dir.display()))?;
    let mut added = Vec::new();
    for path in files {
        let meta = std::fs::metadata(&path).with_context(|| format!("无法读取 {}", path.display()))?;
        if meta.len() > MAX_REPORT_BYTES {
            continue;
        }
        let bytes = std::fs::read(&path).with_context(|| format!("无法读取 {}", path.display()))?;
        let sha = hex(&Sha256::digest(&bytes));
        let known: Option<i64> =
            conn.query_row("SELECT id FROM insights_report WHERE sha256 = ?1", [&sha], |r| r.get(0)).optional()?;
        if known.is_some() {
            continue;
        }
        let generated: DateTime<Utc> = meta.modified().map(DateTime::from).unwrap_or_else(|_| Utc::now());
        let file_name = format!("{}-{}.html", generated.format("%Y%m%d-%H%M%S"), &sha[..12]);
        let dest = archive_dir.join(&file_name);
        std::fs::write(&dest, &bytes).with_context(|| format!("无法写入 {}", dest.display()))?;

        let h = parse_header(&String::from_utf8_lossy(&bytes));
        let inserted = conn.execute(
            "INSERT INTO insights_report (sha256, source_path, archived_file, generated_at, subtitle,
               sessions_analyzed, sessions_total, period_start, period_end, bytes, archived_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                sha,
                path.display().to_string(),
                file_name,
                fmt_ts(&generated),
                h.subtitle,
                h.sessions_analyzed,
                h.sessions_total,
                h.period_start,
                h.period_end,
                bytes.len() as i64,
                fmt_ts(&Utc::now()),
            ],
        );
        if let Err(e) = inserted {
            let _ = std::fs::remove_file(&dest);
            return Err(e.into());
        }
        added.push(conn.last_insert_rowid());
    }
    Ok(added)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightsReport {
    pub id: i64,
    pub generated_at: String,
    pub subtitle: Option<String>,
    pub sessions_analyzed: Option<i64>,
    pub sessions_total: Option<i64>,
    pub period_start: Option<String>,
    pub period_end: Option<String>,
    pub bytes: i64,
    pub source_path: String,
    /// 已有中文译文
    pub translated: bool,
    /// 从 DevDeck 生成与翻译的估算费用；手动运行 /insights 的报告为空
    pub cost_usd: Option<f64>,
}

/// 未删除的报告，新的在前
pub fn list(conn: &Connection) -> Result<Vec<InsightsReport>> {
    let mut stmt = conn.prepare(
        "SELECT id, generated_at, subtitle, sessions_analyzed, sessions_total, period_start, period_end, bytes, source_path,
           translated_file IS NOT NULL, cost_usd
         FROM insights_report WHERE deleted_at IS NULL ORDER BY generated_at DESC, id DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(InsightsReport {
            id: r.get(0)?,
            generated_at: r.get(1)?,
            subtitle: r.get(2)?,
            sessions_analyzed: r.get(3)?,
            sessions_total: r.get(4)?,
            period_start: r.get(5)?,
            period_end: r.get(6)?,
            bytes: r.get(7)?,
            source_path: r.get(8)?,
            translated: r.get(9)?,
            cost_usd: r.get(10)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// 归档文件（`zh` 为真时取中文译文）的完整路径；只接受库中记录的纯文件名
pub fn archived_path(conn: &Connection, archive_dir: &Path, id: i64, zh: bool) -> Result<PathBuf> {
    let sql = if zh {
        "SELECT translated_file FROM insights_report WHERE id = ?1 AND deleted_at IS NULL"
    } else {
        "SELECT archived_file FROM insights_report WHERE id = ?1 AND deleted_at IS NULL"
    };
    let name: Option<Option<String>> = conn.query_row(sql, [id], |r| r.get(0)).optional()?;
    let name = name
        .flatten()
        .ok_or_else(|| anyhow!(if zh { "该报告还没有中文译文" } else { "报告不存在或已删除" }))?;
    if name.contains(['/', '\\']) || name.contains("..") {
        return Err(anyhow!("非法的归档文件名：{name}"));
    }
    let path = archive_dir.join(name);
    if !path.is_file() {
        return Err(anyhow!("归档文件已不存在：{}", path.display()));
    }
    Ok(path)
}

/// 保存中文译文：`<原归档文件名>.zh.html`
pub fn save_translation(conn: &Connection, archive_dir: &Path, id: i64, html: &str) -> Result<()> {
    let src = archived_path(conn, archive_dir, id, false)?;
    let name = format!("{}.zh.html", src.file_stem().and_then(|s| s.to_str()).unwrap_or("report"));
    let dest = archive_dir.join(&name);
    std::fs::write(&dest, html).with_context(|| format!("无法写入 {}", dest.display()))?;
    conn.execute("UPDATE insights_report SET translated_file = ?1 WHERE id = ?2", params![name, id])?;
    Ok(())
}

/// 删除一份：删掉归档文件、译文和统计信息，只留哈希，避免下次扫描重新归档
pub fn delete(conn: &Connection, archive_dir: &Path, id: i64) -> Result<()> {
    let names: Option<(Option<String>, Option<String>)> = conn
        .query_row("SELECT archived_file, translated_file FROM insights_report WHERE id = ?1", [id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()?;
    if let Some((a, b)) = names {
        for name in [a, b].into_iter().flatten().filter(|n| !n.contains(['/', '\\'])) {
            remove_if_exists(&archive_dir.join(name))?;
        }
    }
    conn.execute(TOMBSTONE_SQL_BY_ID, params![fmt_ts(&Utc::now()), id])?;
    Ok(())
}

/// 全部清除：删除归档目录下所有报告文件，所有记录只留哈希
pub fn clear(conn: &Connection, archive_dir: &Path) -> Result<usize> {
    if let Ok(rd) = std::fs::read_dir(archive_dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("html")) {
                remove_if_exists(&p)?;
            }
        }
    }
    let n = conn.execute(TOMBSTONE_SQL_ALL, params![fmt_ts(&Utc::now())])?;
    Ok(n)
}

/// 删除后只留 sha256（及来源路径、时间），清空归档文件名、译文和报告统计
const TOMBSTONE_SQL_BY_ID: &str = "UPDATE insights_report SET deleted_at = ?1, archived_file = NULL,
    translated_file = NULL, subtitle = NULL,
    sessions_analyzed = NULL, sessions_total = NULL, period_start = NULL, period_end = NULL
    WHERE id = ?2 AND deleted_at IS NULL";
const TOMBSTONE_SQL_ALL: &str = "UPDATE insights_report SET deleted_at = ?1, archived_file = NULL,
    translated_file = NULL, subtitle = NULL,
    sessions_analyzed = NULL, sessions_total = NULL, period_start = NULL, period_end = NULL
    WHERE deleted_at IS NULL";

fn remove_if_exists(p: &Path) -> Result<()> {
    match std::fs::remove_file(p) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| format!("无法删除 {}", p.display())),
    }
}

/// 在 DevDeck 内显示前的处理：去掉外链样式表（报告外链 Google Fonts），
/// 并加上禁止一切外部请求的 CSP（DevDeck 自身不联网）；归档原文件不改动
pub fn prepare_for_view(html: &str) -> String {
    const CSP: &str = "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; \
        style-src 'unsafe-inline'; script-src 'unsafe-inline'; img-src data:; font-src data:\">";
    let mut out = String::with_capacity(html.len() + CSP.len());
    let mut rest = html;
    while let Some(i) = find_ci(rest, "<link") {
        out.push_str(&rest[..i]);
        let tag_end = rest[i..].find('>').map(|j| i + j + 1).unwrap_or(rest.len());
        let tag = &rest[i..tag_end];
        if !tag.contains("http:") && !tag.contains("https:") && !tag.contains("//") {
            out.push_str(tag);
        }
        rest = &rest[tag_end..];
    }
    out.push_str(rest);

    // CSP 尽量放在 <head> 最前面，确保先于其他内容生效
    match find_ci(&out, "<head").and_then(|i| out[i..].find('>').map(|j| i + j + 1)) {
        Some(pos) => out.insert_str(pos, CSP),
        None => out.insert_str(0, CSP),
    }
    out
}

fn find_ci(hay: &str, needle: &str) -> Option<usize> {
    hay.to_ascii_lowercase().find(needle)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    const SAMPLE: &str = r#"<html><head><title>Claude Code Insights</title>
<link href="https://fonts.googleapis.com/css2?family=Inter" rel="stylesheet">
<link rel="icon" href="data:,">
</head><body><h1>Claude Code Insights</h1>
    <p class="subtitle">1,842 messages across 228 sessions (526 total) | 2026-07-27 to 2026-11-11</p>
<script>function f(){}</script></body></html>"#;

    #[test]
    fn parses_header() {
        let h = parse_header(SAMPLE);
        assert_eq!(h.sessions_analyzed, Some(228));
        assert_eq!(h.sessions_total, Some(526));
        assert_eq!(h.period_start.as_deref(), Some("2026-07-27"));
        assert_eq!(h.period_end.as_deref(), Some("2026-11-11"));
        assert!(h.subtitle.unwrap().starts_with("1,842 messages"));
    }

    #[test]
    fn unknown_header_is_empty() {
        assert_eq!(parse_header("<html><body>其他格式</body></html>"), ReportHeader::default());
        let h = parse_header(r#"<p class="subtitle">新版格式</p>"#);
        assert_eq!(h.subtitle.as_deref(), Some("新版格式"));
        assert_eq!(h.sessions_analyzed, None);
    }

    #[test]
    fn view_strips_remote_links_and_adds_csp() {
        let v = prepare_for_view(SAMPLE);
        assert!(!v.contains("fonts.googleapis.com"));
        assert!(v.contains(r#"<link rel="icon" href="data:,">"#), "本地 link 保留");
        let head = v.find("<head>").unwrap();
        assert!(v[head..].starts_with("<head><meta http-equiv=\"Content-Security-Policy\""));
        assert!(v.contains("<script>function f(){}</script>"));
    }

    fn setup() -> (tempfile::TempDir, PathBuf, PathBuf, Connection) {
        let tmp = tempfile::tempdir().unwrap();
        let claude = tmp.path().join("claude");
        let usage = claude.join("usage-data");
        std::fs::create_dir_all(usage.join("facets")).unwrap();
        std::fs::write(usage.join("report.html"), SAMPLE).unwrap();
        std::fs::write(usage.join("report-2026-11-11-021545.html"), SAMPLE).unwrap();
        std::fs::write(usage.join("facets").join("x.json"), "{}").unwrap();
        std::fs::write(usage.join("notes.txt"), "x").unwrap();
        let archive = tmp.path().join("archive");
        (tmp, claude, archive, db::open_in_memory().unwrap())
    }

    #[test]
    fn sync_dedups_and_survives_source_cleanup() {
        let (_tmp, claude, archive, conn) = setup();
        let dirs = vec![claude.clone()];
        assert_eq!(sync(&conn, &dirs, &archive).unwrap().len(), 1, "内容相同的两份只归档一次");
        assert_eq!(sync(&conn, &dirs, &archive).unwrap().len(), 0);

        let list1 = list(&conn).unwrap();
        assert_eq!(list1.len(), 1);
        assert!(list1[0].source_path.ends_with("report-2026-11-11-021545.html"), "优先记录带时间戳的副本");
        assert_eq!(list1[0].sessions_analyzed, Some(228));

        // Claude Code 清理 usage-data 后归档仍可读
        std::fs::remove_dir_all(claude.join("usage-data")).unwrap();
        let p = archived_path(&conn, &archive, list1[0].id, false).unwrap();
        assert_eq!(std::fs::read_to_string(p).unwrap(), SAMPLE);

        // 新报告
        std::fs::create_dir_all(claude.join("usage-data")).unwrap();
        std::fs::write(claude.join("usage-data").join("report.html"), SAMPLE.replace("228", "300")).unwrap();
        assert_eq!(sync(&conn, &dirs, &archive).unwrap().len(), 1);
        assert_eq!(list(&conn).unwrap().len(), 2);
    }

    #[test]
    fn deleted_reports_are_not_reimported() {
        let (_tmp, claude, archive, conn) = setup();
        let dirs = vec![claude];
        sync(&conn, &dirs, &archive).unwrap();
        let id = list(&conn).unwrap()[0].id;
        assert!(archived_path(&conn, &archive, id, true).is_err(), "尚无译文");
        save_translation(&conn, &archive, id, "<html>中文</html>").unwrap();
        assert!(list(&conn).unwrap()[0].translated);
        let zh = archived_path(&conn, &archive, id, true).unwrap();
        assert!(zh.to_string_lossy().ends_with(".zh.html"));
        assert_eq!(std::fs::read_to_string(zh).unwrap(), "<html>中文</html>");

        delete(&conn, &archive, id).unwrap();
        assert!(list(&conn).unwrap().is_empty());
        assert!(archived_path(&conn, &archive, id, false).is_err());
        assert_eq!(std::fs::read_dir(&archive).unwrap().count(), 0);
        assert_eq!(sync(&conn, &dirs, &archive).unwrap().len(), 0, "删除后不再归档同一份");
    }

    #[test]
    fn clear_removes_all_files_and_details() {
        let (_tmp, claude, archive, conn) = setup();
        let dirs = vec![claude.clone()];
        sync(&conn, &dirs, &archive).unwrap();
        std::fs::write(claude.join("usage-data").join("report.html"), SAMPLE.replace("228", "300")).unwrap();
        sync(&conn, &dirs, &archive).unwrap();
        assert_eq!(clear(&conn, &archive).unwrap(), 2);
        assert!(list(&conn).unwrap().is_empty());
        assert_eq!(std::fs::read_dir(&archive).unwrap().count(), 0);
        let left: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM insights_report WHERE subtitle IS NOT NULL OR archived_file IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(left, 0, "只保留哈希");
        assert_eq!(sync(&conn, &dirs, &archive).unwrap().len(), 0);
    }

    #[test]
    fn archived_path_rejects_bad_names() {
        let conn = db::open_in_memory().unwrap();
        conn.execute(
            "INSERT INTO insights_report (sha256, source_path, archived_file, generated_at, bytes, archived_at)
             VALUES ('x', 'p', '..\\evil.html', 't', 1, 't')",
            [],
        )
        .unwrap();
        let tmp = tempfile::tempdir().unwrap();
        assert!(archived_path(&conn, tmp.path(), 1, false).is_err());
    }

    /// 本机真实报告：`cargo test real_report -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_report() {
        let dirs = crate::claude_log::locate::candidate_dirs();
        let files = list_report_files(&dirs);
        assert!(!files.is_empty(), "本机没有 insights 报告");
        for f in files {
            let html = std::fs::read_to_string(&f).unwrap();
            let h = parse_header(&html);
            println!(
                "{} → {:?} / {:?} {:?}~{:?}",
                f.display(),
                h.sessions_analyzed,
                h.sessions_total,
                h.period_start,
                h.period_end
            );
            assert!(h.sessions_analyzed.is_some() && h.sessions_total.is_some());
            let v = prepare_for_view(&html);
            assert!(!v.contains("fonts.googleapis.com"));
        }
    }
}
