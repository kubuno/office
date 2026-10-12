//! `MathsCanvas` — the formula sheet being viewed, drawn like the web Maths editor (`MathsApp.tsx`): one or
//! more paper sheets stacked vertically, each holding freely placed blocks (formula / graph / text). Increment
//! A is read-only: the canvas opens a document and paints it at parity with the web; editing (increment B) and
//! save (increment C) come next.
//!
//! Each formula block's LaTeX is laid out by the shared math core (`kubuno-office-math-core`, a port of KaTeX)
//! and painted from KaTeX's own glyph outlines (`controls::equations`). Graph blocks are sampled from the
//! shared engine's expression compiler and drawn with the grid/tick maths of `engine::graph`.

// The graph guards keep the web's `!(a > b)` form on purpose (it bails on NaN too, unlike `a <= b`);
// `paint_block`'s positional args mirror a device rect plus the paint context and read clearly.
#![allow(clippy::neg_cmp_op_on_partial_ord, clippy::too_many_arguments)]

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::graphics::{Brush, Color, Font, FontStyle, Graphics, Pen, PointF, StringAlignment, StringFormat, StringFormatFlags, StringTrimming};
use kubuno_desktop::ui::{Canvas, Rect, Size};
use kubuno_desktop::views::component::{Component, Control, ControlCore, EventCx, Keys, PaintEventCx};
use kubuno_desktop::views::events::{Event, KeyEventArgs, MouseButton};

use kubuno_office_math_core::engine::doc::{self, BlockAlign, DocPage, MathBlock, Orientation, TextStyle};
use kubuno_office_math_core::engine::expr::compile;
use kubuno_office_math_core::engine::graph::{fmt_tick, nice_step, GraphSpec};
use kubuno_office_math_core::KATEX_EM;
use kubuno_office_maths_common::model::MathsDoc;

use crate::controls::equations::{css_color, draw_rendered, rendered, rendered_size};

/// The gap around and between sheets in the viewport (DIP, unscaled).
const SHEET_GAP: f32 = 24.0;
/// Padding inside a sheet for the legacy vertical stack (web `SHEET_PAD`).
const SHEET_PAD: f32 = 24.0;
/// The base font size (px) a formula block is laid out at before the block scale and the zoom.
const FORMULA_PX: f32 = 24.0;

/// What the ribbon, the Backstage and the status bar show of the open document.
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct MathsStateEventArgs {
    pub title: String,
    pub pages: usize,
    pub blocks: usize,
    pub zoom: f32,
    pub dirty: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    /// A block is selected.
    pub has_selection: bool,
    /// The selected block is a formula (the Calcul/Matrices/… operations apply to it).
    pub active_is_formula: bool,
    /// Bumped each time a document is opened into the canvas.
    pub opened: u64,
}

/// The formula sheet (see the module doc).
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Maths")]
#[toolbox(icon = "sigma")]
#[default_event("StateChanged")]
pub struct MathsCanvas {
    base: ControlCore,
    /// Occurs when what the ribbon / Backstage / status bar show changes.
    #[event]
    #[category("Action")]
    pub state_changed: Event<MathsStateEventArgs>,
    doc: MathsDoc,
    dark: bool,
    /// The zoom (1.0 = 100 %).
    zoom: f32,
    /// The vertical scroll offset (DIP).
    scroll_y: f32,
    /// The total content height of the last layout (for scroll clamping).
    content_h: f32,
    bounds: Rect,
    opened: u64,
    reported: Option<MathsStateEventArgs>,
    /// The selected block `(page, block)`.
    active: Option<(usize, usize)>,
    /// Document snapshots for undo / redo (the whole pages vector, like the web's history).
    undo: Vec<Vec<DocPage>>,
    redo: Vec<Vec<DocPage>>,
    dirty: bool,
    /// Code mode edits a formula's LaTeX as text (visual mode is the default view).
    code_mode: bool,
    /// Each painted block's device rect, for hit-testing a click (rebuilt every paint).
    block_rects: Vec<(usize, usize, Rect)>,
}

