//! Fields, hyperlinks and bookmarks.
//!
//! A field lives in two places at once. In the text it is the triple
//! `0x13 instruction [0x14 result] 0x15`; in the table stream a PLCF
//! (`PlcfFldMom` for the main document, `PlcfFldHdr` for headers…) lists the
//! character position of every one of those three delimiters together with the
//! numeric field type (`flt`) carried by the `0x13` entry.
//!
//! The instruction is a formula — `HYPERLINK "http://…" \l "anchor"` — and the
//! result is what Word had computed the last time it recalculated. We only
//! *recompute* the handful of fields whose value we can honestly produce:
//! hyperlinks (a `link` mark) and, inside a header or a footer, the page and
//! document tokens the editor understands. **Everything else keeps its cached
//! result verbatim** — never the raw instruction, never nothing. That is
//! precisely what LibreOffice does for the types it has no handler for
//! (`sw/source/filter/ww8/ww8par5.cxx`, `Read_Field`: a null entry in
//! `aWW8FieldTab` falls through and the result text is read as plain text).
//!
//! References: [MS-DOC] 2.8.25 (`Plcffld`), 2.9.90 (`flt`), 2.9.20 (`FBKF`);
//! LibreOffice `WW8ReadFieldParams` (`filter/source/msfilter/util.cxx`),
//! `Read_F_Hyperlink` and `WW8PLCFx_Book` (`sw/source/filter/ww8/ww8scan.cxx`).

// The text layer wires this module up; until then nothing here has a caller
// inside the crate.
#![allow(dead_code)]

use std::collections::BTreeMap;

use encoding_rs::Encoding;
use serde_json::json;

use crate::converters::types::PmMark;

use super::fib::{u16_at, u32_at, Fib};

/// Field delimiters, repeated here rather than imported so this module does not
/// depend on the internal shape of the text layer.
const FIELD_BEGIN: char = '\u{13}';
const FIELD_SEP: char = '\u{14}';
const FIELD_END: char = '\u{15}';

// ---------------------------------------------------------------------------
// The field table
// ---------------------------------------------------------------------------

/// One field, located by the character positions of its three delimiters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FieldRange {
    /// Character position of the `0x13` mark.
    pub(crate) cp_begin: u32,
    /// Character position of the `0x14` mark, absent when the field has no
    /// cached result at all.
    pub(crate) cp_sep: Option<u32>,
    /// Character position of the `0x15` mark.
    pub(crate) cp_end: u32,
    /// Field type, `flt` in [MS-DOC] 2.9.90 — 88 is HYPERLINK, 33 is PAGE…
    pub(crate) flt: u8,
}

impl FieldRange {
    /// Position of the last character of the instruction's delimiter run: the
    /// separator when there is one, otherwise the closing mark.
    fn instr_end(&self) -> u32 {
        self.cp_sep.unwrap_or(self.cp_end)
    }

    /// Character range of the cached result, delimiters excluded.
    fn result_range(&self) -> Option<(u32, u32)> {
        let sep = self.cp_sep?;
        (sep + 1 < self.cp_end).then_some((sep + 1, self.cp_end))
    }
}

/// Decode a `Plcffld` — the field table of one subdocument.
///
/// The PLCF holds n+1 character positions followed by n 2-byte `FLD` records;
/// in a record, the low five bits of the first byte are the delimiter
/// (`0x13`/`0x14`/`0x15`) and, for a `0x13`, the second byte is the field type.
/// Fields nest, so the open ones are kept on a stack: a `0x14` belongs to the
/// innermost open field and a `0x15` closes it.
pub(crate) fn parse_fields(table: &[u8], fc: u32, lcb: u32) -> Vec<FieldRange> {
    let mut stack: Vec<FieldRange> = Vec::new();
    let mut out: Vec<FieldRange> = Vec::new();
    for (cp, data) in plcf_special(table, fc, lcb, 2) {
        let Some(&first) = data.first() else { continue };
        match first & 0x1F {
            0x13 => stack.push(FieldRange {
                cp_begin: cp,
                cp_sep: None,
                cp_end: cp,
                flt: data.get(1).copied().unwrap_or(0),
            }),
            0x14 => {
                if let Some(top) = stack.last_mut() {
                    // Only the first separator counts; a second one would come
                    // from a malformed file.
                    if top.cp_sep.is_none() {
                        top.cp_sep = Some(cp);
                    }
                }
            }
            0x15 => {
                if let Some(mut f) = stack.pop() {
                    f.cp_end = cp;
                    if f.cp_end > f.cp_begin {
                        out.push(f);
                    }
                }
            }
            _ => {}
        }
    }
    out.sort_by_key(|f| f.cp_begin);
    out
}

