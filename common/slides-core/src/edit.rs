//! Editing the slide — a port of the web's `SlideCanvas` (`PresentationEditorPage.tsx:1707-3735`): the
//! selection, hit testing, the pointer gestures (move with smart guides and grid snapping, marquee, resize,
//! rotate, adjustment knobs, crop, draw-to-create, lines of every kind), the keyboard, and every element
//! command of the ribbon and the context menus.
//!
//! Everything works on the active slide's `Vec<Element>`: a call returns whether it changed them, and the
//! caller records the PREVIOUS elements in the history (the web's `handleElementsChange`). Pointer positions
//! are fractions of the slide; `scale` is canvas pixels per slide pixel (the zoom), which the web's
//! thresholds are expressed in (7 px smart-guide snap, handle sizes).
//!
//! Web bugs not ported (vskubuno docs/PRESENTATIONS-DESKTOP.md §1.2): a locked element is hit by a click
//! (selectable to unlock it, as the web's own comment intends, never moved); nudging, duplicating and pasting
//! move a line's every point (`x2`, `y2`, `points`); the text tool returns to the selection after placing a
//! box like the other tools.

use std::collections::HashSet;

use kubuno_office_shapes_core::{adjust, draw as shape_draw, Rect};
use serde_json::{json, Map, Value};

use crate::insert::{self, IdGen};
use crate::model::{as_f64, num, truthy, Element, SLIDE_H, SLIDE_W};
use crate::render::{line_path_points, shape_adj};

/// The grid step (`GRID = 1/24` of the slide).
pub const GRID: f64 = 1.0 / 24.0;
/// The smart-guide threshold, canvas px.
pub const SNAP_PX: f64 = 7.0;

/// The canvas tool.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Tool {
    #[default]
    Select,
    Text,
    /// Draw a shape of this kind.
    Shape(String),
    /// Draw a line of this kind (`straight`, `arrow`, `elbow`, `curved`, `arc`, `polyline`, `freehand`).
    Line(String),
}

/// Keyboard modifiers of a gesture.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

/// A smart guide drawn during a gesture (`SnapGuide`): `vertical` at `pos` (fraction across), spanning
/// `a..b` (fractions along).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapGuide {
    pub vertical: bool,
    pub pos: f64,
    pub a: f64,
    pub b: f64,
}

/// A manual guide (`guides`): not saved, like the web.
#[derive(Debug, Clone, PartialEq)]
pub struct Guide {
    pub id: String,
    pub vertical: bool,
    pub pos: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Target {
    pos: f64,
    lo: f64,
    hi: f64,
}

/// `buildSnapTargets`: the slide's edges and centre, the manual guides, every other element's edges and centres.
fn snap_targets(others: &[&Element], guides: &[Guide]) -> (Vec<Target>, Vec<Target>) {
    let full = |pos| Target { pos, lo: 0.0, hi: 1.0 };
    let mut xs = vec![full(0.0), full(0.5), full(1.0)];
    let mut ys = vec![full(0.0), full(0.5), full(1.0)];
    for g in guides {
        if g.vertical {
            xs.push(full(g.pos));
        } else {
            ys.push(full(g.pos));
        }
    }
    for o in others {
        let b = o.bbox();
        for p in [b.x, b.x + b.w / 2.0, b.x + b.w] {
            xs.push(Target { pos: p, lo: b.y, hi: b.y + b.h });
        }
        for p in [b.y, b.y + b.h / 2.0, b.y + b.h] {
            ys.push(Target { pos: p, lo: b.x, hi: b.x + b.w });
        }
    }
    (xs, ys)
}

/// `snapAxis`: the nearest target within `thresh` of any of `edges` (`(value, lo, hi)`).
fn snap_axis(edges: &[(f64, f64, f64)], targets: &[Target], thresh: f64, vertical: bool) -> Option<(f64, SnapGuide)> {
    let mut best: Option<(f64, f64, SnapGuide)> = None;
    for &(v, lo, hi) in edges {
        for t in targets {
            let d = (v - t.pos).abs();
            if d > thresh || best.as_ref().is_some_and(|b| d >= b.0) {
                continue;
            }
            best = Some((d, t.pos - v, SnapGuide { vertical, pos: t.pos, a: lo.min(t.lo), b: hi.max(t.hi) }));
        }
    }
    best.map(|b| (b.1, b.2))
}

/// `snapBox`.
fn snap_box(b: Rect, targets: &(Vec<Target>, Vec<Target>), tx: f64, ty: f64) -> (f64, f64, Vec<SnapGuide>) {
    let mut guides = Vec::new();
    let sx = snap_axis(&[(b.x, b.y, b.y + b.h), (b.x + b.w / 2.0, b.y, b.y + b.h), (b.x + b.w, b.y, b.y + b.h)], &targets.0, tx, true);
    let sy = snap_axis(&[(b.y, b.x, b.x + b.w), (b.y + b.h / 2.0, b.x, b.x + b.w), (b.y + b.h, b.x, b.x + b.w)], &targets.1, ty, false);
    let (mut x, mut y) = (b.x, b.y);
    if let Some((d, g)) = sx {
        x += d;
        guides.push(g);
    }
    if let Some((d, g)) = sy {
        y += d;
        guides.push(g);
    }
    (x, y, guides)
}

fn dist_to_segment(px: f64, py: f64, ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let (dx, dy) = (bx - ax, by - ay);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((px - ax) * dx + (py - ay) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    (px - (ax + t * dx)).hypot(py - (ay + t * dy))
}

/// `hitTest` at slide px `(x, y)`: the topmost visible element (lines by distance to their segments).
pub fn hit_test(elements: &[Element], x: f64, y: f64) -> Option<&Element> {
    let mut sorted: Vec<&Element> = elements.iter().filter(|e| !e.hidden()).collect();
    sorted.sort_by(|a, b| b.z_index().total_cmp(&a.z_index()));
    for el in sorted {
        if el.kind() == "line" {
            let pts = line_path_points(el, SLIDE_W, SLIDE_H);
            let tol = el.get("stroke").and_then(|s| as_f64(s.get("width"))).unwrap_or(6.0).max(6.0);
            if pts.windows(2).any(|s| dist_to_segment(x, y, s[0].0, s[0].1, s[1].0, s[1].1) <= tol) {
                return Some(el);
            }
            continue;
        }
        let (ex, ey, ew, eh) = (el.x() * SLIDE_W, el.y() * SLIDE_H, el.w() * SLIDE_W, el.h() * SLIDE_H);
        if x >= ex && x <= ex + ew && y >= ey && y <= ey + eh {
            return Some(el);
        }
    }
    None
}

/// Moves an element by `(dx, dy)` (fractions), every point of a line included.
pub fn translated(e: &Element, dx: f64, dy: f64) -> Element {
    let mut n = e.clone();
    n.set_f("x", e.x() + dx);
    n.set_f("y", e.y() + dy);
    if e.kind() == "line" {
        n.set_f("x2", e.f_or("x2", 0.0) + dx);
        n.set_f("y2", e.f_or("y2", 0.0) + dy);
        if let Some(Value::Array(pts)) = e.get("points") {
            let moved: Vec<Value> = pts.iter().map(|p| json!({ "x": num(as_f64(p.get("x")).unwrap_or(0.0) + dx), "y": num(as_f64(p.get("y")).unwrap_or(0.0) + dy) })).collect();
            n.set("points", Value::Array(moved));
        }
    }
    n
}

/// The common box of elements (`combinedBBox`).
pub fn combined_bbox<'a>(els: impl IntoIterator<Item = &'a Element>) -> Option<Rect> {
    let bs: Vec<Rect> = els.into_iter().map(Element::bbox).collect();
    if bs.is_empty() {
        return None;
    }
    let x = bs.iter().map(|b| b.x).fold(f64::INFINITY, f64::min);
    let y = bs.iter().map(|b| b.y).fold(f64::INFINITY, f64::min);
    let r = bs.iter().map(|b| b.x + b.w).fold(f64::NEG_INFINITY, f64::max);
    let btm = bs.iter().map(|b| b.y + b.h).fold(f64::NEG_INFINITY, f64::max);
    Some(Rect { x, y, w: r - x, h: btm - y })
}

/// `expandSel`: a selection grown to every member of the groups it touches.
pub fn expand_sel(elements: &[Element], ids: &[String]) -> Vec<String> {
    let gids: HashSet<&str> = elements.iter().filter(|e| ids.iter().any(|i| i == e.id())).filter_map(Element::group_id).collect();
    if gids.is_empty() {
        return ids.to_vec();
    }
    let mut out: Vec<String> = ids.to_vec();
    for e in elements {
        if e.group_id().is_some_and(|g| gids.contains(g)) && !out.iter().any(|i| i == e.id()) {
            out.push(e.id().to_string());
        }
    }
    out
}

/// A resize handle (`n`, `s`, `e`, `w`, `ne`…).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Handle(pub &'static str);

/// What is under the pointer on the selection's furniture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Furniture {
    Resize(Handle),
    Rotate,
    Knob(usize),
    /// The text box's fit button (bottom-left).
    FitButton,
    /// The image bar's buttons: replace, crop, reset proportions, alt text.
    ImageBar(usize),
}

