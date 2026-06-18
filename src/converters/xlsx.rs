/// XLSX (Office Open XML Spreadsheet) importer.
///
/// A .xlsx file is a ZIP archive of XML parts:
///   - xl/sharedStrings.xml   shared string table (cells of type "s" index into it)
///   - xl/workbook.xml        sheet list (order + r:id) + defined names
///   - xl/_rels/workbook.xml.rels  r:id → worksheet path
///   - xl/worksheets/sheetN.xml    cells, merged ranges, column widths, row heights
///   - xl/styles.xml          numFmts, fonts, fills, borders, cellXfs
///   - xl/theme/theme1.xml    colour scheme (for theme colours)
///
/// We import values, formulas, defined names, merged cells, column/row sizes and a
/// faithful subset of cell styling (font weight/italic/underline/size/colour, fill
/// background, borders, alignment) so the rendered sheet resembles the original.
use std::collections::HashMap;
use std::io::{Cursor, Read};

use anyhow::Result;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use serde_json::{json, Map, Value};

// ── Public output structures ─────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct XlsxSheet {
    pub name:        String,
    pub cells:       HashMap<String, Value>, // "A1" → { v?, f?, s? }
    pub merges:      Vec<String>,            // "A1:B2"
    pub col_widths:  HashMap<String, f64>,   // column letter → px
    pub row_heights: HashMap<i32, f64>,      // 1-based row → px
    pub cond_formats: Vec<Value>,            // [{ ranges:[..], rules:[{type,op,formulas,dxf,stop}] }]
    pub show_gridlines: bool,                // sheetView showGridLines (default true)
    pub default_row_height: Option<f64>,     // sheetFormatPr defaultRowHeight → px
    pub images:      Vec<Value>,             // embedded pictures (anchor in grid coords + data URL)
}

#[derive(Debug, Default)]
pub struct XlsxWorkbook {
    pub sheets:        Vec<XlsxSheet>,
    pub defined_names: Vec<(String, String)>, // (name, formula like "='Sheet'!$A$1")
}

// ── Colour helpers ───────────────────────────────────────────────────────────

// Standard Office theme palette, indexed the SpreadsheetML way (0/1 and 2/3 are
// the lt/dk swap): 0=lt1 1=dk1 2=lt2 3=dk2 4..9=accent1..6 10=hlink 11=folHlink.
const THEME: [&str; 12] = [
    "FFFFFF", "000000", "EEECE1", "1F497D", "4F81BD", "C0504D",
    "9BBB59", "8064A2", "4BACC6", "F79646", "0000FF", "800080",
];

// Apply an Excel tint to a colour in HSL luminance space (matches Excel closely).
fn apply_tint(hex: &str, tint: f64) -> String {
    let r = i64::from_str_radix(&hex[0..2], 16).unwrap_or(0) as f64 / 255.0;
    let g = i64::from_str_radix(&hex[2..4], 16).unwrap_or(0) as f64 / 255.0;
    let b = i64::from_str_radix(&hex[4..6], 16).unwrap_or(0) as f64 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let mut l = (max + min) / 2.0;
    let d = max - min;
    let (mut h, mut s) = (0.0, 0.0);
    if d > f64::EPSILON {
        s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
        h = if max == r { (g - b) / d + if g < b { 6.0 } else { 0.0 } }
            else if max == g { (b - r) / d + 2.0 }
            else { (r - g) / d + 4.0 };
        h /= 6.0;
    }
    l = if tint < 0.0 { l * (1.0 + tint) } else { l * (1.0 - tint) + tint };
    let hue = |p: f64, q: f64, mut t: f64| {
        if t < 0.0 { t += 1.0 } if t > 1.0 { t -= 1.0 }
        if t < 1.0 / 6.0 { p + (q - p) * 6.0 * t }
        else if t < 1.0 / 2.0 { q }
        else if t < 2.0 / 3.0 { p + (q - p) * (2.0 / 3.0 - t) * 6.0 }
        else { p }
    };
    let (nr, ng, nb) = if s.abs() < f64::EPSILON {
        (l, l, l)
    } else {
        let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
        let p = 2.0 * l - q;
        (hue(p, q, h + 1.0 / 3.0), hue(p, q, h), hue(p, q, h - 1.0 / 3.0))
    };
    format!("#{:02X}{:02X}{:02X}", (nr * 255.0).round() as u8, (ng * 255.0).round() as u8, (nb * 255.0).round() as u8)
}

