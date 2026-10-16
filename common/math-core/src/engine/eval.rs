//! Port of `office/web/src/mathEval.ts` - numeric evaluation of a closed-form LaTeX formula (the
//! "live calculator"): the LaTeX is parsed to a tree ([`super::tree`]), converted to a plain textual
//! expression and evaluated ([`super::expr`]). Anything that is not a constant numeric expression
//! yields `None`. Also converts a LaTeX formula to a textual expression in `x` for the CAS and the
//! grapher, and formats a result.

use super::expr::compile;
use super::num_fmt::{js_number_to_string, to_precision_str};
use super::tree::{parse, MNode, MRow};

/// `true` when a conversion cannot be evaluated (`mathEval.ts` `NotEvaluable`).
struct NotEvaluable;
type Conv = Result<String, NotEvaluable>;

fn fn_name(t: &str) -> Option<&'static str> {
    Some(match t {
        "\\sin" => "sin",
        "\\cos" => "cos",
        "\\tan" => "tan",
        "\\sec" => "sec",
        "\\csc" => "csc",
        "\\cot" => "cot",
        "\\arcsin" => "asin",
        "\\arccos" => "acos",
        "\\arctan" => "atan",
        "\\sinh" => "sinh",
        "\\cosh" => "cosh",
        "\\tanh" => "tanh",
        "\\ln" => "ln",
        "\\log" => "log",
        "\\lg" => "log",
        "\\exp" => "exp",
        _ => return None,
    })
}

fn const_name(t: &str) -> Option<&'static str> {
    Some(match t {
        "\\pi" => "pi",
        "\\tau" => "tau",
        "\\phi" => "phi",
        "\\varphi" => "phi",
        _ => return None,
    })
}

fn op_name(t: &str) -> Option<&'static str> {
    Some(match t {
        "+" => "+",
        "-" => "-",
        "\\cdot" => "*",
        "\\times" => "*",
        "\\ast" => "*",
        "\\div" => "/",
        "*" => "*",
        "/" => "/",
        _ => return None,
    })
}

fn is_relation(t: &str) -> bool {
    matches!(t, "=" | "<" | ">" | "\\neq" | "\\leq" | "\\geq" | "\\approx" | "\\equiv" | "\\sim" | "\\simeq")
}

fn conv_atom(t: &str) -> Conv {
    if let Some(op) = op_name(t) {
        return Ok(op.to_string());
    }
    if let Some(c) = const_name(t) {
        return Ok(format!(" {c} "));
    }
    if t.len() == 1 {
        let ch = t.chars().next().unwrap();
        if ch.is_ascii_digit() || ch == '.' {
            return Ok(t.to_string());
        }
        if ch.is_ascii_lowercase() {
            return Ok(t.to_string());
        }
        if ch == '(' || ch == ')' || ch == '!' {
            return Ok(t.to_string());
        }
    }
    if t == "," {
        return Ok(".".to_string()); // decimal comma
    }
    Err(NotEvaluable)
}

fn conv_node(n: &MNode) -> Conv {
    match n {
        MNode::Atom(t) => conv_atom(t),
        MNode::Frac { n, d } => Ok(format!("(({})/({}))", conv_row(n)?, conv_row(d)?)),
        MNode::Sqrt { b } => Ok(format!("sqrt({})", conv_row(b)?)),
        MNode::Root { i, b } => Ok(format!("(({})^(1/({})))", conv_row(b)?, conv_row(i)?)),
        MNode::Script { base, sup, sub } => {
            if sub.is_some() || sup.is_none() {
                return Err(NotEvaluable);
            }
            Ok(format!("(({})^({}))", conv_node(base)?, conv_row(sup.as_ref().unwrap())?))
        }
        MNode::Delim { o, c, b } => {
            let inner = conv_row(b)?;
            if o == "|" && c == "|" {
                Ok(format!("abs({inner})"))
            } else {
                Ok(format!("({inner})"))
            }
        }
        _ => Err(NotEvaluable), // big / acc / mat / raw
    }
}

fn conv_row(row: &MRow) -> Conv {
    if row.is_empty() {
        return Err(NotEvaluable);
    }
    let mut out = String::new();
    let mut i = 0usize;
    while i < row.len() {
        let n = &row[i];
        // Function application: a \sin followed by a delimited group, a plain "(", or one operand.
        if let MNode::Atom(t) = n {
            if let Some(fname) = fn_name(t) {
                let next = row.get(i + 1).ok_or(NotEvaluable)?;
                match next {
                    MNode::Delim { b, .. } => {
                        out += &format!("{fname}({})", conv_row(b)?);
                        i += 2;
                        continue;
                    }
                    MNode::Atom(a) if a == "(" => {
                        out += fname; // "(…)" atoms follow
                        i += 1;
                        continue;
                    }
                    _ => {
                        out += &format!("{fname}({})", conv_node(next)?);
                        i += 2;
                        continue;
                    }
                }
            }
        }
        out += &conv_node(n)?;
        i += 1;
    }
    Ok(out)
}

