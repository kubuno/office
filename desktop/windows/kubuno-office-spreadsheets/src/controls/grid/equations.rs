//! The sheet's equations, drawn and handled like the web's equation overlays (`SA:1107-1218`, `SA:7723-7780`):
//! each one a white box (padding 4, border 1 `#e2e4e6`, radius 4, faint shadow) holding the formula laid out
//! by the math core (`kubuno-office-math-core`, a port of KaTeX) and painted from KaTeX's own glyph
//! outlines. A click selects it (the selection chrome of `drawObjectChrome`), a drag moves it, the handles
//! resize it (the formula is then scaled to its box, as on the web), the knob turns it, a double click opens
//! the equation dialog, Delete removes it.
//!
//! Self-contained on purpose: when the generic floating-object layer of the grid lands, it calls
//! [`paint_equation`] for the `equations` kind and [`natural_size`] for an equation's natural box, and the
//! selection/drag code here gives way to it (see `EquationLayer`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use kubuno_desktop::views::component::Control;
use kubuno_desktop::ui::graphics::{Brush, Color, FillMode, Font, FontStyle, Graphics, GraphicsPath, Pen, PointF, StringAlignment, StringFormat, StringFormatFlags, StringTrimming};
use kubuno_desktop::ui::Rect;
use kubuno_office_math_core::sheet::{self, EquationGeometry};
use kubuno_office_math_core::{fonts, Item, PathOp, Rendered};
use kubuno_office_sheets_core::editor::Editor;
use kubuno_office_sheets_core::grid::{COL_HEADER_HEIGHT, ROW_HEADER_WIDTH};
use kubuno_office_sheets_core::objects::{self as obj, Handle, ObjBox, ObjKind, ObjRect, ObjRef};
use serde_json::Value as Json;

use super::GridCanvas;

/// An equation's double click (the window opens the equation dialog).
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct EquationEventArgs {
    /// The equation's id.
    pub id: String,
    /// Its LaTeX.
    pub latex: String,
}

thread_local! {
    static RENDERED: RefCell<HashMap<String, Rc<Rendered>>> = RefCell::new(HashMap::new());
    static GLYPHS: RefCell<HashMap<(String, u16), Rc<GraphicsPath>>> = RefCell::new(HashMap::new());
}

/// The formula of `latex` laid out (cached).
pub fn rendered(latex: &str) -> Rc<Rendered> {
    RENDERED.with(|c| {
        if let Some(r) = c.borrow().get(latex) {
            return r.clone();
        }
        let r = Rc::new(kubuno_office_math_core::render(latex));
        let mut m = c.borrow_mut();
        if m.len() > 256 {
            m.clear();
        }
        m.insert(latex.to_string(), r.clone());
        r
    })
}

/// The LaTeX shown for an empty equation (`renderKatex(latex || '\\,')`).
fn source(latex: &str) -> &str {
    if latex.is_empty() {
        "\\,"
    } else {
        latex
    }
}

/// An equation's natural size at `zoom` (`eqNatRef`: zoom-1 px).
pub fn natural_size(o: &Json, zoom: f64) -> (f64, f64) {
    let latex = o.get("latex").and_then(Json::as_str).unwrap_or("");
    let g = sheet::geometry(&rendered(source(latex)), zoom);
    (g.natural_w, g.natural_h)
}

fn ops_path(ops: &[PathOp], fill_mode: FillMode) -> GraphicsPath {
    let mut p = GraphicsPath::new().with_fill_mode(fill_mode);
    for op in ops {
        match *op {
            PathOp::Move(x, y) => p.move_to(PointF::new(x as f32, y as f32)),
            PathOp::Line(x, y) => p.line_to(PointF::new(x as f32, y as f32)),
            PathOp::Quad(a, b, x, y) => p.quad_to(PointF::new(a as f32, b as f32), PointF::new(x as f32, y as f32)),
            PathOp::Cubic(a, b, c, d, x, y) => p.bezier_to(PointF::new(a as f32, b as f32), PointF::new(c as f32, d as f32), PointF::new(x as f32, y as f32)),
            PathOp::Close => p.close_figure(),
        }
    }
    p
}

