//! `BoardCanvas` — the infinite board being edited: the web editor's `<canvas>` (`WhiteboardApp.tsx`),
//! painted with the shared renderer (`kubuno_office_whiteboard_core::render`) over the Direct2D surface of
//! `crate::platform::surface`, and driven by the shared engine (`kubuno_office_whiteboard_core::engine`).
//!
//! Increment A: open and render read-only, pan and zoom. Increment B: the tools — select, move, resize via
//! handles, draw a shape / sticky / text / frame / arrow, freehand pen, eraser, z-order, group/ungroup,
//! delete, and undo/redo through the core's `yrs` undo manager (local changes only). Every edit mutates the
//! Yjs document and yields an update the window relays to the collaboration room (increment C).

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::{Canvas, Rect as UiRect, Size};
use kubuno_desktop::views::component::{Component, Control, ControlCore, EventCx, Keys, PaintEventCx};
use kubuno_desktop::views::events::Event;
use kubuno_office_desktop_shared::ids::fresh_uuid_v4;
use kubuno_office_whiteboard_common::doc::OpenBoard;
use kubuno_office_whiteboard_core::engine::{self, ResizeHandle};
use kubuno_office_whiteboard_core::model::Rect;
use kubuno_office_whiteboard_core::render::{self, Surface as _};
use kubuno_office_whiteboard_core::undo::Undo;
use serde_json::{json, Value as Json};
use windows_numerics::Matrix3x2;

use crate::platform::surface::{D2DSurface, SurfaceCache};

/// What the ribbon, the backstage and the status bar show of the board.
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct BoardStateEventArgs {
    pub elements: usize,
    pub strokes: usize,
    pub zoom: f32,
    pub background: String,
    /// The active tool.
    pub tool: String,
    pub selected: usize,
    pub has_group: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    /// An update to publish to the collaboration room (empty = nothing changed).
    pub update: Vec<u8>,
}

/// A gesture in progress.
enum Drag {
    None,
    /// Panning the canvas (hand tool, or an empty-space drag).
    Pan { lx: f32, ly: f32 },
    /// Moving the selection: the pointer where the drag began and the elements' original JSON.
    Move { start: (f64, f64), snap: Vec<(String, Json)> },
    /// Resizing one element with a handle.
    Resize { id: String, handle: ResizeHandle, snap: Json },
    /// Drawing an area element (shape / sticky / text / frame): the box so far.
    DrawBox { tool: String, start: (f64, f64), cur: (f64, f64) },
    /// Drawing an arrow: the two ends so far.
    DrawArrow { start: (f64, f64), cur: (f64, f64) },
    /// Freehand pen: the flat point list so far.
    Pen { points: Vec<f64> },
}

/// The board canvas (see the module doc).
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Whiteboard")]
#[toolbox(icon = "pen-tool")]
#[default_event("StateChanged")]
pub struct BoardCanvas {
    base: ControlCore,
    #[event]
    #[category("Action")]
    pub state_changed: Event<BoardStateEventArgs>,
    board: OpenBoard,
    undo: Option<Undo>,
    cache: SurfaceCache,
    bounds: UiRect,
    reported: Option<BoardStateEventArgs>,
    /// The active tool (`select | hand | sticky | text | shape | arrow | pen | eraser | frame`).
    tool: String,
    /// The selected element ids.
    selection: Vec<String>,
    drag: Drag,
    /// Inline text editing in progress: the element id and the caret (a char index into its text).
    editing: Option<(String, usize)>,
    /// A pending update to hand to the window on the next `StateChanged` (relayed to the room).
    pending_update: Vec<u8>,
}

impl Default for BoardCanvas {
    fn default() -> Self {
        let board = OpenBoard::sample();
        let undo = Some(Undo::new(&board.doc));
        Self {
            base: ControlCore::default(),
            state_changed: Event::default(),
            board,
            undo,
            cache: SurfaceCache::default(),
            bounds: UiRect::new(0.0, 0.0, 800.0, 600.0),
            reported: None,
            tool: "select".into(),
            selection: Vec::new(),
            drag: Drag::None,
            editing: None,
            pending_update: Vec::new(),
        }
    }
}

impl BoardCanvas {
    /// Shows `board`.
    pub fn open(&mut self, board: OpenBoard) {
        self.undo = Some(Undo::new(&board.doc));
        self.board = board;
        self.selection.clear();
        self.drag = Drag::None;
        self.changed();
    }

    pub fn board(&self) -> &OpenBoard {
        &self.board
    }
    pub fn board_mut(&mut self) -> &mut OpenBoard {
        &mut self.board
    }

    /// Repaints after an external change (a room update applied to the document).
    pub fn refresh(&mut self) {
        self.changed();
    }

