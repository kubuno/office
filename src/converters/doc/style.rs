//! The stylesheet (`STSH`) and the font table (`SttbfFfn`).
//!
//! A `.doc` never names a style inline: a paragraph carries an `istd`, an index
//! into the stylesheet, and the stylesheet holds the `grpprl` that `istd` adds
//! on top of the style it is BASED ON. Reading a `.doc` therefore means
//! flattening an inheritance chain before a single sprm can be applied.
//!
//! Two traps are handled here:
//!   * `istdBase` can point at itself or form a longer loop — corrupt files in
//!     the wild do, and LibreOffice only guards it in some code paths (see
//!     `aSeenStyles` in `WW8FlyPara::ReadFull`). Every walk below is bounded
//!     and cycle-checked.
//!   * The first 15 `istd` are RESERVED for built-in styles whose identity
//!     comes from the index, not from the name: `istd` 1 is "heading 1" even
//!     when the stored name reads "Titre 1" or "Überschrift 1". Recognising
//!     headings by name is a localisation bug; we go through the `sti`.
//!     The reserved mapping is NOT `sti == istd` past the headings — real Word
//!     files put "Default Paragraph Font" at `istd` 10, "Normal Table" at 11
//!     and "No List" at 12 (see [`FIXED_ISTD_STI`]).
//!
//! Reference: LibreOffice `sw/source/filter/ww8/ww8scan.cxx` (`WW8Style`,
//! `WW8Fonts`), `ww8par2.cxx` (`WW8RStyle::ImportNewFormatStyles`,
//! `ImportUPX`) and [MS-DOC] 2.9.271 (`STSH`), 2.9.270 (`STD`), 2.9.264
//! (`SttbfFfn`).

#![allow(dead_code)]

use encoding_rs::Encoding;

use super::fib::{u16_at, u32_at, Fib};
use super::sprm::{find_sprm, ww8, SprmVersion};

/// `istdBase` value meaning "based on nothing".
pub(crate) const ISTD_NIL: u16 = 0x0FFF;
/// `sti` value meaning "not a built-in style".
pub(crate) const STI_USER: u16 = 0x0FFF;
/// How many `istd` slots have a meaning fixed by their index. Word writes 15
/// and [MS-DOC] allows no more; a file claiming otherwise is clamped.
const ISTD_MAX_FIXED: u16 = 15;
/// Upper bound on an inheritance chain, so a malformed file cannot spin.
const MAX_CHAIN: usize = 32;

/// Built-in identity of the reserved `istd`. Slot 0 is Normal, 1-9 the nine
/// headings, then the three styles Word always emits; 13-14 are reserved but
/// unassigned.
pub(crate) const FIXED_ISTD_STI: [u16; 15] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, // Normal, heading 1-9
    65,       // Default Paragraph Font
    105,      // Normal Table
    107,      // No List
    STI_USER, // reserved, unassigned
    STI_USER,
];

/// How far [`FIXED_ISTD_STI`] may OVERRIDE the `sti` a file stores.
///
/// Measured over 259 real `.doc`: slots 0-9 match the table in every single
/// file, so they can be trusted blindly and that is what makes heading
/// detection locale-proof. Slots 10-14 do not — LibreOffice writes `.doc`
/// that put "Heading" or "Body Text" there — so past 10 the stored `sti` wins.
const FIXED_ISTD_TRUSTED: u16 = 10;

/// Which family of properties a style carries (`STD.sgc`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum StyleKind {
    Paragraph,
    Character,
    Table,
    Numbering,
    #[default]
    Unknown,
}

impl StyleKind {
    fn from_sgc(sgc: u16) -> StyleKind {
        match sgc {
            1 => StyleKind::Paragraph,
            2 => StyleKind::Character,
            3 => StyleKind::Table,
            4 => StyleKind::Numbering,
            _ => StyleKind::Unknown,
        }
    }
}

/// One entry of the stylesheet.
#[derive(Debug, Clone, Default)]
pub(crate) struct Style {
    /// Index of this style in the stylesheet — what a PAPX/CHPX refers to.
    pub(crate) istd: u16,
    /// Built-in style identifier, `STI_USER` for a user-defined style.
    pub(crate) sti: u16,
    /// Family of the style.
    pub(crate) kind: StyleKind,
    /// Style this one is based on, `ISTD_NIL` when it stands alone.
    pub(crate) istd_base: u16,
    /// Style Word applies to the NEXT paragraph.
    pub(crate) istd_next: u16,
    /// Name as stored in the file — localised in non-English documents.
    pub(crate) name: String,
    /// Own paragraph properties (`UPXPapx`), without its leading `istd`.
    pub(crate) papx: Vec<u8>,
    /// Own character properties (`UPXChpx`).
    pub(crate) chpx: Vec<u8>,
    /// Own table properties (`UPXTapx`), only for table styles.
    pub(crate) tapx: Vec<u8>,
    /// False for an unused slot: `cbStd == 0`, or a truncated record.
    pub(crate) valid: bool,
}

/// The whole stylesheet, indexed by `istd`.
#[derive(Debug, Clone, Default)]
pub(crate) struct StyleSheet {
    styles: Vec<Style>,
    /// `istdMaxFixedWhenSaved` — how many leading slots are reserved.
    istd_max_fixed: u16,
    /// Default fonts of the document (`STSHI.rgftcStandardChpStsh`): ASCII,
    /// far-east, other, bidi. Indices into [`FontTable`].
    pub(crate) default_ftc: [u16; 4],
    /// Sprm encoding of the document, needed to read a style's own `grpprl`.
    /// `None` on an empty sheet, where nothing is ever decoded.
    sprm_ver: Option<SprmVersion>,
}

impl StyleSheet {
    /// Decode the `STSH` out of the table stream.
    ///
    /// `doc` is the `WordDocument` stream (the FIB lives there), `table` the
    /// stream named by [`Fib::table_stream_name`].
    pub(crate) fn parse(fib: &Fib, doc: &[u8], table: &[u8]) -> StyleSheet {
        // Word 1/2 stores styles as four parallel arrays instead of STD
        // records (LibreOffice `ImportOldFormatStyles`). Reading them with the
        // newer layout yields convincing garbage, so we decline instead.
        let ver = word_version(doc, fib.nfib);
        if ver == WordVersion::Ww2 {
            return StyleSheet::default();
        }
        let (fc, lcb) = fib_entry(doc, ver, fib.fc_stshf, fib.lcb_stshf, PAIR_STSHF);
        let start = fc as usize;
        let end = start.saturating_add(lcb as usize).min(table.len());
        if start >= end {
            return StyleSheet::default();
        }
        parse_stsh(&table[start..end], fib)
    }

