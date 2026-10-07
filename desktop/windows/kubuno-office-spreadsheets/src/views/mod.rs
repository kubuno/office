//! The views (`.kbview`): the spreadsheet window and its dialogs, each next to its code-behind.

pub mod chart_wizard;
pub mod confirm_dialog;
pub mod object_format_dialog;
pub mod print_dialog;
pub mod prompt_dialog;
pub mod spreadsheet_window;

#[cfg(test)]
mod view_tests;
