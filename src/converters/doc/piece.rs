//! The piece table — how a `.doc` maps character positions onto stream bytes.
//!
//! Word does not store its text contiguously: edits append text at the end of
//! the stream and record the reading order in a piece table (`Clx`). Each piece
//! says where its bytes live and whether they are 8-bit (cp1252-ish) or UTF-16.
//! Ignoring it yields text in the order it was TYPED, not the order it reads.
//! Reference: LibreOffice `WW8PLCFx_PCD` / `WW8ScannerBase`, ww8scan.cxx:6100.

use encoding_rs::Encoding;

use super::fib::{u16_at, u32_at, Fib};

/// One run of text: a character-position range and where its bytes are.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Piece {
    /// First character position covered by this piece.
    pub(crate) cp_start: u32,
    /// One past the last character position.
    pub(crate) cp_end: u32,
    /// Byte offset of the piece's text in the WordDocument stream.
    pub(crate) fc: u32,
    /// 8-bit (a legacy code page) rather than UTF-16LE.
    pub(crate) compressed: bool,
}

/// The document text, decoded, plus the piece boundaries in character positions.
pub(crate) struct DocText {
    /// The whole main text as chars, indexed by character position.
    pub(crate) chars: Vec<char>,
}

/// Read the `Clx` and return the pieces in reading order.
///
/// `Clx` is a sequence of `Prc` entries (marker 0x01) followed by the `Pcdt`
/// (marker 0x02): a length, then a PLCF of `cp` boundaries and 8-byte `Pcd`
/// records. In a `Pcd`, bit 30 of `fc` marks 8-bit text and the real offset is
/// then `fc / 2` ([MS-DOC] 2.9.177).
pub(crate) fn parse_pieces(fib: &Fib, table: &[u8]) -> Vec<Piece> {
    let start = fib.fc_clx as usize;
    let end = start.saturating_add(fib.lcb_clx as usize).min(table.len());
    if start >= end {
        // No piece table: the whole text is one uncompressed run (Word 6 style).
        return vec![Piece {
            cp_start: 0,
            cp_end: fib.fc_mac.saturating_sub(fib.fc_min),
            fc: fib.fc_min,
            compressed: false,
        }];
    }
    let clx = &table[start..end];

    // Skip the Prc entries to find the Pcdt.
    let mut i = 0usize;
    while i < clx.len() {
        match clx[i] {
            0x01 => {
                let cb = u16_at(clx, i + 1) as usize;
                i = i.saturating_add(3).saturating_add(cb);
            }
            0x02 => break,
            // Anything else means we lost sync; give up rather than emit garbage.
            _ => return Vec::new(),
        }
    }
    if i >= clx.len() || clx[i] != 0x02 {
        return Vec::new();
    }
    let lcb = u32_at(clx, i + 1) as usize;
    let pcdt = &clx[(i + 5).min(clx.len())..(i + 5 + lcb).min(clx.len())];
    // A PLCF of n+1 CPs (4 bytes each) then n PCDs (8 bytes each).
    let n = if pcdt.len() >= 4 { (pcdt.len() - 4) / 12 } else { 0 };
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        let cp_start = u32_at(pcdt, k * 4);
        let cp_end = u32_at(pcdt, (k + 1) * 4);
        let pcd = 4 * (n + 1) + k * 8;
        let fc_raw = u32_at(pcdt, pcd + 2);
        let compressed = fc_raw & 0x4000_0000 != 0;
        let fc = if compressed {
            (fc_raw & 0x3FFF_FFFF) / 2
        } else {
            fc_raw & 0x3FFF_FFFF
        };
        if cp_end > cp_start {
            out.push(Piece { cp_start, cp_end, fc, compressed });
        }
    }
    out
}

/// Decode the main text of the document by walking the pieces in order.
pub(crate) fn read_text(fib: &Fib, doc: &[u8], pieces: &[Piece], enc: &'static Encoding) -> DocText {
    let limit = fib.ccp_text;
    let mut chars: Vec<char> = Vec::with_capacity(limit as usize);
    for p in pieces {
        if p.cp_start >= limit {
            break;
        }
        let count = (p.cp_end.min(limit) - p.cp_start) as usize;
        if p.compressed {
            let from = p.fc as usize;
            let to = from.saturating_add(count).min(doc.len());
            if from >= to {
                continue;
            }
            // 8-bit text: a legacy code page, not Latin-1 — 0x92 is a curly
            // apostrophe in cp1252, not a control character.
            let (s, _, _) = enc.decode(&doc[from..to]);
            chars.extend(s.chars());
        } else {
            let from = p.fc as usize;
            let to = from.saturating_add(count * 2).min(doc.len());
            if from + 1 >= to {
                continue;
            }
            let units: Vec<u16> = doc[from..to]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            chars.extend(String::from_utf16_lossy(&units).chars());
        }
    }
    DocText { chars }
}

/// Code page implied by the FIB's language id, for 8-bit pieces.
/// Word 97+ documents are almost always cp1252 unless written in a legacy
/// locale; LibreOffice derives it the same way (`WW8Fib::GetFIBCharset`).
pub(crate) fn charset_for(fib: &Fib) -> &'static Encoding {
    match fib.lid {
        0x0401 | 0x0801 => encoding_rs::WINDOWS_1256, // Arabic
        0x0405 | 0x040E | 0x0415 | 0x041B | 0x0424 => encoding_rs::WINDOWS_1250, // Central Europe
        0x0408 => encoding_rs::WINDOWS_1253,          // Greek
        0x040D => encoding_rs::WINDOWS_1255,          // Hebrew
        0x041F => encoding_rs::WINDOWS_1254,          // Turkish
        0x0419 | 0x0402 | 0x0422 | 0x0423 => encoding_rs::WINDOWS_1251, // Cyrillic
        _ => encoding_rs::WINDOWS_1252,
    }
}
