//! `LoadView` — the workload per resource, day by day (`web/src/project/ResourceLoadView.tsx`): a column of
//! resources (avatar, name, capacity) and, for each, a histogram of the units allocated per day (the core's
//! `views::resource_load`) under a dashed capacity line, over-allocation in red; weekends shaded, Mondays
//! ruled, months labelled. Scrolls horizontally with the wheel (Shift) or the scroll bar.

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::graphics::{Brush, Color, Font, FontStyle, Pen, PointF, StringAlignment, StringFormat, StringFormatFlags, StringTrimming};
use kubuno_desktop::ui::range::{ScrollBar, ScrollPart};
use kubuno_desktop::ui::{Canvas, Rect, Size, Widget, WidgetState};
use kubuno_desktop::views::component::{Component, Control, ControlCore, EventCx, PaintEventCx};
use kubuno_office_projects_core::date::{self, DateFormat, Lang};
use kubuno_office_projects_core::view;
use kubuno_office_projects_core::views::{initial, resource_load};

use crate::controls::model::{hex, sample_model, Palette, Shared};

const LOAD_ROW: f32 = 56.0;
const TOP: f32 = 24.0;
const LEFT_W: f32 = 176.0;

#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Projects")]
#[toolbox(icon = "chart")]
pub struct LoadView {
    base: ControlCore,
    model: Option<Shared>,
    bounds: Rect,
    scroll_x: f32,
    drag: Option<f32>,
}

impl Default for LoadView {
    fn default() -> Self {
        Self { base: ControlCore::default(), model: None, bounds: Rect::default(), scroll_x: 0.0, drag: None }
    }
}

impl LoadView {
    pub fn set_model(&mut self, model: Shared) {
        self.model = Some(model);
        self.invalidate();
    }

    fn content_w(&self) -> f32 {
        let Some(m) = self.model.as_ref().map(|m| m.borrow()) else { return 0.0 };
        let Some(ed) = m.ed.as_ref() else { return 0.0 };
        view::total_days(ed.data(), ed.project_start()) as f32 * m.view.zoom.day_w() as f32
    }

    fn hbar(&self) -> Option<(ScrollBar, Rect)> {
        let b = self.bounds;
        let inner = Rect::new(b.left + LEFT_W, b.top, b.right, b.bottom);
        let bar = ScrollBar::from_content(true, self.content_w(), b.right - b.left - LEFT_W, self.scroll_x)?;
        let rail = bar.rail(&inner);
        Some((bar, rail))
    }
}

/// `formatDate(date, { month: 'short', year: '2-digit' })`.
fn month_short(d: kubuno_office_projects_core::date::Date, lang: Lang) -> String {
    let s = date::format(d, DateFormat::DayMonthYear2, lang);
    // « 1 oct. 26 » → « oct. 26 »; « Oct 1, 26 » → « Oct 26 ».
    match lang {
        Lang::Fr => s.split_once(' ').map(|(_, r)| r.to_string()).unwrap_or(s),
        Lang::En => s.replace(&format!(" {},", d.day), ""),
    }
}

