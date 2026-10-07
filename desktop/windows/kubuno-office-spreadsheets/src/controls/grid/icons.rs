//! The conditional-format icons (`web/src/cfIcons.ts`, `drawCfIcon`): small vector glyphs at the left of a
//! cell — arrows, traffic lights, flags, ratings, quarters, signs, symbols — drawn with the framework's
//! `Graphics`. Returns the width they take, by which left-aligned text is pushed right.

use kubuno_desktop::ui::graphics::{Brush, Color, Graphics, GraphicsPath, LineCap, Pen, PointF};
use kubuno_desktop::ui::Rect;

const GREEN: &str = "#188038";
const AMBER: &str = "#f9ab00";
const RED: &str = "#d93025";
const GRAY: &str = "#5f6368";
const RAMP3: [&str; 3] = [RED, AMBER, GREEN];
const RAMP4: [&str; 4] = [RED, "#f56c00", AMBER, GREEN];
const RAMP5: [&str; 5] = [RED, "#f56c00", AMBER, "#7cb342", GREEN];
const RED_TO_BLACK: [&str; 4] = ["#202124", GRAY, "#e67c73", RED];

pub fn hex(c: &str) -> Color {
    Color::from_hex(c).unwrap_or(Color::BLACK)
}

fn ramp(n: usize, i: usize) -> &'static str {
    match n {
        5 => RAMP5[i.min(4)],
        4 => RAMP4[i.min(3)],
        _ => RAMP3[i.min(2)],
    }
}

fn arrow_angles(n: usize) -> Vec<f32> {
    use std::f32::consts::PI;
    match n {
        4 => vec![PI, PI * 0.75, PI * 0.25, 0.0],
        5 => vec![PI, PI * 0.75, PI * 0.5, PI * 0.25, 0.0],
        _ => vec![PI, PI * 0.5, 0.0],
    }
}

fn rotate(cx: f32, cy: f32, a: f32, x: f32, y: f32) -> PointF {
    let (s, c) = a.sin_cos();
    PointF::new(cx + x * c - y * s, cy + x * s + y * c)
}

fn draw_arrow(g: &Graphics<'_>, cx: f32, cy: f32, s: f32, angle: f32, color: &str) {
    let w = s * 0.32;
    let head = s * 0.48;
    let pts = [(0.0, -s / 2.0), (s * 0.44, -s / 2.0 + head), (w / 2.0, -s / 2.0 + head), (w / 2.0, s / 2.0), (-w / 2.0, s / 2.0), (-w / 2.0, -s / 2.0 + head), (-s * 0.44, -s / 2.0 + head)];
    let p: Vec<PointF> = pts.iter().map(|(x, y)| rotate(cx, cy, angle, *x, *y)).collect();
    g.fill_polygon(Brush::solid(hex(color)), &p);
}

fn draw_circle(g: &Graphics<'_>, cx: f32, cy: f32, s: f32, color: &str) {
    let r = Rect::new(cx - s / 2.0, cy - s / 2.0, cx + s / 2.0, cy + s / 2.0);
    g.fill_ellipse(Brush::solid(hex(color)), r);
    g.draw_ellipse(&Pen::new(Color::rgba_f(0.0, 0.0, 0.0, 0.25), 1.0), r);
}

fn draw_flag(g: &Graphics<'_>, cx: f32, cy: f32, s: f32, color: &str) {
    let x0 = cx - s * 0.32;
    let top = cy - s / 2.0;
    g.draw_line(&Pen::new(hex(GRAY), (s / 9.0).max(1.0)), PointF::new(x0, top), PointF::new(x0, cy + s / 2.0));
    g.fill_polygon(Brush::solid(hex(color)), &[PointF::new(x0, top), PointF::new(x0 + s * 0.68, top + s * 0.22), PointF::new(x0, top + s * 0.44)]);
}

fn draw_rating(g: &Graphics<'_>, cx: f32, cy: f32, s: f32, idx: usize, n: usize) {
    let gap = (s * 0.12).max(1.0);
    let bw = (s - gap * (n as f32 - 1.0)) / n as f32;
    for i in 0..n {
        let bh = s * (0.3 + 0.7 * i as f32 / (n as f32 - 1.0));
        let x = cx - s / 2.0 + i as f32 * (bw + gap);
        let color = if i <= idx { "#1a73e8" } else { "#dadce0" };
        g.fill_rectangle(Brush::solid(hex(color)), Rect::new(x, cy + s / 2.0 - bh, x + bw, cy + s / 2.0));
    }
}

