//! Numbering and bullets — the `PlcfLst` and `PlfLfo` tables.
//!
//! WW8 numbering is the two-tier model OOXML later inherited:
//!
//!   * `PlcfLst` holds the ABSTRACT lists (`LSTF`), each followed by its
//!     levels (`LVL` = `LVLF` + a paragraph `grpprl` + a character `grpprl` +
//!     the marker text). One level for a "simple" list, nine otherwise.
//!   * `PlfLfo` holds the INSTANCES (`LFO`), each pointing at an abstract list
//!     by id and optionally overriding some of its levels (`LFOLVL`) — the
//!     equivalent of `w:num` / `w:lvlOverride`.
//!
//! A paragraph carries `sprmPIlfo` (1-based index into the LFO array, 0 = no
//! list) and `sprmPIlvl` (0-based level). Resolving that pair is what the
//! paragraph assembly needs to emit `bulletList` / `orderedList` / `listItem`
//! the same way the DOCX import does.
//!
//! Reference: LibreOffice `sw/source/filter/ww8/ww8par3.cxx`
//! (`WW8ListManager`, `ReadLSTF`, `ReadLVL`, the LFO loop) and [MS-DOC] 2.9.
//!
//! Sizes are the load-bearing part: `LSTF` is 28 bytes because it always
//! carries a style id for all NINE levels even when the list is simple, and
//! `LVLF` is 28 bytes because it always carries nine `rgbxchNums` offsets.
//! Getting either wrong desynchronises every following record.

// The paragraph assembly (`pap`/`text`) is what consumes this table; until it
// is wired up the richer accessors look unused to the compiler.
#![allow(dead_code)]

use super::fib::Fib;
use super::sprm::iter_sprms;

/// Word always reserves nine levels per list.
pub(crate) const MAX_LEVELS: usize = 9;

/// Fixed sizes, in bytes, of the records of the two tables.
const LSTF_LEN: usize = 28;
const LVLF_LEN: usize = 28;
const LFO_LEN: usize = 16;
const LFOLVL_LEN: usize = 8;

/// `ilfo` Word reserves for Word 6/95 style numbering carried in `sprmPAnld`.
/// It is not an index into the LFO table and must not be resolved as one.
const ILFO_WW6: u16 = 2047;

/// Paragraph sprms that select a list — exposed so the paragraph decoder and
/// this module cannot disagree on the opcodes.
pub(crate) const SPRM_PILVL: u16 = 0x260A;
pub(crate) const SPRM_PILFO: u16 = 0x460B;

// ── Number formats ───────────────────────────────────────────────────────────

/// The `nfc` of a level: how the counter is rendered, or that there is none.
///
/// Word has ~60 of these (Japanese, Hebrew, Thai…); only the ones that change
/// how we classify a list are named, the rest keep their raw code and count as
/// ordinary numbering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NumFmt {
    Decimal,
    UpperRoman,
    LowerRoman,
    UpperLetter,
    LowerLetter,
    Ordinal,
    CardinalText,
    OrdinalText,
    Hex,
    ChicagoManual,
    /// `arabicLZ` — 01, 02, 03…
    DecimalZero,
    /// A bullet: the marker is a literal glyph, not a counter.
    Bullet,
    /// Explicitly "no number" — a list item without a visible marker.
    NoNumber,
    /// Any other `nfc`; rendered as a number.
    Other(u8),
}

impl NumFmt {
    pub(crate) fn from_nfc(nfc: u8) -> NumFmt {
        match nfc {
            0 => NumFmt::Decimal,
            1 => NumFmt::UpperRoman,
            2 => NumFmt::LowerRoman,
            3 => NumFmt::UpperLetter,
            4 => NumFmt::LowerLetter,
            5 => NumFmt::Ordinal,
            6 => NumFmt::CardinalText,
            7 => NumFmt::OrdinalText,
            8 => NumFmt::Hex,
            9 => NumFmt::ChicagoManual,
            22 => NumFmt::DecimalZero,
            23 => NumFmt::Bullet,
            255 => NumFmt::NoNumber,
            other => NumFmt::Other(other),
        }
    }

    pub(crate) fn is_bullet(self) -> bool {
        self == NumFmt::Bullet
    }

    /// True when the level shows a counter. Bullets and "no number" do not, and
    /// the DOCX import classifies `bullet`/`none` exactly the same way.
    pub(crate) fn is_ordered(self) -> bool {
        !matches!(self, NumFmt::Bullet | NumFmt::NoNumber)
    }

    /// The OOXML `w:numFmt` name, so both importers speak one vocabulary.
    pub(crate) fn ooxml_name(self) -> &'static str {
        match self {
            NumFmt::Decimal => "decimal",
            NumFmt::UpperRoman => "upperRoman",
            NumFmt::LowerRoman => "lowerRoman",
            NumFmt::UpperLetter => "upperLetter",
            NumFmt::LowerLetter => "lowerLetter",
            NumFmt::Ordinal => "ordinal",
            NumFmt::CardinalText => "cardinalText",
            NumFmt::OrdinalText => "ordinalText",
            NumFmt::Hex => "hex",
            NumFmt::ChicagoManual => "chicago",
            NumFmt::DecimalZero => "decimalZero",
            NumFmt::Bullet => "bullet",
            NumFmt::NoNumber => "none",
            NumFmt::Other(_) => "decimal",
        }
    }
}

