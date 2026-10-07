//! What to send so that the server's copy becomes ours: a plan of requests computed from the server's
//! workbook (re-read right before the save) and the edited one.
//!
//! The server takes a sheet's content through `PATCH /sheets/:sid` field by field
//! (`UpdateSheetDto`, `server/src/models/spreadsheet.rs`): `data` carries the cells and the column/row styles
//! (as the web sends them, `{ cells, colStyles, rowStyles }`), every other editable key travels under its own
//! name. A field the server cannot clear (JSON `null` is read as "absent") is sent as its empty value; keys
//! the API does not accept at all (`default_row_height`, `tab_color`, …) and the defined names (the web keeps
//! them in its collaboration room only) cannot be saved by REST and are reported, never dropped silently.

use serde_json::{json, Map, Value as Json};

use kubuno_office_sheets_core::model::sheet::Sheet;
use kubuno_office_sheets_core::model::workbook::Workbook;

/// The keys of `SheetData` the PATCH route accepts besides `data` (snake case, as stored).
pub const PATCHABLE: [&str; 16] = [
    "col_widths", "row_heights", "frozen_rows", "frozen_cols", "merges", "gridlines", "images", "equations", "charts", "shapes", "cf", "validations",
    "row_groups", "col_groups", "pivots", "protection",
];

/// One request of a save.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    /// `POST /sheets {id, name}`.
    Create { sheet: String, name: String },
    /// `PATCH /sheets/:sid` with these fields.
    Patch { sheet: String, body: Json },
    /// `DELETE /sheets/:sid`.
    Delete { sheet: String },
}

/// The plan, and what it cannot carry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub ops: Vec<Op>,
    /// Changes the API cannot save (names of the keys, `names` for the defined names).
    pub unsaved: Vec<String>,
}

/// The empty value of a key (what clears it when the server reads `null` as "no change").
fn empty_of(v: &Json) -> Json {
    match v {
        Json::Array(_) => Json::Array(Vec::new()),
        Json::Object(_) => Json::Object(Map::new()),
        Json::Bool(_) => Json::Bool(true),
        Json::Number(_) => json!(0),
        _ => Json::Null,
    }
}

/// The fields that bring `base`'s data to `mine`'s (`None` = nothing to send).
fn sheet_fields(base: Option<&Sheet>, mine: &Sheet, unsaved: &mut Vec<String>) -> Option<Json> {
    let mut body = Map::new();
    let base_cells = base.map(Sheet::cells_json);
    let mine_cells = mine.cells_json();
    let col_styles = |s: Option<&Sheet>| s.and_then(|s| s.get("col_styles").cloned());
    let row_styles = |s: Option<&Sheet>| s.and_then(|s| s.get("row_styles").cloned());
    if base_cells.as_ref() != Some(&mine_cells) || col_styles(base) != col_styles(Some(mine)) || row_styles(base) != row_styles(Some(mine)) {
        let mut data = Map::new();
        data.insert("cells".into(), mine_cells);
        match (col_styles(Some(mine)), col_styles(base)) {
            (Some(cs), _) => {
                data.insert("colStyles".into(), cs);
            }
            (None, Some(_)) => {
                data.insert("colStyles".into(), Json::Object(Map::new()));
            }
            (None, None) => {}
        }
        match (row_styles(Some(mine)), row_styles(base)) {
            (Some(rs), _) => {
                data.insert("rowStyles".into(), rs);
            }
            (None, Some(_)) => {
                data.insert("rowStyles".into(), Json::Object(Map::new()));
            }
            (None, None) => {}
        }
        body.insert("data".into(), Json::Object(data));
    }
    for k in PATCHABLE {
        let (b, m) = (base.and_then(|s| s.get(k)), mine.get(k));
        if b == m {
            continue;
        }
        match (m, b) {
            (Some(Json::Null) | None, Some(prev)) => {
                if k == "protection" {
                    // `null` cannot cross the API and an empty object still reads as protected.
                    unsaved.push(format!("{}: {k}", mine.meta.name));
                } else {
                    body.insert(k.into(), empty_of(prev));
                }
            }
            (Some(v), _) => {
                body.insert(k.into(), v.clone());
            }
            (None, None) => {}
        }
    }
    // `enc`: the encryption envelope, made by the host on the way out (`crypt`).
    let known: std::collections::HashSet<&str> = PATCHABLE.iter().copied().chain(["col_styles", "row_styles", "enc"]).collect();
    if let Some(b) = base {
        for (k, v) in &mine.data {
            if !known.contains(k.as_str()) && b.get(k) != Some(v) {
                unsaved.push(format!("{}: {k}", mine.meta.name));
            }
        }
    }
    if body.is_empty() {
        None
    } else {
        Some(Json::Object(body))
    }
}

