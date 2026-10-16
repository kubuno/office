//! The evaluator (`formula-engine.ts` `Evaluator` and `evaluate`), rule for rule.
//!
//! A cell's value is computed on demand, recursively: a reference to a formula cell evaluates that
//! formula (without the spill index, as on the web), a cell reached again while it is being computed is
//! circular and reads `#REF!`. Two things are added that the web does not need to say and the desktop
//! does:
//!
//! * **a memo**: within one [`Ctx`] (one recalculation over unchanged data), the value a formula cell
//!   gives when referenced is computed once. A result is remembered only when its computation met no
//!   circularity and no depth limit - the only ways the web's result depends on the path that reached
//!   the cell - so the values are the web's. (The web recomputes a cell at every reference, which is
//!   exponential on diamond-shaped dependencies; the results are the same.)
//! * **a depth limit**: the web catches the JavaScript stack overflow of a too-deep chain and shows
//!   `#VALUE!`; native code would abort, so the chain stops at [`MAX_DEPTH`] with `#VALUE!` instead.

use std::cell::{Cell as StdCell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use super::ast::{BinOp, Node};
use super::parser::parse_formula;
use super::value::{broadcast, flatten, map_value, to_bool, to_num, to_str, ErrCode, Lambda, Matrix, Scalar, Value};
use crate::addr;
use crate::js;
use crate::jsdate::LocalNow;
use crate::model::cell::Cell;

/// A sheet's cells by `A1` key.
pub type CellMap = HashMap<String, Cell>;

/// How deep nested cell evaluations and lambda applications may go.
pub const MAX_DEPTH: u32 = 1000;

/// What the engine reads from the outside: the local time and a random source (`new Date()`,
/// `Math.random()`). The core never reads a clock itself.
pub trait Env {
    fn now(&self) -> LocalNow;
    /// A number in [0, 1).
    fn random(&self) -> f64;
}

/// A fixed time and a deterministic random sequence (tests, previews).
pub struct FixedEnv {
    pub now: LocalNow,
    seed: StdCell<u64>,
}

impl FixedEnv {
    pub fn new(now: LocalNow) -> Self {
        Self { now, seed: StdCell::new(0x2545_F491_4F6C_DD1D) }
    }
}

impl Env for FixedEnv {
    fn now(&self) -> LocalNow {
        self.now
    }
    fn random(&self) -> f64 {
        // xorshift64*
        let mut x = self.seed.get();
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.seed.set(x);
        (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// What a formula is evaluated against (the engine's view of `SheetData`).
#[derive(Clone, Copy)]
pub struct Data<'a> {
    /// The sheet's own cells.
    pub cells: &'a CellMap,
    /// Every sheet's cells by name (only when the workbook has several sheets; the active sheet is
    /// in it under its own name).
    pub sheets: Option<&'a HashMap<String, &'a CellMap>>,
    /// Workbook-level defined names (upper-case name → formula text).
    pub names: &'a HashMap<String, String>,
}

/// One spilled anchor (`SpillIndex.anchors[key]`).
#[derive(Clone, Debug, PartialEq)]
pub struct SpillAnchor {
    pub matrix: Matrix,
    pub rows: usize,
    pub cols: usize,
    pub blocked: bool,
}

/// `SpillIndex`: the values spilled into empty cells, the anchors, and which anchor covers which cell.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpillIndex {
    pub values: HashMap<String, Scalar>,
    pub anchors: HashMap<String, SpillAnchor>,
    pub origin: HashMap<String, String>,
}

/// One recalculation's shared state: the environment, the parsed formulas, the memo.
pub struct Ctx<'e> {
    pub env: &'e dyn Env,
    asts: RefCell<HashMap<String, Result<Rc<Node>, ErrCode>>>,
    memo: RefCell<HashMap<(usize, String), Scalar>>,
    depth: StdCell<u32>,
    /// Bumped whenever a result depends on how it was reached (a circular hit, the depth limit).
    tainted: StdCell<u64>,
}

impl<'e> Ctx<'e> {
    pub fn new(env: &'e dyn Env) -> Self {
        Self { env, asts: RefCell::default(), memo: RefCell::default(), depth: StdCell::new(0), tainted: StdCell::new(0) }
    }

    fn ast(&self, formula: &str) -> Result<Rc<Node>, ErrCode> {
        if let Some(a) = self.asts.borrow().get(formula) {
            return a.clone();
        }
        let a = parse_formula(formula).map(Rc::new);
        self.asts.borrow_mut().insert(formula.to_string(), a.clone());
        a
    }

    fn taint(&self) {
        self.tainted.set(self.tainted.get() + 1);
    }

    /// Forgets the memo (the data changed) but keeps the parsed formulas.
    pub fn invalidate(&self) {
        self.memo.borrow_mut().clear();
    }
}

/// LET / LAMBDA bindings: a chain of frames, the innermost first (`{ ...outer, ...inner }`).
#[derive(Debug, Default)]
pub struct ScopeChain {
    vars: HashMap<String, Value>,
    parent: Option<Rc<ScopeChain>>,
}

impl ScopeChain {
    fn get(&self, name: &str) -> Option<Value> {
        match self.vars.get(name) {
            Some(v) => Some(v.clone()),
            None => self.parent.as_ref().and_then(|p| p.get(name)),
        }
    }
    fn extend(parent: &Rc<ScopeChain>, vars: HashMap<String, Value>) -> Rc<ScopeChain> {
        Rc::new(ScopeChain { vars, parent: Some(parent.clone()) })
    }
}

/// A function of the registry: it receives its argument nodes unevaluated (laziness is per
/// function, as on the web) and returns a value or throws an error value.
pub type Fn = for<'a, 'b> fn(&'b Evaluator<'a>, &'b [Node]) -> Result<Value, ErrCode>;

/// Evaluates nodes against data (`class Evaluator`).
pub struct Evaluator<'a> {
    pub data: Data<'a>,
    pub ctx: &'a Ctx<'a>,
    visiting: &'a RefCell<HashSet<String>>,
    pub spill: Option<&'a SpillIndex>,
    scope: Rc<ScopeChain>,
    names_cache: Rc<RefCell<HashMap<String, Value>>>,
    resolving: Rc<RefCell<HashSet<String>>>,
}

/// The text a lambda reads as where the web coerces it as an object (`String(lambda)`).
const OBJECT_TEXT: &str = "[object Object]";

/// A value as a scalar where the web takes `isMatrix(v) ? v[0]?.[0] ?? '' : v` (a lambda is an object).
pub fn scalar_of(v: &Value) -> Scalar {
    match v {
        Value::S(s) => s.clone(),
        Value::M(m) => m.first().and_then(|r| r.first()).cloned().unwrap_or_else(Scalar::empty),
        Value::L(_) => Scalar::str(OBJECT_TEXT),
    }
}

/// `args[i]`, `#VALUE!` when the formula omitted it (the web dereferences `undefined` and catches the
/// TypeError as `#VALUE!`).
pub fn arg(args: &[Node], i: usize) -> Result<&Node, ErrCode> {
    args.get(i).ok_or(ErrCode::VALUE)
}

impl<'a> Evaluator<'a> {
    fn new(data: Data<'a>, ctx: &'a Ctx<'a>, visiting: &'a RefCell<HashSet<String>>, spill: Option<&'a SpillIndex>) -> Self {
        Self {
            data,
            ctx,
            visiting,
            spill,
            scope: Rc::new(ScopeChain::default()),
            names_cache: Rc::default(),
            resolving: Rc::default(),
        }
    }

    fn child(&self, scope: Rc<ScopeChain>) -> Evaluator<'a> {
        Evaluator {
            data: self.data,
            ctx: self.ctx,
            visiting: self.visiting,
            spill: self.spill,
            scope,
            names_cache: self.names_cache.clone(),
            resolving: self.resolving.clone(),
        }
    }

    /// `withScope(extra)`: a child evaluator with more names bound.
    pub fn with_scope(&self, extra: HashMap<String, Value>) -> Evaluator<'a> {
        self.child(ScopeChain::extend(&self.scope, extra))
    }

    /// `makeLambda(params, body)`: a lambda closing over the current scope.
    pub fn make_lambda(&self, params: Vec<String>, body: &Node) -> Value {
        Value::L(Rc::new(Lambda { params, body: Rc::new(body.clone()), scope: self.scope.clone() }))
    }

    /// The local time (`new Date()`).
    pub fn now(&self) -> LocalNow {
        self.ctx.env.now()
    }

    /// `Math.random()`.
    pub fn random(&self) -> f64 {
        self.ctx.env.random()
    }

    /// `resolveName(name)`: a workbook defined name, evaluated once per evaluator tree in a clean scope
    /// (it still sees the other names and itself, so a named LAMBDA may recurse).
    pub fn resolve_name(&self, name: &str) -> Option<Value> {
        let def = self.data.names.get(name)?;
        if let Some(v) = self.names_cache.borrow().get(name) {
            return Some(v.clone());
        }
        if self.resolving.borrow().contains(name) {
            self.ctx.taint();
            return Some(Value::err(ErrCode::CIRC));
        }
        self.resolving.borrow_mut().insert(name.to_string());
        let result = match self.ctx.ast(def) {
            Err(e) => Value::err(e),
            Ok(ast) => {
                let e = self.child(Rc::new(ScopeChain::default()));
                e.eval(&ast)
            }
        };
        self.resolving.borrow_mut().remove(name);
        self.names_cache.borrow_mut().insert(name.to_string(), result.clone());
        Some(result)
    }

    /// `applyLambda(fn, args)`.
    pub fn apply_lambda(&self, f: &Lambda, args: Vec<Value>) -> Value {
        if args.len() != f.params.len() {
            return Value::err(ErrCode::VALUE);
        }
        let depth = self.ctx.depth.get();
        if depth >= MAX_DEPTH {
            self.ctx.taint();
            return Value::err(ErrCode::VALUE);
        }
        self.ctx.depth.set(depth + 1);
        let vars: HashMap<String, Value> = f.params.iter().cloned().zip(args).collect();
        let child = self.child(ScopeChain::extend(&f.scope, vars));
        let r = child.eval(&f.body);
        self.ctx.depth.set(depth);
        r
    }

    /// `cellRaw(ref)`: the stored cell of the own sheet.
    pub fn cell_raw(&self, key: &str) -> Option<&'a Cell> {
        self.data.cells.get(key)
    }

    fn cells_of(&self, sheet: Option<&str>) -> Option<&'a CellMap> {
        match sheet {
            None => Some(self.data.cells),
            Some(name) => self.data.sheets.and_then(|s| s.get(name).copied()),
        }
    }

    /// `cellValue(ref, sheet)`: a cell's value as a reference reads it.
    pub fn cell_value(&self, r: &str, sheet: Option<&str>) -> Scalar {
        let Some(cells) = self.cells_of(sheet) else { return Scalar::Err(ErrCode::REF) };
        let vkey = match sheet {
            None => r.to_string(),
            Some(s) => format!("{s}!{r}"),
        };
        if self.visiting.borrow().contains(&vkey) {
            self.ctx.taint();
            return Scalar::Err(ErrCode::CIRC);
        }
        let Some(cell) = cells.get(r) else {
            if sheet.is_none() {
                if let Some(v) = self.spill.and_then(|s| s.values.get(r)) {
                    return v.clone();
                }
            }
            return Scalar::empty();
        };
        if let Some(f) = cell.formula() {
            let mkey = (cells as *const CellMap as usize, r.to_string());
            if let Some(v) = self.ctx.memo.borrow().get(&mkey) {
                return v.clone();
            }
            let before = self.ctx.tainted.get();
            self.visiting.borrow_mut().insert(vkey.clone());
            let data = match sheet {
                None => self.data,
                Some(_) => Data { cells, sheets: self.data.sheets, names: self.data.names },
            };
            let v = evaluate_in(f, data, self.visiting, None, self.ctx);
            self.visiting.borrow_mut().remove(&vkey);
            let s = match v {
                Value::M(m) => m.first().and_then(|row| row.first()).cloned().unwrap_or_else(Scalar::empty),
                Value::S(s) => s,
                Value::L(_) => Scalar::Err(ErrCode::CALC),
            };
            if self.ctx.tainted.get() == before {
                self.ctx.memo.borrow_mut().insert(mkey, s.clone());
            }
            return s;
        }
        cell.raw_scalar()
    }

    /// `range(a, b, sheet)`: the rectangle's values row by row.
    pub fn range(&self, a: &str, b: &str, sheet: Option<&str>) -> Matrix {
        if self.cells_of(sheet).is_none() {
            return vec![vec![Scalar::Err(ErrCode::REF)]];
        }
        let (Some(ca), Some(cb)) = (addr::Cell::parse(a), addr::Cell::parse(b)) else {
            return vec![vec![Scalar::Err(ErrCode::REF)]];
        };
        let (lo, hi) = (ca.col.min(cb.col), ca.col.max(cb.col));
        let (tr, br) = (ca.row.min(cb.row), ca.row.max(cb.row));
        let mut out = Vec::with_capacity((br - tr + 1) as usize);
        for r in tr..=br {
            let mut row = Vec::with_capacity((hi - lo + 1) as usize);
            for c in lo..=hi {
                row.push(self.cell_value(&addr::key(c, r), sheet));
            }
            out.push(row);
        }
        out
    }

    /// `eval(node)`.
    pub fn eval(&self, node: &Node) -> Value {
        match node {
            Node::Num(v) => Value::num(*v),
            Node::Str(s) => Value::str(s.clone()),
            Node::Bool(b) => Value::bool(*b),
            Node::Ref { r, sheet } => Value::S(self.cell_value(r, sheet.as_deref())),
            Node::Range { a, b, sheet } => Value::M(self.range(a, b, sheet.as_deref())),
            Node::Unary { op, e } => {
                let neg = *op == '-';
                map_value(lambda_as_object(self.eval(e)), |s| match to_num(s) {
                    Ok(n) => Scalar::Num(if neg { -n } else { n }),
                    Err(e) => Scalar::Err(e),
                })
            }
            Node::Postfix { e, .. } => map_value(lambda_as_object(self.eval(e)), |s| match to_num(s) {
                Ok(n) => Scalar::Num(n / 100.0),
                Err(e) => Scalar::Err(e),
            }),
            Node::Bin { op, l, r } => self.binary(*op, l, r),
            Node::Call { name, args } => self.call(name, args),
            Node::Array(rows) => self.array(rows),
            Node::SpillRef(r) => match self.spill.and_then(|s| s.anchors.get(r)) {
                Some(a) if !a.blocked => Value::M(a.matrix.clone()),
                _ => Value::err(ErrCode::REF),
            },
            Node::Var(name) => {
                if let Some(v) = self.scope.get(name) {
                    return v;
                }
                self.resolve_name(name).unwrap_or(Value::err(ErrCode::NAME))
            }
            Node::Apply { f, args } => {
                let fv = self.eval(f);
                let Value::L(lam) = fv else { return Value::err(ErrCode::CALC) };
                let vals = args.iter().map(|a| self.eval(a)).collect();
                self.apply_lambda(&lam, vals)
            }
        }
    }

    /// `{1,2;3,4}` → a rectangular matrix (short rows padded with `#N/A`).
    fn array(&self, rows: &[Vec<Node>]) -> Value {
        let width = rows.iter().map(Vec::len).max().unwrap_or(0);
        if width == 0 {
            return Value::err(ErrCode::VALUE);
        }
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let mut r: Vec<Scalar> = row.iter().map(|c| self.scalar(c)).collect();
            while r.len() < width {
                r.push(Scalar::Err(ErrCode::NA));
            }
            out.push(r);
        }
        Value::M(out)
    }

    /// `scalar(node)`: the node's value, the top-left cell of a matrix.
    pub fn scalar(&self, node: &Node) -> Scalar {
        scalar_of(&self.eval(node))
    }

    fn binary(&self, op: BinOp, l: &Node, r: &Node) -> Value {
        let lv = lambda_as_object(self.eval(l));
        let rv = lambda_as_object(self.eval(r));
        if lv.is_matrix() || rv.is_matrix() {
            return broadcast(&lv, &rv, |a, b| bin_scalar(op, a, b));
        }
        Value::S(bin_scalar(op, &scalar_of(&lv), &scalar_of(&rv)))
    }

    /// `num(ev, node)`: the node as a number (its error, or `#VALUE!` for text that is not one).
    pub fn num(&self, node: &Node) -> Result<f64, ErrCode> {
        let s = self.scalar(node);
        if let Scalar::Err(e) = s {
            return Err(e);
        }
        to_num(&s)
    }

    /// `str(ev, node)`: the node as text.
    pub fn str(&self, node: &Node) -> String {
        to_str(&self.scalar(node))
    }

    /// `nums(args)`: the numbers of the arguments (ranges or scalars): numbers, booleans as 1/0 and
    /// numeric strings; blanks and other text skipped; the first error returned.
    pub fn nums(&self, args: &[Node]) -> Result<Vec<f64>, ErrCode> {
        let mut out = Vec::new();
        for a in args {
            for s in flatten(&lambda_as_object(self.eval(a))) {
                match s {
                    Scalar::Err(e) => return Err(e),
                    Scalar::Num(n) => out.push(n),
                    Scalar::Bool(b) => out.push(if b { 1.0 } else { 0.0 }),
                    Scalar::Str(t) => {
                        if !t.is_empty() {
                            if let Some(n) = js::number(&t) {
                                out.push(n);
                            }
                        }
                    }
                }
            }
        }
        Ok(out)
    }

    /// `call(name, args)`: a built-in, else a bound or defined LAMBDA applied, else `#NAME?`.
    pub fn call(&self, name: &str, args: &[Node]) -> Value {
        if let Some(f) = super::functions::registry().get(name) {
            return f(self, args).unwrap_or_else(Value::err);
        }
        let bound = self.scope.get(name).or_else(|| self.resolve_name(name));
        if let Some(Value::L(lam)) = bound {
            let vals = args.iter().map(|a| self.eval(a)).collect();
            return self.apply_lambda(&lam, vals);
        }
        Value::err(ErrCode::NAME)
    }
}

