//! Drawing shape emission (`<xdr:sp>` inside a sheet's drawing part).
//!
//! A shape is self-contained: no media part, no chart part, no relationship —
//! except an optional external hyperlink, which the drawing writer registers.
//! Everything the internal `SheetShape` model carries (preset geometry, fill,
//! outline, caption + its shape-wide text style) maps 1:1 onto DrawingML.
use serde_json::Value;

use super::super::util::esc_xml;

/// Internal `SheetShapeKind` → OOXML preset geometry (`a:prstGeom@prst`).
/// Exact inverse of the canonical half of `read::shape::PRESET_KINDS`.
/// Preset name and adjustment names of a kind, from the GENERATED LibreOffice
/// table (144 kinds); the handful of bespoke kinds (lines, connectors) fall back
/// to plain presets without adjustments.
pub fn preset_of_kind(kind: &str) -> (&'static str, &'static [&'static str]) {
    for (k, prst, names) in crate::converters::xlsx::shape_presets::KIND_PRESETS {
        if *k == kind { return (prst, names); }
    }
    match kind {
        "line" => ("line", &[]),
        _ => ("rect", &[]),
    }
}


/// `<a:avLst>` of a shape: its raw adjustment values, written under the preset's
/// own adjustment NAMES (from the generated LibreOffice table) — the names are
/// what Excel resolves the guides against, wrong ones break the geometry.
fn av_lst_xml(shape: &Value) -> String {
    let names = preset_of_kind(shape.get("kind").and_then(|v| v.as_str()).unwrap_or("rect")).1;
    let vals = shape.get("adj").and_then(|v| v.as_array());
    let Some(vals) = vals else { return "<a:avLst/>".to_string() };
    let mut gds = String::new();
    for (i, name) in names.iter().enumerate() {
        if let Some(v) = vals.get(i).and_then(|v| v.as_f64()) {
            gds.push_str(&format!("<a:gd name=\"{name}\" fmla=\"val {}\"/>", v.round() as i64));
        }
    }
    if gds.is_empty() { "<a:avLst/>".to_string() } else { format!("<a:avLst>{gds}</a:avLst>") }
}

pub fn preset_from_kind(kind: &str) -> &'static str {
    preset_of_kind(kind).0
}

/// "#1A73E8" / "1a73e8" → "1A73E8". None for `none`, empty or malformed values.
pub fn hex6(s: &str) -> Option<String> {
    let h = s.trim().trim_start_matches('#');
    // Accept the 8-digit #RRGGBBAA form by dropping the alpha (OOXML carries it
    // in a separate <a:alpha> child, which the model does not describe).
    let h = if h.len() == 8 { &h[..6] } else { h };
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) { return None; }
    Some(h.to_ascii_uppercase())
}

/// `<a:solidFill>`/`<a:noFill>` for a model colour slot. `None` (absent) emits
/// nothing at all so the shape keeps the producer's/theme default.
pub fn fill_xml(color: Option<&str>) -> String {
    match color {
        None => String::new(),
        Some(c) if c.eq_ignore_ascii_case("none") => "<a:noFill/>".into(),
        Some(c) => match hex6(c) {
            Some(h) => format!("<a:solidFill><a:srgbClr val=\"{h}\"/></a:solidFill>"),
            None => String::new(),
        },
    }
}

/// `<a:ln>` for an outline colour + width (px). Empty when neither is set.
pub fn line_xml(color: Option<&str>, width_px: Option<f64>) -> String {
    if color.is_none() && width_px.is_none() { return String::new(); }
    let w = width_px
        .filter(|w| *w > 0.0)
        .map(|w| format!(" w=\"{}\"", (w * 12_700.0).round() as i64))
        .unwrap_or_default();
    format!("<a:ln{w}>{}</a:ln>", fill_xml(color))
}

/// The standard soft drop shadow Excel applies from its picture-styles gallery.
pub const OUTER_SHADOW: &str = concat!(
    "<a:effectLst><a:outerShdw blurRad=\"50800\" dist=\"38100\" dir=\"2700000\" algn=\"tl\" rotWithShape=\"0\">",
    "<a:srgbClr val=\"000000\"><a:alpha val=\"40000\"/></a:srgbClr></a:outerShdw></a:effectLst>"
);

/// `<a:hlinkClick>` child of a cNvPr. The relationship itself is created by the
/// drawing writer (external target, `TargetMode="External"`).
pub fn hlink_xml(rid: Option<&str>) -> String {
    rid.map(|r| format!("<a:hlinkClick r:id=\"{}\"/>", esc_xml(r))).unwrap_or_default()
}

