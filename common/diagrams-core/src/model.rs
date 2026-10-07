//! A diagram page, held losslessly — the `data` of `PUT /office/diagrams/:id/pages/:pid/data` and of
//! each page of the `.kbdia` file.
//!
//! The web keeps a page as plain JavaScript objects (`DiagramEditorPage.tsx:49-123`) and edits them with
//! spreads (`{ ...s, x: nx }`): a key it does not know is carried along, a key that was never set stays
//! absent (a missing `style.strokeWidth` is not the same as `1.5`: `mergeStyle` fills it in at drawing
//! time), and `groupId: null` (after « Dégrouper ») is not an absent `groupId`. So each object here IS
//! its JSON map, read and written through typed accessors, and numbers are written the way
//! `JSON.stringify` writes them (`120`, never `120.0`; `NaN` becomes `null`).

use serde_json::{Map, Value};

use crate::canvas::Point;

/// A JSON object.
pub type Obj = Map<String, Value>;

/// A number as `JSON.stringify` writes it: integers without a fraction, non-finite values as `null`.
pub fn js_number(v: f64) -> Value {
    if !v.is_finite() {
        return Value::Null;
    }
    if v.fract() == 0.0 && v.abs() < 9_007_199_254_740_992.0 {
        // `-0` is written `0` by JSON.stringify.
        return Value::from(v as i64);
    }
    serde_json::Number::from_f64(v).map(Value::Number).unwrap_or(Value::Null)
}

/// A JSON value read as a JavaScript number would be used by the web code: numbers as they are,
/// anything else `None`.
pub fn num(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64)
}

fn str_of(v: Option<&Value>) -> Option<&str> {
    v.and_then(Value::as_str)
}

fn point_of(v: Option<&Value>) -> Option<Point> {
    let o = v?.as_object()?;
    Some(Point::new(num(o.get("x")).unwrap_or(0.0), num(o.get("y")).unwrap_or(0.0)))
}

/// `{ x, y }`.
pub fn point_value(p: Point) -> Value {
    let mut o = Obj::new();
    o.insert("x".into(), js_number(p.x));
    o.insert("y".into(), js_number(p.y));
    Value::Object(o)
}

// ── Styles ───────────────────────────────────────────────────────────────────

/// `ShapeStyle` merged with its defaults (`mergeStyle`, `stencils.ts:28-40`).
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeStyle {
    pub fill_color: String,
    pub stroke_color: String,
    pub stroke_width: f64,
    /// `solid`, `dashed` or `dotted`.
    pub stroke_style: String,
    /// 0–100.
    pub opacity: f64,
    pub shadow: bool,
    pub rounded: f64,
}

impl Default for ShapeStyle {
    /// `DEF_STYLE`.
    fn default() -> Self {
        Self {
            fill_color: "#dae8fc".into(),
            stroke_color: "#6c8ebf".into(),
            stroke_width: 1.5,
            stroke_style: "solid".into(),
            opacity: 100.0,
            shadow: false,
            rounded: 0.0,
        }
    }
}

impl ShapeStyle {
    /// `mergeStyle(partial)`: the partial style's keys over the defaults.
    pub fn merge(partial: &Obj) -> ShapeStyle {
        let d = ShapeStyle::default();
        ShapeStyle {
            fill_color: str_of(partial.get("fillColor")).map(str::to_string).unwrap_or(d.fill_color),
            stroke_color: str_of(partial.get("strokeColor")).map(str::to_string).unwrap_or(d.stroke_color),
            stroke_width: num(partial.get("strokeWidth")).unwrap_or(d.stroke_width),
            stroke_style: str_of(partial.get("strokeStyle")).map(str::to_string).unwrap_or(d.stroke_style),
            opacity: num(partial.get("opacity")).unwrap_or(d.opacity),
            shadow: partial.get("shadow").and_then(Value::as_bool).unwrap_or(d.shadow),
            rounded: num(partial.get("rounded")).unwrap_or(d.rounded),
        }
    }

    /// The canvas dash of a stroke style (`[6, 3]` dashed, `[2, 3]` dotted, solid otherwise).
    pub fn dash(stroke_style: &str) -> &'static [f64] {
        match stroke_style {
            "dashed" => &[6.0, 3.0],
            "dotted" => &[2.0, 3.0],
            _ => &[],
        }
    }
}

