//! Tracked changes (revisions) on import.
//!
//! Word records an edit made while "track changes" was on as MARKUP around the
//! content, never as a separate stream:
//!
//! * `<w:ins w:id w:author w:date>` wraps the runs that were ADDED;
//! * `<w:del …>` wraps the runs that were REMOVED — and inside such a run the
//!   text element is `<w:delText>`, **not** `<w:t>`. Looking for `w:t` there
//!   silently loses every deleted character (`is_run_text` exists for that);
//! * `<w:moveTo>` / `<w:moveFrom>` are the two halves of a move; Word paints
//!   them like an insertion and a deletion, and so do we
//!   (DomainMapper_Impl.cxx:4360, `CreateRedline`: moveTo falls through to
//!   XML_ins, moveFrom to XML_del);
//! * a revision on the PARAGRAPH MARK lives in `<w:pPr><w:rPr><w:ins|w:del>`.
//!   It says the paragraph split (ins) or the paragraph merge (del) is itself
//!   the tracked edit — dropping it turns "accept all" into a different text;
//! * `w:rPrChange`, `w:pPrChange`, `w:sectPrChange`, `w:tblPrChange` &co. hold
//!   the formatting **as it was BEFORE** the change. They must never be read as
//!   current formatting: a `<w:sectPr>` nested in a `<w:sectPrChange>` carries
//!   the OLD page layout, and taking it gives the pre-edit margins/columns
//!   (`is_format_change` / `formatting_children` exist for that).
//!
//! Everything here is total: a missing author, a missing id, a zero or
//! malformed date (Word writes `0-00-00T00:00:00Z`, cf. tdf#146171) yields an
//! empty string, never an error and never a panic.

// The revision surface is consumed piecewise by the run/paragraph/table readers
// and by the writer; keep it whole even when one consumer is not wired yet.
#![allow(dead_code)]

use std::io::Cursor;

use chrono::{NaiveDate, NaiveDateTime, TimeZone, Utc};
use roxmltree::{Document as XmlDoc, Node};
use serde_json::{json, Value};
use zip::ZipArchive;

use crate::converters::docx::xml::{attr_val, child, local};
use crate::converters::docx::zip_io::read_zip_entry;
use crate::converters::types::PmMark;

/// ProseMirror mark names of the shared contract.
pub(crate) const INSERTION_MARK: &str = "insertion";
pub(crate) const DELETION_MARK: &str = "deletion";

/// Paragraph attribute carrying the revision of the paragraph MARK itself
/// (`<w:pPr><w:rPr><w:ins|w:del>`). Value: `{type, author, authorId, date, id}`
/// where `type` is `"insertion"` or `"deletion"`.
pub(crate) const PARA_MARK_ATTR: &str = "paraMark";

/// The two states a tracked change can put content in. Moves collapse onto
/// them: a `w:moveTo` reads as an insertion, a `w:moveFrom` as a deletion.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum RevisionKind {
    Insertion,
    Deletion,
}

impl RevisionKind {
    /// Name of the ProseMirror mark this revision maps onto.
    pub(crate) fn mark_type(self) -> &'static str {
        match self {
            RevisionKind::Insertion => INSERTION_MARK,
            RevisionKind::Deletion => DELETION_MARK,
        }
    }
}

/// One tracked change, as the contract describes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Revision {
    pub kind: RevisionKind,
    /// Display name (`w:author`); may be empty — an unattributed change is legal.
    pub author: String,
    /// Stable id of the author. OOXML has none, so this stays empty; the field
    /// exists because the contract mandates the attribute.
    pub author_id: String,
    /// ISO-8601 UTC (`2026-07-28T09:12:00Z`), or empty when absent/invalid.
    pub date: String,
    /// `w:id` verbatim — used to accept/reject a single change.
    pub id: String,
    /// The change came from `w:moveTo` / `w:moveFrom` (a move, not a plain
    /// insert/delete). Kept for round-tripping; the mark is the same.
    pub moved: bool,
}

impl Revision {
    /// `{author, authorId, date, id}` — the contract's mark attributes.
    pub(crate) fn attrs(&self) -> Value {
        json!({
            "author": self.author,
            "authorId": self.author_id,
            "date": self.date,
            "id": self.id,
        })
    }

    /// The ProseMirror mark to hang on the text nodes this revision covers.
    pub(crate) fn mark(&self) -> PmMark {
        PmMark {
            mark_type: self.kind.mark_type().to_string(),
            attrs: Some(self.attrs()),
        }
    }

    /// Same attributes plus `type` — the shape used for a paragraph-mark
    /// revision, which is a node ATTRIBUTE and not a mark.
    pub(crate) fn typed_attrs(&self) -> Value {
        json!({
            "type": self.kind.mark_type(),
            "author": self.author,
            "authorId": self.author_id,
            "date": self.date,
            "id": self.id,
        })
    }

    pub(crate) fn is_deletion(&self) -> bool {
        self.kind == RevisionKind::Deletion
    }
}

// ─── Recognising the markup ──────────────────────────────────────────────────

