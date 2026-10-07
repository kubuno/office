//! The two small lists of the « Informations » panel, custom-drawn because their rows carry their own
//! controls: [`CheckList`] (the project's resources, a check box each: assigned to the task or not —
//! `TaskDetailPanel`'s resources) and [`PredecessorList`] (the task's predecessors: « n · name », the link
//! type, the lag, delete — `ProjectEditorPage.tsx:1945-1957`).

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::graphics::{Brush, Color, Font, FontStyle, Pen, StringAlignment, StringFormat, StringFormatFlags, StringTrimming};
use kubuno_desktop::ui::{Canvas, Rect, Size};
use kubuno_desktop::views::component::{AccessiblePart, Control, ControlCore, EventCx, PaintEventCx};
use kubuno_desktop::views::events::Event;

use crate::controls::model::Palette;

const ROW: f32 = 28.0;

fn palette() -> Palette {
    Palette::of(kubuno_desktop::Application::theme().mode == kubuno_desktop::ui::ThemeMode::Dark)
}

fn one(a: StringAlignment) -> StringFormat {
    StringFormat { alignment: a, line_alignment: StringAlignment::Center, trimming: StringTrimming::EllipsisCharacter, flags: StringFormatFlags::NO_WRAP }
}

/// A row of a list was acted on: `action` (`toggle`, `type`, `lag`, `delete`), the row's `key`, and where.
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct ListActionEventArgs {
    pub action: String,
    pub key: String,
    pub x: f32,
    pub y: f32,
}

/// Rows `key|label|color|checked` (one per line); a click toggles.
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Projects")]
#[toolbox(icon = "list")]
#[default_event("Action")]
pub struct CheckList {
    base: ControlCore,
    #[property(bindable)]
    #[category("Data")]
    pub items: String,
    /// Shown when there is no row.
    #[property(bindable)]
    #[category("Appearance")]
    pub empty_text: String,
    #[event]
    #[category("Action")]
    pub action: Event<ListActionEventArgs>,
}

impl Default for CheckList {
    fn default() -> Self {
        Self { base: ControlCore::default(), items: "r1|Alice Martin|#1a73e8|1\nr2|Bruno Diallo|#188038|0".into(), empty_text: String::new(), action: Event::default() }
    }
}

impl CheckList {
    fn rows(&self) -> Vec<(String, String, String, bool)> {
        self.items
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| {
                let p: Vec<&str> = l.splitn(4, '|').collect();
                (p.first().unwrap_or(&"").to_string(), p.get(1).unwrap_or(&"").to_string(), p.get(2).unwrap_or(&"").to_string(), p.get(3) == Some(&"1"))
            })
            .collect()
    }

    /// The height the rows need.
    pub fn wanted_height(&self) -> f32 {
        (self.rows().len().max(1) as f32) * ROW
    }
}

impl Control for CheckList {
    fn get_preferred_size(&self, _c: &dyn Canvas, proposed: Size) -> Size {
        Size { width: proposed.width, height: self.wanted_height() }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        let b = e.clip_rectangle;
        let th = palette();
        let g = e.graphics;
        let f = Font::with_dip("Segoe UI", 12.0, FontStyle::REGULAR);
        let rows = self.rows();
        if rows.is_empty() {
            g.draw_string(&self.empty_text, &Font::with_dip("Segoe UI", 12.0, FontStyle::ITALIC), Brush::solid(th.text_tertiary), Rect::new(b.left, b.top, b.right, b.top + ROW), &one(StringAlignment::Near));
        }
        for (i, (_, label, color, checked)) in rows.iter().enumerate() {
            let y = b.top + i as f32 * ROW;
            let cb = Rect::new(b.left + 2.0, y + 7.0, b.left + 16.0, y + 21.0);
            if *checked {
                g.fill_rounded_rectangle(Brush::solid(th.primary), cb, 3.0);
                g.draw_icon("Check", cb, 11.0, Color::WHITE);
            } else {
                g.fill_rounded_rectangle(Brush::solid(th.surface0), cb, 3.0);
                g.draw_rounded_rectangle(&Pen::new(th.border, 1.0), cb, 3.0);
            }
            let dot = Rect::new(cb.right + 8.0, y + 9.0, cb.right + 18.0, y + 19.0);
            g.fill_ellipse(Brush::solid(Color::from_hex(color).unwrap_or(th.primary)), dot);
            g.draw_string(label, &f, Brush::solid(th.text_primary), Rect::new(dot.right + 8.0, y, b.right, y + ROW), &one(StringAlignment::Near));
        }
        e.raise(self, "OnPaint");
    }

    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let i = (e.args().y / ROW) as usize;
        if let Some((key, ..)) = self.rows().into_iter().nth(i) {
            self.raise_action(ListActionEventArgs { action: "toggle".into(), key, x: e.args().x, y: e.args().y });
        }
        e.raise(&*self, "OnMouseDown");
    }

    fn cursor_at(&self, _x: f32, _y: f32) -> Option<kubuno_desktop::controls::host::Cursor> {
        Some(kubuno_desktop::controls::host::Cursor::Hand)
    }

    fn accessible_parts(&self) -> Vec<AccessiblePart> {
        use kubuno_desktop::controls::host::access::AccessRole;
        let w = self.size().width;
        self.rows().into_iter().enumerate().map(|(i, (_, l, _, _))| AccessiblePart { name: l, role: AccessRole::CheckBox, bounds: Rect::new(0.0, i as f32 * ROW, w, (i + 1) as f32 * ROW) }).collect()
    }
}

