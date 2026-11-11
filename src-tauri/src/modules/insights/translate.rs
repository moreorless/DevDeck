//! 报告翻译：`/insights` 生成各部分时使用固定的空系统提示，`language` 设置不生效，报告只有英文。
//! 这里只抽取 HTML 中的文本节点（跳过 script / style / 注释），交给 `claude -p` 翻译成简体中文后
//! 按原位置填回，标签、属性、脚本不变。

use std::{collections::HashMap, path::Path, time::Duration};

use anyhow::{Result, anyhow};

use super::cli;

/// 每批最多的字符数（实测报告正文约 2.7 万字符，分 3 批左右）
const BATCH_CHARS: usize = 10_000;
const BATCH_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const MODEL: &str = "sonnet";

const SYSTEM_PROMPT: &str = "你是专业的技术文档翻译。用户会给出一个 JSON 对象，键是编号、值是待翻译文本，内容来自 Claude Code 使用情况报告。\
请把每个值翻译成简体中文，输出键完全相同的 JSON 对象（每个键对应一个字符串，不要拆分或合并条目），不要输出任何其他内容。\
规则：保留 Markdown 标记与换行；多行的 Markdown 文本（例如建议写入 CLAUDE.md 的规则、SKILL.md 内容）\
和可复制给 Claude 的提示词也要翻译其中的自然语言部分；纯代码条目（JSON、脚本、命令行）原样返回；\
命令、代码、文件路径、URL、标识符、YAML 键名、配置项名保持原样；\
产品与功能名（Claude Code、CLAUDE.md、MCP、Hooks、Skills、subagent 等）保持原样；\
数字、百分比、日期保持原样；已经是中文或无需翻译的条目原样返回。";

/// HTML 中一个可翻译的文本节点
#[derive(Debug, PartialEq)]
struct Segment {
    /// 去掉首尾空白后的文本在 html 中的字节范围
    start: usize,
    end: usize,
    /// 解码实体后的文本
    text: String,
}

fn segments(html: &str) -> Vec<Segment> {
    let lower = html.to_ascii_lowercase();
    let bytes = html.as_bytes();
    let mut out = Vec::new();
    let mut text_start = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        push_segment(html, text_start, i, &mut out);
        let rest = &lower[i..];
        let end = if rest.starts_with("<!--") {
            rest.find("-->").map(|j| i + j + 3)
        } else if let Some(tag) = ["<script", "<style"].iter().find(|t| rest.starts_with(**t)) {
            let close = format!("</{}", &tag[1..]);
            rest.find(&close).and_then(|j| lower[i + j..].find('>').map(|k| i + j + k + 1))
        } else {
            rest.find('>').map(|j| i + j + 1)
        }
        .unwrap_or(bytes.len());
        i = end;
        text_start = end;
    }
    push_segment(html, text_start, bytes.len(), &mut out);
    out
}

fn push_segment(html: &str, start: usize, end: usize, out: &mut Vec<Segment>) {
    let raw = &html[start..end];
    let trimmed = raw.trim();
    if !trimmed.bytes().any(|b| b.is_ascii_alphabetic()) {
        return;
    }
    let s = start + (raw.len() - raw.trim_start().len());
    out.push(Segment { start: s, end: s + trimmed.len(), text: decode_entities(trimmed) });
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(semi) = rest.find(';').filter(|j| *j <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let name = &rest[1..semi];
        let ch = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some('\u{a0}'),
            _ if name.starts_with("#x") || name.starts_with("#X") => {
                u32::from_str_radix(&name[2..], 16).ok().and_then(char::from_u32)
            }
            _ if name.starts_with('#') => name[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// 按原位置填回译文；`translated` 以原文为键
fn apply(html: &str, segs: &[Segment], translated: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(html.len() * 2);
    let mut pos = 0;
    for s in segs {
        out.push_str(&html[pos..s.start]);
        match translated.get(&s.text) {
            Some(t) => out.push_str(&escape(t)),
            None => out.push_str(&html[s.start..s.end]),
        }
        pos = s.end;
    }
    out.push_str(&html[pos..]);
    // 页面语言声明
    localize_scripts(&out.replacen("<html lang=\"en\"", "<html lang=\"zh-CN\"", 1))
}

/// 报告脚本中写死的界面文字（时段图表、复制按钮），按固定对照表替换带引号的字面量
const SCRIPT_LITERALS: [(&str, &str); 9] = [
    ("Morning (6-12)", "上午 (6-12)"),
    ("Afternoon (12-18)", "下午 (12-18)"),
    ("Evening (18-24)", "晚上 (18-24)"),
    ("Night (0-6)", "凌晨 (0-6)"),
    ("Copied!", "已复制！"),
    ("Copied ", "已复制 "),
    (" items!", " 项！"),
    ("Copy All Checked", "复制全部勾选项"),
    ("Copy", "复制"),
];

fn localize_scripts(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut pos = 0;
    while let Some(i) = lower[pos..].find("<script").map(|i| pos + i) {
        let Some(end) = lower[i..].find("</script").map(|j| i + j) else { break };
        out.push_str(&html[pos..i]);
        let mut js = html[i..end].to_string();
        for (en, zh) in SCRIPT_LITERALS {
            for q in ['\'', '"'] {
                js = js.replace(&format!("{q}{en}{q}"), &format!("{q}{zh}{q}"));
            }
        }
        out.push_str(&js);
        pos = end;
    }
    out.push_str(&html[pos..]);
    out
}

/// 去重后按字符数分批
fn batches(segs: &[Segment]) -> Vec<Vec<String>> {
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<Vec<String>> = Vec::new();
    let mut size = 0;
    for s in segs {
        if !seen.insert(s.text.as_str()) {
            continue;
        }
        if out.is_empty() || size + s.text.len() > BATCH_CHARS {
            out.push(Vec::new());
            size = 0;
        }
        size += s.text.len();
        out.last_mut().unwrap().push(s.text.clone());
    }
    out
}

/// 从模型输出中取出 JSON 对象（可能带 ``` 包裹），只保留值为字符串的键
fn parse_object(result: &str) -> Result<HashMap<String, String>> {
    let (a, b) = (result.find('{'), result.rfind('}'));
    let json = match (a, b) {
        (Some(a), Some(b)) if a < b => &result[a..=b],
        _ => return Err(anyhow!("翻译结果不是 JSON 对象")),
    };
    let v: HashMap<String, serde_json::Value> =
        serde_json::from_str(json).map_err(|e| anyhow!("翻译结果无法解析：{e}"))?;
    Ok(v.into_iter().filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_string()))).collect())
}

