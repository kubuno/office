//! Port of the web's `formula-fns/financial.ts`: annuities, NPV/IRR families, depreciation, rate
//! conversions, dollar fractions, simple-interest securities and T-bills.
//!
//! Conventions of the web file kept as is: a missing required argument is `#VALUE!` (the web
//! dereferences `undefined`), the `basis` argument of the simple securities is ignored (actual/360).

use super::base::num_or;
use super::Registry;
use crate::formula::ast::Node;
use crate::formula::eval::{arg, Evaluator};
use crate::formula::value::{ErrCode, Scalar, Value};
use crate::js;
use crate::jsdate;

type R = Result<Value, ErrCode>;
type N = Result<f64, ErrCode>;

fn ok(x: f64) -> R {
    Ok(Value::S(Scalar::Num(x)))
}

/// `num(ev, a[i])`: a missing argument is `#VALUE!`.
fn nm(ev: &Evaluator, a: &[Node], i: usize) -> N {
    ev.num(arg(a, i)?)
}

/// `numList(ev, args)`: the numbers of the arguments (same rules as `ev.nums`).
fn num_list(ev: &Evaluator, args: &[Node]) -> Result<Vec<f64>, ErrCode> {
    ev.nums(args)
}

/// `numList(ev, [a[i]])`.
fn num_list_at(ev: &Evaluator, a: &[Node], i: usize) -> Result<Vec<f64>, ErrCode> {
    ev.nums(std::slice::from_ref(arg(a, i)?))
}

// ── Core annuity primitives ──────────────────────────────────────────────────

fn fv_core(rate: f64, nper: f64, pmt: f64, pv: f64, ty: f64) -> f64 {
    if rate == 0.0 {
        return -(pv + pmt * nper);
    }
    let f = js::pow(1.0 + rate, nper);
    -(pv * f + pmt * (1.0 + rate * ty) * (f - 1.0) / rate)
}

fn pv_core(rate: f64, nper: f64, pmt: f64, fv: f64, ty: f64) -> f64 {
    if rate == 0.0 {
        return -(fv + pmt * nper);
    }
    let f = js::pow(1.0 + rate, nper);
    -(fv + pmt * (1.0 + rate * ty) * (f - 1.0) / rate) / f
}

fn pmt_core(rate: f64, nper: f64, pv: f64, fv: f64, ty: f64) -> f64 {
    if rate == 0.0 {
        return -(pv + fv) / nper;
    }
    let f = js::pow(1.0 + rate, nper);
    -(pv * f + fv) * rate / ((1.0 + rate * ty) * (f - 1.0))
}

fn ipmt_core(rate: f64, per: f64, nper: f64, pv: f64, fv: f64, ty: f64) -> f64 {
    let pmt = pmt_core(rate, nper, pv, fv, ty);
    if per == 1.0 {
        if ty == 1.0 {
            0.0
        } else {
            -pv * rate
        }
    } else {
        let fv_prev = fv_core(rate, per - 1.0, pmt, pv, ty);
        if ty == 1.0 {
            -fv_prev * rate / (1.0 + rate)
        } else {
            -fv_prev * rate
        }
    }
}

// ── Annuity functions ────────────────────────────────────────────────────────

fn pmt(ev: &Evaluator, a: &[Node]) -> R {
    let rate = nm(ev, a, 0)?;
    let nper = nm(ev, a, 1)?;
    let pv = nm(ev, a, 2)?;
    let fv = num_or(ev, a, 3, 0.0)?;
    let ty = num_or(ev, a, 4, 0.0)?;
    if nper == 0.0 {
        return Err(ErrCode::NUM);
    }
    ok(pmt_core(rate, nper, pv, fv, ty))
}

fn fv(ev: &Evaluator, a: &[Node]) -> R {
    let rate = nm(ev, a, 0)?;
    let nper = nm(ev, a, 1)?;
    let pmt = nm(ev, a, 2)?;
    let pv = num_or(ev, a, 3, 0.0)?;
    let ty = num_or(ev, a, 4, 0.0)?;
    ok(fv_core(rate, nper, pmt, pv, ty))
}