/// Which kind of change an element name stands for, `None` when it is not a
/// revision wrapper at all.
pub(crate) fn revision_kind(name: &str) -> Option<RevisionKind> {
    match name {
        "ins" | "moveTo" => Some(RevisionKind::Insertion),
        "del" | "moveFrom" => Some(RevisionKind::Deletion),
        _ => None,
    }
}

/// True for `<w:ins>`, `<w:del>`, `<w:moveTo>`, `<w:moveFrom>` — the elements
/// that WRAP content and must be descended into (their children are runs,
/// hyperlinks, smartTags, bookmarks… exactly like a paragraph's children).
pub(crate) fn is_revision_wrapper(name: &str) -> bool {
    revision_kind(name).is_some()
}

/// Range markers of a tracked change (`w:moveFromRangeStart`, the customXml
/// variants…). They carry no content: skip them silently instead of letting
/// them fall into an "unknown element" branch.
pub(crate) fn is_revision_range_marker(name: &str) -> bool {
    matches!(
        name,
        "moveFromRangeStart"
            | "moveFromRangeEnd"
            | "moveToRangeStart"
            | "moveToRangeEnd"
            | "customXmlInsRangeStart"
            | "customXmlInsRangeEnd"
            | "customXmlDelRangeStart"
            | "customXmlDelRangeEnd"
            | "customXmlMoveFromRangeStart"
            | "customXmlMoveFromRangeEnd"
            | "customXmlMoveToRangeStart"
            | "customXmlMoveToRangeEnd"
    )
}

/// True for the `*Change` records — `w:rPrChange`, `w:pPrChange`,
/// `w:sectPrChange`, `w:tblPrChange`, `w:tblGridChange`, `w:trPrChange`,
/// `w:tcPrChange`, `w:tblPrExChange`, `w:numberingChange`.
///
/// Their CONTENT is the formatting **before** the change. Never merge it into
/// the current properties: LibreOffice guards the same way by pushing a fresh
/// property context around them (DomainMapper.cxx:3255, `LN_CT_RPrChange_rPr`).
pub(crate) fn is_format_change(name: &str) -> bool {
    matches!(
        name,
        "rPrChange"
            | "pPrChange"
            | "sectPrChange"
            | "tblPrChange"
            | "tblGridChange"
            | "trPrChange"
            | "tcPrChange"
            | "tblPrExChange"
            | "numberingChange"
    )
}

/// Element children of a property container (`w:rPr`, `w:pPr`, `w:sectPr`,
/// `w:tblPr`…) with every revision record filtered out. Walk THIS instead of
/// `children()` when reading formatting.
pub(crate) fn formatting_children<'a, 'd>(n: &Node<'a, 'd>) -> Vec<Node<'a, 'd>> {
    n.children()
        .filter(|c| c.is_element() && !is_format_change(local(c)))
        .collect()
}

/// The formatting a `*Change` record captured, i.e. the state BEFORE the edit
/// (`<w:rPrChange><w:rPr>`, `<w:pPrChange><w:pPr>`, `<w:sectPrChange><w:sectPr>`…).
/// Only useful to a "reject change" implementation; never for the current look.
pub(crate) fn format_change_previous<'a, 'd>(change: &Node<'a, 'd>) -> Option<Node<'a, 'd>> {
    let inner = match local(change) {
        "rPrChange" => "rPr",
        "pPrChange" => "pPr",
        "sectPrChange" => "sectPr",
        "tblPrChange" => "tblPr",
        "tblGridChange" => "tblGrid",
        "trPrChange" => "trPr",
        "tcPrChange" => "tcPr",
        "tblPrExChange" => "tblPrEx",
        _ => return None,
    };
    child(change, inner)
}

/// Text-carrying element of a run: `<w:t>`, or `<w:delText>` when the run sits
/// inside a `<w:del>`. THE classic import bug is to test only for `t`.
pub(crate) fn is_run_text(name: &str) -> bool {
    matches!(name, "t" | "delText")
}

/// Field instruction element of a run, deleted variant included.
pub(crate) fn is_instr_text(name: &str) -> bool {
    matches!(name, "instrText" | "delInstrText")
}

// ─── Reading a revision ──────────────────────────────────────────────────────

/// Build a [`Revision`] out of a wrapper element (`w:ins`, `w:del`, `w:moveTo`,
/// `w:moveFrom`). `None` for anything else — in particular for the `*Change`
/// records, which are a formatting change and carry no insertion/deletion mark;
/// use [`revision_attrs_of`] if their author/date is wanted.
pub(crate) fn parse_revision(n: &Node<'_, '_>) -> Option<Revision> {
    let name = local(n);
    let kind = revision_kind(name)?;
    Some(revision_from_attrs(n, kind, matches!(name, "moveTo" | "moveFrom")))
}

/// Author/date/id of any element carrying the `CT_TrackChange` attributes —
/// the `*Change` records and `w:cellIns`/`w:cellDel` have them too.
pub(crate) fn revision_attrs_of(n: &Node<'_, '_>, kind: RevisionKind) -> Revision {
    revision_from_attrs(n, kind, false)
}

