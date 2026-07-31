//! Sprms — "single property modifiers", the atoms of formatting in a `.doc`.
//!
//! A sprm is an opcode followed by an operand. In Word 97+ (WW8) the opcode is
//! 16 bits and self-describing: bit 9 is `fSpec`, bits 10-12 (`sgc`) say which
//! kind of property it touches (paragraph, character, picture, section, table)
//! and bits 13-15 (`spra`) give the operand size, so an unknown sprm can still
//! be SKIPPED correctly instead of desynchronising the whole stream.
//!
//! Word 6/95 (WW6/WW7) used 8-bit opcodes with no size information at all: the
//! reader must carry a full table of sizes, and unknown opcodes are assumed to
//! be length-prefixed. Both encodings still turn up in the wild — the corpus
//! shipped with LibreOffice contains three of them.
//!
//! **Why the sizes matter so much**: a grpprl is a packed list with no
//! separators. One wrong operand size shifts every following property by that
//! many bytes, and the rest of the list decodes into plausible-looking
//! garbage — silently. So the size rules below are transcribed from the
//! reference implementation rather than guessed:
//! LibreOffice `sw/source/filter/ww8/ww8scan.cxx` (`wwSprmParser::GetSprmInfo`,
//! `GetSprmTailLen`, `GetSprmSize`, `WW8SprmIter`) plus `sprmids.hxx`, checked
//! against [MS-DOC] 2.6.1-2.6.5.
//!
//! The size of one sprm, following `GetSprmSize`, is
//! `id_bytes + data_offset + tail`, where `data_offset` is 0 for a fixed
//! operand, 1 for a one-byte length prefix and 2 for a two-byte one.

// This module is a reference table: the opcode constants and the typed operand
// accessors exist so that the property readers built on top of it never write a
// raw hex literal, and they come into use one at a time.
#![allow(dead_code)]

use super::fib::Fib;

// ---------------------------------------------------------------------------
// Version
// ---------------------------------------------------------------------------

/// Which sprm encoding a document uses.
///
/// Selected from the FIB: `nFib < 193` means Word 6/95, which uses 8-bit
/// opcodes and a completely different id space (`sprmCFBold` is 85, not
/// 0x0835). Everything from Word 97 on uses the 16-bit encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SprmVersion {
    /// Word 6.0 / Word 95 — 8-bit opcodes.
    Ww6,
    /// Word 6/95 written by an East-Asian build, where opcodes 111-113 carry a
    /// two-byte operand instead of being length-prefixed. LibreOffice detects
    /// this from `wIdent` in 0xA697..=0xA699 (`wwSprmSearcher::patchCJKVariant`).
    /// The FIB parser does not expose `wIdent` today, so nothing selects this
    /// automatically yet; it exists so that it can, without touching callers.
    Ww6Cjk,
    /// Word 97 and later — 16-bit opcodes.
    Ww8,
}

impl SprmVersion {
    /// Pick the encoding from the FIB. Word 6/95 stops at `nFib` 105.
    pub(crate) fn from_fib(fib: &Fib) -> SprmVersion {
        if fib.nfib < 193 {
            SprmVersion::Ww6
        } else {
            SprmVersion::Ww8
        }
    }

    fn is_ww6(self) -> bool {
        !matches!(self, SprmVersion::Ww8)
    }

    /// Width of the opcode itself.
    fn id_bytes(self) -> usize {
        if self.is_ww6() {
            1
        } else {
            2
        }
    }

    /// Smallest buffer that can still hold a sprm; below this, stop.
    /// (`wwSprmParser::MinSprmLen`.)
    fn min_len(self) -> usize {
        if self.is_ww6() {
            2
        } else {
            3
        }
    }
}

// ---------------------------------------------------------------------------
// Categories and the decoded sprm
// ---------------------------------------------------------------------------

/// Which category of property a sprm belongs to (`sgc`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Sgc {
    Paragraph,
    Character,
    Picture,
    Section,
    Table,
    Unknown,
}

/// One decoded sprm: its opcode and the bytes of its operand.
///
/// For a length-prefixed sprm the count byte(s) are NOT part of `operand`:
/// callers get the payload only.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Sprm<'a> {
    pub(crate) opcode: u16,
    pub(crate) operand: &'a [u8],
}

impl Sprm<'_> {
    /// Category, from bits 10-12. Meaningless for Word 6/95 opcodes, which are
    /// classified by range instead — use [`Sprm::sgc_ver`].
    pub(crate) fn sgc(&self) -> Sgc {
        match (self.opcode >> 10) & 0x07 {
            1 => Sgc::Paragraph,
            2 => Sgc::Character,
            3 => Sgc::Picture,
            4 => Sgc::Section,
            5 => Sgc::Table,
            _ => Sgc::Unknown,
        }
    }

    /// Category for either encoding. Word 6/95 has no `sgc` field; its opcode
    /// ranges are fixed (paragraph 2-63, character 64-118, picture 119-130,
    /// section 131-181, table 182-207).
    pub(crate) fn sgc_ver(&self, ver: SprmVersion) -> Sgc {
        if !ver.is_ww6() {
            return self.sgc();
        }
        match self.opcode {
            2..=63 => Sgc::Paragraph,
            64..=118 => Sgc::Character,
            119..=130 => Sgc::Picture,
            131..=181 => Sgc::Section,
            182..=207 => Sgc::Table,
            _ => Sgc::Unknown,
        }
    }

    /// The low 9 bits identify the property within its category.
    pub(crate) fn ispmd(&self) -> u16 {
        self.opcode & 0x01FF
    }

    /// Bit 9: "this sprm has a special interpretation" — set on the opcodes
    /// Word treats as complex structures rather than plain values.
    pub(crate) fn f_spec(&self) -> bool {
        self.opcode & 0x0200 != 0
    }

    /// Semantic identity of this sprm, for the WW8 id space.
    pub(crate) fn kind8(&self) -> SprmKind {
        kind_of(self.opcode, SprmVersion::Ww8)
    }

    /// Semantic identity of this sprm in a given encoding.
    pub(crate) fn kind(&self, ver: SprmVersion) -> SprmKind {
        kind_of(self.opcode, ver)
    }

    /// Operand as an unsigned byte / word / dword, when it is that size.
    pub(crate) fn u8(&self) -> Option<u8> {
        self.operand.first().copied()
    }
    pub(crate) fn i8(&self) -> Option<i8> {
        self.u8().map(|v| v as i8)
    }
    pub(crate) fn u16(&self) -> Option<u16> {
        let (a, b) = (*self.operand.first()?, *self.operand.get(1)?);
        Some(u16::from_le_bytes([a, b]))
    }
    pub(crate) fn i16(&self) -> Option<i16> {
        self.u16().map(|v| v as i16)
    }
    pub(crate) fn u32(&self) -> Option<u32> {
        let b = self.operand.get(..4)?;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    pub(crate) fn i32(&self) -> Option<i32> {
        self.u32().map(|v| v as i32)
    }
    /// A toggle operand: 0 off, 1 on, 128 inherit, 129 invert — same convention
    /// OOXML later spelled out in `w:val` (ww8scan.cxx, `Get_Toggle`).
    pub(crate) fn toggle(&self, inherited: bool) -> Option<bool> {
        match self.u8()? {
            0 => Some(false),
            1 => Some(true),
            128 => None,
            129 => Some(!inherited),
            _ => Some(true),
        }
    }
}

// ---------------------------------------------------------------------------
// Operand size rules
// ---------------------------------------------------------------------------

/// How the operand length is expressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Vari {
    /// Fixed size, no prefix.
    Fix,
    /// One-byte count, then that many bytes (plus `Rule::len` extra bytes for
    /// the handful of sprms whose table row carries a non-zero fixed part).
    Var,
    /// Two-byte count. Used only by `sprmTDefTable`/`sprmTDefTable10`, whose
    /// count is the size of the whole operand (count field included) MINUS 1.
    Var2,
}

/// One row of the size table: `nLen` and `nVari` of LibreOffice's `SprmInfo`.
#[derive(Debug, Clone, Copy)]
struct Rule {
    len: u8,
    vari: Vari,
}

impl Rule {
    const fn new(len: u8, vari: Vari) -> Rule {
        Rule { len, vari }
    }
}

/// Size rule for a Word 97+ opcode.
///
/// The general case is derived from `spra`, exactly as
/// `wwSprmParser::GetSprmInfo` does for ids it does not know. The exceptions
/// below are opcodes whose real operand disagrees with their own `spra` field;
/// they are hand-written rows in LibreOffice's `GetWW8SprmSearcher` table and
/// come from files Word actually wrote. Getting any of them from `spra` would
/// desynchronise the stream.
fn rule_ww8(opcode: u16) -> Rule {
    match opcode {
        // The "default sprm": a padding zero, never carries an operand.
        0x0000 => Rule::new(0, Vari::Fix),
        // sprmCChs — spra says 3 bytes, Word writes 1.
        0xEA08 => Rule::new(1, Vari::Fix),
        // sprmCIdCharType — obsolete, spra says 2 bytes, Word writes none.
        0x480B => Rule::new(0, Vari::Fix),
        // sprmCFFtcAsciSymb — spra says 1 byte, Word writes none.
        0x2A10 => Rule::new(0, Vari::Fix),
        // sprmCDefault — a whole CHP: length-prefixed despite spra == 1.
        0x2A32 => Rule::new(0, Vari::Var),
        // sprmCMajority50 — length-prefixed by spra, but really 2 fixed bytes.
        0xCA4C => Rule::new(2, Vari::Fix),
        // sprmTHTMLProps — spra says 4 bytes, Word writes 1.
        0x740C => Rule::new(1, Vari::Fix),
        // sprmTSetShd80 / sprmTSetShdOdd80 — spra says 4 bytes, really variable.
        0x7627 | 0x7628 => Rule::new(0, Vari::Var),
        // sprmTDiagLine — length-prefixed by spra, but really 1 fixed byte.
        0xD62A => Rule::new(1, Vari::Fix),
        // sprmTDefTable10 / sprmTDefTable — the only two-byte counts there are.
        0xD606 | 0xD608 => Rule::new(0, Vari::Var2),
        _ => rule_from_spra(opcode),
    }
}

/// Fallback for any WW8 opcode: read the size straight out of `spra`.
fn rule_from_spra(opcode: u16) -> Rule {
    match (opcode >> 13) & 0x07 {
        // 0 is a toggle byte, 1 a plain byte.
        0 | 1 => Rule::new(1, Vari::Fix),
        // 2 is a word; 4 and 5 are words too (dxa / dya scaled differently).
        2 | 4 | 5 => Rule::new(2, Vari::Fix),
        3 => Rule::new(4, Vari::Fix),
        7 => Rule::new(3, Vari::Fix),
        // 6 is the only variable-length encoding.
        _ => Rule::new(0, Vari::Var),
    }
}

