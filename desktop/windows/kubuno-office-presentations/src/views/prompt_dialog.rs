//! Code-behind of the prompt dialog (`prompt_dialog.kbview`): one line of text asked with a title and a message.

use kubuno_desktop::prelude::*;

#[kubuno_desktop::view("prompt_dialog.kbview")]
pub struct PromptDialog {
    #[bind]
    value: String,
}

impl PromptDialog {
    pub fn new(title: &str, message: &str, current: &str, placeholder: &str) -> Self {
        let mut dialog = Self { value: current.to_string(), ..Self::default() };
        dialog.initialize_component();
        dialog.set_text(title.to_string());
        dialog.message.set_text(message.to_string());
        dialog.value_box.set_placeholder(placeholder.to_string());
        dialog
    }

    /// Asks; `None` when cancelled, or when the answer is empty and `allow_empty` is false.
    pub fn ask(owner: &dyn AsForm, title: &str, message: &str, current: &str, placeholder: &str, allow_empty: bool) -> Option<String> {
        let mut d = Self::new(title, message, current, placeholder);
        if d.show_dialog(owner) != DialogResult::Ok {
            return None;
        }
        let v = d.value.trim().to_string();
        (allow_empty || !v.is_empty()).then_some(v)
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