/// `LabelStyle` merged with `DEFAULT_LABEL_STYLE` (`DiagramEditorPage.tsx:136-139`).
#[derive(Debug, Clone, PartialEq)]
pub struct LabelStyle {
    pub font_family: String,
    pub font_size: f64,
    pub bold: bool,
    pub italic: bool,
    pub color: String,
    /// `left`, `center` or `right`.
    pub align: String,
    /// `top`, `middle` or `bottom`.
    pub vertical_align: String,
}

impl Default for LabelStyle {
    fn default() -> Self {
        Self {
            font_family: "Inter".into(),
            font_size: 12.0,
            bold: false,
            italic: false,
            color: "#000000".into(),
            align: "center".into(),
            vertical_align: "middle".into(),
        }
    }
}

impl LabelStyle {
    /// `{ ...DEFAULT_LABEL_STYLE, ...partial }`.
    pub fn merge(partial: &Obj) -> LabelStyle {
        let d = LabelStyle::default();
        LabelStyle {
            font_family: str_of(partial.get("fontFamily")).map(str::to_string).unwrap_or(d.font_family),
            font_size: num(partial.get("fontSize")).unwrap_or(d.font_size),
            bold: partial.get("bold").and_then(Value::as_bool).unwrap_or(d.bold),
            italic: partial.get("italic").and_then(Value::as_bool).unwrap_or(d.italic),
            color: str_of(partial.get("color")).map(str::to_string).unwrap_or(d.color),
            align: str_of(partial.get("align")).map(str::to_string).unwrap_or(d.align),
            vertical_align: str_of(partial.get("verticalAlign")).map(str::to_string).unwrap_or(d.vertical_align),
        }
    }
}

/// `ConnectorStyle`, as the web reads it off the stored object (`DEFAULT_CONN_STYLE` for what is
/// missing, which only an imported or hand-written connector can lack).
#[derive(Debug, Clone, PartialEq)]
pub struct ConnStyle {
    pub stroke_color: String,
    pub stroke_width: f64,
    pub stroke_style: String,
    pub arrow_start: String,
    pub arrow_end: String,
    pub orthogonal: bool,
    /// `routing ?? (orthogonal ? 'orthogonal' : 'straight')` (`connRouting`).
    pub routing: Routing,
}

/// How a connector is routed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Routing {
    #[default]
    Straight,
    Orthogonal,
    Curved,
}

impl Routing {
    pub fn as_str(self) -> &'static str {
        match self {
            Routing::Straight => "straight",
            Routing::Orthogonal => "orthogonal",
            Routing::Curved => "curved",
        }
    }

    pub fn parse(s: &str) -> Option<Routing> {
        Some(match s {
            "straight" => Routing::Straight,
            "orthogonal" => Routing::Orthogonal,
            "curved" => Routing::Curved,
            _ => return None,
        })
    }
}

impl ConnStyle {
    pub fn read(o: &Obj) -> ConnStyle {
        let orthogonal = o.get("orthogonal").and_then(Value::as_bool).unwrap_or(false);
        let routing = str_of(o.get("routing")).and_then(Routing::parse).unwrap_or(if orthogonal { Routing::Orthogonal } else { Routing::Straight });
        ConnStyle {
            stroke_color: str_of(o.get("strokeColor")).unwrap_or("#6c8ebf").to_string(),
            stroke_width: num(o.get("strokeWidth")).unwrap_or(1.5),
            stroke_style: str_of(o.get("strokeStyle")).unwrap_or("solid").to_string(),
            arrow_start: str_of(o.get("arrowStart")).unwrap_or("none").to_string(),
            arrow_end: str_of(o.get("arrowEnd")).unwrap_or("block").to_string(),
            orthogonal,
            routing,
        }
    }

    /// `DEFAULT_CONN_STYLE` (`DiagramEditorPage.tsx:140-143`), as an object.
    pub fn default_object() -> Obj {
        let mut o = Obj::new();
        o.insert("strokeColor".into(), "#6c8ebf".into());
        o.insert("strokeWidth".into(), js_number(1.5));
        o.insert("strokeStyle".into(), "solid".into());
        o.insert("arrowStart".into(), "none".into());
        o.insert("arrowEnd".into(), "block".into());
        o.insert("orthogonal".into(), false.into());
        o.insert("routing".into(), "straight".into());
        o
    }
}

// ── Shapes, connectors, layers ───────────────────────────────────────────────

