//! `SlideCanvas` — the editing surface: the workspace's ground, the active slide drawn by the shared renderer
//! (`kubuno_office_slides_core::render`) at the fitted or chosen zoom, the editor's furniture
//! (`kubuno_office_slides_core::overlay`), and the zoom pill of the web (bottom right: zoom out, « Ajusté » / n %,
//! zoom in, %, grid, snap to grid, checkerboard, dark ground, ruler).
//!
//! Input goes to the shared editor (`kubuno_office_slides_core::edit`) through the window's workspace: press,
//! drag and release (move with smart guides, marquee, resize, rotate, knobs, crop, drawing), double click (text,
//! table cell, crop, end of a polyline), keys (the canvas's, or the text's while it is edited), typing. What the
//! ribbon shows leaves as `StateChanged`; a right click as `MenuRequested`.

use std::rc::Rc;

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::{Canvas, Rect, Size};
use kubuno_desktop::views::component::{Component, Control, ControlCore, EventCx, Keys, PaintEventCx};
use kubuno_desktop::views::events::Event;
use kubuno_office_slides_core::edit::{Furniture, Handle, Mods, Tool};
use kubuno_office_slides_core::model::{Element, SLIDE_H, SLIDE_W};
use kubuno_office_slides_core::overlay::{self, Scene};
use kubuno_office_slides_core::render::{self, Mode, Options, Renderer, TextBox};
use kubuno_office_slides_core::surface::{Ctx, Matrix};
use kubuno_office_slides_core::textedit::Pos;

use crate::model::{now_ms, Shared};
use crate::platform::d2d::{D2dSurface, Fonts};
use crate::platform::images::BitmapCache;

/// `pointer-select.ts`-like multi-click timing.
const MULTI_CLICK_MS: u64 = 500;
const MULTI_CLICK_SLOP: f32 = 4.0;
/// The caret: solid 500 ms after a move, then blinking (1 s period).
const BLINK_SOLID_MS: u64 = 500;
const BLINK_HALF_MS: u64 = 500;
/// The zoom pill's buttons, from its left: zoom out, the label, zoom in, %, |, grid, snap, checker, dark, ruler.
const PILL_H: f32 = 36.0;

/// What the ribbon and the status bar show of the canvas (raised as `StateChanged` when it differs).
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct CanvasStateEventArgs {
    pub revision: u64,
    /// Number of selected elements, and the kind of the single one (`text`, `shape`… or empty).
    pub selected: usize,
    pub kind: String,
    pub editing_text: bool,
    pub tool: String,
    pub zoom_text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub superscript: bool,
    pub subscript: bool,
    pub align: String,
    pub list: String,
    pub font_family: String,
    pub font_size: String,
    pub can_group: bool,
    pub can_ungroup: bool,
    pub locked: bool,
    pub shadow: bool,
    pub anim: String,
    pub anim_exit: String,
    pub anim_duration: String,
    pub anim_delay: String,
    pub message: String,
}

/// A right click: on an element (its menu) or on the slide's ground (the canvas menu), at window DIP `(x, y)`.
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct CanvasMenuEventArgs {
    pub on_element: bool,
    pub x: f32,
    pub y: f32,
}

/// An image bar button the window answers (`replace`, `alt`).
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct ImageActionEventArgs {
    pub action: String,
}

/// The slide's editing surface (see the module doc).
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Presentations")]
#[toolbox(icon = "presentation")]
#[default_event("StateChanged")]
pub struct SlideCanvas {
    base: ControlCore,
    /// Occurs when what the ribbon shows of the selection or the document changes.
    #[event]
    #[category("Action")]
    pub state_changed: Event<CanvasStateEventArgs>,
    /// Occurs on a right click (the window opens the element's or the canvas's menu).
    #[event]
    #[category("Action")]
    pub menu_requested: Event<CanvasMenuEventArgs>,
    /// Occurs when an image's bar asks for something: `replace`, `alt` (the window owns the dialogs).
    #[event]
    #[category("Action")]
    pub image_action: Event<ImageActionEventArgs>,
    doc: Option<Shared>,
    fonts: Option<(usize, Rc<Fonts>)>,
    bitmaps: BitmapCache,
    bounds: Rect,
    /// `zoomFit`: the scale follows the view; otherwise `manual` (canvas px per slide px).
    pub zoom_fit: bool,
    manual: f64,
    fit: f64,
    /// Where the slide's top-left is (window DIP) and the current scale, from the last paint.
    origin: (f64, f64),
    scale: f64,
    /// Wheel pan when the zoomed slide overflows the view.
    pan: (f64, f64),
    pub dark_ground: bool,
    pub checker: bool,
    pub show_ruler: bool,
    has_focus: bool,
    caret_moved_ms: u64,
    last_click: (u64, f32, f32, u32),
    pressing: bool,
    /// Text selection by drag.
    text_drag: bool,
    reported: Option<CanvasStateEventArgs>,
    message: String,
    hover_pill: Option<usize>,
}

impl Default for SlideCanvas {
    fn default() -> Self {
        SlideCanvas {
            base: ControlCore::default(),
            state_changed: Event::default(),
            menu_requested: Event::default(),
            image_action: Event::default(),
            doc: None,
            fonts: None,
            bitmaps: BitmapCache::default(),
            bounds: Rect::default(),
            zoom_fit: true,
            manual: 1.0,
            fit: 1.0,
            origin: (0.0, 0.0),
            scale: 1.0,
            pan: (0.0, 0.0),
            dark_ground: false,
            checker: false,
            show_ruler: false,
            has_focus: false,
            caret_moved_ms: 0,
            last_click: (0, 0.0, 0.0, 0),
            pressing: false,
            text_drag: false,
            reported: None,
            message: String::new(),
            hover_pill: None,
        }
    }
}

