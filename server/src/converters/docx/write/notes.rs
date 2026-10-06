//! `word/footnotes.xml`.
//!
//! Ids 0 and 1 are NOT footnotes: they are the mandatory separator and
//! continuation-separator entries that carry the ruling line. Real notes start
//! at 2, which is why `render_footnote_ref` numbers from `index + 2`.
//! Reference: LibreOffice `sw/source/filter/ww8/docxattributeoutput.cxx:9224`
//! and `docxfootnotes.hxx:57` ("skip ids 0 and 1 — they are reserved").

use crate::converters::docx::xml::{escape_xml, text_to_run_children};

use super::ctx::ExportCtx;

/// Register a footnote's text and return the run that references it.
pub(crate) fn render_footnote_ref(text: &str, ctx: &mut ExportCtx) -> String {
    ctx.footnotes.push(text.to_string());
    let id = ctx.footnotes.len() + 1; // ids 0 and 1 are reserved
    format!(
        r#"<w:r><w:rPr><w:rStyle w:val="FootnoteReference"/></w:rPr><w:footnoteReference w:id="{id}"/></w:r>"#
    )
}

/// Register an endnote and return the run that references it.
///
/// Endnotes are a SEPARATE sequence in their own part: sharing the footnote
/// counter would make the two parts disagree about which note is which.
pub(crate) fn render_endnote_ref(text: &str, ctx: &mut ExportCtx) -> String {
    ctx.endnotes.push(text.to_string());
    let id = ctx.endnotes.len() + 1; // ids 0 and 1 are reserved here too
    format!(
        r#"<w:r><w:rPr><w:rStyle w:val="EndnoteReference"/></w:rPr><w:endnoteReference w:id="{id}"/></w:r>"#
    )
}

/// `word/endnotes.xml`, same shape as the footnotes part.
pub(crate) fn endnotes_xml(ctx: &ExportCtx) -> Option<String> {
    if ctx.endnotes.is_empty() {
        return None;
    }
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:endnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:endnote w:id="0" w:type="separator"><w:p><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:r><w:separator/></w:r></w:p></w:endnote>
<w:endnote w:id="1" w:type="continuationSeparator"><w:p><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:r><w:continuationSeparator/></w:r></w:p></w:endnote>
"#,
    );
    for (i, text) in ctx.endnotes.iter().enumerate() {
        let id = i + 2;
        let mut paras = String::new();
        for (j, line) in text.split('\n').enumerate() {
            let mark = if j == 0 {
                concat!(
                    r#"<w:r><w:rPr><w:rStyle w:val="EndnoteReference"/></w:rPr><w:endnoteRef/></w:r>"#,
                    r#"<w:r><w:t xml:space="preserve"> </w:t></w:r>"#
                )
            } else {
                ""
            };
            paras.push_str(&format!(
                r#"<w:p><w:pPr><w:pStyle w:val="EndnoteText"/></w:pPr>{mark}<w:r>{}</w:r></w:p>"#,
                text_to_run_children(line)
            ));
        }
        out.push_str(&format!(r#"<w:endnote w:id="{id}">{paras}</w:endnote>"#));
    }
    out.push_str("</w:endnotes>");
    Some(out)
}

/// The whole part. Returns `None` when the document has no footnote, so the
/// package does not declare a part nobody references.
pub(crate) fn footnotes_xml(ctx: &ExportCtx) -> Option<String> {
    if ctx.footnotes.is_empty() {
        return None;
    }
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:footnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:footnote w:id="0" w:type="separator"><w:p><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:r><w:separator/></w:r></w:p></w:footnote>
<w:footnote w:id="1" w:type="continuationSeparator"><w:p><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>
"#,
    );
    for (i, text) in ctx.footnotes.iter().enumerate() {
        let id = i + 2;
        let mut paras = String::new();
        // Our model holds a footnote as plain text: one note becomes one or more
        // paragraphs, split on newlines.
        for (j, line) in text.split('\n').enumerate() {
            // The reference mark belongs to the FIRST paragraph only.
            let mark = if j == 0 {
                concat!(
                    r#"<w:r><w:rPr><w:rStyle w:val="FootnoteReference"/></w:rPr><w:footnoteRef/></w:r>"#,
                    r#"<w:r><w:t xml:space="preserve"> </w:t></w:r>"#
                )
            } else {
                ""
            };
            paras.push_str(&format!(
                r#"<w:p><w:pPr><w:pStyle w:val="FootnoteText"/></w:pPr>{mark}<w:r>{}</w:r></w:p>"#,
                text_to_run_children(line)
            ));
        }
        out.push_str(&format!(r#"<w:footnote w:id="{id}">{paras}</w:footnote>"#));
    }
    out.push_str("</w:footnotes>");
    let _ = escape_xml; // kept for symmetry with the other writers
    Some(out)
}
