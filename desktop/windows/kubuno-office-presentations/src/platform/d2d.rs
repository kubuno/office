//! The Windows half of the slide renderer: [`D2dSurface`] draws what `kubuno_office_slides_core::render` asks
//! (paths, gradients, text runs, images, clips, shadows, filters) with Direct2D on the window's device context,
//! and [`Fonts`] measures text with DirectWrite for the core's line breaking — the canvas `measureText` of the
//! web.
//!
//! Each call sets the device context's transform to the call's matrix and puts the previous one back, so it
//! interleaves with the framework's own painting in the same `BeginDraw`.

use std::cell::RefCell;
use std::collections::HashMap;

use kubuno_drive_desktop_app_controls::Renderer;
use kubuno_office_slides_core::color::Rgba;
use kubuno_office_slides_core::model::Rect;
use kubuno_office_slides_core::richtext::{Measure, Style};
use kubuno_office_slides_core::surface::{Baseline, Filters, Font, LineCap, LineJoin, Matrix, Paint, Shadow, State, Stroke, Surface};
use kubuno_office_slides_core::{Cmd, Path};
use windows::core::{Interface, HSTRING};
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows_numerics::{Matrix3x2, Vector2};

use super::images::{BitmapCache, ImageStore};

const UNBOUNDED: f32 = 65_536.0;

fn v2(x: f64, y: f64) -> Vector2 {
    Vector2 { X: x as f32, Y: y as f32 }
}

fn m3(m: &Matrix) -> Matrix3x2 {
    Matrix3x2 { M11: m.a as f32, M12: m.b as f32, M21: m.c as f32, M22: m.d as f32, M31: m.e as f32, M32: m.f as f32 }
}

fn color(c: Rgba, alpha: f64) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: c.r as f32, g: c.g as f32, b: c.b as f32, a: (c.a * alpha) as f32 }
}

/// The first family of a CSS list that this system has, with the browser's generic families mapped to what
/// Chrome uses on Windows (sans-serif → Arial, serif → Times New Roman, monospace → Consolas).
pub fn resolve_family(dwrite: &IDWriteFactory, css: &str) -> String {
    thread_local! {
        static CACHE: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
    }
    if let Some(hit) = CACHE.with(|c| c.borrow().get(css).cloned()) {
        return hit;
    }
    let collection = unsafe {
        let mut c = None;
        dwrite.GetSystemFontCollection(&mut c, false).ok().and(c)
    };
    let has = |name: &str| -> bool {
        let Some(coll) = collection.as_ref() else { return false };
        let (mut index, mut exists) = (0u32, windows::core::BOOL(0));
        unsafe { coll.FindFamilyName(&HSTRING::from(name), &mut index, &mut exists).is_ok() && exists.as_bool() }
    };
    let mut chosen = None;
    for part in css.split(',') {
        let name = part.trim().trim_matches(|c| c == '"' || c == '\'').trim();
        let name = match name.to_ascii_lowercase().as_str() {
            "sans-serif" | "system-ui" | "-apple-system" => "Arial".to_string(),
            "serif" => "Times New Roman".to_string(),
            "monospace" => "Consolas".to_string(),
            "cursive" => "Comic Sans MS".to_string(),
            _ => name.to_string(),
        };
        if !name.is_empty() && has(&name) {
            chosen = Some(name);
            break;
        }
    }
    // Nothing installed: the browser falls back to its default font, Times New Roman on Windows.
    let chosen = chosen.unwrap_or_else(|| "Times New Roman".to_string());
    CACHE.with(|c| c.borrow_mut().insert(css.to_string(), chosen.clone()));
    chosen
}

/// DirectWrite text formats and measures, cached.
/// A measured run: family, size, bold, italic, letter spacing (×100), text.
type WidthKey = (String, u32, bool, bool, u32, String);

pub struct Fonts {
    pub dwrite: IDWriteFactory,
    formats: RefCell<HashMap<(String, u32, bool, bool), IDWriteTextFormat>>,
    widths: RefCell<HashMap<WidthKey, f64>>,
}

impl Fonts {
    pub fn new(dwrite: IDWriteFactory) -> Fonts {
        Fonts { dwrite, formats: RefCell::default(), widths: RefCell::default() }
    }

