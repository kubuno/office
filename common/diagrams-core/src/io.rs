//! Diagram interchange: draw.io / mxGraph XML (compressed or not) both ways, and CSV import — a port of
//! `office/web/src/diagramIo.ts` (and of `inflate.ts`, whose raw DEFLATE decoder is `flate2` here).
//!
//! The web parses the XML with regular expressions, not with a DOM, and that shapes what it accepts
//! (the first `>` closes a tag even inside an attribute value, `<Array as="points">` is looked for in
//! the text that follows an `<mxCell` up to the next `<mxCell`, …). The import below scans the text the
//! same way, so a file opens on the desktop exactly as it opens on the web. The export writes the very
//! same text as `toDrawioXml`.
//!
//! The data is the web's `IoData`: shapes and connectors as their JSON objects ([`Shape`], [`Connector`]),
//! built with the same keys as the web's object literals.

use std::io::Read;

use kubuno_office_shapes_core::path::{js_num, js_round};
use serde_json::{Map, Value};

use crate::model::{js_number, Connector, IdSource, Obj, Shape};

/// `IoData`: what an import produces and an export reads.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct IoData {
    pub shapes: Vec<Shape>,
    pub connectors: Vec<Connector>,
}

// ── JavaScript value semantics the web code relies on ─────────────────────────

/// `String(v)` / a template literal's `${v}`.
fn js_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => js_num(n.as_f64().unwrap_or(f64::NAN)),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".into(),
        Value::Array(a) => a.iter().map(|e| if e.is_null() { String::new() } else { js_str(e) }).collect::<Vec<_>>().join(","),
        Value::Object(_) => "[object Object]".into(),
    }
}

/// JavaScript truthiness of a (possibly absent) value.
fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

/// `Number(s)` for a string: trimmed, empty is 0, anything that is not a number is NaN.
fn js_to_number(s: &str) -> f64 {
    let t = s.trim();
    if t.is_empty() {
        return 0.0;
    }
    match t {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return u64::from_str_radix(h, 16).map(|v| v as f64).unwrap_or(f64::NAN);
    }
    // Rust also accepts `inf`, `nan`, `infinity`: JavaScript does not.
    if t.chars().any(|c| c.is_ascii_alphabetic() && c != 'e' && c != 'E') {
        return f64::NAN;
    }
    t.parse::<f64>().unwrap_or(f64::NAN)
}

/// `Math.round(v)` printed like JavaScript.
fn round_str(v: Option<&Value>) -> String {
    let n = v.and_then(Value::as_f64).unwrap_or(f64::NAN);
    js_num(js_round(n))
}

/// `+v.toFixed(5)`.
fn to_fixed5(v: f64) -> f64 {
    if !v.is_finite() {
        return v;
    }
    format!("{v:.5}").parse::<f64>().unwrap_or(v)
}

/// `enc`: XML-escapes `& < > "`.
fn enc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// `dec`: the reverse, `&amp;` last (so `&amp;lt;` reads `&lt;`, as on the web).
fn dec(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&amp;", "&")
}

/// `stripHtml`: decodes, turns `<br>` into new lines, drops every other tag, trims.
fn strip_html(s: &str) -> String {
    let d = dec(s);
    let mut out = String::with_capacity(d.len());
    let mut rest = d.as_str();
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        // `/<br\s*\/?>/gi` first, then `/<[^>]+>/g`.
        if let Some(end) = br_tag_len(tail) {
            out.push('\n');
            rest = &tail[end..];
        } else if let Some(close) = tail[1..].find('>').filter(|&p| p > 0) {
            rest = &tail[close + 2..];
        } else {
            out.push('<');
            rest = &tail[1..];
        }
    }
    out.push_str(rest);
    out.trim().to_string()
}

/// The length of a `<br>`, `<br/>`, `<BR />` at the start of `s`.
fn br_tag_len(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    if b.len() < 4 || !b[1].eq_ignore_ascii_case(&b'b') || !b[2].eq_ignore_ascii_case(&b'r') {
        return None;
    }
    let mut i = 3;
    while i < b.len() && (b[i] as char).is_whitespace() {
        i += 1;
    }
    if i < b.len() && b[i] == b'/' {
        i += 1;
    }
    (i < b.len() && b[i] == b'>').then_some(i + 1)
}

// ── Export ────────────────────────────────────────────────────────────────────

/// `TYPE_TO_MX`: our shape type → an mxGraph style prefix.
fn type_to_mx(t: &str) -> Option<&'static str> {
    Some(match t {
        "rect" | "flow_process" | "er_entity" | "uml_object" | "ui_input" => "",
        "rounded_rect" => "rounded=1;",
        "flow_terminator" | "flow_start" => "rounded=1;arcSize=40;",
        "bpmn_task" | "ui_button" => "rounded=1;",
        "ellipse" | "flow_connector" | "uml_usecase" | "bpmn_start" | "bpmn_end" | "bpmn_event" | "er_attribute" => "ellipse;",
        "diamond" | "flow_decision" | "bpmn_gateway" | "er_relationship" => "rhombus;",
        "triangle" => "triangle;",
        "parallelogram" | "flow_data" => "shape=parallelogram;",
        "hexagon" | "flow_prep" => "shape=hexagon;",
        "cylinder" | "flow_db" | "net_database" => "shape=cylinder;",
        "cloud" | "net_cloud" | "net_internet" => "ellipse;shape=cloud;",
        "flow_document" => "shape=document;",
        "shp_cube" | "uml_node" => "shape=cube;",
        "shp_step" => "shape=step;",
        "shp_card" | "flow_card" => "shape=card;",
        "shp_note" | "uml_note" => "shape=note;",
        "text" => "text;html=1;",
        "container" | "swimlane_v" => "swimlane;",
        "swimlane_h" => "swimlane;horizontal=0;",
        "shp_pentagon" => "shape=pentagon;",
        "shp_octagon" => "shape=octagon;",
        _ => return None,
    })
}