fn revision_from_attrs(n: &Node<'_, '_>, kind: RevisionKind, moved: bool) -> Revision {
    // `w16du:dateUtc` (Word 2021) is the unambiguous UTC form of `w:date`,
    // which Word otherwise writes as local time stamped `Z`.
    let raw_date = attr_val(n, "dateUtc").or_else(|| attr_val(n, "date"));
    Revision {
        kind,
        author: attr_val(n, "author").unwrap_or_default(),
        author_id: String::new(),
        date: normalize_date(raw_date.as_deref()),
        id: attr_val(n, "id").unwrap_or_default(),
        moved,
    }
}

/// The mark a wrapper element contributes (empty when it is not a wrapper).
pub(crate) fn revision_marks(n: &Node<'_, '_>) -> Vec<PmMark> {
    parse_revision(n).map(|r| vec![r.mark()]).unwrap_or_default()
}

/// Every revision covering `n`, read from its ANCESTORS, outermost first.
///
/// This is the robust entry point for the run reader: whatever the nesting
/// (`w:ins > w:hyperlink > w:r`, `w:del > w:ins > w:r` for an insertion that was
/// then deleted), a run only has to ask for the revisions above it. The walk
/// stops at the enclosing block and at any property container, so a
/// paragraph-mark revision (`w:pPr > w:rPr > w:ins`) can never leak onto text.
pub(crate) fn ancestor_revisions(n: &Node<'_, '_>) -> Vec<Revision> {
    let mut out = Vec::new();
    let mut cur = n.parent();
    while let Some(node) = cur {
        if !node.is_element() {
            break;
        }
        let name = local(&node);
        // Block / property boundaries: nothing above them applies to this run.
        if matches!(
            name,
            "p" | "tbl"
                | "tr"
                | "tc"
                | "body"
                | "hdr"
                | "ftr"
                | "footnote"
                | "endnote"
                | "txbxContent"
                | "rPr"
                | "pPr"
                | "trPr"
                | "tcPr"
                | "tblPr"
        ) {
            break;
        }
        if let Some(r) = parse_revision(&node) {
            out.push(r);
        }
        cur = node.parent();
    }
    out.reverse();
    out
}

/// The marks a run must carry because of the revisions wrapping it.
pub(crate) fn ancestor_marks(n: &Node<'_, '_>) -> Vec<PmMark> {
    ancestor_revisions(n).iter().map(Revision::mark).collect()
}

/// True when `n` sits inside a deletion — i.e. its text is in `<w:delText>`.
pub(crate) fn is_inside_deletion(n: &Node<'_, '_>) -> bool {
    ancestor_revisions(n).iter().any(Revision::is_deletion)
}

// ─── Paragraph mark, table row, table cell ───────────────────────────────────

/// Revision of the PARAGRAPH MARK: `<w:pPr><w:rPr><w:ins|w:del>`. Accepts
/// either the `<w:p>` or its `<w:pPr>`.
///
/// An inserted mark means the paragraph SPLIT is the change (rejecting it merges
/// the paragraph with the next one); a deleted mark means the paragraph will be
/// merged with the next one when the change is accepted.
pub(crate) fn para_mark_revision(p_or_ppr: &Node<'_, '_>) -> Option<Revision> {
    let ppr = if local(p_or_ppr) == "pPr" {
        *p_or_ppr
    } else {
        child(p_or_ppr, "pPr")?
    };
    let rpr = child(&ppr, "rPr")?;
    rpr.children()
        .filter(|c| c.is_element())
        .find_map(|c| parse_revision(&c))
}

/// Ready-to-insert paragraph attribute for a paragraph-mark revision:
/// `(PARA_MARK_ATTR, {type, author, authorId, date, id})`, `None` when the
/// paragraph mark carries no change.
pub(crate) fn para_mark_attr(p_or_ppr: &Node<'_, '_>) -> Option<(&'static str, Value)> {
    para_mark_revision(p_or_ppr).map(|r| (PARA_MARK_ATTR, r.typed_attrs()))
}

/// Revision of a whole table ROW: `<w:trPr><w:ins|w:del>`. Accepts the `<w:tr>`
/// or its `<w:trPr>`.
pub(crate) fn row_revision(tr_or_trpr: &Node<'_, '_>) -> Option<Revision> {
    let trpr = if local(tr_or_trpr) == "trPr" {
        *tr_or_trpr
    } else {
        child(tr_or_trpr, "trPr")?
    };
    trpr.children()
        .filter(|c| c.is_element())
        .find_map(|c| parse_revision(&c))
}

/// Revision of a table CELL: `<w:tcPr><w:cellIns|w:cellDel>`. Accepts the
/// `<w:tc>` or its `<w:tcPr>`. (`w:cellMerge` — a tracked merge — is ignored.)
pub(crate) fn cell_revision(tc_or_tcpr: &Node<'_, '_>) -> Option<Revision> {
    let tcpr = if local(tc_or_tcpr) == "tcPr" {
        *tc_or_tcpr
    } else {
        child(tc_or_tcpr, "tcPr")?
    };
    tcpr.children().filter(|c| c.is_element()).find_map(|c| {
        let kind = match local(&c) {
            "cellIns" => RevisionKind::Insertion,
            "cellDel" => RevisionKind::Deletion,
            _ => return None,
        };
        Some(revision_attrs_of(&c, kind))
    })
}

