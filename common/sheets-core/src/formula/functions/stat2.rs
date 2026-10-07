//! Port of the web's `formula-fns/stat2.ts`: statistical distributions (gamma, beta, chi-square, F,
//! Student-t, ...), tests, confidence helpers and regressions. The numerical helpers are ported
//! operation for operation (same loop limits, constants and iteration order) so results match the web.
#![allow(clippy::excessive_precision, clippy::manual_range_contains, clippy::needless_range_loop)]

use super::Registry;
use crate::formula::ast::Node;
use crate::formula::eval::{arg, scalar_of, Evaluator, Fn as FnPtr};
use crate::formula::value::{flatten, to_bool, ErrCode, Matrix, Scalar, Value};
use crate::js;

type R = Result<Value, ErrCode>;

fn ok(x: f64) -> R {
    Ok(Value::num(x))
}

// ── Numerical helpers ─────────────────────────────────────────────────────────

const LANCZOS_G: f64 = 7.0;
const LANCZOS_C: [f64; 9] = [
    0.99999999999980993,
    676.5203681218851,
    -1259.1392167224028,
    771.32342877765313,
    -176.61502916214059,
    12.507343278686905,
    -0.13857109526572012,
    9.9843695780195716e-6,
    1.5056327351493116e-7,
];

/// Natural log of the gamma function (Lanczos approximation, g = 7).
fn gammaln(x: f64) -> f64 {
    if x < 0.5 {
        return (std::f64::consts::PI / (std::f64::consts::PI * x).sin()).ln() - gammaln(1.0 - x);
    }
    let x = x - 1.0;
    let mut a = LANCZOS_C[0];
    let t = x + LANCZOS_G + 0.5;
    for (i, c) in LANCZOS_C.iter().enumerate().take(LANCZOS_G as usize + 2).skip(1) {
        a += c / (x + i as f64);
    }
    0.5 * (2.0 * std::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + a.ln()
}

/// True gamma function with the right sign for negative non-integers.
fn gamma_fn(x: f64) -> f64 {
    if x > 0.0 {
        return gammaln(x).exp();
    }
    let sinpx = (std::f64::consts::PI * x).sin();
    if sinpx == 0.0 {
        return f64::NAN;
    }
    std::f64::consts::PI / (sinpx * gammaln(1.0 - x).exp())
}

/// Regularized lower incomplete gamma P(a, x).
fn gamma_p(a: f64, x: f64) -> f64 {
    if x < 0.0 || a <= 0.0 {
        return f64::NAN;
    }
    if x == 0.0 {
        return 0.0;
    }
    if x < a + 1.0 {
        let mut ap = a;
        let mut sum = 1.0 / a;
        let mut del = sum;
        for _ in 0..1000 {
            ap += 1.0;
            del *= x / ap;
            sum += del;
            if del.abs() < sum.abs() * 1e-15 {
                break;
            }
        }
        return sum * (-x + a * x.ln() - gammaln(a)).exp();
    }
    let tiny = 1e-300;
    let mut b = x + 1.0 - a;
    let mut c = 1.0 / tiny;
    let mut d = 1.0 / b;
    let mut h = d;
    for i in 1..1000 {
        let fi = i as f64;
        let an = -fi * (fi - a);
        b += 2.0;
        d = an * d + b;
        if d.abs() < tiny {
            d = tiny;
        }
        c = b + an / c;
        if c.abs() < tiny {
            c = tiny;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < 1e-15 {
            break;
        }
    }
    let q = (-x + a * x.ln() - gammaln(a)).exp() * h;
    1.0 - q
}

/// Continued fraction of the regularized incomplete beta function (modified Lentz).
fn betacf(a: f64, b: f64, x: f64) -> f64 {
    let tiny = 1e-300;
    let qab = a + b;
    let qap = a + 1.0;
    let qam = a - 1.0;
    let mut c = 1.0;
    let mut d = 1.0 - (qab * x) / qap;
    if d.abs() < tiny {
        d = tiny;
    }
    d = 1.0 / d;
    let mut h = d;
    for mi in 1..300 {
        let m = mi as f64;
        let m2 = 2.0 * m;
        let mut aa = (m * (b - m) * x) / ((qam + m2) * (a + m2));
        d = 1.0 + aa * d;
        if d.abs() < tiny {
            d = tiny;
        }
        c = 1.0 + aa / c;
        if c.abs() < tiny {
            c = tiny;
        }
        d = 1.0 / d;
        h *= d * c;
        aa = (-(a + m) * (qab + m) * x) / ((a + m2) * (qap + m2));
        d = 1.0 + aa * d;
        if d.abs() < tiny {
            d = tiny;
        }
        c = 1.0 + aa / c;
        if c.abs() < tiny {
            c = tiny;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < 1e-15 {
            break;
        }
    }
    h
}

fn beta_i(x: f64, a: f64, b: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let bt = (gammaln(a + b) - gammaln(a) - gammaln(b) + a * x.ln() + b * (1.0 - x).ln()).exp();
    if x < (a + 1.0) / (a + b + 2.0) {
        return (bt * betacf(a, b, x)) / a;
    }
    1.0 - (bt * betacf(b, a, 1.0 - x)) / b
}

fn erf(x: f64) -> f64 {
    if x == 0.0 {
        return 0.0;
    }
    let p = gamma_p(0.5, x * x);
    if x >= 0.0 {
        p
    } else {
        -p
    }
}

fn norm_pdf(z: f64) -> f64 {
    (-0.5 * z * z).exp() / (2.0 * std::f64::consts::PI).sqrt()
}

fn norm_cdf(z: f64) -> f64 {
    0.5 * (1.0 + erf(z / std::f64::consts::SQRT_2))
}

/// Inverse standard normal CDF (Acklam's rational approximation + one Newton step).
fn norm_inv(p: f64) -> f64 {
    if p <= 0.0 {
        return f64::NEG_INFINITY;
    }
    if p >= 1.0 {
        return f64::INFINITY;
    }
    let a = [-3.969683028665376e1, 2.209460984245205e2, -2.759285104469687e2, 1.38357751867269e2, -3.066479806614716e1, 2.506628277459239];
    let b = [-5.447609879822406e1, 1.615858368580409e2, -1.556989798598866e2, 6.680131188771972e1, -1.328068155288572e1];
    let c = [-7.784894002430293e-3, -3.223964580411365e-1, -2.400758277161838, -2.549732539343734, 4.374664141464968, 2.938163982698783];
    let d = [7.784695709041462e-3, 3.224671290700398e-1, 2.445134137142996, 3.754408661907416];
    let pl = 0.02425;
    let mut x: f64;
    if p < pl {
        let q = (-2.0 * p.ln()).sqrt();
        x = (((((c[0] * q + c[1]) * q + c[2]) * q + c[3]) * q + c[4]) * q + c[5]) / ((((d[0] * q + d[1]) * q + d[2]) * q + d[3]) * q + 1.0);
    } else if p <= 1.0 - pl {
        let q = p - 0.5;
        let r = q * q;
        x = (((((a[0] * r + a[1]) * r + a[2]) * r + a[3]) * r + a[4]) * r + a[5]) * q / (((((b[0] * r + b[1]) * r + b[2]) * r + b[3]) * r + b[4]) * r + 1.0);
    } else {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        x = -(((((c[0] * q + c[1]) * q + c[2]) * q + c[3]) * q + c[4]) * q + c[5]) / ((((d[0] * q + d[1]) * q + d[2]) * q + d[3]) * q + 1.0);
    }
    let e = norm_cdf(x) - p;
    let u = e / norm_pdf(x);
    x -= u / (1.0 + (x * u) / 2.0);
    x
}

/// Generic monotone-increasing inverse via bisection on `[lo, hi]`.
fn bisect_inv(cdf: impl Fn(f64) -> f64, target: f64, lo: f64, hi: f64) -> f64 {
    let mut a = lo;
    let mut b = hi;
    let mut guard = 0;
    while cdf(b) < target && guard < 200 {
        b = b * 2.0 + 1.0;
        guard += 1;
    }
    for _ in 0..200 {
        let m = (a + b) / 2.0;
        let v = cdf(m);
        if v < target {
            a = m;
        } else {
            b = m;
        }
        if b - a < 1e-12 * js::max(&[1.0, m.abs()]) {
            break;
        }
    }
    (a + b) / 2.0
}

// ── Distribution primitives ───────────────────────────────────────────────────

fn gamma_cdf(x: f64, alpha: f64, beta: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    gamma_p(alpha, x / beta)
}

fn gamma_pdf(x: f64, alpha: f64, beta: f64) -> f64 {
    if x < 0.0 {
        return 0.0;
    }
    if x == 0.0 {
        return if alpha == 1.0 { 1.0 / beta } else { 0.0 };
    }
    ((alpha - 1.0) * x.ln() - x / beta - alpha * beta.ln() - gammaln(alpha)).exp()
}

fn chisq_cdf(x: f64, df: f64) -> f64 {
    gamma_cdf(x, df / 2.0, 2.0)
}

fn f_cdf(x: f64, d1: f64, d2: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    let y = (d1 * x) / (d1 * x + d2);
    beta_i(y, d1 / 2.0, d2 / 2.0)
}

fn t_cdf(t: f64, df: f64) -> f64 {
    let x = df / (df + t * t);
    let ib = 0.5 * beta_i(x, df / 2.0, 0.5);
    if t > 0.0 {
        1.0 - ib
    } else {
        ib
    }
}

// ── Argument helpers ──────────────────────────────────────────────────────────

/// `n(ev, a[i])`: a numeric argument (`#VALUE!` when omitted).
fn n(ev: &Evaluator, a: &[Node], i: usize) -> Result<f64, ErrCode> {
    ev.num(arg(a, i)?)
}

/// `a[i] ? n(ev, a[i]) : def`.
fn n_or(ev: &Evaluator, a: &[Node], i: usize, def: f64) -> Result<f64, ErrCode> {
    match a.get(i) {
        Some(node) => ev.num(node),
        None => Ok(def),
    }
}

/// `nums(ev, a[i])`: the numeric scalars of a range/array argument (errors propagate).
fn nums(ev: &Evaluator, a: &[Node], i: usize) -> Result<Vec<f64>, ErrCode> {
    let mut out = Vec::new();
    for s in flatten(&ev.eval(arg(a, i)?)) {
        match s {
            Scalar::Err(e) => return Err(e),
            Scalar::Num(x) => out.push(x),
            _ => {}
        }
    }
    Ok(out)
}

/// `optBool`: an optional boolean argument.
fn opt_bool(ev: &Evaluator, a: &[Node], i: usize, def: bool) -> Result<bool, ErrCode> {
    match a.get(i) {
        None => Ok(def),
        Some(node) => match ev.scalar(node) {
            Scalar::Err(e) => Err(e),
            s => Ok(to_bool(&s)),
        },
    }
}

/// `matrix(ev, a[i])`: a 2D matrix (a single scalar is 1x1).
fn matrix(ev: &Evaluator, a: &[Node], i: usize) -> Result<Matrix, ErrCode> {
    let v = ev.eval(arg(a, i)?);
    Ok(match v {
        Value::M(m) => m,
        other => vec![vec![scalar_of(&other)]],
    })
}

/// `numColumns`: the numeric columns of a matrix (column-major).
fn num_columns(m: &Matrix) -> Result<Vec<Vec<f64>>, ErrCode> {
    let cols = m.first().map_or(0, Vec::len);
    let mut out = Vec::new();
    for c in 0..cols {
        let mut col = Vec::new();
        for row in m {
            match row.get(c) {
                Some(Scalar::Err(e)) => return Err(e.clone()),
                Some(Scalar::Num(x)) => col.push(*x),
                _ => return Err(ErrCode::VALUE),
            }
        }
        out.push(col);
    }
    Ok(out)
}

fn sum_arr(a: &[f64]) -> f64 {
    a.iter().fold(0.0, |x, y| x + y)
}

fn mean_arr(a: &[f64]) -> f64 {
    sum_arr(a) / a.len() as f64
}

/// Combinations C(n, k) through log-gamma.
fn comb(nn: f64, k: f64) -> f64 {
    if k < 0.0 || k > nn {
        return 0.0;
    }
    (gammaln(nn + 1.0) - gammaln(k + 1.0) - gammaln(nn - k + 1.0)).exp()
}

// ── Gamma / Beta family ───────────────────────────────────────────────────────

fn gamma(ev: &Evaluator, a: &[Node]) -> R {
    let x = n(ev, a, 0)?;
    if x == 0.0 || (x < 0.0 && x.is_finite() && x.fract() == 0.0) {
        return Err(ErrCode::NUM);
    }
    let g = gamma_fn(x);
    if g.is_finite() {
        ok(g)
    } else {
        Err(ErrCode::NUM)
    }
}

fn gamma_dist(ev: &Evaluator, a: &[Node]) -> R {
    let x = n(ev, a, 0)?;
    let alpha = n(ev, a, 1)?;
    let beta = n(ev, a, 2)?;
    let cum = opt_bool(ev, a, 3, true)?;
    if x < 0.0 || alpha <= 0.0 || beta <= 0.0 {
        return Err(ErrCode::NUM);
    }
    ok(if cum { gamma_cdf(x, alpha, beta) } else { gamma_pdf(x, alpha, beta) })
}

fn gamma_inv(ev: &Evaluator, a: &[Node]) -> R {
    let p = n(ev, a, 0)?;
    let alpha = n(ev, a, 1)?;
    let beta = n(ev, a, 2)?;
    if p < 0.0 || p > 1.0 || alpha <= 0.0 || beta <= 0.0 {
        return Err(ErrCode::NUM);
    }
    if p == 0.0 {
        return ok(0.0);
    }
    ok(bisect_inv(|t| gamma_cdf(t, alpha, beta), p, 0.0, alpha * beta + 10.0 * beta))
}

fn beta_dist(ev: &Evaluator, a: &[Node]) -> R {
    let x = n(ev, a, 0)?;
    let alpha = n(ev, a, 1)?;
    let beta = n(ev, a, 2)?;
    let cum = opt_bool(ev, a, 3, true)?;
    let lo = n_or(ev, a, 4, 0.0)?;
    let hi = n_or(ev, a, 5, 1.0)?;
    if alpha <= 0.0 || beta <= 0.0 || hi <= lo {
        return Err(ErrCode::NUM);
    }
    let z = (x - lo) / (hi - lo);
    if z < 0.0 || z > 1.0 {
        return Err(ErrCode::NUM);
    }
    if cum {
        return ok(beta_i(z, alpha, beta));
    }
    if z <= 0.0 || z >= 1.0 {
        return ok(0.0);
    }
    let pdf = (gammaln(alpha + beta) - gammaln(alpha) - gammaln(beta) + (alpha - 1.0) * z.ln() + (beta - 1.0) * (1.0 - z).ln()).exp();
    ok(pdf / (hi - lo))
}

fn betadist(ev: &Evaluator, a: &[Node]) -> R {
    let x = n(ev, a, 0)?;
    let alpha = n(ev, a, 1)?;
    let beta = n(ev, a, 2)?;
    let lo = n_or(ev, a, 3, 0.0)?;
    let hi = n_or(ev, a, 4, 1.0)?;
    if alpha <= 0.0 || beta <= 0.0 || hi <= lo {
        return Err(ErrCode::NUM);
    }
    let z = (x - lo) / (hi - lo);
    if z < 0.0 || z > 1.0 {
        return Err(ErrCode::NUM);
    }
    ok(beta_i(z, alpha, beta))
}

fn beta_inv(ev: &Evaluator, a: &[Node]) -> R {
    let p = n(ev, a, 0)?;
    let alpha = n(ev, a, 1)?;
    let beta = n(ev, a, 2)?;
    let lo = n_or(ev, a, 3, 0.0)?;
    let hi = n_or(ev, a, 4, 1.0)?;
    if p < 0.0 || p > 1.0 || alpha <= 0.0 || beta <= 0.0 || hi <= lo {
        return Err(ErrCode::NUM);
    }
    let z = bisect_inv(|t| beta_i(t, alpha, beta), p, 0.0, 1.0);
    ok(lo + z * (hi - lo))
}

// ── Chi-square ────────────────────────────────────────────────────────────────

fn chisq_dist(ev: &Evaluator, a: &[Node]) -> R {
    let x = n(ev, a, 0)?;
    let df = n(ev, a, 1)?;
    let cum = opt_bool(ev, a, 2, true)?;
    if x < 0.0 || df < 1.0 {
        return Err(ErrCode::NUM);
    }
    if cum {
        return ok(chisq_cdf(x, df));
    }
    ok(gamma_pdf(x, df / 2.0, 2.0))
}

fn chisq_dist_rt(ev: &Evaluator, a: &[Node]) -> R {
    let x = n(ev, a, 0)?;
    let df = n(ev, a, 1)?;
    if x < 0.0 || df < 1.0 {
        return Err(ErrCode::NUM);
    }
    ok(1.0 - chisq_cdf(x, df))
}

fn chisq_inv(ev: &Evaluator, a: &[Node]) -> R {
    let p = n(ev, a, 0)?;
    let df = n(ev, a, 1)?;
    if p < 0.0 || p > 1.0 || df < 1.0 {
        return Err(ErrCode::NUM);
    }
    if p == 0.0 {
        return ok(0.0);
    }
    ok(bisect_inv(|t| chisq_cdf(t, df), p, 0.0, df + 10.0 * (2.0 * df).sqrt() + 20.0))
}

fn chisq_inv_rt(ev: &Evaluator, a: &[Node]) -> R {
    let p = n(ev, a, 0)?;
    let df = n(ev, a, 1)?;
    if p <= 0.0 || p > 1.0 || df < 1.0 {
        return Err(ErrCode::NUM);
    }
    ok(bisect_inv(|t| chisq_cdf(t, df), 1.0 - p, 0.0, df + 10.0 * (2.0 * df).sqrt() + 20.0))
}

fn chisq_test(ev: &Evaluator, a: &[Node]) -> R {
    let obs = matrix(ev, a, 0)?;
    let exp = matrix(ev, a, 1)?;
    let rows = obs.len();
    let cols = obs.first().map_or(0, Vec::len);
    if exp.len() != rows || exp.first().map_or(0, Vec::len) != cols {
        return Err(ErrCode::NA);
    }
    let mut chi2 = 0.0;
    for r in 0..rows {
        for c in 0..cols {
            let o = obs[r].get(c);
            let e = exp[r].get(c);
            if let Some(Scalar::Err(x)) = o {
                return Err(x.clone());
            }
            if let Some(Scalar::Err(x)) = e {
                return Err(x.clone());
            }
            let (Some(Scalar::Num(o)), Some(Scalar::Num(e))) = (o, e) else { return Err(ErrCode::VALUE) };
            if *e == 0.0 {
                return Err(ErrCode::DIV0);
            }
            chi2 += ((o - e) * (o - e)) / e;
        }
    }
    let df = if rows == 1 || cols == 1 { (rows * cols) as f64 - 1.0 } else { (rows as f64 - 1.0) * (cols as f64 - 1.0) };
    if df < 1.0 {
        return Err(ErrCode::NUM);
    }
    ok(1.0 - chisq_cdf(chi2, df))
}

// ── F-distribution ────────────────────────────────────────────────────────────

fn f_dist(ev: &Evaluator, a: &[Node]) -> R {
    let x = n(ev, a, 0)?;
    let d1 = n(ev, a, 1)?;
    let d2 = n(ev, a, 2)?;
    let cum = opt_bool(ev, a, 3, true)?;
    if x < 0.0 || d1 < 1.0 || d2 < 1.0 {
        return Err(ErrCode::NUM);
    }
    if cum {
        return ok(f_cdf(x, d1, d2));
    }
    if x == 0.0 {
        return ok(if d1 > 2.0 {
            0.0
        } else if d1 == 2.0 {
            1.0
        } else {
            f64::INFINITY
        });
    }
    let logpdf = 0.5 * (d1 * d1.ln() + d2 * d2.ln()) + (d1 / 2.0 - 1.0) * x.ln() - 0.5 * (d1 + d2) * (d2 + d1 * x).ln() - (gammaln(d1 / 2.0) + gammaln(d2 / 2.0) - gammaln((d1 + d2) / 2.0));
    ok(logpdf.exp())
}

fn f_dist_rt(ev: &Evaluator, a: &[Node]) -> R {
    let x = n(ev, a, 0)?;
    let d1 = n(ev, a, 1)?;
    let d2 = n(ev, a, 2)?;
    if x < 0.0 || d1 < 1.0 || d2 < 1.0 {
        return Err(ErrCode::NUM);
    }
    ok(1.0 - f_cdf(x, d1, d2))
}

fn f_inv(ev: &Evaluator, a: &[Node]) -> R {
    let p = n(ev, a, 0)?;
    let d1 = n(ev, a, 1)?;
    let d2 = n(ev, a, 2)?;
    if p < 0.0 || p > 1.0 || d1 < 1.0 || d2 < 1.0 {
        return Err(ErrCode::NUM);
    }
    if p == 0.0 {
        return ok(0.0);
    }
    ok(bisect_inv(|t| f_cdf(t, d1, d2), p, 0.0, 1e6))
}

fn f_inv_rt(ev: &Evaluator, a: &[Node]) -> R {
    let p = n(ev, a, 0)?;
    let d1 = n(ev, a, 1)?;
    let d2 = n(ev, a, 2)?;
    if p <= 0.0 || p > 1.0 || d1 < 1.0 || d2 < 1.0 {
        return Err(ErrCode::NUM);
    }
    ok(bisect_inv(|t| f_cdf(t, d1, d2), 1.0 - p, 0.0, 1e6))
}

fn sample_var(arr: &[f64]) -> f64 {
    let m = mean_arr(arr);
    arr.iter().fold(0.0, |s, v| s + (v - m) * (v - m)) / (arr.len() as f64 - 1.0)
}

fn f_test(ev: &Evaluator, a: &[Node]) -> R {
    let x = nums(ev, a, 0)?;
    let y = nums(ev, a, 1)?;
    if x.len() < 2 || y.len() < 2 {
        return Err(ErrCode::DIV0);
    }
    let v1 = sample_var(&x);
    let v2 = sample_var(&y);
    if v1 == 0.0 || v2 == 0.0 {
        return Err(ErrCode::DIV0);
    }
    let f = v1 / v2;
    let d1 = x.len() as f64 - 1.0;
    let d2 = y.len() as f64 - 1.0;
    let cdf = f_cdf(f, d1, d2);
    let tail = js::min(&[cdf, 1.0 - cdf]);
    ok(2.0 * tail)
}

// ── Student-t ─────────────────────────────────────────────────────────────────

fn t_dist(ev: &Evaluator, a: &[Node]) -> R {
    let x = n(ev, a, 0)?;
    let df = n(ev, a, 1)?;
    let cum = opt_bool(ev, a, 2, true)?;
    if df < 1.0 {
        return Err(ErrCode::NUM);
    }
    if cum {
        return ok(t_cdf(x, df));
    }
    let logpdf = gammaln((df + 1.0) / 2.0) - gammaln(df / 2.0) - 0.5 * (df * std::f64::consts::PI).ln() - ((df + 1.0) / 2.0) * (1.0 + (x * x) / df).ln();
    ok(logpdf.exp())
}

fn t_dist_2t(ev: &Evaluator, a: &[Node]) -> R {
    let x = n(ev, a, 0)?;
    let df = n(ev, a, 1)?;
    if x < 0.0 || df < 1.0 {
        return Err(ErrCode::NUM);
    }
    ok(2.0 * (1.0 - t_cdf(x, df)))
}

fn t_dist_rt(ev: &Evaluator, a: &[Node]) -> R {
    let x = n(ev, a, 0)?;
    let df = n(ev, a, 1)?;
    if df < 1.0 {
        return Err(ErrCode::NUM);
    }
    ok(1.0 - t_cdf(x, df))
}

fn tdist(ev: &Evaluator, a: &[Node]) -> R {
    let x = n(ev, a, 0)?;
    let df = n(ev, a, 1)?;
    let tails = n(ev, a, 2)?;
    if x < 0.0 || df < 1.0 || (tails != 1.0 && tails != 2.0) {
        return Err(ErrCode::NUM);
    }
    let rt = 1.0 - t_cdf(x, df);
    ok(if tails == 2.0 { 2.0 * rt } else { rt })
}

fn t_inv(ev: &Evaluator, a: &[Node]) -> R {
    let p = n(ev, a, 0)?;
    let df = n(ev, a, 1)?;
    if p <= 0.0 || p >= 1.0 || df < 1.0 {
        return Err(ErrCode::NUM);
    }
    ok(bisect_inv(|t| t_cdf(t, df), p, -1e6, 1e6))
}

fn t_inv_2t(ev: &Evaluator, a: &[Node]) -> R {
    let p = n(ev, a, 0)?;
    let df = n(ev, a, 1)?;
    if p <= 0.0 || p > 1.0 || df < 1.0 {
        return Err(ErrCode::NUM);
    }
    ok(bisect_inv(|t| t_cdf(t, df), 1.0 - p / 2.0, 0.0, 1e6))
}

fn t_test(ev: &Evaluator, a: &[Node]) -> R {
    let x = nums(ev, a, 0)?;
    let y = nums(ev, a, 1)?;
    let tails = n(ev, a, 2)?;
    let typ = n(ev, a, 3)?;
    if tails != 1.0 && tails != 2.0 {
        return Err(ErrCode::NUM);
    }
    let t: f64;
    let df: f64;
    let mx = mean_arr(&x);
    let my = mean_arr(&y);
    if typ == 1.0 {
        if x.len() != y.len() {
            return Err(ErrCode::NA);
        }
        let np = x.len() as f64;
        let diffs: Vec<f64> = x.iter().zip(&y).map(|(v, w)| v - w).collect();
        let md = mean_arr(&diffs);
        let sd = diffs.iter().fold(0.0, |s, v| s + (v - md) * (v - md)) / (np - 1.0);
        if sd == 0.0 {
            return Err(ErrCode::DIV0);
        }
        t = md / (sd / np).sqrt();
        df = np - 1.0;
    } else {
        let nx = x.len() as f64;
        let ny = y.len() as f64;
        let vx = x.iter().fold(0.0, |s, v| s + (v - mx) * (v - mx)) / (nx - 1.0);
        let vy = y.iter().fold(0.0, |s, v| s + (v - my) * (v - my)) / (ny - 1.0);
        if typ == 2.0 {
            let sp = ((nx - 1.0) * vx + (ny - 1.0) * vy) / (nx + ny - 2.0);
            t = (mx - my) / (sp * (1.0 / nx + 1.0 / ny)).sqrt();
            df = nx + ny - 2.0;
        } else {
            let sxn = vx / nx;
            let syn = vy / ny;
            t = (mx - my) / (sxn + syn).sqrt();
            df = (sxn + syn) * (sxn + syn) / ((sxn * sxn) / (nx - 1.0) + (syn * syn) / (ny - 1.0));
        }
    }
    let rt = 1.0 - t_cdf(t.abs(), df);
    ok(if tails == 2.0 { 2.0 * rt } else { rt })
}

// ── Other distributions ───────────────────────────────────────────────────────

fn hypgeom_dist(ev: &Evaluator, a: &[Node]) -> R {
    let k = n(ev, a, 0)?;
    let ns = n(ev, a, 1)?;
    let kk = n(ev, a, 2)?;
    let nn = n(ev, a, 3)?;
    let cum = opt_bool(ev, a, 4, false)?;
    if ns < 0.0 || ns > nn || kk < 0.0 || kk > nn || k < 0.0 || k > ns {
        return Err(ErrCode::NUM);
    }
    let pmf = |j: f64| (comb(kk, j) * comb(nn - kk, ns - j)) / comb(nn, ns);
    if !cum {
        return ok(pmf(k));
    }
    let mut s = 0.0;
    let mut j = js::max(&[0.0, ns - (nn - kk)]);
    while j <= k {
        s += pmf(j);
        j += 1.0;
    }
    ok(s)
}

fn hypgeomdist(ev: &Evaluator, a: &[Node]) -> R {
    let k = n(ev, a, 0)?;
    let ns = n(ev, a, 1)?;
    let kk = n(ev, a, 2)?;
    let nn = n(ev, a, 3)?;
    if ns < 0.0 || ns > nn || kk < 0.0 || kk > nn || k < 0.0 || k > ns {
        return Err(ErrCode::NUM);
    }
    ok((comb(kk, k) * comb(nn - kk, ns - k)) / comb(nn, ns))
}

fn lognorm_dist(ev: &Evaluator, a: &[Node]) -> R {
    let x = n(ev, a, 0)?;
    let mu = n(ev, a, 1)?;
    let sd = n(ev, a, 2)?;
    let cum = opt_bool(ev, a, 3, true)?;
    if sd <= 0.0 || x <= 0.0 {
        return Err(ErrCode::NUM);
    }
    let z = (x.ln() - mu) / sd;
    if cum {
        return ok(norm_cdf(z));
    }
    ok(norm_pdf(z) / (x * sd))
}

fn lognormdist(ev: &Evaluator, a: &[Node]) -> R {
    let x = n(ev, a, 0)?;
    let mu = n(ev, a, 1)?;
    let sd = n(ev, a, 2)?;
    if sd <= 0.0 || x <= 0.0 {
        return Err(ErrCode::NUM);
    }
    ok(norm_cdf((x.ln() - mu) / sd))
}

fn lognorm_inv(ev: &Evaluator, a: &[Node]) -> R {
    let p = n(ev, a, 0)?;
    let mu = n(ev, a, 1)?;
    let sd = n(ev, a, 2)?;
    if p <= 0.0 || p >= 1.0 || sd <= 0.0 {
        return Err(ErrCode::NUM);
    }
    ok((mu + sd * norm_inv(p)).exp())
}

fn negbinom_pmf(j: f64, s: f64, p: f64) -> f64 {
    comb(j + s - 1.0, s - 1.0) * js::pow(p, s) * js::pow(1.0 - p, j)
}

fn negbinom_dist(ev: &Evaluator, a: &[Node]) -> R {
    let f = n(ev, a, 0)?;
    let s = n(ev, a, 1)?;
    let p = n(ev, a, 2)?;
    let cum = opt_bool(ev, a, 3, false)?;
    if f < 0.0 || s < 1.0 || p < 0.0 || p > 1.0 {
        return Err(ErrCode::NUM);
    }
    if !cum {
        return ok(negbinom_pmf(f, s, p));
    }
    let mut acc = 0.0;
    let mut j = 0.0;
    while j <= f {
        acc += negbinom_pmf(j, s, p);
        j += 1.0;
    }
    ok(acc)
}

fn negbinomdist(ev: &Evaluator, a: &[Node]) -> R {
    let f = n(ev, a, 0)?;
    let s = n(ev, a, 1)?;
    let p = n(ev, a, 2)?;
    if f < 0.0 || s < 1.0 || p < 0.0 || p > 1.0 {
        return Err(ErrCode::NUM);
    }
    ok(negbinom_pmf(f, s, p))
}

fn weibull_dist(ev: &Evaluator, a: &[Node]) -> R {
    let x = n(ev, a, 0)?;
    let alpha = n(ev, a, 1)?;
    let beta = n(ev, a, 2)?;
    let cum = opt_bool(ev, a, 3, true)?;
    if x < 0.0 || alpha <= 0.0 || beta <= 0.0 {
        return Err(ErrCode::NUM);
    }
    let r = js::pow(x / beta, alpha);
    if cum {
        return ok(1.0 - (-r).exp());
    }
    ok((alpha / beta) * js::pow(x / beta, alpha - 1.0) * (-r).exp())
}

// ── Binomial helpers ──────────────────────────────────────────────────────────

fn binom_pmf(k: f64, nn: f64, p: f64) -> f64 {
    comb(nn, k) * js::pow(p, k) * js::pow(1.0 - p, nn - k)
}

fn binom_inv(ev: &Evaluator, a: &[Node]) -> R {
    let trials = n(ev, a, 0)?;
    let p = n(ev, a, 1)?;
    let alpha = n(ev, a, 2)?;
    if trials < 0.0 || p < 0.0 || p > 1.0 || alpha < 0.0 || alpha > 1.0 {
        return Err(ErrCode::NUM);
    }
    let mut acc = 0.0;
    let mut k = 0.0;
    while k <= trials {
        acc += binom_pmf(k, trials, p);
        if acc >= alpha {
            return ok(k);
        }
        k += 1.0;
    }
    ok(trials)
}

fn binom_dist_range(ev: &Evaluator, a: &[Node]) -> R {
    let trials = n(ev, a, 0)?;
    let p = n(ev, a, 1)?;
    let s1 = n(ev, a, 2)?;
    let s2 = n_or(ev, a, 3, s1)?;
    if trials < 0.0 || p < 0.0 || p > 1.0 || s1 < 0.0 || s2 < s1 || s2 > trials {
        return Err(ErrCode::NUM);
    }
    let mut acc = 0.0;
    let mut k = s1.ceil();
    while k <= s2.floor() {
        acc += binom_pmf(k, trials, p);
        k += 1.0;
    }
    ok(acc)
}

// ── Confidence / covariance / misc ────────────────────────────────────────────

fn confidence_t(ev: &Evaluator, a: &[Node]) -> R {
    let alpha = n(ev, a, 0)?;
    let sd = n(ev, a, 1)?;
    let size = n(ev, a, 2)?;
    if alpha <= 0.0 || alpha >= 1.0 || sd <= 0.0 || size < 1.0 {
        return Err(ErrCode::NUM);
    }
    if size == 1.0 {
        return Err(ErrCode::DIV0);
    }
    let df = size - 1.0;
    let tcrit = bisect_inv(|t| t_cdf(t, df), 1.0 - alpha / 2.0, 0.0, 1e6);
    ok((tcrit * sd) / size.sqrt())
}

fn covar(ev: &Evaluator, a: &[Node]) -> R {
    let x = nums(ev, a, 0)?;
    let y = nums(ev, a, 1)?;
    if x.len() != y.len() || x.is_empty() {
        return Err(ErrCode::NA);
    }
    let mx = mean_arr(&x);
    let my = mean_arr(&y);
    let mut s = 0.0;
    for i in 0..x.len() {
        s += (x[i] - mx) * (y[i] - my);
    }
    ok(s / x.len() as f64)
}

fn prob(ev: &Evaluator, a: &[Node]) -> R {
    let xs = nums(ev, a, 0)?;
    let ps = nums(ev, a, 1)?;
    let lower = n(ev, a, 2)?;
    let upper = n_or(ev, a, 3, lower)?;
    if xs.len() != ps.len() || xs.is_empty() {
        return Err(ErrCode::NA);
    }
    let total = sum_arr(&ps);
    if (total - 1.0).abs() > 1e-6 {
        return Err(ErrCode::NUM);
    }
    for pp in &ps {
        if *pp < 0.0 || *pp > 1.0 {
            return Err(ErrCode::NUM);
        }
    }
    let mut acc = 0.0;
    for i in 0..xs.len() {
        if xs[i] >= lower && xs[i] <= upper {
            acc += ps[i];
        }
    }
    ok(acc)
}

fn z_test(ev: &Evaluator, a: &[Node]) -> R {
    let arr = nums(ev, a, 0)?;
    let x = n(ev, a, 1)?;
    if arr.is_empty() {
        return Err(ErrCode::NA);
    }
    let m = mean_arr(&arr);
    let sigma;
    if let Some(node) = a.get(2) {
        sigma = ev.num(node)?;
    } else {
        if arr.len() < 2 {
            return Err(ErrCode::DIV0);
        }
        let v = arr.iter().fold(0.0, |acc, vv| acc + (vv - m) * (vv - m)) / (arr.len() as f64 - 1.0);
        sigma = v.sqrt();
    }
    if sigma <= 0.0 {
        return Err(ErrCode::NUM);
    }
    let z = (m - x) / (sigma / (arr.len() as f64).sqrt());
    ok(1.0 - norm_cdf(z))
}

fn frequency(ev: &Evaluator, a: &[Node]) -> R {
    let data = nums(ev, a, 0)?;
    let bins = nums(ev, a, 1)?;
    let mut sorted = bins.clone();
    js::sort_numbers(&mut sorted);
    let mut counts = vec![0.0_f64; sorted.len() + 1];
    for v in &data {
        let mut placed = false;
        for (i, s) in sorted.iter().enumerate() {
            if v <= s {
                counts[i] += 1.0;
                placed = true;
                break;
            }
        }
        if !placed {
            let last = counts.len() - 1;
            counts[last] += 1.0;
        }
    }
    let mut out: Matrix = Vec::new();
    for b in &bins {
        // `sorted.indexOf(bins[i])`: -1 (an `undefined` cell) only for NaN, which cannot occur in practice.
        let cell = match sorted.iter().position(|s| s == b) {
            Some(rank) => Scalar::Num(counts[rank]),
            None => Scalar::Str(String::new()),
        };
        out.push(vec![cell]);
    }
    out.push(vec![Scalar::Num(counts[counts.len() - 1])]);
    Ok(Value::M(out))
}

// ── Regression ────────────────────────────────────────────────────────────────

/// Gaussian elimination with partial pivoting; `None` when singular.
fn solve_linear(a_mat: &[Vec<f64>], b: &[f64]) -> Option<Vec<f64>> {
    let nn = a_mat.len();
    let mut m: Vec<Vec<f64>> = a_mat
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let mut r = row.clone();
            r.push(b.get(i).copied().unwrap_or(f64::NAN));
            r
        })
        .collect();
    for col in 0..nn {
        let mut piv = col;
        for r in col + 1..nn {
            if m[r][col].abs() > m[piv][col].abs() {
                piv = r;
            }
        }
        if m[piv][col].abs() < 1e-12 {
            return None;
        }
        m.swap(col, piv);
        let pivot_row = m[col].clone();
        for r in 0..nn {
            if r == col {
                continue;
            }
            let f = m[r][col] / pivot_row[col];
            for c in col..=nn {
                m[r][c] -= f * pivot_row[c];
            }
        }
    }
    Some(m.iter().enumerate().map(|(i, row)| row[nn] / row[i]).collect())
}

struct Regression {
    y: Vec<f64>,
    x: Vec<Vec<f64>>,
    n_vars: usize,
}

fn build_design(ev: &Evaluator, a: &[Node], use_const: bool) -> Result<Regression, ErrCode> {
    let ymat = matrix(ev, a, 0)?;
    let ycols = num_columns(&ymat)?;
    let y: Vec<f64> = ycols.into_iter().flatten().collect();
    let x_cols: Vec<Vec<f64>> = if a.get(1).is_some() {
        let xmat = matrix(ev, a, 1)?;
        let xc = num_columns(&xmat)?;
        if xmat.len() == y.len() {
            xc
        } else {
            rows_as_vars(&xmat)?
        }
    } else {
        vec![(0..y.len()).map(|i| i as f64 + 1.0).collect()]
    };
    let n_obs = y.len();
    for col in &x_cols {
        if col.len() != n_obs {
            return Err(ErrCode::REF);
        }
    }
    let n_vars = x_cols.len();
    let mut x = Vec::with_capacity(n_obs);
    for i in 0..n_obs {
        let mut row = Vec::with_capacity(n_vars + 1);
        for col in &x_cols {
            row.push(col[i]);
        }
        if use_const {
            row.push(1.0);
        }
        x.push(row);
    }
    Ok(Regression { y, x, n_vars })
}

/// The matrix read as one variable per row.
fn rows_as_vars(m: &Matrix) -> Result<Vec<Vec<f64>>, ErrCode> {
    let cols = m.first().map_or(0, Vec::len);
    let mut out = Vec::new();
    for row in m {
        let mut r = Vec::new();
        for c in 0..cols {
            match row.get(c) {
                Some(Scalar::Err(e)) => return Err(e.clone()),
                Some(Scalar::Num(x)) => r.push(*x),
                _ => return Err(ErrCode::VALUE),
            }
        }
        out.push(r);
    }
    Ok(out)
}

/// `X'X` accumulated row by row.
fn normal_matrix(x: &[Vec<f64>], cols: usize) -> Vec<Vec<f64>> {
    let mut ata = vec![vec![0.0; cols]; cols];
    for row in x {
        for r in 0..cols {
            for c in 0..cols {
                ata[r][c] += row[r] * row[c];
            }
        }
    }
    ata
}

/// Ordinary least squares; `Err(#VALUE!)` where the web dereferences `X[0]` of an empty design.
fn ols(y: &[f64], x: &[Vec<f64>]) -> Result<Option<Vec<f64>>, ErrCode> {
    let first = x.first().ok_or(ErrCode::VALUE)?;
    let cols = first.len();
    let mut ata = vec![vec![0.0; cols]; cols];
    let mut atb = vec![0.0; cols];
    for i in 0..x.len() {
        for r in 0..cols {
            atb[r] += x[i][r] * y[i];
            for c in 0..cols {
                ata[r][c] += x[i][r] * x[i][c];
            }
        }
    }
    Ok(solve_linear(&ata, &atb))
}

fn predict(coef: &[f64], x: &[Vec<f64>]) -> Vec<f64> {
    x.iter()
        .map(|row| row.iter().enumerate().fold(0.0, |s, (i, v)| s + v * coef.get(i).copied().unwrap_or(f64::NAN)))
        .collect()
}

/// New-x design rows for TREND/GROWTH (an empty result means "use the training design").
fn build_new_x(ev: &Evaluator, a: &[Node], n_vars: usize, use_const: bool) -> Result<Vec<Vec<f64>>, ErrCode> {
    if a.get(2).is_none() {
        return Ok(Vec::new());
    }
    let m = matrix(ev, a, 2)?;
    let cols = num_columns(&m)?;
    let width = m.first().map_or(0, Vec::len);
    let x_cols: Vec<Vec<f64>> = if cols.len() == n_vars && width == n_vars {
        cols
    } else if n_vars == 1 {
        vec![cols.into_iter().flatten().collect()]
    } else if m.len() == n_vars {
        rows_as_vars(&m)?
    } else {
        cols
    };
    let len = x_cols.first().map_or(0, Vec::len);
    let mut rows = Vec::with_capacity(len);
    for i in 0..len {
        let mut row = Vec::with_capacity(n_vars + 1);
        for v in 0..n_vars {
            // `xCols[v]` missing is a TypeError (#VALUE!); a missing element is `undefined` (NaN in arithmetic).
            let col = x_cols.get(v).ok_or(ErrCode::VALUE)?;
            row.push(col.get(i).copied().unwrap_or(f64::NAN));
        }
        if use_const {
            row.push(1.0);
        }
        rows.push(row);
    }
    Ok(rows)
}

/// Gauss-Jordan inverse; `None` when singular.
fn invert_matrix(a_mat: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let nn = a_mat.len();
    let mut m: Vec<Vec<f64>> = a_mat
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let mut r = row.clone();
            r.extend((0..row.len()).map(|j| if i == j { 1.0 } else { 0.0 }));
            r
        })
        .collect();
    for col in 0..nn {
        let mut piv = col;
        for r in col + 1..nn {
            if m[r][col].abs() > m[piv][col].abs() {
                piv = r;
            }
        }
        if m[piv][col].abs() < 1e-12 {
            return None;
        }
        m.swap(col, piv);
        let d = m[col][col];
        for c in 0..2 * nn {
            m[col][c] /= d;
        }
        let pivot_row = m[col].clone();
        for r in 0..nn {
            if r == col {
                continue;
            }
            let f = m[r][col];
            for c in 0..2 * nn {
                m[r][c] -= f * pivot_row[c];
            }
        }
    }
    Some(m.into_iter().map(|row| row[nn..].to_vec()).collect())
}