fn glyph_path(font: &str, id: u16) -> Rc<GraphicsPath> {
    GLYPHS.with(|c| {
        let key = (font.to_string(), id);
        if let Some(p) = c.borrow().get(&key) {
            return p.clone();
        }
        let p = Rc::new(ops_path(&fonts::outline(font, id), FillMode::Winding));
        c.borrow_mut().insert(key, p.clone());
        p
    })
}

/// A CSS colour (KaTeX's `\color` accepts names, `#rgb`, `#rrggbb`).
pub fn css_color(s: &str) -> Option<Color> {
    let s = s.trim();
    if let Some(h) = s.strip_prefix('#') {
        let v = |i: usize, n: usize| u8::from_str_radix(&h[i..i + n], 16).ok();
        return match h.len() {
            3 | 4 => Some(Color::rgb(v(0, 1)? * 17, v(1, 1)? * 17, v(2, 1)? * 17).with_alpha(if h.len() == 4 { v(3, 1)? as f32 * 17.0 / 255.0 } else { 1.0 })),
            6 | 8 => Some(Color::rgb(v(0, 2)?, v(2, 2)?, v(4, 2)?).with_alpha(if h.len() == 8 { v(6, 2)? as f32 / 255.0 } else { 1.0 })),
            _ => None,
        };
    }
    let (r, g, b) = match s.to_ascii_lowercase().as_str() {
        "transparent" => return Some(Color::rgba_f(0.0, 0.0, 0.0, 0.0)),
        "black" => (0, 0, 0),
        "white" => (255, 255, 255),
        "red" => (255, 0, 0),
        "green" => (0, 128, 0),
        "blue" => (0, 0, 255),
        "yellow" => (255, 255, 0),
        "orange" => (255, 165, 0),
        "purple" => (128, 0, 128),
        "magenta" | "fuchsia" => (255, 0, 255),
        "cyan" | "aqua" => (0, 255, 255),
        "gray" | "grey" => (128, 128, 128),
        "brown" => (165, 42, 42),
        "pink" => (255, 192, 203),
        "violet" => (238, 130, 238),
        "teal" => (0, 128, 128),
        "olive" => (128, 128, 0),
        "navy" => (0, 0, 128),
        "maroon" => (128, 0, 0),
        "lime" => (0, 255, 0),
        "silver" => (192, 192, 192),
        "gold" => (255, 215, 0),
        "indigo" => (75, 0, 130),
        "darkblue" => (0, 0, 139),
        "darkred" => (139, 0, 0),
        "darkgreen" => (0, 100, 0),
        "lightgray" | "lightgrey" => (211, 211, 211),
        "darkgray" | "darkgrey" => (169, 169, 169),
        _ => return None,
    };
    Some(Color::rgb(r, g, b))
}

