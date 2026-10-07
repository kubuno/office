//! Paints a sheet shape as the web's `ShapeView` does (`web/src/sheet-shape/ShapeView.tsx`): the ten native
//! geometries of the sheet through the shared native engine with the shape's own outline width, every other
//! catalogue kind through the OOXML presets (`shapeSvg`: a size-proportional outline), the lines and
//! connectors hand-drawn like `shapes/svg.ts` (with their arrow heads), then the caption in the geometry's
//! text box. Mirroring applies to the outline only.

use kubuno_desktop::ui::graphics::{Brush, FillMode, Font, FontStyle, Graphics, GraphicsPath, LineCap, LineJoin, Pen, PointF, StringAlignment, StringFormat, StringFormatFlags, StringTrimming};
use kubuno_desktop::ui::Rect;
use kubuno_office_shapes_core::{native, path::shade_colour, path::Cmd, preset, resolve_shape_kind};
use serde_json::Value as Json;

use super::prims::css_color;

/// The sheet's own geometries (`NATIVE_KINDS`).
const NATIVE_KINDS: [&str; 10] = ["rect", "roundRect", "ellipse", "triangle", "diamond", "arrow", "line", "star", "callout", "plus"];
/// The bespoke lines and connectors of `shapes/svg.ts`.
const LINE_KINDS: [&str; 9] = ["lineArrow", "lineDouble", "elbowConnector", "elbowArrow", "elbowDoubleArrow", "curveConnector", "curveArrow", "curveDoubleArrow", "curve"];
/// `DEFAULT_SHAPE_FONT_PT`.
const DEFAULT_FONT_PT: f64 = 11.0;
const SHAPE_ACCENT: &str = "#1a73e8";

fn s<'a>(o: &'a Json, k: &str) -> Option<&'a str> {
    o.get(k).and_then(Json::as_str)
}

fn adj(o: &Json) -> Option<Vec<f64>> {
    o.get("adj").and_then(Json::as_array).map(|a| a.iter().filter_map(Json::as_f64).collect())
}

/// `paintOf`: the resolved fill, stroke and outline width (zoomed) of a native kind.
fn paint_of(o: &Json, kind: &str, zoom: f64) -> (String, String, f64) {
    let open = !native::shape_geometry(native::as_native(kind), 10.0, 10.0, 0.0, None).closed;
    let fill = s(o, "fill").unwrap_or("#ffffff").to_string();
    let border = s(o, "border").unwrap_or("#000000").to_string();
    let width = o.get("borderWidth").and_then(Json::as_f64).unwrap_or(1.0).max(0.0) * zoom;
    if open {
        let stroke = if border == "none" {
            if fill == "none" {
                SHAPE_ACCENT.to_string()
            } else {
                fill.clone()
            }
        } else {
            border
        };
        return ("none".into(), stroke, (1.0 * zoom).max(width));
    }
    let sw = if border == "none" { 0.0 } else { width };
    (fill, border, sw)
}

fn add_cmds(path: &mut GraphicsPath, cmds: &[Cmd], map: &dyn Fn(f64, f64) -> PointF, scale: f32) {
    for c in cmds {
        match *c {
            Cmd::MoveTo(x, y) => path.move_to(map(x, y)),
            Cmd::LineTo(x, y) => path.line_to(map(x, y)),
            Cmd::CubicTo(a, b, c2, d, x, y) => path.bezier_to(map(a, b), map(c2, d), map(x, y)),
            Cmd::QuadTo(a, b, x, y) => path.quad_to(map(a, b), map(x, y)),
            Cmd::ArcTo { rx, ry, large, sweep, x, y } => path.arc_to(map(x, y), rx as f32 * scale, ry as f32 * scale, 0.0, large, sweep),
            Cmd::Close => path.close_figure(),
        }
    }
}

/// A shape's box on the surface (rotation applied by the caller).
pub struct ShapeBox {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// The sheet zoom (outline widths and caption size scale with it).
    pub zoom: f64,
}