/// Size rule for a Word 6/95 opcode.
///
/// Transcribed from `wwSprmParser::GetWW6SprmSearcher`. Ids absent from the
/// table are assumed length-prefixed, which is what LibreOffice does too
/// ("All the unknown ww7- sprms appear to be variable").
fn rule_ww6(opcode: u16, cjk: bool) -> Rule {
    // East-Asian Word 7 reuses 111-113 for a two-byte font/language operand.
    if cjk && (111..=113).contains(&opcode) {
        return Rule::new(2, Vari::Fix);
    }
    match opcode {
        // No operand at all: the padding sprm, sprmPRuler, sprmCPlain,
        // sprmSBCustomize.
        0 | 52 | 83 | 163 => Rule::new(0, Vari::Fix),

        // One byte.
        4..=11 | 13 | 14 | 24 | 25 | 29 | 37 | 44 | 50 | 51 | 65..=67 | 71 | 75 | 85..=92 | 94
        | 98 | 100 | 102 | 104 | 117..=119 | 131 | 132 | 138 | 139 | 142 | 143 | 146 | 147
        | 150..=153 | 158 | 159 | 162 | 185 | 186 => Rule::new(1, Vari::Fix),

        // One word. Note 99 (sprmCHps) and 101 (sprmCHpsPos) are words here
        // even though the Word 6 documentation calls them bytes.
        2 | 16..=19 | 21 | 22 | 26..=28 | 30..=36 | 38..=43 | 45..=49 | 69 | 72 | 80 | 93 | 96
        | 97 | 99 | 101 | 107 | 109 | 110 | 121..=124 | 140 | 141 | 144 | 145 | 148 | 149
        | 154..=157 | 160 | 161 | 164..=171 | 182..=184 | 189 | 195 | 197 | 198 => {
            Rule::new(2, Vari::Fix)
        }

        // Three bytes: sprmCChse, sprmCSizePos, the two column-width sprms.
        73 | 95 | 136 | 137 => Rule::new(3, Vari::Fix),

        // Four bytes: sprmPDyaLine, sprmCDttmRMark, sprmTTlp, sprmTInsert,
        // sprmTDxaCol, sprmTSetShd.
        20 | 70 | 192 | 194 | 196 | 200 => Rule::new(4, Vari::Fix),

        // Five bytes: sprmTSetBrc, sprmTSetBrc10.
        193 | 199 => Rule::new(5, Vari::Fix),

        // Twelve bytes: sprmTTableBorders (six BRCs).
        187 => Rule::new(12, Vari::Fix),

        // Length-prefixed with an extra fixed part.
        3 => Rule::new(3, Vari::Var),    // sprmPIstdPermute
        120 => Rule::new(12, Vari::Var), // sprmPicScale
        191 => Rule::new(1, Vari::Var),  // sprmTDefTableShd

        // Two-byte count.
        188 | 190 => Rule::new(0, Vari::Var2), // sprmTDefTable10 / sprmTDefTable

        // Plain length-prefixed, plus every unknown id.
        _ => Rule::new(0, Vari::Var),
    }
}

/// One sprm measured inside a buffer: how many bytes it occupies in total
/// (opcode included) and where its payload starts.
#[derive(Debug, Clone, Copy)]
struct Measured {
    opcode: u16,
    total: usize,
    data_at: usize,
}

/// Measure the sprm starting at the beginning of `buf`.
///
/// Returns `None` when the buffer cannot hold a sprm, when the opcode is
/// invalid, or when the computed size runs past the end. Never panics and
/// `total` is always >= 1, so a caller looping on it always makes progress.
fn measure(buf: &[u8], ver: SprmVersion) -> Option<Measured> {
    let idb = ver.id_bytes();
    if buf.len() < ver.min_len() {
        return None;
    }
    let opcode = if ver.is_ww6() {
        u16::from(*buf.first()?)
    } else {
        u16::from_le_bytes([*buf.first()?, *buf.get(1)?])
    };
    // Zero is the padding sprm; WW8 ids below 0x0800 cannot exist at all
    // (`wwSprmParser::GetSprmId` zeroes them). Either way we are out of sprms.
    if opcode == 0 || (!ver.is_ww6() && opcode < 0x0800) {
        return None;
    }
    let rule = match ver {
        SprmVersion::Ww8 => rule_ww8(opcode),
        SprmVersion::Ww6 => rule_ww6(opcode, false),
        SprmVersion::Ww6Cjk => rule_ww6(opcode, true),
    };

    // sprmPChgTabs stores 255 in its count byte when the real operand is too
    // long to count, and the reader must then walk the deleted/inserted tab
    // arrays to find the end (ww8scan.cxx, GetSprmTailLen, first switch).
    let is_chg_tabs = if ver.is_ww6() { opcode == 23 } else { opcode == 0xC615 };
    if is_chg_tabs {
        let cb = usize::from(*buf.get(idb)?);
        let tail = if cb != 255 {
            cb + usize::from(rule.len)
        } else {
            let n_del = usize::from(buf.get(idb + 1).copied().unwrap_or(0));
            let ins_at = idb.checked_add(2)?.checked_add(n_del.checked_mul(4)?)?;
            let n_ins = usize::from(buf.get(ins_at).copied().unwrap_or(0));
            2 + n_del * 4 + n_ins * 3
        };
        let total = idb.checked_add(1)?.checked_add(tail)?;
        return (total <= buf.len()).then_some(Measured { opcode, total, data_at: idb + 1 });
    }

    let (total, data_at) = match rule.vari {
        Vari::Fix => (idb.checked_add(usize::from(rule.len))?, idb),
        Vari::Var => {
            let cb = usize::from(*buf.get(idb)?);
            (idb.checked_add(1)?.checked_add(cb)?.checked_add(usize::from(rule.len))?, idb + 1)
        }
        Vari::Var2 => {
            // The count is "size of the operand, count field included, minus 1".
            let n = u16::from_le_bytes([*buf.get(idb)?, *buf.get(idb + 1)?]);
            let payload = usize::from(n.saturating_sub(1));
            (idb.checked_add(2)?.checked_add(payload)?.checked_add(usize::from(rule.len))?, idb + 2)
        }
    };
    if total > buf.len() || total == 0 {
        return None;
    }
    Some(Measured { opcode, total, data_at })
}

// ---------------------------------------------------------------------------
// Iteration
// ---------------------------------------------------------------------------

/// Walks a `grpprl` — a packed list of sprms — yielding each one.
///
/// Stops silently at the first thing it cannot make sense of (padding zero,
/// invalid opcode, operand running past the end). [`SprmIter::consumed`] then
/// says how far it got, which is what the tests assert on: on a well-formed
/// grpprl the walk must land exactly on the end of the buffer.
pub(crate) struct SprmIter<'a> {
    buf: &'a [u8],
    pos: usize,
    ver: SprmVersion,
    done: bool,
}

impl<'a> SprmIter<'a> {
    pub(crate) fn new(grpprl: &'a [u8], ver: SprmVersion) -> SprmIter<'a> {
        SprmIter { buf: grpprl, pos: 0, ver, done: false }
    }

    /// Bytes consumed so far.
    pub(crate) fn consumed(&self) -> usize {
        self.pos
    }
}

impl<'a> Iterator for SprmIter<'a> {
    type Item = Sprm<'a>;

    fn next(&mut self) -> Option<Sprm<'a>> {
        if self.done {
            return None;
        }
        let rest = self.buf.get(self.pos..)?;
        let Some(m) = measure(rest, self.ver) else {
            self.done = true;
            return None;
        };
        let operand = rest.get(m.data_at..m.total).unwrap_or(&[]);
        // `measure` guarantees total >= 1, so this always advances.
        self.pos += m.total;
        Some(Sprm { opcode: m.opcode, operand })
    }
}

/// Walk a Word 97+ `grpprl` and collect its sprms.
///
/// Skipping an unknown sprm correctly is the whole point of the size rules
/// above: one mis-sized operand shifts every following property.
pub(crate) fn iter_sprms(grpprl: &[u8]) -> Vec<Sprm<'_>> {
    SprmIter::new(grpprl, SprmVersion::Ww8).collect()
}

/// Same, for a document whose encoding is not necessarily WW8.
pub(crate) fn iter_sprms_ver(grpprl: &[u8], ver: SprmVersion) -> Vec<Sprm<'_>> {
    SprmIter::new(grpprl, ver).collect()
}

/// Find the last sprm with a given opcode, the way Word resolves duplicates
/// inside one grpprl (later wins).
pub(crate) fn find_sprm(grpprl: &[u8], ver: SprmVersion, opcode: u16) -> Option<Sprm<'_>> {
    SprmIter::new(grpprl, ver).fold(None, |acc, s| if s.opcode == opcode { Some(s) } else { acc })
}

// ---------------------------------------------------------------------------
// Opcode tables
// ---------------------------------------------------------------------------

