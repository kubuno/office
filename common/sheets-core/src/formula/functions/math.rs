//! Port of the web's `formula-fns/math.ts`: Math & Trigonometry functions.
#![allow(clippy::manual_range_contains, clippy::needless_range_loop)]

use super::Registry;
use crate::formula::ast::Node;
use crate::formula::eval::{arg, Evaluator};
use crate::formula::functions::base::num_or;
use crate::formula::value::{flatten, to_bool, to_num, ErrCode, Matrix, Scalar, Value};
use crate::js;

type R = Result<Value, ErrCode>;

/// Largest result (in cells) a generator function may build; the web would run out of memory or throw
/// long before, and native code must not abort.
const MAX_CELLS: f64 = 50_000_000.0;

// ── Local helpers ───────────────────────────────────────────────────────────

fn ok(x: f64) -> R {
    Ok(Value::num(x))
}

/// Factorial of a non-negative integer (no overflow guard beyond Infinity).
fn factorial(n: f64) -> f64 {
    let mut r = 1.0;
    let mut i = 2.0;
    while i <= n {
        r *= i;
        // Once infinite the product stays infinite: stop instead of spinning.
        if r.is_infinite() {
            break;
        }
        i += 1.0;
    }
    r
}

/// Greatest common divisor of two non-negative integers.
fn gcd2(a: f64, b: f64) -> f64 {
    let mut a = a.trunc().abs();
    let mut b = b.trunc().abs();
    while b != 0.0 && !b.is_nan() {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

/// A matrix of numbers from an evaluated argument (`toNumMatrix`); any cell that does not coerce makes
/// the whole thing `#VALUE!`.
fn to_num_matrix(v: Value) -> Result<Vec<Vec<f64>>, ErrCode> {
    let m: Matrix = match v {
        Value::M(m) => m,
        Value::S(s) => vec![vec![s]],
        // `toNum` of a lambda object is NaN → `#VALUE!`.
        Value::L(_) => return Err(ErrCode::VALUE),
    };
    let mut out = Vec::with_capacity(m.len());
    for row in &m {
        let mut r = Vec::with_capacity(row.len());
        for cell in row {
            r.push(to_num(cell).map_err(|_| ErrCode::VALUE)?);
        }
        out.push(r);
    }
    Ok(out)
}

/// `m[i][j]` where a missing cell is `undefined` and reads as NaN in arithmetic.
fn at(m: &[Vec<f64>], i: usize, j: usize) -> f64 {
    m.get(i).and_then(|r| r.get(j)).copied().unwrap_or(f64::NAN)
}

/// The arguments of a one-number function: `num(ev, a[0])` then `f`.
fn un(ev: &Evaluator, a: &[Node], f: impl Fn(f64) -> R) -> R {
    let x = ev.num(arg(a, 0)?)?;
    f(x)
}

macro_rules! unary {
    ($name:ident, |$x:ident| $body:expr) => {
        fn $name(ev: &Evaluator, a: &[Node]) -> R {
            un(ev, a, |$x| $body)
        }
    };
}

// ── Trigonometry ────────────────────────────────────────────────────────────

unary!(sin, |x| ok(x.sin()));
unary!(cos, |x| ok(x.cos()));
unary!(tan, |x| ok(x.tan()));
unary!(asin, |x| if !(-1.0..=1.0).contains(&x) && !x.is_nan() { Err(ErrCode::NUM) } else { ok(x.asin()) });
unary!(acos, |x| if !(-1.0..=1.0).contains(&x) && !x.is_nan() { Err(ErrCode::NUM) } else { ok(x.acos()) });
unary!(atan, |x| ok(x.atan()));

fn atan2(ev: &Evaluator, a: &[Node]) -> R {
    let x = ev.num(arg(a, 0)?)?;
    let y = ev.num(arg(a, 1)?)?;
    if x == 0.0 && y == 0.0 {
        return Err(ErrCode::DIV0);
    }
    // Excel's ATAN2(x_num, y_num) maps to Math.atan2(y, x).
    ok(y.atan2(x))
}

unary!(cot, |x| {
    let t = x.tan();
    if t == 0.0 { Err(ErrCode::DIV0) } else { ok(1.0 / t) }
});
unary!(sec, |x| {
    let c = x.cos();
    if c == 0.0 { Err(ErrCode::DIV0) } else { ok(1.0 / c) }
});
unary!(csc, |x| {
    let s = x.sin();
    if s == 0.0 { Err(ErrCode::DIV0) } else { ok(1.0 / s) }
});
unary!(acot, |x| {
    if x == 0.0 {
        return ok(std::f64::consts::PI / 2.0);
    }
    if x > 0.0 { ok((1.0 / x).atan()) } else { ok((1.0 / x).atan() + std::f64::consts::PI) }
});

// ── Hyperbolic ──────────────────────────────────────────────────────────────

unary!(sinh, |x| ok(x.sinh()));
unary!(cosh, |x| ok(x.cosh()));
unary!(tanh, |x| ok(x.tanh()));
unary!(asinh, |x| ok(x.asinh()));
unary!(acosh, |x| if x < 1.0 { Err(ErrCode::NUM) } else { ok(x.acosh()) });
unary!(atanh, |x| if x <= -1.0 || x >= 1.0 { Err(ErrCode::NUM) } else { ok(x.atanh()) });
unary!(coth, |x| {
    let t = x.tanh();
    if t == 0.0 { Err(ErrCode::DIV0) } else { ok(1.0 / t) }
});
unary!(sech, |x| ok(1.0 / x.cosh()));
unary!(csch, |x| {
    let s = x.sinh();
    if s == 0.0 { Err(ErrCode::DIV0) } else { ok(1.0 / s) }
});
unary!(acoth, |x| if x.abs() <= 1.0 { Err(ErrCode::NUM) } else { ok((1.0 / x).atanh()) });

// ── Angles ──────────────────────────────────────────────────────────────────

unary!(degrees, |x| ok(x * 180.0 / std::f64::consts::PI));
unary!(radians, |x| ok(x * std::f64::consts::PI / 180.0));

// ── Logs ────────────────────────────────────────────────────────────────────

unary!(log10, |x| if x <= 0.0 { Err(ErrCode::NUM) } else { ok(x.log10()) });

// ── Rounding family ─────────────────────────────────────────────────────────

unary!(even, |x| {
    let c = (x.abs() / 2.0).ceil() * 2.0;
    ok(if x < 0.0 { -c } else { c })
});

unary!(odd, |x| {
    let s = if x < 0.0 { -1.0 } else { 1.0 };
    let ax = x.abs();
    let mut r = ax.ceil();
    if r % 2.0 == 0.0 {
        r += 1.0;
    }
    if ax == 0.0 {
        r = 1.0;
    }
    ok(s * r)
});

fn mround(ev: &Evaluator, a: &[Node]) -> R {
    let x = ev.num(arg(a, 0)?)?;
    let m = ev.num(arg(a, 1)?)?;
    if m == 0.0 {
        return ok(0.0);
    }
    if (x > 0.0 && m < 0.0) || (x < 0.0 && m > 0.0) {
        return Err(ErrCode::NUM);
    }
    ok(js::round(x / m) * m)
}

fn trunc(ev: &Evaluator, a: &[Node]) -> R {
    let x = ev.num(arg(a, 0)?)?;
    let d = num_or(ev, a, 1, 0.0)?;
    let f = js::pow(10.0, d);
    ok((x * f).trunc() / f)
}

/// `a[1] !== undefined ? num(ev, a[1]) : 1`.
fn significance(ev: &Evaluator, a: &[Node]) -> Result<f64, ErrCode> {
    num_or(ev, a, 1, 1.0)
}

fn ceiling_math(ev: &Evaluator, a: &[Node]) -> R {
    let x = ev.num(arg(a, 0)?)?;
    let sig = significance(ev, a)?;
    let mode = num_or(ev, a, 2, 0.0)?;
    if sig == 0.0 {
        return ok(0.0);
    }
    let s = sig.abs();
    // mode != 0 rounds away from zero for negatives, else toward zero.
    if x >= 0.0 {
        return ok((x / s).ceil() * s);
    }
    if mode != 0.0 {
        ok(-(x.abs() / s).ceil() * s)
    } else {
        ok(-(x.abs() / s).floor() * s)
    }
}

fn ceiling_precise(ev: &Evaluator, a: &[Node]) -> R {
    let x = ev.num(arg(a, 0)?)?;
    let sig = significance(ev, a)?;
    let s = sig.abs();
    if s == 0.0 {
        return ok(0.0);
    }
    ok((x / s).ceil() * s)
}

fn floor_math(ev: &Evaluator, a: &[Node]) -> R {
    let x = ev.num(arg(a, 0)?)?;
    let sig = significance(ev, a)?;
    let mode = num_or(ev, a, 2, 0.0)?;
    if sig == 0.0 {
        return ok(0.0);
    }
    let s = sig.abs();
    if x >= 0.0 {
        return ok((x / s).floor() * s);
    }
    if mode != 0.0 {
        ok(-(x.abs() / s).floor() * s)
    } else {
        ok(-(x.abs() / s).ceil() * s)
    }
}

fn floor_precise(ev: &Evaluator, a: &[Node]) -> R {
    let x = ev.num(arg(a, 0)?)?;
    let sig = significance(ev, a)?;
    let s = sig.abs();
    if s == 0.0 {
        return ok(0.0);
    }
    ok((x / s).floor() * s)
}

// ── Sign / quotient ─────────────────────────────────────────────────────────

unary!(sign, |x| ok(js::sign(x)));

fn quotient(ev: &Evaluator, a: &[Node]) -> R {
    let x = ev.num(arg(a, 0)?)?;
    let d = ev.num(arg(a, 1)?)?;
    if d == 0.0 {
        Err(ErrCode::DIV0)
    } else {
        ok((x / d).trunc())
    }
}

// ── Combinatorics / factorials ──────────────────────────────────────────────

unary!(fact, |x| {
    let n = x.trunc();
    if n < 0.0 { Err(ErrCode::NUM) } else { ok(factorial(n)) }
});

unary!(factdouble, |x| {
    let n = x.trunc();
    if n < -1.0 {
        return Err(ErrCode::NUM);
    }
    let mut r = 1.0;
    let mut i = n;
    while i > 1.0 {
        r *= i;
        if r.is_infinite() {
            break;
        }
        i -= 2.0;
    }
    ok(r)
});

fn combin(ev: &Evaluator, a: &[Node]) -> R {
    let n = ev.num(arg(a, 0)?)?;
    let k = ev.num(arg(a, 1)?)?;
    let (nn, kk) = (n.trunc(), k.trunc());
    if nn < 0.0 || kk < 0.0 || kk > nn {
        return Err(ErrCode::NUM);
    }
    // Multiplicative form avoids large intermediate factorials.
    let mut r = 1.0;
    let mut i = 0.0;
    while i < kk {
        r = r * (nn - i) / (i + 1.0);
        if r.is_infinite() {
            break;
        }
        i += 1.0;
    }
    ok(js::round(r))
}

fn combina(ev: &Evaluator, a: &[Node]) -> R {
    let n = ev.num(arg(a, 0)?)?;
    let k = ev.num(arg(a, 1)?)?;
    let (nn, kk) = (n.trunc(), k.trunc());
    if nn < 0.0 || kk < 0.0 {
        return Err(ErrCode::NUM);
    }
    if nn == 0.0 && kk == 0.0 {
        return ok(1.0);
    }
    // COMBINA(n,k) = COMBIN(n+k-1, k)
    let m = nn + kk - 1.0;
    let mut r = 1.0;
    let mut i = 0.0;
    while i < kk {
        r = r * (m - i) / (i + 1.0);
        if r.is_infinite() {
            break;
        }
        i += 1.0;
    }
    ok(js::round(r))
}

fn multinomial(ev: &Evaluator, a: &[Node]) -> R {
    let nums = ev.nums(a)?;
    let mut sum = 0.0;
    let mut denom = 1.0;
    for v in nums {
        let n = v.trunc();
        if n < 0.0 {
            return Err(ErrCode::NUM);
        }
        sum += n;
        denom *= factorial(n);
    }
    ok(factorial(sum) / denom)
}

// ── GCD / LCM ───────────────────────────────────────────────────────────────

fn gcd(ev: &Evaluator, a: &[Node]) -> R {
    let nums = ev.nums(a)?;
    let mut g = 0.0;
    for v in nums {
        if v < 0.0 {
            return Err(ErrCode::NUM);
        }
        g = gcd2(g, v);
    }
    ok(g)
}

fn lcm(ev: &Evaluator, a: &[Node]) -> R {
    let nums = ev.nums(a)?;
    let mut l = 1.0;
    for v in nums {
        let n = v.abs().trunc();
        if v < 0.0 {
            return Err(ErrCode::NUM);
        }
        if n == 0.0 {
            return ok(0.0);
        }
        l = l / gcd2(l, n) * n;
    }
    ok(l)
}

// ── Roman / Arabic / base conversion ────────────────────────────────────────

const ROMAN_TABLE: [(f64, &str); 13] = [
    (1000.0, "M"),
    (900.0, "CM"),
    (500.0, "D"),
    (400.0, "CD"),
    (100.0, "C"),
    (90.0, "XC"),
    (50.0, "L"),
    (40.0, "XL"),
    (10.0, "X"),
    (9.0, "IX"),
    (5.0, "V"),
    (4.0, "IV"),
    (1.0, "I"),
];

fn roman_value(c: char) -> Option<f64> {
    match c {
        'I' => Some(1.0),
        'V' => Some(5.0),
        'X' => Some(10.0),
        'L' => Some(50.0),
        'C' => Some(100.0),
        'D' => Some(500.0),
        'M' => Some(1000.0),
        _ => None,
    }
}

fn roman(ev: &Evaluator, a: &[Node]) -> R {
    let x = ev.num(arg(a, 0)?)?;
    let mut n = x.trunc();
    if n < 0.0 || n > 3999.0 {
        return Err(ErrCode::VALUE);
    }
    // Only classic form (mode 0) is supported. QUIRK: a NaN input passes the range test and gives "".
    let mut out = String::new();
    for (val, sym) in ROMAN_TABLE {
        while n >= val {
            out.push_str(sym);
            n -= val;
        }
    }
    Ok(Value::str(out))
}

fn arabic(ev: &Evaluator, a: &[Node]) -> R {
    let s0 = ev.str(arg(a, 0)?);
    let upper = js::upper(&s0);
    let s = js::trim(&upper);
    if s.is_empty() {
        return ok(0.0);
    }
    let (sign, rest) = match s.strip_prefix('-') {
        Some(r) => (-1.0, r),
        None => (1.0, s),
    };
    let mut total = 0.0;
    let mut prev = 0.0;
    for ch in rest.chars().rev() {
        let Some(v) = roman_value(ch) else { return Err(ErrCode::VALUE) };
        if v < prev {
            total -= v;
        } else {
            total += v;
            prev = v;
        }
    }
    ok(sign * total)
}

/// `Number.prototype.toString(radix)` (radix other than 10) for an integral, finite, non-negative value:
/// V8 divides by the radix, writing a zero digit each time, while the quotient is still >= 2^53, then
/// extracts digits from the (possibly non-integral) remainder; reproduced exactly.
fn to_radix(value: f64, radix: u32) -> String {
    if value == 0.0 {
        return "0".to_string();
    }
    let r = radix as f64;
    let mut integer = value.floor();
    let mut digits: Vec<u8> = Vec::new();
    while integer / r >= 9_007_199_254_740_992.0 {
        integer /= r;
        digits.push(b'0');
    }
    loop {
        let rem = integer % r;
        let d = rem as u32;
        digits.push(char::from_digit(d, 36).map_or(b'0', |c| c as u8));
        integer = (integer - rem) / r;
        if integer <= 0.0 {
            break;
        }
    }
    digits.reverse();
    String::from_utf8(digits).unwrap_or_default().to_ascii_lowercase()
}

fn base(ev: &Evaluator, a: &[Node]) -> R {
    let x = ev.num(arg(a, 0)?)?;
    let radix = ev.num(arg(a, 1)?)?;
    let min_len = num_or(ev, a, 2, 0.0)?;
    let n = x.trunc();
    let r = radix.trunc();
    if n < 0.0 || r < 2.0 || r > 36.0 {
        return Err(ErrCode::NUM);
    }
    let s = if n.is_nan() {
        "NAN".to_string()
    } else if n.is_infinite() {
        "INFINITY".to_string()
    } else {
        if r == 10.0 { js::upper(&js::to_string(n)) } else { js::upper(&to_radix(n, r as u32)) }
    };
    let len = s.len() as f64;
    if min_len > len {
        // Padding one zero at a time until the string is long enough; past the engine's maximum string
        // length the web throws (a `#VALUE!`).
        let target = min_len.ceil();
        if target > 536_870_888.0 {
            return Err(ErrCode::VALUE);
        }
        let pad = (target - len) as usize;
        return Ok(Value::str(format!("{}{}", "0".repeat(pad), s)));
    }
    Ok(Value::str(s))
}

fn decimal(ev: &Evaluator, a: &[Node]) -> R {
    let s0 = ev.str(arg(a, 0)?);
    let s = js::upper(js::trim(&s0));
    let radix = ev.num(arg(a, 1)?)?;
    let r = radix.trunc();
    if r < 2.0 || r > 36.0 {
        return Err(ErrCode::NUM);
    }
    if s.is_empty() {
        return ok(0.0);
    }
    let mut acc = 0.0;
    for ch in s.chars() {
        // `parseInt(ch, 36)`: only ASCII letters and digits are digits.
        let Some(d) = ch.to_digit(36) else { return Err(ErrCode::NUM) };
        let d = d as f64;
        if d >= r {
            return Err(ErrCode::NUM);
        }
        acc = acc * r + d;
    }
    ok(acc)
}

// ── Misc scalar math ────────────────────────────────────────────────────────

unary!(sqrtpi, |x| if x < 0.0 { Err(ErrCode::NUM) } else { ok((x * std::f64::consts::PI).sqrt()) });

// ── Aggregates over multiple args ───────────────────────────────────────────

fn sumsq(ev: &Evaluator, a: &[Node]) -> R {
    let nums = ev.nums(a)?;
    ok(nums.iter().fold(0.0, |acc, v| acc + v * v))
}

fn sumproduct(ev: &Evaluator, a: &[Node]) -> R {
    if a.is_empty() {
        return ok(0.0);
    }
    let mut arrays: Vec<Vec<f64>> = Vec::new();
    let mut len: Option<usize> = None;
    for node in a {
        let v = ev.eval(node);
        // A lambda flattens to itself on the web: not an error, and no number, so it counts as 0.
        let flat = if v.is_lambda() { vec![Scalar::empty()] } else { flatten(&v) };
        let mut nums = Vec::with_capacity(flat.len());
        for s in &flat {
            if let Scalar::Err(e) = s {
                return Err(e.clone());
            }
            // Non-numeric cells count as 0 in SUMPRODUCT.
            nums.push(to_num(s).unwrap_or(0.0));
        }
        match len {
            None => len = Some(nums.len()),
            Some(l) if l != nums.len() => return Err(ErrCode::VALUE),
            _ => {}
        }
        arrays.push(nums);
    }
    let len = len.unwrap_or(0);
    let mut sum = 0.0;
    for i in 0..len {
        let mut prod = 1.0;
        for arr in &arrays {
            prod *= arr[i];
        }
        sum += prod;
    }
    ok(sum)
}

/// `flatten(v).map(toNum)`: a lambda flattens to itself on the web and `toNum` of that object is `#VALUE!`.
fn flat_nums(v: &Value) -> Vec<Result<f64, ErrCode>> {
    if v.is_lambda() {
        return vec![Err(ErrCode::VALUE)];
    }
    flatten(v).iter().map(to_num).collect()
}

/// The two flattened arguments of SUMX2MY2 / SUMX2PY2 / SUMXMY2 folded with `f(x, y)`.
fn pairwise(ev: &Evaluator, a: &[Node], f: impl Fn(f64, f64) -> f64) -> R {
    let xs = flat_nums(&ev.eval(arg(a, 0)?));
    let ys = flat_nums(&ev.eval(arg(a, 1)?));
    if xs.len() != ys.len() {
        return Err(ErrCode::NA);
    }
    let mut sum = 0.0;
    for (xv, yv) in xs.iter().zip(&ys) {
        let (x, y) = (xv.clone()?, yv.clone()?);
        sum += f(x, y);
    }
    ok(sum)
}

fn sumx2my2(ev: &Evaluator, a: &[Node]) -> R {
    pairwise(ev, a, |x, y| x * x - y * y)
}

fn sumx2py2(ev: &Evaluator, a: &[Node]) -> R {
    pairwise(ev, a, |x, y| x * x + y * y)
}

fn sumxmy2(ev: &Evaluator, a: &[Node]) -> R {
    pairwise(ev, a, |x, y| {
        let d = x - y;
        d * d
    })
}

fn seriessum(ev: &Evaluator, a: &[Node]) -> R {
    let x = ev.num(arg(a, 0)?)?;
    let n = ev.num(arg(a, 1)?)?;
    let m = ev.num(arg(a, 2)?)?;
    let coeffs = flat_nums(&ev.eval(arg(a, 3)?));
    let mut sum = 0.0;
    for (i, cv) in coeffs.iter().enumerate() {
        let c = cv.clone()?;
        sum += c * js::pow(x, n + i as f64 * m);
    }
    ok(sum)
}

// ── SUBTOTAL ────────────────────────────────────────────────────────────────

/// Sum of squared deviations from the mean (`nums.reduce((acc, x) => acc + (x - mean) ** 2, 0)`).
fn sq_dev(nums: &[f64], mean: f64) -> f64 {
    nums.iter().fold(0.0, |acc, x| acc + js::pow(x - mean, 2.0))
}

fn subtotal(ev: &Evaluator, a: &[Node]) -> R {
    let code = ev.num(arg(a, 0)?)?;
    // Codes >= 101 ignore hidden rows; there is no row-visibility info, so they behave like 1-11.
    let f = if code >= 101.0 { code - 100.0 } else { code };
    let rest = a.get(1..).unwrap_or(&[]);
    let nums = ev.nums(rest)?;
    let len = nums.len();
    let n = len as f64;
    let sum = nums.iter().fold(0.0, |x, y| x + y);
    let r = if f == 1.0 {
        // AVERAGE
        if len > 0 { sum / n } else { return Err(ErrCode::DIV0) }
    } else if f == 2.0 {
        // COUNT
        n
    } else if f == 3.0 {
        // COUNTA
        let mut c = 0.0;
        for node in rest {
            for s in flatten(&ev.eval(node)) {
                if !s.is_blank() {
                    c += 1.0;
                }
            }
        }
        c
    } else if f == 4.0 {
        // MAX
        if len > 0 { js::max(&nums) } else { 0.0 }
    } else if f == 5.0 {
        // MIN
        if len > 0 { js::min(&nums) } else { 0.0 }
    } else if f == 6.0 {
        // PRODUCT
        nums.iter().fold(1.0, |x, y| x * y)
    } else if f == 7.0 {
        // STDEV (sample)
        if len < 2 {
            return Err(ErrCode::DIV0);
        }
        (sq_dev(&nums, sum / n) / (n - 1.0)).sqrt()
    } else if f == 8.0 {
        // STDEVP (population)
        if len < 1 {
            return Err(ErrCode::DIV0);
        }
        (sq_dev(&nums, sum / n) / n).sqrt()
    } else if f == 9.0 {
        // SUM
        sum
    } else if f == 10.0 {
        // VAR (sample)
        if len < 2 {
            return Err(ErrCode::DIV0);
        }
        sq_dev(&nums, sum / n) / (n - 1.0)
    } else if f == 11.0 {
        // VARP (population)
        if len < 1 {
            return Err(ErrCode::DIV0);
        }
        sq_dev(&nums, sum / n) / n
    } else {
        return Err(ErrCode::VALUE);
    };
    ok(r)
}

// ── Random ──────────────────────────────────────────────────────────────────

fn rand(ev: &Evaluator, _a: &[Node]) -> R {
    ok(ev.random())
}

fn randbetween(ev: &Evaluator, a: &[Node]) -> R {
    let lo = ev.num(arg(a, 0)?)?;
    let hi = ev.num(arg(a, 1)?)?;
    let l = lo.ceil();
    let h = hi.floor();
    if l > h {
        return Err(ErrCode::NUM);
    }
    ok(l + (ev.random() * (h - l + 1.0)).floor())
}

/// `Math.trunc(numOpt(...) as number)`: an error is read as NaN (QUIRK: the web does not check it).
fn dim_or(ev: &Evaluator, a: &[Node], i: usize) -> f64 {
    match num_or(ev, a, i, 1.0) {
        Ok(v) => v.trunc(),
        Err(_) => f64::NAN,
    }
}

/// Whether a `rows` × `cols` result is too big to build.
fn too_big(rows: f64, cols: f64) -> bool {
    rows > MAX_CELLS || cols > MAX_CELLS || rows * cols > MAX_CELLS
}

fn randarray(ev: &Evaluator, a: &[Node]) -> R {
    let rows = dim_or(ev, a, 0);
    let cols = dim_or(ev, a, 1);
    let min = num_or(ev, a, 2, 0.0)?;
    let max = num_or(ev, a, 3, 1.0)?;
    let integer = match a.get(4) {
        Some(n) => to_bool(&ev.scalar(n)),
        None => false,
    };
    // QUIRK: a NaN dimension (an error argument) passes the `< 1` test and yields an empty matrix / rows.
    if rows < 1.0 || cols < 1.0 {
        return Err(ErrCode::VALUE);
    }
    if min > max {
        return Err(ErrCode::VALUE);
    }
    if too_big(rows, cols) {
        return Err(ErrCode::VALUE);
    }
    let mut out: Matrix = Vec::new();
    let mut r = 0.0;
    while r < rows {
        let mut row: Vec<Scalar> = Vec::new();
        let mut c = 0.0;
        while c < cols {
            let v = min + ev.random() * (max - min);
            row.push(Scalar::Num(if integer { (min + ev.random() * (max - min + 1.0)).floor() } else { v }));
            c += 1.0;
        }
        out.push(row);
        r += 1.0;
    }
    Ok(Value::M(out))
}

// ── SEQUENCE ────────────────────────────────────────────────────────────────

fn sequence(ev: &Evaluator, a: &[Node]) -> R {
    let rows = dim_or(ev, a, 0);
    let cols = dim_or(ev, a, 1);
    let start = num_or(ev, a, 2, 1.0)?;
    let step = num_or(ev, a, 3, 1.0)?;
    if rows < 1.0 || cols < 1.0 {
        return Err(ErrCode::VALUE);
    }
    if too_big(rows, cols) {
        return Err(ErrCode::VALUE);
    }
    let mut out: Matrix = Vec::new();
    let mut v = start;
    let mut r = 0.0;
    while r < rows {
        let mut row: Vec<Scalar> = Vec::new();
        let mut c = 0.0;
        while c < cols {
            row.push(Scalar::Num(v));
            v += step;
            c += 1.0;
        }
        out.push(row);
        r += 1.0;
    }
    Ok(Value::M(out))
}

// ── Matrix algebra ──────────────────────────────────────────────────────────

fn munit(ev: &Evaluator, a: &[Node]) -> R {
    let n = ev.num(arg(a, 0)?)?;
    let dim = n.trunc();
    if dim < 1.0 {
        return Err(ErrCode::VALUE);
    }
    if too_big(dim, dim) {
        return Err(ErrCode::VALUE);
    }
    let mut out: Matrix = Vec::new();
    let mut r = 0.0;
    while r < dim {
        let mut row: Vec<Scalar> = Vec::new();
        let mut c = 0.0;
        while c < dim {
            row.push(Scalar::Num(if r == c { 1.0 } else { 0.0 }));
            c += 1.0;
        }
        out.push(row);
        r += 1.0;
    }
    Ok(Value::M(out))
}

fn mmult(ev: &Evaluator, a: &[Node]) -> R {
    let ma = to_num_matrix(ev.eval(arg(a, 0)?))?;
    let mb = to_num_matrix(ev.eval(arg(a, 1)?))?;
    let ar = ma.len();
    let ac = ma.first().map_or(0, Vec::len);
    let br = mb.len();
    let bc = mb.first().map_or(0, Vec::len);
    if ac != br || ar == 0 || bc == 0 {
        return Err(ErrCode::VALUE);
    }
    let mut out: Matrix = Vec::with_capacity(ar);
    for i in 0..ar {
        let mut row = Vec::with_capacity(bc);
        for j in 0..bc {
            let mut s = 0.0;
            for k in 0..ac {
                s += at(&ma, i, k) * at(&mb, k, j);
            }
            row.push(Scalar::Num(s));
        }
        out.push(row);
    }
    Ok(Value::M(out))
}

fn square(m: &[Vec<f64>]) -> bool {
    let n = m.len();
    n != 0 && m.iter().all(|r| r.len() == n)
}

fn mdeterm(ev: &Evaluator, a: &[Node]) -> R {
    let m = to_num_matrix(ev.eval(arg(a, 0)?))?;
    if !square(&m) {
        return Err(ErrCode::VALUE);
    }
    ok(determinant(m))
}

fn minverse(ev: &Evaluator, a: &[Node]) -> R {
    let m = to_num_matrix(ev.eval(arg(a, 0)?))?;
    if !square(&m) {
        return Err(ErrCode::VALUE);
    }
    match invert(&m) {
        None => Err(ErrCode::NUM),
        Some(inv) => Ok(Value::M(inv.into_iter().map(|r| r.into_iter().map(Scalar::Num).collect()).collect())),
    }
}

// ── Linear-algebra helpers (Gaussian elimination) ───────────────────────────

/// Determinant via LU-style elimination with partial pivoting (`m` is square).
fn determinant(mut m: Vec<Vec<f64>>) -> f64 {
    let n = m.len();
    let mut det = 1.0;
    for i in 0..n {
        let mut pivot = i;
        for r in i + 1..n {
            if m[r][i].abs() > m[pivot][i].abs() {
                pivot = r;
            }
        }
        if m[pivot][i] == 0.0 {
            return 0.0;
        }
        if pivot != i {
            m.swap(i, pivot);
            det = -det;
        }
        det *= m[i][i];
        for r in i + 1..n {
            let f = m[r][i] / m[i][i];
            for c in i..n {
                let sub = f * m[i][c];
                m[r][c] -= sub;
            }
        }
    }
    det
}

/// Matrix inverse via Gauss-Jordan elimination; `None` if singular (`m` is square).
fn invert(m: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let n = m.len();
    // Build [m | I].
    let mut aug: Vec<Vec<f64>> = m
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let mut r = row.clone();
            r.extend((0..n).map(|j| if i == j { 1.0 } else { 0.0 }));
            r
        })
        .collect();
    for i in 0..n {
        let mut pivot = i;
        for r in i + 1..n {
            if aug[r][i].abs() > aug[pivot][i].abs() {
                pivot = r;
            }
        }
        if aug[pivot][i].abs() < 1e-12 {
            return None;
        }
        if pivot != i {
            aug.swap(i, pivot);
        }
        let pv = aug[i][i];
        for c in 0..2 * n {
            aug[i][c] /= pv;
        }
        for r in 0..n {
            if r == i {
                continue;
            }
            let f = aug[r][i];
            for c in 0..2 * n {
                let sub = f * aug[i][c];
                aug[r][c] -= sub;
            }
        }
    }
    Some(aug.into_iter().map(|row| row[n..].to_vec()).collect())
}

