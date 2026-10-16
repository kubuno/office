//! Port of `office/web/src/mathExpr.ts` - a tiny zero-dependency expression compiler. It turns a
//! textual expression like `sin(x) + x^2` into an AST and evaluates it for a given `x`. Tolerant
//! recursive-descent parser; supports `+ - * / ^`, unary minus, parentheses, implicit
//! multiplication, the variable `x`, the constants pi/e/tau/phi and common functions. Invalid input
//! yields an error (and, via [`compile`], a function returning NaN).

use std::f64::consts::{E, PI};

/// Expression AST node (`mathExpr.ts` `Node`).
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Num(f64),
    Var,
    Neg(Box<Node>),
    Bin { op: char, a: Box<Node>, b: Box<Node> },
    Call { name: String, args: Vec<Node> },
}

/// Integer factorial (NaN outside 0..=170 or for non-integers, like a calculator).
fn factorial(n: f64) -> f64 {
    if n.fract() != 0.0 || n < 0.0 || n > 170.0 {
        return f64::NAN;
    }
    let mut r = 1.0;
    let mut i = 2.0;
    while i <= n {
        r *= i;
        i += 1.0;
    }
    r
}

/// JS `Math.sign` (differs from `f64::signum`: sign(0) == 0, sign(NaN) == NaN).
fn js_sign(x: f64) -> f64 {
    if x.is_nan() {
        f64::NAN
    } else if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        x
    }
}

/// JS `Math.round` (rounds half toward +∞, unlike `f64::round` which rounds half away from zero).
fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}
fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.max(b)
    }
}

fn is_unary(name: &str) -> bool {
    matches!(
        name,
        "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "sec" | "csc" | "cot" | "sinh" | "cosh"
            | "tanh" | "asinh" | "acosh" | "atanh" | "exp" | "ln" | "log" | "log10" | "log2"
            | "sqrt" | "cbrt" | "abs" | "floor" | "ceil" | "round" | "sign" | "trunc" | "fact"
    )
}
fn is_binary(name: &str) -> bool {
    matches!(name, "pow" | "atan2" | "min" | "max" | "mod" | "log")
}

fn apply_unary(name: &str, a: f64) -> f64 {
    match name {
        "sin" => a.sin(),
        "cos" => a.cos(),
        "tan" => a.tan(),
        "asin" => a.asin(),
        "acos" => a.acos(),
        "atan" => a.atan(),
        "sec" => 1.0 / a.cos(),
        "csc" => 1.0 / a.sin(),
        "cot" => 1.0 / a.tan(),
        "sinh" => a.sinh(),
        "cosh" => a.cosh(),
        "tanh" => a.tanh(),
        "asinh" => a.asinh(),
        "acosh" => a.acosh(),
        "atanh" => a.atanh(),
        "exp" => a.exp(),
        "ln" => a.ln(),
        "log" | "log10" => a.log10(),
        "log2" => a.log2(),
        "sqrt" => a.sqrt(),
        "cbrt" => a.cbrt(),
        "abs" => a.abs(),
        "floor" => a.floor(),
        "ceil" => a.ceil(),
        "round" => js_round(a),
        "sign" => js_sign(a),
        "trunc" => a.trunc(),
        "fact" => factorial(a),
        _ => f64::NAN,
    }
}

fn apply_binary(name: &str, a: f64, b: f64) -> f64 {
    match name {
        "pow" => a.powf(b),
        "atan2" => a.atan2(b),
        "min" => js_min(a, b),
        "max" => js_max(a, b),
        "mod" => a - b * (a / b).floor(),
        "log" => b.ln() / a.ln(), // log(base, value)
        _ => f64::NAN,
    }
}

