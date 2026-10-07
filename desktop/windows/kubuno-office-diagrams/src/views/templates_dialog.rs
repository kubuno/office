//! Code-behind of the template gallery (`templates_dialog.kbview`), and its grid of cards ([`TemplateGrid`]):
//! each template's preview drawn by the core's `renderCanvas` port, as the web's `renderIoPreview` does.

use kubuno_desktop::prelude::*;
use kubuno_desktop::views::events::ItemEventArgs;
use kubuno_office_diagrams_core::templates::DiagramTemplate;

pub use grid::TemplateGrid;

#[kubuno_desktop::view("templates_dialog.kbview")]
pub struct TemplatesDialog {
    picked: Option<usize>,
}

impl TemplatesDialog {
    /// Shows the gallery over `owner`; the template picked, `None` when cancelled.
    pub fn pick(owner: &dyn AsForm) -> Option<&'static DiagramTemplate> {
        let mut d = Self::default();
        d.initialize_component();
        if d.show_dialog(owner) == DialogResult::Ok {
            d.picked.and_then(|i| kubuno_office_diagrams_core::templates::TEMPLATES.get(i))
        } else {
            None
        }
    }

    fn cancel_click(&mut self) {
        self.set_dialog_result(DialogResult::Cancel);
        self.close();
    }

    fn grid_template_picked(&mut self, e: &ItemEventArgs) {
        self.picked = Some(e.index);
        self.set_dialog_result(DialogResult::Ok);
        self.close();
    }
}

/// `renderIoPreview`'s view of shapes whose corners span `(min_x, min_y)`–`(max_x, max_y)` in a `w`×`h`
/// image: the scale and the pan (a 10 px margin, centred).
pub fn preview_view(min_x: f64, min_y: f64, max_x: f64, max_y: f64, w: f64, h: f64) -> (f64, f64, f64) {
    let pad = 10.0;
    let (cw, ch) = ((max_x - min_x) + 2.0 * pad, (max_y - min_y) + 2.0 * pad);
    let scale = (w / cw).min(h / ch);
    (scale, (pad - min_x) * scale + (w - cw * scale) / 2.0, (pad - min_y) * scale + (h - ch * scale) / 2.0)
}

mod grid {
    use kubuno_desktop::prelude::*;
    use kubuno_desktop::ui::{Canvas, Rect, Size};
    use kubuno_desktop::views::component::{Control, ControlCore, EventCx, PaintEventCx};
    use kubuno_desktop::views::events::{EmptyEventArgs, Event, ItemEventArgs};
    use kubuno_office_diagrams_core::canvas::{Ctx2D, Matrix};
    use kubuno_office_diagrams_core::geometry::GRID_SIZE;
    use kubuno_office_diagrams_core::io::IoData;
    use kubuno_office_diagrams_core::render::{render_canvas, Scene};
    use kubuno_office_diagrams_core::templates::TEMPLATES;
    use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;

    use crate::platform::surface::{D2DSurface, SurfaceCache};
    use crate::Resources;

    /// `grid-cols-3 gap-3`.
    const COLS: usize = 3;
    const GAP: f32 = 12.0;
    /// A card: `p-2 gap-2`, the preview `h-[112px]`, the name `text-xs`.
    const PAD: f32 = 8.0;
    const PREVIEW_H: f32 = 112.0;
    const NAME_H: f32 = 16.0;
    const CARD_H: f32 = PAD + PREVIEW_H + PAD + NAME_H + PAD;
    /// The preview image (`renderIoPreview(io, 168, 112)`).
    const IMAGE_W: f32 = 168.0;

    /// The cards of `n` templates in a `w`-wide box at the origin.
    pub(super) fn cards(n: usize, w: f32) -> Vec<Rect> {
        let cw = ((w - GAP * (COLS - 1) as f32) / COLS as f32).max(1.0);
        (0..n)
            .map(|i| {
                let (col, row) = ((i % COLS) as f32, (i / COLS) as f32);
                let (x, y) = (col * (cw + GAP), row * (CARD_H + GAP));
                Rect::new(x, y, x + cw, y + CARD_H)
            })
            .collect()
    }

