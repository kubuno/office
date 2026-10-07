//! The grid's pointer and keyboard handling of floating objects, and the object commands the window calls
//! (`web/src/SpreadsheetApp.tsx`: `chartHitTest`/`imageHitTest`/`shapeHitTest`, `startChartDrag`,
//! `startShapeDrag`, `startGroupMove`, `createShapeAt`, the key handler's object branch, `removeSelection`,
//! `copySelectionObjs`, `pasteObjFromClipboard`, `duplicateSelection`, `orderSelection`, `alignSelection`).

use kubuno_desktop::prelude::*;
use kubuno_desktop::views::component::Control as _;
use kubuno_office_sheets_core::addr::Rect as CellRect;
use kubuno_office_sheets_core::editor::objects::{SHAPE_BORDER, SHAPE_FILL};
use kubuno_office_sheets_core::grid::{COL_HEADER_HEIGHT, ROW_HEADER_WIDTH};
use kubuno_office_sheets_core::objects::order::OrderOp;
use kubuno_office_sheets_core::objects::{self as obj, AlignMode, Handle, ObjKind, ObjRect, ObjRef};
use serde_json::{Map, Value as Json};

use super::objects::{hit_adjust, ObjDrag};
use super::{GridCanvas, ObjectEventArgs};

fn kind_name(k: ObjKind) -> &'static str {
    match k {
        ObjKind::Chart => "chart",
        ObjKind::Image => "image",
        ObjKind::Shape => "shape",
        ObjKind::Equation => "equation",
    }
}

fn none(_: &ObjRef) -> Option<(f64, f64)> {
    None
}

impl GridCanvas {
    /// The selected objects (the last one is active).
    pub fn selected_objects(&self) -> &[ObjRef] {
        &self.objects.sel
    }

    /// The cell selection as the web's `bounds()`: none while objects are selected.
    pub fn cell_bounds(&self) -> Option<CellRect> {
        let ed = self.ed.as_ref()?;
        ed.selected.then(|| ed.sel.rect())
    }

    /// Replaces the object selection (an empty one gives the cells back their selection).
    pub fn select_objects(&mut self, sel: Vec<ObjRef>) {
        let any = !sel.is_empty();
        self.objects.sel = sel;
        if any {
            self.select_equation(None);
            self.commit_edit_quietly();
            if let Some(ed) = self.ed.as_mut() {
                ed.selected = false;
            }
        }
        self.invalidate();
    }

    /// Drops the object selection.
    pub fn clear_objects(&mut self) {
        if !self.objects.sel.is_empty() {
            self.objects.sel.clear();
            self.invalidate();
        }
    }

    /// Arms a shape kind: the next drag on the grid draws it (`armShape`).
    pub fn arm_shape(&mut self, kind: &str) {
        self.objects.armed = Some(kind.to_string());
        self.objects.sel.clear();
        self.invalidate();
    }

    fn content(&self, x: f64, y: f64) -> (f64, f64) {
        (x + self.view.scroll_x, y + self.view.scroll_y)
    }

    fn in_data(&self, x: f64, y: f64) -> bool {
        x >= ROW_HEADER_WIDTH && y >= COL_HEADER_HEIGHT && x < self.view.width && y < self.view.height
    }

