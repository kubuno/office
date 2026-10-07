//! Port of the web's `formula-fns/date.ts`: the Date & Time functions.
//!
//! Dates are Excel serials (days since 1899-12-30, the fraction is the time of day). Every date is built
//! and read with the zone-less JavaScript `Date` model of [`crate::jsdate`].
//!
//! `DATEVALUE` ends in `Date.parse` on the web. Rust has no such function, so the parts of V8's date
//! parser that can be reached are ported in [`js_date_parse`] (the ES5 ISO format and V8's legacy
//! free-form parser: month names, `a.m.`/`p.m.`, `GMT`/`UTC`/`Z`/US zone names, numeric offsets, parentheses
//! comments). The result's calendar date (local year/month/day) is all `DATEVALUE` reads. Differences
//! from V8: none known for the tokens above; two-digit years and defaulted fields follow V8's rules
//! (`"March 5"` is 2001-03-05, as in Chrome).

use std::collections::HashSet;
use std::sync::OnceLock;

use regex::Regex;

use super::Registry;
use crate::formula::ast::Node;
use crate::formula::eval::{arg, Evaluator};
use crate::formula::value::{flatten, to_num, ErrCode, Value};
use crate::js;
use crate::jsdate;

type R = Result<Value, ErrCode>;

/// A cap on the day-by-day loops (NETWORKDAYS / WORKDAY): beyond it the web would hang its tab for
/// minutes (or forever for a weekend mask that excludes every day); the desktop answers `#VALUE!`.
const MAX_LOOP: u64 = 100_000_000;

// ── Helpers ──────────────────────────────────────────────────────────────────

/// `ymdToSerial(y, m, d)`: `excelSerial(new Date(y, m - 1, d))`.
fn ymd_to_serial(y: f64, m: f64, d: f64) -> f64 {
    jsdate::excel_serial(jsdate::ymd(y, m - 1.0, d))
}

fn re_iso() -> Option<&'static Regex> {
    static R: OnceLock<Option<Regex>> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^([0-9]{4})-([0-9]{1,2})-([0-9]{1,2})").ok()).as_ref()
}

fn re_mdy() -> Option<&'static Regex> {
    static R: OnceLock<Option<Regex>> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^([0-9]{1,2})[/.]([0-9]{1,2})[/.]([0-9]{2,4})$").ok()).as_ref()
}

fn re_time() -> Option<&'static Regex> {
    static R: OnceLock<Option<Regex>> = OnceLock::new();
    R.get_or_init(|| {
        // `\s` of JavaScript is spelled out (U+FEFF in, U+0085 out); `\d` is ASCII.
        let ws = r"[\t\n\x0B\x0C\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}]";
        Regex::new(&format!("([0-9]{{1,2}}):([0-9]{{1,2}})(?::([0-9]{{1,2}}))?{ws}*((?i:am|pm))?")).ok()
    })
    .as_ref()
}

/// `+s` of an all-digit string.
fn digits(s: &str) -> f64 {
    js::number_or_nan(s)
}

/// `parseDate(text)`: a textual date as a serial (`None` on failure).
fn parse_date(text: &str) -> Option<f64> {
    let t = js::trim(text);
    if t.is_empty() {
        return None;
    }
    if let Some(m) = re_iso().and_then(|r| r.captures(t)) {
        return Some(ymd_to_serial(digits(&m[1]), digits(&m[2]), digits(&m[3])));
    }
    if let Some(m) = re_mdy().and_then(|r| r.captures(t)) {
        let mut y = digits(&m[3]);
        if y < 100.0 {
            y += if y < 30.0 { 2000.0 } else { 1900.0 };
        }
        return Some(ymd_to_serial(y, digits(&m[1]), digits(&m[2])));
    }
    // Fall back to the JS date parser.
    let parsed = js_date_parse(t);
    if !parsed.is_nan() {
        let p = jsdate::parts(jsdate::from_time(parsed))?;
        return Some(ymd_to_serial(p.year as f64, p.month as f64 + 1.0, p.date as f64));
    }
    None
}

// ── Date.parse (V8) ──────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kw {
    Month,
    AmPm,
    TzName,
    TimeSep,
    Invalid,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Tk {
    End,
    Invalid,
    Unknown,
    Num { n: i64, len: usize },
    Sym(char),
    Ws,
    Word { kind: Kw, value: i64, len: usize },
}

impl Tk {
    fn is_num(self) -> bool {
        matches!(self, Tk::Num { .. })
    }
    fn fixed(self, n: usize) -> Option<i64> {
        match self {
            Tk::Num { n: v, len } if len == n => Some(v),
            _ => None,
        }
    }
    fn is_sign(self) -> bool {
        matches!(self, Tk::Sym('+') | Tk::Sym('-'))
    }
    fn is_z(self) -> bool {
        matches!(self, Tk::Word { kind: Kw::TzName, len: 1, .. })
    }
}

const KEYWORDS: [(&str, Kw, i64); 27] = [
    ("jan", Kw::Month, 1),
    ("feb", Kw::Month, 2),
    ("mar", Kw::Month, 3),
    ("apr", Kw::Month, 4),
    ("may", Kw::Month, 5),
    ("jun", Kw::Month, 6),
    ("jul", Kw::Month, 7),
    ("aug", Kw::Month, 8),
    ("sep", Kw::Month, 9),
    ("oct", Kw::Month, 10),
    ("nov", Kw::Month, 11),
    ("dec", Kw::Month, 12),
    ("am", Kw::AmPm, 0),
    ("pm", Kw::AmPm, 12),
    ("ut", Kw::TzName, 0),
    ("utc", Kw::TzName, 0),
    ("z", Kw::TzName, 0),
    ("gmt", Kw::TzName, 0),
    ("cdt", Kw::TzName, -5),
    ("cst", Kw::TzName, -6),
    ("edt", Kw::TzName, -4),
    ("est", Kw::TzName, -5),
    ("mdt", Kw::TzName, -6),
    ("mst", Kw::TzName, -7),
    ("pdt", Kw::TzName, -7),
    ("pst", Kw::TzName, -8),
    ("t", Kw::TimeSep, 0),
];

