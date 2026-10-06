//! The Windows integration: the overrides of desktop/common's extension points ([`platform`]), and what
//! the views do not touch: `painter` (the Direct2D drawing surface), `clipboard` (the multi-format
//! clipboard), `ime` (where the IME and the system caret think the caret is), `images` (WIC decoding and
//! re-encoding) and `file_dialog` (the system's file dialogs).

pub mod clipboard;
pub mod file_dialog;
pub mod ime;
pub mod images;
pub mod painter;

use kubuno_office_desktop_common::platform::{LaunchRules, Platform};

/// The command line on Windows: a Debug build under a debugger shows the offline sample header unless
/// `--live` is given (`kubuno_desktop_header_data::sample_requested`).
struct WindowsLaunch;

impl LaunchRules for WindowsLaunch {
    fn sample_requested(&self, args: &[String]) -> bool {
        kubuno_desktop_header_data::sample_requested(args)
    }
}

/// The portable defaults with what Windows overrides.
pub fn platform() -> Platform {
    Platform { name: "windows", launch: Box::new(WindowsLaunch) }
}
