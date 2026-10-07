//! The slide renderer — a port of the web's `SlideRenderer` (`PresentationEditorPage.tsx:593-1319`), drawn
//! through [`Ctx`] on the platform's [`Surface`].
//!
//! One renderer for every use, like the web: the editor's slide, the thumbnails, exports (`Mode::Thumbnail`)
//! and the slideshow (`Mode::Present`, with the animation reveal of `renderPresent`). Sizes follow the web: the
//! slide is drawn in a `width`×`height` canvas, element geometry is a fraction of it, absolute sizes (fonts,
//! strokes, shadows, arrows) are slide pixels times `sf = width / 960`.
//!
//! Web bugs NOT ported (vskubuno docs/PRESENTATIONS-DESKTOP.md §1.2): a shape's gradient is drawn in the
//! shape's own box (the web paints it shifted by the shape's position: the gradient is built before a
//! `translate`), and placeholders are not shown in the slideshow.

use std::collections::HashSet;

use kubuno_office_shapes_core::{self as shapes, path::shade_colour, ViewOptions};
use serde_json::Value;

use crate::color::Rgba;
use crate::model::{as_f64, truthy, Background, Element, Gradient, Rect, SlideData, Theme, SLIDE_H, SLIDE_W};
use crate::richtext::{self, Align, Defaults, Line, Measure, Para, Style};
use crate::surface::{Baseline, Ctx, Filters, Font, LineCap, LineJoin, Matrix, Paint, PathBuilder, Shadow, Stroke, Surface};

/// What the slide is drawn for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// The editor: placeholders shown.
    #[default]
    Edit,
    /// Thumbnails, PDF and PNG: no placeholders.
    Thumbnail,
    /// The slideshow: `renderPresent` (animated elements hidden until revealed).
    Present,
}

/// How a slide is drawn.
#[derive(Debug, Clone, Default)]
pub struct Options<'a> {
    pub mode: Mode,
    /// Present mode: elements not revealed yet.
    pub hidden: Option<&'a HashSet<String>>,
    /// Present mode: the element being revealed and its progress 0..1.
    pub animating: Option<(&'a str, f64)>,
    /// Elements not drawn (the image being cropped: its overlay shows it).
    pub skip: Option<&'a str>,
}

// ── Easings (`PresentationEditorPage.tsx:184-194`) ──────────────────────────

pub fn ease_out_cubic(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(3)
}

pub fn ease_out_back(t: f64) -> f64 {
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

pub fn ease_out_bounce(t: f64) -> f64 {
    let (n1, d1) = (7.5625, 2.75);
    if t < 1.0 / d1 {
        n1 * t * t
    } else if t < 2.0 / d1 {
        let t = t - 1.5 / d1;
        n1 * t * t + 0.75
    } else if t < 2.5 / d1 {
        let t = t - 2.25 / d1;
        n1 * t * t + 0.9375
    } else {
        let t = t - 2.625 / d1;
        n1 * t * t + 0.984375
    }
}

/// `buildCanvasGradient`: a linear gradient through the box's centre along `angle`, or a radial one of radius
/// `max(w, h) / 2`; stops sorted by position.
pub fn gradient_paint(g: &Gradient, bx: f64, by: f64, w: f64, h: f64) -> Paint {
    if g.legacy {
        // Legacy two-stop: corner to corner (`createLinearGradient(x, y, x + w, y + h)`).
        return Paint::Linear { x0: bx, y0: by, x1: bx + w, y1: by + h, stops: g.stops.iter().map(|s| (s.1, Rgba::parse(&s.0).unwrap_or(Rgba::BLACK))).collect() };
    }
    let (cx, cy) = (bx + w / 2.0, by + h / 2.0);
    let mut stops: Vec<(f64, Rgba)> = g.stops.iter().map(|s| (s.1.clamp(0.0, 1.0), Rgba::from_hex_opacity(&s.0, s.2))).collect();
    stops.sort_by(|a, b| a.0.total_cmp(&b.0));
    if g.radial {
        Paint::Radial { cx, cy, r: w.max(h) / 2.0, stops }
    } else {
        let ang = g.angle.to_radians();
        let (dx, dy) = (ang.cos(), ang.sin());
        let half = (dx.abs() * w + dy.abs() * h) / 2.0;
        Paint::Linear { x0: cx - dx * half, y0: cy - dy * half, x1: cx + dx * half, y1: cy + dy * half, stops }
    }
}

/// `{type:'color'|'gradient', color?, grad?, gradient?}` (a shape's fill) as a paint, `None` for no fill.
pub fn fill_paint(fill: Option<&Value>, x: f64, y: f64, w: f64, h: f64) -> Option<Paint> {
    let f = fill?;
    match f.get("type").and_then(Value::as_str) {
        Some("color") => f.get("color").and_then(Value::as_str).filter(|c| !c.is_empty()).map(Paint::css),
        Some("gradient") => {
            if let Some(g) = f.get("grad").and_then(Gradient::parse) {
                Some(gradient_paint(&g, x, y, w, h))
            } else {
                f.get("gradient").and_then(Gradient::legacy).map(|g| gradient_paint(&g, x, y, w, h))
            }
        }
        _ => None,
    }
}

/// The renderer for one canvas size.
pub struct Renderer<'m> {
    pub width: f64,
    pub height: f64,
    /// `width / 960`.
    pub sf: f64,
    pub measure: &'m dyn Measure,
}

impl<'m> Renderer<'m> {
    pub fn new(width: f64, height: f64, measure: &'m dyn Measure) -> Self {
        Renderer { width, height, sf: width / SLIDE_W, measure }
    }

