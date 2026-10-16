//! The workbook: its sheets in order, the workbook-level defined names, and where they come from -
//! the API (`GET /spreadsheets/:id` + each `GET …/sheets/:sid`), a content file (`.kbcal`, gzipped or not),
//! or the desktop's bundle (the crash journal) - and back.

use std::collections::HashMap;
use std::io::{Read, Write};

use serde_json::{Map, Value as Json};

use crate::formula::eval::{CellMap, Data};
use crate::model::sheet::{Sheet, SheetMeta};

/// Why a workbook could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    /// Not JSON (after gunzip when it was gzipped).
    NotJson(String),
    /// JSON, but not a workbook.
    NotWorkbook,
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::NotJson(e) => write!(f, "not a workbook file: {e}"),
            LoadError::NotWorkbook => f.write_str("the file is not a Kubuno workbook"),
        }
    }
}

impl std::error::Error for LoadError {}

/// A workbook.
#[derive(Clone, Debug, PartialEq)]
pub struct Workbook {
    /// The server id (`None` for a local file).
    pub id: Option<String>,
    pub title: String,
    /// The `spreadsheet` row as the server sent it, kept.
    pub spreadsheet: Map<String, Json>,
    /// The sheets, in `position` order.
    pub sheets: Vec<Sheet>,
    /// Defined names (`names` of the content file): upper-case name → formula text.
    pub names: Map<String, Json>,
    /// The content file's other keys (`version`…), kept.
    pub content_rest: Map<String, Json>,
}

/// Inflates `bytes` when they are gzipped (`1f 8b`), else returns them.
pub fn gunzip(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(bytes).read_to_end(&mut out).map_err(|e| e.to_string())?;
        Ok(out)
    } else {
        Ok(bytes.to_vec())
    }
}

/// Gzips `bytes` (the content file's encoding).
pub fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    // Writing into a Vec cannot fail.
    if e.write_all(bytes).is_err() {
        return Vec::new();
    }
    e.finish().unwrap_or_default()
}

impl Workbook {
    /// An empty workbook as the server creates one (one sheet « Feuille 1 »).
    pub fn new_local(title: &str, sheet_id: &str) -> Workbook {
        let meta = SheetMeta { id: sheet_id.to_string(), name: "Feuille 1".into(), position: 0, raw: Map::new() };
        Workbook {
            id: None,
            title: title.to_string(),
            spreadsheet: Map::new(),
            sheets: vec![Sheet::empty(meta)],
            names: Map::new(),
            content_rest: {
                let mut m = Map::new();
                m.insert("version".into(), Json::from(1));
                m
            },
        }
    }

    /// From the API: the `GET /spreadsheets/:id` body and, for each sheet, its `GET …/sheets/:sid` body
    /// (`{ sheet, data, names }`).
    pub fn from_api(head: &Json, sheet_bodies: &[Json]) -> Result<Workbook, LoadError> {
        let ss = head.get("spreadsheet").and_then(Json::as_object).ok_or(LoadError::NotWorkbook)?;
        let mut metas: Vec<SheetMeta> = head.get("sheets").and_then(Json::as_array).ok_or(LoadError::NotWorkbook)?.iter().filter_map(SheetMeta::from_json).collect();
        metas.sort_by_key(|m| m.position);
        let mut names = Map::new();
        let mut sheets = Vec::with_capacity(metas.len());
        for meta in metas {
            let body = sheet_bodies.iter().find(|b| b.get("sheet").and_then(|s| s.get("id")).and_then(Json::as_str) == Some(meta.id.as_str()));
            let data = body.and_then(|b| b.get("data")).cloned().unwrap_or_else(|| Json::Object(Map::new()));
            if let Some(n) = body.and_then(|b| b.get("names")).and_then(Json::as_object) {
                names = n.clone();
            }
            sheets.push(Sheet::from_data(meta, &data));
        }
        if sheets.is_empty() {
            return Err(LoadError::NotWorkbook);
        }
        let mut rest = Map::new();
        rest.insert("version".into(), Json::from(1));
        Ok(Workbook {
            id: ss.get("id").and_then(Json::as_str).map(str::to_string),
            title: ss.get("title").and_then(Json::as_str).unwrap_or("").to_string(),
            spreadsheet: ss.clone(),
            sheets,
            names,
            content_rest: rest,
        })
    }

