//! New elements, exactly as the web builds them (`PresentationEditorPage.tsx`): placeholders and layouts,
//! text boxes, shapes, lines, images, charts, tables, SmartArt, fields. Keys follow the web's object literals
//! (nested objects included), `undefined` keys are left out, so a new element is stored as the web stores it.

use serde_json::{json, Value};

use crate::model::{num, Element, Obj, Slot, SLIDE_H, SLIDE_W};
use crate::obj::quote;
use crate::smartart::{smartart_layout, SmartArtKind};
use crate::table;

/// Element ids: the web's `uid()` (8 base-36 characters). Platform code seeds it; tests use a sequence.
pub trait IdGen {
    fn next_id(&mut self) -> String;
}

/// A small xorshift generator of `uid()`-like ids.
#[derive(Debug, Clone)]
pub struct Ids(u64);

impl Ids {
    pub fn seeded(seed: u64) -> Ids {
        Ids(seed | 1)
    }
}

impl IdGen for Ids {
    fn next_id(&mut self) -> String {
        let mut out = String::with_capacity(8);
        for _ in 0..8 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            let d = (self.0 % 36) as u32;
            out.push(std::char::from_digit(d, 36).unwrap_or('0'));
        }
        out
    }
}

/// A predictable sequence (`id1`, `id2`…) for tests.
#[derive(Debug, Clone, Default)]
pub struct SeqIds(pub u32);

impl IdGen for SeqIds {
    fn next_id(&mut self) -> String {
        self.0 += 1;
        format!("id{}", self.0)
    }
}

/// The localised placeholder texts (`pres_ph_title`, `pres_ph_subtitle`, `pres_ph_body`).
#[derive(Debug, Clone)]
pub struct Placeholders {
    pub title: String,
    pub subtitle: String,
    pub body: String,
}

impl Default for Placeholders {
    fn default() -> Self {
        Placeholders { title: "Cliquez ici pour ajouter un titre".into(), subtitle: "Cliquez ici pour ajouter un sous-titre".into(), body: "Cliquez ici pour ajouter du texte".into() }
    }
}

/// An ordered element builder: keys in the order they are given (an existing key keeps its place, like a
/// JavaScript object spread).
#[derive(Default)]
struct B(Obj);

impl B {
    fn new() -> B {
        B::default()
    }
    fn v(mut self, k: &str, v: Value) -> B {
        self.0.set(k, v);
        self
    }
    fn s(self, k: &str, v: &str) -> B {
        self.v(k, Value::String(v.to_string()))
    }
    fn n(self, k: &str, v: f64) -> B {
        self.v(k, num(v))
    }
    fn b(self, k: &str, v: bool) -> B {
        self.v(k, Value::Bool(v))
    }
    fn null(self, k: &str) -> B {
        self.v(k, Value::Null)
    }
    /// A nested value as JSON text (its keys stay in the web's order).
    fn raw(mut self, k: &str, json: &str) -> B {
        self.0.set_slot(k, Slot::raw(json));
        self
    }
    fn el(self) -> Element {
        Element(self.0)
    }
}

