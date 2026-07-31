//! Revision marks — what Word calls "tracked changes" — of a WW8 document.
//!
//! A `.doc` records a revision as three separate character properties that all
//! land on the same character run, never as a structure of its own:
//!
//!   * a flag, `sprmCFRMarkIns` (0x0801) for inserted text and
//!     `sprmCFRMarkDel` (0x0800) for deleted text — deleted text STAYS in the
//!     character stream, which is exactly the model the shared contract wants;
//!   * an author, `sprmCIbstRMark` (0x4804) / `sprmCIbstRMarkDel` (0x4863) —
//!     an INDEX, not a name: it points into `SttbfRMark`, a string table of the
//!     table stream. Without that table every revision is anonymous;
//!   * a timestamp, `sprmCDttmRMark` (0x6805) / `sprmCDttmRMarkDel` (0x6864) —
//!     a `DTTM`, thirty-two bits of packed bit fields, not a Unix time.
//!
//! Word 6/95 uses the same three properties under its 8-bit opcodes (66, 69,
//! 70, plus 65 for the deletion flag, historically named `sprmCFStrikeRM`) and
//! has no separate "…Del" author/date pair at all.
//!
//! Reference: LibreOffice `SwWW8ImplReader::Read_CRevisionMark` and
//! `ReadRevMarkAuthorStrTabl` (ww8par4.cxx), `WW8ReadSTTBF` (ww8scan.cxx),
//! `msfilter::util::DTTM2DateTime` (filter/source/msfilter/util.cxx), and
//! [MS-DOC] 2.6.1 / 2.9.87.

use encoding_rs::Encoding;
use serde_json::json;

use crate::converters::types::PmMark;

use super::fib::{u16_at, u32_at, Fib};
use super::fkp::DocProps;
use super::piece::charset_for;
use super::sprm::{ww8, SprmIter, SprmVersion};

// ---------------------------------------------------------------------------
// Where the tables live
// ---------------------------------------------------------------------------

/// `fcSttbfRMark` in the FIB's `rgFcLcb` array, counted in `(fc, lcb)` PAIRS.
///
/// The array is a list of pairs, so pair *k* starts at the 32-bit word `2k` —
/// reading it as a flat list of `fc`s silently lands on a neighbouring table.
/// Counted from LibreOffice's read order in `WW8Fib::WW8Fib` (ww8scan.cxx):
/// `fcStshfOrig` is pair 0, `fcStshf` 1, `fcPlcfsed` 6, `fcSttbfffn` 15,
/// `fcClx` 33, `fcDggInfo` 50, `fcSttbfRMark` 51. Verified against the
/// LibreOffice corpus: at pair 51 the blob always starts with `FFFF`, the
/// extended-STTB marker, and decodes to author names.
const PAIR_STTBF_RMARK: usize = 51;

/// `fcDop`, which holds the document-wide "revision marking is on" flag.
const PAIR_DOP: usize = 31;

/// Word 6/95 has no self-describing FIB prefix: the same two entries sit at
/// fixed offsets (LibreOffice `ww8scan.hxx`, whose offset comments describe the
/// Ver67 layout). UNVERIFIED — no Word 6/95 file of the corpus carries
/// revisions, so this path is written from the reference implementation alone.
const WW6_FC_STTBF_RMARK: usize = 0x1FA;
const WW6_FC_DOP: usize = 0x150;

/// Byte 5 of `DopBase`, bit 0x80: `fRevMarking`, "changes are being tracked".
/// This is the document setting the contract calls `trackChanges`, not a
/// property of any individual revision.
const DOP_BYTE_REV_MARKING: usize = 5;
const DOP_BIT_REV_MARKING: u8 = 0x80;

