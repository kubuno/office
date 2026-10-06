//! Tables — a `.doc` does not contain any, so they have to be rebuilt.
//!
//! There is no "table" object in the WW8 format. There is a flat run of
//! paragraphs, some of which are flagged as living inside a table. A cell ends
//! with the character 0x07; the ROW ends with a further 0x07 whose paragraph
//! properties carry `sprmPFTtp` — and it is THAT paragraph, at the very end of
//! the row, that holds the row's geometry (the `TAP`): column boundaries, cell
//! descriptors, borders, shading, height. Nothing can be laid out before the
//! whole row has been read.
//!
//! Word 2000 added nesting: `sprmPItap` gives the depth of each paragraph and
//! the inner row terminator is flagged with `sprmPFInnerTtp` instead.
//!
//! Two conversions carry most of the work:
//!   * the `TAP` stores ABSOLUTE column boundaries in twips, so a width is the
//!     difference of two of them and the first one is usually negative (-108,
//!     the standard half-gap, meaning "flush with the margin");
//!   * horizontal merges (`fFirstMerged`/`fMerged`) and vertical merges
//!     (`fVertRestart`/`fVertMerge`) are per-cell FLAGS, not spans: every merged
//!     cell still owns a 0x07 mark in the text, and collapsing them into
//!     `colspan` / `rowspan` is the reader's job.
//!
//! The emitted nodes use exactly the attribute vocabulary of the DOCX reader
//! (`converters::docx::read::table`) — same names, same units (CSS pixels),
//! same "one table-wide inner border plus per-cell overrides" split — so that a
//! table renders identically whether it came from a `.doc` or a `.docx`.
//!
//! Reference: LibreOffice `sw/source/filter/ww8/ww8par2.cxx` (`WW8TabDesc`,
//! `WW8TabBandDesc::ReadDef`, `SetTabBorders`, `CalcDefaults`) and [MS-DOC] 2.4.3.

// The entry points below are called by the paragraph layer, which lives in its
// own file and is still being written; without that caller they look unused.
#![allow(dead_code)]

use serde_json::{json, Map, Value};

use super::sprm::Sprm;
use crate::converters::types::PmNode;

// ─── Units ───────────────────────────────────────────────────────────────────

/// 1440 twips = 1 inch = 96 CSS px, so 1 px = 15 twips. Rounded to 0.1 px like
/// the DOCX reader: the layout engine works in fractional pixels.
fn tw_px(tw: f64) -> f64 {
    (tw / 15.0 * 10.0).round() / 10.0
}

fn i16_at(b: &[u8], off: usize) -> i16 {
    if off + 2 > b.len() {
        return 0;
    }
    i16::from_le_bytes([b[off], b[off + 1]])
}

fn u16_at(b: &[u8], off: usize) -> u16 {
    if off + 2 > b.len() {
        return 0;
    }
    u16::from_le_bytes([b[off], b[off + 1]])
}

// ─── Walking a grpprl ────────────────────────────────────────────────────────

/// Offset and length of a sprm's operand, from the `spra` field of its opcode.
///
/// These are the rules of `sprm::iter_sprms` with one correction this file
/// cannot do without: for `sprmTDefTable` (0xD608, and its Word 6 twin 0xD606)
/// the two-byte count records the operand length PLUS ONE ([MS-DOC] 2.9.315;
/// LibreOffice `wwSprmParser::GetSprmTailLen`, case `L_VAR2`). Reading it as the
/// length itself shifts every following sprm by a byte — and what follows a
/// table definition is precisely the borders and the shading this file needs.
fn operand_at(opcode: u16, rest: &[u8]) -> Option<(usize, usize)> {
    Some(match (opcode >> 13) & 0x07 {
        0 | 1 => (0, 1),
        2 | 4 | 5 => (0, 2),
        3 => (0, 4),
        7 => (0, 3),
        6 => {
            if opcode == 0xD608 || opcode == 0xD606 {
                let cb = *rest.first()? as usize | ((*rest.get(1)? as usize) << 8);
                (2, cb.saturating_sub(1))
            } else {
                (1, *rest.first()? as usize)
            }
        }
        _ => return None,
    })
}

/// Walk a `grpprl`, yielding each sprm. Stops at the first opcode whose operand
/// does not fit rather than emitting garbage.
fn walk(grpprl: &[u8]) -> Vec<Sprm<'_>> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 2 <= grpprl.len() {
        let opcode = u16::from_le_bytes([grpprl[i], grpprl[i + 1]]);
        if opcode == 0 {
            break;
        }
        let rest = &grpprl[i + 2..];
        let Some((skip, len)) = operand_at(opcode, rest) else { break };
        if skip + len > rest.len() {
            break;
        }
        out.push(Sprm { opcode, operand: &rest[skip..skip + len] });
        i += 2 + skip + len;
    }
    out
}

// ─── Colours ─────────────────────────────────────────────────────────────────

/// Word's 17-entry colour palette (`ico`), used by every pre-Word-2000
/// structure. Index 0 is "automatic", which has no colour of its own.
/// (LibreOffice `SwWW8ImplReader::GetCol`, ww8par6.cxx:122.)
const ICO: [u32; 17] = [
    0x000000, 0x000000, 0x0000ff, 0x00ffff, 0x00ff00, 0xff00ff, 0xff0000, 0xffff00, 0xffffff,
    0x000080, 0x008080, 0x008000, 0x800080, 0x800000, 0x808000, 0x808080, 0xc0c0c0,
];

fn ico_rgb(i: u8) -> Option<u32> {
    match i {
        0 => None,
        n if (n as usize) < ICO.len() => Some(ICO[n as usize]),
        _ => None,
    }
}

/// A Word `COLORREF`: red, green, blue, then a flag byte where 0xFF means
/// "automatic" — no colour at all.
fn colorref(b: &[u8]) -> Option<u32> {
    if b.len() < 4 || b[3] == 0xFF {
        return None;
    }
    Some((u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]))
}

fn hex(rgb: u32) -> String {
    format!("#{:06x}", rgb & 0x00ff_ffff)
}

// ─── Borders ─────────────────────────────────────────────────────────────────

/// A border as the renderer wants it: width in px, dash style, hex colour.
/// "Absent" and "explicitly none" are the same thing in a `.doc` — `brcType` 0 —
/// so both are `None`.
#[derive(Debug, Clone, PartialEq)]
struct Border {
    w: f64,
    s: &'static str,
    c: String,
}

impl Border {
    fn json(&self) -> Value {
        json!({ "w": self.w, "s": self.s, "c": self.c })
    }
}

/// Word knows 27 border types; the engine draws solid, dashed and dotted, so
/// they collapse onto those three — the same three the DOCX reader keeps.
fn brc_style(brc_type: u8) -> Option<&'static str> {
    match brc_type {
        0 | 255 => None,
        6 | 8 | 9 => Some("dotted"),   // dot, dotDash, dotDotDash
        7 | 22 | 23 => Some("dashed"), // dashLargeGap, dashDotStroked, dashSmallGap
        _ => Some("solid"),
    }
}

/// `dptLineWidth` is in eighths of a point, exactly like OOXML's `w:sz`:
/// 4 eighths = half a point = 0.67 px. Kept to 0.01 px, as in the DOCX reader.
fn brc_width(eighths: u8) -> f64 {
    let w = (f64::from(eighths) / 6.0 * 100.0).round() / 100.0;
    if w < 0.5 {
        0.5
    } else {
        w
    }
}

/// A `BRC80`: line width, type, `ico` colour index, spacing flags (4 bytes).
fn brc80(b: &[u8]) -> Option<Border> {
    if b.len() < 4 {
        return None;
    }
    let s = brc_style(b[1])?;
    Some(Border { w: brc_width(b[0]), s, c: hex(ico_rgb(b[2]).unwrap_or(0)) })
}

/// A Word 2000 `BRC`: a real `COLORREF` followed by width and type (8 bytes).
fn brc9(b: &[u8]) -> Option<Border> {
    if b.len() < 8 {
        return None;
    }
    let s = brc_style(b[5])?;
    Some(Border { w: brc_width(b[4]), s, c: hex(colorref(b).unwrap_or(0)) })
}