/// Paints the outline of a shape of `kind` (the sheet's stored shape `o`) in `b`.
pub fn paint_glyph(g: &Graphics<'_>, o: &Json, b: &ShapeBox, dark: bool) {
    let kind = s(o, "kind").unwrap_or("rect");
    let (flip_h, flip_v) = (o.get("flipH").and_then(Json::as_bool).unwrap_or(false), o.get("flipV").and_then(Json::as_bool).unwrap_or(false));
    let (w, h) = (b.w as f64, b.h as f64);
    // Device scale is 1: the box is already in DIP at the sheet zoom.
    let map = |x: f64, y: f64| {
        let x = if flip_h { w - x } else { x };
        let y = if flip_v { h - y } else { y };
        PointF::new(b.x + x as f32, b.y + y as f32)
    };
    let av = adj(o);
    if NATIVE_KINDS.contains(&kind) {
        let (fill, stroke, sw) = paint_of(o, kind, b.zoom);
        let geo = native::shape_geometry(native::as_native(kind), w, h, sw / 2.0, av.as_deref());
        let mut path = GraphicsPath::new().with_fill_mode(FillMode::Winding);
        add_cmds(&mut path, &geo.path.cmds, &map, 1.0);
        if let Some(c) = css_color(&fill, 1.0, dark) {
            g.fill_path(Brush::solid(c), &path);
        }
        if let Some(c) = css_color(&stroke, 1.0, dark).filter(|_| sw > 0.0) {
            g.draw_path(&Pen::new(c, sw as f32).with_join(LineJoin::Round).with_caps(LineCap::Round), &path);
        }
        return;
    }
    // `shapeSvg(kind, w, h, fill, stroke, undefined, adj)`: the outline is proportional to the size.
    let (fill, stroke, _) = paint_of_preset(o);
    let m = w.min(h);
    let sw0 = (m * 0.0075).max(1.0);
    let resolved = resolve_shape_kind(kind).unwrap_or(kind);
    if let Some(p) = preset::preset_of(resolved) {
        let subs = preset::preset_path(p, &preset::Env { w, h, adj: preset::adj_values(p, av.as_deref()) });
        for sp in subs {
            let mut path = GraphicsPath::new().with_fill_mode(FillMode::Winding);
            add_cmds(&mut path, &sp.path.cmds, &map, 1.0);
            if sp.fill {
                if let Some(c) = css_color(&shade_colour(&fill, sp.shade), 1.0, dark) {
                    g.fill_path(Brush::solid(c), &path);
                }
            }
            if sp.stroke {
                if let Some(c) = css_color(&stroke, 1.0, dark) {
                    g.draw_path(&Pen::new(c, sw0 as f32).with_join(LineJoin::Round).with_caps(LineCap::Round), &path);
                }
            }
        }
        return;
    }
    if LINE_KINDS.contains(&kind) {
        paint_line_kind(g, kind, w, h, &stroke, dark, &map);
    }
}

/// Fill and stroke as `ShapeGlyph` hands them to `shapeSvg` (`paintOf` of a closed kind).
fn paint_of_preset(o: &Json) -> (String, String, f64) {
    let fill = s(o, "fill").unwrap_or("#ffffff").to_string();
    let border = s(o, "border").unwrap_or("#000000").to_string();
    (if fill == "none" { "none".into() } else { fill }, border, 0.0)
}

/// The connectors of `shapes/svg.ts` with their marker arrow heads.
fn paint_line_kind(g: &Graphics<'_>, kind: &str, w: f64, h: f64, stroke: &str, dark: bool, map: &dyn Fn(f64, f64) -> PointF) {
    let Some(color) = css_color(if stroke == "none" { SHAPE_ACCENT } else { stroke }, 1.0, dark) else { return };
    let m = w.min(h);
    let sw = (m * 0.0075).max(1.0);
    let cs = m * 0.045;
    let cs = cs.clamp(1.5, 3.5);
    let cx = w / 2.0;
    let mut path = GraphicsPath::new();
    let (start, end, start_dir, end_dir);
    match kind {
        "elbowConnector" | "elbowArrow" | "elbowDoubleArrow" => {
            let pts = [(sw, h - sw), (cx, h - sw), (cx, sw), (w - sw, sw)];
            path.move_to(map(pts[0].0, pts[0].1));
            for p in &pts[1..] {
                path.line_to(map(p.0, p.1));
            }
            start = pts[0];
            end = pts[3];
            start_dir = (pts[1].0 - pts[0].0, pts[1].1 - pts[0].1);
            end_dir = (pts[3].0 - pts[2].0, pts[3].1 - pts[2].1);
        }
        "curveConnector" | "curveArrow" | "curveDoubleArrow" => {
            let (p0, c1, c2, p3) = ((sw, h - sw), (w * 0.1, h * 0.15), (w * 0.9, h * 0.85), (w - sw, sw));
            path.move_to(map(p0.0, p0.1));
            path.bezier_to(map(c1.0, c1.1), map(c2.0, c2.1), map(p3.0, p3.1));
            start = p0;
            end = p3;
            start_dir = (c1.0 - p0.0, c1.1 - p0.1);
            end_dir = (p3.0 - c2.0, p3.1 - c2.1);
        }
        "curve" => {
            let (p0, c, p2) = ((sw, h - sw), (cx, sw), (w - sw, h - sw));
            path.move_to(map(p0.0, p0.1));
            path.quad_to(map(c.0, c.1), map(p2.0, p2.1));
            start = p0;
            end = p2;
            start_dir = (c.0 - p0.0, c.1 - p0.1);
            end_dir = (p2.0 - c.0, p2.1 - c.1);
        }
        _ => {
            let (p0, p1) = ((sw, h - sw), (w - sw, sw));
            path.move_to(map(p0.0, p0.1));
            path.line_to(map(p1.0, p1.1));
            start = p0;
            end = p1;
            start_dir = (p1.0 - p0.0, p1.1 - p0.1);
            end_dir = start_dir;
        }
    }
    g.draw_path(&Pen::new(color, cs as f32).with_join(LineJoin::Round).with_caps(LineCap::Round), &path);
    let ah = (m * 0.3).clamp(5.0, 20.0);
    let head = |at: (f64, f64), dir: (f64, f64), back: bool| {
        // The marker's triangle, its reference point on the line's end, turned along the line.
        let len = (dir.0 * dir.0 + dir.1 * dir.1).sqrt().max(1e-9);
        let (ux, uy) = (dir.0 / len, dir.1 / len);
        let (nx, ny) = (-uy, ux);
        let local: [(f64, f64); 3] = if back { [(ah, 0.0), (0.0, ah / 2.0), (ah, ah)] } else { [(0.0, 0.0), (ah, ah / 2.0), (0.0, ah)] };
        let (rx, ry) = if back { (ah * 0.12, ah / 2.0) } else { (ah * 0.88, ah / 2.0) };
        let pts: Vec<PointF> = local
            .iter()
            .map(|(lx, ly)| {
                let (dx, dy) = (lx - rx, ly - ry);
                let (x, y) = (at.0 + dx * ux + dy * nx, at.1 + dx * uy + dy * ny);
                map(x, y)
            })
            .collect();
        g.fill_polygon(Brush::solid(color), &pts);
    };
    let ends_arrow = matches!(kind, "lineArrow" | "lineDouble" | "elbowArrow" | "elbowDoubleArrow" | "curveArrow" | "curveDoubleArrow");
    let starts_arrow = matches!(kind, "lineDouble" | "elbowDoubleArrow" | "curveDoubleArrow");
    if ends_arrow {
        head(end, end_dir, false);
    }
    if starts_arrow {
        head(start, start_dir, true);
    }
}

