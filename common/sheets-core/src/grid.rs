//! The grid's geometry and what each cell shows (`SpreadsheetApp.tsx`): column lefts and row tops at a
//! zoom (`geom`, `SA:2660-2691`), hit testing (`colAtX`/`rowAtY`), the scroll extent (`SA:2609-2610`), a
//! cell's displayed text (`numericValue`/`resolveValue`/`drawCellContent`'s choice, `SA:293-375`,
//! `SA:4852-4858`), the merges, and the status bar's aggregates (`selectionAggregate`, `SA:331-346`).

use std::collections::{HashMap, HashSet};

use crate::addr::{self, Rect, MAX_COLS, MAX_ROWS};
use crate::format;
use crate::formula::eval::{evaluate, Ctx, Data, SpillIndex};
use crate::formula::value::{format_scalar, format_value, Scalar, Value};
use crate::js;
use crate::model::cell::Cell;
use crate::model::sheet::Sheet;
use crate::model::style::Style;

/// The row-number column's width and the letters row's height (they do not zoom).
pub const ROW_HEADER_WIDTH: f64 = 46.0;
pub const COL_HEADER_HEIGHT: f64 = 24.0;

/// Rows a cell's font makes taller (`autoRowHeight`): rows without an explicit height whose tallest cell
/// font (`round(fontSize × 1.2)`) exceeds the default.
pub fn auto_row_heights(sheet: &Sheet) -> HashMap<u32, f64> {
    let default = sheet.default_row_height();
    let explicit: HashSet<u32> = sheet.row_heights().into_iter().map(|(r, _)| r).collect();
    let mut m: HashMap<u32, f64> = HashMap::new();
    for (k, c) in &sheet.cells {
        let Some(pos) = addr::Cell::parse(k) else { continue };
        if explicit.contains(&pos.row) {
            continue;
        }
        let Some(fs) = c.style().font_size.filter(|f| *f != 0.0) else { continue };
        let h = js::round(fs * 1.2);
        if h > default && h > m.get(&pos.row).copied().unwrap_or(0.0) {
            m.insert(pos.row, h);
        }
    }
    m
}

/// Rows and columns hidden by collapsed outline groups (the group's first row/column stays).
pub fn group_hidden(sheet: &Sheet) -> (HashSet<u32>, HashSet<u32>) {
    let mut rows = HashSet::new();
    for g in sheet.row_groups() {
        if g.collapsed {
            for r in g.start + 1..=g.end {
                rows.insert(r);
            }
        }
    }
    let mut cols = HashSet::new();
    for g in sheet.col_groups() {
        if g.collapsed {
            for c in g.start + 1..=g.end {
                cols.insert(c);
            }
        }
    }
    (rows, cols)
}

/// Pixel geometry of a sheet at a zoom (content coordinates: the headers included, before scrolling).
#[derive(Clone, Debug)]
pub struct Geometry {
    pub zoom: f64,
    col_left: Vec<f64>,
    row_idx: Vec<u32>,
    row_prefix: Vec<f64>,
    zrow_h: f64,
}

impl Geometry {
    /// `geom` for `sheet` at `zoom`, with `hidden_rows` (filters, collapsed groups) and `hidden_cols`.
    pub fn new(sheet: &Sheet, zoom: f64, hidden_rows: &HashSet<u32>, hidden_cols: &HashSet<u32>) -> Geometry {
        let col_w = sheet.default_col_width();
        let overrides: HashMap<u32, f64> = sheet.col_widths().into_iter().collect();
        let mut col_left = Vec::with_capacity(MAX_COLS as usize + 1);
        col_left.push(ROW_HEADER_WIDTH);
        let mut x = ROW_HEADER_WIDTH;
        for c in 0..MAX_COLS {
            let w = if hidden_cols.contains(&c) { 0.0 } else { overrides.get(&c).copied().unwrap_or(col_w) * zoom };
            x += w;
            col_left.push(x);
        }
        let row_h = sheet.default_row_height();
        let auto = auto_row_heights(sheet);
        let explicit: HashMap<u32, f64> = sheet.row_heights().into_iter().collect();
        let mut set: HashSet<u32> = HashSet::new();
        set.extend(explicit.keys().copied().filter(|r| (1..=MAX_ROWS).contains(r)));
        set.extend(auto.keys().copied().filter(|r| (1..=MAX_ROWS).contains(r)));
        set.extend(hidden_rows.iter().copied());
        let mut row_idx: Vec<u32> = set.into_iter().collect();
        row_idx.sort_unstable();
        let zrow_h = row_h * zoom;
        let mut row_prefix = Vec::with_capacity(row_idx.len() + 1);
        row_prefix.push(0.0);
        let mut acc = 0.0;
        for r in &row_idx {
            let size = if hidden_rows.contains(r) { 0.0 } else { explicit.get(r).or_else(|| auto.get(r)).copied().unwrap_or(row_h) * zoom };
            acc += size - zrow_h;
            row_prefix.push(acc);
        }
        Geometry { zoom, col_left, row_idx, row_prefix, zrow_h }
    }

