//! Port of `office/web/src/mathMatrix.ts` — numeric linear algebra for the Maths module. Reads a
//! LaTeX matrix (`\begin{…matrix}…\end`, each cell a constant expression evaluated with
//! [`super::eval`]) and computes determinant / transpose / inverse / rank / trace / power /
//! characteristic polynomial / eigenvalues / RREF / linear-system solution, rendering back to LaTeX.

// Gaussian elimination reads and writes `a[r][c]` by index across rows being mutated; iterator form
// would fight the borrow checker and obscure the port, so index loops are kept deliberately.
#![allow(clippy::needless_range_loop)]

use super::eval::eval_latex;
use super::num_fmt::{js_round, to_precision_str};

/// A numeric matrix read from LaTeX.
#[derive(Clone, Debug, PartialEq)]
pub struct Matrix {
    pub m: Vec<Vec<f64>>,
    pub env: String,
}

/// `{ latex, error }` of `mathMatrix.ts`.
#[derive(Clone, Debug, PartialEq)]
pub struct MatResult {
    pub latex: String,
    pub error: Option<String>,
}
impl MatResult {
    fn ok(latex: String) -> Self {
        MatResult { latex, error: None }
    }
    fn err(msg: &str) -> Self {
        MatResult { latex: String::new(), error: Some(msg.to_string()) }
    }
}

const NOT_MAT: &str = "Sélectionnez un bloc contenant une matrice numérique.";
const NOT_SQUARE: &str = "Opération réservée aux matrices carrées.";

fn gcd(mut a: f64, mut b: f64) -> f64 {
    a = a.abs();
    b = b.abs();
    while b != 0.0 {
        let t = b;
        b = a % b;
        a = t;
    }
    if a == 0.0 {
        1.0
    } else {
        a
    }
}

/// A value as a clean integer, a small reduced fraction, or a trimmed decimal (`mathMatrix.ts`
/// `fracLatex`).
fn frac_latex(v: f64) -> String {
    if !v.is_finite() {
        return "\\text{—}".to_string();
    }
    let r = js_round(v);
    if (v - r).abs() < 1e-9 {
        return super::num_fmt::js_number_to_string(r);
    }
    for d in 2..=1000 {
        let p = v * d as f64;
        if (p - js_round(p)).abs() < 1e-9 {
            let pr = js_round(p);
            let g = gcd(pr, d as f64);
            let n = pr / g;
            let den = d as f64 / g;
            return if n < 0.0 {
                format!("-\\frac{{{}}}{{{}}}", super::num_fmt::js_number_to_string(-n), super::num_fmt::js_number_to_string(den))
            } else {
                format!("\\frac{{{}}}{{{}}}", super::num_fmt::js_number_to_string(n), super::num_fmt::js_number_to_string(den))
            };
        }
    }
    to_precision_str(v, 6)
}

/// Parse the first `\begin{…matrix}…\end{…matrix}` found in `latex`; `None` if not a numeric,
/// rectangular matrix (`mathMatrix.ts` `parseMatrix`).
pub fn parse_matrix(latex: &str) -> Option<Matrix> {
    let (env, body) = find_matrix(latex)?;
    let rows: Vec<&str> = body.split("\\\\").map(str::trim).filter(|s| !s.is_empty()).collect();
    if rows.is_empty() {
        return None;
    }
    let mut grid: Vec<Vec<f64>> = Vec::new();
    for r in &rows {
        let mut row = Vec::new();
        for c in r.split('&') {
            row.push(eval_latex(c.trim())?);
        }
        grid.push(row);
    }
    let cols = grid[0].len();
    if grid.iter().any(|r| r.len() != cols) {
        return None;
    }
    Some(Matrix { m: grid, env })
}

