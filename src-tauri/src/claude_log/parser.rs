//! Claude Code jsonl 单行解析。
//!
//! 日志格式没有官方文档，这里只取需要的字段：解析失败返回 None，未知类型与字段一律忽略。

use chrono::{DateTime, Utc};
use serde_json::Value;

/// 去重后计费的最小单位：一次 API 响应的用量
#[derive(Debug, Clone, PartialEq)]
pub struct UsageRecord {
    /// `message.id`；缺失时为 `uuid:<记录 uuid>`
    pub message_id: String,
    /// `requestId`；缺失时为空串
    pub request_id: String,
    pub model: String,
    pub ts: DateTime<Utc>,
    pub input: u64,
    pub output: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
    pub cache_read: u64,
    pub speed: Option<String>,
    pub geo: Option<String>,
}

impl UsageRecord {
    /// 计费口径的 prompt 长度：input + cache 写 + cache 读
    pub fn prompt_tokens(&self) -> u64 {
        self.input + self.cache_write_5m + self.cache_write_1h + self.cache_read
    }
}

/// 一行日志中与用量、会话索引相关的信息
#[derive(Debug, Default, Clone, PartialEq)]
pub struct LineInfo {
    pub ts: Option<DateTime<Utc>>,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub usage: Option<UsageRecord>,
    /// 助手消息使用的模型（不含 `<synthetic>`）
    pub model: Option<String>,
    /// 用户真正输入的提示（已去掉系统注入内容）
    pub user_prompt: Option<String>,
    pub ai_title: Option<String>,
    pub custom_title: Option<String>,
    pub last_prompt: Option<String>,
}

const SYNTHETIC_MODEL: &str = "<synthetic>";
const PROMPT_MAX_CHARS: usize = 500;

/// 解析一行；不是合法 JSON 对象时返回 None
pub fn parse_line(line: &str) -> Option<LineInfo> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    let obj = v.as_object()?;
    let ty = obj.get("type").and_then(Value::as_str).unwrap_or("");

    let mut info = LineInfo {
        ts: obj
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|d| d.with_timezone(&Utc)),
        cwd: non_empty_str(obj.get("cwd")),
        git_branch: non_empty_str(obj.get("gitBranch")),
        ..Default::default()
    };

    match ty {
        "assistant" => {
            let msg = obj.get("message");
            let model = msg.and_then(|m| non_empty_str(m.get("model")));
            if let Some(m) = &model {
                if m != SYNTHETIC_MODEL {
                    info.model = Some(m.clone());
                    info.usage = parse_usage(&v, m, info.ts);
                }
            }
        }
        "user" => info.user_prompt = extract_user_prompt(&v),
        "ai-title" => info.ai_title = non_empty_str(obj.get("aiTitle")),
        "custom-title" => info.custom_title = non_empty_str(obj.get("customTitle")),
        "last-prompt" => info.last_prompt = non_empty_str(obj.get("lastPrompt")).map(truncate),
        _ => {}
    }
    Some(info)
}

fn parse_usage(v: &Value, model: &str, ts: Option<DateTime<Utc>>) -> Option<UsageRecord> {
    let ts = ts?;
    let msg = v.get("message")?;
    let usage = msg.get("usage")?.as_object()?;
    let n = |o: &serde_json::Map<String, Value>, k: &str| o.get(k).and_then(Value::as_u64).unwrap_or(0);

    let cache_total = n(usage, "cache_creation_input_tokens");
    let (w5m, w1h) = match usage.get("cache_creation").and_then(Value::as_object) {
        Some(cc) => {
            let a = n(cc, "ephemeral_5m_input_tokens");
            let b = n(cc, "ephemeral_1h_input_tokens");
            // 拆分不完整时，剩余部分按 5m 计
            (a + cache_total.saturating_sub(a + b), b)
        }
        None => (cache_total, 0),
    };

    let message_id = non_empty_str(msg.get("id"))
        .or_else(|| non_empty_str(v.get("uuid")).map(|u| format!("uuid:{u}")))?;
    Some(UsageRecord {
        message_id,
        request_id: non_empty_str(v.get("requestId")).unwrap_or_default(),
        model: model.to_string(),
        ts,
        input: n(usage, "input_tokens"),
        output: n(usage, "output_tokens"),
        cache_write_5m: w5m,
        cache_write_1h: w1h,
        cache_read: n(usage, "cache_read_input_tokens"),
        speed: non_empty_str(usage.get("speed")),
        geo: non_empty_str(usage.get("inference_geo")),
    })
}

/// 提取用户真正输入的文本。工具结果、meta、压缩摘要、纯系统注入内容返回 None
fn extract_user_prompt(v: &Value) -> Option<String> {
    let flag = |k: &str| v.get(k).and_then(Value::as_bool).unwrap_or(false);
    if flag("isMeta") || flag("isCompactSummary") || flag("isSidechain") {
        return None;
    }
    let content = v.get("message")?.get("content")?;
    let texts: Vec<&str> = match content {
        Value::String(s) => vec![s.as_str()],
        Value::Array(blocks) => {
            if blocks.iter().any(|b| b.get("type").and_then(Value::as_str) == Some("tool_result")) {
                return None;
            }
            blocks
                .iter()
                .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|b| b.get("text").and_then(Value::as_str))
                .collect()
        }
        _ => return None,
    };
    let parts: Vec<String> = texts.into_iter().filter_map(clean_prompt_text).collect();
    if parts.is_empty() { None } else { Some(truncate(parts.join("\n"))) }
}

