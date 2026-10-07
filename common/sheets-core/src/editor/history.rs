//! Undo and redo (`SA:3203-3247`): snapshots of what the web watches — the cells, the column and row
//! styles, the merges, the sizes and the frozen panes — 200 at most, one per committed change (no time
//! grouping), reset when the active sheet changes. Conditional formats, validations, groups and objects
//! are not in a snapshot, as on the web.

use serde_json::{Map, Value as Json};

use crate::formula::eval::CellMap;
use crate::model::sheet::Sheet;

/// The keys of `SheetData` a snapshot holds besides the cells.
pub const WATCHED: [&str; 7] = ["col_styles", "row_styles", "merges", "col_widths", "row_heights", "frozen_rows", "frozen_cols"];

/// The web's history limit.
pub const LIMIT: usize = 200;

/// One snapshot (`HistSnap`).
#[derive(Clone, Debug, PartialEq)]
pub struct Snap {
    pub cells: CellMap,
    pub keys: Map<String, Json>,
}

impl Snap {
    pub fn of(sheet: &Sheet) -> Snap {
        let keys = WATCHED.iter().filter_map(|k| sheet.data.get(*k).map(|v| (k.to_string(), v.clone()))).collect();
        Snap { cells: sheet.cells.clone(), keys }
    }

    /// Puts the snapshot back into `sheet` (a key absent from the snapshot is removed).
    pub fn restore(&self, sheet: &mut Sheet) {
        sheet.cells = self.cells.clone();
        for k in WATCHED {
            match self.keys.get(k) {
                Some(v) => {
                    sheet.data.insert(k.to_string(), v.clone());
                }
                None => {
                    sheet.data.remove(k);
                }
            }
        }
    }
}

/// The two stacks.
#[derive(Clone, Debug, Default)]
pub struct History {
    undo: Vec<Snap>,
    redo: Vec<Snap>,
}

impl History {
    /// Records `before` (the state a change left), when the change touched what a snapshot holds.
    pub fn record(&mut self, before: Snap, after: &Sheet) {
        if Snap::of(after) == before {
            return;
        }
        self.undo.push(before);
        if self.undo.len() > LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Restores the last snapshot into `sheet`; the current state goes to redo.
    pub fn undo(&mut self, sheet: &mut Sheet) -> bool {
        let Some(s) = self.undo.pop() else { return false };
        self.redo.push(Snap::of(sheet));
        s.restore(sheet);
        true
    }

    /// The opposite of [`History::undo`].
    pub fn redo(&mut self, sheet: &mut Sheet) -> bool {
        let Some(s) = self.redo.pop() else { return false };
        self.undo.push(Snap::of(sheet));
        s.restore(sheet);
        true
    }
}
