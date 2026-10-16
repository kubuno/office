//! The open diagram: its pages (metadata and data) and where it came from, and the canonical form the
//! save session compares and saves.
//!
//! A diagram of the server is two things the web keeps apart: the pages' metadata (name, colour, order:
//! `office.diagram_pages`) and their data (the `.kbdia` file). The session works on ONE byte string, so
//! both are written in one canonical JSON - `{"pages":[{"id","name","bg_color","data"}…]}` in display
//! order, object keys sorted - which `GET /diagrams/:id/export/json` and `POST /editing/join` (+ the
//! metadata of `GET /diagrams/:id`) both produce for the same diagram. Saving it creates, renames,
//! reorders and deletes pages and writes the data of the pages that changed (`api::remote`), so a
//! page change made elsewhere is a conflict like any other, never overwritten silently.

use std::path::{Path, PathBuf};

use kubuno_office_diagrams_core::file::{DiagramFile, FileError, Page};
use kubuno_office_diagrams_core::model::PageData;
use serde_json::{json, Map, Value};

/// Where the open diagram lives.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Source {
    /// The built-in sample (nothing named on the command line).
    #[default]
    Sample,
    /// A local `.kbdia` (or JSON) file.
    File(PathBuf),
    /// A diagram of the server.
    Server(String),
}

/// An open diagram.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenDiagram {
    pub title: String,
    pub pages: Vec<Page>,
    pub source: Source,
}

impl OpenDiagram {
    /// The sample: one page with the web's « Organigramme » template.
    pub fn sample() -> OpenDiagram {
        let t = kubuno_office_diagrams_core::templates::TEMPLATES.iter().find(|t| t.id == "flowchart").map(|t| (t.build)()).unwrap_or_default();
        let data = PageData { shapes: t.shapes, connectors: t.connectors, layers: None, rest: Map::new() };
        OpenDiagram { title: "Diagramme".into(), pages: vec![Page::new("page-1", "Page 1", data)], source: Source::Sample }
    }

    /// Opens a local file: a `.kbdia`, its plain JSON, the web's « Export JSON », or the canonical form.
    pub fn open_file(path: &Path) -> Result<OpenDiagram, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let title = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Diagramme".into());
        let pages = match pages_from_canonical(&bytes) {
            Some(p) => p,
            None => {
                let file = DiagramFile::read(&bytes).map_err(|e: FileError| format!("{}: {e}", path.display()))?;
                pages_of_file(file, None)
            }
        };
        Ok(OpenDiagram { title, pages, source: Source::File(path.to_path_buf()) })
    }

    /// The local file's bytes (a gzipped `.kbdia` holding every page's data).
    pub fn to_kbdia(pages: &[Page]) -> Vec<u8> {
        let file = DiagramFile { version: Value::from(1), pages: pages.iter().map(|p| (p.id.clone(), p.data.clone())).collect(), rest: Map::new() };
        file.to_bytes()
    }
}

/// Pages of a `.kbdia` (no metadata: « Page N »), or of an export (names and colours known).
pub fn pages_of_file(file: DiagramFile, meta: Option<&[PageMeta]>) -> Vec<Page> {
    let mut file = file;
    if let Some(meta) = meta {
        file.order_by(&meta.iter().map(|m| m.id.clone()).collect::<Vec<_>>());
    }
    file.pages
        .into_iter()
        .enumerate()
        .map(|(i, (id, data))| {
            let m = meta.and_then(|m| m.iter().find(|p| p.id == id));
            let mut p = Page::new(&id, &m.map(|m| m.name.clone()).unwrap_or_else(|| format!("Page {}", i + 1)), data);
            if let Some(m) = m {
                p.bg_color = m.bg_color.clone();
                p.position = m.position;
            }
            p
        })
        .collect()
}

/// A page's metadata as the server keeps it (`DiagramPage`).
#[derive(Debug, Clone, PartialEq)]
pub struct PageMeta {
    pub id: String,
    pub name: String,
    pub bg_color: String,
    pub position: i64,
}

impl PageMeta {
    pub fn from_value(v: &Value) -> Option<PageMeta> {
        Some(PageMeta {
            id: v.get("id")?.as_str()?.to_string(),
            name: v.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
            bg_color: v.get("bg_color").and_then(Value::as_str).unwrap_or("#ffffff").to_string(),
            position: v.get("position").and_then(Value::as_i64).unwrap_or(0),
        })
    }
}