/// Find the first `\begin{XXXmatrix}BODY\end{XXXmatrix}` (the regex `mathMatrix.ts` uses).
fn find_matrix(latex: &str) -> Option<(String, String)> {
    let mut search = 0usize;
    while let Some(rel) = latex[search..].find("\\begin{") {
        let begin = search + rel;
        let env_start = begin + "\\begin{".len();
        let env_end = latex[env_start..].find('}')? + env_start;
        let env = &latex[env_start..env_end];
        if env.ends_with("matrix") && env.chars().all(|c| c.is_ascii_alphabetic()) {
            let body_start = env_end + 1;
            let close = format!("\\end{{{env}}}");
            if let Some(crel) = latex[body_start..].find(&close) {
                let body = &latex[body_start..body_start + crel];
                return Some((env.to_string(), body.to_string()));
            }
        }
        search = env_start;
    }
    None
}

/// Render a numeric matrix to LaTeX (`mathMatrix.ts` `matToLatex`).
pub fn mat_to_latex(m: &[Vec<f64>], env: &str) -> String {
    let body = m
        .iter()
        .map(|r| r.iter().map(|v| frac_latex(*v)).collect::<Vec<_>>().join(" & "))
        .collect::<Vec<_>>()
        .join(" \\\\ ");
    format!("\\begin{{{env}}}{body}\\end{{{env}}}")
}

fn is_square(m: &[Vec<f64>]) -> bool {
    !m.is_empty() && m.iter().all(|r| r.len() == m.len())
}

pub fn determinant(m: &[Vec<f64>]) -> Option<f64> {
    if !is_square(m) {
        return None;
    }
    let n = m.len();
    let mut a: Vec<Vec<f64>> = m.to_vec();
    let mut d = 1.0;
    for i in 0..n {
        let mut p = i;
        for r in i + 1..n {
            if a[r][i].abs() > a[p][i].abs() {
                p = r;
            }
        }
        if a[p][i].abs() < 1e-12 {
            return Some(0.0);
        }
        if p != i {
            a.swap(i, p);
            d = -d;
        }
        d *= a[i][i];
        for r in i + 1..n {
            let f = a[r][i] / a[i][i];
            for c in i..n {
                a[r][c] -= f * a[i][c];
            }
        }
    }
    Some(d)
}

pub fn transpose(m: &[Vec<f64>]) -> Vec<Vec<f64>> {
    (0..m[0].len()).map(|c| m.iter().map(|r| r[c]).collect()).collect()
}

pub fn inverse(m: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    if !is_square(m) {
        return None;
    }
    let n = m.len();
    let mut a: Vec<Vec<f64>> = m
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let mut row = r.clone();
            row.extend((0..n).map(|j| if i == j { 1.0 } else { 0.0 }));
            row
        })
        .collect();
    for i in 0..n {
        let mut p = i;
        for r in i + 1..n {
            if a[r][i].abs() > a[p][i].abs() {
                p = r;
            }
        }
        if a[p][i].abs() < 1e-12 {
            return None;
        }
        if p != i {
            a.swap(i, p);
        }
        let piv = a[i][i];
        for c in 0..2 * n {
            a[i][c] /= piv;
        }
        for r in 0..n {
            if r != i {
                let f = a[r][i];
                for c in 0..2 * n {
                    a[r][c] -= f * a[i][c];
                }
            }
        }
    }
    Some(a.iter().map(|r| r[n..].to_vec()).collect())
}

pub fn rank(m: &[Vec<f64>]) -> usize {
    rref_full(m).1
}

pub fn trace(m: &[Vec<f64>]) -> Option<f64> {
    if !is_square(m) {
        return None;
    }
    Some((0..m.len()).map(|i| m[i][i]).sum())
}

fn mat_mul(a: &[Vec<f64>], b: &[Vec<f64>]) -> Vec<Vec<f64>> {
    a.iter()
        .map(|row| (0..b[0].len()).map(|j| row.iter().enumerate().map(|(k, v)| v * b[k][j]).sum()).collect())
        .collect()
}

fn identity(n: usize) -> Vec<Vec<f64>> {
    (0..n).map(|i| (0..n).map(|j| if i == j { 1.0 } else { 0.0 }).collect()).collect()
}

