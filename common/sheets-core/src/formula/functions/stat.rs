//! Port of the web's `formula-fns/stat.ts`: descriptive statistics, regression, rank/percentile and the
//! normal/Poisson/binomial/exponential distributions.

use super::Registry;
use crate::formula::ast::Node;
use crate::formula::criteria::match_criteria_opt;
use crate::formula::eval::{arg, Evaluator};
use crate::formula::value::{flatten, to_bool, to_num, ErrCode, Scalar, Value};
use crate::js;

type R = Result<Value, ErrCode>;
type N = Result<f64, ErrCode>;

// â”€â”€ Internal helpers â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// `argNums`: numbers of one argument, ignoring text and blanks.
fn arg_nums(ev: &Evaluator, node: &Node) -> Result<Vec<f64>, ErrCode> {
    let mut out = Vec::new();
    for s in flatten(&ev.eval(node)) {
        match s {
            Scalar::Err(e) => return Err(e),
            Scalar::Num(n) => out.push(n),
            Scalar::Bool(b) => out.push(if b { 1.0 } else { 0.0 }),
            Scalar::Str(t) => {
                if !t.is_empty() {
                    if let Some(n) = js::number(&t) {
                        out.push(n);
                    }
                }
            }
        }
    }
    Ok(out)
}

/// `argNumsA`: like the `*A` functions (text â†’ 0, TRUE â†’ 1, blanks ignored).
fn arg_nums_a(ev: &Evaluator, node: &Node) -> Result<Vec<f64>, ErrCode> {
    let mut out = Vec::new();
    for s in flatten(&ev.eval(node)) {
        match s {
            Scalar::Err(e) => return Err(e),
            Scalar::Num(n) => out.push(n),
            Scalar::Bool(b) => out.push(if b { 1.0 } else { 0.0 }),
            Scalar::Str(t) => {
                if !t.is_empty() {
                    out.push(js::number(&t).unwrap_or(0.0));
                }
            }
        }
    }
    Ok(out)
}

fn nums_a_all(ev: &Evaluator, a: &[Node]) -> Result<Vec<f64>, ErrCode> {
    let mut out = Vec::new();
    for n in a {
        out.extend(arg_nums_a(ev, n)?);
    }
    Ok(out)
}

fn sum(a: &[f64]) -> f64 {
    a.iter().fold(0.0, |x, y| x + y)
}

fn mean(a: &[f64]) -> f64 {
    sum(a) / a.len() as f64
}

fn dev_sq(a: &[f64]) -> f64 {
    let m = mean(a);
    a.iter().fold(0.0, |s, x| s + (x - m) * (x - m))
}

fn covar(x: &[f64], y: &[f64], sample: bool) -> f64 {
    let n = x.len();
    let (mx, my) = (mean(x), mean(y));
    let mut s = 0.0;
    for i in 0..n {
        s += (x[i] - mx) * (y[i] - my);
    }
    s / if sample { n as f64 - 1.0 } else { n as f64 }
}

fn correl(x: &[f64], y: &[f64]) -> N {
    let n = x.len();
    let (mx, my) = (mean(x), mean(y));
    let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let (dx, dy) = (x[i] - mx, y[i] - my);
        sxy += dx * dy;
        sxx += dx * dx;
        syy += dy * dy;
    }
    let den = (sxx * syy).sqrt();
    if den == 0.0 {
        return Err(ErrCode::DIV0);
    }
    Ok(sxy / den)
}

/// `linreg(y, x)` â†’ (slope, intercept).
fn linreg(y: &[f64], x: &[f64]) -> Result<(f64, f64), ErrCode> {
    let n = x.len();
    let (mx, my) = (mean(x), mean(y));
    let (mut sxy, mut sxx) = (0.0, 0.0);
    for i in 0..n {
        let dx = x[i] - mx;
        sxy += dx * (y[i] - my);
        sxx += dx * dx;
    }
    if sxx == 0.0 {
        return Err(ErrCode::DIV0);
    }
    let slope = sxy / sxx;
    Ok((slope, my - slope * mx))
}

fn pair_num(s: &Scalar) -> Option<f64> {
    match s {
        Scalar::Num(n) => Some(*n),
        Scalar::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        Scalar::Str(t) if !t.is_empty() => js::number(t),
        _ => None,
    }
}

type Pair = (Vec<f64>, Vec<f64>);

/// `twoVecs`: `Ok(None)` is the web's `null` (length mismatch).
fn two_vecs(ev: &Evaluator, n1: &Node, n2: &Node) -> Result<Option<Pair>, ErrCode> {
    let va = flatten(&ev.eval(n1));
    let vb = flatten(&ev.eval(n2));
    if va.len() != vb.len() {
        return Ok(None);
    }
    let (mut x, mut y) = (Vec::new(), Vec::new());
    for (a, b) in va.iter().zip(vb.iter()) {
        if let Scalar::Err(e) = a {
            return Err(e.clone());
        }
        if let Scalar::Err(e) = b {
            return Err(e.clone());
        }
        let (Some(an), Some(bn)) = (pair_num(a), pair_num(b)) else { continue };
        x.push(an);
        y.push(bn);
    }
    Ok(Some((x, y)))
}

