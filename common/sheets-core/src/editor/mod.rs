//! The open workbook as the editor sees it (`SpreadsheetEditor`'s state): the active sheet, the
//! selection, the zoom, the filters, the undo history, the calculation caches — and the façade every
//! platform drives (the desktop now, WASM later). Edits are in [`commands`], the history in [`history`].

pub mod commands;
pub mod history;

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::addr::{self, Cell as Pos, Rect, MAX_COLS, MAX_ROWS};
use crate::cf::{compute_cond_formats, CondStyle};
use crate::formula::eval::{CellMap, Ctx, Data, Env, SpillIndex};
use crate::formula::spill::compute_spills;
use crate::grid::{self, CellShown, Geometry, MergeInfo};
use crate::model::sheet::Sheet;
use crate::model::style::Style;
use crate::model::workbook::Workbook;

use history::History;

/// A direction of movement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

/// The selection: the active cell and, for a range, its other corner (`selectedCell` / `rangeEnd`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub active: Pos,
    pub end: Option<Pos>,
}

impl Selection {
    pub fn cell(c: Pos) -> Self {
        Self { active: c, end: None }
    }
    /// The selected rectangle.
    pub fn rect(&self) -> Rect {
        let e = self.end.unwrap_or(self.active);
        Rect::new(self.active.col, self.active.row, e.col, e.row)
    }
    /// A whole column (rows 1 … max) is selected.
    pub fn whole_cols(&self) -> bool {
        let r = self.rect();
        r.r1 == 1 && r.r2 >= MAX_ROWS
    }
    /// A whole row (columns A … XFD) is selected.
    pub fn whole_rows(&self) -> bool {
        let r = self.rect();
        r.c1 == 0 && r.c2 >= MAX_COLS - 1
    }
}

/// What the evaluation of the active sheet needs, cached per data revision.
#[derive(Default)]
struct Calc {
    rev: u64,
    spill: Option<SpillIndex>,
    cf: HashMap<String, CondStyle>,
}

/// The open workbook and its editing state.
pub struct Editor {
    pub wb: Workbook,
    active: usize,
    pub sel: Selection,
    pub zoom: f64,
    /// The app's language (`i18n.language`): Intl number formats.
    pub lang: String,
    /// Column filters: column → the allowed displayed texts (client state, never saved, as on the web).
    pub col_filters: HashMap<u32, HashSet<String>>,
    /// Filter mode: funnels shown on every header.
    pub filter_mode: bool,
    env: Rc<dyn Env>,
    history: History,
    /// Bumped by every change of the workbook.
    rev: u64,
    saved_rev: u64,
    calc: std::cell::RefCell<Calc>,
    geom: std::cell::RefCell<Option<(u64, f64, Rc<Geometry>)>>,
    /// The internal clipboard (`clipboard` ref of the web).
    pub(crate) clipboard: Option<commands::Clip>,
}

impl Editor {
    /// Opens `wb` on its first sheet, A1 selected, at 100 %.
    pub fn new(wb: Workbook, env: Rc<dyn Env>) -> Editor {
        Editor {
            wb,
            active: 0,
            sel: Selection::cell(Pos::new(0, 1)),
            zoom: 1.0,
            lang: "fr".into(),
            col_filters: HashMap::new(),
            filter_mode: false,
            env,
            history: History::default(),
            rev: 1,
            saved_rev: 1,
            calc: Default::default(),
            geom: Default::default(),
            clipboard: None,
        }
    }

    /// The active sheet's index.
    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn sheet(&self) -> &Sheet {
        &self.wb.sheets[self.active.min(self.wb.sheets.len() - 1)]
    }

    pub(crate) fn sheet_mut(&mut self) -> &mut Sheet {
        let i = self.active.min(self.wb.sheets.len() - 1);
        &mut self.wb.sheets[i]
    }

    /// Switches sheet: the selection goes back to A1, the filters and the history reset (the web resets
    /// its history per sheet).
    pub fn set_active(&mut self, index: usize) {
        if index >= self.wb.sheets.len() || index == self.active {
            return;
        }
        self.active = index;
        self.sel = Selection::cell(Pos::new(0, 1));
        self.col_filters.clear();
        self.history = History::default();
        self.invalidate_view();
    }

