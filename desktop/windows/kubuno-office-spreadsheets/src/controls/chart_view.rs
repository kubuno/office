//! The chart controls of the wizard and of the ribbon gallery: `ChartPreview` (the live preview of a draft,
//! `wizard/Preview.tsx`), `ChartTypePicker` (step 1's type list and subtype miniatures, `StepType.tsx`) and
//! `ChartGalleryPanel` (Insertion › Graphique's popup, `chartsGalleryRender`: the catalogue's miniatures by
//! type and « Assistant de diagramme… »). The drawings are the core's primitives (`sheets-core::chart`).

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::graphics::{Brush, Color, Font, FontStyle, Pen, StringAlignment, StringFormat, StringFormatFlags, StringTrimming};
use kubuno_desktop::ui::{Canvas, Rect, Size};
use kubuno_desktop::views::component::{Component, Control, ControlCore, EventCx, PaintEventCx};
use kubuno_desktop::views::events::Event;
use kubuno_office_sheets_core::chart::thumbs::{chart_thumb, H as TH, W as TW};
use kubuno_office_sheets_core::chart::wizard::{catalog, ChartType};
use kubuno_office_sheets_core::chart::Prim;

use super::grid::prims::PrimTarget;

fn hexc(s: &str) -> Color {
    Color::from_hex(s).unwrap_or(Color::BLACK)
}

fn res_or(key: &str, default: &str) -> String {
    let s = kubuno_desktop::resources::string(key);
    if s.is_empty() {
        default.to_string()
    } else {
        s
    }
}

/// A miniature of a (type, subtype) pair scaled into `r`.
pub fn paint_thumb(c: &dyn Canvas, g: &kubuno_desktop::ui::graphics::Graphics<'_>, type_id: &str, subtype: &str, r: Rect) {
    let _ = c;
    let scale = ((r.right - r.left) / TW as f32).min((r.bottom - r.top) / TH as f32);
    let (w, h) = (TW as f32 * scale, TH as f32 * scale);
    let (x, y) = (r.left + ((r.right - r.left) - w) / 2.0, r.top + ((r.bottom - r.top) - h) / 2.0);
    PrimTarget { g, x, y, scale, dark: false, family: "Segoe UI" }.draw(&chart_thumb(type_id, subtype));
}

// ── ChartPreview ────────────────────────────────────────────────────────────

/// The live preview of a draft chart (`WizardPreview`): a white, bordered card holding the drawing.
#[derive(kubuno_desktop::views::component::Component, Default)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Spreadsheets")]
#[toolbox(icon = "chart-column")]
pub struct ChartPreview {
    base: ControlCore,
    scene: Vec<Prim>,
}

impl ChartPreview {
    /// The drawing (the core's primitives at the control's size).
    pub fn set_scene(&mut self, scene: Vec<Prim>) {
        self.scene = scene;
        self.invalidate();
    }
}

impl Control for ChartPreview {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, _proposed: Size) -> Size {
        Size { width: 234.0, height: 180.0 }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        let b = e.clip_rectangle;
        let g = e.graphics;
        let c: &dyn Canvas = e.graphics;
        let dark = c.theme().mode == kubuno_desktop::ui::ThemeMode::Dark;
        g.fill_rectangle(Brush::solid(if dark { Color::from(c.theme().card_background) } else { Color::WHITE }), b);
        let s = g.save();
        g.intersect_clip(b);
        if self.scene.is_empty() && self.design_mode() {
            paint_thumb(c, g, "column", "normal", b);
        } else {
            PrimTarget { g, x: b.left, y: b.top, scale: 1.0, dark, family: "Segoe UI" }.draw(&self.scene);
        }
        g.restore(s);
        g.draw_rectangle(&Pen::new(Color::from(c.theme().card_stroke), 1.0), Rect::new(b.left + 0.5, b.top + 0.5, b.right - 0.5, b.bottom - 0.5));
        e.raise(self, "OnPaint");
    }
}

// ── ChartTypePicker ─────────────────────────────────────────────────────────

/// A type or subtype was picked.
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct ChartPickEventArgs {
    pub type_id: String,
    pub subtype: String,
}

const LIST_W: f32 = 160.0;
const ROW_H: f32 = 30.0;
const CELL_W: f32 = 92.0;
const CELL_H: f32 = 82.0;