/// The requests that turn `base` into `mine`: new sheets first (created then filled), then the changed
/// sheets, then the removed ones.
pub fn plan(base: &Workbook, mine: &Workbook) -> Plan {
    let mut p = Plan::default();
    let mut deletes = Vec::new();
    for s in &mine.sheets {
        match base.sheet(&s.meta.id) {
            None => {
                p.ops.push(Op::Create { sheet: s.meta.id.clone(), name: s.meta.name.clone() });
                let mut body = sheet_fields(None, s, &mut p.unsaved).and_then(|b| b.as_object().cloned()).unwrap_or_default();
                body.insert("position".into(), json!(s.meta.position));
                p.ops.push(Op::Patch { sheet: s.meta.id.clone(), body: Json::Object(body) });
            }
            Some(b) => {
                let mut body = sheet_fields(Some(b), s, &mut p.unsaved).and_then(|b| b.as_object().cloned()).unwrap_or_default();
                if b.meta.name != s.meta.name {
                    // The handler takes either the name or the position in one request.
                    p.ops.push(Op::Patch { sheet: s.meta.id.clone(), body: json!({ "name": s.meta.name }) });
                }
                if b.meta.position != s.meta.position {
                    body.insert("position".into(), json!(s.meta.position));
                }
                if !body.is_empty() {
                    p.ops.push(Op::Patch { sheet: s.meta.id.clone(), body: Json::Object(body) });
                }
            }
        }
    }
    for s in &base.sheets {
        if mine.sheet(&s.meta.id).is_none() {
            deletes.push(Op::Delete { sheet: s.meta.id.clone() });
        }
    }
    p.ops.extend(deletes);
    if base.names != mine.names {
        p.unsaved.push("names".into());
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use kubuno_office_sheets_core::model::cell::Cell;

    fn wb(json: &str) -> Workbook {
        Workbook::from_bytes(json.as_bytes(), "t").expect("a bundle")
    }

    const BASE: &str = r#"{"spreadsheet":{"id":"w","title":"T"},"sheets":[{"id":"s1","name":"A","position":0},{"id":"s2","name":"B","position":1}],
        "content":{"version":1,"sheets":{"s1":{"cells":{"A1":{"v":1}},"merges":["A1:B1"]},"s2":{"cells":{}}}}}"#;

    #[test]
    fn an_unchanged_workbook_sends_nothing() {
        let a = wb(BASE);
        assert_eq!(plan(&a, &a), Plan::default());
    }

    #[test]
    fn cells_and_fields_travel_as_the_web_sends_them() {
        let base = wb(BASE);
        let mut mine = base.clone();
        mine.sheets[0].cells.insert("B2".into(), Cell::value(json!("x")));
        mine.sheets[0].data.remove("merges");
        mine.sheets[0].set("frozen_rows", json!(1));
        let p = plan(&base, &mine);
        assert_eq!(p.ops.len(), 1);
        let Op::Patch { sheet, body } = &p.ops[0] else { panic!("{p:?}") };
        assert_eq!(sheet, "s1");
        assert_eq!(body["data"]["cells"]["B2"]["v"], json!("x"));
        assert_eq!(body["merges"], json!([]));
        assert_eq!(body["frozen_rows"], json!(1));
        assert!(body["data"].get("colStyles").is_none());
    }

    #[test]
    fn sheets_added_renamed_and_removed() {
        let base = wb(BASE);
        let mut mine = base.clone();
        mine.sheets.remove(1);
        mine.sheets[0].meta.name = "Ventes".into();
        let mut extra = mine.sheets[0].clone();
        extra.meta.id = "s3".into();
        extra.meta.name = "C".into();
        extra.meta.position = 2;
        mine.sheets.push(extra);
        let p = plan(&base, &mine);
        assert!(matches!(&p.ops[0], Op::Patch { body, .. } if body["name"] == json!("Ventes")));
        assert!(matches!(&p.ops[1], Op::Create { sheet, name } if sheet == "s3" && name == "C"));
        assert!(matches!(&p.ops[2], Op::Patch { sheet, body } if sheet == "s3" && body["position"] == json!(2)));
        assert!(matches!(&p.ops[3], Op::Delete { sheet } if sheet == "s2"));
    }

    #[test]
    fn what_cannot_be_saved_is_reported() {
        let base = wb(BASE);
        let mut mine = base.clone();
        mine.names.insert("X".into(), json!("=1"));
        mine.sheets[0].set("tab_color", json!("#ff0000"));
        let p = plan(&base, &mine);
        assert!(p.unsaved.contains(&"names".to_string()));
        assert!(p.unsaved.iter().any(|u| u.ends_with("tab_color")));
    }
}
