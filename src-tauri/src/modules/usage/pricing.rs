//! 价格表与费用估算。价格来源见 resources/pricing.json

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

const BUILTIN: &str = include_str!("../../../resources/pricing.json");

/// 单价，USD / 百万 token
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rates {
    pub input: f64,
    #[serde(rename = "cacheWrite5m")]
    pub cache_write_5m: f64,
    #[serde(rename = "cacheWrite1h")]
    pub cache_write_1h: f64,
    pub cache_read: f64,
    pub output: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LongContext {
    /// prompt 长度超过该值时使用 `rates`
    pub threshold: u64,
    #[serde(flatten)]
    pub rates: Rates,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPrice {
    #[serde(flatten)]
    pub base: Rates,
    pub long_context: Option<LongContext>,
    pub fast_multiplier: Option<f64>,
    pub us_geo_multiplier: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PricingTable {
    pub source: String,
    pub checked_at: String,
    pub models: HashMap<String, ModelPrice>,
}

/// 一组同口径（同模型、速度、地域、是否长上下文）token 的合计
#[derive(Debug, Default, Clone, PartialEq)]
pub struct TokenGroup {
    pub model: String,
    pub speed: Option<String>,
    pub geo: Option<String>,
    pub long_context: bool,
    pub input: u64,
    pub output: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
    pub cache_read: u64,
}

impl PricingTable {
    pub fn builtin() -> Self {
        serde_json::from_str(BUILTIN).expect("内置 pricing.json 格式错误")
    }

    pub fn find(&self, model: &str) -> Option<&ModelPrice> {
        self.models.get(&normalize_model(model))
    }

    /// 长上下文分档阈值（目前所有分档模型共用一个阈值）；没有分档模型时为 None
    pub fn long_context_threshold(&self) -> Option<u64> {
        self.models.values().filter_map(|m| m.long_context.as_ref().map(|l| l.threshold)).min()
    }

    /// 估算费用；模型不在价格表中返回 None
    pub fn cost(&self, g: &TokenGroup) -> Option<f64> {
        let p = self.find(&g.model)?;
        let rates = match (&p.long_context, g.long_context) {
            (Some(lc), true) => lc.rates,
            _ => p.base,
        };
        let mut mult = 1.0;
        if g.speed.as_deref() == Some("fast") {
            mult *= p.fast_multiplier.unwrap_or(1.0);
        }
        if g.geo.as_deref().is_some_and(|s| s.eq_ignore_ascii_case("us")) {
            mult *= p.us_geo_multiplier.unwrap_or(1.0);
        }
        let usd = (g.input as f64 * rates.input
            + g.output as f64 * rates.output
            + g.cache_write_5m as f64 * rates.cache_write_5m
            + g.cache_write_1h as f64 * rates.cache_write_1h
            + g.cache_read as f64 * rates.cache_read)
            / 1_000_000.0;
        Some(usd * mult)
    }
}

/// 模型名归一化：小写，去掉 `anthropic.` 前缀、`[1m]` 之类的后缀和 8 位日期后缀
pub fn normalize_model(model: &str) -> String {
    let mut m = model.trim().to_ascii_lowercase();
    if let Some(rest) = m.strip_prefix("anthropic.") {
        m = rest.to_string();
    }
    if let Some(i) = m.find('[') {
        m.truncate(i);
    }
    if let Some((head, tail)) = m.rsplit_once('-') {
        if tail.len() == 8 && tail.bytes().all(|b| b.is_ascii_digit()) {
            m = head.to_string();
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(model: &str) -> TokenGroup {
        TokenGroup {
            model: model.into(),
            input: 1_000_000,
            output: 1_000_000,
            cache_write_5m: 1_000_000,
            cache_write_1h: 1_000_000,
            cache_read: 1_000_000,
            ..Default::default()
        }
    }

    #[test]
    fn builtin_table_parses() {
        let t = PricingTable::builtin();
        assert!(t.models.len() >= 20);
        assert_eq!(t.long_context_threshold(), Some(100_000));
    }

    #[test]
    fn normalizes_model_names() {
        assert_eq!(normalize_model("claude-haiku-4-5-20251001"), "claude-haiku-4-5");
        assert_eq!(normalize_model("anthropic.claude-opus-5-5"), "claude-opus-5-5");
        assert_eq!(normalize_model("claude-opus-5-5[1m]"), "claude-opus-5-5");
        assert_eq!(normalize_model("claude-opus-5"), "claude-opus-5");
    }

    #[test]
    fn computes_cost_per_category() {
        let t = PricingTable::builtin();
        // opus 5.5：4 + 20 + 5 + 8 + 0.2
        let c = t.cost(&group("claude-opus-5-5")).unwrap();
        assert!((c - 37.2).abs() < 1e-9, "{c}");
        // 带日期后缀的 haiku 4.5：1 + 5 + 1.25 + 2 + 0.1
        let c = t.cost(&group("claude-haiku-4-5-20251001")).unwrap();
        assert!((c - 9.35).abs() < 1e-9, "{c}");
    }

    #[test]
    fn fast_geo_and_long_context() {
        let t = PricingTable::builtin();
        let mut g = group("claude-opus-5-5");
        g.speed = Some("fast".into());
        assert!((t.cost(&g).unwrap() - 74.4).abs() < 1e-9);
        g.geo = Some("us".into());
        assert!((t.cost(&g).unwrap() - 81.84).abs() < 1e-9);

        let mut h = group("claude-haiku-5-5");
        // 0.10 + 0.125 + 0.20 + 0.01 + 0.50
        assert!((t.cost(&h).unwrap() - 0.935).abs() < 1e-9);
        h.long_context = true;
        // 0.50 + 0.625 + 1 + 0.05 + 2.50
        assert!((t.cost(&h).unwrap() - 4.675).abs() < 1e-9);
        // 没有长上下文分档的模型不受影响
        let mut o = group("claude-opus-5-5");
        o.long_context = true;
        assert!((t.cost(&o).unwrap() - 37.2).abs() < 1e-9);
    }

    #[test]
    fn unknown_model_has_no_price() {
        assert!(PricingTable::builtin().cost(&group("gpt-x")).is_none());
    }
}
