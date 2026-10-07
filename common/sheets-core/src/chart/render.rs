//! The plot renderers (`web/src/sheet-chart/render/{cartesian,circular,radar,combo,legend}.tsx`) and the
//! root `ChartView`, as [`Prim`] lists.

use std::f64::consts::PI;

use super::axes::{axes, measure_axes};
use super::data::{ChartData, MarkKind, ResolvedSeries};
use super::layout::{approx_measure, compute_layout, ChartLayout, LegendLayout, Rect};
use super::marks::*;
use super::model::*;
use super::scale::*;
use super::scene::{f2, Anchor, Prim};

const TEXT_PRIMARY: &str = "var(--color-text-primary, #202124)";
const SURFACE: &str = "var(--color-surface-0, #ffffff)";

fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    v.max(lo).min(hi)
}

fn label(x: f64, y: f64, s: String, anchor: Anchor, fill: &str) -> Prim {
    Prim::Text { x, y, text: s, size: FONT_DATA_LABEL, bold: false, anchor, fill: fill.into(), rotate: None }
}

fn data_label(spec: &ChartSpec, p: &StackedPoint, lang: &str) -> String {
    match spec.data_labels {
        Some(DataLabels::Percent) => format_share(p.share, lang),
        _ => format_data_value(p.v.unwrap_or(0.0), lang),
    }
}

fn is_bar(k: MarkKind) -> bool {
    matches!(k, MarkKind::Column | MarkKind::Bar)
}

// ── Cartesian ───────────────────────────────────────────────────────────────

fn scatter_x_scale(spec: &ChartSpec, data: &ChartData, width: f64) -> AxisScale {
    let n_ticks = clamp((width / 56.0).floor(), 2.0, 10.0);
    let (lo, hi) = compute_x_extent(data);
    let auto = auto_scale_linear(lo, hi, n_ticks, AutoScaleOptions { expand_close_to_border: true, ..Default::default() });
    let min = spec.axis_x.min.unwrap_or(auto.min);
    let max = spec.axis_x.max.unwrap_or(auto.max);
    if min == auto.min && max == auto.max {
        return auto;
    }
    let span = auto_scale_linear(min, max, n_ticks, AutoScaleOptions::default());
    AxisScale { min, max, step: span.step, minor_count: auto.minor_count }
}

fn tip(name: &str, cat: &str, p: &StackedPoint, percent: bool, lang: &str) -> String {
    let val = p.v.map(|v| format_data_value(v, lang)).unwrap_or_default();
    let pct = if percent { format!(" ({})", format_share(p.share, lang)) } else { String::new() };
    format!("{name}\n{cat}: {val}{pct}")
}

fn line_prim(d: String, color: &str, width: f64) -> Prim {
    Prim::Path { d, fill: "none".into(), fill_opacity: None, evenodd: false, stroke: Some(color.into()), stroke_width: Some(width), round: 2, tip: None }
}

fn hover_circle(x: f64, y: f64, r: f64, tip: String) -> Prim {
    Prim::Circle { cx: x, cy: y, r, fill: "transparent".into(), stroke: None, stroke_opacity: None, tip: Some(tip) }
}

