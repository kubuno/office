//! `EquationView` — a LaTeX formula drawn as the web draws it with KaTeX (display mode), through the math core
//! (`kubuno-office-math-core`): the equation dialog's live preview (`SA:8352`, `renderKatex(editingEq.latex)`
//! in a light-grey box). The formula is centred and shrunk to fit when it is wider than the box.

use kubuno_desktop::ui::graphics::{Brush, Color, Pen};
use kubuno_desktop::ui::{Canvas, Rect, Size};
use kubuno_desktop::views::component::{Component, Control, ControlCore, PaintEventCx};

use crate::controls::grid::equations::{draw_rendered, rendered};

/// The preview's font size (px): KaTeX at the dialog's 14 px × the `.katex` 1.21 em.
const PREVIEW_FONT_PX: f32 = 18.0;

/// A LaTeX formula (see the module doc).
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Spreadsheets")]
#[toolbox(icon = "sigma")]
pub struct EquationView {
    base: ControlCore,
    /// The LaTeX source.
    #[property(bindable)]
    #[category("Appearance")]
    pub latex: String,
}

#[allow(clippy::derivable_impls)]
impl Default for EquationView {
    fn default() -> Self {
        Self { base: ControlCore::default(), latex: String::new() }
    }
}

impl Control for EquationView {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, proposed: Size) -> Size {
        Size { width: proposed.width.max(120.0), height: 64.0 }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        let b = e.clip_rectangle;
        let g = e.graphics;
        // `border border-border rounded p-4 bg-[#fafafa]`
        g.fill_rounded_rectangle(Brush::solid(Color::from_hex("#fafafa").unwrap_or(Color::WHITE)), b, 4.0);
        g.draw_rounded_rectangle(&Pen::new(Color::from_hex("#e2e4e6").unwrap_or(Color::GRAY), 1.0), Rect::new(b.left + 0.5, b.top + 0.5, b.right - 0.5, b.bottom - 0.5), 4.0);
        let src = if self.latex.is_empty() && !self.design_mode() { "\\," } else if self.latex.is_empty() { "x = \\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}" } else { self.latex.as_str() };
        let r = rendered(src);
        let em = PREVIEW_FONT_PX * kubuno_office_math_core::KATEX_EM as f32;
        let (w, h) = (r.width as f32 * em, (r.height + r.depth) as f32 * em);
        let (aw, ah) = ((b.right - b.left - 32.0).max(1.0), (b.bottom - b.top - 32.0).max(1.0));
        let k = if w > 0.0 && h > 0.0 { (aw / w).min(ah / h).min(1.0) } else { 1.0 };
        let em = em * k;
        let (w, h) = (w * k, h * k);
        let x = (b.left + b.right - w) / 2.0;
        let top = (b.top + b.bottom - h) / 2.0;
        let st = g.save();
        g.intersect_clip(b);
        let ink = if r.error.is_some() { Color::from_hex("#d93025").unwrap_or(Color::BLACK) } else { Color::BLACK };
        draw_rendered(g, &r, x, top + r.height as f32 * em, em, ink);
        g.restore(st);
        e.raise(self, "OnPaint");
    }
}