/// The caption's box in the shape (`presetTextFrame` / the native geometry's text rect).
fn text_box(o: &Json, kind: &str, w: f64, h: f64, zoom: f64) -> (f64, f64, f64, f64) {
    let av = adj(o);
    if NATIVE_KINDS.contains(&kind) {
        let (_, _, sw) = paint_of(o, kind, zoom);
        let t = native::shape_geometry(native::as_native(kind), w, h, sw / 2.0, av.as_deref()).text;
        return (t.x, t.y, t.w, t.h);
    }
    let t = kubuno_office_shapes_core::shape_text_box(kind, w, h, av.as_deref());
    (t.x, t.y, t.w, t.h)
}

/// Paints the caption (`ShapeView`'s text layer): wrapped, vertically centred, the text style's weight,
/// slant, colour, size (pt → px, × zoom) and alignment; spilling out of the box like Excel's.
pub fn paint_caption(g: &Graphics<'_>, o: &Json, b: &ShapeBox, dark: bool, family: &str) {
    let Some(text) = s(o, "text").filter(|t| !t.is_empty()) else { return };
    let kind = s(o, "kind").unwrap_or("rect");
    let (tx, ty, tw, th) = text_box(o, kind, b.w as f64, b.h as f64, b.zoom);
    let ts = o.get("textStyle");
    let bold = ts.and_then(|t| t.get("bold")).and_then(Json::as_bool).unwrap_or(false);
    let italic = ts.and_then(|t| t.get("italic")).and_then(Json::as_bool).unwrap_or(false);
    let color = ts.and_then(|t| t.get("color")).and_then(Json::as_str).unwrap_or("var(--color-text-primary, #202124)");
    let size = ts.and_then(|t| t.get("size")).and_then(Json::as_f64).unwrap_or(DEFAULT_FONT_PT) * 4.0 / 3.0 * b.zoom;
    let align = match ts.and_then(|t| t.get("align")).and_then(Json::as_str) {
        Some("left") => StringAlignment::Near,
        Some("right") => StringAlignment::Far,
        _ => StringAlignment::Center,
    };
    let Some(c) = css_color(color, 1.0, dark) else { return };
    let mut style = FontStyle::REGULAR.0;
    if bold {
        style |= FontStyle::BOLD.0;
    }
    if italic {
        style |= FontStyle::ITALIC.0;
    }
    let font = Font::with_dip(family, size.max(1.0) as f32, FontStyle(style));
    let fmt = StringFormat { alignment: align, line_alignment: StringAlignment::Center, trimming: StringTrimming::None, flags: StringFormatFlags::NO_CLIP };
    let r = Rect::new(b.x + tx as f32, b.y + ty as f32, b.x + (tx + tw) as f32, b.y + (ty + th) as f32);
    g.draw_string(text, &font, Brush::solid(c), r, &fmt);
}
