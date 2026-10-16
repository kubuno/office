//! `ShapeGalleryPanel` - Insertion › Formes (`shapes/ShapesInsertButton.tsx`, `shapes/ShapeGallery.tsx`): the
//! office shape catalogue in a popup, « Formes récemment utilisées » first when there are some, then one
//! section per catalogue group (the text box excepted, as the sheet's `SHAPE_GROUPS` drops it), each
//! thumbnail the real geometry as a grey outline on white. A pick arms the kind (the next drag on the grid
//! draws it) and moves it to the head of the recents (12 at most, the file the other office apps of this
//! computer share).

use std::path::PathBuf;
use std::rc::Rc;

use kubuno_desktop::popup::{Placement, Popup};
use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::graphics::{Brush, Color, Font, FontStyle, StringAlignment, StringFormat, StringFormatFlags, StringTrimming};
use kubuno_desktop::ui::{Canvas, Rect, Size};
use kubuno_desktop::views::component::{Control, ControlCore, EventCx, PaintEventCx};
use kubuno_desktop::views::events::Event;
use kubuno_desktop::DockStyle;
use kubuno_office_shapes_core::catalog::SHAPE_CATALOG;

use super::grid::shapes::{paint_glyph, ShapeBox};

/// A kind was picked.
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct ShapePickedEventArgs {
    pub kind: String,
}

const WIDTH: f32 = 268.0;
const PAD: f32 = 8.0;
const THUMB: f32 = 28.0;
const GAP: f32 = 2.0;
const HEAD: f32 = 20.0;
const SECTION_GAP: f32 = 6.0;
const MAX_RECENT: usize = 12;

fn res_or(key: &str, default: &str) -> String {
    let s = kubuno_desktop::resources::string(key);
    if s.is_empty() {
        default.to_string()
    } else {
        s
    }
}

fn recents_path() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
    Some(base.join("Kubuno").join("office").join("recent-shapes.txt"))
}

fn known(kind: &str) -> bool {
    SHAPE_CATALOG.iter().flat_map(|c| c.shapes.iter()).any(|s| s.kind == kind && s.kind != "textBox")
}

/// `loadRecentShapes`.
pub fn load_recents() -> Vec<String> {
    recents_path().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| t.lines().map(str::trim).filter(|k| known(k)).map(str::to_string).take(MAX_RECENT).collect()).unwrap_or_default()
}

/// `pushRecentShape`.
pub fn push_recent(kind: &str) {
    let mut v: Vec<String> = std::iter::once(kind.to_string()).chain(load_recents().into_iter().filter(|k| k != kind)).collect();
    v.truncate(MAX_RECENT);
    let Some(path) = recents_path() else { return };
    if let Some(dir) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            kubuno_desktop::tracing::warn!("[spreadsheets] recent shapes: {e}");
            return;
        }
    }
    if let Err(e) = std::fs::write(&path, v.join("\n")) {
        kubuno_desktop::tracing::warn!("[spreadsheets] recent shapes: {e}");
    }
}

struct Group {
    title: String,
    kinds: Vec<String>,
    recent: bool,
}

fn groups() -> Vec<Group> {
    let mut v = Vec::new();
    let rec = load_recents();
    if !rec.is_empty() {
        v.push(Group { title: res_or("sheet_shape_grp_recent", "Formes récemment utilisées"), kinds: rec, recent: true });
    }
    for c in SHAPE_CATALOG {
        let kinds: Vec<String> = c.shapes.iter().filter(|s| s.kind != "textBox").map(|s| s.kind.to_string()).collect();
        if !kinds.is_empty() {
            v.push(Group { title: res_or(&format!("sheet_shape_grp_{}", c.id), c.title), kinds, recent: false });
        }
    }
    v
}

type Layout = (Vec<(usize, Rect)>, Vec<(usize, usize, Rect)>, f32);

fn layout(gs: &[Group]) -> Layout {
    let per_row = (((WIDTH - 2.0 * PAD + GAP) / (THUMB + GAP)).floor() as usize).max(1);
    let (mut heads, mut cells) = (Vec::new(), Vec::new());
    let mut y = PAD;
    for (gi, g) in gs.iter().enumerate() {
        if gi > 0 {
            y += SECTION_GAP;
        }
        let top = if g.recent { 0.0 } else { 2.0 };
        heads.push((gi, Rect::new(PAD + 4.0, y + top, WIDTH - PAD - 4.0, y + top + HEAD - 4.0)));
        y += top + HEAD;
        for i in 0..g.kinds.len() {
            let (row, col) = ((i / per_row) as f32, (i % per_row) as f32);
            let (l, t) = (PAD + col * (THUMB + GAP), y + row * (THUMB + GAP));
            cells.push((gi, i, Rect::new(l, t, l + THUMB, t + THUMB)));
        }
        let rows = g.kinds.len().div_ceil(per_row) as f32;
        if rows > 0.0 {
            y += rows * THUMB + (rows - 1.0) * GAP;
        }
    }
    (heads, cells, y + PAD)
}

/// The gallery (see the module doc).
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Spreadsheets")]
#[toolbox(icon = "shapes")]
pub struct ShapeGalleryPanel {
    base: ControlCore,
    #[event]
    #[category("Action")]
    pub shape_picked: Event<ShapePickedEventArgs>,
    /// The armed kind (highlighted).
    pub current: String,
    groups: Vec<Group>,
    scroll: f32,
    hot: Option<(usize, usize)>,
}

