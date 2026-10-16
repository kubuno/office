//! Painting the grid: the port of the web's `drawGrid` and `drawSelection` (`SpreadsheetApp.tsx:4817-5466`)
//! onto the framework's `Graphics` - the four freeze quadrants (backgrounds, gridlines, borders drawn once
//! per shared edge with the heavier one winning, text with overflow into empty neighbours and wrap,
//! validation widgets, notes, conditional-format bars and icons, merges in two passes), the freeze dividers,
//! the headers and the corner, then the selection chrome on top.
//!
//! The grid's colours are the web's literals: like the web, the grid is light in both themes (the office
//! module has no dark palette for its canvas); only the window's chrome follows the theme.

use std::collections::HashMap;

use kubuno_desktop::ui::graphics::{Brush, Color, DashStyle, Font, FontStyle, GradientStop, Graphics, LinearGradientBrush, Pen, PointF, RadialGradientBrush, StringAlignment, StringFormat, StringFormatFlags, StringTrimming};
use kubuno_desktop::ui::Rect;
use kubuno_office_sheets_core::addr::{self, Cell as Pos, MAX_COLS, MAX_ROWS};
use kubuno_office_sheets_core::cf::CondStyle;
use kubuno_office_sheets_core::editor::Editor;
use kubuno_office_sheets_core::formula::eval::{Ctx, Data, SpillIndex};
use kubuno_office_sheets_core::grid::{self, Geometry, MergeInfo, COL_HEADER_HEIGHT, ROW_HEADER_WIDTH};
use kubuno_office_sheets_core::model::style::{Align, Style, VAlign};
use kubuno_office_sheets_core::validation::{self, DVCriterion};
use kubuno_office_spreadsheets_common::model::view::{GridView, Quadrant};

use super::icons::{draw_cf_icon, hex};

/// The web's grid colours.
pub mod colors {
    pub const PAGE: &str = "#ffffff";
    pub const GRIDLINE: &str = "#e2e4e6";
    pub const HEADER: &str = "#f8f9fa";
    pub const HEADER_SELECTED: &str = "#d3e3fd";
    pub const HEADER_TEXT: &str = "#444444";
    pub const HEADER_LINE: &str = "#c1c7cd";
    pub const FREEZE: &str = "#9aa0a6";
    pub const HIDDEN_TICK: &str = "#5f6368";
    pub const TEXT: &str = "#202124";
    pub const LINK: &str = "#1155cc";
    pub const ACCENT: &str = "#1a73e8";
    pub const MARK: &str = "#d93025";
}

/// What the grid draws over the cells besides the selection.
#[derive(Clone, Debug, Default)]
pub struct Overlay {
    /// The fill handle's preview box while dragging it.
    pub fill: Option<addr::Rect>,
    /// A header drag-move's drop line: (is column, before index).
    pub drop: Option<(bool, u32)>,
    /// The cell being edited: no fill handle.
    pub editing: bool,
    /// References of the formula being edited, with their colours (`#rrggbb`).
    pub refs: Vec<(addr::Rect, String)>,
    /// Column filters (column → active).
    pub filters: HashMap<u32, bool>,
    pub filter_mode: bool,
}

/// One paint of the grid.
pub struct GridPainter<'a> {
    pub g: &'a Graphics<'a>,
    /// The control's top-left corner on the canvas.
    pub ox: f32,
    pub oy: f32,
    /// Device pixels per DIP.
    pub dpr: f32,
    pub ed: &'a Editor,
    pub geom: &'a Geometry,
    pub view: &'a GridView,
    pub merges: &'a MergeInfo,
    pub lang: &'a str,
    /// Paints the floating objects between the cells and the headers (pictures, charts, shapes).
    pub objects: Option<&'a dyn Fn()>,
}

fn color(c: &str) -> Color {
    hex(c)
}

fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color {
    Color::rgb(r, g, b).with_alpha(a)
}

/// `rgbaFromHex(hex, opacity)` for gradient stops.
fn stop_color(c: &str, opacity: Option<f64>) -> Color {
    color(c).with_alpha(opacity.unwrap_or(1.0) as f32)
}

impl<'a> GridPainter<'a> {
    fn r(&self, x: f64, y: f64, w: f64, h: f64) -> Rect {
        let (x, y) = (self.ox + x as f32, self.oy + y as f32);
        Rect::new(x, y, x + w as f32, y + h as f32)
    }

