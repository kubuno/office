//! `MonthView` — the plan on a month grid (`web/src/project/CalendarView.tsx`): six weeks from the Monday
//! before the 1st (the core's `views::month_grid`), each leaf drawn on the days it spans (full colour on its
//! first day, faded after), three per day then « +n »; ‹ › change the month, « Aujourd'hui » returns to the
//! project's month; a click selects, a right click opens the task's menu.

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::graphics::{Brush, Color, Font, FontStyle, Pen, PointF, StringAlignment, StringFormat, StringFormatFlags, StringTrimming};
use kubuno_desktop::ui::{Canvas, Rect, Size};
use kubuno_desktop::views::component::{Component, Control, ControlCore, EventCx, PaintEventCx};
use kubuno_desktop::views::events::Event;
use kubuno_office_projects_core::date::{self, DateFormat};
use kubuno_office_projects_core::view;
use kubuno_office_projects_core::views::month_grid;

use crate::controls::gantt::GanttContextEventArgs;
use crate::controls::model::{hex, sample_model, Palette, Shared};

const NAV_H: f32 = 41.0;
const DOW_H: f32 = 24.0;

#[derive(kubuno_desktop::views::component::Component, Default)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Projects")]
#[toolbox(icon = "calendar")]
#[default_event("ContextMenuRequested")]
pub struct MonthView {
    base: ControlCore,
    /// Occurs on a right click on a task.
    #[event]
    #[category("Action")]
    pub context_menu_requested: Event<GanttContextEventArgs>,
    model: Option<Shared>,
    bounds: Rect,
    month_offset: i32,
    chips: Vec<(Rect, String)>,
    nav: [Rect; 3],
}


impl MonthView {
    pub fn set_model(&mut self, model: Shared) {
        self.model = Some(model);
        self.invalidate();
    }
}