/// A Word 6/95 `BRC`: everything packed into 16 bits, the width in units of
/// three quarters of a point with two values reserved for dotted and dashed.
fn brc6(b: &[u8]) -> Option<Border> {
    if b.len() < 2 {
        return None;
    }
    let v = u16::from_le_bytes([b[0], b[1]]);
    let line = (v & 0x07) as u8;
    let ty = ((v >> 3) & 0x03) as u8;
    let ico = (((v & 0x00c0) >> 6) | ((v & 0x0700) >> 6)) as u8;
    if ty == 0 && line == 0 {
        return None;
    }
    let s = match line {
        6 => "dotted",
        7 => "dashed",
        _ => "solid",
    };
    // 0.75 pt per unit, i.e. 6 eighths of a point.
    Some(Border { w: brc_width(line.max(1) * 6), s, c: hex(ico_rgb(ico).unwrap_or(0)) })
}

// ─── Shading ─────────────────────────────────────────────────────────────────

/// Word's shading patterns as a foreground weight in per-mille: a `pct25`
/// pattern reads as a quarter of the foreground over three quarters of the
/// background. Table taken from LibreOffice `SwWW8Shade::SetShade`.
const GRAY: [u16; 62] = [
    0, 1000, 50, 100, 200, 250, 300, 400, 500, 600, 700, 750, 800, 900, 333, 333, 333, 333, 333,
    333, 333, 333, 333, 333, 333, 333, 500, 500, 500, 500, 500, 500, 500, 500, 500, 25, 75, 125,
    150, 175, 225, 275, 325, 350, 375, 425, 450, 475, 525, 550, 575, 625, 650, 675, 725, 775, 825,
    850, 875, 925, 950, 975,
];

/// Flatten a Word shading (foreground, background, pattern) into the single
/// solid fill the renderer understands.
fn shade(fore: Option<u32>, back: Option<u32>, ipat: u16) -> Option<String> {
    let idx = if (ipat as usize) < GRAY.len() { ipat as usize } else { 0 };
    let weight = u32::from(GRAY[idx]);
    if weight == 0 {
        // A null brush shows the background through, "automatic" included.
        return back.map(hex);
    }
    let f = fore.unwrap_or(0x00_0000);
    let b = back.unwrap_or(0xff_ffff);
    let mix = |sh: u32| {
        let fc = (f >> sh) & 0xff;
        let bc = (b >> sh) & 0xff;
        (fc * weight + bc * (1000 - weight)) / 1000
    };
    Some(hex((mix(16) << 16) | (mix(8) << 8) | mix(0)))
}

/// A `SHD80`: two colour indices and a pattern packed into 16 bits.
fn shd80(v: u16) -> Option<String> {
    if v == 0 {
        return None;
    }
    shade(ico_rgb((v & 0x1F) as u8), ico_rgb(((v >> 5) & 0x1F) as u8), (v >> 10) & 0x3F)
}

/// A Word 2000 `SHD`: two `COLORREF`s and a 16-bit pattern (10 bytes).
fn shd_new(b: &[u8]) -> Option<String> {
    if b.len() < 10 {
        return None;
    }
    shade(colorref(&b[0..4]), colorref(&b[4..8]), u16_at(b, 8))
}

// ─── Cell descriptor (TC) ────────────────────────────────────────────────────

/// One `TC`: what the `TAP` says about a single cell of the row.
#[derive(Debug, Clone, Default)]
struct Tc {
    /// Head of a horizontal merge.
    first_merged: bool,
    /// Continuation of a horizontal merge — its text is not displayed.
    merged: bool,
    /// Rotated text.
    vertical: bool,
    backward: bool,
    /// Head / continuation of a vertical merge.
    vert_restart: bool,
    vert_merge: bool,
    /// 0 top, 1 centre, 2 bottom.
    valign: u8,
    /// Top, left, bottom, right.
    brc: [Option<Border>; 4],
    shd: Option<String>,
}

/// A Word 97+ `TC80`: a 16-bit flag word, two unused bytes, four `BRC80`.
fn tc80(b: &[u8]) -> Tc {
    let bits = u16_at(b, 0);
    Tc {
        first_merged: bits & 0x0001 != 0,
        merged: bits & 0x0002 != 0,
        vertical: bits & 0x0004 != 0,
        backward: bits & 0x0008 != 0,
        vert_merge: bits & 0x0020 != 0,
        vert_restart: bits & 0x0040 != 0,
        valign: ((bits & 0x0180) >> 7) as u8,
        brc: [
            brc80(b.get(4..8).unwrap_or_default()),
            brc80(b.get(8..12).unwrap_or_default()),
            brc80(b.get(12..16).unwrap_or_default()),
            brc80(b.get(16..20).unwrap_or_default()),
        ],
        shd: None,
    }
}

/// A Word 6/95 `TC`: one flag byte, one spare, four 16-bit borders.
fn tc6(b: &[u8]) -> Tc {
    let bits = *b.first().unwrap_or(&0);
    Tc {
        first_merged: bits & 0x01 != 0,
        merged: bits & 0x02 != 0,
        brc: [
            brc6(b.get(2..4).unwrap_or_default()),
            brc6(b.get(4..6).unwrap_or_default()),
            brc6(b.get(6..8).unwrap_or_default()),
            brc6(b.get(8..10).unwrap_or_default()),
        ],
        ..Tc::default()
    }
}

// ─── Row properties (TAP) ────────────────────────────────────────────────────

/// The `TAP` of one row, as carried by its terminating paragraph.
#[derive(Debug, Clone, Default)]
pub(crate) struct Row {
    /// `rgdxaCenter`: absolute column boundaries in twips, `cells.len() + 1` of
    /// them. The first is frequently negative.
    centers: Vec<i32>,
    cells: Vec<Tc>,
    /// `sprmTDxaGapHalf`: half the gap between two cells, i.e. the horizontal
    /// padding Word applies inside every cell. 108 twips by default.
    gap_half: i32,
    /// `sprmTDxaLeft`: where the row's left edge should really sit.
    dxa_left: Option<i32>,
    /// `sprmTDyaRowHeight`: positive = minimum, negative = exact.
    height: i32,
    cant_split: bool,
    header: bool,
    /// `sprmTFAutofit`.
    autofit: bool,
    /// `sprmTJc`: 0 left, 1 centre, 2 right.
    align: u8,
    /// Table-wide defaults: top, left, bottom, right, insideH, insideV.
    def_brc: [Option<Border>; 6],
    /// `sprmTCellPaddingDefault`, in twips: top, left, bottom, right.
    pad: [Option<i32>; 4],
    /// `sprmTCellSpacingDefault`, in twips.
    spacing: Option<i32>,
}

impl Row {
    fn cols(&self) -> usize {
        self.cells.len()
    }
}

/// Decode `sprmTDefTable`: the cell count, the column boundaries, then one `TC`
/// per cell. A file may store fewer `TC`s than it declares columns; the missing
/// ones stay at their defaults (LibreOffice `WW8TabBandDesc::ReadDef`).
fn read_def(op: &[u8], row: &mut Row) {
    let Some(&n) = op.first() else { return };
    let n = n as usize;
    // 64 columns is Word's own ceiling; beyond that the file is corrupt.
    if n == 0 || n > 64 || op.len() < 1 + 2 * (n + 1) {
        return;
    }
    row.centers = (0..=n).map(|i| i32::from(i16_at(op, 1 + i * 2))).collect();
    let rest = &op[1 + 2 * (n + 1)..];
    // Word 97 writes 20-byte cell descriptors, Word 6/95 10-byte ones; the
    // amount of data left says which, without needing the file version.
    let (size, decode): (usize, fn(&[u8]) -> Tc) =
        if rest.len() >= n * 20 { (20, tc80) } else { (10, tc6) };
    row.cells = (0..n)
        .map(|i| rest.get(i * size..(i + 1) * size).map(decode).unwrap_or_default())
        .collect();
}

/// `sprmTSetBrc` / `sprmTSetBrc80`: override some sides of a range of cells.
fn set_brc(op: &[u8], row: &mut Row, ver9: bool) {
    if op.len() < 3 {
        return;
    }
    let (first, lim, flags) = (op[0] as usize, op[1] as usize, op[2]);
    let brc = if ver9 { brc9(&op[3..]) } else { brc80(&op[3..]) };
    let lim = lim.min(row.cells.len());
    for tc in row.cells.iter_mut().take(lim).skip(first) {
        for (i, bit) in [0x01u8, 0x02, 0x04, 0x08].iter().enumerate() {
            if flags & bit != 0 {
                tc.brc[i] = brc.clone();
            }
        }
    }
}

