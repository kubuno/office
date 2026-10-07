//! Code-behind of the text dialog (`prompt_dialog.kbview`): a label, a field, OK / Cancel.

use kubuno_desktop::prelude::*;

#[kubuno_desktop::view("prompt_dialog.kbview")]
pub struct PromptDialog {
    #[bind]
    caption: String,
    #[bind]
    label: String,
    #[bind]
    value: String,
}

impl PromptDialog {
    /// Asks for a text over `owner`, starting from `current`; `None` when cancelled.
    pub fn ask(owner: &dyn AsForm, title: &str, label: &str, current: &str) -> Option<String> {
        let mut d = Self { caption: title.to_string(), label: label.to_string(), value: current.to_string(), ..Self::default() };
        d.initialize_component();
        if d.show_dialog(owner) == DialogResult::Ok {
            Some(d.value.clone())
        } else {
            None
        }
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
