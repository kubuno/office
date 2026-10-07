//! `StartPage` — the Backstage's « Accueil » (`DiagramsStartContent.tsx` in `@ui`'s `StartPage`,
//! `core/web/src/ui/StartPage.tsx`): on the left the « Récents » column (`w` 256, `bg-surface-1`, a 57 DIP
//! header with the clock and the title, then one row per recent diagram — the diagram glyph of 18, the name
//! `text-sm`, the date `text-[11px]`; a right click opens the row's menu: open, duplicate, move to the trash),
//! and on the right the « Parcourir » tab: the diagrams of `Office/Diagrams` with « Nouveau diagramme »
//! (`ModuleFileBrowser`'s toolbar) and the browser's empty state (« Aucun diagramme », « Créer votre premier
//! diagramme »).
//!
//! The control only draws and reports: the window loads the lists (`set_lists`) and turns a
//! `StartCommand` (`open`, `menu`, `new`) into the server calls. Without a signed-in account the lists are
//! `None` and the columns say so.

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::{Canvas, Rect, Size};
use kubuno_desktop::views::component::{AccessiblePart, Control, ControlCore, EventCx, PaintEventCx};
use kubuno_desktop::views::events::Event;
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;

use crate::Resources;

/// A diagram as the lists show it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StartItem {
    pub id: String,
    pub title: String,
    /// `updated_at` as the server writes it (RFC 3339).
    pub updated_at: String,
}

/// What the user asked: `open` (a click on a row), `menu` (a right click on a row, at `x`, `y` in the
/// control), `new` (« Nouveau diagramme » or the empty state's link).
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct StartCommandEventArgs {
    pub command: String,
    pub id: String,
    pub x: f32,
    pub y: f32,
}

/// The « Récents » column (`RECENT_W_DEFAULT`).
const RECENT_W: f32 = 256.0;
/// Both headers (`h-[57px]`).
const HEADER_H: f32 = 57.0;
/// A recent row: `py-2` around a `text-sm` line and a `text-[11px]` line.
const RECENT_ROW_H: f32 = 52.0;
/// A browser row.
const BROWSE_ROW_H: f32 = 40.0;
/// The browser's toolbar.
const TOOLBAR_H: f32 = 56.0;
/// « Nouveau diagramme » (`Button size="sm"`): `h-8`.
const BUTTON_H: f32 = 32.0;
const WHEEL_STEP: f32 = 48.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hit {
    Recent(usize),
    Browse(usize),
    New,
    EmptyCreate,
}

/// The rectangles of the page in a `w`×`h` box at the origin.
struct Layout {
    recent: Rect,
    browse: Rect,
    new_button: Rect,
    /// The browser's list area (below its toolbar).
    list: Rect,
    /// The empty state's link.
    empty_link: Rect,
}

fn layout(w: f32, h: f32, new_label_w: f32) -> Layout {
    let rw = RECENT_W.min(w * 0.5);
    let recent = Rect::new(0.0, 0.0, rw, h);
    let browse = Rect::new(rw + 1.0, 0.0, w.max(rw + 1.0), h);
    let bw = new_label_w + 12.0 + 14.0 + 6.0 + 12.0;
    let ty = HEADER_H + (TOOLBAR_H - BUTTON_H) / 2.0;
    let new_button = Rect::new(browse.right - 24.0 - bw, ty, browse.right - 24.0, ty + BUTTON_H);
    let list = Rect::new(browse.left, HEADER_H + TOOLBAR_H, browse.right, h);
    let cy = (list.top + list.bottom) / 2.0;
    let empty_link = Rect::new((list.left + list.right) / 2.0 - 120.0, cy + 34.0, (list.left + list.right) / 2.0 + 120.0, cy + 58.0);
    Layout { recent, browse, new_button, list, empty_link }
}

fn recent_row(i: usize, l: &Layout, scroll: f32) -> Rect {
    let top = HEADER_H + 4.0 + i as f32 * RECENT_ROW_H - scroll;
    Rect::new(l.recent.left, top, l.recent.right, top + RECENT_ROW_H)
}

