//! Reference analysis and formula-editing helpers: a port of the web editor's `formula-refs.ts`
//! (colourised references, caret-aware parsing, syntax highlighting segments) and `formula-edit.ts`
//! (F4 cycling, auto-pairing, parenthesis matching, normalisation, "did you mean", fuzzy ranking), plus
//! `translateFormula` and `shiftRefs` of `SpreadsheetApp.tsx` (the latter written as a free function).
//!
//! **Positions are UTF-16 code-unit indexes**, exactly like the JavaScript `string` indexes the web
//! passes around (`start`/`end` of a reference, `caret`, the `a`/`b` of a parenthesis match, the
//! `caret` returned by `apply_f4`/`auto_pair`). The functions take `&str` and convert internally, so a
//! caret reported by a UTF-16 text control can be used as is; characters outside the BMP count for 2.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use regex::Regex;

use crate::{addr, js};

/// A lazily compiled static regex (`None` if the pattern were invalid: callers degrade, never panic).
macro_rules! re {
    ($pat:expr) => {{
        static R: OnceLock<Option<Regex>> = OnceLock::new();
        R.get_or_init(|| Regex::new($pat).ok()).as_ref()
    }};
}

// ── UTF-16 helpers ──────────────────────────────────────────────────────────────

fn u16s(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn text_of(u: &[u16]) -> String {
    js::from_utf16(u)
}

/// `text.slice(a, b)` for non-negative indexes (clamped).
fn slice16(u: &[u16], a: usize, b: usize) -> String {
    let b = b.min(u.len());
    let a = a.min(b);
    text_of(&u[a..b])
}

/// Byte offset → UTF-16 offset table of `s` (length `s.len() + 1`).
fn offsets(s: &str) -> Vec<usize> {
    let mut v = vec![0usize; s.len() + 1];
    let mut u = 0usize;
    for (b, c) in s.char_indices() {
        for k in 0..c.len_utf8() {
            v[b + k] = u;
        }
        u += c.len_utf16();
    }
    v[s.len()] = u;
    v
}

fn is_letter(c: u16) -> bool {
    (b'A' as u16..=b'Z' as u16).contains(&c) || (b'a' as u16..=b'z' as u16).contains(&c)
}

/// `/[A-Za-z0-9_]/`.
fn is_word_unit(c: u16) -> bool {
    is_letter(c) || (b'0' as u16..=b'9' as u16).contains(&c) || c == b'_' as u16
}

fn is_digit_unit(c: u16) -> bool {
    (b'0' as u16..=b'9' as u16).contains(&c)
}

const QUOTE: u16 = b'"' as u16;
const LPAREN: u16 = b'(' as u16;
const RPAREN: u16 = b')' as u16;

/// `stringMask` / `strMask`: true on every unit inside a `"…"` string, quotes included.
fn string_mask(u: &[u16]) -> Vec<bool> {
    let mut mask = vec![false; u.len()];
    let mut open = false;
    for (i, &c) in u.iter().enumerate() {
        if c == QUOTE {
            open = !open;
            mask[i] = true;
        } else {
            mask[i] = open;
        }
    }
    mask
}

// ═══════════════════════════════════════════════════════════════════════════════
// formula-refs.ts
// ═══════════════════════════════════════════════════════════════════════════════

/// A reference found in a formula.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormulaRef {
    /// Start index in the formula (UTF-16).
    pub start: usize,
    /// End index, exclusive (UTF-16).
    pub end: usize,
    /// Raw text (`C7:C16`, `$A$1`).
    pub text: String,
    /// Uniqueness key (upper-case, no `$`): the same reference always gets the same colour.
    pub normalized: String,
    pub color: &'static str,
}

/// Distinct, readable palette (≠ the grid's selection colours).
pub const REF_PALETTE: [&str; 8] = ["#1a73e8", "#d93025", "#188038", "#9334e6", "#e8710a", "#12b5cb", "#c5221f", "#7cb342"];

/// Full A1 references: optional sheet prefix, cells/ranges (1–3 letters, 1–7 digits) AND whole
/// columns/rows (`A:A`, `$A:$C`, `1:1`, `2:5`). (`\d` is ASCII in JavaScript: written `[0-9]`.)
const REF_RE: &str = r"(?:(?:'[^']*'|[A-Za-z_][A-Za-z0-9_.]*)!)?(?:\$?[A-Za-z]{1,3}\$?[0-9]{1,7}(?::\$?[A-Za-z]{1,3}\$?[0-9]{1,7})?|\$?[A-Za-z]{1,3}:\$?[A-Za-z]{1,3}|\$?[0-9]{1,7}:\$?[0-9]{1,7})";

/// `A1_RE` of `formula-edit.ts` as a regex source (exported for completeness; `\d` written `[0-9]`).
pub const A1_RE: &str = r"(?:'[^']+'|[A-Za-z_][A-Za-z0-9_.]*)?!?\$?[A-Za-z]{1,3}\$?[0-9]{1,7}(?::\$?[A-Za-z]{1,3}\$?[0-9]{1,7})?";

