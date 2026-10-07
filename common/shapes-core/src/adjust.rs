//! Adjustment handles — the yellow knobs that reshape a shape without resizing it — a port of the web's
//! `shapes/adjust.ts`.
//!
//! Native kinds (`roundRect`, `arrow`, `star`, `callout`, `plus`) store FRACTIONS of their box; every other
//! catalogue kind stores the raw OOXML units of its preset, and its knobs come from the preset data.

use crate::path::Rect;
use crate::preset::{self, Env};

type At = fn(f64, f64, f64) -> (f64, f64);
type From = fn(f64, f64, f64, f64) -> f64;

/// One fraction adjustment of a native kind.
#[derive(Clone, Copy)]
pub struct AdjustSpec {
    /// OOXML name (`adj`, `adj1`…).
    pub name: &'static str,
    pub def: f64,
    pub min: f64,
    pub max: f64,
    /// `x` or `y`: the box axis the knob slides along.
    pub axis: char,
    /// Knob position (fractions of the box) for value `v` in a `w`×`h` box.
    pub at: At,
    /// The value a knob dropped at `(fx, fy)` (fractions) implies in a `w`×`h` box.
    pub from: From,
}

impl std::fmt::Debug for AdjustSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdjustSpec").field("name", &self.name).field("def", &self.def).finish()
    }
}

fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    v.max(lo).min(hi)
}

const ROUND_RECT: [AdjustSpec; 1] = [AdjustSpec {
    name: "adj",
    def: 0.16,
    min: 0.0,
    max: 0.5,
    axis: 'x',
    at: |v, w, h| (if w > 0.0 { (w.min(h) * v) / w } else { 0.0 }, 0.0),
    from: |fx, _fy, w, h| {
        let m = w.min(h);
        if m > 0.0 {
            clamp((fx * w) / m, 0.0, 0.5)
        } else {
            0.16
        }
    },
}];

const ARROW: [AdjustSpec; 2] = [
    AdjustSpec { name: "adj1", def: 0.5, min: 0.05, max: 0.95, axis: 'y', at: |v, _w, _h| (0.0, 0.5 - v / 2.0), from: |_fx, fy, _w, _h| clamp((0.5 - fy) * 2.0, 0.05, 0.95) },
    AdjustSpec {
        name: "adj2",
        def: 0.5,
        min: 0.05,
        max: 1.0,
        axis: 'x',
        at: |v, w, h| (if w > 0.0 { (w - w.min(w.min(h) * v)) / w } else { 0.0 }, 0.0),
        from: |fx, _fy, w, h| {
            let m = w.min(h);
            if m > 0.0 {
                clamp(((1.0 - fx) * w) / m, 0.05, 1.0)
            } else {
                0.5
            }
        },
    },
];

const STAR: [AdjustSpec; 1] = [AdjustSpec {
    name: "adj",
    def: 0.38,
    min: 0.05,
    max: 0.95,
    axis: 'x',
    at: |v, _w, _h| {
        let a = std::f64::consts::PI / 5.0 - std::f64::consts::FRAC_PI_2;
        (0.5 + (v / 2.0) * a.cos(), 0.5 + (v / 2.0) * a.sin())
    },
    from: |fx, fy, _w, _h| {
        let (dx, dy) = ((fx - 0.5) * 2.0, (fy - 0.5) * 2.0);
        clamp(dx.hypot(dy), 0.05, 0.95)
    },
}];

const CALLOUT: [AdjustSpec; 2] = [
    AdjustSpec { name: "adj1", def: 0.72, min: 0.3, max: 0.95, axis: 'y', at: |v, _w, _h| (0.0, v), from: |_fx, fy, _w, _h| clamp(fy, 0.3, 0.95) },
    AdjustSpec { name: "adj2", def: 0.28, min: 0.0, max: 1.0, axis: 'x', at: |v, _w, _h| (v, 1.0), from: |fx, _fy, _w, _h| clamp(fx, 0.0, 1.0) },
];

const PLUS: [AdjustSpec; 1] = [AdjustSpec {
    name: "adj",
    def: 0.35,
    min: 0.05,
    max: 0.95,
    axis: 'x',
    at: |v, w, h| (if w > 0.0 { 0.5 - (w.min(h) * v) / (2.0 * w) } else { 0.5 }, 0.0),
    from: |fx, _fy, w, h| {
        let m = w.min(h);
        if m > 0.0 {
            clamp(((0.5 - fx) * 2.0 * w) / m, 0.05, 0.95)
        } else {
            0.35
        }
    },
}];

