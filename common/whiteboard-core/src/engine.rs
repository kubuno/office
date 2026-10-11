//! The board engine, platform-neutral — a port of the logic in `whiteboard-engine.ts` and
//! `whiteboard-shapes.ts` that is not drawing: building the scene from the Yjs document, hit-testing,
//! resize handles, grouping, z-order and the draw-to-create gesture. Shapes resolve to geometry through
//! `kubuno-office-shapes-core`, exactly as the web routes them through `shapes/`.

use kubuno_office_shapes_core::{self as shapes, draw as shape_draw, SubPath, ViewOptions};
use serde_json::{Map, Value as Json};

use crate::collab::WhiteboardDoc;
use crate::model::{Rect, Scene, Stroke, WbElement};

/// Corner radius of the board's own plain `rect`, in content px (`BOARD_RECT_RADIUS`,
/// `whiteboard-shapes.ts`). A deliberate divergence from the shared geometry: the board's rectangle is a
/// soft-cornered card. `roundRect` from the gallery keeps its parametric knob.
pub const BOARD_RECT_RADIUS: f64 = 6.0;

/// Below this (content px) on either side, a draw gesture counts as a plain click (`DRAW_MIN_SIZE`).
pub const DRAW_MIN_SIZE: f64 = 10.0;

/// Builds the typed [`Scene`] from the collaborative document. Elements the desktop does not model (an
/// unknown `type`, or a malformed object) are skipped here but stay in the document, so a later save does
/// not drop them. Elements keep the web's paint order (ascending `zIndex`); strokes keep array order.
pub fn scene_from_doc(doc: &WhiteboardDoc, background: &str) -> Scene {
    let mut elements: Vec<WbElement> = doc
        .elements()
        .into_iter()
        .filter_map(|(_, v)| serde_json::from_value::<WbElement>(v).ok())
        .collect();
    elements.sort_by(|a, b| a.z_index().partial_cmp(&b.z_index()).unwrap_or(std::cmp::Ordering::Equal));
    let strokes: Vec<Stroke> = doc.strokes().into_iter().filter_map(|v| serde_json::from_value(v).ok()).collect();
    Scene { elements, strokes, background: background.to_string() }
}

/// Serialises one element back to the JSON the document stores (`JSON.parse(JSON.stringify(el))`).
pub fn element_json(el: &WbElement) -> Json {
    serde_json::to_value(el).unwrap_or(Json::Null)
}

/// Serialises the whole element set to the id → object map the document holds.
pub fn elements_map(scene: &Scene) -> Map<String, Json> {
    let mut m = Map::new();
    for e in &scene.elements {
        m.insert(e.id().to_string(), element_json(e));
    }
    m
}

/// The index of the top-most element whose bounding box contains `(x, y)` (within `tol`), searching from
/// the front — the port of `hitTest`. Arrows have no box and are skipped, exactly like the web's
/// `'width' in e` guard.
pub fn hit_test(x: f64, y: f64, elements: &[WbElement], tol: f64) -> Option<usize> {
    for i in (0..elements.len()).rev() {
        if let Some(b) = elements[i].bounds() {
            if x >= b.x - tol && x <= b.x + b.w + tol && y >= b.y - tol && y <= b.y + b.h + tol {
                return Some(i);
            }
        }
    }
    None
}

/// A resize handle, in the web's eight-point naming (`ResizeHandle`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeHandle {
    Nw,
    N,
    Ne,
    W,
    E,
    Sw,
    S,
    Se,
}

impl ResizeHandle {
    /// The CSS cursor the web shows for the handle (`handleCursor`).
    pub fn cursor(self) -> &'static str {
        match self {
            ResizeHandle::N | ResizeHandle::S => "ns-resize",
            ResizeHandle::E | ResizeHandle::W => "ew-resize",
            ResizeHandle::Nw | ResizeHandle::Se => "nwse-resize",
            ResizeHandle::Ne | ResizeHandle::Sw => "nesw-resize",
        }
    }
}

