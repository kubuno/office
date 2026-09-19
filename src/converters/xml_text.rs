//! Character-data helpers shared by every XML importer.
//!
//! Since quick-xml 0.41 the reader no longer resolves entity references inside
//! text: `<t>a &amp; b</t>` is reported as `Text("a ")`, `GeneralRef("amp")`,
//! `Text(" b")`. Document text therefore has to be reassembled from both event
//! kinds, and this module is the single place where that is done so that no
//! importer can silently drop the characters an entity stood for.

use quick_xml::events::{BytesRef, BytesText};

/// Character data of a `Text` event: decoded, line-end normalised (XML 1.0) and
/// with any remaining reference resolved.
///
/// A malformed reference falls back to the decoded form rather than discarding
/// the whole node.
pub fn text_content(e: &BytesText) -> String {
    match e.xml10_content() {
        Ok(decoded) => match quick_xml::escape::unescape(&decoded) {
            Ok(unescaped) => unescaped.into_owned(),
            Err(_) => decoded.into_owned(),
        },
        Err(_) => String::new(),
    }
}

/// Character data a `GeneralRef` event stands for: `amp` → `&`, `#10` → newline.
///
/// An entity the XML predefines nothing for is kept in its source form
/// (`&name;`) instead of vanishing from the document.
pub fn ref_content(e: &BytesRef) -> String {
    match e.decode() {
        Ok(name) => {
            let source = format!("&{name};");
            match quick_xml::escape::unescape(&source) {
                Ok(resolved) => resolved.into_owned(),
                Err(_) => source,
            }
        }
        Err(_) => String::new(),
    }
}