impl Default for MathsCanvas {
    fn default() -> Self {
        Self {
            base: ControlCore::default(),
            state_changed: Event::default(),
            doc: kubuno_office_maths_common::sample::document(),
            dark: false,
            zoom: 1.0,
            scroll_y: 0.0,
            content_h: 0.0,
            bounds: Rect::new(0.0, 0.0, 800.0, 600.0),
            opened: 0,
            reported: None,
            active: None,
            undo: Vec::new(),
            redo: Vec::new(),
            dirty: false,
            code_mode: false,
            block_rects: Vec::new(),
        }
    }
}

impl MathsCanvas {
    /// Opens `doc` into the canvas (the view resets: scroll to the top, history cleared).
    pub fn open(&mut self, doc: MathsDoc) {
        self.doc = doc;
        self.scroll_y = 0.0;
        self.active = None;
        self.undo.clear();
        self.redo.clear();
        self.dirty = false;
        self.opened += 1;
        self.changed();
    }

    /// The open document.
    pub fn document(&self) -> &MathsDoc {
        &self.doc
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Marks the document saved (after a successful save).
    pub fn mark_saved(&mut self) {
        self.dirty = false;
        self.changed();
    }

    pub fn set_code_mode(&mut self, code: bool) {
        self.code_mode = code;
    }

    // ── Editing (increment B) ───────────────────────────────────────────────

    /// Pushes the current pages onto the undo stack before a mutation (clears redo).
    fn snapshot(&mut self) {
        self.undo.push(self.doc.pages.clone());
        if self.undo.len() > 200 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Re-derives the stored `latex` from the pages and flags the document dirty, after a mutation.
    fn after_mutation(&mut self) {
        self.doc.latex = doc::serialize_doc(&self.doc.pages);
        self.dirty = true;
        self.changed();
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.doc.pages, prev));
            self.active = None;
            self.after_mutation();
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.doc.pages, next));
            self.active = None;
            self.after_mutation();
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// The page a new block lands on (the active block's page, else the last page).
    fn target_page(&self) -> usize {
        self.active.map(|(p, _)| p).unwrap_or(self.doc.pages.len().saturating_sub(1))
    }

    /// Inserts a block at the end of the target page and selects it.
    pub fn insert_block(&mut self, block: MathBlock) {
        if self.doc.pages.is_empty() {
            return;
        }
        self.snapshot();
        let p = self.target_page();
        self.doc.pages[p].blocks.push(block);
        self.active = Some((p, self.doc.pages[p].blocks.len() - 1));
        self.after_mutation();
    }

    /// The LaTeX of the active formula block (for the Calcul/Matrices/… operations).
    pub fn active_formula_latex(&self) -> Option<String> {
        let (p, b) = self.active?;
        match self.doc.pages.get(p)?.blocks.get(b)? {
            MathBlock::Formula { latex, .. } => Some(latex.clone()),
            _ => None,
        }
    }

    /// Replaces the active formula's LaTeX (code-mode edit).
    pub fn set_active_formula_latex(&mut self, latex: String) {
        let Some((p, b)) = self.active else { return };
        self.snapshot();
        if let Some(MathBlock::Formula { latex: l, .. }) = self.doc.pages.get_mut(p).and_then(|pg| pg.blocks.get_mut(b)) {
            *l = latex;
            self.after_mutation();
        } else {
            self.undo.pop();
        }
    }

    /// Inserts the LaTeX result of an operation as a new formula block after the active one.
    pub fn insert_result(&mut self, latex: String) {
        self.insert_block(MathBlock::Formula { latex, align: None, numbered: None, frame: None, scale: None });
    }

    /// Appends a structure/symbol snippet to the selected formula, or inserts a new formula block with it
    /// when no formula is selected (the Équation tab).
    pub fn append_or_insert(&mut self, snippet: &str) {
        match self.active_formula_latex() {
            Some(cur) => {
                let joined = if cur.trim().is_empty() { snippet.to_string() } else { format!("{cur} {snippet}") };
                self.set_active_formula_latex(joined);
            }
            None => self.insert_result(snippet.to_string()),
        }
    }

    pub fn delete_active(&mut self) {
        let Some((p, b)) = self.active else { return };
        self.snapshot();
        if let Some(pg) = self.doc.pages.get_mut(p) {
            if b < pg.blocks.len() {
                pg.blocks.remove(b);
            }
        }
        self.active = None;
        self.after_mutation();
    }

    pub fn duplicate_active(&mut self) {
        let Some((p, b)) = self.active else { return };
        let Some(block) = self.doc.pages.get(p).and_then(|pg| pg.blocks.get(b)).cloned() else { return };
        self.snapshot();
        self.doc.pages[p].blocks.insert(b + 1, block);
        self.active = Some((p, b + 1));
        self.after_mutation();
    }

    /// Moves the active block up (-1) or down (+1) within its page.
    pub fn move_active(&mut self, dir: i32) {
        let Some((p, b)) = self.active else { return };
        let Some(pg) = self.doc.pages.get_mut(p) else { return };
        let j = b as i32 + dir;
        if j < 0 || j as usize >= pg.blocks.len() {
            return;
        }
        self.snapshot();
        self.doc.pages[p].blocks.swap(b, j as usize);
        self.active = Some((p, j as usize));
        self.after_mutation();
    }

    pub fn add_page(&mut self) {
        self.snapshot();
        let name = format!("Page {}", self.doc.pages.len() + 1);
        self.doc.pages.push(DocPage {
            name,
            blocks: vec![MathBlock::Formula { latex: String::new(), align: None, numbered: None, frame: None, scale: None }],
            format: None,
            orientation: None,
        });
        self.after_mutation();
    }

    pub fn has_selection(&self) -> bool {
        self.active.is_some()
    }

    /// Property lines for the selected block (the Propriétés dock); empty when nothing is selected.
    pub fn active_block_lines(&self) -> Vec<String> {
        let Some((p, b)) = self.active else { return Vec::new() };
        let Some(block) = self.doc.pages.get(p).and_then(|pg| pg.blocks.get(b)) else { return Vec::new() };
        let page_line = format!("Page {} · bloc {}", p + 1, b + 1);
        match block {
            MathBlock::Formula { latex, align, numbered, .. } => {
                let al = match align {
                    Some(BlockAlign::Left) => "gauche",
                    Some(BlockAlign::Right) => "droite",
                    None => "centre",
                };
                let mut src = latex.clone();
                if src.chars().count() > 40 {
                    src = format!("{}…", src.chars().take(40).collect::<String>());
                }
                vec![
                    "Formule".to_string(),
                    page_line,
                    format!("Alignement : {al}"),
                    format!("Numérotée : {}", if *numbered == Some(true) { "oui" } else { "non" }),
                    format!("LaTeX : {src}"),
                ]
            }
            MathBlock::Graph { spec, .. } => vec![
                "Graphe".to_string(),
                page_line,
                format!("Fonctions : {}", spec.fns.len()),
                format!("Domaine x : [{}, {}]", spec.xmin, spec.xmax),
                format!("Hauteur : {} px", spec.height),
            ],
            MathBlock::Text { text, style, .. } => {
                let st = match style {
                    TextStyle::H1 => "Titre",
                    TextStyle::H2 => "Sous-titre",
                    TextStyle::P => "Paragraphe",
                };
                vec!["Texte".to_string(), page_line, format!("Style : {st}"), format!("Texte : {text}")]
            }
        }
    }

    pub fn set_dark(&mut self, dark: bool) {
        self.dark = dark;
        self.changed();
    }

    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom.clamp(0.25, 4.0);
        self.clamp_scroll();
        self.changed();
    }

    fn changed(&mut self) {
        self.invalidate();
        kubuno_desktop::controls::host::request_repaint_after(0);
    }

    fn clamp_scroll(&mut self) {
        let max = (self.content_h - (self.bounds.bottom - self.bounds.top)).max(0.0);
        self.scroll_y = self.scroll_y.clamp(0.0, max);
    }

    fn colors(&self) -> Colors {
        Colors::of(self.dark)
    }

    fn state_args(&self) -> MathsStateEventArgs {
        MathsStateEventArgs {
            title: self.doc.name.clone(),
            pages: self.doc.pages.len(),
            blocks: self.doc.block_count(),
            zoom: self.zoom * 100.0,
            dirty: self.dirty,
            can_undo: self.can_undo(),
            can_redo: self.can_redo(),
            has_selection: self.active.is_some(),
            active_is_formula: self.active_formula_latex().is_some(),
            opened: self.opened,
        }
    }
}

