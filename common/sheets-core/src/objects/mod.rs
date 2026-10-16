//! The sheet's floating objects - charts, pictures, shapes and equations - as the web models them
//! (`web/src/sheet-object/*`, `web/src/sheet-chart/interaction.ts`): references, selection, direct
//! manipulation (hit-test, the eight handles and the rotation knob, move/resize/rotate), alignment and paint
//! order. Platform-neutral: a desktop painter draws the chrome from [`handle_points`] / [`knob_point`] and a
//! pointer handler applies [`apply_drag`] / [`rotation_for`].
//!
//! Every object keeps the geometry contract of the web: an explicit box in base pixels (zoom 1) measured from
//! the grid's data origin (`bx`, `by`, `bw`, `bh`) plus a rotation in degrees (`rot`); imported charts and
//! pictures may instead carry an xlsx cell anchor (EMU offsets) until they are first manipulated.
//!
//! The objects themselves stay in the sheet's JSON (`charts`, `images`, `shapes`, `equations` of the sheet
//! data) exactly as stored: [`ObjectsMut`] edits them in place so unknown fields round-trip untouched.

pub mod order;

use serde_json::{json, Map, Value as Json};

use crate::addr::{MAX_COLS, MAX_ROWS};
use crate::grid::{Geometry, COL_HEADER_HEIGHT, ROW_HEADER_WIDTH};

/// The kinds of floating object (`ObjKind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ObjKind {
    Chart,
    Image,
    Shape,
    Equation,
}

impl ObjKind {
    /// The sheet-data key of the kind's array.
    pub fn key(self) -> &'static str {
        match self {
            ObjKind::Chart => "charts",
            ObjKind::Image => "images",
            ObjKind::Shape => "shapes",
            ObjKind::Equation => "equations",
        }
    }

    /// All kinds, in `selectionKinds` order.
    pub const ALL: [ObjKind; 4] = [ObjKind::Chart, ObjKind::Image, ObjKind::Shape, ObjKind::Equation];

    /// Paint order of the kinds on the web (pictures on the grid canvas, then the DOM bands: charts 7,
    /// equations 8, shapes 9). Hit-tests run in the reverse order.
    pub const PAINT: [ObjKind; 4] = [ObjKind::Image, ObjKind::Chart, ObjKind::Equation, ObjKind::Shape];
}

/// Identity of one object (`ObjRef`). Charts, shapes and equations carry an `id`; pictures are addressed by
/// their index in `images` (`imageRef`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ObjRef {
    pub kind: ObjKind,
    pub id: String,
}

impl ObjRef {
    pub fn new(kind: ObjKind, id: impl Into<String>) -> Self {
        Self { kind, id: id.into() }
    }

    pub fn image(index: usize) -> Self {
        Self { kind: ObjKind::Image, id: index.to_string() }
    }

    /// The array index behind a picture ref (`refIndex`).
    pub fn index(&self) -> Option<usize> {
        (self.kind == ObjKind::Image).then(|| self.id.parse().ok()).flatten()
    }
}

/// A content-space rectangle (zoom applied, origin = the grid's top-left including the headers) + rotation.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ObjRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub rot: f64,
}

/// A persisted box in base pixels from the data origin.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ObjBox {
    pub bx: f64,
    pub by: f64,
    pub bw: f64,
    pub bh: f64,
}

/// Which part of an object is under the pointer (`ObjHandle`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    Move,
    Rotate,
    NW,
    N,
    NE,
    W,
    E,
    SW,
    S,
    SE,
}

impl Handle {
    fn has(self, c: char) -> bool {
        let s = match self {
            Handle::NW => "nw",
            Handle::N => "n",
            Handle::NE => "ne",
            Handle::W => "w",
            Handle::E => "e",
            Handle::SW => "sw",
            Handle::S => "s",
            Handle::SE => "se",
            _ => "",
        };
        s.contains(c)
    }

    pub fn is_edge(self) -> bool {
        matches!(self, Handle::N | Handle::S | Handle::W | Handle::E)
    }
}

