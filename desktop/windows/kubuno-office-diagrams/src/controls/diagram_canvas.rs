//! `DiagramCanvas` - the diagram page being edited: the web editor's `<canvas>` with its minimap and its
//! rulers (`DiagramEditorPage.tsx:3640-3838`).
//!
//! The page, the selection, the history and every gesture are the platform-neutral core's
//! (`kubuno_office_diagrams_core::editor::Editor`, a port of the web editor's state and handlers); the scene
//! is drawn by the core's port of `renderCanvas` on the shared Canvas 2D context, over the Direct2D surface
//! of `crate::platform::surface`. This control is the Windows half: it hands the core the pointer, the
//! wheel and the keys in canvas coordinates (DIPs, the web's CSS pixels), and reports what the ribbon,
//! the panels and the status bar show (`StateChanged`) and what was right-clicked (`ContextMenuRequested`).

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::{Canvas, Rect, Size};
use kubuno_desktop::views::component::{Component, Control, ControlCore, EventCx, Keys, PaintEventCx};
use kubuno_desktop::views::events::Event;
use kubuno_office_diagrams_core::canvas::{Ctx2D, Matrix, Recorder};
use kubuno_office_diagrams_core::editor::{Button, ContextTarget, Cursor, Editor, Mods};
use kubuno_office_diagrams_core::file::Page;
use kubuno_office_diagrams_core::render;

use crate::platform::surface::{D2DSurface, SurfaceCache};

/// The minimap's distance from the canvas' bottom-right corner (`bottom-3 right-3`).
const MINIMAP_MARGIN: f32 = 12.0;
/// The inline label editor (`w-[120px]`, `text-sm`, `px-2 py-0.5`, a 1 px border).
const LABEL_W: f64 = 120.0;
const LABEL_H: f64 = 26.0;
const LABEL_PAD: f64 = 8.0;
const LABEL_FONT: &str = "14px \"Plus Jakarta Sans\", sans-serif";

/// What the ribbon, the panels and the status bar show of the editor (raised as `StateChanged` when it
/// differs).
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct DiagramStateEventArgs {
    pub shapes: usize,
    pub connectors: usize,
    /// Selected shapes and connectors.
    pub selected: usize,
    pub selected_conns: usize,
    /// The selected shape when exactly one is (its id), else empty.
    pub shape_id: String,
    /// The selected connector when exactly one is, else empty.
    pub conn_id: String,
    /// The single selected shape's stencil and label (the status bar's « label (type) »).
    pub shape_kind: String,
    pub shape_label: String,
    pub has_group: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub has_clipboard: bool,
    pub dirty: bool,
    /// The editor's change counter (a save compares it).
    pub revision: u64,
    pub zoom: f32,
    /// The armed stencil, empty for none.
    pub armed: String,
    pub show_grid: bool,
    pub snap: bool,
    pub show_minimap: bool,
    pub show_rulers: bool,
    pub page: usize,
    pub pages: usize,
    /// The layers and the active one (for the layers panel).
    pub layers_revision: u64,
    /// Bumped each time a diagram is opened into the canvas.
    pub opened: u64,
    /// The inline label editor: open, where (window DIPs: the centre of the edited thing) and its text.
    pub label_edit: bool,
    pub label_x: f32,
    pub label_y: f32,
    pub label_text: String,
}

/// A right click on the canvas: what is under the pointer and where (window DIPs).
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct DiagramContextEventArgs {
    /// `shape`, `connector` or `canvas`.
    pub kind: String,
    pub id: String,
    pub seg: usize,
    pub world_x: f64,
    pub world_y: f64,
    pub x: f32,
    pub y: f32,
}

/// The diagram canvas (see the module doc).
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Diagrams")]
#[toolbox(icon = "network")]
#[default_event("StateChanged")]
pub struct DiagramCanvas {
    base: ControlCore,
    /// Occurs when what the ribbon, the panels and the status bar show changes.
    #[event]
    #[category("Action")]
    pub state_changed: Event<DiagramStateEventArgs>,
    /// Occurs on a right click: the window opens the shape, connector or canvas menu.
    #[event]
    #[category("Action")]
    pub context_menu_requested: Event<DiagramContextEventArgs>,
    editor: Editor,
    /// How many diagrams were opened into this canvas (a new one reports itself even when its page count and
    /// revision equal the previous one's).
    opened: u64,
    cache: SurfaceCache,
    bounds: Rect,
    reported: Option<DiagramStateEventArgs>,
    cursor: Cursor,
    /// The minimap being dragged.
    minimap_drag: bool,
    /// The DirectWrite factory of the last frame (the hit tests that measure text use it between frames).
    dwrite: Option<windows::Win32::Graphics::DirectWrite::IDWriteFactory>,
    /// The inline label editor's caret (a char index into its text), and whether it was open last time.
    label_caret: usize,
    label_open: bool,
}

