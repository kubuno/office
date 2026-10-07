//! What the slide renderer draws on: a small Canvas-2D-like context ([`Ctx`]) over the platform's
//! [`Surface`].
//!
//! The web's `SlideRenderer` is written against `CanvasRenderingContext2D`; [`Ctx`] keeps the parts of its
//! state model the renderer relies on (the save/restore stack, the transform, `globalAlpha`, the shadow, the
//! clip, `filter`, the `multiply` composite) so the port stays line for line, and hands each drawing call to the
//! surface with the state it was made in. A surface draws paths, text runs and images under a given matrix —
//! Direct2D on Windows, a recorder in the tests.

use crate::color::Rgba;
use crate::model::Rect;
use kubuno_office_shapes_core::{Cmd, Path};

/// An affine transform, canvas order (`a b c d e f`: x' = a·x + c·y + e, y' = b·x + d·y + f).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Matrix {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Default for Matrix {
    fn default() -> Self {
        Matrix::IDENTITY
    }
}

impl Matrix {
    pub const IDENTITY: Matrix = Matrix { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 };

    pub fn translation(x: f64, y: f64) -> Matrix {
        Matrix { e: x, f: y, ..Matrix::IDENTITY }
    }

    pub fn scaling(x: f64, y: f64) -> Matrix {
        Matrix { a: x, d: y, ..Matrix::IDENTITY }
    }

    pub fn rotation(angle: f64) -> Matrix {
        let (s, c) = angle.sin_cos();
        Matrix { a: c, b: s, c: -s, d: c, e: 0.0, f: 0.0 }
    }

    /// `self` applied after `inner` (the canvas's `ctx.transform(inner)` on a context whose matrix is `self`).
    pub fn mul(&self, inner: &Matrix) -> Matrix {
        Matrix {
            a: self.a * inner.a + self.c * inner.b,
            b: self.b * inner.a + self.d * inner.b,
            c: self.a * inner.c + self.c * inner.d,
            d: self.b * inner.c + self.d * inner.d,
            e: self.a * inner.e + self.c * inner.f + self.e,
            f: self.b * inner.e + self.d * inner.f + self.f,
        }
    }

    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        (self.a * x + self.c * y + self.e, self.b * x + self.d * y + self.f)
    }

    pub fn inverse(&self) -> Option<Matrix> {
        let det = self.a * self.d - self.b * self.c;
        if det.abs() < 1e-15 {
            return None;
        }
        let a = self.d / det;
        let b = -self.b / det;
        let c = -self.c / det;
        let d = self.a / det;
        Some(Matrix { a, b, c, d, e: -(a * self.e + c * self.f), f: -(b * self.e + d * self.f) })
    }
}

/// A fill or stroke paint. Gradient coordinates are in the space the path is drawn in.
#[derive(Debug, Clone, PartialEq)]
pub enum Paint {
    Solid(Rgba),
    Linear { x0: f64, y0: f64, x1: f64, y1: f64, stops: Vec<(f64, Rgba)> },
    Radial { cx: f64, cy: f64, r: f64, stops: Vec<(f64, Rgba)> },
}

