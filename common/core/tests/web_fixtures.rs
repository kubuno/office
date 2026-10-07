//! Documents written by the web editor (`fixtures/web/`, see the README): the engine reads what the web reads and
//! writes back, unchanged, the very bytes the web stored.

use kubuno_office_docs_core::editor::Editor;
use kubuno_office_docs_core::measure::FixedMeasure;

fn open(name: &str) -> (Vec<u8>, Editor) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/web").join(name);
    let bytes = std::fs::read(&path).expect("the fixture is there");
    let mut editor = Editor::open(&bytes).expect("opens");
    editor.relayout(&FixedMeasure);
    (bytes, editor)
}

#[test]
fn an_unchanged_web_document_is_written_back_byte_for_byte() {
    for name in ["basic.json", "decorations.json", "track.json"] {
        let (bytes, mut editor) = open(name);
        assert_eq!(editor.to_bytes().expect("serialises"), bytes, "{name}");
    }
}

#[test]
fn the_decorations_the_web_paints_are_read_from_its_envelope() {
    let (_, editor) = open("decorations.json");
    assert_eq!(editor.pages().len(), 1);
    let ctx = editor.decor_context("Décorations", "07/10/2026");
    let d = editor.page_decor(0, &ctx, &FixedMeasure).expect("page 0");
    let header: String = d.header.as_ref().expect("header").paragraphs[0].lines[0].spans.iter().map(|s| &*s.text).collect();
    assert_eq!(header, "Décorations — page i sur i");
    let footer: String = d.footer.as_ref().expect("footer").paragraphs[0].lines[0].spans.iter().map(|s| &*s.text).collect();
    assert_eq!(footer, "Pied de page Kubuno");
    assert_eq!(d.page_number.as_ref().map(|l| l.text.as_str()), Some("i"));
    assert_eq!(d.background.as_deref(), Some("#fff8e1"));
    assert_eq!(d.border.as_ref().map(|b| (b.style.as_str(), b.margin)), Some(("solid", 24.0)));
    assert_eq!(d.watermark.as_ref().map(|w| w.text.as_str()), Some("BROUILLON"));
    assert_eq!(d.line_numbers.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["1", "2", "3"]);
    assert!(d.footnotes[0].text.starts_with("1. La note de bas de page"));
}

#[test]
fn the_basic_web_document_lays_out_its_structures() {
    let (_, editor) = open("basic.json");
    assert_eq!((editor.orientation(0), editor.paper_size().as_str(), editor.columns()), ("portrait", "a4", 1));
    let lines: Vec<String> = editor.pages()[0].paragraphs.iter().flat_map(|p| p.lines.iter()).map(|l| l.spans.iter().map(|s| &*s.text).collect()).collect();
    assert!(lines.iter().any(|l| l.starts_with("Rapport trimestriel")), "{lines:?}");
    assert!(lines.iter().any(|l| l.contains("Chiffre d'affaires en hausse")), "{lines:?}");
}

#[test]
fn a_click_on_a_footnote_reference_finds_the_note() {
    let (_, editor) = open("decorations.json");
    let layout = editor.layout().expect("laid out");
    let (line, span) = layout
        .lines()
        .flat_map(|(_, l)| l.spans.iter().map(move |s| (l, s)))
        .find(|(_, s)| matches!(s.kind, kubuno_office_docs_core::layout::SpanKind::Footnote { .. }))
        .expect("a footnote reference");
    let pos = editor.note_at_point(span.x + span.width / 2.0, line.y + line.height / 2.0);
    assert_eq!(pos, Some(span.pm_pos));
    assert_eq!(kubuno_office_docs_core::edit::commands::note_at(editor.doc(), span.pm_pos).map(|n| n.0).as_deref(), Some("footnote"));
    assert_eq!(editor.note_at_point(span.x + span.width + 30.0, line.y + 1.0), None, "next to it is not on it");
}

#[test]
fn the_revisions_the_web_tracked_are_listed_accepted_and_rejected() {
    use kubuno_office_docs_core::edit::track::ChangeKind;
    let (_, mut editor) = open("track.json");
    assert!(editor.tracking(), "trackChanges is on in the envelope");
    let changes = editor.changes();
    let seen: Vec<(ChangeKind, &str, &str)> = changes.iter().map(|c| (c.kind, c.text.as_str(), c.author.as_str())).collect();
    assert_eq!(
        seen,
        [
            (ChangeKind::Deletion, "chat", "Docs Tester"),
            (ChangeKind::Insertion, "chien", "Docs Tester"),
            (ChangeKind::Deletion, "rase à suppr", "Docs Tester"),
            (ChangeKind::Insertion, " Ajout final.", "Docs Tester"),
        ]
    );
    let text = |e: &Editor| e.doc().children().iter().map(|p| p.children().iter().filter_map(|n| n.text()).collect::<String>()).collect::<Vec<_>>();
    let mut accepted = Editor::open(&editor.to_bytes().expect("bytes")).expect("opens");
    accepted.decide_changes(1000, None, true).expect("accepted");
    assert_eq!(text(&accepted), ["Le chien dort sur le canapé.", "Une phimer en partie. Ajout final."]);
    editor.decide_changes(1000, None, false).expect("rejected");
    assert_eq!(text(&editor), ["Le chat dort sur le canapé.", "Une phrase à supprimer en partie."]);
    assert!(editor.changes().is_empty());
}
