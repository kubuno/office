//! `<w:drawing>` - DrawingML preset shapes and raster pictures.

use std::collections::HashMap;

use roxmltree::Node;
use serde_json::{json, Value};

use crate::converters::docx::xml::{attr_val, local};
use crate::converters::types::PmNode;

// English Metric Units per CSS pixel (914400 EMU/inch ÷ 96 px/inch).
pub(crate) const EMU_PER_PX: f64 = 9525.0;

/// Extract DrawingML preset shapes (`<w:drawing>` → `<a:prstGeom>`) from a
/// paragraph as block-level image nodes carrying `alt="kbshape:{kind,fill,stroke}"`
/// and no `src` — the editor regenerates the SVG from the alt at render time.
/// Raster pictures (`<pic:pic>` without preset geometry) are ignored here.
pub(crate) fn parse_drawings(
    p: &Node<'_, '_>,
    theme: &HashMap<String, String>,
    media: &HashMap<String, String>,
) -> Vec<PmNode> {
    let mut out = Vec::new();
    for d in p.descendants().filter(|n| local(n) == "drawing") {
        // Only ANCHORED objects (`<wp:anchor>`) become floating blocks. Objects that
        // are "in line with text" (`<wp:inline>`) are emitted INLINE by parse_run
        // (an image token in the flow, behaving like a character).
        for frame in d.descendants().filter(|n| local(n) == "anchor") {
            let is_anchor = local(&frame) == "anchor";

            // Size from <wp:extent cx= cy=> (EMU → px). Shared by pictures and shapes.
            let (mut w, mut h) = (240.0_f64, 180.0_f64);
            if let Some(ext) = frame.descendants().find(|n| local(n) == "extent") {
                if let Some(cx) = attr_val(&ext, "cx").and_then(|v| v.parse::<f64>().ok()) {
                    w = (cx / EMU_PER_PX).round().max(8.0);
                }
                if let Some(cy) = attr_val(&ext, "cy").and_then(|v| v.parse::<f64>().ok()) {
                    h = (cy / EMU_PER_PX).round().max(8.0);
                }
            }

            let mut attrs = serde_json::Map::new();
            attrs.insert("width".into(), json!(w));
            attrs.insert("height".into(), json!(h));

            // Drawing rotation (`<a:xfrm rot="…">`, in 1/60000th of a degree,
            // clockwise) — Word rotates the object; without it a picture shot in
            // landscape shows up lying on its side. `wp:extent` stays the UNROTATED
            // size (our engine reserves the rotated AABB at layout time).
            if let Some(rot) = frame
                .descendants()
                .find(|n| local(n) == "xfrm" && n.has_attribute("rot"))
                .and_then(|x| attr_val(&x, "rot"))
                .and_then(|v| v.parse::<f64>().ok())
            {
                let deg = (rot / 60000.0).rem_euclid(360.0);
                if deg.abs() > 0.01 {
                    attrs.insert("rotation".into(), json!(deg));
                }
            }

            // 1) Raster picture: <a:blip r:embed="rIdX"> → data URL from the media map.
            let blip_src = frame
                .descendants()
                .find(|n| local(n) == "blip")
                .and_then(|b| attr_val(&b, "embed"))
                .and_then(|rid| media.get(&rid).cloned());

            if let Some(src) = blip_src {
                attrs.insert("src".into(), json!(src));
                attrs.insert("alt".into(), json!(""));
            } else if let Some(prst) = frame
                .descendants()
                .find(|n| local(n) == "prstGeom")
                .and_then(|g| attr_val(&g, "prst"))
            {
                // 2) Preset-geometry shape → rendered through `kbshape:`.
                let kind = prst_to_kind(&prst);

                // Fill / stroke: the explicit `spPr` first, else the style reference
                // (`wps:style > fillRef/lnRef`), with theme colours resolved.
                let sppr = frame.descendants().find(|n| local(n) == "spPr");
                let style = frame.descendants().find(|n| local(n) == "style");
                let fill = sppr
                    .as_ref()
                    .and_then(|sp| sp.children().find(|n| n.is_element() && local(n) == "solidFill"))
                    .and_then(|sf| first_color_hex(&sf, theme))
                    .or_else(|| style.as_ref().and_then(|st| st.children().find(|n| local(n) == "fillRef")).and_then(|fr| first_color_hex(&fr, theme)))
                    .unwrap_or_else(|| default_shape_fill(kind).to_string());
                let stroke = sppr
                    .as_ref()
                    .and_then(|sp| sp.children().find(|n| n.is_element() && local(n) == "ln"))
                    .and_then(|ln| first_color_hex(&ln, theme))
                    .or_else(|| style.as_ref().and_then(|st| st.children().find(|n| local(n) == "lnRef")).and_then(|lr| first_color_hex(&lr, theme)))
                    .unwrap_or_else(|| "#1a73e8".to_string());

                // Stroke width: `<a:ln w="EMU">` (12700 EMU = 1 pt; px = EMU÷9525).
                // Stored as a FRACTION of the smallest dimension (invariant to the
                // resolution the SVG is generated at). Absent → frontend default.
                let sw_frac = sppr
                    .as_ref()
                    .and_then(|sp| sp.children().find(|n| n.is_element() && local(n) == "ln"))
                    .and_then(|ln| attr_val(&ln, "w"))
                    .and_then(|v| v.parse::<f64>().ok())
                    .map(|emu| (emu / EMU_PER_PX) / w.min(h));

                // alt = `kbshape:` + encodeURIComponent(JSON) — mirrors the frontend.
                let mut params = json!({ "kind": kind, "fill": fill, "stroke": stroke });
                if let Some(f) = sw_frac {
                    params["sw"] = json!((f * 10000.0).round() / 10000.0);
                }
                let alt = format!("kbshape:{}", urlencoding::encode(&params.to_string()));
                attrs.insert("src".into(), json!(""));
                attrs.insert("alt".into(), json!(alt));
            } else {
                continue; // neither a picture nor a shape with a known geometry
            }

            // ANCHORED (floating) object: text wrapping + position offset (EMU → px).
            if is_anchor {
                let behind = attr_val(&frame, "behindDoc").as_deref() == Some("1");
                let wrap = if frame.descendants().any(|n| local(&n) == "wrapNone") {
                    if behind { "behind" } else { "front" }
                } else if frame.descendants().any(|n| local(&n) == "wrapTopAndBottom") {
                    "topBottom"
                } else if frame.descendants().any(|n| {
                    matches!(local(&n), "wrapSquare" | "wrapTight" | "wrapThrough")
                }) {
                    "square"
                } else if behind {
                    "behind"
                } else {
                    "front"
                };
                let off = |which: &str| -> f64 {
                    frame
                        .descendants()
                        .find(|n| local(n) == which)
                        .and_then(|pos| pos.children().find(|c| local(c) == "posOffset"))
                        .and_then(|o| o.text())
                        .and_then(|t| t.trim().parse::<f64>().ok())
                        .map(|emu| (emu / EMU_PER_PX).round())
                        .unwrap_or(0.0)
                };
                attrs.insert("align".into(), json!("left"));
                attrs.insert("wrap".into(), json!(wrap));
                attrs.insert("wrapX".into(), json!(off("positionH").max(0.0)));
                attrs.insert("wrapY".into(), json!(off("positionV")));
            } else {
                attrs.insert("align".into(), json!("center"));
            }

            out.push(PmNode {
                node_type: "image".into(),
                attrs: Some(Value::Object(attrs)),
                content: None,
                marks: None,
                text: None,
            });
        }
    }
    out
}

