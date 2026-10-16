//! Kubuno Projects, the portable app (`desktop/common`).
//!
//! Everything the project editor does that no operating system does differently: the launch options and the
//! project they open ([`app`]), the server path ([`api`]: the project routes, the worker thread, the journal
//! of unsent work; [`session`]: the outbox fed one call at a time, the re-reads, the retries), the table's
//! column layout ([`model`]) and the sample project ([`sample`]). The project itself - held losslessly,
//! scheduled, laid out and edited - is the module's shared engine, `kubuno-office-projects-core`
//! (`common/projects-core`).
//!
//! What only an OS can do goes through the extension points of [`platform`], which have portable defaults:
//! the app builds and runs anywhere with nothing registered ([`app::run`] with [`platform::Platform::portable`]
//! and [`app::TextUi`]). `desktop/windows` registers the Win32 window.

pub mod api;
pub mod app;
pub mod model;
pub mod platform;
pub mod sample;
pub mod session;