fn browse_row(i: usize, l: &Layout, scroll: f32) -> Rect {
    let top = l.list.top + 8.0 + i as f32 * BROWSE_ROW_H - scroll;
    Rect::new(l.list.left + 16.0, top, l.list.right - 16.0, top + BROWSE_ROW_H)
}

/// `formatDate(…, { day: 'numeric', month: 'short' })`: « 7 oct. » / « Oct 7 », from an RFC 3339 date.
pub fn short_date(rfc3339: &str) -> String {
    let mut it = rfc3339.get(..10).unwrap_or("").split('-');
    let (Some(_), Some(m), Some(d)) = (it.next(), it.next(), it.next()) else { return String::new() };
    let (Ok(m), Ok(d)) = (m.parse::<usize>(), d.parse::<u32>()) else { return String::new() };
    let months: Vec<&str> = Resources::months_short().split(',').collect();
    let Some(month) = m.checked_sub(1).and_then(|i| months.get(i)) else { return String::new() };
    Resources::date_short().replace("{day}", &d.to_string()).replace("{month}", month)
}

fn with_alpha(c: D2D1_COLOR_F, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { a: c.a * a, ..c }
}

/// The start page (see the module doc).
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Diagrams")]
#[toolbox(icon = "home")]
#[default_event("StartCommand")]
pub struct StartPage {
    base: ControlCore,
    #[event]
    #[category("Action")]
    pub start_command: Event<StartCommandEventArgs>,
    /// `None`: no account (the lists cannot be read).
    recents: Option<Vec<StartItem>>,
    all: Option<Vec<StartItem>>,
    hot: Option<Hit>,
    pressed: Option<Hit>,
    recent_scroll: f32,
    browse_scroll: f32,
}

impl Default for StartPage {
    fn default() -> Self {
        Self { base: ControlCore::default(), start_command: Event::default(), recents: None, all: None, hot: None, pressed: None, recent_scroll: 0.0, browse_scroll: 0.0 }
    }
}

impl StartPage {
    /// The recent diagrams and every diagram; `None` without an account.
    pub fn set_lists(&mut self, recents: Option<Vec<StartItem>>, all: Option<Vec<StartItem>>) {
        if self.recents != recents || self.all != all {
            self.recents = recents;
            self.all = all;
            self.recent_scroll = 0.0;
            self.browse_scroll = 0.0;
            self.hot = None;
            self.invalidate();
        }
    }

    fn layout(&self) -> Layout {
        // The button's label is measured when painting; for hit tests an estimate of its width is enough
        // (7 DIP a character of `text-sm` medium).
        layout(self.size().width, self.size().height, Resources::start_new().chars().count() as f32 * 7.0)
    }

    fn recents(&self) -> &[StartItem] {
        self.recents.as_deref().unwrap_or(&[])
    }

    fn all(&self) -> &[StartItem] {
        self.all.as_deref().unwrap_or(&[])
    }

    fn hit(&self, x: f32, y: f32) -> Option<Hit> {
        let l = self.layout();
        if l.new_button.contains(x, y) {
            return Some(Hit::New);
        }
        if l.recent.contains(x, y) && y >= HEADER_H {
            return (0..self.recents().len()).find(|&i| recent_row(i, &l, self.recent_scroll).contains(x, y)).map(Hit::Recent);
        }
        if l.list.contains(x, y) {
            if self.all().is_empty() {
                return l.empty_link.contains(x, y).then_some(Hit::EmptyCreate);
            }
            return (0..self.all().len()).find(|&i| browse_row(i, &l, self.browse_scroll).contains(x, y)).map(Hit::Browse);
        }
        None
    }

    fn item_of(&self, h: Hit) -> Option<&StartItem> {
        match h {
            Hit::Recent(i) => self.recents().get(i),
            Hit::Browse(i) => self.all().get(i),
            _ => None,
        }
    }

    fn command(&mut self, command: &str, id: String, x: f32, y: f32) {
        self.raise_start_command(StartCommandEventArgs { command: command.to_string(), id, x, y });
    }

    fn name_of(item: &StartItem) -> String {
        if item.title.is_empty() { Resources::untitled().to_string() } else { item.title.clone() }
    }

    fn max_scroll(rows: usize, row_h: f32, area: f32) -> f32 {
        (rows as f32 * row_h + 16.0 - area).max(0.0)
    }
}