/// The LINEST statistics block.
fn linest_stats(y: &[f64], x: &[Vec<f64>], coef: &[f64], use_const: bool) -> Result<Matrix, ErrCode> {
    let n_obs = y.len();
    let p = x.first().ok_or(ErrCode::VALUE)?.len();
    let yhat = predict(coef, x);
    let ymean = mean_arr(y);
    let mut ssr = 0.0;
    let mut sse = 0.0;
    let mut sst = 0.0;
    for i in 0..n_obs {
        sse += (y[i] - yhat[i]) * (y[i] - yhat[i]);
        ssr += (yhat[i] - ymean) * (yhat[i] - ymean);
        sst += (y[i] - ymean) * (y[i] - ymean);
    }
    let df_res = n_obs as f64 - p as f64;
    let r2 = if sst == 0.0 {
        1.0
    } else if use_const {
        ssr / sst
    } else {
        (sst - sse) / sst
    };
    let se_y = if df_res > 0.0 { (sse / df_res).sqrt() } else { 0.0 };
    let f_stat = if df_res > 0.0 && sse > 0.0 { (ssr / (p as f64 - if use_const { 1.0 } else { 0.0 })) / (sse / df_res) } else { 0.0 };
    let ata = normal_matrix(x, p);
    let mut se = vec![0.0; p];
    if let Some(inv) = invert_matrix(&ata) {
        for i in 0..p {
            se[i] = (js::max(&[0.0, inv[i][i]]) * (sse / js::max(&[1.0, df_res]))).sqrt();
        }
    }
    let coef_rev: Vec<Scalar> = coef.iter().rev().map(|v| Scalar::Num(*v)).collect();
    let se_rev: Vec<Scalar> = se.iter().rev().map(|v| Scalar::Num(*v)).collect();
    let width = coef_rev.len();
    let mut out: Matrix = vec![coef_rev, se_rev, vec![Scalar::Num(r2), Scalar::Num(se_y)], vec![Scalar::Num(f_stat), Scalar::Num(df_res)], vec![Scalar::Num(ssr), Scalar::Num(sse)]];
    for row in out.iter_mut().skip(2) {
        while row.len() < width {
            row.push(Scalar::Err(ErrCode::NA));
        }
    }
    Ok(out)
}

