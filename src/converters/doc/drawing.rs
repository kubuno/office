//! Pictures: from a `PICF` in the `Data` stream to a ProseMirror image node.
//!
//! A `.doc` never stores an image inline with the text. A run whose CHP carries
//! `sprmCPicLocation` (0x6A03, 0x46 in Word 6/95) holds a byte offset into the
//! `Data` stream of the OLE2 container — or into `WordDocument` when there is
//! no `Data` stream. At that offset sits a `PICF`: display size in twips,
//! scaling factors, cropping, borders. **Its length is not fixed** (58 bytes in
//! Word 6/95, 68 in Word 97, more in theory), which is exactly why the
//! structure carries its own `cbHeader`; hard-coding the offset of the payload
//! is the classic way to read garbage out of half the corpus.
//!
//! What follows the header depends on `MFP.mm`:
//!   * 0x64 / 0x66 — an Escher (OfficeArt) shape container; the actual PNG /
//!     JPEG / WMF sits in a `msofbtBSE` → blip record right after it, or is
//!     referenced by index into the drawing group's blip store;
//!   * 94 / 99 — a *linked* file, only its name is stored: nothing to extract;
//!   * anything else — a raw Windows metafile or DIB, written as-is.
//!
//! Floating pictures (the 0x08 character) are anchored through `PlcfSpaMom`:
//! character position → shape id → Escher shape → blip index → blip store.
//! The store itself lives in the drawing group at `fcDggInfo` in the table
//! stream, with the blip bytes either inline in the `BSE` or at `foDelay` in
//! the `Data` stream — or, just as often, at that same offset in
//! `WordDocument`, which is the second stream Word looks in.
//!
//! Reference: LibreOffice `sw/source/filter/ww8/ww8graf2.cxx`
//! (`SwWW8ImplReader::ImportGraf`, `WW8PicDesc`) and
//! `filter/source/msfilter/msdffimp.cxx` (`GetBLIPDirect`,
//! `GetDrawingGroupContainerData`), plus [MS-ODRAW] and [MS-DOC] 2.9.158.
//!
//! Nothing here panics and nothing here guesses: a payload we cannot turn into
//! something a browser renders (a vector metafile with no bitmap inside, an
//! embedded OLE object, a linked file) yields `None` rather than an empty
//! image node.

// The character-property layer is what calls into this module (a run's
// `sprmCPicLocation`, a 0x08 anchor's character position); until it is wired
// up in `mod.rs` the whole surface below reads as unused.
#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read};

use base64::Engine;
use serde_json::{json, Value};

use super::fib::{i16_at, u16_at, u32_at, Fib};
use crate::converters::types::PmNode;

/// 1440 twips per inch ÷ 96 CSS pixels per inch.
const TWIPS_PER_PX: f64 = 15.0;

/// Escher record types we recognise ([MS-ODRAW] 2.2.x).
mod fbt {
    pub(super) const DGG_CONTAINER: u16 = 0xF000;
    pub(super) const BSTORE_CONTAINER: u16 = 0xF001;
    pub(super) const SP_CONTAINER: u16 = 0xF004;
    pub(super) const BSE: u16 = 0xF007;
    pub(super) const FSP: u16 = 0xF00A;
    pub(super) const OPT: u16 = 0xF00B;
    pub(super) const BLIP_FIRST: u16 = 0xF018;
    pub(super) const BLIP_LAST: u16 = 0xF117;
}

/// `msofbtOPT` property holding the blip index of a picture shape.
const DFF_PROP_PIB: u16 = 260;

/// Byte length of the fixed part of a `BSE` record, before the embedded blip.
const BSE_HEADER: usize = 36;

/// Blip record instances — the payload format ([MS-ODRAW] 2.2.23).
mod blip_inst {
    pub(super) const EMF: u16 = 0x3D4;
    pub(super) const WMF: u16 = 0x216;
    pub(super) const PICT: u16 = 0x542;
    pub(super) const JPEG_RGB: u16 = 0x46A;
    pub(super) const JPEG_CMYK: u16 = 0x6E2;
    pub(super) const PNG: u16 = 0x6E0;
    pub(super) const DIB: u16 = 0x7A8;
    pub(super) const TIFF: u16 = 0x6E4;
}

// ---------------------------------------------------------------------------
// Decoded image bytes
// ---------------------------------------------------------------------------

/// An image we managed to turn into something a browser can display.
#[derive(Debug, Clone)]
pub(crate) struct RawImage {
    pub(crate) mime: &'static str,
    pub(crate) bytes: Vec<u8>,
}

impl RawImage {
    /// The `src` attribute of the ProseMirror node: a self-contained data URI,
    /// exactly like the DOCX importer builds in `zip_io::build_media_map`.
    pub(crate) fn data_uri(&self) -> String {
        let b64 = base64::engine::general_purpose::STANDARD.encode(&self.bytes);
        format!("data:{};base64,{}", self.mime, b64)
    }

    /// Intrinsic pixel size, when the format states it in a header we read.
    /// Only used to check that what we extracted really is an image; the
    /// displayed size comes from the `PICF`.
    pub(crate) fn pixel_size(&self) -> Option<(u32, u32)> {
        pixel_size(self.mime, &self.bytes)
    }

    /// A cheap identity for de-duplication: the same blip is often reachable
    /// both through the shape store and through the `Data` stream.
    fn fingerprint(&self) -> (usize, u64) {
        let mut h = 1469598103934665603u64; // FNV-1a
        for &b in self.bytes.iter().take(256) {
            h ^= b as u64;
            h = h.wrapping_mul(1099511628211);
        }
        (self.bytes.len(), h)
    }
}

