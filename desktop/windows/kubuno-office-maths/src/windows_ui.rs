//! The Windows user interface of desktop/common's app ([`UiHost`]): the splash screen, the formula named on
//! the command line, then `Application::run(MathsWindow)`.

use kubuno_desktop::View;
use kubuno_office_maths_common::app::Options;
use kubuno_office_maths_common::platform::UiHost;

use crate::MathsWindow;

/// The Win32 window painted with Direct2D.
#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsUi;

impl UiHost for WindowsUi {
    fn run(&self, options: &Options) -> i32 {
        match run_window(options.clone()) {
            Ok(()) => 0,
            Err(e) => {
                kubuno_desktop::tracing::error!("[maths] {e:?}");
                1
            }
        }
    }
}

fn run_window(options: Options) -> kubuno_desktop::Result {
    kubuno_desktop::ui::diagnostics::set_display_name("Kubuno Maths");
    if let Some(culture) = &options.culture {
        kubuno_desktop::resources::set_culture(culture);
    }
    if options.dark {
        kubuno_desktop::Application::set_theme(kubuno_desktop::ui::Theme::dark());
    }
    // The Office artwork (the framework has no Maths one yet).
    let splash = kubuno_desktop::SplashScreen::new()
        .artwork(kubuno_desktop::Artwork::Documents)
        .product("Kubuno Maths")
        .version(env!("CARGO_PKG_VERSION"))
        .license(env!("CARGO_PKG_LICENSE"))
        .show();
    splash.step("Ouverture de la formule…", 0.3);
    // A formula of the server arrives from the session; until then the sheet is empty, never the sample.
    let document = if options.doc.is_some() { None } else { Some(options.open_document()) };
    splash.step("Préparation de l'éditeur…", 0.7);
    let window = MathsWindow::new(options, document);
    splash.close_when(window.form());
    kubuno_desktop::Application::run(window)
}
