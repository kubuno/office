//! The Direct2D implementation of the board renderer's [`Surface`]
//! (`kubuno_office_whiteboard_core::render::Surface`).
//!
//! The renderer is an immediate-mode Canvas 2D (begin_path / move_to / … / fill / stroke, a current fill,
//! stroke, line width, dash, font and transform). This surface keeps that state, builds a Direct2D path
//! geometry on `fill`/`stroke`, and draws text with DirectWrite — the Windows half of the web's `<canvas>`.
//! Coordinates are the window's DIPs (the web's CSS pixels); the control hands us a base transform that
//! includes its own offset, and the viewport transform the renderer sets scales strokes and dashes exactly
//! as the web canvas does.

use std::cell::RefCell;
use std::collections::HashMap;

use kubuno_office_whiteboard_core::render::{Surface, TextAlign, TextBaseline};
use windows::core::{Interface, HSTRING};
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows_numerics::{Matrix3x2, Vector2};

const UNBOUNDED: f32 = 65536.0;

/// A Direct2D point (the renderer works in f64; Direct2D in f32).
fn v2(x: f64, y: f64) -> Vector2 {
    Vector2 { X: x as f32, Y: y as f32 }
}

/// One accumulated path command, in content space.
enum El {
    Move(f64, f64),
    Line(f64, f64),
    Cubic(f64, f64, f64, f64, f64, f64),
    Quad(f64, f64, f64, f64),
    Arc { rx: f64, ry: f64, large: bool, sweep: bool, x: f64, y: f64 },
    Close,
}

/// A colour, plus whether it paints at all (`none`/`transparent` do not).
#[derive(Clone, Copy)]
struct Paint {
    color: D2D1_COLOR_F,
    visible: bool,
}

impl Paint {
    fn black() -> Paint {
        Paint { color: D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }, visible: true }
    }
}

/// The graphics state saved by `save`/`restore` (the current path is **not** part of it, as in canvas).
#[derive(Clone)]
struct GState {
    transform: Matrix3x2,
    fill: Paint,
    stroke: Paint,
    line_width: f64,
    dash: Vec<f32>,
    alpha: f64,
    shadow: Option<(D2D1_COLOR_F, f64, f64)>,
    font_px: f64,
    font_bold: bool,
    font_family: String,
    baseline: TextBaseline,
    align: TextAlign,
}

/// Per-frame caches that outlive the surface (owned by the control).
#[derive(Default)]
pub struct SurfaceCache {
    formats: RefCell<HashMap<(u32, bool, String), IDWriteTextFormat>>,
    widths: RefCell<HashMap<(u32, bool, String, String), f32>>,
    owner: RefCell<usize>,
}

impl SurfaceCache {
    fn check_owner(&self, dwrite: &IDWriteFactory) {
        let key = dwrite.as_raw() as usize;
        if *self.owner.borrow() != key {
            self.formats.borrow_mut().clear();
            self.widths.borrow_mut().clear();
            *self.owner.borrow_mut() = key;
        }
    }
}

/// The surface over one frame of a Direct2D device context.
pub struct D2DSurface<'a> {
    ctx: &'a ID2D1DeviceContext,
    factory: &'a ID2D1Factory1,
    dwrite: &'a IDWriteFactory,
    cache: &'a SurfaceCache,
    brush: ID2D1SolidColorBrush,
    base: Matrix3x2,
    state: GState,
    stack: Vec<GState>,
    path: Vec<El>,
}

