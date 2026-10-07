//! A Canvas 2D context in Rust — the surface the web's drawing code is ported onto, line for line.
//!
//! The web editor draws with `CanvasRenderingContext2D`: `save`/`restore`, transforms, `beginPath` …
//! `fill`/`stroke`, `fillText`, dashes, shadows, `globalAlpha`. Porting that code onto a different drawing
//! model would be a rewrite, and a rewrite drifts. So [`Ctx2D`] implements the HTML specification's
//! semantics for the subset the office editors use (the state stack, the current path with `arc`,
//! `arcTo`, `ellipse`, `roundRect`, `Path2D` from SVG data, line dashes, text alignment and baselines) and
//! hands the result to a small [`Surface`]: fill a path, stroke a path, draw a run of text, measure text.
//! Direct2D implements it on Windows; [`Recorder`] does in the tests.
//!
//! Geometry is resolved here, in the same way on every platform: points are transformed when they are
//! added to the path (as the specification says), arcs and quadratic curves become cubic Béziers, and
//! the surface receives device-space paths. A stroke's width and dashes are scaled by the transform's
//! linear factor at the time of the stroke (the editors only use translations, rotations, flips and
//! uniform scales, for which that is exact).

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use crate::color::{parse_color, Rgba};

// ── Geometry ─────────────────────────────────────────────────────────────────

/// A point.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// An affine transform, `DOMMatrix`'s `a b c d e f`: `x' = a·x + c·y + e`, `y' = b·x + d·y + f`.
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
        Self::IDENTITY
    }
}

impl Matrix {
    pub const IDENTITY: Matrix = Matrix { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 };

    pub fn apply(&self, x: f64, y: f64) -> Point {
        Point { x: self.a * x + self.c * y + self.e, y: self.b * x + self.d * y + self.f }
    }

    /// `self` then `m` applied to the points first: the canvas' `transform(m)` (post-multiplication).
    pub fn then(&self, m: &Matrix) -> Matrix {
        Matrix {
            a: self.a * m.a + self.c * m.b,
            b: self.b * m.a + self.d * m.b,
            c: self.a * m.c + self.c * m.d,
            d: self.b * m.c + self.d * m.d,
            e: self.a * m.e + self.c * m.f + self.e,
            f: self.b * m.e + self.d * m.f + self.f,
        }
    }

    /// How much the transform scales lengths (√|det|): exact for similarity transforms.
    pub fn scale_factor(&self) -> f64 {
        (self.a * self.d - self.b * self.c).abs().sqrt()
    }

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

    /// The inverse, `None` when the matrix is singular.
    pub fn inverse(&self) -> Option<Matrix> {
        let det = self.a * self.d - self.b * self.c;
        if det.abs() < 1e-300 {
            return None;
        }
        Some(Matrix {
            a: self.d / det,
            b: -self.b / det,
            c: -self.c / det,
            d: self.a / det,
            e: (self.c * self.f - self.d * self.e) / det,
            f: (self.b * self.e - self.a * self.f) / det,
        })
    }
}

/// One element of a device-space path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathEl {
    MoveTo(Point),
    LineTo(Point),
    CubicTo(Point, Point, Point),
    Close,
}

/// A device-space path: sub-paths of lines and cubic Béziers. Filled with the non-zero winding rule
/// (the canvas default).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Path {
    pub els: Vec<PathEl>,
}

impl Path {
    pub fn is_empty(&self) -> bool {
        self.els.is_empty()
    }

    /// The bounding box of the points (control points included), `None` for an empty path.
    pub fn bounds(&self) -> Option<(Point, Point)> {
        let mut it = self.els.iter().flat_map(|e| match *e {
            PathEl::MoveTo(p) | PathEl::LineTo(p) => vec![p],
            PathEl::CubicTo(a, b, c) => vec![a, b, c],
            PathEl::Close => vec![],
        });
        let first = it.next()?;
        let (mut lo, mut hi) = (first, first);
        for p in it {
            lo.x = lo.x.min(p.x);
            lo.y = lo.y.min(p.y);
            hi.x = hi.x.max(p.x);
            hi.y = hi.y.max(p.y);
        }
        Some((lo, hi))
    }
}

// ── Styles ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineCap {
    #[default]
    Butt,
    Round,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

/// How a path is stroked, in device units.
#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    pub color: Rgba,
    pub width: f64,
    /// The dash pattern (already doubled when its length was odd, as the canvas does); empty for solid.
    pub dash: Vec<f64>,
    pub dash_offset: f64,
    pub cap: LineCap,
    pub join: LineJoin,
    pub miter_limit: f64,
}

/// A canvas shadow: the colour, the blur and the offsets — in device units, which the specification
/// leaves untouched by the transform.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    pub color: Rgba,
    pub blur: f64,
    pub offset_x: f64,
    pub offset_y: f64,
}

/// A font, as the CSS `font` shorthand the web sets describes it.
#[derive(Debug, Clone, PartialEq)]
pub struct Font {
    /// The family list, in order of preference, quotes removed (`["Inter", "Arial", "sans-serif"]`).
    pub families: Vec<String>,
    /// The size in CSS pixels (user units).
    pub size: f64,
    pub bold: bool,
    pub italic: bool,
}

impl Default for Font {
    /// The canvas default, `10px sans-serif`.
    fn default() -> Self {
        Self { families: vec!["sans-serif".into()], size: 10.0, bold: false, italic: false }
    }
}

