//! Port of `office/web/src/mathCas.ts` - a small symbolic engine: differentiate, integrate,
//! simplify, expand, factor, solve (numeric and exact deg≤2), Taylor, limits and function study,
//! pretty-printing back to LaTeX. Reuses the AST of [`super::expr`] and the LaTeX→text conversion of
//! [`super::eval`], so it covers exactly the closed-form expressions the module understands.
//! Best-effort: unsupported input returns an `error` string.

use super::eval::{fmt_result, latex_to_expr};
use super::expr::{compile, parse_expr, Node};
use super::num_fmt::{js_number_to_string, js_round, to_precision_str};
use std::f64::consts::{E, PI};

// ── AST constructors ──────────────────────────────────────────────────────────────
fn nnum(v: f64) -> Node {
    Node::Num(v)
}
fn nneg(a: Node) -> Node {
    Node::Neg(Box::new(a))
}
fn nbin(op: char, a: Node, b: Node) -> Node {
    Node::Bin { op, a: Box::new(a), b: Box::new(b) }
}
fn ncall(name: &str, args: Vec<Node>) -> Node {
    Node::Call { name: name.to_string(), args }
}
fn x() -> Node {
    Node::Var
}

/// `{ latex, error }` of `mathCas.ts`.
#[derive(Clone, Debug, PartialEq)]
pub struct CasResult {
    pub latex: String,
    pub error: Option<String>,
}
impl CasResult {
    fn ok(latex: String) -> Self {
        CasResult { latex, error: None }
    }
    fn err(msg: &str) -> Self {
        CasResult { latex: String::new(), error: Some(msg.to_string()) }
    }
}

const NOT_EXPR: &str = "Ce bloc n'est pas une expression de x exploitable.";

fn as_num(n: &Node) -> Option<f64> {
    if let Node::Num(v) = n {
        Some(*v)
    } else {
        None
    }
}

// ── Symbolic differentiation (d/dx) ─────────────────────────────────────────────────
pub fn differentiate(n: &Node) -> Result<Node, String> {
    match n {
        Node::Num(_) => Ok(nnum(0.0)),
        Node::Var => Ok(nnum(1.0)),
        Node::Neg(a) => Ok(nneg(differentiate(a)?)),
        Node::Bin { op, a, b } => match op {
            '+' | '-' => Ok(nbin(*op, differentiate(a)?, differentiate(b)?)),
            '*' => Ok(nbin('+', nbin('*', differentiate(a)?, (**b).clone()), nbin('*', (**a).clone(), differentiate(b)?))),
            '/' => Ok(nbin(
                '/',
                nbin('-', nbin('*', differentiate(a)?, (**b).clone()), nbin('*', (**a).clone(), differentiate(b)?)),
                nbin('^', (**b).clone(), nnum(2.0)),
            )),
            '^' => {
                if let Node::Num(bv) = **b {
                    // n·u^(n−1)·u'
                    Ok(nbin('*', nbin('*', nnum(bv), nbin('^', (**a).clone(), nnum(bv - 1.0))), differentiate(a)?))
                } else if let Node::Num(_) = **a {
                    // aᵘ·ln(a)·u'
                    Ok(nbin('*', nbin('*', n.clone(), ncall("ln", vec![(**a).clone()])), differentiate(b)?))
                } else {
                    // uᵛ·(v'·ln u + v·u'/u)
                    Ok(nbin(
                        '*',
                        n.clone(),
                        nbin(
                            '+',
                            nbin('*', differentiate(b)?, ncall("ln", vec![(**a).clone()])),
                            nbin('*', (**b).clone(), nbin('/', differentiate(a)?, (**a).clone())),
                        ),
                    ))
                }
            }
            _ => Err("Expression non dérivable".to_string()),
        },
        Node::Call { name, args } => {
            if args.len() != 1 {
                return Err(format!("Dérivée de « {name} » non prise en charge"));
            }
            let u = &args[0];
            let du = differentiate(u)?;
            let chain = |d: Node| nbin('*', d, du.clone());
            let r = match name.as_str() {
                "sin" => chain(ncall("cos", vec![u.clone()])),
                "cos" => chain(nneg(ncall("sin", vec![u.clone()]))),
                "tan" => chain(nbin('/', nnum(1.0), nbin('^', ncall("cos", vec![u.clone()]), nnum(2.0)))),
                "exp" => chain(ncall("exp", vec![u.clone()])),
                "ln" => chain(nbin('/', nnum(1.0), u.clone())),
                "log" | "log10" => chain(nbin('/', nnum(1.0), nbin('*', u.clone(), ncall("ln", vec![nnum(10.0)])))),
                "log2" => chain(nbin('/', nnum(1.0), nbin('*', u.clone(), ncall("ln", vec![nnum(2.0)])))),
                "sqrt" => chain(nbin('/', nnum(1.0), nbin('*', nnum(2.0), ncall("sqrt", vec![u.clone()])))),
                "asin" => chain(nbin('/', nnum(1.0), ncall("sqrt", vec![nbin('-', nnum(1.0), nbin('^', u.clone(), nnum(2.0)))]))),
                "acos" => chain(nneg(nbin('/', nnum(1.0), ncall("sqrt", vec![nbin('-', nnum(1.0), nbin('^', u.clone(), nnum(2.0)))])))),
                "atan" => chain(nbin('/', nnum(1.0), nbin('+', nnum(1.0), nbin('^', u.clone(), nnum(2.0))))),
                "sinh" => chain(ncall("cosh", vec![u.clone()])),
                "cosh" => chain(ncall("sinh", vec![u.clone()])),
                "tanh" => chain(nbin('-', nnum(1.0), nbin('^', ncall("tanh", vec![u.clone()]), nnum(2.0)))),
                "abs" => chain(ncall("sign", vec![u.clone()])),
                _ => return Err(format!("Dérivée de « {name} » non prise en charge")),
            };
            Ok(r)
        }
    }
}