    /// `colLeft[c]` (0 ≤ c ≤ 16 384).
    pub fn col_left(&self, c: u32) -> f64 {
        self.col_left[(c as usize).min(self.col_left.len() - 1)]
    }

    /// The width of column `c`.
    pub fn col_width(&self, c: u32) -> f64 {
        self.col_left(c + 1) - self.col_left(c)
    }

    /// `rowYof(r)` (1 ≤ r ≤ 1 048 577).
    pub fn row_top(&self, r: u32) -> f64 {
        let below = self.row_idx.partition_point(|&x| x < r);
        COL_HEADER_HEIGHT + (r as f64 - 1.0) * self.zrow_h + self.row_prefix[below]
    }

    /// The height of row `r`.
    pub fn row_height(&self, r: u32) -> f64 {
        self.row_top(r + 1) - self.row_top(r)
    }

    pub fn total_width(&self) -> f64 {
        self.col_left(MAX_COLS)
    }

    pub fn total_height(&self) -> f64 {
        self.row_top(MAX_ROWS + 1)
    }

    /// `colAtX`: the column whose span holds content-x, `None` outside the body.
    pub fn col_at_x(&self, x: f64) -> Option<u32> {
        if x < self.col_left[0] || x >= self.col_left[MAX_COLS as usize] {
            return None;
        }
        let (mut lo, mut hi) = (0usize, MAX_COLS as usize);
        while lo < hi {
            let m = (lo + hi) / 2;
            if self.col_left[m + 1] <= x {
                lo = m + 1;
            } else {
                hi = m;
            }
        }
        Some(lo as u32)
    }

    /// `rowAtY`: the row whose span holds content-y, `None` outside the body.
    pub fn row_at_y(&self, y: f64) -> Option<u32> {
        if y < self.row_top(1) || y >= self.row_top(MAX_ROWS + 1) {
            return None;
        }
        let (mut lo, mut hi) = (1u32, MAX_ROWS + 1);
        while lo < hi {
            let m = (lo + hi) / 2;
            if self.row_top(m + 1) <= y {
                lo = m + 1;
            } else {
                hi = m;
            }
        }
        Some(lo)
    }

    /// The content rectangle (left, top, right, bottom) of a range.
    pub fn rect_of(&self, r: &Rect) -> (f64, f64, f64, f64) {
        (self.col_left(r.c1), self.row_top(r.r1), self.col_left(r.c2 + 1), self.row_top(r.r2 + 1))
    }
}

/// The scroll extent (`extentRows`/`extentCols`): the used range, the active cell and what is in view,
/// plus a margin, never less than 60 rows × 30 columns.
pub fn extent(used: (u32, u32), active: (u32, u32), view_end: (u32, u32)) -> (u32, u32) {
    let (used_col, used_row) = used;
    let rows = (used_row.max(1) + 40).max(active.1 + 40).max(view_end.1 + 40).clamp(60, MAX_ROWS);
    let cols = (used_col + 6).max(active.0 + 6).max(view_end.0 + 6).clamp(30, MAX_COLS);
    (cols, rows)
}

/// Merged ranges: anchors (top-left key → range) and covered cells (key → anchor key).
#[derive(Clone, Debug, Default)]
pub struct MergeInfo {
    pub anchors: HashMap<String, Rect>,
    pub covered: HashMap<String, String>,
    pub ranges: Vec<Rect>,
}

impl MergeInfo {
    /// `mergeInfo` (`SA:952-968`): huge merges keep their anchor but their covered cells are not listed.
    pub fn new(merges: &[Rect]) -> MergeInfo {
        let mut m = MergeInfo::default();
        for b in merges {
            let a = addr::key(b.c1, b.r1);
            m.anchors.insert(a.clone(), *b);
            m.ranges.push(*b);
            if b.area() > 100_000 {
                continue;
            }
            for c in b.cells() {
                let k = c.key();
                if k != a {
                    m.covered.insert(k, a.clone());
                }
            }
        }
        m
    }

    /// The merge a cell belongs to.
    pub fn merge_of(&self, c: addr::Cell) -> Option<Rect> {
        self.ranges.iter().copied().find(|r| r.contains(c))
    }
}

