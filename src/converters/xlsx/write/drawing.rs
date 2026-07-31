//! Drawing part builder (xl/drawings/drawingN.xml): pictures, chart frames and
//! shapes.
//!
//! One drawing part per sheet that owns images, charts and/or shapes. Pictures
//! are decoded from their internal data-URL into xl/media/imageN.ext; charts get
//! their own xl/charts/chartN.xml part (see `chart.rs`); shapes are entirely
//! inline (see `shape.rs`). Anchors:
//!   - objects carrying an explicit pixel box (`bx`/`by`/`bw`/`bh` — UI-created
//!     or user-moved) are re-anchored by walking the sheet's column widths and
//!     row heights (oneCellAnchor + ext);
//!   - objects keeping their imported EMU anchor round-trip verbatim
//!     (twoCellAnchor when `to*` is present, else oneCellAnchor + ext).
use base64::Engine;
use serde_json::Value;

use super::super::util::{esc_xml, idx_to_col};
use super::chart::build_chart_xml;
use super::shape::{cnv_pr_xml, line_xml, sp_xml, OUTER_SHADOW};

/// EMU per pixel (96 dpi).
const EMU_PER_PX: f64 = 9525.0;
// Frontend grid defaults (SpreadsheetApp DEFAULT_COL_WIDTH / DEFAULT_ROW_HEIGHT).
const DEFAULT_COL_W: f64 = 100.0;
const DEFAULT_ROW_H: f64 = 24.0;
const MAX_COL: i64 = 16_383;
const MAX_ROW: i64 = 1_048_575;

/// Everything one sheet's drawing contributes to the archive.
pub struct SheetDrawing {
    pub xml:    String,              // xl/drawings/drawingN.xml content
    pub rels:   String,              // its _rels part content
    pub media:  Vec<(String, Vec<u8>)>, // (zip path, bytes)
    pub charts: Vec<(String, String)>,  // (zip path, xml)
}

// ── Anchor computation ───────────────────────────────────────────────────────

enum Anchor {
    Two { from: (i64, i64, i64, i64), to: (i64, i64, i64, i64) }, // (col, colOff, row, rowOff) EMU
    One { from: (i64, i64, i64, i64), ext: (i64, i64) },
}

fn px_to_emu(px: f64) -> i64 { (px * EMU_PER_PX).round() as i64 }

// Walk an axis (widths in px per index) to convert an absolute pixel position
// into (index, remaining offset px). `size_of` returns the px size of index i.
fn walk_axis(mut pos: f64, max: i64, size_of: impl Fn(i64) -> f64) -> (i64, f64) {
    let mut i = 0i64;
    while i < max {
        let s = size_of(i).max(0.0);
        if pos < s { break; }
        pos -= s;
        i += 1;
    }
    (i, pos)
}

// Anchor for an explicit pixel box, resolved against the sheet geometry.
fn anchor_from_px_box(bx: f64, by: f64, bw: f64, bh: f64, data: &Value) -> Anchor {
    let col_w = data.get("col_widths").and_then(|v| v.as_object());
    let row_h = data.get("row_heights").and_then(|v| v.as_object());
    let def_w = data.get("default_col_width").and_then(|v| v.as_f64()).unwrap_or(DEFAULT_COL_W);
    let def_h = data.get("default_row_height").and_then(|v| v.as_f64()).unwrap_or(DEFAULT_ROW_H);
    let (col, cx) = walk_axis(bx.max(0.0), MAX_COL, |i| {
        col_w.and_then(|m| m.get(&idx_to_col(i as usize))).and_then(|v| v.as_f64()).unwrap_or(def_w)
    });
    let (row, ry) = walk_axis(by.max(0.0), MAX_ROW, |i| {
        row_h.and_then(|m| m.get(&(i + 1).to_string())).and_then(|v| v.as_f64()).unwrap_or(def_h)
    });
    Anchor::One {
        from: (col, px_to_emu(cx), row, px_to_emu(ry)),
        ext:  (px_to_emu(bw.max(1.0)), px_to_emu(bh.max(1.0))),
    }
}