/// Word 97+ sprm opcodes, named as in [MS-DOC] 2.6.
///
/// Each constant is `ispmd + (fSpec << 9) + (sgc << 10) + (spra << 13)`; the
/// comment gives the `ispmd` so an opcode can be matched against the spec
/// tables without decoding it by hand.
#[allow(dead_code)]
pub(crate) mod ww8 {
    // -- character properties (sgc = 2) -----------------------------------
    pub(crate) const CFRMARKDEL: u16 = 0x0800; // ispmd 0x00
    pub(crate) const CFRMARKINS: u16 = 0x0801; // ispmd 0x01
    pub(crate) const CFFLDVANISH: u16 = 0x0802; // ispmd 0x02
    pub(crate) const CPICLOCATION: u16 = 0x6A03; // ispmd 0x03, picture/OLE offset
    pub(crate) const CIBSTRMARK: u16 = 0x4804; // ispmd 0x04
    pub(crate) const CDTTMRMARK: u16 = 0x6805; // ispmd 0x05
    pub(crate) const CFDATA: u16 = 0x0806; // ispmd 0x06
    pub(crate) const CSYMBOL: u16 = 0x6A09; // ispmd 0x09, symbol font + char
    pub(crate) const CFOLE2: u16 = 0x080A; // ispmd 0x0A
    pub(crate) const CHIGHLIGHT: u16 = 0x2A0C; // ispmd 0x0C, highlight ico
    pub(crate) const CFWEBHIDDEN: u16 = 0x0811; // ispmd 0x11
    pub(crate) const CRSIDPROP: u16 = 0x6815; // ispmd 0x15
    pub(crate) const CRSIDTEXT: u16 = 0x6816; // ispmd 0x16
    pub(crate) const CISTD: u16 = 0x4A30; // ispmd 0x30, character style index
    pub(crate) const CISTDPERMUTE: u16 = 0xCA31; // ispmd 0x31
    pub(crate) const CPLAIN: u16 = 0x2A33; // ispmd 0x33, reset to defaults
    pub(crate) const CKCD: u16 = 0x2A34; // ispmd 0x34, emphasis mark
    pub(crate) const CFBOLD: u16 = 0x0835; // ispmd 0x35
    pub(crate) const CFITALIC: u16 = 0x0836; // ispmd 0x36
    pub(crate) const CFSTRIKE: u16 = 0x0837; // ispmd 0x37
    pub(crate) const CFOUTLINE: u16 = 0x0838; // ispmd 0x38
    pub(crate) const CFSHADOW: u16 = 0x0839; // ispmd 0x39
    pub(crate) const CFSMALLCAPS: u16 = 0x083A; // ispmd 0x3A
    pub(crate) const CFCAPS: u16 = 0x083B; // ispmd 0x3B
    pub(crate) const CFVANISH: u16 = 0x083C; // ispmd 0x3C, hidden text
    pub(crate) const CKUL: u16 = 0x2A3E; // ispmd 0x3E, underline kind
    pub(crate) const CDXASPACE: u16 = 0x8840; // ispmd 0x40, letter spacing (twips)
    pub(crate) const CICO: u16 = 0x2A42; // ispmd 0x42, colour index (palette)
    pub(crate) const CHPS: u16 = 0x4A43; // ispmd 0x43, size in half-points
    pub(crate) const CHPSPOS: u16 = 0x4845; // ispmd 0x45, super/subscript offset
    pub(crate) const CMAJORITY: u16 = 0xCA47; // ispmd 0x47
    pub(crate) const CISS: u16 = 0x2A48; // ispmd 0x48, 0 none 1 super 2 sub
    pub(crate) const CHPSKERN: u16 = 0x484B; // ispmd 0x4B
    pub(crate) const CHRESI: u16 = 0x484E; // ispmd 0x4E
    pub(crate) const CRGFTC0: u16 = 0x4A4F; // ispmd 0x4F, ASCII font index
    pub(crate) const CRGFTC1: u16 = 0x4A50; // ispmd 0x50, East-Asian font index
    pub(crate) const CRGFTC2: u16 = 0x4A51; // ispmd 0x51, other font index
    pub(crate) const CCHARSCALE: u16 = 0x4852; // ispmd 0x52, horizontal scale %
    pub(crate) const CFDSTRIKE: u16 = 0x2A53; // ispmd 0x53, double strikethrough
    pub(crate) const CFIMPRINT: u16 = 0x0854; // ispmd 0x54
    pub(crate) const CFSPEC: u16 = 0x0855; // ispmd 0x55, special character
    pub(crate) const CFOBJ: u16 = 0x0856; // ispmd 0x56
    pub(crate) const CPROPRMARK90: u16 = 0xCA57; // ispmd 0x57
    pub(crate) const CFEMBOSS: u16 = 0x0858; // ispmd 0x58
    pub(crate) const CSFXTEXT: u16 = 0x2859; // ispmd 0x59, text animation
    pub(crate) const CFBIDI: u16 = 0x085A; // ispmd 0x5A
    pub(crate) const CFBOLDBI: u16 = 0x085C; // ispmd 0x5C
    pub(crate) const CFITALICBI: u16 = 0x085D; // ispmd 0x5D
    pub(crate) const CFTCBI: u16 = 0x4A5E; // ispmd 0x5E, complex-script font
    pub(crate) const CLIDBI: u16 = 0x485F; // ispmd 0x5F
    pub(crate) const CICOBI: u16 = 0x4A60; // ispmd 0x60
    pub(crate) const CHPSBI: u16 = 0x4A61; // ispmd 0x61, complex-script size
    pub(crate) const CIBSTRMARKDEL: u16 = 0x4863; // ispmd 0x63
    pub(crate) const CDTTMRMARKDEL: u16 = 0x6864; // ispmd 0x64
    pub(crate) const CBRC80: u16 = 0x6865; // ispmd 0x65, character border
    pub(crate) const CSHD80: u16 = 0x4866; // ispmd 0x66, character shading
    pub(crate) const CRGLID0_80: u16 = 0x486D; // ispmd 0x6D, language (western)
    pub(crate) const CRGLID1_80: u16 = 0x486E; // ispmd 0x6E, language (East Asian)
    pub(crate) const CIDCTHINT: u16 = 0x286F; // ispmd 0x6F
    pub(crate) const CCV: u16 = 0x6870; // ispmd 0x70, RGB text colour
    pub(crate) const CSHD: u16 = 0xCA71; // ispmd 0x71, character shading (new)
    pub(crate) const CBRC: u16 = 0xCA72; // ispmd 0x72, character border (new)
    pub(crate) const CRGLID0: u16 = 0x4873; // ispmd 0x73
    pub(crate) const CRGLID1: u16 = 0x4874; // ispmd 0x74
    pub(crate) const CFNOPROOF: u16 = 0x0875; // ispmd 0x75
    pub(crate) const CFITTEXT: u16 = 0xCA76; // ispmd 0x76
    pub(crate) const CCVUL: u16 = 0x6877; // ispmd 0x77, underline colour
    pub(crate) const CFELAYOUT: u16 = 0xCA78; // ispmd 0x78
    pub(crate) const CFCOMPLEXSCRIPTS: u16 = 0x0882; // ispmd 0x82
    pub(crate) const CCNF: u16 = 0xCA85; // ispmd 0x85

    // -- paragraph properties (sgc = 1) -----------------------------------
    pub(crate) const PISTD: u16 = 0x4600; // ispmd 0x00, paragraph style index
    pub(crate) const PISTDPERMUTE: u16 = 0xC601; // ispmd 0x01
    pub(crate) const PINCLVL: u16 = 0x2602; // ispmd 0x02
    pub(crate) const PJC80: u16 = 0x2403; // ispmd 0x03, alignment (legacy)
    pub(crate) const PFKEEP: u16 = 0x2405; // ispmd 0x05, keep lines together
    pub(crate) const PFKEEPFOLLOW: u16 = 0x2406; // ispmd 0x06, keep with next
    pub(crate) const PFPAGEBREAKBEFORE: u16 = 0x2407; // ispmd 0x07
    pub(crate) const PILVL: u16 = 0x260A; // ispmd 0x0A, list level
    pub(crate) const PILFO: u16 = 0x460B; // ispmd 0x0B, list (LFO) index
    pub(crate) const PFNOLINENUMB: u16 = 0x240C; // ispmd 0x0C
    pub(crate) const PCHGTABSPAPX: u16 = 0xC60D; // ispmd 0x0D, tab stops
    pub(crate) const PDXARIGHT80: u16 = 0x840E; // ispmd 0x0E, right indent
    pub(crate) const PDXALEFT80: u16 = 0x840F; // ispmd 0x0F, left indent
    pub(crate) const PNEST80: u16 = 0x4610; // ispmd 0x10
    pub(crate) const PDXALEFT180: u16 = 0x8411; // ispmd 0x11, first-line indent
    pub(crate) const PDYALINE: u16 = 0x6412; // ispmd 0x12, LSPD line spacing
    pub(crate) const PDYABEFORE: u16 = 0xA413; // ispmd 0x13, space before
    pub(crate) const PDYAAFTER: u16 = 0xA414; // ispmd 0x14, space after
    pub(crate) const PCHGTABS: u16 = 0xC615; // ispmd 0x15, tab stops (complex)
    pub(crate) const PFINTABLE: u16 = 0x2416; // ispmd 0x16
    pub(crate) const PFTTP: u16 = 0x2417; // ispmd 0x17, this para ends a row
    pub(crate) const PDXAABS: u16 = 0x8418; // ispmd 0x18
    pub(crate) const PDYAABS: u16 = 0x8419; // ispmd 0x19
    pub(crate) const PDXAWIDTH: u16 = 0x841A; // ispmd 0x1A
    pub(crate) const PPC: u16 = 0x261B; // ispmd 0x1B, frame anchoring
    pub(crate) const PWR: u16 = 0x2423; // ispmd 0x23, text wrapping
    pub(crate) const PBRCTOP80: u16 = 0x6424; // ispmd 0x24
    pub(crate) const PBRCLEFT80: u16 = 0x6425; // ispmd 0x25
    pub(crate) const PBRCBOTTOM80: u16 = 0x6426; // ispmd 0x26
    pub(crate) const PBRCRIGHT80: u16 = 0x6427; // ispmd 0x27
    pub(crate) const PBRCBETWEEN80: u16 = 0x6428; // ispmd 0x28
    pub(crate) const PBRCBAR80: u16 = 0x6629; // ispmd 0x29
    pub(crate) const PFNOAUTOHYPH: u16 = 0x242A; // ispmd 0x2A
    pub(crate) const PWHEIGHTABS: u16 = 0x442B; // ispmd 0x2B
    pub(crate) const PDCS: u16 = 0x442C; // ispmd 0x2C, drop cap
    pub(crate) const PSHD80: u16 = 0x442D; // ispmd 0x2D, paragraph shading
    pub(crate) const PDYAFROMTEXT: u16 = 0x842E; // ispmd 0x2E
    pub(crate) const PDXAFROMTEXT: u16 = 0x842F; // ispmd 0x2F
    pub(crate) const PFLOCKED: u16 = 0x2430; // ispmd 0x30
    pub(crate) const PFWIDOWCONTROL: u16 = 0x2431; // ispmd 0x31
    pub(crate) const PFKINSOKU: u16 = 0x2433; // ispmd 0x33
    pub(crate) const PFWORDWRAP: u16 = 0x2434; // ispmd 0x34
    pub(crate) const PFOVERFLOWPUNCT: u16 = 0x2435; // ispmd 0x35
    pub(crate) const PFTOPLINEPUNCT: u16 = 0x2436; // ispmd 0x36
    pub(crate) const PFAUTOSPACEDE: u16 = 0x2437; // ispmd 0x37
    pub(crate) const PFAUTOSPACEDN: u16 = 0x2438; // ispmd 0x38
    pub(crate) const PWALIGNFONT: u16 = 0x4439; // ispmd 0x39
    pub(crate) const PFRAMETEXTFLOW: u16 = 0x443A; // ispmd 0x3A
    pub(crate) const POUTLVL: u16 = 0x2640; // ispmd 0x40, outline level
    pub(crate) const PFBIDI: u16 = 0x2441; // ispmd 0x41, right-to-left paragraph
    pub(crate) const PFNUMRMINS: u16 = 0x2443; // ispmd 0x43
    pub(crate) const PNUMRM: u16 = 0xC645; // ispmd 0x45
    pub(crate) const PHUGEPAPX: u16 = 0x6646; // ispmd 0x46, grpprl in Data stream
    pub(crate) const PFUSEPGSUSETTINGS: u16 = 0x2447; // ispmd 0x47
    pub(crate) const PFADJUSTRIGHT: u16 = 0x2448; // ispmd 0x48
    pub(crate) const PITAP: u16 = 0x6649; // ispmd 0x49, table nesting depth
    pub(crate) const PDTAP: u16 = 0x664A; // ispmd 0x4A
    pub(crate) const PFINNERTABLECELL: u16 = 0x244B; // ispmd 0x4B
    pub(crate) const PFINNERTTP: u16 = 0x244C; // ispmd 0x4C
    pub(crate) const PSHD: u16 = 0xC64D; // ispmd 0x4D, paragraph shading (new)
    pub(crate) const PBRCTOP: u16 = 0xC64E; // ispmd 0x4E
    pub(crate) const PBRCLEFT: u16 = 0xC64F; // ispmd 0x4F
    pub(crate) const PBRCBOTTOM: u16 = 0xC650; // ispmd 0x50
    pub(crate) const PBRCRIGHT: u16 = 0xC651; // ispmd 0x51
    pub(crate) const PBRCBETWEEN: u16 = 0xC652; // ispmd 0x52
    pub(crate) const PBRCBAR: u16 = 0xC653; // ispmd 0x53
    pub(crate) const PFDYABEFOREAUTO: u16 = 0x245B; // ispmd 0x5B
    pub(crate) const PFDYAAFTERAUTO: u16 = 0x245C; // ispmd 0x5C
    pub(crate) const PDXARIGHT: u16 = 0x845D; // ispmd 0x5D, right indent
    pub(crate) const PDXALEFT: u16 = 0x845E; // ispmd 0x5E, left indent
    pub(crate) const PNEST: u16 = 0x465F; // ispmd 0x5F
    pub(crate) const PDXALEFT1: u16 = 0x8460; // ispmd 0x60, first-line indent
    pub(crate) const PJC: u16 = 0x2461; // ispmd 0x61, alignment
    pub(crate) const PWALL: u16 = 0x2664; // ispmd 0x64
    pub(crate) const PCNF: u16 = 0xC666; // ispmd 0x66
    pub(crate) const PRSID: u16 = 0x6467; // ispmd 0x67
    pub(crate) const PTABLEPROPS: u16 = 0x646B; // ispmd 0x6B
    pub(crate) const PFCONTEXTUALSPACING: u16 = 0x246D; // ispmd 0x6D
    pub(crate) const PPROPRMARK: u16 = 0xC66F; // ispmd 0x6F
    pub(crate) const PFMIRRORINDENTS: u16 = 0x2470; // ispmd 0x70

