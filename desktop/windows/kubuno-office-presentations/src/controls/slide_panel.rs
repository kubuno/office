//! `SlidePanel` - the slide list (the web's `SlidePanel` + `SlideThumbnail`): one live thumbnail per slide drawn
//! by the shared renderer (`Mode::Thumbnail`) at the panel's width, 16:9, its number below; click, Ctrl+click,
//! Shift+click select; dragging a thumbnail reorders; a right click asks for the slide menu; « + Nouvelle
//! diapositive » at the bottom. Hidden slides show no badge, like the web.

use std::rc::Rc;

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::{Canvas, Rect, Size};
use kubuno_desktop::views::component::{Control, ControlCore, EventCx, PaintEventCx};
use kubuno_desktop::views::events::Event;
use kubuno_office_slides_core::model::{SLIDE_H, SLIDE_W};
use kubuno_office_slides_core::render::{Mode, Options, Renderer};
use kubuno_office_slides_core::surface::Matrix;

use crate::model::Shared;
use crate::platform::d2d::{D2dSurface, Fonts};
use crate::platform::images::BitmapCache;

const PAD: f32 = 8.0;
const GAP: f32 = 4.0;
const NUMBER_H: f32 = 14.0;
const FOOTER_H: f32 = 48.0;

/// A change the panel made to the presentation (selection, order) or asks the window for.
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct PanelEventArgs {
    /// `select`, `reorder`, `new`, `menu`.
    pub action: String,
    /// For `menu`: the slide under the pointer (empty: the list's empty space) and where (window DIP).
    pub slide: String,
    pub x: f32,
    pub y: f32,
}

#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Presentations")]
#[toolbox(icon = "gallery-vertical")]
#[default_event("PanelChanged")]
pub struct SlidePanel {
    base: ControlCore,
    #[event]
    #[category("Action")]
    pub panel_changed: Event<PanelEventArgs>,
    doc: Option<Shared>,
    fonts: Option<(usize, Rc<Fonts>)>,
    bitmaps: BitmapCache,
    bounds: Rect,
    scroll: f32,
    hover: Option<usize>,
    footer_hot: bool,
    press: Option<(usize, f32, f32)>,
    dragging: Option<usize>,
    drop_at: Option<usize>,
}

impl Default for SlidePanel {
    fn default() -> Self {
        SlidePanel {
            base: ControlCore::default(),
            panel_changed: Event::default(),
            doc: None,
            fonts: None,
            bitmaps: BitmapCache::default(),
            bounds: Rect::default(),
            scroll: 0.0,
            hover: None,
            footer_hot: false,
            press: None,
            dragging: None,
            drop_at: None,
        }
    }
}

impl SlidePanel {
    pub fn set_doc(&mut self, doc: Shared) {
        self.doc = Some(doc);
        self.invalidate();
    }

    fn list_rect(&self) -> Rect {
        Rect::new(self.bounds.left, self.bounds.top, self.bounds.right, self.bounds.bottom - FOOTER_H)
    }

    fn footer_button(&self) -> Rect {
        Rect::new(self.bounds.left + PAD, self.bounds.bottom - FOOTER_H + PAD, self.bounds.right - PAD, self.bounds.bottom - PAD)
    }

    /// Each item's rectangle (the button: 4 px padding around the thumbnail and its number).
    fn items(&self) -> Vec<Rect> {
        let n = self.doc.as_ref().map(|d| d.ws.borrow().deck.slides.len()).unwrap_or(0);
        let w = (self.bounds.right - self.bounds.left - 2.0 * PAD).max(10.0);
        let thumb_w = w - 8.0;
        let item_h = 4.0 + thumb_w * (SLIDE_H / SLIDE_W) as f32 + GAP + NUMBER_H + 4.0;
        let mut y = self.bounds.top + PAD - self.scroll;
        (0..n)
            .map(|_| {
                let r = Rect::new(self.bounds.left + PAD, y, self.bounds.left + PAD + w, y + item_h);
                y += item_h + GAP;
                r
            })
            .collect()
    }

    fn content_height(&self) -> f32 {
        self.items().last().map(|r| r.bottom + self.scroll - self.bounds.top + PAD).unwrap_or(0.0)
    }

    fn index_at(&self, x: f32, y: f32) -> Option<usize> {
        if !self.list_rect().contains(x, y) {
            return None;
        }
        self.items().iter().position(|r| r.contains(x, y))
    }

