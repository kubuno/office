//! The element menu's one-click actions (`elementMenuSections`), by key: the window's menu items name them
//! (`flt_gray`, `qs_3`, `lw_4`…) and run them on the selection through [`element_action`]. Kept here, away
//! from the window, so each mirrors its web helper (`setImgFilters`, `applyShapeStyle`, `setLineWidth`…) under
//! test.

use kubuno_office_slides_core::edit::Outcome;
use kubuno_office_slides_core::model::Element;
use serde_json::{json, Value};

use super::workspace::Workspace;

/// `PRES_ANIMATIONS`: the menu key (lower case) and the stored type.
pub const ANIMATIONS: [(&str, &str); 20] = [
    ("none", "none"),
    ("fade", "fade"),
    ("flyl", "flyL"),
    ("flyr", "flyR"),
    ("flyt", "flyT"),
    ("flyb", "flyB"),
    ("zoom", "zoom"),
    ("rise", "rise"),
    ("spin", "spin"),
    ("flip", "flip"),
    ("swivel", "swivel"),
    ("growturn", "growturn"),
    ("expand", "expand"),
    ("pulse", "pulse"),
    ("bounce", "bounce"),
    ("dropin", "dropin"),
    ("wiper", "wiper"),
    ("wipeu", "wipeu"),
    ("flytl", "flyTL"),
    ("flybr", "flyBR"),
];

/// `SHAPE_PRESETS` (« Style rapide »): fill and stroke.
fn shape_preset(i: usize) -> Option<(Value, Value)> {
    let solid = |fill: &str, stroke: &str, width: i64, style: &str| (json!({ "type": "color", "color": fill }), json!({ "color": stroke, "width": width, "style": style }));
    Some(match i {
        0 => solid("#1a73e8", "#1557b0", 0, "solid"),
        1 => solid("#ffffff", "#1a73e8", 2, "solid"),
        2 => solid("#34a853", "#0f9d58", 0, "solid"),
        3 => solid("#ea4335", "#c5221f", 0, "solid"),
        4 => solid("#fbbc04", "#f29900", 0, "solid"),
        5 => solid("#f1f3f4", "#9aa0a6", 1, "solid"),
        6 => (
            json!({ "type": "gradient", "grad": { "type": "linear", "angle": 90, "stops": [{ "color": "#4f9cff", "position": 0 }, { "color": "#1a56c4", "position": 1 }] } }),
            json!({ "color": "#1557b0", "width": 0, "style": "solid" }),
        ),
        7 => solid("#ffffff", "#5f6368", 2, "dashed"),
        _ => return None,
    })
}

/// The quick styles' labels, in order (the menu's items `qs_0`…`qs_7`).
pub const SHAPE_PRESET_LABELS: [&str; 8] = ["Bleu plein", "Contour bleu", "Vert plein", "Rouge plein", "Jaune plein", "Gris clair", "Dégradé bleu", "Pointillé"];

fn num(s: &str) -> Option<f64> {
    s.parse::<f64>().ok()
}

/// A patch applied to every selected element of `kind` (`None`: any kind); `None` from `f` leaves it.
fn patch_of(ws: &mut Workspace, now_ms: u64, kind: Option<&str>, f: impl Fn(&Element) -> Option<Element>) -> bool {
    let kind = kind.map(str::to_string);
    ws.edit(now_ms, |els, ed, _| {
        ed.update_sel(els, |e| {
            if kind.as_deref().is_some_and(|k| k != e.kind()) {
                return e.clone();
            }
            f(e).unwrap_or_else(|| e.clone())
        })
    })
    .changed
}

fn set(e: &Element, key: &str, v: Option<Value>) -> Element {
    let mut n = e.clone();
    match v {
        Some(v) => n.set(key, v),
        None => n.remove(key),
    }
    n
}