/// What follows the marker before the paragraph text (`ixchFollow`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LabelFollow {
    Tab,
    Space,
    Nothing,
}

/// One piece of a level's marker text (`xst`).
///
/// In `xst` the character codes 0x00..=0x08 are NOT text: each is a
/// placeholder for the counter of the level with that (0-based) index — the
/// ancestor of OOXML's `%1`. "Article %1.%2" is stored as
/// `"Article \u{0}.\u{1}"`; `rgbxchNums` in the `LVLF` lists the same offsets,
/// 1-based, and we cross-check against it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LvlPart {
    Text(String),
    /// Counter of a level, 0-based.
    Level(u8),
}

// ── Levels ───────────────────────────────────────────────────────────────────

/// One numbering level of an abstract list (an `LVL`).
#[derive(Debug, Clone)]
pub(crate) struct ListLevel {
    /// Value the counter starts at.
    pub(crate) start_at: i32,
    /// Raw number format code.
    pub(crate) nfc: u8,
    /// Decoded number format.
    pub(crate) fmt: NumFmt,
    /// Marker alignment: 0 left, 1 centred, 2 right.
    pub(crate) justify: u8,
    /// Legal numbering: render inherited levels as decimal.
    pub(crate) legal: bool,
    /// Never restart the counter under a higher level.
    pub(crate) no_restart: bool,
    /// Word 6 compatibility: the marker includes the previous levels.
    pub(crate) legacy_prev: bool,
    /// What separates the marker from the text.
    pub(crate) follow: LabelFollow,
    /// Left indent of the paragraph, in twips (`sprmPDxaLeft`).
    pub(crate) indent_left: i32,
    /// First-line indent, in twips — negative for a hanging marker.
    pub(crate) indent_first: i32,
    /// Tab stop the marker is followed by, in twips (0 = none).
    pub(crate) tab_pos: i32,
    /// Marker text, level placeholders decoded.
    pub(crate) text: Vec<LvlPart>,
    /// Marker text exactly as stored, placeholders included.
    pub(crate) raw_text: String,
    /// The bullet glyph, when this level is a bullet.
    pub(crate) bullet: Option<char>,
    /// Font of the bullet glyph, as an index into the font table (`ftc`).
    /// Wingdings bullets are meaningless without it.
    pub(crate) bullet_font_ftc: Option<u16>,
    /// Font name, once [`ListTable::resolve_fonts`] has been given the table.
    pub(crate) bullet_font: Option<String>,
    /// The marker is a picture, not a glyph.
    pub(crate) picture_bullet: bool,
    /// Paragraph style bound to this level (`istd`), `0x0FFF` when none.
    pub(crate) istd: u16,
}

impl Default for ListLevel {
    fn default() -> ListLevel {
        ListLevel {
            start_at: 1,
            nfc: 0,
            fmt: NumFmt::Decimal,
            justify: 0,
            legal: false,
            no_restart: false,
            legacy_prev: false,
            follow: LabelFollow::Tab,
            indent_left: 0,
            indent_first: 0,
            tab_pos: 0,
            text: Vec::new(),
            raw_text: String::new(),
            bullet: None,
            bullet_font_ftc: None,
            bullet_font: None,
            picture_bullet: false,
            istd: 0x0FFF,
        }
    }
}

impl ListLevel {
    pub(crate) fn is_bullet(&self) -> bool {
        self.fmt.is_bullet() || self.picture_bullet
    }

    pub(crate) fn is_ordered(&self) -> bool {
        !self.picture_bullet && self.fmt.is_ordered()
    }

    /// The marker written the OOXML way (`%1.%2`), for debugging and for
    /// anything that wants to reproduce Word's label verbatim.
    pub(crate) fn text_ooxml(&self) -> String {
        let mut out = String::new();
        for part in &self.text {
            match part {
                LvlPart::Text(t) => out.push_str(t),
                LvlPart::Level(l) => {
                    out.push('%');
                    out.push_str(&(l + 1).to_string());
                }
            }
        }
        out
    }

    /// How many ancestor levels the marker quotes ("1.2.3" quotes two).
    pub(crate) fn included_levels(&self) -> usize {
        self.text
            .iter()
            .filter(|p| matches!(p, LvlPart::Level(_)))
            .count()
    }
}

// ── Abstract lists and overrides ─────────────────────────────────────────────

/// An abstract list — one `LSTF` and the levels that follow it.
#[derive(Debug, Clone)]
pub(crate) struct AbstractList {
    /// Unique list id, stable across the document; two `LFO`s sharing it
    /// continue the SAME numbering.
    pub(crate) lsid: u32,
    /// Template code, unused by us.
    pub(crate) tplc: u32,
    /// Paragraph style per level.
    pub(crate) istd: [u16; MAX_LEVELS],
    /// Only one level is actually stored in the file.
    pub(crate) simple: bool,
    /// Word 6 compatibility: restart the numbering in every section.
    pub(crate) restart_each_section: bool,
    /// One entry for a simple list, nine otherwise.
    pub(crate) levels: Vec<ListLevel>,
}

