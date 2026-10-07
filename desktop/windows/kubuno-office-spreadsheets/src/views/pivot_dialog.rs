//! Code-behind of the pivot dialog (`pivot_dialog.kbview`): the web's `PivotDialog`. It edits a
//! [`PivotChoice`]; the window builds the table with the core (`Editor::apply_pivot`).

use kubuno_desktop::prelude::*;
use kubuno_office_sheets_core::pivot::{PivotAgg, PivotFilterSpec, PivotValueSpec};
use kubuno_office_sheets_core::pivot_table::{PivotChoice, PivotSetup};

use crate::controls::value_list::{encode_list, ValueEventArgs};
use crate::Resources;

/// The distinct values of a source column (absolute index), for the filters.
pub type DistinctFn = Box<dyn Fn(usize) -> Vec<String>>;

const AGGS: [(PivotAgg, &str); 6] = [(PivotAgg::Sum, "sum"), (PivotAgg::Count, "count"), (PivotAgg::CountNum, "countNum"), (PivotAgg::Avg, "avg"), (PivotAgg::Min, "min"), (PivotAgg::Max, "max")];

fn agg_label(a: PivotAgg) -> &'static str {
    match a {
        PivotAgg::Sum => Resources::pv_agg_sum(),
        PivotAgg::Count => Resources::pv_agg_count(),
        PivotAgg::CountNum => Resources::pv_agg_countnum(),
        PivotAgg::Avg => Resources::pv_agg_avg(),
        PivotAgg::Min => Resources::pv_agg_min(),
        PivotAgg::Max => Resources::pv_agg_max(),
    }
}

#[kubuno_desktop::view("pivot_dialog.kbview")]
pub struct PivotDialog {
    #[bind]
    caption: String,
    #[bind]
    source_text: String,
    #[bind]
    row_items: String,
    #[bind]
    row_checked: String,
    #[bind]
    filter_rows: Rows,
    #[bind]
    col_options: Rows,
    #[bind]
    col_field: String,
    #[bind]
    value_rows: Rows,
    #[bind]
    target: String,
    #[bind]
    build_text: String,
    #[bind]
    can_build: bool,
    setup: Option<PivotSetup>,
    choice: PivotChoice,
    distinct: Option<DistinctFn>,
}

impl PivotDialog {
    pub fn new(setup: PivotSetup, distinct: DistinctFn) -> Self {
        let edit = setup.edit.as_ref().map(|(_, c)| c.clone());
        let first = setup.columns.first().map(|c| c.idx);
        let last = setup.columns.last().map(|c| c.idx).unwrap_or(0);
        let choice = edit.clone().unwrap_or(PivotChoice { row_fields: first.into_iter().collect(), col_field: None, values: vec![PivotValueSpec { field: last, agg: PivotAgg::Sum }], filters: Vec::new(), target: setup.target.clone() });
        let mut d = Self {
            caption: if edit.is_some() { Resources::pv_title_edit().into() } else { Resources::pv_title().into() },
            build_text: if edit.is_some() { Resources::pv_apply().into() } else { Resources::pv_build().into() },
            source_text: format!("{} : {}", Resources::pv_source(), setup.label),
            target: choice.target.clone(),
            col_field: choice.col_field.map(|c| c.to_string()).unwrap_or_default(),
            choice,
            setup: Some(setup),
            distinct: Some(distinct),
            ..Self::default()
        };
        d.refresh();
        d.initialize_component();
        d
    }

    /// Shows it over `owner`: the choice when « Créer » / « Appliquer » is clicked.
    pub fn ask(owner: &dyn AsForm, setup: PivotSetup, distinct: DistinctFn) -> Option<PivotChoice> {
        let mut d = Self::new(setup, distinct);
        (d.show_dialog(owner) == DialogResult::Ok).then(|| d.result())
    }

    /// `label(idx)`: the header, else « Colonne B ».
    fn label(&self, idx: usize) -> String {
        let Some(c) = self.setup.as_ref().and_then(|s| s.columns.iter().find(|c| c.idx == idx)) else { return String::new() };
        if c.header.trim().is_empty() {
            Resources::pv_col().replace("{l}", &c.letter)
        } else {
            c.header.clone()
        }
    }

