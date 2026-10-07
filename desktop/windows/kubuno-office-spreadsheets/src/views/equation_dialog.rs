//! Code-behind of « Équation » (`equation_dialog.kbview`): the LaTeX source and its live preview.

use kubuno_desktop::prelude::*;
use kubuno_desktop::views::component::Control;

#[kubuno_desktop::view("equation_dialog.kbview")]
pub struct EquationDialog {
    #[bind]
    latex: String,
}

impl EquationDialog {
    /// Edits `current` over `owner`; the new LaTeX, `None` when cancelled.
    pub fn ask(owner: &dyn AsForm, current: &str) -> Option<String> {
        let mut d = Self { latex: current.to_string(), ..Self::default() };
        d.initialize_component();
        if d.show_dialog(owner) == DialogResult::Ok {
            Some(d.latex.clone())
        } else {
            None
        }
    }

    /// Typing redraws the preview (it is bound to the same text).
    fn source_changed(&mut self, e: &TextChangedEventArgs) {
        self.latex = e.new.clone();
        self.preview.with(|p: &mut crate::EquationView| {
            p.latex = e.new.clone();
            p.invalidate();
        });
    }

    fn cancel_click(&mut self) {
        self.set_dialog_result(DialogResult::Cancel);
        self.close();
    }

    fn ok_click(&mut self) {
        self.set_dialog_result(DialogResult::Ok);
        self.close();
    }
}
