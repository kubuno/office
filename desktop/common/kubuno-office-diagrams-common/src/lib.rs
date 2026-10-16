//! Kubuno Diagrams, the portable app (`desktop/common`).
//!
//! Everything the diagram editor does that no operating system does differently: the launch options and
//! the diagram they open ([`app`], [`doc`]), and the server path ([`api`]: the diagram routes and the
//! host of the shared office save session - digest guard, editing session, crash journal). The diagram
//! itself - held losslessly, drawn, edited - is the module's shared engine, `kubuno-office-diagrams-core`
//! (`common/diagrams-core`). Design of record: vskubuno `docs/DIAGRAMS-DESKTOP.md`.
//!
//! What only an OS can do goes through the extension points of [`platform`], which have portable
//! defaults: the app builds and runs anywhere with nothing registered ([`app::run`] with
//! [`platform::Platform::portable`] and [`app::TextUi`]).

pub mod api;
pub mod app;
pub mod doc;
pub mod platform;