impl Font {
    /// Parses the CSS `font` shorthand the web code writes: `[italic] [bold|<weight>] <size>px <families>`.
    /// `None` when it is not a font (the canvas then keeps the previous font).
    pub fn parse(css: &str) -> Option<Font> {
        let mut rest = css.trim();
        let mut font = Font { families: Vec::new(), size: 10.0, bold: false, italic: false };
        loop {
            let (word, tail) = match rest.find(char::is_whitespace) {
                Some(i) => (&rest[..i], rest[i..].trim_start()),
                None => (rest, ""),
            };
            match word {
                "italic" | "oblique" => font.italic = true,
                "normal" | "small-caps" => {}
                "bold" | "bolder" => font.bold = true,
                "lighter" => {}
                w if w.chars().all(|c| c.is_ascii_digit()) && !w.is_empty() => {
                    font.bold = w.parse::<u32>().map(|n| n >= 600).unwrap_or(false);
                }
                w => {
                    // The size: `12px` (a line height `12px/1.3` is ignored).
                    let size = w.split('/').next().unwrap_or(w);
                    let n = size.strip_suffix("px")?.parse::<f64>().ok()?;
                    if !n.is_finite() || n <= 0.0 {
                        return None;
                    }
                    font.size = n;
                    font.families = tail
                        .split(',')
                        .map(|f| f.trim().trim_matches(|c| c == '"' || c == '\'').to_string())
                        .filter(|f| !f.is_empty())
                        .collect();
                    if font.families.is_empty() {
                        return None;
                    }
                    return Some(font);
                }
            }
            if tail.is_empty() {
                return None;
            }
            rest = tail;
        }
    }
}

/// `textAlign`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Start,
    End,
    Left,
    Right,
    Center,
}

impl TextAlign {
    /// The canvas' values; an unknown one leaves the alignment unchanged (`None`).
    pub fn parse(s: &str) -> Option<TextAlign> {
        Some(match s {
            "start" => TextAlign::Start,
            "end" => TextAlign::End,
            "left" => TextAlign::Left,
            "right" => TextAlign::Right,
            "center" => TextAlign::Center,
            _ => return None,
        })
    }
}

/// `textBaseline`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextBaseline {
    Top,
    Hanging,
    Middle,
    #[default]
    Alphabetic,
    Ideographic,
    Bottom,
}

impl TextBaseline {
    pub fn parse(s: &str) -> Option<TextBaseline> {
        Some(match s {
            "top" => TextBaseline::Top,
            "hanging" => TextBaseline::Hanging,
            "middle" => TextBaseline::Middle,
            "alphabetic" => TextBaseline::Alphabetic,
            "ideographic" => TextBaseline::Ideographic,
            "bottom" => TextBaseline::Bottom,
            _ => return None,
        })
    }
}

/// What a surface measures of a run of text, in the font's own units (user space).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TextMetrics {
    /// The advance width (`measureText().width`).
    pub width: f64,
    /// The em box above the alphabetic baseline (`emHeightAscent`), positive.
    pub em_ascent: f64,
    /// The em box below the alphabetic baseline (`emHeightDescent`), positive.
    pub em_descent: f64,
}

/// A run of text to draw: its left end sits on the alphabetic baseline at `origin` (user space, the
/// alignment and the baseline already resolved), drawn through `transform`.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRun<'a> {
    pub text: &'a str,
    pub font: &'a Font,
    pub origin: Point,
    pub transform: Matrix,
    pub color: Rgba,
}

/// What a platform draws with. Paths arrive in device space, already transformed.
pub trait Surface {
    /// Fills `path` (non-zero winding) with `color`, casting `shadow` when there is one.
    fn fill_path(&mut self, path: &Path, color: Rgba, shadow: Option<&Shadow>);
    /// Strokes `path`.
    fn stroke_path(&mut self, path: &Path, stroke: &Stroke, shadow: Option<&Shadow>);
    /// Draws a run of text.
    fn fill_text(&mut self, run: &TextRun<'_>, shadow: Option<&Shadow>);
    /// Measures `text` in `font` (user units).
    fn measure_text(&mut self, text: &str, font: &Font) -> TextMetrics;
}

// ── The context ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct State {
    transform: Matrix,
    fill: Rgba,
    stroke: Rgba,
    line_width: f64,
    dash: Vec<f64>,
    dash_offset: f64,
    cap: LineCap,
    join: LineJoin,
    miter_limit: f64,
    alpha: f64,
    shadow_color: Rgba,
    shadow_blur: f64,
    shadow_x: f64,
    shadow_y: f64,
    font: Font,
    align: TextAlign,
    baseline: TextBaseline,
}

impl Default for State {
    fn default() -> Self {
        Self {
            transform: Matrix::IDENTITY,
            fill: Rgba::BLACK,
            stroke: Rgba::BLACK,
            line_width: 1.0,
            dash: Vec::new(),
            dash_offset: 0.0,
            cap: LineCap::Butt,
            join: LineJoin::Miter,
            miter_limit: 10.0,
            alpha: 1.0,
            shadow_color: Rgba::TRANSPARENT,
            shadow_blur: 0.0,
            shadow_x: 0.0,
            shadow_y: 0.0,
            font: Font::default(),
            align: TextAlign::Start,
            baseline: TextBaseline::Alphabetic,
        }
    }
}

/// The path under construction, in device space.
#[derive(Debug, Clone, Default)]
struct PathBuilder {
    els: Vec<PathEl>,
    /// The first point of the current sub-path, `None` when there is no sub-path.
    start: Option<Point>,
    /// The last point, `None` when there is no sub-path.
    last: Option<Point>,
}

impl PathBuilder {
    fn move_to(&mut self, p: Point) {
        self.els.push(PathEl::MoveTo(p));
        self.start = Some(p);
        self.last = Some(p);
    }

