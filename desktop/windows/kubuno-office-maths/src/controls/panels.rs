//! The dockable side panels and the structures bar of the Maths workbench, matching the web editor's layout
//! (`MathsApp.tsx`: the Objets / Symboles / Propriétés docks and the WYSIWYG structures toolbar). Each is a
//! custom-drawn control that raises a [`PanelCmdEventArgs`] the window routes to the canvas:
//!
//! - [`ObjectsPanel`] — insert a block (`insert:formula` / `graph` / `h1` / `h2` / `p`);
//! - [`StructuresBar`] — append a LaTeX structure to the selected formula (`struct:<latex>`), each cell a live
//!   KaTeX preview (the structures bar can paint glyph outlines, unlike a ribbon gallery cell);
//! - [`SymbolsPanel`] — append a symbol (`sym:<latex>`), a grid by category;
//! - [`PropertiesPanel`] — shows the selected block's properties (display-only; editing is on the ribbon).

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::graphics::{Brush, Color, Font, FontStyle, Pen, StringAlignment, StringFormat, StringFormatFlags, StringTrimming};
use kubuno_desktop::ui::{Canvas, Rect, Size};
use kubuno_desktop::views::component::{Control, ControlCore, EventCx, PaintEventCx};
use kubuno_desktop::views::events::{Event, MouseButton};

use kubuno_office_math_core::KATEX_EM;

use crate::controls::equations::{draw_rendered, rendered, rendered_size};

/// A command raised by a panel: `"insert:formula"`, `"struct:\\frac{…}{…}"`, `"sym:\\pi"`, …
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct PanelCmdEventArgs {
    pub command: String,
}

fn hex(s: &str) -> Color {
    Color::from_hex(s).unwrap_or(Color::BLACK)
}

/// Panel colours, light and dark.
struct PColors {
    bg: Color,
    row: Color,
    ink: Color,
    secondary: Color,
    border: Color,
}
impl PColors {
    fn of(dark: bool) -> Self {
        if dark {
            PColors { bg: hex("#292a2d"), row: hex("#35363a"), ink: hex("#e8eaed"), secondary: hex("#9aa0a6"), border: hex("#5f6368") }
        } else {
            PColors { bg: hex("#ffffff"), row: hex("#f8f9fa"), ink: hex("#202124"), secondary: hex("#5f6368"), border: hex("#e0e0e0") }
        }
    }
}

fn label_font(px: f32, bold: bool) -> Font {
    Font::with_dip("Segoe UI", px, if bold { FontStyle::BOLD } else { FontStyle::REGULAR })
}
fn near_fmt() -> StringFormat {
    StringFormat { alignment: StringAlignment::Near, line_alignment: StringAlignment::Center, trimming: StringTrimming::EllipsisCharacter, flags: StringFormatFlags::NO_WRAP }
}
fn center_fmt() -> StringFormat {
    StringFormat { alignment: StringAlignment::Center, line_alignment: StringAlignment::Center, trimming: StringTrimming::None, flags: StringFormatFlags::NO_WRAP }
}

// ── Objets ─────────────────────────────────────────────────────────────────────────

const OBJECTS: &[(&str, &str)] = &[
    ("insert:formula", "Σ   Formule"),
    ("insert:graph", "〰  Graphe"),
    ("insert:h1", "H1  Titre"),
    ("insert:h2", "H2  Sous-titre"),
    ("insert:p", "T   Paragraphe"),
];
const OBJ_ROW_H: f32 = 44.0;

/// The Objets dock: click a row to insert a block.
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Maths")]
#[toolbox(icon = "shapes")]
#[default_event("Command")]
pub struct ObjectsPanel {
    base: ControlCore,
    #[event]
    #[category("Action")]
    pub command: Event<PanelCmdEventArgs>,
    dark: bool,
    bounds: Rect,
}
impl Default for ObjectsPanel {
    fn default() -> Self {
        Self { base: ControlCore::default(), command: Event::default(), dark: false, bounds: Rect::new(0.0, 0.0, 210.0, 280.0) }
    }
}
impl ObjectsPanel {
    pub fn set_dark(&mut self, d: bool) {
        self.dark = d;
        self.invalidate();
    }
}
impl Control for ObjectsPanel {
    fn get_preferred_size(&self, _c: &dyn Canvas, _p: Size) -> Size {
        Size { width: 210.0, height: 280.0 }
    }
    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        let g = e.graphics;
        let b = e.clip_rectangle;
        self.bounds = b;
        let c = PColors::of(self.dark);
        g.fill_rectangle(Brush::solid(c.bg), b);
        let font = label_font(14.0, false);
        for (i, (_, label)) in OBJECTS.iter().enumerate() {
            let y = b.top + 8.0 + i as f32 * OBJ_ROW_H;
            let r = Rect::new(b.left + 8.0, y, b.right - 8.0, y + OBJ_ROW_H - 6.0);
            g.fill_rounded_rectangle(Brush::solid(c.row), r, 6.0);
            g.draw_rounded_rectangle(&Pen::new(c.border, 1.0), r, 6.0);
            g.draw_string(label, &font, Brush::solid(c.ink), Rect::new(r.left + 12.0, r.top, r.right - 8.0, r.bottom), &near_fmt());
        }
        e.raise(self, "OnPaint");
    }
    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        if e.args().button == MouseButton::Left {
            let idx = (((e.args().y - self.bounds.top - 8.0) / OBJ_ROW_H).floor()) as i32;
            if idx >= 0 && (idx as usize) < OBJECTS.len() {
                self.raise_command(PanelCmdEventArgs { command: OBJECTS[idx as usize].0.to_string() });
            }
        }
        e.raise(&*self, "OnMouseDown");
    }
}

