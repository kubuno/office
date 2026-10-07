//! Code-behind of the object format dialog (`object_format_dialog.kbview`): the web's « Taille et
//! propriétés » (`SizePropsDialog.tsx`: width/height with the ratio lock, rotation normalised to 0-359,
//! position from A1) and « Format de l'objet » (`FormatObjectDialog.tsx`: fill and outline, `'none'` when
//! switched off) on one page. Returns the patch of the stored object (only the fields that changed).

use kubuno_desktop::prelude::*;
use kubuno_office_sheets_core::model::cell::json_number;
use kubuno_office_sheets_core::objects::{ObjBox, ObjKind};
use serde_json::{json, Map, Value as Json};

#[kubuno_desktop::view("object_format_dialog.kbview")]
pub struct ObjectFormatDialog {
    #[bind]
    bx: f64,
    #[bind]
    by: f64,
    #[bind]
    bw: f64,
    #[bind]
    bh: f64,
    #[bind]
    rot: f64,
    #[bind]
    lock_ratio: bool,
    #[bind]
    has_fill: bool,
    #[bind]
    fill_color: String,
    #[bind]
    no_fill: bool,
    #[bind]
    fill_on: bool,
    #[bind]
    line_color: String,
    #[bind]
    no_line: bool,
    #[bind]
    line_on: bool,
    #[bind]
    line_width: f64,
    ratio: f64,
    adjusting: bool,
}

/// `r1`: one decimal.
fn r1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

impl ObjectFormatDialog {
    /// Opens the dialog on object `o` of `kind` whose box is `b` (rotation `rot`); the patch on OK.
    pub fn run(owner: &dyn AsForm, kind: ObjKind, o: &Json, b: ObjBox, rot: f64) -> Option<Map<String, Json>> {
        let s = |k: &str| o.get(k).and_then(Json::as_str).map(str::to_string);
        // The defaults the painters use (chart card, shape factory style, picture without outline).
        let (fill_def, line_def) = match kind {
            ObjKind::Chart => ("#ffffff", "#e2e4e6"),
            ObjKind::Shape => ("#ffffff", "#000000"),
            _ => ("none", "none"),
        };
        let fill = s("fill").unwrap_or_else(|| fill_def.into());
        let line = s("border").unwrap_or_else(|| line_def.into());
        let width0 = o.get("borderWidth").and_then(Json::as_f64).unwrap_or(1.0);
        let mut d = Self {
            bx: r1(b.bx),
            by: r1(b.by),
            bw: r1(b.bw),
            bh: r1(b.bh),
            rot: rot.round(),
            lock_ratio: true,
            has_fill: kind != ObjKind::Image,
            no_fill: fill == "none",
            fill_on: fill != "none",
            fill_color: if fill == "none" { "#ffffff".into() } else { fill.clone() },
            no_line: line == "none",
            line_on: line != "none",
            line_color: if line == "none" { "#000000".into() } else { line.clone() },
            line_width: width0,
            ratio: if b.bh > 0.0 { b.bw / b.bh } else { 1.0 },
            ..Self::default()
        };
        d.initialize_component();
        if d.show_dialog(owner) != DialogResult::Ok {
            return None;
        }
        let mut p = Map::new();
        let moved = (d.bx - r1(b.bx)).abs() > 1e-6 || (d.by - r1(b.by)).abs() > 1e-6 || (d.bw - r1(b.bw)).abs() > 1e-6 || (d.bh - r1(b.bh)).abs() > 1e-6;
        if moved {
            // The box is baked whole (an anchored picture or chart becomes box-based, as a drag does).
            p.insert("bx".into(), json_number(d.bx));
            p.insert("by".into(), json_number(d.by));
            p.insert("bw".into(), json_number(d.bw.max(1.0)));
            p.insert("bh".into(), json_number(d.bh.max(1.0)));
        }
        let rot_new = ((d.rot.round() % 360.0) + 360.0) % 360.0;
        if (rot_new - rot).abs() > 1e-6 {
            p.insert("rot".into(), json_number(rot_new));
        }
        if d.has_fill {
            let f = if d.no_fill { "none".to_string() } else { d.fill_color.clone() };
            if f != fill {
                p.insert("fill".into(), json!(f));
            }
        }
        let l = if d.no_line { "none".to_string() } else { d.line_color.clone() };
        if l != line {
            p.insert("border".into(), json!(l));
        }
        if (d.line_width - width0).abs() > 1e-6 {
            p.insert("borderWidth".into(), json_number(d.line_width));
        }
        (!p.is_empty()).then_some(p)
    }

    fn w_changed(&mut self, _e: &NumericValueChangedEventArgs) {
        if self.lock_ratio && !self.adjusting && self.ratio > 0.0 {
            self.adjusting = true;
            self.bh = r1(self.bw / self.ratio);
            self.adjusting = false;
        }
    }

    fn h_changed(&mut self, _e: &NumericValueChangedEventArgs) {
        if self.lock_ratio && !self.adjusting && self.ratio > 0.0 {
            self.adjusting = true;
            self.bw = r1(self.bh * self.ratio);
            self.adjusting = false;
        }
    }

    fn no_fill_changed(&mut self, e: &CheckedChangedEventArgs) {
        self.fill_on = !e.new;
    }

    fn no_line_changed(&mut self, e: &CheckedChangedEventArgs) {
        self.line_on = !e.new;
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