fn cartesian(out: &mut Vec<Prim>, spec: &ChartSpec, data: &ChartData, layout: &ChartLayout, lang: &str) {
    let plot = layout.plot;
    let swapped = spec.kind == ChartKind::Bar;
    let is_scatter = spec.kind == ChartKind::Scatter;
    if plot.w < 24.0 || plot.h < 20.0 {
        return;
    }
    let n = point_count(data);
    let cats = category_labels(data, n);
    let percent = spec.stacking == Stacking::Percent;
    let value_len = if swapped { plot.w } else { plot.h };
    let y_scale = compute_value_scale(spec, data, clamp((value_len / 26.0).floor(), 2.0, 10.0));
    let x_scale = is_scatter.then(|| scatter_x_scale(spec, data, plot.w));
    let ax = measure_axes(plot, spec, data, &y_scale, x_scale.as_ref(), lang);
    let inner = ax.inner;
    if inner.w < 4.0 || inner.h < 4.0 {
        return;
    }
    let y_span = if y_scale.max - y_scale.min != 0.0 { y_scale.max - y_scale.min } else { 1.0 };
    let v_pos = |v: f64| if swapped { inner.x + ((v - y_scale.min) / y_span) * inner.w } else { inner.y + (1.0 - (v - y_scale.min) / y_span) * inner.h };
    let (x_min, x_span) = match &x_scale {
        Some(x) => (x.min, if x.max - x.min != 0.0 { x.max - x.min } else { 1.0 }),
        None => (0.0, 1.0),
    };
    let x_pos = |v: f64| inner.x + ((v - x_min) / x_span) * inner.w;
    let band = (if swapped { inner.h } else { inner.w }) / n as f64;
    let cat_slot = |i: usize| if swapped { (n - 1 - i) as f64 } else { i as f64 };
    let cat_center = |i: usize| (if swapped { inner.y } else { inner.x }) + (cat_slot(i) + 0.5) * band;
    let refs: Vec<&ResolvedSeries> = data.series.iter().collect();
    let stacks = compute_stacks(&refs, n, spec.stacking);
    let n_bars = data.series.iter().filter(|s| is_bar(s.kind)).count();
    let grouped = spec.stacking == Stacking::None;
    let bar_w = (if grouped { band / (n_bars.max(1) as f64 + BAR_GAP_RATIO) } else { band / (1.0 + BAR_GAP_RATIO) }).max(0.5);
    let bar_start = |bi: usize, i: usize| {
        let s0 = (if swapped { inner.y } else { inner.x }) + cat_slot(i) * band;
        if grouped {
            s0 + (band - n_bars as f64 * bar_w) / 2.0 + bi as f64 * bar_w
        } else {
            s0 + (band - bar_w) / 2.0
        }
    };
    let point_color = |s: &ResolvedSeries, i: usize| -> String {
        if data.series.len() == 1 {
            if let Some(c) = data.point_colors.as_ref().and_then(|p| p.get(i)) {
                return c.clone();
            }
        }
        s.color.clone()
    };
    let (mut area_els, mut bar_els, mut line_els, mut pt_els, mut label_els) = (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut bar_idx = 0;
    for (si, s) in data.series.iter().enumerate() {
        let st = &stacks[si];
        match s.kind {
            MarkKind::Column | MarkKind::Bar => {
                let bi = bar_idx;
                bar_idx += 1;
                for (i, p) in st.iter().enumerate() {
                    if p.v.is_none() {
                        continue;
                    }
                    let (a, b) = (v_pos(p.base), v_pos(p.top));
                    let off = bar_start(bi, i);
                    let (x, y, w, h) = if swapped { (a.min(b), off, (b - a).abs(), bar_w) } else { (off, a.min(b), bar_w, (b - a).abs()) };
                    bar_els.push(Prim::Rect {
                        x,
                        y,
                        w,
                        h,
                        rx: None,
                        fill: point_color(s, i),
                        fill_opacity: None,
                        stroke: Some(if grouped { "none".into() } else { SURFACE.into() }),
                        stroke_width: Some(if grouped { 0.0 } else { 1.0 }),
                        tip: Some(tip(&s.name, &cats[i], p, percent, lang)),
                    });
                }
            }
            MarkKind::Area => {
                let mut top = Vec::new();
                let mut bot = Vec::new();
                for (i, p) in st.iter().enumerate() {
                    if p.v.is_none() {
                        top.push(None);
                        bot.push(None);
                        continue;
                    }
                    top.push(Some(Pt { x: cat_center(i), y: v_pos(p.top) }));
                    bot.push(Some(Pt { x: cat_center(i), y: v_pos(p.base) }));
                }
                area_els.push(Prim::Path {
                    d: area_path(&top, &bot, spec.smooth),
                    fill: s.color.clone(),
                    fill_opacity: Some(if spec.stacking == Stacking::None && data.series.len() > 1 { 0.7 } else { 1.0 }),
                    evenodd: false,
                    stroke: None,
                    stroke_width: None,
                    round: 0,
                    tip: Some(s.name.clone()),
                });
            }
            MarkKind::Scatter => {
                let pts: Vec<Option<Pt>> = s
                    .values
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        let xv = match &s.x {
                            Some(x) => x.get(i).copied().flatten(),
                            None => Some(i as f64 + 1.0),
                        };
                        match (v, xv) {
                            (Some(v), Some(xv)) => Some(Pt { x: x_pos(xv), y: v_pos(*v) }),
                            _ => None,
                        }
                    })
                    .collect();
                if spec.lines {
                    line_els.push(line_prim(line_path(&pts, spec.smooth), &s.color, 2.0));
                }
                for (i, pt) in pts.iter().enumerate() {
                    let Some(pt) = pt else { continue };
                    let cat = match &s.x {
                        Some(x) => format!("({})", format_data_value(x.get(i).copied().flatten().unwrap_or(0.0), lang)),
                        None => cats.get(i).cloned().unwrap_or_default(),
                    };
                    let sp = st.get(i).copied().unwrap_or(StackedPoint { v: None, base: 0.0, top: 0.0, share: 0.0 });
                    pt_els.push(marker(symbol_for(si), pt.x, pt.y, 3.8, &point_color(s, i), Some(tip(&s.name, &cat, &sp, percent, lang))));
                }
            }
            _ => {
                let pts: Vec<Option<Pt>> = st.iter().enumerate().map(|(i, p)| p.v.map(|_| Pt { x: cat_center(i), y: v_pos(p.top) })).collect();
                if spec.lines || spec.kind == ChartKind::Combo {
                    line_els.push(line_prim(line_path(&pts, spec.smooth), &s.color, 2.0));
                }
                for (i, pt) in pts.iter().enumerate() {
                    let Some(pt) = pt else { continue };
                    let t = tip(&s.name, &cats[i], &st[i], percent, lang);
                    if spec.symbols || !spec.lines {
                        pt_els.push(marker(symbol_for(si), pt.x, pt.y, 3.8, &point_color(s, i), Some(t)));
                    } else {
                        pt_els.push(hover_circle(pt.x, pt.y, 8.0, t));
                    }
                }
            }
        }
        // Data labels.
        if spec.data_labels.is_none() {
            continue;
        }
        let bar_series = is_bar(s.kind);
        if bar_series && bar_w < 14.0 {
            continue;
        }
        let rh = if bar_series { 1 } else { ((30.0 / band).ceil()).max(1.0) as usize };
        let fs = FONT_DATA_LABEL;
        for (i, p) in st.iter().enumerate() {
            let Some(v) = p.v else { continue };
            if i % rh != 0 {
                continue;
            }
            let text = data_label(spec, p, lang);
            if bar_series {
                let (a, b) = (v_pos(p.base), v_pos(p.top));
                let off = bar_start(bar_slot_index(data, si), i) + bar_w / 2.0;
                if !grouped {
                    if (b - a).abs() < 11.0 {
                        continue;
                    }
                    let c = (a + b) / 2.0;
                    let ink = ink_for(&point_color(s, i));
                    label_els.push(if swapped { label(c, off + fs * 0.35, text, Anchor::Middle, &ink) } else { label(off, c + fs * 0.35, text, Anchor::Middle, &ink) });
                } else {
                    let end = v_pos(p.top);
                    label_els.push(if swapped {
                        label(end + if v >= 0.0 { 3.0 } else { -3.0 }, off + fs * 0.35, text, if v >= 0.0 { Anchor::Start } else { Anchor::End }, TEXT_PRIMARY)
                    } else {
                        label(off, end + if v >= 0.0 { -3.0 } else { fs + 1.0 }, text, Anchor::Middle, TEXT_PRIMARY)
                    });
                }
            } else {
                let cx = if s.kind == MarkKind::Scatter {
                    match &s.x {
                        None => Some(x_pos(i as f64 + 1.0)),
                        Some(x) => x.get(i).copied().flatten().map(x_pos),
                    }
                } else {
                    Some(cat_center(i))
                };
                let Some(cx) = cx else { continue };
                label_els.push(label(cx, v_pos(p.top) - 6.0, text, Anchor::Middle, TEXT_PRIMARY));
            }
        }
    }
    axes(out, inner, spec, data, &y_scale, x_scale.as_ref(), swapped, &ax);
    let pad = 4.0;
    out.push(Prim::Clip { x: inner.x - pad, y: inner.y - pad, w: inner.w + 2.0 * pad, h: inner.h + 2.0 * pad });
    out.extend(area_els);
    out.extend(bar_els);
    out.extend(line_els);
    out.extend(pt_els);
    out.push(Prim::Unclip);
    out.extend(label_els);
}

