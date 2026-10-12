//! `PropertiesPanel` — the Propriétés panel of the web Whiteboard (`WhiteboardApp.tsx`, the selected-element
//! properties): the selected element's type, position, size, colour, outline and text, each a gray key with
//! its value; a hint when nothing is selected. A custom-drawn control built exactly like `LayerList` (a
//! compact list of rows), so it top-aligns inside the dock panel the way the Calques list does. The window
//! sets the rows from the canvas' selection with [`PropertiesPanel::set_rows`].

use kubuno_desktop::ui::{Canvas, Rect, Size};
use kubuno_desktop::views::component::{AccessiblePart, Control, ControlCore, PaintEventCx};

/// One property row: a key and its value.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PropRow {
    pub key: String,
    pub value: String,
}

impl PropRow {
    pub fn new(key: &str, value: impl Into<String>) -> Self {
        Self { key: key.to_string(), value: value.into() }
    }
}

const HEADER_H: f32 = 30.0;
const ROW_H: f32 = 30.0;
const PAD: f32 = 12.0;
const KEY_W: f32 = 76.0;
const GAP: f32 = 6.0;

fn row_rect(i: usize, w: f32) -> Rect {
    let top = HEADER_H + 1.0 + 6.0 + i as f32 * ROW_H;
    Rect::new(0.0, top, w, top + ROW_H)
}

/// The Propriétés panel (see the module doc).
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Whiteboard")]
#[toolbox(icon = "sliders")]
pub struct PropertiesPanel {
    base: ControlCore,
    /// The rows to show; empty shows the « select an element » hint.
    rows: Vec<PropRow>,
}

#[allow(clippy::derivable_impls)]
impl Default for PropertiesPanel {
    fn default() -> Self {
        Self { base: ControlCore::default(), rows: Vec::new() }
    }
}

impl PropertiesPanel {
    /// Replaces the rows (empty = nothing selected).
    pub fn set_rows(&mut self, rows: Vec<PropRow>) {
        if self.rows != rows {
            self.rows = rows;
            self.invalidate();
        }
    }
}

impl Control for PropertiesPanel {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, _proposed: Size) -> Size {
        Size { width: 240.0, height: HEADER_H + 1.0 + 6.0 + self.rows.len().max(1) as f32 * ROW_H }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        let b = e.clip_rectangle;
        let c: &dyn Canvas = e.graphics;
        let t = c.theme();
        let w = b.right - b.left;
        let at = |r: Rect| Rect::new(b.left + r.left, b.top + r.top, b.left + r.right, b.top + r.bottom);
        c.fill_rounded(&b, 0.0, &t.card_background);
        // Header.
        c.text("Propriétés", &at(Rect::new(PAD, 0.0, w - PAD, HEADER_H)), &c.formats().caption_strong, &t.text_secondary, false);
        c.fill_rounded(&at(Rect::new(0.0, HEADER_H, w, HEADER_H + 1.0)), 0.0, &t.divider);
        if self.rows.is_empty() {
            c.text(
                "Sélectionnez un élément pour voir ses propriétés.",
                &at(Rect::new(PAD, HEADER_H + 10.0, w - PAD, HEADER_H + 10.0 + ROW_H * 2.0)),
                &c.formats().caption_wrap,
                &t.text_tertiary,
                false,
            );
            e.raise(self, "OnPaint");
            return;
        }
        for (i, row) in self.rows.iter().enumerate() {
            let rr = row_rect(i, w);
            let key_r = Rect::new(b.left + PAD, b.top + rr.top, b.left + PAD + KEY_W, b.top + rr.bottom);
            c.text(&row.key, &key_r, &c.formats().caption, &t.text_secondary, false);
            let val_r = Rect::new(b.left + PAD + KEY_W + GAP, b.top + rr.top, b.right - PAD, b.top + rr.bottom);
            c.text_ellipsis(&row.value, &val_r, &c.formats().body, &t.text_primary);
        }
        e.raise(self, "OnPaint");
    }

    fn accessible_parts(&self) -> Vec<AccessiblePart> {
        use kubuno_desktop::controls::host::access::AccessRole;
        let w = self.size().width;
        self.rows
            .iter()
            .enumerate()
            .map(|(i, r)| AccessiblePart { name: format!("{} : {}", r.key, r.value), role: AccessRole::ListItem, bounds: row_rect(i, w) })
            .collect()
    }
}
