//! `NetworkView` — the plan as a network (precedence) diagram (`web/src/project/NetworkView.tsx`): one card per
//! activity (phases become frames around their activities), one arrow per link, on a free workspace:
//!
//! * the background pans (left or middle button), the wheel scrolls, Ctrl + wheel zooms around the pointer;
//! * a card selects on a click (a second click deselects) and moves when dragged (snap-to-grid optional); a
//!   frame's header moves every card inside it; the hand-placed cards are remembered per project;
//! * the right port of a card draws a link to the card it is released on (the scheduler re-runs; a loop is
//!   refused with a notice); a hovered link shows its cut button;
//! * right clicks: a card or a frame opens the task's menu, a link its menu (type, lag, delete), the background
//!   the view's menu (timescale, snap, fit, reset, rearrange);
//! * the zoom pill at the bottom right: − / % / +, the time-scaled mode, snap, fit, rearrange.
//!
//! The geometry is the core's (`kubuno_office_projects_core::network`); the per-project state is kept by
//! `kubuno_office_projects_common::model::network_store`.

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::graphics::{Brush, Color, Font, FontStyle, Graphics, GraphicsPath, Pen, PointF, StringAlignment, StringFormat, StringFormatFlags, StringTrimming};
use kubuno_desktop::ui::{Canvas, Rect, Size};
use kubuno_desktop::views::component::{AccessiblePart, Component, Control, ControlCore, EventCx, PaintEventCx};
use kubuno_desktop::views::events::Event;
use kubuno_office_projects_common::model::network_store::{NetworkState, NetworkStore};
use kubuno_office_projects_core::date::{self, DateFormat};
use kubuno_office_projects_core::network::{self as net, Hit, Network, Pt, Seg, Viewport, HULL_HEAD, NODE_H, PORT_R};
use kubuno_office_projects_core::view;

use crate::controls::gantt::{GanttContextEventArgs, GanttEditedEventArgs, GanttStateEventArgs};
use crate::controls::model::{hex, sample_model, Palette, Shared};
use crate::Resources;

const FONT: &str = "Segoe UI";
/// The cards' colour strip (`TASK_COLOR`, `CRITICAL_CLR`, `MILESTONE_CLR`: literals, the same in both themes).
const TASK_CLR: u32 = 0x1a73e8;
const CRITICAL_CLR: u32 = 0xd93025;
const MILESTONE_CLR: u32 = 0xea4335;
const LINK_CLR: u32 = 0x9aa0a6;
/// How long a notice stays (ms).
const NOTICE_MS: i64 = 2800;
/// The zoom pill: its height and its margin from the corner.
const PILL_H: f32 = 26.0;
const PILL_MARGIN: f32 = 12.0;

fn font(size: f32, bold: bool) -> Font {
    Font::with_dip(FONT, size, if bold { FontStyle::BOLD } else { FontStyle::REGULAR })
}

fn one_line(align: StringAlignment) -> StringFormat {
    StringFormat { alignment: align, line_alignment: StringAlignment::Center, trimming: StringTrimming::EllipsisCharacter, flags: StringFormatFlags::NO_WRAP }
}

fn now_ms() -> i64 {
    kubuno_office_projects_common::platform::current().clock.now_ms()
}

fn rf(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect::new(x as f32, y as f32, (x + w) as f32, (y + h) as f32)
}

fn pf(p: Pt) -> PointF {
    PointF::new(p.x as f32, p.y as f32)
}

/// A gesture in progress.
#[derive(Debug, Clone, PartialEq)]
enum Gesture {
    None,
    /// Panning: the press point and the viewport's offset then.
    Pan { x: f64, y: f64, tx: f64, ty: f64 },
    /// A card (or a frame's cards) pressed: moves once past the threshold, else selects on release.
    Drag { select: String, items: Vec<(String, Pt)>, sx: f64, sy: f64, moved: bool },
    /// Drawing a link from `source`'s right port.
    Connect { source: String, start: Pt, end: Pt, over: Option<String> },
    /// The cut button of a link pressed.
    Cut(String),
    /// A button of the zoom pill pressed.
    Pill(usize),
}

/// The zoom pill's parts, left to right.
#[derive(Debug, Clone, Copy, PartialEq)]
enum PillPart {
    ZoomOut,
    Percent,
    ZoomIn,
    Timescale,
    Snap,
    Fit,
    Rearrange,
}

const PILL: [PillPart; 7] = [PillPart::ZoomOut, PillPart::Percent, PillPart::ZoomIn, PillPart::Timescale, PillPart::Snap, PillPart::Fit, PillPart::Rearrange];

#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Projects")]
#[toolbox(icon = "network")]
#[default_event("Edited")]
pub struct NetworkView {
    base: ControlCore,
    /// Occurs after a link was drawn or cut.
    #[event]
    #[category("Action")]
    pub edited: Event<GanttEditedEventArgs>,
    /// Occurs when the selection changed (the window re-reads the inspector).
    #[event]
    #[category("Action")]
    pub state_changed: Event<GanttStateEventArgs>,
    /// Occurs on a right click: `row` (a card or a frame: the task's menu), `net_edge` (`task` = the link's id),
    /// `net_canvas` (the background).
    #[event]
    #[category("Action")]
    pub context_menu_requested: Event<GanttContextEventArgs>,
    model: Option<Shared>,
    bounds: Rect,
    /// The project whose state is loaded (`None`: not yet).
    loaded_for: Option<String>,
    state: NetworkState,
    vp: Viewport,
    /// Whether the viewport was framed (or restored) for this project.
    framed: bool,
    scene: Network,
    gesture: Gesture,
    hover_edge: Option<String>,
    hover_port: Option<String>,
    hover_pill: Option<usize>,
    notice: Option<(String, i64)>,
}