fn pv(ev: &Evaluator, a: &[Node]) -> R {
    let rate = nm(ev, a, 0)?;
    let nper = nm(ev, a, 1)?;
    let pmt = nm(ev, a, 2)?;
    let fv = num_or(ev, a, 3, 0.0)?;
    let ty = num_or(ev, a, 4, 0.0)?;
    ok(pv_core(rate, nper, pmt, fv, ty))
}

fn nper(ev: &Evaluator, a: &[Node]) -> R {
    let rate = nm(ev, a, 0)?;
    let pmt = nm(ev, a, 1)?;
    let pv = nm(ev, a, 2)?;
    let fv = num_or(ev, a, 3, 0.0)?;
    let ty = num_or(ev, a, 4, 0.0)?;
    if rate == 0.0 {
        if pmt == 0.0 {
            return Err(ErrCode::NUM);
        }
        return ok(-(pv + fv) / pmt);
    }
    let adj = pmt * (1.0 + rate * ty) / rate;
    let ratio = (adj - fv) / (pv + adj);
    if ratio.is_nan() || ratio <= 0.0 {
        return Err(ErrCode::NUM);
    }
    ok(ratio.ln() / (1.0 + rate).ln())
}

fn rate(ev: &Evaluator, a: &[Node]) -> R {
    let nper = nm(ev, a, 0)?;
    let pmt = nm(ev, a, 1)?;
    let pv = nm(ev, a, 2)?;
    let fv = num_or(ev, a, 3, 0.0)?;
    let ty = num_or(ev, a, 4, 0.0)?;
    let guess = num_or(ev, a, 5, 0.1)?;
    // Newton's method with a numerical derivative.
    let f = |r: f64| fv_core(r, nper, pmt, pv, ty) - fv;
    let mut r = guess;
    for _ in 0..100 {
        let fr = f(r);
        if fr.abs() < 1e-8 {
            return ok(r);
        }
        let dr = 1e-6;
        let dfr = (f(r + dr) - f(r - dr)) / (2.0 * dr);
        if dfr == 0.0 {
            break;
        }
        let next = r - fr / dfr;
        if !next.is_finite() {
            break;
        }
        if (next - r).abs() < 1e-10 {
            r = next;
            if f(r).abs() < 1e-6 {
                return ok(r);
            }
            break;
        }
        r = next;
    }
    if f(r).abs() < 1e-6 {
        ok(r)
    } else {
        Err(ErrCode::NUM)
    }
}

fn ipmt(ev: &Evaluator, a: &[Node]) -> R {
    let rate = nm(ev, a, 0)?;
    let per = nm(ev, a, 1)?;
    let nper = nm(ev, a, 2)?;
    let pv = nm(ev, a, 3)?;
    let fv = num_or(ev, a, 4, 0.0)?;
    let ty = num_or(ev, a, 5, 0.0)?;
    if per < 1.0 || per > nper {
        return Err(ErrCode::NUM);
    }
    ok(ipmt_core(rate, per, nper, pv, fv, ty))
}

fn ppmt(ev: &Evaluator, a: &[Node]) -> R {
    let rate = nm(ev, a, 0)?;
    let per = nm(ev, a, 1)?;
    let nper = nm(ev, a, 2)?;
    let pv = nm(ev, a, 3)?;
    let fv = num_or(ev, a, 4, 0.0)?;
    let ty = num_or(ev, a, 5, 0.0)?;
    if per < 1.0 || per > nper {
        return Err(ErrCode::NUM);
    }
    let pmt = pmt_core(rate, nper, pv, fv, ty);
    let ipmt = ipmt_core(rate, per, nper, pv, fv, ty);
    ok(pmt - ipmt)
}

fn cumipmt(ev: &Evaluator, a: &[Node]) -> R {
    let rate = nm(ev, a, 0)?;
    let nper = nm(ev, a, 1)?;
    let pv = nm(ev, a, 2)?;
    let start = nm(ev, a, 3)?;
    let end = nm(ev, a, 4)?;
    let ty = num_or(ev, a, 5, 0.0)?;
    if rate <= 0.0 || nper <= 0.0 || pv <= 0.0 || start < 1.0 || end < start || end > nper {
        return Err(ErrCode::NUM);
    }
    let mut total = 0.0;
    let mut p = start.floor();
    let last = end.floor();
    while p <= last {
        total += ipmt_core(rate, p, nper, pv, 0.0, ty);
        p += 1.0;
    }
    ok(total)
}

