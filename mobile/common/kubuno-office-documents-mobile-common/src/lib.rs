//! Kubuno Documents for mobile — the app, portable (no OS, no window system).
//!
//! The app is a set of `.kbview` screens (`views/`, edited in Visual Studio like every Kubuno view) rendered by the
//! shared Kubuno mobile renderer (`kubuno-mobile-render`, core repository). The Android and iOS entry points
//! (`../android`, `../ios`) hand [`APP`] to the shared shell of their OS.
//!
//! Migration status: the screens are converted; the app's behaviour (its data and server calls) is still to be
//! migrated from the reference Kotlin app that lived in `mobile/` before this layout (kept in the git history).

use kubuno_mobile_render::App;

/// The `doclist.kbview` screen.
pub const DOCLIST: &str = include_str!("views/doclist.kbview");
/// The `actions.kbview` screen.
pub const ACTIONS: &str = include_str!("views/actions.kbview");
/// The `doceditor.kbview` screen.
pub const DOCEDITOR: &str = include_str!("views/doceditor.kbview");
/// The `docreader.kbview` screen.
pub const DOCREADER: &str = include_str!("views/docreader.kbview");
/// The `export.kbview` screen.
pub const EXPORT: &str = include_str!("views/export.kbview");
/// The `share.kbview` screen.
pub const SHARE: &str = include_str!("views/share.kbview");

/// Every screen, in a tap-to-advance order (the home screen first).
pub const VIEWS: &[&str] = &[DOCLIST, ACTIONS, DOCEDITOR, DOCREADER, EXPORT, SHARE];

/// Register what the app's screens need from the renderer (its host controls and images) — nothing yet. The shell
/// calls it once, on its UI thread, before the first frame (through [`APP`]).
pub fn setup() {}

/// The Kubuno Documents app, as the shells run it.
pub const APP: App = App { id: "documents", label: "Kubuno Documents", views: VIEWS, setup };

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_screen_renders() {
        setup();
        for src in VIEWS {
            assert!(kubuno_mobile_render::render_view(src, 1080, 2340, None, None).is_some());
        }
    }
}