fn bar_slot_index(data: &ChartData, si: usize) -> usize {
    data.series[..si].iter().filter(|s| is_bar(s.kind)).count()
}

// ── Legend ──────────────────────────────────────────────────────────────────

fn legend(out: &mut Vec<Prim>, lg: &LegendLayout, data: &ChartData) {
    let (r, s, fs) = (lg.rect, lg.symbol_size, lg.font_size);
    let gap = (fs * 0.3).max(4.0);
    for (i, e) in lg.entries.iter().enumerate() {
        let kind = data.series.get(i).map(|s| s.kind);
        let (sx, sy) = (r.x + e.x, r.y + e.y);
        let (cx, cy) = (sx + s / 2.0, sy + s / 2.0);
        let text_x = sx + s + gap;
        let lbl = truncate_to_width(&e.label, (r.x + r.w - text_x - 4.0).max(12.0), fs);
        match kind {
            Some(MarkKind::Line) => {
                out.push(Prim::Line { x1: sx - 1.0, y1: cy, x2: sx + s + 1.0, y2: cy, stroke: e.color.clone(), stroke_opacity: None, stroke_width: Some(2.0) });
                out.push(marker(symbol_for(i), cx, cy, s * 0.32, &e.color, None));
            }
            Some(MarkKind::Scatter) => out.push(marker(symbol_for(i), cx, cy, s * 0.42, &e.color, None)),
            _ => out.push(Prim::Rect { x: sx, y: sy, w: s, h: s, rx: Some(2.0), fill: e.color.clone(), fill_opacity: None, stroke: None, stroke_width: None, tip: None }),
        }
        out.push(Prim::Text { x: text_x, y: cy + fs * 0.35, text: lbl, size: fs, bold: false, anchor: Anchor::Default, fill: super::axes::INK.into(), rotate: None });
    }
}

// ── Circular ────────────────────────────────────────────────────────────────

const TWO_PI: f64 = PI * 2.0;
const OUTSIDE_MIN_FRAC: f64 = 0.05;

fn pt_x(cx: f64, r: f64, a: f64) -> f64 {
    cx + r * a.sin()
}

fn pt_y(cy: f64, r: f64, a: f64) -> f64 {
    cy - r * a.cos()
}

/// `sliceColor`.
pub fn slice_color(point_colors: Option<&Vec<String>>, i: usize) -> String {
    point_colors.and_then(|p| p.get(i)).cloned().unwrap_or_else(|| CHART2_PALETTE[i % CHART2_PALETTE.len()].to_string())
}

/// `pieLegendData`.
pub fn pie_legend_data(data: &ChartData) -> ChartData {
    let n = point_count(data);
    let cats = category_labels(data, n);
    ChartData {
        series: cats
            .into_iter()
            .enumerate()
            .map(|(i, name)| ResolvedSeries {
                name,
                color: slice_color(data.point_colors.as_ref(), i),
                kind: MarkKind::Pie,
                secondary: false,
                values: vec![data.series.first().and_then(|s| s.values.get(i).copied().flatten())],
                x: None,
            })
            .collect(),
        categories: Vec::new(),
        point_colors: None,
    }
}