/// The eight selection-handle centres of an element, in content space, with the web's `PAD = 4/zoom`
/// inset (`renderSelectionHandles`/`hitHandle`). `None` for an element with no box.
pub fn handle_points(el: &WbElement, zoom: f64) -> Option<[(ResizeHandle, f64, f64); 8]> {
    let b = el.bounds()?;
    let pad = 4.0 / zoom;
    let (bx, by, bw, bh) = (b.x - pad, b.y - pad, b.w + pad * 2.0, b.h + pad * 2.0);
    Some([
        (ResizeHandle::Nw, bx, by),
        (ResizeHandle::N, bx + bw / 2.0, by),
        (ResizeHandle::Ne, bx + bw, by),
        (ResizeHandle::W, bx, by + bh / 2.0),
        (ResizeHandle::E, bx + bw, by + bh / 2.0),
        (ResizeHandle::Sw, bx, by + bh),
        (ResizeHandle::S, bx + bw / 2.0, by + bh),
        (ResizeHandle::Se, bx + bw, by + bh),
    ])
}

/// The resize handle under a content-space point, or `None` — the port of `hitHandle` (grab tolerance
/// `8/zoom`).
pub fn hit_handle(x: f64, y: f64, el: &WbElement, zoom: f64) -> Option<ResizeHandle> {
    let tol = 8.0 / zoom;
    let pts = handle_points(el, zoom)?;
    for (h, hx, hy) in pts {
        if (x - hx).abs() <= tol && (y - hy).abs() <= tol {
            return Some(h);
        }
    }
    None
}

/// Every element id in the same group as `id` (including it). An element with no `groupId` selects alone —
/// the web's group selection (`WbElement.groupId`).
pub fn group_members<'a>(elements: &'a [WbElement], id: &str) -> Vec<&'a str> {
    let group = elements.iter().find(|e| e.id() == id).and_then(|e| e.group_id());
    match group {
        None => elements.iter().filter(|e| e.id() == id).map(|e| e.id()).collect(),
        Some(g) => elements.iter().filter(|e| e.group_id() == Some(g)).map(|e| e.id()).collect(),
    }
}

/// Geometry resolution for a shape element's `kind`, matching `paintBoardShape`'s routing:
/// the board's own rounded `rect` first, then the shared `shape_view` (native fraction geometry or OOXML
/// preset), else `None` for an open geometry (a line or connector) the caller strokes itself.
pub enum ShapeGeometry {
    /// The board's soft-cornered rectangle: a rounded rect with [`BOARD_RECT_RADIUS`].
    BoardRect { radius: f64 },
    /// Shared sub-paths in the shape's own `w`×`h` box.
    Paths(Vec<SubPath>),
    /// An open geometry (line/connector) with no area path.
    Open,
}

/// Resolves a shape's geometry in a `w`×`h` box (`stroke`/`stroke_width` affect the native inset).
pub fn shape_geometry(kind: &str, w: f64, h: f64, adj: Option<&[f64]>, stroke: bool, stroke_width: f64) -> ShapeGeometry {
    if kind == "rect" {
        return ShapeGeometry::BoardRect { radius: BOARD_RECT_RADIUS };
    }
    let opts = ViewOptions { adj, stroke, stroke_width };
    match shapes::shape_view(kind, w, h, &opts) {
        Some(paths) => ShapeGeometry::Paths(paths),
        None => ShapeGeometry::Open,
    }
}

// ── Draw-to-create gesture ───────────────────────────────────────────────────────────────────────────

/// The live state of a « draw a shape » gesture, in content coordinates (`DrawGesture`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawGesture {
    pub start_x: f64,
    pub start_y: f64,
    pub cur_x: f64,
    pub cur_y: f64,
    /// Shift: constrain to a square.
    pub square: bool,
    /// Alt: grow from the centre.
    pub from_centre: bool,
}

/// The live box a draw gesture spans (`gestureBox`): normalised, with Shift/Alt, then snapped by `snap`.
pub fn gesture_box(g: &DrawGesture, snap: impl Fn(f64) -> f64) -> Rect {
    let b = shape_draw::draw_box_from(g.start_x, g.start_y, g.cur_x, g.cur_y, g.square, g.from_centre);
    Rect { x: snap(b.x), y: snap(b.y), w: snap(b.w), h: snap(b.h) }
}

