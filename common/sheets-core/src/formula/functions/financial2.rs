//! Port of the web's `formula-fns/financial2.ts`: bond pricing, coupon schedules, day-count bases,
//! accrued interest, duration, odd-period bonds and French depreciation.
//!
//! Dates are JavaScript time values (`f64` milliseconds, NaN = invalid `Date`) as in `jsdate`.

use super::base::{index, num_or};
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

// â”€â”€ Date utilities â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// `readDate`: the serial (floored) as a date, `None` when invalid.
fn read_date(d: f64) -> Option<f64> {
    jsdate::serial_to_date(d.floor())
}

/// `(getFullYear(), getMonth(), getDate())`, all NaN for an invalid date.
fn comps(t: f64) -> (f64, f64, f64) {
    match jsdate::parts(t) {
        Some(p) => (p.year as f64, p.month as f64, p.date as f64),
        None => (f64::NAN, f64::NAN, f64::NAN),
    }
}

fn serial(t: f64) -> f64 {
    jsdate::excel_serial(t)
}

fn is_leap(y: f64) -> bool {
    (y % 4.0 == 0.0 && y % 100.0 != 0.0) || y % 400.0 == 0.0
}

/// `daysInMonth(y, m)` with `m` 1-based; `NaN` where the web indexes outside the table (`undefined`).
fn days_in_month(y: f64, m: f64) -> f64 {
    let table = [31.0, if is_leap(y) { 29.0 } else { 28.0 }, 31.0, 30.0, 31.0, 30.0, 31.0, 31.0, 30.0, 31.0, 30.0, 31.0];
    index(m - 1.0).and_then(|i| table.get(i)).copied().unwrap_or(f64::NAN)
}

fn is_last_day_of_month(d: f64) -> bool {
    let (y, m, dt) = comps(d);
    dt == days_in_month(y, m + 1.0)
}

fn is_last_day_of_feb(d: f64) -> bool {
    let (_, m, _) = comps(d);
    m == 1.0 && is_last_day_of_month(d)
}

/// Actual calendar days between two dates (end - start).
fn actual_days(start: f64, end: f64) -> f64 {
    js::round((serial(end) - serial(start)) / 1.0)
}

/// Adds whole months, clamping the day to the target month length.
fn add_months(d: f64, months: f64) -> f64 {
    let (y0, m0, day0) = comps(d);
    let total = y0 * 12.0 + m0 + months;
    let y = (total / 12.0).floor();
    let m = total % 12.0;
    let day = js::min(&[day0, days_in_month(y, m + 1.0)]);
    jsdate::ymd(y, m, day)
}

// â”€â”€ Day-count engine â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

fn day_count(start: f64, end: f64, basis: f64) -> f64 {
    let (y1, mo1, mut d1) = comps(start);
    let (y2, mo2, mut d2) = comps(end);
    let m1 = mo1 + 1.0;
    let m2 = mo2 + 1.0;
    if basis == 0.0 {
        if is_last_day_of_feb(start) && is_last_day_of_feb(end) {
            d2 = 30.0;
        }
        if is_last_day_of_feb(start) {
            d1 = 30.0;
        }
        if d2 == 31.0 && (d1 == 30.0 || d1 == 31.0) {
            d2 = 30.0;
        }
        if d1 == 31.0 {
            d1 = 30.0;
        }
        (y2 - y1) * 360.0 + (m2 - m1) * 30.0 + (d2 - d1)
    } else if basis == 4.0 {
        if d1 == 31.0 {
            d1 = 30.0;
        }
        if d2 == 31.0 {
            d2 = 30.0;
        }
        (y2 - y1) * 360.0 + (m2 - m1) * 30.0 + (d2 - d1)
    } else {
        actual_days(start, end)
    }
}

fn basis_denominator(basis: f64) -> f64 {
    if basis == 3.0 {
        365.0
    } else {
        360.0
    }
}

