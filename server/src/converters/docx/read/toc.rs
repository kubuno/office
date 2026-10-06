//! Recognising a table of contents on import.
//!
//! A TOC is not a structure in OOXML: it is a `TOC` field whose result is a run
//! of paragraphs styled `TOC1`..`TOC9`, each ending with a tab and a `PAGEREF`
//! giving the page. The editor models it with the `tocTitle` / `tocLevel` /
//! `tocPage` / `tocLeader` attributes it already renders, so the job here is
//! recognition, not invention.
//!
//! What the frontend expects (`generateToc`, DocumentEditorPage.tsx:14105) and
//! what the canvas paints (canvas-engine.ts:1827) — reproduced EXACTLY:
//!
//! - the title paragraph is a `heading` of level 2 carrying `tocTitle: true`;
//! - each entry is a `paragraph` with `tocLevel` (1..9), `indent` (= level − 1,
//!   in indent STEPS, not px), `tocPage` (a number or null) and `tocLeader`;
//! - the page number and the dot leader are PAINTED from those attributes, so
//!   the tab and the number must not survive as text — they would show twice.
//!
//! Reference: `handleToc`, DomainMapper_Impl.cxx:7420-7620.
//
// The helpers below are the surface `read/paragraph.rs` (entries + title) and
// `read/styles.rs` (style name → level) call into. Until those call sites land
// they look unused to the compiler.

use roxmltree::Node;
use serde_json::{json, Map, Value};

use crate::converters::docx::model::FontCtx;
use crate::converters::docx::xml::{attr_val, child, local};
use crate::converters::types::PmNode;

use super::fields::FieldState;

/// The heading level the editor gives a TOC title (`generateToc` hard-codes 2).
pub(crate) const TOC_TITLE_LEVEL: u8 = 2;

// ─── Style recognition ───────────────────────────────────────────────────────

/// Style labels that mean "TOC entry", once folded by [`fold_label`]. Word keeps
/// the digit but LOCALISES the rest: a French document says `TM1`, a German one
/// `Verzeichnis1`, a Spanish LibreOffice one `Ndice1` (Word strips accents from
/// styleIds, hence the accent-less spellings).
const TOC_LABELS: &[&str] = &[
    "toc",
    "tableofcontents",
    "tabledesmatieres",
    "tabledesmatires",
    "contents",
    "contenu",
    "verzeichnis",
    "inhaltsverzeichnis",
    "inhoud",
    "inhoudsopgave",
    "inhopg",
    "indice",
    "ndice",
    "sommario",
    "sumario",
    "sumrio",
    "indholdsfortegnelse",
    "indhold",
    "innehall",
    "innehll",
    "innhold",
    "innholdsfortegnelse",
    "spistreci",
    "spistresci",
    "obsah",
    "sisallysluettelo",
    "sisluet",
    "tartalomjegyzek",
    "tartalomjegyzk",
    "icindekiler",
    "indekiler",
    "cuprins",
    "sadrzaj",
    "sadraj",
];

/// Short abbreviations Word uses for the localised TOC styles. Two or three
/// letters would match far too much as a prefix, so they only count as an EXACT
/// label (`TM1`, `TDC2`, `TJ3`).
const TOC_LABELS_EXACT: &[&str] = &["tm", "tdc", "tj"];

/// Words meaning "heading" inside the localised name of the `TOCHeading` style
/// (German `InhaltsverzeichnisÜberschrift`, Spanish `TDC Encabezado`…).
const HEADING_WORDS: &[&str] = &[
    "heading", "berschrift", "uberschrift", "titre", "titel", "titulo", "ttulo", "titolo", "kop",
    "rubrik", "overskrift", "nadpis", "otsikko", "cmsor", "cimsor", "entete", "entte", "encabez",
    "cabec", "balk", "baslik",
];

