//! `StencilPalette` — the Formes panel (`DiagramEditorPage.tsx:3171-3271`): a search box (`p-2 border-b`, an input
//! `pl-7 py-1 text-xs bg-surface-2 rounded`, a 12 DIP magnifier), then either the flat results (`p-2`, « Aucune
//! forme » when none) or the categories as an accordion (`border-b` sections, the first one open, a `px-2 py-2
//! text-xs font-medium text-secondary` header whose ChevronRight turns when open); a 5-column grid (`gap-1`) of
//! square cells holding 38×30 thumbnails drawn by the shared engine (the armed one `border-primary
//! bg-primary/5`, hover `bg-surface-2` with a border); a large preview of the hovered stencil (180×120 and its
//! name in a 220 DIP card); and, while a stencil is armed, the hint `bg-primary/5 text-primary` at the bottom.
//! A click arms the stencil (`StencilClicked`); a drag carries `kubuno-stencil:<id>` to the canvas.

use std::collections::HashSet;

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::range::{ScrollBar, ScrollPart};
use kubuno_desktop::ui::{Canvas, Rect, Size, Widget, WidgetState};
use kubuno_desktop::views::component::{AccessiblePart, Control, ControlCore, EventCx, Keys, PaintEventCx};
use kubuno_desktop::views::events::{DataObject, DragDropEffects, Event};
use kubuno_office_diagrams_core::canvas::{Ctx2D, Matrix};
use kubuno_office_diagrams_core::render;
use kubuno_office_diagrams_core::stencils::{self, StencilDef};
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;

use crate::platform::surface::{D2DSurface, SurfaceCache};
use crate::views::diagram_window::stencil_label;

/// A stencil of the palette was clicked.
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct StencilClickedEventArgs {
    pub id: String,
}

/// The search strip: `p-2` around a `py-1 text-xs` input, and its `border-b`.
const SEARCH_H: f32 = 41.0;
/// A category header: `py-2` around a `text-xs` line.
const HEADER_H: f32 = 32.0;
/// `grid-cols-5 gap-1`, inside `px-2` (and `pb-2`).
const COLS: usize = 5;
const CELL_GAP: f32 = 4.0;
const GRID_PAD: f32 = 8.0;
/// The web's thumbnail (`StencilThumbnail w={38} h={30}`).
const THUMB_W: f32 = 38.0;
const THUMB_H: f32 = 30.0;
/// « Aucune forme »: `py-4` around an 11 px line.
const EMPTY_H: f32 = 48.0;
/// The armed hint: `py-1.5` around up to two `text-xs` lines, and its `border-t`.
const HINT_H: f32 = 45.0;
/// The hover card: 220 wide, `p-4` around 180×120, then the name (`border-t py-2 text-sm`).
const PREVIEW_W: f32 = 220.0;
const PREVIEW_H: f32 = 16.0 + 120.0 + 16.0 + 37.0;
/// How far a press must travel before it becomes a drag.
const DRAG_SLOP: f32 = 4.0;
/// One wheel notch.
const WHEEL_STEP: f32 = 48.0;

/// What the layout of the scrolled content is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Item {
    /// A category's header (its section index).
    Header(usize),
    /// A stencil (section, index in the section).
    Cell(usize, usize),
    /// « Aucune forme ».
    Empty,
}

/// A section of the content, as the layout sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Shape {
    /// A category (with a header and a `border-b`); `false` for the search results.
    header: bool,
    count: usize,
    open: bool,
}

/// The side of a square cell for a `w` DIP wide panel.
fn cell_size(w: f32) -> f32 {
    ((w - 2.0 * GRID_PAD - (COLS as f32 - 1.0) * CELL_GAP) / COLS as f32).max(8.0)
}

