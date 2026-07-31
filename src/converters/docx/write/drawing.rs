//! Images: `word/media` entries plus the `<w:drawing>` that references them.
//!
//! Mirrors the LibreOffice export chain `DocxSdrExport::writeDMLDrawing` /
//! `writeDiagram` (sw/source/filter/ww8/docxsdrexport.cxx:666-1337): the picture
//! bytes become a package part, the part becomes a relationship, and the run
//! carries a `<w:drawing>` whose `<a:blip r:embed>` points at that relationship.
//!
//! Two framings exist and they are NOT interchangeable:
//!   * `<wp:inline>` — the picture behaves like a character on the line;
//!   * `<wp:anchor>` — the picture floats and the text wraps around it.

use base64::Engine;
use serde_json::Value;

use crate::converters::docx::xml::escape_xml_attr;
use crate::converters::types::PmNode;

use super::ctx::ExportCtx;

/// English Metric Units per CSS pixel (914400 EMU/inch / 96 px/inch).
pub(crate) const EMU_PER_PX: f64 = 9525.0;

/// DrawingML angles are stored in 1/60000th of a degree (`a:xfrm@rot`).
const ANGLE_UNITS_PER_DEGREE: f64 = 60000.0;

/// `a:srcRect` insets are in 1/1000th of a percent — the OOXML `MAX_PERCENT`
/// (oox/inc/drawingml/drawingmltypes.hxx:47).
const PERCENT_SCALE: f64 = 100_000.0;

/// `wp:wrapPolygon` coordinates live in a FIXED 21600x21600 space, independent
/// of the real picture size (WrapPolygonHandler.cxx:120-137).
const WRAP_POLY_SPAN: i64 = 21600;

/// `a:graphicData@uri` for a DrawingML picture — same string as the `pic`
/// namespace, which is a coincidence of the spec, not an alias.
const PICTURE_URI: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";

/// Fallback display size when neither the node nor the bitmap header tells us.
const FALLBACK_SIZE: (f64, f64) = (200.0, 150.0);

/// Alt-text prefixes that mark editor-generated objects whose whole definition
/// lives in `alt` and which therefore have no binary source at all.
const SYNTHETIC_ALT_PREFIXES: [&str; 3] = ["kbshape:", "kbtext:", "kbtextrich:"];

/// Prefixes that are Kubuno metadata rather than human-readable alt text.
const META_ALT_PREFIXES: [&str; 5] = ["kbshape:", "kbtext:", "kbtextrich:", "kbenvelope:", "kbfile:"];

/// An `image` / `inlineImage` node -> `<w:drawing>`, registering the bytes and
/// the relationship on the way.
pub(crate) fn render_image(node: &PmNode, ctx: &mut ExportCtx) -> String {
    let alt = attr_str(node, "alt");
    if SYNTHETIC_ALT_PREFIXES.iter().any(|p| alt.starts_with(p)) {
        // Preset shapes and rich text boxes are re-generated from `alt` by the
        // editor; exporting them needs `a:prstGeom` / `wps:txbx`, not a picture.
        return String::new();
    }

    let Some((ext, bytes)) = decode_data_uri(attr_str(node, "src")) else {
        // Anything that is not an inline `data:` payload (remote URL, `kbfile:`
        // reference) has no bytes we could package here.
        return String::new();
    };

    let (w, h) = display_size(node, &bytes);
    let rid = ctx.add_image(ext, bytes);
    let doc_id = ctx.next_docpr_id();

    let descr = if META_ALT_PREFIXES.iter().any(|p| alt.starts_with(p)) { "" } else { alt };
    let graphic = graphic_xml(node, &rid, doc_id, w, h);

    // The frame: `inlineImage` is inline by construction, `image` follows its
    // `wrap` attribute ("inline" or absent meaning "in the text flow").
    let frame = if node.node_type == "inlineImage" || matches!(attr_str(node, "wrap"), "" | "inline") {
        inline_frame_xml(node, doc_id, descr, w, h, &graphic)
    } else {
        anchor_frame_xml(node, doc_id, descr, w, h, &graphic)
    };

    format!("<w:r><w:drawing>{frame}</w:drawing></w:r>")
}

