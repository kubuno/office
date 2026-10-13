//! The ribbon's "no document open" rule, shared by every Kubuno Office desktop app.
//!
//! Like the web (`office/web/src/ribbon/ModuleBackstage.tsx`: `ModuleHome`), an editor
//! launched with **no file and no `--doc`** — the empty default placeholder — shows ONLY
//! the « Fichier » (backstage) tab and lands on it; the content tabs (Accueil, Insertion,
//! Mise en page…) appear once a real document is open. The web does this by rendering the
//! editor with only the File tab in the ribbon; the desktop ribbon (a port of the web one)
//! reaches the same look through its tabs' `Visible` property, which the view runtime
//! resolves against the window's view model every frame.
//!
//! So each window:
//!   * binds every content tab's `Visible` to [`DocumentTabs::show_content`]
//!     (`Visible="{Binding ShowContentTabs}"` on each `<RibbonTab>` after the backstage),
//!   * binds the ribbon's `SelectedTab` to [`DocumentTabs::active_tab`]
//!     (`SelectedTab="{Binding ActiveRibbonTab}"` on the `<Ribbon>`), and
//!   * recomputes both with [`DocumentTabs::for_document`] at launch, and whenever a
//!     document is opened, created or closed.
//!
//! The « Fichier »/backstage tab is never hidden (it has no `Visible` binding). The
//! `SelectedTab` binding is edge-triggered by the view runtime (it only forces the active
//! tab when the bound value changes), so the user can still switch tabs freely once a
//! document is open.
//!
//! This rule has no UI dependency on purpose: it is the same on every OS, and the portable
//! crate stays free of the Windows framework.

/// The ribbon state driven by whether a document is open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentTabs {
    /// Whether the content tabs (everything after « Fichier ») are shown. `false` leaves
    /// only the backstage tab, as the web's `ModuleHome` does.
    pub show_content: bool,
    /// The `x:Name` of the tab the ribbon makes active: the editing tab when a document is
    /// open, the backstage (« Fichier ») tab otherwise (the web's `defaultTab: 'home'` vs
    /// landing on the File screen).
    pub active_tab: String,
}

impl DocumentTabs {
    /// The state for an editor whose document is open (`has_document`) or not.
    ///
    /// * `editing_tab` — the `x:Name` of the first editing tab to land on with a document
    ///   open (e.g. `"tab_home"`).
    /// * `backstage_tab` — the `x:Name` of the « Fichier » backstage tab to land on with
    ///   none (e.g. `"backstage"`).
    pub fn for_document(has_document: bool, editing_tab: &str, backstage_tab: &str) -> Self {
        Self {
            show_content: has_document,
            active_tab: if has_document { editing_tab } else { backstage_tab }.to_string(),
        }
    }

    /// The empty default state: no document, content tabs hidden, landing on the backstage.
    pub fn empty(editing_tab: &str, backstage_tab: &str) -> Self {
        Self::for_document(false, editing_tab, backstage_tab)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_document_hides_content_and_lands_on_backstage() {
        let s = DocumentTabs::for_document(false, "tab_home", "backstage");
        assert!(!s.show_content);
        assert_eq!(s.active_tab, "backstage");
    }

    #[test]
    fn a_document_shows_content_and_lands_on_the_editing_tab() {
        let s = DocumentTabs::for_document(true, "tab_home", "backstage");
        assert!(s.show_content);
        assert_eq!(s.active_tab, "tab_home");
    }
}
