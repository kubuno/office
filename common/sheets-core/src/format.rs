//! Number and date display, as the web's grid writes them.
//!
//! * [`format_code`]: a raw number-format code (`numFmtCode`, imported from `.xlsx`): `formatCode`,
//!   `formatDateSerial`, `formatNumberCode` of `formula-engine.ts`, quirks included (only the first
//!   section, French day and month names, `m` read as minutes only right after `:` or an hour).
//! * [`format_text`]: the `TEXT()` function's own, simpler reading of a format.
//! * [`format_number`]: the ribbon's formats (`numFmt` currency / percent / number / scientific,
//!   `decimals`, `thousands`), which the web renders with `Intl.NumberFormat` in the app's language -
//!   reproduced here for French and English (other languages read as English).

use crate::formula::value::{format_value, to_str, Scalar, Value};
use crate::js;
use crate::jsdate;

/// French day names (the web's `DAY_ABBR` / `DAY_FULL`), Sunday first.
pub const DAY_ABBR: [&str; 7] = ["dim", "lun", "mar", "mer", "jeu", "ven", "sam"];
pub const DAY_FULL: [&str; 7] = ["dimanche", "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi"];
/// French month names (`MON_ABBR` / `MON_FULL`).
pub const MON_ABBR: [&str; 12] = ["janv", "févr", "mars", "avr", "mai", "juin", "juil", "août", "sept", "oct", "nov", "déc"];
pub const MON_FULL: [&str; 12] = ["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre", "décembre"];

