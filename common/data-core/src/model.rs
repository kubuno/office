//! The report document, held losslessly. A port of the shapes `data-api.ts` exposes and the server's
//! `models/data.rs` writes: a [`Report`] plus its [`ReportPage`]s and [`Widget`]s. This is the payload of
//! `GET /office/data/reports/:id` and `POST /office/data/reports/open-by-file`, and the body of the
//! best-effort `.kbdrp` mirror file (`{ version, report, pages, widgets }`).
//!
//! Losslessness: every struct keeps a `#[serde(flatten)] extra` map so an unknown server field survives a
//! round trip, and known fields are declared in the server's order so re-serialisation matches its output.
//! `config`/`theme` stay as [`serde_json::Value`] (free-form JSONB on the server), key order preserved.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The `.kbdrp` / REST report envelope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReportDoc {
    #[serde(default = "one")]
    pub version: u32,
    pub report: Report,
    #[serde(default)]
    pub pages: Vec<ReportPage>,
    #[serde(default)]
    pub widgets: Vec<Widget>,
}

fn one() -> u32 {
    1
}

/// A report row (`office_data.reports`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Report {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub theme: Value,
    #[serde(default)]
    pub page_count: i64,
    #[serde(default)]
    pub dataset_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(default)]
    pub is_public: bool,
    #[serde(default)]
    pub is_trashed: bool,
    #[serde(default)]
    pub is_starred: bool,
    /// Every other server field (share_token, thumbnail_url, created_at, updated_at, …), kept verbatim.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Report {
    /// The theme's `paletteId`, falling back to `"kubuno"` as the editor does.
    pub fn palette_id(&self) -> String {
        self.theme
            .get("paletteId")
            .and_then(Value::as_str)
            .unwrap_or("kubuno")
            .to_string()
    }
}

/// A report page (`office_data.report_pages`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReportPage {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report_id: Option<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub position: i64,
    #[serde(default = "default_page_w")]
    pub width: i64,
    #[serde(default = "default_page_h")]
    pub height: i64,
    #[serde(default)]
    pub background: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn default_page_w() -> i64 {
    1200
}
fn default_page_h() -> i64 {
    800
}

/// A widget placed on a page (`office_data.widgets`). Geometry is page pixels; `config` is free-form JSONB.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Widget {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report_id: Option<String>,
    pub widget_type: String,
    #[serde(default)]
    pub x: i64,
    #[serde(default)]
    pub y: i64,
    #[serde(default)]
    pub width: i64,
    #[serde(default)]
    pub height: i64,
    #[serde(default)]
    pub config: Value,
    #[serde(default)]
    pub z_index: i64,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Widget {
    /// Read a config key (`config[key]`), or `Null` if absent.
    pub fn config_get(&self, key: &str) -> Value {
        self.config.get(key).cloned().unwrap_or(Value::Null)
    }

    /// Write a config key, creating the object if the config was not one (matching the web's spread).
    pub fn config_set(&mut self, key: &str, value: Value) {
        if !self.config.is_object() {
            self.config = Value::Object(Map::new());
        }
        if let Some(obj) = self.config.as_object_mut() {
            obj.insert(key.to_string(), value);
        }
    }

    /// `config.locked` - whether the widget is locked against move/resize.
    pub fn locked(&self) -> bool {
        self.config_get("locked").as_bool().unwrap_or(false)
    }

    /// `config.title` - the widget's display title, if any.
    pub fn title(&self) -> Option<String> {
        self.config_get("title").as_str().map(str::to_string)
    }

    /// `config.dataset_id` - the dataset the widget queries, if bound.
    pub fn dataset_id(&self) -> Option<String> {
        self.config_get("dataset_id").as_str().map(str::to_string)
    }
}

/// Parse a report document from JSON bytes (`.kbdrp` content or a REST body).
pub fn parse(bytes: &[u8]) -> Result<ReportDoc, serde_json::Error> {
    serde_json::from_slice(bytes)
}

/// Serialise a report document to compact JSON, the way the server does.
pub fn to_json(doc: &ReportDoc) -> Result<String, serde_json::Error> {
    serde_json::to_string(doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> Value {
        json!({
            "version": 1,
            "report": {
                "id": "r1", "owner_id": "u1", "title": "Ventes",
                "description": null,
                "theme": { "paletteId": "vibrant" },
                "page_count": 1, "dataset_ids": ["d1"], "file_id": "f1",
                "share_token": null, "is_public": false, "is_trashed": false,
                "is_starred": true, "thumbnail_url": null,
                "created_at": "2026-10-11T00:00:00Z", "updated_at": "2026-10-11T00:00:00Z"
            },
            "pages": [
                { "id": "p1", "report_id": "r1", "title": "Page 1", "position": 0,
                  "width": 1200, "height": 800, "background": null,
                  "created_at": "2026-10-11T00:00:00Z" }
            ],
            "widgets": [
                { "id": "w1", "page_id": "p1", "report_id": "r1", "widget_type": "bar_chart",
                  "x": 16, "y": 16, "width": 400, "height": 240,
                  "config": { "title": "CA", "dataset_id": "d1", "locked": true,
                              "metrics": [ { "column": "amount", "function": "SUM" } ] },
                  "z_index": 0,
                  "created_at": "2026-10-11T00:00:00Z", "updated_at": "2026-10-11T00:00:00Z" }
            ]
        })
    }

    #[test]
    fn round_trip_is_lossless() {
        let original = sample();
        let doc: ReportDoc = serde_json::from_value(original.clone()).expect("parse");
        let back: Value = serde_json::to_value(&doc).expect("serialise");
        // serde_json::Value equality (with preserve_order) ignores object key order, so this asserts
        // every field - including the unknown ones kept in `extra` - survived the round trip.
        assert_eq!(original, back);
    }

    #[test]
    fn config_and_accessors() {
        let doc: ReportDoc = serde_json::from_value(sample()).unwrap();
        let w = &doc.widgets[0];
        assert!(w.locked());
        assert_eq!(w.title().as_deref(), Some("CA"));
        assert_eq!(w.dataset_id().as_deref(), Some("d1"));
        assert_eq!(doc.report.palette_id(), "vibrant");
    }

    #[test]
    fn config_set_creates_object_when_needed() {
        let mut w = Widget {
            id: "w".into(),
            page_id: None,
            report_id: None,
            widget_type: "card".into(),
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            config: Value::Null,
            z_index: 0,
            extra: Map::new(),
        };
        w.config_set("title", json!("Hi"));
        assert_eq!(w.title().as_deref(), Some("Hi"));
    }

    #[test]
    fn parse_tolerates_create_snapshot() {
        // The create-time .kbdrp mirror has empty pages/widgets.
        let bytes = br#"{"version":1,"report":{"id":"r","title":"N"},"pages":[],"widgets":[]}"#;
        let doc = parse(bytes).expect("parse");
        assert_eq!(doc.report.title, "N");
        assert!(doc.pages.is_empty());
    }
}
