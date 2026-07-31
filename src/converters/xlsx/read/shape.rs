//! Drawing shape (`<xdr:sp>`) parsing: preset geometry, fill, outline and caption.
//!
//! Shapes are self-contained (no relationship to a media or chart part), so the
//! whole payload is read straight out of the drawing part. Only the subset the
//! internal `SheetShape` model carries is extracted; everything else (custom
//! geometry, gradients, 3-D effects, per-run rich text) is intentionally dropped.
use quick_xml::events::Event;
use quick_xml::Reader;
use serde_json::{json, Map, Value};

use super::super::util::attr;

/// OOXML preset geometry (`a:prstGeom@prst`) → internal `SheetShapeKind`.
///
/// The first ten rows are the canonical presets: they map 1:1 with the model's
/// kinds and round-trip verbatim. The rest are aliases — presets whose
/// silhouette *and* orientation match one of ours closely enough that keeping
/// the family beats the generic fallback. Anything not listed becomes `rect`,
/// which preserves fill, outline and caption and loses only the silhouette.
const PRESET_KINDS: &[(&str, &str)] = &[
    // ── canonical (exported back verbatim) ──
    ("rect", "rect"),
    ("roundRect", "roundRect"),
    ("ellipse", "ellipse"),
    ("triangle", "triangle"),
    ("diamond", "diamond"),
    ("rightArrow", "arrow"),
    ("line", "line"),
    ("star5", "star"),
    ("wedgeRectCallout", "callout"),
    ("mathPlus", "plus"),
    // The old approximation aliases are gone: every other preset now maps
    // EXACTLY through the generated LibreOffice table below.
];

/// Map a preset geometry name to the internal shape kind (`rect` when unknown).
/// The GENERATED LibreOffice table covers the 144 catalogue kinds; the small
/// legacy list keeps priority so the sheet's native kinds win on their presets.
pub fn kind_from_preset(prst: &str) -> &'static str {
    if let Some(k) = PRESET_KINDS.iter().find(|(p, _)| *p == prst).map(|(_, k)| *k) {
        return k;
    }
    crate::converters::xlsx::shape_presets::KIND_PRESETS
        .iter()
        .find(|(_, p, _)| *p == prst)
        .map(|(k, _, _)| *k)
        .unwrap_or("rect")
}

// Convert an OOXML line width (EMU) into pixels, rounded to a tenth.
fn emu_line_to_px(w: f64) -> f64 { ((w / 12_700.0) * 10.0).round() / 10.0 }

fn has(stack: &[Vec<u8>], name: &[u8]) -> bool { stack.iter().any(|e| e == name) }

// Mutable state of one shape parse.
#[derive(Default)]
struct ShapeState {
    prst: Option<String>,
    adj_named: Vec<(String, f64)>,
    kind:         Option<&'static str>,
    fill:         Option<String>,
    border:       Option<String>,
    border_width: Option<f64>,
    bold:         bool,
    italic:       bool,
    size:         Option<f64>,
    color:        Option<String>,
    align:        Option<&'static str>,
    run_seen:     bool,      // first run's rPr wins for the shape-wide text style
    paras:        Vec<String>,
    cur_para:     String,
}

// Handle an opening tag (Start or Empty). `stack` is the ancestor context; the
// element itself is not on it yet.
fn on_open(st: &mut ShapeState, e: &quick_xml::events::BytesStart, stack: &[Vec<u8>]) {
    let name = e.local_name();
    let name = name.as_ref();
    let in_sp_pr = has(stack, b"spPr");
    let in_ln = has(stack, b"ln");
    let in_tx = has(stack, b"txBody");
    match name {
        b"prstGeom" if in_sp_pr => {
            if let Some(p) = attr(e, b"prst") {
                st.kind = Some(kind_from_preset(&p));
                st.prst = Some(p);
            }
        }
        // <a:gd name="adj1" fmla="val 25000"/> — raw adjustment values, matched to
        // the preset's declared names so the order is the preset's, not the file's.
        b"gd" if in_sp_pr => {
            if let (Some(name), Some(f)) = (attr(e, b"name"), attr(e, b"fmla")) {
                if let Some(v) = f.strip_prefix("val ").and_then(|v| v.trim().parse::<f64>().ok()) {
                    st.adj_named.push((name, v));
                }
            }
        }
        b"ln" if in_sp_pr => {
            if let Some(w) = attr(e, b"w").and_then(|v| v.parse::<f64>().ok()) {
                st.border_width = Some(emu_line_to_px(w));
            }
        }
        b"noFill" if in_sp_pr => {
            let slot = if in_ln { &mut st.border } else { &mut st.fill };
            if slot.is_none() { *slot = Some("none".into()); }
        }
        // Only literal RGB is resolvable here: theme colours (a:schemeClr) need
        // the workbook theme, which the drawing part does not carry.
        b"srgbClr" => {
            let Some(v) = attr(e, b"val") else { return };
            let hex = format!("#{}", v.to_uppercase());
            if in_tx {
                if has(stack, b"rPr") && st.color.is_none() { st.color = Some(hex); }
            } else if in_sp_pr {
                let slot = if in_ln { &mut st.border } else { &mut st.fill };
                if slot.is_none() { *slot = Some(hex); }
            }
        }
        // OOXML defaults an unqualified paragraph to left alignment, whereas the
        // model's own default is centred — so the resolved value is always kept
        // and only "center" is elided on the way out.
        b"pPr" if in_tx && st.align.is_none() => {
            st.align = Some(match attr(e, b"algn").as_deref() {
                Some("ctr") => "center",
                Some("r") => "right",
                _ => "left",
            });
        }
        b"rPr" if in_tx && !st.run_seen => {
            st.run_seen = true;
            if attr(e, b"b").as_deref() == Some("1") { st.bold = true; }
            if attr(e, b"i").as_deref() == Some("1") { st.italic = true; }
            // sz is in hundredths of a point.
            if let Some(sz) = attr(e, b"sz").and_then(|v| v.parse::<f64>().ok()) {
                if sz > 0.0 { st.size = Some((sz / 100.0 * 10.0).round() / 10.0); }
            }
        }
        b"br" if in_tx => st.cur_para.push('\n'),
        _ => {}
    }
}

