//! Bulleted and numbered lists.
//!
//! A list item is a plain `<w:p>` that carries two things: the `ListParagraph`
//! style and a `<w:numPr>` pointing at a definition of `word/numbering.xml`.
//! Drop either one and Word shows a bare paragraph.
//!
//! `super::numbering` hands out one `w:numId` per list TREE: every level of a
//! given tree shares it (so nested counters restart against each other), while
//! two sibling lists never do (so the second one restarts at 1 instead of
//! continuing the first). The nesting depth is carried by `w:ilvl` alone.

use serde_json::Value;

use crate::converters::types::PmNode;

use super::ctx::ExportCtx;
use super::document::render_block;
use super::numbering::{self, LevelSpec, MAX_ILVL};
use super::paragraph::{render_ppr, with_para_mark_revision};
use super::run::render_inline_content;

/// Word's built-in style for list items. Defined in `word/styles.xml`.
const LIST_STYLE: &str = "ListParagraph";

pub(crate) fn render_list(node: &PmNode, ctx: &mut ExportCtx) -> String {
    let ordered = node.node_type == "orderedList";
    let num_id = numbering::alloc_list(ordered, &level_spec(node));
    let mut out = String::new();
    for item in node.children() {
        out.push_str(&render_list_item_with_num(item, &num_id, 0, ctx));
    }
    out
}

/// A stray `listItem` rendered outside any list: fall back on the static bullet
/// definition rather than dropping the item's marker.
pub(crate) fn render_list_item(node: &PmNode, ctx: &mut ExportCtx) -> String {
    render_list_item_with_num(node, numbering::BULLET_NUM_ID, 0, ctx)
}

pub(crate) fn render_list_item_with_num(
    node: &PmNode,
    num_id: &str,
    ilvl: u8,
    ctx: &mut ExportCtx,
) -> String {
    let ilvl = ilvl.min(MAX_ILVL);
    let mut out = String::new();
    // Only the FIRST paragraph of the item bears the numbering; the following
    // ones are continuation paragraphs, which Word keeps in the list style but
    // without a marker of their own.
    let mut numbered = false;
    for child in node.children() {
        match child.node_type.as_str() {
            "paragraph" => {
                let runs = render_inline_content(child.children(), ctx);
                let numpr = if numbered {
                    String::new()
                } else {
                    numbered = true;
                    format!(
                        r#"<w:numPr><w:ilvl w:val="{ilvl}"/><w:numId w:val="{num_id}"/></w:numPr>"#
                    )
                };
                // A deleted list item is tracked on its paragraph mark, exactly
                // like a body paragraph — without this the item's text is struck
                // out but the bullet itself survives accepting the change.
                let ppr = with_para_mark_revision(render_ppr(child, LIST_STYLE, &numpr), child, ctx);
                out.push_str(&format!("<w:p>{ppr}{runs}</w:p>"));
            }
            "bulletList" | "orderedList" => {
                out.push_str(&render_nested_list(child, num_id, ilvl, ctx));
            }
            _ => out.push_str(&render_block(child, ctx)),
        }
    }
    out
}

/// A list nested inside a list item, one level deeper than its parent.
fn render_nested_list(
    list: &PmNode,
    parent_num_id: &str,
    parent_ilvl: u8,
    ctx: &mut ExportCtx,
) -> String {
    let ordered = list.node_type == "orderedList";
    let ilvl = parent_ilvl.saturating_add(1).min(MAX_ILVL);
    let spec = level_spec(list);
    // Same kind as the tree it sits in: keep the parent definition, which is
    // what a multi-level list IS — one `w:num`, nine levels. A bullet list
    // inside a numbered one (or the reverse) is a different tree and needs its
    // own definition, still at the depth it is displayed at.
    let num_id = if ordered == numbering::is_ordered(parent_num_id) {
        parent_num_id.to_string()
    } else {
        numbering::alloc_list(ordered, &LevelSpec::default())
    };
    numbering::set_level(&num_id, ilvl, &spec);

    let mut out = String::new();
    for item in list.children() {
        out.push_str(&render_list_item_with_num(item, &num_id, ilvl, ctx));
    }
    out
}