fn cumprinc(ev: &Evaluator, a: &[Node]) -> R {
    let rate = nm(ev, a, 0)?;
    let nper = nm(ev, a, 1)?;
    let pv = nm(ev, a, 2)?;
    let start = nm(ev, a, 3)?;
    let end = nm(ev, a, 4)?;
    let ty = num_or(ev, a, 5, 0.0)?;
    if rate <= 0.0 || nper <= 0.0 || pv <= 0.0 || start < 1.0 || end < start || end > nper {
        return Err(ErrCode::NUM);
    }
    let pmt = pmt_core(rate, nper, pv, 0.0, ty);
    let mut total = 0.0;
    let mut p = start.floor();
    let last = end.floor();
    while p <= last {
        total += pmt - ipmt_core(rate, p, nper, pv, 0.0, ty);
        p += 1.0;
    }
    ok(total)
}

fn ispmt(ev: &Evaluator, a: &[Node]) -> R {
    let rate = nm(ev, a, 0)?;
    let per = nm(ev, a, 1)?;
    let nper = nm(ev, a, 2)?;
    let pv = nm(ev, a, 3)?;
    if nper == 0.0 {
        return Err(ErrCode::NUM);
    }
    ok(-pv * rate * (1.0 - per / nper))
}

// ── NPV / IRR family ─────────────────────────────────────────────────────────

fn npv(ev: &Evaluator, a: &[Node]) -> R {
    let rate = nm(ev, a, 0)?;
    let values = num_list(ev, a.get(1..).unwrap_or(&[]))?;
    if rate == -1.0 {
        return Err(ErrCode::DIV0);
    }
    let mut total = 0.0;
    for (i, v) in values.iter().enumerate() {
        total += v / js::pow(1.0 + rate, (i + 1) as f64);
    }
    ok(total)
}

fn irr_npv(rate: f64, cf: &[f64]) -> f64 {
    let mut s = 0.0;
    for (i, c) in cf.iter().enumerate() {
        s += c / js::pow(1.0 + rate, i as f64);
    }
    s
}

/// The shared Newton-then-bisection scheme of IRR and XIRR (`f` is the NPV at a rate).
fn newton_bisect(guess: f64, f: &dyn Fn(f64) -> f64) -> N {
    let mut r = guess;
    for _ in 0..100 {
        let fv = f(r);
        if fv.abs() < 1e-7 {
            return Ok(r);
        }
        let dr = 1e-6;
        let d = (f(r + dr) - f(r - dr)) / (2.0 * dr);
        if d == 0.0 {
            break;
        }
        let next = r - fv / d;
        if !next.is_finite() || next <= -1.0 {
            break;
        }
        if (next - r).abs() < 1e-10 {
            r = next;
            break;
        }
        r = next;
    }
    if f(r).abs() < 1e-6 && r > -1.0 {
        return Ok(r);
    }
    let mut lo = -0.9999;
    let mut hi = 10.0;
    let mut flo = f(lo);
    let fhi = f(hi);
    if flo * fhi > 0.0 {
        return Err(ErrCode::NUM);
    }
    for _ in 0..200 {
        let mid = (lo + hi) / 2.0;
        let fm = f(mid);
        if fm.abs() < 1e-9 {
            return Ok(mid);
        }
        if flo * fm < 0.0 {
            hi = mid;
        } else {
            lo = mid;
            flo = fm;
        }
    }
    Ok((lo + hi) / 2.0)
}

fn irr(ev: &Evaluator, a: &[Node]) -> R {
    let cf = num_list_at(ev, a, 0)?;
    if cf.len() < 2 {
        return Err(ErrCode::NUM);
    }
    let guess = num_or(ev, a, 1, 0.1)?;
    ok(newton_bisect(guess, &|r| irr_npv(r, &cf))?)
}

