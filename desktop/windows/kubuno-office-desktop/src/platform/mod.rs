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

/// Today's date in the user's short format (`new Date().toLocaleDateString(lang)` on the web): the
/// `{date}` field of headers and footers.
pub fn today() -> String {
    use windows::Win32::Globalization::{GetDateFormatEx, DATE_SHORTDATE};
    let mut buf = [0u16; 64];
    // `None` locale = the user's default; `None` date = now.
    let n = unsafe { GetDateFormatEx(windows::core::PCWSTR::null(), DATE_SHORTDATE, None, windows::core::PCWSTR::null(), Some(&mut buf), windows::core::PCWSTR::null()) };
    if n <= 1 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..(n as usize - 1)])
}

/// Today in the user's language with the month spelt out (`toLocaleDateString(lang, { day:
/// 'numeric', month: 'long', year: 'numeric' })`: « 7 octobre 2026 »).
pub fn today_long() -> String {
    use windows::Win32::Globalization::{GetDateFormatEx, ENUM_DATE_FORMATS_FLAGS};
    let picture: Vec<u16> = "d MMMM yyyy\0".encode_utf16().collect();
    let mut buf = [0u16; 96];
    let n = unsafe { GetDateFormatEx(windows::core::PCWSTR::null(), ENUM_DATE_FORMATS_FLAGS(0), None, windows::core::PCWSTR(picture.as_ptr()), Some(&mut buf), windows::core::PCWSTR::null()) };
    if n <= 1 {
        return today();
    }
    String::from_utf16_lossy(&buf[..(n as usize - 1)])
}

/// The time now, hours and minutes (`toLocaleTimeString(lang, { hour: '2-digit', minute: '2-digit' })`).
pub fn time_now() -> String {
    use windows::Win32::Globalization::{GetTimeFormatEx, TIME_NOSECONDS};
    let mut buf = [0u16; 64];
    let n = unsafe { GetTimeFormatEx(windows::core::PCWSTR::null(), TIME_NOSECONDS, None, windows::core::PCWSTR::null(), Some(&mut buf)) };
    if n <= 1 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..(n as usize - 1)])
}
