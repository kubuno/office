//! Axis measurement and drawing (`web/src/sheet-chart/render/axes.tsx`): value ticks, category labels with
//! chart2's anti-overlap cascade (flat → stagger → 45° → 90° → rhythm), gridlines and the zero line.

use super::data::ChartData;
use super::layout::{approx_measure, Rect};
use super::marks::{category_labels, format_axis_tick, point_count, truncate_to_width};
use super::model::*;
use super::scale::{ticks_of, AxisScale};
use super::scene::{Anchor, Prim};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XLabelMode {
    Flat,
    Stagger,
    Rot45,
    Rot90,
}

#[derive(Clone, Debug)]
pub struct AxesResult {
    pub inner: Rect,
    pub x_band: f64,
    pub y_band: f64,
    pub x_mode: XLabelMode,
    pub x_rhythm: usize,
    pub y_rhythm: usize,
    pub x_labels: Vec<String>,
    pub y_labels: Vec<String>,
}

const FS: f64 = FONT_AXIS_LABEL;
const TICK_GAP: f64 = 6.0;
const TOP_PAD: f64 = 5.0;
const END_PAD: f64 = 4.0;
const MAX_CAT_PX: f64 = 72.0;
pub const INK: &str = "var(--color-text-secondary, #5f6368)";

fn max_w(arr: &[String]) -> f64 {
    arr.iter().fold(0.0f64, |w, t| w.max(approx_measure(t, FS).0))
}

fn rhythm(need: f64, per: f64) -> usize {
    ((need / per.max(1.0)).ceil()).max(1.0) as usize
}

/// `measureAxes`.
pub fn measure_axes(plot: Rect, spec: &ChartSpec, data: &ChartData, y: &AxisScale, x: Option<&AxisScale>, lang: &str) -> AxesResult {
    let swapped = spec.kind == ChartKind::Bar;
    let percent = spec.stacking == Stacking::Percent;
    let n = point_count(data);
    let cats = category_labels(data, n);
    let lh = approx_measure("Ag", FS).1;
    let val_labels: Vec<String> = ticks_of(y).into_iter().map(|v| format_axis_tick(v, y.step, percent, spec.axis_y.format.as_deref(), lang)).collect();
    if swapped {
        let mut y_band = max_w(&cats).min(plot.w * 0.35) + TICK_GAP;
        if plot.w - y_band < 40.0 {
            y_band = 2.0;
        }
        let mut x_band = lh + 4.0;
        if plot.h - x_band < 24.0 {
            x_band = 2.0;
        }
        let inner = Rect { x: plot.x + y_band, y: plot.y + TOP_PAD, w: (plot.w - y_band - END_PAD).max(0.0), h: (plot.h - TOP_PAD - x_band).max(0.0) };
        let per_tick = inner.w / (val_labels.len() as f64 - 1.0).max(1.0);
        let band_h = inner.h / n as f64;
        return AxesResult {
            inner,
            x_band,
            y_band,
            x_mode: XLabelMode::Flat,
            x_rhythm: rhythm(max_w(&val_labels) + 8.0, per_tick),
            y_rhythm: rhythm(lh + 2.0, band_h),
            x_labels: if x_band > 2.0 { val_labels.clone() } else { Vec::new() },
            y_labels: if y_band > 2.0 { cats.iter().map(|c| truncate_to_width(c, y_band - TICK_GAP, FS)).collect() } else { Vec::new() },
        };
    }
    let mut y_band = max_w(&val_labels).min(plot.w * 0.4) + TICK_GAP;
    if plot.w - y_band < 48.0 {
        y_band = 2.0;
    }
    let avail_w = (plot.w - y_band - END_PAD).max(0.0);
    let mut x_mode = XLabelMode::Flat;
    let mut x_rhythm = 1;
    let mut x_labels: Vec<String>;
    let mut x_band = lh + 4.0;
    if let Some(xs) = x {
        x_labels = ticks_of(xs).into_iter().map(|v| format_axis_tick(v, xs.step, false, spec.axis_x.format.as_deref(), lang)).collect();
        let per = avail_w / (x_labels.len() as f64 - 1.0).max(1.0);
        x_rhythm = rhythm(max_w(&x_labels) + 8.0, per);
    } else {
        let band = avail_w / n as f64;
        let raw_w = max_w(&cats);
        if raw_w <= band - 4.0 {
            x_labels = cats.iter().map(|c| truncate_to_width(c, (band - 4.0).max(12.0), FS)).collect();
        } else if n > 1 && raw_w <= 2.0 * band - 8.0 {
            x_mode = XLabelMode::Stagger;
            x_band = 2.0 * lh + 4.0;
            x_labels = cats.iter().map(|c| truncate_to_width(c, 2.0 * band - 8.0, FS)).collect();
        } else {
            let max_cat = MAX_CAT_PX.min((plot.h * 0.4).max(24.0));
            if band >= lh * 1.5 {
                x_mode = XLabelMode::Rot45;
                #[allow(clippy::approx_constant)] // the web's literal, kept for identical geometry
                {
                    x_band = (raw_w.min(max_cat) + lh) * 0.7071 + 4.0;
                }
            } else {
                x_mode = XLabelMode::Rot90;
                x_band = raw_w.min(max_cat) + 6.0;
                x_rhythm = rhythm(lh + 2.0, band);
            }
            x_labels = cats.iter().map(|c| truncate_to_width(c, max_cat, FS)).collect();
        }
    }
    if plot.h - x_band < 28.0 {
        x_band = 2.0;
        x_labels = Vec::new();
    }
    let inner = Rect { x: plot.x + y_band, y: plot.y + TOP_PAD, w: avail_w, h: (plot.h - TOP_PAD - x_band).max(0.0) };
    AxesResult { inner, x_band, y_band, x_mode, x_rhythm, y_rhythm: 1, x_labels, y_labels: if y_band > 2.0 { val_labels } else { Vec::new() } }
}