/// `sprmTCellPaddingDefault` / `sprmTCellSpacingDefault`: a cell range, a set of
/// side bits, a width type and one value. Only `Fts` 3 (twips) is a measure;
/// anything else must be ignored rather than read as one.
fn side_value(op: &[u8]) -> Option<(u8, i32)> {
    if op.len() < 6 || op[3] != 3 {
        return None;
    }
    Some((op[2], i32::from(i16_at(op, 4))))
}

/// Build the `TAP` of a row from the `grpprl` of its terminating paragraph.
///
/// Order matters: `sprmTDefTable` creates the cells, so the sprms that write
/// INTO those cells — borders, shading, per-cell overrides — can only be applied
/// afterwards. LibreOffice defers them for the same reason.
fn read_tap(sprms: &[Sprm<'_>]) -> Row {
    let mut row = Row { gap_half: 108, ..Row::default() };
    for s in sprms {
        if s.opcode == 0xD608 || s.opcode == 0xD606 {
            read_def(s.operand, &mut row);
        }
    }
    let mut shd_old: Option<&[u8]> = None;
    let mut shd_parts: [Option<&[u8]>; 3] = [None; 3];
    let mut borders: Option<(&[u8], bool)> = None;
    let mut set_brcs: Vec<(&[u8], bool)> = Vec::new();

    for s in sprms {
        match s.opcode {
            0x9602 => row.gap_half = i32::from(s.i16().unwrap_or(108)), // sprmTDxaGapHalf
            0x9601 => row.dxa_left = s.i16().map(i32::from),            // sprmTDxaLeft
            0x9407 => row.height = i32::from(s.i16().unwrap_or(0)),     // sprmTDyaRowHeight
            0x3466 | 0x3403 => row.cant_split = s.u8().unwrap_or(1) != 0, // sprmTFCantSplit(90)
            0x3404 => row.header = s.u8().unwrap_or(1) != 0,            // sprmTTableHeader
            0x3615 => row.autofit = s.u8().unwrap_or(0) != 0,           // sprmTFAutofit
            0x5400 => row.align = (s.u16().unwrap_or(0) & 0x03) as u8,  // sprmTJc90
            0xD605 => borders = Some((s.operand, false)),               // sprmTTableBorders80
            0xD613 => borders = Some((s.operand, true)),                // sprmTTableBorders
            0xD620 => set_brcs.push((s.operand, false)),                // sprmTSetBrc80
            0xD62F => set_brcs.push((s.operand, true)),                 // sprmTSetBrc
            0xD609 => shd_old = Some(s.operand),                        // sprmTDefTableShd80
            0xD612 => shd_parts[0] = Some(s.operand),                   // sprmTDefTableShd
            0xD616 => shd_parts[1] = Some(s.operand),                   // …Shd2nd
            0xD60C => shd_parts[2] = Some(s.operand),                   // …Shd3rd
            0xD634 => {
                // sprmTCellPaddingDefault
                if let Some((sides, v)) = side_value(s.operand) {
                    for (i, slot) in row.pad.iter_mut().enumerate() {
                        if sides & (1 << i) != 0 {
                            *slot = Some(v);
                        }
                    }
                }
            }
            0xD633 => {
                // sprmTCellSpacingDefault
                if let Some((_, v)) = side_value(s.operand) {
                    row.spacing = Some(v);
                }
            }
            _ => {}
        }
    }

    if let Some((op, ver9)) = borders {
        let size = if ver9 { 8usize } else { 4usize };
        for (i, slot) in row.def_brc.iter_mut().enumerate() {
            if let Some(b) = op.get(i * size..(i + 1) * size) {
                *slot = if ver9 { brc9(b) } else { brc80(b) };
            }
        }
    }
    for (op, ver9) in set_brcs {
        set_brc(op, &mut row, ver9);
    }
    // Shading: the 80 form is one 16-bit word per cell; the Word 2000 form is
    // ten bytes per cell, split over three sprms covering cells 0, 22 and 44.
    if let Some(op) = shd_old {
        for (i, tc) in row.cells.iter_mut().enumerate() {
            if op.len() >= (i + 1) * 2 {
                tc.shd = shd80(u16_at(op, i * 2));
            }
        }
    }
    for (part, start) in shd_parts.iter().zip([0usize, 22, 44]) {
        let Some(op) = part else { continue };
        for k in 0..op.len() / 10 {
            if let Some(tc) = row.cells.get_mut(start + k) {
                tc.shd = shd_new(&op[k * 10..]);
            }
        }
    }

    // `sprmTDxaLeft` moves the row's left edge: Word keeps the boundaries where
    // they are and shifts them all by the difference, half-gap included
    // (LibreOffice ww8par2.cxx, "#i30298" / "#i40461").
    if let (Some(left), Some(&first)) = (row.dxa_left, row.centers.first()) {
        let delta = left - (first + row.gap_half);
        if delta != 0 {
            for c in &mut row.centers {
                *c += delta;
            }
        }
    }
    row
}

// ─── Paragraph membership ────────────────────────────────────────────────────

/// What a paragraph's `PAPX` says about its place in a table.
///
/// Filled in by the paragraph-property layer and handed back here; this module
/// never reads the property bins itself.
#[derive(Debug, Clone, Default)]
pub(crate) struct ParaTable {
    /// `sprmPFInTable` / `sprmPFInnerTableCell`.
    pub(crate) in_table: bool,
    /// `sprmPFTtp` / `sprmPFInnerTtp` — this paragraph mark terminates a row.
    pub(crate) row_end: bool,
    /// `sprmPItap`: 0 outside a table, 1 in a table, 2+ in a nested one.
    pub(crate) depth: u32,
    /// The row's geometry, present only on a terminating paragraph.
    pub(crate) row: Option<Row>,
}

impl ParaTable {
    /// Decode the table-related properties of one paragraph's `grpprl`.
    pub(crate) fn parse(grpprl: &[u8]) -> ParaTable {
        let sprms = walk(grpprl);
        let mut t = ParaTable::default();
        let mut itap: Option<u32> = None;
        let mut dtap: i64 = 0;
        let mut inner = false;
        for s in &sprms {
            match s.opcode {
                0x2416 => t.in_table = s.u8().unwrap_or(1) != 0, // sprmPFInTable
                0x2417 => t.row_end = s.u8().unwrap_or(1) != 0,  // sprmPFTtp
                // sprmPFInnerTableCell / sprmPFInnerTtp: the same two facts one
                // level down, which is what makes them mean "depth 2".
                0x244B if s.u8().unwrap_or(1) != 0 => {
                    t.in_table = true;
                    inner = true;
                }
                0x244C if s.u8().unwrap_or(1) != 0 => {
                    t.row_end = true;
                    inner = true;
                }
                0x6649 => itap = s.u32(),                                 // sprmPItap
                0x664A => dtap = i64::from(s.u32().unwrap_or(0) as i32),  // sprmPDtap
                _ => {}
            }
        }
        let mut depth = itap.map_or(u32::from(t.in_table), |v| v.min(16));
        depth = (i64::from(depth) + dtap).clamp(0, 16) as u32;
        // An "inner" flag with no depth of its own can only mean the second
        // level: telling the two apart is the whole reason those sprms exist.
        if inner && depth < 2 {
            depth = 2;
        }
        if t.in_table && depth == 0 {
            depth = 1;
        }
        t.depth = depth;
        if t.row_end {
            t.row = Some(read_tap(&sprms));
        }
        t
    }
}

// ─── Assembly ────────────────────────────────────────────────────────────────

/// One rendered paragraph, ready to be placed.
///
/// `cell_end` mirrors `text::RawParagraph::in_table`: the paragraph mark was a
/// 0x07 cell mark rather than a 0x0D paragraph mark.
pub(crate) struct Para {
    pub(crate) node: PmNode,
    pub(crate) cell_end: bool,
    pub(crate) table: ParaTable,
}

fn depth_of(p: &Para) -> u32 {
    if p.table.depth > 0 {
        p.table.depth
    } else if p.table.in_table || p.cell_end {
        1
    } else {
        0
    }
}

/// Fold a flat paragraph stream into blocks, turning every run of table
/// paragraphs into a `table` node.
///
/// Two tables in a row are separated by a paragraph outside any table — Word
/// inserts one itself — so a maximal run of table paragraphs is one table.
pub(crate) fn assemble(paras: &[Para]) -> Vec<PmNode> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < paras.len() {
        if depth_of(&paras[i]) == 0 {
            out.push(paras[i].node.clone());
            i += 1;
            continue;
        }
        let mut j = i;
        while j < paras.len() && depth_of(&paras[j]) > 0 {
            j += 1;
        }
        // Word always terminates a row; a run with no terminator at all is not
        // a table but a stray 0x07 — Word 6/95 files and text produced by other
        // writers hit this, and inventing a table there is worse than no table.
        if paras[i..j].iter().any(|p| p.table.row_end) {
            out.push(build_table(&paras[i..j], 1));
        } else {
            out.extend(paras[i..j].iter().map(|p| p.node.clone()));
        }
        i = j;
    }
    out
}