/// First colour child of a fill/line/ref node → hex, resolving theme `schemeClr`.
pub(crate) fn first_color_hex(parent: &Node<'_, '_>, theme: &HashMap<String, String>) -> Option<String> {
    let c = parent
        .descendants()
        .find(|n| matches!(local(n), "srgbClr" | "schemeClr" | "sysClr"))?;
    let base = match local(&c) {
        "srgbClr" => attr_val(&c, "val")?,
        "sysClr" => attr_val(&c, "lastClr")?,
        "schemeClr" => {
            let raw = attr_val(&c, "val")?;
            // The theme colour mapping: tx1↔dk1, bg1↔lt1, tx2↔dk2, bg2↔lt2.
            let key = match raw.as_str() {
                "tx1" => "dk1", "bg1" => "lt1", "tx2" => "dk2", "bg2" => "lt2",
                other => other,
            };
            theme.get(key)?.clone()
        }
        _ => return None,
    };
    // Colour modifiers (shade/tint/lumMod/lumOff) — e.g. the STROKE of a shape is
    // accent + shade → darker than the fill (otherwise the border is invisible).
    Some(apply_color_mods(&base, &c))
}

/// Apply the OOXML modifiers (`a:shade`/`a:tint`/`a:lumMod`/`a:lumOff`) carried by a
/// colour element. Values are in thousandths of a % (50000 = 50%). Linear
/// approximation (good enough to tell the fill and the stroke apart).
pub(crate) fn apply_color_mods(hex: &str, c: &Node<'_, '_>) -> String {
    let h = hex.trim_start_matches('#');
    let parse = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0) as f64;
    let (mut r, mut g, mut b) = (parse(0), parse(2), parse(4));
    let val = |name: &str| -> Option<f64> {
        c.children()
            .find(|n| local(n) == name)
            .and_then(|n| attr_val(&n, "val"))
            .and_then(|v| v.parse::<f64>().ok())
            .map(|x| x / 100_000.0)
    };
    if let Some(s) = val("shade") {
        r *= s; g *= s; b *= s;                       // darkens towards black
    }
    if let Some(t) = val("tint") {
        r = r * t + 255.0 * (1.0 - t);                // lightens towards white
        g = g * t + 255.0 * (1.0 - t);
        b = b * t + 255.0 * (1.0 - t);
    }
    if let Some(lm) = val("lumMod") {
        r *= lm; g *= lm; b *= lm;
    }
    if let Some(lo) = val("lumOff") {
        r += 255.0 * lo; g += 255.0 * lo; b += 255.0 * lo;
    }
    let cl = |x: f64| x.round().clamp(0.0, 255.0) as u8;
    format!("#{:02X}{:02X}{:02X}", cl(r), cl(g), cl(b))
}

