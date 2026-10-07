//! The Windows integration: the overrides of desktop/common's extension points ([`platform`]) and what the
//! views do not touch: `surface` (the Direct2D surface of the shared Canvas 2D context), `raster` (exports
//! through WIC), `clipboard`, `file_dialog` and `launch` (another window of the app).

pub mod clipboard;
pub mod file_dialog;
pub mod launch;
pub mod print;
pub mod raster;
pub mod surface;

use kubuno_office_diagrams_common::platform::{LaunchRules, Platform};

/// The command line on Windows: a Debug build under a debugger shows the offline sample header unless
/// `--live` is given.
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