/// Runs the element menu's action `key` on the selection. Returns whether it was one.
pub fn element_action(ws: &mut Workspace, now_ms: u64, key: &str) -> bool {
    let (head, tail) = key.split_once('_').unwrap_or((key, ""));
    match head {
        "anim" => {
            let Some((_, kind)) = ANIMATIONS.iter().find(|(k, _)| *k == tail) else { return false };
            let kind = (*kind != "none").then_some(*kind);
            ws.edit(now_ms, |els, ed, _| ed.set_anim(els, kind));
        }
        "qs" => {
            let Some((fill, stroke)) = num(tail).and_then(|i| shape_preset(i as usize)) else { return false };
            patch_of(ws, now_ms, Some("shape"), |e| {
                let mut n = e.clone();
                n.set("fill", fill.clone());
                n.set("stroke", stroke.clone());
                Some(n)
            });
        }
        "cr" => {
            let Some(r) = num(tail) else { return false };
            patch_of(ws, now_ms, Some("shape"), |e| {
                let mut n = set(e, "cornerRadius", Some(json!(r as i64)));
                n.remove("adj");
                Some(n)
            });
        }
        "flt" => {
            let filters = match tail {
                "none" => None,
                "gray" => Some(json!({ "grayscale": 1 })),
                "sepia" => Some(json!({ "sepia": 0.8 })),
                "bright" => Some(json!({ "brightness": 1.3 })),
                "dark" => Some(json!({ "brightness": 0.7 })),
                "contrast" => Some(json!({ "contrast": 1.4 })),
                "saturate" => Some(json!({ "saturate": 1.7 })),
                "blur" => Some(json!({ "blur": 2 })),
                "cold" => Some(json!({ "grayscale": 0.6, "brightness": 1.05 })),
                _ => return false,
            };
            patch_of(ws, now_ms, Some("image"), |e| Some(set(e, "filters", filters.clone())));
        }
        "bd" => {
            let border = match tail {
                "none" => None,
                "thin" => Some(json!({ "color": "#202124", "width": 2 })),
                "white" => Some(json!({ "color": "#ffffff", "width": 8 })),
                "blue" => Some(json!({ "color": "#1a73e8", "width": 4 })),
                _ => return false,
            };
            patch_of(ws, now_ms, Some("image"), |e| Some(set(e, "border", border.clone())));
        }
        "ir" => {
            let Some(r) = num(tail) else { return false };
            patch_of(ws, now_ms, Some("image"), |e| Some(set(e, "cornerRadius", Some(json!(r as i64)))));
        }
        "tint" => {
            let tint = match tail {
                "none" => None,
                "blue" => Some("#9bc0ff"),
                "warm" => Some("#ffd9a0"),
                "green" => Some("#a8e6a3"),
                _ => return false,
            };
            patch_of(ws, now_ms, Some("image"), |e| Some(set(e, "tint", tint.map(|t| json!(t)))));
        }
        "fx" => {
            let ok = matches!(tail, "shadow" | "outline" | "wa_blue" | "wa_gold" | "wa_none");
            if !ok {
                return false;
            }
            let tail = tail.to_string();
            patch_of(ws, now_ms, Some("text"), move |e| {
                Some(match tail.as_str() {
                    "shadow" => set(e, "textShadow", (!e.truthy("textShadow")).then_some(Value::Bool(true))),
                    "outline" => set(e, "textOutline", (!e.truthy("textOutline")).then(|| json!({ "color": "#202124", "width": 1 }))),
                    "wa_blue" => set(e, "wordArt", Some(json!({ "from": "#4f9cff", "to": "#1a56c4" }))),
                    "wa_gold" => set(e, "wordArt", Some(json!({ "from": "#ffd86b", "to": "#c98a00" }))),
                    _ => set(e, "wordArt", None),
                })
            });
        }
        "case" => {
            let v = match tail {
                "none" => None,
                "upper" => Some("upper"),
                "lower" => Some("lower"),
                "capitalize" => Some("capitalize"),
                _ => return false,
            };
            patch_of(ws, now_ms, Some("text"), |e| Some(set(e, "textTransform", v.map(|v| json!(v)))));
        }
        "ls" => {
            let Some(s) = num(tail) else { return false };
            patch_of(ws, now_ms, Some("text"), |e| Some(set(e, "letterSpacing", Some(json!(s as i64)))));
        }
        "col" => {
            let Some(c) = num(tail) else { return false };
            patch_of(ws, now_ms, Some("text"), |e| Some(set(e, "columns", Some(json!(c as i64)))));
        }
        "as" => {
            let Some(n) = num(tail) else { return false };
            patch_of(ws, now_ms, Some("line"), |e| Some(set(e, "arrowSize", Some(json!(n as i64)))));
        }
        "lw" | "dash" => {
            let (field, v) = if head == "lw" {
                let Some(n) = num(tail) else { return false };
                ("width", json!(n as i64))
            } else {
                if !matches!(tail, "solid" | "dashed" | "dotted") {
                    return false;
                }
                ("style", json!(tail))
            };
            patch_of(ws, now_ms, Some("line"), |e| {
                let mut stroke = e.get("stroke").and_then(Value::as_object).cloned().unwrap_or_default();
                stroke.insert(field.to_string(), v.clone());
                Some(set(e, "stroke", Some(Value::Object(stroke))))
            });
        }
        "arrow" => {
            let key = match tail {
                "end" => "arrowEnd",
                "start" => "arrowStart",
                _ => return false,
            };
            patch_of(ws, now_ms, Some("line"), |e| Some(set(e, key, Some(if e.truthy(key) { Value::Null } else { json!("triangle") }))));
        }
        "op" => {
            let Some(p) = num(tail) else { return false };
            ws.edit(now_ms, |els, ed, _| ed.set_opacity(els, p / 100.0));
        }
        "shape" => {
            // « Modifier la forme »: `shape_<kind>`.
            if tail.is_empty() {
                return false;
            }
            let kind = tail.to_string();
            patch_of(ws, now_ms, Some("shape"), |e| Some(set(e, "shape", Some(json!(kind)))));
        }
        _ => return false,
    }
    true
}