impl Paint {
    pub fn css(s: &str) -> Paint {
        Paint::Solid(Rgba::parse(s).unwrap_or(Rgba::BLACK))
    }
    pub fn is_invisible(&self) -> bool {
        match self {
            Paint::Solid(c) => c.a <= 0.0,
            Paint::Linear { stops, .. } | Paint::Radial { stops, .. } => stops.iter().all(|s| s.1.a <= 0.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineCap {
    #[default]
    Butt,
    Round,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineJoin {
    #[default]
    Miter,
    Round,
}

/// How a path is stroked (width and dashes in the path's own units).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Stroke {
    pub width: f64,
    pub dash: Vec<f64>,
    pub cap: LineCap,
    pub join: LineJoin,
}

/// A canvas shadow: offsets and blur in canvas pixels, unaffected by the transform (the canvas rule).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    pub color: Rgba,
    pub blur: f64,
    pub dx: f64,
    pub dy: f64,
}

/// CSS filters on images (`ctx.filter`), as the renderer sets them.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Filters {
    pub grayscale: f64,
    pub sepia: f64,
    pub brightness: f64,
    pub contrast: f64,
    pub saturate: f64,
    /// Blur radius in canvas px.
    pub blur: f64,
}

impl Filters {
    pub const NONE: Filters = Filters { grayscale: 0.0, sepia: 0.0, brightness: 1.0, contrast: 1.0, saturate: 1.0, blur: 0.0 };
    pub fn is_none(&self) -> bool {
        self.grayscale == 0.0 && self.sepia == 0.0 && self.brightness == 1.0 && self.contrast == 1.0 && self.saturate == 1.0 && self.blur == 0.0
    }
}

/// A font as a canvas `font` shorthand names it.
#[derive(Debug, Clone, PartialEq)]
pub struct Font {
    /// The CSS family list (`Outfit, Arial, sans-serif`): the platform takes the first it has.
    pub family: String,
    pub size: f64,
    pub bold: bool,
    pub italic: bool,
}

impl Font {
    pub fn new(family: &str, size: f64) -> Font {
        Font { family: family.to_string(), size, bold: false, italic: false }
    }
}

/// Where `y` is for a text call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Baseline {
    #[default]
    Alphabetic,
    /// The middle of the em box (`textBaseline = 'middle'`).
    Middle,
}

/// The drawing state of one call.
#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub m: Matrix,
    pub alpha: f64,
    pub shadow: Option<Shadow>,
    pub filter: Filters,
    /// `globalCompositeOperation = 'multiply'`.
    pub multiply: bool,
}

impl Default for State {
    fn default() -> Self {
        State { m: Matrix::IDENTITY, alpha: 1.0, shadow: None, filter: Filters::NONE, multiply: false }
    }
}

/// The platform's drawing surface. Coordinates are canvas pixels before `State::m`.
pub trait Surface {
    fn fill_path(&mut self, path: &Path, paint: &Paint, st: &State);
    fn stroke_path(&mut self, path: &Path, paint: &Paint, stroke: &Stroke, st: &State);
    /// One run of text at `(x, y)` (left edge; `y` per `baseline`), `letter_spacing` after every character.
    #[allow(clippy::too_many_arguments)]
    fn fill_text(&mut self, text: &str, font: &Font, x: f64, y: f64, baseline: Baseline, paint: &Paint, letter_spacing: f64, st: &State);
    #[allow(clippy::too_many_arguments)]
    fn stroke_text(&mut self, text: &str, font: &Font, x: f64, y: f64, color: Rgba, width: f64, letter_spacing: f64, st: &State);
    /// An image source (`kbfile:<id>`, `data:`, URL) into `dst`; `crop` in fractions of the source. Returns
    /// false when the image is not available (yet): the renderer then paints the web's placeholder.
    fn draw_image(&mut self, src: &str, crop: Option<Rect>, dst: Rect, st: &State) -> bool;
    /// The natural size of an available image.
    fn image_size(&mut self, src: &str) -> Option<(f64, f64)>;
    fn push_clip(&mut self, path: &Path, m: &Matrix);
    fn pop_clip(&mut self);
}

/// Builds canvas paths (`beginPath`, `moveTo`, `arc`, `ellipse`, `quadraticCurveTo`…) into [`Path`] commands.
#[derive(Debug, Clone, Default)]
pub struct PathBuilder {
    pub path: Path,
    cur: Option<(f64, f64)>,
    start: Option<(f64, f64)>,
}