fn mirr(ev: &Evaluator, a: &[Node]) -> R {
    let cf = num_list_at(ev, a, 0)?;
    let finance_rate = nm(ev, a, 1)?;
    let reinvest_rate = nm(ev, a, 2)?;
    let n = cf.len();
    if n < 2 {
        return Err(ErrCode::NUM);
    }
    let mut pv_neg = 0.0;
    let mut fv_pos = 0.0;
    for (i, c) in cf.iter().enumerate() {
        if *c < 0.0 {
            pv_neg += c / js::pow(1.0 + finance_rate, i as f64);
        } else {
            fv_pos += c * js::pow(1.0 + reinvest_rate, (n - 1 - i) as f64);
        }
    }
    if pv_neg == 0.0 || fv_pos == 0.0 {
        return Err(ErrCode::DIV0);
    }
    ok(js::pow(-fv_pos / pv_neg, 1.0 / (n - 1) as f64) - 1.0)
}

/// `readDatedFlows`: values and whole-day serials (`#NUM!` when the lengths differ or are below 2).
fn read_dated_flows(ev: &Evaluator, a: &[Node], vi: usize, di: usize) -> Result<(Vec<f64>, Vec<f64>), ErrCode> {
    let v = num_list_at(ev, a, vi)?;
    let draw = num_list_at(ev, a, di)?;
    if v.len() != draw.len() || v.len() < 2 {
        return Err(ErrCode::NUM);
    }
    let d = draw.iter().map(|x| x.floor()).collect();
    Ok((v, d))
}

fn xnpv_core(rate: f64, v: &[f64], d: &[f64]) -> f64 {
    let d0 = d[0];
    let mut s = 0.0;
    for i in 0..v.len() {
        s += v[i] / js::pow(1.0 + rate, (d[i] - d0) / 365.0);
    }
    s
}

fn xnpv(ev: &Evaluator, a: &[Node]) -> R {
    let rate = nm(ev, a, 0)?;
    let (v, d) = read_dated_flows(ev, a, 1, 2)?;
    if rate <= -1.0 {
        return Err(ErrCode::NUM);
    }
    ok(xnpv_core(rate, &v, &d))
}

fn xirr(ev: &Evaluator, a: &[Node]) -> R {
    let (v, d) = read_dated_flows(ev, a, 0, 1)?;
    let guess = num_or(ev, a, 2, 0.1)?;
    ok(newton_bisect(guess, &|r| xnpv_core(r, &v, &d))?)
}

fn fvschedule(ev: &Evaluator, a: &[Node]) -> R {
    let principal = nm(ev, a, 0)?;
    let rates = num_list(ev, a.get(1..).unwrap_or(&[]))?;
    let mut v = principal;
    for r in rates {
        v *= 1.0 + r;
    }
    ok(v)
}

// ── Depreciation ─────────────────────────────────────────────────────────────

fn sln(ev: &Evaluator, a: &[Node]) -> R {
    let cost = nm(ev, a, 0)?;
    let salvage = nm(ev, a, 1)?;
    let life = nm(ev, a, 2)?;
    if life == 0.0 {
        return Err(ErrCode::DIV0);
    }
    ok((cost - salvage) / life)
}

fn syd(ev: &Evaluator, a: &[Node]) -> R {
    let cost = nm(ev, a, 0)?;
    let salvage = nm(ev, a, 1)?;
    let life = nm(ev, a, 2)?;
    let per = nm(ev, a, 3)?;
    if life <= 0.0 || per < 1.0 || per > life {
        return Err(ErrCode::NUM);
    }
    ok((cost - salvage) * (life - per + 1.0) * 2.0 / (life * (life + 1.0)))
}