fn obj_of<'a>(o: &'a Obj, key: &str) -> Option<&'a Map<String, Value>> {
    o.get(key).and_then(Value::as_object)
}

/// `shapeStyle(s)`.
fn shape_style(s: &Shape) -> String {
    let o = s.obj();
    let kind = o.get("type").map(js_str).unwrap_or_else(|| "undefined".into());
    let mut st = type_to_mx(&kind).unwrap_or("").to_string();
    let empty = Map::new();
    let sy = obj_of(o, "style").unwrap_or(&empty);
    let is = |k: &str, v: &str| sy.get(k).and_then(Value::as_str) == Some(v);
    if truthy(sy.get("fillColor")) && !is("fillColor", "none") {
        st += &format!("fillColor={};", sy.get("fillColor").map(js_str).unwrap_or_default());
    } else if is("fillColor", "none") {
        st += "fillColor=none;";
    }
    if truthy(sy.get("strokeColor")) && !is("strokeColor", "none") {
        st += &format!("strokeColor={};", sy.get("strokeColor").map(js_str).unwrap_or_default());
    }
    if is("strokeStyle", "dashed") {
        st += "dashed=1;";
    } else if is("strokeStyle", "dotted") {
        st += "dashed=1;dashPattern=1 4;";
    }
    if let Some(w) = sy.get("strokeWidth").and_then(Value::as_f64) {
        st += &format!("strokeWidth={};", js_num(w));
    }
    if let Some(op) = sy.get("opacity").and_then(Value::as_f64) {
        if op < 100.0 {
            st += &format!("opacity={};", js_num(op));
        }
    }
    if truthy(sy.get("shadow")) {
        st += "shadow=1;";
    }
    if let Some(adj) = o.get("adj").and_then(Value::as_array).filter(|a| !a.is_empty()) {
        let parts: Vec<String> = adj.iter().map(|v| js_num(to_fixed5(v.as_f64().unwrap_or(f64::NAN)))).collect();
        st += &format!("kbAdj={};", parts.join(" "));
    }
    st += "whiteSpace=wrap;html=1;";
    st
}

/// `edgeStyle(c)`. QUIRK: it reads `style.routing` only, so a legacy connector that says
/// `orthogonal: true` without a `routing` is exported as a straight edge.
fn edge_style(c: &Connector) -> String {
    let empty = Map::new();
    let sy = obj_of(c.obj(), "style").unwrap_or(&empty);
    let routing = sy.get("routing").and_then(Value::as_str);
    let is = |k: &str, v: &str| sy.get(k).and_then(Value::as_str) == Some(v);
    let mut st = format!("edgeStyle={};", if routing == Some("orthogonal") { "orthogonalEdgeStyle" } else { "none" });
    if routing == Some("curved") {
        st += "curved=1;";
    }
    if truthy(sy.get("strokeColor")) {
        st += &format!("strokeColor={};", sy.get("strokeColor").map(js_str).unwrap_or_default());
    }
    if let Some(w) = sy.get("strokeWidth").and_then(Value::as_f64) {
        st += &format!("strokeWidth={};", js_num(w));
    }
    if is("strokeStyle", "dashed") {
        st += "dashed=1;";
    } else if is("strokeStyle", "dotted") {
        st += "dashed=1;dashPattern=1 4;";
    }
    let arrow = |k: &str| if truthy(sy.get(k)) && !is(k, "none") { "classic" } else { "none" };
    st += &format!("startArrow={};", arrow("arrowStart"));
    st += &format!("endArrow={};", arrow("arrowEnd"));
    st += "html=1;rounded=0;";
    st
}

/// `s.label || ''`.
fn label_or_empty(o: &Obj) -> String {
    if truthy(o.get("label")) {
        o.get("label").map(js_str).unwrap_or_default()
    } else {
        String::new()
    }
}