/// English name of a field type, as Word itself writes it since Word 2000.
///
/// Mirrors `ww::GetEnglishFieldName` (`sw/source/filter/ww8/fields.cxx`), which
/// is itself [MS-DOC] 2.9.90. `None` means "no documented name": those fields
/// exist but Word never spells them out.
pub(crate) fn field_name(flt: u8) -> Option<&'static str> {
    const NAMES: [Option<&str>; 98] = [
        None,
        None,
        None,
        Some("REF"),
        Some("XE"),
        Some("REF"),
        Some("SET"),
        Some("IF"),
        Some("INDEX"),
        Some("TC"),
        Some("STYLEREF"),
        Some("RD"),
        Some("SEQ"),
        Some("TOC"),
        Some("INFO"),
        Some("TITLE"),
        Some("SUBJECT"),
        Some("AUTHOR"),
        Some("KEYWORDS"),
        Some("COMMENTS"),
        Some("LASTSAVEDBY"),
        Some("CREATEDATE"),
        Some("SAVEDATE"),
        Some("PRINTDATE"),
        Some("REVNUM"),
        Some("EDITTIME"),
        Some("NUMPAGES"),
        Some("NUMWORDS"),
        Some("NUMCHARS"),
        Some("FILENAME"),
        Some("TEMPLATE"),
        Some("DATE"),
        Some("TIME"),
        Some("PAGE"),
        Some("="),
        Some("QUOTE"),
        None,
        Some("PAGEREF"),
        Some("ASK"),
        Some("FILLIN"),
        None,
        Some("NEXT"),
        Some("NEXTIF"),
        Some("SKIPIF"),
        Some("MERGEREC"),
        None,
        None,
        None,
        Some("PRINT"),
        Some("EQ"),
        Some("GOTOBUTTON"),
        Some("MACROBUTTON"),
        Some("AUTONUMOUT"),
        Some("AUTONUMLGL"),
        Some("AUTONUM"),
        None,
        Some("LINK"),
        Some("SYMBOL"),
        Some("EMBED"),
        Some("MERGEFIELD"),
        Some("USERNAME"),
        Some("USERINITIALS"),
        Some("USERADDRESS"),
        Some("BARCODE"),
        Some("DOCVARIABLE"),
        Some("SECTION"),
        Some("SECTIONPAGES"),
        Some("INCLUDEPICTURE"),
        Some("INCLUDETEXT"),
        Some("FILESIZE"),
        Some("FORMTEXT"),
        Some("FORMCHECKBOX"),
        Some("NOTEREF"),
        Some("TOA"),
        Some("TA"),
        Some("MERGESEQ"),
        None,
        Some("PRIVATE"),
        Some("DATABASE"),
        Some("AUTOTEXT"),
        Some("COMPARE"),
        None,
        None,
        Some("FORMDROPDOWN"),
        Some("ADVANCE"),
        Some("DOCPROPERTY"),
        None,
        Some("CONTROL"),
        Some("HYPERLINK"),
        Some("AUTOTEXTLIST"),
        Some("LISTNUM"),
        None,
        Some("BIDIOUTLINE"),
        Some("ADDRESSBLOCK"),
        Some("GREETINGLINE"),
        Some("SHAPE"),
        Some("BIBLIOGRAPHY"),
        Some("CITATION"),
    ];
    NAMES.get(flt as usize).copied().flatten()
}

/// `flt` of the field types we handle specially.
pub(crate) mod flt {
    pub(crate) const TITLE: u8 = 15;
    pub(crate) const NUMPAGES: u8 = 26;
    pub(crate) const DATE: u8 = 31;
    pub(crate) const PAGE: u8 = 33;
    pub(crate) const SECTIONPAGES: u8 = 66;
    pub(crate) const HYPERLINK: u8 = 88;
}

/// Placeholder the editor substitutes at render time, for the fields that only
/// make sense in a header or a footer.
///
/// The frontend resolves `{page}`, `{pages}`, `{date}` and `{titre}` in header
/// and footer text (`DocumentEditorPage.tsx`); anywhere else those tokens would
/// be shown literally, so outside a header we keep the cached result instead.
fn header_token(flt: u8) -> Option<&'static str> {
    match flt {
        flt::PAGE => Some("{page}"),
        flt::NUMPAGES | flt::SECTIONPAGES => Some("{pages}"),
        flt::DATE => Some("{date}"),
        flt::TITLE => Some("{titre}"),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Field instructions
// ---------------------------------------------------------------------------

/// A parsed field instruction: `HYPERLINK "http://x" \l "top" \o "tip"`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Instruction {
    /// Leading keyword, upper-cased. Empty when the instruction is empty.
    pub(crate) keyword: String,
    /// Positional arguments, in order, keyword excluded.
    pub(crate) args: Vec<String>,
    /// How many of `args` appear before the first `\x` switch. Word treats an
    /// argument that follows a switch as that switch's parameter, never as the
    /// field's own operand (`Read_F_Hyperlink`'s `bOptions`).
    pub(crate) args_before_switch: usize,
    /// Switches with their parameter, empty when the switch takes none.
    pub(crate) switches: Vec<(char, String)>,
}

impl Instruction {
    /// Parameter of the first occurrence of a switch, if present.
    pub(crate) fn switch(&self, c: char) -> Option<&str> {
        self.switches
            .iter()
            .find(|(k, _)| *k == c)
            .map(|(_, v)| v.as_str())
    }

