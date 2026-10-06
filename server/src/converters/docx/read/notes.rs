//! `word/footnotes.xml` and `word/endnotes.xml` on import.
//!
//! The body only holds a REFERENCE (`w:footnoteReference w:id`); the text lives
//! in a separate part. Two traps:
//!
//! - the first entries are NOT notes: `w:type="separator"` and
//!   `"continuationSeparator"` carry the ruling line, and Word numbers them
//!   `-1`/`0` while LibreOffice uses `0`/`1`. Filter on the TYPE, never on the id
//!   (LibreOffice: DomainMapper.cxx:1446 `SkipFootnoteSeparator`);
//! - ids are neither ordered nor contiguous, so the mapping must be by id and
//!   the READING order comes from the body (`lcl_convertToNoteIndices`,
//!   DomainMapper_Impl.cxx:4723).

use std::collections::HashMap;

use roxmltree::{Document as XmlDoc, Node};

use crate::converters::docx::read::revisions;
use crate::converters::docx::xml::{attr_val, local};

/// Note texts by id, for both kinds.
#[derive(Default)]
pub(crate) struct NoteTexts {
    pub(crate) footnotes: HashMap<String, String>,
    pub(crate) endnotes: HashMap<String, String>,
}

impl NoteTexts {
    pub(crate) fn get(&self, endnote: bool, id: &str) -> Option<&String> {
        if endnote {
            self.endnotes.get(id)
        } else {
            self.footnotes.get(id)
        }
    }
}

/// Flatten the runs of a note into plain text — the shape our model holds.
/// Reference marks (`w:footnoteRef`, `w:endnoteRef`) are structure, not text.
fn note_text(container: &Node<'_, '_>) -> String {
    let mut paras: Vec<String> = Vec::new();
    for p in container.children().filter(|n| n.is_element() && local(n) == "p") {
        let mut s = String::new();
        for t in p.descendants().filter(|n| n.is_element()) {
            match local(&t) {
                // `w:delText` too: tracked deletions inside a note still carry
                // text. Our model holds a note as plain text, so the revision
                // itself is lost, but dropping the characters would silently
                // truncate the note.
                x if revisions::is_run_text(x) => s.push_str(t.text().unwrap_or("")),
                "tab" => s.push('\t'),
                "br" => s.push('\n'),
                _ => {}
            }
        }
        paras.push(s.trim_start().to_string());
    }
    paras.join("\n").trim().to_string()
}

/// Parse a notes part. `tag` is `footnote` or `endnote`.
pub(crate) fn parse_notes_part(xml: &str, tag: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Ok(doc) = XmlDoc::parse(xml) else { return map };
    for n in doc.root_element().children().filter(|n| n.is_element()) {
        if local(&n) != tag {
            continue;
        }
        // Anything other than a normal note is the separator machinery.
        if let Some(t) = attr_val(&n, "type") {
            if t != "normal" {
                continue;
            }
        }
        let Some(id) = attr_val(&n, "id") else { continue };
        // Belt and braces: some producers omit `w:type` on the separators.
        if id.parse::<i64>().map(|v| v < 0).unwrap_or(false) {
            continue;
        }
        map.insert(id, note_text(&n));
    }
    map
}