    pub(crate) fn len(&self) -> usize {
        self.styles.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.styles.is_empty()
    }

    /// The style at `istd`, or `None` for an out-of-range or unused slot.
    pub(crate) fn get(&self, istd: u16) -> Option<&Style> {
        self.styles.get(istd as usize).filter(|s| s.valid)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &Style> {
        self.styles.iter().filter(|s| s.valid)
    }

    /// The built-in identity of a slot.
    ///
    /// Inside the reserved range the INDEX is authoritative — the mapping is
    /// fixed by the format, and that is the only locale-proof way to tell a
    /// heading apart. Past the reserved range we trust the stored `STD.sti`.
    pub(crate) fn sti_of(&self, istd: u16) -> u16 {
        if istd < self.istd_max_fixed.min(FIXED_ISTD_TRUSTED) {
            if let Some(&sti) = FIXED_ISTD_STI.get(istd as usize) {
                if sti != STI_USER {
                    return sti;
                }
            }
        }
        self.get(istd).map(|s| s.sti).unwrap_or(STI_USER)
    }

    /// Heading level 1-9 of a style, `None` if it is not a built-in heading.
    pub(crate) fn heading_level(&self, istd: u16) -> Option<u8> {
        match self.sti_of(istd) {
            l @ 1..=9 => Some(l as u8),
            _ => None,
        }
    }

    /// Same, but following `istdBase`: Word renders a user style based on
    /// "heading 2" as a level-2 heading unless it overrides the outline level.
    pub(crate) fn heading_level_inherited(&self, istd: u16) -> Option<u8> {
        self.chain(istd)
            .iter()
            .rev()
            .find_map(|&i| self.heading_level(i))
    }

    /// Outline level a style ASSIGNS ITSELF through `sprmPOutLvl`, flattened
    /// over the inheritance chain. 1-9, `None` for body text.
    ///
    /// This is how a user style becomes a heading without being a built-in one
    /// — real documents do exactly that, and LibreOffice gives such paragraphs
    /// an outline level while their style name says nothing.
    pub(crate) fn outline_level(&self, istd: u16) -> Option<u8> {
        let ver = self.sprm_ver?;
        if ver != SprmVersion::Ww8 {
            // Word 6/95 has no equivalent sprm.
            return None;
        }
        let mut level = None;
        for grpprl in self.papx_chain(istd) {
            if let Some(s) = find_sprm(grpprl, ver, ww8::POUTLVL) {
                // 0-8 are the nine heading levels, 9 means body text.
                level = match s.u8() {
                    Some(v @ 0..=8) => Some(v + 1),
                    _ => None,
                };
            }
        }
        level
    }

    /// The heading level a paragraph in this style gets: its built-in identity
    /// first, then whatever outline level the style assigns itself. This is the
    /// one callers should use.
    pub(crate) fn effective_heading_level(&self, istd: u16) -> Option<u8> {
        self.heading_level(istd).or_else(|| self.outline_level(istd))
    }

    pub(crate) fn kind(&self, istd: u16) -> StyleKind {
        self.get(istd).map(|s| s.kind).unwrap_or(StyleKind::Unknown)
    }

    /// Style name exactly as the document stores it (localised).
    pub(crate) fn name(&self, istd: u16) -> Option<&str> {
        self.get(istd)
            .map(|s| s.name.as_str())
            .filter(|n| !n.is_empty())
    }

    /// Locale-independent name: the English built-in name when the slot holds a
    /// built-in style, the stored name otherwise. This is what a DOCX carries
    /// in `<w:name>`, so a `.doc` and a `.docx` of the same document agree.
    pub(crate) fn canonical_name(&self, istd: u16) -> Option<String> {
        self.get(istd)?;
        if let Some(n) = builtin_name(self.sti_of(istd)) {
            return Some(n.to_string());
        }
        self.name(istd).map(|n| n.to_string())
    }

    /// Identifier in the shape our DOCX writer emits (`Heading1`, `BodyText`…),
    /// so the import can put a `styleName` consistent with the DOCX path.
    pub(crate) fn docx_style_id(&self, istd: u16) -> Option<String> {
        self.canonical_name(istd)
            .map(|n| camel_case_id(&n))
            .filter(|n| !n.is_empty())
    }

    /// The `istdBase` chain, ROOT FIRST, ending with `istd` itself.
    ///
    /// Guarded against the two shapes of corruption that occur in real files: a
    /// style based on itself, and a longer loop.
    pub(crate) fn chain(&self, istd: u16) -> Vec<u16> {
        let mut out: Vec<u16> = Vec::new();
        let mut cur = istd;
        while out.len() < MAX_CHAIN {
            if out.contains(&cur) {
                break; // cycle: stop before walking it a second time
            }
            let Some(s) = self.get(cur) else { break };
            out.push(cur);
            if s.istd_base == ISTD_NIL || s.istd_base == cur {
                break;
            }
            cur = s.istd_base;
        }
        out.reverse();
        out
    }

    /// Paragraph `grpprl` of the whole inheritance chain, root first: apply
    /// them in order and the leaf wins.
    pub(crate) fn papx_chain(&self, istd: u16) -> Vec<&[u8]> {
        self.grpprl_chain(istd, |s| &s.papx)
    }

    /// Character `grpprl` of the whole inheritance chain, root first.
    pub(crate) fn chpx_chain(&self, istd: u16) -> Vec<&[u8]> {
        self.grpprl_chain(istd, |s| &s.chpx)
    }

    fn grpprl_chain(&self, istd: u16, pick: fn(&Style) -> &Vec<u8>) -> Vec<&[u8]> {
        self.chain(istd)
            .into_iter()
            .filter_map(|i| self.styles.get(i as usize))
            .map(|s| pick(s).as_slice())
            .filter(|g| !g.is_empty())
            .collect()
    }
}

/// Position of `fcStshf` and `fcSttbfFfn` in the FIB's `rgFcLcb` array, counted
/// in `(fc, lcb)` PAIRS: `fcStshfOrig` is pair 0, `fcStshf` pair 1.
const PAIR_STSHF: usize = 1;
const PAIR_STTBFFFN: usize = 15;

/// Which generation of the format a file belongs to.
///
/// Derived like LibreOffice's `WW8Fib::GetFIBVersion`: `wIdent` decides first
/// — a Word 2 document keeps `nFib` values that would otherwise read as Word 6
/// — and `nFib` only settles the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum WordVersion {
    /// Word 1/2 for Windows: a different stylesheet and font layout.
    Ww2,
    /// Word 6.0 / Word 95.
    Ww6,
    /// Word 97 and later.
    Ww8,
}