fn mods(m: kubuno_desktop::controls::host::Modifiers) -> Mods {
    Mods { shift: m.shift, ctrl: m.ctrl, alt: m.alt }
}

impl SlideCanvas {
    /// Shows the window's presentation.
    pub fn set_doc(&mut self, doc: Shared) {
        self.doc = Some(doc);
        self.invalidate();
    }

    pub fn doc(&self) -> Option<Shared> {
        self.doc.clone()
    }

    pub fn fonts(&self) -> Option<Rc<Fonts>> {
        self.fonts.as_ref().map(|f| f.1.clone())
    }

    /// The scale in use (canvas px per slide px).
    pub fn scale(&self) -> f64 {
        self.scale
    }

    // ── Zoom (`zoomBy`, `zoomTo`, `zoomToFit`) ──────────────────────────────

    pub fn zoom_by(&mut self, f: f64) {
        self.manual = (self.scale * f).clamp(0.1, 5.0);
        self.zoom_fit = false;
        self.invalidate();
    }

    /// `n` % of the fitted size (the web's % menu).
    pub fn zoom_to_percent(&mut self, n: f64) {
        self.manual = (self.fit * n / 100.0).clamp(0.1, 5.0);
        self.zoom_fit = false;
        self.invalidate();
    }

    pub fn zoom_to_fit(&mut self) {
        self.zoom_fit = true;
        self.pan = (0.0, 0.0);
        self.invalidate();
    }

    /// « Ajusté » or `n%` of the fitted size.
    pub fn zoom_text(&self) -> String {
        if self.zoom_fit {
            "Ajusté".into()
        } else {
            format!("{}%", ((self.scale / self.fit.max(0.0001)) * 100.0).round())
        }
    }

    // ── Geometry ────────────────────────────────────────────────────────────

    /// Window DIP → slide fractions.
    fn to_slide(&self, x: f32, y: f32) -> (f64, f64) {
        let s = self.scale.max(0.0001);
        (((x as f64) - self.origin.0) / (SLIDE_W * s), ((y as f64) - self.origin.1) / (SLIDE_H * s))
    }

    /// Window DIP → canvas px of the slide (origin at its top-left).
    fn to_canvas(&self, x: f32, y: f32) -> (f64, f64) {
        ((x as f64) - self.origin.0, (y as f64) - self.origin.1)
    }

    /// The text layout of the element being edited (canvas px, unrotated frame): a text box's own, or for a
    /// shape's caption a centred box like `renderShapeText`'s.
    fn edit_layout(&self, el: &Element) -> Option<TextBox> {
        let doc = self.doc.as_ref()?;
        let ws = doc.ws.borrow();
        let te = ws.text.as_ref()?;
        let fonts = self.fonts.as_ref()?.1.clone();
        let s = self.scale;
        let (x, y, w, h) = (el.x() * SLIDE_W * s, el.y() * SLIDE_H * s, el.w() * SLIDE_W * s, el.h() * SLIDE_H * s);
        if el.kind() == "shape" {
            let pad = if el.w() > 0.0 { 600.0 / (el.w() * SLIDE_W) } else { 0.0 };
            let synth = Element::parse(&format!(
                r#"{{"type":"text","fontSize":{},"color":{},"fontFamily":{},"align":"center","verticalAlign":"middle","padding":{}}}"#,
                kubuno_office_slides_core::model::num(el.f_or("fontSize", 18.0)),
                kubuno_office_slides_core::obj::quote(el.s("color").unwrap_or("#ffffff")),
                kubuno_office_slides_core::obj::quote(el.s("fontFamily").unwrap_or("Arial, sans-serif")),
                kubuno_office_slides_core::model::num(pad)
            ))?;
            return render::layout_text_box_with(&synth, te.paras.clone(), x, y, w, h, &ws.deck.theme, false, s, &*fonts, true);
        }
        render::layout_text_box_with(el, te.paras.clone(), x, y, w, h, &ws.deck.theme, false, s, &*fonts, true)
    }

    /// A canvas-px point in an element's unrotated frame.
    fn unrotate(el: &Element, s: f64, px: f64, py: f64) -> (f64, f64) {
        let (x, y, w, h) = (el.x() * SLIDE_W * s, el.y() * SLIDE_H * s, el.w() * SLIDE_W * s, el.h() * SLIDE_H * s);
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        let r = -el.rotation().to_radians();
        let (dx, dy) = (px - cx, py - cy);
        (cx + dx * r.cos() - dy * r.sin(), cy + dx * r.sin() + dy * r.cos())
    }

    fn caret_visible(&self) -> bool {
        if !self.has_focus {
            return false;
        }
        let t = now_ms().saturating_sub(self.caret_moved_ms);
        t < BLINK_SOLID_MS || ((t - BLINK_SOLID_MS) / BLINK_HALF_MS) % 2 == 1
    }

    fn moved(&mut self) {
        self.caret_moved_ms = now_ms();
        self.invalidate();
    }

    // ── The zoom pill ───────────────────────────────────────────────────────

