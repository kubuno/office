//! Code-behind of the one-line question (`prompt_dialog.kbview`).

use kubuno_desktop::prelude::*;

#[kubuno_desktop::view("prompt_dialog.kbview")]
pub struct PromptDialog {
    #[bind]
    answer: String,
    #[bind]
    hint: String,
}

impl PromptDialog {
    pub fn new(title: &str, placeholder: &str, confirm: &str) -> Self {
        let mut dialog = Self { hint: placeholder.to_string(), ..Self::default() };
        dialog.initialize_component();
        dialog.set_text(title.to_string());
        dialog.ok.set_text(confirm.to_string());
        dialog
    }

    /// The trimmed answer; `None` when cancelled or left empty (the web's `prompt` resolves `null`
    /// or an empty string, and the callers then do nothing).
    pub fn ask(owner: &dyn AsForm, title: &str, placeholder: &str, confirm: &str) -> Option<String> {
        let mut d = Self::new(title, placeholder, confirm);
        (d.show_dialog(owner) == DialogResult::Ok).then(|| d.answer.trim().to_string()).filter(|s| !s.is_empty())
    }

    fn ok_click(&mut self) {
        self.set_dialog_result(DialogResult::Ok);
        self.close();
    }

    fn cancel_click(&mut self) {
        self.set_dialog_result(DialogResult::Cancel);
        self.close();
    }
}