/// The viewport / sheet / ink palette, light and dark (the web Maths theme tokens).
struct Colors {
    viewport: Color,
    sheet: Color,
    sheet_border: Color,
    ink: Color,
    text_secondary: Color,
}

impl Colors {
    fn of(dark: bool) -> Self {
        if dark {
            Colors {
                viewport: hex("#202124"),
                sheet: hex("#2a2b2e"),
                sheet_border: hex("#5f6368"),
                ink: hex("#e8eaed"),
                text_secondary: hex("#9aa0a6"),
            }
        } else {
            Colors {
                viewport: hex("#f1f3f4"),
                sheet: hex("#ffffff"),
                sheet_border: hex("#e0e0e0"),
                ink: hex("#202124"),
                text_secondary: hex("#5f6368"),
            }
        }
    }
}

fn hex(s: &str) -> Color {
    Color::from_hex(s).unwrap_or(Color::BLACK)
}

impl Control for MathsCanvas {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, _proposed: Size) -> Size {
        Size { width: 800.0, height: 600.0 }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        self.bounds = e.clip_rectangle;
        let g = e.graphics;
        let c = self.colors();
        g.fill_rectangle(Brush::solid(c.viewport), self.bounds);

        let zoom = self.zoom;
        let active = self.active;
        let mut rects: Vec<(usize, usize, Rect)> = Vec::new();
        let mut y = self.bounds.top + SHEET_GAP - self.scroll_y;
        for (pi, page) in self.doc.pages.iter().enumerate() {
            let (pw, ph) = doc::paper_px(
                page.format.as_deref().unwrap_or(doc::DEFAULT_FORMAT),
                page.orientation.unwrap_or(Orientation::Portrait),
                3.78,
            );
            let (pw, ph) = (pw as f32, ph as f32);
            let (sw, sh) = (pw * zoom, ph * zoom);
            // Centre the sheet in the viewport, but never let it push off the right edge: a sheet wider than
            // the viewport is left-aligned at the gutter (its right side then scrolls off, like Word).
            let vw = self.bounds.right - self.bounds.left;
            let centered = self.bounds.left + ((vw - sw) / 2.0);
            let right_limit = (self.bounds.right - sw - SHEET_GAP).max(self.bounds.left + SHEET_GAP);
            let sx = centered.clamp(self.bounds.left + SHEET_GAP, right_limit);
            let sheet = Rect::new(sx, y, sx + sw, y + sh);
            // The sheet: a faint shadow, a white (or dark) rounded page.
            for (dy, a) in [(1.0f32, 0.06f32), (3.0, 0.04)] {
                g.fill_rounded_rectangle(Brush::solid(Color::rgba_f(0.0, 0.0, 0.0, a)), Rect::new(sheet.left, sheet.top + dy, sheet.right, sheet.bottom + dy), 4.0);
            }
            g.fill_rounded_rectangle(Brush::solid(c.sheet), sheet, 4.0);
            g.draw_rounded_rectangle(&Pen::new(c.sheet_border, 1.0), Rect::new(sheet.left + 0.5, sheet.top + 0.5, sheet.right - 0.5, sheet.bottom - 0.5), 4.0);

            let st = g.save();
            g.intersect_clip(Rect::new(sheet.left + 1.0, sheet.top + 1.0, sheet.right - 1.0, sheet.bottom - 1.0));
            self.paint_blocks(g, page, pi, sheet, pw, zoom, &c, active, &mut rects);
            g.restore(st);

            y += sh + SHEET_GAP;
        }
        self.content_h = (y + self.scroll_y) - self.bounds.top;
        self.block_rects = rects;

