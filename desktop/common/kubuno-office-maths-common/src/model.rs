//! The open formula document.
//!
//! A Maths formula's whole content is a single `latex` string (the `.kbmath` Drive file's `latex` field).
//! The shared engine ([`kubuno_office_math_core::engine::doc`]) reads it into pages of freely placed blocks
//! (formula / graph / text) and writes it back byte-identically to the web. This type is the thin document the
//! app holds: the server/file identity plus the parsed pages.

use kubuno_office_math_core::engine::doc::{self, DocPage};

/// An open formula document.
#[derive(Debug, Clone)]
pub struct MathsDoc {
    /// The server formula's id, when it came from the server (`--doc <id>`).
    pub id: Option<String>,
    /// The display name (the formula's name, or the file stem).
    pub name: String,
    /// The raw stored content (plain LaTeX, or a `%%KBMATH-DOC%%` / multipage envelope).
    pub latex: String,
    /// The content parsed into pages of blocks (`doc::parse_doc`).
    pub pages: Vec<DocPage>,
    /// Starred on the server.
    pub starred: bool,
}

impl MathsDoc {
    /// Builds a document from its stored `latex` string and a display `name`.
    pub fn from_latex(name: impl Into<String>, latex: impl Into<String>) -> Self {
        let latex = latex.into();
        let pages = doc::parse_doc(Some(&latex));
        MathsDoc { id: None, name: name.into(), latex, pages, starred: false }
    }

    /// Re-parses `latex` into the document (after a server reload or a local open).
    pub fn set_latex(&mut self, latex: impl Into<String>) {
        self.latex = latex.into();
        self.pages = doc::parse_doc(Some(&self.latex));
    }

    /// The content serialised back to the stored form (what a save sends). Re-derived from the pages so it
    /// stays the smallest faithful form, exactly as the web writes it.
    pub fn to_latex(&self) -> String {
        doc::serialize_doc(&self.pages)
    }

    /// A blank single-page document (a new formula).
    pub fn blank() -> Self {
        Self::from_latex("Nouvelle formule", "")
    }

    /// The number of blocks across all pages (the status bar / Backstage info).
    pub fn block_count(&self) -> usize {
        self.pages.iter().map(|p| p.blocks.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_plain_latex() {
        let d = MathsDoc::from_latex("f", "x^{2}");
        assert_eq!(d.pages.len(), 1);
        assert_eq!(d.block_count(), 1);
        // A trivial single formula round-trips back to bare LaTeX.
        assert_eq!(d.to_latex(), "x^{2}");
    }

    #[test]
    fn blank_is_one_empty_formula() {
        let d = MathsDoc::blank();
        assert_eq!(d.block_count(), 1);
        assert_eq!(d.to_latex(), "");
    }
}
