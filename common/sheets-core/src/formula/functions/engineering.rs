//! Port of the web's `formula-fns/engineering.ts`: base conversions, bitwise operations, DELTA/GESTEP,
//! CONVERT, ERF, the basic complex-number functions and the database (D) functions.

use super::Registry;
use crate::formula::ast::Node;
use crate::formula::criteria::match_criteria_opt;
use crate::formula::eval::{arg, Evaluator};
use crate::formula::value::{flatten, to_str, ErrCode, Matrix, Scalar, Value};
use crate::js;

type R = Result<Value, ErrCode>;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// `optNum`: absent argument → `def`; any error → `#VALUE!`.
fn opt_num(ev: &Evaluator, a: &[Node], i: usize, def: f64) -> Result<f64, ErrCode> {
    match a.get(i) {
        None => Ok(def),
        Some(n) => ev.num(n).map_err(|_| ErrCode::VALUE),
    }
}

/// `toInt`: `#NUM!` for a non-finite number, else the truncation.
fn to_int(v: f64) -> Result<f64, ErrCode> {
    if !v.is_finite() {
        return Err(ErrCode::NUM);
    }
    Ok(js::trunc(v))
}

// ── Base conversions ──────────────────────────────────────────────────────────

fn bits_of(base: u32) -> i32 {
    match base {
        2 => 10,
        8 => 30,
        _ => 40,
    }
}

/// `parseBase`: a string in `base` as a signed decimal (10-digit two's complement).
fn parse_base(raw: &str, base: u32) -> Result<f64, ErrCode> {
    let s = js::upper(js::trim(raw));
    if s.is_empty() {
        return Ok(0.0);
    }
    if js::len16(&s) > 10 {
        return Err(ErrCode::NUM);
    }
    let mut n = 0.0f64;
    for c in s.chars() {
        let ok = match base {
            2 => matches!(c, '0' | '1'),
            8 => ('0'..='7').contains(&c),
            _ => c.is_ascii_digit() || ('A'..='F').contains(&c),
        };
        if !ok {
            return Err(ErrCode::NUM);
        }
        n = n * base as f64 + c.to_digit(16).unwrap_or(0) as f64;
    }
    let bits = bits_of(base);
    let sign_bit = js::pow(2.0, (bits - 1) as f64);
    if n >= sign_bit {
        n -= js::pow(2.0, bits as f64);
    }
    Ok(n)
}

/// `formatBase`: a signed decimal in `base` with the 10-digit width; `places` pads positive results.
fn format_base(dec: f64, base: u32, places: Option<f64>) -> R {
    let n = to_int(dec)?;
    let bits = bits_of(base);
    let max = js::pow(2.0, (bits - 1) as f64) - 1.0;
    let min = -js::pow(2.0, (bits - 1) as f64);
    if n > max || n < min {
        return Err(ErrCode::NUM);
    }
    let radix = |v: u64| -> String {
        match base {
            2 => format!("{v:b}"),
            8 => format!("{v:o}"),
            _ => format!("{v:X}"),
        }
    };
    if n < 0.0 {
        // Negative results are always full-width, `places` is ignored.
        return Ok(Value::str(radix((n + js::pow(2.0, bits as f64)) as u64)));
    }
    let mut out = radix(n as u64);
    if let Some(pl) = places {
        let p = js::trunc(pl);
        if p < 0.0 || p < out.len() as f64 {
            return Err(ErrCode::NUM);
        }
        // `padStart` throws a RangeError (→ #VALUE!) beyond the engine's maximum string length.
        if p > 536_870_888.0 {
            return Err(ErrCode::VALUE);
        }
        out = js::pad_start(&out, p as usize, '0');
    }
    Ok(Value::str(out))
}

/// The optional `places` argument of the conversions.
fn places_arg(ev: &Evaluator, a: &[Node]) -> Result<Option<f64>, ErrCode> {
    if a.len() > 1 {
        let p = opt_num(ev, a, 1, f64::NAN)?;
        Ok(if p.is_nan() { None } else { Some(p) })
    } else {
        Ok(None)
    }
}

