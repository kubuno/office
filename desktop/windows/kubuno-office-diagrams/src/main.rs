//! Kubuno Diagrams for Windows (`kubuno-diagrams.exe`) — `Program.cs`: the app of desktop/common with the
//! Windows platform registered and the Win32 window as its user interface.
//!
//! `kubuno-diagrams.exe [--dark] [--culture fr|en] [--no-splash] [--zoom 50] [--doc <id> | file.kbdia]`

// A GUI application: no console window, in Debug too.
#![windows_subsystem = "windows"]

use kubuno_office_diagrams::WindowsUi;

fn main() {
    let code = kubuno_office_diagrams_common::app::run(kubuno_office_diagrams::platform::platform(), &WindowsUi);
    std::process::exit(code);
}
