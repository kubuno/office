//! Insertion › Équation (`equationRender`, `SA:7014`): the web's predefined equations inserted as they are, « Insérer
//! une nouvelle équation » inserting an empty one and opening the equation dialog, and a double click on an
//! equation opening it (`setEditingEq`).

use crate::controls::grid::equations::EquationEventArgs;
use crate::views::equation_dialog::EquationDialog;

use super::SpreadsheetWindow;

/// `PREDEFINED_EQUATIONS` (`SA:7003`): gallery value → LaTeX.
pub const PRESETS: [(&str, &str); 9] = [
    ("circle", r"A = \pi r^2"),
    ("binomial", r"(x+a)^n = \sum_{k=0}^{n} \binom{n}{k} x^k a^{n-k}"),
    ("expansion", r"(1+x)^n = 1 + \frac{nx}{1!} + \frac{n(n-1)x^2}{2!} + \cdots"),
    ("fourier", r"f(x) = a_0 + \sum_{n=1}^{\infty} \left( a_n \cos \frac{n\pi x}{L} + b_n \sin \frac{n\pi x}{L} \right)"),
    ("pythagore", r"a^2 + b^2 = c^2"),
    ("quadratic", r"x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}"),
    ("taylor", r"e^x = 1 + \frac{x}{1!} + \frac{x^2}{2!} + \frac{x^3}{3!} + \cdots, \quad -\infty < x < \infty"),
    ("trig1", r"\sin\alpha \pm \sin\beta = 2 \sin\tfrac{1}{2}(\alpha \pm \beta) \cos\tfrac{1}{2}(\alpha \mp \beta)"),
    ("trig2", r"\cos\alpha + \cos\beta = 2 \cos\tfrac{1}{2}(\alpha + \beta) \cos\tfrac{1}{2}(\alpha - \beta)"),
];

impl SpreadsheetWindow {
    /// A choice of the equation gallery.
    pub(super) fn equation_gallery_click(&mut self, e: &kubuno_desktop::prelude::TextChangedEventArgs) {
        self.grid.with(|g| g.commit_edit(None));
        if e.new == "new" {
            let Some(id) = self.grid.with(|g| g.insert_equation("")).flatten() else { return };
            // Once the gallery's drop-down has closed (`later`).
            self.later(move |w| w.equation_edit(&id, ""));
        } else if let Some((_, latex)) = PRESETS.iter().find(|(k, _)| *k == e.new) {
            self.grid.with(|g| g.insert_equation(latex));
        }
    }

    /// A double click on an equation.
    pub(super) fn grid_equation_edit(&mut self, e: &EquationEventArgs) {
        let (id, latex) = (e.id.clone(), e.latex.clone());
        self.equation_edit(&id, &latex);
    }

    fn equation_edit(&mut self, id: &str, latex: &str) {
        if let Some(new) = EquationDialog::ask(self, latex) {
            self.grid.with(|g| g.set_equation_latex(id, &new));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PRESETS;

    #[test]
    fn every_preset_lays_out() {
        for (k, latex) in PRESETS {
            let r = kubuno_office_math_core::render(latex);
            assert!(r.error.is_none() && r.width > 1.0, "{k}");
        }
    }
}