/// Where the web treats a lambda as a plain object (operators, `flatten`), it reads as text.
fn lambda_as_object(v: Value) -> Value {
    match v {
        Value::L(_) => Value::str(OBJECT_TEXT),
        other => other,
    }
}

/// `binScalar(op, L, R)`.
pub fn bin_scalar(op: BinOp, l: &Scalar, r: &Scalar) -> Scalar {
    if let Scalar::Err(e) = l {
        return Scalar::Err(e.clone());
    }
    if let Scalar::Err(e) = r {
        return Scalar::Err(e.clone());
    }
    if op == BinOp::Concat {
        return Scalar::Str(to_str(l) + &to_str(r));
    }
    if op.is_comparison() {
        let cmp: f64 = match (l, r) {
            (Scalar::Num(a), Scalar::Num(b)) => a - b,
            _ if l.is_bool() || r.is_bool() => (to_bool(l) as i32 - to_bool(r) as i32) as f64,
            _ => {
                let (a, b) = (to_str(l).to_lowercase(), to_str(r).to_lowercase());
                match js::cmp_str(&a, &b) {
                    std::cmp::Ordering::Less => -1.0,
                    std::cmp::Ordering::Greater => 1.0,
                    std::cmp::Ordering::Equal => 0.0,
                }
            }
        };
        return Scalar::Bool(match op {
            BinOp::Eq => cmp == 0.0,
            BinOp::Ne => cmp != 0.0,
            BinOp::Lt => cmp < 0.0,
            BinOp::Gt => cmp > 0.0,
            BinOp::Le => cmp <= 0.0,
            _ => cmp >= 0.0,
        });
    }
    let a = match to_num(l) {
        Ok(a) => a,
        Err(e) => return Scalar::Err(e),
    };
    let b = match to_num(r) {
        Ok(b) => b,
        Err(e) => return Scalar::Err(e),
    };
    Scalar::Num(match op {
        BinOp::Add => a + b,
        BinOp::Sub => a - b,
        BinOp::Mul => a * b,
        BinOp::Div => {
            if b == 0.0 {
                return Scalar::Err(ErrCode::DIV0);
            }
            a / b
        }
        _ => js::pow(a, b),
    })
}

