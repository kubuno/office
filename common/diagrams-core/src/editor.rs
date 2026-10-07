//! The editor — the state and the handlers of `DiagramEditorPage` (`:993-3147`): the pages, the view
//! (zoom, pan, grid, magnetism, minimap, rulers), the selection, the armed shape tool, the history, the
//! pointer state machine (every drag the web knows), the context menus' targets and every command of
//! the ribbon, the panels and the menus. A platform drives it with pointer and key events in canvas
//! coordinates (CSS pixels / DIPs) and paints it with [`Editor::render`].
//!
//! Deliberate differences from the web, each a web bug that loses work (vskubuno
//! `docs/DIAGRAMS-DESKTOP.md` §6): the layers and the unknown keys of a page are kept when it loads and
//! when shapes are deleted, cut or pasted (the web rebuilds `{ shapes, connectors }` there and drops
//! them), and the history is kept per page (the web's single history restores another page's
//! snapshot into the current page after a page switch).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::canvas::{Ctx2D, Font, Point, Surface};
use crate::file::Page;
use crate::geometry::{
    self, canvas_to_world, connector_at, connector_label_center, connector_points, handle_at, is_container, js_round, magnetize_right_angle, port_at, shape_at, snap_to_grid, Handle,
    GRID_SIZE, HANDLE_MARGIN, HANDLE_R, MAX_ZOOM, MIN_ZOOM,
};
use crate::layout::{compute_layout, LayoutKind};
use crate::model::{js_number, Connector, IdSource, Layer, Obj, PageData, Shape};
use crate::render::{self, Scene};
use crate::stencils::{self, DrawingShape};

/// Snapshots kept per page (`pastRef`, 100 deep).
pub const HISTORY_DEPTH: usize = 100;

/// What measures text for the hit tests that need it (a connector label's box).
pub trait TextMeasure {
    fn text_width(&mut self, text: &str, font: &Font) -> f64;
}

impl<S: Surface + ?Sized> TextMeasure for S {
    fn text_width(&mut self, text: &str, font: &Font) -> f64 {
        self.measure_text(text, font).width
    }
}

/// Keyboard modifiers of a pointer event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
}

/// A mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Right,
}

/// The pointer the canvas shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cursor {
    Default,
    Move,
    Crosshair,
    Resize(Handle),
}

/// What a right click is on (`ctxMenu`), and where (world).
#[derive(Debug, Clone, PartialEq)]
pub enum ContextTarget {
    Shape { id: String },
    Connector { id: String, seg: usize, world: Point },
    Canvas { world: Point },
}

/// The inline label editor (`editingLabel`).
#[derive(Debug, Clone, PartialEq)]
pub struct LabelEdit {
    pub id: String,
    pub is_connector: bool,
    pub text: String,
}

/// Alignment of a multi-selection (`align`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
    Top,
    Middle,
    Bottom,
}

/// Paint order (`reorderSelection`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    Front,
    Back,
    Forward,
    Backward,
}

/// A gesture in progress (the web's drag refs).
#[derive(Debug, Clone, PartialEq)]
enum Gesture {
    None,
    Pan { start: Point, start_pan: Point },
    Move { shape_id: String, offsets: Vec<(String, f64, f64)> },
    Resize { shape_id: String, handle: Handle, start: Point, orig: (f64, f64, f64, f64) },
    Adjust { shape_id: String, index: usize },
    LabelDrag { conn_id: String, down: Point, orig: Point },
    Waypoint { conn_id: String, index: usize },
    PendingNode { conn_id: String, seg: usize, down: Point },
    Segment { conn_id: String, seg: usize, down: Point, started: bool, moving: Vec<usize>, orig: Vec<Point>, base: Vec<Point> },
    Connect { source_id: String, start: Point, current: Point },
    Lasso { a: Point, b: Point },
}

#[derive(Debug, Clone, Default)]
struct History {
    past: Vec<PageData>,
    future: Vec<PageData>,
}

/// The editor.
pub struct Editor {
    pages: Vec<Page>,
    current: usize,
    // ── View ──
    pub zoom: f64,
    pub pan_x: f64,
    pub pan_y: f64,
    /// The canvas size (CSS pixels / DIPs), set by the platform.
    pub viewport: (f64, f64),
    pub show_grid: bool,
    /// « Magnétisme »: snap to the 10 px grid.
    pub snap: bool,
    pub show_minimap: bool,
    pub show_rulers: bool,
    /// A viewer: pan and zoom only.
    pub read_only: bool,
    // ── Selection and tools ──
    selected: Vec<String>,
    selected_conns: Vec<String>,
    hovered: Option<String>,
    active_layer: String,
    armed: Option<String>,
    drawing: Option<DrawingShape>,
    gesture: Gesture,
    magnet: Vec<(String, Vec<usize>)>,
    guides: Option<(Vec<f64>, Vec<f64>)>,
    label_edit: Option<LabelEdit>,
    // ── History, clipboard, ids ──
    history: BTreeMap<String, History>,
    in_gesture: bool,
    gesture_pushed: bool,
    clipboard: Option<(Vec<Shape>, Vec<Connector>)>,
    ids: IdSource,
    /// Bumped by every change of a page's data (a save compares it).
    revision: u64,
    /// The pages whose data changed since [`Editor::mark_saved`].
    dirty: BTreeSet<String>,
    /// How a placed stencil is labelled (`t('stencil_' + id, { defaultValue: name })`).
    namer: Box<dyn Fn(&stencils::StencilDef) -> String>,
}

impl std::fmt::Debug for Editor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Editor").field("pages", &self.pages.len()).field("current", &self.current).field("zoom", &self.zoom).finish_non_exhaustive()
    }
}

impl Editor {
    /// An editor over `pages` (at least one: an empty page is added otherwise). `seed` seeds the ids.
    pub fn new(mut pages: Vec<Page>, seed: u64) -> Editor {
        if pages.is_empty() {
            pages.push(Page::new("page-1", "Page 1", PageData::default()));
        }
        let mut e = Editor {
            pages,
            current: 0,
            zoom: 1.0,
            pan_x: 60.0,
            pan_y: 60.0,
            viewport: (800.0, 600.0),
            show_grid: true,
            snap: false,
            show_minimap: true,
            show_rulers: false,
            read_only: false,
            selected: Vec::new(),
            selected_conns: Vec::new(),
            hovered: None,
            active_layer: crate::model::DEFAULT_LAYER_ID.to_string(),
            armed: None,
            drawing: None,
            gesture: Gesture::None,
            magnet: Vec::new(),
            guides: None,
            label_edit: None,
            history: BTreeMap::new(),
            in_gesture: false,
            gesture_pushed: false,
            clipboard: None,
            ids: IdSource::new(seed),
            revision: 0,
            dirty: BTreeSet::new(),
            namer: Box::new(|s| s.name.to_string()),
        };
        e.fix_active_layer();
        e
    }

    /// How placed stencils are labelled (the platform's translations).
    pub fn set_stencil_namer(&mut self, namer: Box<dyn Fn(&stencils::StencilDef) -> String>) {
        self.namer = namer;
    }

    // ── Pages ────────────────────────────────────────────────────────────────

    pub fn pages(&self) -> &[Page] {
        &self.pages
    }

    pub fn pages_mut(&mut self) -> &mut Vec<Page> {
        &mut self.pages
    }

    pub fn current_index(&self) -> usize {
        self.current
    }

    pub fn page(&self) -> &Page {
        &self.pages[self.current]
    }

    /// The current page's data (`data`).
    pub fn data(&self) -> &PageData {
        &self.pages[self.current].data
    }

    /// Shows page `i` (`setCurrentPageId`); the selection and any gesture are dropped.
    pub fn set_current_page(&mut self, i: usize) {
        if i < self.pages.len() && i != self.current {
            self.current = i;
            self.clear_selection();
            self.gesture = Gesture::None;
            self.drawing = None;
            self.label_edit = None;
            self.fix_active_layer();
        }
    }

    /// Adds a page (after a successful `POST …/pages`, or locally) and shows it.
    pub fn add_page(&mut self, page: Page) {
        self.pages.push(page);
        let last = self.pages.len() - 1;
        self.set_current_page(last);
        self.revision += 1;
    }

    /// Removes page `id` (never the last one); shows a neighbour when it was the current one.
    pub fn remove_page(&mut self, id: &str) -> bool {
        if self.pages.len() <= 1 {
            return false;
        }
        let Some(i) = self.pages.iter().position(|p| p.id == id) else { return false };
        let was_current = i == self.current;
        self.pages.remove(i);
        self.history.remove(id);
        self.dirty.remove(id);
        if was_current {
            self.current = 0;
            // `pageList.find(pg => pg.id !== p.id)`: the first other page.
            self.clear_selection();
        } else if i < self.current {
            self.current -= 1;
        }
        self.revision += 1;
        true
    }

    pub fn rename_page(&mut self, id: &str, name: &str) {
        if let Some(p) = self.pages.iter_mut().find(|p| p.id == id) {
            p.name = name.to_string();
            self.revision += 1;
        }
    }

    // ── Revision and dirt ────────────────────────────────────────────────────

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The ids of the pages changed since the last [`Editor::mark_saved`].
    pub fn dirty_pages(&self) -> Vec<String> {
        self.dirty.iter().cloned().collect()
    }

    pub fn is_dirty(&self) -> bool {
        !self.dirty.is_empty()
    }

    pub fn mark_saved(&mut self) {
        self.dirty.clear();
    }

    /// Every page's data, by id (what the save session saves).
    pub fn all_pages_data(&self) -> Vec<(String, PageData)> {
        self.pages.iter().map(|p| (p.id.clone(), p.data.clone())).collect()
    }

    // ── Layers ───────────────────────────────────────────────────────────────

    pub fn layers(&self) -> Vec<Layer> {
        self.data().layers()
    }

    pub fn active_layer(&self) -> &str {
        &self.active_layer
    }

    pub fn set_active_layer(&mut self, id: &str) {
        if self.layers().iter().any(|l| l.id() == id) {
            self.active_layer = id.to_string();
        }
    }

    /// Keeps the active layer valid (after deleting a layer, after a page switch).
    fn fix_active_layer(&mut self) {
        let layers = self.layers();
        if !layers.iter().any(|l| l.id() == self.active_layer) {
            self.active_layer = layers.first().map(|l| l.id().to_string()).unwrap_or_default();
        }
    }

    fn layer_of_obj<'a>(layer_id: Option<&'a str>, layers: &'a [Layer]) -> &'a str {
        layer_id.unwrap_or_else(|| layers.first().map(|l| l.id()).unwrap_or(crate::model::DEFAULT_LAYER_ID))
    }

