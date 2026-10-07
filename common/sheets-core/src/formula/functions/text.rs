//! Port of the web's `formula-fns/text.ts`: the Excel/Sheets text family beyond the engine's own
//! LEN/LEFT/RIGHT/MID/… block. Strings are measured and sliced in UTF-16 code units, as on the web.

use std::collections::HashSet;

use super::base::{index, num_or};
use super::Registry;
use crate::formula::ast::Node;
use crate::formula::eval::{arg, Evaluator};
use crate::formula::value::{flatten, to_num, to_str, ErrCode, Scalar, Value};
use crate::js;

type R = Result<Value, ErrCode>;

// ── Small local helpers ──────────────────────────────────────────────────────

fn u16s(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// `groupThousands`: `intStr.replace(/\B(?=(\d{3})+(?!\d))/g, sep)` on the digits after an optional `-`.
fn group_thousands(int_str: &str, sep: &str) -> String {
    let (neg, digits) = match int_str.strip_prefix('-') {
        Some(d) => (true, d),
        None => (false, int_str),
    };
    let ch: Vec<char> = digits.chars().collect();
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    for i in 0..=ch.len() {
        let prev_word = i > 0 && js::is_word(ch[i - 1]);
        let next_word = i < ch.len() && js::is_word(ch[i]);
        if prev_word == next_word {
            // `(?=(\d{3})+(?!\d))`: the run of ASCII digits starting here has a positive multiple of 3 length.
            let k = ch[i..].iter().take_while(|c| c.is_ascii_digit()).count();
            if k > 0 && k % 3 == 0 {
                out.push_str(sep);
            }
        }
        if i < ch.len() {
            out.push(ch[i]);
        }
    }
    out
}

/// `x.toFixed(d)` for an already truncated `d`: a `RangeError` (caught as `#VALUE!`) outside 0..=100.
fn to_fixed_checked(x: f64, d: f64) -> Result<String, ErrCode> {
    if !(0.0..=100.0).contains(&d) {
        return Err(ErrCode::VALUE);
    }
    Ok(js::to_fixed(x, d as u32))
}

/// `const [int, frac] = s.split('.')` then `groupThousands(int) + (frac ? '.' + frac : '')`.
fn group_fixed(s: &str) -> String {
    let mut parts = s.split('.');
    let int = parts.next().unwrap_or("");
    let frac = parts.next().unwrap_or("");
    group_thousands(int, ",") + if frac.is_empty() { "" } else { "." } + frac
}

/// A regular-expression `.` without the `s` flag: anything but a line terminator.
fn is_line_terminator(u: u16) -> bool {
    matches!(u, 0x0A | 0x0D | 0x2028 | 0x2029)
}

/// The `Canonicalize` step of a case-insensitive JavaScript regular expression (no `u` flag).
fn canon(u: u16) -> u16 {
    let Some(c) = char::from_u32(u as u32) else { return u };
    let mut up = c.to_uppercase();
    let (Some(f), None) = (up.next(), up.next()) else { return u };
    let fu = f as u32;
    if fu > 0xFFFF || (u >= 128 && fu < 128) {
        return u;
    }
    fu as u16
}

enum Item {
    Lit(u16),
    Any,
    Star,
}

/// `wildcardToRegExp` as a matcher over UTF-16 units (`~` escapes the next unit, `*` → `.*`, `?` → `.`).
fn wildcard_items(pattern: &[u16]) -> Vec<Item> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < pattern.len() {
        let c = pattern[i];
        if c == b'~' as u16 && i + 1 < pattern.len() {
            out.push(Item::Lit(canon(pattern[i + 1])));
            i += 1;
        } else if c == b'*' as u16 {
            out.push(Item::Star);
        } else if c == b'?' as u16 {
            out.push(Item::Any);
        } else {
            out.push(Item::Lit(canon(c)));
        }
        i += 1;
    }
    out
}

