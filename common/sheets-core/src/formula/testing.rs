//! Helpers of the unit tests: a sheet from `(key, value)` pairs and a formula evaluated on it at a fixed
//! time (2026-10-07 14:30:15.250).

use std::collections::HashMap;

use super::eval::{evaluate, CellMap, Ctx, Data, FixedEnv};
use super::value::Value;
use crate::jsdate::LocalNow;
use crate::model::cell::Cell;

pub fn env() -> FixedEnv {
    FixedEnv::new(LocalNow { year: 2026, month: 10, day: 7, hour: 14, minute: 30, second: 15, millis: 250 })
}

/// A sheet: a string starting with `=` is a formula, anything else the cell's `v`.
pub fn sheet(cells: &[(&str, serde_json::Value)]) -> CellMap {
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

/// `formula` evaluated on `cells` (no other sheet, no defined name, no spill).
pub fn eval_in(cells: &CellMap, formula: &str) -> Value {
    let names = HashMap::new();
    let e = env();
    let ctx = Ctx::new(&e);
    evaluate(formula, Data { cells, sheets: None, names: &names }, &ctx, None)
}

/// `formula` evaluated on an empty sheet.
#[allow(dead_code)]
pub fn eval(formula: &str) -> Value {
    eval_in(&CellMap::new(), formula)
}
