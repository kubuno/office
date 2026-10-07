//! One sheet: its metadata row (`SheetMeta`: id, name, position) and its data (`SheetData` of the content
//! file), the cells modelled, every other key kept as read (snake case, as stored).

use serde_json::{Map, Value as Json};

use crate::addr::{self, Rect};
use crate::formula::eval::CellMap;
use crate::model::cell::{json_number, Cell};
use crate::model::style::{self, Style};

/// The web's default column width and row height (`SA:245-246`).
pub const DEFAULT_COL_WIDTH: f64 = 100.0;
pub const DEFAULT_ROW_HEIGHT: f64 = 24.0;

/// A sheet's row of `spreadsheet_sheets`.
#[derive(Clone, Debug, PartialEq)]
pub struct SheetMeta {
    pub id: String,
    pub name: String,
    pub position: i64,
    /// The whole row as the server sent it (`spreadsheet_id`, `created_at`…), kept.
    pub raw: Map<String, Json>,
}

impl SheetMeta {
    pub fn from_json(v: &Json) -> Option<SheetMeta> {
        let o = v.as_object()?;
        Some(SheetMeta {
            id: o.get("id")?.as_str()?.to_string(),
            name: o.get("name").and_then(Json::as_str).unwrap_or("").to_string(),
            position: o.get("position").and_then(Json::as_i64).unwrap_or(0),
            raw: o.clone(),
        })
    }

    pub fn to_json(&self) -> Json {
        let mut o = self.raw.clone();
        o.insert("id".into(), Json::String(self.id.clone()));
        o.insert("name".into(), Json::String(self.name.clone()));
        o.insert("position".into(), Json::from(self.position));
        Json::Object(o)
    }
}

/// An outline group of rows (1-based) or columns (0-based), inclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutlineGroup {
    pub start: u32,
    pub end: u32,
    pub collapsed: bool,
}

/// A sheet.
#[derive(Clone, Debug, PartialEq)]
pub struct Sheet {
    pub meta: SheetMeta,
    pub cells: CellMap,
    /// Every `SheetData` key but `cells`, as stored.
    pub data: Map<String, Json>,
}

fn num_of(v: Option<&Json>) -> Option<f64> {
    v.and_then(Json::as_f64)
}

impl Sheet {
    /// A sheet from its metadata and its stored data (`get_sheet_data`).
    pub fn from_data(meta: SheetMeta, data: &Json) -> Sheet {
        let mut map = data.as_object().cloned().unwrap_or_default();
        let cells = match map.remove("cells") {
            Some(Json::Object(o)) => o.iter().map(|(k, v)| (k.clone(), Cell::from_json(v))).collect(),
            _ => CellMap::new(),
        };
        Sheet { meta, cells, data: map }
    }

    /// A new empty sheet as the server creates one (`empty_sheet_data`).
    pub fn empty(meta: SheetMeta) -> Sheet {
        let data = serde_json::json!({ "cells": {}, "col_widths": {}, "row_heights": {}, "frozen_rows": 0, "frozen_cols": 0 });
        Sheet::from_data(meta, &data)
    }

    /// The stored data (`cells` included), keys sorted as the server writes them.
    pub fn to_data(&self) -> Json {
        let mut o = self.data.clone();
        o.insert("cells".into(), self.cells_json());
        Json::Object(o)
    }

    /// The cells as stored.
    pub fn cells_json(&self) -> Json {
        Json::Object(self.cells.iter().map(|(k, c)| (k.clone(), c.to_json())).collect())
    }

    pub fn get(&self, key: &str) -> Option<&Json> {
        self.data.get(key)
    }

    pub fn set(&mut self, key: &str, v: Json) {
        self.data.insert(key.to_string(), v);
    }

    /// `default_row_height` (imported) or 24.
    pub fn default_row_height(&self) -> f64 {
        num_of(self.data.get("default_row_height")).unwrap_or(DEFAULT_ROW_HEIGHT)
    }

    /// `default_col_width` (imported) or 100.
    pub fn default_col_width(&self) -> f64 {
        num_of(self.data.get("default_col_width")).unwrap_or(DEFAULT_COL_WIDTH)
    }

    /// The explicit width of a column (by letter), if any (0 = hidden).
    pub fn col_width_override(&self, letter: &str) -> Option<f64> {
        num_of(self.data.get("col_widths").and_then(|m| m.get(letter)))
    }

    /// The explicit height of a row, if any (0 = hidden).
    pub fn row_height_override(&self, row: u32) -> Option<f64> {
        num_of(self.data.get("row_heights").and_then(|m| m.get(row.to_string())))
    }

    /// Every explicit row height (row → px).
    pub fn row_heights(&self) -> Vec<(u32, f64)> {
        self.data
            .get("row_heights")
            .and_then(Json::as_object)
            .map(|o| o.iter().filter_map(|(k, v)| Some((k.parse::<u32>().ok()?, v.as_f64()?))).collect())
            .unwrap_or_default()
    }

    /// Every explicit column width (column index → px).
    pub fn col_widths(&self) -> Vec<(u32, f64)> {
        self.data
            .get("col_widths")
            .and_then(Json::as_object)
            .map(|o| o.iter().filter_map(|(k, v)| Some((addr::Cell::parse(&format!("{k}1"))?.col, v.as_f64()?))).collect())
            .unwrap_or_default()
    }