/// The selection overlay of one element, in canvas px relative to the slide's top-left (`geometry` of the
/// web's DOM overlay): the box, its rotation, and where each piece of furniture is.
#[derive(Debug, Clone, PartialEq)]
pub struct Overlay {
    pub box_px: Rect,
    pub rotation: f64,
    pub is_line: bool,
    pub locked: bool,
    pub knobs: Vec<(usize, f64, f64)>,
    pub kind: String,
}

/// Furniture sizes of the web's overlay (fine pointer): corners 10 px circles, edge pills 20×8, the rotate
/// handle 20 px at 36 px above the box with a 24 px stem, knobs 10 px.
pub const CORNER: f64 = 10.0;
pub const PILL: (f64, f64) = (20.0, 8.0);
pub const ROTATE: f64 = 20.0;
pub const ROTATE_TOP: f64 = 36.0;
pub const KNOB: f64 = 10.0;

pub fn overlay_of(el: &Element, scale: f64) -> Overlay {
    let g = el.bbox();
    let box_px = Rect { x: g.x * SLIDE_W * scale, y: g.y * SLIDE_H * scale, w: g.w * SLIDE_W * scale, h: g.h * SLIDE_H * scale };
    let knobs = if el.kind() == "shape" {
        let adj = shape_adj(el);
        adjust::adjust_handles(el.s("shape").unwrap_or("rect"), Rect { x: 0.0, y: 0.0, w: box_px.w, h: box_px.h }, adj.as_deref()).into_iter().map(|k| (k.index, k.x, k.y)).collect()
    } else {
        Vec::new()
    };
    Overlay { box_px, rotation: el.rotation(), is_line: el.kind() == "line", locked: el.locked(), knobs, kind: el.kind().to_string() }
}

impl Overlay {
    /// A canvas-px point (relative to the slide) in the box's own unrotated frame (origin at its top-left).
    pub fn local(&self, px: f64, py: f64) -> (f64, f64) {
        let b = self.box_px;
        let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
        let r = -self.rotation.to_radians();
        let (dx, dy) = (px - cx, py - cy);
        let (rx, ry) = (dx * r.cos() - dy * r.sin(), dx * r.sin() + dy * r.cos());
        (rx + b.w / 2.0, ry + b.h / 2.0)
    }

    /// The furniture under a canvas-px point, topmost first (the web's DOM order reversed).
    pub fn furniture_at(&self, px: f64, py: f64) -> Option<Furniture> {
        if self.locked || self.is_line {
            return None;
        }
        let (lx, ly) = self.local(px, py);
        let (w, h) = (self.box_px.w, self.box_px.h);
        if self.kind == "image" {
            // The bar: 4 buttons of 28 px from x = 6, 48 px below the box's top-left… bottom edge.
            let top = h + 48.0 - 36.0;
            for i in 0..4 {
                let x0 = 6.0 + i as f64 * 30.0;
                if lx >= x0 && lx <= x0 + 28.0 && ly >= top + 4.0 && ly <= top + 32.0 {
                    return Some(Furniture::ImageBar(i));
                }
            }
        }
        if self.kind == "text" {
            let (fx, fy) = (-8.0 + 16.0, h + 40.0 - 32.0 + 16.0);
            if (lx - fx).hypot(ly - fy) <= 16.0 {
                return Some(Furniture::FitButton);
            }
        }
        for &(i, kx, ky) in self.knobs.iter().rev() {
            if (lx - kx).abs() <= KNOB / 2.0 + 1.0 && (ly - ky).abs() <= KNOB / 2.0 + 1.0 {
                return Some(Furniture::Knob(i));
            }
        }
        let pill = |cx: f64, cy: f64, horiz: bool| {
            let (pw, ph) = if horiz { PILL } else { (PILL.1, PILL.0) };
            (lx - cx).abs() <= pw / 2.0 + 1.0 && (ly - cy).abs() <= ph / 2.0 + 1.0
        };
        if pill(w / 2.0, 0.0, true) {
            return Some(Furniture::Resize(Handle("n")));
        }
        if pill(w / 2.0, h, true) {
            return Some(Furniture::Resize(Handle("s")));
        }
        if pill(0.0, h / 2.0, false) {
            return Some(Furniture::Resize(Handle("w")));
        }
        if pill(w, h / 2.0, false) {
            return Some(Furniture::Resize(Handle("e")));
        }
        for (name, cx, cy) in [("nw", 0.0, 0.0), ("ne", w, 0.0), ("sw", 0.0, h), ("se", w, h)] {
            if (lx - cx).hypot(ly - cy) <= CORNER / 2.0 + 2.0 {
                return Some(Furniture::Resize(Handle(name)));
            }
        }
        if (lx - w / 2.0).hypot(ly - (-ROTATE_TOP + ROTATE / 2.0)) <= ROTATE / 2.0 + 2.0 {
            return Some(Furniture::Rotate);
        }
        None
    }
}

/// The image crop in progress: the frame and the full image's extent, canvas px relative to the slide.
#[derive(Debug, Clone, PartialEq)]
pub struct Crop {
    pub id: String,
    pub frame: Rect,
    pub full: Rect,
}

#[derive(Debug, Clone)]
enum Gesture {
    Move { start: (f64, f64), ids: Vec<String>, snapshot: Vec<Element> },
    Marquee { x0: f64, y0: f64, x1: f64, y1: f64, additive: bool, base: Vec<String> },
    Resize { id: String, handle: &'static str, start: (f64, f64), orig: Rect },
    Rotate { id: String },
    Knob { id: String, index: usize },
    DrawShape { id: String, start: (f64, f64) },
    DrawSegment { id: String },
    Freehand { id: String },
    Guide { id: String },
    CropDrag { handle: &'static str, start: (f64, f64), frame: Rect },
}

/// What a pointer call did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Outcome {
    /// The elements changed (record the previous ones in the history).
    pub changed: bool,
    /// The selection or the overlay changed (repaint).
    pub repaint: bool,
    /// A finished drawing reset the tool to the selection.
    pub tool_reset: bool,
}

impl Outcome {
    fn changed() -> Self {
        Outcome { changed: true, repaint: true, tool_reset: false }
    }
    fn repaint() -> Self {
        Outcome { changed: false, repaint: true, tool_reset: false }
    }
}

/// A text style captured by « Reproduire la mise en forme ».
pub type StyleClip = Map<String, Value>;

/// The canvas editor's state (one per open presentation; the selection is cleared when the slide changes).
#[derive(Debug, Default)]
pub struct Editor {
    pub selection: Vec<String>,
    pub tool: Tool,
    pub snap_guides: Vec<SnapGuide>,
    pub guides: Vec<Guide>,
    pub show_guides: bool,
    pub show_grid: bool,
    pub snap_grid: bool,
    pub marquee: Option<Rect>,
    pub crop: Option<Crop>,
    /// The polyline being drawn.
    pub polyline: Option<String>,
    gesture: Option<Gesture>,
    /// The element clipboard (`elementClipRef`), across slides.
    pub clipboard: Vec<Element>,
    pub style_clip: Option<StyleClip>,
}

impl Editor {
    pub fn new() -> Self {
        Editor { show_guides: true, ..Editor::default() }
    }

    /// A slide change: no selection, no gesture (`useEffect([slide.id])`).
    pub fn reset_for_slide(&mut self) {
        self.selection.clear();
        self.gesture = None;
        self.snap_guides.clear();
        self.marquee = None;
        self.crop = None;
        self.polyline = None;
    }

    pub fn is_busy(&self) -> bool {
        self.gesture.is_some()
    }

