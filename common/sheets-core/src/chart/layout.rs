//! Page layout (`web/src/sheet-chart/layout.ts`): title → legend → axis titles → 2 % page margin → plot.

use super::data::ChartData;
use super::model::*;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// `approxMeasure`: the web measures chart text with this heuristic (no canvas), so the geometry is the
/// same on every client. `text.length` is UTF-16 units.
pub fn approx_measure(text: &str, font_size: f64) -> (f64, f64) {
    (crate::js::len16(text) as f64 * font_size * 0.58, font_size * 1.3)
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlacedText {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub font_size: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LegendEntry {
    pub label: String,
    pub color: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LegendLayout {
    pub rect: Rect,
    pub entries: Vec<LegendEntry>,
    pub symbol_size: f64,
    pub font_size: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChartLayout {
    pub outer: Rect,
    pub title: Option<PlacedText>,
    pub legend: Option<LegendLayout>,
    pub axis_title_x: Option<PlacedText>,
    pub axis_title_y: Option<PlacedText>,
    pub plot: Rect,
}

/// `legendVisible`.
pub fn legend_visible(spec: &ChartSpec, data: &ChartData) -> bool {
    match spec.legend_show {
        LegendShow::Auto => spec.kind == ChartKind::Pie || data.series.len() > 1,
        LegendShow::Yes => true,
        LegendShow::No => false,
    }
}

const TITLE_GAP: f64 = 6.0;

fn layout_legend(rest: Rect, position: LegendPos, data: &ChartData) -> LegendLayout {
    let fs = FONT_LEGEND;
    let symbol = fs * 0.85;
    let pad_x = (fs * 0.5).max(6.0);
    let pad_y = (fs * 0.35).max(4.0);
    let gap = (fs * 0.3).max(4.0);
    let labels: Vec<&str> = data.series.iter().map(|s| s.name.as_str()).collect();
    let colors: Vec<&str> = data.series.iter().map(|s| s.color.as_str()).collect();
    let line_h = symbol.max(approx_measure("Ag", fs).1) + 4.0;
    let vertical = matches!(position, LegendPos::Left | LegendPos::Right);
    let max_text_w = (rest.w * LEGEND_MAX_WIDTH_RATIO - symbol - gap - 2.0 * pad_x).max(20.0);
    let entry_w = |l: &str| approx_measure(l, fs).0.min(max_text_w) + symbol + gap;
    let mut entries = Vec::new();
    let rect;
    if vertical {
        let w = labels.iter().map(|l| entry_w(l)).fold(0.0f64, f64::max) + 2.0 * pad_x;
        let h = rest.h.min(labels.len() as f64 * line_h + 2.0 * pad_y);
        let x = if position == LegendPos::Right { rest.x + rest.w - w } else { rest.x };
        let y = rest.y + ((rest.h - h) / 2.0).max(0.0);
        rect = Rect { x, y, w, h };
        for (i, l) in labels.iter().enumerate() {
            entries.push(LegendEntry { label: l.to_string(), color: colors[i].to_string(), x: pad_x, y: pad_y + i as f64 * line_h });
        }
    } else {
        let avail = rest.w;
        let (mut cx, mut cy, mut row_max) = (0.0, pad_y, 0.0f64);
        let mut pos = Vec::new();
        for l in &labels {
            let w = entry_w(l) + pad_x;
            if cx > 0.0 && cx + w > avail {
                cx = 0.0;
                cy += line_h;
            }
            pos.push((cx + pad_x, cy));
            cx += w;
            row_max = row_max.max(cx);
        }
        let h = cy + line_h + pad_y;
        let w = avail.min(row_max + pad_x);
        let x = rest.x + ((rest.w - w) / 2.0).max(0.0);
        let y = if position == LegendPos::Top { rest.y } else { rest.y + rest.h - h };
        rect = Rect { x, y, w, h };
        for (i, l) in labels.iter().enumerate() {
            entries.push(LegendEntry { label: l.to_string(), color: colors[i].to_string(), x: pos[i].0, y: pos[i].1 });
        }
    }
    LegendLayout { rect, entries, symbol_size: symbol, font_size: fs }
}

/// `computeLayout`.
pub fn compute_layout(width: f64, height: f64, spec: &ChartSpec, data: &ChartData) -> ChartLayout {
    let outer = Rect { x: 0.0, y: 0.0, w: width, h: height };
    let mut rest = outer;
    let mut layout = ChartLayout { outer, title: None, legend: None, axis_title_x: None, axis_title_y: None, plot: rest };
    if let Some(t) = &spec.title {
        let m = approx_measure(t, FONT_TITLE);
        layout.title = Some(PlacedText { text: t.clone(), x: rest.x + rest.w / 2.0, y: rest.y + TITLE_GAP + m.1 * 0.75, font_size: FONT_TITLE });
        let used = m.1 + 2.0 * TITLE_GAP;
        rest = Rect { y: rest.y + used, h: (rest.h - used).max(0.0), ..rest };
    }
    if legend_visible(spec, data) && !data.series.is_empty() {
        let legend = layout_legend(rest, spec.legend_pos, data);
        let vertical = matches!(spec.legend_pos, LegendPos::Left | LegendPos::Right);
        let too_big = if vertical { rest.w - legend.rect.w < 90.0 } else { rest.h - legend.rect.h < 60.0 };
        if !(spec.legend_show == LegendShow::Auto && too_big) {
            let gap = 4.0;
            let r = legend.rect;
            rest = match spec.legend_pos {
                LegendPos::Right => Rect { w: (r.x - gap - rest.x).max(0.0), ..rest },
                LegendPos::Left => Rect { x: r.x + r.w + gap, w: (rest.w - r.w - gap).max(0.0), ..rest },
                LegendPos::Top => Rect { y: r.y + r.h + gap, h: (rest.h - r.h - gap).max(0.0), ..rest },
                LegendPos::Bottom => Rect { h: (r.y - gap - rest.y).max(0.0), ..rest },
            };
            layout.legend = Some(legend);
        }
    }
    if is_cartesian(spec.kind) {
        if let Some(t) = spec.axis_x.title.as_ref().filter(|t| !t.is_empty()) {
            let m = approx_measure(t, FONT_AXIS_TITLE);
            layout.axis_title_x = Some(PlacedText { text: t.clone(), x: rest.x + rest.w / 2.0, y: rest.y + rest.h - m.1 * 0.25, font_size: FONT_AXIS_TITLE });
            rest = Rect { h: (rest.h - m.1 - 2.0).max(0.0), ..rest };
        }
        if let Some(t) = spec.axis_y.title.as_ref().filter(|t| !t.is_empty()) {
            let m = approx_measure(t, FONT_AXIS_TITLE);
            layout.axis_title_y = Some(PlacedText { text: t.clone(), x: rest.x + m.1 * 0.8, y: rest.y + rest.h / 2.0, font_size: FONT_AXIS_TITLE });
            rest = Rect { x: rest.x + m.1 + 2.0, w: (rest.w - m.1 - 2.0).max(0.0), ..rest };
        }
    }
    let (mx, my) = (width * PAGE_MARGIN_RATIO, height * PAGE_MARGIN_RATIO);
    layout.plot = Rect { x: rest.x + mx, y: rest.y + my, w: (rest.w - 2.0 * mx).max(0.0), h: (rest.h - 2.0 * my).max(0.0) };
    layout
}
