//! Painting the scene - a port of the renderers in `office/web/src/whiteboard-engine.ts` onto a
//! renderer-neutral [`Surface`] (the web's `CanvasRenderingContext2D`). Each platform implements `Surface`
//! with its own drawing API (Direct2D on Windows, a recorder in the tests); the geometry and the order are
//! the web's. Painters receive content-space coordinates and assume the caller has applied the viewport
//! transform for elements (`viewport.apply(ctx)`), exactly as `WhiteboardApp` does.

use kubuno_office_shapes_core::path::{shade_colour, Cmd};

use crate::engine::{handle_points, shape_geometry, ShapeGeometry};
use crate::model::{ArrowElement, FrameElement, ShapeElement, StickyNote, Stroke, TextBox, WbElement};
use crate::viewport::Viewport;

/// Sticky-note fill colours (`STICKY_COLORS`, `whiteboard-types.ts:125-136`).
pub fn sticky_color(key: &str) -> &str {
    match key {
        "yellow" => "#FFF9C4",
        "green" => "#C8E6C9",
        "blue" => "#BBDEFB",
        "pink" => "#F8BBD9",
        "orange" => "#FFE0B2",
        "purple" => "#E1BEE7",
        "red" => "#FFCDD2",
        "teal" => "#B2EBF2",
        "white" => "#FFFFFF",
        "dark" => "#37474F",
        other => other,
    }
}

/// `textBaseline` values the painters use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextBaseline {
    Top,
    Bottom,
}

/// `textAlign` values the painters use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

impl TextAlign {
    pub fn parse(s: &str) -> TextAlign {
        match s {
            "center" => TextAlign::Center,
            "right" => TextAlign::Right,
            _ => TextAlign::Left,
        }
    }
}

/// The renderer-neutral drawing surface: the subset of `CanvasRenderingContext2D` the board painters use.
/// A platform implements it; the core never allocates a surface.
pub trait Surface {
    fn save(&mut self);
    fn restore(&mut self);

    /// Replaces the current transform (`setTransform`), used to apply the viewport.
    fn set_transform(&mut self, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64);
    fn translate(&mut self, x: f64, y: f64);
    fn rotate(&mut self, radians: f64);

    fn set_fill(&mut self, css: &str);
    fn set_stroke(&mut self, css: &str);
    fn set_line_width(&mut self, w: f64);
    fn set_line_dash(&mut self, segments: &[f64]);
    fn set_global_alpha(&mut self, a: f64);
    /// A drop shadow (the sticky note's); `clear_shadow` turns it off.
    fn set_shadow(&mut self, css: &str, blur: f64, offset_y: f64);
    fn clear_shadow(&mut self);

    /// Sets the font for the next `fill_text`/`measure_text` (pixels, bold, family).
    fn set_font(&mut self, px: f64, bold: bool, family: &str);
    fn set_text_baseline(&mut self, b: TextBaseline);
    fn set_text_align(&mut self, a: TextAlign);

    fn begin_path(&mut self);
    fn move_to(&mut self, x: f64, y: f64);
    fn line_to(&mut self, x: f64, y: f64);
    fn bezier_curve_to(&mut self, c1x: f64, c1y: f64, c2x: f64, c2y: f64, x: f64, y: f64);
    fn quadratic_curve_to(&mut self, cx: f64, cy: f64, x: f64, y: f64);
    /// An SVG elliptical arc segment (`A rx,ry 0 large sweep x,y`), the form `kubuno-office-shapes-core`
    /// emits; the platform converts it to its own arc call.
    fn arc_svg(&mut self, rx: f64, ry: f64, large: bool, sweep: bool, x: f64, y: f64);
    /// A full circle/ellipse arc (`ctx.arc`), used by dot backgrounds and knobs.
    fn arc(&mut self, cx: f64, cy: f64, r: f64, start: f64, end: f64);
    fn round_rect(&mut self, x: f64, y: f64, w: f64, h: f64, radius: f64);
    fn close_path(&mut self);
    fn fill(&mut self);
    fn stroke(&mut self);