    fn with_alpha(c: D2D1_COLOR_F, a: f32) -> D2D1_COLOR_F {
        D2D1_COLOR_F { a: c.a * a, ..c }
    }

    /// The template gallery's grid: a card per template (its preview, or « Vierge » for an empty one, and
    /// its name); a click raises `TemplatePicked` with the template's index in `TEMPLATES`.
    #[derive(kubuno_desktop::views::component::Component)]
    #[kubuno(extends = Control, overrides(Control))]
    #[category("Diagrams")]
    #[toolbox(icon = "layout-template")]
    #[default_event("TemplatePicked")]
    pub struct TemplateGrid {
        base: ControlCore,
        /// Occurs when a template is clicked (`e.index`).
        #[event]
        #[category("Action")]
        pub template_picked: Event<ItemEventArgs>,
        /// The templates, built once.
        built: Vec<IoData>,
        cache: SurfaceCache,
        hot: Option<usize>,
        pressed: Option<usize>,
    }

    impl Default for TemplateGrid {
        fn default() -> Self {
            Self {
                base: ControlCore::default(),
                template_picked: Event::default(),
                built: TEMPLATES.iter().map(|t| (t.build)()).collect(),
                cache: SurfaceCache::default(),
                hot: None,
                pressed: None,
            }
        }
    }

    impl TemplateGrid {
        fn at(&self, x: f32, y: f32) -> Option<usize> {
            cards(self.built.len(), self.size().width).iter().position(|r| r.contains(x, y))
        }

        /// Draws template `i`'s preview in `r` (a `IMAGE_W`×`PREVIEW_H` image, centred).
        fn paint_preview(&self, c: &dyn kubuno_desktop::controls::ControlCanvas, i: usize, r: &Rect) {
            let Some(io) = self.built.get(i) else { return };
            let Some(renderer) = c.renderer() else { return };
            let Some((min_x, min_y, max_x, max_y)) = io.shapes.iter().fold(None, |acc: Option<(f64, f64, f64, f64)>, s| {
                let (x0, y0, x1, y1) = (s.x(), s.y(), s.x() + s.w(), s.y() + s.h());
                Some(match acc {
                    None => (x0, y0, x1, y1),
                    Some((a, b, c2, d)) => (a.min(x0), b.min(y0), c2.max(x1), d.max(y1)),
                })
            }) else {
                return;
            };
            let (w, h) = (IMAGE_W.min(r.right - r.left), PREVIEW_H);
            let (scale, pan_x, pan_y) = super::preview_view(min_x, min_y, max_x, max_y, w as f64, h as f64);
            let left = (r.left + r.right - w) / 2.0;
            let Ok(mut surface) = D2DSurface::new(&renderer.d2d_context, &renderer.d2d_factory, &renderer.dwrite, &self.cache) else { return };
            let image = Rect::new(left, r.top, left + w, r.top + h);
            c.push_clip(&image);
            {
                let mut ctx = Ctx2D::new(&mut surface);
                ctx.set_transform(Matrix::translation(image.left as f64, image.top as f64));
                render_canvas(
                    &mut ctx,
                    &Scene {
                        shapes: &io.shapes,
                        connectors: &io.connectors,
                        zoom: scale,
                        pan_x,
                        pan_y,
                        selected: &[],
                        selected_conns: &[],
                        hovered: None,
                        drawing_conn: None,
                        lasso: None,
                        bg_color: "#ffffff",
                        magnet_segs: &[],
                        show_grid: false,
                        grid_size: GRID_SIZE,
                        guides: None,
                        drawing_shape: None,
                        width: w as f64,
                        height: h as f64,
                    },
                );
            }
            c.pop_clip();
        }
    }