/// Non-visual properties shared by pictures, chart frames and shapes: the
/// mandatory id/name, the accessibility description and the hyperlink. Returns
/// a complete `<xdr:cNvPr>` element (empty tag when it has no children).
pub fn cnv_pr_xml(cnv_id: usize, name: &str, alt_text: Option<&str>, link_rid: Option<&str>) -> String {
    let descr = alt_text
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(|d| format!(" descr=\"{}\"", esc_xml(d)))
        .unwrap_or_default();
    let hlink = hlink_xml(link_rid);
    if hlink.is_empty() {
        format!("<xdr:cNvPr id=\"{cnv_id}\" name=\"{} {cnv_id}\"{descr}/>", esc_xml(name))
    } else {
        format!("<xdr:cNvPr id=\"{cnv_id}\" name=\"{} {cnv_id}\"{descr}>{hlink}</xdr:cNvPr>", esc_xml(name))
    }
}

// <xdr:txBody> for the shape caption. Always emitted: Excel expects a text body
// on a preset shape even when it is empty.
fn tx_body_xml(shape: &Value) -> String {
    let style = shape.get("textStyle");
    let gb = |k: &str| style.and_then(|s| s.get(k)).and_then(|v| v.as_bool()).unwrap_or(false);
    let align = match style.and_then(|s| s.get("align")).and_then(|v| v.as_str()) {
        Some("left") => "l",
        Some("right") => "r",
        // The model centres a caption by default; OOXML defaults to left, so the
        // alignment is always written out.
        _ => "ctr",
    };
    let mut r_pr = String::from("<a:rPr lang=\"fr-FR\"");
    if let Some(sz) = style.and_then(|s| s.get("size")).and_then(|v| v.as_f64()).filter(|v| *v > 0.0) {
        r_pr.push_str(&format!(" sz=\"{}\"", (sz * 100.0).round() as i64));
    }
    if gb("bold") { r_pr.push_str(" b=\"1\""); }
    if gb("italic") { r_pr.push_str(" i=\"1\""); }
    let color = style.and_then(|s| s.get("color")).and_then(|v| v.as_str()).and_then(hex6);
    match color {
        Some(h) => r_pr.push_str(&format!("><a:solidFill><a:srgbClr val=\"{h}\"/></a:solidFill></a:rPr>")),
        None => r_pr.push_str("/>"),
    }

    let text = shape.get("text").and_then(|v| v.as_str()).unwrap_or("");
    let paras = if text.is_empty() {
        format!("<a:p><a:pPr algn=\"{align}\"/><a:endParaRPr lang=\"fr-FR\"/></a:p>")
    } else {
        text.split('\n')
            .map(|line| format!(
                "<a:p><a:pPr algn=\"{align}\"/><a:r>{r_pr}<a:t xml:space=\"preserve\">{}</a:t></a:r></a:p>",
                esc_xml(line)
            ))
            .collect::<Vec<_>>()
            .join("")
    };
    format!("<xdr:txBody><a:bodyPr wrap=\"square\" rtlCol=\"0\" anchor=\"ctr\"/><a:lstStyle/>{paras}</xdr:txBody>")
}

/// `<xdr:sp>` for one shape. `ext` is the shape's size in EMU (the anchor's own
/// `<xdr:ext>` carries it too; Excel still expects it on the shape transform).
pub fn sp_xml(cnv_id: usize, shape: &Value, link_rid: Option<&str>, ext: (i64, i64)) -> String {
    let gs = |k: &str| shape.get(k).and_then(|v| v.as_str());
    let rot = shape.get("rot").and_then(|v| v.as_f64()).filter(|r| *r != 0.0)
        .map(|r| format!(" rot=\"{}\"", (r * 60_000.0).round() as i64))
        .unwrap_or_default();
    // Mirroring (the Flip menu) rides on the same transform as the rotation.
    let flip = |k: &str, at: &str| if shape.get(k).and_then(|v| v.as_bool()).unwrap_or(false) {
        format!(" {at}=\"1\"")
    } else {
        String::new()
    };
    let rot = format!("{rot}{}{}", flip("flipH", "flipH"), flip("flipV", "flipV"));
    let ln = line_xml(gs("border"), shape.get("borderWidth").and_then(|v| v.as_f64()));
    format!(concat!(
        "<xdr:sp macro=\"\" textlink=\"\"><xdr:nvSpPr>{}<xdr:cNvSpPr/></xdr:nvSpPr>",
        "<xdr:spPr><a:xfrm{}><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm>",
        "<a:prstGeom prst=\"{}\">{}</a:prstGeom>{}{}</xdr:spPr>{}</xdr:sp>"
    ),
        cnv_pr_xml(cnv_id, "Forme", gs("altText"), link_rid),
        rot, ext.0.max(1), ext.1.max(1),
        preset_of_kind(gs("kind").unwrap_or("rect")).0,
        av_lst_xml(shape),
        fill_xml(gs("fill")),
        ln,
        tx_body_xml(shape),
    )
}
