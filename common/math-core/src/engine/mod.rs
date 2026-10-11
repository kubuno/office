//! The Kubuno Maths workbench engine — a platform-neutral Rust port of the web Maths editor's pure
//! modules (`office/web/src/math*.ts`). No UI, no OS API: it operates on strings and numbers and
//! produces LaTeX, which a client renders with [`crate::render`].
//!
//! Every module mirrors the behaviour of its web counterpart exactly (including the user-facing
//! French error strings, which the parity tests compare); it never invents semantics the web lacks.
//! Numbers are IEEE-754 `f64` as in JavaScript, and [`num_fmt`] reproduces `Number.prototype`
//! `toString`/`toPrecision` so result strings match the web.
//!
//! | Module | Web origin |
//! |---|---|
//! | [`num_fmt`] | `Number.prototype.toString` / `toPrecision` (used everywhere) |
//! | [`expr`] | `mathExpr.ts` — textual expression AST, parser, evaluation |
//! | [`tree`] | `mathTree.ts` — the WYSIWYG expression tree, serialise/parse |
//! | [`eval`] | `mathEval.ts` — LaTeX → number / text expression, result formatting |
//! | [`number`] | `mathNumber.ts` — prime factors, gcd/lcm, factorial, binomial, base conversion |
//! | [`cas`] | `mathCas.ts` — differentiate / integrate / simplify / solve / study |
//! | [`matrix`] | `mathMatrix.ts` — numeric linear algebra |
//! | [`stats`] | `mathStats.ts` — descriptive statistics, regression |
//! | [`graph`] | `mathGraph.ts` — function-graph model and tick maths |
//! | [`doc`] | `mathPages.ts` — the multi-page / multi-block stored document |

pub mod num_fmt;

pub mod cas;
pub mod doc;
pub mod eval;
pub mod expr;
pub mod graph;
pub mod matrix;
pub mod number;
pub mod stats;
pub mod tree;