    /// Draws the slide: background, then the elements by `zIndex`.
    pub fn render(&self, surface: &mut dyn Surface, base: Matrix, data: &SlideData, theme: &Theme, opts: &Options) {
        let mut ctx = Ctx::new(surface, base);
        self.render_background(&mut ctx, data, theme);
        let mut sorted: Vec<&Element> = data.elements.iter().collect();
        sorted.sort_by(|a, b| a.z_index().total_cmp(&b.z_index()));
        let show_placeholders = opts.mode == Mode::Edit;
        for el in sorted {
            if el.hidden() || opts.skip == Some(el.id()) {
                continue;
            }
            if opts.mode == Mode::Present {
                let animating = opts.animating.filter(|(id, _)| *id == el.id());
                if opts.hidden.is_some_and(|h| h.contains(el.id())) && animating.is_none() {
                    continue;
                }
                if let Some((_, t)) = animating {
                    ctx.save();
                    self.apply_entry(&mut ctx, el, t.clamp(0.0, 1.0));
                    self.render_element(&mut ctx, el, theme, false);
                    ctx.restore();
                    continue;
                }
            }
            self.render_element(&mut ctx, el, theme, show_placeholders);
        }
    }

    /// The entry animation's transform at progress `t` (`renderPresent`).
    fn apply_entry(&self, ctx: &mut Ctx, el: &Element, t: f64) {
        let (width, height) = (self.width, self.height);
        let kind = el.get("anim").and_then(|a| a.get("type")).and_then(Value::as_str).filter(|s| !s.is_empty()).unwrap_or("fade");
        let geo = el.bbox();
        let (cx, cy) = ((geo.x + geo.w / 2.0) * width, (geo.y + geo.h / 2.0) * height);
        let (gx, gy, gw, gh) = (geo.x * width, geo.y * height, geo.w * width, geo.h * height);
        let rotate_about = |ctx: &mut Ctx, ang: f64| {
            ctx.translate(cx, cy);
            ctx.rotate(ang);
            ctx.translate(-cx, -cy);
        };
        let scale_about = |ctx: &mut Ctx, sx: f64, sy: f64| {
            ctx.translate(cx, cy);
            ctx.scale(sx, sy);
            ctx.translate(-cx, -cy);
        };
        let no_fade = ["flyL", "flyR", "flyT", "flyB", "flyTL", "flyBR", "wiper", "wipeu", "bounce", "dropin"];
        ctx.set_alpha(if no_fade.contains(&kind) { 1.0 } else { t });
        let eo = ease_out_cubic(t);
        let pi = std::f64::consts::PI;
        match kind {
            "flyL" => ctx.translate(-(1.0 - eo) * width * 0.6, 0.0),
            "flyR" => ctx.translate((1.0 - eo) * width * 0.6, 0.0),
            "flyT" => ctx.translate(0.0, -(1.0 - eo) * height * 0.6),
            "flyB" => ctx.translate(0.0, (1.0 - eo) * height * 0.6),
            "flyTL" => ctx.translate(-(1.0 - eo) * width * 0.5, -(1.0 - eo) * height * 0.5),
            "flyBR" => ctx.translate((1.0 - eo) * width * 0.5, (1.0 - eo) * height * 0.5),
            "rise" => ctx.translate(0.0, (1.0 - eo) * height * 0.08),
            "zoom" => scale_about(ctx, 0.6 + 0.4 * t, 0.6 + 0.4 * t),
            "expand" => {
                let s = eo.max(0.001);
                scale_about(ctx, s, s)
            }
            "spin" => {
                let s = eo.max(0.001);
                scale_about(ctx, s, s);
                rotate_about(ctx, (1.0 - eo) * pi * 2.0);
            }
            "flip" => {
                let s = ((1.0 - t) * pi / 2.0).cos();
                scale_about(ctx, s.max(0.02), 1.0)
            }
            "swivel" => {
                let s = ((1.0 - t) * pi * 1.5).cos();
                scale_about(ctx, s.abs().max(0.05), 1.0)
            }
            "growturn" => {
                let s = eo.max(0.001);
                scale_about(ctx, s, s);
                rotate_about(ctx, (1.0 - eo) * pi);
            }
            "pulse" => {
                let s = 1.0 + (ease_out_back(t) - 1.0);
                scale_about(ctx, s, s)
            }
            "bounce" => {
                let s = ease_out_bounce(t);
                scale_about(ctx, 0.5 + 0.5 * s, 0.5 + 0.5 * s)
            }
            "dropin" => ctx.translate(0.0, -(1.0 - ease_out_bounce(t)) * height * 0.5),
            "wiper" => {
                let p = PathBuilder::new().rect(gx, gy, gw * eo, gh).take();
                ctx.clip(&p)
            }
            "wipeu" => {
                let p = PathBuilder::new().rect(gx, gy + gh * (1.0 - eo), gw, gh * eo).take();
                ctx.clip(&p)
            }
            _ => {}
        }
    }

    fn render_background(&self, ctx: &mut Ctx, data: &SlideData, theme: &Theme) {
        let (w, h) = (self.width, self.height);
        match data.background() {
            Background::Color(c) => {
                let c = c.unwrap_or_else(|| theme.bg_color.clone());
                ctx.fill_rect(0.0, 0.0, w, h, &Paint::css(&c));
            }
            Background::Gradient(g) => {
                let p = gradient_paint(&g, 0.0, 0.0, w, h);
                ctx.fill_rect(0.0, 0.0, w, h, &p);
            }
            Background::Image(src) => {
                if !ctx.draw_image(&src, None, Rect { x: 0.0, y: 0.0, w, h }) {
                    ctx.fill_rect(0.0, 0.0, w, h, &Paint::css("#f1f3f4"));
                }
            }
            Background::Empty => {}
        }
    }

