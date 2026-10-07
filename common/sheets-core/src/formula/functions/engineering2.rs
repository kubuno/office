//! Port of the web's `formula-fns/engineering2.ts`: the Bessel family (Numerical Recipes ports) and the
//! transcendental complex-number functions.

// The Numerical Recipes constants are kept literally (0.636619772 is not f64 FRAC_2_PI).
#![allow(clippy::approx_constant)]
use super::Registry;
use crate::formula::ast::Node;
use crate::formula::eval::{arg, Evaluator};
use crate::formula::value::{ErrCode, Value};
use crate::js;

type R = Result<Value, ErrCode>;

// ── Complex helpers ───────────────────────────────────────────────────────────

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

// ── Bessel functions ──────────────────────────────────────────────────────────

fn bessj0(x: f64) -> f64 {
    let ax = x.abs();
    if ax < 8.0 {
        let y = x * x;
        let p1 = -2957821389.0 + y * (7062834065.0 + y * (-512359803.6 + y * (10879881.29 + y * (-86327.92757 + y * 228.4622733))));
        let p2 = 40076544269.0 + y * (745249964.8 + y * (7189466.438 + y * (47447.2647 + y * (226.1030244 + y))));
        return p1 / p2;
    }
    let z = 8.0 / ax;
    let y = z * z;
    let xx = ax - 0.785398164;
    let p1 = 1.0 + y * (-0.1098628627e-2 + y * (0.2734510407e-4 + y * (-0.2073370639e-5 + y * 0.2093887211e-6)));
    let p2 = -0.1562499995e-1 + y * (0.1430488765e-3 + y * (-0.6911147651e-5 + y * (0.7621095161e-6 + y * -0.934935152e-7)));
    (0.636619772 / ax).sqrt() * (xx.cos() * p1 - z * xx.sin() * p2)
}

fn bessj1(x: f64) -> f64 {
    let ax = x.abs();
    if ax < 8.0 {
        let y = x * x;
        let p1 = x * (72362614232.0 + y * (-7895059235.0 + y * (242396853.1 + y * (-2972611.439 + y * (15704.48260 + y * -30.16036606)))));
        let p2 = 144725228442.0 + y * (2300535178.0 + y * (18583304.74 + y * (99447.43394 + y * (376.9991397 + y))));
        return p1 / p2;
    }
    let z = 8.0 / ax;
    let y = z * z;
    let xx = ax - 2.356194491;
    let p1 = 1.0 + y * (0.183105e-2 + y * (-0.3516396496e-4 + y * (0.2457520174e-5 + y * -0.240337019e-6)));
    let p2 = 0.04687499995 + y * (-0.2002690873e-3 + y * (0.8449199096e-5 + y * (-0.88228987e-6 + y * 0.105787412e-6)));
    let ans = (0.636619772 / ax).sqrt() * (xx.cos() * p1 - z * xx.sin() * p2);
    if x < 0.0 {
        -ans
    } else {
        ans
    }
}

fn bessj(n: f64, x: f64) -> f64 {
    if n == 0.0 {
        return bessj0(x);
    }
    if n == 1.0 {
        return bessj1(x);
    }
    let acc = 40.0;
    let bigno = 1.0e10;
    let bigni = 1.0e-10;
    let ax = x.abs();
    if ax == 0.0 {
        return 0.0;
    }
    let mut ans;
    if ax > n {
        let tox = 2.0 / ax;
        let mut bjm = bessj0(ax);
        let mut bj = bessj1(ax);
        let mut j = 1.0;
        while j < n {
            let bjp = j * tox * bj - bjm;
            bjm = bj;
            bj = bjp;
            j += 1.0;
        }
        ans = bj;
    } else {
        let tox = 2.0 / ax;
        let m = 2.0 * ((n + (acc * n).sqrt().floor()) / 2.0).floor();
        let mut jsum = false;
        let mut bjp = 0.0;
        let mut bj = 1.0;
        let mut sum = 0.0;
        ans = 0.0;
        let mut j = m;
        while j > 0.0 {
            let bjm = j * tox * bj - bjp;
            bjp = bj;
            bj = bjm;
            if bj.abs() > bigno {
                bj *= bigni;
                bjp *= bigni;
                ans *= bigni;
                sum *= bigni;
            }
            if jsum {
                sum += bj;
            }
            jsum = !jsum;
            if j == n {
                ans = bjp;
            }
            j -= 1.0;
        }
        sum = 2.0 * sum - bj;
        ans /= sum;
    }
    if x < 0.0 && n % 2.0 == 1.0 {
        -ans
    } else {
        ans
    }
}

