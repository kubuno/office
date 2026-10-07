//! The persistent pivot tables of a sheet (`SA:6191-6372`): their definitions (`sheet.data.pivots`), the
//! dialog's setup (`openPivotDialog` / `openPivotEditor`), the output written into the cells
//! (`computePivotEntries`, only the cells that differ), refresh, delete, and the automatic recalculation when
//! the source range changes. The aggregation itself is [`crate::pivot::build_pivot`].

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value as Json};

use crate::addr::{self, Cell as Pos, Rect, MAX_COLS};
use crate::editor::commands::Refused;
use crate::editor::Editor;
use crate::js;
use crate::model::cell::Cell;
use crate::pivot::{build_pivot, distinct_values, PivotConfig, PivotFilterSpec, PivotValueSpec};

/// Cells to write: each position and its new content (`None` clears it).
type Entries = Vec<(Pos, Option<Cell>)>;

/// `PivotDef` (`api.ts`): the column indexes are ABSOLUTE sheet columns.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PivotDef {
    pub id: String,
    /// The source range, its first row the headers (`A1:D20`).
    pub src: String,
    /// The output's top-left cell (`H1`).
    pub dest: String,
    #[serde(default)]
    pub row_fields: Vec<usize>,
    #[serde(default)]
    pub col_field: Option<usize>,
    #[serde(default)]
    pub values: Vec<PivotValueSpec>,
    #[serde(default)]
    pub filters: Vec<PivotFilterSpec>,
    /// The extent written last (to clear stale cells).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_rows: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_cols: Option<usize>,
}

/// What the dialog chooses (`PivotResult`): absolute columns and the target cell.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PivotChoice {
    pub row_fields: Vec<usize>,
    pub col_field: Option<usize>,
    pub values: Vec<PivotValueSpec>,
    pub filters: Vec<PivotFilterSpec>,
    pub target: String,
}

/// A column of the source (`PivotColumn`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PivotColumn {
    pub idx: usize,
    pub letter: String,
    pub header: String,
}

/// What `openPivotDialog` / `openPivotEditor` prepare.
#[derive(Clone, Debug, PartialEq)]
pub struct PivotSetup {
    pub range: Rect,
    pub columns: Vec<PivotColumn>,
    pub label: String,
    pub target: String,
    /// The pivot being edited (`editId`) and its choices (`initial`).
    pub edit: Option<(String, PivotChoice)>,
}

/// `/^([A-Z]+)(\d+)$/` on an anchor cell.
fn anchor(dest: &str) -> Option<Pos> {
    let d = dest.trim();
    let letters = d.chars().take_while(|c| c.is_ascii_uppercase()).count();
    (letters > 0 && d.len() > letters && d[letters..].chars().all(|c| c.is_ascii_digit())).then(|| addr::Cell::parse(d)).flatten()
}

/// `parseRangeRect`: `A1:D20` (upper-cased, normalised).
fn range_rect(src: &str) -> Option<Rect> {
    let s = src.trim().to_uppercase();
    let (a, b) = s.split_once(':')?;
    let (a, b) = (anchor(a)?, anchor(b)?);
    Some(Rect::new(a.col.min(b.col), a.row.min(b.row), a.col.max(b.col), a.row.max(b.row)))
}

fn rect_label(b: Rect) -> String {
    format!("{}:{}", addr::key(b.c1, b.r1), addr::key(b.c2, b.r2))
}

/// The output's cell style: header and total rows bold on their grounds.
fn pivot_style(i: usize, rows: usize) -> Option<Json> {
    if i == 0 {
        Some(json!({ "bold": true, "bg": "#e8f0fe" }))
    } else if i + 1 == rows {
        Some(json!({ "bold": true, "bg": "#f1f3f4" }))
    } else {
        None
    }
}

impl Editor {
    /// The active sheet's pivot tables.
    pub fn pivots(&self) -> Vec<PivotDef> {
        self.sheet().get("pivots").and_then(Json::as_array).map(|a| a.iter().filter_map(|p| serde_json::from_value(p.clone()).ok()).collect()).unwrap_or_default()
    }

