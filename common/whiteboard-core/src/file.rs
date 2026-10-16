//! The `.kbwbd` snapshot - the board's content as the Drive file holds it.
//!
//! The server stores the board's Yjs document as a gzipped binary snapshot in a Drive file
//! (`application/vnd.kubuno.whiteboard`, `server/src/services/content_files.rs:629-653`), plus a tail of
//! un-consolidated Yjs updates in Postgres (`office_wb.yjs_updates`). On open it returns the snapshot and
//! the updates as an ordered list of binary parts (`whiteboard/services/yjs_service.rs:24` `load_document`);
//! a client applies them in order to rebuild the document.
//!
//! This module gives the platform-neutral core the two gzip ends (read a snapshot, write one) and the
//! part-concatenation load, so a desktop app can open a local `.kbwbd` file offline and write one back
//! byte-for-byte the way the server would.

use std::io::{Read, Write};

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;

use crate::collab::{CollabError, WhiteboardDoc};

/// The extension and MIME the server uses for a board's content file.
pub const EXTENSION: &str = "kbwbd";
pub const MIME: &str = "application/vnd.kubuno.whiteboard";

/// An error reading or writing a snapshot.
#[derive(Debug)]
pub enum FileError {
    /// The gzip wrapper is malformed.
    Gzip(std::io::Error),
    /// The decompressed bytes are not a Yjs v1 update.
    Update(CollabError),
}

impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileError::Gzip(e) => write!(f, "gzip error: {e}"),
            FileError::Update(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for FileError {}

/// Gunzips a `.kbwbd` snapshot to the Yjs update bytes it wraps.
pub fn gunzip(bytes: &[u8]) -> Result<Vec<u8>, FileError> {
    // An empty file (a board created but never written) gunzips to nothing.
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    GzDecoder::new(bytes).read_to_end(&mut out).map_err(FileError::Gzip)?;
    Ok(out)
}

/// Gzips Yjs update bytes into a `.kbwbd` snapshot (default compression, as the server writes it).
pub fn gzip(update: &[u8]) -> Result<Vec<u8>, FileError> {
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    enc.write_all(update).map_err(FileError::Gzip)?;
    enc.finish().map_err(FileError::Gzip)
}

/// Rebuilds a document from a gzipped snapshot (the `.kbwbd` file). `client_id` is this client's - the
/// caller draws it; the core never reads a random source.
pub fn read_snapshot(client_id: u64, gzipped: &[u8]) -> Result<WhiteboardDoc, FileError> {
    let update = gunzip(gzipped)?;
    let doc = WhiteboardDoc::new(client_id);
    if !update.is_empty() {
        doc.apply_update(&update).map_err(FileError::Update)?;
    }
    Ok(doc)
}

/// Rebuilds a document from the ordered binary parts the server sends on open (`load_document`): the
/// snapshot first, then each un-consolidated update, applied in order.
pub fn load_from_parts(client_id: u64, parts: &[Vec<u8>]) -> Result<WhiteboardDoc, FileError> {
    let doc = WhiteboardDoc::new(client_id);
    for part in parts {
        if !part.is_empty() {
            doc.apply_update(part).map_err(FileError::Update)?;
        }
    }
    Ok(doc)
}

/// The document as a gzipped `.kbwbd` snapshot - what the server's `consolidate` writes back
/// (`yjs_service.rs:78`) and what the desktop writes to a local file.
pub fn write_snapshot(doc: &WhiteboardDoc) -> Result<Vec<u8>, FileError> {
    gzip(&doc.state_as_update())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn snapshot_round_trips_through_gzip_and_yjs() {
        let doc = WhiteboardDoc::new(7);
        doc.set_element("e1", &json!({"id":"e1","type":"shape","x":1,"y":2,"width":30,"height":40,"kind":"rect","fill":"#fff","stroke":"#000","strokeWidth":2,"zIndex":0}));
        doc.push_stroke(&json!({"id":"s1","points":[0.0,0.0,5.0,5.0],"color":"#111","width":2,"opacity":1,"tool":"pen","userId":"u","createdAt":1}));

        let snap = write_snapshot(&doc).unwrap();
        let back = read_snapshot(9, &snap).unwrap();

        assert_eq!(back.elements(), doc.elements());
        assert_eq!(back.strokes(), doc.strokes());
    }

    #[test]
    fn empty_file_is_an_empty_document() {
        let doc = read_snapshot(1, &[]).unwrap();
        assert_eq!(doc.elements_len(), 0);
        assert_eq!(doc.strokes_len(), 0);
    }
}
