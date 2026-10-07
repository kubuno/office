//! Code-behind of the chart wizard (`chart_wizard.kbview`, `web/src/sheet-chart/ChartWizard.tsx` and
//! `wizard/Step*.tsx`). It edits a copy of the persisted chart through the core's wizard operations
//! (`sheets-core::chart::wizard`), so unknown and legacy fields survive; « Terminer » hands the draft back.

use std::collections::HashMap;

use kubuno_desktop::prelude::*;
use kubuno_desktop::views::events::ChangeSource;
use kubuno_office_sheets_core::chart::data::CellVal;
use kubuno_office_sheets_core::chart::model::{is_cartesian, normalize_kind, series_color};
use kubuno_office_sheets_core::chart::wizard::{self as wz, catalog};
use kubuno_office_sheets_core::chart::{render_chart, ChartWords};
use serde_json::{json, Value as Json};

use crate::controls::chart_view::{ChartPickEventArgs, ChartPreview};
use crate::Resources;


/// The wizard (see the module doc).
#[kubuno_desktop::view("chart_wizard.kbview")]
pub struct ChartWizard {
    #[control]
    preview: Custom<ChartPreview>,
    #[bind]
    step_value: String,
    #[bind]
    show_type: bool,
    #[bind]
    show_range: bool,
    #[bind]
    show_series: bool,
    #[bind]
    show_elements: bool,
    #[bind]
    can_back: bool,
    #[bind]
    can_next: bool,
    #[bind]
    type_id: String,
    #[bind]
    subtype_id: String,
    #[bind]
    smooth: bool,
    #[bind]
    smooth_option: bool,
    #[bind]
    range_text: String,
    #[bind]
    explicit_hint: bool,
    #[bind]
    has_range: bool,
    #[bind]
    series_in: String,
    #[bind]
    first_row: bool,
    #[bind]
    first_col: bool,
    #[bind]
    series_rows: Rows,
    #[bind]
    series_index: i32,
    #[bind]
    has_series: bool,
    #[bind]
    series_name: String,
    #[bind]
    series_vals: String,
    #[bind]
    series_x: String,
    #[bind]
    is_scatter: bool,
    #[bind]
    series_color: String,
    #[bind]
    cats_text: String,
    #[bind]
    title_text: String,
    #[bind]
    x_title: String,
    #[bind]
    y_title: String,
    #[bind]
    cartesian: bool,
    #[bind]
    legend_on: bool,
    #[bind]
    legend_pos: String,
    #[bind]
    grid_x: bool,
    #[bind]
    grid_y: bool,
    #[bind]
    data_labels: String,
    draft: Json,
    cells: HashMap<String, CellVal>,
    lang: String,
    done: Option<Json>,
}

