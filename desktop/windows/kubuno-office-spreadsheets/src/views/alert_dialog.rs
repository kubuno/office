//! Code-behind of the one-button message (`alert_dialog.kbview`): the web's `appAlert`. Never a system message box.

use kubuno_desktop::prelude::*;

#[kubuno_desktop::view("alert_dialog.kbview")]
pub struct AlertDialog {}

impl AlertDialog {
    /// Shows `message` over `owner` until OK.
    pub fn say(owner: &dyn AsForm, message: &str) {
        let mut d = Self::default();
        d.initialize_component();
        d.message.set_text(message.to_string());
        d.show_dialog(owner);
    }

    fn ok_click(&mut self) {
        self.set_dialog_result(DialogResult::Ok);
        self.close();
    }
}