// ── Structures bar ───────────────────────────────────────────────────────────────────

const STRUCTS: &[&str] = &[
    "\\frac{\\square}{\\square}",
    "\\square^{\\square}",
    "\\square_{\\square}",
    "\\sqrt{\\square}",
    "\\sqrt[\\square]{\\square}",
    "\\left(\\square\\right)",
    "\\left|\\square\\right|",
    "\\sum_{\\square}^{\\square}",
    "\\int_{\\square}^{\\square}",
    "\\lim_{\\square\\to\\square}",
    "\\begin{pmatrix}\\square&\\square\\\\\\square&\\square\\end{pmatrix}",
    "\\vec{\\square}",
];
const CELL_W: f32 = 46.0;

/// The WYSIWYG structures toolbar: each cell a live KaTeX preview; click appends it to the selected formula.
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Maths")]
#[toolbox(icon = "sigma")]
#[default_event("Command")]
pub struct StructuresBar {
    base: ControlCore,
    #[event]
    #[category("Action")]
    pub command: Event<PanelCmdEventArgs>,
    dark: bool,
    bounds: Rect,
}
impl Default for StructuresBar {
    fn default() -> Self {
        Self { base: ControlCore::default(), command: Event::default(), dark: false, bounds: Rect::new(0.0, 0.0, 560.0, 40.0) }
    }
}
impl StructuresBar {
    pub fn set_dark(&mut self, d: bool) {
        self.dark = d;
        self.invalidate();
    }
}
impl Control for StructuresBar {
    fn get_preferred_size(&self, _c: &dyn Canvas, _p: Size) -> Size {
        Size { width: 560.0, height: 40.0 }
    }
    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        let g = e.graphics;
        let b = e.clip_rectangle;
        self.bounds = b;
        let c = PColors::of(self.dark);
        g.fill_rectangle(Brush::solid(c.bg), b);
        for (i, src) in STRUCTS.iter().enumerate() {
            let x = b.left + 6.0 + i as f32 * CELL_W;
            let cell = Rect::new(x, b.top + 4.0, x + CELL_W - 4.0, b.bottom - 4.0);
            let r = rendered(src);
            let em = 13.0 * KATEX_EM as f32;
            let (mut w, mut h) = rendered_size(&r, em);
            let (aw, ah) = (cell.right - cell.left - 8.0, cell.bottom - cell.top - 6.0);
            let k = if w > 0.0 && h > 0.0 { (aw / w).min(ah / h).min(1.0) } else { 1.0 };
            let em = em * k;
            w *= k;
            h *= k;
            let px = cell.left + (cell.right - cell.left - w) / 2.0;
            let top = cell.top + (cell.bottom - cell.top - h) / 2.0;
            let st = g.save();
            g.intersect_clip(cell);
            draw_rendered(g, &r, px, top + r.height as f32 * em, em, c.ink);
            g.restore(st);
        }
        e.raise(self, "OnPaint");
    }
    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        if e.args().button == MouseButton::Left {
            let idx = (((e.args().x - self.bounds.left - 6.0) / CELL_W).floor()) as i32;
            if idx >= 0 && (idx as usize) < STRUCTS.len() {
                self.raise_command(PanelCmdEventArgs { command: format!("struct:{}", STRUCTS[idx as usize]) });
            }
        }
        e.raise(&*self, "OnMouseDown");
    }
}

// ── Symboles ───────────────────────────────────────────────────────────────────────

/// (display, latex), grouped into rows by category (greek, operators, relations, arrows/sets).
const SYMBOLS: &[(&str, &str)] = &[
    ("α", "\\alpha"), ("β", "\\beta"), ("γ", "\\gamma"), ("δ", "\\delta"), ("ε", "\\epsilon"), ("θ", "\\theta"),
    ("λ", "\\lambda"), ("μ", "\\mu"), ("π", "\\pi"), ("ρ", "\\rho"), ("σ", "\\sigma"), ("φ", "\\phi"),
    ("ω", "\\omega"), ("Γ", "\\Gamma"), ("Δ", "\\Delta"), ("Σ", "\\Sigma"), ("Π", "\\Pi"), ("Ω", "\\Omega"),
    ("×", "\\times"), ("÷", "\\div"), ("±", "\\pm"), ("∓", "\\mp"), ("⋅", "\\cdot"), ("∘", "\\circ"),
    ("∇", "\\nabla"), ("∂", "\\partial"), ("∞", "\\infty"), ("∀", "\\forall"), ("∃", "\\exists"), ("¬", "\\neg"),
    ("≤", "\\leq"), ("≥", "\\geq"), ("≠", "\\neq"), ("≈", "\\approx"), ("≡", "\\equiv"), ("∝", "\\propto"),
    ("∈", "\\in"), ("∉", "\\notin"), ("⊂", "\\subset"), ("∪", "\\cup"), ("∩", "\\cap"), ("∅", "\\emptyset"),
    ("→", "\\to"), ("⇒", "\\Rightarrow"), ("⇔", "\\Leftrightarrow"), ("↦", "\\mapsto"), ("⊥", "\\perp"), ("∠", "\\angle"),
];
const SYM_COLS: usize = 6;
const SYM_CELL: f32 = 34.0;

