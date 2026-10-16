//! Code-behind of the user control `FilterPopup` (`filter_popup.kbcontrol`): the column filter of the web's
//! `ColumnFilterPopup` - the values start all checked (or as the column's filter left them), the search only
//! narrows what is listed, OK applies (every value checked = no filter) and closes.

use std::collections::HashSet;

use kubuno_desktop::views::prelude::*;

use crate::controls::value_list::{encode_list, ValueEventArgs};

/// What OK applies: `all` = every value checked (the filter goes), else the checked `values` (a list property:
/// [`crate::controls::value_list::decode_list`]).
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct FilterEventArgs {
    pub all: bool,
    pub values: String,
}

/// The column filter (see the module doc).
#[derive(UserControl, Default)]
#[user_control(view = "filter_popup.kbcontrol")]
#[category("Spreadsheets")]
#[toolbox(icon = "filter")]
#[default_event("Applied")]
pub struct FilterPopup {
    base: UserControlCore,
    /// OK was clicked.
    #[event]
    #[category("Action")]
    pub applied: Event<FilterEventArgs>,
    /// OK or Annuler asks to close.
    #[event]
    #[category("Action")]
    pub close_requested: Event<EmptyEventArgs>,
    #[property(bindable)]
    #[browsable(false)]
    pub query: String,
    #[property(bindable)]
    #[browsable(false)]
    pub shown_items: String,
    #[property(bindable)]
    #[browsable(false)]
    pub checked_items: String,
    values: Vec<String>,
    checked: HashSet<String>,
}

impl FilterPopup {
    /// The column's values and the current filter (`None`: all checked).
    pub fn open(&mut self, values: Vec<String>, allowed: Option<HashSet<String>>) {
        self.checked = allowed.unwrap_or_else(|| values.iter().cloned().collect());
        self.values = values;
        self.query.clear();
        self.refresh();
    }

    fn refresh(&mut self) {
        // `v.toLowerCase().includes(q.toLowerCase())`.
        let q = self.query.to_lowercase();
        let shown: Vec<&String> = self.values.iter().filter(|v| v.to_lowercase().contains(&q)).collect();
        let checked: Vec<&String> = shown.iter().copied().filter(|v| self.checked.contains(*v)).collect();
        self.shown_items = encode_list(&shown);
        self.checked_items = encode_list(&checked);
    }

    /// What OK applies (see [`FilterEventArgs`]).
    pub fn result(&self) -> FilterEventArgs {
        let all = self.values.iter().all(|v| self.checked.contains(v)) && self.checked.len() == self.values.iter().collect::<HashSet<_>>().len();
        let mut values: Vec<&String> = self.checked.iter().collect();
        values.sort();
        FilterEventArgs { all, values: encode_list(&values) }
    }
}

#[kubuno_desktop::views::event_handlers]
impl FilterPopup {
    fn search_box_text_changed(&mut self, e: &TextChangedEventArgs) {
        self.query = e.new.clone();
        self.refresh();
    }

    fn all_link_click(&mut self) {
        self.checked = self.values.iter().cloned().collect();
        self.refresh();
    }

    fn clear_link_click(&mut self) {
        self.checked.clear();
        self.refresh();
    }

    fn values_item_click(&mut self, e: &ValueEventArgs) {
        if !self.checked.remove(&e.value) {
            self.checked.insert(e.value.clone());
        }
        self.refresh();
    }

    fn ok_button_click(&mut self) {
        let r = self.result();
        self.raise_applied(r);
        self.raise_close_requested(EmptyEventArgs);
    }

    fn cancel_button_click(&mut self) {
        self.raise_close_requested(EmptyEventArgs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controls::value_list::decode_list;

    #[test]
    fn the_search_narrows_the_list_and_ok_applies_the_checks() {
        let mut p = FilterPopup::default();
        p.open(vec![String::new(), "Paris".into(), "Lyon".into()], None);
        assert!(p.result().all, "every value checked: no filter");
        p.query = "LY".into();
        p.refresh();
        assert_eq!(decode_list(&p.shown_items), vec!["Lyon"]);
        p.values_item_click(&ValueEventArgs { index: 0, value: "Lyon".into() });
        let r = p.result();
        assert!(!r.all);
        assert_eq!(decode_list(&r.values), vec!["", "Paris"]);
        p.clear_link_click();
        assert!(decode_list(&p.result().values).is_empty());
    }
}