/// Paints a laid-out formula with its baseline origin at (`x`, `y`), 1 em = `em` DIP.
pub fn draw_rendered(g: &Graphics<'_>, r: &Rendered, x: f32, y: f32, em: f32, ink: Color) {
    let color = |c: &Option<String>| c.as_deref().and_then(css_color).unwrap_or(ink);
    for item in &r.items {
        match item {
            Item::Glyph { font, id, x: gx, y: gy, size, color: c, .. } => {
                let path = glyph_path(font, *id);
                let st = g.save();
                g.translate_transform(x + *gx as f32 * em, y + *gy as f32 * em);
                let s = *size as f32 * em;
                g.scale_transform(s, s);
                g.fill_path(Brush::solid(color(c)), &path);
                g.restore(st);
            }
            Item::Rect { x: rx, y: ry, w, h, color: c } => {
                // Rules are at least one device pixel, as the browser draws borders.
                let hh = (*h as f32 * em).max(1.0 / g.dpi_scale().max(0.5));
                g.fill_rectangle(Brush::solid(color(c)), Rect::new(x + *rx as f32 * em, y + *ry as f32 * em, x + (*rx + *w) as f32 * em, y + *ry as f32 * em + hh));
            }
            Item::Frame { x: fx, y: fy, w, h, thickness, color: c } => {
                let t = (*thickness as f32 * em).max(1.0);
                g.draw_rectangle(&Pen::new(color(c), t), Rect::new(x + *fx as f32 * em + t / 2.0, y + *fy as f32 * em + t / 2.0, x + (*fx + *w) as f32 * em - t / 2.0, y + (*fy + *h) as f32 * em - t / 2.0));
            }
            Item::Path { ops, clip, color: c } => {
                let st = g.save();
                if let Some((l, t, rr, b)) = clip {
                    g.intersect_clip(Rect::new(x + *l as f32 * em, y + *t as f32 * em, x + *rr as f32 * em, y + *b as f32 * em));
                }
                g.translate_transform(x, y);
                g.scale_transform(em, em);
                g.fill_path(Brush::solid(color(c)), &ops_path(ops, FillMode::Winding));
                g.restore(st);
            }
            Item::Text { text, x: tx, y: ty, size, color: c } => {
                // Text the KaTeX fonts do not cover: the browser's serif fallback.
                let px = (*size as f32 * em * kubuno_office_math_core::KATEX_EM as f32).max(1.0);
                let font = Font::with_dip("Times New Roman", px, FontStyle::REGULAR);
                let fmt = StringFormat { alignment: StringAlignment::Near, line_alignment: StringAlignment::Far, trimming: StringTrimming::None, flags: StringFormatFlags::NO_WRAP | StringFormatFlags::NO_CLIP };
                let left = x + *tx as f32 * em;
                let base = y + *ty as f32 * em;
                g.draw_string(text, &font, Brush::solid(color(c)), Rect::new(left, base - px * 1.2, left + 10_000.0, base + px * 0.25), &fmt);
            }
        }
    }
}

/// Paints one equation object in its content rect `r` (DIP, already scrolled into the control), as the web's
/// overlay: the box, then the formula scaled to the box when it was resized.
pub fn paint_equation(g: &Graphics<'_>, r: &ObjRect, o: &Json, zoom: f64) {
    let latex = o.get("latex").and_then(Json::as_str).unwrap_or("");
    let rend = rendered(source(latex));
    let geo: EquationGeometry = sheet::geometry(&rend, zoom);
    let sized = o.get("bw").and_then(Json::as_f64).is_some() || o.get("bh").and_then(Json::as_f64).is_some();
    let (bw, bh) = if sized { (r.w, r.h) } else { (geo.auto_w, geo.auto_h) };
    let (sx, sy) = if sized {
        let sx = o.get("bw").and_then(Json::as_f64).map(|w| w / geo.natural_w).unwrap_or(1.0);
        let sy = o.get("bh").and_then(Json::as_f64).map(|h| h / geo.natural_h).unwrap_or(1.0);
        (sx, sy)
    } else {
        (1.0, 1.0)
    };
    let (x, y) = (r.x as f32, r.y as f32);
    let st = g.save();
    if r.rot != 0.0 {
        g.rotate_transform_at(r.rot as f32, PointF::new(x + bw as f32 / 2.0, y + bh as f32 / 2.0));
    }
    let bx = Rect::new(x, y, x + bw as f32, y + bh as f32);
    // box-shadow: 0 1px 3px rgba(0,0,0,0.08)
    for (dy, a) in [(1.0f32, 0.05f32), (2.0, 0.03)] {
        g.fill_rounded_rectangle(Brush::solid(Color::rgba_f(0.0, 0.0, 0.0, a)), Rect::new(bx.left, bx.top + dy, bx.right, bx.bottom + dy), 4.0);
    }
    g.fill_rounded_rectangle(Brush::solid(Color::WHITE), bx, 4.0);
    g.draw_rounded_rectangle(&Pen::new(Color::from_hex("#e2e4e6").unwrap_or(Color::GRAY), 1.0), Rect::new(bx.left + 0.5, bx.top + 0.5, bx.right - 0.5, bx.bottom - 0.5), 4.0);
    // overflow: hidden (inside the border).
    g.intersect_clip(Rect::new(bx.left + 1.0, bx.top + 1.0, bx.right - 1.0, bx.bottom - 1.0));
    // The inner node (transform-origin 0 0) starts at the padding, below the display block's margin.
    let ox = x + geo.content_x as f32;
    let oy = y + geo.content_y as f32;
    g.translate_transform(ox, oy);
    g.scale_transform(sx as f32, sy as f32);
    let ink = Color::from_hex("#202124").unwrap_or(Color::BLACK);
    if rend.error.is_some() {
        draw_rendered(g, &rend, 0.0, geo.html_top as f32, (geo.em_px / kubuno_office_math_core::KATEX_EM * kubuno_office_math_core::KATEX_EM) as f32, ink);
    } else {
        draw_rendered(g, &rend, 0.0, geo.html_top as f32, geo.em_px as f32, Color::BLACK);
    }
    g.restore(st);
}