/// The Symboles dock: a grid of symbols; click appends the symbol's LaTeX to the selected formula.
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Maths")]
#[toolbox(icon = "omega")]
#[default_event("Command")]
pub struct SymbolsPanel {
    base: ControlCore,
    #[event]
    #[category("Action")]
    pub command: Event<PanelCmdEventArgs>,
    dark: bool,
    bounds: Rect,
}
impl Default for SymbolsPanel {
    fn default() -> Self {
        Self { base: ControlCore::default(), command: Event::default(), dark: false, bounds: Rect::new(0.0, 0.0, 210.0, 300.0) }
    }
}
impl SymbolsPanel {
    pub fn set_dark(&mut self, d: bool) {
        self.dark = d;
        self.invalidate();
    }
}
impl Control for SymbolsPanel {
    fn get_preferred_size(&self, _c: &dyn Canvas, _p: Size) -> Size {
        Size { width: 210.0, height: 300.0 }
    }
    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        let g = e.graphics;
        let b = e.clip_rectangle;
        self.bounds = b;
        let c = PColors::of(self.dark);
        g.fill_rectangle(Brush::solid(c.bg), b);
        let font = label_font(16.0, false);
        for (i, (disp, _)) in SYMBOLS.iter().enumerate() {
            let col = i % SYM_COLS;
            let row = i / SYM_COLS;
            let x = b.left + 6.0 + col as f32 * SYM_CELL;
            let y = b.top + 6.0 + row as f32 * SYM_CELL;
            let cell = Rect::new(x, y, x + SYM_CELL - 3.0, y + SYM_CELL - 3.0);
            g.fill_rounded_rectangle(Brush::solid(c.row), cell, 4.0);
            g.draw_rounded_rectangle(&Pen::new(c.border, 1.0), cell, 4.0);
            g.draw_string(disp, &font, Brush::solid(c.ink), cell, &center_fmt());
        }
        e.raise(self, "OnPaint");
    }
    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        if e.args().button == MouseButton::Left {
            let col = (((e.args().x - self.bounds.left - 6.0) / SYM_CELL).floor()) as i32;
            let row = (((e.args().y - self.bounds.top - 6.0) / SYM_CELL).floor()) as i32;
            if col >= 0 && (col as usize) < SYM_COLS && row >= 0 {
                let idx = row as usize * SYM_COLS + col as usize;
                if idx < SYMBOLS.len() {
                    self.raise_command(PanelCmdEventArgs { command: format!("sym:{}", SYMBOLS[idx].1) });
                }
            }
        }
        e.raise(&*self, "OnMouseDown");
    }
}

// ── Propriétés ──────────────────────────────────────────────────────────────────────

/// The Propriétés dock: shows the selected block's properties (display-only; editing is on the ribbon).
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Maths")]
#[toolbox(icon = "sliders")]
pub struct PropertiesPanel {
    base: ControlCore,
    dark: bool,
    /// The lines to show (built by the window from the selection).
    lines: Vec<String>,
}
impl Default for PropertiesPanel {
    fn default() -> Self {
        Self { base: ControlCore::default(), dark: false, lines: vec!["Sélectionnez un bloc sur la feuille.".to_string()] }
    }
}
impl PropertiesPanel {
    pub fn set_dark(&mut self, d: bool) {
        self.dark = d;
        self.invalidate();
    }
    pub fn set_lines(&mut self, lines: Vec<String>) {
        self.lines = if lines.is_empty() { vec!["Sélectionnez un bloc sur la feuille.".to_string()] } else { lines };
        self.invalidate();
    }
}
impl Control for PropertiesPanel {
    fn get_preferred_size(&self, _c: &dyn Canvas, _p: Size) -> Size {
        Size { width: 240.0, height: 300.0 }
    }
    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        let g = e.graphics;
        let b = e.clip_rectangle;
        let c = PColors::of(self.dark);
        g.fill_rectangle(Brush::solid(c.bg), b);
        for (i, line) in self.lines.iter().enumerate() {
            let y = b.top + 12.0 + i as f32 * 24.0;
            let bold = i == 0 && self.lines.len() > 1;
            let brush = if bold { Brush::solid(c.ink) } else if i == 0 { Brush::solid(c.secondary) } else { Brush::solid(c.ink) };
            g.draw_string(line, &label_font(13.0, bold), brush, Rect::new(b.left + 12.0, y, b.right - 10.0, y + 22.0), &near_fmt());
        }
        e.raise(self, "OnPaint");
    }
}