    /// The field's own operand: the first positional argument, and only when it
    /// comes before any switch.
    pub(crate) fn operand(&self) -> Option<&str> {
        (self.args_before_switch > 0).then(|| self.args[0].as_str())
    }
}

/// One lexical unit of an instruction.
enum Token {
    Word(String),
    Switch(char),
}

/// Word accepts straight and typographic quotes indifferently; in a document
/// written in a legacy code page the raw cp1252 bytes also survive as C1
/// control characters (LibreOffice tests 132 and 147 for exactly that reason).
fn is_quote(c: char) -> bool {
    matches!(c, '"' | '\u{201C}' | '\u{201D}' | '\u{84}' | '\u{93}')
}

/// Split an instruction into words and switches.
///
/// Follows `WW8ReadFieldParams::FindNextStringPiece`: a quoted run ends at the
/// closing quote, an unquoted one at a space or at a single backslash, and a
/// doubled backslash is an escaped one that does not end the run.
fn tokenize(src: &str) -> Vec<Token> {
    let cs: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < cs.len() {
        while i < cs.len() && cs[i].is_whitespace() {
            i += 1;
        }
        if i >= cs.len() {
            break;
        }
        // A nested field inside the instruction: its own instruction is not
        // ours to read, so jump to its separator and take the result as text.
        if cs[i] == FIELD_BEGIN {
            while i < cs.len() && cs[i] != FIELD_SEP {
                i += 1;
            }
            if i >= cs.len() {
                break;
            }
        }
        // `\x` introduces a switch; `\\` is an escaped backslash, i.e. text.
        if cs[i] == '\\' && cs.get(i + 1).is_some_and(|&c| c != '\\') {
            out.push(Token::Switch(cs[i + 1]));
            i += 2;
            continue;
        }
        if is_quote(cs[i]) || cs[i] == FIELD_SEP {
            i += 1;
            let mut s = String::new();
            while i < cs.len() && !is_quote(cs[i]) && cs[i] != FIELD_END {
                s.push(cs[i]);
                i += 1;
            }
            if i < cs.len() {
                i += 1; // closing quote
            }
            out.push(Token::Word(s.replace("\\\\", "\\")));
            continue;
        }
        let mut s = String::new();
        while i < cs.len() && !cs[i].is_whitespace() {
            if cs[i] == '\\' {
                if cs.get(i + 1) == Some(&'\\') {
                    s.push('\\');
                    i += 2;
                    continue;
                }
                break; // single backslash: the next switch starts here
            }
            s.push(cs[i]);
            i += 1;
        }
        out.push(Token::Word(s));
    }
    out
}

/// Parse a field instruction into a keyword, positional arguments and switches.
pub(crate) fn parse_instruction(src: &str) -> Instruction {
    let mut instr = Instruction::default();
    let tokens = tokenize(src);
    let mut it = tokens.into_iter().peekable();
    if let Some(Token::Word(w)) = it.peek() {
        instr.keyword = w.to_uppercase();
        it.next();
    }
    let mut seen_switch = false;
    while let Some(tok) = it.next() {
        match tok {
            Token::Word(w) => {
                if !seen_switch {
                    instr.args_before_switch += 1;
                }
                instr.args.push(w);
            }
            Token::Switch(c) => {
                seen_switch = true;
                // A switch takes the following word as its parameter, when the
                // next token is a word rather than another switch.
                let param = match it.peek() {
                    Some(Token::Word(_)) => match it.next() {
                        Some(Token::Word(w)) => w,
                        _ => String::new(),
                    },
                    _ => String::new(),
                };
                instr.switches.push((c, param));
            }
        }
    }
    instr
}

/// Target of a HYPERLINK field, in the form the editor expects.
///
/// The convention comes from the DOCX reader (`converters/docx/read/run.rs`):
/// an internal target is written `#name`, everything else is the URL as is.
/// `\l` gives the anchor inside the target document, so `url` + `#` + anchor —
/// exactly `Read_F_Hyperlink`'s `sURL += "#" + sMark`.
pub(crate) fn hyperlink_target(instr: &Instruction) -> Option<(String, Option<String>)> {
    let mut url = instr.operand().unwrap_or("").replace("\\\\", "\\");
    if let Some(mark) = instr.switch('l') {
        let mark = mark.trim_end_matches('"');
        if !mark.is_empty() {
            url.push('#');
            url.push_str(mark);
        }
    }
    if url.is_empty() {
        return None;
    }
    let tip = instr
        .switch('o')
        .filter(|t| !t.is_empty())
        .map(str::to_owned);
    Some((url, tip))
}

// ---------------------------------------------------------------------------
// Bookmarks
// ---------------------------------------------------------------------------

/// A named range of the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Bookmark {
    pub(crate) name: String,
    pub(crate) cp_start: u32,
    /// One past the last character covered; equal to `cp_start` for a bookmark
    /// that marks a position rather than a range.
    pub(crate) cp_end: u32,
}

