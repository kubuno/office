//! Code-behind of the conditional-formatting dialog (`cf_dialog.kbview`): the web's
//! `ConditionalFormatDialog`. The rules are the sheet's stored blocks; adding or deleting one hands the whole
//! list to `apply` (the window's `persistCf`) at once and the dialog stays open, as on the web.

use kubuno_desktop::prelude::*;
use kubuno_office_sheets_core::dialogs::{self, CellOp, CfKind, CfSummary, ColorScale, Dxf, TextOp};
use serde_json::Value as Json;

use crate::Resources;

/// Applies the new list of blocks to the sheet.
pub type ApplyFn = Box<dyn FnMut(Vec<Json>)>;

#[kubuno_desktop::view("cf_dialog.kbview")]
pub struct CfDialog {
    #[bind]
    kind_box_options: Rows,
    #[bind]
    cell_op_box_options: Rows,
    #[bind]
    text_op_box_options: Rows,
    #[bind]
    empty_box_options: Rows,
    #[bind]
    top_side_box_options: Rows,
    #[bind]
    top_unit_box_options: Rows,
    #[bind]
    avg_box_options: Rows,
    #[bind]
    dup_box_options: Rows,
    #[bind]
    icon_set_box_options: Rows,
    #[bind]
    rule_rows: Rows,
    #[bind]
    range: String,
    #[bind]
    kind: String,
    #[bind]
    cell_op: String,
    #[bind]
    text_op: String,
    #[bind]
    v1: String,
    #[bind]
    v2: String,
    #[bind]
    empty_mode: String,
    #[bind]
    formula: String,
    #[bind]
    top_side: String,
    #[bind]
    top_rank: String,
    #[bind]
    top_unit: String,
    #[bind]
    avg_mode: String,
    #[bind]
    dup_mode: String,
    #[bind]
    icon_set: String,
    #[bind]
    bar_color: String,
    #[bind]
    scale_lo: String,
    #[bind]
    scale_mid: String,
    #[bind]
    scale_hi: String,
    #[bind]
    scale_toggle_text: String,
    #[bind]
    dxf_bg: String,
    #[bind]
    dxf_color: String,
    #[bind]
    dxf_bold: bool,
    #[bind]
    dxf_italic: bool,
    // What shows for the kind.
    #[bind]
    is_cell: bool,
    #[bind]
    is_cell_single: bool,
    #[bind]
    is_cell_between: bool,
    #[bind]
    is_text: bool,
    #[bind]
    is_empty: bool,
    #[bind]
    is_formula: bool,
    #[bind]
    is_top: bool,
    #[bind]
    is_avg: bool,
    #[bind]
    is_dupes: bool,
    #[bind]
    is_bar: bool,
    #[bind]
    is_icons: bool,
    #[bind]
    is_scale: bool,
    #[bind]
    has_mid: bool,
    #[bind]
    show_dxf: bool,
    #[bind]
    bar0: bool,
    #[bind]
    bar1: bool,
    #[bind]
    bar2: bool,
    #[bind]
    bar3: bool,
    #[bind]
    bar4: bool,
    #[bind]
    scale0: bool,
    #[bind]
    scale1: bool,
    #[bind]
    scale2: bool,
    #[bind]
    scale3: bool,
    #[bind]
    preset0: bool,
    #[bind]
    preset1: bool,
    #[bind]
    preset2: bool,
    #[bind]
    preset3: bool,
    #[bind]
    preset4: bool,
    blocks: Vec<Json>,
    dxf: Dxf,
    /// The preset the style came from (the web compares the object: any edit leaves the preset).
    preset: Option<usize>,
    scale: ColorScale,
    /// The scale preset picked (`scale === s.cs`).
    scale_preset: Option<usize>,
    apply: Option<ApplyFn>,
}