    /// Layer-ordered (stable) shapes passing `keep(layer)`.
    fn by_layer_shapes(&self, keep: impl Fn(&Layer) -> bool) -> Vec<&Shape> {
        let layers = self.layers();
        let index = |lid: &str| layers.iter().position(|l| l.id() == lid).unwrap_or(0);
        let ok = |lid: &str| layers.iter().find(|l| l.id() == lid).map(&keep).unwrap_or(true);
        let mut v: Vec<&Shape> = self.data().shapes.iter().filter(|s| ok(Self::layer_of_obj(s.layer_id(), &layers))).collect();
        v.sort_by_key(|s| index(Self::layer_of_obj(s.layer_id(), &layers)));
        v
    }

    fn by_layer_conns(&self, keep: impl Fn(&Layer) -> bool) -> Vec<&Connector> {
        let layers = self.layers();
        let index = |lid: &str| layers.iter().position(|l| l.id() == lid).unwrap_or(0);
        let ok = |lid: &str| layers.iter().find(|l| l.id() == lid).map(&keep).unwrap_or(true);
        let mut v: Vec<&Connector> = self.data().connectors.iter().filter(|c| ok(Self::layer_of_obj(c.layer_id(), &layers))).collect();
        v.sort_by_key(|c| index(Self::layer_of_obj(c.layer_id(), &layers)));
        v
    }

    /// `pickShapes`: visible and unlocked layers, layer-ordered.
    fn pick_shapes(&self) -> Vec<&Shape> {
        self.by_layer_shapes(|l| l.visible() && !l.locked())
    }

    fn pick_conns(&self) -> Vec<&Connector> {
        self.by_layer_conns(|l| l.visible() && !l.locked())
    }

    /// `visibleData`: what is drawn (hidden layers removed), layer-ordered.
    pub fn visible_data(&self) -> (Vec<Shape>, Vec<Connector>) {
        (self.by_layer_shapes(Layer::visible).into_iter().cloned().collect(), self.by_layer_conns(Layer::visible).into_iter().cloned().collect())
    }

    fn is_pickable_shape(&self, s: &Shape) -> bool {
        let layers = self.layers();
        let lid = Self::layer_of_obj(s.layer_id(), &layers);
        layers.iter().find(|l| l.id() == lid).map(|l| l.visible() && !l.locked()).unwrap_or(true)
    }

    // ── Selection and state ──────────────────────────────────────────────────

    pub fn selected(&self) -> &[String] {
        &self.selected
    }

    pub fn selected_conns(&self) -> &[String] {
        &self.selected_conns
    }

    pub fn set_selection(&mut self, shapes: Vec<String>, conns: Vec<String>) {
        self.selected = shapes;
        self.selected_conns = conns;
    }

    pub fn clear_selection(&mut self) {
        self.selected.clear();
        self.selected_conns.clear();
    }

    pub fn has_selection(&self) -> bool {
        !self.selected.is_empty() || !self.selected_conns.is_empty()
    }

    /// `selectedShape`: the shape when exactly one is selected.
    pub fn selected_shape(&self) -> Option<&Shape> {
        match self.selected.as_slice() {
            [one] => self.data().shape(one),
            _ => None,
        }
    }

    /// `selectedConn`.
    pub fn selected_conn(&self) -> Option<&Connector> {
        match self.selected_conns.as_slice() {
            [one] => self.data().connector(one),
            _ => None,
        }
    }

    /// The shape the « Forme » tab reads (`diagActiveShape`): the selected one, else the first selected.
    pub fn active_shape(&self) -> Option<&Shape> {
        self.selected_shape().or_else(|| self.data().shapes.iter().find(|s| self.selected.iter().any(|id| id == s.id())))
    }

    /// `selectionHasGroup`.
    pub fn selection_has_group(&self) -> bool {
        self.data().shapes.iter().any(|s| self.selected.iter().any(|id| id == s.id()) && s.group_id().is_some())
    }

    pub fn hovered(&self) -> Option<&str> {
        self.hovered.as_deref()
    }

    pub fn armed(&self) -> Option<&str> {
        self.armed.as_deref()
    }

    pub fn label_edit(&self) -> Option<&LabelEdit> {
        self.label_edit.as_ref()
    }

    pub fn can_undo(&self) -> bool {
        self.history.get(&self.page().id).is_some_and(|h| !h.past.is_empty())
    }

    pub fn can_redo(&self) -> bool {
        self.history.get(&self.page().id).is_some_and(|h| !h.future.is_empty())
    }

    pub fn has_clipboard(&self) -> bool {
        self.clipboard.as_ref().is_some_and(|(s, _)| !s.is_empty())
    }

    /// A gesture is in progress (the platform captures the pointer).
    pub fn is_dragging(&self) -> bool {
        self.gesture != Gesture::None || self.drawing.is_some()
    }

    // ── Mutation and history ─────────────────────────────────────────────────

    fn grid_step(&self) -> f64 {
        if self.snap {
            GRID_SIZE
        } else {
            1.0
        }
    }

    fn snapv(&self, v: f64) -> f64 {
        snap_to_grid(v, self.grid_step())
    }

    fn push_history(&mut self) {
        let snap = self.data().clone();
        let h = self.history.entry(self.pages[self.current].id.clone()).or_default();
        h.past.push(snap);
        if h.past.len() > HISTORY_DEPTH {
            h.past.remove(0);
        }
        h.future.clear();
    }

    /// `mutateData`: records history (once per gesture), applies `f` to the current page.
    pub fn mutate(&mut self, f: impl FnOnce(&mut PageData)) {
        if self.read_only {
            return;
        }
        if self.in_gesture {
            if !self.gesture_pushed {
                self.push_history();
                self.gesture_pushed = true;
            }
        } else {
            self.push_history();
        }
        f(&mut self.pages[self.current].data);
        self.touched();
    }

    fn touched(&mut self) {
        self.revision += 1;
        let id = self.pages[self.current].id.clone();
        self.dirty.insert(id);
    }

    pub fn undo(&mut self) {
        let id = self.page().id.clone();
        let current = self.data().clone();
        let Some(h) = self.history.get_mut(&id) else { return };
        let Some(target) = h.past.pop() else { return };
        h.future.push(current);
        self.clear_selection();
        self.pages[self.current].data = target;
        self.fix_active_layer();
        self.touched();
    }

    pub fn redo(&mut self) {
        let id = self.page().id.clone();
        let current = self.data().clone();
        let Some(h) = self.history.get_mut(&id) else { return };
        let Some(target) = h.future.pop() else { return };
        h.past.push(current);
        self.clear_selection();
        self.pages[self.current].data = target;
        self.fix_active_layer();
        self.touched();
    }

    /// Replaces page `id`'s data from outside (the server's copy after « take theirs »): no history.
    pub fn replace_page_data(&mut self, id: &str, data: PageData) {
        if let Some(i) = self.pages.iter().position(|p| p.id == id) {
            self.pages[i].data = data;
            self.history.remove(id);
            if i == self.current {
                self.clear_selection();
                self.fix_active_layer();
            }
            self.revision += 1;
        }
    }

    pub fn make_id(&mut self) -> String {
        self.ids.make_id()
    }

    // ── View ─────────────────────────────────────────────────────────────────

    pub fn world(&self, x: f64, y: f64) -> Point {
        canvas_to_world(x, y, self.pan_x, self.pan_y, self.zoom)
    }

    pub fn set_zoom(&mut self, z: f64) {
        self.zoom = z.clamp(MIN_ZOOM, MAX_ZOOM);
    }

    /// « Zoom avant »: `min(MAX, +(z × 1.2).toFixed(2))`.
    pub fn zoom_in(&mut self) {
        self.zoom = MAX_ZOOM.min(to_fixed2(self.zoom * 1.2));
    }

    pub fn zoom_out(&mut self) {
        self.zoom = MIN_ZOOM.max(to_fixed2(self.zoom / 1.2));
    }

    /// « Réinitialiser »: 100 %, pan 60/60.
    pub fn reset_view(&mut self) {
        self.zoom = 1.0;
        self.pan_x = 60.0;
        self.pan_y = 60.0;
    }

    /// « Ajuster » (`zoomToFit`): every shape in view with a 50 px margin.
    pub fn zoom_to_fit(&mut self) {
        let shapes = &self.data().shapes;
        if shapes.is_empty() {
            self.reset_view();
            return;
        }
        let min_x = shapes.iter().map(Shape::x).fold(f64::INFINITY, f64::min);
        let min_y = shapes.iter().map(Shape::y).fold(f64::INFINITY, f64::min);
        let max_x = shapes.iter().map(|s| s.x() + s.w()).fold(f64::NEG_INFINITY, f64::max);
        let max_y = shapes.iter().map(|s| s.y() + s.h()).fold(f64::NEG_INFINITY, f64::max);
        let pad = 50.0;
        let (vw, vh) = self.viewport;
        let z = ((vw - 2.0 * pad) / (max_x - min_x).max(1.0)).min((vh - 2.0 * pad) / (max_y - min_y).max(1.0)).clamp(MIN_ZOOM, MAX_ZOOM);
        self.zoom = z;
        self.pan_x = pad - min_x * z;
        self.pan_y = pad - min_y * z;
    }

    /// The wheel (`handleWheel`): ×1.1 or ×0.9 a notch about the pointer.
    pub fn wheel(&mut self, x: f64, y: f64, delta_y: f64) {
        let delta = if delta_y > 0.0 { 0.9 } else { 1.1 };
        let nz = (self.zoom * delta).clamp(MIN_ZOOM, MAX_ZOOM);
        self.pan_x = x - (x - self.pan_x) * (nz / self.zoom);
        self.pan_y = y - (y - self.pan_y) * (nz / self.zoom);
        self.zoom = nz;
    }

    /// A click in the minimap (`minimapJump`): centres the view on that world point.
    pub fn minimap_jump(&mut self, wx: f64, wy: f64) {
        self.pan_x = self.viewport.0 / 2.0 - wx * self.zoom;
        self.pan_y = self.viewport.1 / 2.0 - wy * self.zoom;
    }

    // ── Painting ─────────────────────────────────────────────────────────────