fn sorted(a: &[f64]) -> Vec<f64> {
    let mut s = a.to_vec();
    js::sort_numbers(&mut s);
    s
}

/// `s[i]`, `NaN` (â‰ˆ `undefined`) when out of range.
fn at(s: &[f64], i: f64) -> f64 {
    super::base::index(i).and_then(|i| s.get(i)).copied().unwrap_or(f64::NAN)
}

fn nth_sorted(a: &[f64], k: f64, largest: bool) -> N {
    if k < 1.0 || k > a.len() as f64 {
        return Err(ErrCode::NUM);
    }
    let s = sorted(a);
    let k = k as usize;
    Ok(if largest { s[s.len() - k] } else { s[k - 1] })
}

fn percentile_inc(a: &[f64], p: f64) -> N {
    if p < 0.0 || p > 1.0 {
        return Err(ErrCode::NUM);
    }
    let s = sorted(a);
    let n = s.len();
    if n == 1 {
        return Ok(s[0]);
    }
    let rank = p * (n as f64 - 1.0);
    let lo = rank.floor();
    let frac = rank - lo;
    if lo + 1.0 >= n as f64 {
        return Ok(at(&s, n as f64 - 1.0));
    }
    Ok(at(&s, lo) + frac * (at(&s, lo + 1.0) - at(&s, lo)))
}

fn percentile_exc(a: &[f64], p: f64) -> N {
    let s = sorted(a);
    let n = s.len() as f64;
    if p < 1.0 / (n + 1.0) || p > n / (n + 1.0) {
        return Err(ErrCode::NUM);
    }
    let rank = p * (n + 1.0) - 1.0;
    let lo = rank.floor();
    let frac = rank - lo;
    if lo + 1.0 >= n {
        return Ok(at(&s, n - 1.0));
    }
    if lo < 0.0 {
        return Ok(at(&s, 0.0));
    }
    Ok(at(&s, lo) + frac * (at(&s, lo + 1.0) - at(&s, lo)))
}

fn trunc_to(v: f64, sig: f64) -> f64 {
    let f = js::pow(10.0, sig);
    js::trunc(v * f) / f
}

fn percent_rank(a: &[f64], x: f64, sig: f64, exc: bool) -> N {
    let s = sorted(a);
    let n = s.len();
    if x < at(&s, 0.0) || x > at(&s, n as f64 - 1.0) {
        return Err(ErrCode::NA);
    }
    let nf = n as f64;
    for (i, v) in s.iter().enumerate() {
        if *v == x {
            // Repeated values: the first occurrence's position.
            let i = i as f64;
            return Ok(trunc_to(if exc { (i + 1.0) / (nf + 1.0) } else { i / (nf - 1.0) }, sig));
        }
    }
    let mut i = 0usize;
    while i + 1 < n && s[i + 1] < x {
        i += 1;
    }
    let (lo, hi) = (at(&s, i as f64), at(&s, i as f64 + 1.0));
    let fi = i as f64;
    let rank = if exc { ((fi + 1.0) + (x - lo) / (hi - lo)) / (nf + 1.0) } else { (fi + (x - lo) / (hi - lo)) / (nf - 1.0) };
    Ok(trunc_to(rank, sig))
}

fn permut(n: f64, k: f64) -> f64 {
    let mut r = 1.0;
    let mut i = 0.0;
    while i < k {
        r *= n - i;
        i += 1.0;
    }
    r
}

// â”€â”€ erf / normal distribution primitives â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Abramowitz & Stegun 7.1.26 approximation of the error function.
fn erf(x: f64) -> f64 {
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();
    let t = 1.0 / (1.0 + 0.3275911 * x);
    let y = 1.0 - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t + 0.254829592) * t * (-x * x).exp();
    sign * y
}

fn norm_pdf(x: f64) -> f64 {
    (-0.5 * x * x).exp() / (2.0 * std::f64::consts::PI).sqrt()
}

fn norm_cdf(x: f64) -> f64 {
    0.5 * (1.0 + erf(x / std::f64::consts::SQRT_2))
}

#[allow(clippy::excessive_precision)] // literals kept verbatim from the web source
fn norm_inv_std(p: f64) -> f64 {
    if p <= 0.0 || p >= 1.0 {
        return f64::NAN;
    }
    let a = [-3.969683028665376e+01, 2.209460984245205e+02, -2.759285104469687e+02, 1.383577518672690e+02, -3.066479806614716e+01, 2.506628277459239e+00];
    let b = [-5.447609879822406e+01, 1.615858368580409e+02, -1.556989798598866e+02, 6.680131188771972e+01, -1.328068155288572e+01];
    let c = [-7.784894002430293e-03, -3.223964580411365e-01, -2.400758277161838e+00, -2.549732539343734e+00, 4.374664141464968e+00, 2.938163982698783e+00];
    let d = [7.784695709041462e-03, 3.224671290700398e-01, 2.445134137142996e+00, 3.754408661907416e+00];
    let plow = 0.02425;
    let phigh = 1.0 - plow;
    let mut x;
    if p < plow {
        let q = (-2.0 * p.ln()).sqrt();
        x = (((((c[0] * q + c[1]) * q + c[2]) * q + c[3]) * q + c[4]) * q + c[5]) / ((((d[0] * q + d[1]) * q + d[2]) * q + d[3]) * q + 1.0);
    } else if p <= phigh {
        let q = p - 0.5;
        let r = q * q;
        x = (((((a[0] * r + a[1]) * r + a[2]) * r + a[3]) * r + a[4]) * r + a[5]) * q / (((((b[0] * r + b[1]) * r + b[2]) * r + b[3]) * r + b[4]) * r + 1.0);
    } else {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        x = -(((((c[0] * q + c[1]) * q + c[2]) * q + c[3]) * q + c[4]) * q + c[5]) / ((((d[0] * q + d[1]) * q + d[2]) * q + d[3]) * q + 1.0);
    }
    // One Halley refinement step.
    let e = norm_cdf(x) - p;
    let u = e * (2.0 * std::f64::consts::PI).sqrt() * (x * x / 2.0).exp();
    x -= u / (1.0 + x * u / 2.0);
    x
}

