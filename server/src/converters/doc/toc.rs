//! Recognising a table of contents in a `.doc`.
//!
//! Same shape as the DOCX case but reached differently. OOXML names the entry
//! style (`TOC1`..`TOC9`); a `.doc` only carries a NUMBER — the built-in style
//! identifier `sti`, which is fixed by the format and therefore the same in a
//! French, German or Japanese Word. `toc 1` is `sti` 19 and `toc 9` is `sti` 27
//! (`ww::sti` in `sw/source/filter/inc/wwstyles.hxx`, names in
//! `sw/source/filter/ww8/styles.cxx`); the page number closing each entry is
//! the cached result of a `PAGEREF` field (`flt` 37, [MS-DOC] 2.9.90).
//!
//! What comes out are the four attributes the editor ALREADY renders — no new
//! model: `tocTitle` on the heading of the block, then `tocLevel`, `tocPage`
//! and `tocLeader` on each entry (`DocumentEditorPage.tsx`, and
//! `canvas-engine.ts` which paints the page number flush right with its dot
//! leader). Because the canvas repaints them, the tab and the digits MUST NOT
//! reach the text: [`TocPlan::is_suppressed`] tells the assembler which
//! character positions to drop, exactly as `field::Annotations::is_hidden`
//! does for field instructions.
//!
//! Wiring, for `assemble.rs`: build the plan once per document, then per
//! paragraph
//!
//! ```text
//! let toc = toc::TocPlan::main(&fib, &table, &doc_text.chars, &paras, &props, &styles);
//! // … in build_body, after `p.to_attrs()`:
//! ctx.toc.apply_attrs(rp.cp_start, &p, &mut attrs);
//! // … in inline_content, next to the existing hidden test:
//! if ctx.notes.is_hidden(at) || ctx.toc.is_suppressed(at) { continue; }
//! ```
//!
//! Reference: LibreOffice `ww8par5.cxx` (`Read_Field`, `Read_F_Tox`) and
//! `wrtw8sty.cxx` (`MSWordStyles::GetWWId`, which maps Writer's "Contents 1"
//! back onto `stiToc1`).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Map, Value};

use super::fib::Fib;
use super::field::{parse_fields, FieldRange};
use super::fkp::DocProps;
use super::pap::Pap;
use super::style::StyleSheet;
use super::text::RawParagraph;

/// `sti` of the built-in style `toc 1` — `ww::stiToc1`.
pub(crate) const STI_TOC_FIRST: u16 = 19;
/// `sti` of `toc 9` — `ww::stiToc9`.
pub(crate) const STI_TOC_LAST: u16 = 27;

/// `sti` of the style that heads a table of contents.
///
/// [MS-DOC] calls slot 46 `toa heading` (table of AUTHORITIES heading) because
/// Word 97 had no separate style for the title of a TOC; OOXML's latent style
/// list gives the same index the name `TOC Heading`, and LibreOffice writes its
/// own "Contents Heading" there (`GetWWId`: `COLL_TOX_CNTNTH` →
/// `ww::stiToaHeading`, tdf#143726). So the answer to "does WW8 have a TOC
/// Heading style?" is: not under that name, but this slot is it.
pub(crate) const STI_TOC_HEADING: u16 = 46;

/// `sti` of `index heading`, which older documents reuse for the same purpose.
pub(crate) const STI_INDEX_HEADING: u16 = 33;

/// `flt` of a `PAGEREF` field — the page number of one entry.
const FLT_PAGEREF: u8 = 37;
/// `flt` of the `TOC` field itself, whose result spans the whole block.
const FLT_TOC: u8 = 13;

/// Entry level of a paragraph from its style identifier, or `None`.
///
/// The nine slots are contiguous, so the level is a subtraction. Verified
/// against `ww::sti`: `stiToc1 = 19`, `stiToc9 = 27` — NOT 20..29, and not
/// `stiIndex1`..`stiIndex9` (10..18), which belong to an alphabetical index and
/// must never be mistaken for a table of contents.
pub(crate) fn toc_level_from_sti(sti: u16) -> Option<u8> {
    match sti {
        STI_TOC_FIRST..=STI_TOC_LAST => Some((sti - STI_TOC_FIRST) as u8 + 1),
        _ => None,
    }
}

