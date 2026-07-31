//! Page layout: the section table (`PLCFsed`) and its `SEPX` property lists.
//!
//! A `.doc` describes its page setup exactly once per section, in a `SEPX`: a
//! `grpprl` of section sprms stored in the WordDocument stream. The `PLCFsed`
//! of the table stream maps character positions onto those `SEPX` offsets, so
//! reading the layout means walking two structures, not one.
//!
//! The result is deliberately the DOCX `SectionInfo`: the handlers and the
//! multi-page wrapper of the frontend are already wired onto it, and a parallel
//! structure would only mean two conversions to keep in sync.
//!
//! Reference: LibreOffice `sw/source/filter/ww8/ww8par6.cxx`
//! (`wwSectionManager::CreateSep`, `SetLeftRight`, `GetPageULData`,
//! `SwWW8ImplReader::SetPageBorder`), `ww8scan.cxx` (`WW8PLCFx_SEPX`) and
//! [MS-DOC] 2.6.4 / 2.8.26.

use crate::converters::docx::model::{LineNumbering, PageBorder, SectionColumns, SectionInfo};

use super::fib::{u16_at, u32_at, Fib};
use super::sprm::{iter_sprms_ver, Sgc, SprmVersion};

/// Word's own limit on the number of columns of a section (ww8struc.hxx:1004).
const MAX_COLUMNS: usize = 44;

// ── Section sprm identifiers ────────────────────────────────────────────────
// Only the low 9 bits (`ispmd`) are used: they are stable across the Word 6/95
// 8-bit encoding and the Word 97+ 16-bit one, so a single table serves both
// once the legacy opcodes have been translated (`ww6_ispmd`).
const SP_DXA_COL_WIDTH: u16 = 0x03;
const SP_DXA_COL_SPACING: u16 = 0x04;
const SP_F_EVENLY_SPACED: u16 = 0x05;
const SP_BKC: u16 = 0x09;
const SP_F_TITLE_PAGE: u16 = 0x0A;
const SP_CCOLUMNS: u16 = 0x0B;
const SP_DXA_COLUMNS: u16 = 0x0C;
const SP_NFC_PGN: u16 = 0x0E;
const SP_F_PGN_RESTART: u16 = 0x11;
const SP_LNC: u16 = 0x13;
const SP_N_LNN_MOD: u16 = 0x15;
const SP_DXA_LNN: u16 = 0x16;
const SP_DYA_HDR_TOP: u16 = 0x17;
const SP_DYA_HDR_BOTTOM: u16 = 0x18;
const SP_L_BETWEEN: u16 = 0x19;
const SP_VJC: u16 = 0x1A;
const SP_LNN_MIN: u16 = 0x1B;
const SP_PGN_START97: u16 = 0x1C;
const SP_B_ORIENTATION: u16 = 0x1D;
const SP_XA_PAGE: u16 = 0x1F;
const SP_YA_PAGE: u16 = 0x20;
const SP_DXA_LEFT: u16 = 0x21;
const SP_DXA_RIGHT: u16 = 0x22;
const SP_DYA_TOP: u16 = 0x23;
const SP_DYA_BOTTOM: u16 = 0x24;
const SP_DZA_GUTTER: u16 = 0x25;
const SP_F_BIDI: u16 = 0x28;
/// `sprmSBrcTop80`… — the four page borders in their 4-byte (Word 97) form.
const SP_BRC80_TOP: u16 = 0x2B;
const SP_BRC80_RIGHT: u16 = 0x2E;
const SP_PGB_PROP: u16 = 0x2F;
/// `sprmSBrcTop`… — the 8-byte (Word 2000+) form, which overrides the above.
const SP_BRC9_TOP: u16 = 0x34;
const SP_BRC9_RIGHT: u16 = 0x37;
/// `sprmSPgnStart` — the 32-bit replacement of `sprmSPgnStart97`.
const SP_PGN_START: u16 = 0x44;

/// One section of the document: the character range it covers and its layout.
// TODO(mod.rs): drop this allow once `import_doc` returns the real layout.
#[allow(dead_code)]
#[derive(Clone)]
pub(crate) struct DocSection {
    /// First character position of the section.
    pub(crate) cp_start: u32,
    /// One past the last character position.
    pub(crate) cp_end: u32,
    pub(crate) info: SectionInfo,
}

/// Every section of the document, in reading order.
///
/// `doc` is the WordDocument stream (the `SEPX` live there), `table` the table
/// stream (the `PLCFsed` lives there). A document always has at least one
/// section; when the table is missing or unreadable we still return one, filled
/// with Word's own defaults rather than nothing.
// TODO(mod.rs): drop this allow once `import_doc` returns the real layout.
#[allow(dead_code)]
pub(crate) fn parse_sections(fib: &Fib, doc: &[u8], table: &[u8]) -> Vec<DocSection> {
    let start = fib.fc_plcf_sed as usize;
    let end = start
        .saturating_add(fib.lcb_plcf_sed as usize)
        .min(table.len());
    // A PLCF of n+1 CPs (4 bytes each) followed by n SED structs (12 bytes).
    if start >= end || end - start < 4 + 12 {
        return vec![default_section(fib)];
    }
    let plc = &table[start..end];
    let n = (plc.len() - 4) / 16;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let cp_start = u32_at(plc, i * 4);
        let cp_end = u32_at(plc, (i + 1) * 4);
        // SED: fn (2) · fcSepx (4) · fnMpr (2) · fcMpr (4). 0xFFFFFFFF = none.
        let sed = 4 * (n + 1) + i * 12;
        let fc_sepx = u32_at(plc, sed + 2);
        let mut sep = Sep::new(fib);
        if let Some(grpprl) = sepx_grpprl(fib, doc, fc_sepx) {
            sep.apply(fib, grpprl);
        }
        out.push(DocSection { cp_start, cp_end, info: sep.to_info() });
    }
    if out.is_empty() {
        out.push(default_section(fib));
    }
    out
}

