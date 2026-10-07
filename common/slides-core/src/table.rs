//! Table elements — a port of the web's `presentationTable.ts`: drawing, the cell under a point, and the
//! structural edits (rows, columns, cells, styles).

use serde_json::{json, Map, Value};

use crate::model::{as_f64, num, Element};
use crate::render::{font_of, plain_style};
use crate::richtext::Measure;
use crate::surface::{Baseline, Ctx, Paint, PathBuilder, Stroke};

/// `TABLE_STYLES`: (name, header background, band background, border).
pub const TABLE_STYLES: [(&str, &str, &str, &str); 5] = [
    ("Bleu", "#1a73e8", "#e8f0fe", "#a8c7fa"),
    ("Vert", "#34a853", "#e6f4ea", "#a8dab5"),
    ("Gris", "#5f6368", "#f1f3f4", "#bdc1c6"),
    ("Rouge", "#ea4335", "#fce8e6", "#f5b3ac"),
    ("Minimal", "#202124", "#ffffff", "#dadce0"),
];

fn rows_cols(el: &Element) -> (usize, usize) {
    (el.f_or("rows", 0.0).max(0.0) as usize, el.f_or("cols", 0.0).max(0.0) as usize)
}

fn edges(fracs: Option<&Value>, n: usize, total: f64) -> Vec<f64> {
    let given: Vec<f64> = fracs.and_then(Value::as_array).map(|a| a.iter().map(|v| v.as_f64().unwrap_or(0.0)).collect()).unwrap_or_default();
    let sizes = if given.len() == n && n > 0 { given } else { vec![1.0 / n.max(1) as f64; n] };
    let sum: f64 = sizes.iter().sum();
    let sum = if sum == 0.0 { 1.0 } else { sum };
    let mut out = vec![0.0];
    for (c, s) in sizes.iter().enumerate() {
        out.push(out[c] + (s / sum) * total);
    }
    out
}

/// `colEdges`.
pub fn col_edges(el: &Element, w: f64) -> Vec<f64> {
    edges(el.get("colWidths"), rows_cols(el).1, w)
}

/// `rowEdges`.
pub fn row_edges(el: &Element, h: f64) -> Vec<f64> {
    edges(el.get("rowHeights"), rows_cols(el).0, h)
}

/// `cellAt`: the cell at a point given as fractions of the table box.
pub fn cell_at(el: &Element, fx: f64, fy: f64) -> Option<(usize, usize)> {
    if !(0.0..=1.0).contains(&fx) || !(0.0..=1.0).contains(&fy) {
        return None;
    }
    let (rows, cols) = rows_cols(el);
    let cx = col_edges(el, 1.0);
    let ry = row_edges(el, 1.0);
    let col = (0..cols).find(|&c| fx >= cx[c] && fx < cx[c + 1])?;
    let row = (0..rows).find(|&r| fy >= ry[r] && fy < ry[r + 1])?;
    Some((row, col))
}

/// The cell object at (row, col), if any.
pub fn cell(el: &Element, row: usize, col: usize) -> Option<&Value> {
    el.get("cells")?.as_array()?.get(row)?.as_array()?.get(col)
}