impl Default for NetworkView {
    fn default() -> Self {
        Self {
            base: ControlCore::default(),
            edited: Event::default(),
            state_changed: Event::default(),
            context_menu_requested: Event::default(),
            model: None,
            bounds: Rect::default(),
            loaded_for: None,
            state: NetworkState::default(),
            vp: Viewport::default(),
            framed: false,
            scene: Network::default(),
            gesture: Gesture::None,
            hover_edge: None,
            hover_port: None,
            hover_pill: None,
            notice: None,
        }
    }
}

impl NetworkView {
    pub fn set_model(&mut self, model: Shared) {
        self.model = Some(model);
        self.invalidate();
    }

    fn store(&self) -> Option<NetworkStore> {
        if self.design_mode() {
            None
        } else {
            NetworkStore::for_user()
        }
    }

    fn save(&mut self) {
        self.state.viewport = Some(self.vp);
        let (Some(id), Some(store)) = (self.loaded_for.clone(), self.store()) else { return };
        // Best effort: the layout stays in memory when the file cannot be written.
        let _ = store.write(&id, &self.state);
    }

    /// Loads the project's state when the open project changed.
    fn sync_project(&mut self) {
        let id = self.model.as_ref().and_then(|m| m.borrow().data().map(|d| d.project.id.clone()));
        if id == self.loaded_for {
            return;
        }
        self.state = match (&id, self.store()) {
            (Some(id), Some(s)) => s.read(id),
            _ => NetworkState::default(),
        };
        self.framed = self.state.viewport.is_some();
        self.vp = self.state.viewport.unwrap_or_default();
        self.loaded_for = id;
        self.gesture = Gesture::None;
    }

    /// Re-computes the scene from the model.
    fn rebuild(&mut self) {
        self.sync_project();
        let Some(model) = self.model.clone() else {
            self.scene = Network::default();
            return;
        };
        let m = model.borrow();
        let Some(d) = m.data() else {
            self.scene = Network::default();
            return;
        };
        let shown = view::display_tasks(d, &m.view.filter);
        self.scene = Network::build(d, &shown, &self.state.manual, self.state.timescale);
    }

    fn view_size(&self) -> (f64, f64) {
        ((self.bounds.right - self.bounds.left) as f64, (self.bounds.bottom - self.bounds.top) as f64)
    }

    /// « Ajuster à l'écran ».
    pub fn fit_to_content(&mut self) {
        let (w, h) = self.view_size();
        self.vp = if self.scene.nodes.is_empty() { Viewport::default() } else { net::fit(&self.scene.content_boxes(), w, h) };
        self.framed = true;
        self.save();
        self.invalidate();
    }

    /// « Réinitialiser la vue ».
    pub fn reset_view(&mut self) {
        self.vp = Viewport::default();
        self.save();
        self.invalidate();
    }

    /// « Échelle de temps » (re-frames, the time-scaled mode spreads far wider).
    pub fn toggle_timescale(&mut self) {
        self.state.timescale = !self.state.timescale;
        self.rebuild();
        self.fit_to_content();
    }

    /// « Magnétisme sur la grille » (not in the time-scaled mode).
    pub fn toggle_snap(&mut self) {
        if self.state.timescale {
            return;
        }
        self.state.snap = !self.state.snap;
        self.save();
        self.invalidate();
    }

    /// « Réorganiser automatiquement »: forgets the hand-placed cards.
    pub fn rearrange(&mut self) {
        if !self.can_rearrange() {
            return;
        }
        self.state.manual.clear();
        self.save();
        self.rebuild();
        self.invalidate();
    }

    pub fn timescale(&self) -> bool {
        self.state.timescale
    }

    pub fn snap(&self) -> bool {
        self.state.snap
    }

    pub fn can_rearrange(&self) -> bool {
        !self.state.timescale && !self.state.manual.is_empty()
    }

    fn flash(&mut self, text: &str) {
        self.notice = Some((text.to_string(), now_ms() + NOTICE_MS));
        kubuno_desktop::controls::host::request_repaint_after(NOTICE_MS as u32 + 20);
    }

    fn selected(&self) -> Option<String> {
        self.model.as_ref().and_then(|m| m.borrow().selected_task())
    }

    fn select(&mut self, id: Option<String>) {
        if let Some(m) = &self.model {
            m.borrow_mut().view.selected = id.clone();
        }
        self.raise_state_changed(GanttStateEventArgs { selected: id.unwrap_or_default(), revision: 0 });
        self.invalidate();
    }

    // ── The zoom pill ────────────────────────────────────────────────────────

    fn pill_widths(&self) -> [f32; 7] {
        [19.0, 40.0, 19.0, 25.0, 25.0, 25.0, 25.0]
    }

    /// The pill's frame and each part's rectangle (control coordinates).
    fn pill_layout(&self) -> (Rect, Vec<Rect>) {
        let widths = self.pill_widths();
        let inner: f32 = widths.iter().sum::<f32>() + 4.0 * (widths.len() as f32 - 1.0);
        let w = inner + 16.0 + 2.0;
        let (vw, vh) = self.view_size();
        let frame = Rect::new(vw as f32 - PILL_MARGIN - w, vh as f32 - PILL_MARGIN - PILL_H, vw as f32 - PILL_MARGIN, vh as f32 - PILL_MARGIN);
        let mut x = frame.left + 9.0;
        let parts = widths
            .iter()
            .map(|w| {
                let r = Rect::new(x, frame.top + 1.0, x + w, frame.bottom - 1.0);
                x += w + 4.0;
                r
            })
            .collect();
        (frame, parts)
    }