fn match_here(items: &[Item], hay: &[u16], i: usize, k: usize, failed: &mut HashSet<(usize, usize)>) -> bool {
    if k == items.len() {
        return true;
    }
    if failed.contains(&(i, k)) {
        return false;
    }
    let ok = match &items[k] {
        Item::Lit(c) => i < hay.len() && canon(hay[i]) == *c && match_here(items, hay, i + 1, k + 1, failed),
        Item::Any => i < hay.len() && !is_line_terminator(hay[i]) && match_here(items, hay, i + 1, k + 1, failed),
        Item::Star => {
            let mut j = i;
            loop {
                if match_here(items, hay, j, k + 1, failed) {
                    break true;
                }
                if j < hay.len() && !is_line_terminator(hay[j]) {
                    j += 1;
                } else {
                    break false;
                }
            }
        }
    };
    if !ok {
        failed.insert((i, k));
    }
    ok
}

/// `s.indexOf(needle, from)` over UTF-16 units.
fn find_units(h: &[u16], n: &[u16], from: usize) -> Option<usize> {
    let from = from.min(h.len());
    if n.is_empty() {
        return Some(from);
    }
    if n.len() > h.len() {
        return None;
    }
    (from..=h.len() - n.len()).find(|&i| h[i..i + n.len()] == n[..])
}

fn unit_ws(u: u16) -> bool {
    char::from_u32(u as u32).is_some_and(js::is_whitespace)
}

// ── Functions ────────────────────────────────────────────────────────────────

fn char_fn(ev: &Evaluator, a: &[Node]) -> R {
    let n = ev.num(arg(a, 0)?)?;
    let code = js::trunc(n);
    if code < 1.0 || code > 255.0 {
        return Err(ErrCode::VALUE);
    }
    Ok(Value::str(js::from_char_code(code as u32)))
}

fn clean(ev: &Evaluator, a: &[Node]) -> R {
    let s = ev.str(arg(a, 0)?);
    Ok(Value::str(s.chars().filter(|&c| !(c <= '\u{1f}' || c == '\u{7f}')).collect::<String>()))
}

fn code(ev: &Evaluator, a: &[Node]) -> R {
    let s = ev.str(arg(a, 0)?);
    match js::char_code_at(&s, 0) {
        Some(u) => Ok(Value::num(u as f64)),
        None => Err(ErrCode::VALUE),
    }
}

fn dollar(ev: &Evaluator, a: &[Node]) -> R {
    let x = ev.num(arg(a, 0)?)?;
    let dec = num_or(ev, a, 1, 2.0)?;
    let d = js::trunc(dec);
    let body = if d >= 0.0 {
        let fixed = to_fixed_checked(x.abs(), d)?;
        "$".to_string() + &group_fixed(&fixed)
    } else {
        // Negative decimals: round to the left of the decimal point.
        let f = js::pow(10.0, -d);
        let rounded = js::round(x.abs() / f) * f;
        "$".to_string() + &group_thousands(&js::to_string(rounded), ",")
    };
    Ok(Value::str(if x < 0.0 { format!("({body})") } else { body }))
}

fn exact(ev: &Evaluator, a: &[Node]) -> R {
    let x = ev.str(arg(a, 0)?);
    let y = ev.str(arg(a, 1)?);
    Ok(Value::bool(x == y))
}

/// The 0-based start of FIND/SEARCH: `Math.trunc(start) - 1`, `#VALUE!` outside the text.
fn search_from(ev: &Evaluator, a: &[Node], within_len: usize) -> Result<usize, ErrCode> {
    let start = num_or(ev, a, 2, 1.0)?;
    let from = js::trunc(start) - 1.0;
    if from < 0.0 || from > within_len as f64 {
        return Err(ErrCode::VALUE);
    }
    // NaN passes both tests and `indexOf` reads it as 0.
    Ok(if from.is_nan() { 0 } else { from as usize })
}

