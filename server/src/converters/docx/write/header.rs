//! `word/header1.xml` and `word/footer1.xml`.
//!
//! Without these parts the `{page}` / `{pages}` tokens of the editor have
//! nowhere to live, so page numbering cannot survive an export at all.
//!
//! Mirrors `DocxExport::WriteHeaderFooter`
//! (sw/source/filter/ww8/docxexport.cxx:1015): one part per flavour, a
//! relationship from `document.xml` to it, and a `<w:headerReference>` /
//! `<w:footerReference>` written into the `sectPr`.

use crate::converters::docx::xml::escape_xml_attr;
use crate::converters::types::{PmMark, PmNode};

use super::ctx::{ExportCtx, Rel};
use super::document::render_body;
use super::field::{render_hf_text_with, HfValues};
use super::run::render_run;

/// Base of every OPC relationship type; `ExportCtx` appends the suffixes we
/// pass to `add_rel` (kept in sync with `ctx::REL_BASE`, which is private).
const REL_BASE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

const CT_HEADER: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml";
const CT_FOOTER: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml";
const CT_RELS: &str = "application/vnd.openxmlformats-package.relationships+xml";

/// Namespaces of a header/footer part. Identical to the set on
/// `<w:document>`: LibreOffice reuses `MainXmlNamespaces` for `<w:hdr>`
/// (docxexport.cxx:1031), and a part missing `r:` or `wp:` cannot carry a
/// hyperlink or an image.
const HF_NS: &str = concat!(
    r#" xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#,
    r#" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#,
    r#" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing""#,
    r#" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main""#,
    r#" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture""#,
    r#" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006""#,
    r#" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml""#,
    r#" mc:Ignorable="w14""#,
);

/// Everything that differs between a header part and a footer part.
struct Flavour {
    /// Root element of the part: `w:hdr` or `w:ftr`.
    root: &'static str,
    /// Relationship type suffix handed to `ExportCtx::add_rel`.
    rel_type: &'static str,
    content_type: &'static str,
    /// File stem inside `word/`: `header1.xml`, `footer2.xml`…
    stem: &'static str,
    /// Element written in the `sectPr`.
    reference: &'static str,
}

const HEADER: Flavour = Flavour {
    root: "w:hdr",
    rel_type: "header",
    content_type: CT_HEADER,
    stem: "header",
    reference: "w:headerReference",
};

const FOOTER: Flavour = Flavour {
    root: "w:ftr",
    rel_type: "footer",
    content_type: CT_FOOTER,
    stem: "footer",
    reference: "w:footerReference",
};

/// Write the header/footer parts and return the `sectPr` references for them.
///
/// The returned string goes FIRST in `<w:sectPr>`: `EG_HdrFtrReferences`
/// precedes `w:footnotePr`, `w:type`, `w:pgSz` and the rest in the normative
/// content model, and Word refuses (or silently drops) a reference placed
/// after the page geometry.
pub(crate) fn write_parts(
    header: Option<&PmNode>,
    footer: Option<&PmNode>,
    ctx: &mut ExportCtx,
) -> String {
    write_parts_typed(&[("default", header, footer)], &HfValues::default(), ctx)
}

/// Same, for a section needing several flavours: `<w:titlePg>` requires a
/// `first` entry and `<w:evenAndOddHeaders>` an `even` one, otherwise Word
/// falls back to the `default` part on those pages.
///
/// Each entry is `(type, header, footer)` with `type` ∈ `even` | `default` |
/// `first` — the order LibreOffice writes them in
/// (`DocxExport::WriteHeaderFooterFlags`, docxexport.cxx:334).
///
/// `vals` carries the cached field results: pass
/// `HfValues::new(title, page_num_fmt)` so `{titre}` leaves with the real title
/// and `{page}` with the numbering format of the section.
pub(crate) fn write_parts_typed(
    entries: &[(&str, Option<&PmNode>, Option<&PmNode>)],
    vals: &HfValues,
    ctx: &mut ExportCtx,
) -> String {
    let mut refs = String::new();
    // Headers first, then footers: that is the order Word writes and the one
    // its schema validator is happiest with, even though the group allows any.
    for (kind, header, _) in entries {
        if let Some(node) = header {
            refs.push_str(&write_one(node, kind, &HEADER, vals, ctx));
        }
    }
    for (kind, _, footer) in entries {
        if let Some(node) = footer {
            refs.push_str(&write_one(node, kind, &FOOTER, vals, ctx));
        }
    }
    refs
}