/// Recognise a payload by its magic number and, when needed, repackage it.
///
/// A DIB is a `BITMAPINFOHEADER` with no file header at all: handed to a
/// browser as-is it is not an image, so we prepend the 14-byte `BM` header.
/// A metafile is not renderable either, but Word 6/95 documents very often
/// wrap a plain bitmap in a one-record metafile — we pull that bitmap out.
fn normalise(bytes: &[u8]) -> Option<RawImage> {
    if bytes.len() < 24 {
        return None;
    }
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some(RawImage { mime: "image/png", bytes: bytes.to_vec() });
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(RawImage { mime: "image/jpeg", bytes: bytes.to_vec() });
    }
    if bytes.starts_with(b"GIF8") {
        return Some(RawImage { mime: "image/gif", bytes: bytes.to_vec() });
    }
    if bytes.starts_with(b"BM") && bytes.len() > 54 && u32_at(bytes, 10) as usize <= bytes.len() {
        return Some(RawImage { mime: "image/bmp", bytes: bytes.to_vec() });
    }
    if bytes.starts_with(&[0x49, 0x49, 0x2A, 0x00]) || bytes.starts_with(&[0x4D, 0x4D, 0x00, 0x2A])
    {
        return Some(RawImage { mime: "image/tiff", bytes: bytes.to_vec() });
    }
    // Enhanced metafile: EMR_HEADER, then the " EMF" signature.
    if u32_at(bytes, 0) == 1 && bytes.len() > 44 && &bytes[40..44] == b" EMF" {
        return emf_carve_dib(bytes).as_deref().and_then(dib_to_bmp);
    }
    // Windows metafile, placeable or not.
    if is_wmf(bytes) {
        return wmf_carve_dib(bytes);
    }
    // A bare DIB — the `PICF` payload of a Word 6 bitmap picture.
    dib_to_bmp(bytes)
}

fn is_wmf(b: &[u8]) -> bool {
    if u32_at(b, 0) == 0x9AC6_CDD7 {
        return true;
    }
    matches!(u16_at(b, 0), 1 | 2) && u16_at(b, 2) == 9 && matches!(u16_at(b, 4), 0x0100 | 0x0300)
}

// ---------------------------------------------------------------------------
// DIB → BMP
// ---------------------------------------------------------------------------

/// Wrap a device-independent bitmap in a BMP file header.
///
/// The whole difficulty is `bfOffBits`: the pixels start after the info header
/// *and* the colour table (or the bit-field masks of a `BI_BITFIELDS` image),
/// whose size depends on the bit depth. Get it wrong and the image decodes as
/// noise rather than failing loudly.
fn dib_to_bmp(dib: &[u8]) -> Option<RawImage> {
    if dib.len() < 28 {
        return None;
    }
    let hdr = u32_at(dib, 0) as usize;
    let (bit_count, planes, clr_used, compression) = match hdr {
        12 => (u16_at(dib, 10) as u32, u16_at(dib, 8), 0u32, 0u32),
        40 | 52 | 56 | 64 | 108 | 124 => (
            u16_at(dib, 14) as u32,
            u16_at(dib, 12),
            u32_at(dib, 32),
            u32_at(dib, 16),
        ),
        _ => return None,
    };
    if hdr > dib.len() || planes != 1 || !matches!(bit_count, 1 | 2 | 4 | 8 | 16 | 24 | 32) {
        return None;
    }
    // Compression codes actually defined for DIBs; anything else means we are
    // looking at bytes that merely resemble a header.
    if hdr != 12 && compression > 6 {
        return None;
    }
    let (w, h) = if hdr == 12 {
        (u16_at(dib, 4) as i64, u16_at(dib, 6) as i64)
    } else {
        (u32_at(dib, 4) as i32 as i64, u32_at(dib, 8) as i32 as i64)
    };
    if w <= 0 || h == 0 || w > 60_000 || h.abs() > 60_000 {
        return None;
    }

    let entry = if hdr == 12 { 3u32 } else { 4 };
    let mut extra = if bit_count <= 8 {
        let max = 1u32 << bit_count;
        let n = if clr_used == 0 || clr_used > max { max } else { clr_used };
        n * entry
    } else {
        0
    };
    // BI_BITFIELDS / BI_ALPHABITFIELDS put their masks after a plain
    // BITMAPINFOHEADER; the V4/V5 headers carry them inside instead.
    if hdr == 40 {
        match compression {
            3 => extra += 12,
            6 => extra += 16,
            _ => {}
        }
    }
    let off_bits = 14u64 + hdr as u64 + extra as u64;
    let size = 14u64 + dib.len() as u64;
    if off_bits > size || size > u32::MAX as u64 {
        return None;
    }

    let mut out = Vec::with_capacity(size as usize);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(size as u32).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(off_bits as u32).to_le_bytes());
    out.extend_from_slice(dib);
    Some(RawImage { mime: "image/bmp", bytes: out })
}

/// Pull the bitmap out of a metafile that only paints one.
///
/// Word 6/95 stored most pictures this way: an 18-byte `METAHEADER` and a
/// single `META_STRETCHDIB` / `META_DIBSTRETCHBLT` record whose parameters are
/// followed by a DIB. Records we do not know are skipped by their own length,
/// so an unusual metafile costs us nothing but a `None`.
fn wmf_carve_dib(wmf: &[u8]) -> Option<RawImage> {
    let mut p = if u32_at(wmf, 0) == 0x9AC6_CDD7 { 22usize } else { 0 };
    if !(matches!(u16_at(wmf, p), 1 | 2) && u16_at(wmf, p + 2) == 9) {
        return None;
    }
    p += 18;
    while p + 6 <= wmf.len() {
        let size = (u32_at(wmf, p) as usize).checked_mul(2)?;
        let func = u16_at(wmf, p + 4);
        if size < 6 || p + size > wmf.len() {
            return None;
        }
        let params = &wmf[p + 6..p + size];
        let skip = match func {
            0x0F43 => 22, // META_STRETCHDIB
            0x0B41 => 20, // META_DIBSTRETCHBLT
            0x0940 => 16, // META_DIBBITBLT
            0x0D33 => 18, // META_SETDIBTODEV
            _ => {
                p += size;
                continue;
            }
        };
        if params.len() > skip {
            if let Some(img) = dib_to_bmp(&params[skip..]) {
                return Some(img);
            }
        }
        p += size;
    }
    None
}

