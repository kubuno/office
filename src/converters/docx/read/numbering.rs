//! `numbering.xml` - which `numId` is a bullet list and which is ordered.

use std::collections::HashMap;

use roxmltree::Document as XmlDoc;

use crate::converters::docx::xml::{attr_val, child, local};

/// Map numId → is the list ordered? Bullets and "none" formats are unordered.
pub(crate) fn parse_numbering(xml: &str) -> HashMap<String, bool> {
    let mut result = HashMap::new();
    let doc = match XmlDoc::parse(xml) {
        Ok(d) => d,
        Err(_) => return result,
    };

    // abstractNumId → ordered? (decided by the level-0 numFmt)
    let mut abstract_ordered: HashMap<String, bool> = HashMap::new();
    for an in doc.descendants().filter(|n| local(n) == "abstractNum") {
        let aid = match attr_val(&an, "abstractNumId") {
            Some(v) => v,
            None => continue,
        };
        let mut ordered = true;
        for lvl in an.descendants().filter(|n| local(n) == "lvl") {
            if attr_val(&lvl, "ilvl").as_deref() != Some("0") {
                continue;
            }
            if let Some(fmt) = child(&lvl, "numFmt").and_then(|n| attr_val(&n, "val")) {
                ordered = fmt != "bullet" && fmt != "none";
            }
            break;
        }
        abstract_ordered.insert(aid, ordered);
    }

    // numId → abstractNumId → ordered
    for num in doc.descendants().filter(|n| local(n) == "num") {
        let nid = match attr_val(&num, "numId") {
            Some(v) => v,
            None => continue,
        };
        if let Some(anid) = child(&num, "abstractNumId").and_then(|n| attr_val(&n, "val")) {
            let ordered = abstract_ordered.get(&anid).copied().unwrap_or(false);
            result.insert(nid, ordered);
        }
    }
    result
}