    pub fn format(&self, font: &Font) -> Option<IDWriteTextFormat> {
        let size = (font.size.max(0.01) * 100.0).round() as u32;
        let key = (font.family.clone(), size, font.bold, font.italic);
        if let Some(f) = self.formats.borrow().get(&key) {
            return Some(f.clone());
        }
        let family = resolve_family(&self.dwrite, &font.family);
        let f = unsafe {
            self.dwrite
                .CreateTextFormat(
                    &HSTRING::from(family),
                    None,
                    if font.bold { DWRITE_FONT_WEIGHT_BOLD } else { DWRITE_FONT_WEIGHT_NORMAL },
                    if font.italic { DWRITE_FONT_STYLE_ITALIC } else { DWRITE_FONT_STYLE_NORMAL },
                    DWRITE_FONT_STRETCH_NORMAL,
                    font.size.max(0.01) as f32,
                    &HSTRING::from("fr-fr"),
                )
                .ok()?
        };
        unsafe {
            let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
        }
        self.formats.borrow_mut().insert(key, f.clone());
        Some(f)
    }

    /// A one-line layout of `text`, letter spacing applied.
    pub fn layout(&self, text: &str, font: &Font, ls: f64) -> Option<IDWriteTextLayout> {
        let format = self.format(font)?;
        let wide: Vec<u16> = text.encode_utf16().collect();
        unsafe {
            let layout = self.dwrite.CreateTextLayout(&wide, &format, UNBOUNDED, UNBOUNDED).ok()?;
            if ls != 0.0 {
                if let Ok(l1) = layout.cast::<IDWriteTextLayout1>() {
                    let _ = l1.SetCharacterSpacing(0.0, ls as f32, 0.0, DWRITE_TEXT_RANGE { startPosition: 0, length: wide.len() as u32 });
                }
            }
            Some(layout)
        }
    }

    /// The distance from the layout's top to its first baseline.
    pub fn baseline(layout: &IDWriteTextLayout) -> f32 {
        unsafe {
            let mut lines = [DWRITE_LINE_METRICS::default(); 1];
            let mut count = 0u32;
            let _ = layout.GetLineMetrics(Some(&mut lines), &mut count);
            lines[0].baseline
        }
    }

    /// The font's ascent and descent at `font.size` (design metrics).
    pub fn ascent_descent(&self, font: &Font) -> (f64, f64) {
        let Some(layout) = self.layout("Hg", font, 0.0) else { return (font.size * 0.9, font.size * 0.25) };
        unsafe {
            let mut lines = [DWRITE_LINE_METRICS::default(); 1];
            let mut count = 0u32;
            let _ = layout.GetLineMetrics(Some(&mut lines), &mut count);
            (lines[0].baseline as f64, (lines[0].height - lines[0].baseline) as f64)
        }
    }

    fn width_of(&self, text: &str, font: &Font, ls: f64) -> f64 {
        let key = (font.family.clone(), (font.size * 100.0).round() as u32, font.bold, font.italic, (ls * 100.0).round() as u32, text.to_string());
        if let Some(w) = self.widths.borrow().get(&key) {
            return *w;
        }
        let w = self
            .layout(text, font, ls)
            .and_then(|l| unsafe {
                let mut m = DWRITE_TEXT_METRICS::default();
                l.GetMetrics(&mut m).ok().map(|_| m.widthIncludingTrailingWhitespace as f64)
            })
            .unwrap_or(0.0);
        let mut cache = self.widths.borrow_mut();
        if cache.len() > 20_000 {
            cache.clear();
        }
        cache.insert(key, w);
        w
    }
}

impl Measure for Fonts {
    fn width(&self, text: &str, style: &Style, ls: f64) -> f64 {
        if text.is_empty() {
            return 0.0;
        }
        self.width_of(text, &Font { family: style.family.clone(), size: style.size, bold: style.bold, italic: style.italic }, ls)
    }
}

enum Pushed {
    Axis,
    Layer,
}

/// The slide renderer's surface on a Direct2D device context.
pub struct D2dSurface<'a> {
    factory: ID2D1Factory1,
    pub fonts: &'a Fonts,
    pub store: &'a ImageStore,
    pub bitmaps: &'a mut BitmapCache,
    ctx: ID2D1DeviceContext,
    /// The transform the context had before (restored after every call).
    outer: Matrix3x2,
    clips: Vec<Pushed>,
    /// The last image drawn (its bitmap, source and destination rectangles, state), for the `multiply` tint
    /// the renderer lays over it.
    last_image: Option<(ID2D1Bitmap1, D2D_RECT_F, D2D_RECT_F, State)>,
}