impl Default for ShapeGalleryPanel {
    fn default() -> Self {
        Self { base: ControlCore::default(), shape_picked: Event::default(), current: String::new(), groups: groups(), scroll: 0.0, hot: None }
    }
}

impl ShapeGalleryPanel {
    fn hit(&self, x: f32, y: f32) -> Option<(usize, usize)> {
        let (_, cells, _) = layout(&self.groups);
        cells.into_iter().find(|(_, _, r)| r.contains(x, y + self.scroll)).map(|(g, i, _)| (g, i))
    }

    /// Opens the gallery under the pointer; `on_pick` gets the kind.
    pub fn open(armed: &str, on_pick: impl Fn(String) + 'static) {
        let owner = kubuno_desktop::popup::current_window();
        let geometry = kubuno_desktop::controls::host::screen_geometry(owner);
        let work_h = geometry.as_ref().map_or(800.0, |g| g.work_area.3 - g.work_area.1);
        let height = layout(&groups()).2.min((work_h * 0.64).max(120.0));
        let gallery = Custom::<ShapeGalleryPanel>::new().dock(DockStyle::Fill).name("gallery");
        let back = {
            let c = kubuno_desktop::Application::theme().card_background;
            let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            format!("#{:02X}{:02X}{:02X}", byte(c.r), byte(c.g), byte(c.b))
        };
        let popup = Popup::new(&gallery, WIDTH, height).corner_radius(12.0).back_color(&back).title(&res_or("sheet_shapes", "Formes")).placement(Placement::BottomStart).gap(4.0);
        let form = popup.form().clone();
        {
            let (gallery, armed) = (gallery.clone(), armed.to_string());
            form.load().subscribe(move |_f, _e| {
                let armed = armed.clone();
                gallery.with(|g| {
                    g.current = armed;
                    g.invalidate();
                });
            });
        }
        {
            let (form, on_pick) = (form.clone(), Rc::new(on_pick));
            gallery.on::<ShapePickedEventArgs>("OnShapePicked").subscribe(move |_g, e| {
                push_recent(&e.kind);
                on_pick(e.kind.clone());
                form.close();
            });
        }
        popup.show(owner, super::anchor_at_pointer(owner));
    }
}

impl Control for ShapeGalleryPanel {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, _proposed: Size) -> Size {
        Size { width: WIDTH, height: layout(&self.groups).2 }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        let b = e.clip_rectangle;
        let g = e.graphics;
        let c: &dyn Canvas = e.graphics;
        let t = c.theme();
        let dark = t.mode == kubuno_desktop::ui::ThemeMode::Dark;
        let (heads, cells, _) = layout(&self.groups);
        let saved = g.save();
        g.intersect_clip(b);
        let font = Font::with_dip("Segoe UI", 11.0, FontStyle::BOLD);
        let fmt = StringFormat { alignment: StringAlignment::Near, line_alignment: StringAlignment::Center, trimming: StringTrimming::EllipsisCharacter, flags: StringFormatFlags::NO_WRAP };
        for (gi, r) in heads {
            let rr = Rect::new(b.left + r.left, b.top + r.top - self.scroll, b.left + r.right, b.top + r.bottom - self.scroll);
            g.draw_string(&self.groups[gi].title, &font, Brush::solid(Color::from(t.text_secondary)), rr, &fmt);
        }
        let hover = if dark { Color::rgba_f(1.0, 1.0, 1.0, 0.08) } else { Color::from_hex("#f1f3f4").unwrap_or(Color::WHITE) };
        let armed_bg = if dark { Color::rgba_f(0.1, 0.45, 0.91, 0.25) } else { Color::from_hex("#e8f0fe").unwrap_or(Color::WHITE) };
        for (gi, i, r) in cells {
            let rr = Rect::new(b.left + r.left, b.top + r.top - self.scroll, b.left + r.right, b.top + r.bottom - self.scroll);
            let kind = &self.groups[gi].kinds[i];
            if *kind == self.current {
                g.fill_rounded_rectangle(Brush::solid(armed_bg), rr, 4.0);
            } else if self.hot == Some((gi, i)) {
                g.fill_rounded_rectangle(Brush::solid(hover), rr, 4.0);
            }
            let glyph = serde_json::json!({ "kind": kind, "fill": "#ffffff", "border": "#5f6470", "borderWidth": 1 });
            let (gw, gh) = (22.0f32, 19.0f32);
            paint_glyph(g, &glyph, &ShapeBox { x: rr.left + (THUMB - gw) / 2.0, y: rr.top + (THUMB - gh) / 2.0, w: gw, h: gh, zoom: 1.0 }, false);
        }
        g.restore(saved);
        e.raise(self, "OnPaint");
    }

    fn tool_tip_at(&self, x: f32, y: f32) -> Option<String> {
        let (g, i) = self.hit(x, y)?;
        let kind = &self.groups[g].kinds[i];
        Some(kubuno_office_shapes_core::catalog::label_of(kind).unwrap_or(kind).to_string())
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
        let max = (layout(&self.groups).2 - self.size().height).max(0.0);
        self.scroll = (self.scroll + e.args().delta * 48.0).clamp(0.0, max);
        self.invalidate();
        e.raise(&*self, "OnMouseWheel");
    }

    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        if e.args().button == MouseButton::Left {
            if let Some((g, i)) = self.hit(e.args().x, e.args().y) {
                let kind = self.groups[g].kinds[i].clone();
                self.raise_shape_picked(ShapePickedEventArgs { kind });
            }
        }
        e.raise(&*self, "OnMouseDown");
    }
}