/// Excel YEARFRAC with the five bases.
fn year_frac(start: f64, end: f64, basis: f64) -> N {
    if basis < 0.0 || basis > 4.0 {
        return Err(ErrCode::NUM);
    }
    if serial(start) == serial(end) {
        return Ok(0.0);
    }
    if basis == 1.0 {
        let mut a = start;
        let mut b = end;
        let mut sign = 1.0;
        if serial(a) > serial(b) {
            std::mem::swap(&mut a, &mut b);
            sign = -1.0;
        }
        let days = actual_days(a, b);
        let y1 = comps(a).0;
        let y2 = comps(b).0;
        let denom = if y1 == y2 {
            if is_leap(y1) {
                366.0
            } else {
                365.0
            }
        } else {
            let years = y2 - y1 + 1.0;
            let mut total = 0.0;
            let mut y = y1;
            while y <= y2 {
                total += if is_leap(y) { 366.0 } else { 365.0 };
                y += 1.0;
            }
            total / years
        };
        return Ok((sign * days) / denom);
    }
    Ok(day_count(start, end, basis) / basis_denominator(basis))
}

// â”€â”€ Coupon schedule â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

fn prev_coupon_index(settlement: f64, maturity: f64, frequency: f64) -> f64 {
    let step = 12.0 / frequency;
    let mut k = 0.0;
    while serial(add_months(maturity, -step * (k + 1.0))) > serial(settlement) {
        k += 1.0;
        if k > 100000.0 {
            break;
        }
    }
    k + 1.0
}

fn coupon_prev(settlement: f64, maturity: f64, frequency: f64) -> f64 {
    let step = 12.0 / frequency;
    add_months(maturity, -step * prev_coupon_index(settlement, maturity, frequency))
}

fn coupon_next(settlement: f64, maturity: f64, frequency: f64) -> f64 {
    let step = 12.0 / frequency;
    add_months(maturity, -step * (prev_coupon_index(settlement, maturity, frequency) - 1.0))
}

fn freq_ok(f: f64) -> bool {
    f == 1.0 || f == 2.0 || f == 4.0
}

fn basis_ok(b: f64) -> bool {
    (0.0..=4.0).contains(&b)
}

/// Days in the coupon period containing settlement.
fn coup_days(settlement: f64, maturity: f64, frequency: f64, basis: f64) -> f64 {
    if basis == 1.0 {
        let prev = coupon_prev(settlement, maturity, frequency);
        let next = coupon_next(settlement, maturity, frequency);
        return actual_days(prev, next);
    }
    if basis == 3.0 {
        return 365.0 / frequency;
    }
    360.0 / frequency
}

/// Number of coupons payable between settlement and maturity (COUPNUM logic).
fn coup_num_between(settlement: f64, maturity: f64, frequency: f64) -> f64 {
    let prev = coupon_prev(settlement, maturity, frequency);
    let step = 12.0 / frequency;
    let mut count = 0.0;
    let mut k = 1.0;
    loop {
        let cand = add_months(prev, step * k);
        if serial(cand) > serial(maturity) {
            break;
        }
        count += 1.0;
        k += 1.0;
        if k > 100000.0 {
            break;
        }
    }
    count
}

/// Days from settlement to the next coupon (COUPDAYSNC logic).
fn coup_days_settle_next(settlement: f64, maturity: f64, frequency: f64, basis: f64) -> f64 {
    if basis == 0.0 || basis == 4.0 {
        let prev = coupon_prev(settlement, maturity, frequency);
        return coup_days(settlement, maturity, frequency, basis) - day_count(prev, settlement, basis);
    }
    let next = coupon_next(settlement, maturity, frequency);
    day_count(settlement, next, basis)
}

// â”€â”€ Argument helpers â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// `readSettleMaturity(num(ev, a[0]), num(ev, a[1]))`: both arguments are evaluated (a missing second
/// one throws `#VALUE!` even when the first is an error) before any error is returned.
fn read_settle_maturity(ev: &Evaluator, a: &[Node]) -> Result<(f64, f64), ErrCode> {
    let n0 = arg(a, 0)?;
    let s = ev.num(n0);
    let n1 = arg(a, 1)?;
    let m = ev.num(n1);
    let s = s?;
    let m = m?;
    let (Some(settlement), Some(maturity)) = (read_date(s), read_date(m)) else {
        return Err(ErrCode::VALUE);
    };
    if serial(settlement) >= serial(maturity) {
        return Err(ErrCode::NUM);
    }
    Ok((settlement, maturity))
}