    /// Sets the active tool (from the ribbon).
    pub fn set_tool(&mut self, tool: &str) {
        self.tool = tool.to_string();
        self.changed();
    }
    pub fn tool(&self) -> &str {
        &self.tool
    }

    pub fn set_background(&mut self, bg: &str) {
        self.board.background = bg.to_string();
        self.changed();
    }

    pub fn zoom_in(&mut self) {
        let (w, h) = self.size();
        self.board.viewport.zoom_at(1.1, w / 2.0, h / 2.0);
        self.changed();
    }
    pub fn zoom_out(&mut self) {
        let (w, h) = self.size();
        self.board.viewport.zoom_at(1.0 / 1.1, w / 2.0, h / 2.0);
        self.changed();
    }
    pub fn zoom_reset(&mut self) {
        self.board.viewport.set_zoom(1.0);
        self.changed();
    }
    pub fn zoom_fit(&mut self) {
        let (w, h) = self.size();
        let scene = self.board.scene();
        self.board.viewport.fit_to_elements(&scene.elements, w, h);
        self.changed();
    }
    pub fn set_zoom_percent(&mut self, percent: f32) {
        self.board.viewport.set_zoom(percent as f64 / 100.0);
        self.changed();
    }

    // ── Editing commands (ribbon) ───────────────────────────────────────────

    pub fn undo(&mut self) {
        if let Some(u) = self.undo.as_mut() {
            u.undo();
        }
        self.after_edit(Vec::new());
    }
    pub fn redo(&mut self) {
        if let Some(u) = self.undo.as_mut() {
            u.redo();
        }
        self.after_edit(Vec::new());
    }

    pub fn delete_selection(&mut self) {
        let ids = self.selected_group();
        let mut update = Vec::new();
        for id in &ids {
            if let Some(u) = self.board.doc.remove_element(id) {
                update.extend_from_slice(&u);
            }
        }
        self.selection.clear();
        self.stop_capturing();
        self.after_edit(update);
    }

    pub fn bring_to_front(&mut self) {
        self.reorder(true);
    }
    pub fn send_to_back(&mut self) {
        self.reorder(false);
    }

    fn reorder(&mut self, front: bool) {
        let scene = self.board.scene();
        let (min_z, max_z) = scene.elements.iter().fold((0.0_f64, -1.0_f64), |(mn, mx), e| (mn.min(e.z_index()), mx.max(e.z_index())));
        let mut update = Vec::new();
        for id in self.selected_group() {
            if let Some(mut obj) = self.element_json(&id) {
                let z = if front { max_z + 1.0 } else { min_z - 1.0 };
                obj["zIndex"] = json!(z);
                if let Some(u) = self.board.doc.set_element(&id, &obj) {
                    update.extend_from_slice(&u);
                }
            }
        }
        self.stop_capturing();
        self.after_edit(update);
    }

    pub fn group_selection(&mut self) {
        if self.selection.len() < 2 {
            return;
        }
        let gid = fresh_uuid_v4();
        let ids = self.selection.clone();
        let mut update = Vec::new();
        for id in &ids {
            if let Some(mut obj) = self.element_json(id) {
                obj["groupId"] = json!(gid);
                if let Some(u) = self.board.doc.set_element(id, &obj) {
                    update.extend_from_slice(&u);
                }
            }
        }
        self.stop_capturing();
        self.after_edit(update);
    }

    pub fn ungroup_selection(&mut self) {
        let ids = self.selected_group();
        let mut update = Vec::new();
        for id in &ids {
            if let Some(mut obj) = self.element_json(id) {
                if obj.get("groupId").is_some() {
                    obj.as_object_mut().map(|o| o.remove("groupId"));
                    if let Some(u) = self.board.doc.set_element(id, &obj) {
                        update.extend_from_slice(&u);
                    }
                }
            }
        }
        self.stop_capturing();
        self.after_edit(update);
    }

    // ── Internals ────────────────────────────────────────────────────────────

    // ── Panels (Calques / Propriétés) ───────────────────────────────────────

    /// The elements for the Calques panel, front first (top of the list = front).
    pub fn element_rows(&self) -> Vec<crate::controls::layer_list::ElementRow> {
        use crate::controls::layer_list::ElementRow;
        use kubuno_office_whiteboard_core::WbElement as E;
        let scene = self.board.scene();
        let mut rows: Vec<ElementRow> = scene
            .z_ordered()
            .into_iter()
            .map(|e| {
                let (kind, name) = match e {
                    E::Sticky(s) => ("sticky", first_line(&s.text, "Post-it")),
                    E::Text(t) => ("text", first_line(&t.text, "Texte")),
                    E::Shape(s) => ("shape", s.kind.clone()),
                    E::Arrow(_) => ("arrow", "Flèche".to_string()),
                    E::Frame(f) => ("frame", if f.title.is_empty() { "Cadre".into() } else { f.title.clone() }),
                    E::Image(_) => ("image", "Image".to_string()),
                };
                ElementRow { id: e.id().to_string(), kind: kind.to_string(), name }
            })
            .collect();
        rows.reverse(); // front first
        rows
    }