fn word_version(doc: &[u8], nfib: u16) -> WordVersion {
    match u16_at(doc, 0) {
        0xA59B | 0xA59C | 0xA5DB => WordVersion::Ww2,
        _ if nfib >= 193 => WordVersion::Ww8,
        _ => WordVersion::Ww6,
    }
}

/// A FIB entry, preferring what [`Fib`] already parsed and falling back to
/// reading the `rgFcLcb` array directly.
///
/// `fib.rs` only fills its fields for the layouts it covers — `fcSttbfFfn` is
/// left at zero for Word 6/95 and Word 1/2 — and this module must still find
/// the font table there. A non-zero value from the FIB always wins, so the two
/// can never disagree.
fn fib_entry(doc: &[u8], ver: WordVersion, fc: u32, lcb: u32, pair: usize) -> (u32, u32) {
    if fc != 0 && lcb != 0 {
        return (fc, lcb);
    }
    rg_fc_lcb(doc, ver, pair)
}

/// Read one `(fc, lcb)` pair of the FIB's `rgFcLcb` array.
///
/// Word 97+ prefixes the array with self-describing counters (`csw`, `cslw`,
/// `cbRgFcLcb`); older versions have a fixed layout whose array starts at 0x58,
/// right after the `ccp*` counts, and Word 1/2 stores `lcb` on 16 bits so its
/// pairs are six bytes, not eight (LibreOffice `WW8Fib::WW8Fib` and `Readcb`,
/// ww8scan.cxx).
fn rg_fc_lcb(doc: &[u8], ver: WordVersion, pair: usize) -> (u32, u32) {
    let base = if ver == WordVersion::Ww8 {
        let csw = u16_at(doc, 0x20) as usize;
        let after_sw = 0x22 + csw * 2;
        let cslw = u16_at(doc, after_sw) as usize;
        after_sw + 2 + cslw * 4 + 2
    } else {
        0x58
    };
    if ver == WordVersion::Ww2 {
        let at = base + pair * 6;
        return (u32_at(doc, at), u16_at(doc, at + 4) as u32);
    }
    let at = base + pair * 8;
    (u32_at(doc, at), u32_at(doc, at + 4))
}

/// Decode the `STSHI` header then the `cstd` successive `STD` records.
fn parse_stsh(stsh: &[u8], fib: &Fib) -> StyleSheet {
    // The STSHI is length-prefixed from Word 6 on; older files start straight
    // at `cstd` (LibreOffice: `if (m_rFib.m_nFib < 67) cbStshi = 4`).
    let (mut cb_stshi, hdr) = if fib.nfib < 67 {
        (4usize, 0usize)
    } else {
        (u16_at(stsh, 0) as usize, 2usize)
    };
    cb_stshi = cb_stshi.min(stsh.len().saturating_sub(hdr));
    if cb_stshi < 4 {
        return StyleSheet::default();
    }
    let hd = &stsh[hdr..hdr + cb_stshi];

    let cstd = u16_at(hd, 0) as usize;
    let cb_std_base = u16_at(hd, 2) as usize;
    // `istdMaxFixedWhenSaved`, clamped; a file that omits it or writes 0 still
    // has the reserved slots — the count is fixed by the format, not by them.
    let fixed = match u16_at(hd, 8) {
        _ if cb_stshi < 10 => ISTD_MAX_FIXED,
        0 => ISTD_MAX_FIXED,
        n => n.min(ISTD_MAX_FIXED),
    };
    let mut sheet = StyleSheet {
        styles: Vec::new(),
        sprm_ver: Some(SprmVersion::from_fib(fib)),
        istd_max_fixed: fixed,
        default_ftc: [
            if cb_stshi >= 14 { u16_at(hd, 12) } else { 0 },
            if cb_stshi >= 16 { u16_at(hd, 14) } else { 0 },
            if cb_stshi >= 18 { u16_at(hd, 16) } else { 0 },
            if cb_stshi >= 20 { u16_at(hd, 18) } else { 0 },
        ],
    };

    let body = &stsh[(hdr + cb_stshi).min(stsh.len())..];
    // Every record costs at least its own 2-byte length, which bounds `cstd`.
    let cstd = cstd.min(body.len() / 2);
    sheet.styles.reserve(cstd);

    // Word 97+ stores style names in UTF-16, Word 6/95 as an 8-bit Pascal
    // string (LibreOffice `WW8Style::Read1Style`, switch on `nVersion`).
    let unicode_names = fib.nfib >= 193;

    let mut pos = 0usize;
    for istd in 0..cstd {
        let record_start = pos;
        if pos + 2 > body.len() {
            break;
        }
        let cb_std = u16_at(body, pos) as usize;
        pos += 2;
        let rec_end = pos.saturating_add(cb_std).min(body.len());
        let mut st = Style {
            istd: istd as u16,
            sti: STI_USER,
            istd_base: ISTD_NIL,
            ..Default::default()
        };
        // `cbStd == 0` is a legal empty slot; a record shorter than the fixed
        // head is corrupt. Either way the slot stays in place so that the
        // vector index keeps being the `istd`.
        if cb_std >= cb_std_base && cb_std_base >= 2 && pos + cb_std_base <= body.len() {
            read_std_base(&body[pos..pos + cb_std_base], &mut st);
            let mut p = pos + cb_std_base;
            st.name = read_std_name(body, &mut p, rec_end, unicode_names);
            read_grupx(body, &mut p, rec_end, record_start, &mut st);
            st.valid = true;
        }
        pos = rec_end;
        sheet.styles.push(st);
    }
    sheet
}