struct Slice {
    i: usize,
    v: f64,
    frac: f64,
    a0: f64,
    a1: f64,
}

fn compute_slices(values: &[Option<f64>], n: usize) -> Vec<Slice> {
    let clamped: Vec<f64> = (0..n).map(|i| values.get(i).copied().flatten().filter(|v| v.is_finite()).map(|v| v.max(0.0)).unwrap_or(0.0)).collect();
    let total: f64 = clamped.iter().sum();
    if total <= 0.0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut a = 0.0;
    for (i, c) in clamped.iter().enumerate() {
        if *c <= 0.0 {
            continue;
        }
        let frac = c / total;
        out.push(Slice { i, v: *c, frac, a0: a, a1: a + frac * TWO_PI });
        a += frac * TWO_PI;
    }
    out
}

fn n2(v: f64) -> String {
    super::scene::num(f2(v))
}

fn sector_path(cx: f64, cy: f64, r0: f64, r1: f64, a0: f64, a1: f64) -> String {
    if a1 - a0 >= TWO_PI - 1e-4 {
        let ring = |r: f64, sweep: u8| format!("M {} {} A {} {} 0 1 {sweep} {} {} A {} {} 0 1 {sweep} {} {} Z", n2(cx), n2(cy - r), n2(r), n2(r), n2(cx), n2(cy + r), n2(r), n2(r), n2(cx), n2(cy - r));
        return if r0 > 0.5 { format!("{} {}", ring(r1, 1), ring(r0, 0)) } else { ring(r1, 1) };
    }
    let large = if a1 - a0 > PI { 1 } else { 0 };
    let o0 = format!("{} {}", n2(pt_x(cx, r1, a0)), n2(pt_y(cy, r1, a0)));
    let o1 = format!("{} {}", n2(pt_x(cx, r1, a1)), n2(pt_y(cy, r1, a1)));
    if r0 <= 0.5 {
        return format!("M {} {} L {o0} A {} {} 0 {large} 1 {o1} Z", n2(cx), n2(cy), n2(r1), n2(r1));
    }
    let i0 = format!("{} {}", n2(pt_x(cx, r0, a0)), n2(pt_y(cy, r0, a0)));
    let i1 = format!("{} {}", n2(pt_x(cx, r0, a1)), n2(pt_y(cy, r0, a1)));
    format!("M {o0} A {} {} 0 {large} 1 {o1} L {i1} A {} {} 0 {large} 0 {i0} Z", n2(r1), n2(r1), n2(r0), n2(r0))
}