    fn line_to(&mut self, p: Point) {
        if self.last.is_none() {
            self.move_to(p);
            return;
        }
        self.els.push(PathEl::LineTo(p));
        self.last = Some(p);
    }

    fn cubic_to(&mut self, a: Point, b: Point, c: Point) {
        if self.last.is_none() {
            self.move_to(a);
        }
        self.els.push(PathEl::CubicTo(a, b, c));
        self.last = Some(c);
    }

    fn close(&mut self) {
        if let Some(s) = self.start {
            self.els.push(PathEl::Close);
            // A closed sub-path is followed by a new one starting at the same point.
            self.last = Some(s);
        }
    }
}

/// The Canvas 2D context: the web's drawing calls, resolved onto a [`Surface`].
pub struct Ctx2D<'s> {
    surface: &'s mut dyn Surface,
    state: State,
    stack: Vec<State>,
    path: PathBuilder,
}

/// Bézier circle constant for one quarter turn.
fn kappa(sweep: f64) -> f64 {
    4.0 / 3.0 * (sweep / 4.0).tan()
}

impl<'s> Ctx2D<'s> {
    pub fn new(surface: &'s mut dyn Surface) -> Self {
        Self { surface, state: State::default(), stack: Vec::new(), path: PathBuilder::default() }
    }

    /// The surface, for what is not a canvas call (a backend-specific bitmap…).
    pub fn surface(&mut self) -> &mut dyn Surface {
        &mut *self.surface
    }

    // ── State ────────────────────────────────────────────────────────────────

    pub fn save(&mut self) {
        self.stack.push(self.state.clone());
    }

    pub fn restore(&mut self) {
        if let Some(s) = self.stack.pop() {
            self.state = s;
        }
    }

    pub fn transform_matrix(&self) -> Matrix {
        self.state.transform
    }

    pub fn set_transform(&mut self, m: Matrix) {
        self.state.transform = m;
    }

    pub fn reset_transform(&mut self) {
        self.state.transform = Matrix::IDENTITY;
    }

    pub fn transform(&mut self, m: Matrix) {
        self.state.transform = self.state.transform.then(&m);
    }

    pub fn translate(&mut self, x: f64, y: f64) {
        self.transform(Matrix::translation(x, y));
    }

    pub fn scale(&mut self, x: f64, y: f64) {
        self.transform(Matrix::scaling(x, y));
    }

    pub fn rotate(&mut self, angle: f64) {
        self.transform(Matrix::rotation(angle));
    }

    /// `fillStyle = …`: a CSS colour; an unparseable one is ignored, as the canvas does.
    pub fn set_fill_style(&mut self, css: &str) {
        if let Some(c) = parse_color(css) {
            self.state.fill = c;
        }
    }

    pub fn set_fill_color(&mut self, c: Rgba) {
        self.state.fill = c;
    }

    pub fn fill_color(&self) -> Rgba {
        self.state.fill
    }

    /// `strokeStyle = …`.
    pub fn set_stroke_style(&mut self, css: &str) {
        if let Some(c) = parse_color(css) {
            self.state.stroke = c;
        }
    }

    pub fn set_stroke_color(&mut self, c: Rgba) {
        self.state.stroke = c;
    }

    pub fn stroke_color(&self) -> Rgba {
        self.state.stroke
    }

    /// `lineWidth = …` (non-positive and non-finite values are ignored, as the canvas does).
    pub fn set_line_width(&mut self, w: f64) {
        if w.is_finite() && w > 0.0 {
            self.state.line_width = w;
        }
    }

    pub fn line_width(&self) -> f64 {
        self.state.line_width
    }

    /// `setLineDash(…)`: a negative or non-finite value rejects the whole pattern; an odd-length one is
    /// doubled.
    pub fn set_line_dash(&mut self, segments: &[f64]) {
        if segments.iter().any(|v| !v.is_finite() || *v < 0.0) {
            return;
        }
        let mut d = segments.to_vec();
        if d.len() % 2 == 1 {
            d.extend_from_slice(segments);
        }
        self.state.dash = d;
    }

    pub fn set_line_dash_offset(&mut self, offset: f64) {
        if offset.is_finite() {
            self.state.dash_offset = offset;
        }
    }

    pub fn set_line_cap(&mut self, cap: LineCap) {
        self.state.cap = cap;
    }

    pub fn set_line_join(&mut self, join: LineJoin) {
        self.state.join = join;
    }

    /// `globalAlpha = …` (outside 0..=1 ignored).
    pub fn set_global_alpha(&mut self, a: f64) {
        if (0.0..=1.0).contains(&a) {
            self.state.alpha = a;
        }
    }

    pub fn global_alpha(&self) -> f64 {
        self.state.alpha
    }

    pub fn set_shadow_color(&mut self, css: &str) {
        if let Some(c) = parse_color(css) {
            self.state.shadow_color = c;
        }
    }

    pub fn set_shadow_blur(&mut self, v: f64) {
        if v.is_finite() && v >= 0.0 {
            self.state.shadow_blur = v;
        }
    }

    pub fn set_shadow_offset(&mut self, x: f64, y: f64) {
        if x.is_finite() && y.is_finite() {
            self.state.shadow_x = x;
            self.state.shadow_y = y;
        }
    }

    /// `font = …` (an unparseable value is ignored).
    pub fn set_font(&mut self, css: &str) {
        if let Some(f) = Font::parse(css) {
            self.state.font = f;
        }
    }

    pub fn font(&self) -> &Font {
        &self.state.font
    }

    pub fn set_text_align(&mut self, a: TextAlign) {
        self.state.align = a;
    }

    pub fn set_text_baseline(&mut self, b: TextBaseline) {
        self.state.baseline = b;
    }