/// Serialise one part, register it plus its relationship, and return the
/// reference element for the `sectPr`.
fn write_one(
    node: &PmNode,
    kind: &str,
    f: &Flavour,
    vals: &HfValues,
    ctx: &mut ExportCtx,
) -> String {
    // Part names must be unique across every call (a document can have several
    // sections, each with its own parts), so derive the index from what is
    // already registered rather than from a counter of our own.
    let prefix = format!("word/{}", f.stem);
    let n = 1 + ctx
        .parts
        .iter()
        .filter(|(path, _, _)| path.starts_with(&prefix))
        .count();
    let name = format!("{}{n}.xml", f.stem);

    // Relationships discovered while rendering (hyperlinks, images) belong to
    // THIS part, not to document.xml: an `r:id` is resolved against the rels of
    // the part that uses it. Snapshot the list to tell the new ones apart.
    let first_new = ctx.rels.len();
    let body = expand_hf_tokens(render_body(node, ctx), node, vals);
    let own_rels = part_rels_xml(&ctx.rels[first_new..]);

    // Added after the snapshot: the relationship TO the part must not end up
    // inside the part's own rels file.
    let rid = ctx.add_rel(f.rel_type, name.clone(), false);

    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<{root}{HF_NS}>
{body}
</{root}>"#,
        root = f.root,
    );
    ctx.parts.push((format!("word/{name}"), f.content_type, xml));
    if let Some(rels) = own_rels {
        ctx.parts
            .push((format!("word/_rels/{name}.rels"), CT_RELS, rels));
    }

    format!(
        r#"<{} w:type="{}" r:id="{rid}"/>"#,
        f.reference,
        escape_xml_attr(kind)
    )
}

/// `word/_rels/<part>.xml.rels` for the relationships a header/footer owns, or
/// `None` when it owns none (the usual case: plain text and page fields).
fn part_rels_xml(rels: &[Rel]) -> Option<String> {
    if rels.is_empty() {
        return None;
    }
    let mut s = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
    );
    for r in rels {
        let mode = if r.external {
            r#" TargetMode="External""#
        } else {
            ""
        };
        s.push_str(&format!(
            r#"<Relationship Id="{}" Type="{REL_BASE}/{}" Target="{}"{mode}/>"#,
            r.id,
            r.rel_type,
            escape_xml_attr(&r.target)
        ));
    }
    s.push_str("</Relationships>");
    Some(s)
}

/// Turn the editor's `{page}` / `{pages}` / `{date}` / `{titre}` tokens into
/// real Word fields, which is the whole point of exporting a header: frozen text
/// never renumbers, and a literal `{page}` on every page is worse than no header
/// at all.
///
/// The conversion itself belongs to `write/field.rs` (`render_hf_text_with`);
/// here we only substitute, run by run. Both sides of the substitution are
/// produced by their owners — `run::render_run` for what the body renderer just
/// emitted, `render_hf_text_with` for the field sequence replacing it — so the
/// match is exact by construction. When a run does not match (the body renderer
/// merged or reshaped it), the token is simply left as text: degraded, never
/// corrupt.
///
/// The upstream fix is a "writing header/footer" flag on the run renderer, the
/// way LibreOffice does it (`SetWritingHeaderFooter`,
/// docxattributeoutput.hxx:1165); this keeps the change inside the header
/// exporter until that flag exists.
fn expand_hf_tokens(body: String, node: &PmNode, vals: &HfValues) -> String {
    let mut out = body;
    for (text, marks) in token_texts(node) {
        let expanded = render_hf_text_with(text, marks, vals);
        let plain = render_run(text, marks);
        // `render_hf_text_with` returns exactly `render_run` when the text holds
        // no token, so a difference IS the presence of a token.
        if expanded != plain {
            out = out.replace(&plain, &expanded);
        }
    }
    out
}

/// Every text node of the tree that may hold a token, with its marks.
fn token_texts(node: &PmNode) -> Vec<(&str, &[PmMark])> {
    let mut found = Vec::new();
    collect_token_texts(node, &mut found);
    found
}