fn circular(out: &mut Vec<Prim>, spec: &ChartSpec, data: &ChartData, layout: &ChartLayout, lang: &str) {
    let plot = layout.plot;
    if plot.w < 32.0 || plot.h < 32.0 {
        return;
    }
    let n = point_count(data);
    let cats = category_labels(data, n);
    let rings: Vec<&ResolvedSeries> = if spec.hole_size > 0.0 { data.series.iter().collect() } else { data.series.iter().take(1).collect() };
    let k_n = rings.len().max(1);
    let per_ring: Vec<Vec<Slice>> = rings.iter().map(|s| compute_slices(&s.values, n)).collect();
    if !per_ring.iter().any(|s| !s.is_empty()) {
        return;
    }
    let fs = FONT_DATA_LABEL;
    let (cx, cy) = (plot.x + plot.w / 2.0, plot.y + plot.h / 2.0);
    let mut max_r = (plot.w.min(plot.h) / 2.0 - 2.0).max(4.0);
    let label_text = |sl: &Slice| if spec.data_labels == Some(DataLabels::Percent) { format_share(sl.frac, lang) } else { format_data_value(sl.v, lang) };
    let single = k_n == 1;
    let want_labels = spec.data_labels.is_some() && single;
    let mut out_set = std::collections::HashSet::new();
    if want_labels {
        let hole = spec.hole_size;
        let r_l = max_r * (hole + (1.0 - hole) * 0.66);
        let mut max_out_w = 0.0f64;
        for sl in &per_ring[0] {
            let room = 2.0 * r_l * ((sl.a1 - sl.a0) / 2.0).min(PI / 2.0).sin();
            let w = approx_measure(&label_text(sl), fs).0;
            if sl.frac < OUTSIDE_MIN_FRAC || w > room {
                out_set.insert(sl.i);
                max_out_w = max_out_w.max(w);
            }
        }
        if !out_set.is_empty() {
            let reserve = (max_out_w + 16.0).min(plot.w * 0.3);
            max_r = ((plot.w / 2.0 - reserve).min(plot.h / 2.0 - fs * 1.3 - 4.0) - 2.0).max(4.0);
        }
    }
    let explode = if single { spec.explode } else { 0.0 };
    let r = if explode > 0.0 { max_r / (1.0 + explode) } else { max_r };
    let off_r = explode * r;
    let hole = spec.hole_size * r;
    let mut els = Vec::new();
    let mut labels = Vec::new();
    for (k, slices) in per_ring.iter().enumerate() {
        let s = rings[k];
        let r0 = hole + (r - hole) * (k as f64 / k_n as f64);
        let r1 = hole + (r - hole) * ((k + 1) as f64 / k_n as f64);
        for sl in slices {
            let mid = (sl.a0 + sl.a1) / 2.0;
            let (dx, dy) = if explode > 0.0 { (off_r * mid.sin(), -off_r * mid.cos()) } else { (0.0, 0.0) };
            let fill = slice_color(data.point_colors.as_ref(), sl.i);
            let tip_txt = format!(
                "{}{}: {} ({})",
                if k_n > 1 { format!("{}\n", s.name) } else { String::new() },
                cats[sl.i],
                format_data_value(sl.v, lang),
                format_share(sl.frac, lang)
            );
            els.push(Prim::Path { d: sector_path(cx + dx, cy + dy, r0, r1, sl.a0, sl.a1), fill: fill.clone(), fill_opacity: None, evenodd: true, stroke: Some(SURFACE.into()), stroke_width: Some(1.0), round: 1, tip: Some(tip_txt) });
            if spec.data_labels.is_some() && !single && r1 - r0 >= fs + 4.0 && sl.frac >= 0.04 {
                let rl = (r0 + r1) / 2.0;
                labels.push(label(f2(pt_x(cx, rl, mid)), f2(pt_y(cy, rl, mid) + fs * 0.35), label_text(sl), Anchor::Middle, &ink_for(&fill)));
            }
        }
    }
    if want_labels {
        let (mut last_right, mut last_left) = (f64::NEG_INFINITY, f64::INFINITY);
        for sl in &per_ring[0] {
            let mid = (sl.a0 + sl.a1) / 2.0;
            let (dx, dy) = if explode > 0.0 { (off_r * mid.sin(), -off_r * mid.cos()) } else { (0.0, 0.0) };
            let txt = label_text(sl);
            if !out_set.contains(&sl.i) {
                let rl = hole + (r - hole) * 0.66;
                labels.push(label(f2(pt_x(cx + dx, rl, mid)), f2(pt_y(cy + dy, rl, mid) + fs * 0.35), txt, Anchor::Middle, &ink_for(&slice_color(data.point_colors.as_ref(), sl.i))));
            } else {
                let right = mid.sin() >= 0.0;
                let ay = pt_y(cy + dy, r, mid);
                if right {
                    if ay - last_right < fs * 1.15 {
                        continue;
                    }
                    last_right = ay;
                } else {
                    if last_left - ay < fs * 1.15 {
                        continue;
                    }
                    last_left = ay;
                }
                let ax = pt_x(cx + dx, r, mid);
                let ex = pt_x(cx + dx, r + 7.0, mid);
                let ey = pt_y(cy + dy, r + 7.0, mid);
                let tx = ex + if right { 6.0 } else { -6.0 };
                els.push(Prim::Polyline { points: vec![(f2(ax), f2(ay)), (f2(ex), f2(ey)), (f2(tx), f2(ey))], stroke: CHART_NEUTRAL.into() });
                labels.push(label(f2(tx + if right { 3.0 } else { -3.0 }), f2(ey + fs * 0.35), txt, if right { Anchor::Start } else { Anchor::End }, TEXT_PRIMARY));
            }
        }
    }
    out.extend(els);
    out.extend(labels);
}

// ── Radar ───────────────────────────────────────────────────────────────────