/// 去掉系统注入的标签块；斜杠命令转成 `/name args`
pub(crate) fn clean_prompt_text(s: &str) -> Option<String> {
    if let Some(name) = tag_inner(s, "command-name") {
        let args = tag_inner(s, "command-args").unwrap_or_default();
        let cmd = format!("{} {}", name.trim(), args.trim());
        return Some(cmd.trim().to_string());
    }
    let mut out = s.to_string();
    for tag in ["system-reminder", "local-command-caveat", "local-command-stdout", "local-command-stderr"] {
        out = strip_tag_blocks(&out, tag);
    }
    let out = out.trim();
    // 其余以标签开头的内容（如 task-notification）都是系统注入
    if out.is_empty() || out.starts_with('<') { None } else { Some(out.to_string()) }
}

fn tag_inner(s: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = s.find(&open)? + open.len();
    let end = s[start..].find(&close)? + start;
    Some(s[start..end].to_string())
}

fn strip_tag_blocks(s: &str, tag: &str) -> String {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find(&open) {
        out.push_str(&rest[..i]);
        match rest[i..].find(&close) {
            Some(j) => rest = &rest[i + j + close.len()..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

fn non_empty_str(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string)
}

fn truncate(s: String) -> String {
    if s.chars().count() <= PROMPT_MAX_CHARS {
        s
    } else {
        let mut t: String = s.chars().take(PROMPT_MAX_CHARS).collect();
        t.push('…');
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assistant(usage: &str, model: &str) -> String {
        format!(
            r#"{{"type":"assistant","timestamp":"2026-11-01T10:00:00.000Z","cwd":"C:\\proj","requestId":"req_1","uuid":"u1","message":{{"id":"msg_1","model":"{model}","usage":{usage}}}}}"#
        )
    }

    #[test]
    fn parses_usage_with_cache_split() {
        let line = assistant(
            r#"{"input_tokens":10,"output_tokens":20,"cache_creation_input_tokens":300,"cache_read_input_tokens":40,"cache_creation":{"ephemeral_5m_input_tokens":100,"ephemeral_1h_input_tokens":200},"speed":"standard","inference_geo":"global"}"#,
            "claude-opus-5-5",
        );
        let u = parse_line(&line).unwrap().usage.unwrap();
        assert_eq!((u.input, u.output, u.cache_write_5m, u.cache_write_1h, u.cache_read), (10, 20, 100, 200, 40));
        assert_eq!(u.message_id, "msg_1");
        assert_eq!(u.request_id, "req_1");
        assert_eq!(u.speed.as_deref(), Some("standard"));
        assert_eq!(u.prompt_tokens(), 350);
    }

    #[test]
    fn cache_without_split_counts_as_5m() {
        let line = assistant(r#"{"input_tokens":1,"output_tokens":1,"cache_creation_input_tokens":50}"#, "m");
        let u = parse_line(&line).unwrap().usage.unwrap();
        assert_eq!((u.cache_write_5m, u.cache_write_1h), (50, 0));
    }

    #[test]
    fn synthetic_model_is_ignored() {
        let line = assistant(r#"{"input_tokens":1,"output_tokens":1}"#, "<synthetic>");
        let info = parse_line(&line).unwrap();
        assert!(info.usage.is_none());
        assert!(info.model.is_none());
    }

    #[test]
    fn invalid_json_returns_none() {
        assert!(parse_line("{not json").is_none());
        assert!(parse_line("[1,2]").is_none());
    }

    #[test]
    fn unknown_type_is_tolerated() {
        let info = parse_line(r#"{"type":"brand-new-type","foo":1}"#).unwrap();
        assert_eq!(info, LineInfo::default());
    }

    #[test]
    fn user_prompt_strips_system_reminder() {
        let line = r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"<system-reminder>x</system-reminder>"},{"type":"text","text":"总结当前项目"}]}}"#;
        assert_eq!(parse_line(line).unwrap().user_prompt.as_deref(), Some("总结当前项目"));
    }

    #[test]
    fn user_prompt_slash_command() {
        let line = r#"{"type":"user","message":{"content":"<command-name>/login</command-name>\n<command-message>login</command-message>\n<command-args></command-args>"}}"#;
        assert_eq!(parse_line(line).unwrap().user_prompt.as_deref(), Some("/login"));
    }

    #[test]
    fn tool_result_and_meta_are_not_prompts() {
        let tool = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t","content":"ok"}]}}"#;
        let meta = r#"{"type":"user","isMeta":true,"message":{"content":"hi"}}"#;
        let notify = r#"{"type":"user","message":{"content":"<task-notification>done</task-notification>"}}"#;
        assert!(parse_line(tool).unwrap().user_prompt.is_none());
        assert!(parse_line(meta).unwrap().user_prompt.is_none());
        assert!(parse_line(notify).unwrap().user_prompt.is_none());
    }

    #[test]
    fn titles_and_last_prompt() {
        assert_eq!(
            parse_line(r#"{"type":"ai-title","aiTitle":"标题"}"#).unwrap().ai_title.as_deref(),
            Some("标题")
        );
        assert_eq!(
            parse_line(r#"{"type":"custom-title","customTitle":"自定义"}"#).unwrap().custom_title.as_deref(),
            Some("自定义")
        );
        assert_eq!(
            parse_line(r#"{"type":"last-prompt","lastPrompt":"最后"}"#).unwrap().last_prompt.as_deref(),
            Some("最后")
        );
    }

    #[test]
    fn long_prompt_is_truncated() {
        let long = "字".repeat(600);
        let line = format!(r#"{{"type":"user","message":{{"content":"{long}"}}}}"#);
        let p = parse_line(&line).unwrap().user_prompt.unwrap();
        assert_eq!(p.chars().count(), PROMPT_MAX_CHARS + 1);
    }
}