/// The items of the scrolled content in a `w` DIP wide panel (y from the content's top), and its height.
fn content_layout(w: f32, sections: &[Shape]) -> (Vec<(Item, Rect)>, f32) {
    let cell = cell_size(w);
    let mut items = Vec::new();
    let mut y = 0.0;
    for (si, s) in sections.iter().enumerate() {
        if s.header {
            items.push((Item::Header(si), Rect::new(0.0, y, w, y + HEADER_H)));
            y += HEADER_H;
        } else {
            y += GRID_PAD;
        }
        if !s.header || s.open {
            for i in 0..s.count {
                let (row, col) = ((i / COLS) as f32, (i % COLS) as f32);
                let left = GRID_PAD + col * (cell + CELL_GAP);
                let top = y + row * (cell + CELL_GAP);
                items.push((Item::Cell(si, i), Rect::new(left, top, left + cell, top + cell)));
            }
            let rows = s.count.div_ceil(COLS) as f32;
            if rows > 0.0 {
                y += rows * cell + (rows - 1.0) * CELL_GAP + GRID_PAD;
            } else if !s.header {
                items.push((Item::Empty, Rect::new(0.0, y, w, y + EMPTY_H)));
                y += EMPTY_H;
            }
        }
        if s.header {
            y += 1.0;
        }
    }
    (items, y)
}

/// Where the hover card goes for a cell (in the control's coordinates): under the cell's row when it fits in
/// `room`, else above it, kept inside `room`.
fn preview_rect(cell: Rect, room: Rect) -> Rect {
    let w = PREVIEW_W.min(room.right - room.left - 16.0).max(60.0);
    let left = room.left + ((room.right - room.left) - w) / 2.0;
    let top = if cell.bottom + 4.0 + PREVIEW_H <= room.bottom { cell.bottom + 4.0 } else { (cell.top - 4.0 - PREVIEW_H).max(room.top) };
    Rect::new(left, top, left + w, top + PREVIEW_H)
}

/// The scroll offset a thumb drag gives (Documents' `scroll_from_thumb_along`, vertical).
fn scroll_from_thumb(bar: &ScrollBar, rail: &Rect, at: f32, grab: f32, max_scroll: f32) -> f32 {
    let thumb = bar.thumb_rect(*rail);
    let travel = (rail.bottom - rail.top) - (thumb.bottom - thumb.top);
    if travel <= 0.0 {
        return 0.0;
    }
    ((at - grab - rail.top).clamp(0.0, travel)) / travel * max_scroll
}

/// One section of the palette: a category or the search results.
struct Section {
    /// The category id (`None` for the search results).
    cat: Option<String>,
    label: String,
    stencils: Vec<&'static StencilDef>,
}

fn category_text(cat: &str) -> String {
    let s = kubuno_desktop::resources::string(&format!("stencil_cat_{cat}"));
    if s.is_empty() {
        stencils::category_label(cat)
    } else {
        s
    }
}

fn build_sections(query: &str) -> Vec<Section> {
    let q = query.trim();
    if q.is_empty() {
        stencils::get_categories().into_iter().map(|cat| Section { label: category_text(&cat), stencils: stencils::get_stencils_by_category(&cat), cat: Some(cat) }).collect()
    } else {
        let name = |s: &StencilDef| stencil_label(s);
        vec![Section { cat: None, label: String::new(), stencils: stencils::search_stencils(q, Some(&name)) }]
    }
}