/// One overridden level of an `LFO` (an `LFOLVL`).
#[derive(Debug, Clone)]
pub(crate) struct LevelOverride {
    /// Overridden start value, when the override carries one.
    pub(crate) start_at: Option<i32>,
    /// Completely replaced level definition.
    pub(crate) level: Option<Box<ListLevel>>,
}

/// A list instance — an `LFO` and its overrides.
#[derive(Debug, Clone)]
pub(crate) struct ListOverride {
    /// Id of the abstract list this instance uses.
    pub(crate) lsid: u32,
    /// Index into [`ListTable::lists`], when the abstract list exists.
    pub(crate) list: Option<usize>,
    /// Number of overridden levels, as declared by the file.
    pub(crate) n_lvl: u8,
    /// Overrides by level; `None` where the abstract list applies unchanged.
    pub(crate) overrides: Vec<Option<LevelOverride>>,
}

/// A level resolved for a given (`ilfo`, `ilvl`) pair.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ResolvedLevel<'a> {
    /// Identity of the abstract list — paragraphs that disagree on it belong
    /// to different lists even at the same level.
    pub(crate) lsid: u32,
    /// Index of the `LFO`, 0-based.
    pub(crate) lfo: usize,
    /// Level actually used, 0-based and already clamped to what the list
    /// defines (a simple list only ever has level 0).
    pub(crate) ilvl: u8,
    /// Start value, override applied.
    pub(crate) start_at: i32,
    /// The level definition, override applied.
    pub(crate) level: &'a ListLevel,
}

impl ResolvedLevel<'_> {
    pub(crate) fn is_ordered(&self) -> bool {
        self.level.is_ordered()
    }
    pub(crate) fn is_bullet(&self) -> bool {
        self.level.is_bullet()
    }
}

// ── The table ────────────────────────────────────────────────────────────────

/// Everything a document says about numbering.
#[derive(Debug, Clone, Default)]
pub(crate) struct ListTable {
    pub(crate) lists: Vec<AbstractList>,
    pub(crate) overrides: Vec<ListOverride>,
    /// Bytes read from `fcPlcfLst`, levels included — larger than `lcbPlcfLst`
    /// by design, see [`ListTable::parse_lst`].
    pub(crate) lst_bytes: usize,
    /// Bytes of `PlfLfo` left unread; 0 when the table came out exact.
    pub(crate) lfo_slack: usize,
    /// Every record the file announced was decoded. False means a truncated or
    /// damaged table — what was read before the break is still usable.
    pub(crate) complete: bool,
}

impl ListTable {
    /// Decode both tables from the table stream, located through the FIB.
    ///
    /// Lists only exist from Word 97 on; older documents number paragraphs
    /// through `sprmPAnld`, which is not handled here.
    ///
    /// ⚠️ Trusts `fib.fc_plcf_lst` / `fib.fc_plf_lfo` to be pairs 73 and 74 of
    /// `FibRgFcLcb97`. [`ListTable::parse_streams`] reads them itself and is
    /// the safer call when the FIB decoder is not known to be right.
    pub(crate) fn parse(fib: &Fib, table: &[u8]) -> ListTable {
        if fib.nfib < 193 || fib.fc_plcf_lst == fib.fc_plf_lfo {
            return ListTable::default();
        }
        ListTable::parse_ranges(
            table,
            (fib.fc_plcf_lst, fib.lcb_plcf_lst),
            (fib.fc_plf_lfo, fib.lcb_plf_lfo),
        )
    }

    /// Same, but locating both tables in the WordDocument stream directly.
    ///
    /// The two `fc`/`lcb` pairs sit at a fixed place in the FIB, and reading
    /// them here keeps this module usable even when the FIB decoder does not
    /// expose them (or exposes them from the wrong slot).
    pub(crate) fn parse_streams(doc: &[u8], table: &[u8]) -> ListTable {
        match locate(doc) {
            Some((lst, lfo)) => ListTable::parse_ranges(table, lst, lfo),
            None => ListTable::default(),
        }
    }

    /// Decode both tables from explicit `(fc, lcb)` ranges of the table stream.
    pub(crate) fn parse_ranges(table: &[u8], lst: (u32, u32), lfo: (u32, u32)) -> ListTable {
        let mut t = ListTable { complete: true, ..Default::default() };
        t.parse_lst(table, lst.0, lst.1);
        t.parse_lfo(slice(table, lfo.0, lfo.1));
        t.link_overrides();
        t
    }

