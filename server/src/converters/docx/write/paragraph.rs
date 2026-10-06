//! `<w:p>` and `<w:pPr>` — paragraph serialisation.

use serde_json::{Map, Value};

use crate::converters::types::PmNode;

use super::ctx::ExportCtx;
use super::revisions;
use super::run::render_inline_content;

/// Twips per CSS pixel at 96 dpi (1440 twips per inch / 96 px per inch).
const TWIP_PER_PX: f64 = 15.0;

/// A paragraph with the given style. `render_ppr` owns everything inside
/// `<w:pPr>`; the child ORDER there is imposed by the `CT_PPrBase` schema.
pub(crate) fn render_paragraph_with_style(node: &PmNode, style: &str, ctx: &mut ExportCtx) -> String {
    let runs = render_inline_content(node.children(), ctx);
    let ppr = with_para_mark_revision(render_ppr(node, style, ""), node, ctx);
    format!("<w:p>{ppr}{runs}</w:p>")
}

/// Splice the paragraph-MARK revision into an already-serialised `<w:pPr>`.
///
/// The mark on the pilcrow is what splits a paragraph (`w:ins`) or merges it
/// with the next one (`w:del`) when the change is accepted, so it cannot be
/// deduced from the runs.
///
/// It lands LAST: `CT_PPr` extends `CT_PPrBase` with the sequence rPr, sectPr,
/// pPrChange, so `w:rPr` follows every `CT_PPrBase` child — `w:outlineLvl`
/// included — and Word reports unreadable content on any other position. A
/// paragraph that had no `w:pPr` at all gets one.
///
/// Why not inside `render_ppr`: `w:ins`/`w:del` need a document-unique `w:id`,
/// which only `ExportCtx` allocates, and `render_ppr` takes no context — its
/// signature is depended upon elsewhere (`list.rs`). Callers of `render_ppr`
/// that hold a context should route their result through here.
pub(crate) fn with_para_mark_revision(ppr: String, node: &PmNode, ctx: &mut ExportCtx) -> String {
    let rpr = revisions::paragraph_mark_rpr(node, ctx);
    if rpr.is_empty() {
        return ppr;
    }
    match ppr.strip_suffix("</w:pPr>") {
        Some(head) => format!("{head}{rpr}</w:pPr>"),
        None => format!("<w:pPr>{ppr}{rpr}</w:pPr>"),
    }
}

