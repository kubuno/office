//! Tests over the views themselves and their resources: every view compiles (the runtime reports what the
//! macros do not, such as an attribute an element does not have), every icon they name is embedded, no ribbon
//! command is a blank box, the tabs follow the web's order, and every `{Res}` exists in both languages.

use kubuno_desktop::views::ast::{AstNode, Document, Element};
use kubuno_desktop::views::syntax::parse;

const WINDOW: &str = include_str!("presentation_window.kbview");
const VIEWS: [(&str, &str); 9] = [
    ("presentation_window", include_str!("presentation_window.kbview")),
    ("slideshow_window", include_str!("slideshow_window.kbview")),
    ("background_dialog", include_str!("background_dialog.kbview")),
    ("chart_dialog", include_str!("chart_dialog.kbview")),
    ("geometry_dialog", include_str!("geometry_dialog.kbview")),
    ("prompt_dialog", include_str!("prompt_dialog.kbview")),
    ("share_dialog", include_str!("share_dialog.kbview")),
    ("confirm_dialog", include_str!("confirm_dialog.kbview")),
    ("presentation_info", include_str!("../pages/presentation_info.kbcontrol")),
];
const RES_EN: &str = include_str!("../resources/resources.kbres");
const RES_FR: &str = include_str!("../resources/resources.fr.kbres");

fn elements(text: &str) -> Vec<Element> {
    let parsed = parse(text);
    let Some(root) = Document::cast(parsed.syntax()).and_then(|d| d.root_element()) else { panic!("a root element") };
    root.syntax().descendants().filter_map(Element::cast).collect()
}

fn attr(e: &Element, name: &str) -> Option<String> {
    e.attribute(name).and_then(|a| a.value())
}

fn commands() -> Vec<Element> {
    elements(WINDOW).into_iter().filter(|e| e.name().as_deref() == Some("Command")).collect()
}

fn resource_keys(text: &str) -> Vec<String> {
    elements(text).iter().filter_map(|e| attr(e, "Name")).collect()
}

#[test]
fn every_view_compiles() {
    // The custom controls register when the library is linked (this test binary).
    let _ = (crate::SlideCanvas::default(), crate::SlidePanel::default(), crate::SlideShow::default(), crate::PresentationInfo::default());
    for (name, text) in VIEWS.iter().filter(|(n, _)| *n != "presentation_info") {
        let mut rt = kubuno_desktop::views::runtime::Runtime::new();
        let ok = rt.reload_from_text(text);
        assert!(ok, "{name}: {:#?}", rt.diagnostics());
    }
}

#[test]
fn every_declared_icon_exists_in_the_asset_files() {
    let names: Vec<String> = kubuno_drive_desktop_app_controls::themed_icon::catalog().into_iter().map(|e| e.name.to_string()).collect();
    assert!(names.len() > 100, "the icon catalogue has only {} names", names.len());
    for (view, text) in VIEWS {
        for e in elements(text) {
            for a in ["SmallIcon", "LargeIcon", "Icon"] {
                if let Some(icon) = attr(&e, a).filter(|i| !i.starts_with('{')) {
                    assert!(names.contains(&icon), "{view}: `{}` declares the icon `{icon}`, which is not embedded", e.name().unwrap_or_default());
                }
            }
        }
    }
}

#[test]
fn no_ribbon_command_would_draw_a_blank_box() {
    let cmds = commands();
    for e in elements(WINDOW) {
        let name = e.name().unwrap_or_default();
        if !["RibbonButton", "RibbonToggleButton", "RibbonSplitButton", "RibbonMenuButton", "RibbonMenuItem", "RibbonGallery"].contains(&name.as_str()) {
            continue;
        }
        let command = attr(&e, "Command").and_then(|c| cmds.iter().find(|k| attr(k, "x:Name").as_deref() == Some(c.as_str())).cloned());
        let label = attr(&e, "Label").or_else(|| command.as_ref().and_then(|c| attr(c, "Label")));
        assert!(label.as_deref().is_some_and(|l| !l.is_empty()), "a `{name}` has no label ({:?})", attr(&e, "Command"));
        let icon = attr(&e, "SmallIcon").or_else(|| command.as_ref().and_then(|c| attr(c, "SmallIcon")));
        if name != "RibbonMenuItem" {
            assert!(icon.is_some(), "a `{name}` ({label:?}) has no icon");
        }
    }
}

#[test]
fn tabs_follow_the_web_order() {
    let tabs: Vec<String> = elements(WINDOW)
        .iter()
        .filter(|e| matches!(e.name().as_deref(), Some("RibbonTab") | Some("RibbonBackstage")))
        .filter_map(|e| attr(e, "x:Name"))
        .collect();
    // `presRibbon`: Fichier, Accueil, Insertion (spliced after Accueil), Disposition, Animations, Transitions,
    // then the contextual « Forme ».
    assert_eq!(tabs, ["backstage", "tab_home", "tab_insert", "tab_arrange", "tab_anim", "tab_trans", "tab_view", "tab_shape"]);
}

#[test]
fn clipboard_is_the_first_group_of_home() {
    let all = elements(WINDOW);
    let Some(home) = all.iter().find(|e| attr(e, "x:Name").as_deref() == Some("tab_home")) else { panic!("an Accueil tab") };
    let Some(first) = home.syntax().children().filter_map(Element::cast).next() else { panic!("a group") };
    assert_eq!(attr(&first, "x:Name").as_deref(), Some("grp_clip"));
}

#[test]
fn every_resource_the_views_name_exists_in_both_languages() {
    let (en, fr) = (resource_keys(RES_EN), resource_keys(RES_FR));
    for (view, text) in VIEWS {
        for part in text.split("{Res ").skip(1) {
            let key = part.split('}').next().unwrap_or_default().trim();
            assert!(en.iter().any(|k| k == key), "{view}: `{key}` is missing from resources.kbres");
            assert!(key == "app_icon" || fr.iter().any(|k| k == key), "{view}: `{key}` is missing from resources.fr.kbres");
        }
    }
    let mut seen = std::collections::HashSet::new();
    for k in &en {
        assert!(seen.insert(k), "`{k}` is defined twice");
    }
    for k in &en {
        assert!(k == "app_icon" || fr.contains(k), "`{k}` has no French text");
    }
}