/// Content of one cell plus the column range it occupies, in twips.
struct Cell {
    content: Vec<PmNode>,
    from: i32,
    to: i32,
    /// Continuation of a vertical merge: absorbed by the cell above, never
    /// emitted, but its column range is what gives that cell its `rowspan`.
    cont: bool,
    tc: Tc,
    /// Index of the `TC` this cell starts at, needed for the border defaults.
    idx: usize,
    last: bool,
}

struct BuiltRow {
    cells: Vec<Cell>,
    tap: Row,
}

/// Rebuild one table from the paragraphs of a single nesting level.
fn build_table(paras: &[Para], level: u32) -> PmNode {
    // A broken itap chain could skip a level; clamping to the shallowest
    // paragraph actually present keeps the row marks recognisable.
    let level = paras.iter().map(depth_of).min().unwrap_or(level).max(1);

    // ── Pass 1: rows and cells, following the 0x07 marks ─────────────────────
    let mut raw_rows: Vec<(Vec<Vec<PmNode>>, Option<Row>)> = Vec::new();
    let mut cells: Vec<Vec<PmNode>> = Vec::new();
    let mut buf: Vec<PmNode> = Vec::new();
    let mut i = 0usize;
    while i < paras.len() {
        if depth_of(&paras[i]) > level {
            let mut j = i;
            while j < paras.len() && depth_of(&paras[j]) > level {
                j += 1;
            }
            buf.push(build_table(&paras[i..j], level + 1));
            i = j;
            continue;
        }
        let p = &paras[i];
        if p.table.row_end {
            // The row mark carries no text of its own, only the geometry.
            if !buf.is_empty() {
                cells.push(std::mem::take(&mut buf));
            }
            raw_rows.push((std::mem::take(&mut cells), p.table.row.clone()));
        } else {
            buf.push(p.node.clone());
            if p.cell_end {
                cells.push(std::mem::take(&mut buf));
            }
        }
        i += 1;
    }
    // Text after the last row mark is a truncated row, not lost content.
    if !buf.is_empty() {
        cells.push(buf);
    }
    if !cells.is_empty() {
        raw_rows.push((cells, None));
    }

    // ── Pass 2: geometry ─────────────────────────────────────────────────────
    let mut rows: Vec<BuiltRow> = Vec::new();
    for (contents, tap) in raw_rows {
        let mut tap = tap.unwrap_or_default();
        if tap.centers.len() < 2 && !contents.is_empty() {
            // No usable TAP: equal columns, so the row at least reads.
            let n = contents.len().max(1);
            tap.centers = (0..=n).map(|k| (k as i32) * 1440).collect();
            tap.cells = vec![Tc::default(); n];
            tap.gap_half = 108;
        }
        rows.push(BuiltRow { cells: place_cells(&tap, contents), tap });
    }
    rows.retain(|r| !r.cells.is_empty());
    if rows.is_empty() {
        return empty_table();
    }

    let grid = column_grid(&rows);
    if grid.len() < 2 {
        return empty_table();
    }
    emit(rows, &grid)
}

/// Map a row's cell contents onto its `TAP`, collapsing horizontal merges and
/// marking vertical-merge continuations.
fn place_cells(tap: &Row, mut contents: Vec<Vec<PmNode>>) -> Vec<Cell> {
    let n = tap.cols();
    if n == 0 {
        return Vec::new();
    }
    // More 0x07 marks than cell descriptors: the TAP owns the geometry, so the
    // surplus text joins the last cell rather than vanishing.
    if contents.len() > n {
        let extra: Vec<PmNode> = contents.split_off(n).into_iter().flatten().collect();
        if let Some(last) = contents.last_mut() {
            last.extend(extra);
        }
    }
    let mut out: Vec<Cell> = Vec::new();
    for k in 0..n {
        let tc = tap.cells[k].clone();
        let from = tap.centers[k];
        let to = tap.centers[k + 1];
        let content = contents.get(k).cloned().unwrap_or_default();
        // A zero-width cell does not exist for Word either (`bExist`).
        if to <= from && !tc.merged {
            continue;
        }
        if tc.merged && !tc.first_merged {
            // Continuation of a horizontal merge: Word does not show its text,
            // it only widens the cell that started the merge.
            if let Some(prev) = out.last_mut() {
                prev.to = to;
                prev.last = k + 1 == n;
                continue;
            }
        }
        let cont = tc.vert_merge && !tc.vert_restart;
        out.push(Cell { content, from, to, cont, tc, idx: k, last: k + 1 == n });
    }
    out
}

/// The table-wide column grid: every boundary any row declares, in order.
///
/// Rows are free to disagree — that is exactly how Word expresses a merged or a
/// short row — so the union of their boundaries IS the column model, and each
/// cell then covers a run of columns. The boundaries come from the `TAP`, not
/// from the merged cell ranges: a column that every row happens to merge away
/// still exists, and Word (like the DOCX grid) keeps it.
fn column_grid(rows: &[BuiltRow]) -> Vec<i32> {
    let mut b: Vec<i32> = Vec::new();
    for r in rows {
        b.extend_from_slice(&r.tap.centers);
        for c in &r.cells {
            b.push(c.from);
            b.push(c.to);
        }
    }
    b.sort_unstable();
    b.dedup();
    // Boundaries less than a minimum cell width apart are one edge rounded
    // twice; keeping both would mint invisible sliver columns. 23 twips is the
    // threshold Writer itself uses (`MINLAY`).
    let mut out: Vec<i32> = Vec::new();
    for v in b {
        if out.last().is_none_or(|l| v - l >= 23) {
            out.push(v);
        }
    }
    out
}

fn grid_index(grid: &[i32], pos: i32) -> usize {
    let mut best = 0usize;
    let mut dist = i32::MAX;
    for (i, &g) in grid.iter().enumerate() {
        let d = (g - pos).abs();
        if d < dist {
            dist = d;
            best = i;
        }
    }
    best
}

fn empty_table() -> PmNode {
    PmNode {
        node_type: "table".into(),
        attrs: None,
        content: Some(vec![PmNode {
            node_type: "tableRow".into(),
            attrs: None,
            content: Some(vec![]),
            marks: None,
            text: None,
        }]),
        marks: None,
        text: None,
    }
}

/// A borderless cell filling a gap at the start or the end of a short row, so
/// that every other row keeps its column indices — the same device the DOCX
/// reader uses for `w:gridBefore` / `w:gridAfter`.
fn spacer(span: usize) -> PmNode {
    PmNode {
        node_type: "tableCell".into(),
        attrs: Some(json!({
            "colspan": span,
            "rowspan": 1,
            "cellBorders": { "t": null, "b": null, "l": null, "r": null },
        })),
        content: Some(vec![PmNode::paragraph(vec![])]),
        marks: None,
        text: None,
    }
}

/// One cell of the emitted grid, kept alongside its column span so the vertical
/// merges can be resolved once every row is known.
struct Placed {
    col: usize,
    span: usize,
    cont: bool,
    node: Option<PmNode>,
    /// Effective borders: top, left, bottom, right.
    brc: [Option<Border>; 4],
}

