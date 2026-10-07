//! The scene — `renderCanvas` and `renderConnector` (`DiagramEditorPage.tsx:497-834`), the minimap
//! (`:866-938`), the rulers (`:940-989`) and the stencil thumbnails (`:836-864`), drawn through
//! [`Ctx2D`] exactly as the web draws them.

use crate::canvas::{Ctx2D, Point};
use crate::geometry::{self, connector_label_center, connector_points, display_points, port_points, resize_handles, seg_intersect, HANDLE_MARGIN, HANDLE_R, PORT_R};
use crate::model::{Connector, LabelStyle, Shape, ShapeStyle};
use crate::stencils;

/// What `renderCanvas` takes besides the canvas.
pub struct Scene<'a> {
    /// The shapes and connectors to draw (hidden layers removed, layer-ordered).
    pub shapes: &'a [Shape],
    pub connectors: &'a [Connector],
    pub zoom: f64,
    pub pan_x: f64,
    pub pan_y: f64,
    pub selected: &'a [String],
    pub selected_conns: &'a [String],
    pub hovered: Option<&'a str>,
    /// The rubber-band connector: start and current point (world).
    pub drawing_conn: Option<(Point, Point)>,
    /// The lasso: two corners (world).
    pub lasso: Option<(Point, Point)>,
    pub bg_color: &'a str,
    /// Segments highlighted green while a node or a shape snaps (connector id, segment indices).
    pub magnet_segs: &'a [(String, Vec<usize>)],
    pub show_grid: bool,
    pub grid_size: f64,
    /// Alignment guides: vertical lines (x) and horizontal lines (y), world.
    pub guides: Option<(&'a [f64], &'a [f64])>,
    pub drawing_shape: Option<&'a stencils::DrawingShape>,
    /// The canvas size, CSS pixels.
    pub width: f64,
    pub height: f64,
}

/// JavaScript's `v || 1` for a number: 0 and NaN become 1.
fn or_one(v: f64) -> f64 {
    if v == 0.0 || v.is_nan() {
        1.0
    } else {
        v
    }
}

/// The selection blue.
pub const SELECT_BLUE: &str = "#1a73e8";