/// Step 1 of the wizard: the types on the left (miniature + name), the subtypes of the current type as
/// miniatures with their names on the right.
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Spreadsheets")]
#[toolbox(icon = "chart-pie")]
pub struct ChartTypePicker {
    base: ControlCore,
    /// The current type id (`column`, `bar`, `pie`, …).
    #[property(bindable)]
    #[category("Data")]
    pub type_id: String,
    /// The current subtype id.
    #[property(bindable)]
    #[category("Data")]
    pub subtype: String,
    /// A type or subtype was clicked.
    #[event]
    #[category("Action")]
    pub picked: Event<ChartPickEventArgs>,
    types: Vec<ChartType>,
    hot: Option<(bool, usize)>,
}

impl Default for ChartTypePicker {
    fn default() -> Self {
        Self { base: ControlCore::default(), type_id: "column".into(), subtype: "normal".into(), picked: Event::default(), types: catalog(), hot: None }
    }
}

impl ChartTypePicker {
    fn current(&self) -> usize {
        self.types.iter().position(|t| t.id == self.type_id).unwrap_or(0)
    }

    fn hit(&self, x: f32, y: f32) -> Option<(bool, usize)> {
        if x < LIST_W {
            let i = ((y - 4.0) / ROW_H).floor();
            return (i >= 0.0 && (i as usize) < self.types.len()).then_some((true, i as usize));
        }
        let t = &self.types[self.current()];
        let per_row = (((self.size().width - LIST_W - 24.0) / (CELL_W + 8.0)).floor() as usize).max(1);
        for (i, _) in t.subtypes.iter().enumerate() {
            let (col, row) = ((i % per_row) as f32, (i / per_row) as f32);
            let left = LIST_W + 12.0 + col * (CELL_W + 8.0);
            let top = 32.0 + row * (CELL_H + 8.0);
            if x >= left && x < left + CELL_W && y >= top && y < top + CELL_H {
                return Some((false, i));
            }
        }
        None
    }
}

impl Control for ChartTypePicker {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, _proposed: Size) -> Size {
        Size { width: 480.0, height: 300.0 }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        let b = e.clip_rectangle;
        let g = e.graphics;
        let c: &dyn Canvas = e.graphics;
        let t = c.theme();
        let dark = t.mode == kubuno_desktop::ui::ThemeMode::Dark;
        let primary = hexc("#1a73e8");
        let light = if dark { Color::rgba_f(0.1, 0.45, 0.91, 0.25) } else { hexc("#e8f0fe") };
        let hover = if dark { Color::rgba_f(1.0, 1.0, 1.0, 0.06) } else { hexc("#f1f3f4") };
        let ink = Color::from(t.text_primary);
        let ink2 = Color::from(t.text_secondary);
        let border = Color::from(t.card_stroke);
        let fmt = StringFormat { alignment: StringAlignment::Near, line_alignment: StringAlignment::Center, trimming: StringTrimming::EllipsisCharacter, flags: StringFormatFlags::NO_WRAP };
        let font = Font::with_dip("Segoe UI", 12.0, FontStyle::REGULAR);
        let cur = self.current();
        g.draw_line(&Pen::new(border, 1.0), kubuno_desktop::ui::graphics::PointF::new(b.left + LIST_W - 0.5, b.top), kubuno_desktop::ui::graphics::PointF::new(b.left + LIST_W - 0.5, b.bottom));
        for (i, ty) in self.types.iter().enumerate() {
            let r = Rect::new(b.left, b.top + 4.0 + i as f32 * ROW_H, b.left + LIST_W - 1.0, b.top + 4.0 + (i + 1) as f32 * ROW_H);
            if i == cur {
                g.fill_rectangle(Brush::solid(light), r);
            } else if self.hot == Some((true, i)) {
                g.fill_rectangle(Brush::solid(hover), r);
            }
            paint_thumb(c, g, ty.id, ty.subtypes[0].id, Rect::new(r.left + 10.0, r.top + 5.0, r.left + 38.0, r.top + 25.0));
            g.draw_string(&res_or(ty.label_key, ty.label), &font, Brush::solid(if i == cur { primary } else { ink }), Rect::new(r.left + 46.0, r.top, r.right - 4.0, r.bottom), &fmt);
        }
        let ty = &self.types[cur];
        g.draw_string(&res_or(ty.label_key, ty.label), &font, Brush::solid(ink2), Rect::new(b.left + LIST_W + 12.0, b.top + 6.0, b.right - 8.0, b.top + 26.0), &fmt);
        let per_row = (((b.right - b.left - LIST_W - 24.0) / (CELL_W + 8.0)).floor() as usize).max(1);
        let small = Font::with_dip("Segoe UI", 10.0, FontStyle::REGULAR);
        let cfmt = StringFormat { alignment: StringAlignment::Center, ..fmt };
        for (i, st) in ty.subtypes.iter().enumerate() {
            let (col, row) = ((i % per_row) as f32, (i / per_row) as f32);
            let left = b.left + LIST_W + 12.0 + col * (CELL_W + 8.0);
            let top = b.top + 32.0 + row * (CELL_H + 8.0);
            let r = Rect::new(left, top, left + CELL_W, top + CELL_H);
            let selected = st.id == self.subtype;
            if selected {
                g.fill_rounded_rectangle(Brush::solid(light), r, 4.0);
            } else if self.hot == Some((false, i)) {
                g.fill_rounded_rectangle(Brush::solid(hover), r, 4.0);
            }
            g.draw_rounded_rectangle(&Pen::new(if selected { primary } else { hexc("#dadce0") }, 1.0), r, 4.0);
            paint_thumb(c, g, ty.id, st.id, Rect::new(left + 10.0, top + 8.0, left + CELL_W - 10.0, top + 58.0));
            g.draw_string(&res_or(st.label_key, st.label), &small, Brush::solid(ink), Rect::new(left + 2.0, top + 60.0, left + CELL_W - 2.0, top + CELL_H - 4.0), &cfmt);
        }
        e.raise(self, "OnPaint");
    }

    fn on_mouse_move(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let h = self.hit(e.args().x, e.args().y);
        if h != self.hot {
            self.hot = h;
            self.invalidate();
        }
        e.raise(&*self, "OnMouseMove");
    }

    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        if e.args().button == MouseButton::Left {
            match self.hit(e.args().x, e.args().y) {
                Some((true, i)) => {
                    let ty = &self.types[i];
                    if ty.id != self.type_id {
                        let args = ChartPickEventArgs { type_id: ty.id.into(), subtype: ty.subtypes[0].id.into() };
                        self.raise_picked(args);
                    }
                }
                Some((false, i)) => {
                    let ty = &self.types[self.current()];
                    let args = ChartPickEventArgs { type_id: ty.id.into(), subtype: ty.subtypes[i].id.into() };
                    self.raise_picked(args);
                }
                None => {}
            }
        }
        e.raise(&*self, "OnMouseDown");
    }
}

