//! The editing operations of the web editor, each one a port of its handler in `SpreadsheetApp.tsx`
//! (line numbers in the doc of each function). Every change goes through [`Editor::commit`]: the
//! history records what the web records, the revision moves (the save and the caches follow it).

use std::collections::{HashMap, HashSet};

use serde_json::{json, Map, Value as Json};

use super::history::Snap;
use super::{Editor, Selection};
use crate::addr::{self, index_to_col, Cell as Pos, Rect, MAX_COLS, MAX_ROWS};
use crate::formula::eval::CellMap;
use crate::js;
use crate::model::cell::{json_number, Cell};
use crate::model::style::Style;
use crate::fill::{self, Fill};
use crate::refs;
use crate::validation::{self, DVBlock, DVCriterion, DataValidation};

/// One copied block (`clipboard` ref, `SA:708`).
#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub cells: Vec<Vec<Option<Cell>>>,
    pub rows: u32,
    pub cols: u32,
    pub cut: bool,
    pub origin_col: u32,
    pub origin_row: u32,
}

/// The kinds of paste special (`pasteSpecial`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteMode {
    Values,
    Format,
    Formula,
    Transpose,
    NoBorders,
}

/// The border presets of the ribbon (`applyBorders`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Borders {
    All,
    Outer,
    Inner,
    Top,
    Bottom,
    Left,
    Right,
    None,
}

/// An axis of a structural edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Row,
    Col,
}

/// Why an edit was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// The sheet is password-protected (`guardProtected`).
    Protected,
    /// A validation rule rejects the value; the text to show (`rule.help` or the default).
    Invalid(String),
}

/// One sort level of the custom sort (`SortLevel`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SortLevel {
    pub col: u32,
    pub asc: bool,
}

