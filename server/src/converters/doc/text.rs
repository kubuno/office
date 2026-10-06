//! Turning the decoded character stream into ProseMirror blocks.
//!
//! Word's text is a flat character array; structure comes from control
//! characters. Paragraph marks are 0x0D, cell and row ends are 0x07, and a
//! field is the triple 0x13 instruction 0x14 result 0x15 — the instruction must
//! never reach the document ([MS-DOC] 2.8.25).

use crate::converters::types::PmNode;

/// Word control characters that carry structure rather than text.
pub(crate) mod ch {
    pub(crate) const PARA_END: char = '\r';
    pub(crate) const CELL_END: char = '\u{7}';
    pub(crate) const LINE_BREAK: char = '\u{B}';
    pub(crate) const PAGE_BREAK: char = '\u{C}';
    pub(crate) const COLUMN_BREAK: char = '\u{E}';
    pub(crate) const FIELD_BEGIN: char = '\u{13}';
    pub(crate) const FIELD_SEP: char = '\u{14}';
    pub(crate) const FIELD_END: char = '\u{15}';
    pub(crate) const PICTURE: char = '\u{1}';
    pub(crate) const DRAWING: char = '\u{8}';
    pub(crate) const NOTE_REF: char = '\u{2}';
    pub(crate) const NB_HYPHEN: char = '\u{1E}';
    pub(crate) const SOFT_HYPHEN: char = '\u{1F}';
}

/// One paragraph's text plus the character position it starts at, so the
/// property layers can attach formatting to it later.
pub(crate) struct RawParagraph {
    pub(crate) cp_start: u32,
    pub(crate) text: String,
    /// The paragraph ended with a cell mark rather than a paragraph mark.
    pub(crate) in_table: bool,
}

/// Split the character stream into paragraphs, dropping control characters that
/// are not text and skipping field instructions.
pub(crate) fn split_paragraphs(chars: &[char]) -> Vec<RawParagraph> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut start = 0u32;
    // Depth of nested field instructions currently being skipped.
    let mut in_instruction = 0u32;

    for (i, &c) in chars.iter().enumerate() {
        match c {
            ch::FIELD_BEGIN => in_instruction += 1,
            ch::FIELD_SEP | ch::FIELD_END => in_instruction = in_instruction.saturating_sub(1),
            ch::PARA_END | ch::CELL_END => {
                out.push(RawParagraph {
                    cp_start: start,
                    text: std::mem::take(&mut buf),
                    in_table: c == ch::CELL_END,
                });
                start = i as u32 + 1;
            }
            ch::LINE_BREAK => {
                if in_instruction == 0 {
                    buf.push('\n');
                }
            }
            ch::NB_HYPHEN => {
                if in_instruction == 0 {
                    buf.push('\u{2011}');
                }
            }
            // Structural or placeholder characters with no textual meaning.
            ch::PAGE_BREAK
            | ch::COLUMN_BREAK
            | ch::PICTURE
            | ch::DRAWING
            | ch::NOTE_REF
            | ch::SOFT_HYPHEN => {}
            _ => {
                if in_instruction == 0 && (c == '\t' || !c.is_control()) {
                    buf.push(c);
                }
            }
        }
    }
    if !buf.is_empty() {
        out.push(RawParagraph { cp_start: start, text: buf, in_table: false });
    }
    out
}

/// Build a ProseMirror `doc` from paragraphs carrying no formatting yet.
pub(crate) fn to_pm(paras: &[RawParagraph]) -> PmNode {
    let mut nodes: Vec<PmNode> = Vec::new();
    for p in paras {
        let content = if p.text.is_empty() {
            vec![]
        } else {
            vec![PmNode {
                node_type: "text".into(),
                attrs: None,
                content: None,
                marks: None,
                text: Some(p.text.clone()),
            }]
        };
        nodes.push(PmNode::paragraph(content));
    }
    if nodes.is_empty() {
        nodes.push(PmNode::paragraph(vec![]));
    }
    PmNode::doc(nodes)
}