fn bessy0(x: f64) -> f64 {
    if x < 8.0 {
        let y = x * x;
        let p1 = -2957821389.0 + y * (7062834065.0 + y * (-512359803.6 + y * (10879881.29 + y * (-86327.92757 + y * 228.4622733))));
        let p2 = 40076544269.0 + y * (745249964.8 + y * (7189466.438 + y * (47447.26470 + y * (226.1030244 + y))));
        return p1 / p2 + 0.636619772 * bessj0(x) * x.ln();
    }
    let z = 8.0 / x;
    let y = z * z;
    let xx = x - 0.785398164;
    let p1 = 1.0 + y * (-0.1098628627e-2 + y * (0.2734510407e-4 + y * (-0.2073370639e-5 + y * 0.2093887211e-6)));
    let p2 = -0.1562499995e-1 + y * (0.1430488765e-3 + y * (-0.6911147651e-5 + y * (0.7621095161e-6 + y * -0.934945152e-7)));
    (0.636619772 / x).sqrt() * (xx.sin() * p1 + z * xx.cos() * p2)
}

fn bessy1(x: f64) -> f64 {
    if x < 8.0 {
        let y = x * x;
        let p1 = x
            * (-4.900604943e13
                + y * (1.275274390e13 + y * (-5.153438139e11 + y * (7.349264551e9 + y * (-4.237922726e7 + y * 8.511937935e4)))));
        let p2 = 2.499580570e14
            + y * (4.244419664e12 + y * (3.733650367e10 + y * (2.245904002e8 + y * (1.020426050e6 + y * (3.549632885e3 + y)))));
        return p1 / p2 + 0.636619772 * (bessj1(x) * x.ln() - 1.0 / x);
    }
    let z = 8.0 / x;
    let y = z * z;
    let xx = x - 2.356194491;
    let p1 = 1.0 + y * (0.183105e-2 + y * (-0.3516396496e-4 + y * (0.2457520174e-5 + y * -0.240337019e-6)));
    let p2 = 0.04687499995 + y * (-0.2002690873e-3 + y * (0.8449199096e-5 + y * (-0.88228987e-6 + y * 0.105787412e-6)));
    (0.636619772 / x).sqrt() * (xx.sin() * p1 + z * xx.cos() * p2)
}

fn bessy(n: f64, x: f64) -> f64 {
    if n == 0.0 {
        return bessy0(x);
    }
    if n == 1.0 {
        return bessy1(x);
    }
    let tox = 2.0 / x;
    let mut by = bessy1(x);
    let mut bym = bessy0(x);
    let mut j = 1.0;
    while j < n {
        let byp = j * tox * by - bym;
        bym = by;
        by = byp;
        j += 1.0;
    }
    by
}

fn bessi0(x: f64) -> f64 {
    let ax = x.abs();
    if ax < 3.75 {
        let mut y = x / 3.75;
        y *= y;
        return 1.0 + y * (3.5156229 + y * (3.0899424 + y * (1.2067492 + y * (0.2659732 + y * (0.360768e-1 + y * 0.45813e-2)))));
    }
    let y = 3.75 / ax;
    (ax.exp() / ax.sqrt())
        * (0.39894228
            + y * (0.1328592e-1
                + y * (0.225319e-2
                    + y * (-0.157565e-2 + y * (0.916281e-2 + y * (-0.2057706e-1 + y * (0.2635537e-1 + y * (-0.1647633e-1 + y * 0.392377e-2))))))))
}

