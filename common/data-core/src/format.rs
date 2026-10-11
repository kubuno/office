//! Number / date / value formatting, a port of `web/src/data/format.ts`. Power BI-like format tokens
//! drive how a measure renders across cards, charts, tables and tooltips. Pure functions, no state.
//!
//! Locale note (QUIRK): the web calls `Number.prototype.toLocaleString('fr-FR' | 'en-US' | 'en-GB')`,
//! whose grouping separator in a modern ICU/V8 is the narrow no-break space `U+202F` for `fr-FR` and a
//! comma for the English locales. This module reproduces that exactly. Rounding uses round-half-away-from-
//! zero (ICU `halfExpand`, which `toLocaleString`/`toFixed` also use), not Rust's default round-half-to-even.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The format tokens offered by the editor (`NUMBER_FORMATS` in `format.ts`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NumberFormat {
    Auto,
    Number,
    Integer,
    Decimal1,
    Decimal2,
    Thousands,
    Compact,
    Percent,
    Percent1,
    Percent2,
    CurrencyEur,
    CurrencyUsd,
    CurrencyGbp,
    Scientific,
    Duration,
    Bytes,
}

/// `{ value, label }` pairs in editor order, mirroring `NUMBER_FORMATS`.
pub const NUMBER_FORMATS: &[(NumberFormat, &str)] = &[
    (NumberFormat::Auto, "Automatique"),
    (NumberFormat::Number, "1 234,56"),
    (NumberFormat::Integer, "1 235"),
    (NumberFormat::Decimal1, "1 234,5"),
    (NumberFormat::Decimal2, "1 234,56"),
    (NumberFormat::Thousands, "1 234 (milliers)"),
    (NumberFormat::Compact, "1,2 k / 3,4 M"),
    (NumberFormat::Percent, "12 %"),
    (NumberFormat::Percent1, "12,3 %"),
    (NumberFormat::Percent2, "12,34 %"),
    (NumberFormat::CurrencyEur, "1 234 €"),
    (NumberFormat::CurrencyUsd, "$1,234"),
    (NumberFormat::CurrencyGbp, "£1,234"),
    (NumberFormat::Scientific, "1,23E3"),
    (NumberFormat::Duration, "1 h 23 min"),
    (NumberFormat::Bytes, "1,2 Ko / 3,4 Mo"),
]; // kept in sync with format.ts

impl NumberFormat {
    /// Parse the stored token string (`"currency_eur"`, …); unknown → `Auto`, matching the web `default`.
    pub fn from_token(token: &str) -> NumberFormat {
        match token {
            "number" => NumberFormat::Number,
            "integer" => NumberFormat::Integer,
            "decimal1" => NumberFormat::Decimal1,
            "decimal2" => NumberFormat::Decimal2,
            "thousands" => NumberFormat::Thousands,
            "compact" => NumberFormat::Compact,
            "percent" => NumberFormat::Percent,
            "percent1" => NumberFormat::Percent1,
            "percent2" => NumberFormat::Percent2,
            "currency_eur" => NumberFormat::CurrencyEur,
            "currency_usd" => NumberFormat::CurrencyUsd,
            "currency_gbp" => NumberFormat::CurrencyGbp,
            "scientific" => NumberFormat::Scientific,
            "duration" => NumberFormat::Duration,
            "bytes" => NumberFormat::Bytes,
            _ => NumberFormat::Auto,
        }
    }
}

/// Options for [`format_value`] (`opts` in `formatValue`).
#[derive(Debug, Clone, Default)]
pub struct FormatOpts {
    pub decimals: Option<u32>,
    pub prefix: Option<String>,
    pub suffix: Option<String>,
}

const NNBSP: char = '\u{202F}'; // narrow no-break space: fr-FR grouping separator

/// `toNum` — coerce a JSON value to a number the way the web does (strings stripped of spaces, comma→dot).
pub fn to_num(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => parse_loose(s).unwrap_or(0.0),
        _ => 0.0,
    }
}

/// `isNumericValue` — true for a number, or a string that parses once spaces are dropped and comma→dot.
pub fn is_numeric_value(v: &Value) -> bool {
    match v {
        Value::Number(_) => true,
        Value::String(s) => !s.trim().is_empty() && parse_loose(s).is_some(),
        _ => false,
    }
}

fn parse_loose(s: &str) -> Option<f64> {
    let cleaned: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let cleaned = cleaned.replacen(',', ".", 1);
    // JS `parseFloat` reads a leading numeric prefix and ignores a trailing tail.
    parse_js_float(&cleaned)
}

