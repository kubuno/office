//! The Windows user interface of desktop/common's app ([`UiHost`]): the splash screen, the project named on
//! the command line, then `Application::run(ProjectWindow)`.

use kubuno_desktop::View;
use kubuno_office_projects_common::app::Options;
use kubuno_office_projects_common::platform::UiHost;

use crate::ProjectWindow;

/// The Win32 window painted with Direct2D.
#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsUi;

impl UiHost for WindowsUi {
    fn run(&self, options: &Options) -> i32 {
        match run_window(options.clone()) {
            Ok(()) => 0,
            Err(e) => {
                kubuno_desktop::tracing::error!("[projects] {e:?}");
                1
            }
        }
    }
}

fn run_window(options: Options) -> kubuno_desktop::Result {
    kubuno_desktop::ui::diagnostics::set_display_name("Kubuno Projects");
    if let Some(culture) = &options.culture {
        kubuno_desktop::resources::set_culture(culture);
    }
    if options.dark {
        kubuno_desktop::Application::set_theme(kubuno_desktop::ui::Theme::dark());
    }
    // The splash screen paints on its own thread while the project opens and the window is built
    // (`--no-splash` / KUBUNO_NO_SPLASH=1 turn it off).
    let splash = kubuno_desktop::SplashScreen::new()
        .artwork(kubuno_desktop::Artwork::Documents)
        .product("Kubuno Projects")
        .version(env!("CARGO_PKG_VERSION"))
        .license(env!("CARGO_PKG_LICENSE"))
        .show();
    splash.step("Ouverture du projet…", 0.4);
    // A project of the server arrives from the session (or its journal): until then the plan is empty, never
    // the built-in sample.
    let project = if options.doc.is_some() { None } else { Some(options.open_project()) };
    splash.step("Planification…", 0.7);
    let window = ProjectWindow::new(options, project);
    splash.close_when(window.form());
    kubuno_desktop::Application::run(window)
}
