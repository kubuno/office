//! The Windows integration: the overrides of desktop/common's extension points ([`platform`]) — the launch
//! rules of the shell's header and the local clock — and what the views do not touch: the system clipboard
//! (`clipboard`).

pub mod clipboard;

use kubuno_office_projects_common::platform::{Clock, LaunchRules, Platform};
use kubuno_office_projects_core::date::Date;

/// The command line on Windows: a Debug build under a debugger shows the offline sample header unless
/// `--live` is given (`kubuno_desktop_header_data::sample_requested`).
struct WindowsLaunch;

impl LaunchRules for WindowsLaunch {
    fn sample_requested(&self, args: &[String]) -> bool {
        kubuno_desktop_header_data::sample_requested(args)
    }
}

/// The local date (`GetLocalTime`), as the browser's `new Date()` reads it: the today band, the project
/// without a start date.
struct LocalClock;

impl Clock for LocalClock {
    fn today(&self) -> Date {
        // SAFETY: GetLocalTime only writes the structure it returns.
        let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
        Date::new(t.wYear as i32, t.wMonth as u32, t.wDay as u32).unwrap_or_else(|| Date::from_days(kubuno_office_projects_common::platform::now_ms().div_euclid(86_400_000)))
    }
}

/// The portable defaults with what Windows overrides.
pub fn platform() -> Platform {
    Platform { name: "windows", launch: Box::new(WindowsLaunch), clock: Box::new(LocalClock) }
}
