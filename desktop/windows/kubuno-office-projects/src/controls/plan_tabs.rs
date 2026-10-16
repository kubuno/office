//! `PlanTabs` - the plan views' tab strip, centred at the top of the plan area (the web's `@ui` `Tabs`,
//! underline variant, size md: 48-px tabs of equal width, 14-px medium labels with a 16-px icon, the active
//! one in the primary colour over a 3-px indicator, on `bg-surface-1` with a bottom rule).

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::graphics::{Brush, Font, FontStyle, Pen, PointF, StringAlignment, StringFormat, StringFormatFlags, StringTrimming};
use kubuno_desktop::ui::{Canvas, Rect, Size};
use kubuno_desktop::views::component::{AccessiblePart, Control, ControlCore, EventCx, PaintEventCx};
use kubuno_desktop::views::events::Event;

use crate::controls::model::Palette;

const TAB_H: f32 = 48.0;
const PAD: f32 = 16.0;
const GAP: f32 = 4.0;

/// A tab was clicked.
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct PlanTabEventArgs {
    pub id: String,
}

#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Projects")]
#[toolbox(icon = "tabs")]
#[default_event("TabSelected")]
pub struct PlanTabs {
    base: ControlCore,
    /// The tabs, one per line: `id|label|icon`.
    #[property(bindable)]
    #[category("Data")]
    pub items: String,
    /// The selected tab's id.
    #[property(bindable)]
    #[category("Data")]
    pub selected: String,
    /// Occurs when a tab is clicked.
    #[event]
    #[category("Action")]
    pub tab_selected: Event<PlanTabEventArgs>,
    hot: Option<usize>,
}

impl Default for PlanTabs {
    fn default() -> Self {
        Self {
            base: ControlCore::default(),
            items: "gantt|Gantt|BarChart2\nboard|Tableau|KanbanSquare\ncalendar|Calendrier|CalendarDays\nresources|Ressources|Users\npert|Réseau|Network\nroadmap|Roadmap|Milestone".into(),
            selected: "gantt".into(),
            tab_selected: Event::default(),
            hot: None,
        }
    }
}

fn icon_name(s: &str) -> &'static str {
    match s {
        "BarChart2" => "BarChart2",
        "KanbanSquare" => "KanbanSquare",
        "CalendarDays" => "CalendarDays",
        "Users" => "Users",
        "Network" => "Network",
        "Milestone" => "Milestone",
        _ => "Circle",
    }
}

impl PlanTabs {
    fn tabs(&self) -> Vec<(String, String, &'static str)> {
        self.items
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                let mut p = l.split('|');
                let id = p.next().unwrap_or("").to_string();
                let label = p.next().unwrap_or("").to_string();
                (id, label, icon_name(p.next().unwrap_or("")))
            })
            .collect()
    }

    fn font() -> Font {
        Font::with_dip("Segoe UI", 14.0, FontStyle::BOLD)
    }

    /// Each tab's rectangle (equal widths, centred), in the control's coordinates.
    fn layout(&self) -> Vec<Rect> {
        let tabs = self.tabs();
        let w = self.size().width;
        let widest = tabs.iter().map(|(_, l, _)| kubuno_desktop::ui::graphics::text::approximate_measure(l, &Self::font(), None).0 + 16.0 + 6.0 + 2.0 * PAD).fold(0.0f32, f32::max);
        let total = widest * tabs.len() as f32 + GAP * tabs.len().saturating_sub(1) as f32;
        let mut x = ((w - total) / 2.0).max(0.0);
        tabs.iter()
            .map(|_| {
                let r = Rect::new(x, 0.0, x + widest, TAB_H);
                x += widest + GAP;
                r
            })
            .collect()
    }
}

impl Control for PlanTabs {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, proposed: Size) -> Size {
        Size { width: proposed.width, height: TAB_H + 1.0 }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        let b = e.clip_rectangle;
        let th = Palette::of(kubuno_desktop::Application::theme().mode == kubuno_desktop::ui::ThemeMode::Dark);
        let g = e.graphics;
        g.fill_rectangle(Brush::solid(th.surface1), b);
        g.draw_line(&Pen::new(th.border, 1.0), PointF::new(b.left, b.bottom - 0.5), PointF::new(b.right, b.bottom - 0.5));
        let rects = self.layout();
        let fmt = StringFormat { alignment: StringAlignment::Near, line_alignment: StringAlignment::Center, trimming: StringTrimming::None, flags: StringFormatFlags::NO_WRAP };
        for (i, ((id, label, icon), r)) in self.tabs().into_iter().zip(rects).enumerate() {
            let r = Rect::new(b.left + r.left, b.top + r.top, b.left + r.right, b.top + r.bottom);
            let active = id == self.selected;
            if self.hot == Some(i) {
                g.fill_rectangle(Brush::solid(if active { th.primary.with_alpha(0.08) } else { th.surface2 }), r);
            }
            let ink = if active { th.primary } else if self.hot == Some(i) { th.text_primary } else { th.text_secondary };
            let tw = g.measure_string(&label, &Self::font(), None, &fmt).width;
            let x = r.left + (r.right - r.left - (16.0 + 6.0 + tw)) / 2.0;
            g.draw_icon(icon, Rect::new(x, r.top, x + 16.0, r.bottom), 16.0, ink);
            g.draw_string(&label, &Self::font(), Brush::solid(ink), Rect::new(x + 22.0, r.top, x + 22.0 + tw + 4.0, r.bottom), &fmt);
            if active {
                g.fill_rounded_rectangle(Brush::solid(th.primary), Rect::new(r.left, r.bottom - 3.0, r.right, r.bottom + 2.0), 3.0);
            }
        }
        g.draw_line(&Pen::new(th.border, 1.0), PointF::new(b.left, b.bottom - 0.5), PointF::new(b.right, b.bottom - 0.5));
        e.raise(self, "OnPaint");
    }

    fn on_mouse_move(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y) = (e.args().x, e.args().y);
        let hot = self.layout().iter().position(|r| r.contains(x, y));
        if hot != self.hot {
            self.hot = hot;
            self.invalidate();
        }
        e.raise(&*self, "OnMouseMove");
    }

    fn on_mouse_leave(&mut self, e: &mut EventCx<'_, kubuno_desktop::views::events::EmptyEventArgs>) {
        self.hot = None;
        self.invalidate();
        e.raise(&*self, "OnMouseLeave");
    }

    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y) = (e.args().x, e.args().y);
        let rects = self.layout();
        if let Some(i) = rects.iter().position(|r| r.contains(x, y)) {
            if let Some((id, _, _)) = self.tabs().into_iter().nth(i) {
                self.selected = id.clone();
                self.invalidate();
                self.raise_tab_selected(PlanTabEventArgs { id });
            }
        }
        e.raise(&*self, "OnMouseDown");
    }

    fn cursor_at(&self, _x: f32, _y: f32) -> Option<kubuno_desktop::controls::host::Cursor> {
        Some(kubuno_desktop::controls::host::Cursor::Hand)
    }

    fn accessible_parts(&self) -> Vec<AccessiblePart> {
        use kubuno_desktop::controls::host::access::AccessRole;
        let rects = self.layout();
        self.tabs().into_iter().zip(rects).map(|((_, l, _), r)| AccessiblePart { name: l, role: AccessRole::Tab, bounds: r }).collect()
    }
}
