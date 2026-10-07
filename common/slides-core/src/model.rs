//! The presentation as stored, held losslessly.
//!
//! The content file (`.kbsld`, gzip JSON) is `{"version":1,"slides":{"<uuid>":{elements, background, notes,
//! transition}}}`; the slide ORDER and the hidden flags are rows of the server's `office.slides`, the title and
//! the theme columns of `office.presentations` (vskubuno docs/PRESENTATIONS-DESKTOP.md §2). Every object is an
//! [`Obj`]: its keys in their stored order, its untouched values written back byte for byte — so an element the
//! user did not touch is written back exactly, and a key a newer web editor adds survives a desktop edit.

use serde_json::Value;

pub use crate::obj::{Obj, Slot};
pub use kubuno_office_shapes_core::Rect;

/// The canonical slide (`SLIDE_W` × `SLIDE_H`): element geometry is in fractions of it, absolute sizes (fonts,
/// strokes, shadows) in its pixels. QUIRK: fixed 16:9 even for a 4:3 presentation, like the web.
pub const SLIDE_W: f64 = 960.0;
pub const SLIDE_H: f64 = 540.0;

/// A number as the web writes it: an integral value is an integer (`120`, never `120.0`).
pub fn num(v: f64) -> Value {
    if v.is_finite() && v.fract() == 0.0 && v.abs() < 9_007_199_254_740_992.0 {
        Value::from(v as i64)
    } else if v.is_finite() {
        serde_json::Number::from_f64(v).map(Value::Number).unwrap_or(Value::Null)
    } else {
        // `JSON.stringify(NaN)` is `null`.
        Value::Null
    }
}

/// JavaScript truthiness of a JSON value (`if (el.hidden)`).
pub fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

/// A number field, JavaScript-style: absent, `null` or not a number → `None`.
pub fn as_f64(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64)
}

/// One slide element: its ordered object, untouched until a setter changes a key.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Element(pub Obj);

impl Element {
    /// From a parsed value (key order: `serde_json`'s; tests and values built by hand).
    pub fn from_value(v: Value) -> Option<Element> {
        Obj::from_value(&v).map(Element)
    }

    /// From JSON text, keys in their written order.
    pub fn parse(text: &str) -> Option<Element> {
        Obj::parse(text).ok().map(Element)
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }

    pub fn f(&self, key: &str) -> Option<f64> {
        as_f64(self.0.get(key))
    }

    pub fn f_or(&self, key: &str, default: f64) -> f64 {
        self.f(key).unwrap_or(default)
    }

    pub fn s(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(Value::as_str)
    }

    pub fn truthy(&self, key: &str) -> bool {
        truthy(self.0.get(key))
    }

    pub fn set(&mut self, key: &str, v: Value) {
        self.0.set(key, v);
    }

    /// Sets a key to JSON text kept as written (nested objects keep the web's key order).
    pub fn set_raw(&mut self, key: &str, json: &str) {
        self.0.set_slot(key, Slot::raw(json));
    }

    pub fn set_f(&mut self, key: &str, v: f64) {
        self.set(key, num(v));
    }

    pub fn set_b(&mut self, key: &str, v: bool) {
        self.set(key, Value::Bool(v));
    }

    pub fn set_s(&mut self, key: &str, v: &str) {
        self.set(key, Value::String(v.to_string()));
    }

    /// Removes a key (the web's `undefined`: absent from the JSON).
    pub fn remove(&mut self, key: &str) {
        self.0.remove(key);
    }

    pub fn id(&self) -> &str {
        self.s("id").unwrap_or("")
    }

    /// `type`: text, shape, image, line, chart, table.
    pub fn kind(&self) -> &str {
        self.s("type").unwrap_or("")
    }

    pub fn x(&self) -> f64 {
        self.f_or("x", 0.0)
    }
    pub fn y(&self) -> f64 {
        self.f_or("y", 0.0)
    }
    pub fn w(&self) -> f64 {
        self.f_or("w", 0.0)
    }
    pub fn h(&self) -> f64 {
        self.f_or("h", 0.0)
    }
    pub fn rotation(&self) -> f64 {
        self.f_or("rotation", 0.0)
    }
    /// `zIndex ?? 0`.
    pub fn z_index(&self) -> f64 {
        self.f_or("zIndex", 0.0)
    }
    pub fn locked(&self) -> bool {
        self.truthy("locked")
    }
    pub fn hidden(&self) -> bool {
        self.truthy("hidden")
    }
    pub fn group_id(&self) -> Option<&str> {
        self.s("groupId").filter(|g| !g.is_empty())
    }