/// 按编号对齐译文：返回（原文 → 译文，缺失的原文）。多出的编号忽略
fn align(batch: &[String], got: &HashMap<String, String>) -> (Vec<(String, String)>, Vec<String>) {
    let mut done = Vec::new();
    let mut missing = Vec::new();
    for (i, src) in batch.iter().enumerate() {
        match got.get(&i.to_string()).filter(|t| !t.trim().is_empty()) {
            Some(t) => done.push((src.clone(), t.clone())),
            None => missing.push(src.clone()),
        }
    }
    (done, missing)
}

/// 调用一次 claude 翻译一批，返回（已对齐的译文，缺失的原文，费用）
/// 一次调用的（已对齐的译文，缺失的原文，费用）
type CallResult = (Vec<(String, String)>, Vec<String>, f64);

fn translate_batch(claude: &Path, cwd: &Path, batch: &[String]) -> Result<CallResult> {
    let input: serde_json::Map<String, serde_json::Value> =
        batch.iter().enumerate().map(|(i, s)| (i.to_string(), serde_json::Value::String(s.clone()))).collect();
    let out = cli::run_json(
        claude,
        &[
            "-p",
            "--model",
            MODEL,
            "--system-prompt",
            SYSTEM_PROMPT,
            "--tools",
            "",
            "--strict-mcp-config",
            "--disable-slash-commands",
            "--no-session-persistence",
            "--settings",
            cli::NO_HOOKS_SETTINGS,
        ],
        Some(&serde_json::to_string(&input)?),
        cwd,
        BATCH_TIMEOUT,
    )?;
    let (done, missing) = align(batch, &parse_object(&out.result)?);
    Ok((done, missing, out.cost_usd.unwrap_or(0.0)))
}

/// 一批的译文、仍缺失的条数与费用
type BatchResult = (Vec<(String, String)>, usize, f64);

pub struct Translated {
    pub html: String,
    pub cost_usd: f64,
    /// 重试后仍未翻译、保留原文的段数
    pub untranslated: usize,
}

