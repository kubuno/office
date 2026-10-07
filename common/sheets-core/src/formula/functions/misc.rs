//! Port of the web's `formula-fns/misc.ts`: REGEXTEST / REGEXEXTRACT / REGEXREPLACE, WRAPROWS / WRAPCOLS,
//! HYPERLINK, CELL, INFO, ENCODEURL, FILTERXML and AGGREGATE.
//!
//! Differences from the web, by function:
//!
//! * **REGEX\***: the web uses JavaScript's `RegExp`; here a JS pattern is translated into the syntax of
//!   the `regex` crate ([`translate_js_regex`]): `\d \w \s \b` keep their ASCII / JS meanings, `.` excludes
//!   the four line terminators, a `{` that is not a quantifier is a literal, legacy octal escapes and
//!   identity escapes work as in JS, named groups become plain groups (their names are kept for
//!   `$<name>` in a replacement), inline flags `(?i)` are rejected as the JS parser rejects them. What the
//!   `regex` crate cannot do gives `#VALUE!` where the web gives a result: look-around (`(?=` `(?!` `(?<=`
//!   `(?<!`) and back-references (`\1`, `\k<name>`). Also different: matching is by Unicode scalar value
//!   (JS: UTF-16 code unit, so an astral character is two `.`), JS resets the captures of a group at each
//!   iteration of an enclosing quantifier (the crate keeps the last), case-insensitivity follows Unicode
//!   simple case folding (JS: `toUpperCase` canonicalisation), and a pattern whose compiled size exceeds
//!   the crate's limit is `#VALUE!`.
//! * **FILTERXML**: the web needs the browser's `DOMParser` and `document.evaluate` and answers `#VALUE!`
//!   where they are missing (Node, SSR). Here a small namespace-aware XML 1.0 parser and an XPath 1.0
//!   evaluator reproduce the browser's behaviour for the node-set results the function accepts (see
//!   [`xml`]). Not supported (→ `#VALUE!`): DTD entity declarations, XPath variables, `id()`, `lang()`,
//!   any prefixed name test (the web passes no namespace resolver, so the browser throws).
//! * **WRAPROWS / WRAPCOLS**: a result larger than [`MAX_CELLS`] cells is `#VALUE!` (the web would run out
//!   of memory).

use std::collections::HashMap;

use regex::{Regex, RegexBuilder};

use super::Registry;
use crate::formula::ast::Node;
use crate::formula::eval::{scalar_of, Evaluator};
use crate::formula::value::{flatten, to_str, ErrCode, Matrix, Scalar, Value};
use crate::js;

type R = Result<Value, ErrCode>;

/// The largest matrix WRAPROWS / WRAPCOLS build.
const MAX_CELLS: f64 = 50_000_000.0;

/// `optNum(ev, args, i, def)`.
fn opt_num(ev: &Evaluator, a: &[Node], i: usize, def: f64) -> Result<f64, ErrCode> {
    match a.get(i) {
        Some(n) => ev.num(n),
        None => Ok(def),
    }
}

// ── JavaScript regular expressions ───────────────────────────────────────────

/// A JS pattern translated for the `regex` crate.
struct Translated {
    pattern: String,
    /// The name of each capturing group, in order (`None` for an unnamed group).
    names: Vec<Option<String>>,
}

/// The JS whitespace set of `\s`, as the body of a class.
const JS_SPACE: &str = r"\t\n\x0B\x0C\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}";

/// A literal character as `regex` syntax.
fn lit(c: char) -> String {
    if c.is_ascii_alphanumeric() {
        c.to_string()
    } else {
        format!("\\x{{{:X}}}", c as u32)
    }
}

/// A class escape (`\d`, `\W`, …) as a bracketed class, or `None`.
fn class_escape(c: char) -> Option<String> {
    Some(match c {
        'd' => "[0-9]".to_string(),
        'D' => "[^0-9]".to_string(),
        'w' => "[0-9A-Za-z_]".to_string(),
        'W' => "[^0-9A-Za-z_]".to_string(),
        's' => format!("[{JS_SPACE}]"),
        'S' => format!("[^{JS_SPACE}]"),
        _ => return None,
    })
}

fn hex_value(chars: &[char], from: usize, n: usize) -> Option<u32> {
    if from + n > chars.len() {
        return None;
    }
    let mut v = 0u32;
    for c in &chars[from..from + n] {
        v = v * 16 + c.to_digit(16)?;
    }
    Some(v)
}

/// One escape sequence (the backslash already consumed, `*i` on the character after it): the character it
/// stands for, advancing `*i`. `in_class` selects the class-context rules. `Err` for what the `regex` crate
/// cannot express (back-references) or what JS rejects.
fn escape_char(chars: &[char], i: &mut usize, in_class: bool, group_count: usize, has_named: bool) -> Result<char, ()> {
    let Some(&c) = chars.get(*i) else { return Err(()) };
    *i += 1;
    Ok(match c {
        't' => '\t',
        'n' => '\n',
        'r' => '\r',
        'f' => '\u{C}',
        'v' => '\u{B}',
        'b' if in_class => '\u{8}',
        'x' => match hex_value(chars, *i, 2) {
            Some(v) => {
                *i += 2;
                char::from_u32(v).ok_or(())?
            }
            None => 'x',
        },
        'u' => match hex_value(chars, *i, 4) {
            Some(v) => {
                *i += 4;
                if (0xD800..0xDC00).contains(&v) && chars.get(*i) == Some(&'\\') && chars.get(*i + 1) == Some(&'u') {
                    if let Some(lo) = hex_value(chars, *i + 2, 4).filter(|lo| (0xDC00..0xE000).contains(lo)) {
                        *i += 6;
                        return char::from_u32(0x10000 + ((v - 0xD800) << 10) + (lo - 0xDC00)).ok_or(());
                    }
                }
                // A lone surrogate cannot be matched against Rust text.
                char::from_u32(v).ok_or(())?
            }
            None => 'u',
        },
        'c' => match chars.get(*i) {
            Some(l) if l.is_ascii_alphabetic() => {
                *i += 1;
                char::from_u32(*l as u32 % 32).ok_or(())?
            }
            _ => {
                // `\c` without a letter is a literal backslash; the `c` is read again.
                *i -= 1;
                '\\'
            }
        },
        'k' if !in_class && has_named => return Err(()), // named back-reference
        '0'..='9' => {
            let first = c;
            if !in_class && first != '0' {
                // A decimal escape: a back-reference when that group exists.
                let mut j = *i;
                let mut n = first.to_digit(10).unwrap_or(0) as usize;
                while let Some(d) = chars.get(j).and_then(|c| c.to_digit(10)) {
                    n = n.saturating_mul(10).saturating_add(d as usize);
                    j += 1;
                }
                if n <= group_count {
                    return Err(());
                }
            }
            if first == '8' || first == '9' {
                first
            } else {
                // Legacy octal escape (up to three digits, value ≤ 255).
                let mut v = first.to_digit(8).unwrap_or(0);
                let mut count = 1;
                while count < 3 {
                    match chars.get(*i).and_then(|c| c.to_digit(8)) {
                        Some(d) if v * 8 + d <= 255 => {
                            v = v * 8 + d;
                            *i += 1;
                            count += 1;
                        }
                        _ => break,
                    }
                }
                char::from_u32(v).ok_or(())?
            }
        }
        other => other,
    })
}

/// Counts the capturing groups and tells whether any is named (the pre-scan JS does for `\N` and `\k`).
fn prescan_groups(chars: &[char]) -> (usize, bool) {
    let mut count = 0;
    let mut named = false;
    let mut i = 0;
    let mut in_class = false;
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 1,
            '[' => in_class = true,
            ']' => in_class = false,
            '(' if !in_class => {
                if chars.get(i + 1) == Some(&'?') {
                    if chars.get(i + 2) == Some(&'<') && !matches!(chars.get(i + 3), Some('=') | Some('!')) {
                        count += 1;
                        named = true;
                    }
                } else {
                    count += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    (count, named)
}

/// A class atom: one character, or a set (`\d`, …) as a bracketed class.
enum Atom {
    Ch(char),
    Set(String),
}

fn class_atom(chars: &[char], i: &mut usize, group_count: usize, has_named: bool) -> Result<Atom, ()> {
    let c = *chars.get(*i).ok_or(())?;
    *i += 1;
    if c == '\\' {
        if let Some(&e) = chars.get(*i) {
            if let Some(set) = class_escape(e) {
                *i += 1;
                return Ok(Atom::Set(set));
            }
        }
        return Ok(Atom::Ch(escape_char(chars, i, true, group_count, has_named)?));
    }
    Ok(Atom::Ch(c))
}

/// Translates a JS pattern (no `u` / `s` / `m` flag) for the `regex` crate.
fn translate_js_regex(src: &str) -> Result<Translated, ()> {
    let chars: Vec<char> = src.chars().collect();
    let (group_count, has_named) = prescan_groups(&chars);
    let mut out = String::new();
    let mut names: Vec<Option<String>> = Vec::new();
    let mut can_quant = false;
    let mut depth = 0usize;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        i += 1;
        match c {
            '\\' => {
                let Some(&e) = chars.get(i) else { return Err(()) };
                if let Some(set) = class_escape(e) {
                    i += 1;
                    out.push_str(&set);
                    can_quant = true;
                } else if e == 'b' || e == 'B' {
                    i += 1;
                    out.push_str(if e == 'b' { r"(?-u:\b)" } else { r"(?-u:\B)" });
                    can_quant = false;
                } else {
                    let ch = escape_char(&chars, &mut i, false, group_count, has_named)?;
                    out.push_str(&lit(ch));
                    can_quant = true;
                }
            }
            '[' => {
                let negated = chars.get(i) == Some(&'^');
                if negated {
                    i += 1;
                }
                let mut body = String::new();
                loop {
                    match chars.get(i) {
                        None => return Err(()),
                        Some(']') => {
                            i += 1;
                            break;
                        }
                        Some(_) => {}
                    }
                    let lo = class_atom(&chars, &mut i, group_count, has_named)?;
                    if chars.get(i) == Some(&'-') && chars.get(i + 1).is_some_and(|c| *c != ']') {
                        let save = i;
                        i += 1;
                        let hi = class_atom(&chars, &mut i, group_count, has_named)?;
                        match (&lo, &hi) {
                            (Atom::Ch(a), Atom::Ch(b)) => {
                                if a > b {
                                    return Err(());
                                }
                                body.push_str(&format!("\\x{{{:X}}}-\\x{{{:X}}}", *a as u32, *b as u32));
                                continue;
                            }
                            _ => {
                                // A range with a class escape: the `-` is a literal.
                                i = save;
                            }
                        }
                    }
                    match lo {
                        Atom::Ch(a) => body.push_str(&format!("\\x{{{:X}}}", a as u32)),
                        Atom::Set(s) => body.push_str(&s),
                    }
                }
                if body.is_empty() {
                    // `[]` matches nothing, `[^]` anything.
                    out.push_str(if negated { r"[\x{0}-\x{10FFFF}]" } else { r"[^\x{0}-\x{10FFFF}]" });
                } else {
                    out.push('[');
                    if negated {
                        out.push('^');
                    }
                    out.push_str(&body);
                    out.push(']');
                }
                can_quant = true;
            }
            '(' => {
                if chars.get(i) == Some(&'?') {
                    match chars.get(i + 1) {
                        Some(':') => {
                            i += 2;
                            out.push_str("(?:");
                        }
                        Some('<') if !matches!(chars.get(i + 2), Some('=') | Some('!')) => {
                            let start = i + 2;
                            let mut j = start;
                            while j < chars.len() && chars[j] != '>' {
                                j += 1;
                            }
                            if j >= chars.len() || j == start {
                                return Err(());
                            }
                            let name: String = chars[start..j].iter().collect();
                            let first = name.chars().next().ok_or(())?;
                            if first.is_ascii_digit() || !name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$') {
                                return Err(());
                            }
                            i = j + 1;
                            names.push(Some(name));
                            out.push('(');
                        }
                        // Look-around and inline flags: not expressible (look-around) or not JS (flags).
                        _ => return Err(()),
                    }
                } else {
                    names.push(None);
                    out.push('(');
                }
                depth += 1;
                can_quant = false;
            }
            ')' => {
                if depth == 0 {
                    return Err(());
                }
                depth -= 1;
                out.push(')');
                can_quant = true;
            }
            '|' => {
                out.push('|');
                can_quant = false;
            }
            '^' | '$' => {
                out.push(c);
                can_quant = false;
            }
            '.' => {
                out.push_str(r"[^\n\r\x{2028}\x{2029}]");
                can_quant = true;
            }
            '*' | '+' | '?' => {
                if !can_quant {
                    return Err(());
                }
                out.push(c);
                if chars.get(i) == Some(&'?') {
                    i += 1;
                    out.push('?');
                }
                can_quant = false;
            }
            '{' => {
                // `{n}`, `{n,}`, `{n,m}` is a quantifier; anything else is a literal brace.
                let mut j = i;
                let mut lo = String::new();
                while chars.get(j).is_some_and(|c| c.is_ascii_digit()) {
                    lo.push(chars[j]);
                    j += 1;
                }
                let mut quant = None;
                if !lo.is_empty() {
                    if chars.get(j) == Some(&'}') {
                        quant = Some((j + 1, format!("{{{lo}}}"), lo.clone(), lo.clone()));
                    } else if chars.get(j) == Some(&',') {
                        let mut k = j + 1;
                        let mut hi = String::new();
                        while chars.get(k).is_some_and(|c| c.is_ascii_digit()) {
                            hi.push(chars[k]);
                            k += 1;
                        }
                        if chars.get(k) == Some(&'}') {
                            quant = Some((k + 1, format!("{{{lo},{hi}}}"), lo.clone(), hi));
                        }
                    }
                }
                match quant {
                    Some((end, text, lo, hi)) => {
                        if !can_quant {
                            return Err(());
                        }
                        if !hi.is_empty() {
                            let (l, h): (f64, f64) = (lo.parse().unwrap_or(f64::INFINITY), hi.parse().unwrap_or(f64::INFINITY));
                            if l > h {
                                return Err(());
                            }
                        }
                        out.push_str(&text);
                        i = end;
                        if chars.get(i) == Some(&'?') {
                            i += 1;
                            out.push('?');
                        }
                        can_quant = false;
                    }
                    None => {
                        out.push_str(&lit('{'));
                        can_quant = true;
                    }
                }
            }
            other => {
                out.push_str(&lit(other));
                can_quant = true;
            }
        }
    }
    if depth != 0 {
        return Err(());
    }
    Ok(Translated { pattern: out, names })
}