    /// Adds an empty sheet after the last one and activates it (`createSheetMut`: the server names it
    /// « Feuille N » when no name is given, N = the sheet count after the insertion).
    pub fn add_sheet(&mut self, id: String, name: Option<String>) -> usize {
        let position = self.wb.sheets.iter().map(|s| s.meta.position).max().unwrap_or(-1) + 1;
        let name = name.unwrap_or_else(|| format!("Feuille {}", self.wb.sheets.len() + 1));
        let meta = crate::model::sheet::SheetMeta { id, name, position, raw: Default::default() };
        self.wb.sheets.push(Sheet::empty(meta));
        self.touch();
        let last = self.wb.sheets.len() - 1;
        self.set_active(last);
        last
    }

    /// Renames sheet `index` (`renameSheetMut`); an empty name is ignored.
    pub fn rename_sheet(&mut self, index: usize, name: &str) -> bool {
        let name = name.trim();
        match self.wb.sheets.get_mut(index) {
            Some(s) if !name.is_empty() && s.meta.name != name => {
                s.meta.name = name.to_string();
                self.touch();
                true
            }
            _ => false,
        }
    }

    /// Deletes sheet `index` (`deleteSheetMut`); the last sheet stays. The deleted active sheet hands over
    /// to the first one, as on the web.
    pub fn delete_sheet(&mut self, index: usize) -> bool {
        if self.wb.sheets.len() <= 1 || index >= self.wb.sheets.len() {
            return false;
        }
        let was_active = index == self.active;
        self.wb.sheets.remove(index);
        if was_active {
            self.active = 0;
            self.sel = Selection::cell(Pos::new(0, 1));
            self.col_filters.clear();
            self.history = History::default();
        } else if index < self.active {
            self.active -= 1;
        }
        self.touch();
        true
    }

    /// The data changed: caches go, the revision moves.
    pub fn touch(&mut self) {
        self.rev += 1;
        self.invalidate_view();
    }

    fn invalidate_view(&self) {
        self.calc.borrow_mut().rev = 0;
        *self.geom.borrow_mut() = None;
    }

    /// The workbook's change counter.
    pub fn revision(&self) -> u64 {
        self.rev
    }

    pub fn is_dirty(&self) -> bool {
        self.rev != self.saved_rev
    }

