//! `matchCriteria(cell, crit)`: the criteria of COUNTIF, SUMIF, MAXIFS, the D-functions.
//!
//! An operator prefix (`<= >= <> = < >`) compares numerically when both sides read as numbers
//! (`Number()`, so an empty side is 0), else only `=`/`<>` compare as lower-cased text; `*` and `?` are
//! wildcards (case-insensitive, `.` of a JavaScript regular expression: no line terminators); a plain
//! criterion is numeric equality for a number, else case-insensitive equality.

use super::value::{to_str, Scalar};
use crate::js;

fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// `Number(cell)` for a scalar (`NaN` → `None`).
fn number_of(s: &Scalar) -> Option<f64> {
    match s {
        Scalar::Num(n) => Some(*n),
        Scalar::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        Scalar::Str(t) => js::number(t),
        Scalar::Err(_) => None,
    }
    .filter(|n| !n.is_nan())
}

/// Matches `text` against a `*`/`?` pattern, ignoring case (as `new RegExp(…, 'i')` does).
pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().flat_map(char::to_lowercase).collect();
    let t: Vec<char> = text.chars().flat_map(char::to_lowercase).collect();
    // Classic two-pointer glob with backtracking on the last '*'.
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ti));
            pi += 1;
            continue;
        }
        if pi < p.len() && ((p[pi] == '?' && !is_line_terminator(t[ti])) || (p[pi] != '?' && p[pi] == t[ti])) {
            pi += 1;
            ti += 1;
            continue;
        }
        match star {
            Some((sp, st)) if !is_line_terminator(t[st]) => {
                pi = sp + 1;
                ti = st + 1;
                star = Some((sp, st + 1));
            }
            _ => return false,
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// `matchCriteria(cell, crit)`.
pub fn match_criteria(cell: &Scalar, crit: &Scalar) -> bool {
    match_criteria_opt(Some(cell), crit)
}

/// `matchCriteria(cell, crit)` where `cell` may be `undefined` (read past the end of a shorter range):
/// it is then not a number (`Number(undefined)` is NaN) and reads as `''`.
pub fn match_criteria_opt(cell: Option<&Scalar>, crit: &Scalar) -> bool {
    let cs_owned = to_str(crit);
    let cs = js::trim(&cs_owned);
    let cell_text = || cell.map(to_str).unwrap_or_default();
    let ops = ["<=", ">=", "<>", "=", "<", ">"];
    if let Some(op) = ops.iter().find(|o| cs.starts_with(*o)) {
        let rhs_raw = &cs[op.len()..];
        // `(.*)$` cannot cross a line terminator: such a criterion is not an operator one.
        if !rhs_raw.contains(is_line_terminator) {
            let rhs = js::trim(rhs_raw);
            let cn = cell.and_then(number_of);
            let rn = js::number(rhs).filter(|n| !n.is_nan());
            if let (Some(cn), Some(rn)) = (cn, rn) {
                return match *op {
                    "<=" => cn <= rn,
                    ">=" => cn >= rn,
                    "<>" => cn != rn,
                    "=" => cn == rn,
                    "<" => cn < rn,
                    _ => cn > rn,
                };
            }
            let a = cell_text().to_lowercase();
            let b = rhs.to_lowercase();
            return match *op {
                "=" => a == b,
                "<>" => a != b,
                _ => false,
            };
        }
    }
    if cs.contains(['*', '?']) {
        return wildcard_match(cs, &cell_text());
    }
    if let Some(Scalar::Num(n)) = cell {
        if let Some(c) = js::number(cs) {
            return *n == c;
        }
    }
    cell_text().to_lowercase() == cs.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn criteria_follow_the_web() {
        let n = |x: f64| Scalar::Num(x);
        let s = |x: &str| Scalar::str(x);
        assert!(match_criteria(&n(5.0), &s(">3")));
        assert!(!match_criteria(&n(2.0), &s(">3")));
        assert!(match_criteria(&s("abc"), &s("=ABC")));
        assert!(!match_criteria(&s("abc"), &s(">b")));
        assert!(match_criteria(&s("Pomme"), &s("p*e")));
        assert!(match_criteria(&s("Pomme"), &s("p?mme")));
        assert!(!match_criteria(&s("Pomme"), &s("p?me")));
        assert!(match_criteria(&n(3.0), &n(3.0)));
        assert!(match_criteria(&n(0.0), &s(""))); // Number('') is 0
        assert!(match_criteria(&s(""), &s("=")));
        assert!(match_criteria(&n(4.0), &s("<>5")));
        assert!(match_criteria(&Scalar::Bool(true), &s("=1")));
    }
}
