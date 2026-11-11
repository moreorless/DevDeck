//! 用量查询：按日按模型聚合、汇总指标、筛选项

use std::collections::BTreeMap;

use anyhow::Result;
use chrono::{Days, NaiveDate};
use rusqlite::{Connection, params_from_iter, types::Value};
use serde::{Deserialize, Serialize};

use super::pricing::{PricingTable, TokenGroup};

/// 一组 token 的合计与费用
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Agg {
    pub input: u64,
    pub output: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
    pub cache_read: u64,
    pub cost_usd: f64,
    /// 所有 token 都能在价格表中找到单价
    pub priced: bool,
}

impl Agg {
    pub fn total(&self) -> u64 {
        self.input + self.output + self.cache_write_5m + self.cache_write_1h + self.cache_read
    }
}

/// 按 `key_expr` 与模型分组汇总用量并计算费用。
/// `where_sql` 使用匿名 `?` 占位符，对应 `params`；`?1` 已被长上下文阈值占用。
pub fn aggregate(
    conn: &Connection,
    pricing: &PricingTable,
    key_expr: &str,
    where_sql: &str,
    params: Vec<Value>,
) -> Result<BTreeMap<(String, String), Agg>> {
    let threshold = pricing.long_context_threshold().map_or(i64::MAX, |t| t as i64);
    let sql = format!(
        "SELECT {key_expr} AS k, model, speed, geo, prompt_tokens > ?1 AS lc,
                SUM(input_tokens), SUM(output_tokens), SUM(cache_write_5m_tokens),
                SUM(cache_write_1h_tokens), SUM(cache_read_tokens)
         FROM usage_event WHERE {where_sql}
         GROUP BY k, model, speed, geo, lc"
    );
    let mut all = vec![Value::Integer(threshold)];
    all.extend(params);

    let mut out: BTreeMap<(String, String), Agg> = BTreeMap::new();
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(all))?;
    while let Some(r) = rows.next()? {
        let g = TokenGroup {
            model: r.get(1)?,
            speed: r.get(2)?,
            geo: r.get(3)?,
            long_context: r.get(4)?,
            input: r.get::<_, i64>(5)? as u64,
            output: r.get::<_, i64>(6)? as u64,
            cache_write_5m: r.get::<_, i64>(7)? as u64,
            cache_write_1h: r.get::<_, i64>(8)? as u64,
            cache_read: r.get::<_, i64>(9)? as u64,
        };
        let entry = out.entry((r.get(0)?, g.model.clone())).or_insert(Agg { priced: true, ..Default::default() });
        entry.input += g.input;
        entry.output += g.output;
        entry.cache_write_5m += g.cache_write_5m;
        entry.cache_write_1h += g.cache_write_1h;
        entry.cache_read += g.cache_read;
        match pricing.cost(&g) {
            Some(c) => entry.cost_usd += c,
            None => entry.priced = false,
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageFilter {
    pub since: NaiveDate,
    pub until: NaiveDate,
    /// 为空表示不过滤
    #[serde(default)]
    pub projects: Vec<String>,
    #[serde(default)]
    pub models: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyModelUsage {
    pub date: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_write_5m_tokens: u64,
    pub cache_write_1h_tokens: u64,
    pub cache_read_tokens: u64,
    pub total_tokens: u64,
    pub cost_usd: f64,
    pub priced: bool,
}

/// 每日 × 模型的用量，按日期、模型升序
pub fn daily_usage(conn: &Connection, pricing: &PricingTable, f: &UsageFilter) -> Result<Vec<DailyModelUsage>> {
    let mut where_sql = String::from("date BETWEEN ? AND ?");
    let mut params = vec![Value::Text(f.since.to_string()), Value::Text(f.until.to_string())];
    for (col, vals) in [("project", &f.projects), ("model", &f.models)] {
        if !vals.is_empty() {
            where_sql.push_str(&format!(" AND {col} IN ({})", vec!["?"; vals.len()].join(",")));
            params.extend(vals.iter().cloned().map(Value::Text));
        }
    }
    Ok(aggregate(conn, pricing, "date", &where_sql, params)?
        .into_iter()
        .map(|((date, model), a)| DailyModelUsage {
            date,
            model,
            input_tokens: a.input,
            output_tokens: a.output,
            cache_write_5m_tokens: a.cache_write_5m,
            cache_write_1h_tokens: a.cache_write_1h,
            cache_read_tokens: a.cache_read,
            total_tokens: a.total(),
            cost_usd: a.cost_usd,
            priced: a.priced,
        })
        .collect())
}

#[derive(Debug, Default, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub tokens: u64,
    pub cost_usd: f64,
}

/// 指标卡与托盘使用的全局汇总（不受筛选影响）
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub today: Totals,
    pub yesterday: Totals,
    pub last7: Totals,
    pub prev7: Totals,
    pub last30: Totals,
    pub prev30: Totals,
    /// 价格表中找不到的模型，界面提示「价格未知」
    pub unpriced_models: Vec<String>,
}

pub fn summary(conn: &Connection, pricing: &PricingTable, today: NaiveDate) -> Result<Summary> {
    let day = |n: u64| today.checked_sub_days(Days::new(n)).unwrap_or(today);
    let rows = daily_usage(conn, pricing, &UsageFilter { since: day(59), until: today, ..Default::default() })?;

    let mut s = Summary::default();
    for r in &rows {
        let d = NaiveDate::parse_from_str(&r.date, "%Y-%m-%d")?;
        let age = (today - d).num_days();
        let add = |t: &mut Totals| {
            t.tokens += r.total_tokens;
            t.cost_usd += r.cost_usd;
        };
        match age {
            0 => add(&mut s.today),
            1 => add(&mut s.yesterday),
            _ => {}
        }
        if (0..7).contains(&age) { add(&mut s.last7) } else if (7..14).contains(&age) { add(&mut s.prev7) }
        if (0..30).contains(&age) { add(&mut s.last30) } else if (30..60).contains(&age) { add(&mut s.prev30) }
        if !r.priced && !s.unpriced_models.contains(&r.model) {
            s.unpriced_models.push(r.model.clone());
        }
    }
    Ok(s)
}

/// 生成 CSV：每日 × 模型一行，日期倒序。带 UTF-8 BOM，方便 Excel 识别中文
pub fn to_csv(rows: &[DailyModelUsage]) -> String {
    let mut sorted: Vec<&DailyModelUsage> = rows.iter().collect();
    sorted.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.model.cmp(&b.model)));
    let mut out = String::from(
        "\u{feff}日期,模型,Input,Output,Cache 写 (5m),Cache 写 (1h),Cache 读,合计,估算费用 (USD),价格已知\r\n",
    );
    for r in sorted {
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{:.6},{}\r\n",
            r.date,
            csv_field(&r.model),
            r.input_tokens,
            r.output_tokens,
            r.cache_write_5m_tokens,
            r.cache_write_1h_tokens,
            r.cache_read_tokens,
            r.total_tokens,
            r.cost_usd,
            if r.priced { "是" } else { "否" },
        ));
    }
    out
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