    /// 1. `PlcfLst`: a count of `LSTF`, the `LSTF`s, then ALL their levels.
    ///
    /// ⚠️ `lcbPlcfLst` covers the count and the `LSTF` array ONLY — the `LVL`
    /// array runs past it, up to wherever the next structure starts. Clamping
    /// the read to `lcb` yields lists with no levels at all. LibreOffice reads
    /// the levels straight off the stream for the same reason
    /// (`WW8ListManager`, step 1.2).
    fn parse_lst(&mut self, table: &[u8], fc: u32, lcb: u32) {
        let start = (fc as usize).min(table.len());
        let data = &table[start..];
        if data.len() < 2 || lcb < 2 {
            return;
        }
        let mut r = Reader::new(data);
        let count = r.u16().unwrap_or(0) as usize;
        // The LSTF array cannot be bigger than what `lcb` announced, nor than
        // the stream itself: refuse a corrupt count instead of trusting it.
        let max = ((lcb as usize - 2) / LSTF_LEN).min(data.len() / LSTF_LEN);
        if count > max {
            self.complete = false;
        }
        for _ in 0..count.min(max) {
            let Some(lst) = read_lstf(&mut r) else {
                self.complete = false;
                break;
            };
            self.lists.push(lst);
        }
        // The levels of every list follow, back to back, in list order. A
        // simple list stores ONE level here even though its `LSTF` still
        // carries style ids for all nine.
        for i in 0..self.lists.len() {
            let n = if self.lists[i].simple { 1 } else { MAX_LEVELS };
            for _ in 0..n {
                let Some(lvl) = read_lvl(&mut r) else {
                    self.complete = false;
                    self.lst_bytes = r.consumed();
                    return;
                };
                self.lists[i].levels.push(lvl);
            }
        }
        // Bind each level to the paragraph style the LSTF named for it.
        for list in &mut self.lists {
            for (i, lvl) in list.levels.iter_mut().enumerate() {
                lvl.istd = list.istd[i.min(MAX_LEVELS - 1)];
            }
        }
        self.lst_bytes = r.consumed();
    }

    /// 2. `PlfLfo`: a count of `LFO`, the `LFO`s, then one `LFOData` each.
    ///
    /// Unlike `PlcfLst`, this table is self-contained: `lcbPlfLfo` covers the
    /// `LFOData` array too.
    fn parse_lfo(&mut self, data: &[u8]) {
        if data.len() < 4 {
            return;
        }
        let mut r = Reader::new(data);
        let count = r.u32().unwrap_or(0) as usize;
        let max = (data.len() - 4) / LFO_LEN;
        if count > max {
            self.complete = false;
        }
        for _ in 0..count.min(max) {
            let Some(lfo) = read_lfo(&mut r) else {
                self.complete = false;
                break;
            };
            self.overrides.push(lfo);
        }
        // Every LFO owns an `LFOData` — even one that overrides nothing, which
        // is then just its 4-byte header. Skipping the headers of the empty
        // ones is what keeps the `LFOLVL`s of the others aligned.
        for i in 0..self.overrides.len() {
            if r.remaining() < 4 {
                self.complete = false;
                break;
            }
            r.skip(4);
            for _ in 0..self.overrides[i].n_lvl {
                match read_lfolvl(&mut r) {
                    Some((lvl, ovr)) => {
                        if (lvl as usize) < MAX_LEVELS {
                            self.overrides[i].overrides[lvl as usize] = Some(ovr);
                        }
                    }
                    // One unreadable override means every following record is
                    // at an unknown offset: stop rather than invent levels.
                    None => {
                        self.complete = false;
                        self.lfo_slack = r.remaining();
                        return;
                    }
                }
            }
        }
        self.lfo_slack = r.remaining();
    }

    /// Resolve every `LFO`'s list id to an index into `lists`.
    fn link_overrides(&mut self) {
        for o in &mut self.overrides {
            o.list = self.lists.iter().position(|l| l.lsid == o.lsid);
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.lists.is_empty() && self.overrides.is_empty()
    }

    /// The level a paragraph's `sprmPIlfo` / `sprmPIlvl` select.
    ///
    /// `ilfo` is 1-based; 0 means "no list" and 2047 means Word 6 numbering,
    /// which lives elsewhere. Out-of-range values yield `None` instead of
    /// panicking — corrupt documents do reach this.
    pub(crate) fn level(&self, ilfo: u16, ilvl: u8) -> Option<ResolvedLevel<'_>> {
        if ilfo == 0 || ilfo == ILFO_WW6 {
            return None;
        }
        let idx = (ilfo as usize).checked_sub(1)?;
        let ovr = self.overrides.get(idx)?;
        let list = self.lists.get(ovr.list?)?;
        if list.levels.is_empty() {
            return None;
        }
        // A simple list defines level 0 only; Word still lets a paragraph ask
        // for a deeper one, and shows it with the single level it has.
        let want = (ilvl as usize).min(MAX_LEVELS - 1);
        let ilvl = want.min(list.levels.len() - 1);

        let base = &list.levels[ilvl];
        let over = ovr.overrides.get(ilvl).and_then(|o| o.as_ref());
        let level = over.and_then(|o| o.level.as_deref()).unwrap_or(base);
        // An override that replaces the formatting without claiming a start
        // value keeps the abstract list's one (`bSetStartNo` in `ReadLVL`).
        let start_at = over.and_then(|o| o.start_at).unwrap_or(base.start_at);

        Some(ResolvedLevel {
            lsid: list.lsid,
            lfo: idx,
            ilvl: ilvl as u8,
            start_at,
            level,
        })
    }