fn db(ev: &Evaluator, a: &[Node]) -> R {
    let cost = nm(ev, a, 0)?;
    let salvage = nm(ev, a, 1)?;
    let life = nm(ev, a, 2)?;
    let period = nm(ev, a, 3)?;
    let month = num_or(ev, a, 4, 12.0)?;
    if life <= 0.0 || cost < 0.0 || period < 1.0 || month < 1.0 || month > 12.0 {
        return Err(ErrCode::NUM);
    }
    if cost == 0.0 {
        return ok(0.0);
    }
    let rate = js::round((1.0 - js::pow(salvage / cost, 1.0 / life)) * 1000.0) / 1000.0;
    let first_year = cost * rate * month / 12.0;
    if period == 1.0 {
        return ok(first_year);
    }
    let mut total = first_year;
    let mut dep = first_year;
    let last = period.floor();
    let mut p = 2.0;
    while p <= last {
        dep = (cost - total) * rate;
        if p == last {
            // QUIRK: the partial-year rule of year life+1 only applies when `period` lands exactly on it.
            if p == life + 1.0 {
                return ok((cost - total) * rate * (12.0 - month) / 12.0);
            }
            return ok(dep);
        }
        total += dep;
        p += 1.0;
    }
    ok(dep)
}

fn ddb(ev: &Evaluator, a: &[Node]) -> R {
    let cost = nm(ev, a, 0)?;
    let salvage = nm(ev, a, 1)?;
    let life = nm(ev, a, 2)?;
    let period = nm(ev, a, 3)?;
    let factor = num_or(ev, a, 4, 2.0)?;
    if life <= 0.0 || period < 1.0 || period > life || factor <= 0.0 {
        return Err(ErrCode::NUM);
    }
    let mut total = 0.0;
    let mut dep = 0.0;
    let last = period.floor();
    let mut p = 1.0;
    while p <= last {
        dep = js::min(&[(cost - total) * factor / life, js::max(&[cost - salvage - total, 0.0])]);
        if dep < 0.0 {
            dep = 0.0;
        }
        total += dep;
        p += 1.0;
    }
    ok(dep)
}

fn vdb(ev: &Evaluator, a: &[Node]) -> R {
    let cost = nm(ev, a, 0)?;
    let salvage = nm(ev, a, 1)?;
    let life = nm(ev, a, 2)?;
    let start = nm(ev, a, 3)?;
    let end = nm(ev, a, 4)?;
    let factor = num_or(ev, a, 5, 2.0)?;
    let no_switch = num_or(ev, a, 6, 0.0)?;
    if life <= 0.0 || start < 0.0 || end < start || end > life || factor <= 0.0 {
        return Err(ErrCode::NUM);
    }
    let mut book = cost;
    let mut switched = false;
    let end_i = end.ceil();
    let mut sum = 0.0;
    let mut p = 1.0;
    while p <= end_i {
        let remaining = book - salvage;
        let ddb = js::min(&[book * factor / life, js::max(&[remaining, 0.0])]);
        let dep;
        if no_switch == 0.0 {
            let sl_periods_left = life - (p - 1.0);
            let sl_dep = if sl_periods_left > 0.0 { remaining / sl_periods_left } else { 0.0 };
            if switched || sl_dep > ddb {
                switched = true;
                dep = js::max(&[js::min(&[sl_dep, remaining]), 0.0]);
            } else {
                dep = ddb;
            }
        } else {
            dep = ddb;
        }
        book -= dep;
        let lo = js::max(&[p - 1.0, start]);
        let hi = js::min(&[p, end]);
        if hi > lo {
            sum += dep * (hi - lo);
        }
        p += 1.0;
    }
    ok(sum)
}

// ── Rate conversions & growth ────────────────────────────────────────────────

fn effect(ev: &Evaluator, a: &[Node]) -> R {
    let nominal = nm(ev, a, 0)?;
    let npery = nm(ev, a, 1)?;
    let n = npery.floor();
    if nominal <= 0.0 || n < 1.0 {
        return Err(ErrCode::NUM);
    }
    ok(js::pow(1.0 + nominal / n, n) - 1.0)
}

fn nominal(ev: &Evaluator, a: &[Node]) -> R {
    let effect = nm(ev, a, 0)?;
    let npery = nm(ev, a, 1)?;
    let n = npery.floor();
    if effect <= 0.0 || n < 1.0 {
        return Err(ErrCode::NUM);
    }
    ok((js::pow(1.0 + effect, 1.0 / n) - 1.0) * n)
}

