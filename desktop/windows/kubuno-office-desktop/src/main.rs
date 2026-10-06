//! Kubuno Documents for Windows (`kubuno-documents.exe`) — `Program.cs`: the app of desktop/common with the
//! Windows platform registered and the Win32 window as its user interface ([`WindowsUi`]: the splash screen,
//! the document named on the command line, then `Application::run(DocumentWindow)`).
//!
//! `kubuno-documents.exe [--dark] [--culture fr|en] [--no-splash] [--zoom 50] [--doc <id> | content.json]`

// A GUI application: no console window, in Debug too (like a Windows Forms `WinExe`). Its logs,
// `println!`s and panics go to the debugger's Output window or to %LOCALAPPDATA%\Kubuno\logs.
#![windows_subsystem = "windows"]

use kubuno_office_desktop::WindowsUi;

fn main() {
    let code = kubuno_office_desktop_common::app::run(kubuno_office_desktop::platform::platform(), &WindowsUi);
    std::process::exit(code);
}