    /// The single selected element, if exactly one.
    pub fn single<'e>(&self, els: &'e [Element]) -> Option<&'e Element> {
        if self.selection.len() == 1 {
            els.iter().find(|e| e.id() == self.selection[0])
        } else {
            None
        }
    }

    pub fn select(&mut self, ids: Vec<String>) {
        self.selection = ids;
    }

    // ── Pointer ─────────────────────────────────────────────────────────────

    /// Press at `(fx, fy)` (slide fractions).
    pub fn pointer_down(&mut self, els: &mut Vec<Element>, fx: f64, fy: f64, mods: Mods, scale: f64, ids: &mut dyn IdGen) -> Outcome {
        if self.crop.is_some() {
            // A press on the crop's frame or handles drags it; elsewhere it confirms (`confirmCrop`).
            if let Some(h) = self.crop_handle_at(fx * SLIDE_W * scale, fy * SLIDE_H * scale) {
                let frame = self.crop.as_ref().map(|c| c.frame).unwrap_or_default();
                self.gesture = Some(Gesture::CropDrag { handle: h, start: (fx * SLIDE_W * scale, fy * SLIDE_H * scale), frame });
                return Outcome::repaint();
            }
            return self.confirm_crop(els, scale);
        }
        let (px, py) = (fx * SLIDE_W, fy * SLIDE_H);
        // The selection's furniture first (it lies over the elements).
        if self.tool == Tool::Select && self.selection.len() == 1 {
            if let Some(el) = els.iter().find(|e| e.id() == self.selection[0]) {
                let ov = overlay_of(el, scale);
                match ov.furniture_at(px * scale, py * scale) {
                    Some(Furniture::Resize(Handle(h))) => {
                        self.gesture = Some(Gesture::Resize { id: el.id().into(), handle: h, start: (fx, fy), orig: el.bbox() });
                        return Outcome::repaint();
                    }
                    Some(Furniture::Rotate) => {
                        self.gesture = Some(Gesture::Rotate { id: el.id().into() });
                        return Outcome::repaint();
                    }
                    Some(Furniture::Knob(i)) => {
                        self.gesture = Some(Gesture::Knob { id: el.id().into(), index: i });
                        return Outcome::repaint();
                    }
                    _ => {}
                }
            }
        }
        // A manual guide (6 px band).
        if self.tool == Tool::Select && self.show_guides {
            if let Some(g) = self.guides.iter().rev().find(|g| if g.vertical { (g.pos * SLIDE_W * scale - px * scale).abs() <= 3.0 } else { (g.pos * SLIDE_H * scale - py * scale).abs() <= 3.0 }) {
                self.gesture = Some(Gesture::Guide { id: g.id.clone() });
                return Outcome::repaint();
            }
        }
        let z = els.len() + 1;
        match self.tool.clone() {
            Tool::Select => {
                let Some(hit) = hit_test(els, px, py) else {
                    if !mods.shift {
                        self.selection.clear();
                    }
                    self.gesture = Some(Gesture::Marquee { x0: fx, y0: fy, x1: fx, y1: fy, additive: mods.shift, base: self.selection.clone() });
                    self.marquee = Some(Rect { x: fx, y: fy, w: 0.0, h: 0.0 });
                    return Outcome::repaint();
                };
                let hit_id = hit.id().to_string();
                let locked = hit.locked();
                let group = expand_sel(els, std::slice::from_ref(&hit_id));
                if mods.shift {
                    let all_in = group.iter().all(|g| self.selection.contains(g));
                    for g in group {
                        if all_in {
                            self.selection.retain(|s| *s != g);
                        } else if !self.selection.contains(&g) {
                            self.selection.push(g);
                        }
                    }
                    return Outcome::repaint();
                }
                if locked {
                    self.selection = group;
                    return Outcome::repaint();
                }
                let mut sel = if self.selection.contains(&hit_id) && self.selection.len() > 1 { self.selection.clone() } else { group };
                if mods.ctrl {
                    // Ctrl+drag duplicates the selection and moves the copies (groups re-keyed).
                    let mut gid_map: Vec<(String, String)> = Vec::new();
                    let n = els.len();
                    let clones: Vec<Element> = els
                        .iter()
                        .filter(|e| sel.iter().any(|s| s == e.id()))
                        .enumerate()
                        .map(|(i, e)| {
                            let mut c = e.clone();
                            c.set_s("id", &ids.next_id());
                            c.set_f("zIndex", (n + 1 + i) as f64);
                            if let Some(g) = e.group_id().map(str::to_string) {
                                let ng = match gid_map.iter().find(|(o, _)| *o == g) {
                                    Some((_, n)) => n.clone(),
                                    None => {
                                        let ng = ids.next_id();
                                        gid_map.push((g, ng.clone()));
                                        ng
                                    }
                                };
                                c.set_s("groupId", &ng);
                            }
                            c
                        })
                        .collect();
                    sel = clones.iter().map(|c| c.id().to_string()).collect();
                    els.extend(clones.iter().cloned());
                    self.selection = sel.clone();
                    self.gesture = Some(Gesture::Move { start: (fx, fy), ids: sel, snapshot: clones });
                    return Outcome::changed();
                }
                self.selection = sel.clone();
                let snapshot = els.iter().filter(|e| sel.iter().any(|s| s == e.id())).cloned().collect();
                self.gesture = Some(Gesture::Move { start: (fx, fy), ids: sel, snapshot });
                Outcome::repaint()
            }
            Tool::Text => {
                let el = insert::text_tool_box(ids, fx, fy, z, "Texte");
                self.selection = vec![el.id().to_string()];
                els.push(el);
                self.tool = Tool::Select;
                Outcome { changed: true, repaint: true, tool_reset: true }
            }
            Tool::Shape(kind) => {
                let el = insert::shape_at(ids, &kind, fx, fy, 0.0, 0.0, z);
                let id = el.id().to_string();
                self.selection = vec![id.clone()];
                els.push(el);
                self.gesture = Some(Gesture::DrawShape { id, start: (fx, fy) });
                Outcome::changed()
            }
            Tool::Line(kind) => {
                if kind == "polyline" {
                    if let Some(pid) = self.polyline.clone() {
                        if let Some(e) = els.iter_mut().find(|e| e.id() == pid) {
                            let mut pts = e.line_points();
                            pts.push((fx, fy));
                            e.set("points", insert::points_value(&pts));
                        }
                        return Outcome::changed();
                    }
                    let el = insert::line_at(ids, &kind, fx, fy, z, Some(vec![(fx, fy), (fx, fy)]));
                    self.polyline = Some(el.id().to_string());
                    self.selection = vec![el.id().to_string()];
                    els.push(el);
                    return Outcome::changed();
                }
                let freehand = kind == "freehand";
                let el = insert::line_at(ids, &kind, fx, fy, z, if freehand { Some(vec![(fx, fy)]) } else { None });
                let id = el.id().to_string();
                self.selection = vec![id.clone()];
                els.push(el);
                self.gesture = Some(if freehand { Gesture::Freehand { id } } else { Gesture::DrawSegment { id } });
                Outcome::changed()
            }
        }
    }

    /// Pointer move at `(fx, fy)` with the button held (or a polyline's rubber band).
    pub fn pointer_move(&mut self, els: &mut [Element], fx: f64, fy: f64, mods: Mods, scale: f64) -> Outcome {
        let Some(g) = self.gesture.clone() else {
            if let Some(pid) = &self.polyline {
                if let Some(e) = els.iter_mut().find(|e| e.id() == pid) {
                    let mut pts = e.line_points();
                    if let Some(last) = pts.last_mut() {
                        *last = (fx, fy);
                    }
                    e.set("points", insert::points_value(&pts));
                    return Outcome::changed();
                }
            }
            return Outcome::default();
        };
        match g {
            Gesture::DrawShape { id, start } => {
                let b = shape_draw::draw_box_from(start.0, start.1, fx, fy, mods.shift, mods.alt);
                if let Some(e) = els.iter_mut().find(|e| e.id() == id) {
                    set_box(e, b);
                }
                Outcome::changed()
            }
            Gesture::DrawSegment { id } => {
                if let Some(e) = els.iter_mut().find(|e| e.id() == id) {
                    e.set_f("x2", fx);
                    e.set_f("y2", fy);
                }
                Outcome::changed()
            }
            Gesture::Freehand { id } => {
                if let Some(e) = els.iter_mut().find(|e| e.id() == id) {
                    let mut pts = e.line_points();
                    if e.get("points").and_then(Value::as_array).map(|a| a.len()) == Some(1) {
                        pts.truncate(1);
                    }
                    let far = pts.last().is_none_or(|l| (fx - l.0).hypot(fy - l.1) > 0.004);
                    if far {
                        pts.push((fx, fy));
                    }
                    e.set("points", insert::points_value(&pts));
                    e.set_f("x2", fx);
                    e.set_f("y2", fy);
                }
                Outcome::changed()
            }
            Gesture::Marquee { x0, y0, additive, base, .. } => {
                self.gesture = Some(Gesture::Marquee { x0, y0, x1: fx, y1: fy, additive, base });
                self.marquee = Some(Rect { x: x0.min(fx), y: y0.min(fy), w: (fx - x0).abs(), h: (fy - y0).abs() });
                Outcome::repaint()
            }
            Gesture::Move { start, ids, snapshot } => {
                let (mut dx, mut dy) = (fx - start.0, fy - start.1);
                if mods.shift {
                    if dx.abs() > dy.abs() {
                        dy = 0.0;
                    } else {
                        dx = 0.0;
                    }
                }
                let Some(orig) = combined_bbox(snapshot.iter()) else { return Outcome::default() };
                if self.snap_grid && !mods.alt {
                    let gx = crate::js_round((orig.x + dx) / GRID) * GRID;
                    let gy = crate::js_round((orig.y + dy) / GRID) * GRID;
                    dx += gx - (orig.x + dx);
                    dy += gy - (orig.y + dy);
                    self.snap_guides.clear();
                } else if !mods.alt {
                    let others: Vec<&Element> = els.iter().filter(|e| !ids.iter().any(|i| i == e.id())).collect();
                    let t = snap_targets(&others, &self.guides);
                    let (sx, sy, guides) = snap_box(Rect { x: orig.x + dx, y: orig.y + dy, w: orig.w, h: orig.h }, &t, SNAP_PX / (SLIDE_W * scale), SNAP_PX / (SLIDE_H * scale));
                    dx += sx - (orig.x + dx);
                    dy += sy - (orig.y + dy);
                    self.snap_guides = guides;
                } else {
                    self.snap_guides.clear();
                }
                for e in els.iter_mut() {
                    if let Some(o) = snapshot.iter().find(|o| o.id() == e.id()) {
                        *e = translated(o, dx, dy);
                    }
                }
                Outcome::changed()
            }
            Gesture::Resize { id, handle, start, orig } => {
                let (ddx, ddy) = (fx - start.0, fy - start.1);
                let (mut x, mut y, mut w, mut h) = (orig.x, orig.y, orig.w, orig.h);
                if handle.contains('e') {
                    w = (orig.w + ddx).max(0.02);
                }
                if handle.contains('s') {
                    h = (orig.h + ddy).max(0.02);
                }
                if handle.contains('w') {
                    w = (orig.w - ddx).max(0.02);
                    x = orig.x + (orig.w - w);
                }
                if handle.contains('n') {
                    h = (orig.h - ddy).max(0.02);
                    y = orig.y + (orig.h - h);
                }
                if mods.shift && orig.w > 0.0 && orig.h > 0.0 {
                    let ar = orig.w / orig.h;
                    if w / h > ar {
                        w = h * ar;
                    } else {
                        h = w / ar;
                    }
                    if handle.contains('w') {
                        x = orig.x + (orig.w - w);
                    }
                    if handle.contains('n') {
                        y = orig.y + (orig.h - h);
                    }
                }
                if self.snap_grid && !mods.alt {
                    let r2 = |v: f64| crate::js_round(v / GRID) * GRID;
                    if handle.contains('e') {
                        w = (r2(x + w) - x).max(0.02);
                    }
                    if handle.contains('s') {
                        h = (r2(y + h) - y).max(0.02);
                    }
                    if handle.contains('w') {
                        let nx = r2(x);
                        w = (w + (x - nx)).max(0.02);
                        x = nx;
                    }
                    if handle.contains('n') {
                        let ny = r2(y);
                        h = (h + (y - ny)).max(0.02);
                        y = ny;
                    }
                    self.snap_guides.clear();
                } else if !mods.alt && !mods.shift {
                    let others: Vec<&Element> = els.iter().filter(|e| e.id() != id).collect();
                    let t = snap_targets(&others, &self.guides);
                    let (tx, ty) = (SNAP_PX / (SLIDE_W * scale), SNAP_PX / (SLIDE_H * scale));
                    let mut gs = Vec::new();
                    if handle.contains('e') {
                        if let Some((d, g)) = snap_axis(&[(x + w, y, y + h)], &t.0, tx, true) {
                            w += d;
                            gs.push(g);
                        }
                    }
                    if handle.contains('w') {
                        if let Some((d, g)) = snap_axis(&[(x, y, y + h)], &t.0, tx, true) {
                            x += d;
                            w -= d;
                            gs.push(g);
                        }
                    }
                    if handle.contains('s') {
                        if let Some((d, g)) = snap_axis(&[(y + h, x, x + w)], &t.1, ty, false) {
                            h += d;
                            gs.push(g);
                        }
                    }
                    if handle.contains('n') {
                        if let Some((d, g)) = snap_axis(&[(y, x, x + w)], &t.1, ty, false) {
                            y += d;
                            h -= d;
                            gs.push(g);
                        }
                    }
                    self.snap_guides = gs;
                } else {
                    self.snap_guides.clear();
                }
                if let Some(e) = els.iter_mut().find(|e| e.id() == id) {
                    set_box(e, Rect { x, y, w, h });
                }
                Outcome::changed()
            }
            Gesture::Rotate { id } => {
                let Some(e) = els.iter_mut().find(|e| e.id() == id) else { return Outcome::default() };
                let g = e.bbox();
                let (cx, cy) = ((g.x + g.w / 2.0) * SLIDE_W, (g.y + g.h / 2.0) * SLIDE_H);
                let mut deg = (fy * SLIDE_H - cy).atan2(fx * SLIDE_W - cx).to_degrees() + 90.0;
                if mods.shift {
                    deg = crate::js_round(deg / 15.0) * 15.0;
                }
                let deg = crate::js_round(((deg % 360.0) + 360.0) % 360.0);
                e.set_f("rotation", deg);
                Outcome::changed()
            }
            Gesture::Knob { id, index } => {
                let Some(e) = els.iter_mut().find(|e| e.id() == id) else { return Outcome::default() };
                let (bw, bh) = (e.w() * SLIDE_W * scale, e.h() * SLIDE_H * scale);
                if bw <= 0.0 || bh <= 0.0 {
                    return Outcome::default();
                }
                let (cx, cy) = ((e.x() + e.w() / 2.0) * SLIDE_W * scale, (e.y() + e.h() / 2.0) * SLIDE_H * scale);
                let (mut dx, mut dy) = (fx * SLIDE_W * scale - cx, fy * SLIDE_H * scale - cy);
                let rot = e.rotation().to_radians();
                if rot != 0.0 {
                    let (s, c) = (-rot).sin_cos();
                    (dx, dy) = (dx * c - dy * s, dx * s + dy * c);
                }
                let adj = shape_adj(e);
                let next = adjust::adjust_from_drag(e.s("shape").unwrap_or("rect"), Rect { x: 0.0, y: 0.0, w: bw, h: bh }, index, bw / 2.0 + dx, bh / 2.0 + dy, adj.as_deref());
                e.set("adj", Value::Array(next.into_iter().map(num).collect()));
                Outcome::changed()
            }
            Gesture::Guide { id } => {
                if let Some(g) = self.guides.iter_mut().find(|g| g.id == id) {
                    g.pos = if g.vertical { fx } else { fy }.clamp(0.0, 1.0);
                }
                Outcome::repaint()
            }
            Gesture::CropDrag { handle, start, frame } => {
                let (cx, cy) = (fx * SLIDE_W * scale, fy * SLIDE_H * scale);
                if let Some(c) = &mut self.crop {
                    c.frame = crop_drag(handle, frame, c.full, cx - start.0, cy - start.1);
                }
                Outcome::repaint()
            }
        }
    }

    /// Release.
    pub fn pointer_up(&mut self, els: &mut [Element]) -> Outcome {
        let g = self.gesture.take();
        self.snap_guides.clear();
        match g {
            Some(Gesture::DrawShape { id, start }) => {
                let mut out = Outcome { changed: false, repaint: true, tool_reset: true };
                if let Some(e) = els.iter_mut().find(|e| e.id() == id) {
                    let b = shape_draw::finalize_draw_box(e.s("shape").unwrap_or("rect"), e.bbox(), start.0, start.1, 0.02, Some((0.2, 0.15)));
                    if b.w != e.w() || b.h != e.h() {
                        set_box(e, b);
                        out.changed = true;
                    }
                }
                self.tool = Tool::Select;
                out
            }
            Some(Gesture::DrawSegment { .. }) | Some(Gesture::Freehand { .. }) => {
                self.tool = Tool::Select;
                Outcome { changed: false, repaint: true, tool_reset: true }
            }
            Some(Gesture::Marquee { x0, y0, x1, y1, additive, base }) => {
                self.marquee = None;
                let (rx0, rx1, ry0, ry1) = (x0.min(x1), x0.max(x1), y0.min(y1), y0.max(y1));
                if rx1 - rx0 > 0.005 || ry1 - ry0 > 0.005 {
                    let hits: Vec<String> = els
                        .iter()
                        .filter(|e| {
                            let b = e.bbox();
                            b.x < rx1 && b.x + b.w > rx0 && b.y < ry1 && b.y + b.h > ry0
                        })
                        .map(|e| e.id().to_string())
                        .collect();
                    let mut all: Vec<String> = if additive { base } else { Vec::new() };
                    for h in hits {
                        if !all.contains(&h) {
                            all.push(h);
                        }
                    }
                    self.selection = expand_sel(els, &all);
                }
                Outcome::repaint()
            }
            Some(_) => Outcome::repaint(),
            None => Outcome::default(),
        }
    }

    /// Ends the polyline being drawn (double click, Escape): drops the point following the pointer.
    pub fn finish_polyline(&mut self, els: &mut [Element]) -> Outcome {
        let Some(pid) = self.polyline.take() else { return Outcome::default() };
        if let Some(e) = els.iter_mut().find(|e| e.id() == pid) {
            let mut pts = e.line_points();
            if pts.len() > 2 {
                pts.pop();
            }
            e.set("points", insert::points_value(&pts));
        }
        self.tool = Tool::Select;
        Outcome { changed: true, repaint: true, tool_reset: true }
    }

    // ── Crop ────────────────────────────────────────────────────────────────

    /// `enterCrop`: the frame is the element's box, the full extent the uncropped image.
    pub fn enter_crop(&mut self, els: &[Element], id: &str, scale: f64) {
        let Some(el) = els.iter().find(|e| e.id() == id && e.kind() == "image") else { return };
        let er = Rect { x: el.x() * SLIDE_W * scale, y: el.y() * SLIDE_H * scale, w: el.w() * SLIDE_W * scale, h: el.h() * SLIDE_H * scale };
        let c = el.get("crop").filter(|c| c.is_object());
        let g = |k: &str, d: f64| c.and_then(|c| as_f64(c.get(k))).unwrap_or(d);
        let (cx, cy, cw, ch) = (g("x", 0.0), g("y", 0.0), g("w", 1.0), g("h", 1.0));
        let full = Rect { x: er.x - (cx / cw) * er.w, y: er.y - (cy / ch) * er.h, w: er.w / cw, h: er.h / ch };
        self.selection = vec![id.to_string()];
        self.crop = Some(Crop { id: id.to_string(), frame: er, full });
    }

    pub fn cancel_crop(&mut self) {
        self.crop = None;
        self.gesture = None;
    }

    /// `confirmCrop`: the crop as fractions of the source, the box resized to the frame.
    pub fn confirm_crop(&mut self, els: &mut [Element], scale: f64) -> Outcome {
        let Some(c) = self.crop.take() else { return Outcome::default() };
        self.gesture = None;
        let (f, full) = (c.frame, c.full);
        let crop = json!({ "x": num((f.x - full.x) / full.w), "y": num((f.y - full.y) / full.h), "w": num(f.w / full.w), "h": num(f.h / full.h) });
        if let Some(e) = els.iter_mut().find(|e| e.id() == c.id) {
            set_box(e, Rect { x: f.x / (SLIDE_W * scale), y: f.y / (SLIDE_H * scale), w: f.w / (SLIDE_W * scale), h: f.h / (SLIDE_H * scale) });
            e.set("crop", crop);
        }
        Outcome::changed()
    }

    fn crop_handle_at(&self, px: f64, py: f64) -> Option<&'static str> {
        let c = self.crop.as_ref()?;
        let f = c.frame;
        let hs = [("nw", f.x, f.y), ("ne", f.x + f.w, f.y), ("sw", f.x, f.y + f.h), ("se", f.x + f.w, f.y + f.h), ("n", f.x + f.w / 2.0, f.y), ("s", f.x + f.w / 2.0, f.y + f.h), ("w", f.x, f.y + f.h / 2.0), ("e", f.x + f.w, f.y + f.h / 2.0)];
        for (n, x, y) in hs {
            if (px - x).abs() <= 7.0 && (py - y).abs() <= 7.0 {
                return Some(n);
            }
        }
        (px >= f.x && px <= f.x + f.w && py >= f.y && py <= f.y + f.h).then_some("move")
    }

    // ── Keyboard (`handleKeyDown`) ──────────────────────────────────────────

    /// The canvas keys while no text is being edited. `key`: `Delete`, `Backspace`, `Escape`, `ArrowLeft`…,
    /// or a letter with `ctrl`. Returns `None` when the key is not the canvas's.
    pub fn key(&mut self, els: &mut Vec<Element>, key: &str, mods: Mods, ids: &mut dyn IdGen, scale: f64) -> Option<Outcome> {
        if self.crop.is_some() {
            return match key {
                "Enter" => Some(self.confirm_crop(els, scale)),
                "Escape" => {
                    self.cancel_crop();
                    Some(Outcome::repaint())
                }
                _ => None,
            };
        }
        if key == "Escape" && self.polyline.is_some() {
            return Some(self.finish_polyline(els));
        }
        let has_sel = !self.selection.is_empty();
        let k = key.to_ascii_lowercase();
        if (key == "Delete" || key == "Backspace") && has_sel {
            return Some(self.remove(els));
        }
        if mods.ctrl {
            match k.as_str() {
                "d" if has_sel => return Some(self.duplicate(els, ids)),
                "a" => {
                    self.selection = els.iter().map(|e| e.id().to_string()).collect();
                    return Some(Outcome::repaint());
                }
                "g" => return Some(if mods.shift { self.ungroup(els) } else { self.group(els, ids) }),
                "c" if has_sel => {
                    self.copy(els);
                    return Some(Outcome::default());
                }
                "x" if has_sel => {
                    self.copy(els);
                    return Some(self.remove(els));
                }
                "v" if !self.clipboard.is_empty() => return Some(self.paste(els, ids)),
                "]" if has_sel => return Some(self.arrange(els, if mods.shift { "front" } else { "forward" })),
                "[" if has_sel => return Some(self.arrange(els, if mods.shift { "back" } else { "backward" })),
                _ => {}
            }
        }
        if key == "Escape" && has_sel {
            self.selection.clear();
            return Some(Outcome::repaint());
        }
        if has_sel && key.starts_with("Arrow") {
            let step = if mods.shift { 0.02 } else { 0.004 };
            let (dx, dy) = match key {
                "ArrowLeft" => (-step, 0.0),
                "ArrowRight" => (step, 0.0),
                "ArrowUp" => (0.0, -step),
                "ArrowDown" => (0.0, step),
                _ => return None,
            };
            return Some(self.update_sel(els, |e| translated(e, dx, dy)));
        }
        None
    }

    // ── Commands (`CanvasApi` and the context menus) ────────────────────────

    fn sel_set(&self) -> HashSet<String> {
        self.selection.iter().cloned().collect()
    }

    /// Applies `f` to the selected elements.
    pub fn update_sel(&mut self, els: &mut [Element], f: impl Fn(&Element) -> Element) -> Outcome {
        let ids = self.sel_set();
        if ids.is_empty() {
            return Outcome::default();
        }
        let mut changed = false;
        for e in els.iter_mut() {
            if ids.contains(e.id()) {
                let n = f(e);
                if n != *e {
                    *e = n;
                    changed = true;
                }
            }
        }
        if changed {
            Outcome::changed()
        } else {
            Outcome::default()
        }
    }

    pub fn remove(&mut self, els: &mut Vec<Element>) -> Outcome {
        let ids = self.sel_set();
        if ids.is_empty() {
            return Outcome::default();
        }
        els.retain(|e| !ids.contains(e.id()));
        self.selection.clear();
        Outcome::changed()
    }

    pub fn copy(&mut self, els: &[Element]) {
        let ids = self.sel_set();
        self.clipboard = els.iter().filter(|e| ids.contains(e.id())).cloned().collect();
    }

    /// `pasteEl`: the clipboard +0.03 (capped at 0.92), new ids, on top.
    pub fn paste(&mut self, els: &mut Vec<Element>, ids: &mut dyn IdGen) -> Outcome {
        if self.clipboard.is_empty() {
            return Outcome::default();
        }
        let z = els.len() + 1;
        let news: Vec<Element> = self
            .clipboard
            .iter()
            .map(|e| {
                let dx = (e.x() + 0.03).min(0.92) - e.x();
                let dy = (e.y() + 0.03).min(0.92) - e.y();
                let mut n = translated(e, dx, dy);
                n.set_s("id", &ids.next_id());
                n.set_f("zIndex", z as f64);
                n
            })
            .collect();
        self.selection = news.iter().map(|n| n.id().to_string()).collect();
        els.extend(news);
        Outcome::changed()
    }

    /// `duplicateSel`.
    pub fn duplicate(&mut self, els: &mut Vec<Element>, ids: &mut dyn IdGen) -> Outcome {
        let sel = self.sel_set();
        let n = els.len();
        let mut gid_map: Vec<(String, String)> = Vec::new();
        let news: Vec<Element> = els
            .iter()
            .filter(|e| sel.contains(e.id()))
            .enumerate()
            .map(|(i, e)| {
                let dx = (e.x() + 0.03).min(0.92) - e.x();
                let dy = (e.y() + 0.03).min(0.92) - e.y();
                let mut c = translated(e, dx, dy);
                c.set_s("id", &ids.next_id());
                c.set_f("zIndex", (n + 1 + i) as f64);
                if let Some(g) = e.group_id().map(str::to_string) {
                    let ng = match gid_map.iter().find(|(o, _)| *o == g) {
                        Some((_, x)) => x.clone(),
                        None => {
                            let x = ids.next_id();
                            gid_map.push((g, x.clone()));
                            x
                        }
                    };
                    c.set_s("groupId", &ng);
                }
                c
            })
            .collect();
        if news.is_empty() {
            return Outcome::default();
        }
        self.selection = news.iter().map(|n| n.id().to_string()).collect();
        els.extend(news);
        Outcome::changed()
    }

    pub fn can_group(&self) -> bool {
        self.sel_set().len() >= 2
    }

    pub fn can_ungroup(&self, els: &[Element]) -> bool {
        els.iter().any(|e| self.selection.iter().any(|s| s == e.id()) && e.group_id().is_some())
    }

    pub fn group(&mut self, els: &mut [Element], ids: &mut dyn IdGen) -> Outcome {
        if !self.can_group() {
            return Outcome::default();
        }
        let gid = ids.next_id();
        self.update_sel(els, |e| {
            let mut n = e.clone();
            n.set_s("groupId", &gid);
            n
        })
    }

    pub fn ungroup(&mut self, els: &mut [Element]) -> Outcome {
        self.update_sel(els, |e| {
            let mut n = e.clone();
            if e.group_id().is_some() || e.get("groupId").is_some() {
                n.remove("groupId");
            }
            n
        })
    }

    /// `arrangeZ`: `front`, `back`, `forward`, `backward` — every element renumbered 0..n by the new order.
    pub fn arrange(&mut self, els: &mut [Element], op: &str) -> Outcome {
        let sel = self.sel_set();
        if sel.is_empty() {
            return Outcome::default();
        }
        let mut order: Vec<(String, f64)> = els.iter().map(|e| (e.id().to_string(), e.f("zIndex").filter(|z| *z != 0.0).unwrap_or(0.0))).collect();
        order.sort_by(|a, b| a.1.total_cmp(&b.1));
        let mut order: Vec<String> = order.into_iter().map(|o| o.0).collect();
        match op {
            "front" => {
                let (a, b): (Vec<String>, Vec<String>) = order.into_iter().partition(|i| !sel.contains(i));
                order = a.into_iter().chain(b).collect();
            }
            "back" => {
                let (a, b): (Vec<String>, Vec<String>) = order.into_iter().partition(|i| sel.contains(i));
                order = a.into_iter().chain(b).collect();
            }
            "forward" => {
                for i in (0..order.len().saturating_sub(1)).rev() {
                    if sel.contains(&order[i]) && !sel.contains(&order[i + 1]) {
                        order.swap(i, i + 1);
                    }
                }
            }
            "backward" => {
                for i in 1..order.len() {
                    if sel.contains(&order[i]) && !sel.contains(&order[i - 1]) {
                        order.swap(i, i - 1);
                    }
                }
            }
            _ => return Outcome::default(),
        }
        let mut changed = false;
        for e in els.iter_mut() {
            if let Some(z) = order.iter().position(|i| i == e.id()) {
                if e.f("zIndex") != Some(z as f64) {
                    e.set_f("zIndex", z as f64);
                    changed = true;
                }
            }
        }
        if changed {
            Outcome::changed()
        } else {
            Outcome::default()
        }
    }

    /// `alignSel`: on the common box (two or more) or the slide (one).
    pub fn align(&mut self, els: &mut [Element], mode: &str) -> Outcome {
        let sel = self.sel_set();
        if sel.is_empty() {
            return Outcome::default();
        }
        let (mut l, mut t, mut r, mut b) = (0.0, 0.0, 1.0, 1.0);
        let chosen: Vec<&Element> = els.iter().filter(|e| sel.contains(e.id())).collect();
        if chosen.len() >= 2 {
            if let Some(bb) = combined_bbox(chosen.iter().copied()) {
                (l, t, r, b) = (bb.x, bb.y, bb.x + bb.w, bb.y + bb.h);
            }
        }
        self.update_sel(els, |e| {
            let bb = e.bbox();
            let (mut dx, mut dy) = (0.0, 0.0);
            match mode {
                "left" => dx = l - bb.x,
                "center" => dx = (l + r) / 2.0 - (bb.x + bb.w / 2.0),
                "right" => dx = r - (bb.x + bb.w),
                "top" => dy = t - bb.y,
                "middle" => dy = (t + b) / 2.0 - (bb.y + bb.h / 2.0),
                "bottom" => dy = b - (bb.y + bb.h),
                _ => {}
            }
            translated(e, dx, dy)
        })
    }

    /// `distributeSel`: three or more, the left (top) edges evenly spaced.
    pub fn distribute(&mut self, els: &mut [Element], horizontal: bool) -> Outcome {
        let sel = self.sel_set();
        if sel.len() < 3 {
            return Outcome::default();
        }
        let mut arr: Vec<(String, Rect)> = els.iter().filter(|e| sel.contains(e.id())).map(|e| (e.id().to_string(), e.bbox())).collect();
        arr.sort_by(|a, b| if horizontal { a.1.x.total_cmp(&b.1.x) } else { a.1.y.total_cmp(&b.1.y) });
        let pos = |r: &Rect| if horizontal { r.x } else { r.y };
        let start = pos(&arr[0].1);
        let end = pos(&arr[arr.len() - 1].1);
        let gap = (end - start) / (arr.len() - 1) as f64;
        let moves: Vec<(String, f64)> = arr.iter().enumerate().map(|(i, (id, r))| (id.clone(), start + gap * i as f64 - pos(r))).collect();
        self.update_sel(els, |e| match moves.iter().find(|m| m.0 == e.id()) {
            Some((_, d)) => translated(e, if horizontal { *d } else { 0.0 }, if horizontal { 0.0 } else { *d }),
            None => e.clone(),
        })
    }

    /// `matchSize` (`w`, `h`, `both`): the others take the first selected element's size.
    pub fn match_size(&mut self, els: &mut [Element], mode: &str) -> Outcome {
        if self.selection.len() < 2 {
            return Outcome::default();
        }
        let Some(first) = els.iter().find(|e| e.id() == self.selection[0]).cloned() else { return Outcome::default() };
        if first.kind() == "line" {
            return Outcome::default();
        }
        let first_id = first.id().to_string();
        self.update_sel(els, |e| {
            if e.id() == first_id || e.kind() == "line" {
                return e.clone();
            }
            let mut n = e.clone();
            if mode != "h" {
                n.set_f("w", first.w());
            }
            if mode != "w" {
                n.set_f("h", first.h());
            }
            n
        })
    }

    /// `swapPositions` (exactly two).
    pub fn swap(&mut self, els: &mut [Element]) -> Outcome {
        if self.selection.len() != 2 {
            return Outcome::default();
        }
        let a = els.iter().find(|e| e.id() == self.selection[0]).map(Element::bbox);
        let b = els.iter().find(|e| e.id() == self.selection[1]).map(Element::bbox);
        let (Some(ba), Some(bb)) = (a, b) else { return Outcome::default() };
        let (ia, ib) = (self.selection[0].clone(), self.selection[1].clone());
        for e in els.iter_mut() {
            if e.id() == ia {
                *e = translated(e, bb.x - ba.x, bb.y - ba.y);
            } else if e.id() == ib {
                *e = translated(e, ba.x - bb.x, ba.y - bb.y);
            }
        }
        Outcome::changed()
    }

    pub fn rotate_by(&mut self, els: &mut [Element], deg: f64) -> Outcome {
        self.update_sel(els, |e| {
            let mut n = e.clone();
            n.set_f("rotation", (((e.rotation() + deg) % 360.0) + 360.0) % 360.0);
            n
        })
    }

    pub fn reset_rotation(&mut self, els: &mut [Element]) -> Outcome {
        self.update_sel(els, |e| {
            let mut n = e.clone();
            n.set_f("rotation", 0.0);
            n
        })
    }

    pub fn flip(&mut self, els: &mut [Element], horizontal: bool) -> Outcome {
        let key = if horizontal { "flipX" } else { "flipY" };
        self.update_sel(els, |e| {
            let mut n = e.clone();
            n.set_b(key, !e.truthy(key));
            n
        })
    }

    /// `centerSelOnSlide` (lines excluded); `axis`: `None` both, `Some(true)` horizontally, `Some(false)` vertically.
    pub fn center(&mut self, els: &mut [Element], axis: Option<bool>) -> Outcome {
        self.update_sel(els, |e| {
            if e.kind() == "line" && axis.is_none() {
                return e.clone();
            }
            let mut n = e.clone();
            if axis != Some(false) {
                n.set_f("x", (1.0 - e.w()) / 2.0);
            }
            if axis != Some(true) {
                n.set_f("y", (1.0 - e.h()) / 2.0);
            }
            n
        })
    }

    pub fn stretch(&mut self, els: &mut [Element], horizontal: bool) -> Outcome {
        self.update_sel(els, |e| {
            if e.kind() == "line" {
                return e.clone();
            }
            let mut n = e.clone();
            if horizontal {
                n.set_f("x", 0.0);
                n.set_f("w", 1.0);
            } else {
                n.set_f("y", 0.0);
                n.set_f("h", 1.0);
            }
            n
        })
    }

    /// Sets (or removes, `None`) one key on the selection.
    pub fn set_key(&mut self, els: &mut [Element], key: &str, v: Option<Value>) -> Outcome {
        self.update_sel(els, |e| {
            let mut n = e.clone();
            match &v {
                Some(v) => n.set(key, v.clone()),
                None => n.remove(key),
            }
            n
        })
    }

    /// Sets a key on the selected elements of one type only.
    pub fn set_key_of(&mut self, els: &mut [Element], kind: &str, key: &str, v: Option<Value>) -> Outcome {
        self.update_sel(els, |e| {
            if e.kind() != kind {
                return e.clone();
            }
            let mut n = e.clone();
            match &v {
                Some(v) => n.set(key, v.clone()),
                None => n.remove(key),
            }
            n
        })
    }

    /// Merges `patch` into an object key (`stroke: {...stroke, width}`) of the selected elements of `kind`.
    pub fn merge_key_of(&mut self, els: &mut [Element], kind: &str, key: &str, patch: Value) -> Outcome {
        self.update_sel(els, |e| {
            if e.kind() != kind {
                return e.clone();
            }
            let mut n = e.clone();
            let mut obj = e.get(key).and_then(Value::as_object).cloned().unwrap_or_default();
            if let Value::Object(p) = &patch {
                for (k, v) in p {
                    obj.insert(k.clone(), v.clone());
                }
            }
            n.set(key, Value::Object(obj));
            n
        })
    }

    pub fn toggle_lock(&mut self, els: &mut [Element]) -> Outcome {
        self.update_sel(els, |e| {
            let mut n = e.clone();
            n.set_b("locked", !e.locked());
            n
        })
    }

    pub fn hide(&mut self, els: &mut [Element]) -> Outcome {
        let o = self.update_sel(els, |e| {
            let mut n = e.clone();
            n.set_b("hidden", !e.hidden());
            n
        });
        self.selection.clear();
        o
    }

    pub fn set_opacity(&mut self, els: &mut [Element], v: f64) -> Outcome {
        let v = v.clamp(0.0, 1.0);
        self.update_sel(els, |e| {
            let mut n = e.clone();
            n.set_f("opacity", v);
            n
        })
    }

    pub fn toggle_shadow(&mut self, els: &mut [Element]) -> Outcome {
        self.update_sel(els, |e| {
            let mut n = e.clone();
            if e.truthy("shadow") {
                n.remove("shadow");
            } else {
                n.set_b("shadow", true);
            }
            n
        })
    }

    /// `setAnimSel`: `None` removes the entry animation.
    pub fn set_anim(&mut self, els: &mut [Element], kind: Option<&str>) -> Outcome {
        self.set_key(els, "anim", kind.map(|k| json!({ "type": k })))
    }

    /// `setAnimField`: merges `duration`/`delay` into the entry animation (type `fade` when none).
    pub fn set_anim_field(&mut self, els: &mut [Element], key: &str, ms: f64) -> Outcome {
        self.update_sel(els, |e| {
            let mut anim = Map::new();
            let cur = e.get("anim").and_then(Value::as_object);
            anim.insert("type".into(), cur.and_then(|a| a.get("type")).cloned().unwrap_or(json!("fade")));
            if let Some(c) = cur {
                for (k, v) in c {
                    anim.insert(k.clone(), v.clone());
                }
            }
            anim.insert(key.into(), num(ms));
            let mut n = e.clone();
            n.set("anim", Value::Object(anim));
            n
        })
    }

    pub fn set_anim_exit(&mut self, els: &mut [Element], kind: Option<&str>) -> Outcome {
        self.set_key(els, "animExit", kind.map(|k| json!({ "type": k })))
    }

    /// `animMeta` of the single selected element.
    pub fn anim_meta(&self, els: &[Element]) -> (String, f64, f64, String) {
        let e = self.single(els);
        let a = e.and_then(|e| e.get("anim"));
        let x = e.and_then(|e| e.get("animExit"));
        (
            a.and_then(|a| a.get("type")).and_then(Value::as_str).unwrap_or("none").to_string(),
            a.and_then(|a| as_f64(a.get("duration"))).unwrap_or(450.0),
            a.and_then(|a| as_f64(a.get("delay"))).unwrap_or(0.0),
            x.and_then(|a| a.get("type")).and_then(Value::as_str).unwrap_or("none").to_string(),
        )
    }

    /// `copyStyle` (« Reproduire la mise en forme »).
    pub fn copy_style(&mut self, els: &[Element]) {
        let Some(el) = els.iter().find(|e| Some(e.id()) == self.selection.first().map(String::as_str)) else { return };
        let mut st = Map::new();
        let put = |st: &mut Map<String, Value>, k: &str| {
            if let Some(v) = el.get(k) {
                st.insert(k.into(), v.clone());
            }
        };
        put(&mut st, "opacity");
        put(&mut st, "shadow");
        st.insert("type".into(), json!(el.kind()));
        match el.kind() {
            "text" => {
                for k in ["bold", "italic", "underline", "color", "fontFamily", "fontSize", "align", "background", "borderRadius"] {
                    put(&mut st, k);
                }
            }
            "shape" => {
                put(&mut st, "fill");
                put(&mut st, "stroke");
            }
            "line" => {
                put(&mut st, "stroke");
                put(&mut st, "arrowEnd");
            }
            _ => {}
        }
        self.style_clip = Some(st);
    }

    /// `pasteStyle`: the captured keys (never the type); only opacity and shadow on another type.
    pub fn paste_style(&mut self, els: &mut [Element]) -> Outcome {
        let Some(st) = self.style_clip.clone() else { return Outcome::default() };
        let kind = st.get("type").and_then(Value::as_str).unwrap_or("").to_string();
        self.update_sel(els, |e| {
            let mut n = e.clone();
            if !kind.is_empty() && kind != e.kind() {
                for k in ["opacity", "shadow"] {
                    match st.get(k) {
                        Some(v) => n.set(k, v.clone()),
                        None => n.remove(k),
                    }
                }
                return n;
            }
            for (k, v) in &st {
                if k != "type" {
                    n.set(k, v.clone());
                }
            }
            n
        })
    }

    /// « Sélectionner le même type ».
    pub fn select_same_type(&mut self, els: &[Element], id: &str) {
        if let Some(kind) = els.iter().find(|e| e.id() == id).map(|e| e.kind().to_string()) {
            self.selection = els.iter().filter(|e| e.kind() == kind).map(|e| e.id().to_string()).collect();
        }
    }

    /// `applyAutofit` (`none`, `shape`, `shrink`): `shape` sets the box's height from the measured text.
    pub fn set_autofit(&mut self, els: &mut [Element], mode: &str, text_height: impl Fn(&Element) -> f64) -> Outcome {
        self.update_sel(els, |e| {
            if e.kind() != "text" {
                return e.clone();
            }
            let mut n = e.clone();
            n.set_s("autofit", mode);
            if mode == "shape" {
                n.set_f("h", (text_height(e) / SLIDE_H).max(0.04));
            }
            n
        })
    }

    // ── Guides ──────────────────────────────────────────────────────────────

    pub fn add_guide(&mut self, vertical: bool, ids: &mut dyn IdGen) {
        self.guides.push(Guide { id: ids.next_id(), vertical, pos: 0.5 });
        self.show_guides = true;
    }
}