    /// The single selected element's id, if exactly one is selected.
    pub fn primary_selection(&self) -> Option<String> {
        if self.selection.len() == 1 {
            Some(self.selection[0].clone())
        } else {
            None
        }
    }

    /// Selects exactly one element by id (from the Calques panel).
    pub fn select_only(&mut self, id: &str) {
        self.selection = vec![id.to_string()];
        self.changed();
    }

    /// The selected element's stored JSON (for the Propriétés panel), when one is selected.
    pub fn selected_element_json(&self) -> Option<Json> {
        let id = self.primary_selection()?;
        self.element_json(&id)
    }

    fn selected_group(&self) -> Vec<String> {
        let scene = self.board.scene();
        let mut out: Vec<String> = Vec::new();
        for id in &self.selection {
            for m in engine::group_members(&scene.elements, id) {
                if !out.iter().any(|x| x == m) {
                    out.push(m.to_string());
                }
            }
        }
        out
    }

    fn element_json(&self, id: &str) -> Option<Json> {
        self.board.doc.elements().get(id).cloned()
    }

    fn stop_capturing(&mut self) {
        if let Some(u) = self.undo.as_mut() {
            u.stop_capturing();
        }
    }

    fn after_edit(&mut self, update: Vec<u8>) {
        if !update.is_empty() {
            self.pending_update.extend_from_slice(&update);
        }
        self.changed();
    }

    // ── Inline text editing (double-click a sticky / text box) ───────────────

    fn start_edit(&mut self, id: &str) {
        let text = self.element_json(id).and_then(|o| o.get("text").and_then(Json::as_str).map(str::to_string)).unwrap_or_default();
        self.selection = vec![id.to_string()];
        self.editing = Some((id.to_string(), text.chars().count()));
        self.changed();
    }

    fn commit_edit(&mut self) {
        if self.editing.take().is_some() {
            self.stop_capturing();
            self.changed();
        }
    }

    fn edit_text(&self) -> Option<String> {
        let (id, _) = self.editing.as_ref()?;
        self.element_json(id).and_then(|o| o.get("text").and_then(Json::as_str).map(str::to_string))
    }

    fn write_edit_text(&mut self, text: &str) {
        if let Some((id, _)) = self.editing.clone() {
            if let Some(mut obj) = self.element_json(&id) {
                obj["text"] = json!(text);
                let u = self.board.doc.set_element(&id, &obj);
                self.after_edit(u.unwrap_or_default());
            }
        }
    }

    /// Inserts one character at the caret.
    fn edit_char(&mut self, c: char) {
        let Some((_, caret)) = self.editing.clone() else { return };
        let text = self.edit_text().unwrap_or_default();
        let mut chars: Vec<char> = text.chars().collect();
        let caret = caret.min(chars.len());
        chars.insert(caret, c);
        let new: String = chars.iter().collect();
        self.write_edit_text(&new);
        if let Some((_, cr)) = self.editing.as_mut() {
            *cr = caret + 1;
        }
    }

    /// Handles an editing key; `true` when it was used.
    fn edit_key(&mut self, key: u16) -> bool {
        use kubuno_desktop::controls::host::vk;
        let Some((_, caret)) = self.editing.clone() else { return false };
        let text = self.edit_text().unwrap_or_default();
        let mut chars: Vec<char> = text.chars().collect();
        let caret = caret.min(chars.len());
        match key {
            k if k == vk::ESCAPE => {
                self.commit_edit();
                return true;
            }
            k if k == vk::ENTER => {
                chars.insert(caret, '\n');
                let new: String = chars.iter().collect();
                self.write_edit_text(&new);
                if let Some((_, cr)) = self.editing.as_mut() {
                    *cr = caret + 1;
                }
            }
            k if k == vk::LEFT => self.set_caret(caret.saturating_sub(1)),
            k if k == vk::RIGHT => self.set_caret((caret + 1).min(chars.len())),
            k if k == vk::HOME => self.set_caret(0),
            k if k == vk::END => self.set_caret(chars.len()),
            k if k == vk::BACK => {
                if caret > 0 {
                    chars.remove(caret - 1);
                    let new: String = chars.iter().collect();
                    self.write_edit_text(&new);
                    self.set_caret(caret - 1);
                }
            }
            k if k == vk::DELETE => {
                if caret < chars.len() {
                    chars.remove(caret);
                    let new: String = chars.iter().collect();
                    self.write_edit_text(&new);
                }
            }
            _ => return false,
        }
        self.changed();
        true
    }

