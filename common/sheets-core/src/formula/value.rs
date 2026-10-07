//! The values a formula computes, and JavaScript's coercions between them (`formula-engine.ts`
//! "Valeurs" and "Coercions").
//!
//! A scalar is a number, a string, a boolean or an error; an empty cell reads as the empty string, as on
//! the web. A matrix is rows of scalars, possibly ragged (the web's arrays are); a lambda is a function
//! value that flows through evaluation and surfaces as `#CALC!` when it reaches a cell.

use std::fmt;
use std::rc::Rc;

use crate::js;

use super::ast::Node;

/// An error value (`{ err: '#DIV/0!' }`): its text is its identity.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ErrCode(pub std::borrow::Cow<'static, str>);

impl ErrCode {
    pub const DIV0: ErrCode = ErrCode(std::borrow::Cow::Borrowed("#DIV/0!"));
    pub const VALUE: ErrCode = ErrCode(std::borrow::Cow::Borrowed("#VALUE!"));
    pub const REF: ErrCode = ErrCode(std::borrow::Cow::Borrowed("#REF!"));
    pub const NAME: ErrCode = ErrCode(std::borrow::Cow::Borrowed("#NAME?"));
    pub const NA: ErrCode = ErrCode(std::borrow::Cow::Borrowed("#N/A"));
    pub const NUM: ErrCode = ErrCode(std::borrow::Cow::Borrowed("#NUM!"));
    /// A circular reference: the web reports it as `#REF!`.
    pub const CIRC: ErrCode = ErrCode(std::borrow::Cow::Borrowed("#REF!"));
    pub const SPILL: ErrCode = ErrCode(std::borrow::Cow::Borrowed("#SPILL!"));
    pub const CALC: ErrCode = ErrCode(std::borrow::Cow::Borrowed("#CALC!"));
    pub const NULL: ErrCode = ErrCode(std::borrow::Cow::Borrowed("#NULL!"));

    pub fn new(text: impl Into<String>) -> ErrCode {
        ErrCode(std::borrow::Cow::Owned(text.into()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ErrCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// `Scalar = number | string | boolean | FErr`.
#[derive(Clone, Debug, PartialEq)]
pub enum Scalar {
    Num(f64),
    Str(String),
    Bool(bool),
    Err(ErrCode),
}

impl Scalar {
    /// The empty string: what an empty cell reads as.
    pub fn empty() -> Scalar {
        Scalar::Str(String::new())
    }
    pub fn str(s: impl Into<String>) -> Scalar {
        Scalar::Str(s.into())
    }
    pub fn is_err(&self) -> bool {
        matches!(self, Scalar::Err(_))
    }
    pub fn err(&self) -> Option<&ErrCode> {
        match self {
            Scalar::Err(e) => Some(e),
            _ => None,
        }
    }
    /// `s === '' || s == null`.
    pub fn is_blank(&self) -> bool {
        matches!(self, Scalar::Str(s) if s.is_empty())
    }
    pub fn as_num(&self) -> Option<f64> {
        match self {
            Scalar::Num(n) => Some(*n),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Scalar::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn is_num(&self) -> bool {
        matches!(self, Scalar::Num(_))
    }
    pub fn is_str(&self) -> bool {
        matches!(self, Scalar::Str(_))
    }
    pub fn is_bool(&self) -> bool {
        matches!(self, Scalar::Bool(_))
    }
}

impl From<f64> for Scalar {
    fn from(v: f64) -> Self {
        Scalar::Num(v)
    }
}
impl From<bool> for Scalar {
    fn from(v: bool) -> Self {
        Scalar::Bool(v)
    }
}
impl From<String> for Scalar {
    fn from(v: String) -> Self {
        Scalar::Str(v)
    }
}
impl From<&str> for Scalar {
    fn from(v: &str) -> Self {
        Scalar::Str(v.to_string())
    }
}
impl From<ErrCode> for Scalar {
    fn from(v: ErrCode) -> Self {
        Scalar::Err(v)
    }
}

/// `Matrix = Scalar[][]` (rows may differ in length, as JavaScript arrays may).
pub type Matrix = Vec<Vec<Scalar>>;

/// A first-class function value with its closure.
#[derive(Debug)]
pub struct Lambda {
    pub params: Vec<String>,
    pub body: Rc<Node>,
    pub scope: Rc<super::eval::ScopeChain>,
}

/// What evaluating a node gives.
#[derive(Clone, Debug)]
pub enum Value {
    S(Scalar),
    M(Matrix),
    L(Rc<Lambda>),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::S(a), Value::S(b)) => a == b,
            (Value::M(a), Value::M(b)) => a == b,
            (Value::L(a), Value::L(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl Value {
    pub fn num(n: f64) -> Value {
        Value::S(Scalar::Num(n))
    }
    pub fn str(s: impl Into<String>) -> Value {
        Value::S(Scalar::Str(s.into()))
    }
    pub fn bool(b: bool) -> Value {
        Value::S(Scalar::Bool(b))
    }
    pub fn err(e: ErrCode) -> Value {
        Value::S(Scalar::Err(e))
    }
    pub fn is_matrix(&self) -> bool {
        matches!(self, Value::M(_))
    }
    pub fn is_lambda(&self) -> bool {
        matches!(self, Value::L(_))
    }
    /// `isErr(v)` (a matrix or a lambda is not an error).
    pub fn is_err(&self) -> bool {
        matches!(self, Value::S(Scalar::Err(_)))
    }
    pub fn as_scalar(&self) -> Option<&Scalar> {
        match self {
            Value::S(s) => Some(s),
            _ => None,
        }
    }
    /// The top-left scalar of a matrix, the scalar itself, `#CALC!` for a lambda (`lamScalar` /
    /// the `isMatrix(v) ? v[0]?.[0] ?? '' : v` idiom with `''` for an empty matrix).
    pub fn top_left(&self) -> Scalar {
        match self {
            Value::S(s) => s.clone(),
            Value::M(m) => m.first().and_then(|r| r.first()).cloned().unwrap_or_else(Scalar::empty),
            Value::L(_) => Scalar::Err(ErrCode::CALC),
        }
    }
}

impl From<Scalar> for Value {
    fn from(s: Scalar) -> Self {
        Value::S(s)
    }
}
impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::num(v)
    }
}
impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::bool(v)
    }
}
impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::str(v)
    }
}
impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::str(v)
    }
}
impl From<ErrCode> for Value {
    fn from(v: ErrCode) -> Self {
        Value::err(v)
    }
}
impl From<Matrix> for Value {
    fn from(v: Matrix) -> Self {
        Value::M(v)
    }
}

