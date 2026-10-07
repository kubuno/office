//! The editor's furniture over the slide — the web's DOM overlay of `SlideCanvas`
//! (`PresentationEditorPage.tsx:3330-3700`), drawn through the same [`Ctx`] as the slide: the selection frame and
//! its handles, the rotation handle, the yellow knobs, the multi-selection frames, the marquee, the smart and
//! manual guides, the grid, the ruler ticks, the crop frame, the text caret and selection, the table cell editor.
//!
//! Coordinates are canvas pixels with the slide's top-left at the origin (`slide px × scale`). Icons (the rotate
//! arrow, the lock, the fit and image-bar buttons) are left to the platform's icon set: [`paint`] returns where
//! each goes.

use crate::color::Rgba;
use crate::edit::{overlay_of, Editor, CORNER, KNOB, PILL, ROTATE, ROTATE_TOP};
use crate::model::{Element, Rect, SLIDE_H, SLIDE_W};
use crate::render::TextBox;
use crate::surface::{Baseline, Ctx, Font, Matrix, Paint, PathBuilder, Stroke};
use crate::textedit::TextEdit;

/// `--color-primary`.
pub const PRIMARY: &str = "#1a73e8";
/// Smart guides.
pub const MAGENTA: &str = "#e1149e";
/// Manual guides (`red-400`).
pub const GUIDE: &str = "#f87171";

/// An icon the platform draws (a Lucide name in a canvas-px rectangle, rotated by `rotation` degrees about the
/// overlay box's centre).
#[derive(Debug, Clone, PartialEq)]
pub struct IconSpot {
    pub name: &'static str,
    pub rect: Rect,
    pub color: &'static str,
    pub rotation: f64,
    pub center: (f64, f64),
}

/// What the overlay needs besides the editor.
pub struct Scene<'a> {
    pub elements: &'a [Element],
    pub scale: f64,
    pub show_ruler: bool,
    /// The text being edited and its layout (canvas px, unrotated frame of its element).
    pub text: Option<(&'a TextEdit, &'a TextBox)>,
    /// The caret is in its visible half of the blink.
    pub caret_on: bool,
    /// The table cell being edited: (element, row, col, value, caret).
    pub cell: Option<(&'a str, usize, usize, &'a str, usize)>,
    pub measure: &'a dyn crate::richtext::Measure,
    /// The tertiary text colour (the lock frame, the ruler).
    pub tertiary: &'a str,
}

fn stroke(w: f64) -> Stroke {
    Stroke { width: w, ..Stroke::default() }
}

fn rect_path(x: f64, y: f64, w: f64, h: f64) -> crate::Path {
    PathBuilder::new().rect(x, y, w, h).take()
}

/// Rotates the context about a box's centre (the overlay div's `rotate(deg)`).
fn rotated(ctx: &mut Ctx, b: Rect, deg: f64) {
    if deg != 0.0 {
        let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
        ctx.translate(cx, cy);
        ctx.rotate(deg.to_radians());
        ctx.translate(-cx, -cy);
    }
}

fn circle(ctx: &mut Ctx, cx: f64, cy: f64, d: f64, fill: &str, border: &str, bw: f64) {
    let p = PathBuilder::new().arc(cx, cy, d / 2.0, 0.0, std::f64::consts::TAU, false).close().take();
    ctx.fill(&p, &Paint::css(fill));
    // A CSS border is inside the box.
    let inner = PathBuilder::new().arc(cx, cy, (d - bw) / 2.0, 0.0, std::f64::consts::TAU, false).close().take();
    ctx.stroke(&inner, &Paint::css(border), &stroke(bw));
}

fn pill(ctx: &mut Ctx, cx: f64, cy: f64, w: f64, h: f64) {
    let r = w.min(h) / 2.0;
    let p = PathBuilder::new().round_rect_q(cx - w / 2.0, cy - h / 2.0, w, h, r).take();
    ctx.fill(&p, &Paint::css("#ffffff"));
    let q = PathBuilder::new().round_rect_q(cx - w / 2.0 + 1.0, cy - h / 2.0 + 1.0, w - 2.0, h - 2.0, (r - 1.0).max(0.0)).take();
    ctx.stroke(&q, &Paint::css(PRIMARY), &stroke(2.0));
}