    fn set_caret(&mut self, caret: usize) {
        if let Some((_, cr)) = self.editing.as_mut() {
            *cr = caret;
        }
    }

    /// Whether an element kind can be edited in place.
    fn editable_kind(kind: &str) -> bool {
        kind == "sticky" || kind == "text"
    }

    /// Draws the inline-edit caret over the element being edited (content space, after the viewport
    /// transform is applied).
    fn paint_caret(&self, surface: &mut D2DSurface, scene: &kubuno_office_whiteboard_core::Scene) {
        use kubuno_office_whiteboard_core::WbElement as E;
        let Some((id, caret)) = &self.editing else { return };
        let Some(el) = scene.elements.iter().find(|e| e.id() == id) else { return };
        let zoom = self.board.viewport.scale();
        let (ox, oy, fs, bold) = match el {
            E::Sticky(s) => (s.base.x + 10.0, s.base.y + 10.0, s.font_size.max(7.0 / zoom), false),
            E::Text(t) => (t.base.x, t.base.y, t.font_size.max(6.0 / zoom), t.font_weight == "bold"),
            _ => return,
        };
        let text = match el {
            E::Sticky(s) => s.text.clone(),
            E::Text(t) => t.text.clone(),
            _ => String::new(),
        };
        let before: String = text.chars().take(*caret).collect();
        let line_index = before.matches('\n').count() as f64;
        let cur_line = before.rsplit('\n').next().unwrap_or("");
        surface.set_font(fs, bold, "Arial, sans-serif");
        let cx = ox + surface.measure_text(cur_line);
        let lh = fs * 1.4;
        let cy = oy + line_index * lh;
        surface.save();
        surface.set_stroke("#1a73e8");
        surface.set_line_width((1.5 / zoom).max(0.5));
        surface.begin_path();
        surface.move_to(cx, cy);
        surface.line_to(cx, cy + fs);
        surface.stroke();
        surface.restore();
    }

    fn changed(&mut self) {
        self.invalidate();
        kubuno_desktop::controls::host::request_repaint_after(0);
    }

    fn size(&self) -> (f64, f64) {
        (((self.bounds.right - self.bounds.left).max(1.0)) as f64, ((self.bounds.bottom - self.bounds.top).max(1.0)) as f64)
    }

    /// A pointer position (window DIPs) as a content-space point.
    fn world(&self, x: f32, y: f32) -> (f64, f64) {
        let p = self.board.viewport.screen_to_canvas((x - self.bounds.left) as f64, (y - self.bounds.top) as f64);
        (p.x, p.y)
    }

    fn can_undo(&self) -> bool {
        self.undo.as_ref().is_some_and(|u| u.can_undo())
    }
    fn can_redo(&self) -> bool {
        self.undo.as_ref().is_some_and(|u| u.can_redo())
    }

    fn state_args(&mut self) -> BoardStateEventArgs {
        let scene = self.board.scene();
        let has_group = scene.elements.iter().any(|e| self.selection.iter().any(|s| s == e.id()) && e.group_id().is_some());
        let update = std::mem::take(&mut self.pending_update);
        BoardStateEventArgs {
            elements: scene.elements.len(),
            strokes: scene.strokes.len(),
            zoom: self.board.viewport.zoom_percent() as f32,
            background: self.board.background.clone(),
            tool: self.tool.clone(),
            selected: self.selection.len(),
            has_group,
            can_undo: self.can_undo(),
            can_redo: self.can_redo(),
            update,
        }
    }

    // ── Painting ──────────────────────────────────────────────────────────────

    fn paint_scene(&mut self, c: &dyn kubuno_desktop::controls::ControlCanvas) {
        let Some(renderer) = c.renderer() else { return };
        let base = Matrix3x2 { M11: 1.0, M12: 0.0, M21: 0.0, M22: 1.0, M31: self.bounds.left, M32: self.bounds.top };
        let Ok(mut surface) = D2DSurface::new(&renderer.d2d_context, &renderer.d2d_factory, &renderer.dwrite, &self.cache, base) else { return };
        let (w, h) = self.size();
        let scene = self.board.scene();
        let vp = self.board.viewport;
        c.push_clip(&self.bounds);
        // The board is a LIGHT canvas, as on the web — even when the shell is in dark mode.
        surface.set_fill("#ffffff");
        surface.fill_rect(0.0, 0.0, w, h);
        render::render_background(&mut surface, &scene.background, &vp, w, h);
        surface.set_transform(vp.scale(), 0.0, 0.0, vp.scale(), vp.tx(), vp.ty());
        for e in scene.z_ordered() {
            render::render_element(&mut surface, e, &scene.elements, vp.scale());
        }
        for s in &scene.strokes {
            render::render_stroke(&mut surface, s);
        }
        // The live preview of a gesture.
        self.paint_preview(&mut surface);
        // Selection handles.
        for id in &self.selection {
            if let Some(el) = scene.elements.iter().find(|e| e.id() == id) {
                render::render_selection_handles(&mut surface, el, vp.scale());
            }
        }
        // The inline-edit caret.
        self.paint_caret(&mut surface, &scene);
        c.pop_clip();
    }