fn collect_token_texts<'a>(node: &'a PmNode, found: &mut Vec<(&'a str, &'a [PmMark])>) {
    let text = node.text.as_deref().unwrap_or_default();
    if node.node_type == "text" && text.contains('{') {
        found.push((text, node.marks.as_deref().unwrap_or(&[])));
    }
    for child in node.children() {
        collect_token_texts(child, found);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hf(text: &str) -> PmNode {
        PmNode::doc(vec![PmNode::paragraph(vec![PmNode::text(text, vec![])])])
    }

    #[test]
    fn no_header_and_no_footer_writes_nothing() {
        let mut ctx = ExportCtx::new();
        assert!(write_parts(None, None, &mut ctx).is_empty());
        assert!(ctx.parts.is_empty());
        assert!(ctx.rels.is_empty());
    }

    #[test]
    fn default_flavour_writes_both_parts_and_their_references() {
        let mut ctx = ExportCtx::new();
        let (h, f) = (hf("En-tête"), hf("Pied"));
        let refs = write_parts(Some(&h), Some(&f), &mut ctx);

        assert!(refs.contains(r#"<w:headerReference w:type="default" r:id="rId3"/>"#));
        assert!(refs.contains(r#"<w:footerReference w:type="default" r:id="rId4"/>"#));
        // Header reference before footer reference, as Word writes them.
        let at = |s: &str| refs.find(s).unwrap_or(usize::MAX);
        assert!(at("<w:headerReference") < at("<w:footerReference"));

        let paths: Vec<&str> = ctx.parts.iter().map(|(p, _, _)| p.as_str()).collect();
        assert_eq!(paths, vec!["word/header1.xml", "word/footer1.xml"]);

        let (_, ct, xml) = &ctx.parts[0];
        assert_eq!(*ct, CT_HEADER);
        assert!(xml.contains("<w:hdr"));
        assert!(xml.contains(
            r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#
        ));
        assert!(xml.contains("En-tête"));
        assert!(xml.ends_with("</w:hdr>"));
        assert!(ctx.parts[1].2.contains("<w:ftr"));
    }

    #[test]
    fn titlepg_and_evenodd_add_numbered_parts() {
        let mut ctx = ExportCtx::new();
        let (even, def, first) = (hf("Pair"), hf("Défaut"), hf("Première"));
        let refs = write_parts_typed(
            &[
                ("even", Some(&even), None),
                ("default", Some(&def), None),
                ("first", Some(&first), None),
            ],
            &HfValues::default(),
            &mut ctx,
        );
        assert!(refs.contains(r#"w:type="even""#));
        assert!(refs.contains(r#"w:type="first""#));
        let paths: Vec<&str> = ctx.parts.iter().map(|(p, _, _)| p.as_str()).collect();
        assert_eq!(
            paths,
            vec!["word/header1.xml", "word/header2.xml", "word/header3.xml"]
        );
    }

    #[test]
    fn a_hyperlink_in_a_header_gets_its_own_rels_part() {
        let mut ctx = ExportCtx::new();
        // The relationship is created by the run renderer; simulate the state it
        // leaves behind by rendering a header holding a link mark.
        let link = PmNode {
            node_type: "text".into(),
            attrs: None,
            content: None,
            marks: Some(vec![PmMark {
                mark_type: "link".into(),
                attrs: Some(serde_json::json!({ "href": "https://kubuno.test/" })),
            }]),
            text: Some("Kubuno".into()),
        };
        let h = PmNode::doc(vec![PmNode::paragraph(vec![link])]);
        write_parts(Some(&h), None, &mut ctx);

        let rels_part = ctx
            .parts
            .iter()
            .find(|(p, _, _)| p == "word/_rels/header1.xml.rels");
        match rels_part {
            Some((_, ct, xml)) => {
                assert_eq!(*ct, CT_RELS);
                assert!(xml.contains("https://kubuno.test/"));
                assert!(xml.contains(r#"TargetMode="External""#));
                // The relationship to the part itself stays out of its own rels.
                assert!(!xml.contains("header1.xml"));
            }
            // The run renderer may not emit hyperlink relations yet; the part is
            // then legitimately absent.
            None => assert!(ctx.rels.iter().all(|r| r.rel_type != "hyperlink")),
        }
    }

    #[test]
    fn page_tokens_leave_as_fields_and_plain_braces_survive() {
        let mut ctx = ExportCtx::new();
        let h = hf("Page {page} / {pages} — Coût {net}");
        write_parts(Some(&h), None, &mut ctx);
        let xml = &ctx.parts[0].2;
        assert!(xml.contains(r#"<w:fldChar w:fldCharType="begin"/>"#));
        assert!(xml.contains("PAGE"));
        assert!(xml.contains("NUMPAGES"));
        // A brace that starts no token stays text.
        assert!(xml.contains("{net}"));
        assert!(!xml.contains("{page}"));
        assert!(!xml.contains("{pages}"));
    }

    #[test]
    fn cached_values_come_from_the_caller() {
        let mut ctx = ExportCtx::new();
        let h = hf("{titre}");
        let vals = HfValues::new("Rapport annuel", Some("roman-upper"));
        write_parts_typed(&[("default", Some(&h), None)], &vals, &mut ctx);
        let xml = &ctx.parts[0].2;
        assert!(xml.contains("Rapport annuel"));
        assert!(xml.contains("TITLE"));
    }

    #[test]
    fn an_empty_header_still_holds_one_paragraph() {
        let mut ctx = ExportCtx::new();
        let h = PmNode::doc(vec![]);
        write_parts(Some(&h), None, &mut ctx);
        assert!(ctx.parts[0].2.contains("<w:p/>"));
    }

    /// Builds a complete package with a header and a footer and drops it on
    /// disk, so the result can be checked with `unzip -l` and reopened by
    /// LibreOffice. Ignored by default (it writes a file):
    /// `KB_DOCX_OUT=/tmp/hf.docx cargo test -- --ignored dump_package`.
    #[test]
    #[ignore]
    fn dump_package() {
        use std::io::Write;

        use crate::converters::docx::model::SectionInfo;
        use crate::converters::docx::write::{numbering, package, section, styles};

        let body = PmNode::doc(vec![
            PmNode::heading(1, vec![PmNode::text("Rapport", vec![])]),
            PmNode::paragraph(vec![PmNode::text("Corps du document.", vec![])]),
        ]);
        let header = hf("Kubuno — Page {page} / {pages}");
        let footer = hf("Pied de page");

        let mut ctx = ExportCtx::new();
        let body_xml = render_body(&body, &mut ctx);
        // Header/footer parts must be written BEFORE [Content_Types].xml and
        // document.xml.rels, which declare what they registered.
        let refs = write_parts(Some(&header), Some(&footer), &mut ctx);
        let sect_pr = section::render_sect_pr(&SectionInfo::default())
            .replacen("<w:sectPr>", &format!("<w:sectPr>{refs}"), 1);

        let buf = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(buf);
        let opts = zip::write::FileOptions::<()>::default()
            .compression_method(zip::CompressionMethod::Deflated);
        let put = |zip: &mut zip::ZipWriter<_>, path: &str, data: &str| {
            zip.start_file(path, opts).expect("start_file");
            zip.write_all(data.as_bytes()).expect("write");
        };
        put(&mut zip, "[Content_Types].xml", &package::build_content_types(&ctx));
        put(&mut zip, "_rels/.rels", package::RELS);
        put(&mut zip, "word/_rels/document.xml.rels", &ctx.doc_rels_xml());
        put(&mut zip, "word/styles.xml", &styles::build_styles_xml());
        put(&mut zip, "word/numbering.xml", &numbering::numbering_xml());
        for (path, _, xml) in ctx.parts.clone() {
            put(&mut zip, &path, &xml);
        }
        put(
            &mut zip,
            "docProps/core.xml",
            r#"<?xml version="1.0" encoding="UTF-8"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Test</dc:title></cp:coreProperties>"#,
        );
        put(
            &mut zip,
            "word/document.xml",
            &format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
    xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
    xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
    xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
    xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"
    xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"
    xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml"
    mc:Ignorable="w14">
<w:body>
{body_xml}
{sect_pr}
</w:body>
</w:document>"#
            ),
        );
        let bytes = zip.finish().expect("finish").into_inner();

        let out = std::env::var("KB_DOCX_OUT").unwrap_or_else(|_| {
            std::env::temp_dir()
                .join("kubuno-header-footer.docx")
                .to_string_lossy()
                .into_owned()
        });
        std::fs::write(&out, &bytes).expect("write docx");
        println!("wrote {out} ({} bytes)", bytes.len());
    }
}
