//! Kubuno Maths, the portable app (`desktop/common`).
//!
//! Everything the Maths workbench (expression editor, calculator, computer algebra, graphs, matrices,
//! statistics, number theory) does that no operating system does differently: the launch options and the
//! formula document they open ([`app`], [`model`]), the server path ([`api`]: the Maths REST routes built as
//! requests of the shared transport), and the offline sample ([`sample`]). The expression tree, the CAS, the
//! matrices/stats/number engine, the function-graph model and the stored block-document are the module's
//! shared engine, `kubuno-office-math-core` (`common/math-core`, module `engine`).
//!
//! Maths is **lightly** server-coupled: a formula's whole content is a single `latex` string, saved with a
//! plain `PATCH /office/maths/formulas/:id {latex}` (there is no digest/editing-session/collab model like
//! Documents — see vskubuno `docs/MATHS-DESKTOP.md` §2).
//!
//! What only an OS can do goes through the extension points of [`platform`], which have portable defaults:
//! the app builds and runs anywhere with nothing registered ([`app::run`] with [`platform::Platform::portable`]
//! and [`app::TextUi`]). `desktop/windows` registers the Win32 window.

pub mod api;
pub mod app;
pub mod model;
pub mod platform;
pub mod sample;