    fn pill_enabled(&self, part: PillPart) -> bool {
        match part {
            PillPart::Percent => false,
            PillPart::Snap => !self.state.timescale,
            PillPart::Rearrange => self.can_rearrange(),
            _ => true,
        }
    }

    fn pill_at(&self, x: f32, y: f32) -> Option<usize> {
        let (frame, parts) = self.pill_layout();
        if !frame.contains(x, y) || self.scene.nodes.is_empty() {
            return None;
        }
        parts.iter().position(|r| x >= r.left && x <= r.right)
    }

    fn pill_tip(&self, part: PillPart) -> String {
        match part {
            PillPart::ZoomOut => Resources::net_zoom_out().into(),
            PillPart::ZoomIn => Resources::net_zoom_in().into(),
            PillPart::Percent => format!("{}%", (self.vp.scale * 100.0).round()),
            PillPart::Timescale => if self.state.timescale { Resources::net_timescale_on() } else { Resources::net_timescale_off() }.into(),
            PillPart::Snap => {
                if self.state.timescale {
                    Resources::net_snap_na()
                } else if self.state.snap {
                    Resources::net_snap_on()
                } else {
                    Resources::net_snap_off()
                }
            }
            .into(),
            PillPart::Fit => Resources::net_fit().into(),
            PillPart::Rearrange => Resources::net_rearrange().into(),
        }
    }

    fn pill_press(&mut self, part: PillPart) {
        match part {
            PillPart::ZoomOut => {
                self.vp.zoom_by(0.9);
                self.save();
            }
            PillPart::ZoomIn => {
                self.vp.zoom_by(1.1);
                self.save();
            }
            PillPart::Timescale => self.toggle_timescale(),
            PillPart::Snap => self.toggle_snap(),
            PillPart::Fit => self.fit_to_content(),
            PillPart::Rearrange => self.rearrange(),
            PillPart::Percent => {}
        }
        self.invalidate();
    }

    // ── Painting ─────────────────────────────────────────────────────────────

    fn paint_dots(&self, g: &Graphics<'_>, th: &Palette, b: Rect) {
        let step = 24.0 * self.vp.scale;
        if step < 2.0 {
            return;
        }
        // `radial-gradient(1px)` tiles positioned at (tx, ty): a dot in the middle of each tile.
        let ox = (self.vp.tx + step / 2.0).rem_euclid(step);
        let oy = (self.vp.ty + step / 2.0).rem_euclid(step);
        let mut rects = Vec::new();
        let mut y = oy;
        while y < (b.bottom - b.top) as f64 {
            let mut x = ox;
            while x < (b.right - b.left) as f64 {
                let (cx, cy) = (b.left + x as f32, b.top + y as f32);
                rects.push(Rect::new(cx - 0.8, cy - 0.8, cx + 0.8, cy + 0.8));
                x += step;
            }
            y += step;
        }
        g.fill_rectangles(Brush::solid(th.border), &rects);
    }

    fn arrow(&self, g: &Graphics<'_>, color: Color, stroke: f64, tail: Pt, tip: Pt) {
        // The marker: `M0 0 L10 5 L0 10 z` in an 8 × stroke box, its point at x = 9 (10 % past the end).
        let (dx, dy) = (tip.x - tail.x, tip.y - tail.y);
        let len = dx.hypot(dy);
        if len == 0.0 {
            return;
        }
        let (ux, uy) = (dx / len, dy / len);
        let k = 8.0 * stroke / 10.0;
        let base = Pt::new(tip.x - ux * 9.0 * k, tip.y - uy * 9.0 * k);
        let point = Pt::new(base.x + ux * 10.0 * k, base.y + uy * 10.0 * k);
        let (nx, ny) = (-uy * 5.0 * k, ux * 5.0 * k);
        g.fill_polygon(Brush::solid(color), &[pf(point), PointF::new((base.x + nx) as f32, (base.y + ny) as f32), PointF::new((base.x - nx) as f32, (base.y - ny) as f32)]);
    }

    fn path_of(route: &[Pt]) -> GraphicsPath {
        let mut p = GraphicsPath::new();
        if let Some(first) = route.first() {
            p.move_to(pf(*first));
            for s in net::rounded_path(route) {
                match s {
                    Seg::Line(q) => p.line_to(pf(q)),
                    Seg::Quad(c, q) => p.quad_to(pf(c), pf(q)),
                }
            }
        }
        p
    }