    /// One element (`renderElement`).
    pub fn render_element(&self, ctx: &mut Ctx, el: &Element, theme: &Theme, show_placeholders: bool) {
        if el.hidden() {
            return;
        }
        let (width, height, sf) = (self.width, self.height, self.sf);
        let (x, y, w, h) = (el.x() * width, el.y() * height, el.w() * width, el.h() * height);
        ctx.save();
        let rot = el.rotation();
        if rot != 0.0 && rot.is_finite() {
            ctx.translate(x + w / 2.0, y + h / 2.0);
            ctx.rotate(rot.to_radians());
            ctx.translate(-(x + w / 2.0), -(y + h / 2.0));
        }
        let (fx, fy) = (el.truthy("flipX"), el.truthy("flipY"));
        if fx || fy {
            ctx.translate(x + w / 2.0, y + h / 2.0);
            ctx.scale(if fx { -1.0 } else { 1.0 }, if fy { -1.0 } else { 1.0 });
            ctx.translate(-(x + w / 2.0), -(y + h / 2.0));
        }
        if let Some(o) = el.f("opacity").filter(|o| *o < 1.0) {
            let a = ctx.alpha() * o.max(0.0);
            ctx.set_alpha(a);
        }
        if let Some(sh) = el.get("shadow").filter(|s| truthy(Some(s))) {
            ctx.set_shadow(Some(shadow_of(sh, "rgba(0,0,0,0.35)", 8.0, 3.0, 3.0, sf)));
        }
        match el.kind() {
            "text" => self.render_text(ctx, el, x, y, w, h, theme, show_placeholders),
            "shape" => self.render_shape(ctx, el, x, y, w, h),
            "image" => self.render_image(ctx, el, x, y, w, h),
            "line" => self.render_line(ctx, el),
            "chart" => crate::chart::render_chart(ctx, el, x, y, w, h, sf, self.measure),
            "table" => crate::table::render_table(ctx, el, x, y, w, h, sf, self.measure),
            _ => {}
        }
        ctx.restore();
    }

    // ── Text ────────────────────────────────────────────────────────────────

    #[allow(clippy::too_many_arguments)]
    fn render_text(&self, ctx: &mut Ctx, el: &Element, x: f64, y: f64, w: f64, h: f64, theme: &Theme, show_placeholders: bool) {
        if let Some(bg) = el.s("background").filter(|b| !b.is_empty()) {
            let r = el.f_or("borderRadius", 0.0);
            let p = if r != 0.0 { PathBuilder::new().round_rect_q(x, y, w, h, r).take() } else { PathBuilder::new().rect(x, y, w, h).take() };
            ctx.fill(&p, &Paint::css(bg));
        }
        let Some(tb) = layout_text_box(el, x, y, w, h, theme, show_placeholders, self.sf, self.measure) else { return };
        let word_art = el.get("wordArt").filter(|v| v.is_object()).map(|wa| Paint::Linear {
            x0: 0.0,
            y0: y,
            x1: 0.0,
            y1: y + h,
            stops: vec![
                (0.0, Rgba::parse(wa.get("from").and_then(Value::as_str).unwrap_or("#000")).unwrap_or(Rgba::BLACK)),
                (1.0, Rgba::parse(wa.get("to").and_then(Value::as_str).unwrap_or("#000")).unwrap_or(Rgba::BLACK)),
            ],
        });
        let text_shadow = el.get("textShadow").filter(|s| truthy(Some(s))).map(|s| shadow_of(s, "rgba(0,0,0,0.45)", 4.0, 2.0, 2.0, self.sf));
        let outline = el.get("textOutline").and_then(|o| {
            let width = as_f64(o.get("width")).unwrap_or(0.0);
            (width > 0.0).then(|| (Rgba::parse(o.get("color").and_then(Value::as_str).unwrap_or("#000")).unwrap_or(Rgba::BLACK), width * self.sf))
        });
        for pl in &tb.lines {
            if let Some(mk) = &pl.line.marker {
                ctx.fill_text(&mk.text, &font_of(&mk.style), pl.col_left + mk.x, pl.baseline, Baseline::Alphabetic, &Paint::css(&mk.style.color), tb.letter_spacing);
            }
            for ps in &pl.segs {
                let s = &ps.seg.style;
                let font = font_of(s);
                let sb = pl.baseline + s.rise * s.size;
                if let Some(hl) = s.hl.as_deref().filter(|h| !h.is_empty()) {
                    ctx.fill_rect(ps.x, sb - s.size * 0.82, ps.seg.width, s.size * 1.04, &Paint::css(hl));
                }
                if text_shadow.is_some() {
                    ctx.set_shadow(text_shadow);
                }
                let paint = word_art.clone().unwrap_or_else(|| Paint::css(&s.color));
                ctx.fill_text(&ps.seg.text, &font, ps.x, sb, Baseline::Alphabetic, &paint, tb.letter_spacing);
                ctx.set_shadow(None);
                if let Some((oc, ow)) = outline {
                    if !ps.seg.text.trim().is_empty() {
                        ctx.stroke_text(&ps.seg.text, &font, ps.x, sb, oc, ow, tb.letter_spacing);
                    }
                }
                if (s.underline || s.strike) && !ps.seg.text.trim().is_empty() {
                    let stroke = Stroke { width: (s.size / 16.0).max(1.0), ..Stroke::default() };
                    let ink = Paint::css(&s.color);
                    if s.underline {
                        let p = PathBuilder::new().move_to(ps.x, sb + s.size * 0.12).line_to(ps.x + ps.seg.width, sb + s.size * 0.12).take();
                        ctx.stroke(&p, &ink, &stroke);
                    }
                    if s.strike {
                        let p = PathBuilder::new().move_to(ps.x, sb - s.size * 0.30).line_to(ps.x + ps.seg.width, sb - s.size * 0.30).take();
                        ctx.stroke(&p, &ink, &stroke);
                    }
                }
            }
        }
    }

