//! JavaScript's number and string semantics, exactly.
//!
//! The web editor's engine is TypeScript: a cell typed `" 12 "` is a number because `Number(" 12 ")` is
//! 12, a result is displayed through `String(x)`, a format rounds with `x.toFixed(2)` and `Math.round`
//! rounds halves toward +∞. Rust's own conversions differ on every one of these (`"1e21"` prints as
//! twenty-two digits, `{:.2}` rounds exact ties to even, `f64::round` rounds -2.5 to -3), so the port
//! goes through this module wherever the TypeScript calls one of them. Strings are measured and sliced
//! in UTF-16 code units, as `String.prototype.length` and `slice` count them.

use std::cmp::Ordering;

// ── Whitespace ───────────────────────────────────────────────────────────────

/// `StrWhiteSpaceChar` of ECMA-262: white space and line terminators (U+FEFF included, U+0085 not).
pub fn is_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{9}' | '\u{A}' | '\u{B}' | '\u{C}' | '\u{D}' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'..='\u{200A}'
            | '\u{2028}' | '\u{2029}' | '\u{202F}' | '\u{205F}' | '\u{3000}' | '\u{FEFF}'
    )
}

/// `String.prototype.trim`.
pub fn trim(s: &str) -> &str {
    s.trim_matches(is_whitespace)
}

/// The regular-expression class `\s` (the same set as [`is_whitespace`]).
pub fn is_regex_space(c: char) -> bool {
    is_whitespace(c)
}

// ── Number(string) and parseFloat ───────────────────────────────────────────

/// Length of the longest prefix of `s` that is a `StrUnsignedDecimalLiteral` (digits with an optional
/// fraction and exponent, or `Infinity`), and whether it is `Infinity`.
fn decimal_prefix(s: &[u8]) -> (usize, bool) {
    if s.starts_with(b"Infinity") {
        return (8, true);
    }
    let mut i = 0;
    let mut digits = 0;
    while i < s.len() && s[i].is_ascii_digit() {
        i += 1;
        digits += 1;
    }
    if i < s.len() && s[i] == b'.' {
        let mut j = i + 1;
        let mut frac = 0;
        while j < s.len() && s[j].is_ascii_digit() {
            j += 1;
            frac += 1;
        }
        if digits + frac > 0 {
            i = j;
            digits += frac;
        }
    }
    if digits == 0 {
        return (0, false);
    }
    if i < s.len() && (s[i] == b'e' || s[i] == b'E') {
        let mut j = i + 1;
        if j < s.len() && (s[j] == b'+' || s[j] == b'-') {
            j += 1;
        }
        let start = j;
        while j < s.len() && s[j].is_ascii_digit() {
            j += 1;
        }
        if j > start {
            i = j;
        }
    }
    (i, false)
}

/// Parses an unsigned decimal literal Rust's parser accepts as well (it never sees `inf`/`nan`).
fn parse_decimal(text: &str) -> f64 {
    // Rust rejects a bare trailing point only in some versions; normalise "1." to "1".
    let t = text.strip_suffix('.').unwrap_or(text);
    let t = if let Some(p) = t.find(['e', 'E']) {
        if t[..p].ends_with('.') {
            // "1.e5"
            return format!("{}{}", &t[..p - 1], &t[p..]).parse().unwrap_or(f64::NAN);
        }
        t
    } else {
        t
    };
    t.parse::<f64>().unwrap_or(f64::NAN)
}

