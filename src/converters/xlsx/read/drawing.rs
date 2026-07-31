//! Drawing parts (xl/drawings/*) parsing: picture, chart and shape anchors, plus
//! the resolution of a worksheet's drawings into images (data URLs), charts and
//! shapes.
use std::collections::HashMap;
use std::io::Cursor;

use quick_xml::events::Event;
use quick_xml::Reader;
use serde_json::{json, Map, Value};

use super::super::util::{attr, col_to_idx, image_mime, read_zip_bytes, read_zip_text, resolve_path};
use super::chart::{normalize_refs, parse_chart_xml};
use super::shape::parse_shape;
use super::workbook::parse_rels;

/// EMU per pixel at 96 dpi (mirrors the frontend's EMU_PER_PX).
const EMU_PER_PX: f64 = 9525.0;
/// Frontend grid defaults (SpreadsheetApp DEFAULT_COL_WIDTH / DEFAULT_ROW_HEIGHT).
const DEFAULT_COL_W: f64 = 100.0;
const DEFAULT_ROW_H: f64 = 24.0;

// `util::attr` hands back raw attribute values, entities included — fine for the
// numeric and enum attributes it is normally used on, not for human text. Alt
// text and hyperlink targets are decoded so the model stores real strings.
fn unescape_attr(s: &str) -> String {
    quick_xml::escape::unescape(s).map(|c| c.into_owned()).unwrap_or_else(|_| s.to_string())
}

/// What an anchor carries.
pub enum AnchorObj {
    /// Picture: relationship id of the embedded media part.
    Image(String),
    /// Chart: relationship id of the chart part.
    Chart(String),
    /// Shape: its own properties (kind, fill, outline, caption).
    Shape(Box<Map<String, Value>>),
}

/// One anchored drawing object: its position (grid coordinates, EMU offsets),
/// the object itself and the relationship id of an `a:hlinkClick`, if any.
pub struct DrawingAnchor {
    pub anchor:   Map<String, Value>,
    pub obj:      AnchorObj,
    pub link_rid: Option<String>,
}

// Per-anchor scratch state (reset on every anchor start).
#[derive(Default)]
struct AnchorScratch {
    embed:    Option<String>,
    chart:    Option<String>,
    shape:    Option<Map<String, Value>>,
    link_rid: Option<String>,
    alt_text: Option<String>,
    // Outline / shadow of a picture (only merged when the anchor holds a pic:
    // a shape reads its own from `parse_shape`).
    ln_width: Option<f64>,
    ln_color: Option<String>,
    ln_none:  bool,
    shadow:   bool,
}