/// Page layout of the document as a whole.
///
/// Unlike OOXML — where the layout of the body sits in the LAST `<w:sectPr>` —
/// a `.doc` stores its sections in document order, so the FIRST one is the one
/// that governs the opening pages and is the faithful single-section answer.
// TODO(mod.rs): drop this allow once `import_doc` returns the real layout.
#[allow(dead_code)]
pub(crate) fn parse_section_props(fib: &Fib, doc: &[u8], table: &[u8]) -> SectionInfo {
    parse_sections(fib, doc, table)
        .into_iter()
        .next()
        .map(|s| s.info)
        .unwrap_or_default()
}

/// The whole document as one section, with Word's defaults.
fn default_section(fib: &Fib) -> DocSection {
    DocSection { cp_start: 0, cp_end: fib.ccp_text, info: Sep::new(fib).to_info() }
}

/// The `grpprl` a SED points at, inside the WordDocument stream.
///
/// The `SEPX` is a byte count followed by the sprms. Word 6 and later write a
/// 16-bit count, Word 1/2 an 8-bit one (ww8scan.cxx:3753). Those two only ever
/// reach us if the FIB parser starts accepting their `wIdent`, but the branch
/// costs nothing and the alternative is reading a length that is off by a byte.
fn sepx_grpprl<'a>(fib: &Fib, doc: &'a [u8], fc_sepx: u32) -> Option<&'a [u8]> {
    if fc_sepx == u32::MAX {
        return None; // no SEPX for this section
    }
    let at = fc_sepx as usize;
    // `u16_at` answers 0 past the end of the stream, which folds a truncated
    // header into the "nothing to read" case below.
    let (head, cb) = if fib.nfib < 100 {
        (1usize, *doc.get(at)? as usize)
    } else {
        (2usize, u16_at(doc, at) as usize)
    };
    let from = at.checked_add(head)?;
    let to = from.saturating_add(cb).min(doc.len());
    (from < to).then(|| &doc[from..to])
}

/// The raw `SEP`, in the units Word stores it in: twips and flags.
///
/// Keeping the decode and the conversion apart is what makes the negative
/// margin and the paper-size rules readable — both need the untouched values.
#[derive(Debug, Clone)]
struct Sep {
    xa_page: u32,
    ya_page: u32,
    dxa_left: u32,
    dxa_right: u32,
    dya_top: i32,
    dya_bottom: i32,
    dza_gutter: u32,
    dya_hdr_top: u32,
    dya_hdr_bottom: u32,
    dm_orient_page: u8,
    /// Break code: 0 continuous · 1 new column · 2 new page · 3 even · 4 odd.
    bkc: u8,
    /// Vertical justification: 0 top · 1 centre · 2 justified · 3 bottom.
    vjc: u8,
    /// Page number format: 0 arabic · 1/2 roman · 3/4 letters.
    nfc_pgn: u8,
    f_title_page: bool,
    f_bidi: bool,
    f_pgn_restart: bool,
    pgn_start: u32,
    /// Number of columns minus one.
    ccol_m1: u16,
    dxa_columns: u32,
    f_evenly_spaced: bool,
    f_l_between: bool,
    col_width: [u32; MAX_COLUMNS],
    col_spacing: [u32; MAX_COLUMNS],
    /// Line numbering: interval, distance from the margin, first number, and
    /// when the count restarts.
    n_lnn_mod: u16,
    dxa_lnn: u32,
    lnn_min: u16,
    lnc: u8,
    /// Page borders: top, left, bottom, right.
    brc: [Option<Brc>; 4],
    pgb_prop: u16,
}

impl Sep {
    /// Word's defaults, which depend on the document language: an English
    /// document falls back to 1 inch top/bottom and 1.25 inch left/right,
    /// everything else to 2.5 cm / 2 cm (ww8par6.cxx:1122 and 1139).
    fn new(fib: &Fib) -> Sep {
        let english = (fib.lid & 0xFF) == 0x09;
        let (lr, top, bottom) = if english {
            (1800u32, 1440i32, 1440i32)
        } else {
            (1417u32, 1417i32, 1134i32)
        };
        Sep {
            // Letter, as Word itself defaults (ww8scan.hxx:1879).
            xa_page: 12242,
            ya_page: 15842,
            dxa_left: lr,
            dxa_right: lr,
            dya_top: top,
            dya_bottom: bottom,
            dza_gutter: 0,
            dya_hdr_top: 720,
            dya_hdr_bottom: 720,
            dm_orient_page: 0,
            bkc: 2,
            vjc: 0,
            nfc_pgn: 0,
            f_title_page: false,
            f_bidi: false,
            f_pgn_restart: false,
            pgn_start: 0,
            ccol_m1: 0,
            dxa_columns: 708, // 1.25 cm
            f_evenly_spaced: true,
            f_l_between: false,
            col_width: [0; MAX_COLUMNS],
            col_spacing: [0; MAX_COLUMNS],
            n_lnn_mod: 0,
            dxa_lnn: 0,
            lnn_min: 0,
            lnc: 0,
            brc: [None, None, None, None],
            pgb_prop: 0,
        }
    }