/// `compileRegex(pattern, flags)`: the compiled expression and its group names, `#VALUE!` when the web's
/// pattern is invalid (or uses what the `regex` crate lacks).
fn compile_regex(pattern: &str, insensitive: bool) -> Result<(Regex, Vec<Option<String>>), ErrCode> {
    let t = translate_js_regex(pattern).map_err(|_| ErrCode::VALUE)?;
    let re = RegexBuilder::new(&t.pattern).case_insensitive(insensitive).build().map_err(|_| ErrCode::VALUE)?;
    Ok((re, t.names))
}

/// The next position after an empty match at `pos` (`lastIndex++`), `None` past the end.
fn advance(text: &str, pos: usize) -> Option<usize> {
    text[pos..].chars().next().map(|c| pos + c.len_utf8())
}

/// Every match of a global exec loop (`while ((m = re.exec(text)) !== null) …`), with the JS rule that
/// an empty match advances `lastIndex` by one.
fn all_matches<'t>(re: &Regex, text: &'t str) -> Vec<regex::Captures<'t>> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos <= text.len() {
        let Some(caps) = re.captures_at(text, pos) else { break };
        let Some(m) = caps.get(0) else { break };
        let (start, end) = (m.start(), m.end());
        out.push(caps);
        if start == end {
            // `m.index === re.lastIndex`
            match advance(text, end) {
                Some(p) => pos = p,
                None => break,
            }
        } else {
            pos = end;
        }
    }
    out
}

fn regextest(ev: &Evaluator, a: &[Node]) -> R {
    if a.len() < 2 {
        return Err(ErrCode::VALUE);
    }
    let text = ev.str(&a[0]);
    let pattern = ev.str(&a[1]);
    let cs = opt_num(ev, a, 2, 0.0)?;
    let (re, _) = compile_regex(&pattern, cs == 1.0)?;
    Ok(Value::bool(re.is_match(&text)))
}

fn regexextract(ev: &Evaluator, a: &[Node]) -> R {
    if a.len() < 2 {
        return Err(ErrCode::VALUE);
    }
    let text = ev.str(&a[0]);
    let pattern = ev.str(&a[1]);
    let mode = opt_num(ev, a, 2, 0.0)?;
    let cs = opt_num(ev, a, 3, 0.0)?;
    let insensitive = cs == 1.0;

    if mode == 1.0 {
        let (re, _) = compile_regex(&pattern, insensitive)?;
        let out: Matrix = all_matches(&re, &text).iter().map(|c| vec![Scalar::str(c.get(0).map(|m| m.as_str()).unwrap_or(""))]).collect();
        return if out.is_empty() { Err(ErrCode::NA) } else { Ok(Value::M(out)) };
    }
    if mode == 2.0 {
        let (re, _) = compile_regex(&pattern, insensitive)?;
        let Some(caps) = re.captures(&text) else { return Err(ErrCode::NA) };
        if caps.len() <= 1 {
            return Ok(Value::str(caps.get(0).map(|m| m.as_str()).unwrap_or("")));
        }
        let rows: Matrix = (1..caps.len()).map(|i| vec![Scalar::str(caps.get(i).map(|m| m.as_str()).unwrap_or(""))]).collect();
        return Ok(Value::M(rows));
    }
    let (re, _) = compile_regex(&pattern, insensitive)?;
    match re.find(&text) {
        Some(m) => Ok(Value::str(m.as_str())),
        None => Err(ErrCode::NA),
    }
}

/// `GetSubstitution` of ECMA-262 for a replacement template.
fn expand_template(tpl: &str, text: &str, start: usize, end: usize, caps: &[Option<&str>], names: &[Option<String>]) -> String {
    let t: Vec<char> = tpl.chars().collect();
    let has_named = names.iter().any(Option::is_some);
    let mut out = String::new();
    let mut i = 0;
    while i < t.len() {
        let c = t[i];
        if c != '$' || i + 1 >= t.len() {
            out.push(c);
            i += 1;
            continue;
        }
        let n = t[i + 1];
        match n {
            '$' => {
                out.push('$');
                i += 2;
            }
            '&' => {
                out.push_str(&text[start..end]);
                i += 2;
            }
            '`' => {
                out.push_str(&text[..start]);
                i += 2;
            }
            '\'' => {
                out.push_str(&text[end..]);
                i += 2;
            }
            '0'..='9' => {
                let d1 = n.to_digit(10).unwrap_or(0) as usize;
                let mut digit_count = 1;
                let mut index = d1;
                if let Some(d2) = t.get(i + 2).and_then(|c| c.to_digit(10)) {
                    let two = d1 * 10 + d2 as usize;
                    if two <= caps.len() {
                        digit_count = 2;
                        index = two;
                    }
                }
                if index >= 1 && index <= caps.len() {
                    out.push_str(caps[index - 1].unwrap_or(""));
                } else {
                    out.extend(&t[i..i + 1 + digit_count]);
                }
                i += 1 + digit_count;
            }
            '<' => {
                if !has_named {
                    out.push_str("$<");
                    i += 2;
                } else if let Some(rel) = t[i + 2..].iter().position(|c| *c == '>') {
                    let name: String = t[i + 2..i + 2 + rel].iter().collect();
                    if let Some(idx) = names.iter().position(|nm| nm.as_deref() == Some(name.as_str())) {
                        out.push_str(caps.get(idx).copied().flatten().unwrap_or(""));
                    }
                    i += 2 + rel + 1;
                } else {
                    out.push_str("$<");
                    i += 2;
                }
            }
            _ => {
                out.push('$');
                i += 1;
            }
        }
    }
    out
}

fn regexreplace(ev: &Evaluator, a: &[Node]) -> R {
    if a.len() < 3 {
        return Err(ErrCode::VALUE);
    }
    let text = ev.str(&a[0]);
    let pattern = ev.str(&a[1]);
    let replacement = ev.str(&a[2]);
    let occurrence = opt_num(ev, a, 3, 0.0)?;
    let cs = opt_num(ev, a, 4, 0.0)?;
    let insensitive = cs == 1.0;
    let (re, names) = compile_regex(&pattern, insensitive)?;
    let matches = all_matches(&re, &text);
    let mut out = String::new();
    let mut last = 0usize;
    for (k, caps) in matches.iter().enumerate() {
        let Some(m) = caps.get(0) else { continue };
        out.push_str(&text[last..m.start()]);
        if occurrence == 0.0 {
            let groups: Vec<Option<&str>> = (1..caps.len()).map(|i| caps.get(i).map(|g| g.as_str())).collect();
            out.push_str(&expand_template(&replacement, &text, m.start(), m.end(), &groups, &names));
        } else if (k + 1) as f64 == occurrence {
            // QUIRK: with an occurrence the web replaces through a function, so `$1`, `$&`… stay literal.
            out.push_str(&replacement);
        } else {
            out.push_str(m.as_str());
        }
        last = m.end();
    }
    out.push_str(&text[last..]);
    Ok(Value::str(out))
}

// ── Array shaping ────────────────────────────────────────────────────────────

fn wraprows(ev: &Evaluator, a: &[Node]) -> R {
    if a.len() < 2 {
        return Err(ErrCode::VALUE);
    }
    let data = flatten(&ev.eval(&a[0]));
    let wrap = ev.num(&a[1])?;
    let w = js::trunc(wrap);
    if w < 1.0 {
        return Err(ErrCode::NUM);
    }
    let pad = if a.len() > 2 { ev.scalar(&a[2]) } else { Scalar::Err(ErrCode::NA) };
    let n = data.len();
    let row_count = (n as f64 / w).ceil();
    if row_count * w > MAX_CELLS {
        return Err(ErrCode::VALUE);
    }
    let w = w as usize;
    let mut rows: Matrix = Vec::new();
    let mut i = 0;
    while i < n {
        let row: Vec<Scalar> = (0..w).map(|j| data.get(i + j).cloned().unwrap_or_else(|| pad.clone())).collect();
        rows.push(row);
        i += w;
    }
    if rows.is_empty() {
        Err(ErrCode::VALUE)
    } else {
        Ok(Value::M(rows))
    }
}