fn column(v: Vec<f64>) -> Value {
    Value::M(v.into_iter().map(|x| vec![Scalar::Num(x)]).collect())
}

fn trend(ev: &Evaluator, a: &[Node]) -> R {
    let use_const = if a.len() >= 4 { opt_bool(ev, a, 3, true)? } else { true };
    let reg = build_design(ev, a, use_const)?;
    let coef = ols(&reg.y, &reg.x)?.ok_or(ErrCode::NUM)?;
    let mut new_rows = build_new_x(ev, a, reg.n_vars, use_const)?;
    if new_rows.is_empty() {
        new_rows = reg.x;
    }
    Ok(column(predict(&coef, &new_rows)))
}

/// `log` of the y values, `#NUM!` for a non-positive one (GROWTH/LOGEST).
fn log_y(ev: &Evaluator, a: &[Node]) -> Result<Vec<f64>, ErrCode> {
    let ymat = matrix(ev, a, 0)?;
    let yc = num_columns(&ymat)?;
    let yvals: Vec<f64> = yc.into_iter().flatten().collect();
    for v in &yvals {
        if *v <= 0.0 {
            return Err(ErrCode::NUM);
        }
    }
    Ok(yvals.iter().map(|v| v.ln()).collect())
}

fn growth(ev: &Evaluator, a: &[Node]) -> R {
    let use_const = if a.len() >= 4 { opt_bool(ev, a, 3, true)? } else { true };
    let logy = log_y(ev, a)?;
    let reg = build_design(ev, a, use_const)?;
    let coef = ols(&logy, &reg.x)?.ok_or(ErrCode::NUM)?;
    let mut new_rows = build_new_x(ev, a, reg.n_vars, use_const)?;
    if new_rows.is_empty() {
        new_rows = reg.x;
    }
    Ok(column(predict(&coef, &new_rows).into_iter().map(f64::exp).collect()))
}