struct Scanner {
    chars: Vec<char>,
    pos: usize,
    next_: Tk,
}

impl Scanner {
    fn new(s: &str) -> Scanner {
        let mut sc = Scanner { chars: s.chars().collect(), pos: 0, next_: Tk::End };
        sc.next_ = sc.scan();
        sc
    }

    fn scan(&mut self) -> Tk {
        let start = self.pos;
        let Some(&c) = self.chars.get(self.pos) else { return Tk::End };
        if c.is_ascii_digit() {
            // ReadUnsignedNumeral: leading zeros skipped, at most 9 significant digits kept.
            let mut n: i64 = 0;
            let mut i = 0;
            while self.chars.get(self.pos) == Some(&'0') {
                self.pos += 1;
            }
            while let Some(d) = self.chars.get(self.pos).filter(|c| c.is_ascii_digit()).and_then(|c| c.to_digit(10)) {
                if i < 9 {
                    n = n * 10 + d as i64;
                }
                i += 1;
                self.pos += 1;
            }
            return Tk::Num { n, len: self.pos - start };
        }
        if matches!(c, ':' | '-' | '+' | '.' | ')') {
            self.pos += 1;
            return Tk::Sym(c);
        }
        if c >= 'A' && !js::is_whitespace(c) {
            let mut buf: [char; 3] = ['\0'; 3];
            let mut len = 0usize;
            while let Some(&ch) = self.chars.get(self.pos) {
                if ch < 'A' || js::is_whitespace(ch) {
                    break;
                }
                if len < 3 {
                    buf[len] = ch.to_ascii_lowercase();
                }
                len += 1;
                self.pos += 1;
            }
            for (name, kind, value) in KEYWORDS.iter() {
                let mut padded = ['\0'; 3];
                for (i, ch) in name.chars().take(3).enumerate() {
                    padded[i] = ch;
                }
                if padded == buf && (len <= 3 || *kind == Kw::Month) {
                    return Tk::Word { kind: *kind, value: *value, len };
                }
            }
            return Tk::Word { kind: Kw::Invalid, value: 0, len };
        }
        if js::is_whitespace(c) {
            while self.chars.get(self.pos).is_some_and(|&ch| js::is_whitespace(ch)) {
                self.pos += 1;
            }
            return Tk::Ws;
        }
        if c == '(' {
            let mut balance = 0i32;
            loop {
                match self.chars.get(self.pos) {
                    Some(')') => balance -= 1,
                    Some('(') => balance += 1,
                    _ => {}
                }
                self.pos += 1;
                if !(balance > 0 && self.pos < self.chars.len()) {
                    break;
                }
            }
            return Tk::Unknown;
        }
        self.pos += 1;
        Tk::Unknown
    }

    fn peek(&self) -> Tk {
        self.next_
    }

    fn next(&mut self) -> Tk {
        let t = self.next_;
        self.next_ = self.scan();
        t
    }

    fn skip_symbol(&mut self, c: char) -> bool {
        if self.next_ == Tk::Sym(c) {
            self.next();
            true
        } else {
            false
        }
    }
}

fn between(x: i64, lo: i64, hi: i64) -> bool {
    x >= lo && x <= hi
}

#[derive(Default)]
struct Day {
    comp: [i64; 3],
    index: usize,
    named_month: Option<i64>,
    iso: bool,
}

impl Day {
    fn add(&mut self, n: i64) -> bool {
        if self.index < 3 {
            self.comp[self.index] = n;
            self.index += 1;
            true
        } else {
            false
        }
    }

    /// `(year, month 0-based, day)`.
    fn write(&mut self) -> Option<(i64, i64, i64)> {
        if self.index < 1 {
            return None;
        }
        while self.index < 3 {
            self.comp[self.index] = 1;
            self.index += 1;
        }
        let (mut year, month, day);
        let is_day = |x: i64| between(x, 1, 31);
        match self.named_month {
            None => {
                if self.iso || (self.index == 3 && !is_day(self.comp[0])) {
                    year = self.comp[0];
                    month = self.comp[1];
                    day = self.comp[2];
                } else {
                    month = self.comp[0];
                    day = self.comp[1];
                    year = if self.index == 3 { self.comp[2] } else { 0 };
                }
            }
            Some(nm) => {
                month = nm;
                if self.index == 1 {
                    day = self.comp[0];
                    year = 0;
                } else if !is_day(self.comp[0]) {
                    year = self.comp[0];
                    day = self.comp[1];
                } else {
                    day = self.comp[0];
                    year = self.comp[1];
                }
            }
        }
        if !self.iso {
            if between(year, 0, 49) {
                year += 2000;
            } else if between(year, 50, 99) {
                year += 1900;
            }
        }
        if !between(month, 1, 12) || !is_day(day) {
            return None;
        }
        Some((year, month - 1, day))
    }
}

#[derive(Default)]
struct Time {
    comp: [i64; 4],
    index: usize,
    hour_offset: Option<i64>,
}