/// Lanczos approximation of ln(Gamma).
#[allow(clippy::excessive_precision)] // literals kept verbatim from the web source
fn gammaln(x: f64) -> f64 {
    let g = [676.5203681218851, -1259.1392167224028, 771.32342877765313, -176.61502916214059, 12.507343278686905, -0.13857109526572012, 9.9843695780195716e-6, 1.5056327351493116e-7];
    if x < 0.5 {
        return (std::f64::consts::PI / (std::f64::consts::PI * x).sin()).ln() - gammaln(1.0 - x);
    }
    let x = x - 1.0;
    let mut a = 0.99999999999980993;
    let t = x + 7.5;
    for (i, gi) in g.iter().enumerate() {
        a += gi / (x + i as f64 + 1.0);
    }
    0.5 * (2.0 * std::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + a.ln()
}

fn ln_factorial(n: f64) -> f64 {
    gammaln(n + 1.0)
}

fn binom_coef(n: f64, k: f64) -> f64 {
    (ln_factorial(n) - ln_factorial(k) - ln_factorial(n - k)).exp()
}

fn n1(ev: &Evaluator, a: &[Node], i: usize) -> N {
    ev.num(arg(a, i)?)
}

/// `a[i] ? n1(a[i]) : def`.
fn n1_or(ev: &Evaluator, a: &[Node], i: usize, def: f64) -> N {
    match a.get(i) {
        Some(n) => ev.num(n),
        None => Ok(def),
    }
}

fn num(v: f64) -> R {
    Ok(Value::num(v))
}

// â”€â”€ Descriptive â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

fn avedev(ev: &Evaluator, a: &[Node]) -> R {
    let n = ev.nums(a)?;
    if n.is_empty() {
        return Err(ErrCode::NUM);
    }
    let m = mean(&n);
    num(n.iter().fold(0.0, |s, x| s + (x - m).abs()) / n.len() as f64)
}

fn averagea(ev: &Evaluator, a: &[Node]) -> R {
    let out = nums_a_all(ev, a)?;
    if out.is_empty() {
        return Err(ErrCode::DIV0);
    }
    num(mean(&out))
}

fn devsq(ev: &Evaluator, a: &[Node]) -> R {
    let n = ev.nums(a)?;
    if n.is_empty() {
        return Err(ErrCode::NUM);
    }
    num(dev_sq(&n))
}

fn geomean(ev: &Evaluator, a: &[Node]) -> R {
    let n = ev.nums(a)?;
    if n.is_empty() {
        return Err(ErrCode::NUM);
    }
    let mut log_sum = 0.0;
    for x in &n {
        if *x <= 0.0 {
            return Err(ErrCode::NUM);
        }
        log_sum += x.ln();
    }
    num((log_sum / n.len() as f64).exp())
}

fn harmean(ev: &Evaluator, a: &[Node]) -> R {
    let n = ev.nums(a)?;
    if n.is_empty() {
        return Err(ErrCode::NUM);
    }
    let mut s = 0.0;
    for x in &n {
        if *x <= 0.0 {
            return Err(ErrCode::NUM);
        }
        s += 1.0 / x;
    }
    num(n.len() as f64 / s)
}

fn kurt(ev: &Evaluator, a: &[Node]) -> R {
    let n = ev.nums(a)?;
    let nn = n.len() as f64;
    if n.len() < 4 {
        return Err(ErrCode::NUM);
    }
    let m = mean(&n);
    let sd = (dev_sq(&n) / (nn - 1.0)).sqrt();
    if sd == 0.0 {
        return Err(ErrCode::DIV0);
    }
    let mut s = 0.0;
    for x in &n {
        s += js::pow((x - m) / sd, 4.0);
    }
    num((nn * (nn + 1.0)) / ((nn - 1.0) * (nn - 2.0) * (nn - 3.0)) * s - (3.0 * (nn - 1.0) * (nn - 1.0)) / ((nn - 2.0) * (nn - 3.0)))
}

fn skew(ev: &Evaluator, a: &[Node]) -> R {
    let n = ev.nums(a)?;
    let nn = n.len() as f64;
    if n.len() < 3 {
        return Err(ErrCode::NUM);
    }
    let m = mean(&n);
    let sd = (dev_sq(&n) / (nn - 1.0)).sqrt();
    if sd == 0.0 {
        return Err(ErrCode::DIV0);
    }
    let mut s = 0.0;
    for x in &n {
        s += js::pow((x - m) / sd, 3.0);
    }
    num((nn / ((nn - 1.0) * (nn - 2.0))) * s)
}

fn skew_p(ev: &Evaluator, a: &[Node]) -> R {
    let n = ev.nums(a)?;
    let nn = n.len() as f64;
    if n.is_empty() {
        return Err(ErrCode::NUM);
    }
    let m = mean(&n);
    let sd = (dev_sq(&n) / nn).sqrt();
    if sd == 0.0 {
        return Err(ErrCode::DIV0);
    }
    let mut s = 0.0;
    for x in &n {
        s += js::pow((x - m) / sd, 3.0);
    }
    num(s / nn)
}

// â”€â”€ Largest / smallest â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

fn large(ev: &Evaluator, a: &[Node]) -> R {
    let n = arg_nums(ev, arg(a, 0)?)?;
    let k = n1(ev, a, 1)?;
    num(nth_sorted(&n, js::trunc(k), true)?)
}

fn small(ev: &Evaluator, a: &[Node]) -> R {
    let n = arg_nums(ev, arg(a, 0)?)?;
    let k = n1(ev, a, 1)?;
    num(nth_sorted(&n, js::trunc(k), false)?)
}

fn maxa(ev: &Evaluator, a: &[Node]) -> R {
    let out = nums_a_all(ev, a)?;
    num(if out.is_empty() { 0.0 } else { js::max(&out) })
}

fn mina(ev: &Evaluator, a: &[Node]) -> R {
    let out = nums_a_all(ev, a)?;
    num(if out.is_empty() { 0.0 } else { js::min(&out) })
}

type IfsPairs = Vec<(Vec<Scalar>, Scalar)>;

/// The (range, criterion) pairs of an *IFS function, from argument 1.
fn ifs_pairs(ev: &Evaluator, a: &[Node]) -> IfsPairs {
    let mut pairs = Vec::new();
    let mut i = 1;
    while i + 1 < a.len() {
        pairs.push((flatten(&ev.eval(&a[i])), ev.scalar(&a[i + 1])));
        i += 2;
    }
    pairs
}

fn ifs_match(pairs: &IfsPairs, i: usize) -> bool {
    pairs.iter().all(|(range, crit)| match_criteria_opt(range.get(i), crit))
}

fn ifs_extreme(ev: &Evaluator, a: &[Node], max: bool) -> R {
    let range = flatten(&ev.eval(arg(a, 0)?));
    let pairs = ifs_pairs(ev, a);
    let mut best: Option<f64> = None;
    for (i, cell) in range.iter().enumerate() {
        if ifs_match(&pairs, i) {
            let Ok(v) = to_num(cell) else { continue };
            if !matches!(cell, Scalar::Num(_)) {
                continue;
            }
            best = match best {
                None => Some(v),
                Some(b) if (max && v > b) || (!max && v < b) => Some(v),
                keep => keep,
            };
        }
    }
    // Excel returns 0 when no cell matches.
    num(best.unwrap_or(0.0))
}

fn maxifs(ev: &Evaluator, a: &[Node]) -> R {
    ifs_extreme(ev, a, true)
}

fn minifs(ev: &Evaluator, a: &[Node]) -> R {
    ifs_extreme(ev, a, false)
}

fn averageifs(ev: &Evaluator, a: &[Node]) -> R {
    let avg_range = flatten(&ev.eval(arg(a, 0)?));
    let pairs = ifs_pairs(ev, a);
    let (mut s, mut c) = (0.0, 0.0);
    for (i, cell) in avg_range.iter().enumerate() {
        if ifs_match(&pairs, i) {
            if let Ok(v) = to_num(cell) {
                s += v;
                c += 1.0;
            }
        }
    }
    if c != 0.0 {
        num(s / c)
    } else {
        Err(ErrCode::DIV0)
    }
}

// â”€â”€ Mode â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

fn same_value_zero(a: f64, b: f64) -> bool {
    a == b || (a.is_nan() && b.is_nan())
}

/// Occurrence counts in first-seen order (a `Map<number, number>`).
fn counts_of(n: &[f64]) -> Vec<(f64, usize)> {
    let mut counts: Vec<(f64, usize)> = Vec::new();
    for x in n {
        match counts.iter_mut().find(|(v, _)| same_value_zero(*v, *x)) {
            Some(e) => e.1 += 1,
            None => counts.push((*x, 1)),
        }
    }
    counts
}

fn count_of(counts: &[(f64, usize)], x: f64) -> usize {
    counts.iter().find(|(v, _)| same_value_zero(*v, x)).map(|e| e.1).unwrap_or(0)
}

fn mode_sngl(ev: &Evaluator, a: &[Node]) -> R {
    let n = ev.nums(a)?;
    let counts = counts_of(&n);
    let mut found = false;
    let mut best_c = 1;
    for x in &n {
        let c = count_of(&counts, *x);
        if c > best_c {
            best_c = c;
            found = true;
        }
    }
    if !found {
        return Err(ErrCode::NA);
    }
    for x in &n {
        if count_of(&counts, *x) == best_c {
            return num(*x);
        }
    }
    Err(ErrCode::NA)
}

fn mode_mult(ev: &Evaluator, a: &[Node]) -> R {
    let n = ev.nums(a)?;
    let counts = counts_of(&n);
    let max_c = counts.iter().map(|e| e.1).max().unwrap_or(0);
    if max_c < 2 {
        return Err(ErrCode::NA);
    }
    // A vertical array in first-seen order.
    let out = counts.iter().filter(|e| e.1 == max_c).map(|e| vec![Scalar::Num(e.0)]).collect();
    Ok(Value::M(out))
}

// â”€â”€ Variance / standard deviation â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

fn var_sample_n(ev: &Evaluator, a: &[Node]) -> N {
    let n = ev.nums(a)?;
    if n.len() < 2 {
        return Err(ErrCode::DIV0);
    }
    Ok(dev_sq(&n) / (n.len() as f64 - 1.0))
}

fn var_pop_n(ev: &Evaluator, a: &[Node]) -> N {
    let n = ev.nums(a)?;
    if n.is_empty() {
        return Err(ErrCode::DIV0);
    }
    Ok(dev_sq(&n) / n.len() as f64)
}

fn var_a_n(ev: &Evaluator, a: &[Node], sample: bool) -> N {
    let out = nums_a_all(ev, a)?;
    if sample {
        if out.len() < 2 {
            return Err(ErrCode::DIV0);
        }
        return Ok(dev_sq(&out) / (out.len() as f64 - 1.0));
    }
    if out.is_empty() {
        return Err(ErrCode::DIV0);
    }
    Ok(dev_sq(&out) / out.len() as f64)
}

fn var_sample(ev: &Evaluator, a: &[Node]) -> R {
    num(var_sample_n(ev, a)?)
}
fn var_pop(ev: &Evaluator, a: &[Node]) -> R {
    num(var_pop_n(ev, a)?)
}
fn vara(ev: &Evaluator, a: &[Node]) -> R {
    num(var_a_n(ev, a, true)?)
}
fn varpa(ev: &Evaluator, a: &[Node]) -> R {
    num(var_a_n(ev, a, false)?)
}
fn stdev_sample(ev: &Evaluator, a: &[Node]) -> R {
    num(var_sample_n(ev, a)?.sqrt())
}
fn stdev_pop(ev: &Evaluator, a: &[Node]) -> R {
    num(var_pop_n(ev, a)?.sqrt())
}
fn stdeva(ev: &Evaluator, a: &[Node]) -> R {
    num(var_a_n(ev, a, true)?.sqrt())
}
fn stdevpa(ev: &Evaluator, a: &[Node]) -> R {
    num(var_a_n(ev, a, false)?.sqrt())
}

// â”€â”€ Regression / correlation â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// `twoVecs(a[0], a[1])` with the `null` â†’ `#N/A` and error handling every caller repeats.
fn vecs(ev: &Evaluator, a: &[Node]) -> Result<Pair, ErrCode> {
    two_vecs(ev, arg(a, 0)?, arg(a, 1)?)?.ok_or(ErrCode::NA)
}

fn correl_fn(ev: &Evaluator, a: &[Node]) -> R {
    let (x, y) = vecs(ev, a)?;
    if x.is_empty() {
        return Err(ErrCode::DIV0);
    }
    num(correl(&x, &y)?)
}

fn rsq(ev: &Evaluator, a: &[Node]) -> R {
    let (x, y) = vecs(ev, a)?;
    if x.is_empty() {
        return Err(ErrCode::DIV0);
    }
    let c = correl(&x, &y)?;
    num(c * c)
}

fn covariance_p(ev: &Evaluator, a: &[Node]) -> R {
    let (x, y) = vecs(ev, a)?;
    if x.is_empty() {
        return Err(ErrCode::DIV0);
    }
    num(covar(&x, &y, false))
}

fn covariance_s(ev: &Evaluator, a: &[Node]) -> R {
    let (x, y) = vecs(ev, a)?;
    if x.len() < 2 {
        return Err(ErrCode::DIV0);
    }
    num(covar(&x, &y, true))
}

fn slope(ev: &Evaluator, a: &[Node]) -> R {
    let (x, y) = vecs(ev, a)?;
    if x.len() < 2 {
        return Err(ErrCode::DIV0);
    }
    num(linreg(&x, &y)?.0)
}

fn intercept(ev: &Evaluator, a: &[Node]) -> R {
    let (x, y) = vecs(ev, a)?;
    if x.len() < 2 {
        return Err(ErrCode::DIV0);
    }
    num(linreg(&x, &y)?.1)
}

fn forecast(ev: &Evaluator, a: &[Node]) -> R {
    let x = n1(ev, a, 0)?;
    let (ys, xs) = two_vecs(ev, arg(a, 1)?, arg(a, 2)?)?.ok_or(ErrCode::NA)?;
    if ys.is_empty() {
        return Err(ErrCode::DIV0);
    }
    let (slope, intercept) = linreg(&ys, &xs)?;
    num(intercept + slope * x)
}

fn steyx(ev: &Evaluator, a: &[Node]) -> R {
    let (y, x) = vecs(ev, a)?;
    let n = x.len();
    if n < 3 {
        return Err(ErrCode::DIV0);
    }
    let (mx, my) = (mean(&x), mean(&y));
    let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let (dx, dy) = (x[i] - mx, y[i] - my);
        sxx += dx * dx;
        syy += dy * dy;
        sxy += dx * dy;
    }
    if sxx == 0.0 {
        return Err(ErrCode::DIV0);
    }
    let v = (syy - (sxy * sxy) / sxx) / (n as f64 - 2.0);
    num(js::max(&[0.0, v]).sqrt())
}