/// `convFn(from, to)` (`to == 10` → decimal).
fn conv(ev: &Evaluator, a: &[Node], from: u32, to: u32) -> R {
    let s = ev.str(arg(a, 0)?);
    let dec = parse_base(&s, from)?;
    if to == 10 {
        return Ok(Value::num(dec));
    }
    let places = places_arg(ev, a)?;
    format_base(dec, to, places)
}

fn dec2(ev: &Evaluator, a: &[Node], to: u32) -> R {
    let v = ev.num(arg(a, 0)?)?;
    let places = places_arg(ev, a)?;
    format_base(v, to, places)
}

// ── Bitwise ───────────────────────────────────────────────────────────────────

const TWO_48: f64 = 281_474_976_710_656.0;

fn bit_arg(ev: &Evaluator, n: &Node) -> Result<f64, ErrCode> {
    let v = ev.num(n).map_err(|_| ErrCode::NUM)?;
    let t = to_int(v)?;
    if t < 0.0 || t >= TWO_48 {
        return Err(ErrCode::NUM);
    }
    Ok(t)
}

/// `bitwise`: the op over the high/low 24-bit halves (JS 32-bit integer semantics on each half).
fn bitwise(x: f64, y: f64, op: fn(i32, i32) -> i32) -> f64 {
    let xl = (x % 16_777_216.0) as i32;
    let xh = (x / 16_777_216.0).floor() as i32;
    let yl = (y % 16_777_216.0) as i32;
    let yh = (y / 16_777_216.0).floor() as i32;
    op(xh, yh) as f64 * 16_777_216.0 + op(xl, yl) as f64
}

fn bit_op(ev: &Evaluator, a: &[Node], op: fn(i32, i32) -> i32) -> R {
    let x = bit_arg(ev, arg(a, 0)?)?;
    let y = bit_arg(ev, arg(a, 1)?)?;
    Ok(Value::num(bitwise(x, y, op)))
}

fn bit_shift(ev: &Evaluator, a: &[Node], left: bool) -> R {
    let x = bit_arg(ev, arg(a, 0)?)?;
    let sh = ev.num(arg(a, 1)?)?;
    let s = js::trunc(sh);
    if s.abs() > 53.0 {
        return Err(ErrCode::NUM);
    }
    let r = if left == (s >= 0.0) { x * js::pow(2.0, s.abs()) } else { (x / js::pow(2.0, s.abs())).floor() };
    if r >= TWO_48 {
        return Err(ErrCode::NUM);
    }
    Ok(Value::num(r))
}

// ── ERF ───────────────────────────────────────────────────────────────────────

fn erf(x: f64) -> f64 {
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let ax = x.abs();
    let t = 1.0 / (1.0 + 0.3275911 * ax);
    let y = 1.0 - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t + 0.254829592) * t * (-ax * ax).exp();
    sign * y
}

fn erfc(x: f64) -> f64 {
    1.0 - erf(x)
}

// ── Complex numbers ───────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct Complex {
    re: f64,
    im: f64,
    suffix: char,
}

fn parse_complex(s: &str) -> Result<Complex, ErrCode> {
    let raw = js::trim(s);
    if raw.is_empty() {
        return Ok(Complex { re: 0.0, im: 0.0, suffix: 'i' });
    }
    let suffix = if raw.contains('j') { 'j' } else { 'i' };
    if !raw.contains('i') && !raw.contains('j') {
        return match js::number(raw) {
            Some(n) => Ok(Complex { re: n, im: 0.0, suffix }),
            None => Err(ErrCode::NUM),
        };
    }
    let body: Vec<char> = raw.chars().filter(|c| *c != 'i' && *c != 'j').collect();
    if body.is_empty() || (body.len() == 1 && body[0] == '+') {
        return Ok(Complex { re: 0.0, im: 1.0, suffix });
    }
    if body.len() == 1 && body[0] == '-' {
        return Ok(Complex { re: 0.0, im: -1.0, suffix });
    }
    let mut split_at = None;
    for k in (1..body.len()).rev() {
        let c = body[k];
        if (c == '+' || c == '-') && body[k - 1] != 'e' && body[k - 1] != 'E' {
            split_at = Some(k);
            break;
        }
    }
    let fix = |s: String| -> String {
        if s == "+" || s.is_empty() {
            "1".to_string()
        } else if s == "-" {
            "-1".to_string()
        } else {
            s
        }
    };
    match split_at {
        None => {
            let im = js::number(&fix(body.iter().collect())).ok_or(ErrCode::NUM)?;
            Ok(Complex { re: 0.0, im, suffix })
        }
        Some(k) => {
            let re_str: String = body[..k].iter().collect();
            let im_str = fix(body[k..].iter().collect());
            match (js::number(&re_str), js::number(&im_str)) {
                (Some(re), Some(im)) => Ok(Complex { re, im, suffix }),
                _ => Err(ErrCode::NUM),
            }
        }
    }
}