impl Time {
    fn is_empty(&self) -> bool {
        self.index == 0
    }
    fn is_expecting(&self, n: i64) -> bool {
        (self.index == 1 && between(n, 0, 59)) || (self.index == 2 && between(n, 0, 59)) || (self.index == 3 && between(n, 0, 999))
    }
    fn add(&mut self, n: i64) -> bool {
        if self.index < 4 {
            self.comp[self.index] = n;
            self.index += 1;
            true
        } else {
            false
        }
    }
    fn add_final(&mut self, n: i64) -> bool {
        if !self.add(n) {
            return false;
        }
        while self.index < 4 {
            self.comp[self.index] = 0;
            self.index += 1;
        }
        true
    }
    fn write(&mut self) -> Option<[i64; 4]> {
        while self.index < 4 {
            self.comp[self.index] = 0;
            self.index += 1;
        }
        let [mut hour, minute, second, ms] = self.comp;
        if let Some(off) = self.hour_offset {
            if !between(hour, 0, 12) {
                return None;
            }
            hour %= 12;
            hour += off;
        }
        let in_range = between(hour, 0, 23) && between(minute, 0, 59) && between(second, 0, 59) && between(ms, 0, 999);
        if !in_range && (hour != 24 || minute != 0 || second != 0 || ms != 0) {
            return None;
        }
        Some([hour, minute, second, ms])
    }
}

#[derive(Default)]
struct Zone {
    sign: Option<i64>,
    hour: Option<i64>,
    minute: Option<i64>,
}

impl Zone {
    fn set(&mut self, hours: i64) {
        self.sign = Some(if hours < 0 { -1 } else { 1 });
        self.hour = Some(hours.abs());
        self.minute = Some(0);
    }
    fn set_sign(&mut self, s: i64) {
        self.sign = Some(if s < 0 { -1 } else { 1 });
    }
    fn is_expecting(&self, n: i64) -> bool {
        self.hour.is_some() && self.minute.is_none() && between(n, 0, 59)
    }
    fn is_utc(&self) -> bool {
        self.hour == Some(0) && self.minute == Some(0)
    }
    fn is_empty(&self) -> bool {
        self.hour.is_none()
    }
    /// `Some(None)` = local time, `Some(Some(seconds))` = an offset, `None` = failure.
    fn write(&mut self) -> Option<Option<i64>> {
        match self.sign {
            Some(sign) => {
                let h = self.hour.unwrap_or(0);
                let m = self.minute.unwrap_or(0);
                let total = h * 3600 + m * 60;
                if total > 1_073_741_823 {
                    return None;
                }
                Some(Some(sign * total))
            }
            None => Some(None),
        }
    }
}

fn read_milliseconds(n: i64, len: usize) -> i64 {
    let mut number = n;
    let mut length = len;
    if length < 3 {
        if length == 1 {
            number *= 100;
        } else if length == 2 {
            number *= 10;
        }
    } else if length > 3 {
        if length > 9 {
            length = 9;
        }
        let mut factor = 1;
        loop {
            factor *= 10;
            length -= 1;
            if length <= 3 {
                break;
            }
        }
        number /= factor;
    }
    number
}

/// `DateParser::ParseES5DateTime`: the next token the legacy parser must handle, `Tk::End` when the whole
/// string was an ES5 date-time, `Tk::Invalid` on failure.
fn parse_es5(sc: &mut Scanner, day: &mut Day, time: &mut Time, tz: &mut Zone) -> Tk {
    if sc.peek().is_sign() {
        let sign_token = sc.next();
        let Some(year) = sc.peek().fixed(6) else { return sign_token };
        sc.next();
        let sign: i64 = if sign_token == Tk::Sym('-') { -1 } else { 1 };
        if sign < 0 && year == 0 {
            return sign_token;
        }
        day.add(sign * year);
    } else if let Some(y) = sc.peek().fixed(4) {
        sc.next();
        day.add(y);
    } else {
        return sc.next();
    }
    if sc.skip_symbol('-') {
        match sc.peek().fixed(2) {
            Some(m) if between(m, 1, 12) => {
                sc.next();
                day.add(m);
            }
            _ => return sc.next(),
        }
        if sc.skip_symbol('-') {
            match sc.peek().fixed(2) {
                Some(d) if between(d, 1, 31) => {
                    sc.next();
                    day.add(d);
                }
                _ => return sc.next(),
            }
        }
    }
    if !matches!(sc.peek(), Tk::Word { kind: Kw::TimeSep, .. }) {
        if sc.peek() != Tk::End {
            return sc.next();
        }
    } else {
        sc.next();
        let Some(h) = sc.peek().fixed(2).filter(|h| between(*h, 0, 24)) else { return Tk::Invalid };
        let hour_is_24 = h == 24;
        sc.next();
        time.add(h);
        if !sc.skip_symbol(':') {
            return Tk::Invalid;
        }
        let Some(mi) = sc.peek().fixed(2).filter(|m| between(*m, 0, 59) && !(hour_is_24 && *m > 0)) else { return Tk::Invalid };
        sc.next();
        time.add(mi);
        if sc.skip_symbol(':') {
            let Some(s) = sc.peek().fixed(2).filter(|s| between(*s, 0, 59) && !(hour_is_24 && *s > 0)) else { return Tk::Invalid };
            sc.next();
            time.add(s);
            if sc.skip_symbol('.') {
                let Tk::Num { n, len } = sc.peek() else { return Tk::Invalid };
                if hour_is_24 && n > 0 {
                    return Tk::Invalid;
                }
                sc.next();
                time.add(read_milliseconds(n, len));
            }
        }
        if sc.peek().is_z() {
            sc.next();
            tz.set(0);
        } else if sc.peek().is_sign() {
            let s = sc.next();
            tz.set_sign(if s == Tk::Sym('+') { 1 } else { -1 });
            if let Some(hm) = sc.peek().fixed(4) {
                sc.next();
                let (hour, min) = (hm / 100, hm % 100);
                if !between(hour, 0, 23) || !between(min, 0, 59) {
                    return Tk::Invalid;
                }
                tz.hour = Some(hour);
                tz.minute = Some(min);
            } else {
                let Some(hour) = sc.peek().fixed(2).filter(|h| between(*h, 0, 23)) else { return Tk::Invalid };
                sc.next();
                tz.hour = Some(hour);
                if !sc.skip_symbol(':') {
                    return Tk::Invalid;
                }
                let Some(min) = sc.peek().fixed(2).filter(|m| between(*m, 0, 59)) else { return Tk::Invalid };
                sc.next();
                tz.minute = Some(min);
            }
        }
        if sc.peek() != Tk::End {
            return Tk::Invalid;
        }
    }
    if tz.is_empty() && time.is_empty() {
        tz.set(0);
    }
    day.iso = true;
    Tk::End
}