/// Sets an element's box (fractions).
pub fn set_box(e: &mut Element, b: Rect) {
    e.set_f("x", b.x);
    e.set_f("y", b.y);
    e.set_f("w", b.w);
    e.set_f("h", b.h);
}

/// The crop frame after dragging `handle` by `(dx, dy)` canvas px (min 24 px, inside the full image).
fn crop_drag(handle: &str, f: Rect, full: Rect, dx: f64, dy: f64) -> Rect {
    let clamp = |v: f64, lo: f64, hi: f64| v.max(lo).min(hi.max(lo));
    let (mut l, mut t, mut w, mut h) = (f.x, f.y, f.w, f.h);
    let min = 24.0;
    if handle == "move" {
        l = clamp(f.x + dx, full.x, full.x + full.w - w);
        t = clamp(f.y + dy, full.y, full.y + full.h - h);
    } else {
        if handle.contains('e') {
            w = clamp(f.w + dx, min, full.x + full.w - l);
        }
        if handle.contains('s') {
            h = clamp(f.h + dy, min, full.y + full.h - t);
        }
        if handle.contains('w') {
            let nl = clamp(f.x + dx, full.x, f.x + f.w - min);
            w = f.w + (f.x - nl);
            l = nl;
        }
        if handle.contains('n') {
            let nt = clamp(f.y + dy, full.y, f.y + f.h - min);
            h = f.h + (f.y - nt);
            t = nt;
        }
    }
    Rect { x: l, y: t, w, h }
}