fn fmt_complex(re: f64, im: f64, suffix: char) -> String {
    let r = js::round(re * 1e12) / 1e12;
    let i = js::round(im * 1e12) / 1e12;
    if i == 0.0 {
        return js::to_string(r);
    }
    let im_part = if i == 1.0 {
        suffix.to_string()
    } else if i == -1.0 {
        format!("-{suffix}")
    } else {
        format!("{}{suffix}", js::to_string(i))
    };
    if r == 0.0 {
        return im_part;
    }
    let sign = if i < 0.0 { "" } else { "+" };
    format!("{}{sign}{im_part}", js::to_string(r))
}

fn get_complex(ev: &Evaluator, n: &Node) -> Result<Complex, ErrCode> {
    parse_complex(&ev.str(n))
}

fn cplx_val(re: f64, im: f64, suffix: char) -> R {
    Ok(Value::str(fmt_complex(re, im, suffix)))
}

/// IMSUM / IMPRODUCT fold: `step` combines the accumulator with each parsed element.
fn im_fold(ev: &Evaluator, a: &[Node], init: (f64, f64), step: fn((f64, f64), &Complex) -> (f64, f64)) -> R {
    let mut acc = init;
    let mut suffix = 'i';
    for n in a {
        for s in flatten(&ev.eval(n)) {
            if let Scalar::Err(e) = &s {
                return Err(e.clone());
            }
            let c = parse_complex(&to_str(&s))?;
            acc = step(acc, &c);
            suffix = c.suffix;
        }
    }
    cplx_val(acc.0, acc.1, suffix)
}

// ── CONVERT ───────────────────────────────────────────────────────────────────

/// A `CONVERT_UNITS[name]` lookup: `Some((group, factor))`.
// QUIRK: the TS record is a plain object, so names inherited from `Object.prototype` are "units" without
// group or factor (`undefined`): converting one to itself gives NaN.
fn convert_unit(name: &str) -> Option<(Option<&'static str>, f64)> {
    Some(match name {
        "g" => (Some("mass"), 1.0),
        "kg" => (Some("mass"), 1000.0),
        "lbm" => (Some("mass"), 453.59237),
        "m" => (Some("dist"), 1.0),
        "km" => (Some("dist"), 1000.0),
        "mi" => (Some("dist"), 1609.344),
        "ft" => (Some("dist"), 0.3048),
        "in" => (Some("dist"), 0.0254),
        "sec" | "s" => (Some("time"), 1.0),
        "min" => (Some("time"), 60.0),
        "hr" => (Some("time"), 3600.0),
        "day" => (Some("time"), 86400.0),
        "constructor" | "hasOwnProperty" | "isPrototypeOf" | "propertyIsEnumerable" | "toLocaleString" | "toString" | "valueOf"
        | "__proto__" | "__defineGetter__" | "__defineSetter__" | "__lookupGetter__" | "__lookupSetter__" => (None, f64::NAN),
        _ => return None,
    })
}

fn is_temp(u: &str) -> bool {
    matches!(u, "C" | "F" | "K" | "cel" | "fah" | "kel")
}

fn temp_to_k(v: f64, unit: &str) -> f64 {
    match unit {
        "C" | "cel" => v + 273.15,
        "F" | "fah" => ((v - 32.0) * 5.0) / 9.0 + 273.15,
        _ => v,
    }
}

