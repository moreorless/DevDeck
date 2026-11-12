//! 解析 `claude -p /usage` 的文本输出（思路参考 magpie，MIT）：
//!
//! ```text
//! Current session: 42% used · resets Nov 12, 2am (America/Denver)
//! Current week (all models): 91% used · resets Nov 12, 2pm (America/Denver)
//! Current week (Fable): 95% used · resets Nov 12, 2pm (America/Denver)
//! ```
//!
//! 输出格式没有文档，按行宽松匹配；认不出的行忽略。

use chrono::{DateTime, Datelike, Duration, LocalResult, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};

/// 一个额度窗口
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindow {
    /// session（5 小时）| week（7 天）
    pub kind: String,
    /// 周额度的范围：None 为全部模型，否则为模型名（如 Fable）
    pub scope: Option<String>,
    pub used_percent: f64,
    /// 重置时间（UTC，RFC 3339）；解析不了时为 None，界面显示原文
    pub resets_at: Option<String>,
    /// 原始的重置时间文字，如 `Nov 12, 2am (America/Denver)`
    pub resets_text: Option<String>,
}

/// 解析结果
#[derive(Debug, PartialEq)]
pub enum Parsed {
    Windows(Vec<QuotaWindow>),
    /// 运行正常但没有额度信息（如 API Key 计费）；附带首行说明
    NoQuota(String),
    /// 账号问题（未登录、凭据过期等）
    Denied(String),
}

/// 账号级错误的关键词（小写匹配）
const DENIED: &[&str] = &[
    "not logged in",
    "not signed in",
    "signed out",
    "sign-in expired",
    "sign-in has expired",
    "unauthorized",
    "forbidden",
    "authentication failed",
    "authentication required",
    "invalid token",
    "invalid access token",
    "please run /login",
    " 401",
    " 403",
];

pub fn parse(text: &str, now: DateTime<Utc>) -> Parsed {
    let text = strip_ansi(text);
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let mut out: Vec<QuotaWindow> = Vec::new();
    for line in &lines {
        if let Some(w) = parse_line(line, now)
            && !out.iter().any(|x| x.kind == w.kind && x.scope == w.scope)
        {
            out.push(w);
        }
    }
    if !out.is_empty() {
        return Parsed::Windows(out);
    }
    if let Some(l) = lines.iter().find(|l| {
        let low = format!(" {}", l.to_lowercase());
        DENIED.iter().any(|k| low.contains(k))
    }) {
        return Parsed::Denied(clip(l));
    }
    Parsed::NoQuota(clip(lines.iter().find(|l| !l.is_empty()).copied().unwrap_or("")))
}