impl CfDialog {
    /// The dropdowns' options (resources: see [`super::options`]).
    fn fill_options(&mut self) {
        self.kind_box_options = super::options(&[("cell", Resources::cf_k_cell()), ("text", Resources::cf_k_text()), ("empty", Resources::cf_k_empty()), ("formula", Resources::cf_k_formula()), ("top", Resources::cf_k_top()), ("avg", Resources::cf_k_avg()), ("dupes", Resources::cf_k_dupes()), ("scale", Resources::cf_k_scale()), ("bar", Resources::cf_k_bar()), ("icons", Resources::cf_k_icons())]);
        self.cell_op_box_options = super::options(&[("gt", Resources::cf_op_gt()), ("lt", Resources::cf_op_lt()), ("ge", Resources::cf_op_ge()), ("le", Resources::cf_op_le()), ("eq", Resources::cf_op_eq()), ("ne", Resources::cf_op_ne()), ("between", Resources::cf_op_between())]);
        self.text_op_box_options = super::options(&[("contains", Resources::cf_t_contains()), ("ncontains", Resources::cf_t_ncontains()), ("starts", Resources::cf_t_starts()), ("ends", Resources::cf_t_ends())]);
        self.empty_box_options = super::options(&[("empty", Resources::cf_is_empty()), ("notempty", Resources::cf_is_notempty())]);
        self.top_side_box_options = super::options(&[("top", Resources::cf_top()), ("bottom", Resources::cf_bottom())]);
        self.top_unit_box_options = super::options(&[("values", Resources::cf_unit_values()), ("percent", "%")]);
        self.avg_box_options = super::options(&[("above", Resources::cf_above_avg()), ("below", Resources::cf_below_avg())]);
        self.dup_box_options = super::options(&[("dup", Resources::cf_dup()), ("uniq", Resources::cf_uniq())]);
        self.icon_set_box_options = super::options(&[("3Arrows", Resources::cf_set_3arrows()), ("3TrafficLights1", Resources::cf_set_3lights()), ("3Symbols2", Resources::cf_set_3symbols()), ("3Flags", Resources::cf_set_3flags()), ("4Arrows", Resources::cf_set_4arrows()), ("4Rating", Resources::cf_set_4rating()), ("5Arrows", Resources::cf_set_5arrows()), ("5Rating", Resources::cf_set_5rating()), ("5Quarters", Resources::cf_set_5quarters())]);
    }

    /// The dialog over `blocks`, the new rule's range defaulting to `selection` (`selectionRefNorm`).
    pub fn new(blocks: Vec<Json>, selection: &str, apply: ApplyFn) -> Self {
        let presets = dialogs::cf_dxf_presets();
        let scales = dialogs::cf_scale_presets();
        let mut d = Self {
            range: if selection.is_empty() { "A1".into() } else { selection.to_string() },
            kind: "cell".into(),
            cell_op: "gt".into(),
            text_op: "contains".into(),
            empty_mode: "empty".into(),
            top_side: "top".into(),
            top_rank: "10".into(),
            top_unit: "values".into(),
            avg_mode: "above".into(),
            dup_mode: "dup".into(),
            icon_set: "3Arrows".into(),
            bar_color: dialogs::CF_BAR_COLORS[0].into(),
            dxf: presets[0].1.clone(),
            preset: Some(0),
            scale: scales[0].clone(),
            scale_preset: Some(0),
            blocks,
            apply: Some(apply),
            ..Self::default()
        };
        d.fill_options();
        d.refresh_rows();
        d.refresh_kind();
        d.refresh_style();
        d.initialize_component();
        d
    }

    /// Shows it over `owner` until « Fermer ».
    pub fn edit(owner: &dyn AsForm, blocks: Vec<Json>, selection: &str, apply: ApplyFn) {
        let mut d = Self::new(blocks, selection, apply);
        d.show_dialog(owner);
    }

