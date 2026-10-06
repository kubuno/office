//! `<w:p>` and `<w:pPr>` - paragraph structure and formatting.

use std::collections::HashMap;

use roxmltree::Node;
use serde_json::{json, Value};

use crate::converters::docx::model::{Block, FontCtx};
use crate::converters::docx::xml::{attr_val, bool_on, child};
use crate::converters::types::PmNode;

use super::revisions;
use super::run::parse_inline;
use super::styles::{effective_para_attrs, heading_level};
use super::fields::FieldState;
use super::toc;
use super::notes::NoteTexts;

#[allow(clippy::too_many_arguments)]
pub(crate) fn parse_paragraph(
    p: &Node<'_, '_>,
    rels: &HashMap<String, String>,
    numbering: &HashMap<String, bool>,
    fonts: &FontCtx,
    theme: &HashMap<String, String>,
    media: &HashMap<String, String>,
    fs: &mut FieldState,
    notes: &NoteTexts,
) -> Block {
    let mut style = String::new();
    let mut num: Option<(String, u8)> = None;
    // Every paragraph property (`<w:pPr>`) → editor attributes (alignment,
    // indents, spacing, line height, pagination flags…).
    let ppr_attrs = if let Some(ppr) = child(p, "pPr") {
        if let Some(ps) = child(&ppr, "pStyle") {
            style = attr_val(&ps, "val").unwrap_or_default();
        }
        if let Some(numpr) = child(&ppr, "numPr") {
            let numid = child(&numpr, "numId").and_then(|n| attr_val(&n, "val"));
            let ilvl = child(&numpr, "ilvl")
                .and_then(|n| attr_val(&n, "val"))
                .and_then(|v| v.parse().ok())
                .unwrap_or(0u8);
            if let Some(id) = numid {
                num = Some((id, ilvl));
            }
        }
        let mut a = parse_ppr_attrs(&ppr);
        // Tracked change on the paragraph MARK itself (`<w:pPr><w:rPr><w:ins|w:del>`):
        // an inserted mark is a paragraph SPLIT made while tracking, a deleted one a
        // MERGE with the paragraph that follows. It belongs to the paragraph, not to
        // any run, hence a node attribute rather than a mark.
        if let Some((key, value)) = revisions::para_mark_attr(&ppr) {
            a.insert(key.into(), value);
        }
        a
    } else {
        serde_json::Map::new()
    };

    // RUN properties inherited from the paragraph style (bold/italic/size/font/
    // colour) — applied as the base of every run, overridden by the direct rPr.
    let style_eff = if style.is_empty() {
        fonts.para_style_default.clone().unwrap_or_default()
    } else {
        style.clone()
    };
    let empty_run = serde_json::Map::new();
    let style_run = fonts.run_styles.get(&style_eff).unwrap_or(&empty_run);
    let mut inline = parse_inline(p, rels, fonts, theme, media, style_run, fs, notes);
    // EFFECTIVE properties = docDefaults < named style (or default style) < direct pPr.
    let mut eff = effective_para_attrs(fonts, &style, ppr_attrs);

    // Table of contents. The TITLE sits before the field and is betrayed only by
    // its style; the ENTRIES carry their page number in a trailing PAGEREF that
    // must not stay as text — the canvas repaints it from `tocPage`.
    if toc::is_toc_heading_style(&style) {
        toc::mark_toc_title(&mut eff);
        let mut n = PmNode::heading(toc::TOC_TITLE_LEVEL, inline);
        apply_attrs(&mut n, eff);
        return Block::Node(n);
    }
    if toc::apply_toc_entry(p, &style, fonts, fs, &mut inline, &mut eff) {
        let mut para = PmNode::paragraph(inline);
        apply_attrs(&mut para, eff);
        return Block::Node(para);
    }

    // List item: defer wrapping into a list to fold_blocks.
    if let Some((numid, ilvl)) = num {
        let ordered = numbering.get(&numid).copied().unwrap_or(false);
        let mut para = PmNode::paragraph(inline);
        apply_attrs(&mut para, eff);
        return Block::Item { ordered, ilvl, para };
    }

    // Heading / block style. Level resolved through the style NAME (locale-robust:
    // Titre2/Überschrift2/…), falling back to the English styleId.
    let lvl = fonts
        .heading_levels
        .get(&style)
        .copied()
        .or_else(|| heading_level(&style));
    let node = if let Some(level) = lvl {
        let mut n = PmNode::heading(level, inline);
        apply_attrs(&mut n, eff);
        n
    } else if style.eq_ignore_ascii_case("Quote") || style.eq_ignore_ascii_case("IntenseQuote") {
        let mut pp = PmNode::paragraph(inline);
        apply_attrs(&mut pp, eff);
        PmNode::blockquote(vec![pp])
    } else if style == "Code" || style.eq_ignore_ascii_case("HTMLPreformatted") {
        let text: String = inline
            .iter()
            .filter_map(|n| n.text.as_deref())
            .collect::<Vec<_>>()
            .join("");
        PmNode::code_block(text)
    } else {
        let mut n = PmNode::paragraph(inline);
        apply_attrs(&mut n, eff);
        n
    };
    Block::Node(node)
}