    fn paint_preview(&self, surface: &mut D2DSurface) {
        match &self.drag {
            Drag::DrawBox { tool, start, cur } => {
                let b = engine::gesture_box(&gesture(*start, *cur), |v| v);
                surface.save();
                surface.set_stroke("#1a73e8");
                surface.set_fill("rgba(26,115,232,0.08)");
                surface.set_line_width(1.5 / self.board.viewport.scale());
                surface.set_line_dash(&[4.0 / self.board.viewport.scale(), 2.0 / self.board.viewport.scale()]);
                surface.begin_path();
                surface.round_rect(b.x, b.y, b.w, b.h, if tool == "shape" { 6.0 } else { 2.0 });
                surface.fill();
                surface.stroke();
                surface.restore();
            }
            Drag::DrawArrow { start, cur } => {
                surface.save();
                surface.set_stroke("#444746");
                surface.set_line_width(2.0);
                surface.begin_path();
                surface.move_to(start.0, start.1);
                surface.line_to(cur.0, cur.1);
                surface.stroke();
                surface.restore();
            }
            Drag::Pen { points } if points.len() >= 4 => {
                surface.save();
                surface.set_stroke("#202124");
                surface.set_line_width(2.0);
                surface.begin_path();
                surface.move_to(points[0], points[1]);
                let mut i = 2;
                while i + 1 < points.len() {
                    surface.line_to(points[i], points[i + 1]);
                    i += 2;
                }
                surface.stroke();
                surface.restore();
            }
            _ => {}
        }
    }
}

/// The first non-empty line of `text` (for a Calques row), or `fallback` when the text is empty.
fn first_line(text: &str, fallback: &str) -> String {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    if line.is_empty() {
        fallback.to_string()
    } else {
        line.to_string()
    }
}

/// A [`engine::DrawGesture`] from two content-space points (no modifiers).
fn gesture(start: (f64, f64), cur: (f64, f64)) -> engine::DrawGesture {
    engine::DrawGesture { start_x: start.0, start_y: start.1, cur_x: cur.0, cur_y: cur.1, square: false, from_centre: false }
}

