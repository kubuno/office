//! DOCX -> ProseMirror JSON.
//!
//! DOM-based parser (roxmltree). Handles run formatting (bold/italic/underline/
//! strike, color, font size & family, highlight, super/subscript), paragraph
//! alignment, headings & block styles, hyperlinks (resolved via document rels),
//! nested ordered/bulleted lists (type detected from numbering.xml) and tables.

pub(crate) mod drawing;
pub(crate) mod fields;
pub(crate) mod notes;
pub(crate) mod numbering;
pub(crate) mod paragraph;
pub(crate) mod revisions;
pub(crate) mod run;
pub(crate) mod section;
pub(crate) mod styles;
pub(crate) mod table;
pub(crate) mod toc;
pub(crate) mod theme;

use std::collections::HashMap;
use std::io::Cursor;

use roxmltree::{Document as XmlDoc, Node};
use zip::ZipArchive;

use crate::converters::types::PmNode;
use crate::errors::{OfficeError, Result};

use super::model::{Block, FontCtx, SectionInfo};
use super::xml::{attr_val, local};
use super::zip_io::{build_media_map, read_zip_entry};
use drawing::parse_drawings;
use fields::FieldState;
use notes::NoteTexts;
use numbering::parse_numbering;
use paragraph::parse_paragraph;
use section::parse_section_props;
use styles::{parse_para_default, parse_para_styles};
use table::parse_table;
use theme::{parse_default_font, parse_default_size, parse_theme, parse_theme_fonts};

/// Import a DOCX → (body, header, footer). Header/footer = `Option<PM doc>` (None when
/// absent or empty). The handler builds the multi-page wrapper when either exists.
pub fn import_docx(data: &[u8]) -> Result<(PmNode, Option<PmNode>, Option<PmNode>, SectionInfo)> {
    let cursor = Cursor::new(data);
    let mut archive = ZipArchive::new(cursor)
        .map_err(|e| OfficeError::Conversion(format!("ZIP invalide: {e}")))?;

    let document_xml = read_zip_entry(&mut archive, "word/document.xml")
        .ok_or_else(|| OfficeError::Conversion("word/document.xml introuvable".into()))?;
    let rels = read_zip_entry(&mut archive, "word/_rels/document.xml.rels")
        .as_deref()
        .map(parse_rels)
        .unwrap_or_default();
    let numbering = read_zip_entry(&mut archive, "word/numbering.xml")
        .as_deref()
        .map(parse_numbering)
        .unwrap_or_default();
    // Theme: colour palette (`schemeClr`) + major/minor fonts (`fontScheme`).
    let theme_xml = read_zip_entry(&mut archive, "word/theme/theme1.xml");
    let theme = theme_xml.as_deref().map(parse_theme).unwrap_or_default();
    let (major, minor) = theme_xml.as_deref().map(parse_theme_fonts).unwrap_or_default();
    // Document default font (docDefaults of styles.xml), resolved through the theme.
    let styles_xml = read_zip_entry(&mut archive, "word/styles.xml");
    let default_font = styles_xml.as_deref().and_then(|s| parse_default_font(s, &major, &minor));
    let default_size = styles_xml.as_deref().and_then(parse_default_size);
    let has_run_default = styles_xml.as_deref().is_some_and(theme::has_run_default);
    // Inherited paragraph properties (docDefaults + named styles).
    let para_default = styles_xml.as_deref().map(parse_para_default).unwrap_or_default();
    let (para_styles, para_style_default, heading_levels, run_styles) =
        styles_xml.as_deref().map(parse_para_styles).unwrap_or_default();
    // Fallback when the document states no default: Word 2007+ uses Calibri 11,
    // but ONLY when `docDefaults/rPrDefault` is absent — see `has_run_default`.
    let (fallback_font, fallback_size) = if has_run_default {
        ("Times New Roman", 10.0)
    } else {
        ("Calibri", 11.0)
    };
    let fonts = FontCtx {
        major,
        minor,
        default: Some(default_font.unwrap_or_else(|| fallback_font.to_string())),
        default_size: default_size.unwrap_or(fallback_size),
        para_default,
        para_styles,
        para_style_default,
        heading_levels,
        run_styles,
    };
    // Pictures embedded in the body: `rId → data-URL` (read from word/media/…).
    let media = build_media_map(&mut archive, &rels, "word");
    // Note texts live in their own parts; the body only holds references.
    let note_texts = NoteTexts {
        footnotes: read_zip_entry(&mut archive, "word/footnotes.xml")
            .as_deref()
            .map(|x| notes::parse_notes_part(x, "footnote"))
            .unwrap_or_default(),
        endnotes: read_zip_entry(&mut archive, "word/endnotes.xml")
            .as_deref()
            .map(|x| notes::parse_notes_part(x, "endnote"))
            .unwrap_or_default(),
    };

    let mut fs = FieldState::default();
    let body = parse_document(&document_xml, &rels, &numbering, &theme, &fonts, &media, &mut fs, &note_texts)?;
    let header = parse_hf_part(&mut archive, &document_xml, &rels, &numbering, &theme, &fonts, "headerReference", "hdr");
    let footer = parse_hf_part(&mut archive, &document_xml, &rels, &numbering, &theme, &fonts, "footerReference", "ftr");
    let mut section = parse_section_props(&document_xml);
    section.track_changes = revisions::read_track_revisions(&mut archive);
    Ok((body, header, footer, section))
}