    /// The line's points (`points`, else the two ends), fractions of the slide.
    pub fn line_points(&self) -> Vec<(f64, f64)> {
        if let Some(Value::Array(pts)) = self.0.get("points") {
            if pts.len() >= 2 {
                return pts.iter().map(|p| (as_f64(p.get("x")).unwrap_or(0.0), as_f64(p.get("y")).unwrap_or(0.0))).collect();
            }
        }
        vec![(self.x(), self.y()), (self.f_or("x2", 0.0), self.f_or("y2", 0.0))]
    }

    /// `elemBBox`: the box in slide fractions (a line's from its points).
    pub fn bbox(&self) -> Rect {
        if self.kind() == "line" {
            let pts = self.line_points();
            let min_x = pts.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
            let min_y = pts.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
            let max_x = pts.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
            let max_y = pts.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
            return Rect { x: min_x, y: min_y, w: max_x - min_x, h: max_y - min_y };
        }
        Rect { x: self.x(), y: self.y(), w: self.w(), h: self.h() }
    }

    /// The element as a value (reading; key order is `serde_json`'s).
    pub fn to_value(&self) -> Value {
        self.0.to_value()
    }

    /// The element as stored JSON text.
    pub fn to_json(&self) -> String {
        self.0.to_json()
    }
}

/// A slide background (`SlideBackground`).
#[derive(Debug, Clone, PartialEq)]
pub enum Background {
    Color(Option<String>),
    /// Multi-stop `grad` (preferred) or the legacy two-stop `gradient`.
    Gradient(Gradient),
    Image(String),
    /// A gradient or image background without its data: nothing is drawn, like the web.
    Empty,
}

/// A gradient (`@ui` `Gradient`), the legacy `{from, to, angle}` converted.
#[derive(Debug, Clone, PartialEq)]
pub struct Gradient {
    pub radial: bool,
    pub angle: f64,
    /// (colour, position 0..1, opacity 0..100), in stored order.
    pub stops: Vec<(String, f64, f64)>,
    /// The legacy two-stop form: drawn from the top-left corner to the bottom-right one.
    pub legacy: bool,
}

impl Gradient {
    pub fn parse(v: &Value) -> Option<Gradient> {
        let stops = v.get("stops")?.as_array()?;
        Some(Gradient {
            radial: v.get("type").and_then(Value::as_str) == Some("radial"),
            angle: as_f64(v.get("angle")).unwrap_or(0.0),
            stops: stops
                .iter()
                .map(|s| (s.get("color").and_then(Value::as_str).unwrap_or("#000000").to_string(), as_f64(s.get("position")).unwrap_or(0.0), as_f64(s.get("opacity")).unwrap_or(100.0)))
                .collect(),
            legacy: false,
        })
    }

    pub fn legacy(v: &Value) -> Option<Gradient> {
        let from = v.get("from")?.as_str()?.to_string();
        let to = v.get("to")?.as_str()?.to_string();
        Some(Gradient { radial: false, angle: as_f64(v.get("angle")).unwrap_or(0.0), stops: vec![(from, 0.0, 100.0), (to, 1.0, 100.0)], legacy: true })
    }
}

impl Background {
    pub fn parse(v: Option<&Value>) -> Background {
        let Some(v) = v.filter(|v| v.is_object()) else { return Background::Color(None) };
        match v.get("type").and_then(Value::as_str) {
            None | Some("color") => Background::Color(v.get("color").and_then(Value::as_str).map(str::to_string)),
            Some("gradient") => {
                if let Some(g) = v.get("grad").and_then(Gradient::parse) {
                    Background::Gradient(g)
                } else if let Some(g) = v.get("gradient").and_then(Gradient::legacy) {
                    Background::Gradient(g)
                } else {
                    Background::Empty
                }
            }
            Some("image") => match v.get("imagePath").and_then(Value::as_str).filter(|p| !p.is_empty()) {
                Some(p) => Background::Image(p.to_string()),
                None => Background::Empty,
            },
            Some(_) => Background::Empty,
        }
    }
}

/// A slide's content (`elements`, `background`, `notes`, `transition`), every other key kept.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SlideData {
    /// The stored object; its `elements` slot is rewritten from [`SlideData::elements`] when written.
    obj: Obj,
    pub elements: Vec<Element>,
}

/// `cf::empty_slide_data()`, as the server writes it.
pub const EMPTY_SLIDE: &str = r##"{"elements":[],"background":{"type":"color","color":"#ffffff"},"notes":"","transition":{"type":"none","duration":0.3}}"##;

impl SlideData {
    pub fn empty() -> SlideData {
        SlideData::parse(EMPTY_SLIDE)
    }

