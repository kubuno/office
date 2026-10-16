//! The table of contents, generated from the document's outline (`generateToc`,
//! `documents/references/generate.ts`, `outline.ts`): a `tocTitle` heading then one paragraph per
//! entry carrying `tocLevel`, `tocPage`, a leader and `tocKind: "toc"` - the shape the layout paints
//! (`layout::toc`) and « Mettre à jour » finds again.

use serde_json::{json, Value};

use super::commands::Change;
use super::history::Selection;
use super::step::EditError;
use super::commands::EditState;
use crate::model::Node;
use crate::pm;

/// `outlineLevelOfJson`: an explicit `outlineLevel` (0–9) wins; else a heading's level; else 0.
pub fn outline_level_of(node: &Node) -> usize {
    let a = node.attrs();
    if let Some(n) = a.get("outlineLevel").and_then(Value::as_f64) {
        let n = n.trunc() as i64;
        return if (0..=9).contains(&n) { n as usize } else { 0 };
    }
    if node.node_type() != Some("heading") {
        return 0;
    }
    let lvl = a.get("level").and_then(Value::as_f64).map(|l| l.trunc() as i64).unwrap_or(1);
    if (1..=9).contains(&lvl) { lvl as usize } else { 1 }
}

/// One entry of the outline.
#[derive(Clone, Debug, PartialEq)]
pub struct OutlineItem {
    pub text: String,
    pub level: usize,
    pub pos: usize,
    pub page: usize,
}

/// `collectOutline`: every block with an outline level, in reading order (an entry never nests
/// another), with its page (`page_of(pos)`, 1-based).
pub fn collect_outline(doc: &Node, page_of: &dyn Fn(usize) -> usize) -> Vec<OutlineItem> {
    fn walk(node: &Node, start: usize, page_of: &dyn Fn(usize) -> usize, out: &mut Vec<OutlineItem>) {
        let mut pos = start;
        for child in node.children() {
            let level = if matches!(child.node_type(), Some("paragraph" | "heading")) { outline_level_of(child) } else { 0 };
            if level > 0 {
                let text = pm::text_between(child, 0, pm::content_size(child), "", &|_| String::new());
                out.push(OutlineItem { text: if text.is_empty() { "…".into() } else { text }, level, pos, page: page_of(pos) });
            } else if pm::is_container(child) {
                walk(child, pos + 1, page_of, out);
            }
            pos += pm::node_size(child);
        }
    }
    let mut out = Vec::new();
    walk(doc, 0, page_of, &mut out);
    out
}

/// `TocSettings` (the parts generation reads).
#[derive(Clone, Debug, PartialEq)]
pub struct TocSettings {
    pub show_page_numbers: bool,
    pub right_align: bool,
    /// `dots`, `dashes`, `underline`, `none`.
    pub leader: String,
    pub levels: usize,
}

impl Default for TocSettings {
    /// The gallery's automatic table: three levels, numbers right-aligned after dots.
    fn default() -> Self {
        Self { show_page_numbers: true, right_align: true, leader: "dots".into(), levels: 3 }
    }
}

/// One generated entry paragraph (`entryParagraph`, format `template`: level 1 in bold).
fn entry_paragraph(item: &OutlineItem, s: &TocSettings) -> Value {
    let show_page = s.show_page_numbers;
    let mut text = json!({ "type": "text", "text": if show_page && !s.right_align { format!("{}, {}", item.text, item.page) } else { item.text.clone() } });
    if item.level == 1 {
        text["marks"] = json!([{ "type": "bold" }]);
    }
    json!({
        "type": "paragraph",
        "attrs": {
            "indent": item.level.saturating_sub(1),
            "tocLevel": item.level,
            "tocPage": if show_page && s.right_align { json!(item.page) } else { Value::Null },
            "tocPageText": if show_page && s.right_align { json!(item.page.to_string()) } else { Value::Null },
            "tocLeader": show_page && s.right_align && s.leader != "none",
            "tocLeaderKind": s.leader,
            "tocKind": "toc",
            "spaceAfter": 2,
        },
        "content": [text],
    })
}

