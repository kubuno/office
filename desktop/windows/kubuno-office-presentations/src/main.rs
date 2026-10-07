//! Kubuno Presentations for Windows (`kubuno-presentations.exe`) — `Program.cs`: the app of desktop/common with
//! the Windows platform registered and the Win32 window as its user interface ([`WindowsUi`]).
//!
//! `kubuno-presentations.exe [--dark] [--culture fr|en] [--no-splash] [--zoom 100] [--slide N] [--show] [--doc <id> | file.kbsld]`

// A GUI application: no console window (like a Windows Forms `WinExe`).
#![windows_subsystem = "windows"]

use kubuno_office_presentations::WindowsUi;

fn main() {
    let code = kubuno_office_presentations_common::app::run(kubuno_office_presentations::platform::platform(), &WindowsUi);
    std::process::exit(code);
}