impl Default for DiagramCanvas {
    fn default() -> Self {
        let sample = kubuno_office_diagrams_common::doc::OpenDiagram::sample();
        Self {
            base: ControlCore::default(),
            state_changed: Event::default(),
            context_menu_requested: Event::default(),
            editor: Editor::new(sample.pages, 1),
            opened: 0,
            cache: SurfaceCache::default(),
            bounds: Rect::new(0.0, 0.0, 800.0, 600.0),
            reported: None,
            cursor: Cursor::Default,
            minimap_drag: false,
            dwrite: None,
            label_caret: 0,
            label_open: false,
        }
    }
}

/// A monotonic clock in milliseconds (ids are seeded with it).
fn now() -> u64 {
    kubuno_desktop::controls::host::now_ms()
}

impl DiagramCanvas {
    /// Opens `pages` (the editor starts over: selection, history, view).
    pub fn open(&mut self, pages: Vec<Page>) {
        let seed = now() ^ (std::process::id() as u64) << 32;
        let mut e = Editor::new(pages, seed);
        e.viewport = self.editor.viewport;
        e.show_grid = self.editor.show_grid;
        e.snap = self.editor.snap;
        e.show_minimap = self.editor.show_minimap;
        e.show_rulers = self.editor.show_rulers;
        e.read_only = self.editor.read_only;
        self.editor = e;
        self.opened += 1;
        self.changed();
    }

    pub fn editor(&self) -> &Editor {
        &self.editor
    }

    /// Runs `f` on the editor, then repaints and reports.
    pub fn edit<R>(&mut self, f: impl FnOnce(&mut Editor) -> R) -> R {
        let r = f(&mut self.editor);
        self.changed();
        r
    }

    fn changed(&mut self) {
        self.invalidate();
        kubuno_desktop::controls::host::request_repaint_after(0);
    }

    /// The canvas size (DIPs).
    fn size(&self) -> (f64, f64) {
        (((self.bounds.right - self.bounds.left).max(1.0)) as f64, ((self.bounds.bottom - self.bounds.top).max(1.0)) as f64)
    }

    /// The minimap's box (canvas coordinates), when it shows.
    fn minimap_box(&self) -> Option<(f32, f32, f32, f32)> {
        if !self.editor.show_minimap {
            return None;
        }
        let (w, h) = self.size();
        let (mw, mh) = (render::MINIMAP_W as f32 + 2.0, render::MINIMAP_H as f32 + 2.0);
        Some((w as f32 - MINIMAP_MARGIN - mw, h as f32 - MINIMAP_MARGIN - mh, mw, mh))
    }

    fn minimap_jump(&mut self, x: f32, y: f32) {
        let Some((mx, my, _, _)) = self.minimap_box() else { return };
        let (w, h) = self.size();
        let (shapes, _) = self.editor.visible_data();
        let t = render::minimap_transform(&self.editor.data().shapes, self.editor.zoom, self.editor.pan_x, self.editor.pan_y, w, h);
        let _ = shapes;
        let p = t.to_world((x - mx - 1.0) as f64, (y - my - 1.0) as f64);
        self.editor.minimap_jump(p.x, p.y);
    }

    fn state_args(&self) -> DiagramStateEventArgs {
        let e = &self.editor;
        let (shapes, connectors, _) = e.counts();
        let single = e.selected_shape();
        let label = e.label_edit();
        let anchor = e.label_edit_anchor();
        DiagramStateEventArgs {
            shapes,
            connectors,
            selected: e.selected().len(),
            selected_conns: e.selected_conns().len(),
            shape_id: single.map(|s| s.id().to_string()).unwrap_or_default(),
            conn_id: e.selected_conn().map(|c| c.id().to_string()).unwrap_or_default(),
            shape_kind: single.map(|s| s.kind().to_string()).unwrap_or_default(),
            shape_label: single.map(|s| s.label().to_string()).unwrap_or_default(),
            has_group: e.selection_has_group(),
            can_undo: e.can_undo(),
            can_redo: e.can_redo(),
            has_clipboard: e.has_clipboard(),
            dirty: e.is_dirty(),
            revision: e.revision(),
            zoom: e.zoom as f32,
            armed: e.armed().unwrap_or("").to_string(),
            show_grid: e.show_grid,
            snap: e.snap,
            show_minimap: e.show_minimap,
            show_rulers: e.show_rulers,
            page: e.current_index(),
            pages: e.pages().len(),
            layers_revision: e.revision(),
            opened: self.opened,
            label_edit: label.is_some(),
            label_x: anchor.map(|p| self.bounds.left + p.x as f32).unwrap_or(0.0),
            label_y: anchor.map(|p| self.bounds.top + p.y as f32).unwrap_or(0.0),
            label_text: label.map(|l| l.text.clone()).unwrap_or_default(),
        }
    }

