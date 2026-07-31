//! Styles part builder (xl/styles.xml): deduplicates fonts / fills / borders /
//! number formats coming from the internal `CellStyle` JSON objects into
//! `cellXfs` entries.
//!
//! Extension hooks for later agents: `register_dxf` (differential formats for
//! conditional formatting) can be added alongside `xf_for` — the final XML is
//! assembled in one place (`build_xml`) in the schema-mandated section order.
use std::collections::HashMap;

use serde_json::Value;

use super::super::util::builtin_numfmt_id;

/// Default font of the internal renderer: Arial at 13 px (= 9.75 pt).
const DEFAULT_FONT_NAME: &str = "Arial";
const DEFAULT_FONT_SIZE_PT: f64 = 9.75;

/// First id available for custom number formats (0–163 are reserved builtins).
const FIRST_CUSTOM_NUMFMT: u32 = 164;

#[derive(Default)]
pub struct StyleRegistry {
    fonts:   Vec<String>,           // <font> fragments
    fills:   Vec<String>,           // <fill> fragments
    borders: Vec<String>,           // <border> fragments
    num_fmts: Vec<(u32, String)>,   // custom formats (id ≥ 164, escaped code)
    xfs:     Vec<String>,           // <xf/> fragments of cellXfs
    dxfs:    Vec<String>,           // <dxf> fragments (conditional formatting)
    font_ix:   HashMap<String, usize>,
    fill_ix:   HashMap<String, usize>,
    border_ix: HashMap<String, usize>,
    numfmt_ix: HashMap<String, u32>,
    xf_ix:     HashMap<String, u32>,
    dxf_ix:    HashMap<String, u32>,
}

// "#RRGGBB" → "FFRRGGBB" (ARGB, alpha opaque). Returns None for invalid input.
fn argb(hex: &str) -> Option<String> {
    let h = hex.strip_prefix('#')?;
    if h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(format!("FF{}", h.to_uppercase()))
    } else { None }
}

// Trim a float to a compact decimal representation ("9.75", "11").
fn fmt_f64(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 { format!("{}", v.round() as i64) } else { format!("{v}") }
}

// Escape text placed inside an XML attribute value.
fn esc_attr(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

// Internal border edge width (px) → xlsx line style.
fn edge_style(width: u64) -> &'static str {
    match width { 2 => "medium", w if w >= 3 => "thick", _ => "thin" }
}

// Build a `<fill><gradientFill …>` fragment from the internal `bgGradient`
// object ({ type: 'linear'|'radial', angle, stops: [{color, position, opacity}] },
// stop positions 0..1). CSS angles are 0=up/90=right; xlsx `degree` is
// 0=left→right/90=top→bottom, hence the -90° shift. Radial gradients map to
// the xlsx "path" type centred in the cell. Returns None when no stop has a
// usable colour.
fn gradient_fill_xml(g: &serde_json::Map<String, Value>) -> Option<String> {
    let stops = g.get("stops")?.as_array()?;
    let mut parsed: Vec<(f64, String)> = stops.iter().filter_map(|st| {
        let o = st.as_object()?;
        let pos = o.get("position").and_then(|v| v.as_f64()).unwrap_or(0.0).clamp(0.0, 1.0);
        let c = o.get("color").and_then(|v| v.as_str()).and_then(argb)?;
        Some((pos, c))
    }).collect();
    if parsed.is_empty() { return None; }
    parsed.sort_by(|a, b| a.0.total_cmp(&b.0));
    let attrs = if g.get("type").and_then(|v| v.as_str()) == Some("radial") {
        " type=\"path\" left=\"0.5\" right=\"0.5\" top=\"0.5\" bottom=\"0.5\"".to_string()
    } else {
        let angle = g.get("angle").and_then(|v| v.as_f64()).unwrap_or(90.0);
        let deg = ((angle - 90.0) % 360.0 + 360.0) % 360.0;
        format!(" degree=\"{}\"", fmt_f64(deg))
    };
    let mut out = format!("<fill><gradientFill{attrs}>");
    for (pos, c) in &parsed {
        out.push_str(&format!("<stop position=\"{}\"><color rgb=\"{c}\"/></stop>", fmt_f64(*pos)));
    }
    out.push_str("</gradientFill></fill>");
    Some(out)
}