// Resolve a <color rgb/theme/indexed tint> element's attributes to "#RRGGBB".
fn resolve_color(rgb: Option<&str>, theme: Option<usize>, tint: f64) -> Option<String> {
    if let Some(rgb) = rgb {
        // ARGB "FFRRGGBB" or "RRGGBB".
        let h = if rgb.len() == 8 { &rgb[2..] } else { rgb };
        if h.len() == 6 { return Some(if tint != 0.0 { apply_tint(h, tint) } else { format!("#{h}") }); }
    }
    if let Some(t) = theme {
        if let Some(base) = THEME.get(t) { return Some(apply_tint(base, tint)); }
    }
    None
}

// ── Generic XML attribute helper ──────────────────────────────────────────────

fn attr(e: &BytesStart, name: &[u8]) -> Option<String> {
    e.attributes().flatten()
        .find(|a| a.key.local_name().as_ref() == name)
        .map(|a| String::from_utf8_lossy(&a.value).into_owned())
}

// ── Column letter ↔ index ─────────────────────────────────────────────────────

fn split_ref(r: &str) -> Option<(String, i32)> {
    let pos = r.find(|c: char| c.is_ascii_digit())?;
    let row = r[pos..].parse::<i32>().ok()?;
    Some((r[..pos].to_uppercase(), row))
}

fn col_to_idx(col: &str) -> usize {
    col.chars().fold(0usize, |a, c| a * 26 + (c as usize - 'A' as usize + 1)) - 1
}
fn idx_to_col(mut i: usize) -> String {
    let mut s = String::new();
    loop { s.insert(0, (b'A' + (i % 26) as u8) as char); if i < 26 { break } i = i / 26 - 1 }
    s
}