/// The selected line's arrow toggles' current state (end, start).
pub fn arrows_of(ws: &Workspace) -> (bool, bool) {
    let sel = &ws.editor.selection;
    ws.active_elements()
        .iter()
        .find(|e| sel.first().is_some_and(|s| s == e.id()) && e.kind() == "line")
        .map(|e| (e.truthy("arrowEnd"), e.truthy("arrowStart")))
        .unwrap_or_default()
}

/// Whether `o` changed the elements (re-exported for the window's own edits).
pub fn changed(o: Outcome) -> bool {
    o.changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use kubuno_office_slides_core::model::Deck;

    fn ws_with(el: &str) -> Workspace {
        let bundle = format!(r##"{{"version":1,"slides":{{"s1":{{"elements":[{el}],"background":{{"type":"color","color":"#ffffff"}},"notes":"","transition":{{"type":"none","duration":0.3}}}}}}}}"##);
        let deck = Deck::from_json(bundle.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
        let mut ws = Workspace::new(deck, 1);
        ws.editor.selection = ws.active_elements().iter().map(|e| e.id().to_string()).collect();
        ws
    }

    fn first(ws: &Workspace) -> Element {
        ws.active_elements().first().cloned().unwrap_or_else(|| panic!("no element"))
    }

    #[test]
    fn image_filters_border_tint() {
        let mut ws = ws_with(r#"{"id":"i","type":"image","x":0,"y":0,"w":0.5,"h":0.5,"src":"x"}"#);
        assert!(element_action(&mut ws, 0, "flt_sepia"));
        assert_eq!(first(&ws).get("filters"), Some(&json!({ "sepia": 0.8 })));
        element_action(&mut ws, 2000, "flt_none");
        assert_eq!(first(&ws).get("filters"), None);
        element_action(&mut ws, 4000, "bd_white");
        assert_eq!(first(&ws).get("border"), Some(&json!({ "color": "#ffffff", "width": 8 })));
        element_action(&mut ws, 6000, "tint_warm");
        assert_eq!(first(&ws).s("tint"), Some("#ffd9a0"));
    }

    #[test]
    fn line_stroke_and_arrows() {
        let mut ws = ws_with(r##"{"id":"l","type":"line","x":0,"y":0,"w":0.5,"h":0,"stroke":{"color":"#000000","width":2,"style":"solid"}}"##);
        element_action(&mut ws, 0, "lw_6");
        element_action(&mut ws, 2000, "dash_dotted");
        assert_eq!(first(&ws).get("stroke"), Some(&json!({ "color": "#000000", "width": 6, "style": "dotted" })));
        element_action(&mut ws, 4000, "arrow_end");
        assert_eq!(arrows_of(&ws), (true, false));
        element_action(&mut ws, 6000, "arrow_end");
        assert_eq!(first(&ws).get("arrowEnd"), Some(&Value::Null));
        assert_eq!(arrows_of(&ws), (false, false));
    }

    #[test]
    fn shape_style_corner_and_kind() {
        let mut ws = ws_with(r#"{"id":"s","type":"shape","shape":"roundRect","x":0,"y":0,"w":0.5,"h":0.5,"adj":{"adj":5000}}"#);
        element_action(&mut ws, 0, "cr_24");
        let e = first(&ws);
        assert_eq!(e.f("cornerRadius"), Some(24.0));
        assert_eq!(e.get("adj"), None);
        element_action(&mut ws, 2000, "qs_7");
        assert_eq!(first(&ws).get("stroke"), Some(&json!({ "color": "#5f6368", "width": 2, "style": "dashed" })));
        element_action(&mut ws, 4000, "shape_ellipse");
        assert_eq!(first(&ws).s("shape"), Some("ellipse"));
        // Not a text: the text effects leave it.
        element_action(&mut ws, 6000, "fx_shadow");
        assert!(!first(&ws).truthy("textShadow"));
        assert!(!element_action(&mut ws, 8000, "nope_1"));
    }

    #[test]
    fn text_effects_toggle() {
        let mut ws = ws_with(r#"{"id":"t","type":"text","x":0,"y":0,"w":0.5,"h":0.2}"#);
        element_action(&mut ws, 0, "fx_shadow");
        assert!(first(&ws).truthy("textShadow"));
        element_action(&mut ws, 2000, "fx_shadow");
        assert_eq!(first(&ws).get("textShadow"), None);
        element_action(&mut ws, 4000, "case_upper");
        assert_eq!(first(&ws).s("textTransform"), Some("upper"));
        element_action(&mut ws, 6000, "anim_flyl");
        assert_eq!(first(&ws).get("anim").and_then(|a| a.get("type")), Some(&json!("flyL")));
    }
}