/// A subset of JavaScript `parseFloat`: the longest valid numeric prefix of the input.
fn parse_js_float(s: &str) -> Option<f64> {
    let bytes = s.as_bytes();
    let mut end = 0usize;
    let mut seen_digit = false;
    let mut seen_dot = false;
    let mut seen_exp = false;
    let mut i = 0usize;
    if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
        i += 1;
    }
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_digit() {
            seen_digit = true;
            i += 1;
            end = i;
        } else if c == b'.' && !seen_dot && !seen_exp {
            seen_dot = true;
            i += 1;
            if seen_digit {
                end = i;
            }
        } else if (c == b'e' || c == b'E') && seen_digit && !seen_exp {
            seen_exp = true;
            let mut j = i + 1;
            if j < bytes.len() && (bytes[j] == b'+' || bytes[j] == b'-') {
                j += 1;
            }
            if j < bytes.len() && bytes[j].is_ascii_digit() {
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                i = j;
                end = i;
            } else {
                break;
            }
        } else {
            break;
        }
    }
    if !seen_digit {
        return None;
    }
    s[..end].parse::<f64>().ok()
}

/// Round half away from zero to `frac` decimals, like `toFixed`/`halfExpand`.
fn round_to(n: f64, frac: u32) -> f64 {
    if !n.is_finite() {
        return n;
    }
    let factor = 10f64.powi(frac as i32);
    (n * factor).round() / factor
}

/// Format a finite number with a locale's grouping, `min..=max` fraction digits (trailing zeros trimmed
/// toward `min`). Reproduces `toLocaleString(locale, { minimumFractionDigits, maximumFractionDigits })`.
fn locale_number(n: f64, min_frac: u32, max_frac: u32, group: char, decimal: char) -> String {
    if !n.is_finite() {
        return if n.is_nan() {
            "NaN".to_string()
        } else if n > 0.0 {
            "∞".to_string()
        } else {
            "-∞".to_string()
        };
    }
    let rounded = round_to(n, max_frac);
    let negative = rounded.is_sign_negative() && rounded != 0.0;
    let abs = rounded.abs();
    // Fixed string at max_frac, then trim trailing zeros down to min_frac.
    let fixed = format!("{:.*}", max_frac as usize, abs);
    let (int_part, frac_part) = match fixed.split_once('.') {
        Some((i, f)) => (i.to_string(), f.to_string()),
        None => (fixed, String::new()),
    };
    let mut frac = frac_part;
    while frac.len() as u32 > min_frac && frac.ends_with('0') {
        frac.pop();
    }
    // Group the integer part in threes.
    let grouped = group_thousands(&int_part, group);
    let mut out = String::new();
    if negative {
        out.push('-');
    }
    out.push_str(&grouped);
    if !frac.is_empty() {
        out.push(decimal);
        out.push_str(&frac);
    }
    out
}

fn group_thousands(digits: &str, group: char) -> String {
    let bytes = digits.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len + len / 3);
    for (idx, b) in bytes.iter().enumerate() {
        if idx > 0 && (len - idx).is_multiple_of(3) {
            out.push(group);
        }
        out.push(*b as char);
    }
    out
}

fn fr(n: f64, min_frac: u32, max_frac: u32) -> String {
    locale_number(n, min_frac, max_frac, NNBSP, ',')
}

fn compact(n: f64, digits: u32) -> String {
    let abs = n.abs();
    let sign = if n < 0.0 { "-" } else { "" };
    let scaled = |div: f64, unit: &str| {
        format!(
            "{}{} {}",
            sign,
            format!("{:.*}", digits as usize, abs / div).replace('.', ","),
            unit
        )
    };
    if abs >= 1e12 {
        scaled(1e12, "T")
    } else if abs >= 1e9 {
        scaled(1e9, "Md")
    } else if abs >= 1e6 {
        scaled(1e6, "M")
    } else if abs >= 1e3 {
        scaled(1e3, "k")
    } else {
        fr(n, 0, digits)
    }
}

fn bytes(n: f64) -> String {
    let units = ["o", "Ko", "Mo", "Go", "To", "Po"];
    let mut v = n.abs();
    let mut i = 0usize;
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    let sign = if n < 0.0 { "-" } else { "" };
    let digits = if i == 0 { 0 } else { 1 };
    format!(
        "{}{} {}",
        sign,
        format!("{:.*}", digits, v).replace('.', ","),
        units[i]
    )
}

fn duration(seconds: f64) -> String {
    let s = seconds.abs().round() as i64;
    let h = s / 3600;
    let m = (s % 3600) / 60;
    let sec = s % 60;
    if h != 0 {
        format!("{h} h {m} min")
    } else if m != 0 {
        format!("{m} min {sec} s")
    } else {
        format!("{sec} s")
    }
}