    /// From a content file (`{ version, sheets: { id: data }, names }`): the sheets get the names the file
    /// does not carry (« Feuille 1 »…), in key order.
    pub fn from_content(content: &Json, title: &str) -> Result<Workbook, LoadError> {
        let o = content.as_object().ok_or(LoadError::NotWorkbook)?;
        let sheets_obj = o.get("sheets").and_then(Json::as_object).ok_or(LoadError::NotWorkbook)?;
        let sheets: Vec<Sheet> = sheets_obj
            .iter()
            .enumerate()
            .map(|(i, (id, data))| Sheet::from_data(SheetMeta { id: id.clone(), name: format!("Feuille {}", i + 1), position: i as i64, raw: Map::new() }, data))
            .collect();
        if sheets.is_empty() {
            return Err(LoadError::NotWorkbook);
        }
        let mut rest = o.clone();
        rest.remove("sheets");
        let names = match rest.remove("names") {
            Some(Json::Object(n)) => n,
            _ => Map::new(),
        };
        Ok(Workbook { id: None, title: title.to_string(), spreadsheet: Map::new(), sheets, names, content_rest: rest })
    }

    /// From the desktop's bundle (`{ spreadsheet, sheets: [meta], content }`).
    pub fn from_bundle(b: &Json) -> Result<Workbook, LoadError> {
        let ss = b.get("spreadsheet").and_then(Json::as_object).cloned().unwrap_or_default();
        let content = b.get("content").ok_or(LoadError::NotWorkbook)?;
        let mut wb = Workbook::from_content(content, ss.get("title").and_then(Json::as_str).unwrap_or(""))?;
        let metas: Vec<SheetMeta> = b.get("sheets").and_then(Json::as_array).map(|a| a.iter().filter_map(SheetMeta::from_json).collect()).unwrap_or_default();
        for s in &mut wb.sheets {
            if let Some(m) = metas.iter().find(|m| m.id == s.meta.id) {
                s.meta = m.clone();
            }
        }
        wb.sheets.sort_by_key(|s| s.meta.position);
        wb.id = ss.get("id").and_then(Json::as_str).map(str::to_string);
        wb.spreadsheet = ss;
        Ok(wb)
    }

    /// Reads any of the three shapes from bytes (gzipped or not).
    pub fn from_bytes(bytes: &[u8], title: &str) -> Result<Workbook, LoadError> {
        let raw = gunzip(bytes).map_err(LoadError::NotJson)?;
        let v: Json = serde_json::from_slice(&raw).map_err(|e| LoadError::NotJson(e.to_string()))?;
        if v.get("content").is_some() && v.get("sheets").is_some_and(Json::is_array) {
            Workbook::from_bundle(&v)
        } else if v.get("spreadsheet").is_some() && v.get("sheets").is_some_and(Json::is_array) {
            // A `GET /spreadsheets/:id` body alone: no data.
            Workbook::from_api(&v, &[])
        } else {
            Workbook::from_content(&v, title)
        }
    }

    /// The content file (`{ version, sheets, names }`), as the server stores it.
    pub fn to_content(&self) -> Json {
        let mut o = self.content_rest.clone();
        o.insert("sheets".into(), Json::Object(self.sheets.iter().map(|s| (s.meta.id.clone(), s.to_data())).collect()));
        if !self.names.is_empty() || o.contains_key("names") {
            o.insert("names".into(), Json::Object(self.names.clone()));
        }
        Json::Object(o)
    }