/// Read one `(fc, lcb)` pair of the FIB.
///
/// Word 97+ prefixes `rgFcLcb` with the `csw` / `cslw` / `cbRgFcLcb` counters
/// that make it self-describing; older versions have a fixed layout, and their
/// pairs are not at a computable index because the FIB interleaves non-pair
/// fields, so the caller passes the byte offset instead.
fn fib_entry(doc: &[u8], ver: SprmVersion, pair: usize, ww6_off: usize) -> (u32, u32) {
    if ver == SprmVersion::Ww8 {
        let csw = u16_at(doc, 0x20) as usize;
        let after_sw = 0x22 + csw * 2;
        let cslw = u16_at(doc, after_sw) as usize;
        let base = after_sw + 2 + cslw * 4 + 2;
        let at = base + pair * 8;
        (u32_at(doc, at), u32_at(doc, at + 4))
    } else {
        (u32_at(doc, ww6_off), u32_at(doc, ww6_off + 4))
    }
}

/// Slice a table-stream blob, tolerating a truncated or out-of-range entry.
fn blob(table: &[u8], fc: u32, lcb: u32) -> &[u8] {
    let start = fc as usize;
    let end = start.saturating_add(lcb as usize).min(table.len());
    table.get(start..end).unwrap_or(&[])
}

// ---------------------------------------------------------------------------
// The author table
// ---------------------------------------------------------------------------

/// Decode `SttbfRMark`: the author names a revision's `ibst` indexes into.
///
/// Word 97 writes the extended (UTF-16) form, whose header is `FFFF`, a count
/// of strings and a per-entry extra-data length; Word 6/95 writes the total
/// byte count followed by 8-bit Pascal strings with two bytes of extra data
/// each. Entry 0 is Word's own placeholder ("Unknown") and is kept: an `ibst`
/// of 0 must resolve to it rather than to the first real author.
fn parse_sttbf_rmark(sttb: &[u8], ver: SprmVersion, enc: &'static Encoding) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if sttb.len() < 2 {
        return out;
    }
    if ver != SprmVersion::Ww8 {
        // Ver67: a total length, then `cch`-prefixed 8-bit strings, each
        // followed by two bytes of extra data the FIB does not describe.
        let total = (u16_at(sttb, 0) as usize).min(sttb.len());
        let mut p = 2usize;
        while p < total && out.len() < 0x4000 {
            let cch = sttb[p] as usize;
            p += 1;
            let end = p.saturating_add(cch).min(sttb.len());
            out.push(enc.decode(sttb.get(p..end).unwrap_or(&[])).0.into_owned());
            p = end.saturating_add(2);
        }
        return out;
    }

    let extended = u16_at(sttb, 0) == 0xFFFF;
    let (count, extra, mut p) = if extended {
        (u16_at(sttb, 2) as usize, u16_at(sttb, 4) as usize, 6usize)
    } else {
        (u16_at(sttb, 0) as usize, u16_at(sttb, 2) as usize, 4usize)
    };
    // A corrupt count must not make us loop forever or allocate wildly: every
    // entry costs at least one length unit, so the buffer bounds it.
    for _ in 0..count.min(sttb.len()) {
        if p >= sttb.len() {
            break;
        }
        let (name, next) = if extended {
            let cch = u16_at(sttb, p) as usize;
            let from = p + 2;
            let to = from.saturating_add(cch * 2).min(sttb.len());
            (utf16(sttb.get(from..to).unwrap_or(&[])), to)
        } else {
            let cch = sttb[p] as usize;
            let from = p + 1;
            let to = from.saturating_add(cch).min(sttb.len());
            (
                enc.decode(sttb.get(from..to).unwrap_or(&[])).0.into_owned(),
                to,
            )
        };
        out.push(name);
        p = next.saturating_add(extra);
    }
    out
}

fn utf16(b: &[u8]) -> String {
    let units: Vec<u16> = b
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}

// ---------------------------------------------------------------------------
// DTTM
// ---------------------------------------------------------------------------