    /// Decode one `grpprl` of section sprms onto this `SEP`.
    fn apply(&mut self, fib: &Fib, grpprl: &[u8]) {
        for (ispmd, op) in section_sprms(fib, grpprl) {
            match ispmd {
                SP_XA_PAGE => set_u32(&mut self.xa_page, op),
                SP_YA_PAGE => set_u32(&mut self.ya_page, op),
                SP_DXA_LEFT => set_u32(&mut self.dxa_left, op),
                SP_DXA_RIGHT => set_u32(&mut self.dxa_right, op),
                SP_DYA_TOP => set_i32(&mut self.dya_top, op),
                SP_DYA_BOTTOM => set_i32(&mut self.dya_bottom, op),
                SP_DZA_GUTTER => set_u32(&mut self.dza_gutter, op),
                SP_DYA_HDR_TOP => set_u32(&mut self.dya_hdr_top, op),
                SP_DYA_HDR_BOTTOM => set_u32(&mut self.dya_hdr_bottom, op),
                SP_B_ORIENTATION => set_u8(&mut self.dm_orient_page, op),
                SP_BKC => set_u8(&mut self.bkc, op),
                SP_VJC => set_u8(&mut self.vjc, op),
                SP_NFC_PGN => set_u8(&mut self.nfc_pgn, op),
                SP_F_TITLE_PAGE => set_flag(&mut self.f_title_page, op),
                SP_F_BIDI => set_flag(&mut self.f_bidi, op),
                SP_F_PGN_RESTART => set_flag(&mut self.f_pgn_restart, op),
                SP_PGN_START97 => set_u32(&mut self.pgn_start, op),
                SP_PGN_START => {
                    if let Some(v) = op_u32(op) {
                        self.pgn_start = v;
                    }
                }
                SP_CCOLUMNS => {
                    if let Some(v) = op_u16(op) {
                        self.ccol_m1 = v.min(MAX_COLUMNS as u16 - 1);
                    }
                }
                SP_DXA_COLUMNS => set_u32(&mut self.dxa_columns, op),
                SP_F_EVENLY_SPACED => set_flag(&mut self.f_evenly_spaced, op),
                SP_L_BETWEEN => set_flag(&mut self.f_l_between, op),
                // Per-column width / gutter: a column index then a 16-bit value.
                SP_DXA_COL_WIDTH | SP_DXA_COL_SPACING if op.len() >= 3 => {
                    let idx = op[0] as usize;
                    let v = u16::from_le_bytes([op[1], op[2]]) as u32;
                    if idx < MAX_COLUMNS {
                        if ispmd == SP_DXA_COL_WIDTH {
                            self.col_width[idx] = v;
                        } else {
                            self.col_spacing[idx] = v;
                        }
                    }
                }
                SP_N_LNN_MOD => {
                    if let Some(v) = op_u16(op) {
                        self.n_lnn_mod = v;
                    }
                }
                SP_DXA_LNN => set_u32(&mut self.dxa_lnn, op),
                SP_LNN_MIN => {
                    if let Some(v) = op_u16(op) {
                        self.lnn_min = v;
                    }
                }
                SP_LNC => set_u8(&mut self.lnc, op),
                SP_PGB_PROP => {
                    if let Some(v) = op_u16(op) {
                        self.pgb_prop = v;
                    }
                }
                // Page borders. The 8-byte form is applied on top of the 4-byte
                // one on purpose: when a document carries both, Word 2000+
                // honours the newer one (ww8par6.cxx:1356).
                SP_BRC80_TOP..=SP_BRC80_RIGHT => {
                    let side = (ispmd - SP_BRC80_TOP) as usize;
                    if let Some(b) = Brc::from_brc80(op) {
                        self.brc[side] = Some(b);
                    }
                }
                SP_BRC9_TOP..=SP_BRC9_RIGHT => {
                    let side = (ispmd - SP_BRC9_TOP) as usize;
                    if let Some(b) = Brc::from_brc9(op) {
                        self.brc[side] = Some(b);
                    }
                }
                _ => {}
            }
        }
    }

    /// Convert onto the shared page-layout model.
    fn to_info(&self) -> SectionInfo {
        let mut s = SectionInfo::default();

        // Page geometry. Word stores the two dimensions as the page is
        // PRINTED, so the orientation follows from them; `sprmSBOrientation`
        // only breaks the tie of a square page.
        let (w, h) = (self.xa_page, self.ya_page);
        s.orientation = if w > h || (w == h && self.dm_orient_page != 0) {
            "landscape".into()
        } else {
            "portrait".into()
        };
        s.paper = paper_of(w, h);

        // A NEGATIVE top/bottom margin means "keep this margin even if the
        // header overlaps it", not a negative margin: LibreOffice takes the
        // absolute value too (`IsFixedHeightHeader`, ww8par.hxx:826, and
        // `GetPageULData`, ww8par6.cxx:648). Same rule as the DOCX reader.
        s.margin_top = twips_px(self.dya_top.unsigned_abs());
        s.margin_bottom = twips_px(self.dya_bottom.unsigned_abs());

        // Some label templates specify left+right wider than the page; Word
        // honours the left margin and pins the right one (`SetLeftRight`,
        // ww8par6.cxx:489). MINLAY = 23 twips.
        let (mut left, mut right) = (self.dxa_left, self.dxa_right);
        if left.saturating_add(right).saturating_add(23) > self.xa_page {
            right = self.xa_page.saturating_sub(left).saturating_sub(23);
            if left.saturating_add(23) > self.xa_page {
                left = 0;
                right = 0;
            }
        }
        s.margin_left = twips_px(left);
        s.margin_right = twips_px(right);
        s.gutter = twips_px(self.dza_gutter);
        s.header_dist = twips_px(self.dya_hdr_top);
        s.footer_dist = twips_px(self.dya_hdr_bottom);

        s.title_pg = self.f_title_page;
        s.bidi = self.f_bidi;
        s.v_align = match self.vjc {
            1 => "center",
            2 => "both",
            3 => "bottom",
            _ => "top",
        }
        .into();
        s.section_start = match self.bkc {
            0 => "continuous",
            3 => "evenPage",
            4 => "oddPage",
            // 1 = new column: our model has no such value, and Word itself
            // degrades it to a page break when the column count changes.
            _ => "nextPage",
        }
        .into();

        s.columns = self.columns();

        // The first page number only means something when the section restarts
        // the numbering (`PageRestartNo`/`PageStartAt`, ww8par.hxx:816).
        if self.f_pgn_restart && self.pgn_start > 0 {
            s.page_num_start = Some(self.pgn_start.min(i32::MAX as u32) as i32);
        }
        s.page_num_fmt = match self.nfc_pgn {
            1 => "roman-upper",
            2 => "roman-lower",
            3 => "alpha-upper",
            4 => "alpha-lower",
            _ => "arabic",
        }
        .into();

        s.line_numbers = self.line_numbering();
        s.page_border = self.page_border();
        s
    }

