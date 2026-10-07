//! `kubuno-office-slides-core` — Kubuno Office's presentation engine, a platform-neutral port of the web
//! editor (`office/web/src/PresentationEditorPage.tsx` and its helpers), shared by the office clients
//! (the desktop apps of every OS today; the web through WASM later).
//!
//! | Module | Web origin | Content |
//! |---|---|---|
//! | [`model`] | `api.ts` types, server `content_files.rs` | the `.kbsld` content held losslessly, the deck, the theme |
//! | [`richtext`] | `presentationRichText.ts` | doc ⇄ paragraphs of runs, `layoutRich` over [`richtext::Measure`] |
//! | [`textedit`] | the browser's `contentEditable` | caret, selection, typing and marks inside a text box |
//! | [`surface`], [`color`] | `CanvasRenderingContext2D` | the drawing context the renderer uses, over a platform [`surface::Surface`] |
//! | [`render`] | `SlideRenderer` | every element type, backgrounds, the slideshow reveal |
//! | [`chart`], [`table`], [`smartart`] | `presentationChart.ts`, `presentationTable.ts`, `presentationSmartArt.ts` | charts, tables, SmartArt |
//! | [`insert`] | the editor's insert handlers, `SLIDE_LAYOUTS` | new elements exactly as the web builds them |
//! | [`edit`] | `SlideCanvas` | selection, hit testing, the pointer gestures, snapping, every element command |
//! | [`history`] | the editor's undo stack | per-slide snapshots, 500 ms coalescing, depth 80 |
//! | [`show`] | `PresenterMode` | the slideshow sequence, reveals, transitions |
//!
//! No UI, no Windows API, no clock, no randomness: platforms measure text, draw, give the time and seed the ids.
//! Shapes come from the office-wide `kubuno-office-shapes-core`.

pub mod chart;
pub mod color;
pub mod edit;
pub mod history;
pub mod insert;
pub mod model;
pub mod render;
pub mod richtext;
pub mod show;
pub mod smartart;
pub mod surface;
pub mod table;
pub mod textedit;

/// A number as JavaScript's `String(n)` prints it.
pub(crate) fn path_num(v: f64) -> String {
    kubuno_office_shapes_core::path::js_num(v)
}

/// JavaScript's `Math.round`.
pub(crate) fn js_round(v: f64) -> f64 {
    kubuno_office_shapes_core::path::js_round(v)
}