/// `Date.parse(s)` of V8, as a time value in the zone-less model (`NaN` when it does not parse).
fn js_date_parse(s: &str) -> f64 {
    js_date_parse_opt(s).unwrap_or(f64::NAN)
}

fn js_date_parse_opt(s: &str) -> Option<f64> {
    let mut sc = Scanner::new(s);
    let (mut day, mut time, mut tz) = (Day::default(), Time::default(), Zone::default());
    let first = parse_es5(&mut sc, &mut day, &mut time, &mut tz);
    if first == Tk::Invalid {
        return None;
    }
    let mut has_read_number = day.index != 0;
    let mut token = first;
    while token != Tk::End {
        match token {
            Tk::Num { n, .. } => {
                has_read_number = true;
                if sc.skip_symbol(':') {
                    if sc.skip_symbol(':') {
                        if !time.is_empty() {
                            return None;
                        }
                        time.add(n);
                        time.add(0);
                    } else {
                        if !time.add(n) {
                            return None;
                        }
                        if sc.peek() == Tk::Sym('.') {
                            sc.next();
                        }
                    }
                } else if sc.skip_symbol('.') && time.is_expecting(n) {
                    time.add(n);
                    let Tk::Num { n: ms, len } = sc.peek() else { return None };
                    sc.next();
                    let ms = read_milliseconds(ms, len);
                    if ms < 0 {
                        return None;
                    }
                    time.add_final(ms);
                } else if tz.is_expecting(n) {
                    tz.minute = Some(n);
                } else if time.is_expecting(n) {
                    time.add_final(n);
                    let p = sc.peek();
                    if !(p == Tk::End || p == Tk::Ws || p.is_z() || p.is_sign()) {
                        return None;
                    }
                } else {
                    if !day.add(n) {
                        return None;
                    }
                    sc.skip_symbol('-');
                }
            }
            Tk::Word { kind, value, .. } => {
                if kind == Kw::AmPm && !time.is_empty() {
                    time.hour_offset = Some(value);
                } else if kind == Kw::Month {
                    day.named_month = Some(value);
                    sc.skip_symbol('-');
                } else if kind == Kw::TzName && has_read_number {
                    tz.set(value);
                } else {
                    if has_read_number {
                        return None;
                    }
                    if sc.peek().is_num() {
                        return None;
                    }
                }
            }
            t if t.is_sign() && (tz.is_utc() || !time.is_empty()) => {
                tz.set_sign(if t == Tk::Sym('-') { -1 } else { 1 });
                let mut n = 0i64;
                let mut length = 0usize;
                if let Tk::Num { n: v, len } = sc.peek() {
                    sc.next();
                    n = v;
                    length = len;
                }
                has_read_number = true;
                if sc.peek() == Tk::Sym(':') {
                    tz.hour = Some(n);
                    tz.minute = None;
                } else if length == 2 || length == 1 {
                    tz.hour = Some(n);
                    tz.minute = Some(0);
                } else if length == 4 || length == 3 {
                    tz.hour = Some(n / 100);
                    tz.minute = Some(n % 100);
                } else {
                    return None;
                }
            }
            t if (t.is_sign() || t == Tk::Sym(')')) && has_read_number => return None,
            _ => {}
        }
        token = sc.next();
    }
    let (y, mo, d) = day.write()?;
    let [h, mi, se, ms] = time.write()?;
    let off = tz.write()?;
    let mut t = jsdate::make_date(jsdate::make_day(y as f64, mo as f64, d as f64), jsdate::make_time(h as f64, mi as f64, se as f64, ms as f64));
    if let Some(o) = off {
        t -= o as f64 * 1000.0;
    }
    let t = jsdate::time_clip(t);
    if t.is_nan() {
        None
    } else {
        Some(t)
    }
}

// ── Time and calendar helpers ────────────────────────────────────────────────

/// `timePartSeconds(serial)`: the fraction of the day as whole seconds (rounded).
fn time_part_seconds(serial: f64) -> f64 {
    let mut frac = serial - serial.floor();
    if frac < 0.0 {
        frac += 1.0;
    }
    js::round(frac * 86400.0)
}

/// `weekdayValue(serial, type)`.
fn weekday_value(serial: f64, ty: f64) -> Result<f64, ErrCode> {
    let Some(d) = jsdate::serial_to_date(serial) else { return Err(ErrCode::VALUE) };
    let js = jsdate::parts(d).map(|p| p.weekday as f64).unwrap_or(f64::NAN);
    let code = if ty.fract() == 0.0 && ty.abs() < 1000.0 { ty as i64 } else { return Err(ErrCode::NUM) };
    Ok(match code {
        1 => js + 1.0,
        2 => {
            if js == 0.0 {
                7.0
            } else {
                js
            }
        }
        3 => {
            if js == 0.0 {
                6.0
            } else {
                js - 1.0
            }
        }
        11 => {
            if js == 0.0 {
                7.0
            } else {
                js
            }
        }
        12 => ((js + 5.0) % 7.0) + 1.0,
        13 => ((js + 4.0) % 7.0) + 1.0,
        14 => ((js + 3.0) % 7.0) + 1.0,
        15 => ((js + 2.0) % 7.0) + 1.0,
        16 => ((js + 1.0) % 7.0) + 1.0,
        17 => js + 1.0,
        _ => return Err(ErrCode::NUM),
    })
}

