//! Word 97-2003 binary documents (`.doc`, the WW8 format).
//!
//! Nothing here is shared with the DOCX converter: a `.doc` is an OLE2 compound
//! file whose `WordDocument` stream begins with a File Information Block, and
//! whose text is scattered across the stream and reassembled through a piece
//! table. Formatting lives in separate "property bins" addressed by character
//! position, not inline with the text.
//!
//! Layout — one file per domain, mirroring `sw/source/filter/ww8/` of
//! LibreOffice, which is the reference implementation:
//!   `fib` FIB · `piece` piece table + decoding · `text` control characters →
//!   blocks · `sprm`/`fkp` property machinery · `chp`/`pap` character and
//!   paragraph properties · `style` stylesheet · `list` numbering ·
//!   `table` tables · `section` page setup · `field` fields/hyperlinks/bookmarks ·
//!   `drawing` pictures.

// Work-in-progress importer: the pipeline below is being wired up module by
// module, so some items are not reachable from `lib.rs` yet.
#![allow(dead_code)]

pub(crate) mod assemble;
pub(crate) mod chp;
pub(crate) mod drawing;
pub(crate) mod fib;
pub(crate) mod field;
pub(crate) mod fkp;
pub(crate) mod list;
pub(crate) mod pap;
pub(crate) mod piece;
pub(crate) mod revisions;
pub(crate) mod section;
pub(crate) mod sprm;
pub(crate) mod style;
pub(crate) mod table;
pub(crate) mod hf;
pub(crate) mod text;
pub(crate) mod toc;

#[cfg(test)]
mod tests;

use std::io::Cursor;

use crate::converters::docx::SectionInfo;
use crate::converters::types::PmNode;
use crate::errors::{OfficeError, Result};

/// Read a `.doc` → (body, header, footer, page layout).
///
/// Signature mirrors `import_docx` so the handlers can treat both the same way.
pub fn import_doc(data: &[u8]) -> Result<(PmNode, Option<PmNode>, Option<PmNode>, SectionInfo)> {
    let mut comp = cfb::CompoundFile::open(Cursor::new(data))
        .map_err(|e| OfficeError::Conversion(format!("conteneur OLE2 invalide: {e}")))?;

    let doc_stream = read_stream(&mut comp, "WordDocument")
        .ok_or_else(|| OfficeError::Conversion("flux WordDocument introuvable".into()))?;

    let fib = fib::Fib::parse(&doc_stream)?;
    if fib.encrypted {
        return Err(OfficeError::Conversion(
            "document protégé par mot de passe : lecture impossible".into(),
        ));
    }

    let table = read_stream(&mut comp, fib.table_stream_name()).unwrap_or_default();
    // The `Data` stream holds the "huge PAPX" indirection: a paragraph whose
    // property list is too large for its 512-byte page stores it here instead.
    // Without it those paragraphs silently lose their formatting.
    let data = read_stream(&mut comp, "Data").unwrap_or_default();
    let pieces = piece::parse_pieces(&fib, &table);
    let enc = piece::charset_for(&fib);
    let doc_text = piece::read_text(&fib, &doc_stream, &pieces, enc);

    // Each layer reads one thing; `assemble` is where they meet.
    let props = fkp::read_props(&fib, &doc_stream, &table, &data, &pieces);
    let styles = style::StyleSheet::parse(&fib, &doc_stream, &table);
    let font_table = style::FontTable::parse(&fib, &doc_stream, &table);
    let lists = list::ListTable::parse(&fib, &table);
    let notes = field::Annotations::main(&fib, &table, &doc_text.chars, enc);
    let pics = drawing::Pictures::load(&fib, &doc_stream, &table, data);
    let paras = text::split_paragraphs(&doc_text.chars);
    // The TOC plan needs the paragraphs, so it is built before the context.
    let toc_plan = toc::TocPlan::main(&fib, &table, &doc_text.chars, &paras, &props, &styles);
    let revs = revisions::Revisions::parse(&fib, &doc_stream, &table, &props, sprm::SprmVersion::from_fib(&fib));
    let ctx = assemble::Context {
        props: &props,
        styles: &styles,
        fonts: assemble::Context::font_names(&font_table),
        lists: &lists,
        notes: &notes,
        pics: &pics,
        toc: &toc_plan,
        revs: &revs,
        ver: sprm::SprmVersion::from_fib(&fib),
    };
    let body = assemble::build_body(&ctx, &doc_text.chars, &paras);

    // Page layout. Unlike DOCX — where the body's `sectPr` is the LAST section —
    // a `.doc` puts the section descriptors in reading order, so the first one
    // describes the start of the document.
    let mut section = section::parse_section_props(&fib, &doc_stream, &table);
    // `Dop.fRevMarking` — Word's "track changes while editing", the `.doc`
    // counterpart of `w:trackRevisions`.
    section.track_changes = revs.track_changes();

    // Headers and footers live in the SAME character stream as the body, after
    // the main text; `PlcfHdd` says where each slice starts.
    let (header, footer) = hf::read_first_section(
        &hf::HfInput { fib: &fib, doc: &doc_stream, table: &table, pieces: &pieces, enc },
        &ctx,
    );

    Ok((body, header, footer, section))
}

/// Read a whole stream by name, tolerating its absence.
fn read_stream(comp: &mut cfb::CompoundFile<Cursor<&[u8]>>, name: &str) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut s = comp.open_stream(name).ok()?;
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).ok()?;
    Some(buf)
}
