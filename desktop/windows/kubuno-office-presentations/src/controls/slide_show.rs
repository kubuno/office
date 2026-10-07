//! `SlideShow` — the slideshow stage (the web's `PresenterMode`): black, the visible slides fitted into the
//! width and 82 % of the height, the incoming slide's transition, the animated elements revealed one press at a
//! time, the presenter notes under the slide, and the control bar (‹ n / N › mm:ss ✕). The sequence is the
//! shared engine's (`kubuno_office_slides_core::show::Show`); this control draws it and maps the input:
//! → Space PageDown Enter or a click: next; ← PageUp or a right click: previous; Home / End; B black screen;
//! N slide numbers; P autoplay; L loop; Escape closes (`Closed`).

use std::rc::Rc;

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::{Canvas, Rect, Size};
use kubuno_desktop::views::component::{Control, ControlCore, EventCx, Keys, PaintEventCx};
use kubuno_desktop::views::events::Event;
use kubuno_office_slides_core::model::{SLIDE_H, SLIDE_W};
use kubuno_office_slides_core::render::{Mode, Options, Renderer};
use kubuno_office_slides_core::show::{transition_look, Show};
use kubuno_office_slides_core::surface::Matrix;
use windows::Win32::Graphics::Direct2D::Common::{D2D_RECT_F, D2D1_COLOR_F};
use windows::Win32::Graphics::Direct2D::{D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_LAYER_OPTIONS1_NONE, D2D1_LAYER_PARAMETERS1};

use crate::model::{now_ms, Shared};
use crate::platform::d2d::{D2dSurface, Fonts};
use crate::platform::images::BitmapCache;

/// The control bar's height (`py-2` around 44 px buttons).
const BAR_H: f32 = 60.0;
const BTN: f32 = 44.0;

/// The show ended (Escape, ✕).
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct ShowClosedEventArgs {
    /// The deck index of the slide shown last.
    pub slide: usize,
}

#[derive(kubuno_desktop::views::component::Component, Default)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Presentations")]
#[toolbox(icon = "presentation")]
#[default_event("Closed")]
pub struct SlideShow {
    base: ControlCore,
    #[event]
    #[category("Action")]
    pub closed: Event<ShowClosedEventArgs>,
    doc: Option<Shared>,
    show: Option<Show>,
    fonts: Option<(usize, Rc<Fonts>)>,
    bitmaps: BitmapCache,
    bounds: Rect,
    hover: Option<usize>,
}

fn white(a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: 1.0, g: 1.0, b: 1.0, a }
}

fn black(a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a }
}

impl SlideShow {
    /// Starts the show at deck slide `start`.
    pub fn start(&mut self, doc: Shared, start: usize) {
        let show = Show::new(&doc.ws.borrow().deck, start, now_ms() as f64);
        self.show = Some(show);
        self.doc = Some(doc);
        self.invalidate();
    }

    /// The deck index of the slide shown.
    pub fn current_slide(&self) -> usize {
        self.show.as_ref().and_then(|s| s.visible.get(s.current).copied()).unwrap_or(0)
    }

    fn with_show(&mut self, f: impl FnOnce(&mut Show, &kubuno_office_slides_core::model::Deck, f64)) {
        let (Some(doc), Some(show)) = (self.doc.clone(), self.show.as_mut()) else { return };
        let ws = doc.ws.borrow();
        f(show, &ws.deck, now_ms() as f64);
        drop(ws);
        self.invalidate();
    }

    pub fn next(&mut self) {
        self.with_show(|s, d, now| s.next(d, now));
    }

    pub fn prev(&mut self) {
        self.with_show(|s, _, now| s.prev(now));
    }

    fn close(&mut self) {
        let slide = self.current_slide();
        if let Some(s) = self.show.as_mut() {
            s.closed = true;
        }
        self.raise_closed(ShowClosedEventArgs { slide });
    }