    // -- table properties (sgc = 5) ---------------------------------------
    pub(crate) const TJC90: u16 = 0x5400; // ispmd 0x00, row alignment (legacy)
    pub(crate) const TJC: u16 = 0x548A; // ispmd 0x8A, row alignment
    pub(crate) const TDXALEFT: u16 = 0x9601; // ispmd 0x01, row left offset
    pub(crate) const TDXAGAPHALF: u16 = 0x9602; // ispmd 0x02, half cell margin
    pub(crate) const TFCANTSPLIT90: u16 = 0x3403; // ispmd 0x03
    pub(crate) const TTABLEHEADER: u16 = 0x3404; // ispmd 0x04, repeat as header
    pub(crate) const TTABLEBORDERS80: u16 = 0xD605; // ispmd 0x05
    pub(crate) const TDEFTABLE10: u16 = 0xD606; // ispmd 0x06, two-byte count
    pub(crate) const TDYAROWHEIGHT: u16 = 0x9407; // ispmd 0x07
    pub(crate) const TDEFTABLE: u16 = 0xD608; // ispmd 0x08, two-byte count
    pub(crate) const TDEFTABLESHD80: u16 = 0xD609; // ispmd 0x09
    pub(crate) const TTLP: u16 = 0x740A; // ispmd 0x0A, table look
    pub(crate) const TFBIDI: u16 = 0x560B; // ispmd 0x0B
    pub(crate) const TPC: u16 = 0x360D; // ispmd 0x0D
    pub(crate) const TDXAABS: u16 = 0x940E; // ispmd 0x0E
    pub(crate) const TDYAABS: u16 = 0x940F; // ispmd 0x0F
    pub(crate) const TDXAFROMTEXT: u16 = 0x9410; // ispmd 0x10
    pub(crate) const TDYAFROMTEXT: u16 = 0x9411; // ispmd 0x11
    pub(crate) const TDEFTABLESHD: u16 = 0xD612; // ispmd 0x12, per-cell shading
    pub(crate) const TTABLEBORDERS: u16 = 0xD613; // ispmd 0x13
    pub(crate) const TTABLEWIDTH: u16 = 0xF614; // ispmd 0x14
    pub(crate) const TFAUTOFIT: u16 = 0x3615; // ispmd 0x15
    pub(crate) const TWIDTHBEFORE: u16 = 0xF617; // ispmd 0x17
    pub(crate) const TWIDTHAFTER: u16 = 0xF618; // ispmd 0x18
    pub(crate) const TBRCTOPCV: u16 = 0xD61A; // ispmd 0x1A, top border colour
    pub(crate) const TBRCLEFTCV: u16 = 0xD61B; // ispmd 0x1B
    pub(crate) const TBRCBOTTOMCV: u16 = 0xD61C; // ispmd 0x1C
    pub(crate) const TBRCRIGHTCV: u16 = 0xD61D; // ispmd 0x1D
    pub(crate) const TSETBRC80: u16 = 0xD620; // ispmd 0x20
    pub(crate) const TINSERT: u16 = 0x7621; // ispmd 0x21
    pub(crate) const TDELETE: u16 = 0x5622; // ispmd 0x22
    pub(crate) const TDXACOL: u16 = 0x7623; // ispmd 0x23
    pub(crate) const TMERGE: u16 = 0x5624; // ispmd 0x24, horizontal merge
    pub(crate) const TSPLIT: u16 = 0x5625; // ispmd 0x25
    pub(crate) const TTEXTFLOW: u16 = 0x7629; // ispmd 0x29, cell text direction
    pub(crate) const TVERTMERGE: u16 = 0xD62B; // ispmd 0x2B, vertical merge
    pub(crate) const TVERTALIGN: u16 = 0xD62C; // ispmd 0x2C, vertical alignment
    pub(crate) const TSETSHD: u16 = 0xD62D; // ispmd 0x2D
    pub(crate) const TSETSHDODD: u16 = 0xD62E; // ispmd 0x2E
    pub(crate) const TSETBRC: u16 = 0xD62F; // ispmd 0x2F, per-cell borders
    pub(crate) const TCELLPADDING: u16 = 0xD632; // ispmd 0x32
    pub(crate) const TCELLPADDINGDEFAULT: u16 = 0xD634; // ispmd 0x34
    pub(crate) const TCELLWIDTH: u16 = 0xD635; // ispmd 0x35
    pub(crate) const TFITTEXT: u16 = 0xF636; // ispmd 0x36
    pub(crate) const TISTD: u16 = 0x563A; // ispmd 0x3A, table style index
    pub(crate) const TSETSHDTABLE: u16 = 0xD660; // ispmd 0x60
    pub(crate) const TWIDTHINDENT: u16 = 0xF661; // ispmd 0x61
    pub(crate) const TFCANTSPLIT: u16 = 0x3466; // ispmd 0x66
    pub(crate) const TCNF: u16 = 0xD66A; // ispmd 0x6A
    pub(crate) const TDEFTABLESHDRAW: u16 = 0xD670; // ispmd 0x70
    pub(crate) const TRSID: u16 = 0x7479; // ispmd 0x79

    // -- section properties (sgc = 4) -------------------------------------
    pub(crate) const SCNSPGN: u16 = 0x3000; // ispmd 0x00
    pub(crate) const SIHEADINGPGN: u16 = 0x3001; // ispmd 0x01
    pub(crate) const SDXACOLWIDTH: u16 = 0xF203; // ispmd 0x03, column width
    pub(crate) const SDXACOLSPACING: u16 = 0xF204; // ispmd 0x04, column gap
    pub(crate) const SFEVENLYSPACED: u16 = 0x3005; // ispmd 0x05
    pub(crate) const SFPROTECTED: u16 = 0x3006; // ispmd 0x06
    pub(crate) const SDMBINFIRST: u16 = 0x5007; // ispmd 0x07
    pub(crate) const SDMBINOTHER: u16 = 0x5008; // ispmd 0x08
    pub(crate) const SBKC: u16 = 0x3009; // ispmd 0x09, section break kind
    pub(crate) const SFTITLEPAGE: u16 = 0x300A; // ispmd 0x0A, different first page
    pub(crate) const SCCOLUMNS: u16 = 0x500B; // ispmd 0x0B, column count minus 1
    pub(crate) const SDXACOLUMNS: u16 = 0x900C; // ispmd 0x0C, default column gap
    pub(crate) const SNFCPGN: u16 = 0x300E; // ispmd 0x0E
    pub(crate) const SFPGNRESTART: u16 = 0x3011; // ispmd 0x11
    pub(crate) const SFENDNOTE: u16 = 0x3012; // ispmd 0x12
    pub(crate) const SLNC: u16 = 0x3013; // ispmd 0x13
    pub(crate) const SNLNNMOD: u16 = 0x5015; // ispmd 0x15
    pub(crate) const SDXALNN: u16 = 0x9016; // ispmd 0x16
    pub(crate) const SDYAHDRTOP: u16 = 0xB017; // ispmd 0x17, header distance
    pub(crate) const SDYAHDRBOTTOM: u16 = 0xB018; // ispmd 0x18, footer distance
    pub(crate) const SLBETWEEN: u16 = 0x3019; // ispmd 0x19, line between columns
    pub(crate) const SVJC: u16 = 0x301A; // ispmd 0x1A, vertical alignment
    pub(crate) const SLNNMIN: u16 = 0x501B; // ispmd 0x1B
    pub(crate) const SPGNSTART97: u16 = 0x501C; // ispmd 0x1C, first page number
    pub(crate) const SBORIENTATION: u16 = 0x301D; // ispmd 0x1D, 1 portrait 2 landscape
    pub(crate) const SXAPAGE: u16 = 0xB01F; // ispmd 0x1F, page width
    pub(crate) const SYAPAGE: u16 = 0xB020; // ispmd 0x20, page height
    pub(crate) const SDXALEFT: u16 = 0xB021; // ispmd 0x21, left margin
    pub(crate) const SDXARIGHT: u16 = 0xB022; // ispmd 0x22, right margin
    pub(crate) const SDYATOP: u16 = 0x9023; // ispmd 0x23, top margin
    pub(crate) const SDYABOTTOM: u16 = 0x9024; // ispmd 0x24, bottom margin
    pub(crate) const SDZAGUTTER: u16 = 0xB025; // ispmd 0x25, gutter
    pub(crate) const SDMPAPERREQ: u16 = 0x5026; // ispmd 0x26
    pub(crate) const SFBIDI: u16 = 0x3228; // ispmd 0x28
    pub(crate) const SFRTLGUTTER: u16 = 0x322A; // ispmd 0x2A
    pub(crate) const SBRCTOP80: u16 = 0x702B; // ispmd 0x2B, page border
    pub(crate) const SBRCLEFT80: u16 = 0x702C; // ispmd 0x2C
    pub(crate) const SBRCBOTTOM80: u16 = 0x702D; // ispmd 0x2D
    pub(crate) const SBRCRIGHT80: u16 = 0x702E; // ispmd 0x2E
    pub(crate) const SPGBPROP: u16 = 0x522F; // ispmd 0x2F, page border options
    pub(crate) const SDXTCHARSPACE: u16 = 0x7030; // ispmd 0x30
    pub(crate) const SDYALINEPITCH: u16 = 0x9031; // ispmd 0x31
    pub(crate) const SCLM: u16 = 0x5032; // ispmd 0x32
    pub(crate) const STEXTFLOW: u16 = 0x5033; // ispmd 0x33, text direction
    pub(crate) const SBRCTOP: u16 = 0xD234; // ispmd 0x34, page border (new)
    pub(crate) const SBRCLEFT: u16 = 0xD235; // ispmd 0x35
    pub(crate) const SBRCBOTTOM: u16 = 0xD236; // ispmd 0x36
    pub(crate) const SBRCRIGHT: u16 = 0xD237; // ispmd 0x37
    pub(crate) const SRSID: u16 = 0x703A; // ispmd 0x3A
    pub(crate) const SFPC: u16 = 0x303B; // ispmd 0x3B
    pub(crate) const SRNCFTN: u16 = 0x303C; // ispmd 0x3C
    pub(crate) const SPGNSTART: u16 = 0x7044; // ispmd 0x44, first page number

    // -- picture properties (sgc = 3) -------------------------------------
    pub(crate) const PICBRCTOP80: u16 = 0x6C02; // ispmd 0x02
    pub(crate) const PICBRCLEFT80: u16 = 0x6C03; // ispmd 0x03
    pub(crate) const PICBRCBOTTOM80: u16 = 0x6C04; // ispmd 0x04
    pub(crate) const PICBRCRIGHT80: u16 = 0x6C05; // ispmd 0x05
    pub(crate) const PICSCALE: u16 = 0xCE01; // ispmd 0x01, crop and scale
}