fn const_value(name: &str) -> Option<f64> {
    match name {
        "pi" => Some(PI),
        "e" => Some(E),
        "tau" => Some(PI * 2.0),
        "phi" => Some((1.0 + 5.0_f64.sqrt()) / 2.0),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Id(String),
    Op(char),
    P(char), // '(' ')' ','
}

fn tokenize(src: &str) -> Result<Vec<Tok>, String> {
    // Normalise common unicode operators so pasted formulas still parse.
    let s: String = src
        .replace(['×', '·', '∗'], "*")
        .replace(['−', '–', '\u{2014}'], "-")
        .replace('÷', "/")
        .replace('π', "pi");
    let chars: Vec<char> = s.chars().collect();
    let mut toks = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c == ' ' || c == '\t' || c == '\n' {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || c == '.' {
            let mut j = i + 1;
            while j < chars.len() && (chars[j].is_ascii_digit() || chars[j] == '.') {
                j += 1;
            }
            // exponent form 1e-3
            if j < chars.len() && chars[j] == 'e' {
                let nxt = chars.get(j + 1).copied().unwrap_or(' ');
                if nxt.is_ascii_digit() || nxt == '+' || nxt == '-' {
                    j += 1;
                    if j < chars.len() && (chars[j] == '+' || chars[j] == '-') {
                        j += 1;
                    }
                    while j < chars.len() && chars[j].is_ascii_digit() {
                        j += 1;
                    }
                }
            }
            let slice: String = chars[i..j].iter().collect();
            toks.push(Tok::Num(slice.parse().unwrap_or(f64::NAN)));
            i = j;
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let mut j = i + 1;
            while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                j += 1;
            }
            toks.push(Tok::Id(chars[i..j].iter().collect()));
            i = j;
            continue;
        }
        if "+-*/^!".contains(c) {
            toks.push(Tok::Op(c));
            i += 1;
            continue;
        }
        if c == '(' || c == ')' || c == ',' {
            toks.push(Tok::P(c));
            i += 1;
            continue;
        }
        return Err(format!("Caractère inattendu « {c} »"));
    }
    Ok(toks)
}

struct Parser {
    toks: Vec<Tok>,
    p: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.p)
    }
    fn is_factor_start(tk: Option<&Tok>) -> bool {
        matches!(tk, Some(Tok::Num(_)) | Some(Tok::Id(_)) | Some(Tok::P('(')))
    }

    // expr := term (('+'|'-') term)*
    fn parse_expr(&mut self) -> Result<Node, String> {
        let mut left = self.parse_term()?;
        while let Some(Tok::Op(op @ ('+' | '-'))) = self.peek() {
            let op = *op;
            self.p += 1;
            let b = self.parse_term()?;
            left = Node::Bin { op, a: Box::new(left), b: Box::new(b) };
        }
        Ok(left)
    }
    // term := factor (('*'|'/'| implicit) factor)*
    fn parse_term(&mut self) -> Result<Node, String> {
        let mut left = self.parse_factor()?;
        loop {
            match self.peek() {
                Some(Tok::Op(op @ ('*' | '/'))) => {
                    let op = *op;
                    self.p += 1;
                    let b = self.parse_factor()?;
                    left = Node::Bin { op, a: Box::new(left), b: Box::new(b) };
                }
                tk if Self::is_factor_start(tk) => {
                    let b = self.parse_factor()?;
                    left = Node::Bin { op: '*', a: Box::new(left), b: Box::new(b) };
                }
                _ => break,
            }
        }
        Ok(left)
    }
    // factor := base '!'* ('^' factor)?  ('^' right associative, '!' postfix)
    fn parse_factor(&mut self) -> Result<Node, String> {
        let mut base = self.parse_base()?;
        while let Some(Tok::Op('!')) = self.peek() {
            self.p += 1;
            base = Node::Call { name: "fact".into(), args: vec![base] };
        }
        if let Some(Tok::Op('^')) = self.peek() {
            self.p += 1;
            let b = self.parse_factor()?;
            return Ok(Node::Bin { op: '^', a: Box::new(base), b: Box::new(b) });
        }
        Ok(base)
    }
    fn parse_base(&mut self) -> Result<Node, String> {
        let tk = self.peek().cloned();
        match tk {
            None => Err("Expression incomplète".into()),
            Some(Tok::Op('-')) => {
                self.p += 1;
                Ok(Node::Neg(Box::new(self.parse_base()?)))
            }
            Some(Tok::Op('+')) => {
                self.p += 1;
                self.parse_base()
            }
            Some(Tok::Num(v)) => {
                self.p += 1;
                Ok(Node::Num(v))
            }
            Some(Tok::P('(')) => {
                self.p += 1;
                let e = self.parse_expr()?;
                self.expect_close()?;
                Ok(e)
            }
            Some(Tok::Id(id)) => {
                self.p += 1;
                let name = id.to_lowercase();
                if let Some(Tok::P('(')) = self.peek() {
                    // function call
                    self.p += 1;
                    let mut args = Vec::new();
                    if !matches!(self.peek(), Some(Tok::P(')'))) {
                        args.push(self.parse_expr()?);
                        while let Some(Tok::P(',')) = self.peek() {
                            self.p += 1;
                            args.push(self.parse_expr()?);
                        }
                    }
                    self.expect_close()?;
                    if !is_unary(&name) && !is_binary(&name) {
                        return Err(format!("Fonction inconnue « {id} »"));
                    }
                    return Ok(Node::Call { name, args });
                }
                if name == "x" {
                    return Ok(Node::Var);
                }
                if let Some(v) = const_value(&name) {
                    return Ok(Node::Num(v));
                }
                Err(format!("Symbole inconnu « {id} »"))
            }
            Some(_) => Err("Expression invalide".into()),
        }
    }
    fn expect_close(&mut self) -> Result<(), String> {
        if let Some(Tok::P(')')) = self.peek() {
            self.p += 1;
            Ok(())
        } else {
            Err("Parenthèse fermante manquante".into())
        }
    }
}