/// Word's own bookmarks, which must not show up in the "go to" dialog.
///
/// `_Toc…` is deliberately absent: those are the anchors a table of contents
/// links to, and dropping them would break every entry. So is
/// `__RefHeading__…`, the equivalent LibreOffice writes when it round-trips a
/// heading — note it does not start with `_Ref`, which is a different family.
pub(crate) fn is_technical_bookmark(name: &str) -> bool {
    if name.is_empty() {
        return true;
    }
    if name.starts_with("_Toc") || name.starts_with("__RefHeading__") {
        return false;
    }
    // `_1473670096` and friends: what Word puts around an embedded object.
    if let Some(rest) = name.strip_prefix('_') {
        if !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()) {
            return true;
        }
    }
    const TECHNICAL: [&str; 13] = [
        "_GoBack",         // where the cursor was when the file was saved
        "_Hlk",            // scratch marks left by editing operations
        "_Hlt",            //  idem
        "_Ref",            // targets generated for REF / PAGEREF fields
        "_MailAutoSig",    // signature block
        "_PictureBullets", // list bullet images
        "_DdeLink",
        "_DdeCurrent",
        "_MsoAnchor",
        "_AtnMark",
        "_Direct",
        "__Fieldmark__", // form-field marker written by LibreOffice
        "OLE_LINK",
    ];
    TECHNICAL.iter().any(|p| name.starts_with(p))
}

/// Read the bookmark table: names in `SttbfBkmk`, starts in `PlcfBkf`, ends in
/// `PlcfBkl`.
///
/// The three are joined by index: entry *i* of `PlcfBkf` carries an `FBKF`
/// whose first 16 bits are the index into `PlcfBkl` holding the matching end
/// (`WW8PLCFx_Book::GetBookmark`). The tables are truncated to their shortest,
/// because a damaged file can and does have them disagree.
pub(crate) fn parse_bookmarks(fib: &Fib, table: &[u8], enc: &'static Encoding) -> Vec<Bookmark> {
    parse_bookmarks_at(
        table,
        (fib.fc_sttbf_bkmk, fib.lcb_sttbf_bkmk),
        (fib.fc_plcf_bkf, fib.lcb_plcf_bkf),
        (fib.fc_plcf_bkl, fib.lcb_plcf_bkl),
        fib.nfib >= 193,
        enc,
    )
}

/// Same, with the three table positions given explicitly — the annotation
/// bookmarks (`SttbfAtnbkmk`) live elsewhere in the FIB but read identically.
pub(crate) fn parse_bookmarks_at(
    table: &[u8],
    sttb: (u32, u32),
    bkf: (u32, u32),
    bkl: (u32, u32),
    word97: bool,
    enc: &'static Encoding,
) -> Vec<Bookmark> {
    let names = read_sttb(table, sttb.0, sttb.1, word97, enc);
    if names.is_empty() {
        return Vec::new();
    }
    let starts = plcf_special(table, bkf.0, bkf.1, 4);
    let ends = plcf_cps(table, bkl.0, bkl.1);
    let n = names.len().min(starts.len());
    let mut out = Vec::with_capacity(n);
    for (i, name) in names.into_iter().take(n).enumerate() {
        let (cp_start, data) = starts[i];
        let ibkl = u16_at(data, 0) as usize;
        let cp_end = ends.get(ibkl).copied().unwrap_or(cp_start).max(cp_start);
        out.push(Bookmark { name, cp_start, cp_end });
    }
    out.sort_by_key(|b| (b.cp_start, b.cp_end));
    out
}

// ---------------------------------------------------------------------------
// PLCF and STTB primitives
// ---------------------------------------------------------------------------

/// A PLCF carrying fixed-size records: n+1 character positions then n records.
fn plcf_special(table: &[u8], fc: u32, lcb: u32, cb: usize) -> Vec<(u32, &[u8])> {
    let start = fc as usize;
    let end = start.saturating_add(lcb as usize).min(table.len());
    if start >= end || end - start < 4 + cb {
        return Vec::new();
    }
    let plc = &table[start..end];
    let n = (plc.len() - 4) / (4 + cb);
    let base = 4 * (n + 1);
    (0..n)
        .filter_map(|i| {
            let from = base + i * cb;
            let to = from + cb;
            (to <= plc.len()).then(|| (u32_at(plc, i * 4), &plc[from..to]))
        })
        .collect()
}

/// A PLCF with no record payload: just the n+1 character positions, all of
/// which are meaningful because other tables index into them.
fn plcf_cps(table: &[u8], fc: u32, lcb: u32) -> Vec<u32> {
    let start = fc as usize;
    let end = start.saturating_add(lcb as usize).min(table.len());
    if start >= end || end - start < 4 {
        return Vec::new();
    }
    let plc = &table[start..end];
    (0..plc.len() / 4).map(|i| u32_at(plc, i * 4)).collect()
}