    /// Paints the canvas, the minimap and the rulers through the shared Canvas 2D context.
    fn paint_scene(&mut self, c: &dyn kubuno_desktop::controls::ControlCanvas) {
        let Some(renderer) = c.renderer() else { return };
        self.dwrite = Some(renderer.dwrite.clone());
        self.sync_label();
        let Ok(mut surface) = D2DSurface::new(&renderer.d2d_context, &renderer.d2d_factory, &renderer.dwrite, &self.cache) else { return };
        let (w, h) = self.size();
        self.editor.viewport = (w, h);
        c.push_clip(&self.bounds);
        {
            let mut ctx = Ctx2D::new(&mut surface);
            let origin = Matrix::translation(self.bounds.left as f64, self.bounds.top as f64);
            ctx.set_transform(origin);
            self.editor.render(&mut ctx);
            if let Some((mx, my, mw, mh)) = self.minimap_box() {
                // `rounded-md border border-border bg-white/95 shadow-md`.
                ctx.save();
                ctx.set_shadow_color("rgba(0,0,0,0.10)");
                ctx.set_shadow_blur(6.0);
                ctx.set_shadow_offset(0.0, 2.0);
                ctx.set_fill_style("rgba(255,255,255,0.95)");
                ctx.begin_path();
                ctx.round_rect(mx as f64, my as f64, mw as f64, mh as f64, 6.0);
                ctx.fill();
                ctx.restore();
                ctx.save();
                ctx.translate(mx as f64 + 1.0, my as f64 + 1.0);
                render::render_minimap(&mut ctx, &self.editor.data().shapes, self.editor.zoom, self.editor.pan_x, self.editor.pan_y, w, h);
                ctx.restore();
                ctx.set_stroke_style("#e5e7eb");
                ctx.set_line_width(1.0);
                ctx.begin_path();
                ctx.round_rect(mx as f64 + 0.5, my as f64 + 0.5, mw as f64 - 1.0, mh as f64 - 1.0, 6.0);
                ctx.stroke();
            }
            if self.editor.show_rulers {
                let t = render::RULER_THICK;
                ctx.save();
                ctx.translate(t, 0.0);
                render::render_ruler(&mut ctx, true, self.editor.pan_x - t, self.editor.zoom, (w - t).max(0.0));
                ctx.restore();
                ctx.save();
                ctx.translate(0.0, t);
                render::render_ruler(&mut ctx, false, self.editor.pan_y - t, self.editor.zoom, (h - t).max(0.0));
                ctx.restore();
                // The corner: `bg-surface-1 border-r border-b border-border`.
                ctx.set_fill_style("#f8f9fa");
                ctx.fill_rect(0.0, 0.0, t, t);
                ctx.set_stroke_style("#e5e7eb");
                ctx.begin_path();
                ctx.move_to(t - 0.5, 0.0);
                ctx.line_to(t - 0.5, t - 0.5);
                ctx.line_to(0.0, t - 0.5);
                ctx.stroke();
            }
            if let Some((bx, by, text, caret_x, scroll)) = self.label_box(&mut ctx) {
                // `absolute bg-white border border-primary rounded px-2 py-0.5 text-sm shadow-md`, 120 wide,
                // at (x − 60, y − 14).
                ctx.save();
                ctx.set_shadow_color("rgba(0,0,0,0.10)");
                ctx.set_shadow_blur(6.0);
                ctx.set_shadow_offset(0.0, 4.0);
                ctx.set_fill_style("#ffffff");
                ctx.begin_path();
                ctx.round_rect(bx, by, LABEL_W, LABEL_H, 4.0);
                ctx.fill();
                ctx.restore();
                ctx.set_stroke_style(render::SELECT_BLUE);
                ctx.set_line_width(1.0);
                ctx.begin_path();
                ctx.round_rect(bx + 0.5, by + 0.5, LABEL_W - 1.0, LABEL_H - 1.0, 4.0);
                ctx.stroke();
                let inner = kubuno_desktop::ui::Rect::new(
                    self.bounds.left + (bx + LABEL_PAD) as f32,
                    self.bounds.top + by as f32,
                    self.bounds.left + (bx + LABEL_W - LABEL_PAD) as f32,
                    self.bounds.top + (by + LABEL_H) as f32,
                );
                c.push_clip(&inner);
                ctx.set_font(LABEL_FONT);
                ctx.set_fill_style("#202124");
                ctx.set_text_baseline(kubuno_office_diagrams_core::canvas::TextBaseline::Middle);
                ctx.fill_text(&text, bx + LABEL_PAD - scroll, by + LABEL_H / 2.0);
                ctx.set_fill_style("#202124");
                ctx.fill_rect(bx + LABEL_PAD - scroll + caret_x, by + 6.0, 1.0, LABEL_H - 12.0);
                c.pop_clip();
            }
        }
        c.pop_clip();
    }

