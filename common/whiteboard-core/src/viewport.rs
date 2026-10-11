//! The infinite canvas viewport — a port of `Viewport` in `office/web/src/whiteboard-engine.ts`.
//!
//! A pan (`tx`, `ty`) and a uniform `scale`, with the web's zoom bounds and the exact `zoomAt`, `fitTo
//! elements` and `getBounds` arithmetic. Not persisted (the web keeps it local, `WhiteboardApp.tsx:254`).

use crate::model::{Rect, WbElement};

/// The smallest zoom the board allows (`MIN_ZOOM`).
pub const MIN_ZOOM: f64 = 0.05;
/// The largest zoom the board allows (`MAX_ZOOM`).
pub const MAX_ZOOM: f64 = 20.0;

/// A point in content (canvas) space.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// The viewport transform.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    scale: f64,
    tx: f64,
    ty: f64,
}

impl Default for Viewport {
    fn default() -> Self {
        Viewport { scale: 1.0, tx: 0.0, ty: 0.0 }
    }
}

impl Viewport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn scale(&self) -> f64 {
        self.scale
    }
    pub fn tx(&self) -> f64 {
        self.tx
    }
    pub fn ty(&self) -> f64 {
        self.ty
    }

    /// `zoomPercent`: the scale as a rounded percentage.
    pub fn zoom_percent(&self) -> i32 {
        (self.scale * 100.0).round() as i32
    }

    /// `screenToCanvas`.
    pub fn screen_to_canvas(&self, sx: f64, sy: f64) -> Point {
        Point { x: (sx - self.tx) / self.scale, y: (sy - self.ty) / self.scale }
    }

    /// `canvasToScreen`.
    pub fn canvas_to_screen(&self, cx: f64, cy: f64) -> Point {
        Point { x: cx * self.scale + self.tx, y: cy * self.scale + self.ty }
    }

    /// `pan`.
    pub fn pan(&mut self, dx: f64, dy: f64) {
        self.tx += dx;
        self.ty += dy;
    }

    /// `zoomAt`: multiply the scale by `factor`, keeping the content point under `(cx, cy)` fixed.
    pub fn zoom_at(&mut self, factor: f64, cx: f64, cy: f64) {
        let ns = (self.scale * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let ratio = ns / self.scale;
        self.tx = cx - ratio * (cx - self.tx);
        self.ty = cy - ratio * (cy - self.ty);
        self.scale = ns;
    }

    /// `setZoom`: set the scale directly (clamped), keeping the pan.
    pub fn set_zoom(&mut self, z: f64) {
        self.scale = z.clamp(MIN_ZOOM, MAX_ZOOM);
    }

    /// `getBounds`: the content rectangle visible in a `w`×`h` viewport.
    pub fn bounds(&self, w: f64, h: f64) -> Rect {
        let tl = self.screen_to_canvas(0.0, 0.0);
        let br = self.screen_to_canvas(w, h);
        Rect { x: tl.x, y: tl.y, w: br.x - tl.x, h: br.y - tl.y }
    }

    /// `fitToElements`: centre and scale so the elements fill a `cw`×`ch` viewport.
    ///
    /// Arrows (and strokes) carry no `x/y/width/height`, so they are filtered out — the web's fix for the
    /// `NaN` transform that « Ajuster » produced when an arrow was present (`whiteboard-engine.ts:62-70`).
    pub fn fit_to_elements(&mut self, elements: &[WbElement], cw: f64, ch: f64) {
        let positioned: Vec<Rect> = elements
            .iter()
            .filter_map(|e| e.bounds())
            .filter(|b| b.x.is_finite() && b.y.is_finite() && b.w.is_finite() && b.h.is_finite())
            .collect();
        if positioned.is_empty() {
            return;
        }
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for b in &positioned {
            min_x = min_x.min(b.x);
            min_y = min_y.min(b.y);
            max_x = max_x.max(b.x + b.w);
            max_y = max_y.max(b.y + b.h);
        }
        let bw = if max_x - min_x != 0.0 { max_x - min_x } else { 400.0 };
        let bh = if max_y - min_y != 0.0 { max_y - min_y } else { 400.0 };
        // Proportional padding, like the web (8 % of the smaller side, clamped 12..80).
        let pad = (cw.min(ch) * 0.08).round().clamp(12.0, 80.0);
        self.scale = ((cw - pad * 2.0) / bw).min((ch - pad * 2.0) / bh).min(MAX_ZOOM);
        self.tx = (cw - bw * self.scale) / 2.0 - min_x * self.scale;
        self.ty = (ch - bh * self.scale) / 2.0 - min_y * self.scale;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_at_keeps_the_anchor_point_fixed() {
        let mut v = Viewport::new();
        let before = v.screen_to_canvas(100.0, 100.0);
        v.zoom_at(2.0, 100.0, 100.0);
        let after = v.screen_to_canvas(100.0, 100.0);
        assert!((before.x - after.x).abs() < 1e-9);
        assert!((before.y - after.y).abs() < 1e-9);
        assert_eq!(v.scale(), 2.0);
    }

    #[test]
    fn zoom_is_clamped() {
        let mut v = Viewport::new();
        v.set_zoom(1000.0);
        assert_eq!(v.scale(), MAX_ZOOM);
        v.set_zoom(0.0);
        assert_eq!(v.scale(), MIN_ZOOM);
    }

    #[test]
    fn screen_canvas_round_trip() {
        let mut v = Viewport::new();
        v.pan(37.0, -12.0);
        v.set_zoom(1.5);
        let p = v.canvas_to_screen(10.0, 20.0);
        let b = v.screen_to_canvas(p.x, p.y);
        assert!((b.x - 10.0).abs() < 1e-9 && (b.y - 20.0).abs() < 1e-9);
    }
}
