//! Tests over the views themselves (`diagram_window.kbview`, its dialogs and user controls) and their
//! resources: the views compile, every icon they name is embedded, every `{Res}` exists in both languages,
//! the ribbon's tabs follow the web's order.

use kubuno_desktop::views::ast::{AstNode, Document, Element};
use kubuno_desktop::views::syntax::parse;

const WINDOW: &str = include_str!("diagram_window.kbview");
const VIEWS: &[(&str, &str)] = &[
    ("diagram_window", include_str!("diagram_window.kbview")),
    ("confirm_dialog", include_str!("confirm_dialog.kbview")),
    ("prompt_dialog", include_str!("prompt_dialog.kbview")),
    ("templates_dialog", include_str!("templates_dialog.kbview")),
    ("style_dialog", include_str!("style_dialog.kbview")),
    ("backstage_info", include_str!("../pages/backstage_info.kbcontrol")),
    ("format_panel", include_str!("../pages/format_panel.kbcontrol")),
];
const RES_EN: &str = include_str!("../resources/resources.kbres");
const RES_FR: &str = include_str!("../resources/resources.fr.kbres");

fn elements(text: &str) -> Vec<Element> {
    let parsed = parse(text);
    let root = Document::cast(parsed.syntax()).and_then(|d| d.root_element()).expect("a root element");
    root.syntax().descendants().filter_map(Element::cast).collect()
}

fn attr(e: &Element, name: &str) -> Option<String> {
    e.attribute(name).and_then(|a| a.value())
}

fn resource_keys(text: &str) -> Vec<String> {
    elements(text).iter().filter_map(|e| attr(e, "Name")).collect()
}

#[test]
fn the_window_view_compiles() {
    // The view's own controls register when the library is linked (this test binary).
    let _ = (crate::DiagramCanvas::default(), crate::StencilPalette::default(), crate::LayerList::default(), crate::PageTabs::default(), crate::StatusZoom::default(), crate::StartPage::default());
    let mut rt = kubuno_desktop::views::runtime::Runtime::new();
    let ok = rt.reload_from_text(WINDOW);
    assert!(ok, "{:#?}", rt.diagnostics());
}

#[test]
fn the_format_panel_and_the_dialogs_compile() {
    let _ = (crate::pages::format_panel::StylePresets::default(), crate::pages::format_panel::SegmentBar::default(), crate::pages::format_panel::PanelTabs::default(), super::templates_dialog::TemplateGrid::default());
    for (name, text) in VIEWS.iter().filter(|(n, _)| matches!(*n, "format_panel" | "style_dialog" | "templates_dialog")) {
        let mut rt = kubuno_desktop::views::runtime::Runtime::new();
        assert!(rt.reload_from_text(text), "{name}: {:#?}", rt.diagnostics());
    }
}

#[test]
fn every_declared_icon_exists_in_the_asset_files() {
    let names: Vec<String> = kubuno_drive_desktop_app_controls::themed_icon::catalog().into_iter().map(|e| e.name.to_string()).collect();
    for (view, text) in VIEWS {
        for e in elements(text) {
            for a in ["SmallIcon", "LargeIcon", "Icon"] {
                if let Some(icon) = attr(&e, a).filter(|i| !i.starts_with('{') && !i.is_empty()) {
                    assert!(names.contains(&icon), "{view}: `{}` declares the icon `{icon}`, which is not embedded", e.name().unwrap_or_default());
                }
            }
        }
    }
}

#[test]
fn every_resource_the_views_name_exists_in_both_languages() {
    let (en, fr) = (resource_keys(RES_EN), resource_keys(RES_FR));
    for (view, text) in VIEWS {
        let mut rest = *text;
        while let Some(i) = rest.find("{Res ") {
            let after = &rest[i + 5..];
            let key: String = after.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
            assert!(en.contains(&key), "{view}: `{key}` is missing from resources.kbres");
            assert!(fr.contains(&key) || key == "app_icon", "{view}: `{key}` is missing from resources.fr.kbres");
            rest = after;
        }
    }
}

#[test]
fn the_tabs_follow_the_web() {
    let tabs: Vec<String> = elements(WINDOW).iter().filter(|e| e.name().as_deref() == Some("RibbonTab")).filter_map(|e| attr(e, "x:Name")).collect();
    assert_eq!(tabs, ["tab_home", "tab_insert", "tab_arrange", "tab_view", "tab_conn", "tab_shape"]);
}
