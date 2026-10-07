//! Dynamic-array spilling (`computeSpills`): a formula whose result is larger than 1×1 spills from its
//! cell into the cells below and to the right, unless one of them has content of its own or was claimed
//! by an earlier spill (row-major order), the result exceeds 200 000 cells or leaves the grid — then the
//! anchor shows `#SPILL!`. Up to four passes, so a spill that reads another spilled cell settles.

use std::collections::HashMap;

use super::eval::{evaluate, Ctx, Data, SpillAnchor, SpillIndex};
use super::value::{width, ErrCode, Scalar, Value};
use crate::addr;

/// `MAX_SPILL_CELLS`.
pub const MAX_SPILL_CELLS: usize = 200_000;

/// `computeSpills(data)`.
pub fn compute_spills(data: Data<'_>, ctx: &Ctx<'_>) -> SpillIndex {
    let mut prev = SpillIndex::default();
    let mut formula_keys: Vec<(String, u32, u32)> = data
        .cells
        .iter()
        .filter(|(_, c)| c.formula().is_some())
        .filter_map(|(k, _)| addr::Cell::parse(k).map(|c| (k.clone(), c.col, c.row)))
        .collect();
    formula_keys.sort_by(|a, b| a.2.cmp(&b.2).then(a.1.cmp(&b.1)));

    for _pass in 0..4 {
        let mut values: HashMap<String, Scalar> = HashMap::new();
        let mut anchors: HashMap<String, SpillAnchor> = HashMap::new();
        let mut origin: HashMap<String, String> = HashMap::new();
        let mut claimed: HashMap<String, String> = HashMap::new();
        // The memo stays valid across passes: a referenced formula cell is evaluated without the
        // spill index (as on the web), so its value does not depend on the previous pass.
        for (key, c, r) in &formula_keys {
            let Some(f) = data.cells.get(key).and_then(|cell| cell.formula()) else { continue };
            let v = evaluate(f, data, ctx, Some(&prev));
            let Value::M(m) = v else { continue };
            let rows = m.len();
            let cols = width(&m);
            if rows <= 1 && cols <= 1 {
                continue;
            }
            let mut blocked = rows * cols > MAX_SPILL_CELLS;
            let mut targets = Vec::new();
            if !blocked {
                'outer: for dr in 0..rows {
                    for dc in 0..cols {
                        if dr == 0 && dc == 0 {
                            continue;
                        }
                        let (tc, tr) = (*c as u64 + dc as u64, *r as u64 + dr as u64);
                        if tc >= addr::MAX_COLS as u64 || tr > addr::MAX_ROWS as u64 {
                            blocked = true;
                            break 'outer;
                        }
                        let tkey = addr::key(tc as u32, tr as u32);
                        let has_own = data.cells.get(&tkey).is_some_and(|t| t.has_f() || t.has_value());
                        if has_own || claimed.contains_key(&tkey) {
                            blocked = true;
                            break 'outer;
                        }
                        targets.push((tkey, dr, dc));
                    }
                }
            }
            origin.insert(key.clone(), key.clone());
            if !blocked {
                for (tkey, dr, dc) in targets {
                    let v = m.get(dr).and_then(|row| row.get(dc)).cloned().unwrap_or(Scalar::Err(ErrCode::NA));
                    values.insert(tkey.clone(), v);
                    origin.insert(tkey.clone(), key.clone());
                    claimed.insert(tkey, key.clone());
                }
            }
            anchors.insert(key.clone(), SpillAnchor { matrix: m, rows, cols, blocked });
        }
        let stable = prev.values.len() == values.len() && values.iter().all(|(k, v)| prev.values.get(k) == Some(v));
        prev = SpillIndex { values, anchors, origin };
        if stable {
            break;
        }
    }
    prev
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formula::testing::{env, sheet};
    use serde_json::json;

    #[test]
    fn arrays_spill_and_block() {
        let c = sheet(&[("A1", json!("={1,2;3,4}")), ("D1", json!("={1;2}")), ("D2", json!(9))]);
        let names = HashMap::new();
        let e = env();
        let ctx = Ctx::new(&e);
        let s = compute_spills(Data { cells: &c, sheets: None, names: &names }, &ctx);
        assert_eq!(s.values.get("B2"), Some(&Scalar::Num(4.0)));
        assert_eq!(s.origin.get("B1").map(String::as_str), Some("A1"));
        assert!(s.anchors["D1"].blocked);
        assert!(!s.anchors["A1"].blocked);
        let v = evaluate("=SUM(A1#)", Data { cells: &c, sheets: None, names: &names }, &ctx, Some(&s));
        assert_eq!(v, Value::num(10.0));
    }
}
