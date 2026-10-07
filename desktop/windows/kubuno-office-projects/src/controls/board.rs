//! `BoardView` — the plan as a Kanban board (`web/src/project/BoardView.tsx`): the leaves grouped by status in
//! five columns (the core's `views::board`), each card with its milestone mark, priority flag, name, progress
//! bar and assignees; a click selects, a right click opens the task's menu, dragging a card onto another
//! column sets its status (`setStatus`: « Terminé » also sets 100 %).

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::graphics::{Brush, Color, Font, FontStyle, Graphics, Pen, PointF, StringAlignment, StringFormat, StringFormatFlags, StringTrimming};
use kubuno_desktop::ui::{Canvas, Rect, Size};
use kubuno_desktop::views::component::{Component, Control, ControlCore, EventCx, PaintEventCx};
use kubuno_desktop::views::events::Event;
use kubuno_office_projects_core::view;
use kubuno_office_projects_core::views::{assignees, board, initial, priority_color, BOARD_COLUMNS};

use crate::controls::gantt::{GanttContextEventArgs, GanttEditedEventArgs};
use crate::controls::model::{hex, sample_model, Palette, Shared};

const COL_W: f32 = 256.0;
const GAP: f32 = 12.0;
const PAD: f32 = 12.0;
const HEAD_H: f32 = 36.0;

#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Projects")]
#[toolbox(icon = "kanban")]
#[default_event("Edited")]
pub struct BoardView {
    base: ControlCore,
    /// Occurs after a card was moved to another column.
    #[event]
    #[category("Action")]
    pub edited: Event<GanttEditedEventArgs>,
    /// Occurs on a right click on a card.
    #[event]
    #[category("Action")]
    pub context_menu_requested: Event<GanttContextEventArgs>,
    model: Option<Shared>,
    bounds: Rect,
    /// Each card's rectangle (control coordinates) and task id, as last painted.
    cards: Vec<(Rect, String)>,
    columns: Vec<Rect>,
    drag: Option<(String, f32, f32, bool)>,
    over: Option<usize>,
    scroll_x: f32,
}

impl Default for BoardView {
    fn default() -> Self {
        Self { base: ControlCore::default(), edited: Event::default(), context_menu_requested: Event::default(), model: None, bounds: Rect::default(), cards: Vec::new(), columns: Vec::new(), drag: None, over: None, scroll_x: 0.0 }
    }
}

fn status_label(st: &str) -> String {
    match st {
        "not_started" => crate::Resources::status_not_started(),
        "in_progress" => crate::Resources::status_in_progress(),
        "on_hold" => crate::Resources::status_on_hold(),
        "completed" => crate::Resources::status_completed(),
        _ => crate::Resources::status_cancelled(),
    }
    .to_string()
}

