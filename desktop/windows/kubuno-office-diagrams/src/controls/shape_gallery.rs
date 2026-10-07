//! `ShapeGallery` — Insertion › Formes (`shapes/ShapesInsertButton.tsx`, `shapes/ShapeGallery.tsx`): the office
//! shape catalogue in a popup (`w-[268px] p-2 bg-surface-1 rounded-xl shadow-2xl`, at most 64 % of the screen
//! high, scrolling past that): « Formes récemment utilisées » on top when there are some, then one section per
//! catalogue group (an `text-[11px] font-semibold text-secondary` heading over `w-7 h-7` thumbnails, `gap-0.5`,
//! hover `bg-surface-2`, the armed kind `bg-primary-light`), each glyph the real geometry as a grey outline on
//! white (22×19, `#5f6470`, 1 px) drawn by the shared engine. A pick moves the kind to the head of the recents
//! (12 at most, shared by the office apps of this computer) and closes the popup.

use std::path::PathBuf;
use std::rc::Rc;

use kubuno_desktop::popup::{Placement, Popup};
use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::range::{ScrollBar, ScrollPart};
use kubuno_desktop::ui::{Canvas, Rect, Size, Widget, WidgetState};
use kubuno_desktop::views::component::{AccessiblePart, Control, ControlCore, EventCx, PaintEventCx};
use kubuno_desktop::views::events::Event;
use kubuno_desktop::DockStyle;
use kubuno_office_diagrams_core::canvas::{Ctx2D, Matrix};
use kubuno_office_diagrams_core::{render, stencils};

use crate::platform::surface::{D2DSurface, SurfaceCache};

/// A kind was picked in the gallery.
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct ShapePickedEventArgs {
    pub kind: String,
}

/// The panel's width (`width = 268`: 8 thumbnails a row).
pub const WIDTH: f32 = 268.0;
/// `p-2`.
const PAD: f32 = 8.0;
/// `w-7 h-7`, `gap-0.5`.
const THUMB: f32 = 28.0;
const GAP: f32 = 2.0;
/// The glyph inside a thumbnail.
const GLYPH_W: f64 = 22.0;
const GLYPH_H: f64 = 19.0;
/// A heading: `pt-0.5` (the groups only), an 11 px line, `pb-1`; a section's `mb-1.5`.
const HEADING_H: f32 = 20.0;
const HEADING_TOP: f32 = 2.0;
const SECTION_GAP: f32 = 6.0;
/// The recents kept (`MAX`).
pub const MAX_RECENT: usize = 12;
const WHEEL_STEP: f32 = 48.0;

/// `SHAPE_CATALOG` of `kubuno-office-shapes-core` (`shapes/catalog.ts`), in the web's order.
use kubuno_office_shapes_core::catalog::SHAPE_CATALOG as CATALOG;

/// A section of the gallery: its heading and kinds.
#[derive(Debug, Clone, PartialEq)]
struct Group {
    recent: bool,
    title: String,
    kinds: Vec<String>,
}

fn res_or(key: &str, default: &str) -> String {
    let s = kubuno_desktop::resources::string(key);
    if s.is_empty() {
        default.to_string()
    } else {
        s
    }
}

/// `SHAPE_GALLERY_GROUPS`: the catalogue minus the text box.
fn catalogue_groups() -> Vec<Group> {
    CATALOG
        .iter()
        .map(|c| Group {
            recent: false,
            title: res_or(&format!("sheet_shape_grp_{}", c.id), c.title),
            kinds: c.shapes.iter().filter(|s| s.kind != "textBox").map(|s| s.kind.to_string()).collect(),
        })
        .filter(|g| !g.kinds.is_empty())
        .collect()
}

fn is_known(kind: &str) -> bool {
    CATALOG.iter().flat_map(|c| c.shapes.iter()).any(|s| s.kind == kind && s.kind != "textBox")
}