/// Numbering attributes a list node carries — set by the DOCX import when the
/// source document defined its own marker; absent for a list typed in the
/// editor, which then keeps Word's default level.
fn level_spec(node: &PmNode) -> LevelSpec<'_> {
    let Some(Value::Object(a)) = node.attrs.as_ref() else {
        return LevelSpec::default();
    };
    let s = |k: &str| a.get(k).and_then(|v| v.as_str()).filter(|v| !v.is_empty());
    LevelSpec {
        // `start` is a `<ol start="…">`: 0 is legal, negatives are not.
        start: a
            .get("start")
            .and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)))
            .filter(|v| *v >= 0),
        num_fmt: s("numFmt"),
        lvl_text: s("lvlText"),
        font: s("bulletFont"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn node(json: Value) -> PmNode {
        serde_json::from_value(json).expect("valid PmNode fixture")
    }

    fn item(text: &str) -> Value {
        json!({
            "type": "listItem",
            "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": text }] }]
        })
    }

    #[test]
    fn every_item_carries_a_style_and_a_numpr() {
        let list = node(json!({ "type": "bulletList", "content": [item("a"), item("b")] }));
        let xml = render_list(&list, &mut ExportCtx::new());
        assert_eq!(xml.matches(r#"<w:pStyle w:val="ListParagraph"/>"#).count(), 2);
        assert_eq!(xml.matches(r#"<w:ilvl w:val="0"/>"#).count(), 2);
    }

    #[test]
    fn a_nested_list_of_the_same_kind_keeps_the_num_id() {
        let list = node(json!({
            "type": "orderedList",
            "content": [{
                "type": "listItem",
                "content": [
                    { "type": "paragraph", "content": [{ "type": "text", "text": "a" }] },
                    { "type": "orderedList", "content": [item("a.1")] }
                ]
            }]
        }));
        let xml = render_list(&list, &mut ExportCtx::new());
        let ids: Vec<&str> = xml.match_indices(r#"<w:numId w:val=""#)
            .map(|(i, m)| {
                let rest = &xml[i + m.len()..];
                &rest[..rest.find('"').unwrap_or(0)]
            })
            .collect();
        assert_eq!(ids.len(), 2);
        assert_eq!(ids[0], ids[1], "a multi-level list is a single definition");
        assert!(xml.contains(r#"<w:ilvl w:val="1"/>"#));
    }

    #[test]
    fn a_nested_list_of_the_other_kind_gets_its_own_num_id() {
        let list = node(json!({
            "type": "orderedList",
            "content": [{
                "type": "listItem",
                "content": [
                    { "type": "paragraph", "content": [{ "type": "text", "text": "a" }] },
                    { "type": "bulletList", "content": [item("a.1")] }
                ]
            }]
        }));
        let xml = render_list(&list, &mut ExportCtx::new());
        let ids: Vec<&str> = xml.match_indices(r#"<w:numId w:val=""#)
            .map(|(i, m)| {
                let rest = &xml[i + m.len()..];
                &rest[..rest.find('"').unwrap_or(0)]
            })
            .collect();
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1], "bullets and numbers are two definitions");
    }

    #[test]
    fn only_the_first_paragraph_of_an_item_is_numbered() {
        let list = node(json!({
            "type": "bulletList",
            "content": [{
                "type": "listItem",
                "content": [
                    { "type": "paragraph", "content": [{ "type": "text", "text": "a" }] },
                    { "type": "paragraph", "content": [{ "type": "text", "text": "still a" }] }
                ]
            }]
        }));
        let xml = render_list(&list, &mut ExportCtx::new());
        assert_eq!(xml.matches("<w:numPr>").count(), 1);
        assert_eq!(xml.matches(r#"<w:pStyle w:val="ListParagraph"/>"#).count(), 2);
    }

    /// The `start` of an ordered list survives as a `w:startOverride`, which is
    /// also what stops the NEXT list from continuing this one's counter.
    #[test]
    fn an_ordered_list_restarts_at_its_start_attribute() {
        let list = node(json!({
            "type": "orderedList",
            "attrs": { "start": 3 },
            "content": [item("un"), item("deux")]
        }));
        let body = render_list(&list, &mut ExportCtx::new());
        let num_id = body
            .split(r#"<w:numId w:val=""#)
            .nth(1)
            .and_then(|s| s.split('"').next())
            .unwrap_or_default()
            .to_string();
        let xml = numbering::numbering_xml();
        assert!(xml.contains(&format!(
            r#"<w:num w:numId="{num_id}"><w:abstractNumId w:val="1"/><w:lvlOverride w:ilvl="0"><w:startOverride w:val="3"/>"#
        )));
    }

    #[test]
    fn depth_is_capped_at_word_s_nine_levels() {
        // 12 nested bullet lists: `w:ilvl` must stop at 8, not run to 11.
        let mut inner = json!({ "type": "bulletList", "content": [item("deep")] });
        for _ in 0..12 {
            inner = json!({
                "type": "bulletList",
                "content": [{ "type": "listItem", "content": [
                    { "type": "paragraph", "content": [{ "type": "text", "text": "x" }] },
                    inner
                ]}]
            });
        }
        let xml = render_list(&node(inner), &mut ExportCtx::new());
        assert!(xml.contains(r#"<w:ilvl w:val="8"/>"#));
        assert!(!xml.contains(r#"<w:ilvl w:val="9"/>"#));
    }
}