/// Parse a textual expression into its AST (`mathExpr.ts` `parseExpr` / `buildAst`).
pub fn parse_expr(src: &str) -> Result<Node, String> {
    let toks = tokenize(src)?;
    let mut parser = Parser { toks, p: 0 };
    let ast = parser.parse_expr()?;
    if parser.p < parser.toks.len() {
        return Err("Texte en trop dans l'expression".into());
    }
    Ok(ast)
}

/// Evaluate an AST at a given `x` (`mathExpr.ts` `evalNode`).
pub fn eval_node(n: &Node, x: f64) -> f64 {
    match n {
        Node::Num(v) => *v,
        Node::Var => x,
        Node::Neg(a) => -eval_node(a, x),
        Node::Bin { op, a, b } => {
            let a = eval_node(a, x);
            let b = eval_node(b, x);
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
            let a: Vec<f64> = args.iter().map(|arg| eval_node(arg, x)).collect();
            if args.len() == 2 && is_binary(name) {
                return apply_binary(name, a[0], a[1]);
            }
            if is_unary(name) {
                return apply_unary(name, a.first().copied().unwrap_or(f64::NAN));
            }
            if is_binary(name) {
                return apply_binary(name, a.first().copied().unwrap_or(f64::NAN), a.get(1).copied().unwrap_or(f64::NAN));
            }
            f64::NAN
        }
    }
}

/// A compiled expression (`mathExpr.ts` `Compiled`): the AST (if it parsed) and the parse error.
#[derive(Clone, Debug)]
pub struct Compiled {
    pub ast: Option<Node>,
    pub error: Option<String>,
}

impl Compiled {
    /// Evaluate at `x`, returning NaN when the expression did not parse.
    pub fn eval(&self, x: f64) -> f64 {
        match &self.ast {
            Some(n) => eval_node(n, x),
            None => f64::NAN,
        }
    }
}

/// Compile a textual expression (`mathExpr.ts` `compile`). An empty expression is `error: None`
/// with an AST that evaluates to NaN.
pub fn compile(expr: &str) -> Compiled {
    let src = expr.trim();
    if src.is_empty() {
        return Compiled { ast: None, error: None };
    }
    match parse_expr(src) {
        Ok(ast) => Compiled { ast: Some(ast), error: None },
        Err(e) => Compiled { ast: None, error: Some(e) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(src: &str, x: f64) -> f64 {
        compile(src).eval(x)
    }

    #[test]
    fn arithmetic_and_precedence() {
        assert_eq!(ev("1 + 2 * 3", 0.0), 7.0);
        assert_eq!(ev("2 ^ 3 ^ 2", 0.0), 512.0); // right associative
        assert_eq!(ev("-2^2", 0.0), 4.0); // unary minus binds below ^ via parse_base
        assert_eq!(ev("(1 + 2) * 3", 0.0), 9.0);
    }

    #[test]
    fn implicit_multiplication_and_vars() {
        assert_eq!(ev("2x", 3.0), 6.0);
        assert_eq!(ev("2(x+1)", 3.0), 8.0);
        // QUIRK (matches the web): an identifier followed by '(' is a function call, so `x(x+1)`
        // is "call function x", not implicit multiplication → unknown function → NaN.
        assert!(ev("x(x+1)", 3.0).is_nan());
    }

    #[test]
    fn functions_and_consts() {
        assert!((ev("sin(pi/2)", 0.0) - 1.0).abs() < 1e-12);
        assert!((ev("log(8)", 0.0) - (8f64).log10()).abs() < 1e-12);
        assert_eq!(ev("log(2, 8)", 0.0), 3.0); // log base 2 of 8
        assert_eq!(ev("5!", 0.0), 120.0);
        assert_eq!(ev("max(3, 7)", 0.0), 7.0);
    }

    #[test]
    fn errors() {
        assert_eq!(compile("(1+2").error.as_deref(), Some("Parenthèse fermante manquante"));
        assert_eq!(compile("sin(").error.as_deref(), Some("Expression incomplète"));
        assert_eq!(compile("foo(x)").error.as_deref(), Some("Fonction inconnue « foo »"));
        assert_eq!(compile("@").error.as_deref(), Some("Caractère inattendu « @ »"));
    }
}
