//! Changes made by collaborators (`applyingRemote`, `SA:3180-3201`, `SA:3260-3268`): they replace what the
//! editor shows without an undo entry and without marking the workbook as changed by this user — the
//! collaborator saves their own edits.

use serde_json::{Map, Value as Json};

use super::Editor;
use crate::model::cell::Cell;

impl Editor {
    /// Applies a change that did not come from this user: `f` edits the workbook; the caches go, the
    /// revision moves (so the window repaints), but the save state stays what it was.
    fn remote(&mut self, f: impl FnOnce(&mut Editor) -> bool) -> bool {
        let was_dirty = self.is_dirty();
        if !f(self) {
            return false;
        }
        self.touch();
        if !was_dirty {
            self.mark_saved();
        }
        true
    }

    /// Replaces the cells of the sheet whose id is `sheet_id` by a collaborator's (`mirror()`, `SA:3253-3263`).
    /// Returns whether anything changed.
    pub fn apply_remote_cells(&mut self, sheet_id: &str, cells: &Map<String, Json>) -> bool {
        self.remote(|ed| {
            let Some(sheet) = ed.wb.sheets.iter_mut().find(|s| s.meta.id == sheet_id) else { return false };
            if sheet.cells_json().as_object() == Some(cells) {
                return false;
            }
            sheet.cells = cells.iter().map(|(k, v)| (k.clone(), Cell::from_json(v))).collect();
            true
        })
    }

    /// Replaces the defined names by the room's (`namesMap.observe`, `SA:879-883`).
    pub fn apply_remote_names(&mut self, names: &Map<String, Json>) -> bool {
        self.remote(|ed| {
            if &ed.wb.names == names {
                return false;
            }
            ed.wb.names = names.clone();
            true
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::tests::editor;
    use serde_json::json;

    #[test]
    fn remote_changes_neither_dirty_nor_undoable() {
        let mut e = editor(json!({"A1": {"v": 1}}));
        let cells = json!({"A1": {"v": 2}, "B1": {"f": "=A1*10"}}).as_object().cloned().unwrap_or_default();
        assert!(e.apply_remote_cells("s1", &cells));
        assert!(!e.is_dirty());
        assert!(!e.can_undo());
        assert_eq!(e.shown(1, 1).text, "20");
        assert!(!e.apply_remote_cells("s1", &cells), "same content: no change");
        assert!(!e.apply_remote_cells("nope", &cells));
        let names = json!({"TAUX": "=0.5"}).as_object().cloned().unwrap_or_default();
        assert!(e.apply_remote_names(&names));
        assert_eq!(e.wb.names_map().get("TAUX").map(String::as_str), Some("=0.5"));
    }
}