/// Word 6/95 sprm opcodes, from the "Word 6.0 Binary File Format" tables
/// (LibreOffice `NS_sprm::v6`). Same properties, entirely different numbers.
#[allow(dead_code)]
pub(crate) mod ww6 {
    // -- paragraph --------------------------------------------------------
    pub(crate) const PISTD: u8 = 2;
    pub(crate) const PISTDPERMUTE: u8 = 3;
    pub(crate) const PINCLV1: u8 = 4;
    pub(crate) const PJC: u8 = 5;
    pub(crate) const PFKEEP: u8 = 7;
    pub(crate) const PFKEEPFOLLOW: u8 = 8;
    pub(crate) const PPAGEBREAKBEFORE: u8 = 9;
    pub(crate) const PANLD: u8 = 12;
    pub(crate) const PNLVLANM: u8 = 13;
    pub(crate) const PFNOLINENUMB: u8 = 14;
    pub(crate) const PCHGTABSPAPX: u8 = 15;
    pub(crate) const PDXARIGHT: u8 = 16;
    pub(crate) const PDXALEFT: u8 = 17;
    pub(crate) const PNEST: u8 = 18;
    pub(crate) const PDXALEFT1: u8 = 19;
    pub(crate) const PDYALINE: u8 = 20;
    pub(crate) const PDYABEFORE: u8 = 21;
    pub(crate) const PDYAAFTER: u8 = 22;
    pub(crate) const PCHGTABS: u8 = 23;
    pub(crate) const PFINTABLE: u8 = 24;
    pub(crate) const PTTP: u8 = 25;
    pub(crate) const PBRCTOP: u8 = 38;
    pub(crate) const PBRCLEFT: u8 = 39;
    pub(crate) const PBRCBOTTOM: u8 = 40;
    pub(crate) const PBRCRIGHT: u8 = 41;
    pub(crate) const PBRCBETWEEN: u8 = 42;
    pub(crate) const PSHD: u8 = 47;
    pub(crate) const PFWIDOWCONTROL: u8 = 51;

    // -- character --------------------------------------------------------
    pub(crate) const CPICLOCATION: u8 = 68;
    pub(crate) const CSYMBOL: u8 = 74;
    pub(crate) const CISTD: u8 = 80;
    pub(crate) const CPLAIN: u8 = 83;
    pub(crate) const CFBOLD: u8 = 85;
    pub(crate) const CFITALIC: u8 = 86;
    pub(crate) const CFSTRIKE: u8 = 87;
    pub(crate) const CFOUTLINE: u8 = 88;
    pub(crate) const CFSHADOW: u8 = 89;
    pub(crate) const CFSMALLCAPS: u8 = 90;
    pub(crate) const CFCAPS: u8 = 91;
    pub(crate) const CFVANISH: u8 = 92;
    pub(crate) const CFTC: u8 = 93; // font index
    pub(crate) const CKUL: u8 = 94; // underline kind
    pub(crate) const CDXASPACE: u8 = 96;
    pub(crate) const CLID: u8 = 97;
    pub(crate) const CICO: u8 = 98; // colour index
    pub(crate) const CHPS: u8 = 99; // size in half-points
    pub(crate) const CHPSPOS: u8 = 101;
    pub(crate) const CISS: u8 = 104;
    pub(crate) const CHPSKERN: u8 = 107;
    pub(crate) const CFSPEC: u8 = 117;
    pub(crate) const CFOBJ: u8 = 118;

    // -- section ----------------------------------------------------------
    pub(crate) const SDXACOLWIDTH: u8 = 136;
    pub(crate) const SDXACOLSPACING: u8 = 137;
    pub(crate) const SBKC: u8 = 142;
    pub(crate) const SFTITLEPAGE: u8 = 143;
    pub(crate) const SCCOLUMNS: u8 = 144;
    pub(crate) const SDXACOLUMNS: u8 = 145;
    pub(crate) const SDYAHDRTOP: u8 = 156;
    pub(crate) const SDYAHDRBOTTOM: u8 = 157;
    pub(crate) const SLBETWEEN: u8 = 158;
    pub(crate) const SVJC: u8 = 159;
    pub(crate) const SPGNSTART: u8 = 161;
    pub(crate) const SBORIENTATION: u8 = 162;
    pub(crate) const SXAPAGE: u8 = 164;
    pub(crate) const SYAPAGE: u8 = 165;
    pub(crate) const SDXALEFT: u8 = 166;
    pub(crate) const SDXARIGHT: u8 = 167;
    pub(crate) const SDYATOP: u8 = 168;
    pub(crate) const SDYABOTTOM: u8 = 169;
    pub(crate) const SDZAGUTTER: u8 = 170;

    // -- table ------------------------------------------------------------
    pub(crate) const TJC: u8 = 182;
    pub(crate) const TDXALEFT: u8 = 183;
    pub(crate) const TDXAGAPHALF: u8 = 184;
    pub(crate) const TFCANTSPLIT: u8 = 185;
    pub(crate) const TTABLEHEADER: u8 = 186;
    pub(crate) const TTABLEBORDERS: u8 = 187;
    pub(crate) const TDEFTABLE10: u8 = 188;
    pub(crate) const TDYAROWHEIGHT: u8 = 189;
    pub(crate) const TDEFTABLE: u8 = 190;
    pub(crate) const TDEFTABLESHD: u8 = 191;
    pub(crate) const TSETBRC: u8 = 193;
    pub(crate) const TMERGE: u8 = 197;
    pub(crate) const TSETSHD: u8 = 200;
}

// ---------------------------------------------------------------------------
// Semantic mapping
// ---------------------------------------------------------------------------

/// What a sprm *means*, independent of the encoding it came from.
///
/// Lets a consumer write `match sprm.kind(ver) { SprmKind::CharBold => ... }`
/// once instead of matching two id spaces, and keeps raw hex literals out of
/// `chp.rs`, `pap.rs`, `table.rs` and `section.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum SprmKind {
    // Character
    CharStyle,
    CharPlain,
    CharBold,
    CharItalic,
    CharStrike,
    CharDoubleStrike,
    CharOutline,
    CharShadow,
    CharEmboss,
    CharImprint,
    CharSmallCaps,
    CharCaps,
    CharHidden,
    CharUnderline,
    CharUnderlineColor,
    CharSize,
    CharSizeComplex,
    CharFontAscii,
    CharFontFarEast,
    CharFontOther,
    CharFontComplex,
    CharColorIco,
    CharColorRgb,
    CharHighlight,
    CharShading,
    CharBorder,
    CharSubSuper,
    CharPosition,
    CharSpacing,
    CharScale,
    CharKerning,
    CharLanguage,
    CharLanguageFarEast,
    CharBidi,
    CharSpecial,
    CharPicLocation,
    CharSymbol,

    // Paragraph
    ParaStyle,
    ParaJustify,
    ParaIndentLeft,
    ParaIndentRight,
    ParaIndentFirstLine,
    ParaSpaceBefore,
    ParaSpaceAfter,
    ParaSpaceBeforeAuto,
    ParaSpaceAfterAuto,
    ParaLineSpacing,
    ParaContextualSpacing,
    ParaKeepLines,
    ParaKeepNext,
    ParaPageBreakBefore,
    ParaWidowControl,
    ParaOutlineLevel,
    ParaListLevel,
    ParaListIndex,
    ParaTabs,
    ParaShading,
    ParaBorderTop,
    ParaBorderLeft,
    ParaBorderBottom,
    ParaBorderRight,
    ParaBorderBetween,
    ParaBidi,
    ParaInTable,
    ParaTableRowEnd,
    ParaTableDepth,
    ParaInnerTableCell,
    ParaInnerTableRowEnd,
    ParaHugePapx,

    // Table
    TableJustify,
    TableRowLeft,
    TableCellMarginHalf,
    TableRowHeight,
    TableDefinition,
    TableCellShading,
    TableBorders,
    TableCellBorders,
    TableCellBorderColor,
    TableWidth,
    TableWidthBefore,
    TableWidthAfter,
    TableIndent,
    TableCantSplit,
    TableHeaderRow,
    TableAutofit,
    TableCellMerge,
    TableCellVertMerge,
    TableCellVertAlign,
    TableCellPadding,
    TableCellWidth,
    TableCellTextFlow,
    TableStyle,
    TableLook,

    // Section
    SecBreakKind,
    SecPageWidth,
    SecPageHeight,
    SecMarginLeft,
    SecMarginRight,
    SecMarginTop,
    SecMarginBottom,
    SecGutter,
    SecHeaderDistance,
    SecFooterDistance,
    SecOrientation,
    SecColumnCount,
    SecColumnSpacing,
    SecColumnWidth,
    SecColumnLine,
    SecTitlePage,
    SecPageNumberStart,
    SecVertAlign,
    SecTextFlow,
    SecPageBorderTop,
    SecPageBorderLeft,
    SecPageBorderBottom,
    SecPageBorderRight,

    /// Recognised structurally (so it is skipped correctly) but with no
    /// meaning attached here.
    Other,
}

/// Map an opcode to its meaning.
fn kind_of(opcode: u16, ver: SprmVersion) -> SprmKind {
    if ver.is_ww6() {
        return kind_ww6(opcode);
    }
    kind_ww8(opcode)
}