fn get_date(t: f64) -> f64 {
    jsdate::parts(t).map(|p| p.date as f64).unwrap_or(f64::NAN)
}

fn get_year(t: f64) -> f64 {
    jsdate::parts(t).map(|p| p.year as f64).unwrap_or(f64::NAN)
}

fn get_month(t: f64) -> f64 {
    jsdate::parts(t).map(|p| p.month as f64).unwrap_or(f64::NAN)
}

/// `d.setDate(n)` on a local midnight date: the same year and month, no two-digit-year mapping.
fn set_date(t: f64, n: f64) -> f64 {
    match jsdate::parts(t) {
        Some(p) => jsdate::time_clip(jsdate::make_date(jsdate::make_day(p.year as f64, p.month as f64, n), 0.0)),
        None => f64::NAN,
    }
}

/// `Math.min(a, b)` with NaN propagation.
fn min2(a: f64, b: f64) -> f64 {
    js::min(&[a, b])
}

/// `days360(start, end, european)`.
fn days360(start: f64, end: f64, european: bool) -> Option<f64> {
    let ds = jsdate::serial_to_date(start)?;
    let de = jsdate::serial_to_date(end)?;
    let (ps, pe) = (jsdate::parts(ds)?, jsdate::parts(de)?);
    let mut d1 = ps.date as f64;
    let mut d2 = pe.date as f64;
    let m1 = ps.month as f64 + 1.0;
    let m2 = pe.month as f64 + 1.0;
    let y1 = ps.year as f64;
    let y2 = pe.year as f64;
    if european {
        if d1 == 31.0 {
            d1 = 30.0;
        }
        if d2 == 31.0 {
            d2 = 30.0;
        }
    } else {
        if d1 == 31.0 {
            d1 = 30.0;
        }
        if d2 == 31.0 && d1 == 30.0 {
            d2 = 30.0;
        }
    }
    Some((y2 - y1) * 360.0 + (m2 - m1) * 30.0 + (d2 - d1))
}

/// `addMonths(serial, months)`.
fn add_months(serial: f64, months: f64) -> Option<f64> {
    let d = jsdate::serial_to_date(serial)?;
    let p = jsdate::parts(d)?;
    let y = p.year as f64;
    let m = p.month as f64 + months;
    let day = p.date as f64;
    let target_y = y + (m / 12.0).floor();
    let target_m = ((m % 12.0) + 12.0) % 12.0;
    let last_day = get_date(jsdate::ymd(target_y, target_m + 1.0, 0.0));
    Some(ymd_to_serial(target_y, target_m + 1.0, min2(day, last_day)))
}

/// The `weekend` argument of the `.INTL` functions.
enum Weekend {
    Num(f64),
    Text(String),
}

/// `weekendMask(weekend)`: indexed by `getDay()` (0 = Sunday).
fn weekend_mask(w: &Weekend) -> Option<[bool; 7]> {
    match w {
        Weekend::Text(s) => {
            let b: Vec<char> = s.chars().collect();
            if b.len() != 7 || !b.iter().all(|c| *c == '0' || *c == '1') {
                return None;
            }
            let mon: Vec<bool> = b.iter().map(|c| *c == '1').collect();
            Some([mon[6], mon[0], mon[1], mon[2], mon[3], mon[4], mon[5]])
        }
        Weekend::Num(n) => {
            if n.fract() != 0.0 || n.abs() > 100.0 {
                return None;
            }
            let days: &[usize] = match *n as i64 {
                1 => &[0, 6],
                2 => &[0, 1],
                3 => &[1, 2],
                4 => &[2, 3],
                5 => &[3, 4],
                6 => &[4, 5],
                7 => &[5, 6],
                11 => &[0],
                12 => &[1],
                13 => &[2],
                14 => &[3],
                15 => &[4],
                16 => &[5],
                17 => &[6],
                _ => return None,
            };
            let mut mask = [false; 7];
            for d in days {
                mask[*d] = true;
            }
            Some(mask)
        }
    }
}

/// `collectHolidays(ev, node)`: the integer day numbers of the argument (errors skipped).
fn collect_holidays(ev: &Evaluator, node: Option<&Node>) -> HashSet<u64> {
    let mut set = HashSet::new();
    let Some(node) = node else { return set };
    for s in flatten(&ev.eval(node)) {
        if s.is_err() {
            continue;
        }
        if let Ok(n) = to_num(&s) {
            set.insert(day_key(n.floor()));
        }
    }
    set
}

/// A `Set<number>` key: `-0` and `0` are the same value.
fn day_key(x: f64) -> u64 {
    if x == 0.0 {
        0.0f64.to_bits()
    } else {
        x.to_bits()
    }
}

/// `serialToDate(cur)?.getDay() ?? 0`.
fn dow_of(serial: f64) -> usize {
    jsdate::serial_to_date(serial).and_then(jsdate::parts).map(|p| p.weekday as usize).unwrap_or(0)
}

