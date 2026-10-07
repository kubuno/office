//! The Windows integration: the overrides of desktop/common's extension points ([`platform`]) — the
//! launch rules of the shell's header and the local clock — and what the views do not touch: the system
//! clipboard (`clipboard`).

pub mod clipboard;

use kubuno_office_sheets_core::jsdate::LocalNow;
use kubuno_office_spreadsheets_common::platform::{Clock, LaunchRules, Platform};

/// The command line on Windows: a Debug build under a debugger shows the offline sample header unless
/// `--live` is given (`kubuno_desktop_header_data::sample_requested`).
struct WindowsLaunch;

impl LaunchRules for WindowsLaunch {
    fn sample_requested(&self, args: &[String]) -> bool {
        kubuno_desktop_header_data::sample_requested(args)
    }
}

/// The local wall clock (`GetLocalTime`), as the browser's `new Date()` reads it.
struct LocalClock;

impl Clock for LocalClock {
    fn now(&self) -> LocalNow {
        // SAFETY: GetLocalTime only writes the structure it returns.
        let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
        LocalNow {
            year: t.wYear as i32,
            month: t.wMonth as u32,
            day: t.wDay as u32,
            hour: t.wHour as u32,
            minute: t.wMinute as u32,
            second: t.wSecond as u32,
            millis: t.wMilliseconds as u32,
        }
    }
}

/// The portable defaults with what Windows overrides.
pub fn platform() -> Platform {
    Platform { name: "windows", launch: Box::new(WindowsLaunch), clock: Box::new(LocalClock) }
}

/// « Plein écran » (`requestFullscreen` on the web): the window of this thread maximized, or restored.
pub fn set_maximized(on: bool) {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow;
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_MAXIMIZE, SW_RESTORE};
    // SAFETY: plain Win32 calls on the calling (UI) thread's active window; a null window does nothing.
    unsafe {
        let hwnd = GetActiveWindow();
        if !hwnd.is_invalid() {
            let _ = ShowWindow(hwnd, if on { SW_MAXIMIZE } else { SW_RESTORE });
        }
    }
}
