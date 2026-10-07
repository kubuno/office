//! `RoadmapView` — the plan's roadmap (`web/src/project/RoadmapView.tsx`): the release versions (the core's
//! `views::roadmap`) as cards in a centred 768 px column, each with its name, status, due date and bin, a
//! progress bar (done / total), the counts and its tasks (done ones struck through). The header has the
//! « Versions terminées » check box and « Nouvelle version ».
//!
//! The fields of a card are drawn as the web's inputs; a click raises [`RoadmapView::action`] and the window
//! edits through a prompt (name, due date) or a menu (status). A click on a task selects it.

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::graphics::{Brush, Color, Font, FontStyle, Graphics, Pen, PointF, StringAlignment, StringFormat, StringFormatFlags, StringTrimming};
use kubuno_desktop::ui::{Canvas, Rect, Size};
use kubuno_desktop::views::component::{Component, Control, ControlCore, EventCx, PaintEventCx};
use kubuno_desktop::views::events::Event;
use kubuno_office_projects_core::views::{roadmap, RoadmapCard};

use crate::controls::model::{hex, sample_model, Palette, Shared};

const MAX_W: f32 = 768.0;
const PAD: f32 = 24.0;
const CARD_PAD: f32 = 20.0;
const FIELD_H: f32 = 36.0;

/// A click on a roadmap control: `new`, `rename`, `status`, `due`, `delete` (with the version id) or `task`
/// (with the task id); `x`, `y`: where a menu opens (the coordinates the board uses for its menus).
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct RoadmapActionEventArgs {
    pub action: String,
    pub id: String,
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, PartialEq)]
struct Hit {
    rect: Rect,
    action: &'static str,
    id: String,
    name: String,
}

#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Projects")]
#[toolbox(icon = "milestone")]
#[default_event("Action")]
pub struct RoadmapView {
    base: ControlCore,
    /// Occurs on a click on a card's field, the bin, a task or « Nouvelle version ».
    #[event]
    #[category("Action")]
    pub action: Event<RoadmapActionEventArgs>,
    model: Option<Shared>,
    show_closed: bool,
    bounds: Rect,
    hits: Vec<Hit>,
    hover: Option<usize>,
    scroll_y: f32,
    content_h: f32,
}

impl Default for RoadmapView {
    fn default() -> Self {
        Self { base: ControlCore::default(), action: Event::default(), model: None, show_closed: false, bounds: Rect::default(), hits: Vec::new(), hover: None, scroll_y: 0.0, content_h: 0.0 }
    }
}

fn status_label(s: &str) -> String {
    match s {
        "locked" => crate::Resources::version_locked(),
        "closed" => crate::Resources::version_closed(),
        _ => crate::Resources::version_open(),
    }
    .to_string()
}

fn one_line(align: StringAlignment) -> StringFormat {
    StringFormat { alignment: align, line_alignment: StringAlignment::Center, trimming: StringTrimming::EllipsisCharacter, flags: StringFormatFlags::NO_WRAP }
}

impl RoadmapView {
    pub fn set_model(&mut self, model: Shared) {
        self.model = Some(model);
        self.invalidate();
    }

    fn cards(&self) -> Option<(Vec<RoadmapCard>, bool)> {
        let m = self.model.as_ref()?.borrow();
        let ed = m.ed.as_ref()?;
        Some((roadmap(&ed.versions, ed.data(), self.show_closed), ed.versions.is_empty()))
    }

    fn card_height(card: &RoadmapCard) -> f32 {
        let list = if card.tasks.is_empty() { 18.0 } else { card.tasks.len() as f32 * 24.0 - 4.0 };
        CARD_PAD + FIELD_H + 12.0 + 10.0 + 8.0 + 16.0 + 12.0 + list + CARD_PAD
    }