fn kind_ww8(opcode: u16) -> SprmKind {
    use SprmKind as K;
    match opcode {
        ww8::CISTD => K::CharStyle,
        ww8::CPLAIN => K::CharPlain,
        ww8::CFBOLD | ww8::CFBOLDBI => K::CharBold,
        ww8::CFITALIC | ww8::CFITALICBI => K::CharItalic,
        ww8::CFSTRIKE => K::CharStrike,
        ww8::CFDSTRIKE => K::CharDoubleStrike,
        ww8::CFOUTLINE => K::CharOutline,
        ww8::CFSHADOW => K::CharShadow,
        ww8::CFEMBOSS => K::CharEmboss,
        ww8::CFIMPRINT => K::CharImprint,
        ww8::CFSMALLCAPS => K::CharSmallCaps,
        ww8::CFCAPS => K::CharCaps,
        ww8::CFVANISH => K::CharHidden,
        ww8::CKUL => K::CharUnderline,
        ww8::CCVUL => K::CharUnderlineColor,
        ww8::CHPS => K::CharSize,
        ww8::CHPSBI => K::CharSizeComplex,
        ww8::CRGFTC0 => K::CharFontAscii,
        ww8::CRGFTC1 => K::CharFontFarEast,
        ww8::CRGFTC2 => K::CharFontOther,
        ww8::CFTCBI => K::CharFontComplex,
        ww8::CICO | ww8::CICOBI => K::CharColorIco,
        ww8::CCV => K::CharColorRgb,
        ww8::CHIGHLIGHT => K::CharHighlight,
        ww8::CSHD | ww8::CSHD80 => K::CharShading,
        ww8::CBRC | ww8::CBRC80 => K::CharBorder,
        ww8::CISS => K::CharSubSuper,
        ww8::CHPSPOS => K::CharPosition,
        ww8::CDXASPACE => K::CharSpacing,
        ww8::CCHARSCALE => K::CharScale,
        ww8::CHPSKERN => K::CharKerning,
        ww8::CRGLID0 | ww8::CRGLID0_80 | ww8::CLIDBI => K::CharLanguage,
        ww8::CRGLID1 | ww8::CRGLID1_80 => K::CharLanguageFarEast,
        ww8::CFBIDI | ww8::CFCOMPLEXSCRIPTS => K::CharBidi,
        ww8::CFSPEC => K::CharSpecial,
        ww8::CPICLOCATION => K::CharPicLocation,
        ww8::CSYMBOL => K::CharSymbol,

        ww8::PISTD => K::ParaStyle,
        ww8::PJC | ww8::PJC80 => K::ParaJustify,
        ww8::PDXALEFT | ww8::PDXALEFT80 => K::ParaIndentLeft,
        ww8::PDXARIGHT | ww8::PDXARIGHT80 => K::ParaIndentRight,
        ww8::PDXALEFT1 | ww8::PDXALEFT180 => K::ParaIndentFirstLine,
        ww8::PDYABEFORE => K::ParaSpaceBefore,
        ww8::PDYAAFTER => K::ParaSpaceAfter,
        ww8::PFDYABEFOREAUTO => K::ParaSpaceBeforeAuto,
        ww8::PFDYAAFTERAUTO => K::ParaSpaceAfterAuto,
        ww8::PDYALINE => K::ParaLineSpacing,
        ww8::PFCONTEXTUALSPACING => K::ParaContextualSpacing,
        ww8::PFKEEP => K::ParaKeepLines,
        ww8::PFKEEPFOLLOW => K::ParaKeepNext,
        ww8::PFPAGEBREAKBEFORE => K::ParaPageBreakBefore,
        ww8::PFWIDOWCONTROL => K::ParaWidowControl,
        ww8::POUTLVL => K::ParaOutlineLevel,
        ww8::PILVL => K::ParaListLevel,
        ww8::PILFO => K::ParaListIndex,
        ww8::PCHGTABS | ww8::PCHGTABSPAPX => K::ParaTabs,
        ww8::PSHD | ww8::PSHD80 => K::ParaShading,
        ww8::PBRCTOP | ww8::PBRCTOP80 => K::ParaBorderTop,
        ww8::PBRCLEFT | ww8::PBRCLEFT80 => K::ParaBorderLeft,
        ww8::PBRCBOTTOM | ww8::PBRCBOTTOM80 => K::ParaBorderBottom,
        ww8::PBRCRIGHT | ww8::PBRCRIGHT80 => K::ParaBorderRight,
        ww8::PBRCBETWEEN | ww8::PBRCBETWEEN80 => K::ParaBorderBetween,
        ww8::PFBIDI => K::ParaBidi,
        ww8::PFINTABLE => K::ParaInTable,
        ww8::PFTTP => K::ParaTableRowEnd,
        ww8::PITAP => K::ParaTableDepth,
        ww8::PFINNERTABLECELL => K::ParaInnerTableCell,
        ww8::PFINNERTTP => K::ParaInnerTableRowEnd,
        ww8::PHUGEPAPX => K::ParaHugePapx,

        ww8::TJC | ww8::TJC90 => K::TableJustify,
        ww8::TDXALEFT => K::TableRowLeft,
        ww8::TDXAGAPHALF => K::TableCellMarginHalf,
        ww8::TDYAROWHEIGHT => K::TableRowHeight,
        ww8::TDEFTABLE | ww8::TDEFTABLE10 => K::TableDefinition,
        ww8::TDEFTABLESHD | ww8::TDEFTABLESHD80 | ww8::TDEFTABLESHDRAW | ww8::TSETSHD
        | ww8::TSETSHDODD | ww8::TSETSHDTABLE => K::TableCellShading,
        ww8::TTABLEBORDERS | ww8::TTABLEBORDERS80 => K::TableBorders,
        ww8::TSETBRC | ww8::TSETBRC80 => K::TableCellBorders,
        ww8::TBRCTOPCV | ww8::TBRCLEFTCV | ww8::TBRCBOTTOMCV | ww8::TBRCRIGHTCV => {
            K::TableCellBorderColor
        }
        ww8::TTABLEWIDTH => K::TableWidth,
        ww8::TWIDTHBEFORE => K::TableWidthBefore,
        ww8::TWIDTHAFTER => K::TableWidthAfter,
        ww8::TWIDTHINDENT => K::TableIndent,
        ww8::TFCANTSPLIT | ww8::TFCANTSPLIT90 => K::TableCantSplit,
        ww8::TTABLEHEADER => K::TableHeaderRow,
        ww8::TFAUTOFIT => K::TableAutofit,
        ww8::TMERGE => K::TableCellMerge,
        ww8::TVERTMERGE => K::TableCellVertMerge,
        ww8::TVERTALIGN => K::TableCellVertAlign,
        ww8::TCELLPADDING | ww8::TCELLPADDINGDEFAULT => K::TableCellPadding,
        ww8::TCELLWIDTH => K::TableCellWidth,
        ww8::TTEXTFLOW => K::TableCellTextFlow,
        ww8::TISTD => K::TableStyle,
        ww8::TTLP => K::TableLook,

        ww8::SBKC => K::SecBreakKind,
        ww8::SXAPAGE => K::SecPageWidth,
        ww8::SYAPAGE => K::SecPageHeight,
        ww8::SDXALEFT => K::SecMarginLeft,
        ww8::SDXARIGHT => K::SecMarginRight,
        ww8::SDYATOP => K::SecMarginTop,
        ww8::SDYABOTTOM => K::SecMarginBottom,
        ww8::SDZAGUTTER => K::SecGutter,
        ww8::SDYAHDRTOP => K::SecHeaderDistance,
        ww8::SDYAHDRBOTTOM => K::SecFooterDistance,
        ww8::SBORIENTATION => K::SecOrientation,
        ww8::SCCOLUMNS => K::SecColumnCount,
        ww8::SDXACOLUMNS | ww8::SDXACOLSPACING => K::SecColumnSpacing,
        ww8::SDXACOLWIDTH => K::SecColumnWidth,
        ww8::SLBETWEEN => K::SecColumnLine,
        ww8::SFTITLEPAGE => K::SecTitlePage,
        ww8::SPGNSTART | ww8::SPGNSTART97 => K::SecPageNumberStart,
        ww8::SVJC => K::SecVertAlign,
        ww8::STEXTFLOW => K::SecTextFlow,
        ww8::SBRCTOP | ww8::SBRCTOP80 => K::SecPageBorderTop,
        ww8::SBRCLEFT | ww8::SBRCLEFT80 => K::SecPageBorderLeft,
        ww8::SBRCBOTTOM | ww8::SBRCBOTTOM80 => K::SecPageBorderBottom,
        ww8::SBRCRIGHT | ww8::SBRCRIGHT80 => K::SecPageBorderRight,

        _ => K::Other,
    }
}