impl<'a> D2DSurface<'a> {
    /// A surface drawing on `ctx`. `base` is the control's device transform (its offset).
    pub fn new(ctx: &'a ID2D1DeviceContext, factory: &'a ID2D1Factory1, dwrite: &'a IDWriteFactory, cache: &'a SurfaceCache, base: Matrix3x2) -> windows::core::Result<Self> {
        cache.check_owner(dwrite);
        let brush = unsafe { ctx.CreateSolidColorBrush(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }, None)? };
        let state = GState {
            transform: base,
            fill: Paint::black(),
            stroke: Paint::black(),
            line_width: 1.0,
            dash: Vec::new(),
            alpha: 1.0,
            shadow: None,
            font_px: 14.0,
            font_bold: false,
            font_family: "Arial".into(),
            baseline: TextBaseline::Top,
            align: TextAlign::Left,
        };
        Ok(Self { ctx, factory, dwrite, cache, brush, base, state, stack: Vec::new(), path: Vec::new() })
    }

    fn brush_for(&self, mut c: D2D1_COLOR_F) -> &ID2D1SolidColorBrush {
        c.a *= self.state.alpha as f32;
        unsafe { self.brush.SetColor(&c) };
        &self.brush
    }

    fn set_ctx_transform(&self, t: &Matrix3x2) {
        unsafe { self.ctx.SetTransform(t) };
    }

    /// Builds the current path into a Direct2D geometry (`filled` picks the figure begin flag).
    fn build_geometry(&self, filled: bool) -> Option<ID2D1PathGeometry1> {
        unsafe {
            let geom = self.factory.CreatePathGeometry().ok()?;
            let sink = geom.Open().ok()?;
            sink.SetFillMode(D2D1_FILL_MODE_WINDING);
            let begin = if filled { D2D1_FIGURE_BEGIN_FILLED } else { D2D1_FIGURE_BEGIN_HOLLOW };
            let mut open = false;
            for el in &self.path {
                match *el {
                    El::Move(x, y) => {
                        if open {
                            sink.EndFigure(D2D1_FIGURE_END_OPEN);
                        }
                        sink.BeginFigure(v2(x, y), begin);
                        open = true;
                    }
                    El::Line(x, y) => {
                        if open {
                            sink.AddLine(v2(x, y));
                        }
                    }
                    El::Cubic(a, b, c, d, e, f) => {
                        if open {
                            sink.AddBezier(&D2D1_BEZIER_SEGMENT { point1: v2(a, b), point2: v2(c, d), point3: v2(e, f) });
                        }
                    }
                    El::Quad(cx, cy, x, y) => {
                        if open {
                            sink.AddQuadraticBezier(&D2D1_QUADRATIC_BEZIER_SEGMENT { point1: v2(cx, cy), point2: v2(x, y) });
                        }
                    }
                    El::Arc { rx, ry, large, sweep, x, y } => {
                        if open {
                            sink.AddArc(&D2D1_ARC_SEGMENT {
                                point: v2(x, y),
                                size: D2D_SIZE_F { width: rx as f32, height: ry as f32 },
                                rotationAngle: 0.0,
                                sweepDirection: if sweep { D2D1_SWEEP_DIRECTION_CLOCKWISE } else { D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE },
                                arcSize: if large { D2D1_ARC_SIZE_LARGE } else { D2D1_ARC_SIZE_SMALL },
                            });
                        }
                    }
                    El::Close => {
                        if open {
                            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                            open = false;
                        }
                    }
                }
            }
            if open {
                sink.EndFigure(D2D1_FIGURE_END_OPEN);
            }
            sink.Close().ok()?;
            Some(geom)
        }
    }

    fn stroke_style(&self) -> Option<ID2D1StrokeStyle> {
        let w = self.state.line_width.max(1e-6) as f32;
        let dashes: Vec<f32> = self.state.dash.iter().map(|d| d / w).collect();
        let props = D2D1_STROKE_STYLE_PROPERTIES {
            startCap: D2D1_CAP_STYLE_ROUND,
            endCap: D2D1_CAP_STYLE_ROUND,
            dashCap: D2D1_CAP_STYLE_ROUND,
            lineJoin: D2D1_LINE_JOIN_ROUND,
            miterLimit: 10.0,
            dashStyle: if dashes.is_empty() { D2D1_DASH_STYLE_SOLID } else { D2D1_DASH_STYLE_CUSTOM },
            dashOffset: 0.0,
        };
        let base: ID2D1Factory = self.factory.cast().ok()?;
        unsafe { base.CreateStrokeStyle(&props, if dashes.is_empty() { None } else { Some(&dashes) }).ok() }
    }

    fn text_format(&self) -> Option<IDWriteTextFormat> {
        let key = ((self.state.font_px * 100.0).round() as u32, self.state.font_bold, self.state.font_family.clone());
        if let Some(f) = self.cache.formats.borrow().get(&key) {
            return Some(f.clone());
        }
        let weight = if self.state.font_bold { DWRITE_FONT_WEIGHT_BOLD } else { DWRITE_FONT_WEIGHT_NORMAL };
        let fmt = unsafe {
            let f = self
                .dwrite
                .CreateTextFormat(&HSTRING::from(self.state.font_family.as_str()), None, weight, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, self.state.font_px as f32, &HSTRING::from("fr-FR"))
                .ok()?;
            let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
            f
        };
        self.cache.formats.borrow_mut().insert(key, fmt.clone());
        Some(fmt)
    }

    /// Draws a geometry (and its shadow, if any) filled or stroked under the current transform.
    fn draw_geometry(&self, geom: &ID2D1PathGeometry1, fill: bool) {
        let t = self.state.transform;
        if let Some((sc, ox, oy)) = self.state.shadow {
            let shifted = compose(&t, &Matrix3x2 { M11: 1.0, M12: 0.0, M21: 0.0, M22: 1.0, M31: ox as f32, M32: oy as f32 });
            self.set_ctx_transform(&shifted);
            let brush = self.brush_for(sc);
            unsafe {
                if fill {
                    self.ctx.FillGeometry(geom, brush, None);
                } else {
                    self.ctx.DrawGeometry(geom, brush, self.state.line_width as f32, self.stroke_style().as_ref());
                }
            }
        }
        self.set_ctx_transform(&t);
        unsafe {
            if fill {
                let b = self.brush_for(self.state.fill.color);
                self.ctx.FillGeometry(geom, b, None);
            } else {
                let b = self.brush_for(self.state.stroke.color);
                self.ctx.DrawGeometry(geom, b, self.state.line_width as f32, self.stroke_style().as_ref());
            }
        }
    }
}