    fn shadow(&self) -> Option<Shadow> {
        let c = self.state.shadow_color;
        let drawn = c.a > 0.0 && (self.state.shadow_blur > 0.0 || self.state.shadow_x != 0.0 || self.state.shadow_y != 0.0);
        drawn.then(|| Shadow {
            color: c.with_alpha((c.a as f64 * self.state.alpha) as f32),
            blur: self.state.shadow_blur,
            offset_x: self.state.shadow_x,
            offset_y: self.state.shadow_y,
        })
    }

    // ── The current path ─────────────────────────────────────────────────────

    fn tp(&self, x: f64, y: f64) -> Point {
        self.state.transform.apply(x, y)
    }

    pub fn begin_path(&mut self) {
        self.path = PathBuilder::default();
    }

    pub fn move_to(&mut self, x: f64, y: f64) {
        if !(x.is_finite() && y.is_finite()) {
            return;
        }
        let p = self.tp(x, y);
        self.path.move_to(p);
    }

    pub fn line_to(&mut self, x: f64, y: f64) {
        if !(x.is_finite() && y.is_finite()) {
            return;
        }
        let p = self.tp(x, y);
        self.path.line_to(p);
    }

    pub fn bezier_curve_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x: f64, y: f64) {
        if ![x1, y1, x2, y2, x, y].iter().all(|v| v.is_finite()) {
            return;
        }
        let (a, b, c) = (self.tp(x1, y1), self.tp(x2, y2), self.tp(x, y));
        if self.path.last.is_none() {
            self.path.move_to(a);
        }
        self.path.cubic_to(a, b, c);
    }

    pub fn quadratic_curve_to(&mut self, cx: f64, cy: f64, x: f64, y: f64) {
        if ![cx, cy, x, y].iter().all(|v| v.is_finite()) {
            return;
        }
        let c = self.tp(cx, cy);
        let e = self.tp(x, y);
        let Some(s) = self.path.last else {
            self.path.move_to(c);
            self.path.cubic_to(c, c, e);
            return;
        };
        // The exact cubic of a quadratic (affine maps keep Bézier curves, so this is done in device space).
        let a = Point::new(s.x + 2.0 / 3.0 * (c.x - s.x), s.y + 2.0 / 3.0 * (c.y - s.y));
        let b = Point::new(e.x + 2.0 / 3.0 * (c.x - e.x), e.y + 2.0 / 3.0 * (c.y - e.y));
        self.path.cubic_to(a, b, e);
    }

    pub fn close_path(&mut self) {
        self.path.close();
    }

    /// `rect(x, y, w, h)`: a closed four-point sub-path, then a new sub-path at `(x, y)`.
    pub fn rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        if ![x, y, w, h].iter().all(|v| v.is_finite()) {
            return;
        }
        self.move_to(x, y);
        self.line_to(x + w, y);
        self.line_to(x + w, y + h);
        self.line_to(x, y + h);
        self.close_path();
        self.move_to(x, y);
    }

    /// `roundRect(x, y, w, h, r)` with one radius for the four corners (the only form the office code
    /// uses): radii larger than the box are scaled down together, as the specification does.
    pub fn round_rect(&mut self, x: f64, y: f64, w: f64, h: f64, r: f64) {
        if ![x, y, w, h, r].iter().all(|v| v.is_finite()) || r < 0.0 {
            return;
        }
        // A negative width or height mirrors the rectangle (`roundRect` normalises it).
        let (x, w) = if w < 0.0 { (x + w, -w) } else { (x, w) };
        let (y, h) = if h < 0.0 { (y + h, -h) } else { (y, h) };
        let mut r = r;
        let scale = [w / (2.0 * r), h / (2.0 * r)].into_iter().filter(|s| s.is_finite()).fold(1.0f64, f64::min);
        if scale < 1.0 {
            r *= scale;
        }
        if r <= 0.0 {
            self.rect(x, y, w, h);
            return;
        }
        self.move_to(x + r, y);
        self.line_to(x + w - r, y);
        self.ellipse_arc(x + w - r, y + r, r, r, 0.0, -FRAC_PI_2, 0.0, false);
        self.line_to(x + w, y + h - r);
        self.ellipse_arc(x + w - r, y + h - r, r, r, 0.0, 0.0, FRAC_PI_2, false);
        self.line_to(x + r, y + h);
        self.ellipse_arc(x + r, y + h - r, r, r, 0.0, FRAC_PI_2, PI, false);
        self.line_to(x, y + r);
        self.ellipse_arc(x + r, y + r, r, r, 0.0, PI, PI + FRAC_PI_2, false);
        self.close_path();
        self.move_to(x, y);
    }

    /// `arc(x, y, r, start, end, anticlockwise)`.
    pub fn arc(&mut self, x: f64, y: f64, r: f64, start: f64, end: f64, anticlockwise: bool) {
        self.ellipse(x, y, r, r, 0.0, start, end, anticlockwise);
    }

    /// `ellipse(x, y, rx, ry, rotation, start, end, anticlockwise)`.
    #[allow(clippy::too_many_arguments)]
    pub fn ellipse(&mut self, x: f64, y: f64, rx: f64, ry: f64, rotation: f64, start: f64, end: f64, anticlockwise: bool) {
        if ![x, y, rx, ry, rotation, start, end].iter().all(|v| v.is_finite()) || rx < 0.0 || ry < 0.0 {
            return;
        }
        self.ellipse_arc(x, y, rx, ry, rotation, start, end, anticlockwise);
    }

    /// The sweep of an arc as the specification defines it.
    fn sweep(start: f64, end: f64, anticlockwise: bool) -> f64 {
        if !anticlockwise {
            if end - start >= TAU {
                TAU
            } else {
                (end - start).rem_euclid(TAU)
            }
        } else if start - end >= TAU {
            -TAU
        } else {
            -((start - end).rem_euclid(TAU))
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn ellipse_arc(&mut self, cx: f64, cy: f64, rx: f64, ry: f64, rotation: f64, start: f64, end: f64, anticlockwise: bool) {
        let sweep = Self::sweep(start, end, anticlockwise);
        let (rs, rc) = rotation.sin_cos();
        let at = |t: f64| {
            let (s, c) = t.sin_cos();
            let (px, py) = (rx * c, ry * s);
            (cx + px * rc - py * rs, cy + px * rs + py * rc)
        };
        let deriv = |t: f64| {
            let (s, c) = t.sin_cos();
            let (dx, dy) = (-rx * s, ry * c);
            (dx * rc - dy * rs, dx * rs + dy * rc)
        };
        let (sx, sy) = at(start);
        let p0 = self.tp(sx, sy);
        if self.path.last.is_some() {
            self.path.line_to(p0);
        } else {
            self.path.move_to(p0);
        }
        if sweep == 0.0 {
            return;
        }
        let n = ((sweep.abs() / FRAC_PI_2).ceil() as usize).max(1);
        let step = sweep / n as f64;
        let k = kappa(step);
        let mut t = start;
        for _ in 0..n {
            let t2 = t + step;
            let (x1, y1) = at(t);
            let (dx1, dy1) = deriv(t);
            let (x2, y2) = at(t2);
            let (dx2, dy2) = deriv(t2);
            let a = self.tp(x1 + k * dx1, y1 + k * dy1);
            let b = self.tp(x2 - k * dx2, y2 - k * dy2);
            let e = self.tp(x2, y2);
            self.path.cubic_to(a, b, e);
            t = t2;
        }
    }

    /// `arcTo(x1, y1, x2, y2, r)`, as the specification describes it.
    pub fn arc_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, r: f64) {
        if ![x1, y1, x2, y2, r].iter().all(|v| v.is_finite()) || r < 0.0 {
            return;
        }
        // The previous point, back in user space (the specification works in the user space of the
        // current transform).
        let Some(last) = self.path.last else {
            self.move_to(x1, y1);
            return;
        };
        let Some(inv) = self.state.transform.inverse() else {
            return;
        };
        let p0 = inv.apply(last.x, last.y);
        let (p1, p2) = (Point::new(x1, y1), Point::new(x2, y2));
        let same = |a: Point, b: Point| (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9;
        if same(p0, p1) || same(p1, p2) || r == 0.0 {
            self.line_to(x1, y1);
            return;
        }
        let (v1x, v1y) = (p0.x - p1.x, p0.y - p1.y);
        let (v2x, v2y) = (p2.x - p1.x, p2.y - p1.y);
        let l1 = v1x.hypot(v1y);
        let l2 = v2x.hypot(v2y);
        let cross = v1x * v2y - v1y * v2x;
        if cross.abs() < 1e-9 * l1 * l2 {
            // Collinear: a straight line to (x1, y1).
            self.line_to(x1, y1);
            return;
        }
        let cos = ((v1x * v2x + v1y * v2y) / (l1 * l2)).clamp(-1.0, 1.0);
        let theta = cos.acos();
        let dist = r / (theta / 2.0).tan();
        let t1 = Point::new(p1.x + v1x / l1 * dist, p1.y + v1y / l1 * dist);
        let t2 = Point::new(p1.x + v2x / l2 * dist, p1.y + v2y / l2 * dist);
        // The centre lies along the bisector, at r / sin(θ/2) from p1.
        let (bx, by) = (v1x / l1 + v2x / l2, v1y / l1 + v2y / l2);
        let bl = bx.hypot(by);
        let cd = r / (theta / 2.0).sin();
        let c = Point::new(p1.x + bx / bl * cd, p1.y + by / bl * cd);
        let a0 = (t1.y - c.y).atan2(t1.x - c.x);
        let a1 = (t2.y - c.y).atan2(t2.x - c.x);
        // Clockwise (positive angles, y down) when the turn p0→p1→p2 is clockwise.
        let anticlockwise = cross > 0.0;
        self.line_to(t1.x, t1.y);
        self.ellipse_arc(c.x, c.y, r, r, 0.0, a0, a1, anticlockwise);
    }

    /// Appends a path of shape commands (`kubuno_office_shapes_core::Path`, the web's `Path2D` data),
    /// transformed by the current transform. SVG arcs become cubic curves.
    pub fn add_shape_path(&mut self, path: &kubuno_office_shapes_core::Path) {
        use kubuno_office_shapes_core::Cmd;
        // SVG path semantics: the current point, and the start of the sub-path (for `Z`), in user space.
        let mut cur = Point::default();
        let mut start = Point::default();
        for c in &path.cmds {
            match *c {
                Cmd::MoveTo(x, y) => {
                    self.move_to(x, y);
                    cur = Point::new(x, y);
                    start = cur;
                }
                Cmd::LineTo(x, y) => {
                    self.line_to(x, y);
                    cur = Point::new(x, y);
                }
                Cmd::CubicTo(a, b, c2, d, e, f) => {
                    self.bezier_curve_to(a, b, c2, d, e, f);
                    cur = Point::new(e, f);
                }
                Cmd::QuadTo(a, b, e, f) => {
                    self.quadratic_curve_to(a, b, e, f);
                    cur = Point::new(e, f);
                }
                Cmd::ArcTo { rx, ry, large, sweep, x, y } => {
                    self.svg_arc(cur, rx, ry, 0.0, large, sweep, Point::new(x, y));
                    cur = Point::new(x, y);
                }
                Cmd::Close => {
                    self.close_path();
                    cur = start;
                }
            }
        }
    }

    /// An SVG elliptical arc from `from` to `to` (SVG 1.1 appendix F.6: endpoint to centre
    /// parameterisation, radii scaled up when too small).
    #[allow(clippy::too_many_arguments)]
    pub fn svg_arc(&mut self, from: Point, rx: f64, ry: f64, x_rotation_deg: f64, large: bool, sweep: bool, to: Point) {
        if (from.x - to.x).abs() < 1e-12 && (from.y - to.y).abs() < 1e-12 {
            return;
        }
        let (mut rx, mut ry) = (rx.abs(), ry.abs());
        if rx == 0.0 || ry == 0.0 {
            self.line_to(to.x, to.y);
            return;
        }
        let phi = x_rotation_deg.to_radians();
        let (sp, cp) = phi.sin_cos();
        let dx2 = (from.x - to.x) / 2.0;
        let dy2 = (from.y - to.y) / 2.0;
        let x1p = cp * dx2 + sp * dy2;
        let y1p = -sp * dx2 + cp * dy2;
        let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
        if lambda > 1.0 {
            let s = lambda.sqrt();
            rx *= s;
            ry *= s;
        }
        let num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
        let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
        let mut coef = if den == 0.0 { 0.0 } else { (num / den).max(0.0).sqrt() };
        if large == sweep {
            coef = -coef;
        }
        let cxp = coef * rx * y1p / ry;
        let cyp = -coef * ry * x1p / rx;
        let cx = cp * cxp - sp * cyp + (from.x + to.x) / 2.0;
        let cy = sp * cxp + cp * cyp + (from.y + to.y) / 2.0;
        let angle = |ux: f64, uy: f64, vx: f64, vy: f64| {
            let dot = ux * vx + uy * vy;
            let len = ux.hypot(uy) * vx.hypot(vy);
            let a = (dot / len).clamp(-1.0, 1.0).acos();
            if ux * vy - uy * vx < 0.0 {
                -a
            } else {
                a
            }
        };
        let theta1 = angle(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
        let mut dtheta = angle((x1p - cxp) / rx, (y1p - cyp) / ry, (-x1p - cxp) / rx, (-y1p - cyp) / ry);
        if !sweep && dtheta > 0.0 {
            dtheta -= TAU;
        } else if sweep && dtheta < 0.0 {
            dtheta += TAU;
        }
        // Cubic segments of at most a quarter turn, in user space, then transformed.
        let n = ((dtheta.abs() / FRAC_PI_2).ceil() as usize).max(1);
        let step = dtheta / n as f64;
        let k = kappa(step);
        let at = |t: f64| {
            let (s, c) = t.sin_cos();
            let (px, py) = (rx * c, ry * s);
            (cx + px * cp - py * sp, cy + px * sp + py * cp)
        };
        let deriv = |t: f64| {
            let (s, c) = t.sin_cos();
            let (dx, dy) = (-rx * s, ry * c);
            (dx * cp - dy * sp, dx * sp + dy * cp)
        };
        let mut t = theta1;
        for i in 0..n {
            let t2 = t + step;
            let (x1, y1) = at(t);
            let (dx1, dy1) = deriv(t);
            let (x2, y2) = if i + 1 == n { (to.x, to.y) } else { at(t2) };
            let (dx2, dy2) = deriv(t2);
            self.bezier_curve_to(x1 + k * dx1, y1 + k * dy1, x2 - k * dx2, y2 - k * dy2, x2, y2);
            t = t2;
        }
    }

    // ── Painting ─────────────────────────────────────────────────────────────

    fn current_path(&self) -> Path {
        Path { els: self.path.els.clone() }
    }

    fn stroke_of(&self) -> Stroke {
        let k = self.state.transform.scale_factor();
        Stroke {
            color: self.state.stroke.with_alpha((self.state.stroke.a as f64 * self.state.alpha) as f32),
            width: self.state.line_width * k,
            dash: self.state.dash.iter().map(|d| d * k).collect(),
            dash_offset: self.state.dash_offset * k,
            cap: self.state.cap,
            join: self.state.join,
            miter_limit: self.state.miter_limit,
        }
    }

    fn fill_paint(&self) -> Rgba {
        self.state.fill.with_alpha((self.state.fill.a as f64 * self.state.alpha) as f32)
    }

    /// `fill()`: the current path with the fill style.
    pub fn fill(&mut self) {
        let p = self.current_path();
        if p.is_empty() {
            return;
        }
        let shadow = self.shadow();
        let c = self.fill_paint();
        self.surface.fill_path(&p, c, shadow.as_ref());
    }

    /// `stroke()`: the current path with the stroke style.
    pub fn stroke(&mut self) {
        let p = self.current_path();
        if p.is_empty() {
            return;
        }
        let shadow = self.shadow();
        let s = self.stroke_of();
        self.surface.stroke_path(&p, &s, shadow.as_ref());
    }

    /// `fill(new Path2D(d))` for a shape path: the current path is left alone.
    pub fn fill_shape_path(&mut self, path: &kubuno_office_shapes_core::Path) {
        let saved = std::mem::take(&mut self.path);
        self.add_shape_path(path);
        self.fill();
        self.path = saved;
    }

    /// `stroke(new Path2D(d))` for a shape path.
    pub fn stroke_shape_path(&mut self, path: &kubuno_office_shapes_core::Path) {
        let saved = std::mem::take(&mut self.path);
        self.add_shape_path(path);
        self.stroke();
        self.path = saved;
    }

    /// `fillRect(x, y, w, h)` (the current path is left alone).
    pub fn fill_rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        let saved = std::mem::take(&mut self.path);
        self.rect(x, y, w, h);
        self.fill();
        self.path = saved;
    }

    /// `strokeRect(x, y, w, h)`.
    pub fn stroke_rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        let saved = std::mem::take(&mut self.path);
        self.rect(x, y, w, h);
        self.stroke();
        self.path = saved;
    }

    /// `clearRect` over the whole drawing: the office code only clears the full canvas before painting,
    /// which a surface does when it begins a frame. Kept so the ported code reads the same.
    pub fn clear_rect(&mut self, _x: f64, _y: f64, _w: f64, _h: f64) {}

    /// `measureText(text)` in the current font.
    pub fn measure_text(&mut self, text: &str) -> TextMetrics {
        let font = self.state.font.clone();
        self.surface.measure_text(text, &font)
    }

    /// `fillText(text, x, y)` with the current alignment and baseline.
    pub fn fill_text(&mut self, text: &str, x: f64, y: f64) {
        if text.is_empty() || !(x.is_finite() && y.is_finite()) {
            return;
        }
        let font = self.state.font.clone();
        let m = self.surface.measure_text(text, &font);
        // The canvas' direction is left-to-right here: `start` is `left`, `end` is `right`.
        let dx = match self.state.align {
            TextAlign::Start | TextAlign::Left => 0.0,
            TextAlign::Center => -m.width / 2.0,
            TextAlign::End | TextAlign::Right => -m.width,
        };
        let dy = match self.state.baseline {
            TextBaseline::Alphabetic => 0.0,
            TextBaseline::Top | TextBaseline::Hanging => m.em_ascent,
            TextBaseline::Middle => (m.em_ascent - m.em_descent) / 2.0,
            TextBaseline::Bottom | TextBaseline::Ideographic => -m.em_descent,
        };
        let run = TextRun {
            text,
            font: &font,
            origin: Point::new(x + dx, y + dy),
            transform: self.state.transform,
            color: self.fill_paint(),
        };
        let shadow = self.shadow();
        self.surface.fill_text(&run, shadow.as_ref());
    }
}