    /// From JSON text (a slide object); anything else is an empty object.
    pub fn parse(text: &str) -> SlideData {
        SlideData::from_obj(Obj::parse(text).unwrap_or_default())
    }

    pub fn from_obj(mut obj: Obj) -> SlideData {
        let elements = match obj.slot("elements") {
            Some(s) if s.v.is_array() => crate::obj::parse_array_of_obj(&s.text()).into_iter().map(Element).collect(),
            _ => Vec::new(),
        };
        if obj.contains("elements") {
            obj.set("elements", Value::Array(Vec::new()));
        }
        SlideData { obj, elements }
    }

    /// From a parsed value (tests).
    pub fn from_value(v: Value) -> SlideData {
        SlideData::parse(&v.to_string())
    }

    /// The elements as the web sends them in `PUT …/slides/:sid`, as JSON text.
    pub fn elements_json(&self) -> String {
        let mut s = String::from("[");
        for (i, e) in self.elements.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            s.push_str(&e.to_json());
        }
        s.push(']');
        s
    }

    /// The slide as stored JSON text.
    pub fn to_json(&self) -> String {
        let mut obj = self.obj.clone();
        if obj.contains("elements") || !self.elements.is_empty() {
            obj.set_slot("elements", Slot::raw(&self.elements_json()));
        }
        obj.to_json()
    }

    /// The slide as a value (reading).
    pub fn to_value(&self) -> Value {
        serde_json::from_str(&self.to_json()).unwrap_or(Value::Null)
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.obj.get(key)
    }

    /// The stored text of a key (a background or a transition, as the server wants it back).
    pub fn json_of(&self, key: &str) -> Option<String> {
        self.obj.slot(key).map(Slot::text)
    }

    pub fn set(&mut self, key: &str, v: Value) {
        self.obj.set(key, v);
    }

    pub fn set_raw(&mut self, key: &str, json: &str) {
        self.obj.set_slot(key, Slot::raw(json));
    }

    pub fn notes(&self) -> &str {
        self.obj.get("notes").and_then(Value::as_str).unwrap_or("")
    }

    pub fn set_notes(&mut self, notes: &str) {
        self.set("notes", Value::String(notes.to_string()));
    }

    pub fn background(&self) -> Background {
        Background::parse(self.obj.get("background"))
    }

    /// `transition.type` (`none` when absent) and `transition.duration` (ms, `None` when absent).
    pub fn transition(&self) -> (String, Option<f64>) {
        let t = self.obj.get("transition");
        let kind = t.and_then(|t| t.get("type")).and_then(Value::as_str).unwrap_or("none").to_string();
        (kind, as_f64(t.and_then(|t| t.get("duration"))))
    }

    pub fn element(&self, id: &str) -> Option<&Element> {
        self.elements.iter().find(|e| e.id() == id)
    }

    pub fn element_mut(&mut self, id: &str) -> Option<&mut Element> {
        self.elements.iter_mut().find(|e| e.id() == id)
    }
}

/// One slide: its id, the server's hidden flag, its content.
#[derive(Debug, Clone, PartialEq)]
pub struct Slide {
    pub id: String,
    pub hidden: bool,
    pub data: SlideData,
}

/// The presentation's theme (`Presentation.theme`): rendering defaults only.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub name: String,
    pub primary_color: String,
    pub bg_color: String,
    pub font_family: String,
    pub accent_color: String,
    pub text_color: String,
}

impl Default for Theme {
    /// `DEFAULT_THEME` (`PresentationEditorPage.tsx:102`).
    fn default() -> Self {
        Theme {
            name: "Défaut".into(),
            primary_color: "#1a73e8".into(),
            bg_color: "#ffffff".into(),
            font_family: "Outfit, Arial, sans-serif".into(),
            accent_color: "#ea4335".into(),
            text_color: "#202124".into(),
        }
    }
}

impl Theme {
    /// The server's `theme` column; a missing key keeps the default's value (the web reads `theme?.bgColor ?? …`).
    pub fn parse(v: Option<&Value>) -> Theme {
        let mut t = Theme::default();
        let Some(v) = v.filter(|v| v.is_object()) else { return t };
        let s = |k: &str| v.get(k).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
        if let Some(x) = s("name") {
            t.name = x;
        }
        if let Some(x) = s("primaryColor") {
            t.primary_color = x;
        }
        if let Some(x) = s("bgColor") {
            t.bg_color = x;
        }
        if let Some(x) = s("fontFamily") {
            t.font_family = x;
        }
        if let Some(x) = s("accentColor") {
            t.accent_color = x;
        }
        if let Some(x) = s("textColor") {
            t.text_color = x;
        }
        t
    }
}

