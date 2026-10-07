//! `PageTabs` — the page bar under the canvas (`DiagramEditorPage.tsx:3856-3925`): a `h-9` bar with a top
//! border; one tab per page (`px-3 text-xs border-r`; the active one `text-primary font-medium border-b-2
//! border-primary bg-primary/5`, the others `text-secondary hover:bg-surface-2`), its « × » shown on hover
//! when there is more than one page (it keeps its room otherwise: `opacity-0`), a double click to rename, « + »
//! (`px-2`) to add a page, and « Macros » on the right — drawn disabled: the desktop has no script runtime.

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::{Canvas, Rect, Size};
use kubuno_desktop::views::component::{AccessiblePart, Control, ControlCore, EventCx, PaintEventCx};
use kubuno_desktop::views::events::Event;
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;

/// A page of the bar: its index and name.
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct PageTabEventArgs {
    pub index: usize,
    pub name: String,
}

/// `px-3`.
const TAB_PAD: f32 = 12.0;
/// `gap-2` + `ml-1` + the « × » glyph (`text-base`).
const CLOSE_GAP: f32 = 12.0;
const CLOSE_W: f32 = 10.0;
/// « + »: `px-2` around a 14 DIP glyph.
const ADD_W: f32 = 30.0;
/// « Macros »: the Zap glyph, a gap and the label, `pr-2`.
const MACROS_W: f32 = 84.0;

/// Where each part of the bar is, in the control's own coordinates.
#[derive(Debug, Clone, Default, PartialEq)]
struct Layout {
    tabs: Vec<(Rect, Option<Rect>)>,
    add: Rect,
    macros: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hit {
    Tab(usize),
    Close(usize),
    Add,
}

/// The page bar (see the module doc).
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Diagrams")]
#[toolbox(icon = "panel-bottom")]
#[default_event("PageSelected")]
pub struct PageTabs {
    base: ControlCore,
    #[event]
    #[category("Action")]
    pub page_selected: Event<PageTabEventArgs>,
    #[event]
    #[category("Action")]
    pub page_added: Event<kubuno_desktop::views::events::EmptyEventArgs>,
    #[event]
    #[category("Action")]
    pub page_renamed: Event<PageTabEventArgs>,
    #[event]
    #[category("Action")]
    pub page_deleted: Event<PageTabEventArgs>,
    pages: Vec<String>,
    active: usize,
    /// The widths of the names, measured at the last paint.
    widths: Vec<f32>,
    hot: Option<Hit>,
    pressed: Option<Hit>,
}

impl Default for PageTabs {
    fn default() -> Self {
        Self {
            base: ControlCore::default(),
            page_selected: Event::default(),
            page_added: Event::default(),
            page_renamed: Event::default(),
            page_deleted: Event::default(),
            pages: vec!["Page 1".into()],
            active: 0,
            widths: Vec::new(),
            hot: None,
            pressed: None,
        }
    }
}

/// The bar's layout for these name widths, in a `w`×`h` box at the origin.
fn layout(widths: &[f32], w: f32, h: f32) -> Layout {
    let closable = widths.len() > 1;
    let mut x = 0.0;
    let mut tabs = Vec::with_capacity(widths.len());
    for tw in widths {
        let width = TAB_PAD * 2.0 + tw + if closable { CLOSE_GAP + CLOSE_W } else { 0.0 } + 1.0;
        let tab = Rect::new(x, 0.0, x + width, h);
        let close = closable.then(|| {
            let cx = x + TAB_PAD + tw + CLOSE_GAP;
            Rect::new(cx - 2.0, h / 2.0 - 8.0, cx + CLOSE_W + 2.0, h / 2.0 + 8.0)
        });
        tabs.push((tab, close));
        x += width;
    }
    Layout { tabs, add: Rect::new(x, 0.0, x + ADD_W, h), macros: Rect::new((w - MACROS_W).max(x + ADD_W), 0.0, w, h) }
}

fn hit(l: &Layout, x: f32, y: f32) -> Option<Hit> {
    for (i, (tab, close)) in l.tabs.iter().enumerate() {
        if tab.contains(x, y) {
            if close.is_some_and(|c| c.contains(x, y)) {
                return Some(Hit::Close(i));
            }
            return Some(Hit::Tab(i));
        }
    }
    l.add.contains(x, y).then_some(Hit::Add)
}

fn with_alpha(c: D2D1_COLOR_F, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { a: c.a * a, ..c }
}

impl PageTabs {
    pub fn set_pages(&mut self, pages: Vec<String>, active: usize) {
        if self.pages != pages || self.active != active {
            self.pages = pages;
            self.active = active;
            self.invalidate();
        }
    }