/// Removes `"…"` literals and `[…]` brackets (the web's `replace(/"[^"]*"/g, '').replace(/\[[^\]]*\]/g, '')`).
fn strip_literals(fmt: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = fmt.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '"' {
            if let Some(j) = chars[i + 1..].iter().position(|&c| c == '"') {
                i += j + 2;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    let chars: Vec<char> = out.chars().collect();
    let mut out2 = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '[' {
            if let Some(j) = chars[i + 1..].iter().position(|&c| c == ']') {
                i += j + 2;
                continue;
            }
        }
        out2.push(chars[i]);
        i += 1;
    }
    out2
}

/// `isDateFormat`: date letters (`j d a y h s`, and `j d m a`) with no `#` or `0` outside literals.
pub fn is_date_format(fmt: &str) -> bool {
    let s = strip_literals(fmt);
    if s.contains(['#', '0']) {
        return false;
    }
    let lower = s.to_lowercase();
    lower.contains(['j', 'd', 'a', 'y', 'h', 's']) && lower.contains(['j', 'd', 'm', 'a'])
}

fn pad2(n: i64) -> String {
    format!("{n:02}")
}

/// `formatDateSerial(serial, fmt)`.
pub fn format_date_serial(serial: f64, fmt: &str) -> String {
    let Some(t) = jsdate::serial_to_date(serial) else { return js::to_string(serial) };
    let Some(p) = jsdate::parts(t) else { return js::to_string(serial) };
    let (day, mon, yr, dow, hr, se, mi) = (p.date, p.month as usize, p.year, p.weekday as usize, p.hours, p.seconds, p.minutes);
    let f: Vec<char> = fmt.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let lower = |c: char| c.to_lowercase().next().unwrap_or(c);
    while i < f.len() {
        let c = f[i];
        if c == '"' {
            let mut j = i + 1;
            while j < f.len() && f[j] != '"' {
                j += 1;
            }
            out.extend(&f[(i + 1).min(f.len())..j.min(f.len())]);
            i = j + 1;
            continue;
        }
        let lc = lower(c);
        if lc == 'j' || lc == 'd' {
            let mut n = 0;
            while i < f.len() && lower(f[i]) == lc {
                n += 1;
                i += 1;
            }
            out.push_str(&match n {
                1 => day.to_string(),
                2 => pad2(day),
                3 => DAY_ABBR[dow].to_string(),
                _ => DAY_FULL[dow].to_string(),
            });
            continue;
        }
        if lc == 'm' {
            let mut n = 0;
            while i < f.len() && lower(f[i]) == 'm' {
                n += 1;
                i += 1;
            }
            // Minutes when right after ':' or an hour (the last character of the output, a trailing
            // whitespace ignored).
            let trimmed = out.strip_suffix(|c: char| js::is_whitespace(c)).unwrap_or(&out);
            let prev = trimmed.chars().last();
            if prev == Some(':') || matches!(prev, Some('h') | Some('H')) {
                out.push_str(&if n >= 2 { pad2(mi) } else { mi.to_string() });
            } else {
                out.push_str(&match n {
                    1 => (mon + 1).to_string(),
                    2 => pad2(mon as i64 + 1),
                    3 => MON_ABBR[mon].to_string(),
                    _ => MON_FULL[mon].to_string(),
                });
            }
            continue;
        }
        if lc == 'a' || lc == 'y' {
            let mut n = 0;
            while i < f.len() && matches!(f[i], 'a' | 'y' | 'A' | 'Y') {
                n += 1;
                i += 1;
            }
            let ys = yr.to_string();
            out.push_str(&if n <= 2 { js::slice(&ys, -2.0, None) } else { ys });
            continue;
        }
        if lc == 'h' {
            let mut n = 0;
            while i < f.len() && lower(f[i]) == 'h' {
                n += 1;
                i += 1;
            }
            out.push_str(&if n >= 2 { pad2(hr) } else { hr.to_string() });
            continue;
        }
        if lc == 's' {
            let mut n = 0;
            while i < f.len() && lower(f[i]) == 's' {
                n += 1;
                i += 1;
            }
            out.push_str(&if n >= 2 { pad2(se) } else { se.to_string() });
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Groups the digits of an integer part by three with `sep` (`/\B(?=(\d{3})+(?!\d))/g`).
pub fn group_digits(int: &str, sep: &str) -> String {
    let digits: Vec<char> = int.chars().collect();
    let mut out = String::new();
    // The lookahead groups the trailing run of digits; a non-digit prefix (a sign) is kept as is.
    let start = digits.iter().position(|c| c.is_ascii_digit()).unwrap_or(digits.len());
    let run_end = digits[start..].iter().position(|c| !c.is_ascii_digit()).map(|p| start + p).unwrap_or(digits.len());
    out.extend(&digits[..start]);
    let run = &digits[start..run_end];
    for (k, c) in run.iter().enumerate() {
        if k > 0 && (run.len() - k).is_multiple_of(3) {
            out.push_str(sep);
        }
        out.push(*c);
    }
    out.extend(&digits[run_end..]);
    out
}

/// `formatNumberCode(value, sec)`: one section of a numeric format.
pub fn format_number_code(value: f64, sec: &str) -> String {
    if sec == "@" || sec.is_empty() {
        return js::to_string(value);
    }
    let is_percent = sec.contains('%');
    let mut v = if is_percent { value * 100.0 } else { value };
    let neg = v < 0.0;
    v = v.abs();
    let dot = sec.find('.');
    let int_tok: String = match dot {
        Some(d) => &sec[..d],
        None => sec,
    }
    .chars()
    .filter(|c| matches!(c, '0' | '#' | ','))
    .collect();
    let frac_tok: String = match dot {
        Some(d) => sec[d + 1..].chars().filter(|c| matches!(c, '0' | '#')).collect(),
        None => String::new(),
    };
    let dec = frac_tok.chars().count() as u32;
    let min_int = int_tok.chars().filter(|&c| c == '0').count();
    let grouping = int_tok.contains(',');
    let fixed = js::to_fixed(v, dec.min(100));
    let (mut ip, fp) = match fixed.split_once('.') {
        Some((a, b)) => (a.to_string(), Some(b.to_string())),
        None => (fixed.clone(), None),
    };
    if ip.chars().count() < min_int {
        ip = js::pad_start(&ip, min_int, '0');
    }
    if grouping {
        ip = group_digits(&ip, "\u{202F}");
    }
    let mut out = match fp {
        Some(f) if !f.is_empty() => format!("{ip}.{f}"),
        _ => ip,
    };
    if is_percent {
        out.push_str(" %");
    }
    if neg {
        format!("-{out}")
    } else {
        out
    }
}

/// `formatCode(value, code)`: `General`, a date format, or the first section of a numeric one.
pub fn format_code(value: f64, code: &str) -> String {
    if js::trim(code).eq_ignore_ascii_case("general") {
        return format_value(&Value::num(value));
    }
    if is_date_format(code) {
        return format_date_serial(value, code);
    }
    format_number_code(value, code.split(';').next().unwrap_or(""))
}

/// `formatText(v, fmt)`: `TEXT()`.
pub fn format_text(v: &Scalar, fmt: &str) -> String {
    if let Scalar::Num(n) = v {
        if is_date_format(fmt) {
            return format_date_serial(*n, fmt);
        }
        if fmt.contains(['0', '#']) {
            let after = fmt.split('.').nth(1).unwrap_or("");
            let dec = after.chars().filter(|c| matches!(c, '0' | '#')).count() as u32;
            let mut s = js::to_fixed(*n, dec.min(100));
            if fmt.contains(',') {
                let (int, frac) = match s.split_once('.') {
                    Some((a, b)) => (a.to_string(), Some(b.to_string())),
                    None => (s.clone(), None),
                };
                s = group_digits(&int, ",") + &frac.map(|f| format!(".{f}")).unwrap_or_default();
            }
            return s;
        }
    }
    to_str(v)
}

// ── Intl.NumberFormat, for the ribbon's formats ─────────────────────────────

/// The separators and patterns of a language.
struct Locale {
    decimal: &'static str,
    group: &'static str,
    /// `12 %` vs `12%`.
    percent_suffix: &'static str,
    /// Currency before (`€1.00`) or after (`1,00 €`).
    currency_after: bool,
    currency_space: &'static str,
}

fn locale(lang: &str) -> Locale {
    let l = lang.to_ascii_lowercase();
    if l.starts_with("fr") {
        Locale { decimal: ",", group: "\u{202F}", percent_suffix: "\u{202F}%", currency_after: true, currency_space: "\u{A0}" }
    } else {
        Locale { decimal: ".", group: ",", percent_suffix: "%", currency_after: false, currency_space: "" }
    }
}

/// The decimal digits of `x` ≥ 0 rounded to at most `max` fraction digits from its shortest
/// representation, halves away from zero (ICU's `halfExpand`), with at least `min` fraction digits.
fn intl_digits(x: f64, min: u32, max: u32) -> (String, String) {
    let s = js::to_string(x);
    // Expand an exponent form into plain digits.
    let (mant, exp) = match s.split_once('e') {
        Some((m, e)) => (m.to_string(), e.parse::<i32>().unwrap_or(0)),
        None => (s.clone(), 0),
    };
    let (ip, fp) = match mant.split_once('.') {
        Some((a, b)) => (a.to_string(), b.to_string()),
        None => (mant.clone(), String::new()),
    };
    let mut digits: Vec<u8> = format!("{ip}{fp}").bytes().collect();
    let mut point = ip.len() as i32 + exp; // digits before the decimal point
    if point < 0 {
        let mut z = vec![b'0'; (-point) as usize];
        z.extend(digits);
        digits = z;
        point = 0;
    }
    while (digits.len() as i32) < point {
        digits.push(b'0');
    }
    let point = point as usize;
    let mut int: Vec<u8> = digits[..point].to_vec();
    let mut frac: Vec<u8> = digits[point..].to_vec();
    if frac.len() > max as usize {
        let up = frac[max as usize] >= b'5';
        frac.truncate(max as usize);
        if up {
            // Carry through the fraction then the integer part.
            let mut i = frac.len();
            let mut carry = true;
            while carry && i > 0 {
                i -= 1;
                if frac[i] == b'9' {
                    frac[i] = b'0';
                } else {
                    frac[i] += 1;
                    carry = false;
                }
            }
            if carry {
                let mut j = int.len();
                while carry && j > 0 {
                    j -= 1;
                    if int[j] == b'9' {
                        int[j] = b'0';
                    } else {
                        int[j] += 1;
                        carry = false;
                    }
                }
                if carry {
                    int.insert(0, b'1');
                }
            }
        }
    }
    while frac.len() > min as usize && frac.last() == Some(&b'0') {
        frac.pop();
    }
    while frac.len() < min as usize {
        frac.push(b'0');
    }
    let mut int = String::from_utf8(int).unwrap_or_default();
    let int_trim = int.trim_start_matches('0');
    int = if int_trim.is_empty() { "0".into() } else { int_trim.to_string() };
    (int, String::from_utf8(frac).unwrap_or_default())
}

/// The ribbon's number formats of a style (`formatNumber(n, s, lang)`).
pub fn format_number(n: f64, num_fmt: Option<&str>, decimals: Option<f64>, thousands: Option<bool>, lang: &str) -> String {
    if num_fmt == Some("scientific") {
        let d = decimals.unwrap_or(2.0);
        if !(0.0..=100.0).contains(&d) || d.is_nan() {
            return js::to_string(n);
        }
        return js::to_exponential(n, d.trunc() as u32);
    }
    let currency = num_fmt == Some("currency");
    let percent = num_fmt == Some("percent");
    let grouping = thousands.unwrap_or(currency || num_fmt == Some("number"));
    let (min, max) = match decimals {
        Some(d) => {
            if !(0.0..=100.0).contains(&d) || d.is_nan() {
                return js::to_string(n);
            }
            (d.trunc() as u32, d.trunc() as u32)
        }
        None => (if currency { 2 } else { 0 }, if currency || percent { 2 } else { 10 }),
    };
    let loc = locale(lang);
    if n.is_nan() {
        return "NaN".into();
    }
    let v = if percent { n * 100.0 } else { n };
    let neg = v < 0.0 || (v == 0.0 && v.is_sign_negative());
    let body = if v.is_infinite() {
        "∞".to_string()
    } else {
        let (int, frac) = intl_digits(v.abs(), min, max);
        let int = if grouping { group_intl(&int, loc.group, lang) } else { int };
        if frac.is_empty() {
            int
        } else {
            format!("{int}{}{frac}", loc.decimal)
        }
    };
    let sign = if neg { "-" } else { "" };
    if currency {
        if loc.currency_after {
            format!("{sign}{body}{}€", loc.currency_space)
        } else {
            format!("{sign}€{body}")
        }
    } else if percent {
        format!("{sign}{body}{}", loc.percent_suffix)
    } else {
        format!("{sign}{body}")
    }
}

/// Grouping as ICU does it (French and English group from four digits on).
fn group_intl(int: &str, sep: &str, _lang: &str) -> String {
    group_digits(int, sep)
}

/// `n.toLocaleString(lang, { minimumFractionDigits: min, maximumFractionDigits: max })` (grouping on).
pub fn locale_fixed(n: f64, min: u32, max: u32, lang: &str) -> String {
    let loc = locale(lang);
    if n.is_nan() {
        return "NaN".into();
    }
    if n.is_infinite() {
        return if n > 0.0 { "∞".into() } else { "-∞".into() };
    }
    let (int, frac) = intl_digits(n.abs(), min, max.max(min));
    let int = group_intl(&int, loc.group, lang);
    let neg = n < 0.0 && !(int.chars().all(|c| c == '0') && frac.chars().all(|c| c == '0'));
    let sign = if neg { "-" } else { "" };
    if frac.is_empty() {
        format!("{sign}{int}")
    } else {
        format!("{sign}{int}{}{frac}", loc.decimal)
    }
}

/// `new Intl.NumberFormat(lang, { notation: 'compact', maximumFractionDigits: 1 }).format(n)` for English
/// and French: `1.2K`, `3M`, `1.5B` / `1,2 k`, `3 M`, `1,5 Md` (no-break spaces).
pub fn compact_number(n: f64, lang: &str) -> String {
    let fr = lang.to_ascii_lowercase().starts_with("fr");
    let a = n.abs();
    let units: [(f64, &str, &str); 4] = [(1e12, "T", "\u{a0}Bn"), (1e9, "B", "\u{a0}Md"), (1e6, "M", "\u{a0}M"), (1e3, "K", "\u{a0}k")];
    for (i, (scale, en, frs)) in units.iter().enumerate() {
        if a >= *scale {
            let v = n / scale;
            // Rounding up to the next unit (999 950 → 1M): ICU re-scales.
            if i > 0 && js::round(v.abs() * 10.0) / 10.0 >= 1000.0 {
                let (s2, en2, fr2) = units[i - 1];
                let body = locale_fixed(n / s2, 0, 1, lang);
                return format!("{body}{}", if fr { fr2 } else { en2 });
            }
            let body = locale_fixed(v, 0, 1, lang);
            return format!("{body}{}", if fr { *frs } else { *en });
        }
    }
    locale_fixed(n, 0, 1, lang)
}

/// `toLocaleString(lang, { maximumFractionDigits: 2 })` (the status bar's aggregates).
pub fn locale_number(n: f64, max_fraction: u32, lang: &str) -> String {
    let loc = locale(lang);
    if n.is_nan() {
        return "NaN".into();
    }
    if n.is_infinite() {
        return if n > 0.0 { "∞".into() } else { "-∞".into() };
    }
    let (int, frac) = intl_digits(n.abs(), 0, max_fraction);
    let int = group_intl(&int, loc.group, lang);
    let sign = if n < 0.0 || (n == 0.0 && n.is_sign_negative()) { "-" } else { "" };
    if frac.is_empty() {
        format!("{sign}{int}")
    } else {
        format!("{sign}{int}{}{frac}", loc.decimal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_codes() {
        // 45292 = 2024-01-01, a Monday.
        assert_eq!(format_code(45292.0, "dd/mm/yyyy"), "01/01/2024");
        assert_eq!(format_code(45292.0, "jjjj d mmmm aaaa"), "lundi 1 janvier 2024");
        assert_eq!(format_code(45292.75, "hh:mm"), "18:00");
        // QUIRK: `mm` after a literal that is not an hour or ':' is the month.
        assert_eq!(format_code(45292.75, "mm:ss"), "01:00");
        assert_eq!(format_code(45292.0, "ddd"), "lun");
        assert!(is_date_format("dd/mm/yyyy"));
        assert!(!is_date_format("0.00"));
        assert!(!is_date_format("\"jour\" 0"));
    }

    #[test]
    fn number_codes() {
        assert_eq!(format_code(1234.567, "#,##0.00"), "1\u{202F}234.57");
        assert_eq!(format_code(0.256, "0.0%"), "25.6 %");
        assert_eq!(format_code(-5.0, "000"), "-005");
        assert_eq!(format_code(3.0, "General"), "3");
        assert_eq!(format_code(3.0, "0;(0)"), "3");
        assert_eq!(format_code(1.0 / 3.0, "@"), "0.3333333333333333");
    }

    #[test]
    fn text_function_formats() {
        assert_eq!(format_text(&Scalar::Num(1234.5), "#,##0.00"), "1,234.50");
        assert_eq!(format_text(&Scalar::Num(5.0), "000"), "5");
        assert_eq!(format_text(&Scalar::str("x"), "0"), "x");
        // QUIRK: "yyyy" has no j/d/m/a, so it is not a date format on the web.
        assert_eq!(format_text(&Scalar::Num(45292.0), "yyyy"), "45292");
        assert_eq!(format_text(&Scalar::Num(45292.0), "yyyy-mm-dd"), "2024-01-01");
    }

    #[test]
    fn intl_formats() {
        assert_eq!(format_number(1234.5, Some("currency"), None, None, "fr"), "1\u{202F}234,50\u{A0}€");
        assert_eq!(format_number(1234.5, Some("currency"), None, None, "en"), "€1,234.50");
        assert_eq!(format_number(0.125, Some("percent"), None, None, "fr"), "12,5\u{202F}%");
        assert_eq!(format_number(1.005, Some("number"), Some(2.0), None, "en"), "1.01");
        assert_eq!(format_number(1234.0, None, Some(0.0), Some(false), "fr"), "1234");
        assert_eq!(format_number(12345.678, Some("scientific"), None, None, "fr"), "1.23e+4");
        assert_eq!(format_number(-0.5, Some("number"), None, None, "en"), "-0.5");
        assert_eq!(locale_number(1234.567, 2, "fr"), "1\u{202F}234,57");
    }
}
