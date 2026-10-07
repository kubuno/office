//! The Windows user interface of desktop/common's app ([`UiHost`]): the splash screen, the diagram named on
//! the command line, then `Application::run(DiagramWindow)`.

use kubuno_desktop::View;
use kubuno_office_diagrams_common::app::Options;
use kubuno_office_diagrams_common::platform::UiHost;

use crate::DiagramWindow;

/// The Win32 window painted with Direct2D.
#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsUi;

impl UiHost for WindowsUi {
    fn run(&self, options: &Options) -> i32 {
        match run_window(options.clone()) {
            Ok(()) => 0,
            Err(e) => {
                kubuno_desktop::tracing::error!("[diagrams] {e:?}");
                1
            }
        }
    }
}

fn run_window(options: Options) -> kubuno_desktop::Result {
    kubuno_desktop::ui::diagnostics::set_display_name("Kubuno Diagrams");
    if let Some(culture) = &options.culture {
        kubuno_desktop::resources::set_culture(culture);
    }
    if options.dark {
        kubuno_desktop::Application::set_theme(kubuno_desktop::ui::Theme::dark());
    }
    // The Office artwork (the framework has no Diagrams one yet).
    let splash = kubuno_desktop::SplashScreen::new()
        .artwork(kubuno_desktop::Artwork::Documents)
        .product("Kubuno Diagrams")
        .version(env!("CARGO_PKG_VERSION"))
        .license(env!("CARGO_PKG_LICENSE"))
        .show();
    splash.step("Ouverture du diagramme…", 0.3);
    let diagram = options.open_diagram();
    splash.step("Préparation de l'éditeur…", 0.7);
    let window = DiagramWindow::new(options, diagram);
    splash.close_when(window.form());
    kubuno_desktop::Application::run(window)
}