/// The common prologue of COUPNCD..COUPNUM: dates, frequency, basis, validation.
fn coupon_args(ev: &Evaluator, a: &[Node]) -> Result<(f64, f64, f64, f64), ErrCode> {
    let (s, m) = read_settle_maturity(ev, a)?;
    let f = nm(ev, a, 2)?;
    let b = num_or(ev, a, 3, 0.0)?;
    if !freq_ok(f) || !basis_ok(b) {
        return Err(ErrCode::NUM);
    }
    Ok((s, m, f, b))
}

// â”€â”€ COUPON functions â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

fn coupncd(ev: &Evaluator, a: &[Node]) -> R {
    let (s, m, f, _) = coupon_args(ev, a)?;
    ok(serial(coupon_next(s, m, f)))
}

fn couppcd(ev: &Evaluator, a: &[Node]) -> R {
    let (s, m, f, _) = coupon_args(ev, a)?;
    ok(serial(coupon_prev(s, m, f)))
}

fn coupdaybs(ev: &Evaluator, a: &[Node]) -> R {
    let (s, m, f, b) = coupon_args(ev, a)?;
    let prev = coupon_prev(s, m, f);
    ok(day_count(prev, s, b))
}

fn coupdaysnc(ev: &Evaluator, a: &[Node]) -> R {
    let (s, m, f, b) = coupon_args(ev, a)?;
    let next = coupon_next(s, m, f);
    if b == 0.0 || b == 4.0 {
        let prev = coupon_prev(s, m, f);
        return ok(coup_days(s, m, f, b) - day_count(prev, s, b));
    }
    ok(day_count(s, next, b))
}

fn coupdays(ev: &Evaluator, a: &[Node]) -> R {
    let (s, m, f, b) = coupon_args(ev, a)?;
    ok(coup_days(s, m, f, b))
}

fn coupnum(ev: &Evaluator, a: &[Node]) -> R {
    let (s, m, f, _) = coupon_args(ev, a)?;
    ok(coup_num_between(s, m, f))
}

// â”€â”€ ACCRINT â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

fn accrint(ev: &Evaluator, a: &[Node]) -> R {
    let issue_n = nm(ev, a, 0)?;
    let first_n = nm(ev, a, 1)?;
    let settle_n = nm(ev, a, 2)?;
    let rate = nm(ev, a, 3)?;
    let par = nm(ev, a, 4)?;
    let f = nm(ev, a, 5)?;
    let b = num_or(ev, a, 6, 0.0)?;
    let calc_method = num_or(ev, a, 7, 1.0)?;

    let (Some(issue), Some(first), Some(settle)) = (read_date(issue_n), read_date(first_n), read_date(settle_n)) else {
        return Err(ErrCode::VALUE);
    };
    if !freq_ok(f) || !basis_ok(b) {
        return Err(ErrCode::NUM);
    }
    if rate <= 0.0 || par <= 0.0 {
        return Err(ErrCode::NUM);
    }
    if serial(issue) >= serial(settle) {
        return Err(ErrCode::NUM);
    }
    let mut acc_start = issue;
    if calc_method == 0.0 && serial(settle) > serial(first) {
        acc_start = first;
    }
    let yf = year_frac(acc_start, settle, b)?;
    ok(par * rate * yf)
}

// â”€â”€ PRICE / YIELD â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Core PRICE computation: price per 100 face value.
fn price_core(settlement: f64, maturity: f64, rate: f64, yld: f64, redemption: f64, frequency: f64, basis: f64) -> f64 {
    let n = coup_num_between(settlement, maturity, frequency);
    let dsc = coup_days_settle_next(settlement, maturity, frequency, basis);
    let e = coup_days(settlement, maturity, frequency, basis);
    let a = e - dsc;
    let coupon_amt = (100.0 * rate) / frequency;
    let t = dsc / e;
    let yf = yld / frequency;

    let mut price = redemption / js::pow(1.0 + yf, n - 1.0 + t);
    let mut k = 1.0;
    while k <= n {
        price += coupon_amt / js::pow(1.0 + yf, k - 1.0 + t);
        k += 1.0;
    }
    price -= coupon_amt * (a / e);
    price
}

