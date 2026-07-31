//! Serialising a table of contents as a real `TOC` field, so Word and
//! LibreOffice can refresh it instead of seeing frozen text.
//!
//! The decisive point: a TOC is **not** a run-level field. Its result spans
//! whole paragraphs, so `fldChar begin` + `instrText` go on the FIRST entry
//! paragraph and `fldChar end` on the LAST one; every entry in between is the
//! cached result. A field opened and closed inside a single paragraph leaves the
//! remaining entries outside the field, and readers then import plain
//! paragraphs instead of a table of contents.
//!
//! Switch semantics follow `DomainMapper_Impl::handleToc`
//! (sw/source/writerfilter/dmapper/DomainMapper_Impl.cxx:7420).

use crate::converters::docx::xml::escape_xml;
use crate::converters::types::PmNode;

use super::ctx::ExportCtx;
use super::paragraph::render_paragraph_with_style;
use super::run::render_inline_content;

/// Right tab stop carrying the page number: the A4 text width, i.e. the page
/// width (11906 twips) minus the two default 1440-twip side margins.
const TOC_TAB_POS: i64 = 9026;

/// Left indent Word applies to `TOCn`, one step per level, in twips. Kept in
/// sync with the `TOC1..TOC9` styles (`write/styles.rs`: `220 * (level - 1)`).
const TOC_IND_STEP: i64 = 220;