/// The fraction adjustments of a native kind (`SHAPE_ADJUSTMENTS`), `None` for preset-backed kinds.
pub fn adjustments(kind: &str) -> Option<&'static [AdjustSpec]> {
    match kind {
        "roundRect" => Some(&ROUND_RECT),
        "arrow" => Some(&ARROW),
        "star" => Some(&STAR),
        "callout" => Some(&CALLOUT),
        "plus" => Some(&PLUS),
        _ => None,
    }
}

/// True for the kinds whose adjustments are fractions (`FRACTION_KINDS`).
pub fn is_fraction_kind(kind: &str) -> bool {
    adjustments(kind).is_some()
}

/// Adjustment values in use: the shape's own (clamped) or the defaults; raw preset units for
/// preset-backed kinds.
pub fn adj_values(kind: &str, adj: Option<&[f64]>) -> Vec<f64> {
    if let Some(specs) = adjustments(kind) {
        return specs
            .iter()
            .enumerate()
            .map(|(i, s)| match adj.and_then(|a| a.get(i)) {
                Some(v) if v.is_finite() => clamp(*v, s.min, s.max),
                _ => s.def,
            })
            .collect();
    }
    match preset::preset_of(kind) {
        Some(p) => preset::adj_values(p, adj),
        None => Vec::new(),
    }
}

/// A knob: which adjustment it drives and where it sits (content space).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Handle {
    pub index: usize,
    pub x: f64,
    pub y: f64,
}

/// Knob positions for a shape occupying `rect`.
pub fn adjust_handles(kind: &str, rect: Rect, adj: Option<&[f64]>) -> Vec<Handle> {
    if let Some(specs) = adjustments(kind) {
        let values = adj_values(kind, adj);
        return specs
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let (fx, fy) = (s.at)(values[i], rect.w, rect.h);
                Handle { index: i, x: rect.x + fx * rect.w, y: rect.y + fy * rect.h }
            })
            .collect();
    }
    let Some(p) = preset::preset_of(kind) else { return Vec::new() };
    let env = Env { w: rect.w, h: rect.h, adj: preset::adj_values(p, adj) };
    preset::preset_handle_positions(p, &env).into_iter().map(|(index, x, y)| Handle { index, x: rect.x + x, y: rect.y + y }).collect()
}

/// New adjustment values after dragging knob `index` to the content-space point `(px, py)`.
pub fn adjust_from_drag(kind: &str, rect: Rect, index: usize, px: f64, py: f64, adj: Option<&[f64]>) -> Vec<f64> {
    let mut values = adj_values(kind, adj);
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return values;
    }
    if let Some(specs) = adjustments(kind) {
        let Some(spec) = specs.get(index) else { return values };
        let fx = (px - rect.x) / rect.w;
        let fy = (py - rect.y) / rect.h;
        values[index] = (spec.from)(fx, fy, rect.w, rect.h);
        return values;
    }
    let Some(p) = preset::preset_of(kind) else { return values };
    preset::preset_adjust_from_drag(p, &Env { w: rect.w, h: rect.h, adj: values }, index, px - rect.x, py - rect.y)
}

/// The knob under a point, if any (within `slop`).
pub fn hit_adjust(kind: &str, rect: Rect, px: f64, py: f64, adj: Option<&[f64]>, slop: f64) -> Option<usize> {
    adjust_handles(kind, rect, adj).into_iter().find(|h| (px - h.x).abs() <= slop && (py - h.y).abs() <= slop).map(|h| h.index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_values_default_and_clamp() {
        assert_eq!(adj_values("roundRect", None), vec![0.16]);
        assert_eq!(adj_values("roundRect", Some(&[0.9])), vec![0.5]);
        assert_eq!(adj_values("arrow", Some(&[f64::NAN])), vec![0.5, 0.5]);
    }

    #[test]
    fn a_knob_drag_round_trips_on_the_rounded_rectangle() {
        let rect = Rect { x: 10.0, y: 10.0, w: 200.0, h: 100.0 };
        let h = adjust_handles("roundRect", rect, Some(&[0.2]))[0];
        assert!((h.x - (10.0 + 20.0)).abs() < 1e-9 && (h.y - 10.0).abs() < 1e-9);
        let v = adjust_from_drag("roundRect", rect, 0, 10.0 + 30.0, 10.0, None);
        assert!((v[0] - 0.3).abs() < 1e-9);
        assert_eq!(hit_adjust("roundRect", rect, 31.0, 12.0, Some(&[0.2]), 7.0), Some(0));
    }
}
