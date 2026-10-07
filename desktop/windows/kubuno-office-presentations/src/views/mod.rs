//! The views (`.kbview`), each next to its code-behind: the presentation window, the slideshow and the dialogs.

pub mod background_dialog;
pub mod chart_dialog;
pub mod confirm_dialog;
pub mod geometry_dialog;
pub mod presentation_window;
pub mod prompt_dialog;
pub mod slideshow_window;

#[cfg(test)]
mod view_tests;