impl StyleRegistry {
    pub fn new() -> Self {
        let mut reg = Self::default();
        // Reserved slots: font 0 = default font; fills 0/1 = none/gray125
        // (mandated by consumers); border 0 = empty; xf 0 = default style.
        reg.intern_font(&format!(
            "<font><sz val=\"{}\"/><name val=\"{DEFAULT_FONT_NAME}\"/></font>",
            fmt_f64(DEFAULT_FONT_SIZE_PT)
        ));
        reg.intern_fill("<fill><patternFill patternType=\"none\"/></fill>");
        reg.intern_fill("<fill><patternFill patternType=\"gray125\"/></fill>");
        reg.intern_border("<border><left/><right/><top/><bottom/><diagonal/></border>");
        let xf0 = "<xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/>".to_string();
        reg.xf_ix.insert(xf0.clone(), 0);
        reg.xfs.push(xf0);
        reg
    }

    fn intern_font(&mut self, frag: &str) -> usize {
        if let Some(&i) = self.font_ix.get(frag) { return i; }
        let i = self.fonts.len();
        self.fonts.push(frag.to_string());
        self.font_ix.insert(frag.to_string(), i);
        i
    }
    fn intern_fill(&mut self, frag: &str) -> usize {
        if let Some(&i) = self.fill_ix.get(frag) { return i; }
        let i = self.fills.len();
        self.fills.push(frag.to_string());
        self.fill_ix.insert(frag.to_string(), i);
        i
    }
    fn intern_border(&mut self, frag: &str) -> usize {
        if let Some(&i) = self.border_ix.get(frag) { return i; }
        let i = self.borders.len();
        self.borders.push(frag.to_string());
        self.border_ix.insert(frag.to_string(), i);
        i
    }
    fn intern_numfmt(&mut self, code: &str) -> u32 {
        if let Some(id) = builtin_numfmt_id(code) { return id; }
        if let Some(&id) = self.numfmt_ix.get(code) { return id; }
        let id = FIRST_CUSTOM_NUMFMT + self.num_fmts.len() as u32;
        self.num_fmts.push((id, esc_attr(code)));
        self.numfmt_ix.insert(code.to_string(), id);
        id
    }