// ── Frames ──────────────────────────────────────────────────────────────────

/// `<wp:inline>` — picture anchored AS a character (LibreOffice imports this as
/// `text::TextContentAnchorType_AS_CHARACTER`, GraphicImport.cxx:1738-1741).
fn inline_frame_xml(node: &PmNode, doc_id: u32, descr: &str, w: f64, h: f64, graphic: &str) -> String {
    format!(
        r#"<wp:inline distT="0" distB="0" distL="0" distR="0">{extent}{effect}{doc_pr}{lock}{graphic}</wp:inline>"#,
        extent = extent_xml(w, h),
        effect = effect_extent_xml(node, w, h),
        doc_pr = doc_pr_xml(doc_id, descr),
        lock = FRAME_LOCKS,
    )
}

/// `<wp:anchor>` — floating picture. Child ORDER is imposed by `CT_Anchor`:
/// simplePos, positionH, positionV, extent, effectExtent, wrap*, docPr,
/// cNvGraphicFramePr, graphic. Word reports "unreadable content" otherwise.
fn anchor_frame_xml(node: &PmNode, doc_id: u32, descr: &str, w: f64, h: f64, graphic: &str) -> String {
    let mode = attr_str(node, "wrap");
    // `behindDoc` only makes sense with `wrapNone`: for every other wrapping
    // Word >= 2013 ignores it anyway (tdf#137850, GraphicImport.cxx:1583-1594).
    let behind = i32::from(mode == "behind");
    let locked = i32::from(attr_bool(node, "lockAnchor", false));
    let overlap = i32::from(attr_bool(node, "allowOverlap", true));

    let pos_h = format!(
        r#"<wp:positionH relativeFrom="{}"><wp:posOffset>{}</wp:posOffset></wp:positionH>"#,
        rel_from_h(attr_str(node, "posHRel")),
        emu(attr_f64(node, "wrapX")),
    );
    let pos_v = format!(
        r#"<wp:positionV relativeFrom="{}"><wp:posOffset>{}</wp:posOffset></wp:positionV>"#,
        rel_from_v(attr_str(node, "posVRel")),
        emu(attr_f64(node, "wrapY")),
    );

    format!(
        concat!(
            r#"<wp:anchor distT="{dt}" distB="{db}" distL="{dl}" distR="{dr}" simplePos="0""#,
            r#" relativeHeight="{z}" behindDoc="{behind}" locked="{locked}" layoutInCell="1""#,
            r#" allowOverlap="{overlap}">"#,
            r#"<wp:simplePos x="0" y="0"/>{pos_h}{pos_v}{extent}{effect}{wrap}{doc_pr}{lock}{graphic}"#,
            "</wp:anchor>",
        ),
        dt = emu(attr_f64(node, "wrapDistT").max(0.0)),
        db = emu(attr_f64(node, "wrapDistB").max(0.0)),
        dl = emu(attr_f64(node, "wrapDistL").max(0.0)),
        dr = emu(attr_f64(node, "wrapDistR").max(0.0)),
        // `relativeHeight` must be a non-negative integer and unique enough to
        // give a stable paint order; the document-wide docPr counter fits.
        z = doc_id,
        behind = behind,
        locked = locked,
        overlap = overlap,
        pos_h = pos_h,
        pos_v = pos_v,
        extent = extent_xml(w, h),
        effect = effect_extent_xml(node, w, h),
        wrap = wrap_xml(mode, wrap_text_attr(node)),
        doc_pr = doc_pr_xml(doc_id, descr),
        lock = FRAME_LOCKS,
        graphic = graphic,
    )
}

const FRAME_LOCKS: &str =
    r#"<wp:cNvGraphicFramePr><a:graphicFrameLocks noChangeAspect="1"/></wp:cNvGraphicFramePr>"#;