/// `asNumber`: a number, or a non-empty numeric string.
fn as_number(s: &Scalar) -> Option<f64> {
    match s {
        Scalar::Num(n) => Some(*n),
        Scalar::Str(t) if !t.is_empty() => js::number(t),
        _ => None,
    }
}

/// What a cell shows.
#[derive(Clone, Debug, PartialEq)]
pub struct CellShown {
    /// `numericValue`: the number the cell holds (formula result or numeric value), if any.
    pub num: Option<f64>,
    /// The text drawn (number format applied).
    pub text: String,
    /// The text without the number format (`resolveValue`).
    pub raw_text: String,
    /// The value is an error (for the status bar's explanation).
    pub error: Option<String>,
}

/// The value of a cell as the grid evaluates it: `(numericValue, resolveValue, error)`.
pub fn cell_value(cell: Option<&Cell>, key: &str, data: Data<'_>, ctx: &Ctx<'_>, spill: Option<&SpillIndex>) -> (Option<f64>, String, Option<String>) {
    let Some(cell) = cell else {
        if let Some(sv) = spill.and_then(|s| s.values.get(key)) {
            return (as_number(sv), format_scalar(sv), sv.err().map(|e| e.as_str().to_string()));
        }
        return (None, String::new(), None);
    };
    if let Some(f) = cell.formula() {
        if spill.and_then(|s| s.anchors.get(key)).is_some_and(|a| a.blocked) {
            return (None, "#SPILL!".into(), Some("#SPILL!".into()));
        }
        let v = evaluate(f, data, ctx, spill);
        let top = match &v {
            Value::M(m) => m.first().and_then(|r| r.first()).cloned(),
            Value::S(s) => Some(s.clone()),
            Value::L(_) => None,
        };
        let num = match &top {
            Some(Scalar::Num(n)) => Some(*n),
            _ => None,
        };
        let err = top.as_ref().and_then(|s| s.err()).map(|e| e.as_str().to_string());
        return (num, format_value(&v), err);
    }
    let num = match &cell.v {
        Some(serde_json::Value::Number(n)) => n.as_f64(),
        Some(serde_json::Value::String(s)) if !s.is_empty() => js::number(s),
        _ => None,
    };
    (num, cell.display(), None)
}

/// The text a cell draws (`drawCellContent`): a raw number-format code, else the ribbon's formats,
/// else the value.
#[allow(clippy::too_many_arguments)]
pub fn shown(cell: Option<&Cell>, key: &str, style: &Style, data: Data<'_>, ctx: &Ctx<'_>, spill: Option<&SpillIndex>, lang: &str) -> CellShown {
    let (num, raw_text, error) = cell_value(cell, key, data, ctx, spill);
    let text = match (num, style.num_fmt_code.as_deref()) {
        (Some(n), Some(code)) if !code.is_empty() => format::format_code(n, code),
        (Some(n), _) if style.num_fmt.as_deref().is_some_and(|f| !f.is_empty()) || style.decimals.is_some() || style.thousands == Some(true) => {
            format::format_number(n, style.num_fmt.as_deref(), style.decimals, style.thousands, lang)
        }
        _ => raw_text.clone(),
    };
    CellShown { num, text, raw_text, error }
}

/// The text of a cell without its raw number-format code (`cellText`, the filter lists and find).
pub fn cell_text(cell: Option<&Cell>, key: &str, data: Data<'_>, ctx: &Ctx<'_>, spill: Option<&SpillIndex>, lang: &str) -> String {
    let (num, raw, _) = cell_value(cell, key, data, ctx, spill);
    let Some(cell) = cell else { return raw };
    let s = cell.style();
    match num {
        Some(n) if s.num_fmt.as_deref().is_some_and(|f| !f.is_empty()) || s.decimals.is_some() || s.thousands == Some(true) => format::format_number(n, s.num_fmt.as_deref(), s.decimals, s.thousands, lang),
        _ => raw,
    }
}

/// The status bar's aggregate of a selection.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Aggregate {
    pub count: usize,
    pub num: usize,
    pub sum: f64,
    pub avg: f64,
    pub min: f64,
    pub max: f64,
}