pub const HANDLE_HIT: f64 = 7.0;
pub const CHROME_BORDER: f64 = 2.0;
pub const CHROME_GAP: f64 = 2.0;
pub const CORNER_R: f64 = 5.0;
pub const PILL_LONG: f64 = 10.0;
pub const PILL_SHORT: f64 = 4.0;
pub const ROTATE_KNOB_R: f64 = 10.0;
pub const ROTATE_ARM: f64 = 24.0;
pub const MIN_SIDE: f64 = 24.0;
/// 1 px = 9525 EMU.
pub const EMU_PER_PX: f64 = 9525.0;
/// The accent of the selection chrome.
pub const ACCENT: &str = "#1a73e8";

/// `handlePoints`: the eight resize handles (rotation not applied).
pub fn handle_points(r: &ObjRect) -> [(Handle, f64, f64); 8] {
    let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
    let (l, t) = (r.x - CHROME_GAP, r.y - CHROME_GAP);
    let (rt, b) = (r.x + r.w + CHROME_GAP, r.y + r.h + CHROME_GAP);
    [(Handle::NW, l, t), (Handle::N, cx, t), (Handle::NE, rt, t), (Handle::W, l, cy), (Handle::E, rt, cy), (Handle::SW, l, b), (Handle::S, cx, b), (Handle::SE, rt, b)]
}

/// `handleExtent`: half-extents of a handle's hit area.
pub fn handle_extent(h: Handle) -> (f64, f64) {
    match h {
        Handle::N | Handle::S => (PILL_LONG, PILL_SHORT),
        Handle::W | Handle::E => (PILL_SHORT, PILL_LONG),
        _ => (CORNER_R, CORNER_R),
    }
}

/// `toLocal`: a content point in the object's unrotated frame.
pub fn to_local(r: &ObjRect, px: f64, py: f64) -> (f64, f64) {
    let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
    let a = -r.rot * std::f64::consts::PI / 180.0;
    let (dx, dy) = (px - cx, py - cy);
    (cx + dx * a.cos() - dy * a.sin(), cy + dx * a.sin() + dy * a.cos())
}

/// `knobPoint`: the rotation knob (in the object's own frame) and whether it flipped below the object.
pub fn knob_point(r: &ObjRect, min_top: f64) -> (f64, f64, bool) {
    let kx = r.x + r.w / 2.0;
    let below = r.y - CHROME_GAP - ROTATE_ARM - ROTATE_KNOB_R < min_top;
    let ky = if below { r.y + r.h + CHROME_GAP + ROTATE_ARM + ROTATE_KNOB_R } else { r.y - CHROME_GAP - ROTATE_ARM - ROTATE_KNOB_R };
    (kx, ky, below)
}

/// `hitObject`.
pub fn hit_object(r: &ObjRect, px: f64, py: f64, selected: bool, min_top: f64) -> Option<Handle> {
    if r.w <= 0.0 || r.h <= 0.0 {
        return None;
    }
    let (lx, ly) = to_local(r, px, py);
    if selected {
        let (kx, ky, _) = knob_point(r, min_top);
        if (lx - kx).hypot(ly - ky) <= ROTATE_KNOB_R + 2.0 {
            return Some(Handle::Rotate);
        }
        for (h, hx, hy) in handle_points(r) {
            let (ex, ey) = handle_extent(h);
            if (lx - hx).abs() <= ex + 2.0 && (ly - hy).abs() <= ey + 2.0 {
                return Some(h);
            }
        }
    }
    (lx >= r.x && lx <= r.x + r.w && ly >= r.y && ly <= r.y + r.h).then_some(Handle::Move)
}

/// The mouse cursor of a handle (`cursorFor`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cursor {
    Move,
    Grab,
    NS,
    NESW,
    EW,
    NWSE,
}

pub fn cursor_for(h: Handle, rot: f64) -> Cursor {
    let base = match h {
        Handle::Move => return Cursor::Move,
        Handle::Rotate => return Cursor::Grab,
        Handle::N => 0.0,
        Handle::NE => 45.0,
        Handle::E => 90.0,
        Handle::SE => 135.0,
        Handle::S => 180.0,
        Handle::SW => 225.0,
        Handle::W => 270.0,
        Handle::NW => 315.0,
    };
    let a = (((base + rot) % 180.0) + 180.0) % 180.0;
    if !(22.5..157.5).contains(&a) {
        Cursor::NS
    } else if a < 67.5 {
        Cursor::NESW
    } else if a < 112.5 {
        Cursor::EW
    } else {
        Cursor::NWSE
    }
}