        if !self.design_mode() {
            let args = self.state_args();
            if self.reported.as_ref() != Some(&args) {
                self.reported = Some(args.clone());
                self.raise_state_changed(args);
            }
        }
        e.raise(self, "OnPaint");
    }

    fn on_mouse_wheel(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let notches = e.args().delta;
        // One notch scrolls ~60 DIP; positive notch (away from the user) scrolls up.
        self.scroll_y -= notches * 60.0;
        self.clamp_scroll();
        self.changed();
        e.raise(&*self, "OnMouseWheel");
    }

    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        if e.args().button == MouseButton::Left {
            let (x, y) = (e.args().x, e.args().y);
            // Topmost block under the point (later blocks paint on top).
            let hit = self
                .block_rects
                .iter()
                .rev()
                .find(|(_, _, r)| x >= r.left && x <= r.right && y >= r.top && y <= r.bottom)
                .map(|(p, b, _)| (*p, *b));
            self.focus();
            if self.active != hit {
                self.active = hit;
                self.changed();
            }
        }
        e.raise(&*self, "OnMouseDown");
    }

    fn is_input_key(&self, key: Keys) -> bool {
        use kubuno_desktop::controls::host::vk;
        [vk::DELETE, vk::BACK].contains(&key.key.0)
    }

    fn on_key_down(&mut self, e: &mut EventCx<'_, KeyEventArgs>) {
        use kubuno_desktop::controls::host::vk;
        let key = e.args().key.0;
        if (key == vk::DELETE || key == vk::BACK) && self.active.is_some() {
            self.delete_active();
            e.args_mut().handled = true;
        }
        e.raise(&*self, "OnKeyDown");
    }
}