fn bessi1(x: f64) -> f64 {
    let ax = x.abs();
    let mut ans;
    if ax < 3.75 {
        let mut y = x / 3.75;
        y *= y;
        ans = ax * (0.5 + y * (0.87890594 + y * (0.51498869 + y * (0.15084934 + y * (0.2658733e-1 + y * (0.301532e-2 + y * 0.32411e-3))))));
    } else {
        let y = 3.75 / ax;
        ans = 0.2282967e-1 + y * (-0.2895312e-1 + y * (0.1787654e-1 - y * 0.420059e-2));
        ans = 0.39894228 + y * (-0.3988024e-1 + y * (-0.362018e-2 + y * (0.163801e-2 + y * (-0.1031555e-1 + y * ans))));
        ans *= ax.exp() / ax.sqrt();
    }
    if x < 0.0 {
        -ans
    } else {
        ans
    }
}

fn bessi(n: f64, x: f64) -> f64 {
    if n == 0.0 {
        return bessi0(x);
    }
    if n == 1.0 {
        return bessi1(x);
    }
    if x == 0.0 {
        return 0.0;
    }
    let acc = 40.0;
    let bigno = 1.0e10;
    let bigni = 1.0e-10;
    let tox = 2.0 / x.abs();
    let mut bip = 0.0;
    let mut bi = 1.0;
    let mut ans = 0.0;
    let m = 2.0 * (n + (acc * n).sqrt().floor());
    let mut j = m;
    while j > 0.0 {
        let bim = bip + j * tox * bi;
        bip = bi;
        bi = bim;
        if bi.abs() > bigno {
            ans *= bigni;
            bi *= bigni;
            bip *= bigni;
        }
        if j == n {
            ans = bip;
        }
        j -= 1.0;
    }
    ans *= bessi0(x) / bi;
    if x < 0.0 && n % 2.0 == 1.0 {
        -ans
    } else {
        ans
    }
}

fn bessk0(x: f64) -> f64 {
    if x <= 2.0 {
        let y = (x * x) / 4.0;
        return -(x / 2.0).ln() * bessi0(x)
            + (-0.57721566 + y * (0.42278420 + y * (0.23069756 + y * (0.3488590e-1 + y * (0.262698e-2 + y * (0.10750e-3 + y * 0.74e-5))))));
    }
    let y = 2.0 / x;
    ((-x).exp() / x.sqrt())
        * (1.25331414 + y * (-0.7832358e-1 + y * (0.2189568e-1 + y * (-0.1062446e-1 + y * (0.587872e-2 + y * (-0.251540e-2 + y * 0.53208e-3))))))
}

fn bessk1(x: f64) -> f64 {
    if x <= 2.0 {
        let y = (x * x) / 4.0;
        return (x / 2.0).ln() * bessi1(x)
            + (1.0 / x) * (1.0 + y * (0.15443144 + y * (-0.67278579 + y * (-0.18156897 + y * (-0.1919402e-1 + y * (-0.110404e-2 + y * -0.4686e-4))))));
    }
    let y = 2.0 / x;
    ((-x).exp() / x.sqrt())
        * (1.25331414 + y * (0.23498619 + y * (-0.3655620e-1 + y * (0.1504268e-1 + y * (-0.780353e-2 + y * (0.325614e-2 + y * -0.68245e-3))))))
}

fn bessk(n: f64, x: f64) -> f64 {
    if n == 0.0 {
        return bessk0(x);
    }
    if n == 1.0 {
        return bessk1(x);
    }
    let tox = 2.0 / x;
    let mut bkm = bessk0(x);
    let mut bk = bessk1(x);
    let mut j = 1.0;
    while j < n {
        let bkp = bkm + j * tox * bk;
        bkm = bk;
        bk = bkp;
        j += 1.0;
    }
    bk
}

