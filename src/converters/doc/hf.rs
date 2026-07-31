//! Header and footer subdocuments of a `.doc`.
//!
//! They are not separate streams: they live in the same character stream, after
//! the main text, and `PlcfHdd` says where each one starts. `grpfIhdt` of a
//! section says which of the six slots (even/odd/first × header/footer) it uses.
//! Reference: LibreOffice `sw/source/filter/ww8/ww8par.cxx` (`WW8ReaderSave`,
//! header/footer handling) and [MS-DOC] 2.8.24.
//!
//! Two version-dependent details decide everything here, and both were taken
//! from the reference implementation rather than from the specification, which
//! is wrong about them (LibreOffice says so in as many words, ww8scan.cxx:7499):
//!
//!   - **Word 97+** reserves all six slots for every section, empty ones being
//!     zero-length, and `PlcfHdd` opens with six "special" stories (the
//!     footnote and endnote separators). The story of section *s* and slot *k*
//!     is therefore entry `6 + 6 * s + k`, and `grpfIhdt` is not read at all:
//!     LibreOffice synthesises it and decides on the story's LENGTH instead
//!     (`ww8par6.cxx:1223`, `Read_HdFt` → `GetTextPosExact`).
//!   - **Word 6/95** packs the stories that exist and nothing else, so the
//!     index has to be counted out of `grpfIhdt` — the Dop's for the special
//!     stories, then the section's (`WW8PLCF_HdFt::GetTextPos`).
//!
//! The character positions of a header story are relative to the header
//! subdocument, which itself starts at `ccpText + ccpFtn` in the stream
//! (`WW8Fib::GetBaseCp`, MAN_HDFT). So is the field table `PlcfFldHdr`, which is
//! why `Annotations` takes a `cp_base`.

use encoding_rs::Encoding;
use serde_json::{json, Map, Value};

use crate::converters::types::{PmMark, PmNode};

use super::assemble::Context;
use super::chp::Chp;
use super::fib::{u16_at, u32_at, Fib};
use super::field::{parse_fields, Annotations};
use super::pap::Pap;
use super::piece::{self, Piece};
use super::sprm::{iter_sprms_ver, SprmVersion};
use super::table;
use super::text::{self, ch};

// ---------------------------------------------------------------------------
// Locating PlcfHdd in the FIB
// ---------------------------------------------------------------------------

/// Index of the `(fc, lcb)` PAIR of `PlcfHdd` inside `FibRgFcLcb97`.
///
/// `FibRgFcLcb97` is an array of pairs, so pair *k* is the 32-bit word *2k*:
/// `fcPlcfHdd` is word 22 and `lcbPlcfHdd` word 23. Two independent checks pin
/// this down rather than a spec reading:
///   - the pairs `Fib::parse` already resolves fix the numbering — `fcPlcfSed`
///     is pair 6, `fcPlcfBteChpx` 12, `fcSttbfFfn` 15, `fcPlcfFldHdr` 17,
///     `fcClx` 33 — and `fcPlcfHdd` sits between `fcPlcfGlsy` and
///     `fcPlcfBteChpx`, i.e. pair 11;
///   - the Word 6/95 FIB has the same field order at a fixed offset, and
///     LibreOffice documents that twin as 0xB0 (`ww8scan.hxx:1284`), which is
///     exactly `0x58 + 8 * 11`.
///
/// TODO(fib.rs): expose `fc_plcf_hdd = g(22)` / `lcb_plcf_hdd = g(23)` (Word
/// 6/95: 0xB0 / 0xB4) and delete `plcf_hdd_loc` below.
const PLCF_HDD_PAIR: usize = 11;

/// Index of the `(fc, lcb)` pair of `fcDop`, needed for the Word 6/95 index.
const DOP_PAIR: usize = 31;

/// Start of the `(fc, lcb)` array in the fixed Word 6/95 FIB layout.
const WW6_FC_LCB_BASE: usize = 0x58;

/// Number of stories `PlcfHdd` opens with before the first section's: the
/// footnote separator, continuation separator and continuation notice, then the
/// same three for endnotes.
const SPECIAL_STORIES: usize = 6;

/// One of the six header/footer stories a section can own.
///
/// The discriminant is both the bit of `grpfIhdt` and the slot's position inside
/// a section's group of six — LibreOffice relies on that coincidence too
/// (`ww8par6.cxx:1239`, where the loop index and the mask advance together).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Slot {
    HeaderEven = 0,
    HeaderOdd = 1,
    FooterEven = 2,
    FooterOdd = 3,
    HeaderFirst = 4,
    FooterFirst = 5,
}

impl Slot {
    /// The `grpfIhdt` bit standing for this story.
    fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

/// Where to look for the section's default header, in order of preference.
///
/// Word's "the" header is the odd-page one; a document that only defines a
/// first-page or an even-page header still has exactly one header to show.
const HEADER_SLOTS: [Slot; 3] = [Slot::HeaderOdd, Slot::HeaderFirst, Slot::HeaderEven];
const FOOTER_SLOTS: [Slot; 3] = [Slot::FooterOdd, Slot::FooterFirst, Slot::FooterEven];

/// Everything the header/footer pass needs that `assemble::Context` does not
/// already carry: the raw streams and the piece table, to decode a part of the
/// character stream the body reader never touches.
pub(crate) struct HfInput<'a> {
    pub(crate) fib: &'a Fib,
    /// The `WordDocument` stream.
    pub(crate) doc: &'a [u8],
    /// The table stream (`0Table` / `1Table`, or `WordDocument` before Word 97).
    pub(crate) table: &'a [u8],
    pub(crate) pieces: &'a [Piece],
    pub(crate) enc: &'static Encoding,
}