fn radar(out: &mut Vec<Prim>, spec: &ChartSpec, data: &ChartData, layout: &ChartLayout, lang: &str) {
    let plot = layout.plot;
    if plot.w < 48.0 || plot.h < 48.0 {
        return;
    }
    let n = point_count(data);
    let percent = spec.stacking == Stacking::Percent;
    let cats_raw = category_labels(data, n);
    let lfs = FONT_AXIS_LABEL;
    let max_cat_w = (plot.w * 0.22).min(80.0);
    let cats: Vec<String> = cats_raw.iter().map(|c| truncate_to_width(c, max_cat_w, lfs)).collect();
    let cat_w = cats.iter().fold(0.0f64, |w, c| w.max(approx_measure(c, lfs).0));
    let (cx, cy) = (plot.x + plot.w / 2.0, plot.y + plot.h / 2.0);
    let r = (plot.w / 2.0 - cat_w - 10.0).min(plot.h / 2.0 - lfs * 1.3 - 6.0).max(12.0);
    let scale = compute_value_scale(spec, data, clamp((r / 28.0).floor(), 2.0, 6.0));
    let span = if scale.max - scale.min != 0.0 { scale.max - scale.min } else { 1.0 };
    let r_of = |v: f64| clamp((v - scale.min) / span, 0.0, 1.0) * r;
    let ang = |i: usize| (i as f64 * TWO_PI) / (n.max(1) as f64);
    let px = |rr: f64, a: f64| cx + rr * a.sin();
    let py = |rr: f64, a: f64| cy - rr * a.cos();
    let ink = super::axes::INK;
    let mut grid = Vec::new();
    let mut texts = Vec::new();
    let ticks = ticks_of(&scale);
    for t in &ticks {
        let rr = r_of(*t);
        if rr < 1.0 {
            continue;
        }
        if n >= 3 {
            let pts = (0..n).map(|i| (f2(px(rr, ang(i))), f2(py(rr, ang(i))))).collect();
            grid.push(Prim::Polygon { points: pts, fill: "none".into(), fill_opacity: None, stroke: Some(CHART_NEUTRAL.into()), stroke_opacity: Some(0.55) });
        } else {
            grid.push(Prim::Circle { cx: f2(cx), cy: f2(cy), r: f2(rr), fill: "none".into(), stroke: Some(CHART_NEUTRAL.into()), stroke_opacity: Some(0.55), tip: None });
        }
    }
    for i in 0..n {
        grid.push(Prim::Line { x1: f2(cx), y1: f2(cy), x2: f2(px(r, ang(i))), y2: f2(py(r, ang(i))), stroke: CHART_NEUTRAL.into(), stroke_opacity: Some(0.55), stroke_width: None });
    }
    for (ti, t) in ticks.iter().enumerate() {
        let rr = r_of(*t);
        if ti > 0 && rr < lfs {
            continue;
        }
        texts.push(Prim::Text { x: f2(cx + 4.0), y: f2(cy - rr + lfs * 0.35), text: format_axis_tick(*t, scale.step, percent, spec.axis_y.format.as_deref(), lang), size: lfs, bold: false, anchor: Anchor::Default, fill: ink.into(), rotate: None });
    }
    for (i, c) in cats.iter().enumerate() {
        if c.is_empty() {
            continue;
        }
        let a = ang(i);
        let (sn, co) = (a.sin(), a.cos());
        let anchor = if sn.abs() < 0.35 {
            Anchor::Middle
        } else if sn > 0.0 {
            Anchor::Start
        } else {
            Anchor::End
        };
        let dy = lfs * (0.35 - 0.5 * co);
        texts.push(Prim::Text { x: f2(px(r + 6.0, a)), y: f2(py(r + 6.0, a) + dy), text: c.clone(), size: lfs, bold: false, anchor, fill: ink.into(), rotate: None });
    }
    let refs: Vec<&ResolvedSeries> = data.series.iter().collect();
    let stacks = compute_stacks(&refs, n, spec.stacking);
    let show_symbols = spec.symbols || (!spec.lines && !spec.filled);
    let (mut fills, mut lines, mut pts_els, mut labels) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for (si, s) in data.series.iter().enumerate() {
        let st = &stacks[si];
        let pts: Vec<Option<Pt>> = (0..n).map(|i| st[i].v.map(|_| Pt { x: px(r_of(st[i].top), ang(i)), y: py(r_of(st[i].top), ang(i)) })).collect();
        let solid: Vec<Pt> = pts.iter().flatten().copied().collect();
        if spec.filled && solid.len() >= 3 {
            fills.push(Prim::Polygon { points: solid.iter().map(|p| (f2(p.x), f2(p.y))).collect(), fill: s.color.clone(), fill_opacity: Some(0.4), stroke: Some("none".into()), stroke_opacity: None });
        }
        if spec.lines || spec.filled {
            let mut closed = pts.clone();
            if n >= 2 {
                closed.push(pts[0]);
            }
            lines.push(line_prim(line_path(&closed, false), &s.color, if spec.filled { 1.5 } else { 2.0 }));
        }
        for (i, pt) in pts.iter().enumerate() {
            let Some(pt) = pt else { continue };
            let p = &st[i];
            let val = p.v.map(|v| format_data_value(v, lang)).unwrap_or_default();
            let t = format!("{}\n{}: {val}{}", s.name, cats_raw[i], if percent { format!(" ({})", format_share(p.share, lang)) } else { String::new() });
            if show_symbols {
                pts_els.push(marker(symbol_for(si), pt.x, pt.y, 3.5, &s.color, Some(t)));
            } else {
                pts_els.push(hover_circle(f2(pt.x), f2(pt.y), 7.0, t));
            }
            if spec.data_labels.is_some() && p.v.is_some() {
                let a = ang(i);
                labels.push(label(f2(pt.x + 8.0 * a.sin()), f2(pt.y - 8.0 * a.cos() + FONT_DATA_LABEL * 0.35), data_label(spec, p, lang), Anchor::Middle, TEXT_PRIMARY));
            }
        }
    }
    out.extend(grid);
    out.extend(fills);
    out.extend(lines);
    out.extend(pts_els);
    out.extend(texts);
    out.extend(labels);
}

// ── Combo with a secondary axis ─────────────────────────────────────────────

/// `hasSecondaryAxis`.
pub fn has_secondary_axis(data: &ChartData) -> bool {
    data.series.iter().any(|s| s.secondary)
}