    fn paint_edges(&self, g: &Graphics<'_>, th: &Palette) {
        let lbl_font = font(9.0, false);
        for e in &self.scene.edges {
            let hovered = self.hover_edge.as_deref() == Some(e.dep_id.as_str());
            let color = if hovered { th.primary } else if e.critical { hex(CRITICAL_CLR) } else { hex(LINK_CLR) };
            let stroke = if e.critical || hovered { 2.0 } else { 1.5 };
            g.draw_path(&Pen::new(color, stroke as f32), &Self::path_of(&e.route));
            if let [.., a, b] = e.route.as_slice() {
                self.arrow(g, color, stroke, *a, *b);
            }
            let mid = net::mid_point(&e.route);
            if !e.label.is_empty() && !hovered {
                let r = rf(mid.x - 18.0, mid.y - 8.0, 36.0, 16.0);
                g.fill_rounded_rectangle(Brush::solid(Color::WHITE), r, 8.0);
                g.draw_rounded_rectangle(&Pen::new(if e.critical { hex(CRITICAL_CLR) } else { hex(0xdadce0) }, 1.0), r, 8.0);
                g.draw_string(&e.label, &lbl_font, Brush::solid(hex(0x5f6368)), r, &StringFormat::centered());
            }
            if hovered {
                g.fill_ellipse(Brush::solid(hex(CRITICAL_CLR)), rf(mid.x - 9.0, mid.y - 9.0, 18.0, 18.0));
                let pen = Pen::new(Color::WHITE, 1.8);
                g.draw_line(&pen, pf(Pt::new(mid.x - 3.5, mid.y - 3.5)), pf(Pt::new(mid.x + 3.5, mid.y + 3.5)));
                g.draw_line(&pen, pf(Pt::new(mid.x + 3.5, mid.y - 3.5)), pf(Pt::new(mid.x - 3.5, mid.y + 3.5)));
            }
        }
        if let Gesture::Connect { start, end, over, .. } = &self.gesture {
            let dst = match over {
                Some(id) => {
                    let p = self.scene.pos_of(id);
                    Pt::new(p.x, p.y + NODE_H / 2.0)
                }
                None => *end,
            };
            let route = net::orth_route(*start, dst);
            g.draw_path(&Pen::new(th.primary, 2.0).with_dash_pattern(&[2.5, 2.0]), &Self::path_of(&route));
            if let [.., a, b] = route.as_slice() {
                self.arrow(g, th.primary, 2.0, *a, *b);
            }
        }
    }

