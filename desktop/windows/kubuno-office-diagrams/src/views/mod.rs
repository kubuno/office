//! The top-level views (`.kbview`): the diagram window and its dialogs, each next to its code-behind.

pub mod confirm_dialog;
pub mod diagram_window;
pub mod prompt_dialog;
pub mod style_dialog;
pub mod templates_dialog;

#[cfg(test)]
mod view_tests;