    pub fn set_col_width(&mut self, col: u32, w: f64) {
        let letter = addr::index_to_col(col as i64);
        let entry = self.data.entry("col_widths").or_insert_with(|| Json::Object(Map::new()));
        if !entry.is_object() {
            *entry = Json::Object(Map::new());
        }
        if let Some(o) = entry.as_object_mut() {
            o.insert(letter, json_number(w));
        }
    }

    pub fn set_row_height(&mut self, row: u32, h: f64) {
        let entry = self.data.entry("row_heights").or_insert_with(|| Json::Object(Map::new()));
        if !entry.is_object() {
            *entry = Json::Object(Map::new());
        }
        if let Some(o) = entry.as_object_mut() {
            o.insert(row.to_string(), json_number(h));
        }
    }

    pub fn frozen_rows(&self) -> u32 {
        num_of(self.data.get("frozen_rows")).unwrap_or(0.0).max(0.0) as u32
    }

    pub fn frozen_cols(&self) -> u32 {
        num_of(self.data.get("frozen_cols")).unwrap_or(0.0).max(0.0) as u32
    }

    /// `gridlines !== false`.
    pub fn gridlines(&self) -> bool {
        self.data.get("gridlines") != Some(&Json::Bool(false))
    }

    /// The merged ranges (`merges`).
    pub fn merges(&self) -> Vec<Rect> {
        self.data.get("merges").and_then(Json::as_array).map(|a| a.iter().filter_map(|m| m.as_str().and_then(Rect::parse)).collect()).unwrap_or_default()
    }

    pub fn set_merges(&mut self, merges: &[Rect]) {
        self.set("merges", Json::Array(merges.iter().map(|r| Json::String(r.a1_pair())).collect()));
    }

    fn groups(&self, key: &str) -> Vec<OutlineGroup> {
        self.data
            .get(key)
            .and_then(Json::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|g| {
                        let o = g.as_object()?;
                        Some(OutlineGroup {
                            start: o.get("start")?.as_f64()?.max(0.0) as u32,
                            end: o.get("end")?.as_f64()?.max(0.0) as u32,
                            collapsed: o.get("collapsed").and_then(Json::as_bool).unwrap_or(false),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn row_groups(&self) -> Vec<OutlineGroup> {
        self.groups("row_groups")
    }

    pub fn col_groups(&self) -> Vec<OutlineGroup> {
        self.groups("col_groups")
    }

    /// The column's default style (`col_styles[letter]`).
    pub fn col_style(&self, letter: &str) -> Option<&Json> {
        self.data.get("col_styles").and_then(|m| m.get(letter))
    }

    /// The row's default style (`row_styles[row]`).
    pub fn row_style(&self, row: u32) -> Option<&Json> {
        self.data.get("row_styles").and_then(|m| m.get(row.to_string()))
    }

    /// `styleAt(col, row, cell.s)`.
    pub fn style_at(&self, col: u32, row: u32) -> Style {
        let letter = addr::index_to_col(col as i64);
        let cell_s = self.cells.get(&addr::key(col, row)).and_then(|c| c.s.as_ref());
        style::effective(self.col_style(&letter), self.row_style(row), cell_s)
    }

    /// The conditional-formatting blocks (`cf`).
    pub fn cf(&self) -> Option<&Json> {
        self.data.get("cf")
    }

    /// The data-validation blocks (`validations`).
    pub fn validations(&self) -> Option<&Json> {
        self.data.get("validations")
    }

    /// The used extent: the largest column index and row number holding a cell (`usedMaxCol/Row`).
    pub fn used_extent(&self) -> (u32, u32) {
        let mut mc = 0;
        let mut mr = 0;
        for k in self.cells.keys() {
            if let Some(c) = addr::Cell::parse(k) {
                mc = mc.max(c.col);
                mr = mr.max(c.row);
            }
        }
        (mc, mr)
    }

    /// Whether the sheet's cells are encrypted (`enc` envelope present).
    pub fn is_encrypted(&self) -> bool {
        matches!(self.data.get("enc"), Some(Json::Object(_)))
    }

    /// Whether the sheet is password-protected.
    pub fn is_protected(&self) -> bool {
        matches!(self.data.get("protection"), Some(Json::Object(_)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn meta() -> SheetMeta {
        SheetMeta { id: "s1".into(), name: "Feuille 1".into(), position: 0, raw: Map::new() }
    }

    #[test]
    fn data_round_trips() {
        let d = json!({"cells": {"A1": {"v": 1}}, "col_widths": {"B": 140}, "merges": ["A1:B2"], "zz": [1, 2], "gridlines": false});
        let s = Sheet::from_data(meta(), &d);
        assert_eq!(s.to_data(), d);
        assert_eq!(s.col_width_override("B"), Some(140.0));
        assert!(!s.gridlines());
        assert_eq!(s.merges(), vec![Rect::new(0, 1, 1, 2)]);
    }

    #[test]
    fn styles_combine_column_row_and_cell() {
        let d = json!({"cells": {"B2": {"v": 1, "s": {"italic": true}}}, "col_styles": {"B": {"bold": true}}, "row_styles": {"2": {"color": "#ff0000"}}});
        let s = Sheet::from_data(meta(), &d);
        let st = s.style_at(1, 2);
        assert!(st.is_bold() && st.is_italic());
        assert_eq!(st.color.as_deref(), Some("#ff0000"));
    }
}