/// `<w:pPr>` → the editor's paragraph attributes. DOCX indents/spacing are in twips
/// (1440/inch = 96 px ⇒ ÷15). Line spacing: `lineRule="auto"` is a multiplier
/// (240 = single), otherwise (`atLeast`/`exact`) an absolute value in px.
pub(crate) fn parse_ppr_attrs(ppr: &Node<'_, '_>) -> serde_json::Map<String, Value> {
    const TWIP_PER_PX: f64 = 15.0;
    let mut m = serde_json::Map::new();
    let twip_px = |v: Option<String>| v.and_then(|s| s.parse::<f64>().ok()).map(|t| (t / TWIP_PER_PX).round());

    if let Some(a) = child(ppr, "jc").and_then(|jc| attr_val(&jc, "val")).as_deref().and_then(map_align) {
        m.insert("textAlign".into(), json!(a));
    }
    if let Some(ind) = child(ppr, "ind") {
        if let Some(px) = twip_px(attr_val(&ind, "left").or_else(|| attr_val(&ind, "start"))) {
            if px != 0.0 { m.insert("indentLeft".into(), json!(px)); }
        }
        if let Some(px) = twip_px(attr_val(&ind, "right").or_else(|| attr_val(&ind, "end"))) {
            if px != 0.0 { m.insert("indentRight".into(), json!(px)); }
        }
        // A positive first line; hanging = a negative first-line indent.
        if let Some(fl) = twip_px(attr_val(&ind, "firstLine")).filter(|v| *v != 0.0) {
            m.insert("indentFirstLine".into(), json!(fl));
        } else if let Some(hg) = twip_px(attr_val(&ind, "hanging")).filter(|v| *v != 0.0) {
            m.insert("indentFirstLine".into(), json!(-hg));
        }
    }
    if let Some(sp) = child(ppr, "spacing") {
        if let Some(px) = twip_px(attr_val(&sp, "before")) {
            m.insert("spaceBefore".into(), json!(px));
        }
        if let Some(px) = twip_px(attr_val(&sp, "after")) {
            m.insert("spaceAfter".into(), json!(px));
        }
        if let Some(line) = attr_val(&sp, "line").and_then(|v| v.parse::<f64>().ok()) {
            match attr_val(&sp, "lineRule").as_deref() {
                Some("exact") | Some("exactly") => {
                    m.insert("lineSpacingMode".into(), json!("exactly"));
                    m.insert("lineSpacingPt".into(), json!((line / TWIP_PER_PX).round()));
                }
                Some("atLeast") => {
                    m.insert("lineSpacingMode".into(), json!("atLeast"));
                    m.insert("lineSpacingPt".into(), json!((line / TWIP_PER_PX).round()));
                }
                _ => {
                    // auto: 240 = single line spacing → a multiplier.
                    m.insert("lineHeight".into(), json!((line / 240.0 * 100.0).round() / 100.0));
                }
            }
        }
    }
    // Pagination flags + exceptions (ON unless an explicit falsey val).
    let flag = |name: &str| child(ppr, name).map(|n| bool_on(&n)).unwrap_or(false);
    if flag("keepNext") { m.insert("keepNext".into(), json!(true)); }
    if flag("keepLines") { m.insert("keepLines".into(), json!(true)); }
    if flag("pageBreakBefore") { m.insert("pageBreakBefore".into(), json!(true)); }
    if flag("contextualSpacing") { m.insert("contextualSpacing".into(), json!(true)); }
    if flag("suppressLineNumbers") { m.insert("suppressLineNumbers".into(), json!(true)); }
    if flag("suppressAutoHyphens") { m.insert("dontHyphenate".into(), json!(true)); }
    if flag("mirrorIndents") { m.insert("mirrorIndents".into(), json!(true)); }
    // widowControl: on by default in the editor → store only the explicit opt-out.
    if let Some(wc) = child(ppr, "widowControl") {
        if !bool_on(&wc) { m.insert("widowControl".into(), json!(false)); }
    }
    // Outline level: `<w:outlineLvl val="0">` is Level 1.
    if let Some(ol) = child(ppr, "outlineLvl").and_then(|n| attr_val(&n, "val")).and_then(|v| v.parse::<i64>().ok()) {
        m.insert("outlineLevel".into(), json!(ol + 1));
    }
    // `<w15:collapsed/>` — Word's « Réduire par défaut » (MS-DOCX, namespace
    // .../office/word/2012/wordml). The heading opens collapsed, hiding the
    // paragraphs of a HIGHER level that follow it. Matched on the local name:
    // the prefix is the producer's choice, only the namespace is normative.
    if flag("collapsed") {
        m.insert("collapsedDefault".into(), json!(true));
        m.insert("collapsed".into(), json!(true));
    }
    m
}

/// Merge attributes into a node (existing ones are preserved, e.g. `level`).
pub(crate) fn apply_attrs(node: &mut PmNode, attrs: serde_json::Map<String, Value>) {
    if attrs.is_empty() {
        return;
    }
    let mut obj = match node.attrs.take() {
        Some(Value::Object(m)) => m,
        _ => serde_json::Map::new(),
    };
    for (k, v) in attrs {
        obj.insert(k, v);
    }
    node.attrs = Some(Value::Object(obj));
}

pub(crate) fn map_align(jc: &str) -> Option<&'static str> {
    match jc {
        "center" => Some("center"),
        "right" | "end" => Some("right"),
        "both" | "distribute" => Some("justify"),
        "left" | "start" => Some("left"),
        _ => None,
    }
}
