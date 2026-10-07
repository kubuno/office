//! New elements, exactly as the web builds them (`PresentationEditorPage.tsx`): placeholders and layouts,
//! text boxes, shapes, lines, images, charts, tables, SmartArt, fields. Key order follows the web's object
//! literals, `undefined` keys are left out, so a new element is stored as the web would store it.

use serde_json::{json, Map, Value};

use crate::model::{num, Element, SLIDE_H, SLIDE_W};
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

fn obj(v: Value) -> Element {
    Element::from_value(v).unwrap_or_default()
}

/// `placeholderSlideElements`: the title and subtitle of a new slide.
pub fn placeholder_slide_elements(ids: &mut dyn IdGen, ph: &Placeholders) -> Vec<Element> {
    vec![
        obj(json!({ "id": ids.next_id(), "type": "text", "x": 0.08, "y": 0.30, "w": 0.84, "h": 0.28, "rotation": 0, "zIndex": 1, "locked": false, "hidden": false,
            "content": null, "padding": 8, "verticalAlign": "middle", "background": null, "borderRadius": 0,
            "placeholder": ph.title, "fontSize": 44, "align": "center", "color": "#3c4043" })),
        obj(json!({ "id": ids.next_id(), "type": "text", "x": 0.08, "y": 0.60, "w": 0.84, "h": 0.13, "rotation": 0, "zIndex": 2, "locked": false, "hidden": false,
            "content": null, "padding": 8, "verticalAlign": "middle", "background": null, "borderRadius": 0,
            "placeholder": ph.subtitle, "fontSize": 22, "align": "center" })),
    ]
}

/// `mkText`: the layout placeholder text box with `o`'s keys after the defaults (the web's spread order).
fn mk_text(ids: &mut dyn IdGen, o: Value) -> Element {
    let mut m = Map::new();
    m.insert("id".into(), Value::String(ids.next_id()));
    for (k, v) in [("type", json!("text")), ("rotation", json!(0)), ("zIndex", json!(1)), ("locked", json!(false)), ("hidden", json!(false)), ("content", Value::Null), ("padding", json!(8)), ("verticalAlign", json!("middle")), ("background", Value::Null), ("borderRadius", json!(0)), ("placeholder", Value::Null)] {
        m.insert(k.into(), v);
    }
    if let Value::Object(o) = o {
        for (k, v) in o {
            m.insert(k, v);
        }
    }
    Element(m)
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
    let (t, b) = (ph.title.as_str(), ph.body.as_str());
    let spec: Vec<Value> = match id {
        "title" => vec![
            json!({ "x": 0.08, "y": 0.34, "w": 0.84, "h": 0.20, "placeholder": t, "fontSize": 44, "align": "center", "color": "#3c4043", "zIndex": 1 }),
            json!({ "x": 0.08, "y": 0.57, "w": 0.84, "h": 0.10, "placeholder": ph.subtitle, "fontSize": 20, "align": "center", "zIndex": 2 }),
        ],
        "section" => vec![json!({ "x": 0.1, "y": 0.40, "w": 0.8, "h": 0.18, "placeholder": t, "fontSize": 36, "align": "left", "color": "#3c4043" })],
        "title_body" => vec![
            json!({ "x": 0.06, "y": 0.06, "w": 0.88, "h": 0.16, "placeholder": t, "fontSize": 30, "align": "left", "color": "#3c4043" }),
            json!({ "x": 0.06, "y": 0.26, "w": 0.88, "h": 0.66, "placeholder": b, "fontSize": 18, "align": "left", "verticalAlign": "top", "zIndex": 2 }),
        ],
        "two_col" => vec![
            json!({ "x": 0.06, "y": 0.06, "w": 0.88, "h": 0.14, "placeholder": t, "fontSize": 28, "align": "left", "color": "#3c4043" }),
            json!({ "x": 0.06, "y": 0.24, "w": 0.43, "h": 0.68, "placeholder": b, "fontSize": 16, "align": "left", "verticalAlign": "top", "zIndex": 2 }),
            json!({ "x": 0.51, "y": 0.24, "w": 0.43, "h": 0.68, "placeholder": b, "fontSize": 16, "align": "left", "verticalAlign": "top", "zIndex": 3 }),
        ],
        "title_only" => vec![json!({ "x": 0.06, "y": 0.06, "w": 0.88, "h": 0.16, "placeholder": t, "fontSize": 30, "align": "left", "color": "#3c4043" })],
        "one_col" => vec![json!({ "x": 0.1, "y": 0.1, "w": 0.8, "h": 0.8, "placeholder": b, "fontSize": 18, "align": "left", "verticalAlign": "top" })],
        "main" => vec![json!({ "x": 0.08, "y": 0.34, "w": 0.84, "h": 0.30, "placeholder": t, "fontSize": 48, "align": "left", "color": "#3c4043" })],
        "section_desc" => vec![
            json!({ "x": 0.05, "y": 0.18, "w": 0.42, "h": 0.18, "placeholder": t, "fontSize": 26, "align": "left", "color": "#3c4043" }),
            json!({ "x": 0.05, "y": 0.40, "w": 0.42, "h": 0.42, "placeholder": b, "fontSize": 15, "align": "left", "verticalAlign": "top", "zIndex": 2 }),
        ],
        "caption" => vec![json!({ "x": 0.1, "y": 0.78, "w": 0.8, "h": 0.12, "placeholder": b, "fontSize": 16, "align": "left" })],
        "number" => vec![
            json!({ "x": 0.1, "y": 0.28, "w": 0.8, "h": 0.30, "placeholder": "xx%", "fontSize": 72, "align": "center", "color": "#3c4043" }),
            json!({ "x": 0.1, "y": 0.62, "w": 0.8, "h": 0.10, "placeholder": b, "fontSize": 18, "align": "center", "zIndex": 2 }),
        ],
        _ => Vec::new(),
    };
    spec.into_iter().map(|o| mk_text(ids, o)).collect()
}