    fn fill_rect(&mut self, x: f64, y: f64, w: f64, h: f64);
    fn stroke_rect(&mut self, x: f64, y: f64, w: f64, h: f64);
    fn fill_text(&mut self, text: &str, x: f64, y: f64);
    /// The advance width of `text` in the current font (`measureText(text).width`).
    fn measure_text(&mut self, text: &str) -> f64;
}

/// Word-wraps `text` to `max_width`, breaking on spaces then overflowing a single word
/// (`wrapText`, `whiteboard-engine.ts`).
pub fn wrap_text(surface: &mut dyn Surface, text: &str, max_width: f64) -> Vec<String> {
    let mut lines = Vec::new();
    for para in text.split('\n') {
        if surface.measure_text(para) <= max_width {
            lines.push(para.to_string());
            continue;
        }
        let mut cur = String::new();
        for w in para.split(' ') {
            let test = if cur.is_empty() { w.to_string() } else { format!("{cur} {w}") };
            if surface.measure_text(&test) <= max_width {
                cur = test;
            } else {
                if !cur.is_empty() {
                    lines.push(cur.clone());
                }
                cur = w.to_string();
            }
        }
        if !cur.is_empty() {
            lines.push(cur);
        }
    }
    lines
}

/// Paints the canvas background (`renderBackground`): grid, dots or lines. The caller passes the viewport
/// and the viewport size in screen px. `white` paints nothing.
pub fn render_background(surface: &mut dyn Surface, kind: &str, viewport: &Viewport, cw: f64, ch: f64) {
    if kind == "white" {
        return;
    }
    const GRID: f64 = 24.0;
    let zoom = viewport.scale();
    let bounds = viewport.bounds(cw, ch);
    let sx = (bounds.x / GRID).floor() * GRID;
    let sy = (bounds.y / GRID).floor() * GRID;

    surface.save();
    surface.set_transform(zoom, 0.0, 0.0, zoom, viewport.tx(), viewport.ty());
    surface.set_fill("#b8bcc8");
    surface.set_stroke("#e0e0e0");
    surface.set_line_width(0.5 / zoom);

    let right = bounds.x + bounds.w + GRID;
    let bottom = bounds.y + bounds.h + GRID;
    if kind == "dots" {
        let r = (1.2 / zoom).max(0.5);
        let mut x = sx;
        while x < right {
            let mut y = sy;
            while y < bottom {
                surface.begin_path();
                surface.arc(x, y, r, 0.0, std::f64::consts::TAU);
                surface.fill();
                y += GRID;
            }
            x += GRID;
        }
    } else if kind == "grid" {
        let mut x = sx;
        while x < right {
            surface.begin_path();
            surface.move_to(x, bounds.y);
            surface.line_to(x, bounds.y + bounds.h);
            surface.stroke();
            x += GRID;
        }
        let mut y = sy;
        while y < bottom {
            surface.begin_path();
            surface.move_to(bounds.x, y);
            surface.line_to(bounds.x + bounds.w, y);
            surface.stroke();
            y += GRID;
        }
    } else if kind == "lines" {
        let mut y = sy;
        while y < bottom {
            surface.begin_path();
            surface.move_to(bounds.x, y);
            surface.line_to(bounds.x + bounds.w, y);
            surface.stroke();
            y += GRID;
        }
    }
    surface.restore();
}

/// Dispatches to the painter for an element. Freehand strokes are painted separately with
/// [`render_stroke`]. The caller paints elements in `zIndex` order.
pub fn render_element(surface: &mut dyn Surface, el: &WbElement, elements: &[WbElement], zoom: f64) {
    match el {
        WbElement::Sticky(n) => render_sticky_note(surface, n, zoom),
        WbElement::Text(t) => render_text_box(surface, t, zoom),
        WbElement::Shape(s) => render_shape(surface, s),
        WbElement::Arrow(a) => render_arrow(surface, a, elements),
        WbElement::Frame(f) => render_frame(surface, f, zoom),
        WbElement::Image(_) => {}
    }
}

