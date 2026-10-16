//! The open board: its title, its background and its Yjs document, and where it came from.
//!
//! Unlike a document or a diagram, a board's content is not a JSON blob saved by a digest-guarded PATCH:
//! it is a Yjs CRDT document (`kubuno_office_whiteboard_core::collab::WhiteboardDoc`) synced over a
//! WebSocket and consolidated into a gzipped `.kbwbd` Drive file (vskubuno `docs/WHITEBOARD-DESKTOP.md`
//! §2). So an open board owns its `WhiteboardDoc`; a local file is read from / written to the `.kbwbd`
//! snapshot, and a server board is filled by the collaboration room ([`crate::collab`]).

use std::path::{Path, PathBuf};

use kubuno_office_whiteboard_core::collab::WhiteboardDoc;
use kubuno_office_whiteboard_core::engine::scene_from_doc;
use kubuno_office_whiteboard_core::model::Scene;
use kubuno_office_whiteboard_core::{file, Viewport};
use serde_json::json;

/// Where the open board lives.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Source {
    /// The built-in sample (nothing named on the command line).
    #[default]
    Sample,
    /// A local `.kbwbd` snapshot file.
    File(PathBuf),
    /// A board of the server, filled by the collaboration room.
    Server(String),
}

/// An open board: the title, the background and the live Yjs document.
pub struct OpenBoard {
    pub title: String,
    /// `white | grid | dots | lines` - board metadata, not content.
    pub background: String,
    pub doc: WhiteboardDoc,
    pub source: Source,
    /// The viewport is local (never persisted), as on the web.
    pub viewport: Viewport,
}

impl OpenBoard {
    /// The sample board: one sticky note and one shape on a dotted canvas, so a first launch shows
    /// something recognisable without a server.
    pub fn sample() -> OpenBoard {
        let doc = WhiteboardDoc::new(crate::collab::new_client_id());
        // One of each element kind, so a first launch shows the renderer's coverage without a server.
        doc.set_element(
            "sample-sticky",
            &json!({"id":"sample-sticky","type":"sticky","x":120.0,"y":140.0,"width":180.0,"height":140.0,"rotation":0.0,"opacity":1.0,"zIndex":0,"locked":false,"text":"Bienvenue sur le tableau","color":"yellow","fontSize":16.0,"textAlign":"left"}),
        );
        doc.set_element(
            "sample-shape",
            &json!({"id":"sample-shape","type":"shape","x":420.0,"y":160.0,"width":220.0,"height":120.0,"rotation":0.0,"opacity":1.0,"zIndex":1,"locked":false,"kind":"roundRect","fill":"#BBDEFB","stroke":"#1a73e8","strokeWidth":2.0}),
        );
        doc.set_element(
            "sample-ellipse",
            &json!({"id":"sample-ellipse","type":"shape","x":440.0,"y":360.0,"width":150.0,"height":150.0,"rotation":0.0,"opacity":1.0,"zIndex":2,"locked":false,"kind":"ellipse","fill":"#C8E6C9","stroke":"#2e7d32","strokeWidth":2.0}),
        );
        doc.set_element(
            "sample-text",
            &json!({"id":"sample-text","type":"text","x":120.0,"y":320.0,"width":240.0,"height":40.0,"rotation":0.0,"opacity":1.0,"zIndex":3,"locked":false,"text":"Un tableau blanc collaboratif","fontSize":20.0,"fontWeight":"bold","color":"#202124","textAlign":"left"}),
        );
        doc.set_element(
            "sample-arrow",
            &json!({"id":"sample-arrow","type":"arrow","startX":300.0,"startY":200.0,"endX":420.0,"endY":210.0,"color":"#444746","width":2.0,"style":"straight","startArrow":"none","endArrow":"triangle","zIndex":4,"opacity":1.0}),
        );
        doc.set_element(
            "sample-frame",
            &json!({"id":"sample-frame","type":"frame","x":90.0,"y":110.0,"width":560.0,"height":430.0,"rotation":0.0,"opacity":1.0,"zIndex":5,"locked":false,"title":"Idées","color":"#1a73e8"}),
        );
        doc.push_stroke(
            &json!({"id":"sample-stroke","points":[700.0,180.0,720.0,160.0,760.0,220.0,800.0,160.0,840.0,240.0],"color":"#d81b60","width":3.0,"opacity":1.0,"tool":"pen","userId":"sample","createdAt":0.0}),
        );
        OpenBoard { title: "Tableau blanc".into(), background: "dots".into(), doc, source: Source::Sample, viewport: Viewport::new() }
    }

    /// Opens a local `.kbwbd` snapshot. Its background is not stored in the file (it is board metadata), so
    /// it opens on the default `dots`.
    pub fn open_file(path: &Path) -> Result<OpenBoard, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let doc = file::read_snapshot(crate::collab::new_client_id(), &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        let title = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Tableau blanc".into());
        Ok(OpenBoard { title, background: "dots".into(), doc, source: Source::File(path.to_path_buf()), viewport: Viewport::new() })
    }

    /// An empty board for a server id: the room fills it once it connects (`crate::collab`).
    pub fn for_server(id: &str, title: &str, background: &str) -> OpenBoard {
        OpenBoard {
            title: title.to_string(),
            background: background.to_string(),
            doc: WhiteboardDoc::new(crate::collab::new_client_id()),
            source: Source::Server(id.to_string()),
            viewport: Viewport::new(),
        }
    }

    /// The scene the renderer and the engine work on (derived from the document; unknown element kinds are
    /// skipped but stay in the document).
    pub fn scene(&self) -> Scene {
        scene_from_doc(&self.doc, &self.background)
    }

    /// The local `.kbwbd` snapshot bytes (the gzipped Yjs state) - the form a local file is written in.
    pub fn to_kbwbd(&self) -> Vec<u8> {
        file::write_snapshot(&self.doc).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sample_has_one_of_each_element_kind() {
        let b = OpenBoard::sample();
        let scene = b.scene();
        assert_eq!(scene.elements.len(), 6);
        assert_eq!(scene.strokes.len(), 1);
        assert_eq!(b.background, "dots");
    }

    #[test]
    fn a_local_kbwbd_round_trips() {
        let dir = std::env::temp_dir().join(format!("kubuno-whiteboard-doc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("Essai.kbwbd");
        let sample = OpenBoard::sample();
        std::fs::write(&path, sample.to_kbwbd()).expect("write");
        let b = OpenBoard::open_file(&path).expect("open");
        assert_eq!(b.title, "Essai");
        assert_eq!(b.scene().elements.len(), sample.scene().elements.len());
        let _ = std::fs::remove_dir_all(dir);
    }
}