/// `renderConnector`: routing, corner rounding (radius 8), line hops (radius 5) over older connectors,
/// green magnet segments, arrow heads, the label on its white box.
pub fn render_connector(ctx: &mut Ctx2D, conn: &Connector, shapes: &[Shape], selected: bool, green: Option<&[usize]>, hops: &[Vec<Point>]) {
    let pts = display_points(conn, shapes);
    let st = conn.style();
    let color = if selected { SELECT_BLUE.to_string() } else { st.stroke_color.clone() };
    ctx.save();
    ctx.set_stroke_style(&color);
    ctx.set_line_width(st.stroke_width);
    ctx.set_line_dash(ShapeStyle::dash(&st.stroke_style));

    const BEND_R: f64 = 8.0;
    const HOP_R: f64 = 5.0;
    let dist = |p: Point, q: Point| (q.x - p.x).hypot(q.y - p.y);
    let mut rad = vec![0.0; pts.len()];
    for i in 1..pts.len().saturating_sub(1) {
        rad[i] = BEND_R.min(dist(pts[i - 1], pts[i]) / 2.0).min(dist(pts[i], pts[i + 1]) / 2.0);
    }
    ctx.begin_path();
    if let Some(p0) = pts.first() {
        ctx.move_to(p0.x, p0.y);
    }
    for s in 0..pts.len().saturating_sub(1) {
        let (p0, p1) = (pts[s], pts[s + 1]);
        let seg_len = or_one(dist(p0, p1));
        let (dx, dy) = ((p1.x - p0.x) / seg_len, (p1.y - p0.y) / seg_len);
        let start_cut = if s > 0 { rad[s] } else { 0.0 };
        let end_cut = if s + 1 < pts.len() - 1 { rad[s + 1] } else { 0.0 };
        let a = Point::new(p0.x + dx * start_cut, p0.y + dy * start_cut);
        let b = Point::new(p1.x - dx * end_cut, p1.y - dy * end_cut);
        let straight = dist(a, b);
        let mut seg_hops: Vec<(Point, f64)> = hops
            .get(s)
            .map(|h| h.iter().map(|p| (*p, (p.x - a.x) * dx + (p.y - a.y) * dy)).filter(|(_, t)| *t > HOP_R && *t < straight - HOP_R).collect())
            .unwrap_or_default();
        seg_hops.sort_by(|u, v| u.1.total_cmp(&v.1));
        for (p, t) in seg_hops {
            let ha = Point::new(a.x + dx * (t - HOP_R), a.y + dy * (t - HOP_R));
            let hb = Point::new(a.x + dx * (t + HOP_R), a.y + dy * (t + HOP_R));
            ctx.line_to(ha.x, ha.y);
            let a0 = (ha.y - p.y).atan2(ha.x - p.x);
            let a1 = (hb.y - p.y).atan2(hb.x - p.x);
            ctx.arc(p.x, p.y, HOP_R, a0, a1, true);
        }
        ctx.line_to(b.x, b.y);
        if s + 1 < pts.len() - 1 {
            let p2 = pts[s + 2];
            let l2 = or_one(dist(p1, p2));
            let c_end = Point::new(p1.x + (p2.x - p1.x) / l2 * rad[s + 1], p1.y + (p2.y - p1.y) / l2 * rad[s + 1]);
            ctx.arc_to(p1.x, p1.y, c_end.x, c_end.y, rad[s + 1]);
        }
    }
    ctx.stroke();
    ctx.set_line_dash(&[]);

    if let Some(green) = green.filter(|g| !g.is_empty()) {
        ctx.save();
        ctx.set_stroke_style("#1e8e3e");
        ctx.set_line_width(st.stroke_width + 1.0);
        ctx.set_line_cap(crate::canvas::LineCap::Round);
        for &s in green {
            if s + 1 >= pts.len() {
                continue;
            }
            ctx.begin_path();
            ctx.move_to(pts[s].x, pts[s].y);
            ctx.line_to(pts[s + 1].x, pts[s + 1].y);
            ctx.stroke();
        }
        ctx.restore();
    }

    if pts.len() >= 2 {
        let last = pts[pts.len() - 1];
        let prev = pts[pts.len() - 2];
        if !st.arrow_end.is_empty() && st.arrow_end != "none" {
            stencils::draw_arrow(ctx, prev, last, &st.arrow_end, &color, st.stroke_width);
        }
        if !st.arrow_start.is_empty() && st.arrow_start != "none" {
            stencils::draw_arrow(ctx, pts[1], pts[0], &st.arrow_start, &color, st.stroke_width);
        }
    }

    let label = conn.label();
    if !label.is_empty() {
        let mid = connector_label_center(conn, shapes);
        ctx.set_font("12px Inter, sans-serif");
        let fm = ctx.measure_text(label);
        ctx.set_fill_style("#ffffff");
        ctx.fill_rect(mid.x - fm.width / 2.0 - 4.0, mid.y - 9.0, fm.width + 8.0, 18.0);
        stencils::draw_label(ctx, label, mid.x - 40.0, mid.y - 10.0, 80.0, 20.0, &LabelStyle::default());
    }
    ctx.restore();
}

/// The crossings of each segment of connector `i` with the connectors drawn before it (`hops`).
pub fn connector_hops(polys: &[Vec<Point>], i: usize) -> Vec<Vec<Point>> {
    let pi = &polys[i];
    (0..pi.len().saturating_sub(1))
        .map(|s| {
            let mut crossings = Vec::new();
            for pj in &polys[..i] {
                for t in 0..pj.len().saturating_sub(1) {
                    if let Some(x) = seg_intersect(pi[s], pi[s + 1], pj[t], pj[t + 1]) {
                        crossings.push(x);
                    }
                }
            }
            crossings
        })
        .collect()
}