/// Fixed head of an `STD` (`StdfBase`): identity, family and inheritance.
fn read_std_base(b: &[u8], st: &mut Style) {
    st.sti = u16_at(b, 0) & 0x0FFF;
    if b.len() >= 4 {
        let w = u16_at(b, 2);
        st.kind = StyleKind::from_sgc(w & 0x000F);
        st.istd_base = (w & 0xFFF0) >> 4;
    }
    if b.len() >= 6 {
        st.istd_next = (u16_at(b, 4) & 0xFFF0) >> 4;
    }
}

/// Style name: a "belt and braces" string — a count, the characters, then a
/// terminating zero.
///
/// Word 97 writes it in UTF-16, but "Lotus SmartSuite Product: Word Pro" writes
/// an 8-bit one in files that otherwise claim to be Word 97 (LibreOffice
/// #i8114#), so the UTF-16 reading is checked against its terminator before
/// being trusted.
fn read_std_name(b: &[u8], pos: &mut usize, end: usize, unicode: bool) -> String {
    if unicode {
        let cch = u16_at(b, *pos) as usize;
        let bytes = cch.saturating_mul(2);
        if *pos + 4 + bytes <= end && u16_at(b, *pos + 2 + bytes) == 0 {
            let units: Vec<u16> = b[*pos + 2..*pos + 2 + bytes]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            *pos = (*pos + 4 + bytes).min(end);
            return String::from_utf16_lossy(&units);
        }
    }
    let cch = b.get(*pos).copied().unwrap_or(0) as usize;
    let from = *pos + 1;
    let to = from.saturating_add(cch).min(end);
    // Style names are always written in the ANSI code page, even by the Mac
    // versions of Word (ww8scan.cxx, `Read1Style`).
    let name = if from < to {
        encoding_rs::WINDOWS_1252.decode(&b[from..to]).0.into_owned()
    } else {
        String::new()
    };
    *pos = (*pos + cch + 2).min(end);
    name
}

/// The variable tail of an `STD`: the `UPX` structures holding the style's own
/// `grpprl`. A paragraph style carries paragraph then character properties, a
/// table style prepends table properties, anything else carries only character
/// properties (LibreOffice `WW8RStyle::ImportGrupx`).
fn read_grupx(b: &[u8], pos: &mut usize, end: usize, record_start: usize, st: &mut Style) {
    match st.kind {
        StyleKind::Paragraph => {
            st.papx = read_upx(b, pos, end, record_start, true).to_vec();
            st.chpx = read_upx(b, pos, end, record_start, false).to_vec();
        }
        StyleKind::Table => {
            st.tapx = read_upx(b, pos, end, record_start, false).to_vec();
            st.papx = read_upx(b, pos, end, record_start, true).to_vec();
            st.chpx = read_upx(b, pos, end, record_start, false).to_vec();
        }
        _ => st.chpx = read_upx(b, pos, end, record_start, false).to_vec(),
    }
}

/// One `UPX`: a 16-bit byte count then the `grpprl`.
///
/// Each `UPX` starts at an EVEN offset relative to the start of its `STD`, not
/// of the stream — the distinction matters when the stylesheet itself begins on
/// an odd offset (LibreOffice `WW8RStyle::ImportUPX` and #89439#).
fn read_upx<'a>(
    b: &'a [u8],
    pos: &mut usize,
    end: usize,
    record_start: usize,
    is_papx: bool,
) -> &'a [u8] {
    if (*pos ^ record_start) & 1 != 0 {
        *pos += 1;
    }
    if *pos + 2 > end {
        *pos = end;
        return &[];
    }
    let cb = (u16_at(b, *pos) as usize).min(end - *pos - 2);
    *pos += 2;
    let stop = *pos + cb;
    // A UPXPapx opens with the style's own istd, which is not a sprm.
    let from = if is_papx { (*pos + 2).min(stop) } else { *pos };
    *pos = stop;
    &b[from..stop]
}

// ---------------------------------------------------------------------------
// Font table
// ---------------------------------------------------------------------------

/// `SttbfFfn`: the document's fonts, addressed by the `ftc` index that the
/// character sprms (`sprmCRgFtc0` and friends) carry.
#[derive(Debug, Clone, Default)]
pub(crate) struct FontTable {
    /// Primary name and, when the file gives one, the alternate name Word falls
    /// back to when the primary font is not installed.
    fonts: Vec<(String, Option<String>)>,
}

impl FontTable {
    /// Decode the font table. `doc` is the `WordDocument` stream (for the FIB),
    /// `table` the stream named by [`Fib::table_stream_name`].
    pub(crate) fn parse(fib: &Fib, doc: &[u8], table: &[u8]) -> FontTable {
        let ver = word_version(doc, fib.nfib);
        let (fc, lcb) = fib_entry(doc, ver, fib.fc_sttbf_ffn, fib.lcb_sttbf_ffn, PAIR_STTBFFFN);
        let start = fc as usize;
        let end = start.saturating_add(lcb as usize).min(table.len());
        if lcb <= 2 || start >= end || end - start < 4 {
            return FontTable::default();
        }
        parse_sttbf_ffn(&table[start..end], fib, ver)
    }

    /// Name of font `ftc`, if the document declares it.
    pub(crate) fn name(&self, ftc: u16) -> Option<&str> {
        self.fonts
            .get(ftc as usize)
            .map(|f| f.0.as_str())
            .filter(|s| !s.is_empty())
    }

    /// The font Word substitutes when `ftc` is not installed, if the file names
    /// one — usually the localised alias of a CJK font ("SimSun" → "宋体").
    pub(crate) fn alt_name(&self, ftc: u16) -> Option<&str> {
        self.fonts
            .get(ftc as usize)
            .and_then(|f| f.1.as_deref())
            .filter(|s| !s.is_empty())
    }

