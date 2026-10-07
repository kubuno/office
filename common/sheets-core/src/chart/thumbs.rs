//! The miniatures of the chart gallery and of the wizard's first step (`wizard/thumbs.tsx`), in a 64×44
//! box the painter scales.

use std::f64::consts::PI;

use super::model::{CHART2_PALETTE, CHART_NEUTRAL};
use super::scene::Prim;

pub const W: f64 = 64.0;
pub const H: f64 = 44.0;
const X0: f64 = 4.0;
const Y0: f64 = 3.0;
const YB: f64 = 40.0;
const XE: f64 = 61.0;

fn c(i: usize) -> String {
    CHART2_PALETTE[i].to_string()
}

fn rect(x: f64, y: f64, w: f64, h: f64, fill: String) -> Prim {
    Prim::Rect { x, y, w, h, rx: None, fill, fill_opacity: None, stroke: None, stroke_width: None, tip: None }
}

fn path(d: String, fill: String, opacity: Option<f64>) -> Prim {
    Prim::Path { d, fill, fill_opacity: opacity, evenodd: false, stroke: None, stroke_width: None, round: 0, tip: None }
}

fn stroke_path(d: String, fill: String, opacity: Option<f64>, stroke: String, width: f64) -> Prim {
    Prim::Path { d, fill, fill_opacity: opacity, evenodd: false, stroke: Some(stroke), stroke_width: Some(width), round: 0, tip: None }
}

fn poly_d(pts: &[(f64, f64)], close: bool) -> String {
    let mut d = String::new();
    for (i, (x, y)) in pts.iter().enumerate() {
        d.push_str(&format!("{}{x},{y} ", if i == 0 { "M" } else { "L" }));
    }
    if close {
        d.push('Z');
    }
    d.trim().to_string()
}

fn line(x1: f64, y1: f64, x2: f64, y2: f64, w: f64) -> Prim {
    Prim::Line { x1, y1, x2, y2, stroke: CHART_NEUTRAL.into(), stroke_opacity: None, stroke_width: Some(w) }
}

#[allow(clippy::too_many_arguments)] // the web helper's signature
fn slice(cx: f64, cy: f64, r: f64, a0: f64, a1: f64, hole: f64, dx: f64, dy: f64) -> String {
    let (x0, y0) = (cx + dx + r * a0.cos(), cy + dy + r * a0.sin());
    let (x1, y1) = (cx + dx + r * a1.cos(), cy + dy + r * a1.sin());
    let large = if a1 - a0 > PI { 1 } else { 0 };
    if hole <= 0.0 {
        return format!("M{},{} L{x0},{y0} A{r},{r} 0 {large} 1 {x1},{y1} Z", cx + dx, cy + dy);
    }
    let h = r * hole;
    let (hx0, hy0) = (cx + dx + h * a0.cos(), cy + dy + h * a0.sin());
    let (hx1, hy1) = (cx + dx + h * a1.cos(), cy + dy + h * a1.sin());
    format!("M{hx0},{hy0} L{x0},{y0} A{r},{r} 0 {large} 1 {x1},{y1} L{hx1},{hy1} A{h},{h} 0 {large} 0 {hx0},{hy0} Z")
}

fn columns(out: &mut Vec<Prim>, mode: &str) {
    let groups = [[14.0, 20.0, 9.0], [22.0, 12.0, 15.0], [10.0, 26.0, 12.0]];
    for (gi, g) in groups.iter().enumerate() {
        if mode == "normal" {
            for (si, v) in g.iter().enumerate() {
                out.push(rect(7.0 + gi as f64 * 19.0 + si as f64 * 5.0, YB - v, 4.0, *v, c(si)));
            }
        } else {
            let total: f64 = g.iter().sum();
            let scale = if mode == "percent" { (YB - Y0 - 2.0) / total } else { 1.0 };
            let mut y = YB;
            for (si, v) in g.iter().enumerate() {
                let h = v * scale;
                y -= h;
                out.push(rect(10.0 + gi as f64 * 18.0, y, 9.0, h, c(si)));
            }
        }
    }
}