/// `renderCanvas`: background, grid, connectors (with hops), connector handles, the rubber band, shapes
/// with their selection furniture and ports, the shape being drawn, the alignment guides, the lasso.
/// The context is in CSS pixels (the caller applies the device scale).
pub fn render_canvas(ctx: &mut Ctx2D, sc: &Scene) {
    let (w, h, zoom, pan_x, pan_y) = (sc.width, sc.height, sc.zoom, sc.pan_x, sc.pan_y);
    ctx.save();
    ctx.set_fill_style(if sc.bg_color.is_empty() { "#ffffff" } else { sc.bg_color });
    ctx.fill_rect(0.0, 0.0, w, h);

    if sc.show_grid && sc.grid_size > 0.0 {
        let step = sc.grid_size * zoom;
        if step >= 4.0 {
            let (start_x, start_y) = (pan_x % step, pan_y % step);
            ctx.save();
            ctx.set_line_width(1.0);
            ctx.set_stroke_style("rgba(0,0,0,0.05)");
            ctx.begin_path();
            let mut x = start_x;
            while x < w {
                ctx.move_to(x, 0.0);
                ctx.line_to(x, h);
                x += step;
            }
            let mut y = start_y;
            while y < h {
                ctx.move_to(0.0, y);
                ctx.line_to(w, y);
                y += step;
            }
            ctx.stroke();
            let major = step * 5.0;
            let (mx0, my0) = (pan_x % major, pan_y % major);
            ctx.set_stroke_style("rgba(0,0,0,0.10)");
            ctx.begin_path();
            let mut x = mx0;
            while x < w {
                ctx.move_to(x, 0.0);
                ctx.line_to(x, h);
                x += major;
            }
            let mut y = my0;
            while y < h {
                ctx.move_to(0.0, y);
                ctx.line_to(w, y);
                y += major;
            }
            ctx.stroke();
            ctx.restore();
        }
    }

    ctx.save();
    ctx.translate(pan_x, pan_y);
    ctx.scale(zoom, zoom);

    let polys: Vec<Vec<Point>> = sc.connectors.iter().map(|c| display_points(c, sc.shapes)).collect();
    for (i, conn) in sc.connectors.iter().enumerate() {
        let hops = connector_hops(&polys, i);
        let green = sc.magnet_segs.iter().find(|(id, _)| id == conn.id()).map(|(_, s)| s.as_slice());
        let selected = sc.selected_conns.iter().any(|id| id == conn.id());
        render_connector(ctx, conn, sc.shapes, selected, green, &hops);
    }

    // Node handles of the selected connectors: « + » at each segment's middle, the waypoints filled.
    for conn in sc.connectors {
        if !sc.selected_conns.iter().any(|id| id == conn.id()) {
            continue;
        }
        let pts = connector_points(conn, sc.shapes);
        for s in 0..pts.len().saturating_sub(1) {
            let (mx, my) = ((pts[s].x + pts[s + 1].x) / 2.0, (pts[s].y + pts[s + 1].y) / 2.0);
            ctx.begin_path();
            ctx.arc(mx, my, 4.0 / zoom, 0.0, std::f64::consts::TAU, false);
            ctx.set_fill_style("#ffffff");
            ctx.fill();
            ctx.set_stroke_style(SELECT_BLUE);
            ctx.set_line_width(1.5 / zoom);
            ctx.stroke();
            ctx.set_fill_style(SELECT_BLUE);
            ctx.set_font(&format!("{}px sans-serif", kubuno_office_shapes_core::path::js_num(7.0 / zoom)));
            ctx.set_text_align(crate::canvas::TextAlign::Center);
            ctx.set_text_baseline(crate::canvas::TextBaseline::Middle);
            ctx.fill_text("+", mx, my + 0.5 / zoom);
            ctx.set_text_align(crate::canvas::TextAlign::Left);
            ctx.set_text_baseline(crate::canvas::TextBaseline::Alphabetic);
        }
        for wp in conn.waypoints() {
            ctx.begin_path();
            ctx.arc(wp.x, wp.y, 5.0 / zoom, 0.0, std::f64::consts::TAU, false);
            ctx.set_fill_style(SELECT_BLUE);
            ctx.fill();
            ctx.set_stroke_style("#ffffff");
            ctx.set_line_width(1.5 / zoom);
            ctx.stroke();
        }
    }

    if let Some((a, b)) = sc.drawing_conn {
        ctx.save();
        ctx.set_stroke_style(SELECT_BLUE);
        ctx.set_line_width(1.5);
        ctx.set_line_dash(&[6.0, 3.0]);
        ctx.begin_path();
        ctx.move_to(a.x, a.y);
        ctx.line_to(b.x, b.y);
        ctx.stroke();
        ctx.set_line_dash(&[]);
        ctx.restore();
    }

    for shape in sc.shapes {
        let sel = sc.selected.iter().any(|id| id == shape.id());
        let style = shape.style();
        let draw_style = if sel { ShapeStyle { stroke_color: SELECT_BLUE.into(), stroke_width: 2.0, ..style } } else { style };
        let rot = shape.rotation();
        let transformed = shape.flip_h() || shape.flip_v() || rot != 0.0;
        if transformed {
            let c = shape.center();
            ctx.save();
            ctx.translate(c.x, c.y);
            if rot != 0.0 {
                ctx.rotate(rot.to_radians());
            }
            ctx.scale(if shape.flip_h() { -1.0 } else { 1.0 }, if shape.flip_v() { -1.0 } else { 1.0 });
            ctx.translate(-c.x, -c.y);
        }
        let adj = shape.adj();
        stencils::render_shape(ctx, shape.kind(), shape.x(), shape.y(), shape.w(), shape.h(), &draw_style, shape.label(), &shape.label_style(), adj.as_deref());
        if transformed {
            ctx.restore();
        }

        if sel {
            let m = HANDLE_MARGIN / zoom;
            ctx.save();
            ctx.set_stroke_style(SELECT_BLUE);
            ctx.set_line_width(1.0 / zoom);
            ctx.set_line_dash(&[4.0 / zoom, 2.0 / zoom]);
            ctx.stroke_rect(shape.x() - m, shape.y() - m, shape.w() + 2.0 * m, shape.h() + 2.0 * m);
            ctx.set_line_dash(&[]);
            for (p, _) in resize_handles(shape, m) {
                ctx.set_fill_style("#ffffff");
                ctx.set_stroke_style(SELECT_BLUE);
                ctx.set_line_width(1.5 / zoom);
                let r = HANDLE_R / zoom;
                ctx.fill_rect(p.x - r, p.y - r, r * 2.0, r * 2.0);
                ctx.stroke_rect(p.x - r, p.y - r, r * 2.0, r * 2.0);
            }
            ctx.restore();
            if sc.selected.len() == 1 {
                stencils::paint_adjust_handles(ctx, shape, zoom);
            }
        }

        if sc.hovered == Some(shape.id()) || sel {
            ctx.save();
            for p in port_points(shape) {
                ctx.begin_path();
                ctx.arc(p.x, p.y, PORT_R / zoom, 0.0, std::f64::consts::TAU, false);
                ctx.set_fill_style(SELECT_BLUE);
                ctx.fill();
                ctx.set_stroke_style("#ffffff");
                ctx.set_line_width(1.5 / zoom);
                ctx.stroke();
            }
            ctx.restore();
        }
    }

    if let Some(d) = sc.drawing_shape {
        stencils::paint_draw_ghost(ctx, d, zoom);
    }

    if let Some((vs, hs)) = sc.guides.filter(|(v, h)| !v.is_empty() || !h.is_empty()) {
        let (wl, wr) = (-pan_x / zoom, (w - pan_x) / zoom);
        let (wt, wb) = (-pan_y / zoom, (h - pan_y) / zoom);
        ctx.save();
        ctx.set_stroke_style("#ff3399");
        ctx.set_line_width(1.0 / zoom);
        ctx.set_line_dash(&[4.0 / zoom, 3.0 / zoom]);
        ctx.begin_path();
        for vx in vs {
            ctx.move_to(*vx, wt);
            ctx.line_to(*vx, wb);
        }
        for hy in hs {
            ctx.move_to(wl, *hy);
            ctx.line_to(wr, *hy);
        }
        ctx.stroke();
        ctx.set_line_dash(&[]);
        ctx.restore();
    }

    ctx.restore();

    if let Some((a, b)) = sc.lasso {
        ctx.save();
        ctx.set_stroke_style(SELECT_BLUE);
        ctx.set_line_width(1.0);
        ctx.set_line_dash(&[4.0, 3.0]);
        ctx.set_fill_style("rgba(26, 115, 232, 0.06)");
        let rx = a.x.min(b.x) * zoom + pan_x;
        let ry = a.y.min(b.y) * zoom + pan_y;
        let rw = (b.x - a.x).abs() * zoom;
        let rh = (b.y - a.y).abs() * zoom;
        ctx.fill_rect(rx, ry, rw, rh);
        ctx.stroke_rect(rx, ry, rw, rh);
        ctx.set_line_dash(&[]);
        ctx.restore();
    }

    ctx.restore();
}