fn wrapcols(ev: &Evaluator, a: &[Node]) -> R {
    if a.len() < 2 {
        return Err(ErrCode::VALUE);
    }
    let data = flatten(&ev.eval(&a[0]));
    let wrap = ev.num(&a[1])?;
    let h = js::trunc(wrap);
    if h < 1.0 {
        return Err(ErrCode::NUM);
    }
    let pad = if a.len() > 2 { ev.scalar(&a[2]) } else { Scalar::Err(ErrCode::NA) };
    let cols = (data.len() as f64 / h).ceil();
    if cols < 1.0 {
        return Err(ErrCode::VALUE);
    }
    if cols * h > MAX_CELLS {
        return Err(ErrCode::VALUE);
    }
    let (h, cols) = (h as usize, cols as usize);
    let mut out: Matrix = (0..h).map(|_| vec![pad.clone(); cols]).collect();
    for (k, v) in data.into_iter().enumerate() {
        out[k % h][k / h] = v;
    }
    Ok(Value::M(out))
}

// ── Lookup-ish and information ───────────────────────────────────────────────

fn hyperlink(ev: &Evaluator, a: &[Node]) -> R {
    if a.is_empty() {
        return Err(ErrCode::VALUE);
    }
    let link = ev.str(&a[0]);
    if a.len() > 1 {
        let friendly = ev.scalar(&a[1]);
        if let Some(e) = friendly.err() {
            return Err(e.clone());
        }
        return Ok(Value::str(to_str(&friendly)));
    }
    Ok(Value::str(link))
}

/// `refAddress(node)`: the A1 address of a plain reference or range start, `""` otherwise.
fn ref_address(node: Option<&Node>) -> String {
    match node {
        Some(Node::Ref { r, .. }) => r.clone(),
        Some(Node::Range { a, .. }) => a.clone(),
        _ => String::new(),
    }
}

/// `splitA1(addr)`: upper-case column letters and the row number.
fn split_a1(addr: &str) -> Option<(String, f64)> {
    let letters: String = addr.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    let digits = &addr[letters.len()..];
    if letters.is_empty() || digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((letters.to_uppercase(), js::parse_int(digits, 10)))
}

/// `colNumber(col)`.
fn col_number(col: &str) -> f64 {
    let mut n = 0.0;
    for ch in col.chars() {
        n = n * 26.0 + (ch as u32 as f64 - 64.0);
    }
    n
}

fn cell(ev: &Evaluator, a: &[Node]) -> R {
    if a.is_empty() {
        return Err(ErrCode::VALUE);
    }
    let info_type = js::lower(&ev.str(&a[0]));
    let ref_node = a.get(1);
    let addr = ref_address(ref_node);
    let parts = if addr.is_empty() { None } else { split_a1(&addr) };
    match info_type.as_str() {
        "address" => Ok(Value::str(match parts {
            Some((col, row)) => format!("${}${}", col, js::to_string(row)),
            None => String::new(),
        })),
        "col" => Ok(Value::num(parts.map(|(c, _)| col_number(&c)).unwrap_or(0.0))),
        "row" => Ok(Value::num(parts.map(|(_, r)| r).unwrap_or(0.0))),
        "contents" => match ref_node {
            None => Ok(Value::str("")),
            Some(n) => Ok(Value::S(scalar_of(&ev.eval(n)))),
        },
        "type" => match ref_node {
            None => Ok(Value::str("b")),
            Some(n) => Ok(Value::str(match scalar_of(&ev.eval(n)) {
                Scalar::Str(s) if s.is_empty() => "b",
                Scalar::Str(_) => "l",
                _ => "v",
            })),
        },
        "prefix" => Ok(Value::str("")),
        "width" => Ok(Value::num(8.0)),
        "format" => Ok(Value::str("G")),
        "color" => Ok(Value::num(0.0)),
        "parentheses" => Ok(Value::num(0.0)),
        "protect" => Ok(Value::num(1.0)),
        _ => Err(ErrCode::VALUE),
    }
}

fn info(ev: &Evaluator, a: &[Node]) -> R {
    if a.is_empty() {
        return Err(ErrCode::VALUE);
    }
    match js::lower(&ev.str(&a[0])).as_str() {
        "numfile" => Ok(Value::num(1.0)),
        "recalc" => Ok(Value::str("Automatic")),
        "system" => Ok(Value::str("pcdos")),
        "release" => Ok(Value::str("16.0")),
        "origin" => Ok(Value::str("$A:$A1")),
        "osversion" => Ok(Value::str("Windows (32-bit) NT 10.00")),
        "directory" => Ok(Value::str("")),
        _ => Err(ErrCode::VALUE),
    }
}

/// `encodeURIComponent(text)`.
fn encode_uri_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn encodeurl(ev: &Evaluator, a: &[Node]) -> R {
    if a.is_empty() {
        return Err(ErrCode::VALUE);
    }
    Ok(Value::str(encode_uri_component(&ev.str(&a[0]))))
}

fn filterxml(ev: &Evaluator, a: &[Node]) -> R {
    if a.len() < 2 {
        return Err(ErrCode::VALUE);
    }
    let xml_text = ev.str(&a[0]);
    let xpath = ev.str(&a[1]);
    match xml::filter(&xml_text, &xpath) {
        Ok(items) => match items.len() {
            0 => Err(ErrCode::VALUE),
            1 => Ok(Value::str(items.into_iter().next().unwrap_or_default())),
            _ => Ok(Value::M(items.into_iter().map(|s| vec![Scalar::Str(s)]).collect())),
        },
        Err(()) => Err(ErrCode::VALUE),
    }
}

// ── AGGREGATE ────────────────────────────────────────────────────────────────

/// `sortAsc(xs)`.
fn sort_asc(xs: &[f64]) -> Vec<f64> {
    let mut v = xs.to_vec();
    js::sort_numbers(&mut v);
    v
}

fn num_or_err(x: Option<f64>, e: ErrCode) -> Scalar {
    match x {
        Some(v) => Scalar::Num(v),
        None => Scalar::Err(e),
    }
}

/// `percentileInc(sorted, p)`.
fn percentile_inc(sorted: &[f64], p: f64) -> Scalar {
    if sorted.is_empty() || p < 0.0 || p > 1.0 {
        return Scalar::Err(ErrCode::NUM);
    }
    if sorted.len() == 1 {
        return Scalar::Num(sorted[0]);
    }
    let rank = p * (sorted.len() - 1) as f64;
    let lo = rank.floor();
    let frac = rank - lo;
    let lo_i = lo as usize;
    if lo_i + 1 >= sorted.len() {
        return Scalar::Num(sorted[lo_i]);
    }
    Scalar::Num(sorted[lo_i] + frac * (sorted[lo_i + 1] - sorted[lo_i]))
}

/// `percentileExc(sorted, p)`.
fn percentile_exc(sorted: &[f64], p: f64) -> Scalar {
    let n = sorted.len();
    if n == 0 {
        return Scalar::Err(ErrCode::NUM);
    }
    if p <= 0.0 || p >= 1.0 {
        return Scalar::Err(ErrCode::NUM);
    }
    let rank = p * (n as f64 + 1.0) - 1.0;
    if rank < 0.0 || rank > n as f64 - 1.0 {
        return Scalar::Err(ErrCode::NUM);
    }
    let lo = rank.floor();
    let frac = rank - lo;
    let lo_i = lo as usize;
    if lo_i + 1 >= n {
        return Scalar::Num(sorted[lo_i]);
    }
    Scalar::Num(sorted[lo_i] + frac * (sorted[lo_i + 1] - sorted[lo_i]))
}

/// A `Map<number, …>` key (`SameValueZero`).
fn key_of(x: f64) -> u64 {
    if x.is_nan() {
        f64::NAN.to_bits()
    } else if x == 0.0 {
        0.0f64.to_bits()
    } else {
        x.to_bits()
    }
}

/// `aggregateCompute(fnum, xs, k)`.
fn aggregate_compute(fnum: f64, xs: &[f64], k: f64) -> Scalar {
    let n = xs.len();
    let nf = n as f64;
    let sum = xs.iter().fold(0.0, |s, x| s + x);
    let mean = if n > 0 { sum / nf } else { 0.0 };
    let ssd = xs.iter().fold(0.0, |s, x| s + (x - mean) * (x - mean));
    let case = if fnum.fract() == 0.0 && fnum.abs() < 1000.0 { fnum as i64 } else { 0 };
    match case {
        1 => {
            if n > 0 {
                Scalar::Num(sum / nf)
            } else {
                Scalar::Err(ErrCode::DIV0)
            }
        }
        2 | 3 => Scalar::Num(nf),
        4 => Scalar::Num(if n > 0 { js::max(xs) } else { 0.0 }),
        5 => Scalar::Num(if n > 0 { js::min(xs) } else { 0.0 }),
        6 => Scalar::Num(xs.iter().fold(1.0, |s, x| s * x)),
        7 => {
            if n > 1 {
                Scalar::Num((ssd / (nf - 1.0)).sqrt())
            } else {
                Scalar::Err(ErrCode::DIV0)
            }
        }
        8 => {
            if n > 0 {
                Scalar::Num((ssd / nf).sqrt())
            } else {
                Scalar::Err(ErrCode::DIV0)
            }
        }
        9 => Scalar::Num(sum),
        10 => {
            if n > 1 {
                Scalar::Num(ssd / (nf - 1.0))
            } else {
                Scalar::Err(ErrCode::DIV0)
            }
        }
        11 => {
            if n > 0 {
                Scalar::Num(ssd / nf)
            } else {
                Scalar::Err(ErrCode::DIV0)
            }
        }
        12 => {
            if n == 0 {
                return Scalar::Err(ErrCode::NUM);
            }
            let s = sort_asc(xs);
            let m = s.len() / 2;
            Scalar::Num(if s.len() % 2 == 1 { s[m] } else { (s[m - 1] + s[m]) / 2.0 })
        }
        13 => {
            if n == 0 {
                return Scalar::Err(ErrCode::NA);
            }
            let mut counts: HashMap<u64, usize> = HashMap::new();
            let mut best = xs[0];
            let mut best_count = 0;
            for &x in xs {
                let c = counts.get(&key_of(x)).copied().unwrap_or(0) + 1;
                counts.insert(key_of(x), c);
                if c > best_count {
                    best_count = c;
                    best = x;
                }
            }
            if best_count <= 1 {
                Scalar::Err(ErrCode::NA)
            } else {
                Scalar::Num(best)
            }
        }
        14 | 15 => {
            let kk = js::trunc(k);
            if kk < 1.0 || kk > nf || kk.is_nan() {
                return Scalar::Err(ErrCode::NUM);
            }
            let s = sort_asc(xs);
            let idx = if case == 14 { n - kk as usize } else { kk as usize - 1 };
            num_or_err(s.get(idx).copied(), ErrCode::NUM)
        }
        16 => percentile_inc(&sort_asc(xs), k),
        17 => {
            let q = js::trunc(k);
            if !(0.0..=4.0).contains(&q) {
                return Scalar::Err(ErrCode::NUM);
            }
            percentile_inc(&sort_asc(xs), q / 4.0)
        }
        18 => percentile_exc(&sort_asc(xs), k),
        19 => {
            let q = js::trunc(k);
            if !(1.0..=3.0).contains(&q) {
                return Scalar::Err(ErrCode::NUM);
            }
            percentile_exc(&sort_asc(xs), q / 4.0)
        }
        _ => Scalar::Err(ErrCode::VALUE),
    }
}

