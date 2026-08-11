//! `<w:drawing>` - DrawingML preset shapes and raster pictures.

use std::collections::HashMap;

use roxmltree::Node;
use serde_json::{json, Value};

use crate::converters::docx::xml::{attr_val, local};
use crate::converters::types::PmNode;

// English Metric Units per CSS pixel (914400 EMU/inch ÷ 96 px/inch).
pub(crate) const EMU_PER_PX: f64 = 9525.0;

/// `wp:wrapPolygon` coordinates live in a FIXED 21600x21600 space mapped onto
/// the object box, whatever the real size of the object
/// (`WrapPolygon::correctWordWrapPolygonPixel`, WrapPolygonHandler.cxx:112-133).
const WRAP_POLY_SPAN: f64 = 21600.0;

/// A wrap polygon is a hand-drawn contour, not a mesh: Word's own editor caps it
/// far below this. The guard only stops a corrupt file from inflating the JSON.
const WRAP_POLY_MAX_POINTS: usize = 2048;

/// Word reads `relativeHeight` 0, 1 and anything above `0x1DFFFFFF` as "topmost"
/// (GraphicImport.cxx:682-690).
const Z_ORDER_MAX: i64 = 0x1DFF_FFFF;

/// The `EG_WrapType` alternatives of `CT_Anchor` (ECMA-376 §20.4.2.15-19).
const WRAP_ELEMENTS: [&str; 5] = [
    "wrapNone",
    "wrapSquare",
    "wrapTight",
    "wrapThrough",
    "wrapTopAndBottom",
];