fn temp_from_k(k: f64, unit: &str) -> f64 {
    match unit {
        "C" | "cel" => k - 273.15,
        "F" | "fah" => ((k - 273.15) * 9.0) / 5.0 + 32.0,
        _ => k,
    }
}

fn convert(ev: &Evaluator, a: &[Node]) -> R {
    let v = ev.num(arg(a, 0)?)?;
    let from = ev.str(arg(a, 1)?);
    let to = ev.str(arg(a, 2)?);
    if is_temp(&from) || is_temp(&to) {
        if !is_temp(&from) || !is_temp(&to) {
            return Err(ErrCode::NA);
        }
        return Ok(Value::num(temp_from_k(temp_to_k(v, &from), &to)));
    }
    let (Some(uf), Some(ut)) = (convert_unit(&from), convert_unit(&to)) else { return Err(ErrCode::NA) };
    if uf.0 != ut.0 {
        return Err(ErrCode::NA);
    }
    Ok(Value::num((v * uf.1) / ut.1))
}

// ── Database (D-functions) ────────────────────────────────────────────────────

fn resolve_field(field: &Scalar, headers: &[Scalar]) -> Option<usize> {
    if let Scalar::Num(f) = field {
        let idx = js::trunc(*f) - 1.0;
        return if idx >= 0.0 && idx < headers.len() as f64 { Some(idx as usize) } else { None };
    }
    let want = js::lower(&to_str(field));
    headers.iter().position(|h| js::lower(&to_str(h)) == want)
}

/// `dbMatches`: the target field's value (possibly absent in a short row) of every matching row.
fn db_matches<'a>(database: &'a Matrix, field_idx: usize, criteria: &Matrix) -> Vec<Option<&'a Scalar>> {
    let db_headers: &[Scalar] = database.first().map(|r| r.as_slice()).unwrap_or(&[]);
    let crit_headers: &[Scalar] = criteria.first().map(|r| r.as_slice()).unwrap_or(&[]);
    let mut out = Vec::new();
    for row in database.iter().skip(1) {
        let mut row_ok = false;
        for crit_row in criteria.iter().skip(1) {
            let mut all_ok = true;
            for (cc, header) in crit_headers.iter().enumerate() {
                let Some(crit) = crit_row.get(cc) else { continue };
                if crit.is_blank() {
                    continue;
                }
                let hname = js::lower(&to_str(header));
                let Some(db_col) = db_headers.iter().position(|h| js::lower(&to_str(h)) == hname) else {
                    all_ok = false;
                    break;
                };
                if !match_criteria_opt(row.get(db_col), crit) {
                    all_ok = false;
                    break;
                }
            }
            if all_ok {
                row_ok = true;
                break;
            }
        }
        if row_ok {
            out.push(row.get(field_idx));
        }
    }
    out
}

/// The shared prologue of the D-functions: the matching cells of the target field.
fn d_cells(ev: &Evaluator, a: &[Node], f: impl FnOnce(Vec<Option<&Scalar>>) -> R) -> R {
    let database = ev.eval(arg(a, 0)?);
    let criteria = ev.eval(arg(a, 2)?);
    let (Value::M(database), Value::M(criteria)) = (&database, &criteria) else { return Err(ErrCode::VALUE) };
    let field_val = ev.scalar(arg(a, 1)?);
    if let Scalar::Err(e) = field_val {
        return Err(e);
    }
    let headers: &[Scalar] = database.first().map(|r| r.as_slice()).unwrap_or(&[]);
    let Some(field_idx) = resolve_field(&field_val, headers) else { return Err(ErrCode::VALUE) };
    f(db_matches(database, field_idx, criteria))
}

/// `dFn(agg)`: numeric aggregation over the matching cells.
fn d_fn(ev: &Evaluator, a: &[Node], agg: fn(&[f64]) -> Result<f64, ErrCode>) -> R {
    d_cells(ev, a, |cells| {
        let mut nums = Vec::new();
        for c in cells.into_iter().flatten() {
            match c {
                Scalar::Err(e) => return Err(e.clone()),
                Scalar::Num(n) => nums.push(*n),
                Scalar::Bool(b) => nums.push(if *b { 1.0 } else { 0.0 }),
                Scalar::Str(s) if !s.is_empty() => {
                    if let Some(n) = js::number(s) {
                        nums.push(n);
                    }
                }
                Scalar::Str(_) => {}
            }
        }
        agg(&nums).map(Value::num)
    })
}

