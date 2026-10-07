//! Code-behind of the custom sort (`sort_dialog.kbview`): the web's `SortDialog`. It starts with one level on
//! the range's first column; « Trier » returns the header choice and the levels (`sortRangeMulti`).

use kubuno_desktop::prelude::*;
use kubuno_office_sheets_core::dialogs::{SortColumn, SortSetup};
use kubuno_office_sheets_core::editor::commands::SortLevel;

use crate::Resources;

#[kubuno_desktop::view("sort_dialog.kbview")]
pub struct SortDialog {
    #[bind]
    range_text: String,
    #[bind]
    headers: bool,
    #[bind]
    level_rows: Rows,
    #[bind]
    can_add: bool,
    columns: Vec<SortColumn>,
    levels: Vec<SortLevel>,
}

impl SortDialog {
    pub fn new(setup: &SortSetup) -> Self {
        let first = setup.columns.first().map(|c| c.idx).unwrap_or(0);
        let mut d = Self {
            range_text: format!("{} : {}", Resources::sort_range(), setup.label),
            headers: setup.headers,
            columns: setup.columns.clone(),
            levels: vec![SortLevel { col: first, asc: true }],
            ..Self::default()
        };
        d.refresh();
        d.initialize_component();
        d
    }

    /// Shows it over `owner`: `Some((headers, levels))` when « Trier » is clicked.
    pub fn ask(owner: &dyn AsForm, setup: &SortSetup) -> Option<(bool, Vec<SortLevel>)> {
        let mut d = Self::new(setup);
        (d.show_dialog(owner) == DialogResult::Ok).then(|| (d.headers, d.levels.clone()))
    }

    /// `colLabel`: the header text when the first row is a header, else « Colonne B ».
    fn col_label(&self, c: &SortColumn) -> String {
        if self.headers && !c.header.trim().is_empty() {
            c.header.clone()
        } else {
            Resources::sort_column().replace("{l}", &c.letter)
        }
    }

    fn refresh(&mut self) {
        let options: Vec<Row> = self.columns.iter().map(|c| Row::new().with("Value", Value::Str(c.idx.to_string())).with("Label", Value::Str(self.col_label(c)))).collect();
        let options = Rows::from(options);
        let orders = super::options(&[("1", Resources::sort_asc()), ("0", Resources::sort_desc())]);
        let n = self.levels.len();
        self.level_rows = Rows::from(
            self.levels
                .iter()
                .enumerate()
                .map(|(i, lv)| {
                    Row::new()
                        .with("Caption", Value::Str(if i == 0 { Resources::sort_by().into() } else { Resources::sort_then().into() }))
                        .with("Columns", Value::List(options.clone()))
                        .with("Orders", Value::List(orders.clone()))
                        .with("Col", Value::Str(lv.col.to_string()))
                        .with("Asc", Value::Str(if lv.asc { "1".into() } else { "0".into() }))
                        .with("CanUp", Value::Bool(i > 0))
                        .with("CanDown", Value::Bool(i + 1 < n))
                        .with("CanRemove", Value::Bool(n > 1))
                })
                .collect::<Vec<_>>(),
        );
        self.can_add = self.levels.len() < self.columns.len();
    }

    fn index() -> Option<usize> {
        kubuno_desktop::views::binding::current_item().map(|i| i.index)
    }

    fn headers_check_changed(&mut self, e: &CheckedChangedEventArgs) {
        self.headers = e.new;
        self.refresh();
    }

    fn level_col_changed(&mut self, e: &TextChangedEventArgs) {
        if let (Some(i), Ok(c)) = (Self::index(), e.new.parse::<u32>()) {
            if let Some(lv) = self.levels.get_mut(i) {
                lv.col = c;
            }
        }
    }

    fn level_order_changed(&mut self, e: &TextChangedEventArgs) {
        if let Some(lv) = Self::index().and_then(|i| self.levels.get_mut(i)) {
            lv.asc = e.new == "1";
        }
    }

    fn move_level(&mut self, d: isize) {
        let Some(i) = Self::index() else { return };
        let j = i as isize + d;
        if j < 0 || j as usize >= self.levels.len() {
            return;
        }
        self.levels.swap(i, j as usize);
        self.refresh();
    }

    fn level_up_click(&mut self) {
        self.move_level(-1);
    }

    fn level_down_click(&mut self) {
        self.move_level(1);
    }

    fn level_remove_click(&mut self) {
        if let Some(i) = Self::index() {
            if self.levels.len() > 1 && i < self.levels.len() {
                self.levels.remove(i);
                self.refresh();
            }
        }
    }

    /// `addLevel`: the first column not sorted yet (each column once).
    fn add_level_link_click(&mut self) {
        if self.levels.len() >= self.columns.len() {
            return;
        }
        let used: Vec<u32> = self.levels.iter().map(|l| l.col).collect();
        let free = self.columns.iter().find(|c| !used.contains(&c.idx)).or(self.columns.first()).map(|c| c.idx).unwrap_or(0);
        self.levels.push(SortLevel { col: free, asc: true });
        self.refresh();
    }

    fn sort_button_click(&mut self) {
        self.set_dialog_result(DialogResult::Ok);
        self.close();
    }

    fn cancel_button_click(&mut self) {
        self.set_dialog_result(DialogResult::Cancel);
        self.close();
    }
}
