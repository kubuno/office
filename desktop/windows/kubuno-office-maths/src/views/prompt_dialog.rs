//! Code-behind of the name dialog (`prompt_dialog.kbview`).

use kubuno_desktop::prelude::*;

#[kubuno_desktop::view("prompt_dialog.kbview")]
pub struct PromptDialog {
    #[bind]
    value: String,
}

impl PromptDialog {
    /// Asks for a name over `owner`, starting from `current`; `None` when cancelled.
    pub fn ask(owner: &dyn AsForm, title: &str, current: &str) -> Option<String> {
        let mut d = Self { value: current.to_string(), ..Self::default() };
        d.initialize_component();
        d.set_text(title.to_string());
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
