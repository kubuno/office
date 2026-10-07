//! Stacking (`render/stacks.ts`), label formatting (`render/format.ts`) and marks/paths (`render/marks.tsx`).

use super::data::{ChartData, ResolvedSeries};
use super::layout::approx_measure;
use super::model::{Stacking, SYMBOL_CYCLE};
use super::scene::{f2, num, Prim};
use crate::format::{compact_number, locale_fixed};
use crate::js;

// ── stacks.ts ───────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StackedPoint {
    pub v: Option<f64>,
    pub base: f64,
    pub top: f64,
    pub share: f64,
}

/// `pointCount`.
pub fn point_count(data: &ChartData) -> usize {
    let mut n = data.categories.len();
    for s in &data.series {
        n = n.max(s.values.len());
        if let Some(x) = &s.x {
            n = n.max(x.len());
        }
    }
    n.max(1)
}

/// `categoryLabels`.
pub fn category_labels(data: &ChartData, n: usize) -> Vec<String> {
    (0..n).map(|i| data.categories.get(i).cloned().unwrap_or_else(|| (i + 1).to_string())).collect()
}

/// `computeStacks`.
pub fn compute_stacks(series: &[&ResolvedSeries], n: usize, stacking: Stacking) -> Vec<Vec<StackedPoint>> {
    let mut sum_abs = vec![0.0; n];
    for s in series {
        for (i, slot) in sum_abs.iter_mut().enumerate() {
            if let Some(Some(v)) = s.values.get(i) {
                *slot += v.abs();
            }
        }
    }
    let mut pos = vec![0.0; n];
    let mut neg = vec![0.0; n];
    series
        .iter()
        .map(|s| {
            (0..n)
                .map(|i| {
                    let Some(v) = s.values.get(i).copied().flatten() else {
                        return StackedPoint { v: None, base: pos[i], top: pos[i], share: 0.0 };
                    };
                    let share = if sum_abs[i] > 0.0 { v / sum_abs[i] } else { 0.0 };
                    let val = if stacking == Stacking::Percent { share } else { v };
                    if stacking == Stacking::None {
                        StackedPoint { v: Some(v), base: 0.0, top: val, share }
                    } else if val >= 0.0 {
                        let base = pos[i];
                        pos[i] = base + val;
                        StackedPoint { v: Some(v), base, top: pos[i], share }
                    } else {
                        let base = neg[i];
                        neg[i] = base + val;
                        StackedPoint { v: Some(v), base, top: neg[i], share }
                    }
                })
                .collect()
        })
        .collect()
}

// ── format.ts ───────────────────────────────────────────────────────────────

fn pattern_decimals(p: &str) -> u32 {
    let Some(dot) = p.find('.') else { return 0 };
    p[dot + 1..].chars().take_while(|c| *c == '0' || *c == '#').count() as u32
}

/// `formatAxisTick`.
pub fn format_axis_tick(v: f64, step: f64, percent: bool, pattern: Option<&str>, lang: &str) -> String {
    if percent {
        return format!("{}%", js::to_string(js::round(v * 100.0)));
    }
    if let Some(p) = pattern.filter(|p| !p.is_empty()) {
        let dec = pattern_decimals(p);
        if p.contains('%') {
            return format!("{}%", locale_fixed(v * 100.0, 0, dec, lang));
        }
        return locale_fixed(v, dec, dec, lang);
    }
    if step.abs() >= 1000.0 {
        return compact_number(v, lang);
    }
    super::scale::format_tick(v, step, lang)
}

/// `formatDataValue`.
pub fn format_data_value(v: f64, lang: &str) -> String {
    let clean = js::number(&js::to_precision(v, 10)).unwrap_or(v);
    locale_fixed(clean, 0, 2, lang)
}

/// `formatShare`.
pub fn format_share(frac: f64, lang: &str) -> String {
    let pct = frac * 100.0;
    let digits = if pct.abs() < 9.95 { 1 } else { 0 };
    format!("{}%", locale_fixed(pct, 0, digits, lang))
}

/// `truncateToWidth`.
pub fn truncate_to_width(text: &str, max_w: f64, fs: f64) -> String {
    if approx_measure(text, fs).0 <= max_w {
        return text.to_string();
    }
    let len = js::len16(text);
    let (mut lo, mut hi) = (0usize, len);
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if approx_measure(&format!("{}…", js::slice(text, 0.0, Some(mid as f64))), fs).0 <= max_w {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    if lo == 0 {
        "…".into()
    } else {
        format!("{}…", js::slice(text, 0.0, Some(lo as f64)))
    }
}

/// `inkFor`: readable ink over a fill.
pub fn ink_for(hex: &str) -> String {
    let h = hex.strip_prefix('#').unwrap_or(hex);
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return "var(--color-text-primary, #202124)".into();
    }
    let n = u32::from_str_radix(h, 16).unwrap_or(0);
    let lum = 0.2126 * ((n >> 16) & 255) as f64 + 0.7152 * ((n >> 8) & 255) as f64 + 0.0722 * (n & 255) as f64;
    if lum < 140.0 { "#ffffff" } else { "#202124" }.into()
}

// ── marks.tsx ───────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pt {
    pub x: f64,
    pub y: f64,
}

pub fn symbol_for(i: usize) -> &'static str {
    SYMBOL_CYCLE[i % SYMBOL_CYCLE.len()]
}