/// Same idea for an enhanced metafile: `EMR_STRETCHDIBITS` and
/// `EMR_SETDIBITSTODEVICE` reference a header block and a pixel block by
/// offset, which we concatenate back into a DIB.
fn emf_carve_dib(emf: &[u8]) -> Option<Vec<u8>> {
    let mut p = 0usize;
    while p + 8 <= emf.len() {
        let rec_type = u32_at(emf, p);
        let size = u32_at(emf, p + 4) as usize;
        if size < 8 || !size.is_multiple_of(4) || p + size > emf.len() {
            return None;
        }
        let rec = &emf[p..p + size];
        // Offsets of (offBmiSrc, cbBmiSrc, offBitsSrc, cbBitsSrc) inside the
        // record, per [MS-EMF] 2.3.1.
        let at = match rec_type {
            81 => Some(52usize), // EMR_STRETCHDIBITS
            80 => Some(48),      // EMR_SETDIBITSTODEVICE
            _ => None,
        };
        if let Some(at) = at.filter(|a| rec.len() >= a + 16) {
            let (obmi, cbmi) = (u32_at(rec, at) as usize, u32_at(rec, at + 4) as usize);
            let (obits, cbits) = (u32_at(rec, at + 8) as usize, u32_at(rec, at + 12) as usize);
            if cbmi >= 12
                && obmi.saturating_add(cbmi) <= rec.len()
                && obits.saturating_add(cbits) <= rec.len()
                && cbits > 0
            {
                let mut dib = Vec::with_capacity(cbmi + cbits);
                dib.extend_from_slice(&rec[obmi..obmi + cbmi]);
                dib.extend_from_slice(&rec[obits..obits + cbits]);
                return Some(dib);
            }
        }
        if rec_type == 14 {
            break; // EMR_EOF
        }
        p += size;
    }
    None
}

/// Intrinsic size, read from the format's own header.
fn pixel_size(mime: &str, b: &[u8]) -> Option<(u32, u32)> {
    match mime {
        "image/png" => {
            (b.len() >= 24 && &b[12..16] == b"IHDR").then(|| (be32(b, 16), be32(b, 20)))
        }
        "image/bmp" => {
            if b.len() < 26 {
                return None;
            }
            let hdr = u32_at(b, 14) as usize;
            if hdr == 12 {
                Some((u16_at(b, 18) as u32, u16_at(b, 20) as u32))
            } else {
                Some((u32_at(b, 18), (u32_at(b, 22) as i32).unsigned_abs()))
            }
        }
        "image/gif" => (b.len() >= 10).then(|| (u16_at(b, 6) as u32, u16_at(b, 8) as u32)),
        "image/jpeg" => jpeg_size(b),
        _ => None,
    }
}

fn be32(b: &[u8], off: usize) -> u32 {
    if off + 4 > b.len() {
        return 0;
    }
    u32::from_be_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

/// Walk the JPEG segments up to the frame header, which carries the size.
fn jpeg_size(b: &[u8]) -> Option<(u32, u32)> {
    let mut i = 2usize;
    while i + 4 <= b.len() {
        if b[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = b[i + 1];
        if marker == 0xFF {
            i += 1;
            continue;
        }
        // Start-of-frame markers, minus the three that are not frames.
        if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            let h = ((*b.get(i + 5)? as u32) << 8) | *b.get(i + 6)? as u32;
            let w = ((*b.get(i + 7)? as u32) << 8) | *b.get(i + 8)? as u32;
            return Some((w, h));
        }
        if matches!(marker, 0xD8 | 0xD9 | 0x01) || (0xD0..=0xD7).contains(&marker) {
            i += 2;
            continue;
        }
        let len = ((*b.get(i + 2)? as usize) << 8) | *b.get(i + 3)? as usize;
        if len < 2 {
            return None;
        }
        i += 2 + len;
    }
    None
}

// ---------------------------------------------------------------------------
// Escher records
// ---------------------------------------------------------------------------

/// One record header of the OfficeArt stream.
struct Rec {
    ver: u16,
    inst: u16,
    fbt: u16,
    /// The record's payload, clamped to what the buffer really holds.
    body_start: usize,
    body_end: usize,
    /// Offset just past the record, for the caller's walk.
    next: usize,
}

/// Every OfficeArt record type lives in `0xF000..=0xF11F`; anything else at a
/// record boundary means the walk lost alignment.
fn plausible_fbt(fbt: u16) -> bool {
    (0xF000..=0xF11F).contains(&fbt)
}

fn read_rec(buf: &[u8], at: usize) -> Option<Rec> {
    if at + 8 > buf.len() {
        return None;
    }
    let v = u16_at(buf, at);
    let fbt = u16_at(buf, at + 2);
    let len = u32_at(buf, at + 4) as usize;
    // A record longer than any plausible document means we lost sync.
    if len > 0x0400_0000 {
        return None;
    }
    let body_start = at + 8;
    Some(Rec {
        ver: v & 0x0F,
        inst: v >> 4,
        fbt,
        body_start,
        body_end: body_start.saturating_add(len).min(buf.len()),
        next: body_start.saturating_add(len),
    })
}

/// Decode a blip record body into image bytes.
///
/// The payload starts after one or two 16-byte UIDs — `inst & 1` says which —
/// then a one-byte tag for raster formats, or a 34-byte metafile header whose
/// content is usually zlib-compressed
/// (msdffimp.cxx, `SvxMSDffManager::GetBLIPDirect`).
fn blip_payload(inst: u16, body: &[u8]) -> Option<RawImage> {
    let uid = if inst & 1 != 0 { 32usize } else { 16 };
    match inst & 0xFFFE {
        blip_inst::EMF | blip_inst::WMF | blip_inst::PICT => {
            // metafileHeader: cbSize(4) rcBounds(16) ptSize(8) cbSave(4)
            // compression(1) filter(1).
            let compression = *body.get(uid + 32)?;
            let raw = body.get(uid + 34..)?;
            let decoded = if compression == 0 { inflate(raw)? } else { raw.to_vec() };
            normalise(&decoded)
        }
        blip_inst::JPEG_RGB
        | blip_inst::JPEG_CMYK
        | blip_inst::PNG
        | blip_inst::DIB
        | blip_inst::TIFF => normalise(body.get(uid + 1..)?),
        // An instance we do not know: try the two usual payload offsets and
        // let the magic numbers decide.
        _ => normalise(body.get(uid + 1..)?).or_else(|| normalise(body.get(uid..)?)),
    }
}

/// zlib inflate, tolerating a truncated tail: a damaged file should still give
/// us the pixels it does have.
fn inflate(src: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut dec = flate2::read::ZlibDecoder::new(src);
    match dec.read_to_end(&mut out) {
        Ok(_) => Some(out),
        Err(_) if !out.is_empty() => Some(out),
        Err(_) => None,
    }
}

// ---------------------------------------------------------------------------
// PICF
// ---------------------------------------------------------------------------

/// The picture descriptor that precedes every embedded image.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Picf {
    /// Total size: this header plus the picture data that follows it.
    pub(crate) lcb: u32,
    /// Size of this header — variable, which is the whole point of the field.
    pub(crate) cb_header: u16,
    /// Storage kind (`MFP.mm`): 0x64/0x66 Escher, 94/99 linked file, else raw.
    pub(crate) mm: i16,
    /// Size of the rectangle the picture is drawn in, in twips, before scaling.
    pub(crate) dxa_goal: i16,
    pub(crate) dya_goal: i16,
    /// Scaling in thousandths, applied on top of the goal size.
    pub(crate) mx: u16,
    pub(crate) my: u16,
    /// Cropping, in twips, taken off the goal size before scaling.
    pub(crate) crop_l: i16,
    pub(crate) crop_t: i16,
    pub(crate) crop_r: i16,
    pub(crate) crop_b: i16,
}

