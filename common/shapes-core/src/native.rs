//! The suite's NATIVE drawing shapes — the handful drawn from fraction adjustments rather than from the
//! preset data — a port of the web's `shapes/native-geometry.ts`.
//!
//! Coordinate space: the shape's own box, `(0, 0)` to `(w, h)`. Callers pass `inset = strokeWidth / 2` so
//! an outline stays inside the box.

use crate::adjust::adj_values;
use crate::path::{round2, Cmd, Path, Rect};

/// The native kinds, in gallery order.
pub const NATIVE_KINDS: [&str; 10] = ["rect", "roundRect", "ellipse", "triangle", "diamond", "arrow", "line", "star", "callout", "plus"];

/// True when this module draws the kind itself (fraction adjustments).
pub fn has_native_geometry(kind: &str) -> bool {
    NATIVE_KINDS.contains(&kind)
}

/// `asNative`: a kind not drawn here falls back to the rectangle.
pub fn as_native(kind: &str) -> &str {
    if has_native_geometry(kind) {
        kind
    } else {
        "rect"
    }
}

const ROUND_FRAC: f64 = 0.14;
const ARROW_SHAFT: f64 = 0.5;
const ARROW_HEAD: f64 = 0.5;
const STAR_INNER: f64 = 0.382;
const STAR_SPIKES: usize = 5;
const CALLOUT_BODY: f64 = 0.78;
const CALLOUT_TAIL: [f64; 3] = [0.22, 0.3, 0.42];
const PLUS_ARM: f64 = 0.36;
const TEXT_PAD: f64 = 3.0;

/// A path, whether it is closed, and where a caption fits.
#[derive(Debug, Clone, PartialEq)]
pub struct Geometry {
    pub path: Path,
    pub closed: bool,
    pub text: Rect,
}

/// Corner radius of a rounded rectangle of that size, proportional to the shorter side.
pub fn corner_radius(w: f64, h: f64, frac: f64) -> f64 {
    let m = w.min(h).max(0.0);
    (m * frac).min(m / 2.0).max(0.0)
}

fn pad_rect(r: Rect, pad: f64) -> Rect {
    let dx = pad.min(r.w / 2.0);
    let dy = pad.min(r.h / 2.0);
    Rect { x: r.x + dx, y: r.y + dy, w: (r.w - 2.0 * dx).max(0.0), h: (r.h - 2.0 * dy).max(0.0) }
}

