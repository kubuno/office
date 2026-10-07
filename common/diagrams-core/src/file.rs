//! The `.kbdia` file: `gzip(JSON { "version": 1, "pages": { "<page id>": <page data> } })`
//! (`office/server/src/services/content_files.rs:373-420`), also read when it is plain JSON (older files
//! and the server's drafts are tolerated the same way), and the web's « Export JSON »
//! (`GET /diagrams/:id/export/json`, `{ format: "kubuno-diagram/v1", diagram, pages: [{ id, name, …, data }] }`).

use std::io::{Read, Write};

use serde_json::{Map, Value};

use crate::model::PageData;

/// One page of a diagram: its metadata (from the server, or invented for a local file) and its data.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub id: String,
    pub name: String,
    pub bg_color: String,
    pub position: i64,
    pub data: PageData,
}

impl Page {
    pub fn new(id: &str, name: &str, data: PageData) -> Self {
        Self { id: id.to_string(), name: name.to_string(), bg_color: "#ffffff".into(), position: 0, data }
    }
}

/// Why a file could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileError {
    /// Not gzip-compressed JSON nor JSON.
    NotJson(String),
    /// JSON, but not a diagram (no `pages`).
    NotADiagram,
}

impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileError::NotJson(e) => write!(f, "ce fichier n'est pas un diagramme lisible ({e})"),
            FileError::NotADiagram => write!(f, "ce fichier ne contient pas de pages de diagramme"),
        }
    }
}

impl std::error::Error for FileError {}

/// Un-gzips when the bytes start with the gzip signature, else returns them as they are (`gunzip`).
pub fn gunzip(raw: &[u8]) -> Result<Vec<u8>, FileError> {
    if raw.len() >= 2 && raw[0] == 0x1f && raw[1] == 0x8b {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(raw).read_to_end(&mut out).map_err(|e| FileError::NotJson(e.to_string()))?;
        Ok(out)
    } else {
        Ok(raw.to_vec())
    }
}

/// Gzips (`gzip`, default compression).
pub fn gzip(raw: &[u8]) -> Vec<u8> {
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    // Writing into a Vec cannot fail.
    let _ = enc.write_all(raw);
    enc.finish().unwrap_or_default()
}

/// The content of a `.kbdia` file.
#[derive(Debug, Clone, PartialEq)]
pub struct DiagramFile {
    /// `version` as stored (1).
    pub version: Value,
    /// The pages, in their stored order (a JSON object: the server's order, which sorts the ids). The
    /// server's page metadata, when known, reorders them ([`DiagramFile::order_by`]).
    pub pages: Vec<(String, PageData)>,
    /// Other top-level keys, kept.
    pub rest: Map<String, Value>,
}

impl DiagramFile {
    /// Reads the bytes of a `.kbdia` (gzipped or plain), of a page's bare data (`{ shapes, connectors }`,
    /// one page `page-1`), or of the web's « Export JSON ».
    pub fn read(bytes: &[u8]) -> Result<DiagramFile, FileError> {
        let raw = gunzip(bytes)?;
        let v: Value = serde_json::from_slice(&raw).map_err(|e| FileError::NotJson(e.to_string()))?;
        Self::from_value(&v)
    }

    pub fn from_value(v: &Value) -> Result<DiagramFile, FileError> {
        let o = v.as_object().ok_or(FileError::NotADiagram)?;
        // The web's export: pages as an array of records carrying their data.
        if let Some(arr) = o.get("pages").and_then(Value::as_array) {
            let mut pages = Vec::new();
            for p in arr {
                let id = p.get("id").and_then(Value::as_str).unwrap_or("").to_string();
                pages.push((id, PageData::from_value(p.get("data").unwrap_or(&Value::Null))));
            }
            return Ok(DiagramFile { version: Value::from(1), pages, rest: Map::new() });
        }
        if let Some(map) = o.get("pages").and_then(Value::as_object) {
            let pages = map.iter().map(|(id, d)| (id.clone(), PageData::from_value(d))).collect();
            let mut rest = o.clone();
            rest.remove("pages");
            let version = rest.remove("version").unwrap_or(Value::from(1));
            return Ok(DiagramFile { version, pages, rest });
        }
        if o.contains_key("shapes") || o.contains_key("connectors") {
            return Ok(DiagramFile { version: Value::from(1), pages: vec![("page-1".into(), PageData::from_value(v))], rest: Map::new() });
        }
        Err(FileError::NotADiagram)
    }

    /// The JSON the server stores.
    pub fn to_value(&self) -> Value {
        let mut o = self.rest.clone();
        o.insert("version".into(), self.version.clone());
        let mut pages = Map::new();
        for (id, d) in &self.pages {
            pages.insert(id.clone(), d.to_value());
        }
        o.insert("pages".into(), Value::Object(pages));
        Value::Object(o)
    }

    /// The `.kbdia` bytes (gzipped JSON).
    pub fn to_bytes(&self) -> Vec<u8> {
        gzip(&serde_json::to_vec(&self.to_value()).unwrap_or_default())
    }

    /// Puts the pages in the order of `ids` (the server's `position` order); pages it does not list keep
    /// their order after the listed ones.
    pub fn order_by(&mut self, ids: &[String]) {
        let rank = |id: &str| ids.iter().position(|x| x == id).unwrap_or(usize::MAX);
        self.pages.sort_by_key(|(id, _)| rank(id));
    }
}

/// The canonical bytes of a set of pages, for the save session's digest: `{ "<page id>": data }` with
/// sorted keys, so two reads of the same content compare equal whatever the server's formatting.
pub fn canonical_pages(pages: &[(String, PageData)]) -> Vec<u8> {
    let mut m = Map::new();
    for (id, d) in pages {
        m.insert(id.clone(), d.to_value());
    }
    serde_json::to_vec(&Value::Object(m)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_kbdia_round_trips_gzipped_and_reads_plain() {
        let v = json!({ "version": 1, "pages": { "p1": { "shapes": [], "connectors": [] } } });
        let bytes = gzip(&serde_json::to_vec(&v).expect("json"));
        let f = DiagramFile::read(&bytes).expect("reads");
        assert_eq!(f.pages.len(), 1);
        assert_eq!(f.to_value(), v);
        let again = DiagramFile::read(&f.to_bytes()).expect("reads its own bytes");
        assert_eq!(again, f);
        let plain = DiagramFile::read(serde_json::to_vec(&v).expect("json").as_slice()).expect("plain");
        assert_eq!(plain, f);
    }

    #[test]
    fn exports_and_bare_pages_read_too() {
        let export = json!({ "format": "kubuno-diagram/v1", "pages": [{ "id": "b", "name": "B", "data": { "shapes": [{ "id": "s" }], "connectors": [] } }] });
        let f = DiagramFile::from_value(&export).expect("export");
        assert_eq!(f.pages[0].0, "b");
        assert_eq!(f.pages[0].1.shapes.len(), 1);
        let bare = DiagramFile::from_value(&json!({ "shapes": [], "connectors": [] })).expect("bare");
        assert_eq!(bare.pages.len(), 1);
        assert_eq!(DiagramFile::from_value(&json!({ "type": "doc" })), Err(FileError::NotADiagram));
    }

    #[test]
    fn pages_follow_the_server_order() {
        let v = json!({ "version": 1, "pages": { "a": {}, "b": {}, "c": {} } });
        let mut f = DiagramFile::from_value(&v).expect("file");
        f.order_by(&["c".into(), "a".into()]);
        let ids: Vec<&str> = f.pages.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, ["c", "a", "b"]);
    }
}
