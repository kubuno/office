//! Kubuno Projects for Windows (`kubuno-projects.exe`) - `Program.cs`: the app of desktop/common with the
//! Windows platform registered and the Win32 window as its user interface.
//!
//! `kubuno-projects.exe [--dark] [--culture fr|en] [--no-splash] [--zoom day|week|month] [--view gantt|board|…]
//!  [--doc <id> | projet.kbprj]`

// A GUI application: no console window, in Debug too.
#![windows_subsystem = "windows"]

fn main() {
    let code = kubuno_office_projects_common::app::run(kubuno_office_projects::platform::platform(), &kubuno_office_projects::WindowsUi);
    std::process::exit(code);
}