fn find(ev: &Evaluator, a: &[Node]) -> R {
    let find = ev.str(arg(a, 0)?);
    let within = ev.str(arg(a, 1)?);
    let (hu, nu) = (u16s(&within), u16s(&find));
    let from = search_from(ev, a, hu.len())?;
    match find_units(&hu, &nu, from) {
        Some(i) => Ok(Value::num(i as f64 + 1.0)),
        None => Err(ErrCode::VALUE),
    }
}

fn search(ev: &Evaluator, a: &[Node]) -> R {
    let find = ev.str(arg(a, 0)?);
    let within = ev.str(arg(a, 1)?);
    let hu = u16s(&within);
    let from = search_from(ev, a, hu.len())?;
    if find.contains(['*', '?', '~']) {
        let items = wildcard_items(&u16s(&find));
        let rest = &hu[from..];
        let mut failed = HashSet::new();
        for start in 0..=rest.len() {
            if match_here(&items, rest, start, 0, &mut failed) {
                return Ok(Value::num((from + start) as f64 + 1.0));
            }
        }
        return Err(ErrCode::VALUE);
    }
    let (hl, nl) = (u16s(&js::lower(&within)), u16s(&js::lower(&find)));
    match find_units(&hl, &nl, from) {
        Some(i) => Ok(Value::num(i as f64 + 1.0)),
        None => Err(ErrCode::VALUE),
    }
}

fn fixed(ev: &Evaluator, a: &[Node]) -> R {
    let x = ev.num(arg(a, 0)?)?;
    let dec = num_or(ev, a, 1, 2.0)?;
    let no_commas = match a.get(2) {
        Some(n) => to_num(&ev.scalar(n))?,
        None => 0.0,
    };
    let d = js::trunc(dec);
    let s = if d >= 0.0 {
        to_fixed_checked(x, d)?
    } else {
        let f = js::pow(10.0, -d);
        js::to_string(js::round(x / f) * f)
    };
    // `!noCommas`: true for 0 and NaN.
    if no_commas == 0.0 || no_commas.is_nan() {
        return Ok(Value::str(group_fixed(&s)));
    }
    Ok(Value::str(s))
}

fn rept(ev: &Evaluator, a: &[Node]) -> R {
    let s = ev.str(arg(a, 0)?);
    let n = ev.num(arg(a, 1)?)?;
    let count = js::trunc(n);
    if count < 0.0 {
        return Err(ErrCode::VALUE);
    }
    // `String.prototype.repeat` throws a RangeError for an infinite count or a result over V8's limit.
    if count == f64::INFINITY {
        return Err(ErrCode::VALUE);
    }
    if count.is_nan() || s.is_empty() {
        return Ok(Value::str(""));
    }
    if count * js::len16(&s) as f64 > 536_870_888.0 {
        return Err(ErrCode::VALUE);
    }
    Ok(Value::str(s.repeat(count as usize)))
}

fn t_fn(ev: &Evaluator, a: &[Node]) -> R {
    match ev.scalar(arg(a, 0)?) {
        Scalar::Err(e) => Err(e),
        Scalar::Str(s) => Ok(Value::str(s)),
        _ => Ok(Value::str("")),
    }
}

fn unichar(ev: &Evaluator, a: &[Node]) -> R {
    let n = ev.num(arg(a, 0)?)?;
    let code = js::trunc(n);
    if code < 1.0 {
        return Err(ErrCode::VALUE);
    }
    // `String.fromCodePoint` throws for NaN and anything above U+10FFFF.
    if code.is_nan() || code > 1_114_111.0 {
        return Err(ErrCode::VALUE);
    }
    // QUIRK: a lone surrogate (U+D800..U+DFFF) is a valid JS string but not a Rust one: U+FFFD stands in.
    Ok(Value::str(char::from_u32(code as u32).unwrap_or('\u{FFFD}').to_string()))
}