// ── Coercions ───────────────────────────────────────────────────────────────

/// `toNum`: number as is, boolean 1/0, error kept, `''` → 0, otherwise `Number(s)` (`#VALUE!` on NaN).
pub fn to_num(v: &Scalar) -> Result<f64, ErrCode> {
    match v {
        Scalar::Num(n) => Ok(*n),
        Scalar::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
        Scalar::Err(e) => Err(e.clone()),
        Scalar::Str(s) if s.is_empty() => Ok(0.0),
        Scalar::Str(s) => js::number(s).ok_or(ErrCode::VALUE),
    }
}

/// `toStr`: the error's text, `TRUE`/`FALSE`, `String(x)`.
pub fn to_str(v: &Scalar) -> String {
    match v {
        Scalar::Err(e) => e.as_str().to_string(),
        Scalar::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_string(),
        Scalar::Num(n) => js::to_string(*n),
        Scalar::Str(s) => s.clone(),
    }
}

/// `toBool`: a string is true when it is `true` (any case) or non-empty; an error is false.
pub fn to_bool(v: &Scalar) -> bool {
    match v {
        Scalar::Bool(b) => *b,
        Scalar::Num(n) => *n != 0.0,
        Scalar::Str(s) => s.eq_ignore_ascii_case("true") || !s.is_empty(),
        Scalar::Err(_) => false,
    }
}

/// `flatten`: the scalars of a matrix row by row, or the scalar itself (a lambda flattens to `#CALC!`).
pub fn flatten(v: &Value) -> Vec<Scalar> {
    match v {
        Value::S(s) => vec![s.clone()],
        Value::M(m) => m.iter().flat_map(|r| r.iter().cloned()).collect(),
        Value::L(_) => vec![Scalar::Err(ErrCode::CALC)],
    }
}

/// `asMat`: a matrix, or the value as a 1×1 matrix.
pub fn as_mat(v: Value) -> Matrix {
    match v {
        Value::M(m) => m,
        Value::S(s) => vec![vec![s]],
        Value::L(_) => vec![vec![Scalar::Err(ErrCode::CALC)]],
    }
}

/// `mapValue`: applies `f` to a scalar or to every cell of a matrix.
pub fn map_value(v: Value, f: impl Fn(&Scalar) -> Scalar) -> Value {
    match v {
        Value::S(s) => Value::S(f(&s)),
        Value::M(m) => Value::M(m.iter().map(|r| r.iter().map(&f).collect()).collect()),
        Value::L(_) => Value::S(f(&Scalar::Err(ErrCode::CALC))),
    }
}