/// Extract every ANCHORED `<w:drawing>` of a paragraph as a block-level `image`
/// node: raster pictures (`<a:blip>` → data URL), preset shapes (`<a:prstGeom>`
/// → `alt="kbshape:{kind,fill,stroke}"` and no `src`, the editor regenerates the
/// SVG from the alt), and — as an empty frame — the objects whose bytes live in
/// package parts this reader does not open (SmartArt, charts, OLE).
/// Each node also carries the full anchor layout (see `apply_anchor_attrs`).
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
                attrs.insert("src".into(), json!(""));
                attrs.insert("alt".into(), json!(shape_alt(&frame, theme, prst_to_kind(&prst), Some((w, h)))));
            } else if frame.descendants().any(|n| local(&n) == "custGeom") {
                // 3) Freeform shape (`a:custGeom`): the path is a `a:pathLst` we do
                //    not rasterise. Word itself falls back to the bounding box for
                //    an unknown geometry — keep the frame, its colours and its
                //    wrapping rather than losing the object.
                attrs.insert("src".into(), json!(""));
                attrs.insert("alt".into(), json!(shape_alt(&frame, theme, "rect", Some((w, h)))));
            } else {
                // 4) SmartArt (`dgm:relIds`), charts, OLE frames, locked canvases:
                //    the picture lives in package parts this reader does not open.
                //    Writer routes them through the SAME anchoring code as a plain
                //    picture (GraphicImport.cxx:1561, `LN_dgm_relIds` sits in the
                //    generic switch), so dropping the node would lose both the
                //    content AND the room it claims in the text flow. Keep an empty
                //    frame carrying the object's own label.
                attrs.insert("src".into(), json!(""));
                attrs.insert("alt".into(), json!(placeholder_alt(&frame)));
            }

            // ANCHORED (floating) object: wrapping, distances, position, z-order.
            if is_anchor {
                apply_anchor_attrs(&frame, &mut attrs, w, h);
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

// ── Anchored (floating) objects ─────────────────────────────────────────────

/// Every `CT_Anchor` part named `name` that belongs to THIS anchor. A drawing
/// nested in a text box (`wps:txbx`) carries its own `wp:anchor`/`wp:inline`
/// with its own position and wrapping: a plain `descendants()` would let the
/// inner object's properties overwrite the outer one's.
///
/// The search cannot be limited to direct children: Word wraps `wp:positionH`
/// in an `mc:AlternateContent` whenever the offset is a percentage
/// (`wp14:pctPosHOffset` in `mc:Choice`, the absolute offset in `mc:Fallback`).
fn own_parts<'a, 'd>(frame: &Node<'a, 'd>, name: &str) -> Vec<Node<'a, 'd>> {
    frame
        .descendants()
        .filter(|n| local(n) == name)
        .filter(|n| {
            n.ancestors()
                .skip(1)
                .take_while(|a| a != frame)
                .all(|a| !matches!(local(&a), "anchor" | "inline"))
        })
        .collect()
}

fn own_part<'a, 'd>(frame: &Node<'a, 'd>, name: &str) -> Option<Node<'a, 'd>> {
    own_parts(frame, name).into_iter().next()
}

/// Read every layout property of a `<wp:anchor>` into the image node.
fn apply_anchor_attrs(frame: &Node<'_, '_>, attrs: &mut serde_json::Map<String, Value>, w: f64, h: f64) {
    let behind = matches!(attr_val(frame, "behindDoc").as_deref(), Some("1") | Some("true"));
    let wrap_el = WRAP_ELEMENTS
        .iter()
        .find_map(|name| own_part(frame, name));
    let mode = wrap_mode(wrap_el.as_ref(), behind);

    attrs.insert("align".into(), json!("left"));
    attrs.insert("wrap".into(), json!(mode));

    // Which side(s) the text may flow on — `@wrapText` of the wrap element
    // (`handleWrapTextValue`, GraphicImport.cxx:385-403).
    if let Some(side) = wrap_el.as_ref().and_then(wrap_side) {
        attrs.insert("wrapSide".into(), json!(side));
    }

    // Object-to-text distances. `wrapNone` means the text runs over the object,
    // so Word ignores the distances entirely (GraphicImport.cxx:1686-1694).
    if mode != "behind" && mode != "front" {
        for (attr, key) in [
            ("distT", "wrapDistT"),
            ("distB", "wrapDistB"),
            ("distL", "wrapDistL"),
            ("distR", "wrapDistR"),
        ] {
            if let Some(px) = dist_px(frame, wrap_el.as_ref(), attr) {
                attrs.insert(key.into(), json!(px));
            }
        }
    }

    // Contour of a `wrapTight` / `wrapThrough`: the polygon the file carries is
    // exact, where sampling the alpha channel is a guess.
    if let Some(poly) = wrap_el.as_ref().and_then(|el| wrap_polygon(el, w, h)) {
        attrs.insert("wrapPolygon".into(), json!(poly));
    }

    // Position. `simplePos="1"` overrides both axes with a single page-relative
    // point (`m_bUseSimplePos`, GraphicImport.cxx:679 and 1727-1736).
    let simple = matches!(attr_val(frame, "simplePos").as_deref(), Some("1") | Some("true"))
        .then(|| own_part(frame, "simplePos"))
        .flatten();
    if let Some(sp) = simple {
        // Same whole-pixel rounding as the `wp:posOffset` branch below.
        attrs.insert("wrapX".into(), json!(emu_px(attr_val(&sp, "x")).unwrap_or(0.0).round()));
        attrs.insert("wrapY".into(), json!(emu_px(attr_val(&sp, "y")).unwrap_or(0.0).round()));
        attrs.insert("posHRel".into(), json!("page"));
        attrs.insert("posVRel".into(), json!("page"));
    } else {
        for (elem, off_key, rel_key) in [
            ("positionH", "wrapX", "posHRel"),
            ("positionV", "wrapY", "posVRel"),
        ] {
            let horizontal = elem == "positionH";
            let pos = own_part(frame, elem);
            let rel = pos
                .as_ref()
                .and_then(|p| attr_val(p, "relativeFrom"))
                .map(|v| if horizontal { rel_from_h(&v) } else { rel_from_v(&v) })
                .unwrap_or(if horizontal { "column" } else { "paragraph" });
            attrs.insert(rel_key.into(), json!(rel));

            // `wp:align` and `wp:posOffset` are exclusive; when the file gives an
            // alignment there is no offset to read (GraphicHelpers.cxx:150-186).
            let align = pos
                .as_ref()
                .and_then(|p| p.descendants().find(|n| local(n) == "align"))
                .and_then(|a| a.text().map(|t| t.trim().to_string()));
            let mapped = align.as_deref().and_then(|a| {
                if horizontal {
                    align_h(a)
                } else {
                    align_v(a, rel)
                }
            });
            if let Some(a) = mapped {
                attrs.insert(if horizontal { "alignH".into() } else { "alignV".into() }, json!(a));
            }

            // A NEGATIVE offset is legal on both axes: Word lets an object bleed
            // into the margin. Only `wp:posOffset` counts here — the percentage
            // form (`wp14:pctPosHOffset`) has no pixel value.
            let off = pos
                .as_ref()
                .and_then(|p| p.descendants().find(|n| local(n) == "posOffset"))
                .and_then(|o| o.text())
                .and_then(|t| emu_px(Some(t.to_string())))
                .unwrap_or(0.0);
            attrs.insert(off_key.into(), json!(off.round()));
        }
    }

    // Paint order. Word only compares these values; the frontend needs one to
    // tell which of two overlapping objects the pointer is on.
    if let Some(z) = z_order(frame) {
        attrs.insert("zOrder".into(), json!(z));
    }
    // Anchor behaviour flags (defaults from `CT_Anchor`: allowOverlap is
    // REQUIRED, locked defaults to false).
    if matches!(attr_val(frame, "allowOverlap").as_deref(), Some("0") | Some("false")) {
        attrs.insert("allowOverlap".into(), json!(false));
    }
    if matches!(attr_val(frame, "locked").as_deref(), Some("1") | Some("true")) {
        attrs.insert("lockAnchor".into(), json!(true));
    }
}

/// EMU string → CSS pixels, keeping two decimals (114300 EMU = exactly 12 px).
fn emu_px(v: Option<String>) -> Option<f64> {
    let emu = v?.trim().parse::<f64>().ok()?;
    if !emu.is_finite() {
        return None;
    }
    Some((emu / EMU_PER_PX * 100.0).round() / 100.0)
}

/// `EG_WrapType` → the editor's wrap mode. A missing wrap element means the same
/// as `wrapNone`: the text ignores the object and `behindDoc` alone decides
/// which layer it is painted on (GraphicImport.cxx:1686-1694).
fn wrap_mode(el: Option<&Node<'_, '_>>, behind: bool) -> &'static str {
    match el.map(local) {
        Some("wrapSquare") => "square",
        Some("wrapTight") => "tight",
        Some("wrapThrough") => "through",
        Some("wrapTopAndBottom") => "topBottom",
        _ if behind => "behind",
        _ => "front",
    }
}

/// `@wrapText` (`ST_WrapText`) → the side(s) text may flow on.
fn wrap_side(el: &Node<'_, '_>) -> Option<&'static str> {
    match attr_val(el, "wrapText").as_deref() {
        Some("bothSides") => Some("both"),
        Some("left") => Some("left"),
        Some("right") => Some("right"),
        Some("largest") => Some("largest"),
        _ => None,
    }
}