/// A `DTTM` — Word's packed date — as an ISO-8601 instant.
///
/// Bit fields, low to high: minute (6), hour (5), day of month (5), month (4),
/// year since 1900 (9), day of week (3, redundant and ignored). Zero means "no
/// date", which Word writes freely, so it maps to `None` rather than to the
/// year 1900. The stored time is local and carries no zone; like OOXML's
/// `w:date` it is emitted as if it were UTC — inventing an offset would be
/// worse than admitting there is none.
pub(crate) fn dttm_to_iso(dttm: u32) -> Option<String> {
    if dttm == 0 {
        return None;
    }
    let minute = dttm & 0x3F;
    let hour = (dttm >> 6) & 0x1F;
    let day = (dttm >> 11) & 0x1F;
    let month = (dttm >> 16) & 0x0F;
    let year = ((dttm >> 20) & 0x1FF) + 1900;
    // Reject impossible calendars (31 February, month 0, hour 24…) instead of
    // emitting a date no parser will accept.
    let date = chrono::NaiveDate::from_ymd_opt(i32::try_from(year).ok()?, month, day)?;
    date.and_hms_opt(hour, minute, 0)?;
    Some(format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:00Z"))
}

// ---------------------------------------------------------------------------
// Reading the sprms of one character run
// ---------------------------------------------------------------------------

/// The revision opcodes of one encoding.
struct Ops {
    f_ins: u16,
    f_del: u16,
    ibst_ins: u16,
    dttm_ins: u16,
    ibst_del: u16,
    dttm_del: u16,
}

/// Word 6/95 opcode numbers, from LibreOffice `sprmids.hxx`: `sprmCFStrikeRM`
/// 65 is the deletion flag, `sprmCFRMark` 66 the insertion flag,
/// `sprmCIbstRMark` 69 the author, `sprmCDttmRMark` 70 the date. That encoding
/// has no separate author/date for deletions.
const WW6_NONE: u16 = u16::MAX;

fn ops(ver: SprmVersion) -> Ops {
    if ver == SprmVersion::Ww8 {
        Ops {
            f_ins: ww8::CFRMARKINS,
            f_del: ww8::CFRMARKDEL,
            ibst_ins: ww8::CIBSTRMARK,
            dttm_ins: ww8::CDTTMRMARK,
            ibst_del: ww8::CIBSTRMARKDEL,
            dttm_del: ww8::CDTTMRMARKDEL,
        }
    } else {
        Ops {
            f_ins: 66,
            f_del: 65,
            ibst_ins: 69,
            dttm_ins: 70,
            ibst_del: WW6_NONE,
            dttm_del: WW6_NONE,
        }
    }
}

/// What one `grpprl` says about revisions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct RunRevision {
    inserted: bool,
    deleted: bool,
    ibst_ins: Option<u16>,
    dttm_ins: Option<u32>,
    ibst_del: Option<u16>,
    dttm_del: Option<u32>,
}

/// Decode the revision properties of a single character `grpprl`.
///
/// Later sprms win over earlier ones, the way Word resolves a duplicate inside
/// one property list — and duplicates do happen here: Word has been observed
/// writing several `sprmCDttmRMark` for one run, which is why LibreOffice takes
/// "the last one as the true one" (ww8par4.cxx).
fn run_revision(grpprl: &[u8], ver: SprmVersion) -> RunRevision {
    let o = ops(ver);
    let mut r = RunRevision::default();
    for s in SprmIter::new(grpprl, ver) {
        match s.opcode {
            c if c == o.f_ins => r.inserted = s.toggle(false).unwrap_or(r.inserted),
            c if c == o.f_del => r.deleted = s.toggle(false).unwrap_or(r.deleted),
            c if c == o.ibst_ins => r.ibst_ins = s.u16().or(r.ibst_ins),
            c if c == o.dttm_ins => r.dttm_ins = s.u32().or(r.dttm_ins),
            c if c == o.ibst_del && c != WW6_NONE => r.ibst_del = s.u16().or(r.ibst_del),
            c if c == o.dttm_del && c != WW6_NONE => r.dttm_del = s.u32().or(r.dttm_del),
            _ => {}
        }
    }
    r
}

