//! `styles.xml` - paragraph styles, inheritance and the resolved cascade.

use std::collections::HashMap;

use roxmltree::{Document as XmlDoc, Node};
use serde_json::{json, Value};

use crate::converters::docx::xml::{attr_val, bool_on, child, local};
use crate::converters::docx::model::FontCtx;
use super::paragraph::parse_ppr_attrs;

/// The document's DEFAULT paragraph properties (`<w:docDefaults><w:pPrDefault>
/// <w:pPr>`) — e.g. Word applies "After 8 pt" + 1.16 line spacing to EVERY paragraph.
pub(crate) fn parse_para_default(styles_xml: &str) -> serde_json::Map<String, Value> {
    XmlDoc::parse(styles_xml)
        .ok()
        .and_then(|d| {
            d.descendants()
                .find(|n| local(n) == "pPrDefault")
                .and_then(|pd| child(&pd, "pPr").map(|p| parse_ppr_attrs(&p)))
        })
        .unwrap_or_default()
}

/// Paragraph styles (`<w:style w:type="paragraph">`) → RESOLVED pPr (the
/// `<w:basedOn>` chain flattened, root→leaf) by styleId, plus the default styleId
/// (`w:default="1"`). Lets spacing/line height/indents be inherited from the named style.
// Heading level deduced from the style NAME ("heading 2", "title") — robust to the
// localisation of the styleId (Titre2 / Überschrift2 / Heading2…).
pub(crate) fn name_to_heading_level(name: &str) -> Option<u8> {
    let s = name.trim().to_lowercase();
    if s == "title" {
        return Some(1);
    }
    s.strip_prefix("heading")
        .map(|r| r.trim())
        .and_then(|r| r.parse::<u8>().ok())
        .filter(|l| (1..=6).contains(l))
}

#[allow(clippy::type_complexity)]
// RUN properties of a style (bold/italic/underline/strike/size/colour/font).
pub(crate) fn parse_style_rpr(rpr: &Node<'_, '_>) -> serde_json::Map<String, Value> {
    let mut m = serde_json::Map::new();
    for c in rpr.children().filter(|n| n.is_element()) {
        match local(&c) {
            "b" => { m.insert("bold".into(), json!(bool_on(&c))); }
            "i" => { m.insert("italic".into(), json!(bool_on(&c))); }
            "u" => { m.insert("underline".into(), json!(attr_val(&c, "val").as_deref() != Some("none"))); }
            "strike" => { m.insert("strike".into(), json!(bool_on(&c))); }
            "sz" => {
                if let Some(v) = attr_val(&c, "val").and_then(|v| v.parse::<f64>().ok()) {
                    m.insert("fontSize".into(), json!(v / 2.0));
                }
            }
            "color" => {
                if let Some(v) = attr_val(&c, "val") {
                    if v != "auto" {
                        m.insert("color".into(), json!(format!("#{}", v.trim_start_matches('#'))));
                    }
                }
            }
            "rFonts" => {
                if let Some(f) = attr_val(&c, "ascii").or_else(|| attr_val(&c, "hAnsi")) {
                    m.insert("fontFamily".into(), json!(f));
                }
            }
            _ => {}
        }
    }
    m
}

#[allow(clippy::type_complexity)]
pub(crate) fn parse_para_styles(
    styles_xml: &str,
) -> (
    HashMap<String, serde_json::Map<String, Value>>,
    Option<String>,
    HashMap<String, u8>,
    HashMap<String, serde_json::Map<String, Value>>,
) {
    let doc = match XmlDoc::parse(styles_xml) {
        Ok(d) => d,
        Err(_) => return (HashMap::new(), None, HashMap::new(), HashMap::new()),
    };
    // styleId → (basedOn, own pPr, own rPr)
    let mut raw: HashMap<String, (Option<String>, serde_json::Map<String, Value>, serde_json::Map<String, Value>)> = HashMap::new();
    let mut default_id = None;
    let mut heading_levels: HashMap<String, u8> = HashMap::new();
    for st in doc.descendants().filter(|n| local(n) == "style") {
        if attr_val(&st, "type").as_deref() != Some("paragraph") {
            continue;
        }
        let Some(id) = attr_val(&st, "styleId") else { continue };
        if matches!(attr_val(&st, "default").as_deref(), Some("1") | Some("true")) {
            default_id = Some(id.clone());
        }
        // Heading level from the style name (falling back to the English styleId).
        if let Some(lvl) = child(&st, "name")
            .and_then(|n| attr_val(&n, "val"))
            .and_then(|n| name_to_heading_level(&n))
            .or_else(|| heading_level(&id))
        {
            heading_levels.insert(id.clone(), lvl);
        }
        let based = child(&st, "basedOn").and_then(|b| attr_val(&b, "val"));
        let ppr = child(&st, "pPr").map(|p| parse_ppr_attrs(&p)).unwrap_or_default();
        let rpr = child(&st, "rPr").map(|p| parse_style_rpr(&p)).unwrap_or_default();
        raw.insert(id, (based, ppr, rpr));
    }
    // Flatten every basedOn chain (root first, the leaf overwrites) for both pPr AND rPr.
    let mut resolved = HashMap::new();
    let mut run_resolved = HashMap::new();
    for id in raw.keys() {
        let mut chain = Vec::new();
        let mut cur = Some(id.clone());
        let mut guard = 0;
        while let Some(c) = cur {
            if !raw.contains_key(&c) || chain.contains(&c) || guard > 25 {
                break;
            }
            chain.push(c.clone());
            cur = raw[&c].0.clone();
            guard += 1;
        }
        let mut m = serde_json::Map::new();
        let mut rm = serde_json::Map::new();
        for c in chain.iter().rev() {
            for (k, v) in &raw[c].1 {
                m.insert(k.clone(), v.clone());
            }
            for (k, v) in &raw[c].2 {
                rm.insert(k.clone(), v.clone());
            }
        }
        resolved.insert(id.clone(), m);
        run_resolved.insert(id.clone(), rm);
    }
    (resolved, default_id, heading_levels, run_resolved)
}

/// EFFECTIVE paragraph properties = docDefaults < named style (or default) < direct
/// pPr. `direct` is consumed (explicit values win).
pub(crate) fn effective_para_attrs(
    fonts: &FontCtx,
    style_id: &str,
    direct: serde_json::Map<String, Value>,
) -> serde_json::Map<String, Value> {
    let mut eff = fonts.para_default.clone();
    let sid = if !style_id.is_empty() {
        Some(style_id.to_string())
    } else {
        fonts.para_style_default.clone()
    };
    if let Some(sid) = sid {
        if let Some(sm) = fonts.para_styles.get(&sid) {
            for (k, v) in sm {
                eff.insert(k.clone(), v.clone());
            }
        }
    }
    for (k, v) in direct {
        eff.insert(k, v);
    }
    eff
}

/// "Heading1".."Heading6" / "Heading 2" / "Title" → ProseMirror heading level.
pub(crate) fn heading_level(style: &str) -> Option<u8> {
    let s = style.to_lowercase().replace(' ', "");
    if s == "title" {
        return Some(1);
    }
    s.strip_prefix("heading")
        .and_then(|rest| rest.parse::<u8>().ok())
        .filter(|l| (1..=6).contains(l))
}