/// `broadcast`: element-wise combination with Excel's array broadcasting — a 1×N or N×1 operand is
/// reused across the missing dimension, and a cell out of an operand's range is `#N/A`.
pub fn broadcast(l: &Value, r: &Value, f: impl Fn(&Scalar, &Scalar) -> Scalar) -> Value {
    let one = |v: &Value| -> Matrix {
        match v {
            Value::M(m) => m.clone(),
            other => vec![vec![other.top_left()]],
        }
    };
    let lm = one(l);
    let rm = one(r);
    let rows = lm.len().max(rm.len());
    let width = |m: &Matrix| m.first().map(|r| r.len()).unwrap_or(0);
    let cols = width(&lm).max(width(&rm));
    let pick = |m: &Matrix, i: usize, j: usize| -> Option<Scalar> {
        let row = m.get(if m.len() == 1 { 0 } else { i })?;
        let c = if row.len() == 1 { 0 } else { j };
        row.get(c).cloned()
    };
    let mut out = Vec::with_capacity(rows);
    for i in 0..rows {
        let mut row = Vec::with_capacity(cols);
        for j in 0..cols {
            match (pick(&lm, i, j), pick(&rm, i, j)) {
                (Some(a), Some(b)) => row.push(f(&a, &b)),
                _ => row.push(Scalar::Err(ErrCode::NA)),
            }
        }
        out.push(row);
    }
    Value::M(out)
}

/// `compareScalar`: numbers by difference, everything else by lower-cased text (UTF-16 order).
pub fn compare_scalar(a: &Scalar, b: &Scalar) -> f64 {
    if let (Scalar::Num(x), Scalar::Num(y)) = (a, b) {
        return x - y;
    }
    let (sa, sb) = (to_str(a).to_lowercase(), to_str(b).to_lowercase());
    match js::cmp_str(&sa, &sb) {
        std::cmp::Ordering::Less => -1.0,
        std::cmp::Ordering::Greater => 1.0,
        std::cmp::Ordering::Equal => 0.0,
    }
}

/// The widest row of a matrix (`m.reduce((w, r) => Math.max(w, r.length), 0)`).
pub fn width(m: &Matrix) -> usize {
    m.iter().map(|r| r.len()).max().unwrap_or(0)
}

/// `formatValue`: a value as a cell displays it — the top-left of a matrix, the error's text,
/// `TRUE`/`FALSE`, a number rounded to 10 decimals (`#NUM!` when not finite), the string.
pub fn format_value(v: &Value) -> String {
    let s = match v {
        Value::L(_) => return ErrCode::CALC.as_str().to_string(),
        other => other.top_left(),
    };
    format_scalar(&s)
}

/// [`format_value`] of a scalar.
pub fn format_scalar(s: &Scalar) -> String {
    match s {
        Scalar::Err(e) => e.as_str().to_string(),
        Scalar::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_string(),
        Scalar::Num(n) => {
            if !n.is_finite() {
                ErrCode::NUM.as_str().to_string()
            } else {
                js::to_string(js::round(n * 1e10) / 1e10)
            }
        }
        Scalar::Str(s) => s.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coercions_follow_the_engine() {
        assert_eq!(to_num(&Scalar::str(" 4 ")), Ok(4.0));
        assert_eq!(to_num(&Scalar::str("x")), Err(ErrCode::VALUE));
        assert_eq!(to_num(&Scalar::empty()), Ok(0.0));
        assert_eq!(to_str(&Scalar::Num(0.1 + 0.2)), "0.30000000000000004");
        assert!(to_bool(&Scalar::str("non")));
        assert!(!to_bool(&Scalar::empty()));
        assert!(to_bool(&Scalar::str("TRUE")));
    }

    #[test]
    fn broadcast_reuses_a_row_or_column() {
        let col = Value::M(vec![vec![Scalar::Num(1.0)], vec![Scalar::Num(2.0)]]);
        let row = Value::M(vec![vec![Scalar::Num(10.0), Scalar::Num(20.0)]]);
        let r = broadcast(&col, &row, |a, b| Scalar::Num(a.as_num().unwrap_or(0.0) + b.as_num().unwrap_or(0.0)));
        assert_eq!(r, Value::M(vec![vec![Scalar::Num(11.0), Scalar::Num(21.0)], vec![Scalar::Num(12.0), Scalar::Num(22.0)]]));
        let a = Value::M(vec![vec![Scalar::Num(1.0), Scalar::Num(2.0), Scalar::Num(3.0)]]);
        let b = Value::M(vec![vec![Scalar::Num(1.0), Scalar::Num(2.0)]]);
        let r = broadcast(&a, &b, |x, _| x.clone());
        assert_eq!(flatten(&r)[2], Scalar::Err(ErrCode::NA));
    }

    #[test]
    fn values_display_like_the_web() {
        assert_eq!(format_value(&Value::num(1.0 / 3.0)), "0.3333333333");
        assert_eq!(format_value(&Value::num(f64::INFINITY)), "#NUM!");
        assert_eq!(format_value(&Value::bool(true)), "TRUE");
        assert_eq!(format_value(&Value::M(vec![])), "");
    }
}
