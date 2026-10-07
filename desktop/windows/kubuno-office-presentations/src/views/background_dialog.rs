//! Code-behind of the background dialog (`background_dialog.kbview`): every pick is applied to the active slide
//! at once (`setSlideBg`), like the web popup; « Fermer » closes it.

use kubuno_desktop::prelude::*;
use kubuno_office_presentations_common::model::gradient_css;
use kubuno_office_slides_core::model::{Background, Gradient};
use serde_json::json;

use crate::model::Shared;

#[kubuno_desktop::view("background_dialog.kbview")]
pub struct BackgroundDialog {
    #[bind]
    bg_color: String,
    #[bind]
    bg_gradient: String,
    doc: Option<Shared>,
}

/// `DEFAULT_GRADIENT` of `@ui` (linear 90°, #4a90d9 → #9b59b6).
fn default_gradient() -> Gradient {
    Gradient { radial: false, angle: 90.0, stops: vec![("#4a90d9".into(), 0.0, 100.0), ("#9b59b6".into(), 1.0, 100.0)], legacy: false }
}

impl BackgroundDialog {
    pub fn new(doc: Shared) -> Self {
        let bg = doc.ws.borrow().active_slide().map(|s| s.data.background());
        let (color, grad) = match bg {
            Some(Background::Color(c)) => (c.unwrap_or_else(|| "#ffffff".into()), default_gradient()),
            Some(Background::Gradient(g)) => ("#ffffff".into(), g),
            _ => ("#ffffff".into(), default_gradient()),
        };
        let mut d = Self { bg_color: color, bg_gradient: gradient_css::to_css(&grad), doc: Some(doc), ..Self::default() };
        d.initialize_component();
        d
    }

    /// Shows it over `owner` until « Fermer ».
    pub fn edit(owner: &dyn AsForm, doc: Shared) {
        let mut d = Self::new(doc);
        d.show_dialog(owner);
    }

    fn apply(&mut self, json: String) {
        if let Some(doc) = &self.doc {
            doc.ws.borrow_mut().set_background(&json);
        }
    }

    fn color_changed(&mut self, e: &TextChangedEventArgs) {
        let c = e.new.trim().to_string();
        if !c.is_empty() {
            self.apply(json!({ "type": "color", "color": c }).to_string());
        }
    }

    fn gradient_changed(&mut self, e: &TextChangedEventArgs) {
        if let Some(grad) = gradient_css::from_css(&e.new) {
            self.apply(json!({ "type": "gradient", "grad": grad }).to_string());
        }
    }

    fn close_click(&mut self) {
        self.set_dialog_result(DialogResult::Ok);
        self.close();
    }
}
