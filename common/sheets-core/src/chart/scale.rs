//! Value-axis auto-scaling (`web/src/sheet-chart/scale.ts`, a port of LibreOffice chart2's
//! `ScaleAutomatism`, linear branch).

use super::data::ChartData;
use super::model::{ChartKind, ChartSpec, Stacking};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxisScale {
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub minor_count: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AutoScaleOptions {
    pub expand_to_zero: bool,
    pub expand_toward_zero: bool,
    pub expand_close_to_border: bool,
}

/// `scaleOptionsFor`.
pub fn scale_options_for(kind: ChartKind) -> AutoScaleOptions {
    let bar = matches!(kind, ChartKind::Column | ChartKind::Bar | ChartKind::Combo);
    AutoScaleOptions { expand_to_zero: bar || kind == ChartKind::Area, expand_toward_zero: true, expand_close_to_border: !bar && kind != ChartKind::Radar }
}

/// `autoScaleLinear`.
pub fn auto_scale_linear(data_min: f64, data_max: f64, n_max_ticks: f64, o: AutoScaleOptions) -> AxisScale {
    let (mut min, mut max) = if data_min.is_finite() && data_max.is_finite() { (data_min, data_max) } else { (0.0, 1.0) };
    if min > max {
        max = min;
    }
    let swap = min < 0.0 && max <= 0.0;
    if swap {
        let t = min;
        min = -max;
        max = -t;
    }
    let (src_min, src_max) = (min, max);
    if min > 0.0 {
        if min == max || min / max < 5.0 / 6.0 {
            if o.expand_to_zero {
                min = 0.0;
            }
        } else if o.expand_toward_zero {
            min -= (max - min) / 2.0;
        }
    }
    if min == max {
        if max == 0.0 {
            max = 1.0;
        } else if max > 0.0 {
            max *= 2.0;
        } else {
            min *= 2.0;
        }
    }
    let (mut ax_min, mut ax_max, mut distance) = (0.0, 0.0, 0.0);
    let (mut norm, mut mag) = (0.0f64, 0.0f64);
    let mut first = true;
    for _ in 0..80 {
        if first {
            let raw = (max - min) / n_max_ticks.max(1.0);
            let exp = (if raw.abs() > 0.0 { raw.abs() } else { 1.0 }).log10().floor();
            mag = 10f64.powf(exp);
            norm = raw / mag;
            if norm <= 1.0 {
                norm = 1.0;
            } else if norm <= 2.0 {
                norm = 2.0;
            } else if norm <= 5.0 {
                norm = 5.0;
            } else {
                mag *= 10.0;
                norm = 1.0;
            }
            first = false;
        } else if norm == 1.0 {
            norm = 2.0;
        } else if norm == 2.0 {
            norm = 5.0;
        } else {
            mag *= 10.0;
            norm = 1.0;
        }
        distance = norm * mag;
        ax_min = (min / distance + 1e-9).floor() * distance;
        ax_max = (max / distance - 1e-9).ceil() * distance;
        if o.expand_close_to_border {
            if ax_min != 0.0 && (ax_max - src_min) / (ax_max - ax_min) > 20.0 / 21.0 {
                ax_min -= distance;
            }
            if ax_max != 0.0 && (src_max - ax_min) / (ax_max - ax_min) > 20.0 / 21.0 {
                ax_max += distance;
            }
        }
        if ((ax_max - ax_min) / distance + 1e-7).floor() <= n_max_ticks {
            break;
        }
    }
    if swap {
        let t = ax_min;
        ax_min = -ax_max;
        ax_max = -t;
    }
    AxisScale { min: ax_min, max: ax_max, step: distance, minor_count: 2 }
}

/// `computeExtent`.
pub fn compute_extent(data: &ChartData, stacking: Stacking) -> (f64, f64) {
    if stacking == Stacking::Percent {
        return (0.0, 1.0);
    }
    let (mut min, mut max) = (f64::INFINITY, f64::NEG_INFINITY);
    if stacking == Stacking::Stacked {
        let len = data.series.iter().map(|s| s.values.len()).max().unwrap_or(0);
        for i in 0..len {
            let (mut pos, mut neg) = (0.0, 0.0);
            for s in &data.series {
                if let Some(Some(v)) = s.values.get(i) {
                    if *v >= 0.0 {
                        pos += v;
                    } else {
                        neg += v;
                    }
                }
            }
            if pos > max {
                max = pos;
            }
            if neg < min {
                min = neg;
            }
            if pos < min {
                min = pos;
            }
            if neg > f64::NEG_INFINITY && neg < 0.0 && 0.0 > max {
                max = 0.0;
            }
        }
    } else {
        for s in &data.series {
            for v in s.values.iter().flatten() {
                if *v < min {
                    min = *v;
                }
                if *v > max {
                    max = *v;
                }
            }
        }
    }
    if min == f64::INFINITY {
        return (0.0, 1.0);
    }
    (min, max)
}

/// `computeXExtent`.
pub fn compute_x_extent(data: &ChartData) -> (f64, f64) {
    let (mut min, mut max) = (f64::INFINITY, f64::NEG_INFINITY);
    for s in &data.series {
        if let Some(xs) = &s.x {
            for v in xs.iter().flatten() {
                min = min.min(*v);
                max = max.max(*v);
            }
        } else if !s.values.is_empty() {
            min = min.min(1.0);
            max = max.max(s.values.len() as f64);
        }
    }
    if min == f64::INFINITY {
        return (0.0, 1.0);
    }
    (min, max)
}

/// `computeValueScale`.
pub fn compute_value_scale(spec: &ChartSpec, data: &ChartData, n_max_ticks: f64) -> AxisScale {
    if spec.stacking == Stacking::Percent {
        let s = auto_scale_linear(0.0, 1.0, n_max_ticks, AutoScaleOptions { expand_to_zero: true, ..Default::default() });
        return AxisScale { min: 0.0, max: 1.0, ..s };
    }
    let (lo, hi) = compute_extent(data, spec.stacking);
    let auto = auto_scale_linear(lo, hi, n_max_ticks, scale_options_for(spec.kind));
    let min = spec.axis_y.min.unwrap_or(auto.min);
    let max = spec.axis_y.max.unwrap_or(auto.max);
    if min == auto.min && max == auto.max {
        return auto;
    }
    let span = auto_scale_linear(min, max, n_max_ticks, AutoScaleOptions::default());
    AxisScale { min, max, step: span.step, minor_count: auto.minor_count }
}

/// `ticksOf`.
pub fn ticks_of(s: &AxisScale) -> Vec<f64> {
    let mut out = Vec::new();
    if s.step <= 0.0 || !s.step.is_finite() {
        return out;
    }
    let n = crate::js::round((s.max - s.min) / s.step);
    let mut i = 0.0;
    while i <= n && i <= 1000.0 {
        out.push(s.min + i * s.step);
        i += 1.0;
    }
    out
}

/// `formatTick`: float-noise-free, locale decimal separator.
pub fn format_tick(v: f64, step: f64, lang: &str) -> String {
    let clean = crate::js::number(&crate::js::to_precision(v, 12)).unwrap_or(v);
    let dec = if step < 1.0 { (-(step.log10().floor())).clamp(0.0, 6.0) } else { 0.0 };
    crate::format::locale_fixed(clean, 0, dec.max(0.0) as u32, lang)
}