    /// Register the style object of a cell (internal `CellStyle` JSON, already
    /// merged column < row < cell) and return the `cellXfs` index to reference
    /// from `<c s="…">`. Returns 0 (the default xf) for empty/absent styles.
    pub fn xf_for(&mut self, style: &Value) -> u32 {
        let Some(s) = style.as_object() else { return 0 };
        if s.is_empty() { return 0 }

        let get_bool = |k: &str| s.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
        let get_str  = |k: &str| s.get(k).and_then(|v| v.as_str());

        // ── Font ──
        let mut font = String::from("<font>");
        let mut font_significant = false;
        if get_bool("bold")      { font.push_str("<b/>"); font_significant = true; }
        if get_bool("italic")    { font.push_str("<i/>"); font_significant = true; }
        if get_bool("underline") { font.push_str("<u/>"); font_significant = true; }
        if get_bool("strike")    { font.push_str("<strike/>"); font_significant = true; }
        let size_pt = s.get("fontSize").and_then(|v| v.as_f64())
            .map(|px| px * 3.0 / 4.0) // px → pt
            .unwrap_or(DEFAULT_FONT_SIZE_PT);
        font.push_str(&format!("<sz val=\"{}\"/>", fmt_f64(size_pt)));
        if (size_pt - DEFAULT_FONT_SIZE_PT).abs() > 1e-9 { font_significant = true; }
        if let Some(c) = get_str("color").and_then(argb) {
            font.push_str(&format!("<color rgb=\"{c}\"/>"));
            font_significant = true;
        }
        let name = get_str("fontFamily").unwrap_or(DEFAULT_FONT_NAME);
        font.push_str(&format!("<name val=\"{}\"/>", esc_attr(name)));
        if name != DEFAULT_FONT_NAME { font_significant = true; }
        font.push_str("</font>");
        let font_id = if font_significant { self.intern_font(&font) } else { 0 };

        // ── Fill: `bgGradient` (takes precedence at render time) exports as a
        // real xlsx gradientFill; otherwise a solid patternFill from `bg`. ──
        let gradient = s.get("bgGradient").and_then(|v| v.as_object()).and_then(gradient_fill_xml);
        let fill_id = match gradient {
            Some(frag) => self.intern_fill(&frag),
            None => match get_str("bg").and_then(argb) {
                Some(c) => self.intern_fill(&format!(
                    "<fill><patternFill patternType=\"solid\"><fgColor rgb=\"{c}\"/></patternFill></fill>"
                )),
                None => 0,
            },
        };

        // ── Border (per-edge colour bt/br/bb/bl + width btw/brw/bbw/blw px) ──
        let edge = |ck: &str, wk: &str| -> String {
            match get_str(ck).and_then(argb) {
                Some(c) => {
                    let w = s.get(wk).and_then(|v| v.as_u64()).unwrap_or(1);
                    format!("style=\"{}\"><color rgb=\"{c}\"/>", edge_style(w))
                }
                None => "/>".into(),
            }
        };
        // CT_Border child order: left, right, top, bottom, diagonal.
        let mk = |tag: &str, e: String| if e == "/>" { format!("<{tag}/>") } else { format!("<{tag} {e}</{tag}>") };
        let border = format!(
            "<border>{}{}{}{}<diagonal/></border>",
            mk("left",   edge("bl", "blw")),
            mk("right",  edge("br", "brw")),
            mk("top",    edge("bt", "btw")),
            mk("bottom", edge("bb", "bbw")),
        );
        let border_id = if border.contains("style=") { self.intern_border(&border) } else { 0 };

        // ── Number format ──
        let num_fmt_id = if let Some(code) = get_str("numFmtCode") {
            if code.eq_ignore_ascii_case("general") { 0 } else { self.intern_numfmt(code) }
        } else {
            match self.synth_ui_numfmt(s) { Some(code) => self.intern_numfmt(&code), None => 0 }
        };

        // ── Alignment ──
        let mut align = String::new();
        if let Some(h) = get_str("align") {
            if matches!(h, "left" | "center" | "right") { align.push_str(&format!(" horizontal=\"{h}\"")); }
        }
        if let Some(v) = get_str("valign") {
            // Internal "center" maps to the OOXML vertical value "center" as well.
            if matches!(v, "top" | "center" | "bottom") { align.push_str(&format!(" vertical=\"{v}\"")); }
        }
        if get_bool("wrap") { align.push_str(" wrapText=\"1\""); }

        // ── Compose the xf ──
        let mut xf = format!("<xf numFmtId=\"{num_fmt_id}\" fontId=\"{font_id}\" fillId=\"{fill_id}\" borderId=\"{border_id}\" xfId=\"0\"");
        if num_fmt_id != 0 { xf.push_str(" applyNumberFormat=\"1\""); }
        if font_id != 0 { xf.push_str(" applyFont=\"1\""); }
        if fill_id > 1 { xf.push_str(" applyFill=\"1\""); }
        if border_id != 0 { xf.push_str(" applyBorder=\"1\""); }
        if !align.is_empty() {
            xf.push_str(&format!(" applyAlignment=\"1\"><alignment{align}/></xf>"));
        } else {
            xf.push_str("/>");
        }

        if let Some(&i) = self.xf_ix.get(&xf) { return i; }
        let i = self.xfs.len() as u32;
        self.xf_ix.insert(xf.clone(), i);
        self.xfs.push(xf);
        i
    }