fn price(ev: &Evaluator, a: &[Node]) -> R {
    let (s, m) = read_settle_maturity(ev, a)?;
    let rate = nm(ev, a, 2)?;
    let yld = nm(ev, a, 3)?;
    let redemption = nm(ev, a, 4)?;
    let f = nm(ev, a, 5)?;
    let b = num_or(ev, a, 6, 0.0)?;
    if !freq_ok(f) || !basis_ok(b) {
        return Err(ErrCode::NUM);
    }
    if rate < 0.0 || yld < 0.0 || redemption <= 0.0 {
        return Err(ErrCode::NUM);
    }
    ok(price_core(s, m, rate, yld, redemption, f, b))
}

fn yield_fn(ev: &Evaluator, a: &[Node]) -> R {
    let (s, m) = read_settle_maturity(ev, a)?;
    let rate = nm(ev, a, 2)?;
    let pr = nm(ev, a, 3)?;
    let redemption = nm(ev, a, 4)?;
    let f = nm(ev, a, 5)?;
    let b = num_or(ev, a, 6, 0.0)?;
    if !freq_ok(f) || !basis_ok(b) {
        return Err(ErrCode::NUM);
    }
    if rate < 0.0 || pr <= 0.0 || redemption <= 0.0 {
        return Err(ErrCode::NUM);
    }
    ok(solve_yield(&|y| price_core(s, m, rate, y, redemption, f, b) - pr)?)
}

/// Root finder: bisection with a bracketing scan; yields in (-0.999, 10).
fn solve_yield(fun: &dyn Fn(f64) -> f64) -> N {
    let lo0 = -0.999;
    let hi0 = 10.0;
    let mut lo = lo0;
    let mut hi = hi0;
    let mut flo = fun(lo);
    let fhi = fun(hi);
    if !flo.is_finite() || !fhi.is_finite() {
        return Err(ErrCode::NUM);
    }
    if flo * fhi > 0.0 {
        let mut prev_x = lo;
        let mut prev_f = flo;
        let mut found = false;
        let steps = 200.0;
        let mut i = 1.0;
        while i <= steps {
            let x = lo0 + ((hi0 - lo0) * i) / steps;
            let fx = fun(x);
            if fx.is_finite() && prev_f * fx <= 0.0 {
                lo = prev_x;
                hi = x;
                flo = prev_f;
                found = true;
                break;
            }
            prev_x = x;
            prev_f = fx;
            i += 1.0;
        }
        if !found {
            return Err(ErrCode::NUM);
        }
    }
    for _ in 0..200 {
        let mid = (lo + hi) / 2.0;
        let fm = fun(mid);
        if fm.abs() < 1e-10 || (hi - lo) / 2.0 < 1e-12 {
            return Ok(mid);
        }
        if flo * fm <= 0.0 {
            hi = mid;
        } else {
            lo = mid;
            flo = fm;
        }
    }
    Ok((lo + hi) / 2.0)
}

// â”€â”€ PRICEDISC / YIELDDISC â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

fn pricedisc(ev: &Evaluator, a: &[Node]) -> R {
    let (s, m) = read_settle_maturity(ev, a)?;
    let discount = nm(ev, a, 2)?;
    let redemption = nm(ev, a, 3)?;
    let b = num_or(ev, a, 4, 0.0)?;
    if !basis_ok(b) {
        return Err(ErrCode::NUM);
    }
    if discount <= 0.0 || redemption <= 0.0 {
        return Err(ErrCode::NUM);
    }
    let yf = year_frac(s, m, b)?;
    ok(redemption - discount * redemption * yf)
}

fn yielddisc(ev: &Evaluator, a: &[Node]) -> R {
    let (s, m) = read_settle_maturity(ev, a)?;
    let pr = nm(ev, a, 2)?;
    let redemption = nm(ev, a, 3)?;
    let b = num_or(ev, a, 4, 0.0)?;
    if !basis_ok(b) {
        return Err(ErrCode::NUM);
    }
    if pr <= 0.0 || redemption <= 0.0 {
        return Err(ErrCode::NUM);
    }
    let yf = year_frac(s, m, b)?;
    if yf == 0.0 {
        return Err(ErrCode::NUM);
    }
    ok((redemption - pr) / pr / yf)
}