/// One object-to-text distance in pixels. `CT_WrapSquare`/`CT_WrapTight`/
/// `CT_WrapThrough` may restate `distT`/`distB`/`distL`/`distR`, in which case
/// their value wins over the anchor's. `ST_WrapDistance` is unsigned.
fn dist_px(frame: &Node<'_, '_>, wrap_el: Option<&Node<'_, '_>>, name: &str) -> Option<f64> {
    let raw = wrap_el
        .and_then(|e| attr_val(e, name))
        .or_else(|| attr_val(frame, name))?;
    emu_px(Some(raw)).map(|px| px.max(0.0))
}

/// `wp:wrapPolygon` → the contour in pixels inside the object box.
fn wrap_polygon(el: &Node<'_, '_>, w: f64, h: f64) -> Option<Value> {
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let poly = el
        .children()
        .find(|n| n.is_element() && local(n) == "wrapPolygon")?;
    let mut pts: Vec<Value> = Vec::new();
    for p in poly
        .children()
        .filter(|n| n.is_element() && matches!(local(n), "start" | "lineTo"))
    {
        let (Some(x), Some(y)) = (
            attr_val(&p, "x").and_then(|v| v.trim().parse::<f64>().ok()),
            attr_val(&p, "y").and_then(|v| v.trim().parse::<f64>().ok()),
        ) else {
            continue;
        };
        let px = |v: f64, span: f64| (v / WRAP_POLY_SPAN * span * 100.0).round() / 100.0;
        pts.push(json!({ "x": px(x, w), "y": px(y, h) }));
        if pts.len() >= WRAP_POLY_MAX_POINTS {
            break;
        }
    }
    // Fewer than three points cannot bound an area.
    (pts.len() >= 3).then_some(Value::Array(pts))
}

/// `wp:anchor/@relativeHeight` → paint order.
fn z_order(frame: &Node<'_, '_>) -> Option<i64> {
    let v = attr_val(frame, "relativeHeight")?.trim().parse::<i64>().ok()?;
    Some(if !(2..=Z_ORDER_MAX).contains(&v) { Z_ORDER_MAX } else { v })
}

/// `wp:positionH/@relativeFrom` (`ST_RelFromH`). The four margin-band values
/// have no equivalent in the editor's vocabulary (`column|margin|page|character`)
/// and collapse onto `margin`, the closest reference frame — LibreOffice spreads
/// them over PAGE_LEFT / PAGE_RIGHT / PAGE_FRAME (GraphicHelpers.cxx:96-133).
fn rel_from_h(v: &str) -> &'static str {
    match v {
        "page" => "page",
        "character" => "character",
        "margin" | "leftMargin" | "rightMargin" | "insideMargin" | "outsideMargin" => "margin",
        // `column` is the Word default and equals the content area in a
        // single-column section.
        _ => "column",
    }
}

/// `wp:positionV/@relativeFrom` (`ST_RelFromV`).
fn rel_from_v(v: &str) -> &'static str {
    match v {
        "page" => "page",
        "line" => "line",
        "margin" | "topMargin" | "bottomMargin" | "insideMargin" | "outsideMargin" => "margin",
        _ => "paragraph",
    }
}

/// `wp:positionH/wp:align` (`ST_AlignH`). `inside`/`outside` depend on the page
/// parity, which the layout engine does not track: they resolve as on an
/// odd page (GraphicHelpers.cxx:154-170).
fn align_h(v: &str) -> Option<&'static str> {
    match v {
        "left" | "inside" => Some("left"),
        "center" => Some("center"),
        "right" | "outside" => Some("right"),
        _ => None,
    }
}

/// `wp:positionV/wp:align` (`ST_AlignV`). Relative to the LINE of text the
/// meaning of top and bottom is reversed — `top` puts the object ON TOP of the
/// line (`PositionHandler::orientation`, GraphicHelpers.cxx:189-200).
fn align_v(v: &str, rel: &str) -> Option<&'static str> {
    let a = match v {
        "top" | "inside" => "top",
        "center" => "center",
        "bottom" | "outside" => "bottom",
        _ => return None,
    };
    Some(match (rel, a) {
        ("line", "top") => "bottom",
        ("line", "bottom") => "top",
        _ => a,
    })
}

/// Label of an object we cannot draw: its own accessibility text, else the name
/// Word gave it ("Diagram 1", "Chart 2"), else the family of the graphic.
fn placeholder_alt(frame: &Node<'_, '_>) -> String {
    let doc_pr = own_part(frame, "docPr");
    let named = |k: &str| {
        doc_pr
            .as_ref()
            .and_then(|d| attr_val(d, k))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    named("descr")
        .or_else(|| named("name"))
        .unwrap_or_else(|| graphic_family(frame).to_string())
}

/// `a:graphicData/@uri` → the family of the object, the only hint the body gives
/// about a graphic whose data lives in another package part.
fn graphic_family(frame: &Node<'_, '_>) -> &'static str {
    let uri = own_part(frame, "graphicData")
        .and_then(|g| attr_val(&g, "uri"))
        .unwrap_or_default();
    match () {
        _ if uri.ends_with("/diagram") => "Diagram",
        _ if uri.contains("/chart") => "Chart",
        _ if uri.contains("/ole") || uri.contains("OLEObject") => "Embedded object",
        _ if uri.ends_with("/table") => "Table",
        _ => "Drawing",
    }
}