fn rri(ev: &Evaluator, a: &[Node]) -> R {
    let nper = nm(ev, a, 0)?;
    let pv = nm(ev, a, 1)?;
    let fv = nm(ev, a, 2)?;
    if nper <= 0.0 || pv == 0.0 {
        return Err(ErrCode::NUM);
    }
    ok(js::pow(fv / pv, 1.0 / nper) - 1.0)
}

fn pduration(ev: &Evaluator, a: &[Node]) -> R {
    let rate = nm(ev, a, 0)?;
    let pv = nm(ev, a, 1)?;
    let fv = nm(ev, a, 2)?;
    if rate <= 0.0 || pv <= 0.0 || fv <= 0.0 {
        return Err(ErrCode::NUM);
    }
    ok((fv.ln() - pv.ln()) / (1.0 + rate).ln())
}

// ── Dollar fraction conversions ──────────────────────────────────────────────

/// `Math.ceil(Math.log10(f) || 1)`.
fn fraction_digits(f: f64) -> f64 {
    let l = f.log10();
    (if l == 0.0 || l.is_nan() { 1.0 } else { l }).ceil()
}

fn dollarde(ev: &Evaluator, a: &[Node]) -> R {
    let fractional = nm(ev, a, 0)?;
    let fraction = nm(ev, a, 1)?;
    let f = fraction.floor();
    if f < 0.0 {
        return Err(ErrCode::NUM);
    }
    if f == 0.0 {
        return Err(ErrCode::DIV0);
    }
    let int_part = js::trunc(fractional);
    let frac = fractional - int_part;
    let digits = fraction_digits(f);
    ok(int_part + (frac * js::pow(10.0, digits)) / f)
}

fn dollarfr(ev: &Evaluator, a: &[Node]) -> R {
    let decimal = nm(ev, a, 0)?;
    let fraction = nm(ev, a, 1)?;
    let f = fraction.floor();
    if f < 0.0 {
        return Err(ErrCode::NUM);
    }
    if f == 0.0 {
        return Err(ErrCode::DIV0);
    }
    let int_part = js::trunc(decimal);
    let frac = decimal - int_part;
    let digits = fraction_digits(f);
    ok(int_part + (frac * f) / js::pow(10.0, digits))
}

// ── Simple-interest securities (actual/360; basis ignored) ───────────────────

/// `daysBetween`: whole-serial day difference, `None` when a serial is not a valid date.
fn days_between(d1: f64, d2: f64) -> Option<f64> {
    let a = jsdate::serial_to_date(d1.floor())?;
    let b = jsdate::serial_to_date(d2.floor())?;
    Some((b - a) / 86_400_000.0)
}

fn accrintm(ev: &Evaluator, a: &[Node]) -> R {
    let issue = nm(ev, a, 0)?;
    let settlement = nm(ev, a, 1)?;
    let rate = nm(ev, a, 2)?;
    let par = nm(ev, a, 3)?;
    if rate <= 0.0 || par <= 0.0 {
        return Err(ErrCode::NUM);
    }
    let days = match days_between(issue, settlement) {
        Some(d) if d.is_nan() || d >= 0.0 => d,
        _ => return Err(ErrCode::NUM),
    };
    ok(par * rate * (days / 360.0))
}

/// Days from settlement to maturity, `#NUM!` unless strictly positive.
fn positive_days(settlement: f64, maturity: f64) -> N {
    match days_between(settlement, maturity) {
        Some(d) if d.is_nan() || d > 0.0 => Ok(d),
        _ => Err(ErrCode::NUM),
    }
}

fn disc(ev: &Evaluator, a: &[Node]) -> R {
    let settlement = nm(ev, a, 0)?;
    let maturity = nm(ev, a, 1)?;
    let pr = nm(ev, a, 2)?;
    let redemption = nm(ev, a, 3)?;
    if pr <= 0.0 || redemption <= 0.0 {
        return Err(ErrCode::NUM);
    }
    let days = positive_days(settlement, maturity)?;
    ok((redemption - pr) / redemption * (360.0 / days))
}