fn aggregate(ev: &Evaluator, a: &[Node]) -> R {
    if a.len() < 3 {
        return Err(ErrCode::VALUE);
    }
    let fnum = ev.num(&a[0])?;
    let opt = ev.num(&a[1])?;
    let f = js::trunc(fnum);
    let o = js::trunc(opt);
    if f < 1.0 || f > 19.0 {
        return Err(ErrCode::VALUE);
    }
    if o < 0.0 || o > 7.0 {
        return Err(ErrCode::VALUE);
    }
    let ignore_errors = o == 2.0 || o == 3.0 || o == 6.0 || o == 7.0;
    let needs_k = f >= 14.0;
    let mut k = 0.0;
    let data_args: &[Node] = if needs_k {
        if a.len() < 4 {
            return Err(ErrCode::VALUE);
        }
        k = ev.num(&a[a.len() - 1])?;
        &a[2..a.len() - 1]
    } else {
        &a[2..]
    };
    let mut xs: Vec<f64> = Vec::new();
    for n in data_args {
        for s in flatten(&ev.eval(n)) {
            match s {
                Scalar::Err(e) => {
                    if ignore_errors {
                        continue;
                    }
                    return Err(e);
                }
                Scalar::Num(x) => xs.push(x),
                Scalar::Bool(b) => xs.push(if b { 1.0 } else { 0.0 }),
                Scalar::Str(t) => {
                    if !t.is_empty() {
                        if let Some(x) = js::number(&t) {
                            xs.push(x);
                        }
                    }
                }
            }
        }
    }
    Ok(Value::S(aggregate_compute(f, &xs, k)))
}

pub fn register(r: &mut Registry) {
    r.insert("REGEXTEST", regextest);
    r.insert("REGEXEXTRACT", regexextract);
    r.insert("REGEXREPLACE", regexreplace);
    r.insert("WRAPROWS", wraprows);
    r.insert("WRAPCOLS", wrapcols);
    r.insert("HYPERLINK", hyperlink);
    r.insert("CELL", cell);
    r.insert("INFO", info);
    r.insert("ENCODEURL", encodeurl);
    r.insert("FILTERXML", filterxml);
    r.insert("AGGREGATE", aggregate);
}


#[cfg(test)]
mod tests {
    use super::*;

    fn rx(p: &str, t: &str) -> Option<String> {
        let (re, _) = compile_regex(p, false).ok()?;
        re.find(t).map(|m| m.as_str().to_string())
    }

    #[test]
    fn js_patterns_translate() {
        assert_eq!(rx(r"\d+", "ab123c"), Some("123".into()));
        assert_eq!(rx(r"a{,2}", "xa{,2}"), Some("a{,2}".into()));
        assert_eq!(rx(r"[\w.]+@x", "me.you@x"), Some("me.you@x".into()));
        assert_eq!(rx(r"[a-c&&b]+", "a&b"), Some("a&b".into()));
        assert_eq!(rx(r"\bfoo\b", "a foo b"), Some("foo".into()));
        assert_eq!(rx(r"(?<y>\d{4})-(\d\d)", "on 2020-03"), Some("2020-03".into()));
        assert_eq!(rx(r"[^]", "x"), Some("x".into()));
        assert_eq!(rx(r"[]", "x"), None);
        assert_eq!(rx(r"a.c", "a\nc"), None);
        assert_eq!(rx(r"\101", "A"), Some("A".into()));
    }

    #[test]
    fn invalid_or_unsupported_patterns() {
        assert!(compile_regex("(", false).is_err());
        assert!(compile_regex("*a", false).is_err());
        assert!(compile_regex("a**", false).is_err());
        assert!(compile_regex("[z-a]", false).is_err());
        assert!(compile_regex("(?i)a", false).is_err());
        assert!(compile_regex("a(?=b)", false).is_err());
        assert!(compile_regex(r"(a)\1", false).is_err());
        assert!(compile_regex("a{2,1}", false).is_err());
        assert!(compile_regex("\\", false).is_err());
        assert!(compile_regex(")", false).is_err());
    }

    #[test]
    fn global_loop_matches_js_empty_match_rule() {
        let (re, _) = compile_regex("a*", false).unwrap();
        let ms: Vec<String> = all_matches(&re, "baaab").iter().map(|c| c[0].to_string()).collect();
        assert_eq!(ms, vec!["", "aaa", "", ""]);
    }

    #[test]
    fn replacement_templates() {
        let names = vec![None, None];
        let caps = [Some("x"), None];
        assert_eq!(expand_template("[$1|$2|$&|$$|$0|$3|$01]", "abc", 1, 2, &caps, &names), "[x||b|$|$0|$3|x]");
        assert_eq!(expand_template("$`-$'", "abc", 1, 2, &caps, &names), "a-c");
        assert_eq!(expand_template("$<n>", "abc", 1, 2, &caps, &names), "$<n>");
        let named = vec![Some("n".to_string())];
        assert_eq!(expand_template("<$<n>>", "abc", 1, 2, &[Some("Z")], &named), "<Z>");
    }

    #[test]
    fn encode_uri_component_matches_js() {
        assert_eq!(encode_uri_component("a b&c/é~*'()!-_."), "a%20b%26c%2F%C3%A9~*'()!-_.");
    }

    #[test]
    fn percentiles() {
        let s = [1.0, 2.0, 3.0, 4.0];
        assert_eq!(percentile_inc(&s, 0.5), Scalar::Num(2.5));
        assert_eq!(percentile_exc(&s, 0.5), Scalar::Num(2.5));
        assert_eq!(percentile_exc(&s, 0.1), Scalar::Err(ErrCode::NUM));
    }
}

// ── FILTERXML: XML 1.0 parser and XPath 1.0 evaluator ────────────────────────

/// What `DOMParser` + `document.evaluate(xpath, doc, null, ORDERED_NODE_SNAPSHOT_TYPE)` give for the
/// node-set expressions FILTERXML accepts, in a small namespace-aware implementation.
///
/// The parser is strict like the browser's XML parser (any well-formedness error, an unbound namespace
/// prefix, an undefined entity, a second root, text outside the root… fails). The evaluator is XPath 1.0
/// without namespace resolution: an unprefixed name test only matches elements and attributes in no
/// namespace, and a prefixed name test is an error, as in a browser given no resolver.
mod xml {
    const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";
    const MAX_DEPTH: usize = 256;

    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Kind {
        Doc,
        Elem,
        Attr,
        Text,
        Comment,
        Pi,
    }

    struct XNode {
        kind: Kind,
        /// The qualified name (a processing instruction's target).
        name: String,
        local: String,
        ns: Option<String>,
        /// Text, attribute value, comment or processing-instruction data.
        value: String,
        parent: Option<usize>,
        children: Vec<usize>,
        attrs: Vec<usize>,
        /// An `xmlns` declaration (not an attribute for XPath).
        ns_decl: bool,
        /// Index of the last node of this node's subtree (document order).
        end: usize,
    }

    struct Doc {
        nodes: Vec<XNode>,
    }

    type Res<T> = Result<T, ()>;

    // ── Parser ───────────────────────────────────────────────────────────────

    struct Parser {
        s: Vec<char>,
        i: usize,
        doc: Doc,
    }

    fn is_xml_char(c: char) -> bool {
        matches!(c, '\t' | '\n' | '\r') || (c >= ' ' && c != '\u{FFFE}' && c != '\u{FFFF}')
    }

    fn is_name_start(c: char) -> bool {
        c.is_alphabetic() || c == '_' || c == ':'
    }

    fn is_name_char(c: char) -> bool {
        c.is_alphanumeric() || matches!(c, '_' | ':' | '-' | '.' | '\u{B7}')
    }

    fn is_ws(c: char) -> bool {
        matches!(c, ' ' | '\t' | '\n' | '\r')
    }

    impl Parser {
        fn peek(&self) -> Option<char> {
            self.s.get(self.i).copied()
        }

        fn starts(&self, t: &str) -> bool {
            for (j, c) in (self.i..).zip(t.chars()) {
                if self.s.get(j) != Some(&c) {
                    return false;
                }
            }
            true
        }

        fn eat(&mut self, t: &str) -> bool {
            if self.starts(t) {
                self.i += t.chars().count();
                true
            } else {
                false
            }
        }

        fn skip_ws(&mut self) -> bool {
            let start = self.i;
            while self.peek().is_some_and(is_ws) {
                self.i += 1;
            }
            self.i > start
        }

        fn name(&mut self) -> Res<String> {
            let first = self.peek().ok_or(())?;
            if !is_name_start(first) {
                return Err(());
            }
            let start = self.i;
            while self.peek().is_some_and(is_name_char) {
                self.i += 1;
            }
            Ok(self.s[start..self.i].iter().collect())
        }

        fn add(&mut self, n: XNode) -> usize {
            let idx = self.doc.nodes.len();
            if let Some(p) = n.parent {
                if n.kind == Kind::Attr {
                    self.doc.nodes[p].attrs.push(idx);
                } else {
                    self.doc.nodes[p].children.push(idx);
                }
            }
            self.doc.nodes.push(n);
            idx
        }

        fn node(kind: Kind, name: &str, value: String, parent: usize) -> XNode {
            XNode { kind, name: name.to_string(), local: name.to_string(), ns: None, value, parent: Some(parent), children: Vec::new(), attrs: Vec::new(), ns_decl: false, end: 0 }
        }

        /// `&…;` after the `&` was consumed.
        fn reference(&mut self, out: &mut String) -> Res<()> {
            if self.eat("#") {
                let hex = self.eat("x");
                let start = self.i;
                while self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                    self.i += 1;
                }
                let digits: String = self.s[start..self.i].iter().collect();
                if digits.is_empty() || !self.eat(";") {
                    return Err(());
                }
                let v = u32::from_str_radix(&digits, if hex { 16 } else { 10 }).map_err(|_| ())?;
                let c = char::from_u32(v).ok_or(())?;
                if !is_xml_char(c) {
                    return Err(());
                }
                out.push(c);
                return Ok(());
            }
            for (n, c) in [("lt;", '<'), ("gt;", '>'), ("amp;", '&'), ("quot;", '"'), ("apos;", '\'')] {
                if self.eat(n) {
                    out.push(c);
                    return Ok(());
                }
            }
            Err(())
        }

        fn comment(&mut self, parent: usize) -> Res<()> {
            // `<!--` consumed
            let start = self.i;
            loop {
                if self.starts("--") {
                    if !self.starts("-->") {
                        return Err(());
                    }
                    break;
                }
                let c = self.peek().ok_or(())?;
                if !is_xml_char(c) {
                    return Err(());
                }
                self.i += 1;
            }
            let data: String = self.s[start..self.i].iter().collect();
            self.i += 3;
            self.add(Self::node(Kind::Comment, "", data, parent));
            Ok(())
        }

        fn pi(&mut self, parent: usize) -> Res<()> {
            // `<?` consumed
            let target = self.name()?;
            if target.eq_ignore_ascii_case("xml") || target.contains(':') {
                return Err(());
            }
            let had_ws = self.skip_ws();
            let start = self.i;
            loop {
                if self.starts("?>") {
                    break;
                }
                let c = self.peek().ok_or(())?;
                if !is_xml_char(c) {
                    return Err(());
                }
                self.i += 1;
            }
            if !had_ws && self.i != start {
                return Err(());
            }
            let data: String = self.s[start..self.i].iter().collect();
            self.i += 2;
            self.add(Self::node(Kind::Pi, &target, data, parent));
            Ok(())
        }