/// `shapeLabel`: the catalogue's French label, or the presentations' wording for the few shared names.
pub fn shape_label(kind: &str) -> String {
    let shared = match kind {
        "rect" => Some("pres_shape_rect"),
        "roundRect" => Some("pres_shape_round"),
        "ellipse" => Some("pres_shape_ellipse"),
        "triangle" => Some("pres_shape_triangle"),
        "diamond" => Some("pres_shape_diamond"),
        "arrow" => Some("pres_shape_arrow"),
        "star" => Some("pres_shape_star"),
        _ => None,
    };
    let label = CATALOG.iter().flat_map(|c| c.shapes.iter()).find(|s| s.kind == kind).map_or(kind, |s| s.label);
    match shared {
        Some(key) => res_or(key, label),
        None => label.to_string(),
    }
}

// ── The recents (`shapes/recent.ts`) ────────────────────────────────────────

/// `pushRecentShape`: `kind` first, deduplicated, at most [`MAX_RECENT`].
pub fn push_recent(list: &[String], kind: &str) -> Vec<String> {
    let mut v: Vec<String> = std::iter::once(kind.to_string()).chain(list.iter().filter(|k| *k != kind).cloned()).collect();
    v.truncate(MAX_RECENT);
    v
}

/// The file holding the recents, one kind a line (the web's `localStorage` key `kubuno.office.recentShapes`, per
/// device): `%LOCALAPPDATA%\Kubuno\office\recent-shapes.txt`.
fn recents_path() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share")))?;
    Some(base.join("Kubuno").join("office").join("recent-shapes.txt"))
}

fn parse_recents(text: &str) -> Vec<String> {
    text.lines().map(str::trim).filter(|k| is_known(k)).map(str::to_string).take(MAX_RECENT).collect()
}

/// `loadRecentShapes`, filtered to the kinds the gallery draws.
pub fn load_recents() -> Vec<String> {
    recents_path().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| parse_recents(&t)).unwrap_or_default()
}

fn save_recents(list: &[String]) {
    let Some(path) = recents_path() else { return };
    if let Some(dir) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            kubuno_desktop::tracing::warn!("[diagrams] recent shapes: {e}");
            return;
        }
    }
    // The gallery works without recents (the web's private mode): a failure is only logged.
    if let Err(e) = std::fs::write(&path, list.join("\n")) {
        kubuno_desktop::tracing::warn!("[diagrams] recent shapes: {e}");
    }
}

// ── Layout ──────────────────────────────────────────────────────────────────

/// The gallery's layout: each group's heading (group, rect), each cell (group, kind, rect), the content height.
type GalleryLayout = (Vec<(usize, Rect)>, Vec<(usize, usize, Rect)>, f32);

/// The headings and thumbnails of the groups in a `w` DIP wide panel (y from the content's top, `p-2`
/// included), and the content's height.
fn gallery_layout(w: f32, groups: &[(bool, usize)]) -> GalleryLayout {
    let per_row = (((w - 2.0 * PAD + GAP) / (THUMB + GAP)).floor() as usize).max(1);
    let mut headings = Vec::new();
    let mut thumbs = Vec::new();
    let mut y = PAD;
    for (gi, (recent, count)) in groups.iter().enumerate() {
        if gi > 0 {
            y += SECTION_GAP;
        }
        let top = if *recent { 0.0 } else { HEADING_TOP };
        headings.push((gi, Rect::new(PAD + 4.0, y + top, w - PAD - 4.0, y + top + HEADING_H - 4.0)));
        y += top + HEADING_H;
        for i in 0..*count {
            let (row, col) = ((i / per_row) as f32, (i % per_row) as f32);
            let left = PAD + col * (THUMB + GAP);
            let t = y + row * (THUMB + GAP);
            thumbs.push((gi, i, Rect::new(left, t, left + THUMB, t + THUMB)));
        }
        let rows = count.div_ceil(per_row) as f32;
        if rows > 0.0 {
            y += rows * THUMB + (rows - 1.0) * GAP;
        }
    }
    (headings, thumbs, y + PAD)
}

fn groups_with(recents: &[String]) -> Vec<Group> {
    let mut v = Vec::new();
    if !recents.is_empty() {
        v.push(Group { recent: true, title: res_or("sheet_shape_grp_recent", "Formes récemment utilisées"), kinds: recents.to_vec() });
    }
    v.extend(catalogue_groups());
    v
}

/// The height the gallery's content needs (before the popup caps it).
fn content_height(groups: &[Group]) -> f32 {
    let shape: Vec<(bool, usize)> = groups.iter().map(|g| (g.recent, g.kinds.len())).collect();
    gallery_layout(WIDTH, &shape).2
}

