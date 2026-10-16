//! Code-behind of the review dialog (`review_dialog.kbview`): shows one revision and returns what
//! the user asked for; the document window applies it and opens the dialog again.

use kubuno_desktop::prelude::*;

use crate::Resources;

#[kubuno_desktop::view("review_dialog.kbview")]
pub struct ReviewDialog {
    action: ReviewAction,
}

/// What the user asked for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReviewAction {
    Prev,
    Next,
    Accept,
    Reject,
    AcceptAll,
    RejectAll,
    /// Fermer, Échap or the close box.
    #[default]
    Close,
}

/// What the dialog shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReviewView {
    pub count: String,
    pub head: String,
    pub body: String,
    pub has_items: bool,
}

impl ReviewDialog {
    pub fn new(view: &ReviewView) -> Self {
        let mut d = Self::default();
        d.initialize_component();
        // Texts set once from code, as the confirmation dialog does: the count goes in the title.
        let title = if view.count.is_empty() { Resources::cmd_reviewpane().to_string() } else { format!("{} - {}", Resources::cmd_reviewpane(), view.count) };
        d.set_text(title);
        d.head_label.set_text(view.head.clone());
        d.body_label.set_text(view.body.clone());
        d
    }

    /// Shows `view` and returns the action chosen.
    pub fn ask(owner: &dyn AsForm, view: &ReviewView) -> ReviewAction {
        let mut d = Self::new(view);
        d.show_dialog(owner);
        d.action
    }

    fn finish(&mut self, action: ReviewAction) {
        self.action = action;
        self.set_dialog_result(if action == ReviewAction::Close { DialogResult::Cancel } else { DialogResult::Ok });
        self.close();
    }

    fn prev_click(&mut self) {
        self.finish(ReviewAction::Prev);
    }

    fn next_click(&mut self) {
        self.finish(ReviewAction::Next);
    }

    fn accept_click(&mut self) {
        self.finish(ReviewAction::Accept);
    }

    fn reject_click(&mut self) {
        self.finish(ReviewAction::Reject);
    }

    fn accept_all_click(&mut self) {
        self.finish(ReviewAction::AcceptAll);
    }

    fn reject_all_click(&mut self) {
        self.finish(ReviewAction::RejectAll);
    }

    fn close_click(&mut self) {
        self.finish(ReviewAction::Close);
    }
}
