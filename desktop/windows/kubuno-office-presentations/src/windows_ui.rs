//! The Windows user interface of desktop/common's app ([`UiHost`]): the splash screen, the presentation named on
//! the command line, then `Application::run(PresentationWindow)`.

use kubuno_desktop::View;
use kubuno_office_presentations_common::app::{self, Options};
use kubuno_office_presentations_common::platform::UiHost;
use kubuno_office_slides_core::model::Deck;

use crate::PresentationWindow;

/// The Win32 window painted with Direct2D.
#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsUi;

impl UiHost for WindowsUi {
    fn run(&self, options: &Options) -> i32 {
        match run_window(options.clone()) {
            Ok(()) => 0,
            Err(e) => {
                kubuno_desktop::tracing::error!("[presentations] {e:?}");
                1
            }
        }
    }
}

/// The presentation the window opens with: the file named on the command line, else an empty deck while a
/// server presentation (`--doc`) loads, else the new-presentation sample.
fn first_deck(options: &Options) -> Deck {
    if let Some(path) = &options.file {
        match app::read_file(path) {
            Ok(deck) => return deck,
            Err(e) => kubuno_desktop::tracing::error!("[presentations] {e}"),
        }
    }
    if options.doc.is_some() {
        return Deck::from_server(&serde_json::json!({ "slides": [] }), r#"{"version":1,"slides":{}}"#);
    }
    app::sample_deck()
}

fn run_window(options: Options) -> kubuno_desktop::Result {
    kubuno_desktop::ui::diagnostics::set_display_name("Kubuno Presentations");
    if let Some(culture) = &options.culture {
        kubuno_desktop::resources::set_culture(culture);
    }
    if options.dark {
        kubuno_desktop::Application::set_theme(kubuno_desktop::ui::Theme::dark());
    }
    // The splash paints on its own thread while the presentation opens and the window is built
    // (`--no-splash` / KUBUNO_NO_SPLASH=1 turn it off). The framework has no presentation artwork yet.
    let splash = kubuno_desktop::SplashScreen::new()
        .artwork(kubuno_desktop::Artwork::Kubuno)
        .product("Kubuno Présentations")
        .version(env!("CARGO_PKG_VERSION"))
        .license(env!("CARGO_PKG_LICENSE"))
        .show();
    splash.step("Ouverture de la présentation…", 0.3);
    let deck = first_deck(&options);
    splash.step("Préparation des diapositives…", 0.7);
    let window = PresentationWindow::new(options, deck);
    splash.close_when(window.form());
    kubuno_desktop::Application::run(window)
}