fn kind_ww6(opcode: u16) -> SprmKind {
    use SprmKind as K;
    let Ok(op) = u8::try_from(opcode) else { return K::Other };
    match op {
        ww6::PISTD => K::ParaStyle,
        ww6::PJC => K::ParaJustify,
        ww6::PDXALEFT => K::ParaIndentLeft,
        ww6::PDXARIGHT => K::ParaIndentRight,
        ww6::PDXALEFT1 => K::ParaIndentFirstLine,
        ww6::PDYABEFORE => K::ParaSpaceBefore,
        ww6::PDYAAFTER => K::ParaSpaceAfter,
        ww6::PDYALINE => K::ParaLineSpacing,
        ww6::PFKEEP => K::ParaKeepLines,
        ww6::PFKEEPFOLLOW => K::ParaKeepNext,
        ww6::PPAGEBREAKBEFORE => K::ParaPageBreakBefore,
        ww6::PFWIDOWCONTROL => K::ParaWidowControl,
        ww6::PNLVLANM => K::ParaListLevel,
        ww6::PCHGTABS | ww6::PCHGTABSPAPX => K::ParaTabs,
        ww6::PSHD => K::ParaShading,
        ww6::PBRCTOP => K::ParaBorderTop,
        ww6::PBRCLEFT => K::ParaBorderLeft,
        ww6::PBRCBOTTOM => K::ParaBorderBottom,
        ww6::PBRCRIGHT => K::ParaBorderRight,
        ww6::PBRCBETWEEN => K::ParaBorderBetween,
        ww6::PFINTABLE => K::ParaInTable,
        ww6::PTTP => K::ParaTableRowEnd,

        ww6::CISTD => K::CharStyle,
        ww6::CPLAIN => K::CharPlain,
        ww6::CFBOLD => K::CharBold,
        ww6::CFITALIC => K::CharItalic,
        ww6::CFSTRIKE => K::CharStrike,
        ww6::CFOUTLINE => K::CharOutline,
        ww6::CFSHADOW => K::CharShadow,
        ww6::CFSMALLCAPS => K::CharSmallCaps,
        ww6::CFCAPS => K::CharCaps,
        ww6::CFVANISH => K::CharHidden,
        ww6::CFTC => K::CharFontAscii,
        ww6::CKUL => K::CharUnderline,
        ww6::CHPS => K::CharSize,
        ww6::CICO => K::CharColorIco,
        ww6::CISS => K::CharSubSuper,
        ww6::CHPSPOS => K::CharPosition,
        ww6::CDXASPACE => K::CharSpacing,
        ww6::CHPSKERN => K::CharKerning,
        ww6::CLID => K::CharLanguage,
        ww6::CFSPEC => K::CharSpecial,
        ww6::CPICLOCATION => K::CharPicLocation,
        ww6::CSYMBOL => K::CharSymbol,

        ww6::TJC => K::TableJustify,
        ww6::TDXALEFT => K::TableRowLeft,
        ww6::TDXAGAPHALF => K::TableCellMarginHalf,
        ww6::TDYAROWHEIGHT => K::TableRowHeight,
        ww6::TDEFTABLE | ww6::TDEFTABLE10 => K::TableDefinition,
        ww6::TDEFTABLESHD | ww6::TSETSHD => K::TableCellShading,
        ww6::TTABLEBORDERS => K::TableBorders,
        ww6::TSETBRC => K::TableCellBorders,
        ww6::TFCANTSPLIT => K::TableCantSplit,
        ww6::TTABLEHEADER => K::TableHeaderRow,
        ww6::TMERGE => K::TableCellMerge,

        ww6::SBKC => K::SecBreakKind,
        ww6::SXAPAGE => K::SecPageWidth,
        ww6::SYAPAGE => K::SecPageHeight,
        ww6::SDXALEFT => K::SecMarginLeft,
        ww6::SDXARIGHT => K::SecMarginRight,
        ww6::SDYATOP => K::SecMarginTop,
        ww6::SDYABOTTOM => K::SecMarginBottom,
        ww6::SDZAGUTTER => K::SecGutter,
        ww6::SDYAHDRTOP => K::SecHeaderDistance,
        ww6::SDYAHDRBOTTOM => K::SecFooterDistance,
        ww6::SBORIENTATION => K::SecOrientation,
        ww6::SCCOLUMNS => K::SecColumnCount,
        ww6::SDXACOLUMNS | ww6::SDXACOLSPACING => K::SecColumnSpacing,
        ww6::SDXACOLWIDTH => K::SecColumnWidth,
        ww6::SLBETWEEN => K::SecColumnLine,
        ww6::SFTITLEPAGE => K::SecTitlePage,
        ww6::SPGNSTART => K::SecPageNumberStart,
        ww6::SVJC => K::SecVertAlign,

        _ => K::Other,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Decode a hex string written inline. Panicking here is fine: it is the
    /// test fixture, not document data.
    fn hex(s: &str) -> Vec<u8> {
        assert!(s.len().is_multiple_of(2), "hex literal of odd length");
        (0..s.len() / 2)
            .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).expect("bad hex digit"))
            .collect()
    }

    fn walk(buf: &[u8], ver: SprmVersion) -> (Vec<u16>, usize) {
        let mut it = SprmIter::new(buf, ver);
        let ids: Vec<u16> = it.by_ref().map(|s| s.opcode).collect();
        (ids, it.consumed())
    }

    /// A real grpprl and how much of it the walk must consume. PAPX buffers are
    /// padded to an even length by the FKP, so they may leave one byte over.
    struct Vector {
        /// Where it came from, so a failure can be reproduced.
        origin: &'static str,
        ver: SprmVersion,
        hex: &'static str,
        slack: usize,
    }

    /// Real property lists lifted out of the LibreOffice `.doc` corpus with a
    /// throwaway CFB/FKP reader (`WordDocument` -> PlcfBteChpx/PlcfBtePapx ->
    /// 512-byte FKP pages -> CHPX/PAPX -> grpprl). They are here as literals so
    /// the suite does not depend on those files being present.
    const VECTORS: &[Vector] = &[
        // --- Word 97+ -----------------------------------------------------
        Vector {
            origin: "fdo59530.doc CHPX — 28 character sprms",
            ver: SprmVersion::Ww8,
            hex: "422a006d480904734809046e48040874480408408800004b480100482a0045480000434a1400\
                  390800380800504a0500360800532a003708005f4839044f4a0600514a0600434a1400524864\
                  003508005e4a06003e2a00614a18005c08005d0800342a00",
            slack: 0,
        },
        Vector {
            origin: "floattable-hidden-anchor.doc PAPX — table row with sprmTDefTable",
            ver: SprmVersion::Ww8,
            hex: "6b6400000000162401172401496601000000029667000794722908d61a00019effaa1800060c\
                  190401010004010100040101000000000009d60200010d36500f9415281094b40012d60a0000\
                  00ffffffff00000014f601000017f60300001ad604000000001bd604000000001cd604000000\
                  001dd604000000ff1e94b40034d60600010503000034d60600010203620034d606000108036c\
                  0061f603000070d60a000000ffffffff0000007974f96c9a0010",
            slack: 1,
        },
        Vector {
            origin: "tdf104239_numbering.doc PAPX — sprmTDefTable with cb = 26",
            ver: SprmVersion::Ww8,
            hex: "6b640000000016240117240149660100000002966c0008d61a0001b41679230006c50c000000\
                  0000000000000000000000000014f603c50c1ad604000000ff1bd604000000ff1cd604000000\
                  ff1dd604000000ff34d60600010a036c0061f6032017",
            slack: 0,
        },
        Vector {
            origin: "tdf112517_maxSprms.doc PAPX — 21 sprms, table + shading",
            ver: SprmVersion::Ww8,
            hex: "6b64000000001624011724014966010000000054010002963300079410ff08d61a0001cdfffd\
                  2b0006302c0601010006010100060101000601010009d602010d12d60a00000000ffffff0003\
                  0014f603372c17f603000018f60307001ad604000000001bd604000000001cd604000000001d\
                  d6040000000034d60600010a03330061f603000070d60a00000000ffffff0003008a54010000",
            slack: 1,
        },
        Vector {
            origin: "tdf128608_tableParaBackColor.doc PAPX — sprmTSetBrc runs",
            ver: SprmVersion::Ww8,
            hex: "16240117240149660100000008d61a000100008526000000000401010004010100040101000401\
                  010009d602000012d60a000000ff000000ff000070d60a000000ff000000ff000034d60600010\
                  103000034d606000102031e0034d60600010403000034d606000108031e006634010334012fd6\
                  0b00010100000000040100002fd60b00010200000000040100002fd60b0001040000000004010\
                  0002fd60b0001080000000004010000",
            slack: 0,
        },
        Vector {
            origin: "bookmark-delete-redline.doc PAPX — sprmPChgTabsPapx, one pad byte",
            ver: SprmVersion::Ww8,
            hex: "0dc6050001000000",
            slack: 1,
        },
        Vector {
            origin: "page-border.doc SEPX — page setup, borders and columns",
            ver: SprmVersion::Ww8,
            hex: "265001002b70300100182c70300100182d70300100182e7030010018319068012f52200034d208\
                  000000ff3001180035d208000000ff3001180036d208000000ff3001180037d208000000ff300\
                  1180 03a703721ed001fb0d02f20b0e03d21b08a0522b08a0523908a0524908a0525b0000017b\
                  0d00218b0d0020c90d002",
            slack: 0,
        },
        // --- Word 6/95 ----------------------------------------------------
        Vector {
            origin: "tdf59896.doc (nFib 101) CHPX — bold + font",
            ver: SprmVersion::Ww6,
            hex: "55015d0200",
            slack: 0,
        },
        Vector {
            origin: "tdf59896.doc (nFib 101) PAPX — sprmPJc + sprmPDyaLine, one pad byte",
            ver: SprmVersion::Ww6,
            hex: "050114f000010000",
            slack: 1,
        },
        Vector {
            origin: "tdf150197_anlv2ListFormat.doc (nFib 104) PAPX — sprmPAnld",
            ver: SprmVersion::Ww6,
            hex: "0d0a11d0021398fe0c34000001080000ffff0000010068017800000000002e00000000000000\
                  0000000000000000000000000000000000000000000000000f050001d0020000",
            slack: 1,
        },
        Vector {
            origin: "forcepoint92.doc (nFib 101) PAPX — sprmTDefTable, two-byte count",
            ver: SprmVersion::Ww6,
            hex: "18011901b86b00bb090009000900090006000600be0e000595ff19057c0fb92b4331cd36",
            slack: 0,
        },
        Vector {
            origin: "forcepoint92.doc (nFib 101) CHPX — picture location + style",
            ver: SprmVersion::Ww6,
            hex: "75014404000000005031005681631200",
            slack: 0,
        },
        Vector {
            origin: "forcepoint92.doc (nFib 101) PAPX — 11 paragraph sprms",
            ver: SprmVersion::Ww6,
            hex: "1d9014f00001001600001a090f1b48041c6f182811082a110831bb0020bb000f050001d01702",
            slack: 0,
        },
    ];

    fn clean(h: &str) -> String {
        h.chars().filter(|c| !c.is_whitespace()).collect()
    }

    /// The heart of the matter: on a real property list the walk must land
    /// exactly on the end of the buffer. Anything else means a size rule is
    /// wrong and every property after that point is misread.
    #[test]
    fn real_grpprls_are_consumed_exactly() {
        for v in VECTORS {
            let buf = hex(&clean(v.hex));
            let (ids, consumed) = walk(&buf, v.ver);
            assert!(!ids.is_empty(), "{}: nothing decoded", v.origin);
            let left = buf.len() - consumed;
            assert!(
                left <= v.slack,
                "{}: {left} byte(s) left over out of {} (ids so far: {:04X?})",
                v.origin,
                buf.len(),
                ids
            );
        }
    }

    /// The operand handed to callers must never include the length prefix and
    /// must stay inside the buffer.
    #[test]
    fn operands_stay_inside_the_buffer() {
        for v in VECTORS {
            let buf = hex(&clean(v.hex));
            let base = buf.as_ptr() as usize;
            for s in SprmIter::new(&buf, v.ver) {
                let start = s.operand.as_ptr() as usize - base;
                assert!(
                    start + s.operand.len() <= buf.len(),
                    "{}: operand of {:04X} escapes the buffer",
                    v.origin,
                    s.opcode
                );
            }
        }
    }

    /// sprmTDefTable is the one opcode with a two-byte count, and the count is
    /// the operand size PLUS ONE (count field included). Reading it as a plain
    /// size — the obvious mistake — shifts everything after it by one byte.
    /// The row below really is `08 d6 | 1a 00 | ...25 bytes...`, immediately
    /// followed by sprmTTableWidth `14 f6`.
    #[test]
    fn tdeftable_two_byte_count_is_size_minus_one() {
        let buf = hex(&clean(VECTORS[2].hex));
        let sprms = iter_sprms(&buf);
        let def = sprms
            .iter()
            .find(|s| s.opcode == ww8::TDEFTABLE)
            .expect("sprmTDefTable not found");
        // cb == 26 -> 25 bytes of operand.
        assert_eq!(def.operand.len(), 25);
        // itcMac = 1 cell: 1 + 2 * (1 + 1) + 20 = 25 bytes. Consistent.
        assert_eq!(def.operand.first().copied(), Some(1));
        // And the next sprm must be sprmTTableWidth, not garbage.
        let pos = sprms.iter().position(|s| s.opcode == ww8::TDEFTABLE).unwrap_or(0);
        assert_eq!(sprms.get(pos + 1).map(|s| s.opcode), Some(ww8::TTABLEWIDTH));
    }

    /// Same rule in the 8-bit id space: sprm 190 with count 14 carries 13 bytes.
    #[test]
    fn ww6_tdeftable_two_byte_count() {
        let buf = hex(&clean(VECTORS[10].hex));
        let sprms = iter_sprms_ver(&buf, SprmVersion::Ww6);
        let ids: Vec<u16> = sprms.iter().map(|s| s.opcode).collect();
        assert_eq!(ids, vec![24, 25, 184, 187, 190]);
        let def = sprms.last().expect("no sprms");
        assert_eq!(def.opcode, u16::from(ww6::TDEFTABLE));
        assert_eq!(def.operand.len(), 13);
        assert_eq!(sprms[3].operand.len(), 12, "sprmTTableBorders is 12 fixed bytes");
    }

    /// A 12-byte fixed operand read as a length-prefixed one (or the reverse)
    /// is exactly the sort of mistake `spra` cannot catch in Word 6/95.
    #[test]
    fn ww6_sizes_follow_the_table_not_the_opcode() {
        // sprmCHps (99) is a word even though Word 6 documents call it a byte.
        let buf = hex("631200");
        let sprms = iter_sprms_ver(&buf, SprmVersion::Ww6);
        assert_eq!(sprms.len(), 1);
        assert_eq!(sprms[0].u16(), Some(0x0012));
        // sprmCPicLocation (68) is length-prefixed with no fixed part.
        let buf = hex("4404aabbccdd");
        let sprms = iter_sprms_ver(&buf, SprmVersion::Ww6);
        assert_eq!(sprms.len(), 1);
        assert_eq!(sprms[0].operand, &[0xAA, 0xBB, 0xCC, 0xDD]);
    }

    /// The exceptions LibreOffice hand-codes: these opcodes lie about their own
    /// size through `spra`, so decoding must not trust it.
    #[test]
    fn ww8_exceptions_override_spra() {
        // sprmCChs 0xEA08: spra 7 would say 3 bytes, the truth is 1.
        assert_eq!(rule_ww8(0xEA08).len, 1);
        assert_eq!(rule_from_spra(0xEA08).len, 3);
        // sprmCDefault 0x2A32: spra 1 would say a fixed byte, it is variable.
        assert_eq!(rule_ww8(0x2A32).vari, Vari::Var);
        assert_eq!(rule_from_spra(0x2A32).vari, Vari::Fix);
        // sprmCMajority50 0xCA4C: spra 6 would say variable, it is 2 fixed.
        assert_eq!(rule_ww8(0xCA4C).vari, Vari::Fix);
        assert_eq!(rule_ww8(0xCA4C).len, 2);
        // sprmTSetShd80 0x7627: spra 3 would say 4 bytes, it is variable.
        assert_eq!(rule_ww8(0x7627).vari, Vari::Var);
        // sprmTDiagLine 0xD62A: spra 6 would say variable, it is 1 fixed.
        assert_eq!(rule_ww8(0xD62A).vari, Vari::Fix);
        // Both TDefTable flavours use the two-byte count.
        assert_eq!(rule_ww8(0xD606).vari, Vari::Var2);
        assert_eq!(rule_ww8(0xD608).vari, Vari::Var2);
    }

    /// Every `spra` value must map to the size LibreOffice uses, including the
    /// two "same size, different scaling" cases 4 and 5.
    #[test]
    fn spra_sizes() {
        let cases = [
            (0x0835u16, 1usize), // spra 0, toggle byte
            (0x2A42, 1),         // spra 1, byte
            (0x4A43, 2),         // spra 2, word
            (0x6412, 4),         // spra 3, long
            (0x840F, 2),         // spra 4, word
            (0xA413, 2),         // spra 5, word
            (0xF614, 3),         // spra 7, three bytes
        ];
        for (op, len) in cases {
            assert_eq!(rule_from_spra(op).len as usize, len, "spra of {op:04X}");
            assert_eq!(rule_from_spra(op).vari, Vari::Fix);
        }
        assert_eq!(rule_from_spra(0xC60D).vari, Vari::Var); // spra 6
    }

    /// sprmPChgTabs uses 255 as an escape: the real length must then be worked
    /// out from the deleted/inserted tab counts.
    #[test]
    fn chgtabs_escape_length() {
        // 2 deleted tabs (4 bytes each), 1 inserted (3 bytes).
        let mut buf = vec![0x15, 0xC6, 255, 2];
        buf.extend_from_slice(&[0xAA; 8]);
        buf.push(1);
        buf.extend_from_slice(&[0xBB; 3]);
        buf.extend_from_slice(&[0x35, 0x08, 0x01]); // sprmCFBold after it
        let (ids, consumed) = walk(&buf, SprmVersion::Ww8);
        assert_eq!(ids, vec![ww8::PCHGTABS, ww8::CFBOLD]);
        assert_eq!(consumed, buf.len());
        // The plain form still works.
        let buf = hex("15c60401020304350801");
        let (ids, consumed) = walk(&buf, SprmVersion::Ww8);
        assert_eq!(ids, vec![ww8::PCHGTABS, ww8::CFBOLD]);
        assert_eq!(consumed, buf.len());
    }

    /// Opcodes below 0x0800 cannot exist in WW8; treating one as a sprm would
    /// send the walk off into the weeds.
    #[test]
    fn ww8_rejects_impossible_opcodes() {
        let buf = hex("0100ffffffff");
        let (ids, consumed) = walk(&buf, SprmVersion::Ww8);
        assert!(ids.is_empty());
        assert_eq!(consumed, 0);
    }

    /// Truncation must stop the walk, never panic and never spin.
    #[test]
    fn truncated_buffers_never_panic() {
        for v in VECTORS {
            let full = hex(&clean(v.hex));
            for cut in 0..full.len() {
                let (_, consumed) = walk(&full[..cut], v.ver);
                assert!(consumed <= cut, "{}: consumed past the end", v.origin);
            }
        }
    }

    /// Arbitrary bytes must terminate too. A byte pattern that made the walk
    /// return a zero-length sprm would loop forever.
    #[test]
    fn garbage_terminates() {
        let patterns: [Vec<u8>; 6] = [
            vec![],
            vec![0u8; 64],
            vec![0xFFu8; 64],
            (0..=255u8).collect(),
            (0..=255u8).rev().collect(),
            (0..512u32).map(|i| (i.wrapping_mul(2654435761) >> 13) as u8).collect(),
        ];
        for ver in [SprmVersion::Ww6, SprmVersion::Ww6Cjk, SprmVersion::Ww8] {
            for p in &patterns {
                let mut it = SprmIter::new(p, ver);
                let mut n = 0usize;
                for _ in it.by_ref() {
                    n += 1;
                    assert!(n <= p.len() + 1, "walk did not advance on {ver:?}");
                }
                assert!(it.consumed() <= p.len());
            }
        }
    }

    /// Every prefix of every window into a real buffer: brute force against
    /// panics on misaligned data.
    #[test]
    fn misaligned_windows_never_panic() {
        for v in VECTORS {
            let full = hex(&clean(v.hex));
            for start in 0..full.len() {
                let slice = &full[start..];
                let mut n = 0usize;
                for _ in SprmIter::new(slice, v.ver) {
                    n += 1;
                    assert!(n <= slice.len(), "{}: no progress at offset {start}", v.origin);
                }
            }
        }
    }

    /// Semantic mapping: the same meaning must come out of both id spaces.
    #[test]
    fn kinds_agree_across_versions() {
        let pairs = [
            (ww8::CFBOLD, u16::from(ww6::CFBOLD), SprmKind::CharBold),
            (ww8::CHPS, u16::from(ww6::CHPS), SprmKind::CharSize),
            (ww8::CICO, u16::from(ww6::CICO), SprmKind::CharColorIco),
            (ww8::PJC, u16::from(ww6::PJC), SprmKind::ParaJustify),
            (ww8::PDXALEFT, u16::from(ww6::PDXALEFT), SprmKind::ParaIndentLeft),
            (ww8::PDYABEFORE, u16::from(ww6::PDYABEFORE), SprmKind::ParaSpaceBefore),
            (ww8::PISTD, u16::from(ww6::PISTD), SprmKind::ParaStyle),
            (ww8::TDEFTABLE, u16::from(ww6::TDEFTABLE), SprmKind::TableDefinition),
            (ww8::SXAPAGE, u16::from(ww6::SXAPAGE), SprmKind::SecPageWidth),
            (ww8::SDXALEFT, u16::from(ww6::SDXALEFT), SprmKind::SecMarginLeft),
        ];
        for (a, b, want) in pairs {
            assert_eq!(kind_of(a, SprmVersion::Ww8), want, "ww8 {a:04X}");
            assert_eq!(kind_of(b, SprmVersion::Ww6), want, "ww6 {b}");
        }
        // The legacy and modern spellings of the same property agree.
        assert_eq!(kind_ww8(ww8::PJC80), SprmKind::ParaJustify);
        assert_eq!(kind_ww8(ww8::PDXALEFT80), SprmKind::ParaIndentLeft);
    }

    /// Named constants must actually match the encoding they claim.
    #[test]
    fn constants_decode_to_their_category() {
        let s = |op| Sprm { opcode: op, operand: &[] };
        assert_eq!(s(ww8::CFBOLD).sgc(), Sgc::Character);
        assert_eq!(s(ww8::PJC).sgc(), Sgc::Paragraph);
        assert_eq!(s(ww8::TDEFTABLE).sgc(), Sgc::Table);
        assert_eq!(s(ww8::SXAPAGE).sgc(), Sgc::Section);
        assert_eq!(s(ww8::PICBRCTOP80).sgc(), Sgc::Picture);
        assert_eq!(s(ww8::CFBOLD).ispmd(), 0x35);
        assert_eq!(s(ww8::TDEFTABLE).ispmd(), 0x08);
        assert!(s(ww8::TDEFTABLE).f_spec());
        assert!(!s(ww8::CFBOLD).f_spec());
        // Word 6/95 has no sgc field; classification is by range.
        assert_eq!(s(u16::from(ww6::CFBOLD)).sgc_ver(SprmVersion::Ww6), Sgc::Character);
        assert_eq!(s(u16::from(ww6::PJC)).sgc_ver(SprmVersion::Ww6), Sgc::Paragraph);
        assert_eq!(s(u16::from(ww6::TDEFTABLE)).sgc_ver(SprmVersion::Ww6), Sgc::Table);
        assert_eq!(s(u16::from(ww6::SXAPAGE)).sgc_ver(SprmVersion::Ww6), Sgc::Section);
    }

    /// Toggles use Word's 0/1/128/129 convention.
    #[test]
    fn toggle_convention() {
        let t = |b: u8, inh: bool| Sprm { opcode: ww8::CFBOLD, operand: &[b] }.toggle(inh);
        assert_eq!(t(0, true), Some(false));
        assert_eq!(t(1, false), Some(true));
        assert_eq!(t(128, true), None);
        assert_eq!(t(129, true), Some(false));
        assert_eq!(t(129, false), Some(true));
        assert_eq!(Sprm { opcode: ww8::CFBOLD, operand: &[] }.toggle(false), None);
    }

    /// Values read out of a real character run.
    #[test]
    fn character_run_decodes_to_sensible_values() {
        let buf = hex(&clean(VECTORS[0].hex));
        let sprms = iter_sprms(&buf);
        let get = |op| sprms.iter().rev().find(|s| s.opcode == op).copied();
        // 0x4A43 sprmCHps appears twice; the last one wins (20 half-points).
        assert_eq!(get(ww8::CHPS).and_then(|s| s.u16()), Some(20));
        assert_eq!(
            find_sprm(&buf, SprmVersion::Ww8, ww8::CHPS).and_then(|s| s.u16()),
            Some(20)
        );
        // 0x0835 sprmCFBold with operand 0 -> not bold.
        assert_eq!(get(ww8::CFBOLD).and_then(|s| s.toggle(false)), Some(false));
        // 0x4852 sprmCCharScale, 100%.
        assert_eq!(get(ww8::CCHARSCALE).and_then(|s| s.u16()), Some(100));
        // 0x4A61 complex-script size, 24 half-points.
        assert_eq!(get(ww8::CHPSBI).and_then(|s| s.u16()), Some(24));
    }

    /// Page setup out of a real section list, to prove the section sprms line
    /// up (a shifted stream would give absurd page dimensions).
    #[test]
    fn section_decodes_to_a_plausible_page() {
        let buf = hex(&clean(VECTORS[6].hex));
        let get = |op| find_sprm(&buf, SprmVersion::Ww8, op).and_then(|s| s.u16());
        // US Letter in twips: 12240 x 15840, one inch of margin all round.
        assert_eq!(get(ww8::SXAPAGE), Some(12240));
        assert_eq!(get(ww8::SYAPAGE), Some(15840));
        assert_eq!(get(ww8::SDXALEFT), Some(1418));
        assert_eq!(get(ww8::SDXARIGHT), Some(1418));
        assert_eq!(get(ww8::SDYATOP), Some(1418));
        assert_eq!(get(ww8::SDYABOTTOM), Some(1418));
        assert_eq!(get(ww8::SDZAGUTTER), Some(0));
        assert_eq!(get(ww8::SDYAHDRTOP), Some(720));
        assert_eq!(get(ww8::SDYAHDRBOTTOM), Some(720));
    }

    /// `SprmVersion::from_fib` is the switch that selects the whole 8-bit path.
    #[test]
    fn version_comes_from_the_fib() {
        let mut fib = Fib { nfib: 101, ..Default::default() };
        assert_eq!(SprmVersion::from_fib(&fib), SprmVersion::Ww6);
        fib.nfib = 104;
        assert_eq!(SprmVersion::from_fib(&fib), SprmVersion::Ww6);
        fib.nfib = 192;
        assert_eq!(SprmVersion::from_fib(&fib), SprmVersion::Ww6);
        fib.nfib = 193;
        assert_eq!(SprmVersion::from_fib(&fib), SprmVersion::Ww8);
        fib.nfib = 257;
        assert_eq!(SprmVersion::from_fib(&fib), SprmVersion::Ww8);
    }

    /// The East-Asian Word 7 variant reinterprets 111-113 as fixed words.
    #[test]
    fn ww6_cjk_variant() {
        let buf = hex("6f34120000");
        assert_eq!(walk(&buf, SprmVersion::Ww6Cjk).1, 3);
        assert_eq!(rule_ww6(111, false).vari, Vari::Var);
        assert_eq!(rule_ww6(111, true).vari, Vari::Fix);
        assert_eq!(rule_ww6(113, true).len, 2);
    }
}