// â”€â”€ Percentiles / quartiles / rank â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

fn percentile_inc_fn(ev: &Evaluator, a: &[Node]) -> R {
    let n = arg_nums(ev, arg(a, 0)?)?;
    if n.is_empty() {
        return Err(ErrCode::NUM);
    }
    let p = n1(ev, a, 1)?;
    num(percentile_inc(&n, p)?)
}

fn percentile_exc_fn(ev: &Evaluator, a: &[Node]) -> R {
    let n = arg_nums(ev, arg(a, 0)?)?;
    if n.is_empty() {
        return Err(ErrCode::NUM);
    }
    let p = n1(ev, a, 1)?;
    num(percentile_exc(&n, p)?)
}

fn quartile(ev: &Evaluator, a: &[Node], exc: bool) -> R {
    let n = arg_nums(ev, arg(a, 0)?)?;
    if n.is_empty() {
        return Err(ErrCode::NUM);
    }
    let q = n1(ev, a, 1)?;
    let qi = js::trunc(q);
    if qi < 0.0 || qi > 4.0 {
        return Err(ErrCode::NUM);
    }
    let p = qi / 4.0;
    num(if exc { percentile_exc(&n, p)? } else { percentile_inc(&n, p)? })
}

fn quartile_inc(ev: &Evaluator, a: &[Node]) -> R {
    quartile(ev, a, false)
}