/// The measured height (slide px) of a text box's text at its width (`measureTextHeightSlide` +
/// padding), for « Redimensionner la forme pour l'adapter au texte ».
pub fn measured_text_height(el: &Element, family: &str, m: &dyn crate::richtext::Measure) -> f64 {
    let txt = crate::richtext::paras_to_plain(&crate::richtext::doc_to_paras(el.get("content")));
    let txt = if txt.is_empty() { el.s("placeholder").unwrap_or("").to_string() } else { txt };
    let fs = el.f_or("fontSize", 24.0);
    let pad = el.f_or("padding", 8.0) * (el.w() * SLIDE_W / 100.0);
    let width = el.w() * SLIDE_W - 2.0 * pad;
    let style = crate::render::plain_style(family, fs, "#000");
    let mut lines = 0usize;
    let src = if txt.is_empty() { " ".to_string() } else { txt };
    for raw in src.split('\n') {
        let mut cur = String::new();
        let mut n = 1;
        for word in raw.split(' ') {
            let test = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
            if m.width(&test, &style, 0.0) > width && !cur.is_empty() {
                n += 1;
                cur = word.to_string();
            } else {
                cur = test;
            }
        }
        lines += n;
    }
    lines as f64 * fs * 1.3 + 2.0 * pad
}