fn combo(out: &mut Vec<Prim>, spec: &ChartSpec, data: &ChartData, layout: &ChartLayout, lang: &str) {
    let plot = layout.plot;
    if plot.w < 40.0 || plot.h < 20.0 {
        return;
    }
    let n = point_count(data);
    let cats = category_labels(data, n);
    let percent = spec.stacking == Stacking::Percent;
    let grouped = spec.stacking == Stacking::None;
    let prim: Vec<&ResolvedSeries> = data.series.iter().filter(|s| !s.secondary).collect();
    let sec: Vec<&ResolvedSeries> = data.series.iter().filter(|s| s.secondary).collect();
    let n_ticks = clamp((plot.h / 26.0).floor(), 2.0, 10.0);
    let sub = |list: &[&ResolvedSeries]| ChartData { series: list.iter().map(|s| (*s).clone()).collect(), ..data.clone() };
    let y_scale = compute_value_scale(spec, &if prim.is_empty() { data.clone() } else { sub(&prim) }, n_ticks);
    let spec2 = ChartSpec { axis_y: AxisSpec { format: spec.axis_y.format.clone(), ..Default::default() }, ..spec.clone() };
    let y2_scale = if sec.is_empty() { y_scale } else { compute_value_scale(&spec2, &sub(&sec), n_ticks) };
    let y2_labels: Vec<String> = ticks_of(&y2_scale).into_iter().map(|v| format_axis_tick(v, y2_scale.step, percent, spec.axis_y.format.as_deref(), lang)).collect();
    let band2 = y2_labels.iter().fold(0.0f64, |w, t| w.max(approx_measure(t, FONT_AXIS_LABEL).0)).min(plot.w * 0.25) + 8.0;
    let plot_main = Rect { w: (plot.w - band2).max(0.0), ..plot };
    let ax = measure_axes(plot_main, spec, data, &y_scale, None, lang);
    let inner = ax.inner;
    if inner.w < 4.0 || inner.h < 4.0 {
        return;
    }
    let span1 = if y_scale.max - y_scale.min != 0.0 { y_scale.max - y_scale.min } else { 1.0 };
    let span2 = if y2_scale.max - y2_scale.min != 0.0 { y2_scale.max - y2_scale.min } else { 1.0 };
    let v1 = |v: f64| inner.y + (1.0 - (v - y_scale.min) / span1) * inner.h;
    let v2 = |v: f64| inner.y + (1.0 - (v - y2_scale.min) / span2) * inner.h;
    let band = inner.w / n as f64;
    let cat_center = |i: usize| inner.x + (i as f64 + 0.5) * band;
    let sp = compute_stacks(&prim, n, spec.stacking);
    let ss = compute_stacks(&sec, n, spec.stacking);
    let mut stacks: Vec<Vec<StackedPoint>> = Vec::new();
    let (mut pi, mut sj) = (0, 0);
    for s in &data.series {
        if s.secondary {
            stacks.push(ss[sj].clone());
            sj += 1;
        } else {
            stacks.push(sp[pi].clone());
            pi += 1;
        }
    }
    let col: Vec<&ResolvedSeries> = data.series.iter().filter(|s| is_bar(s.kind)).collect();
    let prim_cols = col.iter().any(|s| !s.secondary);
    let sec_cols = col.iter().any(|s| s.secondary);
    let n_slots = (if grouped { col.len() } else { prim_cols as usize + sec_cols as usize }).max(1);
    let bar_w = (band / (n_slots as f64 + BAR_GAP_RATIO)).max(0.5);
    let bar_x = |slot: usize, i: usize| inner.x + i as f64 * band + (band - n_slots as f64 * bar_w) / 2.0 + slot as f64 * bar_w;
    let (mut bars, mut lines, mut pts, mut labels) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let fs = FONT_DATA_LABEL;
    let mut col_idx = 0;
    for (si, s) in data.series.iter().enumerate() {
        let st = &stacks[si];
        let v_pos = |v: f64| if s.secondary { v2(v) } else { v1(v) };
        if is_bar(s.kind) {
            let slot = if grouped {
                col_idx += 1;
                col_idx - 1
            } else if s.secondary {
                prim_cols as usize
            } else {
                0
            };
            for (i, p) in st.iter().enumerate() {
                let Some(v) = p.v else { continue };
                let (a, b) = (v_pos(p.base), v_pos(p.top));
                bars.push(Prim::Rect {
                    x: bar_x(slot, i),
                    y: a.min(b),
                    w: bar_w,
                    h: (b - a).abs(),
                    rx: None,
                    fill: s.color.clone(),
                    fill_opacity: None,
                    stroke: Some(if grouped { "none".into() } else { SURFACE.into() }),
                    stroke_width: Some(if grouped { 0.0 } else { 1.0 }),
                    tip: Some(tip(&s.name, &cats[i], p, percent, lang)),
                });
                if spec.data_labels.is_some() && bar_w >= 14.0 {
                    let text = data_label(spec, p, lang);
                    let cxm = bar_x(slot, i) + bar_w / 2.0;
                    if grouped {
                        labels.push(label(cxm, b + if v >= 0.0 { -3.0 } else { fs + 1.0 }, text, Anchor::Middle, TEXT_PRIMARY));
                    } else if (b - a).abs() >= 11.0 {
                        labels.push(label(cxm, (a + b) / 2.0 + fs * 0.35, text, Anchor::Middle, &ink_for(&s.color)));
                    }
                }
            }
        } else {
            let p_list: Vec<Option<Pt>> = st.iter().enumerate().map(|(i, p)| p.v.map(|_| Pt { x: cat_center(i), y: v_pos(p.top) })).collect();
            lines.push(line_prim(line_path(&p_list, spec.smooth), &s.color, 2.0));
            let rh = ((30.0 / band).ceil()).max(1.0) as usize;
            for (i, pt) in p_list.iter().enumerate() {
                let Some(pt) = pt else { continue };
                let p = &st[i];
                if spec.symbols {
                    pts.push(marker(symbol_for(si), pt.x, pt.y, 3.8, &s.color, Some(tip(&s.name, &cats[i], p, percent, lang))));
                } else {
                    pts.push(hover_circle(pt.x, pt.y, 8.0, tip(&s.name, &cats[i], p, percent, lang)));
                }
                if spec.data_labels.is_some() && p.v.is_some() && i % rh == 0 {
                    labels.push(label(pt.x, pt.y - 6.0, data_label(spec, p, lang), Anchor::Middle, TEXT_PRIMARY));
                }
            }
        }
    }
    let mut axis2 = Vec::new();
    if !sec.is_empty() {
        let rx = inner.x + inner.w;
        let bottom = inner.y + inner.h;
        axis2.push(Prim::Line { x1: rx, y1: inner.y, x2: rx, y2: bottom, stroke: CHART_NEUTRAL.into(), stroke_opacity: None, stroke_width: None });
        for (i, t) in ticks_of(&y2_scale).into_iter().enumerate() {
            let y = v2(t);
            axis2.push(Prim::Line { x1: rx, y1: y, x2: rx + 3.0, y2: y, stroke: CHART_NEUTRAL.into(), stroke_opacity: None, stroke_width: None });
            axis2.push(Prim::Text { x: rx + 5.0, y: y + FONT_AXIS_LABEL * 0.35, text: y2_labels.get(i).cloned().unwrap_or_default(), size: FONT_AXIS_LABEL, bold: false, anchor: Anchor::Default, fill: super::axes::INK.into(), rotate: None });
        }
    }
    axes(out, inner, spec, data, &y_scale, None, false, &ax);
    out.extend(axis2);
    let pad = 4.0;
    out.push(Prim::Clip { x: inner.x - pad, y: inner.y - pad, w: inner.w + 2.0 * pad, h: inner.h + 2.0 * pad });
    out.extend(bars);
    out.extend(lines);
    out.extend(pts);
    out.push(Prim::Unclip);
    out.extend(labels);
}

