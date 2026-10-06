//! Styles part (xl/styles.xml) parsing: numFmts, fonts, fills, borders,
//! cellXfs and dxfs (differential formats used by conditional formatting).
//!
//! Colours are resolved through a `ColorCtx`: the real theme palette parsed
//! from xl/theme/theme1.xml (falling back to the Office 2007 defaults) plus
//! the legacy indexed palette, which the file may redefine via
//! `<colors><indexedColors>` (extracted in a pre-pass since that section
//! comes after the styles that reference it).
use std::collections::HashMap;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use serde_json::{json, Map, Value};

use super::super::util::{attr, numfmt_code, ColorCtx};

#[derive(Default, Clone)]
pub struct Font {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub size: Option<f64>,
    pub color: Option<String>,
    pub name: Option<String>,
}

// Each edge: (colour, width-in-px). Width from the xlsx line style.
#[derive(Default, Clone)]
pub struct Border {
    pub top: Option<(String, u8)>,
    pub right: Option<(String, u8)>,
    pub bottom: Option<(String, u8)>,
    pub left: Option<(String, u8)>,
}

// Map an xlsx border line style to a pixel width (thin/hair → 1, medium → 2, thick/double → 3).
pub fn border_width(style: &str) -> u8 {
    match style { "medium" | "mediumDashed" | "mediumDashDot" | "mediumDashDotDot" => 2, "thick" | "double" => 3, _ => 1 }
}

#[derive(Default, Clone)]
pub struct Xf {
    pub font: usize,
    pub fill: usize,
    pub border: usize,
    pub num_fmt: u32,
    pub halign: Option<String>,
    pub valign: Option<String>,
    pub wrap: bool,
    pub apply_fill: bool,
}

#[derive(Default)]
pub struct Styles {
    pub num_fmts: HashMap<u32, String>,
    pub fonts:    Vec<Font>,
    pub fills:    Vec<Option<String>>, // resolved bg colour (None = no fill)
    pub borders:  Vec<Border>,
    pub xfs:      Vec<Xf>,
    pub dxfs:     Vec<Value>,          // differential formats (for conditional formatting)
    pub ctx:      ColorCtx,            // resolved palettes, reused by worksheet-level colours (CF scales/bars)
}

// Read the rgb/theme/indexed/tint attributes of a colour element and resolve
// them against the palette context. `auto="1"` (and indexed 64/65) yield None
// so the caller keeps its default colour.
fn color_of(ctx: &ColorCtx, e: &BytesStart) -> Option<String> {
    let tint = attr(e, b"tint").and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let theme = attr(e, b"theme").and_then(|v| v.parse().ok());
    let indexed = attr(e, b"indexed").and_then(|v| v.parse().ok());
    ctx.resolve(attr(e, b"rgb").as_deref(), theme, indexed, tint)
}

// Boolean font-property element: `<b/>` is true, `<b val="0"/>`/`val="false"` is false.
fn flag_on(e: &BytesStart) -> bool {
    !matches!(attr(e, b"val").as_deref(), Some("0") | Some("false"))
}