    // ── Shapes ──────────────────────────────────────────────────────────────

    fn render_shape(&self, ctx: &mut Ctx, el: &Element, x: f64, y: f64, w: f64, h: f64) {
        let sf = self.sf;
        let fill = fill_paint(el.get("fill"), x, y, w, h);
        let stroke_v = el.get("stroke");
        let sw = stroke_v.and_then(|s| as_f64(s.get("width"))).unwrap_or(0.0);
        let stroke_paint = Paint::css(stroke_v.and_then(|s| s.get("color")).and_then(Value::as_str).unwrap_or("#000"));
        let stroke = Stroke { width: sw * sf, dash: dash_of(stroke_v.and_then(|s| s.get("style")).and_then(Value::as_str)), ..Stroke::default() };
        let kind = el.s("shape").unwrap_or("rect");
        let solid = el.get("fill").filter(|f| f.get("type").and_then(Value::as_str) == Some("color")).and_then(|f| f.get("color")).and_then(Value::as_str);
        let adj = shape_adj(el);
        let view = if kind != "rect" { shapes::shape_view(kind, w, h, &ViewOptions { adj: adj.as_deref(), stroke: sw > 0.0, stroke_width: sw * sf }) } else { None };
        match view {
            Some(subs) => {
                for sp in &subs {
                    let path = sp.path.translated(x, y);
                    if sp.fill {
                        if let Some(f) = &fill {
                            match solid {
                                Some(c) if sp.shade != 1.0 => ctx.fill(&path, &Paint::css(&shade_colour(c, sp.shade))),
                                _ => ctx.fill(&path, f),
                            }
                        }
                    }
                    if sw > 0.0 && sp.stroke {
                        ctx.stroke(&path, &stroke_paint, &stroke);
                    }
                }
            }
            None => {
                // `rect`, and the fallback for a kind no shared geometry draws.
                let p = PathBuilder::new().rect(x, y, w, h).take();
                if let Some(f) = &fill {
                    ctx.fill(&p, f);
                }
                if sw > 0.0 {
                    ctx.stroke(&p, &stroke_paint, &stroke);
                }
            }
        }
        if el.get("content").is_some_and(|c| truthy(Some(c))) {
            self.render_shape_text(ctx, el, x, y, w, h);
        }
    }

    /// The caption of a shape (`renderShapeText`): plain text, centred, wrapped on spaces. QUIRK (web): the
    /// caption's marks are not drawn.
    fn render_shape_text(&self, ctx: &mut Ctx, el: &Element, x: f64, y: f64, w: f64, h: f64) {
        let paras = richtext::doc_to_paras(el.get("content"));
        let plain = richtext::paras_to_plain(&paras);
        if plain.is_empty() {
            return;
        }
        let size = el.f_or("fontSize", 18.0) * self.sf;
        let color = el.s("color").unwrap_or("#ffffff");
        let fam = el.s("fontFamily").unwrap_or("Arial, sans-serif");
        let style = plain_style(fam, size, color);
        let font = font_of(&style);
        let max_w = w - 12.0 * self.sf;
        let lines: Vec<String> = plain.split('\n').flat_map(|l| wrap_line(l, max_w, &style, self.measure)).collect();
        let line_h = size * 1.3;
        let mut ty = y + h / 2.0 - (lines.len() as f64 * line_h) / 2.0 + size;
        for ln in &lines {
            let lw = self.measure.width(ln, &style, 0.0);
            ctx.fill_text(ln, &font, x + w / 2.0 - lw / 2.0, ty, Baseline::Alphabetic, &Paint::css(color), 0.0);
            ty += line_h;
        }
    }

    // ── Images ──────────────────────────────────────────────────────────────

    fn render_image(&self, ctx: &mut Ctx, el: &Element, x: f64, y: f64, w: f64, h: f64) {
        let src = el.s("storagePath").unwrap_or("");
        let radius = el.f("cornerRadius").map(|r| (r * self.sf).min(w.min(h) / 2.0)).unwrap_or(0.0);
        ctx.save();
        if radius > 0.0 {
            let p = PathBuilder::new().round_rect_q(x, y, w, h, radius).take();
            ctx.clip(&p);
        }
        if let Some(f) = el.get("filters").filter(|f| f.is_object()) {
            let g = |k: &str| as_f64(f.get(k));
            ctx.set_filter(Filters {
                grayscale: g("grayscale").unwrap_or(0.0),
                sepia: g("sepia").unwrap_or(0.0),
                brightness: g("brightness").unwrap_or(1.0),
                contrast: g("contrast").unwrap_or(1.0),
                saturate: g("saturate").unwrap_or(1.0),
                blur: g("blur").unwrap_or(0.0) * self.sf,
            });
        }
        let crop = el.get("crop").filter(|c| c.is_object()).map(|c| Rect { x: as_f64(c.get("x")).unwrap_or(0.0), y: as_f64(c.get("y")).unwrap_or(0.0), w: as_f64(c.get("w")).unwrap_or(1.0), h: as_f64(c.get("h")).unwrap_or(1.0) });
        if !ctx.draw_image(src, crop, Rect { x, y, w, h }) {
            ctx.set_filter(Filters::NONE);
            ctx.fill_rect(x, y, w, h, &Paint::css("#e8eaed"));
            let style = plain_style("Arial", 14.0, "#9aa0a6");
            ctx.fill_text("Image", &font_of(&style), x + 8.0, y + 24.0, Baseline::Alphabetic, &Paint::css("#9aa0a6"), 0.0);
        }
        ctx.set_filter(Filters::NONE);
        if let Some(t) = el.s("tint").filter(|t| !t.is_empty()) {
            ctx.set_multiply(true);
            ctx.fill_rect(x, y, w, h, &Paint::css(t));
            ctx.set_multiply(false);
        }
        ctx.restore();
        if let Some(b) = el.get("border") {
            let bw = as_f64(b.get("width")).unwrap_or(0.0);
            if bw > 0.0 {
                let paint = Paint::css(b.get("color").and_then(Value::as_str).unwrap_or("#000"));
                let stroke = Stroke { width: bw * self.sf, ..Stroke::default() };
                let p = if radius > 0.0 { PathBuilder::new().round_rect_q(x, y, w, h, radius).take() } else { PathBuilder::new().rect(x, y, w, h).take() };
                ctx.stroke(&p, &paint, &stroke);
            }
        }
    }