/// `<wp:extent>` — the UNROTATED size, always strictly positive.
fn extent_xml(w: f64, h: f64) -> String {
    format!(r#"<wp:extent cx="{}" cy="{}"/>"#, emu(w).max(1), emu(h).max(1))
}

/// `<wp:effectExtent>` — extra room around the frame for shadows, fat strokes
/// and, here, rotation. It NEVER changes the picture size, only the space the
/// frame claims (GraphicImport.cxx:1036-1055). Values must be >= 0: a negative
/// one makes Word refuse the file.
fn effect_extent_xml(node: &PmNode, w: f64, h: f64) -> String {
    let deg = rotation_degrees(node);
    let (dx, dy) = if deg == 0.0 {
        (0, 0)
    } else {
        let r = deg.to_radians();
        let (c, s) = (r.cos().abs(), r.sin().abs());
        let bw = w * c + h * s;
        let bh = w * s + h * c;
        (emu(((bw - w) / 2.0).max(0.0)), emu(((bh - h) / 2.0).max(0.0)))
    };
    format!(r#"<wp:effectExtent l="{dx}" t="{dy}" r="{dx}" b="{dy}"/>"#)
}

/// `<wp:docPr>` — `descr` is the alternative text Word shows for accessibility.
fn doc_pr_xml(doc_id: u32, descr: &str) -> String {
    let name = format!("Picture {doc_id}");
    if descr.is_empty() {
        format!(r#"<wp:docPr id="{doc_id}" name="{name}"/>"#)
    } else {
        format!(
            r#"<wp:docPr id="{doc_id}" name="{name}" descr="{}"/>"#,
            escape_xml_attr(descr)
        )
    }
}

// ── Wrapping ────────────────────────────────────────────────────────────────

/// `@wrapText` — which side(s) the text may flow on
/// (GraphicImport.cxx:385-403 on the import side).
fn wrap_text_attr(node: &PmNode) -> &'static str {
    match attr_str(node, "wrapSide") {
        "left" => "left",
        "right" => "right",
        "largest" => "largest",
        _ => "bothSides",
    }
}

/// The wrapping element itself. `wrapTight` and `wrapThrough` REQUIRE a
/// `wp:wrapPolygon` child (`CT_WrapTight`/`CT_WrapThrough`); we emit the frame
/// rectangle, which is what Word itself writes when no contour was computed.
fn wrap_xml(mode: &str, wrap_text: &'static str) -> String {
    match mode {
        "square" => format!(r#"<wp:wrapSquare wrapText="{wrap_text}"/>"#),
        "tight" => format!(
            r#"<wp:wrapTight wrapText="{wrap_text}">{}</wp:wrapTight>"#,
            wrap_polygon_xml()
        ),
        "through" => format!(
            r#"<wp:wrapThrough wrapText="{wrap_text}">{}</wp:wrapThrough>"#,
            wrap_polygon_xml()
        ),
        "topBottom" => "<wp:wrapTopAndBottom/>".to_string(),
        // "behind" / "front" / anything unknown: the text ignores the object,
        // `behindDoc` alone decides which layer it is painted on.
        _ => "<wp:wrapNone/>".to_string(),
    }
}

/// Frame rectangle in the fixed 21600x21600 wrap space, closed on its start.
fn wrap_polygon_xml() -> String {
    let m = WRAP_POLY_SPAN;
    format!(
        concat!(
            r#"<wp:wrapPolygon edited="0"><wp:start x="0" y="0"/>"#,
            r#"<wp:lineTo x="0" y="{m}"/><wp:lineTo x="{m}" y="{m}"/>"#,
            r#"<wp:lineTo x="{m}" y="0"/><wp:lineTo x="0" y="0"/></wp:wrapPolygon>"#,
        ),
        m = m
    )
}

/// `wp:positionH@relativeFrom` — ST_RelFromH. `column` is the Word default and
/// equals the content area in a single-column section.
fn rel_from_h(v: &str) -> &'static str {
    match v {
        "margin" => "margin",
        "page" => "page",
        "character" => "character",
        "leftMargin" => "leftMargin",
        "rightMargin" => "rightMargin",
        "insideMargin" => "insideMargin",
        "outsideMargin" => "outsideMargin",
        _ => "column",
    }
}

/// `wp:positionV@relativeFrom` — ST_RelFromV.
fn rel_from_v(v: &str) -> &'static str {
    match v {
        "margin" => "margin",
        "page" => "page",
        "line" => "line",
        "topMargin" => "topMargin",
        "bottomMargin" => "bottomMargin",
        "insideMargin" => "insideMargin",
        "outsideMargin" => "outsideMargin",
        _ => "paragraph",
    }
}

// ── The picture itself ──────────────────────────────────────────────────────

/// `<a:graphic>` -> `<a:graphicData>` -> `<pic:pic>`, the part that actually
/// names the relationship holding the bytes.
fn graphic_xml(node: &PmNode, rid: &str, doc_id: u32, w: f64, h: f64) -> String {
    format!(
        concat!(
            r#"<a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">"#,
            r#"<a:graphicData uri="{uri}">"#,
            r#"<pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture">"#,
            r#"<pic:nvPicPr><pic:cNvPr id="{doc_id}" name="Picture {doc_id}"/>"#,
            r#"<pic:cNvPicPr><a:picLocks noChangeAspect="1" noChangeArrowheads="1"/></pic:cNvPicPr>"#,
            "</pic:nvPicPr>",
            // CT_BlipFillProperties order: blip, srcRect, then the fill mode.
            r#"<pic:blipFill><a:blip r:embed="{rid}"/>{crop}<a:stretch><a:fillRect/></a:stretch></pic:blipFill>"#,
            r#"<pic:spPr bwMode="auto"><a:xfrm{rot}{flip}><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm>"#,
            r#"<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr>"#,
            "</pic:pic></a:graphicData></a:graphic>",
        ),
        uri = PICTURE_URI,
        doc_id = doc_id,
        rid = escape_xml_attr(rid),
        crop = src_rect_xml(node),
        rot = rotation_attr(node),
        flip = flip_attrs(node),
        cx = emu(w).max(1),
        cy = emu(h).max(1),
    )
}

/// `a:xfrm@rot`, clockwise, in 1/60000th of a degree. The rotation happens
/// around the centre and does NOT change `wp:extent`.
fn rotation_attr(node: &PmNode) -> String {
    let deg = rotation_degrees(node);
    if deg == 0.0 {
        return String::new();
    }
    format!(r#" rot="{}""#, (deg * ANGLE_UNITS_PER_DEGREE).round() as i64)
}

fn rotation_degrees(node: &PmNode) -> f64 {
    let deg = attr_f64(node, "rotation").rem_euclid(360.0);
    if deg.abs() < 0.001 {
        0.0
    } else {
        deg
    }
}

/// `a:xfrm@flipH/@flipV` — mirroring, applied BEFORE the rotation in OOXML.
fn flip_attrs(node: &PmNode) -> String {
    let mut s = String::new();
    if attr_bool(node, "flipH", false) {
        s.push_str(r#" flipH="1""#);
    }
    if attr_bool(node, "flipV", false) {
        s.push_str(r#" flipV="1""#);
    }
    s
}

/// `<a:srcRect>` — cropping expressed as INSETS from each edge of the NATIVE
/// bitmap, in 1/1000th of a percent (fillproperties.cxx:864-873). `r` and `b`
/// are insets, not coordinates. A fully cropped rectangle is dropped rather
/// than written: Word renders nothing for it.
fn src_rect_xml(node: &PmNode) -> String {
    let pct = |key: &str| -> i64 {
        let frac = attr_f64(node, key);
        if !frac.is_finite() || frac <= 0.0 {
            0
        } else {
            (frac.min(1.0) * PERCENT_SCALE).round() as i64
        }
    };
    let (l, t, r, b) = (pct("cropL"), pct("cropT"), pct("cropR"), pct("cropB"));
    if (l | t | r | b) == 0 {
        return String::new();
    }
    let max = PERCENT_SCALE as i64;
    if l + r >= max || t + b >= max {
        return String::new();
    }
    let mut s = String::from("<a:srcRect");
    for (name, v) in [("l", l), ("t", t), ("r", r), ("b", b)] {
        if v != 0 {
            s.push_str(&format!(r#" {name}="{v}""#));
        }
    }
    s.push_str("/>");
    s
}

// ── Source decoding ─────────────────────────────────────────────────────────

/// `data:<mime>[;charset=…][;base64],<payload>` -> (file extension, bytes).
/// The importer always produces such a URI (`build_media_map` in zip_io.rs), so
/// this is the round-trip path; anything else yields `None`.
fn decode_data_uri(src: &str) -> Option<(&'static str, Vec<u8>)> {
    let rest = src.strip_prefix("data:").or_else(|| src.strip_prefix("DATA:"))?;
    let (meta, payload) = rest.split_once(',')?;
    let mut parts = meta.split(';');
    let mime = parts.next().unwrap_or("").trim().to_ascii_lowercase();
    let ext = mime_to_ext(&mime)?;
    let is_b64 = meta.split(';').any(|p| p.trim().eq_ignore_ascii_case("base64"));
    let bytes = if is_b64 {
        // Data URIs sometimes carry whitespace from a wrapped source.
        let clean: String = payload.chars().filter(|c| !c.is_ascii_whitespace()).collect();
        base64::engine::general_purpose::STANDARD.decode(clean).ok()?
    } else {
        urlencoding::decode_binary(payload.as_bytes()).into_owned()
    };
    if bytes.is_empty() {
        return None;
    }
    Some((ext, bytes))
}

/// Only extensions `image_content_type` can declare in `[Content_Types].xml`:
/// an undeclared extension makes the whole package invalid.
fn mime_to_ext(mime: &str) -> Option<&'static str> {
    Some(match mime {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/gif" => "gif",
        "image/bmp" | "image/x-ms-bmp" => "bmp",
        "image/webp" => "webp",
        "image/tiff" => "tif",
        "image/svg+xml" => "svg",
        "image/x-emf" | "image/emf" => "emf",
        "image/x-wmf" | "image/wmf" => "wmf",
        _ => return None,
    })
}

/// Display size in doc pixels: the node's own size when it has one, otherwise
/// the bitmap's intrinsic size, otherwise a sane default. A zero `wp:extent`
/// makes Word drop the picture silently.
fn display_size(node: &PmNode, bytes: &[u8]) -> (f64, f64) {
    let (mut w, mut h) = (attr_f64(node, "width"), attr_f64(node, "height"));
    if w <= 0.0 || h <= 0.0 {
        let (iw, ih) = intrinsic_size(bytes).unwrap_or(FALLBACK_SIZE);
        // A single known side keeps the aspect ratio of the other one.
        match (w > 0.0, h > 0.0) {
            (true, false) => h = w * ih / iw.max(1.0),
            (false, true) => w = h * iw / ih.max(1.0),
            _ => {
                w = iw;
                h = ih;
            }
        }
    }
    (w.max(1.0), h.max(1.0))
}

/// Pixel size read straight from the bitmap header (PNG, GIF, BMP, JPEG).
/// Vector and exotic formats return `None` — the caller falls back.
fn intrinsic_size(b: &[u8]) -> Option<(f64, f64)> {
    let be16 = |i: usize| -> Option<u32> {
        Some((u32::from(*b.get(i)?) << 8) | u32::from(*b.get(i + 1)?))
    };
    let be32 = |i: usize| -> Option<u32> { Some((be16(i)? << 16) | be16(i + 2)?) };
    let le32 = |i: usize| -> Option<i32> {
        let v = u32::from(*b.get(i)?)
            | (u32::from(*b.get(i + 1)?) << 8)
            | (u32::from(*b.get(i + 2)?) << 16)
            | (u32::from(*b.get(i + 3)?) << 24);
        Some(v as i32)
    };

    if b.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        // IHDR is always the first chunk: width/height at offsets 16 and 20.
        return Some((f64::from(be32(16)?), f64::from(be32(20)?)));
    }
    if b.starts_with(b"GIF8") {
        let le16 = |i: usize| -> Option<u32> {
            Some(u32::from(*b.get(i)?) | (u32::from(*b.get(i + 1)?) << 8))
        };
        return Some((f64::from(le16(6)?), f64::from(le16(8)?)));
    }
    if b.starts_with(b"BM") {
        // BITMAPINFOHEADER: height may be negative (top-down rows).
        return Some((f64::from(le32(18)?.abs()), f64::from(le32(22)?.abs())));
    }
    if b.starts_with(&[0xFF, 0xD8]) {
        let mut pos = 2usize;
        while pos + 9 < b.len() {
            if b[pos] != 0xFF {
                pos += 1;
                continue;
            }
            let marker = b[pos + 1];
            // SOF0..SOF15 carry the frame size; DHT/JPG/DAC are not frames.
            if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
                return Some((f64::from(be16(pos + 7)?), f64::from(be16(pos + 5)?)));
            }
            if matches!(marker, 0xD8 | 0xD9 | 0x01) || (0xD0..=0xD7).contains(&marker) {
                pos += 2;
                continue;
            }
            let seg = be16(pos + 2)? as usize;
            if seg < 2 {
                return None;
            }
            pos += 2 + seg;
        }
    }
    None
}

// ── Small helpers ───────────────────────────────────────────────────────────

/// Doc pixels -> EMU, rounded once at the very end.
fn emu(px: f64) -> i64 {
    if px.is_finite() {
        (px * EMU_PER_PX).round() as i64
    } else {
        0
    }
}

fn attr<'a>(node: &'a PmNode, key: &str) -> Option<&'a Value> {
    node.attrs.as_ref().and_then(|a| a.get(key))
}

fn attr_str<'a>(node: &'a PmNode, key: &str) -> &'a str {
    attr(node, key).and_then(|v| v.as_str()).unwrap_or("")
}