/// Connectors and lines have no fill; every other shape takes the editor's default
/// tint when the DOCX states no colour.
pub(crate) fn default_shape_fill(kind: &str) -> &'static str {
    match kind {
        "line" | "lineArrow" | "lineDouble" | "elbowConnector" | "elbowArrow"
        | "elbowDoubleArrow" | "curveConnector" | "curveArrow" | "curveDoubleArrow" | "curve" => "none",
        _ => "#dbe7ff",
    }
}

/// OOXML preset geometry (`a:prstGeom@prst`) → the editor's ShapeKind.
/// Fallback: `rect` (an unknown shape must never fail the import).
pub(crate) fn prst_to_kind(prst: &str) -> &'static str {
    match prst {
        // Rectangles
        "rect" => "rect", "roundRect" => "roundRect", "snip1Rect" => "snipRect",
        "snip2SameRect" => "snip2SameRect", "snip2DiagRect" => "snip2DiagRect", "snipRoundRect" => "snipRoundRect",
        "round1Rect" => "roundRect1", "round2SameRect" => "round2SameRect", "round2DiagRect" => "round2DiagRect", "plaque" => "plaque",
        // Basic shapes
        "ellipse" | "oval" => "ellipse", "triangle" => "triangle", "rtTriangle" => "rtTriangle",
        "parallelogram" => "parallelogram", "trapezoid" | "trapezoid2" => "trapezoid", "diamond" => "diamond",
        "pentagon" => "pentagon", "hexagon" => "hexagon", "heptagon" => "heptagon", "octagon" => "octagon",
        "decagon" => "decagon", "dodecagon" => "dodecagon",
        "pie" | "pieWedge" => "pie", "chord" => "chord", "teardrop" => "teardrop",
        "frame" => "frame", "halfFrame" => "halfFrame", "corner" => "corner", "diagStripe" => "diagStripe",
        "plus" => "cross", "bevel" => "bevel", "can" => "cylinder", "cube" => "cube",
        "blockArc" => "blockArc", "foldedCorner" => "foldedCorner",
        "heart" => "heart", "lightningBolt" => "lightning", "sun" => "sun", "moon" => "moon",
        "cloud" => "cloud", "smileyFace" => "smiley", "arc" => "arc", "donut" => "donut", "noSmoking" => "noSymbol",
        "leftBrace" => "leftBrace", "rightBrace" => "rightBrace", "leftBracket" => "leftBracket", "rightBracket" => "rightBracket",
        "bracePair" => "doubleBrace", "bracketPair" => "doubleBracket",
        // Solid arrows
        "rightArrow" => "arrow", "leftArrow" => "arrowLeft", "upArrow" => "arrowUp", "downArrow" => "arrowDown",
        "leftRightArrow" => "arrowLeftRight", "upDownArrow" => "arrowUpDown", "quadArrow" => "arrowQuad",
        "leftRightUpArrow" => "leftRightUpArrow", "bentArrow" => "bentArrow", "bentUpArrow" => "bentUpArrow",
        "uturnArrow" => "uTurnArrow", "curvedRightArrow" => "curvedRightArrow", "curvedLeftArrow" => "curvedLeftArrow",
        "curvedUpArrow" => "curvedUpArrow", "curvedDownArrow" => "curvedDownArrow",
        "stripedRightArrow" => "stripedRightArrow", "notchedRightArrow" => "notchedArrow",
        "homePlate" => "pentagonArrow", "chevron" => "chevron", "circularArrow" => "circularArrow",
        "rightArrowCallout" => "rightArrowCallout", "leftArrowCallout" => "leftArrowCallout",
        "upArrowCallout" => "upArrowCallout", "downArrowCallout" => "downArrowCallout",
        // Equation shapes
        "mathPlus" => "mathPlus", "mathMinus" => "mathMinus", "mathMultiply" => "mathMultiply",
        "mathDivide" => "mathDivide", "mathEqual" => "mathEqual", "mathNotEqual" => "mathNotEqual",
        // Flowcharts
        "flowChartProcess" => "flowProcess", "flowChartAlternateProcess" => "flowAltProcess",
        "flowChartDecision" => "flowDecision", "flowChartInputOutput" => "flowData",
        "flowChartPredefinedProcess" => "flowPredefined", "flowChartInternalStorage" => "flowInternal",
        "flowChartDocument" => "flowDocument", "flowChartMultidocument" => "flowMultidoc",
        "flowChartTerminator" => "flowTerminator", "flowChartPreparation" => "flowPreparation",
        "flowChartManualInput" => "flowManualInput", "flowChartManualOperation" => "flowManualOp",
        "flowChartConnector" => "flowConnector", "flowChartOffpageConnector" => "flowOffPage",
        "flowChartPunchedCard" => "flowCard", "flowChartPunchedTape" => "flowPunchedTape",
        "flowChartSummingJunction" => "flowSumming", "flowChartOr" => "flowOr",
        "flowChartCollate" => "flowCollate", "flowChartSort" => "flowSort",
        "flowChartExtract" => "flowExtract", "flowChartMerge" => "flowMerge",
        "flowChartOnlineStorage" => "flowStored", "flowChartDelay" => "flowDelay",
        "flowChartMagneticTape" => "flowSequential", "flowChartMagneticDisk" => "flowMagneticDisk",
        "flowChartMagneticDrum" => "flowDirectAccess", "flowChartDisplay" => "flowDisplay",
        // Stars and banners
        "star4" => "star4", "star5" => "star", "star6" => "star6", "star7" => "star7",
        "star8" => "star8", "star10" => "star10", "star12" => "star12", "star16" => "star16",
        "star24" => "star24", "star32" => "star32",
        "irregularSeal1" => "explosion1", "irregularSeal2" => "explosion2",
        "ribbon" => "ribbonDown", "ribbon2" => "ribbon", "ellipseRibbon" | "ellipseRibbon2" => "ribbonCurved",
        "verticalScroll" => "scrollV", "horizontalScroll" => "scrollH", "wave" => "wave", "doubleWave" => "doubleWave",
        // Callouts and speech bubbles
        "wedgeRectCallout" => "calloutRect", "wedgeRoundRectCallout" => "calloutRoundRect",
        "wedgeEllipseCallout" => "calloutOval", "cloudCallout" => "calloutCloud",
        "borderCallout1" | "callout1" => "lineCallout", "borderCallout2" | "callout2" => "calloutLine2",
        // Lines / connectors
        "line" | "straightConnector1" => "line",
        "bentConnector2" | "bentConnector3" | "bentConnector4" | "bentConnector5" => "elbowConnector",
        "curvedConnector2" | "curvedConnector3" | "curvedConnector4" | "curvedConnector5" => "curveConnector",
        _ => "rect",
    }
}

