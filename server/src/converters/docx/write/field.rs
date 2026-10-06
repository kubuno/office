//! Serialising Word fields.
//!
//! A field is not an element, it is a RUN SEQUENCE:
//!
//! ```text
//! fldChar begin → instrText → fldChar separate → cached result → fldChar end
//! ```
//!
//! Writing the instruction alone is the classic mistake: Word recomputes its
//! fields when it opens a file, but nothing else does — a viewer, a converter,
//! or Word itself with "update fields" declined then shows an EMPTY spot where
//! the page number was. The cached result is what makes the export readable
//! everywhere, so it is always written.
//!
//! Every run of the sequence carries the SAME `rPr`. See the diagram above
//! `DoWriteFieldRunProperties` (docxattributeoutput.cxx:3239, tdf#38778): the
//! result run is the only one the reader sees, but Word rebuilds it from the
//! other runs' properties on refresh — formatting just one of them makes a bold
//! page number lose its bold on the first F9.
//!
//! Mirrors `StartField_Impl` / `CmdField_Impl` / `EndField_Impl`
//! (sw/source/filter/ww8/docxattributeoutput.cxx:3041-3350).

use std::fmt::Display;

use chrono::Local;

use crate::converters::docx::xml::{escape_xml, text_to_run_children};
use crate::converters::types::PmMark;

use super::run::render_rpr;

/// `\* MERGEFORMAT` tells Word to re-apply the character formatting of the old
/// result to the new one when the field is refreshed. Without it, a page number
/// the user made bold or grey reverts to the paragraph's formatting on the first
/// recalculation. It only makes sense on fields whose result is a short piece of
/// text we format (PAGE, NUMPAGES, TITLE); a DATE carries a picture switch
/// instead, and a TOC rebuilds its result from paragraph styles.
const MERGEFORMAT: &str = r"\* MERGEFORMAT";

/// Date picture for the DATE field. `\@` takes a quoted picture and the
/// day/month order MUST be explicit: with no picture every reader falls back to
/// its own locale, and a French document read on a US machine shows the day and
/// the month swapped. Word's picture letters are case-sensitive (`MM` = month,
/// `mm` = minutes).
const DATE_PICTURE: &str = "dd/MM/yyyy";