fn with_alpha(c: D2D1_COLOR_F, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { a: c.a * a, ..c }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Hit {
    Search,
    Item(Item),
    Bar(ScrollPart),
}

/// The stencil palette (see the module doc).
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Diagrams")]
#[toolbox(icon = "shapes")]
#[default_event("StencilClicked")]
pub struct StencilPalette {
    base: ControlCore,
    /// The armed stencil (highlighted), empty for none.
    #[property(bindable)]
    #[category("Data")]
    pub armed: String,
    #[event]
    #[category("Action")]
    pub stencil_clicked: Event<StencilClickedEventArgs>,
    query: String,
    sections: Vec<Section>,
    /// The open categories (`expandedCats`).
    open: HashSet<String>,
    scroll: f32,
    /// The search box has the keyboard.
    editing: bool,
    hot: Option<Hit>,
    /// What the left button went down on, and where.
    pressed: Option<(Hit, f32, f32)>,
    /// The thumb's grab offset while it is dragged.
    thumb_grab: Option<f32>,
    cache: SurfaceCache,
}

impl Default for StencilPalette {
    fn default() -> Self {
        let sections = build_sections("");
        let open = sections.first().and_then(|s| s.cat.clone()).into_iter().collect();
        Self {
            base: ControlCore::default(),
            armed: String::new(),
            stencil_clicked: Event::default(),
            query: String::new(),
            sections,
            open,
            scroll: 0.0,
            editing: false,
            hot: None,
            pressed: None,
            thumb_grab: None,
            cache: SurfaceCache::default(),
        }
    }
}

impl StencilPalette {
    fn shapes(&self) -> Vec<Shape> {
        self.sections
            .iter()
            .map(|s| Shape { header: s.cat.is_some(), count: s.stencils.len(), open: s.cat.as_ref().is_some_and(|c| self.open.contains(c)) })
            .collect()
    }

    fn hint_h(&self) -> f32 {
        if self.armed.is_empty() {
            0.0
        } else {
            HINT_H
        }
    }

    /// The scrolled area, in the control's coordinates.
    fn viewport(&self) -> Rect {
        let s = self.size();
        Rect::new(0.0, SEARCH_H, s.width, (s.height - self.hint_h()).max(SEARCH_H))
    }

    fn search_box() -> (f32, f32, f32) {
        (8.0, 8.0, 32.0)
    }

    fn layout(&self) -> (Vec<(Item, Rect)>, f32) {
        content_layout(self.size().width, &self.shapes())
    }

    fn max_scroll(&self, extent: f32) -> f32 {
        let v = self.viewport();
        (extent - (v.bottom - v.top)).max(0.0)
    }

    fn bar(&self, extent: f32) -> Option<ScrollBar> {
        let v = self.viewport();
        ScrollBar::from_content(false, extent, v.bottom - v.top, self.scroll).map(|b| b.with_expanded(matches!(self.hot, Some(Hit::Bar(_))) || self.thumb_grab.is_some()))
    }

    fn scroll_to(&mut self, y: f32) {
        let (_, extent) = self.layout();
        let y = y.clamp(0.0, self.max_scroll(extent));
        if (y - self.scroll).abs() > 0.01 {
            self.scroll = y;
            self.invalidate();
        }
    }

    fn hit(&self, x: f32, y: f32) -> Option<Hit> {
        let w = self.size().width;
        if y < SEARCH_H {
            let (l, t, b) = Self::search_box();
            return (x >= l && x <= w - l && y >= t && y <= b).then_some(Hit::Search);
        }
        let v = self.viewport();
        if !v.contains(x, y) {
            return None;
        }
        let (items, extent) = self.layout();
        if let Some(bar) = self.bar(extent) {
            if let Some(part) = bar.part_at(bar.rail(&v), x, y) {
                return Some(Hit::Bar(part));
            }
        }
        let cy = y - v.top + self.scroll;
        items.into_iter().find(|(it, r)| *it != Item::Empty && r.contains(x, cy)).map(|(it, _)| Hit::Item(it))
    }

    fn stencil_at(&self, item: Item) -> Option<&'static StencilDef> {
        match item {
            Item::Cell(s, i) => self.sections.get(s)?.stencils.get(i).copied(),
            _ => None,
        }
    }

    fn set_query(&mut self, q: String) {
        if q != self.query {
            self.sections = build_sections(&q);
            self.query = q;
            self.scroll = 0.0;
            self.hot = None;
            self.invalidate();
        }
    }

    fn toggle(&mut self, section: usize) {
        if let Some(cat) = self.sections.get(section).and_then(|s| s.cat.clone()) {
            if !self.open.remove(&cat) {
                self.open.insert(cat);
            }
            let (_, extent) = self.layout();
            self.scroll = self.scroll.min(self.max_scroll(extent));
            self.invalidate();
        }
    }

    /// Paints `stencil` as the web's `StencilThumbnail` (`w`×`h` at `(x, y)`, window DIP).
    fn thumbnails(&self, c: &dyn kubuno_desktop::controls::ControlCanvas, thumbs: &[(&'static StencilDef, f32, f32, f32, f32)]) {
        if thumbs.is_empty() {
            return;
        }
        let Some(renderer) = c.renderer() else { return };
        let Ok(mut surface) = D2DSurface::new(&renderer.d2d_context, &renderer.d2d_factory, &renderer.dwrite, &self.cache) else { return };
        let mut ctx = Ctx2D::new(&mut surface);
        for (st, x, y, w, h) in thumbs {
            ctx.save();
            ctx.set_transform(Matrix::translation(*x as f64, *y as f64));
            render::render_stencil_thumbnail(&mut ctx, st, *w as f64, *h as f64);
            ctx.restore();
        }
    }
}

impl Control for StencilPalette {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, _proposed: Size) -> Size {
        Size { width: 250.0, height: 600.0 }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        let b = e.clip_rectangle;
        let c = e.canvas();
        let cv: &dyn Canvas = c;
        let t = cv.theme();
        let w = b.right - b.left;
        let at = |r: Rect| Rect::new(b.left + r.left, b.top + r.top, b.left + r.right, b.top + r.bottom);
        cv.fill_rounded(&b, 0.0, &t.card_background);

        // The search box.
        let (l, top, bottom) = Self::search_box();
        let input = at(Rect::new(l, top, w - l, bottom));
        cv.fill_rounded(&input, 4.0, if self.editing { &t.card_background } else { &t.surface_2 });
        if self.editing {
            cv.stroke_rounded(&input, 4.0, &t.accent);
        }
        let cy = (input.top + input.bottom) / 2.0;
        cv.vector_icon("Search", &Rect::new(input.left + 10.0 - 6.0, cy - 6.0, input.left + 10.0 + 6.0, cy + 6.0), 12.0, &t.text_tertiary);
        let text_r = Rect::new(input.left + 28.0, input.top, input.right - 8.0, input.bottom);
        let caption = &cv.formats().caption;
        if self.query.is_empty() {
            cv.text_ellipsis("Chercher une forme…", &text_r, caption, &t.text_tertiary);
        } else {
            cv.text_ellipsis(&self.query, &text_r, caption, &t.text_primary);
        }
        if self.editing {
            let x = (text_r.left + cv.measure(&self.query, caption) + 1.0).min(text_r.right);
            cv.fill_rounded(&Rect::new(x, cy - 7.0, x + 1.0, cy + 7.0), 0.0, &t.text_primary);
        }
        cv.fill_rounded(&at(Rect::new(0.0, SEARCH_H - 1.0, w, SEARCH_H)), 0.0, &t.divider);

        // The scrolled content.
        let v = self.viewport();
        let va = at(v);
        let (items, extent) = self.layout();
        let dy = va.top - self.scroll;
        let mut thumbs = Vec::new();
        let mut hovered: Option<(&'static StencilDef, Rect)> = None;
        c.push_clip(&va);
        for (item, r) in &items {
            if r.bottom + dy < va.top || r.top + dy > va.bottom {
                continue;
            }
            let r = Rect::new(b.left + r.left, r.top + dy, b.left + r.right, r.bottom + dy);
            let hot = self.hot == Some(Hit::Item(*item));
            match *item {
                Item::Header(si) => {
                    if hot {
                        cv.fill_rounded(&r, 0.0, &t.surface_2);
                    }
                    let open = self.sections.get(si).and_then(|s| s.cat.as_ref()).is_some_and(|cat| self.open.contains(cat));
                    let cy = (r.top + r.bottom) / 2.0;
                    let glyph = Rect::new(r.left + 8.0, cy - 7.0, r.left + 22.0, cy + 7.0);
                    cv.vector_icon(if open { "ChevronDown" } else { "ChevronRight" }, &glyph, 14.0, &t.text_secondary);
                    let label = self.sections.get(si).map_or("", |s| s.label.as_str());
                    cv.text_ellipsis(label, &Rect::new(r.left + 28.0, r.top, r.right - 8.0, r.bottom), &cv.formats().caption_strong, &t.text_secondary);
                    // The section's `border-b`, under its grid when open.
                    let section_bottom = items.iter().filter(|(it, _)| matches!(it, Item::Cell(s, _) if *s == si)).map(|(_, r)| r.bottom + GRID_PAD).fold(r.bottom - dy, f32::max);
                    let y = section_bottom + dy;
                    cv.fill_rounded(&Rect::new(r.left, y, r.right, y + 1.0), 0.0, &t.divider);
                }
                Item::Cell(..) => {
                    let Some(st) = self.stencil_at(*item) else { continue };
                    if st.id == self.armed {
                        cv.fill_rounded(&r, 4.0, &with_alpha(t.accent, 0.05));
                        cv.stroke_rounded(&r, 4.0, &t.accent);
                    } else if hot {
                        cv.fill_rounded(&r, 4.0, &t.surface_2);
                        cv.stroke_rounded(&r, 4.0, &t.card_stroke);
                    }
                    let (cx, cy) = ((r.left + r.right) / 2.0, (r.top + r.bottom) / 2.0);
                    thumbs.push((st, cx - THUMB_W / 2.0, cy - THUMB_H / 2.0, THUMB_W, THUMB_H));
                    if hot && self.pressed.is_none() {
                        hovered = Some((st, r));
                    }
                }
                Item::Empty => cv.text("Aucune forme", &r, &cv.formats().micro, &t.text_tertiary, true),
            }
        }
        self.thumbnails(c, &thumbs);
        if let Some(bar) = self.bar(extent) {
            let rail = bar.rail(&va);
            bar.paint(cv, rail, WidgetState::REST.hot(matches!(self.hot, Some(Hit::Bar(_)))).pressed(self.thumb_grab.is_some()));
        }
        c.pop_clip();

        // The armed hint.
        if !self.armed.is_empty() {
            let h = Rect::new(b.left, b.bottom - HINT_H, b.right, b.bottom);
            cv.fill_rounded(&h, 0.0, &t.card_background);
            cv.fill_rounded(&h, 0.0, &with_alpha(t.accent, 0.05));
            cv.fill_rounded(&Rect::new(h.left, h.top, h.right, h.top + 1.0), 0.0, &t.divider);
            let text = "Cliquez sur le canevas pour placer · ou glissez pour dessiner";
            cv.text(text, &Rect::new(h.left + 8.0, h.top + 1.0, h.right - 8.0, h.bottom), caption, &t.accent, true);
        }

        // The hover card (on the web, beside the panel).
        if let Some((st, cell)) = hovered {
            let p = preview_rect(cell, Rect::new(b.left, b.top, b.right, b.bottom));
            cv.draw_card_shadow(&p, 8.0);
            cv.fill_rounded(&p, 8.0, &t.card_background);
            cv.stroke_rounded(&p, 8.0, &t.card_stroke);
            let tw = (p.right - p.left - 32.0).min(180.0);
            self.thumbnails(c, &[(st, (p.left + p.right - tw) / 2.0, p.top + 16.0, tw, 120.0)]);
            let line = p.top + 152.0;
            cv.fill_rounded(&Rect::new(p.left, line, p.right, line + 1.0), 0.0, &t.divider);
            cv.text_ellipsis(&stencil_label(st), &Rect::new(p.left + 8.0, line + 1.0, p.right - 8.0, p.bottom), &cv.formats().body_small, &t.text_secondary);
        }
        e.raise(self, "OnPaint");
    }

    fn tool_tip_at(&self, x: f32, y: f32) -> Option<String> {
        match self.hit(x, y)? {
            Hit::Item(item) => self.stencil_at(item).map(stencil_label),
            _ => None,
        }
    }

    fn cursor_at(&self, x: f32, y: f32) -> Option<kubuno_desktop::controls::host::Cursor> {
        use kubuno_desktop::controls::host::Cursor as C;
        match self.hit(x, y)? {
            Hit::Search => Some(C::IBeam),
            Hit::Item(Item::Cell(..)) => Some(C::Hand),
            _ => None,
        }
    }

    fn accessible_parts(&self) -> Vec<AccessiblePart> {
        use kubuno_desktop::controls::host::access::AccessRole;
        let w = self.size().width;
        let (l, t, b) = Self::search_box();
        let mut parts = vec![AccessiblePart { name: "Chercher une forme".to_string(), role: AccessRole::TextInput, bounds: Rect::new(l, t, w - l, b) }];
        let v = self.viewport();
        let (items, _) = self.layout();
        for (item, r) in items {
            let r = Rect::new(r.left, r.top + v.top - self.scroll, r.right, r.bottom + v.top - self.scroll);
            if r.bottom < v.top || r.top > v.bottom {
                continue;
            }
            match item {
                Item::Header(si) => {
                    let name = self.sections.get(si).map(|s| s.label.clone()).unwrap_or_default();
                    parts.push(AccessiblePart { name, role: AccessRole::Button, bounds: r });
                }
                Item::Cell(..) => {
                    if let Some(st) = self.stencil_at(item) {
                        parts.push(AccessiblePart { name: stencil_label(st), role: AccessRole::ListItem, bounds: r });
                    }
                }
                Item::Empty => parts.push(AccessiblePart { name: "Aucune forme".to_string(), role: AccessRole::ListItem, bounds: r }),
            }
        }
        parts
    }

    fn is_input_key(&self, key: Keys) -> bool {
        use kubuno_desktop::controls::host::vk;
        self.editing && [vk::BACK, vk::ESCAPE, vk::DELETE, vk::ENTER, vk::SPACE].contains(&key.key.0)
    }

    fn on_key_down(&mut self, e: &mut EventCx<'_, KeyEventArgs>) {
        use kubuno_desktop::controls::host::vk;
        if self.editing {
            let key = e.args().key.0;
            if key == vk::BACK {
                let mut q = self.query.clone();
                q.pop();
                self.set_query(q);
                e.args_mut().handled = true;
            } else if key == vk::ESCAPE {
                self.set_query(String::new());
                self.editing = false;
                self.invalidate();
                e.args_mut().handled = true;
            }
        }
        e.raise(&*self, "OnKeyDown");
    }

    fn on_key_press(&mut self, e: &mut EventCx<'_, KeyPressEventArgs>) {
        let ch = e.args().key_char;
        if self.editing && !ch.is_control() {
            let mut q = self.query.clone();
            q.push(ch);
            self.set_query(q);
            e.args_mut().handled = true;
        }
        e.raise(&*self, "OnKeyPress");
    }

    fn on_lost_focus(&mut self, e: &mut EventCx<'_, kubuno_desktop::views::events::EmptyEventArgs>) {
        if self.editing {
            self.editing = false;
            self.invalidate();
        }
        e.raise(&*self, "OnLostFocus");
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
            let editing = h == Some(Hit::Search);
            if editing != self.editing {
                self.editing = editing;
                self.invalidate();
            }
            if editing {
                self.focus();
            }
            if let Some(Hit::Bar(part)) = h {
                let (_, extent) = self.layout();
                let page = self.viewport().bottom - self.viewport().top;
                match part {
                    ScrollPart::Thumb => {
                        if let Some(bar) = self.bar(extent) {
                            self.thumb_grab = Some(y - bar.thumb_rect(bar.rail(&self.viewport())).top);
                        }
                    }
                    ScrollPart::PageLow => self.scroll_to(self.scroll - page),
                    ScrollPart::PageHigh => self.scroll_to(self.scroll + page),
                    ScrollPart::ArrowLow => self.scroll_to(self.scroll - WHEEL_STEP),
                    ScrollPart::ArrowHigh => self.scroll_to(self.scroll + WHEEL_STEP),
                }
            }
            self.pressed = h.map(|h| (h, x, y));
            self.invalidate();
        }
        e.raise(&*self, "OnMouseDown");
    }

    fn on_mouse_move(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y) = (e.args().x, e.args().y);
        if let Some(grab) = self.thumb_grab {
            if e.args().button == MouseButton::Left {
                let (_, extent) = self.layout();
                if let Some(bar) = self.bar(extent) {
                    let s = scroll_from_thumb(&bar, &bar.rail(&self.viewport()), y, grab, self.max_scroll(extent));
                    self.scroll_to(s);
                }
            } else {
                self.thumb_grab = None;
            }
        } else if let Some((Hit::Item(item @ Item::Cell(..)), px, py)) = self.pressed {
            if e.args().button == MouseButton::Left && ((x - px).powi(2) + (y - py).powi(2)).sqrt() > DRAG_SLOP {
                self.pressed = None;
                if let Some(st) = self.stencil_at(item) {
                    drop(self.do_drag_drop(DataObject::from_text(format!("kubuno-stencil:{}", st.id)), DragDropEffects::COPY));
                }
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
        if let Some((Hit::Item(item), _, _)) = self.pressed.take() {
            if h == Some(Hit::Item(item)) {
                match item {
                    Item::Header(si) => self.toggle(si),
                    Item::Cell(..) => {
                        if let Some(st) = self.stencil_at(item) {
                            self.raise_stencil_clicked(StencilClickedEventArgs { id: st.id.to_string() });
                        }
                    }
                    Item::Empty => {}
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
    fn a_closed_category_is_a_header_and_its_border() {
        let (items, h) = content_layout(250.0, &[Shape { header: true, count: 12, open: false }, Shape { header: true, count: 3, open: false }]);
        assert_eq!(items.len(), 2);
        assert_eq!(items[1], (Item::Header(1), Rect::new(0.0, HEADER_H + 1.0, 250.0, 2.0 * HEADER_H + 1.0)));
        assert_eq!(h, 2.0 * (HEADER_H + 1.0));
    }

    #[test]
    fn an_open_category_lays_its_stencils_five_a_row() {
        let cell = cell_size(250.0);
        let (items, h) = content_layout(250.0, &[Shape { header: true, count: 7, open: true }]);
        let cells: Vec<Rect> = items.iter().filter(|(i, _)| matches!(i, Item::Cell(..))).map(|(_, r)| *r).collect();
        assert_eq!(cells.len(), 7);
        assert_eq!(cells[0].top, HEADER_H);
        assert_eq!(cells[0].left, GRID_PAD);
        assert_eq!(cells[5].top, HEADER_H + cell + CELL_GAP);
        assert!((cells[4].right - (250.0 - GRID_PAD)).abs() < 0.01, "the fifth column ends at the padding");
        assert!((h - (HEADER_H + 2.0 * cell + CELL_GAP + GRID_PAD + 1.0)).abs() < 0.01);
    }

    #[test]
    fn no_search_result_says_so() {
        let (items, h) = content_layout(250.0, &[Shape { header: false, count: 0, open: true }]);
        assert_eq!(items, vec![(Item::Empty, Rect::new(0.0, GRID_PAD, 250.0, GRID_PAD + EMPTY_H))]);
        assert_eq!(h, GRID_PAD + EMPTY_H);
    }

    #[test]
    fn the_preview_goes_under_the_cell_or_above_it() {
        let room = Rect::new(0.0, 0.0, 250.0, 600.0);
        assert_eq!(preview_rect(Rect::new(10.0, 50.0, 50.0, 90.0), room).top, 94.0);
        let high = preview_rect(Rect::new(10.0, 500.0, 50.0, 540.0), room);
        assert_eq!(high.bottom, 496.0);
        assert_eq!(high.right - high.left, PREVIEW_W.min(234.0));
    }

    #[test]
    fn the_catalogue_opens_on_its_first_category_and_search_flattens_it() {
        let p = StencilPalette::default();
        assert!(p.sections.len() > 1);
        assert_eq!(p.open.len(), 1);
        assert!(p.open.contains(p.sections[0].cat.as_deref().unwrap_or_default()));
        let found = build_sections("rect");
        assert_eq!(found.len(), 1);
        assert!(found[0].cat.is_none());
        assert!(found[0].stencils.iter().any(|s| s.id == "rect"));
        assert!(build_sections("zzzz-none")[0].stencils.is_empty());
    }
}