/// `buildTable` for the table of contents: the title heading (when `title`), then the entries,
/// or one paragraph saying there is no heading.
pub fn build_table(items: &[OutlineItem], s: &TocSettings, title: Option<&str>, empty_text: &str) -> Vec<Node> {
    let mut out: Vec<Value> = Vec::new();
    if let Some(t) = title {
        out.push(json!({ "type": "heading", "attrs": { "level": 2, "tocTitle": true, "outlineLevel": 0, "tocKind": "toc" }, "content": [{ "type": "text", "text": t }] }));
    }
    if items.is_empty() {
        out.push(json!({ "type": "paragraph", "attrs": { "tocLevel": 1, "tocKind": "toc" }, "content": [{ "type": "text", "text": empty_text }] }));
    } else {
        out.extend(items.iter().map(|i| entry_paragraph(i, s)));
    }
    out.into_iter().filter_map(|v| serde_json::from_value(v).ok()).collect()
}

/// The existing table: `(first block, end block, has a title, an imported title heading's index)` -
/// a `tocTitle` heading and the contiguous `tocLevel` paragraphs after it, or (imported) the
/// `tocLevel` paragraphs alone.
fn existing_block(doc: &Node) -> Option<(usize, usize, bool, Option<usize>)> {
    let kids = doc.children();
    let mut start = None;
    let mut end = 0;
    let mut with_title = true;
    let mut title_idx = None;
    for (i, node) in kids.iter().enumerate() {
        let a = node.attrs();
        let is_entry = a.get("tocLevel").is_some_and(|v| !v.is_null());
        match start {
            None => {
                if a.get("tocTitle").and_then(Value::as_bool) == Some(true) {
                    start = Some(i);
                    end = i + 1;
                } else if is_entry {
                    start = Some(i);
                    end = i + 1;
                    with_title = false;
                    if i > 0 && kids[i - 1].node_type() == Some("heading") {
                        title_idx = Some(i - 1);
                    }
                }
            }
            Some(_) if end == i && is_entry => end = i + 1,
            Some(_) => {}
        }
    }
    start.map(|s| (s, end, with_title, title_idx))
}

/// `generateToc`: builds the table from `outline` and puts it where the existing one is (« Mettre
/// à jour »), else before the caret's top-level block (never inside a paragraph).
pub fn generate(state: &mut EditState, outline: &[OutlineItem], s: &TocSettings, title: &str, empty_text: &str) -> Result<Change, EditError> {
    let existing = existing_block(&state.doc);
    // The table's own title headings are not entries (nor an imported title above its entries).
    let mut excluded: Vec<usize> = Vec::new();
    let mut pos = 0;
    for (i, node) in state.doc.children().iter().enumerate() {
        if node.attrs().get("tocTitle").and_then(Value::as_bool) == Some(true) || existing.and_then(|e| e.3) == Some(i) {
            excluded.push(pos);
        }
        pos += pm::node_size(node);
    }
    let items: Vec<OutlineItem> = outline.iter().filter(|it| !excluded.contains(&it.pos) && it.level >= 1 && it.level <= s.levels).cloned().collect();
    let with_title = existing.map(|e| e.2).unwrap_or(true);
    let nodes = build_table(&items, s, with_title.then_some(title), empty_text);
    let (from, to) = match existing {
        Some((a, b, ..)) => (a, b),
        None => {
            let head = state.sel.head;
            let mut pos = 0;
            let mut at = state.doc.children().len();
            for (i, node) in state.doc.children().iter().enumerate() {
                let size = pm::node_size(node);
                if head <= pos + size {
                    at = i;
                    break;
                }
                pos += size;
            }
            (at, at)
        }
    };
    replace_blocks(state, from, to, nodes)
}

/// `handleTocRemove`: deletes every block of the table (`tocKind: "toc"`, `tocTitle`, `tocLevel`)
/// from the first to the last.
pub fn remove(state: &mut EditState) -> Result<Change, EditError> {
    let kids = state.doc.children();
    let hit = |n: &Node| {
        let a = n.attrs();
        a.get("tocKind").and_then(Value::as_str) == Some("toc") || a.get("tocTitle").and_then(Value::as_bool) == Some(true) || a.get("tocLevel").is_some_and(|v| !v.is_null())
    };
    let first = kids.iter().position(hit).ok_or(EditError::NotApplicable)?;
    let last = kids.iter().rposition(hit).ok_or(EditError::NotApplicable)?;
    replace_blocks(state, first, last + 1, Vec::new())
}