/// Lowercase, strip accents, keep only alphanumerics: `"Índice 1"` → `"ndice1"`,
/// `"TOC Heading"` → `"tocheading"`. Word already strips non-ASCII from styleIds,
/// so folding the NAME the same way makes the two comparable.
fn fold_label(s: &str) -> String {
    s.chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'à'..='å' => 'a',
            'è'..='ë' => 'e',
            'ì'..='ï' => 'i',
            'ò'..='ö' | 'ø' => 'o',
            'ù'..='ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            'ý' | 'ÿ' => 'y',
            other => other,
        })
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

/// Split a folded label into `(word, trailing digit)`: `"toc3"` → `("toc", 3)`.
/// Exactly ONE trailing digit — `TOC10` is not a level.
fn split_trailing_level(folded: &str) -> Option<(&str, u8)> {
    let digits = folded.len() - folded.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    if digits != 1 {
        return None;
    }
    let (word, num) = folded.split_at(folded.len() - 1);
    let lvl = num.parse::<u8>().ok().filter(|l| (1..=9).contains(l))?;
    Some((word, lvl))
}

/// Level of a TOC style from its styleId OR its localised name — shared by
/// [`toc_level_from_style`] and [`name_to_toc_level`].
fn level_from_label(label: &str) -> Option<u8> {
    let folded = fold_label(label);
    let (word, lvl) = split_trailing_level(&folded)?;
    let known = TOC_LABELS.iter().any(|p| word.starts_with(p)) || TOC_LABELS_EXACT.contains(&word);
    known.then_some(lvl)
}

/// Entry level of a TOC paragraph, from its style. Word uses the styleId
/// `TOC1`..`TOC9`; localised builds keep the digit but translate the NAME, so
/// both are worth looking at.
///
/// NOTE: `FontCtx` exposes no styleId → style NAME map today (only
/// `heading_levels`, already distilled from names in `read/styles.rs`), so the
/// name half cannot be consulted from here yet — see [`name_to_toc_level`],
/// which `read/styles.rs` can use to build a `toc_levels` map exactly the way it
/// builds `heading_levels`. `fonts` serves the negative check that matters in
/// practice: a paragraph styled as a HEADING is never a TOC entry.
pub(crate) fn toc_level_from_style(style: &str, fonts: &FontCtx) -> Option<u8> {
    if style.is_empty() || fonts.heading_levels.contains_key(style) {
        return None;
    }
    level_from_label(style)
}

/// Level of a TOC style from its `<w:name>` — `"toc 1"` in an English file and in
/// most Word-localised ones, `"Índice 1"` in a LibreOffice-written one. For
/// `read/styles.rs`, the only place that reads style names.
#[allow(dead_code)] // reserved for read/styles.rs, not wired yet
pub(crate) fn name_to_toc_level(name: &str) -> Option<u8> {
    level_from_label(name)
}

/// Does this label name the TOC TITLE style? `TOCHeading` in English, and in a
/// localised file a TOC word glued to a heading word
/// (`InhaltsverzeichnisÜberschrift`).
fn is_toc_heading_label(label: &str) -> bool {
    let folded = fold_label(label);
    let toc = TOC_LABELS.iter().any(|p| folded.starts_with(p));
    toc && HEADING_WORDS.iter().any(|h| folded.contains(h))
}

/// `<w:pStyle>` of the "Table of contents" title paragraph. It sits BEFORE the
/// field, so no field state can help: the style is the only clue.
pub(crate) fn is_toc_heading_style(style: &str) -> bool {
    is_toc_heading_label(style)
}

/// The same test on a style `<w:name>`, for `read/styles.rs`.
#[allow(dead_code)] // reserved for read/styles.rs, not wired yet
pub(crate) fn name_is_toc_heading(name: &str) -> bool {
    is_toc_heading_label(name)
}

/// Mark a paragraph's attributes as the TOC title. The caller must ALSO build the
/// node as a heading of [`TOC_TITLE_LEVEL`] — that is the shape `generateToc`
/// looks for when it replaces an existing block.
pub(crate) fn mark_toc_title(attrs: &mut Map<String, Value>) {
    attrs.insert("tocTitle".into(), json!(true));
}

// ─── Entry attributes ────────────────────────────────────────────────────────

