//! 会话对话记录：读取单个 jsonl，沿消息树取主线并转换成前端可直接渲染的条目。
//!
//! 主线：从文件中最后一条（非 sidechain）消息出发，沿 `parentUuid` 回溯；
//! 遇到压缩边界（`parentUuid` 为空）时改走 `logicalParentUuid`，把压缩前的历史接上。
//! 被回退、编辑掉的分支不在主线上，不展示。

use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;

use super::parser::clean_prompt_text;

/// 单段文本上限，超出截断（工具输出可能非常长）
const TEXT_LIMIT: usize = 20_000;
const INPUT_LIMIT: usize = 10_000;
const META_LIMIT: usize = 5_000;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Clipped {
    pub text: String,
    /// 原始字符数；大于 text 的长度说明被截断
    pub full_len: usize,
}

impl Clipped {
    fn new(s: &str, limit: usize) -> Self {
        let full_len = s.chars().count();
        let text = if full_len > limit { s.chars().take(limit).collect() } else { s.to_string() };
        Self { text, full_len }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResult {
    pub content: Clipped,
    pub is_error: bool,
    /// 结果中的图片数量（不传图片本身）
    pub images: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Item {
    /// 用户输入
    User { uuid: String, ts: Option<String>, text: String, images: usize },
    /// 助手回复的文本
    Assistant { uuid: String, ts: Option<String>, model: Option<String>, text: String },
    Thinking { uuid: String, ts: Option<String>, text: Clipped },
    ToolUse {
        uuid: String,
        ts: Option<String>,
        id: String,
        name: String,
        input: Clipped,
        result: Option<ToolResult>,
        /// Agent 工具启动的子代理 id
        agent_id: Option<String>,
        /// 子代理日志文件是否存在
        agent_available: bool,
    },
    /// 压缩点：之前的对话被压缩为摘要
    Compact { uuid: String, ts: Option<String>, trigger: Option<String>, pre_tokens: Option<u64>, summary: Option<Clipped> },
    /// 系统注入内容（meta、命令输出、hook 等），默认隐藏
    Meta { uuid: String, ts: Option<String>, label: String, text: Clipped },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Transcript {
    pub items: Vec<Item>,
    /// 文件中解析失败的行数
    pub bad_lines: usize,
}

/// 读取主会话文件。`subagent_dir` 为 `<sessionId>/subagents`，用于判断子代理日志是否存在
pub fn read_transcript(path: &Path, subagent_dir: Option<&Path>, sidechain: bool) -> Result<Transcript> {
    let file = File::open(path).with_context(|| format!("无法打开 {}", path.display()))?;
    let mut records: Vec<Value> = Vec::new();
    let mut bad_lines = 0;
    for line in BufReader::new(file).split(b'\n') {
        let line = line?;
        let s = String::from_utf8_lossy(&line);
        if s.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(&s) {
            Ok(v) if v.is_object() => records.push(v),
            _ => bad_lines += 1,
        }
    }

    let chain = main_chain(&records, sidechain);
    let results = collect_tool_results(&records);
    let mut items = Vec::new();
    for idx in chain {
        convert(&records[idx], &results, subagent_dir, &mut items);
    }
    Ok(Transcript { items, bad_lines })
}

fn str_of<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str).filter(|s| !s.is_empty())
}

fn is_message(v: &Value) -> bool {
    matches!(str_of(v, "type"), Some("user" | "assistant" | "system" | "attachment")) && str_of(v, "uuid").is_some()
}

/// 主线上记录的下标，按时间正序
fn main_chain(records: &[Value], sidechain: bool) -> Vec<usize> {
    let by_uuid: HashMap<&str, usize> =
        records.iter().enumerate().filter_map(|(i, r)| str_of(r, "uuid").map(|u| (u, i))).collect();
    let leaf = records.iter().rposition(|r| {
        is_message(r) && r.get("isSidechain").and_then(Value::as_bool).unwrap_or(false) == sidechain
    });
    let mut chain = Vec::new();
    let mut seen = HashSet::new();
    let mut cur = leaf;
    while let Some(i) = cur {
        if !seen.insert(i) {
            break; // 防御环
        }
        chain.push(i);
        let r = &records[i];
        let next = str_of(r, "parentUuid").or_else(|| str_of(r, "logicalParentUuid"));
        cur = next.and_then(|u| by_uuid.get(u).copied());
    }
    chain.reverse();
    chain
}

struct RawResult {
    result: ToolResult,
    agent_id: Option<String>,
}

/// tool_use_id → 工具结果（含子代理 id）
fn collect_tool_results(records: &[Value]) -> HashMap<String, RawResult> {
    let mut out = HashMap::new();
    for r in records.iter().filter(|r| str_of(r, "type") == Some("user")) {
        let Some(blocks) = r.get("message").and_then(|m| m.get("content")).and_then(Value::as_array) else {
            continue;
        };
        let agent_id = r.get("toolUseResult").and_then(|t| str_of(t, "agentId")).map(str::to_string);
        for b in blocks.iter().filter(|b| str_of(b, "type") == Some("tool_result")) {
            let Some(id) = str_of(b, "tool_use_id") else { continue };
            let (text, images) = flatten_content(b.get("content"));
            out.insert(
                id.to_string(),
                RawResult {
                    result: ToolResult {
                        content: Clipped::new(&text, TEXT_LIMIT),
                        is_error: b.get("is_error").and_then(Value::as_bool).unwrap_or(false),
                        images,
                    },
                    agent_id: agent_id.clone(),
                },
            );
        }
    }
    out
}

/// 内容可能是字符串或块数组：拼接文本，统计图片
fn flatten_content(c: Option<&Value>) -> (String, usize) {
    match c {
        Some(Value::String(s)) => (s.clone(), 0),
        Some(Value::Array(blocks)) => {
            let mut texts = Vec::new();
            let mut images = 0;
            for b in blocks {
                match str_of(b, "type") {
                    Some("text") => texts.push(str_of(b, "text").unwrap_or("").to_string()),
                    Some("image") => images += 1,
                    _ => {}
                }
            }
            (texts.join("\n"), images)
        }
        _ => (String::new(), 0),
    }
}

fn convert(r: &Value, results: &HashMap<String, RawResult>, subagent_dir: Option<&Path>, items: &mut Vec<Item>) {
    let uuid = str_of(r, "uuid").unwrap_or_default().to_string();
    let ts = str_of(r, "timestamp").map(str::to_string);
    let flag = |k: &str| r.get(k).and_then(Value::as_bool).unwrap_or(false);
    let content = r.get("message").and_then(|m| m.get("content"));

    match str_of(r, "type") {
        Some("user") => {
            if flag("isCompactSummary") {
                let (text, _) = flatten_content(content);
                let summary = Some(Clipped::new(&text, TEXT_LIMIT));
                // 摘要紧跟在压缩边界之后：合并到边界条目
                if let Some(Item::Compact { summary: s @ None, .. }) = items.last_mut() {
                    *s = summary;
                } else {
                    items.push(Item::Compact { uuid, ts, trigger: None, pre_tokens: None, summary });
                }
                return;
            }
            // 去掉工具结果块（已挂到对应的工具调用上），剩余的是用户输入
            let (text, images) = match content {
                Some(Value::Array(blocks)) => {
                    let rest: Vec<Value> =
                        blocks.iter().filter(|b| str_of(b, "type") != Some("tool_result")).cloned().collect();
                    if rest.is_empty() {
                        return;
                    }
                    flatten_content(Some(&Value::Array(rest)))
                }
                other => flatten_content(other),
            };
            let cleaned = if flag("isMeta") { None } else { clean_prompt_text(&text) };
            match cleaned {
                Some(t) => items.push(Item::User { uuid, ts, text: t, images }),
                None if !text.trim().is_empty() => {
                    items.push(Item::Meta { uuid, ts, label: "系统注入".into(), text: Clipped::new(&text, META_LIMIT) })
                }
                None => {}
            }
        }
        Some("assistant") => {
            let model = r.get("message").and_then(|m| str_of(m, "model")).map(str::to_string);
            let synthetic = model.as_deref() == Some("<synthetic>");
            let Some(blocks) = content.and_then(Value::as_array) else { return };
            for b in blocks {
                match str_of(b, "type") {
                    Some("text") => {
                        let text = str_of(b, "text").unwrap_or("").to_string();
                        if text.trim().is_empty() {
                            continue;
                        }
                        if synthetic {
                            items.push(Item::Meta {
                                uuid: uuid.clone(),
                                ts: ts.clone(),
                                label: "合成消息".into(),
                                text: Clipped::new(&text, META_LIMIT),
                            });
                        } else {
                            items.push(Item::Assistant { uuid: uuid.clone(), ts: ts.clone(), model: model.clone(), text });
                        }
                    }
                    Some("thinking") => {
                        if let Some(t) = str_of(b, "thinking") {
                            items.push(Item::Thinking { uuid: uuid.clone(), ts: ts.clone(), text: Clipped::new(t, TEXT_LIMIT) });
                        }
                    }
                    Some("tool_use") => {
                        let id = str_of(b, "id").unwrap_or_default().to_string();
                        let input = b
                            .get("input")
                            .map(|v| serde_json::to_string_pretty(v).unwrap_or_default())
                            .unwrap_or_default();
                        let raw = results.get(&id);
                        let agent_id = raw.and_then(|r| r.agent_id.clone());
                        let agent_available = match (&agent_id, subagent_dir) {
                            (Some(a), Some(dir)) => dir.join(format!("agent-{a}.jsonl")).is_file(),
                            _ => false,
                        };
                        items.push(Item::ToolUse {
                            uuid: uuid.clone(),
                            ts: ts.clone(),
                            id,
                            name: str_of(b, "name").unwrap_or("?").to_string(),
                            input: Clipped::new(&input, INPUT_LIMIT),
                            result: raw.map(|r| r.result.clone()),
                            agent_id,
                            agent_available,
                        });
                    }
                    _ => {}
                }
            }
        }
        Some("system") => {
            if str_of(r, "subtype") == Some("compact_boundary") {
                let meta = r.get("compactMetadata");
                items.push(Item::Compact {
                    uuid,
                    ts,
                    trigger: meta.and_then(|m| str_of(m, "trigger")).map(str::to_string),
                    pre_tokens: meta.and_then(|m| m.get("preTokens")).and_then(Value::as_u64),
                    summary: None,
                });
            } else {
                let text = str_of(r, "content").unwrap_or("").to_string();
                if !text.trim().is_empty() {
                    let label = format!("系统 · {}", str_of(r, "subtype").unwrap_or("info"));
                    items.push(Item::Meta { uuid, ts, label, text: Clipped::new(&text, META_LIMIT) });
                }
            }
        }
        Some("attachment") => {
            let a = r.get("attachment");
            let label = format!("附件 · {}", a.and_then(|a| str_of(a, "type")).unwrap_or("unknown"));
            let text = a.map(|a| serde_json::to_string_pretty(a).unwrap_or_default()).unwrap_or_default();
            items.push(Item::Meta { uuid, ts, label, text: Clipped::new(&text, META_LIMIT) });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude_log::test_support::fixture_root;

    fn kinds(t: &Transcript) -> Vec<&'static str> {
        t.items
            .iter()
            .map(|i| match i {
                Item::User { .. } => "user",
                Item::Assistant { .. } => "assistant",
                Item::Thinking { .. } => "thinking",
                Item::ToolUse { .. } => "tool",
                Item::Compact { .. } => "compact",
                Item::Meta { .. } => "meta",
            })
            .collect()
    }

    fn write(dir: &Path, name: &str, lines: &[&str]) -> std::path::PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, lines.join("\n") + "\n").unwrap();
        p
    }

    #[test]
    fn fixture_session_main_line() {
        let dir = fixture_root().join("projects").join("C--demo-proj");
        let t = read_transcript(&dir.join("sess-a.jsonl"), Some(&dir.join("sess-a").join("subagents")), false).unwrap();
        assert_eq!(t.bad_lines, 1);
        assert_eq!(kinds(&t), vec!["user", "thinking", "tool", "assistant", "meta", "user", "assistant"]);
        match &t.items[2] {
            Item::ToolUse { name, result: Some(r), .. } => {
                assert_eq!(name, "Read");
                assert_eq!(r.content.text, "文件内容");
            }
            other => panic!("{other:?}"),
        }
        match &t.items[0] {
            Item::User { text, .. } => assert_eq!(text, "实现登录功能"),
            other => panic!("{other:?}"),
        }
        match &t.items[5] {
            Item::User { text, .. } => assert_eq!(text, "/clear"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn follows_branch_compaction_and_subagent() {
        let tmp = tempfile::tempdir().unwrap();
        let sub = tmp.path().join("s").join("subagents");
        std::fs::create_dir_all(&sub).unwrap();
        write(&sub, "agent-ag1.jsonl", &[r#"{"type":"user","uuid":"x","isSidechain":true,"message":{"content":"子任务"}}"#]);
        let main = write(
            tmp.path(),
            "s.jsonl",
            &[
                r#"{"type":"user","uuid":"1","parentUuid":null,"message":{"content":"第一问"}}"#,
                r#"{"type":"assistant","uuid":"2","parentUuid":"1","message":{"model":"m","content":[{"type":"text","text":"旧回答"}]}}"#,
                // 回退后从 1 重新分叉
                r#"{"type":"assistant","uuid":"2b","parentUuid":"1","message":{"model":"m","content":[{"type":"tool_use","id":"t1","name":"Agent","input":{"prompt":"p"}}]}}"#,
                r#"{"type":"user","uuid":"3","parentUuid":"2b","toolUseResult":{"agentId":"ag1"},"message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"完成"},{"type":"image","source":{}}]}]}}"#,
                r#"{"type":"system","subtype":"compact_boundary","uuid":"4","parentUuid":null,"logicalParentUuid":"3","compactMetadata":{"trigger":"auto","preTokens":290975}}"#,
                r#"{"type":"user","uuid":"5","parentUuid":"4","isCompactSummary":true,"message":{"content":"摘要内容"}}"#,
                r#"{"type":"user","uuid":"6","parentUuid":"5","message":{"content":"压缩后提问"}}"#,
                r#"{"type":"user","uuid":"side","parentUuid":"6","isSidechain":true,"message":{"content":"不在主线"}}"#,
            ],
        );
        let t = read_transcript(&main, Some(&sub), false).unwrap();
        assert_eq!(kinds(&t), vec!["user", "tool", "compact", "user"], "旧分支「旧回答」不在主线");
        match &t.items[1] {
            Item::ToolUse { agent_id, agent_available, result: Some(r), .. } => {
                assert_eq!(agent_id.as_deref(), Some("ag1"));
                assert!(agent_available);
                assert_eq!(r.images, 1);
            }
            other => panic!("{other:?}"),
        }
        match &t.items[2] {
            Item::Compact { trigger, pre_tokens, summary: Some(s), .. } => {
                assert_eq!((trigger.as_deref(), *pre_tokens, s.text.as_str()), (Some("auto"), Some(290975), "摘要内容"));
            }
            other => panic!("{other:?}"),
        }

        let st = read_transcript(&sub.join("agent-ag1.jsonl"), None, true).unwrap();
        assert_eq!(kinds(&st), vec!["user"]);
    }

    #[test]
    fn long_text_is_clipped() {
        let c = Clipped::new(&"字".repeat(30), 10);
        assert_eq!((c.text.chars().count(), c.full_len), (10, 30));
    }
}