// ---------------------------------------------------------------------------
// Spans
// ---------------------------------------------------------------------------

/// Which of the two marks of the contract a span carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RevKind {
    /// Text added while tracking was on.
    Insertion,
    /// Text removed while tracking was on. The characters are still in the
    /// stream and must stay in the document, carrying this mark.
    Deletion,
}

impl RevKind {
    /// The ProseMirror mark name of the shared contract.
    pub(crate) fn mark_name(self) -> &'static str {
        match self {
            RevKind::Insertion => "insertion",
            RevKind::Deletion => "deletion",
        }
    }
}

/// One revision, over a contiguous range of character positions.
#[derive(Debug, Clone)]
pub(crate) struct RevisionSpan {
    pub(crate) kind: RevKind,
    /// First character position covered.
    pub(crate) cp_start: u32,
    /// One past the last character position covered.
    pub(crate) cp_end: u32,
    /// Displayable author name; empty when the document does not name one.
    pub(crate) author: String,
    /// ISO-8601 timestamp, absent when the file stores none.
    pub(crate) date: Option<String>,
    /// Identifier of the change, for accept/reject. A `.doc` stores no such id,
    /// so it is minted here: sequential in reading order, stable for a given
    /// file, and numeric so it can be written straight back as an OOXML `w:id`.
    pub(crate) id: String,
}

impl RevisionSpan {
    /// This span as the ProseMirror mark the contract describes.
    pub(crate) fn to_mark(&self) -> PmMark {
        PmMark {
            mark_type: self.kind.mark_name().into(),
            attrs: Some(json!({
                "author": self.author,
                // A `.doc` identifies its authors by name only: there is no
                // stable account id to carry over, and inventing one would make
                // two people with the same name look like the same account.
                "authorId": "",
                "date": self.date.clone().unwrap_or_default(),
                "id": self.id,
            })),
        }
    }
}

/// A set of non-overlapping spans of one kind, queryable by character position.
#[derive(Debug, Clone, Default)]
struct SpanList(Vec<RevisionSpan>);

impl SpanList {
    fn at(&self, cp: u32) -> Option<&RevisionSpan> {
        let i = self.0.partition_point(|s| s.cp_start <= cp);
        let s = self.0.get(i.checked_sub(1)?)?;
        (cp < s.cp_end).then_some(s)
    }
}

// ---------------------------------------------------------------------------
// The document's revisions
// ---------------------------------------------------------------------------

/// Every tracked change of a document, resolved to author and date.
///
/// Built once per file and queried by character position, the way `assemble`
/// queries every other layer.
#[derive(Debug, Clone, Default)]
pub(crate) struct Revisions {
    authors: Vec<String>,
    insertions: SpanList,
    deletions: SpanList,
    track_changes: bool,
}

impl Revisions {
    /// Decode the revisions of a document.
    ///
    /// `doc` is the `WordDocument` stream (for the FIB), `table` the stream
    /// named by [`Fib::table_stream_name`], and `props` the already-decoded
    /// property bins — the revision flags live in the character `grpprl`s, so
    /// there is nothing to read twice.
    pub(crate) fn parse(
        fib: &Fib,
        doc: &[u8],
        table: &[u8],
        props: &DocProps,
        ver: SprmVersion,
    ) -> Revisions {
        let (fc, lcb) = fib_entry(doc, ver, PAIR_STTBF_RMARK, WW6_FC_STTBF_RMARK);
        let authors = parse_sttbf_rmark(blob(table, fc, lcb), ver, charset_for(fib));

        let (dop_fc, dop_lcb) = fib_entry(doc, ver, PAIR_DOP, WW6_FC_DOP);
        let dop = blob(table, dop_fc, dop_lcb);
        let track_changes = dop
            .get(DOP_BYTE_REV_MARKING)
            .is_some_and(|b| b & DOP_BIT_REV_MARKING != 0);

        let mut revs = Revisions {
            authors,
            track_changes,
            ..Default::default()
        };
        revs.collect_spans(props, ver);
        revs
    }