    /// `sprmSCcolumns` & friends → the shared column model.
    fn columns(&self) -> SectionColumns {
        let mut c = SectionColumns::default();
        let count = (self.ccol_m1 as usize + 1).clamp(1, MAX_COLUMNS);
        c.count = count as u16;
        c.space = twips_px(self.dxa_columns);
        c.sep = self.f_l_between;

        // Individual widths only exist when the columns are NOT evenly spaced,
        // and only when Word actually wrote one per column (ww8par6.cxx:990).
        if !self.f_evenly_spaced && count > 1 {
            let widths: Vec<f64> = self.col_width[..count].iter().map(|w| twips_px(*w)).collect();
            if widths.iter().all(|w| *w > 0.0) {
                c.equal_width = false;
                c.widths = widths;
                // Word stores n-1 gutters; the last column has none, so the
                // section default stands in — as the DOCX reader does too.
                c.spaces = (0..count)
                    .map(|i| {
                        if i + 1 < count {
                            twips_px(self.col_spacing[i])
                        } else {
                            c.space
                        }
                    })
                    .collect();
            }
        }
        c
    }

    /// `sprmSNLnnMod` & friends → line numbering, or None when it is off.
    fn line_numbering(&self) -> Option<LineNumbering> {
        if self.n_lnn_mod == 0 {
            return None; // disabled (`HandleLineNumbering`, ww8par6.cxx:779)
        }
        Some(LineNumbering {
            count_by: self.n_lnn_mod as i32,
            // Word counts from 0, our display from 1 (ww8par6.cxx:819).
            start: self.lnn_min as i32 + 1,
            distance: twips_px(self.dxa_lnn),
            // lnc: 0 restart each page · 1 restart each section · 2 continuous.
            restart: match self.lnc {
                1 => "newSection",
                2 => "continuous",
                _ => "newPage",
            }
            .into(),
        })
    }

    /// `sprmSBrc*` + `sprmSPgbProp` → the page frame.
    ///
    /// Our frontend has ONE style for the four sides, so the first side that
    /// actually draws something wins, in the same order as the DOCX reader.
    fn page_border(&self) -> Option<PageBorder> {
        let b = self.brc.iter().flatten().find(|b| b.draws())?;
        // pgbApplyTo (bits 0-2) and pgbOffsetFrom (bits 5-7), ww8par6.cxx:1197.
        let apply_to = self.pgb_prop & 0x0007;
        let offset_from = (self.pgb_prop & 0x00E0) >> 5;
        Some(PageBorder {
            color: b.color.clone(),
            width: (b.width_twips() as f64 / 15.0).round().max(1.0),
            style: b.style().into(),
            // dptSpace is in points, 0..31 (ww8struc.hxx:327).
            margin: (b.space_pt.min(31) as f64 * 96.0 / 72.0).round(),
            offset_from: if offset_from == 1 { "page" } else { "text" }.into(),
            display: match apply_to {
                1 => "firstPage",
                2 => "notFirstPage",
                _ => "allPages",
            }
            .into(),
        })
    }
}

/// One border code, in the two shapes Word writes it in.
#[derive(Debug, Clone)]
struct Brc {
    /// Line width in eighths of a point.
    line_width: u8,
    /// Border type: 0 none, 1 single, 3 double, 6 dotted, 7 dashed…
    brc_type: u8,
    /// Distance from the text or the page edge, in points.
    space_pt: u8,
    /// Resolved colour, "#rrggbb".
    color: String,
}

impl Brc {
    /// 4-byte form (Word 97): width, type, colour INDEX, spacing bits.
    fn from_brc80(op: &[u8]) -> Option<Brc> {
        if op.len() < 4 {
            return None;
        }
        // 0xFFFF in the first two bytes is the "nil" border (ww8struc.hxx:285).
        if op[0] == 0xFF && op[1] == 0xFF {
            return None;
        }
        Some(Brc {
            line_width: op[0],
            brc_type: op[1],
            space_pt: op[3] & 0x1F,
            color: ico_color(op[2]),
        })
    }

    /// 8-byte form (Word 2000+): a COLORREF then width, type, spacing bits.
    fn from_brc9(op: &[u8]) -> Option<Brc> {
        if op.len() < 8 {
            return None;
        }
        if op[4] == 0xFF && op[5] == 0xFF && op[6] == 0xFF && op[7] == 0xFF {
            return None; // nil border
        }
        // cv is a Windows COLORREF (0x00bbggrr); byte 3 set to 0xFF means
        // "automatic", which every reader draws black (ww8par6.cxx:1470).
        let color = if op[3] == 0xFF {
            "#000000".to_string()
        } else {
            format!("#{:02X}{:02X}{:02X}", op[0], op[1], op[2])
        };
        Some(Brc { line_width: op[4], brc_type: op[5], space_pt: op[6] & 0x1F, color })
    }