impl<'a> D2dSurface<'a> {
    pub fn new(renderer: &Renderer, fonts: &'a Fonts, store: &'a ImageStore, bitmaps: &'a mut BitmapCache) -> D2dSurface<'a> {
        D2dSurface::with_context(renderer.d2d_context.clone(), renderer.d2d_factory.clone(), fonts, store, bitmaps)
    }

    /// A surface over any device context (an off-screen bitmap for the exports).
    pub fn with_context(ctx: ID2D1DeviceContext, factory: ID2D1Factory1, fonts: &'a Fonts, store: &'a ImageStore, bitmaps: &'a mut BitmapCache) -> D2dSurface<'a> {
        let mut outer = Matrix3x2::identity();
        unsafe { ctx.GetTransform(&mut outer) };
        D2dSurface { factory, fonts, store, bitmaps, ctx, outer, clips: Vec::new(), last_image: None }
    }

    /// The device context's transform before the surface was made (the caller's base matrix starts from it).
    pub fn outer(&self) -> Matrix {
        let o = self.outer;
        Matrix { a: o.M11 as f64, b: o.M12 as f64, c: o.M21 as f64, d: o.M22 as f64, e: o.M31 as f64, f: o.M32 as f64 }
    }

    fn set(&self, m: &Matrix) {
        unsafe { self.ctx.SetTransform(&m3(m)) };
    }

    fn restore(&self) {
        unsafe { self.ctx.SetTransform(&self.outer) };
    }

    fn geometry(&self, path: &Path) -> Option<ID2D1PathGeometry1> {
        unsafe {
            let g = self.factory.CreatePathGeometry().ok()?;
            let sink = g.Open().ok()?;
            sink.SetFillMode(D2D1_FILL_MODE_WINDING);
            let mut open = false;
            let (mut cur, mut start) = ((0.0f64, 0.0f64), (0.0f64, 0.0f64));
            let begin = |sink: &ID2D1GeometrySink, open: &mut bool, at: (f64, f64)| {
                if !*open {
                    sink.BeginFigure(v2(at.0, at.1), D2D1_FIGURE_BEGIN_FILLED);
                    *open = true;
                }
            };
            for c in &path.cmds {
                match *c {
                    Cmd::MoveTo(x, y) => {
                        if open {
                            sink.EndFigure(D2D1_FIGURE_END_OPEN);
                        }
                        cur = (x, y);
                        start = cur;
                        sink.BeginFigure(v2(x, y), D2D1_FIGURE_BEGIN_FILLED);
                        open = true;
                    }
                    Cmd::LineTo(x, y) => {
                        begin(&sink, &mut open, cur);
                        sink.AddLine(v2(x, y));
                        cur = (x, y);
                    }
                    Cmd::CubicTo(a, b, c2, d, x, y) => {
                        begin(&sink, &mut open, cur);
                        sink.AddBezier(&D2D1_BEZIER_SEGMENT { point1: v2(a, b), point2: v2(c2, d), point3: v2(x, y) });
                        cur = (x, y);
                    }
                    Cmd::QuadTo(a, b, x, y) => {
                        begin(&sink, &mut open, cur);
                        sink.AddQuadraticBezier(&D2D1_QUADRATIC_BEZIER_SEGMENT { point1: v2(a, b), point2: v2(x, y) });
                        cur = (x, y);
                    }
                    Cmd::ArcTo { rx, ry, large, sweep, x, y } => {
                        begin(&sink, &mut open, cur);
                        sink.AddArc(&D2D1_ARC_SEGMENT {
                            point: v2(x, y),
                            size: D2D_SIZE_F { width: rx.max(0.0) as f32, height: ry.max(0.0) as f32 },
                            rotationAngle: 0.0,
                            sweepDirection: if sweep { D2D1_SWEEP_DIRECTION_CLOCKWISE } else { D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE },
                            arcSize: if large { D2D1_ARC_SIZE_LARGE } else { D2D1_ARC_SIZE_SMALL },
                        });
                        cur = (x, y);
                    }
                    Cmd::Close => {
                        if open {
                            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                            open = false;
                        }
                        cur = start;
                    }
                }
            }
            if open {
                sink.EndFigure(D2D1_FIGURE_END_OPEN);
            }
            sink.Close().ok()?;
            Some(g)
        }
    }

    fn brush(&self, paint: &Paint, alpha: f64) -> Option<ID2D1Brush> {
        unsafe {
            let stops = |stops: &[(f64, Rgba)]| -> Option<ID2D1GradientStopCollection1> {
                let list: Vec<D2D1_GRADIENT_STOP> = stops.iter().map(|(p, c)| D2D1_GRADIENT_STOP { position: *p as f32, color: color(*c, 1.0) }).collect();
                self.ctx
                    .CreateGradientStopCollection(&list, D2D1_COLOR_SPACE_SRGB, D2D1_COLOR_SPACE_SRGB, D2D1_BUFFER_PRECISION_8BPC_UNORM, D2D1_EXTEND_MODE_CLAMP, D2D1_COLOR_INTERPOLATION_MODE_STRAIGHT)
                    .ok()
            };
            let props = D2D1_BRUSH_PROPERTIES { opacity: alpha as f32, transform: Matrix3x2::identity() };
            match paint {
                Paint::Solid(c) => self.ctx.CreateSolidColorBrush(&color(*c, 1.0), Some(&props)).ok().and_then(|b| b.cast().ok()),
                Paint::Linear { x0, y0, x1, y1, stops: s } => {
                    let coll = stops(s)?;
                    let lp = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES { startPoint: v2(*x0, *y0), endPoint: v2(*x1, *y1) };
                    self.ctx.CreateLinearGradientBrush(&lp, Some(&props), &coll).ok().and_then(|b| b.cast().ok())
                }
                Paint::Radial { cx, cy, r, stops: s } => {
                    let coll = stops(s)?;
                    let rp = D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES { center: v2(*cx, *cy), gradientOriginOffset: v2(0.0, 0.0), radiusX: r.max(0.0) as f32, radiusY: r.max(0.0) as f32 };
                    self.ctx.CreateRadialGradientBrush(&rp, Some(&props), &coll).ok().and_then(|b| b.cast().ok())
                }
            }
        }
    }

    fn stroke_style(&self, s: &Stroke) -> Option<ID2D1StrokeStyle> {
        let cap = match s.cap {
            LineCap::Butt => D2D1_CAP_STYLE_FLAT,
            LineCap::Round => D2D1_CAP_STYLE_ROUND,
        };
        let join = match s.join {
            LineJoin::Miter => D2D1_LINE_JOIN_MITER_OR_BEVEL,
            LineJoin::Round => D2D1_LINE_JOIN_ROUND,
        };
        // Canvas dashes are in user units; Direct2D's in stroke widths.
        let w = s.width.max(0.0001);
        let dashes: Vec<f32> = s.dash.iter().map(|d| (d / w) as f32).collect();
        let props = D2D1_STROKE_STYLE_PROPERTIES1 {
            startCap: cap,
            endCap: cap,
            dashCap: cap,
            lineJoin: join,
            miterLimit: 10.0,
            dashStyle: if dashes.is_empty() { D2D1_DASH_STYLE_SOLID } else { D2D1_DASH_STYLE_CUSTOM },
            dashOffset: 0.0,
            transformType: D2D1_STROKE_TRANSFORM_TYPE_NORMAL,
        };
        unsafe { self.factory.CreateStrokeStyle(&props, if dashes.is_empty() { None } else { Some(&dashes) }).ok().and_then(|x| x.cast().ok()) }
    }

    /// Draws `draw` (with a brush given) into a command list, then its canvas shadow under the current target.
    fn shadow(&self, sh: &Shadow, st: &State, draw: &dyn Fn(&ID2D1DeviceContext, &ID2D1Brush)) {
        unsafe {
            let Ok(list) = self.ctx.CreateCommandList() else { return };
            let Ok(prev) = self.ctx.GetTarget() else { return };
            self.ctx.SetTarget(&list);
            if let Some(black) = self.brush(&Paint::Solid(Rgba::BLACK), 1.0) {
                self.set(&st.m);
                draw(&self.ctx, &black);
            }
            let _ = list.Close();
            self.ctx.SetTarget(&prev);
            let Ok(effect) = self.ctx.CreateEffect(&CLSID_D2D1Shadow) else { return };
            effect.SetInput(0, &list, true);
            let sigma = (sh.blur / 2.0) as f32;
            let _ = effect.SetValue(D2D1_SHADOW_PROP_BLUR_STANDARD_DEVIATION.0 as u32, D2D1_PROPERTY_TYPE_FLOAT, &sigma.to_le_bytes());
            let c = color(sh.color, st.alpha);
            let mut col = Vec::with_capacity(16);
            for v in [c.r, c.g, c.b, c.a] {
                col.extend_from_slice(&v.to_le_bytes());
            }
            let _ = effect.SetValue(D2D1_SHADOW_PROP_COLOR.0 as u32, D2D1_PROPERTY_TYPE_VECTOR4, &col);
            let Ok(out) = effect.GetOutput() else { return };
            // The shadow's offset is in canvas pixels, the transform does not move it (the canvas rule).
            self.ctx.SetTransform(&self.outer);
            let offset = v2(sh.dx, sh.dy);
            self.ctx.DrawImage(&out, Some(&offset), None, D2D1_INTERPOLATION_MODE_LINEAR, D2D1_COMPOSITE_MODE_SOURCE_OVER);
        }
    }

    fn text_layout(&self, text: &str, font: &Font, ls: f64) -> Option<IDWriteTextLayout> {
        self.fonts.layout(text, font, ls)
    }

    /// The image drawn with the filters (CSS filter functions as Direct2D effects), or plainly.
    fn draw_bitmap(&self, bmp: &ID2D1Bitmap1, src: D2D_RECT_F, dst: D2D_RECT_F, st: &State, tint: Option<Rgba>) {
        unsafe {
            if st.filter.is_none() && tint.is_none() {
                self.ctx.DrawBitmap(bmp, Some(&dst), st.alpha as f32, D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC, Some(&src), None);
                return;
            }
            // Effects work on the bitmap's pixels: crop, scale into place with a transform, then filter.
            let Ok(crop) = self.ctx.CreateEffect(&CLSID_D2D1Crop) else { return };
            crop.SetInput(0, bmp, true);
            let mut rect = Vec::with_capacity(16);
            for v in [src.left, src.top, src.right, src.bottom] {
                rect.extend_from_slice(&v.to_le_bytes());
            }
            let _ = crop.SetValue(D2D1_CROP_PROP_RECT.0 as u32, D2D1_PROPERTY_TYPE_VECTOR4, &rect);
            let mut current: ID2D1Image = match crop.GetOutput() {
                Ok(o) => o,
                Err(_) => return,
            };
            let matrix = color_matrix(&st.filter, tint);
            if let Some(mx) = matrix {
                if let Ok(cm) = self.ctx.CreateEffect(&CLSID_D2D1ColorMatrix) {
                    cm.SetInput(0, &current, true);
                    let mut bytes = Vec::with_capacity(80);
                    for v in mx {
                        bytes.extend_from_slice(&v.to_le_bytes());
                    }
                    let _ = cm.SetValue(D2D1_COLORMATRIX_PROP_COLOR_MATRIX.0 as u32, D2D1_PROPERTY_TYPE_MATRIX_5X4, &bytes);
                    if let Ok(o) = cm.GetOutput() {
                        current = o;
                    }
                }
            }
            let (sw, sh) = ((src.right - src.left).max(0.001), (src.bottom - src.top).max(0.001));
            let (sx, sy) = ((dst.right - dst.left) / sw, (dst.bottom - dst.top) / sh);
            let place = Matrix3x2 { M11: sx, M12: 0.0, M21: 0.0, M22: sy, M31: dst.left - src.left * sx, M32: dst.top - src.top * sy };
            let mut world = Matrix3x2::identity();
            self.ctx.GetTransform(&mut world);
            self.ctx.SetTransform(&compose(place, world));
            if st.filter.blur > 0.0 {
                if let Ok(blur) = self.ctx.CreateEffect(&CLSID_D2D1GaussianBlur) {
                    blur.SetInput(0, &current, true);
                    // The blur is in canvas pixels, the image is drawn scaled: express it in source pixels.
                    let sigma = (st.filter.blur / sx.max(0.0001) as f64) as f32;
                    let _ = blur.SetValue(D2D1_GAUSSIANBLUR_PROP_STANDARD_DEVIATION.0 as u32, D2D1_PROPERTY_TYPE_FLOAT, &sigma.to_le_bytes());
                    if let Ok(o) = blur.GetOutput() {
                        current = o;
                    }
                }
            }
            // The global alpha through a layer (DrawImage has no opacity).
            let layer = st.alpha < 1.0;
            if layer {
                let params = D2D1_LAYER_PARAMETERS1 {
                    contentBounds: D2D_RECT_F { left: f32::NEG_INFINITY, top: f32::NEG_INFINITY, right: f32::INFINITY, bottom: f32::INFINITY },
                    geometricMask: std::mem::ManuallyDrop::new(None),
                    maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                    maskTransform: Matrix3x2::identity(),
                    opacity: st.alpha as f32,
                    opacityBrush: std::mem::ManuallyDrop::new(None),
                    layerOptions: D2D1_LAYER_OPTIONS1_NONE,
                };
                self.ctx.PushLayer(&params, None);
            }
            self.ctx.DrawImage(&current, None, Some(&src), D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC, D2D1_COMPOSITE_MODE_SOURCE_OVER);
            if layer {
                self.ctx.PopLayer();
            }
            self.ctx.SetTransform(&world);
        }
    }
}

/// `a` applied first, then `b`.
fn compose(a: Matrix3x2, b: Matrix3x2) -> Matrix3x2 {
    Matrix3x2 {
        M11: a.M11 * b.M11 + a.M12 * b.M21,
        M12: a.M11 * b.M12 + a.M12 * b.M22,
        M21: a.M21 * b.M11 + a.M22 * b.M21,
        M22: a.M21 * b.M12 + a.M22 * b.M22,
        M31: a.M31 * b.M11 + a.M32 * b.M21 + b.M31,
        M32: a.M31 * b.M12 + a.M32 * b.M22 + b.M32,
    }
}

/// The CSS filter functions (in the canvas's order: grayscale, sepia, brightness, contrast, saturate) and a
/// multiply tint, as one 5×4 colour matrix (Direct2D's row-vector convention), `None` when it is the identity.
pub fn color_matrix(f: &Filters, tint: Option<Rgba>) -> Option<[f32; 20]> {
    type M = [[f64; 4]; 5];
    let id: M = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 0.0]];
    // A CSS matrix is given as out = M·in (rows: r, g, b); Direct2D wants in·M (columns).
    let css = |rows: [[f64; 3]; 3]| -> M {
        let mut m = id;
        for (o, row) in rows.iter().enumerate() {
            for (i, v) in row.iter().enumerate() {
                m[i][o] = *v;
            }
        }
        m
    };
    let mul = |a: M, b: M| -> M {
        // Apply a then b: in·a·b.
        let mut out = [[0.0; 4]; 5];
        for r in 0..5 {
            for c in 0..4 {
                let mut s = if r == 4 { b[4][c] } else { 0.0 };
                for k in 0..4 {
                    s += a[r][k] * b[k][c];
                }
                out[r][c] = s;
            }
        }
        out
    };
    let mut m = id;
    let mut any = false;
    if f.grayscale > 0.0 {
        let a = 1.0 - f.grayscale.min(1.0);
        m = mul(m, css([[0.2126 + 0.7874 * a, 0.7152 - 0.7152 * a, 0.0722 - 0.0722 * a], [0.2126 - 0.2126 * a, 0.7152 + 0.2848 * a, 0.0722 - 0.0722 * a], [0.2126 - 0.2126 * a, 0.7152 - 0.7152 * a, 0.0722 + 0.9278 * a]]));
        any = true;
    }
    if f.sepia > 0.0 {
        let a = 1.0 - f.sepia.min(1.0);
        m = mul(m, css([[0.393 + 0.607 * a, 0.769 - 0.769 * a, 0.189 - 0.189 * a], [0.349 - 0.349 * a, 0.686 + 0.314 * a, 0.168 - 0.168 * a], [0.272 - 0.272 * a, 0.534 - 0.534 * a, 0.131 + 0.869 * a]]));
        any = true;
    }
    if f.brightness != 1.0 {
        let b = f.brightness;
        m = mul(m, css([[b, 0.0, 0.0], [0.0, b, 0.0], [0.0, 0.0, b]]));
        any = true;
    }
    if f.contrast != 1.0 {
        let c = f.contrast;
        let mut cm = css([[c, 0.0, 0.0], [0.0, c, 0.0], [0.0, 0.0, c]]);
        let off = 0.5 - 0.5 * c;
        cm[4] = [off, off, off, 0.0];
        m = mul(m, cm);
        any = true;
    }
    if f.saturate != 1.0 {
        let s = f.saturate;
        m = mul(m, css([[0.213 + 0.787 * s, 0.715 - 0.715 * s, 0.072 - 0.072 * s], [0.213 - 0.213 * s, 0.715 + 0.285 * s, 0.072 - 0.072 * s], [0.213 - 0.213 * s, 0.715 - 0.715 * s, 0.072 + 0.928 * s]]));
        any = true;
    }
    if let Some(t) = tint {
        m = mul(m, css([[t.r, 0.0, 0.0], [0.0, t.g, 0.0], [0.0, 0.0, t.b]]));
        any = true;
    }
    if !any {
        return None;
    }
    let mut out = [0.0f32; 20];
    for r in 0..5 {
        for c in 0..4 {
            out[r * 4 + c] = m[r][c] as f32;
        }
    }
    Some(out)
}

