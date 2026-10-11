//! Kubuno Office's math typesetting: LaTeX laid out as the web editors render it with KaTeX
//! (`katex.renderToString(latex, { displayMode: true, throwOnError: false, strict: false })`), producing a
//! display list any platform paints.
//!
//! It is a Rust port of KaTeX 0.16 — its lexer, macro expander, parser, HTML builders, delimiter and SVG
//! geometry — run against KaTeX's own data (font metrics, symbol table, macros, extracted by
//! `tools/extract.ts`) and KaTeX's own fonts (embedded). What the browser does with KaTeX's HTML and CSS
//! (inline layout, vertical lists, sizing ratios) is done by [`layout`]. Units are ems of the formula's font
//! size (`.katex`: 1.21 × the container's font size).

pub mod build;
pub mod data;
pub mod engine;
pub mod delim;
pub mod fonts;
pub mod layout;
pub mod lexer;
pub mod options;
pub mod parse;
pub mod sheet;
pub mod stretchy;
pub mod svg;
pub mod tree;

pub use layout::Item;
pub use svg::PathOp;

/// KaTeX's default `errorColor`.
pub const ERROR_COLOR: &str = "#cc0000";

/// A parse error (KaTeX's `ParseError`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError(pub String);

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "KaTeX parse error: {}", self.0)
    }
}

/// A formula laid out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rendered {
    /// The width of `.katex-html`, in ems of the formula's font size.
    pub width: f64,
    /// The height above and depth below the baseline of the tallest `.base` strut.
    pub height: f64,
    pub depth: f64,
    /// The paint operations, origin at the left of the baseline.
    pub items: Vec<Item>,
    /// The parse error, when the source is shown in the error colour instead (`throwOnError: false`).
    pub error: Option<String>,
}

/// The `.katex` font size relative to its container's (`font: normal 1.21em KaTeX_Main`).
pub const KATEX_EM: f64 = 1.21;

/// Renders `latex` in display mode.
pub fn render(latex: &str) -> Rendered {
    match parse::parse(latex) {
        Ok(tree) => {
            let options = options::Options::display();
            let html = build::build_html(&tree, &options);
            let (width, items) = layout::layout(&html);
            Rendered { width, height: html.height(), depth: html.depth(), items, error: None }
        }
        Err(e) => {
            // `<span class="katex-error" style="color:#cc0000">latex</span>`: the source as text.
            Rendered { width: 0.0, height: 0.0, depth: 0.0, items: vec![Item::Text { text: latex.to_string(), x: 0.0, y: 0.0, size: 1.0 / KATEX_EM, color: Some(ERROR_COLOR.into()) }], error: Some(e.to_string()) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_the_presets() {
        for src in ["A = \\pi r^2", "x = \\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}", "\\sum_{k=0}^{n} \\binom{n}{k}", "\\left( \\frac{a}{b} \\right)", "\\begin{pmatrix} a & b \\\\ c & d \\end{pmatrix}", "\\hat{x} + \\vec{v}"] {
            let r = render(src);
            assert!(r.error.is_none(), "{src}: {:?}", r.error);
            assert!(r.width > 0.5, "{src}: width {}", r.width);
            assert!(!r.items.is_empty(), "{src}");
        }
    }
}