/// `toDrawioXml(data, title)`: uncompressed mxGraph XML, openable in draw.io.
pub fn to_drawio_xml(data: &IoData, title: &str) -> String {
    let mut cells: Vec<String> = Vec::new();
    for s in &data.shapes {
        let o = s.obj();
        let id = o.get("id").map(js_str).unwrap_or_else(|| "undefined".into());
        cells.push(format!(
            "        <mxCell id=\"{}\" value=\"{}\" style=\"{}\" vertex=\"1\" parent=\"1\">\n          <mxGeometry x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" as=\"geometry\" />\n        </mxCell>",
            enc(&id),
            enc(&label_or_empty(o)),
            enc(&shape_style(s)),
            round_str(o.get("x")),
            round_str(o.get("y")),
            round_str(o.get("w")),
            round_str(o.get("h")),
        ));
    }
    for c in &data.connectors {
        let o = c.obj();
        let pts: Vec<String> = o
            .get("waypoints")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|p| format!("            <mxPoint x=\"{}\" y=\"{}\" />", round_str(p.get("x")), round_str(p.get("y"))))
                    .collect()
            })
            .unwrap_or_default();
        let geom = if pts.is_empty() {
            "          <mxGeometry relative=\"1\" as=\"geometry\" />".to_string()
        } else {
            format!(
                "          <mxGeometry relative=\"1\" as=\"geometry\">\n            <Array as=\"points\">\n{}\n            </Array>\n          </mxGeometry>",
                pts.join("\n")
            )
        };
        let id = o.get("id").map(js_str).unwrap_or_else(|| "undefined".into());
        let mut cell = format!("        <mxCell id=\"{}\" value=\"{}\" style=\"{}\" edge=\"1\" parent=\"1\"", enc(&id), enc(&label_or_empty(o)), enc(&edge_style(c)));
        if truthy(o.get("sourceId")) {
            cell += &format!(" source=\"{}\"", enc(&o.get("sourceId").map(js_str).unwrap_or_default()));
        }
        if truthy(o.get("targetId")) {
            cell += &format!(" target=\"{}\"", enc(&o.get("targetId").map(js_str).unwrap_or_default()));
        }
        cell += &format!(">\n{geom}\n        </mxCell>");
        cells.push(cell);
    }
    format!(
        "<mxfile host=\"kubuno\">\n  <diagram name=\"{}\">\n    <mxGraphModel dx=\"800\" dy=\"600\" grid=\"1\" gridSize=\"10\" guides=\"1\" tooltips=\"1\" connect=\"1\" arrows=\"1\" fold=\"1\" page=\"1\" pageScale=\"1\" math=\"0\" shadow=\"0\">\n      <root>\n        <mxCell id=\"0\" />\n        <mxCell id=\"1\" parent=\"0\" />\n{}\n      </root>\n    </mxGraphModel>\n  </diagram>\n</mxfile>\n",
        enc(title),
        cells.join("\n")
    )
}

// ── Import ────────────────────────────────────────────────────────────────────

/// `parseStyle`: `a=1;b;c=x` → a map (a bare token is `'1'`; a later key wins).
fn parse_style(style: &str) -> Map<String, Value> {
    let mut out = Map::new();
    for tok in style.split(';') {
        if tok.is_empty() {
            continue;
        }
        match tok.find('=') {
            None => {
                out.insert(tok.trim().to_string(), Value::from("1"));
            }
            Some(eq) => {
                out.insert(tok[..eq].trim().to_string(), Value::from(tok[eq + 1..].trim()));
            }
        }
    }
    out
}

fn sget<'a>(st: &'a Map<String, Value>, k: &str) -> Option<&'a str> {
    st.get(k).and_then(Value::as_str)
}

/// A style token is set to a non-empty value (JavaScript truthiness of a string).
fn sset(st: &Map<String, Value>, k: &str) -> bool {
    sget(st, k).is_some_and(|v| !v.is_empty())
}

/// `mxToType`.
fn mx_to_type(st: &Map<String, Value>) -> &'static str {
    let shape = sget(st, "shape");
    if sset(st, "text") || shape == Some("text") {
        return "text";
    }
    if sset(st, "ellipse") || shape == Some("cloud") {
        return if shape == Some("cloud") { "cloud" } else { "ellipse" };
    }
    if sset(st, "rhombus") {
        return "diamond";
    }
    if sset(st, "triangle") {
        return "triangle";
    }
    match shape {
        Some("parallelogram") => return "parallelogram",
        Some("hexagon") => return "hexagon",
        Some("cylinder") => return "cylinder",
        Some("cube") => return "shp_cube",
        Some("step") => return "shp_step",
        Some("card") => return "shp_card",
        Some("note") => return "shp_note",
        Some("document") => return "flow_document",
        Some("process") => return "flow_predefined",
        Some("pentagon") => return "shp_pentagon",
        Some("octagon") => return "shp_octagon",
        _ => {}
    }
    if st.contains_key("swimlane") || shape == Some("swimlane") {
        return if sget(st, "horizontal") == Some("0") { "swimlane_h" } else { "swimlane_v" };
    }
    if sget(st, "rounded") == Some("1") {
        return "rounded_rect";
    }
    "rect"
}

/// `attr(tag, name)`: the value of the first `<whitespace>name="…"` in the tag (raw, not decoded).
fn attr(tag: &str, name: &str) -> Option<String> {
    let pat = format!("{name}=\"");
    let mut from = 0;
    while let Some(i) = tag[from..].find(&pat) {
        let at = from + i;
        let preceded = tag[..at].chars().next_back().is_some_and(char::is_whitespace);
        let vstart = at + pat.len();
        if preceded {
            if let Some(end) = tag[vstart..].find('"') {
                return Some(tag[vstart..vstart + end].to_string());
            }
        }
        from = at + 1;
    }
    None
}