// ─── Dates ───────────────────────────────────────────────────────────────────

/// OOXML `w:date` (xsd:dateTime, e.g. `2026-07-28T09:12:00Z`) → ISO-8601 UTC.
///
/// Returns an empty string — never an error — when the attribute is absent, is
/// the zero date Word writes for an unstamped change (`0-00-00T00:00:00Z`,
/// tdf#146171) or cannot be parsed at all. A stamp without a zone is taken as
/// UTC: Word writes local time and marks it `Z` anyway
/// (ConversionHelper.cxx:670, "MSOffice always treats the time as local").
pub(crate) fn normalize_date(raw: Option<&str>) -> String {
    let s = raw.unwrap_or("").trim();
    if s.is_empty() || s.starts_with("0-00-00") || s.starts_with("0000-00-00") {
        return String::new();
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return dt.with_timezone(&Utc).format("%Y-%m-%dT%H:%M:%SZ").to_string();
    }
    // Zone-less forms, with or without a trailing marker.
    let bare = s.trim_end_matches(['Z', 'z']);
    for fmt in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M:%S"] {
        if let Ok(nd) = NaiveDateTime::parse_from_str(bare, fmt) {
            return Utc
                .from_utc_datetime(&nd)
                .format("%Y-%m-%dT%H:%M:%SZ")
                .to_string();
        }
    }
    if let Ok(d) = NaiveDate::parse_from_str(bare, "%Y-%m-%d") {
        if let Some(nd) = d.and_hms_opt(0, 0, 0) {
            return Utc
                .from_utc_datetime(&nd)
                .format("%Y-%m-%dT%H:%M:%SZ")
                .to_string();
        }
    }
    String::new()
}

// ─── Document setting ────────────────────────────────────────────────────────

/// `<w:trackRevisions>` of `word/settings.xml`: is change tracking ON when the
/// document opens? Absent element = off; `w:val="false"|"0"|"off"` = off.
///
/// The element is `w:trackRevisions` — `CT_Settings` has no `trackChanges` at
/// all, and LibreOffice writes `XML_trackRevisions` (DocxExport::WriteSettings).
/// The document setting our envelope carries is still named `trackChanges`.
pub(crate) fn track_revisions_enabled(settings_xml: &str) -> bool {
    let Ok(doc) = XmlDoc::parse(settings_xml) else {
        return false;
    };
    doc.descendants()
        .filter(|n| n.is_element() && local(n) == "trackRevisions")
        .any(|n| !matches!(attr_val(&n, "val").as_deref(), Some("0" | "false" | "off")))
}

