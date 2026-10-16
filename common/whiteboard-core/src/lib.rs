//! `kubuno-office-whiteboard-core` - the whiteboard engine every Kubuno Office client shares, ported from
//! the web's Whiteboard editor (`office/web/src/WhiteboardApp.tsx`, `whiteboard-engine.ts`,
//! `whiteboard-shapes.ts`, `whiteboard-excalidraw.ts`, `whiteboard-types.ts`). Design of record: vskubuno
//! `docs/WHITEBOARD-DESKTOP.md`.
//!
//! | Module | Web origin | Content |
//! |---|---|---|
//! | [`model`] | `whiteboard-types.ts` | the scene held losslessly (elements, strokes) |
//! | [`collab`] | `WhiteboardApp.tsx:314-315`, `collabProvider.ts` | the Yjs document (`elements` map + `strokes` array) and the sync URL |
//! | [`file`] | `content_files.rs`, `yjs_service.rs` | the gzipped `.kbwbd` snapshot and the part-concatenation load |
//! | [`viewport`] | `Viewport` | the infinite-canvas pan/zoom transform |
//! | [`engine`] | `whiteboard-engine.ts`, `whiteboard-shapes.ts` | scene build, hit-testing, handles, grouping, geometry, draw gesture |
//! | [`excalidraw`] | `whiteboard-excalidraw.ts` | the `.excalidraw` import |
//! | [`undo`] | `Y.UndoManager` | undo/redo of local changes, 500 ms grouping |
//! | [`render`] | `whiteboard-engine.ts` renderers | painting the scene onto a renderer-neutral [`render::Surface`] |
//!
//! Unlike `kubuno-office-docs-core`, the whiteboard's stored content is **not** a JSON document saved with
//! a digest-guarded PATCH: it is a Yjs CRDT document synced over a WebSocket and consolidated into a
//! gzipped Drive file, so this crate is built around [`collab::WhiteboardDoc`] rather than a `to_vec`
//! serialiser. No clock, no randomness, no I/O: time and ids are given by the caller, files are bytes.
//!
//! Shapes come from the office-wide [`kubuno_office_shapes_core`]. No UI, no Windows API: platform-neutral
//! for `wasm32-unknown-unknown`, Linux and macOS.

pub mod collab;
pub mod engine;
pub mod excalidraw;
pub mod file;
pub mod model;
pub mod render;
pub mod undo;
pub mod viewport;

pub use collab::{CollabError, WhiteboardDoc};
pub use model::{ArrowElement, BaseElement, FrameElement, ImageElement, Rect, Scene, ShapeElement, StickyNote, Stroke, TextBox, WbElement};
pub use viewport::{Point, Viewport};

/// Re-export of the shared shape engine, so clients draw the same geometries as every other office editor.
pub use kubuno_office_shapes_core as shapes;
