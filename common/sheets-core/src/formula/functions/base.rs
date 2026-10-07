//! The engine's own functions (`formula-engine.ts`, the `FUNCTIONS` literal after the spread category
//! maps): the LAMBDA family, the everyday aggregates, logic, rounding, the IF-family, the classic
//! lookups, text and dates.

use std::collections::HashMap;

use super::Registry;
use crate::format;
use crate::formula::criteria::{match_criteria, match_criteria_opt};
use crate::formula::eval::{arg, scalar_of, Evaluator};
use crate::formula::value::{as_mat, compare_scalar, flatten, to_bool, to_num, to_str, width, ErrCode, Lambda, Scalar, Value};
use crate::formula::ast::Node;
use crate::js;
use crate::jsdate;

type R = Result<Value, ErrCode>;

/// `lamScalar`: a lambda's result reduced to one scalar (`#CALC!` for a lambda, top-left of a matrix,
/// `#N/A` for an empty matrix).
pub fn lam_scalar(v: Value) -> Scalar {
    match v {
        Value::L(_) => Scalar::Err(ErrCode::CALC),
        Value::M(m) => m.first().and_then(|r| r.first()).cloned().unwrap_or(Scalar::Err(ErrCode::NA)),
        Value::S(s) => s,
    }
}

pub fn lambda_of(v: Value) -> Option<std::rc::Rc<Lambda>> {
    match v {
        Value::L(l) => Some(l),
        _ => None,
    }
}

/// `a[i] ? num(ev, a[i]) : def`.
pub fn num_or(ev: &Evaluator, a: &[Node], i: usize, def: f64) -> Result<f64, ErrCode> {
    match a.get(i) {
        Some(n) => ev.num(n),
        None => Ok(def),
    }
}

/// An array index from a JavaScript number: `m[x]` is defined only for an integer in range.
pub fn index(x: f64) -> Option<usize> {
    if x >= 0.0 && x.fract() == 0.0 && x < 4_294_967_295.0 {
        Some(x as usize)
    } else {
        None
    }
}

fn lambda_fn(ev: &Evaluator, a: &[Node]) -> R {
    if a.is_empty() {
        return Err(ErrCode::VALUE);
    }
    let mut params = Vec::new();
    for p in &a[..a.len() - 1] {
        match p.var_name() {
            Some(n) => params.push(n.to_string()),
            None => return Err(ErrCode::VALUE),
        }
    }
    Ok(ev.make_lambda(params, &a[a.len() - 1]))
}

fn let_fn(ev: &Evaluator, a: &[Node]) -> R {
    if a.len() < 3 || a.len().is_multiple_of(2) {
        return Err(ErrCode::VALUE);
    }
    let mut e = ev.with_scope(HashMap::new());
    let mut i = 0;
    while i < a.len() - 1 {
        let Some(name) = a[i].var_name() else { return Err(ErrCode::VALUE) };
        let val = e.eval(&a[i + 1]);
        let mut m = HashMap::new();
        m.insert(name.to_string(), val);
        e = e.with_scope(m);
        i += 2;
    }
    Ok(e.eval(&a[a.len() - 1]))
}

fn map_fn(ev: &Evaluator, a: &[Node]) -> R {
    if a.len() < 2 {
        return Err(ErrCode::VALUE);
    }
    let Some(lam) = lambda_of(ev.eval(&a[a.len() - 1])) else { return Err(ErrCode::VALUE) };
    let arrays: Vec<_> = a[..a.len() - 1].iter().map(|n| as_mat(ev.eval(n))).collect();
    let rows = arrays[0].len();
    let cols = width(&arrays[0]);
    let mut out = Vec::with_capacity(rows);
    for i in 0..rows {
        let mut row = Vec::with_capacity(cols);
        for j in 0..cols {
            let args = arrays.iter().map(|m| Value::S(m.get(i).and_then(|r| r.get(j)).cloned().unwrap_or(Scalar::Err(ErrCode::NA)))).collect();
            row.push(lam_scalar(ev.apply_lambda(&lam, args)));
        }
        out.push(row);
    }
    Ok(Value::M(out))
}

fn reduce_fn(ev: &Evaluator, a: &[Node]) -> R {
    if a.len() != 3 {
        return Err(ErrCode::VALUE);
    }
    let Some(lam) = lambda_of(ev.eval(&a[2])) else { return Err(ErrCode::VALUE) };
    let mut acc = ev.eval(&a[0]);
    for s in flatten(&Value::M(as_mat(ev.eval(&a[1])))) {
        acc = ev.apply_lambda(&lam, vec![acc, Value::S(s)]);
    }
    Ok(Value::S(lam_scalar(acc)))
}