/// Is a dot/dash leader drawn between the text and the page number? The TOC
/// paragraph carries the tab that does it:
/// `<w:tabs><w:tab w:val="right" w:leader="dot" w:pos="9016"/></w:tabs>`.
/// `None` when the paragraph declares no tab at all (it inherits the style's,
/// which we do not parse) — the caller then assumes Word's dotted default.
fn leader_from_ppr(p: &Node<'_, '_>) -> Option<bool> {
    let tabs = child(p, "pPr").and_then(|ppr| child(&ppr, "tabs"))?;
    let mut seen = false;
    for t in tabs.children().filter(|n| n.is_element() && local(n) == "tab") {
        seen = true;
        // `w:leader` defaults to "none" when absent (ECMA-376 §17.3.1.37).
        match attr_val(&t, "leader").as_deref() {
            Some("none") | None => {}
            Some(_) => return Some(true),
        }
    }
    seen.then_some(false)
}

/// A page-number tail: `"12"` → `Some(Some(12))`; a roman numeral → `Some(None)`
/// (front matter — the editor's `tocPage` only holds a number, but the text must
/// still go); anything else → `None`, meaning "not a page number, leave it".
fn page_tail(tail: &str) -> Option<Option<u32>> {
    let t = tail.trim();
    if t.is_empty() {
        return Some(None);
    }
    if t.len() <= 6 {
        if let Ok(n) = t.parse::<u32>() {
            return Some(Some(n));
        }
    }
    let roman = t.len() <= 8 && t.chars().all(|c| "ivxlcdmIVXLCDM".contains(c));
    roman.then_some(None)
}

/// Strip the trailing tab + page number of a TOC entry, returning the page number
/// when it was plain text.
///
/// The canvas repaints both from `tocPage`/`tocLeader`, so leaving them in the
/// content shows the number twice. Word puts the page in a `PAGEREF` field (whose
/// result the field machine already drops, leaving just the tab), but
/// LibreOffice-written and pasted TOCs carry it as plain text.
///
/// A tail that is neither a number nor a roman numeral is real content: it is
/// left untouched, tab included.
pub(crate) fn strip_toc_tail(inline: &mut Vec<PmNode>) -> Option<u32> {
    let is_text = |n: &PmNode| n.node_type == "text";
    // Trailing empty text nodes are noise the field machine may have left.
    while inline.last().is_some_and(|n| is_text(n) && n.text.as_deref().unwrap_or("").is_empty()) {
        inline.pop();
    }
    // The last tab, scanning back over TEXT nodes only (an image or a footnote
    // after the tab means this is not a plain entry).
    let i = inline
        .iter()
        .enumerate()
        .rev()
        .take_while(|(_, n)| is_text(n))
        .find(|(_, n)| n.text.as_deref().unwrap_or("").contains('\t'))
        .map(|(i, _)| i)?;
    let (head, mut tail) = {
        let t = inline[i].text.as_deref().unwrap_or("");
        // The scan above guarantees there is a tab in this node.
        let p = t.rfind('\t').unwrap_or(0);
        (t[..p].to_string(), t[p + 1..].to_string())
    };
    for n in inline.iter().skip(i + 1) {
        tail.push_str(n.text.as_deref().unwrap_or(""));
    }
    let page = page_tail(&tail)?;
    inline.truncate(i + 1);
    let head = head.trim_end().to_string();
    if head.is_empty() {
        inline.pop();
    } else if let Some(n) = inline.last_mut() {
        n.text = Some(head);
    }
    page
}