/// `renderTable`.
#[allow(clippy::too_many_arguments)]
pub fn render_table(ctx: &mut Ctx, el: &Element, x: f64, y: f64, w: f64, h: f64, sf: f64, m: &dyn Measure) {
    let (rows, cols) = rows_cols(el);
    let cx = col_edges(el, w);
    let ry = row_edges(el, h);
    let border = el.s("borderColor").unwrap_or("#9aa0a6");
    let header_bg = el.s("headerBg").unwrap_or("#1a73e8");
    let band_bg = el.s("bandBg").unwrap_or("#f1f3f4");
    let fs = el.f_or("fontSize", 14.0) * sf;
    let header_row = el.truthy("headerRow");
    let banded = el.truthy("banded");
    let first_col = el.truthy("firstCol");
    ctx.save();
    let clip = PathBuilder::new().rect(x, y, w, h).take();
    ctx.clip(&clip);
    for r in 0..rows {
        for c in 0..cols {
            let cv = cell(el, r, c);
            let mut bg = cv.and_then(|v| v.get("bg")).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
            if bg.is_none() && header_row && r == 0 {
                bg = Some(header_bg.to_string());
            } else if bg.is_none() && banded && (if header_row { r % 2 == 0 } else { r % 2 == 1 }) {
                bg = Some(band_bg.to_string());
            }
            if let Some(bg) = bg {
                ctx.fill_rect(x + cx[c], y + ry[r], cx[c + 1] - cx[c], ry[r + 1] - ry[r], &Paint::css(&bg));
            }
        }
    }
    let mut grid = PathBuilder::new();
    for e in cx.iter().take(cols + 1) {
        grid.move_to(x + e, y).line_to(x + e, y + h);
    }
    for e in ry.iter().take(rows + 1) {
        grid.move_to(x, y + e).line_to(x + w, y + e);
    }
    ctx.stroke(&grid.take(), &Paint::css(border), &Stroke { width: sf, ..Stroke::default() });
    for r in 0..rows {
        for c in 0..cols {
            let Some(cv) = cell(el, r, c) else { continue };
            let text = cv.get("text").and_then(Value::as_str).unwrap_or("");
            if text.is_empty() {
                continue;
            }
            let is_header = header_row && r == 0;
            let bold = crate::model::truthy(cv.get("bold")) || is_header || (first_col && c == 0);
            let color = cv.get("color").and_then(Value::as_str).filter(|s| !s.is_empty()).unwrap_or(if is_header { "#ffffff" } else { "#202124" });
            let mut style = plain_style("Arial, sans-serif", fs, color);
            style.bold = bold;
            let align = cv.get("align").and_then(Value::as_str).unwrap_or("left");
            let pad = 6.0 * sf;
            let tw = m.width(text, &style, 0.0);
            let tx = match align {
                "center" => x + (cx[c] + cx[c + 1]) / 2.0 - tw / 2.0,
                "right" => x + cx[c + 1] - pad - tw,
                _ => x + cx[c] + pad,
            };
            let ty = y + (ry[r] + ry[r + 1]) / 2.0;
            ctx.fill_text(text, &font_of(&style), tx, ty, Baseline::Middle, &Paint::css(color), 0.0);
        }
    }
    ctx.restore();
}

// ── Structure (pure: they return the keys to set) ──────────────────────────

fn cells_of(el: &Element) -> Vec<Vec<Value>> {
    el.get("cells").and_then(Value::as_array).map(|rows| rows.iter().map(|r| r.as_array().cloned().unwrap_or_default()).collect()).unwrap_or_default()
}

fn empty_cell() -> Value {
    json!({ "text": "" })
}

/// `makeTableCells`.
pub fn make_cells(rows: usize, cols: usize) -> Value {
    Value::Array((0..rows).map(|_| Value::Array((0..cols).map(|_| empty_cell()).collect())).collect())
}

fn cells_value(cells: Vec<Vec<Value>>) -> Value {
    Value::Array(cells.into_iter().map(Value::Array).collect())
}

/// `addRow` at `at` (default: the end): the keys to set (`rowHeights` removed).
pub fn add_row(el: &Element, at: Option<usize>) -> Vec<(&'static str, Option<Value>)> {
    let (rows, cols) = rows_cols(el);
    let mut cells = cells_of(el);
    let idx = at.unwrap_or(rows).min(cells.len());
    cells.insert(idx, (0..cols).map(|_| empty_cell()).collect());
    vec![("rows", Some(num((rows + 1) as f64))), ("cells", Some(cells_value(cells))), ("rowHeights", None)]
}