fn evaluate_in(formula: &str, data: Data<'_>, visiting: &RefCell<HashSet<String>>, spill: Option<&SpillIndex>, ctx: &Ctx<'_>) -> Value {
    let ast = match ctx.ast(formula) {
        Ok(a) => a,
        Err(e) => return Value::err(e),
    };
    let depth = ctx.depth.get();
    if depth >= MAX_DEPTH {
        ctx.taint();
        return Value::err(ErrCode::VALUE);
    }
    ctx.depth.set(depth + 1);
    // SAFETY of lifetimes: the evaluator borrows everything for the duration of this call only.
    let r = {
        let ev = Evaluator::new(data, shrink(ctx), visiting, spill);
        ev.eval(&ast)
    };
    ctx.depth.set(depth);
    if r.is_lambda() {
        Value::err(ErrCode::CALC)
    } else {
        r
    }
}

/// `Ctx<'e>` is covariant in `'e`: a context borrowed for `'a` can be seen as `Ctx<'a>`.
fn shrink<'a, 'e: 'a>(ctx: &'a Ctx<'e>) -> &'a Ctx<'a> {
    ctx
}

/// `evaluate(formula, data, visiting, spill)`: a formula's value (`#CALC!` for a bare lambda).
pub fn evaluate(formula: &str, data: Data<'_>, ctx: &Ctx<'_>, spill: Option<&SpillIndex>) -> Value {
    let visiting = RefCell::new(HashSet::new());
    evaluate_in(formula, data, &visiting, spill, ctx)
}

