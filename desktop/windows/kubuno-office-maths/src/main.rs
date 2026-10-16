//! Kubuno Maths for Windows (`kubuno-maths.exe`) - `Program.cs`: the app of desktop/common with the Windows
//! platform registered and the Win32 window as its user interface.
//!
//! `kubuno-maths.exe [--dark] [--culture fr|en] [--no-splash] [--zoom 100] [--doc <id> | formule.kbmath]`

// A GUI application: no console window, in Debug too.
#![windows_subsystem = "windows"]

use kubuno_office_maths::WindowsUi;

fn main() {
    let code = kubuno_office_maths_common::app::run(kubuno_office_maths::platform::platform(), &WindowsUi);
    std::process::exit(code);
}