/// `addCol`.
pub fn add_col(el: &Element, at: Option<usize>) -> Vec<(&'static str, Option<Value>)> {
    let (_, cols) = rows_cols(el);
    let idx = at.unwrap_or(cols);
    let cells: Vec<Vec<Value>> = cells_of(el)
        .into_iter()
        .map(|mut r| {
            let i = idx.min(r.len());
            r.insert(i, empty_cell());
            r
        })
        .collect();
    vec![("cols", Some(num((cols + 1) as f64))), ("cells", Some(cells_value(cells))), ("colWidths", None)]
}

/// `delRow` (never the last row).
pub fn del_row(el: &Element, at: usize) -> Vec<(&'static str, Option<Value>)> {
    let (rows, _) = rows_cols(el);
    if rows <= 1 {
        return Vec::new();
    }
    let mut cells = cells_of(el);
    if at < cells.len() {
        cells.remove(at);
    }
    vec![("rows", Some(num((rows - 1) as f64))), ("cells", Some(cells_value(cells))), ("rowHeights", None)]
}

/// `delCol` (never the last column).
pub fn del_col(el: &Element, at: usize) -> Vec<(&'static str, Option<Value>)> {
    let (_, cols) = rows_cols(el);
    if cols <= 1 {
        return Vec::new();
    }
    let cells: Vec<Vec<Value>> = cells_of(el)
        .into_iter()
        .map(|mut r| {
            if at < r.len() {
                r.remove(at);
            }
            r
        })
        .collect();
    vec![("cols", Some(num((cols - 1) as f64))), ("cells", Some(cells_value(cells))), ("colWidths", None)]
}

/// `setCell`: the new `cells` with `patch` merged into (row, col).
pub fn set_cell(el: &Element, row: usize, col: usize, patch: &Map<String, Value>) -> Value {
    let cells: Vec<Vec<Value>> = cells_of(el)
        .into_iter()
        .enumerate()
        .map(|(ri, r)| {
            r.into_iter()
                .enumerate()
                .map(|(ci, c)| {
                    if ri == row && ci == col {
                        let mut obj = c.as_object().cloned().unwrap_or_default();
                        for (k, v) in patch {
                            obj.insert(k.clone(), v.clone());
                        }
                        Value::Object(obj)
                    } else {
                        c
                    }
                })
                .collect()
        })
        .collect();
    cells_value(cells)
}

/// A cell's text.
pub fn cell_text(el: &Element, row: usize, col: usize) -> String {
    cell(el, row, col).and_then(|c| c.get("text")).and_then(Value::as_str).unwrap_or("").to_string()
}

/// The font size of a table (`fontSize ?? 14`).
pub fn font_size(el: &Element) -> f64 {
    as_f64(el.get("fontSize")).unwrap_or(14.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> Element {
        Element::from_value(json!({ "type": "table", "rows": 2, "cols": 3, "cells": make_cells(2, 3), "colWidths": [0.5, 0.25, 0.25] })).expect("table")
    }

    #[test]
    fn edges_follow_the_fractions() {
        assert_eq!(col_edges(&table(), 100.0), [0.0, 50.0, 75.0, 100.0]);
        assert_eq!(row_edges(&table(), 10.0), [0.0, 5.0, 10.0]);
        assert_eq!(cell_at(&table(), 0.6, 0.7), Some((1, 1)));
        assert_eq!(cell_at(&table(), 1.0, 0.7), None, "the right edge is outside (half-open)");
    }

    #[test]
    fn structure_edits_keep_the_grid_rectangular() {
        let mut t = table();
        for (k, v) in add_col(&t, Some(1)) {
            match v {
                Some(v) => t.set(k, v),
                None => t.remove(k),
            }
        }
        assert_eq!(t.f("cols"), Some(4.0));
        assert!(t.get("colWidths").is_none());
        assert_eq!(cells_of(&t)[1].len(), 4);
        assert!(del_row(&Element::from_value(json!({ "rows": 1, "cols": 1, "cells": [[{}]] })).expect("t"), 0).is_empty());
        let mut patch = Map::new();
        patch.insert("text".into(), json!("x"));
        t.set("cells", set_cell(&t, 0, 2, &patch));
        assert_eq!(cell_text(&t, 0, 2), "x");
    }
}