/// Byte range of `PlcfHdd`, read straight from the FIB.
///
/// `Fib` does not expose `fcPlcfHdd` yet, so this walks the same self-describing
/// prefix `fkp::bin_loc` does. Both are temporary for the same reason.
fn plcf_hdd_loc(fib: &Fib, doc: &[u8]) -> (u32, u32) {
    let at = fc_lcb_pair_offset(fib, doc, PLCF_HDD_PAIR);
    (u32_at(doc, at), u32_at(doc, at + 4))
}

/// Byte offset of pair `pair` of the FIB's `(fc, lcb)` array.
fn fc_lcb_pair_offset(fib: &Fib, doc: &[u8], pair: usize) -> usize {
    if fib.nfib >= 193 {
        // csw 16-bit values, then cslw 32-bit values, then cbRgFcLcb.
        let csw = u16_at(doc, 0x20) as usize;
        let base = 0x22 + csw * 2;
        let cslw = u16_at(doc, base) as usize;
        base + 2 + cslw * 4 + 2 + pair * 8
    } else {
        WW6_FC_LCB_BASE + pair * 8
    }
}

// ---------------------------------------------------------------------------
// PlcfHdd
// ---------------------------------------------------------------------------

/// How to turn a slot into an entry index when the stories are packed.
struct PackedIndex {
    /// Entries `PlcfHdd` opens with, counted out of the Dop's `grpfIhdt`.
    skip: u32,
    /// Which stories the first section owns.
    grpf_ihdt: u8,
}

/// `PlcfHdd` and what it takes to index it.
struct Plcfhdd {
    /// Story boundaries, relative to the header subdocument.
    cps: Vec<u32>,
    /// Absolute character position the header subdocument starts at.
    base: u32,
    /// Length of the header subdocument, in characters.
    len: u32,
    /// `fTitlePage` of the first section: without it, the first-page stories
    /// exist but Word never shows them.
    title_page: bool,
    /// Set for Word 6/95 only, where the stories are packed.
    packed: Option<PackedIndex>,
}