fn row_of(v: impl Iterator<Item = f64>) -> Value {
    Value::M(vec![v.map(Scalar::Num).collect()])
}

fn linest(ev: &Evaluator, a: &[Node]) -> R {
    let use_const = if a.len() >= 3 { opt_bool(ev, a, 2, true)? } else { true };
    let stats = if a.len() >= 4 { opt_bool(ev, a, 3, false)? } else { false };
    let reg = build_design(ev, a, use_const)?;
    let coef = ols(&reg.y, &reg.x)?.ok_or(ErrCode::NUM)?;
    if !stats {
        return Ok(row_of(coef.iter().rev().copied()));
    }
    Ok(Value::M(linest_stats(&reg.y, &reg.x, &coef, use_const)?))
}

fn logest(ev: &Evaluator, a: &[Node]) -> R {
    let use_const = if a.len() >= 3 { opt_bool(ev, a, 2, true)? } else { true };
    let stats = if a.len() >= 4 { opt_bool(ev, a, 3, false)? } else { false };
    let logy = log_y(ev, a)?;
    let reg = build_design(ev, a, use_const)?;
    let coef = ols(&logy, &reg.x)?.ok_or(ErrCode::NUM)?;
    if !stats {
        return Ok(row_of(coef.iter().map(|c| c.exp()).rev()));
    }
    let mut st = linest_stats(&logy, &reg.x, &coef, use_const)?;
    st[0] = st[0]
        .iter()
        .map(|v| match v {
            Scalar::Num(x) => Scalar::Num(x.exp()),
            other => other.clone(),
        })
        .collect();
    Ok(Value::M(st))
}