    /// The bundle: everything needed to reopen the workbook (the journal; the save's digest input). Only
    /// what the user can change is in it - the id, the title, each sheet's id, name and position, the
    /// content - so two reads of an unchanged workbook give the same bytes (no `updated_at`).
    pub fn to_bundle(&self) -> Json {
        let mut ss = Map::new();
        if let Some(id) = &self.id {
            ss.insert("id".into(), Json::String(id.clone()));
        }
        ss.insert("title".into(), Json::String(self.title.clone()));
        let metas: Vec<Json> = self
            .sheets
            .iter()
            .map(|s| serde_json::json!({ "id": s.meta.id, "name": s.meta.name, "position": s.meta.position }))
            .collect();
        serde_json::json!({
            "spreadsheet": Json::Object(ss),
            "sheets": metas,
            "content": self.to_content(),
        })
    }

    /// The bundle's bytes (compact JSON, keys sorted).
    pub fn to_bundle_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(&self.to_bundle()).unwrap_or_default()
    }

    /// The sheet with this id.
    pub fn sheet(&self, id: &str) -> Option<&Sheet> {
        self.sheets.iter().find(|s| s.meta.id == id)
    }

    pub fn sheet_mut(&mut self, id: &str) -> Option<&mut Sheet> {
        self.sheets.iter_mut().find(|s| s.meta.id == id)
    }

    /// The defined names as the engine reads them (name → formula text).
    pub fn names_map(&self) -> HashMap<String, String> {
        self.names.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect()
    }

    /// Every sheet's cells by name - only when there are several sheets, as on the web.
    pub fn sheets_by_name(&self) -> Option<HashMap<String, &CellMap>> {
        if self.sheets.len() <= 1 {
            return None;
        }
        Some(self.sheets.iter().map(|s| (s.meta.name.clone(), &s.cells)).collect())
    }

    /// What a formula of sheet `index` is evaluated against.
    pub fn data<'a>(&'a self, index: usize, names: &'a HashMap<String, String>, by_name: Option<&'a HashMap<String, &'a CellMap>>) -> Option<Data<'a>> {
        let s = self.sheets.get(index)?;
        Some(Data { cells: &s.cells, sheets: by_name, names })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_content_file_round_trips_byte_for_byte() {
        let text = r#"{"names":{"TAUX":"=0.2"},"sheets":{"a1b2":{"cells":{"A1":{"f":"=B1*2","s":{"bold":true}},"B1":{"v":2.5}},"col_widths":{},"frozen_cols":0,"frozen_rows":0,"row_heights":{"3":40}}},"version":1}"#;
        let wb = Workbook::from_bytes(text.as_bytes(), "t").unwrap();
        assert_eq!(serde_json::to_string(&wb.to_content()).unwrap(), text);
        let gz = gzip(text.as_bytes());
        let wb2 = Workbook::from_bytes(&gz, "t").unwrap();
        assert_eq!(wb2, wb);
    }

    #[test]
    fn the_api_and_the_bundle() {
        let head = json!({"spreadsheet": {"id": "w1", "title": "Budget"}, "sheets": [{"id": "s2", "name": "B", "position": 1}, {"id": "s1", "name": "A", "position": 0}]});
        let bodies = vec![
            json!({"sheet": {"id": "s1"}, "data": {"cells": {"A1": {"v": 1}}}, "names": {"X": "=1"}}),
            json!({"sheet": {"id": "s2"}, "data": {"cells": {}}, "names": {"X": "=1"}}),
        ];
        let wb = Workbook::from_api(&head, &bodies).unwrap();
        assert_eq!(wb.sheets.iter().map(|s| s.meta.name.as_str()).collect::<Vec<_>>(), vec!["A", "B"]);
        assert_eq!(wb.title, "Budget");
        let again = Workbook::from_bytes(&wb.to_bundle_bytes(), "").unwrap();
        assert_eq!(again.to_bundle(), wb.to_bundle());
        assert_eq!(again.id.as_deref(), Some("w1"));
        assert!(wb.sheets_by_name().is_some());
    }
}