    // ── Lines ───────────────────────────────────────────────────────────────

    fn render_line(&self, ctx: &mut Ctx, el: &Element) {
        let (width, height, sf) = (self.width, self.height, self.sf);
        let kind = el.s("lineType").unwrap_or("straight");
        let sv = el.get("stroke");
        let color = sv.and_then(|s| s.get("color")).and_then(Value::as_str).unwrap_or("#000");
        let paint = Paint::css(color);
        let stroke = Stroke { width: sv.and_then(|s| as_f64(s.get("width"))).unwrap_or(2.0) * sf, dash: dash_of(sv.and_then(|s| s.get("style")).and_then(Value::as_str)), cap: LineCap::Round, join: LineJoin::Round };
        let p1 = (el.x() * width, el.y() * height);
        let p2 = (el.f_or("x2", 0.0) * width, el.f_or("y2", 0.0) * height);
        let mut b = PathBuilder::new();
        let (tip_from, tip);
        if kind == "curved" || kind == "arc" {
            let (mx, my) = ((p1.0 + p2.0) / 2.0, (p1.1 + p2.1) / 2.0);
            let (dx, dy) = (p2.0 - p1.0, p2.1 - p1.1);
            let len = dx.hypot(dy);
            let len = if len == 0.0 { 1.0 } else { len };
            let (nx, ny) = (-dy / len, dx / len);
            let bow = if kind == "arc" { 0.5 } else { 0.28 } * len;
            let (cx, cy) = (mx + nx * bow, my + ny * bow);
            b.move_to(p1.0, p1.1).quad_to(cx, cy, p2.0, p2.1);
            tip_from = (cx, cy);
            tip = p2;
        } else {
            let pts = line_path_points(el, width, height);
            b.move_to(pts[0].0, pts[0].1);
            for p in &pts[1..] {
                b.line_to(p.0, p.1);
            }
            tip = pts[pts.len() - 1];
            tip_from = if pts.len() >= 2 { pts[pts.len() - 2] } else { p1 };
        }
        ctx.stroke(&b.take(), &paint, &stroke);
        let a_size = el.f_or("arrowSize", 12.0);
        if kind == "arrow" || el.get("arrowEnd").is_some_and(|v| truthy(Some(v))) {
            self.arrow_head(ctx, tip_from, tip, a_size, &paint);
        }
        if el.get("arrowStart").is_some_and(|v| truthy(Some(v))) {
            let next = if kind == "curved" || kind == "arc" { p2 } else { line_path_points(el, width, height).get(1).copied().unwrap_or(p2) };
            self.arrow_head(ctx, next, p1, a_size, &paint);
        }
    }

    fn arrow_head(&self, ctx: &mut Ctx, from: (f64, f64), to: (f64, f64), size_slide: f64, paint: &Paint) {
        let angle = (to.1 - from.1).atan2(to.0 - from.0);
        let size = size_slide * self.sf;
        let pi6 = std::f64::consts::PI / 6.0;
        let p = PathBuilder::new()
            .move_to(to.0, to.1)
            .line_to(to.0 - size * (angle - pi6).cos(), to.1 - size * (angle - pi6).sin())
            .line_to(to.0 - size * (angle + pi6).cos(), to.1 - size * (angle + pi6).sin())
            .close()
            .take();
        ctx.fill(&p, paint);
    }
}

/// A shadow (`shadow` / `textShadow`): `true` = the defaults, or `{color, blur, dx, dy}` in slide px.
fn shadow_of(v: &Value, color: &str, blur: f64, dx: f64, dy: f64, sf: f64) -> Shadow {
    let o = v.as_object();
    let g = |k: &str| o.and_then(|o| as_f64(o.get(k)));
    let c = o.and_then(|o| o.get("color")).and_then(Value::as_str).unwrap_or(color);
    Shadow { color: Rgba::parse(c).unwrap_or(Rgba::TRANSPARENT), blur: g("blur").unwrap_or(blur) * sf, dx: g("dx").unwrap_or(dx) * sf, dy: g("dy").unwrap_or(dy) * sf }
}

/// `dashed` → [8, 4], `dotted` → [2, 4] (canvas px, not scaled — the web's values).
pub fn dash_of(style: Option<&str>) -> Vec<f64> {
    match style {
        Some("dashed") => vec![8.0, 4.0],
        Some("dotted") => vec![2.0, 4.0],
        _ => Vec::new(),
    }
}

