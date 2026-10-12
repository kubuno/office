//! `LayerList` — the Calques panel of the web Whiteboard (`WhiteboardApp.tsx`, the Layers/« Calques »
//! panel): the board's elements in paint order (top of the list = front), each a type icon and a name,
//! the selected one highlighted; clicking a row selects that element on the canvas. Empty: « Aucun objet
//! sur le tableau. »

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::{Canvas, Rect, Size};
use kubuno_desktop::views::component::{AccessiblePart, Control, ControlCore, EventCx, PaintEventCx};
use kubuno_desktop::views::events::Event;
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;

fn with_alpha(c: D2D1_COLOR_F, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { a: c.a * a, ..c }
}

/// One element as the Calques list shows it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ElementRow {
    pub id: String,
    /// The element kind (`sticky | text | shape | arrow | frame | image`), for the icon.
    pub kind: String,
    /// The display name (a sticky's / text's text, a shape's kind, « Flèche », « Cadre »…).
    pub name: String,
}

/// The icon of a kind (lucide names, drawn by `vector_icon`).
fn kind_icon(kind: &str) -> &'static str {
    match kind {
        "sticky" => "StickyNote",
        "text" => "Type",
        "shape" => "Square",
        "arrow" => "MoveUpRight",
        "frame" => "Frame",
        "image" => "Image",
        _ => "Square",
    }
}

/// The user clicked a row (its element id).
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct LayerSelectEventArgs {
    pub id: String,
}

const HEADER_H: f32 = 30.0;
const ROW_H: f32 = 28.0;
const PAD: f32 = 8.0;
const ICON_W: f32 = 17.0;
const GAP: f32 = 6.0;

fn header() -> Rect {
    Rect::new(0.0, 0.0, 0.0, HEADER_H)
}

fn row_rect(i: usize, w: f32) -> Rect {
    let top = HEADER_H + 1.0 + i as f32 * ROW_H;
    Rect::new(0.0, top, w, top + ROW_H)
}

/// The Calques list (see the module doc).
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Whiteboard")]
#[toolbox(icon = "layers")]
#[default_event("LayerSelect")]
pub struct LayerList {
    base: ControlCore,
    #[event]
    #[category("Action")]
    pub layer_select: Event<LayerSelectEventArgs>,
    rows: Vec<ElementRow>,
    selected: String,
    hot: Option<usize>,
}

#[allow(clippy::derivable_impls)]
impl Default for LayerList {
    fn default() -> Self {
        Self { base: ControlCore::default(), layer_select: Event::default(), rows: Vec::new(), selected: String::new(), hot: None }
    }
}

impl LayerList {
    /// Replaces the list (front first) and the selected element.
    pub fn set_rows(&mut self, rows: Vec<ElementRow>, selected: String) {
        if self.rows != rows || self.selected != selected {
            self.rows = rows;
            self.selected = selected;
            self.invalidate();
        }
    }

    fn row_at(&self, x: f32, y: f32) -> Option<usize> {
        let w = self.size().width;
        (0..self.rows.len()).find(|&i| row_rect(i, w).contains(x, y))
    }
}

impl Control for LayerList {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, _proposed: Size) -> Size {
        Size { width: 240.0, height: HEADER_H + 1.0 + self.rows.len().max(1) as f32 * ROW_H }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        let b = e.clip_rectangle;
        let c: &dyn Canvas = e.graphics;
        let t = c.theme();
        let w = b.right - b.left;
        let at = |r: Rect| Rect::new(b.left + r.left, b.top + r.top, b.left + r.right, b.top + r.bottom);
        c.fill_rounded(&b, 0.0, &t.card_background);
        // Header.
        let _ = header();
        c.text("Calques", &at(Rect::new(PAD, 0.0, w - PAD, HEADER_H)), &c.formats().caption_strong, &t.text_secondary, false);
        c.fill_rounded(&at(Rect::new(0.0, HEADER_H, w, HEADER_H + 1.0)), 0.0, &t.divider);
        if self.rows.is_empty() {
            c.text("Aucun objet sur le tableau.", &at(Rect::new(PAD, HEADER_H + 10.0, w - PAD, HEADER_H + 10.0 + ROW_H)), &c.formats().caption, &t.text_tertiary, false);
            e.raise(self, "OnPaint");
            return;
        }
        for (i, row) in self.rows.iter().enumerate() {
            let rr = row_rect(i, w);
            let selected = row.id == self.selected;
            if selected {
                c.fill_rounded(&at(rr), 0.0, &with_alpha(t.accent, 0.05));
            } else if self.hot == Some(i) {
                c.fill_rounded(&at(rr), 0.0, &t.row_hover);
            }
            let (icx, icy) = (b.left + PAD + ICON_W / 2.0, b.top + rr.top + ROW_H / 2.0);
            c.vector_icon(kind_icon(&row.kind), &Rect::new(icx - 6.5, icy - 6.5, icx + 6.5, icy + 6.5), 13.0, &t.text_secondary);
            let name_r = Rect::new(b.left + PAD + ICON_W + GAP, b.top + rr.top, b.right - PAD, b.top + rr.bottom);
            c.text_ellipsis(&row.name, &name_r, &c.formats().caption, if selected { &t.accent } else { &t.text_primary });
        }
        e.raise(self, "OnPaint");
    }

    fn accessible_parts(&self) -> Vec<AccessiblePart> {
        use kubuno_desktop::controls::host::access::AccessRole;
        let w = self.size().width;
        self.rows.iter().enumerate().map(|(i, r)| AccessiblePart { name: r.name.clone(), role: AccessRole::ListItem, bounds: row_rect(i, w) }).collect()
    }

    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        if e.args().button == MouseButton::Left {
            if let Some(i) = self.row_at(e.args().x, e.args().y) {
                let id = self.rows[i].id.clone();
                self.raise_layer_select(LayerSelectEventArgs { id });
            }
        }
        e.raise(&*self, "OnMouseDown");
    }

    fn on_mouse_move(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let h = self.row_at(e.args().x, e.args().y);
        if h != self.hot {
            self.hot = h;
            self.invalidate();
        }
        e.raise(&*self, "OnMouseMove");
    }

    fn on_mouse_leave(&mut self, e: &mut EventCx<'_, kubuno_desktop::views::events::EmptyEventArgs>) {
        if self.hot.take().is_some() {
            self.invalidate();
        }
        e.raise(&*self, "OnMouseLeave");
    }
}