/// The shape quick styles (`SHAPE_PRESETS`): label, fill, stroke.
pub fn shape_presets() -> Vec<(&'static str, Value, Value)> {
    let solid = |c: &str| json!({ "type": "color", "color": c });
    let stroke = |c: &str, w: f64, s: &str| json!({ "color": c, "width": num(w), "style": s });
    vec![
        ("Bleu plein", solid("#1a73e8"), stroke("#1557b0", 0.0, "solid")),
        ("Contour bleu", solid("#ffffff"), stroke("#1a73e8", 2.0, "solid")),
        ("Vert plein", solid("#34a853"), stroke("#0f9d58", 0.0, "solid")),
        ("Rouge plein", solid("#ea4335"), stroke("#c5221f", 0.0, "solid")),
        ("Jaune plein", solid("#fbbc04"), stroke("#f29900", 0.0, "solid")),
        ("Gris clair", solid("#f1f3f4"), stroke("#9aa0a6", 1.0, "solid")),
        ("Dégradé bleu", json!({ "type": "gradient", "grad": { "type": "linear", "angle": 90, "stops": [{ "color": "#4f9cff", "position": 0 }, { "color": "#1a56c4", "position": 1 }] } }), stroke("#1557b0", 0.0, "solid")),
        ("Pointillé", solid("#ffffff"), stroke("#5f6368", 2.0, "dashed")),
    ]
}

