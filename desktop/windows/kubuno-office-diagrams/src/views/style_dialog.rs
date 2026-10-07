//! Code-behind of « Modifier le style » (`style_dialog.kbview`): the selected shape's raw style as JSON.

use kubuno_desktop::prelude::*;
use kubuno_office_diagrams_core::model::Obj;

#[kubuno_desktop::view("style_dialog.kbview")]
pub struct StyleDialog {
    #[bind]
    json: String,
    /// The last « Appliquer » found the JSON invalid.
    #[bind]
    invalid: bool,
    style: Option<Obj>,
}

/// The style the JSON stands for: an object (`JSON.parse`; anything else is refused, the shape's style
/// being an object).
pub fn parse_style(text: &str) -> Option<Obj> {
    match serde_json::from_str(text) {
        Ok(serde_json::Value::Object(o)) => Some(o),
        _ => None,
    }
}

impl StyleDialog {
    /// Edits `json` over `owner`; the new style, `None` when cancelled.
    pub fn ask(owner: &dyn AsForm, json: &str) -> Option<Obj> {
        let mut d = Self { json: json.to_string(), ..Self::default() };
        d.initialize_component();
        if d.show_dialog(owner) == DialogResult::Ok {
            d.style.take()
        } else {
            None
        }
    }

    fn cancel_click(&mut self) {
        self.set_dialog_result(DialogResult::Cancel);
        self.close();
    }

    /// Invalid JSON keeps the dialog open (`catch { /* keep dialog open */ }`).
    fn apply_click(&mut self) {
        match parse_style(&self.json) {
            Some(style) => {
                self.style = Some(style);
                self.set_dialog_result(DialogResult::Ok);
                self.close();
            }
            None => self.invalid = true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_style;

    #[test]
    fn only_a_json_object_is_a_style() {
        let s = parse_style("{\n  \"fillColor\": \"#fff\",\n  \"rounded\": 8\n}").expect("an object");
        assert_eq!(s.get("rounded").and_then(|v| v.as_f64()), Some(8.0));
        assert!(parse_style("{ fillColor: '#fff' }").is_none());
        assert!(parse_style("42").is_none());
    }
}
