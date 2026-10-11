//! The Windows user interface of desktop/common's app ([`UiHost`]): the splash screen, the board named on
//! the command line, then `Application::run(WhiteboardWindow)`.

use kubuno_desktop::View;
use kubuno_office_whiteboard_common::app::Options;
use kubuno_office_whiteboard_common::platform::UiHost;

use crate::WhiteboardWindow;

/// The Win32 window painted with Direct2D.
#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsUi;

impl UiHost for WindowsUi {
    fn run(&self, options: &Options) -> i32 {
        match run_window(options.clone()) {
            Ok(()) => 0,
            Err(e) => {
                kubuno_desktop::tracing::error!("[whiteboard] {e:?}");
                1
            }
        }
    }
}

fn run_window(options: Options) -> kubuno_desktop::Result {
    kubuno_desktop::ui::diagnostics::set_display_name("Kubuno Whiteboard");
    if let Some(culture) = &options.culture {
        kubuno_desktop::resources::set_culture(culture);
    }
    if options.dark {
        kubuno_desktop::Application::set_theme(kubuno_desktop::ui::Theme::dark());
    }
    let splash = kubuno_desktop::SplashScreen::new()
        .artwork(kubuno_desktop::Artwork::Documents)
        .product("Kubuno Whiteboard")
        .version(env!("CARGO_PKG_VERSION"))
        .license(env!("CARGO_PKG_LICENSE"))
        .show();
    splash.step("Ouverture du tableau…", 0.3);
    let board = options.open_board();
    splash.step("Préparation de l'éditeur…", 0.7);
    let window = WhiteboardWindow::new(options, board);
    splash.close_when(window.form());
    kubuno_desktop::Application::run(window)
}
