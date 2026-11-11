//! 测试辅助：把 fixture 日志复制到临时目录并扫描入库

use std::path::{Path, PathBuf};

use chrono::Utc;
use rusqlite::Connection;
use tempfile::TempDir;

use super::{
    locate::list_log_files,
    scanner::{ScanMode, ScanStats, scan},
};

pub fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join("claude")
}

/// 复制 fixture 到临时目录（允许测试修改文件），返回临时目录与其中的 claude 目录
pub fn copy_fixture() -> (TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let dst = tmp.path().join("claude");
    copy_dir(&fixture_root(), &dst);
    (tmp, dst)
}

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for e in std::fs::read_dir(src).unwrap() {
        let p = e.unwrap().path();
        let target = dst.join(p.file_name().unwrap());
        if p.is_dir() { copy_dir(&p, &target) } else { std::fs::copy(&p, &target).unwrap(); }
    }
}

/// 以 UTC 归日扫描 `claude_dir`
pub fn scan_utc(conn: &mut Connection, claude_dir: &Path, mode: ScanMode) -> ScanStats {
    let files = list_log_files(&[claude_dir.to_path_buf()]);
    scan(conn, &files, mode, &Utc, &mut |_, _| {}).unwrap()
}

/// 扫描好的内存数据库
pub fn scanned_db() -> (TempDir, PathBuf, Connection) {
    let (tmp, dir) = copy_fixture();
    let mut conn = crate::db::open_in_memory().unwrap();
    scan_utc(&mut conn, &dir, ScanMode::Full);
    (tmp, dir, conn)
}