pub fn mat_pow(m: &[Vec<f64>], mut p: i64) -> Option<Vec<Vec<f64>>> {
    if !is_square(m) {
        return None;
    }
    let n = m.len();
    let mut base = m.to_vec();
    if p < 0 {
        base = inverse(m)?;
        p = -p;
    }
    let mut r = identity(n);
    for _ in 0..p {
        r = mat_mul(&r, &base);
    }
    Some(r)
}

/// Reduced row-echelon form + rank (`mathMatrix.ts` `rrefFull`).
fn rref_full(m: &[Vec<f64>]) -> (Vec<Vec<f64>>, usize) {
    let mut a: Vec<Vec<f64>> = m.to_vec();
    let rows = a.len();
    let cols = a[0].len();
    let mut lead = 0usize;
    let mut rk = 0usize;
    let mut r = 0usize;
    while r < rows && lead < cols {
        let mut i = r;
        while a[i][lead].abs() < 1e-12 {
            i += 1;
            if i == rows {
                i = r;
                lead += 1;
                if lead == cols {
                    return (a, rk);
                }
            }
        }
        a.swap(i, r);
        let piv = a[r][lead];
        for c in 0..cols {
            a[r][c] /= piv;
        }
        for j in 0..rows {
            if j != r {
                let f = a[j][lead];
                for c in 0..cols {
                    a[j][c] -= f * a[r][c];
                }
            }
        }
        rk += 1;
        lead += 1;
        r += 1;
    }
    (a, rk)
}

/// Characteristic-polynomial coefficients `[1, c₁, …, cₙ]` via Faddeev–LeVerrier.
fn char_poly_coeffs(a: &[Vec<f64>]) -> Vec<f64> {
    let n = a.len();
    let mut c = vec![1.0];
    let mut m = identity(n);
    c.push(-trace(&mat_mul(a, &m)).unwrap());
    for k in 2..=n {
        let am = mat_mul(a, &m);
        let ck1 = c[k - 1];
        m = am.iter().enumerate().map(|(i, row)| row.iter().enumerate().map(|(j, v)| v + if i == j { ck1 } else { 0.0 }).collect()).collect();
        c.push(-trace(&mat_mul(a, &m)).unwrap() / k as f64);
    }
    c
}

fn poly_var_latex(c: &[f64], v: &str) -> String {
    let deg = c.len() as i64 - 1;
    let mut parts: Vec<String> = Vec::new();
    for (i, &co) in c.iter().enumerate() {
        let p = deg - i as i64;
        if co.abs() < 1e-9 && i > 0 {
            continue;
        }
        let mag = co.abs();
        let coef = if p > 0 && (mag - 1.0).abs() < 1e-9 { String::new() } else { frac_latex(mag) };
        let xp = if p == 0 {
            String::new()
        } else if p == 1 {
            v.to_string()
        } else {
            format!("{v}^{{{p}}}")
        };
        let sign = if co < 0.0 { "-" } else if !parts.is_empty() { "+" } else { "" };
        let body = format!("{coef}{xp}");
        let body = if body.is_empty() { "0".to_string() } else { body };
        parts.push(format!("{sign}{body}"));
    }
    parts.join(" ")
}

// ── LaTeX entry points ─────────────────────────────────────────────────────────