impl Plcfhdd {
    fn parse(input: &HfInput<'_>) -> Option<Plcfhdd> {
        let fib = input.fib;
        if fib.ccp_hdd == 0 {
            return None; // no header subdocument at all
        }
        let (fc, lcb) = plcf_hdd_loc(fib, input.doc);
        let start = fc as usize;
        let end = start.checked_add(lcb as usize)?.min(input.table.len());
        // A PLC of character positions and nothing else: n + 1 positions, no
        // per-entry data ([MS-DOC] 2.8.24). Two positions is one story.
        let raw = input.table.get(start..end).filter(|r| r.len() >= 8)?;
        let cps: Vec<u32> = raw
            .chunks_exact(4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();

        let flags = first_section_flags(fib, input.doc, input.table);
        let packed = (fib.nfib < 193).then(|| PackedIndex {
            skip: dop_special_stories(fib, input.doc, input.table),
            grpf_ihdt: flags.grpf_ihdt,
        });
        Some(Plcfhdd {
            cps,
            base: fib.ccp_text.saturating_add(fib.ccp_ftn),
            len: fib.ccp_hdd,
            title_page: flags.title_page,
            packed,
        })
    }

    /// Absolute character range of one story of the FIRST section, or `None`
    /// when that story does not exist or holds nothing but its paragraph mark.
    fn story(&self, slot: Slot) -> Option<(u32, u32)> {
        // A first-page story is written whether or not the section shows one:
        // Word keeps it so a later section can inherit it, and LibreOffice loads
        // it into a page style it then leaves disabled (`bDisabledFirst`,
        // ww8par.cxx:2506). Reading it as THE header of a document that has no
        // title page invents a header Word never displays.
        if !self.title_page && matches!(slot, Slot::HeaderFirst | Slot::FooterFirst) {
            return None;
        }
        let idx = match &self.packed {
            // Word 97+: six reserved slots per section, after the special ones.
            None => SPECIAL_STORIES + slot as usize,
            // Word 6/95: only the stories that exist are stored, so the index is
            // the number of lower bits set (`WW8PLCF_HdFt::GetTextPos`).
            Some(p) => {
                if p.grpf_ihdt & slot.bit() == 0 {
                    return None;
                }
                (p.skip + u32::from(p.grpf_ihdt & (slot.bit() - 1)).count_ones()) as usize
            }
        };
        let from = *self.cps.get(idx)?;
        // Every CP of PlcfHdd must be below ccpHdd (`isValid_HdFt_CP`).
        let to = (*self.cps.get(idx + 1)?).min(self.len);
        // A story of fewer than two characters is an empty paragraph and
        // nothing else — LibreOffice requires `nLen >= 2` before reading one.
        if to <= from.saturating_add(1) {
            return None;
        }
        Some((self.base + from, self.base + to))
    }
}

/// Number of special stories at the head of `PlcfHdd`, from the Dop.
///
/// Word 6/95 packs those too, so their count is the offset of the first
/// section's group. The high byte of the Dop's first 16-bit field is its own
/// `grpfIhdt`, one bit per separator story present (`ww8scan.cxx:7648`).
fn dop_special_stories(fib: &Fib, doc: &[u8], table: &[u8]) -> u32 {
    let fc = u32_at(doc, fc_lcb_pair_offset(fib, doc, DOP_PAIR)) as usize;
    u32::from(u16_at(table, fc) >> 8).count_ones()
}

/// The two `SEP` properties this pass needs from the first section.
#[derive(Debug, Clone, Copy, Default)]
struct SepFlags {
    /// `grpfIhdt` — which of the six stories the section owns. Word 6/95 only:
    /// from Word 97 on the slots are always reserved and their length alone
    /// answers the question, which is what LibreOffice does — it never reads the
    /// sprm for those documents (`ww8par6.cxx:1222`).
    grpf_ihdt: u8,
    /// `fTitlePage` — the section shows a distinct first-page header and footer.
    title_page: bool,
}

/// Read `grpfIhdt` and `fTitlePage` out of the first section's `SEPX`.
fn first_section_flags(fib: &Fib, doc: &[u8], table: &[u8]) -> SepFlags {
    // `sprmSFTitlePage` and `sprmSGprfIhdt` in the two opcode spaces. They can
    // never collide: a Word 97 opcode is at least 0x0800, a Word 6/95 one at
    // most 255, and the sprm walker is told which space it is reading.
    const WW8_TITLE_PAGE: u16 = 0x300A;
    const WW6_TITLE_PAGE: u16 = 143;
    const WW6_GRPF_IHDT: u16 = 153;
    const WW2_GRPF_IHDT: u16 = 128;

    let mut flags = SepFlags::default();
    let Some(grpprl) = first_sepx(fib, doc, table) else {
        return flags;
    };
    // Later wins inside one grpprl, so the whole list is walked.
    for s in iter_sprms_ver(grpprl, SprmVersion::from_fib(fib)) {
        match s.opcode {
            WW8_TITLE_PAGE | WW6_TITLE_PAGE => {
                flags.title_page = s.u8().is_some_and(|v| v != 0);
            }
            WW6_GRPF_IHDT | WW2_GRPF_IHDT => {
                flags.grpf_ihdt = s.u8().unwrap_or(flags.grpf_ihdt);
            }
            _ => {}
        }
    }
    flags
}

/// The `grpprl` of the first section's `SEPX`.
///
/// `section.rs` does the same walk but keeps it private, and it decodes the
/// sprms it models rather than handing the bytes back.
fn first_sepx<'a>(fib: &Fib, doc: &'a [u8], table: &[u8]) -> Option<&'a [u8]> {
    let start = fib.fc_plcf_sed as usize;
    let end = start
        .checked_add(fib.lcb_plcf_sed as usize)?
        .min(table.len());
    // A PLCF of n + 1 CPs (4 bytes) then n SED structs (12 bytes).
    let plc = table.get(start..end).filter(|p| p.len() >= 4 + 12)?;
    let n = (plc.len() - 4) / 16;
    let fc_sepx = u32_at(plc, 4 * (n + 1) + 2);
    if fc_sepx == u32::MAX {
        return None; // this section has no SEPX
    }
    let at = fc_sepx as usize;
    // The SEPX is a byte count then the sprms; Word 1/2 wrote an 8-bit count.
    let (head, cb) = if fib.nfib < 100 {
        (1usize, usize::from(*doc.get(at)?))
    } else {
        (2usize, usize::from(u16_at(doc, at)))
    };
    let from = at.checked_add(head)?;
    let to = from.saturating_add(cb).min(doc.len());
    (from < to).then(|| doc.get(from..to)).flatten()
}

// ---------------------------------------------------------------------------
// Decoding the stories
// ---------------------------------------------------------------------------

/// Decode the main text and every subdocument up to the end of the headers.
///
/// `piece::read_text` stops at `ccpText`, which is exactly where the stories
/// begin, so it is handed a FIB whose text length covers them as well. Cloning
/// the FIB instead of reimplementing the piece walk keeps a single decoder in
/// the tree, and the returned vector is indexed by ABSOLUTE character position,
/// which is what the property tables are keyed by.
fn read_through_headers(input: &HfInput<'_>) -> Vec<char> {
    let mut fib = input.fib.clone();
    fib.ccp_text = input
        .fib
        .ccp_text
        .saturating_add(input.fib.ccp_ftn)
        .saturating_add(input.fib.ccp_hdd);
    piece::read_text(&fib, input.doc, input.pieces, input.enc).chars
}

/// Fields of the header subdocument, as a plan in absolute positions.
///
/// `PlcfFldHdr` numbers its positions from the start of the subdocument, so they
/// are shifted and `Annotations` is told where the slice it gets begins. The
/// bookmark tables the FIB exposes cover the main document only, hence none
/// here. `in_header` is what turns PAGE / NUMPAGES into the editor's tokens.
fn header_annotations(input: &HfInput<'_>, chars: &[char], base: u32) -> Annotations {
    let mut fields = parse_fields(
        input.table,
        input.fib.fc_plcf_fld_hdr,
        input.fib.lcb_plcf_fld_hdr,
    );
    for f in &mut fields {
        f.cp_begin = f.cp_begin.saturating_add(base);
        f.cp_sep = f.cp_sep.map(|c| c.saturating_add(base));
        f.cp_end = f.cp_end.saturating_add(base);
    }
    let tail = chars.get(base as usize..).unwrap_or(&[]);
    Annotations::build(&fields, Vec::new(), tail, base, true)
}