/// The selection chrome of an object (`drawObjectChrome`): outline, eight handles, rotation arm and knob.
pub fn paint_chrome(g: &Graphics<'_>, r: &ObjRect, min_top: f64) {
    let accent = Color::from_hex("#1a73e8").unwrap_or(Color::BLUE);
    let (x, y, w, h) = (r.x as f32, r.y as f32, r.w as f32, r.h as f32);
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let gap = obj::CHROME_GAP as f32;
    let (_, ky, below) = obj::knob_point(r, min_top);
    let st = g.save();
    if r.rot != 0.0 {
        g.rotate_transform_at(r.rot as f32, PointF::new(cx, cy));
    }
    let pen = Pen::new(accent, obj::CHROME_BORDER as f32);
    g.draw_rectangle(&pen, Rect::new(x - gap, y - gap, x + w + gap, y + h + gap));
    let arm_from = if below { y + h + gap } else { y - gap };
    let ky = ky as f32;
    g.draw_line(&pen, PointF::new(cx, arm_from), PointF::new(cx, ky));
    let kr = obj::ROTATE_KNOB_R as f32;
    let knob = Rect::new(cx - kr, ky - kr, cx + kr, ky + kr);
    g.fill_ellipse(Brush::solid(Color::WHITE), knob);
    g.draw_ellipse(&pen, knob);
    // The circular arrow inside the knob.
    let gr = 5.0f32;
    let (a0, a1) = (-std::f32::consts::PI * 0.6, std::f32::consts::PI * 0.95);
    let thin = Pen::new(accent, 1.5);
    g.draw_arc(&thin, Rect::new(cx - gr, ky - gr, cx + gr, ky + gr), a0.to_degrees(), (a1 - a0).to_degrees());
    let (px, py) = (cx + gr * a0.cos(), ky + gr * a0.sin());
    let (tx, ty) = (a0.sin(), -a0.cos());
    let (nx, ny) = (-ty, tx);
    g.fill_polygon(Brush::solid(accent), &[PointF::new(px + tx * 4.2, py + ty * 4.2), PointF::new(px + nx * 2.6, py + ny * 2.6), PointF::new(px - nx * 2.6, py - ny * 2.6)]);
    for (hd, hx, hy) in obj::handle_points(r) {
        let (ex, ey) = obj::handle_extent(hd);
        let (hx, hy, ex, ey) = (hx as f32, hy as f32, ex as f32, ey as f32);
        let rr = Rect::new(hx - ex, hy - ey, hx + ex, hy + ey);
        if hd.is_edge() {
            g.fill_rounded_rectangle(Brush::solid(Color::WHITE), rr, ex.min(ey));
            g.draw_rounded_rectangle(&pen, rr, ex.min(ey));
        } else {
            g.fill_ellipse(Brush::solid(Color::WHITE), rr);
            g.draw_ellipse(&pen, rr);
        }
    }
    g.restore(st);
}

/// A gesture on an equation.
#[derive(Clone, Debug, PartialEq)]
struct EqDrag {
    id: String,
    handle: Handle,
    start: (f64, f64),
    start_box: ObjBox,
    rot: f64,
    moved: bool,
}

/// The equations' selection and gesture state (the generic object layer replaces it when it lands).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EquationLayer {
    /// The selected equation.
    pub selected: Option<String>,
    drag: Option<EqDrag>,
}