/// `inlineImage` node (inline atom, "in line with text") built from a `<wp:inline>`:
/// raster picture (blip→data-URL) or shape (`kbshape:`), rotation `<a:xfrm rot>`.
pub(crate) fn inline_image_node(
    frame: &Node<'_, '_>,
    theme: &HashMap<String, String>,
    media: &HashMap<String, String>,
) -> Option<PmNode> {
    let (mut w, mut h) = (200.0_f64, 150.0_f64);
    if let Some(ext) = frame.descendants().find(|n| local(n) == "extent") {
        if let Some(cx) = attr_val(&ext, "cx").and_then(|v| v.parse::<f64>().ok()) {
            w = (cx / EMU_PER_PX).round().max(4.0);
        }
        if let Some(cy) = attr_val(&ext, "cy").and_then(|v| v.parse::<f64>().ok()) {
            h = (cy / EMU_PER_PX).round().max(4.0);
        }
    }
    let mut attrs = serde_json::Map::new();
    attrs.insert("width".into(), json!(w));
    attrs.insert("height".into(), json!(h));
    if let Some(rot) = frame
        .descendants()
        .find(|n| local(n) == "xfrm" && n.has_attribute("rot"))
        .and_then(|x| attr_val(&x, "rot"))
        .and_then(|v| v.parse::<f64>().ok())
    {
        let deg = (rot / 60000.0).rem_euclid(360.0);
        if deg.abs() > 0.01 {
            attrs.insert("rotation".into(), json!(deg));
        }
    }
    // Raster picture (blip), else a preset-geometry shape.
    let blip_src = frame
        .descendants()
        .find(|n| local(n) == "blip")
        .and_then(|b| attr_val(&b, "embed"))
        .and_then(|rid| media.get(&rid).cloned());
    if let Some(src) = blip_src {
        attrs.insert("src".into(), json!(src));
        attrs.insert("alt".into(), json!(""));
    } else if let Some(prst) = frame
        .descendants()
        .find(|n| local(n) == "prstGeom")
        .and_then(|g| attr_val(&g, "prst"))
    {
        let kind = prst_to_kind(&prst);
        let sppr = frame.descendants().find(|n| local(n) == "spPr");
        let style = frame.descendants().find(|n| local(n) == "style");
        let fill = sppr
            .as_ref()
            .and_then(|sp| sp.children().find(|n| n.is_element() && local(n) == "solidFill"))
            .and_then(|sf| first_color_hex(&sf, theme))
            .or_else(|| style.as_ref().and_then(|st| st.children().find(|n| local(n) == "fillRef")).and_then(|fr| first_color_hex(&fr, theme)))
            .unwrap_or_else(|| default_shape_fill(kind).to_string());
        let stroke = sppr
            .as_ref()
            .and_then(|sp| sp.children().find(|n| n.is_element() && local(n) == "ln"))
            .and_then(|ln| first_color_hex(&ln, theme))
            .or_else(|| style.as_ref().and_then(|st| st.children().find(|n| local(n) == "lnRef")).and_then(|lr| first_color_hex(&lr, theme)))
            .unwrap_or_else(|| "#1a73e8".to_string());
        let params = json!({ "kind": kind, "fill": fill, "stroke": stroke });
        attrs.insert("src".into(), json!(""));
        attrs.insert("alt".into(), json!(format!("kbshape:{}", urlencoding::encode(&params.to_string()))));
    } else {
        return None;
    }
    Some(PmNode {
        node_type: "inlineImage".into(),
        attrs: Some(Value::Object(attrs)),
        content: None,
        marks: None,
        text: None,
    })
}