impl Control for BoardCanvas {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, _proposed: Size) -> Size {
        Size { width: 800.0, height: 600.0 }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        self.bounds = e.clip_rectangle;
        let c = e.canvas();
        self.paint_scene(c);
        if !self.design_mode() {
            let args = self.state_args();
            if self.reported.as_ref() != Some(&args) {
                self.reported = Some(args.clone());
                self.raise_state_changed(args);
                kubuno_desktop::controls::host::request_repaint_after(0);
            }
        }
        e.raise(self, "OnPaint");
    }

    fn on_mouse_wheel(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y, notches) = (e.args().x as f64, e.args().y as f64, e.args().delta as f64);
        let (lx, ly) = (x - self.bounds.left as f64, y - self.bounds.top as f64);
        self.board.viewport.zoom_at(1.1f64.powf(notches), lx, ly);
        self.changed();
        e.raise(&*self, "OnMouseWheel");
    }

    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        self.focus();
        if e.args().button != MouseButton::Left {
            e.raise(&*self, "OnMouseDown");
            return;
        }
        // A click outside the current edit commits it.
        if self.editing.is_some() {
            self.commit_edit();
        }
        let (px, py) = (e.args().x, e.args().y);
        let (wx, wy) = self.world(px, py);
        let scene = self.board.scene();
        match self.tool.as_str() {
            "hand" => self.drag = Drag::Pan { lx: px, ly: py },
            "pen" => self.drag = Drag::Pen { points: vec![wx, wy] },
            "arrow" => self.drag = Drag::DrawArrow { start: (wx, wy), cur: (wx, wy) },
            "sticky" | "text" | "shape" | "frame" => self.drag = Drag::DrawBox { tool: self.tool.clone(), start: (wx, wy), cur: (wx, wy) },
            "eraser" => {
                if let Some(i) = engine::hit_test(wx, wy, &scene.elements, 4.0 / self.board.viewport.scale()) {
                    let id = scene.elements[i].id().to_string();
                    let u = self.board.doc.remove_element(&id);
                    self.stop_capturing();
                    self.after_edit(u.unwrap_or_default());
                }
            }
            _ => {
                // Double-click a sticky / text box: edit its text in place.
                if e.args().clicks >= 2 {
                    if let Some(i) = engine::hit_test(wx, wy, &scene.elements, 4.0 / self.board.viewport.scale()) {
                        let kind = match &scene.elements[i] {
                            kubuno_office_whiteboard_core::WbElement::Sticky(_) => "sticky",
                            kubuno_office_whiteboard_core::WbElement::Text(_) => "text",
                            _ => "",
                        };
                        if Self::editable_kind(kind) {
                            let id = scene.elements[i].id().to_string();
                            self.start_edit(&id);
                            e.raise(&*self, "OnMouseDown");
                            return;
                        }
                    }
                }
                // Select: a handle of the single selection, else an element (with its group), else pan.
                if self.selection.len() == 1 {
                    if let Some(el) = scene.elements.iter().find(|e| e.id() == self.selection[0]) {
                        if let Some(h) = engine::hit_handle(wx, wy, el, self.board.viewport.scale()) {
                            self.drag = Drag::Resize { id: el.id().to_string(), handle: h, snap: self.element_json(&self.selection[0]).unwrap_or(Json::Null) };
                            e.raise(&*self, "OnMouseDown");
                            return;
                        }
                    }
                }
                match engine::hit_test(wx, wy, &scene.elements, 4.0 / self.board.viewport.scale()) {
                    Some(i) => {
                        let id = scene.elements[i].id().to_string();
                        if !self.selection.contains(&id) {
                            self.selection = vec![id];
                        }
                        let snap: Vec<(String, Json)> = self.selected_group().into_iter().filter_map(|id| self.element_json(&id).map(|j| (id, j))).collect();
                        self.drag = Drag::Move { start: (wx, wy), snap };
                    }
                    None => {
                        self.selection.clear();
                        self.drag = Drag::Pan { lx: px, ly: py };
                    }
                }
            }
        }
        self.changed();
        e.raise(&*self, "OnMouseDown");
    }

    fn on_mouse_move(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        if e.args().button != MouseButton::Left {
            if !matches!(self.drag, Drag::None) {
                self.drag = Drag::None;
            }
            e.raise(&*self, "OnMouseMove");
            return;
        }
        let (px, py) = (e.args().x, e.args().y);
        let (wx, wy) = self.world(px, py);
        match &mut self.drag {
            Drag::Pan { lx, ly } => {
                let (dx, dy) = ((px - *lx) as f64, (py - *ly) as f64);
                *lx = px;
                *ly = py;
                self.board.viewport.pan(dx, dy);
                self.changed();
            }
            Drag::Pen { points } => {
                points.push(wx);
                points.push(wy);
                self.changed();
            }
            Drag::DrawArrow { cur, .. } => {
                *cur = (wx, wy);
                self.changed();
            }
            Drag::DrawBox { cur, .. } => {
                *cur = (wx, wy);
                self.changed();
            }
            Drag::Move { start, snap } => {
                let (dx, dy) = (wx - start.0, wy - start.1);
                let snap = snap.clone();
                let mut update = Vec::new();
                for (id, orig) in &snap {
                    if let Some(moved) = move_element(orig, dx, dy) {
                        if let Some(u) = self.board.doc.set_element(id, &moved) {
                            update.extend_from_slice(&u);
                        }
                    }
                }
                self.after_edit(update);
            }
            Drag::Resize { id, handle, snap } => {
                let (id, handle, snap) = (id.clone(), *handle, snap.clone());
                if let Some(resized) = resize_element(&snap, handle, wx, wy) {
                    let u = self.board.doc.set_element(&id, &resized);
                    self.after_edit(u.unwrap_or_default());
                }
            }
            Drag::None => {}
        }
        e.raise(&*self, "OnMouseMove");
    }

    fn on_mouse_up(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let drag = std::mem::replace(&mut self.drag, Drag::None);
        match drag {
            Drag::DrawBox { tool, start, cur } => {
                let (id, obj) = self.build_box_element(&tool, start, cur);
                let u = self.board.doc.set_element(&id, &obj);
                self.selection = vec![id];
                self.stop_capturing();
                self.after_edit(u.unwrap_or_default());
            }
            Drag::DrawArrow { start, cur } => {
                let (id, obj) = build_arrow(start, cur, self.board.scene().max_z() + 1.0);
                let u = self.board.doc.set_element(&id, &obj);
                self.selection = vec![id];
                self.stop_capturing();
                self.after_edit(u.unwrap_or_default());
            }
            Drag::Pen { points } => {
                if points.len() >= 4 {
                    let stroke = json!({"id": fresh_uuid_v4(), "points": points, "color": "#202124", "width": 2.0, "opacity": 1.0, "tool": "pen", "userId": "", "createdAt": kubuno_desktop::controls::host::now_ms() as f64});
                    let u = self.board.doc.push_stroke(&stroke);
                    self.stop_capturing();
                    self.after_edit(u.unwrap_or_default());
                }
            }
            Drag::Move { .. } | Drag::Resize { .. } => {
                self.stop_capturing();
                self.changed();
            }
            _ => {}
        }
        e.raise(&*self, "OnMouseUp");
    }

    fn cursor_at(&self, _x: f32, _y: f32) -> Option<kubuno_desktop::controls::host::Cursor> {
        use kubuno_desktop::controls::host::Cursor as C;
        Some(match self.tool.as_str() {
            "hand" => C::Hand,
            "select" => C::Arrow,
            _ => C::Crosshair,
        })
    }

    fn is_input_key(&self, key: Keys) -> bool {
        use kubuno_desktop::controls::host::vk;
        if self.editing.is_some() {
            return [vk::DELETE, vk::BACK, vk::ESCAPE, vk::ENTER, vk::LEFT, vk::RIGHT, vk::HOME, vk::END].contains(&key.key.0);
        }
        false
    }

    fn on_key_down(&mut self, e: &mut EventCx<'_, KeyEventArgs>) {
        if self.editing.is_some() && self.edit_key(e.args().key.0) {
            e.args_mut().handled = true;
            e.args_mut().suppress_key_press = true;
        }
        e.raise(&*self, "OnKeyDown");
    }

    fn on_key_press(&mut self, e: &mut EventCx<'_, KeyPressEventArgs>) {
        let c = e.args().key_char;
        if self.editing.is_some() && !c.is_control() {
            self.edit_char(c);
            e.args_mut().handled = true;
        }
        e.raise(&*self, "OnKeyPress");
    }
}