impl Picf {
    /// Displayed size in CSS pixels.
    ///
    /// `dxaGoal` is the *uncropped* size; Word crops first and scales after,
    /// and `mx`/`my` are per-mille — the visible size is the product of the
    /// two, never the raw goal size (LibreOffice: `WW8PicDesc::WW8PicDesc`).
    pub(crate) fn size_px(&self) -> (f64, f64) {
        let cw = (self.dxa_goal as i64 - self.crop_l as i64 - self.crop_r as i64).max(1);
        let ch = (self.dya_goal as i64 - self.crop_t as i64 - self.crop_b as i64).max(1);
        let w = cw as f64 * self.mx.max(1) as f64 / 1000.0 / TWIPS_PER_PX;
        let h = ch as f64 * self.my.max(1) as f64 / 1000.0 / TWIPS_PER_PX;
        (w.round().clamp(1.0, 4000.0), h.round().clamp(1.0, 4000.0))
    }
}

/// Read a `PICF` at `fc`, rejecting anything that cannot be one.
///
/// Word writes a metafile-shaped structure for some field results too, hence
/// LibreOffice's `lcb >= 58` check, which we keep.
pub(crate) fn parse_picf(buf: &[u8], fc: u32, ver67: bool) -> Option<Picf> {
    let at = fc as usize;
    if at.checked_add(58)? > buf.len() {
        return None;
    }
    let b = &buf[at..];
    let lcb = u32_at(b, 0);
    let mut cb_header = u16_at(b, 4);
    // 0x2E fixed part + 4 borders (2 bytes each in Word 6/95, 4 in Word 97)
    // + dxaOrigin + dyaOrigin (+ cProps in Word 97).
    let expected: u16 = if ver67 { 58 } else { 68 };
    if cb_header < 58 || u32::from(cb_header) > lcb || cb_header as usize > b.len() {
        cb_header = expected;
    }
    if lcb < 58 || lcb as usize > b.len() || lcb < u32::from(cb_header) {
        return None;
    }
    Some(Picf {
        lcb,
        cb_header,
        mm: i16_at(b, 6),
        dxa_goal: i16_at(b, 0x1C),
        dya_goal: i16_at(b, 0x1E),
        mx: u16_at(b, 0x20),
        my: u16_at(b, 0x22),
        crop_l: i16_at(b, 0x24),
        crop_t: i16_at(b, 0x26),
        crop_r: i16_at(b, 0x28),
        crop_b: i16_at(b, 0x2A),
    })
}

// ---------------------------------------------------------------------------
// The picture store of one document
// ---------------------------------------------------------------------------

/// A floating shape anchored in the text: where it hangs, which shape it is,
/// and the rectangle Word laid it out in (twips).
#[derive(Debug, Clone, Copy)]
struct Anchor {
    cp: u32,
    spid: u32,
    left: i32,
    top: i32,
    dx: i32,
    dy: i32,
    /// The `FSPA` bit field: wrapping mode and z-order live in there.
    flags: u16,
}

impl Anchor {
    /// Wrapping mode, in the vocabulary the DOCX importer already uses
    /// (`docx::read::drawing::parse_drawings`).
    fn wrap(&self) -> &'static str {
        let below = self.flags & 0x4000 != 0;
        match (self.flags >> 5) & 0x0F {
            // 3 = "wrap as if the object were not there".
            3 => {
                if below {
                    "behind"
                } else {
                    "front"
                }
            }
            // 1 = no text alongside the shape.
            1 => "topBottom",
            _ => "square",
        }
    }
}

/// Everything needed to resolve a picture reference into image bytes.
///
/// Built once per document; `inline_at` and `floating_at_cp` are then cheap.
pub(crate) struct Pictures {
    /// The `Data` stream, or the `WordDocument` stream when there is none —
    /// which is the fallback Word itself uses.
    data: Vec<u8>,
    /// The `WordDocument` stream. A blip store entry that points nowhere in
    /// `Data` very often points here instead: LibreOffice keeps exactly this
    /// pair of streams (`pStData` / `pStData2`) and tries them in that order.
    doc: Vec<u8>,
    /// Blip store of the drawing group, indexed by blip id − 1.
    blips: Vec<Option<RawImage>>,
    /// Shape id → blip id, from the Escher shape containers.
    spid_blip: HashMap<u32, u32>,
    /// Anchors of the floating shapes, from `PlcfSpaMom`.
    anchors: Vec<Anchor>,
    ver67: bool,
}

