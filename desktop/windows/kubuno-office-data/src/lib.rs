//! Kubuno Data for Windows - the Win32 window of the Office module's Data (BI) report editor, over the
//! portable app of desktop/common (`kubuno-office-data-common`) and the shared engine
//! (`kubuno-office-data-core`, common/data-core: the report document, the data-shaping, the formatting, the
//! palettes, the field wells, the visual catalogue, the in-memory query and the editing operations).
//!
//! The library holds everything the Visual Studio designer links to render the views:
//!
//! - `views/` - the main form [`DataWindow`] with its declarative ribbon;
//! - `controls/` - the custom-drawn [`ReportCanvas`] (the report page and its widgets) and the model they
//!   share (`controls::model`);
//! - `platform/` - the Windows overrides of the common extension points (the clipboard);
//! - `resources/` - the strings and the icon (`resources.kbres`, `resources.fr.kbres`).

pub mod controls;
pub mod platform;
pub mod views;
mod windows_ui;

pub use controls::report_canvas::ReportCanvas;
pub use views::data_window::{DataWindow, Options};
pub use windows_ui::WindowsUi;

// `Resources::app_title()`… - the strings of `resources/resources.kbres` (neutral English) and
// `resources/resources.fr.kbres`, in the current UI culture; `{Res key}` in the views.
kubuno_desktop::resources!(pub Resources, "resources/resources.kbres");