/// Paints the overlay; returns the icons for the platform.
pub fn paint(ctx: &mut Ctx, ed: &Editor, scene: &Scene) -> Vec<IconSpot> {
    let mut icons = Vec::new();
    let s = scene.scale;
    let (sw, sh) = (SLIDE_W * s, SLIDE_H * s);
    let px = |r: Rect| Rect { x: r.x * sw, y: r.y * sh, w: r.w * sw, h: r.h * sh };
    // Grid (`showGrid`): 1 px lines every 1/24 of the slide.
    if ed.show_grid {
        let ink = Paint::Solid(Rgba { r: 26.0 / 255.0, g: 115.0 / 255.0, b: 232.0 / 255.0, a: 0.12 });
        let mut b = PathBuilder::new();
        for i in 0..=24 {
            let x = (i as f64 * crate::edit::GRID * sw).floor() + 0.5;
            b.move_to(x, 0.0).line_to(x, sh);
            let y = (i as f64 * crate::edit::GRID * sh).floor() + 0.5;
            b.move_to(0.0, y).line_to(sw, y);
        }
        ctx.stroke(&b.take(), &ink, &stroke(1.0));
    }
    // Ruler ticks outside the slide (`showRuler`): 13 on top (long every 3), 9 on the left (long every 2).
    if scene.show_ruler {
        let ink = Paint::css(scene.tertiary);
        for i in 0..=12 {
            let x = i as f64 * sw / 12.0;
            let h = if i % 3 == 0 { 8.0 } else { 4.0 };
            ctx.fill_rect(x, -h, 1.0, h, &ink);
        }
        for i in 0..=8 {
            let y = i as f64 * sh / 8.0;
            let w = if i % 2 == 0 { 8.0 } else { 4.0 };
            ctx.fill_rect(-w, y, w, 1.0, &ink);
        }
    }
    let editing = scene.text.map(|t| t.0.id.as_str());
    let by_id = |id: &str| scene.elements.iter().find(|e| e.id() == id);
    // The text being edited: the editor's outline (2 px outside the box), the selection, the caret.
    if let Some((te, tb)) = scene.text {
        if let Some(el) = by_id(&te.id) {
            let b = px(el.bbox());
            ctx.save();
            rotated(ctx, b, el.rotation());
            ctx.stroke(&rect_path(b.x - 1.0, b.y - 1.0, b.w + 2.0, b.h + 2.0), &Paint::css(PRIMARY), &stroke(2.0));
            let sel = Paint::Solid(Rgba { r: 0.0, g: 120.0 / 255.0, b: 215.0 / 255.0, a: 0.3 });
            for (x, y, w, h) in te.selection_rects(tb, scene.measure) {
                ctx.fill_rect(x, y, w, h, &sel);
            }
            if scene.caret_on && te.is_collapsed() {
                if let Some((x, top, h)) = te.caret_rect(tb, scene.measure) {
                    ctx.fill_rect(x.round(), top, 1.0, h, &Paint::css("#202124"));
                }
            }
            ctx.restore();
        }
        return icons;
    }
    // The table cell being edited: a white input with a 2 px outline.
    if let Some((id, row, col, value, caret)) = scene.cell {
        if let Some(el) = by_id(id) {
            let b = px(el.bbox());
            let cx = crate::table::col_edges(el, b.w);
            let ry = crate::table::row_edges(el, b.h);
            if row + 1 < ry.len() && col + 1 < cx.len() {
                let r = Rect { x: b.x + cx[col], y: b.y + ry[row], w: cx[col + 1] - cx[col], h: ry[row + 1] - ry[row] };
                ctx.fill_rect(r.x, r.y, r.w, r.h, &Paint::css("#ffffff"));
                ctx.stroke(&rect_path(r.x - 1.0, r.y - 1.0, r.w + 2.0, r.h + 2.0), &Paint::css(PRIMARY), &stroke(2.0));
                let fs = crate::table::font_size(el) * s;
                let style = crate::render::plain_style("Arial, sans-serif", fs, "#202124");
                let font = Font { family: "Arial, sans-serif".into(), size: fs, bold: false, italic: false };
                ctx.fill_text(value, &font, r.x + 6.0, r.y + r.h / 2.0, Baseline::Middle, &Paint::css("#202124"), 0.0);
                if scene.caret_on {
                    let before: String = value.chars().take(caret).collect();
                    let x = r.x + 6.0 + scene.measure.width(&before, &style, 0.0);
                    ctx.fill_rect(x.round(), r.y + (r.h - fs * 1.2) / 2.0, 1.0, fs * 1.2, &Paint::css("#202124"));
                }
            }
        }
        return icons;
    }
    let _ = editing;
    // Multi-selection: a thin frame per element, the common box dashed.
    if ed.crop.is_none() && ed.selection.len() > 1 {
        let els: Vec<&Element> = scene.elements.iter().filter(|e| ed.selection.iter().any(|s| s == e.id())).collect();
        if els.len() >= 2 {
            for el in &els {
                let b = px(el.bbox());
                ctx.save();
                rotated(ctx, b, el.rotation());
                ctx.stroke(&rect_path(b.x + 0.5, b.y + 0.5, b.w - 1.0, b.h - 1.0), &Paint::Solid(Rgba { r: 26.0 / 255.0, g: 115.0 / 255.0, b: 232.0 / 255.0, a: 0.7 }), &stroke(1.0));
                ctx.restore();
            }
            if let Some(cb) = crate::edit::combined_bbox(els.iter().copied()) {
                let b = px(cb);
                ctx.stroke(&rect_path(b.x + 1.0, b.y + 1.0, b.w - 2.0, b.h - 2.0), &Paint::css(PRIMARY), &Stroke { width: 2.0, dash: vec![6.0, 4.0], ..Stroke::default() });
            }
        }
    }
    // The marquee.
    if let Some(m) = ed.marquee {
        let b = px(m);
        ctx.fill_rect(b.x, b.y, b.w, b.h, &Paint::Solid(Rgba { r: 26.0 / 255.0, g: 115.0 / 255.0, b: 232.0 / 255.0, a: 0.1 }));
        ctx.stroke(&rect_path(b.x + 0.5, b.y + 0.5, b.w - 1.0, b.h - 1.0), &Paint::css(PRIMARY), &stroke(1.0));
    }
    // One element: the frame and its handles (or, locked, a dashed frame and a lock).
    if ed.crop.is_none() && ed.selection.len() == 1 {
        if let Some(el) = by_id(&ed.selection[0]) {
            let ov = overlay_of(el, s);
            let b = ov.box_px;
            ctx.save();
            rotated(ctx, b, ov.rotation);
            if ov.locked {
                ctx.stroke(&rect_path(b.x + 1.0, b.y + 1.0, b.w - 2.0, b.h - 2.0), &Paint::css(scene.tertiary), &Stroke { width: 2.0, dash: vec![6.0, 4.0], ..Stroke::default() });
                icons.push(IconSpot { name: "Lock", rect: Rect { x: b.x + b.w - 15.0, y: b.y + 2.0, w: 13.0, h: 13.0 }, color: "tertiary", rotation: ov.rotation, center: (b.x + b.w / 2.0, b.y + b.h / 2.0) });
            } else {
                ctx.stroke(&rect_path(b.x + 1.0, b.y + 1.0, b.w - 2.0, b.h - 2.0), &Paint::css(PRIMARY), &stroke(2.0));
                if !ov.is_line {
                    let cx = b.x + b.w / 2.0;
                    ctx.fill_rect(cx - 1.0, b.y - 24.0, 2.0, 24.0, &Paint::css(PRIMARY));
                    let ry = b.y - ROTATE_TOP + ROTATE / 2.0;
                    circle(ctx, cx, ry, ROTATE, "#ffffff", PRIMARY, 2.0);
                    icons.push(IconSpot { name: "RotateCw", rect: Rect { x: cx - 5.5, y: ry - 5.5, w: 11.0, h: 11.0 }, color: "primary", rotation: ov.rotation, center: (cx, b.y + b.h / 2.0) });
                    for (hx, hy) in [(b.x, b.y), (b.x + b.w, b.y), (b.x, b.y + b.h), (b.x + b.w, b.y + b.h)] {
                        circle(ctx, hx, hy, CORNER, "#ffffff", PRIMARY, 2.0);
                    }
                    pill(ctx, cx, b.y, PILL.0, PILL.1);
                    pill(ctx, cx, b.y + b.h, PILL.0, PILL.1);
                    pill(ctx, b.x, b.y + b.h / 2.0, PILL.1, PILL.0);
                    pill(ctx, b.x + b.w, b.y + b.h / 2.0, PILL.1, PILL.0);
                    for &(_, kx, ky) in &ov.knobs {
                        circle(ctx, b.x + kx, b.y + ky, KNOB, "#ffd400", "#8a6d00", 1.0);
                    }
                    let center = (b.x + b.w / 2.0, b.y + b.h / 2.0);
                    if ov.kind == "text" {
                        // The fit button (32 px, bottom-left).
                        let (fx, fy) = (b.x - 8.0 + 16.0, b.y + b.h + 8.0 + 16.0);
                        circle(ctx, fx, fy, 32.0, "#ffffff", "#e0e0e0", 1.0);
                        icons.push(IconSpot { name: "AlignVerticalSpaceAround", rect: Rect { x: fx - 7.5, y: fy - 7.5, w: 15.0, h: 15.0 }, color: "secondary", rotation: ov.rotation, center });
                    }
                    if ov.kind == "image" {
                        let top = b.y + b.h + 12.0;
                        let bar = PathBuilder::new().round_rect_q(b.x, top, 6.0 + 4.0 * 30.0 - 2.0 + 6.0, 36.0, 18.0).take();
                        ctx.fill(&bar, &Paint::css("#ffffff"));
                        ctx.stroke(&bar, &Paint::css("#e0e0e0"), &stroke(1.0));
                        for (i, name) in ["Image", "Crop", "Maximize2", "Accessibility"].into_iter().enumerate() {
                            let x0 = b.x + 6.0 + i as f64 * 30.0;
                            icons.push(IconSpot { name, rect: Rect { x: x0 + 6.5, y: top + 10.5, w: 15.0, h: 15.0 }, color: "secondary", rotation: ov.rotation, center });
                        }
                    }
                }
            }
            ctx.restore();
        }
    }
    // The crop: the whole image dimmed, the kept part bright, a 2 px frame and 8 square handles.
    if let Some(c) = &ed.crop {
        if let Some(el) = by_id(&c.id) {
            let src = el.s("storagePath").unwrap_or("");
            let prev = ctx.alpha();
            ctx.set_alpha(0.35);
            ctx.draw_image(src, None, c.full);
            ctx.set_alpha(prev);
            ctx.save();
            ctx.clip(&rect_path(c.frame.x, c.frame.y, c.frame.w, c.frame.h));
            ctx.draw_image(src, None, c.full);
            ctx.restore();
            let f = c.frame;
            ctx.stroke(&rect_path(f.x - 1.0, f.y - 1.0, f.w + 2.0, f.h + 2.0), &Paint::css(PRIMARY), &stroke(2.0));
            for (hx, hy) in [(f.x, f.y), (f.x + f.w, f.y), (f.x, f.y + f.h), (f.x + f.w, f.y + f.h), (f.x + f.w / 2.0, f.y), (f.x + f.w / 2.0, f.y + f.h), (f.x, f.y + f.h / 2.0), (f.x + f.w, f.y + f.h / 2.0)] {
                ctx.fill_rect(hx - 6.0, hy - 6.0, 12.0, 12.0, &Paint::css("#ffffff"));
                ctx.stroke(&rect_path(hx - 5.0, hy - 5.0, 10.0, 10.0), &Paint::css("#444444"), &stroke(2.0));
            }
        }
    }
    // Manual guides (red), then the smart guides of the gesture (magenta).
    if ed.show_guides {
        for g in &ed.guides {
            if g.vertical {
                ctx.fill_rect((g.pos * sw).floor(), 0.0, 1.0, sh, &Paint::css(GUIDE));
            } else {
                ctx.fill_rect(0.0, (g.pos * sh).floor(), sw, 1.0, &Paint::css(GUIDE));
            }
        }
    }
    for g in &ed.snap_guides {
        let (lo, hi) = (g.a.min(g.b).max(0.0), g.a.max(g.b).min(1.0));
        if g.vertical {
            ctx.fill_rect(g.pos * sw - 0.5, lo * sh, 1.0, (hi - lo) * sh, &Paint::css(MAGENTA));
        } else {
            ctx.fill_rect(lo * sw, g.pos * sh - 0.5, (hi - lo) * sw, 1.0, &Paint::css(MAGENTA));
        }
    }
    icons
}