        fn doctype(&mut self) -> Res<()> {
            // `<!DOCTYPE` consumed: skip to the closing `>`, through an internal subset.
            let mut depth = 0i32;
            let mut quote: Option<char> = None;
            loop {
                let c = self.peek().ok_or(())?;
                self.i += 1;
                if let Some(q) = quote {
                    if c == q {
                        quote = None;
                    }
                    continue;
                }
                match c {
                    '"' | '\'' => quote = Some(c),
                    '[' => depth += 1,
                    ']' => depth -= 1,
                    '>' if depth <= 0 => return Ok(()),
                    _ => {}
                }
            }
        }

        fn attr_value(&mut self) -> Res<String> {
            let q = self.peek().ok_or(())?;
            if q != '"' && q != '\'' {
                return Err(());
            }
            self.i += 1;
            let mut out = String::new();
            loop {
                let c = self.peek().ok_or(())?;
                self.i += 1;
                if c == q {
                    return Ok(out);
                }
                match c {
                    '<' => return Err(()),
                    '&' => self.reference(&mut out)?,
                    '\t' | '\n' => out.push(' '),
                    c if !is_xml_char(c) => return Err(()),
                    c => out.push(c),
                }
            }
        }

        fn lookup(scope: &[(String, String)], prefix: &str) -> Option<String> {
            if prefix == "xml" {
                return Some(XML_NS.to_string());
            }
            scope.iter().rev().find(|(p, _)| p == prefix).map(|(_, u)| u.clone())
        }

        fn split_qname(name: &str) -> Res<(Option<&str>, &str)> {
            match name.find(':') {
                None => Ok((None, name)),
                Some(p) => {
                    let (pre, local) = (&name[..p], &name[p + 1..]);
                    if pre.is_empty() || local.is_empty() || local.contains(':') {
                        return Err(());
                    }
                    Ok((Some(pre), local))
                }
            }
        }

        fn element(&mut self, parent: usize, scope: &mut Vec<(String, String)>, depth: usize) -> Res<()> {
            if depth > MAX_DEPTH {
                return Err(());
            }
            // `<` consumed
            let qname = self.name()?;
            let mut raw_attrs: Vec<(String, String)> = Vec::new();
            let self_closing;
            loop {
                let had_ws = self.skip_ws();
                if self.eat("/>") {
                    self_closing = true;
                    break;
                }
                if self.eat(">") {
                    self_closing = false;
                    break;
                }
                if !had_ws {
                    return Err(());
                }
                let an = self.name()?;
                self.skip_ws();
                if !self.eat("=") {
                    return Err(());
                }
                self.skip_ws();
                let av = self.attr_value()?;
                if raw_attrs.iter().any(|(n, _)| *n == an) {
                    return Err(());
                }
                raw_attrs.push((an, av));
            }
            // Namespace declarations of this element.
            let scope_len = scope.len();
            for (n, v) in &raw_attrs {
                if n == "xmlns" {
                    scope.push((String::new(), v.clone()));
                } else if let Some(p) = n.strip_prefix("xmlns:") {
                    if p.is_empty() || p == "xmlns" || v.is_empty() || (p == "xml" && v != XML_NS) {
                        return Err(());
                    }
                    scope.push((p.to_string(), v.clone()));
                }
            }
            let (pre, local) = Self::split_qname(&qname)?;
            if pre == Some("xmlns") {
                return Err(());
            }
            let ns = match pre {
                Some(p) => Some(Self::lookup(scope, p).ok_or(())?),
                None => Self::lookup(scope, "").filter(|u| !u.is_empty()),
            };
            let el = self.add(XNode { kind: Kind::Elem, name: qname.clone(), local: local.to_string(), ns, value: String::new(), parent: Some(parent), children: Vec::new(), attrs: Vec::new(), ns_decl: false, end: 0 });
            let mut seen: Vec<(Option<String>, String)> = Vec::new();
            for (n, v) in raw_attrs {
                let decl = n == "xmlns" || n.starts_with("xmlns:");
                let (apre, alocal) = Self::split_qname(&n)?;
                let ans = if decl {
                    None
                } else {
                    match apre {
                        Some(p) => Some(Self::lookup(scope, p).ok_or(())?),
                        None => None,
                    }
                };
                if !decl {
                    let key = (ans.clone(), alocal.to_string());
                    if seen.contains(&key) {
                        return Err(());
                    }
                    seen.push(key);
                }
                self.add(XNode { kind: Kind::Attr, name: n.clone(), local: alocal.to_string(), ns: ans, value: v, parent: Some(el), children: Vec::new(), attrs: Vec::new(), ns_decl: decl, end: 0 });
            }
            if !self_closing {
                self.content(el, scope, depth)?;
                if !self.eat("</") {
                    return Err(());
                }
                let close = self.name()?;
                if close != qname {
                    return Err(());
                }
                self.skip_ws();
                if !self.eat(">") {
                    return Err(());
                }
            }
            scope.truncate(scope_len);
            let last = self.doc.nodes.len() - 1;
            self.doc.nodes[el].end = last;
            Ok(())
        }

        fn content(&mut self, el: usize, scope: &mut Vec<(String, String)>, depth: usize) -> Res<()> {
            let mut text = String::new();
            loop {
                match self.peek() {
                    None => return Err(()),
                    Some('<') => {
                        if self.starts("</") {
                            self.flush_text(el, &mut text);
                            return Ok(());
                        }
                        if self.eat("<!--") {
                            self.flush_text(el, &mut text);
                            self.comment(el)?;
                        } else if self.eat("<![CDATA[") {
                            self.flush_text(el, &mut text);
                            let start = self.i;
                            loop {
                                if self.starts("]]>") {
                                    break;
                                }
                                let c = self.peek().ok_or(())?;
                                if !is_xml_char(c) {
                                    return Err(());
                                }
                                self.i += 1;
                            }
                            let data: String = self.s[start..self.i].iter().collect();
                            self.i += 3;
                            if !data.is_empty() {
                                self.add(Self::node(Kind::Text, "", data, el));
                            }
                        } else if self.eat("<?") {
                            self.flush_text(el, &mut text);
                            self.pi(el)?;
                        } else {
                            self.flush_text(el, &mut text);
                            self.i += 1;
                            self.element(el, scope, depth + 1)?;
                        }
                    }
                    Some('&') => {
                        self.i += 1;
                        self.reference(&mut text)?;
                    }
                    Some(']') if self.starts("]]>") => return Err(()),
                    Some(c) => {
                        if !is_xml_char(c) {
                            return Err(());
                        }
                        text.push(c);
                        self.i += 1;
                    }
                }
            }
        }