/// `new RegExp(`${n}="([\-\d.]+)"`)` on the geometry's attribute text: the first match (no word
/// boundary before the name, as on the web).
fn geom_num(gt: &str, name: &str) -> Option<f64> {
    let pat = format!("{name}=\"");
    let mut from = 0;
    while let Some(i) = gt[from..].find(&pat) {
        let vstart = from + i + pat.len();
        let rest = &gt[vstart..];
        let len = rest.find(|c: char| !(c == '-' || c == '.' || c.is_ascii_digit())).unwrap_or(rest.len());
        if len > 0 && rest[len..].starts_with('"') {
            return Some(js_to_number(&rest[..len]));
        }
        from = from + i + 1;
    }
    None
}

/// `/<mxCell\b/`: is there an `<mxCell` that is a whole word at `i`?
fn mxcell_positions(s: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = s[from..].find("<mxCell") {
        let at = from + i;
        let next = s[at + 7..].chars().next();
        if !next.is_some_and(|c| c.is_alphanumeric() || c == '_') {
            out.push(at);
        }
        from = at + 7;
    }
    out
}

/// The waypoints of an edge block: `<mxPoint x="…" y="…"/>` inside the first `<Array … as="points" …>`.
fn waypoints_of(raw: &str) -> Vec<Value> {
    let mut pts = Vec::new();
    // `/<Array[^>]*as="points"[^>]*>([\s\S]*?)<\/Array>/`.
    let mut from = 0;
    let inner = loop {
        let Some(i) = raw[from..].find("<Array") else { break None };
        let at = from + i;
        let Some(gt) = raw[at..].find('>') else { break None };
        let open = &raw[at..at + gt];
        if open.contains("as=\"points\"") {
            let body_start = at + gt + 1;
            break raw[body_start..].find("</Array>").map(|e| &raw[body_start..body_start + e]);
        }
        from = at + 1;
    };
    let Some(inner) = inner else { return pts };
    // `/<mxPoint\s+x="([\-\d.]+)"\s+y="([\-\d.]+)"\s*\/>/g`.
    let mut rest = inner;
    while let Some(i) = rest.find("<mxPoint") {
        let after = &rest[i + 8..];
        rest = after;
        let ws = |s: &str| s.len() - s.trim_start().len();
        let n1 = ws(after);
        if n1 == 0 {
            continue;
        }
        let a = &after[n1..];
        let Some(xs) = a.strip_prefix("x=\"") else { continue };
        let xl = xs.find(|c: char| !(c == '-' || c == '.' || c.is_ascii_digit())).unwrap_or(xs.len());
        if xl == 0 || !xs[xl..].starts_with('"') {
            continue;
        }
        let b = &xs[xl + 1..];
        let n2 = ws(b);
        if n2 == 0 {
            continue;
        }
        let Some(ys) = b[n2..].strip_prefix("y=\"") else { continue };
        let yl = ys.find(|c: char| !(c == '-' || c == '.' || c.is_ascii_digit())).unwrap_or(ys.len());
        if yl == 0 || !ys[yl..].starts_with('"') {
            continue;
        }
        let c = &ys[yl + 1..];
        let c = c.trim_start();
        if !c.starts_with("/>") {
            continue;
        }
        let mut o = Obj::new();
        o.insert("x".into(), js_number(js_to_number(&xs[..xl])));
        o.insert("y".into(), js_number(js_to_number(&ys[..yl])));
        pts.push(Value::Object(o));
        rest = &c[2..];
    }
    pts
}

/// `atob` (whitespace already removed by the caller): standard base64, padding optional.
fn atob(s: &str) -> Option<Vec<u8>> {
    let body = s.trim_end_matches('=');
    if s.len() - body.len() > 2 || body.len() % 4 == 1 {
        return None;
    }
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        })
    };
    let mut out = Vec::with_capacity(body.len() * 3 / 4);
    let (mut acc, mut nbits) = (0u32, 0u32);
    for &c in body.as_bytes() {
        acc = (acc << 6) | val(c)?;
        nbits += 6;
        if nbits >= 8 {
            nbits -= 8;
            out.push((acc >> nbits) as u8);
            acc &= (1 << nbits) - 1;
        }
    }
    Some(out)
}

