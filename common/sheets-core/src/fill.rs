//! Autofill series generator for the fill handle: a faithful port of the web editor's `fill-series.ts`.
//!
//! Given the source cells of a fill-handle drag, [`fill_series`] extends the detected pattern by `count`
//! steps in either direction, or returns `None` (the caller then tiles/repeats the source). Detectors,
//! tried most-specific first: numbers (linear / geometric / regression, leading-zero padding), booleans,
//! thousands-grouped numbers, 24 h and 12 h (AM/PM) times, dates (ISO / DMY, daily / weekly / weekday-only
//! / end-of-month / monthly / yearly), `<month> <year>`, quarters, month and weekday names, roman numerals,
//! alphabetic base-26 letters, English and French ordinals, numbers inside fixed affixes, and generic
//! "text + number" tokens.
//!
//! # Locale names
//!
//! The web asks `Intl.DateTimeFormat(lang, {weekday|month: long|short})` for the names. Here the names are
//! built in: **French** (`fr`, any region; `fr-CA` writes `juill.` for the short July) and **English** (`en`;
//! `en-GB` writes `Sept`), exactly as V8/ICU produces them (lower-cased, one trailing `.` removed). **Every
//! other language falls back to English** (an invalid language tag, which makes `Intl` throw on the web,
//! does too).
//!
//! # Quirks kept
//!
//! * The thousands separators the web recognises are `,` and the plain space (its pattern lists the plain
//!   space twice although its comment promises the no-break space).
//! * `Item-12` is read as prefix `Item` and the number `-12` (the affix pattern takes the sign), so it
//!   continues `Item-11`, `Item-10`…

use std::sync::OnceLock;

use regex::Regex;

use crate::{js, jsdate};

/// A lazily compiled static regex (`None` if the pattern were invalid: callers degrade, never panic).
macro_rules! re {
    ($pat:expr) => {{
        static R: OnceLock<Option<Regex>> = OnceLock::new();
        R.get_or_init(|| Regex::new($pat).ok()).as_ref()
    }};
}

/// JavaScript's `\s` as a character class (differs from Rust's: no U+0085, with U+FEFF).
macro_rules! ws {
    () => {
        r"[\t\n\x0B\x0C\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}]"
    };
}

/// A source or generated cell value: the web's `string | number`.
#[derive(Clone, Debug, PartialEq)]
pub enum Fill {
    Num(f64),
    Str(String),
}

impl Fill {
    /// `String(v)`.
    fn to_js_string(&self) -> String {
        match self {
            Fill::Num(n) => js::to_string(*n),
            Fill::Str(s) => s.clone(),
        }
    }
}

impl From<&str> for Fill {
    fn from(s: &str) -> Self {
        Fill::Str(s.to_string())
    }
}

impl From<f64> for Fill {
    fn from(n: f64) -> Self {
        Fill::Num(n)
    }
}

// ── tiny numeric helpers ────────────────────────────────────────────────────────

/// `pad2`: `String(Math.abs(n)).padStart(2, '0')`.
pub fn pad2(n: f64) -> String {
    js::pad_start(&js::to_string(n.abs()), 2, '0')
}

fn all_equal<T: PartialEq>(a: &[T]) -> bool {
    a.iter().all(|x| *x == a[0])
}

fn consecutive_diffs(a: &[f64]) -> Vec<f64> {
    a.windows(2).map(|w| w[1] - w[0]).collect()
}

fn const_step(a: &[f64]) -> bool {
    a.len() >= 2 && all_equal(&consecutive_diffs(a))
}

fn last_step(a: &[f64]) -> f64 {
    match a.len() {
        0 => 1.0,
        1 => 0.0,
        n => a[n - 1] - a[n - 2],
    }
}

/// `x || 1` for a number.
fn or1(x: f64) -> f64 {
    if x == 0.0 || x.is_nan() {
        1.0
    } else {
        x
    }
}

fn round10(n: f64) -> f64 {
    js::round(n * 1e10) / 1e10
}

/// `new Date(Date.UTC(y, m, 0)).getUTCDate()` (m = 1..12): `NaN` for an invalid date.
fn days_in_month(y: f64, m: f64) -> f64 {
    jsdate::parts(jsdate::new_date(y, m, 0.0, 0.0, 0.0, 0.0, 0.0)).map(|p| p.date as f64).unwrap_or(f64::NAN)
}

fn first_or_nan(a: &[f64], i: usize) -> f64 {
    a.get(i).copied().unwrap_or(f64::NAN)
}

// ── locale month / weekday names ────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Weekday,
    Month,
}