impl Pictures {
    /// Parse the drawing group and the shape anchors.
    ///
    /// `data` may be empty: a document with only Word 6 style pictures has no
    /// `Data` stream and everything lives in `WordDocument`.
    pub(crate) fn load(fib: &Fib, doc: &[u8], table: &[u8], data: Vec<u8>) -> Pictures {
        let ver67 = fib.nfib < 193;
        let data = if data.is_empty() { doc.to_vec() } else { data };
        let mut p = Pictures {
            data,
            doc: doc.to_vec(),
            blips: Vec::new(),
            spid_blip: HashMap::new(),
            anchors: Vec::new(),
            ver67,
        };
        // Word 6/95 has neither a drawing group nor an FSPA table; its
        // pictures are all inline and reached through sprmCPicLocation.
        if !ver67 {
            let (fc_dgg, lcb_dgg) = fc_lcb(doc, PAIR_DGG_INFO);
            let end = (fc_dgg as usize).saturating_add(lcb_dgg as usize).min(table.len());
            if (fc_dgg as usize) < end {
                let blob = table[fc_dgg as usize..end].to_vec();
                p.scan_escher(&blob, 0, blob.len(), 0);
            }
            let (fc_spa, lcb_spa) = fc_lcb(doc, PAIR_PLCF_SPA_MOM);
            p.anchors = parse_plcf_spa(table, fc_spa, lcb_spa);
        }
        p
    }

    /// Read the `Data` stream of an OLE2 container, tolerating its absence.
    ///
    /// Word only creates it when it has something to put there, so a missing
    /// stream is normal, not an error.
    pub(crate) fn read_data_stream(comp: &mut cfb::CompoundFile<Cursor<&[u8]>>) -> Vec<u8> {
        let Ok(mut s) = comp.open_stream("Data") else { return Vec::new() };
        let mut buf = Vec::new();
        if s.read_to_end(&mut buf).is_err() {
            return Vec::new();
        }
        buf
    }

    /// How many blips the drawing group declares — zero for a document whose
    /// pictures are all inline, so this is a hint, not a "has no picture" test.
    pub(crate) fn blip_count(&self) -> usize {
        self.blips.iter().flatten().count()
    }

    /// The image node for a run carrying `sprmCPicLocation` = `fc`.
    ///
    /// Returns `None` — never an empty node — when the reference points at
    /// something we cannot display: a linked file, an embedded OLE object, a
    /// form-field `FFDATA` (runs with `sprmCFData` set reuse the same offset
    /// for that), or a vector drawing with no bitmap inside.
    pub(crate) fn inline_at(&self, fc: u32) -> Option<PmNode> {
        let picf = parse_picf(&self.data, fc, self.ver67)?;
        let img = self.picture_bytes(&picf, fc)?;
        let (w, h) = picf.size_px();
        Some(image_node(&img.data_uri(), w, h))
    }

    /// The image node for the floating shape anchored at character position
    /// `cp` — the position of a 0x08 character in the main text.
    ///
    /// Anchored objects become block `image` nodes, not `inlineImage` ones,
    /// which is the split the DOCX importer already makes between
    /// `<wp:anchor>` and `<wp:inline>`.
    pub(crate) fn floating_at_cp(&self, cp: u32) -> Option<PmNode> {
        let a = self.anchors.iter().find(|a| a.cp == cp)?;
        let img = self.blip(*self.spid_blip.get(&a.spid)?)?;
        let px = |t: i32| (t as f64 / TWIPS_PER_PX).round();
        let (w, h) = if a.dx > 0 && a.dy > 0 {
            (px(a.dx).clamp(1.0, 4000.0), px(a.dy).clamp(1.0, 4000.0))
        } else {
            let (iw, ih) = img.pixel_size().unwrap_or((200, 150));
            (iw.max(1) as f64, ih.max(1) as f64)
        };
        let mut attrs = base_attrs(&img.data_uri(), w, h);
        attrs.insert("align".into(), json!("left"));
        attrs.insert("wrap".into(), json!(a.wrap()));
        attrs.insert("wrapX".into(), json!(px(a.left).max(0.0)));
        attrs.insert("wrapY".into(), json!(px(a.top)));
        Some(PmNode {
            node_type: "image".into(),
            attrs: Some(Value::Object(attrs)),
            content: None,
            marks: None,
            text: None,
        })
    }

    /// Image bytes for one `PICF`, whichever way the payload is stored.
    fn picture_bytes(&self, picf: &Picf, fc: u32) -> Option<RawImage> {
        let start = (fc as usize).checked_add(picf.cb_header as usize)?;
        let end = (fc as usize).checked_add(picf.lcb as usize)?.min(self.data.len());
        if start >= end {
            return None;
        }
        match picf.mm {
            // A file name, not a picture: nothing embedded to extract.
            94 | 99 => None,
            0x64 | 0x66 => {
                // 0x66 prefixes the shape with a Pascal-style name.
                let mut s = start;
                if picf.mm == 0x66 {
                    let n = *self.data.get(s)? as usize;
                    s = s.checked_add(1 + n)?;
                }
                if s >= end {
                    return None;
                }
                self.escher_image(&self.data[s..end])
            }
            _ => normalise(&self.data[start..end]),
        }
    }

    /// Find the image of an inline Escher shape: a blip embedded right there,
    /// or the `pib` property pointing into the drawing group's store.
    fn escher_image(&self, buf: &[u8]) -> Option<RawImage> {
        let mut found = None;
        let mut pib = None;
        find_inline_blip(buf, 0, buf.len(), 0, &mut found, &mut pib);
        found.or_else(|| pib.and_then(|i| self.blip(i)))
    }

    /// A blip of the drawing group store, by 1-based index.
    fn blip(&self, idx: u32) -> Option<RawImage> {
        if idx == 0 {
            return None;
        }
        self.blips.get(idx as usize - 1)?.clone()
    }

