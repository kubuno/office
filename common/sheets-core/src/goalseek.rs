//! Données › Valeur cible (`runGoalSeek` / `applyGoalSeek`, SA:4050-4114): solves « target cell = value »
//! by changing one input cell - the secant method first, else a bracket found by geometric widening and a
//! bisection; a « clean » root (7 rather than 6.9999999958) is preferred when it reaches the target too.

use crate::addr;
use crate::editor::commands::Refused;
use crate::editor::Editor;
use crate::formula::eval::{evaluate, Data};
use crate::formula::value::{Scalar, Value};
use crate::js;
use crate::model::cell::Cell;

/// What the search found (`gsResult`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GoalSeekResult {
    pub ok: bool,
    /// The input value found (NaN when the inputs are invalid).
    pub x: f64,
    /// What the target cell computes with it.
    pub achieved: f64,
}

impl GoalSeekResult {
    fn invalid() -> Self {
        GoalSeekResult { ok: false, x: f64::NAN, achieved: f64::NAN }
    }
}

/// `/^[A-Z]{1,3}[0-9]+$/` on the upper-cased reference.
fn plain_ref(s: &str) -> bool {
    let letters = s.chars().take_while(|c| c.is_ascii_uppercase()).count();
    (1..=3).contains(&letters) && s.len() > letters && s[letters..].chars().all(|c| c.is_ascii_digit())
}

/// JavaScript `Number(x.toFixed(digits))`.
fn to_fixed(x: f64, digits: usize) -> f64 {
    format!("{x:.digits$}").parse().unwrap_or(x)
}