pub fn determinant_latex(latex: &str) -> MatResult {
    let Some(mat) = parse_matrix(latex) else {
        return MatResult::err(NOT_MAT);
    };
    match determinant(&mat.m) {
        None => MatResult::err(NOT_SQUARE),
        Some(d) => MatResult::ok(format!("{} = {}", mat_to_latex(&mat.m, "vmatrix"), frac_latex(d))),
    }
}
pub fn transpose_latex(latex: &str) -> MatResult {
    let Some(mat) = parse_matrix(latex) else {
        return MatResult::err(NOT_MAT);
    };
    MatResult::ok(format!("{}^{{T}} = {}", mat_to_latex(&mat.m, "pmatrix"), mat_to_latex(&transpose(&mat.m), "pmatrix")))
}
pub fn inverse_latex(latex: &str) -> MatResult {
    let Some(mat) = parse_matrix(latex) else {
        return MatResult::err(NOT_MAT);
    };
    if !is_square(&mat.m) {
        return MatResult::err(NOT_SQUARE);
    }
    match inverse(&mat.m) {
        None => MatResult::err("Matrice non inversible (déterminant nul)."),
        Some(inv) => MatResult::ok(format!("{}^{{-1}} = {}", mat_to_latex(&mat.m, "pmatrix"), mat_to_latex(&inv, "pmatrix"))),
    }
}
pub fn rank_latex(latex: &str) -> MatResult {
    let Some(mat) = parse_matrix(latex) else {
        return MatResult::err(NOT_MAT);
    };
    MatResult::ok(format!("\\operatorname{{rg}}\\,{} = {}", mat_to_latex(&mat.m, "pmatrix"), rank(&mat.m)))
}
pub fn trace_latex(latex: &str) -> MatResult {
    let Some(mat) = parse_matrix(latex) else {
        return MatResult::err(NOT_MAT);
    };
    match trace(&mat.m) {
        None => MatResult::err(NOT_SQUARE),
        Some(tr) => MatResult::ok(format!("\\operatorname{{tr}}\\,{} = {}", mat_to_latex(&mat.m, "pmatrix"), frac_latex(tr))),
    }
}
pub fn power_latex(latex: &str, p: i64) -> MatResult {
    let Some(mat) = parse_matrix(latex) else {
        return MatResult::err(NOT_MAT);
    };
    if !is_square(&mat.m) {
        return MatResult::err(NOT_SQUARE);
    }
    match mat_pow(&mat.m, p) {
        None => MatResult::err("Matrice non inversible (puissance négative)."),
        Some(r) => MatResult::ok(format!("{}^{{{p}}} = {}", mat_to_latex(&mat.m, "pmatrix"), mat_to_latex(&r, "pmatrix"))),
    }
}
pub fn rref_latex(latex: &str) -> MatResult {
    let Some(mat) = parse_matrix(latex) else {
        return MatResult::err(NOT_MAT);
    };
    MatResult::ok(format!("{} \\;\\sim\\; {}", mat_to_latex(&mat.m, "pmatrix"), mat_to_latex(&rref_full(&mat.m).0, "pmatrix")))
}
pub fn char_poly_latex(latex: &str) -> MatResult {
    let Some(mat) = parse_matrix(latex) else {
        return MatResult::err(NOT_MAT);
    };
    if !is_square(&mat.m) {
        return MatResult::err(NOT_SQUARE);
    }
    MatResult::ok(format!("\\chi(\\lambda) = {}", poly_var_latex(&char_poly_coeffs(&mat.m), "\\lambda")))
}
pub fn eigenvalues_latex(latex: &str) -> MatResult {
    let Some(mat) = parse_matrix(latex) else {
        return MatResult::err(NOT_MAT);
    };
    if !is_square(&mat.m) {
        return MatResult::err(NOT_SQUARE);
    }
    let c = char_poly_coeffs(&mat.m);
    let n = c.len() - 1;
    let poly = |x: f64| c.iter().fold(0.0, |s, &k| s * x + k); // Horner
    let r_bound = 1.0 + c[1..].iter().fold(0.0_f64, |m, &v| m.max(v.abs()));
    let mut roots: Vec<f64> = Vec::new();
    let push = |roots: &mut Vec<f64>, r: f64| {
        if !roots.iter().any(|&x| (x - r).abs() < 1e-6) {
            roots.push(r);
        }
    };
    let steps = 4000;
    let dx = (2.0 * r_bound) / steps as f64;
    let mut px = -r_bound;
    let mut pv = poly(-r_bound);
    for i in 1..=steps {
        let x = -r_bound + i as f64 * dx;
        let v = poly(x);
        if pv * v < 0.0 {
            let (mut lo, mut hi, mut fl) = (px, x, pv);
            for _ in 0..60 {
                let mid = (lo + hi) / 2.0;
                let fm = poly(mid);
                if fl * fm < 0.0 {
                    hi = mid;
                } else {
                    lo = mid;
                    fl = fm;
                }
            }
            push(&mut roots, (lo + hi) / 2.0);
        } else if v.abs() < 1e-9 {
            push(&mut roots, x);
        }
        px = x;
        pv = v;
    }
    roots.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let list = roots
        .iter()
        .enumerate()
        .map(|(i, &r)| {
            let val = if (r - js_round(r)).abs() < 1e-6 { js_round(r) } else { to_precision_str_num(r, 6) };
            format!("\\lambda_{{{}}} \\approx {}", i + 1, frac_latex(val))
        })
        .collect::<Vec<_>>()
        .join(",\\quad ");
    let suffix = if roots.len() < n { "\\quad (\\text{+ valeurs propres complexes})" } else { "" };
    MatResult::ok(if roots.is_empty() { "\\text{Aucune valeur propre réelle}".to_string() } else { format!("{list}{suffix}") })
}