    /// Walk the drawing group: record every `BSE` of the blip store in order,
    /// and every shape's `spid` → `pib` mapping.
    fn scan_escher(&mut self, buf: &[u8], from: usize, to: usize, depth: u8) {
        if depth > 12 {
            return;
        }
        let mut i = from;
        while i + 8 <= to {
            let Some(r) = read_rec(buf, i) else { return };
            // `OfficeArtContent` is the drawing group followed by one
            // `OfficeArtWordDrawing` per subdocument, and each of those starts
            // with a single `dgglbl` byte before its container ([MS-DOC]
            // 2.9.172). That odd byte shifts everything after the group by one,
            // so the shapes — and with them every floating picture — vanish
            // unless the walk realigns here.
            if !plausible_fbt(r.fbt) {
                if plausible_fbt(u16_at(buf, i + 3)) {
                    i += 1;
                    continue;
                }
                return;
            }
            match r.fbt {
                fbt::BSE => {
                    let img = self.bse_image(&buf[r.body_start..r.body_end]);
                    self.blips.push(img);
                }
                fbt::SP_CONTAINER => {
                    self.scan_shape(buf, r.body_start, r.body_end);
                    self.scan_escher(buf, r.body_start, r.body_end, depth + 1);
                }
                fbt::DGG_CONTAINER | fbt::BSTORE_CONTAINER => {
                    self.scan_escher(buf, r.body_start, r.body_end, depth + 1)
                }
                _ if r.ver == 0x0F => self.scan_escher(buf, r.body_start, r.body_end, depth + 1),
                _ => {}
            }
            if r.next <= i {
                return;
            }
            i = r.next;
        }
    }

    /// The immediate children of a shape container: `FSP` gives the shape id,
    /// `OPT` the blip index of a picture fill.
    fn scan_shape(&mut self, buf: &[u8], from: usize, to: usize) {
        let (mut spid, mut pib) = (None, None);
        let mut i = from;
        while i + 8 <= to {
            let Some(r) = read_rec(buf, i) else { return };
            match r.fbt {
                fbt::FSP => spid = Some(u32_at(buf, r.body_start)),
                fbt::OPT => pib = pib.or(opt_pib(&buf[r.body_start..r.body_end], r.inst)),
                _ => {}
            }
            if r.next <= i {
                return;
            }
            i = r.next;
        }
        if let (Some(s), Some(p)) = (spid, pib) {
            self.spid_blip.insert(s, p);
        }
    }

    /// The blip of one `BSE`: embedded right after its 36-byte header, or at
    /// `foDelay` in the `Data` stream (msdffimp.cxx,
    /// `GetDrawingGroupContainerData`).
    fn bse_image(&self, body: &[u8]) -> Option<RawImage> {
        if body.len() < BSE_HEADER {
            return None;
        }
        let size = u32_at(body, 20) as usize;
        let fo_delay = u32_at(body, 28);
        // "If the blip is smaller than the record, it is inside the record" —
        // the same heuristic LibreOffice applies.
        if fo_delay == 0 || size < body.len() {
            if let Some(img) = blip_in(&body[BSE_HEADER..]) {
                return Some(img);
            }
        }
        if fo_delay != 0 {
            let at = fo_delay as usize;
            // `Data` first, `WordDocument` second — Word puts delayed blips in
            // either, and only trying one loses whole documents' pictures.
            return self
                .data
                .get(at..)
                .and_then(blip_in)
                .or_else(|| self.doc.get(at..).and_then(blip_in));
        }
        None
    }
}

/// Read a blip record sitting at the start of `buf`.
fn blip_in(buf: &[u8]) -> Option<RawImage> {
    let r = read_rec(buf, 0)?;
    if !(fbt::BLIP_FIRST..=fbt::BLIP_LAST).contains(&r.fbt) {
        return None;
    }
    blip_payload(r.inst, &buf[r.body_start..r.body_end])
}

/// Depth-first search for the first blip inside an inline shape, remembering
/// the `pib` property in case there is none embedded.
fn find_inline_blip(
    buf: &[u8],
    from: usize,
    to: usize,
    depth: u8,
    found: &mut Option<RawImage>,
    pib: &mut Option<u32>,
) {
    if depth > 12 || found.is_some() {
        return;
    }
    let mut i = from;
    while i + 8 <= to && found.is_none() {
        let Some(r) = read_rec(buf, i) else { return };
        if (fbt::BLIP_FIRST..=fbt::BLIP_LAST).contains(&r.fbt) {
            *found = blip_payload(r.inst, &buf[r.body_start..r.body_end]);
        } else if r.fbt == fbt::BSE && r.body_end > r.body_start + BSE_HEADER {
            find_inline_blip(buf, r.body_start + BSE_HEADER, r.body_end, depth + 1, found, pib);
        } else if r.fbt == fbt::OPT && pib.is_none() {
            *pib = opt_pib(&buf[r.body_start..r.body_end], r.inst);
        } else if r.ver == 0x0F {
            find_inline_blip(buf, r.body_start, r.body_end, depth + 1, found, pib);
        }
        if r.next <= i {
            return;
        }
        i = r.next;
    }
}

/// The blip index from an `msofbtOPT` property table: `inst` fixed-size
/// entries of `(id, value)`, complex values appended after them.
fn opt_pib(body: &[u8], inst: u16) -> Option<u32> {
    for k in 0..inst as usize {
        let at = k * 6;
        if at + 6 > body.len() {
            break;
        }
        if u16_at(body, at) & 0x3FFF == DFF_PROP_PIB {
            let v = u32_at(body, at + 2);
            return (v != 0).then_some(v);
        }
    }
    None
}

/// `PlcfSpaMom`: n+1 character positions then n 26-byte `FSPA` records.
fn parse_plcf_spa(table: &[u8], fc: u32, lcb: u32) -> Vec<Anchor> {
    let start = fc as usize;
    let end = start.saturating_add(lcb as usize).min(table.len());
    if start >= end || end - start < 4 + 26 {
        return Vec::new();
    }
    let plc = &table[start..end];
    let n = (plc.len() - 4) / 30;
    (0..n)
        .map(|i| {
            let f = 4 * (n + 1) + i * 26;
            let (l, t) = (u32_at(plc, f + 4) as i32, u32_at(plc, f + 8) as i32);
            let (r, b) = (u32_at(plc, f + 12) as i32, u32_at(plc, f + 16) as i32);
            Anchor {
                cp: u32_at(plc, i * 4),
                spid: u32_at(plc, f),
                left: l,
                top: t,
                dx: r.saturating_sub(l),
                dy: b.saturating_sub(t),
                flags: u16_at(plc, f + 20),
            }
        })
        .collect()
}