    /// `savePivots` (saved at once, not an undoable edit).
    fn save_pivots(&mut self, next: &[PivotDef]) {
        let v = serde_json::to_value(next).unwrap_or(Json::Array(Vec::new()));
        self.sheet_mut().set("pivots", v);
        self.touch();
    }

    /// `readSrcRows`: below the header row, a number when the cell holds one, else its text.
    fn pivot_rows(&self, b: Rect) -> Vec<Vec<Json>> {
        let cells = self.cells();
        (b.r1 + 1..=b.r2)
            .map(|r| {
                (b.c1..=b.c2)
                    .map(|c| {
                        if !cells.contains_key(&addr::key(c, r)) {
                            return Json::String(String::new());
                        }
                        let s = self.shown(c, r);
                        s.num.map(|n| json!(n)).unwrap_or(Json::String(s.raw_text))
                    })
                    .collect()
            })
            .collect()
    }

    fn pivot_columns(&self, b: Rect) -> Vec<PivotColumn> {
        (b.c1..=b.c2)
            .map(|c| PivotColumn {
                idx: c as usize,
                letter: addr::index_to_col(c as i64),
                header: if self.cells().contains_key(&addr::key(c, b.r1)) { self.shown(c, b.r1).raw_text } else { String::new() },
            })
            .collect()
    }

    /// `openPivotDialog`: the selection (the used area for one cell), the output two columns to its right.
    pub fn pivot_setup(&self) -> Option<PivotSetup> {
        let (uc, ur) = self.sheet().used_extent();
        let single = self.sel.end.is_none_or(|e| e == self.sel.active);
        let sb = self.sel.rect();
        let b = if single { Rect::new(0, 1, uc, ur.max(1)) } else { Rect::new(sb.c1, sb.r1, sb.c2.min(uc.max(sb.c1)), sb.r2.min(ur.max(sb.r1))) };
        if b.r2 <= b.r1 {
            return None;
        }
        let target = addr::key((b.c2 + 2).min(MAX_COLS - 1), b.r1);
        Some(PivotSetup { range: b, columns: self.pivot_columns(b), label: rect_label(b), target, edit: None })
    }

    /// `openPivotEditor`.
    pub fn pivot_editor_setup(&self, def: &PivotDef) -> Option<PivotSetup> {
        let b = range_rect(&def.src)?;
        let choice = PivotChoice { row_fields: def.row_fields.clone(), col_field: def.col_field, values: def.values.clone(), filters: def.filters.clone(), target: def.dest.clone() };
        Some(PivotSetup { range: b, columns: self.pivot_columns(b), label: def.src.clone(), target: def.dest.clone(), edit: Some((def.id.clone(), choice)) })
    }

    /// The distinct values of source column `field` (absolute) for the filters' lists (`distinct`).
    pub fn pivot_distinct(&self, b: Rect, field: usize) -> Vec<String> {
        let rows = self.pivot_rows(b);
        distinct_values(&rows, field.saturating_sub(b.c1 as usize))
    }

