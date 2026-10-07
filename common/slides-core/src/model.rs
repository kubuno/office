//! The presentation as stored, held losslessly.
//!
//! The content file (`.kbsld`, gzip JSON) is `{"version":1,"slides":{"<uuid>":{elements, background, notes,
//! transition}}}`; the slide ORDER and the hidden flags are rows of the server's `office.slides`, the title and
//! the theme columns of `office.presentations` (vskubuno docs/PRESENTATIONS-DESKTOP.md §2). This module keeps
//! every JSON object as it was read — key order, unknown keys and exact numbers included — and offers typed
//! accessors over it, so an element the user did not touch is written back byte for byte, and a key a newer
//! web editor adds survives a desktop edit.

use serde_json::{Map, Value};

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

/// One slide element: its JSON object, untouched until a setter changes a key.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Element(pub Map<String, Value>);

impl Element {
    pub fn from_value(v: Value) -> Option<Element> {
        match v {
            Value::Object(m) => Some(Element(m)),
            _ => None,
        }
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
        self.0.insert(key.to_string(), v);
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
        self.0.shift_remove(key);
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

    pub fn to_value(&self) -> Value {
        Value::Object(self.0.clone())
    }
}

/// A slide background (`SlideBackground`).
#[derive(Debug, Clone, PartialEq)]
pub enum Background {
    Color(Option<String>),
    /// Multi-stop `grad` (preferred) or the legacy two-stop `gradient`.
    Gradient(Gradient),
    Image(String),
    /// A gradient or image background without its data: painted like the web (nothing drawn for a gradient,
    /// the theme colour never), i.e. transparent.
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
    /// The stored object; its `elements` value is replaced by [`SlideData::elements`] when written.
    obj: Map<String, Value>,
    pub elements: Vec<Element>,
}

impl SlideData {
    /// `cf::empty_slide_data()`.
    pub fn empty() -> SlideData {
        let v = serde_json::json!({
            "elements": [],
            "background": { "type": "color", "color": "#ffffff" },
            "notes": "",
            "transition": { "type": "none", "duration": 0.3 }
        });
        SlideData::from_value(v)
    }

    pub fn from_value(v: Value) -> SlideData {
        let mut obj = match v {
            Value::Object(m) => m,
            _ => Map::new(),
        };
        let elements = match obj.get_mut("elements") {
            Some(Value::Array(a)) => std::mem::take(a).into_iter().filter_map(Element::from_value).collect(),
            _ => Vec::new(),
        };
        SlideData { obj, elements }
    }

    pub fn to_value(&self) -> Value {
        let mut obj = self.obj.clone();
        let els = Value::Array(self.elements.iter().map(Element::to_value).collect());
        if obj.contains_key("elements") || !self.elements.is_empty() {
            obj.insert("elements".to_string(), els);
        }
        Value::Object(obj)
    }

    /// The elements as the web sends them in `PUT …/slides/:sid`.
    pub fn elements_value(&self) -> Value {
        Value::Array(self.elements.iter().map(Element::to_value).collect())
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.obj.get(key)
    }

