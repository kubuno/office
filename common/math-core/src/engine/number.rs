//! Port of `office/web/src/mathNumber.ts` - integer / number-theory helpers (prime factorisation,
//! gcd/lcm, factorial, binomial + arrangements, base conversion). Pure numeric. Each public
//! `*_latex` returns a [`NumResult`] whose strings match the web exactly.

use super::num_fmt::js_number_to_string;

/// `{ latex, error }` of `mathNumber.ts` (`error` is a user-facing French message).
#[derive(Clone, Debug, PartialEq)]
pub struct NumResult {
    pub latex: String,
    pub error: Option<String>,
}
impl NumResult {
    fn ok(latex: String) -> Self {
        NumResult { latex, error: None }
    }
    fn err(msg: &str) -> Self {
        NumResult { latex: String::new(), error: Some(msg.to_string()) }
    }
}

fn is_int(v: f64) -> bool {
    v.is_finite() && v.fract() == 0.0
}

fn gcd(mut a: f64, mut b: f64) -> f64 {
    a = a.abs();
    b = b.abs();
    while b != 0.0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

fn factorial(n: f64) -> f64 {
    let mut r = 1.0;
    let mut i = 2.0;
    while i <= n {
        r *= i;
        i += 1.0;
    }
    r
}

/// Prime factorisation of a positive integer → `360 = 2^{3} \times 3^{2} \times 5`.
pub fn prime_factors_latex(n: f64) -> NumResult {
    if !is_int(n) || n.abs() < 2.0 {
        return NumResult::err("Entrez un entier ≥ 2.");
    }
    let mut m = n.abs();
    let mut factors: Vec<(f64, u32)> = Vec::new();
    let mut p = 2.0;
    while p * p <= m {
        if m % p == 0.0 {
            let mut e = 0u32;
            while m % p == 0.0 {
                m /= p;
                e += 1;
            }
            factors.push((p, e));
        }
        p += 1.0;
    }
    if m > 1.0 {
        factors.push((m, 1));
    }
    let body = factors
        .iter()
        .map(|(p, e)| if *e == 1 { js_number_to_string(*p) } else { format!("{}^{{{}}}", js_number_to_string(*p), e) })
        .collect::<Vec<_>>()
        .join(" \\times ");
    let sign = if n < 0.0 { "-" } else { "" };
    NumResult::ok(format!("{} = {sign}{body}", js_number_to_string(n)))
}

/// gcd and lcm of two integers.
pub fn gcd_lcm_latex(a: f64, b: f64) -> NumResult {
    if !is_int(a) || !is_int(b) {
        return NumResult::err("Entrez deux entiers.");
    }
    let g = gcd(a, b);
    let l = if g == 0.0 { 0.0 } else { (a / g * b).abs() };
    NumResult::ok(format!(
        "\\gcd({},\\, {}) = {} \\qquad \\operatorname{{ppcm}}({},\\, {}) = {}",
        js_number_to_string(a),
        js_number_to_string(b),
        js_number_to_string(g),
        js_number_to_string(a),
        js_number_to_string(b),
        js_number_to_string(l)
    ))
}

/// `n! = …`.
pub fn factorial_latex(n: f64) -> NumResult {
    if !is_int(n) || n < 0.0 {
        return NumResult::err("Entrez un entier ≥ 0.");
    }
    if n > 170.0 {
        return NumResult::err("Trop grand (n ≤ 170).");
    }
    NumResult::ok(format!("{}! = {}", js_number_to_string(n), js_number_to_string(factorial(n))))
}

/// Binomial coefficient and arrangements: `\binom{n}{k}` and `A_{n}^{k}`.
pub fn binomial_latex(n: f64, k: f64) -> NumResult {
    if !is_int(n) || !is_int(k) || n < 0.0 || k < 0.0 || k > n {
        return NumResult::err("Entrez des entiers 0 ≤ k ≤ n.");
    }
    let mut c = 1.0;
    let mut i = 0.0;
    while i < k {
        c = (c * (n - i)) / (i + 1.0);
        i += 1.0;
    }
    let c_val = c.round(); // Math.round, but c is a positive near-integer here
    let a_val = (c * factorial(k)).round();
    NumResult::ok(format!(
        "\\binom{{{}}}{{{}}} = {} \\qquad A_{{{}}}^{{{}}} = {}",
        js_number_to_string(n),
        js_number_to_string(k),
        js_number_to_string(c_val),
        js_number_to_string(n),
        js_number_to_string(k),
        js_number_to_string(a_val)
    ))
}

/// Base conversion of a (possibly negative) integer to binary, octal and hexadecimal.
pub fn base_convert_latex(n: f64) -> NumResult {
    if !is_int(n) {
        return NumResult::err("Entrez un entier.");
    }
    let s = if n < 0.0 { "-" } else { "" };
    let a = n.abs() as i128;
    NumResult::ok(format!(
        "\\begin{{aligned}} {} &= ({s}{})_2 \\\\ &= ({s}{})_8 \\\\ &= ({s}\\text{{{}}})_{{16}} \\end{{aligned}}",
        js_number_to_string(n),
        format_radix(a, 2),
        format_radix(a, 8),
        format_radix(a, 16).to_uppercase()
    ))
}

/// Integer → string in a given radix (JS `Number.prototype.toString(radix)` for a non-negative int).
fn format_radix(mut v: i128, radix: u32) -> String {
    if v == 0 {
        return "0".to_string();
    }
    let digits = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = Vec::new();
    while v > 0 {
        out.push(digits[(v % radix as i128) as usize]);
        v /= radix as i128;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prime_factors() {
        assert_eq!(prime_factors_latex(360.0).latex, "360 = 2^{3} \\times 3^{2} \\times 5");
        assert_eq!(prime_factors_latex(1.0).error.as_deref(), Some("Entrez un entier ≥ 2."));
        assert_eq!(prime_factors_latex(17.0).latex, "17 = 17");
    }

    #[test]
    fn gcd_lcm() {
        assert_eq!(gcd_lcm_latex(12.0, 18.0).latex, "\\gcd(12,\\, 18) = 6 \\qquad \\operatorname{ppcm}(12,\\, 18) = 36");
    }

    #[test]
    fn factorial_and_binomial() {
        assert_eq!(factorial_latex(5.0).latex, "5! = 120");
        assert_eq!(factorial_latex(171.0).error.as_deref(), Some("Trop grand (n ≤ 170)."));
        assert_eq!(binomial_latex(5.0, 2.0).latex, "\\binom{5}{2} = 10 \\qquad A_{5}^{2} = 20");
    }

    #[test]
    fn base_convert() {
        assert_eq!(
            base_convert_latex(255.0).latex,
            "\\begin{aligned} 255 &= (11111111)_2 \\\\ &= (377)_8 \\\\ &= (\\text{FF})_{16} \\end{aligned}"
        );
    }
}