    /// The pill's buttons (window DIP rectangles).
    fn pill_buttons(&self) -> Vec<Rect> {
        let widths = [28.0, 48.0, 28.0, 40.0, 9.0, 28.0, 28.0, 28.0, 28.0, 28.0];
        let total: f32 = widths.iter().sum::<f32>() + 12.0 + 2.0 * (widths.len() as f32 - 1.0);
        let right = self.bounds.right - 12.0;
        let top = self.bounds.bottom - 12.0 - PILL_H;
        let mut x = right - total + 6.0;
        widths
            .iter()
            .map(|w| {
                let r = Rect::new(x, top + 4.0, x + w, top + PILL_H - 4.0);
                x += w + 2.0;
                r
            })
            .collect()
    }

    fn pill_rect(&self) -> Rect {
        let b = self.pill_buttons();
        match (b.first(), b.last()) {
            (Some(f), Some(l)) => Rect::new(f.left - 6.0, f.top - 4.0, l.right + 6.0, f.bottom + 4.0),
            _ => Rect::default(),
        }
    }

    fn pill_click(&mut self, i: usize) {
        match i {
            0 => self.zoom_by(1.0 / 1.2),
            1 => self.zoom_to_fit(),
            2 => self.zoom_by(1.2),
            3 => {
                let r = self.pill_buttons()[3];
                kubuno_desktop::views::window::show_context_menu("zoom_menu", kubuno_desktop::views::window::MenuAnchor::Point(r.left, r.top - 200.0));
            }
            5 => self.with_editor(|ed| ed.show_grid = !ed.show_grid),
            6 => self.with_editor(|ed| ed.snap_grid = !ed.snap_grid),
            7 => self.checker = !self.checker,
            8 => self.dark_ground = !self.dark_ground,
            9 => self.show_ruler = !self.show_ruler,
            _ => {}
        }
        self.invalidate();
    }

    fn with_editor(&self, f: impl FnOnce(&mut kubuno_office_slides_core::edit::Editor)) {
        if let Some(d) = &self.doc {
            f(&mut d.ws.borrow_mut().editor);
        }
    }

    fn pill_states(&self) -> [bool; 5] {
        let (grid, snap) = self.doc.as_ref().map(|d| {
            let ws = d.ws.borrow();
            (ws.editor.show_grid, ws.editor.snap_grid)
        }).unwrap_or((false, false));
        [grid, snap, self.checker, self.dark_ground, self.show_ruler]
    }

    // ── State for the ribbon ────────────────────────────────────────────────

    fn state_args(&self) -> CanvasStateEventArgs {
        let mut a = CanvasStateEventArgs { zoom_text: self.zoom_text(), message: self.message.clone(), ..Default::default() };
        let Some(doc) = &self.doc else { return a };
        let ws = doc.ws.borrow();
        a.revision = ws.revision;
        let els = ws.active_elements();
        a.selected = ws.editor.selection.len();
        a.editing_text = ws.text.is_some();
        a.tool = match &ws.editor.tool {
            Tool::Select => "select".into(),
            Tool::Text => "text".into(),
            Tool::Shape(_) => "shape".into(),
            Tool::Line(k) => format!("line:{k}"),
        };
        a.can_group = ws.editor.can_group();
        a.can_ungroup = ws.editor.can_ungroup(els);
        let (anim, dur, delay, exit) = ws.editor.anim_meta(els);
        a.anim = anim;
        a.anim_exit = exit;
        a.anim_duration = format!("{dur}");
        a.anim_delay = format!("{delay}");
        if let Some(el) = ws.editor.single(els) {
            a.kind = el.kind().to_string();
            a.locked = el.locked();
            a.shadow = el.truthy("shadow");
            a.bold = el.truthy("bold");
            a.italic = el.truthy("italic");
            a.underline = el.truthy("underline");
            a.align = el.s("align").unwrap_or("left").to_string();
            a.font_family = el.s("fontFamily").unwrap_or("").to_string();
            a.font_size = el.f("fontSize").map(|f| format!("{f}")).unwrap_or_else(|| if el.kind() == "text" { "24".into() } else { "18".into() });
            let paras = kubuno_office_slides_core::richtext::doc_to_paras(el.get("content"));
            a.list = paras.first().and_then(|p| p.attrs.list()).unwrap_or("").to_string();
            if let Some(p) = paras.first().and_then(|p| p.attrs.align()) {
                a.align = p.to_string();
            }
        }
        if let Some(te) = &ws.text {
            let m = te.marks_state();
            a.bold |= m.b;
            a.italic |= m.i;
            a.underline |= m.u;
            a.strike = m.s;
            a.superscript = m.sup;
            a.subscript = m.sub;
            if let Some(sz) = m.size {
                a.font_size = format!("{sz}");
            }
        }
        a
    }

    /// Raises `StateChanged` when it differs (and repaints).
    pub fn report(&mut self) {
        if self.design_mode() {
            return;
        }
        let a = self.state_args();
        if self.reported.as_ref() != Some(&a) {
            self.reported = Some(a.clone());
            self.raise_state_changed(a);
        }
    }

    pub fn set_message(&mut self, m: &str) {
        self.message = m.to_string();
        self.report();
    }

    // ── Pointer helpers ─────────────────────────────────────────────────────

    fn edit_with(&mut self, f: impl FnOnce(&mut Vec<Element>, &mut kubuno_office_slides_core::edit::Editor, &mut kubuno_office_slides_core::insert::Ids) -> kubuno_office_slides_core::edit::Outcome) {
        let Some(doc) = self.doc.clone() else { return };
        let out = doc.ws.borrow_mut().edit(now_ms(), f);
        if out.repaint || out.changed {
            self.invalidate();
        }
        self.report();
    }

