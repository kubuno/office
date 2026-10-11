//! Kubuno Data, the portable app (`desktop/common`).
//!
//! Everything the Data (BI) report editor does that no operating system does differently: the launch options
//! and the report they open ([`app`]), the server path ([`api`]: the Data REST routes, built as requests of
//! the shared transport), and the sample report rendered offline ([`sample`]). The report document, the
//! data-shaping, the formatting, the palettes, the field wells, the visual catalogue and the editing
//! operations with undo/redo are the module's shared engine, `kubuno-office-data-core` (`common/data-core`).
//!
//! Data is **server-coupled** (datasets and queries run SQL on the server, the database is the source of
//! truth, the `.kbdrp` Drive file is a best-effort mirror): unlike Documents, there is no file-save session —
//! each edit is its own REST call. See vskubuno `docs/DATA-DESKTOP.md`.
//!
//! What only an OS can do goes through the extension points of [`platform`], which have portable defaults:
//! the app builds and runs anywhere with nothing registered ([`app::run`] with [`platform::Platform::portable`]
//! and [`app::TextUi`]). `desktop/windows` registers the Win32 window.

pub mod api;
pub mod app;
pub mod platform;
pub mod sample;