    /// `computePivotEntries`: the cells to write (clearing a previous, larger or moved, extent) and the size.
    fn pivot_entries(&self, def: &PivotDef, prev: Option<&PivotDef>) -> Option<(Entries, usize, usize)> {
        let b = range_rect(&def.src)?;
        let t = anchor(&def.dest)?;
        let c1 = b.c1 as usize;
        let cfg = PivotConfig {
            row_fields: def.row_fields.iter().filter(|f| **f >= c1).map(|f| f - c1).collect(),
            col_field: def.col_field.and_then(|f| f.checked_sub(c1)),
            values: def.values.iter().map(|v| PivotValueSpec { field: v.field.saturating_sub(c1), agg: v.agg }).collect(),
            filters: Some(def.filters.iter().map(|f| PivotFilterSpec { field: f.field.saturating_sub(c1), selected: f.selected.clone() }).collect()),
            headers: (b.c1..=b.c2).map(|c| if self.cells().contains_key(&addr::key(c, b.r1)) { self.shown(c, b.r1).raw_text } else { String::new() }).collect(),
        };
        let grid = build_pivot(&self.pivot_rows(b), &cfg);
        let (n_r, n_c) = (grid.len(), grid.iter().map(Vec::len).max().unwrap_or(0));
        let (max_r, max_c) = (n_r.max(def.last_rows.unwrap_or(0)), n_c.max(def.last_cols.unwrap_or(0)));
        let mut entries = Vec::new();
        for i in 0..max_r {
            for j in 0..max_c {
                let at = Pos::new((t.col + j as u32).min(MAX_COLS - 1), t.row + i as u32);
                let val = grid.get(i).and_then(|r| r.get(j)).cloned().unwrap_or_default();
                if val.is_empty() {
                    entries.push((at, None));
                    continue;
                }
                let n = js::number_or_nan(&val);
                let v = if n.is_nan() { Json::String(val) } else { json!(n) };
                entries.push((at, Some(Cell { v: Some(v), s: pivot_style(i, n_r), ..Cell::default() })));
            }
        }
        if let Some(p) = prev.filter(|p| p.dest != def.dest) {
            if let Some(pa) = anchor(&p.dest) {
                for i in 0..p.last_rows.unwrap_or(1) {
                    for j in 0..p.last_cols.unwrap_or(1) {
                        entries.push((Pos::new((pa.col + j as u32).min(MAX_COLS - 1), pa.row + i as u32), None));
                    }
                }
            }
        }
        Some((entries, n_r, n_c))
    }

    /// `writePivotEntries`: only the cells whose value or style differ (no spurious history).
    fn write_pivot_entries(&mut self, entries: Entries) -> Result<usize, Refused> {
        let cells = self.cells();
        let changed: Vec<(Pos, Option<Cell>)> = entries
            .into_iter()
            .filter(|(at, cell)| {
                let cur = cells.get(&at.key());
                match cell {
                    None => cur.is_some_and(|c| !(c.v.as_ref().is_none_or(Json::is_null) && c.f.is_none() && c.s.is_none() && c.c.is_none())),
                    Some(new) => cur.map(|c| (&c.v, &c.s)) != Some((&new.v, &new.s)),
                }
            })
            .collect();
        let n = changed.len();
        if n > 0 {
            self.write_cells(changed)?;
        }
        Ok(n)
    }

    /// `applyPivotResult`: creates or updates the pivot from the dialog, writes it and selects its corner.
    pub fn apply_pivot(&mut self, setup: &PivotSetup, choice: &PivotChoice) -> Result<(), Refused> {
        let target = choice.target.trim().to_uppercase();
        let Some(t) = anchor(if target.is_empty() { "A1" } else { &target }) else { return Ok(()) };
        let b = setup.range;
        let mut tc = t.col;
        // A destination inside the source moves just right of it.
        if b.contains(t) {
            tc = (b.c2 + 2).min(MAX_COLS - 1);
        }
        let dest = addr::key(tc, t.row);
        let pivots = self.pivots();
        let edit_id = setup.edit.as_ref().map(|(id, _)| id.clone());
        let prev = edit_id.as_ref().and_then(|id| pivots.iter().find(|p| &p.id == id)).cloned();
        let id = edit_id.clone().unwrap_or_else(new_pivot_id);
        let def = PivotDef {
            id,
            src: rect_label(b),
            dest,
            row_fields: choice.row_fields.clone(),
            col_field: choice.col_field,
            values: choice.values.clone(),
            filters: choice.filters.clone(),
            last_rows: prev.as_ref().and_then(|p| p.last_rows),
            last_cols: prev.as_ref().and_then(|p| p.last_cols),
        };
        let Some((entries, n_r, n_c)) = self.pivot_entries(&def, prev.as_ref()) else { return Ok(()) };
        self.write_pivot_entries(entries)?;
        let with = PivotDef { last_rows: Some(n_r), last_cols: Some(n_c), ..def };
        let next: Vec<PivotDef> = if edit_id.is_some() { pivots.into_iter().map(|p| if p.id == with.id { with.clone() } else { p }).collect() } else { pivots.into_iter().chain(std::iter::once(with)).collect() };
        self.save_pivots(&next);
        self.select(Pos::new(tc, t.row));
        Ok(())
    }