/// `applyDrag`: a pointer delta (content px) applied to a box along the object's local axes.
pub fn apply_drag(b: ObjBox, h: Handle, wdx: f64, wdy: f64, rot: f64, zoom: f64) -> ObjBox {
    if h == Handle::Move {
        return ObjBox { bx: b.bx + wdx / zoom, by: b.by + wdy / zoom, ..b };
    }
    let a = -rot * std::f64::consts::PI / 180.0;
    let ldx = wdx * a.cos() - wdy * a.sin();
    let ldy = wdx * a.sin() + wdy * a.cos();
    let mut out = b;
    if h.has('e') {
        out.bw = MIN_SIDE.max(b.bw + ldx / zoom);
    }
    if h.has('s') {
        out.bh = MIN_SIDE.max(b.bh + ldy / zoom);
    }
    if h.has('w') {
        let n = MIN_SIDE.max(b.bw - ldx / zoom);
        out.bx = b.bx + (b.bw - n);
        out.bw = n;
    }
    if h.has('n') {
        let n = MIN_SIDE.max(b.bh - ldy / zoom);
        out.by = b.by + (b.bh - n);
        out.bh = n;
    }
    out
}

/// `rotationFor`: degrees for a pointer around the centre (0 = knob up), snapped to 15° when asked.
pub fn rotation_for(cx: f64, cy: f64, px: f64, py: f64, snap: bool) -> f64 {
    let deg = (py - cy).atan2(px - cx) * 180.0 / std::f64::consts::PI + 90.0;
    let r = crate::js::round(deg);
    if snap {
        crate::js::round(r / 15.0) * 15.0
    } else {
        r
    }
}

// ── Selection (`sheet-object/selection.ts`) ─────────────────────────────────

// ── Lines and connectors (`shapes/lines.ts`) ─────────────────────────────────

/// The kinds drawn as a stroke: no frame, two grabbable ends (`isLineShape`).
pub const LINE_KINDS: [&str; 10] = ["line", "lineArrow", "lineDouble", "elbowConnector", "elbowArrow", "elbowDoubleArrow", "curveConnector", "curveArrow", "curveDoubleArrow", "curve"];

pub fn is_line_shape(kind: &str) -> bool {
    LINE_KINDS.contains(&kind)
}

/// `lineEnds`: the two grabbable ends of a line shape (as resize handles).
pub fn line_ends(kind: &str, r: &ObjRect) -> [(Handle, f64, f64); 2] {
    let (l, t, right, b) = (r.x, r.y, r.x + r.w, r.y + r.h);
    if kind == "curve" {
        [(Handle::SW, l, b), (Handle::SE, right, b)]
    } else {
        [(Handle::SW, l, b), (Handle::NE, right, t)]
    }
}