fn grid_line(x1: f64, x2: f64, y1: f64, y2: f64) -> Prim {
    Prim::Line { x1, y1, x2, y2, stroke: CHART_NEUTRAL.into(), stroke_opacity: Some(0.45), stroke_width: None }
}

fn plain_line(x1: f64, x2: f64, y1: f64, y2: f64, stroke: &str) -> Prim {
    Prim::Line { x1, y1, x2, y2, stroke: stroke.into(), stroke_opacity: None, stroke_width: None }
}

fn text(x: f64, y: f64, s: &str, anchor: Anchor, rotate: Option<f64>) -> Prim {
    Prim::Text { x, y, text: s.into(), size: FS, bold: false, anchor, fill: INK.into(), rotate }
}

/// `<Axes>`.
#[allow(clippy::too_many_arguments)] // the web component's props
pub fn axes(out: &mut Vec<Prim>, inner: Rect, spec: &ChartSpec, data: &ChartData, y_scale: &AxisScale, x_scale: Option<&AxisScale>, swapped: bool, a: &AxesResult) {
    let y_span = if y_scale.max - y_scale.min != 0.0 { y_scale.max - y_scale.min } else { 1.0 };
    let v_pos = |v: f64| if swapped { inner.x + ((v - y_scale.min) / y_span) * inner.w } else { inner.y + (1.0 - (v - y_scale.min) / y_span) * inner.h };
    let x_span = x_scale.map(|x| if x.max - x.min != 0.0 { x.max - x.min } else { 1.0 }).unwrap_or(1.0);
    let x_min = x_scale.map(|x| x.min).unwrap_or(0.0);
    let x_pos = |v: f64| inner.x + ((v - x_min) / x_span) * inner.w;
    let y_ticks = ticks_of(y_scale);
    let n = point_count(data);
    let band = (if swapped { inner.h } else { inner.w }) / n as f64;
    let bottom = inner.y + inner.h;
    let right = inner.x + inner.w;
    let lh = FS * 1.3;
    if spec.grid_y {
        for t in &y_ticks {
            let p = v_pos(*t);
            out.push(if swapped { grid_line(p, p, inner.y, bottom) } else { grid_line(inner.x, right, p, p) });
        }
    }
    if spec.grid_x {
        if let Some(xs) = x_scale {
            for t in ticks_of(xs) {
                let p = x_pos(t);
                out.push(grid_line(p, p, inner.y, bottom));
            }
        } else {
            for i in 0..=n {
                let p = (if swapped { inner.y } else { inner.x }) + i as f64 * band;
                out.push(if swapped { grid_line(inner.x, right, p, p) } else { grid_line(p, p, inner.y, bottom) });
            }
        }
    }
    out.push(plain_line(inner.x, inner.x, inner.y, bottom, CHART_NEUTRAL));
    out.push(plain_line(inner.x, right, bottom, bottom, CHART_NEUTRAL));
    if y_scale.min < 0.0 && y_scale.max > 0.0 {
        let p = v_pos(0.0);
        let ink = "var(--color-text-tertiary, #80868b)";
        out.push(if swapped { plain_line(p, p, inner.y, bottom, ink) } else { plain_line(inner.x, right, p, p, ink) });
    }
    for t in &y_ticks {
        let p = v_pos(*t);
        out.push(if swapped { plain_line(p, p, bottom, bottom + 3.0, CHART_NEUTRAL) } else { plain_line(inner.x - 3.0, inner.x, p, p, CHART_NEUTRAL) });
    }
    if swapped {
        for (i, label) in a.y_labels.iter().enumerate() {
            if i % a.y_rhythm != 0 || label.is_empty() {
                continue;
            }
            let cy = inner.y + ((n - 1 - i) as f64 + 0.5) * band + FS * 0.35;
            out.push(text(inner.x - TICK_GAP + 2.0, cy, label, Anchor::End, None));
        }
        for (i, label) in a.x_labels.iter().enumerate() {
            if i % a.x_rhythm != 0 {
                continue;
            }
            out.push(text(v_pos(y_ticks.get(i).copied().unwrap_or(y_scale.min)), bottom + lh * 0.75, label, Anchor::Middle, None));
        }
        return;
    }
    for (i, label) in a.y_labels.iter().enumerate() {
        if i % a.y_rhythm != 0 {
            continue;
        }
        out.push(text(inner.x - TICK_GAP + 2.0, v_pos(y_ticks.get(i).copied().unwrap_or(y_scale.min)) + FS * 0.35, label, Anchor::End, None));
    }
    let x_ticks = x_scale.map(ticks_of);
    for (i, label) in a.x_labels.iter().enumerate() {
        if i % a.x_rhythm != 0 || label.is_empty() {
            continue;
        }
        let cx = match &x_ticks {
            Some(t) => x_pos(t.get(i).copied().unwrap_or(0.0)),
            None => inner.x + (i as f64 + 0.5) * band,
        };
        match a.x_mode {
            XLabelMode::Stagger => out.push(text(cx, bottom + lh * if i % 2 == 0 { 0.75 } else { 1.75 }, label, Anchor::Middle, None)),
            XLabelMode::Rot45 => out.push(text(cx, bottom + lh * 0.7, label, Anchor::End, Some(-45.0))),
            XLabelMode::Rot90 => out.push(text(cx + FS * 0.35, bottom + 4.0, label, Anchor::End, Some(-90.0))),
            XLabelMode::Flat => out.push(text(cx, bottom + lh * 0.75, label, Anchor::Middle, None)),
        }
    }
}
