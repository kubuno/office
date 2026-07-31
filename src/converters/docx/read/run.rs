//! `<w:r>` and `<w:rPr>` - runs, character formatting and inline content.

use std::collections::HashMap;

use roxmltree::Node;
use serde_json::{json, Value};

use crate::converters::docx::model::FontCtx;
use crate::converters::docx::xml::{attr_val, child, local, simple_mark, toggle_on};
use crate::converters::types::{PmMark, PmNode};

use super::drawing::inline_image_node;
use super::theme::resolve_run_font;
use super::fields::{self, handle_field_run, FieldState, ResultAction};
use super::notes::NoteTexts;
use super::revisions;

/// Inline content of a paragraph: runs, hyperlinks and tracked changes.
#[allow(clippy::too_many_arguments)]
pub(crate) fn parse_inline(
    parent: &Node<'_, '_>,
    rels: &HashMap<String, String>,
    fonts: &FontCtx,
    theme: &HashMap<String, String>,
    media: &HashMap<String, String>,
    style_run: &serde_json::Map<String, Value>,
    fs: &mut FieldState,
    notes: &NoteTexts,
) -> Vec<PmNode> {
    let mut out = Vec::new();
    for node in parent.children().filter(|n| n.is_element()) {
        // Bookmarks and fields are RANGES, tracked across runs and paragraphs.
        if fs.handle_bookmark(&node) {
            continue;
        }
        match local(&node) {
            // The compact form: instruction in an attribute, children = result.
            "fldSimple" => {
                fields::begin_simple(&node, fs);
                out.extend(parse_inline(&node, rels, fonts, theme, media, style_run, fs, notes));
                fields::end_simple(fs);
            }
            "r" => {
                // A run made only of fldChar/instrText is structure, not text.
                if handle_field_run(&node, fs) {
                    continue;
                }
                match fs.result_action() {
                    ResultAction::Drop => {
                        // Still capture a PAGEREF result: it is a TOC page number.
                        for t in node.children().filter(|n| local(n) == "t") {
                            fs.note_toc_page(t.text().unwrap_or(""));
                        }
                    }
                    ResultAction::Replace(tok) => {
                        let mut tmp = Vec::new();
                        parse_run(&node, &fs.extra_marks(), &mut tmp, fonts, theme, media, style_run, fs, notes);
                        match tmp.into_iter().find(|n| n.node_type == "text") {
                            Some(mut n) => {
                                n.text = Some(tok);
                                out.push(n);
                            }
                            None => out.push(PmNode {
                                node_type: "text".into(),
                                attrs: None,
                                content: None,
                                marks: None,
                                text: Some(tok),
                            }),
                        }
                        fs.mark_emitted();
                    }
                    ResultAction::Keep => {
                        let extra = fs.extra_marks();
                        parse_run(&node, &extra, &mut out, fonts, theme, media, style_run, fs, notes);
                    }
                }
            }
            // Run CONTAINERS: hyperlinks and tracked-change ranges. They nest in
            // any order, so they share a single recursive walk.
            t if is_run_container(t) => {
                parse_run_container(
                    &node, &[], &mut out, rels, fonts, theme, media, style_run, fs, notes, 0,
                );
            }
            _ => {}
        }
    }
    out
}

/// Elements that WRAP runs inside a paragraph: the hyperlink and the four
/// tracked-change wrappers (`w:ins`, `w:del`, `w:moveFrom`, `w:moveTo`).
fn is_run_container(tag: &str) -> bool {
    tag == "hyperlink" || revisions::is_revision_wrapper(tag)
}

/// Marks contributed by one container element, to be merged with those of the
/// containers already opened above it.
fn container_marks(node: &Node<'_, '_>, rels: &HashMap<String, String>) -> Vec<PmMark> {
    if local(node) == "hyperlink" {
        let href = attr_val(node, "id")
            .and_then(|rid| rels.get(&rid).cloned())
            .or_else(|| attr_val(node, "anchor").map(|a| format!("#{a}")));
        return href
            .map(|h| PmMark {
                mark_type: "link".into(),
                attrs: Some(json!({ "href": h })),
            })
            .into_iter()
            .collect();
    }
    revisions::revision_marks(node)
}

/// Walk a run container, accumulating the marks of every enclosing container.
/// Revisions and hyperlinks nest BOTH ways (an insertion may hold a hyperlink,
/// a hyperlink may hold an insertion); walking them with one function is the
/// only way not to lose the inner content. `depth` guards a pathological file.
#[allow(clippy::too_many_arguments)]
fn parse_run_container(
    node: &Node<'_, '_>,
    inherited: &[PmMark],
    out: &mut Vec<PmNode>,
    rels: &HashMap<String, String>,
    fonts: &FontCtx,
    theme: &HashMap<String, String>,
    media: &HashMap<String, String>,
    style_run: &serde_json::Map<String, Value>,
    fs: &mut FieldState,
    notes: &NoteTexts,
    depth: u8,
) {
    let mut marks = inherited.to_vec();
    marks.extend(container_marks(node, rels));
    for c in node.children().filter(|n| n.is_element()) {
        match local(&c) {
            "r" => parse_run(&c, &marks, out, fonts, theme, media, style_run, fs, notes),
            t if is_run_container(t) && depth < 8 => {
                parse_run_container(
                    &c, &marks, out, rels, fonts, theme, media, style_run, fs, notes, depth + 1,
                );
            }
            // Everything else (range markers such as `w:moveFromRangeStart`,
            // bookmarks, `w:proofErr`…) carries no inline content here.
            _ => {}
        }
    }
}