impl PathBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn move_to(&mut self, x: f64, y: f64) -> &mut Self {
        self.path.cmds.push(Cmd::MoveTo(x, y));
        self.cur = Some((x, y));
        self.start = Some((x, y));
        self
    }

    pub fn line_to(&mut self, x: f64, y: f64) -> &mut Self {
        if self.cur.is_none() {
            return self.move_to(x, y);
        }
        self.path.cmds.push(Cmd::LineTo(x, y));
        self.cur = Some((x, y));
        self
    }

    pub fn quad_to(&mut self, cx: f64, cy: f64, x: f64, y: f64) -> &mut Self {
        if self.cur.is_none() {
            self.move_to(cx, cy);
        }
        self.path.cmds.push(Cmd::QuadTo(cx, cy, x, y));
        self.cur = Some((x, y));
        self
    }

    pub fn bezier_to(&mut self, c1x: f64, c1y: f64, c2x: f64, c2y: f64, x: f64, y: f64) -> &mut Self {
        if self.cur.is_none() {
            self.move_to(c1x, c1y);
        }
        self.path.cmds.push(Cmd::CubicTo(c1x, c1y, c2x, c2y, x, y));
        self.cur = Some((x, y));
        self
    }

    pub fn close(&mut self) -> &mut Self {
        self.path.cmds.push(Cmd::Close);
        self.cur = self.start;
        self
    }

    pub fn rect(&mut self, x: f64, y: f64, w: f64, h: f64) -> &mut Self {
        self.move_to(x, y).line_to(x + w, y).line_to(x + w, y + h).line_to(x, y + h).close();
        self
    }

    /// The renderer's own `roundRect` (quadratic corners).
    pub fn round_rect_q(&mut self, x: f64, y: f64, w: f64, h: f64, r: f64) -> &mut Self {
        self.move_to(x + r, y)
            .line_to(x + w - r, y)
            .quad_to(x + w, y, x + w, y + r)
            .line_to(x + w, y + h - r)
            .quad_to(x + w, y + h, x + w - r, y + h)
            .line_to(x + r, y + h)
            .quad_to(x, y + h, x, y + h - r)
            .line_to(x, y + r)
            .quad_to(x, y, x + r, y)
            .close();
        self
    }

    /// `ctx.ellipse(cx, cy, rx, ry, 0, start, end, anticlockwise)`, as canvas: a line from the current point to
    /// the arc's start, then the arc (split into SVG arcs of at most a half turn).
    #[allow(clippy::too_many_arguments)]
    pub fn ellipse(&mut self, cx: f64, cy: f64, rx: f64, ry: f64, start: f64, end: f64, anticlockwise: bool) -> &mut Self {
        let tau = std::f64::consts::TAU;
        let mut sweep = end - start;
        if !anticlockwise {
            if sweep >= tau {
                sweep = tau;
            } else if sweep < 0.0 {
                sweep = sweep.rem_euclid(tau);
            }
        } else if -sweep >= tau {
            sweep = -tau;
        } else if sweep > 0.0 {
            sweep = -((-sweep).rem_euclid(tau));
            if sweep == 0.0 {
                sweep = -tau;
            }
        }
        let p0 = (cx + rx * start.cos(), cy + ry * start.sin());
        if self.cur.is_some() {
            self.line_to(p0.0, p0.1);
        } else {
            self.move_to(p0.0, p0.1);
        }
        if rx <= 0.0 || ry <= 0.0 || sweep == 0.0 {
            return self;
        }
        let steps = (sweep.abs() / std::f64::consts::PI).ceil().max(1.0) as usize;
        for k in 1..=steps {
            let a = start + sweep * k as f64 / steps as f64;
            let (x, y) = (cx + rx * a.cos(), cy + ry * a.sin());
            self.path.cmds.push(Cmd::ArcTo { rx, ry, large: false, sweep: sweep > 0.0, x, y });
            self.cur = Some((x, y));
        }
        self
    }

    pub fn arc(&mut self, cx: f64, cy: f64, r: f64, start: f64, end: f64, anticlockwise: bool) -> &mut Self {
        self.ellipse(cx, cy, r, r, start, end, anticlockwise)
    }

    pub fn take(&mut self) -> Path {
        self.cur = None;
        self.start = None;
        std::mem::take(&mut self.path)
    }
}

