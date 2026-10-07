//! The editor's floating-object commands (`web/src/SpreadsheetApp.tsx` 1029-2580): insertion and placement
//! of charts, pictures and shapes, move/resize/rotate, delete, duplicate, paint order, alignment, the
//! object clipboard. Each command edits the sheet's stored arrays in place (unknown fields kept) and marks
//! the workbook changed; like the web, objects are not part of the undo history (`HistSnap`).
//!
//! The cell selection `cells` passed to the placement commands is the web's `bounds()`: `None` when an
//! object, not a cell range, is selected.

use serde_json::{json, Map, Value as Json};

use super::Editor;
use crate::addr::{index_to_col, Rect, MAX_COLS, MAX_ROWS};
use crate::grid::{COL_HEADER_HEIGHT, ROW_HEADER_WIDTH};
use crate::model::cell::json_number;
use crate::objects::order::{reorder_at, reorder_many, OrderOp};
use crate::objects::{self as obj, ObjBox, ObjKind, ObjRect, ObjRef};

/// Default box of a new chart (`insertChartObj`).
pub const CHART_W: f64 = 380.0;
pub const CHART_H: f64 = 240.0;
/// Default box of a shape inserted from the gallery (`DEFAULT_SHAPE_WIDTH`/`HEIGHT`).
pub const SHAPE_W: f64 = 160.0;
pub const SHAPE_H: f64 = 96.0;
/// A shape's factory style (`DEFAULT_SHAPE_STYLE`).
pub const SHAPE_FILL: &str = "#ffffff";
pub const SHAPE_BORDER: &str = "#000000";

/// One copied object (`ObjClipItem`).
#[derive(Clone, Debug, PartialEq)]
pub struct ClipObj {
    pub kind: ObjKind,
    pub data: Json,
    pub source: Option<ObjRef>,
}

/// The object clipboard (`ObjClipEntry`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ObjClipboard {
    pub items: Vec<ClipObj>,
    pub cut: bool,
}

impl Editor {
    /// The active sheet's stored data (`charts`, `images`, `shapes`, `equations` among it).
    pub fn sheet_data(&self) -> &Map<String, Json> {
        &self.sheet().data
    }

    /// One stored object.
    pub fn object(&self, r: &ObjRef) -> Option<&Json> {
        obj::get(self.sheet_data(), r)
    }