fn dist_to_segment(px: f64, py: f64, ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let (dx, dy) = (bx - ax, by - ay);
    let len2 = dx * dx + dy * dy;
    let u = if len2 > 0.0 { (((px - ax) * dx + (py - ay) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    (px - (ax + u * dx)).hypot(py - (ay + u * dy))
}

/// `hitLineShape`.
pub fn hit_line_shape(kind: &str, r: &ObjRect, px: f64, py: f64, selected: bool) -> Option<Handle> {
    let slop = 7.0;
    if selected {
        for (h, x, y) in line_ends(kind, r) {
            if (px - x).abs() <= CORNER_R + slop / 2.0 && (py - y).abs() <= CORNER_R + slop / 2.0 {
                return Some(h);
            }
        }
    }
    let in_box = px >= r.x - slop && px <= r.x + r.w + slop && py >= r.y - slop && py <= r.y + r.h + slop;
    if !in_box {
        return None;
    }
    if matches!(kind, "line" | "lineArrow" | "lineDouble") {
        return (dist_to_segment(px, py, r.x, r.y + r.h, r.x + r.w, r.y) <= slop).then_some(Handle::Move);
    }
    Some(Handle::Move)
}

/// `clickSelection`.
pub fn click_selection(sel: &[ObjRef], r: &ObjRef, additive: bool) -> Vec<ObjRef> {
    let inside = sel.contains(r);
    if additive {
        return if inside { sel.iter().filter(|x| *x != r).cloned().collect() } else { sel.iter().cloned().chain([r.clone()]).collect() };
    }
    if sel.len() > 1 && inside {
        return sel.iter().filter(|x| *x != r).cloned().chain([r.clone()]).collect();
    }
    vec![r.clone()]
}

/// `selectionKinds`.
pub fn selection_kinds(sel: &[ObjRef]) -> Vec<ObjKind> {
    ObjKind::ALL.into_iter().filter(|k| sel.iter().any(|r| r.kind == *k)).collect()
}

/// `soleKind`.
pub fn sole_kind(sel: &[ObjRef]) -> Option<ObjKind> {
    let k = selection_kinds(sel);
    (k.len() == 1).then(|| k[0])
}

/// Alignment modes (`AlignMode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignMode {
    Left,
    CenterH,
    Right,
    Top,
    MiddleV,
    Bottom,
    DistH,
    DistV,
}

/// `boundingBox`.
pub fn bounding_box(boxes: &[ObjBox]) -> Option<ObjBox> {
    if boxes.is_empty() {
        return None;
    }
    let (mut x1, mut y1, mut x2, mut y2) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for b in boxes {
        x1 = x1.min(b.bx);
        y1 = y1.min(b.by);
        x2 = x2.max(b.bx + b.bw);
        y2 = y2.max(b.by + b.bh);
    }
    Some(ObjBox { bx: x1, by: y1, bw: x2 - x1, bh: y2 - y1 })
}

/// `alignBoxes`: new top-left corners, in input order.
pub fn align_boxes(boxes: &[ObjBox], mode: AlignMode) -> Vec<(f64, f64)> {
    let Some(bb) = bounding_box(boxes) else { return Vec::new() };
    let mut out: Vec<(f64, f64)> = boxes.iter().map(|b| (b.bx, b.by)).collect();
    if matches!(mode, AlignMode::DistH | AlignMode::DistV) {
        if boxes.len() < 3 {
            return out;
        }
        let horiz = mode == AlignMode::DistH;
        let mut idx: Vec<usize> = (0..boxes.len()).collect();
        idx.sort_by(|a, b| {
            let (p, q) = if horiz { (boxes[*a].bx, boxes[*b].bx) } else { (boxes[*a].by, boxes[*b].by) };
            p.partial_cmp(&q).unwrap_or(std::cmp::Ordering::Equal)
        });
        let span = if horiz { bb.bw } else { bb.bh };
        let sizes: f64 = idx.iter().map(|i| if horiz { boxes[*i].bw } else { boxes[*i].bh }).sum();
        let gap = (span - sizes) / (idx.len() - 1) as f64;
        let mut cursor = if horiz { bb.bx } else { bb.by };
        for i in idx {
            if horiz {
                out[i].0 = cursor;
                cursor += boxes[i].bw + gap;
            } else {
                out[i].1 = cursor;
                cursor += boxes[i].bh + gap;
            }
        }
        return out;
    }
    for (i, b) in boxes.iter().enumerate() {
        match mode {
            AlignMode::Left => out[i].0 = bb.bx,
            AlignMode::CenterH => out[i].0 = bb.bx + (bb.bw - b.bw) / 2.0,
            AlignMode::Right => out[i].0 = bb.bx + bb.bw - b.bw,
            AlignMode::Top => out[i].1 = bb.by,
            AlignMode::MiddleV => out[i].1 = bb.by + (bb.bh - b.bh) / 2.0,
            AlignMode::Bottom => out[i].1 = bb.by + bb.bh - b.bh,
            _ => {}
        }
    }
    out
}

// ── Geometry of a stored object ─────────────────────────────────────────────

fn f(o: &Json, k: &str) -> Option<f64> {
    o.get(k).and_then(Json::as_f64)
}

/// The content-space rectangle of a stored object at the current zoom (`chartRect`, `imageRect`,
/// `shapeRect`, `equationRect`). `natural` is an equation's laid-out size (zoom 1) when it has no box.
pub fn rect_of(kind: ObjKind, o: &Json, g: &Geometry, natural: Option<(f64, f64)>) -> ObjRect {
    let z = g.zoom;
    let rot = f(o, "rot").unwrap_or(0.0);
    let (bx, by, bw, bh) = (f(o, "bx"), f(o, "by"), f(o, "bw"), f(o, "bh"));
    if let (Some(bx), Some(by), Some(bw), Some(bh)) = (bx, by, bw, bh) {
        return ObjRect { x: ROW_HEADER_WIDTH + bx * z, y: COL_HEADER_HEIGHT + by * z, w: bw * z, h: bh * z, rot };
    }
    match kind {
        ObjKind::Chart | ObjKind::Image if f(o, "fromCol").is_some() && f(o, "fromRow").is_some() => {
            let fc = f(o, "fromCol").unwrap_or(0.0).clamp(0.0, MAX_COLS as f64) as u32;
            let fr = (f(o, "fromRow").unwrap_or(0.0) + 1.0).clamp(1.0, MAX_ROWS as f64) as u32;
            let x = g.col_left(fc) + f(o, "fromColOff").unwrap_or(0.0) / EMU_PER_PX * z;
            let y = g.row_top(fr) + f(o, "fromRowOff").unwrap_or(0.0) / EMU_PER_PX * z;
            let (w, h) = match (f(o, "toCol"), f(o, "toRow")) {
                (Some(tc), Some(tr)) => {
                    let tc = tc.clamp(0.0, MAX_COLS as f64) as u32;
                    let tr = (tr + 1.0).clamp(1.0, MAX_ROWS as f64) as u32;
                    (g.col_left(tc) + f(o, "toColOff").unwrap_or(0.0) / EMU_PER_PX * z - x, g.row_top(tr) + f(o, "toRowOff").unwrap_or(0.0) / EMU_PER_PX * z - y)
                }
                _ => (f(o, "extCx").unwrap_or(0.0) / EMU_PER_PX * z, f(o, "extCy").unwrap_or(0.0) / EMU_PER_PX * z),
            };
            ObjRect { x, y, w, h, rot }
        }
        ObjKind::Chart => ObjRect {
            x: ROW_HEADER_WIDTH + bx.unwrap_or(0.0) * z,
            y: COL_HEADER_HEIGHT + by.unwrap_or(0.0) * z,
            w: bw.unwrap_or(380.0) * z,
            h: bh.unwrap_or(240.0) * z,
            rot,
        },
        ObjKind::Equation => {
            let (nw, nh) = natural.unwrap_or((120.0, 40.0));
            ObjRect { x: ROW_HEADER_WIDTH + bx.unwrap_or(0.0) * z, y: COL_HEADER_HEIGHT + by.unwrap_or(0.0) * z, w: bw.unwrap_or(nw) * z, h: bh.unwrap_or(nh) * z, rot }
        }
        _ => ObjRect { x: ROW_HEADER_WIDTH + bx.unwrap_or(0.0) * z, y: COL_HEADER_HEIGHT + by.unwrap_or(0.0) * z, w: bw.unwrap_or(0.0) * z, h: bh.unwrap_or(0.0) * z, rot },
    }
}

/// A content rect back to a persisted box (`rectToBox`).
pub fn rect_to_box(r: &ObjRect, zoom: f64) -> ObjBox {
    ObjBox { bx: (r.x - ROW_HEADER_WIDTH) / zoom, by: (r.y - COL_HEADER_HEIGHT) / zoom, bw: r.w / zoom, bh: r.h / zoom }
}

/// Writes a box (and rotation) into a stored object, keeping its other fields.
pub fn write_box(o: &mut Json, b: ObjBox, rot: Option<f64>) {
    if let Some(m) = o.as_object_mut() {
        for (k, v) in [("bx", b.bx), ("by", b.by), ("bw", b.bw), ("bh", b.bh)] {
            m.insert(k.into(), crate::model::cell::json_number(v));
        }
        if let Some(r) = rot {
            m.insert("rot".into(), crate::model::cell::json_number(r));
        }
    }
}

/// Merges `patch` (an object of fields; `null` removes) into a stored object.
pub fn merge_patch(o: &mut Json, patch: &Map<String, Json>) {
    if let Some(m) = o.as_object_mut() {
        for (k, v) in patch {
            if v.is_null() {
                m.remove(k);
            } else {
                m.insert(k.clone(), v.clone());
            }
        }
    }
}

/// A fresh object id (`uid()` of the web: a short random base-36 string).
pub fn new_id(seed: u64) -> String {
    let mut x = seed ^ 0x9E37_79B9_7F4A_7C15;
    let mut s = String::new();
    for _ in 0..9 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let d = (x % 36) as u32;
        s.push(std::char::from_digit(d, 36).unwrap_or('0'));
    }
    s
}

/// The ref of every stored object of a sheet, in paint order (bottom first).
pub fn paint_order(data: &Map<String, Json>) -> Vec<ObjRef> {
    let mut out = Vec::new();
    for k in ObjKind::PAINT {
        if let Some(a) = data.get(k.key()).and_then(Json::as_array) {
            for (i, o) in a.iter().enumerate() {
                match k {
                    ObjKind::Image => out.push(ObjRef::image(i)),
                    _ => {
                        if let Some(id) = o.get("id").and_then(Json::as_str) {
                            out.push(ObjRef::new(k, id));
                        }
                    }
                }
            }
        }
    }
    out
}

/// Finds a stored object by its ref (its index in its kind's array).
pub fn find_index(data: &Map<String, Json>, r: &ObjRef) -> Option<usize> {
    let a = data.get(r.kind.key())?.as_array()?;
    match r.kind {
        ObjKind::Image => r.index().filter(|i| *i < a.len()),
        _ => a.iter().position(|o| o.get("id").and_then(Json::as_str) == Some(r.id.as_str())),
    }
}

/// A stored object by its ref.
pub fn get<'a>(data: &'a Map<String, Json>, r: &ObjRef) -> Option<&'a Json> {
    let i = find_index(data, r)?;
    data.get(r.kind.key())?.as_array()?.get(i)
}