fn quartile_exc(ev: &Evaluator, a: &[Node]) -> R {
    quartile(ev, a, true)
}

fn percentrank(ev: &Evaluator, a: &[Node], exc: bool) -> R {
    let n = arg_nums(ev, arg(a, 0)?)?;
    if n.is_empty() {
        return Err(ErrCode::NUM);
    }
    let x = n1(ev, a, 1)?;
    let sig = n1_or(ev, a, 2, 3.0)?;
    num(percent_rank(&n, x, js::trunc(sig), exc)?)
}

fn percentrank_inc(ev: &Evaluator, a: &[Node]) -> R {
    percentrank(ev, a, false)
}

fn percentrank_exc(ev: &Evaluator, a: &[Node]) -> R {
    percentrank(ev, a, true)
}

fn rank(ev: &Evaluator, a: &[Node], avg: bool) -> R {
    let x = n1(ev, a, 0)?;
    let reference = arg_nums(ev, arg(a, 1)?)?;
    let order = n1_or(ev, a, 2, 0.0)?;
    let mut s = sorted(&reference);
    if order == 0.0 {
        s.reverse();
    }
    let Some(first) = s.iter().position(|v| *v == x) else { return Err(ErrCode::NA) };
    if !avg {
        return num(first as f64 + 1.0);
    }
    let mut last = first;
    while last + 1 < s.len() && s[last + 1] == x {
        last += 1;
    }
    num(((first + 1) + (last + 1)) as f64 / 2.0)
}

