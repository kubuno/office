//! `LayerList` — the Calques panel (`DiagramEditorPage.tsx:3272-3316`): the layers top first, the active one
//! highlighted, each with its visibility and lock toggles, its name (double click to rename), up / down and
//! delete; « + » adds a layer; « Déplacer la sélection ici » when something is selected.

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::{Canvas, Size};
use kubuno_desktop::views::component::{Component, Control, ControlCore, PaintEventCx};
use kubuno_desktop::views::events::Event;

/// A layer as the list shows it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LayerRow {
    pub id: String,
    pub name: String,
    pub visible: bool,
    pub locked: bool,
}

/// What the user asked of a layer: `activate`, `add`, `delete`, `visible`, `lock`, `up`, `down`, `rename`,
/// `move_here`.
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct LayersCommandEventArgs {
    pub command: String,
    pub id: String,
    pub name: String,
}

/// The layers list (see the module doc).
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Diagrams")]
#[toolbox(icon = "layers")]
#[default_event("LayersCommand")]
pub struct LayerList {
    base: ControlCore,
    #[event]
    #[category("Action")]
    pub layers_command: Event<LayersCommandEventArgs>,
    layers: Vec<LayerRow>,
    active: String,
    has_selection: bool,
}

impl Default for LayerList {
    fn default() -> Self {
        Self {
            base: ControlCore::default(),
            layers_command: Event::default(),
            layers: vec![LayerRow { id: "default".into(), name: "Calque 1".into(), visible: true, locked: false }],
            active: "default".into(),
            has_selection: false,
        }
    }
}

impl LayerList {
    pub fn set_layers(&mut self, layers: Vec<LayerRow>, active: String, has_selection: bool) {
        if self.layers != layers || self.active != active || self.has_selection != has_selection {
            self.layers = layers;
            self.active = active;
            self.has_selection = has_selection;
            self.invalidate();
        }
    }
}

impl Control for LayerList {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, _proposed: Size) -> Size {
        Size { width: 240.0, height: 200.0 }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        e.raise(self, "OnPaint");
    }
}