impl Control for MonthView {
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
        g.fill_rectangle(Brush::solid(th.surface0), b);
        self.chips.clear();
        let Some(model) = self.model.clone() else { return };
        let m = model.borrow();
        let Some(ed) = m.ed.as_ref() else { return };
        let d = ed.data();
        let start = ed.project_start();
        let shown = view::display_tasks(d, &m.view.filter);
        let (month, days) = month_grid(d, &shown, start, self.month_offset);
        let lang = m.view.lang;
        let one = |a| StringFormat { alignment: a, line_alignment: StringAlignment::Center, trimming: StringTrimming::EllipsisCharacter, flags: StringFormatFlags::NO_WRAP };
        // The navigation row.
        let cy = b.top + NAV_H / 2.0;
        let prev = Rect::new(b.left + 16.0, cy - 12.0, b.left + 40.0, cy + 12.0);
        g.draw_icon("ChevronLeft", prev, 16.0, th.text_secondary);
        let title = date::format(month, DateFormat::MonthYear, lang);
        let title = capitalize(&title);
        let tf = Font::with_dip("Segoe UI", 14.0, FontStyle::BOLD);
        let title_r = Rect::new(prev.right + 12.0, cy - 12.0, prev.right + 12.0 + 140.0, cy + 12.0);
        g.draw_string(&title, &tf, Brush::solid(th.text_primary), title_r, &one(StringAlignment::Center));
        let next = Rect::new(title_r.right + 12.0, cy - 12.0, title_r.right + 36.0, cy + 12.0);
        g.draw_icon("ChevronRight", next, 16.0, th.text_secondary);
        let today_r = Rect::new(next.right + 20.0, cy - 12.0, next.right + 120.0, cy + 12.0);
        g.draw_string(crate::Resources::today(), &Font::with_dip("Segoe UI", 12.0, FontStyle::REGULAR), Brush::solid(th.primary), today_r, &one(StringAlignment::Near));
        self.nav = [prev, next, today_r].map(|r| Rect::new(r.left - b.left, r.top - b.top, r.right - b.left, r.bottom - b.top));
        g.draw_line(&Pen::new(th.border, 1.0), PointF::new(b.left, b.top + NAV_H - 0.5), PointF::new(b.right, b.top + NAV_H - 0.5));
        // The weekdays.
        let cw = (b.right - b.left) / 7.0;
        let dt = b.top + NAV_H;
        let small = Font::with_dip("Segoe UI", 11.0, FontStyle::BOLD);
        for (k, day) in days.iter().enumerate().take(7) {
            let x = b.left + k as f32 * cw;
            let label = date::format(day.date, DateFormat::WeekdayShort, lang);
            let label = capitalize(&label);
            g.draw_string(&label, &small, Brush::solid(th.text_tertiary), Rect::new(x, dt, x + cw, dt + DOW_H), &one(StringAlignment::Center));
            if k < 6 {
                g.draw_line(&Pen::new(th.border, 1.0), PointF::new(x + cw - 0.5, dt), PointF::new(x + cw - 0.5, dt + DOW_H));
            }
        }
        g.draw_line(&Pen::new(th.border, 1.0), PointF::new(b.left, dt + DOW_H - 0.5), PointF::new(b.right, dt + DOW_H - 0.5));
        // The grid.
        let gt = dt + DOW_H;
        let ch = ((b.bottom - gt) / 6.0).max(70.0);
        let selected = m.selected_task();
        let f10 = Font::with_dip("Segoe UI", 10.0, FontStyle::REGULAR);
        for (k, day) in days.iter().enumerate() {
            let (col, row) = (k % 7, k / 7);
            let r = Rect::new(b.left + col as f32 * cw, gt + row as f32 * ch, b.left + (col + 1) as f32 * cw, gt + (row + 1) as f32 * ch);
            if !day.in_month {
                g.fill_rectangle(Brush::solid(th.surface1), r);
            }
            g.draw_line(&Pen::new(th.border, 1.0), PointF::new(r.right - 0.5, r.top), PointF::new(r.right - 0.5, r.bottom));
            g.draw_line(&Pen::new(th.border, 1.0), PointF::new(r.left, r.bottom - 0.5), PointF::new(r.right, r.bottom - 0.5));
            let num = Rect::new(r.left + 4.0, r.top + 4.0, r.left + 20.0, r.top + 20.0);
            if day.date == m.view.today {
                g.fill_ellipse(Brush::solid(th.primary), Rect::new(num.left, num.top, num.left + 16.0, num.top + 16.0));
                g.draw_string(&day.date.day.to_string(), &f10, Brush::solid(Color::WHITE), Rect::new(num.left, num.top, num.left + 16.0, num.top + 16.0), &one(StringAlignment::Center));
            } else {
                let ink = if day.in_month { th.text_secondary } else { th.text_tertiary };
                g.draw_string(&day.date.day.to_string(), &f10, Brush::solid(ink), num, &one(StringAlignment::Near));
            }
            let mut y = r.top + 22.0;
            for &i in day.items.iter().take(3) {
                let t = &d.tasks[i];
                let color = if t.is_critical { hex(0xd93025) } else if t.is_milestone() { hex(0xea4335) } else { hex(0x1a73e8) };
                let first = view::sched_start(t, start) == day.date;
                let chip = Rect::new(r.left + 4.0, y, r.right - 4.0, y + 17.0);
                g.fill_rounded_rectangle(Brush::solid(color.with_alpha(if first { 1.0 } else { 0.6 })), chip, 3.0);
                if selected.as_deref() == Some(t.id.as_str()) {
                    g.draw_rounded_rectangle(&Pen::new(Color::BLACK.with_alpha(0.3), 1.0), chip, 3.0);
                }
                let text = if t.is_milestone() { format!("◆ {}", t.name) } else { t.name.clone() };
                g.draw_string(&text, &f10, Brush::solid(Color::WHITE), Rect::new(chip.left + 4.0, chip.top, chip.right - 2.0, chip.bottom), &one(StringAlignment::Near));
                self.chips.push((Rect::new(chip.left - b.left, chip.top - b.top, chip.right - b.left, chip.bottom - b.top), t.id.clone()));
                y += 19.0;
            }
            if day.items.len() > 3 {
                g.draw_string(&format!("+{}", day.items.len() - 3), &f10, Brush::solid(th.text_tertiary), Rect::new(r.left + 4.0, y, r.right, y + 14.0), &one(StringAlignment::Near));
            }
        }
        e.raise(self, "OnPaint");
    }

    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y) = (e.args().x, e.args().y);
        if self.nav[0].contains(x, y) {
            self.month_offset -= 1;
        } else if self.nav[1].contains(x, y) {
            self.month_offset += 1;
        } else if self.nav[2].contains(x, y) {
            self.month_offset = 0;
        } else if let Some((_, id)) = self.chips.iter().find(|(r, _)| r.contains(x, y)).cloned() {
            if let Some(m) = &self.model {
                m.borrow_mut().view.selected = Some(id.clone());
            }
            if e.args().button == MouseButton::Right {
                self.raise_context_menu_requested(GanttContextEventArgs { kind: "row".into(), task: id, x: x + self.bounds.left, y: y + self.bounds.top });
            }
        }
        self.invalidate();
        e.raise(&*self, "OnMouseDown");
    }

    fn cursor_at(&self, x: f32, y: f32) -> Option<kubuno_desktop::controls::host::Cursor> {
        use kubuno_desktop::controls::host::Cursor;
        Some(if self.nav.iter().any(|r| r.contains(x, y)) || self.chips.iter().any(|(r, _)| r.contains(x, y)) { Cursor::Hand } else { Cursor::Arrow })
    }
}

/// The first letter upper-cased (the web's `capitalize` class).
fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}