    /// A border type of 0 draws nothing (`lcl_IsBorder`, ww8par6.cxx:1494).
    fn draws(&self) -> bool {
        self.brc_type != 0 && self.line_width != 0
    }

    /// Real width in twips. Word does not store the width a compound border
    /// actually occupies, it has to be derived from the type
    /// (`DetermineBorderProperties`, ww8scan.cxx:1366).
    fn width_twips(&self) -> u32 {
        let base = self.line_width as u32 * 20 / 8;
        match self.brc_type {
            // A triple line is five times an ordinary one, except at the two
            // smallest widths.
            10 => match base {
                5 => base * 3,
                10 => base * 9 / 2,
                _ => base * 5,
            },
            20 => base + 45, // wave
            21 => base + 90, // double wave
            _ => base,
        }
    }

    /// Border type → the four styles our page frame can draw
    /// (`ConvertBorderStyleFromWord`, editeng/source/items/borderline.cxx:136).
    fn style(&self) -> &'static str {
        match self.brc_type {
            6 => "dotted",
            7 | 8 | 9 | 22 => "dashed",
            3 | 10 | 11..=19 | 21 | 23 => "double",
            _ => "solid",
        }
    }
}

/// Word's 17-entry colour palette, used by the 4-byte border code
/// (`SwWW8ImplReader::GetCol`, ww8par6.cxx:124). Index 0 is "automatic",
/// which we draw black like every other reader.
fn ico_color(ico: u8) -> String {
    const PALETTE: [&str; 17] = [
        "#000000", "#000000", "#0000FF", "#00FFFF", "#00FF00", "#FF00FF", "#FF0000", "#FFFF00",
        "#FFFFFF", "#000080", "#008080", "#008000", "#800080", "#800000", "#808000", "#808080",
        "#C0C0C0",
    ];
    PALETTE.get(ico as usize).copied().unwrap_or("#000000").to_string()
}

/// Paper format from the (short side, long side) COUPLE.
///
/// Comparing the long side alone is what confused A5 (11906 twips tall) with
/// the WIDTH of A4 in the DOCX reader — this mirrors that fix. LibreOffice
/// snaps each dimension the same way (`sloppyFitPageDimension`, paperinf.cxx:94).
fn paper_of(w: u32, h: u32) -> String {
    let (short, long) = (w.min(h) as f64, w.max(h) as f64);
    let near = |a: f64, b: f64| (short - a).abs() < 300.0 && (long - b).abs() < 300.0;
    if near(16838.0, 23811.0) {
        "a3".into()
    } else if near(12240.0, 20160.0) {
        "legal".into()
    } else if near(12240.0, 15840.0) {
        "letter".into()
    } else if near(8391.0, 11906.0) {
        "a5".into()
    } else {
        "a4".into()
    }
}

/// twips → CSS px (1440 twips per inch, 96 px per inch).
fn twips_px(v: u32) -> f64 {
    // Word ignores anything past the 16-bit range and renders 0, the same
    // guard the DOCX reader applies (ConversionHelper.cxx:423).
    if v >= 32768 {
        return 0.0;
    }
    (v as f64 / 15.0).round()
}

// ── sprm plumbing ───────────────────────────────────────────────────────────

fn op_u8(op: &[u8]) -> Option<u8> {
    op.first().copied()
}
fn op_u16(op: &[u8]) -> Option<u16> {
    (op.len() >= 2).then(|| u16::from_le_bytes([op[0], op[1]]))
}
fn op_u32(op: &[u8]) -> Option<u32> {
    (op.len() >= 4).then(|| u32::from_le_bytes([op[0], op[1], op[2], op[3]]))
}
fn set_u8(dst: &mut u8, op: &[u8]) {
    if let Some(v) = op_u8(op) {
        *dst = v;
    }
}
fn set_flag(dst: &mut bool, op: &[u8]) {
    if let Some(v) = op_u8(op) {
        *dst = v != 0;
    }
}
fn set_u32(dst: &mut u32, op: &[u8]) {
    if let Some(v) = op_u16(op) {
        *dst = v as u32;
    }
}
/// Signed 16-bit operand: only the top and bottom margins use one, and there
/// the sign is meaningful (see `to_info`).
fn set_i32(dst: &mut i32, op: &[u8]) {
    if let Some(v) = op_u16(op) {
        *dst = v as i16 as i32;
    }
}

/// The section sprms of a `grpprl`, as (ispmd, operand) pairs.
///
/// Both sprm encodings are walked by the shared iterator, which knows their
/// operand sizes; all this adds is the translation of the Word 6/95 opcodes
/// onto the Word 97 `ispmd`, so everything downstream sees one set of
/// identifiers.
fn section_sprms<'a>(fib: &Fib, grpprl: &'a [u8]) -> Vec<(u16, &'a [u8])> {
    let ver = SprmVersion::from_fib(fib);
    iter_sprms_ver(grpprl, ver)
        .into_iter()
        .filter(|s| s.sgc_ver(ver) == Sgc::Section)
        .filter_map(|s| {
            let id = if ver == SprmVersion::Ww8 {
                s.ispmd()
            } else {
                ww6_ispmd(s.opcode)?
            };
            Some((id, s.operand))
        })
        .collect()
}

