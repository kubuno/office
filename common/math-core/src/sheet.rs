//! The spreadsheet's equation object as the web draws it (SpreadsheetApp.tsx, the equation overlay): a white
//! box (padding 4 px, border 1 px `#e2e4e6`, radius 4, faint shadow) holding KaTeX at `font-size: 18px × zoom`
//! in display mode — so 1 em of the formula is `18 × 1.21 × zoom` px and the display block's `1em` margins
//! (18 px × zoom) sit above and below it. An equation never resized (`bw`/`bh` absent) takes its natural
//! size (`eqNatRef`: the inner node's rounded `offsetWidth`/`offsetHeight` + 10, in zoom-1 px); a resized one
//! scales the formula by `bw / natural.w`, `bh / natural.h`.

use crate::{Rendered, KATEX_EM};

/// The overlay's font size at zoom 1 (px).
pub const BASE_FONT_PX: f64 = 18.0;
/// The line box the formula's block always has, in ems of the formula's font (measured in Chrome: 20 px
/// above and 8.125 px below the baseline at 21.78 px).
pub const LINE_TOP_EM: f64 = 20.0 / 21.78;
pub const LINE_BOTTOM_EM: f64 = 8.125 / 21.78;
/// ... and only 6.125 px below when the formula rises above that line box (measured: accents, wide accents).
pub const LINE_BOTTOM_TALL_EM: f64 = 6.125 / 21.78;
/// Padding + border of the box, each side (px, not zoomed).
pub const INSET_PX: f64 = 5.0;

/// The geometry of a laid-out equation in the sheet at a zoom (px).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EquationGeometry {
    /// The formula's font size (1 em of the display list), px.
    pub em_px: f64,
    /// The `.katex-html` block: width, height above and below the baseline (px).
    pub html_w: f64,
    pub html_top: f64,
    pub html_bottom: f64,
    /// The natural size the web stores and hit-tests (`eqNatRef`, zoom-1 px).
    pub natural_w: f64,
    pub natural_h: f64,
    /// The box the web paints when the equation was never resized (px at this zoom).
    pub auto_w: f64,
    pub auto_h: f64,
    /// The formula's origin (left of the block, top of the block) inside the box, px at this zoom.
    pub content_x: f64,
    pub content_y: f64,
}

/// The geometry of `r` drawn at `zoom`.
pub fn geometry(r: &Rendered, zoom: f64) -> EquationGeometry {
    let em_px = BASE_FONT_PX * KATEX_EM * zoom;
    let (html_w, html_top, html_bottom) = if r.error.is_some() {
        // The error text in the box's font (18 px, normal line height ≈ 1.2).
        let w = r.items.iter().map(|i| if let crate::Item::Text { text, .. } = i { text.chars().count() as f64 * 0.5 * BASE_FONT_PX * zoom } else { 0.0 }).sum::<f64>();
        (w, 0.9 * BASE_FONT_PX * zoom, 0.3 * BASE_FONT_PX * zoom)
    } else {
        {
        let bottom = if r.height > LINE_TOP_EM { LINE_BOTTOM_TALL_EM } else { LINE_BOTTOM_EM };
        (r.width * em_px, r.height.max(LINE_TOP_EM) * em_px, r.depth.max(bottom) * em_px)
    }
    };
    let html_h = html_top + html_bottom;
    // offsetWidth / offsetHeight are rounded to whole CSS px (measured at the current zoom, divided back).
    let natural_w = html_w.round() / zoom + 2.0 * INSET_PX;
    let natural_h = html_h.round() / zoom + 2.0 * INSET_PX;
    let margin = BASE_FONT_PX * zoom;
    EquationGeometry { em_px, html_w, html_top, html_bottom, natural_w, natural_h, auto_w: html_w + 2.0 * INSET_PX, auto_h: html_h + 2.0 * margin + 2.0 * INSET_PX, content_x: INSET_PX, content_y: INSET_PX + margin }
}