    /// Walk the character property runs and fold them into revision spans.
    ///
    /// Two adjacent runs belong to the same change when they agree on author
    /// and timestamp AND touch: a change is normally split across several CHPX
    /// runs simply because the formatting changed inside it, and emitting one
    /// mark per run would make accept/reject act on fragments.
    fn collect_spans(&mut self, props: &DocProps, ver: SprmVersion) {
        // (kind, author index, timestamp) per run, then merged.
        let mut raw: Vec<(RevKind, u32, u32, u16, u32)> = Vec::new();
        for run in props.chars.runs() {
            if run.cp_end <= run.cp_start {
                continue;
            }
            let r = run_revision(&run.grpprl, ver);
            if r.inserted {
                let ibst = r.ibst_ins.unwrap_or(0);
                let dttm = r.dttm_ins.unwrap_or(0);
                raw.push((RevKind::Insertion, run.cp_start, run.cp_end, ibst, dttm));
            }
            if r.deleted {
                // The "…Del" pair is what Word writes for a deletion; when it is
                // missing — some producers only write the plain pair — falling
                // back to it beats reporting every deletion as anonymous.
                let ibst = r.ibst_del.or(r.ibst_ins).unwrap_or(0);
                let dttm = r.dttm_del.or(r.dttm_ins).unwrap_or(0);
                raw.push((RevKind::Deletion, run.cp_start, run.cp_end, ibst, dttm));
            }
        }
        raw.sort_by_key(|(k, start, _, _, _)| (*start, *k == RevKind::Deletion));

        // Merge, then number: ids follow reading order, which is what a user
        // navigating "next change" expects.
        let mut merged: Vec<(RevKind, u32, u32, u16, u32)> = Vec::with_capacity(raw.len());
        for item in raw {
            match merged.last_mut() {
                Some(prev)
                    if prev.0 == item.0 && prev.2 == item.1 && prev.3 == item.3 && prev.4 == item.4 =>
                {
                    prev.2 = item.2;
                }
                _ => merged.push(item),
            }
        }

        for (n, (kind, cp_start, cp_end, ibst, dttm)) in merged.into_iter().enumerate() {
            let span = RevisionSpan {
                kind,
                cp_start,
                cp_end,
                author: self.author_name(ibst).to_string(),
                date: dttm_to_iso(dttm),
                id: (n + 1).to_string(),
            };
            match kind {
                RevKind::Insertion => self.insertions.0.push(span),
                RevKind::Deletion => self.deletions.0.push(span),
            }
        }
    }

    /// Name of author `ibst`, or an empty string when the table does not have
    /// one — an unattributed change is legal and must not be an error.
    pub(crate) fn author_name(&self, ibst: u16) -> &str {
        self.authors.get(ibst as usize).map_or("", |s| s.as_str())
    }

    /// Every author the document declares, index by `ibst`. Entry 0 is Word's
    /// own placeholder.
    pub(crate) fn authors(&self) -> &[String] {
        &self.authors
    }

    /// Whether the document had change tracking switched ON when it was saved
    /// (`Dop.fRevMarking`) — the `trackChanges` setting of the contract.
    pub(crate) fn track_changes(&self) -> bool {
        self.track_changes
    }

    /// No revision anywhere in the document.
    pub(crate) fn is_empty(&self) -> bool {
        self.insertions.0.is_empty() && self.deletions.0.is_empty()
    }

    /// How many distinct changes were recovered, insertions and deletions.
    pub(crate) fn len(&self) -> usize {
        self.insertions.0.len() + self.deletions.0.len()
    }

