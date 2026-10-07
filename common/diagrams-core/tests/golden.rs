//! Golden fixtures written by the WEB editor (vskubuno docs/DIAGRAMS-DESKTOP.md §7): a two-page diagram made
//! through the web Diagrams editor's real input on a development core (a template, drawn stencils — an office
//! star, a hardware icon, a swimlane —, a connector drawn port to shape, a colour theme, a second layer with
//! a cloud on it, a flip; on the second page a template, a curved connector, a rotation, a group), then read
//! back from the server: `golden-1.kbdia` is the stored Drive file, `golden-1.export.json` the « Export JSON ».

use kubuno_office_diagrams_core::canvas::{Ctx2D, Op, Recorder};
use kubuno_office_diagrams_core::editor::Editor;
use kubuno_office_diagrams_core::file::{DiagramFile, Page};
use kubuno_office_diagrams_core::model::PageData;
use serde_json::Value;

const KBDIA: &[u8] = include_bytes!("fixtures/golden-1.kbdia");
const EXPORT: &str = include_str!("fixtures/golden-1.export.json");

fn export_pages() -> Vec<(String, String, Value)> {
    let v: Value = serde_json::from_str(EXPORT).expect("export JSON");
    v["pages"]
        .as_array()
        .expect("pages")
        .iter()
        .map(|p| (p["id"].as_str().unwrap_or("").to_string(), p["name"].as_str().unwrap_or("").to_string(), p["data"].clone()))
        .collect()
}

#[test]
fn every_page_the_web_wrote_round_trips_unchanged() {
    for (_, name, data) in export_pages() {
        assert_eq!(PageData::from_value(&data).to_value(), data, "page {name}");
    }
    let file = DiagramFile::read(KBDIA).expect("the stored .kbdia reads");
    assert_eq!(file.pages.len(), 2);
    let raw: Value = serde_json::from_slice(&kubuno_office_diagrams_core::file::gunzip(KBDIA).expect("gzip")).expect("json");
    assert_eq!(file.to_value(), raw, "the whole file round-trips");
    assert_eq!(DiagramFile::read(&file.to_bytes()).expect("own bytes"), file);
}

#[test]
fn the_file_and_the_export_hold_the_same_pages() {
    let mut file = DiagramFile::read(KBDIA).expect("kbdia");
    let ids: Vec<String> = export_pages().iter().map(|(id, _, _)| id.clone()).collect();
    file.order_by(&ids);
    for ((id, d), (eid, _, edata)) in file.pages.iter().zip(export_pages()) {
        assert_eq!(id, &eid);
        assert_eq!(d.to_value(), edata);
    }
}

fn editor() -> Editor {
    let pages = export_pages().into_iter().map(|(id, name, data)| Page::new(&id, &name, PageData::from_value(&data))).collect();
    let mut e = Editor::new(pages, 1);
    e.viewport = (747.0, 660.0);
    e
}

#[test]
fn what_the_web_did_is_read_back() {
    let mut e = editor();
    let p1 = e.data();
    assert_eq!(p1.layers().len(), 2, "the second layer");
    let star = p1.shapes.iter().find(|s| s.kind() == "star4").expect("the office star");
    assert!(star.flip_h());
    let cloud = p1.shapes.iter().find(|s| s.kind() == "cloud").expect("the cloud");
    assert_eq!(cloud.layer_id(), Some(p1.layers()[1].id()));
    assert_eq!(p1.shapes.iter().filter(|s| s.style().fill_color == "#d5e8d4").count(), 8, "the green theme on the 8 shapes drawn before it");
    e.set_current_page(1);
    let p2 = e.data();
    assert_eq!(p2.shapes.iter().find(|s| s.kind() == "net_internet").map(|s| s.rotation()), Some(30.0));
    let grouped: Vec<_> = p2.shapes.iter().filter(|s| s.group_id().is_some()).collect();
    assert_eq!(grouped.len(), 2);
    assert_eq!(grouped[0].group_id(), grouped[1].group_id());
    assert_eq!(p2.connectors[0].style().routing, kubuno_office_diagrams_core::model::Routing::Curved);
}

#[test]
fn every_page_paints() {
    let mut e = editor();
    for i in 0..e.pages().len() {
        e.set_current_page(i);
        let mut r = Recorder::default();
        let mut ctx = Ctx2D::new(&mut r);
        e.render(&mut ctx);
        let texts: Vec<String> = r.ops.iter().filter_map(|o| if let Op::Text { text, .. } = o { Some(text.clone()) } else { None }).collect();
        assert!(r.ops.len() > 30, "page {i}: {} ops", r.ops.len());
        if i == 0 {
            assert!(texts.iter().any(|t| t == "Début") && texts.iter().any(|t| t == "oui"), "{texts:?}");
        }
    }
}

#[test]
fn an_edit_changes_only_what_it_touches() {
    let mut e = editor();
    let before = e.data().to_value();
    let id = e.data().shapes[0].id().to_string();
    e.set_selection(vec![id.clone()], vec![]);
    e.set_shape_geometry(&id, "x", 210.0);
    let after = e.data().to_value();
    let mut expected = before.clone();
    expected["shapes"][0]["x"] = Value::from(210);
    assert_eq!(after, expected);
    e.undo();
    assert_eq!(e.data().to_value(), before);
}