/// A single run: build marks from rPr, then emit text / breaks / tabs / inline images.
#[allow(clippy::too_many_arguments)]
pub(crate) fn parse_run(
    r: &Node<'_, '_>,
    extra_marks: &[PmMark],
    out: &mut Vec<PmNode>,
    fonts: &FontCtx,
    theme: &HashMap<String, String>,
    media: &HashMap<String, String>,
    style_run: &serde_json::Map<String, Value>,
    _fs: &mut FieldState,
    notes: &NoteTexts,
) {
    let mut marks: Vec<PmMark> = extra_marks.to_vec();
    let mut text_style = serde_json::Map::new();
    // Base inherited from the paragraph STYLE (bold/italic/underline/strike plus
    // size/font/colour). The run's own rPr overrides these values below.
    let mut bold = style_run.get("bold").and_then(|v| v.as_bool()).unwrap_or(false);
    let mut italic = style_run.get("italic").and_then(|v| v.as_bool()).unwrap_or(false);
    let mut underline = style_run.get("underline").and_then(|v| v.as_bool()).unwrap_or(false);
    let mut strike = style_run.get("strike").and_then(|v| v.as_bool()).unwrap_or(false);
    // Everything else the style resolved (fontSize/fontFamily/color today, more
    // later) is a plain `textStyle` attribute: copy it verbatim.
    for (k, v) in style_run {
        if !matches!(k.as_str(), "bold" | "italic" | "underline" | "strike") {
            text_style.insert(k.clone(), v.clone());
        }
    }

    // Character properties that can only be turned into attributes once the whole
    // rPr has been read (order inside `<w:rPr>` is schema-imposed, and `w:position`
    // needs the final font size).
    let mut double_strike = false;
    let mut all_caps = false;
    let mut small_caps = false;
    let mut hidden = false;
    let mut script: Option<&'static str> = None;
    let mut underline_kind: Option<(&'static str, bool)> = None; // (style, words-only)
    let mut underline_color: Option<String> = None;
    let mut position_hp: Option<f64> = None;
    let mut cs_size_pt: Option<f64> = None;
    let mut highlight_hex: Option<String> = None;
    let mut shading_hex: Option<String> = None;

    if let Some(rpr) = child(r, "rPr") {
        for m in rpr.children().filter(|n| n.is_element()) {
            match local(&m) {
                // Toggle properties: 0/1 but also 128 ("inherit") and 129 ("invert
                // the style's value") - cf. `toggle_on`.
                "b" => bold = toggle_on(&m, bold).unwrap_or(bold),
                "i" => italic = toggle_on(&m, italic).unwrap_or(italic),
                // `w:strike` and `w:dstrike` map onto a single editor mark: a
                // `w:strike w:val="0"` must not cancel a `w:dstrike` seen earlier
                // in the same rPr (LibreOffice inserts it with bOverwrite = false,
                // DomainMapper.cxx:2354).
                "strike" => {
                    if let Some(v) = toggle_on(&m, strike) {
                        if v {
                            strike = true;
                        } else if !double_strike {
                            strike = false;
                        }
                    }
                }
                "dstrike" => {
                    if let Some(v) = toggle_on(&m, double_strike) {
                        double_strike = v;
                        if v {
                            strike = true;
                        }
                    }
                }
                // 17 underline styles + "words only" + line colour, instead of the
                // former boolean (DomainMapper::handleUnderlineType, :5042-5104).
                "u" => match underline_style(attr_val(&m, "val").as_deref()) {
                    Some(kind) => {
                        underline = true;
                        underline_kind = Some(kind);
                        underline_color = resolve_underline_color(&m, theme);
                    }
                    None => {
                        underline = false;
                        underline_kind = None;
                        underline_color = None;
                    }
                },
                // caps / smallCaps both map to a case transform; they are tracked
                // separately so that `smallCaps w:val="0"` cannot clear a `caps`
                // set earlier in the same rPr (DomainMapper.cxx:2371-2381).
                "caps" => {
                    if let Some(v) = toggle_on(&m, all_caps) {
                        all_caps = v;
                    }
                }
                "smallCaps" => {
                    if let Some(v) = toggle_on(&m, small_caps) {
                        small_caps = v;
                    }
                }
                // Hidden text (CharHidden, DomainMapper.cxx:2280-2283): the run
                // must not be rendered.
                "vanish" | "webHidden" => {
                    if let Some(v) = toggle_on(&m, hidden) {
                        hidden = v;
                    }
                }
                "vertAlign" => {
                    script = match attr_val(&m, "val").as_deref() {
                        Some("superscript") => Some("superscript"),
                        Some("subscript") => Some("subscript"),
                        _ => None, // "baseline" (and anything unknown) clears it
                    }
                }
                // Literal `w:val`, else `w:themeColor` resolved through theme1.xml
                // with `w:themeTint`/`w:themeShade` applied.
                "color" => {
                    if let Some(c) = resolve_theme_color(&m, theme) {
                        text_style.insert("color".into(), json!(c));
                    }
                }
                "sz" => {
                    if let Some(v) = attr_val(&m, "val").and_then(|v| v.parse::<f64>().ok()) {
                        // DOCX half-points → points.
                        text_style.insert("fontSize".into(), json!(v / 2.0));
                    }
                }
                // Complex-script size: a property of its own in Word. Word writes it
                // on nearly every run, so it is only kept when it actually differs
                // from `w:sz` (resolved after the loop) — otherwise it is pure noise.
                "szCs" => {
                    cs_size_pt = attr_val(&m, "val")
                        .and_then(|v| v.parse::<f64>().ok())
                        .map(|v| v / 2.0);
                }
                // w:spacing = ST_SignedTwipsMeasure (signed twips, 1 pt = 20 twips),
                // NOT half-points despite the misleading comment in
                // DomainMapper.cxx:2470. Word's legacy 16-bit guard drops anything
                // at or above 0x8000 (ConversionHelper.cxx:423-430).
                "spacing" => {
                    if let Some(tw) = attr_val(&m, "val").and_then(|v| v.parse::<i64>().ok()) {
                        if tw != 0 && tw.unsigned_abs() < 0x8000 {
                            text_style.insert("letterSpacing".into(), json!(tw as f64 / 20.0));
                        }
                    }
                }
                // w:position = signed half-points; the editor stores a percentage of
                // the font size, so it is resolved after the loop (the schema puts
                // `w:position` *before* `w:sz`).
                "position" => {
                    position_hp = attr_val(&m, "val").and_then(|v| v.parse::<f64>().ok());
                }
                // w:w = ST_TextScale (%). Out-of-range values fall back to 100, they
                // are not clamped (DomainMapper.cxx:2486-2496).
                "w" => {
                    if let Some(p) = attr_val(&m, "val").and_then(|v| v.parse::<i64>().ok()) {
                        let p = if (1..=600).contains(&p) { p } else { 100 };
                        if p != 100 {
                            text_style.insert("charScale".into(), json!(p));
                        }
                    }
                }
                "rFonts" => {
                    // Literal name OR theme reference (asciiTheme="minorHAnsi"…).
                    if let Some(f) = resolve_run_font(&m, fonts) {
                        text_style.insert("fontFamily".into(), json!(f));
                    }
                }
                "highlight" => {
                    if let Some(c) = attr_val(&m, "val").filter(|c| c != "none") {
                        highlight_hex = Some(highlight_color(&c));
                    }
                }
                // `w:shd` is an arbitrary character background (CharBackColor), a
                // property distinct from `w:highlight` (CharHighlight) and of lower
                // priority: the highlight wins visually (DomainMapper.cxx:2222 vs
                // CellColorHandler.cxx:302). `w:fill="auto"` means white.
                "shd" => {
                    shading_hex = None;
                    if attr_val(&m, "val").as_deref() != Some("nil") {
                        if let Some(fill) = attr_val(&m, "fill") {
                            let f = fill.trim_start_matches('#');
                            if !f.eq_ignore_ascii_case("auto")
                                && f.len() == 6
                                && !f.eq_ignore_ascii_case("FFFFFF")
                            {
                                shading_hex = Some(format!("#{}", f.to_ascii_uppercase()));
                            }
                        }
                    }
                }
                "rStyle" if attr_val(&m, "val").as_deref() == Some("CodeChar") => {
                    marks.push(simple_mark("code"))
                }
                _ => {}
            }
        }
    }

    // Character attributes gathered from the rPr.
    if let Some((style, words)) = underline_kind {
        if style != "single" {
            text_style.insert("underlineStyle".into(), json!(style));
        }
        if words {
            text_style.insert("underlineWords".into(), json!(true));
        }
        if let Some(c) = &underline_color {
            text_style.insert("underlineColor".into(), json!(c));
        }
    }
    if double_strike {
        text_style.insert("doubleStrike".into(), json!(true));
    }
    if all_caps {
        text_style.insert("allCaps".into(), json!(true));
    }
    if small_caps {
        text_style.insert("smallCaps".into(), json!(true));
    }
    if hidden {
        text_style.insert("hidden".into(), json!(true));
    }
    // Both a highlight and a shading: the highlight takes the `highlight` mark, the
    // shading is kept as an attribute so the information is not lost.
    if highlight_hex.is_some() {
        if let Some(sh) = &shading_hex {
            text_style.insert("shading".into(), json!(sh));
        }
    }

    // No explicit font on the run → the document default (often "Calibri" via the
    // minor theme). Without it the body text falls back to Arial.
    if !text_style.contains_key("fontFamily") {
        if let Some(def) = &fonts.default {
            text_style.insert("fontFamily".into(), json!(def));
        }
    }
    // Same for the SIZE: a run without `<w:sz>` inherits the default size.
    if !text_style.contains_key("fontSize") {
        text_style.insert("fontSize".into(), json!(fonts.default_size));
    }

    // Complex-script size, only when it differs from the effective font size.
    if let Some(cs) = cs_size_pt {
        let size = text_style.get("fontSize").and_then(length_pt);
        if size.is_none_or(|s| (s - cs).abs() > 0.01) {
            text_style.insert("fontSizeCs".into(), json!(cs));
        }
    }

    // w:position → vertical offset as a percentage of the (now known) font size,
    // capped at ±13999 like LibreOffice (DomainMapper.cxx:3997-4021). A value of 0
    // is equivalent to the property being absent (:2435).
    if let Some(hp) = position_hp.filter(|v| *v != 0.0) {
        let size = text_style
            .get("fontSize")
            .and_then(length_pt)
            .unwrap_or(fonts.default_size);
        let pct = if size > 0.0 {
            (hp / 2.0 / size * 100.0).round()
        } else if hp > 0.0 {
            33.0 // DFLT_ESC_SUPER, editeng/escapementitem.hxx:28
        } else {
            -8.0 // DFLT_ESC_SUB, :29
        };
        text_style.insert("charPosition".into(), json!(pct.clamp(-13999.0, 13999.0)));
    }

    // Final toggle marks (inherited style, overridden by the run's own rPr).
    if bold { marks.push(simple_mark("bold")); }
    if italic { marks.push(simple_mark("italic")); }
    if underline { marks.push(simple_mark("underline")); }
    if strike { marks.push(simple_mark("strike")); }
    if let Some(s) = script { marks.push(simple_mark(s)); }
    if let Some(color) = highlight_hex.or(shading_hex) {
        marks.push(PmMark {
            mark_type: "highlight".into(),
            attrs: Some(json!({ "color": color })),
        });
    }

    if !text_style.is_empty() {
        marks.push(PmMark {
            mark_type: "textStyle".into(),
            attrs: Some(Value::Object(text_style)),
        });
    }

    for c in r.children().filter(|n| n.is_element()) {
        match local(&c) {
            // Inside a `<w:del>` / `<w:moveFrom>` the text element is named
            // `<w:delText>` — reading only `<w:t>` drops EVERY deleted word
            // silently, which is the classic tracked-changes import bug.
            t if revisions::is_run_text(t) => {
                let txt = c.text().unwrap_or("");
                if !txt.is_empty() {
                    out.push(PmNode::text(txt, marks.clone()));
                }
            }
            "br" | "cr" => out.push(PmNode::hard_break()),
            "tab" => out.push(PmNode::text("\t", marks.clone())),
            // Drawing anchored "in line with text" (`<wp:inline>`) → inlineImage node.
            "drawing" => {
                if let Some(frame) = c.descendants().find(|n| local(n) == "inline") {
                    if let Some(node) = inline_image_node(&frame, theme, media) {
                        out.push(node);
                    }
                }
            }
            // A note reference: the body only carries the id, the text comes
            // from `footnotes.xml` / `endnotes.xml`. A reference whose text is
            // missing is dropped rather than turned into an empty note.
            "footnoteReference" | "endnoteReference" => {
                let endnote = local(&c) == "endnoteReference";
                if let Some(text) = attr_val(&c, "id").and_then(|id| notes.get(endnote, &id)) {
                    out.push(PmNode {
                        node_type: if endnote { "endnote".into() } else { "footnote".into() },
                        attrs: Some(json!({ "text": text })),
                        content: None,
                        marks: None,
                        text: None,
                    });
                }
            }
            _ => {}
        }
    }
}

/// A model length: either a raw number of points or a CSS string such as "11pt".
fn length_pt(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().trim_end_matches("pt").trim().parse::<f64>().ok(),
        _ => None,
    }
}