    /// The stored objects of a kind.
    pub fn objects(&self, k: ObjKind) -> &[Json] {
        self.sheet_data().get(k.key()).and_then(Json::as_array).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Every object, bottom first (the web's paint order).
    pub fn objects_in_paint_order(&self) -> Vec<ObjRef> {
        obj::paint_order(self.sheet_data())
    }

    /// The content rect of an object at the current zoom. `natural` = an equation's natural size.
    pub fn object_rect(&self, r: &ObjRef, natural: Option<(f64, f64)>) -> Option<ObjRect> {
        let o = self.object(r)?;
        Some(obj::rect_of(r.kind, o, &self.geometry(), natural))
    }

    /// The persisted box of an object (its anchor baked into a box) and rotation (`objBox`).
    pub fn object_box(&self, r: &ObjRef, natural: Option<(f64, f64)>) -> Option<(ObjBox, f64)> {
        let rect = self.object_rect(r, natural)?;
        Some((obj::rect_to_box(&rect, self.zoom), rect.rot))
    }

    /// The topmost object under a content point and the part hit (shapes, then equations, charts, pictures,
    /// each array from its last element — the web's hit-test chain).
    pub fn hit_object(&self, px: f64, py: f64, selected: &[ObjRef], natural: &dyn Fn(&ObjRef) -> Option<(f64, f64)>) -> Option<(ObjRef, obj::Handle)> {
        let order = self.objects_in_paint_order();
        for r in order.iter().rev() {
            let Some(rect) = self.object_rect(r, natural(r)) else { continue };
            if let Some(h) = obj::hit_object(&rect, px, py, selected.contains(r), COL_HEADER_HEIGHT) {
                return Some((r.clone(), h));
            }
        }
        None
    }

    /// Edits the stored arrays and marks the workbook changed.
    pub fn mutate_objects<R>(&mut self, f: impl FnOnce(&mut Map<String, Json>) -> R) -> R {
        let r = f(&mut self.sheet_mut().data);
        self.touch();
        r
    }

    /// Bakes a box (+ rotation) into an object (`patchChart({...box, rot})`, the end of a drag).
    pub fn set_object_box(&mut self, r: &ObjRef, b: ObjBox, rot: Option<f64>) {
        self.mutate_objects(|d| {
            if let Some(o) = obj::get_mut(d, r) {
                if r.kind == ObjKind::Shape || r.kind == ObjKind::Equation {
                    // Shapes and equations carry no anchor: only their box fields move.
                    obj::write_box(o, b, rot);
                } else {
                    obj::write_box(o, b, rot.or_else(|| o.get("rot").and_then(Json::as_f64)).or(Some(0.0)));
                }
            }
        });
    }

    /// Moves several objects by a base-px delta from their snapshot boxes (`startGroupMove`).
    pub fn move_objects(&mut self, snap: &[(ObjRef, ObjBox, f64)], dx: f64, dy: f64) {
        self.mutate_objects(|d| {
            for (r, b, rot) in snap {
                if let Some(o) = obj::get_mut(d, r) {
                    let nb = ObjBox { bx: b.bx + dx, by: b.by + dy, ..*b };
                    match r.kind {
                        ObjKind::Shape | ObjKind::Equation => {
                            if let Some(m) = o.as_object_mut() {
                                m.insert("bx".into(), json_number(nb.bx));
                                m.insert("by".into(), json_number(nb.by));
                            }
                        }
                        ObjKind::Image => {
                            let rot = o.get("rot").and_then(Json::as_f64).unwrap_or(*rot);
                            obj::write_box(o, nb, Some(rot));
                        }
                        ObjKind::Chart => obj::write_box(o, nb, None),
                    }
                }
            }
        });
    }

    /// Merges fields into an object (`null` removes a field).
    pub fn patch_object(&mut self, r: &ObjRef, patch: &Map<String, Json>) {
        self.mutate_objects(|d| {
            if let Some(o) = obj::get_mut(d, r) {
                obj::merge_patch(o, patch);
            }
        });
    }

    /// Removes objects (`removeSelection`).
    pub fn delete_objects(&mut self, refs: &[ObjRef]) {
        if refs.is_empty() {
            return;
        }
        self.mutate_objects(|d| {
            for k in ObjKind::ALL {
                let mine: Vec<&ObjRef> = refs.iter().filter(|r| r.kind == k).collect();
                if mine.is_empty() {
                    continue;
                }
                if let Some(a) = d.get_mut(k.key()).and_then(Json::as_array_mut) {
                    let mut i = 0;
                    a.retain(|o| {
                        let gone = match k {
                            ObjKind::Image => mine.iter().any(|r| r.index() == Some(i)),
                            _ => o.get("id").and_then(Json::as_str).is_some_and(|id| mine.iter().any(|r| r.id == id)),
                        };
                        i += 1;
                        !gone
                    });
                }
            }
        });
    }

    /// Duplicates objects 16 px off their source (`duplicateSelection`); returns the copies' refs.
    pub fn duplicate_objects(&mut self, refs: &[ObjRef], natural: &dyn Fn(&ObjRef) -> Option<(f64, f64)>, seed: u64) -> Vec<ObjRef> {
        let mut created = Vec::new();
        let mut adds: Vec<(ObjKind, Json)> = Vec::new();
        for (n, k) in [ObjKind::Chart, ObjKind::Shape, ObjKind::Equation, ObjKind::Image].into_iter().enumerate() {
            for (j, r) in refs.iter().filter(|r| r.kind == k).enumerate() {
                let (Some(o), Some((b, rot))) = (self.object(r).cloned(), self.object_box(r, natural(r))) else { continue };
                let mut copy = o;
                match k {
                    ObjKind::Image => obj::write_box(&mut copy, ObjBox { bx: b.bx + 16.0, by: b.by + 16.0, ..b }, Some(rot)),
                    ObjKind::Chart => obj::write_box(&mut copy, ObjBox { bx: b.bx + 16.0, by: b.by + 16.0, ..b }, None),
                    _ => {
                        if let Some(m) = copy.as_object_mut() {
                            let bx = m.get("bx").and_then(Json::as_f64).unwrap_or(b.bx) + 16.0;
                            let by = m.get("by").and_then(Json::as_f64).unwrap_or(b.by) + 16.0;
                            m.insert("bx".into(), json_number(bx));
                            m.insert("by".into(), json_number(by));
                        }
                    }
                }
                if k != ObjKind::Image {
                    let id = obj::new_id(seed.wrapping_add((n * 1000 + j) as u64));
                    if let Some(m) = copy.as_object_mut() {
                        m.insert("id".into(), json!(id));
                    }
                    created.push(ObjRef::new(k, id));
                }
                adds.push((k, copy));
            }
        }
        let base_images = self.objects(ObjKind::Image).len();
        let n_images = adds.iter().filter(|(k, _)| *k == ObjKind::Image).count();
        created.extend((0..n_images).map(|i| ObjRef::image(base_images + i)));
        if !adds.is_empty() {
            self.mutate_objects(|d| {
                for (k, o) in adds {
                    if let Some(a) = obj::array_mut(d, k) {
                        a.push(o);
                    }
                }
            });
        }
        created
    }

    /// Paint order inside each object's own array (`orderSelection`); returns the refs as they are after the
    /// move (pictures are addressed by index).
    pub fn order_objects(&mut self, refs: &[ObjRef], op: OrderOp) -> Vec<ObjRef> {
        let mut out: Vec<ObjRef> = refs.to_vec();
        self.mutate_objects(|d| {
            for k in ObjKind::ALL {
                let idx: Vec<usize> = refs.iter().filter(|r| r.kind == k).filter_map(|r| obj::find_index(d, r)).collect();
                if idx.is_empty() {
                    continue;
                }
                let Some(a) = d.get_mut(k.key()).and_then(Json::as_array_mut) else { continue };
                if idx.len() == 1 {
                    if let Some(to) = reorder_at(a, idx[0], op) {
                        if k == ObjKind::Image {
                            for r in out.iter_mut().filter(|r| r.kind == ObjKind::Image && r.index() == Some(idx[0])) {
                                *r = ObjRef::image(to);
                            }
                        }
                    }
                } else {
                    let tagged: Vec<(usize, Json)> = a.iter().cloned().enumerate().collect();
                    let moved = reorder_many(&tagged, &idx, op);
                    if k == ObjKind::Image {
                        for r in out.iter_mut().filter(|r| r.kind == ObjKind::Image) {
                            if let Some(old) = r.index() {
                                if let Some(new) = moved.iter().position(|(i, _)| *i == old) {
                                    *r = ObjRef::image(new);
                                }
                            }
                        }
                    }
                    *a = moved.into_iter().map(|(_, o)| o).collect();
                }
            }
        });
        out
    }

    /// Aligns or distributes objects (`alignSelection`).
    pub fn align_objects(&mut self, refs: &[ObjRef], mode: obj::AlignMode, natural: &dyn Fn(&ObjRef) -> Option<(f64, f64)>) {
        let snap: Vec<(ObjRef, ObjBox, f64)> = refs.iter().filter_map(|r| self.object_box(r, natural(r)).map(|(b, rot)| (r.clone(), b, rot))).collect();
        if snap.len() < 2 {
            return;
        }
        let boxes: Vec<ObjBox> = snap.iter().map(|(_, b, _)| *b).collect();
        let pos = obj::align_boxes(&boxes, mode);
        self.mutate_objects(|d| {
            for ((r, b, rot), (x, y)) in snap.iter().zip(pos) {
                if let Some(o) = obj::get_mut(d, r) {
                    match r.kind {
                        ObjKind::Shape | ObjKind::Equation => {
                            if let Some(m) = o.as_object_mut() {
                                m.insert("bx".into(), json_number(x));
                                m.insert("by".into(), json_number(y));
                            }
                        }
                        _ => obj::write_box(o, ObjBox { bx: x, by: y, ..*b }, Some(*rot)),
                    }
                }
            }
        });
    }

    /// Turns objects by a quarter turn, or resets them (`rotateChart`, the menus' « Rotation »).
    pub fn rotate_objects(&mut self, refs: &[ObjRef], deg: f64, reset: bool) {
        self.mutate_objects(|d| {
            for r in refs {
                if let Some(m) = obj::get_mut(d, r).and_then(Json::as_object_mut) {
                    let cur = m.get("rot").and_then(Json::as_f64).unwrap_or(0.0);
                    let v = if reset { 0.0 } else { crate::js::round((((cur + deg) % 360.0) + 360.0) % 360.0) };
                    m.insert("rot".into(), json_number(v));
                }
            }
        });
    }

    // ── Insertion ───────────────────────────────────────────────────────────

    fn used_bounds(&self) -> (u32, u32) {
        let (c, r) = self.sheet().used_extent();
        (c.min(MAX_COLS - 1), r.clamp(1, MAX_ROWS))
    }

    /// `chartAutoRange`: the selection grown to its contiguous data block when it is one cell, capped to the
    /// used extent.
    pub fn chart_auto_range(&self, cells: Option<Rect>) -> String {
        let Some(b) = cells else { return "A1:B5".into() };
        let (mut c1, mut r1, mut c2, mut r2) = (b.c1 as i64, b.r1 as i64, b.c2 as i64, b.r2 as i64);
        let (max_col, max_row) = self.used_bounds();
        if c1 == c2 && r1 == r2 {
            let filled = |c: i64, r: i64| {
                if c < 0 || r < 1 || c > max_col as i64 || r > max_row as i64 {
                    return false;
                }
                self.sheet().cells.get(&format!("{}{r}", index_to_col(c))).is_some_and(|cell| cell.has_f() || cell.has_value())
            };
            let any_row = |r: i64, c1: i64, c2: i64| (c1..=c2).any(|c| filled(c, r));
            let any_col = |c: i64, r1: i64, r2: i64| (r1..=r2).any(|r| filled(c, r));
            let mut grown = true;
            let mut guard = 0;
            while grown && guard < 4096 {
                guard += 1;
                grown = false;
                if r1 > 1 && any_row(r1 - 1, c1, c2) {
                    r1 -= 1;
                    grown = true;
                }
                if any_row(r2 + 1, c1, c2) {
                    r2 += 1;
                    grown = true;
                }
                if c1 > 0 && any_col(c1 - 1, r1, r2) {
                    c1 -= 1;
                    grown = true;
                }
                if any_col(c2 + 1, r1, r2) {
                    c2 += 1;
                    grown = true;
                }
            }
        }
        let r_end = r2.min((max_row as i64).max(r1));
        let c_end = c2.min((max_col as i64).max(c1));
        format!("{}{r1}:{}{r_end}", index_to_col(c1), index_to_col(c_end))
    }

    /// `insertChartObj`: a chart right of the selection with the default 380×240 box.
    pub fn insert_chart(&mut self, proto: &Json, cells: Option<Rect>, seed: u64) -> ObjRef {
        let g = self.geometry();
        let (col, row) = match cells {
            Some(b) => (b.c2 + 1, b.r1),
            None => (2, 1),
        };
        let bx = (g.col_left(col.min(MAX_COLS)) - ROW_HEADER_WIDTH) / self.zoom;
        let by = (g.row_top(row) - COL_HEADER_HEIGHT) / self.zoom;
        let id = obj::new_id(seed);
        let mut m = Map::new();
        m.insert("bw".into(), json_number(CHART_W));
        m.insert("bh".into(), json_number(CHART_H));
        if let Some(p) = proto.as_object() {
            for (k, v) in p {
                m.insert(k.clone(), v.clone());
            }
        }
        m.insert("id".into(), json!(id));
        m.insert("bx".into(), json_number(bx));
        m.insert("by".into(), json_number(by));
        self.mutate_objects(|d| {
            if let Some(a) = obj::array_mut(d, ObjKind::Chart) {
                a.push(Json::Object(m));
            }
        });
        ObjRef::new(ObjKind::Chart, id)
    }

    /// `insertShape`: a gallery shape centred on the selection, cascaded 16 px while its slot is taken.
    pub fn insert_shape(&mut self, kind: &str, cells: Option<Rect>, style: (&str, &str, f64), seed: u64) -> ObjRef {
        let g = self.geometry();
        let (c1, c2, r1, r2) = match cells {
            Some(b) => (b.c1, b.c2, b.r1, b.r2),
            None => (0, 0, 1, 1),
        };
        let (x1, x2) = (g.col_left(c1.min(MAX_COLS)), g.col_left((c2 + 1).min(MAX_COLS)));
        let (y1, y2) = (g.row_top(r1.clamp(1, MAX_ROWS)), g.row_top((r2 + 1).min(MAX_ROWS)));
        let (bw, bh) = (SHAPE_W, SHAPE_H);
        let mut bx = (((x1 + x2) / 2.0 - ROW_HEADER_WIDTH) / self.zoom - bw / 2.0).max(0.0);
        let mut by = (((y1 + y2) / 2.0 - COL_HEADER_HEIGHT) / self.zoom - bh / 2.0).max(0.0);
        let occupied = |x: f64, y: f64| {
            self.objects(ObjKind::Shape).iter().any(|s| {
                let (sx, sy) = (s.get("bx").and_then(Json::as_f64).unwrap_or(f64::NAN), s.get("by").and_then(Json::as_f64).unwrap_or(f64::NAN));
                (sx - x).abs() < 1.0 && (sy - y).abs() < 1.0
            })
        };
        let mut i = 0;
        while i < 64 && occupied(bx, by) {
            bx += 16.0;
            by += 16.0;
            i += 1;
        }
        self.add_shape(kind, ObjBox { bx, by, bw, bh }, style, seed)
    }

    /// `createShapeAt`: a shape for a drawn box (content px), or at its default size around the click when the
    /// box is tiny. `default_size` = the catalogue's size for the kind.
    pub fn create_shape_at(&mut self, kind: &str, b: Option<ObjRect>, default_size: Option<(f64, f64)>, style: (&str, &str, f64), seed: u64) -> ObjRef {
        let small = b.is_none_or(|b| b.w < 6.0 || b.h < 6.0);
        let (dw, dh) = default_size.unwrap_or((SHAPE_W, SHAPE_H));
        let z = self.zoom;
        let bw = if small { dw } else { b.map(|b| b.w / z).unwrap_or(dw) };
        let bh = if small { dh } else { b.map(|b| b.h / z).unwrap_or(dh) };
        let (x, y) = b.map(|b| (b.x, b.y)).unwrap_or((ROW_HEADER_WIDTH, COL_HEADER_HEIGHT));
        let bx = ((x - ROW_HEADER_WIDTH) / z - if small { bw / 2.0 } else { 0.0 }).max(0.0);
        let by = ((y - COL_HEADER_HEIGHT) / z - if small { bh / 2.0 } else { 0.0 }).max(0.0);
        self.add_shape(kind, ObjBox { bx, by, bw, bh }, style, seed)
    }

    /// `newShape` + `persistShapes`.
    fn add_shape(&mut self, kind: &str, b: ObjBox, style: (&str, &str, f64), seed: u64) -> ObjRef {
        let id = obj::new_id(seed);
        let s = json!({
            "id": id,
            "kind": kind,
            "bx": crate::js::round(b.bx),
            "by": crate::js::round(b.by),
            "bw": crate::js::round(b.bw).max(8.0),
            "bh": crate::js::round(b.bh).max(8.0),
            "fill": style.0,
            "border": style.1,
            "borderWidth": style.2,
        });
        let s = normalize_numbers(s);
        self.mutate_objects(|d| {
            if let Some(a) = obj::array_mut(d, ObjKind::Shape) {
                a.push(s);
            }
        });
        ObjRef::new(ObjKind::Shape, id)
    }

    /// `insertImageFromFile`: a picture at the active cell, scaled to at most 240 px wide.
    pub fn insert_image(&mut self, src: &str, natural_w: f64, natural_h: f64, at: (u32, u32)) -> ObjRef {
        let g = self.geometry();
        let (col, row) = at;
        let bx = (g.col_left(col.min(MAX_COLS)) - ROW_HEADER_WIDTH) / self.zoom;
        let by = (g.row_top(row.clamp(1, MAX_ROWS)) - COL_HEADER_HEIGHT) / self.zoom;
        let scale = if natural_w > 240.0 { 240.0 / natural_w } else { 1.0 };
        let mut m = Map::new();
        m.insert("fromCol".into(), json!(col));
        m.insert("fromRow".into(), json!(row.saturating_sub(1)));
        m.insert("bx".into(), json_number(bx));
        m.insert("by".into(), json_number(by));
        m.insert("bw".into(), json_number(natural_w * scale));
        m.insert("bh".into(), json_number(natural_h * scale));
        m.insert("src".into(), json!(src));
        let idx = self.objects(ObjKind::Image).len();
        self.mutate_objects(|d| {
            if let Some(a) = obj::array_mut(d, ObjKind::Image) {
                a.push(Json::Object(m));
            }
        });
        ObjRef::image(idx)
    }

    // ── Object clipboard (`sheet-object/clipboard.ts`, `copySelectionObjs`, `pasteObjFromClipboard`) ──

    /// Copies (or cuts) objects into a clipboard entry, in selection order, each with its box (and
    /// rotation) baked in (`copySelectionObjs`: `{ ...obj, ...box }`).
    pub fn copy_objects(&self, refs: &[ObjRef], cut: bool, natural: &dyn Fn(&ObjRef) -> Option<(f64, f64)>) -> ObjClipboard {
        let items = refs
            .iter()
            .filter_map(|r| {
                let (b, rot) = self.object_box(r, natural(r))?;
                let mut data = self.object(r)?.clone();
                obj::write_box(&mut data, b, Some(rot));
                Some(ClipObj { kind: r.kind, data, source: Some(r.clone()) })
            })
            .collect();
        ObjClipboard { items, cut }
    }

    /// Pastes a clipboard entry 16 px off the originals (fresh ids); a cut removes its sources. Returns the
    /// pasted refs.
    pub fn paste_objects(&mut self, clip: &ObjClipboard, seed: u64) -> Vec<ObjRef> {
        if clip.items.is_empty() {
            return Vec::new();
        }
        // Pictures are addressed by index: only the one that is still the cut picture goes.
        let sources: Vec<ObjRef> = if clip.cut {
            clip.items
                .iter()
                .filter_map(|i| i.source.clone())
                .filter(|s| s.kind != ObjKind::Image || self.object(s).and_then(|o| o.get("src")) == clip.items.iter().find(|i| i.source.as_ref() == Some(s)).and_then(|i| i.data.get("src")))
                .collect()
        } else {
            Vec::new()
        };
        if !sources.is_empty() {
            self.delete_objects(&sources);
        }
        let offset = if clip.cut { 0.0 } else { 16.0 };
        let mut created = Vec::new();
        let base_images = self.objects(ObjKind::Image).len();
        let mut n_images = 0;
        let items: Vec<(ObjKind, Json)> = clip
            .items
            .iter()
            .enumerate()
            .map(|(j, it)| {
                let mut o = it.data.clone();
                if let Some(m) = o.as_object_mut() {
                    for k in ["bx", "by"] {
                        if let Some(v) = m.get(k).and_then(Json::as_f64) {
                            m.insert(k.into(), json_number(v + offset));
                        }
                    }
                    if it.kind != ObjKind::Image {
                        let id = obj::new_id(seed.wrapping_add(j as u64));
                        m.insert("id".into(), json!(id));
                        created.push(ObjRef::new(it.kind, id));
                    } else {
                        created.push(ObjRef::image(base_images + n_images));
                        n_images += 1;
                    }
                }
                (it.kind, o)
            })
            .collect();
        self.mutate_objects(|d| {
            for (k, o) in items {
                if let Some(a) = obj::array_mut(d, k) {
                    a.push(o);
                }
            }
        });
        created
    }
}

/// Whole numbers as JSON integers (`Math.round` results stringify without `.0`).
fn normalize_numbers(v: Json) -> Json {
    match v {
        Json::Object(m) => Json::Object(m.into_iter().map(|(k, v)| (k, normalize_numbers(v))).collect()),
        Json::Number(n) => n.as_f64().map(json_number).unwrap_or(Json::Number(n)),
        o => o,
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::editor;
    use super::*;
    use crate::addr::Cell as Pos;

    #[test]
    fn charts_are_placed_like_the_web() {
        let mut e = editor(json!({"A1": {"v": ""}, "B1": {"v": "s"}, "A2": {"v": "a"}, "B2": {"v": 1}, "A3": {"v": "b"}, "B3": {"v": 2}}));
        e.select(Pos::new(0, 2));
        assert_eq!(e.chart_auto_range(Some(Rect::new(0, 2, 0, 2))), "A1:B3");
        let r = e.insert_chart(&json!({"type": "bar", "range": "A1:B3"}), Some(Rect::new(0, 1, 1, 3)), 7);
        let c = e.object(&r).cloned().unwrap_or_default();
        assert_eq!(c["bx"], json!(200));
        assert_eq!(c["by"], json!(0));
        assert_eq!(c["bw"], json!(380));
        assert!(e.is_dirty());
        let copies = e.duplicate_objects(std::slice::from_ref(&r), &|_| None, 9);
        assert_eq!(e.objects(ObjKind::Chart).len(), 2);
        assert_eq!(e.object(&copies[0]).and_then(|o| o["bx"].as_f64()), Some(216.0));
        e.delete_objects(&[r]);
        assert_eq!(e.objects(ObjKind::Chart).len(), 1);
    }

    #[test]
    fn shapes_cascade_and_order() {
        let mut e = editor(json!({}));
        let style = (SHAPE_FILL, SHAPE_BORDER, 1.0);
        let a = e.insert_shape("rect", Some(Rect::new(0, 1, 0, 1)), style, 1);
        let b = e.insert_shape("ellipse", Some(Rect::new(0, 1, 0, 1)), style, 2);
        let (ba, bb) = (e.object(&a).cloned().unwrap_or_default(), e.object(&b).cloned().unwrap_or_default());
        assert_eq!(bb["bx"].as_f64(), ba["bx"].as_f64().map(|x| x + 16.0));
        e.order_objects(std::slice::from_ref(&b), OrderOp::Back);
        assert_eq!(e.objects(ObjKind::Shape)[0]["kind"], json!("ellipse"));
        let clip = e.copy_objects(std::slice::from_ref(&a), true, &|_| None);
        let pasted = e.paste_objects(&clip, 5);
        assert_eq!(e.objects(ObjKind::Shape).len(), 2);
        assert_eq!(e.object(&pasted[0]).map(|o| o["bx"].clone()), Some(ba["bx"].clone()));
    }
}