/// Paints a gallery glyph (`ShapeGlyph`, grey outline on white) with its top-left at `(x, y)`: the shared
/// geometry, else the stencil thumbnail, else (the lines, which have no geometry) a stroke across the box.
fn paint_glyph(ctx: &mut Ctx2D<'_>, kind: &str, x: f64, y: f64) {
    ctx.save();
    ctx.set_fill_style("#ffffff");
    ctx.set_stroke_style("#5f6470");
    ctx.set_line_width(1.0);
    if !stencils::paint_shape_view(ctx, kind, x, y, GLYPH_W, GLYPH_H, None, true, true, 1.0, None) {
        if let Some(st) = stencils::stencil(kind) {
            ctx.translate(x, y);
            render::render_stencil_thumbnail(ctx, st, GLYPH_W, GLYPH_H);
        } else {
            let (x0, y0, x1, y1) = (x + 2.0, y + GLYPH_H - 2.0, x + GLYPH_W - 2.0, y + 2.0);
            ctx.begin_path();
            ctx.move_to(x0, y0);
            if kind.starts_with("elbow") {
                ctx.line_to((x0 + x1) / 2.0, y0);
                ctx.line_to((x0 + x1) / 2.0, y1);
                ctx.line_to(x1, y1);
            } else if kind.starts_with("curve") {
                ctx.bezier_curve_to(x0 + 10.0, y0, x1 - 10.0, y1, x1, y1);
            } else {
                ctx.line_to(x1, y1);
            }
            ctx.stroke();
            let head = |ctx: &mut Ctx2D<'_>, tx: f64, ty: f64, dx: f64, dy: f64| {
                ctx.begin_path();
                ctx.move_to(tx, ty);
                ctx.line_to(tx - 4.0 * dx + 2.5 * dy, ty - 4.0 * dy - 2.5 * dx);
                ctx.line_to(tx - 4.0 * dx - 2.5 * dy, ty - 4.0 * dy + 2.5 * dx);
                ctx.close_path();
                ctx.set_fill_style("#5f6470");
                ctx.fill();
            };
            let (dx, dy) = if kind.starts_with("line") { (std::f64::consts::FRAC_1_SQRT_2, -std::f64::consts::FRAC_1_SQRT_2) } else { (1.0, 0.0) };
            if kind.ends_with("Arrow") || kind.ends_with("DoubleArrow") || kind == "lineDouble" {
                head(ctx, x1, y1, dx, dy);
            }
            if kind.ends_with("DoubleArrow") || kind == "lineDouble" {
                head(ctx, x0, y0, -dx, -dy);
            }
        }
    }
    ctx.restore();
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Hit {
    Thumb(usize, usize),
    Bar(ScrollPart),
}

/// The gallery's panel (see the module doc): the content of the popup [`ShapeGallery::open`] shows.
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Diagrams")]
#[toolbox(icon = "shapes")]
#[default_event("ShapePicked")]
pub struct ShapeGallery {
    base: ControlCore,
    /// The armed kind, highlighted.
    #[property(bindable)]
    #[category("Data")]
    pub current: String,
    #[event]
    #[category("Action")]
    pub shape_picked: Event<ShapePickedEventArgs>,
    /// Read when the panel is made (the web's snapshot at mount: a pick closes it).
    groups: Vec<Group>,
    recents: Vec<String>,
    scroll: f32,
    hot: Option<Hit>,
    pressed: Option<Hit>,
    thumb_grab: Option<f32>,
    cache: SurfaceCache,
}

impl Default for ShapeGallery {
    fn default() -> Self {
        let recents = load_recents();
        Self {
            base: ControlCore::default(),
            current: String::new(),
            shape_picked: Event::default(),
            groups: groups_with(&recents),
            recents,
            scroll: 0.0,
            hot: None,
            pressed: None,
            thumb_grab: None,
            cache: SurfaceCache::default(),
        }
    }
}

