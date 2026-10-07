//! Code-behind of the password dialogs (`password_dialog.kbview`): the web's `SheetProtectDialog` and
//! `WorkbookEncryptDialog`. The window gives the check to run on submit; the dialog closes when it passes.

use kubuno_desktop::prelude::*;

use crate::Resources;

/// Which dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasswordMode {
    Protect,
    Unprotect,
    Encrypt,
    Decrypt,
    UnlockWorkbook,
}

impl PasswordMode {
    /// A new password (typed twice, with the warning).
    fn is_new(self) -> bool {
        matches!(self, PasswordMode::Protect | PasswordMode::Encrypt)
    }
}

/// Runs on submit with the password: `true` closes the dialog, `false` shows « Mot de passe incorrect ».
pub type CheckFn = Box<dyn FnMut(&str) -> bool>;

#[kubuno_desktop::view("password_dialog.kbview")]
pub struct PasswordDialog {
    #[bind]
    caption: String,
    #[bind]
    password: String,
    #[bind]
    confirm: String,
    #[bind]
    is_new: bool,
    #[bind]
    is_unlock: bool,
    #[bind]
    warning: String,
    #[bind]
    error: String,
    #[bind]
    has_error: bool,
    #[bind]
    action: String,
    check: Option<CheckFn>,
}

impl PasswordDialog {
    pub fn new(mode: PasswordMode, check: CheckFn) -> Self {
        let (caption, action, warning) = match mode {
            PasswordMode::Protect => (Resources::prot_title(), Resources::prot_apply(), Resources::prot_warn()),
            PasswordMode::Unprotect => (Resources::prot_unlock_title(), Resources::prot_remove(), ""),
            PasswordMode::Encrypt => (Resources::enc_title(), Resources::enc_do(), Resources::enc_warn()),
            PasswordMode::Decrypt => (Resources::enc_dec_title(), Resources::enc_dec_do(), ""),
            PasswordMode::UnlockWorkbook => (Resources::enc_unlock_title(), Resources::enc_unlock_do(), ""),
        };
        let mut d = Self {
            caption: caption.into(),
            action: action.into(),
            warning: warning.into(),
            is_new: mode.is_new(),
            is_unlock: mode == PasswordMode::UnlockWorkbook,
            check: Some(check),
            ..Self::default()
        };
        d.initialize_component();
        d
    }

    /// Shows it over `owner`: `true` when the check passed.
    pub fn ask(owner: &dyn AsForm, mode: PasswordMode, check: CheckFn) -> bool {
        let mut d = Self::new(mode, check);
        d.show_dialog(owner) == DialogResult::Ok
    }

    fn fail(&mut self, msg: &str) {
        self.error = msg.to_string();
        self.has_error = true;
    }

    /// `submit`: empty, mismatch, then the check.
    fn submit(&mut self) -> bool {
        self.has_error = false;
        if self.password.is_empty() {
            self.fail(Resources::prot_empty());
            return false;
        }
        if self.is_new && self.password != self.confirm {
            self.fail(Resources::prot_mismatch());
            return false;
        }
        let pwd = self.password.clone();
        let ok = self.check.as_mut().is_some_and(|c| c(&pwd));
        if !ok {
            self.fail(Resources::prot_wrong());
        }
        ok
    }

    fn ok_button_click(&mut self) {
        if self.submit() {
            self.set_dialog_result(DialogResult::Ok);
            self.close();
        }
    }

    fn cancel_button_click(&mut self) {
        self.set_dialog_result(DialogResult::Cancel);
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_password_is_typed_twice_and_a_wrong_one_is_refused() {
        let mut d = PasswordDialog { is_new: true, check: Some(Box::new(|_| true)), ..PasswordDialog::default() };
        assert!(!d.submit());
        assert_eq!(d.error, Resources::prot_empty());
        d.password = "a".into();
        d.confirm = "b".into();
        assert!(!d.submit());
        assert_eq!(d.error, Resources::prot_mismatch());
        d.confirm = "a".into();
        assert!(d.submit());
        let mut u = PasswordDialog { password: "x".into(), check: Some(Box::new(|p| p == "right")), ..PasswordDialog::default() };
        assert!(!u.submit());
        assert_eq!(u.error, Resources::prot_wrong());
    }
}
