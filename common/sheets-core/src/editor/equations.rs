//! The editor's equation commands (`web/src/SpreadsheetApp.tsx` 1107-1180): an equation inserted at the
//! active cell (`insertEquation`), its LaTeX changed by the equation dialog. The objects live in the sheet's
//! `equations` array (`{id, bx, by, latex, bw?, bh?, rot?}`), every unknown field kept; moving, resizing,
//! rotating and deleting them are the floating-object commands (`editor/objects.rs`). Laying the formula
//! out is the math core's (`kubuno-office-math-core`); the natural size it gives is what an equation
//! without a box takes.

use serde_json::{Map, Value as Json};

use super::Editor;
use crate::grid::{COL_HEADER_HEIGHT, ROW_HEADER_WIDTH};
use crate::model::cell::json_number;
use crate::objects::{self as obj, ObjKind, ObjRef};

impl Editor {
    /// `insertEquation`: a new equation at the active cell (A1 when no cell is selected), at its natural
    /// size. `id` is the new object's UUID (`crypto.randomUUID()` on the web).
    pub fn insert_equation(&mut self, latex: &str, id: String) -> ObjRef {
        let g = self.geometry();
        let (col, row) = if self.selected { (self.sel.active.col, self.sel.active.row) } else { (0, 1) };
        let bx = (g.col_left(col) - ROW_HEADER_WIDTH) / self.zoom;
        let by = (g.row_top(row) - COL_HEADER_HEIGHT) / self.zoom;
        let mut o = Map::new();
        o.insert("id".into(), Json::String(id.clone()));
        o.insert("bx".into(), json_number(bx));
        o.insert("by".into(), json_number(by));
        o.insert("latex".into(), Json::String(latex.to_string()));
        self.mutate_objects(|d| {
            if let Some(a) = obj::array_mut(d, ObjKind::Equation) {
                a.push(Json::Object(o));
            }
        });
        ObjRef::new(ObjKind::Equation, id)
    }

    /// The dialog's « Valider »: the equation's LaTeX replaced (`persistEquations(… latex …)`).
    pub fn set_equation_latex(&mut self, id: &str, latex: &str) -> bool {
        let r = ObjRef::new(ObjKind::Equation, id);
        if self.object(&r).and_then(|o| o.get("latex")).and_then(Json::as_str) == Some(latex) {
            return false;
        }
        let mut found = false;
        self.mutate_objects(|d| {
            if let Some(o) = obj::get_mut(d, &r).and_then(Json::as_object_mut) {
                o.insert("latex".into(), Json::String(latex.to_string()));
                found = true;
            }
        });
        found
    }

    /// The LaTeX of equation `id`.
    pub fn equation_latex(&self, id: &str) -> Option<String> {
        self.object(&ObjRef::new(ObjKind::Equation, id)).and_then(|o| o.get("latex")).and_then(Json::as_str).map(str::to_string)
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::editor;
    use crate::addr::Cell as Pos;
    use crate::objects::ObjKind;
    use serde_json::json;

    #[test]
    fn equations_are_inserted_at_the_active_cell() {
        let mut e = editor(json!({}));
        let r = e.insert_equation("A = \\pi r^2", "e1".into());
        assert_eq!(r.kind, ObjKind::Equation);
        assert_eq!(e.object(&r).cloned(), Some(json!({"id": "e1", "bx": 0, "by": 0, "latex": "A = \\pi r^2"})));
        e.select(Pos::new(2, 4));
        let r2 = e.insert_equation("", "e2".into());
        let o = e.object(&r2).cloned().unwrap_or_default();
        assert_eq!(o["bx"], json!(200));
        assert_eq!(o["by"], json!(72));
        assert!(e.set_equation_latex("e2", "x^2"));
        assert!(!e.set_equation_latex("e2", "x^2"));
        assert_eq!(e.equation_latex("e2").as_deref(), Some("x^2"));
        assert_eq!(e.objects(ObjKind::Equation).len(), 2);
    }
}