fn rank_eq(ev: &Evaluator, a: &[Node]) -> R {
    rank(ev, a, false)
}

fn rank_avg(ev: &Evaluator, a: &[Node]) -> R {
    rank(ev, a, true)
}

fn standardize(ev: &Evaluator, a: &[Node]) -> R {
    let x = n1(ev, a, 0)?;
    let m = n1(ev, a, 1)?;
    let sd = n1(ev, a, 2)?;
    if sd <= 0.0 {
        return Err(ErrCode::NUM);
    }
    num((x - m) / sd)
}

fn trimmean(ev: &Evaluator, a: &[Node]) -> R {
    let n = arg_nums(ev, arg(a, 0)?)?;
    if n.is_empty() {
        return Err(ErrCode::NUM);
    }
    let pct = n1(ev, a, 1)?;
    if pct < 0.0 || pct >= 1.0 {
        return Err(ErrCode::NUM);
    }
    let s = sorted(&n);
    let mut trim = (s.len() as f64 * pct).floor();
    trim = (trim / 2.0).floor();
    let t = trim as usize;
    if s.len() < 2 * t + 1 {
        return Err(ErrCode::NUM);
    }
    num(mean(&s[t..s.len() - t]))
}

// â”€â”€ Combinatorics / transforms â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

fn permut_fn(ev: &Evaluator, a: &[Node]) -> R {
    let n = n1(ev, a, 0)?;
    let k = n1(ev, a, 1)?;
    let (ni, ki) = (js::trunc(n), js::trunc(k));
    if ni < 0.0 || ki < 0.0 || ki > ni {
        return Err(ErrCode::NUM);
    }
    num(permut(ni, ki))
}