/// Accessors shared by the three object kinds.
macro_rules! object_wrapper {
    ($name:ident) => {
        impl $name {
            /// The JSON object, as stored.
            pub fn obj(&self) -> &Obj {
                &self.0
            }

            pub fn obj_mut(&mut self) -> &mut Obj {
                &mut self.0
            }

            pub fn into_obj(self) -> Obj {
                self.0
            }

            pub fn id(&self) -> &str {
                str_of(self.0.get("id")).unwrap_or("")
            }

            pub fn set_id(&mut self, id: &str) {
                self.0.insert("id".into(), id.into());
            }

            /// The layer (`layerId`), when set.
            pub fn layer_id(&self) -> Option<&str> {
                str_of(self.0.get("layerId"))
            }

            pub fn set_layer_id(&mut self, id: &str) {
                self.0.insert("layerId".into(), id.into());
            }

            pub fn label(&self) -> &str {
                str_of(self.0.get("label")).unwrap_or("")
            }

            pub fn set_label(&mut self, label: &str) {
                self.0.insert("label".into(), label.into());
            }

            /// Sets a number the way the web writes it.
            pub fn set_num(&mut self, key: &str, v: f64) {
                self.0.insert(key.into(), js_number(v));
            }
        }
    };
}

/// A shape (`DiagramShape`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Shape(pub Obj);
object_wrapper!(Shape);

impl Shape {
    /// The stencil id (`type`).
    pub fn kind(&self) -> &str {
        str_of(self.0.get("type")).unwrap_or("")
    }

    pub fn x(&self) -> f64 {
        num(self.0.get("x")).unwrap_or(0.0)
    }

    pub fn y(&self) -> f64 {
        num(self.0.get("y")).unwrap_or(0.0)
    }

    pub fn w(&self) -> f64 {
        num(self.0.get("w")).unwrap_or(0.0)
    }

    pub fn h(&self) -> f64 {
        num(self.0.get("h")).unwrap_or(0.0)
    }

    pub fn set_rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        self.set_num("x", x);
        self.set_num("y", y);
        self.set_num("w", w);
        self.set_num("h", h);
    }

    pub fn set_pos(&mut self, x: f64, y: f64) {
        self.set_num("x", x);
        self.set_num("y", y);
    }

    pub fn center(&self) -> Point {
        Point::new(self.x() + self.w() / 2.0, self.y() + self.h() / 2.0)
    }

    /// The partial style object, as stored (`{}` when missing).
    pub fn style_obj(&self) -> Obj {
        self.0.get("style").and_then(Value::as_object).cloned().unwrap_or_default()
    }

    /// `mergeStyle(shape.style)`.
    pub fn style(&self) -> ShapeStyle {
        ShapeStyle::merge(self.0.get("style").and_then(Value::as_object).unwrap_or(&Obj::new()))
    }

    /// `{ ...s.style, ...patch }`.
    pub fn patch_style(&mut self, patch: &Obj) {
        let mut st = self.style_obj();
        for (k, v) in patch {
            st.insert(k.clone(), v.clone());
        }
        self.0.insert("style".into(), Value::Object(st));
    }

    /// Replaces the style object (the « Modifier le style… » dialog).
    pub fn set_style_obj(&mut self, st: Obj) {
        self.0.insert("style".into(), Value::Object(st));
    }

    pub fn label_style_obj(&self) -> Obj {
        self.0.get("labelStyle").and_then(Value::as_object).cloned().unwrap_or_default()
    }

    /// `{ ...DEFAULT_LABEL_STYLE, ...shape.labelStyle }`.
    pub fn label_style(&self) -> LabelStyle {
        LabelStyle::merge(self.0.get("labelStyle").and_then(Value::as_object).unwrap_or(&Obj::new()))
    }

    pub fn patch_label_style(&mut self, patch: &Obj) {
        let mut st = self.label_style_obj();
        for (k, v) in patch {
            st.insert(k.clone(), v.clone());
        }
        self.0.insert("labelStyle".into(), Value::Object(st));
    }

    pub fn z_index(&self) -> f64 {
        num(self.0.get("zIndex")).unwrap_or(0.0)
    }

    pub fn flip_h(&self) -> bool {
        self.0.get("flipH").and_then(Value::as_bool).unwrap_or(false)
    }

    pub fn flip_v(&self) -> bool {
        self.0.get("flipV").and_then(Value::as_bool).unwrap_or(false)
    }

    pub fn set_flip_h(&mut self, v: bool) {
        self.0.insert("flipH".into(), v.into());
    }

    pub fn set_flip_v(&mut self, v: bool) {
        self.0.insert("flipV".into(), v.into());
    }

    /// Degrees; 0 when missing.
    pub fn rotation(&self) -> f64 {
        num(self.0.get("rotation")).unwrap_or(0.0)
    }

    pub fn set_rotation(&mut self, deg: f64) {
        self.set_num("rotation", deg);
    }

    /// The group, `None` when absent or `null`.
    pub fn group_id(&self) -> Option<&str> {
        str_of(self.0.get("groupId"))
    }

    /// Sets the group; `None` writes `null`, as « Dégrouper » does.
    pub fn set_group_id(&mut self, gid: Option<&str>) {
        self.0.insert("groupId".into(), gid.map(Value::from).unwrap_or(Value::Null));
    }

    /// The adjustment values (yellow knobs), when set.
    pub fn adj(&self) -> Option<Vec<f64>> {
        let a = self.0.get("adj")?.as_array()?;
        Some(a.iter().map(|v| v.as_f64().unwrap_or(f64::NAN)).collect())
    }

    pub fn set_adj(&mut self, adj: &[f64]) {
        self.0.insert("adj".into(), Value::Array(adj.iter().map(|v| js_number(*v)).collect()));
    }

    /// A new shape with the fields the web writes for a placed stencil (`placeStencilBox`).
    #[allow(clippy::too_many_arguments)]
    pub fn new(id: &str, kind: &str, x: f64, y: f64, w: f64, h: f64, label: &str, style: Obj, z_index: f64, layer_id: &str) -> Shape {
        let mut o = Obj::new();
        o.insert("id".into(), id.into());
        o.insert("type".into(), kind.into());
        o.insert("x".into(), js_number(x));
        o.insert("y".into(), js_number(y));
        o.insert("w".into(), js_number(w));
        o.insert("h".into(), js_number(h));
        o.insert("label".into(), label.into());
        o.insert("style".into(), Value::Object(style));
        o.insert("labelStyle".into(), Value::Object(Obj::new()));
        o.insert("zIndex".into(), js_number(z_index));
        o.insert("layerId".into(), layer_id.into());
        Shape(o)
    }
}