impl Surface for D2DSurface<'_> {
    fn save(&mut self) {
        self.stack.push(self.state.clone());
    }
    fn restore(&mut self) {
        if let Some(s) = self.stack.pop() {
            self.state = s;
        }
    }

    fn set_transform(&mut self, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) {
        let user = Matrix3x2 { M11: a as f32, M12: b as f32, M21: c as f32, M22: d as f32, M31: e as f32, M32: f as f32 };
        self.state.transform = compose(&user, &self.base);
    }
    fn translate(&mut self, x: f64, y: f64) {
        let tr = Matrix3x2 { M11: 1.0, M12: 0.0, M21: 0.0, M22: 1.0, M31: x as f32, M32: y as f32 };
        self.state.transform = compose(&tr, &self.state.transform);
    }
    fn rotate(&mut self, radians: f64) {
        let (s, c) = (radians.sin() as f32, radians.cos() as f32);
        let rot = Matrix3x2 { M11: c, M12: s, M21: -s, M22: c, M31: 0.0, M32: 0.0 };
        self.state.transform = compose(&rot, &self.state.transform);
    }

    fn set_fill(&mut self, css: &str) {
        self.state.fill = parse_paint(css);
    }
    fn set_stroke(&mut self, css: &str) {
        self.state.stroke = parse_paint(css);
    }
    fn set_line_width(&mut self, w: f64) {
        self.state.line_width = w;
    }
    fn set_line_dash(&mut self, segments: &[f64]) {
        self.state.dash = segments.iter().map(|d| *d as f32).collect();
    }
    fn set_global_alpha(&mut self, a: f64) {
        self.state.alpha = a.clamp(0.0, 1.0);
    }
    fn set_shadow(&mut self, css: &str, _blur: f64, offset_y: f64) {
        // The canvas blurs the shadow; here it is the silhouette offset in the shadow colour, unblurred.
        self.state.shadow = Some((parse_paint(css).color, 0.0, offset_y));
    }
    fn clear_shadow(&mut self) {
        self.state.shadow = None;
    }

    fn set_font(&mut self, px: f64, bold: bool, family: &str) {
        self.state.font_px = px;
        self.state.font_bold = bold;
        self.state.font_family = first_family(family);
    }
    fn set_text_baseline(&mut self, b: TextBaseline) {
        self.state.baseline = b;
    }
    fn set_text_align(&mut self, a: TextAlign) {
        self.state.align = a;
    }

    fn begin_path(&mut self) {
        self.path.clear();
    }
    fn move_to(&mut self, x: f64, y: f64) {
        self.path.push(El::Move(x, y));
    }
    fn line_to(&mut self, x: f64, y: f64) {
        self.path.push(El::Line(x, y));
    }
    fn bezier_curve_to(&mut self, c1x: f64, c1y: f64, c2x: f64, c2y: f64, x: f64, y: f64) {
        self.path.push(El::Cubic(c1x, c1y, c2x, c2y, x, y));
    }
    fn quadratic_curve_to(&mut self, cx: f64, cy: f64, x: f64, y: f64) {
        self.path.push(El::Quad(cx, cy, x, y));
    }
    fn arc_svg(&mut self, rx: f64, ry: f64, large: bool, sweep: bool, x: f64, y: f64) {
        self.path.push(El::Arc { rx, ry, large, sweep, x, y });
    }
    fn arc(&mut self, cx: f64, cy: f64, r: f64, _start: f64, _end: f64) {
        // A full circle as two SVG half-arcs (the board only draws full circles here: dots, knobs).
        self.path.push(El::Move(cx + r, cy));
        self.path.push(El::Arc { rx: r, ry: r, large: false, sweep: true, x: cx - r, y: cy });
        self.path.push(El::Arc { rx: r, ry: r, large: false, sweep: true, x: cx + r, y: cy });
        self.path.push(El::Close);
    }
    fn round_rect(&mut self, x: f64, y: f64, w: f64, h: f64, radius: f64) {
        let r = radius.min(w / 2.0).min(h / 2.0).max(0.0);
        self.path.push(El::Move(x + r, y));
        self.path.push(El::Line(x + w - r, y));
        self.path.push(El::Arc { rx: r, ry: r, large: false, sweep: true, x: x + w, y: y + r });
        self.path.push(El::Line(x + w, y + h - r));
        self.path.push(El::Arc { rx: r, ry: r, large: false, sweep: true, x: x + w - r, y: y + h });
        self.path.push(El::Line(x + r, y + h));
        self.path.push(El::Arc { rx: r, ry: r, large: false, sweep: true, x, y: y + h - r });
        self.path.push(El::Line(x, y + r));
        self.path.push(El::Arc { rx: r, ry: r, large: false, sweep: true, x: x + r, y });
        self.path.push(El::Close);
    }
    fn close_path(&mut self) {
        self.path.push(El::Close);
    }
    fn fill(&mut self) {
        if !self.state.fill.visible && self.state.shadow.is_none() {
            return;
        }
        if let Some(geom) = self.build_geometry(true) {
            self.draw_geometry(&geom, true);
        }
    }
    fn stroke(&mut self) {
        if !self.state.stroke.visible || self.state.line_width <= 0.0 {
            return;
        }
        if let Some(geom) = self.build_geometry(false) {
            self.draw_geometry(&geom, false);
        }
    }

    fn fill_rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        if !self.state.fill.visible {
            return;
        }
        self.set_ctx_transform(&self.state.transform);
        let rect = D2D_RECT_F { left: x as f32, top: y as f32, right: (x + w) as f32, bottom: (y + h) as f32 };
        let b = self.brush_for(self.state.fill.color);
        unsafe { self.ctx.FillRectangle(&rect, b) };
    }
    fn stroke_rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        if !self.state.stroke.visible || self.state.line_width <= 0.0 {
            return;
        }
        self.set_ctx_transform(&self.state.transform);
        let rr = D2D1_ROUNDED_RECT { rect: D2D_RECT_F { left: x as f32, top: y as f32, right: (x + w) as f32, bottom: (y + h) as f32 }, radiusX: 0.0, radiusY: 0.0 };
        let style = self.stroke_style();
        let b = self.brush_for(self.state.stroke.color);
        unsafe { self.ctx.DrawRoundedRectangle(&rr, b, self.state.line_width as f32, style.as_ref()) };
    }

    fn fill_text(&mut self, text: &str, x: f64, y: f64) {
        if !self.state.fill.visible || text.is_empty() {
            return;
        }
        let Some(fmt) = self.text_format() else { return };
        let wide: Vec<u16> = text.encode_utf16().collect();
        let layout = match unsafe { self.dwrite.CreateTextLayout(&wide, &fmt, UNBOUNDED, UNBOUNDED) } {
            Ok(l) => l,
            Err(_) => return,
        };
        let mut tm = DWRITE_TEXT_METRICS::default();
        unsafe {
            let _ = layout.GetMetrics(&mut tm);
        };
        let ox = match self.state.align {
            TextAlign::Left => x,
            TextAlign::Center => x - tm.width as f64 / 2.0,
            TextAlign::Right => x - tm.width as f64,
        };
        let oy = match self.state.baseline {
            TextBaseline::Top => y,
            TextBaseline::Bottom => y - tm.height as f64,
        };
        self.set_ctx_transform(&self.state.transform);
        let b = self.brush_for(self.state.fill.color);
        unsafe { self.ctx.DrawTextLayout(v2(ox, oy), &layout, b, D2D1_DRAW_TEXT_OPTIONS_NONE) };
    }

    fn measure_text(&mut self, text: &str) -> f64 {
        let key = ((self.state.font_px * 100.0).round() as u32, self.state.font_bold, self.state.font_family.clone(), text.to_string());
        if let Some(&w) = self.cache.widths.borrow().get(&key) {
            return w as f64;
        }
        let Some(fmt) = self.text_format() else { return text.chars().count() as f64 * self.state.font_px * 0.5 };
        let wide: Vec<u16> = text.encode_utf16().collect();
        let width = unsafe {
            match self.dwrite.CreateTextLayout(&wide, &fmt, UNBOUNDED, UNBOUNDED) {
                Ok(layout) => {
                    let mut tm = DWRITE_TEXT_METRICS::default();
                    if layout.GetMetrics(&mut tm).is_ok() {
                        tm.widthIncludingTrailingWhitespace
                    } else {
                        0.0
                    }
                }
                Err(_) => 0.0,
            }
        };
        self.cache.widths.borrow_mut().insert(key, width);
        width as f64
    }
}