fn bars(out: &mut Vec<Prim>, mode: &str) {
    let groups = [[26.0, 14.0, 20.0], [16.0, 30.0, 10.0], [34.0, 20.0, 14.0]];
    for (gi, g) in groups.iter().enumerate() {
        if mode == "normal" {
            for (si, v) in g.iter().enumerate() {
                out.push(rect(X0 + 1.0, 5.0 + gi as f64 * 13.0 + si as f64 * 4.0, *v, 3.0, c(si)));
            }
        } else {
            let total: f64 = g.iter().sum();
            let scale = if mode == "percent" { 54.0 / total } else { 0.8 };
            let mut x = X0 + 1.0;
            for (si, v) in g.iter().enumerate() {
                let w = v * scale;
                out.push(rect(x, 7.0 + gi as f64 * 12.0, w, 8.0, c(si)));
                x += w;
            }
        }
    }
}

fn pie(out: &mut Vec<Prim>, hole: f64, exploded: bool) {
    let (cx, cy, r) = (32.0, 22.0, 17.0);
    let angles = [-PI / 2.0, 0.4, 1.9, 3.4, PI * 1.5];
    for i in 0..4 {
        let mid = (angles[i] + angles[i + 1]) / 2.0;
        let off = if exploded && i == 1 {
            4.0
        } else if exploded {
            1.5
        } else {
            0.0
        };
        out.push(path(slice(cx, cy, r - if exploded { 2.0 } else { 0.0 }, angles[i], angles[i + 1], hole, off * mid.cos(), off * mid.sin()), c(i), None));
    }
}

const LINE_PTS: [[(f64, f64); 5]; 2] = [[(6.0, 30.0), (19.0, 20.0), (32.0, 26.0), (45.0, 12.0), (58.0, 16.0)], [(6.0, 36.0), (19.0, 33.0), (32.0, 28.0), (45.0, 31.0), (58.0, 24.0)]];

fn lines(out: &mut Vec<Prim>, ln: bool, symbols: bool) {
    for (si, pts) in LINE_PTS.iter().enumerate() {
        if ln {
            out.push(stroke_path(poly_d(pts, false), "none".into(), None, c(si), 2.0));
        }
        if symbols {
            for p in pts {
                out.push(rect(p.0 - 2.0, p.1 - 2.0, 4.0, 4.0, c(si)));
            }
        }
    }
}

fn areas(out: &mut Vec<Prim>, mode: &str) {
    let p = |pts: &[(f64, f64)]| {
        let mut v = pts.to_vec();
        v.push((XE, YB));
        v.push((X0, YB));
        poly_d(&v, true)
    };
    match mode {
        "percent" => {
            out.push(rect(X0, Y0 + 2.0, XE - X0, YB - Y0 - 2.0, c(2)));
            out.push(path(p(&[(X0, 28.0), (20.0, 20.0), (38.0, 25.0), (XE, 16.0)]), c(1), None));
            out.push(path(p(&[(X0, 34.0), (20.0, 30.0), (38.0, 33.0), (XE, 28.0)]), c(0), None));
        }
        "stacked" => {
            out.push(path(p(&[(X0, 18.0), (20.0, 8.0), (38.0, 14.0), (XE, 6.0)]), c(2), None));
            out.push(path(p(&[(X0, 27.0), (20.0, 20.0), (38.0, 25.0), (XE, 18.0)]), c(1), None));
            out.push(path(p(&[(X0, 34.0), (20.0, 30.0), (38.0, 33.0), (XE, 28.0)]), c(0), None));
        }
        _ => {
            out.push(path(p(&[(X0, 20.0), (22.0, 10.0), (40.0, 18.0), (XE, 8.0)]), c(1), Some(0.85)));
            out.push(path(p(&[(X0, 30.0), (22.0, 24.0), (40.0, 28.0), (XE, 20.0)]), c(0), Some(0.9)));
        }
    }
}

const SCATTER_PTS: [(f64, f64, usize); 11] = [(10.0, 30.0, 0), (16.0, 24.0, 1), (22.0, 28.0, 0), (27.0, 18.0, 1), (33.0, 22.0, 0), (38.0, 12.0, 1), (44.0, 17.0, 0), (50.0, 9.0, 1), (56.0, 14.0, 0), (13.0, 36.0, 1), (47.0, 26.0, 0)];