// ── ChartGalleryPanel ───────────────────────────────────────────────────────

const GAL_W: f32 = 296.0;
const GAL_THUMB_W: f32 = 64.0;
const GAL_THUMB_H: f32 = 46.0;
const GAL_HEAD: f32 = 16.0;

/// The ribbon gallery's popup (`chartsGalleryRender`): every type's subtypes as miniatures, then the
/// wizard's entry. `picked` carries the pair, or `type_id = "wizard"`.
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Spreadsheets")]
#[toolbox(icon = "chart-bar")]
pub struct ChartGalleryPanel {
    base: ControlCore,
    #[event]
    #[category("Action")]
    pub picked: Event<ChartPickEventArgs>,
    types: Vec<ChartType>,
    hot: Option<(usize, usize)>,
}

impl Default for ChartGalleryPanel {
    fn default() -> Self {
        Self { base: ControlCore::default(), picked: Event::default(), types: catalog(), hot: None }
    }
}

/// The cells of the gallery: (type, subtype, rect) and the heading rects, and the wizard row's top.
fn gallery_layout(types: &[ChartType]) -> (Vec<(usize, usize, Rect)>, Vec<(usize, Rect)>, f32) {
    let mut cells = Vec::new();
    let mut heads = Vec::new();
    let mut y = 8.0;
    let per_row = ((GAL_W - 16.0 + 4.0) / (GAL_THUMB_W + 4.0)).floor().max(1.0) as usize;
    for (ti, t) in types.iter().enumerate() {
        heads.push((ti, Rect::new(12.0, y, GAL_W - 12.0, y + GAL_HEAD)));
        y += GAL_HEAD;
        for (si, _) in t.subtypes.iter().enumerate() {
            let (col, row) = ((si % per_row) as f32, (si / per_row) as f32);
            let left = 8.0 + col * (GAL_THUMB_W + 4.0);
            let top = y + row * (GAL_THUMB_H + 4.0);
            cells.push((ti, si, Rect::new(left, top, left + GAL_THUMB_W, top + GAL_THUMB_H)));
        }
        let rows = t.subtypes.len().div_ceil(per_row) as f32;
        y += rows * (GAL_THUMB_H + 4.0) + 6.0;
    }
    (cells, heads, y)
}