/// `true` for the heading of a TOC block (`tocTitle`, DocumentEditorPage.tsx:1068).
pub(crate) fn is_toc_title(node: &PmNode) -> bool {
    node.attrs
        .as_ref()
        .and_then(|a| a.get("tocTitle"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// The `tocLevel` of an entry paragraph, clamped to the nine levels OOXML
/// defines. `None` means the node is not a TOC entry at all.
pub(crate) fn toc_level(node: &PmNode) -> Option<u8> {
    let lvl = node.attrs.as_ref()?.get("tocLevel")?.as_u64()?;
    Some(lvl.clamp(1, 9) as u8)
}

/// `tocPage` — the entry's page number, absent when the TOC shows no numbers.
fn toc_page(node: &PmNode) -> Option<u64> {
    node.attrs.as_ref()?.get("tocPage")?.as_u64()
}

/// `tocLeader` — dot leaders requested between the entry and its page number.
fn toc_leader(node: &PmNode) -> bool {
    node.attrs
        .as_ref()
        .and_then(|a| a.get("tocLeader"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// A whole TOC block starting at `nodes[0]`: a `tocTitle` heading followed by
/// its contiguous `tocLevel` paragraphs (`generateToc`,
/// DocumentEditorPage.tsx:14090). Returns the serialised XML and how many nodes
/// it consumed, or `None` when `nodes` does not start on such a block — the
/// caller then falls back to rendering those nodes one by one.
///
/// This is the only entry point `write/document.rs` needs: it emits the title
/// with the `TOCHeading` style and the entries as the field.
pub(crate) fn render_toc_block(nodes: &[PmNode], ctx: &mut ExportCtx) -> Option<(String, usize)> {
    let title = nodes.first()?;
    if !is_toc_title(title) {
        return None;
    }
    let rest = &nodes[1..];
    let count = rest.iter().take_while(|n| toc_level(n).is_some()).count();
    if count == 0 {
        return None; // a lone marked heading is not a table of contents
    }
    let mut out = render_paragraph_with_style(title, "TOCHeading", ctx);
    out.push_str(&render_toc_field(&rest[..count], ctx));
    Some((out, 1 + count))
}

/// The contiguous `tocLevel` paragraphs of a TOC → a real `TOC` field.
///
/// Every entry becomes a `<w:p>` styled `TOC1..TOC9` with its level indent and,
/// when page numbers are shown, a right-aligned tab stop (dotted when
/// `tocLeader` is set) followed by `<w:tab/>` and the number.
pub(crate) fn render_toc_field(entries: &[PmNode], ctx: &mut ExportCtx) -> String {
    if entries.is_empty() {
        return String::new();
    }
    // `\o "1-N"`: N is the deepest level actually present, so a refresh in Word
    // rebuilds exactly the depth the editor generated.
    let max_lvl = entries.iter().filter_map(toc_level).max().unwrap_or(3);
    let pages = entries.iter().any(|n| toc_page(n).is_some());
    let leader = pages && entries.iter().any(toc_leader);

    let instr = toc_instruction(max_lvl, pages);
    let tabs = render_toc_tabs(pages, leader);
    let last = entries.len() - 1;
    let mut out = String::new();
    for (i, entry) in entries.iter().enumerate() {
        let lvl = toc_level(entry).unwrap_or(1);
        let ind = TOC_IND_STEP * i64::from(lvl - 1);
        // Child order inside `w:pPr` is imposed by `CT_PPrBase`: pStyle, tabs, ind.
        out.push_str(&format!(
            r#"<w:p><w:pPr><w:pStyle w:val="TOC{lvl}"/>{tabs}<w:ind w:left="{ind}"/></w:pPr>"#
        ));
        // Begin on the first entry, end on the last: what sits between the
        // separator and the end is the cached result readers show until they
        // refresh the field.
        if i == 0 {
            out.push_str(&format!(
                concat!(
                    r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r>"#,
                    r#"<w:r><w:instrText xml:space="preserve">{instr}</w:instrText></w:r>"#,
                    r#"<w:r><w:fldChar w:fldCharType="separate"/></w:r>"#
                ),
                instr = escape_xml(&instr)
            ));
        }
        out.push_str(&render_inline_content(entry.children(), ctx));
        if let Some(page) = toc_page(entry) {
            out.push_str(&format!(
                r#"<w:r><w:tab/></w:r><w:r><w:t>{page}</w:t></w:r>"#
            ));
        }
        if i == last {
            out.push_str(r#"<w:r><w:fldChar w:fldCharType="end"/></w:r>"#);
        }
        out.push_str("</w:p>");
    }
    out
}

/// The field instruction. `\h` makes the entries hyperlinks (Word expects it),
/// `\z` hides tab leader and page numbers in Web layout only, `\u` builds from
/// the applied paragraph outline level. `\n` with no range suppresses page
/// numbers for *every* level, so it is emitted only when no entry has one
/// (handleToc: `n` at DomainMapper_Impl.cxx:7497, `z` at :7551, `u` at :7530).
fn toc_instruction(max_lvl: u8, pages: bool) -> String {
    let mut instr = format!(" TOC \\o \"1-{max_lvl}\" \\h \\z \\u ");
    if !pages {
        instr.push_str("\\n ");
    }
    instr
}

/// The right-aligned tab stop the page number sits on, dotted on request.
/// No page numbers means no tab stop at all: the entry would otherwise trail a
/// leader line to the margin for nothing.
fn render_toc_tabs(pages: bool, leader: bool) -> String {
    if !pages {
        return String::new();
    }
    let ld = if leader { r#" w:leader="dot""# } else { "" };
    format!(r#"<w:tabs><w:tab w:val="right"{ld} w:pos="{TOC_TAB_POS}"/></w:tabs>"#)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A TOC entry paragraph exactly as `generateToc` builds it.
    fn entry(level: u64, text: &str, page: Option<u64>, leader: bool) -> PmNode {
        PmNode {
            node_type: "paragraph".into(),
            attrs: Some(json!({
                "indent": level.saturating_sub(1),
                "tocLevel": level,
                "tocPage": page,
                "tocLeader": leader,
                "spaceAfter": 2,
            })),
            content: Some(vec![PmNode::text(text, vec![])]),
            marks: None,
            text: None,
        }
    }

    fn toc_title_node(text: &str) -> PmNode {
        PmNode {
            node_type: "heading".into(),
            attrs: Some(json!({ "level": 2, "tocTitle": true })),
            content: Some(vec![PmNode::text(text, vec![])]),
            marks: None,
            text: None,
        }
    }

    fn sample_entries() -> Vec<PmNode> {
        vec![
            entry(1, "Introduction", Some(1), true),
            entry(2, "Contexte", Some(2), true),
            entry(3, "Détails & suites", Some(5), true),
        ]
    }

    #[test]
    fn field_brackets_the_whole_block_not_each_paragraph() {
        let mut ctx = ExportCtx::new();
        let xml = render_toc_field(&sample_entries(), &mut ctx);
        assert_eq!(xml.matches(r#"w:fldCharType="begin""#).count(), 1);
        assert_eq!(xml.matches(r#"w:fldCharType="separate""#).count(), 1);
        assert_eq!(xml.matches(r#"w:fldCharType="end""#).count(), 1);
        assert_eq!(xml.matches("<w:p>").count(), 3);
        let begin = xml.find(r#"w:fldCharType="begin""#).unwrap_or(usize::MAX);
        let first_p_end = xml.find("</w:p>").unwrap_or(0);
        let end = xml.rfind(r#"w:fldCharType="end""#).unwrap_or(0);
        let last_p = xml.rfind("<w:p>").unwrap_or(usize::MAX);
        assert!(begin < first_p_end, "begin must stay in the first paragraph");
        assert!(end > last_p, "end must sit in the last paragraph");
    }

    #[test]
    fn instruction_depth_and_switches() {
        let mut ctx = ExportCtx::new();
        let xml = render_toc_field(&sample_entries(), &mut ctx);
        assert!(xml.contains(
            r#"<w:instrText xml:space="preserve"> TOC \o &quot;1-3&quot; \h \z \u </w:instrText>"#
        ));
        // The deepest level present drives N.
        let two = vec![entry(1, "A", Some(1), false), entry(2, "B", Some(3), false)];
        assert!(render_toc_field(&two, &mut ctx).contains(r#"1-2"#));
    }

    #[test]
    fn styles_indents_and_dotted_right_tab() {
        let mut ctx = ExportCtx::new();
        let xml = render_toc_field(&sample_entries(), &mut ctx);
        assert!(xml.contains(
            r#"<w:pStyle w:val="TOC1"/><w:tabs><w:tab w:val="right" w:leader="dot" w:pos="9026"/></w:tabs><w:ind w:left="0"/>"#
        ));
        assert!(xml.contains(r#"<w:pStyle w:val="TOC2"/>"#));
        assert!(xml.contains(r#"<w:ind w:left="220"/>"#));
        assert!(xml.contains(r#"<w:pStyle w:val="TOC3"/>"#));
        assert!(xml.contains(r#"<w:ind w:left="440"/>"#));
        // The page number is reached through a tab, never glued to the text.
        assert!(xml.contains(r#"<w:r><w:tab/></w:r><w:r><w:t>5</w:t></w:r>"#));
        // Entry text is escaped, not injected raw.
        assert!(xml.contains("Détails &amp; suites"));
    }

    #[test]
    fn no_page_numbers_adds_backslash_n_and_drops_the_tab_stop() {
        let mut ctx = ExportCtx::new();
        let entries = vec![entry(1, "Aucun titre dans le document.", None, false)];
        let xml = render_toc_field(&entries, &mut ctx);
        assert!(xml.contains(r#" TOC \o &quot;1-1&quot; \h \z \u \n </w:instrText>"#));
        assert!(!xml.contains("<w:tabs>"));
        assert!(!xml.contains("<w:tab/>"));
    }

    #[test]
    fn leader_needs_page_numbers() {
        let mut ctx = ExportCtx::new();
        // tocLeader with no tocPage anywhere: no tab stop, hence no leader.
        let entries = vec![entry(1, "A", None, true)];
        assert!(!render_toc_field(&entries, &mut ctx).contains("w:leader"));
        // Numbers but leader off: a plain right tab stop.
        let entries = vec![entry(1, "A", Some(1), false)];
        assert!(render_toc_field(&entries, &mut ctx)
            .contains(r#"<w:tab w:val="right" w:pos="9026"/>"#));
    }

    #[test]
    fn block_emits_the_title_as_toc_heading() {
        let mut ctx = ExportCtx::new();
        let mut nodes = vec![toc_title_node("Table des matières")];
        nodes.extend(sample_entries());
        nodes.push(PmNode::paragraph(vec![]));
        let (xml, used) =
            render_toc_block(&nodes, &mut ctx).expect("nodes start on a TOC block");
        assert_eq!(used, 4, "title + 3 entries; the trailing paragraph is not ours");
        assert!(xml.starts_with(r#"<w:p><w:pPr><w:pStyle w:val="TOCHeading"/>"#));
        assert!(xml.contains(r#"w:fldCharType="begin""#));
    }

    #[test]
    fn block_declines_what_is_not_a_toc() {
        let mut ctx = ExportCtx::new();
        assert!(render_toc_block(&[], &mut ctx).is_none());
        assert!(render_toc_block(&[PmNode::paragraph(vec![])], &mut ctx).is_none());
        // A marked heading with no entries after it is not a table of contents.
        assert!(render_toc_block(&[toc_title_node("Vide")], &mut ctx).is_none());
        assert!(render_toc_field(&[], &mut ctx).is_empty());
    }

    /// Writes real `.docx` files holding a TOC — one with page numbers and dot
    /// leaders, one without (the `\n` form) — so they can be reopened with
    /// LibreOffice (`soffice --convert-to fodt`) and checked for a
    /// `text:table-of-content` section. Ignored by default because it touches
    /// the filesystem:
    /// `KB_TOC_DIR=/path cargo test write::toc:: -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn writes_a_real_docx_for_libreoffice() {
        let dir = std::env::var("KB_TOC_DIR").unwrap_or_else(|_| "/tmp".to_string());
        let numbered = sample_entries();
        let plain: Vec<PmNode> = [(1u64, "Introduction"), (2, "Contexte"), (3, "Détails & suites")]
            .iter()
            .map(|(lvl, txt)| entry(*lvl, txt, None, false))
            .collect();
        for (name, entries) in [("toc_pages", &numbered), ("toc_nopages", &plain)] {
            let path = format!("{dir}/kubuno_{name}.docx");
            write_docx(&path, entries);
            println!("wrote {path}");
        }
    }

    /// A minimal valid package whose body is the TOC block followed by the real
    /// headings, so a reader refreshing the field has something to rebuild from.
    fn write_docx(path: &str, entries: &[PmNode]) {
        use std::io::Write;

        let mut ctx = ExportCtx::new();
        let mut nodes = vec![toc_title_node("Table des matières")];
        nodes.extend_from_slice(entries);
        let (toc_xml, _) =
            render_toc_block(&nodes, &mut ctx).expect("nodes start on a TOC block");

        let mut body = toc_xml;
        for (lvl, text) in [(1u8, "Introduction"), (2, "Contexte"), (3, "Détails & suites")] {
            let node = PmNode::heading(lvl, vec![PmNode::text(text, vec![])]);
            body.push_str(&render_paragraph_with_style(
                &node,
                &format!("Heading{lvl}"),
                &mut ctx,
            ));
        }
        let sect_pr = super::super::section::render_sect_pr(
            &super::super::super::model::SectionInfo::default(),
        );
        let doc_xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
    xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
    xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
    xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
    xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"
    xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"
    xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml"
    mc:Ignorable="w14">
<w:body>{body}{sect_pr}</w:body></w:document>"#
        );

        let file = std::fs::File::create(path).expect("create the docx");
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::FileOptions::<()>::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, data) in [
            ("[Content_Types].xml", super::super::package::build_content_types(&ctx)),
            ("_rels/.rels", super::super::package::RELS.to_string()),
            ("word/_rels/document.xml.rels", ctx.doc_rels_xml()),
            ("word/styles.xml", super::super::styles::build_styles_xml()),
            ("word/numbering.xml", super::super::numbering::numbering_xml()),
            ("word/document.xml", doc_xml),
        ] {
            zip.start_file(name, opts).expect("start the zip entry");
            zip.write_all(data.as_bytes()).expect("write the zip entry");
        }
        zip.finish().expect("finish the zip");
    }
}