    /// The slide area (above the notes and the bar) and the slide's rectangle in it.
    fn layout(&self) -> (Rect, Rect, Option<Rect>) {
        let b = self.bounds;
        let notes_h = if self.notes_text().is_empty() || self.show.as_ref().is_some_and(|s| s.black) { 0.0 } else { self.notes_height() };
        let area = Rect::new(b.left, b.top, b.right, b.bottom - BAR_H - notes_h);
        let (w, h) = ((b.right - b.left) as f64, ((b.bottom - b.top) as f64 * 0.82).min((area.bottom - area.top) as f64));
        let s = (w / SLIDE_W).min(h / SLIDE_H).max(0.01);
        let (sw, sh) = ((SLIDE_W * s) as f32, (SLIDE_H * s) as f32);
        let x = area.left + ((area.right - area.left) - sw) / 2.0;
        let y = area.top + ((area.bottom - area.top) - sh) / 2.0;
        let notes = (notes_h > 0.0).then(|| Rect::new(b.left, area.bottom, b.right, area.bottom + notes_h));
        (area, Rect::new(x, y, x + sw, y + sh), notes)
    }

    fn notes_text(&self) -> String {
        match (&self.doc, &self.show) {
            (Some(d), Some(s)) => s.notes(&d.ws.borrow().deck).to_string(),
            _ => String::new(),
        }
    }

    /// `px-8 py-4 max-h-32`: the notes' height (text-sm lines of 20 px, at most 128 px).
    fn notes_height(&self) -> f32 {
        let lines = self.notes_text().lines().count().max(1) as f32;
        (32.0 + lines * 20.0).min(128.0)
    }

    /// The control bar's buttons: previous, next, close (the counter and the timer between them).
    fn bar_buttons(&self) -> [Rect; 3] {
        let b = self.bounds;
        let cy = b.bottom - BAR_H / 2.0;
        let mid = (b.left + b.right) / 2.0;
        let prev = Rect::new(mid - 120.0, cy - BTN / 2.0, mid - 120.0 + BTN, cy + BTN / 2.0);
        let next = Rect::new(mid - 20.0, cy - BTN / 2.0, mid - 20.0 + BTN, cy + BTN / 2.0);
        let close = Rect::new(mid + 92.0, cy - BTN / 2.0, mid + 92.0 + BTN, cy + BTN / 2.0);
        [prev, next, close]
    }
}