/// The `weekend` argument of NETWORKDAYS.INTL / WORKDAY.INTL (`1` when absent).
fn weekend_arg(ev: &Evaluator, a: &[Node], i: usize) -> Result<Weekend, ErrCode> {
    match a.get(i) {
        None => Ok(Weekend::Num(1.0)),
        Some(n) => {
            let w = ev.scalar(n);
            if let Some(e) = w.err() {
                return Err(e.clone());
            }
            Ok(match w {
                crate::formula::value::Scalar::Str(s) => Weekend::Text(s),
                other => Weekend::Num(to_num(&other).unwrap_or(f64::NAN)),
            })
        }
    }
}

/// The count loop shared by NETWORKDAYS and NETWORKDAYS.INTL.
fn count_days(start: f64, end: f64, holidays: &HashSet<u64>, is_weekend: impl Fn(usize) -> bool) -> R {
    let mut s = start.floor();
    let mut e = end.floor();
    let sign = if s <= e { 1.0 } else { -1.0 };
    if sign < 0.0 {
        std::mem::swap(&mut s, &mut e);
    }
    let mut count = 0.0;
    let mut cur = s;
    let mut steps: u64 = 0;
    while cur <= e {
        steps += 1;
        if steps > MAX_LOOP {
            return Err(ErrCode::VALUE);
        }
        if !is_weekend(dow_of(cur)) && !holidays.contains(&day_key(cur)) {
            count += 1.0;
        }
        cur += 1.0;
    }
    Ok(Value::num(count * sign))
}

/// The walk shared by WORKDAY and WORKDAY.INTL.
fn walk_days(start: f64, days: f64, holidays: &HashSet<u64>, is_weekend: impl Fn(usize) -> bool) -> R {
    let mut n = js::trunc(days);
    let mut cur = start.floor();
    let step = if n >= 0.0 { 1.0 } else { -1.0 };
    n = n.abs();
    let mut steps: u64 = 0;
    while n > 0.0 {
        cur += step;
        steps += 1;
        if steps > MAX_LOOP {
            return Err(ErrCode::VALUE);
        }
        if is_weekend(dow_of(cur)) || holidays.contains(&day_key(cur)) {
            continue;
        }
        n -= 1.0;
    }
    Ok(Value::num(cur))
}

// ── The functions ────────────────────────────────────────────────────────────

fn date(ev: &Evaluator, a: &[Node]) -> R {
    let y = ev.num(arg(a, 0)?)?;
    let m = ev.num(arg(a, 1)?)?;
    let d = ev.num(arg(a, 2)?)?;
    Ok(Value::num(ymd_to_serial(y, m, d)))
}

fn datevalue(ev: &Evaluator, a: &[Node]) -> R {
    let text = ev.str(arg(a, 0)?);
    match parse_date(&text) {
        Some(s) => Ok(Value::num(s)),
        None => Err(ErrCode::VALUE),
    }
}

fn timevalue(ev: &Evaluator, a: &[Node]) -> R {
    let text = ev.str(arg(a, 0)?);
    let text = js::trim(&text);
    let Some(m) = re_time().and_then(|r| r.captures(text)) else { return Err(ErrCode::VALUE) };
    let mut h = digits(&m[1]);
    let mm = digits(&m[2]);
    let ss = m.get(3).map(|g| digits(g.as_str())).unwrap_or(0.0);
    let ap = m.get(4).map(|g| g.as_str().to_lowercase());
    if ap.as_deref() == Some("pm") && h < 12.0 {
        h += 12.0;
    }
    if ap.as_deref() == Some("am") && h == 12.0 {
        h = 0.0;
    }
    if h > 23.0 || mm > 59.0 || ss > 59.0 {
        return Err(ErrCode::VALUE);
    }
    Ok(Value::num((h * 3600.0 + mm * 60.0 + ss) / 86400.0))
}

fn time(ev: &Evaluator, a: &[Node]) -> R {
    let h = ev.num(arg(a, 0)?)?;
    let m = ev.num(arg(a, 1)?)?;
    let s = ev.num(arg(a, 2)?)?;
    let total = h * 3600.0 + m * 60.0 + s;
    let mut frac = (total / 86400.0) % 1.0;
    if frac < 0.0 {
        frac += 1.0;
    }
    Ok(Value::num(frac))
}

fn hour(ev: &Evaluator, a: &[Node]) -> R {
    let serial = ev.num(arg(a, 0)?)?;
    Ok(Value::num((time_part_seconds(serial) / 3600.0).floor()))
}

fn minute(ev: &Evaluator, a: &[Node]) -> R {
    let serial = ev.num(arg(a, 0)?)?;
    Ok(Value::num(((time_part_seconds(serial) % 3600.0) / 60.0).floor()))
}

fn second(ev: &Evaluator, a: &[Node]) -> R {
    let serial = ev.num(arg(a, 0)?)?;
    Ok(Value::num(time_part_seconds(serial) % 60.0))
}

fn weekday(ev: &Evaluator, a: &[Node]) -> R {
    let serial = ev.num(arg(a, 0)?)?;
    let ty = match a.get(1) {
        Some(n) => ev.num(n)?,
        None => 1.0,
    };
    Ok(Value::num(weekday_value(serial, ty)?))
}

fn weeknum(ev: &Evaluator, a: &[Node]) -> R {
    let serial = ev.num(arg(a, 0)?)?;
    let ty = match a.get(1) {
        Some(n) => ev.num(n)?,
        None => 1.0,
    };
    let Some(d) = jsdate::serial_to_date(serial) else { return Err(ErrCode::VALUE) };
    if ty == 21.0 {
        return isoweeknum(ev, a);
    }
    let week_starts_monday = ty == 2.0;
    let jan1 = jsdate::ymd(get_year(d), 0.0, 1.0);
    let jan1_dow = jsdate::parts(jan1).map(|p| p.weekday as f64).unwrap_or(f64::NAN);
    let offset = if week_starts_monday {
        if jan1_dow == 0.0 {
            6.0
        } else {
            jan1_dow - 1.0
        }
    } else {
        jan1_dow
    };
    let day_of_year = ((d - jan1) / jsdate::DAY_MS).floor() + 1.0;
    Ok(Value::num(((day_of_year + offset - 1.0) / 7.0).floor() + 1.0))
}