    /// A field drawn as the web's `Input` / `Dropdown`.
    fn field(&mut self, g: &Graphics<'_>, th: &Palette, r: Rect, text: &str, placeholder: bool, chevron: bool, hit: (&'static str, &str, String)) {
        let hovered = self.hover == Some(self.hits.len());
        g.fill_rounded_rectangle(Brush::solid(th.surface0), r, 8.0);
        g.draw_rounded_rectangle(&Pen::new(if hovered { th.text_tertiary } else { th.border }, 1.0), r, 8.0);
        let f = Font::with_dip("Segoe UI", 14.0, FontStyle::REGULAR);
        let right = if chevron { r.right - 24.0 } else { r.right - 12.0 };
        g.draw_string(text, &f, Brush::solid(if placeholder { th.text_tertiary } else { th.text_primary }), Rect::new(r.left + 12.0, r.top, right, r.bottom), &one_line(StringAlignment::Near));
        if chevron {
            g.draw_string("▾", &Font::with_dip("Segoe UI", 12.0, FontStyle::REGULAR), Brush::solid(th.text_primary), Rect::new(r.right - 24.0, r.top, r.right - 8.0, r.bottom), &one_line(StringAlignment::Center));
        }
        self.hits.push(Hit { rect: r, action: hit.0, id: hit.1.to_string(), name: hit.2 });
    }

    fn paint_card(&mut self, g: &Graphics<'_>, th: &Palette, card: &RoadmapCard, r: Rect, selected: Option<&str>) {
        g.fill_rounded_rectangle(Brush::solid(th.surface0), r, 12.0);
        g.draw_rounded_rectangle(&Pen::new(th.border, 1.0), r, 12.0);
        let (l, rt) = (r.left + CARD_PAD, r.right - CARD_PAD);
        let mut y = r.top + CARD_PAD;
        // Header: icon, name, status, due date, bin.
        g.draw_icon("Package", Rect::new(l, y + 9.0, l + 18.0, y + 27.0), 18.0, th.text_secondary);
        let name_w = 178.0f32.min(rt - l - 26.0 - 148.0 - 96.0 - 40.0).max(96.0);
        let name_r = Rect::new(l + 26.0, y, l + 26.0 + name_w, y + FIELD_H);
        self.field(g, th, name_r, &card.name, false, false, ("rename", &card.id, crate::Resources::version_name().to_string()));
        let st_r = Rect::new(name_r.right + 8.0, y, name_r.right + 8.0 + 87.0, y + FIELD_H);
        self.field(g, th, st_r, &status_label(&card.status), false, true, ("status", &card.id, crate::Resources::version_status().to_string()));
        let bin = Rect::new(rt - 27.0, y + 6.0, rt, y + 33.0);
        let due_r = Rect::new(bin.left - 8.0 - 148.0, y, bin.left - 8.0, y + FIELD_H);
        let due = card.due_date.as_deref().map(format_date).unwrap_or_else(|| crate::Resources::date_placeholder().to_string());
        self.field(g, th, due_r, &due, card.due_date.is_none(), false, ("due", &card.id, crate::Resources::version_due().to_string()));
        g.draw_icon("Calendar", Rect::new(due_r.right - 26.0, due_r.top + 11.0, due_r.right - 12.0, due_r.top + 25.0), 14.0, th.text_primary);
        let bin_hover = self.hover == Some(self.hits.len());
        if bin_hover {
            g.fill_rounded_rectangle(Brush::solid(th.surface2), bin, 6.0);
        }
        g.draw_icon("Trash2", Rect::new(bin.left + 6.0, bin.top + 6.0, bin.right - 6.0, bin.bottom - 6.0), 15.0, if bin_hover { th.danger } else { th.text_tertiary });
        self.hits.push(Hit { rect: bin, action: "delete", id: card.id.clone(), name: crate::Resources::delete_word().to_string() });
        y += FIELD_H + 12.0;
        // Progress.
        let fs = Font::with_dip("Segoe UI", 12.0, FontStyle::REGULAR);
        let label = crate::Resources::roadmap_progress().replace("{pct}", &card.pct.to_string());
        let lw = g.measure_string(&label, &fs, None, &one_line(StringAlignment::Near)).width;
        let bar = Rect::new(l, y, rt - lw - 12.0, y + 10.0);
        g.fill_rounded_rectangle(Brush::solid(th.surface2), bar, 5.0);
        if card.pct > 0 {
            let fill = if card.pct == 100 { hex(0x1e8e3e) } else { th.primary };
            g.fill_rounded_rectangle(Brush::solid(fill), Rect::new(bar.left, bar.top, bar.left + (bar.right - bar.left) * card.pct as f32 / 100.0, bar.bottom), 5.0);
        }
        g.draw_string(&label, &fs, Brush::solid(th.text_secondary), Rect::new(rt - lw - 2.0, y - 4.0, rt + 2.0, y + 14.0), &one_line(StringAlignment::Far));
        y += 10.0 + 8.0;
        // Counts.
        let mut counts = crate::Resources::roadmap_counts().replace("{done}", &card.done.to_string()).replace("{open}", &(card.tasks.len() - card.done).to_string());
        if let Some(d) = card.due_date.as_deref() {
            counts.push_str(&format!(" · {} {}", crate::Resources::version_due(), kubuno_office_projects_core::date::Date::parse(d).map(|x| kubuno_office_projects_core::date::format(x, kubuno_office_projects_core::date::DateFormat::Date, self.lang())).unwrap_or_else(|| d.to_string())));
        }
        g.draw_string(&counts, &fs, Brush::solid(th.text_tertiary), Rect::new(l, y, rt, y + 16.0), &one_line(StringAlignment::Near));
        y += 16.0 + 12.0;
        // Tasks.
        if card.tasks.is_empty() {
            let fi = Font::with_dip("Segoe UI", 12.0, FontStyle::ITALIC);
            g.draw_string(crate::Resources::roadmap_no_tasks(), &fi, Brush::solid(th.text_tertiary), Rect::new(l, y, rt, y + 18.0), &one_line(StringAlignment::Near));
            return;
        }
        let ft = Font::with_dip("Segoe UI", 14.0, FontStyle::REGULAR);
        for t in &card.tasks {
            let color = if t.done { th.text_tertiary } else { th.primary };
            if t.milestone {
                g.draw_icon("Milestone", Rect::new(l, y + 4.0, l + 12.0, y + 16.0), 12.0, color);
            } else {
                flag(g, l, y + 4.5, color);
            }
            let fg = if t.done { th.text_tertiary } else { th.text_primary };
            let tw = g.measure_string(&t.name, &ft, None, &one_line(StringAlignment::Near)).width.min(rt - l - 18.0);
            let tr = Rect::new(l + 18.0, y, l + 18.0 + tw + 2.0, y + 20.0);
            g.draw_string(&t.name, &ft, Brush::solid(fg), tr, &one_line(StringAlignment::Near));
            let hovered = self.hover == Some(self.hits.len());
            if t.done || hovered {
                let ly = if t.done { y + 11.0 } else { y + 18.0 };
                g.draw_line(&Pen::new(fg, 1.0), PointF::new(tr.left, ly), PointF::new(tr.left + tw, ly));
            }
            if selected == Some(t.id.as_str()) {
                g.draw_rounded_rectangle(&Pen::new(th.primary.with_alpha(0.5), 1.0), Rect::new(l - 4.0, y - 2.0, tr.right + 4.0, y + 22.0), 4.0);
            }
            self.hits.push(Hit { rect: Rect::new(l, y, tr.right, y + 20.0), action: "task", id: t.id.clone(), name: t.name.clone() });
            y += 24.0;
        }
    }

    fn lang(&self) -> kubuno_office_projects_core::date::Lang {
        self.model.as_ref().map(|m| m.borrow().view.lang).unwrap_or(kubuno_office_projects_core::date::Lang::Fr)
    }

    fn hit_at(&self, x: f32, y: f32) -> Option<usize> {
        self.hits.iter().position(|h| h.rect.contains(x + self.bounds.left, y + self.bounds.top))
    }
}

