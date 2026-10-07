//! The formula language: the port of the web editor's `formula-engine.ts` and `formula-fns/`.
//!
//! * [`parser`] — tokenizer and recursive-descent parser ([`ast::Node`]);
//! * [`value`] — scalars, matrices, lambdas and JavaScript's coercions;
//! * [`eval`] — the evaluator (references, ranges, spills, LET/LAMBDA, defined names);
//! * [`criteria`] — COUNTIF-style criteria;
//! * [`functions`] — the registry and the functions, one module per web file.

pub mod ast;
pub mod criteria;
pub mod eval;
pub mod functions;
pub mod parser;
pub mod spill;
pub mod translate;
pub mod value;

#[cfg(test)]
pub(crate) mod testing;

pub use eval::{evaluate, CellMap, Ctx, Data, Env, FixedEnv, SpillIndex};
pub use parser::{is_valid_defined_name, parse_formula, validate_name_formula};
pub use value::{format_value, ErrCode, Matrix, Scalar, Value};