/// Word 6/95 section opcode → the `ispmd` Word 97 gave the same property.
///
/// The two numberings run in parallel but not in step — Word 97 dropped a few
/// opcodes and added others — so the mapping has to be spelled out. Table:
/// sprmids.hxx:181 (`NS_sprm::v6`) versus sprmids.hxx:555.
fn ww6_ispmd(id: u16) -> Option<u16> {
    Some(match id {
        131 => 0x00, // sprmSScnsPgn
        132 => 0x01, // sprmSiHeadingPgn
        136 => SP_DXA_COL_WIDTH,
        137 => SP_DXA_COL_SPACING,
        138 => SP_F_EVENLY_SPACED,
        139 => 0x06, // sprmSFProtected
        140 => 0x07, // sprmSDmBinFirst
        141 => 0x08, // sprmSDmBinOther
        142 => SP_BKC,
        143 => SP_F_TITLE_PAGE,
        144 => SP_CCOLUMNS,
        145 => SP_DXA_COLUMNS,
        146 => 0x0D, // sprmSFAutoPgn
        147 => SP_NFC_PGN,
        148 => 0x0F, // sprmSDyaPgn
        149 => 0x10, // sprmSDxaPgn
        150 => SP_F_PGN_RESTART,
        151 => 0x12, // sprmSFEndnote
        152 => SP_LNC,
        153 => 0x14, // sprmSGprfIhdt
        154 => SP_N_LNN_MOD,
        155 => SP_DXA_LNN,
        156 => SP_DYA_HDR_TOP,
        157 => SP_DYA_HDR_BOTTOM,
        158 => SP_L_BETWEEN,
        159 => SP_VJC,
        160 => SP_LNN_MIN,
        161 => SP_PGN_START97,
        162 => SP_B_ORIENTATION,
        164 => SP_XA_PAGE,
        165 => SP_YA_PAGE,
        166 => SP_DXA_LEFT,
        167 => SP_DXA_RIGHT,
        168 => SP_DYA_TOP,
        169 => SP_DYA_BOTTOM,
        170 => SP_DZA_GUTTER,
        // 133 sprmSOlstAnm, 163 sprmSBCustomize, 171 sprmSDMPaperReq and the
        // two right-to-left opcodes carry nothing we model.
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal document: a FIB whose PLCFsed points at one SED, a SEPX built
    /// from the given sprms, and the two streams they live in.
    fn doc_with(sprms: &[u8], nfib: u16, lid: u16) -> (Fib, Vec<u8>, Vec<u8>) {
        // WordDocument: 512 bytes of padding, then the SEPX at 0x200.
        let mut doc = vec![0u8; 512];
        doc.extend_from_slice(&(sprms.len() as u16).to_le_bytes());
        doc.extend_from_slice(sprms);

        // Table stream: a PLCF of 2 CPs and one 12-byte SED at offset 0.
        let mut table = Vec::new();
        table.extend_from_slice(&0u32.to_le_bytes());
        table.extend_from_slice(&100u32.to_le_bytes());
        table.extend_from_slice(&0u16.to_le_bytes()); // fn
        table.extend_from_slice(&512u32.to_le_bytes()); // fcSepx
        table.extend_from_slice(&0u16.to_le_bytes()); // fnMpr
        table.extend_from_slice(&0u32.to_le_bytes()); // fcMpr

        let fib = Fib {
            nfib,
            lid,
            fc_plcf_sed: 0,
            lcb_plcf_sed: table.len() as u32,
            ccp_text: 100,
            ..Default::default()
        };
        (fib, doc, table)
    }

    /// A 16-bit sprm with a 2-byte operand.
    fn w(opcode: u16, v: u16) -> Vec<u8> {
        let mut b = opcode.to_le_bytes().to_vec();
        b.extend_from_slice(&v.to_le_bytes());
        b
    }
    /// A 16-bit sprm with a 1-byte operand.
    fn b(opcode: u16, v: u8) -> Vec<u8> {
        let mut b = opcode.to_le_bytes().to_vec();
        b.push(v);
        b
    }

    fn parse(sprms: &[u8]) -> SectionInfo {
        let (fib, doc, table) = doc_with(sprms, 193, 0x040C);
        parse_section_props(&fib, &doc, &table)
    }

    #[test]
    fn a5_is_not_mistaken_for_a4() {
        // A5 portrait: its HEIGHT is exactly the WIDTH of A4.
        let mut s = w(0xB01F, 8391);
        s.extend(w(0xB020, 11906));
        let info = parse(&s);
        assert_eq!(info.paper, "a5");
        assert_eq!(info.orientation, "portrait");

        // A5 landscape: the couple is orientation independent.
        let mut s = w(0xB01F, 11906);
        s.extend(w(0xB020, 8391));
        let info = parse(&s);
        assert_eq!(info.paper, "a5");
        assert_eq!(info.orientation, "landscape");

        // A4, A3, letter and legal — letter and legal share their width.
        let mut s = w(0xB01F, 11906);
        s.extend(w(0xB020, 16838));
        assert_eq!(parse(&s).paper, "a4");
        let mut s = w(0xB01F, 16838);
        s.extend(w(0xB020, 23811));
        assert_eq!(parse(&s).paper, "a3");
        let mut s = w(0xB01F, 12240);
        s.extend(w(0xB020, 15840));
        assert_eq!(parse(&s).paper, "letter");
        let mut s = w(0xB01F, 12240);
        s.extend(w(0xB020, 20160));
        assert_eq!(parse(&s).paper, "legal");
    }

    #[test]
    fn negative_top_and_bottom_margins_are_fixed_margins() {
        let mut s = w(0xB01F, 11906);
        s.extend(w(0xB020, 16838));
        s.extend(w(0x9023, (-1134i16) as u16)); // dyaTop
        s.extend(w(0x9024, (-720i16) as u16)); // dyaBottom
        s.extend(w(0xB021, 1440)); // dxaLeft
        s.extend(w(0xB022, 1440)); // dxaRight
        let info = parse(&s);
        assert_eq!(info.margin_top, 76.0); // abs(-1134) twips
        assert_eq!(info.margin_bottom, 48.0);
        assert_eq!(info.margin_left, 96.0);
        assert_eq!(info.margin_right, 96.0);
    }

    #[test]
    fn margins_gutter_and_header_distances() {
        let mut s = w(0xB01F, 11906);
        s.extend(w(0xB020, 16838));
        s.extend(w(0x9023, 1440));
        s.extend(w(0x9024, 1440));
        s.extend(w(0xB021, 1701));
        s.extend(w(0xB022, 1701));
        s.extend(w(0xB025, 720)); // dzaGutter
        s.extend(w(0xB017, 360)); // dyaHdrTop
        s.extend(w(0xB018, 360)); // dyaHdrBottom
        s.extend(b(0x301A, 1)); // vjc = centre
        s.extend(b(0x3009, 0)); // bkc = continuous
        s.extend(b(0x300A, 1)); // fTitlePage
        let info = parse(&s);
        assert_eq!(info.margin_left, 113.0);
        assert_eq!(info.gutter, 48.0);
        assert_eq!(info.header_dist, 24.0);
        assert_eq!(info.footer_dist, 24.0);
        assert_eq!(info.v_align, "center");
        assert_eq!(info.section_start, "continuous");
        assert!(info.title_pg);
        assert!(info.is_custom());
    }

    #[test]
    fn orientation_follows_the_printed_dimensions() {
        // A4 landscape written by Word: swapped dimensions AND the sprm.
        let mut s = w(0xB01F, 16838);
        s.extend(w(0xB020, 11906));
        s.extend(b(0x301D, 2));
        let info = parse(&s);
        assert_eq!(info.orientation, "landscape");
        assert_eq!(info.paper, "a4");
        // A file that only sets the flag but keeps portrait dimensions is
        // printed portrait by Word too — the dimensions win.
        let mut s = w(0xB01F, 11906);
        s.extend(w(0xB020, 16838));
        s.extend(b(0x301D, 2));
        assert_eq!(parse(&s).orientation, "portrait");
    }

    #[test]
    fn columns_page_numbers_and_line_numbering() {
        let mut s = w(0x500B, 2); // ccolM1 = 2 -> 3 columns
        s.extend(w(0x900C, 720)); // dxaColumns
        s.extend(b(0x3019, 1)); // fLBetween -> separator
        s.extend(b(0x300E, 2)); // nfcPgn -> lower roman
        s.extend(b(0x3011, 1)); // fPgnRestart
        s.extend(w(0x501C, 7)); // pgnStart
        s.extend(w(0x5015, 5)); // nLnnMod
        s.extend(w(0x9016, 360)); // dxaLnn
        s.extend(w(0x501B, 4)); // lnnMin
        s.extend(b(0x3013, 1)); // lnc -> restart per section
        let info = parse(&s);
        assert_eq!(info.columns.count, 3);
        assert_eq!(info.columns.space, 48.0);
        assert!(info.columns.sep && info.columns.equal_width);
        assert_eq!(info.page_num_fmt, "roman-lower");
        assert_eq!(info.page_num_start, Some(7));
        let ln = info.line_numbers.expect("lnNumType");
        assert_eq!(ln.count_by, 5);
        assert_eq!(ln.start, 5); // Word 0-based -> our 1-based
        assert_eq!(ln.distance, 24.0);
        assert_eq!(ln.restart, "newSection");
    }

    #[test]
    fn uneven_columns_need_a_width_for_every_column() {
        // fEvenlySpaced = 0 plus one sprmSDxaColWidth per column.
        let mut s = w(0x500B, 1); // 2 columns
        s.extend(b(0x3005, 0)); // fEvenlySpaced = 0
        s.extend([0x03, 0xF2, 0x00, 0xE0, 0x0B]); // col 0 width 3040
        s.extend([0x03, 0xF2, 0x01, 0x70, 0x17]); // col 1 width 6000
        s.extend([0x04, 0xF2, 0x00, 0xD0, 0x02]); // col 0 spacing 720
        let info = parse(&s);
        assert!(!info.columns.equal_width);
        assert_eq!(info.columns.widths, vec![203.0, 400.0]);
        // The last column has no gutter of its own: the section default (708
        // twips, Word's own 1.25 cm) stands in.
        assert_eq!(info.columns.spaces, vec![48.0, 47.0]);

        // A missing width falls back to evenly spread columns.
        let mut s = w(0x500B, 1);
        s.extend(b(0x3005, 0));
        s.extend([0x03, 0xF2, 0x00, 0xE0, 0x0B]);
        let info = parse(&s);
        assert!(info.columns.equal_width && info.columns.widths.is_empty());
    }

    #[test]
    fn page_borders_in_both_encodings() {
        // 4-byte BRC: 6/8 pt double, ico 6 (light red), 24 pt from the page.
        let mut s: Vec<u8> = vec![0x2B, 0x70, 6, 3, 6, 24];
        s.extend(w(0x522F, 0x0021)); // pgbApplyTo = 1 (first page), from page
        let info = parse(&s);
        let brd = info.page_border.expect("pgBorders");
        assert_eq!(brd.style, "double");
        assert_eq!(brd.color, "#FF0000");
        assert_eq!(brd.width, 1.0); // 6/8 pt = 15 twips
        assert_eq!(brd.margin, 32.0); // 24 pt -> px
        assert_eq!(brd.offset_from, "page");
        assert_eq!(brd.display, "firstPage");

        // 8-byte BRC: an explicit COLORREF, dotted, overriding the 4-byte one.
        let mut s: Vec<u8> = vec![0x2B, 0x70, 8, 1, 0, 0];
        s.extend([0x34, 0xD2, 8, 0x12, 0x34, 0x56, 0x00, 16, 6, 12, 0]);
        let brd = parse(&s).page_border.expect("pgBorders");
        assert_eq!(brd.style, "dotted");
        assert_eq!(brd.color, "#123456");
        assert_eq!(brd.width, 3.0); // 16/8 pt = 2 pt = 40 twips
        assert_eq!(brd.margin, 16.0);

        // A nil border draws nothing.
        let s: Vec<u8> = vec![0x2B, 0x70, 0xFF, 0xFF, 0xFF, 0xFF];
        assert!(parse(&s).page_border.is_none());
    }

    #[test]
    fn word6_sprms_are_decoded_too() {
        // Word 6/95: 8-bit opcodes, different numbering, same properties.
        let s: Vec<u8> = vec![
            164, 0x82, 0x2E, // sprmSXaPage = 11906
            165, 0xC6, 0x41, // sprmSYaPage = 16838
            168, 0x50, 0xFA, // sprmSDyaTop = -1456
            166, 0xA0, 0x05, // sprmSDxaLeft = 1440
            162, 1, // sprmSBOrientation
            143, 1, // sprmSFTitlePage
        ];
        let (fib, doc, table) = doc_with(&s, 104, 0x040C);
        let info = parse_section_props(&fib, &doc, &table);
        assert_eq!(info.paper, "a4");
        assert_eq!(info.margin_top, 97.0); // abs(-1456) twips
        assert_eq!(info.margin_left, 96.0);
        assert!(info.title_pg);
    }

    #[test]
    fn a_document_without_a_section_table_gets_words_defaults() {
        let fib = Fib { nfib: 193, lid: 0x040C, ccp_text: 42, ..Default::default() };
        let secs = parse_sections(&fib, &[], &[]);
        assert_eq!(secs.len(), 1);
        assert_eq!(secs[0].cp_end, 42);
        // 2.5 cm / 2 cm, Word's non-English defaults.
        assert_eq!(secs[0].info.margin_top, 94.0);
        assert_eq!(secs[0].info.margin_bottom, 76.0);
        assert_eq!(secs[0].info.paper, "letter");
        // An English document defaults to 1 inch / 1.25 inch instead.
        let fib = Fib { nfib: 193, lid: 0x0409, ..Default::default() };
        let info = parse_section_props(&fib, &[], &[]);
        assert_eq!(info.margin_top, 96.0);
        assert_eq!(info.margin_left, 120.0);
    }

    #[test]
    fn every_section_of_the_document_is_returned() {
        // Two SEDs: A4 portrait then A4 landscape with two columns.
        let mut doc = vec![0u8; 512];
        let s1 = {
            let mut v = w(0xB01F, 11906);
            v.extend(w(0xB020, 16838));
            v
        };
        let s2 = {
            let mut v = w(0xB01F, 16838);
            v.extend(w(0xB020, 11906));
            v.extend(w(0x500B, 1));
            v
        };
        let off1 = doc.len() as u32;
        doc.extend_from_slice(&(s1.len() as u16).to_le_bytes());
        doc.extend_from_slice(&s1);
        let off2 = doc.len() as u32;
        doc.extend_from_slice(&(s2.len() as u16).to_le_bytes());
        doc.extend_from_slice(&s2);

        let mut table = Vec::new();
        for cp in [0u32, 50, 120] {
            table.extend_from_slice(&cp.to_le_bytes());
        }
        for off in [off1, off2] {
            table.extend_from_slice(&0u16.to_le_bytes());
            table.extend_from_slice(&off.to_le_bytes());
            table.extend_from_slice(&0u16.to_le_bytes());
            table.extend_from_slice(&0u32.to_le_bytes());
        }
        let fib = Fib {
            nfib: 193,
            lid: 0x040C,
            fc_plcf_sed: 0,
            lcb_plcf_sed: table.len() as u32,
            ccp_text: 120,
            ..Default::default()
        };
        let secs = parse_sections(&fib, &doc, &table);
        assert_eq!(secs.len(), 2);
        assert_eq!((secs[0].cp_start, secs[0].cp_end), (0, 50));
        assert_eq!(secs[0].info.orientation, "portrait");
        assert_eq!(secs[1].info.orientation, "landscape");
        assert_eq!(secs[1].info.columns.count, 2);
        // The facade keeps the FIRST section: a `.doc` stores them in order.
        assert_eq!(parse_section_props(&fib, &doc, &table).orientation, "portrait");
    }

    #[test]
    fn a_truncated_sepx_never_panics() {
        // fcSepx past the end of the stream.
        let mut table = Vec::new();
        table.extend_from_slice(&0u32.to_le_bytes());
        table.extend_from_slice(&10u32.to_le_bytes());
        table.extend_from_slice(&0u16.to_le_bytes());
        table.extend_from_slice(&99_999u32.to_le_bytes());
        table.extend_from_slice(&0u16.to_le_bytes());
        table.extend_from_slice(&0u32.to_le_bytes());
        let fib = Fib {
            nfib: 193,
            fc_plcf_sed: 0,
            lcb_plcf_sed: table.len() as u32,
            ..Default::default()
        };
        assert_eq!(parse_sections(&fib, &[0u8; 16], &table).len(), 1);

        // A SEPX announcing more bytes than the stream holds.
        let mut doc = vec![0u8; 512];
        doc.extend_from_slice(&9000u16.to_le_bytes());
        doc.extend_from_slice(&w(0xB01F, 11906));
        let (fib, _, table) = doc_with(&[], 193, 0x040C);
        assert_eq!(parse_sections(&fib, &doc, &table).len(), 1);
    }
}
