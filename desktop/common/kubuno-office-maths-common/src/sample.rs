//! The offline sample formula, shown in the designer and with `--sample` when no server formula is open.

use kubuno_office_math_core::engine::doc::{self, DocPage, MathBlock, TextStyle};
use kubuno_office_math_core::engine::graph::default_graph_spec;

use crate::model::MathsDoc;

/// A one-page document showing a title, the quadratic formula (numbered) and a graph of sin(x) - the three
/// block kinds the editor renders.
pub fn document() -> MathsDoc {
    let pages = vec![DocPage {
        name: "Page 1".to_string(),
        blocks: vec![
            MathBlock::Text { text: "Formule quadratique".to_string(), style: TextStyle::H1, frame: None, scale: None },
            MathBlock::Formula {
                latex: "x = \\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}".to_string(),
                align: None,
                numbered: Some(true),
                frame: None,
                scale: None,
            },
            MathBlock::Graph { spec: default_graph_spec(0), frame: None },
        ],
        format: None,
        orientation: None,
    }];
    let latex = doc::serialize_doc(&pages);
    MathsDoc { id: None, name: "Exemple".to_string(), latex, pages, starred: false }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_sample_has_three_blocks() {
        let d = super::document();
        assert_eq!(d.block_count(), 3);
    }
}