fn scan_fn(ev: &Evaluator, a: &[Node]) -> R {
    if a.len() != 3 {
        return Err(ErrCode::VALUE);
    }
    let Some(lam) = lambda_of(ev.eval(&a[2])) else { return Err(ErrCode::VALUE) };
    let mut acc = ev.eval(&a[0]);
    let m = as_mat(ev.eval(&a[1]));
    let mut out = Vec::with_capacity(m.len());
    for row in m {
        let mut r = Vec::with_capacity(row.len());
        for s in row {
            acc = ev.apply_lambda(&lam, vec![acc, Value::S(s)]);
            r.push(lam_scalar(acc.clone()));
        }
        out.push(r);
    }
    Ok(Value::M(out))
}

fn byrow_fn(ev: &Evaluator, a: &[Node]) -> R {
    if a.len() != 2 {
        return Err(ErrCode::VALUE);
    }
    let m = as_mat(ev.eval(&a[0]));
    let Some(lam) = lambda_of(ev.eval(&a[1])) else { return Err(ErrCode::VALUE) };
    Ok(Value::M(m.into_iter().map(|row| vec![lam_scalar(ev.apply_lambda(&lam, vec![Value::M(vec![row])]))]).collect()))
}

fn bycol_fn(ev: &Evaluator, a: &[Node]) -> R {
    if a.len() != 2 {
        return Err(ErrCode::VALUE);
    }
    let m = as_mat(ev.eval(&a[0]));
    let Some(lam) = lambda_of(ev.eval(&a[1])) else { return Err(ErrCode::VALUE) };
    let cols = width(&m);
    let mut row = Vec::with_capacity(cols);
    for j in 0..cols {
        let col: Vec<Vec<Scalar>> = m.iter().map(|r| vec![r.get(j).cloned().unwrap_or(Scalar::Err(ErrCode::NA))]).collect();
        row.push(lam_scalar(ev.apply_lambda(&lam, vec![Value::M(col)])));
    }
    Ok(Value::M(vec![row]))
}

fn makearray_fn(ev: &Evaluator, a: &[Node]) -> R {
    if a.len() != 3 {
        return Err(ErrCode::VALUE);
    }
    let rn = ev.num(&a[0])?;
    let cn = ev.num(&a[1])?;
    let (r, c) = (rn.trunc(), cn.trunc());
    if r < 1.0 || c < 1.0 || r * c > 200_000.0 || r.is_nan() || c.is_nan() {
        return Err(ErrCode::VALUE);
    }
    let Some(lam) = lambda_of(ev.eval(&a[2])) else { return Err(ErrCode::VALUE) };
    let mut out = Vec::with_capacity(r as usize);
    for i in 1..=(r as i64) {
        let mut row = Vec::with_capacity(c as usize);
        for j in 1..=(c as i64) {
            row.push(lam_scalar(ev.apply_lambda(&lam, vec![Value::num(i as f64), Value::num(j as f64)])));
        }
        out.push(row);
    }
    Ok(Value::M(out))
}

fn sum(ev: &Evaluator, a: &[Node]) -> R {
    Ok(Value::num(ev.nums(a)?.iter().sum()))
}

fn average(ev: &Evaluator, a: &[Node]) -> R {
    let n = ev.nums(a)?;
    if n.is_empty() {
        return Err(ErrCode::DIV0);
    }
    Ok(Value::num(n.iter().sum::<f64>() / n.len() as f64))
}

fn min(ev: &Evaluator, a: &[Node]) -> R {
    let n = ev.nums(a)?;
    Ok(Value::num(if n.is_empty() { 0.0 } else { js::min(&n) }))
}

fn max(ev: &Evaluator, a: &[Node]) -> R {
    let n = ev.nums(a)?;
    Ok(Value::num(if n.is_empty() { 0.0 } else { js::max(&n) }))
}

fn count(ev: &Evaluator, a: &[Node]) -> R {
    Ok(Value::num(ev.nums(a)?.len() as f64))
}

fn counta(ev: &Evaluator, a: &[Node]) -> R {
    let mut c = 0;
    for n in a {
        c += flatten(&lambda_text(ev.eval(n))).iter().filter(|s| !s.is_blank()).count();
    }
    Ok(Value::num(c as f64))
}

fn countblank(ev: &Evaluator, a: &[Node]) -> R {
    let mut c = 0;
    for n in a {
        c += flatten(&lambda_text(ev.eval(n))).iter().filter(|s| s.is_blank()).count();
    }
    Ok(Value::num(c as f64))
}

/// A lambda flattens as an object (counted by COUNTA), not as an error.
pub fn lambda_text(v: Value) -> Value {
    match v {
        Value::L(_) => Value::str("[object Object]"),
        o => o,
    }
}

fn product(ev: &Evaluator, a: &[Node]) -> R {
    Ok(Value::num(ev.nums(a)?.iter().product()))
}