/// `Number(s)` for a string: `None` where JavaScript gives `NaN`.
///
/// Whitespace around is ignored, the empty string is 0, `0x`/`0o`/`0b` prefixes read an integer in that
/// base, `Infinity` with an optional sign is accepted, and anything else must be a whole decimal literal.
pub fn number(s: &str) -> Option<f64> {
    let t = trim(s);
    if t.is_empty() {
        return Some(0.0);
    }
    let b = t.as_bytes();
    if b.len() > 2 && b[0] == b'0' {
        let radix = match b[1] {
            b'x' | b'X' => 16,
            b'o' | b'O' => 8,
            b'b' | b'B' => 2,
            _ => 0,
        };
        if radix != 0 {
            let mut v = 0.0f64;
            for &c in &b[2..] {
                let d = (c as char).to_digit(radix)?;
                v = v * radix as f64 + d as f64;
            }
            return Some(v);
        }
    }
    let (sign, rest) = match b[0] {
        b'-' => (-1.0, &t[1..]),
        b'+' => (1.0, &t[1..]),
        _ => (1.0, t),
    };
    let (len, inf) = decimal_prefix(rest.as_bytes());
    if len == 0 || len != rest.len() {
        return None;
    }
    if inf {
        return Some(sign * f64::INFINITY);
    }
    Some(sign * parse_decimal(rest))
}

/// `Number(s)`, with `NaN` for what is not a number.
pub fn number_or_nan(s: &str) -> f64 {
    number(s).unwrap_or(f64::NAN)
}

/// `parseFloat(s)`: the longest decimal prefix after leading whitespace, `NaN` when there is none.
pub fn parse_float(s: &str) -> f64 {
    let t = s.trim_start_matches(is_whitespace);
    let b = t.as_bytes();
    if b.is_empty() {
        return f64::NAN;
    }
    let (sign, off) = match b[0] {
        b'-' => (-1.0, 1),
        b'+' => (1.0, 1),
        _ => (1.0, 0),
    };
    let (len, inf) = decimal_prefix(&b[off..]);
    if len == 0 {
        return f64::NAN;
    }
    if inf {
        return sign * f64::INFINITY;
    }
    sign * parse_decimal(&t[off..off + len])
}

/// `parseInt(s, radix)` (radix 0 = 10, or 16 with a `0x` prefix). `NaN` when no digit is read.
pub fn parse_int(s: &str, radix: u32) -> f64 {
    let t = s.trim_start_matches(is_whitespace);
    let mut b = t.as_bytes();
    let mut sign = 1.0;
    if let Some(&c) = b.first() {
        if c == b'-' || c == b'+' {
            if c == b'-' {
                sign = -1.0;
            }
            b = &b[1..];
        }
    }
    let mut radix = radix;
    if (radix == 0 || radix == 16) && b.len() >= 2 && b[0] == b'0' && (b[1] == b'x' || b[1] == b'X') {
        b = &b[2..];
        radix = 16;
    }
    if radix == 0 {
        radix = 10;
    }
    if !(2..=36).contains(&radix) {
        return f64::NAN;
    }
    let mut v = 0.0f64;
    let mut any = false;
    for &c in b {
        match (c as char).to_digit(radix) {
            Some(d) => {
                v = v * radix as f64 + d as f64;
                any = true;
            }
            None => break,
        }
    }
    if any {
        sign * v
    } else {
        f64::NAN
    }
}

/// `isNaN(Number(s))` negated: whether JavaScript reads the string as a number.
pub fn is_numeric(s: &str) -> bool {
    number(s).is_some()
}

// ── String(number) ───────────────────────────────────────────────────────────

