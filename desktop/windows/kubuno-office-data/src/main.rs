//! Kubuno Data for Windows (`kubuno-data.exe`) — `Program.cs`: the app of desktop/common with the
//! Windows platform registered and the Win32 window as its user interface.
//!
//! `kubuno-data.exe [--dark] [--culture fr|en] [--no-splash] [--zoom day|week|month] [--view gantt|board|…]
//!  [--doc <id> | projet.kbprj]`

// A GUI application: no console window, in Debug too.
#![windows_subsystem = "windows"]

fn main() {
    let code = kubuno_office_data_common::app::run(kubuno_office_data::platform::platform(), &kubuno_office_data::WindowsUi);
    std::process::exit(code);
}