fn median(ev: &Evaluator, a: &[Node]) -> R {
    let mut n = ev.nums(a)?;
    if n.is_empty() {
        return Err(ErrCode::NUM);
    }
    js::sort_numbers(&mut n);
    let m = n.len() / 2;
    Ok(Value::num(if n.len() % 2 == 1 { n[m] } else { (n[m - 1] + n[m]) / 2.0 }))
}

fn if_fn(ev: &Evaluator, a: &[Node]) -> R {
    if a.len() < 2 {
        return Err(ErrCode::VALUE);
    }
    let c = ev.scalar(&a[0]);
    if let Scalar::Err(e) = c {
        return Err(e);
    }
    if to_bool(&c) {
        Ok(ev.eval(&a[1]))
    } else {
        Ok(match a.get(2) {
            Some(n) => ev.eval(n),
            None => Value::bool(false),
        })
    }
}

fn iferror(ev: &Evaluator, a: &[Node]) -> R {
    let v = ev.eval(arg(a, 0)?);
    let is_err = match &v {
        Value::S(Scalar::Err(_)) => true,
        Value::M(m) => matches!(m.first().and_then(|r| r.first()), Some(Scalar::Err(_))),
        _ => false,
    };
    if is_err {
        Ok(ev.eval(arg(a, 1)?))
    } else {
        Ok(v)
    }
}

fn ifs(ev: &Evaluator, a: &[Node]) -> R {
    let mut i = 0;
    while i + 1 < a.len() {
        let c = ev.scalar(&a[i]);
        if let Scalar::Err(e) = c {
            return Err(e);
        }
        if to_bool(&c) {
            return Ok(ev.eval(&a[i + 1]));
        }
        i += 2;
    }
    Err(ErrCode::NA)
}

fn and(ev: &Evaluator, a: &[Node]) -> R {
    for n in a {
        for s in flatten(&lambda_text(ev.eval(n))) {
            if let Scalar::Err(e) = s {
                return Err(e);
            }
            if !to_bool(&s) {
                return Ok(Value::bool(false));
            }
        }
    }
    Ok(Value::bool(true))
}

fn or(ev: &Evaluator, a: &[Node]) -> R {
    for n in a {
        for s in flatten(&lambda_text(ev.eval(n))) {
            if let Scalar::Err(e) = s {
                return Err(e);
            }
            if to_bool(&s) {
                return Ok(Value::bool(true));
            }
        }
    }
    Ok(Value::bool(false))
}

fn not(ev: &Evaluator, a: &[Node]) -> R {
    let s = ev.scalar(arg(a, 0)?);
    if let Scalar::Err(e) = s {
        return Err(e);
    }
    Ok(Value::bool(!to_bool(&s)))
}

fn round_fn(ev: &Evaluator, a: &[Node]) -> R {
    let x = ev.num(arg(a, 0)?)?;
    let d = num_or(ev, a, 1, 0.0)?;
    let f = js::pow(10.0, d);
    Ok(Value::num(js::round(x * f) / f))
}

fn roundup(ev: &Evaluator, a: &[Node]) -> R {
    let x = ev.num(arg(a, 0)?)?;
    let d = num_or(ev, a, 1, 0.0)?;
    let f = js::pow(10.0, d);
    Ok(Value::num((if x < 0.0 { -1.0 } else { 1.0 }) * (x.abs() * f).ceil() / f))
}

fn rounddown(ev: &Evaluator, a: &[Node]) -> R {
    let x = ev.num(arg(a, 0)?)?;
    let d = num_or(ev, a, 1, 0.0)?;
    let f = js::pow(10.0, d);
    Ok(Value::num((if x < 0.0 { -1.0 } else { 1.0 }) * (x.abs() * f).floor() / f))
}

fn floor(ev: &Evaluator, a: &[Node]) -> R {
    let x = ev.num(arg(a, 0)?)?;
    let m = num_or(ev, a, 1, 1.0)?;
    Ok(Value::num(if m == 0.0 { 0.0 } else { (x / m).floor() * m }))
}

fn ceiling(ev: &Evaluator, a: &[Node]) -> R {
    let x = ev.num(arg(a, 0)?)?;
    let m = num_or(ev, a, 1, 1.0)?;
    Ok(Value::num(if m == 0.0 { 0.0 } else { (x / m).ceil() * m }))
}

fn int(ev: &Evaluator, a: &[Node]) -> R {
    Ok(Value::num(ev.num(arg(a, 0)?)?.floor()))
}

fn abs(ev: &Evaluator, a: &[Node]) -> R {
    Ok(Value::num(ev.num(arg(a, 0)?)?.abs()))
}

fn mod_fn(ev: &Evaluator, a: &[Node]) -> R {
    let x = ev.num(arg(a, 0)?)?;
    let d = ev.num(arg(a, 1)?)?;
    if d == 0.0 {
        return Err(ErrCode::DIV0);
    }
    Ok(Value::num(((x % d) + d) % d))
}

