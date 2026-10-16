//! `ValueList` - the lists of the grid's popups: the « liste » validation's menu (SA:8126-8140, a plain
//! list of buttons) and the column filter's values (`ColumnFilterPopup` SA:556, each with its checkbox).
//! In the light theme these popups use the web's literal colours (white ground, `#f1f3f4` hover,
//! `#1a73e8` checked boxes); in the dark one, the theme's, as the web's dark theme does.

use std::collections::HashSet;

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::graphics::{Brush, Color, Font, FontStyle, Pen, StringAlignment, StringFormat, StringFormatFlags, StringTrimming};
use kubuno_desktop::ui::{Canvas, Rect, Size};
use kubuno_desktop::views::component::{AccessiblePart, Component, Control, ControlCore, EventCx, PaintEventCx};
use kubuno_desktop::views::events::Event;

/// A row was clicked (`index` in [`ValueList::items`]).
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct ValueEventArgs {
    pub index: i32,
    pub value: String,
}

/// The list (see the module doc).
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Spreadsheets")]
#[toolbox(icon = "list")]
#[default_event("ItemClick")]
pub struct ValueList {
    base: ControlCore,
    /// The values, one per line (an empty line is a value: the blanks).
    #[property(bindable, on_change = "items_changed")]
    #[category("Data")]
    pub items: String,
    /// The checked values, one per line (the filter; the owner updates it on `ItemClick`).
    #[property(bindable, on_change = "checked_changed")]
    #[category("Data")]
    pub checked_items: String,
    /// Shows a checkbox before each value (the filter).
    #[property(bindable)]
    #[category("Appearance")]
    pub checkable: bool,
    /// How a blank value reads (« (Vides) »).
    #[property(bindable)]
    #[category("Appearance")]
    pub blank_text: String,
    /// What shows when there is no value (`-`, « Aucune valeur »).
    #[property(bindable)]
    #[category("Appearance")]
    pub empty_text: String,
    /// The row height in DIP.
    #[property(bindable)]
    #[default_value(28.0)]
    #[category("Layout")]
    pub row_height: f32,
    /// A row was clicked.
    #[event]
    #[category("Action")]
    pub item_click: Event<ValueEventArgs>,
    values: Vec<String>,
    /// The checked values (filter mode).
    checked: HashSet<String>,
    hot: Option<usize>,
    scroll: f32,
}

impl Default for ValueList {
    fn default() -> Self {
        Self {
            base: ControlCore::default(),
            items: "Oui\nNon\nPeut-être".into(),
            checked_items: String::new(),
            checkable: false,
            blank_text: "(Vides)".into(),
            empty_text: "-".into(),
            row_height: 28.0,
            item_click: Event::default(),
            values: vec!["Oui".into(), "Non".into(), "Peut-être".into()],
            checked: HashSet::new(),
            hot: None,
            scroll: 0.0,
        }
    }
}

impl ValueList {
    fn items_changed(&mut self) {
        self.values = decode_list(&self.items);
        self.scroll = 0.0;
        self.hot = None;
        self.invalidate();
    }

    fn checked_changed(&mut self) {
        self.checked = decode_list(&self.checked_items).into_iter().collect();
        self.invalidate();
    }

    /// The values shown, set directly (a value may be empty: the blanks).
    pub fn set_values(&mut self, values: Vec<String>) {
        self.items = encode_list(&values);
        self.values = values;
        self.scroll = 0.0;
        self.hot = None;
        self.invalidate();
    }

    /// The values shown.
    pub fn values(&self) -> &[String] {
        &self.values
    }

    /// The checked values.
    pub fn checked(&self) -> &HashSet<String> {
        &self.checked
    }

    pub fn set_checked(&mut self, checked: HashSet<String>) {
        self.checked = checked;
        self.invalidate();
    }

    fn rh(&self) -> f32 {
        self.row_height.max(12.0)
    }

    fn max_scroll(&self) -> f32 {
        (self.values.len() as f32 * self.rh() - self.size().height).max(0.0)
    }

    fn row_at(&self, y: f32) -> Option<usize> {
        let i = ((y + self.scroll) / self.rh()).floor();
        (i >= 0.0 && (i as usize) < self.values.len()).then_some(i as usize)
    }

    fn label<'a>(&'a self, v: &'a str) -> &'a str {
        if v.is_empty() {
            &self.blank_text
        } else {
            v
        }
    }
}

