//! Property bins: how a character position finds its formatting.
//!
//! Word stores formatting in 512-byte pages ("formatted disk pages", FKPs) in
//! the WordDocument stream. A bin table (`PlcfbteChpx` / `PlcfbtePapx`) in the
//! table stream says which page covers which byte range; inside a page, a run
//! of byte offsets is followed by the offsets of the `grpprl` that applies.
//! Reference: LibreOffice `WW8PLCFx_Fc_FKP`, ww8scan.cxx:2400, and [MS-DOC] 2.8.
//!
//! Two coordinate systems meet here and confusing them is the classic way to
//! corrupt a `.doc` import:
//!   * FKPs index **bytes** of the WordDocument stream (`fc`);
//!   * everything above this layer works in **character positions** (`cp`).
//!
//! The piece table is the only bridge: an 8-bit piece spends one byte per
//! character, a UTF-16 piece two, and pieces appear in the stream in the order
//! they were typed rather than the order they read. `to_cp_runs` performs the
//! translation piece by piece, which is why a document with several pieces (any
//! file ever saved with "fast save") or with 8-bit text still lands on the
//! right characters.

// The consumers of this module (`chp`, `pap`, `style`, `table`) are being
// written alongside it; until they are wired up parts of the API look unused.
#![allow(dead_code)]

use super::fib::{u16_at, u32_at, Fib};
use super::piece::Piece;

/// Size of one formatted disk page.
const FKP_SIZE: usize = 512;

/// A `grpprl` that applies to a byte range of the WordDocument stream.
#[derive(Debug, Clone)]
pub(crate) struct PropRun {
    /// First byte position covered.
    pub(crate) fc_start: u32,
    /// One past the last byte position covered.
    pub(crate) fc_end: u32,
    /// The raw property list, to be decoded with `super::sprm::iter_sprms`.
    pub(crate) grpprl: Vec<u8>,
    /// Style index of the paragraph — PAPX only, `None` for character runs.
    pub(crate) istd: Option<u16>,
}

/// The same thing expressed in character positions, which is what every
/// consumer above this layer actually indexes by.
#[derive(Debug, Clone)]
pub(crate) struct CpRun {
    /// First character position covered.
    pub(crate) cp_start: u32,
    /// One past the last character position covered.
    pub(crate) cp_end: u32,
    /// The raw property list, to be decoded with `super::sprm::iter_sprms`.
    pub(crate) grpprl: Vec<u8>,
    /// Style index of the paragraph — PAPX only, `None` for character runs.
    pub(crate) istd: Option<u16>,
}

/// All the property runs of one kind (character or paragraph), sorted by
/// character position and free of overlaps.
#[derive(Debug, Clone, Default)]
pub(crate) struct PropTable {
    runs: Vec<CpRun>,
}