fn isoweeknum(ev: &Evaluator, a: &[Node]) -> R {
    let serial = ev.num(arg(a, 0)?)?;
    let Some(d) = jsdate::serial_to_date(serial) else { return Err(ErrCode::VALUE) };
    let target = jsdate::ymd(get_year(d), get_month(d), get_date(d));
    let wd = |t: f64| jsdate::parts(t).map(|p| p.weekday as f64).unwrap_or(f64::NAN);
    let day_nr = (wd(target) + 6.0) % 7.0;
    let target = set_date(target, get_date(target) - day_nr + 3.0);
    let first_thursday = jsdate::ymd(get_year(target), 0.0, 4.0);
    let first_day_nr = (wd(first_thursday) + 6.0) % 7.0;
    let first_thursday = set_date(first_thursday, get_date(first_thursday) - first_day_nr + 3.0);
    Ok(Value::num(1.0 + js::round((target - first_thursday) / (7.0 * jsdate::DAY_MS))))
}

fn edate(ev: &Evaluator, a: &[Node]) -> R {
    let serial = ev.num(arg(a, 0)?)?;
    let months = ev.num(arg(a, 1)?)?;
    match add_months(serial, js::trunc(months)) {
        Some(r) => Ok(Value::num(r)),
        None => Err(ErrCode::VALUE),
    }
}

fn eomonth(ev: &Evaluator, a: &[Node]) -> R {
    let serial = ev.num(arg(a, 0)?)?;
    let months = ev.num(arg(a, 1)?)?;
    let Some(d) = jsdate::serial_to_date(serial) else { return Err(ErrCode::VALUE) };
    let m = get_month(d) + js::trunc(months);
    let target_y = get_year(d) + (m / 12.0).floor();
    let target_m = ((m % 12.0) + 12.0) % 12.0;
    Ok(Value::num(ymd_to_serial(target_y, target_m + 2.0, 0.0)))
}

fn days(ev: &Evaluator, a: &[Node]) -> R {
    let end = ev.num(arg(a, 0)?)?;
    let start = ev.num(arg(a, 1)?)?;
    Ok(Value::num(end.floor() - start.floor()))
}

fn days360_fn(ev: &Evaluator, a: &[Node]) -> R {
    let start = ev.num(arg(a, 0)?)?;
    let end = ev.num(arg(a, 1)?)?;
    let mut european = false;
    if let Some(n) = a.get(2) {
        let meth = ev.scalar(n);
        if let Some(e) = meth.err() {
            return Err(e.clone());
        }
        let mn = to_num(&meth)?;
        european = mn != 0.0;
    }
    match days360(start, end, european) {
        Some(r) => Ok(Value::num(r)),
        None => Err(ErrCode::VALUE),
    }
}

fn datedif(ev: &Evaluator, a: &[Node]) -> R {
    let start = ev.num(arg(a, 0)?)?;
    let end = ev.num(arg(a, 1)?)?;
    let unit = js::upper(&ev.str(arg(a, 2)?));
    let (Some(ds), Some(de)) = (jsdate::serial_to_date(start), jsdate::serial_to_date(end)) else { return Err(ErrCode::VALUE) };
    if end < start {
        return Err(ErrCode::NUM);
    }
    let (Some(ps), Some(pe)) = (jsdate::parts(ds), jsdate::parts(de)) else { return Err(ErrCode::VALUE) };
    let (y1, m1, d1) = (ps.year as f64, ps.month as f64, ps.date as f64);
    let (y2, m2, d2) = (pe.year as f64, pe.month as f64, pe.date as f64);
    let v = match unit.as_str() {
        "D" => end.floor() - start.floor(),
        "Y" => {
            let mut years = y2 - y1;
            if m2 < m1 || (m2 == m1 && d2 < d1) {
                years -= 1.0;
            }
            years
        }
        "M" => {
            let mut months = (y2 - y1) * 12.0 + (m2 - m1);
            if d2 < d1 {
                months -= 1.0;
            }
            months
        }
        "MD" => {
            let mut days = d2 - d1;
            if days < 0.0 {
                days += get_date(jsdate::ymd(y2, m2, 0.0));
            }
            days
        }
        "YM" => {
            let mut months = m2 - m1;
            if d2 < d1 {
                months -= 1.0;
            }
            if months < 0.0 {
                months += 12.0;
            }
            months
        }
        "YD" => {
            let mut anchor = jsdate::ymd(y1, m2, d2);
            if anchor < ds {
                anchor = jsdate::ymd(y1 + 1.0, m2, d2);
            }
            js::round((anchor - ds) / jsdate::DAY_MS)
        }
        _ => return Err(ErrCode::NUM),
    };
    Ok(Value::num(v))
}

fn networkdays(ev: &Evaluator, a: &[Node]) -> R {
    let start = ev.num(arg(a, 0)?)?;
    let end = ev.num(arg(a, 1)?)?;
    let holidays = collect_holidays(ev, a.get(2));
    count_days(start, end, &holidays, |dow| dow == 0 || dow == 6)
}

fn networkdays_intl(ev: &Evaluator, a: &[Node]) -> R {
    let start = ev.num(arg(a, 0)?)?;
    let end = ev.num(arg(a, 1)?)?;
    let weekend = weekend_arg(ev, a, 2)?;
    let Some(mask) = weekend_mask(&weekend) else { return Err(ErrCode::NUM) };
    let holidays = collect_holidays(ev, a.get(3));
    count_days(start, end, &holidays, |dow| mask[dow])
}