// â”€â”€ PRICEMAT / YIELDMAT â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

fn pricemat(ev: &Evaluator, a: &[Node]) -> R {
    let (s, m) = read_settle_maturity(ev, a)?;
    let issue_n = nm(ev, a, 2)?;
    let rate = nm(ev, a, 3)?;
    let yld = nm(ev, a, 4)?;
    let b = num_or(ev, a, 5, 0.0)?;
    let Some(issue) = read_date(issue_n) else { return Err(ErrCode::VALUE) };
    if !basis_ok(b) {
        return Err(ErrCode::NUM);
    }
    if rate < 0.0 || yld < 0.0 {
        return Err(ErrCode::NUM);
    }
    if serial(issue) >= serial(s) {
        return Err(ErrCode::NUM);
    }
    let dim = year_frac(issue, m, b)?;
    let dis = year_frac(issue, s, b)?;
    let dsm = year_frac(s, m, b)?;
    let num1 = 100.0 + dim * rate * 100.0;
    let den1 = 1.0 + dsm * yld;
    ok(num1 / den1 - dis * rate * 100.0)
}

fn yieldmat(ev: &Evaluator, a: &[Node]) -> R {
    let (s, m) = read_settle_maturity(ev, a)?;
    let issue_n = nm(ev, a, 2)?;
    let rate = nm(ev, a, 3)?;
    let pr = nm(ev, a, 4)?;
    let b = num_or(ev, a, 5, 0.0)?;
    let Some(issue) = read_date(issue_n) else { return Err(ErrCode::VALUE) };
    if !basis_ok(b) {
        return Err(ErrCode::NUM);
    }
    if rate < 0.0 || pr <= 0.0 {
        return Err(ErrCode::NUM);
    }
    if serial(issue) >= serial(s) {
        return Err(ErrCode::NUM);
    }
    let dim = year_frac(issue, m, b)?;
    let dis = year_frac(issue, s, b)?;
    let dsm = year_frac(s, m, b)?;
    let term1 = 1.0 + dim * rate;
    let term2 = pr / 100.0 + dis * rate;
    let numr = (term1 / term2) - 1.0;
    ok(numr / dsm)
}

// â”€â”€ DURATION / MDURATION â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

fn duration_core(settlement: f64, maturity: f64, coupon: f64, yld: f64, frequency: f64, basis: f64) -> f64 {
    let dsc = coup_days_settle_next(settlement, maturity, frequency, basis);
    let e = coup_days(settlement, maturity, frequency, basis);
    let n = coup_num_between(settlement, maturity, frequency);
    let t = dsc / e;
    let yf = yld / frequency;
    let coupon_amt = (100.0 * coupon) / frequency;

    let mut pv_sum = 0.0;
    let mut weighted = 0.0;
    let mut k = 1.0;
    while k <= n {
        let time = k - 1.0 + t;
        let cf = coupon_amt + if k == n { 100.0 } else { 0.0 };
        let pv = cf / js::pow(1.0 + yf, time);
        pv_sum += pv;
        weighted += time * pv;
        k += 1.0;
    }
    let duration_periods = weighted / pv_sum;
    duration_periods / frequency
}

fn duration_args(ev: &Evaluator, a: &[Node]) -> Result<(f64, f64, f64, f64, f64, f64), ErrCode> {
    let (s, m) = read_settle_maturity(ev, a)?;
    let coupon = nm(ev, a, 2)?;
    let yld = nm(ev, a, 3)?;
    let f = nm(ev, a, 4)?;
    let b = num_or(ev, a, 5, 0.0)?;
    if !freq_ok(f) || !basis_ok(b) {
        return Err(ErrCode::NUM);
    }
    if coupon < 0.0 || yld < 0.0 {
        return Err(ErrCode::NUM);
    }
    Ok((s, m, coupon, yld, f, b))
}