/// `shapeAdj`: the shape's `adj`, or a legacy `roundRect` pixel `cornerRadius` as the engine's fraction.
pub fn shape_adj(el: &Element) -> Option<Vec<f64>> {
    if let Some(Value::Array(a)) = el.get("adj") {
        return Some(a.iter().map(|v| v.as_f64().unwrap_or(f64::NAN)).collect());
    }
    if el.s("shape") == Some("roundRect") {
        if let Some(r) = el.f("cornerRadius") {
            let (w, h) = (el.w() * SLIDE_W, el.h() * SLIDE_H);
            if w > 0.0 && h > 0.0 {
                return Some(vec![(r / w.min(h)).clamp(0.0, 0.5)]);
            }
        }
    }
    None
}

/// `linePathPoints`: the line's drawn points in canvas px.
pub fn line_path_points(el: &Element, width: f64, height: f64) -> Vec<(f64, f64)> {
    let kind = el.s("lineType").unwrap_or("straight");
    let p = |nx: f64, ny: f64| (nx * width, ny * height);
    if kind == "polyline" || kind == "freehand" {
        if let Some(Value::Array(pts)) = el.get("points") {
            if pts.len() >= 2 {
                return pts.iter().map(|q| p(as_f64(q.get("x")).unwrap_or(0.0), as_f64(q.get("y")).unwrap_or(0.0))).collect();
            }
        }
    }
    let p1 = p(el.x(), el.y());
    let p2 = p(el.f_or("x2", 0.0), el.f_or("y2", 0.0));
    if kind == "elbow" {
        let mid_x = (p1.0 + p2.0) / 2.0;
        return vec![p1, (mid_x, p1.1), (mid_x, p2.1), p2];
    }
    vec![p1, p2]
}

pub fn font_of(s: &Style) -> Font {
    Font { family: s.family.clone(), size: s.size, bold: s.bold, italic: s.italic }
}

/// A plain style (`${size}px ${family}`, normal weight).
pub fn plain_style(family: &str, size: f64, color: &str) -> Style {
    Style { bold: false, italic: false, underline: false, strike: false, color: color.into(), size, family: family.into(), hl: None, rise: 0.0 }
}