// Translate a shared formula to a follower cell by shifting relative references
// (those without a `$`) by (d_col, d_row). Skips strings, sheet-qualified refs
// and function names (a letters+digits run immediately followed by '(').
fn translate_formula(f: &str, d_col: i64, d_row: i64) -> String {
    if d_col == 0 && d_row == 0 { return f.to_string(); }
    let b = f.as_bytes();
    let mut out = String::with_capacity(f.len());
    let mut i = 0usize;
    let mut in_str = false;
    while i < b.len() {
        let c = b[i] as char;
        if in_str { out.push(c); if c == '"' { in_str = false } i += 1; continue; }
        if c == '"' { in_str = true; out.push(c); i += 1; continue; }
        let prev = out.chars().last().unwrap_or(' ');
        let boundary = !(prev.is_ascii_alphanumeric() || prev == '_' || prev == '!' || prev == '\'' || prev == '$' || prev == '.');
        if boundary && (c == '$' || c.is_ascii_alphabetic()) {
            let mut j = i;
            let col_abs = b[j] as char == '$'; if col_abs { j += 1; }
            let ls = j;
            while j < b.len() && (b[j] as char).is_ascii_alphabetic() { j += 1; }
            let letters = &f[ls..j];
            let row_abs = j < b.len() && b[j] as char == '$';
            let ds = if row_abs { j + 1 } else { j };
            let mut k = ds;
            while k < b.len() && (b[k] as char).is_ascii_digit() { k += 1; }
            let digits = &f[ds..k];
            let next = if k < b.len() { b[k] as char } else { ' ' };
            if !letters.is_empty() && letters.len() <= 3 && !digits.is_empty() && digits.len() <= 7 && next != '(' {
                let mut col = col_to_idx(&letters.to_uppercase()) as i64;
                let mut row = digits.parse::<i64>().unwrap_or(0);
                if !col_abs { col += d_col; }
                if !row_abs { row += d_row; }
                if col < 0 { col = 0; }
                if row < 1 { row = 1; }
                if col_abs { out.push('$'); }
                out.push_str(&idx_to_col(col as usize));
                if row_abs { out.push('$'); }
                out.push_str(&row.to_string());
                i = k;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

// ── Styles ──────────────────────────────────────────────────────────────────

#[derive(Default, Clone)]
struct Font { bold: bool, italic: bool, underline: bool, strike: bool, size: Option<f64>, color: Option<String>, name: Option<String> }
// Each edge: (colour, width-in-px). Width from the xlsx line style.
#[derive(Default, Clone)]
struct Border { top: Option<(String, u8)>, right: Option<(String, u8)>, bottom: Option<(String, u8)>, left: Option<(String, u8)> }

// Map an xlsx border line style to a pixel width (thin/hair → 1, medium → 2, thick/double → 3).
fn border_width(style: &str) -> u8 {
    match style { "medium" | "mediumDashed" | "mediumDashDot" | "mediumDashDotDot" => 2, "thick" | "double" => 3, _ => 1 }
}
#[derive(Default, Clone)]
struct Xf { font: usize, fill: usize, border: usize, num_fmt: u32, halign: Option<String>, valign: Option<String>, wrap: bool, apply_fill: bool }

#[derive(Default)]
struct Styles {
    num_fmts: HashMap<u32, String>,
    fonts:    Vec<Font>,
    fills:    Vec<Option<String>>, // resolved bg colour (None = no fill)
    borders:  Vec<Border>,
    xfs:      Vec<Xf>,
    dxfs:     Vec<Value>,          // differential formats (for conditional formatting)
}

fn read_zip_text(archive: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Option<String> {
    let mut f = archive.by_name(name).ok()?;
    let mut s = String::new();
    f.read_to_string(&mut s).ok()?;
    Some(s)
}

fn read_zip_bytes(archive: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Option<Vec<u8>> {
    let mut f = archive.by_name(name).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    Some(buf)
}

// Resolve a relationship target (possibly "../media/x.png" or "/xl/media/x.png")
// against the directory of the part that declared it (e.g. "xl/worksheets/sheet1.xml").
fn resolve_path(base_part: &str, target: &str) -> String {
    if let Some(abs) = target.strip_prefix('/') { return abs.to_string(); }
    let base_dir = base_part.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let mut segs: Vec<&str> = if base_dir.is_empty() { Vec::new() } else { base_dir.split('/').collect() };
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => { segs.pop(); }
            p => segs.push(p),
        }
    }
    segs.join("/")
}

// Guess an image MIME type from the media part's file extension.
fn image_mime(path: &str) -> &'static str {
    match path.rsplit('.').next().map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("bmp") => "image/bmp",
        Some("svg") => "image/svg+xml",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    }
}

// Parse a drawing part into a list of (anchor, embed-relationship-id) pairs. The anchor
// holds the picture's position in grid coordinates: `from` (col/colOff/row/rowOff) plus
// either `to` (twoCellAnchor) or `ext` cx/cy in EMU (oneCellAnchor). Offsets stay in EMU;
// the frontend converts to pixels (1 px = 9525 EMU) against its column/row geometry.
fn parse_drawing(xml: &str) -> Vec<(Map<String, Value>, String)> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut out = Vec::new();
    let mut cur: Option<Map<String, Value>> = None;
    let mut embed: Option<String> = None;
    let mut side = 0u8;     // 1 = inside <from>, 2 = inside <to>
    let mut field = 0u8;    // 1 col, 2 colOff, 3 row, 4 rowOff
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.local_name().as_ref() {
                b"twoCellAnchor" | b"oneCellAnchor" => { cur = Some(Map::new()); embed = None; }
                b"from" => side = 1,
                b"to" => side = 2,
                b"col" => field = 1,
                b"colOff" => field = 2,
                b"row" => field = 3,
                b"rowOff" => field = 4,
                b"ext" => {
                    if let Some(m) = cur.as_mut() {
                        if let Some(cx) = attr(&e, b"cx").and_then(|v| v.parse::<i64>().ok()) { m.insert("extCx".into(), json!(cx)); }
                        if let Some(cy) = attr(&e, b"cy").and_then(|v| v.parse::<i64>().ok()) { m.insert("extCy".into(), json!(cy)); }
                    }
                }
                b"blip" => { if let Some(r) = attr(&e, b"embed") { embed = Some(r); } }
                // Rotation (a:xfrm rot, in 60000ths of a degree) on the picture's shape.
                b"xfrm" => {
                    if let Some(m) = cur.as_mut() {
                        if let Some(r) = attr(&e, b"rot").and_then(|v| v.parse::<f64>().ok()) {
                            if r != 0.0 { m.insert("rot".into(), json!(r / 60000.0)); }
                        }
                    }
                }
                // Crop insets (a:srcRect inside blipFill): l/t/r/b in 1000ths of a
                // percent → fractions of the source image cut from each side.
                b"srcRect" => {
                    if let Some(m) = cur.as_mut() {
                        let frac = |name: &[u8]| attr(&e, name).and_then(|v| v.parse::<f64>().ok()).map(|n| n / 100_000.0);
                        if let Some(v) = frac(b"l") { m.insert("cropL".into(), json!(v)); }
                        if let Some(v) = frac(b"t") { m.insert("cropT".into(), json!(v)); }
                        if let Some(v) = frac(b"r") { m.insert("cropR".into(), json!(v)); }
                        if let Some(v) = frac(b"b") { m.insert("cropB".into(), json!(v)); }
                    }
                }
                _ => {}
            },
            Ok(Event::Text(e)) if field != 0 && side != 0 => {
                if let (Some(m), Ok(n)) = (cur.as_mut(), e.unescape().unwrap_or_default().parse::<i64>()) {
                    let prefix = if side == 1 { "from" } else { "to" };
                    let suffix = match field { 1 => "Col", 2 => "ColOff", 3 => "Row", _ => "RowOff" };
                    m.insert(format!("{prefix}{suffix}"), json!(n));
                }
            }
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"col" | b"colOff" | b"row" | b"rowOff" => field = 0,
                b"from" | b"to" => side = 0,
                b"twoCellAnchor" | b"oneCellAnchor" => {
                    if let (Some(m), Some(r)) = (cur.take(), embed.take()) { out.push((m, r)); }
                }
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

fn parse_shared_strings(xml: &str) -> Vec<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_si = false;
    let mut in_t = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match e.local_name().as_ref() {
                b"si" => { cur.clear(); in_si = true; }
                b"t" if in_si => in_t = true,
                _ => {}
            },
            Ok(Event::Text(e)) if in_t => cur.push_str(&e.unescape().unwrap_or_default()),
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"t" => in_t = false,
                b"si" => { out.push(std::mem::take(&mut cur)); in_si = false; }
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

fn parse_styles(xml: &str) -> Styles {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut st = Styles::default();
    // section: 0 none, 1 fonts, 2 fills, 3 borders, 4 cellXfs
    let mut section = 0u8;
    let mut cur_font = Font::default();
    let mut cur_fill: Option<String> = None;
    let mut in_pattern_solid = false;
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
                    b"b" if section == 5 && in_dxf_font => { cur_dxf.insert("bold".into(), json!(true)); }
                    b"i" if section == 5 && in_dxf_font => { cur_dxf.insert("italic".into(), json!(true)); }
                    b"color" if section == 5 && in_dxf_font => {
                        let tint = attr(&e, b"tint").and_then(|v| v.parse().ok()).unwrap_or(0.0);
                        let theme = attr(&e, b"theme").and_then(|v| v.parse().ok());
                        if let Some(c) = resolve_color(attr(&e, b"rgb").as_deref(), theme, tint) { cur_dxf.insert("color".into(), json!(c)); }
                    }
                    b"bgColor" if section == 5 => { // dxf solid fill colour
                        let tint = attr(&e, b"tint").and_then(|v| v.parse().ok()).unwrap_or(0.0);
                        let theme = attr(&e, b"theme").and_then(|v| v.parse().ok());
                        if let Some(c) = resolve_color(attr(&e, b"rgb").as_deref(), theme, tint) { cur_dxf.insert("bg".into(), json!(c)); }
                    }
                    b"numFmt" => {
                        if let (Some(id), Some(code)) = (attr(&e, b"numFmtId"), attr(&e, b"formatCode")) {
                            if let Ok(id) = id.parse::<u32>() { st.num_fmts.insert(id, code); }
                        }
                    }
                    b"font" if section == 1 => cur_font = Font::default(),
                    b"b" if section == 1 => cur_font.bold = true,
                    b"i" if section == 1 => cur_font.italic = true,
                    b"u" if section == 1 => cur_font.underline = true,
                    b"strike" if section == 1 => cur_font.strike = true,
                    b"sz" if section == 1 => cur_font.size = attr(&e, b"val").and_then(|v| v.parse().ok()),
                    b"name" if section == 1 => cur_font.name = attr(&e, b"val"),
                    b"color" if section == 1 => {
                        let tint = attr(&e, b"tint").and_then(|v| v.parse().ok()).unwrap_or(0.0);
                        let theme = attr(&e, b"theme").and_then(|v| v.parse().ok());
                        cur_font.color = resolve_color(attr(&e, b"rgb").as_deref(), theme, tint);
                    }
                    b"fill" if section == 2 => { cur_fill = None; in_pattern_solid = false; }
                    b"patternFill" if section == 2 => {
                        in_pattern_solid = attr(&e, b"patternType").as_deref() == Some("solid");
                    }
                    b"fgColor" if section == 2 && in_pattern_solid => {
                        let tint = attr(&e, b"tint").and_then(|v| v.parse().ok()).unwrap_or(0.0);
                        let theme = attr(&e, b"theme").and_then(|v| v.parse().ok());
                        cur_fill = resolve_color(attr(&e, b"rgb").as_deref(), theme, tint);
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
                        let tint = attr(&e, b"tint").and_then(|v| v.parse().ok()).unwrap_or(0.0);
                        let theme = attr(&e, b"theme").and_then(|v| v.parse().ok());
                        let c = resolve_color(attr(&e, b"rgb").as_deref(), theme, tint).unwrap_or_else(|| "#000000".into());
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
                b"fill" if section == 2 => st.fills.push(cur_fill.take()),
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
    st
}

// Build the CellData style object for a given cell-format (xf) index.
fn style_for(styles: &Styles, xf_idx: usize) -> Option<Value> {
    let xf = styles.xfs.get(xf_idx)?;
    let mut s = Map::new();
    if let Some(f) = styles.fonts.get(xf.font) {
        if f.bold { s.insert("bold".into(), json!(true)); }
        if f.italic { s.insert("italic".into(), json!(true)); }
        if f.underline { s.insert("underline".into(), json!(true)); }
        if f.strike { s.insert("strike".into(), json!(true)); }
        if let Some(sz) = f.size { s.insert("fontSize".into(), json!((sz * 4.0 / 3.0).round())); } // pt → px
        if let Some(c) = &f.color { if c != "#000000" { s.insert("color".into(), json!(c)); } }
        if let Some(n) = &f.name { s.insert("fontFamily".into(), json!(n)); }
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
        s.insert("valign".into(), json!(xf.valign.clone().unwrap()));
    }
    // Number format code (builtin or custom) → applied at display time (dates, "00", …).
    if let Some(code) = numfmt_code(xf.num_fmt, &styles.num_fmts) {
        s.insert("numFmtCode".into(), json!(code));
    }
    if s.is_empty() { None } else { Some(Value::Object(s)) }
}

// Resolve an xlsx number-format id to its format code (builtin table or custom map).
// Returns None for "General" (id 0) and unknown ids.
fn numfmt_code(id: u32, custom: &HashMap<u32, String>) -> Option<String> {
    if id >= 164 { return custom.get(&id).cloned(); }
    let c = match id {
        1 => "0", 2 => "0.00", 3 => "#,##0", 4 => "#,##0.00",
        9 => "0%", 10 => "0.00%", 11 => "0.00E+00", 12 => "# ?/?", 13 => "# ??/??",
        14 => "dd/mm/yyyy", 15 => "d-mmm-yy", 16 => "d-mmm", 17 => "mmm-yy",
        18 => "h:mm", 19 => "h:mm:ss", 20 => "h:mm", 21 => "h:mm:ss",
        22 => "dd/mm/yyyy h:mm", 37 => "#,##0;(#,##0)", 38 => "#,##0;[Red](#,##0)",
        39 => "#,##0.00;(#,##0.00)", 40 => "#,##0.00;[Red](#,##0.00)",
        45 => "mm:ss", 46 => "[h]:mm:ss", 47 => "mm:ss.0", 48 => "##0.0E+0", 49 => "@",
        _ => return None,
    };
    Some(c.to_string())
}

// ── Worksheet ─────────────────────────────────────────────────────────────────

fn parse_worksheet(xml: &str, shared: &[String], styles: &Styles) -> XlsxSheet {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut sheet = XlsxSheet::default();
    sheet.show_gridlines = true; // default; overridden by <sheetView showGridLines="0">

    let mut cur_ref = String::new();
    let mut cur_type = String::new();
    let mut cur_xf: Option<usize> = None;
    let mut cur_formula: Option<String> = None;
    let mut cur_value = String::new();
    let mut in_v = false;
    let mut in_f = false;
    let mut in_is_t = false;
    let mut cur_row_ht: Option<(i32, f64)> = None;
    // Shared formulas: the master cell (with text + si) defines the formula; the
    // followers carry only si and reuse the master, translated to their position.
    let mut shared_masters: HashMap<u32, (String, usize, i32)> = HashMap::new();
    let mut cur_f_si: Option<u32> = None;
    // Conditional formatting state.
    let mut cf_ranges: Vec<String> = Vec::new();
    let mut cf_rules: Vec<Value> = Vec::new();
    let mut cur_rule: Option<(String, String, u32, bool)> = None; // (type, operator, dxfId, stop)
    let mut cur_rule_formulas: Vec<String> = Vec::new();
    let mut in_cf_formula = false;
    let mut cf_formula_buf = String::new();

    loop {
        match reader.read_event() {
            // A self-closing styled cell `<c r=".." s=".."/>` carries no value/formula
            // and gets no End event — emit its style now (e.g. an empty blue header cell).
            Ok(Event::Empty(e)) if e.local_name().as_ref() == b"c" => {
                let r = attr(&e, b"r").unwrap_or_default();
                let xf = attr(&e, b"s").and_then(|v| v.parse::<usize>().ok());
                if let (true, Some(xf)) = (split_ref(&r).is_some(), xf) {
                    if let Some(s) = style_for(styles, xf) {
                        let mut obj = Map::new();
                        obj.insert("s".into(), s);
                        sheet.cells.insert(r, Value::Object(obj));
                    }
                }
            }
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let ln = e.local_name();
                match ln.as_ref() {
                    b"col" => {
                        let min = attr(&e, b"min").and_then(|v| v.parse::<usize>().ok());
                        let max = attr(&e, b"max").and_then(|v| v.parse::<usize>().ok());
                        let w = attr(&e, b"width").and_then(|v| v.parse::<f64>().ok());
                        let hidden = attr(&e, b"hidden").as_deref() == Some("1");
                        if let (Some(min), Some(max)) = (min, max) {
                            // A hidden column collapses to width 0; otherwise use its width.
                            // Cap the range so a trailing "min..16384" default block doesn't
                            // write 16k entries.
                            let hi = if max > 256 { min } else { max };
                            let px = if hidden { Some(0.0) } else { w.map(|w| (w * 7.0 + 5.0).round()) };
                            if let Some(px) = px {
                                for c in min..=hi { sheet.col_widths.insert(idx_to_col(c - 1), px); }
                            }
                        }
                    }
                    b"row" => {
                        let r = attr(&e, b"r").and_then(|v| v.parse::<i32>().ok());
                        let ht = attr(&e, b"ht").and_then(|v| v.parse::<f64>().ok());
                        let hidden = attr(&e, b"hidden").as_deref() == Some("1");
                        cur_row_ht = match r {
                            Some(r) if hidden => Some((r, 0.0)),                       // hidden row → height 0
                            Some(r) => ht.map(|h| (r, (h * 4.0 / 3.0).round())),
                            None => None,
                        };
                    }
                    b"c" => {
                        cur_ref = attr(&e, b"r").unwrap_or_default();
                        cur_type = attr(&e, b"t").unwrap_or_default();
                        cur_xf = attr(&e, b"s").and_then(|v| v.parse().ok());
                        cur_formula = None;
                        cur_value.clear();
                        in_f = false; in_v = false; cur_f_si = None;
                    }
                    b"f" => { in_f = true; cur_formula = Some(String::new()); cur_f_si = attr(&e, b"si").and_then(|v| v.parse().ok()); }
                    b"v" => { in_v = true; in_f = false; }
                    b"t" if cur_type == "inlineStr" => in_is_t = true,
                    b"mergeCell" => { if let Some(r) = attr(&e, b"ref") { sheet.merges.push(r); } }
                    b"sheetView" => { if attr(&e, b"showGridLines").as_deref() == Some("0") { sheet.show_gridlines = false; } }
                    b"sheetFormatPr" => { sheet.default_row_height = attr(&e, b"defaultRowHeight").and_then(|v| v.parse::<f64>().ok()).map(|h| (h * 4.0 / 3.0).round()); }
                    b"conditionalFormatting" => {
                        cf_ranges = attr(&e, b"sqref").unwrap_or_default().split_whitespace().map(|s| s.to_string()).collect();
                        cf_rules = Vec::new();
                    }
                    b"cfRule" => {
                        cur_rule = Some((
                            attr(&e, b"type").unwrap_or_default(),
                            attr(&e, b"operator").unwrap_or_default(),
                            attr(&e, b"dxfId").and_then(|v| v.parse().ok()).unwrap_or(0),
                            attr(&e, b"stopIfTrue").as_deref() == Some("1"),
                        ));
                        cur_rule_formulas = Vec::new();
                    }
                    b"formula" if cur_rule.is_some() => { in_cf_formula = true; cf_formula_buf.clear(); }
                    _ => {}
                }
            }
            Ok(Event::Text(e)) => {
                let txt = e.unescape().unwrap_or_default();
                if in_cf_formula { cf_formula_buf.push_str(&txt); }
                else if in_v { cur_value.push_str(&txt); }
                else if in_f { if let Some(f) = cur_formula.as_mut() { f.push_str(&txt); } }
                else if in_is_t { cur_value.push_str(&txt); }
            }
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"v" => in_v = false,
                b"f" => in_f = false,
                b"t" if in_is_t => in_is_t = false,
                b"formula" if in_cf_formula => { in_cf_formula = false; cur_rule_formulas.push(cf_formula_buf.clone()); }
                b"cfRule" => {
                    if let Some((ty, op, dxf_id, stop)) = cur_rule.take() {
                        let dxf = styles.dxfs.get(dxf_id as usize).cloned().unwrap_or_else(|| json!({}));
                        cf_rules.push(json!({ "type": ty, "op": op, "formulas": cur_rule_formulas, "dxf": dxf, "stop": stop }));
                    }
                }
                b"conditionalFormatting" => {
                    if !cf_ranges.is_empty() && !cf_rules.is_empty() {
                        sheet.cond_formats.push(json!({ "ranges": cf_ranges.clone(), "rules": cf_rules.clone() }));
                    }
                }
                b"row" => {
                    if let Some((r, px)) = cur_row_ht.take() { sheet.row_heights.insert(r, px); }
                }
                b"c" => {
                    let mut obj = Map::new();
                    // Resolve the formula text, expanding shared formulas.
                    let (cur_col, cur_row) = split_ref(&cur_ref).map(|(c, r)| (col_to_idx(&c), r)).unwrap_or((0, 0));
                    let formula: Option<String> = match (&cur_formula, cur_f_si) {
                        (Some(f), Some(si)) if !f.is_empty() => { // shared master (or plain with si)
                            shared_masters.insert(si, (f.clone(), cur_col, cur_row));
                            Some(f.clone())
                        }
                        (Some(f), Some(si)) => { // shared follower → translate the master
                            shared_masters.get(&si).map(|(mf, mc, mr)| translate_formula(mf, cur_col as i64 - *mc as i64, cur_row as i64 - *mr as i64))
                                .filter(|f| !f.is_empty()).or_else(|| if f.is_empty() { None } else { Some(f.clone()) })
                        }
                        (Some(f), None) if !f.is_empty() => Some(f.clone()),
                        _ => None,
                    };
                    if let Some(f) = formula {
                        obj.insert("f".into(), json!(format!("={f}")));
                    }
                    let has_val = !cur_value.is_empty();
                    if has_val {
                        match cur_type.as_str() {
                            "s" => { // shared string index
                                if let Ok(i) = cur_value.parse::<usize>() {
                                    obj.insert("v".into(), json!(shared.get(i).cloned().unwrap_or_default()));
                                }
                            }
                            "b" => { obj.insert("v".into(), json!(cur_value == "1")); }
                            "str" | "inlineStr" | "e" => { obj.insert("v".into(), json!(cur_value.clone())); }
                            _ => {
                                if let Ok(n) = cur_value.parse::<f64>() { obj.insert("v".into(), json!(n)); }
                                else { obj.insert("v".into(), json!(cur_value.clone())); }
                            }
                        }
                    }
                    if let Some(xf) = cur_xf { if let Some(s) = style_for(styles, xf) { obj.insert("s".into(), s); } }
                    if !obj.is_empty() {
                        if let Some((_, _)) = split_ref(&cur_ref) { sheet.cells.insert(cur_ref.clone(), Value::Object(obj)); }
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    sheet
}

// ── Workbook + relationships ──────────────────────────────────────────────────

fn parse_workbook(xml: &str) -> (Vec<(String, String)>, Vec<(String, String)>) {
    // → (sheets: [(name, r:id)], defined_names: [(name, formula)])
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut sheets = Vec::new();
    let mut names = Vec::new();
    let mut cur_name: Option<String> = None;
    let mut cur_def = String::new();
    let mut in_def = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.local_name().as_ref() {
                b"sheet" => {
                    let name = attr(&e, b"name").unwrap_or_default();
                    let rid = e.attributes().flatten()
                        .find(|a| a.key.as_ref().ends_with(b":id") || a.key.local_name().as_ref() == b"id")
                        .map(|a| String::from_utf8_lossy(&a.value).into_owned())
                        .unwrap_or_default();
                    sheets.push((name, rid));
                }
                b"definedName" => {
                    let n = attr(&e, b"name").unwrap_or_default();
                    // Skip Excel built-ins (Print_Area, Print_Titles, …).
                    cur_name = if n.starts_with("_xlnm") { None } else { Some(n) };
                    cur_def.clear();
                    in_def = true;
                }
                _ => {}
            },
            Ok(Event::Text(e)) if in_def => cur_def.push_str(&e.unescape().unwrap_or_default()),
            Ok(Event::End(e)) if e.local_name().as_ref() == b"definedName" => {
                if let Some(n) = cur_name.take() {
                    if !cur_def.trim().is_empty() { names.push((n, format!("={}", cur_def.trim()))); }
                }
                in_def = false;
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    (sheets, names)
}

fn parse_rels(xml: &str) -> HashMap<String, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut map = HashMap::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if e.local_name().as_ref() == b"Relationship" => {
                if let (Some(id), Some(target)) = (attr(&e, b"Id"), attr(&e, b"Target")) { map.insert(id, target); }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    map
}

// ── Entry point ─────────────────────────────────────────────────────────────

// Resolve a worksheet's embedded pictures: worksheet → drawing part (via the sheet's
// rels) → media files (via the drawing's rels) → base64 data URLs, keeping each anchor.
fn extract_sheet_images(archive: &mut zip::ZipArchive<Cursor<&[u8]>>, sheet_path: &str) -> Vec<Value> {
    use base64::Engine;
    let mut images = Vec::new();
    // xl/worksheets/sheet1.xml → xl/worksheets/_rels/sheet1.xml.rels
    let (dir, file) = match sheet_path.rsplit_once('/') { Some(v) => v, None => return images };
    let sheet_rels = format!("{dir}/_rels/{file}.rels");
    let rels = match read_zip_text(archive, &sheet_rels) { Some(x) => parse_rels(&x), None => return images };
    // Each drawing relationship → a drawing part.
    let drawing_targets: Vec<String> = rels.values().filter(|t| t.contains("drawings/")).cloned().collect();
    for dt in drawing_targets {
        let drawing_path = resolve_path(sheet_path, &dt);
        let Some(dxml) = read_zip_text(archive, &drawing_path) else { continue };
        let anchors = parse_drawing(&dxml);
        if anchors.is_empty() { continue }
        // Drawing's own rels map embed ids → media files.
        let (ddir, dfile) = match drawing_path.rsplit_once('/') { Some(v) => v, None => continue };
        let drels = read_zip_text(archive, &format!("{ddir}/_rels/{dfile}.rels"))
            .map(|x| parse_rels(&x)).unwrap_or_default();
        for (mut anchor, embed) in anchors {
            let Some(media_target) = drels.get(&embed) else { continue };
            let media_path = resolve_path(&drawing_path, media_target);
            let Some(bytes) = read_zip_bytes(archive, &media_path) else { continue };
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            anchor.insert("src".into(), json!(format!("data:{};base64,{}", image_mime(&media_path), b64)));
            images.push(Value::Object(anchor));
        }
    }
    images
}

pub fn import_xlsx(bytes: &[u8]) -> Result<XlsxWorkbook> {
    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)?;

    let shared = read_zip_text(&mut archive, "xl/sharedStrings.xml")
        .map(|x| parse_shared_strings(&x)).unwrap_or_default();
    let styles = read_zip_text(&mut archive, "xl/styles.xml")
        .map(|x| parse_styles(&x)).unwrap_or_default();
    let wb_xml = read_zip_text(&mut archive, "xl/workbook.xml")
        .ok_or_else(|| anyhow::anyhow!("xl/workbook.xml manquant"))?;
    let (sheet_refs, defined_names) = parse_workbook(&wb_xml);
    let rels = read_zip_text(&mut archive, "xl/_rels/workbook.xml.rels")
        .map(|x| parse_rels(&x)).unwrap_or_default();

    let mut wb = XlsxWorkbook { sheets: Vec::new(), defined_names };
    for (name, rid) in sheet_refs {
        let target = rels.get(&rid).cloned().unwrap_or_default();
        let path = if target.starts_with("/xl/") { target.trim_start_matches('/').to_string() }
                   else if target.starts_with("xl/") { target }
                   else { format!("xl/{}", target.trim_start_matches("./")) };
        if let Some(xml) = read_zip_text(&mut archive, &path) {
            let mut sheet = parse_worksheet(&xml, &shared, &styles);
            sheet.name = name;
            sheet.images = extract_sheet_images(&mut archive, &path);
            wb.sheets.push(sheet);
        }
    }
    if wb.sheets.is_empty() { anyhow::bail!("aucune feuille lisible dans le .xlsx"); }
    Ok(wb)
}