    /// Paints the canvas (`requestRender`), the context in CSS pixels.
    pub fn render(&self, ctx: &mut Ctx2D) {
        let (shapes, conns) = self.visible_data();
        let (drawing_conn, lasso) = match &self.gesture {
            Gesture::Connect { start, current, .. } => (Some((*start, *current)), None),
            Gesture::Lasso { a, b } => (None, Some((*a, *b))),
            _ => (None, None),
        };
        let guides = self.guides.as_ref().map(|(v, h)| (v.as_slice(), h.as_slice()));
        render::render_canvas(
            ctx,
            &Scene {
                shapes: &shapes,
                connectors: &conns,
                zoom: self.zoom,
                pan_x: self.pan_x,
                pan_y: self.pan_y,
                selected: &self.selected,
                selected_conns: &self.selected_conns,
                hovered: self.hovered.as_deref(),
                drawing_conn,
                lasso,
                bg_color: &self.page().bg_color,
                magnet_segs: &self.magnet,
                show_grid: self.show_grid,
                grid_size: GRID_SIZE,
                guides,
                drawing_shape: self.drawing.as_ref(),
                width: self.viewport.0,
                height: self.viewport.1,
            },
        );
    }

    // ── Pointer ──────────────────────────────────────────────────────────────

    fn label_hit(&self, w: Point, measure: &mut dyn TextMeasure) -> Option<String> {
        let font = Font::parse("12px Inter, sans-serif").unwrap_or_default();
        let shapes = &self.data().shapes;
        for c in self.data().connectors.iter().rev() {
            if c.label().is_empty() {
                continue;
            }
            let center = connector_label_center(c, shapes);
            let half_w = measure.text_width(c.label(), &font) / 2.0 + 6.0;
            if (w.x - center.x).abs() <= half_w && (w.y - center.y).abs() <= 11.0 {
                return Some(c.id().to_string());
            }
        }
        None
    }

    /// `handleMouseDown` at canvas point `(x, y)`.
    pub fn pointer_down(&mut self, x: f64, y: f64, button: Button, mods: Mods, measure: &mut dyn TextMeasure) {
        if self.label_edit.is_some() {
            self.commit_label();
            return;
        }
        let w = self.world(x, y);
        if button == Button::Middle || (button == Button::Left && mods.alt) || (self.read_only && button == Button::Left) {
            self.gesture = Gesture::Pan { start: Point::new(x, y), start_pan: Point::new(self.pan_x, self.pan_y) };
            return;
        }
        if button != Button::Left {
            return;
        }
        self.in_gesture = true;
        self.gesture_pushed = false;

        if let Some(kind) = self.armed.clone() {
            let step = self.grid_step();
            self.drawing = Some(stencils::begin_draw(&kind, w.x, w.y, &|v| snap_to_grid(v, step)));
            return;
        }

        let zoom = self.zoom;
        let pick = self.pick_shapes();
        if let Some((si, _, p)) = port_at(&pick, w.x, w.y) {
            let source_id = pick[si].id().to_string();
            self.gesture = Gesture::Connect { source_id, start: p, current: p };
            return;
        }

        if self.selected.len() == 1 {
            if let Some(only) = self.data().shape(&self.selected[0]) {
                if self.is_pickable_shape(only) {
                    if let Some(ai) = stencils::hit_shape_adjust(only, w.x, w.y, 7.0 / zoom) {
                        self.gesture = Gesture::Adjust { shape_id: only.id().to_string(), index: ai };
                        return;
                    }
                }
            }
        }

        if let Some((s, handle)) = handle_at(&self.data().shapes, &self.selected, w.x, w.y, HANDLE_MARGIN / zoom) {
            self.gesture = Gesture::Resize { shape_id: s.id().to_string(), handle, start: w, orig: (s.x(), s.y(), s.w(), s.h()) };
            return;
        }

        if let Some(cid) = self.label_hit(w, measure) {
            let orig = self.data().connector(&cid).map(Connector::label_offset).unwrap_or_default();
            self.selected_conns = vec![cid.clone()];
            self.selected.clear();
            self.gesture = Gesture::LabelDrag { conn_id: cid, down: w, orig };
            return;
        }

        if !self.selected_conns.is_empty() {
            let tol = (HANDLE_R + 8.0) / zoom;
            let shapes = &self.data().shapes;
            for c in &self.data().connectors {
                if !self.selected_conns.iter().any(|id| id == c.id()) {
                    continue;
                }
                let wps = c.waypoints();
                if let Some(wi) = wps.iter().position(|p| (p.x - w.x).hypot(p.y - w.y) <= tol) {
                    self.gesture = Gesture::Waypoint { conn_id: c.id().to_string(), index: wi };
                    return;
                }
                let pts = connector_points(c, shapes);
                for s in 0..pts.len().saturating_sub(1) {
                    let (mx, my) = ((pts[s].x + pts[s + 1].x) / 2.0, (pts[s].y + pts[s + 1].y) / 2.0);
                    if (mx - w.x).hypot(my - w.y) <= tol {
                        self.gesture = Gesture::PendingNode { conn_id: c.id().to_string(), seg: s, down: w };
                        return;
                    }
                }
            }
        }

        let pick = self.pick_shapes();
        if let Some(i) = shape_at(&pick, w.x, w.y) {
            let shape = pick[i].clone();
            let members: Vec<String> = match shape.group_id() {
                Some(g) => self.data().shapes.iter().filter(|s| s.group_id() == Some(g)).map(|s| s.id().to_string()).collect(),
                None => vec![shape.id().to_string()],
            };
            let sel_set: Vec<String> = if self.selected.iter().any(|id| id == shape.id()) {
                self.selected.clone()
            } else {
                let next = if mods.shift { union(&self.selected, &members) } else { members };
                self.selected = next.clone();
                self.selected_conns.clear();
                next
            };
            let mut moving = sel_set.clone();
            let shapes = &self.data().shapes;
            for sid in &sel_set {
                let Some(cs) = shapes.iter().find(|s| s.id() == sid) else { continue };
                if !is_container(cs.kind()) {
                    continue;
                }
                for o in shapes {
                    if moving.iter().any(|m| m == o.id()) || is_container(o.kind()) {
                        continue;
                    }
                    let (ocx, ocy) = (o.x() + o.w() / 2.0, o.y() + o.h() / 2.0);
                    if ocx > cs.x() && ocx < cs.x() + cs.w() && ocy > cs.y() && ocy < cs.y() + cs.h() {
                        moving.push(o.id().to_string());
                    }
                }
            }
            let offsets = moving.iter().filter_map(|sid| shapes.iter().find(|s| s.id() == sid).map(|s| (sid.clone(), s.x() - w.x, s.y() - w.y))).collect();
            self.gesture = Gesture::Move { shape_id: shape.id().to_string(), offsets };
            return;
        }

        let pick_c = self.pick_conns();
        if let Some((ci, seg)) = connector_at(&pick_c, &self.data().shapes, w.x, w.y, 10.0 / zoom) {
            let cid = pick_c[ci].id().to_string();
            self.selected_conns = vec![cid.clone()];
            self.selected.clear();
            self.gesture = Gesture::Segment { conn_id: cid, seg, down: w, started: false, moving: Vec::new(), orig: Vec::new(), base: Vec::new() };
            return;
        }

        self.clear_selection();
        self.gesture = Gesture::Lasso { a: w, b: w };
    }

    /// `handleMouseMove`; returns the cursor to show when it changes.
    pub fn pointer_move(&mut self, x: f64, y: f64, mods: Mods) -> Option<Cursor> {
        let w = self.world(x, y);
        let zoom = self.zoom;
        match self.gesture.clone() {
            Gesture::Pan { start, start_pan } => {
                self.pan_x = start_pan.x + (x - start.x);
                self.pan_y = start_pan.y + (y - start.y);
                return None;
            }
            _ if self.drawing.is_some() => {
                let step = self.grid_step();
                if let Some(d) = self.drawing.take() {
                    self.drawing = Some(stencils::update_draw(&d, w.x, w.y, mods.shift, mods.alt, &|v| snap_to_grid(v, step)));
                }
                return None;
            }
            Gesture::Adjust { shape_id, index } => {
                let next = self.data().shape(&shape_id).and_then(|s| stencils::shape_adjust_from_drag(s, index, w.x, w.y));
                if let Some(adj) = next {
                    self.mutate(|d| {
                        if let Some(s) = d.shape_mut(&shape_id) {
                            s.set_adj(&adj);
                        }
                    });
                }
                return None;
            }
            Gesture::LabelDrag { conn_id, down, orig } => {
                let p = Point::new(orig.x + (w.x - down.x), orig.y + (w.y - down.y));
                self.mutate(|d| {
                    if let Some(c) = d.connector_mut(&conn_id) {
                        c.set_label_offset(p);
                    }
                });
                return None;
            }
            Gesture::Resize { shape_id, handle, start, orig } => {
                let (dx, dy) = (w.x - start.x, w.y - start.y);
                self.mutate(|d| {
                    if let Some(s) = d.shape_mut(&shape_id) {
                        let (ox, oy, ow, oh) = orig;
                        let (mut nx, mut ny, mut nw, mut nh) = (s.x(), s.y(), s.w(), s.h());
                        if handle.has_e() {
                            nw = (ow + dx).max(20.0);
                        }
                        if handle.has_s() {
                            nh = (oh + dy).max(20.0);
                        }
                        if handle.has_w() {
                            nx = ox + dx;
                            nw = (ow - dx).max(20.0);
                        }
                        if handle.has_n() {
                            ny = oy + dy;
                            nh = (oh - dy).max(20.0);
                        }
                        s.set_rect(nx, ny, nw, nh);
                    }
                });
                return None;
            }
            Gesture::Move { shape_id, offsets } => {
                self.drag_move(&shape_id, &offsets, w);
                return None;
            }
            Gesture::Segment { conn_id, seg, down, started, moving, orig, base } => {
                self.drag_segment(conn_id, seg, down, started, moving, orig, base, w);
                return None;
            }
            Gesture::PendingNode { conn_id, seg, down } => {
                if (w.x - down.x).hypot(w.y - down.y) > 3.0 / zoom {
                    let p = Point::new(self.snapv(w.x), self.snapv(w.y));
                    self.mutate(|d| {
                        if let Some(c) = d.connector_mut(&conn_id) {
                            let mut wps = c.waypoints();
                            wps.insert(seg.min(wps.len()), p);
                            c.set_waypoints(&wps);
                        }
                    });
                    self.gesture = Gesture::Waypoint { conn_id, index: seg };
                }
                return None;
            }
            Gesture::Waypoint { conn_id, index } => {
                let mut np = Point::new(self.snapv(w.x), self.snapv(w.y));
                let mut green = None;
                if let Some(c) = self.data().connector(&conn_id) {
                    let pts = connector_points(c, &self.data().shapes);
                    if let (Some(p), Some(n)) = (pts.get(index), pts.get(index + 2)) {
                        let (m, snapped) = magnetize_right_angle(np, *p, *n, 15.0);
                        np = m;
                        if snapped {
                            green = Some(vec![index, index + 1]);
                        }
                    }
                }
                self.magnet = green.map(|g| vec![(conn_id.clone(), g)]).unwrap_or_default();
                self.mutate(|d| {
                    if let Some(c) = d.connector_mut(&conn_id) {
                        let mut wps = c.waypoints();
                        if let Some(p) = wps.get_mut(index) {
                            *p = np;
                        }
                        c.set_waypoints(&wps);
                    }
                });
                return None;
            }
            Gesture::Connect { source_id, start, .. } => {
                self.gesture = Gesture::Connect { source_id, start, current: w };
                return None;
            }
            Gesture::Lasso { a, .. } => {
                self.gesture = Gesture::Lasso { a, b: w };
                return None;
            }
            Gesture::None => {}
        }

        if self.armed.is_some() {
            self.hovered = None;
            return Some(Cursor::Crosshair);
        }
        if self.read_only {
            return Some(Cursor::Default);
        }
        let pick = self.pick_shapes();
        let hit = shape_at(&pick, w.x, w.y).map(|i| pick[i].id().to_string());
        let over_shape = hit.is_some();
        self.hovered = hit;
        if over_shape {
            return Some(Cursor::Move);
        }
        if let Some((_, h)) = handle_at(&self.data().shapes, &self.selected, w.x, w.y, HANDLE_MARGIN / zoom) {
            return Some(Cursor::Resize(h));
        }
        if !self.selected_conns.is_empty() && connector_at(&self.pick_conns(), &self.data().shapes, w.x, w.y, 8.0 / zoom).is_some() {
            return Some(Cursor::Move);
        }
        Some(Cursor::Default)
    }