fn power(ev: &Evaluator, a: &[Node]) -> R {
    let x = ev.num(arg(a, 0)?)?;
    let p = ev.num(arg(a, 1)?)?;
    Ok(Value::num(js::pow(x, p)))
}

fn sqrt(ev: &Evaluator, a: &[Node]) -> R {
    let x = ev.num(arg(a, 0)?)?;
    if x < 0.0 {
        return Err(ErrCode::NUM);
    }
    Ok(Value::num(x.sqrt()))
}

fn exp(ev: &Evaluator, a: &[Node]) -> R {
    Ok(Value::num(ev.num(arg(a, 0)?)?.exp()))
}

fn ln(ev: &Evaluator, a: &[Node]) -> R {
    let x = ev.num(arg(a, 0)?)?;
    if x <= 0.0 {
        return Err(ErrCode::NUM);
    }
    Ok(Value::num(x.ln()))
}

fn log(ev: &Evaluator, a: &[Node]) -> R {
    let x = ev.num(arg(a, 0)?)?;
    let b = num_or(ev, a, 1, 10.0)?;
    if x <= 0.0 {
        return Err(ErrCode::NUM);
    }
    Ok(Value::num(x.ln() / b.ln()))
}

fn sumif_like(ev: &Evaluator, a: &[Node], avg: bool) -> R {
    let range = ev.eval(arg(a, 0)?);
    let crit = ev.scalar(arg(a, 1)?);
    let sum_range = match a.get(2) {
        Some(n) => ev.eval(n),
        None => range.clone(),
    };
    let rf = flatten(&range);
    let sf = flatten(&sum_range);
    let (mut sum, mut cnt) = (0.0, 0usize);
    for (i, c) in rf.iter().enumerate() {
        if match_criteria(c, &crit) {
            let v = to_num(sf.get(i).unwrap_or(&Scalar::Num(0.0)));
            if let Ok(v) = v {
                sum += v;
                cnt += 1;
            }
        }
    }
    if avg {
        if cnt == 0 {
            Err(ErrCode::DIV0)
        } else {
            Ok(Value::num(sum / cnt as f64))
        }
    } else {
        Ok(Value::num(sum))
    }
}

fn sumif(ev: &Evaluator, a: &[Node]) -> R {
    sumif_like(ev, a, false)
}

fn averageif(ev: &Evaluator, a: &[Node]) -> R {
    sumif_like(ev, a, true)
}

fn countif(ev: &Evaluator, a: &[Node]) -> R {
    let range = ev.eval(arg(a, 0)?);
    let crit = ev.scalar(arg(a, 1)?);
    Ok(Value::num(flatten(&range).iter().filter(|s| match_criteria(s, &crit)).count() as f64))
}

fn sumifs(ev: &Evaluator, a: &[Node]) -> R {
    let sum_range = flatten(&ev.eval(arg(a, 0)?));
    let mut pairs = Vec::new();
    let mut i = 1;
    while i + 1 < a.len() {
        pairs.push((flatten(&ev.eval(&a[i])), ev.scalar(&a[i + 1])));
        i += 2;
    }
    let mut sum = 0.0;
    for (i, s) in sum_range.iter().enumerate() {
        if pairs.iter().all(|(r, c)| match_criteria_opt(r.get(i), c)) {
            if let Ok(v) = to_num(s) {
                sum += v;
            }
        }
    }
    Ok(Value::num(sum))
}

fn countifs(ev: &Evaluator, a: &[Node]) -> R {
    let mut pairs = Vec::new();
    let mut i = 0;
    while i + 1 < a.len() {
        pairs.push((flatten(&ev.eval(&a[i])), ev.scalar(&a[i + 1])));
        i += 2;
    }
    if pairs.is_empty() {
        return Ok(Value::num(0.0));
    }
    let n = pairs[0].0.len();
    let c = (0..n).filter(|&i| pairs.iter().all(|(r, c)| match_criteria_opt(r.get(i), c))).count();
    Ok(Value::num(c as f64))
}

fn lookup_vh(ev: &Evaluator, a: &[Node], vertical: bool) -> R {
    let target = ev.scalar(arg(a, 0)?);
    let m = match ev.eval(arg(a, 1)?) {
        Value::M(m) => m,
        _ => return Err(ErrCode::REF),
    };
    let idx = ev.num(arg(a, 2)?)?;
    let exact = match a.get(3) {
        Some(n) => !to_bool(&ev.scalar(n)),
        None => false,
    };
    let eq = |cell: &Scalar| -> bool {
        if exact {
            to_str(cell).to_lowercase() == to_str(&target).to_lowercase()
        } else {
            compare_scalar(cell, &target) == 0.0
        }
    };
    let pick = |i: f64| index(i - 1.0);
    if vertical {
        for row in &m {
            let first = row.first().cloned().unwrap_or_else(|| Scalar::str(""));
            if eq(&first) {
                return Ok(Value::S(pick(idx).and_then(|k| row.get(k).cloned()).unwrap_or(Scalar::Err(ErrCode::REF))));
            }
        }
    } else if let Some(head) = m.first() {
        for (c, h) in head.iter().enumerate() {
            if eq(h) {
                return Ok(Value::S(pick(idx).and_then(|k| m.get(k)).and_then(|r| r.get(c).cloned()).unwrap_or(Scalar::Err(ErrCode::REF))));
            }
        }
    }
    Err(ErrCode::NA)
}

