//! Code-behind of the note editor (`note_dialog.kbview`): the plain text of a footnote or an
//! endnote, or its deletion.

use kubuno_desktop::prelude::*;

#[kubuno_desktop::view("note_dialog.kbview")]
pub struct NoteDialog {
    #[bind]
    note_text: String,
}

/// What the user did with the note.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NoteAnswer {
    /// OK, with this text.
    Text(String),
    /// « Supprimer la note ».
    Delete,
    /// Annuler, or closed.
    Cancel,
}

impl NoteAnswer {
    /// The answer a dialog result stands for (`Abort` is the delete button).
    pub fn of(result: DialogResult, text: &str) -> Self {
        match result {
            DialogResult::Ok => NoteAnswer::Text(text.to_string()),
            DialogResult::Abort => NoteAnswer::Delete,
            _ => NoteAnswer::Cancel,
        }
    }
}

impl NoteDialog {
    pub fn new(title: &str, text: &str) -> Self {
        let mut dialog = Self { note_text: text.to_string(), ..Self::default() };
        dialog.initialize_component();
        dialog.set_text(title.to_string());
        dialog
    }

    /// Edits a note titled `title` (« Note de bas de page », « Note de fin — ii »).
    pub fn ask(owner: &dyn AsForm, title: &str, text: &str) -> NoteAnswer {
        let mut d = Self::new(title, text);
        let result = d.show_dialog(owner);
        NoteAnswer::of(result, &d.note_text)
    }

    fn ok_click(&mut self) {
        self.set_dialog_result(DialogResult::Ok);
        self.close();
    }

    fn delete_click(&mut self) {
        self.set_dialog_result(DialogResult::Abort);
        self.close();
    }

    fn cancel_click(&mut self) {
        self.set_dialog_result(DialogResult::Cancel);
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_buttons_map_to_the_three_answers() {
        assert_eq!(NoteAnswer::of(DialogResult::Ok, "x"), NoteAnswer::Text("x".into()));
        assert_eq!(NoteAnswer::of(DialogResult::Abort, "x"), NoteAnswer::Delete);
        assert_eq!(NoteAnswer::of(DialogResult::Cancel, "x"), NoteAnswer::Cancel);
        assert_eq!(NoteAnswer::of(DialogResult::None, "x"), NoteAnswer::Cancel);
    }
}
