//! The floating objects on the grid (`web/src/SpreadsheetApp.tsx` 1029-2580, 5135-5200, 5309-5376,
//! 7780-7860): painting pictures, charts and shapes over the cells, the selection chrome (frame, eight
//! handles, rotation knob; a line's two ends; a shape's yellow adjustment knobs), and the pointer gestures -
//! select (Shift/Ctrl add), move, resize, rotate, reshape, move a group, draw an armed shape. A drag previews
//! in this layer and is written to the workbook once, on release (the web saves on mouse-up too).
//!
//! Paint order is the web's: pictures on the grid, then charts, equations and shapes; the headers and the
//! object chrome above them. Hit-tests run the other way (`Editor::hit_object`).

use std::cell::RefCell;
use std::collections::HashMap;

use kubuno_desktop::ui::graphics::{Brush, Color, DashStyle, Graphics, Image, Pen, PointF};
use kubuno_desktop::ui::Rect;
use kubuno_office_sheets_core::chart::{render_chart, CellVal, ChartWords, Prim};
use kubuno_office_sheets_core::editor::objects::ObjClipboard;
use kubuno_office_sheets_core::editor::Editor;
use kubuno_office_sheets_core::grid::{COL_HEADER_HEIGHT, ROW_HEADER_WIDTH};
use kubuno_office_sheets_core::objects::{self as obj, Handle, ObjBox, ObjKind, ObjRect, ObjRef};
use kubuno_office_shapes_core::adjust as shape_adjust;
use serde_json::Value as Json;

use super::prims::{css_color, PrimTarget};
use super::shapes::{paint_caption, paint_glyph, ShapeBox};

/// A pointer gesture on objects.
#[derive(Clone, Debug)]
pub enum ObjDrag {
    /// One object moved, resized or rotated: its box and rotation at the start, the live ones.
    One { r: ObjRef, handle: Handle, box0: ObjBox, rot0: f64, start: (f64, f64), cur: ObjBox, rot: f64, moved: bool },
    /// A shape's adjustment knob.
    Adjust { r: ObjRef, index: usize, rect: ObjRect, adj: Vec<f64>, moved: bool },
    /// The whole selection moved.
    Group { snap: Vec<(ObjRef, ObjBox, f64)>, start: (f64, f64), dx: f64, dy: f64, moved: bool },
    /// An armed shape being drawn (content px).
    Draw { start: (f64, f64), cur: (f64, f64), square: bool, centre: bool },
}

/// The grid's object state.
#[derive(Default)]
pub struct ObjectLayer {
    /// The selected objects, oldest first (the last one is active).
    pub sel: Vec<ObjRef>,
    pub drag: Option<ObjDrag>,
    /// The shape kind armed by the gallery (the next drag on the grid draws it).
    pub armed: Option<String>,
    /// The object clipboard (process-wide on the web; per window here).
    pub clip: ObjClipboard,
    /// Chart drawings by chart id: (cache key, primitives).
    scenes: RefCell<HashMap<String, (u64, Vec<Prim>)>>,
    pub words: ChartWords,
    seed: u64,
}

impl ObjectLayer {
    /// A fresh seed for new object ids.
    pub fn next_seed(&mut self) -> u64 {
        self.seed = self.seed.wrapping_add(1);
        kubuno_desktop::controls::host::now_ms().wrapping_mul(2_654_435_761).wrapping_add(self.seed)
    }

    /// The live rectangle of an object (the drag preview over the stored one).
    pub fn rect_for(&self, ed: &Editor, r: &ObjRef) -> Option<ObjRect> {
        let base = ed.object_rect(r, None)?;
        let z = ed.zoom;
        match &self.drag {
            Some(ObjDrag::One { r: d, cur, rot, .. }) if d == r => Some(ObjRect { x: ROW_HEADER_WIDTH + cur.bx * z, y: COL_HEADER_HEIGHT + cur.by * z, w: cur.bw * z, h: cur.bh * z, rot: *rot }),
            Some(ObjDrag::Group { snap, dx, dy, .. }) => match snap.iter().find(|(x, _, _)| x == r) {
                Some((_, b, rot)) => Some(ObjRect { x: ROW_HEADER_WIDTH + (b.bx + dx) * z, y: COL_HEADER_HEIGHT + (b.by + dy) * z, w: b.bw * z, h: b.bh * z, rot: *rot }),
                None => Some(base),
            },
            _ => Some(base),
        }
    }