    /// `refreshPivot`.
    pub fn refresh_pivot(&mut self, id: &str) -> Result<(), Refused> {
        let pivots = self.pivots();
        let Some(def) = pivots.iter().find(|p| p.id == id).cloned() else { return Ok(()) };
        let Some((entries, n_r, n_c)) = self.pivot_entries(&def, None) else { return Ok(()) };
        self.write_pivot_entries(entries)?;
        if Some(n_r) != def.last_rows || Some(n_c) != def.last_cols {
            let next: Vec<PivotDef> = pivots.into_iter().map(|p| if p.id == def.id { PivotDef { last_rows: Some(n_r), last_cols: Some(n_c), ..p } } else { p }).collect();
            self.save_pivots(&next);
        }
        Ok(())
    }

    /// `refreshAllPivots` (Données › Actualiser tout).
    pub fn refresh_all_pivots(&mut self) -> Result<(), Refused> {
        for p in self.pivots() {
            self.refresh_pivot(&p.id)?;
        }
        Ok(())
    }

    /// `deletePivot`: clears its output and forgets it.
    pub fn delete_pivot(&mut self, id: &str) -> Result<(), Refused> {
        let pivots = self.pivots();
        let Some(def) = pivots.iter().find(|p| p.id == id).cloned() else { return Ok(()) };
        if let Some(pa) = anchor(&def.dest) {
            let mut entries = Vec::new();
            for i in 0..def.last_rows.unwrap_or(1) {
                for j in 0..def.last_cols.unwrap_or(1) {
                    entries.push((Pos::new((pa.col + j as u32).min(MAX_COLS - 1), pa.row + i as u32), None));
                }
            }
            self.write_pivot_entries(entries)?;
        }
        let next: Vec<PivotDef> = pivots.into_iter().filter(|p| p.id != id).collect();
        self.save_pivots(&next);
        Ok(())
    }

    /// `pivotAtSel`: the pivot whose output holds the active cell.
    pub fn pivot_at_selection(&self) -> Option<PivotDef> {
        let a = self.sel.active;
        self.pivots().into_iter().find(|p| {
            anchor(&p.dest).is_some_and(|pa| a.col >= pa.col && a.col < pa.col + p.last_cols.unwrap_or(1) as u32 && a.row >= pa.row && a.row < pa.row + p.last_rows.unwrap_or(1) as u32)
        })
    }

    /// The automatic recalculation (`SA:6304-6325`): each pivot whose source range changed since `sigs` last
    /// saw it is refreshed. The caller keeps `sigs` (the web's `pivotSigs`).
    pub fn refresh_stale_pivots(&mut self, sigs: &mut Map<String, Json>) -> Result<bool, Refused> {
        let mut any = false;
        for def in self.pivots() {
            let Some(b) = range_rect(&def.src) else { continue };
            let mut parts = Vec::new();
            for r in b.r1..=b.r2 {
                for c in b.c1..=b.c2 {
                    if let Some(cell) = self.cells().get(&addr::key(c, r)) {
                        parts.extend([json!(c), json!(r), cell.v.clone().unwrap_or(Json::Null), cell.f.clone().unwrap_or(Json::Null)]);
                    }
                }
            }
            let sig = Json::Array(parts);
            if sigs.get(&def.id) != Some(&sig) {
                sigs.insert(def.id.clone(), sig);
                self.refresh_pivot(&def.id)?;
                any = true;
            }
        }
        Ok(any)
    }
}

