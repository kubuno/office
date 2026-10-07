//! Kubuno Presentations, the portable app (`desktop/common`).
//!
//! Everything the presentation editor does that no operating system does differently: the launch options and
//! the presentation they open ([`app`]), the open presentation's editing state ([`model::workspace`]: slides,
//! selections, the canvas editor, history, text editing, clipboards), and the whole server path ([`api`]: the
//! routes, the per-slide save rules that protect the user's work, the crash journal and the live thread). The
//! engine — the stored format, the renderer, the editing gestures, the slideshow — is the module's shared core,
//! `kubuno-office-slides-core` (`common/slides-core`).
//!
//! What only an OS can do goes through [`platform`]; `desktop/windows` registers the Win32 window painted with
//! Direct2D, Linux and macOS run the text interface until their windows exist.

pub mod api;
pub mod app;
pub mod model;
pub mod pdf;
pub mod platform;