    /// The insertion covering `cp`, if any.
    pub(crate) fn insertion_at(&self, cp: u32) -> Option<&RevisionSpan> {
        self.insertions.at(cp)
    }

    /// The deletion covering `cp`, if any.
    pub(crate) fn deletion_at(&self, cp: u32) -> Option<&RevisionSpan> {
        self.deletions.at(cp)
    }

    /// The marks to put on the character at `cp`.
    ///
    /// This is what the assembly calls. A character can be both: text somebody
    /// inserted and somebody else then deleted carries the two marks at once.
    pub(crate) fn marks_at(&self, cp: u32) -> Vec<PmMark> {
        let mut out = Vec::new();
        if let Some(s) = self.insertion_at(cp) {
            out.push(s.to_mark());
        }
        if let Some(s) = self.deletion_at(cp) {
            out.push(s.to_mark());
        }
        out
    }

    /// All spans in reading order — for callers that want the change list
    /// itself rather than per-character marks.
    pub(crate) fn spans(&self) -> Vec<&RevisionSpan> {
        let mut all: Vec<&RevisionSpan> =
            self.insertions.0.iter().chain(self.deletions.0.iter()).collect();
        all.sort_by_key(|s| (s.cp_start, s.kind == RevKind::Deletion));
        all
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dttm_decodes_word_bit_fields() {
        // 2005-06-14 09:30, Tuesday. Built field by field, low to high.
        let dttm = 30 | (9 << 6) | (14 << 11) | (6 << 16) | ((2005 - 1900) << 20) | (2u32 << 29);
        assert_eq!(dttm_to_iso(dttm).as_deref(), Some("2005-06-14T09:30:00Z"));
    }

    #[test]
    fn dttm_zero_and_impossible_dates_are_not_errors() {
        assert_eq!(dttm_to_iso(0), None);
        // 31 February: a real value seen in damaged files.
        let bad = (31 << 11) | (2 << 16) | ((2000 - 1900) << 20);
        assert_eq!(dttm_to_iso(bad), None);
        // Hour 24 / minute 63 cannot exist either.
        let bad = 63 | (24 << 6) | (1 << 11) | (1 << 16) | ((2000 - 1900) << 20);
        assert_eq!(dttm_to_iso(bad), None);
    }

    #[test]
    fn sttbf_rmark_reads_the_extended_form() {
        // FFFF · 2 strings · 0 extra · "Ann" · "Bo"
        let mut b = vec![0xFF, 0xFF, 0x02, 0x00, 0x00, 0x00];
        b.extend_from_slice(&[0x03, 0x00]);
        b.extend_from_slice("Ann".encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<_>>().as_slice());
        b.extend_from_slice(&[0x02, 0x00]);
        b.extend_from_slice("Bo".encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<_>>().as_slice());
        let names = parse_sttbf_rmark(&b, SprmVersion::Ww8, encoding_rs::WINDOWS_1252);
        assert_eq!(names, vec!["Ann".to_string(), "Bo".to_string()]);
    }

    #[test]
    fn sttbf_rmark_survives_a_lying_count() {
        // Claims 40000 strings in eight bytes.
        let b = vec![0xFF, 0xFF, 0x40, 0x9C, 0x00, 0x00, 0x01, 0x00];
        let names = parse_sttbf_rmark(&b, SprmVersion::Ww8, encoding_rs::WINDOWS_1252);
        assert!(names.len() <= 2, "{names:?}");
    }

    #[test]
    fn run_revision_reads_flag_author_and_date() {
        // sprmCFRMarkIns=1 · sprmCIbstRMark=3 · sprmCDttmRMark
        let dttm: u32 = 30 | (9 << 6) | (14 << 11) | (6 << 16) | ((2005 - 1900) << 20);
        let mut g = vec![0x01, 0x08, 0x01, 0x04, 0x48, 0x03, 0x00, 0x05, 0x68];
        g.extend_from_slice(&dttm.to_le_bytes());
        let r = run_revision(&g, SprmVersion::Ww8);
        assert!(r.inserted && !r.deleted);
        assert_eq!(r.ibst_ins, Some(3));
        assert_eq!(r.dttm_ins, Some(dttm));
    }

    /// The exact bytes Word wrote for run `[11..21)` of `tdf122452.doc`: text
    /// Author2 inserted and Author1 then deleted. One run carrying both marks,
    /// each with its OWN author, is the case a naive reader collapses into one.
    #[test]
    fn a_run_can_be_inserted_and_deleted_at_once() {
        let g = [
            0x01, 0x08, 0x01, // sprmCFRMarkIns = 1
            0x04, 0x48, 0x02, 0x00, // sprmCIbstRMark = 2 (Author2)
            0x05, 0x68, 0x00, 0x00, 0x00, 0x00, // sprmCDttmRMark = 0 (no date)
            0x00, 0x08, 0x01, // sprmCFRMarkDel = 1
            0x63, 0x48, 0x01, 0x00, // sprmCIbstRMarkDel = 1 (Author1)
            0x64, 0x68, 0x00, 0x00, 0x00, 0x00, // sprmCDttmRMarkDel = 0
        ];
        let r = run_revision(&g, SprmVersion::Ww8);
        assert!(r.inserted && r.deleted);
        assert_eq!((r.ibst_ins, r.ibst_del), (Some(2), Some(1)));
        assert_eq!((r.dttm_ins, r.dttm_del), (Some(0), Some(0)));
    }

    #[test]
    fn run_revision_ignores_junk() {
        for g in [vec![], vec![0x00], vec![0xFF; 9], vec![0x01, 0x08]] {
            let _ = run_revision(&g, SprmVersion::Ww8);
            let _ = run_revision(&g, SprmVersion::Ww6);
        }
    }

    // -----------------------------------------------------------------------
    // Corpus
    // -----------------------------------------------------------------------

    /// Rebuild just enough of the pipeline to reach the revisions: the reader
    /// is not wired into `import_doc` yet, so this test does the plumbing.
    fn revisions_of(bytes: &[u8]) -> Option<Revisions> {
        decoded(bytes).map(|(r, _, _)| r)
    }

    #[allow(clippy::type_complexity)]
    fn decoded(bytes: &[u8]) -> Option<(Revisions, DocProps, Vec<char>)> {
        use super::super::{fkp, piece};
        use std::io::{Cursor, Read};
        let mut comp = cfb::CompoundFile::open(Cursor::new(bytes)).ok()?;
        let read = |c: &mut cfb::CompoundFile<Cursor<&[u8]>>, n: &str| -> Vec<u8> {
            let mut buf = Vec::new();
            if let Ok(mut s) = c.open_stream(n) {
                let _ = s.read_to_end(&mut buf);
            }
            buf
        };
        let doc = read(&mut comp, "WordDocument");
        let fib = Fib::parse(&doc).ok()?;
        if fib.encrypted {
            return None;
        }
        let table = read(&mut comp, fib.table_stream_name());
        let data = read(&mut comp, "Data");
        let pieces = piece::parse_pieces(&fib, &table);
        let props = fkp::read_props(&fib, &doc, &table, &data, &pieces);
        let ver = SprmVersion::from_fib(&fib);
        let revs = Revisions::parse(&fib, &doc, &table, &props, ver);
        let enc = piece::charset_for(&fib);
        let text = piece::read_text(&fib, &doc, &pieces, enc);
        Some((revs, props, text.chars))
    }

    fn corpus_files() -> Vec<std::path::PathBuf> {
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

    /// Diagnostic: what exactly was decoded out of one file, to be held against
    /// `soffice --headless --convert-to fodt` (`text:tracked-changes`). Run with
    /// `cargo test --release revisions::tests::dump -- --ignored --nocapture
    /// <chemin.doc>`.
    #[test]
    #[ignore = "diagnostic, needs a file path on the command line"]
    fn dump_one_file() {
        let Some(path) = std::env::args().find(|a| a.ends_with(".doc")) else {
            eprintln!("passer le chemin d'un .doc en argument");
            return;
        };
        let Ok(bytes) = std::fs::read(&path) else { return };
        let Some((r, props, chars)) = decoded(&bytes) else { return };
        let (mut ins, mut del, mut ins_ch, mut del_ch) = (0usize, 0usize, 0u32, 0u32);
        for s in r.spans() {
            let n = s.cp_end - s.cp_start;
            match s.kind {
                RevKind::Insertion => (ins, ins_ch) = (ins + 1, ins_ch + n),
                RevKind::Deletion => (del, del_ch) = (del + 1, del_ch + n),
            }
        }
        eprintln!(
            "{path}\n  auteurs {:?} · suivi={}\n  insertions {ins} ({ins_ch} caractères) · \
             suppressions {del} ({del_ch} caractères)",
            r.authors(),
            r.track_changes()
        );
        // The text of each change, to be matched against the `text:change-start`
        // / `text:change-end` anchors LibreOffice writes into the ODF body.
        for s in r.spans().iter().take(12) {
            let t: String = chars
                .get(s.cp_start as usize..(s.cp_end as usize).min(chars.len()))
                .unwrap_or(&[])
                .iter()
                .take(50)
                .collect();
            eprintln!(
                "   #{} {:?} [{}..{}) {:?} {:?} = {t:?}",
                s.id, s.kind, s.cp_start, s.cp_end, s.author, s.date
            );
        }
        // The raw property lists behind them, for when a range looks wrong.
        for run in props.chars.runs().iter().take(8) {
            let hex: String = run.grpprl.iter().map(|b| format!("{b:02x}")).collect();
            eprintln!("   CHPX [{}..{}) {hex}", run.cp_start, run.cp_end);
        }
    }

    #[test]
    fn revisions_are_recovered_from_the_corpus() {
        let files = corpus_files();
        if files.is_empty() {
            eprintln!("KUBUNO_DOC_CORPUS absent — test ignoré");
            return;
        }
        let (mut read, mut with_authors, mut with_spans, mut spans, mut dated, mut tracked) =
            (0, 0, 0, 0, 0, 0);
        let mut named: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for p in &files {
            let Ok(bytes) = std::fs::read(p) else { continue };
            let Some(r) = revisions_of(&bytes) else { continue };
            read += 1;
            if r.authors().len() > 1 {
                with_authors += 1;
            }
            if r.track_changes() {
                tracked += 1;
            }
            if !r.is_empty() {
                with_spans += 1;
                spans += r.len();
                for s in r.spans() {
                    if s.date.is_some() {
                        dated += 1;
                    }
                    if !s.author.is_empty() {
                        named.insert(s.author.clone());
                    }
                }
                eprintln!(
                    "  {} → {} révisions, auteurs {:?}, suivi={}",
                    p.file_name().unwrap_or_default().to_string_lossy(),
                    r.len(),
                    r.authors(),
                    r.track_changes()
                );
            }
        }
        eprintln!(
            "révisions .doc : {read} fichiers lus · {with_authors} avec table d'auteurs · \
             {with_spans} avec révisions · {spans} marques ({dated} datées) · {tracked} suivi actif\n\
             auteurs vus : {named:?}"
        );
        assert!(read > 0, "aucun .doc lu");
        assert!(with_spans > 0, "aucune marque de révision décodée sur le corpus");
        assert!(
            named.contains("Grzegorz Kulesza"),
            "l'auteur de changes-in-footnote.doc n'est pas résolu : {named:?}"
        );
    }
}
