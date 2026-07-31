//! Comment part parsing: legacy notes (xl/commentsN.xml) and modern threaded
//! comments (xl/threadedComments/*.xml + the workbook-level persons part).
//!
//! The internal cell model keeps a single plain-text note per cell (the `c`
//! key of CellData), so:
//!   - rich-text runs of a legacy note are flattened like shared strings
//!     (`<t>` fragments concatenated in document order, phonetic `<rPh>` runs
//!     excluded — LibreOffice oox/richstring.cxx does the same);
//!   - a comment thread is folded into one text, one "Author: message" block
//!     per entry, in document order (replies follow their parent in the part).
//!
//! When a cell carries both a legacy note and a thread, the thread wins: the
//! legacy note is only Excel's "[Threaded comment]" compatibility placeholder.
use std::collections::HashMap;

use quick_xml::events::Event;
use quick_xml::Reader;

use super::super::util::attr;

// Note texts may carry Windows line endings (raw CRLF inside `<t>`) — the
// internal model (textarea-edited plain text) uses `\n`.
fn normalize_newlines(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

/// Parse xl/commentsN.xml → ordered (cell ref, note text).
pub fn parse_comments(xml: &str) -> Vec<(String, String)> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false); // note text is free-form (newlines, indent)
    let mut out: Vec<(String, String)> = Vec::new();
    let mut cur_ref: Option<String> = None;
    let mut cur = String::new();
    let mut in_rph = false;
    let mut in_t = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.local_name().as_ref() {
                b"comment" => {
                    cur_ref = attr(&e, b"ref").filter(|r| !r.is_empty());
                    cur.clear();
                    in_rph = false;
                }
                b"rPh" if cur_ref.is_some() => in_rph = true,
                b"t" if cur_ref.is_some() && !in_rph => in_t = true,
                _ => {}
            },
            Ok(Event::Text(e)) if in_t => cur.push_str(&e.unescape().unwrap_or_default()),
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"t" => in_t = false,
                b"rPh" => in_rph = false,
                b"comment" => {
                    if let Some(r) = cur_ref.take() {
                        if !cur.trim().is_empty() { out.push((r, normalize_newlines(&cur))); }
                        cur.clear();
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

/// Parse xl/persons/person.xml → person id → display name.
pub fn parse_persons(xml: &str) -> HashMap<String, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut out = HashMap::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if e.local_name().as_ref() == b"person" => {
                if let (Some(id), Some(name)) = (attr(&e, b"id"), attr(&e, b"displayName")) {
                    out.insert(id, name);
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

/// One `<threadedComment>` message (document order preserved by the caller).
#[derive(Debug)]
pub struct ThreadedNote {
    pub cell:      String, // "A1"
    pub person_id: String,
    pub text:      String,
}

/// Parse xl/threadedComments/threadedCommentN.xml. Messages come in document
/// order (a reply follows its parent), which is the order we fold them in.
pub fn parse_threaded_comments(xml: &str) -> Vec<ThreadedNote> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut out: Vec<ThreadedNote> = Vec::new();
    let mut cur: Option<ThreadedNote> = None;
    let mut in_text = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.local_name().as_ref() {
                b"threadedComment" => {
                    cur = attr(&e, b"ref").filter(|r| !r.is_empty()).map(|cell| ThreadedNote {
                        cell,
                        person_id: attr(&e, b"personId").unwrap_or_default(),
                        text: String::new(),
                    });
                }
                b"text" if cur.is_some() => in_text = true,
                _ => {}
            },
            Ok(Event::Text(e)) if in_text => {
                if let Some(c) = cur.as_mut() { c.text.push_str(&e.unescape().unwrap_or_default()); }
            }
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"text" => in_text = false,
                b"threadedComment" => {
                    if let Some(mut c) = cur.take() {
                        if !c.text.trim().is_empty() {
                            c.text = normalize_newlines(&c.text);
                            out.push(c);
                        }
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

/// Fold legacy notes and threaded comments into the final ref → note list
/// (ordered: legacy notes in part order, then refs only seen in threads).
/// A thread replaces the legacy note of the same cell.
pub fn merge_comments(
    legacy: Vec<(String, String)>,
    threads: Vec<ThreadedNote>,
    persons: &HashMap<String, String>,
) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = legacy;
    let mut index: HashMap<String, usize> = out.iter().enumerate()
        .map(|(i, (r, _))| (r.clone(), i)).collect();

    // Per-ref folded thread text, in document order.
    let mut folded: Vec<(String, String)> = Vec::new(); // (ref, text) — first-seen ref order
    let mut fold_idx: HashMap<String, usize> = HashMap::new();
    for note in threads {
        let author = persons.get(&note.person_id).map(|s| s.as_str()).unwrap_or("");
        let block = if author.is_empty() { note.text.clone() } else { format!("{author}: {}", note.text) };
        match fold_idx.get(&note.cell) {
            Some(&i) => {
                folded[i].1.push('\n');
                folded[i].1.push_str(&block);
            }
            None => {
                fold_idx.insert(note.cell.clone(), folded.len());
                folded.push((note.cell, block));
            }
        }
    }
    for (r, text) in folded {
        match index.get(&r) {
            Some(&i) => out[i].1 = text, // thread wins over the legacy placeholder
            None => {
                index.insert(r.clone(), out.len());
                out.push((r, text));
            }
        }
    }
    out
}