fn unicode(ev: &Evaluator, a: &[Node]) -> R {
    let s = ev.str(arg(a, 0)?);
    match s.chars().next() {
        Some(c) => Ok(Value::num(c as u32 as f64)),
        None => Err(ErrCode::VALUE),
    }
}

fn numbervalue(ev: &Evaluator, a: &[Node]) -> R {
    let s = ev.str(arg(a, 0)?);
    // `charAt(0) || def`: the first UTF-16 unit, or the default when the text is empty.
    let first_unit = |n: &Node, def: char| -> u16 { u16s(&ev.str(n)).first().copied().unwrap_or(def as u16) };
    let dec_sep = match a.get(1) {
        Some(n) => first_unit(n, '.'),
        None => b'.' as u16,
    };
    let grp_sep = match a.get(2) {
        Some(n) => first_unit(n, ','),
        None => b',' as u16,
    };
    if js::trim(&s).is_empty() {
        return Ok(Value::num(0.0));
    }
    let mut u: Vec<u16> = u16s(&s).into_iter().filter(|&x| !unit_ws(x)).collect();
    // Trailing percent signs, each dividing by 100.
    let mut pct = 0.0;
    while u.last() == Some(&(b'%' as u16)) {
        pct += 1.0;
        u.pop();
    }
    u.retain(|&x| x != grp_sep);
    for x in u.iter_mut() {
        if *x == dec_sep {
            *x = b'.' as u16;
        }
    }
    match js::number(&js::from_utf16(&u)) {
        Some(n) => Ok(Value::num(n / js::pow(100.0, pct))),
        None => Err(ErrCode::VALUE),
    }
}

fn text_before_after(ev: &Evaluator, a: &[Node], before: bool) -> R {
    let text = ev.str(arg(a, 0)?);
    let delim = ev.str(arg(a, 1)?);
    let inst = match a.get(2) {
        Some(n) => ev.num(n)?,
        None => 1.0,
    };
    let instance = js::trunc(inst);
    if delim.is_empty() || instance == 0.0 {
        return Err(ErrCode::VALUE);
    }
    let (tu, du) = (u16s(&text), u16s(&delim));
    // Every (non-overlapping) delimiter position.
    let mut positions: Vec<usize> = Vec::new();
    let mut idx = find_units(&tu, &du, 0);
    while let Some(i) = idx {
        positions.push(i);
        idx = find_units(&tu, &du, i + du.len());
    }
    if positions.is_empty() {
        return Err(ErrCode::NA);
    }
    // A negative instance counts from the end.
    let at = if instance > 0.0 { instance - 1.0 } else { positions.len() as f64 + instance };
    let Some(pos) = index(at).and_then(|i| positions.get(i)).copied() else {
        return Err(ErrCode::NA);
    };
    Ok(Value::str(if before {
        js::slice(&text, 0.0, Some(pos as f64))
    } else {
        js::slice(&text, (pos + du.len()) as f64, None)
    }))
}

fn textafter(ev: &Evaluator, a: &[Node]) -> R {
    text_before_after(ev, a, false)
}

fn textbefore(ev: &Evaluator, a: &[Node]) -> R {
    text_before_after(ev, a, true)
}

fn textsplit(ev: &Evaluator, a: &[Node]) -> R {
    let text = ev.str(arg(a, 0)?);
    let col_delim = match a.get(1) {
        Some(n) => ev.str(n),
        None => String::new(),
    };
    let row_delim = match a.get(2) {
        Some(n) => ev.str(n),
        None => String::new(),
    };
    let split_by = |s: &str, delim: &str| -> Vec<String> {
        if delim.is_empty() {
            vec![s.to_string()]
        } else {
            s.split(delim).map(str::to_string).collect()
        }
    };
    let rows = if row_delim.is_empty() { vec![text] } else { split_by(&text, &row_delim) };
    Ok(Value::M(rows.iter().map(|r| split_by(r, &col_delim).into_iter().map(Scalar::Str).collect()).collect()))
}

