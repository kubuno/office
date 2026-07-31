//! `<w:ins>` / `<w:del>` — tracked changes (revisions) on export.
//!
//! The editor holds a revision as a MARK on inline nodes (`insertion` /
//! `deletion`, see the shared contract), whereas OOXML holds it as an ELEMENT
//! WRAPPING the runs. The translation therefore mirrors what `run.rs` already
//! does for `<w:hyperlink>`: consecutive nodes carrying the same change are
//! gathered into a single wrapper, instead of one wrapper per run — Word and
//! Writer show one change per element, so a per-run wrapper turns a single edit
//! into as many changes as it has formatting boundaries
//! (cf. LibreOffice `DocxAttributeOutput::StartRedline`,
//! docxattributeoutput.cxx:4515).
//!
//! Intended use from `run.rs`, inside the inline loop:
//!
//! ```text
//! let revs = revisions::node_revisions(&nodes[i]);
//! let j = i + revisions::group_len(nodes, i);
//! let mut inner = String::new();
//! for node in &nodes[i..j] {
//!     inner.push_str(&render_inline_node(node, ctx));
//! }
//! out.push_str(&revisions::wrap(&revs, &inner, ctx));
//! i = j;
//! ```
//!
//! `wrap` with an empty slice returns `inner` untouched, so the loop needs no
//! special case.
//!
//! THE trap: inside a `<w:del>`, character data must be carried by
//! `<w:delText>`, never by `<w:t>` (docxattributeoutput.cxx:4117 switches the
//! token for exactly this reason). A `w:t` there makes the reader treat the
//! deleted text as ordinary text — the deletion is displayed, and once accepted
//! the text stays in the document. `wrap` performs the rewrite itself on the
//! whole serialised group, so a caller cannot get it wrong; `render_del_run`
//! and `del_text_to_run_children` are there for callers that build a deleted
//! run directly.

use serde_json::Value;

use crate::converters::docx::xml::{escape_xml_attr, text_to_run_children};
use crate::converters::types::{PmMark, PmNode};

use super::ctx::ExportCtx;

/// The two tracked changes the editor can produce.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RevKind {
    Insertion,
    Deletion,
}

impl RevKind {
    /// Qualified element name of the wrapper.
    fn element(self) -> &'static str {
        match self {
            RevKind::Insertion => "w:ins",
            RevKind::Deletion => "w:del",
        }
    }
}

/// One tracked change, as carried by an `insertion` / `deletion` mark.
#[derive(Clone, Debug)]
pub(crate) struct Revision {
    pub(crate) kind: RevKind,
    /// The EDITOR's change id — used to group runs, never written out: the
    /// `w:id` attribute is a document-wide annotation id allocated at
    /// serialisation time.
    pub(crate) id: String,
    /// Display name; may be empty (an unattributed change is legal).
    pub(crate) author: String,
    /// Already normalised to the `xsd:dateTime` form Word accepts, or `None`
    /// when the mark carried no usable date — the attribute is then omitted,
    /// as LibreOffice does (docxattributeoutput.cxx:4545).
    pub(crate) date: Option<String>,
}

impl Revision {
    pub(crate) fn is_deletion(&self) -> bool {
        self.kind == RevKind::Deletion
    }

    /// Grouping identity. An unattributed change has no id to group on, so it
    /// falls back to author + date: without it every run of one edit would
    /// become a separate change.
    fn group_id(&self) -> String {
        if self.id.is_empty() {
            format!("{}|{}", self.author, self.date.as_deref().unwrap_or(""))
        } else {
            self.id.clone()
        }
    }
}

/// The revisions carried by a set of marks, INSERTION FIRST.
///
/// Both marks can sit on the same text: something inserted by one author and
/// then deleted by another. Word writes that as `<w:ins><w:del>…`, which is
/// what the ordering here produces.
pub(crate) fn revisions_of(marks: &[PmMark]) -> Vec<Revision> {
    let mut out = Vec::new();
    for (mark_type, kind) in [
        ("insertion", RevKind::Insertion),
        ("deletion", RevKind::Deletion),
    ] {
        if let Some(m) = marks.iter().find(|m| m.mark_type == mark_type) {
            out.push(Revision {
                kind,
                id: mark_attr(m, &["id", "changeId", "data-ins-id", "data-del-id"]).unwrap_or_default(),
                author: mark_attr(m, &["author", "data-ins-author", "data-del-author"])
                    .unwrap_or_default(),
                date: mark_attr(m, &["date", "data-ins-date", "data-del-date"])
                    .as_deref()
                    .and_then(docx_date),
            });
        }
    }
    out
}