    /// The shape drag: connector magnetism (8 px), then alignment guides, then the move.
    fn drag_move(&mut self, shape_id: &str, offsets: &[(String, f64, f64)], w: Point) {
        let snap_px = 8.0 / self.zoom;
        let (mut snap_x, mut snap_y) = (None::<f64>, None::<f64>);
        let mut green: Vec<(String, Vec<usize>)> = Vec::new();
        let data = self.data();
        if offsets.len() == 1 {
            if let Some(s) = data.shape(shape_id) {
                let (_, odx, ody) = offsets[0];
                let cx = js_round(w.x + odx) + s.w() / 2.0;
                let cy = js_round(w.y + ody) + s.h() / 2.0;
                let center_of = |id: Option<&str>| id.and_then(|id| data.shape(id)).map(Shape::center);
                let mut items: Vec<(String, usize, Point)> = Vec::new();
                for c in data.connectors.iter().filter(|c| c.source_id() == Some(s.id()) || c.target_id() == Some(s.id())) {
                    let wps = c.waypoints();
                    let is_source = c.source_id() == Some(s.id());
                    let (seg, r) = if is_source {
                        (0, wps.first().copied().or_else(|| if c.target_id().is_some() { center_of(c.target_id()) } else { c.target_point() }))
                    } else {
                        (wps.len(), wps.last().copied().or_else(|| if c.source_id().is_some() { center_of(c.source_id()) } else { c.source_point() }))
                    };
                    if let Some(r) = r {
                        items.push((c.id().to_string(), seg, r));
                    }
                }
                let mut best_h: Option<(f64, f64)> = None;
                let mut best_v: Option<(f64, f64)> = None;
                for (_, _, r) in &items {
                    let off_y = (r.y - cy).abs();
                    let off_x = (r.x - cx).abs();
                    if off_y <= snap_px && best_h.is_none_or(|b| off_y < b.1) {
                        best_h = Some((r.y, off_y));
                    }
                    if off_x <= snap_px && best_v.is_none_or(|b| off_x < b.1) {
                        best_v = Some((r.x, off_x));
                    }
                }
                snap_y = best_h.map(|b| b.0);
                snap_x = best_v.map(|b| b.0);
                let final_cy = snap_y.unwrap_or(cy);
                let final_cx = snap_x.unwrap_or(cx);
                for (cid, seg, r) in &items {
                    let aligned = (snap_y.is_some() && (r.y - final_cy).abs() < 0.5) || (snap_x.is_some() && (r.x - final_cx).abs() < 0.5);
                    if !aligned {
                        continue;
                    }
                    match green.iter_mut().find(|(id, _)| id == cid) {
                        Some((_, segs)) => {
                            if !segs.contains(seg) {
                                segs.push(*seg);
                            }
                        }
                        None => green.push((cid.clone(), vec![*seg])),
                    }
                }
            }
        }
        let (mut align_x, mut align_y) = (None::<f64>, None::<f64>);
        let mut guides = None;
        if offsets.len() == 1 {
            if let Some(s) = data.shape(shape_id) {
                let (_, odx, ody) = offsets[0];
                let (px, py) = (js_round(w.x + odx), js_round(w.y + ody));
                let sx = [px, px + s.w() / 2.0, px + s.w()];
                let sy = [py, py + s.h() / 2.0, py + s.h()];
                let mut best_v: Option<(f64, f64, f64)> = None;
                let mut best_h: Option<(f64, f64, f64)> = None;
                for o in data.shapes.iter().filter(|o| o.id() != s.id()) {
                    let ox = [o.x(), o.x() + o.w() / 2.0, o.x() + o.w()];
                    let oy = [o.y(), o.y() + o.h() / 2.0, o.y() + o.h()];
                    for a in sx {
                        for b in ox {
                            let d = (a - b).abs();
                            if d <= snap_px && best_v.is_none_or(|v| d < v.2) {
                                best_v = Some((b, b - a, d));
                            }
                        }
                    }
                    for a in sy {
                        for b in oy {
                            let d = (a - b).abs();
                            if d <= snap_px && best_h.is_none_or(|v| d < v.2) {
                                best_h = Some((b, b - a, d));
                            }
                        }
                    }
                }
                let (mut gv, mut gh) = (Vec::new(), Vec::new());
                if let (None, Some((line, adjust, _))) = (snap_x, best_v) {
                    align_x = Some(px + adjust);
                    gv.push(line);
                }
                if let (None, Some((line, adjust, _))) = (snap_y, best_h) {
                    align_y = Some(py + adjust);
                    gh.push(line);
                }
                if !gv.is_empty() || !gh.is_empty() {
                    guides = Some((gv, gh));
                }
            }
        }
        self.magnet = green;
        self.guides = guides;
        let shape_id = shape_id.to_string();
        let offsets = offsets.to_vec();
        self.mutate(|d| {
            for s in d.shapes.iter_mut() {
                let Some((_, odx, ody)) = offsets.iter().find(|(id, _, _)| id == s.id()) else { continue };
                let (mut nx, mut ny) = (js_round(w.x + odx), js_round(w.y + ody));
                if s.id() == shape_id {
                    if let Some(sx) = snap_x {
                        nx = js_round(sx - s.w() / 2.0);
                    } else if let Some(ax) = align_x {
                        nx = js_round(ax);
                    }
                    if let Some(sy) = snap_y {
                        ny = js_round(sy - s.h() / 2.0);
                    } else if let Some(ay) = align_y {
                        ny = js_round(ay);
                    }
                }
                s.set_pos(nx, ny);
            }
        });
    }

    /// A connector portion dragged (`segDragRef`): two elbows made on a straight segment, the moved
    /// elbows snapped to 90°.
    #[allow(clippy::too_many_arguments)]
    fn drag_segment(&mut self, conn_id: String, seg: usize, down: Point, started: bool, mut moving: Vec<usize>, mut orig: Vec<Point>, mut base: Vec<Point>, w: Point) {
        let (ddx, ddy) = (w.x - down.x, w.y - down.y);
        let Some(c) = self.data().connector(&conn_id).cloned() else {
            self.gesture = Gesture::None;
            return;
        };
        if !started {
            if ddx.hypot(ddy) <= 3.0 / self.zoom {
                return;
            }
            let wps = c.waypoints();
            let left = seg.checked_sub(1);
            let left_is = left.is_some_and(|l| l < wps.len());
            let right_is = seg < wps.len();
            if left_is || right_is {
                moving = [left.filter(|_| left_is), right_is.then_some(seg)].into_iter().flatten().collect();
                orig = moving.iter().map(|&i| wps[i]).collect();
                base = wps;
            } else {
                let pts = connector_points(&c, &self.data().shapes);
                // `seg` may index the displayed polyline (orthogonal/curved has more segments).
                let (Some(p0), Some(p1)) = (pts.get(seg), pts.get(seg + 1)) else {
                    self.gesture = Gesture::None;
                    return;
                };
                let a = Point::new(self.snapv(p0.x), self.snapv(p0.y));
                let b = Point::new(self.snapv(p1.x), self.snapv(p1.y));
                let mut nb = wps[..seg.min(wps.len())].to_vec();
                nb.push(a);
                nb.push(b);
                nb.extend_from_slice(&wps[seg.min(wps.len())..]);
                base = nb;
                moving = vec![seg, seg + 1];
                orig = vec![a, b];
            }
        }
        let cand: Vec<Point> = base
            .iter()
            .enumerate()
            .map(|(i, p)| match moving.iter().position(|&m| m == i) {
                Some(k) => Point::new(self.snapv(orig[k].x + ddx), self.snapv(orig[k].y + ddy)),
                None => *p,
            })
            .collect();
        let mut probe = c.clone();
        probe.set_waypoints(&cand);
        let poly = connector_points(&probe, &self.data().shapes);
        let mut green: Vec<usize> = Vec::new();
        let final_wps: Vec<Point> = cand
            .iter()
            .enumerate()
            .map(|(i, p)| {
                if !moving.contains(&i) {
                    return *p;
                }
                match (poly.get(i), poly.get(i + 2)) {
                    (Some(pp), Some(nn)) => {
                        let (m, snapped) = magnetize_right_angle(poly[i + 1], *pp, *nn, 15.0);
                        if snapped {
                            green.push(i);
                            green.push(i + 1);
                        }
                        m
                    }
                    _ => *p,
                }
            })
            .collect();
        self.magnet = if green.is_empty() { Vec::new() } else { vec![(conn_id.clone(), green)] };
        let id = conn_id.clone();
        self.mutate(|d| {
            if let Some(cc) = d.connector_mut(&id) {
                cc.set_waypoints(&final_wps);
            }
        });
        self.gesture = Gesture::Segment { conn_id, seg, down, started: true, moving, orig, base };
    }