    impl Control for TemplateGrid {
        fn get_preferred_size(&self, _canvas: &dyn Canvas, proposed: Size) -> Size {
            let rows = self.built.len().div_ceil(COLS) as f32;
            Size { width: proposed.width.max(600.0), height: rows * CARD_H + (rows - 1.0).max(0.0) * GAP }
        }

        fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
            let b = e.clip_rectangle;
            let cc = e.canvas();
            let c: &dyn Canvas = e.graphics;
            let t = c.theme();
            for (i, card) in cards(self.built.len(), b.right - b.left).iter().enumerate() {
                let r = Rect::new(b.left + card.left, b.top + card.top, b.left + card.right, b.top + card.bottom);
                // `border border-border rounded-lg p-2 hover:border-primary hover:bg-primary/5`.
                let hot = self.hot == Some(i);
                if hot {
                    c.fill_rounded(&r, 8.0, &with_alpha(t.accent, 0.05));
                }
                c.stroke_rounded(&Rect::new(r.left + 0.5, r.top + 0.5, r.right - 0.5, r.bottom - 0.5), 8.0, if hot { &t.accent } else { &t.card_stroke });
                // `w-full h-[112px] bg-surface-1 rounded`.
                let preview = Rect::new(r.left + PAD, r.top + PAD, r.right - PAD, r.top + PAD + PREVIEW_H);
                c.fill_rounded(&preview, 4.0, &t.surface_2);
                if self.built.get(i).is_some_and(|io| !io.shapes.is_empty()) {
                    self.paint_preview(cc, i, &preview);
                } else {
                    c.text(Resources::templates_blank(), &preview, &c.formats().caption, &t.text_tertiary, true);
                }
                let name = Rect::new(r.left + PAD, preview.bottom + PAD, r.right - PAD, preview.bottom + PAD + NAME_H);
                let label = TEMPLATES.get(i).map(|t| t.name).unwrap_or_default();
                c.text(label, &name, &c.formats().caption, &t.text_secondary, true);
            }
            e.raise(self, "OnPaint");
        }

        fn tool_tip_at(&self, x: f32, y: f32) -> Option<String> {
            self.at(x, y).and_then(|i| TEMPLATES.get(i)).map(|t| t.name.to_string())
        }

        fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
            if e.args().button == MouseButton::Left {
                self.pressed = self.at(e.args().x, e.args().y);
            }
            e.raise(&*self, "OnMouseDown");
        }

        fn on_mouse_move(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
            let hot = self.at(e.args().x, e.args().y);
            if hot != self.hot {
                self.hot = hot;
                self.invalidate();
            }
            e.raise(&*self, "OnMouseMove");
        }

        fn on_mouse_up(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
            let released = self.at(e.args().x, e.args().y);
            if let Some(i) = self.pressed.take().filter(|p| Some(*p) == released) {
                self.raise_template_picked(ItemEventArgs { index: i });
            }
            e.raise(&*self, "OnMouseUp");
        }

        fn on_mouse_leave(&mut self, e: &mut EventCx<'_, EmptyEventArgs>) {
            if self.hot.take().is_some() {
                self.invalidate();
            }
            e.raise(&*self, "OnMouseLeave");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_preview_is_centred_with_a_ten_pixel_margin() {
        // 100×50 of shapes at (0, 0) in 168×112: the width limits, scale = 168 / 120.
        let (scale, px, py) = preview_view(0.0, 0.0, 100.0, 50.0, 168.0, 112.0);
        assert!((scale - 1.4).abs() < 1e-9);
        assert!((px - 14.0).abs() < 1e-9);
        assert!((py - (14.0 + (112.0 - 70.0 * 1.4) / 2.0)).abs() < 1e-9);
    }

    #[test]
    fn the_cards_are_three_columns() {
        let cards = grid::cards(8, 648.0);
        assert_eq!(cards.len(), 8);
        assert_eq!(cards[3].left, 0.0);
        assert!(cards[3].top > cards[0].bottom);
        assert!((cards[2].right - 648.0).abs() < 0.01);
    }
}