// ── A recording surface, for tests and headless callers ──────────────────────

/// One recorded drawing operation.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Fill { path: Path, color: Rgba, shadow: Option<Shadow> },
    Stroke { path: Path, stroke: Stroke, shadow: Option<Shadow> },
    Text { text: String, font: Font, origin: Point, transform: Matrix, color: Rgba },
}

/// A [`Surface`] that records what is drawn and measures text with fixed metrics (each character
/// `0.5 × size` wide, em box 0.8/0.2 of the size) — deterministic on every platform.
#[derive(Debug, Default)]
pub struct Recorder {
    pub ops: Vec<Op>,
}

impl Recorder {
    pub fn fixed_metrics(text: &str, font: &Font) -> TextMetrics {
        TextMetrics {
            width: text.chars().count() as f64 * font.size * 0.5,
            em_ascent: font.size * 0.8,
            em_descent: font.size * 0.2,
        }
    }
}

impl Surface for Recorder {
    fn fill_path(&mut self, path: &Path, color: Rgba, shadow: Option<&Shadow>) {
        self.ops.push(Op::Fill { path: path.clone(), color, shadow: shadow.copied() });
    }

    fn stroke_path(&mut self, path: &Path, stroke: &Stroke, shadow: Option<&Shadow>) {
        self.ops.push(Op::Stroke { path: path.clone(), stroke: stroke.clone(), shadow: shadow.copied() });
    }

