//! Code-behind of the template gallery (`templates_dialog.kbview`).

use kubuno_desktop::prelude::*;
use kubuno_office_diagrams_core::templates::DiagramTemplate;

#[kubuno_desktop::view("templates_dialog.kbview")]
pub struct TemplatesDialog {
    picked: Option<usize>,
}

impl TemplatesDialog {
    /// Shows the gallery over `owner`; the template picked, `None` when cancelled.
    pub fn pick(owner: &dyn AsForm) -> Option<&'static DiagramTemplate> {
        let mut d = Self::default();
        d.initialize_component();
        if d.show_dialog(owner) == DialogResult::Ok {
            d.picked.and_then(|i| kubuno_office_diagrams_core::templates::TEMPLATES.get(i))
        } else {
            None
        }
    }

    fn cancel_click(&mut self) {
        self.set_dialog_result(DialogResult::Cancel);
        self.close();
    }
}