    /// A press inside the text being edited moves its caret (or starts a selection drag); elsewhere it ends the
    /// editing. Returns true when the press was the text's.
    fn text_press(&mut self, x: f32, y: f32, count: u32, extend: bool) -> bool {
        let Some(doc) = self.doc.clone() else { return false };
        let el = {
            let ws = doc.ws.borrow();
            let Some(te) = ws.text.as_ref() else { return false };
            match ws.active_elements().iter().find(|e| e.id() == te.id) {
                Some(e) => e.clone(),
                None => return false,
            }
        };
        let (cx, cy) = self.to_canvas(x, y);
        let s = self.scale;
        let (lx, ly) = Self::unrotate(&el, s, cx, cy);
        let (bx, by, bw, bh) = (el.x() * SLIDE_W * s, el.y() * SLIDE_H * s, el.w() * SLIDE_W * s, el.h() * SLIDE_H * s);
        if !(lx >= bx && lx <= bx + bw && ly >= by && ly <= by + bh) {
            doc.ws.borrow_mut().end_text();
            self.invalidate();
            return false;
        }
        let (Some(tb), Some(fonts)) = (self.edit_layout(&el), self.fonts()) else { return true };
        let mut ws = doc.ws.borrow_mut();
        if let Some(te) = ws.text.as_mut() {
            let p = te.pos_at(&tb, lx, ly, &*fonts);
            match count {
                2 => te.select_word(p),
                3 => te.select_para(p),
                _ => te.set_caret(p, extend),
            }
            te.goal_x = None;
        }
        drop(ws);
        self.text_drag = count == 1;
        self.moved();
        self.report();
        true
    }

    fn double_click(&mut self, fx: f64, fy: f64) {
        let Some(doc) = self.doc.clone() else { return };
        // A polyline in progress ends.
        if doc.ws.borrow().editor.polyline.is_some() {
            self.edit_with(|els, ed, _| ed.finish_polyline(els));
            return;
        }
        let hit = {
            let ws = doc.ws.borrow();
            kubuno_office_slides_core::edit::hit_test(ws.active_elements(), fx * SLIDE_W, fy * SLIDE_H).cloned()
        };
        let Some(el) = hit else { return };
        let mut ws = doc.ws.borrow_mut();
        match el.kind() {
            "table" => {
                if let Some((row, col)) = kubuno_office_slides_core::table::cell_at(&el, (fx - el.x()) / el.w().max(1e-9), (fy - el.y()) / el.h().max(1e-9)) {
                    ws.begin_cell(el.id(), row, col);
                }
            }
            "text" | "shape" => {
                ws.begin_text(el.id());
            }
            "image" => {
                let els = ws.active_elements().to_vec();
                ws.editor.enter_crop(&els, el.id(), self.scale);
            }
            _ => {}
        }
        drop(ws);
        self.moved();
        self.report();
    }

    // ── Keys ────────────────────────────────────────────────────────────────

    fn text_key(&mut self, key: u16, m: kubuno_desktop::controls::host::Modifiers) -> bool {
        use kubuno_desktop::controls::host::vk;
        let Some(doc) = self.doc.clone() else { return false };
        if doc.ws.borrow().text.is_none() {
            return false;
        }
        let now = now_ms();
        let el = {
            let ws = doc.ws.borrow();
            ws.text.as_ref().and_then(|t| ws.active_elements().iter().find(|e| e.id() == t.id).cloned())
        };
        let layout = el.as_ref().and_then(|e| self.edit_layout(e));
        let fonts = self.fonts();
        let mut ws = doc.ws.borrow_mut();
        let (ctrl, shift) = (m.ctrl, m.shift);
        match key {
            k if k == vk::ESCAPE => ws.end_text(),
            k if k == vk::LEFT => {
                if let Some(t) = ws.text.as_mut() {
                    t.move_horizontal(false, ctrl, shift)
                }
            }
            k if k == vk::RIGHT => {
                if let Some(t) = ws.text.as_mut() {
                    t.move_horizontal(true, ctrl, shift)
                }
            }
            k if k == vk::UP || k == vk::DOWN => {
                if let (Some(t), Some(tb), Some(f)) = (ws.text.as_mut(), layout.as_ref(), fonts.as_ref()) {
                    t.move_vertical(tb, k == vk::DOWN, shift, &**f)
                }
            }
            k if k == vk::HOME || k == vk::END => {
                if let Some(t) = ws.text.as_mut() {
                    if ctrl {
                        t.move_doc(k == vk::END, shift);
                    } else if let Some(tb) = layout.as_ref() {
                        t.move_line_edge(tb, k == vk::END, shift);
                    }
                }
            }
            k if k == vk::BACK => {
                ws.text_op(now, |t| t.delete_backward(ctrl));
            }
            k if k == vk::DELETE => {
                ws.text_op(now, |t| t.delete_forward(ctrl));
            }
            k if k == vk::ENTER => {
                ws.text_op(now, |t| t.enter());
            }
            k if k == vk::letter('A') && ctrl => {
                if let Some(t) = ws.text.as_mut() {
                    t.select_all()
                }
            }
            k if k == vk::letter('C') && ctrl => {
                if let Some(t) = ws.text.as_ref() {
                    crate::platform::clipboard::set_text(&t.selected_text());
                }
            }
            k if k == vk::letter('X') && ctrl => {
                if let Some(t) = ws.text.as_ref() {
                    crate::platform::clipboard::set_text(&t.selected_text());
                }
                ws.text_op(now, |t| {
                    t.delete_selection();
                });
            }
            k if k == vk::letter('V') && ctrl => {
                if let Some(s) = crate::platform::clipboard::get_text() {
                    let s = s.replace("\r\n", "\n");
                    ws.text_op(now, |t| t.insert_text(&s));
                }
            }
            _ => return false,
        }
        drop(ws);
        self.moved();
        self.report();
        true
    }