// ── Minimap ──────────────────────────────────────────────────────────────────

/// The minimap's size (`MW`, `MH`).
pub const MINIMAP_W: f64 = 180.0;
pub const MINIMAP_H: f64 = 120.0;

/// Where the minimap maps the world (`transformRef`): world → minimap is `o + (w − min) × scale`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MinimapTransform {
    pub min_x: f64,
    pub min_y: f64,
    pub scale: f64,
    pub ox: f64,
    pub oy: f64,
}

impl MinimapTransform {
    /// A minimap point back to the world (`jump`).
    pub fn to_world(&self, mx: f64, my: f64) -> Point {
        Point::new((mx - self.ox) / self.scale + self.min_x, (my - self.oy) / self.scale + self.min_y)
    }
}

/// The minimap's transform for these shapes and this view (`vw`×`vh` the canvas size).
pub fn minimap_transform(shapes: &[Shape], zoom: f64, pan_x: f64, pan_y: f64, vw: f64, vh: f64) -> MinimapTransform {
    let (vx0, vy0) = (-pan_x / zoom, -pan_y / zoom);
    let (vx1, vy1) = ((vw - pan_x) / zoom, (vh - pan_y) / zoom);
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for s in shapes {
        min_x = min_x.min(s.x());
        min_y = min_y.min(s.y());
        max_x = max_x.max(s.x() + s.w());
        max_y = max_y.max(s.y() + s.h());
    }
    min_x = min_x.min(vx0);
    min_y = min_y.min(vy0);
    max_x = max_x.max(vx1);
    max_y = max_y.max(vy1);
    if !min_x.is_finite() {
        (min_x, min_y, max_x, max_y) = (0.0, 0.0, vw, vh);
    }
    let pad = 20.0;
    min_x -= pad;
    min_y -= pad;
    max_x += pad;
    max_y += pad;
    let cw = or_one(max_x - min_x);
    let ch = or_one(max_y - min_y);
    let scale = (MINIMAP_W / cw).min(MINIMAP_H / ch);
    MinimapTransform { min_x, min_y, scale, ox: (MINIMAP_W - cw * scale) / 2.0, oy: (MINIMAP_H - ch * scale) / 2.0 }
}