impl ShapeGallery {
    /// Opens the gallery in a popup under the pointer (the ribbon button that was just clicked), `armed`
    /// highlighted; `on_pick` gets the kind picked, then the popup closes. Nothing is reported when it is
    /// dismissed.
    pub fn open(armed: &str, on_pick: impl Fn(String) + 'static) {
        let owner = kubuno_desktop::popup::current_window();
        let geometry = kubuno_desktop::controls::host::screen_geometry(owner);
        let work_h = geometry.as_ref().map_or(800.0, |g| g.work_area.3 - g.work_area.1);
        let height = content_height(&groups_with(&load_recents())).min((work_h * 0.64).max(120.0));
        let gallery = Custom::<ShapeGallery>::new().dock(DockStyle::Fill).name("gallery");
        let back = {
            let c = kubuno_desktop::Application::theme().card_background;
            let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            format!("#{:02X}{:02X}{:02X}", byte(c.r), byte(c.g), byte(c.b))
        };
        let popup = Popup::new(&gallery, WIDTH, height).corner_radius(12.0).back_color(&back).title(&res_or("sheet_shapes", "Formes")).placement(Placement::BottomStart).gap(4.0);
        let form = popup.form().clone();
        {
            let (gallery, armed) = (gallery.clone(), armed.to_string());
            form.load().subscribe(move |_form, _e| {
                let armed = armed.clone();
                gallery.with(|g| {
                    g.current = armed;
                    g.invalidate();
                });
            });
        }
        {
            let (form, on_pick) = (form.clone(), Rc::new(on_pick));
            gallery.on::<ShapePickedEventArgs>("OnShapePicked").subscribe(move |_gallery, e| {
                on_pick(e.kind.clone());
                form.close();
            });
        }
        // The anchor: the pointer, on the button whose command opened the gallery.
        let anchor = match (geometry, cursor_on_screen()) {
            (Some(g), Some((x, y))) => {
                let (cx, cy) = (x / g.scale - g.client_origin.0, y / g.scale - g.client_origin.1);
                Rect::new(cx - 8.0, cy - 8.0, cx + 8.0, cy + 8.0)
            }
            _ => Rect::new(0.0, 0.0, 0.0, 0.0),
        };
        popup.show(owner, anchor);
    }

    fn layout(&self) -> GalleryLayout {
        let shape: Vec<(bool, usize)> = self.groups.iter().map(|g| (g.recent, g.kinds.len())).collect();
        gallery_layout(self.size().width, &shape)
    }

    fn view(&self) -> Rect {
        Rect::new(0.0, 0.0, self.size().width, self.size().height)
    }

    fn bar(&self, extent: f32) -> Option<ScrollBar> {
        ScrollBar::from_content(false, extent, self.size().height, self.scroll).map(|b| b.with_expanded(matches!(self.hot, Some(Hit::Bar(_))) || self.thumb_grab.is_some()))
    }

    fn max_scroll(&self, extent: f32) -> f32 {
        (extent - self.size().height).max(0.0)
    }

    fn scroll_to(&mut self, y: f32) {
        let (_, _, extent) = self.layout();
        let y = y.clamp(0.0, self.max_scroll(extent));
        if (y - self.scroll).abs() > 0.01 {
            self.scroll = y;
            self.invalidate();
        }
    }

    fn hit(&self, x: f32, y: f32) -> Option<Hit> {
        let (_, thumbs, extent) = self.layout();
        if let Some(bar) = self.bar(extent) {
            if let Some(part) = bar.part_at(bar.rail(&self.view()), x, y) {
                return Some(Hit::Bar(part));
            }
        }
        let cy = y + self.scroll;
        thumbs.into_iter().find(|(_, _, r)| r.contains(x, cy)).map(|(g, i, _)| Hit::Thumb(g, i))
    }

    fn kind_at(&self, g: usize, i: usize) -> Option<&str> {
        self.groups.get(g)?.kinds.get(i).map(String::as_str)
    }

    fn pick(&mut self, kind: String) {
        self.recents = push_recent(&self.recents, &kind);
        save_recents(&self.recents);
        self.raise_shape_picked(ShapePickedEventArgs { kind });
    }
}

/// The pointer on screen, in physical pixels.
fn cursor_on_screen() -> Option<(f32, f32)> {
    let mut p = windows::Win32::Foundation::POINT::default();
    // SAFETY: a plain query into a local.
    unsafe { windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut p) }.ok()?;
    Some((p.x as f32, p.y as f32))
}

