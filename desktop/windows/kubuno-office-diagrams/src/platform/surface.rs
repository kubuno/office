//! The Direct2D [`Surface`] the shared Canvas 2D context (`kubuno_office_diagrams_core::canvas`) draws on.
//!
//! The core resolves the web's drawing calls into device-space paths (lines and cubic Béziers), strokes
//! (width, dashes, caps, joins) and runs of text; this module turns them into Direct2D path geometries,
//! stroke styles and DirectWrite layouts on the window's device context. "Device space" here is the
//! window's DIPs: the canvas control hands the core a transform that already includes its own offset, and
//! Direct2D maps DIPs to pixels.
//!
//! Fonts follow the CSS family lists the web writes (`"Inter", Arial, sans-serif`): the first family
//! installed on the system is used, the generic ones map to the families Chrome uses on Windows. Text
//! is drawn with the baseline exactly at the font's ascent (`DWRITE_LINE_SPACING_METHOD_UNIFORM`), where
//! the canvas puts it.
//!
//! Shadows: the canvas blurs them; here the shadow is the shape's silhouette offset and drawn in the
//! shadow colour, unblurred (vskubuno docs/DIAGRAMS-DESKTOP.md §6).

use std::cell::RefCell;
use std::collections::HashMap;

use kubuno_office_diagrams_core::canvas::{Font, LineCap, LineJoin, Path, PathEl, Shadow, Stroke, Surface, TextMetrics, TextRun};
use kubuno_office_diagrams_core::color::Rgba;
use windows::core::{Interface, BOOL, HSTRING};
use windows::Win32::Graphics::Direct2D::Common::{D2D1_COLOR_F, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_BEGIN_HOLLOW, D2D1_FIGURE_END_CLOSED, D2D1_FIGURE_END_OPEN, D2D1_FILL_MODE_WINDING, D2D1_BEZIER_SEGMENT};
use windows::Win32::Graphics::Direct2D::{
    ID2D1DeviceContext, ID2D1Factory, ID2D1Factory1, ID2D1PathGeometry1, ID2D1SolidColorBrush, ID2D1StrokeStyle, D2D1_CAP_STYLE, D2D1_CAP_STYLE_FLAT, D2D1_CAP_STYLE_ROUND, D2D1_CAP_STYLE_SQUARE,
    D2D1_DASH_STYLE_CUSTOM, D2D1_DASH_STYLE_SOLID, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_LINE_JOIN, D2D1_LINE_JOIN_BEVEL, D2D1_LINE_JOIN_MITER_OR_BEVEL, D2D1_LINE_JOIN_ROUND, D2D1_STROKE_STYLE_PROPERTIES,
};
use windows::Win32::Graphics::DirectWrite::{
    IDWriteFactory, IDWriteFontCollection, IDWriteTextFormat, DWRITE_FONT_METRICS, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE, DWRITE_FONT_STYLE_ITALIC, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT,
    DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_LINE_SPACING_METHOD_UNIFORM, DWRITE_TEXT_METRICS, DWRITE_WORD_WRAPPING_NO_WRAP,
};
use windows_numerics::{Matrix3x2, Vector2};

/// DirectWrite's layout box when only the natural width is wanted (not `f32::MAX`: metrics overflow).
const UNBOUNDED: f32 = 65536.0;

/// A resolved font: the installed family and its metrics (ascent and descent per em).
#[derive(Clone)]
struct FontEntry {
    format: IDWriteTextFormat,
    /// Ascent and descent, in pixels at the font's size.
    ascent: f32,
    descent: f32,
}

/// What survives from one frame to the next: text formats, family resolutions, measured widths, stroke
/// styles. Owned by the control that paints.
#[derive(Default)]
pub struct SurfaceCache {
    fonts: RefCell<HashMap<(String, u32, bool, bool), FontEntry>>,
    families: RefCell<HashMap<String, Option<String>>>,
    widths: RefCell<HashMap<(String, u32, bool, bool, String), f32>>,
    strokes: RefCell<HashMap<String, ID2D1StrokeStyle>>,
    /// The DirectWrite factory the cache was filled with (a new renderer invalidates it).
    owner: RefCell<usize>,
}

impl SurfaceCache {
    pub fn check_owner(&self, dwrite: &IDWriteFactory) {
        let key = dwrite.as_raw() as usize;
        if *self.owner.borrow() != key {
            self.fonts.borrow_mut().clear();
            self.families.borrow_mut().clear();
            self.widths.borrow_mut().clear();
            self.strokes.borrow_mut().clear();
            *self.owner.borrow_mut() = key;
        }
    }
}