impl BoardCanvas {
    /// Builds a new area element (shape / sticky / text / frame) from a draw gesture.
    fn build_box_element(&self, tool: &str, start: (f64, f64), cur: (f64, f64)) -> (String, Json) {
        let b: Rect = {
            let g = gesture(start, cur);
            let box_ = engine::gesture_final_box(if tool == "shape" { "rect" } else { tool }, &g, |v| v);
            Rect { x: box_.x, y: box_.y, w: box_.w, h: box_.h }
        };
        let id = fresh_uuid_v4();
        let z = self.board.scene().max_z() + 1.0;
        let obj = match tool {
            "sticky" => json!({"id": id, "type": "sticky", "x": b.x, "y": b.y, "width": b.w.max(160.0), "height": b.h.max(140.0), "rotation": 0.0, "opacity": 1.0, "zIndex": z, "locked": false, "text": "", "color": "yellow", "fontSize": 16.0, "textAlign": "left"}),
            "text" => json!({"id": id, "type": "text", "x": b.x, "y": b.y, "width": b.w.max(160.0), "height": b.h.max(40.0), "rotation": 0.0, "opacity": 1.0, "zIndex": z, "locked": false, "text": "Texte", "fontSize": 18.0, "fontWeight": "normal", "color": "#202124", "textAlign": "left"}),
            "frame" => json!({"id": id, "type": "frame", "x": b.x, "y": b.y, "width": b.w.max(200.0), "height": b.h.max(150.0), "rotation": 0.0, "opacity": 1.0, "zIndex": z, "locked": false, "title": "Cadre", "color": "#1a73e8"}),
            _ => json!({"id": id, "type": "shape", "x": b.x, "y": b.y, "width": b.w, "height": b.h, "rotation": 0.0, "opacity": 1.0, "zIndex": z, "locked": false, "kind": "rect", "fill": "#BBDEFB", "stroke": "#1a73e8", "strokeWidth": 2.0}),
        };
        (id, obj)
    }
}

/// Moves one element's JSON by `(dx, dy)` (an arrow moves its endpoints; everything else its `x`/`y`).
fn move_element(obj: &Json, dx: f64, dy: f64) -> Option<Json> {
    let mut o = obj.clone();
    let m = o.as_object_mut()?;
    let add = |m: &mut serde_json::Map<String, Json>, k: &str, d: f64| {
        if let Some(v) = m.get(k).and_then(Json::as_f64) {
            m.insert(k.to_string(), json!(v + d));
        }
    };
    if m.get("type").and_then(Json::as_str) == Some("arrow") {
        add(m, "startX", dx);
        add(m, "startY", dy);
        add(m, "endX", dx);
        add(m, "endY", dy);
    } else {
        add(m, "x", dx);
        add(m, "y", dy);
    }
    Some(o)
}

