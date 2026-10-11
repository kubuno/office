//! The whiteboard scene, held losslessly — a port of `office/web/src/whiteboard-types.ts`.
//!
//! The authoritative store is the Yjs document (`collab`): an `elements` map keyed by element id and a
//! `strokes` array, each entry a plain JSON object exactly as the web writes it
//! (`JSON.parse(JSON.stringify(el))`). These typed structs are a *view* over that JSON for the engine and
//! the renderer; every one carries an `extra` catch-all so a field the desktop does not model yet (or a
//! whole element kind it does not know) survives a round trip unchanged. String unions are kept as `String`
//! rather than Rust enums for the same reason: the web preserves `ShapeKind` legacy names as stored, so the
//! desktop must not reject or rewrite a value it has not seen.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// A board's canvas background (`Background`, `whiteboard-types.ts:26`). Stored as a board-metadata column,
/// never inside the content document.
pub type Background = String;

/// The whole scene the engine works on: every element and freehand stroke, plus the background.
///
/// This is a *derived* view of [`crate::collab::WhiteboardDoc`]; it is not itself the stored format.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scene {
    pub elements: Vec<WbElement>,
    pub strokes: Vec<Stroke>,
    pub background: Background,
}

impl Scene {
    /// The elements in paint order: ascending `z_index`, ties keeping input order (a stable sort, like the
    /// web's `[...elements].sort((a,b)=>a.zIndex-b.zIndex)`).
    pub fn z_ordered(&self) -> Vec<&WbElement> {
        let mut v: Vec<&WbElement> = self.elements.iter().collect();
        v.sort_by(|a, b| a.z_index().partial_cmp(&b.z_index()).unwrap_or(std::cmp::Ordering::Equal));
        v
    }

    /// The highest `z_index` in the scene (elements and arrows), or `-1` when empty — the base a newly
    /// created element sits above.
    pub fn max_z(&self) -> f64 {
        self.elements.iter().map(|e| e.z_index()).fold(-1.0, f64::max)
    }
}

/// One element, discriminated by its `type` field, exactly as stored.
///
/// `Arrow` deliberately does not extend [`BaseElement`]: an arrow stores its own `startX/startY/endX/endY`
/// and has no `x/y/width/height/rotation/locked` (`whiteboard-types.ts:77-96`). An element kind the desktop
/// does not model stays as raw JSON in [`crate::collab::WhiteboardDoc`] (never surfaced here), so it is
/// never lost on save.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WbElement {
    #[serde(rename = "sticky")]
    Sticky(StickyNote),
    #[serde(rename = "text")]
    Text(TextBox),
    #[serde(rename = "shape")]
    Shape(ShapeElement),
    #[serde(rename = "arrow")]
    Arrow(ArrowElement),
    #[serde(rename = "frame")]
    Frame(FrameElement),
    #[serde(rename = "image")]
    Image(ImageElement),
}

/// The geometry shared by every element but the arrow (`BaseElement`, `whiteboard-types.ts:28-42`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaseElement {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub rotation: f64,
    #[serde(default = "one")]
    pub opacity: f64,
    #[serde(rename = "zIndex", default)]
    pub z_index: f64,
    #[serde(default)]
    pub locked: bool,
    /// Group membership: every element sharing a `groupId` moves and deletes as one unit.
    #[serde(rename = "groupId", skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
}

fn one() -> f64 {
    1.0
}