/// Errors reading a presentation.
#[derive(Debug, Clone, PartialEq)]
pub enum LoadError {
    NotJson(String),
    NotAPresentation,
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::NotJson(e) => write!(f, "not JSON: {e}"),
            LoadError::NotAPresentation => write!(f, "not a presentation (no `slides`)"),
        }
    }
}

impl std::error::Error for LoadError {}

/// The whole presentation: the content file's root (version and unknown keys), the slides in order, the
/// title and the theme.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Deck {
    root: Obj,
    pub slides: Vec<Slide>,
    pub title: String,
    pub theme: Theme,
    /// The server's metadata (`presentation`), kept for the backstage (aspect ratio, dates…).
    pub meta: Value,
}

/// gzip's signature: the content file is gzipped, an older or exported one may be plain.
pub fn is_gzip(bytes: &[u8]) -> bool {
    bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b
}

impl Deck {
    /// From the server: `GET /presentations/:id` (`{presentation, slides:[summary]}`) and the content file's JSON
    /// text (`join`'s `content`, or the `.kbsld`). Slides are in the server's order; a slide missing from the
    /// content gets `empty_slide_data()` like `cf::get_slide_data`; content without a row is dropped.
    pub fn from_server(meta: &Value, content_json: &str) -> Deck {
        let pres = meta.get("presentation").cloned().unwrap_or(Value::Null);
        let mut root = Obj::parse(content_json).unwrap_or_default();
        let stored = root.slot("slides").and_then(Slot::as_obj).unwrap_or_default();
        if root.contains("slides") {
            root.set("slides", Value::Object(Default::default()));
        }
        let mut summaries: Vec<&Value> = meta.get("slides").and_then(Value::as_array).map(|a| a.iter().collect()).unwrap_or_default();
        summaries.sort_by(|a, b| as_f64(a.get("position")).unwrap_or(0.0).total_cmp(&as_f64(b.get("position")).unwrap_or(0.0)));
        let slides = summaries
            .iter()
            .filter_map(|s| {
                let id = s.get("id")?.as_str()?.to_string();
                let data = stored.slot(&id).map(|sl| SlideData::parse(&sl.text())).unwrap_or_else(SlideData::empty);
                Some(Slide { id, hidden: truthy(s.get("is_hidden")), data })
            })
            .collect();
        Deck {
            root,
            slides,
            title: pres.get("title").and_then(Value::as_str).unwrap_or_default().to_string(),
            theme: Theme::parse(pres.get("theme")),
            meta: pres,
        }
    }

    /// From a file: a content file (`.kbsld`, JSON) — the slides in the file's key order, all visible — or a
    /// desktop bundle (`{presentation, slides, content}`, what [`Deck::to_bundle`] writes), which keeps its
    /// order, hidden flags, title and theme.
    pub fn from_json(bytes: &[u8]) -> Result<Deck, LoadError> {
        let text = std::str::from_utf8(bytes).map_err(|e| LoadError::NotJson(e.to_string()))?;
        let top = Obj::parse(text).map_err(|e| LoadError::NotJson(e.to_string()))?;
        if let Some(content) = top.slot("content").filter(|c| c.v.get("slides").is_some()) {
            return Ok(Deck::from_server(&top.to_value(), &content.text()));
        }
        let ids: Vec<Value> = match top.slot("slides").and_then(Slot::as_obj) {
            Some(slides) if top.get("slides").is_some_and(Value::is_object) => {
                slides.keys().enumerate().map(|(i, k)| serde_json::json!({ "id": k, "position": i, "is_hidden": false })).collect()
            }
            _ => return Err(LoadError::NotAPresentation),
        };
        Ok(Deck::from_server(&serde_json::json!({ "slides": ids }), text))
    }

    /// The content file's JSON text (`{"version":1,"slides":{…}}`), slides in deck order, root keys kept.
    pub fn content_json(&self) -> String {
        let mut root = self.root.clone();
        if !root.contains("version") {
            root.0.insert(0, ("version".into(), Slot::raw("1")));
        }
        let mut slides = Obj::new();
        for s in &self.slides {
            slides.set_slot(&s.id, Slot::raw(&s.data.to_json()));
        }
        root.set_slot("slides", Slot::raw(&slides.to_json()));
        root.to_json()
    }