/// `parseRefs`: the references of a formula (starting with `=`), outside strings, with a colour stable
/// per distinct reference (first occurrence = first palette colour, and so on).
pub fn parse_refs(formula: &str) -> Vec<FormulaRef> {
    if !formula.starts_with('=') {
        return Vec::new();
    }
    let Some(re) = re!(REF_RE) else { return Vec::new() };
    let u = u16s(formula);
    let mask = string_mask(&u);
    let off = offsets(formula);
    let mut out = Vec::new();
    let mut color_by_norm: HashMap<String, &'static str> = HashMap::new();
    for m in re.find_iter(formula) {
        let start = off[m.start()];
        let end = off[m.end()];
        if mask[start] {
            continue; // inside a string
        }
        if start > 0 && is_word_unit(u[start - 1]) {
            continue; // suffix of an identifier (LOG10)
        }
        if let Some(&after) = u.get(end) {
            if after == LPAREN {
                continue; // a function name
            }
            if is_letter(after) || after == b'_' as u16 {
                continue; // followed by letters: not a pure reference
            }
        }
        let normalized = js::upper(&m.as_str().replace('$', ""));
        let color = match color_by_norm.get(&normalized) {
            Some(c) => c,
            None => {
                let c = REF_PALETTE[color_by_norm.len() % REF_PALETTE.len()];
                color_by_norm.insert(normalized.clone(), c);
                c
            }
        };
        out.push(FormulaRef { start, end, text: m.as_str().to_string(), normalized, color });
    }
    out
}

/// Bounds of a reference: 0-based columns, 1-based rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellBounds {
    pub c1: i64,
    pub r1: i64,
    pub c2: i64,
    pub r2: i64,
}

const COL_MAX: i64 = 16383;
const ROW_MAX: i64 = 1_048_576;

/// `colIdx` of `formula-refs.ts` (the upper-case letters `A`… give `ch − 64`).
fn col_idx(s: &str) -> i64 {
    let mut c: i64 = 0;
    for ch in s.chars() {
        c = c * 26 + (ch as i64 - 64);
    }
    c - 1
}

/// `refBounds`: `"C7:C16"` → `{c1:2, r1:7, c2:2, r2:16}`; a sheet prefix is ignored; whole columns and
/// whole rows are supported; `None` when invalid.
pub fn ref_bounds(r: &str) -> Option<CellBounds> {
    let no_sheet = match r.rfind('!') {
        Some(i) => &r[i + 1..],
        None => r,
    };
    let u = js::upper(&no_sheet.replace('$', ""));
    if let Some(re) = re!(r"^([A-Z]{1,3}):([A-Z]{1,3})$") {
        if let Some(w) = re.captures(&u) {
            let a = col_idx(&w[1]);
            let b = col_idx(&w[2]);
            return Some(CellBounds { c1: a.min(b), r1: 1, c2: a.max(b), r2: ROW_MAX });
        }
    }
    if let Some(re) = re!(r"^([0-9]{1,7}):([0-9]{1,7})$") {
        if let Some(w) = re.captures(&u) {
            let a: i64 = w[1].parse().ok()?;
            let b: i64 = w[2].parse().ok()?;
            return Some(CellBounds { c1: 0, r1: a.min(b), c2: COL_MAX, r2: a.max(b) });
        }
    }
    let one = |s: &str| -> Option<(i64, i64)> {
        let re = re!(r"^([A-Z]{1,3})([0-9]{1,7})$")?;
        let m = re.captures(s)?;
        let mut c: i64 = 0;
        for ch in m[1].chars() {
            c = c * 26 + (ch as i64 - 64);
        }
        Some((c - 1, m[2].parse().ok()?))
    };
    let mut parts = u.split(':');
    let a = one(parts.next().unwrap_or(""))?;
    let b = match parts.next() {
        Some(p) if !p.is_empty() => one(p)?,
        _ => a,
    };
    Some(CellBounds { c1: a.0.min(b.0), r1: a.1.min(b.1), c2: a.0.max(b.0), r2: a.1.max(b.1) })
}

/// `nameTokenAt`: the partial function name just before the caret (for name autocompletion), upper-cased;
/// empty when none. `caret` is a UTF-16 index (a caret past the end behaves as the end, as in JavaScript
/// where `undefined` is "a letter" but the slice stays empty past the text).
pub fn name_token_at(text: &str, caret: usize) -> String {
    if !text.starts_with('=') {
        return String::new();
    }
    let u = u16s(text);
    if u.get(caret) == Some(&LPAREN) {
        return String::new(); // already a call
    }
    let mut i = caret.min(u.len());
    while i > 0 && is_letter(u[i - 1]) {
        i -= 1;
    }
    let token = &u[i..caret.min(u.len())];
    if !token.is_empty() && token.iter().all(|&c| is_letter(c)) {
        js::upper(&text_of(token))
    } else {
        String::new()
    }
}

/// The function whose parentheses contain the caret, with the index of the current argument.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArgContext {
    pub name: String,
    pub arg_index: usize,
}