fn equation_rect(ed: &Editor, o: &Json) -> ObjRect {
    let geom = ed.geometry();
    let natural = natural_size(o, ed.zoom);
    obj::rect_of(ObjKind::Equation, o, &geom, Some(natural))
}

impl GridCanvas {
    /// A content point (the sheet's unscrolled coordinates) of a control point.
    fn eq_content_point(&self, x: f64, y: f64) -> (f64, f64) {
        (x + self.view.scroll_x, y + self.view.scroll_y)
    }

    /// The equations in paint order with their content rects.
    fn eq_rects(&self) -> Vec<(String, ObjRect, Json)> {
        let Some(ed) = self.ed.as_ref() else { return vec![] };
        ed.objects(ObjKind::Equation)
            .iter()
            .filter_map(|o| {
                let id = o.get("id").and_then(Json::as_str)?.to_string();
                Some((id, equation_rect(ed, o), o.clone()))
            })
            .collect()
    }

    /// Paints the equations above the cells (the web's object layer, `z-index: 7`), clipped to the cell area.
    pub(super) fn paint_equations(&self, g: &Graphics<'_>) {
        let Some(ed) = self.ed.as_ref() else { return };
        let rects = self.eq_rects();
        if rects.is_empty() {
            return;
        }
        let (ox, oy) = (self.bounds.left - self.view.scroll_x as f32, self.bounds.top - self.view.scroll_y as f32);
        let st = g.save();
        g.intersect_clip(Rect::new(self.bounds.left + ROW_HEADER_WIDTH as f32, self.bounds.top + COL_HEADER_HEIGHT as f32, self.bounds.right, self.bounds.bottom));
        for (id, r, o) in &rects {
            let sr = ObjRect { x: r.x + ox as f64, y: r.y + oy as f64, ..*r };
            paint_equation(g, &sr, o, ed.zoom);
            if self.eq.selected.as_deref() == Some(id.as_str()) {
                paint_chrome(g, &sr, oy as f64 + COL_HEADER_HEIGHT);
            }
        }
        g.restore(st);
    }

    /// A left press: selects an equation and starts its gesture. `true` when an equation took the press.
    pub(super) fn equations_mouse_down(&mut self, x: f64, y: f64) -> bool {
        if x < ROW_HEADER_WIDTH || y < COL_HEADER_HEIGHT {
            return false;
        }
        let (px, py) = self.eq_content_point(x, y);
        let selected = self.eq.selected.clone();
        let hit = self.eq_rects().into_iter().rev().find_map(|(id, r, o)| {
            let sel = selected.as_deref() == Some(id.as_str());
            obj::hit_object(&r, px, py, sel, COL_HEADER_HEIGHT + self.view.scroll_y).map(|h| (id, h, r, o))
        });
        match hit {
            Some((id, handle, r, _)) => {
                let zoom = self.ed.as_ref().map(|e| e.zoom).unwrap_or(1.0);
                let start_box = obj::rect_to_box(&r, zoom);
                self.eq.selected = Some(id.clone());
                self.eq.drag = Some(EqDrag { id, handle, start: (px, py), start_box, rot: r.rot, moved: false });
                self.invalidate();
                true
            }
            None => {
                if self.eq.selected.take().is_some() {
                    self.invalidate();
                }
                false
            }
        }
    }

    /// A move while an equation gesture runs: previews the new box. `true` while one runs.
    pub(super) fn equations_mouse_move(&mut self, x: f64, y: f64, shift: bool) -> bool {
        let Some(mut d) = self.eq.drag.clone() else { return false };
        let (px, py) = self.eq_content_point(x, y);
        if !d.moved && (px - d.start.0).hypot(py - d.start.1) < 1.0 {
            return true;
        }
        d.moved = true;
        let zoom = self.ed.as_ref().map(|e| e.zoom).unwrap_or(1.0);
        let r = ObjRef::new(ObjKind::Equation, d.id.clone());
        let (b, rot) = if d.handle == Handle::Rotate {
            let cx = ROW_HEADER_WIDTH + (d.start_box.bx + d.start_box.bw / 2.0) * zoom;
            let cy = COL_HEADER_HEIGHT + (d.start_box.by + d.start_box.bh / 2.0) * zoom;
            (d.start_box, obj::rotation_for(cx, cy, px, py, shift))
        } else {
            (obj::apply_drag(d.start_box, d.handle, px - d.start.0, py - d.start.1, d.rot, zoom), d.rot)
        };
        if let Some(ed) = self.ed.as_mut() {
            // Live: the object follows the pointer; the session saves the final state on release.
            ed.set_object_box(&r, b, Some(rot));
        }
        self.eq.drag = Some(d);
        self.invalidate();
        true
    }