    /// Shorthand: is this paragraph in an ordered list, a bulleted one, or
    /// none at all?
    pub(crate) fn is_ordered(&self, ilfo: u16, ilvl: u8) -> Option<bool> {
        self.level(ilfo, ilvl).map(|r| r.is_ordered())
    }

    /// Fill in bullet font NAMES from the document's font table, which is
    /// parsed elsewhere (`SttbfFfn`). Indices we cannot resolve stay `None`.
    pub(crate) fn resolve_fonts(&mut self, fonts: &[String]) {
        let name = |ftc: Option<u16>| -> Option<String> {
            fonts.get(ftc? as usize).cloned()
        };
        for l in &mut self.lists {
            for lvl in &mut l.levels {
                lvl.bullet_font = name(lvl.bullet_font_ftc);
            }
        }
        for o in &mut self.overrides {
            for ovr in o.overrides.iter_mut().flatten() {
                if let Some(lvl) = ovr.level.as_deref_mut() {
                    lvl.bullet_font = name(lvl.bullet_font_ftc);
                }
            }
        }
    }
}

// ── Record readers ───────────────────────────────────────────────────────────

/// One `LSTF` (28 bytes): id, template, nine style ids, flags.
fn read_lstf(r: &mut Reader) -> Option<AbstractList> {
    let rec = r.take(LSTF_LEN)?;
    let mut istd = [0u16; MAX_LEVELS];
    for (i, s) in istd.iter_mut().enumerate() {
        *s = u16::from_le_bytes([rec[8 + i * 2], rec[9 + i * 2]]);
    }
    let bits = rec[26];
    Some(AbstractList {
        lsid: u32::from_le_bytes([rec[0], rec[1], rec[2], rec[3]]),
        tplc: u32::from_le_bytes([rec[4], rec[5], rec[6], rec[7]]),
        istd,
        simple: bits & 0x01 != 0,
        restart_each_section: bits & 0x02 != 0,
        levels: Vec::new(),
    })
}

/// One `LVL`: the 28-byte `LVLF`, a paragraph `grpprl`, a character `grpprl`,
/// then the marker text as a 16-bit-counted UTF-16 string.
fn read_lvl(r: &mut Reader) -> Option<ListLevel> {
    let f = r.take(LVLF_LEN)?;
    let mut lvl = ListLevel {
        start_at: i32::from_le_bytes([f[0], f[1], f[2], f[3]]),
        nfc: f[4],
        fmt: NumFmt::from_nfc(f[4]),
        ..Default::default()
    };
    let bits = f[5];
    lvl.justify = bits & 0x03;
    lvl.legal = bits & 0x04 != 0;
    lvl.no_restart = bits & 0x08 != 0;
    lvl.legacy_prev = bits & 0x10 != 0;
    // f[6..15] = rgbxchNums, f[15] = ixchFollow.
    let rgbxch = [
        f[6], f[7], f[8], f[9], f[10], f[11], f[12], f[13], f[14],
    ];
    lvl.follow = match f[15] {
        1 => LabelFollow::Space,
        2 => LabelFollow::Nothing,
        _ => LabelFollow::Tab,
    };
    // f[16..20] = dxaIndentSav, f[20..24] unused, then the two grpprl sizes.
    let cb_chpx = f[24] as usize;
    let cb_papx = f[25] as usize;

    // The paragraph properties come first and carry the indents.
    let papx = r.take(cb_papx)?;
    read_papx(&mut lvl, papx);
    let chpx = r.take(cb_chpx)?;
    read_chpx(&mut lvl, chpx);

    // Marker text.
    let cch = r.u16()? as usize;
    let raw = r.take(cch * 2)?;
    let units: Vec<u16> = raw
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    lvl.raw_text = String::from_utf16_lossy(&units);
    lvl.text = split_marker(&lvl.raw_text, &rgbxch);

    if lvl.fmt.is_bullet() {
        // A bullet's marker is the glyph itself, never a placeholder.
        lvl.bullet = lvl.raw_text.chars().next().filter(|c| *c as u32 >= 0x20);
    }
    Some(lvl)
}

/// Indents and the marker's tab stop, out of a level's paragraph `grpprl`.
fn read_papx(lvl: &mut ListLevel, grpprl: &[u8]) {
    for s in iter_sprms(grpprl) {
        match s.opcode {
            // sprmPDxaLeft — the Word 97 opcode and the Word 2000 one.
            0x840F | 0x845E => {
                if let Some(v) = s.i16() {
                    lvl.indent_left = v as i32;
                }
            }
            // sprmPDxaLeft1 (first line, negative = hanging).
            0x8411 | 0x8460 => {
                if let Some(v) = s.i16() {
                    lvl.indent_first = v as i32;
                }
            }
            // sprmPChgTabs: a numbering level writes exactly one tab stop —
            // 0 deleted, 1 added, its position, then its type.
            0xC615 => {
                let o = s.operand;
                if o.len() >= 5 && o[0] == 0 && o[1] == 1 {
                    lvl.tab_pos = i16::from_le_bytes([o[2], o[3]]) as i32;
                }
            }
            _ => {}
        }
    }
}