    /// The live adjustment values of a shape.
    fn adj_for(&self, r: &ObjRef) -> Option<&[f64]> {
        match &self.drag {
            Some(ObjDrag::Adjust { r: d, adj, .. }) if d == r => Some(adj),
            _ => None,
        }
    }

    /// Renders the charts' drawings for the current revision and sizes (outside the painter: chart data
    /// reads cell values through the evaluator).
    pub fn prepare(&self, ed: &Editor) {
        let rev = ed.revision();
        let mut live = Vec::new();
        for (i, c) in ed.objects(ObjKind::Chart).iter().enumerate() {
            let Some(id) = c.get("id").and_then(Json::as_str) else { continue };
            let r = ObjRef::new(ObjKind::Chart, id);
            let Some(rect) = self.rect_for(ed, &r) else { continue };
            let (w, h) = ((rect.w - 14.0).max(24.0), (rect.h - 14.0).max(24.0));
            let key = {
                use std::hash::{Hash, Hasher};
                let mut s = std::collections::hash_map::DefaultHasher::new();
                c.to_string().hash(&mut s);
                rev.hash(&mut s);
                i.hash(&mut s);
                ((w * 8.0) as i64).hash(&mut s);
                ((h * 8.0) as i64).hash(&mut s);
                ed.lang.hash(&mut s);
                s.finish()
            };
            live.push(id.to_string());
            if self.scenes.borrow().get(id).is_some_and(|(k, _)| *k == key) {
                continue;
            }
            let get = |rf: &str| cell_val(ed, rf);
            let prims = render_chart(c, w, h, &get, &self.words, &ed.lang);
            self.scenes.borrow_mut().insert(id.to_string(), (key, prims));
        }
        self.scenes.borrow_mut().retain(|k, _| live.contains(k));
    }

    pub fn has_selection(&self) -> bool {
        !self.sel.is_empty()
    }
}

/// `cellRefVal`: a bare A1 ref's number, else its value as text.
fn cell_val(ed: &Editor, rf: &str) -> CellVal {
    let Some(p) = kubuno_office_sheets_core::addr::Cell::parse(rf) else { return CellVal::Str(String::new()) };
    let shown = ed.shown(p.col, p.row);
    if let Some(n) = shown.num {
        return CellVal::Num(n);
    }
    if shown.raw_text.is_empty() {
        CellVal::Str(String::new())
    } else {
        CellVal::Str(shown.raw_text)
    }
}

/// Where objects are painted: the grid's surface, origin and scroll.
pub struct ObjPaint<'a> {
    pub g: &'a Graphics<'a>,
    pub ox: f32,
    pub oy: f32,
    pub scroll_x: f64,
    pub scroll_y: f64,
    pub vw: f64,
    pub vh: f64,
    pub dark: bool,
    pub family: &'a str,
}

