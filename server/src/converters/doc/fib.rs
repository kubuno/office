//! The File Information Block — the header of the `WordDocument` stream.
//!
//! Everything else in a `.doc` is found through the FIB: which table stream
//! holds the metadata, where the piece table lives, where the property bins
//! are. Reference: LibreOffice `sw/source/filter/ww8/ww8scan.cxx`
//! (`WW8Fib::WW8Fib`) and [MS-DOC] 2.5.1.

use crate::errors::{OfficeError, Result};

/// `wIdent` values Word has used. 0xA5EC is Word 97+, 0xA5DC Word 6/95,
/// 0xA5DB Word 2 — all three occur in the LibreOffice corpus, so accepting only
/// the first makes the whole Word 6/95 path unreachable.
const WORD_MAGICS: [u16; 3] = [0xA5EC, 0xA5DC, 0xA5DB];

pub(crate) fn u16_at(b: &[u8], off: usize) -> u16 {
    if off + 2 > b.len() {
        return 0;
    }
    u16::from_le_bytes([b[off], b[off + 1]])
}

pub(crate) fn i16_at(b: &[u8], off: usize) -> i16 {
    u16_at(b, off) as i16
}

pub(crate) fn u32_at(b: &[u8], off: usize) -> u32 {
    if off + 4 > b.len() {
        return 0;
    }
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

/// The subset of the FIB we actually use.
#[derive(Debug, Clone, Default)]
pub(crate) struct Fib {
    /// Format version. 101/102 = Word 6/95 (16-bit), 193+ = Word 97 and later.
    pub(crate) nfib: u16,
    /// Text encoding hint for Word 6/95, unused for 97+.
    pub(crate) lid: u16,
    /// The piece table is only present when set; otherwise the text is one run.
    pub(crate) complex: bool,
    /// Table stream to read: `1Table` when set, `0Table` otherwise.
    pub(crate) which_table_stream: bool,
    /// Whether the document is encrypted — we cannot read those.
    pub(crate) encrypted: bool,
    /// First and last character position of the main text in the stream.
    pub(crate) fc_min: u32,
    pub(crate) fc_mac: u32,
    /// Piece table (`Clx`) in the table stream.
    pub(crate) fc_clx: u32,
    pub(crate) lcb_clx: u32,
    /// Stylesheet.
    pub(crate) fc_stshf: u32,
    pub(crate) lcb_stshf: u32,
    /// Bin tables locating the character / paragraph property FKPs.
    pub(crate) fc_plcf_bte_chpx: u32,
    pub(crate) lcb_plcf_bte_chpx: u32,
    pub(crate) fc_plcf_bte_papx: u32,
    pub(crate) lcb_plcf_bte_papx: u32,
    /// Section descriptors.
    pub(crate) fc_plcf_sed: u32,
    pub(crate) lcb_plcf_sed: u32,
    /// List definitions (`LSTF`) and list format overrides (`LFO`).
    pub(crate) fc_plcf_lst: u32,
    pub(crate) lcb_plcf_lst: u32,
    pub(crate) fc_plf_lfo: u32,
    pub(crate) lcb_plf_lfo: u32,
    /// `PlcfHdd`: where each header/footer slice starts. They are NOT separate
    /// streams — they live in the same character stream, after the main text.
    pub(crate) fc_plcf_hdd: u32,
    pub(crate) lcb_plcf_hdd: u32,
    /// Font table (`SttbfFfn`): index → typeface name. Character properties
    /// carry a font INDEX, never a name, so without this every run keeps the
    /// default font.
    pub(crate) fc_sttbf_ffn: u32,
    pub(crate) lcb_sttbf_ffn: u32,
    /// Field tables: main text, then the header/footer subdocument. The
    /// character positions of the latter are relative to that subdocument.
    pub(crate) fc_plcf_fld_mom: u32,
    pub(crate) lcb_plcf_fld_mom: u32,
    pub(crate) fc_plcf_fld_hdr: u32,
    pub(crate) lcb_plcf_fld_hdr: u32,
    pub(crate) fc_sttbf_bkmk: u32,
    pub(crate) lcb_sttbf_bkmk: u32,
    pub(crate) fc_plcf_bkf: u32,
    pub(crate) lcb_plcf_bkf: u32,
    pub(crate) fc_plcf_bkl: u32,
    pub(crate) lcb_plcf_bkl: u32,
    /// Character counts of the individual text subdocuments, in order:
    /// main, footnote, header, macro, annotation, endnote, textbox, header
    /// textbox. Needed to know where the main text stops.
    pub(crate) ccp_text: u32,
    pub(crate) ccp_ftn: u32,
    pub(crate) ccp_hdd: u32,
}

impl Fib {
    pub(crate) fn parse(doc: &[u8]) -> Result<Fib> {
        if doc.len() < 98 {
            return Err(OfficeError::Conversion("flux WordDocument tronqué".into()));
        }
        if !WORD_MAGICS.contains(&u16_at(doc, 0)) {
            return Err(OfficeError::Conversion(
                "ce n'est pas un document Word binaire (wIdent absent)".into(),
            ));
        }
        let mut f = Fib {
            nfib: u16_at(doc, 2),
            lid: u16_at(doc, 6),
            ..Default::default()
        };
        let flags = u16_at(doc, 10);
        f.encrypted = flags & 0x0100 != 0;
        f.which_table_stream = flags & 0x0200 != 0;
        f.complex = flags & 0x0004 != 0;
        f.fc_min = u32_at(doc, 0x18);
        f.fc_mac = u32_at(doc, 0x1C);

        // Word 97+ lays the FibRgFcLcb out after a variable-length prefix; the
        // csw/cslw/cbRgFcLcb counters make it self-describing. Word 6/95 has a
        // fixed layout instead (LibreOffice: WW8Fib::WW8Fib, ww8scan.cxx:4900).
        if f.nfib >= 193 {
            let csw = u16_at(doc, 0x20) as usize; // count of 16-bit values
            let base = 0x22 + csw * 2;
            let cslw = u16_at(doc, base) as usize; // count of 32-bit values
            let fc_lcb_base = base + 2 + cslw * 4 + 2; // + cbRgFcLcb
            // Indices into FibRgFcLcb97, each entry a (fc, lcb) pair of u32.
            // These were verified empirically against the corpus, not guessed:
            // getting `fcPlcfBteChpx` wrong makes it read the SECTION table and
            // every character property silently lands on the wrong text.
            let g = |i: usize| u32_at(doc, fc_lcb_base + i * 4);
            f.fc_stshf = g(2);
            f.lcb_stshf = g(3);
            f.fc_plcf_sed = g(12);
            f.lcb_plcf_sed = g(13);
            f.fc_plcf_bte_chpx = g(24);
            f.lcb_plcf_bte_chpx = g(25);
            f.fc_plcf_bte_papx = g(26);
            f.lcb_plcf_bte_papx = g(27);
            f.fc_plcf_hdd = g(22);
            f.lcb_plcf_hdd = g(23);
            f.fc_sttbf_ffn = g(30);
            f.lcb_sttbf_ffn = g(31);
            f.fc_plcf_fld_mom = g(32);
            f.lcb_plcf_fld_mom = g(33);
            f.fc_plcf_fld_hdr = g(34);
            f.lcb_plcf_fld_hdr = g(35);
            f.fc_sttbf_bkmk = g(42);
            f.lcb_sttbf_bkmk = g(43);
            f.fc_plcf_bkf = g(44);
            f.lcb_plcf_bkf = g(45);
            f.fc_plcf_bkl = g(46);
            f.lcb_plcf_bkl = g(47);
            f.fc_clx = g(66);
            f.lcb_clx = g(67);
            f.fc_plcf_lst = g(146);
            f.lcb_plcf_lst = g(147);
            f.fc_plf_lfo = g(148);
            f.lcb_plf_lfo = g(149);
            // FibRgLw97: the ccp counts start at `base + 2`.
            let lw = base + 2;
            f.ccp_text = u32_at(doc, lw + 3 * 4);
            f.ccp_ftn = u32_at(doc, lw + 4 * 4);
            f.ccp_hdd = u32_at(doc, lw + 5 * 4);
        } else {
            // Word 6/95: fixed layout, no self-describing prefix. The (fc, lcb)
            // block starts at 0x58 with 8-byte pairs; offsets below were checked
            // against the three Word 6/95 files of the corpus.
            let g = |off: usize| u32_at(doc, off);
            f.fc_stshf = g(0x60);
            f.lcb_stshf = g(0x64);
            f.fc_plcf_sed = g(0x88);
            f.lcb_plcf_sed = g(0x8C);
            f.fc_plcf_bte_chpx = g(0xB8);
            f.lcb_plcf_bte_chpx = g(0xBC);
            f.fc_plcf_bte_papx = g(0xC0);
            f.lcb_plcf_bte_papx = g(0xC4);
            f.fc_plcf_hdd = g(0xB0);
            f.lcb_plcf_hdd = g(0xB4);
            f.fc_sttbf_ffn = g(0xD0);
            f.lcb_sttbf_ffn = g(0xD4);
            // `fcClx` is UNVERIFIED for this format version: not one Word 6/95
            // file of the LibreOffice corpus actually uses a piece table
            // (all report lcbClx = 0, the rest being fuzzer crash inputs), so
            // there is nothing to check the offset against. These documents are
            // read through the single-piece fallback of `piece::parse_pieces`,
            // which covers the non-complex case. Do not "fix" this offset from a
            // spec reading alone — measure it on a real complex Word 6 file.
            f.fc_clx = g(0x160);
            f.lcb_clx = g(0x164);
            f.ccp_text = g(0x34);
        }
        Ok(f)
    }

    /// Name of the table stream this document's metadata lives in.
    pub(crate) fn table_stream_name(&self) -> &'static str {
        if self.nfib < 193 {
            // Word 6/95 keeps everything in WordDocument.
            "WordDocument"
        } else if self.which_table_stream {
            "1Table"
        } else {
            "0Table"
        }
    }
}
