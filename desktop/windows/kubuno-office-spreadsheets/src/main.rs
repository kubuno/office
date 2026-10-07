//! Kubuno Spreadsheets for Windows (`kubuno-spreadsheets.exe`) — `Program.cs`: the app of desktop/common with the
//! Windows platform registered and the Win32 window as its user interface.
//!
//! `kubuno-spreadsheets.exe [--dark] [--culture fr|en] [--no-splash] [--zoom 100] [--doc <id> | workbook.kbcal]`

// A GUI application: no console window, in Debug too.
#![windows_subsystem = "windows"]

fn main() {
    let code = kubuno_office_spreadsheets_common::app::run(kubuno_office_spreadsheets::platform::platform(), &kubuno_office_spreadsheets::WindowsUi);
    std::process::exit(code);
}