/// `StickyNote` (`whiteboard-types.ts:44-50`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StickyNote {
    #[serde(flatten)]
    pub base: BaseElement,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub color: String,
    #[serde(rename = "fontSize", default = "font_default")]
    pub font_size: f64,
    #[serde(rename = "textAlign", default)]
    pub text_align: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `TextBox` (`whiteboard-types.ts:52-59`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextBox {
    #[serde(flatten)]
    pub base: BaseElement,
    #[serde(default)]
    pub text: String,
    #[serde(rename = "fontSize", default = "font_default")]
    pub font_size: f64,
    #[serde(rename = "fontWeight", default)]
    pub font_weight: String,
    #[serde(default)]
    pub color: String,
    #[serde(rename = "textAlign", default)]
    pub text_align: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn font_default() -> f64 {
    16.0
}

/// `ShapeElement` (`whiteboard-types.ts:61-75`). `kind` is kept as the stored string (catalogue kind or a
/// legacy name), resolved to geometry through `kubuno-office-shapes-core`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShapeElement {
    #[serde(flatten)]
    pub base: BaseElement,
    pub kind: String,
    #[serde(default)]
    pub fill: String,
    #[serde(default)]
    pub stroke: String,
    #[serde(rename = "strokeWidth", default = "two")]
    pub stroke_width: f64,
    /// Yellow-knob adjustment values (OOXML `avLst`); absent = the geometry's defaults.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adj: Option<Vec<f64>>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn two() -> f64 {
    2.0
}

/// `ArrowElement` (`whiteboard-types.ts:77-96`) — flat, no [`BaseElement`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArrowElement {
    pub id: String,
    #[serde(rename = "startX")]
    pub start_x: f64,
    #[serde(rename = "startY")]
    pub start_y: f64,
    #[serde(rename = "endX")]
    pub end_x: f64,
    #[serde(rename = "endY")]
    pub end_y: f64,
    #[serde(rename = "startElementId", skip_serializing_if = "Option::is_none")]
    pub start_element_id: Option<String>,
    #[serde(rename = "startSide", skip_serializing_if = "Option::is_none")]
    pub start_side: Option<String>,
    #[serde(rename = "endElementId", skip_serializing_if = "Option::is_none")]
    pub end_element_id: Option<String>,
    #[serde(rename = "endSide", skip_serializing_if = "Option::is_none")]
    pub end_side: Option<String>,
    #[serde(default)]
    pub color: String,
    #[serde(default = "two")]
    pub width: f64,
    #[serde(default)]
    pub style: String,
    #[serde(rename = "startArrow", default)]
    pub start_arrow: String,
    #[serde(rename = "endArrow", default)]
    pub end_arrow: String,
    #[serde(rename = "zIndex", default)]
    pub z_index: f64,
    #[serde(default = "one")]
    pub opacity: f64,
    #[serde(rename = "groupId", skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `FrameElement` (`whiteboard-types.ts:98-102`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameElement {
    #[serde(flatten)]
    pub base: BaseElement,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub color: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `ImageElement` (`whiteboard-types.ts:104-109`). Note the snake_case `natural_width`/`natural_height`
/// written by the web, kept verbatim.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageElement {
    #[serde(flatten)]
    pub base: BaseElement,
    #[serde(default)]
    pub src: String,
    #[serde(default)]
    pub natural_width: f64,
    #[serde(default)]
    pub natural_height: f64,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A freehand stroke (`Stroke`, `whiteboard-types.ts:113-122`), stored in the `strokes` array.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub id: String,
    /// Flat point array `[x0,y0, x1,y1, …]`.
    pub points: Vec<f64>,
    #[serde(default)]
    pub color: String,
    #[serde(default = "two")]
    pub width: f64,
    #[serde(default = "one")]
    pub opacity: f64,
    #[serde(default)]
    pub tool: String,
    #[serde(rename = "userId", default)]
    pub user_id: String,
    #[serde(rename = "createdAt", default)]
    pub created_at: f64,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl WbElement {
    /// The element's id, whatever its kind.
    pub fn id(&self) -> &str {
        match self {
            WbElement::Sticky(e) => &e.base.id,
            WbElement::Text(e) => &e.base.id,
            WbElement::Shape(e) => &e.base.id,
            WbElement::Arrow(e) => &e.id,
            WbElement::Frame(e) => &e.base.id,
            WbElement::Image(e) => &e.base.id,
        }
    }

    /// The element's paint order key (`zIndex`).
    pub fn z_index(&self) -> f64 {
        match self {
            WbElement::Sticky(e) => e.base.z_index,
            WbElement::Text(e) => e.base.z_index,
            WbElement::Shape(e) => e.base.z_index,
            WbElement::Arrow(e) => e.z_index,
            WbElement::Frame(e) => e.base.z_index,
            WbElement::Image(e) => e.base.z_index,
        }
    }

    /// The group every element in the same unit shares, if any.
    pub fn group_id(&self) -> Option<&str> {
        match self {
            WbElement::Sticky(e) => e.base.group_id.as_deref(),
            WbElement::Text(e) => e.base.group_id.as_deref(),
            WbElement::Shape(e) => e.base.group_id.as_deref(),
            WbElement::Arrow(e) => e.group_id.as_deref(),
            WbElement::Frame(e) => e.base.group_id.as_deref(),
            WbElement::Image(e) => e.base.group_id.as_deref(),
        }
    }

    /// The axis-aligned bounding box in content space, when the element has one. An arrow has none (its
    /// geometry is its two endpoints), matching the web's `'width' in e` guard in hit-testing and « fit ».
    pub fn bounds(&self) -> Option<Rect> {
        let b = match self {
            WbElement::Sticky(e) => &e.base,
            WbElement::Text(e) => &e.base,
            WbElement::Shape(e) => &e.base,
            WbElement::Frame(e) => &e.base,
            WbElement::Image(e) => &e.base,
            WbElement::Arrow(_) => return None,
        };
        Some(Rect { x: b.x, y: b.y, w: b.width, h: b.height })
    }

    /// The mutable base geometry, when the element has one (not an arrow).
    pub fn base_mut(&mut self) -> Option<&mut BaseElement> {
        match self {
            WbElement::Sticky(e) => Some(&mut e.base),
            WbElement::Text(e) => Some(&mut e.base),
            WbElement::Shape(e) => Some(&mut e.base),
            WbElement::Frame(e) => Some(&mut e.base),
            WbElement::Image(e) => Some(&mut e.base),
            WbElement::Arrow(_) => None,
        }
    }
}

/// An axis-aligned rectangle in content space.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}
