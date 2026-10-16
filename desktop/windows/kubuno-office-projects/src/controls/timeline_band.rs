//! `TimelineBand` - the overview above the Gantt (`web/src/project/TimelineBand.tsx`): the « Chronologie »
//! caption, then the phases and the milestones of the whole project on up to six lanes (the core's
//! `views::band_layout`), the start and end dates in their gutters; a click selects.

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::graphics::{Brush, Color, Font, FontStyle, Pen, PointF, StringAlignment, StringFormat, StringFormatFlags, StringTrimming};
use kubuno_desktop::ui::{Canvas, Rect, Size};
use kubuno_desktop::views::component::{Component, Control, ControlCore, EventCx, PaintEventCx};
use kubuno_desktop::views::events::Event;
use kubuno_office_projects_core::date::{self, DateFormat};
use kubuno_office_projects_core::view;
use kubuno_office_projects_core::views::{band_lane_y, band_layout, band_track_h, BandItem, DATE_GUTTER, LANE_H};

use crate::controls::model::{hex, sample_model, Palette, Shared};

/// The band's padding (`px-4 py-2`) and caption row (`mb-1`, 10 px uppercase).
const PAD_X: f32 = 16.0;
const PAD_Y: f32 = 8.0;
const CAPTION_H: f32 = 18.0;

/// A click selected a task.
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct BandSelectEventArgs {
    pub task: String,
}

#[derive(kubuno_desktop::views::component::Component, Default)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Projects")]
#[toolbox(icon = "calendar")]
#[default_event("TaskSelected")]
pub struct TimelineBand {
    base: ControlCore,
    /// Occurs when a phase or a milestone is clicked.
    #[event]
    #[category("Action")]
    pub task_selected: Event<BandSelectEventArgs>,
    model: Option<Shared>,
    bounds: Rect,
}


impl TimelineBand {
    pub fn set_model(&mut self, model: Shared) {
        self.model = Some(model);
        self.invalidate();
    }

    /// The height the band needs for its lanes (the web's band grows with them).
    pub fn wanted_height(&self) -> f32 {
        let lanes = self.layout(self.size().width).1;
        PAD_Y * 2.0 + CAPTION_H + band_track_h(lanes) as f32 + 1.0
    }

    fn layout(&self, width: f32) -> (Vec<BandItem>, usize) {
        let Some(model) = self.model.as_ref() else { return (Vec::new(), 1) };
        let m = model.borrow();
        let Some(ed) = m.ed.as_ref() else { return (Vec::new(), 1) };
        let start = ed.project_start();
        let span = view::total_days(ed.data(), start);
        let inner = ((width - 2.0 * PAD_X) as f64 - 2.0 * DATE_GUTTER).max(0.0);
        band_layout(ed.data(), start, span, inner)
    }
}