/// `wrapLine`: words joined by single spaces, broken before the word that overflows.
pub fn wrap_line(line: &str, max_w: f64, style: &Style, m: &dyn Measure) -> Vec<String> {
    if line.is_empty() {
        return vec![String::new()];
    }
    let mut out = Vec::new();
    let mut cur = String::new();
    for word in line.split(' ') {
        let test = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
        if m.width(&test, style, 0.0) > max_w && !cur.is_empty() {
            out.push(std::mem::replace(&mut cur, word.to_string()));
        } else {
            cur = test;
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

// ── Text boxes: the placement both the renderer and the editor read ─────────

/// A segment placed on the canvas.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedSeg {
    pub seg: richtext::Seg,
    pub x: f64,
}

/// A line placed on the canvas.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedLine {
    pub line: Line,
    pub top: f64,
    pub baseline: f64,
    pub col_left: f64,
    /// Where an empty line's caret goes (the pen's start).
    pub pen_x: f64,
    pub segs: Vec<PlacedSeg>,
    /// Extra advance after each whitespace-only segment of a justified line.
    pub extra_per_gap: f64,
}

/// A laid-out text box (`renderText`'s geometry).
#[derive(Debug, Clone, PartialEq)]
pub struct TextBox {
    pub lines: Vec<PlacedLine>,
    pub placeholder: bool,
    /// The shrink-to-fit factor (1 when not shrinking).
    pub factor: f64,
    /// The letter spacing in canvas px.
    pub letter_spacing: f64,
    pub defaults: Defaults,
    /// The paragraphs laid out (display transform applied).
    pub paras: Vec<Para>,
}

/// The defaults a text element's runs fall back to.
pub fn text_defaults(el: &Element, theme: &Theme) -> Defaults {
    Defaults {
        bold: el.truthy("bold"),
        italic: el.truthy("italic"),
        underline: el.truthy("underline"),
        color: el.s("color").map(str::to_string).unwrap_or_else(|| theme.text_color.clone()),
        size: el.f_or("fontSize", 24.0),
        family: el.s("fontFamily").map(str::to_string).unwrap_or_else(|| theme.font_family.clone()),
        align: Align::parse(el.s("align")).unwrap_or(Align::Left),
    }
}

/// The geometry of a text element's text (`renderText`, `PresentationEditorPage.tsx:824-955`) in canvas px:
/// `None` when nothing is drawn (no text and no placeholder). `editing`: the box is being edited (no
/// placeholder, an empty paragraph still gets its line).
#[allow(clippy::too_many_arguments)]
pub fn layout_text_box(el: &Element, x: f64, y: f64, w: f64, h: f64, theme: &Theme, show_placeholders: bool, sf: f64, m: &dyn Measure) -> Option<TextBox> {
    layout_text_box_with(el, richtext::doc_to_paras(el.get("content")), x, y, w, h, theme, show_placeholders, sf, m, false)
}

/// [`layout_text_box`] over given paragraphs (the editor's live copy).
#[allow(clippy::too_many_arguments)]
pub fn layout_text_box_with(el: &Element, paras: Vec<Para>, x: f64, y: f64, w: f64, h: f64, theme: &Theme, show_placeholders: bool, sf: f64, m: &dyn Measure, editing: bool) -> Option<TextBox> {
    let plain = richtext::paras_to_plain(&paras);
    let pad = el.f_or("padding", 8.0) * (w / 100.0);
    let max_w = (w - 2.0 * pad).max(1.0);
    let defaults = text_defaults(el, theme);
    let ls = el.f("letterSpacing").filter(|v| *v != 0.0).map(|v| v * sf).unwrap_or(0.0);
    let tf = el.s("textTransform");
    let map = |ps: Vec<Para>| -> Vec<Para> {
        if tf.is_none() {
            return ps;
        }
        ps.into_iter().map(|p| Para { runs: p.runs.into_iter().map(|r| { let t = richtext::transform_text(&r.text, tf); r.with_text(t) }).collect(), attrs: p.attrs }).collect()
    };
    let placeholder_text = el.s("placeholder").filter(|p| !p.is_empty());
    let is_placeholder = plain.is_empty() && show_placeholders && !editing && placeholder_text.is_some();
    if plain.is_empty() && !is_placeholder && !editing {
        return None;
    }
    let layout_paras = map(if is_placeholder { vec![Para { runs: vec![richtext::Run::plain(placeholder_text.unwrap_or(""))], attrs: Default::default() }] } else { paras });
    let layout_defaults = if is_placeholder { Defaults { color: el.s("color").unwrap_or("#9aa0a6").to_string(), ..defaults.clone() } } else { defaults.clone() };
    let col_count = if el.f("columns") == Some(2.0) { 2usize } else { 1 };
    let col_gap = 16.0 * sf;
    let col_w = (max_w - (col_count as f64 - 1.0) * col_gap) / col_count as f64;
    let h_avail = h - 2.0 * pad;
    let mut factor = 1.0;
    let mut lines = richtext::layout_rich(&layout_paras, &layout_defaults, col_w, m, ls, sf * factor);
    if el.s("autofit") == Some("shrink") && !plain.is_empty() {
        let mut guard = 0;
        while guard < 40 && factor > 0.15 {
            guard += 1;
            if lines.iter().map(|l| l.height).sum::<f64>() <= h_avail * col_count as f64 {
                break;
            }
            factor *= 0.9;
            lines = richtext::layout_rich(&layout_paras, &layout_defaults, col_w, m, ls, sf * factor);
        }
    }
    let total_h: f64 = lines.iter().map(|l| l.height).sum();
    let col_total = if col_count == 1 { total_h } else { total_h / col_count as f64 };
    let mut cy = match el.s("verticalAlign") {
        Some("middle") => y + h / 2.0 - col_total / 2.0,
        Some("bottom") => y + h - pad - col_total,
        _ => y + pad,
    };
    let mut col = 0usize;
    let col_top = cy;
    let col_bottom = y + h - pad;
    let mut placed = Vec::with_capacity(lines.len());
    for line in lines {
        if col_count > 1 && col < col_count - 1 && cy + line.height > col_bottom && cy > col_top {
            col += 1;
            cy = col_top;
        }
        let col_left = x + pad + col as f64 * (col_w + col_gap);
        let baseline = cy + line.ascent;
        let (mut content_w, mut trailing, mut gaps) = (0.0, 0.0, 0usize);
        for seg in &line.segs {
            content_w += seg.width;
            if seg.text.ends_with(richtext::is_space) {
                trailing += seg.width;
            } else {
                trailing = 0.0;
            }
            if !seg.text.is_empty() && seg.text.chars().all(richtext::is_space) {
                gaps += 1;
            }
        }
        let content_no_trail = content_w - trailing;
        let left = col_left + line.indent;
        let avail = col_w - line.indent;
        let mut pen_x = match line.align {
            Align::Center => left + (avail - content_no_trail) / 2.0,
            Align::Right => col_left + col_w - content_no_trail,
            _ => left,
        };
        let extra = if line.justify && gaps > 0 { (avail - content_no_trail) / gaps as f64 } else { 0.0 };
        let start_x = pen_x;
        let mut segs = Vec::with_capacity(line.segs.len());
        for seg in &line.segs {
            segs.push(PlacedSeg { seg: seg.clone(), x: pen_x });
            let is_gap = line.justify && !seg.text.is_empty() && seg.text.chars().all(richtext::is_space);
            pen_x += seg.width + if is_gap { extra } else { 0.0 };
        }
        let height = line.height;
        placed.push(PlacedLine { line, top: cy, baseline, col_left, pen_x: start_x, segs, extra_per_gap: extra });
        cy += height;
    }
    Some(TextBox { lines: placed, placeholder: is_placeholder, factor, letter_spacing: ls, defaults, paras: layout_paras })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::richtext::FixedMeasure;
    use crate::surface::{Op, Recorder};
    use serde_json::json;

    fn slide(elements: Value) -> SlideData {
        SlideData::from_value(json!({ "elements": elements, "background": { "type": "color", "color": "#ffffff" } }))
    }

    fn draw(data: &SlideData, mode: Mode) -> Recorder {
        let mut rec = Recorder::default();
        Renderer::new(960.0, 540.0, &FixedMeasure).render(&mut rec, Matrix::IDENTITY, data, &Theme::default(), &Options { mode, ..Options::default() });
        rec
    }

    #[test]
    fn the_background_then_the_elements_by_z_order() {
        let data = slide(json!([
            { "id": "b", "type": "shape", "shape": "rect", "x": 0.5, "y": 0.5, "w": 0.1, "h": 0.1, "zIndex": 2, "fill": { "type": "color", "color": "#ff0000" }, "stroke": { "width": 0 } },
            { "id": "a", "type": "shape", "shape": "rect", "x": 0.1, "y": 0.1, "w": 0.1, "h": 0.1, "zIndex": 1, "fill": { "type": "color", "color": "#00ff00" }, "stroke": { "width": 0 } },
            { "id": "h", "type": "shape", "shape": "rect", "hidden": true, "x": 0, "y": 0, "w": 1, "h": 1, "fill": { "type": "color", "color": "#000" } } ]));
        let rec = draw(&data, Mode::Edit);
        let fills: Vec<Paint> = rec.ops.iter().filter_map(|o| if let Op::Fill { paint, .. } = o { Some(paint.clone()) } else { None }).collect();
        assert_eq!(fills, [Paint::css("#ffffff"), Paint::css("#00ff00"), Paint::css("#ff0000")]);
    }

    #[test]
    fn placeholders_show_in_the_editor_only() {
        let data = slide(json!([{ "id": "t", "type": "text", "x": 0.1, "y": 0.1, "w": 0.8, "h": 0.2, "content": null, "padding": 8, "placeholder": "Titre", "fontSize": 44 }]));
        let texts = |m| draw(&data, m).ops.iter().filter(|o| matches!(o, Op::Text { .. })).count();
        assert_eq!(texts(Mode::Edit), 1);
        assert_eq!(texts(Mode::Thumbnail), 0);
        assert_eq!(texts(Mode::Present), 0);
    }

    #[test]
    fn a_rotated_element_turns_about_its_centre() {
        let data = slide(json!([{ "id": "r", "type": "shape", "shape": "rect", "x": 0.0, "y": 0.0, "w": 0.5, "h": 0.5, "rotation": 90, "fill": { "type": "color", "color": "#000" } }]));
        let rec = draw(&data, Mode::Edit);
        let st = rec.ops.iter().filter_map(|o| if let Op::Fill { st, .. } = o { Some(st.clone()) } else { None }).nth(1).expect("shape fill");
        let (cx, cy) = st.m.apply(240.0, 135.0);
        assert!((cx - 240.0).abs() < 1e-9 && (cy - 135.0).abs() < 1e-9, "the centre stays: {cx},{cy}");
    }

    #[test]
    fn text_is_centred_and_vertically_middled() {
        let el = Element::from_value(json!({ "type": "text", "x": 0, "y": 0, "w": 0.5, "h": 0.5, "padding": 0, "align": "center", "verticalAlign": "middle", "fontSize": 20,
            "content": { "type": "doc", "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "abcd" }] }] } })).expect("el");
        let tb = layout_text_box(&el, 0.0, 0.0, 480.0, 270.0, &Theme::default(), true, 1.0, &FixedMeasure).expect("box");
        let l = &tb.lines[0];
        assert!((l.segs[0].x - (240.0 - 20.0)).abs() < 1e-9, "x {}", l.segs[0].x);
        assert!((l.top - (135.0 - 13.0)).abs() < 1e-9, "top {}", l.top);
    }

    #[test]
    fn shrink_reduces_until_the_text_fits() {
        let long = "word ".repeat(80);
        let el = Element::from_value(json!({ "type": "text", "x": 0, "y": 0, "w": 0.3, "h": 0.1, "autofit": "shrink",
            "content": { "type": "doc", "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": long }] }] } })).expect("el");
        let tb = layout_text_box(&el, 0.0, 0.0, 288.0, 54.0, &Theme::default(), true, 1.0, &FixedMeasure).expect("box");
        assert!(tb.factor < 1.0);
    }

    #[test]
    fn a_line_with_an_arrow_draws_its_head() {
        let data = slide(json!([{ "id": "l", "type": "line", "lineType": "arrow", "x": 0.1, "y": 0.1, "x2": 0.5, "y2": 0.1, "w": 0, "h": 0, "stroke": { "color": "#202124", "width": 2, "style": "solid" }, "arrowEnd": "triangle" }]));
        let rec = draw(&data, Mode::Edit);
        assert_eq!(rec.ops.iter().filter(|o| matches!(o, Op::Stroke { .. })).count(), 1);
        assert_eq!(rec.ops.iter().filter(|o| matches!(o, Op::Fill { .. })).count(), 2, "background + head");
    }

    #[test]
    fn an_image_not_loaded_shows_the_placeholder() {
        let data = slide(json!([{ "id": "i", "type": "image", "x": 0, "y": 0, "w": 0.2, "h": 0.2, "storagePath": "kbfile:1" }]));
        let rec = draw(&data, Mode::Edit);
        assert!(rec.ops.iter().any(|o| matches!(o, Op::Text { text, .. } if text == "Image")));
        let mut rec = Recorder { images: vec![("kbfile:1".into(), 100.0, 100.0)], ..Recorder::default() };
        Renderer::new(960.0, 540.0, &FixedMeasure).render(&mut rec, Matrix::IDENTITY, &data, &Theme::default(), &Options::default());
        assert!(rec.ops.iter().any(|o| matches!(o, Op::Image { .. })));
    }

    #[test]
    fn the_entry_animation_hides_until_revealed_and_fades_in() {
        let data = slide(json!([{ "id": "a", "type": "shape", "shape": "rect", "x": 0, "y": 0, "w": 0.1, "h": 0.1, "anim": { "type": "fade" }, "fill": { "type": "color", "color": "#000" } }]));
        let hidden: HashSet<String> = ["a".to_string()].into();
        let mut rec = Recorder::default();
        let r = Renderer::new(960.0, 540.0, &FixedMeasure);
        r.render(&mut rec, Matrix::IDENTITY, &data, &Theme::default(), &Options { mode: Mode::Present, hidden: Some(&hidden), ..Options::default() });
        assert_eq!(rec.ops.len(), 1, "background only");
        let mut rec = Recorder::default();
        r.render(&mut rec, Matrix::IDENTITY, &data, &Theme::default(), &Options { mode: Mode::Present, hidden: Some(&hidden), animating: Some(("a", 0.25)), ..Options::default() });
        assert!(matches!(&rec.ops[1], Op::Fill { st, .. } if (st.alpha - 0.25).abs() < 1e-12));
    }
}