        fn flush_text(&mut self, parent: usize, text: &mut String) {
            if !text.is_empty() {
                let t = std::mem::take(text);
                self.add(Self::node(Kind::Text, "", t, parent));
            }
        }
    }

    fn parse(src: &str) -> Res<Doc> {
        // End-of-line normalisation.
        let normalised = src.replace("\r\n", "\n").replace('\r', "\n");
        let chars: Vec<char> = normalised.strip_prefix('\u{FEFF}').unwrap_or(&normalised).chars().collect();
        let root = XNode { kind: Kind::Doc, name: String::new(), local: String::new(), ns: None, value: String::new(), parent: None, children: Vec::new(), attrs: Vec::new(), ns_decl: false, end: 0 };
        let mut p = Parser { s: chars, i: 0, doc: Doc { nodes: vec![root] } };
        if p.starts("<?xml") && p.s.get(5).is_some_and(|c| is_ws(*c)) {
            p.i += 5;
            loop {
                if p.starts("?>") {
                    p.i += 2;
                    break;
                }
                let c = p.peek().ok_or(())?;
                if !is_xml_char(c) {
                    return Err(());
                }
                p.i += 1;
            }
        }
        let mut have_root = false;
        loop {
            p.skip_ws();
            if p.peek().is_none() {
                break;
            }
            if p.eat("<!--") {
                p.comment(0)?;
            } else if p.eat("<?") {
                p.pi(0)?;
            } else if p.eat("<!DOCTYPE") {
                if have_root {
                    return Err(());
                }
                p.doctype()?;
            } else if p.peek() == Some('<') {
                if have_root {
                    return Err(());
                }
                p.i += 1;
                let mut scope = Vec::new();
                p.element(0, &mut scope, 1)?;
                have_root = true;
            } else {
                return Err(());
            }
        }
        if !have_root {
            return Err(());
        }
        let last = p.doc.nodes.len() - 1;
        p.doc.nodes[0].end = last;
        Ok(p.doc)
    }

    impl Doc {
        fn text_content(&self, i: usize) -> String {
            let n = &self.nodes[i];
            match n.kind {
                Kind::Doc => String::new(),
                Kind::Elem => {
                    let mut out = String::new();
                    for j in i + 1..=n.end {
                        if self.nodes[j].kind == Kind::Text {
                            out.push_str(&self.nodes[j].value);
                        }
                    }
                    out
                }
                _ => n.value.clone(),
            }
        }

        fn is_ancestor(&self, a: usize, d: usize) -> bool {
            let mut cur = self.nodes[d].parent;
            while let Some(p) = cur {
                if p == a {
                    return true;
                }
                cur = self.nodes[p].parent;
            }
            false
        }
    }

    // ── XPath: lexer ─────────────────────────────────────────────────────────

    #[derive(Clone, Debug, PartialEq)]
    enum Tok {
        Num(f64),
        Lit(String),
        Name(String),
        Axis(String),
        LParen,
        RParen,
        LBrack,
        RBrack,
        Dot,
        DotDot,
        At,
        Comma,
        Slash,
        DSlash,
        Pipe,
        Plus,
        Minus,
        Eq,
        Ne,
        Lt,
        Le,
        Gt,
        Ge,
        Star,
        Mul,
        OpName(String),
    }

    fn lex(src: &str) -> Res<Vec<Tok>> {
        let c: Vec<char> = src.chars().collect();
        let mut i = 0;
        let mut out: Vec<Tok> = Vec::new();
        // After these tokens (or at the start) a `*` is a name test and a name is not an operator name.
        let operand_expected = |prev: Option<&Tok>| match prev {
            None => true,
            Some(t) => matches!(
                t,
                Tok::At | Tok::Axis(_) | Tok::LParen | Tok::LBrack | Tok::Comma | Tok::Slash | Tok::DSlash | Tok::Pipe | Tok::Plus | Tok::Minus | Tok::Eq | Tok::Ne | Tok::Lt | Tok::Le | Tok::Gt | Tok::Ge | Tok::Mul | Tok::OpName(_)
            ),
        };
        while i < c.len() {
            let ch = c[i];
            if is_ws(ch) {
                i += 1;
                continue;
            }
            let prev = out.last();
            match ch {
                '(' => out.push(Tok::LParen),
                ')' => out.push(Tok::RParen),
                '[' => out.push(Tok::LBrack),
                ']' => out.push(Tok::RBrack),
                ',' => out.push(Tok::Comma),
                '@' => out.push(Tok::At),
                '|' => out.push(Tok::Pipe),
                '+' => out.push(Tok::Plus),
                '-' => out.push(Tok::Minus),
                '=' => out.push(Tok::Eq),
                '!' => {
                    if c.get(i + 1) != Some(&'=') {
                        return Err(());
                    }
                    i += 1;
                    out.push(Tok::Ne);
                }
                '<' => {
                    if c.get(i + 1) == Some(&'=') {
                        i += 1;
                        out.push(Tok::Le);
                    } else {
                        out.push(Tok::Lt);
                    }
                }
                '>' => {
                    if c.get(i + 1) == Some(&'=') {
                        i += 1;
                        out.push(Tok::Ge);
                    } else {
                        out.push(Tok::Gt);
                    }
                }
                '/' => {
                    if c.get(i + 1) == Some(&'/') {
                        i += 1;
                        out.push(Tok::DSlash);
                    } else {
                        out.push(Tok::Slash);
                    }
                }
                '*' => out.push(if operand_expected(prev) { Tok::Star } else { Tok::Mul }),
                '"' | '\'' => {
                    let mut j = i + 1;
                    while j < c.len() && c[j] != ch {
                        j += 1;
                    }
                    if j >= c.len() {
                        return Err(());
                    }
                    out.push(Tok::Lit(c[i + 1..j].iter().collect()));
                    i = j;
                }
                '.' if c.get(i + 1).is_some_and(|d| d.is_ascii_digit()) => {
                    let (n, j) = lex_number(&c, i);
                    out.push(Tok::Num(n));
                    i = j - 1;
                }
                '.' => {
                    if c.get(i + 1) == Some(&'.') {
                        i += 1;
                        out.push(Tok::DotDot);
                    } else {
                        out.push(Tok::Dot);
                    }
                }
                d if d.is_ascii_digit() => {
                    let (n, j) = lex_number(&c, i);
                    out.push(Tok::Num(n));
                    i = j - 1;
                }
                s if is_name_start(s) && s != ':' => {
                    let mut j = i;
                    while j < c.len() && is_name_char(c[j]) && c[j] != ':' {
                        j += 1;
                    }
                    let mut name: String = c[i..j].iter().collect();
                    // A prefix: `p:local` or `p:*` (not `::`).
                    if c.get(j) == Some(&':') && c.get(j + 1) != Some(&':') {
                        if c.get(j + 1) == Some(&'*') {
                            name.push_str(":*");
                            j += 2;
                        } else {
                            let k0 = j + 1;
                            let mut k = k0;
                            while k < c.len() && is_name_char(c[k]) && c[k] != ':' {
                                k += 1;
                            }
                            if k == k0 {
                                return Err(());
                            }
                            name.push(':');
                            name.extend(&c[k0..k]);
                            j = k;
                        }
                    }
                    // An axis name followed by `::`.
                    let mut k = j;
                    while k < c.len() && is_ws(c[k]) {
                        k += 1;
                    }
                    if c.get(k) == Some(&':') && c.get(k + 1) == Some(&':') {
                        out.push(Tok::Axis(name));
                        i = k + 2;
                        continue;
                    }
                    if !operand_expected(prev) && matches!(name.as_str(), "and" | "or" | "mod" | "div") {
                        out.push(Tok::OpName(name));
                    } else {
                        out.push(Tok::Name(name));
                    }
                    i = j;
                    continue;
                }
                _ => return Err(()),
            }
            i += 1;
        }
        Ok(out)
    }

    fn lex_number(c: &[char], start: usize) -> (f64, usize) {
        let mut j = start;
        while j < c.len() && c[j].is_ascii_digit() {
            j += 1;
        }
        if c.get(j) == Some(&'.') {
            j += 1;
            while j < c.len() && c[j].is_ascii_digit() {
                j += 1;
            }
        }
        let text: String = c[start..j].iter().collect();
        (text.parse().unwrap_or(f64::NAN), j)
    }

    // ── XPath: AST and parser ────────────────────────────────────────────────

    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Axis {
        Child,
        Descendant,
        DescendantOrSelf,
        Parent,
        Ancestor,
        AncestorOrSelf,
        FollowingSibling,
        PrecedingSibling,
        Following,
        Preceding,
        Attribute,
        OwnNode,
    }

    #[derive(Clone, Debug)]
    enum Test {
        Name(String),
        Any,
        Text,
        Comment,
        Pi(Option<String>),
        Node,
    }

    #[derive(Clone, Debug)]
    struct Step {
        axis: Axis,
        test: Test,
        preds: Vec<Ex>,
    }

    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Op {
        Or,
        And,
        Eq,
        Ne,
        Lt,
        Le,
        Gt,
        Ge,
        Add,
        Sub,
        Mul,
        Div,
        Mod,
    }

    #[derive(Clone, Debug)]
    enum Ex {
        Num(f64),
        Lit(String),
        Neg(Box<Ex>),
        Bin(Op, Box<Ex>, Box<Ex>),
        Union(Box<Ex>, Box<Ex>),
        Func(String, Vec<Ex>),
        Filter(Box<Ex>, Vec<Ex>),
        Path { origin: Option<Box<Ex>>, absolute: bool, steps: Vec<Step> },
    }

    struct XParser {
        t: Vec<Tok>,
        i: usize,
    }

    impl XParser {
        fn peek(&self) -> Option<&Tok> {
            self.t.get(self.i)
        }
        fn peek_at(&self, k: usize) -> Option<&Tok> {
            self.t.get(self.i + k)
        }
        fn next(&mut self) -> Option<Tok> {
            let t = self.t.get(self.i).cloned();
            self.i += 1;
            t
        }
        fn eat(&mut self, t: &Tok) -> bool {
            if self.peek() == Some(t) {
                self.i += 1;
                true
            } else {
                false
            }
        }

        fn or(&mut self) -> Res<Ex> {
            let mut l = self.and()?;
            while matches!(self.peek(), Some(Tok::OpName(n)) if n == "or") {
                self.i += 1;
                let r = self.and()?;
                l = Ex::Bin(Op::Or, Box::new(l), Box::new(r));
            }
            Ok(l)
        }
        fn and(&mut self) -> Res<Ex> {
            let mut l = self.equality()?;
            while matches!(self.peek(), Some(Tok::OpName(n)) if n == "and") {
                self.i += 1;
                let r = self.equality()?;
                l = Ex::Bin(Op::And, Box::new(l), Box::new(r));
            }
            Ok(l)
        }
        fn equality(&mut self) -> Res<Ex> {
            let mut l = self.relational()?;
            loop {
                let op = match self.peek() {
                    Some(Tok::Eq) => Op::Eq,
                    Some(Tok::Ne) => Op::Ne,
                    _ => return Ok(l),
                };
                self.i += 1;
                let r = self.relational()?;
                l = Ex::Bin(op, Box::new(l), Box::new(r));
            }
        }
        fn relational(&mut self) -> Res<Ex> {
            let mut l = self.additive()?;
            loop {
                let op = match self.peek() {
                    Some(Tok::Lt) => Op::Lt,
                    Some(Tok::Le) => Op::Le,
                    Some(Tok::Gt) => Op::Gt,
                    Some(Tok::Ge) => Op::Ge,
                    _ => return Ok(l),
                };
                self.i += 1;
                let r = self.additive()?;
                l = Ex::Bin(op, Box::new(l), Box::new(r));
            }
        }
        fn additive(&mut self) -> Res<Ex> {
            let mut l = self.multiplicative()?;
            loop {
                let op = match self.peek() {
                    Some(Tok::Plus) => Op::Add,
                    Some(Tok::Minus) => Op::Sub,
                    _ => return Ok(l),
                };
                self.i += 1;
                let r = self.multiplicative()?;
                l = Ex::Bin(op, Box::new(l), Box::new(r));
            }
        }
        fn multiplicative(&mut self) -> Res<Ex> {
            let mut l = self.unary()?;
            loop {
                let op = match self.peek() {
                    Some(Tok::Mul) => Op::Mul,
                    Some(Tok::OpName(n)) if n == "div" => Op::Div,
                    Some(Tok::OpName(n)) if n == "mod" => Op::Mod,
                    _ => return Ok(l),
                };
                self.i += 1;
                let r = self.unary()?;
                l = Ex::Bin(op, Box::new(l), Box::new(r));
            }
        }
        fn unary(&mut self) -> Res<Ex> {
            if self.eat(&Tok::Minus) {
                let e = self.unary()?;
                return Ok(Ex::Neg(Box::new(e)));
            }
            self.union()
        }
        fn union(&mut self) -> Res<Ex> {
            let mut l = self.path()?;
            while self.eat(&Tok::Pipe) {
                let r = self.path()?;
                l = Ex::Union(Box::new(l), Box::new(r));
            }
            Ok(l)
        }

        fn is_node_type(n: &str) -> bool {
            matches!(n, "comment" | "text" | "processing-instruction" | "node")
        }

        fn starts_step(&self) -> bool {
            matches!(self.peek(), Some(Tok::Name(_)) | Some(Tok::Star) | Some(Tok::At) | Some(Tok::Dot) | Some(Tok::DotDot) | Some(Tok::Axis(_)))
        }

        fn path(&mut self) -> Res<Ex> {
            match self.peek() {
                Some(Tok::Slash) => {
                    self.i += 1;
                    let mut steps = Vec::new();
                    if self.starts_step() {
                        self.rel_steps(&mut steps)?;
                    }
                    Ok(Ex::Path { origin: None, absolute: true, steps })
                }
                Some(Tok::DSlash) => {
                    self.i += 1;
                    let mut steps = vec![Step { axis: Axis::DescendantOrSelf, test: Test::Node, preds: Vec::new() }];
                    self.rel_steps(&mut steps)?;
                    Ok(Ex::Path { origin: None, absolute: true, steps })
                }
                Some(Tok::Num(_)) | Some(Tok::Lit(_)) | Some(Tok::LParen) => self.filter_path(),
                Some(Tok::Name(n)) if matches!(self.peek_at(1), Some(Tok::LParen)) && !Self::is_node_type(n) => self.filter_path(),
                _ => {
                    let mut steps = Vec::new();
                    self.rel_steps(&mut steps)?;
                    Ok(Ex::Path { origin: None, absolute: false, steps })
                }
            }
        }

        fn filter_path(&mut self) -> Res<Ex> {
            let primary = self.primary()?;
            let mut preds = Vec::new();
            while self.peek() == Some(&Tok::LBrack) {
                preds.push(self.predicate()?);
            }
            let base = if preds.is_empty() { primary } else { Ex::Filter(Box::new(primary), preds) };
            match self.peek() {
                Some(Tok::Slash) => {
                    self.i += 1;
                    let mut steps = Vec::new();
                    self.rel_steps(&mut steps)?;
                    Ok(Ex::Path { origin: Some(Box::new(base)), absolute: false, steps })
                }
                Some(Tok::DSlash) => {
                    self.i += 1;
                    let mut steps = vec![Step { axis: Axis::DescendantOrSelf, test: Test::Node, preds: Vec::new() }];
                    self.rel_steps(&mut steps)?;
                    Ok(Ex::Path { origin: Some(Box::new(base)), absolute: false, steps })
                }
                _ => Ok(base),
            }
        }

        fn primary(&mut self) -> Res<Ex> {
            match self.next() {
                Some(Tok::Num(n)) => Ok(Ex::Num(n)),
                Some(Tok::Lit(s)) => Ok(Ex::Lit(s)),
                Some(Tok::LParen) => {
                    let e = self.or()?;
                    if !self.eat(&Tok::RParen) {
                        return Err(());
                    }
                    Ok(e)
                }
                Some(Tok::Name(name)) => {
                    // A function call.
                    if !self.eat(&Tok::LParen) {
                        return Err(());
                    }
                    let mut args = Vec::new();
                    if !self.eat(&Tok::RParen) {
                        loop {
                            args.push(self.or()?);
                            if self.eat(&Tok::Comma) {
                                continue;
                            }
                            if self.eat(&Tok::RParen) {
                                break;
                            }
                            return Err(());
                        }
                    }
                    Ok(Ex::Func(name, args))
                }
                _ => Err(()),
            }
        }

        fn predicate(&mut self) -> Res<Ex> {
            if !self.eat(&Tok::LBrack) {
                return Err(());
            }
            let e = self.or()?;
            if !self.eat(&Tok::RBrack) {
                return Err(());
            }
            Ok(e)
        }

        fn rel_steps(&mut self, steps: &mut Vec<Step>) -> Res<()> {
            steps.push(self.step()?);
            loop {
                match self.peek() {
                    Some(Tok::Slash) => {
                        self.i += 1;
                    }
                    Some(Tok::DSlash) => {
                        self.i += 1;
                        steps.push(Step { axis: Axis::DescendantOrSelf, test: Test::Node, preds: Vec::new() });
                    }
                    _ => return Ok(()),
                }
                steps.push(self.step()?);
            }
        }

        fn step(&mut self) -> Res<Step> {
            match self.peek() {
                Some(Tok::Dot) => {
                    self.i += 1;
                    return Ok(Step { axis: Axis::OwnNode, test: Test::Node, preds: Vec::new() });
                }
                Some(Tok::DotDot) => {
                    self.i += 1;
                    return Ok(Step { axis: Axis::Parent, test: Test::Node, preds: Vec::new() });
                }
                _ => {}
            }
            let axis = match self.peek() {
                Some(Tok::At) => {
                    self.i += 1;
                    Axis::Attribute
                }
                Some(Tok::Axis(n)) => {
                    let a = match n.as_str() {
                        "child" => Axis::Child,
                        "descendant" => Axis::Descendant,
                        "descendant-or-self" => Axis::DescendantOrSelf,
                        "parent" => Axis::Parent,
                        "ancestor" => Axis::Ancestor,
                        "ancestor-or-self" => Axis::AncestorOrSelf,
                        "following-sibling" => Axis::FollowingSibling,
                        "preceding-sibling" => Axis::PrecedingSibling,
                        "following" => Axis::Following,
                        "preceding" => Axis::Preceding,
                        "attribute" => Axis::Attribute,
                        "self" => Axis::OwnNode,
                        _ => return Err(()),
                    };
                    self.i += 1;
                    a
                }
                _ => Axis::Child,
            };
            let test = match self.next() {
                Some(Tok::Star) => Test::Any,
                Some(Tok::Name(n)) => {
                    if self.peek() == Some(&Tok::LParen) && Self::is_node_type(&n) {
                        self.i += 1;
                        let t = match n.as_str() {
                            "text" => Test::Text,
                            "comment" => Test::Comment,
                            "node" => Test::Node,
                            _ => {
                                let mut target = None;
                                if let Some(Tok::Lit(s)) = self.peek().cloned() {
                                    self.i += 1;
                                    target = Some(s);
                                }
                                Test::Pi(target)
                            }
                        };
                        if !self.eat(&Tok::RParen) {
                            return Err(());
                        }
                        t
                    } else {
                        // No namespace resolver: a prefixed name test throws in the browser.
                        if n.contains(':') {
                            return Err(());
                        }
                        Test::Name(n)
                    }
                }
                _ => return Err(()),
            };
            let mut preds = Vec::new();
            while self.peek() == Some(&Tok::LBrack) {
                preds.push(self.predicate()?);
            }
            Ok(Step { axis, test, preds })
        }
    }

    // ── XPath: evaluation ────────────────────────────────────────────────────

    #[derive(Clone, Debug)]
    enum Val {
        Nodes(Vec<usize>),
        Str(String),
        Num(f64),
        Bool(bool),
    }

    #[derive(Clone, Copy)]
    struct Ctx {
        node: usize,
        pos: usize,
        size: usize,
    }

    fn xpath_number(s: &str) -> f64 {
        let t = s.trim_matches(is_ws);
        let b = t.as_bytes();
        let mut i = 0;
        if b.first() == Some(&b'-') {
            i = 1;
        }
        let ds = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        let int_digits = i - ds;
        let mut frac_digits = 0;
        if i < b.len() && b[i] == b'.' {
            i += 1;
            let fs = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            frac_digits = i - fs;
        }
        if i != b.len() || (int_digits == 0 && frac_digits == 0) {
            return f64::NAN;
        }
        t.parse().unwrap_or(f64::NAN)
    }

    fn number_string(n: f64) -> String {
        if n.is_nan() {
            "NaN".to_string()
        } else if n.is_infinite() {
            if n > 0.0 { "Infinity" } else { "-Infinity" }.to_string()
        } else if n == 0.0 {
            "0".to_string()
        } else {
            format!("{n}")
        }
    }

    struct Eval<'a> {
        d: &'a Doc,
    }

    impl Eval<'_> {
        fn string_of(&self, v: &Val) -> String {
            match v {
                Val::Str(s) => s.clone(),
                Val::Num(n) => number_string(*n),
                Val::Bool(b) => if *b { "true" } else { "false" }.to_string(),
                Val::Nodes(n) => n.first().map(|i| self.d.text_content(*i)).unwrap_or_default(),
            }
        }

        fn number_of(&self, v: &Val) -> f64 {
            match v {
                Val::Num(n) => *n,
                Val::Bool(b) => if *b { 1.0 } else { 0.0 },
                other => xpath_number(&self.string_of(other)),
            }
        }

        fn bool_of(&self, v: &Val) -> bool {
            match v {
                Val::Bool(b) => *b,
                Val::Num(n) => *n != 0.0 && !n.is_nan(),
                Val::Str(s) => !s.is_empty(),
                Val::Nodes(n) => !n.is_empty(),
            }
        }

        fn axis_nodes(&self, axis: Axis, n: usize) -> Vec<usize> {
            let d = self.d;
            let node = &d.nodes[n];
            match axis {
                Axis::Child => node.children.clone(),
                Axis::Descendant => (n + 1..=node.end).filter(|j| d.nodes[*j].kind != Kind::Attr).collect(),
                Axis::DescendantOrSelf => std::iter::once(n).chain((n + 1..=node.end).filter(|j| d.nodes[*j].kind != Kind::Attr)).collect(),
                Axis::Parent => node.parent.into_iter().collect(),
                Axis::Ancestor | Axis::AncestorOrSelf => {
                    let mut out = Vec::new();
                    if axis == Axis::AncestorOrSelf {
                        out.push(n);
                    }
                    let mut cur = node.parent;
                    while let Some(p) = cur {
                        out.push(p);
                        cur = d.nodes[p].parent;
                    }
                    out
                }
                Axis::FollowingSibling | Axis::PrecedingSibling => {
                    let Some(p) = node.parent else { return Vec::new() };
                    if node.kind == Kind::Attr {
                        return Vec::new();
                    }
                    let sibs = &d.nodes[p].children;
                    let pos = sibs.iter().position(|j| *j == n).unwrap_or(0);
                    if axis == Axis::FollowingSibling {
                        sibs[pos + 1..].to_vec()
                    } else {
                        sibs[..pos].iter().rev().copied().collect()
                    }
                }
                Axis::Following => {
                    let from = if node.kind == Kind::Attr { n } else { node.end };
                    (from + 1..d.nodes.len()).filter(|j| d.nodes[*j].kind != Kind::Attr).collect()
                }
                Axis::Preceding => (0..n).rev().filter(|j| d.nodes[*j].kind != Kind::Attr && !d.is_ancestor(*j, n)).collect(),
                Axis::Attribute => node.attrs.iter().copied().filter(|j| !d.nodes[*j].ns_decl).collect(),
                Axis::OwnNode => vec![n],
            }
        }

        fn test_matches(&self, test: &Test, axis: Axis, n: usize) -> bool {
            let node = &self.d.nodes[n];
            let principal = if axis == Axis::Attribute { Kind::Attr } else { Kind::Elem };
            match test {
                Test::Any => node.kind == principal,
                Test::Name(name) => node.kind == principal && node.ns.is_none() && node.local == *name,
                Test::Text => node.kind == Kind::Text,
                Test::Comment => node.kind == Kind::Comment,
                Test::Pi(t) => node.kind == Kind::Pi && t.as_ref().is_none_or(|t| *t == node.name),
                Test::Node => true,
            }
        }

        fn apply_preds(&self, mut list: Vec<usize>, preds: &[Ex]) -> Res<Vec<usize>> {
            for p in preds {
                let size = list.len();
                let mut kept = Vec::new();
                for (k, n) in list.iter().enumerate() {
                    let v = self.eval(p, Ctx { node: *n, pos: k + 1, size })?;
                    let keep = match v {
                        Val::Num(x) => x == (k + 1) as f64,
                        other => self.bool_of(&other),
                    };
                    if keep {
                        kept.push(*n);
                    }
                }
                list = kept;
            }
            Ok(list)
        }

        fn eval_path(&self, origin: &Option<Box<Ex>>, absolute: bool, steps: &[Step], ctx: Ctx) -> Res<Val> {
            let mut cur: Vec<usize> = match origin {
                Some(o) => match self.eval(o, ctx)? {
                    Val::Nodes(n) => n,
                    _ => return Err(()),
                },
                None if absolute => vec![0],
                None => vec![ctx.node],
            };
            for st in steps {
                let mut next: Vec<usize> = Vec::new();
                for n in &cur {
                    let cands: Vec<usize> = self.axis_nodes(st.axis, *n).into_iter().filter(|j| self.test_matches(&st.test, st.axis, *j)).collect();
                    next.extend(self.apply_preds(cands, &st.preds)?);
                }
                next.sort_unstable();
                next.dedup();
                cur = next;
            }
            Ok(Val::Nodes(cur))
        }

        /// Compares two scalars (no node-set) with XPath 1.0's conversion rules.
        fn compare_scalars(&self, op: Op, l: &Val, r: &Val) -> bool {
            if matches!(op, Op::Eq | Op::Ne) {
                let eq = if matches!(l, Val::Bool(_)) || matches!(r, Val::Bool(_)) {
                    self.bool_of(l) == self.bool_of(r)
                } else if matches!(l, Val::Num(_)) || matches!(r, Val::Num(_)) {
                    self.number_of(l) == self.number_of(r)
                } else {
                    self.string_of(l) == self.string_of(r)
                };
                return if op == Op::Eq { eq } else { !eq };
            }
            let (a, b) = (self.number_of(l), self.number_of(r));
            match op {
                Op::Lt => a < b,
                Op::Le => a <= b,
                Op::Gt => a > b,
                _ => a >= b,
            }
        }

        /// A comparison where at least one side is a node-set.
        fn compare(&self, op: Op, l: &Val, r: &Val) -> bool {
            match (l, r) {
                (Val::Nodes(a), Val::Nodes(b)) => a.iter().any(|i| {
                    let x = Val::Str(self.d.text_content(*i));
                    b.iter().any(|j| self.compare_scalars_text(op, &x, &Val::Str(self.d.text_content(*j))))
                }),
                (Val::Nodes(a), other) => match other {
                    Val::Bool(_) => self.compare_scalars(op, &Val::Bool(!a.is_empty()), other),
                    _ => a.iter().any(|i| self.compare_scalars(op, &Val::Str(self.d.text_content(*i)), other)),
                },
                (other, Val::Nodes(b)) => match other {
                    Val::Bool(_) => self.compare_scalars(op, other, &Val::Bool(!b.is_empty())),
                    _ => b.iter().any(|j| self.compare_scalars(op, other, &Val::Str(self.d.text_content(*j)))),
                },
                (a, b) => self.compare_scalars(op, a, b),
            }
        }

        /// Two node string-values: `=` / `!=` compare as strings, the relations as numbers.
        fn compare_scalars_text(&self, op: Op, l: &Val, r: &Val) -> bool {
            self.compare_scalars(op, l, r)
        }

        fn eval(&self, e: &Ex, ctx: Ctx) -> Res<Val> {
            match e {
                Ex::Num(n) => Ok(Val::Num(*n)),
                Ex::Lit(s) => Ok(Val::Str(s.clone())),
                Ex::Neg(x) => Ok(Val::Num(-self.number_of(&self.eval(x, ctx)?))),
                Ex::Union(a, b) => match (self.eval(a, ctx)?, self.eval(b, ctx)?) {
                    (Val::Nodes(mut x), Val::Nodes(y)) => {
                        x.extend(y);
                        x.sort_unstable();
                        x.dedup();
                        Ok(Val::Nodes(x))
                    }
                    _ => Err(()),
                },
                Ex::Bin(op, a, b) => match op {
                    Op::Or => {
                        let l = self.eval(a, ctx)?;
                        if self.bool_of(&l) {
                            return Ok(Val::Bool(true));
                        }
                        Ok(Val::Bool(self.bool_of(&self.eval(b, ctx)?)))
                    }
                    Op::And => {
                        let l = self.eval(a, ctx)?;
                        if !self.bool_of(&l) {
                            return Ok(Val::Bool(false));
                        }
                        Ok(Val::Bool(self.bool_of(&self.eval(b, ctx)?)))
                    }
                    Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge => {
                        let (l, r) = (self.eval(a, ctx)?, self.eval(b, ctx)?);
                        Ok(Val::Bool(self.compare(*op, &l, &r)))
                    }
                    _ => {
                        let x = self.number_of(&self.eval(a, ctx)?);
                        let y = self.number_of(&self.eval(b, ctx)?);
                        Ok(Val::Num(match op {
                            Op::Add => x + y,
                            Op::Sub => x - y,
                            Op::Mul => x * y,
                            Op::Div => x / y,
                            _ => x % y,
                        }))
                    }
                },
                Ex::Filter(base, preds) => match self.eval(base, ctx)? {
                    Val::Nodes(n) => Ok(Val::Nodes(self.apply_preds(n, preds)?)),
                    _ => Err(()),
                },
                Ex::Path { origin, absolute, steps } => self.eval_path(origin, *absolute, steps, ctx),
                Ex::Func(name, args) => self.call(name, args, ctx),
            }
        }

        fn arg_nodes(&self, args: &[Ex], i: usize, ctx: Ctx) -> Res<Vec<usize>> {
            match args.get(i) {
                None => Ok(vec![ctx.node]),
                Some(a) => match self.eval(a, ctx)? {
                    Val::Nodes(n) => Ok(n),
                    _ => Err(()),
                },
            }
        }

        fn arg_str(&self, args: &[Ex], i: usize, ctx: Ctx) -> Res<String> {
            match args.get(i) {
                None => Ok(self.d.text_content(ctx.node)),
                Some(a) => Ok(self.string_of(&self.eval(a, ctx)?)),
            }
        }

        fn call(&self, name: &str, args: &[Ex], ctx: Ctx) -> Res<Val> {
            let n = args.len();
            let num_arg = |i: usize| -> Res<f64> { Ok(self.number_of(&self.eval(&args[i], ctx)?)) };
            match name {
                "last" if n == 0 => Ok(Val::Num(ctx.size as f64)),
                "position" if n == 0 => Ok(Val::Num(ctx.pos as f64)),
                "count" if n == 1 => match self.eval(&args[0], ctx)? {
                    Val::Nodes(x) => Ok(Val::Num(x.len() as f64)),
                    _ => Err(()),
                },
                "local-name" | "name" | "namespace-uri" if n <= 1 => {
                    let nodes = self.arg_nodes(args, 0, ctx)?;
                    let s = match nodes.first() {
                        None => String::new(),
                        Some(i) => {
                            let nd = &self.d.nodes[*i];
                            match (name, nd.kind) {
                                ("namespace-uri", Kind::Elem | Kind::Attr) => nd.ns.clone().unwrap_or_default(),
                                ("namespace-uri", _) => String::new(),
                                ("name", Kind::Elem | Kind::Attr | Kind::Pi) => nd.name.clone(),
                                ("local-name", Kind::Elem | Kind::Attr) => nd.local.clone(),
                                ("local-name", Kind::Pi) => nd.name.clone(),
                                _ => String::new(),
                            }
                        }
                    };
                    Ok(Val::Str(s))
                }
                "string" if n <= 1 => Ok(Val::Str(self.arg_str(args, 0, ctx)?)),
                "concat" if n >= 2 => {
                    let mut out = String::new();
                    for a in args {
                        out.push_str(&self.string_of(&self.eval(a, ctx)?));
                    }
                    Ok(Val::Str(out))
                }
                "starts-with" if n == 2 => Ok(Val::Bool(self.arg_str(args, 0, ctx)?.starts_with(&self.arg_str(args, 1, ctx)?))),
                "contains" if n == 2 => Ok(Val::Bool(self.arg_str(args, 0, ctx)?.contains(&self.arg_str(args, 1, ctx)?))),
                "substring-before" if n == 2 => {
                    let (s, t) = (self.arg_str(args, 0, ctx)?, self.arg_str(args, 1, ctx)?);
                    Ok(Val::Str(s.find(&t).map(|p| s[..p].to_string()).unwrap_or_default()))
                }
                "substring-after" if n == 2 => {
                    let (s, t) = (self.arg_str(args, 0, ctx)?, self.arg_str(args, 1, ctx)?);
                    Ok(Val::Str(s.find(&t).map(|p| s[p + t.len()..].to_string()).unwrap_or_default()))
                }
                "substring" if n == 2 || n == 3 => {
                    let s = self.arg_str(args, 0, ctx)?;
                    let start = round_half_up(num_arg(1)?);
                    let end = if n == 3 { start + round_half_up(num_arg(2)?) } else { f64::INFINITY };
                    Ok(Val::Str(
                        s.chars()
                            .enumerate()
                            .filter(|(k, _)| {
                                let p = (*k + 1) as f64;
                                p >= start && p < end
                            })
                            .map(|(_, c)| c)
                            .collect(),
                    ))
                }
                "string-length" if n <= 1 => Ok(Val::Num(self.arg_str(args, 0, ctx)?.chars().count() as f64)),
                "normalize-space" if n <= 1 => Ok(Val::Str(self.arg_str(args, 0, ctx)?.split(is_ws).filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" "))),
                "translate" if n == 3 => {
                    let (s, from, to) = (self.arg_str(args, 0, ctx)?, self.arg_str(args, 1, ctx)?, self.arg_str(args, 2, ctx)?);
                    let (from, to): (Vec<char>, Vec<char>) = (from.chars().collect(), to.chars().collect());
                    Ok(Val::Str(
                        s.chars()
                            .filter_map(|c| match from.iter().position(|f| *f == c) {
                                None => Some(c),
                                Some(p) => to.get(p).copied(),
                            })
                            .collect(),
                    ))
                }
                "boolean" if n == 1 => Ok(Val::Bool(self.bool_of(&self.eval(&args[0], ctx)?))),
                "not" if n == 1 => Ok(Val::Bool(!self.bool_of(&self.eval(&args[0], ctx)?))),
                "true" if n == 0 => Ok(Val::Bool(true)),
                "false" if n == 0 => Ok(Val::Bool(false)),
                "number" if n <= 1 => Ok(Val::Num(match args.first() {
                    None => xpath_number(&self.d.text_content(ctx.node)),
                    Some(a) => self.number_of(&self.eval(a, ctx)?),
                })),
                "sum" if n == 1 => match self.eval(&args[0], ctx)? {
                    Val::Nodes(x) => Ok(Val::Num(x.iter().map(|i| xpath_number(&self.d.text_content(*i))).sum())),
                    _ => Err(()),
                },
                "floor" if n == 1 => Ok(Val::Num(num_arg(0)?.floor())),
                "ceiling" if n == 1 => Ok(Val::Num(num_arg(0)?.ceil())),
                "round" if n == 1 => Ok(Val::Num(round_half_up(num_arg(0)?))),
                // `id()`, `lang()` and unknown functions are errors.
                _ => Err(()),
            }
        }
    }

    /// XPath `round()`: the closest integer, halves toward +infinity.
    fn round_half_up(x: f64) -> f64 {
        if x.is_nan() || x.is_infinite() {
            x
        } else {
            (x + 0.5).floor()
        }
    }

    /// FILTERXML: the text content of every node the XPath selects, in document order.
    pub fn filter(xml: &str, xpath: &str) -> Res<Vec<String>> {
        let doc = parse(xml)?;
        // `getElementsByTagName('parsererror')` on the parsed document.
        if doc.nodes.iter().any(|n| n.kind == Kind::Elem && n.name == "parsererror") {
            return Err(());
        }
        let toks = lex(xpath)?;
        let mut p = XParser { t: toks, i: 0 };
        let ex = p.or()?;
        if p.i != p.t.len() {
            return Err(());
        }
        let ev = Eval { d: &doc };
        match ev.eval(&ex, Ctx { node: 0, pos: 1, size: 1 })? {
            Val::Nodes(n) => Ok(n.iter().map(|i| doc.text_content(*i)).collect()),
            // `document.evaluate` with a snapshot type throws for a non-node-set result.
            _ => Err(()),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn f(x: &str, p: &str) -> Result<Vec<String>, ()> {
            filter(x, p)
        }

        #[test]
        fn selects_nodes() {
            let x = "<?xml version=\"1.0\"?><r><i id=\"a\">one</i><i id=\"b\">two &amp; <b>three</b></i><!--c--></r>";
            assert_eq!(f(x, "//i"), Ok(vec!["one".into(), "two & three".into()]));
            assert_eq!(f(x, "/r/i[2]"), Ok(vec!["two & three".into()]));
            assert_eq!(f(x, "//i[@id='a']"), Ok(vec!["one".into()]));
            assert_eq!(f(x, "//i/@id"), Ok(vec!["a".into(), "b".into()]));
            assert_eq!(f(x, "//i[last()]/text()"), Ok(vec!["two & ".into()]));
            assert_eq!(f(x, "//i[contains(., 'tw')]"), Ok(vec!["two & three".into()]));
            assert_eq!(f(x, "//comment()"), Ok(vec!["c".into()]));
            assert_eq!(f(x, "//i[position() < 2] | //b"), Ok(vec!["one".into(), "three".into()]));
            assert_eq!(f(x, "//b/.."), Ok(vec!["two & three".into()]));
            assert_eq!(f(x, "//missing"), Ok(vec![]));
        }

        #[test]
        fn errors_like_the_browser() {
            assert!(f("<a>", "/a").is_err());
            assert!(f("<a/><b/>", "/a").is_err());
            assert!(f("x<a/>", "/a").is_err());
            assert!(f("<a>&nbsp;</a>", "/a").is_err());
            assert!(f("<p:a/>", "/a").is_err());
            assert!(f("<a/>", "count(//a)").is_err());
            assert!(f("<a/>", "//p:a").is_err());
            assert!(f("<a/>", "//").is_err());
            assert!(f("<parsererror/>", "/*").is_err());
            // A default namespace hides the element from an unprefixed name test.
            assert_eq!(f("<a xmlns=\"u\"><b>x</b></a>", "//b"), Ok(vec![]));
            assert_eq!(f("<a xmlns=\"u\"><b>x</b></a>", "//*[local-name()='b']"), Ok(vec!["x".into()]));
        }
    }
}
