//! Code-behind of the confirmation dialog (`confirm_dialog.kbview`): a title, a message and two
//! answers. Never a system message box: the project's questions look like the rest of Kubuno.
//!
//! Closing the dialog (its ✕, Alt+F4, Escape) is **not** the second answer: both questions it asks
//! (settle a conflict, delete a project) have a destructive second answer, which a dismissal must
//! never pick. [`ConfirmDialog::choose`] tells the three apart.

use kubuno_desktop::prelude::*;

#[kubuno_desktop::view("confirm_dialog.kbview")]
pub struct ConfirmDialog {}

/// What the user did with the question.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    /// The first (primary) answer.
    First,
    /// The second answer, clicked.
    Second,
    /// Closed without answering.
    Dismissed,
}

impl Answer {
    /// The answer a dialog result stands for (the second button closes with `No`, a dismissal with
    /// anything else).
    pub fn of(result: DialogResult) -> Self {
        match result {
            DialogResult::Ok => Answer::First,
            DialogResult::No => Answer::Second,
            _ => Answer::Dismissed,
        }
    }
}

impl ConfirmDialog {
    pub fn new(title: &str, message: &str, confirm: &str, decline: &str) -> Self {
        let mut dialog = Self::default();
        dialog.initialize_component();
        dialog.set_text(title.to_string());
        dialog.message.set_text(message.to_string());
        dialog.confirm.set_text(confirm.to_string());
        dialog.cancel.set_text(decline.to_string());
        dialog
    }

    /// Shows it over `owner` and returns what the user did.
    pub fn choose(owner: &dyn AsForm, title: &str, message: &str, confirm: &str, decline: &str) -> Answer {
        let mut d = Self::new(title, message, confirm, decline);
        Answer::of(d.show_dialog(owner))
    }

    fn cancel_click(&mut self) {
        self.set_dialog_result(DialogResult::No);
        self.close();
    }

    fn confirm_click(&mut self) {
        self.set_dialog_result(DialogResult::Ok);
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dismissal_is_never_the_second_answer() {
        assert_eq!(Answer::of(DialogResult::Ok), Answer::First);
        assert_eq!(Answer::of(DialogResult::No), Answer::Second);
        assert_eq!(Answer::of(DialogResult::Cancel), Answer::Dismissed);
        assert_eq!(Answer::of(DialogResult::None), Answer::Dismissed);
    }
}
