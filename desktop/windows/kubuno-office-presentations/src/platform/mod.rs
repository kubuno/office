//! The Windows integration: the overrides of desktop/common's extension points ([`platform`]), and what the
//! views do not touch: `d2d` (the slide renderer's Direct2D surface and DirectWrite measuring), `images` (image
//! bytes and WIC bitmaps), `file_dialog` (the system's file dialogs).

pub mod clipboard;
pub mod d2d;
pub mod export;
pub mod file_dialog;
pub mod images;

use kubuno_office_presentations_common::platform::{LaunchRules, Platform};

/// The command line on Windows: a Debug build under a debugger shows the offline sample header unless
/// `--live` is given (`kubuno_desktop_header_data::sample_requested`).
struct WindowsLaunch;

impl LaunchRules for WindowsLaunch {
    fn sample_requested(&self, args: &[String]) -> bool {
        kubuno_desktop_header_data::sample_requested(args)
    }
}

/// Today in the fr-FR form of `toLocaleDateString` (`07/10/2026`), from the local clock.
pub fn local_date() -> String {
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    format!("{:02}/{:02}/{}", t.wDay, t.wMonth, t.wYear)
}

/// The time like `toLocaleTimeString('fr-FR', { hour: '2-digit', minute: '2-digit' })` (`09:05`).
pub fn local_time() -> String {
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    format!("{:02}:{:02}", t.wHour, t.wMinute)
}

/// The portable defaults with what Windows overrides.
pub fn platform() -> Platform {
    Platform { name: "windows", launch: Box::new(WindowsLaunch) }
}
