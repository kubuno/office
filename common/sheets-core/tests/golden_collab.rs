//! Interop of the collaboration module with the web's libraries (yjs, y-protocols).
//!
//! `tests/golden/collab_from_rust.json` is what this crate writes (regenerate with
//! `cargo test --test golden_collab -- --ignored`); `tests/golden/collab_web.json` is produced on the Linux
//! host by `tools/golden/collab.mjs` with the web's own `node_modules`: what the web writes, and what the web
//! reads back from our file.

use kubuno_office_sheets_core::collab::awareness::{self, base64_decode, base64_encode, Awareness};
use kubuno_office_sheets_core::collab::SheetsDoc;
use serde_json::{json, Map, Value as Json};

fn golden(name: &str) -> String {
    format!("{}/tests/golden/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn obj(v: Json) -> Map<String, Json> {
    v.as_object().cloned().unwrap_or_default()
}

fn rust_cells() -> Map<String, Json> {
    obj(json!({
        "A1": {"v": 7, "s": {"bold": true, "color": "#d93025"}},
        "A2": {"f": "=SUM(A1:A1)*TAUX"},
        "B5": {"v": "Désolé ✓ — 😀"},
        "C9": {"v": 1.5e-300},
        "D1": {"v": -12.25, "c": "a note"},
    }))
}

fn rust_names() -> Map<String, Json> {
    obj(json!({"TAUX": "=0.2", "PLAGE": "='Feuille 1'!$A$1:$B$3"}))
}

fn rust_presence() -> Json {
    json!({"user": {"id": "u-desk", "name": "Desk Ana", "color": "#1e8e3e", "avatar": null}, "sel": {"col": "AB", "row": 12, "sheet": "s1"}})
}

#[test]
#[ignore = "writes the vectors the web reads (tools/golden/collab.mjs)"]
fn writes_vectors_for_the_web() {
    let d = SheetsDoc::new(5151);
    d.write_cells("s1", &rust_cells());
    d.write_names(&rust_names());
    let mut a = Awareness::new(5151, 0);
    a.set_local_state(Some(rust_presence()), 1);
    let out = json!({"update": base64_encode(&d.state_as_update()), "awareness": base64_encode(&a.encode_local())});
    std::fs::write(golden("collab_from_rust.json"), serde_json::to_string_pretty(&out).expect("json")).expect("writes");
}

#[test]
fn the_web_and_this_crate_read_each_other() {
    let text = std::fs::read_to_string(golden("collab_web.json")).expect("the web vectors");
    let web: Json = serde_json::from_str(&text).expect("json");
    let b = |k: &str| base64_decode(web[k].as_str().expect("a string")).expect("base64");

    // What the web wrote, applied in the order the room sends it.
    let d = SheetsDoc::new(1);
    d.apply_update(&b("update")).expect("the first update");
    assert_eq!(d.cells("s1"), obj(web["first_cells"].clone()));
    assert_eq!(d.names(), obj(web["names"].clone()));
    d.apply_update(&b("incremental")).expect("the incremental update");
    assert_eq!(d.cells("s1"), obj(web["final_cells"].clone()));
    // The room echoes: applying again changes nothing.
    d.apply_update(&b("update")).expect("again");
    assert_eq!(d.cells("s1"), obj(web["final_cells"].clone()));

    let entries = awareness::decode(&b("awareness")).expect("the web's awareness");
    assert_eq!(entries.len(), 1);
    assert_eq!(Some(entries[0].client), web["awareness_client"].as_u64());
    assert_eq!(Some(entries[0].clock), web["awareness_clock"].as_u64());
    assert_eq!(entries[0].state.as_ref(), Some(&web["awareness_state"]));
    let mut a = Awareness::new(2, 0);
    a.apply(&b("awareness"), 0).expect("applies");
    let peers = a.peers();
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].name, "Web Zoé");
    assert_eq!(peers[0].sel, Some(("C".into(), 4, Some("s1".into()))));
    assert_eq!(peers[0].cursor, Some((120.5, 48.0)));

    // What the web read from our file.
    let read = &web["rust_read"];
    assert_eq!(obj(read["cells"].clone()), rust_cells());
    assert_eq!(obj(read["names"].clone()), rust_names());
    assert_eq!(read["awareness"]["5151"], rust_presence());
}