/// The surface over one frame of a Direct2D device context.
pub struct D2DSurface<'a> {
    ctx: &'a ID2D1DeviceContext,
    factory: &'a ID2D1Factory1,
    dwrite: &'a IDWriteFactory,
    brush: ID2D1SolidColorBrush,
    cache: &'a SurfaceCache,
    /// The device context's transform outside this surface (restored after each run of text).
    base: Matrix3x2,
}

fn d2d_color(c: Rgba) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: c.r, g: c.g, b: c.b, a: c.a }
}

fn pt(p: kubuno_office_diagrams_core::canvas::Point) -> Vector2 {
    Vector2 { X: p.x as f32, Y: p.y as f32 }
}

/// The installed family for a CSS generic family (Chrome's choices on Windows).
fn generic(name: &str) -> Option<&'static str> {
    Some(match name.to_ascii_lowercase().as_str() {
        "sans-serif" | "system-ui" => "Arial",
        "serif" => "Times New Roman",
        "monospace" => "Consolas",
        "cursive" => "Comic Sans MS",
        "fantasy" => "Impact",
        _ => return None,
    })
}

impl<'a> D2DSurface<'a> {
    /// A surface drawing on `ctx` (whose current transform is the identity or the caller's base).
    pub fn new(ctx: &'a ID2D1DeviceContext, factory: &'a ID2D1Factory1, dwrite: &'a IDWriteFactory, cache: &'a SurfaceCache) -> windows::core::Result<Self> {
        cache.check_owner(dwrite);
        let brush = unsafe { ctx.CreateSolidColorBrush(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }, None)? };
        let mut base = Matrix3x2::default();
        unsafe { ctx.GetTransform(&mut base) };
        Ok(Self { ctx, factory, dwrite, brush, cache, base })
    }

    fn color(&self, c: Rgba) -> &ID2D1SolidColorBrush {
        unsafe { self.brush.SetColor(&d2d_color(c)) };
        &self.brush
    }

    fn geometry(&self, path: &Path, filled: bool) -> Option<ID2D1PathGeometry1> {
        unsafe {
            let geom = self.factory.CreatePathGeometry().ok()?;
            let sink = geom.Open().ok()?;
            sink.SetFillMode(D2D1_FILL_MODE_WINDING);
            let begin = if filled { D2D1_FIGURE_BEGIN_FILLED } else { D2D1_FIGURE_BEGIN_HOLLOW };
            let mut open = false;
            for el in &path.els {
                match *el {
                    PathEl::MoveTo(p) => {
                        if open {
                            sink.EndFigure(D2D1_FIGURE_END_OPEN);
                        }
                        sink.BeginFigure(pt(p), begin);
                        open = true;
                    }
                    PathEl::LineTo(p) => {
                        if open {
                            sink.AddLine(pt(p));
                        }
                    }
                    PathEl::CubicTo(a, b, c) => {
                        if open {
                            sink.AddBezier(&D2D1_BEZIER_SEGMENT { point1: pt(a), point2: pt(b), point3: pt(c) });
                        }
                    }
                    PathEl::Close => {
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

    fn stroke_style(&self, s: &Stroke) -> Option<ID2D1StrokeStyle> {
        let cap = |c: LineCap| -> D2D1_CAP_STYLE {
            match c {
                LineCap::Butt => D2D1_CAP_STYLE_FLAT,
                LineCap::Round => D2D1_CAP_STYLE_ROUND,
                LineCap::Square => D2D1_CAP_STYLE_SQUARE,
            }
        };
        let join: D2D1_LINE_JOIN = match s.join {
            LineJoin::Miter => D2D1_LINE_JOIN_MITER_OR_BEVEL,
            LineJoin::Round => D2D1_LINE_JOIN_ROUND,
            LineJoin::Bevel => D2D1_LINE_JOIN_BEVEL,
        };
        // Direct2D's custom dashes are in multiples of the stroke width.
        let w = s.width.max(1e-6);
        let dashes: Vec<f32> = s.dash.iter().map(|d| (d / w) as f32).collect();
        let key = format!("{:?}{:?}{}{:?}{}", s.cap, s.join, s.miter_limit, dashes, s.dash_offset / w);
        if let Some(found) = self.cache.strokes.borrow().get(&key) {
            return Some(found.clone());
        }
        let props = D2D1_STROKE_STYLE_PROPERTIES {
            startCap: cap(s.cap),
            endCap: cap(s.cap),
            // The canvas caps each dash like the line ends.
            dashCap: cap(s.cap),
            lineJoin: join,
            miterLimit: s.miter_limit as f32,
            dashStyle: if dashes.is_empty() { D2D1_DASH_STYLE_SOLID } else { D2D1_DASH_STYLE_CUSTOM },
            dashOffset: (s.dash_offset / w) as f32,
        };
        let base: ID2D1Factory = self.factory.cast().ok()?;
        let style = unsafe { base.CreateStrokeStyle(&props, if dashes.is_empty() { None } else { Some(&dashes) }).ok()? };
        self.cache.strokes.borrow_mut().insert(key, style.clone());
        Some(style)
    }

    fn shadowed(&self, shadow: Option<&Shadow>, draw: impl Fn(&Self, Rgba)) {
        if let Some(sh) = shadow {
            unsafe {
                let m = Matrix3x2 { M31: self.base.M31 + sh.offset_x as f32, M32: self.base.M32 + sh.offset_y as f32, ..self.base };
                self.ctx.SetTransform(&m);
            }
            draw(self, sh.color);
            unsafe { self.ctx.SetTransform(&self.base) };
        }
    }

    /// The first installed family of a CSS list (generic families mapped), cached.
    fn family(&self, families: &[String]) -> String {
        let key = families.join(",");
        if let Some(found) = self.cache.families.borrow().get(&key) {
            return found.clone().unwrap_or_else(|| "Arial".into());
        }
        let collection = unsafe {
            let mut c: Option<IDWriteFontCollection> = None;
            self.dwrite.GetSystemFontCollection(&mut c, false).ok().and(c)
        };
        let installed = |name: &str| -> bool {
            let Some(col) = &collection else { return false };
            let mut index = 0u32;
            let mut exists = BOOL::default();
            unsafe { col.FindFamilyName(&HSTRING::from(name), &mut index, &mut exists).is_ok() && exists.as_bool() }
        };
        let found = families.iter().find_map(|f| match generic(f) {
            Some(g) => Some(g.to_string()),
            None => installed(f).then(|| f.clone()),
        });
        self.cache.families.borrow_mut().insert(key, found.clone());
        found.unwrap_or_else(|| "Arial".into())
    }

    fn font(&self, font: &Font) -> Option<FontEntry> {
        let family = self.family(&font.families);
        let size_key = (font.size * 100.0).round() as u32;
        let key = (family.clone(), size_key, font.bold, font.italic);
        if let Some(found) = self.cache.fonts.borrow().get(&key) {
            return Some(found.clone());
        }
        let weight: DWRITE_FONT_WEIGHT = if font.bold { DWRITE_FONT_WEIGHT_BOLD } else { DWRITE_FONT_WEIGHT_NORMAL };
        let style: DWRITE_FONT_STYLE = if font.italic { DWRITE_FONT_STYLE_ITALIC } else { DWRITE_FONT_STYLE_NORMAL };
        let size = font.size as f32;
        let (ascent, descent) = self.metrics(&family, weight, style, size).unwrap_or((size * 0.905, size * 0.212));
        let format = unsafe {
            let f = self.dwrite.CreateTextFormat(&HSTRING::from(family.as_str()), None, weight, style, DWRITE_FONT_STRETCH_NORMAL, size, &HSTRING::from("fr-FR")).ok()?;
            let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
            let _ = f.SetLineSpacing(DWRITE_LINE_SPACING_METHOD_UNIFORM, ascent + descent, ascent);
            f
        };
        let entry = FontEntry { format, ascent, descent };
        self.cache.fonts.borrow_mut().insert(key, entry.clone());
        Some(entry)
    }

    fn metrics(&self, family: &str, weight: DWRITE_FONT_WEIGHT, style: DWRITE_FONT_STYLE, size: f32) -> Option<(f32, f32)> {
        unsafe {
            let mut c: Option<IDWriteFontCollection> = None;
            self.dwrite.GetSystemFontCollection(&mut c, false).ok()?;
            let c = c?;
            let mut index = 0u32;
            let mut exists = BOOL::default();
            c.FindFamilyName(&HSTRING::from(family), &mut index, &mut exists).ok()?;
            if !exists.as_bool() {
                return None;
            }
            let fam = c.GetFontFamily(index).ok()?;
            let f = fam.GetFirstMatchingFont(weight, DWRITE_FONT_STRETCH_NORMAL, style).ok()?;
            let face = f.CreateFontFace().ok()?;
            let mut fm = DWRITE_FONT_METRICS::default();
            face.GetMetrics(&mut fm);
            if fm.designUnitsPerEm == 0 {
                return None;
            }
            let em = fm.designUnitsPerEm as f32;
            Some((fm.ascent as f32 / em * size, fm.descent as f32 / em * size))
        }
    }
}

impl Surface for D2DSurface<'_> {
    fn fill_path(&mut self, path: &Path, color: Rgba, shadow: Option<&Shadow>) {
        if color.a <= 0.0 && shadow.is_none() {
            return;
        }
        let Some(geom) = self.geometry(path, true) else { return };
        self.shadowed(shadow, |s, c| unsafe { s.ctx.FillGeometry(&geom, s.color(c), None) });
        if color.a > 0.0 {
            unsafe { self.ctx.FillGeometry(&geom, self.color(color), None) };
        }
    }

    fn stroke_path(&mut self, path: &Path, stroke: &Stroke, shadow: Option<&Shadow>) {
        if stroke.width <= 0.0 {
            return;
        }
        let Some(geom) = self.geometry(path, false) else { return };
        let style = self.stroke_style(stroke);
        let w = stroke.width as f32;
        self.shadowed(shadow, |s, c| unsafe { s.ctx.DrawGeometry(&geom, s.color(c), w, style.as_ref()) });
        if stroke.color.a > 0.0 {
            unsafe { self.ctx.DrawGeometry(&geom, self.color(stroke.color), w, style.as_ref()) };
        }
    }

    fn fill_text(&mut self, run: &TextRun<'_>, shadow: Option<&Shadow>) {
        let Some(entry) = self.font(run.font) else { return };
        let wide: Vec<u16> = run.text.encode_utf16().collect();
        let Ok(layout) = (unsafe { self.dwrite.CreateTextLayout(&wide, &entry.format, UNBOUNDED, UNBOUNDED) }) else { return };
        let t = run.transform;
        let m = Matrix3x2 { M11: t.a as f32, M12: t.b as f32, M21: t.c as f32, M22: t.d as f32, M31: t.e as f32, M32: t.f as f32 };
        let origin = Vector2 { X: run.origin.x as f32, Y: run.origin.y as f32 - entry.ascent };
        let draw = |s: &Self, c: Rgba, dx: f32, dy: f32| unsafe {
            let shifted = Matrix3x2 { M31: m.M31 + dx, M32: m.M32 + dy, ..m };
            s.ctx.SetTransform(&compose(&shifted, &s.base));
            s.ctx.DrawTextLayout(origin, &layout, s.color(c), D2D1_DRAW_TEXT_OPTIONS_NONE);
        };
        if let Some(sh) = shadow {
            draw(self, sh.color, sh.offset_x as f32, sh.offset_y as f32);
        }
        draw(self, run.color, 0.0, 0.0);
        unsafe { self.ctx.SetTransform(&self.base) };
    }

    fn measure_text(&mut self, text: &str, font: &Font) -> TextMetrics {
        let Some(entry) = self.font(font) else {
            return TextMetrics { width: text.chars().count() as f64 * font.size * 0.5, em_ascent: font.size * 0.8, em_descent: font.size * 0.2 };
        };
        let em = (entry.ascent + entry.descent).max(1e-6);
        let em_ascent = (entry.ascent / em) as f64 * font.size;
        let em_descent = (entry.descent / em) as f64 * font.size;
        let family = self.family(&font.families);
        let key = (family, (font.size * 100.0).round() as u32, font.bold, font.italic, text.to_string());
        if let Some(&w) = self.cache.widths.borrow().get(&key) {
            return TextMetrics { width: w as f64, em_ascent, em_descent };
        }
        let wide: Vec<u16> = text.encode_utf16().collect();
        let width = unsafe {
            match self.dwrite.CreateTextLayout(&wide, &entry.format, UNBOUNDED, UNBOUNDED) {
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
        TextMetrics { width: width as f64, em_ascent, em_descent }
    }
}

/// `a` applied first, then `b`.
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