/// `renderStickyNote`.
pub fn render_sticky_note(surface: &mut dyn Surface, note: &StickyNote, zoom: f64) {
    let (x, y, w, h) = (note.base.x, note.base.y, note.base.width, note.base.height);
    const FOLD: f64 = 14.0;
    const PAD: f64 = 10.0;

    surface.save();
    surface.set_shadow("rgba(0,0,0,0.15)", 6.0, 2.0);
    surface.set_fill(sticky_color(&note.color));
    surface.begin_path();
    surface.move_to(x, y);
    surface.line_to(x + w - FOLD, y);
    surface.line_to(x + w, y + FOLD);
    surface.line_to(x + w, y + h);
    surface.line_to(x, y + h);
    surface.close_path();
    surface.fill();
    surface.restore();

    // Folded corner.
    surface.save();
    surface.clear_shadow();
    surface.set_fill("rgba(0,0,0,0.1)");
    surface.begin_path();
    surface.move_to(x + w - FOLD, y);
    surface.line_to(x + w, y + FOLD);
    surface.line_to(x + w - FOLD, y + FOLD);
    surface.close_path();
    surface.fill();
    surface.restore();

    if note.text.is_empty() {
        return;
    }
    let efs = note.font_size.max(7.0 / zoom);
    surface.save();
    surface.set_fill(if note.color == "dark" { "#ffffff" } else { "#1a1a1a" });
    surface.set_font(efs, false, "Arial, sans-serif");
    surface.set_text_baseline(TextBaseline::Top);
    let lines = wrap_text(surface, &note.text, w - PAD * 2.0);
    let lh = efs * 1.4;
    for (i, line) in lines.iter().enumerate() {
        let ly = y + PAD + i as f64 * lh;
        if ly + lh > y + h - PAD {
            break;
        }
        surface.fill_text(line, x + PAD, ly);
    }
    surface.restore();
}

/// `renderTextBox`.
pub fn render_text_box(surface: &mut dyn Surface, tb: &TextBox, zoom: f64) {
    if tb.text.is_empty() {
        return;
    }
    let efs = tb.font_size.max(6.0 / zoom);
    surface.save();
    surface.set_fill(if tb.color.is_empty() { "#202124" } else { &tb.color });
    surface.set_font(efs, tb.font_weight == "bold", "Arial, sans-serif");
    surface.set_text_baseline(TextBaseline::Top);
    let align = TextAlign::parse(&tb.text_align);
    surface.set_text_align(align);
    let lines = wrap_text(surface, &tb.text, tb.base.width);
    let lh = efs * 1.4;
    let ox = match align {
        TextAlign::Center => tb.base.x + tb.base.width / 2.0,
        TextAlign::Right => tb.base.x + tb.base.width,
        TextAlign::Left => tb.base.x,
    };
    for (i, line) in lines.iter().enumerate() {
        surface.fill_text(line, ox, tb.base.y + i as f64 * lh);
    }
    surface.restore();
}

/// `renderShape` → `paintBoardShape`: the board's rounded `rect`, the shared geometry, or an open
/// line/connector. The caller has translated/rotated into the shape's frame when it is rotated.
pub fn render_shape(surface: &mut dyn Surface, shape: &ShapeElement) {
    let (x, y, w, h) = (shape.base.x, shape.base.y, shape.base.width, shape.base.height);
    let sw = shape.stroke_width;
    let fill = if shape.fill.is_empty() { "#BBDEFB" } else { &shape.fill };
    let stroke = if shape.stroke.is_empty() { "#1a73e8" } else { &shape.stroke };

    surface.save();
    surface.set_fill(fill);
    surface.set_stroke(stroke);
    surface.set_line_width(sw);

    match shape_geometry(&shape.kind, w, h, shape.adj.as_deref(), sw > 0.0, sw) {
        ShapeGeometry::BoardRect { radius } => {
            surface.begin_path();
            surface.round_rect(x, y, w, h, radius);
            surface.fill();
            if sw > 0.0 {
                surface.stroke();
            }
        }
        ShapeGeometry::Paths(subs) => {
            for sub in subs {
                surface.begin_path();
                emit_path(surface, &sub.path.cmds, x, y);
                if sub.fill {
                    if (sub.shade - 1.0).abs() > f64::EPSILON {
                        surface.set_fill(&shade_colour(fill, sub.shade));
                    } else {
                        surface.set_fill(fill);
                    }
                    surface.fill();
                }
                if sub.stroke && sw > 0.0 {
                    surface.stroke();
                }
            }
        }
        ShapeGeometry::Open => paint_open_shape(surface, &shape.kind, x, y, w, h, sw),
    }
    surface.restore();
}