/// DOCX named highlight colour → editor hex (highlight mark `color` attr). These
/// are the EXACT Word colours, identical to LibreOffice's `getColorFromId`
/// (DomainMapper.cxx:5196-5212): saturated colours, not pastels. Anything else
/// falls back to yellow, like Word.
pub(crate) fn highlight_color(name: &str) -> String {
    let hex = match name {
        "black" => "#000000",
        "blue" => "#0000FF",
        "cyan" => "#00FFFF",
        "green" => "#00FF00",
        "magenta" => "#FF00FF",
        "red" => "#FF0000",
        "yellow" => "#FFFF00",
        "white" => "#FFFFFF",
        "darkBlue" => "#000080",
        "darkCyan" => "#008080",
        "darkGreen" => "#008000",
        "darkMagenta" => "#800080",
        "darkRed" => "#800000",
        "darkYellow" => "#808000",
        "darkGray" => "#808080",
        "lightGray" => "#C0C0C0",
        _ => "#FFFF00",
    };
    hex.to_string()
}

/// `w:u/@w:val` (ST_Underline) → (line style, "words only"). Table taken from
/// LibreOffice's `DomainMapper::handleUnderlineType` (DomainMapper.cxx:5042-5104);
/// `words` is a single underline plus PROP_CHAR_WORD_MODE (:5051). A `<w:u/>` with
/// no `val` means a single underline (Word's behaviour); `none` disables it.
fn underline_style(val: Option<&str>) -> Option<(&'static str, bool)> {
    Some(match val.unwrap_or("single") {
        "none" => return None,
        "words" => ("single", true),
        "double" => ("double", false),
        "thick" => ("thick", false),
        "dotted" => ("dotted", false),
        "dottedHeavy" => ("dottedHeavy", false),
        "dash" => ("dash", false),
        "dashedHeavy" => ("dashedHeavy", false),
        "dashLong" => ("dashLong", false),
        "dashLongHeavy" => ("dashLongHeavy", false),
        "dotDash" => ("dotDash", false),
        "dashDotHeavy" => ("dashDotHeavy", false),
        "dotDotDash" => ("dotDotDash", false),
        "dashDotDotHeavy" => ("dashDotDotHeavy", false),
        "wave" => ("wave", false),
        "wavyHeavy" => ("wavyHeavy", false),
        "wavyDouble" => ("wavyDouble", false),
        _ => ("single", false), // "single" and anything unknown
    })
}