/// The bullet glyph's font, out of a level's character `grpprl`.
fn read_chpx(lvl: &mut ListLevel, grpprl: &[u8]) {
    for s in iter_sprms(grpprl) {
        match s.opcode {
            // sprmCRgFtc0 — index into the font table for the ASCII range.
            0x4A4F => lvl.bullet_font_ftc = s.u16(),
            // sprmCPbiGrf — low bit says the bullet is a picture.
            0x4888 => {
                if let Some(v) = s.u16() {
                    lvl.picture_bullet = v & 0x0001 != 0;
                }
            }
            _ => {}
        }
    }
}

/// Split a marker into literal text and level placeholders.
///
/// `rgbxchNums` gives the 1-based offsets of the placeholders; the placeholder
/// characters themselves are 0x00..=0x08 and their value IS the level index.
/// We scan for those characters (which is what Word itself does) and only use
/// `rgbxchNums` to stop trusting a byte that looks like a placeholder but was
/// not declared as one.
fn split_marker(raw: &str, rgbxch: &[u8; MAX_LEVELS]) -> Vec<LvlPart> {
    let declared: Vec<u8> = rgbxch.iter().copied().take_while(|o| *o != 0).collect();
    let mut parts: Vec<LvlPart> = Vec::new();
    let mut text = String::new();
    for (i, c) in raw.chars().enumerate() {
        let code = c as u32;
        // Offsets in rgbxchNums are 1-based positions in the marker.
        let is_placeholder = code < MAX_LEVELS as u32
            && (declared.is_empty() || declared.contains(&((i + 1) as u8)));
        if is_placeholder {
            if !text.is_empty() {
                parts.push(LvlPart::Text(std::mem::take(&mut text)));
            }
            parts.push(LvlPart::Level(code as u8));
        } else if code >= 0x20 || c == '\t' {
            text.push(c);
        }
        // Anything else is a control character the marker has no use for.
    }
    if !text.is_empty() {
        parts.push(LvlPart::Text(text));
    }
    parts
}

/// One `LFO` (16 bytes): list id, then the count of overridden levels.
fn read_lfo(r: &mut Reader) -> Option<ListOverride> {
    let rec = r.take(LFO_LEN)?;
    let n_lvl = rec[12];
    Some(ListOverride {
        lsid: u32::from_le_bytes([rec[0], rec[1], rec[2], rec[3]]),
        list: None,
        // More than nine levels cannot be honoured; keep the count so the
        // LFOLVL records are still consumed and the stream stays in sync.
        n_lvl,
        overrides: vec![None; MAX_LEVELS],
    })
}

