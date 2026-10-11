//! The Windows integration: the override of desktop/common's launch rules.

use kubuno_office_maths_common::platform::{LaunchRules, Platform, Portable};

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
    Platform { name: "windows", launch: Box::new(WindowsLaunch), clock: Box::new(Portable) }
}