fn asc(ev: &Evaluator, a: &[Node]) -> R {
    let s = ev.str(arg(a, 0)?);
    Ok(Value::str(
        s.chars()
            .map(|c| match c {
                '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
                '\u{3000}' => ' ',
                other => other,
            })
            .collect::<String>(),
    ))
}

fn dbcs(ev: &Evaluator, a: &[Node]) -> R {
    let s = ev.str(arg(a, 0)?);
    Ok(Value::str(
        s.chars()
            .map(|c| match c {
                '!'..='~' => char::from_u32(c as u32 + 0xFEE0).unwrap_or(c),
                ' ' => '\u{3000}',
                other => other,
            })
            .collect::<String>(),
    ))
}

fn lenb(ev: &Evaluator, a: &[Node]) -> R {
    Ok(Value::num(js::len16(&ev.str(arg(a, 0)?)) as f64))
}

fn leftb(ev: &Evaluator, a: &[Node]) -> R {
    let s = ev.str(arg(a, 0)?);
    let n = num_or(ev, a, 1, 1.0)?;
    Ok(Value::str(js::slice(&s, 0.0, Some(js::max(&[0.0, js::trunc(n)])))))
}

fn rightb(ev: &Evaluator, a: &[Node]) -> R {
    let s = ev.str(arg(a, 0)?);
    let n = num_or(ev, a, 1, 1.0)?;
    let k = js::trunc(n);
    if k <= 0.0 {
        return Ok(Value::str(""));
    }
    Ok(Value::str(js::slice(&s, -k, None)))
}

fn midb(ev: &Evaluator, a: &[Node]) -> R {
    let s = ev.str(arg(a, 0)?);
    let st = ev.num(arg(a, 1)?)?;
    let ln = ev.num(arg(a, 2)?)?;
    let (start, len) = (js::trunc(st), js::trunc(ln));
    if start < 1.0 || len < 0.0 {
        return Err(ErrCode::VALUE);
    }
    Ok(Value::str(js::slice(&s, start - 1.0, Some(start - 1.0 + len))))
}

fn replaceb(ev: &Evaluator, a: &[Node]) -> R {
    let s = ev.str(arg(a, 0)?);
    let st = ev.num(arg(a, 1)?)?;
    let ln = ev.num(arg(a, 2)?)?;
    let ns = ev.str(arg(a, 3)?);
    let (start, len) = (js::trunc(st), js::trunc(ln));
    if start < 1.0 || len < 0.0 {
        return Err(ErrCode::VALUE);
    }
    Ok(Value::str(js::slice(&s, 0.0, Some(start - 1.0)) + &ns + &js::slice(&s, start - 1.0 + len, None)))
}

fn arraytotext(ev: &Evaluator, a: &[Node]) -> R {
    let v = ev.eval(arg(a, 0)?);
    let fmt = match a.get(1) {
        Some(n) => to_num(&ev.scalar(n))?,
        None => 0.0,
    };
    // A lambda is a plain object there: not text, and `String(x)` gives "[object Object]".
    let is_lambda = matches!(v, Value::L(_));
    let cells = if is_lambda { vec![Scalar::Num(0.0)] } else { flatten(&v) };
    let text = |c: &Scalar| -> String {
        if is_lambda {
            "[object Object]".to_string()
        } else {
            to_str(c)
        }
    };
    if fmt == 1.0 {
        // Strict format: quote text, brace the whole array.
        let parts: Vec<String> = cells
            .iter()
            .map(|c| match c {
                Scalar::Str(s) => format!("\"{s}\""),
                other => text(other),
            })
            .collect();
        return Ok(Value::str(format!("{{{}}}", parts.join(","))));
    }
    Ok(Value::str(cells.iter().map(text).collect::<Vec<_>>().join(", ")))
}