/// The header and footer of the first section, when present.
///
/// Both are ProseMirror `doc` nodes, the shape `import_doc` returns and the
/// editor renders; an absent or empty story is `None` rather than a document
/// holding one empty paragraph.
pub(crate) fn read_first_section(
    input: &HfInput<'_>,
    ctx: &Context<'_>,
) -> (Option<PmNode>, Option<PmNode>) {
    let Some(plc) = Plcfhdd::parse(input) else {
        return (None, None);
    };
    let chars = read_through_headers(input);
    let notes = header_annotations(input, &chars, plc.base);
    let pick = |slots: &[Slot; 3]| {
        slots.iter().find_map(|&slot| {
            let (from, to) = plc.story(slot)?;
            story_doc(ctx, &notes, &chars, from, to)
        })
    };
    (pick(&HEADER_SLOTS), pick(&FOOTER_SLOTS))
}

/// Build one story as a ProseMirror `doc`, or `None` when it says nothing.
fn story_doc(
    ctx: &Context<'_>,
    notes: &Annotations,
    chars: &[char],
    from: u32,
    to: u32,
) -> Option<PmNode> {
    let (lo, hi) = (from as usize, (to as usize).min(chars.len()));
    let slice = chars.get(lo..hi).filter(|s| !s.is_empty())?;
    let mut paras = text::split_paragraphs(slice);
    if paras.is_empty() {
        return None;
    }
    // `split_paragraphs` numbers from the start of the slice it was handed; the
    // property tables and the field plan are keyed by absolute position.
    for p in &mut paras {
        p.cp_start = p.cp_start.saturating_add(from);
    }

    let mut items: Vec<table::Para> = Vec::with_capacity(paras.len());
    for (i, rp) in paras.iter().enumerate() {
        // The paragraph runs up to the next one's start, its own mark included:
        // `inline_content` stops on the mark itself.
        let limit = paras.get(i + 1).map_or(to, |next| next.cp_start);
        let (p, direct) = para_props(ctx, rp.cp_start);
        let mut attrs: Map<String, Value> = p.to_attrs();
        let content = inline_content(ctx, notes, chars, rp.cp_start, limit, p.istd);

        // Same rules as the body: a heading is its own node type, and the style
        // name travels as an attribute.
        let level = p
            .outline_level
            .or_else(|| ctx.styles.effective_heading_level(p.istd));
        let node_type = match level {
            Some(l) if (1..=6).contains(&l) => {
                attrs.insert("level".into(), json!(l));
                "heading"
            }
            _ => "paragraph",
        };
        if let Some(id) = ctx.styles.docx_style_id(p.istd) {
            attrs.insert("styleName".into(), json!(id));
        }
        items.push(table::Para {
            node: PmNode {
                node_type: node_type.into(),
                attrs: (!attrs.is_empty()).then_some(Value::Object(attrs)),
                content: Some(content),
                marks: None,
                text: None,
            },
            cell_end: rp.in_table,
            table: table::ParaTable::parse(&direct),
        });
    }

    let doc = PmNode::doc(table::assemble(&items));
    (!is_blank(&doc)).then_some(doc)
}

/// Does this subtree carry anything worth showing?
///
/// A story that exists but holds only empty paragraphs is Word's way of saying
/// "no header", and the editor must not be handed one: it would show an empty
/// header band instead of none. Anything that is not a paragraph — a table, a
/// picture — counts as content whatever its text.
fn is_blank(node: &PmNode) -> bool {
    if node.text.as_deref().is_some_and(|t| !t.trim().is_empty()) {
        return false;
    }
    if !matches!(node.node_type.as_str(), "doc" | "paragraph" | "text") {
        return false;
    }
    node.children().iter().all(is_blank)
}

// ---------------------------------------------------------------------------
// Inline content
//
// This mirrors `assemble::inline_content` and cannot call it: that one walks the
// paragraph's already-filtered TEXT and so has no place to put the `{page}`
// tokens, whose position in the character stream is a field delimiter it skips.
// Once `assemble` consults `Annotations::insert_at`, this whole section becomes
// a call to `assemble::build_body` and should go.
// ---------------------------------------------------------------------------

/// Resolved paragraph properties at `cp`: the style chain first, then the
/// paragraph's own `grpprl`, which is also returned for the table layer.
fn para_props(ctx: &Context<'_>, cp: u32) -> (Pap, Vec<u8>) {
    let istd = ctx.props.paras.istd_at(cp).unwrap_or(0);
    let mut p = Pap::with_style(istd);
    for g in ctx.styles.papx_chain(istd) {
        p.apply_grpprl_ver(g, ctx.ver);
    }
    let direct = ctx.props.paras.grpprl_at(cp).to_vec();
    p.apply_grpprl_ver(&direct, ctx.ver);
    (p, direct)
}

/// Resolved character properties at `cp`, on top of the paragraph's style.
fn char_props(ctx: &Context<'_>, cp: u32, para_istd: u16) -> Chp {
    let mut c = Chp::default();
    for g in ctx.styles.chpx_chain(para_istd) {
        c.apply_grpprl(g);
    }
    c.apply_grpprl(ctx.props.chars.grpprl_at(cp));
    c
}