/// `decodeURIComponent`: `None` where JavaScript throws (a malformed escape or invalid UTF-8).
fn decode_uri_component(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let h = |c: u8| (c as char).to_digit(16);
            let hi = b.get(i + 1).copied().and_then(h)?;
            let lo = b.get(i + 2).copied().and_then(h)?;
            out.push((hi * 16 + lo) as u8);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// `inflateDiagram`: a compressed `<diagram>` payload (base64 → raw DEFLATE → URI-encoded XML).
fn inflate_diagram(xml: &str) -> Option<String> {
    // `/<diagram[^>]*>([\s\S]*?)<\/diagram>/`.
    let at = xml.find("<diagram")?;
    let gt = at + xml[at..].find('>')?;
    let body_start = gt + 1;
    let end = xml[body_start..].find("</diagram>")?;
    let payload = xml[body_start..body_start + end].trim();
    if payload.is_empty() || payload.contains('<') {
        return None;
    }
    let clean: String = payload.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = atob(&clean)?;
    let mut raw = Vec::new();
    flate2::read::DeflateDecoder::new(bytes.as_slice()).read_to_end(&mut raw).ok()?;
    let inflated = String::from_utf8_lossy(&raw).into_owned();
    let mut decoded = inflated.clone();
    if let Some(u) = decode_uri_component(&inflated) {
        if !mxcell_positions(&u).is_empty() {
            decoded = u;
        }
    }
    (!mxcell_positions(&decoded).is_empty()).then_some(decoded)
}

/// `fromDrawioXml(xml)`: `None` when the text holds no diagram (`null` on the web).
pub fn from_drawio_xml(xml: &str) -> Option<IoData> {
    let starts = mxcell_positions(xml);
    if starts.is_empty() {
        let inflated = inflate_diagram(xml)?;
        return from_drawio_xml(&inflated);
    }
    let mut data = IoData::default();
    let mut z = 0.0;
    for (k, &start) in starts.iter().enumerate() {
        let end = starts.get(k + 1).copied().unwrap_or(xml.len());
        // `'<mxCell ' + raw`, raw being what follows the `<mxCell` word.
        let raw = format!("<mxCell {}", &xml[start + 7..end]);
        let open_tag = match raw.find('>') {
            Some(i) => &raw[..=i],
            None => "",
        };
        let Some(id) = attr(open_tag, "id") else { continue };
        if id.is_empty() || id == "0" || id == "1" {
            continue;
        }
        let is_edge = attr(open_tag, "edge").as_deref() == Some("1");
        let is_vertex = attr(open_tag, "vertex").as_deref() == Some("1");
        let st = parse_style(&dec(&attr(open_tag, "style").unwrap_or_default()));
        let value = strip_html(&attr(open_tag, "value").unwrap_or_default());
        if is_edge {
            let pts = waypoints_of(&raw);
            let mut style = Obj::new();
            style.insert("strokeColor".into(), sget(&st, "strokeColor").filter(|s| !s.is_empty()).unwrap_or("#6c8ebf").into());
            let sw = if sset(&st, "strokeWidth") { js_to_number(sget(&st, "strokeWidth").unwrap_or("")) } else { 1.5 };
            style.insert("strokeWidth".into(), js_number(sw));
            let stroke_style = if sget(&st, "dashed") == Some("1") {
                if sset(&st, "dashPattern") { "dotted" } else { "dashed" }
            } else {
                "solid"
            };
            style.insert("strokeStyle".into(), stroke_style.into());
            let start_arrow = if sset(&st, "startArrow") && sget(&st, "startArrow") != Some("none") { "block" } else { "none" };
            style.insert("arrowStart".into(), start_arrow.into());
            let end_arrow = if !sset(&st, "endArrow") || sget(&st, "endArrow") != Some("none") { "block" } else { "none" };
            style.insert("arrowEnd".into(), end_arrow.into());
            let routing = if sget(&st, "curved") == Some("1") {
                "curved"
            } else if sset(&st, "edgeStyle") && sget(&st, "edgeStyle") != Some("none") {
                "orthogonal"
            } else {
                "straight"
            };
            style.insert("routing".into(), routing.into());
            let mut o = Obj::new();
            o.insert("id".into(), id.into());
            o.insert("sourceId".into(), attr(open_tag, "source").map(Value::from).unwrap_or(Value::Null));
            o.insert("targetId".into(), attr(open_tag, "target").map(Value::from).unwrap_or(Value::Null));
            o.insert("sourcePoint".into(), Value::Null);
            o.insert("targetPoint".into(), Value::Null);
            o.insert("waypoints".into(), Value::Array(pts));
            o.insert("label".into(), value.into());
            o.insert("style".into(), Value::Object(style));
            data.connectors.push(Connector(o));
        } else if is_vertex {
            // `raw.match(/<mxGeometry\b([^>]*)/)`.
            let gt = raw
                .match_indices("<mxGeometry")
                .find(|(i, _)| !raw[i + 11..].chars().next().is_some_and(|c| c.is_alphanumeric() || c == '_'))
                .map(|(i, _)| {
                    let rest = &raw[i + 11..];
                    &rest[..rest.find('>').unwrap_or(rest.len())]
                })
                .unwrap_or("");
            let mut style = Obj::new();
            if sset(&st, "fillColor") {
                style.insert("fillColor".into(), sget(&st, "fillColor").unwrap_or("").into());
            }
            if sset(&st, "strokeColor") {
                style.insert("strokeColor".into(), sget(&st, "strokeColor").unwrap_or("").into());
            }
            if sget(&st, "dashed") == Some("1") {
                style.insert("strokeStyle".into(), if sset(&st, "dashPattern") { "dotted" } else { "dashed" }.into());
            }
            if sset(&st, "strokeWidth") {
                style.insert("strokeWidth".into(), js_number(js_to_number(sget(&st, "strokeWidth").unwrap_or(""))));
            }
            if sset(&st, "opacity") {
                style.insert("opacity".into(), js_number(js_to_number(sget(&st, "opacity").unwrap_or(""))));
            }
            if sget(&st, "shadow") == Some("1") {
                style.insert("shadow".into(), true.into());
            }
            let kind = mx_to_type(&st);
            if sget(&st, "rounded") == Some("1") && kind == "rounded_rect" {
                style.insert("rounded".into(), js_number(12.0));
            }
            // Private token written by our own export (see `shape_style`).
            let adj: Vec<f64> = match sget(&st, "kbAdj").filter(|s| !s.is_empty()) {
                Some(s) => s.split(|c: char| c.is_whitespace() || c == ',').map(js_to_number).filter(|v| v.is_finite()).collect(),
                None => Vec::new(),
            };
            // QUIRK: `split(/[\s,]+/)` yields empty pieces only at the ends, which `Number('')` turns
            // into 0; splitting on every separator yields more empty pieces in the middle of a run of
            // separators. The web's own export writes single spaces, so the two agree on its files.
            let mut o = Obj::new();
            o.insert("id".into(), id.into());
            o.insert("type".into(), kind.into());
            o.insert("x".into(), js_number(geom_num(gt, "x").unwrap_or(0.0)));
            o.insert("y".into(), js_number(geom_num(gt, "y").unwrap_or(0.0)));
            o.insert("w".into(), js_number(geom_num(gt, "width").unwrap_or(120.0)));
            o.insert("h".into(), js_number(geom_num(gt, "height").unwrap_or(60.0)));
            o.insert("label".into(), value.into());
            o.insert("style".into(), Value::Object(style));
            o.insert("labelStyle".into(), Value::Object(Obj::new()));
            o.insert("zIndex".into(), js_number(z));
            z += 1.0;
            if !adj.is_empty() {
                o.insert("adj".into(), Value::Array(adj.iter().map(|v| js_number(*v)).collect()));
            }
            data.shapes.push(Shape(o));
        }
    }
    Some(data)
}

/// `fromCsv(text)`: one rounded node per row (its first column), on a square-ish grid. The web's ids
/// end with five random base-36 characters; they come from `ids` here.
pub fn from_csv(text: &str, ids: &mut IdSource) -> IoData {
    let lines: Vec<&str> = text
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l).trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    let cols = ((lines.len() as f64).sqrt().ceil() as usize).max(1);
    let mut data = IoData::default();
    for (i, line) in lines.iter().enumerate() {
        let first = line.split(',').next().unwrap_or("");
        let cell = if first.is_empty() { line } else { first };
        let t = cell.trim();
        // `.replace(/^"|"$/g, '')`: one quote off each end.
        let t = t.strip_prefix('"').unwrap_or(t);
        let t = t.strip_suffix('"').unwrap_or(t);
        let mut style = Obj::new();
        style.insert("fillColor".into(), "#dae8fc".into());
        style.insert("strokeColor".into(), "#6c8ebf".into());
        style.insert("rounded".into(), js_number(12.0));
        let mut o = Obj::new();
        let suffix: String = ids.make_id().chars().take(5).collect();
        o.insert("id".into(), format!("csv{i}_{suffix}").into());
        o.insert("type".into(), "rounded_rect".into());
        o.insert("x".into(), js_number(80.0 + (i % cols) as f64 * 180.0));
        o.insert("y".into(), js_number(80.0 + (i / cols) as f64 * 110.0));
        o.insert("w".into(), js_number(150.0));
        o.insert("h".into(), js_number(60.0));
        o.insert("label".into(), t.into());
        o.insert("style".into(), Value::Object(style));
        o.insert("labelStyle".into(), Value::Object(Obj::new()));
        o.insert("zIndex".into(), js_number(i as f64));
        data.shapes.push(Shape(o));
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    fn shape(v: Value) -> Shape {
        Shape(v.as_object().cloned().expect("object"))
    }

    fn conn(v: Value) -> Connector {
        Connector(v.as_object().cloned().expect("object"))
    }

    fn sample() -> IoData {
        IoData {
            shapes: vec![
                shape(json!({ "id": "a", "type": "rect", "x": 10.4, "y": 20.5, "w": 120, "h": 60, "label": "A & <B>",
                    "style": { "fillColor": "#dae8fc", "strokeColor": "#6c8ebf" }, "labelStyle": {}, "zIndex": 0 })),
                shape(json!({ "id": "b", "type": "flow_decision", "x": 200, "y": 20, "w": 80, "h": 60, "label": "",
                    "style": { "fillColor": "none", "strokeStyle": "dotted", "strokeWidth": 2, "opacity": 50, "shadow": true },
                    "labelStyle": {}, "zIndex": 1, "adj": [16667.123456, 0.5] })),
            ],
            connectors: vec![conn(json!({ "id": "c", "sourceId": "a", "targetId": "b", "sourcePoint": null, "targetPoint": null,
                "waypoints": [{ "x": 150.5, "y": 50 }], "label": "oui",
                "style": { "strokeColor": "#6c8ebf", "strokeWidth": 1.5, "strokeStyle": "solid", "arrowStart": "none", "arrowEnd": "block", "routing": "orthogonal" } }))],
        }
    }

    #[test]
    fn the_export_writes_the_webs_text() {
        let xml = to_drawio_xml(&sample(), "Mon \"diagramme\"");
        let expected = "<mxfile host=\"kubuno\">\n  <diagram name=\"Mon &quot;diagramme&quot;\">\n    <mxGraphModel dx=\"800\" dy=\"600\" grid=\"1\" gridSize=\"10\" guides=\"1\" tooltips=\"1\" connect=\"1\" arrows=\"1\" fold=\"1\" page=\"1\" pageScale=\"1\" math=\"0\" shadow=\"0\">\n      <root>\n        <mxCell id=\"0\" />\n        <mxCell id=\"1\" parent=\"0\" />\n\
        \x20\x20\x20\x20\x20\x20\x20\x20<mxCell id=\"a\" value=\"A &amp; &lt;B&gt;\" style=\"fillColor=#dae8fc;strokeColor=#6c8ebf;whiteSpace=wrap;html=1;\" vertex=\"1\" parent=\"1\">\n          <mxGeometry x=\"10\" y=\"21\" width=\"120\" height=\"60\" as=\"geometry\" />\n        </mxCell>\n\
        \x20\x20\x20\x20\x20\x20\x20\x20<mxCell id=\"b\" value=\"\" style=\"rhombus;fillColor=none;dashed=1;dashPattern=1 4;strokeWidth=2;opacity=50;shadow=1;kbAdj=16667.12346 0.5;whiteSpace=wrap;html=1;\" vertex=\"1\" parent=\"1\">\n          <mxGeometry x=\"200\" y=\"20\" width=\"80\" height=\"60\" as=\"geometry\" />\n        </mxCell>\n\
        \x20\x20\x20\x20\x20\x20\x20\x20<mxCell id=\"c\" value=\"oui\" style=\"edgeStyle=orthogonalEdgeStyle;strokeColor=#6c8ebf;strokeWidth=1.5;startArrow=none;endArrow=classic;html=1;rounded=0;\" edge=\"1\" parent=\"1\" source=\"a\" target=\"b\">\n          <mxGeometry relative=\"1\" as=\"geometry\">\n            <Array as=\"points\">\n            <mxPoint x=\"151\" y=\"50\" />\n            </Array>\n          </mxGeometry>\n        </mxCell>\n      </root>\n    </mxGraphModel>\n  </diagram>\n</mxfile>\n";
        assert_eq!(xml, expected);
    }

    #[test]
    fn export_then_import_keeps_the_diagram() {
        let io = from_drawio_xml(&to_drawio_xml(&sample(), "t")).expect("imports");
        assert_eq!(io.shapes.len(), 2);
        let a = io.shapes[0].obj();
        assert_eq!(a["type"], json!("rect"));
        assert_eq!(a["x"], json!(10));
        // QUIRK: `stripHtml` reads `<B>` as a tag and drops it, as on the web.
        assert_eq!(a["label"], json!("A &"));
        assert_eq!(a["style"], json!({ "fillColor": "#dae8fc", "strokeColor": "#6c8ebf" }));
        let b = io.shapes[1].obj();
        assert_eq!(b["type"], json!("diamond"));
        assert_eq!(b["style"], json!({ "fillColor": "none", "strokeStyle": "dotted", "strokeWidth": 2, "opacity": 50, "shadow": true }));
        assert_eq!(b["adj"], json!([16667.12346, 0.5]));
        assert_eq!(b["zIndex"], json!(1));
        let c = io.connectors[0].obj();
        assert_eq!(c["sourceId"], json!("a"));
        assert_eq!(c["waypoints"], json!([{ "x": 151, "y": 50 }]));
        assert_eq!(c["style"]["routing"], json!("orthogonal"));
        assert_eq!(c["style"]["arrowEnd"], json!("block"));
    }

    const DRAWIO: &str = r##"<mxfile host="app.diagrams.net" modified="2024-01-01T00:00:00.000Z" agent="Mozilla" version="22.1.0" type="device">
  <diagram id="abc" name="Page-1">
    <mxGraphModel dx="1000" dy="600" grid="1" gridSize="10">
      <root>
        <mxCell id="0" />
        <mxCell id="1" parent="0" />
        <mxCell id="n1" value="&lt;b&gt;Start&lt;/b&gt;&lt;br&gt;here" style="rounded=1;whiteSpace=wrap;html=1;fillColor=#d5e8d4;strokeColor=#82b366;" vertex="1" parent="1">
          <mxGeometry x="40" y="40" width="120" height="60" as="geometry" />
        </mxCell>
        <mxCell id="n2" value="Cloud" style="ellipse;shape=cloud;whiteSpace=wrap;html=1;" vertex="1" parent="1">
          <mxGeometry x="240" y="40" width="120" height="80" as="geometry" />
        </mxCell>
        <mxCell id="n3" value="Lane" style="swimlane;horizontal=0;" vertex="1" parent="1">
          <mxGeometry x="0" y="200" width="300" height="100" as="geometry" />
        </mxCell>
        <mxCell id="e1" style="edgeStyle=orthogonalEdgeStyle;rounded=0;orthogonalLoop=1;jettySize=auto;html=1;dashed=1;endArrow=none;" edge="1" parent="1" source="n1" target="n2">
          <mxGeometry relative="1" as="geometry">
            <mxPoint x="10" y="10" as="sourcePoint" />
            <Array as="points">
              <mxPoint x="200" y="70" />
              <mxPoint x="200" y="80.5" />
            </Array>
          </mxGeometry>
        </mxCell>
      </root>
    </mxGraphModel>
  </diagram>
</mxfile>"##;

    #[test]
    fn a_drawio_file_imports() {
        let io = from_drawio_xml(DRAWIO).expect("imports");
        assert_eq!(io.shapes.len(), 3);
        let n1 = io.shapes[0].obj();
        assert_eq!(n1["type"], json!("rounded_rect"));
        assert_eq!(n1["label"], json!("Start\nhere"));
        assert_eq!(n1["style"], json!({ "fillColor": "#d5e8d4", "strokeColor": "#82b366", "rounded": 12 }));
        assert_eq!(io.shapes[1].obj()["type"], json!("cloud"));
        assert_eq!(io.shapes[2].obj()["type"], json!("swimlane_h"));
        let e = io.connectors[0].obj();
        assert_eq!(e["waypoints"], json!([{ "x": 200, "y": 70 }, { "x": 200, "y": 80.5 }]));
        assert_eq!(e["style"]["strokeStyle"], json!("dashed"));
        assert_eq!(e["style"]["arrowEnd"], json!("none"));
        assert_eq!(e["style"]["arrowStart"], json!("none"));
        assert_eq!(e["label"], json!(""));
    }

    fn b64(bytes: &[u8]) -> String {
        const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for ch in bytes.chunks(3) {
            let n = (ch[0] as u32) << 16 | (*ch.get(1).unwrap_or(&0) as u32) << 8 | *ch.get(2).unwrap_or(&0) as u32;
            for k in 0..4 {
                if k <= ch.len() {
                    out.push(A[(n >> (18 - 6 * k) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    fn uri_encode(s: &str) -> String {
        s.bytes()
            .map(|b| if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
            .collect()
    }

    #[test]
    fn a_compressed_drawio_file_imports() {
        let model_start = DRAWIO.find("<mxGraphModel").expect("model");
        let model_end = DRAWIO.find("</mxGraphModel>").expect("end") + "</mxGraphModel>".len();
        let model = &DRAWIO[model_start..model_end];
        let mut z = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(uri_encode(model).as_bytes()).expect("deflate");
        let payload = b64(&z.finish().expect("deflate"));
        let xml = format!("<mxfile host=\"x\"><diagram id=\"d\" name=\"P\">\n  {payload}\n</diagram></mxfile>");
        let io = from_drawio_xml(&xml).expect("imports");
        assert_eq!(io.shapes.len(), 3);
        assert_eq!(io.connectors.len(), 1);
        // Not URI-encoded inside the deflate stream: read as is.
        let mut z = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(model.as_bytes()).expect("deflate");
        let xml = format!("<diagram>{}</diagram>", b64(&z.finish().expect("deflate")));
        assert_eq!(from_drawio_xml(&xml).map(|d| d.shapes.len()), Some(3));
        assert_eq!(from_drawio_xml("<diagram>not base64!</diagram>"), None);
        assert_eq!(from_drawio_xml("hello"), None);
    }

    #[test]
    fn csv_rows_become_a_grid_of_nodes() {
        let mut ids = IdSource::new(1);
        let io = from_csv("# comment\r\n\"Alpha\",x\nBeta\n\n,Gamma\n  Delta  ", &mut ids);
        let labels: Vec<&str> = io.shapes.iter().map(|s| s.label()).collect();
        assert_eq!(labels, ["Alpha", "Beta", ",Gamma", "Delta"]);
        // 4 rows → 2 columns.
        let pos: Vec<(f64, f64)> = io.shapes.iter().map(|s| (s.x(), s.y())).collect();
        assert_eq!(pos, [(80.0, 80.0), (260.0, 80.0), (80.0, 190.0), (260.0, 190.0)]);
        let id = io.shapes[0].id();
        assert!(id.starts_with("csv0_") && id.len() == 10);
        assert_eq!(io.shapes[3].obj()["zIndex"], json!(3));
        assert!(from_csv("\n# x\n", &mut ids).shapes.is_empty());
    }

    #[test]
    fn helpers_behave_like_javascript() {
        assert_eq!(js_to_number(""), 0.0);
        assert!(js_to_number("1.2.3").is_nan());
        assert!(js_to_number("inf").is_nan());
        assert_eq!(js_to_number(" 0x10 "), 16.0);
        assert_eq!(strip_html("a<BR />b <i>c</i> &amp;lt;"), "a\nb c &lt;");
        assert_eq!(decode_uri_component("%3Cx%3E"), Some("<x>".into()));
        assert_eq!(decode_uri_component("%E0%A4%A"), None);
        assert_eq!(atob("aGk="), Some(b"hi".to_vec()));
        assert_eq!(atob("a"), None);
        assert_eq!(attr("<mxCell  id=\"2\" parent=\"1\">", "id"), Some("2".into()));
        assert_eq!(attr("<mxCell parentid=\"2\">", "id"), None);
    }
}