/// Turn the rows into ProseMirror nodes.
fn emit(rows: Vec<BuiltRow>, grid: &[i32]) -> PmNode {
    let n_cols = grid.len() - 1;
    let n_rows = rows.len();

    // ── Column ranges and effective borders ──────────────────────────────────
    let mut placed: Vec<Vec<Placed>> = Vec::with_capacity(n_rows);
    for (r, row) in rows.iter().enumerate() {
        let mut line: Vec<Placed> = Vec::new();
        for cell in &row.cells {
            let col = grid_index(grid, cell.from);
            let end = grid_index(grid, cell.to);
            let span = end.saturating_sub(col).max(1);
            // Word stores the grid on the cells: a side the cell leaves unset
            // falls back to a table default, and WHICH default depends on where
            // the cell sits (ww8par2.cxx, "3. pass: replace border with
            // defaults if needed").
            let mut brc = cell.tc.brc.clone();
            for (side, slot) in brc.iter_mut().enumerate() {
                if slot.is_some() {
                    continue;
                }
                let j = match side {
                    0 => if r == 0 { 0 } else { 4 },
                    1 => if cell.idx == 0 { 1 } else { 5 },
                    2 => if r + 1 == n_rows { 2 } else { 4 },
                    _ => if cell.last { 3 } else { 5 },
                };
                *slot = row.tap.def_brc[j].clone();
            }
            line.push(Placed { col, span, cont: cell.cont, node: None, brc });
        }
        placed.push(line);
    }

    // ── The one table-wide inner border ──────────────────────────────────────
    // The renderer knows a single default for every inner edge plus per-cell
    // overrides, so the most frequent inner border becomes that default and
    // only the cells that disagree carry their own.
    let mut tally: Vec<(Option<Border>, usize)> = Vec::new();
    for (r, line) in placed.iter().enumerate() {
        for p in line.iter().filter(|p| !p.cont) {
            for (side, on_edge) in [
                (0usize, r == 0),
                (1, p.col == 0),
                (2, r + 1 == n_rows),
                (3, p.col + p.span >= n_cols),
            ] {
                if on_edge {
                    continue;
                }
                match tally.iter_mut().find(|(b, _)| *b == p.brc[side]) {
                    Some((_, n)) => *n += 1,
                    None => tally.push((p.brc[side].clone(), 1)),
                }
            }
        }
    }
    let inner = tally.into_iter().max_by_key(|(_, n)| *n).map(|(b, _)| b).unwrap_or(None);

    // ── Cell nodes ───────────────────────────────────────────────────────────
    for (line, row) in placed.iter_mut().zip(rows.iter()) {
        for (k, p) in line.iter_mut().enumerate() {
            if p.cont {
                continue;
            }
            let cell = &row.cells[k];
            let mut attrs = json!({ "colspan": p.span, "rowspan": 1 });
            if let Some(bg) = &cell.tc.shd {
                attrs["cellBg"] = json!(bg);
            }
            match cell.tc.valign {
                1 => attrs["cellVAlign"] = json!("center"),
                2 => attrs["cellVAlign"] = json!("bottom"),
                _ => {}
            }
            if cell.tc.vertical {
                attrs["cellDir"] = json!(if cell.tc.backward { 270 } else { 90 });
            }
            // Only the sides that disagree with the table default are written
            // out: the renderer resolves each edge once, treating an absent
            // side as "inherit the default" and an explicit `null` as a veto.
            let mut cb = Map::new();
            for (side, key) in [(0usize, "t"), (1, "l"), (2, "b"), (3, "r")] {
                if p.brc[side] != inner {
                    cb.insert(key.into(), p.brc[side].as_ref().map_or(Value::Null, Border::json));
                }
            }
            if !cb.is_empty() {
                attrs["cellBorders"] = Value::Object(cb);
            }
            let mut content = cell.content.clone();
            if content.is_empty() {
                content.push(PmNode::paragraph(vec![]));
            }
            p.node = Some(PmNode {
                node_type: "tableCell".into(),
                attrs: Some(attrs),
                content: Some(content),
                marks: None,
                text: None,
            });
        }
    }

    // ── Vertical merges → rowspan ────────────────────────────────────────────
    // A cell's rowspan is 1 plus the number of following rows holding a
    // continuation whose column range covers it.
    for r in 0..n_rows {
        for k in 0..placed[r].len() {
            if placed[r][k].cont {
                continue;
            }
            let c = placed[r][k].col;
            let mut span = 1usize;
            let mut rr = r + 1;
            while rr < n_rows
                && placed[rr].iter().any(|p| p.cont && p.col <= c && c < p.col + p.span)
            {
                span += 1;
                rr += 1;
            }
            if span > 1 {
                if let Some(node) = placed[r][k].node.as_mut() {
                    if let Some(a) = node.attrs.as_mut() {
                        a["rowspan"] = json!(span);
                    }
                }
            }
        }
    }

    // ── Rows ─────────────────────────────────────────────────────────────────
    let mut out_rows: Vec<PmNode> = Vec::new();
    let mut heights: Vec<f64> = Vec::new();
    let mut modes: Vec<&'static str> = Vec::new();
    let mut cant: Vec<bool> = Vec::new();
    let mut header_rows = 0u32;
    let mut header_open = true;

    for (line, row) in placed.into_iter().zip(rows.iter()) {
        let mut nodes: Vec<PmNode> = Vec::new();
        let mut next = 0usize;
        for p in line {
            if p.col > next {
                nodes.push(spacer(p.col - next));
            }
            next = p.col + p.span;
            if let Some(n) = p.node {
                nodes.push(n);
            }
        }
        if nodes.is_empty() {
            continue;
        }
        if next < n_cols {
            nodes.push(spacer(n_cols - next));
        }
        out_rows.push(PmNode {
            node_type: "tableRow".into(),
            attrs: None,
            content: Some(nodes),
            marks: None,
            text: None,
        });
        // A negative row height is Word's way of saying "exactly this tall".
        let h = row.tap.height;
        heights.push(tw_px(f64::from(h.abs())));
        modes.push(if h < 0 { "exactly" } else { "atleast" });
        cant.push(row.tap.cant_split);
        // Only the unbroken run of header rows at the top repeats.
        if header_open && row.tap.header && header_rows < 10 {
            header_rows += 1;
        } else {
            header_open = false;
        }
    }
    if out_rows.is_empty() {
        return empty_table();
    }

    // ── Table attributes ─────────────────────────────────────────────────────
    let first = &rows[0].tap;
    let mut attrs: Map<String, Value> = Map::new();
    let widths: Vec<f64> =
        grid.windows(2).map(|w| tw_px(f64::from(w[1] - w[0])).max(8.0)).collect();
    attrs.insert("colWidths".into(), json!(widths));
    // rgdxaCenter is measured from the text margin and already carries the
    // half-gap, so a table flush with the margin starts at -108, not 0.
    let indent = tw_px(f64::from(grid[0] + first.gap_half));
    if indent > 0.0 {
        attrs.insert("tableIndent".into(), json!(indent));
    }
    match first.align {
        1 => {
            attrs.insert("tableAlign".into(), json!("center"));
        }
        2 => {
            attrs.insert("tableAlign".into(), json!("right"));
        }
        _ => {}
    }
    // Word 97 columns are absolute: unless the row asks for AutoFit, the widths
    // above are the layout.
    if !first.autofit {
        attrs.insert("tableLayout".into(), json!("fixed"));
    }
    // `dxaGapHalf` is the default horizontal padding of every cell; the vertical
    // one only exists from Word 2000 (`sprmTCellPaddingDefault`).
    for (key, twips) in [
        ("cellMarginTop", first.pad[0]),
        ("cellMarginLeft", first.pad[1].or(Some(first.gap_half))),
        ("cellMarginBottom", first.pad[2]),
        ("cellMarginRight", first.pad[3].or(Some(first.gap_half))),
    ] {
        if let Some(v) =
            twips.map(|t| tw_px(f64::from(t))).filter(|v| (0.0..=200.0).contains(v))
        {
            attrs.insert(key.into(), json!(v));
        }
    }
    // Cell spacing applies on both sides of an edge, so the visible gap is twice
    // the stored value — the same doubling the DOCX reader does.
    if let Some(sp) = first.spacing.filter(|v| *v > 0) {
        attrs.insert(
            "cellSpacing".into(),
            json!((tw_px(f64::from(sp)) * 2.0 * 10.0).round() / 10.0),
        );
    }
    match &inner {
        Some(b) => {
            attrs.insert("tableBorderWidth".into(), json!(b.w));
            attrs.insert("tableBorderStyle".into(), json!(b.s));
            attrs.insert("tableBorderColor".into(), json!(b.c));
        }
        // No inner border at all: 'plain' switches off the renderer's own
        // default so only the explicit per-cell borders get drawn.
        None => {
            attrs.insert("tableStyle".into(), json!("plain"));
        }
    }
    if heights.iter().any(|h| *h > 0.0) {
        attrs.insert("rowHeights".into(), json!(heights));
        if modes.contains(&"exactly") {
            attrs.insert("rowHeightModes".into(), json!(modes));
        }
    }
    if cant.iter().any(|b| *b) {
        attrs.insert("rowCantSplit".into(), json!(cant));
    }
    if header_rows > 0 {
        attrs.insert("headerRows".into(), json!(header_rows));
        attrs.insert("headerRepeat".into(), json!(true));
    }

    PmNode {
        node_type: "table".into(),
        attrs: Some(Value::Object(attrs)),
        content: Some(out_rows),
        marks: None,
        text: None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Synthetic grpprls ────────────────────────────────────────────────────

    /// Assemble a `sprmTDefTable` operand: cell count, boundaries, `TC80`s.
    /// The two-byte count is the operand length PLUS ONE — the very rule this
    /// module exists to get right.
    fn def_table(centers: &[i16], tcs: &[[u8; 20]]) -> Vec<u8> {
        let mut op = vec![tcs.len() as u8];
        for c in centers {
            op.extend_from_slice(&c.to_le_bytes());
        }
        for t in tcs {
            op.extend_from_slice(t);
        }
        let mut out = vec![0x08, 0xD6];
        out.extend_from_slice(&((op.len() as u16 + 1).to_le_bytes()));
        out.extend_from_slice(&op);
        out
    }

    /// A plain `TC80` with the given flag word and no borders.
    fn tc(flags: u16) -> [u8; 20] {
        let mut t = [0u8; 20];
        t[0..2].copy_from_slice(&flags.to_le_bytes());
        t
    }

    /// A `TC80` whose four sides carry a single hairline border.
    fn tc_boxed(flags: u16) -> [u8; 20] {
        let mut t = tc(flags);
        for side in 0..4 {
            t[4 + side * 4] = 6; // dptLineWidth = 6/8 pt
            t[5 + side * 4] = 1; // brcType = single
        }
        t
    }

    fn row_end_grpprl(extra: &[u8]) -> Vec<u8> {
        let mut g = vec![0x16, 0x24, 1, 0x17, 0x24, 1];
        g.extend_from_slice(extra);
        g
    }

    fn para(text: &str, cell_end: bool, grpprl: &[u8]) -> Para {
        Para {
            node: PmNode::paragraph(vec![PmNode::text(text, vec![])]),
            cell_end,
            table: ParaTable::parse(grpprl),
        }
    }

    /// A cell: one paragraph ending on a 0x07 mark.
    fn cell(text: &str, depth: u8) -> Para {
        let mut g = vec![0x16, 0x24, 1];
        if depth > 1 {
            g.extend_from_slice(&[0x49, 0x66, depth, 0, 0, 0, 0x4B, 0x24, 1]);
        }
        para(text, true, &g)
    }

    fn attr<'a>(n: &'a PmNode, k: &str) -> Option<&'a Value> {
        n.attrs.as_ref()?.get(k)
    }

    fn text_of(n: &PmNode) -> String {
        let mut s = n.text.clone().unwrap_or_default();
        for c in n.children() {
            s.push_str(&text_of(c));
        }
        s
    }

    // ── Geometry ─────────────────────────────────────────────────────────────

    #[test]
    fn widths_are_differences_of_absolute_boundaries() {
        // Flush with the margin (-108), then two columns of 1440 and 2880 twips.
        let def = def_table(&[-108, 1332, 4212], &[tc(0), tc(0)]);
        let t = build_table(
            &[cell("a", 1), cell("b", 1), para("", true, &row_end_grpprl(&def))],
            1,
        );
        assert_eq!(attr(&t, "colWidths"), Some(&json!([96.0, 192.0])));
        // -108 + gapHalf 108 = flush: no indent at all.
        assert_eq!(attr(&t, "tableIndent"), None);
        assert_eq!(attr(&t, "tableLayout"), Some(&json!("fixed")));
        // dxaGapHalf is Word's default cell padding.
        assert_eq!(attr(&t, "cellMarginLeft"), Some(&json!(7.2)));
        assert_eq!(attr(&t, "cellMarginRight"), Some(&json!(7.2)));
        let rows = t.children();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].children().len(), 2);
        assert_eq!(text_of(&rows[0].children()[0]), "a");
    }

    #[test]
    fn a_negative_first_boundary_does_not_become_an_indent() {
        // A table hanging into the left margin: -828 + 108 = -720.
        let def = def_table(&[-828, 612], &[tc(0)]);
        let t = build_table(&[cell("x", 1), para("", true, &row_end_grpprl(&def))], 1);
        assert_eq!(attr(&t, "tableIndent"), None);
        assert_eq!(attr(&t, "colWidths"), Some(&json!([96.0])));
    }

    #[test]
    fn an_indented_table_keeps_its_offset() {
        let def = def_table(&[612, 2052], &[tc(0)]);
        let t = build_table(&[cell("x", 1), para("", true, &row_end_grpprl(&def))], 1);
        // 612 + 108 = 720 twips = 48 px.
        assert_eq!(attr(&t, "tableIndent"), Some(&json!(48.0)));
    }

    // ── Merges ───────────────────────────────────────────────────────────────

    #[test]
    fn horizontal_merge_flags_become_a_colspan() {
        // Three columns, the second and third merged into the first.
        let def = def_table(
            &[0, 1440, 2880, 4320],
            &[tc(0x0001), tc(0x0002), tc(0x0002)],
        );
        let t = build_table(
            &[cell("wide", 1), cell("gone", 1), cell("gone", 1), para("", true, &row_end_grpprl(&def))],
            1,
        );
        let cells = t.children()[0].children();
        assert_eq!(cells.len(), 1);
        assert_eq!(attr(&cells[0], "colspan"), Some(&json!(3)));
        // The text of a merge continuation is not displayed by Word either.
        assert_eq!(text_of(&cells[0]), "wide");
    }

    #[test]
    fn vertical_merge_flags_become_a_rowspan() {
        let head = def_table(&[0, 1440, 2880], &[tc(0x0060), tc(0)]); // fVertMerge|fVertRestart
        let cont = def_table(&[0, 1440, 2880], &[tc(0x0020), tc(0)]); // fVertMerge only
        let paras = vec![
            cell("tall", 1),
            cell("r1", 1),
            para("", true, &row_end_grpprl(&head)),
            cell("hidden", 1),
            cell("r2", 1),
            para("", true, &row_end_grpprl(&cont)),
            cell("hidden", 1),
            cell("r3", 1),
            para("", true, &row_end_grpprl(&cont)),
        ];
        let t = build_table(&paras, 1);
        let rows = t.children();
        assert_eq!(rows.len(), 3);
        assert_eq!(attr(&rows[0].children()[0], "rowspan"), Some(&json!(3)));
        // Continuations are absorbed: the next rows hold one visible cell each.
        assert_eq!(rows[1].children().len(), 1);
        assert_eq!(text_of(&rows[1].children()[0]), "r2");
    }

    #[test]
    fn a_short_row_is_padded_with_a_borderless_spacer() {
        let wide = def_table(&[0, 1440, 2880], &[tc(0), tc(0)]);
        let short = def_table(&[1440, 2880], &[tc(0)]);
        let paras = vec![
            cell("a", 1),
            cell("b", 1),
            para("", true, &row_end_grpprl(&wide)),
            cell("c", 1),
            para("", true, &row_end_grpprl(&short)),
        ];
        let t = build_table(&paras, 1);
        let row1 = t.children()[1].children();
        assert_eq!(row1.len(), 2);
        assert_eq!(attr(&row1[0], "cellBorders").and_then(|b| b.get("l")), Some(&Value::Null));
        assert_eq!(text_of(&row1[1]), "c");
    }

    // ── Row properties ───────────────────────────────────────────────────────

    #[test]
    fn height_header_and_cant_split_reach_the_table_attributes() {
        let def = def_table(&[0, 1440], &[tc(0)]);
        // sprmTDyaRowHeight = -600 (exact), sprmTTableHeader, sprmTFCantSplit.
        let mut extra = vec![0x07, 0x94];
        extra.extend_from_slice(&(-600i16).to_le_bytes());
        extra.extend_from_slice(&[0x04, 0x34, 1, 0x66, 0x34, 1]);
        extra.extend_from_slice(&def);
        let paras = vec![
            cell("h", 1),
            para("", true, &row_end_grpprl(&extra)),
            cell("b", 1),
            para("", true, &row_end_grpprl(&def)),
        ];
        let t = build_table(&paras, 1);
        assert_eq!(attr(&t, "headerRows"), Some(&json!(1)));
        assert_eq!(attr(&t, "headerRepeat"), Some(&json!(true)));
        assert_eq!(attr(&t, "rowHeights"), Some(&json!([40.0, 0.0])));
        assert_eq!(attr(&t, "rowHeightModes"), Some(&json!(["exactly", "atleast"])));
        assert_eq!(attr(&t, "rowCantSplit"), Some(&json!([true, false])));
    }

    // ── The sprm that follows the table definition ───────────────────────────

    #[test]
    fn sprms_placed_after_the_table_definition_survive() {
        // sprmTDefTable is the one variable-length sprm whose count is the
        // length PLUS ONE; getting that wrong swallows everything after it.
        let def = def_table(&[0, 1440], &[tc(0)]);
        let mut g = row_end_grpprl(&def);
        // sprmTTableBorders80: six BRC80, only insideH set (index 4).
        g.extend_from_slice(&[0x05, 0xD6, 24]);
        g.extend_from_slice(&[0u8; 16]);
        g.extend_from_slice(&[12, 1, 6, 0]); // insideH: 1.5 pt, single, red
        g.extend_from_slice(&[0u8; 4]);
        // Two rows, so that insideH is a real inner edge.
        let t = build_table(
            &[cell("x", 1), para("", true, &g), cell("y", 1), para("", true, &g)],
            1,
        );
        assert_eq!(attr(&t, "tableBorderWidth"), Some(&json!(2.0)));
        assert_eq!(attr(&t, "tableBorderStyle"), Some(&json!("solid")));
        assert_eq!(attr(&t, "tableBorderColor"), Some(&json!("#ff0000")));
    }

    #[test]
    fn cell_borders_fall_back_to_the_table_defaults_by_position() {
        let def = def_table(&[0, 1440, 2880], &[tc(0), tc(0)]);
        let mut g = row_end_grpprl(&def);
        g.extend_from_slice(&[0x05, 0xD6, 24]);
        g.extend_from_slice(&[24, 1, 1, 0]); // top: 3 pt black
        g.extend_from_slice(&[0u8; 12]); // left, bottom, right: none
        g.extend_from_slice(&[6, 1, 1, 0]); // insideH
        g.extend_from_slice(&[6, 1, 1, 0]); // insideV
        let t = build_table(
            &[cell("a", 1), cell("b", 1), para("", true, &g)],
            1,
        );
        let cells = t.children()[0].children();
        let cb = attr(&cells[0], "cellBorders").expect("borders");
        assert_eq!(cb["t"], json!({ "w": 4.0, "s": "solid", "c": "#000000" }));
        // The perimeter left/bottom are explicitly absent.
        assert_eq!(cb["l"], Value::Null);
        // The inner vertical edge matches the table default, so it is not
        // repeated on the cell.
        assert!(cb.get("r").is_none());
    }

    #[test]
    fn a_table_without_any_border_asks_for_the_plain_style() {
        let def = def_table(&[0, 1440, 2880], &[tc(0), tc(0)]);
        let t = build_table(
            &[cell("a", 1), cell("b", 1), para("", true, &row_end_grpprl(&def))],
            1,
        );
        assert_eq!(attr(&t, "tableStyle"), Some(&json!("plain")));
        assert_eq!(attr(&t, "tableBorderWidth"), None);
    }

    #[test]
    fn a_fully_boxed_table_keeps_its_grid() {
        let def = def_table(&[0, 1440, 2880], &[tc_boxed(0), tc_boxed(0)]);
        let paras = vec![
            cell("a", 1),
            cell("b", 1),
            para("", true, &row_end_grpprl(&def)),
            cell("c", 1),
            cell("d", 1),
            para("", true, &row_end_grpprl(&def)),
        ];
        let t = build_table(&paras, 1);
        assert_eq!(attr(&t, "tableBorderStyle"), Some(&json!("solid")));
        assert_eq!(attr(&t, "tableBorderWidth"), Some(&json!(1.0)));
    }

    // ── Shading, alignment, direction ────────────────────────────────────────

    #[test]
    fn cell_shading_becomes_a_solid_background() {
        let def = def_table(&[0, 1440], &[tc(0)]);
        let mut g = row_end_grpprl(&def);
        // sprmTDefTableShd80: icoFore=1 (black), icoBack=8 (white), ipat=5 (25%).
        let shd: u16 = 1 | (8 << 5) | (5 << 10);
        g.extend_from_slice(&[0x09, 0xD6, 2]);
        g.extend_from_slice(&shd.to_le_bytes());
        let t = build_table(&[cell("x", 1), para("", true, &g)], 1);
        let c = &t.children()[0].children()[0];
        assert_eq!(attr(c, "cellBg"), Some(&json!("#bfbfbf")));
    }

    #[test]
    fn vertical_alignment_and_text_direction_come_from_the_cell_flags() {
        // fVertical | fBackward | vertAlign = 2 (bottom).
        let def = def_table(&[0, 1440], &[tc(0x0004 | 0x0008 | (2 << 7))]);
        let t = build_table(&[cell("x", 1), para("", true, &row_end_grpprl(&def))], 1);
        let c = &t.children()[0].children()[0];
        assert_eq!(attr(c, "cellVAlign"), Some(&json!("bottom")));
        assert_eq!(attr(c, "cellDir"), Some(&json!(270)));
    }

    #[test]
    fn a_centred_table_is_reported_as_such() {
        let def = def_table(&[0, 1440], &[tc(0)]);
        let mut g = row_end_grpprl(&[0x00, 0x54, 1, 0]); // sprmTJc90 = centre
        g.extend_from_slice(&def);
        let t = build_table(&[cell("x", 1), para("", true, &g)], 1);
        assert_eq!(attr(&t, "tableAlign"), Some(&json!("center")));
    }

    // ── Nesting ──────────────────────────────────────────────────────────────

    #[test]
    fn a_nested_table_lands_inside_its_cell() {
        let inner_def = def_table(&[0, 1440], &[tc(0)]);
        let outer_def = def_table(&[0, 2880], &[tc(0)]);
        // The inner row terminator carries sprmPFInnerTtp and itap = 2.
        let mut inner_end = vec![0x16, 0x24, 1, 0x4C, 0x24, 1, 0x49, 0x66, 2, 0, 0, 0];
        inner_end.extend_from_slice(&inner_def);
        let paras = vec![
            cell("inner", 2),
            para("", true, &inner_end),
            cell("", 1),
            para("", true, &row_end_grpprl(&outer_def)),
        ];
        let t = build_table(&paras, 1);
        assert_eq!(t.node_type, "table");
        let outer_cell = &t.children()[0].children()[0];
        let nested = outer_cell
            .children()
            .iter()
            .find(|n| n.node_type == "table")
            .expect("nested table");
        assert_eq!(text_of(&nested.children()[0].children()[0]), "inner");
    }

    #[test]
    fn assemble_keeps_ordinary_paragraphs_around_the_table() {
        let def = def_table(&[0, 1440], &[tc(0)]);
        let paras = vec![
            para("before", false, &[]),
            cell("x", 1),
            para("", true, &row_end_grpprl(&def)),
            para("after", false, &[]),
        ];
        let out = assemble(&paras);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].node_type, "paragraph");
        assert_eq!(out[1].node_type, "table");
        assert_eq!(out[2].node_type, "paragraph");
    }

    #[test]
    fn every_attribute_name_belongs_to_the_docx_vocabulary() {
        // A table must not render differently depending on where it came from,
        // so the names here are exactly those of `docx::read::table`.
        const TABLE_ATTRS: &[&str] = &[
            "colWidths", "rowHeights", "rowHeightModes", "rowCantSplit", "headerRows",
            "headerRepeat", "tableIndent", "tableAlign", "tableLayout", "tableStyle",
            "tableBorderWidth", "tableBorderStyle", "tableBorderColor", "cellMarginTop",
            "cellMarginBottom", "cellMarginLeft", "cellMarginRight", "cellSpacing",
        ];
        const CELL_ATTRS: &[&str] =
            &["colspan", "rowspan", "cellBg", "cellVAlign", "cellDir", "cellBorders"];

        let def = def_table(&[360, 1800], &[tc_boxed(0x0004 | (1 << 7))]);
        let mut g = row_end_grpprl(&def);
        g.extend_from_slice(&[0x09, 0xD6, 2, 0x01, 0x04]); // shading
        let t = build_table(&[cell("x", 1), para("", true, &g)], 1);
        for k in t.attrs.as_ref().and_then(|a| a.as_object()).expect("attrs").keys() {
            assert!(TABLE_ATTRS.contains(&k.as_str()), "attribut de table inconnu: {k}");
        }
        let c = &t.children()[0].children()[0];
        for k in c.attrs.as_ref().and_then(|a| a.as_object()).expect("attrs").keys() {
            assert!(CELL_ATTRS.contains(&k.as_str()), "attribut de cellule inconnu: {k}");
        }
    }

    // ── Real documents ───────────────────────────────────────────────────────
    //
    // Scaffolding: the paragraph-property layer (`pap`, `fkp`) is still being
    // written, so these tests walk the PAPX bins themselves to feed `assemble`
    // with real files. They go away once `import_doc` wires the tables in.

    mod corpus {
        use super::*;
        use crate::converters::doc::fib::{u32_at, Fib};
        use crate::converters::doc::{fkp, piece, text};
        use std::io::{Cursor, Read};

        /// One `PAPX` and the byte range of the stream it applies to.
        struct PapxRun {
            fc_start: u32,
            fc_end: u32,
            grpprl: Vec<u8>,
        }

        /// A `PAPX` too big for its 512-byte page lives in the `Data` stream:
        /// the in-page list then starts with `sprmPHugePapx` (0x6645/0x6646),
        /// whose operand is an offset there, or with 0x646B, which PREPENDS the
        /// out-of-line bytes instead of replacing them (LibreOffice
        /// `IsReplaceAllSprm` / `IsExpandableSprm`, ww8scan.cxx:2623).
        fn expand_huge(grpprl: Vec<u8>, data: &[u8]) -> Vec<u8> {
            if grpprl.len() < 6 {
                return grpprl;
            }
            let id = u16::from_le_bytes([grpprl[0], grpprl[1]]);
            if id != 0x6645 && id != 0x6646 && id != 0x646B {
                return grpprl;
            }
            let pos = u32_at(&grpprl, 2) as usize;
            let Some(d) = data.get(pos..) else { return grpprl };
            if d.len() < 2 {
                return grpprl;
            }
            let n = u16::from_le_bytes([d[0], d[1]]) as usize;
            let Some(bytes) = d.get(2..2 + n) else { return grpprl };
            let mut out = bytes.to_vec();
            if id == 0x646B {
                out.extend_from_slice(&grpprl);
            }
            out
        }

        /// Decode a 512-byte PAPX FKP page. Byte 511 counts the runs; the page
        /// then holds `n+1` byte positions, `n` 13-byte descriptors, and the
        /// property lists themselves at word offsets ([MS-DOC] 2.9.23).
        fn papx_page(page: &[u8], data: &[u8]) -> Vec<PapxRun> {
            let mut out = Vec::new();
            if page.len() < 512 {
                return out;
            }
            let n = page[511] as usize;
            if n == 0 || 4 * (n + 1) + n * 13 > 511 {
                return out;
            }
            for i in 0..n {
                let word_off = page[4 * (n + 1) + i * 13] as usize;
                if word_off == 0 {
                    continue;
                }
                let p = word_off * 2;
                if p >= page.len() {
                    continue;
                }
                let cb = page[p] as usize;
                let (start, len) = if cb != 0 {
                    (p + 1, cb * 2 - 1)
                } else {
                    (p + 2, (*page.get(p + 1).unwrap_or(&0) as usize) * 2)
                };
                // A GrpPrlAndIstd opens with a 2-byte style index.
                if len < 2 || start + len > page.len() {
                    continue;
                }
                out.push(PapxRun {
                    fc_start: u32_at(page, i * 4),
                    fc_end: u32_at(page, (i + 1) * 4),
                    grpprl: expand_huge(page[start + 2..start + len].to_vec(), data),
                });
            }
            out
        }

        fn papx_runs(doc: &[u8], table: &[u8], data: &[u8], fib: &Fib) -> Vec<PapxRun> {
            let mut out = Vec::new();
            for pn in fkp::bin_table_pages(table, fib.fc_plcf_bte_papx, fib.lcb_plcf_bte_papx) {
                let off = pn as usize * 512;
                if off + 512 <= doc.len() {
                    out.extend(papx_page(&doc[off..off + 512], data));
                }
            }
            out.sort_by_key(|r| r.fc_start);
            out
        }

        /// Character position → byte position, through the piece table.
        fn cp_to_fc(pieces: &[piece::Piece], cp: u32) -> u32 {
            for p in pieces {
                if cp >= p.cp_start && cp < p.cp_end {
                    let n = cp - p.cp_start;
                    return if p.compressed { p.fc + n } else { p.fc + n * 2 };
                }
            }
            0
        }

        /// The whole chain, from bytes to blocks.
        pub(super) fn blocks(data: &[u8]) -> Option<Vec<PmNode>> {
            let mut comp = cfb::CompoundFile::open(Cursor::new(data)).ok()?;
            let mut read = |name: &str| -> Option<Vec<u8>> {
                let mut s = comp.open_stream(name).ok()?;
                let mut b = Vec::new();
                s.read_to_end(&mut b).ok()?;
                Some(b)
            };
            let doc = read("WordDocument")?;
            let fib = Fib::parse(&doc).ok()?;
            if fib.encrypted {
                return None;
            }
            let tbl = read(fib.table_stream_name()).unwrap_or_default();
            let data = read("Data").unwrap_or_default();
            let pieces = piece::parse_pieces(&fib, &tbl);
            let enc = piece::charset_for(&fib);
            let txt = piece::read_text(&fib, &doc, &pieces, enc);
            let paras = text::split_paragraphs(&txt.chars);
            let runs = papx_runs(&doc, &tbl, &data, &fib);

            let mut out = Vec::with_capacity(paras.len());
            for (i, p) in paras.iter().enumerate() {
                // The properties of a paragraph hang off its mark, which is the
                // character just before the next paragraph starts.
                let mark = paras
                    .get(i + 1)
                    .map(|nx| nx.cp_start.saturating_sub(1))
                    .unwrap_or(p.cp_start);
                let fc = cp_to_fc(&pieces, mark);
                let grpprl = runs
                    .iter()
                    .find(|r| fc >= r.fc_start && fc < r.fc_end)
                    .map(|r| r.grpprl.as_slice())
                    .unwrap_or_default();
                let content = if p.text.is_empty() {
                    vec![]
                } else {
                    vec![PmNode::text(p.text.clone(), vec![])]
                };
                out.push(Para {
                    node: PmNode::paragraph(content),
                    cell_end: p.in_table,
                    table: ParaTable::parse(grpprl),
                });
            }
            Some(assemble(&out))
        }

        /// Rows, cells (spacers excluded), and the sum of the spans.
        pub(super) fn count(nodes: &[PmNode], t: &mut (usize, usize, usize, usize)) {
            for n in nodes {
                match n.node_type.as_str() {
                    "table" => {
                        t.0 += 1;
                        for row in n.children() {
                            t.1 += 1;
                            for c in row.children() {
                                t.2 += 1;
                                let span = c
                                    .attrs
                                    .as_ref()
                                    .and_then(|a| a.get("rowspan"))
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(1);
                                if span > 1 {
                                    t.3 += 1;
                                }
                                count(c.children(), t);
                            }
                        }
                    }
                    _ => count(n.children(), t),
                }
            }
        }
    }

    /// Walk a directory of real `.doc` files and report what was rebuilt.
    ///
    /// Set `KUBUNO_DOC_CORPUS` to a directory (e.g. a LibreOffice checkout) to
    /// run it; the numbers it prints are what gets compared against
    /// `soffice --headless --convert-to fodt`.
    #[test]
    fn corpus_tables_are_rebuilt() {
        let Ok(root) = std::env::var("KUBUNO_DOC_CORPUS") else {
            eprintln!("KUBUNO_DOC_CORPUS absent — test ignoré");
            return;
        };
        let mut files = Vec::new();
        let mut stack = vec![std::path::PathBuf::from(root)];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "doc") {
                    files.push(p);
                }
            }
        }
        files.sort();
        let (mut seen, mut with_tables) = (0usize, 0usize);
        let (mut tables, mut rows, mut cells) = (0usize, 0usize, 0usize);
        for p in &files {
            let Ok(bytes) = std::fs::read(p) else { continue };
            seen += 1;
            let Some(blocks) = corpus::blocks(&bytes) else { continue };
            let mut t = (0usize, 0usize, 0usize, 0usize);
            corpus::count(&blocks, &mut t);
            if t.0 > 0 {
                with_tables += 1;
            }
            tables += t.0;
            rows += t.1;
            cells += t.2;
            eprintln!("STAT\t{}\t{}\t{}\t{}\t{}", p.display(), t.0, t.1, t.2, t.3);
        }
        eprintln!(
            "corpus .doc: {seen} fichiers, {with_tables} avec tableaux, \
             {tables} tableaux / {rows} lignes / {cells} cellules"
        );
        assert!(seen > 0, "aucun .doc trouvé dans KUBUNO_DOC_CORPUS");
    }
}