impl MathsCanvas {
    /// Lays out and paints a page's blocks inside `sheet` (device DIP). `pw` is the sheet width in sheet px
    /// (unscaled); `zoom` scales sheet px to DIP.
    #[allow(clippy::too_many_arguments)]
    fn paint_blocks(&self, g: &Graphics<'_>, page: &DocPage, page_idx: usize, sheet: Rect, pw: f32, zoom: f32, c: &Colors, active: Option<(usize, usize)>, rects: &mut Vec<(usize, usize, Rect)>) {
        // Legacy stack cursor (sheet px) for frameless blocks.
        let mut stack_y = SHEET_PAD;
        for (bi, block) in page.blocks.iter().enumerate() {
            let frame = block_frame(block);
            let (fx, fy, fw, used_stack) = match frame {
                Some(f) => (f.x as f32, f.y as f32, f.w as f32, false),
                None => (SHEET_PAD, stack_y, pw - SHEET_PAD * 2.0, true),
            };
            // Block rect in device DIP.
            let bx = sheet.left + fx * zoom;
            let by = sheet.top + fy * zoom;
            let bw = fw * zoom;
            let height_px = self.paint_block(g, block, bx, by, bw, zoom, c);
            let block_rect = Rect::new(bx, by, bx + bw, by + height_px);
            rects.push((page_idx, bi, block_rect));
            if active == Some((page_idx, bi)) {
                let pad = 6.0;
                g.draw_rounded_rectangle(&Pen::new(hex("#1a73e8"), 1.5), Rect::new(block_rect.left - pad, block_rect.top - pad, block_rect.right + pad, block_rect.bottom + pad), 4.0);
            }
            if used_stack {
                stack_y += height_px / zoom + 16.0;
            }
        }
    }

    /// Paints one block at (`bx`, `by`) with device width `bw`; returns the device height it took.
    fn paint_block(&self, g: &Graphics<'_>, block: &MathBlock, bx: f32, by: f32, bw: f32, zoom: f32, c: &Colors) -> f32 {
        match block {
            MathBlock::Formula { latex, align, scale, .. } => {
                let src = if latex.trim().is_empty() { "\\square" } else { latex.as_str() };
                let r = rendered(src);
                let block_scale = scale.unwrap_or(1.0) as f32;
                let mut em = FORMULA_PX * KATEX_EM as f32 * zoom * block_scale;
                let (mut w, mut h) = rendered_size(&r, em);
                // Shrink to fit the block width.
                if w > bw && w > 0.0 {
                    let k = bw / w;
                    em *= k;
                    w *= k;
                    h *= k;
                }
                let x = match align {
                    Some(BlockAlign::Left) => bx,
                    Some(BlockAlign::Right) => bx + (bw - w).max(0.0),
                    None => bx + ((bw - w) / 2.0).max(0.0),
                };
                let ink = if r.error.is_some() { hex("#d93025") } else { c.ink };
                let ascent = r.height as f32 * em;
                draw_rendered(g, &r, x, by + ascent, em, ink);
                h.max(FORMULA_PX * zoom)
            }
            MathBlock::Text { text, style, scale, .. } => {
                let block_scale = scale.unwrap_or(1.0) as f32;
                let (px, fstyle) = match style {
                    TextStyle::H1 => (28.0, FontStyle::BOLD),
                    TextStyle::H2 => (20.0, FontStyle::BOLD),
                    TextStyle::P => (15.0, FontStyle::REGULAR),
                };
                let px = px * zoom * block_scale;
                let font = Font::with_dip("Segoe UI", px, fstyle);
                let fmt = StringFormat { alignment: StringAlignment::Near, line_alignment: StringAlignment::Near, trimming: StringTrimming::None, flags: StringFormatFlags::NO_CLIP };
                let h = px * 1.4;
                g.draw_string(text, &font, Brush::solid(c.ink), Rect::new(bx, by, bx + bw, by + h), &fmt);
                h
            }
            MathBlock::Graph { spec, .. } => {
                let h = spec.height as f32 * zoom;
                draw_graph(g, Rect::new(bx, by, bx + bw, by + h), spec, c);
                h
            }
        }
    }
}