/// The revisions carried by an inline node.
pub(crate) fn node_revisions(node: &PmNode) -> Vec<Revision> {
    revisions_of(node.marks.as_deref().unwrap_or(&[]))
}

/// How many consecutive nodes, from `start`, belong to the SAME change(s).
///
/// Returns 0 past the end, and at least 1 otherwise — including for nodes
/// carrying no revision at all, so the caller can drive one uniform loop.
pub(crate) fn group_len(nodes: &[PmNode], start: usize) -> usize {
    if start >= nodes.len() {
        return 0;
    }
    let key = group_key(&node_revisions(&nodes[start]));
    let mut n = 1;
    while start + n < nodes.len() && group_key(&node_revisions(&nodes[start + n])) == key {
        n += 1;
    }
    n
}

fn group_key(revs: &[Revision]) -> Vec<(RevKind, String)> {
    revs.iter().map(|r| (r.kind, r.group_id())).collect()
}

/// Wrap already-serialised runs in their `<w:ins>` / `<w:del>` elements.
///
/// The slice is nested outermost-first, so `[insertion, deletion]` gives
/// `<w:ins><w:del>…</w:del></w:ins>`. When any of them is a deletion the whole
/// fragment goes through `to_deleted_text`: this is the single place where
/// `w:t` becomes `w:delText`.
///
/// An empty slice returns `inner` unchanged; empty `inner` returns nothing —
/// an empty wrapper is a change with no content, which readers report as a
/// damaged file.
pub(crate) fn wrap(revs: &[Revision], inner: &str, ctx: &mut ExportCtx) -> String {
    if revs.is_empty() {
        return inner.to_string();
    }
    if inner.is_empty() {
        return String::new();
    }
    let mut out = if revs.iter().any(Revision::is_deletion) {
        to_deleted_text(inner)
    } else {
        inner.to_string()
    };
    for rev in revs.iter().rev() {
        let el = rev.kind.element();
        out = format!("<{el}{}>{out}</{el}>", change_attrs(rev, ctx));
    }
    out
}