    /// The release of an equation gesture. `true` when one ended.
    pub(super) fn equations_mouse_up(&mut self) -> bool {
        let Some(d) = self.eq.drag.take() else { return false };
        if !d.moved {
            return true;
        }
        self.invalidate();
        true
    }

    /// A double click on an equation: its id and LaTeX for the dialog.
    pub(super) fn equation_at(&self, x: f64, y: f64) -> Option<(String, String)> {
        if x < ROW_HEADER_WIDTH || y < COL_HEADER_HEIGHT {
            return None;
        }
        let (px, py) = self.eq_content_point(x, y);
        self.eq_rects().into_iter().rev().find_map(|(id, r, o)| {
            obj::hit_object(&r, px, py, false, COL_HEADER_HEIGHT).map(|_| (id, o.get("latex").and_then(Json::as_str).unwrap_or("").to_string()))
        })
    }

    /// The selected equation removed (Delete).
    pub(super) fn delete_selected_equation(&mut self) -> bool {
        let Some(id) = self.eq.selected.take() else { return false };
        if let Some(ed) = self.ed.as_mut() {
            ed.delete_objects(&[ObjRef::new(ObjKind::Equation, id)]);
        }
        self.invalidate();
        true
    }

    /// The cursor over an equation (move, resize along the handle, grab over the knob).
    pub(super) fn equation_cursor(&self, x: f64, y: f64) -> Option<kubuno_desktop::controls::host::Cursor> {
        use kubuno_desktop::controls::host::Cursor;
        if x < ROW_HEADER_WIDTH || y < COL_HEADER_HEIGHT {
            return None;
        }
        let (px, py) = self.eq_content_point(x, y);
        let sel = self.eq.selected.clone();
        let (h, rot) = self.eq_rects().into_iter().rev().find_map(|(id, r, _)| obj::hit_object(&r, px, py, sel.as_deref() == Some(id.as_str()), COL_HEADER_HEIGHT).map(|h| (h, r.rot)))?;
        Some(match obj::cursor_for(h, rot) {
            obj::Cursor::Move => Cursor::Move,
            obj::Cursor::Grab => Cursor::Hand,
            obj::Cursor::NS => Cursor::ResizeNS,
            obj::Cursor::EW => Cursor::ResizeEW,
            obj::Cursor::NESW => Cursor::ResizeNESW,
            obj::Cursor::NWSE => Cursor::ResizeNWSE,
        })
    }

    /// Inserts an equation at the active cell and selects it; returns its id.
    pub fn insert_equation(&mut self, latex: &str) -> Option<String> {
        let id = kubuno_office_desktop_common::api::client::fresh_uuid_v4();
        let r = self.ed.as_mut()?.insert_equation(latex, id.clone());
        self.eq.selected = Some(r.id.clone());
        self.invalidate();
        Some(id)
    }

    /// Replaces an equation's LaTeX.
    pub fn set_equation_latex(&mut self, id: &str, latex: &str) {
        if let Some(ed) = self.ed.as_mut() {
            ed.set_equation_latex(id, latex);
        }
        self.invalidate();
    }

    /// Selects an equation (none with `None`).
    pub fn select_equation(&mut self, id: Option<String>) {
        self.eq.selected = id;
        self.invalidate();
    }

    /// The selected equation.
    pub fn selected_equation(&self) -> Option<String> {
        self.eq.selected.clone()
    }
}