// ── Simplification (constant folding + algebraic identities, run to a fixpoint) ──────
fn simp_once(n: &Node) -> Node {
    match n {
        Node::Num(_) | Node::Var => n.clone(),
        Node::Neg(a) => {
            let a = simp_once(a);
            if let Node::Num(v) = a {
                nnum(-v)
            } else if let Node::Neg(inner) = a {
                *inner
            } else {
                nneg(a)
            }
        }
        Node::Call { name, args } => Node::Call { name: name.clone(), args: args.iter().map(simp_once).collect() },
        Node::Bin { op, a, b } => {
            let a = simp_once(a);
            let b = simp_once(b);
            let op = *op;
            let an = as_num(&a);
            let bn = as_num(&b);
            if let (Some(an), Some(bn)) = (an, bn) {
                match op {
                    '+' => return nnum(an + bn),
                    '-' => return nnum(an - bn),
                    '*' => return nnum(an * bn),
                    '/' if bn != 0.0 => return nnum(an / bn),
                    '^' => {
                        let p = an.powf(bn);
                        if p.is_finite() {
                            return nnum(p);
                        }
                    }
                    _ => {}
                }
            }
            if op == '+' {
                if an == Some(0.0) {
                    return b;
                }
                if bn == Some(0.0) {
                    return a;
                }
            }
            if op == '-' {
                if bn == Some(0.0) {
                    return a;
                }
                if an == Some(0.0) {
                    return nneg(b);
                }
                if a == b {
                    return nnum(0.0);
                }
            }
            if op == '*' {
                if an == Some(0.0) || bn == Some(0.0) {
                    return nnum(0.0);
                }
                if an == Some(1.0) {
                    return b;
                }
                if bn == Some(1.0) {
                    return a;
                }
                if an == Some(-1.0) {
                    return nneg(b);
                }
                if bn == Some(-1.0) {
                    return nneg(a);
                }
                // Fold a numeric factor into a nested product: c·(d·y) → (c·d)·y.
                if let Some(an) = an {
                    if let Node::Bin { op: '*', a: ba, b: bb } = &b {
                        if let Node::Num(v) = **ba {
                            return nbin('*', nnum(an * v), (**bb).clone());
                        }
                        if let Node::Num(v) = **bb {
                            return nbin('*', nnum(an * v), (**ba).clone());
                        }
                    }
                }
                if let Some(bn) = bn {
                    if let Node::Bin { op: '*', a: aa, b: ab } = &a {
                        if let Node::Num(v) = **aa {
                            return nbin('*', nnum(bn * v), (**ab).clone());
                        }
                        if let Node::Num(v) = **ab {
                            return nbin('*', nnum(bn * v), (**aa).clone());
                        }
                    }
                }
            }
            if op == '/' {
                if an == Some(0.0) {
                    return nnum(0.0);
                }
                if bn == Some(1.0) {
                    return a;
                }
                if a == b {
                    return nnum(1.0);
                }
            }
            if op == '^' {
                if bn == Some(0.0) {
                    return nnum(1.0);
                }
                if bn == Some(1.0) {
                    return a;
                }
                if an == Some(1.0) {
                    return nnum(1.0);
                }
                if an == Some(0.0) {
                    return nnum(0.0);
                }
            }
            nbin(op, a, b)
        }
    }
}

pub fn simplify(n: &Node) -> Node {
    let mut cur = n.clone();
    for _ in 0..25 {
        let next = simp_once(&cur);
        if next == cur {
            return next;
        }
        cur = next;
    }
    cur
}

// ── Pretty-print AST → LaTeX ─────────────────────────────────────────────────────────
fn fn_latex(name: &str) -> Option<&'static str> {
    Some(match name {
        "sin" => "\\sin",
        "cos" => "\\cos",
        "tan" => "\\tan",
        "asin" => "\\arcsin",
        "acos" => "\\arccos",
        "atan" => "\\arctan",
        "sinh" => "\\sinh",
        "cosh" => "\\cosh",
        "tanh" => "\\tanh",
        "ln" => "\\ln",
        "log" | "log10" => "\\log",
        "log2" => "\\log_2",
        "sign" => "\\operatorname{sign}",
        _ => return None,
    })
}

fn num_latex(v: f64) -> String {
    if !v.is_finite() {
        return "\\text{indéfini}".to_string();
    }
    if (v - E).abs() < 1e-9 {
        return "e".to_string();
    }
    if (v - PI).abs() < 1e-9 {
        return "\\pi".to_string();
    }
    if (v - PI * 2.0).abs() < 1e-9 {
        return "\\tau".to_string();
    }
    if v.fract() == 0.0 && v.abs() < 1e15 {
        return js_number_to_string(v);
    }
    to_precision_str(v, 10)
}

/// Fold `e^u` back into `exp(u)` (`mathCas.ts` `normConst`).
pub fn norm_const(n: &Node) -> Node {
    match n {
        Node::Neg(a) => nneg(norm_const(a)),
        Node::Call { name, args } => Node::Call { name: name.clone(), args: args.iter().map(norm_const).collect() },
        Node::Bin { op, a, b } => {
            let a = norm_const(a);
            let b = norm_const(b);
            if *op == '^' {
                if let Node::Num(av) = a {
                    if (av - E).abs() < 1e-9 {
                        return ncall("exp", vec![b]);
                    }
                }
            }
            nbin(*op, a, b)
        }
        _ => n.clone(),
    }
}

fn prec(n: &Node) -> i32 {
    match n {
        Node::Num(v) => {
            if *v < 0.0 {
                2
            } else {
                5
            }
        }
        Node::Var | Node::Call { .. } => 5,
        Node::Neg(_) => 2,
        Node::Bin { op, .. } => match op {
            '+' | '-' => 1,
            '*' => 2,
            '/' => 5,
            _ => 4, // ^
        },
    }
}

fn atomic_base(n: &Node) -> bool {
    matches!(n, Node::Num(v) if *v >= 0.0) || matches!(n, Node::Var | Node::Call { .. })
}

fn wrap(c: &Node, min: i32) -> String {
    let s = node_to_latex(c);
    if prec(c) < min {
        format!("\\left({s}\\right)")
    } else {
        s
    }
}