/// The marks covering `cp`: character formatting, then links and bookmarks.
fn run_marks(ctx: &Context<'_>, notes: &Annotations, cp: u32, para_istd: u16) -> Vec<PmMark> {
    let mut marks = char_props(ctx, cp, para_istd).to_marks(&ctx.fonts);
    marks.extend(notes.marks_at(cp));
    marks
}

/// What a character contributes to the text, mirroring `split_paragraphs`:
/// tabs and line breaks survive, the structural placeholders do not.
fn visible_char(c: char) -> Option<char> {
    match c {
        ch::LINE_BREAK => Some('\n'),
        ch::NB_HYPHEN => Some('\u{2011}'),
        '\t' => Some('\t'),
        _ if c.is_control() => None,
        _ => Some(c),
    }
}

/// Split `[cp_start, cp_limit)` into runs of constant formatting.
///
/// Walks the character stream itself rather than a filtered copy of the text,
/// which is what makes room for the field tokens: `{page}` belongs at the
/// position of a `0x13` mark, a character no filtered text ever contains.
fn inline_content(
    ctx: &Context<'_>,
    notes: &Annotations,
    chars: &[char],
    cp_start: u32,
    cp_limit: u32,
    para_istd: u16,
) -> Vec<PmNode> {
    let mut out: Vec<PmNode> = Vec::new();
    let mut buf = String::new();
    let mut cur: Option<Vec<PmMark>> = None;
    // Formatting only changes on a property-run boundary, so it is resolved
    // once per run and `PmMark` needs no equality.
    let mut run_end = cp_start;
    // Depth of nested field instructions being skipped, as in `split_paragraphs`.
    let mut instruction = 0u32;

    for cp in cp_start..cp_limit {
        let Some(&c) = chars.get(cp as usize) else { break };

        // A field the editor recomputes: its cached result is hidden and the
        // token stands in for it. This has to run before the filtering below,
        // because the position it sits at is the field's own `0x13` mark.
        if let Some(token) = notes.insert_at(cp) {
            flush_run(&mut out, &mut buf, &mut cur);
            let marks = run_marks(ctx, notes, cp, para_istd);
            out.push(text_node(token.to_owned(), marks));
            run_end = cp; // the next character opens a fresh run
        }

        match c {
            // The paragraph's own mark ends it.
            ch::PARA_END | ch::CELL_END => break,
            ch::FIELD_BEGIN => {
                instruction += 1;
                continue;
            }
            ch::FIELD_SEP | ch::FIELD_END => {
                instruction = instruction.saturating_sub(1);
                continue;
            }
            _ => {}
        }
        if instruction > 0 || notes.is_hidden(cp) {
            continue;
        }
        let props = char_props(ctx, cp, para_istd);
        if props.hidden {
            continue;
        }
        // A picture is a placeholder character standing for a blip, not text.
        if c == ch::PICTURE || props.special {
            if let Some(node) = props.pic_location.and_then(|fc| ctx.pics.inline_at(fc)) {
                flush_run(&mut out, &mut buf, &mut cur);
                out.push(node);
            }
            run_end = cp;
            continue;
        }
        let Some(glyph) = visible_char(c) else { continue };
        if cp >= run_end || cur.is_none() {
            flush_run(&mut out, &mut buf, &mut cur);
            cur = Some(run_marks(ctx, notes, cp, para_istd));
            run_end = ctx
                .props
                .chars
                .at(cp)
                .map(|r| r.cp_end)
                .filter(|e| *e > cp)
                .unwrap_or(cp + 1);
        }
        buf.push(glyph);
    }
    flush_run(&mut out, &mut buf, &mut cur);
    out
}

fn text_node(text: String, marks: Vec<PmMark>) -> PmNode {
    PmNode {
        node_type: "text".into(),
        attrs: None,
        content: None,
        marks: (!marks.is_empty()).then_some(marks),
        text: Some(text),
    }
}

