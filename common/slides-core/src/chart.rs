//! Chart elements — a port of the web's `presentationChart.ts`: drawing, and the CSV-like data text of the
//! chart editor.

use serde_json::{json, Value};

use crate::model::{as_f64, Element};
use crate::render::{font_of, plain_style};
use crate::richtext::Measure;
use crate::surface::{Baseline, Ctx, Paint, PathBuilder, Stroke};

/// `CHART_PALETTE`.
pub const CHART_PALETTE: [&str; 8] = ["#1a73e8", "#ea4335", "#fbbc04", "#34a853", "#9334e8", "#00acc1", "#ff7043", "#5f6368"];

/// `niceMax`: 1, 2, 5 or 10 × a power of ten.
pub fn nice_max(v: f64) -> f64 {
    if v <= 0.0 {
        return 1.0;
    }
    let pow = 10f64.powf(v.log10().floor());
    let n = v / pow;
    let step = if n <= 1.0 {
        1.0
    } else if n <= 2.0 {
        2.0
    } else if n <= 5.0 {
        5.0
    } else {
        10.0
    };
    step * pow
}

/// A series (`{name, values}`).
#[derive(Debug, Clone, PartialEq)]
pub struct Series {
    pub name: String,
    pub values: Vec<f64>,
}

/// The chart's data as the renderer reads it (defaults filled in like the web).
#[derive(Debug, Clone, PartialEq)]
pub struct ChartData {
    pub kind: String,
    pub categories: Vec<String>,
    pub series: Vec<Series>,
    pub palette: Vec<String>,
    pub title: Option<String>,
    pub show_legend: bool,
}

