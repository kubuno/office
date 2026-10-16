//! Port of the web's `formula-fns/logical.ts`: Logical and Information functions beyond the engine's own
//! IF/IFERROR/IFS/AND/OR/NOT/IS* block.

use super::Registry;
use crate::formula::ast::Node;
use crate::formula::eval::{arg, scalar_of, Evaluator};
use crate::formula::value::{flatten, to_bool, to_num, to_str, ErrCode, Scalar, Value};
use crate::js;

type R = Result<Value, ErrCode>;

/// `scalarOf(ev, n)` with a flag telling whether the value was a lambda (a plain object on the web,
/// which reads as the text "[object Object]" when coerced, but is not a string, a boolean or an error).
fn scalar_lam(ev: &Evaluator, n: &Node) -> (Scalar, bool) {
    let v = ev.eval(n);
    let lam = v.is_lambda();
    (scalar_of(&v), lam)
}

/// `isNA`: the specific `#N/A` error.
fn is_na(s: &Scalar) -> bool {
    matches!(s, Scalar::Err(e) if *e == ErrCode::NA)
}

// ── Logical ──────────────────────────────────────────────────────────────────

/// XOR: TRUE when an odd number of arguments are TRUE; the first error is propagated.
fn xor(ev: &Evaluator, args: &[Node]) -> R {
    let mut true_count = 0usize;
    let mut seen = false;
    for a in args {
        let v = ev.eval(a);
        if v.is_lambda() {
            // `flatten(lambda)` = [lambda]: not an error, `toBool(object)` = false.
            seen = true;
            continue;
        }
        for s in flatten(&v) {
            if let Scalar::Err(e) = s {
                return Err(e);
            }
            seen = true;
            if to_bool(&s) {
                true_count += 1;
            }
        }
    }
    if !seen {
        return Err(ErrCode::VALUE);
    }
    Ok(Value::bool(true_count % 2 == 1))
}

/// IFNA(value, value_if_na).
fn ifna(ev: &Evaluator, args: &[Node]) -> R {
    if args.len() < 2 {
        return Err(ErrCode::VALUE);
    }
    let v = ev.eval(&args[0]);
    if is_na(&scalar_of(&v)) {
        return Ok(ev.eval(&args[1]));
    }
    Ok(v)
}

/// Excel-style equality used by SWITCH: numbers numerically, text case-insensitively, booleans by
/// truthiness (a lambda is an object: false as a boolean, "[object Object]" as text).
fn scalars_equal(a: &Scalar, a_lam: bool, b: &Scalar, b_lam: bool) -> bool {
    if let (Scalar::Num(x), Scalar::Num(y)) = (a, b) {
        return x == y;
    }
    if a.is_bool() || b.is_bool() {
        let tb = |s: &Scalar, lam: bool| !lam && to_bool(s);
        return tb(a, a_lam) == tb(b, b_lam);
    }
    js::lower(&to_str(a)) == js::lower(&to_str(b))
}

/// SWITCH(expr, val1, res1, [val2, res2, ...], [default]).
fn switch(ev: &Evaluator, args: &[Node]) -> R {
    if args.len() < 2 {
        return Err(ErrCode::VALUE);
    }
    let (expr, expr_lam) = scalar_lam(ev, &args[0]);
    if let Scalar::Err(e) = expr {
        return Err(e);
    }
    let mut i = 1;
    while i + 1 < args.len() {
        let (candidate, cand_lam) = scalar_lam(ev, &args[i]);
        if let Scalar::Err(e) = candidate {
            return Err(e);
        }
        if scalars_equal(&expr, expr_lam, &candidate, cand_lam) {
            return Ok(ev.eval(&args[i + 1]));
        }
        i += 2;
    }
    // A trailing odd argument is the default value.
    if i < args.len() {
        return Ok(ev.eval(&args[i]));
    }
    Err(ErrCode::NA)
}

/// `FILTER: unsupported` - the web registers this stub (lookup.ts overrides it later).
fn filter(_ev: &Evaluator, _args: &[Node]) -> R {
    Err(ErrCode::NA)
}

// ── Information ──────────────────────────────────────────────────────────────

fn iserr(ev: &Evaluator, args: &[Node]) -> R {
    let (s, _) = scalar_lam(ev, arg(args, 0)?);
    Ok(Value::bool(s.is_err() && !is_na(&s)))
}

fn isna(ev: &Evaluator, args: &[Node]) -> R {
    let (s, _) = scalar_lam(ev, arg(args, 0)?);
    Ok(Value::bool(is_na(&s)))
}

fn isnontext(ev: &Evaluator, args: &[Node]) -> R {
    let (s, lam) = scalar_lam(ev, arg(args, 0)?);
    Ok(Value::bool(lam || !s.is_str()))
}

/// `numberOf`: the scalar as a number, its own error, or `#VALUE!`.
fn number_of(ev: &Evaluator, node: &Node) -> Result<f64, ErrCode> {
    let (s, _) = scalar_lam(ev, node);
    to_num(&s)
}