fn vlookup(ev: &Evaluator, a: &[Node]) -> R {
    lookup_vh(ev, a, true)
}

fn hlookup(ev: &Evaluator, a: &[Node]) -> R {
    lookup_vh(ev, a, false)
}

fn index_fn(ev: &Evaluator, a: &[Node]) -> R {
    let m = match ev.eval(arg(a, 0)?) {
        Value::M(m) => m,
        _ => return Err(ErrCode::REF),
    };
    let r = num_or(ev, a, 1, 1.0)?;
    let c: Result<f64, ErrCode> = match a.get(2) {
        Some(n) => ev.num(n),
        None => Ok(if m.first().map(|r| r.len()) == Some(1) { 1.0 } else { r }),
    };
    if a.len() < 3 && m.len() == 1 {
        return Ok(Value::S(index(r - 1.0).and_then(|k| m[0].get(k).cloned()).unwrap_or(Scalar::Err(ErrCode::REF))));
    }
    let cn = c.unwrap_or(1.0);
    Ok(Value::S(index(r - 1.0).and_then(|i| m.get(i)).and_then(|row| index(cn - 1.0).and_then(|j| row.get(j).cloned())).unwrap_or(Scalar::Err(ErrCode::REF))))
}

fn match_fn(ev: &Evaluator, a: &[Node]) -> R {
    let target = ev.scalar(arg(a, 0)?);
    let vec = flatten(&ev.eval(arg(a, 1)?));
    let ty = num_or(ev, a, 2, 1.0)?;
    if ty == 0.0 {
        let t = to_str(&target).to_lowercase();
        return match vec.iter().position(|s| to_str(s).to_lowercase() == t) {
            Some(i) => Ok(Value::num(i as f64 + 1.0)),
            None => Err(ErrCode::NA),
        };
    }
    let mut best: i64 = -1;
    for (i, s) in vec.iter().enumerate() {
        let cmp = compare_scalar(s, &target);
        if ty == 1.0 && cmp <= 0.0 {
            best = i as i64;
        }
        if ty == -1.0 && cmp >= 0.0 {
            best = i as i64;
        }
    }
    if best < 0 {
        Err(ErrCode::NA)
    } else {
        Ok(Value::num(best as f64 + 1.0))
    }
}

fn len(ev: &Evaluator, a: &[Node]) -> R {
    Ok(Value::num(js::len16(&ev.str(arg(a, 0)?)) as f64))
}

fn left(ev: &Evaluator, a: &[Node]) -> R {
    let s = ev.str(arg(a, 0)?);
    let n = num_or(ev, a, 1, 1.0)?;
    Ok(Value::str(js::slice(&s, 0.0, Some(n))))
}

fn right(ev: &Evaluator, a: &[Node]) -> R {
    let s = ev.str(arg(a, 0)?);
    let n = num_or(ev, a, 1, 1.0)?;
    Ok(Value::str(if n <= 0.0 { String::new() } else { js::slice(&s, -n, None) }))
}

fn mid(ev: &Evaluator, a: &[Node]) -> R {
    let s = ev.str(arg(a, 0)?);
    let st = ev.num(arg(a, 1)?)?;
    let ln = ev.num(arg(a, 2)?)?;
    Ok(Value::str(js::slice(&s, st - 1.0, Some(st - 1.0 + ln))))
}

/// `s.replace(/\s+/g, ' ').trim()`.
pub fn collapse_spaces(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_space = false;
    for c in s.chars() {
        if js::is_whitespace(c) {
            if !in_space {
                out.push(' ');
            }
            in_space = true;
        } else {
            out.push(c);
            in_space = false;
        }
    }
    js::trim(&out).to_string()
}

fn trim(ev: &Evaluator, a: &[Node]) -> R {
    Ok(Value::str(collapse_spaces(&ev.str(arg(a, 0)?))))
}

fn upper(ev: &Evaluator, a: &[Node]) -> R {
    Ok(Value::str(js::upper(&ev.str(arg(a, 0)?))))
}

fn lower(ev: &Evaluator, a: &[Node]) -> R {
    Ok(Value::str(js::lower(&ev.str(arg(a, 0)?))))
}