/// A connector (`DiagramConnector`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Connector(pub Obj);
object_wrapper!(Connector);

impl Connector {
    pub fn source_id(&self) -> Option<&str> {
        str_of(self.0.get("sourceId"))
    }

    pub fn target_id(&self) -> Option<&str> {
        str_of(self.0.get("targetId"))
    }

    pub fn source_point(&self) -> Option<Point> {
        point_of(self.0.get("sourcePoint"))
    }

    pub fn target_point(&self) -> Option<Point> {
        point_of(self.0.get("targetPoint"))
    }

    /// `waypoints ?? []`.
    pub fn waypoints(&self) -> Vec<Point> {
        self.0
            .get("waypoints")
            .and_then(Value::as_array)
            .map(|a| a.iter().map(|p| point_of(Some(p)).unwrap_or_default()).collect())
            .unwrap_or_default()
    }

    pub fn set_waypoints(&mut self, pts: &[Point]) {
        self.0.insert("waypoints".into(), Value::Array(pts.iter().map(|p| point_value(*p)).collect()));
    }

    /// `labelOffset ?? { x: 0, y: 0 }` (`null` included).
    pub fn label_offset(&self) -> Point {
        point_of(self.0.get("labelOffset")).unwrap_or_default()
    }

    pub fn set_label_offset(&mut self, p: Point) {
        self.0.insert("labelOffset".into(), point_value(p));
    }

    pub fn style_obj(&self) -> Obj {
        self.0.get("style").and_then(Value::as_object).cloned().unwrap_or_default()
    }

    pub fn style(&self) -> ConnStyle {
        ConnStyle::read(self.0.get("style").and_then(Value::as_object).unwrap_or(&Obj::new()))
    }

    pub fn patch_style(&mut self, patch: &Obj) {
        let mut st = self.style_obj();
        for (k, v) in patch {
            st.insert(k.clone(), v.clone());
        }
        self.0.insert("style".into(), Value::Object(st));
    }

    /// Swaps the ends and reverses the waypoints (`connReverse`).
    pub fn reverse(&mut self) {
        let take = |o: &mut Obj, k: &str| o.remove(k).unwrap_or(Value::Null);
        let (s, t) = (take(&mut self.0, "sourceId"), take(&mut self.0, "targetId"));
        let (sp, tp) = (take(&mut self.0, "sourcePoint"), take(&mut self.0, "targetPoint"));
        self.0.insert("sourceId".into(), t);
        self.0.insert("targetId".into(), s);
        self.0.insert("sourcePoint".into(), tp);
        self.0.insert("targetPoint".into(), sp);
        let mut w = self.waypoints();
        w.reverse();
        self.set_waypoints(&w);
    }