/// `symbolPath`.
pub fn symbol_path(shape: &str, cx: f64, cy: f64, r: f64) -> String {
    let (x, y, rr) = (f2(cx), f2(cy), f2(r));
    let p = |a: f64, b: f64| format!("{} {}", num(a), num(b));
    match shape {
        "diamond" => format!("M {} L {} L {} L {} Z", p(x, f2(y - rr)), p(f2(x + rr), y), p(x, f2(y + rr)), p(f2(x - rr), y)),
        "arrow-down" => format!("M {} L {} L {} Z", p(f2(x - rr), f2(y - rr)), p(f2(x + rr), f2(y - rr)), p(x, f2(y + rr))),
        "arrow-up" => format!("M {} L {} L {} Z", p(f2(x - rr), f2(y + rr)), p(f2(x + rr), f2(y + rr)), p(x, f2(y - rr))),
        "arrow-right" => format!("M {} L {} L {} Z", p(f2(x - rr), f2(y - rr)), p(f2(x - rr), f2(y + rr)), p(f2(x + rr), y)),
        "arrow-left" => format!("M {} L {} L {} Z", p(f2(x + rr), f2(y - rr)), p(f2(x + rr), f2(y + rr)), p(f2(x - rr), y)),
        "bowtie" => format!(
            "M {} L {} L {} Z M {} L {} L {} Z",
            p(f2(x - rr), f2(y - rr)),
            p(f2(x - rr), f2(y + rr)),
            p(x, y),
            p(f2(x + rr), f2(y - rr)),
            p(f2(x + rr), f2(y + rr)),
            p(x, y)
        ),
        "hourglass" => format!(
            "M {} L {} L {} Z M {} L {} L {} Z",
            p(f2(x - rr), f2(y - rr)),
            p(f2(x + rr), f2(y - rr)),
            p(x, y),
            p(f2(x - rr), f2(y + rr)),
            p(f2(x + rr), f2(y + rr)),
            p(x, y)
        ),
        _ => format!("M {} L {} L {} L {} Z", p(f2(x - rr), f2(y - rr)), p(f2(x + rr), f2(y - rr)), p(f2(x + rr), f2(y + rr)), p(f2(x - rr), f2(y + rr))),
    }
}

/// `<Marker>`: a symbol with a 1 px surface ring.
pub fn marker(shape: &str, cx: f64, cy: f64, r: f64, fill: &str, tip: Option<String>) -> Prim {
    Prim::Path { d: symbol_path(shape, cx, cy, r), fill: fill.into(), fill_opacity: None, evenodd: false, stroke: Some("var(--color-surface-0, #ffffff)".into()), stroke_width: Some(1.0), round: 0, tip }
}

fn split_runs(pts: &[Option<Pt>]) -> Vec<Vec<Pt>> {
    let mut runs = Vec::new();
    let mut cur = Vec::new();
    for p in pts {
        match p {
            Some(p) => cur.push(*p),
            None => {
                if !cur.is_empty() {
                    runs.push(std::mem::take(&mut cur));
                }
            }
        }
    }
    if !cur.is_empty() {
        runs.push(cur);
    }
    runs
}

fn run_cmds(run: &[Pt], smooth: bool) -> String {
    if run.len() < 2 {
        return String::new();
    }
    if !smooth {
        return run[1..].iter().map(|p| format!("L {} {}", num(f2(p.x)), num(f2(p.y)))).collect::<Vec<_>>().join(" ");
    }
    let mut d = String::new();
    for i in 0..run.len() - 1 {
        let p0 = run[i.saturating_sub(1)];
        let p1 = run[i];
        let p2 = run[i + 1];
        let p3 = run[(i + 2).min(run.len() - 1)];
        let (c1x, c1y) = (p1.x + (p2.x - p0.x) / 6.0, p1.y + (p2.y - p0.y) / 6.0);
        let (c2x, c2y) = (p2.x - (p3.x - p1.x) / 6.0, p2.y - (p3.y - p1.y) / 6.0);
        d.push_str(&format!(" C {} {} {} {} {} {}", num(f2(c1x)), num(f2(c1y)), num(f2(c2x)), num(f2(c2y)), num(f2(p2.x)), num(f2(p2.y))));
    }
    d.trim().to_string()
}

/// `linePath`.
pub fn line_path(pts: &[Option<Pt>], smooth: bool) -> String {
    split_runs(pts)
        .into_iter()
        .filter(|r| r.len() >= 2)
        .map(|r| format!("M {} {} {}", num(f2(r[0].x)), num(f2(r[0].y)), run_cmds(&r, smooth)))
        .collect::<Vec<_>>()
        .join(" ")
}

/// `areaPath`.
pub fn area_path(top: &[Option<Pt>], bottom: &[Option<Pt>], smooth: bool) -> String {
    let n = top.len().min(bottom.len());
    let mut parts = Vec::new();
    let mut start: i64 = -1;
    let flush = |end: usize, start: &mut i64, parts: &mut Vec<String>| {
        if *start < 0 || end as i64 - *start < 2 {
            *start = -1;
            return;
        }
        let s = *start as usize;
        let t: Vec<Pt> = (s..end).filter_map(|i| top[i]).collect();
        let mut b: Vec<Pt> = (s..end).filter_map(|i| bottom[i]).collect();
        b.reverse();
        parts.push(format!("M {} {} {} L {} {} {} Z", num(f2(t[0].x)), num(f2(t[0].y)), run_cmds(&t, smooth), num(f2(b[0].x)), num(f2(b[0].y)), run_cmds(&b, smooth)));
        *start = -1;
    };
    for i in 0..n {
        if top[i].is_some() && bottom[i].is_some() {
            if start < 0 {
                start = i as i64;
            }
        } else {
            flush(i, &mut start, &mut parts);
        }
    }
    flush(n, &mut start, &mut parts);
    parts.join(" ")
}