/// `PROPER`: `/\b\w/` upper-cased, `/\B\w/` lower-cased (ASCII word characters, as JavaScript's `\w`).
pub fn proper(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    for (i, &c) in chars.iter().enumerate() {
        if !js::is_word(c) {
            out.push(c);
            continue;
        }
        let prev_word = i > 0 && js::is_word(chars[i - 1]);
        if prev_word {
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c.to_ascii_uppercase());
        }
    }
    out
}

fn proper_fn(ev: &Evaluator, a: &[Node]) -> R {
    Ok(Value::str(proper(&ev.str(arg(a, 0)?))))
}

fn concat(ev: &Evaluator, a: &[Node]) -> R {
    let mut s = String::new();
    for n in a {
        for x in flatten(&lambda_text(ev.eval(n))) {
            s.push_str(&to_str(&x));
        }
    }
    Ok(Value::str(s))
}

fn concatenate(ev: &Evaluator, a: &[Node]) -> R {
    let mut s = String::new();
    for n in a {
        s.push_str(&ev.str(n));
    }
    Ok(Value::str(s))
}

fn textjoin(ev: &Evaluator, a: &[Node]) -> R {
    let sep = ev.str(arg(a, 0)?);
    let skip = to_bool(&ev.scalar(arg(a, 1)?));
    let mut parts = Vec::new();
    for n in a.iter().skip(2) {
        for x in flatten(&lambda_text(ev.eval(n))) {
            let t = to_str(&x);
            if !(skip && t.is_empty()) {
                parts.push(t);
            }
        }
    }
    Ok(Value::str(parts.join(&sep)))
}

fn replace(ev: &Evaluator, a: &[Node]) -> R {
    let s = ev.str(arg(a, 0)?);
    let st = ev.num(arg(a, 1)?)?;
    let ln = ev.num(arg(a, 2)?)?;
    let ns = ev.str(arg(a, 3)?);
    Ok(Value::str(js::slice(&s, 0.0, Some(st - 1.0)) + &ns + &js::slice(&s, st - 1.0 + ln, None)))
}

/// `s.split(old).join(new)` (an empty `old` splits between UTF-16 units).
pub fn split_join(s: &str, old: &str, new: &str) -> String {
    if old.is_empty() {
        let units: Vec<u16> = s.encode_utf16().collect();
        return units.iter().map(|u| js::from_utf16(&[*u])).collect::<Vec<_>>().join(new);
    }
    s.split(old).collect::<Vec<_>>().join(new)
}

fn substitute(ev: &Evaluator, a: &[Node]) -> R {
    let s = ev.str(arg(a, 0)?);
    let old = ev.str(arg(a, 1)?);
    let new = ev.str(arg(a, 2)?);
    Ok(Value::str(split_join(&s, &old, &new)))
}

fn text(ev: &Evaluator, a: &[Node]) -> R {
    let v = ev.scalar(arg(a, 0)?);
    let fmt = ev.str(arg(a, 1)?);
    Ok(Value::str(format::format_text(&v, &fmt)))
}

fn value(ev: &Evaluator, a: &[Node]) -> R {
    let s = ev.str(arg(a, 0)?);
    let kept: String = s.chars().filter(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | 'e' | 'E')).collect();
    match js::number(&kept) {
        Some(n) if !n.is_nan() => Ok(Value::num(n)),
        _ => Err(ErrCode::VALUE),
    }
}

fn today(ev: &Evaluator, _a: &[Node]) -> R {
    Ok(Value::num(jsdate::today_serial(&ev.now())))
}

fn now(ev: &Evaluator, _a: &[Node]) -> R {
    Ok(Value::num(jsdate::now_serial(&ev.now())))
}

/// `serialToDate(num(ev, a[0]))` components (`None` → `#VALUE!`).
fn date_parts(ev: &Evaluator, a: &[Node]) -> Result<jsdate::Parts, ErrCode> {
    let n = ev.num(arg(a, 0)?).map_err(|_| ErrCode::VALUE)?;
    jsdate::serial_to_date(n).and_then(jsdate::parts).ok_or(ErrCode::VALUE)
}

fn year(ev: &Evaluator, a: &[Node]) -> R {
    Ok(Value::num(date_parts(ev, a)?.year as f64))
}

fn month(ev: &Evaluator, a: &[Node]) -> R {
    Ok(Value::num(date_parts(ev, a)?.month as f64 + 1.0))
}

fn day(ev: &Evaluator, a: &[Node]) -> R {
    Ok(Value::num(date_parts(ev, a)?.date as f64))
}

fn isblank(ev: &Evaluator, a: &[Node]) -> R {
    let v = ev.eval(arg(a, 0)?);
    let s = scalar_of(&v);
    Ok(Value::bool(s.is_blank()))
}