/// Read an `STTB` — Word's string table.
///
/// Word 97 writes a count, then a per-string length prefix; a leading `0xFFFF`
/// means the strings are UTF-16 and the prefix is 16 bits, otherwise they are
/// in the document's code page with an 8-bit prefix. Word 6/95 instead writes
/// the total byte size and a plain run of 8-bit Pascal strings.
/// (`WW8ReadSTTBF`, ww8scan.cxx.)
fn read_sttb(
    table: &[u8],
    fc: u32,
    lcb: u32,
    word97: bool,
    enc: &'static Encoding,
) -> Vec<String> {
    let start = fc as usize;
    let end = start.saturating_add(lcb as usize).min(table.len());
    if start >= end || end - start < 2 {
        return Vec::new();
    }
    let b = &table[start..end];
    let mut out = Vec::new();
    let first = u16_at(b, 0);

    if !word97 {
        // Word 6/95: `first` is the size of the whole structure.
        let limit = (first as usize).min(b.len());
        let mut i = 2usize;
        while i < limit {
            let cch = b[i] as usize;
            i += 1;
            let to = (i + cch).min(b.len());
            if i > to {
                break;
            }
            let (s, _, _) = enc.decode(&b[i..to]);
            out.push(s.into_owned());
            i = to;
        }
        return out;
    }

    let unicode = first == 0xFFFF;
    let mut i = 2usize;
    let count = if unicode {
        let c = u16_at(b, i) as usize;
        i += 2;
        c
    } else {
        first as usize
    };
    let extra = u16_at(b, i) as usize;
    i += 2;
    for _ in 0..count {
        if unicode {
            if i + 2 > b.len() {
                break;
            }
            let cch = u16_at(b, i) as usize;
            i += 2;
            let to = (i + cch * 2).min(b.len());
            if i >= to {
                out.push(String::new());
            } else {
                let units: Vec<u16> = b[i..to]
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                out.push(String::from_utf16_lossy(&units));
            }
            i = to;
        } else {
            if i >= b.len() {
                break;
            }
            let cch = b[i] as usize;
            i += 1;
            let to = (i + cch).min(b.len());
            let (s, _, _) = enc.decode(&b[i..to]);
            out.push(s.into_owned());
            i = to;
        }
        i = i.saturating_add(extra);
    }
    out
}

// ---------------------------------------------------------------------------
// The plan handed to the text layer
// ---------------------------------------------------------------------------

/// A hyperlink over a range of characters.
#[derive(Debug, Clone)]
pub(crate) struct Link {
    pub(crate) cp_start: u32,
    pub(crate) cp_end: u32,
    /// `#anchor` for an internal target, a URL otherwise.
    pub(crate) href: String,
    /// The `\o` tooltip, kept for the day the editor's link mark can show one.
    pub(crate) tooltip: Option<String>,
}

/// Everything the text layer needs to know about a character position: whether
/// it must disappear, what to write in its place, and which marks cover it.
///
/// Built once per subdocument. All positions are absolute character positions;
/// `cp_base` is the position of `chars[0]`, which is 0 for the main document
/// and the start of the header subdocument otherwise.
pub(crate) struct Annotations {
    cp_base: u32,
    hidden: Vec<bool>,
    inserts: BTreeMap<u32, String>,
    links: Vec<Link>,
    bookmarks: Vec<Bookmark>,
}

impl Annotations {
    /// Fields and bookmarks of the main document.
    pub(crate) fn main(
        fib: &Fib,
        table: &[u8],
        chars: &[char],
        enc: &'static Encoding,
    ) -> Annotations {
        let fields = parse_fields(table, fib.fc_plcf_fld_mom, fib.lcb_plcf_fld_mom);
        let bookmarks = parse_bookmarks(fib, table, enc);
        Annotations::build(&fields, bookmarks, chars, 0, false)
    }

    /// Build the plan. `in_header` switches the page/date/title fields from
    /// "keep the cached result" to "emit the editor's token".
    pub(crate) fn build(
        fields: &[FieldRange],
        bookmarks: Vec<Bookmark>,
        chars: &[char],
        cp_base: u32,
        in_header: bool,
    ) -> Annotations {
        let mut a = Annotations {
            cp_base,
            hidden: vec![false; chars.len()],
            inserts: BTreeMap::new(),
            links: Vec::new(),
            bookmarks: Vec::new(),
        };

        // `fields` is sorted by starting position, so a field nested inside
        // another one's *instruction* is always seen after the field that
        // already hid it — and must produce nothing at all. One nested inside a
        // *result* (a hyperlink inside a table of contents) is not hidden and
        // is processed normally.
        for f in fields {
            let buried = a.is_hidden(f.cp_begin);

            // The instruction and its delimiters never reach the document.
            a.hide(f.cp_begin, f.instr_end() + 1);
            a.hide(f.cp_end, f.cp_end + 1);
            if buried {
                continue;
            }

            let instr = a.instruction_text(f, chars);

            if f.flt == flt::HYPERLINK {
                if let Some((href, tooltip)) = hyperlink_target(&parse_instruction(&instr)) {
                    if let Some((from, to)) = f.result_range() {
                        a.links.push(Link { cp_start: from, cp_end: to, href, tooltip });
                    }
                }
                continue;
            }

            if in_header {
                if let Some(token) = header_token(f.flt) {
                    // We can recompute this one: drop the stale value Word had
                    // cached and let the editor render the live one.
                    if let Some((from, to)) = f.result_range() {
                        a.hide(from, to);
                    }
                    a.inserts.insert(f.cp_begin, token.to_owned());
                    continue;
                }
            }
            // Anything else: the cached result stays exactly as Word left it.
        }

        // A bookmark Word created for its own bookkeeping is noise in the "go
        // to" dialog — unless a hyperlink actually points at it, in which case
        // dropping it would break the link.
        a.bookmarks = bookmarks
            .into_iter()
            .filter(|b| {
                !is_technical_bookmark(&b.name)
                    || a.links.iter().any(|l| {
                        l.href
                            .rsplit_once('#')
                            .is_some_and(|(_, anchor)| anchor == b.name)
                    })
            })
            .collect();
        a
    }