impl Control for ValueList {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, proposed: Size) -> Size {
        Size { width: proposed.width.max(120.0), height: (self.values.len().max(1) as f32 * self.rh()).min(256.0) }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        let b = e.clip_rectangle;
        let g = e.graphics;
        let c: &dyn Canvas = e.graphics;
        let hex = |s: &str| Color::from_hex(s).unwrap_or(Color::BLACK);
        let c0: &dyn Canvas = e.graphics;
        let t = c0.theme();
        let dark = t.mode == kubuno_desktop::ui::ThemeMode::Dark;
        // The web's literal light colours (`bg-white`, `#f1f3f4`, `#202124`), the theme's in the dark.
        let ground = if dark { Color::from(t.card_background) } else { Color::WHITE };
        let hover = if dark { Color::rgba_f(1.0, 1.0, 1.0, 0.08) } else { hex("#f1f3f4") };
        let ink = if dark { Color::from(t.text_primary) } else { hex("#202124") };
        let muted = if dark { Color::from(t.text_tertiary) } else { hex("#80868b") };
        let accent = if dark { Color::from(t.accent) } else { hex("#1a73e8") };
        let edge = if dark { Color::from(t.card_stroke) } else { hex("#bbbbbb") };
        g.fill_rectangle(Brush::solid(ground), b);
        let saved = g.save();
        g.intersect_clip(b);
        let font = Font::with_dip("Segoe UI", if self.checkable { 12.0 } else { 14.0 }, FontStyle::REGULAR);
        let fmt = StringFormat { alignment: StringAlignment::Near, line_alignment: StringAlignment::Center, trimming: StringTrimming::EllipsisCharacter, flags: StringFormatFlags::NO_WRAP };
        if self.values.is_empty() {
            g.draw_string(&self.empty_text, &font, Brush::solid(muted), Rect::new(b.left + 12.0, b.top, b.right - 8.0, b.top + self.rh()), &fmt);
        }
        let rh = self.rh();
        for (i, v) in self.values.iter().enumerate() {
            let top = b.top + i as f32 * rh - self.scroll;
            if top + rh < b.top || top > b.bottom {
                continue;
            }
            let row = Rect::new(b.left, top, b.right, top + rh);
            if self.hot == Some(i) && !self.design_mode() {
                g.fill_rectangle(Brush::solid(hover), row);
            }
            let mut tx = b.left + 12.0;
            if self.checkable {
                let on = self.checked.contains(v);
                let cy = (row.top + row.bottom) / 2.0;
                let bx = Rect::new(tx, cy - 8.0, tx + 16.0, cy + 8.0);
                if on {
                    g.fill_rounded_rectangle(Brush::solid(accent), bx, 3.0);
                    let white = Color::WHITE.to_d2d();
                    c.vector_icon("Check", &Rect::new(bx.left + 2.5, bx.top + 2.5, bx.right - 2.5, bx.bottom - 2.5), 11.0, &white);
                } else {
                    g.draw_rounded_rectangle(&Pen::new(edge, 1.0), Rect::new(bx.left + 0.5, bx.top + 0.5, bx.right - 0.5, bx.bottom - 0.5), 3.0);
                }
                tx += 24.0;
            }
            g.draw_string(self.label(v), &font, Brush::solid(ink), Rect::new(tx, row.top, row.right - 8.0, row.bottom), &fmt);
        }
        // A thin scroll indicator when the list overflows.
        let max = self.max_scroll();
        if max > 0.0 {
            let h = b.bottom - b.top;
            let total = self.values.len() as f32 * rh;
            let th = (h * h / total).max(16.0);
            let ty = b.top + (h - th) * (self.scroll / max);
            g.fill_rounded_rectangle(Brush::solid(edge), Rect::new(b.right - 5.0, ty, b.right - 2.0, ty + th), 1.5);
        }
        g.restore(saved);
        e.raise(self, "OnPaint");
    }

    fn accessible_parts(&self) -> Vec<AccessiblePart> {
        use kubuno_desktop::controls::host::access::AccessRole;
        let rh = self.rh();
        let w = self.size().width;
        self.values
            .iter()
            .enumerate()
            .map(|(i, v)| AccessiblePart { name: self.label(v).to_string(), role: if self.checkable { AccessRole::CheckBox } else { AccessRole::Button }, bounds: Rect::new(0.0, i as f32 * rh - self.scroll, w, (i + 1) as f32 * rh - self.scroll) })
            .collect()
    }

    fn on_mouse_move(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let hot = self.row_at(e.args().y);
        if hot != self.hot {
            self.hot = hot;
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

    fn on_mouse_wheel(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let next = (self.scroll + e.args().delta * self.rh() * 3.0).clamp(0.0, self.max_scroll());
        if next != self.scroll {
            self.scroll = next;
            self.hot = self.row_at(e.args().y);
            self.invalidate();
        }
        e.raise(&*self, "OnMouseWheel");
    }

    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        if e.args().button == MouseButton::Left {
            if let Some(i) = self.row_at(e.args().y) {
                let v = self.values[i].clone();
                self.raise_item_click(ValueEventArgs { index: i as i32, value: v });
            }
        }
        e.raise(&*self, "OnMouseDown");
    }
}

/// A blank value in a list property (a lone blank would otherwise read as « no value »).
const BLANK: &str = "\u{1f}";

/// The values as a list property: one per line, a blank as [`BLANK`].
pub fn encode_list<S: AsRef<str>>(values: &[S]) -> String {
    values.iter().map(|v| if v.as_ref().is_empty() { BLANK } else { v.as_ref() }).collect::<Vec<_>>().join("\n")
}

/// [`encode_list`] read back.
pub fn decode_list(s: &str) -> Vec<String> {
    if s.is_empty() {
        return Vec::new();
    }
    s.split('\n').map(|v| if v == BLANK { String::new() } else { v.to_string() }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lone_blank_survives_the_list_property() {
        assert_eq!(decode_list(&encode_list(&[""])), vec![String::new()]);
        assert_eq!(decode_list(&encode_list(&["", "b"])), vec![String::new(), "b".to_string()]);
        assert!(decode_list(&encode_list::<&str>(&[])).is_empty());
    }
}