/// The canonical bytes of a diagram's pages, in display order (see the module doc).
pub fn canonical(pages: &[Page]) -> Vec<u8> {
    let list: Vec<Value> = pages.iter().map(|p| json!({ "id": p.id, "name": p.name, "bg_color": p.bg_color, "data": p.data.to_value() })).collect();
    serde_json::to_vec(&json!({ "pages": list })).unwrap_or_default()
}

/// The pages of canonical bytes, `None` when they are not in that form.
pub fn pages_from_canonical(bytes: &[u8]) -> Option<Vec<Page>> {
    let v: Value = serde_json::from_slice(bytes).ok()?;
    let o = v.as_object()?;
    if o.len() != 1 {
        return None;
    }
    let list = o.get("pages")?.as_array()?;
    let mut pages = Vec::new();
    for (i, p) in list.iter().enumerate() {
        let id = p.get("id")?.as_str()?;
        let name = p.get("name").and_then(Value::as_str).unwrap_or("");
        let mut page = Page::new(id, name, PageData::from_value(p.get("data").unwrap_or(&Value::Null)));
        page.bg_color = p.get("bg_color").and_then(Value::as_str).unwrap_or("#ffffff").to_string();
        page.position = i as i64;
        pages.push(page);
    }
    Some(pages)
}

/// The canonical form of what `GET /diagrams/:id/export/json` returns.
pub fn canonical_of_export(export: &Value) -> Option<Vec<u8>> {
    let list = export.get("pages")?.as_array()?;
    let mut pages = Vec::new();
    for p in list {
        let meta = PageMeta::from_value(p)?;
        let mut page = Page::new(&meta.id, &meta.name, PageData::from_value(p.get("data").unwrap_or(&Value::Null)));
        page.bg_color = meta.bg_color;
        page.position = meta.position;
        pages.push(page);
    }
    pages.sort_by_key(|p| p.position);
    Some(canonical(&pages))
}

/// The canonical form of a joined session's content (`{ version, pages: { id: data } }`) and the pages'
/// metadata (`GET /diagrams/:id`'s `pages`).
pub fn canonical_of_content(content: &Value, meta: &[PageMeta]) -> Option<Vec<u8>> {
    let file = DiagramFile::from_value(content).ok()?;
    let mut sorted = meta.to_vec();
    sorted.sort_by_key(|m| m.position);
    // A page listed by the database but absent from the file reads as an empty page (`get_page_data`).
    let mut pages = Vec::new();
    for m in &sorted {
        let data = file.pages.iter().find(|(id, _)| *id == m.id).map(|(_, d)| d.clone()).unwrap_or_default();
        let mut page = Page::new(&m.id, &m.name, data);
        page.bg_color = m.bg_color.clone();
        page.position = m.position;
        pages.push(page);
    }
    Some(canonical(&pages))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_canonical_form_round_trips_and_ignores_the_servers_formatting() {
        let s = OpenDiagram::sample();
        let bytes = canonical(&s.pages);
        let back = pages_from_canonical(&bytes).expect("canonical");
        assert_eq!(back.len(), 1);
        assert_eq!(canonical(&back), bytes);
        let export = json!({ "format": "kubuno-diagram/v1", "pages": [
            { "id": "b", "name": "Deux", "position": 1, "bg_color": "#ffffff", "data": { "connectors": [], "shapes": [] } },
            { "id": "a", "name": "Un", "position": 0, "bg_color": "#ffffff", "data": { "shapes": [], "connectors": [] } }
        ] });
        let from_export = canonical_of_export(&export).expect("export");
        let content = json!({ "version": 1, "pages": { "a": { "shapes": [], "connectors": [] }, "b": { "shapes": [], "connectors": [] } } });
        let meta = [PageMeta { id: "a".into(), name: "Un".into(), bg_color: "#ffffff".into(), position: 0 }, PageMeta { id: "b".into(), name: "Deux".into(), bg_color: "#ffffff".into(), position: 1 }];
        assert_eq!(canonical_of_content(&content, &meta).expect("content"), from_export, "join and export agree");
        let pages = pages_from_canonical(&from_export).expect("pages");
        assert_eq!((pages[0].name.as_str(), pages[1].name.as_str()), ("Un", "Deux"));
    }

    #[test]
    fn a_local_kbdia_opens_and_saves() {
        let dir = std::env::temp_dir().join(format!("kubuno-diagrams-doc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("Essai.kbdia");
        let sample = OpenDiagram::sample();
        std::fs::write(&path, OpenDiagram::to_kbdia(&sample.pages)).expect("write");
        let d = OpenDiagram::open_file(&path).expect("open");
        assert_eq!(d.title, "Essai");
        assert_eq!(d.pages[0].data, sample.pages[0].data);
        let _ = std::fs::remove_dir_all(dir);
    }
}