// Pre-pass: extract an `<indexedColors>` palette override (position-indexed).
fn parse_indexed_colors(xml: &str) -> Vec<(usize, String)> {
    let mut reader = Reader::from_str(xml);
    let mut in_ic = false;
    let mut i = 0usize;
    let mut out = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.local_name().as_ref() {
                b"indexedColors" => in_ic = true,
                b"rgbColor" if in_ic => {
                    if let Some(rgb) = attr(&e, b"rgb") {
                        let h = if rgb.len() == 8 { &rgb[2..] } else { &rgb[..] };
                        if h.len() == 6 { out.push((i, h.to_uppercase())); }
                    }
                    i += 1;
                }
                _ => {}
            },
            Ok(Event::End(e)) if e.local_name().as_ref() == b"indexedColors" => break,
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

pub fn parse_styles(xml: &str, theme: Option<Vec<String>>) -> Styles {
    // Build the colour context first: real theme palette + (possibly
    // redefined) indexed palette.
    let mut ctx = ColorCtx::default();
    if let Some(t) = theme { if t.len() == 12 { ctx.theme = t; } }
    for (i, c) in parse_indexed_colors(xml) {
        if i < ctx.indexed.len() { ctx.indexed[i] = c; }
    }

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut st = Styles::default();
    // section: 0 none, 1 fonts, 2 fills, 3 borders, 4 cellXfs, 5 dxfs
    let mut section = 0u8;
    let mut cur_font = Font::default();
    let mut cur_fill: Option<String> = None;
    // Fill state: patternType of the current <patternFill> (None outside one)
    // and whether we are inside a <gradientFill>.
    let mut cur_pattern: Option<String> = None;
    let mut in_gradient = false;
    let mut cur_border = Border::default();
    let mut cur_edge = 0u8; // 1 top 2 right 3 bottom 4 left
    let mut cur_edge_width = 0u8;
    let mut cur_dxf = Map::new();
    let mut in_dxf_font = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let ln = e.local_name();
                match ln.as_ref() {
                    b"numFmts" | b"fonts" | b"fills" | b"borders" | b"cellXfs" | b"dxfs" => {
                        section = match ln.as_ref() {
                            b"fonts" => 1, b"fills" => 2, b"borders" => 3, b"cellXfs" => 4, b"dxfs" => 5, _ => 0,
                        };
                    }
                    // ── Differential formats (dxfs) for conditional formatting ──
                    b"dxf" if section == 5 => { cur_dxf = Map::new(); in_dxf_font = false; }
                    b"font" if section == 5 => in_dxf_font = true,
                    b"b" if section == 5 && in_dxf_font && flag_on(&e) => { cur_dxf.insert("bold".into(), json!(true)); }
                    b"i" if section == 5 && in_dxf_font && flag_on(&e) => { cur_dxf.insert("italic".into(), json!(true)); }
                    b"color" if section == 5 && in_dxf_font => {
                        if let Some(c) = color_of(&ctx, &e) { cur_dxf.insert("color".into(), json!(c)); }
                    }
                    b"bgColor" if section == 5 => { // dxf solid fill colour
                        if let Some(c) = color_of(&ctx, &e) { cur_dxf.insert("bg".into(), json!(c)); }
                    }
                    b"numFmt" => {
                        if let (Some(id), Some(code)) = (attr(&e, b"numFmtId"), attr(&e, b"formatCode")) {
                            if let Ok(id) = id.parse::<u32>() { st.num_fmts.insert(id, code); }
                        }
                    }
                    b"font" if section == 1 => cur_font = Font::default(),
                    b"b" if section == 1 => cur_font.bold = flag_on(&e),
                    b"i" if section == 1 => cur_font.italic = flag_on(&e),
                    // <u/> = single; val may be double/singleAccounting/… or "none".
                    b"u" if section == 1 => cur_font.underline = attr(&e, b"val").as_deref() != Some("none"),
                    b"strike" if section == 1 => cur_font.strike = flag_on(&e),
                    b"sz" if section == 1 => cur_font.size = attr(&e, b"val").and_then(|v| v.parse().ok()),
                    b"name" if section == 1 => cur_font.name = attr(&e, b"val"),
                    b"color" if section == 1 => cur_font.color = color_of(&ctx, &e),
                    b"fill" if section == 2 => { cur_fill = None; cur_pattern = None; in_gradient = false; }
                    b"patternFill" if section == 2 => {
                        cur_pattern = Some(attr(&e, b"patternType").unwrap_or_else(|| "none".into()));
                    }
                    b"gradientFill" if section == 2 => in_gradient = true,
                    b"fgColor" if section == 2 => {
                        // solid → exact colour; other patterns (gray125, darkGray, …)
                        // → approximate with the pattern foreground colour.
                        if matches!(cur_pattern.as_deref(), Some(p) if p != "none") {
                            if let Some(c) = color_of(&ctx, &e) { cur_fill = Some(c); }
                        }
                    }
                    // Gradient fill approximated by its first stop colour.
                    b"color" if section == 2 && in_gradient && cur_fill.is_none() => {
                        cur_fill = color_of(&ctx, &e);
                    }
                    b"border" if section == 3 => cur_border = Border::default(),
                    b"left" | b"right" | b"top" | b"bottom" if section == 3 => {
                        cur_edge = match ln.as_ref() { b"top" => 1, b"right" => 2, b"bottom" => 3, b"left" => 4, _ => 0 };
                        let style = attr(&e, b"style");
                        cur_edge_width = match style.as_deref() { Some(s) if s != "none" => border_width(s), _ => 0 };
                        // Pre-set the edge (default black) so a styled edge with no explicit
                        // <color> child still gets a border; <color> below refines the colour.
                        if cur_edge_width > 0 {
                            let v = Some(("#000000".to_string(), cur_edge_width));
                            match cur_edge { 1 => cur_border.top = v, 2 => cur_border.right = v, 3 => cur_border.bottom = v, 4 => cur_border.left = v, _ => {} }
                        }
                    }
                    b"color" if section == 3 && cur_edge != 0 && cur_edge_width > 0 => {
                        let c = color_of(&ctx, &e).unwrap_or_else(|| "#000000".into());
                        let v = Some((c, cur_edge_width));
                        match cur_edge { 1 => cur_border.top = v, 2 => cur_border.right = v, 3 => cur_border.bottom = v, 4 => cur_border.left = v, _ => {} }
                    }
                    b"xf" if section == 4 => {
                        st.xfs.push(Xf {
                            font: attr(&e, b"fontId").and_then(|v| v.parse().ok()).unwrap_or(0),
                            fill: attr(&e, b"fillId").and_then(|v| v.parse().ok()).unwrap_or(0),
                            border: attr(&e, b"borderId").and_then(|v| v.parse().ok()).unwrap_or(0),
                            num_fmt: attr(&e, b"numFmtId").and_then(|v| v.parse().ok()).unwrap_or(0),
                            apply_fill: attr(&e, b"applyFill").as_deref() == Some("1"),
                            ..Default::default()
                        });
                    }
                    b"alignment" if section == 4 => {
                        if let Some(last) = st.xfs.last_mut() {
                            last.halign = attr(&e, b"horizontal");
                            last.valign = attr(&e, b"vertical");
                            last.wrap = attr(&e, b"wrapText").as_deref() == Some("1");
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"font" if section == 1 => st.fonts.push(std::mem::take(&mut cur_font)),
                b"fill" if section == 2 => { st.fills.push(cur_fill.take()); cur_pattern = None; in_gradient = false; }
                b"border" if section == 3 => st.borders.push(std::mem::take(&mut cur_border)),
                b"left" | b"right" | b"top" | b"bottom" if section == 3 => { cur_edge = 0; cur_edge_width = 0; }
                b"fonts" | b"fills" | b"borders" | b"cellXfs" | b"dxfs" => section = 0,
                b"font" if section == 5 => in_dxf_font = false,
                b"dxf" if section == 5 => st.dxfs.push(Value::Object(std::mem::take(&mut cur_dxf))),
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    st.ctx = ctx;
    st
}

// Build the CellData style object for a given cell-format (xf) index.
pub fn style_for(styles: &Styles, xf_idx: usize) -> Option<Value> {
    let xf = styles.xfs.get(xf_idx)?;
    let mut s = Map::new();
    if let Some(f) = styles.fonts.get(xf.font) {
        if f.bold { s.insert("bold".into(), json!(true)); }
        if f.italic { s.insert("italic".into(), json!(true)); }
        if f.underline { s.insert("underline".into(), json!(true)); }
        if f.strike { s.insert("strike".into(), json!(true)); }
        if let Some(sz) = f.size { s.insert("fontSize".into(), json!((sz * 4.0 / 3.0).round())); } // pt → px
        if let Some(c) = &f.color { if c != "#000000" { s.insert("color".into(), json!(c)); } }
        // Default font of an Excel workbook = Calibri when unnamed.
        s.insert("fontFamily".into(), json!(f.name.clone().unwrap_or_else(|| "Calibri".to_string())));
    }
    if xf.apply_fill || styles.fills.get(xf.fill).map(|f| f.is_some()).unwrap_or(false) {
        if let Some(Some(bg)) = styles.fills.get(xf.fill) { s.insert("bg".into(), json!(bg)); }
    }
    if let Some(b) = styles.borders.get(xf.border) {
        // Emit the colour (bt/br/bb/bl) plus a width (btw/…) when thicker than 1px.
        let mut edge = |k: &str, kw: &str, e: &Option<(String, u8)>| {
            if let Some((c, w)) = e { s.insert(k.into(), json!(c)); if *w > 1 { s.insert(kw.into(), json!(w)); } }
        };
        edge("bt", "btw", &b.top);
        edge("br", "brw", &b.right);
        edge("bb", "bbw", &b.bottom);
        edge("bl", "blw", &b.left);
    }
    match xf.halign.as_deref() {
        Some("center") | Some("centerContinuous") => { s.insert("align".into(), json!("center")); }
        Some("right") => { s.insert("align".into(), json!("right")); }
        Some("left") => { s.insert("align".into(), json!("left")); }
        _ => {}
    }
    if xf.wrap { s.insert("wrap".into(), json!(true)); }
    if matches!(xf.valign.as_deref(), Some("center") | Some("top") | Some("bottom")) {
        if let Some(v) = xf.valign.clone() { s.insert("valign".into(), json!(v)); }
    }
    // Number format code (builtin or custom) → applied at display time (dates, "00", …).
    if let Some(code) = numfmt_code(xf.num_fmt, &styles.num_fmts) {
        s.insert("numFmtCode".into(), json!(code));
    }
    if s.is_empty() { None } else { Some(Value::Object(s)) }
}