    /// Register a differential format (conditional-formatting dxf, internal
    /// shape `{ bg?, color?, bold?, italic? }`) and return its `dxfId`.
    /// Returns None when the object carries nothing representable.
    pub fn dxf_for(&mut self, dxf: &Value) -> Option<u32> {
        let s = dxf.as_object()?;
        let get_bool = |k: &str| s.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
        // CT_Dxf child order: font, numFmt, fill, alignment, border, protection.
        let mut font = String::new();
        if get_bool("bold")   { font.push_str("<b/>"); }
        if get_bool("italic") { font.push_str("<i/>"); }
        if let Some(c) = s.get("color").and_then(|v| v.as_str()).and_then(argb) {
            font.push_str(&format!("<color rgb=\"{c}\"/>"));
        }
        let mut frag = String::from("<dxf>");
        if !font.is_empty() { frag.push_str(&format!("<font>{font}</font>")); }
        if let Some(c) = s.get("bg").and_then(|v| v.as_str()).and_then(argb) {
            // In a dxf solid fill the VISIBLE colour is bgColor (unlike cellXfs fills).
            frag.push_str(&format!("<fill><patternFill><bgColor rgb=\"{c}\"/></patternFill></fill>"));
        }
        frag.push_str("</dxf>");
        if frag == "<dxf></dxf>" { return None; }
        if let Some(&i) = self.dxf_ix.get(&frag) { return Some(i); }
        let i = self.dxfs.len() as u32;
        self.dxf_ix.insert(frag.clone(), i);
        self.dxfs.push(frag);
        Some(i)
    }

    // Synthesize a format code from the UI-level format fields
    // (`numFmt`/`decimals`/`thousands`), mirroring the frontend `formatNumber`.
    fn synth_ui_numfmt(&self, s: &serde_json::Map<String, Value>) -> Option<String> {
        let kind = s.get("numFmt").and_then(|v| v.as_str());
        let decimals = s.get("decimals").and_then(|v| v.as_u64());
        let thousands = s.get("thousands").and_then(|v| v.as_bool()).unwrap_or(false);
        if kind.is_none() && decimals.is_none() && !thousands { return None; }
        let dec = |d: u64| if d == 0 { String::new() } else { format!(".{}", "0".repeat(d as usize)) };
        Some(match kind {
            Some("percent")    => format!("0{}%", dec(decimals.unwrap_or(0))),
            Some("scientific") => format!("0{}E+00", dec(decimals.unwrap_or(2))),
            Some("currency")   => format!("#,##0{} \"€\"", dec(decimals.unwrap_or(2))),
            // "number" or bare decimals/thousands.
            _ => format!("{}{}", if thousands { "#,##0" } else { "0" }, dec(decimals.unwrap_or(2))),
        })
    }

    /// Serialize the styles part. Section order is mandated by the schema:
    /// numFmts?, fonts, fills, borders, cellStyleXfs, cellXfs, cellStyles, dxfs?.
    pub fn build_xml(&self) -> String {
        let mut out = String::with_capacity(1024);
        out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
        out.push_str("<styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">");
        if !self.num_fmts.is_empty() {
            out.push_str(&format!("<numFmts count=\"{}\">", self.num_fmts.len()));
            for (id, code) in &self.num_fmts {
                out.push_str(&format!("<numFmt numFmtId=\"{id}\" formatCode=\"{code}\"/>"));
            }
            out.push_str("</numFmts>");
        }
        out.push_str(&format!("<fonts count=\"{}\">", self.fonts.len()));
        for f in &self.fonts { out.push_str(f); }
        out.push_str("</fonts>");
        out.push_str(&format!("<fills count=\"{}\">", self.fills.len()));
        for f in &self.fills { out.push_str(f); }
        out.push_str("</fills>");
        out.push_str(&format!("<borders count=\"{}\">", self.borders.len()));
        for b in &self.borders { out.push_str(b); }
        out.push_str("</borders>");
        out.push_str("<cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs>");
        out.push_str(&format!("<cellXfs count=\"{}\">", self.xfs.len()));
        for x in &self.xfs { out.push_str(x); }
        out.push_str("</cellXfs>");
        out.push_str("<cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles>");
        if !self.dxfs.is_empty() {
            out.push_str(&format!("<dxfs count=\"{}\">", self.dxfs.len()));
            for d in &self.dxfs { out.push_str(d); }
            out.push_str("</dxfs>");
        }
        out.push_str("</styleSheet>");
        out
    }
}