fn duration(ev: &Evaluator, a: &[Node]) -> R {
    let (s, m, coupon, yld, f, b) = duration_args(ev, a)?;
    ok(duration_core(s, m, coupon, yld, f, b))
}

fn mduration(ev: &Evaluator, a: &[Node]) -> R {
    let (s, m, coupon, yld, f, b) = duration_args(ev, a)?;
    let dur = duration_core(s, m, coupon, yld, f, b);
    ok(dur / (1.0 + yld / f))
}

// â”€â”€ Odd-period bonds â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[allow(clippy::too_many_arguments)]
fn odd_first_price_core(
    settlement: f64,
    maturity: f64,
    issue: f64,
    first_coupon: f64,
    rate: f64,
    yld: f64,
    redemption: f64,
    frequency: f64,
    basis: f64,
) -> f64 {
    let yf = yld / frequency;
    let n_reg = coup_num_between(first_coupon, maturity, frequency);
    let Ok(odd_frac) = year_frac(issue, first_coupon, basis) else { return f64::NAN };
    let odd_coupon = 100.0 * rate * odd_frac;
    let reg_coupon = (100.0 * rate) / frequency;

    let Ok(dsc_first) = year_frac(settlement, first_coupon, basis) else { return f64::NAN };
    let Ok(e1) = year_frac(issue, first_coupon, basis) else { return f64::NAN };
    let t1 = dsc_first * frequency;

    let mut price = redemption / js::pow(1.0 + yf, n_reg + t1);
    price += odd_coupon / js::pow(1.0 + yf, t1);
    let mut k = 1.0;
    while k <= n_reg {
        price += reg_coupon / js::pow(1.0 + yf, t1 + k);
        k += 1.0;
    }
    let Ok(acc_frac) = year_frac(issue, settlement, basis) else { return f64::NAN };
    let acc = if e1 > 0.0 { odd_coupon * (acc_frac / e1) } else { 0.0 };
    price -= acc;
    price
}

fn oddfprice(ev: &Evaluator, a: &[Node]) -> R {
    let (s, m) = read_settle_maturity(ev, a)?;
    let issue_n = nm(ev, a, 2)?;
    let first_n = nm(ev, a, 3)?;
    let rate = nm(ev, a, 4)?;
    let yld = nm(ev, a, 5)?;
    let redemption = nm(ev, a, 6)?;
    let f = nm(ev, a, 7)?;
    let b = num_or(ev, a, 8, 0.0)?;
    let (Some(issue), Some(first)) = (read_date(issue_n), read_date(first_n)) else {
        return Err(ErrCode::VALUE);
    };
    if !freq_ok(f) || !basis_ok(b) {
        return Err(ErrCode::NUM);
    }
    if rate < 0.0 || yld < 0.0 || redemption <= 0.0 {
        return Err(ErrCode::NUM);
    }
    let p = odd_first_price_core(s, m, issue, first, rate, yld, redemption, f, b);
    if p.is_finite() {
        ok(p)
    } else {
        Err(ErrCode::NUM)
    }
}

fn oddfyield(ev: &Evaluator, a: &[Node]) -> R {
    let (s, m) = read_settle_maturity(ev, a)?;
    let issue_n = nm(ev, a, 2)?;
    let first_n = nm(ev, a, 3)?;
    let rate = nm(ev, a, 4)?;
    let pr = nm(ev, a, 5)?;
    let redemption = nm(ev, a, 6)?;
    let f = nm(ev, a, 7)?;
    let b = num_or(ev, a, 8, 0.0)?;
    let (Some(issue), Some(first)) = (read_date(issue_n), read_date(first_n)) else {
        return Err(ErrCode::VALUE);
    };
    if !freq_ok(f) || !basis_ok(b) {
        return Err(ErrCode::NUM);
    }
    if rate < 0.0 || pr <= 0.0 || redemption <= 0.0 {
        return Err(ErrCode::NUM);
    }
    ok(solve_yield(&|y| odd_first_price_core(s, m, issue, first, rate, y, redemption, f, b) - pr)?)
}

