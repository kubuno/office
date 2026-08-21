//! A small builder for assembling a ProseMirror document from a project's data.
//!
//! The registers hold everything a PMI report should say; what was missing was
//! turning that into something a person outside Kubuno can be handed. The output
//! here is a real editable document — the same node tree the editor loads and the
//! DOCX/ODT/PDF exporters already read — not a dead PDF. So a produced charter can
//! be edited afterwards like any other document.
//!
//! Reusable on purpose: every producer (charter, status report, closure report,
//! risk register…) builds its body with these helpers, and the handler only has
//! to drop the result in Drive.
use crate::converters::types::{PmMark, PmNode};
use serde_json::json;

fn bold() -> PmMark { PmMark { mark_type: "bold".into(), attrs: None } }
fn italic() -> PmMark { PmMark { mark_type: "italic".into(), attrs: None } }

/// Accumulates document blocks; `finish()` wraps them in the `{version, content}`
/// envelope `create_document_content_file` expects.
#[derive(Default)]
pub struct DocBuilder {
    blocks: Vec<PmNode>,
}

impl DocBuilder {
    pub fn new() -> Self { Self::default() }

    /// A document title — the biggest heading, once at the top.
    pub fn title(&mut self, text: &str) -> &mut Self {
        self.blocks.push(PmNode::heading(1, vec![PmNode::text(text, vec![])]));
        self
    }

    pub fn heading(&mut self, level: u8, text: &str) -> &mut Self {
        self.blocks.push(PmNode::heading(level, vec![PmNode::text(text, vec![])]));
        self
    }

    /// A plain paragraph. Empty text is skipped: a report should not carry a blank
    /// line where a field was left unfilled.
    pub fn para(&mut self, text: &str) -> &mut Self {
        if !text.trim().is_empty() {
            self.blocks.push(PmNode::paragraph(vec![PmNode::text(text, vec![])]));
        }
        self
    }

    /// A "Label: value" line, the label in bold. Skipped when the value is empty,
    /// so an unset field leaves no trace rather than an orphan label.
    pub fn field(&mut self, label: &str, value: &str) -> &mut Self {
        if !value.trim().is_empty() {
            self.blocks.push(PmNode::paragraph(vec![
                PmNode::text(format!("{label} : "), vec![bold()]),
                PmNode::text(value, vec![]),
            ]));
        }
        self
    }

    /// A prose field under its own small heading. Skipped when empty.
    pub fn section(&mut self, heading: &str, text: &str) -> &mut Self {
        if !text.trim().is_empty() {
            self.heading(3, heading);
            for line in text.split('\n').filter(|l| !l.trim().is_empty()) {
                self.blocks.push(PmNode::paragraph(vec![PmNode::text(line, vec![])]));
            }
        }
        self
    }

    /// A muted note — used for "nothing to report" lines, so the absence of a row
    /// is stated rather than left as a gap the reader has to interpret.
    pub fn note(&mut self, text: &str) -> &mut Self {
        self.blocks.push(PmNode::paragraph(vec![PmNode::text(text, vec![italic()])]));
        self
    }

    pub fn bullets(&mut self, items: impl IntoIterator<Item = String>) -> &mut Self {
        let list: Vec<PmNode> = items.into_iter()
            .map(|s| PmNode::list_item(vec![PmNode::paragraph(vec![PmNode::text(s, vec![])])]))
            .collect();
        if !list.is_empty() { self.blocks.push(PmNode::bullet_list(list)); }
        self
    }

    /// A table with a bold header row. Rows are cell strings; ragged rows are
    /// padded so the table stays rectangular, which the exporters require.
    pub fn table(&mut self, header: &[&str], rows: &[Vec<String>]) -> &mut Self {
        if header.is_empty() { return self; }
        let cols = header.len();
        let cell = |text: &str, head: bool| -> PmNode {
            let marks = if head { vec![bold()] } else { vec![] };
            let para = if text.trim().is_empty() {
                PmNode::paragraph(vec![])
            } else {
                PmNode::paragraph(vec![PmNode::text(text, marks)])
            };
            PmNode {
                node_type: "tableCell".into(),
                attrs: Some(json!({
                    "colspan": 1, "rowspan": 1,
                    "cellBorders": { "t": null, "b": null, "l": null, "r": null },
                })),
                content: Some(vec![para]),
                marks: None, text: None,
            }
        };
        let row_node = |cells: Vec<PmNode>| PmNode {
            node_type: "tableRow".into(), attrs: None,
            content: Some(cells), marks: None, text: None,
        };

        let mut trows = vec![row_node(header.iter().map(|h| cell(h, true)).collect())];
        for r in rows {
            let mut cells: Vec<PmNode> = r.iter().take(cols).map(|c| cell(c, false)).collect();
            while cells.len() < cols { cells.push(cell("", false)); }
            trows.push(row_node(cells));
        }
        self.blocks.push(PmNode {
            node_type: "table".into(), attrs: None,
            content: Some(trows), marks: None, text: None,
        });
        self
    }

    pub fn spacer(&mut self) -> &mut Self {
        self.blocks.push(PmNode::paragraph(vec![]));
        self
    }

    /// The `{version, content: {type:doc, …}}` envelope, as
    /// `create_document_content_file` and the editor expect.
    pub fn finish(self) -> serde_json::Value {
        let doc = PmNode::doc(if self.blocks.is_empty() {
            vec![PmNode::paragraph(vec![])]
        } else {
            self.blocks
        });
        json!({ "version": 1, "content": doc })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_empty_fields_and_pads_ragged_rows() {
        let mut b = DocBuilder::new();
        b.title("T").field("Sponsor", "").field("PM", "Alice")
         .table(&["A", "B", "C"], &[vec!["1".into(), "2".into()]]);
        let v = b.finish();
        let content = v["content"]["content"].as_array().unwrap();
        // title + the PM field only (empty sponsor dropped) + table = 3 blocks.
        assert_eq!(content.len(), 3);
        // The single data row was padded to three cells.
        let table = &content[2];
        let data_row = &table["content"][1];
        assert_eq!(data_row["content"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn an_empty_builder_still_produces_a_valid_doc() {
        let v = DocBuilder::new().finish();
        assert_eq!(v["content"]["type"], "doc");
        assert_eq!(v["content"]["content"].as_array().unwrap().len(), 1);
    }
}