/// A complete field. `cached` is what a reader that does not recompute shows.
///
/// `rpr` is an already-serialised `<w:rPr>` (possibly empty) — build it with
/// [`render_rpr`] so a field inherits the formatting of the text around it.
pub(crate) fn render_field(instr: &str, cached: &str, rpr: &str) -> String {
    let instr = instr.trim();
    if instr.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(instr.len() + cached.len() + 5 * rpr.len() + 256);
    out.push_str(&field_run(rpr, r#"<w:fldChar w:fldCharType="begin"/>"#));
    // `xml:space="preserve"` as in `DoWriteCmd` (docxattributeoutput.cxx:3085):
    // the instruction is padded with spaces the way Word writes it, and an XML
    // parser is free to collapse unprotected whitespace.
    out.push_str(&field_run(
        rpr,
        &format!(
            r#"<w:instrText xml:space="preserve"> {} </w:instrText>"#,
            escape_xml(instr)
        ),
    ));
    out.push_str(&field_run(rpr, r#"<w:fldChar w:fldCharType="separate"/>"#));
    if !cached.is_empty() {
        // `text_to_run_children` also turns a tab or a newline in the cached
        // result into `<w:tab/>` / `<w:br/>`, which `<w:t>` cannot hold.
        out.push_str(&field_run(rpr, &text_to_run_children(cached)));
    }
    out.push_str(&field_run(rpr, r#"<w:fldChar w:fldCharType="end"/>"#));
    out
}

/// One run of a field sequence: the same `rPr` for every one of them.
fn field_run(rpr: &str, body: &str) -> String {
    format!("<w:r>{rpr}{body}</w:r>")
}

/// `PAGE`, optionally with the editor's page-number format.
pub(crate) fn instr_page(num_fmt: Option<&str>) -> String {
    match num_fmt.and_then(numbering_switch) {
        // Word really does emit two `\*` switches here, the numbering one first;
        // `find_switch` on import reads that one (read/fields.rs:344).
        Some(sw) => format!(r"PAGE \* {sw} {MERGEFORMAT}"),
        None => format!(r"PAGE {MERGEFORMAT}"),
    }
}

pub(crate) fn instr_num_pages() -> String {
    format!(r"NUMPAGES {MERGEFORMAT}")
}

pub(crate) fn instr_title() -> String {
    format!(r"TITLE {MERGEFORMAT}")
}

pub(crate) fn instr_date() -> String {
    format!(r#"DATE \@ "{DATE_PICTURE}""#)
}

/// Editor `pageNumFormat` → the argument of the `\*` switch. Exact mirror of
/// `parse_numbering_type` (read/fields.rs:413): CASE MATTERS — `ROMAN` means
/// upper-case numbering, `roman` lower-case — so these strings must never be
/// upper-cased on the way out.
fn numbering_switch(fmt: &str) -> Option<&'static str> {
    match fmt {
        "roman-upper" => Some("ROMAN"),
        "roman-lower" => Some("roman"),
        "alpha-upper" => Some("ALPHABETIC"),
        "alpha-lower" => Some("alphabetic"),
        // Arabic is the default: no switch is shorter and just as correct.
        _ => None,
    }
}

/// Cached results for the fields of a header or footer.
///
/// These are placeholders, not truths: the exporter does not paginate, so it
/// cannot know the real page count. They exist so that a reader which never
/// refreshes shows a plausible number instead of a blank.
pub(crate) struct HfValues {
    pub(crate) page: String,
    pub(crate) pages: String,
    pub(crate) title: String,
    pub(crate) date: String,
    /// Editor `pageNumFormat` (`roman-upper`…), `None` for arabic.
    pub(crate) page_format: Option<String>,
}

impl Default for HfValues {
    fn default() -> Self {
        HfValues {
            page: "1".to_string(),
            pages: "1".to_string(),
            title: String::new(),
            date: Local::now().format("%d/%m/%Y").to_string(),
            page_format: None,
        }
    }
}

impl HfValues {
    /// Values for a document whose title and page-number format are known.
    pub(crate) fn new(title: &str, page_format: Option<&str>) -> Self {
        HfValues {
            title: title.to_string(),
            page_format: page_format.map(str::to_string),
            ..Default::default()
        }
    }
}

/// The dynamic tokens of the header/footer editor.
#[derive(Clone, Copy)]
enum HfToken {
    Page,
    Pages,
    Date,
    Title,
}

/// Token spellings accepted by `expandHFDoc`
/// (frontend/src/DocumentEditorPage.tsx:444). Its regexes carry the `i` flag, so
/// matching is case-insensitive here too, and `{title}` is an alias of
/// `{titre}`. Longest first, so a prefix can never win over a longer token.
const HF_TOKENS: [(&str, HfToken); 5] = [
    ("{pages}", HfToken::Pages),
    ("{titre}", HfToken::Title),
    ("{title}", HfToken::Title),
    ("{page}", HfToken::Page),
    ("{date}", HfToken::Date),
];

/// Header/footer text where `{page}` & co. become real Word fields.
///
/// Text around the tokens stays text, and a `{` that starts nothing is kept
/// verbatim: `{page` and `Coût {net}` must survive an export unchanged.
pub(crate) fn render_hf_text(text: &str, marks: &[PmMark]) -> String {
    render_hf_text_with(text, marks, &HfValues::default())
}

/// [`render_hf_text`] with explicit cached values.
pub(crate) fn render_hf_text_with(text: &str, marks: &[PmMark], vals: &HfValues) -> String {
    let rpr = render_rpr(marks);
    let mut out = String::new();
    let mut plain = String::new();
    let mut rest = text;
    while let Some(pos) = rest.find('{') {
        plain.push_str(&rest[..pos]);
        let tail = &rest[pos..];
        match match_hf_token(tail) {
            Some((token, len)) => {
                flush_text(&mut out, &mut plain, &rpr);
                out.push_str(&render_hf_token(token, &rpr, vals));
                rest = &tail[len..];
            }
            None => {
                // Not a token: an ordinary brace. `'{'` is one byte, so slicing
                // just after it always lands on a character boundary.
                plain.push('{');
                rest = &tail[1..];
            }
        }
    }
    plain.push_str(rest);
    flush_text(&mut out, &mut plain, &rpr);
    out
}

/// Token at the start of `s` (which begins with `{`), and its byte length.
fn match_hf_token(s: &str) -> Option<(HfToken, usize)> {
    HF_TOKENS.iter().find_map(|(lit, token)| {
        // Every token is pure ASCII, so a byte-length prefix is safe here.
        let head = s.get(..lit.len())?;
        head.eq_ignore_ascii_case(lit).then_some((*token, lit.len()))
    })
}

fn render_hf_token(token: HfToken, rpr: &str, vals: &HfValues) -> String {
    match token {
        HfToken::Page => {
            render_field(&instr_page(vals.page_format.as_deref()), &vals.page, rpr)
        }
        HfToken::Pages => render_field(&instr_num_pages(), &vals.pages, rpr),
        HfToken::Date => render_field(&instr_date(), &vals.date, rpr),
        HfToken::Title => render_field(&instr_title(), &vals.title, rpr),
    }
}

/// Emit the literal text gathered so far as one run, then reset the buffer.
fn flush_text(out: &mut String, plain: &mut String, rpr: &str) {
    if !plain.is_empty() {
        out.push_str(&field_run(rpr, &text_to_run_children(plain)));
        plain.clear();
    }
}

/// `PAGEREF` to a bookmark — the page number at the end of a TOC entry.
///
/// `page` is the cached number (`impl Display`, so a `u32` or a `&str` both fit);
/// it is what shows until the reader refreshes the TOC.
pub(crate) fn render_pageref(anchor: &str, page: impl Display) -> String {
    render_pageref_with_rpr(anchor, page, "")
}

/// [`render_pageref`] with the formatting of the entry it terminates.
pub(crate) fn render_pageref_with_rpr(anchor: &str, page: impl Display, rpr: &str) -> String {
    let name = bookmark_ref(anchor);
    if name.is_empty() {
        return String::new();
    }
    // `\h` makes the page number a live hyperlink to the heading, which is what
    // Word writes inside a TOC. No MERGEFORMAT: refreshing the TOC rebuilds the
    // whole entry from the heading's style anyway.
    render_field(&format!(r"PAGEREF {name} \h"), &page.to_string(), rpr)
}

/// Bookmark name usable inside a field instruction.
///
/// Same rules as `bookmark_ref` in `write/run.rs` (Word forbids whitespace and
/// truncates at 40 characters, wrtww8.cxx:4624-4646) plus the two characters an
/// instruction parser would choke on: a quote would end the argument early and a
/// backslash would start a switch, both turning the reference into a dead link.
fn bookmark_ref(name: &str) -> String {
    name.trim()
        .chars()
        .filter(|c| *c != '"' && *c != '\\')
        .map(|c| if c.is_whitespace() { '_' } else { c })
        .take(40)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Read, Write};
    use std::path::PathBuf;
    use std::process::Command;

    use super::*;
    use crate::converters::docx::export_docx;
    use crate::converters::docx::xml::simple_mark;
    use crate::converters::types::PmNode;

    #[test]
    fn field_is_a_full_run_sequence() {
        let xml = render_field("PAGE", "3", "");
        for part in [
            r#"<w:fldChar w:fldCharType="begin"/>"#,
            r#"<w:instrText xml:space="preserve"> PAGE </w:instrText>"#,
            r#"<w:fldChar w:fldCharType="separate"/>"#,
            r#"<w:t xml:space="preserve">3</w:t>"#,
            r#"<w:fldChar w:fldCharType="end"/>"#,
        ] {
            assert!(xml.contains(part), "séquence incomplète, {part} manquant: {xml}");
        }
        assert_eq!(xml.matches("<w:r>").count(), 5, "un run par étape: {xml}");
    }

    #[test]
    fn every_run_of_a_field_carries_the_same_rpr() {
        let rpr = render_rpr(&[simple_mark("bold")]);
        assert!(!rpr.is_empty());
        let xml = render_field("PAGE", "3", &rpr);
        assert_eq!(
            xml.matches(rpr.as_str()).count(),
            5,
            "le rPr doit être répété sur les 5 runs: {xml}"
        );
    }

    #[test]
    fn instruction_is_escaped_and_never_empty() {
        assert_eq!(render_field("   ", "x", ""), "");
        let xml = render_field(r#"HYPERLINK "a<b&c""#, "", "");
        assert!(xml.contains("&lt;b&amp;c"), "{xml}");
        assert!(
            !xml.contains("<w:t"),
            "pas de résultat en cache → pas de run texte: {xml}"
        );
    }

    #[test]
    fn hf_tokens_become_fields_and_keep_the_text_around_them() {
        let xml = render_hf_text("Page {page} sur {PAGES} — {titre}", &[]);
        assert!(xml.contains(r"> PAGE \* MERGEFORMAT <"), "{xml}");
        assert!(xml.contains(r"> NUMPAGES \* MERGEFORMAT <"), "{xml}");
        assert!(xml.contains(r"> TITLE \* MERGEFORMAT <"), "{xml}");
        assert!(xml.contains(r#"<w:t xml:space="preserve">Page </w:t>"#), "{xml}");
        assert!(xml.contains(r#"<w:t xml:space="preserve"> sur </w:t>"#), "{xml}");
        assert!(xml.contains(r#"<w:t xml:space="preserve"> — </w:t>"#), "{xml}");
    }

    #[test]
    fn lone_brace_is_plain_text() {
        let xml = render_hf_text("{page} {net} {pag", &[]);
        assert!(xml.contains(r"> PAGE \* MERGEFORMAT <"), "{xml}");
        // Everything after the field is literal, braces included. `{pag` is a
        // truncated token: it must not eat the end of the line either.
        assert!(xml.contains("{net} {pag"), "accolade isolée avalée: {xml}");
        assert_eq!(xml.matches(r#"fldCharType="begin""#).count(), 1, "{xml}");
        // An unterminated `{page ` is not a token: it stays text.
        let raw = render_hf_text("{page sur {pages", &[]);
        assert!(!raw.contains("fldChar"), "{raw}");
        assert!(raw.contains("{page sur {pages"), "{raw}");
    }

    #[test]
    fn no_token_means_no_field_at_all() {
        let xml = render_hf_text("Rapport annuel", &[]);
        assert!(!xml.contains("fldChar"), "{xml}");
        assert_eq!(xml.matches("<w:r>").count(), 1, "{xml}");
    }

    #[test]
    fn date_field_carries_an_explicit_picture() {
        let xml = render_hf_text("{date}", &[]);
        // The quotes of the picture are XML-escaped in element content.
        assert!(xml.contains(r"> DATE \@ &quot;dd/MM/yyyy&quot; <"), "{xml}");
        // The cached result must be a real date, not the instruction.
        let today = Local::now().format("%d/%m/%Y").to_string();
        assert!(xml.contains(&format!(">{today}<")), "{xml}");
    }

    #[test]
    fn page_number_format_survives_as_a_switch() {
        let vals = HfValues::new("Titre", Some("roman-lower"));
        let xml = render_hf_text_with("{page}", &[], &vals);
        assert!(xml.contains(r"> PAGE \* roman \* MERGEFORMAT <"), "{xml}");
    }

    #[test]
    fn pageref_sanitises_its_anchor() {
        let xml = render_pageref("_Toc mon \"titre\"\\x", 12u32);
        assert!(xml.contains(r"> PAGEREF _Toc_mon_titrex \h <"), "{xml}");
        assert!(xml.contains(r#"<w:t xml:space="preserve">12</w:t>"#), "{xml}");
        assert!(render_pageref("   ", 1u32).is_empty());
    }

    // ── Real round-trip through LibreOffice ────────────────────────────────

    /// Insert a paragraph of raw run XML into an exported package, before the
    /// final `sectPr`. The body writer does not emit fields yet (header.rs and
    /// toc.rs are the callers), so this is how a genuine `.docx` containing our
    /// fields is produced for the reopen test.
    fn docx_with_runs(runs: &str) -> Vec<u8> {
        let doc = PmNode::doc(vec![PmNode::paragraph(vec![PmNode::text("Avant", vec![])])]);
        let bytes = export_docx(&doc, "Champs").expect("export");

        let mut zin = zip::ZipArchive::new(Cursor::new(bytes)).expect("zip lisible");
        let mut zout = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts = zip::write::FileOptions::<()>::default()
            .compression_method(zip::CompressionMethod::Deflated);
        let mut patched = false;
        for i in 0..zin.len() {
            let mut f = zin.by_index(i).expect("entrée");
            let name = f.name().to_string();
            let mut data = Vec::new();
            f.read_to_end(&mut data).expect("lecture");
            if name == "word/document.xml" {
                let xml = String::from_utf8(data).expect("utf8");
                let para = format!("<w:p>{runs}</w:p>");
                let at = xml
                    .find("<w:sectPr")
                    .or_else(|| xml.find("</w:body>"))
                    .expect("corps");
                let mut new = String::with_capacity(xml.len() + para.len());
                new.push_str(&xml[..at]);
                new.push_str(&para);
                new.push_str(&xml[at..]);
                data = new.into_bytes();
                patched = true;
            }
            zout.start_file(name, opts).expect("start_file");
            zout.write_all(&data).expect("write");
        }
        assert!(patched, "document.xml introuvable");
        zout.finish().expect("finish").into_inner()
    }

    /// Convert to Flat ODF with LibreOffice and return the result. `None` when
    /// `soffice` is not installed: the assertions are then skipped rather than
    /// failing a machine with no office suite.
    fn reopen_with_libreoffice(bytes: &[u8], tag: &str) -> Option<String> {
        if Command::new("soffice").arg("--version").output().is_err() {
            eprintln!("soffice absent: test de réouverture ignoré");
            return None;
        }
        let dir: PathBuf = std::env::temp_dir()
            .join(format!("kubuno-field-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let docx = dir.join("fields.docx");
        std::fs::write(&docx, bytes).expect("écriture docx");

        let profile = format!("file://{}", dir.join("loprofile").display());
        let out = Command::new("soffice")
            .args(["--headless", "--norestore", "--convert-to", "fodt"])
            .arg(format!("-env:UserInstallation={profile}"))
            .arg("--outdir")
            .arg(&dir)
            .arg(&docx)
            .output()
            .expect("soffice");
        let fodt = dir.join("fields.fodt");
        assert!(
            fodt.exists(),
            "LibreOffice n'a rien produit — fichier refusé.\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let text = std::fs::read_to_string(&fodt).expect("lecture fodt");
        let _ = std::fs::remove_dir_all(&dir);
        Some(text)
    }

    #[test]
    fn libreoffice_reopens_the_fields_as_fields() {
        let runs = render_hf_text("Page {page} / {pages} — {date} — {titre}", &[]);
        let bytes = docx_with_runs(&runs);
        let Some(fodt) = reopen_with_libreoffice(&bytes, "hf") else {
            return;
        };
        // Recognised as live fields, not frozen text.
        assert!(fodt.contains("text:page-number"), "PAGE non reconnu:\n{fodt}");
        assert!(fodt.contains("text:page-count"), "NUMPAGES non reconnu");
        assert!(fodt.contains("text:date"), "DATE non reconnu");
        assert!(fodt.contains("text:title"), "TITLE non reconnu");
        // The literal text around the tokens survived.
        assert!(fodt.contains("Page "), "texte perdu");
    }

    #[test]
    fn libreoffice_reopens_a_pageref() {
        let runs = format!(
            r#"<w:bookmarkStart w:id="1" w:name="_Toc90001"/><w:bookmarkEnd w:id="1"/>{}"#,
            render_pageref("_Toc90001", 1u32)
        );
        let bytes = docx_with_runs(&runs);
        let Some(fodt) = reopen_with_libreoffice(&bytes, "ref") else {
            return;
        };
        assert!(
            fodt.contains("text:bookmark-ref") || fodt.contains("_Toc90001"),
            "PAGEREF non reconnu:\n{fodt}"
        );
    }
}