    /// `handleMouseUp`.
    pub fn pointer_up(&mut self, x: f64, y: f64) {
        let w = self.world(x, y);
        self.in_gesture = false;
        self.gesture_pushed = false;
        self.magnet.clear();
        self.guides = None;
        if let Some(d) = self.drawing.take() {
            if let Some(st) = stencils::stencil(&d.kind) {
                let step = self.grid_step();
                let b = stencils::finish_draw(&d, 6.0 / self.zoom, (st.default_w, st.default_h), &|v| snap_to_grid(v, step));
                self.place_stencil_box(st, b.x, b.y, b.w, b.h);
            }
            self.armed = None;
            self.gesture = Gesture::None;
            return;
        }
        let g = std::mem::replace(&mut self.gesture, Gesture::None);
        match g {
            Gesture::Connect { source_id, .. } => {
                let pick = self.pick_shapes();
                let target = port_at(&pick, w.x, w.y).map(|(i, _, _)| pick[i].id().to_string()).or_else(|| shape_at(&pick, w.x, w.y).map(|i| pick[i].id().to_string()));
                if let Some(target) = target.filter(|t| *t != source_id) {
                    let id = self.make_id();
                    let conn = Connector::between(&id, &source_id, &target, &self.active_layer);
                    self.mutate(|d| d.connectors.push(conn));
                }
            }
            Gesture::Lasso { a, b } => {
                let (x1, y1, x2, y2) = (a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y));
                if (x2 - x1).abs() > 5.0 || (y2 - y1).abs() > 5.0 {
                    let inside: Vec<String> = self
                        .data()
                        .shapes
                        .iter()
                        .filter(|s| self.is_pickable_shape(s) && s.x() >= x1 && s.x() + s.w() <= x2 && s.y() >= y1 && s.y() + s.h() <= y2)
                        .map(|s| s.id().to_string())
                        .collect();
                    self.selected = inside;
                }
            }
            _ => {}
        }
    }

    /// The pointer left the canvas with no button held (`onMouseLeave` with no gesture): the hover
    /// ends.
    pub fn pointer_leave(&mut self) {
        self.hovered = None;
    }

    /// `handleDblClick`: edits a connector's label, a shape's text, or removes a waypoint.
    pub fn double_click(&mut self, x: f64, y: f64, measure: &mut dyn TextMeasure) {
        if self.read_only {
            return;
        }
        let w = self.world(x, y);
        if let Some(cid) = self.label_hit(w, measure) {
            self.selected_conns = vec![cid.clone()];
            let text = self.data().connector(&cid).map(|c| c.label().to_string()).unwrap_or_default();
            self.label_edit = Some(LabelEdit { id: cid, is_connector: true, text });
            return;
        }
        let pick = self.pick_shapes();
        if let Some(i) = shape_at(&pick, w.x, w.y) {
            let (id, text) = (pick[i].id().to_string(), pick[i].label().to_string());
            self.label_edit = Some(LabelEdit { id, is_connector: false, text });
            return;
        }
        let tol = (HANDLE_R + 8.0) / self.zoom;
        let hit = self.data().connectors.iter().filter(|c| self.selected_conns.iter().any(|id| id == c.id())).find_map(|c| {
            c.waypoints().iter().position(|p| (p.x - w.x).hypot(p.y - w.y) <= tol).map(|wi| (c.id().to_string(), wi))
        });
        if let Some((cid, wi)) = hit {
            self.mutate(|d| {
                if let Some(c) = d.connector_mut(&cid) {
                    let mut wps = c.waypoints();
                    wps.remove(wi);
                    c.set_waypoints(&wps);
                }
            });
            return;
        }
        let pick_c = self.pick_conns();
        if let Some((ci, _)) = connector_at(&pick_c, &self.data().shapes, w.x, w.y, 10.0 / self.zoom) {
            let (id, text) = (pick_c[ci].id().to_string(), pick_c[ci].label().to_string());
            self.selected_conns = vec![id.clone()];
            self.label_edit = Some(LabelEdit { id, is_connector: true, text });
        }
    }

    /// `handleContextMenu`: what was right-clicked (the selection follows, as on the web).
    pub fn context_menu(&mut self, x: f64, y: f64) -> ContextTarget {
        if self.label_edit.is_some() {
            self.commit_label();
        }
        let w = self.world(x, y);
        let pick = self.pick_shapes();
        if let Some(i) = shape_at(&pick, w.x, w.y) {
            let id = pick[i].id().to_string();
            if !self.selected.contains(&id) {
                self.selected = vec![id.clone()];
                self.selected_conns.clear();
            }
            return ContextTarget::Shape { id };
        }
        let pick_c = self.pick_conns();
        match connector_at(&pick_c, &self.data().shapes, w.x, w.y, 10.0 / self.zoom) {
            None => {
                self.clear_selection();
                ContextTarget::Canvas { world: w }
            }
            Some((ci, seg)) => {
                let id = pick_c[ci].id().to_string();
                self.selected_conns = vec![id.clone()];
                self.selected.clear();
                ContextTarget::Connector { id, seg, world: w }
            }
        }
    }

    // ── Label editing ────────────────────────────────────────────────────────

    /// Starts editing a label (« Modifier le texte » / « Modifier le label »).
    pub fn start_label_edit(&mut self, id: &str, is_connector: bool) {
        let text = if is_connector { self.data().connector(id).map(|c| c.label().to_string()) } else { self.data().shape(id).map(|s| s.label().to_string()) };
        if let Some(text) = text {
            self.label_edit = Some(LabelEdit { id: id.to_string(), is_connector, text });
        }
    }

    pub fn set_label_text(&mut self, text: &str) {
        if let Some(e) = self.label_edit.as_mut() {
            e.text = text.to_string();
        }
    }

    /// `commitLabel` (Enter, a click elsewhere, the editor losing the focus).
    pub fn commit_label(&mut self) {
        let Some(e) = self.label_edit.take() else { return };
        self.mutate(|d| {
            if e.is_connector {
                if let Some(c) = d.connector_mut(&e.id) {
                    c.set_label(&e.text);
                }
            } else if let Some(s) = d.shape_mut(&e.id) {
                s.set_label(&e.text);
            }
        });
    }

    /// Escape in the label editor.
    pub fn cancel_label(&mut self) {
        self.label_edit = None;
    }

    /// Where the label editor goes (canvas coordinates of its centre): the shape's centre or the
    /// connector label's.
    pub fn label_edit_anchor(&self) -> Option<Point> {
        let e = self.label_edit.as_ref()?;
        let c = if e.is_connector {
            connector_label_center(self.data().connector(&e.id)?, &self.data().shapes)
        } else {
            self.data().shape(&e.id)?.center()
        };
        Some(geometry::world_to_canvas(c.x, c.y, self.pan_x, self.pan_y, self.zoom))
    }

    // ── Keys ─────────────────────────────────────────────────────────────────

    /// Delete / Backspace (`deleteSelected`): the selection and the connectors attached to it.
    pub fn delete_selected(&mut self) {
        let (sel, conns) = (self.selected.clone(), self.selected_conns.clone());
        self.mutate(|d| {
            d.shapes.retain(|s| !sel.iter().any(|id| id == s.id()));
            d.connectors.retain(|c| {
                !conns.iter().any(|id| id == c.id()) && !c.source_id().is_some_and(|s| sel.iter().any(|id| id == s)) && !c.target_id().is_some_and(|t| sel.iter().any(|id| id == t))
            });
        });
        self.clear_selection();
    }

    /// Escape.
    pub fn escape(&mut self) {
        self.clear_selection();
        self.gesture = Gesture::None;
        self.armed = None;
        self.drawing = None;
    }

    /// Ctrl+A: every shape (`data.shapes`, hidden and locked layers included — QUIRK of the web's key
    /// handler; the canvas menu's « Tout sélectionner » takes only the pickable ones).
    pub fn select_all(&mut self) {
        self.selected = self.data().shapes.iter().map(|s| s.id().to_string()).collect();
    }

    /// « Tout sélectionner » of the canvas menu.
    pub fn select_all_pickable(&mut self) {
        self.selected = self.pick_shapes().iter().map(|s| s.id().to_string()).collect();
    }

    // ── Clipboard ────────────────────────────────────────────────────────────

    /// `copySelection`: the selected shapes and the connectors whose two ends are among them.
    pub fn copy(&mut self) {
        let shapes: Vec<Shape> = self.data().shapes.iter().filter(|s| self.selected.iter().any(|id| id == s.id())).cloned().collect();
        if shapes.is_empty() && self.selected_conns.is_empty() {
            return;
        }
        let ids: Vec<&str> = shapes.iter().map(Shape::id).collect();
        let conns = self
            .data()
            .connectors
            .iter()
            .filter(|c| matches!((c.source_id(), c.target_id()), (Some(s), Some(t)) if ids.contains(&s) && ids.contains(&t)))
            .cloned()
            .collect();
        self.clipboard = Some((shapes, conns));
    }

    /// The clipboard's content as JSON (`{ shapes, connectors }`) — for the system clipboard.
    pub fn clipboard_json(&self) -> Option<Value> {
        let (s, c) = self.clipboard.as_ref()?;
        Some(serde_json::json!({
            "shapes": s.iter().map(|x| Value::Object(x.0.clone())).collect::<Vec<_>>(),
            "connectors": c.iter().map(|x| Value::Object(x.0.clone())).collect::<Vec<_>>(),
        }))
    }

    /// Loads the clipboard from that JSON (another window's copy).
    pub fn set_clipboard_json(&mut self, v: &Value) {
        let d = PageData::from_value(v);
        if !d.shapes.is_empty() {
            self.clipboard = Some((d.shapes, d.connectors));
        }
    }

    /// `pasteClipboard(dx, dy, at)`: fresh ids, groups remapped, selected.
    pub fn paste(&mut self, at: Option<Point>) {
        let Some((shapes, conns)) = self.clipboard.clone() else { return };
        if shapes.is_empty() {
            return;
        }
        let (mut dx, mut dy) = (20.0, 20.0);
        if let Some(at) = at {
            let min_x = shapes.iter().map(Shape::x).fold(f64::INFINITY, f64::min);
            let min_y = shapes.iter().map(Shape::y).fold(f64::INFINITY, f64::min);
            dx = js_round(at.x - min_x);
            dy = js_round(at.y - min_y);
        }
        let mut id_map: Vec<(String, String)> = Vec::new();
        let mut group_map: Vec<(String, String)> = Vec::new();
        let base_z = self.data().shapes.len();
        let mut new_shapes = Vec::new();
        for (i, s) in shapes.iter().enumerate() {
            let nid = self.make_id();
            id_map.push((s.id().to_string(), nid.clone()));
            let gid = match s.group_id() {
                Some(g) => match group_map.iter().find(|(o, _)| o == g) {
                    Some((_, n)) => Some(n.clone()),
                    None => {
                        let n = self.make_id();
                        group_map.push((g.to_string(), n.clone()));
                        Some(n)
                    }
                },
                None => None,
            };
            let mut ns = s.clone();
            ns.set_id(&nid);
            ns.set_pos(s.x() + dx, s.y() + dy);
            ns.set_group_id(gid.as_deref());
            ns.set_num("zIndex", (base_z + i) as f64);
            new_shapes.push(ns);
        }
        let lookup = |id: Option<&str>| id.and_then(|id| id_map.iter().find(|(o, _)| o == id).map(|(_, n)| n.clone()));
        let mut new_conns = Vec::new();
        for c in &conns {
            let (Some(s), Some(t)) = (lookup(c.source_id()), lookup(c.target_id())) else { continue };
            let mut nc = c.clone();
            nc.set_id(&self.ids.make_id());
            nc.obj_mut().insert("sourceId".into(), s.into());
            nc.obj_mut().insert("targetId".into(), t.into());
            let wps: Vec<Point> = c.waypoints().iter().map(|p| Point::new(p.x + dx, p.y + dy)).collect();
            nc.set_waypoints(&wps);
            new_conns.push(nc);
        }
        let sel: Vec<String> = new_shapes.iter().map(|s| s.id().to_string()).collect();
        self.mutate(|d| {
            d.shapes.extend(new_shapes);
            d.connectors.extend(new_conns);
        });
        self.selected = sel;
        self.selected_conns.clear();
    }

    /// `cutSelection`.
    pub fn cut(&mut self) {
        self.copy();
        if !self.has_selection() {
            return;
        }
        self.delete_selected();
    }

    /// `duplicateSelection` (Ctrl+D).
    pub fn duplicate(&mut self) {
        self.copy();
        self.paste(None);
    }

    // ── Arrangement ──────────────────────────────────────────────────────────

    fn selected_shapes(&self) -> Vec<Shape> {
        self.data().shapes.iter().filter(|s| self.selected.iter().any(|id| id == s.id())).cloned().collect()
    }

    /// `align(axis)` (two shapes or more).
    pub fn align(&mut self, axis: Align) {
        if self.selected.len() < 2 {
            return;
        }
        let shapes = self.selected_shapes();
        let min_x = shapes.iter().map(Shape::x).fold(f64::INFINITY, f64::min);
        let max_r = shapes.iter().map(|s| s.x() + s.w()).fold(f64::NEG_INFINITY, f64::max);
        let min_y = shapes.iter().map(Shape::y).fold(f64::INFINITY, f64::min);
        let max_b = shapes.iter().map(|s| s.y() + s.h()).fold(f64::NEG_INFINITY, f64::max);
        let sel = self.selected.clone();
        self.mutate(|d| {
            for s in d.shapes.iter_mut().filter(|s| sel.iter().any(|id| id == s.id())) {
                match axis {
                    Align::Left => s.set_num("x", min_x),
                    Align::Center => s.set_num("x", js_round((min_x + max_r) / 2.0) - js_round(s.w() / 2.0)),
                    Align::Right => s.set_num("x", max_r - s.w()),
                    Align::Top => s.set_num("y", min_y),
                    Align::Middle => s.set_num("y", js_round((min_y + max_b) / 2.0) - js_round(s.h() / 2.0)),
                    Align::Bottom => s.set_num("y", max_b - s.h()),
                }
            }
        });
    }

    /// `distribute(axis)` (three shapes or more): equal gaps between centres.
    pub fn distribute(&mut self, horizontal: bool) {
        if self.selected.len() < 3 {
            return;
        }
        let mut sel = self.selected_shapes();
        let key = |s: &Shape| if horizontal { s.x() + s.w() / 2.0 } else { s.y() + s.h() / 2.0 };
        sel.sort_by(|a, b| key(a).total_cmp(&key(b)));
        let (c0, c1) = (key(&sel[0]), key(&sel[sel.len() - 1]));
        let step = (c1 - c0) / (sel.len() - 1) as f64;
        let target: Vec<(String, f64)> = sel.iter().enumerate().map(|(i, s)| (s.id().to_string(), c0 + step * i as f64)).collect();
        self.mutate(|d| {
            for s in d.shapes.iter_mut() {
                let Some((_, c)) = target.iter().find(|(id, _)| id == s.id()) else { continue };
                if horizontal {
                    s.set_num("x", js_round(c - s.w() / 2.0));
                } else {
                    s.set_num("y", js_round(c - s.h() / 2.0));
                }
            }
        });
    }

    /// `reorderSelection(mode)`.
    pub fn reorder(&mut self, mode: Order) {
        if self.selected.is_empty() {
            return;
        }
        let sel = self.selected.clone();
        self.mutate(|d| {
            let is_sel = |s: &Shape| sel.iter().any(|id| id == s.id());
            match mode {
                Order::Front | Order::Back => {
                    let (picked, rest): (Vec<Shape>, Vec<Shape>) = d.shapes.drain(..).partition(|s| is_sel(s));
                    d.shapes = if mode == Order::Front { rest.into_iter().chain(picked).collect() } else { picked.into_iter().chain(rest).collect() };
                }
                Order::Forward | Order::Backward => {
                    let indices: Vec<usize> = (0..d.shapes.len()).filter(|&i| is_sel(&d.shapes[i])).collect();
                    let order: Vec<usize> = if mode == Order::Forward { indices.into_iter().rev().collect() } else { indices };
                    for i in order {
                        let j = if mode == Order::Forward { i as isize + 1 } else { i as isize - 1 };
                        if j < 0 || j as usize >= d.shapes.len() || is_sel(&d.shapes[j as usize]) {
                            continue;
                        }
                        d.shapes.swap(i, j as usize);
                    }
                }
            }
        });
    }

    /// `groupSelection` (two shapes or more).
    pub fn group(&mut self) {
        if self.selected.len() < 2 {
            return;
        }
        let gid = self.make_id();
        let sel = self.selected.clone();
        self.mutate(|d| {
            for s in d.shapes.iter_mut().filter(|s| sel.iter().any(|id| id == s.id())) {
                s.set_group_id(Some(&gid));
            }
        });
    }

    /// `ungroupSelection`: `groupId: null`.
    pub fn ungroup(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        let sel = self.selected.clone();
        self.mutate(|d| {
            for s in d.shapes.iter_mut().filter(|s| sel.iter().any(|id| id == s.id())) {
                s.set_group_id(None);
            }
        });
    }

    /// `flipSelection`.
    pub fn flip(&mut self, horizontal: bool) {
        if self.selected.is_empty() {
            return;
        }
        let sel = self.selected.clone();
        self.mutate(|d| {
            for s in d.shapes.iter_mut().filter(|s| sel.iter().any(|id| id == s.id())) {
                if horizontal {
                    let v = !s.flip_h();
                    s.set_flip_h(v);
                } else {
                    let v = !s.flip_v();
                    s.set_flip_v(v);
                }
            }
        });
    }

    /// The « Forme » tab's rotation: `reset ? 0 : ((rotation + deg) % 360 + 360) % 360`.
    pub fn rotate(&mut self, deg: f64, reset: bool) {
        let sel = self.selected.clone();
        self.mutate(|d| {
            for s in d.shapes.iter_mut().filter(|s| sel.iter().any(|id| id == s.id())) {
                let r = if reset { 0.0 } else { js_mod360(s.rotation() + deg) };
                s.set_rotation(r);
            }
        });
    }

    /// Auto-layout (`applyLayout`): the selection when two shapes or more are selected, else the page.
    pub fn apply_layout(&mut self, kind: LayoutKind) {
        let ids = if self.selected.len() >= 2 { Some(self.selected.clone()) } else { None };
        let pos = compute_layout(kind, &self.data().shapes, &self.data().connectors, ids.as_deref());
        if pos.is_empty() {
            return;
        }
        self.mutate(|d| {
            for s in d.shapes.iter_mut() {
                if let Some((_, p)) = pos.iter().find(|(id, _)| id == s.id()) {
                    s.set_pos(p.x, p.y);
                }
            }
        });
    }

    // ── Styles ───────────────────────────────────────────────────────────────

    /// `applyTheme(fill, stroke, text, conn)`: the selection, or the whole page.
    pub fn apply_theme(&mut self, fill: &str, stroke: &str, text: &str, conn: &str) {
        let (sel, sel_c) = (self.selected.clone(), self.selected_conns.clone());
        self.mutate(|d| {
            for s in d.shapes.iter_mut() {
                if !sel.is_empty() && !sel.iter().any(|id| id == s.id()) {
                    continue;
                }
                s.patch_style(&obj(&[("fillColor", fill.into()), ("strokeColor", stroke.into())]));
                s.patch_label_style(&obj(&[("color", text.into())]));
            }
            for c in d.connectors.iter_mut() {
                let target = if !sel_c.is_empty() { sel_c.iter().any(|id| id == c.id()) } else { sel.is_empty() };
                if target {
                    c.patch_style(&obj(&[("strokeColor", conn.into())]));
                }
            }
        });
    }

    /// `applyStyleToSelection(patch)` (the style presets, the « Forme » tab's colours).
    pub fn apply_style_to_selection(&mut self, patch: &Obj) {
        if self.selected.is_empty() {
            return;
        }
        let sel = self.selected.clone();
        self.mutate(|d| {
            for s in d.shapes.iter_mut().filter(|s| sel.iter().any(|id| id == s.id())) {
                s.patch_style(patch);
            }
        });
    }

    /// `updateShapeStyle(sid, patch)`.
    pub fn update_shape_style(&mut self, sid: &str, patch: &Obj) {
        self.mutate(|d| {
            if let Some(s) = d.shape_mut(sid) {
                s.patch_style(patch);
            }
        });
    }

    /// `updateShapeLabelStyle(sid, patch)`.
    pub fn update_shape_label_style(&mut self, sid: &str, patch: &Obj) {
        self.mutate(|d| {
            if let Some(s) = d.shape_mut(sid) {
                s.patch_label_style(patch);
            }
        });
    }

    /// « Modifier le style… »: the style object replaced (the JSON was validated by the caller).
    pub fn set_shape_style_obj(&mut self, sid: &str, style: Obj) {
        self.mutate(|d| {
            if let Some(s) = d.shape_mut(sid) {
                s.set_style_obj(style);
            }
        });
    }

    /// `updateConnStyle(cid, patch)`.
    pub fn update_conn_style(&mut self, cid: &str, patch: &Obj) {
        self.mutate(|d| {
            if let Some(c) = d.connector_mut(cid) {
                c.patch_style(patch);
            }
        });
    }

    /// The Format panel's text box: the label written at each change.
    pub fn set_shape_label(&mut self, sid: &str, label: &str) {
        self.mutate(|d| {
            if let Some(s) = d.shape_mut(sid) {
                s.set_label(label);
            }
        });
    }

    pub fn set_conn_label(&mut self, cid: &str, label: &str) {
        self.mutate(|d| {
            if let Some(c) = d.connector_mut(cid) {
                c.set_label(label);
            }
        });
    }

    /// The Format panel's X, Y, W, H (`parseInt`, a non-number ignored).
    pub fn set_shape_geometry(&mut self, sid: &str, key: &str, v: f64) {
        if !matches!(key, "x" | "y" | "w" | "h") || !v.is_finite() {
            return;
        }
        let v = v.trunc();
        self.mutate(|d| {
            if let Some(s) = d.shape_mut(sid) {
                s.set_num(key, v);
            }
        });
    }

    /// The Format panel's rotation (`parseInt(v) || 0`, normalised to 0–359).
    pub fn set_shape_rotation(&mut self, sid: &str, v: f64) {
        let v = if v.is_finite() { v.trunc() } else { 0.0 };
        self.mutate(|d| {
            if let Some(s) = d.shape_mut(sid) {
                s.set_rotation(js_mod360(v));
            }
        });
    }

    // ── Connectors ───────────────────────────────────────────────────────────

    /// « Ajouter un nœud ici » (`connAddNode`).
    pub fn conn_add_node(&mut self, cid: &str, seg: usize, wx: f64, wy: f64) {
        let p = Point::new(self.snapv(wx), self.snapv(wy));
        self.mutate(|d| {
            if let Some(c) = d.connector_mut(cid) {
                let mut wps = c.waypoints();
                wps.insert(seg.min(wps.len()), p);
                c.set_waypoints(&wps);
            }
        });
    }

    /// « Effacer les nœuds ».
    pub fn conn_clear_nodes(&mut self, cid: &str) {
        self.mutate(|d| {
            if let Some(c) = d.connector_mut(cid) {
                c.set_waypoints(&[]);
            }
        });
    }

    /// « Inverser le sens ».
    pub fn conn_reverse(&mut self, cid: &str) {
        self.mutate(|d| {
            if let Some(c) = d.connector_mut(cid) {
                c.reverse();
            }
        });
    }

    /// « Dupliquer » a connector: waypoints shifted by 16, the copy selected.
    pub fn conn_duplicate(&mut self, cid: &str) {
        let Some(src) = self.data().connector(cid).cloned() else { return };
        let mut copy = src.clone();
        let nid = self.make_id();
        copy.set_id(&nid);
        let wps: Vec<Point> = src.waypoints().iter().map(|p| Point::new(p.x + 16.0, p.y + 16.0)).collect();
        copy.set_waypoints(&wps);
        self.mutate(|d| d.connectors.push(copy));
        self.selected_conns = vec![nid];
        self.selected.clear();
    }

    /// « Mettre au premier plan / à l'arrière-plan » of a connector (`connReorder`).
    pub fn conn_reorder(&mut self, cid: &str, to_front: bool) {
        self.mutate(|d| {
            let Some(i) = d.connectors.iter().position(|c| c.id() == cid) else { return };
            let me = d.connectors.remove(i);
            if to_front {
                d.connectors.push(me);
            } else {
                d.connectors.insert(0, me);
            }
        });
    }

    /// « Supprimer » a connector.
    pub fn delete_connector(&mut self, cid: &str) {
        self.mutate(|d| d.connectors.retain(|c| c.id() != cid));
        self.selected_conns.clear();
    }

    // ── Layers ───────────────────────────────────────────────────────────────

    /// « Nouveau calque »: `Calque N`, made active.
    pub fn add_layer(&mut self) {
        let id = self.make_id();
        let id2 = id.clone();
        self.mutate(|d| {
            let mut ls = d.layers();
            let name = format!("Calque {}", ls.len() + 1);
            ls.push(Layer::new(&id2, &name));
            d.layers = Some(ls);
        });
        self.active_layer = id;
    }

    /// Deletes a layer and what is on it (never the last one).
    pub fn delete_layer(&mut self, lid: &str) {
        if self.layers().len() <= 1 {
            return;
        }
        self.mutate(|d| {
            let all = d.layers();
            let first = all.first().map(|l| l.id().to_string()).unwrap_or_default();
            let ls: Vec<Layer> = all.into_iter().filter(|l| l.id() != lid).collect();
            d.shapes.retain(|s| s.layer_id().unwrap_or(&first) != lid);
            d.connectors.retain(|c| c.layer_id().unwrap_or(&first) != lid);
            d.layers = Some(ls);
        });
        self.fix_active_layer();
    }

    fn update_layer(&mut self, lid: &str, f: impl Fn(&mut Layer)) {
        self.mutate(|d| {
            let mut ls = d.layers();
            for l in ls.iter_mut().filter(|l| l.id() == lid) {
                f(l);
            }
            d.layers = Some(ls);
        });
    }

    pub fn rename_layer(&mut self, lid: &str, name: &str) {
        let name = name.to_string();
        self.update_layer(lid, |l| l.set_name(&name));
    }

    pub fn toggle_layer_visible(&mut self, lid: &str) {
        self.update_layer(lid, |l| {
            let v = !l.visible();
            l.set_visible(v)
        });
    }

    pub fn toggle_layer_locked(&mut self, lid: &str) {
        self.update_layer(lid, |l| {
            let v = !l.locked();
            l.set_locked(v)
        });
    }

    /// `moveLayer(lid, up)`: up = towards the top of the stack (drawn later).
    pub fn move_layer(&mut self, lid: &str, up: bool) {
        self.mutate(|d| {
            let mut ls = d.layers();
            let Some(i) = ls.iter().position(|l| l.id() == lid) else { return };
            let j = if up { i as isize + 1 } else { i as isize - 1 };
            if j < 0 || j as usize >= ls.len() {
                return;
            }
            ls.swap(i, j as usize);
            d.layers = Some(ls);
        });
    }

    /// « Déplacer la sélection ici ».
    pub fn move_selection_to_layer(&mut self, lid: &str) {
        if !self.has_selection() {
            return;
        }
        let (sel, sel_c) = (self.selected.clone(), self.selected_conns.clone());
        self.mutate(|d| {
            for s in d.shapes.iter_mut().filter(|s| sel.iter().any(|id| id == s.id())) {
                s.set_layer_id(lid);
            }
            for c in d.connectors.iter_mut().filter(|c| sel_c.iter().any(|id| id == c.id())) {
                c.set_layer_id(lid);
            }
        });
    }

    // ── Placing shapes ───────────────────────────────────────────────────────

    /// Arms a shape tool from the gallery (`armShape`): a second pick disarms; the selection is cleared.
    pub fn arm_shape(&mut self, kind: &str) {
        if stencils::stencil(kind).is_none() {
            return;
        }
        self.armed = if self.armed.as_deref() == Some(kind) { None } else { Some(kind.to_string()) };
        self.drawing = None;
        self.clear_selection();
    }

    /// The stencil panel's click: toggles the armed stencil (the selection stays).
    pub fn toggle_armed(&mut self, kind: &str) {
        self.armed = if self.armed.as_deref() == Some(kind) { None } else { Some(kind.to_string()) };
    }

    /// `placeStencilBox`: a new shape on the active layer, selected.
    pub fn place_stencil_box(&mut self, st: &stencils::StencilDef, x: f64, y: f64, w: f64, h: f64) {
        let id = self.make_id();
        let label = if st.id.starts_with("hw_") { String::new() } else { (self.namer)(st) };
        let shape = Shape::new(&id, st.id, x, y, w.max(1.0), h.max(1.0), &label, st.style.clone(), self.data().shapes.len() as f64, &self.active_layer);
        self.mutate(|d| d.shapes.push(shape));
        self.selected = vec![id];
    }

    /// `placeStencil`: the default size centred on a world point (a drop from the panel).
    pub fn place_stencil(&mut self, kind: &str, wx: f64, wy: f64) {
        let Some(st) = stencils::stencil(kind) else { return };
        let (x, y) = (self.snapv(wx - st.default_w / 2.0), self.snapv(wy - st.default_h / 2.0));
        self.place_stencil_box(st, x, y, st.default_w, st.default_h);
    }

    /// `importIoData`: shapes and connectors merged into the page with fresh ids on the active layer
    /// (templates, draw.io and CSV imports), the new shapes selected.
    pub fn import(&mut self, shapes: &[Shape], connectors: &[Connector]) {
        let mut id_map: Vec<(String, String)> = Vec::new();
        let base_z = self.data().shapes.len();
        let mut new_shapes = Vec::new();
        for (i, s) in shapes.iter().enumerate() {
            let nid = self.make_id();
            id_map.push((s.id().to_string(), nid.clone()));
            let mut ns = s.clone();
            ns.set_id(&nid);
            ns.obj_mut().insert("labelStyle".into(), Value::Object(Obj::new()));
            ns.set_num("zIndex", (base_z + i) as f64);
            ns.set_layer_id(&self.active_layer);
            new_shapes.push(ns);
        }
        let lookup = |id: Option<&str>| -> Value { id.and_then(|id| id_map.iter().find(|(o, _)| o == id).map(|(_, n)| Value::from(n.as_str()))).unwrap_or(Value::Null) };
        let mut new_conns = Vec::new();
        for c in connectors {
            let mut nc = c.clone();
            nc.set_id(&self.ids.make_id());
            nc.obj_mut().insert("sourceId".into(), lookup(c.source_id()));
            nc.obj_mut().insert("targetId".into(), lookup(c.target_id()));
            nc.set_layer_id(&self.active_layer);
            new_conns.push(nc);
        }
        let sel: Vec<String> = new_shapes.iter().map(|s| s.id().to_string()).collect();
        self.mutate(|d| {
            d.shapes.extend(new_shapes);
            d.connectors.extend(new_conns);
        });
        self.selected = sel;
    }

    // ── Status ───────────────────────────────────────────────────────────────

    /// What the status bar shows: shapes, connectors, how many are selected.
    pub fn counts(&self) -> (usize, usize, usize) {
        (self.data().shapes.len(), self.data().connectors.len(), self.selected.len() + self.selected_conns.len())
    }
}