/// The shortest round-trip digits of a finite positive `x` and its decimal exponent `n` (ECMA-262
/// `Number::toString`: x = 0.d₁d₂…dₖ × 10ⁿ).
fn shortest_digits(x: f64) -> (String, i32) {
    // `{:e}` prints the shortest round-trip representation in scientific notation: "1.2345e-7".
    let s = format!("{x:e}");
    let (mant, exp) = s.split_once('e').unwrap_or((s.as_str(), "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let digits: String = mant.chars().filter(|c| c.is_ascii_digit()).collect();
    let digits = digits.trim_end_matches('0');
    let digits = if digits.is_empty() { "0".to_string() } else { digits.to_string() };
    (digits, exp + 1)
}

/// `String(x)` / `x.toString()`.
pub fn to_string(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x == 0.0 {
        return "0".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    let neg = x < 0.0;
    let (d, n) = shortest_digits(x.abs());
    let k = d.len() as i32;
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    if k <= n && n <= 21 {
        out.push_str(&d);
        for _ in 0..(n - k) {
            out.push('0');
        }
    } else if 0 < n && n <= 21 {
        out.push_str(&d[..n as usize]);
        out.push('.');
        out.push_str(&d[n as usize..]);
    } else if -6 < n && n <= 0 {
        out.push_str("0.");
        for _ in 0..(-n) {
            out.push('0');
        }
        out.push_str(&d);
    } else {
        let e = n - 1;
        out.push_str(&d[..1]);
        if k > 1 {
            out.push('.');
            out.push_str(&d[1..]);
        }
        out.push('e');
        out.push(if e >= 0 { '+' } else { '-' });
        out.push_str(&e.abs().to_string());
    }
    out
}

// ── Exact decimal arithmetic for toFixed / toPrecision ──────────────────────

/// A little-endian base-2³² unsigned integer: just what exact rounding of a double needs.
#[derive(Clone, Debug)]
struct Big(Vec<u32>);

impl Big {
    fn from_u64(v: u64) -> Big {
        let mut b = Big(vec![v as u32, (v >> 32) as u32]);
        b.trim();
        b
    }
    fn trim(&mut self) {
        while self.0.last() == Some(&0) {
            self.0.pop();
        }
    }
    fn mul_small(&mut self, m: u32) {
        let mut carry = 0u64;
        for limb in &mut self.0 {
            let v = *limb as u64 * m as u64 + carry;
            *limb = v as u32;
            carry = v >> 32;
        }
        if carry > 0 {
            self.0.push(carry as u32);
        }
    }
    fn mul_pow10(&mut self, p: u32) {
        for _ in 0..p {
            self.mul_small(10);
        }
    }
    fn shl(&mut self, k: u32) {
        let words = (k / 32) as usize;
        let bits = k % 32;
        if bits > 0 {
            let mut carry = 0u32;
            for limb in &mut self.0 {
                let v = (*limb as u64) << bits;
                *limb = v as u32 | carry;
                carry = (v >> 32) as u32;
            }
            if carry > 0 {
                self.0.push(carry);
            }
        }
        if words > 0 {
            let mut v = vec![0u32; words];
            v.append(&mut self.0);
            self.0 = v;
        }
    }
    fn bit(&self, k: u32) -> bool {
        let w = (k / 32) as usize;
        w < self.0.len() && (self.0[w] >> (k % 32)) & 1 == 1
    }
    fn shr(&mut self, k: u32) {
        let words = (k / 32) as usize;
        if words >= self.0.len() {
            self.0.clear();
            return;
        }
        self.0.drain(..words);
        let bits = k % 32;
        if bits > 0 {
            let n = self.0.len();
            for i in 0..n {
                let hi = if i + 1 < n { self.0[i + 1] } else { 0 };
                self.0[i] = (self.0[i] >> bits) | (hi << (32 - bits));
            }
        }
        self.trim();
    }
    fn add_one(&mut self) {
        for limb in &mut self.0 {
            let (v, o) = limb.overflowing_add(1);
            *limb = v;
            if !o {
                return;
            }
        }
        self.0.push(1);
    }
    fn is_zero(&self) -> bool {
        self.0.iter().all(|&l| l == 0)
    }
    /// Decimal digits (no leading zeros; "0" for zero).
    fn to_decimal(&self) -> String {
        if self.is_zero() {
            return "0".into();
        }
        let mut n = self.0.clone();
        let mut chunks = Vec::new();
        while n.iter().any(|&l| l != 0) {
            let mut rem = 0u64;
            for limb in n.iter_mut().rev() {
                let v = (rem << 32) | *limb as u64;
                *limb = (v / 1_000_000_000) as u32;
                rem = v % 1_000_000_000;
            }
            chunks.push(rem as u32);
            while n.last() == Some(&0) {
                n.pop();
            }
        }
        let mut s = chunks.last().map(|c| c.to_string()).unwrap_or_default();
        for c in chunks.iter().rev().skip(1) {
            s.push_str(&format!("{c:09}"));
        }
        s
    }
}

/// `x` as `m × 2^e` with `m` an integer (finite, non-negative `x`).
fn decompose(x: f64) -> (u64, i32) {
    let bits = x.to_bits();
    let exp = ((bits >> 52) & 0x7ff) as i32;
    let frac = bits & ((1u64 << 52) - 1);
    if exp == 0 {
        (frac, -1074)
    } else {
        (frac | (1u64 << 52), exp - 1075)
    }
}

/// round(x × 10^f) with ties away from zero, exactly, as decimal digits (x finite, ≥ 0).
fn scaled_round_half_up(x: f64, f: u32) -> String {
    let (m, e) = decompose(x);
    let mut n = Big::from_u64(m);
    n.mul_pow10(f);
    if e >= 0 {
        n.shl(e as u32);
        return n.to_decimal();
    }
    let k = (-e) as u32;
    let up = n.bit(k - 1);
    n.shr(k);
    if up {
        n.add_one();
    }
    n.to_decimal()
}

/// `x.toFixed(digits)` (0 ≤ digits ≤ 100): exact, halves rounded up in magnitude.
pub fn to_fixed(x: f64, digits: u32) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.abs() >= 1e21 || x.is_infinite() {
        return to_string(x);
    }
    let neg = x < 0.0;
    let s = scaled_round_half_up(x.abs(), digits);
    let mut body = if digits == 0 {
        s
    } else {
        let s = if s.len() <= digits as usize { format!("{}{}", "0".repeat(digits as usize + 1 - s.len()), s) } else { s };
        let p = s.len() - digits as usize;
        format!("{}.{}", &s[..p], &s[p..])
    };
    if neg {
        body.insert(0, '-');
    }
    body
}

/// `x.toPrecision(p)` (1 ≤ p ≤ 100).
pub fn to_precision(x: f64, p: u32) -> String {
    if !x.is_finite() {
        return to_string(x);
    }
    let neg = x < 0.0;
    let a = x.abs();
    let (digits, e) = to_precision_digits(a, p);
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    if e < -6 || e >= p as i32 {
        out.push_str(&digits[..1]);
        if p > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        out.push('e');
        out.push(if e >= 0 { '+' } else { '-' });
        out.push_str(&e.abs().to_string());
    } else if e == p as i32 - 1 {
        out.push_str(&digits);
    } else if e >= 0 {
        out.push_str(&digits[..e as usize + 1]);
        out.push('.');
        out.push_str(&digits[e as usize + 1..]);
    } else {
        out.push_str("0.");
        out.push_str(&"0".repeat((-(e + 1)) as usize));
        out.push_str(&digits);
    }
    out
}

/// round(a / 10^k), halves up (a finite, ≥ 0).
///
/// Only the integer part of `a` decides: with I = ⌊a⌋ and a fraction φ < 1, the quotient's dropped
/// part I mod 10^k + φ reaches half of 10^k exactly when I mod 10^k does.
fn round_div_pow10(a: f64, k: u32) -> String {
    let (m, e) = decompose(a);
    let mut n = Big::from_u64(m);
    if e >= 0 {
        n.shl(e as u32);
    } else {
        n.shr((-e) as u32);
    }
    let s = n.to_decimal();
    if s.len() <= k as usize {
        let first = if s.len() == k as usize { s.as_bytes()[0] } else { b'0' };
        return if first >= b'5' { "1".into() } else { "0".into() };
    }
    let (head, tail) = s.split_at(s.len() - k as usize);
    let mut digits: Vec<u8> = head.bytes().collect();
    if tail.as_bytes()[0] >= b'5' {
        let mut i = digits.len();
        loop {
            if i == 0 {
                digits.insert(0, b'1');
                break;
            }
            i -= 1;
            if digits[i] == b'9' {
                digits[i] = b'0';
            } else {
                digits[i] += 1;
                break;
            }
        }
    }
    String::from_utf8(digits).unwrap_or_default()
}

/// `x.toExponential(f)`.
pub fn to_exponential(x: f64, f: u32) -> String {
    if !x.is_finite() {
        return to_string(x);
    }
    let p = to_precision_digits(x.abs(), f + 1);
    let (digits, e) = p;
    let mut out = String::new();
    if x < 0.0 {
        out.push('-');
    }
    out.push_str(&digits[..1]);
    if f > 0 {
        out.push('.');
        out.push_str(&digits[1..]);
    }
    out.push('e');
    out.push(if e >= 0 { '+' } else { '-' });
    out.push_str(&e.abs().to_string());
    out
}

/// The `p` significant digits of `a` (≥ 0) and its exponent, rounded half up.
fn to_precision_digits(a: f64, p: u32) -> (String, i32) {
    if a == 0.0 {
        return ("0".repeat(p as usize), 0);
    }
    let mut e = a.log10().floor() as i32;
    loop {
        let shift = p as i32 - 1 - e;
        let digits = if shift >= 0 { scaled_round_half_up(a, shift as u32) } else { round_div_pow10(a, (-shift) as u32) };
        if digits.len() > p as usize {
            e += 1;
            continue;
        }
        if digits.len() < p as usize {
            e -= 1;
            continue;
        }
        return (digits, e);
    }
}

// ── Math ────────────────────────────────────────────────────────────────────

/// `Math.round`: the nearest integer, halves toward +∞.
pub fn round(x: f64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let f = x.floor();
    let r = if x - f >= 0.5 { f + 1.0 } else { f };
    if r == 0.0 && x < 0.0 {
        -0.0
    } else {
        r
    }
}

/// `Math.trunc`.
pub fn trunc(x: f64) -> f64 {
    x.trunc()
}

/// `Math.sign`.
pub fn sign(x: f64) -> f64 {
    if x.is_nan() {
        f64::NAN
    } else if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        x
    }
}

/// `a % b` (JavaScript's remainder has the sign of the dividend, like Rust's).
pub fn rem(a: f64, b: f64) -> f64 {
    a % b
}

/// `Math.pow` (JavaScript's `NaN` cases: `1 ** ±∞` and `(-1) ** ±∞` are NaN, unlike `powf`).
pub fn pow(a: f64, b: f64) -> f64 {
    if b.is_infinite() && a.abs() == 1.0 {
        return f64::NAN;
    }
    if b.is_nan() {
        return f64::NAN;
    }
    a.powf(b)
}

/// `Math.max` over a list (`-Infinity` when empty, `NaN` if any is NaN).
pub fn max(values: &[f64]) -> f64 {
    let mut m = f64::NEG_INFINITY;
    for &v in values {
        if v.is_nan() {
            return f64::NAN;
        }
        if v > m || (v == 0.0 && m == 0.0 && m.is_sign_negative()) {
            m = v;
        }
    }
    m
}

/// `Math.min` over a list (`Infinity` when empty, `NaN` if any is NaN).
pub fn min(values: &[f64]) -> f64 {
    let mut m = f64::INFINITY;
    for &v in values {
        if v.is_nan() {
            return f64::NAN;
        }
        if v < m || (v == 0.0 && m == 0.0 && v.is_sign_negative()) {
            m = v;
        }
    }
    m
}

/// `Array.prototype.sort((a, b) => a - b)` for numbers (stable; NaN never occurs in practice).
pub fn sort_numbers(v: &mut [f64]) {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
}

// ── Strings in UTF-16 code units ─────────────────────────────────────────────

/// `s.length`.
pub fn len16(s: &str) -> usize {
    s.encode_utf16().count()
}

/// The string from UTF-16 units (lone surrogates become U+FFFD).
pub fn from_utf16(units: &[u16]) -> String {
    String::from_utf16_lossy(units)
}

/// `s.slice(start, end)` with JavaScript's index rules (negative counts from the end, clamped).
pub fn slice(s: &str, start: f64, end: Option<f64>) -> String {
    let u: Vec<u16> = s.encode_utf16().collect();
    let len = u.len() as f64;
    let norm = |i: f64| -> usize {
        let i = if i.is_nan() { 0.0 } else { i.trunc() };
        let i = if i < 0.0 { (len + i).max(0.0) } else { i.min(len) };
        i as usize
    };
    let a = norm(start);
    let b = end.map(norm).unwrap_or(u.len());
    if a >= b {
        String::new()
    } else {
        from_utf16(&u[a..b])
    }
}

/// `s.substring(start, end)`: negative or NaN indexes are 0, and the two are swapped when reversed.
pub fn substring(s: &str, start: f64, end: Option<f64>) -> String {
    let u: Vec<u16> = s.encode_utf16().collect();
    let len = u.len() as f64;
    let clamp = |i: f64| -> usize { (if i.is_nan() { 0.0 } else { i.trunc() }).clamp(0.0, len) as usize };
    let a = clamp(start);
    let b = end.map(clamp).unwrap_or(u.len());
    let (a, b) = if a > b { (b, a) } else { (a, b) };
    from_utf16(&u[a..b])
}

/// `s.indexOf(needle, from)` in UTF-16 units, or -1.
pub fn index_of(s: &str, needle: &str, from: usize) -> i64 {
    let h: Vec<u16> = s.encode_utf16().collect();
    let n: Vec<u16> = needle.encode_utf16().collect();
    let from = from.min(h.len());
    if n.is_empty() {
        return from as i64;
    }
    if n.len() > h.len() {
        return -1;
    }
    (from..=h.len() - n.len()).find(|&i| h[i..i + n.len()] == n[..]).map(|i| i as i64).unwrap_or(-1)
}

/// `s.lastIndexOf(needle)` in UTF-16 units, or -1.
pub fn last_index_of(s: &str, needle: &str) -> i64 {
    let h: Vec<u16> = s.encode_utf16().collect();
    let n: Vec<u16> = needle.encode_utf16().collect();
    if n.len() > h.len() {
        return -1;
    }
    (0..=h.len() - n.len()).rev().find(|&i| h[i..i + n.len()] == n[..]).map(|i| i as i64).unwrap_or(-1)
}

/// `s.charCodeAt(i)`.
pub fn char_code_at(s: &str, i: usize) -> Option<u16> {
    s.encode_utf16().nth(i)
}

/// `s.padStart(n, fill)`.
pub fn pad_start(s: &str, n: usize, fill: char) -> String {
    let l = len16(s);
    if l >= n {
        return s.to_string();
    }
    let mut out: String = std::iter::repeat_n(fill, n - l).collect();
    out.push_str(s);
    out
}

/// `s.repeat(n)`.
pub fn repeat(s: &str, n: usize) -> String {
    s.repeat(n)
}

/// `a < b` for two strings: UTF-16 code unit order.
pub fn cmp_str(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// `s.toLowerCase()`.
pub fn lower(s: &str) -> String {
    s.to_lowercase()
}

/// `s.toUpperCase()`.
pub fn upper(s: &str) -> String {
    s.to_uppercase()
}

/// `String.fromCharCode(code)`.
pub fn from_char_code(code: u32) -> String {
    from_utf16(&[(code & 0xffff) as u16])
}

/// `\w` of a JavaScript regular expression without the `u` flag: ASCII letters, digits and `_`.
pub fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_follows_javascript() {
        assert_eq!(number(""), Some(0.0));
        assert_eq!(number("  12 "), Some(12.0));
        assert_eq!(number("1e3"), Some(1000.0));
        assert_eq!(number(".5"), Some(0.5));
        assert_eq!(number("5."), Some(5.0));
        assert_eq!(number("0x1F"), Some(31.0));
        assert_eq!(number("-Infinity"), Some(f64::NEG_INFINITY));
        assert_eq!(number("12px"), None);
        assert_eq!(number("."), None);
        assert_eq!(number("inf"), None);
        assert_eq!(number("1,5"), None);
        assert_eq!(number("\u{a0}7\u{feff}"), Some(7.0));
        assert_eq!(number("-0x10"), None);
        assert_eq!(number("1e"), None);
    }

    #[test]
    fn parse_float_reads_a_prefix() {
        assert_eq!(parse_float("1.2.3"), 1.2);
        assert_eq!(parse_float("  3abc"), 3.0);
        assert!(parse_float("abc").is_nan());
        assert_eq!(parse_float("1e"), 1.0);
        assert_eq!(parse_float(".5e-1x"), 0.05);
    }

    #[test]
    fn to_string_follows_javascript() {
        assert_eq!(to_string(1.0), "1");
        assert_eq!(to_string(-0.0), "0");
        assert_eq!(to_string(0.1 + 0.2), "0.30000000000000004");
        assert_eq!(to_string(1e21), "1e+21");
        assert_eq!(to_string(1e20), "100000000000000000000");
        assert_eq!(to_string(1.5e-7), "1.5e-7");
        assert_eq!(to_string(0.000001), "0.000001");
        assert_eq!(to_string(123.456), "123.456");
        assert_eq!(to_string(-2.5e-10), "-2.5e-10");
        assert_eq!(to_string(f64::NAN), "NaN");
    }

    #[test]
    fn to_fixed_rounds_halves_up_exactly() {
        assert_eq!(to_fixed(0.125, 2), "0.13");
        assert_eq!(to_fixed(1.005, 2), "1.00"); // 1.005 is 1.00499999… as a double
        assert_eq!(to_fixed(2.5, 0), "3");
        assert_eq!(to_fixed(-2.5, 0), "-3");
        assert_eq!(to_fixed(-0.001, 2), "-0.00");
        assert_eq!(to_fixed(1234.5678, 1), "1234.6");
        assert_eq!(to_fixed(0.0, 3), "0.000");
        assert_eq!(to_fixed(1e21, 2), "1e+21");
        assert_eq!(to_fixed(123.0, 0), "123");
    }

    #[test]
    fn to_precision_and_exponential() {
        assert_eq!(to_precision(123.456, 4), "123.5");
        assert_eq!(to_precision(0.000123, 2), "0.00012");
        assert_eq!(to_precision(123456.0, 2), "1.2e+5");
        assert_eq!(to_precision(1.0, 3), "1.00");
        assert_eq!(to_exponential(12345.0, 2), "1.23e+4");
        assert_eq!(to_exponential(0.00015, 1), "1.5e-4");
    }

    #[test]
    fn math_round_is_half_up() {
        assert_eq!(round(2.5), 3.0);
        assert_eq!(round(-2.5), -2.0);
        assert_eq!(round(0.49999999999999994), 0.0);
        assert!(round(-0.4).is_sign_negative());
    }

    #[test]
    fn strings_count_utf16_units() {
        assert_eq!(len16("a😀"), 3);
        assert_eq!(slice("abcdef", -2.0, None), "ef");
        assert_eq!(slice("abcdef", 1.0, Some(3.0)), "bc");
        assert_eq!(substring("abcdef", 4.0, Some(1.0)), "bcd");
        assert_eq!(index_of("abcabc", "c", 3), 5);
        assert_eq!(cmp_str("B", "a"), Ordering::Less);
    }
}