impl ObjPaint<'_> {
    fn screen(&self, r: &ObjRect) -> (f32, f32, f32, f32) {
        (self.ox + (r.x - self.scroll_x) as f32, self.oy + (r.y - self.scroll_y) as f32, r.w as f32, r.h as f32)
    }

    fn data_clip(&self) -> Rect {
        Rect::new(self.ox + ROW_HEADER_WIDTH as f32, self.oy + COL_HEADER_HEIGHT as f32, self.ox + self.vw as f32, self.oy + self.vh as f32)
    }

    /// Paints every object (pictures, charts, equations, shapes) clipped to the data area.
    pub fn paint_objects(&self, ed: &Editor, layer: &ObjectLayer) {
        let g = self.g;
        let saved = g.save();
        g.intersect_clip(self.data_clip());
        for r in ed.objects_in_paint_order() {
            let (Some(o), Some(rect)) = (ed.object(&r), layer.rect_for(ed, &r)) else { continue };
            if rect.w <= 0.0 || rect.h <= 0.0 {
                continue;
            }
            let (x, y, w, h) = self.screen(&rect);
            let s = g.save();
            if rect.rot != 0.0 {
                g.rotate_transform_at(rect.rot as f32, PointF::new(x + w / 2.0, y + h / 2.0));
            }
            match r.kind {
                ObjKind::Image => self.paint_image(o, x, y, w, h, ed.zoom),
                ObjKind::Chart => self.paint_chart(o, layer, x, y, w, h),
                ObjKind::Shape => {
                    let b = ShapeBox { x, y, w, h, zoom: ed.zoom };
                    match layer.adj_for(&r) {
                        Some(adj) => {
                            let mut o2 = o.clone();
                            if let Some(m) = o2.as_object_mut() {
                                m.insert("adj".into(), Json::Array(adj.iter().map(|v| kubuno_office_sheets_core::model::cell::json_number(*v)).collect()));
                            }
                            paint_glyph(g, &o2, &b, self.dark);
                            paint_caption(g, &o2, &b, self.dark, self.family);
                        }
                        None => {
                            paint_glyph(g, o, &b, self.dark);
                            paint_caption(g, o, &b, self.dark, self.family);
                        }
                    }
                }
                // Equations are drawn by their own renderer when it is plugged in.
                ObjKind::Equation => {}
            }
            g.restore(s);
        }
        g.restore(saved);
    }

    /// A picture: drop shadow, the (cropped) bitmap, the outline (`imageFrame.ts`).
    fn paint_image(&self, o: &Json, x: f32, y: f32, w: f32, h: f32, zoom: f64) {
        let g = self.g;
        let Some(src) = o.get("src").and_then(Json::as_str) else { return };
        let Some(renderer) = g.renderer() else { return };
        let Some((bm, nw, nh)) = crate::platform::images::bitmap(&renderer.d2d_context, src) else { return };
        if o.get("shadow").and_then(Json::as_bool).unwrap_or(false) {
            // `drawImageShadow`: a soft halo offset (3, 4) × zoom, approximated by a few translucent layers.
            let (dx, dy) = (3.0 * zoom as f32, 4.0 * zoom as f32);
            for i in 0..4 {
                let spread = (i as f32 + 1.0) * 2.0 * zoom as f32;
                let r = Rect::new(x + dx - spread, y + dy - spread, x + w + dx + spread, y + h + dy + spread);
                g.fill_rounded_rectangle(Brush::solid(Color::rgba_f(0.0, 0.0, 0.0, 0.06)), r, spread);
            }
        }
        let f = |k: &str| o.get(k).and_then(Json::as_f64).unwrap_or(0.0);
        let (cl, ct, cr, cb) = (f("cropL"), f("cropT"), f("cropR"), f("cropB"));
        let image = Image::from_bitmap(bm);
        let dest = Rect::new(x, y, x + w, y + h);
        if cl != 0.0 || ct != 0.0 || cr != 0.0 || cb != 0.0 {
            let src_r = Rect::new(cl as f32 * nw, ct as f32 * nh, cl as f32 * nw + ((1.0 - cl - cr) as f32 * nw).max(1.0), ct as f32 * nh + ((1.0 - ct - cb) as f32 * nh).max(1.0));
            g.draw_image_with(&image, dest, Some(src_r), 1.0);
        } else {
            g.draw_image(&image, dest);
        }
        if let Some(c) = o.get("border").and_then(Json::as_str).and_then(|c| css_color(c, 1.0, false)) {
            let lw = (o.get("borderWidth").and_then(Json::as_f64).unwrap_or(1.0) * zoom).max(0.5) as f32;
            if lw >= w.min(h) {
                g.fill_rectangle(Brush::solid(c), dest);
            } else {
                let i = lw / 2.0;
                g.draw_rectangle(&Pen::new(c, lw), Rect::new(x + i, y + i, x + w - i, y + h - i));
            }
        }
    }

    /// A chart: its card (fill, border, radius 4, light shadow, 6 px padding) and the drawing.
    fn paint_chart(&self, o: &Json, layer: &ObjectLayer, x: f32, y: f32, w: f32, h: f32) {
        let g = self.g;
        let r = Rect::new(x, y, x + w, y + h);
        g.fill_rounded_rectangle(Brush::solid(Color::rgba_f(0.0, 0.0, 0.0, 0.06)), Rect::new(x - 1.0, y, x + w + 1.0, y + h + 2.0), 5.0);
        let fill = o.get("fill").and_then(Json::as_str).unwrap_or("white");
        if fill != "none" {
            let c = css_color(if fill == "white" { "#ffffff" } else { fill }, 1.0, false).unwrap_or(Color::WHITE);
            g.fill_rounded_rectangle(Brush::solid(c), r, 4.0);
        }
        let border = o.get("border").and_then(Json::as_str).unwrap_or("#e2e4e6");
        let bw = o.get("borderWidth").and_then(Json::as_f64).unwrap_or(1.0) as f32;
        if border != "none" {
            if let Some(c) = css_color(border, 1.0, false) {
                g.draw_rounded_rectangle(&Pen::new(c, bw), Rect::new(x + bw / 2.0, y + bw / 2.0, x + w - bw / 2.0, y + h - bw / 2.0), 4.0);
            }
        }
        let Some(id) = o.get("id").and_then(Json::as_str) else { return };
        let scenes = layer.scenes.borrow();
        let Some((_, prims)) = scenes.get(id) else { return };
        let inset = 6.0 + if border == "none" { 0.0 } else { bw };
        let s = g.save();
        g.intersect_clip(Rect::new(x + inset, y + inset, x + w - inset, y + h - inset));
        let family = o.get("font").and_then(Json::as_str).unwrap_or(self.family);
        PrimTarget { g, x: x + inset, y: y + inset, scale: 1.0, dark: self.dark, family }.draw(prims);
        g.restore(s);
    }

    /// The chrome of the selected objects (`drawObjectChrome`, `drawLineChrome`, the adjustment knobs), and
    /// the band of a shape being drawn.
    pub fn paint_chrome(&self, ed: &Editor, layer: &ObjectLayer) {
        let g = self.g;
        let saved = g.save();
        g.intersect_clip(self.data_clip());
        if let Some(ObjDrag::Draw { start, cur, square, centre }) = &layer.drag {
            let b = kubuno_office_shapes_core::draw::draw_box_from(start.0, start.1, cur.0, cur.1, *square, *centre);
            if b.w > 0.0 && b.h > 0.0 {
                let kind = layer.armed.as_deref().unwrap_or("rect");
                let (x, y) = (self.ox + (b.x - self.scroll_x) as f32, self.oy + (b.y - self.scroll_y) as f32);
                let ghost = serde_json::json!({ "kind": kind, "fill": "#dbe7ff", "border": obj::ACCENT, "borderWidth": 1 });
                if obj::is_line_shape(kind) {
                    g.draw_rectangle(&Pen::new(accent(), 1.0).with_dash(DashStyle::Dash), Rect::new(x, y, x + b.w as f32, y + b.h as f32));
                }
                paint_glyph(g, &ghost, &ShapeBox { x, y, w: b.w as f32, h: b.h as f32, zoom: ed.zoom }, self.dark);
            }
        }
        let active = layer.sel.last().cloned();
        for r in &layer.sel {
            let Some(rect) = layer.rect_for(ed, r) else { continue };
            let kind = ed.object(r).and_then(|o| o.get("kind")).and_then(Json::as_str).unwrap_or("").to_string();
            let content = ObjRect { x: rect.x - self.scroll_x, y: rect.y - self.scroll_y, ..rect };
            if r.kind == ObjKind::Shape && obj::is_line_shape(&kind) {
                for (_, ex, ey) in obj::line_ends(&kind, &content) {
                    let c = PointF::new(self.ox + ex as f32, self.oy + ey as f32);
                    knob(g, c, obj::CORNER_R as f32, Color::WHITE, accent(), obj::CHROME_BORDER as f32);
                }
                continue;
            }
            self.object_chrome(&rect);
            if r.kind == ObjKind::Shape && Some(r) == active.as_ref() {
                let o = ed.object(r);
                let adj: Option<Vec<f64>> = layer.adj_for(r).map(<[f64]>::to_vec).or_else(|| o.and_then(|o| o.get("adj")).and_then(Json::as_array).map(|a| a.iter().filter_map(Json::as_f64).collect()));
                let srect = kubuno_office_shapes_core::Rect { x: rect.x, y: rect.y, w: rect.w, h: rect.h };
                let s = g.save();
                let (sx, sy, sw, sh) = self.screen(&rect);
                if rect.rot != 0.0 {
                    g.rotate_transform_at(rect.rot as f32, PointF::new(sx + sw / 2.0, sy + sh / 2.0));
                }
                for k in shape_adjust::adjust_handles(&kind, srect, adj.as_deref()) {
                    let c = PointF::new(self.ox + (k.x - self.scroll_x) as f32, self.oy + (k.y - self.scroll_y) as f32);
                    knob(g, c, 4.5, Color::from_hex("#ffd400").unwrap_or(Color::WHITE), Color::from_hex("#8a6d00").unwrap_or(Color::BLACK), 1.0);
                }
                g.restore(s);
            }
        }
        g.restore(saved);
    }

    /// `drawObjectChrome`: a 2 px accent frame just outside the box, white corner discs and edge pills, the
    /// rotation stem and knob with its circular arrow - turned with the object.
    fn object_chrome(&self, rect: &ObjRect) {
        let g = self.g;
        let (x, y, w, h) = self.screen(rect);
        let s = g.save();
        if rect.rot != 0.0 {
            g.rotate_transform_at(rect.rot as f32, PointF::new(x + w / 2.0, y + h / 2.0));
        }
        let gap = obj::CHROME_GAP as f32;
        let pen = Pen::new(accent(), obj::CHROME_BORDER as f32);
        g.draw_rectangle(&pen, Rect::new(x - gap, y - gap, x + w + gap, y + h + gap));
        let (_, ky, below) = obj::knob_point(rect, COL_HEADER_HEIGHT);
        let knob_y = self.oy + (ky - self.scroll_y) as f32;
        let cx = x + w / 2.0;
        let arm_from = if below { y + h + gap } else { y - gap };
        g.draw_line(&pen, PointF::new(cx, arm_from), PointF::new(cx, knob_y));
        let r = obj::ROTATE_KNOB_R as f32;
        knob(g, PointF::new(cx, knob_y), r, Color::WHITE, accent(), obj::CHROME_BORDER as f32);
        // The circular arrow: an arc with a gap, its head on the arc's start, along the tangent.
        let gr = 5.0f32;
        let (a0, a1) = (-std::f32::consts::PI * 0.6, std::f32::consts::PI * 0.95);
        let mut arc = kubuno_desktop::ui::graphics::GraphicsPath::new();
        let steps = 24;
        for i in 0..=steps {
            let a = a0 + (a1 - a0) * i as f32 / steps as f32;
            let p = PointF::new(cx + gr * a.cos(), knob_y + gr * a.sin());
            if i == 0 {
                arc.move_to(p);
            } else {
                arc.line_to(p);
            }
        }
        g.draw_path(&Pen::new(accent(), 1.5), &arc);
        let (px, py) = (cx + gr * a0.cos(), knob_y + gr * a0.sin());
        let (tx, ty) = (a0.sin(), -a0.cos());
        let (nx, ny) = (-ty, tx);
        let (head, half) = (4.2f32, 2.6f32);
        g.fill_polygon(Brush::solid(accent()), &[PointF::new(px + tx * head, py + ty * head), PointF::new(px + nx * half, py + ny * half), PointF::new(px - nx * half, py - ny * half)]);
        let local = ObjRect { x: (x - self.ox) as f64, y: (y - self.oy) as f64, ..*rect };
        for (hd, hx, hy) in obj::handle_points(&local) {
            let c = PointF::new(self.ox + hx as f32, self.oy + hy as f32);
            let (ex, ey) = obj::handle_extent(hd);
            if hd.is_edge() {
                let rr = Rect::new(c.x - ex as f32, c.y - ey as f32, c.x + ex as f32, c.y + ey as f32);
                let rad = ex.min(ey) as f32;
                g.fill_rounded_rectangle(Brush::solid(Color::WHITE), rr, rad);
                g.draw_rounded_rectangle(&pen, rr, rad);
            } else {
                knob(g, c, obj::CORNER_R as f32, Color::WHITE, accent(), obj::CHROME_BORDER as f32);
            }
        }
        g.restore(s);
    }
}

