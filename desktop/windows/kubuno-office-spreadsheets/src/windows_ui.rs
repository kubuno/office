//! The Windows user interface of desktop/common's app ([`UiHost`]): the splash screen, the workbook named on
//! the command line, then `Application::run(SpreadsheetWindow)`.

use kubuno_desktop::View;
use kubuno_office_sheets_core::model::workbook::Workbook;
use kubuno_office_spreadsheets_common::app::Options;
use kubuno_office_spreadsheets_common::platform::UiHost;

use crate::SpreadsheetWindow;

/// The Win32 window painted with Direct2D.
#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsUi;

impl UiHost for WindowsUi {
    fn run(&self, options: &Options) -> i32 {
        match run_window(options.clone()) {
            Ok(()) => 0,
            Err(e) => {
                kubuno_desktop::tracing::error!("[spreadsheets] {e:?}");
                1
            }
        }
    }
}

fn run_window(options: Options) -> kubuno_desktop::Result {
    kubuno_desktop::ui::diagnostics::set_display_name("Kubuno Spreadsheets");
    if let Some(culture) = &options.culture {
        kubuno_desktop::resources::set_culture(culture);
    }
    if options.dark {
        kubuno_desktop::Application::set_theme(kubuno_desktop::ui::Theme::dark());
    }

    // The splash screen paints on its own thread while the workbook opens and the window is built
    // (`--no-splash` / KUBUNO_NO_SPLASH=1 turn it off).
    let splash = kubuno_desktop::SplashScreen::new()
        .artwork(kubuno_desktop::Artwork::Documents)
        .product("Kubuno Spreadsheets")
        .version(env!("CARGO_PKG_VERSION"))
        .license(env!("CARGO_PKG_LICENSE"))
        .show();

    splash.step("Ouverture du classeur…", 0.3);
    // A workbook of the server arrives from the session: until then the grid shows an empty sheet, never the
    // built-in sample.
    let workbook = if options.doc.is_some() { Workbook::new_local("", "s1") } else { options.open_workbook() };

    splash.step("Calcul des formules…", 0.6);
    let window = SpreadsheetWindow::new(options, workbook);
    splash.close_when(window.form());
    kubuno_desktop::Application::run(window)
}