/// `argContextAt`: the innermost *named* function around the caret and the current argument index
/// (`,` and `;` advance it; strings are skipped).
pub fn arg_context_at(text: &str, caret: usize) -> Option<ArgContext> {
    if !text.starts_with('=') {
        return None;
    }
    let u = u16s(text);
    let mut stack: Vec<(String, usize)> = Vec::new();
    let mut i = 0usize;
    let mut in_str = false;
    while i < caret && i < u.len() {
        let ch = u[i];
        if in_str {
            if ch == QUOTE {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if ch == QUOTE {
            in_str = true;
            i += 1;
            continue;
        }
        if is_letter(ch) {
            let mut j = i;
            while j < u.len() && is_word_unit(u[j]) {
                j += 1;
            }
            if u.get(j) == Some(&LPAREN) {
                stack.push((js::upper(&slice16(&u, i, j)), 0));
                i = j + 1;
                continue;
            }
            i = j;
            continue;
        }
        if ch == LPAREN {
            stack.push((String::new(), 0));
            i += 1;
            continue;
        }
        if ch == RPAREN {
            stack.pop();
            i += 1;
            continue;
        }
        if (ch == b',' as u16 || ch == b';' as u16) && !stack.is_empty() {
            if let Some(top) = stack.last_mut() {
                top.1 += 1;
            }
            i += 1;
            continue;
        }
        i += 1;
    }
    for (name, arg) in stack.iter().rev() {
        if !name.is_empty() {
            return Some(ArgContext { name: name.clone(), arg_index: *arg });
        }
    }
    None
}

/// One argument of a function syntax.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArgSpec {
    pub name: String,
    pub optional: bool,
    pub repeat: bool,
}

/// `parseSyntaxArgs`: the arguments of a syntax string such as `"FLOOR(nombre, [multiple])"`.
pub fn parse_syntax_args(syntax: &str) -> Vec<ArgSpec> {
    let Some(re) = re!(r"\(([^)]*)\)") else { return Vec::new() };
    let Some(m) = re.captures(syntax) else { return Vec::new() };
    let inner = &m[1];
    if js::trim(inner).is_empty() {
        return Vec::new();
    }
    let mut args: Vec<ArgSpec> = Vec::new();
    for raw in inner.split(',').map(js::trim) {
        if raw == "..." || raw == "…" {
            if let Some(last) = args.last_mut() {
                last.repeat = true;
            }
            continue;
        }
        let repeat = raw.contains("...") || raw.contains('…');
        let t = js::trim(&raw.replace("...", "").replace('…', "")).to_string();
        let optional = t.starts_with('[');
        let t = t.strip_prefix('[').unwrap_or(&t);
        let t = t.strip_suffix(']').unwrap_or(t);
        let t = js::trim(t);
        if !t.is_empty() {
            args.push(ArgSpec { name: t.to_string(), optional, repeat });
        }
    }
    args
}

/// Rainbow parenthesis colours by nesting depth.
pub const PAREN_COLORS: [&str; 6] = ["#1a73e8", "#e8710a", "#188038", "#9334e6", "#12b5cb", "#c5221f"];
const STRING_COLOR: &str = "#188038";
const ERROR_COLOR: &str = "#d93025";
const FUNCTION_COLOR: &str = "#8e24aa";
const KEYWORD_COLOR: &str = "#00838f";
const NAME_COLOR: &str = "#b06000";
const OPERATOR_COLOR: &str = "#5f6368";

/// A run of formula text with one colour (`None` = default) and an optional wavy error underline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub text: String,
    pub color: Option<&'static str>,
    pub wavy: bool,
}

/// `colorSegments`: the formula split into coloured runs (references with their stable colour, rainbow
/// parentheses, green strings, wavy errors for unmatched parentheses / unterminated strings / unknown
/// functions, keywords, named ranges, operators). `known_fns` (upper-case names) enables the function
/// highlighting and unknown-function detection; `names` (upper-case) enables named-range colouring.
pub fn color_segments(formula: &str, known_fns: Option<&HashSet<String>>, names: Option<&HashSet<String>>) -> Vec<Segment> {
    if !formula.starts_with('=') {
        return vec![Segment { text: formula.to_string(), color: None, wavy: false }];
    }
    let u = u16s(formula);
    let n = u.len();
    let off = offsets(formula);
    let mut color: Vec<Option<&'static str>> = vec![None; n];
    let mut wavy = vec![false; n];

    // 1. Strings (green) + mask; an unterminated string is a wavy error.
    let mut in_str = vec![false; n];
    let mut open = false;
    for i in 0..n {
        if u[i] == QUOTE {
            open = !open;
            in_str[i] = true;
            color[i] = Some(STRING_COLOR);
            continue;
        }
        in_str[i] = open;
        if open {
            color[i] = Some(STRING_COLOR);
        }
    }
    if open {
        for i in 0..n {
            if in_str[i] {
                wavy[i] = true;
            }
        }
    }

    // 2. References (stable colour per reference).
    for r in parse_refs(formula) {
        for c in color.iter_mut().take(r.end).skip(r.start) {
            *c = Some(r.color);
        }
    }

    // 3. Rainbow parentheses + unmatched ones (outside strings).
    let mut stack: Vec<usize> = Vec::new();
    for i in 0..n {
        if in_str[i] {
            continue;
        }
        if u[i] == LPAREN {
            color[i] = Some(PAREN_COLORS[stack.len() % PAREN_COLORS.len()]);
            stack.push(i);
        } else if u[i] == RPAREN {
            match stack.pop() {
                Some(o) => color[i] = color[o],
                None => {
                    color[i] = Some(ERROR_COLOR);
                    wavy[i] = true;
                }
            }
        }
    }
    for &i in &stack {
        color[i] = Some(ERROR_COLOR);
        wavy[i] = true;
    }

    // 4. Function names: known → purple; unknown → wavy.
    if let Some(known) = known_fns {
        if let Some(re) = re!(r"[A-Za-z][A-Za-z0-9_.]*\(") {
            for m in re.find_iter(formula) {
                let s = off[m.start()];
                let e = off[m.end() - 1]; // the name without its `(` (the lookahead of the web)
                if in_str[s] {
                    continue;
                }
                let name = &formula[m.start()..m.end() - 1];
                let is_known = known.contains(&js::upper(name));
                for i in s..e {
                    if is_known {
                        if color[i].is_none() {
                            color[i] = Some(FUNCTION_COLOR);
                        }
                    } else {
                        wavy[i] = true;
                    }
                }
            }
        }
    }

    // 5. TRUE/FALSE/VRAI/FAUX keywords (outside strings, not followed by `(`).
    if let Some(re) = re!(r"(?-u:\b)(?:[Tt][Rr][Uu][Ee]|[Ff][Aa][Ll][Ss][Ee]|[Vv][Rr][Aa][Ii]|[Ff][Aa][Uu][Xx])(?-u:\b)") {
        for m in re.find_iter(formula) {
            let s = off[m.start()];
            let e = off[m.end()];
            if in_str[s] || u.get(e) == Some(&LPAREN) {
                continue;
            }
            for c in color.iter_mut().take(e).skip(s) {
                if c.is_none() {
                    *c = Some(KEYWORD_COLOR);
                }
            }
        }
    }

    // 6. Named ranges (an identifier not followed by `(`, present in `names`).
    if let Some(names) = names {
        if !names.is_empty() {
            if let Some(re) = re!(r"[A-Za-z_][A-Za-z0-9_.]*") {
                for m in re.find_iter(formula) {
                    let s = off[m.start()];
                    let e = off[m.end()];
                    if in_str[s] || u.get(e) == Some(&LPAREN) {
                        continue;
                    }
                    if color[s].is_some() {
                        continue; // already coloured (reference/function)
                    }
                    if names.contains(&js::upper(m.as_str())) {
                        for c in color.iter_mut().take(e).skip(s) {
                            if c.is_none() {
                                *c = Some(NAME_COLOR);
                            }
                        }
                    }
                }
            }
        }
    }

    // 7. Operators (outside strings).
    for i in 0..n {
        if in_str[i] || color[i].is_some() {
            continue;
        }
        if matches!(u[i], 0x2b | 0x2d | 0x2a | 0x2f | 0x5e | 0x26 | 0x3c | 0x3e | 0x3d | 0x25) {
            color[i] = Some(OPERATOR_COLOR);
        }
    }

    // Group consecutive units of the same (colour, wavy).
    let mut segs = Vec::new();
    let mut i = 0usize;
    while i < n {
        let c = color[i];
        let w = wavy[i];
        let mut j = i + 1;
        while j < n && color[j] == c && wavy[j] == w {
            j += 1;
        }
        segs.push(Segment { text: slice16(&u, i, j), color: c, wavy: w });
        i = j;
    }
    segs
}