/// A stored object by its ref, mutably.
pub fn get_mut<'a>(data: &'a mut Map<String, Json>, r: &ObjRef) -> Option<&'a mut Json> {
    let i = find_index(data, r)?;
    data.get_mut(r.kind.key())?.as_array_mut()?.get_mut(i)
}

/// The kind's array, created when missing (`None` only if the sheet holds a non-array under the key).
pub fn array_mut(data: &mut Map<String, Json>, k: ObjKind) -> Option<&mut Vec<Json>> {
    data.entry(k.key()).or_insert_with(|| json!([])).as_array_mut()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_and_drags_follow_the_web() {
        let r = ObjRect { x: 100.0, y: 100.0, w: 200.0, h: 100.0, rot: 0.0 };
        assert_eq!(hit_object(&r, 150.0, 150.0, false, 24.0), Some(Handle::Move));
        assert_eq!(hit_object(&r, 98.0, 98.0, true, 24.0), Some(Handle::NW));
        assert_eq!(hit_object(&r, 200.0, 98.0 - 24.0 - 10.0, true, 24.0), Some(Handle::Rotate));
        assert_eq!(hit_object(&r, 50.0, 50.0, true, 24.0), None);
        let b = ObjBox { bx: 10.0, by: 10.0, bw: 100.0, bh: 50.0 };
        assert_eq!(apply_drag(b, Handle::SE, 20.0, 10.0, 0.0, 1.0), ObjBox { bx: 10.0, by: 10.0, bw: 120.0, bh: 60.0 });
        assert_eq!(apply_drag(b, Handle::NW, 200.0, 0.0, 0.0, 1.0).bw, MIN_SIDE);
        assert_eq!(rotation_for(0.0, 0.0, 10.0, 0.0, false), 90.0);
        assert_eq!(rotation_for(0.0, 0.0, 10.0, 1.0, true), 90.0);
        assert_eq!(cursor_for(Handle::E, 90.0), Cursor::NS);
        // The knob flips below an object near the top.
        let top = ObjRect { y: 30.0, ..r };
        assert!(knob_point(&top, 24.0).2);
    }

    #[test]
    fn selection_and_alignment() {
        let a = ObjRef::new(ObjKind::Chart, "a");
        let b = ObjRef::image(0);
        let s = click_selection(&[], &a, false);
        let s = click_selection(&s, &b, true);
        assert_eq!(s, vec![a.clone(), b.clone()]);
        assert_eq!(click_selection(&s, &a, false), vec![b.clone(), a.clone()]);
        assert_eq!(click_selection(&s, &a, true), vec![b.clone()]);
        assert_eq!(sole_kind(&s), None);
        let boxes = [ObjBox { bx: 0.0, by: 0.0, bw: 10.0, bh: 10.0 }, ObjBox { bx: 50.0, by: 5.0, bw: 10.0, bh: 10.0 }, ObjBox { bx: 20.0, by: 0.0, bw: 10.0, bh: 10.0 }];
        assert_eq!(align_boxes(&boxes, AlignMode::Right)[0], (50.0, 0.0));
        assert_eq!(align_boxes(&boxes, AlignMode::DistH)[2], (25.0, 0.0));
    }
}