#[allow(clippy::too_many_arguments)]
fn odd_last_price_core(settlement: f64, maturity: f64, last_interest: f64, rate: f64, yld: f64, redemption: f64, frequency: f64, basis: f64) -> f64 {
    let Ok(odd_frac) = year_frac(last_interest, maturity, basis) else { return f64::NAN };
    let odd_coupon = 100.0 * rate * odd_frac;
    let Ok(dsm) = year_frac(settlement, maturity, basis) else { return f64::NAN };
    let t = dsm * frequency;
    let yf = yld / frequency;

    let price = (redemption + odd_coupon) / js::pow(1.0 + yf, t);
    let Ok(acc_frac) = year_frac(last_interest, settlement, basis) else { return f64::NAN };
    let acc = if odd_frac > 0.0 { odd_coupon * (acc_frac / odd_frac) } else { 0.0 };
    price - acc
}

fn oddlprice(ev: &Evaluator, a: &[Node]) -> R {
    let (s, m) = read_settle_maturity(ev, a)?;
    let last_n = nm(ev, a, 2)?;
    let rate = nm(ev, a, 3)?;
    let yld = nm(ev, a, 4)?;
    let redemption = nm(ev, a, 5)?;
    let f = nm(ev, a, 6)?;
    let b = num_or(ev, a, 7, 0.0)?;
    let Some(last) = read_date(last_n) else { return Err(ErrCode::VALUE) };
    if !freq_ok(f) || !basis_ok(b) {
        return Err(ErrCode::NUM);
    }
    if rate < 0.0 || yld < 0.0 || redemption <= 0.0 {
        return Err(ErrCode::NUM);
    }
    let p = odd_last_price_core(s, m, last, rate, yld, redemption, f, b);
    if p.is_finite() {
        ok(p)
    } else {
        Err(ErrCode::NUM)
    }
}

fn oddlyield(ev: &Evaluator, a: &[Node]) -> R {
    let (s, m) = read_settle_maturity(ev, a)?;
    let last_n = nm(ev, a, 2)?;
    let rate = nm(ev, a, 3)?;
    let pr = nm(ev, a, 4)?;
    let redemption = nm(ev, a, 5)?;
    let f = nm(ev, a, 6)?;
    let b = num_or(ev, a, 7, 0.0)?;
    let Some(last) = read_date(last_n) else { return Err(ErrCode::VALUE) };
    if !freq_ok(f) || !basis_ok(b) {
        return Err(ErrCode::NUM);
    }
    if rate < 0.0 || pr <= 0.0 || redemption <= 0.0 {
        return Err(ErrCode::NUM);
    }
    ok(solve_yield(&|y| odd_last_price_core(s, m, last, rate, y, redemption, f, b) - pr)?)
}

// â”€â”€ French depreciation â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

fn amorlinc(ev: &Evaluator, a: &[Node]) -> R {
    let cost = nm(ev, a, 0)?;
    let pur_n = nm(ev, a, 1)?;
    let first_n = nm(ev, a, 2)?;
    let salvage = nm(ev, a, 3)?;
    let period = nm(ev, a, 4)?;
    let rate = nm(ev, a, 5)?;
    let b = num_or(ev, a, 6, 0.0)?;

    let (Some(purchased), Some(first_period)) = (read_date(pur_n), read_date(first_n)) else {
        return Err(ErrCode::VALUE);
    };
    if !basis_ok(b) {
        return Err(ErrCode::NUM);
    }
    if rate <= 0.0 || cost < salvage || period < 0.0 {
        return Err(ErrCode::NUM);
    }
    let full_dep = cost * rate;
    let frac = year_frac(purchased, first_period, b)?;
    let first_dep = cost * rate * frac;
    let depreciable = cost - salvage;
    if period == 0.0 {
        return ok(js::min(&[first_dep, depreciable]));
    }
    let remaining = depreciable - first_dep;
    if remaining <= 0.0 {
        return ok(0.0);
    }
    let n_full = (remaining / full_dep).floor();
    if period <= n_full {
        return ok(full_dep);
    }
    if period == n_full + 1.0 {
        let last = remaining - n_full * full_dep;
        return ok(if last > 0.0 { last } else { 0.0 });
    }
    ok(0.0)
}

