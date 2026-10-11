//! Painting a laid-out LaTeX formula to the framework's `Graphics`, from the math core's display list
//! (`kubuno-office-math-core`, a Rust port of KaTeX). Vendored from Kubuno Spreadsheets'
//! `controls/grid/equations.rs` (the rendering half only) — cross-crate imports between office apps are not
//! allowed, so the shared painter is copied, not depended on.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use kubuno_desktop::ui::graphics::{Brush, Color, FillMode, Font, FontStyle, Graphics, GraphicsPath, Pen, PointF, StringAlignment, StringFormat, StringFormatFlags, StringTrimming};
use kubuno_desktop::ui::Rect;
use kubuno_office_math_core::{fonts, Item, PathOp, Rendered};

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
        "gray" | "grey" => (128, 128, 128),
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

/// The natural size of a laid-out formula in DIP at 1 em = `em`.
pub fn rendered_size(r: &Rendered, em: f32) -> (f32, f32) {
    (r.width as f32 * em, (r.height + r.depth) as f32 * em)
}
