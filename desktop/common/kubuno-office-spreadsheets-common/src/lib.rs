//! Kubuno Spreadsheets, the portable app (`desktop/common`).
//!
//! Everything the spreadsheet does that no operating system does differently: the launch options and the
//! workbook they open ([`app`]), the open workbook's view state — scrolling, the visible range, keeping the
//! active cell in view ([`model`]) — and the whole server path ([`api`]: the spreadsheet routes, the rules that
//! protect the user's work, the crash journal and the live session thread). The workbook itself — held
//! losslessly, evaluated, formatted and edited — is the module's shared engine, `kubuno-office-sheets-core`
//! (`common/sheets-core`).
//!
//! What only an OS can do goes through the extension points of [`platform`], which have portable defaults:
//! the app builds and runs anywhere with nothing registered ([`app::run`] with [`platform::Platform::portable`]
//! and [`app::TextUi`]). `desktop/windows` registers the Win32 window.

pub mod api;
pub mod app;
pub mod collab;
pub mod model;
pub mod platform;
pub mod sample;