    fn column_options(&self) -> Rows {
        let cols = self.setup.as_ref().map(|s| s.columns.clone()).unwrap_or_default();
        Rows::from(cols.iter().map(|c| Row::new().with("Value", Value::Str(c.idx.to_string())).with("Label", Value::Str(self.label(c.idx)))).collect::<Vec<_>>())
    }

    /// The row fields' list: each column, its rank among the row fields after it.
    fn row_label(&self, idx: usize) -> String {
        match self.choice.row_fields.iter().position(|f| *f == idx) {
            Some(i) => format!("{}   ·  {}", self.label(idx), i + 1),
            None => self.label(idx),
        }
    }

    fn refresh(&mut self) {
        let cols = self.setup.as_ref().map(|s| s.columns.clone()).unwrap_or_default();
        let labels: Vec<String> = cols.iter().map(|c| self.row_label(c.idx)).collect();
        self.row_items = encode_list(&labels);
        let checked: Vec<String> = cols.iter().filter(|c| self.choice.row_fields.contains(&c.idx)).map(|c| self.row_label(c.idx)).collect();
        self.row_checked = encode_list(&checked);
        let options = self.column_options();
        let mut col_opts = vec![Row::new().with("Value", Value::Str(String::new())).with("Label", Value::Str(Resources::pv_none().into()))];
        col_opts.extend(cols.iter().map(|c| Row::new().with("Value", Value::Str(c.idx.to_string())).with("Label", Value::Str(self.label(c.idx)))));
        self.col_options = Rows::from(col_opts);
        let aggs = Rows::from(AGGS.iter().map(|(a, v)| Row::new().with("Value", Value::Str((*v).into())).with("Label", Value::Str(agg_label(*a).into()))).collect::<Vec<_>>());
        let n = self.choice.values.len();
        self.value_rows = Rows::from(
            self.choice
                .values
                .iter()
                .map(|v| {
                    let agg = AGGS.iter().find(|(a, _)| *a == v.agg).map(|(_, s)| *s).unwrap_or("sum");
                    Row::new().with("Columns", Value::List(options.clone())).with("Field", Value::Str(v.field.to_string())).with("Aggs", Value::List(aggs.clone())).with("Agg", Value::Str(agg.into())).with("CanRemove", Value::Bool(n > 1))
                })
                .collect::<Vec<_>>(),
        );
        let distinct = self.distinct.as_ref();
        self.filter_rows = Rows::from(
            self.choice
                .filters
                .iter()
                .map(|f| {
                    let vals = distinct.map(|d| d(f.field)).unwrap_or_default();
                    Row::new()
                        .with("Columns", Value::List(options.clone()))
                        .with("Field", Value::Str(f.field.to_string()))
                        .with("Values", Value::Str(encode_list(&vals)))
                        .with("Checked", Value::Str(encode_list(&f.selected)))
                        .with("NoneChecked", Value::Bool(f.selected.is_empty()))
                })
                .collect::<Vec<_>>(),
        );
        self.can_build = !self.choice.row_fields.is_empty() && !self.choice.values.is_empty();
    }

    /// The choice as the dialog shows it (`onBuild`).
    pub fn result(&self) -> PivotChoice {
        let mut c = self.choice.clone();
        c.col_field = self.col_field.parse().ok();
        let t = self.target.trim().to_uppercase();
        c.target = if t.is_empty() { "A1".into() } else { t };
        c
    }

    fn item() -> Option<usize> {
        kubuno_desktop::views::binding::current_item().map(|i| i.index)
    }

    /// `toggleRow`.
    fn row_list_item_click(&mut self, e: &ValueEventArgs) {
        let Some(idx) = self.setup.as_ref().and_then(|s| s.columns.get(e.index.max(0) as usize)).map(|c| c.idx) else { return };
        if let Some(p) = self.choice.row_fields.iter().position(|f| *f == idx) {
            self.choice.row_fields.remove(p);
        } else {
            self.choice.row_fields.push(idx);
        }
        self.refresh();
    }