pub fn register(r: &mut Registry) {
    use crate::formula::eval::Fn as F;
    let list: &[(&'static str, F)] = &[
        // Trigonometry
        ("SIN", sin),
        ("COS", cos),
        ("TAN", tan),
        ("ASIN", asin),
        ("ACOS", acos),
        ("ATAN", atan),
        ("ATAN2", atan2),
        ("COT", cot),
        ("SEC", sec),
        ("CSC", csc),
        ("ACOT", acot),
        // Hyperbolic
        ("SINH", sinh),
        ("COSH", cosh),
        ("TANH", tanh),
        ("ASINH", asinh),
        ("ACOSH", acosh),
        ("ATANH", atanh),
        ("COTH", coth),
        ("SECH", sech),
        ("CSCH", csch),
        ("ACOTH", acoth),
        // Angles
        ("DEGREES", degrees),
        ("RADIANS", radians),
        // Logs
        ("LOG10", log10),
        // Rounding family
        ("EVEN", even),
        ("ODD", odd),
        ("MROUND", mround),
        ("TRUNC", trunc),
        ("CEILING.MATH", ceiling_math),
        ("CEILING.PRECISE", ceiling_precise),
        ("ISO.CEILING", ceiling_precise),
        ("FLOOR.MATH", floor_math),
        ("FLOOR.PRECISE", floor_precise),
        // Sign / quotient
        ("SIGN", sign),
        ("QUOTIENT", quotient),
        // Combinatorics / factorials
        ("FACT", fact),
        ("FACTDOUBLE", factdouble),
        ("COMBIN", combin),
        ("COMBINA", combina),
        ("MULTINOMIAL", multinomial),
        // GCD / LCM
        ("GCD", gcd),
        ("LCM", lcm),
        // Roman / Arabic / base conversion
        ("ROMAN", roman),
        ("ARABIC", arabic),
        ("BASE", base),
        ("DECIMAL", decimal),
        // Misc scalar math
        ("SQRTPI", sqrtpi),
        // Aggregates over multiple args
        ("SUMSQ", sumsq),
        ("SUMPRODUCT", sumproduct),
        ("SUMX2MY2", sumx2my2),
        ("SUMX2PY2", sumx2py2),
        ("SUMXMY2", sumxmy2),
        ("SERIESSUM", seriessum),
        // SUBTOTAL
        ("SUBTOTAL", subtotal),
        // Random
        ("RAND", rand),
        ("RANDBETWEEN", randbetween),
        ("RANDARRAY", randarray),
        // SEQUENCE
        ("SEQUENCE", sequence),
        // Matrix algebra
        ("MUNIT", munit),
        ("MMULT", mmult),
        ("MDETERM", mdeterm),
        ("MINVERSE", minverse),
    ];
    for (name, f) in list {
        r.insert(name, *f);
    }
}

#[cfg(test)]
mod tests {
    use crate::formula::testing::eval;
    use crate::formula::value::{flatten, Scalar, Value};

    fn num(f: &str) -> f64 {
        match eval(f) {
            Value::S(Scalar::Num(n)) => n,
            other => panic!("{f}: {other:?}"),
        }
    }

    #[test]
    fn random_functions_stay_in_range() {
        for _ in 0..20 {
            let r = num("=RAND()");
            assert!((0.0..1.0).contains(&r));
            let b = num("=RANDBETWEEN(3,7)");
            assert!((3.0..=7.0).contains(&b) && b.fract() == 0.0);
        }
        let Value::M(m) = eval("=RANDARRAY(2,3,5,6,TRUE)") else { panic!("matrix") };
        assert_eq!(m.len(), 2);
        assert!(m.iter().all(|r| r.len() == 3));
        for s in flatten(&Value::M(m)) {
            let Scalar::Num(n) = s else { panic!("number") };
            assert!((5.0..=6.0).contains(&n) && n.fract() == 0.0);
        }
        let Value::M(m) = eval("=RANDARRAY(1,2)") else { panic!("matrix") };
        assert!(flatten(&Value::M(m)).iter().all(|s| matches!(s, Scalar::Num(n) if (0.0..1.0).contains(n))));
    }

    #[test]
    fn base_and_arabic() {
        assert_eq!(eval("=BASE(255,16,4)"), Value::str("00FF"));
        assert_eq!(eval("=ARABIC(\"mcmxc\")"), Value::num(1990.0));
        assert_eq!(eval("=ROMAN(1994)"), Value::str("MCMXCIV"));
    }

    #[test]
    fn subtotal_codes() {
        assert_eq!(num("=SUBTOTAL(109,1,2,3)"), 6.0);
        assert_eq!(num("=SUBTOTAL(1,1,2,3)"), 2.0);
    }
}
