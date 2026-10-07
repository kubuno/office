//! `StencilPalette` — the Formes panel (`DiagramEditorPage.tsx:3170-3271`): a search box, the categories as
//! an accordion (the first one open), a 5-column grid of 38×30 thumbnails drawn by the shared engine, the
//! armed stencil highlighted, a large hover preview, and a hint while a stencil is armed. A click arms the
//! stencil; a drag drops it on the canvas.

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::{Canvas, Size};
use kubuno_desktop::views::component::{Component, Control, ControlCore, PaintEventCx};
use kubuno_desktop::views::events::Event;

/// A stencil of the palette was clicked.
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct StencilClickedEventArgs {
    pub id: String,
}

/// The stencil palette (see the module doc).
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Diagrams")]
#[toolbox(icon = "shapes")]
#[default_event("StencilClicked")]
pub struct StencilPalette {
    base: ControlCore,
    /// The armed stencil (highlighted), empty for none.
    #[property(bindable)]
    #[category("Data")]
    pub armed: String,
    #[event]
    #[category("Action")]
    pub stencil_clicked: Event<StencilClickedEventArgs>,
}

impl Default for StencilPalette {
    fn default() -> Self {
        Self { base: ControlCore::default(), armed: String::new(), stencil_clicked: Event::default() }
    }
}

impl Control for StencilPalette {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, _proposed: Size) -> Size {
        Size { width: 250.0, height: 600.0 }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        e.raise(self, "OnPaint");
    }
}