/// Rows `key|label|type|lag` (one per line).
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Projects")]
#[toolbox(icon = "list")]
#[default_event("Action")]
pub struct PredecessorList {
    base: ControlCore,
    #[property(bindable)]
    #[category("Data")]
    pub items: String,
    #[property(bindable)]
    #[category("Appearance")]
    pub empty_text: String,
    #[event]
    #[category("Action")]
    pub action: Event<ListActionEventArgs>,
}

impl Default for PredecessorList {
    fn default() -> Self {
        Self { base: ControlCore::default(), items: "d1|2 · Choix du déménageur|FS|2".into(), empty_text: String::new(), action: Event::default() }
    }
}

impl PredecessorList {
    fn rows(&self) -> Vec<(String, String, String, String)> {
        self.items
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| {
                let p: Vec<&str> = l.splitn(4, '|').collect();
                (p.first().unwrap_or(&"").to_string(), p.get(1).unwrap_or(&"").to_string(), p.get(2).unwrap_or(&"").to_string(), p.get(3).unwrap_or(&"").to_string())
            })
            .collect()
    }

    /// The type box, the lag box and the delete button of a row (control coordinates).
    fn parts(&self, w: f32, i: usize) -> (Rect, Rect, Rect) {
        let y = i as f32 * (ROW + 6.0);
        let del = Rect::new(w - 20.0, y + 4.0, w, y + ROW - 4.0);
        let lag = Rect::new(del.left - 6.0 - 48.0, y, del.left - 6.0, y + ROW);
        let ty = Rect::new(lag.left - 6.0 - 72.0, y, lag.left - 6.0, y + ROW);
        (ty, lag, del)
    }

    pub fn wanted_height(&self) -> f32 {
        (self.rows().len().max(1) as f32) * (ROW + 6.0)
    }
}

impl Control for PredecessorList {
    fn get_preferred_size(&self, _c: &dyn Canvas, proposed: Size) -> Size {
        Size { width: proposed.width, height: self.wanted_height() }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        let b = e.clip_rectangle;
        let th = palette();
        let g = e.graphics;
        let f = Font::with_dip("Segoe UI", 12.0, FontStyle::REGULAR);
        let rows = self.rows();
        if rows.is_empty() {
            g.draw_string(&self.empty_text, &Font::with_dip("Segoe UI", 12.0, FontStyle::ITALIC), Brush::solid(th.text_tertiary), Rect::new(b.left, b.top, b.right, b.top + ROW), &one(StringAlignment::Near));
        }
        let w = b.right - b.left;
        for (i, (_, label, ty, lag)) in rows.iter().enumerate() {
            let (tr, lr, dr) = self.parts(w, i);
            let off = |r: Rect| Rect::new(b.left + r.left, b.top + r.top, b.left + r.right, b.top + r.bottom);
            let (tr, lr, dr) = (off(tr), off(lr), off(dr));
            g.draw_string(label, &f, Brush::solid(th.text_primary), Rect::new(b.left, tr.top, tr.left - 6.0, tr.bottom), &one(StringAlignment::Near));
            for (r, text, align) in [(tr, ty.as_str(), StringAlignment::Near), (lr, lag.as_str(), StringAlignment::Far)] {
                g.fill_rounded_rectangle(Brush::solid(th.surface0), r, 4.0);
                g.draw_rounded_rectangle(&Pen::new(th.border, 1.0), r, 4.0);
                g.draw_string(text, &f, Brush::solid(th.text_primary), Rect::new(r.left + 6.0, r.top, r.right - 6.0, r.bottom), &one(align));
            }
            g.draw_icon("ChevronDown", Rect::new(tr.right - 18.0, tr.top, tr.right - 4.0, tr.bottom), 12.0, th.text_tertiary);
            g.draw_icon("Trash2", dr, 13.0, th.text_tertiary);
        }
        e.raise(self, "OnPaint");
    }

    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y) = (e.args().x, e.args().y);
        let w = self.size().width;
        for (i, (key, ..)) in self.rows().into_iter().enumerate() {
            let (tr, lr, dr) = self.parts(w, i);
            let action = if tr.contains(x, y) {
                "type"
            } else if lr.contains(x, y) {
                "lag"
            } else if dr.contains(x, y) {
                "delete"
            } else {
                continue;
            };
            self.raise_action(ListActionEventArgs { action: action.into(), key, x, y });
            break;
        }
        e.raise(&*self, "OnMouseDown");
    }

    fn cursor_at(&self, x: f32, y: f32) -> Option<kubuno_desktop::controls::host::Cursor> {
        let w = self.size().width;
        let on = (0..self.rows().len()).any(|i| {
            let (a, b, c) = self.parts(w, i);
            a.contains(x, y) || b.contains(x, y) || c.contains(x, y)
        });
        Some(if on { kubuno_desktop::controls::host::Cursor::Hand } else { kubuno_desktop::controls::host::Cursor::Arrow })
    }

    fn accessible_parts(&self) -> Vec<AccessiblePart> {
        use kubuno_desktop::controls::host::access::AccessRole;
        let w = self.size().width;
        let mut out = Vec::new();
        for (i, (_, l, ty, lag)) in self.rows().into_iter().enumerate() {
            let (a, b, c) = self.parts(w, i);
            out.push(AccessiblePart { name: format!("{l} — {ty}"), role: AccessRole::ComboBox, bounds: a });
            out.push(AccessiblePart { name: format!("{l} — {lag}"), role: AccessRole::SpinButton, bounds: b });
            out.push(AccessiblePart { name: format!("{l} — {}", crate::Resources::delete()), role: AccessRole::Button, bounds: c });
        }
        out
    }
}
