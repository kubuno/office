//! Kubuno Spreadsheets for Windows - the Win32 window of the Office module's desktop spreadsheet, written
//! like a Windows Forms application, over the portable app of desktop/common
//! (`kubuno-office-spreadsheets-common`: the launch options, the view state, the server session) and the
//! shared engine (`kubuno-office-sheets-core`, common/sheets-core: the workbook, the formulas, the editing
//! commands).
//!
//! The library holds everything the Visual Studio designer links to render the views:
//!
//! - `views/` - the main form [`SpreadsheetWindow`] with its declarative ribbon, and its dialogs;
//! - `pages/` - the Backstage's user control [`BackstageInfo`];
//! - `controls/` - the custom-drawn controls [`GridCanvas`], [`SheetTabs`] and [`StatusZoom`];
//! - `platform/` - the Windows overrides of the common extension points (the local clock, the clipboard);
//! - `resources/` - the strings and the icon (`resources.kbres`, `resources.fr.kbres`).

pub mod controls;
pub mod pages;
pub mod platform;
pub mod views;
mod windows_ui;

pub use controls::equation_view::EquationView;
pub use controls::grid::GridCanvas;
pub use controls::presence_avatars::PresenceAvatars;
pub use controls::sheet_tabs::SheetTabs;
pub use controls::status_zoom::StatusZoom;
pub use controls::swatch::Swatch;
pub use controls::value_list::ValueList;
pub use pages::filter_popup::FilterPopup;
pub use pages::backstage_info::BackstageInfo;
pub use views::spreadsheet_window::{Options, SpreadsheetWindow};
pub use windows_ui::WindowsUi;

// `Resources::app_title()`… - the strings of `resources/resources.kbres` (neutral English) and
// `resources/resources.fr.kbres`, in the current UI culture; `{Res key}` in the views.
kubuno_desktop::resources!(pub Resources, "resources/resources.kbres");