impl Surface for D2dSurface<'_> {
    fn fill_path(&mut self, path: &Path, paint: &Paint, st: &State) {
        // The tint of an image: a `multiply` fill laid over the bitmap just drawn — redrawn through a colour
        // matrix (exact for opaque pixels, the canvas result).
        if st.multiply {
            if let (Paint::Solid(c), Some((bmp, src, dst, ist))) = (paint, self.last_image.clone()) {
                self.set(&ist.m);
                self.draw_bitmap(&bmp, src, dst, &ist, Some(*c));
                self.restore();
            }
            return;
        }
        let Some(g) = self.geometry(path) else { return };
        if let Some(sh) = st.shadow {
            self.shadow(&sh, st, &|ctx, b| unsafe { ctx.FillGeometry(&g, b, None) });
        }
        let Some(b) = self.brush(paint, st.alpha) else { return };
        self.set(&st.m);
        unsafe { self.ctx.FillGeometry(&g, &b, None) };
        self.restore();
    }

    fn stroke_path(&mut self, path: &Path, paint: &Paint, stroke: &Stroke, st: &State) {
        let Some(g) = self.geometry(path) else { return };
        let style = self.stroke_style(stroke);
        let w = stroke.width as f32;
        if let Some(sh) = st.shadow {
            let style2 = style.clone();
            self.shadow(&sh, st, &|ctx, b| unsafe { ctx.DrawGeometry(&g, b, w, style2.as_ref()) });
        }
        let Some(b) = self.brush(paint, st.alpha) else { return };
        self.set(&st.m);
        unsafe { self.ctx.DrawGeometry(&g, &b, w, style.as_ref()) };
        self.restore();
    }

    fn fill_text(&mut self, text: &str, font: &Font, x: f64, y: f64, baseline: Baseline, paint: &Paint, ls: f64, st: &State) {
        let Some(layout) = self.text_layout(text, font, ls) else { return };
        let top_to_base = Fonts::baseline(&layout) as f64;
        let base_y = match baseline {
            Baseline::Alphabetic => y,
            Baseline::Middle => {
                let (a, d) = self.fonts.ascent_descent(font);
                y + (a - d) / 2.0
            }
        };
        let origin = v2(x, base_y - top_to_base);
        if let Some(sh) = st.shadow {
            let l2 = layout.clone();
            self.shadow(&sh, st, &|ctx, b| unsafe { ctx.DrawTextLayout(origin, &l2, b, D2D1_DRAW_TEXT_OPTIONS_NONE) });
        }
        let Some(b) = self.brush(paint, st.alpha) else { return };
        self.set(&st.m);
        unsafe { self.ctx.DrawTextLayout(origin, &layout, &b, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT) };
        self.restore();
    }

    fn stroke_text(&mut self, _text: &str, _font: &Font, _x: f64, _y: f64, _color: Rgba, _width: f64, _ls: f64, _st: &State) {
        // The text outline (« Contour ») needs the glyph outlines (a DirectWrite text renderer): not drawn
        // yet — vskubuno docs/PRESENTATIONS-DESKTOP.md §7.
    }

    fn draw_image(&mut self, src: &str, crop: Option<Rect>, dst: Rect, st: &State) -> bool {
        let Some(bmp) = self.bitmaps.bitmap(&self.ctx, self.store, src) else { return false };
        let size = unsafe { bmp.GetSize() };
        let (nw, nh) = (size.width as f64, size.height as f64);
        let c = crop.unwrap_or(Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 });
        let s = D2D_RECT_F { left: (c.x * nw) as f32, top: (c.y * nh) as f32, right: ((c.x + c.w) * nw) as f32, bottom: ((c.y + c.h) * nh) as f32 };
        let d = D2D_RECT_F { left: dst.x as f32, top: dst.y as f32, right: (dst.x + dst.w) as f32, bottom: (dst.y + dst.h) as f32 };
        if let Some(sh) = st.shadow {
            let b2 = bmp.clone();
            self.shadow(&sh, st, &|ctx, _| unsafe { ctx.DrawBitmap(&b2, Some(&d), 1.0, D2D1_INTERPOLATION_MODE_LINEAR, Some(&s), None) });
        }
        self.set(&st.m);
        self.draw_bitmap(&bmp, s, d, st, None);
        self.restore();
        self.last_image = Some((bmp, s, d, st.clone()));
        true
    }

    fn image_size(&mut self, src: &str) -> Option<(f64, f64)> {
        let bmp = self.bitmaps.bitmap(&self.ctx, self.store, src)?;
        let s = unsafe { bmp.GetSize() };
        Some((s.width as f64, s.height as f64))
    }

    fn push_clip(&mut self, path: &Path, m: &Matrix) {
        // An axis-aligned rectangle under no rotation is the cheap clip; anything else a layer mask.
        let rect = axis_rect(path).filter(|_| m.b == 0.0 && m.c == 0.0);
        unsafe {
            match rect {
                Some(r) => {
                    self.set(m);
                    self.ctx.PushAxisAlignedClip(&r, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                    self.restore();
                    self.clips.push(Pushed::Axis);
                }
                None => {
                    let Some(g) = self.geometry(path) else {
                        self.ctx.PushAxisAlignedClip(&D2D_RECT_F::default(), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                        self.clips.push(Pushed::Axis);
                        return;
                    };
                    let geometry: Option<ID2D1Geometry> = g.cast().ok();
                    let params = D2D1_LAYER_PARAMETERS1 {
                        contentBounds: D2D_RECT_F { left: f32::NEG_INFINITY, top: f32::NEG_INFINITY, right: f32::INFINITY, bottom: f32::INFINITY },
                        geometricMask: std::mem::ManuallyDrop::new(geometry),
                        maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                        maskTransform: m3(m),
                        opacity: 1.0,
                        opacityBrush: std::mem::ManuallyDrop::new(None),
                        layerOptions: D2D1_LAYER_OPTIONS1_NONE,
                    };
                    self.ctx.PushLayer(&params, None);
                    drop(std::mem::ManuallyDrop::into_inner(params.geometricMask));
                    self.clips.push(Pushed::Layer);
                }
            }
        }
    }

    fn pop_clip(&mut self) {
        unsafe {
            match self.clips.pop() {
                Some(Pushed::Axis) => self.ctx.PopAxisAlignedClip(),
                Some(Pushed::Layer) => self.ctx.PopLayer(),
                None => {}
            }
        }
    }
}

impl Drop for D2dSurface<'_> {
    fn drop(&mut self) {
        while !self.clips.is_empty() {
            self.pop_clip();
        }
        self.restore();
    }
}