/// `true` when this style identifier heads a table of contents.
pub(crate) fn is_toc_heading_sti(sti: u16) -> bool {
    sti == STI_TOC_HEADING || sti == STI_INDEX_HEADING
}

/// Entry level of a paragraph style, following what it is based on.
///
/// Real documents style their entries with a copy of `toc 2` rather than with
/// `toc 2` itself — the copy is a user style whose `istdBase` points at the
/// built-in one, exactly the case `StyleSheet::heading_level_inherited`
/// handles for headings. The nearest ancestor wins.
pub(crate) fn toc_level(styles: &StyleSheet, istd: u16) -> Option<u8> {
    let chain = styles.chain(istd);
    // Leaf first: a style that IS `toc 3` outranks the `toc 1` it descends from.
    if let Some(l) = chain.iter().rev().find_map(|&i| toc_level_from_sti(styles.sti_of(i))) {
        return Some(l);
    }
    chain
        .iter()
        .rev()
        .find_map(|&i| styles.name(i).and_then(toc_level_from_name))
}

/// Last resort: the stored style NAME.
///
/// Only the two spellings that are not a translation are accepted — Word's own
/// `toc 1` / `TOC 1` and the `Contents 1` LibreOffice displays — because a
/// localised name says nothing a `sti` has not already said better.
fn toc_level_from_name(name: &str) -> Option<u8> {
    let t = name.trim();
    let digit = t.chars().next_back()?.to_digit(10)?;
    if !(1..=9).contains(&digit) {
        return None;
    }
    let head = t[..t.len() - 1].trim_end().to_ascii_lowercase();
    (head == "toc" || head == "contents").then_some(digit as u8)
}

/// One recognised entry of a table of contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TocEntry {
    /// 1-9, from the style.
    pub(crate) level: u8,
    /// Page number as Word last computed it. `None` when the TOC carries no
    /// page numbers (`\n`), or when the `PAGEREF` result is not a plain integer
    /// — roman numerals or `2-4` would be a lie once the canvas reprints them
    /// as a number, so in that case the result stays where it is, as text.
    pub(crate) page: Option<u32>,
}

/// Everything recognised about the tables of contents of one subdocument.
///
/// Keyed by the character position a paragraph STARTS at, which is what
/// `text::RawParagraph` carries and therefore what the assembler has in hand.
#[derive(Debug, Clone, Default)]
pub(crate) struct TocPlan {
    entries: BTreeMap<u32, TocEntry>,
    titles: BTreeSet<u32>,
    /// Sorted, non-overlapping `[from, to)` ranges the canvas repaints and the
    /// text must therefore not contain.
    hidden: Vec<(u32, u32)>,
}

impl TocPlan {
    /// The plan for the main document.
    ///
    /// Parses `PlcfFldMom` itself so the caller has nothing to thread through:
    /// `table` is the stream named by [`Fib::table_stream_name`], `chars` the
    /// decoded character stream and `paras` the paragraphs split out of it.
    pub(crate) fn main(
        fib: &Fib,
        table: &[u8],
        chars: &[char],
        paras: &[RawParagraph],
        props: &DocProps,
        styles: &StyleSheet,
    ) -> TocPlan {
        let fields = parse_fields(table, fib.fc_plcf_fld_mom, fib.lcb_plcf_fld_mom);
        TocPlan::build(&fields, chars, paras, props, styles)
    }

