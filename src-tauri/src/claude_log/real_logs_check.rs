//! 对照验证：扫描本机真实日志，导出每日 token 合计，用于与 `ccusage daily --json` 比对。
//! 运行：DEVDECK_CHECK_OUT=out.json cargo test --release real_logs -- --ignored --nocapture

use std::time::Instant;

use chrono::Local;
use rusqlite::Connection;

use super::{
    locate::{candidate_dirs, existing_dirs, list_log_files},
    scanner::{ScanMode, scan},
};

#[test]
#[ignore = "依赖本机真实日志"]
fn real_logs() {
    let dirs = existing_dirs(&candidate_dirs());
    assert!(!dirs.is_empty(), "未找到 Claude 日志目录");
    let files = list_log_files(&dirs);

    let tmp = tempfile::tempdir().unwrap();
    let mut conn = crate::db::open(&tmp.path().join("check.db")).unwrap();
    let t = Instant::now();
    let stats = scan(&mut conn, &files, ScanMode::Full, &Local, &mut |_, _| {}).unwrap();
    println!("全量扫描: {stats:?}, 耗时 {:?}", t.elapsed());

    let t = Instant::now();
    let inc = scan(&mut conn, &files, ScanMode::Incremental, &Local, &mut |_, _| {}).unwrap();
    println!("增量扫描（无变化）: 读取 {} 个文件, 耗时 {:?}", inc.files_read, t.elapsed());

    let out = daily_totals(&conn);
    if let Ok(path) = std::env::var("DEVDECK_CHECK_OUT") {
        std::fs::write(&path, serde_json::to_string_pretty(&out).unwrap()).unwrap();
        println!("已写入 {path}");
    }
}

fn daily_totals(conn: &Connection) -> serde_json::Value {
    let mut stmt = conn
        .prepare(
            "SELECT date, SUM(input_tokens), SUM(output_tokens),
                    SUM(cache_write_5m_tokens + cache_write_1h_tokens), SUM(cache_read_tokens), COUNT(*)
             FROM usage_event GROUP BY date ORDER BY date",
        )
        .unwrap();
    let rows: Vec<serde_json::Value> = stmt
        .query_map([], |r| {
            Ok(serde_json::json!({
                "date": r.get::<_, String>(0)?,
                "input": r.get::<_, i64>(1)?,
                "output": r.get::<_, i64>(2)?,
                "cacheCreation": r.get::<_, i64>(3)?,
                "cacheRead": r.get::<_, i64>(4)?,
                "events": r.get::<_, i64>(5)?,
            }))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    serde_json::Value::Array(rows)
}
