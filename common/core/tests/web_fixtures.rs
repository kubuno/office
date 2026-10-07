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
    for name in ["basic.json", "decorations.json"] {
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