    fn summary_text(s: &CfSummary) -> String {
        match s {
            CfSummary::None => "—".into(),
            CfSummary::Scale => Resources::cf_sum_scale().into(),
            CfSummary::Bar => Resources::cf_sum_bar().into(),
            CfSummary::Icons => Resources::cf_sum_icons().into(),
            CfSummary::Top { bottom, rank, percent } => format!("{} {rank}{}", if *bottom { Resources::cf_bottom() } else { Resources::cf_top() }, if *percent { " %" } else { "" }),
            CfSummary::AboveAverage => Resources::cf_above_avg().into(),
            CfSummary::BelowAverage => Resources::cf_below_avg().into(),
            CfSummary::Duplicates => Resources::cf_dup().into(),
            CfSummary::Unique => Resources::cf_uniq().into(),
            CfSummary::Period(p) => p.clone(),
            CfSummary::Text(t) => t.clone(),
        }
    }

    fn refresh_rows(&mut self) {
        self.rule_rows = Rows::from(
            self.blocks
                .iter()
                .map(|b| {
                    Row::new()
                        .with("Swatch", Value::Str(crate::controls::swatch::spec_of(&dialogs::cf_swatch(b))))
                        .with("Ranges", Value::Str(dialogs::block_ranges(b)))
                        .with("Summary", Value::Str(Self::summary_text(&dialogs::cf_summary(b))))
                })
                .collect::<Vec<_>>(),
        );
    }

    fn refresh_kind(&mut self) {
        let k = self.kind.as_str();
        self.is_cell = k == "cell";
        self.is_cell_between = self.is_cell && self.cell_op == "between";
        self.is_cell_single = self.is_cell && !self.is_cell_between;
        self.is_text = k == "text";
        self.is_empty = k == "empty";
        self.is_formula = k == "formula";
        self.is_top = k == "top";
        self.is_avg = k == "avg";
        self.is_dupes = k == "dupes";
        self.is_bar = k == "bar";
        self.is_icons = k == "icons";
        self.is_scale = k == "scale";
        self.show_dxf = !matches!(k, "scale" | "bar" | "icons");
        let bar = dialogs::CF_BAR_COLORS.iter().position(|c| *c == self.bar_color);
        (self.bar0, self.bar1, self.bar2, self.bar3, self.bar4) = (bar == Some(0), bar == Some(1), bar == Some(2), bar == Some(3), bar == Some(4));
        let s = self.scale_preset;
        (self.scale0, self.scale1, self.scale2, self.scale3) = (s == Some(0), s == Some(1), s == Some(2), s == Some(3));
        self.scale_lo = self.scale.lo.clone();
        self.scale_hi = self.scale.hi.clone();
        self.scale_mid = self.scale.mid.clone().unwrap_or_default();
        self.has_mid = self.is_scale && self.scale.mid.is_some();
        self.scale_toggle_text = if self.scale.mid.is_some() { Resources::cf_2color().into() } else { Resources::cf_3color().into() };
    }

    fn refresh_style(&mut self) {
        self.dxf_bg = self.dxf.bg.clone().unwrap_or_else(|| "#ffffff".into());
        self.dxf_color = self.dxf.color.clone().unwrap_or_else(|| "#000000".into());
        self.dxf_bold = self.dxf.bold == Some(true);
        self.dxf_italic = self.dxf.italic == Some(true);
        let p = self.preset;
        (self.preset0, self.preset1, self.preset2, self.preset3, self.preset4) = (p == Some(0), p == Some(1), p == Some(2), p == Some(3), p == Some(4));
    }