/// Pair index of `fcPlcfSpaMom` in `FibRgFcLcb97` (byte offset 0x01DA).
const PAIR_PLCF_SPA_MOM: usize = 40;
/// Pair index of `fcDggInfo` in `FibRgFcLcb97` (byte offset 0x022A).
const PAIR_DGG_INFO: usize = 50;

/// One `fc`/`lcb` pair of `FibRgFcLcb97`, by pair index.
///
/// The array starts after the self-describing `csw` / `cslw` / `cbRgFcLcb`
/// prefix, so its position does not have to be hard-coded either.
fn fc_lcb(doc: &[u8], pair: usize) -> (u32, u32) {
    let csw = u16_at(doc, 0x20) as usize;
    let base = 0x22 + csw * 2;
    let cslw = u16_at(doc, base) as usize;
    let fc_lcb_base = base + 2 + cslw * 4 + 2;
    let at = fc_lcb_base + pair * 8;
    (u32_at(doc, at), u32_at(doc, at + 4))
}

/// The attributes every image node carries, named exactly as the DOCX importer
/// names them (`docx::read::drawing`): `src` as a base64 data URI,
/// `width` / `height` in pixels, `alt`.
fn base_attrs(src: &str, w: f64, h: f64) -> serde_json::Map<String, Value> {
    let mut attrs = serde_json::Map::new();
    attrs.insert("width".into(), json!(w));
    attrs.insert("height".into(), json!(h));
    attrs.insert("src".into(), json!(src));
    attrs.insert("alt".into(), json!(""));
    attrs
}

/// An in-flow picture: the node `docx::read::drawing::inline_image_node`
/// produces for a `<wp:inline>` drawing.
fn image_node(src: &str, w: f64, h: f64) -> PmNode {
    let attrs = base_attrs(src, w, h);
    PmNode {
        node_type: "inlineImage".into(),
        attrs: Some(Value::Object(attrs)),
        content: None,
        marks: None,
        text: None,
    }
}

