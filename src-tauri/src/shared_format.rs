//! 托盘等 Rust 侧展示用的数字格式，与前端 shared/format.ts 保持一致

pub fn fmt_tokens(n: u64) -> String {
    let f = n as f64;
    if f >= 1e9 {
        format!("{:.2}B", f / 1e9)
    } else if f >= 1e6 {
        format!("{:.2}M", f / 1e6)
    } else {
        thousands(n)
    }
}

pub fn fmt_usd(v: f64) -> String {
    if v > 0.0 && v < 0.01 {
        return "<$0.01".into();
    }
    let cents = (v * 100.0).round() as u64;
    format!("${}.{:02}", thousands(cents / 100), cents % 100)
}

fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(fmt_tokens(999), "999");
        assert_eq!(fmt_tokens(123_456), "123,456");
        assert_eq!(fmt_tokens(1_234_567), "1.23M");
        assert_eq!(fmt_tokens(5_334_258_918), "5.33B");
        assert_eq!(fmt_usd(0.0), "$0.00");
        assert_eq!(fmt_usd(0.004), "<$0.01");
        assert_eq!(fmt_usd(3217.316), "$3,217.32");
    }
}