/// `pv_<time base 36><0..9999>`.
fn new_pivot_id() -> String {
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let mut b = [0u8; 2];
    let _ = getrandom::getrandom(&mut b);
    let mut n = ms;
    let mut s = String::new();
    while n > 0 {
        let d = (n % 36) as u32;
        s.insert(0, std::char::from_digit(d, 36).unwrap_or('0'));
        n /= 36;
    }
    format!("pv_{s}{}", u16::from_le_bytes(b) % 10_000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::tests::editor;
    use crate::pivot::PivotAgg;

    fn sales() -> Editor {
        editor(json!({
            "A1": {"v": "Région"}, "B1": {"v": "Produit"}, "C1": {"v": "Montant"},
            "A2": {"v": "Nord"}, "B2": {"v": "Pomme"}, "C2": {"v": 10},
            "A3": {"v": "Sud"}, "B3": {"v": "Poire"}, "C3": {"v": 5},
            "A4": {"v": "Nord"}, "B4": {"v": "Poire"}, "C4": {"v": 7},
        }))
    }

    #[test]
    fn a_pivot_is_built_saved_refreshed_and_deleted() {
        let mut ed = sales();
        let setup = ed.pivot_setup().expect("a range");
        assert_eq!(setup.label, "A1:C4");
        assert_eq!(setup.target, "E1");
        assert_eq!(setup.columns[2].header, "Montant");
        let choice = PivotChoice { row_fields: vec![0], col_field: None, values: vec![PivotValueSpec { field: 2, agg: PivotAgg::Sum }], filters: vec![], target: "e1".into() };
        ed.apply_pivot(&setup, &choice).expect("written");
        assert_eq!(ed.cell_text(4, 1), "Région");
        assert_eq!(ed.cell_text(4, 2), "Nord");
        assert_eq!(ed.cell_text(5, 2), "17");
        let p = ed.pivots();
        assert_eq!(p.len(), 1);
        assert_eq!((p[0].last_rows, p[0].dest.as_str()), (Some(4), "E1"));
        assert_eq!(ed.cells().get("E1").and_then(|c| c.s.clone()), Some(json!({"bold": true, "bg": "#e8f0fe"})));
        // The source changes: the stale pivot refreshes.
        let mut sigs = Map::new();
        assert!(ed.refresh_stale_pivots(&mut sigs).expect("ok"));
        ed.update_cell(Pos::new(2, 2), "20").expect("edit");
        assert!(ed.refresh_stale_pivots(&mut sigs).expect("ok"));
        assert!(!ed.refresh_stale_pivots(&mut sigs).expect("ok"), "unchanged: nothing to do");
        assert_eq!(ed.cell_text(5, 2), "27");
        ed.sel = crate::editor::Selection::cell(Pos::new(5, 3));
        let id = ed.pivot_at_selection().expect("in the output").id;
        ed.delete_pivot(&id).expect("deleted");
        assert!(ed.pivots().is_empty());
        assert_eq!(ed.cell_text(4, 1), "");
    }

    #[test]
    fn a_destination_inside_the_source_moves_right_and_editing_clears_the_old_place() {
        let mut ed = sales();
        let setup = ed.pivot_setup().expect("a range");
        let choice = PivotChoice { row_fields: vec![1], col_field: None, values: vec![PivotValueSpec { field: 2, agg: PivotAgg::Count }], filters: vec![], target: "B2".into() };
        ed.apply_pivot(&setup, &choice).expect("written");
        assert_eq!(ed.pivots()[0].dest, "E2");
        let def = ed.pivots()[0].clone();
        let edit = ed.pivot_editor_setup(&def).expect("editable");
        ed.apply_pivot(&edit, &PivotChoice { target: "H1".into(), ..edit.edit.clone().expect("initial").1 }).expect("moved");
        assert_eq!(ed.pivots().len(), 1);
        assert_eq!(ed.cell_text(4, 2), "", "the old output is cleared");
        assert_eq!(ed.cell_text(7, 1), "Produit");
        assert_eq!(ed.pivot_distinct(edit.range, 0), vec!["Nord", "Sud"]);
    }
}