    /// Build the plan from already-decoded layers. Headers and footnotes can
    /// reuse it with their own field table.
    pub(crate) fn build(
        fields: &[FieldRange],
        chars: &[char],
        paras: &[RawParagraph],
        props: &DocProps,
        styles: &StyleSheet,
    ) -> TocPlan {
        let mut plan = TocPlan::default();
        // Both lists stay in reading order, so a paragraph only ever looks at
        // the fields it has not walked past yet.
        let refs: Vec<&FieldRange> = fields.iter().filter(|f| f.flt == FLT_PAGEREF).collect();
        let mut next_ref = 0usize;

        // Style of each paragraph, resolved once: the run below needs to look at
        // its neighbours to find the title of a block.
        let istds: Vec<u16> =
            paras.iter().map(|p| props.paras.istd_at(p.cp_start).unwrap_or(0)).collect();

        for (i, rp) in paras.iter().enumerate() {
            let cp_end = paras.get(i + 1).map_or(chars.len() as u32, |n| n.cp_start);
            while refs.get(next_ref).is_some_and(|f| f.cp_end < rp.cp_start) {
                next_ref += 1;
            }
            let Some(level) = istds.get(i).copied().and_then(|istd| toc_level(styles, istd)) else {
                continue;
            };

            // The page number is the LAST `PAGEREF` of the paragraph: an entry
            // may hold another field before it (a `HYPERLINK` wrapping the whole
            // line, a `SEQ` in the caption it copies).
            let mut field: Option<&FieldRange> = None;
            for f in &refs[next_ref.min(refs.len())..] {
                if f.cp_begin >= cp_end {
                    break;
                }
                if f.cp_begin >= rp.cp_start && f.cp_end < cp_end {
                    field = Some(f);
                }
            }
            let mut page = field.and_then(|f| page_number(f, chars));
            match (field, page) {
                // The separator and the digits both belong to the canvas now.
                (Some(f), Some(_)) => plan
                    .hidden
                    .push((separator_start(chars, rp.cp_start, f.cp_begin), f.cp_end + 1)),
                // No usable field: the page may still be there as plain text.
                _ => {
                    if let Some((p, from, to)) = page_from_tail(chars, rp.cp_start, cp_end) {
                        page = Some(p);
                        plan.hidden.push((from, to));
                    }
                }
            }
            plan.entries.insert(rp.cp_start, TocEntry { level, page });
        }

        plan.mark_titles(paras, &istds, styles);
        // `is_suppressed` binary-searches, which only holds on disjoint ranges.
        plan.hidden.sort_unstable();
        let mut merged: Vec<(u32, u32)> = Vec::with_capacity(plan.hidden.len());
        for (from, to) in plan.hidden.drain(..) {
            match merged.last_mut() {
                Some(last) if from <= last.1 => last.1 = last.1.max(to),
                _ => merged.push((from, to)),
            }
        }
        plan.hidden = merged;
        plan
    }