fn parse_line(line: &str, now: DateTime<Utc>) -> Option<QuotaWindow> {
    let rest = line.strip_prefix("Current ")?;
    let (head, tail) = rest.split_once(':')?;
    let (kind, scope, span) = if head == "session" {
        ("session", None, Duration::hours(5))
    } else {
        let h = head.strip_prefix("week")?;
        let scope = h.trim().strip_prefix('(').and_then(|s| s.strip_suffix(')')).map(str::trim);
        let scope = scope.filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("all models"));
        ("week", scope.map(str::to_string), Duration::days(7))
    };
    let (pct_part, reset_part) = match tail.split_once('·') {
        Some((a, b)) => (a, Some(b)),
        None => (tail, None),
    };
    let used_percent: f64 = pct_part.trim().strip_suffix("used")?.trim().strip_suffix('%')?.trim().parse().ok()?;
    let resets_text = reset_part
        .and_then(|r| r.trim().strip_prefix("resets"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let resets_at = resets_text
        .as_deref()
        .and_then(|s| reset_time(s, now, span))
        .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
    Some(QuotaWindow { kind: kind.into(), scope, used_percent, resets_at, resets_text })
}

/// 解析 `Nov 12, 2am (America/Denver)`、`Oct 1 at 3:30pm (Asia/Shanghai)`、`Jan 2, 2027, 3pm (UTC)`、
/// `3pm (Asia/Shanghai)`。时区名认不出时按本机时区。没有年份时取让重置时间落在
/// （现在 - 1 天, 现在 + 窗口长度 + 1 天] 内的那一年；只有时刻时取下一次出现
fn reset_time(s: &str, now: DateTime<Utc>, span: Duration) -> Option<DateTime<Utc>> {
    let (body, zone) = match s.rfind('(') {
        Some(i) if s.ends_with(')') => (s[..i].trim(), Some(&s[i + 1..s.len() - 1])),
        _ => (s.trim(), None),
    };
    match zone.and_then(|z| z.parse::<chrono_tz::Tz>().ok()) {
        Some(tz) => reset_in(body, &tz, now, span),
        None => reset_in(body, &chrono::Local, now, span),
    }
}

fn reset_in<Z: TimeZone>(body: &str, tz: &Z, now: DateTime<Utc>, span: Duration) -> Option<DateTime<Utc>> {
    let cleaned = body.replace(" at ", " ").replace(',', " ");
    let tokens: Vec<&str> = cleaned.split_whitespace().collect();
    let (h, m) = clock(tokens.last()?)?;
    let at = |d: NaiveDate| -> Option<DateTime<Utc>> {
        let naive = d.and_hms_opt(h, m, 0)?;
        match tz.from_local_datetime(&naive) {
            LocalResult::Single(t) => Some(t.with_timezone(&Utc)),
            // 夏令时回拨时同一时刻出现两次，取较晚的（尚未过去的那次）
            LocalResult::Ambiguous(_, b) => Some(b.with_timezone(&Utc)),
            LocalResult::None => None,
        }
    };
    let today = now.with_timezone(tz).date_naive();
    let latest = now + span + Duration::days(1);
    if tokens.len() == 1 {
        // 只有时刻：今天、明天、后天中第一个还没过去的
        return (0..=2).filter_map(|d| at(today + Duration::days(d))).find(|t| *t > now - Duration::hours(1));
    }
    let month = month_of(tokens[0])?;
    let day: u32 = tokens.get(1)?.parse().ok()?;
    if tokens.len() >= 4
        && let Ok(year) = tokens[2].parse::<i32>()
    {
        return at(NaiveDate::from_ymd_opt(year, month, day)?);
    }
    let mut past = None;
    for y in [today.year() - 1, today.year(), today.year() + 1] {
        let Some(t) = NaiveDate::from_ymd_opt(y, month, day).and_then(at) else { continue };
        if t > now - Duration::days(1) && t <= latest {
            return Some(t);
        }
        if t <= now {
            past = Some(t);
        }
    }
    past
}

/// `2am`、`3:30pm`、`14:00`
fn clock(s: &str) -> Option<(u32, u32)> {
    let low = s.to_ascii_lowercase();
    let (digits, pm) = if let Some(d) = low.strip_suffix("pm") {
        (d, Some(true))
    } else if let Some(d) = low.strip_suffix("am") {
        (d, Some(false))
    } else {
        (low.as_str(), None)
    };
    let (h, m) = match digits.split_once(':') {
        Some((h, m)) => (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?),
        None => (digits.parse::<u32>().ok()?, 0),
    };
    // 12 小时制的小时只能是 1–12
    if pm.is_some() && !(1..=12).contains(&h) {
        return None;
    }
    let h = match pm {
        Some(true) if h < 12 => h + 12,
        Some(false) if h == 12 => 0,
        _ => h,
    };
    (h < 24 && m < 60).then_some((h, m))
}

fn month_of(s: &str) -> Option<u32> {
    const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    let p = s.get(..3)?.to_ascii_lowercase();
    MONTHS.iter().position(|m| *m == p).map(|i| i as u32 + 1)
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn clip(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() > 160 { format!("{}…", s.chars().take(160).collect::<String>()) } else { s.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-11-12 本机实测输出（截取）
    const REAL: &str = "You are currently using your subscription to power your Claude Code usage

Current session: 42% used · resets Nov 12, 2am (America/Denver)
Current week (all models): 91% used · resets Nov 12, 2pm (America/Denver)
Current week (Fable): 95% used · resets Nov 12, 2pm (America/Denver)

What's contributing to your usage?
Approximate · sessions on this machine only · shares overlap
  93%  Give \"general-purpose\" subagents a cheaper model
";

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn parses_real_output() {
        // 当地时间 2026-11-11 23:40（UTC-7）
        let now = utc("2026-11-12T06:40:00Z");
        let Parsed::Windows(w) = parse(REAL, now) else { panic!("应解析出窗口") };
        assert_eq!(w.len(), 3);
        assert_eq!((w[0].kind.as_str(), w[0].scope.as_deref(), w[0].used_percent), ("session", None, 42.0));
        assert_eq!(w[0].resets_at.as_deref(), Some("2026-11-12T09:00:00Z"), "2am MST = 09:00 UTC");
        assert_eq!((w[1].kind.as_str(), w[1].scope.as_deref(), w[1].used_percent), ("week", None, 91.0));
        assert_eq!(w[1].resets_at.as_deref(), Some("2026-11-12T21:00:00Z"));
        assert_eq!((w[2].scope.as_deref(), w[2].used_percent), (Some("Fable"), 95.0));
        assert_eq!(w[2].resets_text.as_deref(), Some("Nov 12, 2pm (America/Denver)"));
    }

    #[test]
    fn reset_formats() {
        let now = utc("2026-09-30T00:00:00Z");
        let five = Duration::hours(5);
        let week = Duration::days(7);
        assert_eq!(reset_time("Oct 1 at 3:30pm (Asia/Shanghai)", now, week), Some(utc("2026-10-01T07:30:00Z")));
        assert_eq!(reset_time("Oct 1, 2027, 3pm (UTC)", now, week), Some(utc("2027-10-01T15:00:00Z")));
        assert_eq!(reset_time("3pm (UTC)", now, five), Some(utc("2026-09-30T15:00:00Z")));
        // 年末读取到 1 月的重置时间 → 下一年
        let dec = utc("2026-12-30T00:00:00Z");
        assert_eq!(reset_time("Jan 2, 9am (UTC)", dec, week), Some(utc("2027-01-02T09:00:00Z")));
        // 12am / 12pm
        assert_eq!(clock("12am"), Some((0, 0)));
        assert_eq!(clock("12:15pm"), Some((12, 15)));
        assert_eq!(clock("13pm"), None);
    }

    #[test]
    fn no_reset_and_ansi() {
        let now = utc("2026-11-12T06:40:00Z");
        let Parsed::Windows(w) = parse("\u{1b}[1mCurrent week (Fable): 0% used\u{1b}[0m", now) else { panic!() };
        assert_eq!((w[0].used_percent, w[0].resets_at.as_deref(), w[0].resets_text.as_deref()), (0.0, None, None));
    }

    #[test]
    fn no_quota_and_denied() {
        let now = Utc::now();
        assert_eq!(
            parse("You are currently using your subscription to power your Claude Code usage\n", now),
            Parsed::NoQuota("You are currently using your subscription to power your Claude Code usage".into())
        );
        assert!(matches!(parse("Not logged in · Please run /login", now), Parsed::Denied(_)));
        assert!(matches!(parse("API Error: 401 Unauthorized", now), Parsed::Denied(_)));
        // 有窗口时，后面说明里出现的关键词不影响
        assert!(matches!(parse(&format!("{REAL}\n  Trim C:\\x\\403\\CLAUDE.md"), now), Parsed::Windows(_)));
    }
}
