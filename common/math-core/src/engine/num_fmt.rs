//! JavaScript-compatible number → string formatting.
//!
//! Every Maths result string the web produces is built with `Number.prototype.toString()` or
//! `Number(x.toPrecision(p)).toString()`. Rust's own `f64` `Display` gives the same shortest
//! round-trip digits but always positional (never exponential) and with different large/small
//! thresholds, so the output would diverge. These functions reproduce the ECMAScript algorithm.

/// Port of ECMAScript `Number.prototype.toString()` (base 10) for a finite `f64`.
///
/// Reproduces the `Number::toString` thresholds: an exponential form is used when the decimal
/// exponent of the first significant digit is `>= 21` or `<= -7`, otherwise a positional form.
pub fn js_number_to_string(v: f64) -> String {
    if v == 0.0 {
        return "0".to_string(); // also collapses -0.0, like JS
    }
    if v.is_nan() {
        return "NaN".to_string();
    }
    if v.is_infinite() {
        return if v < 0.0 { "-Infinity".to_string() } else { "Infinity".to_string() };
    }

    let neg = v < 0.0;
    // Rust's Display is the shortest round-trip decimal, always positional (e.g. "0.0000001",
    // "1000000000000000000000"). Parse it into significant digits + decimal-point position.
    let disp = format!("{}", v.abs());
    let (int_part, frac_part) = match disp.split_once('.') {
        Some((i, f)) => (i, f),
        None => (disp.as_str(), ""),
    };
    let point_pos = int_part.len() as i64; // digits before the decimal point (in the raw string)
    let all: String = format!("{int_part}{frac_part}");
    let lead = all.chars().take_while(|&c| c == '0').count();
    let digits: String = all.trim_start_matches('0').trim_end_matches('0').to_string();
    let digits = if digits.is_empty() { "0".to_string() } else { digits };
    let k = digits.len() as i64; // number of significant digits
    let n = point_pos - lead as i64; // ECMAScript `n`: decimal point sits after `n` significant digits

    let body = if n >= k && n <= 21 {
        // integer-ish: all significant digits then (n - k) zeros
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if n > 0 && n <= 21 {
        // decimal point inside the digits
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if n > -6 && n <= 0 {
        // 0.000… + digits
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        // exponential form: d.ddde±E, E = n - 1
        let e = n - 1;
        let mantissa = if k == 1 {
            digits.clone()
        } else {
            format!("{}.{}", &digits[..1], &digits[1..])
        };
        let sign = if e >= 0 { "+" } else { "-" };
        format!("{mantissa}e{sign}{}", e.abs())
    };

    if neg {
        format!("-{body}")
    } else {
        body
    }
}

/// Port of the web's `Number(x.toPrecision(p)).toString()`: round `v` to `p` significant digits,
/// then format it shortest, exactly as JavaScript would.
///
/// (The only possible divergence from JS is a last-digit tie - Rust formats ties to even, the
/// ECMAScript `toPrecision` spec rounds ties to the larger magnitude - which is vanishingly rare
/// for computed irrationals and is guarded by the parity fixtures.)
pub fn to_precision_str(v: f64, p: usize) -> String {
    if !v.is_finite() {
        return js_number_to_string(v);
    }
    if v == 0.0 {
        return "0".to_string();
    }
    let p = p.max(1);
    // `{:.*e}` rounds to p-1 fractional digits in the mantissa = p significant digits, then we
    // re-parse (as `Number(...)` does) and format shortest.
    let rounded: f64 = format!("{:.*e}", p - 1, v).parse().unwrap_or(v);
    js_number_to_string(rounded)
}

/// JS `Math.round` (rounds half toward +∞, unlike `f64::round` which rounds half away from zero).
pub fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// JSON string literal exactly as `JSON.stringify` writes one (escaping matches: `"`, `\`, control
/// chars as `\uXXXX`, UTF-8 kept, `/` not escaped - the same set serde_json uses).
pub fn json_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}

/// JSON number exactly as `JSON.stringify` writes one (`ToString(Number)`; non-finite → `null`).
pub fn json_number(v: f64) -> String {
    if v.is_finite() {
        js_number_to_string(v)
    } else {
        "null".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_string_matches_js() {
        assert_eq!(js_number_to_string(0.0), "0");
        assert_eq!(js_number_to_string(-0.0), "0");
        assert_eq!(js_number_to_string(100.0), "100");
        assert_eq!(js_number_to_string(123.456), "123.456");
        assert_eq!(js_number_to_string(0.5), "0.5");
        assert_eq!(js_number_to_string(-2.5), "-2.5");
        assert_eq!(js_number_to_string(1e-6), "0.000001");
        assert_eq!(js_number_to_string(1e-7), "1e-7");
        assert_eq!(js_number_to_string(1e21), "1e+21");
        assert_eq!(js_number_to_string(1e20), "100000000000000000000");
        assert_eq!(js_number_to_string(std::f64::consts::PI), "3.141592653589793");
    }

    #[test]
    fn to_precision_matches_js() {
        // Number((1/3).toPrecision(12)).toString() === "0.333333333333"
        assert_eq!(to_precision_str(1.0 / 3.0, 12), "0.333333333333");
        // Number((2/3).toPrecision(6)).toString() === "0.666667"
        assert_eq!(to_precision_str(2.0 / 3.0, 6), "0.666667");
        // Number((1234.5678).toPrecision(6)).toString() === "1234.57"
        assert_eq!(to_precision_str(1234.5678, 6), "1234.57");
        assert_eq!(to_precision_str(10.0, 12), "10");
    }
}