// Anchor of an image/chart JSON object. Precedence mirrors the frontend's
// rendering: explicit pixel box first, then the imported EMU cell anchor.
fn anchor_of(obj: &Value, data: &Value) -> Anchor {
    let gi = |k: &str| obj.get(k).and_then(|v| v.as_i64());
    let gf = |k: &str| obj.get(k).and_then(|v| v.as_f64());
    if let (Some(bx), Some(by), Some(bw), Some(bh)) = (gf("bx"), gf("by"), gf("bw"), gf("bh")) {
        return anchor_from_px_box(bx, by, bw, bh, data);
    }
    if let (Some(fc), Some(fr)) = (gi("fromCol"), gi("fromRow")) {
        let from = (fc.max(0), gi("fromColOff").unwrap_or(0), fr.max(0), gi("fromRowOff").unwrap_or(0));
        if let (Some(tc), Some(tr)) = (gi("toCol"), gi("toRow")) {
            return Anchor::Two { from, to: (tc.max(0), gi("toColOff").unwrap_or(0), tr.max(0), gi("toRowOff").unwrap_or(0)) };
        }
        // oneCellAnchor requires an ext; fall back to ~200×200 px.
        return Anchor::One { from, ext: (gi("extCx").unwrap_or(1_905_000), gi("extCy").unwrap_or(1_905_000)) };
    }
    anchor_from_px_box(0.0, 0.0, gf("bw").unwrap_or(380.0), gf("bh").unwrap_or(240.0), data)
}

fn marker_xml(tag: &str, (col, coff, row, roff): (i64, i64, i64, i64)) -> String {
    format!("<xdr:{tag}><xdr:col>{col}</xdr:col><xdr:colOff>{coff}</xdr:colOff><xdr:row>{row}</xdr:row><xdr:rowOff>{roff}</xdr:rowOff></xdr:{tag}>")
}

// Wrap an object's XML in its anchor element.
fn wrap_anchor(anchor: &Anchor, inner: &str) -> String {
    match anchor {
        Anchor::Two { from, to } => format!(
            "<xdr:twoCellAnchor>{}{}{inner}<xdr:clientData/></xdr:twoCellAnchor>",
            marker_xml("from", *from), marker_xml("to", *to)
        ),
        Anchor::One { from, ext: (cx, cy) } => format!(
            "<xdr:oneCellAnchor>{}<xdr:ext cx=\"{cx}\" cy=\"{cy}\"/>{inner}<xdr:clientData/></xdr:oneCellAnchor>",
            marker_xml("from", *from)
        ),
    }
}

// ── Picture emission ─────────────────────────────────────────────────────────

// "data:image/png;base64,AAA…" → (file extension, decoded bytes). Unknown MIME
// types are skipped (the picture cannot be represented in the archive).
fn decode_data_url(src: &str) -> Option<(&'static str, Vec<u8>)> {
    let rest = src.strip_prefix("data:")?;
    let (mime, b64) = rest.split_once(";base64,")?;
    let ext = match mime {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/gif" => "gif",
        "image/bmp" => "bmp",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        _ => return None,
    };
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64.trim()).ok()?;
    Some((ext, bytes))
}

