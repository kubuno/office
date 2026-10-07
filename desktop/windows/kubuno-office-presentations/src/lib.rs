//! Kubuno Presentations for Windows — the Win32 window of the office module's presentation editor, written
//! like a Windows Forms application, over the portable app of desktop/common
//! (`kubuno-office-presentations-common`: the launch options, the open presentation's editing state, the
//! server session) and the module's presentation engine (`kubuno-office-slides-core`).
//!
//! The library holds everything the Visual Studio designer links to render the views: the main form
//! [`PresentationWindow`] (`views/presentation_window.kbview` + `.rs`) with its declarative ribbon, the
//! slideshow window, the dialogs, the Backstage's user control [`PresentationInfo`], and the custom controls
//! [`SlideCanvas`] (the editing surface), [`SlidePanel`] (the slide list) and [`SlideShow`] (the slideshow
//! stage). `main.rs` only registers [`platform::platform`] and [`WindowsUi`] and runs the common app.
//!
//! - `views/` — the windows and dialogs, each next to its code-behind;
//! - `pages/` — the Backstage's user control;
//! - `controls/` — the custom-drawn controls;
//! - `platform/` — Direct2D drawing and DirectWrite measuring of the shared renderer, WIC images, exports,
//!   the clipboard and the file dialogs;
//! - `resources/` — the strings and the icon (`resources.kbres`, `resources.fr.kbres`).

pub mod controls;
pub mod model;
pub mod pages;
pub mod platform;
pub mod views;
mod windows_ui;

pub use controls::slide_canvas::SlideCanvas;
pub use controls::slide_panel::SlidePanel;
pub use controls::slide_show::SlideShow;
pub use controls::start_page::StartPage;
pub use pages::presentation_info::PresentationInfo;
pub use views::presentation_window::PresentationWindow;
pub use windows_ui::WindowsUi;

// `Resources::app_title()`… — the strings of `resources/resources.kbres` (neutral English) and
// `resources/resources.fr.kbres`, in the current UI culture; `{Res key}` in the views.
kubuno_desktop::resources!(pub Resources, "resources/resources.kbres");