impl ChartWizard {
    fn get(&self) -> impl Fn(&str) -> CellVal + '_ {
        move |r: &str| self.cells.get(r).cloned().unwrap_or(CellVal::Str(String::new()))
    }

    /// Opens the wizard on `initial` at `step` (0 type, 1 range, 2 series, 3 elements); `cells` are the
    /// sheet's values by A1 ref. Returns the confirmed chart.
    pub fn run(owner: &dyn AsForm, initial: &Json, step: usize, cells: HashMap<String, CellVal>, lang: &str) -> Option<Json> {
        let mut w = Self { draft: initial.clone(), cells, lang: lang.to_string(), step_value: step.min(3).to_string(), series_index: 0, ..Self::default() };
        w.initialize_component();
        w.refresh();
        let _ = w.show_dialog(owner);
        w.done.take()
    }

    fn step(&self) -> usize {
        self.step_value.parse().unwrap_or(0)
    }

    fn set_draft(&mut self, d: Json) {
        self.draft = d;
        self.refresh();
    }

    /// Everything the view shows, from the draft.
    fn refresh(&mut self) {
        let step = self.step();
        self.show_type = step == 0;
        self.show_range = step == 1;
        self.show_series = step == 2;
        self.show_elements = step == 3;
        self.can_back = step > 0;
        self.can_next = step < 3;
        let d = self.draft.clone();
        let s = |k: &str| d.get(k).and_then(Json::as_str).unwrap_or("").to_string();
        // Type.
        let ty = wz::catalog_type_of(&d);
        self.type_id = ty.to_string();
        self.subtype_id = wz::subtype_of(&d).to_string();
        self.smooth_option = catalog().iter().find(|t| t.id == ty).is_some_and(|t| t.smooth_option);
        self.smooth = d.get("smooth").and_then(Json::as_bool).unwrap_or(false);
        // Range.
        self.range_text = s("range");
        self.has_range = !self.range_text.is_empty();
        self.explicit_hint = d.get("range").is_none() && (d.get("series").and_then(Json::as_array).is_some_and(|a| !a.is_empty()) || d.get("vals").and_then(Json::as_array).is_some_and(|a| !a.is_empty()));
        let (rows, fr, fc) = {
            let get = self.get();
            wz::range_layout(&d, &get)
        };
        self.series_in = if rows { "rows".into() } else { "cols".into() };
        self.first_row = fr;
        self.first_col = fc;
        // Series.
        let ex = {
            let get = self.get();
            wz::explicit_data(&d, &get)
        };
        let series_n = Resources::sheet_chart_series_n().to_string();
        let names: Vec<String> = {
            let get = self.get();
            ex.series.iter().enumerate().map(|(i, sr)| wz::series_display_name(sr, i, &get, &series_n.replace("{n}", "{{n}}"))).collect()
        };
        self.series_rows = Rows::from(names.into_iter().map(|n| Row::new().with("Text", Value::Str(n))).collect::<Vec<_>>());
        let n = ex.series.len();
        if self.series_index as usize >= n {
            self.series_index = n.saturating_sub(1) as i32;
        }
        self.has_series = n > 0;
        let cur = ex.series.get(self.series_index.max(0) as usize).cloned().unwrap_or(Json::Null);
        let cs = |k: &str| cur.get(k).and_then(Json::as_str).unwrap_or("").to_string();
        self.series_name = cs("name");
        self.series_vals = cs("valsRange");
        self.series_x = cs("xRange");
        self.series_color = cur.get("color").and_then(Json::as_str).map(str::to_string).unwrap_or_else(|| series_color(self.series_index.max(0) as usize, None));
        self.is_scatter = s("type") == "scatter";
        self.cats_text = ex.cats_range.clone().unwrap_or_default();
        // Elements.
        self.title_text = s("title");
        self.x_title = d.get("axisX").and_then(|a| a.get("title")).and_then(Json::as_str).unwrap_or("").to_string();
        self.y_title = d.get("axisY").and_then(|a| a.get("title")).and_then(Json::as_str).unwrap_or("").to_string();
        self.cartesian = is_cartesian(normalize_kind(d.get("type").and_then(Json::as_str)));
        self.legend_on = d.get("legend").and_then(Json::as_bool) != Some(false);
        self.legend_pos = d.get("legendPos").and_then(Json::as_str).unwrap_or("right").to_string();
        self.grid_x = d.get("grid").and_then(|g| g.get("x")).and_then(Json::as_bool).unwrap_or(false);
        self.grid_y = d.get("grid").and_then(|g| g.get("y")).and_then(Json::as_bool).unwrap_or(true);
        self.data_labels = d.get("dataLabels").and_then(Json::as_str).unwrap_or("none").to_string();
        // Preview (`{ ...draft, id: undefined }`).
        let mut pv = d.clone();
        if let Some(m) = pv.as_object_mut() {
            m.remove("id");
        }
        let words = ChartWords { series: Resources::chart_series().to_string(), no_data: Resources::chart_no_data().to_string() };
        let scene = {
            let get = self.get();
            render_chart(&pv, 234.0, 180.0, &get, &words, &self.lang)
        };
        self.preview.with(|p| p.set_scene(scene));
    }

    fn user(e: &TextChangedEventArgs) -> bool {
        e.source == ChangeSource::User
    }

    // ── Roadmap and buttons ─────────────────────────────────────────────────

    fn step_changed(&mut self, e: &CheckedChangedEventArgs) {
        if e.new {
            self.refresh();
        }
    }

    fn back_click(&mut self) {
        let s = self.step().saturating_sub(1);
        self.step_value = s.to_string();
        self.refresh();
    }

    fn next_click(&mut self) {
        let s = (self.step() + 1).min(3);
        self.step_value = s.to_string();
        self.refresh();
    }

    fn finish_click(&mut self) {
        self.done = Some(self.draft.clone());
        self.set_dialog_result(DialogResult::Ok);
        self.close();
    }

    fn cancel_click(&mut self) {
        self.set_dialog_result(DialogResult::Cancel);
        self.close();
    }

    // ── Step 1 ──────────────────────────────────────────────────────────────

    fn type_picked(&mut self, e: &ChartPickEventArgs) {
        let cat = catalog();
        if let Some(st) = cat.iter().find(|t| t.id == e.type_id).and_then(|t| t.subtypes.iter().find(|s| s.id == e.subtype)) {
            let d = wz::apply_subtype(&self.draft, st);
            self.set_draft(d);
        }
    }

    fn smooth_changed(&mut self, e: &CheckedChangedEventArgs) {
        let d = wz::set(&self.draft, "smooth", e.new.then_some(json!(true)));
        self.set_draft(d);
    }

    // ── Step 2 ──────────────────────────────────────────────────────────────

    fn range_changed(&mut self, e: &TextChangedEventArgs) {
        if Self::user(e) {
            self.draft = wz::set_range(&self.draft, e.new.trim());
            self.refresh();
        }
    }

    fn series_in_changed(&mut self, e: &CheckedChangedEventArgs) {
        if e.new && self.has_range {
            let v = self.series_in.clone();
            let d = wz::set(&self.draft, "seriesIn", Some(json!(v)));
            self.set_draft(d);
        }
    }

    fn first_row_changed(&mut self, e: &CheckedChangedEventArgs) {
        let d = wz::set(&self.draft, "firstRowHeader", Some(json!(e.new)));
        self.set_draft(d);
    }

    fn first_col_changed(&mut self, e: &CheckedChangedEventArgs) {
        let d = wz::set(&self.draft, "firstColHeader", Some(json!(e.new)));
        self.set_draft(d);
    }

    // ── Step 3 ──────────────────────────────────────────────────────────────

    fn explicit(&self) -> wz::Explicit {
        let get = self.get();
        wz::explicit_data(&self.draft, &get)
    }

    fn commit_series(&mut self, series: Vec<Json>, cats: Option<String>) {
        let d = wz::with_explicit_series(&self.draft, series, cats.as_deref());
        self.set_draft(d);
    }

    fn patch_sel(&mut self, patch: &[(&str, Option<Json>)]) {
        let ex = self.explicit();
        let i = self.series_index.max(0) as usize;
        let mut series = ex.series.clone();
        let Some(s) = series.get_mut(i).and_then(Json::as_object_mut) else { return };
        for (k, v) in patch {
            match v {
                Some(v) => {
                    s.insert((*k).into(), v.clone());
                }
                None => {
                    s.remove(*k);
                }
            }
        }
        self.commit_series(series, ex.cats_range);
    }

    fn series_selected(&mut self, _e: &SelectionChangedEventArgs) {
        self.refresh();
    }

    fn series_add_click(&mut self) {
        let ex = self.explicit();
        let mut series = ex.series.clone();
        series.push(json!({ "valsRange": "" }));
        self.series_index = (series.len() - 1) as i32;
        self.commit_series(series, ex.cats_range);
    }

    fn series_remove_click(&mut self) {
        let ex = self.explicit();
        let i = self.series_index.max(0) as usize;
        if i >= ex.series.len() {
            return;
        }
        let mut series = ex.series.clone();
        series.remove(i);
        self.series_index = (i as i32 - 1).max(0);
        self.commit_series(series, ex.cats_range);
    }

    fn move_series(&mut self, delta: i64) {
        let ex = self.explicit();
        let i = self.series_index.max(0) as usize;
        if let Some(next) = wz::move_item(&ex.series, i, delta) {
            self.series_index = (i as i64 + delta) as i32;
            self.commit_series(next, ex.cats_range);
        }
    }

    fn series_up_click(&mut self) {
        self.move_series(-1);
    }

    fn series_down_click(&mut self) {
        self.move_series(1);
    }

    fn series_name_changed(&mut self, e: &TextChangedEventArgs) {
        if Self::user(e) {
            let v = e.new.clone();
            let ex = self.explicit();
            let name_ref = ex.series.get(self.series_index.max(0) as usize).and_then(|s| s.get("nameRef")).cloned();
            self.patch_sel(&[("name", (!v.is_empty()).then(|| json!(v))), ("nameRef", if v.is_empty() { name_ref } else { None })]);
        }
    }

    fn series_vals_changed(&mut self, e: &TextChangedEventArgs) {
        if Self::user(e) {
            let v = e.new.trim().to_string();
            self.patch_sel(&[("valsRange", (!v.is_empty()).then(|| json!(v))), ("vals", None)]);
        }
    }

    fn series_x_changed(&mut self, e: &TextChangedEventArgs) {
        if Self::user(e) {
            let v = e.new.trim().to_string();
            self.patch_sel(&[("xRange", (!v.is_empty()).then(|| json!(v)))]);
        }
    }

    fn series_color_changed(&mut self, e: &TextChangedEventArgs) {
        let v = e.new.trim().to_string();
        if !v.is_empty() && Self::user(e) {
            self.patch_sel(&[("color", Some(json!(v)))]);
        }
    }

    fn cats_changed(&mut self, e: &TextChangedEventArgs) {
        if Self::user(e) {
            let ex = self.explicit();
            let v = e.new.trim().to_string();
            self.commit_series(ex.series, (!v.is_empty()).then_some(v));
        }
    }

    // ── Step 4 ──────────────────────────────────────────────────────────────

    fn title_changed(&mut self, e: &TextChangedEventArgs) {
        if Self::user(e) {
            let v = e.new.clone();
            self.draft = wz::set(&self.draft, "title", (!v.is_empty()).then(|| json!(v)));
            self.refresh();
        }
    }

    fn set_axis_title(&mut self, axis: &str, v: String) {
        let mut a = self.draft.get(axis).and_then(Json::as_object).cloned().unwrap_or_default();
        if v.is_empty() {
            a.remove("title");
        } else {
            a.insert("title".into(), json!(v));
        }
        self.draft = wz::set(&self.draft, axis, Some(Json::Object(a)));
        self.refresh();
    }

    fn xtitle_changed(&mut self, e: &TextChangedEventArgs) {
        if Self::user(e) {
            self.set_axis_title("axisX", e.new.clone());
        }
    }

    fn ytitle_changed(&mut self, e: &TextChangedEventArgs) {
        if Self::user(e) {
            self.set_axis_title("axisY", e.new.clone());
        }
    }

    fn legend_changed(&mut self, e: &CheckedChangedEventArgs) {
        let d = wz::set(&self.draft, "legend", Some(json!(e.new)));
        self.set_draft(d);
    }

    fn legend_pos_changed(&mut self, e: &CheckedChangedEventArgs) {
        if e.new {
            let v = self.legend_pos.clone();
            let d = wz::set(&self.draft, "legendPos", Some(json!(v)));
            self.set_draft(d);
        }
    }

    fn set_grid(&mut self, axis: &str, v: bool) {
        let mut g = self.draft.get("grid").and_then(Json::as_object).cloned().unwrap_or_default();
        g.insert(axis.into(), json!(v));
        let d = wz::set(&self.draft, "grid", Some(Json::Object(g)));
        self.set_draft(d);
    }

    fn grid_x_changed(&mut self, e: &CheckedChangedEventArgs) {
        self.set_grid("x", e.new);
    }

    fn grid_y_changed(&mut self, e: &CheckedChangedEventArgs) {
        self.set_grid("y", e.new);
    }

    fn dl_changed(&mut self, e: &CheckedChangedEventArgs) {
        if e.new {
            let v = self.data_labels.clone();
            let d = wz::set(&self.draft, "dataLabels", (v != "none").then(|| json!(v)));
            self.set_draft(d);
        }
    }
}