    pub fn set(&mut self, key: &str, v: Value) {
        self.obj.insert(key.to_string(), v);
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
#[derive(Debug, Clone, PartialEq)]
pub struct Deck {
    root: Map<String, Value>,
    pub slides: Vec<Slide>,
    pub title: String,
    pub theme: Theme,
    /// The server's metadata (`presentation`), kept for the backstage (aspect ratio, dates…).
    pub meta: Value,
}

impl Default for Deck {
    fn default() -> Self {
        Deck { root: Map::new(), slides: Vec::new(), title: String::new(), theme: Theme::default(), meta: Value::Null }
    }
}

/// gzip's signature: the content file is gzipped, an older or exported one may be plain.
pub fn is_gzip(bytes: &[u8]) -> bool {
    bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b
}

impl Deck {
    /// From the server: `GET /presentations/:id` (`{presentation, slides:[summary]}`) and the content (`join`'s
    /// `content`, or the `.kbsld` JSON). Slides are in the server's order; a slide missing from the content gets
    /// `empty_slide_data()` like `cf::get_slide_data`; content without a row (deleted elsewhere) is dropped.
    pub fn from_server(meta: &Value, content: Value) -> Deck {
        let pres = meta.get("presentation").cloned().unwrap_or(Value::Null);
        let mut root = match content {
            Value::Object(m) => m,
            _ => Map::new(),
        };
        let mut stored = match root.get_mut("slides") {
            Some(Value::Object(m)) => std::mem::take(m),
            _ => Map::new(),
        };
        let mut summaries: Vec<&Value> = meta.get("slides").and_then(Value::as_array).map(|a| a.iter().collect()).unwrap_or_default();
        summaries.sort_by(|a, b| as_f64(a.get("position")).unwrap_or(0.0).total_cmp(&as_f64(b.get("position")).unwrap_or(0.0)));
        let slides = summaries
            .iter()
            .filter_map(|s| {
                let id = s.get("id")?.as_str()?.to_string();
                let data = stored.shift_remove(&id).map(SlideData::from_value).unwrap_or_else(SlideData::empty);
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

    /// From a content file alone (`.kbsld`, gzip or plain JSON): the slides in the file's key order, all visible.
    /// A desktop bundle (`{presentation, slides, content}`, what [`Deck::to_bundle`] writes) keeps its order,
    /// hidden flags, title and theme.
    pub fn from_json(bytes: &[u8]) -> Result<Deck, LoadError> {
        let v: Value = serde_json::from_slice(bytes).map_err(|e| LoadError::NotJson(e.to_string()))?;
        if let Some(content) = v.get("content").cloned().filter(|c| c.get("slides").is_some()) {
            return Ok(Deck::from_server(&v, content));
        }
        let ids: Vec<Value> = match v.get("slides") {
            Some(Value::Object(m)) => m.keys().enumerate().map(|(i, k)| serde_json::json!({ "id": k, "position": i, "is_hidden": false })).collect(),
            _ => return Err(LoadError::NotAPresentation),
        };
        Ok(Deck::from_server(&serde_json::json!({ "slides": ids }), v))
    }

    /// The content file's JSON (`{"version":1,"slides":{…}}`), slides in deck order, root keys kept.
    pub fn content_value(&self) -> Value {
        let mut root = self.root.clone();
        if !root.contains_key("version") {
            root.insert("version".into(), Value::from(1));
        }
        let slides: Map<String, Value> = self.slides.iter().map(|s| (s.id.clone(), s.data.to_value())).collect();
        root.insert("slides".into(), Value::Object(slides));
        Value::Object(root)
    }

    /// A self-contained local copy: the content plus what the server keeps in its tables (order, hidden flags,
    /// title, theme) — what the desktop writes for a local file and its crash journal.
    pub fn to_bundle(&self) -> Value {
        let mut pres = match &self.meta {
            Value::Object(m) => m.clone(),
            _ => Map::new(),
        };
        pres.insert("title".into(), Value::String(self.title.clone()));
        let slides: Vec<Value> = self.slides.iter().enumerate().map(|(i, s)| serde_json::json!({ "id": s.id, "position": i, "is_hidden": s.hidden })).collect();
        serde_json::json!({ "presentation": Value::Object(pres), "slides": slides, "content": self.content_value() })
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
    fn an_untouched_slide_round_trips_byte_for_byte() {
        let src = r##"{"version":1,"slides":{"b":{"notes":"n","elements":[{"id":"e1","type":"text","x":0.08,"y":0.3,"w":0.84,"h":0.28,"rotation":0,"zIndex":1,"future":{"k":[1,2.5]}}],"background":{"type":"color","color":"#fff"},"transition":{"type":"fade","duration":500},"extra":true},"a":{"elements":[]}},"other":"kept"}"##;
        let deck = Deck::from_json(src.as_bytes()).expect("deck");
        assert_eq!(deck.slides.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["b", "a"]);
        assert_eq!(serde_json::to_string(&deck.content_value()).ok().as_deref(), Some(src));
    }

    #[test]
    fn the_server_order_and_hidden_flags_win() {
        let meta = json!({ "presentation": { "title": "Deck", "theme": { "bgColor": "#000" } },
                           "slides": [ { "id": "b", "position": 1, "is_hidden": true }, { "id": "a", "position": 0 }, { "id": "c", "position": 2 } ] });
        let deck = Deck::from_server(&meta, json!({ "version": 1, "slides": { "a": { "elements": [] }, "b": { "elements": [] }, "gone": {} } }));
        assert_eq!(deck.slides.iter().map(|s| (s.id.as_str(), s.hidden)).collect::<Vec<_>>(), [("a", false), ("b", true), ("c", false)]);
        assert_eq!(deck.slides[2].data, SlideData::empty());
        assert_eq!(deck.title, "Deck");
        assert_eq!(deck.theme.bg_color, "#000");
        assert_eq!(deck.theme.text_color, "#202124");
        let again = Deck::from_json(serde_json::to_vec(&deck.to_bundle()).unwrap_or_default().as_slice()).expect("bundle");
        assert_eq!(again.slides, deck.slides);
    }

    #[test]
    fn a_setter_changes_one_key_in_place() {
        let mut e = Element::from_value(json!({ "id": "x", "x": 0.1, "y": 0.2, "type": "shape" })).expect("el");
        e.set_f("x", 0.5);
        e.remove("y");
        assert_eq!(serde_json::to_string(&e.to_value()).ok().as_deref(), Some(r#"{"id":"x","x":0.5,"type":"shape"}"#));
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
    }
}
