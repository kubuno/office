//! The text toolbar's formats (`applyTextFormat`, `applyParaAttr`): paragraph attributes applied to every
//! paragraph of the selected (or edited) text box, run marks over the edited selection, and the element-level
//! fallback (the whole box) when nothing is selected inside the text.

use serde_json::Value;

use crate::model::Element;
use crate::richtext::{doc_to_paras, Para};
use crate::textedit::Mark;

/// The kinds `applyParaAttr` handles (`paraAttrKinds`).
pub const PARA_KINDS: [&str; 8] = ["bullet", "number", "justify", "align", "indentIn", "indentOut", "lineHeight", "clearList"];

pub fn is_para_kind(kind: &str) -> bool {
    PARA_KINDS.contains(&kind)
}

/// `applyParaAttr`'s mapping of every paragraph.
pub fn apply_para_attr(paras: &[Para], kind: &str, value: Option<&str>) -> Vec<Para> {
    paras
        .iter()
        .map(|p| {
            let mut p = p.clone();
            let a = &mut p.attrs;
            let s = |v: &str| Some(Value::String(v.to_string()));
            match kind {
                "bullet" => {
                    let v = if a.list() == Some("bullet") { None } else { s("bullet") };
                    a.set("list", v)
                }
                "number" => {
                    let v = if a.list() == Some("number") { None } else { s("number") };
                    a.set("list", v)
                }
                "clearList" => a.set("list", None),
                "justify" => {
                    let v = if a.align() == Some("justify") { None } else { s("justify") };
                    a.set("align", v)
                }
                "align" => a.set("align", value.filter(|v| !v.is_empty()).and_then(s)),
                "indentIn" => {
                    let v = (a.indent() + 24.0).min(240.0);
                    a.set("indent", Some(crate::model::num(v)))
                }
                "indentOut" => {
                    let v = (a.indent() - 24.0).max(0.0);
                    a.set("indent", Some(crate::model::num(v)))
                }
                "lineHeight" => {
                    let v = value.and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite());
                    a.set("lineHeight", v.map(crate::model::num))
                }
                _ => {}
            }
            p
        })
        .collect()
}

/// The paragraph attributes applied to an element's stored text: the new element (the content rewritten, and
/// for `align` left/center/right the element's own default synced, like the web).
pub fn element_para_attr(el: &Element, kind: &str, value: Option<&str>) -> Element {
    let paras = apply_para_attr(&doc_to_paras(el.get("content")), kind, value);
    let mut out = el.clone();
    if kind == "align" {
        if let Some(v @ ("left" | "center" | "right")) = value {
            out.set_s("align", v);
        }
    }
    out.set("content", crate::richtext::paras_to_doc(&paras));
    out
}

/// The run mark of a toolbar kind (inside an edited selection).
pub fn mark_of(kind: &str, value: Option<&str>) -> Option<Mark> {
    Some(match kind {
        "bold" => Mark::Bold,
        "italic" => Mark::Italic,
        "underline" => Mark::Underline,
        "strike" => Mark::Strike,
        "superscript" => Mark::Superscript,
        "subscript" => Mark::Subscript,
        "color" => Mark::Color(value?.to_string()),
        "highlight" => Mark::Highlight(value?.to_string()),
        "size" => Mark::Size(value?.trim().parse::<f64>().ok()?),
        _ => return None,
    })
}

/// The element-level fallback of `applyTextFormat` (the whole box): `None` when the kind has no element-level
/// form (strike, super/subscript, highlight exist only on runs).
pub fn element_format(el: &Element, kind: &str, value: Option<&str>) -> Option<Element> {
    let mut out = el.clone();
    match kind {
        "bold" => out.set_b("bold", !el.truthy("bold")),
        "italic" => out.set_b("italic", !el.truthy("italic")),
        "underline" => out.set_b("underline", !el.truthy("underline")),
        "color" => out.set_s("color", value?),
        "size" => out.set_f("fontSize", value?.trim().parse::<f64>().ok()?),
        "fontFamily" => out.set_s("fontFamily", value?),
        "clear" => {
            out.set_b("bold", false);
            out.set_b("italic", false);
            out.set_b("underline", false);
        }
        _ => return None,
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::richtext::{doc_from_string, paras_to_doc};

    fn el(content: Value) -> Element {
        Element::parse(&format!(r#"{{"id":"t","type":"text","align":"left","content":{content}}}"#)).unwrap_or_else(|| panic!("element"))
    }

    #[test]
    fn bullets_toggle_on_every_paragraph() {
        let e = el(doc_from_string("a\nb"));
        let on = element_para_attr(&e, "bullet", None);
        let paras = doc_to_paras(on.get("content"));
        assert_eq!(paras.len(), 2);
        assert!(paras.iter().all(|p| p.attrs.list() == Some("bullet")));
        let off = element_para_attr(&on, "bullet", None);
        assert!(doc_to_paras(off.get("content")).iter().all(|p| p.attrs.list().is_none()));
    }

    #[test]
    fn indent_is_clamped_and_align_syncs_the_element() {
        let mut e = el(doc_from_string("a"));
        for _ in 0..12 {
            e = element_para_attr(&e, "indentIn", None);
        }
        assert_eq!(doc_to_paras(e.get("content"))[0].attrs.indent(), 240.0);
        let c = element_para_attr(&e, "align", Some("center"));
        assert_eq!(c.s("align"), Some("center"));
        assert_eq!(doc_to_paras(c.get("content"))[0].attrs.align(), Some("center"));
        let j = element_para_attr(&e, "justify", None);
        assert_eq!(j.s("align"), Some("left"));
    }

    #[test]
    fn line_height_sets_and_clears() {
        let e = el(paras_to_doc(&doc_to_paras(Some(&doc_from_string("x")))));
        let l = element_para_attr(&e, "lineHeight", Some("1.5"));
        assert_eq!(doc_to_paras(l.get("content"))[0].attrs.line_height(), Some(1.5));
        let n = element_para_attr(&l, "lineHeight", None);
        assert_eq!(doc_to_paras(n.get("content"))[0].attrs.line_height(), None);
    }

    #[test]
    fn element_level_formats() {
        let e = el(doc_from_string("x"));
        let b = element_format(&e, "bold", None).unwrap_or_else(|| panic!("bold"));
        assert!(b.truthy("bold"));
        assert!(element_format(&e, "strike", None).is_none());
        let s = element_format(&e, "size", Some("32")).unwrap_or_else(|| panic!("size"));
        assert_eq!(s.f("fontSize"), Some(32.0));
        assert_eq!(mark_of("size", Some("12")), Some(Mark::Size(12.0)));
        assert_eq!(mark_of("color", None), None);
    }
}