// ── Registry ──────────────────────────────────────────────────────────────────

pub fn register(r: &mut Registry) {
    let list: &[(&'static str, FnPtr)] = &[
        ("GAMMA", gamma),
        ("GAMMA.DIST", gamma_dist),
        ("GAMMADIST", gamma_dist),
        ("GAMMA.INV", gamma_inv),
        ("GAMMAINV", gamma_inv),
        ("BETA.DIST", beta_dist),
        ("BETADIST", betadist),
        ("BETA.INV", beta_inv),
        ("BETAINV", beta_inv),
        ("CHISQ.DIST", chisq_dist),
        ("CHISQ.DIST.RT", chisq_dist_rt),
        ("CHIDIST", chisq_dist_rt),
        ("CHISQ.INV", chisq_inv),
        ("CHISQ.INV.RT", chisq_inv_rt),
        ("CHIINV", chisq_inv_rt),
        ("CHISQ.TEST", chisq_test),
        ("CHITEST", chisq_test),
        ("F.DIST", f_dist),
        ("F.DIST.RT", f_dist_rt),
        ("FDIST", f_dist_rt),
        ("F.INV", f_inv),
        ("F.INV.RT", f_inv_rt),
        ("FINV", f_inv_rt),
        ("F.TEST", f_test),
        ("FTEST", f_test),
        ("T.DIST", t_dist),
        ("T.DIST.2T", t_dist_2t),
        ("T.DIST.RT", t_dist_rt),
        ("TDIST", tdist),
        ("T.INV", t_inv),
        ("T.INV.2T", t_inv_2t),
        ("TINV", t_inv_2t),
        ("T.TEST", t_test),
        ("TTEST", t_test),
        ("HYPGEOM.DIST", hypgeom_dist),
        ("HYPGEOMDIST", hypgeomdist),
        ("LOGNORM.DIST", lognorm_dist),
        ("LOGNORMDIST", lognormdist),
        ("LOGNORM.INV", lognorm_inv),
        ("LOGINV", lognorm_inv),
        ("NEGBINOM.DIST", negbinom_dist),
        ("NEGBINOMDIST", negbinomdist),
        ("WEIBULL.DIST", weibull_dist),
        ("WEIBULL", weibull_dist),
        ("BINOM.INV", binom_inv),
        ("CRITBINOM", binom_inv),
        ("BINOM.DIST.RANGE", binom_dist_range),
        ("CONFIDENCE.T", confidence_t),
        ("COVAR", covar),
        ("PROB", prob),
        ("Z.TEST", z_test),
        ("ZTEST", z_test),
        ("FREQUENCY", frequency),
        ("TREND", trend),
        ("GROWTH", growth),
        ("LINEST", linest),
        ("LOGEST", logest),
    ];
    for (name, f) in list {
        r.insert(name, *f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn special_functions_are_accurate() {
        assert!((gamma_fn(5.0) - 24.0).abs() < 1e-9);
        assert!((norm_cdf(0.0) - 0.5).abs() < 1e-12);
        assert!((norm_inv(0.975) - 1.959_963_984_540_054).abs() < 1e-8);
        assert!((t_cdf(0.0, 10.0) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn linear_solver_and_inverse() {
        let a = vec![vec![2.0, 0.0], vec![0.0, 4.0]];
        assert_eq!(solve_linear(&a, &[2.0, 8.0]), Some(vec![1.0, 2.0]));
        assert_eq!(invert_matrix(&a), Some(vec![vec![0.5, 0.0], vec![0.0, 0.25]]));
        assert!(solve_linear(&[vec![0.0]], &[1.0]).is_none());
    }
}