/// Path and caption box of one native shape (`shapeGeometry`).
pub fn shape_geometry(kind: &str, w: f64, h: f64, inset: f64, adj: Option<&[f64]>) -> Geometry {
    let kind = as_native(kind);
    let av = adj_values(kind, adj);
    let width = w.max(0.0);
    let height = h.max(0.0);
    let p = inset.min((width.min(height) - 1.0) / 2.0).max(0.0);
    let (x0, y0) = (p, p);
    let x1 = (width - p).max(p);
    let y1 = (height - p).max(p);
    let (iw, ih) = (x1 - x0, y1 - y0);
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let full = Rect { x: x0, y: y0, w: iw, h: ih };
    let frac = |fx: f64, fy: f64, fw: f64, fh: f64| Rect { x: x0 + iw * fx, y: y0 + ih * fy, w: iw * fw, h: ih * fh };
    let a = |i: usize, d: f64| av.get(i).copied().unwrap_or(d);
    let r2 = round2;
    let mut d = Path::new();
    let mut text = full;
    let mut closed = true;
    let arc = |d: &mut Path, rx: f64, ry: f64, x: f64, y: f64| d.cmds.push(Cmd::ArcTo { rx: r2(rx), ry: r2(ry), large: false, sweep: true, x: r2(x), y: r2(y) });
    let mv = |d: &mut Path, x: f64, y: f64| d.cmds.push(Cmd::MoveTo(r2(x), r2(y)));
    let ln = |d: &mut Path, x: f64, y: f64| d.cmds.push(Cmd::LineTo(r2(x), r2(y)));

    match kind {
        "roundRect" => {
            let r = corner_radius(iw, ih, a(0, ROUND_FRAC));
            mv(&mut d, x0 + r, y0);
            ln(&mut d, x1 - r, y0);
            arc(&mut d, r, r, x1, y0 + r);
            ln(&mut d, x1, y1 - r);
            arc(&mut d, r, r, x1 - r, y1);
            ln(&mut d, x0 + r, y1);
            arc(&mut d, r, r, x0, y1 - r);
            ln(&mut d, x0, y0 + r);
            arc(&mut d, r, r, x0 + r, y0);
            d.close();
            text = pad_rect(full, r * 0.25);
        }
        "ellipse" => {
            let (rx, ry) = (iw / 2.0, ih / 2.0);
            mv(&mut d, x0, cy);
            arc(&mut d, rx, ry, x1, cy);
            arc(&mut d, rx, ry, x0, cy);
            d.close();
            text = frac(0.1464, 0.1464, 0.7072, 0.7072);
        }
        "triangle" => {
            mv(&mut d, cx, y0);
            ln(&mut d, x1, y1);
            ln(&mut d, x0, y1);
            d.close();
            text = frac(0.28, 0.46, 0.44, 0.54);
        }
        "diamond" => {
            mv(&mut d, cx, y0);
            ln(&mut d, x1, cy);
            ln(&mut d, cx, y1);
            ln(&mut d, x0, cy);
            d.close();
            text = frac(0.25, 0.25, 0.5, 0.5);
        }
        "arrow" => {
            let head = iw.min(iw.min(ih) * a(1, ARROW_HEAD));
            let shaft = ih * a(0, ARROW_SHAFT);
            let hx = x1 - head;
            let (sy0, sy1) = (cy - shaft / 2.0, cy + shaft / 2.0);
            mv(&mut d, x0, sy0);
            ln(&mut d, hx, sy0);
            ln(&mut d, hx, y0);
            ln(&mut d, x1, cy);
            ln(&mut d, hx, y1);
            ln(&mut d, hx, sy1);
            ln(&mut d, x0, sy1);
            d.close();
            text = Rect { x: x0, y: sy0, w: (hx - x0).max(0.0), h: (sy1 - sy0).max(0.0) };
        }
        "line" => {
            mv(&mut d, x0, y1);
            ln(&mut d, x1, y0);
            closed = false;
        }
        "star" => {
            let (rx, ry) = (iw / 2.0, ih / 2.0);
            for i in 0..STAR_SPIKES * 2 {
                let r = if i % 2 == 0 { 1.0 } else { a(0, STAR_INNER) };
                let ang = (std::f64::consts::PI / STAR_SPIKES as f64) * i as f64 - std::f64::consts::FRAC_PI_2;
                let (x, y) = (cx + rx * r * ang.cos(), cy + ry * r * ang.sin());
                if i == 0 {
                    mv(&mut d, x, y);
                } else {
                    ln(&mut d, x, y);
                }
            }
            d.close();
            text = frac(0.24, 0.32, 0.52, 0.4);
        }
        "callout" => {
            let by = y0 + ih * a(0, CALLOUT_BODY);
            let tip_f = a(1, CALLOUT_TAIL[1]);
            let half = (CALLOUT_TAIL[2] - CALLOUT_TAIL[0]) / 2.0;
            let (tl, tip, tr) = ((tip_f - half).max(0.0), tip_f, (tip_f + half).min(1.0));
            mv(&mut d, x0, y0);
            ln(&mut d, x1, y0);
            ln(&mut d, x1, by);
            ln(&mut d, x0 + iw * tr, by);
            ln(&mut d, x0 + iw * tip, y1);
            ln(&mut d, x0 + iw * tl, by);
            ln(&mut d, x0, by);
            d.close();
            text = Rect { x: x0, y: y0, w: iw, h: (by - y0).max(0.0) };
        }
        "plus" => {
            let arm = iw.min(ih) * a(0, PLUS_ARM);
            let (ax0, ax1) = (cx - arm / 2.0, cx + arm / 2.0);
            let (ay0, ay1) = (cy - arm / 2.0, cy + arm / 2.0);
            mv(&mut d, ax0, y0);
            ln(&mut d, ax1, y0);
            ln(&mut d, ax1, ay0);
            ln(&mut d, x1, ay0);
            ln(&mut d, x1, ay1);
            ln(&mut d, ax1, ay1);
            ln(&mut d, ax1, y1);
            ln(&mut d, ax0, y1);
            ln(&mut d, ax0, ay1);
            ln(&mut d, x0, ay1);
            ln(&mut d, x0, ay0);
            ln(&mut d, ax0, ay0);
            d.close();
            text = Rect { x: x0, y: ay0, w: iw, h: (ay1 - ay0).max(0.0) };
        }
        _ => {
            // rect (`M x0,y0 H x1 V y1 H x0 Z`, written with explicit lines).
            mv(&mut d, x0, y0);
            ln(&mut d, x1, y0);
            ln(&mut d, x1, y1);
            ln(&mut d, x0, y1);
            d.close();
        }
    }
    Geometry { path: d, closed, text: pad_rect(text, TEXT_PAD) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rectangle_stays_inside_its_stroke() {
        let g = shape_geometry("rect", 100.0, 50.0, 2.0, None);
        assert_eq!(g.path.to_svg(), "M 2,2 L 98,2 L 98,48 L 2,48 Z");
        assert_eq!(g.text, Rect { x: 5.0, y: 5.0, w: 90.0, h: 40.0 });
    }

    #[test]
    fn the_rounded_corner_follows_its_fraction() {
        let g = shape_geometry("roundRect", 200.0, 100.0, 0.0, Some(&[0.25]));
        assert!(g.path.to_svg().starts_with("M 25,0 L 175,0 A 25,25 0 0 1 200,25"), "{}", g.path.to_svg());
    }

    #[test]
    fn a_star_has_ten_vertices_and_the_line_is_open() {
        let g = shape_geometry("star", 100.0, 100.0, 0.0, None);
        assert_eq!(g.path.cmds.len(), 11);
        assert!(!shape_geometry("line", 10.0, 10.0, 0.0, None).closed);
        assert_eq!(as_native("cloud"), "rect");
    }
}