// ── ChartView ───────────────────────────────────────────────────────────────

/// The words the chart writes itself (`t('sheet_chart_series')`, `t('sheet_chart_no_data')`).
#[derive(Clone, Debug)]
pub struct ChartWords {
    pub series: String,
    pub no_data: String,
}

impl Default for ChartWords {
    fn default() -> Self {
        Self { series: "Series".into(), no_data: "No data".into() }
    }
}

/// `ChartView`: a persisted chart → its primitives in a `width`×`height` box at the origin.
pub fn render_chart(chart: &serde_json::Value, width: f64, height: f64, get: &super::data::CellGetter, words: &ChartWords, lang: &str) -> Vec<Prim> {
    let spec = normalize_chart(chart);
    let data = super::data::resolve_chart_data(&spec, get, &words.series);
    let legend_data = if spec.kind == ChartKind::Pie { pie_legend_data(&data) } else { data.clone() };
    let layout = compute_layout(width, height, &spec, &legend_data);
    let has_data = data.series.iter().any(|s| s.values.iter().any(Option::is_some));
    let mut out = vec![Prim::Rect { x: 0.0, y: 0.0, w: width, h: height, rx: None, fill: SURFACE.into(), fill_opacity: None, stroke: None, stroke_width: None, tip: None }];
    if let Some(t) = &layout.title {
        out.push(Prim::Text { x: t.x, y: t.y, text: t.text.clone(), size: t.font_size, bold: true, anchor: Anchor::Middle, fill: TEXT_PRIMARY.into(), rotate: None });
    }
    if let Some(t) = &layout.axis_title_x {
        out.push(Prim::Text { x: t.x, y: t.y, text: t.text.clone(), size: t.font_size, bold: false, anchor: Anchor::Middle, fill: super::axes::INK.into(), rotate: None });
    }
    if let Some(t) = &layout.axis_title_y {
        out.push(Prim::Text { x: t.x, y: t.y, text: t.text.clone(), size: t.font_size, bold: false, anchor: Anchor::Middle, fill: super::axes::INK.into(), rotate: Some(-90.0) });
    }
    if let Some(l) = &layout.legend {
        legend(&mut out, l, &legend_data);
    }
    if !has_data {
        let r = layout.plot;
        out.push(Prim::Text { x: r.x + r.w / 2.0, y: r.y + r.h / 2.0, text: words.no_data.clone(), size: 11.0, bold: false, anchor: Anchor::Middle, fill: CHART_NEUTRAL.into(), rotate: None });
    } else if spec.kind == ChartKind::Combo && has_secondary_axis(&data) {
        combo(&mut out, &spec, &data, &layout, lang);
    } else if is_cartesian(spec.kind) {
        cartesian(&mut out, &spec, &data, &layout, lang);
    } else if spec.kind == ChartKind::Pie {
        circular(&mut out, &spec, &data, &layout, lang);
    } else {
        radar(&mut out, &spec, &data, &layout, lang);
    }
    out
}