impl PropTable {
    /// Every run, in increasing character position order.
    pub(crate) fn runs(&self) -> &[CpRun] {
        &self.runs
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    /// The run covering `cp`, if any.
    pub(crate) fn at(&self, cp: u32) -> Option<&CpRun> {
        let i = self.runs.partition_point(|r| r.cp_start <= cp);
        let r = self.runs.get(i.checked_sub(1)?)?;
        (cp < r.cp_end).then_some(r)
    }

    /// The `grpprl` covering `cp`, or an empty slice.
    pub(crate) fn grpprl_at(&self, cp: u32) -> &[u8] {
        self.at(cp).map_or(&[][..], |r| &r.grpprl)
    }

    /// Style index of the paragraph covering `cp` (paragraph tables only).
    pub(crate) fn istd_at(&self, cp: u32) -> Option<u16> {
        self.at(cp)?.istd
    }

    /// Every run intersecting `[cp_start, cp_end)`, in order.
    ///
    /// Runs are contiguous and sorted, so the answer is always a slice.
    pub(crate) fn range(&self, cp_start: u32, cp_end: u32) -> &[CpRun] {
        if cp_end <= cp_start {
            return &[];
        }
        let lo = self.runs.partition_point(|r| r.cp_end <= cp_start);
        let hi = self.runs.partition_point(|r| r.cp_start < cp_end);
        self.runs.get(lo..hi.max(lo)).unwrap_or(&[])
    }
}

/// Both property tables of a document, ready to be queried by character
/// position.
#[derive(Debug, Clone, Default)]
pub(crate) struct DocProps {
    /// Character formatting (CHPX).
    pub(crate) chars: PropTable,
    /// Paragraph formatting (PAPX), one run per paragraph.
    pub(crate) paras: PropTable,
}

/// Read both property bins of a document.
///
/// `data` is the `Data` stream of the compound file; it may be empty, in which
/// case the few paragraphs whose `grpprl` overflowed its 512-byte page (the
/// "huge PAPX" indirection) simply come back without properties.
pub(crate) fn read_props(
    fib: &Fib,
    doc: &[u8],
    table: &[u8],
    data: &[u8],
    pieces: &[Piece],
) -> DocProps {
    DocProps {
        chars: char_props(fib, doc, table, pieces),
        paras: para_props(fib, doc, table, data, pieces),
    }
}

/// Character property runs of the whole document, in character positions.
pub(crate) fn char_props(fib: &Fib, doc: &[u8], table: &[u8], pieces: &[Piece]) -> PropTable {
    PropTable { runs: to_cp_runs(&char_runs(fib, doc, table), pieces) }
}

/// Paragraph property runs of the whole document, in character positions.
pub(crate) fn para_props(
    fib: &Fib,
    doc: &[u8],
    table: &[u8],
    data: &[u8],
    pieces: &[Piece],
) -> PropTable {
    PropTable { runs: to_cp_runs(&para_runs(fib, doc, table, data), pieces) }
}

// ---------------------------------------------------------------------------
// Byte-space layer: walking the bin tables and decoding the pages.
// ---------------------------------------------------------------------------

/// Character property runs in byte positions, sorted and gap-free per page.
pub(crate) fn char_runs(fib: &Fib, doc: &[u8], table: &[u8]) -> Vec<PropRun> {
    let pages = chpx_pages(fib, doc, table);
    collect(doc, &pages, chpx_page)
}

/// Paragraph property runs in byte positions.
pub(crate) fn para_runs(fib: &Fib, doc: &[u8], table: &[u8], data: &[u8]) -> Vec<PropRun> {
    let pages = papx_pages(fib, doc, table);
    let eight_plus = fib.nfib >= 193;
    let item = papx_item_size(fib);
    collect(doc, &pages, |page| papx_page_ex(page, item, eight_plus, data))
}

/// Read every listed page out of the WordDocument stream and decode it.
fn collect(doc: &[u8], pages: &[u32], decode: impl Fn(&[u8]) -> Vec<PropRun>) -> Vec<PropRun> {
    let mut out: Vec<PropRun> = Vec::new();
    for &pn in pages {
        // A page number is an index of 512-byte blocks from the start of the
        // stream; `pn << 9` can overflow on a corrupt file.
        let Some(off) = (pn as usize).checked_mul(FKP_SIZE) else { continue };
        let Some(page) = doc.get(off..off + FKP_SIZE) else { continue };
        out.extend(decode(page));
    }
    out.sort_by_key(|r| r.fc_start);
    out
}

/// Where a bin table lives and how to rebuild it when it is incomplete.
struct BinLoc {
    /// The `PlcfbteChpx` / `PlcfbtePapx` itself, in the table stream.
    fc: u32,
    lcb: u32,
    /// First FKP page and page count, used when the table above is truncated.
    pn_first: u32,
    cpn_bte: u32,
}

/// Locate a bin table in the FIB.
///
/// This deliberately re-reads the raw FIB instead of using `Fib`'s fields:
/// `FibRgFcLcb97` is an array of *pairs*, so `fcPlcfbteChpx` is the 13th pair,
/// i.e. the 24th 32-bit word — an off-by-a-factor-of-two that silently yields a
/// different (and perfectly plausible looking) table.
fn bin_loc(fib: &Fib, doc: &[u8], chp: bool) -> BinLoc {
    if fib.nfib >= 193 {
        let csw = u16_at(doc, 0x20) as usize;
        let base = 0x22 + csw * 2;
        let cslw = u16_at(doc, base) as usize;
        let lw = base + 2; // FibRgLw97
        let fclcb = lw + cslw * 4 + 2; // + cbRgFcLcb
        let pair = if chp { 12 } else { 13 };
        // FibRgLw97: pnFbpChpFirst, pnChpFirst, cpnBteChp, then the same
        // three for paragraphs.
        let (pn_i, cpn_i) = if chp { (12, 13) } else { (15, 16) };
        BinLoc {
            fc: u32_at(doc, fclcb + pair * 8),
            lcb: u32_at(doc, fclcb + pair * 8 + 4),
            pn_first: u32_at(doc, lw + pn_i * 4),
            cpn_bte: u32_at(doc, lw + cpn_i * 4),
        }
    } else {
        // Word 6/95 has a fixed FIB layout.
        let (fc_off, lcb_off) = if chp { (0xFA, 0xFE) } else { (0x102, 0x106) };
        BinLoc {
            fc: u32_at(doc, fc_off),
            lcb: u32_at(doc, lcb_off),
            pn_first: 0,
            cpn_bte: 0,
        }
    }
}

/// Page numbers of the FKPs holding character properties.
fn chpx_pages(fib: &Fib, doc: &[u8], table: &[u8]) -> Vec<u32> {
    fkp_pages(fib, doc, table, true)
}

/// Page numbers of the FKPs holding paragraph properties.
fn papx_pages(fib: &Fib, doc: &[u8], table: &[u8]) -> Vec<u32> {
    fkp_pages(fib, doc, table, false)
}

fn fkp_pages(fib: &Fib, doc: &[u8], table: &[u8], chp: bool) -> Vec<u32> {
    let loc = bin_loc(fib, doc, chp);
    let mut pages = bin_table_pages_ver(table, loc.fc, loc.lcb, fib.nfib);
    if pages.is_empty() {
        // Trust `Fib` as a second opinion: if its field numbering is ever
        // corrected, or a future version moves things around, this still works.
        let (fc, lcb) = if chp {
            (fib.fc_plcf_bte_chpx, fib.lcb_plcf_bte_chpx)
        } else {
            (fib.fc_plcf_bte_papx, fib.lcb_plcf_bte_papx)
        };
        pages = bin_table_pages_ver(table, fc, lcb, fib.nfib);
    }
    // Word 6/95 and WordPad ship truncated bin tables and expect the reader to
    // rebuild the missing part: the FKPs are then simply consecutive pages
    // (LibreOffice `WW8PLCF::GeneratePLCF`).
    if pages.len() < loc.cpn_bte as usize {
        let generated = generated_pages(fib, doc, loc.pn_first, loc.cpn_bte);
        if !generated.is_empty() {
            return generated;
        }
    }
    if pages.is_empty() {
        return generated_pages_legacy(fib, doc, chp);
    }
    pages
}

/// Page numbers of the FKPs holding character or paragraph properties.
///
/// A bin table is a PLCF: n+1 byte positions followed by n page numbers.
/// Word 97 stores the page numbers on 4 bytes, Word 6/95 on 2.
pub(crate) fn bin_table_pages(table: &[u8], fc: u32, lcb: u32) -> Vec<u32> {
    bin_table_pages_ver(table, fc, lcb, 193)
}

fn bin_table_pages_ver(table: &[u8], fc: u32, lcb: u32, nfib: u16) -> Vec<u32> {
    let stru = if nfib >= 193 { 4usize } else { 2 };
    let start = fc as usize;
    let end = start.saturating_add(lcb as usize);
    // A bin table that does not fit in its stream is not a bin table: reading
    // the truncated remainder would hand hundreds of arbitrary page numbers to
    // the decoder and, worse, hide the fact that the real table is elsewhere
    // (LibreOffice `WW8PLCF::ReadPLCF` rejects it the same way).
    if end > table.len() || start >= end || end - start < 4 + 4 + stru {
        return Vec::new();
    }
    let plc = &table[start..end];
    let n = (plc.len() - 4) / (4 + stru);
    let base = 4 * (n + 1);
    (0..n)
        .map(|i| {
            if stru == 4 {
                u32_at(plc, base + i * 4)
            } else {
                u16_at(plc, base + i * 2) as u32
            }
        })
        .collect()
}

/// Rebuild a bin table as `cpn` consecutive pages starting at `pn`.
///
/// We validate rather than trust: the first FC of the first page must be the
/// start of the document's text, otherwise a bad guess would feed arbitrary
/// bytes to the page decoder.
fn generated_pages(fib: &Fib, doc: &[u8], pn: u32, cpn: u32) -> Vec<u32> {
    if cpn == 0 || cpn > 0xFFFF || pn == 0 {
        return Vec::new();
    }
    let Some(off) = (pn as usize).checked_mul(FKP_SIZE) else { return Vec::new() };
    if doc.len() < off + FKP_SIZE || u32_at(doc, off) != fib.fc_min {
        return Vec::new();
    }
    (0..cpn).map(|i| pn + i).collect()
}

/// The Word 6/95 FIB keeps `pnChpFirst` / `cpnBteChp` after the FC/LCB array,
/// but its exact position depends on the sub-version. Two layouts are known in
/// the wild; `generated_pages` tells us which one is real.
fn generated_pages_legacy(fib: &Fib, doc: &[u8], chp: bool) -> Vec<u32> {
    if fib.nfib >= 193 {
        return Vec::new();
    }
    for base in [0x1CC_usize, 0x18A] {
        let (pn_off, cpn_off) = if chp { (base, base + 4) } else { (base + 2, base + 6) };
        let pages = generated_pages(
            fib,
            doc,
            u16_at(doc, pn_off) as u32,
            u16_at(doc, cpn_off) as u32,
        );
        if !pages.is_empty() {
            return pages;
        }
    }
    Vec::new()
}

/// Byte length of one entry of a PAPX FKP's `rgbx` array.
///
/// Word 97 stores a full `BX` (offset byte + 12-byte paragraph height), Word
/// 6/95 a shorter one (offset byte + 6-byte height).
fn papx_item_size(fib: &Fib) -> usize {
    if fib.nfib >= 193 {
        13
    } else {
        7
    }
}

/// The FC array and entry count shared by both page layouts.
///
/// Returns `None` when the page is not a plausible FKP.
fn page_header(page: &[u8], item_size: usize) -> Option<usize> {
    if page.len() < FKP_SIZE || item_size == 0 {
        return None;
    }
    let crun = page[FKP_SIZE - 1] as usize;
    if crun == 0 {
        return None;
    }
    // (crun + 1) FCs then crun entries, all before the count byte.
    if (crun + 1) * 4 + crun * item_size > FKP_SIZE - 1 {
        return None;
    }
    Some(crun)
}

/// Decode one 512-byte CHPX FKP page into property runs.
///
/// Layout ([MS-DOC] 2.9.32): `rgfc` holds `crun + 1` byte positions, `rgb`
/// holds `crun` single-byte word offsets. A zero offset means "no properties";
/// otherwise the byte at `offset * 2` is the length of the `grpprl` that
/// follows it.
pub(crate) fn chpx_page(page: &[u8]) -> Vec<PropRun> {
    let Some(crun) = page_header(page, 1) else { return Vec::new() };
    let base = (crun + 1) * 4;
    let mut out = Vec::with_capacity(crun);
    for i in 0..crun {
        let fc_start = u32_at(page, i * 4);
        let fc_end = u32_at(page, (i + 1) * 4);
        if fc_end <= fc_start {
            continue;
        }
        let n_ofs = page.get(base + i).copied().unwrap_or(0) as usize * 2;
        let grpprl = if n_ofs == 0 {
            Vec::new()
        } else {
            let cb = page.get(n_ofs).copied().unwrap_or(0) as usize;
            let from = n_ofs + 1;
            let to = from.saturating_add(cb).min(FKP_SIZE);
            page.get(from..to).unwrap_or(&[]).to_vec()
        };
        out.push(PropRun { fc_start, fc_end, grpprl, istd: None });
    }
    out
}

/// Decode one 512-byte PAPX FKP page into property runs.
pub(crate) fn papx_page(page: &[u8]) -> Vec<PropRun> {
    papx_page_ex(page, 13, true, &[])
}

/// Decode one 512-byte PAPX FKP page.
///
/// Layout ([MS-DOC] 2.9.24 `PapxFkp`): after the `crun + 1` byte positions come
/// `crun` `BX` structures whose first byte is again a word offset. Unlike a
/// CHPX, what sits at that offset is a *word* count `cb`: when it is non-zero
/// the payload is `2 * cb - 1` bytes, when it is zero the real count is in the
/// next byte and the payload is `2 * cb'` bytes. The payload starts with the
/// paragraph's style index (`istd`, 2 bytes), the rest is the `grpprl`.
/// (LibreOffice reads one byte more than the spec allows in the first case and
/// so do we: a trailing odd byte can never form a sprm, whereas truncating
/// would drop a whole property.)
fn papx_page_ex(page: &[u8], item_size: usize, eight_plus: bool, data: &[u8]) -> Vec<PropRun> {
    let Some(crun) = page_header(page, item_size) else { return Vec::new() };
    let base = (crun + 1) * 4;
    let mut out = Vec::with_capacity(crun);
    for i in 0..crun {
        let fc_start = u32_at(page, i * 4);
        let fc_end = u32_at(page, (i + 1) * 4);
        if fc_end <= fc_start {
            continue;
        }
        let n_ofs = page.get(base + i * item_size).copied().unwrap_or(0) as usize * 2;
        let (mut istd, mut grpprl) = (None, Vec::new());
        if n_ofs != 0 {
            let mut cb = page.get(n_ofs).copied().unwrap_or(0) as usize;
            let mut delta = 0usize;
            if eight_plus && cb == 0 {
                cb = page.get(n_ofs + 1).copied().unwrap_or(0) as usize;
                delta = 1;
            }
            let len = cb * 2;
            if len >= 2 {
                let doff = n_ofs + 1 + delta;
                istd = Some(u16_at(page, doff));
                let from = doff + 2;
                let to = from.saturating_add(len - 2).min(FKP_SIZE);
                grpprl = page.get(from..to).unwrap_or(&[]).to_vec();
            }
        }
        if let Some(expanded) = huge_papx(&grpprl, data) {
            grpprl = expanded;
        }
        out.push(PropRun { fc_start, fc_end, grpprl, istd });
    }
    out
}

/// A `grpprl` too big for its page is replaced by a pointer into the `Data`
/// stream: sprm 0x6646 replaces the list outright, 0x646B prepends to it
/// (LibreOffice `IsReplaceAllSprm` / `IsExpandableSprm`).
fn huge_papx(grpprl: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    if grpprl.len() < 6 {
        return None;
    }
    let id = u16::from_le_bytes([grpprl[0], grpprl[1]]);
    let expand = match id {
        0x6646 => false,
        0x646B => true,
        _ => return None,
    };
    let pos = u32_at(grpprl, 2) as usize;
    let len = u16_at(data, pos) as usize;
    let from = pos.checked_add(2)?;
    let to = from.checked_add(len)?;
    let body = data.get(from..to)?;
    let mut out = body.to_vec();
    if expand {
        out.extend_from_slice(grpprl);
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// Byte positions → character positions.
// ---------------------------------------------------------------------------

/// A piece seen from the byte side: which stream bytes it owns and which
/// character position they start at.
#[derive(Debug, Clone, Copy)]
struct FcPiece {
    fc_start: u32,
    fc_end: u32,
    cp_start: u32,
    /// Bytes per character: 1 for a legacy code page, 2 for UTF-16.
    unit: u32,
}

fn fc_pieces(pieces: &[Piece]) -> Vec<FcPiece> {
    let mut out: Vec<FcPiece> = pieces
        .iter()
        .filter(|p| p.cp_end > p.cp_start)
        .map(|p| {
            let unit = if p.compressed { 1 } else { 2 };
            let len = (p.cp_end - p.cp_start).saturating_mul(unit);
            FcPiece {
                fc_start: p.fc,
                fc_end: p.fc.saturating_add(len),
                cp_start: p.cp_start,
                unit,
            }
        })
        .collect();
    out.sort_by_key(|p| p.fc_start);
    out
}

/// Translate byte-space runs into character-space runs.
///
/// A run is cut at every piece boundary it crosses, because two pieces can be
/// far apart in the stream yet adjacent in the text (and can even use different
/// character widths). The results are then re-sorted — the stream order of the
/// pieces has nothing to do with the reading order — de-overlapped, and runs
/// that a piece boundary split needlessly are glued back together.
pub(crate) fn to_cp_runs(runs: &[PropRun], pieces: &[Piece]) -> Vec<CpRun> {
    let fcp = fc_pieces(pieces);
    if fcp.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<CpRun> = Vec::with_capacity(runs.len());
    for r in runs {
        if r.fc_end <= r.fc_start {
            continue;
        }
        // First piece that can possibly reach into this run.
        let mut i = fcp.partition_point(|p| p.fc_end <= r.fc_start);
        while let Some(p) = fcp.get(i) {
            if p.fc_start >= r.fc_end {
                break;
            }
            i += 1;
            let a = r.fc_start.max(p.fc_start);
            let b = r.fc_end.min(p.fc_end);
            if b <= a {
                continue;
            }
            // Round the start down and the end up so a run never loses the
            // character it partially covers.
            let cp_start = p.cp_start + (a - p.fc_start) / p.unit;
            let cp_end = p.cp_start + (b - p.fc_start).div_ceil(p.unit);
            if cp_end > cp_start {
                out.push(CpRun {
                    cp_start,
                    cp_end,
                    grpprl: r.grpprl.clone(),
                    istd: r.istd,
                });
            }
        }
    }
    normalise(out)
}

/// Sort by character position, drop overlaps, merge identical neighbours.
fn normalise(mut runs: Vec<CpRun>) -> Vec<CpRun> {
    runs.sort_by_key(|r| (r.cp_start, r.cp_end));
    let mut out: Vec<CpRun> = Vec::with_capacity(runs.len());
    for mut r in runs {
        match out.last_mut() {
            Some(prev) if prev.cp_end >= r.cp_end => continue,
            Some(prev) if prev.cp_end > r.cp_start => r.cp_start = prev.cp_end,
            _ => {}
        }
        match out.last_mut() {
            // A paragraph or a character run cut in two by a piece boundary is
            // one run again as soon as the properties match.
            Some(prev)
                if prev.cp_end == r.cp_start
                    && prev.istd == r.istd
                    && prev.grpprl == r.grpprl =>
            {
                prev.cp_end = r.cp_end;
            }
            _ => out.push(r),
        }
    }
    out
}

#[cfg(test)]
mod fkp_tests {
    use super::*;

    /// Build a minimal CHPX page: two runs, the second one with one sprm.
    fn fake_chpx_page() -> Vec<u8> {
        let mut p = vec![0u8; FKP_SIZE];
        p[FKP_SIZE - 1] = 2; // crun
        for (i, fc) in [100u32, 110, 120].iter().enumerate() {
            p[i * 4..i * 4 + 4].copy_from_slice(&fc.to_le_bytes());
        }
        let base = 3 * 4;
        p[base] = 0; // first run: no properties
        p[base + 1] = 100; // second run: grpprl at byte 200
        p[200] = 3; // cb
        p[201..204].copy_from_slice(&[0x35, 0x08, 0x01]); // sprmCFBold on
        p
    }

    #[test]
    fn chpx_page_decodes_offsets_and_lengths() {
        let runs = chpx_page(&fake_chpx_page());
        assert_eq!(runs.len(), 2);
        assert_eq!((runs[0].fc_start, runs[0].fc_end), (100, 110));
        assert!(runs[0].grpprl.is_empty());
        assert_eq!((runs[1].fc_start, runs[1].fc_end), (110, 120));
        assert_eq!(runs[1].grpprl, vec![0x35, 0x08, 0x01]);
        assert!(runs[1].istd.is_none());
    }

    #[test]
    fn papx_page_decodes_istd_and_grpprl() {
        let mut p = vec![0u8; FKP_SIZE];
        p[FKP_SIZE - 1] = 1;
        p[0..4].copy_from_slice(&1000u32.to_le_bytes());
        p[4..8].copy_from_slice(&1050u32.to_le_bytes());
        let base = 2 * 4;
        p[base] = 100; // BX offset byte → grpprl at 200
        p[200] = 3; // cb words → 6 bytes: istd + 4
        p[201..203].copy_from_slice(&7u16.to_le_bytes()); // istd
        p[203..207].copy_from_slice(&[0x0F, 0x24, 0x02, 0x00]); // sprmPFInTable-ish
        let runs = papx_page(&p);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].istd, Some(7));
        assert_eq!(runs[0].grpprl.len(), 4);
    }

    #[test]
    fn truncated_or_absurd_pages_yield_nothing() {
        assert!(chpx_page(&[]).is_empty());
        assert!(chpx_page(&[0u8; 100]).is_empty());
        // crun so large the entries cannot fit.
        let mut p = vec![0u8; FKP_SIZE];
        p[FKP_SIZE - 1] = 255;
        assert!(papx_page(&p).is_empty());
        // Offsets pointing past the page must not panic.
        let mut p = vec![0xFFu8; FKP_SIZE];
        p[FKP_SIZE - 1] = 1;
        let _ = chpx_page(&p);
        let _ = papx_page(&p);
    }

    fn piece(cp_start: u32, cp_end: u32, fc: u32, compressed: bool) -> Piece {
        Piece { cp_start, cp_end, fc, compressed }
    }

    #[test]
    fn cp_conversion_follows_8bit_pieces() {
        // 10 characters of 8-bit text at byte 1000.
        let pieces = [piece(0, 10, 1000, true)];
        let runs = [PropRun {
            fc_start: 1002,
            fc_end: 1005,
            grpprl: vec![1],
            istd: None,
        }];
        let cp = to_cp_runs(&runs, &pieces);
        assert_eq!(cp.len(), 1);
        assert_eq!((cp[0].cp_start, cp[0].cp_end), (2, 5));
    }

    #[test]
    fn cp_conversion_follows_utf16_pieces() {
        let pieces = [piece(0, 10, 1000, false)];
        let runs = [PropRun {
            fc_start: 1004,
            fc_end: 1010,
            grpprl: vec![1],
            istd: None,
        }];
        let cp = to_cp_runs(&runs, &pieces);
        assert_eq!((cp[0].cp_start, cp[0].cp_end), (2, 5));
    }

    #[test]
    fn pieces_out_of_stream_order_are_reordered() {
        // The second half of the text was typed first and lives earlier in the
        // stream: reading order and byte order disagree.
        let pieces = [piece(0, 4, 2000, true), piece(4, 8, 1000, true)];
        let runs = [
            PropRun { fc_start: 1000, fc_end: 1004, grpprl: vec![0xAA], istd: None },
            PropRun { fc_start: 2000, fc_end: 2004, grpprl: vec![0xBB], istd: None },
        ];
        let cp = to_cp_runs(&runs, &pieces);
        assert_eq!(cp.len(), 2);
        assert_eq!((cp[0].cp_start, cp[0].grpprl[0]), (0, 0xBB));
        assert_eq!((cp[1].cp_start, cp[1].grpprl[0]), (4, 0xAA));
    }

    #[test]
    fn a_run_spanning_two_pieces_is_glued_back() {
        let pieces = [piece(0, 4, 1000, true), piece(4, 8, 1004, true)];
        let runs = [PropRun { fc_start: 1000, fc_end: 1008, grpprl: vec![1], istd: None }];
        let cp = to_cp_runs(&runs, &pieces);
        assert_eq!(cp.len(), 1);
        assert_eq!((cp[0].cp_start, cp[0].cp_end), (0, 8));
    }

    #[test]
    fn lookup_finds_the_covering_run() {
        let t = PropTable {
            runs: vec![
                CpRun { cp_start: 0, cp_end: 5, grpprl: vec![1], istd: Some(1) },
                CpRun { cp_start: 5, cp_end: 9, grpprl: vec![2], istd: Some(2) },
            ],
        };
        assert_eq!(t.at(0).map(|r| r.cp_end), Some(5));
        assert_eq!(t.at(4).map(|r| r.cp_end), Some(5));
        assert_eq!(t.at(5).map(|r| r.cp_end), Some(9));
        assert!(t.at(9).is_none());
        assert_eq!(t.istd_at(6), Some(2));
        assert_eq!(t.range(3, 6).len(), 2);
        assert_eq!(t.range(0, 1).len(), 1);
        assert_eq!(t.range(20, 30).len(), 0);
    }

    // -----------------------------------------------------------------
    // Corpus: real Word binaries, where the interesting bugs live.
    // -----------------------------------------------------------------

    /// One document's worth of statistics.
    #[derive(Default)]
    struct Stat {
        files: usize,
        with_chpx: usize,
        with_papx: usize,
        chpx_covered: u64,
        papx_covered: u64,
        total_cp: u64,
        with_istd: usize,
        zeros: usize,
    }

    /// How much of `[0, ccp)` the runs cover, and that they are well ordered.
    fn check_table(t: &PropTable, ccp: u32, what: &str, path: &std::path::Path) -> u64 {
        let mut covered = 0u64;
        let mut prev_end = 0u32;
        for r in t.runs() {
            assert!(
                r.cp_start < r.cp_end,
                "{what} vide dans {}: {}..{}",
                path.display(),
                r.cp_start,
                r.cp_end
            );
            assert!(
                r.cp_start >= prev_end,
                "{what} désordonné dans {}: {} < {prev_end}",
                path.display(),
                r.cp_start
            );
            prev_end = r.cp_end;
            let a = r.cp_start.min(ccp);
            let b = r.cp_end.min(ccp);
            covered += u64::from(b.saturating_sub(a));
            // Lookups must agree with the run we just walked.
            if b > a {
                let found = t.at(a).expect("at() ne retrouve pas un run existant");
                assert_eq!(found.cp_start, r.cp_start);
            }
        }
        covered
    }

    #[test]
    fn corpus_property_bins_are_consistent() {
        let Ok(root) = std::env::var("KUBUNO_DOC_CORPUS") else {
            eprintln!("KUBUNO_DOC_CORPUS absent — test ignoré");
            return;
        };
        let mut st = Stat::default();
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
                let Some(doc) = super::super::read_stream(&mut comp, "WordDocument") else {
                    continue;
                };
                let Ok(fib) = Fib::parse(&doc) else { continue };
                // Several corpus files are fuzzer output whose `ccpText` is
                // nonsense (a billion characters in a 4 kB stream); they would
                // swamp the coverage figure below.
                if fib.encrypted || fib.ccp_text == 0 || fib.ccp_text as usize > doc.len() {
                    continue;
                }
                let table =
                    super::super::read_stream(&mut comp, fib.table_stream_name()).unwrap_or_default();
                let data = super::super::read_stream(&mut comp, "Data").unwrap_or_default();
                let pieces = super::super::piece::parse_pieces(&fib, &table);

                // Byte-space invariants first.
                let cr = char_runs(&fib, &doc, &table);
                let pr = para_runs(&fib, &doc, &table, &data);
                for w in cr.windows(2) {
                    assert!(w[0].fc_start <= w[1].fc_start, "CHPX FC désordonnés");
                }
                for w in pr.windows(2) {
                    assert!(w[0].fc_start <= w[1].fc_start, "PAPX FC désordonnés");
                }

                let props = read_props(&fib, &doc, &table, &data, &pieces);
                st.files += 1;
                st.total_cp += u64::from(fib.ccp_text);
                if !props.chars.is_empty() {
                    st.with_chpx += 1;
                }
                if !props.paras.is_empty() {
                    st.with_papx += 1;
                }
                if props.paras.runs().iter().any(|r| r.istd.is_some_and(|i| i != 0)) {
                    st.with_istd += 1;
                }
                let c = check_table(&props.chars, fib.ccp_text, "CHPX", &p);
                st.chpx_covered += c;
                st.papx_covered += check_table(&props.paras, fib.ccp_text, "PAPX", &p);
                // Naming the documents we get nothing out of is what makes this
                // test useful when the format handling regresses.
                if c == 0 {
                    st.zeros += 1;
                    if st.zeros <= 5 {
                        eprintln!(
                            "  aucune propriété: {} (nfib={}, {} cp, {} pièces, {} FKP runs)",
                            p.display(),
                            fib.nfib,
                            fib.ccp_text,
                            pieces.len(),
                            cr.len()
                        );
                    }
                }
            }
        }
        let pct = |n: u64| {
            if st.total_cp == 0 {
                0.0
            } else {
                n as f64 * 100.0 / st.total_cp as f64
            }
        };
        eprintln!(
            "corpus fkp: {} fichiers · {} cp au total · CHPX {} ({} cp, {:.1}%) · \
             PAPX {} ({} cp, {:.1}%) · istd non nul {}",
            st.files,
            st.total_cp,
            st.with_chpx,
            st.chpx_covered,
            pct(st.chpx_covered),
            st.with_papx,
            st.papx_covered,
            pct(st.papx_covered),
            st.with_istd
        );
        assert!(st.files > 0, "aucun .doc lisible dans KUBUNO_DOC_CORPUS");
        // A property bin that covers almost nothing means the FKP walk is
        // broken; real Word files cover essentially all of their text.
        assert!(pct(st.chpx_covered) > 90.0, "couverture CHPX trop faible");
        assert!(pct(st.papx_covered) > 90.0, "couverture PAPX trop faible");
    }
}