    /// A press on the grid: objects take it first (after the headers and scroll bars). Returns whether it was
    /// taken.
    pub(super) fn object_mouse_down(&mut self, x: f64, y: f64, button: MouseButton, mods: kubuno_desktop::controls::host::Modifiers) -> bool {
        if !self.in_data(x, y) || self.edit.as_ref().is_some_and(|e| e.le.expects_ref()) {
            return false;
        }
        let (cx, cy) = self.content(x, y);
        if button == MouseButton::Left {
            if self.objects.armed.is_some() {
                self.commit_edit_quietly();
                self.objects.drag = Some(ObjDrag::Draw { start: (cx, cy), cur: (cx, cy), square: mods.shift, centre: mods.alt });
                self.invalidate();
                return true;
            }
        }
        let Some(ed) = self.ed.as_ref() else { return false };
        if button == MouseButton::Left {
            if let Some((r, index, rect)) = hit_adjust(ed, &self.objects.sel, cx, cy) {
                let adj: Vec<f64> = ed.object(&r).and_then(|o| o.get("adj")).and_then(Json::as_array).map(|a| a.iter().filter_map(Json::as_f64).collect()).unwrap_or_default();
                self.objects.drag = Some(ObjDrag::Adjust { r, index, rect, adj, moved: false });
                return true;
            }
        }
        // Equations keep their own layer (`equations.rs`) until it moves onto this one.
        let Some((r, handle)) = ed.hit_object(cx, cy, &self.objects.sel, &none).filter(|(r, _)| r.kind != ObjKind::Equation) else {
            if !self.objects.sel.is_empty() {
                self.objects.sel.clear();
                self.invalidate();
            }
            return false;
        };
        if button == MouseButton::Right {
            if !self.objects.sel.contains(&r) {
                self.select_objects(vec![r.clone()]);
            }
            self.raise_object_menu(ObjectEventArgs { x: x as f32, y: y as f32, kind: kind_name(r.kind).into(), id: r.id.clone() });
            return true;
        }
        if button != MouseButton::Left {
            return true;
        }
        let additive = mods.shift || mods.ctrl;
        let sel = obj::click_selection(&self.objects.sel, &r, additive);
        self.select_objects(sel);
        if additive {
            return true;
        }
        let Some(ed) = self.ed.as_ref() else { return true };
        if handle == Handle::Move && self.objects.sel.len() > 1 {
            let snap = self.objects.sel.iter().filter_map(|s| ed.object_box(s, None).map(|(b, rot)| (s.clone(), b, rot))).collect();
            self.objects.drag = Some(ObjDrag::Group { snap, start: (cx, cy), dx: 0.0, dy: 0.0, moved: false });
            return true;
        }
        let Some((b, rot)) = ed.object_box(&r, None) else { return true };
        self.objects.drag = Some(ObjDrag::One { r, handle, box0: b, rot0: rot, start: (cx, cy), cur: b, rot, moved: false });
        true
    }

    /// The pointer moves during an object gesture.
    pub(super) fn object_mouse_move(&mut self, x: f64, y: f64, mods: kubuno_desktop::controls::host::Modifiers) -> bool {
        let (cx, cy) = self.content(x, y);
        let zoom = self.ed.as_ref().map(|e| e.zoom).unwrap_or(1.0);
        let Some(drag) = self.objects.drag.as_mut() else { return false };
        match drag {
            ObjDrag::Draw { cur, square, centre, .. } => {
                *cur = (cx, cy);
                *square = mods.shift;
                *centre = mods.alt;
            }
            ObjDrag::One { handle, box0, rot0, start, cur, rot, moved, .. } => {
                *moved = true;
                if *handle == Handle::Rotate {
                    let (ccx, ccy) = (ROW_HEADER_WIDTH + (box0.bx + box0.bw / 2.0) * zoom, COL_HEADER_HEIGHT + (box0.by + box0.bh / 2.0) * zoom);
                    *rot = obj::rotation_for(ccx, ccy, cx, cy, mods.shift);
                } else {
                    *cur = obj::apply_drag(*box0, *handle, cx - start.0, cy - start.1, *rot0, zoom);
                }
            }
            ObjDrag::Group { start, dx, dy, moved, .. } => {
                *moved = true;
                *dx = (cx - start.0) / zoom;
                *dy = (cy - start.1) / zoom;
            }
            ObjDrag::Adjust { r, index, rect, adj, moved } => {
                *moved = true;
                let (lx, ly) = obj::to_local(rect, cx, cy);
                let kind = self.ed.as_ref().and_then(|e| e.object(r)).and_then(|o| o.get("kind")).and_then(Json::as_str).unwrap_or("").to_string();
                let srect = kubuno_office_shapes_core::Rect { x: rect.x, y: rect.y, w: rect.w, h: rect.h };
                *adj = kubuno_office_shapes_core::adjust::adjust_from_drag(&kind, srect, *index, lx, ly, Some(adj));
            }
        }
        self.invalidate();
        true
    }