pub fn list_projects(conn: &Connection) -> Result<Vec<String>> {
    distinct(conn, "SELECT project FROM usage_event UNION SELECT project FROM sessions WHERE project IS NOT NULL ORDER BY 1")
}

pub fn list_models(conn: &Connection) -> Result<Vec<String>> {
    distinct(conn, "SELECT DISTINCT model FROM usage_event ORDER BY 1")
}

fn distinct(conn: &Connection, sql: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(sql)?;
    Ok(stmt.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude_log::test_support::scanned_db;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn filter(since: &str, until: &str) -> UsageFilter {
        UsageFilter { since: d(since), until: d(until), ..Default::default() }
    }

    #[test]
    fn daily_usage_aggregates_and_prices() {
        let (_t, _d, conn) = scanned_db();
        let p = PricingTable::builtin();
        let rows = daily_usage(&conn, &p, &filter("2026-11-01", "2026-11-03")).unwrap();
        let get = |date: &str, model: &str| rows.iter().find(|r| r.date == date && r.model == model).unwrap();

        let opus = get("2026-11-01", "claude-opus-5-5");
        assert_eq!(
            (
                opus.input_tokens,
                opus.output_tokens,
                opus.cache_write_5m_tokens,
                opus.cache_write_1h_tokens,
                opus.cache_read_tokens
            ),
            (12, 70, 100, 200, 1000)
        );
        assert_eq!(opus.total_tokens, 1382);
        assert!((opus.cost_usd - 0.003748).abs() < 1e-12, "{}", opus.cost_usd);

        let sonnet = get("2026-11-01", "claude-sonnet-5-5");
        assert!((sonnet.cost_usd - 0.0003).abs() < 1e-12);

        // 150k prompt 走 Haiku 5.5 长上下文价格
        let haiku = get("2026-11-02", "claude-haiku-5-5");
        assert!((haiku.cost_usd - 0.07525).abs() < 1e-12, "{}", haiku.cost_usd);

        let unknown = get("2026-11-03", "unknown-model-x");
        assert!(!unknown.priced);
        assert_eq!(unknown.cost_usd, 0.0);
    }

    #[test]
    fn filters_by_project_and_model() {
        let (_t, _d, conn) = scanned_db();
        let p = PricingTable::builtin();
        let mut f = filter("2026-11-01", "2026-11-30");
        f.projects = vec![r"D:\other".into()];
        let rows = daily_usage(&conn, &p, &f).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].model, "unknown-model-x");

        let mut f = filter("2026-11-01", "2026-11-30");
        f.models = vec!["claude-opus-5-5".into(), "claude-sonnet-5-5".into()];
        assert_eq!(daily_usage(&conn, &p, &f).unwrap().len(), 2);

        assert!(daily_usage(&conn, &p, &filter("2026-10-01", "2026-10-31")).unwrap().is_empty());
    }

    #[test]
    fn summary_windows() {
        let (_t, _d, conn) = scanned_db();
        let p = PricingTable::builtin();
        let s = summary(&conn, &p, d("2026-11-02")).unwrap();
        assert_eq!(s.today.tokens, 150_100);
        assert_eq!(s.yesterday.tokens, 1382 + 110);
        assert_eq!(s.last7.tokens, 150_100 + 1382 + 110);
        assert_eq!(s.prev7.tokens, 0);
        assert!(s.unpriced_models.is_empty(), "11-03 不在窗口内");

        // 今天取 11-10：近 7 天为 11-04..11-10，11-01..11-03 落在上一个 7 天
        let s = summary(&conn, &p, d("2026-11-10")).unwrap();
        assert_eq!(s.today.tokens, 0);
        assert_eq!(s.last7.tokens, 0);
        assert_eq!(s.prev7.tokens, 150_100 + 1382 + 110 + 2);
        assert_eq!(s.last30.tokens, s.prev7.tokens);
        assert_eq!(s.unpriced_models, vec!["unknown-model-x".to_string()]);
    }

    #[test]
    fn csv_export() {
        let (_t, _d, conn) = scanned_db();
        let p = PricingTable::builtin();
        let csv = to_csv(&daily_usage(&conn, &p, &filter("2026-11-01", "2026-11-03")).unwrap());
        let lines: Vec<&str> = csv.split("\r\n").filter(|l| !l.is_empty()).collect();
        assert!(lines[0].starts_with("\u{feff}日期,模型"));
        assert_eq!(lines.len(), 5);
        assert!(lines[1].starts_with("2026-11-03,unknown-model-x,1,1,"), "{}", lines[1]);
        assert!(lines[1].ends_with(",否"));
        assert_eq!(lines[3], "2026-11-01,claude-opus-5-5,12,70,100,200,1000,1382,0.003748,是");
        assert_eq!(csv_field(r#"a,"b""#), r#""a,""b""""#);
    }

    #[test]
    fn lists_projects_and_models() {
        let (_t, _d, conn) = scanned_db();
        assert_eq!(list_projects(&conn).unwrap(), vec![r"C:\demo\proj".to_string(), r"D:\other".to_string()]);
        assert_eq!(list_models(&conn).unwrap().len(), 4);
    }
}