/// Evaluates a parsed node at the top level (the function wizard's argument previews).
pub fn evaluate_node(node: &Node, data: Data<'_>, ctx: &Ctx<'_>, spill: Option<&SpillIndex>) -> Value {
    let visiting = RefCell::new(HashSet::new());
    let ev = Evaluator::new(data, shrink(ctx), &visiting, spill);
    let r = ev.eval(node);
    if r.is_lambda() {
        Value::err(ErrCode::CALC)
    } else {
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::cell::Cell;
    use serde_json::json;

    pub(crate) fn env() -> FixedEnv {
        FixedEnv::new(LocalNow { year: 2026, month: 10, day: 7, hour: 14, minute: 30, second: 15, millis: 250 })
    }

    pub(crate) fn sheet(cells: &[(&str, serde_json::Value)]) -> CellMap {
        cells
            .iter()
            .map(|(k, v)| {
                let c = match v {
                    serde_json::Value::String(s) if s.starts_with('=') => Cell::formula_cell(s.clone()),
                    other => Cell::value(other.clone()),
                };
                (k.to_string(), c)
            })
            .collect()
    }

    fn ev(cells: &CellMap, f: &str) -> Value {
        let names = HashMap::new();
        let e = env();
        let ctx = Ctx::new(&e);
        evaluate(f, Data { cells, sheets: None, names: &names }, &ctx, None)
    }

    #[test]
    fn arithmetic_and_references() {
        let c = sheet(&[("A1", json!(2)), ("A2", json!("3")), ("A3", json!("=A1*A2")), ("B1", json!("x"))]);
        assert_eq!(ev(&c, "=A3+1"), Value::num(7.0));
        assert_eq!(ev(&c, "=A1/0"), Value::err(ErrCode::DIV0));
        assert_eq!(ev(&c, "=B1+1"), Value::err(ErrCode::VALUE));
        assert_eq!(ev(&c, "=B1&A1"), Value::str("x2"));
        assert_eq!(ev(&c, "=-2^2"), Value::num(4.0));
        assert_eq!(ev(&c, "=50%"), Value::num(0.5));
        assert_eq!(ev(&c, "=C9"), Value::str(""));
    }

    #[test]
    fn comparisons_follow_the_web() {
        let c = sheet(&[("A1", json!(0))]);
        assert_eq!(ev(&c, "=B1=0"), Value::bool(false)); // "" vs 0 compares as text
        assert_eq!(ev(&c, "=\"abc\"=\"ABC\""), Value::bool(true));
        assert_eq!(ev(&c, "=TRUE=1"), Value::bool(true));
        assert_eq!(ev(&c, "=\"b\">\"a\""), Value::bool(true));
    }

    #[test]
    fn circular_references_read_ref() {
        let c = sheet(&[("A1", json!("=B1+1")), ("B1", json!("=A1+1"))]);
        assert_eq!(ev(&c, "=A1"), Value::err(ErrCode::REF));
        let c = sheet(&[("A1", json!("=A1"))]);
        assert_eq!(ev(&c, "=A1"), Value::err(ErrCode::REF));
    }

    #[test]
    fn deep_chains_stop_with_value_instead_of_crashing() {
        let n = 3000;
        let mut cells: Vec<(String, serde_json::Value)> = vec![("A1".into(), json!(1))];
        for i in 2..=n {
            cells.push((format!("A{i}"), json!(format!("=A{}+1", i - 1))));
        }
        let map: CellMap = cells.iter().map(|(k, v)| (k.clone(), if v.is_string() { Cell::formula_cell(v.as_str().unwrap_or("")) } else { Cell::value(v.clone()) })).collect();
        // Run on a big stack, as the desktop does.
        let r = std::thread::Builder::new().stack_size(256 << 20).spawn(move || format!("{:?}", ev(&map, &format!("=A{n}")))).unwrap().join().unwrap();
        assert!(r.contains("#VALUE!"), "{r}");
    }

    #[test]
    fn spill_refs_and_arrays() {
        let c = sheet(&[]);
        assert_eq!(ev(&c, "={1,2}+{10;20}"), Value::M(vec![vec![Scalar::Num(11.0), Scalar::Num(12.0)], vec![Scalar::Num(21.0), Scalar::Num(22.0)]]));
        assert_eq!(ev(&c, "=A1#"), Value::err(ErrCode::REF));
    }

    #[test]
    fn lambdas_let_and_names() {
        let c = sheet(&[("A1", json!(5))]);
        assert_eq!(ev(&c, "=LAMBDA(x,x+1)(A1)"), Value::num(6.0));
        assert_eq!(ev(&c, "=LET(a,2,b,a*3,a+b)"), Value::num(8.0));
        assert_eq!(ev(&c, "=LAMBDA(x,x)"), Value::err(ErrCode::CALC));
        assert_eq!(ev(&c, "=NOPE(1)"), Value::err(ErrCode::NAME));
        let mut names = HashMap::new();
        names.insert("FACTO".to_string(), "=LAMBDA(n, IF(n<2, 1, n*FACTO(n-1)))".to_string());
        names.insert("TAUX".to_string(), "=0.2".to_string());
        let e = env();
        let ctx = Ctx::new(&e);
        let d = Data { cells: &c, sheets: None, names: &names };
        assert_eq!(evaluate("=FACTO(5)", d, &ctx, None), Value::num(120.0));
        assert_eq!(evaluate("=A1*TAUX", d, &ctx, None), Value::num(1.0));
    }

    #[test]
    fn cross_sheet_references() {
        let other = sheet(&[("B2", json!(40)), ("B3", json!("=B2+2"))]);
        let own = sheet(&[("A1", json!(1))]);
        let mut sheets: HashMap<String, &CellMap> = HashMap::new();
        sheets.insert("Feuille 2".into(), &other);
        sheets.insert("Feuille1".into(), &own);
        let names = HashMap::new();
        let e = env();
        let ctx = Ctx::new(&e);
        let d = Data { cells: &own, sheets: Some(&sheets), names: &names };
        assert_eq!(evaluate("='Feuille 2'!B3+A1", d, &ctx, None), Value::num(43.0));
        assert_eq!(evaluate("=Nope!A1", d, &ctx, None), Value::err(ErrCode::REF));
    }

    #[test]
    fn log10_is_a_reference_on_the_web() {
        // QUIRK: `LOG10` matches the reference pattern, so `LOG10(100)` applies a cell to arguments.
        let c = sheet(&[]);
        assert_eq!(ev(&c, "=LOG10(100)"), Value::err(ErrCode::CALC));
    }
}