    /// The gesture ends: written to the workbook once.
    pub(super) fn object_mouse_up(&mut self) -> bool {
        let Some(drag) = self.objects.drag.take() else { return false };
        let seed = self.objects.next_seed();
        let armed = self.objects.armed.clone();
        let cells = self.cell_bounds();
        let Some(ed) = self.ed.as_mut() else { return true };
        match drag {
            ObjDrag::Draw { start, cur, square, centre } => {
                let kind = armed.unwrap_or_else(|| "rect".into());
                let b = kubuno_office_shapes_core::draw::draw_box_from(start.0, start.1, cur.0, cur.1, square, centre);
                let rect = ObjRect { x: b.x, y: b.y, w: b.w, h: b.h, rot: 0.0 };
                let default = kubuno_office_shapes_core::catalog::shape_default_size(&kind);
                let r = ed.create_shape_at(&kind, Some(rect), Some(default), (SHAPE_FILL, SHAPE_BORDER, 1.0), seed);
                let _ = cells;
                self.objects.armed = None;
                self.select_objects(vec![r]);
            }
            ObjDrag::One { r, handle, cur, rot, moved, .. } => {
                if moved {
                    if handle == Handle::Rotate {
                        let b = ed.object_box(&r, None).map(|(b, _)| b).unwrap_or(cur);
                        ed.set_object_box(&r, b, Some(rot));
                    } else {
                        ed.set_object_box(&r, cur, None);
                    }
                }
            }
            ObjDrag::Group { snap, dx, dy, moved, .. } => {
                if moved {
                    ed.move_objects(&snap, dx, dy);
                }
            }
            ObjDrag::Adjust { r, adj, moved, .. } => {
                if moved {
                    let mut m = Map::new();
                    m.insert("adj".into(), Json::Array(adj.into_iter().map(kubuno_office_sheets_core::model::cell::json_number).collect()));
                    ed.patch_object(&r, &m);
                }
            }
        }
        self.invalidate();
        true
    }

    /// A double click on an object asks the window for its editor.
    pub(super) fn object_double_click(&mut self, x: f64, y: f64) -> bool {
        if !self.in_data(x, y) {
            return false;
        }
        let (cx, cy) = self.content(x, y);
        let Some(ed) = self.ed.as_ref() else { return false };
        let Some((r, _)) = ed.hit_object(cx, cy, &self.objects.sel, &none).filter(|(r, _)| r.kind != ObjKind::Equation) else { return false };
        self.select_objects(vec![r.clone()]);
        self.raise_object_open(ObjectEventArgs { x: x as f32, y: y as f32, kind: kind_name(r.kind).into(), id: r.id.clone() });
        true
    }

    /// The object keys: Delete/Backspace removes the selection, Escape drops it (or the armed tool).
    pub(super) fn object_key(&mut self, key: u16, _mods: kubuno_desktop::controls::host::Modifiers) -> bool {
        use kubuno_desktop::controls::host::vk;
        if self.edit.is_some() {
            return false;
        }
        if key == vk::ESCAPE && self.objects.armed.is_some() {
            self.objects.armed = None;
            self.objects.drag = None;
            self.invalidate();
            return true;
        }
        if self.objects.sel.is_empty() {
            return false;
        }
        match key {
            k if k == vk::DELETE || k == vk::BACK => {
                self.delete_selected_objects();
                true
            }
            k if k == vk::ESCAPE => {
                self.clear_objects();
                true
            }
            _ => false,
        }
    }

    /// The cursor over objects.
    pub(super) fn object_cursor(&self, x: f64, y: f64) -> Option<kubuno_desktop::controls::host::Cursor> {
        use kubuno_desktop::controls::host::Cursor;
        if self.objects.armed.is_some() && self.in_data(x, y) {
            return Some(Cursor::Crosshair);
        }
        if !self.in_data(x, y) {
            return None;
        }
        let ed = self.ed.as_ref()?;
        let (cx, cy) = self.content(x, y);
        if hit_adjust(ed, &self.objects.sel, cx, cy).is_some() {
            return Some(Cursor::Hand);
        }
        let (r, h) = ed.hit_object(cx, cy, &self.objects.sel, &none).filter(|(r, _)| r.kind != ObjKind::Equation)?;
        let rot = ed.object_rect(&r, None).map(|r| r.rot).unwrap_or(0.0);
        Some(match obj::cursor_for(h, rot) {
            obj::Cursor::Move => Cursor::Move,
            obj::Cursor::Grab => Cursor::Hand,
            obj::Cursor::NS => Cursor::ResizeNS,
            obj::Cursor::EW => Cursor::ResizeEW,
            obj::Cursor::NESW => Cursor::ResizeNESW,
            obj::Cursor::NWSE => Cursor::ResizeNWSE,
        })
    }

    // ── Commands ────────────────────────────────────────────────────────────

    /// `removeSelection`.
    pub fn delete_selected_objects(&mut self) {
        let sel = std::mem::take(&mut self.objects.sel);
        if let Some(ed) = self.ed.as_mut() {
            ed.delete_objects(&sel);
        }
        self.invalidate();
    }

    /// `copySelectionObjs(cut)`.
    pub fn copy_selected_objects(&mut self, cut: bool) -> bool {
        let Some(ed) = self.ed.as_ref() else { return false };
        if self.objects.sel.is_empty() {
            return false;
        }
        self.objects.clip = ed.copy_objects(&self.objects.sel, cut, &none);
        true
    }