fn amordegrc(ev: &Evaluator, a: &[Node]) -> R {
    let cost = nm(ev, a, 0)?;
    let pur_n = nm(ev, a, 1)?;
    let first_n = nm(ev, a, 2)?;
    let salvage = nm(ev, a, 3)?;
    let period = nm(ev, a, 4)?;
    let rate = nm(ev, a, 5)?;
    let b = num_or(ev, a, 6, 0.0)?;

    let (Some(purchased), Some(first_period)) = (read_date(pur_n), read_date(first_n)) else {
        return Err(ErrCode::VALUE);
    };
    if !basis_ok(b) {
        return Err(ErrCode::NUM);
    }
    if rate <= 0.0 || rate > 0.5 || cost < salvage || period < 0.0 {
        return Err(ErrCode::NUM);
    }
    let life = 1.0 / rate;
    let coeff = if (3.0..=4.0).contains(&life) {
        1.5
    } else if (5.0..=6.0).contains(&life) {
        2.0
    } else if life > 6.0 {
        2.5
    } else {
        1.0
    };
    let deg_rate = rate * coeff;
    let frac = year_frac(purchased, first_period, b)?;

    let mut book = cost;
    let mut dep = 0.0;
    let max_periods = life.ceil() + 2.0;
    let mut p = 0.0;
    while p <= period && p <= max_periods {
        let mut d;
        if p == 0.0 {
            d = js::round(cost * deg_rate * frac);
        } else {
            let remaining_life = life - p;
            if remaining_life <= 2.0 && remaining_life > 0.0 {
                d = js::round(book / js::max(&[remaining_life, 1.0]));
            } else {
                d = js::round(book * deg_rate);
            }
        }
        if book - d < salvage {
            d = book - salvage;
        }
        if d < 0.0 {
            d = 0.0;
        }
        dep = d;
        book -= d;
        if book <= salvage {
            if p < period {
                return ok(0.0);
            }
            break;
        }
        p += 1.0;
    }
    ok(dep)
}

pub fn register(r: &mut Registry) {
    let list: &[(&'static str, crate::formula::eval::Fn)] = &[
        ("COUPNCD", coupncd),
        ("COUPPCD", couppcd),
        ("COUPDAYBS", coupdaybs),
        ("COUPDAYSNC", coupdaysnc),
        ("COUPDAYS", coupdays),
        ("COUPNUM", coupnum),
        ("ACCRINT", accrint),
        ("PRICE", price),
        ("YIELD", yield_fn),
        ("PRICEDISC", pricedisc),
        ("YIELDDISC", yielddisc),
        ("PRICEMAT", pricemat),
        ("YIELDMAT", yieldmat),
        ("DURATION", duration),
        ("MDURATION", mduration),
        ("ODDFPRICE", oddfprice),
        ("ODDFYIELD", oddfyield),
        ("ODDLPRICE", oddlprice),
        ("ODDLYIELD", oddlyield),
        ("AMORLINC", amorlinc),
        ("AMORDEGRC", amordegrc),
    ];
    for (name, f) in list {
        r.insert(name, *f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn month_arithmetic_clamps_and_wraps() {
        let jan31 = jsdate::ymd(2024.0, 0.0, 31.0);
        assert_eq!(comps(add_months(jan31, 1.0)), (2024.0, 1.0, 29.0));
        assert_eq!(comps(add_months(jan31, -2.0)), (2023.0, 10.0, 30.0));
        assert!(is_last_day_of_feb(jsdate::ymd(2023.0, 1.0, 28.0)));
        assert!(days_in_month(2024.0, 13.0).is_nan());
    }

    #[test]
    fn year_frac_bases() {
        let a = jsdate::ymd(2024.0, 0.0, 15.0);
        let b = jsdate::ymd(2025.0, 0.0, 15.0);
        assert_eq!(year_frac(a, b, 0.0), Ok(1.0));
        assert_eq!(year_frac(a, b, 3.0), Ok(366.0 / 365.0));
        assert_eq!(year_frac(a, b, 5.0), Err(ErrCode::NUM));
    }
}