/// Same, read straight from the package (missing part = off).
pub(crate) fn read_track_revisions(archive: &mut ZipArchive<Cursor<&[u8]>>) -> bool {
    read_zip_entry(archive, "word/settings.xml")
        .as_deref()
        .is_some_and(track_revisions_enabled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::converters::types::PmNode;

    fn parse(xml: &str) -> roxmltree::Document<'_> {
        roxmltree::Document::parse(xml).expect("XML de test valide")
    }

    fn doc_wrap(body: &str) -> String {
        format!(
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}</w:body></w:document>"#
        )
    }

    /// Every `<w:t>`/`<w:delText>` of a document, tagged with the revision
    /// marks its run inherits — the exact walk the run reader performs.
    fn marked_text(xml: &str) -> Vec<(String, Vec<String>)> {
        let doc = parse(xml);
        let mut out = Vec::new();
        for n in doc.descendants().filter(|n| n.is_element() && is_run_text(local(n))) {
            let marks = ancestor_revisions(&n)
                .iter()
                .map(|r| format!("{}:{}", r.kind.mark_type(), r.author))
                .collect();
            out.push((n.text().unwrap_or("").to_string(), marks));
        }
        out
    }

    #[test]
    fn ins_and_del_wrap_their_runs() {
        let xml = doc_wrap(
            r#"<w:p>
                 <w:r><w:t>garde</w:t></w:r>
                 <w:ins w:id="1" w:author="Jean" w:date="2026-07-28T09:12:00Z">
                   <w:r><w:t>ajout</w:t></w:r>
                 </w:ins>
                 <w:del w:id="2" w:author="Marie" w:date="2026-07-28T10:00:00Z">
                   <w:r><w:delText>retrait</w:delText></w:r>
                 </w:del>
               </w:p>"#,
        );
        let got = marked_text(&xml);
        assert_eq!(got.len(), 3, "un w:delText a été perdu: {got:?}");
        assert_eq!(got[0], ("garde".into(), vec![]));
        assert_eq!(got[1], ("ajout".into(), vec!["insertion:Jean".to_string()]));
        assert_eq!(got[2], ("retrait".into(), vec!["deletion:Marie".to_string()]));
    }

    #[test]
    fn revision_attributes_follow_the_contract() {
        let xml = doc_wrap(
            r#"<w:p><w:ins w:id="7" w:author="Jean Dupont" w:date="2026-07-28T09:12:00Z">
                 <w:r><w:t>x</w:t></w:r></w:ins></w:p>"#,
        );
        let doc = parse(&xml);
        let ins = doc
            .descendants()
            .find(|n| local(n) == "ins")
            .expect("w:ins présent");
        let r = parse_revision(&ins).expect("révision lue");
        assert_eq!(r.kind, RevisionKind::Insertion);
        assert_eq!(r.author, "Jean Dupont");
        assert_eq!(r.id, "7");
        assert_eq!(r.date, "2026-07-28T09:12:00Z");
        let mark = r.mark();
        assert_eq!(mark.mark_type, "insertion");
        assert_eq!(
            mark.attrs,
            Some(json!({"author":"Jean Dupont","authorId":"","date":"2026-07-28T09:12:00Z","id":"7"}))
        );
    }

    /// A move is painted like an insertion/deletion pair (CreateRedline).
    #[test]
    fn moves_map_onto_insertion_and_deletion() {
        let xml = doc_wrap(
            r#"<w:p>
                 <w:moveFrom w:id="1" w:author="A"><w:r><w:delText>parti</w:delText></w:r></w:moveFrom>
                 <w:moveTo w:id="2" w:author="A"><w:r><w:t>arrivé</w:t></w:r></w:moveTo>
               </w:p>"#,
        );
        let got = marked_text(&xml);
        assert_eq!(got[0].1, vec!["deletion:A".to_string()]);
        assert_eq!(got[1].1, vec!["insertion:A".to_string()]);
    }

    /// Asymmetry to keep in mind: the WRITER must always emit `<w:delText>`
    /// inside a `<w:del>`, but the READER has to accept `<w:t>` there too —
    /// 7.5% of the corpus' real `<w:del>` blocks contain one, usually because a
    /// `<w:ins>` is nested inside (text inserted by one reviewer, deleted by
    /// another). Classification comes from the ANCESTORS, never from the
    /// element name, so both spellings are deleted text.
    #[test]
    fn plain_w_t_inside_a_deletion_is_still_deleted() {
        let xml = doc_wrap(
            r#"<w:p><w:del w:id="1" w:author="B"><w:ins w:id="2" w:author="A">
                 <w:r><w:t>inséré puis supprimé</w:t></w:r></w:ins>
                 <w:r><w:t>supprimé tout court</w:t></w:r></w:del></w:p>"#,
        );
        let got = marked_text(&xml);
        assert_eq!(got.len(), 2);
        assert!(got[0].1.contains(&"deletion:B".to_string()));
        assert!(got[0].1.contains(&"insertion:A".to_string()));
        assert_eq!(got[1].1, vec!["deletion:B".to_string()]);
        let doc = parse(&xml);
        for t in doc.descendants().filter(|n| n.is_element() && local(n) == "t") {
            assert!(is_inside_deletion(&t), "un w:t dans un w:del a été pris pour du texte normal");
        }
    }

    /// Insert then delete: both marks, outermost first.
    #[test]
    fn nested_revisions_accumulate() {
        let xml = doc_wrap(
            r#"<w:p><w:del w:id="2" w:author="B"><w:ins w:id="1" w:author="A">
                 <w:r><w:delText>zut</w:delText></w:r></w:ins></w:del></w:p>"#,
        );
        let got = marked_text(&xml);
        assert_eq!(
            got[0].1,
            vec!["deletion:B".to_string(), "insertion:A".to_string()]
        );
    }

    /// A run inside a hyperlink inside an insertion still gets the mark.
    #[test]
    fn revision_reaches_through_hyperlinks() {
        let xml = doc_wrap(
            r#"<w:p><w:ins w:id="1" w:author="A"><w:hyperlink r:id="rId1"
                 xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
                 <w:r><w:t>lien</w:t></w:r></w:hyperlink></w:ins></w:p>"#,
        );
        assert_eq!(marked_text(&xml)[0].1, vec!["insertion:A".to_string()]);
    }

    #[test]
    fn paragraph_mark_revision_is_read_and_never_leaks_onto_text() {
        let xml = doc_wrap(
            r#"<w:p>
                 <w:pPr><w:rPr><w:ins w:id="3" w:author="Léa" w:date="2026-01-02T03:04:05Z"/></w:rPr></w:pPr>
                 <w:r><w:t>texte</w:t></w:r>
               </w:p>"#,
        );
        let doc = parse(&xml);
        let p = doc.descendants().find(|n| local(n) == "p").expect("w:p");
        let (key, val) = para_mark_attr(&p).expect("marque de fin de paragraphe");
        assert_eq!(key, PARA_MARK_ATTR);
        assert_eq!(val["type"], "insertion");
        assert_eq!(val["author"], "Léa");
        assert_eq!(val["date"], "2026-01-02T03:04:05Z");
        // The text of the paragraph is NOT part of the change.
        assert!(
            marked_text(&xml)[0].1.is_empty(),
            "la marque de fin a fui sur le texte"
        );
    }

    #[test]
    fn deleted_paragraph_mark_is_a_deletion() {
        let xml = doc_wrap(
            r#"<w:p><w:pPr><w:rPr><w:del w:id="4" w:author="Léa"/></w:rPr></w:pPr></w:p>"#,
        );
        let doc = parse(&xml);
        let p = doc.descendants().find(|n| local(n) == "p").expect("w:p");
        let r = para_mark_revision(&p).expect("marque de fin");
        assert!(r.is_deletion());
    }

    /// The trap: the `<w:sectPr>` nested in a `<w:sectPrChange>` is the layout
    /// BEFORE the change; `formatting_children` must hide it.
    #[test]
    fn format_change_records_are_kept_out_of_current_formatting() {
        let xml = doc_wrap(
            r#"<w:p><w:pPr><w:jc w:val="center"/>
                 <w:pPrChange w:id="1" w:author="A"><w:pPr><w:jc w:val="right"/></w:pPr></w:pPrChange>
               </w:pPr></w:p>
               <w:sectPr><w:cols w:num="1"/>
                 <w:sectPrChange w:id="2" w:author="A"><w:sectPr><w:cols w:num="2"/></w:sectPr></w:sectPrChange>
               </w:sectPr>"#,
        );
        let doc = parse(&xml);
        let ppr = doc.descendants().find(|n| local(n) == "pPr").expect("w:pPr");
        let kept: Vec<&str> = formatting_children(&ppr).iter().map(local).collect();
        assert_eq!(kept, vec!["jc"], "un pPrChange est passé pour de la mise en forme");
        let sectpr = doc.descendants().find(|n| local(n) == "sectPr").expect("w:sectPr");
        let kept: Vec<&str> = formatting_children(&sectpr).iter().map(local).collect();
        assert_eq!(kept, vec!["cols"]);
        // …and the old layout is still reachable when one wants to REJECT.
        let chg = doc
            .descendants()
            .find(|n| local(n) == "sectPrChange")
            .expect("w:sectPrChange");
        let old = format_change_previous(&chg).expect("sectPr d'avant");
        let cols = child(&old, "cols").expect("cols");
        assert_eq!(attr_val(&cols, "num").as_deref(), Some("2"));
    }

    #[test]
    fn row_and_cell_revisions() {
        let xml = doc_wrap(
            r#"<w:tbl><w:tr><w:trPr><w:ins w:id="1" w:author="A"/></w:trPr>
                 <w:tc><w:tcPr><w:cellDel w:id="2" w:author="B"/></w:tcPr><w:p/></w:tc>
               </w:tr></w:tbl>"#,
        );
        let doc = parse(&xml);
        let tr = doc.descendants().find(|n| local(n) == "tr").expect("w:tr");
        assert_eq!(row_revision(&tr).map(|r| r.kind), Some(RevisionKind::Insertion));
        let tc = doc.descendants().find(|n| local(n) == "tc").expect("w:tc");
        assert_eq!(cell_revision(&tc).map(|r| r.kind), Some(RevisionKind::Deletion));
    }

    #[test]
    fn dates_never_fail_the_import() {
        assert_eq!(normalize_date(Some("2026-07-28T09:12:00Z")), "2026-07-28T09:12:00Z");
        // Offset → UTC.
        assert_eq!(normalize_date(Some("2026-07-28T11:12:00+02:00")), "2026-07-28T09:12:00Z");
        // No zone: taken as UTC.
        assert_eq!(normalize_date(Some("2026-07-28T09:12:00")), "2026-07-28T09:12:00Z");
        assert_eq!(normalize_date(Some("2026-07-28T09:12")), "2026-07-28T09:12:00Z");
        assert_eq!(normalize_date(Some("2026-07-28")), "2026-07-28T00:00:00Z");
        // Word's zero date (tdf#146171) and garbage: empty, no panic.
        assert_eq!(normalize_date(Some("0-00-00T00:00:00Z")), "");
        assert_eq!(normalize_date(Some("n'importe quoi")), "");
        assert_eq!(normalize_date(Some("")), "");
        assert_eq!(normalize_date(None), "");
    }

    /// An unattributed change must not blow up: no author, no date, no id.
    #[test]
    fn unattributed_change_is_legal() {
        let xml = doc_wrap(r#"<w:p><w:ins><w:r><w:t>x</w:t></w:r></w:ins></w:p>"#);
        let doc = parse(&xml);
        let ins = doc.descendants().find(|n| local(n) == "ins").expect("w:ins");
        let r = parse_revision(&ins).expect("révision");
        assert_eq!((r.author.as_str(), r.date.as_str(), r.id.as_str()), ("", "", ""));
    }

    #[test]
    fn track_revisions_setting() {
        let on = r#"<w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:trackRevisions/></w:settings>"#;
        let off = r#"<w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:trackRevisions w:val="false"/></w:settings>"#;
        let none = r#"<w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"/>"#;
        assert!(track_revisions_enabled(on));
        assert!(!track_revisions_enabled(off));
        assert!(!track_revisions_enabled(none));
        assert!(!track_revisions_enabled("pas du XML <<<"));
    }

    /// The corpus decides the element name: 102 files carry `w:trackRevisions`,
    /// not one carries a `w:trackChanges`.
    #[test]
    fn corpus_track_revisions_is_read() {
        let Some(bytes) = corpus_file("sw/qa/extras/ooxmlexport/data/tdf146171.docx") else {
            eprintln!("corpus absent — test ignoré");
            return;
        };
        let mut zip = ZipArchive::new(Cursor::new(&bytes[..])).expect("zip");
        // This file has a settings.xml WITHOUT the flag: tracking is off.
        assert!(!read_track_revisions(&mut zip));
        let Some(bytes) = corpus_file("sw/qa/extras/ooxmlexport/data/tdf142387.docx") else {
            return;
        };
        let mut zip = ZipArchive::new(Cursor::new(&bytes[..])).expect("zip");
        assert!(read_track_revisions(&mut zip), "w:trackRevisions non détecté");
    }

    // ── Corpus (real Word files) ─────────────────────────────────────────────

    /// Text of a `word/document.xml`, split by revision state:
    /// `(kept, inserted, deleted)`.
    fn split_by_revision(document_xml: &str) -> (String, String, String) {
        let Ok(doc) = XmlDoc::parse(document_xml) else {
            return (String::new(), String::new(), String::new());
        };
        let (mut kept, mut ins, mut del) = (String::new(), String::new(), String::new());
        for n in doc.descendants().filter(|n| n.is_element() && is_run_text(local(n))) {
            let text = n.text().unwrap_or("");
            let revs = ancestor_revisions(&n);
            if revs.iter().any(Revision::is_deletion) {
                del.push_str(text);
            } else if revs.is_empty() {
                kept.push_str(text);
            } else {
                ins.push_str(text);
            }
        }
        (kept, ins, del)
    }

    fn corpus_file(rel: &str) -> Option<Vec<u8>> {
        let root = std::env::var("KUBUNO_DOCX_CORPUS").ok()?;
        std::fs::read(std::path::Path::new(&root).join(rel)).ok()
    }

    fn document_xml_of(bytes: &[u8]) -> Option<String> {
        let mut zip = ZipArchive::new(Cursor::new(bytes)).ok()?;
        read_zip_entry(&mut zip, "word/document.xml")
    }

    /// tdf#146171: every change of this file is stamped with the ZERO date, and
    /// the deleted text lives in `<w:delText>`. Reference: LibreOffice's own
    /// conversion of the file (`soffice --convert-to fodt`).
    #[test]
    fn corpus_zero_date_and_deleted_text() {
        let Some(bytes) = corpus_file("sw/qa/extras/ooxmlexport/data/tdf146171.docx") else {
            eprintln!("corpus absent — test ignoré");
            return;
        };
        let xml = document_xml_of(&bytes).expect("word/document.xml");
        let (_, ins, del) = split_by_revision(&xml);
        assert!(del.contains("direct effect"), "texte supprimé perdu: {del:?}");
        assert!(!ins.is_empty(), "texte inséré perdu");
        let doc = XmlDoc::parse(&xml).expect("XML");
        let revs: Vec<Revision> = doc
            .descendants()
            .filter(|n| n.is_element() && is_revision_wrapper(local(n)))
            .filter_map(|n| parse_revision(&n))
            .collect();
        assert_eq!(revs.len(), 7, "7 révisions attendues");
        assert!(revs.iter().all(|r| r.date.is_empty()), "la date zéro doit rester vide");
        assert!(revs.iter().all(|r| r.author == "Author"));
        assert_eq!(revs.iter().filter(|r| r.is_deletion()).count(), 2);
    }

    /// Word 2021 stamps both `w:date` (LOCAL time, marked `Z` — the format is a
    /// lie) and `w16du:dateUtc` (the real UTC). The UTC one must win.
    #[test]
    fn corpus_date_utc_wins_over_local_date() {
        let Some(bytes) = corpus_file("sw/qa/extras/ooxmlexport/data/redline-range-comment.docx")
        else {
            eprintln!("corpus absent — test ignoré");
            return;
        };
        let xml = document_xml_of(&bytes).expect("word/document.xml");
        let doc = XmlDoc::parse(&xml).expect("XML");
        let r = doc
            .descendants()
            .filter(|n| n.is_element() && is_revision_wrapper(local(n)))
            .find_map(|n| parse_revision(&n))
            .expect("une révision");
        // w:date = 09:45:00Z (local), w16du:dateUtc = 08:45:00Z.
        assert_eq!(r.date, "2026-01-22T08:45:00Z");
    }

    /// n830205.docx — the heavy one: 341 wrappers and hundreds of `*Change`
    /// records. Checks that the deleted text is found and that not a single
    /// `*Change` record passes for current formatting.
    #[test]
    fn corpus_heavy_revision_document() {
        let Some(bytes) = corpus_file("sw/qa/extras/ooxmlexport/data/n830205.docx") else {
            eprintln!("corpus absent — test ignoré");
            return;
        };
        let xml = document_xml_of(&bytes).expect("word/document.xml");
        let doc = XmlDoc::parse(&xml).expect("XML");
        let wrappers = doc
            .descendants()
            .filter(|n| n.is_element() && is_revision_wrapper(local(n)))
            .count();
        assert_eq!(wrappers, 341, "w:ins + w:del attendus");
        let (kept, ins, del) = split_by_revision(&xml);
        assert!(!kept.is_empty() && !ins.is_empty() && !del.is_empty());
        // Every `*Change` record of the file is filtered out of its container.
        let mut records = 0usize;
        for c in doc.descendants().filter(|n| {
            n.is_element() && matches!(local(n), "rPr" | "pPr" | "tblPr" | "trPr" | "tcPr")
        }) {
            let raw = c.children().filter(|k| k.is_element()).count();
            let kept = formatting_children(&c).len();
            records += raw - kept;
        }
        assert!(records > 100, "les *Change ne sont pas filtrés ({records})");
    }

    /// End to end, through the real importer: the marks must reach the
    /// ProseMirror document with their author, and the deleted text must still
    /// be there (it only disappears when the change is accepted).
    #[test]
    fn corpus_marks_reach_the_prosemirror_document() {
        let Some(bytes) = corpus_file("sw/qa/extras/ooxmlexport/data/tdf146171.docx") else {
            eprintln!("corpus absent — test ignoré");
            return;
        };
        let (body, _, _, _) = crate::converters::docx::import_docx(&bytes).expect("import");
        let mut ins = Vec::new();
        let mut del = Vec::new();
        fn walk(n: &PmNode, ins: &mut Vec<String>, del: &mut Vec<String>) {
            if let (Some(t), Some(marks)) = (n.text.as_deref(), n.marks.as_deref()) {
                for m in marks {
                    if m.mark_type == INSERTION_MARK {
                        ins.push(t.to_string());
                    } else if m.mark_type == DELETION_MARK {
                        del.push(t.to_string());
                        assert_eq!(
                            m.attrs.as_ref().and_then(|a| a["author"].as_str()),
                            Some("Author"),
                            "auteur perdu sur la marque de suppression"
                        );
                    }
                }
            }
            for c in n.children() {
                walk(c, ins, del);
            }
        }
        walk(&body, &mut ins, &mut del);
        assert!(
            del.iter().any(|t| t.contains("direct effect")),
            "texte supprimé absent du document importé: {del:?}"
        );
        assert!(!ins.is_empty(), "aucune insertion importée");
    }

    /// The whole corpus: read every revision of every file. Nothing may panic,
    /// and a `<w:del>` must never leave its text unattributed.
    #[test]
    fn corpus_revisions_never_panic() {
        let Ok(root) = std::env::var("KUBUNO_DOCX_CORPUS") else {
            eprintln!("KUBUNO_DOCX_CORPUS absent — test ignoré");
            return;
        };
        let (mut files, mut with_rev, mut revs, mut del_chars) = (0usize, 0usize, 0usize, 0usize);
        let mut stack = vec![std::path::PathBuf::from(root)];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().is_none_or(|x| x != "docx") {
                    continue;
                }
                files += 1;
                let Ok(bytes) = std::fs::read(&p) else { continue };
                let Some(xml) = document_xml_of(&bytes) else { continue };
                let Ok(doc) = XmlDoc::parse(&xml) else { continue };
                let n = doc
                    .descendants()
                    .filter(|n| n.is_element() && is_revision_wrapper(local(n)))
                    .filter_map(|n| parse_revision(&n))
                    .count();
                if n == 0 {
                    continue;
                }
                with_rev += 1;
                revs += n;
                let (_, _, del) = split_by_revision(&xml);
                del_chars += del.chars().count();
                // Cross-check, the other way round: walking DOWN from every
                // deletion wrapper must select exactly the text nodes the
                // ancestor walk classified as deleted.
                let mut top_down: Vec<usize> = Vec::new();
                for w in doc.descendants().filter(|n| {
                    n.is_element() && revision_kind(local(n)) == Some(RevisionKind::Deletion)
                }) {
                    for t in w.descendants().filter(|t| t.is_element() && is_run_text(local(t))) {
                        top_down.push(t.id().get_usize());
                    }
                }
                top_down.sort_unstable();
                top_down.dedup();
                let mut bottom_up: Vec<usize> = doc
                    .descendants()
                    .filter(|n| n.is_element() && is_run_text(local(n)) && is_inside_deletion(n))
                    .map(|n| n.id().get_usize())
                    .collect();
                bottom_up.sort_unstable();
                assert_eq!(
                    top_down,
                    bottom_up,
                    "{}: texte supprimé mal attribué",
                    p.display()
                );
            }
        }
        eprintln!(
            "corpus révisions: {with_rev}/{files} fichiers, {revs} révisions, {del_chars} caractères supprimés"
        );
        assert!(files > 0, "aucun .docx trouvé");
        assert!(with_rev >= 100, "corpus à révisions trop maigre: {with_rev}");
    }
}