/// Parse a drawing part into its anchored objects. The anchor map holds the
/// position in grid coordinates: `from` (col/colOff/row/rowOff) plus either `to`
/// (twoCellAnchor) or `ext` cx/cy in EMU (oneCellAnchor). Offsets stay in EMU;
/// the frontend converts to pixels (1 px = 9525 EMU) against its column/row
/// geometry.
pub fn parse_drawing(xml: &str) -> Vec<DrawingAnchor> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut out: Vec<DrawingAnchor> = Vec::new();
    let mut cur: Option<Map<String, Value>> = None;
    let mut sc = AnchorScratch::default();
    let mut side = 0u8;     // 1 = inside <from>, 2 = inside <to>
    let mut field = 0u8;    // 1 col, 2 colOff, 3 row, 4 rowOff
    let mut ln_depth = 0u32;   // inside <a:ln> (its colour is the outline, not the fill)
    let mut fallback = 0u32;   // inside <mc:Fallback> — the <mc:Choice> twin already won
    let mut sp_depth = 0u32;   // nesting of <xdr:sp> (a group shape holds several)
    let mut sp_start = 0usize; // byte offset of the top-level <xdr:sp> content
    loop {
        let ev = reader.read_event();
        let pos = reader.buffer_position() as usize;
        match ev {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let start = matches!(ev, Ok(Event::Start(_)));
                let name = e.local_name();
                let name = name.as_ref();
                if start {
                    match name {
                        b"Fallback" => fallback += 1,
                        b"ln" => ln_depth += 1,
                        b"sp" => {
                            if sp_depth == 0 { sp_start = pos; }
                            sp_depth += 1;
                        }
                        _ => {}
                    }
                }
                if fallback > 0 { continue; }
                match name {
                    b"twoCellAnchor" | b"oneCellAnchor" | b"absoluteAnchor" => {
                        cur = Some(Map::new());
                        sc = AnchorScratch::default();
                    }
                    // absoluteAnchor position (EMU from the sheet origin): modelled as an
                    // offset from cell A1 so the frontend geometry code applies unchanged.
                    b"pos" => {
                        if let Some(m) = cur.as_mut() {
                            m.insert("fromCol".into(), json!(0));
                            m.insert("fromRow".into(), json!(0));
                            if let Some(x) = attr(e, b"x").and_then(|v| v.parse::<i64>().ok()) { m.insert("fromColOff".into(), json!(x)); }
                            if let Some(y) = attr(e, b"y").and_then(|v| v.parse::<i64>().ok()) { m.insert("fromRowOff".into(), json!(y)); }
                        }
                    }
                    b"from" => side = 1,
                    b"to" => side = 2,
                    b"col" => field = 1,
                    b"colOff" => field = 2,
                    b"row" => field = 3,
                    b"rowOff" => field = 4,
                    b"ext" => {
                        // First <ext> wins: the anchor's own <xdr:ext> (real size) precedes the
                        // inner shape's <a:ext>. A 0×0 ext (mandatory but meaningless on chart
                        // graphicFrames inside a twoCellAnchor) is ignored so it never shadows
                        // the real geometry.
                        if let Some(m) = cur.as_mut() {
                            if !m.contains_key("extCx") {
                                let cx = attr(e, b"cx").and_then(|v| v.parse::<i64>().ok());
                                let cy = attr(e, b"cy").and_then(|v| v.parse::<i64>().ok());
                                if cx.unwrap_or(0) > 0 || cy.unwrap_or(0) > 0 {
                                    if let Some(cx) = cx { m.insert("extCx".into(), json!(cx)); }
                                    if let Some(cy) = cy { m.insert("extCy".into(), json!(cy)); }
                                }
                            }
                        }
                    }
                    b"blip" => { if let Some(r) = attr(e, b"embed") { sc.embed = Some(r); } }
                    // <c:chart r:id="…"/> inside a graphicFrame → the anchor holds a chart.
                    b"chart" => { if let Some(r) = attr(e, b"id") { sc.chart = Some(r); } }
                    // Non-visual properties, shared by pictures, chart frames and shapes:
                    // `descr` is the accessibility description ("alt text").
                    b"cNvPr" => {
                        if let Some(d) = attr(e, b"descr").filter(|d| !d.trim().is_empty()) {
                            sc.alt_text = Some(unescape_attr(&d));
                        }
                    }
                    // Hyperlink on the object itself (resolved against the drawing's rels).
                    b"hlinkClick" => {
                        if let Some(r) = attr(e, b"id").filter(|r| !r.is_empty()) { sc.link_rid = Some(r); }
                    }
                    // Rotation (a:xfrm rot, in 60000ths of a degree) on the object's shape.
                    b"xfrm" => {
                        if let Some(m) = cur.as_mut() {
                            if let Some(r) = attr(e, b"rot").and_then(|v| v.parse::<f64>().ok()) {
                                if r != 0.0 { m.insert("rot".into(), json!(r / 60_000.0)); }
                            }
                            // Mirroring (the Flip menu) sits on the same transform.
                            for (at, key) in [(&b"flipH"[..], "flipH"), (&b"flipV"[..], "flipV")] {
                                if attr(e, at).as_deref() == Some("1") { m.insert(key.into(), json!(true)); }
                            }
                        }
                    }
                    // Outline width (EMU) — a picture's only <a:ln> lives in its spPr.
                    b"ln" => {
                        if let Some(w) = attr(e, b"w").and_then(|v| v.parse::<f64>().ok()) {
                            sc.ln_width = Some(((w / 12_700.0) * 10.0).round() / 10.0);
                        }
                    }
                    b"noFill" if ln_depth > 0 => sc.ln_none = true,
                    b"srgbClr" if ln_depth > 0 && sc.ln_color.is_none() => {
                        if let Some(v) = attr(e, b"val") { sc.ln_color = Some(format!("#{}", v.to_uppercase())); }
                    }
                    b"outerShdw" => sc.shadow = true,
                    // Crop insets (a:srcRect inside blipFill): l/t/r/b in 1000ths of a
                    // percent → fractions of the source image cut from each side.
                    b"srcRect" => {
                        if let Some(m) = cur.as_mut() {
                            let frac = |name: &[u8]| attr(e, name).and_then(|v| v.parse::<f64>().ok()).map(|n| n / 100_000.0);
                            if let Some(v) = frac(b"l") { m.insert("cropL".into(), json!(v)); }
                            if let Some(v) = frac(b"t") { m.insert("cropT".into(), json!(v)); }
                            if let Some(v) = frac(b"r") { m.insert("cropR".into(), json!(v)); }
                            if let Some(v) = frac(b"b") { m.insert("cropB".into(), json!(v)); }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) if field != 0 && side != 0 && fallback == 0 => {
                if let (Some(m), Ok(n)) = (cur.as_mut(), e.unescape().unwrap_or_default().parse::<i64>()) {
                    let prefix = if side == 1 { "from" } else { "to" };
                    let suffix = match field { 1 => "Col", 2 => "ColOff", 3 => "Row", _ => "RowOff" };
                    m.insert(format!("{prefix}{suffix}"), json!(n));
                }
            }
            Ok(Event::End(ref e)) => {
                let name = e.local_name();
                let name = name.as_ref();
                match name {
                    b"Fallback" => { fallback = fallback.saturating_sub(1); continue; }
                    b"ln" => ln_depth = ln_depth.saturating_sub(1),
                    b"sp" => {
                        sp_depth = sp_depth.saturating_sub(1);
                        if sp_depth == 0 && fallback == 0 && sc.shape.is_none() {
                            // `pos` is just past "</xdr:sp>": cut the closing tag off so
                            // the fragment parses as a well-formed run of children.
                            let frag = xml.get(sp_start..pos).unwrap_or("");
                            let frag = frag.rfind('<').map(|p| &frag[..p]).unwrap_or(frag);
                            sc.shape = Some(parse_shape(frag));
                        }
                    }
                    _ => {}
                }
                if fallback > 0 { continue; }
                match name {
                    b"col" | b"colOff" | b"row" | b"rowOff" => field = 0,
                    b"from" | b"to" => side = 0,
                    b"twoCellAnchor" | b"oneCellAnchor" | b"absoluteAnchor" => {
                        if let Some(mut m) = cur.take() {
                            let sc = std::mem::take(&mut sc);
                            let obj = if let Some(r) = sc.chart { Some(AnchorObj::Chart(r)) }
                                else if let Some(r) = sc.embed {
                                    // Picture formatting lives on the anchor's own map.
                                    if sc.ln_none { m.insert("border".into(), json!("none")); }
                                    else if let Some(c) = sc.ln_color { m.insert("border".into(), json!(c)); }
                                    if let Some(w) = sc.ln_width { m.insert("borderWidth".into(), json!(w)); }
                                    if sc.shadow { m.insert("shadow".into(), json!(true)); }
                                    Some(AnchorObj::Image(r))
                                }
                                else { sc.shape.map(|s| AnchorObj::Shape(Box::new(s))) };
                            if let Some(obj) = obj {
                                if let Some(a) = sc.alt_text { m.insert("altText".into(), json!(a)); }
                                out.push(DrawingAnchor { anchor: m, obj, link_rid: sc.link_rid });
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

// ── Shape geometry: EMU cell anchor → pixel box ──────────────────────────────

/// The sheet geometry a shape's pixel box is resolved against. Charts and
/// pictures keep their EMU anchor verbatim (the frontend resolves it lazily),
/// but `SheetShape` only models an explicit box, so it is computed at import.
pub struct SheetGeom<'a> {
    pub col_widths:  &'a HashMap<String, f64>,
    pub row_heights: &'a HashMap<i32, f64>,
    pub default_col_width:  Option<f64>,
    pub default_row_height: Option<f64>,
}

impl SheetGeom<'_> {
    // Pixel offset of the left edge of column `upto` (0-based). Computed from the
    // default width plus the deltas of the sized columns before it, so a shape
    // anchored far right costs a map walk, not a 16384-step loop.
    fn col_px(&self, upto: i64) -> f64 {
        let def = self.default_col_width.unwrap_or(DEFAULT_COL_W);
        let upto = upto.max(0);
        let mut px = def * upto as f64;
        for (k, w) in self.col_widths {
            if (col_to_idx(k) as i64) < upto { px += w - def; }
        }
        px
    }

    // Pixel offset of the top edge of row `upto` (0-based).
    fn row_px(&self, upto: i64) -> f64 {
        let def = self.default_row_height.unwrap_or(DEFAULT_ROW_H);
        let upto = upto.max(0);
        let mut px = def * upto as f64;
        for (r, h) in self.row_heights {
            if (*r as i64) <= upto && *r >= 1 { px += h - def; }
        }
        px
    }
}

// Replace a shape's EMU cell anchor with the explicit pixel box the model uses.
//
// When the shape carries its own `a:ext` (its size before rotation) that size
// wins and the box is centred on the anchor rectangle: Excel anchors a rotated
// shape by its ROTATED bounding box, whereas the model rotates an upright box
// about its centre — the two agree only on the centre.
fn shape_box(anchor: &mut Map<String, Value>, geom: &SheetGeom) {
    let gi = |m: &Map<String, Value>, k: &str| m.get(k).and_then(|v| v.as_i64());
    let x0 = geom.col_px(gi(anchor, "fromCol").unwrap_or(0)) + gi(anchor, "fromColOff").unwrap_or(0) as f64 / EMU_PER_PX;
    let y0 = geom.row_px(gi(anchor, "fromRow").unwrap_or(0)) + gi(anchor, "fromRowOff").unwrap_or(0) as f64 / EMU_PER_PX;
    let to = match (gi(anchor, "toCol"), gi(anchor, "toRow")) {
        (Some(tc), Some(tr)) => Some((
            geom.col_px(tc) + gi(anchor, "toColOff").unwrap_or(0) as f64 / EMU_PER_PX,
            geom.row_px(tr) + gi(anchor, "toRowOff").unwrap_or(0) as f64 / EMU_PER_PX,
        )),
        _ => None,
    };
    let ext = match (gi(anchor, "extCx"), gi(anchor, "extCy")) {
        (Some(cx), Some(cy)) if cx > 0 && cy > 0 => Some((cx as f64 / EMU_PER_PX, cy as f64 / EMU_PER_PX)),
        _ => None,
    };
    let (bx, by, bw, bh) = match (ext, to) {
        (Some((w, h)), Some((x1, y1))) => ((x0 + x1 - w) / 2.0, (y0 + y1 - h) / 2.0, w, h),
        (Some((w, h)), None) => (x0, y0, w, h),
        (None, Some((x1, y1))) => (x0, y0, (x1 - x0).max(1.0), (y1 - y0).max(1.0)),
        (None, None) => (x0, y0, 100.0, 60.0),
    };
    for k in ["fromCol", "fromColOff", "fromRow", "fromRowOff",
              "toCol", "toColOff", "toRow", "toRowOff", "extCx", "extCy"] {
        anchor.remove(k);
    }
    let r1 = |v: f64| (v * 10.0).round() / 10.0;
    anchor.insert("bx".into(), json!(r1(bx.max(0.0))));
    anchor.insert("by".into(), json!(r1(by.max(0.0))));
    anchor.insert("bw".into(), json!(r1(bw.max(1.0))));
    anchor.insert("bh".into(), json!(r1(bh.max(1.0))));
}

// ── Worksheet resolution ─────────────────────────────────────────────────────

/// Resolve a worksheet's drawings → (images, charts, shapes). Images become
/// base64 data URLs; charts capture their type + category/value cell refs from
/// the linked chart part; shapes carry their preset geometry, style and caption.
pub fn extract_sheet_drawings(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    sheet_path: &str,
    geom: &SheetGeom,
) -> (Vec<Value>, Vec<Value>, Vec<Value>) {
    use base64::Engine;
    let (mut images, mut charts, mut shapes) = (Vec::new(), Vec::new(), Vec::new());
    let (dir, file) = match sheet_path.rsplit_once('/') { Some(v) => v, None => return (images, charts, shapes) };
    let sheet_rels = format!("{dir}/_rels/{file}.rels");
    let rels = match read_zip_text(archive, &sheet_rels) { Some(x) => parse_rels(&x), None => return (images, charts, shapes) };
    let drawing_targets: Vec<String> = rels.values().filter(|t| t.contains("drawings/")).cloned().collect();
    for dt in drawing_targets {
        let drawing_path = resolve_path(sheet_path, &dt);
        let Some(dxml) = read_zip_text(archive, &drawing_path) else { continue };
        let anchors = parse_drawing(&dxml);
        if anchors.is_empty() { continue }
        let drels = read_zip_text(archive, &format!("{}/_rels/{}.rels",
            drawing_path.rsplit_once('/').map(|(d, _)| d).unwrap_or(""),
            drawing_path.rsplit_once('/').map(|(_, f)| f).unwrap_or("")))
            .map(|x| parse_rels(&x)).unwrap_or_default();
        for da in anchors {
            let DrawingAnchor { mut anchor, obj, link_rid } = da;
            // An object hyperlink points at an external relationship target.
            if let Some(t) = link_rid.and_then(|r| drels.get(&r)).filter(|t| !t.is_empty()) {
                anchor.insert("link".into(), json!(unescape_attr(t)));
            }
            match obj {
                AnchorObj::Shape(props) => {
                    shape_box(&mut anchor, geom);
                    anchor.insert("id".into(), json!(format!("xsp{}", shapes.len() + 1)));
                    for (k, v) in props.into_iter() { anchor.insert(k, v); }
                    shapes.push(Value::Object(anchor));
                }
                AnchorObj::Chart(rid) => {
                    let Some(target) = drels.get(&rid) else { continue };
                    let Some(cxml) = read_zip_text(archive, &resolve_path(&drawing_path, target)) else { continue };
                    let p = parse_chart_xml(&cxml);
                    // Flat refs (what the frontend renders): concatenation of all series.
                    let flat_vals: Vec<String> = p.series.iter()
                        .flat_map(|s| s.vals.iter().flat_map(|f| normalize_refs(f))).collect();
                    if flat_vals.is_empty() { continue }
                    let flat_cats: Vec<String> = p.series.iter()
                        .flat_map(|s| s.cats.iter().flat_map(|f| normalize_refs(f))).collect();
                    anchor.insert("type".into(), json!(p.ctype));
                    anchor.insert("vals".into(), json!(flat_vals));
                    if !flat_cats.is_empty() { anchor.insert("cats".into(), json!(flat_cats)); }
                    anchor.insert("legend".into(), json!(p.legend));
                    if let Some(lp) = p.legend_pos { anchor.insert("legendPos".into(), json!(lp)); }
                    if let Some(dl) = p.data_labels { anchor.insert("dataLabels".into(), json!(dl)); }
                    if !p.colors.is_empty() { anchor.insert("colors".into(), json!(p.colors)); }
                    if let Some(t) = p.title { anchor.insert("title".into(), json!(t)); }
                    if let Some(g) = p.grouping { anchor.insert("grouping".into(), json!(g)); }
                    // Chart-area formatting (chartSpace spPr / txPr).
                    if let Some(f) = p.fill { anchor.insert("fill".into(), json!(f)); }
                    if let Some(b) = p.border { anchor.insert("border".into(), json!(b)); }
                    if let Some(w) = p.border_width { anchor.insert("borderWidth".into(), json!(w)); }
                    if let Some(f) = p.font { anchor.insert("font".into(), json!(f)); }
                    // v2 presentation options (absent on charts that never set them).
                    if let Some(h) = p.hole_size { anchor.insert("holeSize".into(), json!(h)); }
                    if let Some(x) = p.explode { anchor.insert("explode".into(), json!(x)); }
                    if p.filled { anchor.insert("filled".into(), json!(true)); }
                    if let Some(b) = p.smooth { anchor.insert("smooth".into(), json!(b)); }
                    if let Some(b) = p.symbols { anchor.insert("symbols".into(), json!(b)); }
                    if let Some(b) = p.lines { anchor.insert("lines".into(), json!(b)); }
                    if p.grid_x || p.grid_y {
                        let mut g = Map::new();
                        if p.grid_x { g.insert("x".into(), json!(true)); }
                        if p.grid_y { g.insert("y".into(), json!(true)); }
                        anchor.insert("grid".into(), Value::Object(g));
                    }
                    for (key, ax) in [("axisX", &p.axis_x), ("axisY", &p.axis_y)] {
                        let Some(ax) = ax else { continue };
                        let mut m = Map::new();
                        if let Some(t) = &ax.title { m.insert("title".into(), json!(t)); }
                        if let Some(v) = ax.min { m.insert("min".into(), json!(v)); }
                        if let Some(v) = ax.max { m.insert("max".into(), json!(v)); }
                        if !m.is_empty() { anchor.insert(key.into(), Value::Object(m)); }
                    }
                    let combo = p.ctype == "combo";
                    if p.series.len() > 1
                        || p.series.iter().any(|s| s.name.is_some() || s.color.is_some() || (combo && s.kind.is_some())) {
                        let sers: Vec<Value> = p.series.iter().map(|s| {
                            let mut m = Map::new();
                            if let Some(n) = &s.name { m.insert("name".into(), json!(n)); }
                            let cats: Vec<String> = s.cats.iter().flat_map(|f| normalize_refs(f)).collect();
                            if !cats.is_empty() { m.insert("cats".into(), json!(cats)); }
                            m.insert("vals".into(), json!(s.vals.iter().flat_map(|f| normalize_refs(f)).collect::<Vec<_>>()));
                            if let Some(c) = &s.color { m.insert("color".into(), json!(c)); }
                            // Combo charts tag their line-plot series; other types carry
                            // a single plot family so the tag would be redundant.
                            if combo && s.kind.as_deref() == Some("line") { m.insert("kind".into(), json!("line")); }
                            Value::Object(m)
                        }).collect();
                        anchor.insert("series".into(), json!(sers));
                    }
                    charts.push(Value::Object(anchor));
                }
                AnchorObj::Image(rid) => {
                    let Some(target) = drels.get(&rid) else { continue };
                    let part_path = resolve_path(&drawing_path, target);
                    let Some(bytes) = read_zip_bytes(archive, &part_path) else { continue };
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                    anchor.insert("src".into(), json!(format!("data:{};base64,{}", image_mime(&part_path), b64)));
                    images.push(Value::Object(anchor));
                }
            }
        }
    }
    (images, charts, shapes)
}
