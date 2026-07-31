//! Shared string table (xl/sharedStrings.xml) parsing.
//!
//! A `<si>` entry is either a plain `<t>` or a sequence of rich-text runs
//! `<r><rPr>…</rPr><t>…</t></r>`. The internal model has a single text value
//! and a single style per cell (the cell keeps its `xf` base style), so runs
//! are flattened: their `<t>` fragments are concatenated in document order and
//! the per-run formatting (`<rPr>`) is dropped.
//!
//! Phonetic runs `<rPh>` carry East-Asian ruby readings of parts of the text —
//! their `<t>` children are *not* part of the visible string and are excluded
//! (LibreOffice oox/richstring.cxx does the same).
use quick_xml::events::Event;
use quick_xml::Reader;

pub fn parse_shared_strings(xml: &str) -> Vec<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_si = false;
    let mut in_rph = false; // inside a phonetic run — skip its text
    let mut in_t = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match e.local_name().as_ref() {
                b"si" => { cur.clear(); in_si = true; in_rph = false; }
                b"rPh" if in_si => in_rph = true,
                b"t" if in_si && !in_rph => in_t = true,
                _ => {}
            },
            Ok(Event::Text(e)) if in_t => cur.push_str(&e.unescape().unwrap_or_default()),
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"t" => in_t = false,
                b"rPh" => in_rph = false,
                b"si" => { out.push(std::mem::take(&mut cur)); in_si = false; }
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}