/// One `LFOLVL` (8 bytes) plus, when it overrides the formatting, a full `LVL`.
fn read_lfolvl(r: &mut Reader) -> Option<(u8, LevelOverride)> {
    let head = r.take(LFOLVL_LEN)?;
    let start_at = i32::from_le_bytes([head[0], head[1], head[2], head[3]]);
    let bits = head[4];
    let level = bits & 0x0F;
    let f_start_at = bits & 0x10 != 0;
    let f_format = bits & 0x20 != 0;

    let mut ovr = LevelOverride { start_at: None, level: None };
    if f_format {
        // A full LVL follows and replaces the level. Its `iStartAt` counts only
        // when the override also claims the start value.
        let lvl = read_lvl(r)?;
        if f_start_at {
            ovr.start_at = Some(lvl.start_at);
        }
        ovr.level = Some(Box::new(lvl));
    } else if f_start_at {
        ovr.start_at = Some(start_at);
    }
    Some((level, ovr))
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Where `PlcfLst` and `PlfLfo` live, read out of the FIB of the WordDocument
/// stream: pairs 73 and 74 of `FibRgFcLcb97`.
///
/// The FIB prefix is self-describing (`csw` 16-bit values then `cslw` 32-bit
/// ones), so the pair array is found the same way whatever the Word build;
/// for a WW8 document it always lands on absolute 0x02E2 / 0x02EA, which is
/// exactly where LibreOffice reads them (`WW8Fib::WW8Fib`, ww8scan.cxx:6211).
fn locate(doc: &[u8]) -> Option<((u32, u32), (u32, u32))> {
    use super::fib::{u16_at, u32_at};
    if u16_at(doc, 0) != 0xA5EC || u16_at(doc, 2) < 193 {
        return None;
    }
    let csw = u16_at(doc, 0x20) as usize;
    let base = 0x22 + csw * 2;
    let cslw = u16_at(doc, base) as usize;
    let pairs = base + 2 + cslw * 4 + 2;
    let g = |i: usize| u32_at(doc, pairs + i * 4);
    if pairs + 150 * 4 > doc.len() {
        return None;
    }
    Some(((g(146), g(147)), (g(148), g(149))))
}

/// A `fc`/`lcb` pair as a slice, clamped to the stream.
fn slice(table: &[u8], fc: u32, lcb: u32) -> &[u8] {
    let start = (fc as usize).min(table.len());
    let end = start.saturating_add(lcb as usize).min(table.len());
    &table[start..end]
}

/// A forward-only cursor. Every read is fallible, so a truncated table stops
/// the parse instead of indexing out of bounds.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Reader<'a> {
        Reader { data, pos: 0 }
    }
    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }
    fn consumed(&self) -> usize {
        self.pos
    }
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        if end > self.data.len() {
            return None;
        }
        let s = &self.data[self.pos..end];
        self.pos = end;
        Some(s)
    }
    fn skip(&mut self, n: usize) {
        self.pos = self.pos.saturating_add(n).min(self.data.len());
    }
    fn u16(&mut self) -> Option<u16> {
        let b = self.take(2)?;
        Some(u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Option<u32> {
        let b = self.take(4)?;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nfc_classification_matches_the_docx_vocabulary() {
        assert!(NumFmt::from_nfc(0).is_ordered());
        assert!(NumFmt::from_nfc(1).is_ordered());
        assert!(!NumFmt::from_nfc(23).is_ordered());
        assert!(NumFmt::from_nfc(23).is_bullet());
        assert!(!NumFmt::from_nfc(255).is_ordered());
        assert_eq!(NumFmt::from_nfc(255).ooxml_name(), "none");
        assert_eq!(NumFmt::from_nfc(4).ooxml_name(), "lowerLetter");
        // An exotic format still numbers.
        assert!(NumFmt::from_nfc(45).is_ordered());
    }

    #[test]
    fn marker_placeholders_become_levels() {
        let rgb = [1, 3, 0, 0, 0, 0, 0, 0, 0];
        let parts = split_marker("\u{0}.\u{1})", &rgb);
        assert_eq!(
            parts,
            vec![
                LvlPart::Level(0),
                LvlPart::Text(".".into()),
                LvlPart::Level(1),
                LvlPart::Text(")".into()),
            ]
        );
        let lvl = ListLevel { text: parts, ..Default::default() };
        assert_eq!(lvl.text_ooxml(), "%1.%2)");
        assert_eq!(lvl.included_levels(), 2);
    }

    #[test]
    fn a_bullet_marker_is_left_alone() {
        let rgb = [0u8; MAX_LEVELS];
        let parts = split_marker("\u{f0b7}", &rgb);
        assert_eq!(parts, vec![LvlPart::Text("\u{f0b7}".into())]);
    }

    #[test]
    fn out_of_range_selectors_are_ignored() {
        let t = ListTable::default();
        assert!(t.level(0, 0).is_none()); // no list
        assert!(t.level(2047, 0).is_none()); // Word 6 numbering
        assert!(t.level(1, 0).is_none()); // empty table
        assert!(t.level(u16::MAX, 200).is_none());
    }

    /// A hand-built table stream: one simple bulleted list, one LFO that only
    /// overrides the start value. Guards the record SIZES, which is what
    /// silently corrupts everything when wrong.
    #[test]
    fn a_synthetic_table_round_trips() {
        let mut t: Vec<u8> = Vec::new();
        // PlcfLst: count, then one LSTF (28 bytes) flagged simple.
        t.extend_from_slice(&1u16.to_le_bytes());
        t.extend_from_slice(&0x1122_3344u32.to_le_bytes()); // lsid
        t.extend_from_slice(&0u32.to_le_bytes()); // tplc
        for i in 0..MAX_LEVELS {
            t.extend_from_slice(&(i as u16).to_le_bytes()); // rgistd
        }
        t.push(0x01); // fSimpleList
        t.push(0);
        let lcb_lst = t.len() as u32; // the LVL below sits PAST it, on purpose
        // One LVL: LVLF, grpprlPapx, grpprlChpx, then the marker.
        t.extend_from_slice(&5i32.to_le_bytes()); // iStartAt
        t.push(23); // nfc = bullet
        t.push(0); // jc = left
        t.extend_from_slice(&[0u8; MAX_LEVELS]); // rgbxchNums: none
        t.push(0); // ixchFollow = tab
        t.extend_from_slice(&0i32.to_le_bytes()); // dxaIndentSav
        t.extend_from_slice(&0i32.to_le_bytes()); // unused
        t.push(4); // cbGrpprlChpx
        t.push(8); // cbGrpprlPapx
        t.push(0);
        t.push(0);
        t.extend_from_slice(&[0x0F, 0x84, 0x68, 0x01]); // sprmPDxaLeft 360
        t.extend_from_slice(&[0x11, 0x84, 0x98, 0xFE]); // sprmPDxaLeft1 -360
        t.extend_from_slice(&[0x4F, 0x4A, 0x05, 0x00]); // sprmCRgFtc0 = font 5
        t.extend_from_slice(&1u16.to_le_bytes()); // cch
        t.extend_from_slice(&0xF0B7u16.to_le_bytes()); // the bullet glyph

        // PlfLfo: count, one LFO, then its LFOData.
        let fc_lfo = t.len() as u32;
        t.extend_from_slice(&1u32.to_le_bytes());
        t.extend_from_slice(&0x1122_3344u32.to_le_bytes()); // lsid
        t.extend_from_slice(&[0u8; 8]); // unused
        t.push(1); // clfolvl
        t.extend_from_slice(&[0u8; 3]);
        t.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes()); // LFOData header
        t.extend_from_slice(&7i32.to_le_bytes()); // iStartAt of the override
        t.push(0x10); // ilvl 0, fStartAt
        t.extend_from_slice(&[0u8; 3]);
        let lcb_lfo = t.len() as u32 - fc_lfo;

        let table = ListTable::parse_ranges(&t, (0, lcb_lst), (fc_lfo, lcb_lfo));
        assert!(table.complete, "table lue partiellement");
        assert_eq!(table.lfo_slack, 0, "PlfLfo mal dimensionné");
        assert_eq!(table.lists.len(), 1);
        assert_eq!(table.overrides.len(), 1);
        // A simple list stores exactly one level even though nine are reserved.
        assert!(table.lists[0].simple);
        assert_eq!(table.lists[0].levels.len(), 1);

        let r = table.level(1, 0).expect("niveau introuvable");
        assert!(r.is_bullet() && !r.is_ordered());
        assert_eq!(r.lsid, 0x1122_3344);
        assert_eq!(r.start_at, 7, "start non surchargé par le LFOLVL");
        assert_eq!(r.level.bullet, Some('\u{f0b7}'));
        assert_eq!(r.level.bullet_font_ftc, Some(5));
        assert_eq!(r.level.indent_left, 360);
        assert_eq!(r.level.indent_first, -360);
        // A deeper level on a simple list falls back to the only one defined.
        assert_eq!(table.level(1, 5).map(|r| r.ilvl), Some(0));
        // And a font table turns the index into a name.
        let mut table = table;
        table.resolve_fonts(&["A".into(), "B".into(), "C".into(), "D".into(), "E".into(), "Wingdings".into()]);
        assert_eq!(
            table.lists[0].levels[0].bullet_font.as_deref(),
            Some("Wingdings")
        );
    }

    /// Corpus run: decode both tables on every real `.doc` we have and check
    /// that `PlfLfo` is consumed EXACTLY — a leftover byte means a record size
    /// is wrong, which is the failure mode that silently corrupts everything.
    #[test]
    fn list_tables_decode_on_the_corpus() {
        let Ok(root) = std::env::var("KUBUNO_DOC_CORPUS") else {
            eprintln!("KUBUNO_DOC_CORPUS absent — test ignoré");
            return;
        };
        use std::io::{Cursor, Read};
        let read_stream = |c: &mut cfb::CompoundFile<Cursor<&[u8]>>, n: &str| -> Option<Vec<u8>> {
            let mut s = c.open_stream(n).ok()?;
            let mut b = Vec::new();
            s.read_to_end(&mut b).ok()?;
            Some(b)
        };

        let (mut files, mut with_lists, mut clean) = (0usize, 0usize, 0usize);
        let (mut lists, mut lfos, mut bullets, mut numbered) = (0usize, 0usize, 0usize, 0usize);
        let mut dirty: Vec<String> = Vec::new();

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
                let Ok(mut comp) = cfb::CompoundFile::open(Cursor::new(&bytes[..])) else {
                    continue;
                };
                let Some(doc) = read_stream(&mut comp, "WordDocument") else { continue };
                let Ok(fib) = Fib::parse(&doc) else { continue };
                if fib.encrypted {
                    continue;
                }
                files += 1;
                let table = read_stream(&mut comp, fib.table_stream_name()).unwrap_or_default();
                let t = ListTable::parse_streams(&doc, &table);
                if t.is_empty() {
                    continue;
                }
                with_lists += 1;
                lists += t.lists.len();
                lfos += t.overrides.len();
                for l in &t.lists {
                    for lv in &l.levels {
                        if lv.is_bullet() {
                            bullets += 1;
                        } else {
                            numbered += 1;
                        }
                    }
                }
                // Every record read, and `PlfLfo` ending exactly on its last
                // byte: the two signals that the sizes above are right.
                if t.complete && t.lfo_slack == 0 {
                    clean += 1;
                } else if dirty.len() < 10 {
                    dirty.push(format!(
                        "{} (complet={} reste_lfo={})",
                        p.file_name().unwrap_or_default().to_string_lossy(),
                        t.complete,
                        t.lfo_slack
                    ));
                }
            }
        }
        eprintln!(
            "listes .doc: {with_lists}/{files} documents en ont · {lists} listes · {lfos} LFO · \
             {bullets} niveaux puce / {numbered} numérotés · {clean}/{with_lists} tables lues \
             exactement"
        );
        for d in &dirty {
            eprintln!("  reste des octets: {d}");
        }
        assert!(files > 0, "aucun .doc lisible dans KUBUNO_DOC_CORPUS");
        // A handful of files in any large corpus are damaged; the sizes are
        // wrong if a large share does not land on a record boundary.
        assert!(
            clean * 10 >= with_lists * 9,
            "trop de tables de listes mal alignées: {clean}/{with_lists}"
        );
    }
}