    fn filter_field_changed(&mut self, e: &TextChangedEventArgs) {
        let (Some(i), Ok(f)) = (Self::item(), e.new.parse::<usize>()) else { return };
        if let Some(flt) = self.choice.filters.get_mut(i) {
            if flt.field != f {
                *flt = PivotFilterSpec { field: f, selected: Vec::new() };
                self.refresh();
            }
        }
    }

    fn filter_value_click(&mut self, e: &ValueEventArgs) {
        let Some(i) = Self::item() else { return };
        if let Some(flt) = self.choice.filters.get_mut(i) {
            if let Some(p) = flt.selected.iter().position(|v| *v == e.value) {
                flt.selected.remove(p);
            } else {
                flt.selected.push(e.value.clone());
            }
            self.refresh();
        }
    }

    fn filter_remove_click(&mut self) {
        if let Some(i) = Self::item().filter(|i| *i < self.choice.filters.len()) {
            self.choice.filters.remove(i);
            self.refresh();
        }
    }

    fn filter_add_link_click(&mut self) {
        let first = self.setup.as_ref().and_then(|s| s.columns.first()).map(|c| c.idx).unwrap_or(0);
        self.choice.filters.push(PivotFilterSpec { field: first, selected: Vec::new() });
        self.refresh();
    }

    fn value_field_changed(&mut self, e: &TextChangedEventArgs) {
        if let (Some(i), Ok(f)) = (Self::item(), e.new.parse::<usize>()) {
            if let Some(v) = self.choice.values.get_mut(i) {
                v.field = f;
            }
        }
    }

    fn value_agg_changed(&mut self, e: &TextChangedEventArgs) {
        if let (Some(i), Some((a, _))) = (Self::item(), AGGS.iter().find(|(_, s)| *s == e.new)) {
            if let Some(v) = self.choice.values.get_mut(i) {
                v.agg = *a;
            }
        }
    }

    fn value_remove_click(&mut self) {
        if let Some(i) = Self::item().filter(|i| *i < self.choice.values.len() && self.choice.values.len() > 1) {
            self.choice.values.remove(i);
            self.refresh();
        }
    }

    fn value_add_link_click(&mut self) {
        let last = self.setup.as_ref().and_then(|s| s.columns.last()).map(|c| c.idx).unwrap_or(0);
        self.choice.values.push(PivotValueSpec { field: last, agg: PivotAgg::Sum });
        self.refresh();
    }

    fn build_button_click(&mut self) {
        if self.can_build {
            self.set_dialog_result(DialogResult::Ok);
            self.close();
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
    use kubuno_office_sheets_core::addr::Rect;
    use kubuno_office_sheets_core::pivot_table::PivotColumn;

    fn setup() -> PivotSetup {
        let col = |i: usize, l: &str, h: &str| PivotColumn { idx: i, letter: l.into(), header: h.into() };
        PivotSetup { range: Rect::new(0, 1, 2, 9), columns: vec![col(0, "A", "Région"), col(1, "B", ""), col(2, "C", "Montant")], label: "A1:C9".into(), target: "E1".into(), edit: None }
    }

    #[test]
    fn the_defaults_and_the_toggles_follow_the_web() {
        let mut d = PivotDialog { setup: Some(setup()), ..PivotDialog::default() };
        d.choice = PivotChoice { row_fields: vec![0], values: vec![PivotValueSpec { field: 2, agg: PivotAgg::Sum }], target: "E1".into(), ..PivotChoice::default() };
        d.refresh();
        assert!(d.can_build);
        assert_eq!(d.label(1), Resources::pv_col().replace("{l}", "B"));
        d.row_list_item_click(&ValueEventArgs { index: 0, value: String::new() });
        assert!(!d.can_build, "no row field: nothing to build");
        d.row_list_item_click(&ValueEventArgs { index: 1, value: String::new() });
        d.row_list_item_click(&ValueEventArgs { index: 0, value: String::new() });
        assert_eq!(d.choice.row_fields, vec![1, 0], "in the order checked");
        d.col_field = "2".into();
        d.target = " h3 ".into();
        let r = d.result();
        assert_eq!((r.col_field, r.target.as_str()), (Some(2), "H3"));
        d.filter_add_link_click();
        assert_eq!(d.choice.filters[0].field, 0);
        d.value_add_link_click();
        assert_eq!(d.choice.values.len(), 2);
    }
}