    /// The inline label editor's box (canvas coordinates), its text, the caret's x in the text and the
    /// horizontal scroll that keeps the caret in view; `None` when no label is being edited.
    fn label_box(&self, ctx: &mut Ctx2D) -> Option<(f64, f64, String, f64, f64)> {
        let text = self.editor.label_edit()?.text.clone();
        let anchor = self.editor.label_edit_anchor()?;
        let caret = self.label_caret.min(text.chars().count());
        ctx.set_font(LABEL_FONT);
        let before: String = text.chars().take(caret).collect();
        let caret_x = ctx.measure_text(&before).width;
        let room = LABEL_W - 2.0 * LABEL_PAD;
        let scroll = (caret_x - room + 1.0).max(0.0);
        Some((anchor.x - 60.0, anchor.y - 14.0, text, caret_x, scroll))
    }

    /// Keeps the caret at the end when an edit opens (the web's input gets the focus there).
    fn sync_label(&mut self) {
        let open = self.editor.label_edit().is_some();
        if open && !self.label_open {
            self.label_caret = self.editor.label_edit().map(|l| l.text.chars().count()).unwrap_or(0);
        }
        self.label_open = open;
    }

    /// A key while the label editor is open: editing keys, Enter commits, Escape cancels. `true` when used.
    fn label_key(&mut self, key: u16) -> bool {
        use kubuno_desktop::controls::host::vk;
        let Some(text) = self.editor.label_edit().map(|l| l.text.clone()) else { return false };
        let mut chars: Vec<char> = text.chars().collect();
        let caret = self.label_caret.min(chars.len());
        match key {
            k if k == vk::ENTER => self.editor.commit_label(),
            k if k == vk::ESCAPE => self.editor.cancel_label(),
            k if k == vk::LEFT => self.label_caret = caret.saturating_sub(1),
            k if k == vk::RIGHT => self.label_caret = (caret + 1).min(chars.len()),
            k if k == vk::HOME => self.label_caret = 0,
            k if k == vk::END => self.label_caret = chars.len(),
            k if k == vk::BACK => {
                if caret > 0 {
                    chars.remove(caret - 1);
                    self.label_caret = caret - 1;
                    self.editor.set_label_text(&chars.iter().collect::<String>());
                }
            }
            k if k == vk::DELETE => {
                if caret < chars.len() {
                    chars.remove(caret);
                    self.editor.set_label_text(&chars.iter().collect::<String>());
                }
            }
            _ => return false,
        }
        true
    }

    /// A character typed in the label editor.
    fn label_char(&mut self, c: char) {
        let Some(text) = self.editor.label_edit().map(|l| l.text.clone()) else { return };
        let mut chars: Vec<char> = text.chars().collect();
        let caret = self.label_caret.min(chars.len());
        chars.insert(caret, c);
        self.label_caret = caret + 1;
        self.editor.set_label_text(&chars.iter().collect::<String>());
    }

    fn mods(m: kubuno_desktop::controls::host::Modifiers) -> Mods {
        Mods { shift: m.shift, alt: m.alt, ctrl: m.ctrl }
    }