// ── Theme colours (w:themeColor + w:themeTint / w:themeShade) ─────────────────
//
// WARNING: this is NOT the DrawingML tint/shade of `read::drawing::apply_color_mods`.
// DrawingML (oox/source/drawingml/color.cxx:797-822) works per channel in a
// gamma-decoded space. The WordprocessingML tint/shade works ONLY on the HSL
// LUMINANCE, in sRGB, without gamma:
//   DomainMapper.cxx:2693-2702 -> nTint = (255 - byte) * 10000 / 255
//   docmodel/source/color/ComplexColor.cxx:119 -> ApplyTintOrShade
//   tools/source/generic/color.cxx:241-264 -> L' = L*f + (1-f)  (tint)
//                                             L' = L*f          (shade)
// with f = 1 - nTint/10000, i.e. exactly f = byte/255. The hex byte is therefore
// the KEPT fraction of the colour (0x99 = 153/255 = 60 %).

/// RGB 0-255 → HSL (h in degrees, s/l in 0..1). Port of `basegfx::utils::rgb2hsl`.
fn rgb_to_hsl(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let (r, g, b) = (r / 255.0, g / 255.0, b / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let l = (max + min) / 2.0;
    if d.abs() < 1e-9 {
        return (0.0, 0.0, l);
    }
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let mut h = 60.0
        * if (r - max).abs() < 1e-9 {
            (g - b) / d
        } else if (g - max).abs() < 1e-9 {
            2.0 + (b - r) / d
        } else {
            4.0 + (r - g) / d
        };
    if h < 0.0 {
        h += 360.0;
    }
    (h, s, l)
}

/// HSL → RGB 0-255 (port of `basegfx::utils::hsl2rgb`).
fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (f64, f64, f64) {
    if s.abs() < 1e-9 {
        let v = l * 255.0;
        return (v, v, v);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let f = |t: f64| -> f64 {
        let t = t.rem_euclid(360.0);
        let v = if t < 60.0 {
            p + (q - p) * t / 60.0
        } else if t < 180.0 {
            q
        } else if t < 240.0 {
            p + (q - p) * (240.0 - t) / 60.0
        } else {
            p
        };
        v * 255.0
    };
    (f(h + 120.0), f(h), f(h - 120.0))
}

/// Apply `w:themeTint` / `w:themeShade` (bytes 0x00-0xFF) to a theme colour. Both
/// are mutually exclusive in practice; like LibreOffice (DomainMapper.cxx:2693/2698)
/// a zero value is ignored.
fn apply_theme_tint_shade(hex: &str, tint: Option<u8>, shade: Option<u8>) -> String {
    let h = hex.trim_start_matches('#');
    let comp = |i: usize| {
        h.get(i..i + 2)
            .and_then(|s| u8::from_str_radix(s, 16).ok())
            .unwrap_or(0) as f64
    };
    let (r, g, b) = (comp(0), comp(2), comp(4));
    let tint = tint.filter(|v| *v > 0);
    let shade = shade.filter(|v| *v > 0);
    if tint.is_none() && shade.is_none() {
        // No modifier: avoid a pointless HSL round-trip (and its rounding).
        let cl = |x: f64| x.round().clamp(0.0, 255.0) as u8;
        return format!("#{:02X}{:02X}{:02X}", cl(r), cl(g), cl(b));
    }
    let (hh, ss, mut ll) = rgb_to_hsl(r, g, b);
    if let Some(t) = tint {
        let f = t as f64 / 255.0;
        ll = ll * f + (1.0 - f); // lightens towards white
    } else if let Some(s) = shade {
        let f = s as f64 / 255.0;
        ll *= f; // darkens towards black
    }
    let (nr, ng, nb) = hsl_to_rgb(hh, ss, ll.clamp(0.0, 1.0));
    let cl = |x: f64| x.round().clamp(0.0, 255.0) as u8;
    format!("#{:02X}{:02X}{:02X}", cl(nr), cl(ng), cl(nb))
}

/// `w:themeColor` (ST_ThemeColor) → key of theme1.xml's `<a:clrScheme>`. The
/// text1↔dk1 / background1↔lt1 aliases are resolved like LibreOffice
/// (TDefTableHandler.cxx:272-317, clrscheme.cxx:103-134).
fn theme_color_key(name: &str) -> Option<&'static str> {
    Some(match name {
        "dark1" | "text1" | "tx1" => "dk1",
        "light1" | "background1" | "bg1" => "lt1",
        "dark2" | "text2" | "tx2" => "dk2",
        "light2" | "background2" | "bg2" => "lt2",
        "accent1" => "accent1",
        "accent2" => "accent2",
        "accent3" => "accent3",
        "accent4" => "accent4",
        "accent5" => "accent5",
        "accent6" => "accent6",
        "hyperlink" | "hlink" => "hlink",
        "followedHyperlink" | "folHlink" => "folHlink",
        _ => return None, // includes "none"
    })
}

/// Hex byte of a `themeTint`/`themeShade` attribute. LibreOffice writes those bytes
/// WITHOUT a leading zero and in lower case (`OString::number(n, 16)`,
/// docxattributeoutput.cxx:401), so accept 1 or 2 digits.
fn hex_byte(v: &str) -> Option<u8> {
    let s = v.trim();
    if s.is_empty() || s.len() > 2 {
        return None;
    }
    u8::from_str_radix(s, 16).ok()
}

/// Literal `w:val` of a colour-bearing element, uppercased, or `None` for `auto`.
fn literal_color(v: Option<&str>) -> Option<String> {
    v.map(|v| v.trim().trim_start_matches('#'))
        .filter(|v| v.len() == 6 && v.chars().all(|c| c.is_ascii_hexdigit()))
        .map(|v| format!("#{}", v.to_ascii_uppercase()))
}

/// Theme reference of an element carrying `themeColor`/`themeTint`/`themeShade`.
fn theme_ref_color(m: &Node<'_, '_>, theme: &HashMap<String, String>) -> Option<String> {
    let base = theme_color_key(&attr_val(m, "themeColor")?)
        .and_then(|k| theme.get(k))?
        .clone();
    Some(apply_theme_tint_shade(
        &base,
        attr_val(m, "themeTint").and_then(|v| hex_byte(&v)),
        attr_val(m, "themeShade").and_then(|v| hex_byte(&v)),
    ))
}

/// Colour of a run's `<w:color>`. `w:val` wins because Word already writes the
/// RESOLVED theme colour there (LibreOffice does the same for display,
/// DomainMapper.cxx:2676); the theme is only consulted when `val` is `auto` or
/// missing — which is exactly where the colour used to be lost.
fn resolve_theme_color(m: &Node<'_, '_>, theme: &HashMap<String, String>) -> Option<String> {
    literal_color(attr_val(m, "val").as_deref()).or_else(|| theme_ref_color(m, theme))
}

/// Underline colour: `<w:u w:color="RRGGBB">`, else the theme reference. `auto`
/// means "same as the text", so nothing is written (LibreOffice only sets
/// CharUnderlineHasColor for an explicit colour, DomainMapper.cxx:368-374).
fn resolve_underline_color(m: &Node<'_, '_>, theme: &HashMap<String, String>) -> Option<String> {
    literal_color(attr_val(m, "color").as_deref()).or_else(|| theme_ref_color(m, theme))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> FontCtx {
        FontCtx {
            major: "Calibri Light".into(),
            minor: "Calibri".into(),
            default: Some("Calibri".into()),
            default_size: 11.0,
            para_default: serde_json::Map::new(),
            para_styles: HashMap::new(),
            para_style_default: None,
            heading_levels: HashMap::new(),
            run_styles: HashMap::new(),
        }
    }

    /// Parse a `<w:r>` snippet and return the marks of its first text node.
    fn run_marks(rpr: &str, theme: &[(&str, &str)]) -> Vec<PmMark> {
        let xml = format!(
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
                 <w:r><w:rPr>{rpr}</w:rPr><w:t>x</w:t></w:r></w:document>"#
        );
        let doc = roxmltree::Document::parse(&xml).expect("xml");
        let r = doc.descendants().find(|n| local(n) == "r").expect("run");
        let theme: HashMap<String, String> = theme
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        let mut out = Vec::new();
        parse_run(
            &r,
            &[],
            &mut out,
            &ctx(),
            &theme,
            &HashMap::new(),
            &serde_json::Map::new(),
            &mut FieldState::default(),
            &NoteTexts::default(),
        );
        out.first().and_then(|n| n.marks.clone()).unwrap_or_default()
    }

    fn style_attr(marks: &[PmMark], key: &str) -> Option<Value> {
        marks
            .iter()
            .find(|m| m.mark_type == "textStyle")
            .and_then(|m| m.attrs.as_ref())
            .and_then(|a| a.get(key).cloned())
    }

    fn has(marks: &[PmMark], t: &str) -> bool {
        marks.iter().any(|m| m.mark_type == t)
    }

    /// Parse the inline content of a `<w:p>` snippet.
    fn inline_of(body: &str) -> Vec<PmNode> {
        let xml = format!(
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
                           xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
                 <w:p>{body}</w:p></w:document>"#
        );
        let doc = roxmltree::Document::parse(&xml).expect("xml");
        let p = doc.descendants().find(|n| local(n) == "p").expect("p");
        let rels: HashMap<String, String> =
            [("rId7".to_string(), "https://kubuno.test/".to_string())].into_iter().collect();
        parse_inline(
            &p,
            &rels,
            &ctx(),
            &HashMap::new(),
            &HashMap::new(),
            &serde_json::Map::new(),
            &mut FieldState::default(),
            &NoteTexts::default(),
        )
    }

    fn mark_attr(node: &PmNode, mark: &str, key: &str) -> Option<Value> {
        node.marks
            .as_ref()?
            .iter()
            .find(|m| m.mark_type == mark)?
            .attrs
            .as_ref()?
            .get(key)
            .cloned()
    }

    /// The classic tracked-changes import bug: inside a `<w:del>` the text lives
    /// in `<w:delText>`, so reading only `<w:t>` loses every deleted word.
    #[test]
    fn deleted_text_is_kept_and_marked() {
        let nodes = inline_of(
            r#"<w:r><w:t>garde</w:t></w:r>
               <w:del w:id="3" w:author="Marie Curie" w:date="2026-07-28T10:00:00Z">
                 <w:r><w:delText>supprime</w:delText></w:r>
               </w:del>
               <w:ins w:id="4" w:author="Jean Dupont" w:date="2026-07-28T09:12:00Z">
                 <w:r><w:t>ajoute</w:t></w:r>
               </w:ins>"#,
        );
        let texts: Vec<&str> = nodes.iter().filter_map(|n| n.text.as_deref()).collect();
        assert_eq!(texts, ["garde", "supprime", "ajoute"], "{texts:?}");
        assert!(!has(&nodes[0].marks.clone().unwrap_or_default(), "deletion"));
        assert_eq!(
            mark_attr(&nodes[1], "deletion", "author"),
            Some(json!("Marie Curie"))
        );
        assert_eq!(mark_attr(&nodes[1], "deletion", "id"), Some(json!("3")));
        assert_eq!(
            mark_attr(&nodes[2], "insertion", "date"),
            Some(json!("2026-07-28T09:12:00Z"))
        );
    }

    /// Inserted by one reviewer, then deleted by another: `<w:del><w:ins>`. The
    /// text is then in a plain `<w:t>` even though it sits inside a deletion
    /// (7.5 % of the `<w:del>` blocks of the LibreOffice corpus), so BOTH text
    /// elements have to be accepted on import — and both marks must survive.
    #[test]
    fn insertion_deleted_afterwards_keeps_w_t_and_both_marks() {
        let nodes = inline_of(
            r#"<w:del w:id="9" w:author="Marie">
                 <w:ins w:id="8" w:author="Jean"><w:r><w:t>double</w:t></w:r></w:ins>
                 <w:r><w:delText>simple</w:delText></w:r>
               </w:del>"#,
        );
        let texts: Vec<&str> = nodes.iter().filter_map(|n| n.text.as_deref()).collect();
        assert_eq!(texts, ["double", "simple"], "{texts:?}");
        let both = nodes[0].marks.clone().unwrap_or_default();
        assert!(has(&both, "deletion") && has(&both, "insertion"), "{both:?}");
        assert_eq!(mark_attr(&nodes[0], "insertion", "author"), Some(json!("Jean")));
        assert_eq!(mark_attr(&nodes[0], "deletion", "author"), Some(json!("Marie")));
    }

    /// Revisions and hyperlinks nest BOTH ways; neither container may swallow
    /// the other's content. `w:moveFrom`/`w:moveTo` are the move flavour of the
    /// same two changes — and `w:moveFrom` also carries `w:delText`.
    #[test]
    fn revisions_nest_with_hyperlinks_and_moves() {
        let nodes = inline_of(
            r#"<w:ins w:id="1" w:author="A">
                 <w:hyperlink r:id="rId7"><w:r><w:t>lien-insere</w:t></w:r></w:hyperlink>
               </w:ins>
               <w:hyperlink r:id="rId7">
                 <w:del w:id="2" w:author="B"><w:r><w:delText>lien-supprime</w:delText></w:r></w:del>
               </w:hyperlink>
               <w:moveFrom w:id="5" w:author="C"><w:r><w:delText>deplace-de</w:delText></w:r></w:moveFrom>
               <w:moveTo w:id="6" w:author="C"><w:r><w:t>deplace-vers</w:t></w:r></w:moveTo>"#,
        );
        let texts: Vec<&str> = nodes.iter().filter_map(|n| n.text.as_deref()).collect();
        assert_eq!(
            texts,
            ["lien-insere", "lien-supprime", "deplace-de", "deplace-vers"],
            "{texts:?}"
        );
        for (i, revision) in [(0usize, "insertion"), (1, "deletion")] {
            let m = nodes[i].marks.clone().unwrap_or_default();
            assert!(has(&m, revision), "noeud {i}: {revision} manquant");
            assert!(has(&m, "link"), "noeud {i}: lien perdu par l'imbrication");
        }
        assert_eq!(
            mark_attr(&nodes[0], "link", "href"),
            Some(json!("https://kubuno.test/"))
        );
        assert!(has(&nodes[2].marks.clone().unwrap_or_default(), "deletion"));
        assert!(has(&nodes[3].marks.clone().unwrap_or_default(), "insertion"));
    }

    #[test]
    fn highlight_uses_word_saturated_colours() {
        assert_eq!(highlight_color("yellow"), "#FFFF00");
        assert_eq!(highlight_color("darkGray"), "#808080");
        assert_eq!(highlight_color("wat"), "#FFFF00");
        let marks = run_marks(r#"<w:highlight w:val="green"/>"#, &[]);
        let hl = marks.iter().find(|m| m.mark_type == "highlight").expect("highlight");
        assert_eq!(hl.attrs.as_ref().and_then(|a| a.get("color")), Some(&json!("#00FF00")));
    }

    #[test]
    fn highlight_wins_over_shd() {
        let marks = run_marks(
            r#"<w:highlight w:val="yellow"/><w:shd w:val="clear" w:fill="C0C0C0"/>"#,
            &[],
        );
        let hl = marks.iter().find(|m| m.mark_type == "highlight").expect("highlight");
        assert_eq!(hl.attrs.as_ref().and_then(|a| a.get("color")), Some(&json!("#FFFF00")));
        assert_eq!(style_attr(&marks, "shading"), Some(json!("#C0C0C0")));
        // …and w:shd alone still paints the background.
        let only = run_marks(r#"<w:shd w:val="clear" w:fill="C0C0C0"/>"#, &[]);
        let hl = only.iter().find(|m| m.mark_type == "highlight").expect("highlight");
        assert_eq!(hl.attrs.as_ref().and_then(|a| a.get("color")), Some(&json!("#C0C0C0")));
    }

    #[test]
    fn toggles_128_and_129() {
        assert!(has(&run_marks(r#"<w:b/>"#, &[]), "bold"));
        assert!(!has(&run_marks(r#"<w:b w:val="0"/>"#, &[]), "bold"));
        // 128 = inherit (nothing set here) / 129 = invert the inherited value.
        assert!(!has(&run_marks(r#"<w:b w:val="128"/>"#, &[]), "bold"));
        assert!(has(&run_marks(r#"<w:b w:val="129"/>"#, &[]), "bold"));
    }

    #[test]
    fn underline_styles_and_colour() {
        let m = run_marks(r#"<w:u w:val="dotted" w:color="FF0000"/>"#, &[]);
        assert!(has(&m, "underline"));
        assert_eq!(style_attr(&m, "underlineStyle"), Some(json!("dotted")));
        assert_eq!(style_attr(&m, "underlineColor"), Some(json!("#FF0000")));
        let m = run_marks(r#"<w:u w:val="words"/>"#, &[]);
        assert_eq!(style_attr(&m, "underlineWords"), Some(json!(true)));
        assert_eq!(style_attr(&m, "underlineStyle"), None); // "single" stays implicit
        assert!(!has(&run_marks(r#"<w:u w:val="none"/>"#, &[]), "underline"));
        assert!(has(&run_marks(r#"<w:u/>"#, &[]), "underline"));
    }

    #[test]
    fn theme_colour_with_tint_and_shade() {
        let theme = [("accent1", "4472C4"), ("dk1", "000000")];
        // val wins when it is a literal colour.
        let m = run_marks(
            r#"<w:color w:val="FF0000" w:themeColor="accent1"/>"#,
            &theme,
        );
        assert_eq!(style_attr(&m, "color"), Some(json!("#FF0000")));
        // val="auto" → fall back to the theme colour.
        let m = run_marks(r#"<w:color w:val="auto" w:themeColor="accent1"/>"#, &theme);
        assert_eq!(style_attr(&m, "color"), Some(json!("#4472C4")));
        // themeShade="BF" on dk1(black) stays black; on accent1 it darkens.
        let m = run_marks(
            r#"<w:color w:themeColor="accent1" w:themeShade="BF"/>"#,
            &theme,
        );
        let shaded = style_attr(&m, "color").expect("colour");
        assert_eq!(shaded, json!("#2F5496")); // Word's "Blue, Accent 1, Darker 25%"
        // themeTint="99" lightens (Word's "Blue, Accent 1, Lighter 40%" = 8EAADB).
        // Word rounds the HSL round-trip slightly differently, hence the ±1 margin.
        let m = run_marks(r#"<w:color w:themeColor="accent1" w:themeTint="99"/>"#, &theme);
        let tinted = style_attr(&m, "color").expect("colour");
        let tinted = tinted.as_str().unwrap_or_default().to_string();
        for (i, want) in [0x8Eu8, 0xAA, 0xDB].iter().enumerate() {
            let got = u8::from_str_radix(&tinted[1 + i * 2..3 + i * 2], 16).unwrap_or(0);
            assert!(
                got.abs_diff(*want) <= 1,
                "themeTint: {tinted} loin de #8EAADB (canal {i})"
            );
        }
        // text1 is an alias of dk1.
        let m = run_marks(r#"<w:color w:themeColor="text1"/>"#, &theme);
        assert_eq!(style_attr(&m, "color"), Some(json!("#000000")));
    }

    /// Corpus survey of the tracked-change import: how many real Word files
    /// carry revisions, and how much deleted text they hand back. Ignored by
    /// default (it walks thousands of files); driven by hand with
    ///
    /// ```text
    /// KUBUNO_DOCX_CORPUS=… cargo test --release \
    ///     converters::docx::read::run::tests::corpus_revision_survey \
    ///     -- --ignored --nocapture
    /// ```
    ///
    /// `KUBUNO_DOCX_FILE` restricts it to one document and dumps its deleted
    /// text, which is what gets diffed against `soffice --convert-to fodt`.
    #[test]
    #[ignore]
    fn corpus_revision_survey() {
        use crate::converters::docx::import_docx;

        fn walk(node: &PmNode, ins: &mut usize, del: &mut String, para: &mut usize) {
            if let Some(marks) = &node.marks {
                if marks.iter().any(|m| m.mark_type == "insertion") {
                    *ins += 1;
                }
                if marks.iter().any(|m| m.mark_type == "deletion") {
                    del.push_str(node.text.as_deref().unwrap_or(""));
                }
            }
            if node.attrs.as_ref().and_then(|a| a.get("paraMark")).is_some() {
                *para += 1;
            }
            for c in node.content.iter().flatten() {
                walk(c, ins, del, para);
            }
        }

        let Ok(root) = std::env::var("KUBUNO_DOCX_CORPUS") else {
            eprintln!("KUBUNO_DOCX_CORPUS absent — test ignoré");
            return;
        };
        let only = std::env::var("KUBUNO_DOCX_FILE").ok();
        let (mut seen, mut with_ins, mut with_del, mut with_para, mut chars) = (0, 0, 0, 0, 0);
        let mut stack = vec![std::path::PathBuf::from(root)];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().is_none_or(|x| x != "docx") {
                    continue;
                }
                if only.as_deref().is_some_and(|f| !p.to_string_lossy().contains(f)) {
                    continue;
                }
                let Ok(bytes) = std::fs::read(&p) else { continue };
                let Ok((body, hdr, ftr, _)) = import_docx(&bytes) else { continue };
                seen += 1;
                let (mut i, mut d, mut pm) = (0, String::new(), 0);
                for part in [Some(&body), hdr.as_ref(), ftr.as_ref()].into_iter().flatten() {
                    walk(part, &mut i, &mut d, &mut pm);
                }
                if i > 0 { with_ins += 1; }
                if !d.is_empty() { with_del += 1; }
                if pm > 0 { with_para += 1; }
                chars += d.chars().count();
                if only.is_some() {
                    println!("{}\n  insertions={i} paraMark={pm}\n  supprimé={d:?}", p.display());
                }
            }
        }
        println!(
            "corpus révisions: {seen} fichiers · insertions {with_ins} · suppressions {with_del} \
             · marques de fin de § {with_para} · {chars} caractères supprimés récupérés"
        );
    }

    #[test]
    fn caps_hidden_spacing_position_and_scale() {
        let m = run_marks(
            r#"<w:caps/><w:smallCaps w:val="0"/><w:vanish/><w:dstrike/>
               <w:spacing w:val="40"/><w:position w:val="6"/><w:w w:val="150"/>
               <w:sz w:val="24"/><w:szCs w:val="20"/>"#,
            &[],
        );
        assert_eq!(style_attr(&m, "allCaps"), Some(json!(true)));
        assert_eq!(style_attr(&m, "smallCaps"), None);
        assert_eq!(style_attr(&m, "hidden"), Some(json!(true)));
        assert_eq!(style_attr(&m, "doubleStrike"), Some(json!(true)));
        assert!(has(&m, "strike"));
        assert_eq!(style_attr(&m, "letterSpacing"), Some(json!(2.0))); // 40 twips = 2 pt
        // 6 half-points = 3 pt over a 12 pt run = 25 %.
        assert_eq!(style_attr(&m, "charPosition"), Some(json!(25.0)));
        assert_eq!(style_attr(&m, "charScale"), Some(json!(150)));
        assert_eq!(style_attr(&m, "fontSizeCs"), Some(json!(10.0)));
        // Out-of-range w:w falls back to 100 % (i.e. no attribute).
        assert_eq!(style_attr(&run_marks(r#"<w:w w:val="900"/>"#, &[]), "charScale"), None);
    }
}