/// Split the top-level row on relation atoms (`"x = 1+2"` → `["x", "1+2"]`).
fn split_on_relations(row: &MRow) -> Vec<MRow> {
    let mut out: Vec<MRow> = Vec::new();
    let mut cur: MRow = Vec::new();
    for n in row {
        if let MNode::Atom(t) = n {
            if is_relation(t) {
                out.push(std::mem::take(&mut cur));
                continue;
            }
        }
        cur.push(n.clone());
    }
    out.push(cur);
    out
}

/// Evaluate a LaTeX formula to a number; `None` unless it is a constant numeric expression.
pub fn eval_latex(latex: &str) -> Option<f64> {
    let src = latex.trim();
    if src.is_empty() || src.contains("\\square") {
        return None;
    }
    let row = parse(src);
    let mut segments = split_on_relations(&row);
    segments.reverse();
    for seg in &segments {
        if seg.is_empty() {
            continue;
        }
        if let Ok(expr) = conv_row(seg) {
            let c = compile(&expr);
            if c.error.is_some() {
                continue;
            }
            let v = c.eval(f64::NAN); // NaN poisons any use of the variable x
            if v.is_finite() {
                return Some(v);
            }
        }
    }
    None
}

/// Convert a LaTeX formula to a plain textual expression in `x` (for the CAS and the grapher).
pub fn latex_to_expr(latex: &str) -> Option<String> {
    let src = latex.trim();
    if src.is_empty() || src.contains("\\square") {
        return None;
    }
    let row = parse(src);
    let segments = split_on_relations(&row);
    let has_x = |seg: &MRow| seg.iter().any(|n| matches!(n, MNode::Atom(t) if t == "x"));
    let ordered: Vec<&MRow> = segments.iter().rev().collect();
    let pick = ordered
        .iter()
        .find(|s| !s.is_empty() && has_x(s))
        .or_else(|| ordered.iter().find(|s| !s.is_empty()))?;
    let expr = conv_row(pick).ok()?;
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Compact display of a result - 12 significant digits, no float noise (`mathEval.ts` `fmtResult`).
pub fn fmt_result(v: f64) -> String {
    if !v.is_finite() {
        return String::new();
    }
    if v.fract() == 0.0 && v.abs() < 1e15 {
        return js_number_to_string(v);
    }
    to_precision_str(v, 12)
}

/// LaTeX form of a result; scientific notation becomes `m \times 10^{k}` (`mathEval.ts`
/// `resultLatex`).
pub fn result_latex(v: f64) -> String {
    let s = fmt_result(v);
    // match /^(-?[\d.]+)e([+-]?\d+)$/
    if let Some(epos) = s.find('e') {
        let (mantissa, exp) = s.split_at(epos);
        let exp = &exp[1..]; // drop 'e'
        let mantissa_ok = !mantissa.is_empty()
            && mantissa.chars().enumerate().all(|(i, c)| c.is_ascii_digit() || c == '.' || (i == 0 && c == '-'));
        let exp_body = exp.strip_prefix(['+', '-']).unwrap_or(exp);
        let exp_ok = !exp_body.is_empty() && exp_body.chars().all(|c| c.is_ascii_digit());
        if mantissa_ok && exp_ok {
            if let Ok(k) = exp.parse::<i64>() {
                return format!("{mantissa} \\times 10^{{{k}}}");
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_constants() {
        assert_eq!(eval_latex("1+2"), Some(3.0));
        assert_eq!(eval_latex("\\frac{1}{2}"), Some(0.5));
        assert!((eval_latex("\\sin(\\pi)").unwrap()).abs() < 1e-12);
        assert_eq!(eval_latex("x = 1+2"), Some(3.0)); // right-to-left: the 1+2 side evaluates
        assert_eq!(eval_latex("x+1"), None); // free variable
        assert_eq!(eval_latex("\\square"), None);
    }

    #[test]
    fn latex_to_expr_picks_x_side() {
        assert_eq!(latex_to_expr("y = x^{2}").as_deref(), Some("((x)^(2))"));
        assert_eq!(latex_to_expr("x+1").as_deref(), Some("x+1"));
    }

    #[test]
    fn formatting() {
        assert_eq!(fmt_result(3.0), "3");
        assert_eq!(fmt_result(0.5), "0.5");
        assert_eq!(fmt_result(1.0 / 3.0), "0.333333333333");
        assert_eq!(result_latex(3.0), "3");
    }
}