/// `formatValue` — format a value against a Power BI-like token.
pub fn format_value(value: &Value, fmt: NumberFormat, opts: &FormatOpts) -> String {
    let n = to_num(value);
    let out = match fmt {
        NumberFormat::Integer => locale_number(n.round(), 0, 0, NNBSP, ','),
        NumberFormat::Decimal1 => fr(n, 1, 1),
        NumberFormat::Decimal2 => fr(n, 2, 2),
        NumberFormat::Thousands => fr(n / 1000.0, 0, 1),
        NumberFormat::Compact => compact(n, opts.decimals.unwrap_or(1)),
        NumberFormat::Percent => format!("{} %", fr(n * 100.0, 0, 0)),
        NumberFormat::Percent1 => format!("{} %", fr(n * 100.0, 1, 1)),
        NumberFormat::Percent2 => format!("{} %", fr(n * 100.0, 2, 2)),
        NumberFormat::CurrencyEur => format!("{} €", fr(n, 0, 0)),
        NumberFormat::CurrencyUsd => format!("${}", locale_number(n, 0, 0, ',', '.')),
        NumberFormat::CurrencyGbp => format!("£{}", locale_number(n, 0, 0, ',', '.')),
        NumberFormat::Scientific => scientific(n, opts.decimals.unwrap_or(2)),
        NumberFormat::Duration => duration(n),
        NumberFormat::Bytes => bytes(n),
        NumberFormat::Number => fr(n, 0, 2),
        NumberFormat::Auto => {
            if n.fract() == 0.0 && n.is_finite() {
                fr(n, 0, 0)
            } else {
                fr(n, 0, 2)
            }
        }
    };
    format!(
        "{}{}{}",
        opts.prefix.as_deref().unwrap_or(""),
        out,
        opts.suffix.as_deref().unwrap_or("")
    )
}

/// `n.toExponential(d)` then `.replace('.', ',').replace('e','E').replace('E+','E')`.
fn scientific(n: f64, digits: u32) -> String {
    let s = format!("{:.*e}", digits as usize, n); // Rust "1.23e3"
    s.replace('.', ",").replace('e', "E").replace("E+", "E")
}

/// `formatAxis` — short, always-compact axis-tick formatting.
pub fn format_axis(value: f64) -> String {
    if value.abs() >= 1000.0 {
        compact(value, 1)
    } else if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        fr(value, 0, 1)
    }
}

/// `formatLabel` — a category label, truncated with an ellipsis past `max` characters.
pub fn format_label(v: &Value, max: usize) -> String {
    let s = match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let chars: Vec<char> = s.chars().collect();
    if chars.len() > max {
        let mut out: String = chars[..max.saturating_sub(1)].iter().collect();
        out.push('…');
        out
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn to_num_coercions() {
        assert_eq!(to_num(&json!(12.5)), 12.5);
        assert_eq!(to_num(&json!("1 234,56")), 1234.56);
        assert_eq!(to_num(&json!("x")), 0.0);
        assert_eq!(to_num(&json!(null)), 0.0);
        assert_eq!(to_num(&json!("12px")), 12.0); // parseFloat tail ignored
    }

    #[test]
    fn numeric_detection() {
        assert!(is_numeric_value(&json!(3)));
        assert!(is_numeric_value(&json!("3,5")));
        assert!(!is_numeric_value(&json!("")));
        assert!(!is_numeric_value(&json!("abc")));
    }

    #[test]
    fn fr_grouping_uses_narrow_nbsp() {
        assert_eq!(fr(1234.56, 0, 2), format!("1{NNBSP}234,56"));
        assert_eq!(fr(1000000.0, 0, 0), format!("1{NNBSP}000{NNBSP}000"));
        assert_eq!(fr(-12.0, 0, 0), "-12");
    }

    #[test]
    fn format_tokens() {
        assert_eq!(format_value(&json!(1235), NumberFormat::Integer, &FormatOpts::default()), format!("1{NNBSP}235"));
        assert_eq!(format_value(&json!(0.1234), NumberFormat::Percent1, &FormatOpts::default()), "12,3 %");
        assert_eq!(format_value(&json!(1234), NumberFormat::CurrencyUsd, &FormatOpts::default()), "$1,234");
        assert_eq!(format_value(&json!(1500000), NumberFormat::Compact, &FormatOpts::default()), "1,5 M");
        assert_eq!(format_value(&json!(1234), NumberFormat::CurrencyEur, &FormatOpts::default()), format!("1{NNBSP}234 €"));
    }

    #[test]
    fn bytes_and_duration() {
        assert_eq!(bytes(1536.0), "1,5 Ko");
        assert_eq!(bytes(512.0), "512 o");
        assert_eq!(duration(5000.0), "1 h 23 min");
        assert_eq!(duration(90.0), "1 min 30 s");
    }

    #[test]
    fn axis_and_label() {
        assert_eq!(format_axis(2500.0), "2,5 k");
        assert_eq!(format_axis(42.0), "42");
        assert_eq!(format_label(&json!("A very long category name"), 18), "A very long categ…");
        assert_eq!(format_label(&json!("short"), 18), "short");
    }

    #[test]
    fn prefix_suffix() {
        let opts = FormatOpts { decimals: None, prefix: Some("~".into()), suffix: Some(" u".into()) };
        assert_eq!(format_value(&json!(3), NumberFormat::Auto, &opts), "~3 u");
    }
}