    fn fill_text(&mut self, run: &TextRun<'_>, _shadow: Option<&Shadow>) {
        self.ops.push(Op::Text {
            text: run.text.to_string(),
            font: run.font.clone(),
            origin: run.origin,
            transform: run.transform,
            color: run.color,
        });
    }

    fn measure_text(&mut self, text: &str, font: &Font) -> TextMetrics {
        Self::fixed_metrics(text, font)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn fonts_parse_like_the_css_shorthand() {
        let f = Font::parse("italic bold 14px \"Inter\", Arial, sans-serif").expect("font");
        assert!(f.italic && f.bold);
        assert_eq!(f.size, 14.0);
        assert_eq!(f.families, vec!["Inter", "Arial", "sans-serif"]);
        let f = Font::parse("12px Inter, sans-serif").expect("font");
        assert!(!f.bold && !f.italic && f.families[0] == "Inter");
        assert!(Font::parse("bold Inter").is_none());
        assert_eq!(Font::parse("600 9px Outfit").map(|f| f.bold), Some(true));
    }

    #[test]
    fn transforms_apply_to_points_when_they_are_added() {
        let mut r = Recorder::default();
        let mut c = Ctx2D::new(&mut r);
        c.translate(10.0, 20.0);
        c.scale(2.0, 2.0);
        c.begin_path();
        c.move_to(1.0, 1.0);
        c.reset_transform();
        c.line_to(5.0, 5.0);
        c.stroke();
        let Op::Stroke { path, stroke, .. } = &r.ops[0] else { panic!("a stroke") };
        assert_eq!(path.els[0], PathEl::MoveTo(Point::new(12.0, 22.0)));
        assert_eq!(path.els[1], PathEl::LineTo(Point::new(5.0, 5.0)));
        // Stroked with the identity in force: width 1.
        assert!(close(stroke.width, 1.0));
    }

    #[test]
    fn rect_closes_and_starts_a_new_sub_path() {
        let mut r = Recorder::default();
        let mut c = Ctx2D::new(&mut r);
        c.begin_path();
        c.rect(0.0, 0.0, 10.0, 5.0);
        c.fill();
        let Op::Fill { path, .. } = &r.ops[0] else { panic!("a fill") };
        assert_eq!(path.els.len(), 6);
        assert_eq!(path.els[4], PathEl::Close);
        assert_eq!(path.els[5], PathEl::MoveTo(Point::new(0.0, 0.0)));
    }

    #[test]
    fn a_full_arc_is_four_quarter_curves_and_the_sweep_follows_the_spec() {
        assert!(close(Ctx2D::sweep(0.0, TAU, false), TAU));
        assert!(close(Ctx2D::sweep(0.0, -FRAC_PI_2, false), 3.0 * FRAC_PI_2));
        assert!(close(Ctx2D::sweep(0.0, FRAC_PI_2, true), -3.0 * FRAC_PI_2));
        assert!(close(Ctx2D::sweep(1.0, 1.0, false), 0.0));
        let mut r = Recorder::default();
        let mut c = Ctx2D::new(&mut r);
        c.begin_path();
        c.arc(0.0, 0.0, 10.0, 0.0, TAU, false);
        c.fill();
        let Op::Fill { path, .. } = &r.ops[0] else { panic!("a fill") };
        assert_eq!(path.els.iter().filter(|e| matches!(e, PathEl::CubicTo(..))).count(), 4);
        let PathEl::CubicTo(_, _, end) = path.els[1] else { panic!("a curve") };
        // A quarter turn clockwise (y down) from (10, 0) is (0, 10).
        assert!(close(end.x, 0.0) && close(end.y, 10.0));
    }

    #[test]
    fn arc_to_rounds_a_right_angle_corner() {
        let mut r = Recorder::default();
        let mut c = Ctx2D::new(&mut r);
        c.begin_path();
        c.move_to(0.0, 0.0);
        c.arc_to(10.0, 0.0, 10.0, 10.0, 4.0);
        c.stroke();
        let Op::Stroke { path, .. } = &r.ops[0] else { panic!("a stroke") };
        // A line to the first tangent point (6, 0), then a quarter arc to (10, 4).
        let PathEl::LineTo(t1) = path.els[1] else { panic!("a line") };
        assert!(close(t1.x, 6.0) && close(t1.y, 0.0));
        let PathEl::CubicTo(_, _, end) = *path.els.last().expect("the arc") else { panic!("a curve") };
        assert!(close(end.x, 10.0) && close(end.y, 4.0), "{end:?}");
    }

    #[test]
    fn svg_arcs_reach_their_end_point() {
        let mut r = Recorder::default();
        let mut c = Ctx2D::new(&mut r);
        let mut p = kubuno_office_shapes_core::Path::new();
        p.move_to(0.0, 50.0);
        p.cmds.push(kubuno_office_shapes_core::Cmd::ArcTo { rx: 50.0, ry: 50.0, large: false, sweep: true, x: 100.0, y: 50.0 });
        c.fill_shape_path(&p);
        let Op::Fill { path, .. } = &r.ops[0] else { panic!("a fill") };
        let PathEl::CubicTo(_, mid, end) = path.els[1] else { panic!("a curve") };
        assert!(close(end.x, 50.0) && close(end.y, 0.0), "the top of the circle first (sweep 1 goes through y < 50): {end:?}");
        assert!(mid.y < 50.0);
        let PathEl::CubicTo(_, _, last) = *path.els.last().expect("arc") else { panic!("a curve") };
        assert!(close(last.x, 100.0) && close(last.y, 50.0));
    }

    #[test]
    fn dashes_double_when_odd_and_scale_with_the_transform() {
        let mut r = Recorder::default();
        let mut c = Ctx2D::new(&mut r);
        c.set_line_dash(&[2.0]);
        c.scale(3.0, 3.0);
        c.set_line_width(2.0);
        c.begin_path();
        c.move_to(0.0, 0.0);
        c.line_to(1.0, 0.0);
        c.stroke();
        let Op::Stroke { stroke, .. } = &r.ops[0] else { panic!("a stroke") };
        assert_eq!(stroke.dash, vec![6.0, 6.0]);
        assert!(close(stroke.width, 6.0));
    }

    #[test]
    fn text_alignment_and_baseline_move_the_origin() {
        let mut r = Recorder::default();
        let mut c = Ctx2D::new(&mut r);
        c.set_font("10px Inter");
        c.set_text_align(TextAlign::Center);
        c.set_text_baseline(TextBaseline::Middle);
        c.fill_text("abcd", 100.0, 50.0);
        let Op::Text { origin, .. } = &r.ops[0] else { panic!("text") };
        // width 20 → left at 90; middle: baseline 3 below the middle (ascent 8, descent 2).
        assert!(close(origin.x, 90.0) && close(origin.y, 53.0), "{origin:?}");
    }

    #[test]
    fn global_alpha_and_shadows_reach_the_surface() {
        let mut r = Recorder::default();
        let mut c = Ctx2D::new(&mut r);
        c.set_global_alpha(0.5);
        c.set_fill_style("#ff0000");
        c.set_shadow_color("rgba(0,0,0,0.25)");
        c.set_shadow_blur(5.0);
        c.set_shadow_offset(2.0, 2.0);
        c.fill_rect(0.0, 0.0, 1.0, 1.0);
        let Op::Fill { color, shadow, .. } = &r.ops[0] else { panic!("a fill") };
        assert!(close(color.a as f64, 0.5));
        let s = shadow.expect("a shadow");
        assert!(close(s.color.a as f64, 0.125) && s.blur == 5.0);
    }
}
