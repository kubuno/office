//! Kubuno Whiteboard for Windows (`kubuno-whiteboard.exe`) - the app of desktop/common with the Windows
//! platform registered and the Win32 window as its user interface.
//!
//! `kubuno-whiteboard.exe [--dark] [--culture fr|en] [--no-splash] [--zoom 50] [--doc <id> | file.kbwbd]`

// A GUI application: no console window, in Debug too.
#![windows_subsystem = "windows"]

use kubuno_office_whiteboard::WindowsUi;

fn main() {
    let code = kubuno_office_whiteboard_common::app::run(kubuno_office_whiteboard::platform::platform(), &WindowsUi);
    std::process::exit(code);
}
