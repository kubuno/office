//! The Windows integration: the overrides of desktop/common's extension points ([`platform`]) — the launch
//! rules of the shell's header — and what the views do not touch: the system clipboard (`clipboard`).

pub mod clipboard;

use kubuno_office_data_common::platform::{LaunchRules, Platform, Portable};

/// The command line on Windows: a Debug build under a debugger shows the offline sample header unless
/// `--live` is given (`kubuno_desktop_header_data::sample_requested`).
struct WindowsLaunch;

impl LaunchRules for WindowsLaunch {
    fn sample_requested(&self, args: &[String]) -> bool {
        kubuno_desktop_header_data::sample_requested(args)
    }
}

/// The portable defaults with what Windows overrides. The clock is the portable one (Unix ms); Data has no
/// calendar of its own.
pub fn platform() -> Platform {
    Platform { name: "windows", launch: Box::new(WindowsLaunch), clock: Box::new(Portable) }
}
