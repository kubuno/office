//! The Windows user interface of desktop/common's app ([`UiHost`]): the splash screen, the document named on
//! the command line, then `Application::run(DocumentWindow)`.

use kubuno_desktop::View;
use kubuno_office_desktop_common::app::Options;
use kubuno_office_desktop_common::platform::UiHost;

use crate::DocumentWindow;

/// The Win32 window painted with Direct2D.
#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsUi;

impl UiHost for WindowsUi {
    fn run(&self, options: &Options) -> i32 {
        match run_window(options.clone()) {
            Ok(()) => 0,
            Err(e) => {
                kubuno_desktop::tracing::error!("[documents] {e:?}");
                1
            }
        }
    }
}

fn run_window(options: Options) -> kubuno_desktop::Result {
    kubuno_desktop::ui::diagnostics::set_display_name("Kubuno Documents");
    if let Some(culture) = &options.culture {
        kubuno_desktop::resources::set_culture(culture);
    }
    if options.dark {
        kubuno_desktop::Application::set_theme(kubuno_desktop::ui::Theme::dark());
    }

    // The splash screen, first of all: it paints on its own thread while the document opens and
    // the window is built, and fades out once the window is on screen (`--no-splash` /
    // KUBUNO_NO_SPLASH=1 turn it off).
    let splash = kubuno_desktop::SplashScreen::new()
        .artwork(kubuno_desktop::Artwork::Documents)
        .product("Kubuno Documents")
        .version(env!("CARGO_PKG_VERSION"))
        .license(env!("CARGO_PKG_LICENSE"))
        .show();

    // A `content_json` file given on the command line opens instead of the built-in sample.
    splash.step("Ouverture du document…", 0.25);
    let document = options.open_document();

    // The window: the ribbon, the fonts of the document, the first layout of its pages.
    splash.step("Chargement des polices et mise en page…", 0.6);
    let window = DocumentWindow::new(options, document);
    splash.close_when(window.form());
    kubuno_desktop::Application::run(window)
}