fn attr_f64(node: &PmNode, key: &str) -> f64 {
    attr(node, key).and_then(|v| v.as_f64()).unwrap_or(0.0)
}

fn attr_bool(node: &PmNode, key: &str, dflt: bool) -> bool {
    match attr(node, key) {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0) != 0.0,
        Some(Value::String(s)) => matches!(s.as_str(), "1" | "true"),
        _ => dflt,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 1x1 opaque PNG — enough to exercise the header sniffing and the packaging.
    const PNG_1X1: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

    fn image(node_type: &str, attrs: Value) -> PmNode {
        PmNode {
            node_type: node_type.into(),
            attrs: Some(attrs),
            content: None,
            marks: None,
            text: None,
        }
    }

    #[test]
    fn inline_image_becomes_a_drawing_with_media_and_relationship() {
        let node = image(
            "inlineImage",
            json!({ "src": PNG_1X1, "width": 96, "height": 48, "alt": "un chat" }),
        );
        let mut ctx = ExportCtx::new();
        let xml = render_image(&node, &mut ctx);

        assert!(xml.starts_with("<w:r><w:drawing><wp:inline"), "{xml}");
        assert!(xml.contains(r#"<wp:extent cx="914400" cy="457200"/>"#), "{xml}");
        assert!(xml.contains(r#"<wp:effectExtent l="0" t="0" r="0" b="0"/>"#), "{xml}");
        assert!(xml.contains(r#"descr="un chat""#), "{xml}");
        assert_eq!(ctx.media.len(), 1);
        assert_eq!(ctx.media[0].0, "word/media/image1.png");
        assert!(ctx.media_exts.contains("png"));
        // The blip must name the relationship the context just created.
        let rid = &ctx.rels[0].id;
        assert!(xml.contains(&format!(r#"<a:blip r:embed="{rid}"/>"#)), "{xml}");
    }

    #[test]
    fn anchored_image_carries_wrapping_position_and_distances() {
        let node = image(
            "image",
            json!({
                "src": PNG_1X1, "width": 100, "height": 50,
                "wrap": "tight", "wrapSide": "left",
                "wrapX": 10, "wrapY": -20,
                "wrapDistT": 3, "wrapDistB": 3, "wrapDistL": 12, "wrapDistR": 12,
                "posHRel": "margin", "posVRel": "page", "allowOverlap": false,
            }),
        );
        let mut ctx = ExportCtx::new();
        let xml = render_image(&node, &mut ctx);

        assert!(xml.contains("<wp:anchor "), "{xml}");
        assert!(xml.contains(r#"distL="114300""#), "{xml}");
        assert!(xml.contains(r#"allowOverlap="0""#), "{xml}");
        assert!(xml.contains(r#"behindDoc="0""#), "{xml}");
        assert!(xml.contains(r#"<wp:positionH relativeFrom="margin"><wp:posOffset>95250</wp:posOffset>"#), "{xml}");
        // Negative offsets are legal: Word lets an object bleed into the margin.
        assert!(xml.contains(r#"<wp:positionV relativeFrom="page"><wp:posOffset>-190500</wp:posOffset>"#), "{xml}");
        // wrapTight is invalid without its polygon.
        assert!(xml.contains(r#"<wp:wrapTight wrapText="left"><wp:wrapPolygon"#), "{xml}");
    }

    #[test]
    fn behind_text_uses_wrap_none_and_behind_doc() {
        let node = image("image", json!({ "src": PNG_1X1, "width": 10, "height": 10, "wrap": "behind" }));
        let mut ctx = ExportCtx::new();
        let xml = render_image(&node, &mut ctx);
        assert!(xml.contains(r#"behindDoc="1""#), "{xml}");
        assert!(xml.contains("<wp:wrapNone/>"), "{xml}");
    }

    /// Rotation must reach `a:xfrm@rot` and grow `wp:effectExtent`, never the
    /// extent itself — otherwise every rotated picture is stretched.
    #[test]
    fn rotation_and_crop_and_flip_are_serialised() {
        let node = image(
            "inlineImage",
            json!({
                "src": PNG_1X1, "width": 100, "height": 100, "rotation": 90,
                "cropL": 0.1, "cropB": 0.25, "flipH": true,
            }),
        );
        let mut ctx = ExportCtx::new();
        let xml = render_image(&node, &mut ctx);
        assert!(xml.contains(r#"rot="5400000""#), "{xml}");
        assert!(xml.contains(r#"flipH="1""#), "{xml}");
        assert!(xml.contains(r#"<a:srcRect l="10000" b="25000"/>"#), "{xml}");
        assert!(xml.contains(r#"<wp:extent cx="952500" cy="952500"/>"#), "{xml}");
        // 90 deg on a square: the bounding box does not grow.
        assert!(xml.contains(r#"<wp:effectExtent l="0" t="0" r="0" b="0"/>"#), "{xml}");
    }

    /// A missing size falls back to the bitmap header, not to zero: a zero
    /// `wp:extent` makes Word drop the picture without a word.
    #[test]
    fn missing_size_falls_back_to_the_bitmap_header() {
        let node = image("inlineImage", json!({ "src": PNG_1X1 }));
        let mut ctx = ExportCtx::new();
        let xml = render_image(&node, &mut ctx);
        assert!(xml.contains(r#"<wp:extent cx="9525" cy="9525"/>"#), "{xml}");
    }

    #[test]
    fn generated_shapes_and_text_boxes_are_skipped() {
        for alt in ["kbshape:%7B%22kind%22%3A%22rect%22%7D", "kbtextrich:%7B%7D", "kbtext:x"] {
            let node = image("image", json!({ "src": "", "alt": alt, "width": 40, "height": 40 }));
            let mut ctx = ExportCtx::new();
            assert!(render_image(&node, &mut ctx).is_empty(), "{alt}");
            assert!(ctx.media.is_empty(), "{alt}");
        }
    }

    #[test]
    fn a_non_data_source_produces_nothing_rather_than_a_broken_part() {
        let node = image("image", json!({ "src": "https://example.org/a.png", "width": 40, "height": 40 }));
        let mut ctx = ExportCtx::new();
        assert!(render_image(&node, &mut ctx).is_empty());
        assert!(ctx.rels.is_empty());
    }

    #[test]
    fn jpeg_and_gif_headers_are_understood() {
        // GIF87a, 4x3.
        let gif = [b'G', b'I', b'F', b'8', b'7', b'a', 4, 0, 3, 0];
        assert_eq!(intrinsic_size(&gif), Some((4.0, 3.0)));
        // JPEG: SOI then a SOF0 declaring 5x7.
        let jpeg = [0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x11, 0x08, 0x00, 0x07, 0x00, 0x05, 0x03, 0x01];
        assert_eq!(intrinsic_size(&jpeg), Some((5.0, 7.0)));
    }
}