/// Map relationship id → target URL (for hyperlinks).
pub(crate) fn parse_rels(xml: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Ok(doc) = XmlDoc::parse(xml) {
        for n in doc.descendants().filter(|n| local(n) == "Relationship") {
            if let (Some(id), Some(target)) = (attr_val(&n, "Id"), attr_val(&n, "Target")) {
                map.insert(id, target);
            }
        }
    }
    map
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn parse_document(
    xml: &str,
    rels: &HashMap<String, String>,
    numbering: &HashMap<String, bool>,
    theme: &HashMap<String, String>,
    fonts: &FontCtx,
    media: &HashMap<String, String>,
    fs: &mut FieldState,
    notes: &NoteTexts,
) -> Result<PmNode> {
    let doc = XmlDoc::parse(xml)
        .map_err(|e| OfficeError::Conversion(format!("XML invalide: {e}")))?;
    let body = doc
        .descendants()
        .find(|n| local(n) == "body")
        .ok_or_else(|| OfficeError::Conversion("w:body introuvable".into()))?;
    Ok(parse_container_doc(&body, rels, numbering, theme, fonts, media, fs, notes))
}

/// Parse a block container (`w:body`, `w:hdr`, `w:ftr`, table cell…) into a
/// ProseMirror `doc` node: paragraphs/tables + DrawingML shapes, lists folded.
#[allow(clippy::too_many_arguments)]
pub(crate) fn parse_container_doc(
    root: &Node<'_, '_>,
    rels: &HashMap<String, String>,
    numbering: &HashMap<String, bool>,
    theme: &HashMap<String, String>,
    fonts: &FontCtx,
    media: &HashMap<String, String>,
    fs: &mut FieldState,
    notes: &NoteTexts,
) -> PmNode {
    let mut blocks: Vec<Block> = Vec::new();
    collect_blocks(root, rels, numbering, theme, fonts, media, &mut blocks, 0, fs, notes);

    let mut nodes = fold_blocks(blocks);
    if nodes.is_empty() {
        nodes.push(PmNode::paragraph(vec![]));
    }
    PmNode::doc(nodes)
}

/// Walk a container's children, descending through `<w:sdt>` wrappers.
///
/// Word 2013+ wraps content controls — and, crucially, every table of contents —
/// in `<w:sdt><w:sdtContent>`. Only looking at direct `w:p`/`w:tbl` children
/// drops the whole block silently. `depth` guards against a pathologically
/// nested document.
#[allow(clippy::too_many_arguments)]
fn collect_blocks(
    root: &Node<'_, '_>,
    rels: &HashMap<String, String>,
    numbering: &HashMap<String, bool>,
    theme: &HashMap<String, String>,
    fonts: &FontCtx,
    media: &HashMap<String, String>,
    blocks: &mut Vec<Block>,
    depth: u8,
    fs: &mut FieldState,
    notes: &NoteTexts,
) {
    for node in root.children().filter(|n| n.is_element()) {
        match local(&node) {
            "sdt" if depth < 16 => {
                if let Some(content) = super::xml::child(&node, "sdtContent") {
                    collect_blocks(
                        &content, rels, numbering, theme, fonts, media, blocks, depth + 1, fs, notes,
                    );
                }
            }
            "p" => {
                // DrawingML shapes (`<w:drawing>`) → block image nodes (alt `kbshape:`).
                let drawings = parse_drawings(&node, theme, media);
                let block = parse_paragraph(&node, rels, numbering, fonts, theme, media, fs, notes);
                let para_empty = matches!(&block, Block::Node(n)
                    if n.node_type == "paragraph" && n.content.as_ref().is_none_or(|c| c.is_empty()));
                // When the paragraph carries ONLY shapes, drop the now-empty paragraph.
                if !para_empty || drawings.is_empty() {
                    blocks.push(block);
                }
                for d in drawings {
                    blocks.push(Block::Node(d));
                }
            }
            "tbl" => blocks.push(Block::Node(parse_table(&node, rels, numbering, fonts, theme, media, fs, notes))),
            _ => {}
        }
    }
}

/// A ProseMirror doc holding only empty paragraphs is considered empty.
pub(crate) fn pm_is_empty(doc: &PmNode) -> bool {
    doc.content.as_ref().is_none_or(|c| {
        c.iter().all(|n| {
            n.node_type == "paragraph" && n.content.as_ref().is_none_or(|cc| cc.is_empty())
        })
    })
}

/// Parse a referenced header/footer part (`<w:headerReference>`/`<w:footerReference>`
/// in the body `sectPr` → rels → `word/headerN.xml`). Returns None if absent/empty.
#[allow(clippy::too_many_arguments)]
pub(crate) fn parse_hf_part(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    document_xml: &str,
    rels: &HashMap<String, String>,
    numbering: &HashMap<String, bool>,
    theme: &HashMap<String, String>,
    fonts: &FontCtx,
    ref_tag: &str,
    root_tag: &str,
) -> Option<PmNode> {
    let doc = XmlDoc::parse(document_xml).ok()?;
    let refs: Vec<Node> = doc.descendants().filter(|n| local(n) == ref_tag).collect();
    // Prefer the "default" reference (else the first one).
    let rid = refs
        .iter()
        .find(|n| attr_val(n, "type").as_deref() == Some("default"))
        .or_else(|| refs.first())
        .and_then(|n| attr_val(n, "id"))?;
    let target = rels.get(&rid)?;
    let part_name = target.trim_start_matches('/').trim_start_matches("./").to_string();
    let part = format!("word/{part_name}");
    let xml = read_zip_entry(archive, &part)?;
    // Relationships OWNED by this part (hyperlinks/images of the header/footer).
    let hf_rels = read_zip_entry(archive, &format!("word/_rels/{part_name}.rels"))
        .as_deref()
        .map(parse_rels)
        .unwrap_or_default();
    let hf_media = build_media_map(archive, &hf_rels, "word");
    let pdoc = XmlDoc::parse(&xml).ok()?;
    let root = pdoc.descendants().find(|n| local(n) == root_tag)?;
    // A header/footer has its own field state: PAGE & co. become tokens there.
    let mut hf_fs = FieldState::for_header();
    let pm = parse_container_doc(&root, &hf_rels, numbering, theme, fonts, &hf_media, &mut hf_fs, &NoteTexts::default());
    if pm_is_empty(&pm) {
        None
    } else {
        Some(pm)
    }
}

/// Fold consecutive list items into properly nested bullet/ordered lists.
pub(crate) fn fold_blocks(blocks: Vec<Block>) -> Vec<PmNode> {
    struct ListFrame {
        ordered: bool,
        items: Vec<PmNode>,
    }

    /// Close open list frames down to `keep`, attaching each closed list to its
    /// parent item (or to the output when at the top level).
    fn close_lists(stack: &mut Vec<ListFrame>, keep: usize, out: &mut Vec<PmNode>) {
        while stack.len() > keep {
            let frame = stack.pop().expect("len > keep ≥ 0");
            let list_node = if frame.ordered {
                PmNode::ordered_list(frame.items)
            } else {
                PmNode::bullet_list(frame.items)
            };
            if let Some(parent) = stack.last_mut() {
                if let Some(last_item) = parent.items.last_mut() {
                    last_item.content.get_or_insert_with(Vec::new).push(list_node);
                } else {
                    parent.items.push(PmNode::list_item(vec![list_node]));
                }
            } else {
                out.push(list_node);
            }
        }
    }

    let mut out: Vec<PmNode> = Vec::new();
    let mut stack: Vec<ListFrame> = Vec::new();

    for block in blocks {
        match block {
            Block::Node(node) => {
                close_lists(&mut stack, 0, &mut out);
                out.push(node);
            }
            Block::Item { ordered, ilvl, para } => {
                let depth = ilvl as usize;
                // Collapse anything deeper than this item's level.
                close_lists(&mut stack, depth + 1, &mut out);
                // A list-type change at the same level starts a fresh list.
                if stack.len() == depth + 1 && stack[depth].ordered != ordered {
                    close_lists(&mut stack, depth, &mut out);
                }
                // Open intermediate levels up to this depth.
                while stack.len() < depth + 1 {
                    stack.push(ListFrame { ordered, items: Vec::new() });
                }
                stack[depth].items.push(PmNode::list_item(vec![para]));
            }
        }
    }
    close_lists(&mut stack, 0, &mut out);
    out
}

// ─── Static XML templates ────────────────────────────────────────────────────
