//! ProseMirror JSON -> DOCX.
//!
//! One module per domain, each owning its own serialisation. The body is
//! rendered BEFORE the package parts are written: rendering is what discovers
//! hyperlinks, images and other relationships that `[Content_Types].xml` and
//! `word/_rels/document.xml.rels` must then declare.

pub(crate) mod app_props;
pub(crate) mod comments;
pub(crate) mod ctx;
pub(crate) mod document;
pub(crate) mod drawing;
// Field and header/footer serialisation: written ahead of the pipeline wiring,
// several entry points are not called yet.
#[allow(dead_code)]
pub(crate) mod field;
#[allow(dead_code)]
pub(crate) mod header;
pub(crate) mod list;
pub(crate) mod notes;
pub(crate) mod numbering;
pub(crate) mod package;
pub(crate) mod paragraph;
pub(crate) mod revisions;
pub(crate) mod run;
pub(crate) mod section;
pub(crate) mod settings;
pub(crate) mod styles;
pub(crate) mod table;
pub(crate) mod toc;

use std::io::{Cursor, Write};

use zip::{write::FileOptions, ZipWriter};

use crate::converters::types::PmNode;
use crate::errors::Result;

use super::model::SectionInfo;
use super::xml::escape_xml;
use ctx::ExportCtx;
use document::render_body;
use package::{build_content_types, RELS};

/// Export with the default A4 page layout.
pub fn export_docx(doc: &PmNode, title: &str) -> Result<Vec<u8>> {
    export_docx_with_layout(doc, title, &SectionInfo::default())
}

pub fn export_docx_with_layout(doc: &PmNode, title: &str, layout: &SectionInfo) -> Result<Vec<u8>> {
    export_docx_full(doc, title, layout, None, None)
}

/// Export with the page layout AND the header/footer parts. Without the parts,
/// the editor's `{page}` / `{pages}` tokens have nowhere to live and page
/// numbering does not survive the export at all.
pub fn export_docx_full(
    doc: &PmNode,
    title: &str,
    layout: &SectionInfo,
    header: Option<&PmNode>,
    footer: Option<&PmNode>,
) -> Result<Vec<u8>> {
    export_docx_all(doc, title, layout, header, footer, &[])
}

/// The full export, including the comment threads. Threads live in the
/// collaborative document rather than in the ProseMirror tree, so they can only
/// come from the caller — an anchored `comment` mark whose thread is missing is
/// dropped rather than written as an orphan range.
pub fn export_docx_all(
    doc: &PmNode,
    title: &str,
    layout: &SectionInfo,
    header: Option<&PmNode>,
    footer: Option<&PmNode>,
    threads: &[ctx::CommentThreadIn],
) -> Result<Vec<u8>> {
    export_docx_hf(doc, title, layout, header, footer, None, None, threads)
}