/// The box a draw gesture becomes on release (`gestureFinalBox`): the drawn box, or a default-sized shape
/// centred on the click.
pub fn gesture_final_box(kind: &str, g: &DrawGesture, snap: impl Fn(f64) -> f64) -> Rect {
    let b = shape_draw::draw_box_from(g.start_x, g.start_y, g.cur_x, g.cur_y, g.square, g.from_centre);
    let f = shape_draw::finalize_draw_box(kind, b, g.start_x, g.start_y, DRAW_MIN_SIZE, None);
    Rect { x: snap(f.x), y: snap(f.y), w: snap(f.w), h: snap(f.h) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn shape(id: &str, x: f64, y: f64, w: f64, h: f64, z: f64) -> WbElement {
        serde_json::from_value(json!({"id":id,"type":"shape","x":x,"y":y,"width":w,"height":h,"zIndex":z,"kind":"rect","fill":"#fff","stroke":"#000","strokeWidth":2})).unwrap()
    }

    #[test]
    fn hit_test_takes_the_front_most() {
        let els = vec![shape("a", 0.0, 0.0, 100.0, 100.0, 0.0), shape("b", 10.0, 10.0, 20.0, 20.0, 1.0)];
        assert_eq!(hit_test(15.0, 15.0, &els, 4.0), Some(1));
        assert_eq!(hit_test(90.0, 90.0, &els, 4.0), Some(0));
        assert_eq!(hit_test(500.0, 500.0, &els, 4.0), None);
    }

    #[test]
    fn handles_and_cursors() {
        let el = shape("a", 0.0, 0.0, 100.0, 50.0, 0.0);
        // The SE handle sits near (100+pad, 50+pad) at zoom 1 (pad = 4).
        assert_eq!(hit_handle(104.0, 54.0, &el, 1.0), Some(ResizeHandle::Se));
        assert_eq!(ResizeHandle::Se.cursor(), "nwse-resize");
        assert_eq!(ResizeHandle::N.cursor(), "ns-resize");
    }

    #[test]
    fn groups_select_together() {
        let mut a = shape("a", 0.0, 0.0, 10.0, 10.0, 0.0);
        let mut b = shape("b", 0.0, 0.0, 10.0, 10.0, 1.0);
        let c = shape("c", 0.0, 0.0, 10.0, 10.0, 2.0);
        if let WbElement::Shape(s) = &mut a {
            s.base.group_id = Some("g".into());
        }
        if let WbElement::Shape(s) = &mut b {
            s.base.group_id = Some("g".into());
        }
        let els = vec![a, b, c];
        let mut m = group_members(&els, "a");
        m.sort();
        assert_eq!(m, vec!["a", "b"]);
        assert_eq!(group_members(&els, "c"), vec!["c"]);
    }

    #[test]
    fn a_click_becomes_a_default_sized_shape_and_a_drag_its_box() {
        let click = DrawGesture { start_x: 50.0, start_y: 50.0, cur_x: 51.0, cur_y: 51.0, square: false, from_centre: false };
        let b = gesture_final_box("ellipse", &click, |v| v);
        assert!(b.w > DRAW_MIN_SIZE && b.h > DRAW_MIN_SIZE, "a click inserts a default-sized shape");
        let drag = DrawGesture { start_x: 10.0, start_y: 10.0, cur_x: 110.0, cur_y: 60.0, square: false, from_centre: false };
        let d = gesture_box(&drag, |v| v);
        assert_eq!((d.x, d.y, d.w, d.h), (10.0, 10.0, 100.0, 50.0));
    }

    #[test]
    fn rect_is_the_boards_rounded_card_but_ellipse_has_paths() {
        assert!(matches!(shape_geometry("rect", 10.0, 10.0, None, true, 2.0), ShapeGeometry::BoardRect { .. }));
        assert!(matches!(shape_geometry("ellipse", 10.0, 10.0, None, true, 2.0), ShapeGeometry::Paths(_)));
        assert!(matches!(shape_geometry("line", 10.0, 10.0, None, true, 2.0), ShapeGeometry::Open));
    }
}