    fn fill(&self, c: Color, x: f64, y: f64, w: f64, h: f64) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        self.g.fill_rectangle(Brush::solid(c), self.r(x, y, w, h));
    }

    /// A device-pixel-snapped coordinate for a stroke `ww` physical pixels wide (`hlineAt`/`vlineAt`).
    fn snap(&self, v: f32, ww: f32) -> f32 {
        let half = if (ww as i32) % 2 == 1 { 0.5 } else { 0.0 };
        ((v * self.dpr).round() + half) / self.dpr
    }

    /// A horizontal line `ww` device pixels thick at viewport y `y0`.
    fn hline(&self, c: &str, ww: f32, x1: f64, x2: f64, y0: f64) {
        let y = self.snap(self.oy + y0 as f32, ww);
        self.g.draw_line(&Pen::new(color(c), ww / self.dpr), PointF::new(self.ox + x1 as f32, y), PointF::new(self.ox + x2 as f32, y));
    }

    /// A vertical line `ww` device pixels thick at viewport x `x0`.
    fn vline(&self, c: &str, ww: f32, y1: f64, y2: f64, x0: f64) {
        let x = self.snap(self.ox + x0 as f32, ww);
        self.g.draw_line(&Pen::new(color(c), ww / self.dpr), PointF::new(x, self.oy + y1 as f32), PointF::new(x, self.oy + y2 as f32));
    }

    /// A 1-DIP line on half-DIP centres (`Math.round(v) + 0.5` / `- 0.5` of the headers).
    fn line_css(&self, c: &str, w: f32, x1: f64, y1: f64, x2: f64, y2: f64) {
        self.g.draw_line(&Pen::new(color(c), w), PointF::new(self.ox + x1 as f32, self.oy + y1 as f32), PointF::new(self.ox + x2 as f32, self.oy + y2 as f32));
    }

    /// A rectangle outline whose stroke of `lw` DIP lands on whole device pixels (`crispRect`).
    #[allow(clippy::too_many_arguments)]
    fn crisp_rect(&self, c: Color, x0: f64, y0: f64, x1: f64, y1: f64, lw: f32, dash: bool) {
        let snap = |v: f32| -> f32 {
            let odd = ((lw * self.dpr).round() as i32) % 2 == 1;
            ((v * self.dpr).round() + if odd { 0.5 } else { 0.0 }) / self.dpr
        };
        let (l, t, r, b) = (snap(self.ox + x0 as f32), snap(self.oy + y0 as f32), snap(self.ox + x1 as f32), snap(self.oy + y1 as f32));
        let mut pen = Pen::new(c, lw);
        if dash {
            pen = pen.with_dash_pattern(&[3.0 / lw, 2.0 / lw]);
        }
        self.g.draw_rectangle(&pen, Rect::new(l, t, r, b));
    }

    fn clip<R>(&self, x0: f64, y0: f64, x1: f64, y1: f64, f: impl FnOnce() -> R) -> R {
        let st = self.g.save();
        self.g.intersect_clip(self.r(x0, y0, x1 - x0, y1 - y0));
        let out = f();
        self.g.restore(st);
        out
    }

    fn font(style: &Style, cf: Option<&CondStyle>, zoom: f64) -> (Font, f64) {
        let fs = style.font_size.unwrap_or(13.0) * zoom;
        let italic = cf.and_then(|c| c.italic).unwrap_or(style.is_italic());
        let bold = cf.and_then(|c| c.bold).unwrap_or(style.is_bold());
        let mut fst = FontStyle::REGULAR.0;
        if bold {
            fst |= FontStyle::BOLD.0;
        }
        if italic {
            fst |= FontStyle::ITALIC.0;
        }
        let family = style.font_family.as_deref().filter(|f| !f.is_empty()).unwrap_or("Arial");
        (Font::with_dip(family, fs as f32, FontStyle(fst)), fs)
    }

    fn measure(&self, text: &str, font: &Font) -> f64 {
        self.g.measure_string(text, font, None, &StringFormat::generic_typographic()).width as f64
    }

    /// `fillText` at `(tx, ty)` with `textAlign` and `textBaseline = 'middle'`.
    #[allow(clippy::too_many_arguments)]
    fn text_at(&self, text: &str, font: &Font, c: Color, tx: f64, ty: f64, align: Align, fs: f64) {
        let h = fs.max(1.0) * 2.0;
        // Right and centred text are placed from their measured width: a far-aligned layout box of a
        // huge width does not render with DirectWrite.
        let w = self.measure(text, font);
        let l = match align {
            Align::Left => tx,
            Align::Right => tx - w,
            Align::Center => tx - w / 2.0,
        };
        let r = l + w + 4.0;
        let fmt = StringFormat { alignment: StringAlignment::Near, line_alignment: StringAlignment::Center, trimming: StringTrimming::None, flags: StringFormatFlags::NO_WRAP | StringFormatFlags::NO_CLIP };
        let rect = Rect::new(self.ox + l as f32, self.oy + (ty - h / 2.0) as f32, self.ox + r as f32, self.oy + (ty + h / 2.0) as f32);
        self.g.draw_string(text, font, Brush::solid(c), rect, &fmt);
    }

    fn sx(&self, q: &Quadrant, cx: f64) -> f64 {
        cx - q.off_x
    }

    fn sy(&self, q: &Quadrant, cy: f64) -> f64 {
        cy - q.off_y
    }

    // ── The whole grid ──────────────────────────────────────────────────────

    /// Paints everything: cells of every quadrant, dividers, headers, corner, selection.
    pub fn paint(&self, data: Data<'_>, ctx: &Ctx<'_>, spill: Option<&SpillIndex>, cf: &HashMap<String, CondStyle>, overlay: &Overlay) {
        let (vw, vh) = (self.view.width, self.view.height);
        self.fill(color(colors::PAGE), 0.0, 0.0, vw, vh);
        let quads = self.view.quadrants(self.ed, self.geom);
        for q in &quads {
            self.clip(q.clip.0, q.clip.1, q.clip.2, q.clip.3, || self.paint_cells(q, data, ctx, spill, cf));
        }
        self.paint_images(overlay);
        let s = self.ed.sheet();
        let (f_cols, f_rows) = (s.frozen_cols().min(MAX_COLS), s.frozen_rows().min(MAX_ROWS));
        let freeze_x = self.geom.col_left(f_cols);
        let freeze_y = self.geom.row_top(f_rows + 1);
        if f_cols > 0 {
            let x = freeze_x.round() - 0.5;
            self.line_css(colors::FREEZE, 1.0, x, COL_HEADER_HEIGHT, x, vh);
        }
        if f_rows > 0 {
            let y = freeze_y.round() - 0.5;
            self.line_css(colors::FREEZE, 1.0, ROW_HEADER_WIDTH, y, vw, y);
        }
        self.paint_headers(overlay);
        self.paint_selection(spill, overlay);
    }

    /// The floating objects (`objects.rs`): pictures, charts, shapes, under the headers and the selection.
    fn paint_images(&self, _overlay: &Overlay) {
        if let Some(f) = self.objects {
            f();
        }
    }

    fn paint_cells(&self, q: &Quadrant, data: Data<'_>, ctx: &Ctx<'_>, spill: Option<&SpillIndex>, cf: &HashMap<String, CondStyle>) {
        let g = self.geom;
        let sheet = self.ed.sheet();
        let cells = &sheet.cells;
        let style_at = |c: u32, r: u32| sheet.style_at(c, r);
        // Backgrounds.
        for c in q.c0..=q.cn {
            let (x, w) = (self.sx(q, g.col_left(c)), g.col_width(c));
            if w <= 0.0 {
                continue;
            }
            for r in q.r0..=q.rn {
                let h = g.row_height(r);
                if h == 0.0 {
                    continue;
                }
                let y = self.sy(q, g.row_top(r));
                let key = addr::key(c, r);
                if self.merges.covered.contains_key(&key) || self.merges.anchors.contains_key(&key) {
                    continue;
                }
                let style = style_at(c, r);
                self.paint_background(&style, cf.get(&key), x, y, w, h, false);
            }
        }
        // Gridlines.
        if sheet.gridlines() {
            let pen = Pen::new(color(colors::GRIDLINE), 1.0 / self.dpr);
            let (top, bottom) = (self.sy(q, g.row_top(q.r0)), self.sy(q, g.row_top(q.rn + 1)));
            for c in q.c0..=q.cn {
                let xr = self.snap(self.ox + self.sx(q, g.col_left(c + 1)) as f32, 1.0);
                self.g.draw_line(&pen, PointF::new(xr, self.oy + top as f32), PointF::new(xr, self.oy + bottom as f32));
            }
            let (left, right) = (self.sx(q, g.col_left(q.c0)), self.sx(q, g.col_left(q.cn + 1)));
            for r in q.r0..=q.rn {
                if g.row_height(r) == 0.0 {
                    continue;
                }
                let yb = self.snap(self.oy + self.sy(q, g.row_top(r + 1)) as f32, 1.0);
                self.g.draw_line(&pen, PointF::new(self.ox + left as f32, yb), PointF::new(self.ox + right as f32, yb));
            }
        }
        // Explicit borders, once per shared edge, the heavier one winning (`pickEdge`).
        let origin_of = |c: i64, r: i64| -> Option<String> {
            if c < 0 || r < 1 {
                return None;
            }
            let k = addr::key(c as u32, r as u32);
            if self.merges.anchors.contains_key(&k) {
                Some(k)
            } else {
                self.merges.covered.get(&k).cloned()
            }
        };
        let s_of = |c: i64, r: i64| -> Option<Style> {
            if c >= 0 && (c as u32) < MAX_COLS && r >= 1 && (r as u32) <= MAX_ROWS {
                Some(style_at(c as u32, r as u32))
            } else {
                None
            }
        };
        let col_visible = |c: i64| c >= 0 && (c as u32) < MAX_COLS && g.col_width(c as u32) > 0.0;
        let pick = |c1: Option<&String>, w1: Option<f64>, c2: Option<&String>, w2: Option<f64>| -> Option<(String, f64)> {
            let a = c1.filter(|c| !c.is_empty()).map(|c| (c.clone(), w1.unwrap_or(1.0)));
            let b = c2.filter(|c| !c.is_empty()).map(|c| (c.clone(), w2.unwrap_or(1.0)));
            match (a, b) {
                (None, b) => b,
                (a, None) => a,
                (Some(a), Some(b)) => Some(if a.1 >= b.1 { a } else { b }),
            }
        };
        for c in (q.c0 as i64 - 1)..=(q.cn as i64) {
            for r in q.r0..=q.rn {
                if g.row_height(r) == 0.0 {
                    continue;
                }
                let (oa, ob) = (origin_of(c, r as i64), origin_of(c + 1, r as i64));
                if oa.is_some() && oa == ob {
                    continue;
                }
                let lc = if col_visible(c) { s_of(c, r as i64) } else { None };
                let rc = if col_visible(c + 1) { s_of(c + 1, r as i64) } else { None };
                let (lb, rb) = (lc.as_ref().and_then(|s| s.br.as_ref()), rc.as_ref().and_then(|s| s.bl.as_ref()));
                if lb.is_none() && rb.is_none() {
                    continue;
                }
                if let Some((col, w)) = pick(lb, lc.as_ref().and_then(|s| s.brw), rb, rc.as_ref().and_then(|s| s.blw)) {
                    let x = self.sx(q, g.col_left((c + 1) as u32));
                    self.vline(&col, w as f32, self.sy(q, g.row_top(r)), self.sy(q, g.row_top(r + 1)), x);
                }
            }
        }
        for r in (q.r0 as i64 - 1)..=(q.rn as i64) {
            if r >= 1 && g.row_height(r as u32) == 0.0 {
                continue;
            }
            for c in q.c0..=q.cn {
                let (oa, ob) = (origin_of(c as i64, r), origin_of(c as i64, r + 1));
                if oa.is_some() && oa == ob {
                    continue;
                }
                let (tc, bc) = (s_of(c as i64, r), s_of(c as i64, r + 1));
                let (tb, bb) = (tc.as_ref().and_then(|s| s.bb.as_ref()), bc.as_ref().and_then(|s| s.bt.as_ref()));
                if tb.is_none() && bb.is_none() {
                    continue;
                }
                if let Some((col, w)) = pick(tb, tc.as_ref().and_then(|s| s.bbw), bb, bc.as_ref().and_then(|s| s.btw)) {
                    let y = self.sy(q, g.row_top((r + 1) as u32));
                    self.hline(&col, w as f32, self.sx(q, g.col_left(c)), self.sx(q, g.col_left(c + 1)), y);
                }
            }
        }
        // Text: first the cell left of the view whose text overflows into it, then the visible cells.
        let has_content = |k: &str| cells.get(k).is_some_and(|c| c.has_value() || c.has_f());
        let spilled = |k: &str| spill.is_some_and(|s| s.values.contains_key(k));
        if q.c0 > 0 {
            for r in q.r0..=q.rn {
                let h = g.row_height(r);
                if h == 0.0 {
                    continue;
                }
                let lo = q.c0.saturating_sub(256);
                let mut cc = q.c0 as i64 - 1;
                while cc >= lo as i64 {
                    let k = addr::key(cc as u32, r);
                    if self.merges.covered.contains_key(&k) || self.merges.anchors.contains_key(&k) || spilled(&k) {
                        break;
                    }
                    if has_content(&k) {
                        self.paint_cell_content(&k, cc as u32, r, self.sx(q, g.col_left(cc as u32)), self.sy(q, g.row_top(r)), g.col_width(cc as u32), h, true, data, ctx, spill, cf);
                        break;
                    }
                    cc -= 1;
                }
            }
        }
        for c in q.c0..=q.cn {
            let (x, w) = (self.sx(q, g.col_left(c)), g.col_width(c));
            for r in q.r0..=q.rn {
                let h = g.row_height(r);
                if h == 0.0 {
                    continue;
                }
                let key = addr::key(c, r);
                if !cells.contains_key(&key) && !spilled(&key) {
                    continue;
                }
                if self.merges.covered.contains_key(&key) || self.merges.anchors.contains_key(&key) {
                    continue;
                }
                self.paint_cell_content(&key, c, r, x, self.sy(q, g.row_top(r)), w, h, true, data, ctx, spill, cf);
            }
        }
        // Merges: every fill first, then borders and content.
        let mut vis = Vec::new();
        for (a_key, b) in &self.merges.anchors {
            if b.c2 < q.c0 || b.c1 > q.cn || b.r2 < q.r0 || b.r1 > q.rn {
                continue;
            }
            let (x, y) = (self.sx(q, g.col_left(b.c1)), self.sy(q, g.row_top(b.r1)));
            let (w, hh) = (g.col_left(b.c2 + 1) - g.col_left(b.c1), g.row_top(b.r2 + 1) - g.row_top(b.r1));
            if w <= 0.0 || hh <= 0.0 {
                continue;
            }
            vis.push((a_key.clone(), *b, x, y, w, hh));
        }
        for (a_key, b, x, y, w, hh) in &vis {
            let st = style_at(b.c1, b.r1);
            let bg = cf.get(a_key).and_then(|c| c.bg.clone()).or(st.bg.clone()).unwrap_or_else(|| "#ffffff".into());
            self.fill(color(&bg), *x, *y, *w, *hh);
        }
        for (a_key, b, x, y, w, hh) in &vis {
            let s_at = |c: u32, r: u32| cells.get(&addr::key(c, r)).map(|c| c.style());
            let st = s_at(b.c1, b.r1);
            let (sl, sr, sb) = (s_at(b.c1, b.r1), s_at(b.c2, b.r1), s_at(b.c1, b.r2));
            if let Some(c) = st.as_ref().and_then(|s| s.bt.clone()) {
                self.hline(&c, st.as_ref().and_then(|s| s.btw).unwrap_or(1.0) as f32, *x, x + w, *y);
            }
            if let Some(c) = sb.as_ref().and_then(|s| s.bb.clone()) {
                self.hline(&c, sb.as_ref().and_then(|s| s.bbw).unwrap_or(1.0) as f32, *x, x + w, y + hh);
            }
            if let Some(c) = sl.as_ref().and_then(|s| s.bl.clone()) {
                self.vline(&c, sl.as_ref().and_then(|s| s.blw).unwrap_or(1.0) as f32, *y, y + hh, *x);
            }
            if let Some(c) = sr.as_ref().and_then(|s| s.br.clone()) {
                self.vline(&c, sr.as_ref().and_then(|s| s.brw).unwrap_or(1.0) as f32, *y, y + hh, x + w);
            }
            self.paint_cell_content(a_key, b.c1, b.r1, *x, *y, *w, *hh, false, data, ctx, spill, cf);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_background(&self, style: &Style, cf: Option<&CondStyle>, x: f64, y: f64, w: f64, h: f64, _merge: bool) {
        if let Some(gr) = &style.bg_gradient {
            let mut stops: Vec<GradientStop> = gr.stops.iter().map(|s| GradientStop::new((s.position / 100.0).clamp(0.0, 1.0) as f32, stop_color(&s.color, s.opacity))).collect();
            stops.sort_by(|a, b| a.position.partial_cmp(&b.position).unwrap_or(std::cmp::Ordering::Equal));
            let rect = self.r(x, y, w, h);
            if stops.is_empty() {
                return;
            }
            let brush = if gr.radial {
                let c = PointF::new(rect.left + (w / 2.0) as f32, rect.top + (h / 2.0) as f32);
                let rad = (w.max(h) / 2.0) as f32;
                Brush::Radial(RadialGradientBrush::new(c, rad, rad, stops[0].color, stops[stops.len() - 1].color).with_stops(&stops))
            } else {
                let (dx, dy) = ((gr.angle.to_radians()).sin() as f32, -(gr.angle.to_radians()).cos() as f32);
                let half = (dx.abs() * w as f32 + dy.abs() * h as f32) / 2.0;
                let (cx, cy) = (rect.left + (w / 2.0) as f32, rect.top + (h / 2.0) as f32);
                Brush::Linear(LinearGradientBrush::new(PointF::new(cx - dx * half, cy - dy * half), PointF::new(cx + dx * half, cy + dy * half), stops[0].color, stops[stops.len() - 1].color).with_stops(&stops))
            };
            self.g.fill_rectangle(brush, rect);
            return;
        }
        if let Some(bg) = cf.and_then(|c| c.bg.as_deref()).or(style.bg.as_deref()) {
            self.fill(color(bg), x, y, w, h);
        }
    }

    /// `drawCellContent(key, cell, x, y, w, h, c, r)`; `neighbours` = whether the overflow may look at the
    /// neighbouring cells (not for a merge's span).
    #[allow(clippy::too_many_arguments)]
    fn paint_cell_content(&self, key: &str, c: u32, r: u32, x: f64, y: f64, w: f64, h: f64, neighbours: bool, data: Data<'_>, ctx: &Ctx<'_>, spill: Option<&SpillIndex>, cf: &HashMap<String, CondStyle>) {
        let sheet = self.ed.sheet();
        let cell = sheet.cells.get(key);
        let style = sheet.style_at(c, r);
        let zoom = self.ed.zoom;
        let shown = grid::shown(cell, key, &style, data, ctx, spill, self.lang);
        let display = shown.text.as_str();
        let num = shown.num;
        // Validation widgets.
        let dv = self.ed.validation_at(Pos::new(c, r));
        if let Some(rule) = &dv {
            if validation::is_checkbox(Some(rule)) {
                let checked = matches!(cell.and_then(|c| c.v.as_ref()), Some(serde_json::Value::Bool(true))) || display == "TRUE" || display == "1";
                let sz = (14.0 * zoom).min(h - 4.0).min(w - 4.0).max(8.0);
                let (bx, by) = ((x + (w - sz) / 2.0).round(), (y + (h - sz) / 2.0).round());
                if checked {
                    self.fill(color(colors::ACCENT), bx, by, sz, sz);
                    let pen = Pen::new(Color::WHITE, (sz / 8.0).max(1.5) as f32);
                    let p = |fx: f64, fy: f64| PointF::new(self.ox + (bx + sz * fx) as f32, self.oy + (by + sz * fy) as f32);
                    self.g.draw_lines(&pen, &[p(0.22, 0.52), p(0.42, 0.72), p(0.78, 0.26)]);
                } else {
                    self.g.draw_rectangle(&Pen::new(color("#9aa0a6"), 1.2), self.r(bx + 0.5, by + 0.5, sz, sz));
                }
                return;
            }
            if validation::has_dropdown(Some(rule)) {
                let aw = 16.0 * zoom;
                let (ax, ay) = (x + w - aw, y + h / 2.0);
                let p = |px: f64, py: f64| PointF::new(self.ox + px as f32, self.oy + py as f32);
                self.g.fill_polygon(Brush::solid(color("#5f6368")), &[p(ax + 3.0, ay - 2.0), p(ax + aw - 5.0, ay - 2.0), p(ax + (aw - 2.0) / 2.0, ay + 3.5)]);
            }
            let val = match num {
                Some(n) => kubuno_office_sheets_core::js::to_string(n),
                None => match cell {
                    Some(cl) if !cl.v_is_nullish() => cl.display(),
                    Some(cl) if cl.has_f() => display.to_string(),
                    _ => String::new(),
                },
            };
            if !kubuno_office_sheets_core::js::trim(&val).is_empty() {
                let allowed = if matches!(rule.crit, DVCriterion::ListRange { .. }) { Some(self.ed.validation_list(rule)) } else { None };
                if !validation::validate_value(&rule.crit, &val, allowed.as_deref()) {
                    let p = |px: f64, py: f64| PointF::new(self.ox + px as f32, self.oy + py as f32);
                    self.g.fill_polygon(Brush::solid(color(colors::MARK)), &[p(x, y), p(x + 7.0, y), p(x, y + 7.0)]);
                }
            }
        }
        // A note: the red corner.
        if cell.and_then(|c| c.note()).is_some() {
            let p = |px: f64, py: f64| PointF::new(self.ox + px as f32, self.oy + py as f32);
            self.g.fill_polygon(Brush::solid(color(colors::MARK)), &[p(x + w - 7.0, y), p(x + w, y), p(x + w, y + 7.0)]);
        }
        // Conditional-format overlays.
        let cfo = cf.get(key);
        let mut icon_inset = 0.0;
        if let Some((bar_color, frac)) = cfo.and_then(|c| c.bar.as_ref()) {
            if *frac > 0.0 {
                let bh = (h - 5.0).max(2.0);
                let bw = ((w - 3.0) * frac.min(1.0)).max(1.0);
                let (bx, by) = (x + 1.5, y + (h - bh) / 2.0);
                let rect = self.r(bx, by, bw, bh);
                let base = color(bar_color);
                let brush = LinearGradientBrush::new(PointF::new(rect.left, rect.top), PointF::new(rect.right, rect.top), base, base.with_alpha(0x55 as f32 / 255.0));
                self.g.fill_rectangle(Brush::Linear(brush), rect);
                self.g.draw_rectangle(&Pen::new(base, 1.0), self.r(bx + 0.5, by + 0.5, (bw - 1.0).max(1.0), bh - 1.0));
            }
        }
        if let Some((set, idx, n)) = cfo.and_then(|c| c.icon.as_ref()) {
            icon_inset = draw_cf_icon(self.g, set, *idx, *n, self.ox + x as f32, self.oy + y as f32, h as f32, zoom as f32) as f64;
        }
        if display.is_empty() || cfo.is_some_and(|c| c.hide_value) {
            return;
        }
        let align = style.align.unwrap_or(if num.is_some() { Align::Right } else { Align::Left });
        let (font, fs) = Self::font(&style, cfo, zoom);
        let is_link = cell.and_then(|c| c.link_target()).is_some();
        let ink = color(cfo.and_then(|c| c.color.as_deref()).or(style.color.as_deref()).unwrap_or(if is_link { colors::LINK } else { colors::TEXT }));
        let pad_x = 4.0;
        let tx = match align {
            Align::Right => x + w - pad_x,
            Align::Center => x + w / 2.0,
            Align::Left => x + pad_x + icon_inset,
        };
        let valign = style.valign.unwrap_or(VAlign::Center);
        let vmid = |block_h: f64| match valign {
            VAlign::Top => y + block_h / 2.0 + 2.0,
            VAlign::Bottom => y + h - block_h / 2.0 - 2.0,
            VAlign::Center => y + h / 2.0,
        };
        // Overflow of unwrapped text into empty neighbours.
        let (mut clip_x, mut clip_w) = (x, w);
        if neighbours && num.is_none() && !style.is_wrap() {
            let text_w = self.measure(display, &font) + pad_x * 2.0;
            if text_w > w {
                let g = self.geom;
                let free = |cc: i64| -> bool {
                    if cc < 0 || cc >= MAX_COLS as i64 {
                        return false;
                    }
                    let k = addr::key(cc as u32, r);
                    if self.merges.covered.contains_key(&k) || self.merges.anchors.contains_key(&k) {
                        return false;
                    }
                    if spill.is_some_and(|s| s.values.contains_key(&k)) {
                        return false;
                    }
                    !sheet.cells.get(&k).is_some_and(|nc| nc.has_value() || nc.has_f())
                };
                if align != Align::Right {
                    let mut cc = c as i64 + 1;
                    while clip_w < text_w && free(cc) {
                        clip_w += g.col_width(cc as u32);
                        cc += 1;
                    }
                }
                if align != Align::Left {
                    let mut cc = c as i64 - 1;
                    while clip_w < text_w && free(cc) {
                        let cw = g.col_width(cc as u32);
                        clip_x -= cw;
                        clip_w += cw;
                        cc -= 1;
                    }
                }
            }
        }
        let vover = if style.is_wrap() { 0.0 } else { ((fs * 1.2 - h) / 2.0).max(0.0) };
        self.clip(clip_x, y - vover, clip_x + clip_w, y + h + vover, || {
            if style.is_wrap() {
                let lh = fs * 1.25;
                let lines = self.wrap_text(display, &font, w - pad_x * 2.0);
                let mut ty = vmid(lines.len() as f64 * lh) - (lines.len() as f64 - 1.0) * lh / 2.0 + 1.0;
                for ln in &lines {
                    self.text_at(ln, &font, ink, tx, ty, align, fs);
                    ty += lh;
                }
            } else {
                let ty = vmid(fs) + 1.0;
                self.text_at(display, &font, ink, tx, ty, align, fs);
                if style.is_underline() || is_link || style.is_strike() {
                    let tw = self.measure(display, &font).min(w - pad_x * 2.0);
                    let lx = match align {
                        Align::Right => tx - tw,
                        Align::Center => tx - tw / 2.0,
                        Align::Left => tx,
                    };
                    if style.is_underline() || is_link {
                        let uy = (ty + fs * 0.42).round() + 0.5;
                        self.line_css_color(ink, 1.0, lx, uy, lx + tw, uy);
                    }
                    if style.is_strike() {
                        let my = ty.round() + 0.5;
                        self.line_css_color(ink, 1.0, lx, my, lx + tw, my);
                    }
                }
            }
        });
    }

    fn line_css_color(&self, c: Color, w: f32, x1: f64, y1: f64, x2: f64, y2: f64) {
        self.g.draw_line(&Pen::new(c, w), PointF::new(self.ox + x1 as f32, self.oy + y1 as f32), PointF::new(self.ox + x2 as f32, self.oy + y2 as f32));
    }

    /// `wrapText`: greedy word wrap on whitespace runs.
    fn wrap_text(&self, text: &str, font: &Font, max_w: f64) -> Vec<String> {
        // `text.split(/\s+/)`: a run of whitespace is one separator; leading and trailing runs give
        // empty words, as in JavaScript.
        let mut words: Vec<&str> = Vec::new();
        let mut start = 0;
        let mut in_ws = false;
        for (i, ch) in text.char_indices() {
            let ws = kubuno_office_sheets_core::js::is_whitespace(ch);
            if ws && !in_ws {
                words.push(&text[start..i]);
            }
            if !ws && in_ws {
                start = i;
            }
            in_ws = ws;
        }
        words.push(if in_ws { "" } else { &text[start..] });
        let mut lines = Vec::new();
        let mut line = String::new();
        for word in words {
            let test = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if !line.is_empty() && self.measure(&test, font) > max_w {
                lines.push(std::mem::replace(&mut line, word.to_string()));
            } else {
                line = test;
            }
        }
        if !line.is_empty() {
            lines.push(line);
        }
        lines
    }

    // ── Headers and corner ──────────────────────────────────────────────────

    fn paint_headers(&self, overlay: &Overlay) {
        let g = self.geom;
        let s = self.ed.sheet();
        let (vw, vh) = (self.view.width, self.view.height);
        let (f_cols, f_rows) = (s.frozen_cols().min(MAX_COLS), s.frozen_rows().min(MAX_ROWS));
        let freeze_x = g.col_left(f_cols);
        let freeze_y = g.row_top(f_rows + 1);
        let quads = self.view.quadrants(self.ed, g);
        let body = quads.first().copied();
        // No header is lit while nothing is selected (an empty rectangle: c1 > c2, r1 > r2).
        let sel = if self.ed.selected { self.ed.sel.rect() } else { addr::Rect { c1: 1, r1: 2, c2: 0, r2: 1 } };
        let col_groups = s.col_groups();
        let row_groups = s.row_groups();
        let header_font = Font::with_dip("Arial", 12.0, FontStyle::REGULAR);
        // Column headers.
        let col_headers = |c0: u32, cn: u32, off_x: f64, hx0: f64, hx1: f64| {
            if c0 > cn || hx1 <= hx0 {
                return;
            }
            self.clip(hx0, 0.0, hx1, COL_HEADER_HEIGHT, || {
                for c in c0..=cn {
                    let (x, w) = (g.col_left(c) - off_x, g.col_width(c));
                    let lit = c >= sel.c1 && c <= sel.c2;
                    self.fill(color(if lit { colors::HEADER_SELECTED } else { colors::HEADER }), x, 0.0, w, COL_HEADER_HEIGHT);
                    if w > 0.0 {
                        self.text_at(&addr::index_to_col(c as i64), &header_font, color(colors::HEADER_TEXT), x + w / 2.0, COL_HEADER_HEIGHT / 2.0 + 1.0, Align::Center, 12.0);
                    }
                    let active = overlay.filters.get(&c).copied().unwrap_or(false);
                    if overlay.filter_mode || overlay.filters.contains_key(&c) {
                        self.funnel(x + w - 13.0, COL_HEADER_HEIGHT / 2.0, active);
                    }
                    if let Some(cg) = col_groups.iter().find(|gg| gg.start == c) {
                        self.outline_toggle(x + 2.0, 2.0, cg.collapsed);
                    }
                }
                for c in c0..=cn {
                    let xr = (g.col_left(c + 1) - off_x).round() + 0.5;
                    self.line_css(colors::HEADER_LINE, 1.0, xr, 0.0, xr, COL_HEADER_HEIGHT);
                }
                for c in c0.max(1)..=cn {
                    if g.col_width(c) > 0.0 && g.col_width(c - 1) == 0.0 {
                        let xm = (g.col_left(c) - off_x).round();
                        self.line_css(colors::HIDDEN_TICK, 2.0, xm, 3.0, xm, COL_HEADER_HEIGHT - 3.0);
                    }
                }
            });
        };
        if let Some(b) = body {
            col_headers(b.c0, b.cn, self.view.scroll_x, freeze_x, vw);
        }
        if f_cols > 0 {
            col_headers(0, f_cols - 1, 0.0, ROW_HEADER_WIDTH, freeze_x);
        }
        let hb = COL_HEADER_HEIGHT.round() - 0.5;
        self.line_css(colors::HEADER_LINE, 1.0, ROW_HEADER_WIDTH, hb, vw, hb);
        // Row headers.
        let row_headers = |r0: u32, rn: u32, off_y: f64, hy0: f64, hy1: f64| {
            if r0 > rn || hy1 <= hy0 {
                return;
            }
            self.clip(0.0, hy0, ROW_HEADER_WIDTH, hy1, || {
                for r in r0..=rn {
                    let h = g.row_height(r);
                    if h == 0.0 {
                        continue;
                    }
                    let y = g.row_top(r) - off_y;
                    let lit = r >= sel.r1 && r <= sel.r2;
                    self.fill(color(if lit { colors::HEADER_SELECTED } else { colors::HEADER }), 0.0, y, ROW_HEADER_WIDTH, h);
                    self.text_at(&r.to_string(), &header_font, color(colors::HEADER_TEXT), ROW_HEADER_WIDTH / 2.0, y + h / 2.0 + 1.0, Align::Center, 12.0);
                    if let Some(gr) = row_groups.iter().find(|gg| gg.start == r) {
                        self.outline_toggle(2.0, y + (h - 11.0) / 2.0, gr.collapsed);
                    }
                }
                for r in r0..=rn {
                    if g.row_height(r) == 0.0 {
                        continue;
                    }
                    let yb = (g.row_top(r + 1) - off_y).round() + 0.5;
                    self.line_css(colors::GRIDLINE, 1.0, 0.0, yb, ROW_HEADER_WIDTH, yb);
                }
                for r in r0.max(2)..=rn {
                    if g.row_height(r) > 0.0 && g.row_height(r - 1) == 0.0 {
                        let ym = (g.row_top(r) - off_y).round();
                        self.line_css(colors::HIDDEN_TICK, 2.0, 3.0, ym, ROW_HEADER_WIDTH - 3.0, ym);
                    }
                }
            });
        };
        if let Some(b) = body {
            row_headers(b.r0, b.rn, self.view.scroll_y, freeze_y, vh);
        }
        if f_rows > 0 {
            row_headers(1, f_rows, 0.0, COL_HEADER_HEIGHT, freeze_y);
        }
        let vb = ROW_HEADER_WIDTH.round() - 0.5;
        self.line_css(colors::HEADER_LINE, 1.0, vb, COL_HEADER_HEIGHT, vb, vh);
        // The corner: « select all ».
        let all = sel.c1 == 0 && sel.r1 == 1 && sel.c2 == MAX_COLS - 1 && sel.r2 == MAX_ROWS && self.ed.sel.active == Pos::new(0, 1);
        self.fill(color(if all { "#e8f0fe" } else { colors::HEADER }), 0.0, 0.0, ROW_HEADER_WIDTH, COL_HEADER_HEIGHT);
        let p = |px: f64, py: f64| PointF::new(self.ox + px as f32, self.oy + py as f32);
        self.g.fill_polygon(
            Brush::solid(color(if all { colors::ACCENT } else { colors::FREEZE })),
            &[p(ROW_HEADER_WIDTH - 3.0, COL_HEADER_HEIGHT - 3.0), p(ROW_HEADER_WIDTH - 3.0, COL_HEADER_HEIGHT - 11.0), p(ROW_HEADER_WIDTH - 11.0, COL_HEADER_HEIGHT - 3.0)],
        );
        let cx = ROW_HEADER_WIDTH.round() - 0.5;
        let cy = COL_HEADER_HEIGHT.round() - 0.5;
        self.line_css(colors::HEADER_LINE, 1.0, cx, 0.0, cx, COL_HEADER_HEIGHT);
        self.line_css(colors::HEADER_LINE, 1.0, 0.0, cy, ROW_HEADER_WIDTH, cy);
    }

    /// `drawOutlineToggle`.
    fn outline_toggle(&self, x: f64, y: f64, collapsed: bool) {
        let bs = 11.0;
        self.fill(Color::WHITE, x + 0.5, y + 0.5, bs, bs);
        self.g.draw_rectangle(&Pen::new(color("#5f6368"), 1.0), self.r(x + 0.5, y + 0.5, bs, bs));
        let pen = Pen::new(color(colors::ACCENT), 1.4);
        let p = |px: f64, py: f64| PointF::new(self.ox + px as f32, self.oy + py as f32);
        self.g.draw_line(&pen, p(x + 3.0, y + bs / 2.0 + 0.5), p(x + bs - 2.0, y + bs / 2.0 + 0.5));
        if collapsed {
            self.g.draw_line(&pen, p(x + bs / 2.0 + 0.5, y + 3.0), p(x + bs / 2.0 + 0.5, y + bs - 2.0));
        }
    }

    /// `drawFunnel`.
    fn funnel(&self, cx: f64, cy: f64, active: bool) {
        let (w, h) = (9.0, 8.0);
        let (x, y) = (cx - w / 2.0, cy - h / 2.0);
        let p = |px: f64, py: f64| PointF::new(self.ox + px as f32, self.oy + py as f32);
        let pts = [p(x, y), p(x + w, y), p(x + w * 0.6, y + h * 0.5), p(x + w * 0.6, y + h), p(x + w * 0.4, y + h * 0.78), p(x + w * 0.4, y + h * 0.5)];
        let c = color(if active { colors::ACCENT } else { colors::FREEZE });
        if active {
            self.g.fill_polygon(Brush::solid(c), &pts);
        } else {
            self.g.draw_polygon(&Pen::new(c, 1.0), &pts);
        }
    }

    // ── Selection ───────────────────────────────────────────────────────────

    /// `drawSelection`: per pane, the range tint, the active cell's tint and box, the range border, the
    /// fill handle, the fill preview, the spill outline, the header-move drop line and the references of
    /// the formula being edited.
    fn paint_selection(&self, spill: Option<&SpillIndex>, overlay: &Overlay) {
        if !self.ed.selected {
            return;
        }
        let g = self.geom;
        let s = self.ed.sheet();
        let (vw, vh) = (self.view.width, self.view.height);
        let (f_cols, f_rows) = (s.frozen_cols().min(MAX_COLS), s.frozen_rows().min(MAX_ROWS));
        let pxr = |v: f64| (v * self.dpr as f64).round() / self.dpr as f64;
        let freeze_x = pxr(g.col_left(f_cols));
        let freeze_y = pxr(g.row_top(f_rows + 1));
        let (sl, st) = (self.view.scroll_x, self.view.scroll_y);
        let mut panes = vec![(freeze_x, freeze_y, vw, vh, sl, st)];
        if f_cols > 0 {
            panes.push((ROW_HEADER_WIDTH, freeze_y, freeze_x, vh, 0.0, st));
        }
        if f_rows > 0 {
            panes.push((freeze_x, COL_HEADER_HEIGHT, vw, freeze_y, sl, 0.0));
        }
        if f_cols > 0 && f_rows > 0 {
            panes.push((ROW_HEADER_WIDTH, COL_HEADER_HEIGHT, freeze_x, freeze_y, 0.0, 0.0));
        }
        let sel = self.ed.sel;
        let rect = sel.rect();
        // The selection target extended over merges at its corners (`selRangeTarget`).
        let mut range = rect;
        for m in self.merges.ranges.iter() {
            if m.intersects(&rect) {
                range = range.union(m);
            }
        }
        let (ax, ay, ax2, ay2) = g.rect_of(&range);
        let active = sel.active;
        let am = self.merges.anchors.get(&active.key()).copied();
        let (ac2, ar2) = am.map(|m| (m.c2, m.r2)).unwrap_or((active.col, active.row));
        let is_range = sel.end.is_some_and(|e| e != active);
        let spill_box = spill.and_then(|sp| {
            let a = sp.origin.get(&active.key())?;
            let anchor = sp.anchors.get(a)?;
            let p = addr::Cell::parse(a)?;
            (!anchor.blocked && anchor.rows * anchor.cols > 1).then(|| addr::Rect::new(p.col, p.row, (p.col + anchor.cols as u32 - 1).min(MAX_COLS - 1), (p.row + anchor.rows as u32 - 1).min(MAX_ROWS)))
        });
        let accent = color(colors::ACCENT);
        for (cx0, cy0, cx1, cy1, off_x, off_y) in panes {
            if cx1 <= cx0 || cy1 <= cy0 {
                continue;
            }
            self.clip(cx0, cy0, cx1, cy1, || {
                self.fill(rgba(26, 115, 232, 0.12), ax - off_x, ay - off_y, ax2 - ax, ay2 - ay);
                if g.row_top(ar2 + 1) > g.row_top(active.row) {
                    let (x, y) = (g.col_left(active.col) - off_x, g.row_top(active.row) - off_y);
                    let (w, h) = (g.col_left(ac2 + 1) - g.col_left(active.col), g.row_top(ar2 + 1) - g.row_top(active.row));
                    self.fill(rgba(26, 115, 232, 0.10), x, y, w, h);
                    self.crisp_rect(accent, x + 1.0, y + 1.0, x + w - 1.0, y + h - 1.0, 2.0, false);
                }
                if is_range {
                    self.crisp_rect(accent, ax - off_x, ay - off_y, ax2 - off_x, ay2 - off_y, 1.0, false);
                }
                if !overlay.editing && overlay.fill.is_none() {
                    let (hx, hy) = ((ax2 - off_x).round(), (ay2 - off_y).round());
                    self.fill(Color::WHITE, hx - 4.0, hy - 4.0, 8.0, 8.0);
                    self.fill(accent, hx - 3.0, hy - 3.0, 6.0, 6.0);
                }
                if let Some(f) = overlay.fill {
                    let (fx, fy, fx2, fy2) = g.rect_of(&f);
                    self.fill(rgba(95, 99, 104, 0.12), fx - off_x, fy - off_y, fx2 - fx, fy2 - fy);
                    self.crisp_rect(color("#5f6368"), fx - off_x, fy - off_y, fx2 - off_x, fy2 - off_y, 1.0, false);
                }
                if let Some(b) = spill_box {
                    let (x0, y0, x1, y1) = g.rect_of(&b);
                    self.crisp_rect(accent, x0 - off_x, y0 - off_y, x1 - off_x, y1 - off_y, 1.0, true);
                }
                if let Some((is_col, before)) = overlay.drop {
                    if is_col {
                        let x = g.col_left(before.min(MAX_COLS)) - off_x;
                        self.g.draw_line(&Pen::new(accent, 3.0), PointF::new(self.snap(self.ox + x as f32, 3.0), self.oy + COL_HEADER_HEIGHT as f32), PointF::new(self.snap(self.ox + x as f32, 3.0), self.oy + vh as f32));
                    } else {
                        let y = g.row_top(before.min(MAX_ROWS)) - off_y;
                        self.g.draw_line(&Pen::new(accent, 3.0), PointF::new(self.ox + ROW_HEADER_WIDTH as f32, self.snap(self.oy + y as f32, 3.0)), PointF::new(self.ox + vw as f32, self.snap(self.oy + y as f32, 3.0)));
                    }
                }
                for (rr, c) in &overlay.refs {
                    let (x0, y0, x1, y1) = g.rect_of(rr);
                    let col = color(c);
                    self.fill(col.with_alpha(0x14 as f32 / 255.0), x0 - off_x, y0 - off_y, x1 - x0, y1 - y0);
                    let inner = self.r(x0 - off_x + 1.0, y0 - off_y + 1.0, x1 - x0 - 2.0, y1 - y0 - 2.0);
                    self.g.draw_rectangle(&Pen::new(col, 2.0), inner);
                }
            });
        }
        let _ = DashStyle::Solid;
    }
}
