//! Kubuno Spreadsheets' platform-neutral core (vskubuno `docs/SPREADSHEETS-DESKTOP.md` §4).
//!
//! The port of the web editor's engine (`office/web/src/formula-engine.ts`, `formula-fns/`, the helpers
//! of `SpreadsheetApp.tsx`), with the web's semantics — quirks included and marked `QUIRK` — so a
//! workbook shows the same values on every client.
//!
//! * [`js`] — JavaScript's number and string semantics (`Number()`, `String(x)`, `toFixed`…);
//! * [`jsdate`] — JavaScript's `Date` without a time zone, Excel serials;
//! * [`addr`] — column letters, `A1` keys, ranges;
//! * [`model`] — the stored workbook (cells, styles, sheets) kept losslessly;
//! * [`formula`] — tokenizer, parser, evaluator and functions;
//! * [`format`] — number and date display.
//!
//! No UI and no OS API: the crate builds for `wasm32-unknown-unknown`, Linux, macOS and Windows. It never
//! reads a clock or a random source ([`formula::Env`]).

pub mod addr;
pub mod cf;
pub mod editor;
pub mod format;
pub mod grid;
pub mod formula;
pub mod js;
pub mod jsdate;
pub mod model;
pub mod catalog;
pub mod fill;
pub mod pivot;
pub mod refs;
pub mod validation;