    /// A self-contained local copy: the content plus what the server keeps in its tables (order, hidden flags,
    /// title, theme) — what the desktop writes for a local file and its crash journal.
    pub fn to_bundle(&self) -> String {
        let mut pres = match &self.meta {
            Value::Object(m) => m.clone(),
            _ => serde_json::Map::new(),
        };
        pres.insert("title".into(), Value::String(self.title.clone()));
        let slides: Vec<Value> = self.slides.iter().enumerate().map(|(i, s)| serde_json::json!({ "id": s.id, "position": i, "is_hidden": s.hidden })).collect();
        let mut top = Obj::new();
        top.set("presentation", Value::Object(pres));
        top.set("slides", Value::Array(slides));
        top.set_slot("content", Slot::raw(&self.content_json()));
        top.to_json()
    }

    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.slides.iter().position(|s| s.id == id)
    }

    pub fn slide(&self, id: &str) -> Option<&Slide> {
        self.slides.iter().find(|s| s.id == id)
    }

    pub fn slide_mut(&mut self, id: &str) -> Option<&mut Slide> {
        self.slides.iter_mut().find(|s| s.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn numbers_are_written_like_the_web() {
        assert_eq!(serde_json::to_string(&num(120.0)).ok().as_deref(), Some("120"));
        assert_eq!(serde_json::to_string(&num(0.5)).ok().as_deref(), Some("0.5"));
        assert_eq!(num(f64::NAN), Value::Null);
    }

    #[test]
    fn an_untouched_presentation_round_trips_byte_for_byte() {
        let src = r##"{"version":1,"slides":{"b":{"notes":"n","elements":[{"id":"e1","type":"text","x":0.08,"y":0.30,"w":0.84,"h":0.28,"rotation":0,"zIndex":1,"fill":{"type":"color","color":"#fff"},"future":{"k":[1,2.5]}}],"background":{"type":"color","color":"#fff"},"transition":{"type":"fade","duration":500},"extra":true},"a":{"elements":[]}},"other":"kept"}"##;
        let deck = Deck::from_json(src.as_bytes()).expect("deck");
        assert_eq!(deck.slides.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["b", "a"]);
        assert_eq!(deck.content_json(), src);
    }

    #[test]
    fn an_edit_changes_one_key_in_place() {
        let mut d = SlideData::parse(r##"{"elements":[{"id":"x","x":0.1,"fill":{"type":"color","color":"#f00"},"y":0.2}],"notes":""}"##);
        d.elements[0].set_f("x", 0.5);
        d.elements[0].remove("y");
        assert_eq!(d.to_json(), r##"{"elements":[{"id":"x","x":0.5,"fill":{"type":"color","color":"#f00"}}],"notes":""}"##);
    }

    #[test]
    fn the_server_order_and_hidden_flags_win() {
        let meta = json!({ "presentation": { "title": "Deck", "theme": { "bgColor": "#000" } },
                           "slides": [ { "id": "b", "position": 1, "is_hidden": true }, { "id": "a", "position": 0 }, { "id": "c", "position": 2 } ] });
        let deck = Deck::from_server(&meta, r#"{"version":1,"slides":{"a":{"elements":[]},"b":{"elements":[]},"gone":{}}}"#);
        assert_eq!(deck.slides.iter().map(|s| (s.id.as_str(), s.hidden)).collect::<Vec<_>>(), [("a", false), ("b", true), ("c", false)]);
        assert_eq!(deck.slides[2].data, SlideData::empty());
        assert_eq!(deck.title, "Deck");
        assert_eq!(deck.theme.bg_color, "#000");
        assert_eq!(deck.theme.text_color, "#202124");
        let again = Deck::from_json(deck.to_bundle().as_bytes()).expect("bundle");
        assert_eq!(again.slides, deck.slides);
        assert_eq!(again.title, "Deck");
        assert!(!deck.content_json().contains("gone"), "a slide without a row is dropped");
    }

    #[test]
    fn a_line_box_comes_from_its_points() {
        let l = Element::from_value(json!({ "type": "line", "x": 0.5, "y": 0.5, "x2": 0.1, "y2": 0.9 })).expect("line");
        let b = l.bbox();
        assert!((b.x - 0.1).abs() < 1e-12 && (b.w - 0.4).abs() < 1e-12 && (b.h - 0.4).abs() < 1e-12);
    }

    #[test]
    fn backgrounds_parse_every_stored_form() {
        assert_eq!(Background::parse(None), Background::Color(None));
        assert_eq!(Background::parse(Some(&json!({ "type": "image", "imagePath": "kbfile:1" }))), Background::Image("kbfile:1".into()));
        let g = Background::parse(Some(&json!({ "type": "gradient", "gradient": { "from": "#000", "to": "#fff", "angle": 45 } })));
        assert!(matches!(g, Background::Gradient(Gradient { legacy: true, .. })));
        assert!(Deck::from_json(b"{\"x\":1}").is_err());
    }
}