/// `a` applied first, then `b` (row-vector convention, like the Direct2D transforms).
pub fn compose(a: &Matrix3x2, b: &Matrix3x2) -> Matrix3x2 {
    Matrix3x2 {
        M11: a.M11 * b.M11 + a.M12 * b.M21,
        M12: a.M11 * b.M12 + a.M12 * b.M22,
        M21: a.M21 * b.M11 + a.M22 * b.M21,
        M22: a.M21 * b.M12 + a.M22 * b.M22,
        M31: a.M31 * b.M11 + a.M32 * b.M21 + b.M31,
        M32: a.M31 * b.M12 + a.M32 * b.M22 + b.M32,
    }
}

/// The first family of a CSS list, generics mapped to a Windows face (Arial for sans-serif).
fn first_family(css: &str) -> String {
    for part in css.split(',') {
        let f = part.trim().trim_matches('"').trim_matches('\'').trim();
        if f.is_empty() {
            continue;
        }
        return match f.to_ascii_lowercase().as_str() {
            "sans-serif" | "system-ui" => "Arial".into(),
            "serif" => "Times New Roman".into(),
            "monospace" => "Consolas".into(),
            other if other.contains("arial") => "Arial".into(),
            _ => f.to_string(),
        };
    }
    "Arial".into()
}