/// Every image the document holds, regardless of where the text references it.
///
/// The property layer is what normally drives extraction; this sweep exists so
/// coverage can be measured against another implementation, and as a safety
/// net for documents whose CHP references we fail to resolve. Duplicates are
/// removed: the same blip is often reachable twice.
pub(crate) fn all_images(p: &Pictures) -> Vec<RawImage> {
    let mut out: Vec<RawImage> = Vec::new();
    let mut seen: HashSet<(usize, u64)> = HashSet::new();
    let push = |img: RawImage, out: &mut Vec<RawImage>, seen: &mut HashSet<(usize, u64)>| {
        if seen.insert(img.fingerprint()) {
            out.push(img);
        }
    };

    // The pictures the text points at: a chain of PICF records from the start
    // of the Data stream, each one giving the offset of the next.
    let mut fc = 0usize;
    while fc + 58 <= p.data.len() {
        let Some(picf) = parse_picf(&p.data, fc as u32, p.ver67) else { break };
        if let Some(img) = p.picture_bytes(&picf, fc as u32) {
            push(img, &mut out, &mut seen);
        }
        fc += (picf.lcb as usize).max(58);
    }

    // Blips the chain missed — a `Data` stream also holds form-field data and
    // OLE descriptors, which desynchronise the walk above, and delayed blips
    // may sit in `WordDocument` instead.
    for buf in [&p.data, &p.doc] {
        let mut i = 0usize;
        while i + 8 <= buf.len() {
            let fbt = u16_at(buf, i + 2);
            if (fbt::BLIP_FIRST..=fbt::BLIP_LAST).contains(&fbt) {
                if let Some(img) = blip_in(&buf[i..]) {
                    push(img, &mut out, &mut seen);
                }
            }
            // Byte granularity, not word: Word does not align the records it
            // appends to `Data`, and stepping by two silently halves the count.
            i += 1;
        }
    }

    for b in p.blips.iter().flatten() {
        push(b.clone(), &mut out, &mut seen);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Open a `.doc` the way `import_doc` does and build its picture store.
    #[cfg(test)]
    fn pictures_of(bytes: &[u8]) -> Option<Pictures> {
        let mut comp = cfb::CompoundFile::open(Cursor::new(bytes)).ok()?;
        let read = |c: &mut cfb::CompoundFile<Cursor<&[u8]>>, n: &str| -> Option<Vec<u8>> {
            let mut s = c.open_stream(n).ok()?;
            let mut b = Vec::new();
            s.read_to_end(&mut b).ok()?;
            Some(b)
        };
        let doc = read(&mut comp, "WordDocument")?;
        let fib = super::super::fib::Fib::parse(&doc).ok()?;
        if fib.encrypted {
            return None;
        }
        let table = read(&mut comp, fib.table_stream_name()).unwrap_or_default();
        let data = Pictures::read_data_stream(&mut comp);
        Some(Pictures::load(&fib, &doc, &table, data))
    }

    #[test]
    fn dib_gets_a_bmp_header() {
        // 2×2 24-bit DIB: header then two rows of 8 bytes (6 + 2 of padding).
        let mut dib = vec![0u8; 40 + 16];
        dib[0] = 40;
        dib[4] = 2; // width
        dib[8] = 2; // height
        dib[12] = 1; // planes
        dib[14] = 24; // bit count
        let img = dib_to_bmp(&dib).expect("un DIB valide doit produire un BMP");
        assert_eq!(img.mime, "image/bmp");
        assert_eq!(&img.bytes[0..2], b"BM");
        // No palette at 24 bpp: the pixels start right after the two headers.
        assert_eq!(u32_at(&img.bytes, 10), 54);
        assert_eq!(img.pixel_size(), Some((2, 2)));
    }

    #[test]
    fn palette_is_counted_in_the_pixel_offset() {
        let mut dib = vec![0u8; 40 + 256 * 4 + 16];
        dib[0] = 40;
        dib[4] = 4;
        dib[8] = 4;
        dib[12] = 1;
        dib[14] = 8; // 8 bpp → 256 palette entries
        let img = dib_to_bmp(&dib).expect("DIB 8 bits");
        assert_eq!(u32_at(&img.bytes, 10), 14 + 40 + 1024);
    }

    #[test]
    fn junk_is_not_mistaken_for_a_bitmap() {
        assert!(dib_to_bmp(&[0xAB; 128]).is_none());
        assert!(normalise(&[0u8; 64]).is_none());
        assert!(normalise(&[]).is_none());
        assert!(blip_in(&[0u8; 64]).is_none());
    }

    #[test]
    fn picf_header_length_is_read_not_assumed() {
        let mut buf = vec![0u8; 200];
        buf[0..4].copy_from_slice(&120u32.to_le_bytes()); // lcb
        buf[4..6].copy_from_slice(&68u16.to_le_bytes()); // cbHeader
        buf[6..8].copy_from_slice(&100i16.to_le_bytes()); // mm = Escher
        buf[0x1C..0x1E].copy_from_slice(&1440i16.to_le_bytes()); // one inch
        buf[0x1E..0x20].copy_from_slice(&720i16.to_le_bytes()); // half an inch
        buf[0x20..0x22].copy_from_slice(&500u16.to_le_bytes()); // 50 %
        buf[0x22..0x24].copy_from_slice(&1000u16.to_le_bytes());
        let p = parse_picf(&buf, 0, false).expect("PICF valide");
        assert_eq!(p.cb_header, 68);
        // 1440 twips × 50 % = 720 twips = 48 px; 720 twips × 100 % = 48 px.
        assert_eq!(p.size_px(), (48.0, 48.0));
    }

    #[test]
    fn a_nonsense_header_length_falls_back_to_the_version_default() {
        let mut buf = vec![0u8; 200];
        buf[0..4].copy_from_slice(&120u32.to_le_bytes());
        buf[4..6].copy_from_slice(&3u16.to_le_bytes()); // impossible
        assert_eq!(parse_picf(&buf, 0, false).expect("PICF").cb_header, 68);
        assert_eq!(parse_picf(&buf, 0, true).expect("PICF").cb_header, 58);
    }

    #[test]
    fn cropping_is_taken_off_before_scaling() {
        let mut buf = vec![0u8; 200];
        buf[0..4].copy_from_slice(&120u32.to_le_bytes());
        buf[4..6].copy_from_slice(&68u16.to_le_bytes());
        buf[0x1C..0x1E].copy_from_slice(&1500i16.to_le_bytes());
        buf[0x1E..0x20].copy_from_slice(&1500i16.to_le_bytes());
        buf[0x20..0x22].copy_from_slice(&1000u16.to_le_bytes());
        buf[0x22..0x24].copy_from_slice(&1000u16.to_le_bytes());
        buf[0x24..0x26].copy_from_slice(&300i16.to_le_bytes()); // crop left
        buf[0x28..0x2A].copy_from_slice(&300i16.to_le_bytes()); // crop right
        let p = parse_picf(&buf, 0, false).expect("PICF");
        // (1500 − 600) / 15 = 60 px wide, 1500 / 15 = 100 px tall.
        assert_eq!(p.size_px(), (60.0, 100.0));
    }

    #[test]
    fn the_node_matches_the_docx_convention() {
        let n = image_node("data:image/png;base64,AAA", 120.0, 90.0);
        assert_eq!(n.node_type, "inlineImage");
        let a = n.attrs.expect("attributs");
        assert_eq!(a["width"], json!(120.0));
        assert_eq!(a["height"], json!(90.0));
        assert_eq!(a["alt"], json!(""));
        assert!(a["src"].as_str().unwrap().starts_with("data:image/png;base64,"));
    }

    /// Run over real Word binaries: every byte string we hand out must really
    /// be an image (magic number *and* a readable size), and we report how
    /// many we found so the count can be compared with another importer.
    #[test]
    fn corpus_images_are_real_images() {
        let Ok(root) = std::env::var("KUBUNO_DOC_CORPUS") else {
            eprintln!("KUBUNO_DOC_CORPUS absent — test ignoré");
            return;
        };
        let (mut files, mut with_img, mut total) = (0usize, 0usize, 0usize);
        let mut by_mime: HashMap<&str, usize> = HashMap::new();
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
                files += 1;
                let Ok(bytes) = std::fs::read(&p) else { continue };
                let Some(pics) = pictures_of(&bytes) else { continue };
                let imgs = all_images(&pics);
                // Per-file counts, for comparing the sweep against another
                // importer; off by default because it is 358 lines of noise.
                if std::env::var_os("KUBUNO_DOC_IMG_LIST").is_some() {
                    eprintln!("IMGCOUNT\t{}\t{}", imgs.len(), p.display());
                }
                if !imgs.is_empty() {
                    with_img += 1;
                }
                for img in &imgs {
                    total += 1;
                    *by_mime.entry(img.mime).or_default() += 1;
                    assert!(img.bytes.len() > 16, "image vide dans {}", p.display());
                    // The magic number must match the type we announced.
                    let sniffed = normalise(&img.bytes).map(|r| r.mime);
                    assert_eq!(
                        sniffed,
                        Some(img.mime),
                        "octets non reconnus comme {} dans {}",
                        img.mime,
                        p.display()
                    );
                    if let Some((w, h)) = img.pixel_size() {
                        assert!(
                            w > 0 && h > 0 && w < 30_000 && h < 30_000,
                            "dimensions absurdes {w}×{h} dans {}",
                            p.display()
                        );
                    }
                }
            }
        }
        let mut kinds: Vec<_> = by_mime.into_iter().collect();
        kinds.sort();
        eprintln!(
            "corpus .doc images: {total} images dans {with_img}/{files} fichiers — {kinds:?}"
        );
        assert!(files > 0, "aucun .doc trouvé dans KUBUNO_DOC_CORPUS");
    }
}
