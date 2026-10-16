//! Golden-fixture test: `tests/fixtures/live-board.kbwbd` is a real `.kbwbd` snapshot captured from a live
//! Kubuno whiteboard board on the dev core (the board's Yjs CRDT state, gzipped - the exact format the
//! server stores and the web editor syncs). It validates that the core decodes the server's actual bytes
//! and that a write-back round-trips the scene losslessly. See vskubuno `docs/WHITEBOARD-DESKTOP.md`.

use kubuno_office_whiteboard_core::engine::scene_from_doc;
use kubuno_office_whiteboard_core::file;
use kubuno_office_whiteboard_core::WbElement;

const GOLDEN: &[u8] = include_bytes!("fixtures/live-board.kbwbd");

fn kind(e: &WbElement) -> &'static str {
    match e {
        WbElement::Sticky(_) => "sticky",
        WbElement::Text(_) => "text",
        WbElement::Shape(_) => "shape",
        WbElement::Arrow(_) => "arrow",
        WbElement::Frame(_) => "frame",
        WbElement::Image(_) => "image",
    }
}

#[test]
fn reads_the_live_board_snapshot_and_round_trips_it() {
    // The server's actual bytes decode to the four elements that were synced onto the board.
    let doc = file::read_snapshot(1, GOLDEN).expect("decode the live .kbwbd snapshot");
    let scene = scene_from_doc(&doc, "dots");
    assert_eq!(scene.elements.len(), 4, "the live board has four elements");
    assert_eq!(scene.strokes.len(), 0);

    let mut kinds: Vec<&str> = scene.elements.iter().map(kind).collect();
    kinds.sort_unstable();
    assert_eq!(kinds, vec!["arrow", "shape", "sticky", "text"]);

    // A sticky with the text written over collaboration, proving the content (not just the count) decoded.
    let sticky = scene.elements.iter().find(|e| matches!(e, WbElement::Sticky(_))).expect("a sticky");
    if let WbElement::Sticky(s) = sticky {
        assert!(s.text.contains("desktop"), "sticky text: {:?}", s.text);
        assert_eq!(s.base.id, "wb-agent-sticky");
    }

    // Write the snapshot back and re-read: the scene must be identical (the Yjs v1 + gzip round trip is
    // loss-free), even though the gzip bytes themselves need not be identical.
    let rewritten = file::write_snapshot(&doc).expect("write snapshot");
    let doc2 = file::read_snapshot(2, &rewritten).expect("re-read");
    let scene2 = scene_from_doc(&doc2, "dots");
    assert_eq!(doc.elements(), doc2.elements(), "elements round-trip byte-for-byte as JSON");
    assert_eq!(scene.elements.len(), scene2.elements.len());
}