impl Control for SlideShow {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, _proposed: Size) -> Size {
        Size { width: 960.0, height: 600.0 }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        let b = e.clip_rectangle;
        self.bounds = b;
        let c = e.canvas();
        c.fill_rounded(&b, 0.0, &black(1.0));
        if let Some(renderer) = c.renderer() {
            let key = windows::core::Interface::as_raw(&renderer.dwrite) as usize;
            if self.fonts.as_ref().is_none_or(|(k, _)| *k != key) {
                self.fonts = Some((key, Rc::new(Fonts::new(renderer.dwrite.clone()))));
            }
        }
        let now = now_ms() as f64;
        let moving = match (self.doc.clone(), self.show.as_mut()) {
            (Some(doc), Some(show)) => show.tick(&doc.ws.borrow().deck, now),
            _ => false,
        };
        let (_, slide_rect, notes_rect) = self.layout();
        let (Some(doc), Some(show), Some(renderer), Some((_, fonts))) = (self.doc.clone(), self.show.clone(), c.renderer(), self.fonts.clone()) else {
            e.raise(self, "OnPaint");
            return;
        };
        let ws = doc.ws.borrow();
        if !show.black {
            if let Some(frame) = show.frame(&ws.deck, now) {
                if let Some(slide) = ws.deck.slides.get(frame.slide) {
                    let (sw, sh) = ((slide_rect.right - slide_rect.left) as f64, (slide_rect.bottom - slide_rect.top) as f64);
                    let look = frame.transition.as_ref().map(|(k, p)| transition_look(k, *p));
                    let ctx = &renderer.d2d_context;
                    // The incoming slide's look: a layer for its opacity and its wipe band.
                    let layered = look.is_some_and(|l| l.opacity < 1.0 || l.clip_x != (0.0, 1.0));
                    if let Some(l) = look.filter(|_| layered) {
                        let bounds = D2D_RECT_F {
                            left: slide_rect.left + (l.clip_x.0 * sw) as f32,
                            top: slide_rect.top,
                            right: slide_rect.left + (l.clip_x.1 * sw) as f32,
                            bottom: slide_rect.bottom,
                        };
                        let params = D2D1_LAYER_PARAMETERS1 {
                            contentBounds: bounds,
                            geometricMask: std::mem::ManuallyDrop::new(None),
                            maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                            maskTransform: windows_numerics::Matrix3x2::identity(),
                            opacity: l.opacity.clamp(0.0, 1.0) as f32,
                            opacityBrush: std::mem::ManuallyDrop::new(None),
                            layerOptions: D2D1_LAYER_OPTIONS1_NONE,
                        };
                        unsafe { ctx.PushLayer(&params, None) };
                    }
                    let mut surface = D2dSurface::new(renderer, &fonts, &doc.store, &mut self.bitmaps);
                    let (cx, cy) = (slide_rect.left as f64 + sw / 2.0, slide_rect.top as f64 + sh / 2.0);
                    let mut m = surface.outer().mul(&Matrix::translation(cx, cy));
                    if let Some(l) = look {
                        m = m
                            .mul(&Matrix::translation(l.dx * sw, l.dy * sh))
                            .mul(&Matrix::rotation(l.rotate.to_radians()))
                            .mul(&Matrix::scaling(l.scale * l.squeeze_x, l.scale));
                    }
                    let base = m.mul(&Matrix::translation(-sw / 2.0, -sh / 2.0));
                    let r = Renderer::new(sw, sh, &*fonts);
                    let opts = Options { mode: Mode::Present, hidden: Some(&frame.hidden), animating: frame.animating.as_ref().map(|(id, t)| (id.as_str(), *t)), ..Options::default() };
                    r.render(&mut surface, base, &slide.data, &ws.deck.theme, &opts);
                    if layered {
                        unsafe { ctx.PopLayer() };
                    }
                }
            }
        }
        let f = c.formats();
        if show.show_num && !show.black {
            let t = show.counter();
            let r = Rect::new(slide_rect.right - 76.0, slide_rect.bottom - 34.0, slide_rect.right - 16.0, slide_rect.bottom - 12.0);
            c.fill_rounded(&r, 4.0, &black(0.4));
            c.text(&t, &r, &f.body, &white(0.8), true);
        }
        if (show.autoplay || show.looping) && !show.black {
            let mut parts = Vec::new();
            if show.autoplay {
                parts.push("▶ Lecture auto");
            }
            if show.looping {
                parts.push("↻ Boucle");
            }
            let t = parts.join("  ");
            let r = Rect::new(slide_rect.left + 16.0, slide_rect.bottom - 32.0, slide_rect.left + 16.0 + 160.0, slide_rect.bottom - 12.0);
            c.fill_rounded(&r, 4.0, &black(0.4));
            c.text(&t, &r, &f.caption, &white(0.7), true);
        }
        if let Some(nr) = notes_rect {
            c.fill_rounded(&nr, 0.0, &black(0.8));
            let inner = Rect::new(nr.left + 32.0, nr.top + 16.0, nr.right - 32.0, nr.bottom - 16.0);
            c.text(show.notes(&ws.deck), &inner, &f.body_wrap, &white(1.0), false);
        }
        // The control bar.
        let bar = Rect::new(b.left, b.bottom - BAR_H, b.right, b.bottom);
        c.fill_rounded(&bar, 0.0, &black(0.6));
        let [prev, next, close] = self.bar_buttons();
        let dim = |on: bool| if on { white(1.0) } else { white(0.3) };
        let hover = self.hover;
        for (i, r) in [prev, next, close].iter().enumerate() {
            if hover == Some(i) {
                c.fill_rounded(r, BTN / 2.0, &white(0.12));
            }
        }
        c.vector_icon("ChevronLeft", &prev, 24.0, &dim(show.can_prev()));
        c.vector_icon("ChevronRight", &next, 24.0, &dim(show.can_next(&ws.deck)));
        c.vector_icon("X", &close, 20.0, &white(1.0));
        let counter = Rect::new(prev.right, prev.top, next.left, prev.bottom);
        c.text(&show.counter(), &counter, &f.body, &white(1.0), true);
        let timer = Rect::new(next.right + 12.0, next.top, close.left - 12.0, next.bottom);
        c.text(&show.elapsed(now), &timer, &f.caption, &white(0.7), true);
        drop(ws);
        // The timer ticks every second; reveals, transitions and autoplay every frame.
        kubuno_desktop::controls::host::request_repaint_after(if moving { 16 } else { 1000 - (now as u64 % 1000) as u32 });
        e.raise(self, "OnPaint");
    }

    fn is_input_key(&self, key: Keys) -> bool {
        use kubuno_desktop::controls::host::vk;
        [vk::PAGE_DOWN, vk::PAGE_UP, vk::DOWN, vk::UP, vk::LEFT, vk::RIGHT, vk::HOME, vk::END, vk::ENTER, vk::SPACE, vk::ESCAPE].contains(&key.key.0)
    }

    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y, button) = (e.args().x + self.bounds.left, e.args().y + self.bounds.top, e.args().button);
        self.focus();
        let [prev, next, close] = self.bar_buttons();
        let on_bar = y >= self.bounds.bottom - BAR_H;
        match button {
            MouseButton::Left if close.contains(x, y) => self.close(),
            MouseButton::Left if prev.contains(x, y) => self.prev(),
            MouseButton::Left if next.contains(x, y) => self.next(),
            // The bar and the notes swallow their clicks (`stopPropagation`).
            MouseButton::Left if on_bar || self.layout().2.is_some_and(|r| r.contains(x, y)) => {}
            MouseButton::Left => self.next(),
            MouseButton::Right => self.prev(),
            _ => {}
        }
        e.raise(&*self, "OnMouseDown");
    }

    fn on_mouse_move(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y) = (e.args().x + self.bounds.left, e.args().y + self.bounds.top);
        let hover = self.bar_buttons().iter().position(|r| r.contains(x, y));
        if hover != self.hover {
            self.hover = hover;
            self.invalidate();
        }
        e.raise(&*self, "OnMouseMove");
    }

    fn on_key_down(&mut self, e: &mut EventCx<'_, KeyEventArgs>) {
        if self.key(e.args().key.0) {
            e.args_mut().handled = true;
        }
        e.raise(&*self, "OnKeyDown");
    }
}