    /// Whether the object clipboard holds something.
    pub fn can_paste_objects(&self) -> bool {
        !self.objects.clip.items.is_empty()
    }

    /// `pasteObjFromClipboard`: the paste becomes the selection; a cut is consumed.
    pub fn paste_objects(&mut self) -> bool {
        if self.objects.clip.items.is_empty() {
            return false;
        }
        let seed = self.objects.next_seed();
        let clip = self.objects.clip.clone();
        let Some(ed) = self.ed.as_mut() else { return false };
        let created = ed.paste_objects(&clip, seed);
        if clip.cut {
            self.objects.clip = Default::default();
        }
        self.select_objects(created);
        true
    }

    /// `duplicateSelection`.
    pub fn duplicate_selected_objects(&mut self) {
        let seed = self.objects.next_seed();
        let sel = self.objects.sel.clone();
        let Some(ed) = self.ed.as_mut() else { return };
        let created = ed.duplicate_objects(&sel, &none, seed);
        if !created.is_empty() {
            self.select_objects(created);
        }
    }

    /// `orderSelection`.
    pub fn order_selected_objects(&mut self, op: OrderOp) {
        let sel = self.objects.sel.clone();
        let Some(ed) = self.ed.as_mut() else { return };
        self.objects.sel = ed.order_objects(&sel, op);
        self.invalidate();
    }

    /// `alignSelection`.
    pub fn align_selected_objects(&mut self, mode: AlignMode) {
        let sel = self.objects.sel.clone();
        if let Some(ed) = self.ed.as_mut() {
            ed.align_objects(&sel, mode, &none);
        }
        self.invalidate();
    }

    /// The quarter turns and reset of the menus.
    pub fn rotate_selected_objects(&mut self, deg: f64, reset: bool) {
        let sel = self.objects.sel.clone();
        if let Some(ed) = self.ed.as_mut() {
            ed.rotate_objects(&sel, deg, reset);
        }
        self.invalidate();
    }

    /// Merges fields into the active object (`applyObjPatch`).
    pub fn patch_active_object(&mut self, patch: &Map<String, Json>) {
        let Some(r) = self.objects.sel.last().cloned() else { return };
        if let Some(ed) = self.ed.as_mut() {
            ed.patch_object(&r, patch);
        }
        self.invalidate();
    }

    /// Merges fields into every selected object (the mini toolbar's fill / outline on a group).
    pub fn patch_selected_objects(&mut self, patch: &Map<String, Json>) {
        let sel = self.objects.sel.clone();
        if let Some(ed) = self.ed.as_mut() {
            for r in &sel {
                ed.patch_object(r, patch);
            }
        }
        self.invalidate();
    }

    /// The stored JSON of the active object.
    pub fn active_object(&self) -> Option<(ObjRef, Json)> {
        let r = self.objects.sel.last()?.clone();
        let o = self.ed.as_ref()?.object(&r)?.clone();
        Some((r, o))
    }

    /// `insertChartObj`: inserts and selects a chart.
    pub fn insert_chart(&mut self, proto: &Json) {
        let seed = self.objects.next_seed();
        let cells = self.cell_bounds();
        let Some(ed) = self.ed.as_mut() else { return };
        let r = ed.insert_chart(proto, cells, seed);
        self.select_objects(vec![r]);
    }

    /// Replaces a chart's stored JSON (the wizard's « Terminer » on an existing chart), keeping its id.
    pub fn replace_chart(&mut self, id: &str, chart: &Json) {
        let r = ObjRef::new(ObjKind::Chart, id);
        let Some(ed) = self.ed.as_mut() else { return };
        ed.mutate_objects(|d| {
            if let Some(o) = obj::get_mut(d, &r) {
                let mut c = chart.clone();
                if let Some(m) = c.as_object_mut() {
                    m.insert("id".into(), Json::String(id.into()));
                }
                *o = c;
            }
        });
        self.invalidate();
    }

    /// `chartAutoRange` for the current cell selection.
    pub fn chart_auto_range(&self) -> String {
        self.ed.as_ref().map(|ed| ed.chart_auto_range(self.cell_bounds())).unwrap_or_else(|| "A1:B5".into())
    }

    /// `insertImageFromFile`: a picture at the active cell.
    pub fn insert_image(&mut self, src: &str, natural: (u32, u32)) {
        let Some(ed) = self.ed.as_mut() else { return };
        let at = if ed.selected { (ed.sel.active.col, ed.sel.active.row) } else { (0, 1) };
        let r = ed.insert_image(src, natural.0 as f64, natural.1 as f64, at);
        self.select_objects(vec![r]);
    }
}