impl Control for LoadView {
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
        let Some(model) = self.model.clone() else { return };
        let m = model.borrow();
        let Some(ed) = m.ed.as_ref() else { return };
        let d = ed.data();
        let one = |a| StringFormat { alignment: a, line_alignment: StringAlignment::Center, trimming: StringTrimming::EllipsisCharacter, flags: StringFormatFlags::NO_WRAP };
        if d.resources.is_empty() {
            let f = Font::with_dip("Segoe UI", 14.0, FontStyle::ITALIC);
            g.draw_string(crate::Resources::no_resources_hint(), &f, Brush::solid(th.text_tertiary), b, &StringFormat::centered());
            return;
        }
        let start = ed.project_start();
        let total = view::total_days(d, start);
        let day_w = m.view.zoom.day_w() as f32;
        self.scroll_x = self.scroll_x.clamp(0.0, (self.content_w() - (b.right - b.left - LEFT_W)).max(0.0));
        let alloc = resource_load(d, total);
        let lang = m.view.lang;
        // The resources' column.
        let left = Rect::new(b.left, b.top, b.left + LEFT_W, b.bottom);
        g.fill_rectangle(Brush::solid(th.surface1), left);
        g.draw_line(&Pen::new(th.border, 1.0), PointF::new(left.right - 0.5, b.top), PointF::new(left.right - 0.5, b.bottom));
        g.draw_line(&Pen::new(th.border, 1.0), PointF::new(left.left, b.top + TOP - 0.5), PointF::new(left.right, b.top + TOP - 0.5));
        let fb = Font::with_dip("Segoe UI", 12.0, FontStyle::BOLD);
        let f10 = Font::with_dip("Segoe UI", 10.0, FontStyle::REGULAR);
        for (i, r) in d.resources.iter().enumerate() {
            let y = b.top + TOP + i as f32 * LOAD_ROW;
            let a = Rect::new(left.left + 8.0, y + LOAD_ROW / 2.0 - 12.0, left.left + 32.0, y + LOAD_ROW / 2.0 + 12.0);
            g.fill_ellipse(Brush::solid(Color::from_hex(&r.color).unwrap_or(th.primary)), a);
            g.draw_string(&initial(&r.name), &Font::with_dip("Segoe UI", 10.0, FontStyle::BOLD), Brush::solid(Color::WHITE), a, &StringFormat::centered());
            g.draw_string(&r.name, &fb, Brush::solid(th.text_primary), Rect::new(a.right + 8.0, y + 10.0, left.right - 6.0, y + 28.0), &one(StringAlignment::Near));
            g.draw_string(&format!("{}%", (r.capacity * 100.0).round()), &f10, Brush::solid(th.text_tertiary), Rect::new(a.right + 8.0, y + 28.0, left.right - 6.0, y + 44.0), &one(StringAlignment::Near));
            g.draw_line(&Pen::new(hex(0xf1f3f4), 1.0), PointF::new(left.left, y + LOAD_ROW - 0.5), PointF::new(left.right, y + LOAD_ROW - 0.5));
        }
        // The histogram (the web's canvas: white, its own literals).
        let cv = Rect::new(left.right, b.top, b.right, b.bottom);
        let saved = g.save();
        g.intersect_clip(cv);
        let h = TOP + d.resources.len() as f32 * LOAD_ROW;
        g.fill_rectangle(Brush::solid(Color::WHITE), Rect::new(cv.left, cv.top, cv.right, cv.top + h));
        let x_of = |day: i32| cv.left + day as f32 * day_w - self.scroll_x;
        for day in 0..=total {
            let date = start.add_days(day as i64);
            let x = x_of(day);
            if date.is_weekend() {
                g.fill_rectangle(Brush::solid(hex(0xf8f9fa)), Rect::new(x, cv.top + TOP, x + day_w, cv.top + h));
            }
            if date.weekday_mon0() == 0 {
                g.draw_line(&Pen::new(hex(0xe8eaed), 0.5), PointF::new(x, cv.top), PointF::new(x, cv.top + h));
            }
        }
        let mut cur = 0;
        for day in 0..=total {
            let date = start.add_days(day as i64);
            if date.month != cur {
                cur = date.month;
                g.draw_string(&month_short(date, lang), &Font::with_dip("Segoe UI", 10.0, FontStyle::BOLD), Brush::solid(hex(0x5f6368)), Rect::new(x_of(day) + 3.0, cv.top + 4.0, x_of(day) + 200.0, cv.top + 18.0), &one(StringAlignment::Near));
            }
        }
        for (i, r) in d.resources.iter().enumerate() {
            let arr = alloc.get(&r.id).cloned().unwrap_or_default();
            let cap = if r.capacity > 0.0 { r.capacity } else { 1.0 };
            let peak = arr.iter().copied().fold(cap, f64::max);
            let base = cv.top + TOP + i as f32 * LOAD_ROW + LOAD_ROW - 8.0;
            let max_h = LOAD_ROW - 18.0;
            let top = cv.top + TOP + i as f32 * LOAD_ROW;
            g.draw_line(&Pen::new(hex(0xe8eaed), 1.0), PointF::new(cv.left, top), PointF::new(cv.right, top));
            let cap_y = base - (cap / peak) as f32 * max_h;
            g.draw_line(&Pen::new(hex(0x9aa0a6), 1.0).with_dash_pattern(&[4.0, 3.0]), PointF::new(cv.left, cap_y), PointF::new(cv.right, cap_y));
            for (day, load) in arr.iter().enumerate() {
                if *load <= 0.0 {
                    continue;
                }
                let bh = (*load / peak) as f32 * max_h;
                let c = if *load > cap { Color::argb(0xcc, 0xd9, 0x30, 0x25) } else { Color::argb(0xaa, 0x1a, 0x73, 0xe8) };
                let x = x_of(day as i32);
                g.fill_rectangle(Brush::solid(c), Rect::new(x + 0.5, base - bh, x + 0.5 + (day_w - 1.0).max(1.0), base));
            }
        }
        g.restore(saved);
        if let Some((bar, rail)) = self.hbar() {
            bar.paint(e.graphics as &dyn Canvas, rail, WidgetState::REST);
        }
        e.raise(self, "OnPaint");
    }

    fn on_mouse_wheel(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        self.scroll_x = (self.scroll_x - e.args().delta * 100.0).max(0.0);
        self.invalidate();
        e.raise(&*self, "OnMouseWheel");
    }

    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (ax, ay) = (e.args().x + self.bounds.left, e.args().y + self.bounds.top);
        if let Some((bar, rail)) = self.hbar().filter(|(_, r)| r.contains(ax, ay)) {
            let page = self.bounds.right - self.bounds.left - LEFT_W;
            match bar.part_at(rail, ax, ay) {
                Some(ScrollPart::Thumb) => self.drag = Some(ax - bar.thumb_rect(rail).left),
                Some(ScrollPart::PageLow) => self.scroll_x -= page,
                Some(ScrollPart::PageHigh) => self.scroll_x += page,
                Some(ScrollPart::ArrowLow) => self.scroll_x -= 40.0,
                Some(ScrollPart::ArrowHigh) => self.scroll_x += 40.0,
                None => {}
            }
            self.scroll_x = self.scroll_x.max(0.0);
            self.invalidate();
        }
        e.raise(&*self, "OnMouseDown");
    }

    fn on_mouse_move(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        if let (Some(grab), Some((bar, rail))) = (self.drag, self.hbar()) {
            let ax = e.args().x + self.bounds.left;
            let thumb = bar.thumb_rect(rail);
            let travel = (rail.right - rail.left) - (thumb.right - thumb.left);
            if travel > 0.0 {
                let max = (self.content_w() - (self.bounds.right - self.bounds.left - LEFT_W)).max(0.0);
                self.scroll_x = ((ax - grab - rail.left).clamp(0.0, travel) / travel) * max;
                self.invalidate();
            }
        }
        e.raise(&*self, "OnMouseMove");
    }

    fn on_mouse_up(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        self.drag = None;
        e.raise(&*self, "OnMouseUp");
    }
}