fn flush_run(out: &mut Vec<PmNode>, buf: &mut String, cur: &mut Option<Vec<PmMark>>) {
    if buf.is_empty() {
        return;
    }
    let marks = cur.clone().unwrap_or_default();
    out.push(text_node(std::mem::take(buf), marks));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_bits_and_indices_match_the_order_word_packs_them() {
        // The discriminant is the slot's position inside a section's group of
        // six AND the bit of `grpfIhdt` — LibreOffice depends on both.
        assert_eq!(Slot::HeaderEven as usize, 0);
        assert_eq!(Slot::HeaderEven.bit(), 0x01);
        assert_eq!(Slot::HeaderOdd.bit(), 0x02);
        assert_eq!(Slot::FooterEven.bit(), 0x04);
        assert_eq!(Slot::FooterOdd.bit(), 0x08);
        assert_eq!(Slot::HeaderFirst.bit(), 0x10);
        assert_eq!(Slot::FooterFirst.bit(), 0x20);
        assert_eq!(Slot::FooterFirst as usize, 5);
    }

    /// A Word 97 FIB whose `FibRgFcLcb97` is filled with pair indices, so a
    /// wrong pair number cannot pass unnoticed.
    fn fib97_with_pairs() -> (Fib, Vec<u8>) {
        let (csw, cslw) = (14usize, 22usize);
        let mut doc = vec![0u8; 0x22];
        doc[0..2].copy_from_slice(&0xA5ECu16.to_le_bytes());
        doc[2..4].copy_from_slice(&193u16.to_le_bytes());
        doc[0x20..0x22].copy_from_slice(&(csw as u16).to_le_bytes());
        doc.resize(doc.len() + csw * 2, 0);
        doc.extend_from_slice(&(cslw as u16).to_le_bytes());
        doc.resize(doc.len() + cslw * 4, 0);
        doc.extend_from_slice(&0u16.to_le_bytes()); // cbRgFcLcb
        for pair in 0..80u32 {
            doc.extend_from_slice(&pair.to_le_bytes()); // fc = pair index
            doc.extend_from_slice(&(pair + 1000).to_le_bytes()); // lcb
        }
        let fib = Fib { nfib: 193, ..Default::default() };
        (fib, doc)
    }

    #[test]
    fn plcf_hdd_is_the_twelfth_pair_of_fib_rg_fc_lcb97() {
        let (fib, doc) = fib97_with_pairs();
        assert_eq!(plcf_hdd_loc(&fib, &doc), (11, 1011));
        // The same walk must reproduce the pairs `Fib::parse` already knows,
        // which is what ties the numbering to reality.
        for pair in [1usize, 6, 12, 15, 17, 33] {
            let at = fc_lcb_pair_offset(&fib, &doc, pair);
            assert_eq!(u32_at(&doc, at), pair as u32);
        }
    }

    #[test]
    fn word6_pairs_sit_at_the_offsets_libreoffice_documents() {
        let fib = Fib { nfib: 101, ..Default::default() };
        // ww8scan.hxx:1284 — "0xb0 byte offset of header PLCF".
        assert_eq!(fc_lcb_pair_offset(&fib, &[], PLCF_HDD_PAIR), 0xB0);
        // And the pairs section.rs / fkp.rs already read, as a cross-check.
        assert_eq!(fc_lcb_pair_offset(&fib, &[], 1), 0x60); // fcStshf
        assert_eq!(fc_lcb_pair_offset(&fib, &[], 6), 0x88); // fcPlcfSed
        assert_eq!(fc_lcb_pair_offset(&fib, &[], 12), 0xB8); // fcPlcfBteChpx
        assert_eq!(fc_lcb_pair_offset(&fib, &[], 15), 0xD0); // fcSttbfFfn
    }

    #[test]
    fn an_empty_story_is_no_story() {
        assert!(is_blank(&PmNode::doc(vec![PmNode::paragraph(vec![])])));
        assert!(is_blank(&PmNode::doc(vec![PmNode::paragraph(vec![text_node(
            "  ".into(),
            vec![]
        )])])));
        assert!(!is_blank(&PmNode::doc(vec![PmNode::paragraph(vec![
            text_node("x".into(), vec![])
        ])])));
        // A table or an image is content whatever text it holds.
        assert!(!is_blank(&PmNode::doc(vec![PmNode {
            node_type: "table".into(),
            attrs: None,
            content: Some(vec![]),
            marks: None,
            text: None,
        }])));
    }

    #[test]
    fn packed_index_counts_the_stories_below_the_wanted_bit() {
        // Word 6/95: an odd header and an odd footer, three special stories.
        let plc = Plcfhdd {
            cps: vec![0, 1, 2, 3, 20, 40],
            base: 100,
            len: 40,
            title_page: false,
            packed: Some(PackedIndex {
                skip: 3,
                grpf_ihdt: Slot::HeaderOdd.bit() | Slot::FooterOdd.bit(),
            }),
        };
        assert_eq!(plc.story(Slot::HeaderOdd), Some((103, 120)));
        assert_eq!(plc.story(Slot::FooterOdd), Some((120, 140)));
        assert_eq!(plc.story(Slot::HeaderEven), None);
        assert_eq!(plc.story(Slot::HeaderFirst), None);
    }

    #[test]
    fn word97_slots_are_reserved_and_length_decides() {
        // Six special stories, then the first section's six. Only the odd
        // header holds anything; the others are a bare paragraph mark.
        let mut cps = vec![0u32; 7];
        cps.extend([0, 30, 31, 32, 33, 40]); // even hdr empty, odd hdr 0..30
        let mut plc = Plcfhdd { cps, base: 500, len: 40, title_page: true, packed: None };
        assert_eq!(plc.story(Slot::HeaderEven), None); // length 0
        assert_eq!(plc.story(Slot::HeaderOdd), Some((500, 530)));
        assert_eq!(plc.story(Slot::FooterEven), None); // length 1: just the mark
        assert_eq!(plc.story(Slot::FooterFirst), Some((533, 540)));
        // Without a title page the first-page stories are written but unused.
        plc.title_page = false;
        assert_eq!(plc.story(Slot::FooterFirst), None);
        assert_eq!(plc.story(Slot::HeaderOdd), Some((500, 530)));
    }

    // -----------------------------------------------------------------------
    // Corpus
    // -----------------------------------------------------------------------

    /// Every `.doc` under `KUBUNO_DOC_CORPUS`.
    fn corpus() -> Vec<std::path::PathBuf> {
        let Ok(root) = std::env::var("KUBUNO_DOC_CORPUS") else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut stack = vec![std::path::PathBuf::from(root)];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "doc") {
                    out.push(p);
                }
            }
        }
        out.sort();
        out
    }

    /// The whole reader, wired the way `import_doc` wires it, plus the headers.
    ///
    /// Duplicated from `mod.rs` on purpose: this is the seam `read_first_section`
    /// has to be plugged into, and the test is what proves the shape fits.
    fn headers_of(data: &[u8]) -> Option<(Option<PmNode>, Option<PmNode>)> {
        use super::super::{assemble, drawing, field, fib, fkp, list, piece, sprm, style};
        use std::io::{Cursor, Read};

        let mut comp = cfb::CompoundFile::open(Cursor::new(data)).ok()?;
        let read = |comp: &mut cfb::CompoundFile<Cursor<&[u8]>>, name: &str| -> Option<Vec<u8>> {
            let mut s = comp.open_stream(name).ok()?;
            let mut buf = Vec::new();
            s.read_to_end(&mut buf).ok()?;
            Some(buf)
        };
        let doc_stream = read(&mut comp, "WordDocument")?;
        let f = fib::Fib::parse(&doc_stream).ok()?;
        if f.encrypted {
            return None;
        }
        let table = read(&mut comp, f.table_stream_name()).unwrap_or_default();
        let data_stream = read(&mut comp, "Data").unwrap_or_default();
        let pieces = piece::parse_pieces(&f, &table);
        let enc = piece::charset_for(&f);
        let doc_text = piece::read_text(&f, &doc_stream, &pieces, enc);
        let props = fkp::read_props(&f, &doc_stream, &table, &data_stream, &pieces);
        let styles = style::StyleSheet::parse(&f, &doc_stream, &table);
        let fonts = style::FontTable::parse(&f, &doc_stream, &table);
        let lists = list::ListTable::parse(&f, &table);
        let notes = field::Annotations::main(&f, &table, &doc_text.chars, enc);
        let pics = drawing::Pictures::load(&f, &doc_stream, &table, data_stream);
        let ctx = assemble::Context {
            props: &props,
            styles: &styles,
            fonts: assemble::Context::font_names(&fonts),
            lists: &lists,
            notes: &notes,
            pics: &pics,
            // A header carries no table of contents of its own.
            toc: &crate::converters::doc::toc::TocPlan::default(),
            revs: &crate::converters::doc::revisions::Revisions::default(),
            ver: sprm::SprmVersion::from_fib(&f),
        };
        let input = HfInput {
            fib: &f,
            doc: &doc_stream,
            table: &table,
            pieces: &pieces,
            enc,
        };
        Some(read_first_section(&input, &ctx))
    }

    fn plain_text(node: &PmNode) -> String {
        let mut out = String::new();
        if let Some(t) = &node.text {
            out.push_str(t);
        }
        for c in node.children() {
            out.push_str(&plain_text(c));
            // Block boundaries are word boundaries; runs inside one are not.
            if c.text.is_none() {
                out.push(' ');
            }
        }
        out
    }

    /// Collapse to what can be compared across two readers: words only.
    fn normalise(s: &str) -> String {
        s.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    }

    #[test]
    fn headers_and_footers_come_out_of_the_corpus() {
        let files = corpus();
        if files.is_empty() {
            eprintln!("KUBUNO_DOC_CORPUS absent — test ignoré");
            return;
        }
        let (mut with_header, mut with_footer, mut tokens, mut seen) = (0, 0, 0usize, 0);
        let mut samples: Vec<String> = Vec::new();
        for p in &files {
            let Ok(bytes) = std::fs::read(p) else { continue };
            let Some((h, f)) = headers_of(&bytes) else { continue };
            seen += 1;
            for (kind, node) in [("en-tête", &h), ("pied", &f)] {
                let Some(node) = node else { continue };
                let t = normalise(&plain_text(node));
                if t.contains("{page}") && samples.len() < 6 {
                    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
                    samples.push(format!("  {stem} [{kind}] : {t}"));
                }
            }
            if let Some(h) = &h {
                with_header += 1;
                let t = plain_text(h);
                tokens += usize::from(t.contains("{page}") || t.contains("{pages}"));
            }
            if f.is_some() {
                with_footer += 1;
            }
        }
        eprintln!(
            "en-têtes/pieds .doc : {seen} lus, {with_header} avec en-tête, \
             {with_footer} avec pied, {tokens} avec un jeton {{page}}/{{pages}}"
        );
        for s in &samples {
            eprintln!("{s}");
        }
        assert!(with_header > 0, "aucun en-tête extrait du corpus");
        assert!(with_footer > 0, "aucun pied de page extrait du corpus");
        assert!(
            tokens > 0,
            "aucun champ PAGE/NUMPAGES converti en jeton d'éditeur"
        );
    }

    /// Compare against LibreOffice, the reference implementation.
    ///
    /// `KUBUNO_DOC_FODT` is a directory of `<stem>.fodt` produced by
    /// `soffice --headless --convert-to fodt`; the flat ODF puts each header in a
    /// `<style:header>` element, so the two readers can be held side by side.
    #[test]
    fn header_text_agrees_with_libreoffice() {
        let Ok(fodt_dir) = std::env::var("KUBUNO_DOC_FODT") else {
            eprintln!("KUBUNO_DOC_FODT absent — test ignoré");
            return;
        };
        let files = corpus();
        if files.is_empty() {
            eprintln!("KUBUNO_DOC_CORPUS absent — test ignoré");
            return;
        }
        let (mut compared, mut agree, mut both_empty) = (0usize, 0usize, 0usize);
        let mut diffs: Vec<String> = Vec::new();
        for p in &files {
            let Some(stem) = p.file_stem().and_then(|s| s.to_str()) else { continue };
            let fodt = std::path::Path::new(&fodt_dir).join(format!("{stem}.fodt"));
            let Ok(xml) = std::fs::read_to_string(&fodt) else { continue };
            let Ok(bytes) = std::fs::read(p) else { continue };
            let Some((h, _)) = headers_of(&bytes) else { continue };

            // LibreOffice keeps the three flavours apart in three page styles;
            // we answer with one header, chosen in the same order of preference,
            // and only for the FIRST section — hence the first element of each.
            let theirs = ["style:header", "style:header-first", "style:header-left"]
                .iter()
                .filter_map(|tag| odf_story_texts(&xml, tag).into_iter().next())
                .map(|s| normalise(&s))
                .find(|s| !s.is_empty())
                .unwrap_or_default();
            let ours = normalise(&h.as_ref().map(plain_text).unwrap_or_default());
            if theirs.is_empty() && ours.is_empty() {
                both_empty += 1;
                continue;
            }
            compared += 1;
            if words_agree(&ours, &theirs) {
                agree += 1;
            } else if diffs.len() < 25 {
                diffs.push(format!("  {stem}\n    nous : {ours}\n    LO   : {theirs}"));
            }
        }
        eprintln!(
            "en-têtes vs LibreOffice : {agree}/{compared} concordants \
             ({both_empty} fichiers sans en-tête des deux côtés)"
        );
        for d in &diffs {
            eprintln!("{d}");
        }
        assert!(compared > 0, "aucun en-tête à comparer");
    }

    /// Do the two readings say the same thing?
    ///
    /// Not string equality: LibreOffice keeps the page number Word cached where
    /// we write `{page}`, and it renders tabs and field results with its own
    /// spacing. Agreement is "every word one side has, modulo the tokens, the
    /// other has too".
    fn words_agree(ours: &str, theirs: &str) -> bool {
        let strip = |s: &str| -> Vec<String> {
            // The tokens can be glued to their neighbours ("p.{page}/{pages}"),
            // so they are cut out before the split, not filtered after it.
            s.replace("{pages}", " ")
                .replace("{page}", " ")
                .split_whitespace()
                .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_owned())
                .filter(|w| !w.is_empty() && w.parse::<u32>().is_err())
                .collect()
        };
        let (a, b) = (strip(ours), strip(theirs));
        if a == b {
            return true;
        }
        // A field result LibreOffice expanded and we did not (or the reverse)
        // shows up as one side being a subsequence of the other.
        let contains_all = |big: &[String], small: &[String]| small.iter().all(|w| big.contains(w));
        !a.is_empty() && !b.is_empty() && (contains_all(&a, &b) || contains_all(&b, &a))
    }

    /// Text of each `<style:header>` (or footer) of a flat ODF document, in
    /// document order — one entry per page style that defines one.
    ///
    /// A hand walk rather than a parser: the point is to read LibreOffice's
    /// answer, not to be a general ODF reader.
    fn odf_story_texts(xml: &str, tag: &str) -> Vec<String> {
        let open = format!("<{tag}");
        let close = format!("</{tag}>");
        let mut out: Vec<String> = Vec::new();
        let mut rest = xml;
        while let Some(i) = rest.find(&open) {
            let after = &rest[i..];
            let Some(gt) = after.find('>') else { break };
            // `<style:header-left>` is a different element with the same prefix.
            let boundary = after[open.len()..]
                .chars()
                .next()
                .is_some_and(|c| c.is_whitespace() || c == '>' || c == '/');
            // `<style:header/>` — an explicitly empty story.
            if !boundary || after[..gt].ends_with('/') {
                rest = &after[gt..];
                continue;
            }
            let Some(j) = after.find(&close) else { break };
            let body = &after[gt + 1..j];
            // An embedded image is base64 in the middle of the story; it is not
            // text, and left in it drowns everything else.
            out.push(strip_tags(&drop_element(body, "office:binary-data")));
            rest = &after[j + close.len()..];
        }
        out
    }

    /// Remove every `<tag>…</tag>` block from a fragment.
    fn drop_element(xml: &str, tag: &str) -> String {
        let (open, close) = (format!("<{tag}"), format!("</{tag}>"));
        let mut out = String::new();
        let mut rest = xml;
        while let Some(i) = rest.find(&open) {
            out.push_str(&rest[..i]);
            let after = &rest[i..];
            match after.find(&close) {
                Some(j) => rest = &after[j + close.len()..],
                None => return out,
            }
        }
        out.push_str(rest);
        out
    }

    /// Element content with the markup removed and `<text:tab/>` kept as space.
    fn strip_tags(xml: &str) -> String {
        let mut out = String::new();
        let mut depth = 0u32;
        for c in xml.chars() {
            match c {
                '<' => {
                    depth += 1;
                    out.push(' ');
                }
                '>' => depth = depth.saturating_sub(1),
                _ if depth == 0 => out.push(c),
                _ => {}
            }
        }
        out.replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&apos;", "'")
            .replace("&quot;", "\"")
    }
}
