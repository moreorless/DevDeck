//! 会话索引查询

use anyhow::Result;
use chrono::{Days, NaiveDate, TimeZone, Utc};
use rusqlite::{Connection, params_from_iter, types::Value};
use serde::{Deserialize, Serialize};

use crate::{
    claude_log::scanner::fmt_ts,
    modules::usage::{pricing::PricingTable, query::aggregate},
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionQuery {
    pub project: Option<String>,
    /// 与会话起止时间有交集的本地日期范围
    pub since: Option<NaiveDate>,
    pub until: Option<NaiveDate>,
    /// 匹配标题、首条提示、最后提示
    pub keyword: Option<String>,
    #[serde(default)]
    pub offset: u32,
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 {
    50
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub session_id: String,
    pub project: Option<String>,
    pub git_branch: Option<String>,
    /// 自定义标题优先，其次 AI 生成的标题
    pub title: Option<String>,
    pub first_prompt: Option<String>,
    pub last_prompt: Option<String>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub prompt_count: i64,
    pub models: Vec<String>,
    pub has_subagents: bool,
    /// 含子代理的用量
    pub total_tokens: u64,
    pub cost_usd: f64,
    pub priced: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPage {
    pub total: i64,
    pub items: Vec<SessionSummary>,
}

/// 分页查询会话，按最后活动时间倒序；只列出至少有一条用户提示的会话
pub fn list_sessions<Tz: TimeZone>(
    conn: &Connection,
    pricing: &PricingTable,
    q: &SessionQuery,
    tz: &Tz,
) -> Result<SessionPage> {
    let mut where_sql = String::from("prompt_count > 0");
    let mut params: Vec<Value> = Vec::new();
    if let Some(p) = q.project.as_ref().filter(|p| !p.is_empty()) {
        where_sql.push_str(" AND project = ?");
        params.push(Value::Text(p.clone()));
    }
    if let Some(d) = q.since {
        where_sql.push_str(" AND ended_at >= ?");
        params.push(Value::Text(local_midnight_utc(d, tz)));
    }
    if let Some(d) = q.until.and_then(|d| d.checked_add_days(Days::new(1))) {
        where_sql.push_str(" AND started_at < ?");
        params.push(Value::Text(local_midnight_utc(d, tz)));
    }
    if let Some(k) = q.keyword.as_ref().map(|k| k.trim()).filter(|k| !k.is_empty()) {
        where_sql.push_str(
            " AND (custom_title LIKE ? ESCAPE '\\' OR ai_title LIKE ? ESCAPE '\\'
                   OR first_prompt LIKE ? ESCAPE '\\' OR last_prompt LIKE ? ESCAPE '\\')",
        );
        let pat = format!("%{}%", k.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
        params.extend(std::iter::repeat_n(Value::Text(pat), 4));
    }

    let total: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM sessions WHERE {where_sql}"),
        params_from_iter(params.clone()),
        |r| r.get(0),
    )?;

    let mut page_params = params;
    page_params.push(Value::Integer(q.limit.clamp(1, 500) as i64));
    page_params.push(Value::Integer(q.offset as i64));
    let mut items: Vec<SessionSummary> = {
        let mut stmt = conn.prepare(&format!(
            "SELECT session_id, project, git_branch, COALESCE(custom_title, ai_title), first_prompt, last_prompt,
                    started_at, ended_at, prompt_count, models, has_subagents
             FROM sessions WHERE {where_sql}
             ORDER BY ended_at DESC LIMIT ? OFFSET ?"
        ))?;
        stmt.query_map(params_from_iter(page_params), |r| {
            let models: String = r.get(9)?;
            Ok(SessionSummary {
                session_id: r.get(0)?,
                project: r.get(1)?,
                git_branch: r.get(2)?,
                title: r.get(3)?,
                first_prompt: r.get(4)?,
                last_prompt: r.get(5)?,
                started_at: r.get(6)?,
                ended_at: r.get(7)?,
                prompt_count: r.get(8)?,
                models: serde_json::from_str(&models).unwrap_or_default(),
                has_subagents: r.get(10)?,
                total_tokens: 0,
                cost_usd: 0.0,
                priced: true,
            })
        })?
        .collect::<Result<_, _>>()?
    };

    if !items.is_empty() {
        let ids: Vec<Value> = items.iter().map(|s| Value::Text(s.session_id.clone())).collect();
        let where_ids = format!("session_id IN ({})", vec!["?"; ids.len()].join(","));
        let agg = aggregate(conn, pricing, "session_id", &where_ids, ids)?;
        for ((sid, _model), a) in agg {
            if let Some(s) = items.iter_mut().find(|s| s.session_id == sid) {
                s.total_tokens += a.total();
                s.cost_usd += a.cost_usd;
                s.priced &= a.priced;
            }
        }
    }
    Ok(SessionPage { total, items })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentProject {
    pub project: String,
    pub last_active_at: String,
    pub session_count: i64,
}

/// 最近活跃的项目：按项目下会话的最后活动时间倒序
pub fn recent_projects(conn: &Connection, limit: u32) -> Result<Vec<RecentProject>> {
    let mut stmt = conn.prepare(
        "SELECT project, MAX(ended_at) AS last, COUNT(*) FROM sessions
         WHERE project IS NOT NULL AND prompt_count > 0 AND ended_at IS NOT NULL
         GROUP BY project ORDER BY last DESC LIMIT ?1",
    )?;
    Ok(stmt
        .query_map([limit.clamp(1, 50)], |r| {
            Ok(RecentProject { project: r.get(0)?, last_active_at: r.get(1)?, session_count: r.get(2)? })
        })?
        .collect::<Result<_, _>>()?)
}

fn local_midnight_utc<Tz: TimeZone>(d: NaiveDate, tz: &Tz) -> String {
    let naive = d.and_hms_opt(0, 0, 0).expect("合法时间");
    let utc = tz
        .from_local_datetime(&naive)
        .earliest()
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or_else(|| Utc.from_utc_datetime(&naive));
    fmt_ts(&utc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude_log::test_support::scanned_db;

    fn q() -> SessionQuery {
        SessionQuery { project: None, since: None, until: None, keyword: None, offset: 0, limit: 50 }
    }

    #[test]
    fn lists_sessions_newest_first_with_usage() {
        let (_t, _d, conn) = scanned_db();
        let p = PricingTable::builtin();
        let page = list_sessions(&conn, &p, &q(), &Utc).unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.items[0].session_id, "sess-b");
        let a = &page.items[1];
        assert_eq!(a.title.as_deref(), Some("我的会话"));
        assert_eq!(a.total_tokens, 1382 + 110 + 150_100, "含子代理用量");
        assert!(a.priced);
        assert!((a.cost_usd - (0.003748 + 0.0003 + 0.07525)).abs() < 1e-12);
        assert!(!page.items[0].priced);
    }

    #[test]
    fn recent_projects_by_last_activity() {
        let (_t, _d, conn) = scanned_db();
        let r = recent_projects(&conn, 5).unwrap();
        let names: Vec<&str> = r.iter().map(|p| p.project.as_str()).collect();
        assert_eq!(names, vec![r"D:\other", r"C:\demo\proj"], "sess-b（11-03）比 sess-a（11-02）更近");
        assert_eq!(r[0].session_count, 1);
        assert_eq!(recent_projects(&conn, 1).unwrap().len(), 1);
    }

    #[test]
    fn filters_and_paging() {
        let (_t, _d, conn) = scanned_db();
        let p = PricingTable::builtin();
        let run = |q: SessionQuery| list_sessions(&conn, &p, &q, &Utc).unwrap();

        assert_eq!(run(SessionQuery { keyword: Some("登录".into()), ..q() }).items[0].session_id, "sess-a");
        assert_eq!(run(SessionQuery { keyword: Some("100%_".into()), ..q() }).total, 1, "% 与 _ 按字面匹配");
        assert_eq!(run(SessionQuery { keyword: Some("%".into()), ..q() }).total, 1);
        assert_eq!(run(SessionQuery { project: Some(r"C:\demo\proj".into()), ..q() }).total, 1);
        let since = NaiveDate::from_ymd_opt(2026, 11, 3);
        assert_eq!(run(SessionQuery { since, ..q() }).items[0].session_id, "sess-b");
        let until = NaiveDate::from_ymd_opt(2026, 11, 1);
        assert_eq!(run(SessionQuery { until, ..q() }).items[0].session_id, "sess-a");

        let page = run(SessionQuery { limit: 1, offset: 1, ..q() });
        assert_eq!((page.total, page.items.len(), page.items[0].session_id.as_str()), (2, 1, "sess-a"));
    }
}