    /// Scrolls so the active slide shows.
    pub fn reveal_active(&mut self) {
        let Some(doc) = &self.doc else { return };
        let Some(i) = doc.ws.borrow().active_index() else { return };
        let items = self.items();
        let Some(r) = items.get(i) else { return };
        let list = self.list_rect();
        if r.top < list.top {
            self.scroll -= list.top - r.top + PAD;
        } else if r.bottom > list.bottom {
            self.scroll += r.bottom - list.bottom + PAD;
        }
        self.clamp_scroll();
        self.invalidate();
    }

    fn clamp_scroll(&mut self) {
        let max = (self.content_height() - (self.list_rect().bottom - self.list_rect().top)).max(0.0);
        self.scroll = self.scroll.clamp(0.0, max);
    }
}

impl Control for SlidePanel {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, _proposed: Size) -> Size {
        Size { width: 150.0, height: 400.0 }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        self.bounds = e.clip_rectangle;
        // Squeezed to nothing (the Backstage takes the whole page while it is open): the footer, anchored to the
        // bottom edge, would otherwise be drawn above the panel, over the Backstage.
        if self.bounds.bottom - self.bounds.top < FOOTER_H || self.bounds.right - self.bounds.left < 1.0 {
            e.raise(self, "OnPaint");
            return;
        }
        let c = e.canvas();
        let t = c.theme();
        c.fill_rounded(&self.bounds, 0.0, &t.panel_background);
        if let Some(renderer) = c.renderer() {
            let key = windows::core::Interface::as_raw(&renderer.dwrite) as usize;
            if self.fonts.as_ref().is_none_or(|(k, _)| *k != key) {
                self.fonts = Some((key, Rc::new(Fonts::new(renderer.dwrite.clone()))));
            }
        }
        let (Some(doc), Some(renderer), Some((_, fonts))) = (self.doc.clone(), c.renderer(), self.fonts.clone()) else {
            e.raise(self, "OnPaint");
            return;
        };
        self.clamp_scroll();
        let list = self.list_rect();
        c.push_clip(&list);
        let items = self.items();
        let ws = doc.ws.borrow();
        let accent = crate::controls::hex(0x1a73e8);
        for (i, r) in items.iter().enumerate() {
            if r.bottom < list.top || r.top > list.bottom {
                continue;
            }
            let Some(slide) = ws.deck.slides.get(i) else { continue };
            let active = ws.slide_sel.contains(&slide.id);
            if active {
                c.fill_rounded(r, 4.0, &t.accent_light);
            } else if self.hover == Some(i) && self.dragging.is_none() {
                c.fill_rounded(r, 4.0, &t.surface_2);
            }
            let thumb = Rect::new(r.left + 4.0, r.top + 4.0, r.right - 4.0, r.top + 4.0 + (r.right - r.left - 8.0) * (SLIDE_H / SLIDE_W) as f32);
            {
                let mut surface = D2dSurface::new(renderer, &fonts, &doc.store, &mut self.bitmaps);
                let base = surface.outer().mul(&Matrix::translation(thumb.left as f64, thumb.top as f64));
                c.push_clip_rounded(&thumb, 4.0);
                let rr = Renderer::new((thumb.right - thumb.left) as f64, (thumb.bottom - thumb.top) as f64, &*fonts);
                rr.render(&mut surface, base, &slide.data, &ws.deck.theme, &Options { mode: Mode::Thumbnail, ..Options::default() });
                drop(surface);
                c.pop_clip_rounded();
            }
            let border = if active { accent } else if self.hover == Some(i) { t.text_tertiary } else { t.card_stroke };
            c.stroke_rounded_w(&Rect::new(thumb.left + 1.0, thumb.top + 1.0, thumb.right - 1.0, thumb.bottom - 1.0), 4.0, &border, 2.0);
            let num = Rect::new(r.left, thumb.bottom + GAP, r.right, thumb.bottom + GAP + NUMBER_H);
            c.text(&(i + 1).to_string(), &num, &c.formats().micro, &t.text_tertiary, true);
            if self.drop_at == Some(i) && self.dragging.is_some_and(|d| d != i) {
                c.fill_rounded(&Rect::new(r.left, r.top - 2.0, r.right, r.top), 0.0, &accent);
            }
        }
        drop(ws);
        c.pop_clip();
        // « + Nouvelle diapositive », a ghost button over a top border.
        c.fill_rounded(&Rect::new(self.bounds.left, list.bottom, self.bounds.right, list.bottom + 1.0), 0.0, &t.card_stroke);
        let b = self.footer_button();
        if self.footer_hot {
            c.fill_rounded(&b, 6.0, &t.surface_2);
        }
        c.vector_icon("Plus", &Rect::new(b.left + 8.0, b.top, b.left + 22.0, b.bottom), 14.0, &t.text_primary);
        c.text("Nouvelle diapositive", &Rect::new(b.left + 28.0, b.top, b.right, b.bottom), &c.formats().caption, &t.text_primary, false);
        e.raise(self, "OnPaint");
    }

    fn on_mouse_wheel(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        self.scroll -= e.args().delta * 60.0;
        self.clamp_scroll();
        self.invalidate();
        e.raise(&*self, "OnMouseWheel");
    }

    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y) = (e.args().x + self.bounds.left, e.args().y + self.bounds.top);
        let Some(doc) = self.doc.clone() else { return };
        self.focus();
        if self.footer_button().contains(x, y) {
            if e.args().button == MouseButton::Left {
                self.raise_panel_changed(PanelEventArgs { action: "new".into(), ..Default::default() });
            }
            e.raise(&*self, "OnMouseDown");
            return;
        }
        let hit = self.index_at(x, y);
        let id = hit.and_then(|i| doc.ws.borrow().deck.slides.get(i).map(|s| s.id.clone()));
        match e.args().button {
            MouseButton::Right => {
                match &id {
                    Some(sid) => {
                        if !doc.ws.borrow().slide_sel.contains(sid) {
                            doc.ws.borrow_mut().select_slide(sid, false, false);
                        }
                    }
                    None => doc.ws.borrow_mut().clear_slide_selection(),
                }
                self.invalidate();
                self.raise_panel_changed(PanelEventArgs { action: "menu".into(), slide: id.unwrap_or_default(), x, y });
            }
            MouseButton::Left => {
                match (&id, hit) {
                    (Some(sid), Some(i)) => {
                        let m = e.args().mods;
                        doc.ws.borrow_mut().select_slide(sid, m.ctrl, m.shift);
                        self.press = Some((i, x, y));
                    }
                    _ => doc.ws.borrow_mut().clear_slide_selection(),
                }
                self.invalidate();
                self.raise_panel_changed(PanelEventArgs { action: "select".into(), ..Default::default() });
            }
            _ => {}
        }
        e.raise(&*self, "OnMouseDown");
    }

    fn on_mouse_move(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y) = (e.args().x + self.bounds.left, e.args().y + self.bounds.top);
        let hover = self.index_at(x, y);
        let hot = self.footer_button().contains(x, y);
        if hover != self.hover || hot != self.footer_hot {
            self.hover = hover;
            self.footer_hot = hot;
            self.invalidate();
        }
        if e.args().button == MouseButton::Left {
            if let Some((i, px, py)) = self.press {
                if self.dragging.is_none() && ((x - px).abs() > 4.0 || (y - py).abs() > 4.0) {
                    self.dragging = Some(i);
                }
                if self.dragging.is_some() {
                    self.drop_at = hover;
                    self.invalidate();
                }
            }
        } else {
            self.press = None;
            self.dragging = None;
            self.drop_at = None;
        }
        e.raise(&*self, "OnMouseMove");
    }

    fn on_mouse_up(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        if let (Some(from), Some(to), Some(doc)) = (self.dragging, self.drop_at, self.doc.clone()) {
            if from != to {
                doc.ws.borrow_mut().reorder(from, to);
                self.raise_panel_changed(PanelEventArgs { action: "reorder".into(), ..Default::default() });
            }
        }
        self.press = None;
        self.dragging = None;
        self.drop_at = None;
        self.invalidate();
        e.raise(&*self, "OnMouseUp");
    }

    fn on_mouse_leave(&mut self, e: &mut EventCx<'_, kubuno_desktop::views::events::EmptyEventArgs>) {
        if self.hover.take().is_some() || std::mem::take(&mut self.footer_hot) {
            self.invalidate();
        }
        e.raise(&*self, "OnMouseLeave");
    }
}