    /// A connector between two shapes with the default style (`handleMouseUp`, `:2166-2176`).
    pub fn between(id: &str, source: &str, target: &str, layer_id: &str) -> Connector {
        let mut o = Obj::new();
        o.insert("id".into(), id.into());
        o.insert("sourceId".into(), source.into());
        o.insert("targetId".into(), target.into());
        o.insert("sourcePoint".into(), Value::Null);
        o.insert("targetPoint".into(), Value::Null);
        o.insert("waypoints".into(), Value::Array(Vec::new()));
        o.insert("label".into(), "".into());
        o.insert("style".into(), Value::Object(ConnStyle::default_object()));
        o.insert("layerId".into(), layer_id.into());
        Connector(o)
    }
}

/// A layer (`LayerDef`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Layer(pub Obj);
object_wrapper!(Layer);

impl Layer {
    pub fn name(&self) -> &str {
        str_of(self.0.get("name")).unwrap_or("")
    }

    pub fn set_name(&mut self, name: &str) {
        self.0.insert("name".into(), name.into());
    }

    pub fn visible(&self) -> bool {
        self.0.get("visible").and_then(Value::as_bool).unwrap_or(true)
    }

    pub fn locked(&self) -> bool {
        self.0.get("locked").and_then(Value::as_bool).unwrap_or(false)
    }

    pub fn set_visible(&mut self, v: bool) {
        self.0.insert("visible".into(), v.into());
    }

    pub fn set_locked(&mut self, v: bool) {
        self.0.insert("locked".into(), v.into());
    }

    pub fn new(id: &str, name: &str) -> Layer {
        let mut o = Obj::new();
        o.insert("id".into(), id.into());
        o.insert("name".into(), name.into());
        o.insert("visible".into(), true.into());
        o.insert("locked".into(), false.into());
        Layer(o)
    }
}

/// The id of the implicit first layer (`DEFAULT_LAYER_ID`).
pub const DEFAULT_LAYER_ID: &str = "default";

// ── A page ───────────────────────────────────────────────────────────────────

/// The data of one page (`DiagramData`): shapes, connectors, the optional layers, and every other key
/// it held.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PageData {
    pub shapes: Vec<Shape>,
    pub connectors: Vec<Connector>,
    /// `None` when the page never had layers (one implicit « Calque 1 »).
    pub layers: Option<Vec<Layer>>,
    /// Keys other than the three above, kept as they were.
    pub rest: Obj,
}

fn objects<T>(v: Option<&Value>, wrap: fn(Obj) -> T) -> Vec<T> {
    v.and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|o| o.as_object().cloned().map(wrap)).collect())
        .unwrap_or_default()
}

impl PageData {
    /// Reads a page's data (`raw?.shapes ?? []`, `raw?.connectors ?? []`; anything that is not an object
    /// reads as an empty page).
    pub fn from_value(v: &Value) -> PageData {
        let Some(o) = v.as_object() else {
            return PageData::default();
        };
        let mut rest = o.clone();
        rest.remove("shapes");
        rest.remove("connectors");
        let layers = o.get("layers").and_then(Value::as_array).map(|_| objects(o.get("layers"), Layer));
        rest.remove("layers");
        PageData { shapes: objects(o.get("shapes"), Shape), connectors: objects(o.get("connectors"), Connector), layers, rest }
    }

    /// The page as the web writes it: `shapes`, `connectors`, then `layers` when there are some, then the
    /// other keys.
    pub fn to_value(&self) -> Value {
        let mut o = Obj::new();
        o.insert("shapes".into(), Value::Array(self.shapes.iter().map(|s| Value::Object(s.0.clone())).collect()));
        o.insert("connectors".into(), Value::Array(self.connectors.iter().map(|c| Value::Object(c.0.clone())).collect()));
        if let Some(layers) = &self.layers {
            o.insert("layers".into(), Value::Array(layers.iter().map(|l| Value::Object(l.0.clone())).collect()));
        }
        for (k, v) in &self.rest {
            o.entry(k.clone()).or_insert_with(|| v.clone());
        }
        Value::Object(o)
    }

    /// `getLayers(d)`: the page's layers, or the implicit « Calque 1 ».
    pub fn layers(&self) -> Vec<Layer> {
        match &self.layers {
            Some(l) if !l.is_empty() => l.clone(),
            _ => vec![Layer::new(DEFAULT_LAYER_ID, "Calque 1")],
        }
    }