/// `{type:'doc', content:[{type:'paragraph', content:[{type:'text', text}]}]}` (no text node for an empty text).
pub fn doc_one_para(text: &str) -> String {
    if text.is_empty() {
        r#"{"type":"doc","content":[{"type":"paragraph","content":[]}]}"#.to_string()
    } else {
        format!(r#"{{"type":"doc","content":[{{"type":"paragraph","content":[{{"type":"text","text":{}}}]}}]}}"#, quote(text))
    }
}

fn stroke(color: &str, width: f64, style: &str) -> String {
    format!(r#"{{"color":{},"width":{},"style":{}}}"#, quote(color), num(width), quote(style))
}

fn solid(color: &str) -> String {
    format!(r#"{{"type":"color","color":{}}}"#, quote(color))
}

/// The common head of a text box (`id, type, x, y, w, h, rotation, zIndex, locked, hidden`).
fn head(ids: &mut dyn IdGen, kind: &str, x: f64, y: f64, w: f64, h: f64, z: f64) -> B {
    B::new().s("id", &ids.next_id()).s("type", kind).n("x", x).n("y", y).n("w", w).n("h", h).n("rotation", 0.0).n("zIndex", z).b("locked", false).b("hidden", false)
}

/// `placeholderSlideElements`: the title and subtitle of a new slide.
pub fn placeholder_slide_elements(ids: &mut dyn IdGen, ph: &Placeholders) -> Vec<Element> {
    vec![
        head(ids, "text", 0.08, 0.30, 0.84, 0.28, 1.0)
            .null("content").n("padding", 8.0).s("verticalAlign", "middle").null("background").n("borderRadius", 0.0)
            .s("placeholder", &ph.title).n("fontSize", 44.0).s("align", "center").s("color", "#3c4043")
            .el(),
        head(ids, "text", 0.08, 0.60, 0.84, 0.13, 2.0)
            .null("content").n("padding", 8.0).s("verticalAlign", "middle").null("background").n("borderRadius", 0.0)
            .s("placeholder", &ph.subtitle).n("fontSize", 22.0).s("align", "center")
            .el(),
    ]
}

/// A layout placeholder's own keys, in the web's literal order.
enum L {
    N(&'static str, f64),
    S(&'static str, String),
}

/// `mkText`: `{id, type, rotation, zIndex, locked, hidden, content, padding, verticalAlign, background,
/// borderRadius, placeholder, ...o}`.
fn mk_text(ids: &mut dyn IdGen, o: Vec<L>) -> Element {
    let mut b = B::new().s("id", &ids.next_id()).s("type", "text").n("rotation", 0.0).n("zIndex", 1.0).b("locked", false).b("hidden", false)
        .null("content").n("padding", 8.0).s("verticalAlign", "middle").null("background").n("borderRadius", 0.0).null("placeholder");
    for l in o {
        b = match l {
            L::N(k, v) => b.n(k, v),
            L::S(k, v) => b.s(k, &v),
        };
    }
    b.el()
}

/// The slide layouts (`SLIDE_LAYOUTS`): (id, i18n key).
pub const LAYOUTS: [(&str, &str); 11] = [
    ("title", "pres_lay_title"),
    ("section", "pres_lay_section"),
    ("title_body", "pres_lay_title_body"),
    ("two_col", "pres_lay_two_col"),
    ("title_only", "pres_lay_title_only"),
    ("one_col", "pres_lay_one_col"),
    ("main", "pres_lay_main"),
    ("section_desc", "pres_lay_section_desc"),
    ("caption", "pres_lay_caption"),
    ("number", "pres_lay_number"),
    ("blank", "pres_lay_blank"),
];

/// The elements of layout `id` (they replace the slide's).
pub fn layout_elements(id: &str, ids: &mut dyn IdGen, ph: &Placeholders) -> Vec<Element> {
    use L::{N, S};
    let (t, s, b) = (ph.title.clone(), ph.subtitle.clone(), ph.body.clone());
    let geo = |x: f64, y: f64, w: f64, h: f64| vec![N("x", x), N("y", y), N("w", w), N("h", h)];
    let with = |mut g: Vec<L>, rest: Vec<L>| {
        g.extend(rest);
        g
    };
    let spec: Vec<Vec<L>> = match id {
        "title" => vec![
            with(geo(0.08, 0.34, 0.84, 0.20), vec![S("placeholder", t), N("fontSize", 44.0), S("align", "center".into()), S("color", "#3c4043".into()), N("zIndex", 1.0)]),
            with(geo(0.08, 0.57, 0.84, 0.10), vec![S("placeholder", s), N("fontSize", 20.0), S("align", "center".into()), N("zIndex", 2.0)]),
        ],
        "section" => vec![with(geo(0.1, 0.40, 0.8, 0.18), vec![S("placeholder", t), N("fontSize", 36.0), S("align", "left".into()), S("color", "#3c4043".into())])],
        "title_body" => vec![
            with(geo(0.06, 0.06, 0.88, 0.16), vec![S("placeholder", t), N("fontSize", 30.0), S("align", "left".into()), S("color", "#3c4043".into())]),
            with(geo(0.06, 0.26, 0.88, 0.66), vec![S("placeholder", b), N("fontSize", 18.0), S("align", "left".into()), S("verticalAlign", "top".into()), N("zIndex", 2.0)]),
        ],
        "two_col" => vec![
            with(geo(0.06, 0.06, 0.88, 0.14), vec![S("placeholder", t), N("fontSize", 28.0), S("align", "left".into()), S("color", "#3c4043".into())]),
            with(geo(0.06, 0.24, 0.43, 0.68), vec![S("placeholder", b.clone()), N("fontSize", 16.0), S("align", "left".into()), S("verticalAlign", "top".into()), N("zIndex", 2.0)]),
            with(geo(0.51, 0.24, 0.43, 0.68), vec![S("placeholder", b), N("fontSize", 16.0), S("align", "left".into()), S("verticalAlign", "top".into()), N("zIndex", 3.0)]),
        ],
        "title_only" => vec![with(geo(0.06, 0.06, 0.88, 0.16), vec![S("placeholder", t), N("fontSize", 30.0), S("align", "left".into()), S("color", "#3c4043".into())])],
        "one_col" => vec![with(geo(0.1, 0.1, 0.8, 0.8), vec![S("placeholder", b), N("fontSize", 18.0), S("align", "left".into()), S("verticalAlign", "top".into())])],
        "main" => vec![with(geo(0.08, 0.34, 0.84, 0.30), vec![S("placeholder", t), N("fontSize", 48.0), S("align", "left".into()), S("color", "#3c4043".into())])],
        "section_desc" => vec![
            with(geo(0.05, 0.18, 0.42, 0.18), vec![S("placeholder", t), N("fontSize", 26.0), S("align", "left".into()), S("color", "#3c4043".into())]),
            with(geo(0.05, 0.40, 0.42, 0.42), vec![S("placeholder", b), N("fontSize", 15.0), S("align", "left".into()), S("verticalAlign", "top".into()), N("zIndex", 2.0)]),
        ],
        "caption" => vec![with(geo(0.1, 0.78, 0.8, 0.12), vec![S("placeholder", b), N("fontSize", 16.0), S("align", "left".into())])],
        "number" => vec![
            with(geo(0.1, 0.28, 0.8, 0.30), vec![S("placeholder", "xx%".into()), N("fontSize", 72.0), S("align", "center".into()), S("color", "#3c4043".into())]),
            with(geo(0.1, 0.62, 0.8, 0.10), vec![S("placeholder", b), N("fontSize", 18.0), S("align", "center".into()), N("zIndex", 2.0)]),
        ],
        _ => Vec::new(),
    };
    spec.into_iter().map(|o| mk_text(ids, o)).collect()
}

/// The text tool's click: a 0.3×0.15 box with « Texte » (`PresentationEditorPage.tsx:2056`).
pub fn text_tool_box(ids: &mut dyn IdGen, x: f64, y: f64, z: usize, text: &str) -> Element {
    head(ids, "text", x, y, 0.3, 0.15, z as f64)
        .raw("content", &doc_one_para(text)).n("padding", 8.0).s("verticalAlign", "top").null("background").n("borderRadius", 0.0).null("placeholder")
        .el()
}

/// Options of [`text_box`] (`insertTextBox`): `None` = the web's default / key left out.
#[derive(Debug, Clone, Default)]
pub struct TextBoxOpts {
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub w: Option<f64>,
    pub h: Option<f64>,
    pub font_size: Option<f64>,
    pub align: Option<&'static str>,
    pub bold: Option<bool>,
    pub color: Option<&'static str>,
    pub columns: Option<f64>,
}

/// `insertTextBox`.
pub fn text_box(ids: &mut dyn IdGen, text: &str, o: &TextBoxOpts, z: usize) -> Element {
    let mut b = head(ids, "text", o.x.unwrap_or(0.35), o.y.unwrap_or(0.42), o.w.unwrap_or(0.3), o.h.unwrap_or(0.16), z as f64)
        .raw("content", &doc_one_para(text)).n("padding", 8.0)
        .s("verticalAlign", if o.columns.is_some_and(|c| c != 0.0) { "top" } else { "middle" })
        .null("background").n("borderRadius", 0.0).null("placeholder")
        .n("fontSize", o.font_size.unwrap_or(32.0)).s("align", o.align.unwrap_or("center"));
    if let Some(v) = o.bold {
        b = b.b("bold", v);
    }
    if let Some(c) = o.color {
        b = b.s("color", c);
    }
    if let Some(c) = o.columns {
        b = b.n("columns", c);
    }
    b.el()
}

/// A shape being drawn (`tool === 'shape'`): a zero box at the press.
pub fn shape_at(ids: &mut dyn IdGen, kind: &str, x: f64, y: f64, w: f64, h: f64, z: usize) -> Element {
    head(ids, "shape", x, y, w, h, z as f64)
        .s("shape", if kind.is_empty() { "rect" } else { kind })
        .raw("fill", &solid("#1a73e8")).raw("stroke", &stroke("#1557b0", 0.0, "solid")).null("content")
        .el()
}

/// A line being drawn (`tool === 'line'`), `points` for polylines and freehand.
pub fn line_at(ids: &mut dyn IdGen, kind: &str, x: f64, y: f64, z: usize, points: Option<Vec<(f64, f64)>>) -> Element {
    let mut b = B::new().s("id", &ids.next_id()).s("type", "line").n("x", x).n("y", y).n("w", 0.0).n("h", 0.0)
        .n("x2", x).n("y2", y).n("rotation", 0.0).b("locked", false).b("hidden", false)
        .raw("stroke", &stroke("#202124", 2.0, "solid"))
        .v("arrowEnd", if kind == "arrow" { json!("triangle") } else { Value::Null })
        .n("zIndex", z as f64).s("lineType", kind);
    if let Some(pts) = points {
        b = b.v("points", points_value(&pts));
    }
    b.el()
}

pub fn points_value(pts: &[(f64, f64)]) -> Value {
    Value::Array(pts.iter().map(|p| json!({ "x": num(p.0), "y": num(p.1) })).collect())
}

/// `insertLineEl`: a horizontal line across the middle.
pub fn line_centered(ids: &mut dyn IdGen, kind: &str, z: usize) -> Element {
    B::new().s("id", &ids.next_id()).s("type", "line").n("x", 0.2).n("y", 0.5).n("x2", 0.8).n("y2", 0.5).n("w", 0.0).n("h", 0.0).n("rotation", 0.0)
        .b("locked", false).b("hidden", false).n("zIndex", z as f64).s("lineType", kind)
        .raw("stroke", &stroke("#202124", 2.0, "solid"))
        .v("arrowEnd", if kind == "arrow" { json!("triangle") } else { Value::Null })
        .el()
}

/// `insertShapeAt`: a standard shape in the middle.
pub fn shape_centered(ids: &mut dyn IdGen, kind: &str, z: usize) -> Element {
    shape_at(ids, kind, 0.34, 0.36, 0.32, 0.28, z)
}

/// `insertSeparator`.
pub fn separator(ids: &mut dyn IdGen, z: usize) -> Element {
    B::new().s("id", &ids.next_id()).s("type", "line").n("x", 0.15).n("y", 0.5).n("x2", 0.85).n("y2", 0.5).n("w", 0.0).n("h", 0.0).n("rotation", 0.0)
        .b("locked", false).b("hidden", false).n("zIndex", z as f64).s("lineType", "straight")
        .raw("stroke", &stroke("#5f6368", 2.0, "solid")).null("arrowEnd")
        .el()
}

/// `makeImageElement`: sized to the image's ratio, centred on `(cx, cy)`.
pub fn image(ids: &mut dyn IdGen, src: &str, nat_w: f64, nat_h: f64, cx: f64, cy: f64, z: usize) -> Element {
    let aspect = (nat_h / if nat_w == 0.0 { 1.0 } else { nat_w }) * (SLIDE_W / SLIDE_H);
    let (mut w, mut h) = (0.5, 0.5 * aspect);
    if h > 0.85 {
        h = 0.85;
        w = h / aspect;
    }
    if w > 0.9 {
        w = 0.9;
        h = w * aspect;
    }
    let x = (cx - w / 2.0).min(1.0 - w).max(0.0);
    let y = (cy - h / 2.0).min(1.0 - h).max(0.0);
    head(ids, "image", x, y, w, h, z as f64).s("storagePath", src).s("alt", "").n("opacity", 1.0).el()
}

/// `insertChart`.
pub fn chart(ids: &mut dyn IdGen, kind: &str, z: usize) -> Element {
    let palette = format!("[{}]", crate::chart::CHART_PALETTE.iter().map(|c| quote(c)).collect::<Vec<_>>().join(","));
    head(ids, "chart", 0.18, 0.2, 0.64, 0.55, z as f64)
        .s("chartType", kind)
        .raw("categories", r#"["Cat 1","Cat 2","Cat 3","Cat 4"]"#)
        .raw("series", r#"[{"name":"Série 1","values":[4,7,3,6]},{"name":"Série 2","values":[2,5,6,4]}]"#)
        .b("showLegend", true).s("title", "").raw("palette", &palette)
        .el()
}

/// `insertTable`: a header row (`Col n`) and banded rows, the first style.
pub fn table(ids: &mut dyn IdGen, rows: usize, cols: usize, z: usize) -> Element {
    let mut cells = table::make_cells(rows, cols);
    if let Some(Value::Array(first)) = cells.as_array_mut().and_then(|r| r.first_mut()) {
        for (i, c) in first.iter_mut().enumerate() {
            *c = json!({ "text": format!("Col {}", i + 1) });
        }
    }
    let (_, header, band, border) = table::TABLE_STYLES[0];
    head(ids, "table", 0.12, 0.2, 0.76, (0.1 * rows as f64 + 0.05).min(0.6), z as f64)
        .n("rows", rows as f64).n("cols", cols as f64).v("cells", cells).b("headerRow", true).b("banded", true)
        .s("headerBg", header).s("bandBg", band).s("borderColor", border).n("fontSize", 14.0)
        .el()
}

/// `insertSmartArt`: connectors then boxes, all in one new group; box captions « Élément n » (`item_label`
/// gives the localised text for n).
pub fn smartart(ids: &mut dyn IdGen, kind: SmartArtKind, count: Option<usize>, first_z: usize, item_label: &dyn Fn(usize) -> String) -> Vec<Element> {
    let lay = smartart_layout(kind, count.unwrap_or(if kind == SmartArtKind::Matrix { 4 } else { 3 }));
    let gid = ids.next_id();
    let palette = ["#1a73e8", "#34a853", "#ea4335", "#fbbc04", "#9334e8", "#00acc1", "#ff7043", "#5f6368"];
    let mut z = first_z as f64;
    let mut boxes = Vec::new();
    for (i, b) in lay.boxes.iter().enumerate() {
        boxes.push(
            head(ids, "shape", b.x, b.y, b.w, b.h, z)
                .s("shape", b.shape.unwrap_or(lay.shape)).s("groupId", &gid)
                .raw("fill", &solid(palette[i % palette.len()])).raw("stroke", &stroke("#ffffff", 1.0, "solid"))
                .raw("content", &doc_one_para(&item_label(i + 1)))
                .s("color", "#ffffff").n("fontSize", 16.0)
                .el(),
        );
        z += 1.0;
    }
    let mut conns = Vec::new();
    for c in &lay.connectors {
        conns.push(
            B::new().s("id", &ids.next_id()).s("type", "line").n("x", c.x).n("y", c.y).n("x2", c.x2).n("y2", c.y2).n("w", 0.0).n("h", 0.0).n("rotation", 0.0)
                .b("locked", false).b("hidden", false).n("zIndex", z).s("lineType", "straight")
                .raw("stroke", &stroke("#5f6368", 2.0, "solid")).s("arrowEnd", "triangle").s("groupId", &gid)
                .el(),
        );
        z += 1.0;
    }
    // The web numbers boxes first, then connectors, but inserts connectors first.
    conns.extend(boxes);
    conns
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_slide_has_the_web_placeholders() {
        let els = placeholder_slide_elements(&mut SeqIds::default(), &Placeholders::default());
        let s = els[0].to_json();
        assert!(s.starts_with(r#"{"id":"id1","type":"text","x":0.08,"y":0.3,"w":0.84,"h":0.28,"rotation":0,"zIndex":1"#), "{s}");
        assert_eq!(els[1].s("placeholder"), Some("Cliquez ici pour ajouter un sous-titre"));
    }

    #[test]
    fn layouts_spread_their_keys_after_the_defaults() {
        let els = layout_elements("two_col", &mut SeqIds::default(), &Placeholders::default());
        assert_eq!(els.len(), 3);
        assert_eq!(els[2].f("zIndex"), Some(3.0));
        assert_eq!(els[1].s("verticalAlign"), Some("top"));
        assert!(layout_elements("blank", &mut SeqIds::default(), &Placeholders::default()).is_empty());
    }

    #[test]
    fn an_image_keeps_its_ratio_inside_the_slide() {
        let e = image(&mut SeqIds::default(), "kbfile:1", 1000.0, 1000.0, 0.95, 0.5, 1);
        assert!((e.w() - 0.478125).abs() < 1e-9, "w {}", e.w());
        assert!((e.h() - 0.85).abs() < 1e-9);
        assert!((e.x() + e.w() - 1.0).abs() < 1e-9, "clamped to the right edge");
    }

    #[test]
    fn smartart_groups_its_boxes_and_connectors() {
        let els = smartart(&mut SeqIds::default(), SmartArtKind::Process, None, 5, &|n| format!("Élément {n}"));
        assert_eq!(els.len(), 5);
        let gid = els[0].group_id().map(str::to_string);
        assert!(els.iter().all(|e| e.group_id().map(str::to_string) == gid));
        assert_eq!(els[0].kind(), "line");
        assert_eq!(els[2].f("zIndex"), Some(5.0), "boxes are numbered first");
    }

    #[test]
    fn ids_look_like_the_web_uid() {
        let mut ids = Ids::seeded(42);
        let a = ids.next_id();
        assert_eq!(a.len(), 8);
        assert!(a.chars().all(|c| c.is_ascii_digit() || c.is_ascii_lowercase()));
        assert_ne!(a, ids.next_id());
    }
}