/// `Number(x.toPrecision(6))` as an f64 (the web rounds then passes the number to `fracLatex`).
fn to_precision_str_num(v: f64, p: usize) -> f64 {
    format!("{:.*e}", p - 1, v).parse().unwrap_or(v)
}

pub fn solve_system_latex(latex: &str) -> MatResult {
    let Some(mat) = parse_matrix(latex) else {
        return MatResult::err(NOT_MAT);
    };
    let cols = mat.m[0].len();
    let unk = cols as i64 - 1;
    if unk < 1 {
        return MatResult::err("Fournissez une matrice augmentée [A | b] (colonnes = inconnues + 1).");
    }
    let unk = unk as usize;
    let (r, _) = rref_full(&mat.m);
    let rank_a = rref_full(&mat.m.iter().map(|row| row[..unk].to_vec()).collect::<Vec<_>>()).1;
    let rank_aug = r.iter().filter(|row| row.iter().any(|v| v.abs() > 1e-9)).count();
    if rank_aug > rank_a {
        return MatResult::ok("\\text{Système incompatible : aucune solution.}".to_string());
    }
    if rank_a < unk {
        return MatResult::ok("\\text{Système à une infinité de solutions.}".to_string());
    }
    let mut sol: Vec<String> = Vec::new();
    for j in 0..unk {
        let row = r.iter().find(|row| (row[j] - 1.0).abs() < 1e-9 && row.iter().enumerate().all(|(c, v)| c == j || c >= unk || v.abs() < 1e-9));
        let val = row.map(|row| row[cols - 1]).unwrap_or(0.0);
        sol.push(format!("x_{{{}}} = {}", j + 1, frac_latex(val)));
    }
    MatResult::ok(format!("\\begin{{cases}} {} \\end{{cases}}", sol.join(" \\\\ ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn determinant_2x2() {
        let r = determinant_latex("\\begin{pmatrix}1&2\\\\3&4\\end{pmatrix}");
        assert_eq!(r.error, None);
        assert!(r.latex.ends_with("= -2"));
    }

    #[test]
    fn inverse_and_errors() {
        let r = inverse_latex("\\begin{pmatrix}1&2\\\\3&4\\end{pmatrix}");
        assert_eq!(r.error, None);
        let r = inverse_latex("x+1");
        assert_eq!(r.error.as_deref(), Some(NOT_MAT));
        let r = determinant_latex("\\begin{pmatrix}1&2&3\\\\4&5&6\\end{pmatrix}");
        assert_eq!(r.error.as_deref(), Some(NOT_SQUARE));
    }

    #[test]
    fn rank_and_trace() {
        assert!(rank_latex("\\begin{pmatrix}1&2\\\\2&4\\end{pmatrix}").latex.ends_with("= 1"));
        assert!(trace_latex("\\begin{pmatrix}1&2\\\\3&4\\end{pmatrix}").latex.ends_with("= 5"));
    }
}