fn valuetotext(ev: &Evaluator, a: &[Node]) -> R {
    let v = ev.scalar(arg(a, 0)?);
    if let Scalar::Err(e) = v {
        return Err(e);
    }
    let fmt = match a.get(1) {
        Some(n) => to_num(&ev.scalar(n))?,
        None => 0.0,
    };
    if fmt == 1.0 {
        if let Scalar::Str(s) = &v {
            return Ok(Value::str(format!("\"{s}\"")));
        }
    }
    Ok(Value::str(to_str(&v)))
}

fn phonetic(ev: &Evaluator, a: &[Node]) -> R {
    Ok(Value::str(ev.str(arg(a, 0)?)))
}

fn bahttext(ev: &Evaluator, a: &[Node]) -> R {
    let x = ev.num(arg(a, 0)?)?;
    Ok(Value::str(js::to_fixed(x, 2) + " \u{0E1A}\u{0E32}\u{0E17}"))
}

pub fn register(r: &mut Registry) {
    let list: &[(&'static str, crate::formula::eval::Fn)] = &[
        ("CHAR", char_fn),
        ("CLEAN", clean),
        ("CODE", code),
        ("DOLLAR", dollar),
        ("EXACT", exact),
        ("FIND", find),
        ("SEARCH", search),
        ("FIXED", fixed),
        ("REPT", rept),
        ("T", t_fn),
        ("UNICHAR", unichar),
        ("UNICODE", unicode),
        ("NUMBERVALUE", numbervalue),
        ("TEXTAFTER", textafter),
        ("TEXTBEFORE", textbefore),
        ("TEXTSPLIT", textsplit),
        ("ASC", asc),
        ("DBCS", dbcs),
        ("LENB", lenb),
        ("LEFTB", leftb),
        ("RIGHTB", rightb),
        ("MIDB", midb),
        ("REPLACEB", replaceb),
        ("FINDB", find),
        ("SEARCHB", search),
        ("ARRAYTOTEXT", arraytotext),
        ("VALUETOTEXT", valuetotext),
        ("PHONETIC", phonetic),
        ("BAHTTEXT", bahttext),
    ];
    for (name, f) in list {
        r.insert(name, *f);
    }
}

#[cfg(test)]
mod tests {
    use crate::formula::testing::eval;
    use crate::formula::value::{Scalar, Value};

    fn s(f: &str) -> String {
        match eval(f) {
            Value::S(Scalar::Str(s)) => s,
            other => format!("{other:?}"),
        }
    }

    #[test]
    fn grouping_and_money() {
        assert_eq!(s("=DOLLAR(1234567.891)"), "$1,234,567.89");
        assert_eq!(s("=DOLLAR(-1234.5,1)"), "($1,234.5)");
        assert_eq!(s("=DOLLAR(1234,-2)"), "$1,200");
        assert_eq!(s("=FIXED(-1234.567,1)"), "-1,234.6");
        assert_eq!(s("=FIXED(1234.567,0,TRUE)"), "1235");
    }

    #[test]
    fn utf16_positions() {
        // U+1F600 is two UTF-16 units: the "b" after it sits at position 4.
        assert_eq!(eval("=FIND(\"b\",\"a\u{1F600}b\")"), Value::num(4.0));
        assert_eq!(s("=TEXTAFTER(\"a\u{1F600}b-c\",\"-\")"), "c");
        assert_eq!(eval("=UNICODE(\"\u{1F600}\")"), Value::num(128512.0));
        assert_eq!(eval("=CODE(\"\u{1F600}\")"), Value::num(55357.0));
    }

    #[test]
    fn wildcard_search_is_case_insensitive() {
        assert_eq!(eval("=SEARCH(\"\u{C9}*S\",\"caf\u{E9} des \u{E9}t\u{E9}s\")"), Value::num(4.0));
        assert_eq!(eval("=SEARCH(\"~?\",\"what?\")"), Value::num(5.0));
    }
}