/// Paints the minimap in a 180×120 box at the context's origin.
pub fn render_minimap(ctx: &mut Ctx2D, shapes: &[Shape], zoom: f64, pan_x: f64, pan_y: f64, vw: f64, vh: f64) -> MinimapTransform {
    let t = minimap_transform(shapes, zoom, pan_x, pan_y, vw, vh);
    ctx.set_fill_style("#fafafa");
    ctx.fill_rect(0.0, 0.0, MINIMAP_W, MINIMAP_H);
    let tx = |wx: f64| t.ox + (wx - t.min_x) * t.scale;
    let ty = |wy: f64| t.oy + (wy - t.min_y) * t.scale;
    for s in shapes {
        let st = s.style();
        ctx.set_fill_style(if st.fill_color == "none" { "#e8eaed" } else { &st.fill_color });
        ctx.set_stroke_style(if st.stroke_color == "none" { "#9aa0a6" } else { &st.stroke_color });
        ctx.set_line_width(0.5);
        ctx.fill_rect(tx(s.x()), ty(s.y()), s.w() * t.scale, s.h() * t.scale);
        ctx.stroke_rect(tx(s.x()), ty(s.y()), s.w() * t.scale, s.h() * t.scale);
    }
    let (vx0, vy0) = (-pan_x / zoom, -pan_y / zoom);
    let (vx1, vy1) = ((vw - pan_x) / zoom, (vh - pan_y) / zoom);
    ctx.set_fill_style("rgba(26,115,232,0.10)");
    ctx.fill_rect(tx(vx0), ty(vy0), (vx1 - vx0) * t.scale, (vy1 - vy0) * t.scale);
    ctx.set_stroke_style(SELECT_BLUE);
    ctx.set_line_width(1.5);
    ctx.stroke_rect(tx(vx0), ty(vy0), (vx1 - vx0) * t.scale, (vy1 - vy0) * t.scale);
    t
}

// ── Rulers ───────────────────────────────────────────────────────────────────

/// `RULER_THICK`.
pub const RULER_THICK: f64 = 18.0;