/// A style patch: key → value, `None` removes the key (`mergeStyle`).
pub type StylePatch = Vec<(&'static str, Option<Json>)>;

fn merge_style(base: Option<&Json>, patch: &StylePatch) -> Json {
    let mut s = base.and_then(Json::as_object).cloned().unwrap_or_default();
    for (k, v) in patch {
        match v {
            Some(v) => {
                s.insert((*k).to_string(), v.clone());
            }
            None => {
                s.remove(*k);
            }
        }
    }
    Json::Object(s)
}

impl Editor {
    /// Applies `f` to the active sheet as one change: refused on a protected sheet, recorded in the
    /// history, the revision bumped.
    pub fn commit(&mut self, f: impl FnOnce(&mut Editor)) -> Result<(), Refused> {
        if self.sheet().is_protected() {
            return Err(Refused::Protected);
        }
        let before = Snap::of(self.sheet());
        f(self);
        let i = self.active_index();
        let mut history = std::mem::take(&mut self.history);
        history.record(before, &self.wb.sheets[i]);
        self.history = history;
        self.touch();
        Ok(())
    }

    /// `writeCells`: each entry replaces a cell; an empty or void cell is deleted.
    pub fn write_cells(&mut self, entries: Vec<(Pos, Option<Cell>)>) -> Result<(), Refused> {
        self.commit(|e| {
            let cells = &mut e.sheet_mut().cells;
            for (p, cell) in entries {
                let k = p.key();
                match cell {
                    Some(c) if !c.is_void() => {
                        cells.insert(k, c);
                    }
                    _ => {
                        cells.remove(&k);
                    }
                }
            }
        })
    }

    // ── Cell input ──────────────────────────────────────────────────────────

    /// The data-validation rule of a cell (`dvAt`: the first block covering it).
    pub fn validation_at(&self, c: Pos) -> Option<DataValidation> {
        validation_blocks(self.sheet().validations()).into_iter().find(|b| b.ranges.iter().any(|r| Rect::parse(r).is_some_and(|m| m.contains(c)))).map(|b| b.rule)
    }

    /// `dvListValues(rule)`: a list's values, or the source range's displayed values (trimmed, empty ones
    /// dropped, duplicates removed).
    pub fn validation_list(&self, rule: &DataValidation) -> Vec<String> {
        match &rule.crit {
            DVCriterion::List { values, .. } => values.clone(),
            DVCriterion::ListRange { source, .. } => {
                let Some(b) = Rect::parse(source) else { return Vec::new() };
                let mut seen = HashSet::new();
                let mut out = Vec::new();
                for p in b.cells() {
                    let key = p.key();
                    let v = self.with_eval(|data, ctx, spill, _| crate::grid::cell_value(self.sheet().cells.get(&key), &key, data, ctx, spill).1);
                    let t = js::trim(&v).to_string();
                    if !t.is_empty() && seen.insert(t.clone()) {
                        out.push(t);
                    }
                }
                out
            }
            _ => Vec::new(),
        }
    }

    /// `updateCell(col, row, raw)`: a formula (balanced and normalised), a number when `Number()` reads
    /// one, else text; empty → `v: null`. A validation rule with `reject` refuses an invalid value.
    pub fn update_cell(&mut self, at: Pos, raw0: &str) -> Result<(), Refused> {
        if self.sheet().is_protected() {
            return Err(Refused::Protected);
        }
        let raw = if raw0.starts_with('=') { refs::normalize_formula(&refs::auto_balance(raw0)) } else { raw0.to_string() };
        let is_formula = raw.starts_with('=');
        if !raw.is_empty() && !is_formula {
            if let Some(rule) = self.validation_at(at) {
                if !validation::is_checkbox(Some(&rule)) && rule.reject == Some(true) {
                    let allowed = if matches!(rule.crit, DVCriterion::ListRange { .. }) { Some(self.validation_list(&rule)) } else { None };
                    if !validation::validate_value(&rule.crit, &raw, allowed.as_deref()) {
                        return Err(Refused::Invalid(rule.help.clone().unwrap_or_default()));
                    }
                }
            }
        }
        let key = at.key();
        self.commit(|e| {
            let cells = &mut e.sheet_mut().cells;
            let mut cell = cells.get(&key).cloned().unwrap_or_default();
            if is_formula {
                cell.v = None;
                cell.f = Some(Json::String(raw.clone()));
            } else {
                cell.f = None;
                cell.v = Some(if raw.is_empty() {
                    Json::Null
                } else {
                    match js::number(&raw) {
                        Some(n) if !js::trim(&raw).is_empty() => json_number(n),
                        _ => Json::String(raw.clone()),
                    }
                });
            }
            cells.insert(key, cell);
        })
    }

    /// `toggleCheckbox`: stores the opposite boolean.
    pub fn toggle_checkbox(&mut self, at: Pos) -> Result<(), Refused> {
        let key = at.key();
        self.commit(|e| {
            let cells = &mut e.sheet_mut().cells;
            let mut cell = cells.get(&key).cloned().unwrap_or_default();
            let checked = matches!(&cell.v, Some(Json::Bool(true))) || matches!(&cell.v, Some(Json::String(s)) if s == "TRUE") || cell.v.as_ref().and_then(Json::as_f64) == Some(1.0);
            cell.v = Some(Json::Bool(!checked));
            cell.f = None;
            cells.insert(key, cell);
        })
    }

    /// `clearSelection`: the selected cells removed whole (value, formula, style, note), capped to the
    /// used extent.
    pub fn clear_selection(&mut self) -> Result<(), Refused> {
        let b = self.bounded_selection();
        self.write_cells(b.cells().map(|p| (p, None)).collect())
    }

    /// The active cell's text with `sym` appended (`insertSymbol`).
    pub fn insert_symbol(&mut self, sym: &str) -> Result<(), Refused> {
        let at = self.sel.active;
        let cur = match self.sheet().cells.get(&at.key()) {
            Some(c) => match &c.f {
                Some(Json::String(f)) => f.clone(),
                _ => {
                    if c.v_is_nullish() {
                        String::new()
                    } else {
                        c.display()
                    }
                }
            },
            None => String::new(),
        };
        self.update_cell(at, &format!("{cur}{sym}"))
    }

    /// `setCellComment`: the note trimmed, removed when empty.
    pub fn set_comment(&mut self, at: Pos, text: Option<&str>) -> Result<(), Refused> {
        let mut cell = self.sheet().cells.get(&at.key()).cloned().unwrap_or_default();
        match text.map(js::trim).filter(|t| !t.is_empty()) {
            Some(t) => cell.c = Some(Json::String(t.to_string())),
            None => cell.c = None,
        }
        self.write_cells(vec![(at, Some(cell))])
    }

    // ── Styles ──────────────────────────────────────────────────────────────

    /// The style shown in the ribbon: the active cell's effective style.
    pub fn selected_style(&self) -> Style {
        self.style_at(self.sel.active.col, self.sel.active.row)
    }

    /// `applyToSelection(patch)`: whole columns/rows store the patch at column/row level (and strip those
    /// keys from the cells of the band); a bounded rectangle patches every cell.
    pub fn apply_to_selection(&mut self, patch: StylePatch) -> Result<(), Refused> {
        let b = self.sel.rect();
        let whole_cols = self.sel.whole_cols();
        let whole_rows = self.sel.whole_rows();
        self.commit(|e| {
            let sheet = e.sheet_mut();
            if whole_cols || whole_rows {
                if whole_cols {
                    let mut cs = sheet.data.get("col_styles").and_then(Json::as_object).cloned().unwrap_or_default();
                    for ci in b.c1..=b.c2 {
                        let l = index_to_col(ci as i64);
                        let merged = merge_style(cs.get(&l), &patch);
                        cs.insert(l, merged);
                    }
                    sheet.data.insert("col_styles".into(), Json::Object(cs));
                }
                if whole_rows {
                    let mut rs = sheet.data.get("row_styles").and_then(Json::as_object).cloned().unwrap_or_default();
                    for ri in b.r1..=b.r2 {
                        let merged = merge_style(rs.get(&ri.to_string()), &patch);
                        rs.insert(ri.to_string(), merged);
                    }
                    sheet.data.insert("row_styles".into(), Json::Object(rs));
                }
                for (key, cell) in sheet.cells.iter_mut() {
                    let Some(p) = addr::Cell::parse(key) else { continue };
                    let in_band = (whole_cols && p.col >= b.c1 && p.col <= b.c2) || (whole_rows && p.row >= b.r1 && p.row <= b.r2);
                    if !in_band {
                        continue;
                    }
                    if let Some(Json::Object(s)) = &mut cell.s {
                        for (k, _) in &patch {
                            s.remove(*k);
                        }
                    }
                }
                return;
            }
            for ci in b.c1..=b.c2 {
                for ri in b.r1..=b.r2 {
                    let k = addr::key(ci, ri);
                    let mut cell = sheet.cells.get(&k).cloned().unwrap_or_default();
                    cell.s = Some(merge_style(cell.s.as_ref(), &patch));
                    sheet.cells.insert(k, cell);
                }
            }
        })
    }

    /// `toggleStyle(prop)`: the opposite of the active cell's effective value.
    pub fn toggle_style(&mut self, prop: &'static str) -> Result<(), Refused> {
        let s = self.selected_style();
        let cur = match prop {
            "bold" => s.is_bold(),
            "italic" => s.is_italic(),
            "underline" => s.is_underline(),
            "strike" => s.is_strike(),
            "wrap" => s.is_wrap(),
            _ => false,
        };
        self.apply_to_selection(vec![(prop, Some(Json::Bool(!cur)))])
    }

    /// `setAlign`.
    pub fn set_align(&mut self, align: &str) -> Result<(), Refused> {
        self.apply_to_selection(vec![("align", Some(Json::String(align.to_string())))])
    }

    /// `setNumFmt`: toggles the format off when it is already the active cell's.
    pub fn set_num_fmt(&mut self, fmt: &str) -> Result<(), Refused> {
        let cur = self.selected_style().num_fmt;
        let v = if cur.as_deref() == Some(fmt) { None } else { Some(Json::String(fmt.to_string())) };
        self.apply_to_selection(vec![("numFmt", v)])
    }

    /// `adjustDecimals(delta)`: from the active cell's decimals (2 for currency/percent, else 0), 0–10.
    pub fn adjust_decimals(&mut self, delta: i32) -> Result<(), Refused> {
        let s = self.selected_style();
        let base = s.decimals.unwrap_or(if matches!(s.num_fmt.as_deref(), Some("currency") | Some("percent")) { 2.0 } else { 0.0 });
        let d = (base + delta as f64).clamp(0.0, 10.0);
        self.apply_to_selection(vec![("decimals", Some(json_number(d)))])
    }

    /// The font family or size.
    pub fn set_font(&mut self, family: Option<&str>, size: Option<f64>) -> Result<(), Refused> {
        let mut p: StylePatch = Vec::new();
        if let Some(f) = family {
            p.push(("fontFamily", Some(Json::String(f.to_string()))));
        }
        if let Some(s) = size {
            p.push(("fontSize", Some(json_number(s))));
        }
        self.apply_to_selection(p)
    }

    /// The text colour (`None` = automatic).
    pub fn set_color(&mut self, color: Option<&str>) -> Result<(), Refused> {
        self.apply_to_selection(vec![("color", color.map(|c| Json::String(c.to_string())))])
    }

    /// The fill colour (`None` = no fill); a solid fill removes a gradient (`ColorField` of the web's fill
    /// popover sets `bg` and clears `bgGradient`).
    pub fn set_fill(&mut self, color: Option<&str>) -> Result<(), Refused> {
        self.apply_to_selection(vec![("bg", color.map(|c| Json::String(c.to_string()))), ("bgGradient", None)])
    }

    /// A gradient fill.
    pub fn set_gradient(&mut self, gradient: Json) -> Result<(), Refused> {
        self.apply_to_selection(vec![("bgGradient", Some(gradient))])
    }

    /// `applyBorders(kind, color)`.
    pub fn apply_borders(&mut self, kind: Borders, color: &str) -> Result<(), Refused> {
        let b = self.sel.rect();
        let (whole_cols, whole_rows) = (self.sel.whole_cols(), self.sel.whole_rows());
        let (uc, ur) = self.sheet().used_extent();
        let c2 = if whole_rows { b.c2.min(uc.max(b.c1)) } else { b.c2 };
        let r2 = if whole_cols { b.r2.min(ur.max(b.r1)) } else { b.r2 };
        let color = color.to_string();
        self.commit(|e| {
            let cells = &mut e.sheet_mut().cells;
            for ci in b.c1..=c2 {
                for ri in b.r1..=r2 {
                    let k = addr::key(ci, ri);
                    let mut cell = cells.get(&k).cloned().unwrap_or_default();
                    let mut s: Map<String, Json> = cell.s.as_ref().and_then(Json::as_object).cloned().unwrap_or_default();
                    let (is_top, is_bottom, is_left, is_right) = (ri == b.r1, ri == r2, ci == b.c1, ci == c2);
                    let set = |edge: &str, on: bool, s: &mut Map<String, Json>| {
                        if on {
                            s.insert(edge.into(), Json::String(color.clone()));
                        } else {
                            s.remove(edge);
                        }
                    };
                    match kind {
                        Borders::None => {
                            for k in ["bt", "br", "bb", "bl"] {
                                s.remove(k);
                            }
                        }
                        Borders::All => {
                            for k in ["bt", "br", "bb", "bl"] {
                                s.insert(k.into(), Json::String(color.clone()));
                            }
                        }
                        Borders::Outer => {
                            set("bt", is_top, &mut s);
                            set("bb", is_bottom, &mut s);
                            set("bl", is_left, &mut s);
                            set("br", is_right, &mut s);
                        }
                        Borders::Inner => {
                            set("bt", !is_top, &mut s);
                            set("bb", !is_bottom, &mut s);
                            set("bl", !is_left, &mut s);
                            set("br", !is_right, &mut s);
                        }
                        Borders::Top => {
                            if is_top {
                                s.insert("bt".into(), Json::String(color.clone()));
                            }
                        }
                        Borders::Bottom => {
                            if is_bottom {
                                s.insert("bb".into(), Json::String(color.clone()));
                            }
                        }
                        Borders::Left => {
                            if is_left {
                                s.insert("bl".into(), Json::String(color.clone()));
                            }
                        }
                        Borders::Right => {
                            if is_right {
                                s.insert("br".into(), Json::String(color.clone()));
                            }
                        }
                    }
                    cell.s = Some(Json::Object(s));
                    cells.insert(k, cell);
                }
            }
        })
    }

    /// `clearFormats`: the styles of the selection (and the column/row styles of a whole column/row).
    pub fn clear_formats(&mut self) -> Result<(), Refused> {
        let b = self.sel.rect();
        let (whole_cols, whole_rows) = (self.sel.whole_cols(), self.sel.whole_rows());
        let (uc, ur) = self.sheet().used_extent();
        self.commit(|e| {
            let sheet = e.sheet_mut();
            if whole_cols {
                if let Some(Json::Object(cs)) = sheet.data.get_mut("col_styles") {
                    for ci in b.c1..=b.c2 {
                        cs.remove(&index_to_col(ci as i64));
                    }
                }
            }
            if whole_rows {
                if let Some(Json::Object(rs)) = sheet.data.get_mut("row_styles") {
                    for ri in b.r1..=b.r2 {
                        rs.remove(&ri.to_string());
                    }
                }
            }
            let c2 = if whole_rows { b.c2.min(uc.max(b.c1)) } else { b.c2 };
            let r2 = if whole_cols { b.r2.min(ur.max(b.r1)) } else { b.r2 };
            for ci in b.c1..=c2 {
                for ri in b.r1..=r2 {
                    if let Some(cell) = sheet.cells.get_mut(&addr::key(ci, ri)) {
                        if cell.s.is_some() {
                            cell.s = None;
                        }
                    }
                }
            }
        })
    }

    /// The format painter's capture: the active cell's own style.
    pub fn capture_format(&self) -> Option<Json> {
        self.sheet().cells.get(&self.sel.active.key()).and_then(|c| c.s.clone())
    }

    /// `applyPainter(style)`: stamps a style over the selection (capped to the used extent).
    pub fn apply_painter(&mut self, style: Option<Json>) -> Result<(), Refused> {
        let b = self.bounded_selection();
        self.commit(|e| {
            let cells = &mut e.sheet_mut().cells;
            let has = style.as_ref().and_then(Json::as_object).is_some_and(|o| !o.is_empty());
            for p in b.cells() {
                let k = p.key();
                if has {
                    let mut cell = cells.get(&k).cloned().unwrap_or_default();
                    cell.s = style.clone();
                    cells.insert(k, cell);
                } else if let Some(cell) = cells.get_mut(&k) {
                    cell.s = None;
                }
            }
        })
    }

    // ── Merges, sizes, freeze, gridlines, groups ────────────────────────────

    /// Whether the selection overlaps a merge (`selectionHasMerge`).
    pub fn selection_has_merge(&self) -> bool {
        let b = self.sel.rect();
        self.sheet().merges().iter().any(|m| m.intersects(&b))
    }

    /// `mergeSelection` (capped to the used extent) / `unmergeSelection` / `toggleMerge`.
    pub fn toggle_merge(&mut self) -> Result<(), Refused> {
        let b = self.sel.rect();
        let merges = self.sheet().merges();
        if self.selection_has_merge() {
            let kept: Vec<Rect> = merges.into_iter().filter(|m| !m.intersects(&b)).collect();
            return self.commit(|e| e.sheet_mut().set_merges(&kept));
        }
        if b.c1 == b.c2 && b.r1 == b.r2 {
            return Ok(());
        }
        let (uc, ur) = self.sheet().used_extent();
        let r2 = b.r2.min(ur.max(b.r1));
        let c2 = b.c2.min(uc.max(b.c1));
        if b.c1 == c2 && b.r1 == r2 {
            return Ok(());
        }
        let mut kept: Vec<Rect> = merges.into_iter().filter(|m| !m.intersects(&b)).collect();
        kept.push(Rect::new(b.c1, b.r1, c2, r2));
        self.commit(|e| e.sheet_mut().set_merges(&kept))
    }

    /// Column widths (px, 0 hides) for the selected columns.
    pub fn set_col_widths(&mut self, c1: u32, c2: u32, w: f64) -> Result<(), Refused> {
        self.commit(|e| {
            for c in c1..=c2 {
                e.sheet_mut().set_col_width(c, w);
            }
        })
    }

    /// Row heights (px, 0 hides) for the selected rows.
    pub fn set_row_heights(&mut self, r1: u32, r2: u32, h: f64) -> Result<(), Refused> {
        self.commit(|e| {
            for r in r1..=r2 {
                e.sheet_mut().set_row_height(r, h);
            }
        })
    }

    /// `hideCols` / `hideRows`.
    pub fn hide_selection(&mut self, axis: Axis) -> Result<(), Refused> {
        let b = self.sel.rect();
        match axis {
            Axis::Col => self.set_col_widths(b.c1, b.c2, 0.0),
            Axis::Row => self.set_row_heights(b.r1, b.r2, 0.0),
        }
    }

    /// `unhideCols` / `unhideRows`: hidden ones get the app default back.
    pub fn unhide_selection(&mut self, axis: Axis) -> Result<(), Refused> {
        let b = self.sel.rect();
        let sheet = self.sheet();
        match axis {
            Axis::Col => {
                let hidden: Vec<u32> = (b.c1..=b.c2).filter(|c| sheet.col_width_override(&index_to_col(*c as i64)).unwrap_or(crate::model::sheet::DEFAULT_COL_WIDTH) == 0.0).collect();
                if hidden.is_empty() {
                    return Ok(());
                }
                self.commit(|e| {
                    for c in hidden {
                        e.sheet_mut().set_col_width(c, crate::model::sheet::DEFAULT_COL_WIDTH);
                    }
                })
            }
            Axis::Row => {
                let hidden: Vec<u32> = (b.r1..=b.r2).filter(|r| sheet.row_height_override(*r).unwrap_or(crate::model::sheet::DEFAULT_ROW_HEIGHT) == 0.0).collect();
                if hidden.is_empty() {
                    return Ok(());
                }
                self.commit(|e| {
                    for r in hidden {
                        e.sheet_mut().set_row_height(r, crate::model::sheet::DEFAULT_ROW_HEIGHT);
                    }
                })
            }
        }
    }

    /// `autofitColumns(c1, c2)`: each column as wide as its widest text + 12 (40–600), measured by
    /// `measure(text, style)`.
    pub fn autofit_columns(&mut self, c1: u32, c2: u32, measure: &dyn Fn(&str, &Style) -> f64) -> Result<(), Refused> {
        let mut max: HashMap<u32, f64> = HashMap::new();
        let keys: Vec<(String, Pos)> = self.sheet().cells.keys().filter_map(|k| addr::Cell::parse(k).map(|p| (k.clone(), p))).filter(|(_, p)| p.col >= c1 && p.col <= c2).collect();
        for (k, p) in keys {
            let txt = self.cell_text(p.col, p.row);
            if txt.is_empty() {
                continue;
            }
            let s = self.sheet().cells.get(&k).map(Cell::style).unwrap_or_default();
            let w = measure(&txt, &s) + 12.0;
            if w > max.get(&p.col).copied().unwrap_or(0.0) {
                max.insert(p.col, w);
            }
        }
        self.commit(|e| {
            for c in c1..=c2 {
                let w = js::round(max.get(&c).copied().unwrap_or(crate::model::sheet::DEFAULT_COL_WIDTH)).clamp(40.0, 600.0);
                e.sheet_mut().set_col_width(c, w);
            }
        })
    }

    /// `toggleGridlines`.
    pub fn toggle_gridlines(&mut self) -> Result<(), Refused> {
        let next = !self.sheet().gridlines();
        self.commit(|e| e.sheet_mut().set("gridlines", Json::Bool(next)))
    }

    /// `applyFreeze(rows, cols)`.
    pub fn set_freeze(&mut self, rows: u32, cols: u32) -> Result<(), Refused> {
        let (r, c) = (rows.min(MAX_ROWS), cols.min(MAX_COLS));
        self.commit(|e| {
            let s = e.sheet_mut();
            s.set("frozen_rows", json!(r));
            s.set("frozen_cols", json!(c));
        })
    }

    fn set_groups(&mut self, key: &'static str, groups: Vec<crate::model::sheet::OutlineGroup>) -> Result<(), Refused> {
        let v = Json::Array(groups.iter().map(|g| json!({ "start": g.start, "end": g.end, "collapsed": g.collapsed })).collect());
        self.commit(|e| e.sheet_mut().set(key, v))
    }

    /// `groupRows` / `groupCols`: no exact duplicate, sorted by start then end.
    pub fn group(&mut self, axis: Axis) -> Result<(), Refused> {
        let b = self.sel.rect();
        let (key, mut groups, (s, e)) = match axis {
            Axis::Row => ("row_groups", self.sheet().row_groups(), (b.r1, b.r2)),
            Axis::Col => ("col_groups", self.sheet().col_groups(), (b.c1, b.c2)),
        };
        if groups.iter().any(|g| g.start == s && g.end == e) {
            return Ok(());
        }
        groups.push(crate::model::sheet::OutlineGroup { start: s, end: e, collapsed: false });
        groups.sort_by(|a, b| a.start.cmp(&b.start).then(a.end.cmp(&b.end)));
        self.set_groups(key, groups)
    }

    /// `ungroupRows` / `ungroupCols`: removes the groups overlapping the selection.
    pub fn ungroup(&mut self, axis: Axis) -> Result<(), Refused> {
        let b = self.sel.rect();
        let (key, groups, (s, e)) = match axis {
            Axis::Row => ("row_groups", self.sheet().row_groups(), (b.r1, b.r2)),
            Axis::Col => ("col_groups", self.sheet().col_groups(), (b.c1, b.c2)),
        };
        let next: Vec<_> = groups.iter().copied().filter(|g| g.end < s || g.start > e).collect();
        if next.len() == groups.len() {
            return Ok(());
        }
        self.set_groups(key, next)
    }

    /// `toggleRowGroup(collapse)` / `toggleColGroup`: the groups overlapping the selection.
    pub fn collapse_groups(&mut self, axis: Axis, collapse: Option<bool>) -> Result<(), Refused> {
        let b = self.sel.rect();
        let (key, groups, (s, e)) = match axis {
            Axis::Row => ("row_groups", self.sheet().row_groups(), (b.r1, b.r2)),
            Axis::Col => ("col_groups", self.sheet().col_groups(), (b.c1, b.c2)),
        };
        let mut done = false;
        let next: Vec<_> = groups
            .iter()
            .map(|g| {
                if g.start <= e && g.end >= s {
                    done = true;
                    crate::model::sheet::OutlineGroup { collapsed: collapse.unwrap_or(!g.collapsed), ..*g }
                } else {
                    *g
                }
            })
            .collect();
        if !done {
            return Ok(());
        }
        self.set_groups(key, next)
    }

    /// The outline toggle of the group starting at `start` (the header's ± button).
    pub fn toggle_group_at(&mut self, axis: Axis, start: u32) -> Result<(), Refused> {
        let (key, groups) = match axis {
            Axis::Row => ("row_groups", self.sheet().row_groups()),
            Axis::Col => ("col_groups", self.sheet().col_groups()),
        };
        let next: Vec<_> = groups.iter().map(|g| if g.start == start { crate::model::sheet::OutlineGroup { collapsed: !g.collapsed, ..*g } } else { *g }).collect();
        self.set_groups(key, next)
    }

    // ── Clipboard ───────────────────────────────────────────────────────────

    /// `copySelection(cut)` (capped to the used extent).
    pub fn copy_selection(&mut self, cut: bool) {
        let b = self.bounded_selection();
        let cells = &self.sheet().cells;
        let rows: Vec<Vec<Option<Cell>>> = (b.r1..=b.r2).map(|r| (b.c1..=b.c2).map(|c| cells.get(&addr::key(c, r)).cloned()).collect()).collect();
        self.clipboard = Some(Clip { cells: rows, rows: b.height(), cols: b.width(), cut, origin_col: b.c1, origin_row: b.r1 });
    }

    pub fn has_clip(&self) -> bool {
        self.clipboard.is_some()
    }

    /// The copied block as tab-separated text (what the OS clipboard gets: the displayed values).
    pub fn clip_text(&self) -> Option<String> {
        let clip = self.clipboard.as_ref()?;
        let mut lines = Vec::new();
        for r in 0..clip.rows {
            let mut parts = Vec::new();
            for c in 0..clip.cols {
                parts.push(self.cell_text(clip.origin_col + c, clip.origin_row + r));
            }
            lines.push(parts.join("\t"));
        }
        Some(lines.join("\r\n"))
    }

    /// `pasteSelection`: the block at the active cell, formulas translated; a cut clears its source and
    /// the clipboard; the pasted rectangle becomes the selection.
    pub fn paste(&mut self) -> Result<(), Refused> {
        let Some(cb) = self.clipboard.clone() else { return Ok(()) };
        let dest = self.sel.active;
        let (d_col, d_row) = (dest.col as i64 - cb.origin_col as i64, dest.row as i64 - cb.origin_row as i64);
        let mut entries = Vec::new();
        if cb.cut {
            for r in 0..cb.rows {
                for c in 0..cb.cols {
                    entries.push((Pos::new(cb.origin_col + c, cb.origin_row + r), None));
                }
            }
        }
        for r in 0..cb.rows {
            for c in 0..cb.cols {
                let (tc, tr) = (dest.col + c, dest.row + r);
                if tc >= MAX_COLS || tr > MAX_ROWS {
                    continue;
                }
                let src = cb.cells.get(r as usize).and_then(|row| row.get(c as usize)).cloned().flatten();
                let cell = src.map(|mut cell| {
                    if let Some(f) = cell.formula().map(str::to_string) {
                        cell.f = Some(Json::String(refs::translate_formula(&f, d_col, d_row)));
                    }
                    cell
                });
                entries.push((Pos::new(tc, tr), cell));
            }
        }
        self.write_cells(entries)?;
        if cb.cut {
            self.clipboard = None;
        }
        self.sel = Selection { active: dest, end: Some(Pos::new((dest.col + cb.cols - 1).min(MAX_COLS - 1), (dest.row + cb.rows - 1).min(MAX_ROWS))) };
        Ok(())
    }

    /// Pastes plain text (from the OS clipboard): rows by line, columns by tab, each value through
    /// `updateCell`'s parsing (a desktop addition: the web's clipboard is internal only).
    pub fn paste_text(&mut self, text: &str) -> Result<(), Refused> {
        let dest = self.sel.active;
        let text = text.strip_suffix("\r\n").or_else(|| text.strip_suffix('\n')).unwrap_or(text);
        let rows: Vec<Vec<&str>> = text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l).split('\t').collect()).collect();
        let mut entries = Vec::new();
        for (r, row) in rows.iter().enumerate() {
            for (c, raw) in row.iter().enumerate() {
                let p = Pos::new(dest.col + c as u32, dest.row + r as u32);
                if p.col >= MAX_COLS || p.row > MAX_ROWS {
                    continue;
                }
                let mut cell = self.sheet().cells.get(&p.key()).cloned().unwrap_or_default();
                if raw.starts_with('=') {
                    cell.v = None;
                    cell.f = Some(Json::String(refs::normalize_formula(&refs::auto_balance(raw))));
                } else {
                    cell.f = None;
                    cell.v = Some(if raw.is_empty() {
                        Json::Null
                    } else {
                        match js::number(raw) {
                            Some(n) if !js::trim(raw).is_empty() => json_number(n),
                            _ => Json::String((*raw).to_string()),
                        }
                    });
                }
                entries.push((p, Some(cell)));
            }
        }
        let h = rows.len() as u32;
        let w = rows.iter().map(Vec::len).max().unwrap_or(1) as u32;
        self.write_cells(entries)?;
        self.sel = Selection { active: dest, end: Some(Pos::new((dest.col + w.max(1) - 1).min(MAX_COLS - 1), (dest.row + h.max(1) - 1).min(MAX_ROWS))) };
        Ok(())
    }

    /// `pasteSpecial(mode)`.
    pub fn paste_special(&mut self, mode: PasteMode) -> Result<(), Refused> {
        let Some(cb) = self.clipboard.clone() else { return Ok(()) };
        let dest = self.sel.active;
        let strip = |s: Option<Json>| -> Option<Json> {
            s.map(|s| {
                let mut o = s.as_object().cloned().unwrap_or_default();
                for k in ["bt", "bb", "bl", "br", "btw", "bbw", "blw", "brw"] {
                    o.remove(k);
                }
                Json::Object(o)
            })
        };
        let mut entries = Vec::new();
        for r in 0..cb.rows {
            for c in 0..cb.cols {
                let src = cb.cells.get(r as usize).and_then(|row| row.get(c as usize)).cloned().flatten();
                let (tc, tr) = if mode == PasteMode::Transpose { (dest.col as i64 + r as i64, dest.row as i64 + c as i64) } else { (dest.col as i64 + c as i64, dest.row as i64 + r as i64) };
                if tc < 0 || tc >= MAX_COLS as i64 || tr < 1 || tr > MAX_ROWS as i64 {
                    continue;
                }
                let tp = Pos::new(tc as u32, tr as u32);
                let existing = self.sheet().cells.get(&tp.key()).cloned();
                let Some(src) = src else {
                    if mode != PasteMode::Format {
                        let keep = existing.and_then(|e| e.s).map(|s| Cell { s: Some(s), ..Cell::default() });
                        entries.push((tp, keep));
                    }
                    continue;
                };
                let d_col = tc - (cb.origin_col + c) as i64;
                let d_row = tr - (cb.origin_row + r) as i64;
                let tf = |f: &Option<Json>| -> Option<Json> {
                    match f {
                        Some(Json::String(s)) if s.starts_with('=') => Some(Json::String(refs::translate_formula(s, d_col, d_row))),
                        Some(Json::Null) | None => None,
                        Some(other) => Some(other.clone()),
                    }
                };
                let cell = match mode {
                    PasteMode::Format => Cell { s: src.s.clone(), ..existing.unwrap_or_default() },
                    PasteMode::Values => Cell { v: Some(src.v.clone().unwrap_or(Json::Null)), f: None, ..existing.unwrap_or_default() },
                    PasteMode::Formula => {
                        let has_f = src.has_f();
                        Cell { v: if has_f { None } else { Some(src.v.clone().unwrap_or(Json::Null)) }, f: tf(&src.f), ..existing.unwrap_or_default() }
                    }
                    PasteMode::Transpose | PasteMode::NoBorders => {
                        let mut c2 = src.clone();
                        c2.f = tf(&src.f);
                        if mode == PasteMode::NoBorders {
                            c2.s = strip(c2.s);
                        }
                        c2
                    }
                };
                entries.push((tp, Some(cell)));
            }
        }
        self.write_cells(entries)
    }

    // ── Structure ───────────────────────────────────────────────────────────

    /// `insertCellsShift(dir)`: blank cells, the band of populated cells moved right or down.
    pub fn insert_cells_shift(&mut self, down: bool) -> Result<(), Refused> {
        let b = self.sel.rect();
        let cnt = if down { b.height() } else { b.width() };
        self.commit(|e| {
            let old = std::mem::take(&mut e.sheet_mut().cells);
            let mut out = CellMap::with_capacity(old.len());
            for (key, cell) in old {
                let Some(p) = addr::Cell::parse(&key) else { continue };
                let in_band = if down { p.col >= b.c1 && p.col <= b.c2 && p.row >= b.r1 } else { p.row >= b.r1 && p.row <= b.r2 && p.col >= b.c1 };
                if !in_band {
                    out.insert(key, cell);
                    continue;
                }
                let (nc, nr) = if down { (p.col, p.row as u64 + cnt as u64) } else { (p.col + cnt, p.row as u64) };
                if nc < MAX_COLS && nr <= MAX_ROWS as u64 {
                    out.insert(addr::key(nc, nr as u32), cell);
                }
            }
            e.sheet_mut().cells = out;
        })
    }

    /// `deleteCellsShift(dir)`: the selected cells removed, the band below or right moved back.
    pub fn delete_cells_shift(&mut self, up: bool) -> Result<(), Refused> {
        let b = self.sel.rect();
        let cnt = if up { b.height() } else { b.width() };
        self.commit(|e| {
            let old = std::mem::take(&mut e.sheet_mut().cells);
            let mut out = CellMap::with_capacity(old.len());
            for (key, cell) in old {
                let Some(p) = addr::Cell::parse(&key) else { continue };
                if up {
                    if p.col < b.c1 || p.col > b.c2 {
                        out.insert(key, cell);
                        continue;
                    }
                    if p.row >= b.r1 && p.row <= b.r2 {
                        continue;
                    }
                    if p.row > b.r2 {
                        out.insert(addr::key(p.col, p.row - cnt), cell);
                    } else {
                        out.insert(key, cell);
                    }
                } else {
                    if p.row < b.r1 || p.row > b.r2 {
                        out.insert(key, cell);
                        continue;
                    }
                    if p.col >= b.c1 && p.col <= b.c2 {
                        continue;
                    }
                    if p.col > b.c2 {
                        out.insert(addr::key(p.col - cnt, p.row), cell);
                    } else {
                        out.insert(key, cell);
                    }
                }
            }
            e.sheet_mut().cells = out;
        })
    }

    /// `structuralEdit(axis, at, count)`: inserts (`count > 0`) or deletes rows/columns, rewriting the
    /// cells' keys and formulas, the column/row styles, the merges and the sizes. `at` is a row number or
    /// a column index.
    pub fn structural_edit(&mut self, axis: Axis, at: u32, count: i64) -> Result<(), Refused> {
        if count == 0 {
            return Ok(());
        }
        let at_i = at as i64;
        let shift = move |v: i64, is_start: bool| -> Option<i64> {
            if count >= 0 {
                return Some(if v >= at_i { v + count } else { v });
            }
            let del_end = at_i - count;
            if v < at_i {
                return Some(v);
            }
            if v >= del_end {
                return Some(v + count);
            }
            Some(if is_start { at_i } else { at_i - 1 })
        };
        let in_deleted = move |v: i64| count < 0 && v >= at_i && v < at_i - count;
        self.commit(|e| {
            let sheet = e.sheet_mut();
            let old = std::mem::take(&mut sheet.cells);
            let mut cells = CellMap::with_capacity(old.len());
            for (key, cell) in old {
                let Some(p) = addr::Cell::parse(&key) else { continue };
                let (mut c, mut r) = (p.col as i64, p.row as i64);
                let pos = if axis == Axis::Col { c } else { r };
                if in_deleted(pos) {
                    continue;
                }
                if pos >= at_i {
                    if axis == Axis::Col {
                        c += count;
                    } else {
                        r += count;
                    }
                }
                let mut nc = cell;
                if let Some(Json::String(f)) = &nc.f {
                    if !f.is_empty() {
                        nc.f = Some(Json::String(refs::shift_refs(f, if axis == Axis::Col { refs::Axis::Col } else { refs::Axis::Row }, at_i, count)));
                    }
                }
                if c >= 0 && r >= 1 {
                    cells.insert(addr::key(c as u32, r as u32), nc);
                }
            }
            sheet.cells = cells;
            if axis == Axis::Col {
                if let Some(Json::Object(cs)) = sheet.data.get("col_styles").cloned() {
                    let mut out = Map::new();
                    for (k, v) in cs {
                        let ci = addr::col_to_index(&k);
                        if in_deleted(ci) {
                            continue;
                        }
                        if let Some(n) = shift(ci, true) {
                            out.insert(index_to_col(n), v);
                        }
                    }
                    sheet.data.insert("col_styles".into(), Json::Object(out));
                }
            } else if let Some(Json::Object(rs)) = sheet.data.get("row_styles").cloned() {
                let mut out = Map::new();
                for (k, v) in rs {
                    let Ok(ri) = k.parse::<i64>() else { continue };
                    if in_deleted(ri) {
                        continue;
                    }
                    if let Some(n) = shift(ri, true) {
                        out.insert(n.to_string(), v);
                    }
                }
                sheet.data.insert("row_styles".into(), Json::Object(out));
            }
            let merges: Vec<Rect> = sheet
                .merges()
                .into_iter()
                .filter_map(|b| {
                    let (mut c1, mut c2, mut r1, mut r2) = (b.c1 as i64, b.c2 as i64, b.r1 as i64, b.r2 as i64);
                    if axis == Axis::Col {
                        let (a, z) = (shift(c1, true)?, shift(c2, false)?);
                        if a > z {
                            return None;
                        }
                        c1 = a;
                        c2 = z;
                    } else {
                        let (a, z) = (shift(r1, true)?, shift(r2, false)?);
                        if a > z {
                            return None;
                        }
                        r1 = a;
                        r2 = z;
                    }
                    if c1 == c2 && r1 == r2 {
                        return None;
                    }
                    Some(Rect::new(c1.max(0) as u32, r1.max(1) as u32, c2.max(0) as u32, r2.max(1) as u32))
                })
                .collect();
            sheet.set_merges(&merges);
            if axis == Axis::Row {
                let mut out = Map::new();
                for (r, h) in sheet.row_heights() {
                    if let Some(n) = shift(r as i64, true) {
                        out.insert(n.to_string(), json_number(h));
                    }
                }
                sheet.data.insert("row_heights".into(), Json::Object(out));
            } else {
                let mut out = Map::new();
                for (c, w) in sheet.col_widths() {
                    if let Some(n) = shift(c as i64, true) {
                        out.insert(index_to_col(n), json_number(w));
                    }
                }
                sheet.data.insert("col_widths".into(), Json::Object(out));
            }
        })
    }

    /// The ribbon's « Insérer des lignes / colonnes », « Supprimer … ».
    pub fn insert_or_delete(&mut self, axis: Axis, insert: bool) -> Result<(), Refused> {
        let b = self.sel.rect();
        let (at, n) = match axis {
            Axis::Row => (b.r1, b.height() as i64),
            Axis::Col => (b.c1, b.width() as i64),
        };
        self.structural_edit(axis, at, if insert { n } else { -n })
    }

    /// `moveColumns(lo, hi, before)` / `moveRows`: a block dragged by its headers lands before
    /// `before`; cells, styles, sizes and merges follow, formulas are not re-pointed (as on the web).
    pub fn move_block(&mut self, axis: Axis, lo: u32, hi: u32, before: u32) -> Result<(), Refused> {
        if before >= lo && before <= hi + 1 {
            return Ok(());
        }
        let w = hi - lo + 1;
        let mut map: HashMap<u32, u32> = HashMap::new();
        if before > hi + 1 {
            for x in lo..=hi {
                map.insert(x, before - w + (x - lo));
            }
            for x in hi + 1..before {
                map.insert(x, x - w);
            }
        } else {
            for x in lo..=hi {
                map.insert(x, before + (x - lo));
            }
            for x in before..lo {
                map.insert(x, x + w);
            }
        }
        let m = |x: u32| map.get(&x).copied().unwrap_or(x);
        self.commit(|e| {
            let sheet = e.sheet_mut();
            let old = std::mem::take(&mut sheet.cells);
            let mut cells = CellMap::with_capacity(old.len());
            for (key, cell) in old {
                match addr::Cell::parse(&key) {
                    Some(p) => {
                        let np = if axis == Axis::Col { Pos::new(m(p.col), p.row) } else { Pos::new(p.col, m(p.row)) };
                        cells.insert(np.key(), cell);
                    }
                    None => {
                        cells.insert(key, cell);
                    }
                }
            }
            sheet.cells = cells;
            let style_key = if axis == Axis::Col { "col_styles" } else { "row_styles" };
            if let Some(Json::Object(st)) = sheet.data.get(style_key).cloned() {
                let mut out = Map::new();
                for (k, v) in st {
                    let nk = if axis == Axis::Col { index_to_col(m(addr::col_to_index(&k).max(0) as u32) as i64) } else { k.parse::<u32>().map(|r| m(r).to_string()).unwrap_or(k) };
                    out.insert(nk, v);
                }
                sheet.data.insert(style_key.into(), Json::Object(out));
            }
            let merges: Vec<Rect> = sheet
                .merges()
                .into_iter()
                .map(|b| if axis == Axis::Col { Rect::new(m(b.c1), b.r1, m(b.c2), b.r2) } else { Rect::new(b.c1, m(b.r1), b.c2, m(b.r2)) })
                .collect();
            sheet.set_merges(&merges);
            if axis == Axis::Col {
                let mut out = Map::new();
                for (c, wv) in sheet.col_widths() {
                    out.insert(index_to_col(m(c) as i64), json_number(wv));
                }
                sheet.data.insert("col_widths".into(), Json::Object(out));
            } else {
                let mut out = Map::new();
                for (r, h) in sheet.row_heights() {
                    out.insert(m(r).to_string(), json_number(h));
                }
                sheet.data.insert("row_heights".into(), Json::Object(out));
            }
        })?;
        let nl = m(lo);
        if axis == Axis::Col {
            self.select_cols(nl, nl + w - 1);
        } else {
            self.select_rows(nl, nl + w - 1);
        }
        Ok(())
    }

    // ── Fill, sort, tools ───────────────────────────────────────────────────

    /// `performFill(src, to)`: the fill handle dragged from the selection `src` to cell `to`, on one axis
    /// (`fillBounds`); a series where the source has no formula (`fillSeries`), else the source tiled
    /// with its formulas translated. The filled box becomes the selection.
    pub fn fill(&mut self, src: Rect, to: Pos) -> Result<(), Refused> {
        let fb = fill_bounds(&src, to);
        if fb.axis == FillAxis::None {
            return Ok(());
        }
        let lang = self.lang.clone();
        let at = |e: &Editor, c: u32, r: u32| e.sheet().cells.get(&addr::key(c, r)).cloned();
        let tile = |cell: Option<Cell>, d_col: i64, d_row: i64| -> Option<Cell> {
            let mut cell = cell?;
            if let Some(f) = cell.formula().map(str::to_string) {
                cell.f = Some(Json::String(refs::translate_formula(&f, d_col, d_row)));
            }
            Some(cell)
        };
        let mut entries = Vec::new();
        if fb.axis == FillAxis::V {
            let down = to.row > src.r2;
            let targets: Vec<u32> = if down { (src.r2 + 1..=to.row).collect() } else { (to.row..src.r1).rev().collect() };
            let h = src.height() as i64;
            for c in src.c1..=src.c2 {
                let src_cells: Vec<Option<Cell>> = (0..src.height()).map(|i| at(self, c, src.r1 + i)).collect();
                let has_formula = src_cells.iter().any(|cd| cd.as_ref().is_some_and(|x| x.formula().is_some()));
                let series = if has_formula { None } else { fill_series_json(&src_cells, targets.len(), down, &lang) };
                let sty = (if down { at(self, c, src.r2) } else { at(self, c, src.r1) }).and_then(|x| x.s);
                for (i, r) in targets.iter().enumerate() {
                    match &series {
                        Some(s) => {
                            let v = s.get(i).cloned().unwrap_or(Json::Null);
                            entries.push((Pos::new(c, *r), Some(Cell { v: Some(v), s: sty.clone(), ..Cell::default() })));
                        }
                        None => {
                            let idx = ((*r as i64 - src.r1 as i64) % h + h) % h;
                            let sr = src.r1 + idx as u32;
                            entries.push((Pos::new(c, *r), tile(at(self, c, sr), 0, *r as i64 - sr as i64)));
                        }
                    }
                }
            }
        } else {
            let right = to.col > src.c2;
            let targets: Vec<u32> = if right { (src.c2 + 1..=to.col).collect() } else { (to.col..src.c1).rev().collect() };
            let w = src.width() as i64;
            for r in src.r1..=src.r2 {
                let src_cells: Vec<Option<Cell>> = (0..src.width()).map(|i| at(self, src.c1 + i, r)).collect();
                let has_formula = src_cells.iter().any(|cd| cd.as_ref().is_some_and(|x| x.formula().is_some()));
                let series = if has_formula { None } else { fill_series_json(&src_cells, targets.len(), right, &lang) };
                let sty = (if right { at(self, src.c2, r) } else { at(self, src.c1, r) }).and_then(|x| x.s);
                for (i, c) in targets.iter().enumerate() {
                    match &series {
                        Some(s) => {
                            let v = s.get(i).cloned().unwrap_or(Json::Null);
                            entries.push((Pos::new(*c, r), Some(Cell { v: Some(v), s: sty.clone(), ..Cell::default() })));
                        }
                        None => {
                            let idx = ((*c as i64 - src.c1 as i64) % w + w) % w;
                            let sc = src.c1 + idx as u32;
                            entries.push((Pos::new(*c, r), tile(at(self, sc, r), *c as i64 - sc as i64, 0)));
                        }
                    }
                }
            }
        }
        self.write_cells(entries)?;
        self.sel = Selection { active: Pos::new(fb.c1, fb.r1), end: Some(Pos::new(fb.c2, fb.r2)) };
        Ok(())
    }

    /// The sort key of a cell (`numericValue` else `resolveValue` lower-cased).
    fn sort_key(&self, cell: Option<&Cell>) -> (Option<f64>, String) {
        self.with_eval(|data, ctx, spill, _| {
            // No key: a blocked spill is not detected and an empty cell shows no spilled value (as on the web).
            let (num, text, _) = match cell {
                Some(c) => crate::grid::cell_value(Some(c), "", data, ctx, spill),
                None => (None, String::new(), None),
            };
            (num, text)
        })
    }

    /// `sortRange(asc)`: the selected rows by the active cell's column (numbers before text, text by
    /// lower-cased code units), reversed for descending.
    pub fn sort_range(&mut self, asc: bool) -> Result<(), Refused> {
        let b = self.sel.rect();
        if b.r2 <= b.r1 {
            return Ok(());
        }
        let bb = self.bounded_selection();
        let (r2, c2) = (bb.r2, bb.c2);
        if r2 <= b.r1 {
            return Ok(());
        }
        let key_idx = (self.sel.active.col.max(b.c1) - b.c1) as usize;
        let cells = &self.sheet().cells;
        let mut rows: Vec<Vec<Option<Cell>>> = (b.r1..=r2).map(|r| (b.c1..=c2).map(|c| cells.get(&addr::key(c, r)).cloned()).collect()).collect();
        let keys: Vec<(Option<f64>, String)> = rows.iter().map(|rc| self.sort_key(rc.get(key_idx).and_then(Option::as_ref))).collect();
        let mut idx: Vec<usize> = (0..rows.len()).collect();
        idx.sort_by(|&a, &b| {
            let (ka, kb) = (&keys[a], &keys[b]);
            match (ka.0, kb.0) {
                (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => js::cmp_str(&ka.1.to_lowercase(), &kb.1.to_lowercase()),
            }
        });
        if !asc {
            idx.reverse();
        }
        rows = idx.into_iter().map(|i| rows[i].clone()).collect();
        self.write_rows(b.c1, b.r1, c2, rows)
    }

    fn write_rows(&mut self, c1: u32, r1: u32, c2: u32, rows: Vec<Vec<Option<Cell>>>) -> Result<(), Refused> {
        self.commit(|e| {
            let cells = &mut e.sheet_mut().cells;
            for (i, row) in rows.into_iter().enumerate() {
                for c in c1..=c2 {
                    let k = addr::key(c, r1 + i as u32);
                    match row.get((c - c1) as usize).cloned().flatten() {
                        Some(cell) => {
                            cells.insert(k, cell);
                        }
                        None => {
                            cells.remove(&k);
                        }
                    }
                }
            }
        })
    }

    /// `sortRangeMulti(b, headers, levels)`: N levels, empties always last.
    pub fn sort_multi(&mut self, b: Rect, headers: bool, levels: &[SortLevel]) -> Result<(), Refused> {
        let (uc, ur) = self.sheet().used_extent();
        let c2 = b.c2.min(uc.max(b.c1));
        let r2 = b.r2.min(ur.max(b.r1));
        let start = if headers { b.r1 + 1 } else { b.r1 };
        if r2 <= start || levels.is_empty() {
            return Ok(());
        }
        let cells = &self.sheet().cells;
        let rows: Vec<Vec<Option<Cell>>> = (start..=r2).map(|r| (b.c1..=c2).map(|c| cells.get(&addr::key(c, r)).cloned()).collect()).collect();
        let keys: Vec<Vec<(Option<f64>, String, bool)>> = rows
            .iter()
            .map(|rc| {
                rc.iter()
                    .map(|cell| {
                        let (n, s) = self.sort_key(cell.as_ref());
                        let empty = n.is_none() && s.is_empty();
                        (n, s.to_lowercase(), empty)
                    })
                    .collect()
            })
            .collect();
        let mut idx: Vec<usize> = (0..rows.len()).collect();
        idx.sort_by(|&a, &bi| {
            for lv in levels {
                if lv.col < b.c1 || lv.col > c2 {
                    continue;
                }
                let ci = (lv.col - b.c1) as usize;
                let (ka, kb) = (&keys[a][ci], &keys[bi][ci]);
                if ka.2 && kb.2 {
                    continue;
                }
                if ka.2 {
                    return std::cmp::Ordering::Greater;
                }
                if kb.2 {
                    return std::cmp::Ordering::Less;
                }
                let d = match (ka.0, kb.0) {
                    (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => js::cmp_str(&ka.1, &kb.1),
                };
                if d != std::cmp::Ordering::Equal {
                    return if lv.asc { d } else { d.reverse() };
                }
            }
            std::cmp::Ordering::Equal
        });
        let sorted = idx.into_iter().map(|i| rows[i].clone()).collect();
        self.write_rows(b.c1, start, c2, sorted)
    }

    /// `removeDuplicates`: the selection's rows deduplicated by their displayed texts (the first kept,
    /// the rest moved up). Returns how many rows went.
    pub fn remove_duplicates(&mut self) -> Result<usize, Refused> {
        let b = self.sel.rect();
        let (_, ur) = self.sheet().used_extent();
        let r2 = b.r2.min(ur.max(b.r1));
        let row_key = |e: &Editor, r: u32| -> String { (b.c1..=b.c2).map(|c| e.cell_text(c, r)).collect::<Vec<_>>().join("\u{1}") };
        let mut seen = HashSet::new();
        let mut keep = Vec::new();
        for r in b.r1..=r2 {
            if seen.insert(row_key(self, r)) {
                keep.push(r);
            }
        }
        let removed = (r2 - b.r1 + 1) as usize - keep.len();
        if removed == 0 {
            return Ok(0);
        }
        let old = self.sheet().cells.clone();
        self.commit(|e| {
            let cells = &mut e.sheet_mut().cells;
            for r in b.r1..=r2 {
                for c in b.c1..=b.c2 {
                    cells.remove(&addr::key(c, r));
                }
            }
            for (i, src) in keep.iter().enumerate() {
                for c in b.c1..=b.c2 {
                    if let Some(cell) = old.get(&addr::key(c, *src)) {
                        cells.insert(addr::key(c, b.r1 + i as u32), cell.clone());
                    }
                }
            }
        })?;
        Ok(removed)
    }

    /// `autoSum`: `=SUM(…)` over the numeric run above, else to the left; `None` when there is none (the
    /// caller then starts editing `=SUM(`).
    pub fn auto_sum(&mut self) -> Result<Option<()>, Refused> {
        let a = self.sel.active;
        let num_at = |e: &Editor, c: u32, r: u32| e.shown(c, r).num.is_some();
        let mut top = a.row as i64 - 1;
        while top >= 1 && num_at(self, a.col, top as u32) {
            top -= 1;
        }
        let col = index_to_col(a.col as i64);
        if top < a.row as i64 - 1 {
            self.update_cell(a, &format!("=SUM({col}{}:{col}{})", top + 1, a.row - 1))?;
            return Ok(Some(()));
        }
        let mut left = a.col as i64 - 1;
        while left >= 0 && num_at(self, left as u32, a.row) {
            left -= 1;
        }
        if left < a.col as i64 - 1 {
            self.update_cell(a, &format!("=SUM({}{}:{}{})", index_to_col(left + 1), a.row, index_to_col(a.col as i64 - 1), a.row))?;
            return Ok(Some(()));
        }
        Ok(None)
    }

    /// `frMatches`: the cells whose displayed text contains `q`, row by row.
    pub fn find_matches(&self, q: &str, match_case: bool) -> Vec<Pos> {
        if q.is_empty() {
            return Vec::new();
        }
        let q = if match_case { q.to_string() } else { q.to_lowercase() };
        let mut out: Vec<Pos> = self
            .sheet()
            .cells
            .keys()
            .filter_map(|k| addr::Cell::parse(k))
            .filter(|p| {
                let t = self.cell_text(p.col, p.row);
                (if match_case { t } else { t.to_lowercase() }).contains(&q)
            })
            .collect();
        out.sort_by(|a, b| a.row.cmp(&b.row).then(a.col.cmp(&b.col)));
        out
    }

    /// `frFindNext(back)`: selects the next (or previous) match; `false` when there is none.
    pub fn find_next(&mut self, q: &str, match_case: bool, back: bool) -> bool {
        let ms = self.find_matches(q, match_case);
        if ms.is_empty() {
            return false;
        }
        let cur = self.sel.active;
        let after = |m: &Pos| m.row > cur.row || (m.row == cur.row && m.col > cur.col);
        let before = |m: &Pos| m.row < cur.row || (m.row == cur.row && m.col < cur.col);
        let t = if back { ms.iter().rev().find(|m| before(m)).copied().unwrap_or(ms[ms.len() - 1]) } else { ms.iter().find(|m| after(m)).copied().unwrap_or(ms[0]) };
        self.sel = Selection::cell(t);
        true
    }

    /// `frReplaceAll`: in formulas and raw values; returns the count.
    pub fn replace_all(&mut self, find: &str, replace: &str, match_case: bool) -> Result<usize, Refused> {
        if find.is_empty() {
            return Ok(0);
        }
        let mut changes = Vec::new();
        for (k, cell) in &self.sheet().cells {
            let (is_f, val) = match &cell.f {
                Some(Json::String(f)) => (true, f.clone()),
                Some(_) => (true, String::new()),
                None => (false, if cell.v_is_nullish() { String::new() } else { cell.display() }),
            };
            if !(if match_case { val.contains(find) } else { val.to_lowercase().contains(&find.to_lowercase()) }) {
                continue;
            }
            let nv = js_replace(&val, find, replace, match_case, true);
            if nv != val {
                changes.push((k.clone(), is_f, nv));
            }
        }
        let n = changes.len();
        if n > 0 {
            self.commit(|e| {
                let cells = &mut e.sheet_mut().cells;
                for (k, is_f, nv) in changes {
                    if let Some(c) = cells.get_mut(&k) {
                        if is_f {
                            c.f = Some(Json::String(nv));
                        } else {
                            c.v = Some(Json::String(nv));
                        }
                    }
                }
            })?;
        }
        Ok(n)
    }

    /// `frReplaceOne`: the active cell's first match replaced, then the next match selected.
    pub fn replace_one(&mut self, find: &str, replace: &str, match_case: bool) -> Result<(), Refused> {
        if find.is_empty() {
            self.find_next(find, match_case, false);
            return Ok(());
        }
        let key = self.sel.active.key();
        if let Some(cell) = self.sheet().cells.get(&key).cloned() {
            let is_f = cell.has_f();
            let val = match &cell.f {
                Some(Json::String(f)) if is_f => f.clone(),
                _ => {
                    if cell.v_is_nullish() {
                        String::new()
                    } else {
                        cell.display()
                    }
                }
            };
            let hit = if match_case { val.contains(find) } else { val.to_lowercase().contains(&find.to_lowercase()) };
            if hit {
                let nv = js_replace(&val, find, replace, match_case, false);
                self.commit(|e| {
                    if let Some(c) = e.sheet_mut().cells.get_mut(&key) {
                        if is_f {
                            c.f = Some(Json::String(nv));
                        } else {
                            c.v = Some(Json::String(nv));
                        }
                    }
                })?;
            }
        }
        self.find_next(find, match_case, false);
        Ok(())
    }

    // ── History ─────────────────────────────────────────────────────────────

    pub fn undo(&mut self) -> bool {
        let i = self.active_index();
        let mut h = std::mem::take(&mut self.history);
        let done = h.undo(&mut self.wb.sheets[i]);
        self.history = h;
        if done {
            self.touch();
        }
        done
    }

    pub fn redo(&mut self) -> bool {
        let i = self.active_index();
        let mut h = std::mem::take(&mut self.history);
        let done = h.redo(&mut self.wb.sheets[i]);
        self.history = h;
        if done {
            self.touch();
        }
        done
    }
}

/// The sheet's validation blocks (`validations`), the unreadable ones skipped.
pub fn validation_blocks(v: Option<&Json>) -> Vec<DVBlock> {
    v.and_then(Json::as_array).map(|a| a.iter().filter_map(|b| serde_json::from_value(b.clone()).ok()).collect()).unwrap_or_default()
}

/// `fillSeries(srcCells.map(cd => cd?.v ?? ''), count, forward, lang)` over stored values.
fn fill_series_json(src: &[Option<Cell>], count: usize, forward: bool, lang: &str) -> Option<Vec<Json>> {
    let vals: Vec<Fill> = src
        .iter()
        .map(|c| match c.as_ref().and_then(|c| c.v.clone()) {
            Some(Json::Number(n)) => Fill::Num(n.as_f64().unwrap_or(f64::NAN)),
            Some(Json::String(s)) => Fill::Str(s),
            Some(Json::Null) | None => Fill::Str(String::new()),
            Some(other) => Fill::Str(crate::model::cell::js_string_of(&other)),
        })
        .collect();
    let out = fill::fill_series(&vals, count as i64, forward, lang)?;
    Some(
        out.into_iter()
            .map(|f| match f {
                Fill::Num(n) => json_number(n),
                Fill::Str(s) => Json::String(s),
            })
            .collect(),
    )
}

/// The axis of a fill (`fillBounds`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillAxis {
    V,
    H,
    None,
}