    /// The rule the inputs describe (`buildRule`).
    fn current_kind(&self) -> CfKind {
        let cell_op = match self.cell_op.as_str() {
            "lt" => CellOp::Lt,
            "ge" => CellOp::Ge,
            "le" => CellOp::Le,
            "eq" => CellOp::Eq,
            "ne" => CellOp::Ne,
            "between" => CellOp::Between,
            _ => CellOp::Gt,
        };
        let text_op = match self.text_op.as_str() {
            "ncontains" => TextOp::NotContains,
            "starts" => TextOp::Starts,
            "ends" => TextOp::Ends,
            _ => TextOp::Contains,
        };
        match self.kind.as_str() {
            "text" => CfKind::Text { op: text_op, v: self.v1.clone() },
            "empty" => CfKind::Empty { empty: self.empty_mode == "empty" },
            "formula" => CfKind::Formula(self.formula.clone()),
            "top" => CfKind::Top { bottom: self.top_side == "bottom", rank: self.top_rank.clone(), percent: self.top_unit == "percent" },
            "avg" => CfKind::Average { above: self.avg_mode != "below" },
            "dupes" => CfKind::Duplicates { duplicate: self.dup_mode == "dup" },
            "scale" => CfKind::Scale(self.scale.clone()),
            "bar" => CfKind::Bar { color: self.bar_color.clone() },
            "icons" => CfKind::Icons { set: self.icon_set.clone() },
            _ => CfKind::Cell { op: cell_op, v1: self.v1.clone(), v2: self.v2.clone() },
        }
    }

    fn set_blocks(&mut self, blocks: Vec<Json>) {
        if let Some(apply) = self.apply.as_mut() {
            apply(blocks.clone());
        }
        self.blocks = blocks;
        self.refresh_rows();
    }

    fn pick_preset(&mut self, i: usize) {
        if let Some((_, dxf)) = dialogs::cf_dxf_presets().into_iter().nth(i) {
            self.dxf = dxf;
            self.preset = Some(i);
            self.refresh_style();
        }
    }

    fn pick_bar(&mut self, i: usize) {
        self.bar_color = dialogs::CF_BAR_COLORS[i].into();
        self.refresh_kind();
    }

    fn pick_scale(&mut self, i: usize) {
        if let Some(s) = dialogs::cf_scale_presets().into_iter().nth(i) {
            self.scale = s;
            self.scale_preset = Some(i);
            self.refresh_kind();
        }
    }

    fn kind_box_changed(&mut self, e: &TextChangedEventArgs) {
        self.kind = e.new.clone();
        self.refresh_kind();
    }

    fn cell_op_box_changed(&mut self, e: &TextChangedEventArgs) {
        self.cell_op = e.new.clone();
        self.refresh_kind();
    }

    fn rule_delete_click(&mut self) {
        let Some(item) = kubuno_desktop::views::binding::current_item() else { return };
        let mut blocks = self.blocks.clone();
        if item.index < blocks.len() {
            blocks.remove(item.index);
            self.set_blocks(blocks);
        }
    }

    fn add_button_click(&mut self) {
        let Some(rule) = dialogs::build_cf_rule(&self.current_kind(), &self.range, &self.dxf) else { return };
        if let Some(blocks) = dialogs::cf_add(&self.blocks, &self.range, rule) {
            self.set_blocks(blocks);
        }
    }

    fn close_button_click(&mut self) {
        self.set_dialog_result(DialogResult::Ok);
        self.close();
    }

    fn bar_0_click(&mut self) {
        self.pick_bar(0);
    }

    fn bar_1_click(&mut self) {
        self.pick_bar(1);
    }

    fn bar_2_click(&mut self) {
        self.pick_bar(2);
    }

    fn bar_3_click(&mut self) {
        self.pick_bar(3);
    }

    fn bar_4_click(&mut self) {
        self.pick_bar(4);
    }

    fn bar_color_changed(&mut self, e: &TextChangedEventArgs) {
        if !e.new.trim().is_empty() {
            self.bar_color = e.new.trim().to_string();
            self.refresh_kind();
        }
    }

    fn scale_0_click(&mut self) {
        self.pick_scale(0);
    }

    fn scale_1_click(&mut self) {
        self.pick_scale(1);
    }

    fn scale_2_click(&mut self) {
        self.pick_scale(2);
    }

    fn scale_3_click(&mut self) {
        self.pick_scale(3);
    }