impl BoardView {
    pub fn set_model(&mut self, model: Shared) {
        self.model = Some(model);
        self.invalidate();
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_card(&self, g: &Graphics<'_>, th: &Palette, r: Rect, t: &kubuno_office_projects_core::model::Task, pct: i32, people: &[&kubuno_office_projects_core::model::Resource], selected: bool) {
        g.fill_rounded_rectangle(Brush::solid(th.surface0), r, 6.0);
        g.draw_rounded_rectangle(&Pen::new(if selected { th.primary } else { th.border }, 1.0), r, 6.0);
        if selected {
            g.draw_rounded_rectangle(&Pen::new(th.primary.with_alpha(0.3), 2.0), Rect::new(r.left - 1.0, r.top - 1.0, r.right + 1.0, r.bottom + 1.0), 7.0);
        }
        let mut x = r.left + 8.0;
        let top = r.top + 8.0;
        if t.is_milestone() {
            g.draw_icon("Milestone", Rect::new(x, top + 2.0, x + 12.0, top + 14.0), 12.0, hex(0xf97316));
            x += 18.0;
        }
        flag(g, x, top + 2.0, Color::from_hex(priority_color(&t.priority)).unwrap_or(hex(0x9aa0a6)));
        x += 17.0;
        let f = Font::with_dip("Segoe UI", 12.0, FontStyle::REGULAR);
        let wrap = StringFormat { alignment: StringAlignment::Near, line_alignment: StringAlignment::Near, trimming: StringTrimming::None, flags: StringFormatFlags::NONE };
        let tw = r.right - 8.0 - x;
        let th_text = g.measure_string(&t.name, &f, Some(tw), &wrap).height;
        g.draw_string(&t.name, &f, Brush::solid(th.text_primary), Rect::new(x, top, r.right - 8.0, top + th_text + 2.0), &wrap);
        let mut y = top + th_text.max(16.0);
        if pct > 0 {
            y += 6.0;
            let bar = Rect::new(r.left + 8.0, y, r.right - 8.0, y + 4.0);
            g.fill_rounded_rectangle(Brush::solid(th.surface3), bar, 2.0);
            g.fill_rounded_rectangle(Brush::solid(hex(0x34a853)), Rect::new(bar.left, bar.top, bar.left + (bar.right - bar.left) * pct as f32 / 100.0, bar.bottom), 2.0);
            y += 4.0;
        }
        if !people.is_empty() {
            y += 6.0;
            let mut ax = r.left + 8.0;
            let fs = Font::with_dip("Segoe UI", 10.0, FontStyle::BOLD);
            for p in people.iter().take(4) {
                let a = Rect::new(ax, y, ax + 20.0, y + 20.0);
                g.fill_ellipse(Brush::solid(Color::WHITE), Rect::new(a.left - 1.0, a.top - 1.0, a.right + 1.0, a.bottom + 1.0));
                g.fill_ellipse(Brush::solid(Color::from_hex(&p.color).unwrap_or(th.primary)), a);
                g.draw_string(&initial(&p.name), &fs, Brush::solid(Color::WHITE), a, &StringFormat::centered());
                ax += 22.0;
            }
        }
    }

    fn card_height(&self, g: &Graphics<'_>, t: &kubuno_office_projects_core::model::Task, pct: i32, people: usize) -> f32 {
        let f = Font::with_dip("Segoe UI", 12.0, FontStyle::REGULAR);
        let indent = if t.is_milestone() { 18.0 } else { 0.0 } + 17.0;
        let wrap = StringFormat { alignment: StringAlignment::Near, line_alignment: StringAlignment::Near, trimming: StringTrimming::None, flags: StringFormatFlags::NONE };
        let tw = COL_W - 16.0 - 16.0 - indent;
        let mut h = 8.0 + g.measure_string(&t.name, &f, Some(tw), &wrap).height.max(16.0);
        if pct > 0 {
            h += 10.0;
        }
        if people > 0 {
            h += 26.0;
        }
        h + 8.0
    }
}

impl Control for BoardView {
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
        self.cards.clear();
        self.columns.clear();
        let Some(model) = self.model.clone() else { return };
        let m = model.borrow();
        let Some(ed) = m.ed.as_ref() else { return };
        let d = ed.data();
        let shown = view::display_tasks(d, &m.view.filter);
        let cols = board(d, &shown);
        let progress = kubuno_office_projects_core::rollup::effective_progress(&d.tasks);
        let selected = m.selected_task();
        let saved = g.save();
        g.intersect_clip(b);
        for (ci, ((st, _, color), cards)) in BOARD_COLUMNS.iter().zip(cols.iter()).enumerate() {
            let x = b.left + PAD + ci as f32 * (COL_W + GAP) - self.scroll_x;
            let mut heights = Vec::new();
            for &i in cards {
                let t = &d.tasks[i];
                heights.push(self.card_height(g, t, progress.get(&t.id).copied().unwrap_or(t.progress), assignees(d, t).len()));
            }
            let body: f32 = if cards.is_empty() { 60.0 } else { heights.iter().sum::<f32>() + 8.0 * (cards.len() as f32 - 1.0) + 16.0 };
            let col = Rect::new(x, b.top + PAD, x + COL_W, (b.top + PAD + HEAD_H + body).min(b.bottom - PAD));
            self.columns.push(Rect::new(col.left - b.left, col.top - b.top, col.right - b.left, b.bottom - b.top));
            let over = self.over == Some(ci) && self.drag.as_ref().is_some_and(|d| d.3);
            g.fill_rounded_rectangle(Brush::solid(th.surface0), col, 8.0);
            g.draw_rounded_rectangle(&Pen::new(if over { th.primary } else { th.border }, 1.0), col, 8.0);
            if over {
                g.draw_rounded_rectangle(&Pen::new(th.primary.with_alpha(0.3), 2.0), Rect::new(col.left - 1.0, col.top - 1.0, col.right + 1.0, col.bottom + 1.0), 9.0);
            }
            g.with_saved(|g| {
                let mut clip = kubuno_desktop::ui::graphics::GraphicsPath::new();
                clip.add_rounded_rectangle(col, 8.0);
                g.intersect_clip_path(&clip);
                g.fill_rectangle(Brush::solid(Color::from_hex(color).unwrap_or(th.border)), Rect::new(col.left, col.top, col.right, col.top + 3.0));
            });
            g.draw_line(&Pen::new(th.border, 1.0), PointF::new(col.left, col.top + HEAD_H - 0.5), PointF::new(col.right, col.top + HEAD_H - 0.5));
            let label = status_label(st);
            let fl = Font::with_dip("Segoe UI", 12.0, FontStyle::BOLD);
            let one = StringFormat { alignment: StringAlignment::Near, line_alignment: StringAlignment::Center, trimming: StringTrimming::None, flags: StringFormatFlags::NO_WRAP };
            let lw = g.measure_string(&label, &fl, None, &one).width;
            g.draw_string(&label, &fl, Brush::solid(th.text_primary), Rect::new(col.left + 12.0, col.top + 3.0, col.right, col.top + HEAD_H), &one);
            g.draw_string(&cards.len().to_string(), &Font::with_dip("Segoe UI", 11.0, FontStyle::REGULAR), Brush::solid(th.text_tertiary), Rect::new(col.left + 20.0 + lw, col.top + 3.0, col.right, col.top + HEAD_H), &one);
            let mut y = col.top + HEAD_H + 8.0;
            if cards.is_empty() {
                let fi = Font::with_dip("Segoe UI", 11.0, FontStyle::ITALIC);
                g.draw_string("—", &fi, Brush::solid(th.text_tertiary), Rect::new(col.left, y, col.right, y + 40.0), &StringFormat::centered());
            }
            for (k, &i) in cards.iter().enumerate() {
                let t = &d.tasks[i];
                let r = Rect::new(col.left + 8.0, y, col.right - 8.0, y + heights[k]);
                let pct = progress.get(&t.id).copied().unwrap_or(t.progress);
                self.paint_card(g, &th, r, t, pct, &assignees(d, t), selected.as_deref() == Some(t.id.as_str()));
                self.cards.push((Rect::new(r.left - b.left, r.top - b.top, r.right - b.left, r.bottom - b.top), t.id.clone()));
                y += heights[k] + 8.0;
            }
        }
        g.restore(saved);
        e.raise(self, "OnPaint");
    }

    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y) = (e.args().x, e.args().y);
        self.focus();
        if let Some((_, id)) = self.cards.iter().find(|(r, _)| r.contains(x, y)).cloned() {
            if let Some(m) = &self.model {
                m.borrow_mut().view.selected = Some(id.clone());
            }
            if e.args().button == MouseButton::Right {
                self.raise_context_menu_requested(GanttContextEventArgs { kind: "row".into(), task: id, x: x + self.bounds.left, y: y + self.bounds.top });
            } else {
                self.drag = Some((id, x, y, false));
            }
            self.invalidate();
        }
        e.raise(&*self, "OnMouseDown");
    }

    fn on_mouse_move(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y) = (e.args().x, e.args().y);
        if let Some(d) = self.drag.as_mut() {
            if !d.3 && ((x - d.1).powi(2) + (y - d.2).powi(2)).sqrt() >= 4.0 {
                d.3 = true;
            }
            let over = self.columns.iter().position(|c| x >= c.left && x <= c.right);
            if over != self.over {
                self.over = over;
                self.invalidate();
            }
        }
        e.raise(&*self, "OnMouseMove");
    }

    fn on_mouse_up(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        if let (Some((id, _, _, true)), Some(ci)) = (self.drag.take(), self.over) {
            let st = BOARD_COLUMNS[ci].0;
            if let Some(m) = self.model.clone() {
                {
                    let mut m = m.borrow_mut();
                    if let Some(ed) = m.ed.as_mut() {
                        ed.set_now(kubuno_office_projects_common::platform::current().clock.now_ms());
                        let _ = ed.set_status(&id, st);
                    }
                }
                self.raise_edited(GanttEditedEventArgs { label: "task-edit".into() });
            }
        }
        self.drag = None;
        self.over = None;
        self.invalidate();
        e.raise(&*self, "OnMouseUp");
    }

    fn on_mouse_wheel(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let w = (self.bounds.right - self.bounds.left).max(1.0);
        let content = PAD * 2.0 + 5.0 * COL_W + 4.0 * GAP;
        self.scroll_x = (self.scroll_x - e.args().delta * 100.0).clamp(0.0, (content - w).max(0.0));
        self.invalidate();
        e.raise(&*self, "OnMouseWheel");
    }

    fn cursor_at(&self, x: f32, y: f32) -> Option<kubuno_desktop::controls::host::Cursor> {
        use kubuno_desktop::controls::host::Cursor;
        if self.drag.as_ref().is_some_and(|d| d.3) {
            return Some(Cursor::Move);
        }
        Some(if self.cards.iter().any(|(r, _)| r.contains(x, y)) { Cursor::Hand } else { Cursor::Arrow })
    }

    fn accessible_parts(&self) -> Vec<kubuno_desktop::views::component::AccessiblePart> {
        use kubuno_desktop::controls::host::access::AccessRole;
        let Some(m) = self.model.as_ref().map(|m| m.borrow()) else { return Vec::new() };
        let Some(d) = m.data() else { return Vec::new() };
        self.cards.iter().filter_map(|(r, id)| d.task(id).map(|t| kubuno_desktop::views::component::AccessiblePart { name: t.name.clone(), role: AccessRole::ListItem, bounds: *r })).collect()
    }
}

/// Lucide's « flag » (11 px), drawn here: the framework's icon set of the pinned tag does not have it.
fn flag(g: &Graphics<'_>, x: f32, y: f32, c: Color) {
    let k = 11.0 / 24.0;
    let p = |px: f32, py: f32| PointF::new(x + px * k, y + py * k);
    let pen = Pen::new(c, 2.0 * k * 1.6).with_caps(kubuno_desktop::ui::graphics::LineCap::Round).with_join(kubuno_desktop::ui::graphics::LineJoin::Round);
    g.draw_line(&pen, p(4.0, 22.0), p(4.0, 3.0));
    g.draw_lines(&pen, &[p(4.0, 3.0), p(8.0, 2.0), p(15.0, 4.0), p(20.0, 3.0), p(20.0, 14.0), p(15.0, 15.5), p(8.0, 14.0), p(4.0, 15.0)]);
}