/// `besselArgs`: `(x, n)` with `n` truncated; errors in the arguments are `#VALUE!`.
fn bessel_args(ev: &Evaluator, a: &[Node]) -> Result<(f64, f64), ErrCode> {
    let x = ev.num(arg(a, 0)?).map_err(|_| ErrCode::VALUE)?;
    let nv = ev.num(arg(a, 1)?).map_err(|_| ErrCode::VALUE)?;
    if !x.is_finite() || !nv.is_finite() {
        return Err(ErrCode::NUM);
    }
    let n = js::trunc(nv);
    if n < 0.0 {
        return Err(ErrCode::NUM);
    }
    Ok((x, n))
}

// ── Complex transcendental functions ──────────────────────────────────────────

fn cdiv(a: Complex, b: Complex) -> Result<Complex, ErrCode> {
    let den = b.re * b.re + b.im * b.im;
    if den == 0.0 {
        return Err(ErrCode::NUM);
    }
    Ok(Complex { re: (a.re * b.re + a.im * b.im) / den, im: (a.im * b.re - a.re * b.im) / den, suffix: a.suffix })
}

fn recip(c: Complex) -> Result<Complex, ErrCode> {
    cdiv(Complex { re: 1.0, im: 0.0, suffix: c.suffix }, c)
}

fn cexp(c: Complex) -> Complex {
    let e = c.re.exp();
    Complex { re: e * c.im.cos(), im: e * c.im.sin(), suffix: c.suffix }
}

fn csin(c: Complex) -> Complex {
    Complex { re: c.re.sin() * c.im.cosh(), im: c.re.cos() * c.im.sinh(), suffix: c.suffix }
}

fn ccos(c: Complex) -> Complex {
    Complex { re: c.re.cos() * c.im.cosh(), im: -c.re.sin() * c.im.sinh(), suffix: c.suffix }
}

fn csinh(c: Complex) -> Complex {
    Complex { re: c.re.sinh() * c.im.cos(), im: c.re.cosh() * c.im.sin(), suffix: c.suffix }
}

fn ccosh(c: Complex) -> Complex {
    Complex { re: c.re.cosh() * c.im.cos(), im: c.re.sinh() * c.im.sin(), suffix: c.suffix }
}

fn modulus(c: Complex) -> f64 {
    (c.re * c.re + c.im * c.im).sqrt()
}

/// `imUnary(fn)`: string in, string out; the result takes the argument's suffix.
fn im_unary(ev: &Evaluator, a: &[Node], f: fn(Complex) -> Result<Complex, ErrCode>) -> R {
    let c = get_complex(ev, arg(a, 0)?)?;
    let r = f(c)?;
    Ok(Value::str(fmt_complex(r.re, r.im, c.suffix)))
}

fn log_base(c: Complex, ln_base: f64) -> Result<Complex, ErrCode> {
    let m = modulus(c);
    if m == 0.0 {
        return Err(ErrCode::NUM);
    }
    Ok(Complex { re: m.ln() / ln_base, im: c.im.atan2(c.re) / ln_base, suffix: c.suffix })
}

fn impower(ev: &Evaluator, a: &[Node]) -> R {
    let c = get_complex(ev, arg(a, 0)?)?;
    let p = ev.num(arg(a, 1)?)?;
    let mod_ = modulus(c);
    let text = |re: f64, im: f64| Ok(Value::str(fmt_complex(re, im, c.suffix)));
    if mod_ == 0.0 {
        if p > 0.0 {
            return text(0.0, 0.0);
        }
        if p == 0.0 {
            return text(1.0, 0.0);
        }
        return Err(ErrCode::NUM);
    }
    let arg_ = c.im.atan2(c.re);
    let rp = js::pow(mod_, p);
    text(rp * (p * arg_).cos(), rp * (p * arg_).sin())
}

// ── Registry ──────────────────────────────────────────────────────────────────