/// `fillBounds(src, to)`: the source extended to `to` along the axis that leaves it furthest
/// (vertical on a tie).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FillBox {
    pub c1: u32,
    pub c2: u32,
    pub r1: u32,
    pub r2: u32,
    pub axis: FillAxis,
}

pub fn fill_bounds(fs: &Rect, to: Pos) -> FillBox {
    let tc = to.col as i64;
    let tr = to.row as i64;
    let (c1, c2, r1, r2) = (fs.c1 as i64, fs.c2 as i64, fs.r1 as i64, fs.r2 as i64);
    let dr = if tr > r2 { tr - r2 } else if tr < r1 { r1 - tr } else { 0 };
    let dc = if tc > c2 { tc - c2 } else if tc < c1 { c1 - tc } else { 0 };
    if dr == 0 && dc == 0 {
        return FillBox { c1: fs.c1, c2: fs.c2, r1: fs.r1, r2: fs.r2, axis: FillAxis::None };
    }
    if dr >= dc {
        FillBox { c1: fs.c1, c2: fs.c2, r1: r1.min(tr) as u32, r2: r2.max(tr) as u32, axis: FillAxis::V }
    } else {
        FillBox { c1: c1.min(tc) as u32, c2: c2.max(tc) as u32, r1: fs.r1, r2: fs.r2, axis: FillAxis::H }
    }
}