fn workday(ev: &Evaluator, a: &[Node]) -> R {
    let start = ev.num(arg(a, 0)?)?;
    let days = ev.num(arg(a, 1)?)?;
    let holidays = collect_holidays(ev, a.get(2));
    walk_days(start, days, &holidays, |dow| dow == 0 || dow == 6)
}

fn workday_intl(ev: &Evaluator, a: &[Node]) -> R {
    let start = ev.num(arg(a, 0)?)?;
    let days = ev.num(arg(a, 1)?)?;
    let weekend = weekend_arg(ev, a, 2)?;
    let Some(mask) = weekend_mask(&weekend) else { return Err(ErrCode::NUM) };
    let holidays = collect_holidays(ev, a.get(3));
    walk_days(start, days, &holidays, |dow| mask[dow])
}

fn is_leap(y: f64) -> bool {
    (y % 4.0 == 0.0 && y % 100.0 != 0.0) || y % 400.0 == 0.0
}

fn yearfrac(ev: &Evaluator, a: &[Node]) -> R {
    let start = ev.num(arg(a, 0)?)?;
    let end = ev.num(arg(a, 1)?)?;
    let basis = match a.get(2) {
        Some(n) => ev.num(n)?,
        None => 0.0,
    };
    let (mut s, mut e) = (start, end);
    if s > e {
        std::mem::swap(&mut s, &mut e);
    }
    let (Some(ds), Some(de)) = (jsdate::serial_to_date(s), jsdate::serial_to_date(e)) else { return Err(ErrCode::VALUE) };
    let v = if basis == 0.0 {
        match days360(s, e, false) {
            Some(d) => d / 360.0,
            None => return Err(ErrCode::VALUE),
        }
    } else if basis == 1.0 {
        let (y1, y2) = (get_year(ds), get_year(de));
        if y1 == y2 {
            let year_days = if is_leap(y1) { 366.0 } else { 365.0 };
            (e - s) / year_days
        } else {
            let mut total = 0.0;
            let mut y = y1;
            while y <= y2 {
                total += if is_leap(y) { 366.0 } else { 365.0 };
                y += 1.0;
            }
            let avg = total / (y2 - y1 + 1.0);
            (e - s) / avg
        }
    } else if basis == 2.0 {
        (e - s) / 360.0
    } else if basis == 3.0 {
        (e - s) / 365.0
    } else if basis == 4.0 {
        match days360(s, e, true) {
            Some(d) => d / 360.0,
            None => return Err(ErrCode::VALUE),
        }
    } else {
        return Err(ErrCode::NUM);
    };
    Ok(Value::num(v))
}

pub fn register(r: &mut Registry) {
    r.insert("DATE", date);
    r.insert("DATEVALUE", datevalue);
    r.insert("TIMEVALUE", timevalue);
    r.insert("TIME", time);
    r.insert("HOUR", hour);
    r.insert("MINUTE", minute);
    r.insert("SECOND", second);
    r.insert("WEEKDAY", weekday);
    r.insert("WEEKNUM", weeknum);
    r.insert("ISOWEEKNUM", isoweeknum);
    r.insert("EDATE", edate);
    r.insert("EOMONTH", eomonth);
    r.insert("DAYS", days);
    r.insert("DAYS360", days360_fn);
    r.insert("DATEDIF", datedif);
    r.insert("NETWORKDAYS", networkdays);
    r.insert("NETWORKDAYS.INTL", networkdays_intl);
    r.insert("WORKDAY", workday);
    r.insert("WORKDAY.INTL", workday_intl);
    r.insert("YEARFRAC", yearfrac);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Option<f64> {
        parse_date(s)
    }

    fn ymd_of(s: &str) -> Option<(i64, i64, i64)> {
        let t = js_date_parse(s);
        jsdate::parts(t).map(|p| (p.year, p.month + 1, p.date))
    }

    #[test]
    fn explicit_formats() {
        assert_eq!(parse("2020-03-05"), Some(43895.0));
        assert_eq!(parse("3/5/2020"), Some(43895.0));
        assert_eq!(parse("3.5.20"), Some(43895.0));
        assert_eq!(parse(""), None);
    }

    #[test]
    fn date_parse_matches_v8() {
        assert_eq!(ymd_of("March 5, 2020"), Some((2020, 3, 5)));
        assert_eq!(ymd_of("5 March 2020"), Some((2020, 3, 5)));
        assert_eq!(ymd_of("Thu, 05 Mar 2020 10:00:00 GMT"), Some((2020, 3, 5)));
        assert_eq!(ymd_of("2020/03/05"), Some((2020, 3, 5)));
        assert_eq!(ymd_of("2020-03"), Some((2020, 3, 1)));
        assert_eq!(ymd_of("2020"), Some((2020, 1, 1)));
        assert_eq!(ymd_of("March 5"), Some((2001, 3, 5)));
        assert_eq!(ymd_of("Mar 5 20"), Some((2020, 3, 5)));
        assert_eq!(ymd_of("Mar 5 99"), Some((1999, 3, 5)));
        assert_eq!(ymd_of("Mar 5 2020 11:59 PM EST"), Some((2020, 3, 6)));
        assert!(js_date_parse("hello").is_nan());
        assert!(js_date_parse("March 32, 2020").is_nan());
        assert!(js_date_parse("2020-13").is_nan() || ymd_of("2020-13").is_none());
    }

    #[test]
    fn serials() {
        assert_eq!(ymd_to_serial(2024.0, 1.0, 1.0), 45292.0);
        assert!(ymd_to_serial(f64::NAN, 1.0, 1.0).is_nan());
    }
}
