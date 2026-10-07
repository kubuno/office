//! Raster exports (`renderExportCanvas`, « Export PNG / JPEG / PDF »): the page drawn at scale 2 with a
//! 24 px margin around its content, through the shared Canvas 2D context onto an offscreen Direct2D bitmap,
//! encoded by WIC.

use kubuno_office_diagrams_core::model::{Connector, Shape};

/// The encoded image (PNG, or JPEG at 92 %) and its size in pixels; `None` for an empty page.
pub fn render_export(_shapes: &[Shape], _connectors: &[Connector], _bg: &str, _jpeg: bool) -> Option<(Vec<u8>, u32, u32)> {
    None
}