pub fn register(r: &mut Registry) {
    r.insert("BESSELJ", |ev, a| {
        let (x, n) = bessel_args(ev, a)?;
        Ok(Value::num(bessj(n, x)))
    });
    r.insert("BESSELY", |ev, a| {
        let (x, n) = bessel_args(ev, a)?;
        if x <= 0.0 {
            return Err(ErrCode::NUM);
        }
        Ok(Value::num(bessy(n, x)))
    });
    r.insert("BESSELI", |ev, a| {
        let (x, n) = bessel_args(ev, a)?;
        Ok(Value::num(bessi(n, x)))
    });
    r.insert("BESSELK", |ev, a| {
        let (x, n) = bessel_args(ev, a)?;
        if x <= 0.0 {
            return Err(ErrCode::NUM);
        }
        Ok(Value::num(bessk(n, x)))
    });

    r.insert("IMEXP", |ev, a| im_unary(ev, a, |c| Ok(cexp(c))));
    r.insert("IMLN", |ev, a| im_unary(ev, a, |c| log_base(c, 1.0)));
    r.insert("IMLOG10", |ev, a| im_unary(ev, a, |c| log_base(c, std::f64::consts::LN_10)));
    r.insert("IMLOG2", |ev, a| im_unary(ev, a, |c| log_base(c, std::f64::consts::LN_2)));
    r.insert("IMPOWER", impower);
    r.insert("IMSQRT", |ev, a| {
        im_unary(ev, a, |c| {
            let r = modulus(c).sqrt();
            let half_arg = c.im.atan2(c.re) / 2.0;
            Ok(Complex { re: r * half_arg.cos(), im: r * half_arg.sin(), suffix: c.suffix })
        })
    });

    r.insert("IMSIN", |ev, a| im_unary(ev, a, |c| Ok(csin(c))));
    r.insert("IMCOS", |ev, a| im_unary(ev, a, |c| Ok(ccos(c))));
    r.insert("IMTAN", |ev, a| im_unary(ev, a, |c| cdiv(csin(c), ccos(c))));
    r.insert("IMCSC", |ev, a| im_unary(ev, a, |c| recip(csin(c))));
    r.insert("IMSEC", |ev, a| im_unary(ev, a, |c| recip(ccos(c))));
    r.insert("IMCOT", |ev, a| im_unary(ev, a, |c| cdiv(ccos(c), csin(c))));

    r.insert("IMSINH", |ev, a| im_unary(ev, a, |c| Ok(csinh(c))));
    r.insert("IMCOSH", |ev, a| im_unary(ev, a, |c| Ok(ccosh(c))));
    r.insert("IMCSCH", |ev, a| im_unary(ev, a, |c| recip(csinh(c))));
    r.insert("IMSECH", |ev, a| im_unary(ev, a, |c| recip(ccosh(c))));
}

#[cfg(test)]
mod tests {
    use crate::formula::ast::Node;
    use crate::formula::eval::{evaluate_node, CellMap, Ctx, Data, FixedEnv};
    use crate::formula::value::Value;
    use crate::jsdate::LocalNow;
    use std::collections::HashMap;

    /// Calls a function by name on string arguments (IMLOG10/IMLOG2 parse as references in formula text,
    /// as on the web, so they can only be reached through the AST).
    fn call(name: &str, args: &[&str]) -> Value {
        let env = FixedEnv::new(LocalNow { year: 2026, month: 10, day: 7, hour: 14, minute: 30, second: 15, millis: 250 });
        let ctx = Ctx::new(&env);
        let cells = CellMap::new();
        let names = HashMap::new();
        let node = Node::Call { name: name.to_string(), args: args.iter().map(|s| Node::Str(s.to_string())).collect() };
        evaluate_node(&node, Data { cells: &cells, sheets: None, names: &names }, &ctx, None)
    }

    #[test]
    fn complex_logarithms() {
        assert_eq!(call("IMLOG10", &["10"]), Value::str("1"));
        assert_eq!(call("IMLOG2", &["8"]), Value::str("3"));
        assert_eq!(call("IMLOG2", &["0"]), Value::err(crate::formula::value::ErrCode::NUM));
    }
}