    /// Mark the heading that introduces each run of entries.
    ///
    /// Deliberately narrow: only a paragraph whose style IS the built-in TOC (or
    /// index) heading qualifies. Guessing at the `Heading 1` above a TOC would
    /// be worse than not marking it — the editor REPLACES the block from its
    /// `tocTitle` paragraph onwards when it refreshes the field, so a wrong mark
    /// deletes real content.
    fn mark_titles(&mut self, paras: &[RawParagraph], istds: &[u16], styles: &StyleSheet) {
        for (i, rp) in paras.iter().enumerate() {
            if !self.entries.contains_key(&rp.cp_start) {
                continue;
            }
            // Only the first entry of a run has a title above it.
            if i > 0 && paras.get(i - 1).is_some_and(|p| self.entries.contains_key(&p.cp_start)) {
                continue;
            }
            let Some(prev) = i.checked_sub(1).and_then(|j| paras.get(j).zip(istds.get(j))) else {
                continue;
            };
            let (title, &istd) = prev;
            if is_toc_heading_sti(styles.sti_of(istd)) {
                self.titles.insert(title.cp_start);
            }
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.titles.is_empty()
    }

    /// How many entries were recognised.
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    /// The entry starting at `cp_start`, if that paragraph is one.
    pub(crate) fn entry(&self, cp_start: u32) -> Option<TocEntry> {
        self.entries.get(&cp_start).copied()
    }

    /// Is the paragraph starting at `cp_start` the heading of a TOC block?
    pub(crate) fn is_title(&self, cp_start: u32) -> bool {
        self.titles.contains(&cp_start)
    }

    /// Does this character position belong to the tab + page number the canvas
    /// repaints? Such a position must not reach the text, or the page number
    /// appears twice.
    pub(crate) fn is_suppressed(&self, cp: u32) -> bool {
        // Ranges are sorted and disjoint: the only candidate is the last one
        // that starts at or before `cp`.
        let i = self.hidden.partition_point(|&(from, _)| from <= cp);
        i.checked_sub(1).and_then(|j| self.hidden.get(j)).is_some_and(|&(_, to)| cp < to)
    }

    /// Add the TOC attributes of the paragraph starting at `cp_start` to the
    /// attribute map the assembler is building, and say whether it added any.
    ///
    /// `pap` is the paragraph's resolved properties, the only place the dot
    /// leader can be read from: Word puts the page number on a right-aligned
    /// tab stop whose leader code says how to fill the gap (`tbd >> 3` in
    /// `pap::Pap::apply_tabs`).
    pub(crate) fn apply_attrs(
        &self,
        cp_start: u32,
        pap: &Pap,
        attrs: &mut Map<String, Value>,
    ) -> bool {
        if self.is_title(cp_start) {
            attrs.insert("tocTitle".into(), json!(true));
            return true;
        }
        let Some(e) = self.entry(cp_start) else { return false };
        attrs.insert("tocLevel".into(), json!(e.level));
        if let Some(page) = e.page {
            attrs.insert("tocPage".into(), json!(page));
            // A leader without a page number would have nothing to point at.
            if has_leader(pap) {
                attrs.insert("tocLeader".into(), json!(true));
            }
        }
        true
    }
}

/// Dot leader of the tab stop the page number sits on: the rightmost one.
fn has_leader(pap: &Pap) -> bool {
    // `Pap::tabs` is kept sorted by position, so the last stop is the one the
    // final tab of the entry reaches.
    pap.tabs.last().is_some_and(|t| t.leader.is_some())
}

/// Numeric value of a `PAGEREF` result, or `None` when it is not a plain
/// number.
///
/// The cached result is what Word displayed: usually `12`, but `iv` in a
/// front-matter section and `2-3` when the pages are chapter-numbered. Only a
/// bare integer can become `tocPage`, since the canvas prints it as one.
fn page_number(f: &FieldRange, chars: &[char]) -> Option<u32> {
    let sep = f.cp_sep?;
    let from = sep.checked_add(1)?;
    if from >= f.cp_end {
        return None; // a field with no cached result at all
    }
    let raw: String = chars.get(from as usize..(f.cp_end as usize).min(chars.len()))?.iter().collect();
    // Word pads a recalculated result with spaces, sometimes unbreakable ones.
    let text = raw.trim_matches(|c: char| c.is_whitespace() || c == '\u{a0}');
    if text.is_empty() || !text.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    // Word's own upper bound on a page count; anything above is a misread.
    text.parse::<u32>().ok().filter(|p| *p > 0 && *p <= 65_535)
}

/// The page number of an entry that has no `PAGEREF`, read off the end of the
/// paragraph: `(page, from, to)`, the range being what must disappear.
///
/// Half the tables of contents in the wild are frozen this way — LibreOffice's
/// own WW8 export writes the entries of the `TOC` field as plain
/// `text` TAB `page` lines, with no subfield to read. The shape is only
/// accepted when it is unambiguous: a tab, then digits, then the end of the
/// paragraph, with entry text before the tab. `Chapter 3` (no tab) and a roman
/// `iv` are left alone.
fn page_from_tail(chars: &[char], cp_start: u32, cp_end: u32) -> Option<(u32, u32, u32)> {
    let at = |i: u32| chars.get(i as usize).copied();
    let mut i = cp_end.min(chars.len() as u32);
    // The paragraph or cell mark, the closing delimiters of the fields that
    // wrap the entry — a TOC field ends on its last entry — and any padding:
    // none of it is the number. Every control character goes, since none of
    // them is text; the tab is the one exception, and it stops the scan below.
    while i > cp_start
        && at(i - 1).is_some_and(|c| c != '\t' && (c.is_control() || c == ' ' || c == '\u{a0}'))
    {
        i -= 1;
    }
    let digits_end = i;
    while i > cp_start && at(i - 1).is_some_and(|c| c.is_ascii_digit()) {
        i -= 1;
    }
    let digits_start = i;
    if digits_start == digits_end {
        return None;
    }
    // What introduces the number must be a TAB — the right tab stop of the
    // entry. A space would make `Article 4` a page number.
    let from = separator_start(chars, cp_start, digits_start);
    if !(from..digits_start).any(|k| at(k) == Some('\t')) {
        return None;
    }
    // An entry that is only a tab and a number has no text to keep.
    if from <= cp_start {
        return None;
    }
    let page: String = (digits_start..digits_end).filter_map(at).collect();
    let page = page.parse::<u32>().ok().filter(|p| *p > 0 && *p <= 65_535)?;
    Some((page, from, digits_end))
}

/// Where the run of separators before the page number begins.
///
/// Word writes `Chapter one → 12`: a tab, sometimes padded with spaces, on a
/// right tab stop. All of it is the canvas's job now, so all of it goes — but
/// never past the start of the paragraph.
fn separator_start(chars: &[char], cp_start: u32, cp_field: u32) -> u32 {
    let mut from = cp_field;
    while from > cp_start {
        match chars.get(from as usize - 1) {
            Some('\t' | ' ' | '\u{a0}') => from -= 1,
            _ => break,
        }
    }
    from
}

/// Character ranges covered by a `TOC` field, useful to anyone who needs the
/// extent of the block rather than its entries — the field's result IS the
/// table of contents ([MS-DOC] 2.9.90, `flt` 13).
pub(crate) fn toc_field_ranges(fields: &[FieldRange]) -> Vec<(u32, u32)> {
    fields
        .iter()
        .filter(|f| f.flt == FLT_TOC)
        .map(|f| (f.cp_begin, f.cp_end))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nine_toc_styles_are_sti_19_to_27() {
        // wwstyles.hxx: stiToc1 = 19 … stiToc9 = 27.
        assert_eq!(toc_level_from_sti(19), Some(1));
        assert_eq!(toc_level_from_sti(20), Some(2));
        assert_eq!(toc_level_from_sti(27), Some(9));
        assert_eq!(toc_level_from_sti(28), None);
        // `index 1`..`index 9` are an alphabetical index, not a TOC.
        for sti in 10..=18 {
            assert_eq!(toc_level_from_sti(sti), None, "sti {sti}");
        }
        // Headings are not entries either.
        for sti in 0..=9 {
            assert_eq!(toc_level_from_sti(sti), None, "sti {sti}");
        }
    }

    /// Everything one `.doc` yields, rebuilt from the raw streams: the corpus
    /// tests below need the plan, not a finished document.
    struct Probe {
        chars: Vec<char>,
        paras: Vec<RawParagraph>,
        props: DocProps,
        styles: StyleSheet,
        fields: Vec<FieldRange>,
        plan: TocPlan,
    }

    fn probe(bytes: &[u8]) -> Option<Probe> {
        use std::io::{Cursor, Read};

        use super::super::{fkp, piece, text};

        let mut comp = cfb::CompoundFile::open(Cursor::new(bytes)).ok()?;
        let read = |comp: &mut cfb::CompoundFile<Cursor<&[u8]>>, name: &str| -> Vec<u8> {
            let mut buf = Vec::new();
            if let Ok(mut s) = comp.open_stream(name) {
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
        let enc = piece::charset_for(&fib);
        let chars = piece::read_text(&fib, &doc, &pieces, enc).chars;
        let props = fkp::read_props(&fib, &doc, &table, &data, &pieces);
        let styles = StyleSheet::parse(&fib, &doc, &table);
        let paras = text::split_paragraphs(&chars);
        let fields = parse_fields(&table, fib.fc_plcf_fld_mom, fib.lcb_plcf_fld_mom);
        let plan = TocPlan::main(&fib, &table, &chars, &paras, &props, &styles);
        Some(Probe { chars, paras, props, styles, fields, plan })
    }

    /// The text of one paragraph as the editor would receive it: field
    /// instructions dropped like `text::split_paragraphs` does, then the
    /// positions the plan suppresses.
    fn shown_text(pr: &Probe, cp_start: u32, cp_end: u32) -> String {
        let mut out = String::new();
        let mut in_instruction = 0u32;
        for cp in cp_start..cp_end {
            let Some(&c) = pr.chars.get(cp as usize) else { break };
            match c {
                '\u{13}' => in_instruction += 1,
                '\u{14}' | '\u{15}' => in_instruction = in_instruction.saturating_sub(1),
                _ if in_instruction == 0
                    && (c == '\t' || !c.is_control())
                    && !pr.plan.is_suppressed(cp) =>
                {
                    out.push(c)
                }
                _ => {}
            }
        }
        out
    }

    fn corpus() -> Vec<std::path::PathBuf> {
        let Ok(root) = std::env::var("KUBUNO_DOC_CORPUS") else { return Vec::new() };
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

    #[test]
    #[ignore = "exploration"]
    fn probe_the_corpus() {
        for p in corpus() {
            let Ok(bytes) = std::fs::read(&p) else { continue };
            let Some(pr) = probe(&bytes) else { continue };
            let heads = pr
                .paras
                .iter()
                .filter(|rp| {
                    let istd = pr.props.paras.istd_at(rp.cp_start).unwrap_or(0);
                    is_toc_heading_sti(pr.styles.sti_of(istd))
                })
                .count();
            if pr.plan.is_empty() && heads == 0 {
                continue;
            }
            eprintln!(
                "\n=== {} — {} entrées, {} titres, {} candidats titre, {} champs TOC",
                p.display(),
                pr.plan.len(),
                pr.plan.titles.len(),
                heads,
                toc_field_ranges(&pr.fields).len(),
            );
            for rp in &pr.paras {
                let istd = pr.props.paras.istd_at(rp.cp_start).unwrap_or(0);
                let sti = pr.styles.sti_of(istd);
                let Some(e) = pr.plan.entry(rp.cp_start) else {
                    if is_toc_heading_sti(sti) {
                        eprintln!(
                            "   TITRE? sti={sti} «{}» titre={}",
                            rp.text.trim(),
                            pr.plan.is_title(rp.cp_start)
                        );
                    }
                    continue;
                };
                eprintln!(
                    "   niv={} page={:?} sti={sti} nom={:?} brut={:?}",
                    e.level,
                    e.page,
                    pr.styles.name(istd),
                    rp.text,
                );
            }
        }
    }

    /// The real thing: every `.doc` of the corpus, plan built, invariants
    /// checked. Compared against `soffice --headless --convert-to fodt`, whose
    /// `text:index-body` lists the same entries with the same page numbers.
    #[test]
    fn corpus_tables_of_contents_are_recognised() {
        let files = corpus();
        if files.is_empty() {
            eprintln!("KUBUNO_DOC_CORPUS absent — test ignoré");
            return;
        }
        let (mut docs, mut entries, mut pages, mut titles) = (0usize, 0usize, 0usize, 0usize);
        for p in &files {
            let Ok(bytes) = std::fs::read(p) else { continue };
            let Some(pr) = probe(&bytes) else { continue };
            if pr.plan.is_empty() {
                continue;
            }
            docs += 1;
            entries += pr.plan.len();
            titles += pr.plan.titles.len();
            for (i, rp) in pr.paras.iter().enumerate() {
                let Some(e) = pr.plan.entry(rp.cp_start) else { continue };
                assert!((1..=9).contains(&e.level), "{}: niveau {}", p.display(), e.level);
                let Some(page) = e.page else { continue };
                pages += 1;
                assert!(page > 0, "{}: page nulle", p.display());
                // A page number that reached `tocPage` must have left the text:
                // the canvas repaints it, so keeping it would show it twice.
                let cp_end = pr.paras.get(i + 1).map_or(u32::MAX, |n| n.cp_start);
                assert!(
                    pr.plan.hidden.iter().any(|&(from, to)| from >= rp.cp_start && to <= cp_end),
                    "{}: «{}» garde son numéro de page {page}",
                    p.display(),
                    rp.text,
                );
            }
        }
        eprintln!(
            "TDM .doc : {docs}/{} fichiers, {entries} entrées ({pages} avec numéro), {titles} titres",
            files.len()
        );
        assert!(docs > 0, "aucune table des matières trouvée dans le corpus");
        assert!(pages > 0, "aucun numéro de page extrait d'un PAGEREF");
    }

    /// What `soffice --headless --convert-to fodt` puts in `text:index-body`,
    /// entry by entry: style (`Contents 1` = `toc 1`), text and page number.
    /// `tdf81705_outlineLevel.doc` holds `D` + tab + `1`; the four entries of
    /// `mw00_table_of_contents_templates.doc` are all on page 1, under a fifth
    /// paragraph that Word styled `TOC 1` too — its title, which carries no
    /// page number of its own.
    #[test]
    fn the_reference_documents_match_libreoffice() {
        // One reference document and the (level, page) of each of its entries.
        type Expected<'a> = (&'a str, &'a [(u8, Option<u32>)]);
        let expected: [Expected<'_>; 2] = [
            ("tdf81705_outlineLevel.doc", &[(1, Some(1))]),
            (
                "mw00_table_of_contents_templates.doc",
                &[(1, None), (1, Some(1)), (1, Some(1)), (1, Some(1)), (1, Some(1))],
            ),
        ];
        let files = corpus();
        for (name, want) in expected {
            let Some(p) = files.iter().find(|p| p.ends_with(name)) else {
                eprintln!("{name} absent — comparaison ignorée");
                continue;
            };
            let bytes = std::fs::read(p).expect("lecture du .doc");
            let pr = probe(&bytes).expect("document lisible");
            let mut got: Vec<(u8, Option<u32>)> = Vec::new();
            for (i, rp) in pr.paras.iter().enumerate() {
                let Some(e) = pr.plan.entry(rp.cp_start) else { continue };
                got.push((e.level, e.page));
                // What the editor would show: the tab and the digits are gone,
                // the entry's own text is not.
                let cp_end = pr.paras.get(i + 1).map_or(pr.chars.len() as u32, |n| n.cp_start);
                let shown = shown_text(&pr, rp.cp_start, cp_end);
                eprintln!("   {name}: niv={} page={:?} → «{shown}»", e.level, e.page);
                if let Some(page) = e.page {
                    assert!(
                        !shown.contains('\t') && !shown.ends_with(&page.to_string()),
                        "{name}: «{shown}» garde la tabulation ou le numéro",
                    );
                }
            }
            assert_eq!(got, want.to_vec(), "entrées de {name}");
        }
    }

    #[test]
    fn a_pageref_result_becomes_a_number_only_when_it_is_one() {
        // 0x13 P A G E R E F 0x14 result 0x15
        let build = |result: &str| -> (Vec<char>, FieldRange) {
            let mut chars: Vec<char> = "Chapitre\t\u{13}PAGEREF _Toc1 \\h \u{14}".chars().collect();
            let sep = chars.len() as u32 - 1;
            chars.extend(result.chars());
            let cp_end = chars.len() as u32;
            chars.push('\u{15}');
            (chars, FieldRange { cp_begin: 9, cp_sep: Some(sep), cp_end, flt: FLT_PAGEREF })
        };
        let (chars, f) = build("12");
        assert_eq!(page_number(&f, &chars), Some(12));
        let (chars, f) = build(" 7 ");
        assert_eq!(page_number(&f, &chars), Some(7));
        // Roman numerals and chapter-numbered pages stay text.
        for r in ["iv", "2-3", "", "  ", "Erreur"] {
            let (chars, f) = build(r);
            assert_eq!(page_number(&f, &chars), None, "résultat {r:?}");
        }
        // The tab and everything up to the closing delimiter is suppressed.
        let (chars, f) = build("12");
        assert_eq!(separator_start(&chars, 0, f.cp_begin), 8);
        let plan = TocPlan {
            entries: BTreeMap::new(),
            titles: BTreeSet::new(),
            hidden: vec![(8, f.cp_end + 1)],
        };
        assert!(!plan.is_suppressed(7)); // "Chapitre" survives
        assert!(plan.is_suppressed(8)); // the tab
        assert!(plan.is_suppressed(f.cp_end)); // the closing 0x15
        assert!(!plan.is_suppressed(f.cp_end + 1));
    }

    #[test]
    fn a_frozen_entry_gives_up_its_page_only_on_a_tab() {
        let tail = |s: &str| {
            let chars: Vec<char> = s.chars().collect();
            page_from_tail(&chars, 0, chars.len() as u32)
        };
        // `text` TAB `page` ¶ — what LibreOffice's WW8 export writes.
        assert_eq!(tail("2 SUSTENTAÇÃO TEÓRICA\t12\r").map(|t| t.0), Some(12));
        assert_eq!(tail("3.5 COLETA DE DADOS \t20\r").map(|t| t.0), Some(20));
        // The suppressed range starts at the tab and stops after the digits.
        let chars: Vec<char> = "D\t7\r".chars().collect();
        assert_eq!(page_from_tail(&chars, 0, chars.len() as u32), Some((7, 1, 3)));
        // No tab: a number that belongs to the title itself.
        assert_eq!(tail("Chapitre 3\r"), None);
        // Nothing but a number, or no number at all.
        assert_eq!(tail("\t12\r"), None);
        assert_eq!(tail("Annexe A\t\r"), None);
        assert_eq!(tail("Préface\tiv\r"), None);
    }

    #[test]
    fn the_editor_attributes_are_the_four_it_already_renders() {
        use super::super::pap::TabStop;

        let plan = TocPlan {
            entries: BTreeMap::from([(0, TocEntry { level: 2, page: Some(7) })]),
            titles: BTreeSet::from([100]),
            hidden: Vec::new(),
        };
        let mut pap = Pap::default();
        pap.tabs.push(TabStop { pos: 9026, kind: "right", leader: Some("dot") });

        let mut attrs = Map::new();
        assert!(plan.apply_attrs(0, &pap, &mut attrs));
        assert_eq!(attrs.get("tocLevel"), Some(&json!(2)));
        assert_eq!(attrs.get("tocPage"), Some(&json!(7)));
        assert_eq!(attrs.get("tocLeader"), Some(&json!(true)));
        assert!(!attrs.contains_key("tocTitle"));

        // No leader on the rightmost stop: no dots.
        let mut plain = Pap::default();
        plain.tabs.push(TabStop { pos: 9026, kind: "right", leader: None });
        let mut attrs = Map::new();
        assert!(plan.apply_attrs(0, &plain, &mut attrs));
        assert!(!attrs.contains_key("tocLeader"));

        // The title of the block carries `tocTitle` and nothing else.
        let mut attrs = Map::new();
        assert!(plan.apply_attrs(100, &pap, &mut attrs));
        assert_eq!(attrs.get("tocTitle"), Some(&json!(true)));
        assert_eq!(attrs.len(), 1);

        // Any other paragraph is left alone.
        let mut attrs = Map::new();
        assert!(!plan.apply_attrs(42, &pap, &mut attrs));
        assert!(attrs.is_empty());
    }

    #[test]
    fn only_untranslated_style_names_are_trusted() {
        assert_eq!(toc_level_from_name("toc 3"), Some(3));
        assert_eq!(toc_level_from_name("TOC3"), Some(3));
        assert_eq!(toc_level_from_name("Contents 2"), Some(2));
        assert_eq!(toc_level_from_name("index 2"), None);
        assert_eq!(toc_level_from_name("heading 2"), None);
        assert_eq!(toc_level_from_name("toc"), None);
        assert_eq!(toc_level_from_name("toc 0"), None);
    }
}
