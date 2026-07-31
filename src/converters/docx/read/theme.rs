//! `theme1.xml` colours/fonts and the document-wide run defaults.

use std::collections::HashMap;

use roxmltree::{Document as XmlDoc, Node};

use crate::converters::docx::model::FontCtx;
use crate::converters::docx::xml::{attr_val, local};

/// Map a theme colour scheme (`<a:clrScheme>`) name → hex (`accent6` → `70AD47`).
pub(crate) fn parse_theme(xml: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let doc = match XmlDoc::parse(xml) {
        Ok(d) => d,
        Err(_) => return map,
    };
    if let Some(scheme) = doc.descendants().find(|n| local(n) == "clrScheme") {
        for slot in scheme.children().filter(|n| n.is_element()) {
            let name = local(&slot).to_string();
            // Every slot carries either an srgbClr (val) or a sysClr (lastClr).
            let hex = slot
                .children()
                .find(|n| n.is_element())
                .and_then(|c| match local(&c) {
                    "srgbClr" => attr_val(&c, "val"),
                    "sysClr" => attr_val(&c, "lastClr").or_else(|| attr_val(&c, "val")),
                    _ => None,
                });
            if let Some(h) = hex {
                map.insert(name, h);
            }
        }
    }
    map
}

/// Read the major/minor latin fonts of theme1.xml's `<a:fontScheme>`.
pub(crate) fn parse_theme_fonts(xml: &str) -> (String, String) {
    let doc = match XmlDoc::parse(xml) {
        Ok(d) => d,
        Err(_) => return (String::new(), String::new()),
    };
    let latin = |which: &str| -> String {
        doc.descendants()
            .find(|n| local(n) == which)
            .and_then(|n| n.children().find(|c| local(c) == "latin"))
            .and_then(|c| attr_val(&c, "typeface"))
            .unwrap_or_default()
    };
    (latin("majorFont"), latin("minorFont"))
}

/// Whether `styles.xml` declares a `<w:docDefaults><w:rPrDefault>` at all.
///
/// This decides the FALLBACK when no explicit default font or size is given.
/// Word 2007+ falls back to Calibri 11 pt — but only when `rPrDefault` is
/// ABSENT. As soon as it exists, the documented default applies instead
/// (Times New Roman 10 pt): LibreOffice sets Calibri 11 up front
/// (DomainMapper.cxx:182) and revokes it in StyleSheetTable.cxx:2163 (tdf#108350).
/// Applying Calibri unconditionally mis-renders every DOCX not produced by Word.
pub(crate) fn has_run_default(xml: &str) -> bool {
    XmlDoc::parse(xml).is_ok_and(|doc| {
        doc.descendants()
            .find(|n| local(n) == "docDefaults")
            .is_some_and(|dd| dd.descendants().any(|n| local(&n) == "rPrDefault"))
    })
}

/// Document default font: `<w:docDefaults><w:rPrDefault>…<w:rFonts>` — the
/// literal `w:ascii` name, else `w:asciiTheme` resolved through the theme.
pub(crate) fn parse_default_font(xml: &str, major: &str, minor: &str) -> Option<String> {
    let doc = XmlDoc::parse(xml).ok()?;
    let dd = doc.descendants().find(|n| local(n) == "docDefaults")?;
    let rfonts = dd
        .descendants()
        .find(|n| local(n) == "rPrDefault")
        .and_then(|rp| rp.descendants().find(|n| local(n) == "rFonts"))?;
    if let Some(f) = attr_val(&rfonts, "ascii").filter(|s| !s.is_empty()) {
        return Some(f);
    }
    let th = attr_val(&rfonts, "asciiTheme").or_else(|| attr_val(&rfonts, "hAnsiTheme"))?;
    let f = if th.starts_with("major") { major } else { minor };
    (!f.is_empty()).then(|| f.to_string())
}

/// Document default size: `<w:docDefaults>…<w:sz w:val="…"/>` (half-points → pt).
pub(crate) fn parse_default_size(xml: &str) -> Option<f64> {
    let doc = XmlDoc::parse(xml).ok()?;
    let dd = doc.descendants().find(|n| local(n) == "docDefaults")?;
    let sz = dd
        .descendants()
        .find(|n| local(n) == "rPrDefault")
        .and_then(|rp| rp.descendants().find(|n| local(n) == "sz"))?;
    attr_val(&sz, "val")
        .and_then(|v| v.parse::<f64>().ok())
        .map(|hp| hp / 2.0)
}

/// Resolve a run's font from its `<w:rFonts>`: the literal `ascii`/`hAnsi` name,
/// else the theme reference `asciiTheme`/`hAnsiTheme` (major*/minor* → theme).
pub(crate) fn resolve_run_font(m: &Node<'_, '_>, fonts: &FontCtx) -> Option<String> {
    if let Some(f) = attr_val(m, "ascii").or_else(|| attr_val(m, "hAnsi")).filter(|s| !s.is_empty()) {
        return Some(f);
    }
    let th = attr_val(m, "asciiTheme").or_else(|| attr_val(m, "hAnsiTheme"))?;
    let f = if th.starts_with("major") { &fonts.major } else { &fonts.minor };
    (!f.is_empty()).then(|| f.clone())
}