    pub fn mark_saved(&mut self) {
        self.saved_rev = self.rev;
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// The environment (local time, randomness) formulas see.
    pub fn env(&self) -> &dyn Env {
        &*self.env
    }

    // ── Evaluation ──────────────────────────────────────────────────────────

    /// Runs `f` with the active sheet's evaluation context (the spill index and the conditional formats
    /// computed once per revision).
    pub fn with_eval<R>(&self, f: impl FnOnce(Data<'_>, &Ctx<'_>, Option<&SpillIndex>, &HashMap<String, CondStyle>) -> R) -> R {
        let names = self.wb.names_map();
        let by_name = self.wb.sheets_by_name();
        let sheet = self.sheet();
        let data = Data { cells: &sheet.cells, sheets: by_name.as_ref(), names: &names };
        let ctx = Ctx::new(&*self.env);
        {
            let mut calc = self.calc.borrow_mut();
            if calc.rev != self.rev {
                let spill = compute_spills(data, &ctx);
                calc.cf = compute_cond_formats(sheet.cf(), data, &ctx);
                calc.spill = Some(spill);
                calc.rev = self.rev;
            }
        }
        let calc = self.calc.borrow();
        f(data, &ctx, calc.spill.as_ref(), &calc.cf)
    }

    /// The spill index of the active sheet.
    pub fn spill(&self) -> SpillIndex {
        self.with_eval(|_, _, s, _| s.cloned().unwrap_or_default())
    }

    /// What cell (col, row) of the active sheet shows.
    pub fn shown(&self, col: u32, row: u32) -> CellShown {
        let key = addr::key(col, row);
        let sheet = self.sheet();
        let style = sheet.style_at(col, row);
        self.with_eval(|data, ctx, spill, _| grid::shown(sheet.cells.get(&key), &key, &style, data, ctx, spill, &self.lang))
    }

    /// `cellText` (no raw number-format code): the filter lists, find.
    pub fn cell_text(&self, col: u32, row: u32) -> String {
        let key = addr::key(col, row);
        let sheet = self.sheet();
        self.with_eval(|data, ctx, spill, _| grid::cell_text(sheet.cells.get(&key), &key, data, ctx, spill, &self.lang))
    }

    /// The conditional-format overrides of a cell.
    pub fn cond_style(&self, key: &str) -> Option<CondStyle> {
        self.with_eval(|_, _, _, cf| cf.get(key).cloned())
    }

    /// The effective style (column < row < cell).
    pub fn style_at(&self, col: u32, row: u32) -> Style {
        self.sheet().style_at(col, row)
    }

    /// The formula bar's text for the active cell (`f ?? cellDisplay`).
    pub fn formula_bar_text(&self) -> String {
        let key = self.sel.active.key();
        match self.sheet().cells.get(&key) {
            Some(c) => match &c.f {
                Some(serde_json::Value::String(f)) => f.clone(),
                Some(serde_json::Value::Null) | None => c.display(),
                Some(other) => crate::model::cell::js_string_of(other),
            },
            None => String::new(),
        }
    }

    // ── Geometry ────────────────────────────────────────────────────────────

    /// Rows hidden by collapsed groups and active filters (`hiddenRows`).
    pub fn hidden_rows(&self) -> HashSet<u32> {
        let (mut rows, _) = grid::group_hidden(self.sheet());
        let active: Vec<(&u32, &HashSet<String>)> = self.col_filters.iter().collect();
        if !active.is_empty() {
            let (_, max_row) = self.sheet().used_extent();
            for r in 1..=max_row.max(1) {
                for (c, allowed) in &active {
                    if !allowed.contains(&self.cell_text(**c, r)) {
                        rows.insert(r);
                        break;
                    }
                }
            }
        }
        rows
    }

    /// The geometry at the current zoom (cached until the data or the zoom changes).
    pub fn geometry(&self) -> Rc<Geometry> {
        if let Some((rev, zoom, g)) = self.geom.borrow().as_ref() {
            if *rev == self.rev && *zoom == self.zoom {
                return g.clone();
            }
        }
        let (_, hidden_cols) = grid::group_hidden(self.sheet());
        let g = Rc::new(Geometry::new(self.sheet(), self.zoom, &self.hidden_rows(), &hidden_cols));
        *self.geom.borrow_mut() = Some((self.rev, self.zoom, g.clone()));
        g
    }

    /// The zoom, clamped to the web's 25–300 %.
    pub fn set_zoom(&mut self, z: f64) {
        let z = ((z.clamp(0.25, 3.0)) * 100.0).round() / 100.0;
        if z != self.zoom {
            self.zoom = z;
            *self.geom.borrow_mut() = None;
        }
    }

    /// The merges of the active sheet.
    pub fn merges(&self) -> MergeInfo {
        MergeInfo::new(&self.sheet().merges())
    }

    // ── Selection ───────────────────────────────────────────────────────────

    /// `anchorOf`: a cell inside a merge stands for the merge's top-left cell.
    pub fn anchor_of(&self, c: Pos) -> Pos {
        match self.sheet().merges().iter().find(|r| r.contains(c)) {
            Some(r) => r.top_left(),
            None => c,
        }
    }

    /// Selects one cell (snapped to its merge's anchor).
    pub fn select(&mut self, c: Pos) {
        self.sel = Selection::cell(self.anchor_of(c));
    }

    /// Extends the selection to `c` (Shift+click, drag).
    pub fn extend_to(&mut self, c: Pos) {
        self.sel.end = Some(c);
    }

    /// Selects a rectangle with `active` as the active cell.
    pub fn select_rect(&mut self, active: Pos, end: Pos) {
        self.sel = Selection { active, end: Some(end) };
    }

    /// Ctrl+A: A1 to the used extent (`SA:4436`).
    pub fn select_all_used(&mut self) {
        let (c, r) = self.sheet().used_extent();
        self.sel = Selection { active: Pos::new(0, 1), end: Some(Pos::new(c, r.max(1))) };
    }

    /// The corner: the whole grid.
    pub fn select_everything(&mut self) {
        self.sel = Selection { active: Pos::new(0, 1), end: Some(Pos::new(MAX_COLS - 1, MAX_ROWS)) };
    }

    /// A column header: the whole column(s) from `c1` to `c2`.
    pub fn select_cols(&mut self, c1: u32, c2: u32) {
        self.sel = Selection { active: Pos::new(c1, 1), end: Some(Pos::new(c2, MAX_ROWS)) };
    }

    /// A row header: the whole row(s).
    pub fn select_rows(&mut self, r1: u32, r2: u32) {
        self.sel = Selection { active: Pos::new(0, r1), end: Some(Pos::new(MAX_COLS - 1, r2)) };
    }

    /// `moveSelection(dir)`: one step, a merge counting as one cell, hidden rows skipped.
    pub fn move_selection(&mut self, dir: Dir) {
        let prev = self.sel.active;
        let cur = self.sheet().merges().into_iter().find(|r| r.contains(prev)).unwrap_or(Rect::cell(prev));
        let hidden = self.hidden_rows();
        let (mut nc, mut nr) = (cur.c1, cur.r1);
        match dir {
            Dir::Right => nc = (cur.c2 + 1).min(MAX_COLS - 1),
            Dir::Left => nc = cur.c1.saturating_sub(1),
            Dir::Down => {
                let mut r = cur.r2;
                loop {
                    r += 1;
                    if r > MAX_ROWS || !hidden.contains(&r) {
                        break;
                    }
                }
                if r <= MAX_ROWS {
                    nr = r;
                }
            }
            Dir::Up => {
                let mut r = cur.r1 as i64;
                loop {
                    r -= 1;
                    if r < 1 || !hidden.contains(&(r as u32)) {
                        break;
                    }
                }
                if r >= 1 {
                    nr = r as u32;
                }
            }
        }
        self.sel = Selection::cell(self.anchor_of(Pos::new(nc, nr)));
    }

    /// Moves the other corner of the range by one cell (Shift+arrow — a desktop convention the web does
    /// not have; the active cell stays).
    pub fn extend_selection(&mut self, dir: Dir) {
        let e = self.sel.end.unwrap_or(self.sel.active);
        let n = match dir {
            Dir::Right => Pos::new((e.col + 1).min(MAX_COLS - 1), e.row),
            Dir::Left => Pos::new(e.col.saturating_sub(1), e.row),
            Dir::Down => Pos::new(e.col, (e.row + 1).min(MAX_ROWS)),
            Dir::Up => Pos::new(e.col, e.row.saturating_sub(1).max(1)),
        };
        self.sel.end = Some(n);
    }

    /// `jumpToEdge(dir, extend)`: Ctrl+arrow to the edge of the data block.
    pub fn jump_to_edge(&mut self, dir: Dir, extend: bool) {
        let cells = &self.sheet().cells;
        let has = |c: i64, r: i64| -> bool { cells.get(&addr::key(c as u32, r as u32)).is_some_and(|v| v.has_value() || v.has_f()) };
        let (dc, dr): (i64, i64) = match dir {
            Dir::Left => (-1, 0),
            Dir::Right => (1, 0),
            Dir::Up => (0, -1),
            Dir::Down => (0, 1),
        };
        let from = if extend { self.sel.end.unwrap_or(self.sel.active) } else { self.sel.active };
        let in_r = |c: i64, r: i64| c >= 0 && c < MAX_COLS as i64 && r >= 1 && r <= MAX_ROWS as i64;
        let (mut c, mut r) = (from.col as i64, from.row as i64);
        let (mut nc, mut nr) = (c + dc, r + dr);
        if in_r(nc, nr) {
            if has(nc, nr) {
                while in_r(nc + dc, nr + dr) && has(nc + dc, nr + dr) {
                    nc += dc;
                    nr += dr;
                }
            } else {
                while in_r(nc + dc, nr + dr) && !has(nc + dc, nr + dr) {
                    nc += dc;
                    nr += dr;
                }
                if in_r(nc + dc, nr + dr) {
                    nc += dc;
                    nr += dr;
                }
            }
            c = nc;
            r = nr;
        }
        let p = Pos::new(c as u32, r as u32);
        if extend {
            self.sel.end = Some(p);
        } else {
            self.sel = Selection::cell(p);
        }
    }

    /// Ctrl+Home: A1.
    pub fn go_home(&mut self) {
        self.sel = Selection::cell(Pos::new(0, 1));
    }

    /// The bottom-right corner of the selection (the fill handle), extended over a merge there.
    pub fn handle_corner(&self) -> Pos {
        let r = self.sel.rect();
        let corner = Pos::new(r.c2, r.r2);
        match self.sheet().merges().into_iter().find(|m| m.contains(corner)) {
            Some(m) => Pos::new(m.c2, m.r2),
            None => corner,
        }
    }

    /// The selected rectangle, clamped to the used extent when it spans whole rows/columns (the
    /// web's `bounds()` caps of whole-column operations).
    pub fn bounded_selection(&self) -> Rect {
        let b = self.sel.rect();
        let (uc, ur) = self.sheet().used_extent();
        Rect::new(b.c1, b.r1, b.c2.min(uc.max(b.c1)), b.r2.min(ur.max(b.r1)))
    }

    /// The cells of the active sheet.
    pub fn cells(&self) -> &CellMap {
        &self.sheet().cells
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::formula::testing::env;
    use crate::model::sheet::SheetMeta;
    use crate::model::workbook::Workbook;
    use serde_json::json;

    pub fn editor(cells: serde_json::Value) -> Editor {
        let mut wb = Workbook::new_local("t", "s1");
        wb.sheets[0] = Sheet::from_data(SheetMeta { id: "s1".into(), name: "Feuille 1".into(), position: 0, raw: Default::default() }, &json!({ "cells": cells }));
        Editor::new(wb, Rc::new(env()))
    }

    #[test]
    fn navigation_follows_the_web() {
        let mut e = editor(json!({"A1": {"v": 1}, "A2": {"v": 2}, "A3": {"v": 3}, "A6": {"v": 6}}));
        e.jump_to_edge(Dir::Down, false);
        assert_eq!(e.sel.active, Pos::new(0, 3));
        e.jump_to_edge(Dir::Down, false);
        assert_eq!(e.sel.active, Pos::new(0, 6));
        e.jump_to_edge(Dir::Down, false);
        assert_eq!(e.sel.active, Pos::new(0, MAX_ROWS));
        e.go_home();
        e.move_selection(Dir::Left);
        assert_eq!(e.sel.active, Pos::new(0, 1));
        e.move_selection(Dir::Right);
        assert_eq!(e.sel.active, Pos::new(1, 1));
        e.select_all_used();
        assert_eq!(e.sel.rect(), Rect::new(0, 1, 0, 6));
    }

    #[test]
    fn display_and_formula_bar() {
        let mut e = editor(json!({"A1": {"v": 2}, "B1": {"f": "=A1*21"}}));
        assert_eq!(e.shown(1, 1).text, "42");
        e.select(Pos::new(1, 1));
        assert_eq!(e.formula_bar_text(), "=A1*21");
    }

    #[test]
    fn sheets_are_added_renamed_and_deleted_like_the_web() {
        let mut e = editor(json!({"A1": {"v": 1}}));
        assert_eq!(e.add_sheet("s2".into(), None), 1);
        assert_eq!(e.wb.sheets[1].meta.name, "Feuille 2");
        assert_eq!(e.wb.sheets[1].meta.position, 1);
        assert_eq!(e.active_index(), 1);
        assert!(e.rename_sheet(1, " Budget "));
        assert_eq!(e.wb.sheets[1].meta.name, "Budget");
        assert!(!e.rename_sheet(1, "  "));
        assert!(e.delete_sheet(1));
        assert_eq!(e.active_index(), 0);
        assert!(!e.delete_sheet(0), "the last sheet stays");
    }
}