fn permutationa(ev: &Evaluator, a: &[Node]) -> R {
    let n = n1(ev, a, 0)?;
    let k = n1(ev, a, 1)?;
    let (ni, ki) = (js::trunc(n), js::trunc(k));
    if ni < 0.0 || ki < 0.0 {
        return Err(ErrCode::NUM);
    }
    num(js::pow(ni, ki))
}

fn fisher(ev: &Evaluator, a: &[Node]) -> R {
    let x = n1(ev, a, 0)?;
    if x <= -1.0 || x >= 1.0 {
        return Err(ErrCode::NUM);
    }
    num(0.5 * ((1.0 + x) / (1.0 - x)).ln())
}

fn fisherinv(ev: &Evaluator, a: &[Node]) -> R {
    let y = n1(ev, a, 0)?;
    let e = (2.0 * y).exp();
    num((e - 1.0) / (e + 1.0))
}

fn gammaln_fn(ev: &Evaluator, a: &[Node]) -> R {
    let x = n1(ev, a, 0)?;
    if x <= 0.0 {
        return Err(ErrCode::NUM);
    }
    num(gammaln(x))
}

// â”€â”€ Normal distribution â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

fn norm_dist(ev: &Evaluator, a: &[Node]) -> R {
    let x = n1(ev, a, 0)?;
    let m = n1(ev, a, 1)?;
    let sd = n1(ev, a, 2)?;
    if sd <= 0.0 {
        return Err(ErrCode::NUM);
    }
    let cum = to_bool(&ev.scalar(arg(a, 3)?));
    let z = (x - m) / sd;
    num(if cum { norm_cdf(z) } else { norm_pdf(z) / sd })
}

fn norm_s_dist(ev: &Evaluator, a: &[Node]) -> R {
    let z = n1(ev, a, 0)?;
    let cum = if a.len() > 1 { to_bool(&ev.scalar(&a[1])) } else { true };
    num(if cum { norm_cdf(z) } else { norm_pdf(z) })
}

fn normsdist(ev: &Evaluator, a: &[Node]) -> R {
    let z = n1(ev, a, 0)?;
    num(norm_cdf(z))
}

fn norm_inv(ev: &Evaluator, a: &[Node]) -> R {
    let p = n1(ev, a, 0)?;
    let m = n1(ev, a, 1)?;
    let sd = n1(ev, a, 2)?;
    if p <= 0.0 || p >= 1.0 {
        return Err(ErrCode::NUM);
    }
    if sd <= 0.0 {
        return Err(ErrCode::NUM);
    }
    num(m + sd * norm_inv_std(p))
}

fn norm_s_inv(ev: &Evaluator, a: &[Node]) -> R {
    let p = n1(ev, a, 0)?;
    if p <= 0.0 || p >= 1.0 {
        return Err(ErrCode::NUM);
    }
    num(norm_inv_std(p))
}

fn gauss(ev: &Evaluator, a: &[Node]) -> R {
    let z = n1(ev, a, 0)?;
    num(norm_cdf(z) - 0.5)
}

fn phi(ev: &Evaluator, a: &[Node]) -> R {
    let z = n1(ev, a, 0)?;
    num(norm_pdf(z))
}

// â”€â”€ Poisson / binomial / exponential / confidence â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

fn poisson(ev: &Evaluator, a: &[Node]) -> R {
    let k = n1(ev, a, 0)?;
    let lambda = n1(ev, a, 1)?;
    let cum = to_bool(&ev.scalar(arg(a, 2)?));
    let kk = js::trunc(k);
    if kk < 0.0 || lambda < 0.0 {
        return Err(ErrCode::NUM);
    }
    if cum {
        let mut s = 0.0;
        let mut i = 0.0;
        while i <= kk {
            s += (-lambda + i * lambda.ln() - ln_factorial(i)).exp();
            i += 1.0;
        }
        return num(s);
    }
    num((-lambda + kk * lambda.ln() - ln_factorial(kk)).exp())
}

fn binom(ev: &Evaluator, a: &[Node]) -> R {
    let k = n1(ev, a, 0)?;
    let n = n1(ev, a, 1)?;
    let p = n1(ev, a, 2)?;
    let cum = to_bool(&ev.scalar(arg(a, 3)?));
    let (kk, nn) = (js::trunc(k), js::trunc(n));
    if kk < 0.0 || kk > nn || p < 0.0 || p > 1.0 {
        return Err(ErrCode::NUM);
    }
    let pmf = |i: f64| binom_coef(nn, i) * js::pow(p, i) * js::pow(1.0 - p, nn - i);
    if cum {
        let mut s = 0.0;
        let mut i = 0.0;
        while i <= kk {
            s += pmf(i);
            i += 1.0;
        }
        return num(s);
    }
    num(pmf(kk))
}

fn expon(ev: &Evaluator, a: &[Node]) -> R {
    let x = n1(ev, a, 0)?;
    let lambda = n1(ev, a, 1)?;
    let cum = to_bool(&ev.scalar(arg(a, 2)?));
    if x < 0.0 || lambda <= 0.0 {
        return Err(ErrCode::NUM);
    }
    num(if cum { 1.0 - (-lambda * x).exp() } else { lambda * (-lambda * x).exp() })
}

