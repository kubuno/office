//! `PageTabs` — the page bar under the canvas (`DiagramEditorPage.tsx:3856-3925`): one tab per page (the
//! active one underlined in the accent), « × » on hover to delete (more than one page), a double click to
//! rename, « + » to add one, and « Macros » on the right.

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::{Canvas, Size};
use kubuno_desktop::views::component::{Component, Control, ControlCore, PaintEventCx};
use kubuno_desktop::views::events::Event;

/// A page of the bar: its index and name.
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct PageTabEventArgs {
    pub index: usize,
    pub name: String,
}

/// The page bar (see the module doc).
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Diagrams")]
#[toolbox(icon = "panel-bottom")]
#[default_event("PageSelected")]
pub struct PageTabs {
    base: ControlCore,
    #[event]
    #[category("Action")]
    pub page_selected: Event<PageTabEventArgs>,
    #[event]
    #[category("Action")]
    pub page_added: Event<kubuno_desktop::views::events::EmptyEventArgs>,
    #[event]
    #[category("Action")]
    pub page_renamed: Event<PageTabEventArgs>,
    #[event]
    #[category("Action")]
    pub page_deleted: Event<PageTabEventArgs>,
    pages: Vec<String>,
    active: usize,
}

impl Default for PageTabs {
    fn default() -> Self {
        Self {
            base: ControlCore::default(),
            page_selected: Event::default(),
            page_added: Event::default(),
            page_renamed: Event::default(),
            page_deleted: Event::default(),
            pages: vec!["Page 1".into()],
            active: 0,
        }
    }
}

impl PageTabs {
    pub fn set_pages(&mut self, pages: Vec<String>, active: usize) {
        if self.pages != pages || self.active != active {
            self.pages = pages;
            self.active = active;
            self.invalidate();
        }
    }
}

impl Control for PageTabs {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, _proposed: Size) -> Size {
        Size { width: 400.0, height: 36.0 }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        e.raise(self, "OnPaint");
    }
}