pub fn node_to_latex(n: &Node) -> String {
    match n {
        Node::Num(v) => num_latex(*v),
        Node::Var => "x".to_string(),
        Node::Neg(a) => format!("-{}", wrap(a, 2)),
        Node::Call { name, args } => {
            let u = &args[0];
            if name == "sqrt" {
                return format!("\\sqrt{{{}}}", node_to_latex(u));
            }
            if name == "cbrt" {
                return format!("\\sqrt[3]{{{}}}", node_to_latex(u));
            }
            if name == "abs" {
                return format!("\\left|{}\\right|", node_to_latex(u));
            }
            if name == "exp" {
                return format!("e^{{{}}}", node_to_latex(u));
            }
            let args_s = args.iter().map(node_to_latex).collect::<Vec<_>>().join(",\\ ");
            let fname = fn_latex(name).map(str::to_string).unwrap_or_else(|| format!("\\operatorname{{{name}}}"));
            format!("{fname}\\left({args_s}\\right)")
        }
        Node::Bin { op, a, b } => match op {
            '/' => format!("\\frac{{{}}}{{{}}}", node_to_latex(a), node_to_latex(b)),
            '^' => {
                let base = if atomic_base(a) { node_to_latex(a) } else { format!("\\left({}\\right)", node_to_latex(a)) };
                format!("{base}^{{{}}}", node_to_latex(b))
            }
            '*' => {
                let sa = wrap(a, 2);
                if let Node::Num(av) = **a {
                    if av >= 0.0 && !matches!(**b, Node::Num(_)) {
                        let right_atomic = matches!(**b, Node::Var | Node::Call { .. })
                            || matches!(&**b, Node::Bin { op: '^', .. });
                        let sum_paren = matches!(&**b, Node::Bin { op: '+' | '-', .. });
                        if right_atomic || sum_paren {
                            let sb = if sum_paren { format!("\\left({}\\right)", node_to_latex(b)) } else { wrap(b, 2) };
                            return if sa.ends_with(|c: char| c.is_ascii_alphabetic()) { format!("{sa} {sb}") } else { format!("{sa}{sb}") };
                        }
                    }
                }
                format!("{sa} \\cdot {}", wrap(b, 2))
            }
            '+' | '-' => format!("{} {op} {}", wrap(a, 1), wrap(b, if *op == '-' { 2 } else { 1 })),
            _ => String::new(),
        },
    }
}

// ── High-level entry points ──────────────────────────────────────────────────────────

/// d/dx of a LaTeX formula → LaTeX.
pub fn derivative_latex(latex: &str) -> CasResult {
    let Some(expr) = latex_to_expr(latex) else {
        return CasResult::err(NOT_EXPR);
    };
    match parse_expr(&expr) {
        Ok(ast) => match differentiate(&norm_const(&ast)) {
            Ok(d) => CasResult::ok(node_to_latex(&simplify(&d))),
            Err(e) => CasResult::err(&e),
        },
        Err(e) => CasResult::err(&e),
    }
}

/// Algebraic simplification of a LaTeX formula → LaTeX.
pub fn simplify_latex(latex: &str) -> CasResult {
    let Some(expr) = latex_to_expr(latex) else {
        return CasResult::err(NOT_EXPR);
    };
    match parse_expr(&expr) {
        Ok(ast) => CasResult::ok(node_to_latex(&simplify(&norm_const(&ast)))),
        Err(e) => CasResult::err(&e),
    }
}

/// Real roots of f(x)=0 on [-10,10] by sampling + bisection (`mathCas.ts` `solveRoots` defaults).
pub fn solve_roots(latex: &str) -> (Vec<f64>, Option<String>) {
    solve_roots_on(latex, -10.0, 10.0, 2000)
}

fn solve_roots_on(latex: &str, xmin: f64, xmax: f64, steps: i64) -> (Vec<f64>, Option<String>) {
    let Some(expr) = latex_to_expr(latex) else {
        return (vec![], Some(NOT_EXPR.to_string()));
    };
    let c = compile(&expr);
    if let Some(e) = c.error {
        return (vec![], Some(e));
    }
    let f = |x: f64| c.eval(x);
    let mut roots: Vec<f64> = Vec::new();
    let push = |roots: &mut Vec<f64>, r: f64| {
        if r.is_finite() && !roots.iter().any(|x| (x - r).abs() < 1e-6) {
            roots.push(r);
        }
    };
    let dx = (xmax - xmin) / steps as f64;
    let mut px = xmin;
    let mut pv = f(xmin);
    if pv.abs() < 1e-9 {
        push(&mut roots, px);
    }
    for i in 1..=steps {
        let xx = xmin + i as f64 * dx;
        let v = f(xx);
        if pv.is_finite() && v.is_finite() && pv * v < 0.0 {
            let (mut lo, mut hi, mut flo) = (px, xx, pv);
            for _ in 0..60 {
                let mid = (lo + hi) / 2.0;
                let fm = f(mid);
                if fm.abs() < 1e-12 {
                    lo = mid;
                    hi = mid;
                    break;
                }
                if flo * fm < 0.0 {
                    hi = mid;
                } else {
                    lo = mid;
                    flo = fm;
                }
            }
            push(&mut roots, (lo + hi) / 2.0);
        } else if v.abs() < 1e-9 {
            push(&mut roots, xx);
        }
        px = xx;
        pv = v;
    }
    roots.sort_by(|a, b| a.partial_cmp(b).unwrap());
    roots.truncate(12);
    (roots, None)
}

/// Table of values → a LaTeX array (`mathCas.ts` `valueTableLatex`).
pub fn value_table_latex(latex: &str, xs: &[f64]) -> CasResult {
    let Some(expr) = latex_to_expr(latex) else {
        return CasResult::err(NOT_EXPR);
    };
    let c = compile(&expr);
    if let Some(e) = c.error {
        return CasResult::err(&e);
    }
    let cell = |v: f64| if v.is_finite() { fmt_result(prec6(v)) } else { "\\text{-}".to_string() };
    let cols = format!("c|{}", "c".repeat(xs.len()));
    let xrow = xs.iter().map(|x| num_latex(*x)).collect::<Vec<_>>().join(" & ");
    let yrow = xs.iter().map(|x| cell(c.eval(*x))).collect::<Vec<_>>().join(" & ");
    CasResult::ok(format!("\\begin{{array}}{{{cols}}} x & {xrow} \\\\ \\hline f(x) & {yrow} \\end{{array}}"))
}

/// `Number(v.toPrecision(6))` as an f64.
fn prec6(v: f64) -> f64 {
    format!("{:.*e}", 5, v).parse().unwrap_or(v)
}