    fn cell_key(&mut self, key: u16) -> bool {
        use kubuno_desktop::controls::host::vk;
        let Some(doc) = self.doc.clone() else { return false };
        let mut ws = doc.ws.borrow_mut();
        let Some(c) = ws.cell.as_mut() else { return false };
        let n = c.value.chars().count();
        match key {
            k if k == vk::ENTER || k == vk::ESCAPE => {
                ws.commit_cell(now_ms());
            }
            k if k == vk::LEFT => c.caret = c.caret.saturating_sub(1),
            k if k == vk::RIGHT => c.caret = (c.caret + 1).min(n),
            k if k == vk::HOME => c.caret = 0,
            k if k == vk::END => c.caret = n,
            k if k == vk::BACK && c.caret > 0 => {
                let mut v: Vec<char> = c.value.chars().collect();
                v.remove(c.caret - 1);
                c.value = v.into_iter().collect();
                c.caret -= 1;
            }
            k if k == vk::DELETE && c.caret < n => {
                let mut v: Vec<char> = c.value.chars().collect();
                v.remove(c.caret);
                c.value = v.into_iter().collect();
            }
            _ => return false,
        }
        drop(ws);
        self.moved();
        self.report();
        true
    }

    fn canvas_key(&mut self, key: u16, m: kubuno_desktop::controls::host::Modifiers) -> bool {
        use kubuno_desktop::controls::host::vk;
        let name: String = match key {
            k if k == vk::DELETE => "Delete".into(),
            k if k == vk::BACK => "Backspace".into(),
            k if k == vk::ESCAPE => "Escape".into(),
            k if k == vk::ENTER => "Enter".into(),
            k if k == vk::LEFT => "ArrowLeft".into(),
            k if k == vk::RIGHT => "ArrowRight".into(),
            k if k == vk::UP => "ArrowUp".into(),
            k if k == vk::DOWN => "ArrowDown".into(),
            0xDB => "[".into(),
            0xDD => "]".into(),
            k if (0x41..=0x5A).contains(&k) && m.ctrl => ((k as u8) as char).to_ascii_lowercase().to_string(),
            _ => return false,
        };
        let Some(doc) = self.doc.clone() else { return false };
        let out = doc.ws.borrow_mut().canvas_key(now_ms(), &name, mods(m), self.scale);
        match out {
            Some(_) => {
                self.invalidate();
                self.report();
                true
            }
            None => false,
        }
    }

    // ── Painting ────────────────────────────────────────────────────────────

    fn paint_ground(&self, c: &dyn kubuno_desktop::controls::ControlCanvas) {
        let t = c.theme();
        let ground = if self.dark_ground { crate::controls::hex(0x262626) } else { t.surface_2 };
        c.fill_rounded(&self.bounds, 0.0, &ground);
        if self.checker {
            // `linear-gradient` checkerboard of 20 px with #d0d0d0.
            let sq = crate::controls::hex(0xd0d0d0);
            let mut y = self.bounds.top;
            let mut row = 0;
            while y < self.bounds.bottom {
                let mut x = self.bounds.left + if row % 2 == 0 { 0.0 } else { 10.0 };
                while x < self.bounds.right {
                    c.fill_rounded(&Rect::new(x, y, (x + 10.0).min(self.bounds.right), (y + 10.0).min(self.bounds.bottom)), 0.0, &sq);
                    x += 20.0;
                }
                y += 10.0;
                row += 1;
            }
        }
    }