/// `alt` payload of a shape node: `kbshape:` + encodeURIComponent(JSON) —
/// mirrors what the frontend generates, since it regenerates the SVG from it.
/// `size` (display width/height in px) turns the stroke width into a fraction;
/// inline shapes pass `None` and take the editor's default.
fn shape_alt(
    frame: &Node<'_, '_>,
    theme: &HashMap<String, String>,
    kind: &str,
    size: Option<(f64, f64)>,
) -> String {
    // Fill / stroke: the explicit `spPr` first, else the style reference
    // (`wps:style > fillRef/lnRef`), with theme colours resolved.
    let sppr = frame.descendants().find(|n| local(n) == "spPr");
    let style = frame.descendants().find(|n| local(n) == "style");
    let ln = sppr
        .as_ref()
        .and_then(|sp| sp.children().find(|n| n.is_element() && local(n) == "ln"));
    let fill = sppr
        .as_ref()
        .and_then(|sp| sp.children().find(|n| n.is_element() && local(n) == "solidFill"))
        .and_then(|sf| first_color_hex(&sf, theme))
        .or_else(|| {
            style
                .as_ref()
                .and_then(|st| st.children().find(|n| local(n) == "fillRef"))
                .and_then(|fr| first_color_hex(&fr, theme))
        })
        .unwrap_or_else(|| default_shape_fill(kind).to_string());
    let stroke = ln
        .as_ref()
        .and_then(|l| first_color_hex(l, theme))
        .or_else(|| {
            style
                .as_ref()
                .and_then(|st| st.children().find(|n| local(n) == "lnRef"))
                .and_then(|lr| first_color_hex(&lr, theme))
        })
        .unwrap_or_else(|| "#1a73e8".to_string());

    let mut params = json!({ "kind": kind, "fill": fill, "stroke": stroke });
    // Stroke width: `<a:ln w="EMU">` (12700 EMU = 1 pt; px = EMU÷9525). Stored
    // as a FRACTION of the smallest dimension so it stays valid whatever
    // resolution the SVG is generated at.
    if let (Some((w, h)), Some(emu)) = (
        size,
        ln.and_then(|l| attr_val(&l, "w")).and_then(|v| v.parse::<f64>().ok()),
    ) {
        let min = w.min(h);
        if min > 0.0 {
            let f = (emu / EMU_PER_PX) / min;
            params["sw"] = json!((f * 10000.0).round() / 10000.0);
        }
    }
    format!("kbshape:{}", urlencoding::encode(&params.to_string()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use roxmltree::Document as XmlDoc;

    /// Wrap a `<w:drawing>` body in the namespaces Word declares on `w:document`
    /// so the fragment parses exactly like the real thing.
    fn parse(drawing: &str) -> Vec<Value> {
        let xml = format!(
            concat!(
                r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" "#,
                r#"xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" "#,
                r#"xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" "#,
                r#"xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape" "#,
                r#"xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture" "#,
                r#"xmlns:dgm="http://schemas.openxmlformats.org/drawingml/2006/diagram" "#,
                r#"xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" "#,
                r#"xmlns:wp14="http://schemas.microsoft.com/office/word/2010/wordml" "#,
                r#"xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">"#,
                "<w:body><w:p><w:r>{}</w:r></w:p></w:body></w:document>",
            ),
            drawing
        );
        let doc = XmlDoc::parse(&xml).expect("XML de test valide");
        let p = doc
            .descendants()
            .find(|n| local(n) == "p")
            .expect("<w:p>");
        let media = HashMap::from([("rId7".to_string(), "data:image/png;base64,AAA".to_string())]);
        parse_drawings(&p, &HashMap::new(), &media)
            .into_iter()
            .filter_map(|n| n.attrs)
            .collect()
    }

    /// Default position block of the fixture: 100 px right, 20 px down.
    const POS: &str = concat!(
        r#"<wp:positionH relativeFrom="column"><wp:posOffset>952500</wp:posOffset></wp:positionH>"#,
        r#"<wp:positionV relativeFrom="paragraph"><wp:posOffset>190500</wp:posOffset></wp:positionV>"#,
    );

    /// A `<wp:anchor>` around `body`, with the attributes Word always writes.
    /// `pos` replaces the default `wp:positionH`/`wp:positionV` block.
    fn anchor(attrs: &str, pos: &str, body: &str) -> String {
        format!(
            concat!(
                r#"<w:drawing><wp:anchor distT="45720" distB="45720" distL="114300" distR="114300" "#,
                r#"simplePos="0" relativeHeight="251659264" locked="0" "#,
                r#"layoutInCell="1" allowOverlap="1" {attrs}>"#,
                r#"<wp:simplePos x="0" y="0"/>{pos}"#,
                r#"<wp:extent cx="1143000" cy="762000"/>"#,
                r#"<wp:docPr id="1" name="Objet 1"/>{body}</wp:anchor></w:drawing>"#,
            ),
            attrs = if attrs.is_empty() { r#"behindDoc="0""# } else { attrs },
            pos = if pos.is_empty() { POS } else { pos },
            body = body,
        )
    }

    /// A minimal raster picture body.
    const PIC: &str = concat!(
        r#"<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">"#,
        r#"<pic:pic><pic:blipFill><a:blip r:embed="rId7"/></pic:blipFill></pic:pic>"#,
        "</a:graphicData></a:graphic>",
    );

    fn s<'a>(a: &'a Value, k: &str) -> &'a str {
        a.get(k).and_then(|v| v.as_str()).unwrap_or("<absent>")
    }

    /// Défaut n°1 de l'audit : les trois modes tombaient tous sur "square".
    #[test]
    fn the_three_wrap_modes_stay_distinct() {
        let cases = [
            (r#"<wp:wrapSquare wrapText="bothSides"/>"#, "square"),
            (
                r#"<wp:wrapTight wrapText="left"><wp:wrapPolygon edited="0"><wp:start x="0" y="0"/></wp:wrapPolygon></wp:wrapTight>"#,
                "tight",
            ),
            (r#"<wp:wrapThrough wrapText="right"/>"#, "through"),
            (r#"<wp:wrapTopAndBottom/>"#, "topBottom"),
            (r#"<wp:wrapNone/>"#, "front"),
        ];
        for (wrap, expect) in cases {
            let got = parse(&anchor("", "", &format!("{wrap}{PIC}")));
            assert_eq!(got.len(), 1, "{wrap}");
            assert_eq!(s(&got[0], "wrap"), expect, "{wrap}");
        }
        // `behindDoc` only flips `wrapNone` between the two layers.
        let behind = parse(&anchor(r#"behindDoc="1""#, "", &format!("<wp:wrapNone/>{PIC}")));
        assert_eq!(s(&behind[0], "wrap"), "behind");
    }

    /// Défaut n°8 : distances et côté d'habillage jamais lus.
    #[test]
    fn distances_and_wrap_side_come_from_the_file() {
        let got = parse(&anchor("", "", &format!(r#"<wp:wrapSquare wrapText="largest"/>{PIC}"#)));
        let a = &got[0];
        assert_eq!(s(a, "wrapSide"), "largest");
        // 114300 EMU = 12 px exactement, 45720 EMU = 4.8 px.
        assert_eq!(a["wrapDistL"], json!(12.0));
        assert_eq!(a["wrapDistR"], json!(12.0));
        assert_eq!(a["wrapDistT"], json!(4.8));
        assert_eq!(a["wrapDistB"], json!(4.8));

        // `wrapNone` : Word ignore les distances (GraphicImport.cxx:1686-1694).
        let none = parse(&anchor("", "", &format!("<wp:wrapNone/>{PIC}")));
        assert!(none[0].get("wrapDistL").is_none(), "{:?}", none[0]);

        // Une distance portée par l'élément d'habillage l'emporte sur l'ancre.
        let over = parse(&anchor(
            "",
            "",
            &format!(r#"<wp:wrapSquare wrapText="bothSides" distL="228600"/>{PIC}"#),
        ));
        assert_eq!(over[0]["wrapDistL"], json!(24.0));
        assert_eq!(over[0]["wrapDistR"], json!(12.0));
    }

    /// Défaut n°6 : référentiels de position et `wp:align` ignorés.
    #[test]
    fn position_reference_frames_and_alignments() {
        let pos = concat!(
            r#"<wp:positionH relativeFrom="page"><wp:align>right</wp:align></wp:positionH>"#,
            r#"<wp:positionV relativeFrom="line"><wp:align>top</wp:align></wp:positionV>"#,
        );
        // Cet ancrage-ci remplace les positions par défaut du gabarit.
        let xml = format!(
            concat!(
                r#"<w:drawing><wp:anchor distL="0" distR="0" simplePos="0" relativeHeight="7" "#,
                r#"behindDoc="0" locked="1" layoutInCell="1" allowOverlap="0">"#,
                r#"<wp:simplePos x="0" y="0"/>{pos}<wp:extent cx="1143000" cy="762000"/>"#,
                r#"<wp:wrapSquare wrapText="bothSides"/><wp:docPr id="1" name="x"/>{pic}"#,
                "</wp:anchor></w:drawing>",
            ),
            pos = pos,
            pic = PIC,
        );
        let got = parse(&xml);
        let a = &got[0];
        assert_eq!(s(a, "posHRel"), "page");
        assert_eq!(s(a, "posVRel"), "line");
        assert_eq!(s(a, "alignH"), "right");
        // Relative à la LIGNE, « top » veut dire au-dessus : haut ↔ bas inversés.
        assert_eq!(s(a, "alignV"), "bottom");
        assert_eq!(a["zOrder"], json!(7));
        assert_eq!(a["allowOverlap"], json!(false));
        assert_eq!(a["lockAnchor"], json!(true));

        // Les bandes de marge se replient sur `margin`, faute de vocabulaire.
        let margins = parse(&anchor(
            "",
            concat!(
                r#"<wp:positionH relativeFrom="rightMargin"><wp:posOffset>-95250</wp:posOffset></wp:positionH>"#,
                r#"<wp:positionV relativeFrom="topMargin"><wp:posOffset>0</wp:posOffset></wp:positionV>"#,
            ),
            &format!("<wp:wrapNone/>{PIC}"),
        ));
        assert_eq!(s(&margins[0], "posHRel"), "margin");
        assert_eq!(s(&margins[0], "posVRel"), "margin");
        assert_eq!(margins[0]["wrapX"], json!(-10.0));

        // Sans référentiel déclaré, les défauts de Word : colonne / paragraphe.
        let dflt = parse(&anchor("", "", &format!("<wp:wrapNone/>{PIC}")));
        assert_eq!(s(&dflt[0], "posHRel"), "column");
        assert_eq!(s(&dflt[0], "posVRel"), "paragraph");
        assert_eq!(dflt[0]["wrapX"], json!(100.0)); // 952500 EMU
        assert_eq!(dflt[0]["wrapY"], json!(20.0));
    }

    #[test]
    fn negative_offsets_are_kept_on_both_axes() {
        let xml = format!(
            concat!(
                r#"<w:drawing><wp:anchor simplePos="0" relativeHeight="2" behindDoc="0" "#,
                r#"locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/>"#,
                r#"<wp:positionH relativeFrom="column"><wp:posOffset>-476250</wp:posOffset></wp:positionH>"#,
                r#"<wp:positionV relativeFrom="paragraph"><wp:posOffset>-95250</wp:posOffset></wp:positionV>"#,
                r#"<wp:extent cx="914400" cy="914400"/><wp:wrapNone/><wp:docPr id="2" name="y"/>{pic}"#,
                "</wp:anchor></w:drawing>",
            ),
            pic = PIC
        );
        let got = parse(&xml);
        assert_eq!(got[0]["wrapX"], json!(-50.0));
        assert_eq!(got[0]["wrapY"], json!(-10.0));
    }

    /// `simplePos="1"` : une position unique, relative à la PAGE.
    #[test]
    fn simple_pos_wins_over_both_axes() {
        let xml = format!(
            concat!(
                r#"<w:drawing><wp:anchor simplePos="1" relativeHeight="3" behindDoc="0" "#,
                r#"locked="0" layoutInCell="1" allowOverlap="1">"#,
                r#"<wp:simplePos x="3771900" y="2169795"/>"#,
                r#"<wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH>"#,
                r#"<wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV>"#,
                r#"<wp:extent cx="914400" cy="914400"/><wp:wrapTopAndBottom/>"#,
                r#"<wp:docPr id="3" name="z"/>{pic}</wp:anchor></w:drawing>"#,
            ),
            pic = PIC
        );
        let got = parse(&xml);
        assert_eq!(got[0]["wrapX"], json!(396.0));
        assert_eq!(got[0]["wrapY"], json!(228.0));
        assert_eq!(s(&got[0], "posHRel"), "page");
        assert_eq!(s(&got[0], "posVRel"), "page");
    }

    /// Défaut n°10 : le polygone d'habillage du fichier, en pixels de l'objet.
    /// Valeurs réelles de `fdo79129.docx` (espace fixe 21600 × 21600).
    #[test]
    fn wrap_polygon_is_scaled_into_the_object_box() {
        let poly = concat!(
            r#"<wp:wrapTight wrapText="bothSides"><wp:wrapPolygon edited="0">"#,
            r#"<wp:start x="-203" y="3028"/><wp:lineTo x="10800" y="10800"/>"#,
            r#"<wp:lineTo x="21600" y="21600"/><wp:lineTo x="-203" y="3028"/>"#,
            "</wp:wrapPolygon></wp:wrapTight>",
        );
        let got = parse(&anchor("", "", &format!("{poly}{PIC}")));
        let a = &got[0];
        // extent 1143000 × 762000 EMU = 120 × 80 px.
        assert_eq!(a["width"], json!(120.0));
        let pts = a["wrapPolygon"].as_array().expect("polygone");
        assert_eq!(pts.len(), 4);
        assert_eq!(pts[0], json!({ "x": -1.13, "y": 11.21 }));
        assert_eq!(pts[1], json!({ "x": 60.0, "y": 40.0 }));
        assert_eq!(pts[2], json!({ "x": 120.0, "y": 80.0 }));

        // Moins de trois points ne borne aucune aire : pas d'attribut.
        let thin = parse(&anchor(
            "",
            "",
            &format!(
                r#"<wp:wrapTight wrapText="bothSides"><wp:wrapPolygon><wp:start x="0" y="0"/></wp:wrapPolygon></wp:wrapTight>{PIC}"#
            ),
        ));
        assert!(thin[0].get("wrapPolygon").is_none());
    }

    /// `relativeHeight` 0, 1 et > 0x1DFFFFFF valent « au-dessus de tout ».
    #[test]
    fn z_order_extremes_are_clamped_like_word() {
        for (raw, expect) in [("0", Z_ORDER_MAX), ("1", Z_ORDER_MAX), ("251658240", 251658240)] {
            let xml = format!(
                concat!(
                    r#"<w:drawing><wp:anchor simplePos="0" relativeHeight="{raw}" behindDoc="0" "#,
                    r#"locked="0" layoutInCell="1" allowOverlap="1"><wp:extent cx="914400" cy="914400"/>"#,
                    r#"<wp:wrapNone/><wp:docPr id="4" name="w"/>{pic}</wp:anchor></w:drawing>"#,
                ),
                raw = raw,
                pic = PIC
            );
            let got = parse(&xml);
            assert_eq!(got[0]["zOrder"], json!(expect), "relativeHeight={raw}");
        }
    }

    /// Défaut n°2 : un SmartArt ne produisait AUCUN nœud.
    #[test]
    fn a_diagram_keeps_its_frame_and_its_wrapping() {
        let dgm = concat!(
            r#"<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/diagram">"#,
            r#"<dgm:relIds r:dm="rId8" r:lo="rId9" r:qs="rId10" r:cs="rId11"/>"#,
            "</a:graphicData></a:graphic>",
        );
        let xml = format!(
            concat!(
                r#"<w:drawing><wp:anchor distL="114300" distR="114300" simplePos="0" "#,
                r#"relativeHeight="251658240" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1">"#,
                r#"<wp:extent cx="4052570" cy="2021205"/><wp:wrapSquare wrapText="bothSides"/>"#,
                r#"<wp:docPr id="6" name="Diagram 1"/>{dgm}</wp:anchor></w:drawing>"#,
            ),
            dgm = dgm
        );
        let got = parse(&xml);
        assert_eq!(got.len(), 1, "le diagramme doit produire un cadre");
        let a = &got[0];
        assert_eq!(s(a, "src"), "");
        assert_eq!(s(a, "alt"), "Diagram 1");
        assert_eq!(s(a, "wrap"), "square");
        assert_eq!(a["width"], json!(425.0));

        // Sans `wp:docPr`, l'étiquette vient de la famille du graphique.
        let bare = format!(
            concat!(
                r#"<w:drawing><wp:anchor simplePos="0" behindDoc="0" locked="0" layoutInCell="1" "#,
                r#"allowOverlap="1"><wp:extent cx="914400" cy="914400"/><wp:wrapNone/>{dgm}"#,
                "</wp:anchor></w:drawing>",
            ),
            dgm = dgm
        );
        assert_eq!(s(&parse(&bare)[0], "alt"), "Diagram");
    }

    /// Une forme libre (`a:custGeom`) garde ses couleurs, approchée par un cadre.
    #[test]
    fn a_freeform_shape_falls_back_to_a_rectangle() {
        let body = concat!(
            r#"<wp:wrapSquare wrapText="bothSides"/>"#,
            r#"<a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">"#,
            r#"<wps:wsp><wps:spPr><a:custGeom><a:pathLst/></a:custGeom>"#,
            r#"<a:solidFill><a:srgbClr val="FF0000"/></a:solidFill></wps:spPr></wps:wsp>"#,
            "</a:graphicData></a:graphic>",
        );
        let got = parse(&anchor("", "", body));
        assert_eq!(got.len(), 1);
        let alt = s(&got[0], "alt");
        assert!(alt.starts_with("kbshape:"), "{alt}");
        let decoded = urlencoding::decode(alt.trim_start_matches("kbshape:")).expect("alt décodable");
        assert!(decoded.contains("\"kind\":\"rect\""), "{decoded}");
        assert!(decoded.contains("\"fill\":\"#FF0000\""), "{decoded}");
    }

    /// Un objet dans une zone de texte porte SON ancrage : celui de la zone de
    /// texte ne doit pas être écrasé par celui de l'objet imbriqué.
    #[test]
    fn a_nested_drawing_does_not_overwrite_the_outer_anchor() {
        let inner = concat!(
            r#"<wps:txbx><w:txbxContent><w:p><w:r><w:drawing>"#,
            r#"<wp:anchor distL="0" distR="0" simplePos="0" relativeHeight="9" behindDoc="1" "#,
            r#"locked="0" layoutInCell="1" allowOverlap="1">"#,
            r#"<wp:positionH relativeFrom="page"><wp:posOffset>0</wp:posOffset></wp:positionH>"#,
            r#"<wp:positionV relativeFrom="page"><wp:posOffset>0</wp:posOffset></wp:positionV>"#,
            r#"<wp:extent cx="190500" cy="190500"/><wp:wrapNone/>"#,
            r#"<wp:docPr id="9" name="Interne"/>"#,
            r#"</wp:anchor></w:drawing></w:r></w:p></w:txbxContent></wps:txbx>"#,
        );
        let body = format!(
            concat!(
                r#"<wp:wrapSquare wrapText="bothSides"/><a:graphic><a:graphicData uri="x">"#,
                r#"<wps:wsp><wps:spPr><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></wps:spPr>{inner}"#,
                "</wps:wsp></a:graphicData></a:graphic>",
            ),
            inner = inner
        );
        let got = parse(&anchor("", "", &body));
        // Le cadre extérieur garde SA position et SON habillage.
        let outer = &got[0];
        assert_eq!(s(outer, "wrap"), "square");
        assert_eq!(s(outer, "posHRel"), "column");
        assert_eq!(outer["wrapX"], json!(100.0));
        assert_eq!(outer["zOrder"], json!(251659264));
    }

    #[test]
    fn wrap_side_mapping_and_relative_frames() {
        assert_eq!(rel_from_h("leftMargin"), "margin");
        assert_eq!(rel_from_h("character"), "character");
        assert_eq!(rel_from_h("bogus"), "column");
        assert_eq!(rel_from_v("bottomMargin"), "margin");
        assert_eq!(rel_from_v("bogus"), "paragraph");
        assert_eq!(align_h("inside"), Some("left"));
        assert_eq!(align_h("outside"), Some("right"));
        assert_eq!(align_v("top", "paragraph"), Some("top"));
        assert_eq!(align_v("bottom", "line"), Some("top"));
        assert_eq!(align_v("bogus", "page"), None);
    }
}

#[cfg(test)]
mod stats_tests {
    use crate::converters::types::PmNode;

    /// Walk every `image` node of an imported document.
    fn images(node: &PmNode, out: &mut Vec<serde_json::Value>) {
        if node.node_type == "image" {
            if let Some(a) = &node.attrs {
                out.push(a.clone());
            }
        }
        for c in node.children() {
            images(c, out);
        }
    }

    /// Import one corpus file and return the attributes of its floating objects.
    fn floats_of(rel: &str) -> Option<Vec<serde_json::Value>> {
        let root = std::env::var("KUBUNO_DOCX_CORPUS").ok()?;
        let path = std::path::Path::new(&root).join(rel);
        let bytes = std::fs::read(&path).ok()?;
        let (body, _, _, _) = crate::converters::docx::import_docx(&bytes).ok()?;
        let mut all = Vec::new();
        images(&body, &mut all);
        Some(
            all.into_iter()
                .filter(|a| {
                    !matches!(
                        a.get("wrap").and_then(|v| v.as_str()).unwrap_or(""),
                        "" | "inline"
                    )
                })
                .collect(),
        )
    }

    /// The very files the audit used to prove the losses. Skipped when the
    /// corpus is absent, so the suite stays green without a LibreOffice tree.
    #[test]
    fn corpus_audit_cases_are_recovered() {
        // 1) `wrapTight` / `wrapThrough` used to be flattened to "square", and
        //    their polygon (14 points each) was never read.
        if let Some(objs) = floats_of("sw/qa/extras/ooxmlexport/data/wrap-tight-through.docx") {
            assert_eq!(objs.len(), 2, "{objs:?}");
            let modes: Vec<&str> = objs
                .iter()
                .map(|a| a["wrap"].as_str().unwrap_or(""))
                .collect();
            assert!(modes.contains(&"through") && modes.contains(&"tight"), "{modes:?}");
            for a in &objs {
                // distL = distR = 114300 EMU = 12 px (le rendu retombait sur 10).
                assert_eq!(a["wrapDistL"], serde_json::json!(12.0), "{a:?}");
                assert_eq!(a["wrapDistR"], serde_json::json!(12.0), "{a:?}");
                assert_eq!(a["wrapSide"], serde_json::json!("both"), "{a:?}");
                assert_eq!(
                    a["wrapPolygon"].as_array().map(|p| p.len()),
                    Some(14),
                    "polygone d'habillage perdu: {a:?}"
                );
                assert!(a["zOrder"].is_i64(), "{a:?}");
            }
        }

        // 2) `simplePos="1"` : une position unique relative à la PAGE.
        if let Some(objs) = floats_of("sw/qa/extras/ooxmlexport/data/tdf166201_simplePos.docx") {
            let a = objs.first().expect("un objet flottant");
            assert_eq!(a["posHRel"], serde_json::json!("page"), "{a:?}");
            assert_eq!(a["posVRel"], serde_json::json!("page"), "{a:?}");
            assert_eq!(a["wrapX"], serde_json::json!(396.0), "{a:?}");
            assert_eq!(a["wrapY"], serde_json::json!(228.0), "{a:?}");
        }

        // 3) SmartArt : 0 objet importé auparavant.
        if let Some(objs) = floats_of("sw/qa/core/data/ooxml/pass/fdo79129.docx") {
            assert!(objs.len() >= 6, "diagrammes perdus: {}", objs.len());
            assert!(
                objs.iter().all(|a| a["alt"].as_str().is_some_and(|s| s.starts_with("Diagram"))),
                "{objs:?}"
            );
        }

        // 4) Côté d'habillage explicite (`wrapText="left"`).
        if let Some(objs) = floats_of("sw/qa/extras/ooxmlexport/data/tdf160049_anchorMargin14.docx") {
            assert!(
                objs.iter().any(|a| a["wrapSide"] == serde_json::json!("left")),
                "{objs:?}"
            );
            // Le fichier aligne quatre objets par `wp:align`.
            assert!(objs.iter().any(|a| a.get("alignH").is_some()), "{objs:?}");
        }
    }

    /// Corpus census of the floating-object attributes. Not an assertion of a
    /// magic number: it prints what the importer recovers so a regression in
    /// the anchor parsing shows up as a drop in the counters.
    ///
    /// KUBUNO_DOCX_CORPUS=… cargo test --release corpus_floating_object_stats -- --nocapture
    #[test]
    fn corpus_floating_object_stats() {
        let Ok(root) = std::env::var("KUBUNO_DOCX_CORPUS") else {
            eprintln!("KUBUNO_DOCX_CORPUS absent — test ignoré");
            return;
        };
        let mut files = 0usize;
        let mut floats = 0usize;
        let mut wrap_kinds: std::collections::BTreeMap<String, usize> = Default::default();
        let (mut side, mut dist, mut zorder, mut poly, mut relpos, mut aligns, mut placeholder) =
            (0usize, 0usize, 0usize, 0usize, 0usize, 0usize, 0usize);
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
                let Ok(bytes) = std::fs::read(&p) else { continue };
                let Ok((body, _, _, _)) = crate::converters::docx::import_docx(&bytes) else {
                    continue;
                };
                files += 1;
                let mut imgs = Vec::new();
                images(&body, &mut imgs);
                for a in imgs {
                    let w = a.get("wrap").and_then(|v| v.as_str()).unwrap_or("");
                    if w.is_empty() || w == "inline" {
                        continue;
                    }
                    floats += 1;
                    *wrap_kinds.entry(w.to_string()).or_default() += 1;
                    if a.get("wrapSide").is_some() {
                        side += 1;
                    }
                    if a.get("wrapDistL").is_some() || a.get("wrapDistT").is_some() {
                        dist += 1;
                    }
                    if a.get("zOrder").is_some() {
                        zorder += 1;
                    }
                    if a.get("wrapPolygon").is_some() {
                        poly += 1;
                    }
                    if a.get("posHRel").is_some() || a.get("posVRel").is_some() {
                        relpos += 1;
                    }
                    if a.get("alignH").is_some() || a.get("alignV").is_some() {
                        aligns += 1;
                    }
                    if a.get("src").and_then(|v| v.as_str()).unwrap_or("").is_empty()
                        && a.get("alt")
                            .and_then(|v| v.as_str())
                            .is_some_and(|s| !s.starts_with("kbshape:"))
                    {
                        placeholder += 1;
                    }
                }
            }
        }
        eprintln!(
            "corpus flottants: {files} fichiers, {floats} objets\n  modes {wrap_kinds:?}\n  \
             wrapSide {side} · wrapDist* {dist} · zOrder {zorder} · wrapPolygon {poly} · \
             posHRel/V {relpos} · alignH/V {aligns} · cadres sans image {placeholder}"
        );
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
        attrs.insert("src".into(), json!(""));
        attrs.insert("alt".into(), json!(shape_alt(frame, theme, prst_to_kind(&prst), None)));
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