/// Emits a shape's path commands, offset to the element's `(ox, oy)` in content space.
fn emit_path(surface: &mut dyn Surface, cmds: &[Cmd], ox: f64, oy: f64) {
    for cmd in cmds {
        match *cmd {
            Cmd::MoveTo(x, y) => surface.move_to(ox + x, oy + y),
            Cmd::LineTo(x, y) => surface.line_to(ox + x, oy + y),
            Cmd::CubicTo(c1x, c1y, c2x, c2y, x, y) => surface.bezier_curve_to(ox + c1x, oy + c1y, ox + c2x, oy + c2y, ox + x, oy + y),
            Cmd::QuadTo(cx, cy, x, y) => surface.quadratic_curve_to(ox + cx, oy + cy, ox + x, oy + y),
            Cmd::ArcTo { rx, ry, large, sweep, x, y } => surface.arc_svg(rx, ry, large, sweep, ox + x, oy + y),
            Cmd::Close => surface.close_path(),
        }
    }
}

/// A line/connector head anchor: `(x, y, toward-x, toward-y)` for one end, and the pair for both ends.
type OpenAnchors = ((f64, f64, f64, f64), (f64, f64, f64, f64));

/// The ten « Traits » of the catalogue that have no area geometry (`paintOpenShape`): a straight line, an
/// elbow or a curve, with arrow heads at one or both ends.
pub fn paint_open_shape(surface: &mut dyn Surface, kind: &str, x: f64, y: f64, w: f64, h: f64, stroke_width: f64) {
    let (l, t, r, b, cx) = (x, y, x + w, y + h, x + w / 2.0);
    let lw = stroke_width.max(1.5).max((w.min(h) * 0.045).min(3.5));
    surface.save();
    surface.set_line_width(lw);

    // The open path, mirroring `openPathFor` (bottom-left → top-right, `curve` resting on the bottom). Each
    // arm returns `(start, end)` as `(x, y, toward-x, toward-y)` giving each head's direction.
    let head_end = matches!(kind, "lineArrow" | "lineDouble" | "elbowArrow" | "elbowDoubleArrow" | "curveArrow" | "curveDoubleArrow");
    let head_start = matches!(kind, "lineDouble" | "elbowDoubleArrow" | "curveDoubleArrow");

    surface.begin_path();
    let anchors: Option<OpenAnchors> = match kind {
        "line" | "lineArrow" | "lineDouble" => {
            surface.move_to(l, b);
            surface.line_to(r, t);
            Some(((l, b, r, t), (r, t, l, b)))
        }
        "elbowConnector" | "elbowArrow" | "elbowDoubleArrow" => {
            surface.move_to(l, b);
            surface.line_to(cx, b);
            surface.line_to(cx, t);
            surface.line_to(r, t);
            Some(((l, b, cx, b), (r, t, cx, t)))
        }
        "curveConnector" | "curveArrow" | "curveDoubleArrow" => {
            surface.move_to(l, b);
            surface.bezier_curve_to(x + w * 0.1, y + h * 0.15, x + w * 0.9, y + h * 0.85, r, t);
            Some(((l, b, x + w * 0.1, y + h * 0.15), (r, t, x + w * 0.9, y + h * 0.85)))
        }
        "curve" => {
            surface.move_to(l, b);
            surface.quadratic_curve_to(cx, t, r, b);
            Some(((l, b, cx, t), (r, b, cx, t)))
        }
        _ => None,
    };
    let (start, end) = match anchors {
        Some(a) => a,
        None => {
            // No geometry: show the dashed box rather than nothing.
            surface.set_line_dash(&[6.0, 4.0]);
            surface.stroke_rect(x, y, w, h);
            surface.restore();
            return;
        }
    };
    surface.stroke();

    let head = (lw * 3.2).max(6.0);
    if head_end {
        arrow_head(surface, end.0, end.1, end.2, end.3, head);
    }
    if head_start {
        arrow_head(surface, start.0, start.1, start.2, start.3, head);
    }
    surface.restore();
}

