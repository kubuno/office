//! Kubuno Whiteboard for Windows — the Win32 window of the Office module's infinite-canvas board editor,
//! written like a Windows Forms application, over the portable app of desktop/common
//! (`kubuno-office-whiteboard-common`) and the shared whiteboard engine (`kubuno-office-whiteboard-core`,
//! common/whiteboard-core). Design of record: vskubuno `docs/WHITEBOARD-DESKTOP.md`.
//!
//! The library holds everything the Visual Studio designer links to render the views:
//!
//! - `views/` — the window (`whiteboard_window`);
//! - `pages/` — the user controls: the Backstage's « Informations »;
//! - `controls/` — the custom-drawn controls: the board canvas and the status bar's zoom;
//! - `platform/` — the Windows overrides of the common extension points and the Direct2D surface the board's
//!   renderer draws on;
//! - `resources/` — the strings (`resources.kbres`, `resources.fr.kbres`).

pub mod controls;
pub mod pages;
pub mod platform;
pub mod views;
mod windows_ui;

pub use controls::board_canvas::BoardCanvas;
pub use controls::status_zoom::StatusZoom;
pub use pages::backstage_info::BackstageInfo;
pub use views::whiteboard_window::{Options, WhiteboardWindow};
pub use windows_ui::WindowsUi;

// `Resources::app_title()`… — the strings of `resources/resources.kbres` (neutral English) and
// `resources/resources.fr.kbres`, in the current UI culture; `{Res key}` in the views.
kubuno_desktop::resources!(pub Resources, "resources/resources.kbres");