/// Parses a CSS colour the board writes: `#rgb`, `#rrggbb`, `#rrggbbaa`, `rgb(...)`, `rgba(...)`, and
/// `none`/`transparent` (which do not paint). A black opaque fallback keeps a shape visible.
fn parse_paint(css: &str) -> Paint {
    let s = css.trim();
    if s.eq_ignore_ascii_case("none") || s.eq_ignore_ascii_case("transparent") {
        return Paint { color: D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }, visible: false };
    }
    if let Some(hex) = s.strip_prefix('#') {
        let v = |c: &str| u8::from_str_radix(c, 16).unwrap_or(0) as f32 / 255.0;
        let (r, g, b, a) = match hex.len() {
            3 => (v(&dup(&hex[0..1])), v(&dup(&hex[1..2])), v(&dup(&hex[2..3])), 1.0),
            6 => (v(&hex[0..2]), v(&hex[2..4]), v(&hex[4..6]), 1.0),
            8 => (v(&hex[0..2]), v(&hex[2..4]), v(&hex[4..6]), v(&hex[6..8])),
            _ => (0.0, 0.0, 0.0, 1.0),
        };
        return Paint { color: D2D1_COLOR_F { r, g, b, a }, visible: a > 0.0 };
    }
    if let Some(inner) = s.strip_prefix("rgba(").or_else(|| s.strip_prefix("rgb(")).and_then(|x| x.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        let n = |i: usize| parts.get(i).and_then(|p| p.parse::<f32>().ok());
        let r = n(0).unwrap_or(0.0) / 255.0;
        let g = n(1).unwrap_or(0.0) / 255.0;
        let b = n(2).unwrap_or(0.0) / 255.0;
        let a = n(3).unwrap_or(1.0);
        return Paint { color: D2D1_COLOR_F { r, g, b, a }, visible: a > 0.0 };
    }
    Paint::black()
}

fn dup(c: &str) -> String {
    format!("{c}{c}")
}
