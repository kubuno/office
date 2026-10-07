//! Code-behind of Goal Seek (`goalseek_dialog.kbview`): « Rechercher » runs the solver on the window's editor,
//! then « Appliquer » writes the value found (`runGoalSeek` / `applyGoalSeek`).

use kubuno_desktop::prelude::*;
use kubuno_office_sheets_core::goalseek::GoalSeekResult;

use crate::Resources;

/// Solves (target, value, changing) on the window's editor.
pub type SolveFn = Box<dyn FnMut(&str, &str, &str) -> Option<GoalSeekResult>>;

/// `Number(x.toPrecision(p))` as JavaScript prints it.
fn precise(x: f64, p: i32) -> String {
    if !x.is_finite() || x == 0.0 {
        return kubuno_office_sheets_core::js::to_string(x);
    }
    let digits = p - 1 - x.abs().log10().floor() as i32;
    let r = if digits >= 0 {
        format!("{x:.*}", digits as usize).parse::<f64>().unwrap_or(x)
    } else {
        let f = 10f64.powi(-digits);
        (x / f).round() * f
    };
    kubuno_office_sheets_core::js::to_string(r)
}

#[kubuno_desktop::view("goalseek_dialog.kbview")]
pub struct GoalSeekDialog {
    #[bind]
    target: String,
    #[bind]
    goal: String,
    #[bind]
    changing: String,
    #[bind]
    result_text: String,
    #[bind]
    found: bool,
    #[bind]
    missed: bool,
    #[bind]
    main_text: String,
    result: Option<GoalSeekResult>,
    solve: Option<SolveFn>,
}

impl GoalSeekDialog {
    pub fn new(target: &str, solve: SolveFn) -> Self {
        let mut d = Self { target: target.to_string(), main_text: Resources::gs_solve().into(), solve: Some(solve), ..Self::default() };
        d.initialize_component();
        d
    }

    /// Shows it over `owner`: `Some((changing, x))` when « Appliquer » is clicked.
    pub fn ask(owner: &dyn AsForm, target: &str, solve: SolveFn) -> Option<(String, f64)> {
        let mut d = Self::new(target, solve);
        let applied = d.show_dialog(owner) == DialogResult::Ok;
        if applied {
            d.result.map(|r| (d.changing.clone(), r.x))
        } else {
            None
        }
    }

    fn show_result(&mut self, r: GoalSeekResult) {
        self.found = r.ok;
        self.missed = !r.ok;
        self.result_text = if r.ok { Resources::gs_found().replace("{x}", &precise(r.x, 10)).replace("{y}", &precise(r.achieved, 8)) } else { Resources::gs_not_found().into() };
        self.main_text = if r.ok { Resources::gs_apply().into() } else { Resources::gs_solve().into() };
        self.result = Some(r);
    }

    fn input_changed(&mut self, _e: &TextChangedEventArgs) {
        // A new question: the previous answer no longer holds.
        if self.result.take().is_some() {
            self.found = false;
            self.missed = false;
            self.main_text = Resources::gs_solve().into();
        }
    }

    fn main_button_click(&mut self) {
        if self.result.is_some_and(|r| r.ok) {
            self.set_dialog_result(DialogResult::Ok);
            self.close();
            return;
        }
        let (t, v, c) = (self.target.clone(), self.goal.clone(), self.changing.clone());
        if let Some(r) = self.solve.as_mut().and_then(|s| s(&t, &v, &c)) {
            self.show_result(r);
        }
    }

    fn cancel_button_click(&mut self) {
        self.set_dialog_result(DialogResult::Cancel);
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_print_like_to_precision() {
        assert_eq!(precise(7.0, 10), "7");
        assert_eq!(precise(1.23456789012345, 10), "1.23456789");
        assert_eq!(precise(22.000000004, 8), "22");
        assert_eq!(precise(123456789.0, 3), "123000000");
    }

    #[test]
    fn solving_then_applying() {
        let mut d = GoalSeekDialog { target: "B1".into(), goal: "22".into(), changing: "A1".into(), solve: Some(Box::new(|_, _, _| Some(GoalSeekResult { ok: true, x: 7.0, achieved: 22.0 }))), ..GoalSeekDialog::default() };
        d.main_button_click();
        assert!(d.found);
        assert_eq!(d.result_text, Resources::gs_found().replace("{x}", "7").replace("{y}", "22"));
        assert_eq!(d.main_text, Resources::gs_apply());
    }
}
