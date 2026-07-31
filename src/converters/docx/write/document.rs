//! Body dispatch: routes each block node to the module that serialises it.

use crate::converters::docx::xml::escape_xml;
use crate::converters::types::PmNode;

use super::ctx::ExportCtx;
use super::{drawing, list, paragraph, table, toc};

pub(crate) fn render_body(doc: &PmNode, ctx: &mut ExportCtx) -> String {
    let mut out = String::new();
    let mut last_was_table = false;
    let kids = doc.children();
    let mut i = 0usize;
    while i < kids.len() {
        // A table of contents spans several paragraphs and must be wrapped in a
        // single `TOC` field, so it is matched before the per-node dispatch.
        if let Some((xml, used)) = toc::render_toc_block(&kids[i..], ctx) {
            out.push_str(&xml);
            last_was_table = false;
            i += used.max(1);
            continue;
        }
        let xml = render_block(&kids[i], ctx);
        i += 1;
        if xml.is_empty() {
            continue;
        }
        last_was_table = xml.starts_with("<w:tbl>");
        out.push_str(&xml);
    }
    // A `w:tbl` may not be the last thing in the body: Word expects a paragraph
    // after it (docxattributeoutput.cxx:864) and offers to repair the file
    // otherwise. The same empty paragraph also covers an empty document.
    if out.is_empty() || last_was_table {
        out.push_str("<w:p/>");
    }
    out
}

pub(crate) fn render_block(node: &PmNode, ctx: &mut ExportCtx) -> String {
    match node.node_type.as_str() {
        "paragraph" => paragraph::render_paragraph_with_style(node, "Normal", ctx),
        "heading" => {
            let style = match node.heading_level() {
                1 => "Heading1",
                2 => "Heading2",
                3 => "Heading3",
                4 => "Heading4",
                5 => "Heading5",
                _ => "Heading6",
            };
            paragraph::render_paragraph_with_style(node, style, ctx)
        }
        // A block-level image: `<w:drawing>` only exists inside a run, so it
        // needs a paragraph of its own to live in.
        "image" | "inlineImage" => {
            let drawing = drawing::render_image(node, ctx);
            if drawing.is_empty() {
                return String::new();
            }
            let jc = match node
                .attrs
                .as_ref()
                .and_then(|a| a.get("align"))
                .and_then(|v| v.as_str())
            {
                Some("center") => r#"<w:pPr><w:jc w:val="center"/></w:pPr>"#,
                Some("right") => r#"<w:pPr><w:jc w:val="right"/></w:pPr>"#,
                _ => "",
            };
            format!("<w:p>{jc}{drawing}</w:p>")
        }
        "bulletList" | "orderedList" => list::render_list(node, ctx),
        "listItem" => list::render_list_item(node, ctx),
        "table" => table::render_table(node, ctx),
        "blockquote" => {
            let mut out = String::new();
            for child in node.children() {
                out.push_str(&paragraph::render_paragraph_with_style(child, "Quote", ctx));
            }
            out
        }
        "codeBlock" => render_code_block(node),
        _ => String::new(),
    }
}

pub(crate) fn render_code_block(node: &PmNode) -> String {
    let text: String = node
        .children()
        .iter()
        .filter_map(|n| n.text.as_deref())
        .collect::<Vec<_>>()
        .join("\n");
    let mut out = String::new();
    for line in text.split('\n') {
        let escaped = escape_xml(line);
        out.push_str(&format!(
            r#"<w:p><w:pPr><w:pStyle w:val="Code"/></w:pPr><w:r><w:t xml:space="preserve">{escaped}</w:t></w:r></w:p>"#
        ));
    }
    if out.is_empty() {
        out.push_str(r#"<w:p><w:pPr><w:pStyle w:val="Code"/></w:pPr></w:p>"#);
    }
    out
}