fn draw_quarters(g: &Graphics<'_>, cx: f32, cy: f32, s: f32, idx: usize, n: usize) {
    let r = s / 2.0;
    let rect = Rect::new(cx - r, cy - r, cx + r, cy + r);
    g.fill_ellipse(Brush::solid(Color::WHITE), rect);
    let frac = if n > 1 { idx as f32 / (n as f32 - 1.0) } else { 1.0 };
    if frac > 0.0 {
        // Canvas angles: 0 = right, clockwise; the pie starts at the top (−90°).
        g.fill_pie(Brush::solid(hex(GRAY)), rect, -90.0, 360.0 * frac);
    }
    g.draw_ellipse(&Pen::new(hex(GRAY), 1.0), rect);
}

fn draw_sign(g: &Graphics<'_>, cx: f32, cy: f32, s: f32, idx: usize) {
    let color = RAMP3[idx.min(2)];
    match idx {
        0 => g.fill_polygon(Brush::solid(hex(color)), &[PointF::new(cx, cy - s / 2.0), PointF::new(cx + s / 2.0, cy), PointF::new(cx, cy + s / 2.0), PointF::new(cx - s / 2.0, cy)]),
        1 => g.fill_polygon(Brush::solid(hex(color)), &[PointF::new(cx, cy - s / 2.0), PointF::new(cx + s / 2.0, cy + s / 2.0), PointF::new(cx - s / 2.0, cy + s / 2.0)]),
        _ => draw_circle(g, cx, cy, s, color),
    }
}

fn draw_symbol(g: &Graphics<'_>, cx: f32, cy: f32, s: f32, idx: usize, circled: bool) {
    let color = RAMP3[idx.min(2)];
    let stroke = if circled { Color::WHITE } else { hex(color) };
    if circled {
        draw_circle(g, cx, cy, s, color);
    }
    let pen = Pen::new(stroke, (s / 7.0).max(1.2)).with_caps(LineCap::Round);
    let r = s * if circled { 0.24 } else { 0.36 };
    match idx {
        0 => {
            g.draw_line(&pen, PointF::new(cx - r, cy - r), PointF::new(cx + r, cy + r));
            g.draw_line(&pen, PointF::new(cx + r, cy - r), PointF::new(cx - r, cy + r));
        }
        1 => {
            g.draw_line(&pen, PointF::new(cx, cy - r), PointF::new(cx, cy + r * 0.35));
            let d = (s / 10.0).max(1.0);
            g.fill_ellipse(Brush::solid(stroke), Rect::new(cx - d, cy + r * 0.95 - d, cx + d, cy + r * 0.95 + d));
        }
        _ => {
            let mut path = GraphicsPath::new();
            path.move_to(PointF::new(cx - r, cy + r * 0.1));
            path.line_to(PointF::new(cx - r * 0.25, cy + r * 0.8));
            path.line_to(PointF::new(cx + r, cy - r * 0.7));
            g.draw_path(&pen, &path);
        }
    }
}

/// `drawCfIcon(ctx, icon, x, y, h, zoom)`: draws the icon at the left of the cell; returns its width + 6.
#[allow(clippy::too_many_arguments)] // the web's `drawCfIcon` signature
pub fn draw_cf_icon(g: &Graphics<'_>, set: &str, idx: usize, n: usize, x: f32, y: f32, h: f32, zoom: f32) -> f32 {
    let n = n.clamp(3, 5);
    let idx = idx.min(n - 1);
    let s = (12.0 * zoom).min(h - 4.0).max(8.0);
    let cx = x + 3.0 + s / 2.0;
    let cy = y + h / 2.0;
    let gray = set.contains("Gray");
    if set.contains("Arrows") {
        let angle = arrow_angles(n)[idx];
        let color = if gray {
            GRAY
        } else if angle == 0.0 {
            GREEN
        } else if (angle - std::f32::consts::PI).abs() < 1e-6 {
            RED
        } else {
            AMBER
        };
        draw_arrow(g, cx, cy, s, angle, color);
    } else if set.contains("Rating") {
        draw_rating(g, cx, cy, s, idx, n);
    } else if set.contains("Quarters") {
        draw_quarters(g, cx, cy, s, idx, n);
    } else if set.contains("Flags") {
        draw_flag(g, cx, cy, s, ramp(n, idx));
    } else if set == "3Signs" {
        draw_sign(g, cx, cy, s, idx);
    } else if set.starts_with("3Symbols") {
        draw_symbol(g, cx, cy, s, idx, set == "3Symbols");
    } else if set.contains("RedToBlack") {
        draw_circle(g, cx, cy, s, RED_TO_BLACK[idx.min(3)]);
    } else {
        draw_circle(g, cx, cy, s, ramp(n, idx));
    }
    s + 6.0
}
