//! Kubuno Diagrams for Windows — the Win32 window of the Office module's diagram editor, written like a
//! Windows Forms application, over the portable app of desktop/common (`kubuno-office-diagrams-common`)
//! and the shared diagram engine (`kubuno-office-diagrams-core`, common/diagrams-core). Design of record:
//! vskubuno `docs/DIAGRAMS-DESKTOP.md`.
//!
//! The library holds everything the Visual Studio designer links to render the views:
//!
//! - `views/` — the window (`diagram_window`) and its dialogs (confirm, prompt, templates);
//! - `pages/` — the user controls: the Backstage's « Informations », the Format panel;
//! - `controls/` — the custom-drawn controls: the canvas, the stencil palette, the layers list, the page bar,
//!   the shape gallery and the status bar's zoom;
//! - `platform/` — the Windows overrides of the common extension points and what the views do not touch:
//!   the Direct2D surface of the shared Canvas 2D context, raster exports through WIC, the clipboard, the
//!   file dialogs, launching another window;
//! - `resources/` — the strings (`resources.kbres`, `resources.fr.kbres`) and the stencils' names.

pub mod controls;
pub mod pages;
pub mod platform;
pub mod views;
mod windows_ui;

pub use controls::diagram_canvas::DiagramCanvas;
pub use controls::layer_list::LayerList;
pub use controls::page_tabs::PageTabs;
pub use controls::shape_gallery::ShapeGallery;
pub use controls::status_zoom::StatusZoom;
pub use controls::stencil_palette::StencilPalette;
pub use pages::backstage_info::BackstageInfo;
pub use pages::format_panel::FormatPanel;
pub use views::diagram_window::{DiagramWindow, Options};
pub use windows_ui::WindowsUi;

// `Resources::app_title()`… — the strings of `resources/resources.kbres` (neutral English) and
// `resources/resources.fr.kbres`, in the current UI culture; `{Res key}` in the views.
kubuno_desktop::resources!(pub Resources, "resources/resources.kbres");