    fn paint_pill(&self, c: &dyn kubuno_desktop::controls::ControlCanvas) {
        let t = c.theme();
        let r = self.pill_rect();
        c.draw_card_shadow(&r, PILL_H / 2.0);
        c.fill_rounded(&r, PILL_H / 2.0, &t.card_background);
        c.stroke_rounded(&r, PILL_H / 2.0, &t.card_stroke);
        let states = self.pill_states();
        let icons: [&'static str; 10] = ["ZoomOut", "", "ZoomIn", "", "", "Grid3x3", "Magnet", "LayoutTemplate", "EyeOff", "AlignStartVertical"];
        for (i, b) in self.pill_buttons().iter().enumerate() {
            if self.hover_pill == Some(i) && i != 4 {
                c.fill_rounded(b, 14.0, &t.surface_2);
            }
            let active = i >= 5 && states[i - 5];
            let ink = if active { t.accent } else { t.text_secondary };
            match i {
                1 => {
                    let f = c.formats();
                    c.text(&self.zoom_text(), b, &f.caption, &t.text_secondary, true);
                }
                3 => {
                    let f = c.formats();
                    c.text("%", b, &f.caption, &t.text_secondary, true);
                }
                4 => c.fill_rounded(&Rect::new(b.left + 4.0, b.top + 6.0, b.left + 5.0, b.bottom - 6.0), 0.0, &t.card_stroke),
                _ => c.vector_icon(icons[i], b, 16.0, &ink),
            }
        }
    }
}

impl Control for SlideCanvas {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, _proposed: Size) -> Size {
        Size { width: 800.0, height: 500.0 }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        let b = e.clip_rectangle;
        self.bounds = b;
        let c = e.canvas();
        self.paint_ground(c);
        if let Some(renderer) = c.renderer() {
            let key = windows::core::Interface::as_raw(&renderer.dwrite) as usize;
            if self.fonts.as_ref().is_none_or(|(k, _)| *k != key) {
                self.fonts = Some((key, Rc::new(Fonts::new(renderer.dwrite.clone()))));
            }
        }
        // `fitScale`: the slide fills the view less 24 px.
        let (cw, ch) = ((b.right - b.left) as f64, (b.bottom - b.top) as f64);
        self.fit = ((cw - 24.0) / SLIDE_W).min((ch - 24.0) / SLIDE_H).max(0.1);
        self.scale = if self.zoom_fit { self.fit } else { self.manual };
        let (sw, sh) = (SLIDE_W * self.scale, SLIDE_H * self.scale);
        let over_x = ((sw - cw) / 2.0).max(0.0);
        let over_y = ((sh - ch) / 2.0).max(0.0);
        self.pan = (self.pan.0.clamp(-over_x, over_x), self.pan.1.clamp(-over_y, over_y));
        let ox = b.left as f64 + (cw - sw) / 2.0 + self.pan.0;
        let oy = b.top as f64 + (ch - sh) / 2.0 + self.pan.1;
        self.origin = (ox, oy);
        let (Some(doc), Some(renderer), Some((_, fonts))) = (self.doc.clone(), c.renderer(), self.fonts.clone()) else {
            e.raise(self, "OnPaint");
            return;
        };
        c.push_clip(&b);
        let slide_rect = Rect::new(ox as f32, oy as f32, (ox + sw) as f32, (oy + sh) as f32);
        let caret_on = self.caret_visible();
        let layout = {
            let ws = doc.ws.borrow();
            ws.text.as_ref().and_then(|t| ws.active_elements().iter().find(|el| el.id() == t.id).cloned())
        }
        .and_then(|el| self.edit_layout(&el));
        let mut icons = Vec::new();
        {
            let ws = doc.ws.borrow();
            match ws.active_slide() {
                Some(slide) => {
                    c.draw_card_shadow(&slide_rect, 0.0);
                    let mut surface = D2dSurface::new(renderer, &fonts, &doc.store, &mut self.bitmaps);
                    let base = surface.outer().mul(&Matrix::translation(ox, oy));
                    let crop_id = ws.editor.crop.as_ref().map(|c| c.id.clone());
                    let text_id = ws.text.as_ref().map(|t| t.id.clone());
                    let r = Renderer::new(sw, sh, &*fonts);
                    let opts = Options { mode: Mode::Edit, skip: crop_id.as_deref(), editing: text_id.as_deref(), ..Options::default() };
                    r.render(&mut surface, base, &slide.data, &ws.deck.theme, &opts);
                    let tertiary = if c.theme().mode == kubuno_desktop::ui::ThemeMode::Dark { "#9aa0a6" } else { "#80868b" };
                    let scene = Scene {
                        elements: &slide.data.elements,
                        scale: self.scale,
                        show_ruler: self.show_ruler,
                        text: match (ws.text.as_ref(), layout.as_ref()) {
                            (Some(t), Some(l)) => Some((t, l)),
                            _ => None,
                        },
                        caret_on,
                        cell: ws.cell.as_ref().map(|c| (c.id.as_str(), c.row, c.col, c.value.as_str(), c.caret)),
                        measure: &*fonts,
                        tertiary,
                    };
                    let mut ctx = Ctx::new(&mut surface, base);
                    icons = overlay::paint(&mut ctx, &ws.editor, &scene);
                }
                None => {
                    // `EmptySlideArea`.
                    let t = c.theme();
                    let f = c.formats();
                    let text = if ws.deck.slides.is_empty() { "Ajouter une diapositive" } else { "Sélectionnez une diapositive" };
                    c.text(text, &b, &f.body, &t.text_tertiary, true);
                }
            }
        }
        let t = c.theme();
        for ic in icons {
            let (cx, cy) = ic.center;
            let _ = (cx, cy);
            let r = Rect::new((ox + ic.rect.x) as f32, (oy + ic.rect.y) as f32, (ox + ic.rect.x + ic.rect.w) as f32, (oy + ic.rect.y + ic.rect.h) as f32);
            let ink = match ic.color {
                "primary" => crate::controls::hex(0x1a73e8),
                "tertiary" => t.text_tertiary,
                _ => t.text_secondary,
            };
            c.vector_icon(ic.name, &r, ic.rect.w as f32, &ink);
        }
        self.paint_pill(c);
        c.pop_clip();
        let editing = doc.ws.borrow().text.is_some() || doc.ws.borrow().cell.is_some();
        if self.has_focus && editing {
            let t = now_ms().saturating_sub(self.caret_moved_ms);
            let next = if t < BLINK_SOLID_MS { BLINK_SOLID_MS - t } else { BLINK_HALF_MS - (t - BLINK_SOLID_MS) % BLINK_HALF_MS };
            kubuno_desktop::controls::host::request_repaint_after(next.max(16) as u32);
        }
        self.report();
        e.raise(self, "OnPaint");
    }

    fn is_input_key(&self, key: Keys) -> bool {
        use kubuno_desktop::controls::host::vk;
        [vk::PAGE_DOWN, vk::PAGE_UP, vk::DOWN, vk::UP, vk::LEFT, vk::RIGHT, vk::HOME, vk::END, vk::TAB, vk::ENTER, vk::BACK, vk::DELETE, vk::SPACE, vk::ESCAPE].contains(&key.key.0)
    }

    fn on_got_focus(&mut self, e: &mut EventCx<'_, kubuno_desktop::views::events::EmptyEventArgs>) {
        self.has_focus = true;
        self.moved();
        e.raise(&*self, "OnGotFocus");
    }

    fn on_lost_focus(&mut self, e: &mut EventCx<'_, kubuno_desktop::views::events::EmptyEventArgs>) {
        self.has_focus = false;
        self.invalidate();
        e.raise(&*self, "OnLostFocus");
    }

