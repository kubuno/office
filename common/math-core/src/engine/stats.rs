//! Port of `office/web/src/mathStats.ts` - descriptive statistics and linear regression. Reads a
//! list of numbers (comma/semicolon/`&`/`\\`-separated, matrix wrappers stripped) or a list of
//! `(x, y)` points, with cells evaluated by [`super::eval`], and summarises them.

use super::eval::{eval_latex, fmt_result};

/// `{ latex, error }` of `mathStats.ts`.
#[derive(Clone, Debug, PartialEq)]
pub struct StatResult {
    pub latex: String,
    pub error: Option<String>,
}
impl StatResult {
    fn ok(latex: String) -> Self {
        StatResult { latex, error: None }
    }
    fn err(msg: &str) -> Self {
        StatResult { latex: String::new(), error: Some(msg.to_string()) }
    }
}

/// `fmtResult(Number(v.toPrecision(8)))` (`mathStats.ts` `fmt`).
fn fmt(v: f64) -> String {
    if !v.is_finite() {
        return "\\text{-}".to_string();
    }
    let rounded: f64 = format!("{:.*e}", 7, v).parse().unwrap_or(v); // toPrecision(8)
    fmt_result(rounded)
}

/// Parse a flat list of numbers from LaTeX (`mathStats.ts` `parseList`).
pub fn parse_list(latex: &str) -> Option<Vec<f64>> {
    let s = latex.trim();
    if s.is_empty() {
        return None;
    }
    let s = strip_wrappers_simple(s);
    // split on , ; & or \\
    let s = s.replace("\\\\", ";");
    let parts: Vec<&str> = s.split([',', ';', '&']).map(str::trim).filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return None;
    }
    let mut nums = Vec::new();
    for p in parts {
        nums.push(eval_latex(p)?);
    }
    Some(nums)
}

/// A simpler, char-safe wrapper strip used by [`parse_list`] / [`parse_points`].
fn strip_wrappers_simple(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("\\begin{").or_else(|| rest.strip_prefix("\\end{")) {
            // drop up to and including the next '}'
            match after.find('}') {
                Some(pos) => rest = &after[pos + 1..],
                None => rest = "",
            }
            out.push(' ');
            continue;
        }
        if let Some(r) = rest.strip_prefix("\\left").or_else(|| rest.strip_prefix("\\right")) {
            out.push(' ');
            rest = r;
            continue;
        }
        if let Some(r) = rest.strip_prefix("\\{").or_else(|| rest.strip_prefix("\\}")) {
            out.push(' ');
            rest = r;
            continue;
        }
        let mut chars = rest.chars();
        let c = chars.next().unwrap();
        if c == '{' || c == '}' {
            out.push(' ');
        } else {
            out.push(c);
        }
        rest = chars.as_str();
    }
    out
}

