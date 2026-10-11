//! `BoardCanvas` — the infinite board being viewed: the web editor's `<canvas>`
//! (`WhiteboardApp.tsx`), painted read-only with the shared renderer
//! (`kubuno_office_whiteboard_core::render`) over the Direct2D surface of `crate::platform::surface`.
//!
//! Increment A: it opens a board (local `.kbwbd` or a server board filled by the collaboration room) and
//! paints the scene — the background, the elements in z-order, the freehand strokes — with pan (left drag)
//! and zoom (wheel at the pointer). Editing and the live room come in increments B and C.

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::{Canvas, Rect, Size};
use kubuno_desktop::views::component::{Component, Control, ControlCore, EventCx, PaintEventCx};
use kubuno_desktop::views::events::Event;
use kubuno_office_whiteboard_common::doc::OpenBoard;
use kubuno_office_whiteboard_core::render::{self, Surface as _};
use windows_numerics::Matrix3x2;

use crate::platform::surface::{D2DSurface, SurfaceCache};

/// What the ribbon, the backstage and the status bar show of the board (raised as `StateChanged` when it
/// differs).
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct BoardStateEventArgs {
    pub elements: usize,
    pub strokes: usize,
    pub zoom: f32,
    pub background: String,
}

/// The board canvas (see the module doc).
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Whiteboard")]
#[toolbox(icon = "pen-tool")]
#[default_event("StateChanged")]
pub struct BoardCanvas {
    base: ControlCore,
    /// Occurs when what the ribbon, the backstage and the status bar show changes.
    #[event]
    #[category("Action")]
    pub state_changed: Event<BoardStateEventArgs>,
    board: OpenBoard,
    cache: SurfaceCache,
    bounds: Rect,
    reported: Option<BoardStateEventArgs>,
    /// A left-drag pan in progress: the last pointer position (window DIPs).
    panning: Option<(f32, f32)>,
}

impl Default for BoardCanvas {
    fn default() -> Self {
        Self {
            base: ControlCore::default(),
            state_changed: Event::default(),
            board: OpenBoard::sample(),
            cache: SurfaceCache::default(),
            bounds: Rect::new(0.0, 0.0, 800.0, 600.0),
            reported: None,
            panning: None,
        }
    }
}

impl BoardCanvas {
    /// Shows `board` (the view resets to its viewport).
    pub fn open(&mut self, board: OpenBoard) {
        self.board = board;
        self.changed();
    }

    /// The open board, for the window (counts, title, the document).
    pub fn board(&self) -> &OpenBoard {
        &self.board
    }

    pub fn board_mut(&mut self) -> &mut OpenBoard {
        &mut self.board
    }

    /// Repaints after an external change (a room update applied to the document).
    pub fn refresh(&mut self) {
        self.changed();
    }

    /// Sets the canvas background (`white | grid | dots | lines`) and repaints.
    pub fn set_background(&mut self, bg: &str) {
        self.board.background = bg.to_string();
        self.changed();
    }

    pub fn zoom_in(&mut self) {
        let (w, h) = self.size();
        self.board.viewport.zoom_at(1.1, w / 2.0, h / 2.0);
        self.changed();
    }
    pub fn zoom_out(&mut self) {
        let (w, h) = self.size();
        self.board.viewport.zoom_at(1.0 / 1.1, w / 2.0, h / 2.0);
        self.changed();
    }
    pub fn zoom_reset(&mut self) {
        self.board.viewport.set_zoom(1.0);
        self.changed();
    }
    pub fn zoom_fit(&mut self) {
        let (w, h) = self.size();
        let scene = self.board.scene();
        self.board.viewport.fit_to_elements(&scene.elements, w, h);
        self.changed();
    }
    pub fn set_zoom_percent(&mut self, percent: f32) {
        self.board.viewport.set_zoom(percent as f64 / 100.0);
        self.changed();
    }

    fn changed(&mut self) {
        self.invalidate();
        kubuno_desktop::controls::host::request_repaint_after(0);
    }

    fn size(&self) -> (f64, f64) {
        (((self.bounds.right - self.bounds.left).max(1.0)) as f64, ((self.bounds.bottom - self.bounds.top).max(1.0)) as f64)
    }

    fn state_args(&self) -> BoardStateEventArgs {
        let scene = self.board.scene();
        BoardStateEventArgs { elements: scene.elements.len(), strokes: scene.strokes.len(), zoom: self.board.viewport.zoom_percent() as f32, background: self.board.background.clone() }
    }

    fn paint_scene(&mut self, c: &dyn kubuno_desktop::controls::ControlCanvas) {
        let Some(renderer) = c.renderer() else { return };
        let base = Matrix3x2 { M11: 1.0, M12: 0.0, M21: 0.0, M22: 1.0, M31: self.bounds.left, M32: self.bounds.top };
        let Ok(mut surface) = D2DSurface::new(&renderer.d2d_context, &renderer.d2d_factory, &renderer.dwrite, &self.cache, base) else { return };
        let (w, h) = self.size();
        let scene = self.board.scene();
        let vp = self.board.viewport;
        c.push_clip(&self.bounds);
        render::render_background(&mut surface, &scene.background, &vp, w, h);
        surface.set_transform(vp.scale(), 0.0, 0.0, vp.scale(), vp.tx(), vp.ty());
        for e in scene.z_ordered() {
            render::render_element(&mut surface, e, &scene.elements, vp.scale());
        }
        for s in &scene.strokes {
            render::render_stroke(&mut surface, s);
        }
        c.pop_clip();
    }
}

impl Control for BoardCanvas {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, _proposed: Size) -> Size {
        Size { width: 800.0, height: 600.0 }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        self.bounds = e.clip_rectangle;
        let c = e.canvas();
        self.paint_scene(c);
        if !self.design_mode() {
            let args = self.state_args();
            if self.reported.as_ref() != Some(&args) {
                self.reported = Some(args.clone());
                self.raise_state_changed(args);
                kubuno_desktop::controls::host::request_repaint_after(0);
            }
        }
        e.raise(self, "OnPaint");
    }

    fn on_mouse_wheel(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y, notches) = (e.args().x as f64, e.args().y as f64, e.args().delta as f64);
        let (lx, ly) = (x - self.bounds.left as f64, y - self.bounds.top as f64);
        let factor = 1.1f64.powf(notches);
        self.board.viewport.zoom_at(factor, lx, ly);
        self.changed();
        e.raise(&*self, "OnMouseWheel");
    }

    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        self.focus();
        if e.args().button == MouseButton::Left {
            self.panning = Some((e.args().x, e.args().y));
        }
        e.raise(&*self, "OnMouseDown");
    }

    fn on_mouse_move(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        if let Some((lx, ly)) = self.panning {
            if e.args().button == MouseButton::Left {
                let (x, y) = (e.args().x, e.args().y);
                self.board.viewport.pan((x - lx) as f64, (y - ly) as f64);
                self.panning = Some((x, y));
                self.changed();
            } else {
                self.panning = None;
            }
        }
        e.raise(&*self, "OnMouseMove");
    }

    fn on_mouse_up(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        self.panning = None;
        e.raise(&*self, "OnMouseUp");
    }

    fn cursor_at(&self, _x: f32, _y: f32) -> Option<kubuno_desktop::controls::host::Cursor> {
        Some(kubuno_desktop::controls::host::Cursor::Hand)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sample_reports_its_counts() {
        let c = BoardCanvas::default();
        let s = c.state_args();
        assert_eq!(s.elements, 2);
        assert_eq!(s.zoom, 100.0);
    }
}