/// `localeNames`: lower-cased names with one trailing `.` removed (weekdays start on Monday).
fn locale_names(lang: &str, kind: Kind, long: bool) -> Vec<String> {
    let tag = lang.to_ascii_lowercase();
    let primary = tag.split('-').next().unwrap_or("");
    let raw: &[&str] = if primary == "fr" {
        match (kind, long) {
            (Kind::Weekday, true) => &["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"],
            (Kind::Weekday, false) => &["lun.", "mar.", "mer.", "jeu.", "ven.", "sam.", "dim."],
            (Kind::Month, true) => &["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre", "décembre"],
            (Kind::Month, false) => {
                if tag == "fr-ca" {
                    &["janv.", "févr.", "mars", "avr.", "mai", "juin", "juill.", "août", "sept.", "oct.", "nov.", "déc."]
                } else {
                    &["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."]
                }
            }
        }
    } else {
        match (kind, long) {
            (Kind::Weekday, true) => &["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"],
            (Kind::Weekday, false) => &["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"],
            (Kind::Month, true) => &["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"],
            (Kind::Month, false) => {
                if tag == "en-gb" {
                    &["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sept", "Oct", "Nov", "Dec"]
                } else {
                    &["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
                }
            }
        }
    };
    raw.iter().map(|s| js::lower(s.strip_suffix('.').unwrap_or(s))).collect()
}

/// Base letter of the precomposed Latin letters that Unicode NFD splits into base + combining marks.
const DECOMPOSABLE: [(char, &str); 25] = [
    ('a', "àáâãäåāăąǎǟǡǻȁȃȧḁạảấầẩẫậắằẳẵặ"),
    ('c', "çćĉċčḉ"),
    ('d', "ďḋḍḏḑḓ"),
    ('e', "èéêëēĕėęěȅȇȩḕḗḙḛḝẹẻẽếềểễệ"),
    ('f', "ḟ"),
    ('g', "ĝğġģǧǵḡ"),
    ('h', "ĥȟḣḥḧḩḫẖ"),
    ('i', "ìíîïĩīĭįǐȉȋḭḯỉị"),
    ('j', "ĵǰ"),
    ('k', "ķǩḱḳḵ"),
    ('l', "ĺļľḷḹḻḽ"),
    ('m', "ḿṁṃ"),
    ('n', "ñńņňǹṅṇṉṋ"),
    ('o', "òóôõöōŏőơǒǫǭȍȏȫȭȯȱṍṏṑṓọỏốồổỗộớờởỡợ"),
    ('p', "ṕṗ"),
    ('r', "ŕŗřȑȓṙṛṝṟ"),
    ('s', "śŝşšșṡṣṥṧṩ"),
    ('t', "ţťțṫṭṯṱẗ"),
    ('u', "ùúûüũūŭůűųưǔǖǘǚǜȕȗṳṵṷṹṻụủứừửữự"),
    ('v', "ṽṿ"),
    ('w', "ŵẁẃẅẇẉẘ"),
    ('x', "ẋẍ"),
    ('y', "ýÿŷẏỳỵỷỹẙ"),
    ('z', "źżžẑẓẕ"),
    ('\0', ""),
];

/// `stripAccents`: `s.normalize('NFD')` without the combining marks U+0300–U+036F. The decomposition is
/// covered for the Latin letters (Latin-1, Latin Extended-A/B and Extended Additional), which is all that
/// can ever equal a month or weekday name.
fn strip_accents(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if ('\u{300}'..='\u{36f}').contains(&c) {
            continue;
        }
        match DECOMPOSABLE.iter().find(|(_, set)| set.contains(c)) {
            Some((base, _)) => out.push(*base),
            None => out.push(c),
        }
    }
    out
}

/// `nameMatch`: accent- and case-insensitive index of `s` in `names` (already lower-cased), or -1.
fn name_match(s: &str, names: &[String]) -> f64 {
    let t = s.strip_suffix('.').unwrap_or(s);
    let norm = strip_accents(&js::lower(js::trim(t)));
    names.iter().position(|n| strip_accents(n) == norm).map(|i| i as f64).unwrap_or(-1.0)
}

/// `applyNameStyle`: re-applies the source cell's casing (Title / ALL-CAPS / lower) and trailing dot.
fn apply_name_style(sample: &str, word: &str) -> String {
    let has_dot = re!(&concat!(r"\.", ws!(), r"*$")).is_some_and(|r| r.is_match(sample));
    let core: String = match re!(&concat!(r"\.", ws!(), r"*$")) {
        Some(r) => js::trim(&r.replace(sample, "")).to_string(),
        None => js::trim(sample).to_string(),
    };
    let upper_re = re!(r"\p{Lu}");
    let starts_upper = re!(r"^\p{Lu}");
    let mut out = word.to_string();
    if js::len16(&core) > 1 && core == js::upper(&core) && upper_re.is_some_and(|r| r.is_match(&core)) {
        out = js::upper(word);
    } else if starts_upper.is_some_and(|r| r.is_match(&core)) {
        let mut cs = word.chars();
        if let Some(first) = cs.next() {
            out = format!("{}{}", js::upper(&first.to_string()), cs.as_str());
        }
    }
    if has_dot {
        format!("{out}.")
    } else {
        out
    }
}

// ── numeric trend (shared by several detectors) ─────────────────────────────────

/// Extends a numeric sequence: geometric (≥3 points, constant ratio ≠ 1), else linear regression (≥3
/// points), else the arithmetic step of the last gap (a copy for a single cell).
fn number_series(nums: &[f64], count: usize, forward: bool) -> Vec<f64> {
    let n = nums.len();
    if n >= 3 && nums.iter().all(|&v| v != 0.0) && !const_step(nums) {
        let ratios: Vec<f64> = (1..n).map(|i| nums[i] / nums[i - 1]).collect();
        if ratios.iter().all(|r| (r - ratios[0]).abs() < 1e-9) && (ratios[0] - 1.0).abs() > 1e-9 {
            let r = ratios[0];
            let anchor = if forward { nums[n - 1] } else { nums[0] };
            return (0..count)
                .map(|k| {
                    let k = k as f64;
                    round10(if forward { anchor * js::pow(r, k + 1.0) } else { anchor * js::pow(r, -(k + 1.0)) })
                })
                .collect();
        }
    }
    if n >= 3 {
        let (mut sx, mut sy, mut sxx, mut sxy) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
        for (i, &v) in nums.iter().enumerate() {
            let i = i as f64;
            sx += i;
            sy += v;
            sxx += i * i;
            sxy += i * v;
        }
        let nf = n as f64;
        let b = (nf * sxy - sx * sy) / (nf * sxx - sx * sx);
        let a = (sy - b * sx) / nf;
        return (0..count)
            .map(|k| {
                let k = k as f64;
                let x = if forward { (nf - 1.0) + (k + 1.0) } else { -(k + 1.0) };
                round10(a + b * x)
            })
            .collect();
    }
    let step = if n >= 2 { nums[n - 1] - nums[n - 2] } else { 0.0 };
    let anchor = if forward { first_or_nan(nums, n.wrapping_sub(1)) } else { first_or_nan(nums, 0) };
    (0..count)
        .map(|k| {
            let k = k as f64;
            round10(if forward { anchor + step * (k + 1.0) } else { anchor - step * (k + 1.0) })
        })
        .collect()
}