/// Turn a paragraph into a TOC entry when it is one: writes `tocLevel`,
/// `tocPage`, `tocLeader` and `indent` into `attrs`, and strips the tab and the
/// page number from `inline`. Returns false when the paragraph is not a TOC
/// entry, in which case nothing is touched.
///
/// `read/paragraph.rs` calls this once, after `effective_para_attrs`, with the
/// paragraph's effective attribute map and its parsed inline content.
pub(crate) fn apply_toc_entry(
    p: &Node<'_, '_>,
    style: &str,
    fonts: &FontCtx,
    fs: &mut FieldState,
    inline: &mut Vec<PmNode>,
    attrs: &mut Map<String, Value>,
) -> bool {
    // Inside a TOC field's result every non-empty paragraph is an entry, even
    // when its style is one we have no label for (an exotic locale): level 1 is a
    // better answer than losing the entry. A table of FIGURES (`\a` / `\c`) is
    // not a table of contents, so it never gets that benefit.
    let in_toc_field = fs.toc.is_some_and(|s| !s.table_of_figures);
    let level = match toc_level_from_style(style, fonts) {
        Some(l) => l,
        None if in_toc_field && !inline.is_empty() => 1,
        None => return false,
    };

    // The page number as PLAIN text (LibreOffice/pasted TOCs), and the one the
    // field machine captured from the closing PAGEREF (Word). Always take the
    // captured one, if only to clear it before the next paragraph.
    let text_page = strip_toc_tail(inline);
    let mut page = fs.take_toc_page().or(text_page);
    // `\n` hides the page number, for a range of levels only when it has one —
    // hence `page_numbers_at`, not the blanket `page_numbers`.
    if fs.toc.is_some_and(|s| !s.page_numbers_at(level)) {
        page = None;
    }

    attrs.insert("tocLevel".into(), json!(level));
    attrs.insert("tocPage".into(), page.map_or(Value::Null, |n| json!(n)));
    if page.is_some() && leader_from_ppr(p).unwrap_or(true) {
        attrs.insert("tocLeader".into(), json!(true));
    }
    // The editor indents entries in STEPS (`indent: level - 1`; canvas-engine
    // multiplies by LIST_INDENT). Word's own left indent would stack on top of
    // it, so it goes.
    attrs.insert("indent".into(), json!(level.saturating_sub(1)));
    attrs.remove("indentLeft");
    attrs.remove("indentFirstLine");
    true
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use roxmltree::Document as XmlDoc;

    use super::super::fields::{handle_field_run, ResultAction};
    use super::*;

    fn fonts() -> FontCtx {
        let mut heading_levels = HashMap::new();
        heading_levels.insert("Heading1".to_string(), 1u8);
        heading_levels.insert("Heading2".to_string(), 2u8);
        FontCtx {
            major: "Calibri Light".into(),
            minor: "Calibri".into(),
            default: None,
            default_size: 11.0,
            para_default: Map::new(),
            para_styles: HashMap::new(),
            para_style_default: None,
            heading_levels,
            run_styles: HashMap::new(),
        }
    }

    fn text(s: &str) -> PmNode {
        PmNode::text(s, vec![])
    }

    #[test]
    fn level_from_word_style_ids() {
        let f = fonts();
        assert_eq!(toc_level_from_style("TOC1", &f), Some(1));
        assert_eq!(toc_level_from_style("TOC3", &f), Some(3));
        assert_eq!(toc_level_from_style("TOC 2", &f), Some(2));
        // Localised styleIds actually present in the LibreOffice corpus.
        assert_eq!(toc_level_from_style("Verzeichnis2", &f), Some(2));
        assert_eq!(toc_level_from_style("Ndice3", &f), Some(3));
        assert_eq!(toc_level_from_style("TM1", &f), Some(1));
        assert_eq!(toc_level_from_style("Contents1", &f), Some(1));
        assert_eq!(toc_level_from_style("Sommario4", &f), Some(4));
        // Not TOC entries.
        assert_eq!(toc_level_from_style("Heading1", &f), None);
        assert_eq!(toc_level_from_style("TOCHeading", &f), None);
        assert_eq!(toc_level_from_style("TOC", &f), None);
        assert_eq!(toc_level_from_style("TOC10", &f), None);
        assert_eq!(toc_level_from_style("Custom1", &f), None);
        assert_eq!(toc_level_from_style("", &f), None);
    }

    #[test]
    fn level_from_style_names() {
        assert_eq!(name_to_toc_level("toc 1"), Some(1));
        assert_eq!(name_to_toc_level("Índice 3"), Some(3));
        assert_eq!(name_to_toc_level("Contents 2"), Some(2));
        assert_eq!(name_to_toc_level("heading 2"), None);
    }

    #[test]
    fn toc_heading_style_recognised() {
        assert!(is_toc_heading_style("TOCHeading"));
        assert!(is_toc_heading_style("TOC Heading"));
        // German Word, from custom-styles-TOC-comma.docx.
        assert!(is_toc_heading_style("Inhaltsverzeichnisberschrift"));
        assert!(name_is_toc_heading("TOC Heading"));
        assert!(!is_toc_heading_style("TOC1"));
        assert!(!is_toc_heading_style("Heading1"));
        assert!(!is_toc_heading_style(""));
    }

    #[test]
    fn tail_stripping() {
        // Word: the PAGEREF result is already gone, only the tab is left.
        let mut v = vec![text("Chapter one"), text("\t")];
        assert_eq!(strip_toc_tail(&mut v), None);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].text.as_deref(), Some("Chapter one"));

        // Plain-text TOC: tab, then the number.
        let mut v = vec![text("Chapter one"), text("\t"), text("12")];
        assert_eq!(strip_toc_tail(&mut v), Some(12));
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].text.as_deref(), Some("Chapter one"));

        // Tab and number inside a single node.
        let mut v = vec![text("Chapter one\t7")];
        assert_eq!(strip_toc_tail(&mut v), Some(7));
        assert_eq!(v[0].text.as_deref(), Some("Chapter one"));

        // Roman front matter: stripped, but not representable as a number.
        let mut v = vec![text("Foreword\tiv")];
        assert_eq!(strip_toc_tail(&mut v), None);
        assert_eq!(v[0].text.as_deref(), Some("Foreword"));

        // No tab (a `\n` TOC): untouched.
        let mut v = vec![text("Level 2")];
        assert_eq!(strip_toc_tail(&mut v), None);
        assert_eq!(v[0].text.as_deref(), Some("Level 2"));

        // A digit belonging to the title, with no tab before it.
        let mut v = vec![text("Annexe 2")];
        assert_eq!(strip_toc_tail(&mut v), None);
        assert_eq!(v[0].text.as_deref(), Some("Annexe 2"));

        // Real content after the tab stays put, tab included.
        let mut v = vec![text("Name\tstatus unknown")];
        assert_eq!(strip_toc_tail(&mut v), None);
        assert_eq!(v[0].text.as_deref(), Some("Name\tstatus unknown"));
    }

    /// Emulate what `read/run.rs` does for one paragraph: drive the field machine
    /// over the runs in document order, keep the text the machine allows, and feed
    /// PAGEREF text back as the entry's page number.
    fn collect_inline(p: &Node<'_, '_>, fs: &mut FieldState) -> Vec<PmNode> {
        fn walk(n: &Node<'_, '_>, fs: &mut FieldState, out: &mut Vec<PmNode>) {
            for c in n.children().filter(|c| c.is_element()) {
                match local(&c) {
                    "r" => {
                        if handle_field_run(&c, fs) {
                            continue;
                        }
                        let drop = fs.result_action() == ResultAction::Drop;
                        for rc in c.children().filter(|x| x.is_element()) {
                            match local(&rc) {
                                "t" => {
                                    let txt = rc.text().unwrap_or("");
                                    fs.note_toc_page(txt);
                                    if !drop {
                                        out.push(PmNode::text(txt, vec![]));
                                    }
                                }
                                "tab" if !drop => out.push(PmNode::text("\t", vec![])),
                                _ => {}
                            }
                        }
                    }
                    "hyperlink" | "smartTag" | "ins" => walk(&c, fs, out),
                    _ => {}
                }
            }
        }
        let mut out = Vec::new();
        walk(p, fs, &mut out);
        out
    }

    type Entry = (u8, Option<u64>, bool, String);

    /// Walk a real DOCX body and return the TOC titles plus, for every paragraph
    /// this module calls an entry, `(level, page, leader, text)`.
    fn toc_of(path: &std::path::Path) -> Option<(Vec<String>, Vec<Entry>)> {
        let bytes = std::fs::read(path).ok()?;
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes[..])).ok()?;
        let xml = crate::converters::docx::zip_io::read_zip_entry(&mut zip, "word/document.xml")?;
        let doc = XmlDoc::parse(&xml).ok()?;
        let f = fonts();
        let mut fs = FieldState::default();
        let mut titles = Vec::new();
        let mut entries: Vec<Entry> = Vec::new();
        for p in doc.descendants().filter(|n| local(n) == "p") {
            let style = child(&p, "pPr")
                .and_then(|ppr| child(&ppr, "pStyle"))
                .and_then(|s| attr_val(&s, "val"))
                .unwrap_or_default();
            let mut inline = collect_inline(&p, &mut fs);
            let joined: String = inline.iter().filter_map(|n| n.text.clone()).collect();
            let mut attrs = Map::new();
            if is_toc_heading_style(&style) {
                mark_toc_title(&mut attrs);
                titles.push(joined);
                continue;
            }
            if apply_toc_entry(&p, &style, &f, &mut fs, &mut inline, &mut attrs) {
                entries.push((
                    attrs.get("tocLevel").and_then(|v| v.as_u64()).unwrap_or(0) as u8,
                    attrs.get("tocPage").and_then(|v| v.as_u64()),
                    attrs.get("tocLeader").and_then(|v| v.as_bool()).unwrap_or(false),
                    inline.iter().filter_map(|n| n.text.clone()).collect(),
                ));
            }
        }
        Some((titles, entries))
    }

    #[test]
    fn corpus_toc_entries() {
        let Ok(root) = std::env::var("KUBUNO_DOCX_CORPUS") else {
            eprintln!("KUBUNO_DOCX_CORPUS absent — test ignoré");
            return;
        };
        let root = std::path::Path::new(&root);

        // TOC \o "1-3" \h \z \u — three entries, one per level, all on page 2.
        // LibreOffice reads the same thing: Contents_20_1..3 + "2" in each entry.
        let p = root.join("sw/qa/extras/ooxmlexport/data/toc_doc.docx");
        if let Some((titles, entries)) = toc_of(&p) {
            assert!(titles.is_empty(), "ce fichier n'a pas de TOCHeading");
            let expect = ["Heading 1", "Heading2", "Heading3"];
            assert_eq!(entries.len(), 3, "entrées: {entries:?}");
            for (i, e) in entries.iter().enumerate() {
                assert_eq!(e.0 as usize, i + 1, "niveau: {entries:?}");
                assert_eq!(e.1, Some(2), "page: {entries:?}");
                assert!(e.2, "points de suite: {entries:?}");
                // Neither the tab nor the page number may survive as text.
                assert_eq!(e.3, expect[i], "texte de l'entrée: {entries:?}");
            }
        }

        // TOC \n "2-2": level 2 loses its page number, level 3 keeps it.
        let p = root.join("sw/qa/extras/ooxmlexport/data/tdf162916_nastyTOC.docx");
        if let Some((titles, entries)) = toc_of(&p) {
            assert_eq!(titles, vec!["Contents".to_string()]);
            assert_eq!(entries.len(), 2, "entrées: {entries:?}");
            assert_eq!((entries[0].0, entries[0].1, entries[0].2), (2, None, false));
            assert_eq!((entries[1].0, entries[1].1), (3, Some(1)));
            assert!(entries.iter().all(|e| !e.3.contains('\t')), "{entries:?}");
        }

        // Localised (German) styleIds: TOCHeading = Inhaltsverzeichnisüberschrift,
        // entries = Verzeichnis1..3.
        let p = root.join("sw/qa/extras/ooxmlexport/data/custom-styles-TOC-comma.docx");
        if let Some((titles, entries)) = toc_of(&p) {
            assert_eq!(titles.len(), 1, "titre localisé manqué");
            assert!(entries.len() >= 3, "entrées: {entries:?}");
            assert_eq!(entries[0].0, 1);
            assert!(entries.iter().all(|e| e.1.is_some()), "pages: {entries:?}");
        }
    }
}