    fn layout(&self) -> Layout {
        let widths: Vec<f32> = if self.widths.len() == self.pages.len() { self.widths.clone() } else { self.pages.iter().map(|p| p.chars().count() as f32 * 6.5).collect() };
        layout(&widths, self.size().width, self.size().height)
    }

    fn page(&self, i: usize) -> PageTabEventArgs {
        PageTabEventArgs { index: i, name: self.pages.get(i).cloned().unwrap_or_default() }
    }
}

impl Control for PageTabs {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, _proposed: Size) -> Size {
        Size { width: 400.0, height: 36.0 }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        let b = e.clip_rectangle;
        let c: &dyn Canvas = e.graphics;
        let t = c.theme();
        let caption = &c.formats().caption;
        let strong = &c.formats().caption_strong;
        self.widths = self.pages.iter().enumerate().map(|(i, p)| c.measure(p, if i == self.active { strong } else { caption })).collect();
        let l = layout(&self.widths, b.right - b.left, b.bottom - b.top);
        let at = |r: &Rect| Rect::new(b.left + r.left, b.top + r.top, b.left + r.right, b.top + r.bottom);
        // `bg-white border-t border-border`.
        c.fill_rounded(&b, 0.0, &t.card_background);
        c.fill_rounded(&Rect::new(b.left, b.top, b.right, b.top + 1.0), 0.0, &t.divider);
        for (i, (tab, close)) in l.tabs.iter().enumerate() {
            let r = at(tab);
            let active = i == self.active;
            let hot = matches!(self.hot, Some(Hit::Tab(j)) | Some(Hit::Close(j)) if j == i);
            if active {
                c.fill_rounded(&r, 0.0, &with_alpha(t.accent, 0.05));
                c.fill_rounded(&Rect::new(r.left, r.bottom - 2.0, r.right - 1.0, r.bottom), 0.0, &t.accent);
            } else if hot {
                c.fill_rounded(&r, 0.0, &t.surface_2);
            }
            // `border-r border-border`.
            c.fill_rounded(&Rect::new(r.right - 1.0, r.top + 1.0, r.right, r.bottom), 0.0, &t.divider);
            let text_r = Rect::new(r.left + TAB_PAD, r.top, r.left + TAB_PAD + self.widths.get(i).copied().unwrap_or(0.0) + 1.0, r.bottom);
            c.text(&self.pages[i], &text_r, if active { strong } else { caption }, if active { &t.accent } else { &t.text_secondary }, false);
            // The « × »: `opacity-0 group-hover:opacity-100 text-text-tertiary hover:text-danger`.
            if let Some(x) = close.filter(|_| hot) {
                let xr = at(&x);
                let ink = if self.hot == Some(Hit::Close(i)) { t.danger } else { t.text_tertiary };
                let (cx, cy) = ((xr.left + xr.right) / 2.0, (xr.top + xr.bottom) / 2.0);
                c.vector_icon("X", &Rect::new(cx - 6.0, cy - 6.0, cx + 6.0, cy + 6.0), 12.0, &ink);
            }
        }
        let add = at(&l.add);
        if self.hot == Some(Hit::Add) {
            c.fill_rounded(&add, 0.0, &t.surface_2);
        }
        let (cx, cy) = ((add.left + add.right) / 2.0, (add.top + add.bottom) / 2.0);
        c.vector_icon("Plus", &Rect::new(cx - 7.0, cy - 7.0, cx + 7.0, cy + 7.0), 14.0, if self.hot == Some(Hit::Add) { &t.accent } else { &t.text_tertiary });
        // « Macros », disabled.
        let m = at(&l.macros);
        let dim = with_alpha(t.text_secondary, 0.45);
        let cy = (m.top + m.bottom) / 2.0;
        c.vector_icon("Zap", &Rect::new(m.left + 6.0, cy - 7.0, m.left + 20.0, cy + 7.0), 14.0, &dim);
        c.text("Macros", &Rect::new(m.left + 26.0, m.top, m.right - 8.0, m.bottom), caption, &dim, false);
        e.raise(self, "OnPaint");
    }