    /// CSS-ready family list: the primary font, then its alternate.
    pub(crate) fn family(&self, ftc: u16) -> Option<String> {
        let name = self.name(ftc)?;
        Some(match self.alt_name(ftc) {
            Some(alt) => format!("{name}, {alt}"),
            None => name.to_string(),
        })
    }

    pub(crate) fn len(&self) -> usize {
        self.fonts.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }

    /// Every declared name, primary and alternate alike.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &str> {
        self.fonts
            .iter()
            .flat_map(|f| std::iter::once(f.0.as_str()).chain(f.1.as_deref()))
    }
}

fn parse_sttbf_ffn(sttb: &[u8], fib: &Fib, ver: WordVersion) -> FontTable {
    let ver8 = ver == WordVersion::Ww8;
    // Word 97 opens with a 16-bit count of entries THEN an unused word; older
    // versions only store the total byte count, which we already know from the
    // FIB. Reading four bytes in both cases eats the first font's length
    // (LibreOffice `WW8Fonts`: the `ReadUInt16` is guarded by the version).
    let (declared, header) = if ver8 {
        (u16_at(sttb, 0) as usize, 4)
    } else {
        (usize::MAX, 2)
    };
    let body = &sttb[header..];

    let mut fonts = Vec::new();
    let mut p = 0usize;
    while p < body.len() && fonts.len() < declared.min(0x4000) {
        // Every FFN opens with cbFfnM1: the length of the rest of the record.
        let cb = body[p] as usize;
        if cb == 0 || p + 1 + cb > body.len() {
            break;
        }
        let ffn = &body[p + 1..p + 1 + cb];
        // `ibszAlt` points at a SECOND name inside the same field: the font
        // Word substitutes when the first one is missing. LibreOffice keeps it
        // too, and dropping it loses the CJK aliases ("SimSun" / "宋体").
        let entry = match ver {
            WordVersion::Ww8 => {
                // flags (1) · wWeight (2) · chs (1) · ibszAlt (1) · PANOSE (10)
                // · FONTSIGNATURE (24) = 39 bytes, then a UTF-16 name.
                if ffn.len() < 41 {
                    break;
                }
                let names = &ffn[39..];
                let alt = (ffn[4] as usize)
                    .checked_mul(2)
                    .filter(|&o| o > 0 && o < names.len());
                (
                    utf16_zero_terminated(names),
                    alt.map(|o| utf16_zero_terminated(&names[o..])),
                )
            }
            WordVersion::Ww6 => {
                // flags (1) · wWeight (2) · chs (1) · ibszAlt (1), then a
                // zero-terminated 8-bit name in the FONT's own code page.
                if ffn.len() < 6 {
                    break;
                }
                let enc = charset_for_chs(ffn[3], fib);
                let names = &ffn[5..];
                let alt = Some(ffn[4] as usize).filter(|&o| o > 0 && o < names.len());
                (cstr(names, enc), alt.map(|o| cstr(&names[o..], enc)))
            }
            WordVersion::Ww2 => {
                // wWeight and chs are single bytes here and there is no
                // alternate name.
                if ffn.len() < 3 {
                    break;
                }
                (cstr(&ffn[2..], charset_for_chs(ffn[1], fib)), None)
            }
        };
        let alt = entry.1.map(|s| clean_font_name(&s)).filter(|s| !s.is_empty());
        fonts.push((clean_font_name(&entry.0), alt));
        p += 1 + cb;
    }
    FontTable { fonts }
}

/// A zero-terminated UTF-16LE string, capped at Word's own 65-character limit
/// and tolerating a record that ends without its terminator.
fn utf16_zero_terminated(b: &[u8]) -> String {
    let mut units = Vec::new();
    for c in b.chunks_exact(2).take(65) {
        let u = u16::from_le_bytes([c[0], c[1]]);
        if u == 0 {
            break;
        }
        units.push(u);
    }
    String::from_utf16_lossy(&units)
}

/// Drop the control characters a font name must not contain, plus the padding
/// spaces some writers leave behind (LibreOffice `lcl_checkFontname`, #i43762#).
fn clean_font_name(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect::<String>().trim().to_string()
}

/// A zero-terminated 8-bit string decoded in `enc`.
fn cstr(b: &[u8], enc: &'static Encoding) -> String {
    let n = b.iter().position(|&c| c == 0).unwrap_or(b.len()).min(65);
    enc.decode(&b[..n]).0.into_owned()
}

/// Windows charset byte of an `FFN` → code page. Word 6/95 stores a font name
/// in the encoding of that font, not of the document (LibreOffice #i8726#).
fn charset_for_chs(chs: u8, fib: &Fib) -> &'static Encoding {
    match chs {
        0 | 1 => super::piece::charset_for(fib),
        128 => encoding_rs::SHIFT_JIS,
        129 => encoding_rs::EUC_KR,
        134 => encoding_rs::GBK,
        136 => encoding_rs::BIG5,
        161 => encoding_rs::WINDOWS_1253,
        162 => encoding_rs::WINDOWS_1254,
        163 => encoding_rs::WINDOWS_1258,
        177 => encoding_rs::WINDOWS_1255,
        178..=180 => encoding_rs::WINDOWS_1256,
        186 => encoding_rs::WINDOWS_1257,
        204 => encoding_rs::WINDOWS_1251,
        222 => encoding_rs::WINDOWS_874,
        238 => encoding_rs::WINDOWS_1250,
        _ => encoding_rs::WINDOWS_1252,
    }
}

// ---------------------------------------------------------------------------
// Built-in style identities
// ---------------------------------------------------------------------------

