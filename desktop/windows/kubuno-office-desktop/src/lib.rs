//! Kubuno Documents for Windows — the Win32 window of the Office module's desktop word processor, written
//! like a Windows Forms application, over the portable app of desktop/common
//! (`kubuno-office-desktop-common`: the launch options, the open document's view state, the server session).
//!
//! The library holds everything the Visual Studio designer links to render the views (the design
//! build compiles this crate): the main form [`DocumentWindow`] (`views/document_window.kbview` +
//! `views/document_window.rs`) with its declarative ribbon, the Backstage's user control
//! [`BackstageInfo`], and the custom controls [`PageCanvas`] (the paginated document),
//! [`HorizontalRuler`], [`VerticalRuler`], [`RulerCorner`] and [`ZoomSlider`]. `main.rs` only
//! registers [`platform::platform`] and [`WindowsUi`] and runs the common app.
//!
//! The sources are grouped by role (vskubuno `docs/DESKTOP-MIGRATION.md`, "Source layout"), each
//! view next to its code-behind of the same name:
//!
//! - `views/` — the top-level view: the document window (and the tests over the views);
//! - `pages/` — the Backstage's user control (`backstage_info`);
//! - `controls/` — the custom-drawn controls: `page_canvas`, `zoom_slider`, `ruler/`;
//! - `doc/` — the Windows half of the document engine: DirectWrite measuring and Direct2D painting of the
//!   pages the shared engine (`kubuno-office-docs-core`, common/core) lays out;
//! - `platform/` — the Windows overrides of the common extension points, `painter` (the Direct2D drawing
//!   surface), the clipboard, the IME, WIC images and the file dialogs;
//! - `resources/` — the strings and icons (`resources.kbres`, `resources.fr.kbres`).
//!
//! [`api`] and [`model`] are desktop/common's, re-exported at their former place.

pub use kubuno_office_desktop_common::{api, model};

pub mod controls;
pub mod doc;
pub mod pages;
pub mod platform;
pub mod views;
mod windows_ui;

pub use controls::page_canvas::PageCanvas;
pub use controls::ruler::{HorizontalRuler, RulerCorner, VerticalRuler};
pub use controls::zoom_slider::ZoomSlider;
pub use pages::backstage_info::BackstageInfo;
pub use views::document_window::{DocumentWindow, Options};
pub use windows_ui::WindowsUi;

// `Resources::app_title()`, `Resources::status_page()`… — the strings of `resources/resources.kbres`
// (neutral English) and `resources/resources.fr.kbres`, in the current UI culture; `{Res key}` in the views.
kubuno_desktop::resources!(pub Resources, "resources/resources.kbres");