    fn on_mouse_wheel(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let notches = e.args().delta as f64;
        if e.args().mods.ctrl {
            self.zoom_by(if notches < 0.0 { 1.1 } else { 1.0 / 1.1 });
        } else if e.args().mods.shift {
            self.pan.0 -= notches * 60.0;
        } else {
            self.pan.1 -= notches * 60.0;
        }
        self.invalidate();
        e.raise(&*self, "OnMouseWheel");
    }

    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y) = (e.args().x + self.bounds.left, e.args().y + self.bounds.top);
        let button = e.args().button;
        let m = e.args().mods;
        self.focus();
        self.has_focus = true;
        self.message.clear();
        // The zoom pill.
        if self.pill_rect().contains(x, y) {
            if button == MouseButton::Left {
                if let Some(i) = self.pill_buttons().iter().position(|r| r.contains(x, y)) {
                    self.pill_click(i);
                }
            }
            e.raise(&*self, "OnMouseDown");
            return;
        }
        let Some(doc) = self.doc.clone() else { return };
        if doc.ws.borrow().active.is_none() {
            return;
        }
        let (fx, fy) = self.to_slide(x, y);
        if button == MouseButton::Right {
            let hit = {
                let ws = doc.ws.borrow();
                kubuno_office_slides_core::edit::hit_test(ws.active_elements(), fx * SLIDE_W, fy * SLIDE_H).map(|h| h.id().to_string())
            };
            {
                let mut ws = doc.ws.borrow_mut();
                ws.end_text();
                if let Some(id) = &hit {
                    if !ws.editor.selection.contains(id) {
                        ws.editor.selection = vec![id.clone()];
                    }
                }
            }
            self.invalidate();
            self.report();
            self.raise_menu_requested(CanvasMenuEventArgs { on_element: hit.is_some(), x, y });
            e.raise(&*self, "OnMouseDown");
            return;
        }
        if button != MouseButton::Left {
            return;
        }
        let t = now_ms();
        let (lt, lx0, ly0, n) = self.last_click;
        let near = (x - lx0).abs() <= MULTI_CLICK_SLOP && (y - ly0).abs() <= MULTI_CLICK_SLOP;
        let count = if near && t.saturating_sub(lt) <= MULTI_CLICK_MS { n + 1 } else { 1 };
        self.last_click = (t, x, y, count);
        // A table cell being edited: a press elsewhere commits it.
        if doc.ws.borrow().cell.is_some() {
            doc.ws.borrow_mut().commit_cell(t);
        }
        // The text being edited takes the press inside its box.
        if self.text_press(x, y, count.min(3), m.shift) {
            self.pressing = true;
            e.raise(&*self, "OnMouseDown");
            return;
        }
        if count == 2 {
            self.double_click(fx, fy);
            e.raise(&*self, "OnMouseDown");
            return;
        }
        // The image bar and the fit button of the selection.
        let furniture = {
            let ws = doc.ws.borrow();
            ws.editor.single(ws.active_elements()).map(|el| kubuno_office_slides_core::edit::overlay_of(el, self.scale)).and_then(|ov| {
                let (cx, cy) = self.to_canvas(x, y);
                ov.furniture_at(cx, cy)
            })
        };
        match furniture {
            Some(Furniture::FitButton) => {
                kubuno_desktop::views::window::show_context_menu("fit_menu", kubuno_desktop::views::window::MenuAnchor::Point(x, y + 18.0));
                e.raise(&*self, "OnMouseDown");
                return;
            }
            Some(Furniture::ImageBar(i)) => {
                match i {
                    1 => {
                        let mut ws = doc.ws.borrow_mut();
                        let els = ws.active_elements().to_vec();
                        if let Some(id) = ws.editor.selection.first().cloned() {
                            ws.editor.enter_crop(&els, &id, self.scale);
                        }
                    }
                    2 => self.reset_image_aspect(),
                    0 => self.raise_image_action(ImageActionEventArgs { action: "replace".into() }),
                    _ => self.raise_image_action(ImageActionEventArgs { action: "alt".into() }),
                }
                self.invalidate();
                self.report();
                e.raise(&*self, "OnMouseDown");
                return;
            }
            _ => {}
        }
        let scale = self.scale;
        self.pressing = true;
        self.edit_with(|els, ed, ids| ed.pointer_down(els, fx, fy, mods(m), scale, ids));
        e.raise(&*self, "OnMouseDown");
    }

    fn on_mouse_move(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y) = (e.args().x + self.bounds.left, e.args().y + self.bounds.top);
        let hover = if self.pill_rect().contains(x, y) { self.pill_buttons().iter().position(|r| r.contains(x, y)) } else { None };
        if hover != self.hover_pill {
            self.hover_pill = hover;
            self.invalidate();
        }
        let left = e.args().button == MouseButton::Left;
        if !left && self.pressing {
            self.pressing = false;
            self.text_drag = false;
            self.edit_with(|els, ed, _| ed.pointer_up(els));
        }
        let Some(doc) = self.doc.clone() else { return };
        if self.text_drag && left {
            // Extend the text selection.
            let el = {
                let ws = doc.ws.borrow();
                ws.text.as_ref().and_then(|t| ws.active_elements().iter().find(|e| e.id() == t.id).cloned())
            };
            if let (Some(el), Some(fonts)) = (el, self.fonts()) {
                if let Some(tb) = self.edit_layout(&el) {
                    let (cx, cy) = self.to_canvas(x, y);
                    let (lx, ly) = Self::unrotate(&el, self.scale, cx, cy);
                    if let Some(t) = doc.ws.borrow_mut().text.as_mut() {
                        let p = t.pos_at(&tb, lx, ly, &*fonts);
                        t.set_caret(p, true);
                    }
                    self.moved();
                }
            }
            e.raise(&*self, "OnMouseMove");
            return;
        }
        let busy = doc.ws.borrow().editor.is_busy() || doc.ws.borrow().editor.polyline.is_some();
        if (self.pressing && left) || busy {
            let (fx, fy) = self.to_slide(x, y);
            let (m, scale) = (e.args().mods, self.scale);
            self.edit_with(|els, ed, _| ed.pointer_move(els, fx, fy, mods(m), scale));
        }
        e.raise(&*self, "OnMouseMove");
    }

    fn on_mouse_up(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        if self.pressing {
            self.pressing = false;
            self.text_drag = false;
            self.edit_with(|els, ed, _| ed.pointer_up(els));
        }
        e.raise(&*self, "OnMouseUp");
    }

    fn cursor_at(&self, x: f32, y: f32) -> Option<kubuno_desktop::controls::host::Cursor> {
        use kubuno_desktop::controls::host::Cursor;
        let (x, y) = (x + self.bounds.left, y + self.bounds.top);
        if self.pill_rect().contains(x, y) {
            return Some(Cursor::Hand);
        }
        let doc = self.doc.as_ref()?;
        let ws = doc.ws.borrow();
        if ws.text.is_some() {
            return Some(Cursor::IBeam);
        }
        if ws.editor.tool != Tool::Select {
            return Some(Cursor::Crosshair);
        }
        let el = ws.editor.single(ws.active_elements())?;
        let ov = kubuno_office_slides_core::edit::overlay_of(el, self.scale);
        let (cx, cy) = self.to_canvas(x, y);
        Some(match ov.furniture_at(cx, cy)? {
            Furniture::Resize(Handle(h)) => match h {
                "n" | "s" => Cursor::ResizeNS,
                "e" | "w" => Cursor::ResizeEW,
                "nw" | "se" => Cursor::ResizeNWSE,
                _ => Cursor::ResizeNESW,
            },
            Furniture::Rotate => Cursor::Move,
            _ => Cursor::Hand,
        })
    }

    fn on_key_down(&mut self, e: &mut EventCx<'_, KeyEventArgs>) {
        let (key, m) = (e.args().key.0, e.args().mods);
        let handled = if m.ctrl && m.alt {
            false
        } else if self.doc.as_ref().is_some_and(|d| d.ws.borrow().cell.is_some()) {
            self.cell_key(key)
        } else if self.doc.as_ref().is_some_and(|d| d.ws.borrow().text.is_some()) {
            self.text_key(key, m)
        } else {
            self.canvas_key(key, m)
        };
        if handled {
            e.args_mut().handled = true;
            e.args_mut().suppress_key_press = true;
        }
        e.raise(&*self, "OnKeyDown");
    }

    fn on_key_press(&mut self, e: &mut EventCx<'_, KeyPressEventArgs>) {
        let ch = e.args().key_char;
        if ch.is_control() {
            e.raise(&*self, "OnKeyPress");
            return;
        }
        let Some(doc) = self.doc.clone() else { return };
        let mut handled = false;
        {
            let mut ws = doc.ws.borrow_mut();
            if let Some(c) = ws.cell.as_mut() {
                let mut v: Vec<char> = c.value.chars().collect();
                v.insert(c.caret.min(v.len()), ch);
                c.value = v.into_iter().collect();
                c.caret += 1;
                handled = true;
            } else if ws.text.is_some() {
                let s = ch.to_string();
                ws.text_op(now_ms(), |t| t.insert_text(&s));
                handled = true;
            }
        }
        if handled {
            e.args_mut().handled = true;
            self.moved();
            self.report();
        }
        e.raise(&*self, "OnKeyPress");
    }
}