impl ChartGalleryPanel {
    pub const WIDTH: f32 = GAL_W;

    /// The panel's height.
    pub fn height() -> f32 {
        gallery_layout(&catalog()).2 + 40.0
    }

    fn hit(&self, x: f32, y: f32) -> Option<(usize, usize)> {
        let (cells, _, wizard_top) = gallery_layout(&self.types);
        if y >= wizard_top + 4.0 && y < wizard_top + 36.0 {
            return Some((usize::MAX, 0));
        }
        cells.into_iter().find(|(_, _, r)| r.contains(x, y)).map(|(t, s, _)| (t, s))
    }
}

impl Control for ChartGalleryPanel {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, _proposed: Size) -> Size {
        Size { width: GAL_W, height: Self::height() }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        let b = e.clip_rectangle;
        let g = e.graphics;
        let c: &dyn Canvas = e.graphics;
        let t = c.theme();
        let ink3 = Color::from(t.text_secondary);
        let ink = Color::from(t.text_primary);
        let primary = hexc("#1a73e8");
        let (cells, heads, wizard_top) = gallery_layout(&self.types);
        let head_font = Font::with_dip("Segoe UI", 10.0, FontStyle::REGULAR);
        let fmt = StringFormat { alignment: StringAlignment::Near, line_alignment: StringAlignment::Center, trimming: StringTrimming::EllipsisCharacter, flags: StringFormatFlags::NO_WRAP };
        for (ti, r) in heads {
            let ty = &self.types[ti];
            let text = res_or(ty.label_key, ty.label).to_uppercase();
            g.draw_string(&text, &head_font, Brush::solid(ink3), Rect::new(b.left + r.left, b.top + r.top, b.left + r.right, b.top + r.bottom), &fmt);
        }
        for (ti, si, r) in cells {
            let rr = Rect::new(b.left + r.left, b.top + r.top, b.left + r.right, b.top + r.bottom);
            if self.hot == Some((ti, si)) {
                g.fill_rounded_rectangle(Brush::solid(hexc("#e8f0fe")), rr, 4.0);
                g.draw_rounded_rectangle(&Pen::new(primary, 1.0), rr, 4.0);
            }
            let ty = &self.types[ti];
            paint_thumb(c, g, ty.id, ty.subtypes[si].id, Rect::new(rr.left + 4.0, rr.top + 4.0, rr.right - 4.0, rr.bottom - 4.0));
        }
        let line_y = b.top + wizard_top;
        g.draw_line(&Pen::new(hexc("#e2e4e6"), 1.0), kubuno_desktop::ui::graphics::PointF::new(b.left + 8.0, line_y), kubuno_desktop::ui::graphics::PointF::new(b.right - 8.0, line_y));
        let row = Rect::new(b.left + 8.0, line_y + 4.0, b.right - 8.0, line_y + 34.0);
        if self.hot == Some((usize::MAX, 0)) {
            g.fill_rectangle(Brush::solid(hexc("#f1f3f4")), row);
        }
        c.vector_icon("ChartColumn", &Rect::new(row.left + 6.0, row.top + 8.0, row.left + 20.0, row.top + 22.0), 14.0, &t.text_primary);
        let label = format!("{}…", res_or("sheet_chart_wizard_title", "Assistant de diagramme"));
        g.draw_string(&label, &Font::with_dip("Segoe UI", 12.0, FontStyle::REGULAR), Brush::solid(ink), Rect::new(row.left + 28.0, row.top, row.right, row.bottom), &fmt);
        e.raise(self, "OnPaint");
    }

    fn on_mouse_move(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let h = self.hit(e.args().x, e.args().y);
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

    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        if e.args().button == MouseButton::Left {
            match self.hit(e.args().x, e.args().y) {
                Some((usize::MAX, _)) => self.raise_picked(ChartPickEventArgs { type_id: "wizard".into(), subtype: String::new() }),
                Some((t, s)) => {
                    let ty = &self.types[t];
                    let args = ChartPickEventArgs { type_id: ty.id.into(), subtype: ty.subtypes[s].id.into() };
                    self.raise_picked(args);
                }
                None => {}
            }
        }
        e.raise(&*self, "OnMouseDown");
    }
}