/// Paints a ruler at the context's origin: horizontal (`length` × 18) or vertical (18 × `length`).
/// `pan` is the view's pan along the ruler minus the ruler's own offset, as the web passes it.
pub fn render_ruler(ctx: &mut Ctx2D, horizontal: bool, pan: f64, zoom: f64, length: f64) {
    if length <= 0.0 {
        return;
    }
    let (w, h) = if horizontal { (length, RULER_THICK) } else { (RULER_THICK, length) };
    ctx.set_fill_style("#f8f9fa");
    ctx.fill_rect(0.0, 0.0, w, h);
    ctx.set_stroke_style("#dadce0");
    ctx.set_line_width(1.0);
    ctx.begin_path();
    if horizontal {
        ctx.move_to(0.0, h - 0.5);
        ctx.line_to(w, h - 0.5);
    } else {
        ctx.move_to(w - 0.5, 0.0);
        ctx.line_to(w - 0.5, h);
    }
    ctx.stroke();
    let bases = [5.0, 10.0, 20.0, 25.0, 50.0, 100.0, 200.0, 250.0, 500.0, 1000.0, 2000.0, 5000.0];
    let step = bases.iter().copied().find(|b| b * zoom >= 70.0).unwrap_or(10000.0);
    ctx.set_fill_style("#80868b");
    ctx.set_font("8px Outfit, sans-serif");
    ctx.set_stroke_style("#bdc1c6");
    let span = if horizontal { w } else { h };
    let start_world = (-pan / (step * zoom)).floor() * step;
    let mut world = start_world;
    // Bounded: a ruler is at most a few hundred ticks long.
    for _ in 0..10_000 {
        let screen = pan + world * zoom;
        if screen > span {
            break;
        }
        if screen >= 0.0 {
            ctx.begin_path();
            if horizontal {
                ctx.move_to(screen + 0.5, RULER_THICK);
                ctx.line_to(screen + 0.5, RULER_THICK - 8.0);
            } else {
                ctx.move_to(RULER_THICK, screen + 0.5);
                ctx.line_to(RULER_THICK - 8.0, screen + 0.5);
            }
            ctx.stroke();
            let label = kubuno_office_shapes_core::path::js_num(world);
            if horizontal {
                ctx.fill_text(&label, screen + 2.0, 3.0);
            } else {
                ctx.save();
                ctx.translate(3.0, screen + 2.0);
                ctx.rotate(-std::f64::consts::FRAC_PI_2);
                ctx.set_text_align(crate::canvas::TextAlign::Right);
                ctx.fill_text(&label, 0.0, 6.0);
                ctx.restore();
            }
        }
        for k in 1..5 {
            let ms = pan + (world + step / 5.0 * k as f64) * zoom;
            if ms < 0.0 || ms > span {
                continue;
            }
            ctx.begin_path();
            if horizontal {
                ctx.move_to(ms + 0.5, RULER_THICK);
                ctx.line_to(ms + 0.5, RULER_THICK - 4.0);
            } else {
                ctx.move_to(RULER_THICK, ms + 0.5);
                ctx.line_to(RULER_THICK - 4.0, ms + 0.5);
            }
            ctx.stroke();
        }
        world += step;
    }
}

// ── Thumbnails and exports ───────────────────────────────────────────────────

/// `StencilThumbnail`: the stencil in a `w`×`h` box with a 12 % margin (at least 4), default style, no
/// label.
pub fn render_stencil_thumbnail(ctx: &mut Ctx2D, stencil: &stencils::StencilDef, w: f64, h: f64) {
    let margin = (w.min(h) * 0.12).round().max(4.0);
    let style = ShapeStyle::merge(&stencil.style);
    stencils::render_shape(ctx, stencil.id, margin, margin, w - margin * 2.0, h - margin * 2.0, &style, "", &LabelStyle::default(), None);
}

/// The world box of what an export draws (`renderExportCanvas`): shapes and displayed connector points,
/// `None` when the page is empty.
pub fn content_bounds(shapes: &[Shape], connectors: &[Connector]) -> Option<(Point, Point)> {
    let mut pts: Vec<Point> = Vec::new();
    for s in shapes {
        pts.push(Point::new(s.x(), s.y()));
        pts.push(Point::new(s.x() + s.w(), s.y() + s.h()));
    }
    for c in connectors {
        pts.extend(geometry::display_points(c, shapes));
    }
    let first = *pts.first()?;
    let (mut lo, mut hi) = (first, first);
    for p in &pts {
        lo.x = lo.x.min(p.x);
        lo.y = lo.y.min(p.y);
        hi.x = hi.x.max(p.x);
        hi.y = hi.y.max(p.y);
    }
    Some((lo, hi))
}