    fn scale_lo_changed(&mut self, e: &TextChangedEventArgs) {
        if !e.new.trim().is_empty() {
            self.scale.lo = e.new.trim().to_string();
            self.scale_preset = None;
            self.refresh_kind();
        }
    }

    fn scale_mid_changed(&mut self, e: &TextChangedEventArgs) {
        if !e.new.trim().is_empty() && self.scale.mid.is_some() {
            self.scale.mid = Some(e.new.trim().to_string());
            self.scale_preset = None;
            self.refresh_kind();
        }
    }

    fn scale_hi_changed(&mut self, e: &TextChangedEventArgs) {
        if !e.new.trim().is_empty() {
            self.scale.hi = e.new.trim().to_string();
            self.scale_preset = None;
            self.refresh_kind();
        }
    }

    fn scale_toggle_click(&mut self) {
        self.scale.mid = if self.scale.mid.is_some() { None } else { Some("#ffffff".into()) };
        self.scale_preset = None;
        self.refresh_kind();
    }

    fn preset_0_click(&mut self) {
        self.pick_preset(0);
    }

    fn preset_1_click(&mut self) {
        self.pick_preset(1);
    }

    fn preset_2_click(&mut self) {
        self.pick_preset(2);
    }

    fn preset_3_click(&mut self) {
        self.pick_preset(3);
    }

    fn preset_4_click(&mut self) {
        self.pick_preset(4);
    }

    fn fill_changed(&mut self, e: &TextChangedEventArgs) {
        if !e.new.trim().is_empty() {
            self.dxf.bg = Some(e.new.trim().to_string());
            self.preset = None;
            self.refresh_style();
        }
    }

    fn text_color_changed(&mut self, e: &TextChangedEventArgs) {
        if !e.new.trim().is_empty() {
            self.dxf.color = Some(e.new.trim().to_string());
            self.preset = None;
            self.refresh_style();
        }
    }

    fn bold_toggle_click(&mut self) {
        self.dxf.bold = Some(self.dxf.bold != Some(true));
        self.preset = None;
        self.refresh_style();
    }

    fn italic_toggle_click(&mut self) {
        self.dxf.italic = Some(self.dxf.italic != Some(true));
        self.preset = None;
        self.refresh_style();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn adding_and_deleting_rules_applies_the_whole_list() {
        let seen: Rc<RefCell<Vec<Vec<Json>>>> = Rc::default();
        let sink = seen.clone();
        let mut d = CfDialog { range: "B2:B9".into(), kind: "cell".into(), cell_op: "gt".into(), v1: "5".into(), dxf: dialogs::cf_dxf_presets()[0].1.clone(), apply: Some(Box::new(move |b| sink.borrow_mut().push(b))), ..CfDialog::default() };
        d.add_button_click();
        assert_eq!(d.blocks.len(), 1);
        assert_eq!(d.blocks[0]["rules"][0]["formulas"][0], "=B2>5");
        assert_eq!(seen.borrow().len(), 1);
        // A missing value adds nothing.
        d.v1.clear();
        d.add_button_click();
        assert_eq!(d.blocks.len(), 1);
        d.kind = "scale".into();
        d.scale = dialogs::cf_scale_presets()[3].clone();
        d.add_button_click();
        assert_eq!(d.blocks[0]["type"], Json::Null);
        assert_eq!(d.blocks[0]["rules"][0]["type"], "colorScale");
        assert_eq!(seen.borrow().last().map(Vec::len), Some(2));
    }

    #[test]
    fn the_kind_decides_what_shows() {
        let mut d = CfDialog { kind: "cell".into(), cell_op: "between".into(), ..CfDialog::default() };
        d.refresh_kind();
        assert!(d.is_cell_between && !d.is_cell_single && d.show_dxf);
        d.kind = "bar".into();
        d.bar_color = "#ff555a".into();
        d.refresh_kind();
        assert!(d.is_bar && !d.show_dxf && d.bar2 && !d.bar0);
    }
}