/// The renderer's canvas context: the state stack and the calls it makes.
pub struct Ctx<'s> {
    pub surface: &'s mut dyn Surface,
    st: State,
    stack: Vec<(State, usize)>,
    clips: usize,
}

impl<'s> Ctx<'s> {
    pub fn new(surface: &'s mut dyn Surface, base: Matrix) -> Ctx<'s> {
        Ctx { surface, st: State { m: base, ..State::default() }, stack: Vec::new(), clips: 0 }
    }

    pub fn state(&self) -> &State {
        &self.st
    }

    pub fn save(&mut self) {
        self.stack.push((self.st.clone(), self.clips));
    }

    pub fn restore(&mut self) {
        if let Some((st, clips)) = self.stack.pop() {
            while self.clips > clips {
                self.surface.pop_clip();
                self.clips -= 1;
            }
            self.st = st;
        }
    }

    pub fn translate(&mut self, x: f64, y: f64) {
        self.st.m = self.st.m.mul(&Matrix::translation(x, y));
    }

    pub fn scale(&mut self, x: f64, y: f64) {
        self.st.m = self.st.m.mul(&Matrix::scaling(x, y));
    }

    pub fn rotate(&mut self, angle: f64) {
        self.st.m = self.st.m.mul(&Matrix::rotation(angle));
    }

    pub fn alpha(&self) -> f64 {
        self.st.alpha
    }

    pub fn set_alpha(&mut self, a: f64) {
        self.st.alpha = a.clamp(0.0, 1.0);
    }

    pub fn set_shadow(&mut self, s: Option<Shadow>) {
        self.st.shadow = s.filter(|s| s.color.a > 0.0 && (s.blur > 0.0 || s.dx != 0.0 || s.dy != 0.0));
    }

    pub fn set_filter(&mut self, f: Filters) {
        self.st.filter = f;
    }

    pub fn set_multiply(&mut self, on: bool) {
        self.st.multiply = on;
    }

    pub fn clip(&mut self, path: &Path) {
        self.surface.push_clip(path, &self.st.m);
        self.clips += 1;
    }

    pub fn fill(&mut self, path: &Path, paint: &Paint) {
        if !paint.is_invisible() && !path.is_empty() {
            self.surface.fill_path(path, paint, &self.st);
        }
    }

    pub fn stroke(&mut self, path: &Path, paint: &Paint, stroke: &Stroke) {
        if stroke.width > 0.0 && !path.is_empty() {
            self.surface.stroke_path(path, paint, stroke, &self.st);
        }
    }

    pub fn fill_rect(&mut self, x: f64, y: f64, w: f64, h: f64, paint: &Paint) {
        let p = PathBuilder::new().rect(x, y, w, h).take();
        self.fill(&p, paint);
    }

    pub fn stroke_rect(&mut self, x: f64, y: f64, w: f64, h: f64, paint: &Paint, stroke: &Stroke) {
        let p = PathBuilder::new().rect(x, y, w, h).take();
        self.stroke(&p, paint, stroke);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn fill_text(&mut self, text: &str, font: &Font, x: f64, y: f64, baseline: Baseline, paint: &Paint, ls: f64) {
        if !text.is_empty() {
            self.surface.fill_text(text, font, x, y, baseline, paint, ls, &self.st);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn stroke_text(&mut self, text: &str, font: &Font, x: f64, y: f64, color: Rgba, width: f64, ls: f64) {
        if !text.is_empty() && width > 0.0 {
            self.surface.stroke_text(text, font, x, y, color, width, ls, &self.st);
        }
    }

    pub fn draw_image(&mut self, src: &str, crop: Option<Rect>, dst: Rect) -> bool {
        self.surface.draw_image(src, crop, dst, &self.st)
    }
}

impl Drop for Ctx<'_> {
    fn drop(&mut self) {
        while self.clips > 0 {
            self.surface.pop_clip();
            self.clips -= 1;
        }
    }
}

/// A surface that records the calls (tests, the text interface).
#[derive(Debug, Default)]
pub struct Recorder {
    pub ops: Vec<Op>,
    /// Image sources reported as available, with their size.
    pub images: Vec<(String, f64, f64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Fill { path: Path, paint: Paint, st: State },
    Stroke { path: Path, paint: Paint, stroke: Stroke, st: State },
    Text { text: String, font: Font, x: f64, y: f64, baseline: Baseline, paint: Paint, st: State },
    StrokeText { text: String, x: f64, y: f64, width: f64 },
    Image { src: String, crop: Option<Rect>, dst: Rect, st: State },
    Clip(Path),
    Unclip,
}

impl Surface for Recorder {
    fn fill_path(&mut self, path: &Path, paint: &Paint, st: &State) {
        self.ops.push(Op::Fill { path: path.clone(), paint: paint.clone(), st: st.clone() });
    }
    fn stroke_path(&mut self, path: &Path, paint: &Paint, stroke: &Stroke, st: &State) {
        self.ops.push(Op::Stroke { path: path.clone(), paint: paint.clone(), stroke: stroke.clone(), st: st.clone() });
    }
    fn fill_text(&mut self, text: &str, font: &Font, x: f64, y: f64, baseline: Baseline, paint: &Paint, _ls: f64, st: &State) {
        self.ops.push(Op::Text { text: text.into(), font: font.clone(), x, y, baseline, paint: paint.clone(), st: st.clone() });
    }
    fn stroke_text(&mut self, text: &str, _font: &Font, x: f64, y: f64, _color: Rgba, width: f64, _ls: f64, _st: &State) {
        self.ops.push(Op::StrokeText { text: text.into(), x, y, width });
    }
    fn draw_image(&mut self, src: &str, crop: Option<Rect>, dst: Rect, st: &State) -> bool {
        if self.images.iter().any(|i| i.0 == src) {
            self.ops.push(Op::Image { src: src.into(), crop, dst, st: st.clone() });
            true
        } else {
            false
        }
    }
    fn image_size(&mut self, src: &str) -> Option<(f64, f64)> {
        self.images.iter().find(|i| i.0 == src).map(|i| (i.1, i.2))
    }
    fn push_clip(&mut self, path: &Path, _m: &Matrix) {
        self.ops.push(Op::Clip(path.clone()));
    }
    fn pop_clip(&mut self) {
        self.ops.push(Op::Unclip);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrices_compose_like_the_canvas() {
        // translate then rotate 90°: (1, 0) → (0, 1) → + (10, 0).
        let m = Matrix::translation(10.0, 0.0).mul(&Matrix::rotation(std::f64::consts::FRAC_PI_2));
        let (x, y) = m.apply(1.0, 0.0);
        assert!((x - 10.0).abs() < 1e-12 && (y - 1.0).abs() < 1e-12);
        let inv = m.inverse().expect("invertible");
        let (bx, by) = inv.apply(x, y);
        assert!((bx - 1.0).abs() < 1e-12 && by.abs() < 1e-12);
    }

    #[test]
    fn a_full_circle_is_two_half_arcs() {
        let p = PathBuilder::new().arc(0.0, 0.0, 10.0, 0.0, std::f64::consts::TAU, false).take();
        let arcs = p.cmds.iter().filter(|c| matches!(c, Cmd::ArcTo { .. })).count();
        assert_eq!(arcs, 2);
        assert!(matches!(p.cmds[0], Cmd::MoveTo(x, _) if (x - 10.0).abs() < 1e-12));
    }

    #[test]
    fn restore_pops_the_clips_of_its_save() {
        let mut rec = Recorder::default();
        {
            let mut ctx = Ctx::new(&mut rec, Matrix::IDENTITY);
            ctx.save();
            let r = PathBuilder::new().rect(0.0, 0.0, 1.0, 1.0).take();
            ctx.clip(&r);
            ctx.restore();
        }
        assert_eq!(rec.ops.last(), Some(&Op::Unclip));
    }
}
