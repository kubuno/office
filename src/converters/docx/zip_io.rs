//! Reading parts out of the DOCX package (a zip archive).

use std::collections::HashMap;
use std::io::Cursor;

use base64::Engine;
use zip::ZipArchive;

/// Read a zip entry as UTF-8 text; returns None if missing or unreadable.
pub(crate) fn read_zip_entry(archive: &mut ZipArchive<Cursor<&[u8]>>, name: &str) -> Option<String> {
    let mut file = archive.by_name(name).ok()?;
    let mut buf = String::new();
    std::io::Read::read_to_string(&mut file, &mut buf).ok()?;
    Some(buf)
}

/// Read a zip entry as raw bytes (images, …).
pub(crate) fn read_zip_bytes(archive: &mut ZipArchive<Cursor<&[u8]>>, name: &str) -> Option<Vec<u8>> {
    let mut file = archive.by_name(name).ok()?;
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut file, &mut buf).ok()?;
    Some(buf)
}

/// MIME type of an image, derived from its file extension.
pub(crate) fn image_mime(name: &str) -> Option<&'static str> {
    match name.rsplit('.').next().map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("png") => Some("image/png"),
        Some("jpg" | "jpeg") => Some("image/jpeg"),
        Some("gif") => Some("image/gif"),
        Some("bmp") => Some("image/bmp"),
        Some("webp") => Some("image/webp"),
        Some("svg") => Some("image/svg+xml"),
        Some("tif" | "tiff") => Some("image/tiff"),
        _ => None,
    }
}

/// Build an `rId → data-URL` map for the IMAGE relationships of a part
/// (document/header/footer): read every media blob out of the zip and encode it in
/// base64. Data-URLs travel inside the content (no dependency on an external file).
pub(crate) fn build_media_map(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    rels: &HashMap<String, String>,
    base_dir: &str,
) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (rid, target) in rels {
        let Some(mime) = image_mime(target) else { continue };
        // Target relative to the part's folder (word/), or absolute (/word/media/…).
        let part = if let Some(abs) = target.strip_prefix('/') {
            abs.to_string()
        } else {
            format!("{base_dir}/{}", target.trim_start_matches("./"))
        };
        if let Some(bytes) = read_zip_bytes(archive, &part) {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            map.insert(rid.clone(), format!("data:{mime};base64,{b64}"));
        }
    }
    map
}