// <xdr:pic> for one image: blip (embed rel), crop, rotation, size, outline,
// drop shadow, alt text and hyperlink.
fn pic_xml(cnv_id: usize, rid: &str, img: &Value, anchor: &Anchor, link_rid: Option<&str>) -> String {
    let gf = |k: &str| img.get(k).and_then(|v| v.as_f64());
    // Crop insets: fractions → 1000ths of a percent.
    let mut src_rect = String::new();
    for (key, at) in [("cropL", "l"), ("cropT", "t"), ("cropR", "r"), ("cropB", "b")] {
        if let Some(v) = gf(key).filter(|v| *v > 0.0) {
            src_rect.push_str(&format!(" {at}=\"{}\"", (v * 100_000.0).round() as i64));
        }
    }
    let src_rect = if src_rect.is_empty() { String::new() } else { format!("<a:srcRect{src_rect}/>") };
    // Shape transform: rotation and/or explicit size (twoCellAnchor pictures keep
    // their a:ext so the imported extCx/extCy round-trips; oneCellAnchor pictures
    // already carry the size on the anchor's xdr:ext, which the reader prefers).
    let rot = gf("rot").filter(|r| *r != 0.0).map(|r| (r * 60_000.0).round() as i64);
    let ext = match anchor {
        Anchor::Two { .. } => img.get("extCx").and_then(|v| v.as_i64())
            .map(|cx| (cx, img.get("extCy").and_then(|v| v.as_i64()).unwrap_or(0))),
        Anchor::One { .. } => None,
    };
    let xfrm = match (rot, ext) {
        (None, None) => String::new(),
        (r, Some((cx, cy))) => format!(
            "<a:xfrm{}><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm>",
            r.map(|r| format!(" rot=\"{r}\"")).unwrap_or_default()
        ),
        (Some(r), None) => format!("<a:xfrm rot=\"{r}\"/>"),
    };
    // Picture formatting. Order inside spPr follows CT_ShapeProperties:
    // xfrm, geometry, fill, line, effects.
    let ln = line_xml(
        img.get("border").and_then(|v| v.as_str()),
        img.get("borderWidth").and_then(|v| v.as_f64()),
    );
    let shadow = if img.get("shadow").and_then(|v| v.as_bool()).unwrap_or(false) { OUTER_SHADOW } else { "" };
    format!(concat!(
        "<xdr:pic><xdr:nvPicPr>{}<xdr:cNvPicPr/></xdr:nvPicPr>",
        "<xdr:blipFill><a:blip r:embed=\"{}\"/>{}<a:stretch><a:fillRect/></a:stretch></xdr:blipFill>",
        "<xdr:spPr>{}<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom>{}{}</xdr:spPr></xdr:pic>"
    ),
        cnv_pr_xml(cnv_id, "Image", img.get("altText").and_then(|v| v.as_str()), link_rid),
        rid, src_rect, xfrm, ln, shadow)
}

// External hyperlink relationship for an object's `a:hlinkClick`.
fn hyperlink_rel(rid: &str, target: &str) -> String {
    format!(
        "<Relationship Id=\"{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink\" Target=\"{}\" TargetMode=\"External\"/>",
        esc_xml(rid), esc_xml(target)
    )
}

// The object's `link`, when it is a usable external target.
fn link_target(obj: &Value) -> Option<&str> {
    obj.get("link").and_then(|v| v.as_str()).map(str::trim).filter(|t| !t.is_empty())
}

// <xdr:graphicFrame> for one chart (the mandatory xfrm carries the rotation;
// the real geometry lives on the anchor, hence the zero off/ext placeholder).
fn graphic_frame_xml(cnv_id: usize, rid: &str, rot_deg: f64, alt_text: Option<&str>) -> String {
    let rot = if rot_deg != 0.0 {
        format!(" rot=\"{}\"", (rot_deg * 60_000.0).round() as i64)
    } else {
        String::new()
    };
    format!(concat!(
        "<xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr>{}",
        "<xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr>",
        "<xdr:xfrm{}><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/></xdr:xfrm>",
        "<a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/chart\">",
        "<c:chart xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" r:id=\"{}\"/>",
        "</a:graphicData></a:graphic></xdr:graphicFrame>"
    ), cnv_pr_xml(cnv_id, "Graphique", alt_text, None), rot, rid)
}

// ── Entry point ──────────────────────────────────────────────────────────────