/// A filled triangular head at `(tx, ty)`, pointing away from `(ax, ay)` (`arrowHead`).
fn arrow_head(surface: &mut dyn Surface, tx: f64, ty: f64, ax: f64, ay: f64, size: f64) {
    let a = (ty - ay).atan2(tx - ax);
    let sp = std::f64::consts::PI / 7.0;
    surface.begin_path();
    surface.move_to(tx, ty);
    surface.line_to(tx - size * (a - sp).cos(), ty - size * (a - sp).sin());
    surface.line_to(tx - size * (a + sp).cos(), ty - size * (a + sp).sin());
    surface.close_path();
    surface.fill();
}

/// `renderArrow`. Arrows bound to an element are drawn from the element's centre.
pub fn render_arrow(surface: &mut dyn Surface, arrow: &ArrowElement, elements: &[WbElement]) {
    let center = |id: &str| -> Option<(f64, f64)> {
        elements.iter().find(|e| e.id() == id).and_then(|e| e.bounds()).map(|b| (b.x + b.w / 2.0, b.y + b.h / 2.0))
    };
    let start = arrow.start_element_id.as_deref().and_then(center).unwrap_or((arrow.start_x, arrow.start_y));
    let end = arrow.end_element_id.as_deref().and_then(center).unwrap_or((arrow.end_x, arrow.end_y));

    let color = if arrow.color.is_empty() { "#444746" } else { &arrow.color };
    surface.save();
    surface.set_stroke(color);
    surface.set_line_width(arrow.width);
    surface.begin_path();
    if arrow.style == "curved" {
        let dx = end.0 - start.0;
        surface.move_to(start.0, start.1);
        surface.bezier_curve_to(start.0 + dx * 0.4, start.1, end.0 - dx * 0.4, end.1, end.0, end.1);
    } else {
        surface.move_to(start.0, start.1);
        surface.line_to(end.0, end.1);
    }
    surface.stroke();

    if arrow.end_arrow != "none" {
        let angle = (end.1 - start.1).atan2(end.0 - start.0);
        let size = arrow.width * 4.0 + 6.0;
        let sp = std::f64::consts::PI / 6.0;
        surface.set_fill(color);
        surface.begin_path();
        surface.move_to(end.0, end.1);
        surface.line_to(end.0 - size * (angle - sp).cos(), end.1 - size * (angle - sp).sin());
        surface.line_to(end.0 - size * (angle + sp).cos(), end.1 - size * (angle + sp).sin());
        surface.close_path();
        surface.fill();
    }
    surface.restore();
}

/// `renderFrame`.
pub fn render_frame(surface: &mut dyn Surface, frame: &FrameElement, zoom: f64) {
    let (x, y, w, h) = (frame.base.x, frame.base.y, frame.base.width, frame.base.height);
    let color = if frame.color.is_empty() { "#1a73e8" } else { &frame.color };
    surface.save();
    surface.set_stroke(color);
    surface.set_line_width((2.0 / zoom).max(1.5));
    surface.set_line_dash(&[8.0 / zoom, 4.0 / zoom]);
    surface.stroke_rect(x, y, w, h);
    surface.set_line_dash(&[]);
    let fs = (13.0 / zoom).max(10.0);
    surface.set_fill(color);
    surface.set_font(fs, true, "Arial, sans-serif");
    surface.set_text_baseline(TextBaseline::Bottom);
    surface.fill_text(&frame.title, x + 4.0 / zoom, y - 4.0 / zoom);
    surface.restore();
}