fn sum(v: &[f64]) -> f64 {
    v.iter().fold(0.0, |x, y| x + y)
}

fn variance(vals: &[f64], pop: bool) -> Result<f64, ErrCode> {
    let n = vals.len();
    if n < if pop { 1 } else { 2 } {
        return Err(ErrCode::NUM);
    }
    let nf = n as f64;
    let mean = sum(vals) / nf;
    let ss = vals.iter().fold(0.0, |s, v| s + (v - mean) * (v - mean));
    Ok(ss / if pop { nf } else { nf - 1.0 })
}

fn dcounta(ev: &Evaluator, a: &[Node]) -> R {
    d_cells(ev, a, |cells| Ok(Value::num(cells.iter().filter(|c| matches!(c, Some(s) if !s.is_blank())).count() as f64)))
}

fn dget(ev: &Evaluator, a: &[Node]) -> R {
    d_cells(ev, a, |cells| {
        if cells.is_empty() {
            return Err(ErrCode::VALUE);
        }
        if cells.len() > 1 {
            return Err(ErrCode::NUM);
        }
        match cells[0] {
            Some(s) => Ok(Value::S(s.clone())),
            None => Err(ErrCode::VALUE),
        }
    })
}

// ── Registry ──────────────────────────────────────────────────────────────────