fn accent() -> Color {
    Color::from_hex(obj::ACCENT).unwrap_or(Color::BLUE)
}

fn knob(g: &Graphics<'_>, c: PointF, r: f32, fill: Color, stroke: Color, width: f32) {
    let rr = Rect::new(c.x - r, c.y - r, c.x + r, c.y + r);
    g.fill_ellipse(Brush::solid(fill), rr);
    g.draw_ellipse(&Pen::new(stroke, width), rr);
}

/// The label of the selection in the Name Box (`cellAddressLabel`, `objLabel`).
pub fn selection_label(ed: &Editor, sel: &[ObjRef], words: &ObjWords) -> Option<String> {
    match sel {
        [] => None,
        [r] => {
            let i = obj::find_index(ed.sheet_data(), r).map(|i| i + 1).unwrap_or(1);
            let stem = match r.kind {
                ObjKind::Chart => &words.chart,
                ObjKind::Image => &words.image,
                ObjKind::Shape => &words.shape,
                ObjKind::Equation => &words.equation,
            };
            Some(format!("{stem} {i}"))
        }
        many => Some(words.count.replace("{count}", &many.len().to_string())),
    }
}

/// The words of the object labels.
#[derive(Clone, Debug, Default)]
pub struct ObjWords {
    pub chart: String,
    pub image: String,
    pub shape: String,
    pub equation: String,
    pub count: String,
}

/// The hit of a shape's adjustment knob (`hitAdjust` in the shape's own frame) for the active shape.
pub fn hit_adjust(ed: &Editor, sel: &[ObjRef], px: f64, py: f64) -> Option<(ObjRef, usize, ObjRect)> {
    let r = sel.last()?;
    if r.kind != ObjKind::Shape {
        return None;
    }
    let o = ed.object(r)?;
    let kind = o.get("kind").and_then(Json::as_str)?;
    if obj::is_line_shape(kind) {
        return None;
    }
    let rect = ed.object_rect(r, None)?;
    let (lx, ly) = obj::to_local(&rect, px, py);
    let adj: Option<Vec<f64>> = o.get("adj").and_then(Json::as_array).map(|a| a.iter().filter_map(Json::as_f64).collect());
    let srect = kubuno_office_shapes_core::Rect { x: rect.x, y: rect.y, w: rect.w, h: rect.h };
    let i = shape_adjust::hit_adjust(kind, srect, lx, ly, adj.as_deref(), 7.0)?;
    Some((r.clone(), i, rect))
}