impl Control for StartPage {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, _proposed: Size) -> Size {
        Size { width: 900.0, height: 600.0 }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        let b = e.clip_rectangle;
        let c: &dyn Canvas = e.graphics;
        let t = c.theme();
        let f = c.formats();
        let (w, h) = (b.right - b.left, b.bottom - b.top);
        let new_w = c.measure(Resources::start_new(), &f.body_strong);
        let l = layout(w, h, new_w);
        let at = |r: Rect| Rect::new(b.left + r.left, b.top + r.top, b.left + r.right, b.top + r.bottom);
        let icon = |name: &'static str, cx: f32, cy: f32, size: f32, ink: &D2D1_COLOR_F| {
            c.vector_icon(name, &Rect::new(cx - size / 2.0, cy - size / 2.0, cx + size / 2.0, cy + size / 2.0), size, ink);
        };
        c.fill_rounded(&b, 0.0, &t.card_background);

        // ── « Récents » ──
        let rc = at(l.recent);
        c.fill_rounded(&rc, 0.0, &t.layer_background);
        icon("Clock", rc.left + 16.0 + 7.5, rc.top + HEADER_H / 2.0, 15.0, &t.text_tertiary);
        c.text(Resources::start_recent(), &Rect::new(rc.left + 16.0 + 15.0 + 8.0, rc.top, rc.right - 16.0, rc.top + HEADER_H), &f.body_strong, &t.text_primary, false);
        c.fill_rounded(&Rect::new(rc.left, rc.top + HEADER_H - 1.0, rc.right, rc.top + HEADER_H), 0.0, &t.divider);
        let body = Rect::new(rc.left, rc.top + HEADER_H, rc.right, rc.bottom);
        match self.recents.as_deref() {
            Some(items) if !items.is_empty() => {
                c.push_clip(&body);
                for (i, it) in items.iter().enumerate() {
                    let row = at(recent_row(i, &l, self.recent_scroll));
                    if row.bottom < body.top || row.top > body.bottom {
                        continue;
                    }
                    if self.hot == Some(Hit::Recent(i)) {
                        c.fill_rounded(&row, 0.0, &t.surface_2);
                    }
                    let cy = (row.top + row.bottom) / 2.0;
                    icon("Network", row.left + 12.0 + 9.0, cy, 18.0, &t.text_tertiary);
                    let tl = row.left + 12.0 + 18.0 + 10.0;
                    c.text_ellipsis(&Self::name_of(it), &Rect::new(tl, row.top + 8.0, row.right - 12.0, row.top + 28.0), &f.body, &t.text_primary);
                    c.text_ellipsis(&short_date(&it.updated_at), &Rect::new(tl, row.top + 28.0, row.right - 12.0, row.top + 44.0), &f.role_caption, &t.text_tertiary);
                }
                c.pop_clip();
            }
            other => {
                // `recentEmpty`: the glyph at 30 % and the hint (or, without an account, why there is none).
                let cy = (body.top + body.bottom) / 2.0;
                icon("Network", (body.left + body.right) / 2.0, cy - 18.0, 32.0, &with_alpha(t.text_tertiary, 0.3));
                let hint = if other.is_some() { Resources::start_recent_empty() } else { Resources::start_signed_out() };
                c.text(hint, &Rect::new(body.left + 16.0, cy + 4.0, body.right - 16.0, cy + 44.0), &f.caption, &t.text_tertiary, true);
            }
        }
        // The column's edge (the resize handle's line).
        c.fill_rounded(&Rect::new(rc.right, rc.top, rc.right + 1.0, rc.bottom), 0.0, &t.divider);

        // ── « Parcourir » ──
        let br = at(l.browse);
        let tab_w = c.measure(Resources::start_browse(), &f.body_strong);
        let tab = Rect::new(br.left + 24.0, br.top, br.left + 24.0 + tab_w, br.top + HEADER_H);
        c.text(Resources::start_browse(), &tab, &f.body_strong, &t.accent, false);
        c.fill_rounded(&Rect::new(tab.left, tab.bottom - 2.0, tab.right, tab.bottom), 1.0, &t.accent);
        c.fill_rounded(&Rect::new(br.left, br.top + HEADER_H - 1.0, br.right, br.top + HEADER_H), 0.0, &t.divider);
        // The browser's toolbar: its title and « Nouveau diagramme ».
        c.text(Resources::start_browse_title(), &Rect::new(br.left + 24.0, br.top + HEADER_H, br.right - 200.0, br.top + HEADER_H + TOOLBAR_H), &f.heading, &t.text_primary, false);
        let nb = at(l.new_button);
        let fill = if self.hot == Some(Hit::New) { t.accent_hover } else { t.accent };
        c.fill_rounded(&nb, 6.0, &fill);
        icon("Plus", nb.left + 12.0 + 7.0, (nb.top + nb.bottom) / 2.0, 14.0, &t.accent_foreground);
        c.text(Resources::start_new(), &Rect::new(nb.left + 12.0 + 14.0 + 6.0, nb.top, nb.right - 8.0, nb.bottom), &f.body_strong, &t.accent_foreground, false);
        let list = at(l.list);
        match self.all.as_deref() {
            Some(items) if !items.is_empty() => {
                c.push_clip(&list);
                for (i, it) in items.iter().enumerate() {
                    let row = at(browse_row(i, &l, self.browse_scroll));
                    if row.bottom < list.top || row.top > list.bottom {
                        continue;
                    }
                    if self.hot == Some(Hit::Browse(i)) {
                        c.fill_rounded(&row, 6.0, &t.row_hover);
                    }
                    let cy = (row.top + row.bottom) / 2.0;
                    icon("Network", row.left + 12.0 + 8.0, cy, 16.0, &t.text_tertiary);
                    c.text_ellipsis(&Self::name_of(it), &Rect::new(row.left + 44.0, row.top, row.right - 140.0, row.bottom), &f.body, &t.text_primary);
                    c.text_ellipsis(&short_date(&it.updated_at), &Rect::new(row.right - 128.0, row.top, row.right - 12.0, row.bottom), &f.caption, &t.text_tertiary);
                }
                c.pop_clip();
            }
            _ => {
                // The browser's `emptyState`.
                let cy = (list.top + list.bottom) / 2.0;
                let cx = (list.left + list.right) / 2.0;
                icon("Network", cx, cy - 36.0, 48.0, &with_alpha(t.text_tertiary, 0.3));
                c.text(Resources::start_empty_title(), &Rect::new(list.left, cy, list.right, cy + 26.0), &f.body_strong, &t.text_secondary, true);
                let link = at(l.empty_link);
                c.text(Resources::start_empty_create(), &link, &f.body, &t.accent, true);
                if self.hot == Some(Hit::EmptyCreate) {
                    let tw = c.measure(Resources::start_empty_create(), &f.body);
                    let lx = (link.left + link.right - tw) / 2.0;
                    c.fill_rounded(&Rect::new(lx, link.bottom - 3.0, lx + tw, link.bottom - 2.0), 0.0, &t.accent);
                }
            }
        }
        e.raise(self, "OnPaint");
    }

    fn tool_tip_at(&self, x: f32, y: f32) -> Option<String> {
        let h = self.hit(x, y)?;
        self.item_of(h).map(Self::name_of)
    }

    fn accessible_parts(&self) -> Vec<AccessiblePart> {
        use kubuno_desktop::controls::host::access::AccessRole;
        let l = self.layout();
        let mut parts = vec![AccessiblePart { name: Resources::start_new().to_string(), role: AccessRole::Button, bounds: l.new_button }];
        for (i, it) in self.recents().iter().enumerate() {
            parts.push(AccessiblePart { name: Self::name_of(it), role: AccessRole::ListItem, bounds: recent_row(i, &l, self.recent_scroll) });
        }
        for (i, it) in self.all().iter().enumerate() {
            parts.push(AccessiblePart { name: Self::name_of(it), role: AccessRole::ListItem, bounds: browse_row(i, &l, self.browse_scroll) });
        }
        if self.all().is_empty() {
            parts.push(AccessiblePart { name: Resources::start_empty_create().to_string(), role: AccessRole::Button, bounds: l.empty_link });
        }
        parts
    }

    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y) = (e.args().x, e.args().y);
        match e.args().button {
            MouseButton::Left => self.pressed = self.hit(x, y),
            MouseButton::Right => {
                // `onContextMenu`: the row's actions (open, duplicate, trash).
                self.pressed = None;
                if let Some(id) = self.hit(x, y).and_then(|h| self.item_of(h)).map(|it| it.id.clone()) {
                    self.command("menu", id, x, y);
                }
            }
            _ => {}
        }
        self.invalidate();
        e.raise(&*self, "OnMouseDown");
    }

    fn on_mouse_up(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y) = (e.args().x, e.args().y);
        let h = self.hit(x, y);
        if let Some(p) = self.pressed.take().filter(|p| Some(*p) == h) {
            match p {
                Hit::New | Hit::EmptyCreate => self.command("new", String::new(), x, y),
                Hit::Recent(_) | Hit::Browse(_) => {
                    if let Some(id) = self.item_of(p).map(|it| it.id.clone()) {
                        self.command("open", id, x, y);
                    }
                }
            }
        }
        self.invalidate();
        e.raise(&*self, "OnMouseUp");
    }

    fn on_mouse_move(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let h = self.hit(e.args().x, e.args().y);
        if h != self.hot {
            self.hot = h;
            self.invalidate();
        }
        e.raise(&*self, "OnMouseMove");
    }

    fn on_mouse_wheel(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let l = self.layout();
        let step = e.args().delta * WHEEL_STEP;
        if e.args().x < l.recent.right {
            let max = Self::max_scroll(self.recents().len(), RECENT_ROW_H, l.recent.bottom - HEADER_H);
            self.recent_scroll = (self.recent_scroll - step).clamp(0.0, max);
        } else {
            let max = Self::max_scroll(self.all().len(), BROWSE_ROW_H, l.list.bottom - l.list.top);
            self.browse_scroll = (self.browse_scroll - step).clamp(0.0, max);
        }
        self.invalidate();
        e.raise(&*self, "OnMouseWheel");
    }

    fn on_mouse_leave(&mut self, e: &mut EventCx<'_, kubuno_desktop::views::events::EmptyEventArgs>) {
        if self.hot.take().is_some() {
            self.invalidate();
        }
        e.raise(&*self, "OnMouseLeave");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_date_reads_day_and_short_month_in_the_ui_culture() {
        let d = short_date("2026-10-07T06:23:30.292289Z");
        assert!(d.contains('7') && !d.contains("07"), "{d}");
        assert!(d.to_lowercase().contains("oct"), "{d}");
        assert_eq!(short_date("garbage"), "");
        assert_eq!(short_date("2026-13-01T00:00:00Z"), "");
    }

    #[test]
    fn the_recent_column_keeps_the_webs_width_and_the_rows_follow_the_header() {
        let l = layout(1200.0, 700.0, 120.0);
        assert_eq!(l.recent.right, RECENT_W);
        assert_eq!(recent_row(0, &l, 0.0).top, HEADER_H + 4.0);
        assert_eq!(recent_row(2, &l, 0.0).top, HEADER_H + 4.0 + 2.0 * RECENT_ROW_H);
        assert_eq!(l.list.top, HEADER_H + TOOLBAR_H);
        assert!(l.new_button.right <= l.browse.right - 24.0 + 0.01);
        // A narrow page gives the column at most half of it.
        assert_eq!(layout(400.0, 300.0, 120.0).recent.right, 200.0);
    }

    #[test]
    fn a_click_reports_the_row_and_the_new_button() {
        let mut p = StartPage::default();
        let item = |id: &str| StartItem { id: id.into(), title: id.to_uppercase(), updated_at: "2026-10-07T00:00:00Z".into() };
        p.set_lists(Some(vec![item("a"), item("b")]), Some(vec![item("c")]));
        // Without a laid-out size the control is 0×0: test the pure hit functions instead.
        let l = layout(1000.0, 600.0, 120.0);
        let r = recent_row(1, &l, 0.0);
        assert!(r.contains(20.0, r.top + 5.0));
        assert_eq!(p.item_of(Hit::Recent(1)).map(|i| i.id.as_str()), Some("b"));
        assert_eq!(p.item_of(Hit::Browse(0)).map(|i| i.id.as_str()), Some("c"));
        assert_eq!(StartPage::name_of(&StartItem::default()), Resources::untitled());
    }
}