impl SlideShow {
    /// The show's keys (`PresenterMode`'s `keydown` handler); true when `k` is one of them. The window also
    /// forwards its own keys here, so they work before the stage has the focus.
    pub fn key(&mut self, k: u16) -> bool {
        use kubuno_desktop::controls::host::vk;
        let now = now_ms() as f64;
        match k {
            _ if k == vk::RIGHT || k == vk::SPACE || k == vk::PAGE_DOWN || k == vk::ENTER => {
                self.next();
                true
            }
            _ if k == vk::LEFT || k == vk::PAGE_UP => {
                self.prev();
                true
            }
            _ if k == vk::ESCAPE => {
                self.close();
                true
            }
            _ if k == vk::HOME => {
                self.with_show(|s, _, _| s.home(now));
                true
            }
            _ if k == vk::END => {
                self.with_show(|s, _, _| s.end(now));
                true
            }
            0x42 => {
                self.with_show(|s, _, _| s.toggle_black());
                true
            }
            0x4E => {
                self.with_show(|s, _, _| s.toggle_num());
                true
            }
            0x50 => {
                self.with_show(|s, _, _| s.toggle_autoplay(now));
                true
            }
            0x4C => {
                self.with_show(|s, _, _| s.toggle_loop());
                true
            }
            _ => false,
        }
    }
}