/// The export raster (`renderExportCanvas`): padding 24, scale 2. Returns the CSS size (before the
/// device scale) and the scene parameters to draw it with [`render_canvas`]: `(width, height, zoom,
/// pan_x, pan_y)`.
pub fn export_frame(shapes: &[Shape], connectors: &[Connector]) -> Option<(f64, f64, f64, f64, f64)> {
    let (lo, hi) = content_bounds(shapes, connectors)?;
    let (pad, scale) = (24.0, 2.0);
    let wc = (hi.x - lo.x) + 2.0 * pad;
    let hc = (hi.y - lo.y) + 2.0 * pad;
    Some((wc * scale, hc * scale, scale, (pad - lo.x) * scale, (pad - lo.y) * scale))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::{Op, Recorder};
    use crate::model::Obj;

    fn shape(id: &str, x: f64, y: f64) -> Shape {
        Shape::new(id, "rect", x, y, 100.0, 50.0, "Box", Obj::new(), 0.0, "default")
    }

    #[test]
    fn a_later_connector_hops_over_an_earlier_one() {
        let shapes = vec![shape("a", 0.0, 0.0), shape("b", 300.0, 0.0), shape("c", 150.0, -200.0), shape("d", 150.0, 200.0)];
        let conns = [Connector::between("1", "a", "b", "x"), Connector::between("2", "c", "d", "x")];
        let polys: Vec<Vec<Point>> = conns.iter().map(|c| display_points(c, &shapes)).collect();
        assert!(connector_hops(&polys, 0).iter().all(|h| h.is_empty()), "the first one never hops");
        assert_eq!(connector_hops(&polys, 1)[0].len(), 1, "the second crosses the first once");
    }

    #[test]
    fn the_scene_draws_background_grid_connectors_shapes_and_selection() {
        let shapes = vec![shape("a", 0.0, 0.0), shape("b", 300.0, 0.0)];
        let conns = vec![Connector::between("1", "a", "b", "x")];
        let sel = vec!["a".to_string()];
        let mut r = Recorder::default();
        let mut ctx = Ctx2D::new(&mut r);
        render_canvas(
            &mut ctx,
            &Scene {
                shapes: &shapes,
                connectors: &conns,
                zoom: 1.0,
                pan_x: 60.0,
                pan_y: 60.0,
                selected: &sel,
                selected_conns: &[],
                hovered: None,
                drawing_conn: None,
                lasso: None,
                bg_color: "#ffffff",
                magnet_segs: &[],
                show_grid: true,
                grid_size: 10.0,
                guides: None,
                drawing_shape: None,
                width: 800.0,
                height: 600.0,
            },
        );
        assert!(matches!(r.ops[0], Op::Fill { .. }), "the background first");
        assert!(r.ops.iter().any(|o| matches!(o, Op::Text { text, .. } if text == "Box")));
        // The selected shape's 8 handles are white squares.
        let handles = r.ops.iter().filter(|o| matches!(o, Op::Fill { color, .. } if *color == crate::color::Rgba::WHITE)).count();
        assert!(handles >= 8, "{handles}");
    }

    #[test]
    fn the_minimap_maps_back_to_the_world() {
        let shapes = vec![shape("a", 0.0, 0.0)];
        let t = minimap_transform(&shapes, 1.0, 60.0, 60.0, 800.0, 600.0);
        let p = t.to_world(t.ox + (50.0 - t.min_x) * t.scale, t.oy + (25.0 - t.min_y) * t.scale);
        assert!((p.x - 50.0).abs() < 1e-9 && (p.y - 25.0).abs() < 1e-9);
    }

    #[test]
    fn exports_frame_the_content() {
        let shapes = vec![shape("a", 10.0, 20.0)];
        let (w, h, zoom, px, py) = export_frame(&shapes, &[]).expect("content");
        assert_eq!((w, h, zoom), ((100.0 + 48.0) * 2.0, (50.0 + 48.0) * 2.0, 2.0));
        assert_eq!((px, py), ((24.0 - 10.0) * 2.0, (24.0 - 20.0) * 2.0));
        assert!(export_frame(&[], &[]).is_none());
    }
}
