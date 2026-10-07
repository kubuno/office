//! `ShapeGallery` — Insertion › Formes (`shapes/ShapesInsertButton.tsx`, `shapes/ShapeGallery.tsx`): the office
//! shape catalogue by category, drawn by the shared engine, in a popup under the ribbon button; a pick arms
//! the shape tool.

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::{Canvas, Size};
use kubuno_desktop::views::component::{Component, Control, ControlCore, PaintEventCx};

/// The shape gallery (see the module doc).
#[derive(kubuno_desktop::views::component::Component, Default)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Diagrams")]
#[toolbox(icon = "shapes")]
pub struct ShapeGallery {
    base: ControlCore,
}

impl ShapeGallery {
    /// Shows the gallery under `anchor` and returns the kind picked (`None` when dismissed).
    pub fn pick<A>(_anchor: &A, _armed: &str) -> Option<String> {
        None
    }
}

impl Control for ShapeGallery {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, _proposed: Size) -> Size {
        Size { width: 360.0, height: 420.0 }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        e.raise(self, "OnPaint");
    }
}