/// `formatDate(…, 'date')`: `dd/mm/yyyy`.
fn format_date(iso: &str) -> String {
    match kubuno_office_projects_core::date::Date::parse(iso) {
        Some(d) => format!("{:02}/{:02}/{}", d.day, d.month, d.year),
        None => iso.to_string(),
    }
}

impl Control for RoadmapView {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, proposed: Size) -> Size {
        proposed
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        self.bounds = e.clip_rectangle;
        if self.model.is_none() && self.design_mode() {
            self.model = Some(std::rc::Rc::new(std::cell::RefCell::new(sample_model())));
        }
        let th = Palette::of(kubuno_desktop::Application::theme().mode == kubuno_desktop::ui::ThemeMode::Dark);
        let g = e.graphics;
        let b = self.bounds;
        g.fill_rectangle(Brush::solid(th.surface1), b);
        self.hits.clear();
        let Some((cards, none)) = self.cards() else { return };
        let selected = self.model.as_ref().and_then(|m| m.borrow().view.selected.clone());
        let saved = g.save();
        g.intersect_clip(b);
        let w = (b.right - b.left - 2.0 * PAD).min(MAX_W);
        let l = b.left + ((b.right - b.left) - w) / 2.0;
        let r = l + w;
        let mut y = b.top + PAD - self.scroll_y;
        // Header.
        let fh = Font::with_dip("Segoe UI", 20.0, FontStyle::BOLD);
        g.draw_string(crate::Resources::view_roadmap(), &fh, Brush::solid(th.text_primary), Rect::new(l, y, r, y + 32.0), &one_line(StringAlignment::Near));
        let fb = Font::with_dip("Segoe UI", 13.0, FontStyle::REGULAR);
        let new_label = crate::Resources::version_new();
        let bw = g.measure_string(new_label, &fb, None, &one_line(StringAlignment::Near)).width + 44.0;
        let btn = Rect::new(r - bw, y, r, y + 32.0);
        let btn_hover = self.hover == Some(self.hits.len());
        let on_primary = if th.dark { hex(0x202124) } else { Color::WHITE };
        g.fill_rounded_rectangle(Brush::solid(if btn_hover { th.primary.with_alpha(0.9) } else { th.primary }), btn, 8.0);
        g.draw_icon("Plus", Rect::new(btn.left + 12.0, btn.top + 9.0, btn.left + 26.0, btn.top + 23.0), 14.0, on_primary);
        g.draw_string(new_label, &fb, Brush::solid(on_primary), Rect::new(btn.left + 32.0, btn.top, btn.right - 8.0, btn.bottom), &one_line(StringAlignment::Near));
        self.hits.push(Hit { rect: btn, action: "new", id: String::new(), name: new_label.to_string() });
        let closed_label = crate::Resources::roadmap_show_closed();
        let cw = g.measure_string(closed_label, &fb, None, &one_line(StringAlignment::Near)).width;
        let cb = Rect::new(btn.left - 12.0 - cw - 26.0, y + 7.0, btn.left - 12.0 - cw - 8.0, y + 25.0);
        if self.show_closed {
            g.fill_rounded_rectangle(Brush::solid(th.primary), cb, 4.0);
            g.draw_icon("Check", Rect::new(cb.left + 2.0, cb.top + 2.0, cb.right - 2.0, cb.bottom - 2.0), 14.0, Color::WHITE);
        } else {
            g.fill_rounded_rectangle(Brush::solid(th.surface0), cb, 4.0);
            g.draw_rounded_rectangle(&Pen::new(th.text_tertiary, 1.5), cb, 4.0);
        }
        g.draw_string(closed_label, &fb, Brush::solid(th.text_primary), Rect::new(cb.right + 8.0, y, btn.left - 8.0, y + 32.0), &one_line(StringAlignment::Near));
        self.hits.push(Hit { rect: Rect::new(cb.left, y, btn.left - 8.0, y + 32.0), action: "closed", id: String::new(), name: closed_label.to_string() });
        y += 32.0 + 20.0;
        if none {
            let card = Rect::new(l, y, r, y + 148.0);
            g.fill_rounded_rectangle(Brush::solid(th.surface0), card, 12.0);
            g.draw_rounded_rectangle(&Pen::new(th.border, 1.0), card, 12.0);
            let cx = (l + r) / 2.0;
            let ib = Rect::new(cx - 24.0, y + 40.0, cx + 24.0, y + 88.0);
            g.fill_rounded_rectangle(Brush::solid(th.surface2), ib, 12.0);
            g.draw_icon("Package", Rect::new(ib.left + 13.0, ib.top + 13.0, ib.right - 13.0, ib.bottom - 13.0), 22.0, th.text_tertiary);
            let f = Font::with_dip("Segoe UI", 14.0, FontStyle::REGULAR);
            let wrap = StringFormat { alignment: StringAlignment::Center, line_alignment: StringAlignment::Near, trimming: StringTrimming::None, flags: StringFormatFlags::NONE };
            g.draw_string(crate::Resources::roadmap_empty(), &f, Brush::solid(th.text_secondary), Rect::new(cx - 224.0, y + 100.0, cx + 224.0, y + 140.0), &wrap);
            y += 148.0;
        }
        for c in &cards {
            let h = Self::card_height(c);
            self.paint_card(g, &th, c, Rect::new(l, y, r, y + h), selected.as_deref());
            y += h + 16.0;
        }
        self.content_h = y + self.scroll_y - b.top + PAD - 16.0;
        g.restore(saved);
        e.raise(self, "OnPaint");
    }

    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        self.focus();
        let (x, y) = (e.args().x, e.args().y);
        if e.args().button == MouseButton::Left {
            if let Some(h) = self.hit_at(x, y).map(|i| self.hits[i].clone()) {
                match h.action {
                    "closed" => {
                        self.show_closed = !self.show_closed;
                        self.invalidate();
                    }
                    "task" => {
                        if let Some(m) = &self.model {
                            m.borrow_mut().view.selected = Some(h.id.clone());
                        }
                        self.invalidate();
                        self.raise_action(RoadmapActionEventArgs { action: "task".into(), id: h.id, x, y });
                    }
                    a => self.raise_action(RoadmapActionEventArgs { action: a.into(), id: h.id, x: h.rect.left, y: h.rect.bottom + 4.0 }),
                }
            }
        }
        e.raise(&*self, "OnMouseDown");
    }

    fn on_mouse_move(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let hover = self.hit_at(e.args().x, e.args().y);
        if hover != self.hover {
            self.hover = hover;
            self.invalidate();
        }
        e.raise(&*self, "OnMouseMove");
    }

    fn on_mouse_leave(&mut self, e: &mut EventCx<'_, kubuno_desktop::views::events::EmptyEventArgs>) {
        if self.hover.take().is_some() {
            self.invalidate();
        }
        e.raise(&*self, "OnMouseLeave");
    }

    fn on_mouse_wheel(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let h = (self.bounds.bottom - self.bounds.top).max(1.0);
        self.scroll_y = (self.scroll_y - e.args().delta * 100.0).clamp(0.0, (self.content_h - h).max(0.0));
        self.invalidate();
        e.raise(&*self, "OnMouseWheel");
    }

    fn cursor_at(&self, x: f32, y: f32) -> Option<kubuno_desktop::controls::host::Cursor> {
        use kubuno_desktop::controls::host::Cursor;
        Some(if self.hit_at(x, y).is_some() { Cursor::Hand } else { Cursor::Arrow })
    }

    fn accessible_parts(&self) -> Vec<kubuno_desktop::views::component::AccessiblePart> {
        use kubuno_desktop::controls::host::access::AccessRole;
        let b = self.bounds;
        self.hits
            .iter()
            .map(|h| kubuno_desktop::views::component::AccessiblePart {
                name: h.name.clone(),
                role: if h.action == "task" { AccessRole::ListItem } else { AccessRole::Button },
                bounds: Rect::new(h.rect.left - b.left, h.rect.top - b.top, h.rect.right - b.left, h.rect.bottom - b.top),
            })
            .collect()
    }
}

/// Lucide's « flag » (12 px), as the board draws it.
fn flag(g: &Graphics<'_>, x: f32, y: f32, c: Color) {
    let k = 11.0 / 24.0;
    let p = |px: f32, py: f32| PointF::new(x + px * k, y + py * k);
    let pen = Pen::new(c, 2.0 * k * 1.6).with_caps(kubuno_desktop::ui::graphics::LineCap::Round).with_join(kubuno_desktop::ui::graphics::LineJoin::Round);
    g.draw_line(&pen, p(4.0, 22.0), p(4.0, 3.0));
    g.draw_lines(&pen, &[p(4.0, 3.0), p(8.0, 2.0), p(15.0, 4.0), p(20.0, 3.0), p(20.0, 14.0), p(15.0, 15.5), p(8.0, 14.0), p(4.0, 15.0)]);
}