impl SlideCanvas {
    /// « Réinitialiser les proportions »: no flips, no rotation, the height from the image's ratio.
    pub fn reset_image_aspect(&mut self) {
        let Some(doc) = self.doc.clone() else { return };
        let (id, src) = {
            let ws = doc.ws.borrow();
            match ws.editor.single(ws.active_elements()) {
                Some(el) if el.kind() == "image" => (el.id().to_string(), el.s("storagePath").unwrap_or("").to_string()),
                _ => return,
            }
        };
        let Some(bytes) = doc.store.bytes(&src) else { return };
        let Some((w, h)) = crate::platform::images::size_of(&bytes) else { return };
        let aspect = (h as f64 / (w as f64).max(1.0)) * (SLIDE_W / SLIDE_H);
        self.edit_with(|els, _, _| {
            let Some(e) = els.iter_mut().find(|e| e.id() == id) else { return Default::default() };
            e.set_b("flipX", false);
            e.set_b("flipY", false);
            e.set_f("rotation", 0.0);
            let nh = e.w() * aspect;
            e.set_f("h", nh);
            kubuno_office_slides_core::edit::Outcome { changed: true, repaint: true, tool_reset: false }
        });
    }

    /// Places the caret of a just-opened text at its end (the web's `collapse(false)`) — already done by
    /// `TextEdit::open`; kept for the window's « Modifier le texte ».
    pub fn begin_text(&mut self, id: &str) {
        if let Some(doc) = self.doc.clone() {
            doc.ws.borrow_mut().begin_text(id);
            let _ = Pos::default();
            self.focus();
            self.moved();
            self.report();
        }
    }
}