// ═══════════════════════════════════════════════════════════════════════════════
// formula-edit.ts
// ═══════════════════════════════════════════════════════════════════════════════

/// `cycleAbsolute`: `A1 → $A$1 → A$1 → $A1 → A1` (Excel's F4 order); anything else is returned as is.
pub fn cycle_absolute(cell: &str) -> String {
    let Some(re) = re!(r"^(\$?)([A-Za-z]{1,3})(\$?)([0-9]{1,7})$") else { return cell.to_string() };
    let Some(m) = re.captures(cell) else { return cell.to_string() };
    let col = &m[2];
    let row = &m[4];
    let state = (if m[1].is_empty() { 0 } else { 1 }) + (if m[3].is_empty() { 0 } else { 2 });
    // Excel's order: (0,0) → (1,1) → (0,1) → (1,0) → (0,0).
    let next = match state {
        0 => 3,
        3 => 2,
        2 => 1,
        _ => 0,
    };
    format!("{}{}{}{}", if next & 1 != 0 { "$" } else { "" }, col, if next & 2 != 0 { "$" } else { "" }, row)
}

/// A reference found under the caret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefSpan {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

/// `refAtCaret`: the cell reference under (or just before) the caret, for F4.
pub fn ref_at_caret(text: &str, caret: usize) -> Option<RefSpan> {
    let re = re!(r"\$?[A-Za-z]{1,3}\$?[0-9]{1,7}")?;
    let u = u16s(text);
    let off = offsets(text);
    for m in re.find_iter(text) {
        let s = off[m.start()];
        let e = off[m.end()];
        if s > 0 && is_word_unit(u[s - 1]) {
            continue; // identifier suffix
        }
        if caret >= s && caret <= e {
            return Some(RefSpan { start: s, end: e, text: m.as_str().to_string() });
        }
    }
    None
}

/// `applyF4`: cycles the reference under the caret; returns the new text and the new caret (UTF-16).
pub fn apply_f4(text: &str, caret: usize) -> Option<(String, usize)> {
    let r = ref_at_caret(text, caret)?;
    let cycled = cycle_absolute(&r.text);
    let u = u16s(text);
    let out = format!("{}{}{}", slice16(&u, 0, r.start), cycled, slice16(&u, r.end, u.len()));
    Some((out, r.start + js::len16(&cycled)))
}

/// `autoPair`: typing `ch` inserts the closing partner, or "types over" an existing closer. `None` =
/// insert the character normally. The caret returned is a UTF-16 index.
pub fn auto_pair(text: &str, caret: usize, ch: &str) -> Option<(String, usize)> {
    let u = u16s(text);
    let next = u.get(caret).copied();
    let with = |ins: &str| format!("{}{}{}", slice16(&u, 0, caret), ins, slice16(&u, caret, u.len()));
    if ch == "(" {
        return Some((with("()"), caret + 1));
    }
    if ch == "\"" {
        if next == Some(QUOTE) {
            return Some((text.to_string(), caret + 1)); // type over
        }
        return Some((with("\"\""), caret + 1));
    }
    if ch == ")" && next == Some(RPAREN) {
        return Some((text.to_string(), caret + 1)); // type over
    }
    None
}

