//! Code-behind of the data-validation dialog (`dv_dialog.kbview`): the web's `DataValidationDialog`. Adding or
//! deleting a rule hands the whole list to `apply` (the window's `persistValidations`) and the dialog stays
//! open, as on the web.

use kubuno_desktop::prelude::*;
use kubuno_office_sheets_core::dialogs::{self, DvInput, DvKind};
use kubuno_office_sheets_core::validation::NumOp;
use serde_json::Value as Json;

use crate::Resources;

/// Applies the new list of blocks to the sheet.
pub type ApplyFn = Box<dyn FnMut(Vec<Json>)>;

#[kubuno_desktop::view("dv_dialog.kbview")]
pub struct DvDialog {
    #[bind]
    kind_box_options: Rows,
    #[bind]
    op_box_options: Rows,
    #[bind]
    rule_rows: Rows,
    #[bind]
    range: String,
    #[bind]
    kind: String,
    #[bind]
    list_text: String,
    #[bind]
    source: String,
    #[bind]
    op: String,
    #[bind]
    v1: String,
    #[bind]
    v2: String,
    #[bind]
    strict: bool,
    #[bind]
    dropdown: bool,
    #[bind]
    reject: bool,
    #[bind]
    help: String,
    #[bind]
    is_list: bool,
    #[bind]
    is_list_range: bool,
    #[bind]
    is_any_list: bool,
    #[bind]
    needs_range: bool,
    #[bind]
    is_between: bool,
    #[bind]
    is_single: bool,
    blocks: Vec<Json>,
    apply: Option<ApplyFn>,
}

impl DvDialog {
    /// The dropdowns' options (resources: see [`super::options`]).
    fn fill_options(&mut self) {
        self.kind_box_options = super::options(&[("list", Resources::dv_k_list()), ("listRange", Resources::dv_k_listrange()), ("checkbox", Resources::dv_k_checkbox()), ("number", Resources::dv_k_number()), ("textLen", Resources::dv_k_textlen())]);
        self.op_box_options = super::options(&[("between", Resources::dv_op_between()), ("notBetween", Resources::dv_op_nbetween()), ("gt", Resources::dv_op_gt()), ("lt", Resources::dv_op_lt()), ("ge", Resources::dv_op_ge()), ("le", Resources::dv_op_le()), ("eq", Resources::dv_op_eq()), ("ne", Resources::dv_op_ne())]);
    }

    pub fn new(blocks: Vec<Json>, selection: &str, apply: ApplyFn) -> Self {
        let mut d = Self {
            range: if selection.is_empty() { "A1".into() } else { selection.to_string() },
            kind: "list".into(),
            op: "between".into(),
            strict: true,
            dropdown: true,
            reject: true,
            blocks,
            apply: Some(apply),
            ..Self::default()
        };
        d.fill_options();
        d.refresh_rows();
        d.refresh_kind();
        d.initialize_component();
        d
    }

    /// Shows it over `owner` until « Fermer ».
    pub fn edit(owner: &dyn AsForm, blocks: Vec<Json>, selection: &str, apply: ApplyFn) {
        let mut d = Self::new(blocks, selection, apply);
        d.show_dialog(owner);
    }

    fn refresh_rows(&mut self) {
        self.rule_rows = Rows::from(self.blocks.iter().map(|b| Row::new().with("Ranges", Value::Str(dialogs::block_ranges(b))).with("Summary", Value::Str(dialogs::dv_summary(b)))).collect::<Vec<_>>());
    }

    fn refresh_kind(&mut self) {
        let k = self.kind.as_str();
        self.is_list = k == "list";
        self.is_list_range = k == "listRange";
        self.is_any_list = self.is_list || self.is_list_range;
        self.needs_range = k == "number" || k == "textLen";
        let between = self.op == "between" || self.op == "notBetween";
        self.is_between = self.needs_range && between;
        self.is_single = self.needs_range && !between;
    }

    fn input(&self) -> DvInput {
        let kind = match self.kind.as_str() {
            "listRange" => DvKind::ListRange,
            "checkbox" => DvKind::Checkbox,
            "number" => DvKind::Number,
            "textLen" => DvKind::TextLength,
            _ => DvKind::List,
        };
        let op = serde_json::from_value::<NumOp>(Json::String(self.op.clone())).unwrap_or(NumOp::Between);
        DvInput {
            range: self.range.clone(),
            kind,
            list_text: self.list_text.clone(),
            source: self.source.clone(),
            op,
            v1: self.v1.clone(),
            v2: self.v2.clone(),
            strict: self.strict,
            dropdown: self.dropdown,
            reject: self.reject,
            help: self.help.clone(),
        }
    }

    fn set_blocks(&mut self, blocks: Vec<Json>) {
        if let Some(apply) = self.apply.as_mut() {
            apply(blocks.clone());
        }
        self.blocks = blocks;
        self.refresh_rows();
    }

    fn kind_box_changed(&mut self, e: &TextChangedEventArgs) {
        self.kind = e.new.clone();
        self.refresh_kind();
    }

    fn op_box_changed(&mut self, e: &TextChangedEventArgs) {
        self.op = e.new.clone();
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
        if let Some(blocks) = dialogs::dv_add(&self.blocks, &self.input()) {
            self.set_blocks(blocks);
        }
    }

    fn close_button_click(&mut self) {
        self.set_dialog_result(DialogResult::Ok);
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_criterion_decides_what_shows_and_what_is_added() {
        let mut d = DvDialog { range: "C1:C9".into(), kind: "number".into(), op: "gt".into(), v1: "3".into(), strict: true, dropdown: true, reject: true, ..DvDialog::default() };
        d.refresh_kind();
        assert!(d.needs_range && d.is_single && !d.is_any_list);
        d.add_button_click();
        assert_eq!(d.blocks[0]["rule"]["crit"]["op"], "gt");
        assert_eq!(d.blocks[0]["ranges"][0], "C1:C9");
        d.kind = "listRange".into();
        d.refresh_kind();
        assert!(d.is_any_list && d.is_list_range && !d.needs_range);
    }
}