// ── dates ───────────────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
struct DateFmt {
    ymd: bool,
    sep: String,
}

struct FlexDate {
    y: f64,
    m: f64,
    d: f64,
    fmt: DateFmt,
}

fn num_of(s: &str) -> f64 {
    js::number_or_nan(s)
}

/// `parseFlexDate`: `YYYY‹sep›M‹sep›D` or `D‹sep›M‹sep›YYYY` (sep = `-`, `/`, `.`, the same twice).
fn parse_flex_date(s: &str) -> Option<FlexDate> {
    if let Some(re) = re!(r"^([0-9]{4})([-/.])([0-9]{1,2})([-/.])([0-9]{1,2})$") {
        if let Some(m) = re.captures(s) {
            if m[2] == m[4] {
                return Some(FlexDate { y: num_of(&m[1]), m: num_of(&m[3]), d: num_of(&m[5]), fmt: DateFmt { ymd: true, sep: m[2].to_string() } });
            }
        }
    }
    if let Some(re) = re!(r"^([0-9]{1,2})([-/.])([0-9]{1,2})([-/.])([0-9]{4})$") {
        if let Some(m) = re.captures(s) {
            if m[2] == m[4] {
                return Some(FlexDate { y: num_of(&m[5]), m: num_of(&m[3]), d: num_of(&m[1]), fmt: DateFmt { ymd: false, sep: m[2].to_string() } });
            }
        }
    }
    None
}

fn fmt_flex_date(y: f64, m: f64, d: f64, fmt: &DateFmt) -> String {
    let dd = js::min(&[d, days_in_month(y, m)]); // never overflow (31 Feb → 28/29)
    let s = &fmt.sep;
    if fmt.ymd {
        format!("{}{}{}{}{}", js::to_string(y), s, pad2(m), s, pad2(dd))
    } else {
        format!("{}{}{}{}{}", pad2(dd), s, pad2(m), s, js::to_string(y))
    }
}

fn is_weekend(ms: f64) -> bool {
    jsdate::parts(jsdate::from_time(ms)).is_some_and(|p| p.weekday == 0 || p.weekday == 6)
}

/// `stepBusinessDays`: advances `n` business days (`n` may be negative).
fn step_business_days(mut ms: f64, n: f64) -> f64 {
    let dir = if n < 0.0 { -1.0 } else { 1.0 };
    let mut left = n.abs();
    while left > 0.0 {
        ms += dir * 86_400_000.0;
        if !is_weekend(ms) {
            left -= 1.0;
        }
    }
    ms
}

// ── roman numerals ──────────────────────────────────────────────────────────────

const ROMAN: [(&str, f64); 13] = [("M", 1000.0), ("CM", 900.0), ("D", 500.0), ("CD", 400.0), ("C", 100.0), ("XC", 90.0), ("L", 50.0), ("XL", 40.0), ("X", 10.0), ("IX", 9.0), ("V", 5.0), ("IV", 4.0), ("I", 1.0)];

fn to_roman(mut n: f64) -> String {
    if n < 1.0 || n > 3999.0 || n.is_nan() {
        return String::new();
    }
    let mut r = String::new();
    for (s, v) in ROMAN {
        while n >= v {
            r.push_str(s);
            n -= v;
        }
    }
    r
}

fn from_roman(s: &str) -> Option<f64> {
    let u = js::upper(s);
    if u.is_empty() || !u.chars().all(|c| "MDCLXVI".contains(c)) {
        return None;
    }
    let (mut i, mut n) = (0usize, 0.0f64);
    for (sym, v) in ROMAN {
        while u[i..].starts_with(sym) {
            n += v;
            i += sym.len();
        }
    }
    if i == u.len() && to_roman(n) == u {
        Some(n)
    } else {
        None
    }
}

// ── alphabetic base-26 ──────────────────────────────────────────────────────────

fn col_val(s: &str) -> f64 {
    let mut n = 0.0f64;
    for c in js::upper(s).chars() {
        n = n * 26.0 + (c as u32 as f64 - 64.0);
    }
    n
}

fn col_str(mut n: f64) -> String {
    let mut s: Vec<char> = Vec::new();
    while n > 0.0 {
        let r = (n - 1.0) % 26.0;
        s.push((65 + r as u8) as char);
        n = ((n - 1.0) / 26.0).floor();
    }
    s.iter().rev().collect()
}

// ── ordinals ────────────────────────────────────────────────────────────────────

fn ord_en(n: f64) -> &'static str {
    let v = n % 100.0;
    if (11.0..=13.0).contains(&v) {
        return "th";
    }
    let d = n % 10.0;
    if d == 1.0 {
        "st"
    } else if d == 2.0 {
        "nd"
    } else if d == 3.0 {
        "rd"
    } else {
        "th"
    }
}

// ── tokens (text + number) ──────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum TokType {
    Num,
    Word,
    Sep,
}

struct Tok {
    ty: TokType,
    val: String,
}

fn tokenize_cell(s: &str) -> Vec<Tok> {
    let Some(re) = re!(r"[0-9]+|\p{L}+|[^0-9\p{L}]+") else { return Vec::new() };
    re.find_iter(s)
        .map(|m| {
            let v = m.as_str();
            let ty = if v.chars().all(|c| c.is_ascii_digit()) {
                TokType::Num
            } else if re!(r"^\p{L}+$").is_some_and(|r| r.is_match(v)) {
                TokType::Word
            } else {
                TokType::Sep
            };
            Tok { ty, val: v.to_string() }
        })
        .collect()
}