/// `matchParen`: indexes `(a, b)` (UTF-16) of the parenthesis pair next to the caret, or `None`.
pub fn match_paren(text: &str, caret: usize) -> Option<(usize, usize)> {
    let u = u16s(text);
    let mask = string_mask(&u);
    let at = |i: i64| -> u16 {
        if i >= 0 && (i as usize) < u.len() && !mask[i as usize] {
            u[i as usize]
        } else {
            0
        }
    };
    let c = caret as i64;
    let (pos, dir): (i64, i64) = if at(c) == LPAREN {
        (c, 1)
    } else if at(c - 1) == RPAREN {
        (c - 1, -1)
    } else if at(c) == RPAREN {
        (c, -1)
    } else if at(c - 1) == LPAREN {
        (c - 1, 1)
    } else {
        return None;
    };
    let mut depth = 0i64;
    let mut i = pos;
    while i >= 0 && (i as usize) < u.len() {
        let iu = i as usize;
        if mask[iu] {
            i += dir;
            continue;
        }
        if u[iu] == LPAREN {
            depth += if dir > 0 { 1 } else { -1 };
        } else if u[iu] == RPAREN {
            depth += if dir > 0 { -1 } else { 1 };
        }
        if depth == 0 {
            return Some((pos.min(i) as usize, pos.max(i) as usize));
        }
        i += dir;
    }
    None
}

/// `parenBalance`: `(missing_close, missing_open)` outside strings.
pub fn paren_balance(text: &str) -> (usize, usize) {
    let u = u16s(text);
    let mask = string_mask(&u);
    let mut depth = 0usize;
    let mut missing_open = 0usize;
    for (i, &c) in u.iter().enumerate() {
        if mask[i] {
            continue;
        }
        if c == LPAREN {
            depth += 1;
        } else if c == RPAREN {
            if depth > 0 {
                depth -= 1;
            } else {
                missing_open += 1;
            }
        }
    }
    (depth, missing_open)
}

/// `autoBalance`: appends the missing `)` at the end.
pub fn auto_balance(text: &str) -> String {
    let (missing_close, _) = paren_balance(text);
    if missing_close > 0 {
        format!("{}{}", text, ")".repeat(missing_close))
    } else {
        text.to_string()
    }
}

/// `normalizeFormula`: upper-cases function names and reference column letters outside strings, as
/// Excel does on validation. Mirrors `/\b[A-Za-z][A-Za-z0-9_.]*(?=\()|\b[A-Za-z]{1,3}(?=\$?\d)/g`
/// (an ASCII word boundary, greedy runs, look-aheads) without a regex engine.
pub fn normalize_formula(text: &str) -> String {
    if !text.starts_with('=') {
        return text.to_string();
    }
    let u = u16s(text);
    let mask = string_mask(&u);
    let n = u.len();
    let mut out: Vec<u16> = Vec::with_capacity(n);
    let mut p = 0usize;
    while p < n {
        let mut matched_end: Option<usize> = None;
        if is_letter(u[p]) && (p == 0 || !is_word_unit(u[p - 1])) {
            // Alternative 1: name followed by `(`.
            let mut r = p + 1;
            while r < n && (is_word_unit(u[r]) || u[r] == b'.' as u16) {
                r += 1;
            }
            if u.get(r) == Some(&LPAREN) {
                matched_end = Some(r);
            } else {
                // Alternative 2: 1–3 letters followed by `$?\d` (greedy, backing off).
                let mut k = 0usize;
                while k < 3 && p + k < n && is_letter(u[p + k]) {
                    k += 1;
                }
                while k >= 1 {
                    let q = p + k;
                    let mut look = q;
                    if u.get(look) == Some(&(b'$' as u16)) {
                        look += 1;
                    }
                    let ok_with_dollar = u.get(look).is_some_and(|&c| is_digit_unit(c));
                    let ok_without = u.get(q).is_some_and(|&c| is_digit_unit(c));
                    if ok_with_dollar || ok_without {
                        matched_end = Some(q);
                        break;
                    }
                    k -= 1;
                }
            }
        }
        match matched_end {
            Some(e) => {
                if mask[p] {
                    out.extend_from_slice(&u[p..e]);
                } else {
                    out.extend(u[p..e].iter().map(|&c| if (b'a' as u16..=b'z' as u16).contains(&c) { c - 32 } else { c }));
                }
                p = e;
            }
            None => {
                out.push(u[p]);
                p += 1;
            }
        }
    }
    text_of(&out)
}

/// `ERROR_HELP`: the explanation of each error code (in the order of the web's object).
pub const ERROR_HELP: [(&str, &str); 10] = [
    ("#DIV/0!", "Division par zéro : le diviseur est 0 ou une cellule vide."),
    ("#N/A", "Valeur non disponible : une recherche (RECHERCHEV/EQUIV…) n’a rien trouvé."),
    ("#NAME?", "Nom non reconnu : nom de fonction mal orthographié, plage nommée inexistante ou texte sans guillemets."),
    ("#NULL!", "Intersection vide : les deux plages ne se croisent pas."),
    ("#NUM!", "Nombre non valide : argument numérique hors limites ou calcul impossible."),
    ("#REF!", "Référence non valide : une cellule référencée a été supprimée."),
    ("#VALUE!", "Type incorrect : du texte est utilisé là où un nombre est attendu."),
    ("#SPILL!", "Plage de déversement bloquée : une cellule sur le trajet du résultat n’est pas vide."),
    ("#CALC!", "Erreur de calcul (tableau vide ou opération non prise en charge)."),
    ("#ERROR!", "Formule mal formée."),
];