impl ChartData {
    pub fn of(el: &Element) -> ChartData {
        let palette: Vec<String> = el.get("palette").and_then(Value::as_array).map(|a| a.iter().filter_map(|c| c.as_str().map(str::to_string)).collect()).unwrap_or_default();
        let palette = if palette.is_empty() { CHART_PALETTE.iter().map(|s| s.to_string()).collect() } else { palette };
        let mut series: Vec<Series> = el
            .get("series")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|s| Series {
                        name: s.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
                        values: s.get("values").and_then(Value::as_array).map(|v| v.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect()).unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        if series.is_empty() {
            series = vec![Series { name: "Série 1".into(), values: vec![3.0, 5.0, 2.0, 6.0] }];
        }
        let mut categories: Vec<String> = el.get("categories").and_then(Value::as_array).map(|a| a.iter().map(|c| c.as_str().map(str::to_string).unwrap_or_else(|| c.to_string())).collect()).unwrap_or_default();
        if categories.is_empty() {
            categories = (0..series[0].values.len()).map(|i| format!("C{}", i + 1)).collect();
        }
        ChartData {
            kind: el.s("chartType").unwrap_or("column").to_string(),
            categories,
            series,
            palette,
            title: el.s("title").filter(|t| !t.is_empty()).map(str::to_string),
            show_legend: el.truthy("showLegend"),
        }
    }

    fn color(&self, i: usize) -> Paint {
        Paint::css(&self.palette[i % self.palette.len()])
    }
}

#[allow(clippy::too_many_arguments)]
fn text(ctx: &mut Ctx, m: &dyn Measure, s: &str, family: &str, size: f64, bold: bool, color: &str, x: f64, y: f64, align: &str) {
    let mut style = plain_style(family, size, color);
    style.bold = bold;
    let w = m.width(s, &style, 0.0);
    let left = match align {
        "center" => x - w / 2.0,
        "right" => x - w,
        _ => x,
    };
    ctx.fill_text(s, &font_of(&style), left, y, Baseline::Alphabetic, &Paint::css(color), 0.0);
}

/// `renderChart`.
#[allow(clippy::too_many_arguments)]
pub fn render_chart(ctx: &mut Ctx, el: &Element, x: f64, y: f64, w: f64, h: f64, sf: f64, m: &dyn Measure) {
    let d = ChartData::of(el);
    let fam = "Arial, sans-serif";
    let fs = 12.0 * sf;
    ctx.save();
    let mut top = y + 8.0 * sf;
    if let Some(t) = &d.title {
        text(ctx, m, t, fam, 14.0 * sf, true, "#202124", x + w / 2.0, top + 14.0 * sf, "center");
        top += 24.0 * sf;
    }
    let pie = d.kind == "pie" || d.kind == "donut";
    let legend_count = if pie { d.categories.len() } else { d.series.len() };
    if d.show_legend && legend_count > 0 {
        let labels: Vec<String> = if pie { d.categories.clone() } else { d.series.iter().map(|s| s.name.clone()).collect() };
        let mut lx = x + 10.0 * sf;
        let ly = top + 10.0 * sf;
        for (i, label) in labels.iter().enumerate() {
            ctx.fill_rect(lx, ly - 8.0 * sf, 10.0 * sf, 10.0 * sf, &d.color(i));
            let tw = label.chars().count() as f64 * fs * 0.55;
            text(ctx, m, label, fam, fs, false, "#5f6368", lx + 14.0 * sf, ly, "left");
            lx += 14.0 * sf + tw + 14.0 * sf;
        }
        top += 22.0 * sf;
    }
    let plot_x = x + 36.0 * sf;
    let plot_y = top;
    let plot_w = w - 46.0 * sf;
    let plot_h = y + h - plot_y - 22.0 * sf;
    if pie {
        let (cx, cy) = (x + w / 2.0, plot_y + plot_h / 2.0);
        let r = (plot_w.min(plot_h) / 2.0 - 4.0 * sf).max(4.0);
        let first = d.series.first();
        let total: f64 = (0..d.categories.len()).map(|i| first.and_then(|s| s.values.get(i)).copied().unwrap_or(0.0).abs()).sum();
        let total = if total == 0.0 { 1.0 } else { total };
        let mut a0 = -std::f64::consts::FRAC_PI_2;
        for i in 0..d.categories.len() {
            let val = first.and_then(|s| s.values.get(i)).copied().unwrap_or(0.0).abs();
            let a1 = a0 + (val / total) * std::f64::consts::TAU;
            let p = PathBuilder::new().move_to(cx, cy).arc(cx, cy, r, a0, a1, false).close().take();
            ctx.fill(&p, &d.color(i));
            a0 = a1;
        }
        if d.kind == "donut" {
            let p = PathBuilder::new().arc(cx, cy, r * 0.55, 0.0, std::f64::consts::TAU, false).close().take();
            ctx.fill(&p, &Paint::css("#ffffff"));
        }
        ctx.restore();
        return;
    }
    let max_v = nice_max(d.series.iter().flat_map(|s| s.values.iter()).fold(1.0f64, |a, v| a.max(v.abs())));
    let thin = Stroke { width: sf, ..Stroke::default() };
    let axes = PathBuilder::new().move_to(plot_x, plot_y).line_to(plot_x, plot_y + plot_h).line_to(plot_x + plot_w, plot_y + plot_h).take();
    ctx.stroke(&axes, &Paint::css("#dadce0"), &thin);
    for g in 0..=4 {
        let gy = plot_y + plot_h - (g as f64 / 4.0) * plot_h;
        let p = PathBuilder::new().move_to(plot_x, gy).line_to(plot_x + plot_w, gy).take();
        ctx.stroke(&p, &Paint::css("#f1f3f4"), &thin);
        let label = crate::path_num(crate::js_round((g as f64 / 4.0) * max_v));
        text(ctx, m, &label, fam, 10.0 * sf, false, "#9aa0a6", plot_x - 4.0 * sf, gy + 3.0 * sf, "right");
    }
    let n = d.categories.len();
    let ns = d.series.len().max(1);
    let val = |s: usize, i: usize| d.series.get(s).and_then(|x| x.values.get(i)).copied().unwrap_or(0.0);
    if d.kind == "column" {
        let group_w = plot_w / n.max(1) as f64;
        let bar_w = (group_w * 0.7) / ns as f64;
        for i in 0..n {
            for s in 0..d.series.len() {
                let bh = (val(s, i).abs() / max_v) * plot_h;
                let bx = plot_x + i as f64 * group_w + group_w * 0.15 + s as f64 * bar_w;
                ctx.fill_rect(bx, plot_y + plot_h - bh, bar_w * 0.9, bh, &d.color(s));
            }
        }
    } else if d.kind == "bar" {
        let group_h = plot_h / n.max(1) as f64;
        let bar_h = (group_h * 0.7) / ns as f64;
        for i in 0..n {
            for s in 0..d.series.len() {
                let bw = (val(s, i).abs() / max_v) * plot_w;
                let by = plot_y + i as f64 * group_h + group_h * 0.15 + s as f64 * bar_h;
                ctx.fill_rect(plot_x, by, bw, bar_h * 0.9, &d.color(s));
            }
        }
    } else {
        for s in 0..d.series.len() {
            let col = d.color(s);
            let pts: Vec<(f64, f64)> = (0..n)
                .map(|i| {
                    let px = plot_x + if n == 1 { plot_w / 2.0 } else { (i as f64 / (n - 1) as f64) * plot_w };
                    let py = plot_y + plot_h - (val(s, i).abs() / max_v) * plot_h;
                    (px, py)
                })
                .collect();
            if pts.is_empty() {
                continue;
            }
            if d.kind == "area" {
                let mut b = PathBuilder::new();
                b.move_to(pts[0].0, plot_y + plot_h);
                for p in &pts {
                    b.line_to(p.0, p.1);
                }
                b.line_to(pts[pts.len() - 1].0, plot_y + plot_h).close();
                let prev = ctx.alpha();
                ctx.set_alpha(prev * 0.3);
                ctx.fill(&b.take(), &col);
                ctx.set_alpha(prev);
            }
            let mut b = PathBuilder::new();
            for (i, p) in pts.iter().enumerate() {
                if i == 0 {
                    b.move_to(p.0, p.1);
                } else {
                    b.line_to(p.0, p.1);
                }
            }
            ctx.stroke(&b.take(), &col, &Stroke { width: 2.0 * sf, ..Stroke::default() });
            for p in &pts {
                let dot = PathBuilder::new().arc(p.0, p.1, 2.5 * sf, 0.0, std::f64::consts::TAU, false).take();
                ctx.fill(&dot, &col);
            }
        }
    }
    if d.kind != "bar" {
        for i in 0..n {
            let cx2 = plot_x
                + if n == 1 {
                    plot_w / 2.0
                } else if d.kind == "column" {
                    (i as f64 + 0.5) * (plot_w / n as f64)
                } else {
                    (i as f64 / (n - 1) as f64) * plot_w
                };
            text(ctx, m, &d.categories[i], fam, 10.0 * sf, false, "#9aa0a6", cx2, plot_y + plot_h + 13.0 * sf, "center");
        }
    }
    ctx.restore();
}

/// `parseChartData`: `"Cat,S1,S2\nA,3,5\nB,6,2"` (`,`, tab or `;`) into categories and series.
pub fn parse_chart_data(text: &str) -> (Vec<String>, Vec<Series>) {
    let rows: Vec<Vec<String>> = text.trim().split('\n').map(|r| r.split([',', '\t', ';']).map(|c| c.trim().to_string()).collect()).collect();
    let Some(header) = rows.first() else { return (Vec::new(), Vec::new()) };
    let mut series: Vec<Series> = header.iter().skip(1).map(|n| Series { name: n.clone(), values: Vec::new() }).collect();
    let mut categories = Vec::new();
    for row in rows.iter().skip(1) {
        if row.is_empty() || row.iter().all(|c| c.is_empty()) {
            continue;
        }
        categories.push(row[0].clone());
        for (s, ser) in series.iter_mut().enumerate() {
            let v = row.get(s + 1).and_then(|c| crate::richtext::parse_float(c)).unwrap_or(0.0);
            ser.values.push(v);
        }
    }
    (categories, series)
}

/// `chartDataToText`.
pub fn chart_data_to_text(categories: &[String], series: &[Series]) -> String {
    let header = std::iter::once(String::new()).chain(series.iter().map(|s| s.name.clone())).collect::<Vec<_>>().join(",");
    let rows = categories.iter().enumerate().map(|(i, c)| std::iter::once(c.clone()).chain(series.iter().map(|s| crate::path_num(s.values.get(i).copied().unwrap_or(0.0)))).collect::<Vec<_>>().join(","));
    std::iter::once(header).chain(rows).collect::<Vec<_>>().join("\n")
}

/// The JSON of categories and series, for an element.
pub fn data_json(categories: &[String], series: &[Series]) -> (Value, Value) {
    (json!(categories), Value::Array(series.iter().map(|s| json!({ "name": s.name, "values": s.values.iter().map(|v| crate::model::num(*v)).collect::<Vec<_>>() })).collect()))
}

/// A chart element's numbers, read without defaults (the editor shows what is stored).
pub fn stored_series(el: &Element) -> Vec<Series> {
    el.get("series").and_then(Value::as_array).map(|a| a.iter().map(|s| Series { name: s.get("name").and_then(Value::as_str).unwrap_or("").into(), values: s.get("values").and_then(Value::as_array).map(|v| v.iter().map(|x| as_f64(Some(x)).unwrap_or(0.0)).collect()).unwrap_or_default() }).collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nice_maxima() {
        assert_eq!(nice_max(7.0), 10.0);
        assert_eq!(nice_max(1.5), 2.0);
        assert_eq!(nice_max(30.0), 50.0);
        assert_eq!(nice_max(0.0), 1.0);
    }

    #[test]
    fn data_text_round_trips() {
        let (c, s) = parse_chart_data("Cat,S1,S2\nA,3,5\nB;6;x\n\n");
        assert_eq!(c, ["A", "B"]);
        assert_eq!(s[1].values, [5.0, 0.0]);
        assert_eq!(chart_data_to_text(&c, &s), ",S1,S2\nA,3,5\nB,6,0");
    }
}