    /// Mark `[from, to)` as not to be emitted.
    fn hide(&mut self, from: u32, to: u32) {
        let lo = from.saturating_sub(self.cp_base) as usize;
        let hi = (to.saturating_sub(self.cp_base) as usize).min(self.hidden.len());
        if lo < hi {
            self.hidden[lo..hi].fill(true);
        }
    }

    /// The instruction of a field, nested fields stripped out.
    fn instruction_text(&self, f: &FieldRange, chars: &[char]) -> String {
        let lo = (f.cp_begin.saturating_sub(self.cp_base) as usize).saturating_add(1);
        let hi = (f.instr_end().saturating_sub(self.cp_base) as usize).min(chars.len());
        if lo >= hi {
            return String::new();
        }
        let mut out = String::new();
        let mut depth = 0u32;
        for &c in &chars[lo..hi] {
            match c {
                FIELD_BEGIN => depth += 1,
                FIELD_END => depth = depth.saturating_sub(1),
                FIELD_SEP => {}
                _ if depth == 0 => out.push(c),
                _ => {}
            }
        }
        out
    }

    /// Does this character belong to a field instruction, or to a result we
    /// replace with a token?
    pub(crate) fn is_hidden(&self, cp: u32) -> bool {
        let i = cp.saturating_sub(self.cp_base) as usize;
        self.hidden.get(i).copied().unwrap_or(false)
    }

    /// Text to emit *before* the character at `cp` — the `{page}`-style tokens.
    pub(crate) fn insert_at(&self, cp: u32) -> Option<&str> {
        self.inserts.get(&cp).map(String::as_str)
    }

    /// Hyperlink target covering `cp`, if any.
    pub(crate) fn href_at(&self, cp: u32) -> Option<&str> {
        self.links
            .iter()
            .find(|l| cp >= l.cp_start && cp < l.cp_end)
            .map(|l| l.href.as_str())
    }

    /// Names of the bookmarks covering `cp`. A zero-length bookmark covers the
    /// single position it sits at, so that it survives as a mark on the text.
    pub(crate) fn bookmarks_at(&self, cp: u32) -> Vec<&str> {
        self.bookmarks
            .iter()
            .filter(|b| cp >= b.cp_start && (cp < b.cp_end || cp == b.cp_start))
            .map(|b| b.name.as_str())
            .collect()
    }

    /// The marks that apply at `cp`: `link` with its `href`, then one
    /// `bookmark` per name — the same shapes the DOCX reader produces.
    pub(crate) fn marks_at(&self, cp: u32) -> Vec<PmMark> {
        let mut out = Vec::new();
        if let Some(href) = self.href_at(cp) {
            out.push(PmMark {
                mark_type: "link".into(),
                attrs: Some(json!({ "href": href })),
            });
        }
        for name in self.bookmarks_at(cp) {
            out.push(PmMark {
                mark_type: "bookmark".into(),
                attrs: Some(json!({ "name": name })),
            });
        }
        out
    }

    pub(crate) fn links(&self) -> &[Link] {
        &self.links
    }