/// `errorHelp`: the explanation of an error code present at the start of a cell value.
pub fn error_help(value: &str) -> Option<&'static str> {
    ERROR_HELP.iter().find(|(c, _)| value.starts_with(c)).map(|(_, h)| *h)
}

/// `levenshtein` over UTF-16 code units, as the web's `a[i]`/`a.length`.
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a = u16s(a);
    let b = u16s(b);
    let (m, n) = (a.len(), b.len());
    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }
    let mut prev: Vec<usize> = (0..=n).collect();
    for i in 1..=m {
        let mut cur = vec![i; n + 1];
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        prev = cur;
    }
    prev[n]
}

/// `didYouMean`: the closest known function name to an unknown one, or `None` when too far.
pub fn did_you_mean<S: AsRef<str>>(name: &str, known: &[S]) -> Option<String> {
    let up = js::upper(name);
    let mut best: Option<&str> = None;
    let mut best_d = usize::MAX;
    for k in known {
        let d = levenshtein(&up, k.as_ref());
        if d < best_d {
            best_d = d;
            best = Some(k.as_ref());
        }
    }
    let limit = 1usize.max(js::len16(&up) / 3);
    match best {
        Some(b) if !b.is_empty() && best_d <= limit => Some(b.to_string()),
        _ => None,
    }
}

/// `fuzzyScore`: subsequence score of `name` against `query` with a prefix bonus; `-1` = no match.
pub fn fuzzy_score(query: &str, name: &str) -> f64 {
    let q = u16s(&js::upper(query));
    let s = u16s(&js::upper(name));
    if q.is_empty() {
        return 0.0;
    }
    if s.starts_with(&q) {
        return 1000.0 - s.len() as f64; // prefix = best
    }
    let (mut qi, mut score, mut run) = (0usize, 0.0f64, 0.0f64);
    let mut i = 0usize;
    while i < s.len() && qi < q.len() {
        if s[i] == q[qi] {
            qi += 1;
            run += 1.0;
            score += 1.0 + run;
        } else {
            run = 0.0;
        }
        i += 1;
    }
    if qi < q.len() {
        return -1.0; // not a subsequence
    }
    if s.windows(q.len()).any(|w| w == &q[..]) {
        score += 50.0; // contiguous substring
    }
    score - s.len() as f64 / 100.0
}

/// `rankFunctions`: ranks `list` by fuzzy relevance to `query` (non-matches dropped), best first, at most
/// `limit` items (the web's default is 12). `name` extracts the name of an item. The sort is stable, as
/// JavaScript's.
pub fn rank_functions<'a, T>(query: &str, list: &'a [T], limit: usize, name: impl Fn(&T) -> &str) -> Vec<&'a T> {
    if query.is_empty() {
        return list.iter().take(limit).collect();
    }
    let mut scored: Vec<(&T, f64)> = list.iter().map(|f| (f, fuzzy_score(query, name(f)))).filter(|x| x.1 >= 0.0).collect();
    scored.sort_by(|a, b| (b.1 - a.1).partial_cmp(&0.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().take(limit).map(|x| x.0).collect()
}

/// A part of a split signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SigPart {
    pub text: String,
    pub active: bool,
}

/// `splitSignature`: cuts `"IF(condition, si_vrai, si_faux)"` into parts, marking argument `arg_index`
/// active (a repeated `...` argument stays active beyond). Returns `(function name, parts)`.
pub fn split_signature(syntax: &str, arg_index: usize) -> (String, Vec<SigPart>) {
    let Some(re) = re!(r"^([^(]*)\(([^)]*)\)") else { return (syntax.to_string(), Vec::new()) };
    let Some(m) = re.captures(syntax) else { return (syntax.to_string(), Vec::new()) };
    let args: Vec<&str> = m[2].split(',').map(js::trim).filter(|s| !s.is_empty()).collect();
    let repeat_at = args.iter().position(|a| a.contains("...") || a.contains('…'));
    let parts = args
        .iter()
        .enumerate()
        .map(|(i, a)| SigPart {
            text: (*a).to_string(),
            active: i == arg_index || repeat_at.is_some_and(|r| i >= r && arg_index >= r),
        })
        .collect();
    (m[1].to_string(), parts)
}

// ═══════════════════════════════════════════════════════════════════════════════
// SpreadsheetApp.tsx: translateFormula / shiftRefs
// ═══════════════════════════════════════════════════════════════════════════════

const CELL_REF_RE: &str = r"(\$?)([A-Za-z]{1,3})(\$?)([0-9]+)";

/// `translateFormula`: shifts the relative references of a formula by `(d_col, d_row)` — for copy/paste
/// and fill (`$`-anchored parts stay). A reference pushed off the grid becomes `#REF!`.
///
/// QUIRK: like the web it rewrites anything that looks like a reference, inside strings and inside
/// function names too (`LOG10`, `ATAN2` → `TAN2` is shifted).
pub fn translate_formula(f: &str, d_col: i64, d_row: i64) -> String {
    let Some(re) = re!(CELL_REF_RE) else { return f.to_string() };
    re.replace_all(f, |caps: &regex::Captures| {
        let (ad, col, ar, row) = (&caps[1], &caps[2], &caps[3], &caps[4]);
        let c = addr::col_to_index(&js::upper(col)) + if ad.is_empty() { d_col } else { 0 };
        let r = js::parse_int(row, 10) + if ar.is_empty() { d_row as f64 } else { 0.0 };
        if c < 0 || r < 1.0 {
            return "#REF!".to_string();
        }
        format!("{}{}{}{}", ad, addr::index_to_col(c), ar, js::to_string(r))
    })
    .into_owned()
}

