//! Kubuno Whiteboard, the portable app (`desktop/common`).
//!
//! Everything the whiteboard editor does that no operating system does differently: the launch options and
//! the board they open ([`app`], [`doc`]), the board routes ([`api`]) and the real-time collaboration the
//! board syncs over ([`collab`]). The board itself — the scene, the Yjs document, the renderer — is the
//! module's shared engine, `kubuno-office-whiteboard-core` (`common/whiteboard-core`). Design of record:
//! vskubuno `docs/WHITEBOARD-DESKTOP.md`.
//!
//! What only an OS can do goes through the extension points of [`platform`], which have portable defaults:
//! the app builds and runs anywhere with nothing registered ([`app::run`] with
//! [`platform::Platform::portable`] and [`app::TextUi`]).
//!
//! Unlike Documents and Diagrams, a board's content is **not** a JSON blob saved by a digest-guarded PATCH:
//! it is a Yjs CRDT document synced over a WebSocket (`crate::collab`), so there is no save session here.

pub mod api;
pub mod app;
pub mod collab;
pub mod doc;
pub mod platform;