/// 翻译整份报告；各批并行执行。某批漏译的条目单独重试一次，仍缺失的保留原文；
/// 调用失败（超时、非零退出、输出无法解析）则整体失败
pub fn translate_html(claude: &Path, cwd: &Path, html: &str) -> Result<Translated> {
    let segs = segments(html);
    let jobs = batches(&segs);
    let results: Vec<Result<BatchResult>> = std::thread::scope(|s| {
        let handles: Vec<_> = jobs
            .into_iter()
            .map(|batch| {
                s.spawn(move || {
                    let (mut done, missing, mut cost) = translate_batch(claude, cwd, &batch)?;
                    let mut left = missing.len();
                    // 重试失败不影响本批已翻译的部分
                    if !missing.is_empty()
                        && let Ok((more, still, c)) = translate_batch(claude, cwd, &missing)
                    {
                        done.extend(more);
                        left = still.len();
                        cost += c;
                    }
                    Ok((done, left, cost))
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap_or_else(|_| Err(anyhow!("翻译线程异常退出")))).collect()
    });

    let mut map = HashMap::new();
    let mut cost = 0.0;
    let mut untranslated = 0;
    for r in results {
        let (done, left, c) = r?;
        cost += c;
        untranslated += left;
        map.extend(done);
    }
    Ok(Translated { html: apply(html, &segs, &map), cost_usd: cost, untranslated })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HTML: &str = r#"<html lang="en"><head><title>Claude Code Insights</title><style>.a{color:red}</style></head>
<body><!-- note --><h1>Claude Code Insights</h1>
<p class="subtitle">1,842 messages across 228 sessions</p>
<div>  Shell &amp; Encoding  </div><div>123</div>
<code>## Rules
- Use `git status`</code>
<button>Copy</button><button>Copy</button>
<script>const x = "<b>not text</b>"; label: "Morning"</script></body></html>"#;

    #[test]
    fn extracts_text_nodes_only() {
        let texts: Vec<String> = segments(HTML).into_iter().map(|s| s.text).collect();
        assert_eq!(
            texts,
            vec![
                "Claude Code Insights",
                "Claude Code Insights",
                "1,842 messages across 228 sessions",
                "Shell & Encoding",
                "## Rules\n- Use `git status`",
                "Copy",
                "Copy",
            ]
        );
    }

    #[test]
    fn batches_dedup() {
        let b = batches(&segments(HTML));
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].len(), 5, "重复文本只翻译一次");
    }

    #[test]
    fn apply_keeps_structure() {
        let segs = segments(HTML);
        let map: HashMap<String, String> = [
            ("Claude Code Insights", "Claude Code 洞察"),
            ("Shell & Encoding", "Shell 与编码 <注意>"),
            ("Copy", "复制"),
        ]
        .into_iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        let out = apply(HTML, &segs, &map);
        assert!(out.starts_with(r#"<html lang="zh-CN">"#));
        assert!(out.contains("<title>Claude Code 洞察</title>"));
        assert!(out.contains("<div>  Shell 与编码 &lt;注意&gt;  </div>"), "保留首尾空白并转义");
        assert!(out.contains("<button>复制</button><button>复制</button>"));
        assert!(out.contains("1,842 messages across 228 sessions"), "未翻译的保留原文");
        assert!(out.contains(r#"<script>const x = "<b>not text</b>"; label: "Morning"</script>"#));
        let js = localize_scripts(
            r#"<script>b.textContent = 'Copied!'; x = "Copy"; y = 'Copy it'; l = "Night (0-6)"</script>"#,
        );
        assert_eq!(js, r#"<script>b.textContent = '已复制！'; x = "复制"; y = 'Copy it'; l = "凌晨 (0-6)"</script>"#);
        assert!(out.contains("<style>.a{color:red}</style>") && out.contains("<!-- note -->"));
    }

    #[test]
    fn entities() {
        assert_eq!(decode_entities("a &amp; b &lt;c&gt; &#39;d&#x27; &bogus; & e"), "a & b <c> 'd' &bogus; & e");
    }

    #[test]
    fn parse_and_align_by_key() {
        let got = parse_object("```json\n{\"0\": \"甲\", \"2\": \"丙\", \"9\": \"多余\", \"1\": 3}\n```").unwrap();
        let batch: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let (done, missing) = align(&batch, &got);
        assert_eq!(done, vec![("a".to_string(), "甲".to_string()), ("c".to_string(), "丙".to_string())]);
        assert_eq!(missing, vec!["b".to_string()], "非字符串值视为缺失，多余的键忽略");
        assert!(parse_object("抱歉").is_err());
    }

    /// 本机真实报告的抽取统计：`cargo test real_report_segments -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_report_segments() {
        let dirs = crate::claude_log::locate::candidate_dirs();
        let f = super::super::archive::list_report_files(&dirs).pop().expect("本机没有报告");
        let html = std::fs::read_to_string(&f).unwrap();
        let segs = segments(&html);
        let b = batches(&segs);
        println!("{} 段，去重后 {} 段，{} 批", segs.len(), b.iter().map(Vec::len).sum::<usize>(), b.len());
        let same: HashMap<String, String> = segs.iter().map(|s| (s.text.clone(), s.text.clone())).collect();
        let round = apply(&html, &segs, &same);
        assert!(!round.is_empty());
    }

    /// 实际调用 claude 翻译本机最新报告（消耗额度）：
    /// `DEVDECK_TRANSLATE_OUT=<输出文件> cargo test real_translate -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_translate() {
        let out = std::env::var("DEVDECK_TRANSLATE_OUT").expect("需设置 DEVDECK_TRANSLATE_OUT");
        let dirs = crate::claude_log::locate::candidate_dirs();
        let f = super::super::archive::list_report_files(&dirs).pop().expect("本机没有报告");
        let html = std::fs::read_to_string(&f).unwrap();
        let claude = cli::find_claude(None).unwrap();
        let start = std::time::Instant::now();
        let t = translate_html(&claude, &std::env::temp_dir(), &html).unwrap();
        println!("耗时 {:?}，费用 ${:.3}，保留原文 {} 段", start.elapsed(), t.cost_usd, t.untranslated);
        std::fs::write(out, &t.html).unwrap();
    }
}
