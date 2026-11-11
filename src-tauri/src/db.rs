//! SQLite 连接与表结构迁移

use std::{path::Path, time::Duration};

use anyhow::Result;
use rusqlite::Connection;

const SCHEMA_V1: &str = r#"
CREATE TABLE file_cursor (
  path TEXT PRIMARY KEY,
  size INTEGER NOT NULL,
  mtime INTEGER NOT NULL,
  offset INTEGER NOT NULL
);
CREATE TABLE usage_event (
  message_id TEXT NOT NULL,
  request_id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  project TEXT NOT NULL,
  model TEXT NOT NULL,
  ts TEXT NOT NULL,
  date TEXT NOT NULL,
  input_tokens INTEGER NOT NULL,
  output_tokens INTEGER NOT NULL,
  cache_write_5m_tokens INTEGER NOT NULL,
  cache_write_1h_tokens INTEGER NOT NULL,
  cache_read_tokens INTEGER NOT NULL,
  prompt_tokens INTEGER NOT NULL,
  speed TEXT,
  geo TEXT,
  PRIMARY KEY (message_id, request_id)
);
CREATE INDEX idx_usage_date ON usage_event(date);
CREATE INDEX idx_usage_session ON usage_event(session_id);
CREATE TABLE sessions (
  session_id TEXT PRIMARY KEY,
  file_path TEXT,
  project TEXT,
  git_branch TEXT,
  ai_title TEXT,
  custom_title TEXT,
  first_prompt TEXT,
  last_prompt TEXT,
  started_at TEXT,
  ended_at TEXT,
  prompt_count INTEGER NOT NULL DEFAULT 0,
  models TEXT NOT NULL DEFAULT '[]',
  has_subagents INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_sessions_ended ON sessions(ended_at);
CREATE TABLE scan_log (
  id INTEGER PRIMARY KEY,
  started_at TEXT NOT NULL,
  finished_at TEXT,
  mode TEXT NOT NULL,
  files_total INTEGER,
  files_read INTEGER,
  bad_lines INTEGER,
  ok INTEGER NOT NULL DEFAULT 0,
  error TEXT
);
"#;

/// Insights 报告归档索引；删除后只保留 sha256 等，避免再次归档
const SCHEMA_V3: &str = r#"
CREATE TABLE insights_report (
  id INTEGER PRIMARY KEY,
  sha256 TEXT UNIQUE NOT NULL,
  source_path TEXT NOT NULL,
  archived_file TEXT,
  generated_at TEXT NOT NULL,
  subtitle TEXT,
  sessions_analyzed INTEGER,
  sessions_total INTEGER,
  period_start TEXT,
  period_end TEXT,
  bytes INTEGER NOT NULL,
  archived_at TEXT NOT NULL,
  deleted_at TEXT
);
"#;

/// 打开数据库（WAL 模式，允许扫描写入时并发读取）并执行迁移
pub fn open(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.busy_timeout(Duration::from_secs(10))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    migrate(&conn)?;
    Ok(conn)
}

/// 内存数据库，测试用
#[cfg(test)]
pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> Result<()> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version < 1 {
        conn.execute_batch(&format!("BEGIN;{SCHEMA_V1}PRAGMA user_version = 1;COMMIT;"))?;
    }
    if version < 2 {
        conn.execute_batch(
            "BEGIN;CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);PRAGMA user_version = 2;COMMIT;",
        )?;
    }
    if version < 3 {
        conn.execute_batch(&format!("BEGIN;{SCHEMA_V3}PRAGMA user_version = 3;COMMIT;"))?;
    }
    if version < 4 {
        // Insights 中文译文文件名（`<归档文件名>.zh.html`）；从 DevDeck 生成 / 翻译的估算费用
        conn.execute_batch(
            "BEGIN;ALTER TABLE insights_report ADD COLUMN translated_file TEXT;
             ALTER TABLE insights_report ADD COLUMN cost_usd REAL;PRAGMA user_version = 4;COMMIT;",
        )?;
    }
    Ok(())
}