    /// The canvas' own text measure (DirectWrite through the cache) for the hit tests that measure text;
    /// fixed metrics when no frame was painted yet.
    fn with_measure<R>(&mut self, f: impl FnOnce(&mut Editor, &mut dyn kubuno_office_diagrams_core::editor::TextMeasure) -> R) -> R {
        match self.dwrite.clone() {
            Some(dwrite) => {
                let mut fonts = crate::platform::surface::Fonts { dwrite: &dwrite, cache: &self.cache };
                f(&mut self.editor, &mut fonts)
            }
            None => {
                let mut fixed = Recorder::default();
                f(&mut self.editor, &mut fixed)
            }
        }
    }
}

impl Control for DiagramCanvas {
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

    fn is_input_key(&self, key: Keys) -> bool {
        use kubuno_desktop::controls::host::vk;
        if self.editor.label_edit().is_some() {
            return [vk::DELETE, vk::BACK, vk::ESCAPE, vk::ENTER, vk::LEFT, vk::RIGHT, vk::HOME, vk::END, vk::TAB].contains(&key.key.0);
        }
        [vk::DELETE, vk::BACK, vk::ESCAPE].contains(&key.key.0)
    }

    fn on_mouse_wheel(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y, notches) = (e.args().x as f64, e.args().y as f64, e.args().delta as f64);
        // The framework's delta is in notches, positive away from the user (up): the web's deltaY < 0.
        self.editor.wheel(x, y, -notches);
        self.changed();
        e.raise(&*self, "OnMouseWheel");
    }

    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y) = (e.args().x, e.args().y);
        let mods = Self::mods(e.args().mods);
        let clicks = e.args().clicks;
        self.focus();
        if e.args().button == MouseButton::Left {
            if let Some((mx, my, mw, mh)) = self.minimap_box() {
                if x >= mx && x <= mx + mw && y >= my && y <= my + mh {
                    self.minimap_drag = true;
                    self.minimap_jump(x, y);
                    self.changed();
                    e.raise(&*self, "OnMouseDown");
                    return;
                }
            }
        }
        match e.args().button {
            MouseButton::Left if clicks >= 2 => {
                // The second press of a double click: the web's `dblclick` (after its two mousedown/up).
                self.with_measure(|ed, m| {
                    ed.pointer_down(x as f64, y as f64, Button::Left, mods, m);
                    ed.pointer_up(x as f64, y as f64);
                    ed.double_click(x as f64, y as f64, m);
                });
            }
            MouseButton::Left => self.with_measure(|ed, m| ed.pointer_down(x as f64, y as f64, Button::Left, mods, m)),
            MouseButton::Middle => self.with_measure(|ed, m| ed.pointer_down(x as f64, y as f64, Button::Middle, mods, m)),
            MouseButton::Right => {
                let target = self.editor.context_menu(x as f64, y as f64);
                let mut args = DiagramContextEventArgs { x: self.bounds.left + x, y: self.bounds.top + y, ..Default::default() };
                match target {
                    ContextTarget::Shape { id } => {
                        args.kind = "shape".into();
                        args.id = id;
                    }
                    ContextTarget::Connector { id, seg, world } => {
                        args.kind = "connector".into();
                        args.id = id;
                        args.seg = seg;
                        args.world_x = world.x;
                        args.world_y = world.y;
                    }
                    ContextTarget::Canvas { world } => {
                        args.kind = "canvas".into();
                        args.world_x = world.x;
                        args.world_y = world.y;
                    }
                }
                self.changed();
                self.raise_context_menu_requested(args);
            }
            MouseButton::None => {}
        }
        self.changed();
        e.raise(&*self, "OnMouseDown");
    }

    fn on_mouse_move(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y) = (e.args().x, e.args().y);
        if self.minimap_drag {
            if e.args().button == MouseButton::Left {
                self.minimap_jump(x, y);
                self.changed();
            } else {
                self.minimap_drag = false;
            }
            e.raise(&*self, "OnMouseMove");
            return;
        }
        let mods = Self::mods(e.args().mods);
        let dragging = self.editor.is_dragging();
        if let Some(c) = self.editor.pointer_move(x as f64, y as f64, mods) {
            self.cursor = c;
        }
        if dragging || self.editor.hovered().is_some() || self.reported.as_ref().is_some_and(|r| r.selected + r.selected_conns > 0) {
            self.changed();
        } else {
            self.invalidate();
        }
        e.raise(&*self, "OnMouseMove");
    }

    fn on_mouse_up(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        if std::mem::take(&mut self.minimap_drag) {
            e.raise(&*self, "OnMouseUp");
            return;
        }
        let (x, y) = (e.args().x, e.args().y);
        if e.args().button != MouseButton::Right {
            self.editor.pointer_up(x as f64, y as f64);
        }
        self.changed();
        e.raise(&*self, "OnMouseUp");
    }

    fn on_mouse_leave(&mut self, e: &mut EventCx<'_, kubuno_desktop::views::events::EmptyEventArgs>) {
        if !self.editor.is_dragging() {
            self.editor.pointer_leave();
            self.changed();
        }
        e.raise(&*self, "OnMouseLeave");
    }

    fn cursor_at(&self, x: f32, y: f32) -> Option<kubuno_desktop::controls::host::Cursor> {
        use kubuno_desktop::controls::host::Cursor as C;
        use kubuno_office_diagrams_core::geometry::Handle;
        if let Some((mx, my, mw, mh)) = self.minimap_box() {
            if x >= mx && x <= mx + mw && y >= my && y <= my + mh {
                return Some(C::Hand);
            }
        }
        Some(match self.cursor {
            Cursor::Default => C::Arrow,
            Cursor::Move => C::Move,
            Cursor::Crosshair => C::Crosshair,
            Cursor::Resize(h) => match h {
                Handle::N | Handle::S => C::ResizeNS,
                Handle::E | Handle::W => C::ResizeEW,
                Handle::Nw | Handle::Se => C::ResizeNWSE,
                Handle::Ne | Handle::Sw => C::ResizeNESW,
            },
        })
    }

    fn on_key_down(&mut self, e: &mut EventCx<'_, KeyEventArgs>) {
        use kubuno_desktop::controls::host::vk;
        let key = e.args().key.0;
        let handled = if self.editor.label_edit().is_some() {
            self.label_key(key)
        } else if key == vk::DELETE || key == vk::BACK {
            self.editor.delete_selected();
            true
        } else if key == vk::ESCAPE {
            self.editor.escape();
            true
        } else {
            false
        };
        if handled {
            self.changed();
            e.args_mut().handled = true;
            e.args_mut().suppress_key_press = true;
        }
        e.raise(&*self, "OnKeyDown");
    }

    fn on_key_press(&mut self, e: &mut EventCx<'_, KeyPressEventArgs>) {
        let c = e.args().key_char;
        if self.editor.label_edit().is_some() && !c.is_control() {
            self.label_char(c);
            self.changed();
            e.args_mut().handled = true;
        }
        e.raise(&*self, "OnKeyPress");
    }

    fn on_drag_over(&mut self, e: &mut EventCx<'_, kubuno_desktop::views::events::DragEventArgs>) {
        if stencil_of(&e.args().data).is_some() {
            e.args_mut().effect = kubuno_desktop::views::events::DragDropEffects::COPY;
        }
        e.raise(&*self, "OnDragOver");
    }

    fn on_drag_drop(&mut self, e: &mut EventCx<'_, kubuno_desktop::views::events::DragEventArgs>) {
        // `handleCanvasDrop`: the stencil at its default size, centred on the drop point.
        if let Some(kind) = stencil_of(&e.args().data) {
            let w = self.editor.world(e.args().x as f64, e.args().y as f64);
            self.editor.place_stencil(&kind, w.x, w.y);
            self.changed();
        }
        e.raise(&*self, "OnDragDrop");
    }
}

/// The stencil a palette drag carries (`kubuno-stencil:<id>`).
fn stencil_of(data: &kubuno_desktop::views::events::DataObject) -> Option<String> {
    data.text.as_deref()?.strip_prefix("kubuno-stencil:").map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_state_reports_the_sample() {
        let c = DiagramCanvas::default();
        let s = c.state_args();
        assert_eq!((s.shapes, s.connectors, s.pages), (5, 4, 1));
        assert!(!s.dirty);
    }

    #[test]
    fn opening_a_diagram_is_reported_even_with_the_same_page_count_and_revision() {
        // A server diagram replaces the sample: one page for one page, revision 0 for revision 0 - the panels
        // (page bar, layers) must still be told, or they keep the sample's layers.
        let mut c = DiagramCanvas::default();
        let before = c.state_args();
        let pages = c.editor().pages().to_vec();
        c.open(pages);
        let after = c.state_args();
        assert_eq!((before.pages, before.revision), (after.pages, after.revision));
        assert_ne!(before.opened, after.opened);
    }
}