fn isnumber(ev: &Evaluator, a: &[Node]) -> R {
    Ok(Value::bool(ev.scalar(arg(a, 0)?).is_num()))
}

fn istext(ev: &Evaluator, a: &[Node]) -> R {
    Ok(Value::bool(ev.scalar(arg(a, 0)?).is_str()))
}

fn iserror(ev: &Evaluator, a: &[Node]) -> R {
    Ok(Value::bool(ev.scalar(arg(a, 0)?).is_err()))
}

fn islogical(ev: &Evaluator, a: &[Node]) -> R {
    Ok(Value::bool(ev.scalar(arg(a, 0)?).is_bool()))
}

fn true_fn(_ev: &Evaluator, _a: &[Node]) -> R {
    Ok(Value::bool(true))
}

fn false_fn(_ev: &Evaluator, _a: &[Node]) -> R {
    Ok(Value::bool(false))
}

fn pi(_ev: &Evaluator, _a: &[Node]) -> R {
    Ok(Value::num(std::f64::consts::PI))
}

pub fn register(r: &mut Registry) {
    let list: &[(&'static str, crate::formula::eval::Fn)] = &[
        ("LAMBDA", lambda_fn),
        ("LET", let_fn),
        ("MAP", map_fn),
        ("REDUCE", reduce_fn),
        ("SCAN", scan_fn),
        ("BYROW", byrow_fn),
        ("BYCOL", bycol_fn),
        ("MAKEARRAY", makearray_fn),
        ("SUM", sum),
        ("AVERAGE", average),
        ("MIN", min),
        ("MAX", max),
        ("COUNT", count),
        ("COUNTA", counta),
        ("COUNTBLANK", countblank),
        ("PRODUCT", product),
        ("MEDIAN", median),
        ("IF", if_fn),
        ("IFERROR", iferror),
        ("IFS", ifs),
        ("AND", and),
        ("OR", or),
        ("NOT", not),
        ("ROUND", round_fn),
        ("ROUNDUP", roundup),
        ("ROUNDDOWN", rounddown),
        ("FLOOR", floor),
        ("CEILING", ceiling),
        ("INT", int),
        ("ABS", abs),
        ("MOD", mod_fn),
        ("POWER", power),
        ("SQRT", sqrt),
        ("EXP", exp),
        ("LN", ln),
        ("LOG", log),
        ("PI", pi),
        ("SUMIF", sumif),
        ("AVERAGEIF", averageif),
        ("COUNTIF", countif),
        ("SUMIFS", sumifs),
        ("COUNTIFS", countifs),
        ("VLOOKUP", vlookup),
        ("HLOOKUP", hlookup),
        ("INDEX", index_fn),
        ("MATCH", match_fn),
        ("LEN", len),
        ("LEFT", left),
        ("RIGHT", right),
        ("MID", mid),
        ("TRIM", trim),
        ("UPPER", upper),
        ("LOWER", lower),
        ("PROPER", proper_fn),
        ("CONCAT", concat),
        ("CONCATENATE", concatenate),
        ("TEXTJOIN", textjoin),
        ("REPLACE", replace),
        ("SUBSTITUTE", substitute),
        ("TEXT", text),
        ("VALUE", value),
        ("TODAY", today),
        ("NOW", now),
        ("YEAR", year),
        ("MONTH", month),
        ("DAY", day),
        ("ISBLANK", isblank),
        ("ISNUMBER", isnumber),
        ("ISTEXT", istext),
        ("ISERROR", iserror),
        ("ISLOGICAL", islogical),
        ("TRUE", true_fn),
        ("FALSE", false_fn),
    ];
    for (name, f) in list {
        r.insert(name, *f);
    }
}

#[cfg(test)]
mod tests {
    use crate::formula::testing::{eval_in, sheet};
    use crate::formula::value::{ErrCode, Scalar, Value};
    use serde_json::json;

    #[test]
    fn aggregates() {
        let c = sheet(&[("A1", json!(1)), ("A2", json!("2")), ("A3", json!("x")), ("A4", json!(true)), ("A5", json!(""))]);
        assert_eq!(eval_in(&c, "=SUM(A1:A5)"), Value::num(4.0));
        assert_eq!(eval_in(&c, "=COUNT(A1:A5)"), Value::num(3.0));
        assert_eq!(eval_in(&c, "=COUNTA(A1:A6)"), Value::num(4.0));
        assert_eq!(eval_in(&c, "=COUNTBLANK(A1:A6)"), Value::num(2.0));
        assert_eq!(eval_in(&c, "=AVERAGE(A1:A2)"), Value::num(1.5));
        assert_eq!(eval_in(&c, "=AVERAGE(B1:B2)"), Value::err(ErrCode::DIV0));
        assert_eq!(eval_in(&c, "=MAX(B1:B2)"), Value::num(0.0));
        assert_eq!(eval_in(&c, "=MEDIAN(1,3,2,4)"), Value::num(2.5));
    }

    #[test]
    fn logic_and_rounding() {
        let c = sheet(&[]);
        assert_eq!(eval_in(&c, "=IF(1>2,\"a\")"), Value::bool(false));
        assert_eq!(eval_in(&c, "=IFERROR(1/0,\"z\")"), Value::str("z"));
        assert_eq!(eval_in(&c, "=ROUND(2.5)"), Value::num(3.0));
        assert_eq!(eval_in(&c, "=ROUND(-2.5)"), Value::num(-2.0));
        assert_eq!(eval_in(&c, "=ROUND(1.2345,2)"), Value::num(1.23));
        assert_eq!(eval_in(&c, "=MOD(-3,5)"), Value::num(2.0));
        assert_eq!(eval_in(&c, "=AND(1,\"false\")"), Value::bool(true)); // "false" is truthy on the web
        assert_eq!(eval_in(&c, "=IFS(FALSE,1)"), Value::err(ErrCode::NA));
    }

    #[test]
    fn lookups() {
        let c = sheet(&[("A1", json!("a")), ("B1", json!(1)), ("A2", json!("b")), ("B2", json!(2))]);
        assert_eq!(eval_in(&c, "=VLOOKUP(\"B\",A1:B2,2,FALSE)"), Value::num(2.0));
        assert_eq!(eval_in(&c, "=VLOOKUP(\"c\",A1:B2,2)"), Value::err(ErrCode::NA));
        assert_eq!(eval_in(&c, "=INDEX(A1:B2,2,1)"), Value::str("b"));
        assert_eq!(eval_in(&c, "=MATCH(\"b\",A1:A2,0)"), Value::num(2.0));
        assert_eq!(eval_in(&c, "=SUMIF(A1:A2,\"a\",B1:B2)"), Value::num(1.0));
        assert_eq!(eval_in(&c, "=COUNTIFS(A1:A2,\"*\",B1:B2,\">1\")"), Value::num(1.0));
    }

    #[test]
    fn text_and_dates() {
        let c = sheet(&[]);
        assert_eq!(eval_in(&c, "=PROPER(\"élan VITAL-x\")"), Value::str("éLan Vital-X"));
        assert_eq!(eval_in(&c, "=TRIM(\"  a   b \")"), Value::str("a b"));
        assert_eq!(eval_in(&c, "=MID(\"abcdef\",2,3)"), Value::str("bcd"));
        assert_eq!(eval_in(&c, "=RIGHT(\"abc\",5)"), Value::str("abc"));
        assert_eq!(eval_in(&c, "=SUBSTITUTE(\"a-b-c\",\"-\",\"+\")"), Value::str("a+b+c"));
        assert_eq!(eval_in(&c, "=VALUE(\"abc\")"), Value::num(0.0));
        assert_eq!(eval_in(&c, "=VALUE(\"12 %\")"), Value::num(12.0));
        assert_eq!(eval_in(&c, "=TODAY()"), Value::num(46302.0));
        assert_eq!(eval_in(&c, "=YEAR(45292)"), Value::num(2024.0));
        assert_eq!(eval_in(&c, "=YEAR(\"x\")"), Value::err(ErrCode::VALUE));
        assert_eq!(eval_in(&c, "=TEXT(0.5,\"0.00\")"), Value::str("0.50"));
        assert_eq!(eval_in(&c, "=LEN(\"a😀\")"), Value::num(3.0));
        assert_eq!(eval_in(&c, "=CONCAT({1,2},\"x\")"), Value::str("12x"));
    }

    #[test]
    fn lambda_helpers() {
        let c = sheet(&[]);
        assert_eq!(eval_in(&c, "=REDUCE(0,{1,2,3},LAMBDA(a,b,a+b))"), Value::num(6.0));
        assert_eq!(eval_in(&c, "=MAP({1,2},LAMBDA(x,x*10))"), Value::M(vec![vec![Scalar::Num(10.0), Scalar::Num(20.0)]]));
        assert_eq!(eval_in(&c, "=BYROW({1,2;3,4},LAMBDA(r,SUM(r)))"), Value::M(vec![vec![Scalar::Num(3.0)], vec![Scalar::Num(7.0)]]));
        assert_eq!(eval_in(&c, "=MAKEARRAY(2,2,LAMBDA(r,c,r*c))"), Value::M(vec![vec![Scalar::Num(1.0), Scalar::Num(2.0)], vec![Scalar::Num(2.0), Scalar::Num(4.0)]]));
        assert_eq!(eval_in(&c, "=SCAN(0,{1,2},LAMBDA(a,b,a+b))"), Value::M(vec![vec![Scalar::Num(1.0), Scalar::Num(3.0)]]));
    }
}