impl Control for ShapeGallery {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, _proposed: Size) -> Size {
        Size { width: WIDTH, height: content_height(&self.groups).min(480.0) }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        let b = e.clip_rectangle;
        let c = e.canvas();
        let cv: &dyn Canvas = c;
        let t = cv.theme();
        cv.fill_rounded(&b, 0.0, &t.card_background);
        let (headings, thumbs, extent) = self.layout();
        let dy = b.top - self.scroll;
        let shift = |r: &Rect| Rect::new(b.left + r.left, r.top + dy, b.left + r.right, r.bottom + dy);
        let visible = |r: &Rect| r.bottom >= b.top && r.top <= b.bottom;
        c.push_clip(&b);
        for (g, r) in &headings {
            let r = shift(r);
            if visible(&r) {
                let title = self.groups.get(*g).map_or("", |g| g.title.as_str());
                cv.text_ellipsis(title, &r, &cv.formats().micro, &t.text_secondary);
            }
        }
        let mut glyphs = Vec::new();
        for (g, i, r) in &thumbs {
            let r = shift(r);
            if !visible(&r) {
                continue;
            }
            let Some(kind) = self.kind_at(*g, *i) else { continue };
            if kind == self.current {
                cv.fill_rounded(&r, 4.0, &t.accent_light);
            } else if self.pressed == Some(Hit::Thumb(*g, *i)) {
                cv.fill_rounded(&r, 4.0, &t.surface_3);
            } else if self.hot == Some(Hit::Thumb(*g, *i)) {
                cv.fill_rounded(&r, 4.0, &t.surface_2);
            }
            glyphs.push((kind.to_string(), r.left + (THUMB - GLYPH_W as f32) / 2.0, r.top + (THUMB - GLYPH_H as f32) / 2.0));
        }
        if let Some(renderer) = c.renderer() {
            if let Ok(mut surface) = D2DSurface::new(&renderer.d2d_context, &renderer.d2d_factory, &renderer.dwrite, &self.cache) {
                let mut ctx = Ctx2D::new(&mut surface);
                ctx.set_transform(Matrix::translation(0.0, 0.0));
                for (kind, x, y) in &glyphs {
                    paint_glyph(&mut ctx, kind, *x as f64, *y as f64);
                }
            }
        }
        if let Some(bar) = self.bar(extent) {
            let rail = bar.rail(&b);
            bar.paint(cv, rail, WidgetState::REST.hot(matches!(self.hot, Some(Hit::Bar(_)))).pressed(self.thumb_grab.is_some()));
        }
        c.pop_clip();
        e.raise(self, "OnPaint");
    }

    fn tool_tip_at(&self, x: f32, y: f32) -> Option<String> {
        match self.hit(x, y)? {
            Hit::Thumb(g, i) => self.kind_at(g, i).map(shape_label),
            Hit::Bar(_) => None,
        }
    }

    fn accessible_parts(&self) -> Vec<AccessiblePart> {
        use kubuno_desktop::controls::host::access::AccessRole;
        let (_, thumbs, _) = self.layout();
        let h = self.size().height;
        thumbs
            .into_iter()
            .filter_map(|(g, i, r)| {
                let r = Rect::new(r.left, r.top - self.scroll, r.right, r.bottom - self.scroll);
                if r.bottom < 0.0 || r.top > h {
                    return None;
                }
                let group = self.groups.get(g)?;
                let kind = group.kinds.get(i)?;
                Some(AccessiblePart { name: format!("{} ({})", shape_label(kind), group.title), role: AccessRole::Button, bounds: r })
            })
            .collect()
    }

    fn on_mouse_wheel(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let notches = e.args().delta;
        self.scroll_to(self.scroll - notches * WHEEL_STEP);
        e.raise(&*self, "OnMouseWheel");
    }

    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        if e.args().button == MouseButton::Left {
            let (x, y) = (e.args().x, e.args().y);
            let h = self.hit(x, y);
            if let Some(Hit::Bar(part)) = h {
                let (_, _, extent) = self.layout();
                let page = self.size().height;
                match part {
                    ScrollPart::Thumb => {
                        if let Some(bar) = self.bar(extent) {
                            self.thumb_grab = Some(y - bar.thumb_rect(bar.rail(&self.view())).top);
                        }
                    }
                    ScrollPart::PageLow => self.scroll_to(self.scroll - page),
                    ScrollPart::PageHigh => self.scroll_to(self.scroll + page),
                    ScrollPart::ArrowLow => self.scroll_to(self.scroll - WHEEL_STEP),
                    ScrollPart::ArrowHigh => self.scroll_to(self.scroll + WHEEL_STEP),
                }
            }
            self.pressed = h;
            self.invalidate();
        }
        e.raise(&*self, "OnMouseDown");
    }

    fn on_mouse_move(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y) = (e.args().x, e.args().y);
        if let Some(grab) = self.thumb_grab {
            if e.args().button == MouseButton::Left {
                let (_, _, extent) = self.layout();
                if let Some(bar) = self.bar(extent) {
                    let rail = bar.rail(&self.view());
                    let thumb = bar.thumb_rect(rail);
                    let travel = (rail.bottom - rail.top) - (thumb.bottom - thumb.top);
                    if travel > 0.0 {
                        self.scroll_to(((y - grab - rail.top).clamp(0.0, travel)) / travel * self.max_scroll(extent));
                    }
                }
            } else {
                self.thumb_grab = None;
            }
        }
        let h = self.hit(x, y);
        if h != self.hot {
            self.hot = h;
            self.invalidate();
        }
        e.raise(&*self, "OnMouseMove");
    }

    fn on_mouse_up(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let h = self.hit(e.args().x, e.args().y);
        self.thumb_grab = None;
        if let Some(Hit::Thumb(g, i)) = self.pressed.take() {
            if h == Some(Hit::Thumb(g, i)) {
                if let Some(kind) = self.kind_at(g, i).map(str::to_string) {
                    self.pick(kind);
                }
            }
        }
        self.invalidate();
        e.raise(&*self, "OnMouseUp");
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
    fn recents_put_the_last_pick_first_without_duplicates_and_keep_twelve() {
        let list: Vec<String> = (0..12).map(|i| format!("k{i}")).collect();
        let v = push_recent(&list, "k5");
        assert_eq!(v[0], "k5");
        assert_eq!(v.len(), 12);
        assert_eq!(v.iter().filter(|k| *k == "k5").count(), 1);
        let v = push_recent(&list, "new");
        assert_eq!(v.len(), MAX_RECENT);
        assert_eq!(v[0], "new");
        assert!(!v.contains(&"k11".to_string()));
    }

    #[test]
    fn stored_recents_keep_only_known_kinds() {
        assert_eq!(parse_recents("rect\n textBox \nnope\nflowDecision\n"), vec!["rect".to_string(), "flowDecision".to_string()]);
    }

    #[test]
    fn the_gallery_lays_eight_thumbnails_a_row() {
        let (headings, thumbs, h) = gallery_layout(WIDTH, &[(true, 3), (false, 10)]);
        assert_eq!(headings.len(), 2);
        assert_eq!(thumbs[0].2.top, PAD + HEADING_H);
        let second: Vec<Rect> = thumbs.iter().filter(|(g, _, _)| *g == 1).map(|(_, _, r)| *r).collect();
        assert_eq!(second[8].left, PAD);
        assert_eq!(second[7].left, PAD + 7.0 * (THUMB + GAP));
        assert_eq!(second[8].top, second[0].top + THUMB + GAP);
        assert_eq!(h, second[9].bottom + PAD);
    }

    #[test]
    fn the_catalogue_drops_the_text_box_and_labels_its_kinds() {
        let groups = catalogue_groups();
        assert_eq!(groups.len(), CATALOG.len());
        assert!(groups.iter().all(|g| !g.kinds.iter().any(|k| k == "textBox")));
        assert_eq!(shape_label("flowDecision"), "Décision");
        assert_eq!(shape_label("unknown-kind"), "unknown-kind");
    }

    #[test]
    fn every_kind_but_the_lines_arms_a_stencil() {
        for c in CATALOG {
            for s in c.shapes {
                let kind = s.kind;
                if c.id != "lines" && kind != "textBox" {
                    assert!(stencils::stencil(kind).is_some(), "{kind} is no stencil");
                }
            }
        }
    }
}