fn intrate(ev: &Evaluator, a: &[Node]) -> R {
    let settlement = nm(ev, a, 0)?;
    let maturity = nm(ev, a, 1)?;
    let investment = nm(ev, a, 2)?;
    let redemption = nm(ev, a, 3)?;
    if investment <= 0.0 || redemption <= 0.0 {
        return Err(ErrCode::NUM);
    }
    let days = positive_days(settlement, maturity)?;
    ok((redemption - investment) / investment * (360.0 / days))
}

fn received(ev: &Evaluator, a: &[Node]) -> R {
    let settlement = nm(ev, a, 0)?;
    let maturity = nm(ev, a, 1)?;
    let investment = nm(ev, a, 2)?;
    let discount = nm(ev, a, 3)?;
    if investment <= 0.0 || discount <= 0.0 {
        return Err(ErrCode::NUM);
    }
    let days = positive_days(settlement, maturity)?;
    let denom = 1.0 - discount * days / 360.0;
    if denom == 0.0 {
        return Err(ErrCode::DIV0);
    }
    ok(investment / denom)
}

// ── Treasury bills ───────────────────────────────────────────────────────────

/// Days of a T-bill: strictly positive and at most 365, else `#NUM!`.
fn tbill_days(settlement: f64, maturity: f64) -> N {
    match days_between(settlement, maturity) {
        Some(d) if d.is_nan() || (d > 0.0 && d <= 365.0) => Ok(d),
        _ => Err(ErrCode::NUM),
    }
}

fn tbilleq(ev: &Evaluator, a: &[Node]) -> R {
    let settlement = nm(ev, a, 0)?;
    let maturity = nm(ev, a, 1)?;
    let discount = nm(ev, a, 2)?;
    if discount <= 0.0 {
        return Err(ErrCode::NUM);
    }
    let days = tbill_days(settlement, maturity)?;
    let denom = 360.0 - discount * days;
    if denom <= 0.0 {
        return Err(ErrCode::NUM);
    }
    ok((365.0 * discount) / denom)
}

fn tbillprice(ev: &Evaluator, a: &[Node]) -> R {
    let settlement = nm(ev, a, 0)?;
    let maturity = nm(ev, a, 1)?;
    let discount = nm(ev, a, 2)?;
    if discount <= 0.0 {
        return Err(ErrCode::NUM);
    }
    let days = tbill_days(settlement, maturity)?;
    ok(100.0 * (1.0 - discount * days / 360.0))
}

fn tbillyield(ev: &Evaluator, a: &[Node]) -> R {
    let settlement = nm(ev, a, 0)?;
    let maturity = nm(ev, a, 1)?;
    let pr = nm(ev, a, 2)?;
    if pr <= 0.0 {
        return Err(ErrCode::NUM);
    }
    let days = tbill_days(settlement, maturity)?;
    ok(((100.0 - pr) / pr) * (360.0 / days))
}

pub fn register(r: &mut Registry) {
    let list: &[(&'static str, crate::formula::eval::Fn)] = &[
        ("PMT", pmt),
        ("IPMT", ipmt),
        ("PPMT", ppmt),
        ("FV", fv),
        ("PV", pv),
        ("NPER", nper),
        ("RATE", rate),
        ("NPV", npv),
        ("IRR", irr),
        ("MIRR", mirr),
        ("XIRR", xirr),
        ("XNPV", xnpv),
        ("FVSCHEDULE", fvschedule),
        ("SLN", sln),
        ("SYD", syd),
        ("DB", db),
        ("DDB", ddb),
        ("VDB", vdb),
        ("CUMIPMT", cumipmt),
        ("CUMPRINC", cumprinc),
        ("EFFECT", effect),
        ("NOMINAL", nominal),
        ("RRI", rri),
        ("PDURATION", pduration),
        ("ISPMT", ispmt),
        ("DOLLARDE", dollarde),
        ("DOLLARFR", dollarfr),
        ("ACCRINTM", accrintm),
        ("DISC", disc),
        ("INTRATE", intrate),
        ("RECEIVED", received),
        ("TBILLEQ", tbilleq),
        ("TBILLPRICE", tbillprice),
        ("TBILLYIELD", tbillyield),
    ];
    for (name, f) in list {
        r.insert(name, *f);
    }
}