/// The rectangle of a path made of one axis-aligned rectangle (`rect()`), if it is one.
fn axis_rect(path: &Path) -> Option<D2D_RECT_F> {
    let c = &path.cmds;
    if c.len() != 5 {
        return None;
    }
    let p = |i: usize| match c[i] {
        Cmd::MoveTo(x, y) | Cmd::LineTo(x, y) => Some((x, y)),
        _ => None,
    };
    let (a, b, cc, d) = (p(0)?, p(1)?, p(2)?, p(3)?);
    if !matches!(c[4], Cmd::Close) {
        return None;
    }
    let rect = a.1 == b.1 && b.0 == cc.0 && cc.1 == d.1 && d.0 == a.0;
    rect.then(|| D2D_RECT_F { left: a.0.min(cc.0) as f32, top: a.1.min(cc.1) as f32, right: a.0.max(cc.0) as f32, bottom: a.1.max(cc.1) as f32 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_compose_like_css() {
        assert_eq!(color_matrix(&Filters::NONE, None), None);
        let gray = color_matrix(&Filters { grayscale: 1.0, ..Filters::NONE }, None).expect("matrix");
        // Every output channel takes the same luminance weights of red.
        assert!((gray[0] - 0.2126).abs() < 1e-6 && (gray[1] - 0.2126).abs() < 1e-6 && (gray[2] - 0.2126).abs() < 1e-6);
        let tint = color_matrix(&Filters::NONE, Some(Rgba::rgb8(255, 0, 0))).expect("tint");
        assert_eq!((tint[0], tint[5], tint[10]), (1.0, 0.0, 0.0));
        let contrast = color_matrix(&Filters { contrast: 2.0, ..Filters::NONE }, None).expect("contrast");
        assert_eq!((contrast[0], contrast[16]), (2.0, -0.5));
    }

    #[test]
    fn a_rect_path_is_an_axis_clip() {
        let p = kubuno_office_slides_core::surface::PathBuilder::new().rect(1.0, 2.0, 3.0, 4.0).take();
        let r = axis_rect(&p).expect("rect");
        assert_eq!((r.left, r.top, r.right, r.bottom), (1.0, 2.0, 4.0, 6.0));
    }
}