    pub fn shape(&self, id: &str) -> Option<&Shape> {
        self.shapes.iter().find(|s| s.id() == id)
    }

    pub fn shape_mut(&mut self, id: &str) -> Option<&mut Shape> {
        self.shapes.iter_mut().find(|s| s.id() == id)
    }

    pub fn connector(&self, id: &str) -> Option<&Connector> {
        self.connectors.iter().find(|c| c.id() == id)
    }

    pub fn connector_mut(&mut self, id: &str) -> Option<&mut Connector> {
        self.connectors.iter_mut().find(|c| c.id() == id)
    }

    /// The layer an object belongs to: its `layerId`, else the first layer (`layerOf`).
    pub fn layer_of<'a>(&'a self, layer_id: Option<&'a str>, layers: &'a [Layer]) -> &'a str {
        layer_id.unwrap_or_else(|| layers.first().map(|l| l.id()).unwrap_or(DEFAULT_LAYER_ID))
    }
}

// ── Ids ──────────────────────────────────────────────────────────────────────

/// The web's `makeId()`: eight base-36 characters (`Math.random().toString(36).slice(2, 10)`). Seeded by
/// the caller (the core never reads a clock or an entropy source).
#[derive(Debug, Clone)]
pub struct IdSource {
    state: u64,
}

impl IdSource {
    pub fn new(seed: u64) -> Self {
        Self { state: seed ^ 0x9E37_79B9_7F4A_7C15 | 1 }
    }

    fn next_u64(&mut self) -> u64 {
        // xorshift64*.
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub fn make_id(&mut self) -> String {
        const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
        let mut n = self.next_u64();
        (0..8)
            .map(|_| {
                let d = DIGITS[(n % 36) as usize] as char;
                n /= 36;
                d
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn numbers_are_written_like_json_stringify() {
        assert_eq!(js_number(120.0), json!(120));
        assert_eq!(js_number(1.5), json!(1.5));
        assert_eq!(js_number(-0.0), json!(0));
        assert_eq!(js_number(f64::NAN), Value::Null);
        assert_eq!(serde_json::to_string(&js_number(120.0)).expect("json"), "120");
    }

    #[test]
    fn a_page_round_trips_with_its_unknown_keys_and_partial_styles() {
        let v = json!({
            "shapes": [{ "id": "a1", "type": "rect", "x": 10, "y": 20.5, "w": 120, "h": 60, "label": "A",
                         "style": { "fillColor": "#dae8fc" }, "labelStyle": {}, "zIndex": 0, "groupId": null,
                         "future": { "k": [1, 2] } }],
            "connectors": [],
            "extra": true
        });
        let p = PageData::from_value(&v);
        assert_eq!(p.to_value(), v);
        let s = &p.shapes[0];
        assert_eq!(s.style().stroke_width, 1.5, "merged with the defaults at read time");
        assert!(!s.style_obj().contains_key("strokeWidth"), "the stored style stays partial");
        assert_eq!(s.group_id(), None);
        assert_eq!(p.layers().len(), 1);
    }

    #[test]
    fn setters_write_what_the_web_writes() {
        let mut s = Shape::new("x", "rect", 1.0, 2.0, 3.0, 4.0, "L", Obj::new(), 0.0, "default");
        s.set_pos(10.0, 12.25);
        s.set_group_id(None);
        let v = Value::Object(s.0.clone());
        assert_eq!(v["x"], json!(10));
        assert_eq!(v["y"], json!(12.25));
        assert_eq!(v["groupId"], Value::Null);
        let mut c = Connector::between("c", "a", "b", "default");
        c.set_waypoints(&[Point::new(1.0, 2.0), Point::new(3.0, 4.0)]);
        c.reverse();
        assert_eq!(c.source_id(), Some("b"));
        assert_eq!(c.waypoints(), vec![Point::new(3.0, 4.0), Point::new(1.0, 2.0)]);
        assert_eq!(c.style().routing, Routing::Straight);
    }

    #[test]
    fn ids_are_eight_base36_characters() {
        let mut ids = IdSource::new(42);
        let a = ids.make_id();
        let b = ids.make_id();
        assert_eq!(a.len(), 8);
        assert!(a.chars().all(|c| c.is_ascii_digit() || c.is_ascii_lowercase()));
        assert_ne!(a, b);
    }
}