/// Replaces top-level blocks `from..to` with `nodes`, the caret at the start of the first new one
/// (or where the removed blocks were).
fn replace_blocks(state: &mut EditState, from: usize, to: usize, nodes: Vec<Node>) -> Result<Change, EditError> {
    let before = state.sel;
    let start: usize = state.doc.children()[..from].iter().map(pm::node_size).sum();
    let step = super::step::Step { parent: Vec::new(), from, to, with: nodes };
    let inverse = step.apply(&mut state.doc)?;
    let after = Selection::caret(pm::textblocks(&state.doc).into_iter().map(|(_, s)| s).find(|&s| s > start).unwrap_or(start));
    state.sel = after;
    Ok(Change { steps: vec![step], inverses: vec![inverse], before, after, merge: true })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(json: &str) -> Node {
        serde_json::from_str(json).expect("doc")
    }

    #[test]
    fn the_outline_follows_outline_level_then_heading_level() {
        let d = doc(r#"{"type":"doc","content":[{"type":"heading","attrs":{"level":1},"content":[{"type":"text","text":"Un"}]},{"type":"paragraph","content":[{"type":"text","text":"corps"}]},{"type":"heading","attrs":{"level":2,"outlineLevel":0},"content":[{"type":"text","text":"Pas une entrée"}]},{"type":"paragraph","attrs":{"outlineLevel":2},"content":[{"type":"text","text":"Promu"}]},{"type":"heading","attrs":{"level":3}}]}"#);
        let o = collect_outline(&d, &|pos| pos / 10 + 1);
        let got: Vec<(&str, usize)> = o.iter().map(|i| (i.text.as_str(), i.level)).collect();
        assert_eq!(got, [("Un", 1), ("Promu", 2), ("…", 3)]);
        assert_eq!(o[0].pos, 0);
    }

    #[test]
    fn a_table_is_built_like_build_table_and_replaced_by_an_update() {
        let d = doc(r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Avant"}]},{"type":"heading","attrs":{"level":1},"content":[{"type":"text","text":"Chapitre"}]},{"type":"heading","attrs":{"level":2},"content":[{"type":"text","text":"Section"}]},{"type":"heading","attrs":{"level":4},"content":[{"type":"text","text":"Trop profond"}]}]}"#);
        let mut state = EditState { doc: d, sel: Selection::caret(0), stored_marks: None };
        state.sel = Selection::caret(1);
        let outline = collect_outline(&state.doc, &|_| 1);
        generate(&mut state, &outline, &TocSettings::default(), "Table des matières", "Aucun titre").expect("inserted");
        let kinds: Vec<String> = state.doc.children().iter().map(|n| n.node_type().unwrap_or("").to_string()).collect();
        assert_eq!(kinds[..3], ["heading", "paragraph", "paragraph"], "title + two entries (level 4 > 3 levels), before the caret's block");
        let e1 = state.doc.children()[1].attrs();
        assert_eq!((e1["tocLevel"].clone(), e1["tocPage"].clone(), e1["tocLeader"].clone(), e1["indent"].clone()), (json!(1), json!(1), json!(true), json!(0)));
        assert_eq!(state.doc.children()[2].attrs()["indent"], json!(1));
        // « Mettre à jour » with new pages: the same block is replaced, not a second one added.
        let outline = collect_outline(&state.doc, &|_| 7);
        generate(&mut state, &outline, &TocSettings::default(), "Table des matières", "Aucun titre").expect("updated");
        let tables = state.doc.children().iter().filter(|n| n.attrs().get("tocTitle").is_some()).count();
        assert_eq!(tables, 1);
        assert_eq!(state.doc.children()[1].attrs()["tocPage"], json!(7));
        remove(&mut state).expect("removed");
        assert!(state.doc.children().iter().all(|n| n.attrs().get("tocKind").is_none()));
        assert_eq!(remove(&mut state).map(|_| ()), Err(EditError::NotApplicable));
    }
}