/// `val.replace(new RegExp(escaped(find), flags), replacement)` with JavaScript's `$` patterns in the
/// replacement (`$$`, `$&`, `` $` ``, `$'`).
pub fn js_replace(val: &str, find: &str, replacement: &str, match_case: bool, global: bool) -> String {
    let hay: Vec<char> = val.chars().collect();
    let needle: Vec<char> = find.chars().collect();
    let eq = |a: char, b: char| if match_case { a == b } else { a.to_lowercase().eq(b.to_lowercase()) };
    let mut out = String::new();
    let mut i = 0;
    let mut done = false;
    while i < hay.len() {
        if !done && i + needle.len() <= hay.len() && !needle.is_empty() && (0..needle.len()).all(|k| eq(hay[i + k], needle[k])) {
            let m: String = hay[i..i + needle.len()].iter().collect();
            let before: String = hay[..i].iter().collect();
            let after: String = hay[i + needle.len()..].iter().collect();
            let r: Vec<char> = replacement.chars().collect();
            let mut j = 0;
            while j < r.len() {
                if r[j] == '$' && j + 1 < r.len() {
                    match r[j + 1] {
                        '$' => {
                            out.push('$');
                            j += 2;
                            continue;
                        }
                        '&' => {
                            out.push_str(&m);
                            j += 2;
                            continue;
                        }
                        '`' => {
                            out.push_str(&before);
                            j += 2;
                            continue;
                        }
                        '\'' => {
                            out.push_str(&after);
                            j += 2;
                            continue;
                        }
                        _ => {}
                    }
                }
                out.push(r[j]);
                j += 1;
            }
            i += needle.len();
            if !global {
                done = true;
            }
            continue;
        }
        out.push(hay[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::tests::editor;
    use super::*;

    #[test]
    fn typed_values_are_stored_like_the_web() {
        let mut e = editor(json!({}));
        e.update_cell(Pos::new(0, 1), "12").unwrap();
        e.update_cell(Pos::new(0, 2), "1,5").unwrap();
        e.update_cell(Pos::new(0, 3), "=sum(a1:a2").unwrap();
        e.update_cell(Pos::new(0, 4), "").unwrap();
        let c = &e.sheet().cells;
        assert_eq!(c["A1"].v, Some(json!(12)));
        assert_eq!(c["A2"].v, Some(json!("1,5")));
        assert_eq!(c["A3"].f, Some(json!("=SUM(A1:A2)")));
        assert_eq!(c["A3"].v, None);
        assert_eq!(c["A4"].v, Some(Json::Null));
        assert!(e.undo());
        assert!(!e.sheet().cells.contains_key("A4"));
        assert!(e.redo());
        assert!(e.sheet().cells.contains_key("A4"));
    }

    #[test]
    fn styles_borders_and_merges() {
        let mut e = editor(json!({"A1": {"v": 1}}));
        e.select_rect(Pos::new(0, 1), Pos::new(1, 2));
        e.toggle_style("bold").unwrap();
        assert!(e.style_at(1, 2).is_bold());
        e.apply_borders(Borders::Outer, "#5f6368").unwrap();
        let s = e.style_at(0, 1);
        assert_eq!((s.bt.is_some(), s.bl.is_some(), s.bb.is_some(), s.br.is_some()), (true, true, false, false));
        e.toggle_merge().unwrap();
        assert_eq!(e.sheet().merges(), vec![Rect::new(0, 1, 1, 2)]);
        e.toggle_merge().unwrap();
        assert!(e.sheet().merges().is_empty());
        e.select_cols(2, 2);
        e.toggle_style("italic").unwrap();
        assert!(e.style_at(2, 500).is_italic());
    }

    #[test]
    fn copy_paste_translates_formulas() {
        let mut e = editor(json!({"A1": {"v": 1}, "B1": {"f": "=A1*2"}}));
        e.select(Pos::new(1, 1));
        e.copy_selection(false);
        e.select(Pos::new(1, 3));
        e.paste().unwrap();
        assert_eq!(e.sheet().cells["B3"].f, Some(json!("=A3*2")));
        assert_eq!(e.sel.rect(), Rect::new(1, 3, 1, 3));
    }

    #[test]
    fn structural_edits_shift_cells_formulas_and_merges() {
        let mut e = editor(json!({"A1": {"v": 1}, "A2": {"v": 2}, "A3": {"f": "=A1+A2"}}));
        e.sheet_mut().set_merges(&[Rect::new(1, 2, 2, 3)]);
        e.structural_edit(Axis::Row, 2, 1).unwrap();
        assert_eq!(e.sheet().cells["A4"].f, Some(json!("=A1+A3")));
        assert_eq!(e.sheet().merges(), vec![Rect::new(1, 3, 2, 4)]);
        e.structural_edit(Axis::Row, 1, -1).unwrap();
        assert_eq!(e.sheet().cells["A3"].f, Some(json!("=#REF!+A2")));
    }

    #[test]
    fn sorting_and_fill() {
        let mut e = editor(json!({"A1": {"v": "b"}, "A2": {"v": 3}, "A3": {"v": "a"}, "A4": {"v": 1}}));
        e.select_rect(Pos::new(0, 1), Pos::new(0, 4));
        e.sort_range(true).unwrap();
        let v: Vec<Json> = (1..=4).map(|r| e.sheet().cells[&format!("A{r}")].v.clone().unwrap_or(Json::Null)).collect();
        assert_eq!(v, vec![json!(1), json!(3), json!("a"), json!("b")]);
        let mut e = editor(json!({"A1": {"v": 1}, "A2": {"v": 2}}));
        e.fill(Rect::new(0, 1, 0, 2), Pos::new(0, 4)).unwrap();
        assert_eq!(e.sheet().cells["A4"].v, Some(json!(4)));
        assert_eq!(e.sel.rect(), Rect::new(0, 1, 0, 4));
    }

    #[test]
    fn replace_follows_javascript() {
        assert_eq!(js_replace("aAa", "a", "x", false, true), "xxx");
        assert_eq!(js_replace("aAa", "a", "[$&]", true, false), "[a]Aa");
        let mut e = editor(json!({"A1": {"v": "chat"}, "B2": {"f": "=\"chat\""}}));
        assert_eq!(e.replace_all("chat", "chien", false).unwrap(), 2);
        assert_eq!(e.sheet().cells["B2"].f, Some(json!("=\"chien\"")));
    }
}