/// The export with separate EVEN-page header and footer.
///
/// `w:evenAndOddHeaders` is a document setting, so the flag on `SectionInfo`
/// drives `settings.xml`; without that part an `even` reference is ignored by
/// Word, which is why the two are written together or not at all.
#[allow(clippy::too_many_arguments)]
pub fn export_docx_hf(
    doc: &PmNode,
    title: &str,
    layout: &SectionInfo,
    header: Option<&PmNode>,
    footer: Option<&PmNode>,
    header_even: Option<&PmNode>,
    footer_even: Option<&PmNode>,
    threads: &[ctx::CommentThreadIn],
) -> Result<Vec<u8>> {
    // Render first: this is what populates ctx with relationships and media.
    let mut ctx = ExportCtx::with_comments(threads);
    let mut body = render_body(doc, &mut ctx);
    // Ranges left open at the end of the body must be closed inside a paragraph.
    let tail = run::close_open_ranges(&mut ctx);
    if !tail.is_empty() {
        body.push_str(&format!("<w:p>{tail}</w:p>"));
    }
    // The header/footer parts register their own relationships, so they must be
    // written before [Content_Types].xml and document.xml.rels are built.
    // Word's own order: even, then default. A reference is only written for a
    // variant that actually has content.
    let mut entries: Vec<(&str, Option<&PmNode>, Option<&PmNode>)> = Vec::new();
    let even_odd = layout.even_odd_headers && (header_even.is_some() || footer_even.is_some());
    if even_odd {
        entries.push(("even", header_even, footer_even));
    }
    entries.push(("default", header, footer));
    let hf_refs = header::write_parts_typed(
        &entries,
        &field::HfValues::new(title, Some(&layout.page_num_fmt)),
        &mut ctx,
    );
    let sect_pr = section::render_sect_pr_with(layout, &hf_refs);

    // Conditional parts are decided — and their RELATIONSHIPS created — before
    // anything is written: `document.xml.rels` is serialised early, so a part
    // registered afterwards would exist with no relationship pointing at it,
    // which is exactly what makes a reader refuse the package.
    let settings = settings::settings_xml(settings::SettingsOptions {
        even_odd_headers: even_odd,
        track_changes: layout.track_changes,
    });
    ctx.add_rel(ctx::REL_SETTINGS, "settings.xml".into(), false);
    let footnotes = notes::footnotes_xml(&ctx);
    if footnotes.is_some() {
        ctx.add_rel(ctx::REL_FOOTNOTES, "footnotes.xml".into(), false);
    }
    let endnotes = notes::endnotes_xml(&ctx);
    if endnotes.is_some() {
        ctx.add_rel(ctx::REL_ENDNOTES, "endnotes.xml".into(), false);
    }
    let comments_part = comments::comments_xml(&ctx);
    let comments_ext = comments::comments_extended_xml(&ctx);
    if comments_part.is_some() {
        ctx.add_rel(ctx::REL_COMMENTS, "comments.xml".into(), false);
        if comments_ext.is_some() {
            ctx.add_rel_absolute(ctx::REL_COMMENTS_EXT, "commentsExtended.xml".into());
        }
    }

    let buf = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(buf);
    let opts = FileOptions::<()>::default().compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("[Content_Types].xml", opts)?;
    zip.write_all(build_content_types(&ctx).as_bytes())?;

    zip.start_file("_rels/.rels", opts)?;
    zip.write_all(RELS.as_bytes())?;

    zip.start_file("word/_rels/document.xml.rels", opts)?;
    zip.write_all(ctx.doc_rels_xml().as_bytes())?;

    zip.start_file("word/styles.xml", opts)?;
    zip.write_all(styles::build_styles_xml().as_bytes())?;

    zip.start_file("word/numbering.xml", opts)?;
    zip.write_all(numbering::numbering_xml().as_bytes())?;

    zip.start_file("word/settings.xml", opts)?;
    zip.write_all(settings.as_bytes())?;

    if let Some(xml) = &footnotes {
        zip.start_file("word/footnotes.xml", opts)?;
        zip.write_all(xml.as_bytes())?;
    }
    if let Some(xml) = &endnotes {
        zip.start_file("word/endnotes.xml", opts)?;
        zip.write_all(xml.as_bytes())?;
    }
    if let Some(xml) = &comments_part {
        zip.start_file("word/comments.xml", opts)?;
        zip.write_all(xml.as_bytes())?;
        if let Some(ext) = &comments_ext {
            zip.start_file("word/commentsExtended.xml", opts)?;
            zip.write_all(ext.as_bytes())?;
        }
    }


    // Media are stored uncompressed: PNG/JPEG are already compressed formats.
    let raw = FileOptions::<()>::default().compression_method(zip::CompressionMethod::Stored);
    for (path, bytes) in &ctx.media {
        zip.start_file(path.as_str(), raw)?;
        zip.write_all(bytes)?;
    }

    for (path, _, xml) in &ctx.parts {
        zip.start_file(path.as_str(), opts)?;
        zip.write_all(xml.as_bytes())?;
    }

    // `docProps/app.xml`: Word's Properties panel reads its statistics here.
    let (paras, words, chars) = app_props::body_stats(&body);
    zip.start_file("docProps/app.xml", opts)?;
    zip.write_all(app_props::app_xml(paras, words, chars).as_bytes())?;

    zip.start_file("docProps/core.xml", opts)?;
    let escaped_title = escape_xml(title);
    zip.write_all(
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties"
    xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>{escaped_title}</dc:title>
</cp:coreProperties>"#
        )
        .as_bytes(),
    )?;

    zip.start_file("word/document.xml", opts)?;
    // Namespace set modelled on DocxExport::MainXmlNamespaces
    // (sw/source/filter/ww8/docxexport.cxx:2232).
    let doc_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
    xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
    xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
    xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
    xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"
    xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"
    xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml"
    xmlns:w15="http://schemas.microsoft.com/office/word/2012/wordml"
    mc:Ignorable="w14 w15">
<w:body>
{body}
{sect_pr}
</w:body>
</w:document>"#
    );
    zip.write_all(doc_xml.as_bytes())?;

    let result = zip.finish()?;
    Ok(result.into_inner())
}