/// English name of every built-in style, INDEXED BY `sti`.
///
/// Transcribed from LibreOffice `sw/source/filter/ww8/styles.cxx`, whose casing
/// matches what Word writes in `w:latentStyles`. The index is the identifier:
/// this table is what makes heading detection immune to localisation.
const BUILTIN_NAMES: [&str; 91] = [
    "Normal",
    "heading 1",
    "heading 2",
    "heading 3",
    "heading 4",
    "heading 5",
    "heading 6",
    "heading 7",
    "heading 8",
    "heading 9",
    "index 1",
    "index 2",
    "index 3",
    "index 4",
    "index 5",
    "index 6",
    "index 7",
    "index 8",
    "index 9",
    "toc 1",
    "toc 2",
    "toc 3",
    "toc 4",
    "toc 5",
    "toc 6",
    "toc 7",
    "toc 8",
    "toc 9",
    "Normal Indent",
    "footnote text",
    "annotation text",
    "header",
    "footer",
    "index heading",
    "caption",
    "table of figures",
    "envelope address",
    "envelope return",
    "footnote reference",
    "annotation reference",
    "line number",
    "page number",
    "endnote reference",
    "endnote text",
    "table of authorities",
    "macro",
    "TOC Heading",
    "List",
    "List Bullet",
    "List Number",
    "List 2",
    "List 3",
    "List 4",
    "List 5",
    "List Bullet 2",
    "List Bullet 3",
    "List Bullet 4",
    "List Bullet 5",
    "List Number 2",
    "List Number 3",
    "List Number 4",
    "List Number 5",
    "Title",
    "Closing",
    "Signature",
    "Default Paragraph Font",
    "Body Text",
    "Body Text Indent",
    "List Continue",
    "List Continue 2",
    "List Continue 3",
    "List Continue 4",
    "List Continue 5",
    "Message Header",
    "Subtitle",
    "Salutation",
    "Date",
    "Body Text First Indent",
    "Body Text First Indent 2",
    "Note Heading",
    "Body Text 2",
    "Body Text 3",
    "Body Text Indent 2",
    "Body Text Indent 3",
    "Block Text",
    "Hyperlink",
    "FollowedHyperlink",
    "Strong",
    "Emphasis",
    "Document Map",
    "Plain Text",
];

/// English name of the built-in style `sti`, `None` for a user style.
///
/// Two identifiers beyond LibreOffice's table are handled explicitly because
/// every Word file carries them.
pub(crate) fn builtin_name(sti: u16) -> Option<&'static str> {
    match sti {
        105 => Some("Normal Table"),
        107 => Some("No List"),
        _ => BUILTIN_NAMES.get(sti as usize).copied(),
    }
}