/// Resizes one element's JSON with a handle dragged to `(wx, wy)`. Arrows are not box-resized.
fn resize_element(obj: &Json, handle: ResizeHandle, wx: f64, wy: f64) -> Option<Json> {
    let mut o = obj.clone();
    let m = o.as_object_mut()?;
    if m.get("type").and_then(Json::as_str) == Some("arrow") {
        return None;
    }
    let x = m.get("x")?.as_f64()?;
    let y = m.get("y")?.as_f64()?;
    let w = m.get("width")?.as_f64()?;
    let h = m.get("height")?.as_f64()?;
    let (mut l, mut t, mut r, mut b) = (x, y, x + w, y + h);
    use ResizeHandle::*;
    match handle {
        Nw | W | Sw => l = wx,
        Ne | E | Se => r = wx,
        _ => {}
    }
    match handle {
        Nw | N | Ne => t = wy,
        Sw | S | Se => b = wy,
        _ => {}
    }
    let (nx, nw) = (l.min(r), (r - l).abs().max(8.0));
    let (ny, nh) = (t.min(b), (b - t).abs().max(8.0));
    m.insert("x".into(), json!(nx));
    m.insert("y".into(), json!(ny));
    m.insert("width".into(), json!(nw));
    m.insert("height".into(), json!(nh));
    Some(o)
}

/// Builds a new arrow element from two content-space points.
fn build_arrow(start: (f64, f64), cur: (f64, f64), z: f64) -> (String, Json) {
    let id = fresh_uuid_v4();
    let obj = json!({"id": id, "type": "arrow", "startX": start.0, "startY": start.1, "endX": cur.0, "endY": cur.1, "color": "#444746", "width": 2.0, "style": "straight", "startArrow": "none", "endArrow": "triangle", "zIndex": z, "opacity": 1.0});
    (id, obj)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sample_reports_its_counts() {
        let mut c = BoardCanvas::default();
        let s = c.state_args();
        assert_eq!(s.elements, 6);
        assert_eq!(s.zoom, 100.0);
        assert_eq!(s.tool, "select");
    }

    #[test]
    fn move_shifts_x_y_and_arrow_endpoints() {
        let shape = json!({"type": "shape", "x": 10.0, "y": 20.0, "width": 5.0, "height": 5.0});
        let m = move_element(&shape, 3.0, -4.0).unwrap();
        assert_eq!(m["x"], json!(13.0));
        assert_eq!(m["y"], json!(16.0));
        let arrow = json!({"type": "arrow", "startX": 0.0, "startY": 0.0, "endX": 10.0, "endY": 10.0});
        let m = move_element(&arrow, 1.0, 2.0).unwrap();
        assert_eq!(m["startX"], json!(1.0));
        assert_eq!(m["endY"], json!(12.0));
    }

    #[test]
    fn resize_se_grows_the_box() {
        let shape = json!({"type": "shape", "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0});
        let r = resize_element(&shape, ResizeHandle::Se, 40.0, 30.0).unwrap();
        assert_eq!(r["width"], json!(40.0));
        assert_eq!(r["height"], json!(30.0));
    }

    #[test]
    fn inline_editing_types_backspaces_and_commits_the_sticky_text() {
        use kubuno_desktop::controls::host::vk;
        let mut c = BoardCanvas::default();
        // The sample's sticky, found through the Calques rows.
        let id = c.element_rows().into_iter().find(|r| r.kind == "sticky").expect("sample has a sticky").id;
        let before = c.element_json(&id).and_then(|o| o["text"].as_str().map(str::to_string)).unwrap();
        c.start_edit(&id);
        assert!(c.editing.is_some(), "a double-click starts an edit");
        for ch in " MODIFIE".chars() {
            c.edit_char(ch);
        }
        assert!(c.edit_key(vk::BACK), "Backspace is an edit key");
        c.commit_edit();
        assert!(c.editing.is_none(), "the edit commits");
        let after = c.element_json(&id).and_then(|o| o["text"].as_str().map(str::to_string)).unwrap();
        assert_eq!(after, format!("{before} MODIFI"), "the typed text, less the backspaced last letter, is stored");
    }

    #[test]
    fn only_sticky_and_text_are_editable_in_place() {
        assert!(BoardCanvas::editable_kind("sticky"));
        assert!(BoardCanvas::editable_kind("text"));
        assert!(!BoardCanvas::editable_kind("shape"));
        assert!(!BoardCanvas::editable_kind("arrow"));
        assert!(!BoardCanvas::editable_kind("frame"));
    }
}