/// The text tool's click: a 0.3×0.15 box with « Texte » (`PresentationEditorPage.tsx:2056`).
pub fn text_tool_box(ids: &mut dyn IdGen, x: f64, y: f64, z: usize, text: &str) -> Element {
    obj(json!({ "id": ids.next_id(), "type": "text", "x": num(x), "y": num(y), "w": 0.3, "h": 0.15, "rotation": 0, "zIndex": z, "locked": false, "hidden": false,
        "content": { "type": "doc", "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": text }] }] },
        "padding": 8, "verticalAlign": "top", "background": null, "borderRadius": 0, "placeholder": null }))
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
    let content = if text.is_empty() { json!([]) } else { json!([{ "type": "text", "text": text }]) };
    let mut m = obj(json!({ "id": ids.next_id(), "type": "text", "x": num(o.x.unwrap_or(0.35)), "y": num(o.y.unwrap_or(0.42)), "w": num(o.w.unwrap_or(0.3)), "h": num(o.h.unwrap_or(0.16)), "rotation": 0,
        "zIndex": z, "locked": false, "hidden": false,
        "content": { "type": "doc", "content": [{ "type": "paragraph", "content": content }] },
        "padding": 8, "verticalAlign": if o.columns.is_some_and(|c| c != 0.0) { "top" } else { "middle" }, "background": null, "borderRadius": 0, "placeholder": null,
        "fontSize": num(o.font_size.unwrap_or(32.0)), "align": o.align.unwrap_or("center") }));
    if let Some(b) = o.bold {
        m.set_b("bold", b);
    }
    if let Some(c) = o.color {
        m.set_s("color", c);
    }
    if let Some(c) = o.columns {
        m.set_f("columns", c);
    }
    m
}

/// A shape being drawn (`tool === 'shape'`): a zero box at the press.
pub fn shape_at(ids: &mut dyn IdGen, kind: &str, x: f64, y: f64, w: f64, h: f64, z: usize) -> Element {
    obj(json!({ "id": ids.next_id(), "type": "shape", "x": num(x), "y": num(y), "w": num(w), "h": num(h), "rotation": 0, "zIndex": z, "locked": false, "hidden": false,
        "shape": if kind.is_empty() { "rect" } else { kind },
        "fill": { "type": "color", "color": "#1a73e8" }, "stroke": { "color": "#1557b0", "width": 0, "style": "solid" }, "content": null }))
}

/// A line being drawn (`tool === 'line'`), `points` for polylines and freehand.
pub fn line_at(ids: &mut dyn IdGen, kind: &str, x: f64, y: f64, z: usize, points: Option<Vec<(f64, f64)>>) -> Element {
    let mut e = obj(json!({ "id": ids.next_id(), "type": "line", "x": num(x), "y": num(y), "w": 0, "h": 0,
        "x2": num(x), "y2": num(y), "rotation": 0, "locked": false, "hidden": false,
        "stroke": { "color": "#202124", "width": 2, "style": "solid" },
        "arrowEnd": if kind == "arrow" { json!("triangle") } else { Value::Null },
        "zIndex": z, "lineType": kind }));
    if let Some(pts) = points {
        e.set("points", points_value(&pts));
    }
    e
}

pub fn points_value(pts: &[(f64, f64)]) -> Value {
    Value::Array(pts.iter().map(|p| json!({ "x": num(p.0), "y": num(p.1) })).collect())
}

