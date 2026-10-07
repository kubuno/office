//! `StatusZoom` — the status bar's zoom cluster (`shell/StatusBar.tsx:48-69`): « − », a slider over
//! 50–200 % (step 10), « + » (±10 points, clamped to 25–300 % before the editor clamps to 10–400 %) and the
//! percentage, which resets to 100 %.

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::{Canvas, Size};
use kubuno_desktop::views::component::{Component, Control, ControlCore, PaintEventCx};
use kubuno_desktop::views::events::Event;

/// The status bar's zoom (see the module doc).
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Diagrams")]
#[toolbox(icon = "zoom-in")]
#[default_event("ZoomChanged")]
pub struct StatusZoom {
    base: ControlCore,
    /// The zoom, in percent.
    #[property(bindable)]
    #[category("Data")]
    pub zoom: f32,
    #[event]
    #[category("Action")]
    pub zoom_changed: Event<NumericValueChangedEventArgs>,
}

impl Default for StatusZoom {
    fn default() -> Self {
        Self { base: ControlCore::default(), zoom: 100.0, zoom_changed: Event::default() }
    }
}

impl Control for StatusZoom {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, _proposed: Size) -> Size {
        Size { width: 230.0, height: 28.0 }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        e.raise(self, "OnPaint");
    }
}
