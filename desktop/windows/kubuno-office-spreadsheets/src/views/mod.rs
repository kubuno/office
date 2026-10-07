//! The views (`.kbview`): the spreadsheet window and its dialogs, each next to its code-behind.

pub mod chart_wizard;
pub mod alert_dialog;
pub mod cf_dialog;
pub mod confirm_dialog;
pub mod object_format_dialog;
pub mod equation_dialog;
pub mod print_dialog;
pub mod dv_dialog;
pub mod goalseek_dialog;
pub mod names_dialog;
pub mod password_dialog;
pub mod pivot_dialog;
pub mod prompt_dialog;
pub mod sort_dialog;
pub mod spreadsheet_window;

#[cfg(test)]
mod view_tests;

/// The options of a bound `Dropdown` (`ItemsSource`, `DisplayMember="Label"`, `ValueMember="Value"`): the labels
/// are resources, which an `<Option Label>` cannot name.
pub(crate) fn options(items: &[(&str, &str)]) -> kubuno_desktop::prelude::Rows {
    use kubuno_desktop::prelude::{Row, Rows, Value};
    Rows::from(items.iter().map(|(v, l)| Row::new().with("Value", Value::Str((*v).to_string())).with("Label", Value::Str((*l).to_string()))).collect::<Vec<_>>())
}
