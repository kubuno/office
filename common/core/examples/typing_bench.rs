//! Where the time of one keystroke goes on a long document: the edit, the relayout (flow +
//! pagination), the caret and selection geometry, the ribbon's selection state and a full
//! serialisation (what the server session is handed). Fixed font metrics, so it measures the engine,
//! not a font system.
//!
//! `cargo run --release --example typing_bench -- <content.json> [chars]`
//! (`-` for the file: a generated document of `[paragraphs]` long paragraphs, 1 700 by default, about 640 pages)
//! `cargo run --release --example typing_bench -- - 50 135` (about 50 pages)

use std::time::{Duration, Instant};

use kubuno_office_docs_core::editor::Editor;
use kubuno_office_docs_core::measure::FixedMeasure;

fn generated(count: usize) -> Vec<u8> {
    let line = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(20);
    let paras: Vec<String> = (0..count).map(|i| format!(r#"{{"type":"paragraph","content":[{{"type":"text","text":"{i} {line}"}}]}}"#)).collect();
    format!(r#"{{"type":"doc","content":[{}]}}"#, paras.join(",")).into_bytes()
}

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v.get(v.len() / 2).copied().unwrap_or_default()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let bytes = match args.first().filter(|p| !p.is_empty() && *p != "-") {
        Some(path) => std::fs::read(path).expect("reads the document"),
        None => generated(args.get(2).and_then(|n| n.parse().ok()).unwrap_or(1700)),
    };
    let chars: usize = args.get(1).and_then(|n| n.parse().ok()).unwrap_or(50);
    let m = FixedMeasure;

    let t = Instant::now();
    let mut editor = Editor::open(&bytes).expect("opens");
    let open = t.elapsed();
    let t = Instant::now();
    editor.relayout(&m);
    let first_layout = t.elapsed();
    println!("document: {} bytes, {} pages; open {:?}, first layout {:?}", bytes.len(), editor.pages().len(), open, first_layout);

    probes(&editor);
    let (mut edit, mut layout, mut caret, mut state, mut ser) = (vec![], vec![], vec![], vec![], vec![]);
    for i in 0..chars {
        let t = Instant::now();
        editor.apply(i as u64 * 1000, |s| kubuno_office_docs_core::edit::commands::insert_text(s, "x")).expect("types");
        edit.push(t.elapsed());
        let t = Instant::now();
        editor.relayout(&m);
        layout.push(t.elapsed());
        let t = Instant::now();
        let _ = editor.caret_on_page(&m);
        let _ = editor.selection_on_pages(&m);
        caret.push(t.elapsed());
        let t = Instant::now();
        let _ = editor.selection_info();
        state.push(t.elapsed());
        let t = Instant::now();
        let _ = editor.to_bytes().expect("serialises");
        ser.push(t.elapsed());
    }
    println!(
        "per keystroke (median of {chars}): edit {:?}, relayout {:?}, caret+selection {:?}, selection state {:?}, serialisation {:?}",
        median(edit),
        median(layout),
        median(caret),
        median(state),
        median(ser)
    );
}

#[allow(dead_code)]
fn probes(editor: &Editor) {
    use kubuno_office_docs_core::layout::paginate;
    use kubuno_office_docs_core::pm;
    let doc = editor.doc();
    let t = Instant::now();
    let n = pm::node_size(doc);
    println!("node_size(doc) = {n} in {:?}", t.elapsed());
    let t = Instant::now();
    let _ = pm::textblock_at(doc, n / 2);
    println!("textblock_at(middle) in {:?}", t.elapsed());
    if let Some(layout) = editor.layout() {
        let geoms: Vec<paginate::SectionGeom> = vec![paginate::SectionGeom { content_h: 931.0, columns: 1, col_w: 602.0, col_gap: 0.0 }];
        let t = Instant::now();
        let pages = paginate::paginate(layout, &geoms);
        println!("paginate: {} pages in {:?}", pages.len(), t.elapsed());
        let t = Instant::now();
        let c = layout.clone();
        println!("clone of the continuous layout in {:?} ({} paragraphs)", t.elapsed(), c.paragraphs.len());
    }
}