/// A style name turned into the identifier shape our DOCX writer emits:
/// `heading 1` → `Heading1`, `footnote text` → `FootnoteText`.
fn camel_case_id(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut upper = true;
    for c in name.chars() {
        if c.is_alphanumeric() {
            if upper {
                out.extend(c.to_uppercase());
            } else {
                out.push(c);
            }
            upper = false;
        } else {
            upper = true;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One synthetic style: `(sti, sgc, istdBase, name, papx, chpx)`.
    type Def<'a> = (u16, u16, u16, &'a str, &'a [u8], &'a [u8]);

    /// Build a well-formed STSH out of style definitions.
    fn synth(styles: &[Def<'_>]) -> Vec<u8> {
        let mut stshi = Vec::new();
        stshi.extend((styles.len() as u16).to_le_bytes()); // cstd
        stshi.extend(10u16.to_le_bytes()); // cbSTDBaseInFile
        stshi.extend(0u16.to_le_bytes()); // flags
        stshi.extend(91u16.to_le_bytes()); // stiMaxWhenSaved
        stshi.extend(15u16.to_le_bytes()); // istdMaxFixedWhenSaved
        stshi.extend(0u16.to_le_bytes()); // verBuiltInNamesWhenSaved
        stshi.extend(7u16.to_le_bytes()); // ftcAsci
        stshi.extend(8u16.to_le_bytes()); // ftcFE
        stshi.extend(9u16.to_le_bytes()); // ftcOther
        stshi.extend(9u16.to_le_bytes()); // ftcBi

        let mut out = Vec::new();
        out.extend((stshi.len() as u16).to_le_bytes());
        out.extend(&stshi);
        for (sti, sgc, base, name, papx, chpx) in styles {
            let mut std = Vec::new();
            std.extend(sti.to_le_bytes());
            std.extend((sgc | (base << 4)).to_le_bytes());
            std.extend(0u16.to_le_bytes()); // cupx / istdNext
            std.extend(0u16.to_le_bytes()); // bchUpe
            std.extend(0u16.to_le_bytes()); // flags
            let units: Vec<u16> = name.encode_utf16().collect();
            std.extend((units.len() as u16).to_le_bytes());
            for u in &units {
                std.extend(u.to_le_bytes());
            }
            std.extend(0u16.to_le_bytes()); // terminator
            {
                // Each UPX starts at an even offset inside the STD; the STD
                // itself starts right after its own 2-byte length.
                let mut upx = |g: &[u8], is_papx: bool| {
                    if std.len() % 2 != 0 {
                        std.push(0);
                    }
                    let cb = g.len() + if is_papx { 2 } else { 0 };
                    std.extend((cb as u16).to_le_bytes());
                    if is_papx {
                        std.extend(0u16.to_le_bytes());
                    }
                    std.extend(g);
                };
                if *sgc == 1 {
                    upx(papx, true);
                    upx(chpx, false);
                } else {
                    upx(chpx, false);
                }
            }
            out.extend((std.len() as u16).to_le_bytes());
            out.extend(&std);
        }
        out
    }

    fn fib97() -> Fib {
        Fib { nfib: 193, ..Default::default() }
    }

    #[test]
    fn reads_names_kinds_and_grpprl() {
        let stsh = synth(&[
            (0, 1, ISTD_NIL, "Normal", &[0x0A, 0x24, 0x01], &[]),
            (1, 1, 0, "Titre 1", &[], &[0x35, 0x08, 0x01]),
            (STI_USER, 2, ISTD_NIL, "Mon style", &[], &[0x36, 0x08, 0x01]),
        ]);
        let sheet = parse_stsh(&stsh, &fib97());
        assert_eq!(sheet.len(), 3);
        assert_eq!(sheet.name(0), Some("Normal"));
        assert_eq!(sheet.name(1), Some("Titre 1"));
        assert_eq!(sheet.name(2), Some("Mon style"));
        assert_eq!(sheet.kind(1), StyleKind::Paragraph);
        assert_eq!(sheet.kind(2), StyleKind::Character);
        assert_eq!(sheet.default_ftc, [7, 8, 9, 9]);
        assert_eq!(sheet.get(0).unwrap().papx, vec![0x0A, 0x24, 0x01]);
        assert_eq!(sheet.get(1).unwrap().chpx, vec![0x35, 0x08, 0x01]);
        assert_eq!(sheet.get(2).unwrap().chpx, vec![0x36, 0x08, 0x01]);
    }

    #[test]
    fn heading_is_found_by_index_not_by_name() {
        // A French document: "heading 1" is stored as "Titre 1".
        let stsh = synth(&[
            (0, 1, ISTD_NIL, "Normal", &[], &[]),
            (1, 1, 0, "Titre 1", &[], &[]),
            (3, 1, 0, "Titre 3", &[], &[]),
        ]);
        let sheet = parse_stsh(&stsh, &fib97());
        assert_eq!(sheet.heading_level(1), Some(1));
        assert_eq!(sheet.heading_level(0), None);
        // Slot 2 is reserved for "heading 2" even though this file stored sti 3
        // in it: inside the fixed range the INDEX wins.
        assert_eq!(sheet.heading_level(2), Some(2));
        assert_eq!(sheet.canonical_name(1).as_deref(), Some("heading 1"));
        assert_eq!(sheet.docx_style_id(1).as_deref(), Some("Heading1"));
        assert_eq!(sheet.docx_style_id(0).as_deref(), Some("Normal"));
    }

    #[test]
    fn user_styles_keep_their_own_name() {
        // 15 reserved slots, then the first user slot.
        let mut defs: Vec<Def<'_>> =
            (0..15u16).map(|i| (i, 1, ISTD_NIL, "réservé", &[][..], &[][..])).collect();
        defs.push((STI_USER, 1, 0, "Corps de texte maison", &[0x01][..], &[][..]));
        let stsh = synth(&defs);
        let sheet = parse_stsh(&stsh, &fib97());
        assert_eq!(sheet.canonical_name(15).as_deref(), Some("Corps de texte maison"));
        assert_eq!(sheet.docx_style_id(15).as_deref(), Some("CorpsDeTexteMaison"));
        assert_eq!(sheet.heading_level(15), None);
    }

    #[test]
    fn inheritance_is_flattened_root_first() {
        let stsh = synth(&[
            (0, 1, ISTD_NIL, "Normal", &[0x01], &[]),
            (STI_USER, 1, 0, "A", &[0x02], &[]),
            (STI_USER, 1, 1, "B", &[0x03], &[]),
        ]);
        let sheet = parse_stsh(&stsh, &fib97());
        assert_eq!(sheet.chain(2), vec![0, 1, 2]);
        assert_eq!(
            sheet.papx_chain(2),
            vec![&[0x01u8][..], &[0x02][..], &[0x03][..]]
        );
    }

    #[test]
    fn self_referencing_base_does_not_loop() {
        let stsh = synth(&[(0, 1, 0, "Normal", &[0x01], &[])]);
        let sheet = parse_stsh(&stsh, &fib97());
        assert_eq!(sheet.chain(0), vec![0]);
    }

    #[test]
    fn cyclic_bases_do_not_loop() {
        // 0 → 2 → 1 → 0: a loop no valid file has, but corrupt ones do.
        let stsh = synth(&[
            (STI_USER, 1, 2, "A", &[0x01], &[]),
            (STI_USER, 1, 0, "B", &[0x02], &[]),
            (STI_USER, 1, 1, "C", &[0x03], &[]),
        ]);
        let sheet = parse_stsh(&stsh, &fib97());
        for istd in 0..3u16 {
            let c = sheet.chain(istd);
            assert!(c.len() <= 3, "chaîne non bornée: {c:?}");
            let mut seen = c.clone();
            seen.sort_unstable();
            seen.dedup();
            assert_eq!(seen.len(), c.len(), "istd répété: {c:?}");
        }
    }

    #[test]
    fn truncated_stylesheet_is_survivable() {
        let stsh = synth(&[
            (0, 1, ISTD_NIL, "Normal", &[0x01], &[]),
            (1, 1, 0, "heading 1", &[0x02], &[0x03]),
        ]);
        for n in 0..stsh.len() {
            let sheet = parse_stsh(&stsh[..n], &fib97());
            for i in 0..4u16 {
                let _ = sheet.chain(i);
                let _ = sheet.papx_chain(i);
                let _ = sheet.chpx_chain(i);
                let _ = sheet.canonical_name(i);
            }
        }
    }

    #[test]
    fn font_names_are_read_from_a_ver8_table() {
        let mut sttb = Vec::new();
        sttb.extend(2u16.to_le_bytes()); // count of entries
        sttb.extend(0u16.to_le_bytes()); // unused
        for name in ["Times New Roman", "Wingdings"] {
            let mut ffn = vec![0u8; 39];
            for u in name.encode_utf16() {
                ffn.extend(u.to_le_bytes());
            }
            ffn.extend(0u16.to_le_bytes());
            sttb.push(ffn.len() as u8);
            sttb.extend(&ffn);
        }
        let fonts = parse_sttbf_ffn(&sttb, &fib97(), WordVersion::Ww8);
        assert_eq!(fonts.len(), 2);
        assert_eq!(fonts.name(0), Some("Times New Roman"));
        assert_eq!(fonts.name(1), Some("Wingdings"));
        assert_eq!(fonts.name(9), None);
        assert_eq!(fonts.alt_name(0), None);
    }

    #[test]
    fn alternate_font_name_is_kept() {
        let mut sttb = Vec::new();
        sttb.extend(1u16.to_le_bytes());
        sttb.extend(0u16.to_le_bytes());
        let (primary, alt) = ("SimSun", "宋体");
        let mut ffn = vec![0u8; 39];
        // ibszAlt counts UTF-16 characters from the start of the name field.
        ffn[4] = primary.encode_utf16().count() as u8 + 1;
        for u in primary.encode_utf16().chain([0]).chain(alt.encode_utf16()).chain([0]) {
            ffn.extend(u.to_le_bytes());
        }
        sttb.push(ffn.len() as u8);
        sttb.extend(&ffn);
        let fonts = parse_sttbf_ffn(&sttb, &fib97(), WordVersion::Ww8);
        assert_eq!(fonts.name(0), Some("SimSun"));
        assert_eq!(fonts.alt_name(0), Some("宋体"));
        assert_eq!(fonts.family(0).as_deref(), Some("SimSun, 宋体"));
    }

    #[test]
    fn outline_level_makes_a_user_style_a_heading() {
        // Real files do this: "Custom1"/"Custom2" carry no built-in identity,
        // only sprmPOutLvl (0x2640), and Word still outlines them.
        let mut defs: Vec<Def<'_>> =
            (0..15u16).map(|i| (i, 1, ISTD_NIL, "x", &[][..], &[][..])).collect();
        defs.push((STI_USER, 1, 0, "Custom2", &[0x40, 0x26, 0x01][..], &[][..]));
        defs.push((STI_USER, 1, 0, "Corps", &[0x40, 0x26, 0x09][..], &[][..]));
        let sheet = parse_stsh(&synth(&defs), &fib97());
        assert_eq!(sheet.heading_level(15), None);
        assert_eq!(sheet.outline_level(15), Some(2));
        assert_eq!(sheet.effective_heading_level(15), Some(2));
        // Outline level 9 means body text, not a heading.
        assert_eq!(sheet.outline_level(16), None);
        assert_eq!(sheet.effective_heading_level(16), None);
    }

    #[test]
    fn reserved_slots_past_the_headings_keep_their_stored_identity() {
        // LibreOffice writes .doc that put real styles at istd 10-12, so only
        // the heading range may override what the file says.
        let mut defs: Vec<Def<'_>> =
            (0..10u16).map(|i| (i, 1, ISTD_NIL, "x", &[][..], &[][..])).collect();
        defs.push((66, 1, 0, "Body Text", &[][..], &[][..])); // istd 10
        let sheet = parse_stsh(&synth(&defs), &fib97());
        assert_eq!(sheet.sti_of(10), 66);
        assert_eq!(sheet.canonical_name(10).as_deref(), Some("Body Text"));
        assert_eq!(sheet.heading_level(9), Some(9));
        assert_eq!(sheet.heading_level(10), None);
    }

    /// Real files: style and font names must be readable, the heading slots
    /// must line up, and no chain may loop. Corrupt fuzz samples are part of
    /// the corpus, so the criterion is statistical rather than per-file.
    /// Needs `KUBUNO_DOC_CORPUS` pointing at a tree of `.doc`.
    #[test]
    fn corpus_styles_are_plausible() {
        let Ok(root) = std::env::var("KUBUNO_DOC_CORPUS") else {
            eprintln!("KUBUNO_DOC_CORPUS absent — test ignoré");
            return;
        };
        let (mut files, mut with_styles, mut with_headings, mut with_fonts) = (0, 0, 0, 0);
        let (mut clean_names, mut clean_fonts) = (0, 0);
        let mut stack = vec![std::path::PathBuf::from(root)];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().is_none_or(|x| x != "doc") {
                    continue;
                }
                let Ok(bytes) = std::fs::read(&p) else { continue };
                let Ok(mut comp) = cfb::CompoundFile::open(std::io::Cursor::new(&bytes[..])) else {
                    continue;
                };
                let mut stream = |name: &str| -> Option<Vec<u8>> {
                    use std::io::Read;
                    let mut b = Vec::new();
                    comp.open_stream(name).ok()?.read_to_end(&mut b).ok()?;
                    Some(b)
                };
                let Some(doc) = stream("WordDocument") else { continue };
                let Ok(fib) = Fib::parse(&doc) else { continue };
                if fib.encrypted {
                    continue;
                }
                let table = stream(fib.table_stream_name()).unwrap_or_default();
                files += 1;

                let sheet = StyleSheet::parse(&fib, &doc, &table);
                if !sheet.is_empty() {
                    with_styles += 1;
                }
                let mut names_ok = true;
                for s in sheet.iter() {
                    names_ok &= !s.name.is_empty() && !s.name.chars().any(|c| c.is_control());
                    // Whatever the file says, a chain is finite and repeats no
                    // slot — that is the guarantee callers rely on.
                    let chain = sheet.chain(s.istd);
                    assert!(chain.len() <= MAX_CHAIN, "{}: chaîne non bornée", p.display());
                    let mut seen = chain.clone();
                    seen.sort_unstable();
                    seen.dedup();
                    assert_eq!(seen.len(), chain.len(), "{}: cycle non coupé", p.display());
                    assert_eq!(chain.last(), Some(&s.istd));
                    let _ = sheet.papx_chain(s.istd);
                    let _ = sheet.chpx_chain(s.istd);
                    // The heading slots are the format's, never the file's.
                    if (1..=9).contains(&s.istd) && s.kind == StyleKind::Paragraph {
                        assert_eq!(
                            sheet.heading_level(s.istd),
                            Some(s.istd as u8),
                            "{}: istd {} n'est pas reconnu comme titre",
                            p.display(),
                            s.istd
                        );
                    }
                }
                clean_names += usize::from(names_ok);
                if (1..=9).any(|i| sheet.get(i).is_some()) {
                    with_headings += 1;
                }

                let fonts = FontTable::parse(&fib, &doc, &table);
                if !fonts.is_empty() {
                    with_fonts += 1;
                }
                clean_fonts += usize::from(
                    fonts
                        .iter()
                        .all(|f| !f.is_empty() && !f.chars().any(|c| c.is_control())),
                );
            }
        }
        eprintln!(
            "corpus styles: {files} fichiers · {with_styles} avec feuille de styles · \
             {with_headings} avec styles de titre · {with_fonts} avec table de polices · \
             noms de styles propres {clean_names}/{files} · polices propres {clean_fonts}/{files}"
        );
        assert!(files > 0, "aucun .doc lisible dans KUBUNO_DOC_CORPUS");
        assert!(
            with_styles * 10 >= files * 9,
            "trop de feuilles de styles vides: {with_styles}/{files}"
        );
        assert!(
            with_fonts * 10 >= files * 9,
            "trop de tables de polices vides: {with_fonts}/{files}"
        );
        // Only deliberately corrupt samples should produce unreadable names.
        assert!(
            clean_names * 100 >= files * 97,
            "trop de noms de styles illisibles: {clean_names}/{files}"
        );
        assert!(
            clean_fonts * 100 >= files * 97,
            "trop de noms de polices illisibles: {clean_fonts}/{files}"
        );
    }
}