fn union(a: &[String], b: &[String]) -> Vec<String> {
    let mut v = a.to_vec();
    for x in b {
        if !v.contains(x) {
            v.push(x.clone());
        }
    }
    v
}

/// `+(v).toFixed(2)`.
fn to_fixed2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// `((v % 360) + 360) % 360` with JavaScript's `%`.
fn js_mod360(v: f64) -> f64 {
    ((v % 360.0) + 360.0) % 360.0
}

/// A small object literal.
pub fn obj(pairs: &[(&str, Value)]) -> Obj {
    pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}

/// A number field of an object literal, written as the web writes numbers.
pub fn num_value(v: f64) -> Value {
    js_number(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::Recorder;

    fn editor_with(shapes: Vec<Shape>, conns: Vec<Connector>) -> Editor {
        let data = PageData { shapes, connectors: conns, layers: None, rest: Obj::new() };
        let mut e = Editor::new(vec![Page::new("p1", "Page 1", data)], 7);
        e.viewport = (1000.0, 800.0);
        e
    }

    fn rect(id: &str, x: f64, y: f64) -> Shape {
        Shape::new(id, "rect", x, y, 120.0, 60.0, id, Obj::new(), 0.0, "default")
    }

    /// Canvas point of a world point at the default view (zoom 1, pan 60/60).
    fn c(x: f64, y: f64) -> (f64, f64) {
        (x + 60.0, y + 60.0)
    }

    #[test]
    fn a_click_selects_and_a_drag_moves_in_one_undo_step() {
        let mut e = editor_with(vec![rect("a", 0.0, 0.0), rect("b", 300.0, 200.0)], vec![]);
        let mut m = Recorder::default();
        let (x, y) = c(50.0, 30.0);
        e.pointer_down(x, y, Button::Left, Mods::default(), &mut m);
        assert_eq!(e.selected(), ["a"]);
        for k in 1..=10 {
            e.pointer_move(x + k as f64 * 10.0, y + k as f64 * 3.3, Mods::default());
        }
        e.pointer_up(x + 100.0, y + 33.0);
        let s = e.data().shape("a").expect("a");
        assert_eq!((s.x(), s.y()), (100.0, 33.0));
        e.undo();
        let s = e.data().shape("a").expect("a");
        assert_eq!((s.x(), s.y()), (0.0, 0.0), "one gesture = one undo step");
        e.redo();
        assert_eq!(e.data().shape("a").map(Shape::x), Some(100.0));
    }

    #[test]
    fn dragging_from_a_port_to_another_shape_connects_them() {
        let mut e = editor_with(vec![rect("a", 0.0, 0.0), rect("b", 300.0, 0.0)], vec![]);
        let mut m = Recorder::default();
        // a's right port is (120, 30).
        let (x, y) = c(120.0, 30.0);
        e.pointer_down(x, y, Button::Left, Mods::default(), &mut m);
        e.pointer_move(c(350.0, 30.0).0, c(350.0, 30.0).1, Mods::default());
        e.pointer_up(c(350.0, 30.0).0, c(350.0, 30.0).1);
        assert_eq!(e.data().connectors.len(), 1);
        let conn = &e.data().connectors[0];
        assert_eq!((conn.source_id(), conn.target_id()), (Some("a"), Some("b")));
        // Released on the source itself: nothing.
        let (x, y) = c(120.0, 30.0);
        e.pointer_down(x, y, Button::Left, Mods::default(), &mut m);
        e.pointer_up(c(60.0, 30.0).0, c(60.0, 30.0).1);
        assert_eq!(e.data().connectors.len(), 1);
    }

    #[test]
    fn resize_keeps_twenty_pixels() {
        let mut e = editor_with(vec![rect("a", 0.0, 0.0)], vec![]);
        e.set_selection(vec!["a".into()], vec![]);
        let mut m = Recorder::default();
        // The se handle sits 14 px outside the box.
        let (x, y) = c(134.0, 74.0);
        e.pointer_down(x, y, Button::Left, Mods::default(), &mut m);
        e.pointer_move(x - 500.0, y - 500.0, Mods::default());
        e.pointer_up(x - 500.0, y - 500.0);
        let s = e.data().shape("a").expect("a");
        assert_eq!((s.w(), s.h()), (20.0, 20.0));
    }

    #[test]
    fn lasso_selects_enclosed_shapes() {
        let mut e = editor_with(vec![rect("a", 0.0, 0.0), rect("b", 300.0, 0.0)], vec![]);
        let mut m = Recorder::default();
        e.pointer_down(c(-10.0, -10.0).0, c(-10.0, -10.0).1, Button::Left, Mods::default(), &mut m);
        e.pointer_move(c(200.0, 100.0).0, c(200.0, 100.0).1, Mods::default());
        e.pointer_up(c(200.0, 100.0).0, c(200.0, 100.0).1);
        assert_eq!(e.selected(), ["a"]);
    }

    #[test]
    fn copy_paste_remaps_ids_groups_and_connectors() {
        let mut a = rect("a", 0.0, 0.0);
        let mut b = rect("b", 300.0, 0.0);
        a.set_group_id(Some("g"));
        b.set_group_id(Some("g"));
        let mut e = editor_with(vec![a, b], vec![Connector::between("c1", "a", "b", "default")]);
        e.set_selection(vec!["a".into(), "b".into()], vec![]);
        e.copy();
        e.paste(None);
        let d = e.data();
        assert_eq!(d.shapes.len(), 4);
        assert_eq!(d.connectors.len(), 2);
        let (n1, n2) = (&d.shapes[2], &d.shapes[3]);
        assert_eq!((n1.x(), n1.y()), (20.0, 20.0));
        assert_eq!(n1.group_id(), n2.group_id());
        assert_ne!(n1.group_id(), Some("g"));
        let nc = &d.connectors[1];
        assert_eq!((nc.source_id(), nc.target_id()), (Some(n1.id()), Some(n2.id())));
        assert_eq!(e.selected().len(), 2);
    }

    #[test]
    fn deleting_keeps_the_layers_unlike_the_web() {
        let mut e = editor_with(vec![rect("a", 0.0, 0.0)], vec![]);
        e.add_layer();
        e.set_selection(vec!["a".into()], vec![]);
        e.delete_selected();
        assert_eq!(e.data().layers.as_ref().map(Vec::len), Some(2));
    }

    #[test]
    fn align_distribute_order_group_flip_rotate() {
        let mut e = editor_with(vec![rect("a", 0.0, 0.0), rect("b", 100.0, 50.0), rect("c", 500.0, 10.0)], vec![]);
        e.set_selection(vec!["a".into(), "b".into(), "c".into()], vec![]);
        e.align(Align::Top);
        assert!(e.data().shapes.iter().all(|s| s.y() == 0.0));
        e.distribute(true);
        assert_eq!(e.data().shape("b").map(Shape::x), Some(250.0));
        e.set_selection(vec!["a".into()], vec![]);
        e.reorder(Order::Front);
        assert_eq!(e.data().shapes.last().map(Shape::id), Some("a"));
        e.reorder(Order::Backward);
        assert_eq!(e.data().shapes[1].id(), "a");
        e.set_selection(vec!["a".into(), "b".into()], vec![]);
        e.group();
        assert!(e.selection_has_group());
        e.ungroup();
        assert_eq!(e.data().shape("a").map(|s| s.obj().get("groupId").cloned()), Some(Some(Value::Null)));
        e.flip(true);
        assert!(e.data().shape("a").is_some_and(Shape::flip_h));
        e.rotate(-90.0, false);
        assert_eq!(e.data().shape("a").map(Shape::rotation), Some(270.0));
    }

    #[test]
    fn arming_then_clicking_places_the_default_size_centred() {
        let mut e = editor_with(vec![], vec![]);
        e.arm_shape("rect");
        let mut m = Recorder::default();
        e.pointer_down(c(200.0, 100.0).0, c(200.0, 100.0).1, Button::Left, Mods::default(), &mut m);
        e.pointer_up(c(200.0, 100.0).0, c(200.0, 100.0).1);
        let s = &e.data().shapes[0];
        assert_eq!((s.x(), s.y(), s.w(), s.h()), (140.0, 70.0, 120.0, 60.0));
        assert_eq!(s.label(), "Rectangle");
        assert!(e.armed().is_none(), "one arming, one shape");
    }

    #[test]
    fn hidden_and_locked_layers_are_not_pickable() {
        let mut e = editor_with(vec![rect("a", 0.0, 0.0)], vec![]);
        e.toggle_layer_locked("default");
        let mut m = Recorder::default();
        e.pointer_down(c(50.0, 30.0).0, c(50.0, 30.0).1, Button::Left, Mods::default(), &mut m);
        e.pointer_up(c(50.0, 30.0).0, c(50.0, 30.0).1);
        assert!(e.selected().is_empty());
    }

    #[test]
    fn the_wheel_zooms_about_the_pointer() {
        let mut e = editor_with(vec![], vec![]);
        let before = e.world(300.0, 200.0);
        e.wheel(300.0, 200.0, -1.0);
        assert!((e.zoom - 1.1).abs() < 1e-12);
        let after = e.world(300.0, 200.0);
        assert!((before.x - after.x).abs() < 1e-9 && (before.y - after.y).abs() < 1e-9);
        e.zoom_in();
        assert_eq!(e.zoom, 1.32);
    }

    #[test]
    fn a_waypoint_drag_snaps_to_a_right_angle() {
        let a = rect("a", 0.0, 0.0);
        let b = rect("b", 400.0, 300.0);
        let mut conn = Connector::between("c", "a", "b", "default");
        conn.set_waypoints(&[Point::new(200.0, 50.0)]);
        let mut e = editor_with(vec![a, b], vec![conn]);
        e.set_selection(vec![], vec!["c".into()]);
        let mut m = Recorder::default();
        e.pointer_down(c(200.0, 50.0).0, c(200.0, 50.0).1, Button::Left, Mods::default(), &mut m);
        e.pointer_move(c(455.0, 35.0).0, c(455.0, 35.0).1, Mods::default());
        e.pointer_up(c(455.0, 35.0).0, c(455.0, 35.0).1);
        let w = e.data().connector("c").map(Connector::waypoints).expect("c");
        // Previous point a's right port (120, 30), next b's top port (460, 300): elbow (460, 30).
        assert_eq!(w, vec![Point::new(460.0, 30.0)]);
    }

    #[test]
    fn the_label_editor_commits_with_history() {
        let mut e = editor_with(vec![rect("a", 0.0, 0.0)], vec![]);
        let mut m = Recorder::default();
        e.double_click(c(50.0, 30.0).0, c(50.0, 30.0).1, &mut m);
        assert_eq!(e.label_edit().map(|l| l.text.as_str()), Some("a"));
        e.set_label_text("Hello");
        e.commit_label();
        assert_eq!(e.data().shape("a").map(Shape::label), Some("Hello"));
        e.undo();
        assert_eq!(e.data().shape("a").map(Shape::label), Some("a"));
    }

    #[test]
    fn history_is_kept_per_page() {
        let mut e = editor_with(vec![rect("a", 0.0, 0.0)], vec![]);
        e.add_page(Page::new("p2", "Page 2", PageData::default()));
        e.set_current_page(0);
        e.set_selection(vec!["a".into()], vec![]);
        e.delete_selected();
        e.set_current_page(1);
        assert!(!e.can_undo());
        e.undo();
        assert!(e.data().shapes.is_empty());
        e.set_current_page(0);
        e.undo();
        assert_eq!(e.data().shapes.len(), 1);
    }
}
