//! Kubuno Maths for Windows - the Win32 window of the Office module's Maths workbench, written like a Windows
//! Forms application, over the portable app of desktop/common (`kubuno-office-maths-common`) and the shared
//! math core (`kubuno-office-math-core`: LaTeX typesetting and the workbench engine). Design of record:
//! vskubuno `docs/MATHS-DESKTOP.md`.
//!
//! The library holds everything the Visual Studio designer links to render the view:
//!
//! - `views/` - the main form [`MathsWindow`] with its declarative ribbon;
//! - `controls/` - the custom-drawn [`MathsCanvas`] (the sheet of blocks), and the vendored math-rendering
//!   helper (`equations`) that paints a laid-out LaTeX formula to the framework's Graphics;
//! - `platform/` - the Windows override of the common launch rules;
//! - `resources/` - the strings and the icon (`resources.kbres`, `resources.fr.kbres`).

pub mod controls;
pub mod platform;
pub mod views;
mod windows_ui;

pub use controls::maths_canvas::{MathsCanvas, MathsStateEventArgs};
pub use controls::panels::{ObjectsPanel, PanelCmdEventArgs, PropertiesPanel, StructuresBar, SymbolsPanel};
pub use views::maths_window::{MathsWindow, Options};
pub use windows_ui::WindowsUi;

// `Resources::app_title()`… - the strings of `resources/resources.kbres` (neutral English) and
// `resources/resources.fr.kbres`, in the current UI culture; `{Res key}` in the view.
kubuno_desktop::resources!(pub Resources, "resources/resources.kbres");
