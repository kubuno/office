//! Tests over the views themselves: every `.kbview` / `.kbcontrol` of the app compiles with the runtime the
//! window uses (an unknown attribute only shows at run time otherwise), and every `{Res key}` they name
//! exists in both languages.

use kubuno_desktop::views::ast::{AstNode, Document, Element};
use kubuno_desktop::views::syntax::parse;

const VIEWS: [(&str, &str); 5] = [
    ("spreadsheet_window.kbview", include_str!("spreadsheet_window.kbview")),
    ("confirm_dialog.kbview", include_str!("confirm_dialog.kbview")),
    ("prompt_dialog.kbview", include_str!("prompt_dialog.kbview")),
    ("print_dialog.kbview", include_str!("print_dialog.kbview")),
    ("backstage_info.kbcontrol", include_str!("../pages/backstage_info.kbcontrol")),
];
const RES_EN: &str = include_str!("../resources/resources.kbres");
const RES_FR: &str = include_str!("../resources/resources.fr.kbres");

fn elements(text: &str) -> Vec<Element> {
    let parsed = parse(text);
    let root = Document::cast(parsed.syntax()).and_then(|d| d.root_element()).expect("a root element");
    root.syntax().descendants().filter_map(Element::cast).collect()
}

fn resource_keys(text: &str) -> Vec<String> {
    elements(text).iter().filter_map(|e| e.attribute("Name").and_then(|a| a.value())).collect()
}

#[test]
fn every_view_compiles() {
    // The app's own controls register when the library is linked (this test binary).
    let _ = (crate::GridCanvas::default(), crate::SheetTabs::default(), crate::StatusZoom::default(), crate::BackstageInfo::default());
    for (name, text) in VIEWS {
        let mut rt = kubuno_desktop::views::runtime::Runtime::new();
        let ok = rt.reload_from_text(text);
        assert!(ok, "{name}: {:#?}", rt.diagnostics());
    }
}

#[test]
fn every_resource_named_by_a_view_exists_in_both_languages() {
    let (en, fr) = (resource_keys(RES_EN), resource_keys(RES_FR));
    for (name, text) in VIEWS {
        let mut rest = text;
        while let Some(i) = rest.find("{Res ") {
            let tail = &rest[i + 5..];
            let end = tail.find('}').unwrap_or(tail.len());
            let key = tail[..end].trim();
            assert!(en.iter().any(|k| k == key), "{name}: `{key}` is missing from resources.kbres");
            assert!(key == "app_icon" || fr.iter().any(|k| k == key), "{name}: `{key}` is missing from resources.fr.kbres");
            rest = &tail[end..];
        }
    }
}