    fn tool_tip_at(&self, x: f32, y: f32) -> Option<String> {
        let l = self.layout();
        if l.macros.contains(x, y) {
            return Some("Les macros (Script) ne sont pas disponibles dans l'application de bureau".to_string());
        }
        match hit(&l, x, y)? {
            Hit::Add => Some("Ajouter une page".to_string()),
            Hit::Close(_) => Some("Supprimer la page".to_string()),
            Hit::Tab(i) => self.pages.get(i).cloned(),
        }
    }

    fn accessible_parts(&self) -> Vec<AccessiblePart> {
        use kubuno_desktop::controls::host::access::AccessRole;
        let l = self.layout();
        let mut parts: Vec<AccessiblePart> = l.tabs.iter().enumerate().map(|(i, (r, _))| AccessiblePart { name: self.pages[i].clone(), role: AccessRole::Tab, bounds: *r }).collect();
        for (i, (_, close)) in l.tabs.iter().enumerate() {
            if let Some(c) = close {
                parts.push(AccessiblePart { name: format!("Supprimer {}", self.pages[i]), role: AccessRole::Button, bounds: *c });
            }
        }
        parts.push(AccessiblePart { name: "Ajouter une page".to_string(), role: AccessRole::Button, bounds: l.add });
        parts
    }

    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        if e.args().button == MouseButton::Left {
            let h = hit(&self.layout(), e.args().x, e.args().y);
            if e.args().clicks >= 2 {
                if let Some(Hit::Tab(i)) = h {
                    self.pressed = None;
                    self.raise_page_renamed(self.page(i));
                }
            } else {
                self.pressed = h;
                if let Some(Hit::Tab(i)) = h {
                    if i != self.active {
                        self.raise_page_selected(self.page(i));
                    }
                }
            }
            self.invalidate();
        }
        e.raise(&*self, "OnMouseDown");
    }

    fn on_mouse_up(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let h = hit(&self.layout(), e.args().x, e.args().y);
        match self.pressed.take() {
            Some(Hit::Add) if h == Some(Hit::Add) => self.raise_page_added(kubuno_desktop::views::events::EmptyEventArgs),
            Some(Hit::Close(i)) if h == Some(Hit::Close(i)) => self.raise_page_deleted(self.page(i)),
            _ => {}
        }
        self.invalidate();
        e.raise(&*self, "OnMouseUp");
    }

    fn on_mouse_move(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let h = hit(&self.layout(), e.args().x, e.args().y);
        if h != self.hot {
            self.hot = h;
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_page_has_no_close_button() {
        let l = layout(&[40.0], 600.0, 36.0);
        assert_eq!(l.tabs[0].1, None);
        assert_eq!(l.tabs[0].0.right, 24.0 + 40.0 + 1.0);
        assert_eq!(l.add.left, l.tabs[0].0.right);
    }

    #[test]
    fn tabs_follow_one_another_and_the_close_sits_after_the_name() {
        let l = layout(&[40.0, 50.0], 600.0, 36.0);
        let w0 = 24.0 + 40.0 + CLOSE_GAP + CLOSE_W + 1.0;
        assert_eq!(l.tabs[1].0.left, w0);
        let close = l.tabs[0].1.expect("closable");
        assert_eq!(hit(&l, (close.left + close.right) / 2.0, 18.0), Some(Hit::Close(0)));
        assert_eq!(hit(&l, 5.0, 18.0), Some(Hit::Tab(0)));
        assert_eq!(hit(&l, l.add.left + 5.0, 18.0), Some(Hit::Add));
        assert_eq!(hit(&l, 590.0, 18.0), None, "Macros is not a hit");
        assert_eq!(l.macros.right, 600.0);
    }
}