pub fn register(r: &mut Registry) {
    r.insert("BIN2DEC", |ev, a| conv(ev, a, 2, 10));
    r.insert("BIN2HEX", |ev, a| conv(ev, a, 2, 16));
    r.insert("BIN2OCT", |ev, a| conv(ev, a, 2, 8));
    r.insert("OCT2DEC", |ev, a| conv(ev, a, 8, 10));
    r.insert("OCT2BIN", |ev, a| conv(ev, a, 8, 2));
    r.insert("OCT2HEX", |ev, a| conv(ev, a, 8, 16));
    r.insert("HEX2DEC", |ev, a| conv(ev, a, 16, 10));
    r.insert("HEX2BIN", |ev, a| conv(ev, a, 16, 2));
    r.insert("HEX2OCT", |ev, a| conv(ev, a, 16, 8));
    r.insert("DEC2BIN", |ev, a| dec2(ev, a, 2));
    r.insert("DEC2OCT", |ev, a| dec2(ev, a, 8));
    r.insert("DEC2HEX", |ev, a| dec2(ev, a, 16));

    r.insert("BITAND", |ev, a| bit_op(ev, a, |p, q| p & q));
    r.insert("BITOR", |ev, a| bit_op(ev, a, |p, q| p | q));
    r.insert("BITXOR", |ev, a| bit_op(ev, a, |p, q| p ^ q));
    r.insert("BITLSHIFT", |ev, a| bit_shift(ev, a, true));
    r.insert("BITRSHIFT", |ev, a| bit_shift(ev, a, false));

    r.insert("DELTA", |ev, a| {
        let x = ev.num(arg(a, 0)?)?;
        let y = opt_num(ev, a, 1, 0.0)?;
        Ok(Value::num(if x == y { 1.0 } else { 0.0 }))
    });
    r.insert("GESTEP", |ev, a| {
        let x = ev.num(arg(a, 0)?)?;
        let step = opt_num(ev, a, 1, 0.0)?;
        Ok(Value::num(if x >= step { 1.0 } else { 0.0 }))
    });

    r.insert("CONVERT", convert);

    r.insert("ERF", |ev, a| {
        let lo = ev.num(arg(a, 0)?)?;
        if a.len() > 1 {
            let hi = ev.num(&a[1])?;
            return Ok(Value::num(erf(hi) - erf(lo)));
        }
        Ok(Value::num(erf(lo)))
    });
    r.insert("ERFC", |ev, a| Ok(Value::num(erfc(ev.num(arg(a, 0)?)?))));
    r.insert("ERF.PRECISE", |ev, a| Ok(Value::num(erf(ev.num(arg(a, 0)?)?))));
    r.insert("ERFC.PRECISE", |ev, a| Ok(Value::num(erfc(ev.num(arg(a, 0)?)?))));

    r.insert("COMPLEX", |ev, a| {
        let re = ev.num(arg(a, 0)?)?;
        let im = ev.num(arg(a, 1)?)?;
        let suffix = if a.len() > 2 { ev.str(&a[2]) } else { "i".to_string() };
        match suffix.as_str() {
            "i" => cplx_val(re, im, 'i'),
            "j" => cplx_val(re, im, 'j'),
            _ => Err(ErrCode::VALUE),
        }
    });
    r.insert("IMABS", |ev, a| {
        let c = get_complex(ev, arg(a, 0)?)?;
        Ok(Value::num((c.re * c.re + c.im * c.im).sqrt()))
    });
    r.insert("IMREAL", |ev, a| Ok(Value::num(get_complex(ev, arg(a, 0)?)?.re)));
    r.insert("IMAGINARY", |ev, a| Ok(Value::num(get_complex(ev, arg(a, 0)?)?.im)));
    r.insert("IMARGUMENT", |ev, a| {
        let c = get_complex(ev, arg(a, 0)?)?;
        if c.re == 0.0 && c.im == 0.0 {
            return Err(ErrCode::DIV0);
        }
        Ok(Value::num(c.im.atan2(c.re)))
    });
    r.insert("IMCONJUGATE", |ev, a| {
        let c = get_complex(ev, arg(a, 0)?)?;
        cplx_val(c.re, -c.im, c.suffix)
    });
    r.insert("IMSUM", |ev, a| im_fold(ev, a, (0.0, 0.0), |acc, c| (acc.0 + c.re, acc.1 + c.im)));
    r.insert("IMSUB", |ev, a| {
        let x = get_complex(ev, arg(a, 0)?)?;
        let y = get_complex(ev, arg(a, 1)?)?;
        cplx_val(x.re - y.re, x.im - y.im, x.suffix)
    });
    r.insert("IMPRODUCT", |ev, a| im_fold(ev, a, (1.0, 0.0), |(re, im), c| (re * c.re - im * c.im, re * c.im + im * c.re)));
    r.insert("IMDIV", |ev, a| {
        let x = get_complex(ev, arg(a, 0)?)?;
        let y = get_complex(ev, arg(a, 1)?)?;
        let den = y.re * y.re + y.im * y.im;
        if den == 0.0 {
            return Err(ErrCode::NUM);
        }
        cplx_val((x.re * y.re + x.im * y.im) / den, (x.im * y.re - x.re * y.im) / den, x.suffix)
    });

    r.insert("DSUM", |ev, a| d_fn(ev, a, |v| Ok(sum(v))));
    r.insert("DAVERAGE", |ev, a| d_fn(ev, a, |v| if v.is_empty() { Err(ErrCode::DIV0) } else { Ok(sum(v) / v.len() as f64) }));
    r.insert("DCOUNT", |ev, a| d_fn(ev, a, |v| Ok(v.len() as f64)));
    r.insert("DMAX", |ev, a| d_fn(ev, a, |v| Ok(if v.is_empty() { 0.0 } else { js::max(v) })));
    r.insert("DMIN", |ev, a| d_fn(ev, a, |v| Ok(if v.is_empty() { 0.0 } else { js::min(v) })));
    r.insert("DPRODUCT", |ev, a| d_fn(ev, a, |v| Ok(if v.is_empty() { 0.0 } else { v.iter().fold(1.0, |x, y| x * y) })));
    r.insert("DSTDEV", |ev, a| d_fn(ev, a, |v| variance(v, false).map(f64::sqrt)));
    r.insert("DSTDEVP", |ev, a| d_fn(ev, a, |v| variance(v, true).map(f64::sqrt)));
    r.insert("DVAR", |ev, a| d_fn(ev, a, |v| variance(v, false)));
    r.insert("DVARP", |ev, a| d_fn(ev, a, |v| variance(v, true)));
    r.insert("DCOUNTA", dcounta);
    r.insert("DGET", dget);
}