fn circle(cx: f64, cy: f64, r: f64, fill: String) -> Prim {
    Prim::Circle { cx, cy, r, fill, stroke: None, stroke_opacity: None, tip: None }
}

fn radar(out: &mut Vec<Prim>, filled: bool) {
    let (cx, cy, spokes) = (32.0, 23.0, 5);
    let pt = |r: f64, i: usize| {
        let a = -PI / 2.0 + i as f64 * 2.0 * PI / spokes as f64;
        (cx + r * a.cos(), cy + r * a.sin())
    };
    for ring in 1..=2 {
        let r = ring as f64 * 9.5;
        let pts: Vec<(f64, f64)> = (0..spokes).map(|i| pt(r, i)).collect();
        out.push(stroke_path(poly_d(&pts, true), "none".into(), None, CHART_NEUTRAL.into(), 0.8));
    }
    for i in 0..spokes {
        let p = pt(19.0, i);
        out.push(line(cx, cy, p.0, p.1, 0.8));
    }
    let s1: Vec<(f64, f64)> = [16.0, 10.0, 17.0, 13.0, 18.0].iter().enumerate().map(|(i, r)| pt(*r, i)).collect();
    let s2: Vec<(f64, f64)> = [9.0, 16.0, 8.0, 17.0, 10.0].iter().enumerate().map(|(i, r)| pt(*r, i)).collect();
    for (k, s) in [s1, s2].iter().enumerate() {
        out.push(stroke_path(poly_d(s, true), if filled { c(k) } else { "none".into() }, if filled { Some(0.75) } else { None }, c(k), 1.5));
    }
}

fn combo(out: &mut Vec<Prim>, stacked: bool) {
    let groups = [[16.0, 10.0], [24.0, 13.0], [12.0, 18.0], [28.0, 9.0]];
    for (gi, g) in groups.iter().enumerate() {
        let gx = gi as f64 * 14.0;
        if stacked {
            out.push(rect(8.0 + gx, YB - g[0] - g[1], 8.0, g[1], c(1)));
            out.push(rect(8.0 + gx, YB - g[0], 8.0, g[0], c(0)));
        } else {
            out.push(rect(7.0 + gx, YB - g[0], 5.0, g[0], c(0)));
            out.push(rect(13.0 + gx, YB - g[1], 5.0, g[1], c(1)));
        }
    }
    let pts = [(11.0, 18.0), (25.0, 10.0), (39.0, 22.0), (53.0, 8.0)];
    out.push(stroke_path(poly_d(&pts, false), "none".into(), None, c(2), 2.0));
    for p in pts {
        out.push(circle(p.0, p.1, 2.0, c(2)));
    }
}

/// `ChartThumb`: the miniature of a (type, subtype) pair in the 64×44 box.
pub fn chart_thumb(type_id: &str, subtype_id: &str) -> Vec<Prim> {
    let mut out = Vec::new();
    if type_id != "pie" && type_id != "radar" {
        out.push(line(X0, Y0, X0, YB, 1.0));
        out.push(line(X0, YB, XE, YB, 1.0));
    }
    match type_id {
        "column" => columns(&mut out, subtype_id),
        "bar" => bars(&mut out, subtype_id),
        "pie" => pie(&mut out, if subtype_id.starts_with("donut") { 0.55 } else { 0.0 }, subtype_id.ends_with("exploded")),
        "area" => areas(&mut out, subtype_id),
        "line" => lines(&mut out, subtype_id != "points", subtype_id != "lines"),
        "scatter" => {
            if subtype_id == "lines" {
                lines(&mut out, true, false);
            } else {
                for p in SCATTER_PTS {
                    out.push(circle(p.0, p.1, 2.0, c(p.2)));
                }
            }
        }
        "radar" => radar(&mut out, subtype_id == "filled"),
        _ => combo(&mut out, subtype_id == "col_stacked_line"),
    }
    out
}