/// Editor paragraph attributes -> `<w:pPr>`. `extra` is already-serialised XML
/// to splice in at its schema position (currently only `<w:numPr>`).
///
/// Child order follows `CT_PPrBase` (ECMA-376 §17.3.1.26): Word refuses to open
/// a document whose `pPr` children are out of order.
/// Units: `w:ind` / `w:spacing` are twips (px × 15); `w:sz` of a border is in
/// eighths of a point; `w:space` of a border is in whole points; `w:line` in
/// `auto` mode is 240 × the line-height multiplier.
pub(crate) fn render_ppr(node: &PmNode, style: &str, extra: &str) -> String {
    let empty = Map::new();
    let a = match node.attrs.as_ref() {
        Some(Value::Object(m)) => m,
        _ => &empty,
    };
    let f = |k: &str| a.get(k).and_then(|v| v.as_f64());
    let b = |k: &str| a.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
    let s = |k: &str| a.get(k).and_then(|v| v.as_str());

    let mut p = String::new();
    // Word never writes the default style's pStyle: `Normal` is what a
    // paragraph gets when no `w:pStyle` is present at all.
    if !style.is_empty() && style != "Normal" {
        p.push_str(&format!(r#"<w:pStyle w:val="{style}"/>"#));
    }
    if b("keepNext") {
        p.push_str("<w:keepNext/>");
    }
    if b("keepLines") {
        p.push_str("<w:keepLines/>");
    }
    if b("pageBreakBefore") {
        p.push_str("<w:pageBreakBefore/>");
    }
    // widowControl is ON by default in Word: only serialise the opt-out.
    if a.get("widowControl").and_then(|v| v.as_bool()) == Some(false) {
        p.push_str(r#"<w:widowControl w:val="0"/>"#);
    }
    p.push_str(extra);
    if b("suppressLineNumbers") {
        p.push_str("<w:suppressLineNumbers/>");
    }
    p.push_str(&render_pbdr(a));
    if let Some(hex) = s("shading").and_then(hex6) {
        p.push_str(&format!(
            r#"<w:shd w:val="clear" w:color="auto" w:fill="{hex}"/>"#
        ));
    }
    p.push_str(&render_tabs(a));
    if b("dontHyphenate") {
        p.push_str("<w:suppressAutoHyphens/>");
    }

    // spacing: before/after in twips; line = 240 × multiplier (auto) or twips.
    let mut sp = String::new();
    if let Some(v) = f("spaceBefore") {
        sp.push_str(&format!(r#" w:before="{}""#, twips(v.max(0.0))));
    }
    if let Some(v) = f("spaceAfter") {
        sp.push_str(&format!(r#" w:after="{}""#, twips(v.max(0.0))));
    }
    match (s("lineSpacingMode"), f("lineSpacingPt")) {
        (Some("exactly"), Some(v)) if v > 0.0 => {
            sp.push_str(&format!(r#" w:line="{}" w:lineRule="exact""#, twips(v)));
        }
        (Some("atLeast"), Some(v)) if v > 0.0 => {
            sp.push_str(&format!(r#" w:line="{}" w:lineRule="atLeast""#, twips(v)));
        }
        _ => {
            if let Some(v) = f("lineHeight").filter(|v| *v > 0.0) {
                sp.push_str(&format!(
                    r#" w:line="{}" w:lineRule="auto""#,
                    (v * 240.0).round() as i64
                ));
            }
        }
    }
    if !sp.is_empty() {
        p.push_str(&format!("<w:spacing{sp}/>"));
    }

    // ind: Word does not accept a negative firstLine → emit `hanging` instead.
    let mut ind = String::new();
    if let Some(v) = f("indentLeft").filter(|v| *v != 0.0) {
        ind.push_str(&format!(r#" w:left="{}""#, twips(v)));
    }
    if let Some(v) = f("indentRight").filter(|v| *v != 0.0) {
        ind.push_str(&format!(r#" w:right="{}""#, twips(v)));
    }
    match f("indentFirstLine") {
        Some(v) if v > 0.0 => ind.push_str(&format!(r#" w:firstLine="{}""#, twips(v))),
        Some(v) if v < 0.0 => ind.push_str(&format!(r#" w:hanging="{}""#, twips(-v))),
        _ => {}
    }
    if !ind.is_empty() {
        p.push_str(&format!("<w:ind{ind}/>"));
    }

    if b("contextualSpacing") {
        p.push_str("<w:contextualSpacing/>");
    }
    if b("mirrorIndents") {
        p.push_str("<w:mirrorIndents/>");
    }
    if let Some(al) = s("textAlign") {
        // `distribute` also stretches the LAST line (LibreOffice
        // DomainMapper.cxx:5136 sets LastLineAdjust=BLOCK).
        let jc = match (al, b("lastLineJustify")) {
            ("justify", true) => "distribute",
            ("justify", false) => "both",
            ("center", _) => "center",
            ("right", _) => "right",
            _ => "left",
        };
        p.push_str(&format!(r#"<w:jc w:val="{jc}"/>"#));
    }
    // Vertical alignment of characters within the line.
    if let Some(va) = s("textAlignment").filter(|v| {
        matches!(*v, "auto" | "top" | "center" | "baseline" | "bottom")
    }) {
        p.push_str(&format!(r#"<w:textAlignment w:val="{va}"/>"#));
    }
    // Editor level 1..9 → `w:val` 0..8; level 0 (body text) is the default and
    // is left implicit.
    if let Some(l) = f("outlineLevel").filter(|l| (1.0..=9.0).contains(l)) {
        p.push_str(&format!(r#"<w:outlineLvl w:val="{}"/>"#, l as i64 - 1));
    }
    // Word « Réduire par défaut ». An EXTENSION element (namespace
    // .../office/word/2012/wordml, declared on `w:document` and listed in
    // `mc:Ignorable`), so a reader that ignores it simply opens the document
    // expanded — which is exactly the fallback Word intends. Only the DEFAULT
    // flag is written: the live collapse state never travels with the file.
    if node.attrs.as_ref().and_then(|a| a.get("collapsedDefault")).and_then(|v| v.as_bool()) == Some(true) {
        p.push_str("<w15:collapsed/>");
    }

    // NOTE: the paragraph-MARK revision closes `w:pPr`, after `w:outlineLvl`.
    // It is added by `with_para_mark_revision` rather than here, because it
    // needs the export context to allocate its `w:id`.

    if p.is_empty() {
        String::new()
    } else {
        format!("<w:pPr>{p}</w:pPr>")
    }
}

/// px → twips, clamped to the OOXML signed-measure range.
fn twips(px: f64) -> i64 {
    if !px.is_finite() {
        return 0;
    }
    (px * TWIP_PER_PX).round().clamp(-31680.0, 31680.0) as i64
}

/// A CSS colour → a bare 6-digit uppercase hex, or `None` when unusable.
/// Also acts as the XML-injection guard: only hex digits ever reach the output.
fn hex6(c: &str) -> Option<String> {
    let h = c.trim().trim_start_matches('#');
    if h.len() == 6 && h.bytes().all(|b| b.is_ascii_hexdigit()) {
        Some(h.to_uppercase())
    } else {
        None
    }
}

/// `paraBorder` → `<w:pBdr>`. Accepts BOTH the legacy uniform shape
/// `{color,width,style}` (applied to the four sides) and the per-side shape
/// `{top,left,bottom,right,between,bar}`. Units, per LibreOffice
/// `BorderHandler.cxx:58-73`: `w:sz` = eighths of a point (px × 6),
/// `w:space` = whole points (px × 0.75), capped at 31 by the schema.
fn render_pbdr(a: &Map<String, Value>) -> String {
    let Some(Value::Object(bd)) = a.get("paraBorder") else {
        return String::new();
    };
    let one = |side: &Map<String, Value>| -> (&'static str, i64, String, i64) {
        let val = match side.get("style").and_then(|v| v.as_str()).unwrap_or("solid") {
            "dashed" => "dashed",
            "dotted" => "dotted",
            "double" => "double",
            _ => "single",
        };
        let sz = ((side.get("width").and_then(|v| v.as_f64()).unwrap_or(1.0) * 6.0).round() as i64)
            .clamp(2, 96);
        let color = side
            .get("color")
            .and_then(|v| v.as_str())
            .and_then(hex6)
            .unwrap_or_else(|| "000000".into());
        let space = ((side.get("space").and_then(|v| v.as_f64()).unwrap_or(0.0) * 0.75).round()
            as i64)
            .clamp(0, 31);
        (val, sz, color, space)
    };
    let mut out = String::new();
    if bd.contains_key("style") || bd.contains_key("width") || bd.contains_key("color") {
        // Legacy shape: one uniform box.
        let (val, sz, color, space) = one(bd);
        for side in ["top", "left", "bottom", "right"] {
            out.push_str(&format!(
                r#"<w:{side} w:val="{val}" w:sz="{sz}" w:space="{space}" w:color="{color}"/>"#
            ));
        }
    } else {
        // Order imposed by CT_PBdr: top, left, bottom, right, between, bar.
        for side in ["top", "left", "bottom", "right", "between", "bar"] {
            if let Some(Value::Object(sd)) = bd.get(side) {
                let (val, sz, color, space) = one(sd);
                out.push_str(&format!(
                    r#"<w:{side} w:val="{val}" w:sz="{sz}" w:space="{space}" w:color="{color}"/>"#
                ));
            }
        }
    }
    if out.is_empty() {
        String::new()
    } else {
        format!("<w:pBdr>{out}</w:pBdr>")
    }
}

/// `tabStops` → `<w:tabs>`; `tabStopsClear` → `w:val="clear"` entries, which
/// cancel a stop inherited from the style. Positions px → twips.
fn render_tabs(a: &Map<String, Value>) -> String {
    let mut out = String::new();
    if let Some(Value::Array(cl)) = a.get("tabStopsClear") {
        for pos in cl.iter().filter_map(|v| v.as_f64()) {
            out.push_str(&format!(
                r#"<w:tab w:val="clear" w:pos="{}"/>"#,
                twips(pos)
            ));
        }
    }
    if let Some(Value::Array(ts)) = a.get("tabStops") {
        for t in ts {
            // Historical shape: a bare number means a left tab stop.
            let (pos, ty, leader) = match t {
                Value::Number(n) => (n.as_f64().unwrap_or(0.0), "left", None),
                Value::Object(o) => (
                    o.get("pos").and_then(|v| v.as_f64()).unwrap_or(0.0),
                    o.get("type").and_then(|v| v.as_str()).unwrap_or("left"),
                    o.get("leader").and_then(|v| v.as_str()),
                ),
                _ => continue,
            };
            let val = match ty {
                "center" => "center",
                "right" => "right",
                "decimal" => "decimal",
                "bar" => "bar",
                _ => "left",
            };
            let ld = match leader {
                Some("dot") => r#" w:leader="dot""#,
                Some("hyphen") => r#" w:leader="hyphen""#,
                Some("underscore") => r#" w:leader="underscore""#,
                Some("middleDot") => r#" w:leader="middleDot""#,
                _ => "",
            };
            out.push_str(&format!(
                r#"<w:tab w:val="{val}" w:pos="{}"{ld}/>"#,
                twips(pos)
            ));
        }
    }
    if out.is_empty() {
        String::new()
    } else {
        format!("<w:tabs>{out}</w:tabs>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Build a paragraph node carrying the given attribute map.
    fn n(v: serde_json::Value) -> PmNode {
        PmNode { node_type: "paragraph".into(), attrs: Some(v), content: None, marks: None, text: None }
    }
    /// Word « Réduire par défaut » travels as `<w15:collapsed/>`, and ONLY the
    /// default flag: the live collapse state must never reach the file, or a
    /// document would reopen collapsed where Word reopens it expanded.
    #[test]
    fn collapsed_by_default_is_written_but_the_live_state_is_not() {
        let with_default = n(serde_json::json!({ "outlineLevel": 1, "collapsedDefault": true, "collapsed": true }));
        let out = render_ppr(&with_default, "Heading1", "");
        assert!(out.contains("<w15:collapsed/>"), "{out}");
        // After `w:outlineLvl`, as Word writes it.
        let i = |needle: &str| out.find(needle).unwrap_or(usize::MAX);
        assert!(i("<w:outlineLvl") < i("<w15:collapsed/>"), "{out}");

        // Collapsed on screen but NOT « by default » → nothing in the file.
        let live_only = n(serde_json::json!({ "outlineLevel": 1, "collapsed": true }));
        assert!(!render_ppr(&live_only, "Heading1", "").contains("collapsed"));
    }

    /// Every supported property, checked for value AND for CT_PPrBase order.
    #[test]
    fn full_ppr_is_complete_and_ordered() {
        let node = n(serde_json::json!({
            "textAlign": "justify", "lastLineJustify": true,
            "indentLeft": 48.0, "indentFirstLine": -24.0, "indentRight": 12.0,
            "spaceBefore": 8.0, "spaceAfter": 0.0, "lineHeight": 1.5,
            "keepNext": true, "widowControl": false, "dontHyphenate": true,
            "contextualSpacing": true, "mirrorIndents": true, "outlineLevel": 2,
            "shading": "#fff2cc", "textAlignment": "center",
            "paraBorder": {"color": "#9aa0a6", "width": 1, "style": "dashed"},
            "tabStops": [{"pos": 96, "type": "decimal", "leader": "dot"}, 48],
            "tabStopsClear": [200]
        }));
        let out = render_ppr(&node, "Normal", "<w:numPr/>");
        // `Normal` is the default style: Word omits its pStyle entirely.
        assert!(out.starts_with(r#"<w:pPr><w:keepNext/><w:widowControl w:val="0"/><w:numPr/>"#));
        assert!(out.contains(r#"<w:ind w:left="720" w:right="180" w:hanging="360"/>"#));
        assert!(out.contains(r#"<w:spacing w:before="120" w:after="0" w:line="360" w:lineRule="auto"/>"#));
        assert!(out.contains(r#"<w:jc w:val="distribute"/>"#));
        assert!(out.contains(r#"<w:outlineLvl w:val="1"/>"#));
        assert!(out.contains(r#"<w:shd w:val="clear" w:color="auto" w:fill="FFF2CC"/>"#));
        assert!(out.contains(r#"<w:tab w:val="clear" w:pos="3000"/><w:tab w:val="decimal" w:pos="1440" w:leader="dot"/><w:tab w:val="left" w:pos="720"/>"#));
        assert!(out.contains(r#"<w:pBdr><w:top w:val="dashed" w:sz="6" w:space="0" w:color="9AA0A6"/><w:left"#));
        // Order check: pBdr < shd < tabs < suppressAutoHyphens < spacing < ind < jc
        let i = |s: &str| out.find(s).expect(s);
        assert!(i("<w:pBdr>") < i("<w:shd") && i("<w:shd") < i("<w:tabs>"));
        assert!(i("<w:tabs>") < i("<w:suppressAutoHyphens/>"));
        assert!(i("<w:suppressAutoHyphens/>") < i("<w:spacing"));
        assert!(i("<w:spacing") < i("<w:ind ") && i("<w:ind ") < i("<w:contextualSpacing/>"));
        assert!(i("<w:mirrorIndents/>") < i("<w:jc ") && i("<w:jc ") < i("<w:textAlignment"));
        assert!(i("<w:textAlignment") < i("<w:outlineLvl"));
    }
    /// The paragraph-mark revision closes `w:pPr`: `CT_PPr` puts `rPr` after
    /// every `CT_PPrBase` child, `w:outlineLvl` included.
    #[test]
    fn para_mark_revision_comes_last_in_ppr() {
        let node = n(serde_json::json!({
            "outlineLevel": 2,
            "textAlign": "center",
            "insertion": { "author": "Jean Dupont", "date": "2026-07-28T09:12:00Z", "id": "p1" }
        }));
        let mut ctx = ExportCtx::new();
        let out = with_para_mark_revision(render_ppr(&node, "", ""), &node, &mut ctx);
        let i = |s: &str| out.find(s).unwrap_or_else(|| panic!("manque {s} dans {out}"));
        assert!(i("<w:outlineLvl") < i("<w:rPr>"), "{out}");
        assert!(out.contains(r#"<w:ins w:id="#), "{out}");
        assert!(out.ends_with("</w:rPr></w:pPr>"), "{out}");
        // A paragraph with no other property still gets a well-formed `w:pPr`.
        let bare = n(serde_json::json!({
            "deletion": { "author": "Marie", "date": "2026-07-28T10:00:00Z", "id": "p2" }
        }));
        let out = with_para_mark_revision(render_ppr(&bare, "", ""), &bare, &mut ctx);
        assert!(out.starts_with("<w:pPr><w:rPr><w:del ") && out.ends_with("</w:rPr></w:pPr>"), "{out}");
    }

    #[test]
    fn empty_ppr_and_exact_line_rule() {
        assert_eq!(render_ppr(&n(serde_json::json!({})), "", ""), "");
        let node = n(serde_json::json!({"lineSpacingMode": "exactly", "lineSpacingPt": 20.0}));
        assert!(render_ppr(&node, "", "").contains(r#"w:line="300" w:lineRule="exact""#));
    }
}
