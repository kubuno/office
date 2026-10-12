//! Kubuno Documents, the portable app (`desktop/common`).
//!
//! Everything the word processor does that no operating system does differently: the launch options and
//! the document they open ([`app`]), the open document's view state and page placement ([`model`]), and the
//! whole server path ([`api`]: the routes, the rules that protect the user's work, the crash journal and the
//! live session thread). The document itself — held losslessly, laid out, paginated and edited — is the
//! module's shared engine, `kubuno-office-docs-core` (`common/core`).
//!
//! What only an OS can do goes through the extension points of [`platform`], which have portable defaults:
//! the app builds and runs anywhere with nothing registered ([`app::run`] with
//! [`platform::Platform::portable`] and [`app::TextUi`]). `desktop/windows` registers the Win32 window
//! painted with Direct2D; `desktop/linux` and `desktop/macos` run the defaults today.

pub mod api;
pub mod app;
pub mod model;
pub mod platform;