/// `nameColumn`: continues a column of month/weekday names, or `None`.
fn name_column(col: &[String], count: usize, forward: bool, lang: &str) -> Option<Vec<String>> {
    for kind in [Kind::Weekday, Kind::Month] {
        for long in [true, false] {
            let names = locale_names(lang, kind, long);
            let idxs: Vec<f64> = col.iter().map(|s| name_match(s, &names)).collect();
            if idxs.iter().all(|&i| i >= 0.0) {
                let l = names.len() as f64;
                let step = or1(last_step(&idxs));
                let anchor = if forward { first_or_nan(&idxs, idxs.len().wrapping_sub(1)) } else { first_or_nan(&idxs, 0) };
                return Some(
                    (0..count)
                        .map(|k| {
                            let k = k as f64;
                            let raw = if forward { anchor + step * (k + 1.0) } else { anchor - step * (k + 1.0) };
                            let i = ((raw % l) + l) % l;
                            apply_name_style(&col[0], &names[i as usize])
                        })
                        .collect(),
                );
            }
        }
    }
    None
}

fn nums_out(v: Vec<f64>) -> Vec<Fill> {
    v.into_iter().map(Fill::Num).collect()
}

fn strs_out(v: Vec<String>) -> Vec<Fill> {
    v.into_iter().map(Fill::Str).collect()
}

/// A prefix match of ASCII letters, case-insensitive the way a JavaScript `/i` regex is (ASCII only).
fn ci(word: &str) -> String {
    word.chars().map(|c| format!("[{}{}]", c.to_ascii_uppercase(), c.to_ascii_lowercase())).collect()
}

// ════════════════════════════════════════════════════════════════════════════════