// ── Rational / polynomial helpers ─────────────────────────────────────────────────────
fn gcd_i(mut a: f64, mut b: f64) -> f64 {
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
fn factorial(n: i64) -> f64 {
    let mut r = 1.0;
    for i in 2..=n {
        r *= i as f64;
    }
    r
}

/// Best rational p/q ≈ x with a bounded denominator (`mathCas.ts` `toFrac`).
fn to_frac(x: f64, max_den: i64) -> (f64, f64) {
    if x.fract() == 0.0 {
        return (x, 1.0);
    }
    for d in 2..=max_den {
        let p = x * d as f64;
        if (p - js_round(p)).abs() < 1e-9 {
            let pr = js_round(p);
            let g = gcd_i(pr, d as f64);
            return (pr / g, d as f64 / g);
        }
    }
    (x, 1.0)
}
fn x_pow(k: i64) -> String {
    match k {
        0 => String::new(),
        1 => "x".to_string(),
        _ => format!("x^{{{k}}}"),
    }
}

/// A signed term `{neg, body}`.
type Term = Option<(bool, String)>;

fn monomial(coeff: f64, pow: i64) -> Term {
    if coeff.abs() < 1e-12 {
        return None;
    }
    let (p, q) = to_frac(coeff.abs(), 5040);
    let ps = if p.fract() == 0.0 { js_number_to_string(p) } else { to_precision_str(p, 6) };
    let xp = x_pow(pow);
    let hide_one = pow > 0 && p == 1.0;
    let num_tex = if pow == 0 { ps.clone() } else { format!("{}{xp}", if hide_one { String::new() } else { ps }) };
    let body = if q == 1.0 { num_tex } else { format!("\\frac{{{num_tex}}}{{{}}}", js_number_to_string(q)) };
    Some((coeff < 0.0, body))
}

fn join_terms(parts: &[Term]) -> String {
    let t: Vec<&(bool, String)> = parts.iter().filter_map(|p| p.as_ref()).collect();
    if t.is_empty() {
        return "0".to_string();
    }
    let mut s = format!("{}{}", if t[0].0 { "-" } else { "" }, t[0].1);
    for item in &t[1..] {
        s += &format!("{}{}", if item.0 { " - " } else { " + " }, item.1);
    }
    s
}

/// Coefficient array (index = power of x) of a single-variable polynomial; `None` if not polynomial.
fn poly_of(n: &Node) -> Option<Vec<f64>> {
    match n {
        Node::Num(v) => Some(vec![*v]),
        Node::Var => Some(vec![0.0, 1.0]),
        Node::Neg(a) => poly_of(a).map(|p| p.iter().map(|c| -c).collect()),
        Node::Call { .. } => None,
        Node::Bin { op, a, b } => {
            let pa = poly_of(a);
            let pb = poly_of(b);
            match op {
                '+' | '-' => {
                    let (a, b) = (pa?, pb?);
                    let len = a.len().max(b.len());
                    let mut r = vec![0.0; len];
                    for (i, rv) in r.iter_mut().enumerate() {
                        *rv = a.get(i).copied().unwrap_or(0.0) + if *op == '-' { -1.0 } else { 1.0 } * b.get(i).copied().unwrap_or(0.0);
                    }
                    Some(r)
                }
                '*' => {
                    let (a, b) = (pa?, pb?);
                    let mut r = vec![0.0; a.len() + b.len() - 1];
                    for (i, &av) in a.iter().enumerate() {
                        for (j, &bv) in b.iter().enumerate() {
                            r[i + j] += av * bv;
                        }
                    }
                    Some(r)
                }
                '/' => {
                    let (a, b) = (pa?, pb?);
                    if b.len() == 1 && b[0].abs() > 1e-12 {
                        Some(a.iter().map(|c| c / b[0]).collect())
                    } else {
                        None
                    }
                }
                '^' => {
                    let a = pa?;
                    if let Node::Num(bv) = **b {
                        if bv.fract() == 0.0 && (0.0..=10.0).contains(&bv) {
                            let mut r = vec![1.0];
                            for _ in 0..bv as i64 {
                                let mut nr = vec![0.0; r.len() + a.len() - 1];
                                for (i, &rv) in r.iter().enumerate() {
                                    for (j, &av) in a.iter().enumerate() {
                                        nr[i + j] += rv * av;
                                    }
                                }
                                r = nr;
                            }
                            return Some(r);
                        }
                    }
                    None
                }
                _ => None,
            }
        }
    }
}

fn trim_poly(p: &[f64]) -> Vec<f64> {
    let mut e = p.len() - 1;
    while e > 0 && p[e].abs() < 1e-12 {
        e -= 1;
    }
    p[..=e].to_vec()
}
fn poly_to_latex(p: &[f64]) -> String {
    let t = trim_poly(p);
    let mut terms: Vec<Term> = t.iter().enumerate().map(|(i, &c)| monomial(c, i as i64)).collect();
    terms.reverse();
    join_terms(&terms)
}

// ── Symbolic integration ───────────────────────────────────────────────────────────
fn lin_coef(u: &Node) -> Option<f64> {
    if matches!(u, Node::Var) {
        return Some(1.0);
    }
    if let Node::Bin { op: '*', a, b } = u {
        if let (Node::Num(v), Node::Var) = (&**a, &**b) {
            return Some(*v);
        }
        if let (Node::Num(v), Node::Var) = (&**b, &**a) {
            return Some(*v);
        }
    }
    None
}

pub fn integrate(n: &Node) -> Result<Node, String> {
    match n {
        Node::Num(v) => Ok(nbin('*', nnum(*v), x())),
        Node::Var => Ok(nbin('/', nbin('^', x(), nnum(2.0)), nnum(2.0))),
        Node::Neg(a) => Ok(nneg(integrate(a)?)),
        Node::Bin { op, a, b } => match op {
            '+' | '-' => Ok(nbin(*op, integrate(a)?, integrate(b)?)),
            '*' => {
                if let Node::Num(_) = **a {
                    Ok(nbin('*', (**a).clone(), integrate(b)?))
                } else if let Node::Num(_) = **b {
                    Ok(nbin('*', (**b).clone(), integrate(a)?))
                } else {
                    Err("Intégrale de ce produit non prise en charge".to_string())
                }
            }
            '/' => {
                if let Node::Num(_) = **b {
                    Ok(nbin('/', integrate(a)?, (**b).clone()))
                } else if matches!(**a, Node::Num(_)) && matches!(**b, Node::Var) {
                    Ok(nbin('*', (**a).clone(), ncall("ln", vec![ncall("abs", vec![x()])])))
                } else {
                    Err("Intégrale de ce quotient non prise en charge".to_string())
                }
            }
            '^' => {
                if let (Node::Var, Node::Num(bv)) = (&**a, &**b) {
                    if *bv == -1.0 {
                        Ok(ncall("ln", vec![ncall("abs", vec![x()])]))
                    } else {
                        Ok(nbin('/', nbin('^', x(), nnum(bv + 1.0)), nnum(bv + 1.0)))
                    }
                } else {
                    Err("Intégrale de cette puissance non prise en charge".to_string())
                }
            }
            _ => Err("Intégrale non prise en charge".to_string()),
        },
        Node::Call { name, args } => {
            let u = &args[0];
            let k = lin_coef(u);
            let over = |node: Node| if k == Some(1.0) { node } else { nbin('/', node, nnum(k.unwrap())) };
            if let Some(_k) = k {
                match name.as_str() {
                    "exp" => return Ok(over(ncall("exp", vec![u.clone()]))),
                    "sin" => return Ok(over(nneg(ncall("cos", vec![u.clone()])))),
                    "cos" => return Ok(over(ncall("sin", vec![u.clone()]))),
                    _ => {}
                }
            }
            if name == "sqrt" && matches!(u, Node::Var) {
                return Ok(nbin('*', nnum(2.0 / 3.0), nbin('^', x(), nnum(1.5))));
            }
            Err(format!("Intégrale de « {name} » non prise en charge"))
        }
    }
}

/// ∫ f dx (+ C) (`mathCas.ts` `integralLatex`).
pub fn integral_latex(latex: &str) -> CasResult {
    let Some(expr) = latex_to_expr(latex) else {
        return CasResult::err(NOT_EXPR);
    };
    let ast = match parse_expr(&expr) {
        Ok(a) => norm_const(&a),
        Err(e) => return CasResult::err(&e),
    };
    if let Some(p) = poly_of(&ast) {
        let t = trim_poly(&p);
        let mut terms: Vec<Term> = t.iter().enumerate().map(|(i, &c)| monomial(c / (i as f64 + 1.0), i as i64 + 1)).collect();
        terms.reverse();
        return CasResult::ok(format!("{} + C", join_terms(&terms)));
    }
    match integrate(&ast) {
        Ok(i) => CasResult::ok(format!("{} + C", node_to_latex(&simplify(&i)))),
        Err(e) => CasResult::err(&e),
    }
}

/// Numeric definite integral ∫ₐᵇ f dx (composite Simpson).
pub fn definite_integral(latex: &str, a: f64, b: f64) -> (Option<f64>, Option<String>) {
    let Some(expr) = latex_to_expr(latex) else {
        return (None, Some(NOT_EXPR.to_string()));
    };
    let c = compile(&expr);
    if let Some(e) = c.error {
        return (None, Some(e));
    }
    let n = 1000;
    let h = (b - a) / n as f64;
    let mut s = c.eval(a) + c.eval(b);
    for i in 1..n {
        s += (if i % 2 == 1 { 4.0 } else { 2.0 }) * c.eval(a + i as f64 * h);
    }
    let v = (s * h) / 3.0;
    if v.is_finite() {
        (Some(v), None)
    } else {
        (None, Some("Intégrale divergente sur cet intervalle.".to_string()))
    }
}

/// Expand a polynomial.
pub fn expand_latex(latex: &str) -> CasResult {
    let Some(expr) = latex_to_expr(latex) else {
        return CasResult::err(NOT_EXPR);
    };
    match parse_expr(&expr) {
        Ok(ast) => match poly_of(&norm_const(&ast)) {
            Some(p) => CasResult::ok(poly_to_latex(&p)),
            None => CasResult::err("Développement limité aux expressions polynomiales."),
        },
        Err(e) => CasResult::err(&e),
    }
}

// ── nth derivative ─────────────────────────────────────────────────────────────────
pub fn nth_derivative_latex(latex: &str, n: i64) -> CasResult {
    let Some(expr) = latex_to_expr(latex) else {
        return CasResult::err(NOT_EXPR);
    };
    let mut cur = match parse_expr(&expr) {
        Ok(a) => norm_const(&a),
        Err(e) => return CasResult::err(&e),
    };
    for _ in 0..n {
        cur = match differentiate(&cur) {
            Ok(d) => simplify(&d),
            Err(e) => return CasResult::err(&e),
        };
    }
    CasResult::ok(node_to_latex(&cur))
}

// ── evalAt (closed-form numeric evaluation of an AST) ─────────────────────────────────
fn eval_at(n: &Node, x: f64) -> f64 {
    match n {
        Node::Num(v) => *v,
        Node::Var => x,
        Node::Neg(a) => -eval_at(a, x),
        Node::Bin { op, a, b } => {
            let a = eval_at(a, x);
            let b = eval_at(b, x);
            match op {
                '+' => a + b,
                '-' => a - b,
                '*' => a * b,
                '/' => a / b,
                '^' => a.powf(b),
                _ => f64::NAN,
            }
        }
        Node::Call { name, args } => {
            let a = eval_at(&args[0], x);
            match name.as_str() {
                "sin" => a.sin(),
                "cos" => a.cos(),
                "tan" => a.tan(),
                "exp" => a.exp(),
                "ln" => a.ln(),
                "log" | "log10" => a.log10(),
                "log2" => a.log2(),
                "sqrt" => a.sqrt(),
                "abs" => a.abs(),
                "sign" => {
                    if a.is_nan() {
                        f64::NAN
                    } else if a > 0.0 {
                        1.0
                    } else if a < 0.0 {
                        -1.0
                    } else {
                        a
                    }
                }
                "sinh" => a.sinh(),
                "cosh" => a.cosh(),
                "tanh" => a.tanh(),
                "asin" => a.asin(),
                "acos" => a.acos(),
                "atan" => a.atan(),
                _ => f64::NAN,
            }
        }
    }
}

fn clean(v: f64) -> f64 {
    if (v - js_round(v)).abs() < 1e-9 {
        js_round(v)
    } else {
        prec6(v)
    }
}

/// Tangent line at x = a (`mathCas.ts` `tangentAt`): returns (equation LaTeX, plain expr, error).
pub fn tangent_at(latex: &str, a: f64) -> (String, String, Option<String>) {
    let Some(e) = latex_to_expr(latex) else {
        return (String::new(), String::new(), Some(NOT_EXPR.to_string()));
    };
    let ast = match parse_expr(&e) {
        Ok(x) => norm_const(&x),
        Err(err) => return (String::new(), String::new(), Some(err)),
    };
    let fa = eval_at(&ast, a);
    let fpa = match differentiate(&ast) {
        Ok(d) => eval_at(&simplify(&d), a),
        Err(err) => return (String::new(), String::new(), Some(err)),
    };
    if !fa.is_finite() || !fpa.is_finite() {
        return (String::new(), String::new(), Some("Tangente indéfinie en ce point.".to_string()));
    }
    let node = simplify(&nbin('+', nbin('*', nnum(fpa), nbin('-', x(), nnum(a))), nnum(fa)));
    (
        format!("y = {}", node_to_latex(&node)),
        format!("({fpa})*(x-({a}))+({fa})"),
        None,
    )
}

/// Critical points on [-10,10], classified via f'' (`mathCas.ts` `extremaLatex`).
pub fn extrema_latex(latex: &str) -> CasResult {
    let Some(e) = latex_to_expr(latex) else {
        return CasResult::err(NOT_EXPR);
    };
    let ast = match parse_expr(&e) {
        Ok(a) => norm_const(&a),
        Err(err) => return CasResult::err(&err),
    };
    let fp = match differentiate(&ast) {
        Ok(d) => simplify(&d),
        Err(err) => return CasResult::err(&err),
    };
    if matches!(fp, Node::Num(_)) {
        return CasResult::err("Pas d'extremum (dérivée constante).");
    }
    let (roots, error) = solve_roots(&node_to_latex(&fp));
    if let Some(e) = error {
        return CasResult::err(&e);
    }
    if roots.is_empty() {
        return CasResult::err("Aucun point critique sur [-10, 10].");
    }
    let fpp = match differentiate(&fp) {
        Ok(d) => simplify(&d),
        Err(err) => return CasResult::err(&err),
    };
    let xr = roots.iter().map(|&r| num_latex(clean(r))).collect::<Vec<_>>().join(" & ");
    let yr = roots
        .iter()
        .map(|&r| {
            let v = eval_at(&ast, r);
            if v.is_finite() {
                num_latex(clean(v))
            } else {
                "\\text{-}".to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" & ");
    let nr = roots
        .iter()
        .map(|&r| {
            let s = eval_at(&fpp, r);
            if s > 1e-7 {
                "\\text{min}".to_string()
            } else if s < -1e-7 {
                "\\text{max}".to_string()
            } else {
                "\\text{infl.}".to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" & ");
    let cols = format!("c|{}", "c".repeat(roots.len()));
    CasResult::ok(format!("\\begin{{array}}{{{cols}}} x & {xr} \\\\ \\hline f(x) & {yr} \\\\ \\hline \\text{{nature}} & {nr} \\end{{array}}"))
}

/// Numeric limit of f at a finite point or ±∞ (`mathCas.ts` `limitLatex`).
pub fn limit_latex(latex: &str, at: f64) -> CasResult {
    let Some(e) = latex_to_expr(latex) else {
        return CasResult::err(NOT_EXPR);
    };
    let c = compile(&e);
    if let Some(err) = c.error {
        return CasResult::err(&err);
    }
    let f = |x: f64| c.eval(x);
    let at_l = if !at.is_finite() {
        if at > 0.0 {
            "+\\infty".to_string()
        } else {
            "-\\infty".to_string()
        }
    } else {
        num_latex(clean(at))
    };
    let wrap = |l: String| CasResult::ok(format!("\\lim_{{x \\to {at_l}}} f(x) = {l}"));
    if !at.is_finite() {
        let s = if at > 0.0 { 1.0 } else { -1.0 };
        let v: Vec<f64> = [1e5, 1e8, 1e11].iter().map(|x| f(s * x)).collect();
        if v.iter().all(|x| x.is_finite()) && (v[2] - v[1]).abs() < 1e-5 * (1.0 + v[2].abs()) {
            return wrap(num_latex(clean(v[2])));
        }
        if v[2].abs() > 1e10 {
            return wrap(if v[2] > 0.0 { "+\\infty".to_string() } else { "-\\infty".to_string() });
        }
        return CasResult::err("La limite en l'infini n'existe pas (divergence).");
    }
    let l: Vec<f64> = [1e-4, 1e-6, 1e-8].iter().map(|h| f(at - h)).collect();
    let r: Vec<f64> = [1e-4, 1e-6, 1e-8].iter().map(|h| f(at + h)).collect();
    let (lv, rv) = (l[2], r[2]);
    if lv.is_finite() && rv.is_finite() && (lv - rv).abs() < 1e-4 * (1.0 + lv.abs()) {
        return wrap(num_latex(clean((lv + rv) / 2.0)));
    }
    if (!lv.is_finite() || lv.abs() > 1e8) && (!rv.is_finite() || rv.abs() > 1e8) {
        return CasResult::err("Limite infinie (asymptote verticale en ce point).");
    }
    CasResult::err("La limite n'existe pas (valeurs différentes à gauche et à droite).")
}

// ── Exact / radical helpers ────────────────────────────────────────────────────────
fn rat_latex(v: f64) -> String {
    if (v - js_round(v)).abs() < 1e-9 {
        return js_number_to_string(js_round(v));
    }
    let (p, q) = to_frac(v, 5040);
    if q == 1.0 {
        js_number_to_string(p)
    } else if p < 0.0 {
        format!("-\\frac{{{}}}{{{}}}", js_number_to_string(-p), js_number_to_string(q))
    } else {
        format!("\\frac{{{}}}{{{}}}", js_number_to_string(p), js_number_to_string(q))
    }
}
fn simplify_radical(n: f64) -> (f64, f64) {
    let mut rad = js_round(n);
    let mut coef = 1.0;
    let mut f = 2.0;
    while f * f <= rad {
        while rad % (f * f) == 0.0 {
            rad /= f * f;
            coef *= f;
        }
        f += 1.0;
    }
    (coef, rad)
}
fn rad_term(coef: f64, rad: f64) -> String {
    if rad == 1.0 {
        js_number_to_string(coef)
    } else if coef == 1.0 {
        format!("\\sqrt{{{}}}", js_number_to_string(rad))
    } else {
        format!("{}\\sqrt{{{}}}", js_number_to_string(coef), js_number_to_string(rad))
    }
}

/// Exact roots of a polynomial of degree ≤ 2 (`mathCas.ts` `solveExactLatex`).
pub fn solve_exact_latex(latex: &str) -> CasResult {
    let Some(e) = latex_to_expr(latex) else {
        return CasResult::err(NOT_EXPR);
    };
    let p = match parse_expr(&e) {
        Ok(a) => poly_of(&norm_const(&a)),
        Err(_) => None,
    };
    let Some(p) = p else {
        return CasResult::err("Résolution exacte limitée aux polynômes (degré ≤ 2).");
    };
    let c = trim_poly(&p);
    if c.len() <= 1 {
        return if c.first().copied().unwrap_or(0.0).abs() > 1e-9 {
            CasResult::err("Aucune solution.")
        } else {
            CasResult::err("Tout réel est solution.")
        };
    }
    if c.len() == 2 {
        let (b, a) = (c[0], c[1]);
        return CasResult::ok(format!("x = {}", rat_latex(-b / a)));
    }
    // degree 2
    let (cc, b, a) = (c[0], c[1], c[2]);
    let disc = b * b - 4.0 * a * cc;
    if disc.abs() < 1e-9 {
        return CasResult::ok(format!("x = {} \\quad (\\text{{racine double}})", rat_latex(-b / (2.0 * a))));
    }
    let pm = |nb: f64, coef: f64, rad: f64, den: f64, imaginary: bool| {
        let all_int = [nb, coef, den].iter().all(|v| v.fract() == 0.0);
        let g = if all_int { gcd_i(gcd_i(nb.abs(), coef), den.abs()) } else { 1.0 };
        let (nb, co, den) = (nb / g, coef / g, den / g);
        let mut rt = rad_term(co, rad);
        if imaginary {
            rt = if rt == "1" { "i".to_string() } else { format!("{rt}\\,i") };
        }
        let numer = if nb.abs() < 1e-9 { format!("\\pm {rt}") } else { format!("{} \\pm {rt}", rat_latex(nb)) };
        if den == 1.0 {
            format!("x = {numer}")
        } else {
            format!("x = \\frac{{{numer}}}{{{}}}", js_number_to_string(den))
        }
    };
    if disc > 0.0 {
        let (coef, rad) = simplify_radical(disc);
        if rad == 1.0 {
            return CasResult::ok(format!(
                "x_1 = {}, \\quad x_2 = {}",
                rat_latex((-b - coef) / (2.0 * a)),
                rat_latex((-b + coef) / (2.0 * a))
            ));
        }
        return CasResult::ok(pm(-b, coef, rad, 2.0 * a, false));
    }
    let (coef, rad) = simplify_radical(-disc);
    CasResult::ok(pm(-b, coef, rad, 2.0 * a, true))
}

/// Finite sum Σ / product Π over integer x from a to b (`mathCas.ts` `accumulate`).
fn accumulate(latex: &str, mut a: f64, mut b: f64, kind: &str) -> CasResult {
    let Some(e) = latex_to_expr(latex) else {
        return CasResult::err(NOT_EXPR);
    };
    let c = compile(&e);
    if let Some(err) = c.error {
        return CasResult::err(&err);
    }
    a = js_round(a);
    b = js_round(b);
    if b < a || b - a > 1e6 {
        return CasResult::err("Bornes invalides (a ≤ b, ≤ 10⁶ termes).");
    }
    let mut acc = if kind == "sum" { 0.0 } else { 1.0 };
    let mut i = a;
    while i <= b {
        let v = c.eval(i);
        if !v.is_finite() {
            return CasResult::err("Terme non défini dans l’intervalle.");
        }
        acc = if kind == "sum" { acc + v } else { acc * v };
        i += 1.0;
    }
    let op = if kind == "sum" { "\\sum" } else { "\\prod" };
    CasResult::ok(format!("{op}_{{x={}}}^{{{}}} {} = {}", js_number_to_string(a), js_number_to_string(b), latex.trim(), num_latex(clean(acc))))
}
pub fn sum_latex(latex: &str, a: f64, b: f64) -> CasResult {
    accumulate(latex, a, b, "sum")
}
pub fn product_latex(latex: &str, a: f64, b: f64) -> CasResult {
    accumulate(latex, a, b, "prod")
}

/// Evaluate the current function at x = a (`mathCas.ts` `evalPointLatex`).
pub fn eval_point_latex(latex: &str, a: f64) -> CasResult {
    let Some(e) = latex_to_expr(latex) else {
        return CasResult::err(NOT_EXPR);
    };
    let c = compile(&e);
    if let Some(err) = c.error {
        return CasResult::err(&err);
    }
    let v = c.eval(a);
    if !v.is_finite() {
        return CasResult::err("Fonction non définie en ce point.");
    }
    CasResult::ok(format!("f\\left({}\\right) = {}", rat_latex(a), num_latex(clean(v))))
}

// ── Factor (polynomials of degree ≤ 2) ────────────────────────────────────────────────
pub fn factor_latex(latex: &str) -> CasResult {
    let Some(expr) = latex_to_expr(latex) else {
        return CasResult::err(NOT_EXPR);
    };
    let p = match parse_expr(&expr) {
        Ok(a) => poly_of(&norm_const(&a)),
        Err(_) => None,
    };
    let Some(p) = p else {
        return CasResult::err("Factorisation limitée aux polynômes.");
    };
    let c = trim_poly(&p);
    let lead = |a: f64| if (a - 1.0).abs() < 1e-9 { String::new() } else { num_latex(a) };
    let lin_factor = |r: f64| {
        let (rp, rq) = to_frac(r, 5040);
        let rl = if rq == 1.0 { js_number_to_string(rp) } else { format!("\\frac{{{}}}{{{}}}", js_number_to_string(rp), js_number_to_string(rq)) };
        if r.abs() < 1e-12 {
            "x".to_string()
        } else {
            format!("\\left(x {} {}\\right)", if r < 0.0 { "+" } else { "-" }, rl.replacen('-', "", 1))
        }
    };
    if c.len() <= 1 {
        return CasResult::ok(num_latex(c.first().copied().unwrap_or(0.0)));
    }
    if c.len() == 2 {
        let (b, a) = (c[0], c[1]);
        return CasResult::ok(format!("{}{}", lead(a), lin_factor(-b / a)));
    }
    if c.len() == 3 {
        let (cc, b, a) = (c[0], c[1], c[2]);
        let disc = b * b - 4.0 * a * cc;
        if disc < -1e-9 {
            return CasResult::err("Pas de factorisation réelle (discriminant < 0).");
        }
        let s = disc.max(0.0).sqrt();
        let r1 = (-b - s) / (2.0 * a);
        let r2 = (-b + s) / (2.0 * a);
        let body = if (r1 - r2).abs() < 1e-9 {
            format!("{}^{{2}}", lin_factor(r1))
        } else {
            format!("{}{}", lin_factor(r1), lin_factor(r2))
        };
        return CasResult::ok(format!("{}{body}", lead(a)));
    }
    CasResult::err("Factorisation limitée au degré 2.")
}

// ── Taylor / Maclaurin ─────────────────────────────────────────────────────────────
fn pow_term_base(coeff: f64, pow: i64, base: &str) -> Term {
    if coeff.abs() < 1e-12 {
        return None;
    }
    let (p, q) = to_frac(coeff.abs(), 5040);
    let ps = if p.fract() == 0.0 { js_number_to_string(p) } else { to_precision_str(p, 6) };
    let xp = if pow == 0 {
        String::new()
    } else if pow == 1 {
        base.to_string()
    } else {
        format!("{base}^{{{pow}}}")
    };
    let num_tex = if pow == 0 { ps.clone() } else { format!("{}{xp}", if pow > 0 && p == 1.0 { String::new() } else { ps }) };
    let body = if q == 1.0 { num_tex } else { format!("\\frac{{{num_tex}}}{{{}}}", js_number_to_string(q)) };
    Some((coeff < 0.0, body))
}

/// Taylor polynomial up to `order` around `center` (Maclaurin when center = 0).
pub fn taylor_latex(latex: &str, order: i64, center: f64) -> CasResult {
    let Some(expr) = latex_to_expr(latex) else {
        return CasResult::err(NOT_EXPR);
    };
    let base = if center == 0.0 {
        String::new()
    } else if center > 0.0 {
        format!("\\left(x - {}\\right)", rat_latex(center))
    } else {
        format!("\\left(x + {}\\right)", rat_latex(-center))
    };
    let mut cur = match parse_expr(&expr) {
        Ok(a) => norm_const(&a),
        Err(e) => return CasResult::err(&e),
    };
    let mut terms: Vec<Term> = Vec::new();
    for k in 0..=order {
        let va = eval_at(&cur, center);
        if !va.is_finite() {
            return CasResult::err(&format!("Développement impossible (singularité en x = {}).", js_number_to_string(center)));
        }
        let rounded = if (va - js_round(va)).abs() < 1e-9 { js_round(va) } else { va };
        let coeff = rounded / factorial(k);
        terms.push(if center == 0.0 { monomial(coeff, k) } else { pow_term_base(coeff, k, &base) });
        cur = match differentiate(&cur) {
            Ok(d) => simplify(&d),
            Err(e) => return CasResult::err(&e),
        };
    }
    let rem = if center == 0.0 { format!("x^{{{}}}", order + 1) } else { format!("{base}^{{{}}}", order + 1) };
    CasResult::ok(format!("{} + O\\left({rem}\\right)", join_terms(&terms)))
}

// ── Inflection points ──────────────────────────────────────────────────────────────
pub fn inflection_latex(latex: &str) -> CasResult {
    let Some(e) = latex_to_expr(latex) else {
        return CasResult::err(NOT_EXPR);
    };
    let ast = match parse_expr(&e) {
        Ok(a) => norm_const(&a),
        Err(err) => return CasResult::err(&err),
    };
    let fpp = match differentiate(&ast).and_then(|d| differentiate(&simplify(&d))) {
        Ok(d) => simplify(&d),
        Err(err) => return CasResult::err(&err),
    };
    if matches!(fpp, Node::Num(_)) {
        return CasResult::err("Pas de point d'inflexion (f'' constante).");
    }
    let (roots, error) = solve_roots(&node_to_latex(&fpp));
    if let Some(e) = error {
        return CasResult::err(&e);
    }
    if roots.is_empty() {
        return CasResult::err("Aucun point d'inflexion sur [-10, 10].");
    }
    let xr = roots.iter().map(|&r| num_latex(clean(r))).collect::<Vec<_>>().join(" & ");
    let yr = roots
        .iter()
        .map(|&r| {
            let v = eval_at(&ast, r);
            if v.is_finite() {
                num_latex(clean(v))
            } else {
                "\\text{-}".to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" & ");
    CasResult::ok(format!("\\begin{{array}}{{c|{}}} x & {xr} \\\\ \\hline f(x) & {yr} \\end{{array}}", "c".repeat(roots.len())))
}

// ── Asymptotes ─────────────────────────────────────────────────────────────────────
pub fn asymptotes_latex(latex: &str) -> CasResult {
    let Some(e) = latex_to_expr(latex) else {
        return CasResult::err(NOT_EXPR);
    };
    let c = compile(&e);
    if let Some(err) = c.error {
        return CasResult::err(&err);
    }
    let f = |x: f64| c.eval(x);
    let mut lines: Vec<String> = Vec::new();
    let snap0 = |v: f64| if v.abs() < 1e-6 { 0.0 } else { clean(v) };
    let lp = f(1e9);
    let lp2 = f(1e6);
    if lp.is_finite() && (lp - lp2).abs() < 1e-4 * (1.0 + lp.abs()) {
        lines.push(format!("y = {} \\quad (x \\to +\\infty)", num_latex(snap0(lp))));
    }
    let lm = f(-1e9);
    let lm2 = f(-1e6);
    if lm.is_finite() && (lm - lm2).abs() < 1e-4 * (1.0 + lm.abs()) && !(lp.is_finite() && (lp - lm).abs() < 1e-6) {
        lines.push(format!("y = {} \\quad (x \\to -\\infty)", num_latex(snap0(lm))));
    }
    let mut va: Vec<f64> = Vec::new();
    let mut xx = -20.0f64;
    while xx <= 20.0 {
        let v = f(xx);
        if v.is_finite() && v.abs() > 1e5 {
            let xr = js_round(xx * 50.0) / 50.0;
            if !va.iter().any(|a| (a - xr).abs() < 0.5) {
                va.push(xr);
            }
        }
        xx += 0.02;
    }
    for a in &va {
        lines.push(format!("x = {} \\quad (\\text{{asymptote verticale}})", num_latex(clean(*a))));
    }
    if lines.is_empty() {
        return CasResult::err("Aucune asymptote détectée.");
    }
    CasResult::ok(if lines.len() > 1 {
        format!("\\begin{{gathered}} {} \\end{{gathered}}", lines.join(" \\\\ "))
    } else {
        lines[0].clone()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derivative() {
        assert_eq!(derivative_latex("x^{2}").latex, "2x");
        assert_eq!(derivative_latex("\\sin(x)").latex, "\\cos\\left(x\\right)");
        // `y` converts to the expression "y", which mathExpr rejects as an unknown symbol.
        assert_eq!(derivative_latex("y").error.as_deref(), Some("Symbole inconnu « y »"));
    }

    #[test]
    fn simplify_and_expand() {
        assert_eq!(simplify_latex("x + 0").latex, "x");
        // Plain parens are literal atoms in the WYSIWYG tree; a power of a group uses \left…\right.
        assert_eq!(expand_latex("\\left(x+1\\right)^{2}").latex, "x^{2} + 2x + 1");
    }

    #[test]
    fn integral_and_roots() {
        assert_eq!(integral_latex("x").latex, "\\frac{x^{2}}{2} + C");
        let (roots, err) = solve_roots("x^{2} - 4");
        assert_eq!(err, None);
        assert_eq!(roots.len(), 2);
        assert!((roots[0] + 2.0).abs() < 1e-4 && (roots[1] - 2.0).abs() < 1e-4);
    }

    #[test]
    fn exact_solve() {
        assert_eq!(solve_exact_latex("x^{2} - 2").latex, "x = \\pm \\sqrt{2}");
        assert_eq!(eval_point_latex("x^{2}", 3.0).latex, "f\\left(3\\right) = 9");
    }

    #[test]
    fn factor_and_taylor() {
        assert_eq!(factor_latex("x^{2} - 1").latex, "\\left(x + 1\\right)\\left(x - 1\\right)");
        // Maclaurin of e^x to order 3: 1 + x + x²/2 + x³/6
        assert_eq!(taylor_latex("e^{x}", 3, 0.0).latex, "1 + x + \\frac{x^{2}}{2} + \\frac{x^{3}}{6} + O\\left(x^{4}\\right)");
    }
}