fn confidence(ev: &Evaluator, a: &[Node]) -> R {
    let alpha = n1(ev, a, 0)?;
    let sd = n1(ev, a, 1)?;
    let size = n1(ev, a, 2)?;
    if alpha <= 0.0 || alpha >= 1.0 {
        return Err(ErrCode::NUM);
    }
    if sd <= 0.0 {
        return Err(ErrCode::NUM);
    }
    let nn = js::trunc(size);
    if nn < 1.0 {
        return Err(ErrCode::NUM);
    }
    let z = norm_inv_std(1.0 - alpha / 2.0);
    num(z * sd / nn.sqrt())
}

pub fn register(r: &mut Registry) {
    let list: &[(&'static str, crate::formula::eval::Fn)] = &[
        ("AVEDEV", avedev),
        ("AVERAGEA", averagea),
        ("DEVSQ", devsq),
        ("GEOMEAN", geomean),
        ("HARMEAN", harmean),
        ("KURT", kurt),
        ("SKEW", skew),
        ("SKEW.P", skew_p),
        ("LARGE", large),
        ("SMALL", small),
        ("MAXA", maxa),
        ("MINA", mina),
        ("MAXIFS", maxifs),
        ("MINIFS", minifs),
        ("AVERAGEIFS", averageifs),
        ("MODE", mode_sngl),
        ("MODE.SNGL", mode_sngl),
        ("MODE.MULT", mode_mult),
        ("VAR.S", var_sample),
        ("VAR", var_sample),
        ("VAR.P", var_pop),
        ("VARP", var_pop),
        ("VARA", vara),
        ("VARPA", varpa),
        ("STDEV.S", stdev_sample),
        ("STDEV", stdev_sample),
        ("STDEV.P", stdev_pop),
        ("STDEVP", stdev_pop),
        ("STDEVA", stdeva),
        ("STDEVPA", stdevpa),
        ("CORREL", correl_fn),
        ("PEARSON", correl_fn),
        ("RSQ", rsq),
        ("COVARIANCE.P", covariance_p),
        ("COVARIANCE.S", covariance_s),
        ("SLOPE", slope),
        ("INTERCEPT", intercept),
        ("FORECAST", forecast),
        ("FORECAST.LINEAR", forecast),
        ("STEYX", steyx),
        ("PERCENTILE.INC", percentile_inc_fn),
        ("PERCENTILE", percentile_inc_fn),
        ("PERCENTILE.EXC", percentile_exc_fn),
        ("QUARTILE.INC", quartile_inc),
        ("QUARTILE", quartile_inc),
        ("QUARTILE.EXC", quartile_exc),
        ("PERCENTRANK.INC", percentrank_inc),
        ("PERCENTRANK", percentrank_inc),
        ("PERCENTRANK.EXC", percentrank_exc),
        ("RANK.EQ", rank_eq),
        ("RANK", rank_eq),
        ("RANK.AVG", rank_avg),
        ("STANDARDIZE", standardize),
        ("TRIMMEAN", trimmean),
        ("PERMUT", permut_fn),
        ("PERMUTATIONA", permutationa),
        ("FISHER", fisher),
        ("FISHERINV", fisherinv),
        ("GAMMALN", gammaln_fn),
        ("GAMMALN.PRECISE", gammaln_fn),
        ("NORM.DIST", norm_dist),
        ("NORMDIST", norm_dist),
        ("NORM.S.DIST", norm_s_dist),
        ("NORMSDIST", normsdist),
        ("NORM.INV", norm_inv),
        ("NORMINV", norm_inv),
        ("NORM.S.INV", norm_s_inv),
        ("NORMSINV", norm_s_inv),
        ("GAUSS", gauss),
        ("PHI", phi),
        ("POISSON.DIST", poisson),
        ("POISSON", poisson),
        ("BINOM.DIST", binom),
        ("BINOMDIST", binom),
        ("EXPON.DIST", expon),
        ("EXPONDIST", expon),
        ("CONFIDENCE.NORM", confidence),
        ("CONFIDENCE", confidence),
    ];
    for (name, f) in list {
        r.insert(name, *f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_interpolates() {
        assert_eq!(percentile_inc(&[1.0, 2.0, 3.0, 4.0], 0.5), Ok(2.5));
        assert_eq!(percentile_inc(&[5.0], 0.3), Ok(5.0));
        assert_eq!(percentile_inc(&[1.0, 2.0], 1.5), Err(ErrCode::NUM));
    }

    #[test]
    fn exclusive_percentile_range() {
        assert_eq!(percentile_exc(&[1.0, 2.0, 3.0], 0.1), Err(ErrCode::NUM));
        assert_eq!(percentile_exc(&[1.0, 2.0, 3.0], 0.5), Ok(2.0));
    }

    #[test]
    fn percent_rank_truncates() {
        assert_eq!(percent_rank(&[1.0, 2.0, 3.0, 4.0], 2.5, 3.0, false), Ok(0.5));
    }
}