/// Parse the inner XML of one `<xdr:sp>` into the shape fields of the internal
/// model (`kind`, `fill`, `border`, `borderWidth`, `text`, `textStyle`).
/// Geometry, alt text and hyperlink come from the anchor walk in `drawing.rs`.
pub fn parse_shape(inner_xml: &str) -> Map<String, Value> {
    let mut reader = Reader::from_str(inner_xml);
    reader.config_mut().trim_text(false);
    reader.config_mut().check_end_names = false;
    let mut st = ShapeState::default();
    let mut stack: Vec<Vec<u8>> = Vec::new();
    let mut text = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = e.local_name().as_ref().to_vec();
                on_open(&mut st, &e, &stack);
                if name == b"t" { text.clear(); }
                stack.push(name);
            }
            Ok(Event::Empty(e)) => on_open(&mut st, &e, &stack),
            Ok(Event::Text(e)) if stack.last().map(|n| n.as_slice()) == Some(b"t".as_slice()) => {
                text.push_str(&e.unescape().unwrap_or_default());
            }
            Ok(Event::End(e)) => {
                let name = e.local_name().as_ref().to_vec();
                if stack.last() == Some(&name) { stack.pop(); }
                match name.as_slice() {
                    b"t" if has(&stack, b"txBody") => st.cur_para.push_str(&std::mem::take(&mut text)),
                    b"p" if has(&stack, b"txBody") => st.paras.push(std::mem::take(&mut st.cur_para)),
                    _ => {}
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    if !st.cur_para.is_empty() { st.paras.push(std::mem::take(&mut st.cur_para)); }

    let mut out = Map::new();
    out.insert("kind".into(), json!(st.kind.unwrap_or("rect")));
    // Adjustment values, reordered to the preset's declared adjustment names (the
    // generated LibreOffice table): the file may list <a:gd> in any order.
    if !st.adj_named.is_empty() {
        if let Some(prst) = st.prst.as_deref() {
            if let Some((_, _, names)) = crate::converters::xlsx::shape_presets::KIND_PRESETS
                .iter().find(|(_, p, _)| *p == prst)
            {
                let adj: Vec<f64> = names.iter().map(|n| {
                    st.adj_named.iter().find(|(an, _)| an == n).map(|(_, v)| *v)
                        .unwrap_or(f64::NAN)
                }).collect();
                if adj.iter().all(|v| v.is_finite()) && !adj.is_empty() {
                    out.insert("adj".into(), json!(adj));
                }
            }
        }
    }
    if let Some(v) = st.fill { out.insert("fill".into(), json!(v)); }
    if let Some(v) = st.border { out.insert("border".into(), json!(v)); }
    if let Some(v) = st.border_width { out.insert("borderWidth".into(), json!(v)); }
    // Trailing empty paragraphs (an <a:p> holding only endParaRPr) are noise.
    while st.paras.last().is_some_and(|p| p.trim().is_empty()) { st.paras.pop(); }
    let text = st.paras.join("\n");
    if !text.trim().is_empty() {
        out.insert("text".into(), json!(text));
        let mut ts = Map::new();
        if st.bold { ts.insert("bold".into(), json!(true)); }
        if st.italic { ts.insert("italic".into(), json!(true)); }
        if let Some(c) = st.color { ts.insert("color".into(), json!(c)); }
        if let Some(s) = st.size { ts.insert("size".into(), json!(s)); }
        let align = st.align.unwrap_or("left");
        if align != "center" { ts.insert("align".into(), json!(align)); }
        if !ts.is_empty() { out.insert("textStyle".into(), Value::Object(ts)); }
    }
    out
}