/// Build the drawing part for one sheet (images + charts + shapes). Returns None
/// when the sheet has no representable drawing object. `media_no` / `chart_no`
/// are workbook-global part counters.
pub fn build_sheet_drawing(
    data: &Value,
    sheet_name: &str,
    media_no: &mut usize,
    chart_no: &mut usize,
) -> Option<SheetDrawing> {
    let arr = |k: &str| data.get(k).and_then(|v| v.as_array()).map(|a| a.as_slice()).unwrap_or(&[]);
    let images = arr("images");
    let charts = arr("charts");
    let shapes = arr("shapes");
    if images.is_empty() && charts.is_empty() && shapes.is_empty() { return None; }

    let mut anchors = String::new();
    let mut rels = String::new();
    let mut media: Vec<(String, Vec<u8>)> = Vec::new();
    let mut chart_parts: Vec<(String, String)> = Vec::new();
    let mut rid_no = 0usize;
    let mut cnv = 1usize;

    for img in images {
        let Some(src) = img.get("src").and_then(|v| v.as_str()) else { continue };
        let Some((ext, bytes)) = decode_data_url(src) else { continue };
        rid_no += 1;
        cnv += 1;
        let rid = format!("rId{rid_no}");
        let fname = format!("image{}.{ext}", *media_no);
        *media_no += 1;
        media.push((format!("xl/media/{fname}"), bytes));
        rels.push_str(&format!(
            "<Relationship Id=\"{rid}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"../media/{fname}\"/>"
        ));
        let hl_rid = link_target(img).map(|t| {
            rid_no += 1;
            let hl = format!("rId{rid_no}");
            rels.push_str(&hyperlink_rel(&hl, t));
            hl
        });
        let anchor = anchor_of(img, data);
        anchors.push_str(&wrap_anchor(&anchor, &pic_xml(cnv, &rid, img, &anchor, hl_rid.as_deref())));
    }

    for chart in charts {
        let Some(cxml) = build_chart_xml(chart, sheet_name, data) else { continue };
        rid_no += 1;
        cnv += 1;
        let rid = format!("rId{rid_no}");
        let fname = format!("chart{}.xml", *chart_no);
        *chart_no += 1;
        chart_parts.push((format!("xl/charts/{fname}"), cxml));
        rels.push_str(&format!(
            "<Relationship Id=\"{rid}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart\" Target=\"../charts/{fname}\"/>"
        ));
        let anchor = anchor_of(chart, data);
        let rot = chart.get("rot").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let alt = chart.get("altText").and_then(|v| v.as_str());
        anchors.push_str(&wrap_anchor(&anchor, &graphic_frame_xml(cnv, &rid, rot, alt)));
    }

    // Shapes: no media, no chart part — only an optional hyperlink relationship.
    for shape in shapes {
        cnv += 1;
        let hl_rid = link_target(shape).map(|t| {
            rid_no += 1;
            let hl = format!("rId{rid_no}");
            rels.push_str(&hyperlink_rel(&hl, t));
            hl
        });
        let anchor = anchor_of(shape, data);
        // The shape transform repeats the size the anchor already carries.
        let ext = match &anchor {
            Anchor::One { ext, .. } => *ext,
            // A shape always carries its own pixel box, so this is defensive only.
            Anchor::Two { .. } => (
                px_to_emu(shape.get("bw").and_then(|v| v.as_f64()).unwrap_or(160.0)),
                px_to_emu(shape.get("bh").and_then(|v| v.as_f64()).unwrap_or(96.0)),
            ),
        };
        anchors.push_str(&wrap_anchor(&anchor, &sp_xml(cnv, shape, hl_rid.as_deref(), ext)));
    }

    if anchors.is_empty() { return None; }
    let xml = format!(concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n",
        "<xdr:wsDr xmlns:xdr=\"http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing\" ",
        "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" ",
        "xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">{}</xdr:wsDr>"
    ), anchors);
    let rels = format!(concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n",
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{}</Relationships>"
    ), rels);
    Some(SheetDrawing { xml, rels, media, charts: chart_parts })
}