/// `fillSeries(srcVals, count, forward, lang)`: the `count` next values after (`forward`) or before
/// (`!forward`) the source cells, or `None` when no pattern is recognised. `count <= 0` gives `Some([])`.
pub fn fill_series(src_vals: &[Fill], count: i64, forward: bool, lang: &str) -> Option<Vec<Fill>> {
    if count <= 0 {
        return Some(Vec::new());
    }
    let count = count as usize;
    let strs_owned: Vec<String> = src_vals.iter().map(|v| js::trim(&v.to_js_string()).to_string()).collect();
    let strs: Vec<&str> = strs_owned.iter().map(String::as_str).collect();
    if !strs.iter().all(|s| !s.is_empty()) {
        return None;
    }
    let gen = |f: &mut dyn FnMut(f64) -> String| -> Vec<Fill> { (0..count).map(|k| Fill::Str(f(k as f64 + 1.0))).collect() };

    // 1 ── Pure numbers (leading-zero pad preserved: 001, 002 → 003).
    if let Some(re) = re!(r"^-?[0-9]+(?:\.[0-9]+)?$") {
        if strs.iter().all(|s| re.is_match(s)) {
            let nums: Vec<f64> = strs.iter().map(|s| num_of(s)).collect();
            let all_digits = strs.iter().all(|s| s.chars().all(|c| c.is_ascii_digit()));
            let pad_w = if all_digits && strs.iter().any(|s| s.len() > 1 && s.starts_with('0')) { strs.iter().map(|s| s.len()).max().unwrap_or(0) } else { 0 };
            let seq = number_series(&nums, count, forward);
            return Some(if pad_w > 0 { seq.into_iter().map(|n| Fill::Str(js::pad_start(&js::to_string(js::round(n)), pad_w, '0'))).collect() } else { nums_out(seq) });
        }
    }

    // 2 ── Booleans (TRUE/FALSE, VRAI/FAUX, Yes/No…) → cycle, preserving the source case.
    const BOOL_SETS: [[&str; 2]; 5] = [["true", "false"], ["vrai", "faux"], ["yes", "no"], ["oui", "non"], ["on", "off"]];
    for set in BOOL_SETS {
        if !strs.is_empty() && strs.iter().all(|s| set.contains(&js::lower(s).as_str())) {
            let n = strs.len() as f64;
            return Some(gen(&mut |k| {
                let idx = if forward { n - 1.0 + k } else { -k };
                strs[((idx % n + n) % n) as usize].to_string()
            }));
        }
    }

    // 3 ── Thousands-grouped numbers (1,000 → 2,000 / 1 000 → 2 000).
    if let Some(re) = re!(r"^(-?[0-9]{1,3}(?:([, ])[0-9]{3})+)$") {
        let grp: Vec<Option<regex::Captures>> = strs.iter().map(|s| re.captures(s)).collect();
        if grp.iter().all(Option::is_some) {
            let sep_ch = grp[0].as_ref().and_then(|c| c.get(2)).map(|m| m.as_str().to_string()).unwrap_or_default();
            let nums: Vec<f64> = strs.iter().map(|s| num_of(&s.replace([',', ' '], ""))).collect();
            let fmt_group = |n: f64| -> String {
                let neg = n < 0.0;
                let digits: Vec<char> = js::to_string(js::round(n.abs())).chars().collect();
                let mut out = String::new();
                for (i, d) in digits.iter().enumerate() {
                    if i > 0 && (digits.len() - i).is_multiple_of(3) {
                        out.push_str(&sep_ch);
                    }
                    out.push(*d);
                }
                format!("{}{}", if neg { "-" } else { "" }, out)
            };
            return Some(strs_out(number_series(&nums, count, forward).into_iter().map(fmt_group).collect()));
        }
    }

    // 4 ── 24-hour times HH:MM[:SS].
    if let Some(re) = re!(r"^([0-9]{1,2}):([0-9]{2})(?::([0-9]{2}))?$") {
        let tm: Vec<Option<regex::Captures>> = strs.iter().map(|s| re.captures(s)).collect();
        if tm.iter().all(Option::is_some) {
            let secs: Vec<f64> = tm
                .iter()
                .flatten()
                .map(|m| num_of(&m[1]) * 3600.0 + num_of(&m[2]) * 60.0 + m.get(3).map(|x| num_of(x.as_str())).unwrap_or(0.0))
                .collect();
            let has_sec = tm[0].as_ref().is_some_and(|m| m.get(3).is_some());
            let step = if secs.len() >= 2 { secs[secs.len() - 1] - secs[secs.len() - 2] } else { 3600.0 };
            let anchor = if forward { first_or_nan(&secs, secs.len().wrapping_sub(1)) } else { first_or_nan(&secs, 0) };
            return Some(gen(&mut |k| {
                let mut t = if forward { anchor + step * k } else { anchor - step * k };
                t = ((t % 86400.0) + 86400.0) % 86400.0;
                let h = (t / 3600.0).floor();
                let mi = ((t % 3600.0) / 60.0).floor();
                let se = t % 60.0;
                if has_sec {
                    format!("{}:{}:{}", pad2(h), pad2(mi), pad2(se))
                } else {
                    format!("{}:{}", pad2(h), pad2(mi))
                }
            }));
        }
    }

    // 5 ── 12-hour times with meridiem (1:00 PM → 2:00 PM), case and dotting preserved.
    if let Some(re) = re!(concat!(r"^([0-9]{1,2}):([0-9]{2})(?::([0-9]{2}))?", ws!(), r"*([AaPp])(\.?)[Mm](\.?)$")) {
        let ap: Vec<Option<regex::Captures>> = strs.iter().map(|s| re.captures(s)).collect();
        if ap.iter().all(Option::is_some) {
            let secs: Vec<f64> = ap
                .iter()
                .flatten()
                .map(|m| {
                    let mut h = num_of(&m[1]) % 12.0;
                    if m[4].eq_ignore_ascii_case("p") {
                        h += 12.0;
                    }
                    h * 3600.0 + num_of(&m[2]) * 60.0 + m.get(3).map(|x| num_of(x.as_str())).unwrap_or(0.0)
                })
                .collect();
            let first = ap[0].as_ref()?;
            let has_sec = first.get(3).is_some();
            let upper = first[4] == js::upper(&first[4]);
            let dots = &first[5] == ".";
            let step = if secs.len() >= 2 { secs[secs.len() - 1] - secs[secs.len() - 2] } else { 3600.0 };
            let anchor = if forward { first_or_nan(&secs, secs.len().wrapping_sub(1)) } else { first_or_nan(&secs, 0) };
            return Some(gen(&mut |k| {
                let mut t = if forward { anchor + step * k } else { anchor - step * k };
                t = ((t % 86400.0) + 86400.0) % 86400.0;
                let h = (t / 3600.0).floor();
                let mi = ((t % 3600.0) / 60.0).floor();
                let se = t % 60.0;
                let mut h12 = h % 12.0;
                if h12 == 0.0 {
                    h12 = 12.0;
                }
                let mut mer = format!("{}{}m{}", if h < 12.0 { "a" } else { "p" }, if dots { "." } else { "" }, if dots { "." } else { "" });
                if upper {
                    mer = js::upper(&mer);
                }
                format!("{}:{}{} {}", js::to_string(h12), pad2(mi), if has_sec { format!(":{}", pad2(se)) } else { String::new() }, mer)
            }));
        }
    }

    // 6 ── Dates: daily / weekly / weekday-only / end-of-month / monthly / yearly.
    let dts: Vec<Option<FlexDate>> = strs.iter().map(|s| parse_flex_date(s)).collect();
    if dts.iter().all(Option::is_some) && {
        let keys: Vec<(bool, &str)> = dts.iter().flatten().map(|d| (d.fmt.ymd, d.fmt.sep.as_str())).collect();
        all_equal(&keys)
    } {
        let dd: Vec<&FlexDate> = dts.iter().flatten().collect();
        let fmt = dd[0].fmt.clone();
        let ms: Vec<f64> = dd.iter().map(|d| jsdate::new_date(d.y, d.m - 1.0, d.d, 0.0, 0.0, 0.0, 0.0)).collect();
        let month_idx: Vec<f64> = dd.iter().map(|d| d.y * 12.0 + (d.m - 1.0)).collect();
        let out = |mss: f64| -> String {
            match jsdate::parts(jsdate::from_time(mss)) {
                Some(p) => fmt_flex_date(p.year as f64, p.month as f64 + 1.0, p.date as f64, &fmt),
                None => fmt_flex_date(f64::NAN, f64::NAN, f64::NAN, &fmt),
            }
        };

        // End-of-month: every source is the last day of its month and months progress.
        if dd.iter().all(|p| p.d == days_in_month(p.y, p.m)) && const_step(&month_idx) && consecutive_diffs(&month_idx)[0] != 0.0 {
            let step = last_step(&month_idx);
            let anchor = if forward { month_idx[month_idx.len() - 1] } else { month_idx[0] };
            return Some(gen(&mut |k| {
                let mi = if forward { anchor + step * k } else { anchor - step * k };
                let y = (mi / 12.0).floor();
                let m = (mi % 12.0) + 1.0;
                fmt_flex_date(y, m, days_in_month(y, m), &fmt)
            }));
        }
        // Weekday-only: consecutive business days, with at least one Fri→Mon weekend gap.
        if dd.len() >= 2 && ms.iter().all(|&m| !is_weekend(m)) {
            let diffs: Vec<f64> = (1..ms.len()).map(|i| js::round((ms[i] - ms[i - 1]) / 86_400_000.0)).collect();
            let business_consec = (1..ms.len()).all(|i| step_business_days(ms[i - 1], 1.0) == ms[i]);
            if business_consec && diffs.iter().any(|&d| d > 1.0) {
                let anchor = if forward { ms[ms.len() - 1] } else { ms[0] };
                return Some(gen(&mut |k| out(step_business_days(anchor, if forward { k } else { -k }))));
            }
        }
        let days: Vec<f64> = dd.iter().map(|d| d.d).collect();
        let months: Vec<f64> = dd.iter().map(|d| d.m).collect();
        let years: Vec<f64> = dd.iter().map(|d| d.y).collect();
        let same_day = all_equal(&days);
        let same_month_day = all_equal(&months) && same_day;
        if same_day && const_step(&month_idx) && consecutive_diffs(&month_idx)[0] % 12.0 != 0.0 && consecutive_diffs(&month_idx)[0] != 0.0 {
            // monthly
            let step = last_step(&month_idx);
            let anchor = if forward { month_idx[month_idx.len() - 1] } else { month_idx[0] };
            return Some(gen(&mut |k| {
                let mi = if forward { anchor + step * k } else { anchor - step * k };
                fmt_flex_date((mi / 12.0).floor(), (mi % 12.0) + 1.0, dd[0].d, &fmt)
            }));
        }
        if same_month_day && const_step(&years) && consecutive_diffs(&years)[0] != 0.0 {
            // yearly
            let step = last_step(&years);
            let anchor = if forward { years[years.len() - 1] } else { years[0] };
            return Some(gen(&mut |k| fmt_flex_date(if forward { anchor + step * k } else { anchor - step * k }, dd[0].m, dd[0].d, &fmt)));
        }
        let day_step = if ms.len() >= 2 { js::round((ms[ms.len() - 1] - ms[ms.len() - 2]) / 86_400_000.0) } else { 1.0 }; // daily / weekly
        let anchor = if forward { ms[ms.len() - 1] } else { ms[0] };
        return Some(gen(&mut |k| out(anchor + (if forward { day_step * k } else { -day_step * k }) * 86_400_000.0)));
    }

    // 7 ── Quarters Qn / Tn (+ optional year): cycle 1..4 with carry into the year.
    let qpat = format!(r"^([Qq]|[Tt]|{}{ws}*|{}{ws}*)([1-4])(?:({ws}+)([0-9]{{4}}))?$", ci("Quarter"), ci("Trimestre"), ws = ws!());
    if let Some(qre) = Regex::new(&qpat).ok().as_ref() {
        let qz: Vec<Option<regex::Captures>> = strs.iter().map(|s| qre.captures(s)).collect();
        if qz.iter().all(Option::is_some) {
            let qq: Vec<&regex::Captures> = qz.iter().flatten().collect();
            let pre_lower: Vec<String> = qq.iter().map(|m| js::lower(&m[1])).collect();
            let year_flag: Vec<bool> = qq.iter().map(|m| m.get(4).is_some()).collect();
            if all_equal(&pre_lower) && all_equal(&year_flag) {
                let has_year = qq[0].get(4).is_some();
                let pre = qq[0][1].to_string();
                let sep = qq[0].get(3).map(|m| m.as_str().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| " ".to_string());
                let abs: Vec<f64> = qq
                    .iter()
                    .map(|m| if has_year { num_of(m.get(4).map(|x| x.as_str()).unwrap_or("")) * 4.0 + (num_of(&m[2]) - 1.0) } else { num_of(&m[2]) - 1.0 })
                    .collect();
                if if has_year { const_step(&abs) } else { true } {
                    let step = or1(last_step(&abs));
                    let anchor = if forward { abs[abs.len() - 1] } else { abs[0] };
                    return Some(gen(&mut |k| {
                        let a = if forward { anchor + step * k } else { anchor - step * k };
                        let q = ((a % 4.0) + 4.0) % 4.0 + 1.0;
                        if has_year {
                            format!("{}{}{}{}", pre, js::to_string(q), sep, js::to_string((a / 4.0).floor()))
                        } else {
                            format!("{}{}", pre, js::to_string(q))
                        }
                    }));
                }
            }
        }
    }

    // 8 ── "<month> <year>" (Jan 2024 → Feb 2024 … Dec 2024 → Jan 2025).
    if let Some(re) = re!(concat!(r"^(\p{L}+)(", ws!(), r"+)([0-9]{4})$")) {
        let my: Vec<Option<regex::Captures>> = strs.iter().map(|s| re.captures(s)).collect();
        if my.iter().all(Option::is_some) {
            let my: Vec<&regex::Captures> = my.iter().flatten().collect();
            for long in [true, false] {
                let names = locale_names(lang, Kind::Month, long);
                let idxs: Vec<f64> = my.iter().map(|p| name_match(&p[1], &names)).collect();
                if idxs.iter().all(|&i| i >= 0.0) {
                    let abs: Vec<f64> = my.iter().zip(&idxs).map(|(p, &i)| num_of(&p[3]) * 12.0 + i).collect();
                    if const_step(&abs) || abs.len() == 1 {
                        let step = or1(last_step(&abs));
                        let anchor = if forward { abs[abs.len() - 1] } else { abs[0] };
                        let sep = my[0][2].to_string();
                        let first_name = my[0][1].to_string();
                        return Some(gen(&mut |k| {
                            let a = if forward { anchor + step * k } else { anchor - step * k };
                            format!("{}{}{}", apply_name_style(&first_name, &names[(((a % 12.0) + 12.0) % 12.0) as usize]), sep, js::to_string((a / 12.0).floor()))
                        }));
                    }
                }
            }
        }
    }

    // 9 ── Month / weekday names alone (long/short, accents, ALL-CAPS, dot).
    if let Some(names) = name_column(&strs_owned, count, forward, lang) {
        return Some(strs_out(names));
    }

    // 10 ── Roman numerals (≥2 cells, at least one multi-char; single letters fall through).
    if strs.len() >= 2 && strs.iter().any(|s| js::len16(s) > 1) {
        let rv: Vec<Option<f64>> = strs.iter().map(|s| from_roman(s)).collect();
        if rv.iter().all(Option::is_some) {
            let v: Vec<f64> = rv.iter().flatten().copied().collect();
            if const_step(&v) && consecutive_diffs(&v)[0] != 0.0 {
                let lower = strs.iter().all(|s| *s == js::lower(s));
                let step = or1(last_step(&v));
                let anchor = if forward { v[v.len() - 1] } else { v[0] };
                return Some(gen(&mut |k| {
                    let n = if forward { anchor + step * k } else { anchor - step * k };
                    let r = to_roman(n);
                    if lower {
                        js::lower(&r)
                    } else {
                        r
                    }
                }));
            }
        }
    }

    // 11 ── Alphabetic base-26 letters (A,B → C ; AA,AB → AC), single letters or short UPPERCASE runs.
    let letter_like = re!(r"^[A-Za-z]$").is_some_and(|r| strs.iter().all(|s| r.is_match(s))) || re!(r"^[A-Z]{1,3}$").is_some_and(|r| strs.iter().all(|s| r.is_match(s)));
    if strs.len() >= 2 && letter_like && all_equal(&strs.iter().map(|s| *s == js::upper(s)).collect::<Vec<_>>()) {
        let v: Vec<f64> = strs.iter().map(|s| col_val(s)).collect();
        if const_step(&v) && consecutive_diffs(&v)[0] != 0.0 {
            let lower = strs[0] == js::lower(strs[0]);
            let step = or1(last_step(&v));
            let anchor = if forward { v[v.len() - 1] } else { v[0] };
            return Some(gen(&mut |k| {
                let n = if forward { anchor + step * k } else { anchor - step * k };
                if n < 1.0 {
                    return String::new();
                }
                let s = col_str(n);
                if lower {
                    js::lower(&s)
                } else {
                    s
                }
            }));
        }
    }

    // 12 ── Ordinals: English (1st,2nd → 3rd) and French (1er,2e → 3e).
    let oen_re = Regex::new(&format!(r"^([0-9]+)({}|{}|{}|{})$", ci("st"), ci("nd"), ci("rd"), ci("th"))).ok();
    if let Some(re) = oen_re.as_ref() {
        let oen: Vec<Option<regex::Captures>> = strs.iter().map(|s| re.captures(s)).collect();
        if oen.iter().all(Option::is_some) {
            let oo: Vec<&regex::Captures> = oen.iter().flatten().collect();
            let nums: Vec<f64> = oo.iter().map(|m| num_of(&m[1])).collect();
            let up = oo[0][2] == js::upper(&oo[0][2]);
            let step = or1(last_step(&nums));
            let anchor = if forward { nums[nums.len() - 1] } else { nums[0] };
            return Some(gen(&mut |k| {
                let n = if forward { anchor + step * k } else { anchor - step * k };
                if n < 1.0 {
                    return String::new();
                }
                let suf = if up { js::upper(ord_en(n)) } else { ord_en(n).to_string() };
                format!("{}{}", js::to_string(n), suf)
            }));
        }
    }
    let ofr_re = Regex::new(&format!(r"^([0-9]+)({}{}|{}{}|{}{}|{})$", ci("er"), ci("s") + "?", ci("re"), ci("s") + "?", ci("e"), ci("s") + "?", ci("d"))).ok();
    if let Some(re) = ofr_re.as_ref() {
        let ofr: Vec<Option<regex::Captures>> = strs.iter().map(|s| re.captures(s)).collect();
        if ofr.iter().all(Option::is_some) && strs.iter().any(|s| s.chars().any(|c| !c.is_ascii_digit())) {
            let nums: Vec<f64> = ofr.iter().flatten().map(|m| num_of(&m[1])).collect();
            let step = or1(last_step(&nums));
            let anchor = if forward { nums[nums.len() - 1] } else { nums[0] };
            return Some(gen(&mut |k| {
                let n = if forward { anchor + step * k } else { anchor - step * k };
                if n < 1.0 {
                    return String::new();
                }
                format!("{}{}", js::to_string(n), if n == 1.0 { "er" } else { "e" })
            }));
        }
    }

    // 13 ── Single number wrapped in fixed affixes: currency / percent / decimals.
    if let Some(re) = re!(r"^([^0-9]*?)(-?[0-9]+(?:\.[0-9]+)?)([^0-9]*)$") {
        let af: Vec<Option<regex::Captures>> = strs.iter().map(|s| re.captures(s)).collect();
        if af.iter().all(Option::is_some) {
            let aa: Vec<&regex::Captures> = af.iter().flatten().collect();
            let pres: Vec<&str> = aa.iter().map(|m| m.get(1).map(|x| x.as_str()).unwrap_or("")).collect();
            let sufs: Vec<&str> = aa.iter().map(|m| m.get(3).map(|x| x.as_str()).unwrap_or("")).collect();
            if all_equal(&pres) && all_equal(&sufs) {
                let (pre, suf) = (pres[0], sufs[0]);
                let decimals = aa
                    .iter()
                    .map(|m| {
                        let t = &m[2];
                        match t.find('.') {
                            Some(dot) => t.len() - dot - 1,
                            None => 0,
                        }
                    })
                    .max()
                    .unwrap_or(0);
                let nums: Vec<f64> = aa.iter().map(|m| num_of(&m[2])).collect();
                // A single affixed number increments by 1 (Item1 → Item2); two or more follow the trend.
                let seq: Vec<f64> = if nums.len() >= 2 { number_series(&nums, count, forward) } else { (0..count).map(|k| if forward { nums[0] + (k + 1) as f64 } else { nums[0] - (k + 1) as f64 }).collect() };
                return Some(strs_out(
                    seq.into_iter().map(|n| format!("{}{}{}", pre, if decimals > 0 { js::to_fixed(n, decimals as u32) } else { js::to_string(js::round(n)) }, suf)).collect(),
                ));
            }
        }
    }

    // 14 ── Generic "text + number" tokens: aligns cells token by token and extends each varying token.
    let toks: Vec<Vec<Tok>> = strs.iter().map(|s| tokenize_cell(s)).collect();
    let len = toks[0].len();
    if len > 0 && toks.iter().all(|t| t.len() == len && t.iter().enumerate().all(|(i, tk)| tk.ty == toks[0][i].ty)) {
        let mut col_series: Vec<Vec<String>> = Vec::new();
        let is_const = |col: &[String]| col.len() >= 2 && all_equal(col);
        let num_idx: Vec<usize> = toks[0].iter().enumerate().filter(|(_, tk)| tk.ty == TokType::Num).map(|(i, _)| i).collect();
        let last_num_idx = num_idx.last().copied();
        let mut ok = true;
        let mut any_varies = false;
        let mut i = 0usize;
        while i < len && ok {
            let col: Vec<String> = toks.iter().map(|t| t[i].val.clone()).collect();
            let ty = toks[0][i].ty;
            if is_const(&col) {
                col_series.push(vec![col[0].clone(); count]);
                i += 1;
                continue;
            }
            if ty == TokType::Sep {
                if all_equal(&col) {
                    col_series.push(vec![col[0].clone(); count]);
                    i += 1;
                    continue;
                }
                ok = false;
                break;
            }
            if ty == TokType::Num {
                let pad_w = if col.iter().any(|x| js::len16(x) > 1 && x.starts_with('0')) { col.iter().map(|x| js::len16(x)).max().unwrap_or(0) } else { 0 };
                let ni: Vec<f64> = col.iter().map(|x| js::parse_int(x, 10)).collect();
                let advance = ni.len() >= 2 || Some(i) == last_num_idx;
                let seq: Vec<f64> = if ni.len() >= 2 {
                    number_series(&ni, count, forward)
                } else if advance {
                    (0..count).map(|k| if forward { ni[0] + (k + 1) as f64 } else { ni[0] - (k + 1) as f64 }).collect()
                } else {
                    vec![ni[0]; count]
                };
                col_series.push(
                    seq.into_iter()
                        .map(|v| {
                            let r = js::round(v);
                            if pad_w > 0 {
                                let clamped = if r.is_nan() { f64::NAN } else if r < 0.0 { 0.0 } else { r };
                                js::pad_start(&js::to_string(clamped), pad_w, '0')
                            } else {
                                js::to_string(r)
                            }
                        })
                        .collect(),
                );
                if advance {
                    any_varies = true;
                }
                i += 1;
                continue;
            }
            if let Some(ws) = name_column(&col, count, forward, lang) {
                col_series.push(ws);
                any_varies = true;
                i += 1;
                continue;
            }
            if all_equal(&col) {
                col_series.push(vec![col[0].clone(); count]);
                i += 1;
                continue;
            }
            ok = false;
            break;
        }
        if ok && any_varies {
            return Some(gen(&mut |k| col_series.iter().map(|c| c[k as usize - 1].as_str()).collect::<String>()));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<Fill> {
        v.iter().map(|x| Fill::Str((*x).to_string())).collect()
    }

    fn run(v: &[&str], count: i64, forward: bool, lang: &str) -> Option<Vec<String>> {
        fill_series(&s(v), count, forward, lang).map(|r| r.iter().map(Fill::to_js_string).collect())
    }

    #[test]
    fn numbers() {
        assert_eq!(run(&["1", "2"], 3, true, "fr"), Some(vec!["3".into(), "4".into(), "5".into()]));
        assert_eq!(run(&["001", "002"], 2, true, "fr"), Some(vec!["003".into(), "004".into()]));
        assert_eq!(run(&["2", "4", "8"], 2, true, "fr"), Some(vec!["16".into(), "32".into()]));
        assert_eq!(run(&["5"], 2, false, "en"), Some(vec!["5".into(), "5".into()]));
    }

    #[test]
    fn names_and_calendar() {
        assert_eq!(run(&["lundi"], 3, true, "fr"), Some(vec!["mardi".into(), "mercredi".into(), "jeudi".into()]));
        assert_eq!(run(&["Dec"], 2, true, "en"), Some(vec!["Jan".into(), "Feb".into()]));
        assert_eq!(run(&["janv."], 2, true, "fr"), Some(vec!["févr.".into(), "mars.".into()]));
        assert_eq!(run(&["Dec 2024"], 2, true, "en"), Some(vec!["Jan 2025".into(), "Feb 2025".into()]));
        assert_eq!(run(&["2024-01-31", "2024-02-29"], 2, true, "en"), Some(vec!["2024-03-31".into(), "2024-04-30".into()]));
        assert_eq!(run(&["Q4"], 2, true, "en"), Some(vec!["Q1".into(), "Q2".into()]));
        assert_eq!(run(&["FÉVRIER"], 1, true, "fr"), Some(vec!["MARS".into()]));
    }

    #[test]
    fn misc_patterns() {
        assert_eq!(run(&["Item1"], 2, true, "en"), Some(vec!["Item2".into(), "Item3".into()]));
        assert_eq!(run(&["Item-12"], 2, true, "en"), Some(vec!["Item-11".into(), "Item-10".into()])); // QUIRK
        assert_eq!(run(&["I", "II"], 2, true, "en"), Some(vec!["III".into(), "IV".into()]));
        assert_eq!(run(&["A", "B"], 2, true, "en"), Some(vec!["C".into(), "D".into()]));
        assert_eq!(run(&["1st", "2nd"], 2, true, "en"), Some(vec!["3rd".into(), "4th".into()]));
        assert_eq!(run(&["9:00 AM"], 2, true, "en"), Some(vec!["10:00 AM".into(), "11:00 AM".into()]));
        assert_eq!(run(&["1,000", "2,000"], 1, true, "en"), Some(vec!["3,000".into()]));
        assert_eq!(run(&["hello"], 2, true, "en"), None);
        assert_eq!(run(&[""], 2, true, "en"), None);
        assert_eq!(fill_series(&s(&["1"]), 0, true, "en"), Some(vec![]));
    }
}