/// `w:id` / `w:author` / `w:date` of a `CT_TrackChange`. The id must be unique
/// document-wide, so it comes from the context's annotation counter — the same
/// one bookmarks and comments draw from.
fn change_attrs(rev: &Revision, ctx: &mut ExportCtx) -> String {
    let id = ctx.next_mark_id();
    let author = escape_xml_attr(&rev.author);
    match &rev.date {
        Some(d) => format!(
            r#" w:id="{id}" w:author="{author}" w:date="{}""#,
            escape_xml_attr(d)
        ),
        None => format!(r#" w:id="{id}" w:author="{author}""#),
    }
}

/// Run children for DELETED text: same splitting as `text_to_run_children`
/// (`w:br` / `w:tab` cannot live inside character data), but the text elements
/// are `w:delText`.
pub(crate) fn del_text_to_run_children(text: &str) -> String {
    to_deleted_text(&text_to_run_children(text))
}

/// A complete deleted run, `rpr` being an already-serialised `<w:rPr>`.
pub(crate) fn render_del_run(rpr: &str, text: &str) -> String {
    format!("<w:r>{rpr}{}</w:r>", del_text_to_run_children(text))
}

/// Rewrite an already-serialised fragment for a deletion context: `w:t` →
/// `w:delText` and `w:instrText` → `w:delInstrText`.
///
/// Done by scanning rather than re-parsing so that any run the rest of the
/// writer can produce — including nested `w:hyperlink` content — is covered.
/// The delimiter check is what keeps `<w:tab/>` and `<w:tbl>` from matching the
/// `<w:t` prefix.
pub(crate) fn to_deleted_text(xml: &str) -> String {
    const RENAMES: [(&str, &str); 2] = [
        ("w:t", "w:delText"),
        ("w:instrText", "w:delInstrText"),
    ];
    let mut out = String::with_capacity(xml.len() + 32);
    let mut i = 0;
    while i < xml.len() {
        let Some(offset) = xml[i..].find('<') else {
            out.push_str(&xml[i..]);
            break;
        };
        let lt = i + offset;
        out.push_str(&xml[i..lt]);
        let closing = xml[lt + 1..].starts_with('/');
        let name_at = if closing { lt + 2 } else { lt + 1 };
        let rest = &xml[name_at..];
        let renamed = RENAMES.iter().find(|(from, _)| {
            rest.strip_prefix(*from)
                .and_then(|tail| tail.chars().next())
                .is_some_and(|c| matches!(c, '>' | ' ' | '/'))
        });
        match renamed {
            Some((from, to)) => {
                out.push('<');
                if closing {
                    out.push('/');
                }
                out.push_str(to);
                i = name_at + from.len();
            }
            None => {
                out.push('<');
                i = lt + 1;
            }
        }
    }
    out
}

/// Revision of the PARAGRAPH MARK, read from the paragraph node's attributes.
///
/// An inserted paragraph mark is a paragraph SPLIT made while tracking, a
/// deleted one is a MERGE with the paragraph that follows. Accepted shapes:
/// `{"insertion": {"author": …, "date": …, "id": …}}` / `"deletion"`, and the
/// flat `insAuthor` / `insDate` / `insId` / `delAuthor` / `delDate` / `delId`.
pub(crate) fn paragraph_mark_revisions(attrs: Option<&Value>) -> Vec<Revision> {
    let Some(a) = attrs else {
        return Vec::new();
    };
    let mut out = Vec::new();
    // The importer stores the paragraph mark as a single typed object
    // (`paraMark: {type, author, …}`); the editor stores it per kind. Both
    // shapes must round-trip, so a `paraMark` whose `type` matches feeds the
    // same fields as the nested form.
    let para_mark = a.get("paraMark").filter(|v| v.is_object());
    for (key, prefix, kind) in [
        ("insertion", "ins", RevKind::Insertion),
        ("deletion", "del", RevKind::Deletion),
    ] {
        let nested = a.get(key).filter(|v| v.is_object()).or_else(|| {
            para_mark.filter(|m| json_str(m.get("type")).as_deref() == Some(key))
        });
        let pick = |field: &str| -> Option<String> {
            let flat = format!("{prefix}{}{}", field[..1].to_uppercase(), &field[1..]);
            nested
                .and_then(|n| json_str(n.get(field)))
                .or_else(|| json_str(a.get(flat.as_str())))
        };
        // A mark with neither author nor date nor id is not a revision at all.
        let (author, date, id) = (pick("author"), pick("date"), pick("id"));
        if nested.is_none() && author.is_none() && date.is_none() && id.is_none() {
            continue;
        }
        out.push(Revision {
            kind,
            id: id.unwrap_or_default(),
            author: author.unwrap_or_default(),
            date: date.as_deref().and_then(docx_date),
        });
    }
    out
}

/// `<w:ins/>` / `<w:del/>` for the paragraph mark, to place FIRST inside the
/// `<w:rPr>` of a `<w:pPr>`: `CT_ParaRPr` starts with ins, del, moveFrom,
/// moveTo, and Word rejects a document whose children deviate from that order.
pub(crate) fn paragraph_mark_props(attrs: Option<&Value>, ctx: &mut ExportCtx) -> String {
    let mut out = String::new();
    for rev in paragraph_mark_revisions(attrs) {
        out.push_str(&format!("<{0}{1}/>", rev.kind.element(), change_attrs(&rev, ctx)));
    }
    out
}

/// The whole `<w:rPr>` of the paragraph mark, empty when nothing is tracked.
/// It belongs at the END of `<w:pPr>` (after `w:outlineLvl`, before
/// `w:sectPr`), per `CT_PPr`.
pub(crate) fn paragraph_mark_rpr(node: &PmNode, ctx: &mut ExportCtx) -> String {
    let props = paragraph_mark_props(node.attrs.as_ref(), ctx);
    if props.is_empty() {
        String::new()
    } else {
        format!("<w:rPr>{props}</w:rPr>")
    }
}

/// First non-empty value among `keys`, tolerating the number form a JSON
/// round-trip can produce for an id.
fn mark_attr(m: &PmMark, keys: &[&str]) -> Option<String> {
    let attrs = m.attrs.as_ref()?;
    keys.iter().find_map(|k| json_str(attrs.get(*k)))
}

fn json_str(v: Option<&Value>) -> Option<String> {
    let s = match v? {
        Value::String(s) => s.trim().to_string(),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    (!s.is_empty()).then_some(s)
}

/// A date from the model → the `xsd:dateTime` form Word writes, or `None`.
///
/// `w:date` is typed, so a value the schema rejects is not a cosmetic problem:
/// Word declares the whole document unreadable. Anything not understood here is
/// therefore dropped rather than passed through — the change itself survives,
/// only its timestamp is lost.
fn docx_date(raw: &str) -> Option<String> {
    const OUT: &str = "%Y-%m-%dT%H:%M:%SZ";
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    // Epoch milliseconds, the form the collaborative layer stores.
    if let Ok(ms) = s.parse::<i64>() {
        return chrono::DateTime::from_timestamp_millis(ms).map(|d| d.format(OUT).to_string());
    }
    if let Ok(d) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(d.with_timezone(&chrono::Utc).format(OUT).to_string());
    }
    // Zone-less ISO forms: assume UTC, which is what the contract stores.
    for fmt in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M:%S"] {
        if let Ok(d) = chrono::NaiveDateTime::parse_from_str(s, fmt) {
            return Some(d.format(OUT).to_string());
        }
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(d.format("%Y-%m-%dT00:00:00Z").to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rev_mark(kind: &str, id: &str, author: &str, date: &str) -> PmMark {
        PmMark {
            mark_type: kind.into(),
            attrs: Some(json!({ "id": id, "author": author, "date": date })),
        }
    }

    fn ins(id: &str) -> PmMark {
        rev_mark("insertion", id, "Jean Dupont", "2026-07-28T09:12:00Z")
    }

    fn del(id: &str) -> PmMark {
        rev_mark("deletion", id, "Marie Curie", "2026-07-28T10:00:00Z")
    }

    fn nodes_of(marks: &[Vec<PmMark>]) -> Vec<PmNode> {
        marks
            .iter()
            .enumerate()
            .map(|(i, m)| PmNode::text(format!("t{i}"), m.clone()))
            .collect()
    }

    /// The whole point of the module: one wrapper per change, not per run.
    #[test]
    fn consecutive_runs_share_one_wrapper() {
        let nodes = nodes_of(&[
            vec![ins("c1")],
            vec![ins("c1"), PmMark { mark_type: "bold".into(), attrs: None }],
            vec![ins("c2")],
            vec![],
        ]);
        assert_eq!(group_len(&nodes, 0), 2);
        assert_eq!(group_len(&nodes, 2), 1);
        assert_eq!(group_len(&nodes, 3), 1);
        assert_eq!(group_len(&nodes, 4), 0);

        let mut ctx = ExportCtx::new();
        let xml = wrap(&node_revisions(&nodes[0]), "<w:r/><w:r/>", &mut ctx);
        assert_eq!(xml.matches("<w:ins ").count(), 1, "{xml}");
        assert!(xml.contains(r#"w:author="Jean Dupont" w:date="2026-07-28T09:12:00Z""#), "{xml}");
    }

    /// The classic bug: a `w:t` inside a `w:del` resurrects the deleted text.
    #[test]
    fn deletion_turns_w_t_into_w_del_text() {
        let mut ctx = ExportCtx::new();
        let run = format!("<w:r><w:rPr><w:b/></w:rPr>{}</w:r>", text_to_run_children("a\tb\nc"));
        let xml = wrap(&revisions_of(&[del("d1")]), &run, &mut ctx);
        assert!(!xml.contains("<w:t "), "un w:t subsiste dans le w:del : {xml}");
        assert!(!xml.contains("</w:t>"), "{xml}");
        assert_eq!(xml.matches("<w:delText ").count(), 3, "{xml}");
        assert_eq!(xml.matches("</w:delText>").count(), 3, "{xml}");
        // Structure around the text must be untouched.
        assert!(xml.contains("<w:tab/>") && xml.contains("<w:br/>"), "{xml}");
        assert!(xml.contains("<w:b/>"), "{xml}");
        assert!(xml.starts_with("<w:del ") && xml.ends_with("</w:del>"), "{xml}");
    }

    /// `w:instrText` has its own deleted form, and `<w:tab/>` must NOT match the
    /// `<w:t` prefix.
    #[test]
    fn only_text_elements_are_renamed() {
        let src = r#"<w:tbl/><w:tab/><w:instrText> PAGE </w:instrText><w:t>x</w:t><w:tc/>"#;
        let out = to_deleted_text(src);
        assert!(out.contains("<w:tbl/>") && out.contains("<w:tab/>"), "{out}");
        assert!(out.contains("<w:delInstrText> PAGE </w:delInstrText>"), "{out}");
        assert!(out.contains("<w:delText>x</w:delText>"), "{out}");
        assert!(out.contains("<w:tc/>"), "{out}");
    }

    /// Inserted then deleted: Word's shape is `<w:ins><w:del>`.
    #[test]
    fn insertion_then_deletion_nests() {
        let mut ctx = ExportCtx::new();
        let revs = revisions_of(&[del("d1"), ins("c1")]);
        assert_eq!(revs.len(), 2);
        let xml = wrap(&revs, "<w:r><w:t>x</w:t></w:r>", &mut ctx);
        assert!(xml.starts_with("<w:ins "), "{xml}");
        assert!(xml.contains("<w:del "), "{xml}");
        assert!(xml.contains("<w:delText>x</w:delText>"), "{xml}");
        let i = |s: &str| xml.find(s).unwrap_or(usize::MAX);
        assert!(i("<w:ins ") < i("<w:del "), "{xml}");
    }

    /// Ids must be unique document-wide, and they are annotation ids — NOT the
    /// editor's change id.
    #[test]
    fn ids_are_unique_and_attributes_escaped() {
        let mut ctx = ExportCtx::new();
        let hostile = PmMark {
            mark_type: "insertion".into(),
            attrs: Some(json!({ "id": "c1", "author": r#"A"><script>&"#, "date": "" })),
        };
        let revs = revisions_of(&[hostile]);
        let a = wrap(&revs, "<w:r/>", &mut ctx);
        let b = wrap(&revs, "<w:r/>", &mut ctx);
        assert!(a.contains(r#"w:author="A&quot;&gt;&lt;script&gt;&amp;""#), "{a}");
        // No usable date: the attribute is omitted rather than written empty.
        assert!(!a.contains("w:date="), "{a}");
        let id_of = |s: &str| s.split(r#"w:id=""#).nth(1).and_then(|t| t.split('"').next()).map(str::to_string);
        assert_ne!(id_of(&a), id_of(&b), "{a} / {b}");
    }

    #[test]
    fn empty_slice_and_empty_content() {
        let mut ctx = ExportCtx::new();
        assert_eq!(wrap(&[], "<w:r/>", &mut ctx), "<w:r/>");
        assert_eq!(wrap(&revisions_of(&[ins("c1")]), "", &mut ctx), "");
    }

    #[test]
    fn dates_are_normalised_or_dropped() {
        assert_eq!(docx_date("2026-07-28T09:12:00Z").as_deref(), Some("2026-07-28T09:12:00Z"));
        assert_eq!(docx_date("2026-07-28T11:12:00+02:00").as_deref(), Some("2026-07-28T09:12:00Z"));
        assert_eq!(docx_date("2026-07-28T09:12:00.123Z").as_deref(), Some("2026-07-28T09:12:00Z"));
        assert_eq!(docx_date("2026-07-28").as_deref(), Some("2026-07-28T00:00:00Z"));
        assert_eq!(docx_date("1753699920000").as_deref(), Some("2025-07-28T10:52:00Z"));
        assert_eq!(docx_date("hier"), None);
        assert_eq!(docx_date("  "), None);
    }

    /// An unattributed change must not panic, and must still be a change.
    #[test]
    fn unattributed_change_is_written() {
        let mut ctx = ExportCtx::new();
        let m = PmMark { mark_type: "deletion".into(), attrs: None };
        let revs = revisions_of(&[m]);
        assert_eq!(revs.len(), 1);
        let xml = wrap(&revs, "<w:r><w:t>x</w:t></w:r>", &mut ctx);
        assert!(xml.contains(r#"w:author="""#), "{xml}");
        assert!(xml.contains("<w:delText>x</w:delText>"), "{xml}");
    }

    #[test]
    fn paragraph_mark_accepts_both_attribute_shapes() {
        let mut ctx = ExportCtx::new();
        let nested = PmNode {
            node_type: "paragraph".into(),
            attrs: Some(json!({ "insertion": { "author": "Jean", "date": "2026-07-28T09:12:00Z", "id": "c1" } })),
            content: None,
            marks: None,
            text: None,
        };
        let xml = paragraph_mark_rpr(&nested, &mut ctx);
        assert!(xml.starts_with("<w:rPr><w:ins "), "{xml}");
        assert!(xml.contains(r#"w:author="Jean" w:date="2026-07-28T09:12:00Z""#), "{xml}");

        let flat = PmNode {
            node_type: "paragraph".into(),
            attrs: Some(json!({ "delAuthor": "Marie", "delDate": "2026-07-28T10:00:00Z" })),
            content: None,
            marks: None,
            text: None,
        };
        let xml = paragraph_mark_rpr(&flat, &mut ctx);
        assert!(xml.contains("<w:del "), "{xml}");
        // A paragraph with no tracked mark writes nothing at all.
        let plain = PmNode {
            node_type: "paragraph".into(),
            attrs: Some(json!({ "textAlign": "center" })),
            content: None,
            marks: None,
            text: None,
        };
        assert_eq!(paragraph_mark_rpr(&plain, &mut ctx), "");
    }

    /// Produces a REAL `.docx` carrying an insertion and a deletion, for
    /// verification by an actual reader:
    ///
    /// ```text
    /// cargo test -p kubuno-office revisions::tests::emit -- --ignored --nocapture
    /// soffice --headless --norestore -env:UserInstallation=file:///tmp/lo-check \
    ///         --convert-to fodt --outdir /tmp /tmp/kubuno_revisions.docx
    /// ```
    ///
    /// The FODT must contain `text:tracked-changes`, `text:insertion` and
    /// `text:deletion`, and the deleted text must NOT appear as body text.
    /// Ignored by default: it writes a file and is meant to be driven by hand.
    #[test]
    #[ignore]
    fn emit_sample_docx() {
        use std::io::{Cursor, Read, Write};

        // A valid package from the normal exporter, whose placeholder run is
        // then swapped for the revision markup: everything the package needs
        // (content types, relationships, styles) stays owned by its writer.
        let doc = PmNode::doc(vec![
            PmNode::paragraph(vec![PmNode::text("Texte normal avant. ", vec![])]),
            PmNode::paragraph(vec![PmNode::text("@@REVISIONS@@", vec![])]),
            PmNode::paragraph(vec![PmNode::text("Texte normal apres.", vec![])]),
        ]);
        let base = super::super::export_docx(&doc, "Revisions").expect("export");

        let mut ctx = ExportCtx::new();
        let mut fragment = String::new();
        fragment.push_str(&wrap(
            &revisions_of(&[ins("c1")]),
            "<w:r><w:t xml:space=\"preserve\">INSERE-PAR-JEAN </w:t></w:r>",
            &mut ctx,
        ));
        fragment.push_str(&wrap(
            &revisions_of(&[del("d1")]),
            &format!("<w:r>{}</w:r>", text_to_run_children("SUPPRIME-PAR-MARIE")),
            &mut ctx,
        ));
        let patched_body = format!("</w:r>{fragment}<w:r>");

        let mut zin = zip::ZipArchive::new(Cursor::new(base)).expect("zip in");
        let mut zout = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts = zip::write::FileOptions::<()>::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for i in 0..zin.len() {
            let mut f = zin.by_index(i).expect("entry");
            let name = f.name().to_string();
            let mut data = Vec::new();
            f.read_to_end(&mut data).expect("read");
            if name == "word/document.xml" {
                let xml = String::from_utf8(data).expect("utf8");
                let placeholder = r#"<w:t xml:space="preserve">@@REVISIONS@@</w:t>"#;
                assert!(xml.contains(placeholder), "placeholder introuvable");
                data = xml.replace(placeholder, &patched_body).into_bytes();
            }
            zout.start_file(name, opts).expect("start");
            zout.write_all(&data).expect("write");
        }
        let out = zout.finish().expect("finish").into_inner();
        let path = std::env::var("KUBUNO_DOCX_OUT")
            .unwrap_or_else(|_| "/tmp/kubuno_revisions.docx".to_string());
        std::fs::write(&path, out).expect("write file");
        println!("docx: {path}");
    }
}