impl Editor {
    /// `runGoalSeek`: `target` (a formula cell), the value to reach as typed (spaces dropped, a decimal
    /// comma accepted), the cell to change.
    pub fn goal_seek(&self, target: &str, value: &str, changing: &str) -> GoalSeekResult {
        let tgt = target.trim().to_uppercase();
        let chg = changing.trim().to_uppercase();
        let wanted = js::number_or_nan(&value.chars().filter(|c| !c.is_whitespace()).collect::<String>().replacen(',', ".", 1));
        let cells = self.cells();
        let Some(formula) = cells.get(&tgt).and_then(|c| c.formula()).filter(|f| !f.is_empty()).map(str::to_string) else { return GoalSeekResult::invalid() };
        if !wanted.is_finite() || !plain_ref(&chg) || tgt == chg {
            return GoalSeekResult::invalid();
        }
        let names = self.wb.names_map();
        let by_name = self.wb.sheets_by_name();
        let spill = self.spill();
        let eval_at = |x: f64| -> f64 {
            let mut c = cells.clone();
            c.insert(chg.clone(), Cell::value(serde_json::json!(x)));
            let data = Data { cells: &c, sheets: by_name.as_ref(), names: &names };
            let ctx = crate::formula::eval::Ctx::new(self.env());
            match evaluate(&formula, data, &ctx, Some(&spill)) {
                Value::S(Scalar::Num(n)) if n.is_finite() => n,
                _ => f64::NAN,
            }
        };
        let tol = 1e-7 * wanted.abs().max(1.0);
        // `Number(cells[chg]?.v ?? 0)`.
        let mut x0 = match cells.get(&chg).and_then(|c| c.v.as_ref()) {
            None | Some(serde_json::Value::Null) => 0.0,
            Some(serde_json::Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
            Some(serde_json::Value::Bool(b)) => f64::from(u8::from(*b)),
            Some(serde_json::Value::String(s)) => js::number_or_nan(s),
            Some(_) => f64::NAN,
        };
        if !x0.is_finite() {
            x0 = 0.0;
        }
        let mut x1 = if x0 == 0.0 { 1.0 } else { x0 * 1.1 + 0.1 };
        let mut f0 = eval_at(x0) - wanted;
        let mut f1 = eval_at(x1) - wanted;
        let mut ok = false;
        for _ in 0..100 {
            if f1.is_nan() {
                break;
            }
            if f1.abs() <= tol {
                ok = true;
                break;
            }
            let d = f1 - f0;
            if !d.is_finite() || d == 0.0 {
                break;
            }
            let x2 = x1 - f1 * (x1 - x0) / d;
            if !x2.is_finite() {
                break;
            }
            x0 = x1;
            f0 = f1;
            x1 = x2;
            f1 = eval_at(x1) - wanted;
        }
        if !ok {
            let mut lo = x0;
            let mut hi = x0;
            let mut flo = eval_at(lo) - wanted;
            let mut bracket = false;
            let mut step = 1.0;
            while step <= 1e9 && !bracket {
                for cand in [x0 - step, x0 + step] {
                    let fc = eval_at(cand) - wanted;
                    if !fc.is_nan() && !flo.is_nan() && fc * flo <= 0.0 {
                        hi = cand;
                        bracket = true;
                        break;
                    }
                }
                step *= 4.0;
            }
            if bracket {
                for _ in 0..200 {
                    let mid = (lo + hi) / 2.0;
                    let fm = eval_at(mid) - wanted;
                    if fm.is_nan() {
                        break;
                    }
                    if fm.abs() <= tol {
                        x1 = mid;
                        f1 = fm;
                        ok = true;
                        break;
                    }
                    if fm * flo <= 0.0 {
                        hi = mid;
                    } else {
                        lo = mid;
                        flo = fm;
                    }
                    x1 = mid;
                    f1 = fm;
                }
            }
        }
        if ok {
            for digits in [0, 2, 4, 6] {
                let xr = to_fixed(x1, digits);
                let fr = eval_at(xr) - wanted;
                if fr.abs() <= tol {
                    x1 = xr;
                    f1 = fr;
                    break;
                }
            }
        }
        GoalSeekResult { ok, x: x1, achieved: f1 + wanted }
    }

    /// `applyGoalSeek`: the input cell takes the value found (its formula goes, its style stays).
    pub fn apply_goal_seek(&mut self, changing: &str, x: f64) -> Result<(), Refused> {
        let chg = changing.trim().to_uppercase();
        let Some(at) = addr::Cell::parse(&chg).filter(|_| plain_ref(&chg) && x.is_finite()) else { return Ok(()) };
        let mut cell = self.cells().get(&chg).cloned().unwrap_or_default();
        cell.v = Some(serde_json::json!(x));
        cell.f = None;
        self.write_cells(vec![(at, Some(cell))])
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::tests::editor;
    use serde_json::json;

    #[test]
    fn a_linear_target_is_solved_with_a_clean_root() {
        let ed = editor(json!({"A1": {"v": 2}, "B1": {"f": "=A1*3+1"}}));
        let r = ed.goal_seek("b1", " 2 2 ", "A1");
        assert!(r.ok);
        assert_eq!(r.x, 7.0, "the clean root, not 6.99999…");
        assert_eq!(r.achieved, 22.0);
    }

    #[test]
    fn a_non_linear_target_and_a_decimal_comma() {
        let ed = editor(json!({"A1": {"v": 1}, "B1": {"f": "=A1*A1"}}));
        let r = ed.goal_seek("B1", "2,25", "A1");
        assert!(r.ok);
        assert!((r.x - 1.5).abs() < 1e-6, "{}", r.x);
    }

    #[test]
    fn invalid_inputs_find_nothing() {
        let ed = editor(json!({"A1": {"v": 1}, "B1": {"v": 3}, "C1": {"f": "=A1"}}));
        assert!(!ed.goal_seek("B1", "4", "A1").ok, "the target is no formula");
        assert!(ed.goal_seek("C1", "x", "A1").x.is_nan());
        assert!(!ed.goal_seek("C1", "4", "C1").ok);
        assert!(!ed.goal_seek("C1", "4", "$A$1").ok);
        let ed = editor(json!({"A1": {"v": 1}, "B1": {"f": "=ABS(A1)"}}));
        assert!(!ed.goal_seek("B1", "-5", "A1").ok, "unreachable");
    }

    #[test]
    fn applying_writes_the_value_and_drops_the_formula() {
        let mut ed = editor(json!({"A1": {"f": "=1", "s": {"bold": true}}, "B1": {"f": "=A1*3+1"}}));
        let r = ed.goal_seek("B1", "22", "A1");
        ed.apply_goal_seek("a1", r.x).expect("not protected");
        let c = ed.cells().get("A1").cloned().expect("A1");
        assert_eq!(c.v, Some(json!(7.0)));
        assert!(c.f.is_none());
        assert!(c.s.is_some());
    }
}