fn block_frame(block: &MathBlock) -> Option<doc::BlockFrame> {
    match block {
        MathBlock::Formula { frame, .. } => *frame,
        MathBlock::Graph { frame, .. } => *frame,
        MathBlock::Text { frame, .. } => *frame,
    }
}

/// Draws a function graph in `rect` (device DIP), a port of `mathGraph.ts` `drawGraph`: resolve the Y range,
/// the grid, the axes with numeric labels, then each curve (breaking the path on non-finite values and large
/// jumps). The sampling/scaling/tick maths is the shared engine's; the painting is the desktop's.
fn draw_graph(g: &Graphics<'_>, rect: Rect, spec: &GraphSpec, c: &Colors) {
    let (css_w, css_h) = (rect.right - rect.left, rect.bottom - rect.top);
    if css_w <= 1.0 || css_h <= 1.0 || !(spec.xmax > spec.xmin) {
        return;
    }
    // A faint card behind the plot.
    g.fill_rectangle(Brush::solid(c.sheet), rect);

    let compiled: Vec<(kubuno_office_math_core::engine::expr::Compiled, Color)> =
        spec.fns.iter().map(|f| (compile(&f.expr), css_color(&f.color).unwrap_or(hex("#1a73e8")))).collect();

    // Resolve the Y range (auto-scale from samples when not fixed).
    let (mut ymin, mut ymax) = (spec.ymin, spec.ymax);
    if ymin.is_none() || ymax.is_none() {
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        let n = (css_w as i32).max(64);
        for (cc, _) in &compiled {
            if cc.error.is_some() {
                continue;
            }
            for i in 0..=n {
                let yv = cc.eval(spec.xmin + (spec.xmax - spec.xmin) * i as f64 / n as f64);
                if yv.is_finite() {
                    lo = lo.min(yv);
                    hi = hi.max(yv);
                }
            }
        }
        if !lo.is_finite() || !hi.is_finite() || lo == hi {
            lo = -1.0;
            hi = 1.0;
        }
        let pad = (hi - lo) * 0.1;
        let pad = if pad == 0.0 { 1.0 } else { pad };
        ymin = Some(ymin.unwrap_or(lo - pad));
        ymax = Some(ymax.unwrap_or(hi + pad));
    }
    let (mut ymin, mut ymax) = (ymin.unwrap(), ymax.unwrap());
    if !(ymax > ymin) {
        ymin -= 1.0;
        ymax += 1.0;
    }

    let (l, t) = (rect.left, rect.top);
    let px = |x: f64| l + ((x - spec.xmin) / (spec.xmax - spec.xmin) * css_w as f64) as f32;
    let py = |yv: f64| t + (css_h as f64 - (yv - ymin) / (ymax - ymin) * css_h as f64) as f32;
    let clamp = |v: f32, a: f32, b: f32| v.max(a).min(b);

    let x_step = nice_step(spec.xmin, spec.xmax, css_w as f64 / 72.0);
    let y_step = nice_step(ymin, ymax, css_h as f64 / 56.0);

    // Grid.
    if spec.grid {
        let grid_pen = Pen::new(if c_is_dark(c) { hex("#35363a") } else { hex("#eceff1") }, 1.0);
        let mut x = (spec.xmin / x_step).ceil() * x_step;
        while x <= spec.xmax {
            let xx = px(x).round() + 0.5;
            g.draw_line(&grid_pen, PointF::new(xx, t), PointF::new(xx, t + css_h));
            x += x_step;
        }
        let mut yv = (ymin / y_step).ceil() * y_step;
        while yv <= ymax {
            let yy = py(yv).round() + 0.5;
            g.draw_line(&grid_pen, PointF::new(l, yy), PointF::new(l + css_w, yy));
            yv += y_step;
        }
    }
    // Axes.
    let axis_pen = Pen::new(hex("#9aa0a6"), 1.2);
    let ax0 = clamp(py(0.0), t, t + css_h);
    let ay0 = clamp(px(0.0), l, l + css_w);
    g.draw_line(&axis_pen, PointF::new(l, ax0.round() + 0.5), PointF::new(l + css_w, ax0.round() + 0.5));
    g.draw_line(&axis_pen, PointF::new(ay0.round() + 0.5, t), PointF::new(ay0.round() + 0.5, t + css_h));

    // Axis numbers.
    if spec.axis_numbers {
        let font = Font::with_dip("Segoe UI", 10.0, FontStyle::REGULAR);
        let brush = || Brush::solid(c.text_secondary);
        let fmt_c = StringFormat { alignment: StringAlignment::Center, line_alignment: StringAlignment::Near, trimming: StringTrimming::None, flags: StringFormatFlags::NO_WRAP | StringFormatFlags::NO_CLIP };
        let label_y = clamp(ax0 + 3.0, t + 2.0, t + css_h - 12.0);
        let mut x = (spec.xmin / x_step).ceil() * x_step;
        while x <= spec.xmax {
            if x.abs() >= x_step / 2.0 {
                let cx = clamp(px(x), l + 12.0, l + css_w - 12.0);
                g.draw_string(&fmt_tick(x, x_step), &font, brush(), Rect::new(cx - 20.0, label_y, cx + 20.0, label_y + 12.0), &fmt_c);
            }
            x += x_step;
        }
        let fmt_l = StringFormat { alignment: StringAlignment::Near, line_alignment: StringAlignment::Center, trimming: StringTrimming::None, flags: StringFormatFlags::NO_WRAP | StringFormatFlags::NO_CLIP };
        let label_x = clamp(ay0 + 4.0, l + 2.0, l + css_w - 28.0);
        let mut yv = (ymin / y_step).ceil() * y_step;
        while yv <= ymax {
            if yv.abs() >= y_step / 2.0 {
                let cy = clamp(py(yv), t + 8.0, t + css_h - 8.0);
                g.draw_string(&fmt_tick(yv, y_step), &font, brush(), Rect::new(label_x, cy - 8.0, label_x + 40.0, cy + 8.0), &fmt_l);
            }
            yv += y_step;
        }
    }

    // Curves.
    let jump = (ymax - ymin) * 4.0;
    let st = g.save();
    g.intersect_clip(rect);
    for (cc, color) in &compiled {
        if cc.error.is_some() {
            continue;
        }
        let pen = Pen::new(*color, 2.0);
        let steps = css_w as i32;
        let (mut pen_down, mut last_y, mut last_pt) = (false, f64::NAN, PointF::new(0.0, 0.0));
        for i in 0..=steps {
            let x = spec.xmin + (spec.xmax - spec.xmin) * i as f64 / steps as f64;
            let yv = cc.eval(x);
            if !yv.is_finite() {
                pen_down = false;
                last_y = f64::NAN;
                continue;
            }
            if pen_down && (yv - last_y).abs() > jump {
                pen_down = false;
            }
            let clamped = yv.max(ymin - jump).min(ymax + jump);
            let pt = PointF::new(px(x), py(clamped));
            if pen_down {
                g.draw_line(&pen, last_pt, pt);
            }
            pen_down = true;
            last_pt = pt;
            last_y = yv;
        }
    }
    g.restore(st);
}

fn c_is_dark(c: &Colors) -> bool {
    // The viewport is near-black in the dark palette.
    c.viewport == hex("#202124")
}