/// `insertLineEl`: a horizontal line across the middle.
pub fn line_centered(ids: &mut dyn IdGen, kind: &str, z: usize) -> Element {
    obj(json!({ "id": ids.next_id(), "type": "line", "x": 0.2, "y": 0.5, "x2": 0.8, "y2": 0.5, "w": 0, "h": 0, "rotation": 0,
        "locked": false, "hidden": false, "zIndex": z, "lineType": kind,
        "stroke": { "color": "#202124", "width": 2, "style": "solid" },
        "arrowEnd": if kind == "arrow" { json!("triangle") } else { Value::Null } }))
}

/// `insertShapeAt`: a standard shape in the middle.
pub fn shape_centered(ids: &mut dyn IdGen, kind: &str, z: usize) -> Element {
    shape_at(ids, kind, 0.34, 0.36, 0.32, 0.28, z)
}

/// `insertSeparator`.
pub fn separator(ids: &mut dyn IdGen, z: usize) -> Element {
    obj(json!({ "id": ids.next_id(), "type": "line", "x": 0.15, "y": 0.5, "x2": 0.85, "y2": 0.5, "w": 0, "h": 0, "rotation": 0, "locked": false, "hidden": false, "zIndex": z, "lineType": "straight", "stroke": { "color": "#5f6368", "width": 2, "style": "solid" }, "arrowEnd": null }))
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
    obj(json!({ "id": ids.next_id(), "type": "image", "x": num(x), "y": num(y), "w": num(w), "h": num(h), "rotation": 0, "zIndex": z, "locked": false, "hidden": false, "storagePath": src, "alt": "", "opacity": 1 }))
}

/// `insertChart`.
pub fn chart(ids: &mut dyn IdGen, kind: &str, z: usize) -> Element {
    obj(json!({ "id": ids.next_id(), "type": "chart", "x": 0.18, "y": 0.2, "w": 0.64, "h": 0.55, "rotation": 0,
        "zIndex": z, "locked": false, "hidden": false, "chartType": kind,
        "categories": ["Cat 1", "Cat 2", "Cat 3", "Cat 4"],
        "series": [{ "name": "Série 1", "values": [4, 7, 3, 6] }, { "name": "Série 2", "values": [2, 5, 6, 4] }],
        "showLegend": true, "title": "", "palette": crate::chart::CHART_PALETTE }))
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
    obj(json!({ "id": ids.next_id(), "type": "table", "x": 0.12, "y": 0.2, "w": 0.76, "h": num((0.1 * rows as f64 + 0.05).min(0.6)), "rotation": 0,
        "zIndex": z, "locked": false, "hidden": false,
        "rows": rows, "cols": cols, "cells": cells, "headerRow": true, "banded": true,
        "headerBg": header, "bandBg": band, "borderColor": border, "fontSize": 14 }))
}

/// `insertSmartArt`: connectors then boxes, all in one new group; box captions « Élément n » (`item_label`
/// gives the localised text for n).
pub fn smartart(ids: &mut dyn IdGen, kind: SmartArtKind, count: Option<usize>, first_z: usize, item_label: &dyn Fn(usize) -> String) -> Vec<Element> {
    let lay = smartart_layout(kind, count.unwrap_or(if kind == SmartArtKind::Matrix { 4 } else { 3 }));
    let gid = ids.next_id();
    let palette = ["#1a73e8", "#34a853", "#ea4335", "#fbbc04", "#9334e8", "#00acc1", "#ff7043", "#5f6368"];
    let mut z = first_z;
    let mut boxes = Vec::new();
    for (i, b) in lay.boxes.iter().enumerate() {
        boxes.push(obj(json!({ "id": ids.next_id(), "type": "shape", "x": num(b.x), "y": num(b.y), "w": num(b.w), "h": num(b.h), "rotation": 0, "zIndex": z, "locked": false, "hidden": false,
            "shape": b.shape.unwrap_or(lay.shape), "groupId": gid,
            "fill": { "type": "color", "color": palette[i % palette.len()] }, "stroke": { "color": "#ffffff", "width": 1, "style": "solid" },
            "content": { "type": "doc", "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": item_label(i + 1) }] }] },
            "color": "#ffffff", "fontSize": 16 })));
        z += 1;
    }
    let mut conns = Vec::new();
    for c in &lay.connectors {
        conns.push(obj(json!({ "id": ids.next_id(), "type": "line", "x": num(c.x), "y": num(c.y), "x2": num(c.x2), "y2": num(c.y2), "w": 0, "h": 0, "rotation": 0, "locked": false, "hidden": false,
            "zIndex": z, "lineType": "straight", "stroke": { "color": "#5f6368", "width": 2, "style": "solid" }, "arrowEnd": "triangle", "groupId": gid })));
        z += 1;
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
        let s = serde_json::to_string(&els[0].to_value()).unwrap_or_default();
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