fn iseven(ev: &Evaluator, args: &[Node]) -> R {
    let n = number_of(ev, arg(args, 0)?)?;
    Ok(Value::bool(js::trunc(n) % 2.0 == 0.0))
}

fn isodd(ev: &Evaluator, args: &[Node]) -> R {
    let n = number_of(ev, arg(args, 0)?)?;
    Ok(Value::bool((js::trunc(n) % 2.0).abs() == 1.0))
}

/// ISREF: TRUE only when the argument is a literal cell/range node.
fn isref(_ev: &Evaluator, args: &[Node]) -> R {
    Ok(Value::bool(matches!(args.first(), Some(Node::Ref { .. } | Node::Range { .. }))))
}

fn isformula(_ev: &Evaluator, _args: &[Node]) -> R {
    Ok(Value::bool(false))
}

/// N(v): number → itself, TRUE → 1, FALSE → 0, text → 0, error → the error.
fn n_fn(ev: &Evaluator, args: &[Node]) -> R {
    let (s, _) = scalar_lam(ev, arg(args, 0)?);
    match s {
        Scalar::Err(e) => Err(e),
        Scalar::Num(n) => Ok(Value::num(n)),
        Scalar::Bool(b) => Ok(Value::num(if b { 1.0 } else { 0.0 })),
        Scalar::Str(_) => Ok(Value::num(0.0)),
    }
}

fn na(_ev: &Evaluator, _args: &[Node]) -> R {
    Err(ErrCode::NA)
}

/// TYPE(v): 1 number, 2 text, 4 logical, 16 error, 64 array.
fn type_fn(ev: &Evaluator, args: &[Node]) -> R {
    let v = ev.eval(arg(args, 0)?);
    Ok(Value::num(match v {
        Value::M(_) => 64.0,
        Value::S(Scalar::Err(_)) => 16.0,
        Value::S(Scalar::Num(_)) => 1.0,
        Value::S(Scalar::Bool(_)) => 4.0,
        _ => 2.0,
    }))
}

/// ERROR.TYPE(err): 1..7 for the error, else `#N/A`.
fn error_type(ev: &Evaluator, args: &[Node]) -> R {
    let (s, _) = scalar_lam(ev, arg(args, 0)?);
    let Scalar::Err(e) = s else { return Err(ErrCode::NA) };
    let code = match e.as_str() {
        "#NULL!" => 1.0,
        "#DIV/0!" => 2.0,
        "#VALUE!" => 3.0,
        "#REF!" => 4.0,
        "#NAME?" => 5.0,
        "#NUM!" => 6.0,
        "#N/A" => 7.0,
        _ => return Err(ErrCode::NA),
    };
    Ok(Value::num(code))
}

/// SHEET / SHEETS: the web's engine is single-sheet, so always 1.
fn one(_ev: &Evaluator, _args: &[Node]) -> R {
    Ok(Value::num(1.0))
}

fn isomitted(_ev: &Evaluator, _args: &[Node]) -> R {
    Ok(Value::bool(false))
}

pub fn register(r: &mut Registry) {
    let list: &[(&'static str, crate::formula::eval::Fn)] = &[
        ("XOR", xor),
        ("IFNA", ifna),
        ("SWITCH", switch),
        ("FILTER", filter),
        ("ISERR", iserr),
        ("ISNA", isna),
        ("ISNONTEXT", isnontext),
        ("ISEVEN", iseven),
        ("ISODD", isodd),
        ("ISREF", isref),
        ("ISFORMULA", isformula),
        ("N", n_fn),
        ("NA", na),
        ("TYPE", type_fn),
        ("ERROR.TYPE", error_type),
        ("SHEET", one),
        ("SHEETS", one),
        ("ISOMITTED", isomitted),
    ];
    for (name, f) in list {
        r.insert(name, *f);
    }
}

#[cfg(test)]
mod tests {
    use crate::formula::testing::eval;
    use crate::formula::value::{ErrCode, Value};

    #[test]
    fn switch_and_xor() {
        assert_eq!(eval("=SWITCH(2,1,\"a\",2,\"b\",\"z\")"), Value::str("b"));
        assert_eq!(eval("=SWITCH(9,1,\"a\",\"z\")"), Value::str("z"));
        assert_eq!(eval("=SWITCH(9,1,\"a\")"), Value::err(ErrCode::NA));
        assert_eq!(eval("=XOR(TRUE,FALSE,TRUE)"), Value::bool(false));
        assert_eq!(eval("=XOR()"), Value::err(ErrCode::VALUE));
    }

    #[test]
    fn information() {
        assert_eq!(eval("=ERROR.TYPE(1/0)"), Value::num(2.0));
        assert_eq!(eval("=TYPE({1,2})"), Value::num(64.0));
        assert_eq!(eval("=ISODD(-3)"), Value::bool(true));
        assert_eq!(eval("=ISREF(A1)"), Value::bool(true));
    }
}