fn fit(g: &kubuno_desktop::ui::graphics::Graphics<'_>, text: &str, font: &Font, max_w: f32) -> String {
    if max_w < 8.0 {
        return String::new();
    }
    let f = StringFormat { flags: StringFormatFlags::NO_WRAP, ..StringFormat::default() };
    if g.measure_string(text, font, None, &f).width <= max_w {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let s: String = chars[..mid].iter().collect::<String>() + "…";
        if g.measure_string(&s, font, None, &f).width <= max_w {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    if lo == 0 {
        String::new()
    } else {
        chars[..lo].iter().collect::<String>() + "…"
    }
}

impl Control for TimelineBand {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, proposed: Size) -> Size {
        let lanes = self.layout(proposed.width.max(400.0)).1;
        Size { width: proposed.width, height: PAD_Y * 2.0 + CAPTION_H + band_track_h(lanes) as f32 + 1.0 }
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
        g.draw_line(&Pen::new(th.border, 1.0), PointF::new(b.left, b.bottom - 0.5), PointF::new(b.right, b.bottom - 0.5));
        let cap = Rect::new(b.left + PAD_X, b.top + PAD_Y, b.right - PAD_X, b.top + PAD_Y + 14.0);
        g.draw_icon("CalendarRange", Rect::new(cap.left, cap.top, cap.left + 12.0, cap.bottom), 12.0, th.text_tertiary);
        let small = StringFormat { alignment: StringAlignment::Near, line_alignment: StringAlignment::Center, trimming: StringTrimming::None, flags: StringFormatFlags::NO_WRAP };
        g.draw_string(&crate::Resources::timeline().to_uppercase(), &Font::with_dip("Segoe UI", 10.0, FontStyle::BOLD), Brush::solid(th.text_tertiary), Rect::new(cap.left + 18.0, cap.top, cap.right, cap.bottom), &small);
        let Some(model) = self.model.clone() else { return };
        let m = model.borrow();
        let Some(ed) = m.ed.as_ref() else { return };
        let (items, lanes) = self.layout(b.right - b.left);
        let track_h = band_track_h(lanes) as f32;
        let track = Rect::new(b.left + PAD_X, b.top + PAD_Y + CAPTION_H, b.right - PAD_X, b.top + PAD_Y + CAPTION_H + track_h);
        g.fill_rounded_rectangle(Brush::solid(th.surface0), track, 6.0);
        g.draw_rounded_rectangle(&Pen::new(th.border, 1.0), track, 6.0);
        let start = ed.project_start();
        let total = view::total_days(ed.data(), start);
        let lang = m.view.lang;
        let f10 = Font::with_dip("Segoe UI", 10.0, FontStyle::REGULAR);
        let mid = Rect::new(track.left + 6.0, track.top, track.right - 6.0, track.bottom);
        g.draw_string(&date::format(start, DateFormat::DayMonthYear2, lang), &f10, Brush::solid(th.text_secondary), mid, &small);
        g.draw_string(&date::format(start.add_days(total as i64), DateFormat::DayMonthYear2, lang), &f10, Brush::solid(th.text_secondary), mid, &StringFormat { alignment: StringAlignment::Far, ..small });
        let selected = m.selected_task();
        let f11 = Font::with_dip("Segoe UI", 11.0, FontStyle::REGULAR);
        for it in &items {
            let t = &ed.data().tasks[it.index];
            let x = track.left + DATE_GUTTER as f32 + it.x as f32;
            let y = track.top + band_lane_y(lanes, it.lane) as f32;
            let sel = selected.as_deref() == Some(t.id.as_str());
            if it.milestone {
                let (cx, cy, s) = (x + 10.0, y + LANE_H as f32 / 2.0, 8.0);
                let pts = [PointF::new(cx, cy - s), PointF::new(cx + s, cy), PointF::new(cx, cy + s), PointF::new(cx - s, cy)];
                g.fill_polygon(Brush::solid(th.warning), &pts);
                if sel {
                    g.draw_polygon(&Pen::new(th.primary, 2.0), &pts);
                }
            } else {
                let c = if t.is_summary_type() || t.parent_id.is_none() { hex(0x666666) } else if t.is_critical { th.danger } else { th.primary };
                let r = Rect::new(x, y, x + it.w as f32, y + LANE_H as f32);
                g.fill_rounded_rectangle(Brush::solid(c), r, 5.0);
                if sel {
                    g.draw_rounded_rectangle(&Pen::new(th.primary, 2.0), r, 5.0);
                }
                let label = fit(g, &t.name, &f11, it.w as f32 - 12.0);
                if !label.is_empty() {
                    g.draw_string(&label, &f11, Brush::solid(Color::WHITE), Rect::new(x + 6.0, y, x + it.w as f32, y + LANE_H as f32), &small);
                }
            }
        }
        e.raise(self, "OnPaint");
    }

    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y) = (e.args().x, e.args().y);
        let (items, lanes) = self.layout(self.size().width);
        let ids: Vec<String> = match self.model.as_ref().map(|m| m.borrow()) {
            Some(m) => match m.data() {
                Some(d) => items.iter().map(|i| d.tasks[i.index].id.clone()).collect(),
                None => Vec::new(),
            },
            None => Vec::new(),
        };
        let track_left = PAD_X + DATE_GUTTER as f32;
        let track_top = PAD_Y + CAPTION_H;
        // The last drawn (topmost) wins.
        for (it, id) in items.iter().zip(ids).rev() {
            let top = track_top + band_lane_y(lanes, it.lane) as f32;
            let (hx, hw) = if it.milestone { (track_left + it.x as f32 + 10.0 - 10.0, 20.0) } else { (track_left + it.x as f32, it.w as f32) };
            if x >= hx && x <= hx + hw && y >= top && y <= top + LANE_H as f32 {
                self.raise_task_selected(BandSelectEventArgs { task: id });
                break;
            }
        }
        e.raise(&*self, "OnMouseDown");
    }

    fn cursor_at(&self, _x: f32, _y: f32) -> Option<kubuno_desktop::controls::host::Cursor> {
        Some(kubuno_desktop::controls::host::Cursor::Hand)
    }
}