/// The axis of a structural edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Row,
    Col,
}

/// `shiftRefs` (inside the web's component): shifts the references of a formula when a band is inserted
/// (`delta > 0`) or deleted (`delta < 0`) on `axis`. `at`: row axis = 1-based row; column axis = 0-based
/// column index. A reference inside a deleted band becomes `#REF!`.
pub fn shift_refs(f: &str, axis: Axis, at: i64, delta: i64) -> String {
    let Some(re) = re!(CELL_REF_RE) else { return f.to_string() };
    re.replace_all(f, |caps: &regex::Captures| {
        let (ad, col, ar, row) = (&caps[1], &caps[2], &caps[3], &caps[4]);
        let mut c = addr::col_to_index(&js::upper(col));
        let mut r = js::parse_int(row, 10);
        match axis {
            Axis::Col => {
                if delta < 0 && c >= at && c < at - delta {
                    return "#REF!".to_string();
                }
                if c >= at {
                    c += delta;
                }
            }
            Axis::Row => {
                if delta < 0 && r >= at as f64 && r < (at - delta) as f64 {
                    return "#REF!".to_string();
                }
                if r >= at as f64 {
                    r += delta as f64;
                }
            }
        }
        if c < 0 || r < 1.0 {
            return "#REF!".to_string();
        }
        format!("{}{}{}{}", ad, addr::index_to_col(c), ar, js::to_string(r))
    })
    .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_refs_colours_and_skips() {
        let r = parse_refs("=SUM(A1:B2)+a1+LOG10(C3)+\"D4\"+Sheet1!E5+'My Sheet'!F6");
        let texts: Vec<&str> = r.iter().map(|x| x.text.as_str()).collect();
        assert_eq!(texts, vec!["A1:B2", "a1", "C3", "Sheet1!E5", "'My Sheet'!F6"]);
        assert_eq!(r[0].color, REF_PALETTE[0]);
        assert_eq!(r[1].color, REF_PALETTE[1]);
        assert_eq!(r[1].normalized, "A1");
        assert!(parse_refs("A1").is_empty());
        // Whole columns / rows.
        let r = parse_refs("=SUM(A:A)+SUM(2:5)");
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn utf16_positions() {
        // "é" is one UTF-16 unit, the emoji two.
        let r = parse_refs("=é+😀+A1");
        assert_eq!(r.len(), 1);
        assert_eq!((r[0].start, r[0].end), (6, 8));
        assert_eq!(name_token_at("=😀SU", 5), "SU");
        assert_eq!(match_paren("=(😀)", 1), Some((1, 4)));
        let (t, c) = apply_f4("=😀+A1", 5).unwrap();
        assert_eq!((t.as_str(), c), ("=😀+$A$1", 8));
    }

    #[test]
    fn bounds() {
        assert_eq!(ref_bounds("C7:C16"), Some(CellBounds { c1: 2, r1: 7, c2: 2, r2: 16 }));
        assert_eq!(ref_bounds("'S'!$B$2"), Some(CellBounds { c1: 1, r1: 2, c2: 1, r2: 2 }));
        assert_eq!(ref_bounds("C:A"), Some(CellBounds { c1: 0, r1: 1, c2: 2, r2: ROW_MAX }));
        assert_eq!(ref_bounds("5:2"), Some(CellBounds { c1: 0, r1: 2, c2: COL_MAX, r2: 5 }));
        assert_eq!(ref_bounds("A1:"), Some(CellBounds { c1: 0, r1: 1, c2: 0, r2: 1 }));
        assert_eq!(ref_bounds("hello"), None);
    }

    #[test]
    fn f4_cycle() {
        assert_eq!(cycle_absolute("A1"), "$A$1");
        assert_eq!(cycle_absolute("$A$1"), "A$1");
        assert_eq!(cycle_absolute("A$1"), "$A1");
        assert_eq!(cycle_absolute("$A1"), "A1");
        assert_eq!(cycle_absolute("A1:B2"), "A1:B2");
    }

    #[test]
    fn pairing_and_balance() {
        assert_eq!(auto_pair("=SUM", 4, "("), Some(("=SUM()".to_string(), 5)));
        assert_eq!(auto_pair("=\"\"", 1, "\""), Some(("=\"\"".to_string(), 2)));
        assert_eq!(auto_pair("=(1)", 3, ")"), Some(("=(1)".to_string(), 4)));
        assert_eq!(auto_pair("=1", 2, "x"), None);
        assert_eq!(paren_balance("=((1)))"), (0, 1));
        assert_eq!(auto_balance("=SUM((1"), "=SUM((1))");
        assert_eq!(match_paren("=(1+(2))", 1), Some((1, 7)));
        assert_eq!(match_paren("=(1+(2))", 0), None);
    }

    #[test]
    fn normalize() {
        assert_eq!(normalize_formula("=sum(a1:b2)+\"abc1\"+x"), "=SUM(A1:B2)+\"abc1\"+x");
        assert_eq!(normalize_formula("=abcd1+ab12"), "=abcd1+AB12");
        assert_eq!(normalize_formula("=if(a$1>0,b1,c$2)"), "=IF(A$1>0,B1,C$2)");
        assert_eq!(normalize_formula("hello(a1)"), "hello(a1)");
        assert_eq!(normalize_formula("=log10(x)"), "=LOG10(x)");
    }

    #[test]
    fn translate_and_shift() {
        assert_eq!(translate_formula("=A1+$B$2+C$3+$D4", 1, 2), "=B3+$B$2+D$3+$D6");
        assert_eq!(translate_formula("=A1", -1, 0), "=#REF!");
        assert_eq!(translate_formula("=A2", 0, -1), "=A1");
        assert_eq!(translate_formula("=A2", 0, -2), "=#REF!");
        assert_eq!(translate_formula("=LOG10(A1)", 1, 1), "=LOH11(B2)");
        assert_eq!(shift_refs("=A1+B5", Axis::Row, 3, 2), "=A1+B7");
        assert_eq!(shift_refs("=A1+B5", Axis::Row, 5, -1), "=A1+#REF!");
        assert_eq!(shift_refs("=A1+C1", Axis::Col, 1, 1), "=A1+D1");
        assert_eq!(shift_refs("=A1+B1+C1", Axis::Col, 1, -2), "=A1+#REF!+#REF!");
    }


    #[test]
    fn translate_details() {
        // The column is upper-cased; whole-sheet anchors stay.
        assert_eq!(translate_formula("=a1+B2", 1, 1), "=B2+C3");
        assert_eq!(translate_formula("=$A$1:$B$2", 5, 5), "=$A$1:$B$2");
        assert_eq!(translate_formula("=$A1+A$1", 2, 3), "=$A4+C$1");
        // QUIRK: a sheet name ending in digits looks like a reference too.
        assert_eq!(translate_formula("=Sheet1!A1", 0, 1), "=ShEET2!A2");
        // Row numbers are JavaScript numbers.
        assert_eq!(translate_formula("=A1000000000000000000000", 0, 0), "=A1e+21");
        // Off the grid on either side.
        assert_eq!(translate_formula("=B1+A1", -1, 0), "=A1+#REF!");
        assert_eq!(translate_formula("=$A1", -5, 0), "=$A1");
        assert_eq!(translate_formula("=A$1", 0, -5), "=A$1");
        assert_eq!(translate_formula("no refs here", 3, 3), "no refs here");
        // Inside a string too (QUIRK).
        assert_eq!(translate_formula("=\"A1\"", 1, 0), "=\"B1\"");
    }

    #[test]
    fn shift_details() {
        assert_eq!(shift_refs("=A5", Axis::Row, 5, -1), "=#REF!");
        assert_eq!(shift_refs("=A6", Axis::Row, 5, -1), "=A5");
        assert_eq!(shift_refs("=A4", Axis::Row, 5, -1), "=A4");
        assert_eq!(shift_refs("=$B$3", Axis::Row, 3, 2), "=$B$5");
        assert_eq!(shift_refs("=A1", Axis::Col, 0, -1), "=#REF!");
        assert_eq!(shift_refs("=B1", Axis::Col, 0, -1), "=A1");
        assert_eq!(shift_refs("=A1", Axis::Col, 0, 1), "=B1");
        assert_eq!(shift_refs("=XFD1", Axis::Col, 0, 1), "=XFE1");
        assert_eq!(shift_refs("=A1+A2", Axis::Col, 0, 0), "=A1+A2");
        // Deleting 3 rows from row 2: rows 2..4 vanish, row 5 becomes row 2.
        assert_eq!(shift_refs("=A2+A4+A5+A1", Axis::Row, 2, -3), "=#REF!+#REF!+A2+A1");
    }

    #[test]
    fn segments() {
        let s = color_segments("=SUM(A1", None, None);
        assert_eq!(s.len(), 4);
        assert_eq!(s[2].text, "(");
        assert_eq!(s[2].color, Some(ERROR_COLOR));
        assert!(s[2].wavy);
        let known: HashSet<String> = ["SUM".to_string()].into_iter().collect();
        let s = color_segments("=SUM(1)+FOO(2)", Some(&known), None);
        assert!(s.iter().any(|x| x.text == "SUM" && x.color == Some(FUNCTION_COLOR)));
        assert!(s.iter().any(|x| x.text == "FOO" && x.wavy));
        assert_eq!(color_segments("abc", None, None), vec![Segment { text: "abc".into(), color: None, wavy: false }]);
    }

    #[test]
    fn syntax() {
        let a = parse_syntax_args("SWITCH(expr, cas1, val1, ..., [défaut])");
        assert_eq!(a.len(), 4);
        assert!(a[2].repeat);
        assert!(a[3].optional);
        assert_eq!(a[3].name, "défaut");
        let (f, parts) = split_signature("SUM(nombre1, ...)", 5);
        assert_eq!(f, "SUM");
        assert!(parts[1].active);
    }

    #[test]
    fn fuzzy() {
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(did_you_mean("sumif", &["SUM", "SUMIF"]), Some("SUMIF".to_string()));
        assert_eq!(did_you_mean("zzzzzz", &["SUM"]), None);
        assert_eq!(fuzzy_score("su", "SUM"), 997.0);
        let l = ["SUM", "ASUM", "XYZ"];
        let r = rank_functions("su", &l, 12, |s| s);
        assert_eq!(r, vec![&"SUM", &"ASUM"]);
    }
}