    pub(crate) fn bookmarks(&self) -> &[Bookmark] {
        &self.bookmarks
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.inserts.is_empty()
            && self.links.is_empty()
            && self.bookmarks.is_empty()
            && !self.hidden.iter().any(|&h| h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instr(s: &str) -> Instruction {
        parse_instruction(s)
    }

    #[test]
    fn hyperlink_instruction_splits_into_url_and_switches() {
        let i = instr(" HYPERLINK \"http://example.com/a b\" \\o \"tip\" \\t \"_blank\" ");
        assert_eq!(i.keyword, "HYPERLINK");
        assert_eq!(i.operand(), Some("http://example.com/a b"));
        assert_eq!(i.switch('o'), Some("tip"));
        assert_eq!(i.switch('t'), Some("_blank"));
    }

    #[test]
    fn internal_anchor_uses_the_docx_convention() {
        let i = instr(" HYPERLINK \\l \"_Toc12345\" ");
        let (href, _) = hyperlink_target(&i).expect("cible");
        assert_eq!(href, "#_Toc12345");
    }

    #[test]
    fn url_and_anchor_are_joined_by_a_hash() {
        let i = instr(" HYPERLINK \"http://x/y.doc\" \\l bookmark ");
        assert_eq!(hyperlink_target(&i).unwrap().0, "http://x/y.doc#bookmark");
    }

    #[test]
    fn a_positional_argument_after_a_switch_is_not_the_url() {
        // Word only takes the URL when it precedes every switch.
        let i = instr(" HYPERLINK \\l \"top\" \"not-the-url\" ");
        assert_eq!(i.operand(), None);
        assert_eq!(hyperlink_target(&i).unwrap().0, "#top");
    }

    #[test]
    fn doubled_backslashes_are_unescaped_in_unc_paths() {
        let i = instr(" HYPERLINK \\\\\\\\server\\\\share\\\\f.doc ");
        assert_eq!(i.operand(), Some(r"\\server\share\f.doc"));
    }

    #[test]
    fn field_names_match_the_ms_doc_table() {
        assert_eq!(field_name(flt::HYPERLINK), Some("HYPERLINK"));
        assert_eq!(field_name(flt::PAGE), Some("PAGE"));
        assert_eq!(field_name(flt::NUMPAGES), Some("NUMPAGES"));
        assert_eq!(field_name(flt::DATE), Some("DATE"));
        assert_eq!(field_name(flt::TITLE), Some("TITLE"));
        assert_eq!(field_name(200), None);
    }

    #[test]
    fn word_scratch_bookmarks_are_filtered_but_toc_targets_are_kept() {
        assert!(is_technical_bookmark("_GoBack"));
        assert!(is_technical_bookmark("_Hlk123456"));
        assert!(is_technical_bookmark("_Ref456789"));
        assert!(is_technical_bookmark("_PictureBullets"));
        assert!(is_technical_bookmark("_1473670096")); // embedded object
        assert!(is_technical_bookmark("__Fieldmark__3_4145073789"));
        assert!(!is_technical_bookmark("_Toc123456"));
        // LibreOffice's heading anchors are link targets, and `_Ref` is not
        // a prefix of `__RefHeading__`.
        assert!(!is_technical_bookmark("__RefHeading__1499_777803520"));
        assert!(!is_technical_bookmark("Chapitre1"));
    }

    /// Build a `chars`/`fields` pair from a readable sketch of the text.
    fn sketch(s: &str) -> Vec<char> {
        s.replace('«', "\u{13}")
            .replace('|', "\u{14}")
            .replace('»', "\u{15}")
            .chars()
            .collect()
    }

    fn visible(a: &Annotations, chars: &[char]) -> String {
        let mut out = String::new();
        for (i, &c) in chars.iter().enumerate() {
            if let Some(t) = a.insert_at(i as u32) {
                out.push_str(t);
            }
            if !a.is_hidden(i as u32) {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn an_unknown_field_keeps_its_cached_result() {
        let chars = sketch("a« SAVEDATE |12/03/1999»b");
        let f = [FieldRange { cp_begin: 1, cp_sep: Some(12), cp_end: 23, flt: 22 }];
        let a = Annotations::build(&f, Vec::new(), &chars, 0, false);
        assert_eq!(visible(&a, &chars), "a12/03/1999b");
    }

    #[test]
    fn a_field_without_a_result_disappears_entirely() {
        let chars = sketch("a« XE \"x\" »b");
        let f = [FieldRange { cp_begin: 1, cp_sep: None, cp_end: 10, flt: 4 }];
        let a = Annotations::build(&f, Vec::new(), &chars, 0, false);
        assert_eq!(visible(&a, &chars), "ab");
    }

    #[test]
    fn page_becomes_a_token_in_a_header_and_a_number_elsewhere() {
        let chars = sketch("p.« PAGE |7»");
        let f = [FieldRange { cp_begin: 2, cp_sep: Some(9), cp_end: 11, flt: flt::PAGE }];
        let body = Annotations::build(&f, Vec::new(), &chars, 0, false);
        assert_eq!(visible(&body, &chars), "p.7");
        let header = Annotations::build(&f, Vec::new(), &chars, 0, true);
        assert_eq!(visible(&header, &chars), "p.{page}");
    }

    #[test]
    fn a_hyperlink_marks_its_result_and_hides_its_instruction() {
        let chars = sketch("« HYPERLINK \"http://a.b\" |cliquez»");
        let sep = chars.iter().position(|&c| c == '\u{14}').unwrap() as u32;
        let end = chars.len() as u32 - 1;
        let f = [FieldRange { cp_begin: 0, cp_sep: Some(sep), cp_end: end, flt: flt::HYPERLINK }];
        let a = Annotations::build(&f, Vec::new(), &chars, 0, false);
        assert_eq!(visible(&a, &chars), "cliquez");
        assert_eq!(a.href_at(sep + 1), Some("http://a.b"));
        assert_eq!(a.href_at(0), None);
        let marks = a.marks_at(sep + 1);
        assert_eq!(marks.len(), 1);
        assert_eq!(marks[0].mark_type, "link");
    }

    #[test]
    fn a_field_nested_in_an_instruction_produces_nothing() {
        // ` TOC \o « PAGE |9» ` — the inner PAGE lives in the outer field's
        // instruction, so not even its token may surface.
        let chars = sketch("« TOC « PAGE |9»|1.Titre»");
        let inner_b = 7u32;
        let inner_sep = chars.iter().position(|&c| c == '\u{14}').unwrap() as u32;
        let inner_end = chars.iter().position(|&c| c == '\u{15}').unwrap() as u32;
        let outer_sep = inner_end + 1;
        let outer_end = chars.len() as u32 - 1;
        let f = [
            FieldRange { cp_begin: 0, cp_sep: Some(outer_sep), cp_end: outer_end, flt: 13 },
            FieldRange {
                cp_begin: inner_b,
                cp_sep: Some(inner_sep),
                cp_end: inner_end,
                flt: flt::PAGE,
            },
        ];
        let a = Annotations::build(&f, Vec::new(), &chars, 0, true);
        assert_eq!(visible(&a, &chars), "1.Titre");
    }

    #[test]
    fn a_bookmark_targeted_by_a_link_survives_the_technical_filter() {
        let chars = sketch("« HYPERLINK \\l \"_Ref1\" |voir»x");
        let sep = chars.iter().position(|&c| c == '\u{14}').unwrap() as u32;
        let end = chars.iter().position(|&c| c == '\u{15}').unwrap() as u32;
        let f = [FieldRange { cp_begin: 0, cp_sep: Some(sep), cp_end: end, flt: flt::HYPERLINK }];
        let bks = vec![
            Bookmark { name: "_Ref1".into(), cp_start: end + 1, cp_end: end + 2 },
            Bookmark { name: "_GoBack".into(), cp_start: end + 1, cp_end: end + 2 },
        ];
        let a = Annotations::build(&f, bks, &chars, 0, false);
        assert_eq!(a.bookmarks().len(), 1);
        assert_eq!(a.bookmarks()[0].name, "_Ref1");
    }

    /// Read every `.doc` of the corpus and report what the field layer finds.
    /// Run with `KUBUNO_DOC_CORPUS=… cargo test … -- --nocapture` to eyeball it
    /// next to `soffice --convert-to fodt`.
    #[test]
    fn corpus_fields_and_bookmarks() {
        use std::io::{Cursor, Read};

        let Ok(root) = std::env::var("KUBUNO_DOC_CORPUS") else {
            eprintln!("KUBUNO_DOC_CORPUS absent — test ignoré");
            return;
        };
        let only = std::env::var("KUBUNO_DOC_ONLY").unwrap_or_default();

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

        let (mut docs, mut with_fields, mut links, mut bookmarks) = (0, 0, 0, 0);
        for path in files {
            if !only.is_empty() && !path.to_string_lossy().contains(&only) {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else { continue };
            let Ok(mut comp) = cfb::CompoundFile::open(Cursor::new(&bytes[..])) else { continue };
            let mut doc_stream = Vec::new();
            if comp
                .open_stream("WordDocument")
                .map(|mut s| s.read_to_end(&mut doc_stream))
                .is_err()
            {
                continue;
            }
            let Ok(fib) = Fib::parse(&doc_stream) else { continue };
            if fib.encrypted {
                continue;
            }
            let mut table = Vec::new();
            if let Ok(mut s) = comp.open_stream(fib.table_stream_name()) {
                let _ = s.read_to_end(&mut table);
            }
            let pieces = super::super::piece::parse_pieces(&fib, &table);
            let enc = super::super::piece::charset_for(&fib);
            let text = super::super::piece::read_text(&fib, &doc_stream, &pieces, enc);

            let fields = parse_fields(&table, fib.fc_plcf_fld_mom, fib.lcb_plcf_fld_mom);
            let a = Annotations::main(&fib, &table, &text.chars, enc);
            docs += 1;
            if !fields.is_empty() {
                with_fields += 1;
            }
            links += a.links().len();
            bookmarks += a.bookmarks().len();

            if !only.is_empty() {
                eprintln!("=== {}", path.display());
                for f in &fields {
                    let name = field_name(f.flt).unwrap_or("?");
                    let instr = a.instruction_text(f, &text.chars);
                    let res = f
                        .result_range()
                        .map(|(x, y)| {
                            text.chars[x as usize..(y as usize).min(text.chars.len())]
                                .iter()
                                .collect::<String>()
                        })
                        .unwrap_or_default();
                    eprintln!("  [{}] {name}: {instr:?} => {res:?}", f.flt);
                }
                for l in a.links() {
                    eprintln!("  lien {:?} [{}..{}]", l.href, l.cp_start, l.cp_end);
                }
                for b in a.bookmarks() {
                    eprintln!("  signet {:?} [{}..{}]", b.name, b.cp_start, b.cp_end);
                }
                eprintln!("  texte: {:?}", visible(&a, &text.chars));
            }
        }
        eprintln!(
            "corpus champs: {docs} documents, {with_fields} avec champs, {links} liens, {bookmarks} signets"
        );
    }
}