/// `selectionAggregate`: over the populated cells (and spilled values) inside the rectangle.
pub fn selection_aggregate(sheet: &Sheet, rect: &Rect, data: Data<'_>, ctx: &Ctx<'_>, spill: Option<&SpillIndex>) -> Aggregate {
    let mut a = Aggregate::default();
    let (mut min, mut max) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut add = |key: &str, cell: Option<&Cell>| {
        let Some(p) = addr::Cell::parse(key) else { return };
        if !rect.contains(p) {
            return;
        }
        let spilled = cell.is_none() && spill.is_some_and(|s| s.values.contains_key(key));
        if !spilled && cell.is_none_or(|c| c.v_is_nullish() && !c.has_f()) {
            return;
        }
        a.count += 1;
        let n = if cell.is_none() {
            spill.and_then(|s| s.values.get(key)).and_then(as_number)
        } else {
            cell_value(cell, key, data, ctx, spill).0.filter(|_| !cell.and_then(Cell::formula).is_some_and(|_| spill.and_then(|s| s.anchors.get(key)).is_some_and(|x| x.blocked)))
        };
        if let Some(n) = n.filter(|n| !n.is_nan()) {
            a.num += 1;
            a.sum += n;
            if n < min {
                min = n;
            }
            if n > max {
                max = n;
            }
        }
    };
    for (k, c) in &sheet.cells {
        add(k, Some(c));
    }
    if let Some(s) = spill {
        for k in s.values.keys() {
            if !sheet.cells.contains_key(k) {
                add(k, None);
            }
        }
    }
    if a.num > 0 {
        a.avg = a.sum / a.num as f64;
        a.min = min;
        a.max = max;
    }
    a
}

/// `formatValue` of a scalar for the filter lists.
pub fn scalar_text(s: &Scalar) -> String {
    format_scalar(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formula::testing::env;
    use crate::model::sheet::SheetMeta;
    use serde_json::json;

    fn sheet(d: serde_json::Value) -> Sheet {
        Sheet::from_data(SheetMeta { id: "s".into(), name: "S".into(), position: 0, raw: Default::default() }, &d)
    }

    #[test]
    fn geometry_follows_the_web() {
        let s = sheet(json!({"cells": {"A3": {"v": "T", "s": {"fontSize": 40}}}, "col_widths": {"B": 0, "C": 150}, "row_heights": {"2": 50}}));
        let g = Geometry::new(&s, 1.0, &HashSet::new(), &HashSet::new());
        assert_eq!(g.col_left(0), 46.0);
        assert_eq!(g.col_left(1), 146.0);
        assert_eq!(g.col_width(1), 0.0);
        assert_eq!(g.col_width(2), 150.0);
        assert_eq!(g.row_top(1), 24.0);
        assert_eq!(g.row_top(2), 48.0);
        assert_eq!(g.row_top(3), 98.0);
        assert_eq!(g.row_height(3), 48.0); // round(40 × 1.2)
        assert_eq!(g.row_at_y(60.0), Some(2));
        assert_eq!(g.col_at_x(10.0), None);
        assert_eq!(g.col_at_x(146.0), Some(2)); // B is hidden
        let mut hidden = HashSet::new();
        hidden.insert(1);
        let g2 = Geometry::new(&s, 2.0, &hidden, &HashSet::new());
        assert_eq!(g2.row_top(2), 24.0);
        assert_eq!(g2.col_left(1), 246.0);
    }

    #[test]
    fn shown_text_and_aggregates() {
        let s = sheet(json!({"cells": {
            "A1": {"v": 1234.5, "s": {"numFmt": "currency"}},
            "A2": {"f": "=A1*2", "s": {"numFmtCode": "0.0"}},
            "A3": {"v": "x"},
            "A4": {"f": "=1/0"}
        }}));
        let names = HashMap::new();
        let e = env();
        let ctx = Ctx::new(&e);
        let data = Data { cells: &s.cells, sheets: None, names: &names };
        let a1 = shown(s.cells.get("A1"), "A1", &s.style_at(0, 1), data, &ctx, None, "fr");
        assert_eq!(a1.text, "1\u{202F}234,50\u{A0}€");
        let a2 = shown(s.cells.get("A2"), "A2", &s.style_at(0, 2), data, &ctx, None, "fr");
        assert_eq!(a2.text, "2469.0");
        let a4 = shown(s.cells.get("A4"), "A4", &Style::default(), data, &ctx, None, "fr");
        assert_eq!((a4.text.as_str(), a4.error.as_deref()), ("#DIV/0!", Some("#DIV/0!")));
        let agg = selection_aggregate(&s, &Rect::new(0, 1, 0, 4), data, &ctx, None);
        assert_eq!((agg.count, agg.num, agg.sum), (4, 2, 3703.5));
    }

    #[test]
    fn the_extent_grows_with_use() {
        assert_eq!(extent((0, 1), (0, 1), (0, 0)), (30, 60));
        assert_eq!(extent((40, 100), (2, 3), (10, 50)), (46, 140));
    }
}
