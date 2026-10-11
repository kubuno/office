//! The Windows user interface of desktop/common's app ([`UiHost`]): the splash screen, the report named on
//! the command line (or the sample), then `Application::run(DataWindow)`.

use kubuno_desktop::View;
use kubuno_office_data_common::app::Options;
use kubuno_office_data_common::platform::UiHost;

use crate::DataWindow;

/// The Win32 window painted with Direct2D.
#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsUi;

impl UiHost for WindowsUi {
    fn run(&self, options: &Options) -> i32 {
        match run_window(options.clone()) {
            Ok(()) => 0,
            Err(e) => {
                kubuno_desktop::tracing::error!("[data] {e:?}");
                1
            }
        }
    }
}

fn run_window(options: Options) -> kubuno_desktop::Result {
    kubuno_desktop::ui::diagnostics::set_display_name("Kubuno Data");
    if let Some(culture) = &options.culture {
        kubuno_desktop::resources::set_culture(culture);
    }
    if options.dark {
        kubuno_desktop::Application::set_theme(kubuno_desktop::ui::Theme::dark());
    }
    let splash = kubuno_desktop::SplashScreen::new()
        .artwork(kubuno_desktop::Artwork::Documents)
        .product("Kubuno Data")
        .version(env!("CARGO_PKG_VERSION"))
        .license(env!("CARGO_PKG_LICENSE"))
        .show();
    splash.step("Ouverture du rapport…", 0.4);
    // A server report (`--doc`) is fetched by the window; otherwise the file or the sample opens now.
    let report = if options.doc.is_some() { None } else { Some(options.open_report()) };
    splash.step("Rendu…", 0.7);
    let window = DataWindow::new(options, report);
    splash.close_when(window.form());
    kubuno_desktop::Application::run(window)
}