    fn paint_hulls(&self, g: &Graphics<'_>, th: &Palette, selected: Option<&str>, progress: &std::collections::HashMap<String, i32>) {
        for h in &self.scene.hulls {
            let sel = selected == Some(h.id.as_str());
            let r = rf(h.x, h.y, h.w, h.h);
            g.fill_rounded_rectangle(Brush::solid(th.primary.with_alpha(0.05)), r, 12.0);
            g.draw_rounded_rectangle(&Pen::new(if sel { th.primary } else { th.primary.with_alpha(0.25) }, 1.0), r, 12.0);
            if sel {
                g.draw_rounded_rectangle(&Pen::new(th.primary.with_alpha(0.4), 2.0), Rect::new(r.left - 1.5, r.top - 1.5, r.right + 1.5, r.bottom + 1.5), 13.5);
            }
            let head = rf(h.x + 10.0, h.y, h.w - 20.0, HULL_HEAD);
            let fb = font(11.0, true);
            let name_w = g.measure_string(&h.name, &fb, None, &one_line(StringAlignment::Near)).width.min(head.right - head.left);
            g.draw_string(&h.name, &fb, Brush::solid(th.primary), Rect::new(head.left, head.top, head.left + name_w + 1.0, head.bottom), &one_line(StringAlignment::Near));
            if let Some(p) = progress.get(&h.id) {
                g.draw_string(&format!("· {p}%"), &font(11.0, false), Brush::solid(th.primary.with_alpha(0.7)), Rect::new(head.left + name_w + 6.0, head.top, head.right, head.bottom), &one_line(StringAlignment::Near));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_card(&self, g: &Graphics<'_>, th: &Palette, n: &net::Node, t: &kubuno_office_projects_core::model::Task, pct: i32, sel: bool, target: bool, start: kubuno_office_projects_core::date::Date, lang: kubuno_office_projects_core::date::Lang) {
        let r = rf(n.rect.x, n.rect.y, n.rect.w, n.rect.h);
        let clr = hex(if t.is_milestone() {
            MILESTONE_CLR
        } else if t.is_critical {
            CRITICAL_CLR
        } else {
            TASK_CLR
        });
        // shadow-sm / shadow-lg
        let sh = if target { 4.0 } else { 1.0 };
        g.fill_rounded_rectangle(Brush::solid(Color::BLACK.with_alpha(if target { 0.10 } else { 0.06 })), Rect::new(r.left, r.top + sh, r.right, r.bottom + sh), 8.0);
        g.fill_rounded_rectangle(Brush::solid(th.surface0), r, 8.0);
        g.with_saved(|g| {
            let mut clip = GraphicsPath::new();
            clip.add_rounded_rectangle(r, 8.0);
            g.intersect_clip_path(&clip);
            g.fill_rectangle(Brush::solid(clr), Rect::new(r.left, r.top, r.right, r.top + 6.0));
        });
        g.draw_rounded_rectangle(&Pen::new(if sel { th.primary } else { th.border }, 1.0), r, 8.0);
        if sel || target {
            g.draw_rounded_rectangle(&Pen::new(th.primary, 2.0), Rect::new(r.left - 1.0, r.top - 1.0, r.right + 1.0, r.bottom + 1.0), 9.0);
        }
        let (x0, x1) = (r.left + 10.0, r.right - 10.0);
        g.draw_string(&t.name, &font(11.0, true), Brush::solid(th.text_primary), Rect::new(x0, r.top + 12.0, x1, r.top + 28.5), &one_line(StringAlignment::Near));
        let dates = format!("{} → {}", date::format(view::sched_start(t, start), DateFormat::DayMonth, lang), date::format(view::sched_end(t, start), DateFormat::DayMonth, lang));
        let f10 = font(10.0, false);
        g.draw_string(&dates, &f10, Brush::solid(th.text_secondary), Rect::new(x0, r.top + 28.5, x1, r.top + 43.5), &one_line(StringAlignment::Near));
        let left = format!("{} · {pct}%", Resources::days_short().replace("{n}", &t.duration_days.to_string()));
        // `justify-between` without shrinking: on a narrow card the badge follows the text and overflows.
        let left_w = g.measure_string(&left, &f10, None, &one_line(StringAlignment::Near)).width;
        g.draw_string(&left, &f10, Brush::solid(th.text_secondary), Rect::new(x0, r.top + 43.5, x0 + left_w + 2.0, r.top + 58.5), &one_line(StringAlignment::Near));
        if let (Some(fl), false) = (t.total_float, t.is_milestone()) {
            let text = Resources::net_float().replace("{n}", &fl.to_string());
            let tw = g.measure_string(&text, &f10, None, &one_line(StringAlignment::Near)).width + 8.0;
            let bx = (x1 - tw).max(x0 + left_w);
            let badge = Rect::new(bx, r.top + 44.5, bx + tw, r.top + 57.5);
            let (bg, fg) = if fl <= 0 { (th.danger.with_alpha(0.1), th.danger) } else { (th.surface2, th.text_tertiary) };
            g.fill_rounded_rectangle(Brush::solid(bg), badge, 4.0);
            g.draw_string(&text, &f10, Brush::solid(fg), badge, &StringFormat::centered());
        }
        // The ports: the left one receives links, the right one starts them.
        let cy = r.top + NODE_H as f32 / 2.0;
        for (cx, on, hover) in [(r.left, n.has_in, false), (r.right, n.has_out, self.hover_port.as_deref() == Some(n.id.as_str()))] {
            let rad = PORT_R as f32 * if hover { 1.25 } else { 1.0 };
            let c = Rect::new(cx - rad, cy - rad, cx + rad, cy + rad);
            g.fill_ellipse(Brush::solid(if on { clr } else { th.surface0 }), c);
            let inner = Rect::new(c.left + 1.0, c.top + 1.0, c.right - 1.0, c.bottom - 1.0);
            g.draw_ellipse(&Pen::new(if on { clr } else { hex(LINK_CLR) }, 2.0), inner);
        }
    }

    fn paint_ruler(&self, g: &Graphics<'_>, th: &Palette, b: Rect, ticks: &[net::MonthTick]) {
        let band = Rect::new(b.left, b.top, b.right, b.top + net::RULER_H as f32);
        g.fill_rectangle(Brush::solid(th.surface0.with_alpha(0.9)), band);
        g.draw_line(&Pen::new(th.border, 1.0), PointF::new(band.left, band.bottom - 0.5), PointF::new(band.right, band.bottom - 0.5));
        let f = font(11.0, false);
        for t in ticks {
            let x = b.left + t.screen_x as f32;
            g.fill_rectangle(Brush::solid(th.border.with_alpha(0.7)), Rect::new(x, band.top, x + 1.0, band.bottom));
            g.draw_string(&t.label, &f, Brush::solid(th.text_secondary), Rect::new(x + 6.0, band.top, x + 200.0, band.bottom), &one_line(StringAlignment::Near));
        }
    }

    fn paint_pill(&self, g: &Graphics<'_>, th: &Palette, b: Rect) {
        let (frame, parts) = self.pill_layout();
        let off = |r: Rect| Rect::new(r.left + b.left, r.top + b.top, r.right + b.left, r.bottom + b.top);
        let fr = off(frame);
        g.fill_rounded_rectangle(Brush::solid(th.surface0), fr, 8.0);
        g.draw_rounded_rectangle(&Pen::new(th.border, 1.0), fr, 8.0);
        let f = font(12.0, false);
        for (i, part) in PILL.iter().enumerate() {
            let r = off(parts[i]);
            let enabled = self.pill_enabled(*part) || *part == PillPart::Percent;
            let active = match part {
                PillPart::Timescale => self.state.timescale,
                PillPart::Snap => self.state.snap && !self.state.timescale,
                _ => false,
            };
            let hover = self.hover_pill == Some(i) && self.pill_enabled(*part);
            let mut c = if active { th.primary } else if hover { th.text_primary } else { th.text_secondary };
            if !enabled {
                c = c.with_alpha(0.4);
            }
            let icon = |name: &'static str| {
                let s = 13.0;
                let cx = (r.left + r.right) / 2.0;
                let cy = (r.top + r.bottom) / 2.0;
                g.draw_icon(name, Rect::new(cx - s / 2.0, cy - s / 2.0, cx + s / 2.0, cy + s / 2.0), s, c);
            };
            match part {
                PillPart::ZoomOut => g.draw_string("−", &f, Brush::solid(c), r, &StringFormat::centered()),
                PillPart::ZoomIn => g.draw_string("+", &f, Brush::solid(c), r, &StringFormat::centered()),
                PillPart::Percent => g.draw_string(&format!("{}%", (self.vp.scale * 100.0).round()), &f, Brush::solid(c), r, &StringFormat::centered()),
                PillPart::Timescale => icon("CalendarClock"),
                PillPart::Snap => icon("Magnet"),
                PillPart::Fit => icon("Maximize"),
                PillPart::Rearrange => icon("LayoutGrid"),
            }
        }
    }

    fn paint_toast(&self, g: &Graphics<'_>, b: Rect, text: &str, bg: Color) {
        let f = font(12.0, false);
        let w = g.measure_string(text, &f, None, &one_line(StringAlignment::Center)).width + 24.0;
        let cx = (b.left + b.right) / 2.0;
        let r = Rect::new(cx - w / 2.0, b.top + 8.0, cx + w / 2.0, b.top + 8.0 + 26.0);
        g.fill_rounded_rectangle(Brush::solid(Color::BLACK.with_alpha(0.12)), Rect::new(r.left, r.top + 2.0, r.right, r.bottom + 2.0), 13.0);
        g.fill_rounded_rectangle(Brush::solid(bg), r, 13.0);
        g.draw_string(text, &f, Brush::solid(Color::WHITE), r, &StringFormat::centered());
    }

    // ── Input helpers ────────────────────────────────────────────────────────

    fn world(&self, x: f32, y: f32) -> Pt {
        self.vp.to_world(x as f64, y as f64)
    }

    fn update_hover(&mut self, x: f32, y: f32) -> bool {
        let pill = self.pill_at(x, y);
        let (edge, port) = if pill.is_some() {
            (None, None)
        } else {
            match self.scene.hit(self.world(x, y), self.hover_edge.as_deref()) {
                Hit::Edge(id) | Hit::Cut(id) => (Some(id), None),
                Hit::OutPort(id) => (None, Some(id)),
                _ => (None, None),
            }
        };
        let changed = edge != self.hover_edge || port != self.hover_port || pill != self.hover_pill;
        self.hover_edge = edge;
        self.hover_port = port;
        self.hover_pill = pill;
        changed
    }

    fn edit(&mut self, f: impl FnOnce(&mut kubuno_office_projects_core::editor::Editor) -> Result<(), kubuno_office_projects_core::change::Refused>) -> bool {
        let Some(m) = self.model.clone() else { return false };
        let r = {
            let mut m = m.borrow_mut();
            let Some(ed) = m.ed.as_mut() else { return false };
            ed.set_now(now_ms());
            f(ed)
        };
        self.raise_edited(GanttEditedEventArgs { label: "network".into() });
        r.is_ok()
    }
}

impl Control for NetworkView {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, proposed: Size) -> Size {
        proposed
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        self.bounds = e.clip_rectangle;
        if self.model.is_none() && self.design_mode() {
            self.model = Some(std::rc::Rc::new(std::cell::RefCell::new(sample_model())));
        }
        // A link gesture or a card being moved paints the live scene, which the drag already rebuilt.
        if !matches!(self.gesture, Gesture::Drag { moved: true, .. }) {
            self.rebuild();
        }
        if self.notice.as_ref().is_some_and(|(_, until)| now_ms() >= *until) {
            self.notice = None;
        }
        let th = Palette::of(kubuno_desktop::Application::theme().mode == kubuno_desktop::ui::ThemeMode::Dark);
        let g = e.graphics;
        let b = self.bounds;
        let saved = g.save();
        g.intersect_clip(b);
        if self.scene.nodes.is_empty() {
            g.fill_rectangle(Brush::solid(th.surface0), b);
            let fi = Font::with_dip(FONT, 14.0, FontStyle::ITALIC);
            g.draw_string(Resources::net_no_tasks(), &fi, Brush::solid(th.text_tertiary), b, &StringFormat::centered());
            g.restore(saved);
            e.raise(self, "OnPaint");
            return;
        }
        if !self.framed {
            self.fit_to_content();
        }
        g.fill_rectangle(Brush::solid(th.surface1), b);
        self.paint_dots(g, &th, b);
        let Some(model) = self.model.clone() else {
            g.restore(saved);
            return;
        };
        let m = model.borrow();
        let Some(ed) = m.ed.as_ref() else {
            g.restore(saved);
            return;
        };
        let d = ed.data();
        let start = ed.project_start();
        let lang = m.view.lang;
        let selected = m.selected_task();
        let progress = kubuno_office_projects_core::rollup::effective_progress(&d.tasks);
        let ticks = if self.scene.timescale { net::month_ticks(self.scene.min_day, start, &self.vp, (b.right - b.left) as f64, lang) } else { Vec::new() };
        g.with_saved(|g| {
            g.translate_transform(b.left + self.vp.tx as f32, b.top + self.vp.ty as f32);
            g.scale_transform(self.vp.scale as f32, self.vp.scale as f32);
            if self.scene.timescale {
                let bottom = self.scene.grid_bottom() as f32;
                for t in &ticks {
                    g.fill_rectangle(Brush::solid(th.border.with_alpha(0.5)), Rect::new(t.world_x as f32, 0.0, t.world_x as f32 + 1.0, bottom));
                }
            }
            self.paint_hulls(g, &th, selected.as_deref(), &progress);
            self.paint_edges(g, &th);
            let over = match &self.gesture {
                Gesture::Connect { over, .. } => over.clone(),
                _ => None,
            };
            for n in &self.scene.nodes {
                let Some(t) = d.tasks.get(n.index) else { continue };
                let pct = progress.get(&t.id).copied().unwrap_or(t.progress);
                self.paint_card(g, &th, n, t, pct, selected.as_deref() == Some(n.id.as_str()), over.as_deref() == Some(n.id.as_str()), start, lang);
            }
        });
        drop(m);
        if self.scene.timescale {
            self.paint_ruler(g, &th, b, &ticks);
        }
        if matches!(self.gesture, Gesture::Connect { .. }) {
            self.paint_toast(g, b, Resources::net_connect_hint(), th.primary);
        }
        if let Some((text, _)) = &self.notice {
            self.paint_toast(g, b, text, th.danger);
        }
        self.paint_pill(g, &th, b);
        g.restore(saved);
        e.raise(self, "OnPaint");
    }

    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y, button) = (e.args().x, e.args().y, e.args().button);
        self.focus();
        if let Some(i) = self.pill_at(x, y) {
            if button == MouseButton::Left && self.pill_enabled(PILL[i]) {
                self.gesture = Gesture::Pill(i);
            }
            e.raise(&*self, "OnMouseDown");
            return;
        }
        let w = self.world(x, y);
        let hit = self.scene.hit(w, self.hover_edge.as_deref());
        let (ax, ay) = (x + self.bounds.left, y + self.bounds.top);
        match button {
            MouseButton::Middle => self.gesture = Gesture::Pan { x: x as f64, y: y as f64, tx: self.vp.tx, ty: self.vp.ty },
            MouseButton::Right => match hit {
                Hit::Node(id) | Hit::OutPort(id) | Hit::HullHead(id) => {
                    if let Some(m) = &self.model {
                        m.borrow_mut().view.selected = Some(id.clone());
                    }
                    self.invalidate();
                    self.raise_context_menu_requested(GanttContextEventArgs { kind: "row".into(), task: id, x: ax, y: ay });
                }
                Hit::Edge(id) | Hit::Cut(id) => self.raise_context_menu_requested(GanttContextEventArgs { kind: "net_edge".into(), task: id, x: ax, y: ay }),
                Hit::Background => self.raise_context_menu_requested(GanttContextEventArgs { kind: "net_canvas".into(), task: String::new(), x: ax, y: ay }),
            },
            MouseButton::Left => match hit {
                Hit::OutPort(id) => {
                    let p = self.scene.pos_of(&id);
                    let wdt = self.scene.node(&id).map(|n| n.rect.w).unwrap_or(net::NODE_W);
                    self.gesture = Gesture::Connect { source: id, start: Pt::new(p.x + wdt, p.y + NODE_H / 2.0), end: w, over: None };
                }
                Hit::Node(id) => {
                    let orig = self.scene.pos_of(&id);
                    self.gesture = Gesture::Drag { select: id.clone(), items: vec![(id, orig)], sx: x as f64, sy: y as f64, moved: false };
                }
                Hit::HullHead(id) => {
                    let items = self.scene.hulls.iter().find(|h| h.id == id).map(|h| h.ids.iter().map(|c| (c.clone(), self.scene.pos_of(c))).collect()).unwrap_or_default();
                    self.gesture = Gesture::Drag { select: id, items, sx: x as f64, sy: y as f64, moved: false };
                }
                Hit::Cut(id) => self.gesture = Gesture::Cut(id),
                Hit::Edge(_) => {}
                Hit::Background => self.gesture = Gesture::Pan { x: x as f64, y: y as f64, tx: self.vp.tx, ty: self.vp.ty },
            },
            MouseButton::None => {}
        }
        self.invalidate();
        e.raise(&*self, "OnMouseDown");
    }

    fn on_mouse_move(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y) = (e.args().x, e.args().y);
        let w = self.world(x, y);
        let mut gesture = std::mem::replace(&mut self.gesture, Gesture::None);
        match &mut gesture {
            Gesture::Connect { source, end, over, .. } => {
                *end = w;
                *over = self.scene.target_at(w, source);
                self.invalidate();
            }
            Gesture::Drag { items, sx, sy, moved, .. } => {
                // In the time-scaled mode the cards are bound to the schedule: a press only selects.
                if !self.state.timescale {
                    let (ddx, ddy) = (x as f64 - *sx, y as f64 - *sy);
                    if *moved || ddx.hypot(ddy) >= net::DRAG_THRESHOLD {
                        *moved = true;
                        let (mut dx, mut dy) = (ddx / self.vp.scale, ddy / self.vp.scale);
                        if self.state.snap {
                            if let Some((_, lead)) = items.first() {
                                dx = net::snap_to(lead.x + dx) - lead.x;
                                dy = net::snap_to(lead.y + dy) - lead.y;
                            }
                        }
                        for (id, orig) in items.iter() {
                            self.state.manual.insert(id.clone(), Pt::new((orig.x + dx).round(), (orig.y + dy).round()));
                        }
                        self.rebuild();
                        self.invalidate();
                    }
                }
            }
            Gesture::Pan { x: px, y: py, tx, ty } => {
                self.vp.tx = *tx + (x as f64 - *px);
                self.vp.ty = *ty + (y as f64 - *py);
                self.invalidate();
            }
            _ => {
                if self.update_hover(x, y) {
                    self.invalidate();
                }
            }
        }
        self.gesture = gesture;
        e.raise(&*self, "OnMouseMove");
    }

    fn on_mouse_up(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y) = (e.args().x, e.args().y);
        match std::mem::replace(&mut self.gesture, Gesture::None) {
            Gesture::Connect { source, over: Some(to), .. } => {
                if !self.edit(|ed| ed.add_dependency(&source, &to, true)) {
                    self.flash(Resources::net_link_refused());
                }
            }
            Gesture::Drag { select, moved, .. } => {
                if moved {
                    self.save();
                } else {
                    let again = self.selected().as_deref() == Some(select.as_str());
                    self.select(if again { None } else { Some(select) });
                }
            }
            Gesture::Pan { .. } => self.save(),
            Gesture::Cut(id) => {
                if self.scene.hit(self.world(x, y), Some(&id)) == Hit::Cut(id.clone()) {
                    self.hover_edge = None;
                    self.edit(|ed| ed.remove_dependencies(&[id]));
                }
            }
            Gesture::Pill(i) if self.pill_at(x, y) == Some(i) => self.pill_press(PILL[i]),
            _ => {}
        }
        self.update_hover(x, y);
        self.invalidate();
        e.raise(&*self, "OnMouseUp");
    }

    fn on_mouse_leave(&mut self, e: &mut EventCx<'_, kubuno_desktop::views::events::EmptyEventArgs>) {
        if self.gesture == Gesture::None && (self.hover_edge.is_some() || self.hover_port.is_some() || self.hover_pill.is_some()) {
            self.hover_edge = None;
            self.hover_port = None;
            self.hover_pill = None;
            self.invalidate();
        }
        e.raise(&*self, "OnMouseLeave");
    }

    fn on_mouse_wheel(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y, delta, mods) = (e.args().x as f64, e.args().y as f64, e.args().delta as f64, e.args().mods);
        if mods.ctrl {
            self.vp.zoom_at(x, y, if delta > 0.0 { 1.1 } else { 0.9 });
        } else if mods.shift {
            self.vp.tx += delta * 100.0;
        } else {
            self.vp.ty += delta * 100.0;
        }
        self.save();
        self.invalidate();
        e.raise(&*self, "OnMouseWheel");
    }

    fn on_key_down(&mut self, e: &mut EventCx<'_, KeyEventArgs>) {
        use kubuno_desktop::controls::host::vk;
        if e.args().key.0 == vk::ESCAPE && matches!(self.gesture, Gesture::Connect { .. } | Gesture::Drag { .. }) {
            self.gesture = Gesture::None;
            self.invalidate();
            e.args_mut().handled = true;
        }
        e.raise(&*self, "OnKeyDown");
    }

    fn cursor_at(&self, x: f32, y: f32) -> Option<kubuno_desktop::controls::host::Cursor> {
        use kubuno_desktop::controls::host::Cursor;
        Some(match &self.gesture {
            Gesture::Pan { .. } => Cursor::Move,
            Gesture::Connect { .. } => Cursor::Crosshair,
            Gesture::Drag { moved: true, .. } => Cursor::Move,
            _ => {
                if let Some(i) = self.pill_at(x, y) {
                    return Some(if self.pill_enabled(PILL[i]) { Cursor::Hand } else { Cursor::Arrow });
                }
                match self.scene.hit(self.world(x, y), self.hover_edge.as_deref()) {
                    Hit::OutPort(_) => Cursor::Crosshair,
                    Hit::Node(_) if self.state.timescale => Cursor::Hand,
                    Hit::Node(_) | Hit::HullHead(_) => Cursor::Move,
                    Hit::Edge(_) | Hit::Cut(_) => Cursor::Hand,
                    Hit::Background => Cursor::Arrow,
                }
            }
        })
    }

    fn tool_tip_at(&self, x: f32, y: f32) -> Option<String> {
        if let Some(i) = self.pill_at(x, y) {
            return Some(self.pill_tip(PILL[i]));
        }
        let m = self.model.as_ref()?.borrow();
        let d = m.data()?;
        match self.scene.hit(self.world(x, y), self.hover_edge.as_deref()) {
            Hit::OutPort(_) => Some(Resources::net_port_hint().into()),
            Hit::HullHead(_) => Some(Resources::net_phase_hint().into()),
            Hit::Cut(_) => Some(Resources::net_dep_delete().into()),
            Hit::Node(id) => {
                let n = self.scene.node(&id)?;
                let w = self.world(x, y);
                // The float badge's title, else the name's.
                let t = d.task(&id)?;
                if w.y >= n.rect.y + 43.5 && w.x >= n.rect.x + n.rect.w * 0.55 && t.total_float.is_some() && !t.is_milestone() {
                    Some(Resources::net_float_hint().into())
                } else {
                    Some(t.name.clone())
                }
            }
            _ => None,
        }
    }

    fn accessible_parts(&self) -> Vec<AccessiblePart> {
        use kubuno_desktop::controls::host::access::AccessRole;
        let mut out = Vec::new();
        let Some(m) = self.model.as_ref().map(|m| m.borrow()) else { return out };
        let Some(d) = m.data() else { return out };
        let to_screen = |x: f64, y: f64, w: f64, h: f64| {
            let a = self.vp.to_screen(Pt::new(x, y));
            Rect::new(a.x as f32, a.y as f32, (a.x + w * self.vp.scale) as f32, (a.y + h * self.vp.scale) as f32)
        };
        for h in &self.scene.hulls {
            out.push(AccessiblePart { name: h.name.clone(), role: AccessRole::Group, bounds: to_screen(h.x, h.y, h.w, HULL_HEAD) });
        }
        for n in &self.scene.nodes {
            if let Some(t) = d.task(&n.id) {
                out.push(AccessiblePart { name: t.name.clone(), role: AccessRole::ListItem, bounds: to_screen(n.rect.x, n.rect.y, n.rect.w, n.rect.h) });
            }
        }
        if !self.scene.nodes.is_empty() {
            let (_, parts) = self.pill_layout();
            for (i, r) in parts.iter().enumerate() {
                out.push(AccessiblePart { name: self.pill_tip(PILL[i]), role: if PILL[i] == PillPart::Percent { AccessRole::Label } else { AccessRole::Button }, bounds: *r });
            }
        }
        out
    }
}