/// Truthiness shortcut for callers.
pub fn is_truthy(v: Option<&Value>) -> bool {
    truthy(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::insert::SeqIds;

    fn el(v: Value) -> Element {
        Element::from_value(v).unwrap_or_default()
    }

    fn rect(id: &str, x: f64, y: f64, w: f64, h: f64, z: f64) -> Element {
        el(json!({ "id": id, "type": "shape", "shape": "rect", "x": x, "y": y, "w": w, "h": h, "zIndex": z }))
    }

    #[test]
    fn a_click_selects_the_topmost_and_its_group() {
        let mut els = vec![rect("a", 0.1, 0.1, 0.3, 0.3, 1.0), rect("b", 0.2, 0.2, 0.3, 0.3, 2.0), el(json!({ "id": "c", "type": "shape", "x": 0.8, "y": 0.8, "w": 0.1, "h": 0.1, "groupId": "g" })), el(json!({ "id": "d", "type": "shape", "x": 0.0, "y": 0.9, "w": 0.05, "h": 0.05, "groupId": "g" }))];
        let mut ed = Editor::new();
        let mut ids = SeqIds::default();
        ed.pointer_down(&mut els, 0.25, 0.25, Mods::default(), 1.0, &mut ids);
        assert_eq!(ed.selection, ["b"]);
        ed.pointer_up(&mut els);
        ed.pointer_down(&mut els, 0.85, 0.85, Mods::default(), 1.0, &mut ids);
        assert_eq!(ed.selection, ["c", "d"]);
    }

    #[test]
    fn dragging_moves_and_snaps_to_the_slide_centre() {
        let mut els = vec![rect("a", 0.1, 0.1, 0.2, 0.2, 1.0)];
        let mut ed = Editor::new();
        let mut ids = SeqIds::default();
        ed.pointer_down(&mut els, 0.15, 0.15, Mods::default(), 1.0, &mut ids);
        // Box centre lands at 0.2 + 0.248 = 0.448… → snaps to the slide's centre 0.5 within 7 px? (0.05 × 960 = 48 px: no.)
        let o = ed.pointer_move(&mut els, 0.25, 0.15, Mods::default(), 1.0);
        assert!(o.changed);
        assert!((els[0].x() - 0.2).abs() < 1e-9);
        // x 0.298: the right edge 0.498 is 2 px from the slide centre 0.5 → snaps.
        ed.pointer_move(&mut els, 0.348, 0.15, Mods::default(), 1.0);
        assert!((els[0].x() + els[0].w() - 0.5).abs() < 1e-9, "the right edge snaps: x {}", els[0].x());
        assert_eq!(ed.snap_guides.len(), 1);
        ed.pointer_up(&mut els);
        assert!(ed.snap_guides.is_empty());
    }

    #[test]
    fn the_marquee_selects_what_it_touches() {
        let mut els = vec![rect("a", 0.1, 0.1, 0.1, 0.1, 1.0), rect("b", 0.6, 0.6, 0.1, 0.1, 2.0)];
        let mut ed = Editor::new();
        let mut ids = SeqIds::default();
        ed.pointer_down(&mut els, 0.0, 0.0, Mods::default(), 1.0, &mut ids);
        ed.pointer_move(&mut els, 0.15, 0.15, Mods::default(), 1.0);
        assert!(ed.marquee.is_some());
        ed.pointer_up(&mut els);
        assert_eq!(ed.selection, ["a"]);
    }

    #[test]
    fn resize_from_the_corner_keeps_the_ratio_with_shift() {
        let mut els = vec![rect("a", 0.1, 0.1, 0.2, 0.1, 1.0)];
        let mut ed = Editor::new();
        ed.select(vec!["a".into()]);
        let mut ids = SeqIds::default();
        // The south-east corner at canvas px (0.3 × 960, 0.2 × 540) = (288, 108), scale 1.
        ed.pointer_down(&mut els, 288.0 / 960.0, 108.0 / 540.0, Mods::default(), 1.0, &mut ids);
        ed.pointer_move(&mut els, 0.5, 0.2, Mods { shift: true, ..Mods::default() }, 1.0);
        assert!((els[0].w() / els[0].h() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn shapes_are_drawn_and_a_click_gets_the_default_size() {
        let mut els = Vec::new();
        let mut ed = Editor::new();
        ed.tool = Tool::Shape("ellipse".into());
        let mut ids = SeqIds::default();
        ed.pointer_down(&mut els, 0.5, 0.5, Mods::default(), 1.0, &mut ids);
        let o = ed.pointer_up(&mut els);
        assert!(o.tool_reset && o.changed);
        assert!((els[0].w() - 0.2).abs() < 1e-12 && (els[0].x() - 0.4).abs() < 1e-12);
        assert_eq!(ed.tool, Tool::Select);
    }

    #[test]
    fn a_polyline_takes_clicks_and_drops_its_ghost_point() {
        let mut els = Vec::new();
        let mut ed = Editor::new();
        ed.tool = Tool::Line("polyline".into());
        let mut ids = SeqIds::default();
        ed.pointer_down(&mut els, 0.1, 0.1, Mods::default(), 1.0, &mut ids);
        ed.pointer_move(&mut els, 0.3, 0.1, Mods::default(), 1.0);
        ed.pointer_down(&mut els, 0.3, 0.1, Mods::default(), 1.0, &mut ids);
        ed.pointer_move(&mut els, 0.3, 0.4, Mods::default(), 1.0);
        ed.finish_polyline(&mut els);
        assert_eq!(els[0].line_points(), [(0.1, 0.1), (0.3, 0.1)]);
    }

    #[test]
    fn z_order_renumbers_like_the_web() {
        let mut els = vec![rect("a", 0.0, 0.0, 0.1, 0.1, 1.0), rect("b", 0.0, 0.0, 0.1, 0.1, 2.0), rect("c", 0.0, 0.0, 0.1, 0.1, 3.0)];
        let mut ed = Editor::new();
        ed.select(vec!["a".into()]);
        ed.arrange(&mut els, "forward");
        assert_eq!(els.iter().map(|e| e.f("zIndex").unwrap_or(-1.0)).collect::<Vec<_>>(), [1.0, 0.0, 2.0]);
        ed.arrange(&mut els, "front");
        assert_eq!(els[0].f("zIndex"), Some(2.0));
    }

    #[test]
    fn align_distribute_and_nudge_move_every_line_point() {
        let mut els = vec![el(json!({ "id": "l", "type": "line", "x": 0.1, "y": 0.1, "x2": 0.2, "y2": 0.2, "points": [{ "x": 0.1, "y": 0.1 }, { "x": 0.2, "y": 0.2 }] }))];
        let mut ed = Editor::new();
        ed.select(vec!["l".into()]);
        let mut ids = SeqIds::default();
        ed.key(&mut els, "ArrowRight", Mods::default(), &mut ids, 1.0);
        assert!((els[0].line_points()[1].0 - 0.204).abs() < 1e-12);
        ed.align(&mut els, "left");
        assert!(els[0].bbox().x.abs() < 1e-12);
    }

    #[test]
    fn duplicate_and_paste_offset_and_rekey_groups() {
        let mut els = vec![el(json!({ "id": "a", "type": "shape", "x": 0.95, "y": 0.1, "w": 0.1, "h": 0.1, "groupId": "g" }))];
        let mut ed = Editor::new();
        ed.select(vec!["a".into()]);
        let mut ids = SeqIds::default();
        ed.duplicate(&mut els, &mut ids);
        assert_eq!(els.len(), 2);
        assert!((els[1].x() - 0.92).abs() < 1e-12, "min(0.92, x + 0.03)");
        assert!((els[1].y() - 0.13).abs() < 1e-12);
        assert_ne!(els[1].group_id(), Some("g"));
    }

    #[test]
    fn a_locked_element_is_selectable_but_does_not_move() {
        let mut els = vec![el(json!({ "id": "a", "type": "shape", "x": 0.1, "y": 0.1, "w": 0.2, "h": 0.2, "locked": true }))];
        let mut ed = Editor::new();
        let mut ids = SeqIds::default();
        ed.pointer_down(&mut els, 0.2, 0.2, Mods::default(), 1.0, &mut ids);
        assert_eq!(ed.selection, ["a"]);
        let o = ed.pointer_move(&mut els, 0.5, 0.5, Mods::default(), 1.0);
        assert!(!o.changed);
    }

    #[test]
    fn crop_writes_fractions_and_resizes_the_box() {
        let mut els = vec![el(json!({ "id": "i", "type": "image", "x": 0.0, "y": 0.0, "w": 0.5, "h": 0.5, "storagePath": "kbfile:1" }))];
        let mut ed = Editor::new();
        ed.enter_crop(&els, "i", 1.0);
        if let Some(c) = &mut ed.crop {
            c.frame = Rect { x: 120.0, y: 0.0, w: 240.0, h: 270.0 };
        }
        ed.confirm_crop(&mut els, 1.0);
        let c = els[0].get("crop").cloned().unwrap_or(Value::Null);
        assert_eq!(c, json!({ "x": 0.25, "y": 0, "w": 0.5, "h": 1 }));
        assert!((els[0].x() - 0.125).abs() < 1e-12 && (els[0].w() - 0.25).abs() < 1e-12);
    }

    #[test]
    fn style_paste_respects_the_type() {
        let mut els = vec![el(json!({ "id": "s", "type": "shape", "fill": { "type": "color", "color": "#f00" }, "opacity": 0.5 })), el(json!({ "id": "t", "type": "text" }))];
        let mut ed = Editor::new();
        ed.select(vec!["s".into()]);
        ed.copy_style(&els);
        ed.select(vec!["t".into()]);
        ed.paste_style(&mut els);
        assert_eq!(els[1].f("opacity"), Some(0.5));
        assert!(els[1].get("fill").is_none());
    }
}