/// Parse a list of `(x, y)` points (`mathStats.ts` `parsePoints`).
pub fn parse_points(latex: &str) -> Option<Vec<(f64, f64)>> {
    let s = latex.replace("\\left", "").replace("\\right", "");
    let mut pts: Vec<(f64, f64)> = Vec::new();
    let bytes: Vec<char> = s.chars().collect();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == '(' {
            // read up to the next ')' with no nested '(' - the `\(([^()]+)\)` match
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && bytes[j] != ')' && bytes[j] != '(' {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == ')' && j > start {
                let inner: String = bytes[start..j].iter().collect();
                let c: Vec<Option<f64>> = inner.split([',', ';']).map(|x| eval_latex(x.trim())).collect();
                if c.len() != 2 || c.iter().any(Option::is_none) {
                    return None;
                }
                pts.push((c[0].unwrap(), c[1].unwrap()));
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    if pts.is_empty() {
        None
    } else {
        Some(pts)
    }
}

fn quantile(sorted: &[f64], q: f64) -> f64 {
    let pos = (sorted.len() as f64 - 1.0) * q;
    let base = pos.floor() as usize;
    let rest = pos - base as f64;
    match sorted.get(base + 1) {
        Some(next) => sorted[base] + rest * (next - sorted[base]),
        None => sorted[base],
    }
}

/// Full descriptive summary: n, sum, mean, variance, standard deviation, min, max.
pub fn stats_latex(latex: &str) -> StatResult {
    let Some(xs) = parse_list(latex) else {
        return StatResult::err("Entrez une liste de nombres (ex. 1, 2, 3, 4).");
    };
    let n = xs.len();
    let sum: f64 = xs.iter().sum();
    let mean = sum / n as f64;
    let varc = xs.iter().map(|b| (b - mean).powi(2)).sum::<f64>() / n as f64;
    let min = xs.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    StatResult::ok(format!(
        "\\begin{{aligned}} n &= {n} & \\bar{{x}} &= {} \\\\ \\sum x &= {} & \\sigma &= {} \\\\ \\min &= {} & \\sigma^2 &= {} \\\\ \\max &= {} & & \\end{{aligned}}",
        fmt(mean),
        fmt(sum),
        fmt(varc.sqrt()),
        fmt(min),
        fmt(varc),
        fmt(max)
    ))
}

/// Five-number summary (min, Q1, median, Q3, max).
pub fn five_number_latex(latex: &str) -> StatResult {
    let Some(xs) = parse_list(latex) else {
        return StatResult::err("Entrez une liste de nombres.");
    };
    let mut s = xs.clone();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    StatResult::ok(format!(
        "\\min = {},\\ Q_1 = {},\\ \\tilde{{x}} = {},\\ Q_3 = {},\\ \\max = {}",
        fmt(s[0]),
        fmt(quantile(&s, 0.25)),
        fmt(quantile(&s, 0.5)),
        fmt(quantile(&s, 0.75)),
        fmt(s[s.len() - 1])
    ))
}

/// Least-squares linear regression y = a·x + b, with R².
pub fn regression_latex(latex: &str) -> StatResult {
    let Some(pts) = parse_points(latex) else {
        return StatResult::err("Entrez au moins deux points (ex. (1,2),(2,3),(3,5)).");
    };
    if pts.len() < 2 {
        return StatResult::err("Entrez au moins deux points (ex. (1,2),(2,3),(3,5)).");
    }
    let n = pts.len() as f64;
    let sx: f64 = pts.iter().map(|p| p.0).sum();
    let sy: f64 = pts.iter().map(|p| p.1).sum();
    let sxx: f64 = pts.iter().map(|p| p.0 * p.0).sum();
    let sxy: f64 = pts.iter().map(|p| p.0 * p.1).sum();
    let den = n * sxx - sx * sx;
    if den.abs() < 1e-12 {
        return StatResult::err("Régression impossible (abscisses toutes égales).");
    }
    let a = (n * sxy - sx * sy) / den;
    let b = (sy - a * sx) / n;
    let my = sy / n;
    let ss_tot: f64 = pts.iter().map(|p| (p.1 - my).powi(2)).sum();
    let ss_res: f64 = pts.iter().map(|p| (p.1 - (a * p.0 + b)).powi(2)).sum();
    let r2 = if ss_tot < 1e-12 { 1.0 } else { 1.0 - ss_res / ss_tot };
    let b_term = if b.abs() < 1e-9 {
        String::new()
    } else {
        format!(" {} {}", if b < 0.0 { "-" } else { "+" }, fmt(b.abs()))
    };
    StatResult::ok(format!("y = {}\\,x{b_term} \\qquad R^2 = {}", fmt(a), fmt(r2)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptive() {
        let r = stats_latex("1, 2, 3, 4");
        assert_eq!(r.error, None);
        assert!(r.latex.contains("n &= 4"));
        assert!(r.latex.contains("\\bar{x} &= 2.5"));
        assert_eq!(stats_latex("").error.as_deref(), Some("Entrez une liste de nombres (ex. 1, 2, 3, 4)."));
    }

    #[test]
    fn five_number() {
        let r = five_number_latex("1,2,3,4,5");
        assert_eq!(r.error, None);
        assert!(r.latex.contains("\\tilde{x} = 3"));
    }

    #[test]
    fn regression_line() {
        let r = regression_latex("(1,2),(2,3),(3,5)");
        assert_eq!(r.error, None);
        assert!(r.latex.starts_with("y = 1.5\\,x"));
    }
}