/// The matrix that maps the overlay's canvas px onto the slide's place in a control.
pub fn base(origin_x: f64, origin_y: f64) -> Matrix {
    Matrix::translation(origin_x, origin_y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::richtext::FixedMeasure;
    use crate::surface::{Op, Recorder};
    use serde_json::json;

    #[test]
    fn a_selected_shape_gets_its_handles_and_a_text_box_its_fit_button() {
        let els = vec![Element::from_value(json!({ "id": "t", "type": "text", "x": 0.1, "y": 0.1, "w": 0.3, "h": 0.2 })).unwrap_or_default()];
        let mut ed = Editor::new();
        ed.select(vec!["t".into()]);
        let mut rec = Recorder::default();
        let icons = {
            let mut ctx = Ctx::new(&mut rec, Matrix::IDENTITY);
            paint(&mut ctx, &ed, &Scene { elements: &els, scale: 1.0, show_ruler: false, text: None, caret_on: false, cell: None, measure: &FixedMeasure, tertiary: "#80868b" })
        };
        assert!(icons.iter().any(|i| i.name == "RotateCw"));
        assert!(icons.iter().any(|i| i.name == "AlignVerticalSpaceAround"));
        // Frame + stem + rotate circle + 4 corners + 4 pills (fills and strokes).
        assert!(rec.ops.iter().filter(|o| matches!(o, Op::Fill { .. })).count() >= 10);
    }
}