/// `renderStroke`.
pub fn render_stroke(surface: &mut dyn Surface, stroke: &Stroke) {
    let pts = &stroke.points;
    if pts.len() < 4 {
        return;
    }
    surface.save();
    surface.set_stroke(if stroke.color.is_empty() { "#202124" } else { &stroke.color });
    surface.set_line_width(stroke.width);
    surface.set_global_alpha(stroke.opacity);
    surface.begin_path();
    surface.move_to(pts[0], pts[1]);
    let mut i = 2;
    while i + 1 < pts.len() {
        surface.line_to(pts[i], pts[i + 1]);
        i += 2;
    }
    surface.stroke();
    surface.restore();
}

/// `renderSelectionHandles`: the blue dashed box and its eight white squares around a selected element.
pub fn render_selection_handles(surface: &mut dyn Surface, el: &WbElement, zoom: f64) {
    let pts = match handle_points(el, zoom) {
        Some(p) => p,
        None => return,
    };
    let b = match el.bounds() {
        Some(b) => b,
        None => return,
    };
    let pad = 4.0 / zoom;
    let hs = 7.0 / zoom;
    surface.save();
    surface.set_stroke("#1a73e8");
    surface.set_line_width(1.5 / zoom);
    surface.set_line_dash(&[4.0 / zoom, 2.0 / zoom]);
    surface.stroke_rect(b.x - pad, b.y - pad, b.w + pad * 2.0, b.h + pad * 2.0);
    surface.set_line_dash(&[]);
    surface.set_fill("#ffffff");
    surface.set_stroke("#1a73e8");
    surface.set_line_width(1.5 / zoom);
    for (_, hx, hy) in pts {
        surface.fill_rect(hx - hs / 2.0, hy - hs / 2.0, hs, hs);
        surface.stroke_rect(hx - hs / 2.0, hy - hs / 2.0, hs, hs);
    }
    surface.restore();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A recording surface: it logs the high-level calls so a test can assert the painter's order and
    /// geometry without a real canvas.
    #[derive(Default)]
    struct Recorder {
        log: Vec<String>,
        font_px: f64,
    }

    impl Surface for Recorder {
        fn save(&mut self) {
            self.log.push("save".into());
        }
        fn restore(&mut self) {
            self.log.push("restore".into());
        }
        fn set_transform(&mut self, a: f64, _b: f64, _c: f64, _d: f64, e: f64, f: f64) {
            self.log.push(format!("xform {a} {e} {f}"));
        }
        fn translate(&mut self, x: f64, y: f64) {
            self.log.push(format!("translate {x} {y}"));
        }
        fn rotate(&mut self, r: f64) {
            self.log.push(format!("rotate {r}"));
        }
        fn set_fill(&mut self, css: &str) {
            self.log.push(format!("fillStyle {css}"));
        }
        fn set_stroke(&mut self, css: &str) {
            self.log.push(format!("strokeStyle {css}"));
        }
        fn set_line_width(&mut self, w: f64) {
            self.log.push(format!("lineWidth {w}"));
        }
        fn set_line_dash(&mut self, s: &[f64]) {
            self.log.push(format!("dash {s:?}"));
        }
        fn set_global_alpha(&mut self, a: f64) {
            self.log.push(format!("alpha {a}"));
        }
        fn set_shadow(&mut self, css: &str, blur: f64, oy: f64) {
            self.log.push(format!("shadow {css} {blur} {oy}"));
        }
        fn clear_shadow(&mut self) {
            self.log.push("noshadow".into());
        }
        fn set_font(&mut self, px: f64, bold: bool, _family: &str) {
            self.font_px = px;
            self.log.push(format!("font {px} {bold}"));
        }
        fn set_text_baseline(&mut self, _b: TextBaseline) {}
        fn set_text_align(&mut self, _a: TextAlign) {}
        fn begin_path(&mut self) {
            self.log.push("begin".into());
        }
        fn move_to(&mut self, x: f64, y: f64) {
            self.log.push(format!("M {x} {y}"));
        }
        fn line_to(&mut self, x: f64, y: f64) {
            self.log.push(format!("L {x} {y}"));
        }
        fn bezier_curve_to(&mut self, _a: f64, _b: f64, _c: f64, _d: f64, x: f64, y: f64) {
            self.log.push(format!("C {x} {y}"));
        }
        fn quadratic_curve_to(&mut self, _cx: f64, _cy: f64, x: f64, y: f64) {
            self.log.push(format!("Q {x} {y}"));
        }
        fn arc_svg(&mut self, _rx: f64, _ry: f64, _l: bool, _s: bool, x: f64, y: f64) {
            self.log.push(format!("A {x} {y}"));
        }
        fn arc(&mut self, cx: f64, cy: f64, r: f64, _s: f64, _e: f64) {
            self.log.push(format!("arc {cx} {cy} {r}"));
        }
        fn round_rect(&mut self, x: f64, y: f64, w: f64, h: f64, radius: f64) {
            self.log.push(format!("roundRect {x} {y} {w} {h} {radius}"));
        }
        fn close_path(&mut self) {
            self.log.push("close".into());
        }
        fn fill(&mut self) {
            self.log.push("fill".into());
        }
        fn stroke(&mut self) {
            self.log.push("stroke".into());
        }
        fn fill_rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
            self.log.push(format!("fillRect {x} {y} {w} {h}"));
        }
        fn stroke_rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
            self.log.push(format!("strokeRect {x} {y} {w} {h}"));
        }
        fn fill_text(&mut self, text: &str, x: f64, y: f64) {
            self.log.push(format!("text {text:?} {x} {y}"));
        }
        fn measure_text(&mut self, text: &str) -> f64 {
            // A deterministic metric: 8 px per character at the current font (enough to exercise wrapping).
            text.chars().count() as f64 * (self.font_px / 2.0).max(4.0)
        }
    }

    #[test]
    fn the_board_rect_is_a_rounded_card() {
        let shape: ShapeElement = serde_json::from_value(json!({"id":"a","type":"shape","x":1.0,"y":2.0,"width":30.0,"height":40.0,"kind":"rect","fill":"#fff","stroke":"#000","strokeWidth":2.0})).unwrap();
        let mut r = Recorder::default();
        render_shape(&mut r, &shape);
        assert!(r.log.iter().any(|l| l.starts_with("roundRect 1 2 30 40 6")), "{:?}", r.log);
        assert!(r.log.contains(&"fill".to_string()) && r.log.contains(&"stroke".to_string()));
    }

    #[test]
    fn an_ellipse_emits_shared_geometry_not_a_round_rect() {
        let shape: ShapeElement = serde_json::from_value(json!({"id":"a","type":"shape","x":0.0,"y":0.0,"width":40.0,"height":40.0,"kind":"ellipse","fill":"#eee","stroke":"#000","strokeWidth":2.0})).unwrap();
        let mut r = Recorder::default();
        render_shape(&mut r, &shape);
        assert!(!r.log.iter().any(|l| l.starts_with("roundRect")));
        assert!(r.log.iter().any(|l| l.starts_with("A ") || l.starts_with("C ")), "{:?}", r.log);
    }

    #[test]
    fn a_straight_line_draws_and_an_arrow_kind_adds_a_head() {
        let mut r = Recorder::default();
        paint_open_shape(&mut r, "lineArrow", 0.0, 0.0, 100.0, 100.0, 2.0);
        // Two begin_paths: the stroke, then the arrow head (a filled triangle).
        assert_eq!(r.log.iter().filter(|l| *l == "begin").count(), 2);
        assert_eq!(r.log.iter().filter(|l| *l == "fill").count(), 1);
    }

    #[test]
    fn dots_background_paints_a_grid_of_arcs() {
        let mut vp = Viewport::new();
        vp.set_zoom(1.0);
        let mut r = Recorder::default();
        render_background(&mut r, "dots", &vp, 48.0, 48.0);
        assert!(r.log.iter().filter(|l| l.starts_with("arc ")).count() > 1);
        // `white` paints nothing.
        let mut r2 = Recorder::default();
        render_background(&mut r2, "white", &vp, 48.0, 48.0);
        assert!(r2.log.is_empty());
    }

    #[test]
    fn long_text_wraps_on_spaces() {
        let mut r = Recorder::default();
        r.set_font(16.0, false, "Arial");
        let lines = wrap_text(&mut r, "aaaa bbbb cccc", 40.0);
        assert!(lines.len() >= 2);
    }
}
