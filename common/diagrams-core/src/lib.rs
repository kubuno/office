//! `kubuno-office-diagrams-core` — the diagram engine every Kubuno Office client shares, ported from the
//! web's Diagrams editor (`office/web/src/DiagramEditorPage.tsx`, `stencils.ts` and what they import).
//! Design of record: vskubuno `docs/DIAGRAMS-DESKTOP.md`.
//!
//! | Module | Web origin | Content |
//! |---|---|---|
//! | [`canvas`] | `CanvasRenderingContext2D` | a Canvas 2D context over a platform [`canvas::Surface`] |
//! | [`color`] | CSS colours | the colour strings the web code writes |
//! | [`model`] | `DiagramData`, `DiagramShape`, `DiagramConnector`, `LayerDef` | a page, held losslessly |
//! | [`file`] | `content_files.rs`, « Export JSON » | the `.kbdia` file |
//! | [`stencils`] | `stencils.ts`, `diagram-shape-kinds.ts` | the stencil catalogue and its painting |
//! | [`hardware`] | `hardwareIcons.ts` | the « Ordinateur et Matériel » icons |
//! | [`geometry`] | `DiagramEditorPage.tsx:163-426` | ports, anchors, routing, hit tests, magnetism |
//! | [`layout`] | `computeLayout` | auto-layout |
//! | [`render`] | `renderCanvas`, `Minimap`, `Ruler`, `StencilThumbnail` | the scene |
//! | [`editor`] | the component's state and handlers | selection, gestures, commands, history |
//! | [`io`] | `diagramIo.ts`, `inflate.ts` | draw.io XML and CSV |
//! | [`templates`] | `diagramTemplates.ts` | the template gallery |
//! | [`pdf`] | `diagramPdf.ts` | one-page PDF around a JPEG |
//!
//! No clock, no randomness, no I/O: time and ids are given by the caller, files are bytes.

pub mod canvas;
pub mod color;
pub mod editor;
pub mod file;
pub mod geometry;
pub mod hardware;
pub mod io;
pub mod layout;
pub mod model;
pub mod pdf;
pub mod render;
pub mod stencils;
pub mod templates;
