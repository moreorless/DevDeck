//! 增量扫描：按文件记录已读字节偏移，只解析新增的完整行，写入 usage_event 与 sessions

use std::{
    collections::{BTreeSet, HashSet},
    fs::File,
    io::{BufRead, BufReader, Seek, SeekFrom},
    time::{Instant, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::Serialize;

use super::{
    locate::LogFile,
    parser::{LineInfo, UsageRecord, parse_line},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScanMode {
    /// 只读新增内容
    Incremental,
    /// 全部重读（在同一事务中完成，失败时保留旧数据）。
    /// 日志文件已被删除的历史用量保留，只按当前时区重算日期
    Full,
}

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStats {
    pub files_total: usize,
    pub files_read: usize,
    pub bytes_read: u64,
    pub lines: u64,
    pub bad_lines: u64,
    pub usage_records: u64,
    pub duration_ms: u64,
}

/// 扫描给定文件并写库。`tz` 决定 usage 的归属日期；`progress(已处理, 总数)`
pub fn scan<Tz: TimeZone>(
    conn: &mut Connection,
    files: &[LogFile],
    mode: ScanMode,
    tz: &Tz,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<ScanStats> {
    let started = Instant::now();
    let mut stats = ScanStats { files_total: files.len(), ..Default::default() };
    let tx = conn.transaction()?;

    if mode == ScanMode::Full {
        tx.execute_batch("DELETE FROM file_cursor; DELETE FROM sessions;")?;
        // 现存日志会被完整重读，先删掉它们的旧用量；已无日志的会话用量保留
        let mut del = tx.prepare("DELETE FROM usage_event WHERE session_id = ?1")?;
        let sids: BTreeSet<&str> = files.iter().map(|f| f.session_id.as_str()).collect();
        for sid in sids {
            del.execute([sid])?;
        }
    }

    for (i, f) in files.iter().enumerate() {
        scan_file(&tx, f, tz, &mut stats).with_context(|| format!("扫描失败: {}", f.path.display()))?;
        progress(i + 1, files.len());
    }
    remove_missing(&tx, files)?;
    if mode == ScanMode::Full {
        recompute_dates(&tx, tz)?;
    }
    tx.commit()?;

    stats.duration_ms = started.elapsed().as_millis() as u64;
    Ok(stats)
}

struct Cursor {
    size: u64,
    mtime: i64,
    offset: u64,
}

fn scan_file<Tz: TimeZone>(tx: &Transaction, f: &LogFile, tz: &Tz, stats: &mut ScanStats) -> Result<()> {
    let path_key = f.path.to_string_lossy().into_owned();
    let meta = match std::fs::metadata(&f.path) {
        Ok(m) => m,
        Err(_) => return Ok(()), // 扫描过程中被删除，跳过
    };
    let size = meta.len();
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    let cursor: Option<Cursor> = tx
        .query_row(
            "SELECT size, mtime, offset FROM file_cursor WHERE path = ?1",
            [&path_key],
            |r| Ok(Cursor { size: r.get::<_, i64>(0)? as u64, mtime: r.get(1)?, offset: r.get::<_, i64>(2)? as u64 }),
        )
        .optional()?;

    let mut offset = match &cursor {
        Some(c) if c.size == size && c.mtime == mtime => return Ok(()), // 未变化
        Some(c) if size < c.offset => {
            // 文件变小：视为被重写，整文件重读；会话元数据也从头累计
            if !f.is_subagent {
                tx.execute("DELETE FROM sessions WHERE session_id = ?1", [&f.session_id])?;
            }
            0
        }
        Some(c) => c.offset,
        None => 0,
    };

    let mut session = if f.is_subagent { None } else { Some(SessionRow::load(tx, &f.session_id)?) };

    let mut reader = BufReader::new(File::open(&f.path)?);
    reader.seek(SeekFrom::Start(offset))?;
    let mut buf = Vec::new();
    loop {
        buf.clear();
        let n = reader.read_until(b'\n', &mut buf)?;
        if n == 0 || buf.last() != Some(&b'\n') {
            break; // 末尾不完整的行留到下次读取
        }
        offset += n as u64;
        stats.bytes_read += n as u64;
        let line = String::from_utf8_lossy(&buf);
        if line.trim().is_empty() {
            continue;
        }
        stats.lines += 1;
        let Some(info) = parse_line(&line) else {
            stats.bad_lines += 1;
            continue;
        };
        if let Some(u) = &info.usage {
            let project = info
                .cwd
                .clone()
                .or_else(|| session.as_ref().and_then(|s| s.project.clone()))
                .unwrap_or_else(|| f.project_dir.clone());
            upsert_usage(tx, u, &f.session_id, &project, tz)?;
            stats.usage_records += 1;
        }
        if let Some(s) = session.as_mut() {
            s.apply(&info);
        }
    }

    if let Some(mut s) = session {
        s.file_path = Some(path_key.clone());
        s.has_subagents = s.has_subagents || f.has_subagents;
        if s.started_at.is_some() {
            s.save(tx)?;
        }
    }
    tx.execute(
        "INSERT INTO file_cursor (path, size, mtime, offset) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(path) DO UPDATE SET size = excluded.size, mtime = excluded.mtime, offset = excluded.offset",
        params![path_key, size as i64, mtime, offset as i64],
    )?;
    stats.files_read += 1;
    Ok(())
}

/// 同一 (message_id, request_id) 出现多次时，保留 output_tokens 最大的一条
/// （实测重复行中少数后出现的行 output 更大，个别重复行 usage 全为 0）
fn upsert_usage<Tz: TimeZone>(tx: &Transaction, u: &UsageRecord, session_id: &str, project: &str, tz: &Tz) -> Result<()> {
    let date = u.ts.with_timezone(tz).date_naive().format("%Y-%m-%d").to_string();
    tx.execute(
        "INSERT INTO usage_event (message_id, request_id, session_id, project, model, ts, date,
            input_tokens, output_tokens, cache_write_5m_tokens, cache_write_1h_tokens, cache_read_tokens,
            prompt_tokens, speed, geo)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
         ON CONFLICT(message_id, request_id) DO UPDATE SET
            input_tokens = excluded.input_tokens, output_tokens = excluded.output_tokens,
            cache_write_5m_tokens = excluded.cache_write_5m_tokens,
            cache_write_1h_tokens = excluded.cache_write_1h_tokens,
            cache_read_tokens = excluded.cache_read_tokens, prompt_tokens = excluded.prompt_tokens
         WHERE excluded.output_tokens > usage_event.output_tokens",
        params![
            u.message_id,
            u.request_id,
            session_id,
            project,
            u.model,
            fmt_ts(&u.ts),
            date,
            u.input as i64,
            u.output as i64,
            u.cache_write_5m as i64,
            u.cache_write_1h as i64,
            u.cache_read as i64,
            u.prompt_tokens() as i64,
            u.speed,
            u.geo,
        ],
    )?;
    Ok(())
}

/// 文件已不存在：删除游标与会话索引；用量保留（Claude Code 会定期清理旧日志，用量历史不应随之消失）
fn remove_missing(tx: &Transaction, files: &[LogFile]) -> Result<()> {
    let present: HashSet<String> = files.iter().map(|f| f.path.to_string_lossy().into_owned()).collect();
    let known: Vec<String> = {
        let mut stmt = tx.prepare("SELECT path FROM file_cursor")?;
        stmt.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?
    };
    for p in known.into_iter().filter(|p| !present.contains(p)) {
        tx.execute("DELETE FROM file_cursor WHERE path = ?1", [&p])?;
        tx.execute("DELETE FROM sessions WHERE file_path = ?1", [&p])?;
    }
    Ok(())
}

/// 按当前时区重算所有用量的归属日期（时区变更后保留的历史用量也要跟着变）
fn recompute_dates<Tz: TimeZone>(tx: &Transaction, tz: &Tz) -> Result<()> {
    let rows: Vec<(String, String, String, String)> = {
        let mut stmt = tx.prepare("SELECT message_id, request_id, ts, date FROM usage_event")?;
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<Result<_, _>>()?
    };
    let mut upd = tx.prepare("UPDATE usage_event SET date = ?1 WHERE message_id = ?2 AND request_id = ?3")?;
    for (mid, rid, ts, old) in rows {
        if let Some(t) = parse_ts(Some(ts)) {
            let d = t.with_timezone(tz).date_naive().format("%Y-%m-%d").to_string();
            if d != old {
                upd.execute(params![d, mid, rid])?;
            }
        }
    }
    Ok(())
}

pub fn fmt_ts(ts: &DateTime<Utc>) -> String {
    ts.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn parse_ts(s: Option<String>) -> Option<DateTime<Utc>> {
    s.and_then(|s| DateTime::parse_from_rfc3339(&s).ok()).map(|d| d.with_timezone(&Utc))
}

/// sessions 表中的一行，扫描时在内存中累计后整行写回
#[derive(Debug, Default)]
struct SessionRow {
    session_id: String,
    file_path: Option<String>,
    project: Option<String>,
    git_branch: Option<String>,
    ai_title: Option<String>,
    custom_title: Option<String>,
    first_prompt: Option<String>,
    last_prompt: Option<String>,
    started_at: Option<DateTime<Utc>>,
    ended_at: Option<DateTime<Utc>>,
    prompt_count: i64,
    models: BTreeSet<String>,
    has_subagents: bool,
}

impl SessionRow {
    fn load(tx: &Transaction, session_id: &str) -> Result<Self> {
        let row = tx
            .query_row(
                "SELECT file_path, project, git_branch, ai_title, custom_title, first_prompt, last_prompt,
                        started_at, ended_at, prompt_count, models, has_subagents
                 FROM sessions WHERE session_id = ?1",
                [session_id],
                |r| {
                    let models: String = r.get(10)?;
                    Ok(SessionRow {
                        session_id: session_id.to_string(),
                        file_path: r.get(0)?,
                        project: r.get(1)?,
                        git_branch: r.get(2)?,
                        ai_title: r.get(3)?,
                        custom_title: r.get(4)?,
                        first_prompt: r.get(5)?,
                        last_prompt: r.get(6)?,
                        started_at: parse_ts(r.get(7)?),
                        ended_at: parse_ts(r.get(8)?),
                        prompt_count: r.get(9)?,
                        models: serde_json::from_str(&models).unwrap_or_default(),
                        has_subagents: r.get(11)?,
                    })
                },
            )
            .optional()?;
        Ok(row.unwrap_or_else(|| SessionRow { session_id: session_id.to_string(), ..Default::default() }))
    }

    fn apply(&mut self, info: &LineInfo) {
        if let Some(ts) = info.ts {
            self.started_at = Some(self.started_at.map_or(ts, |s| s.min(ts)));
            self.ended_at = Some(self.ended_at.map_or(ts, |e| e.max(ts)));
        }
        if self.project.is_none() {
            self.project = info.cwd.clone();
        }
        if info.git_branch.is_some() {
            self.git_branch = info.git_branch.clone();
        }
        if let Some(m) = &info.model {
            self.models.insert(m.clone());
        }
        if let Some(p) = &info.user_prompt {
            self.prompt_count += 1;
            if self.first_prompt.is_none() {
                self.first_prompt = Some(p.clone());
            }
        }
        if info.ai_title.is_some() {
            self.ai_title = info.ai_title.clone();
        }
        if info.custom_title.is_some() {
            self.custom_title = info.custom_title.clone();
        }
        if info.last_prompt.is_some() {
            self.last_prompt = info.last_prompt.clone();
        }
    }

    fn save(&self, tx: &Transaction) -> Result<()> {
        tx.execute(
            "INSERT OR REPLACE INTO sessions (session_id, file_path, project, git_branch, ai_title, custom_title,
                first_prompt, last_prompt, started_at, ended_at, prompt_count, models, has_subagents)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                self.session_id,
                self.file_path,
                self.project,
                self.git_branch,
                self.ai_title,
                self.custom_title,
                self.first_prompt,
                self.last_prompt,
                self.started_at.as_ref().map(fmt_ts),
                self.ended_at.as_ref().map(fmt_ts),
                self.prompt_count,
                serde_json::to_string(&self.models)?,
                self.has_subagents,
            ],
        )?;
        Ok(())
    }
}

/// 数据库中是否已有扫描记录（用于启动时决定全量还是增量）
pub fn has_any_cursor(conn: &Connection) -> Result<bool> {
    Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM file_cursor)", [], |r| r.get(0))?)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use chrono::FixedOffset;

    use super::*;
    use crate::claude_log::{locate::list_log_files, test_support::*};

    fn count(conn: &Connection, sql: &str) -> i64 {
        conn.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn full_scan_dedups_and_indexes() {
        let (_tmp, dir) = copy_fixture();
        let mut conn = crate::db::open_in_memory().unwrap();
        let stats = scan_utc(&mut conn, &dir, ScanMode::Full);

        assert_eq!(stats.files_total, 4);
        assert_eq!(stats.bad_lines, 1);
        // msg_a1 两行、msg_a2 跨文件重复、<synthetic> 跳过 → 5 条
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM usage_event"), 5);
        // 重复行保留 output 最大的那条
        assert_eq!(count(&conn, "SELECT output_tokens FROM usage_event WHERE message_id='msg_a1'"), 50);
        assert_eq!(count(&conn, "SELECT output_tokens FROM usage_event WHERE message_id='msg_a2'"), 20);
        assert_eq!(count(&conn, "SELECT cache_write_1h_tokens FROM usage_event WHERE message_id='msg_a2'"), 200);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM usage_event WHERE model='<synthetic>'"), 0);
        // 子代理用量归到父会话
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM usage_event WHERE session_id='sess-a'"), 4);
        // 只有元数据、无时间戳的文件不建会话
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM sessions"), 2);
    }

    #[test]
    fn session_metadata() {
        let (_tmp, _dir, conn) = scanned_db();
        let row = conn
            .query_row(
                "SELECT project, git_branch, ai_title, custom_title, first_prompt, last_prompt,
                        started_at, ended_at, prompt_count, models, has_subagents
                 FROM sessions WHERE session_id='sess-a'",
                [],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, String>(5)?,
                        r.get::<_, String>(6)?,
                        r.get::<_, String>(7)?,
                        r.get::<_, i64>(8)?,
                        r.get::<_, String>(9)?,
                        r.get::<_, bool>(10)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(row.0, r"C:\demo\proj");
        assert_eq!(row.1, "feature/login");
        assert_eq!(row.2, "登录功能实现");
        assert_eq!(row.3, "我的会话");
        assert_eq!(row.4, "实现登录功能");
        assert_eq!(row.5, "/clear");
        assert_eq!(row.6, "2026-11-01T09:00:00.000Z");
        assert_eq!(row.7, "2026-11-02T01:00:05.000Z");
        assert_eq!(row.8, 2, "工具结果不算提示");
        assert_eq!(row.9, r#"["claude-haiku-5-5","claude-opus-5-5"]"#);
        assert!(row.10);
    }

    #[test]
    fn date_follows_timezone() {
        let (_tmp, dir) = copy_fixture();
        let mut conn = crate::db::open_in_memory().unwrap();
        let files = list_log_files(&[dir]);
        let mst = FixedOffset::west_opt(7 * 3600).unwrap();
        scan(&mut conn, &files, ScanMode::Full, &mst, &mut |_, _| {}).unwrap();
        // 2026-11-02T01:00Z 在 UTC-7 是 11-01 18:00
        let d: String =
            conn.query_row("SELECT date FROM usage_event WHERE message_id='msg_a4'", [], |r| r.get(0)).unwrap();
        assert_eq!(d, "2026-11-01");
    }

    #[test]
    fn incremental_reads_only_new_complete_lines() {
        let (_tmp, dir) = copy_fixture();
        let mut conn = crate::db::open_in_memory().unwrap();
        scan_utc(&mut conn, &dir, ScanMode::Incremental);
        let file_b = dir.join("projects").join("D--other").join("sess-b.jsonl");

        // 未变化：不读任何文件
        let s = scan_utc(&mut conn, &dir, ScanMode::Incremental);
        assert_eq!(s.files_read, 0);

        let line = r#"{"type":"assistant","timestamp":"2026-11-03T12:01:00.000Z","cwd":"D:\\other","requestId":"req_b2","message":{"id":"msg_b2","model":"claude-opus-5-5","usage":{"input_tokens":5,"output_tokens":5}}}"#;
        let half = r#"{"type":"assistant","timestamp":"2026-11-03T12:02:00.000Z","requestId":"req_b3","message":{"id":"msg_b3","#;
        let mut f = std::fs::OpenOptions::new().append(true).open(&file_b).unwrap();
        write!(f, "{line}\n{half}").unwrap();
        drop(f);

        let s = scan_utc(&mut conn, &dir, ScanMode::Incremental);
        assert_eq!(s.files_read, 1);
        assert_eq!(s.lines, 1, "不完整的行留到下次");
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM usage_event"), 6);

        let mut f = std::fs::OpenOptions::new().append(true).open(&file_b).unwrap();
        writeln!(f, r#""model":"claude-opus-5-5","usage":{{"input_tokens":1,"output_tokens":1}}}}}}"#).unwrap();
        drop(f);
        let s = scan_utc(&mut conn, &dir, ScanMode::Incremental);
        assert_eq!(s.lines, 1);
        assert_eq!(s.bad_lines, 0);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM usage_event"), 7);
    }

    #[test]
    fn rewritten_file_is_reread_from_start() {
        let (_tmp, dir) = copy_fixture();
        let mut conn = crate::db::open_in_memory().unwrap();
        scan_utc(&mut conn, &dir, ScanMode::Full);
        let file_b = dir.join("projects").join("D--other").join("sess-b.jsonl");
        std::fs::write(
            &file_b,
            "{\"type\":\"user\",\"timestamp\":\"2026-11-05T00:00:00.000Z\",\"message\":{\"content\":\"新\"}}\n",
        )
        .unwrap();
        scan_utc(&mut conn, &dir, ScanMode::Incremental);
        let (first, cnt): (String, i64) = conn
            .query_row("SELECT first_prompt, prompt_count FROM sessions WHERE session_id='sess-b'", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((first.as_str(), cnt), ("新", 1));
    }

    #[test]
    fn deleted_file_removes_session_but_keeps_usage() {
        let (_tmp, dir) = copy_fixture();
        let mut conn = crate::db::open_in_memory().unwrap();
        scan_utc(&mut conn, &dir, ScanMode::Full);
        std::fs::remove_file(dir.join("projects").join("D--other").join("sess-b.jsonl")).unwrap();
        scan_utc(&mut conn, &dir, ScanMode::Incremental);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM sessions WHERE session_id='sess-b'"), 0);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM usage_event WHERE session_id='sess-b'"), 1);
    }

    #[test]
    fn full_rebuild_keeps_usage_of_deleted_logs_and_recomputes_dates() {
        let (_tmp, dir) = copy_fixture();
        let mut conn = crate::db::open_in_memory().unwrap();
        scan_utc(&mut conn, &dir, ScanMode::Full);
        // sess-b 的日志被 Claude Code 清理后再全量重建（换成 UTC+14，日期整体后移）
        std::fs::remove_file(dir.join("projects").join("D--other").join("sess-b.jsonl")).unwrap();
        let files = list_log_files(&[dir.clone()]);
        let tz = FixedOffset::east_opt(14 * 3600).unwrap();
        scan(&mut conn, &files, ScanMode::Full, &tz, &mut |_, _| {}).unwrap();

        assert_eq!(count(&conn, "SELECT COUNT(*) FROM usage_event"), 5);
        let d: String =
            conn.query_row("SELECT date FROM usage_event WHERE message_id='msg_b1'", [], |r| r.get(0)).unwrap();
        assert_eq!(d, "2026-11-04", "保留的历史用量按新时区重算日期");
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM sessions"), 1);
    }

    #[test]
    fn full_rebuild_rereads_existing_logs() {
        let (_tmp, dir) = copy_fixture();
        let mut conn = crate::db::open_in_memory().unwrap();
        scan_utc(&mut conn, &dir, ScanMode::Full);
        // 模拟旧版本写入了错误数据：现存日志的用量应被重读覆盖
        conn.execute("UPDATE usage_event SET output_tokens = 999999 WHERE message_id = 'msg_a1'", []).unwrap();
        scan_utc(&mut conn, &dir, ScanMode::Full);
        assert_eq!(count(&conn, "SELECT output_tokens FROM usage_event WHERE message_id='msg_a1'"), 50);
    }

    #[test]
    fn full_rescan_is_idempotent() {
        let (_tmp, dir) = copy_fixture();
        let mut conn = crate::db::open_in_memory().unwrap();
        scan_utc(&mut conn, &dir, ScanMode::Full);
        scan_utc(&mut conn, &dir, ScanMode::Full);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM usage_event"), 5);
        assert_eq!(count(&conn, "SELECT prompt_count FROM sessions WHERE session_id='sess-a'"), 2);
    }
}
