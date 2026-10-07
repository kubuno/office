//! The Accueil › Outils de texte menu and its neighbours, ported from `DocumentEditorPage.tsx`
//! (`removeEmptyParagraphs`, `transformTextNodes`, `smartQuotes`, `numberParagraphs`,
//! `reverseParagraphs`, `dedupeParagraphs`, `sortParagraphs`, `textToTable`, `tableToText`, and the
//! inserted blocks of `handleInsertCaption`, `handleSignatureLine`, `handleInsertCoverPage`).
//!
//! Each runs as ONE step replacing the body's blocks, so a single Ctrl+Z restores the exact bytes.

use serde_json::{json, Value};

use super::commands::{Change, EditState};
use super::history::Selection;
use super::step::{EditError, Step};
use crate::model::Node;
use crate::pm::{self, Path};

/// Replaces the whole body with `kids` (one undoable step), the caret at `caret`.
fn replace_body(state: &mut EditState, kids: Vec<Node>, caret: Option<usize>) -> Result<Change, EditError> {
    let before = state.sel;
    let n = state.doc.children().len();
    let step = Step { parent: Vec::new(), from: 0, to: n, with: kids };
    let inverse = step.apply(&mut state.doc)?;
    let size = pm::content_size(&state.doc);
    let after = match caret {
        Some(c) => Selection::caret(c.min(size)),
        None => Selection::new(before.anchor.min(size), before.head.min(size)),
    };
    state.sel = after;
    Ok(Change { steps: vec![step], inverses: vec![inverse], before, after, merge: true })
}

fn text_of(node: &Node) -> String {
    pm::text_between(node, 0, pm::content_size(node), "", &|_| String::new())
}

/// `selectedBlocks`: the paragraphs and headings the selection touches (the whole document when it
/// is empty), at any depth, never inside one another: their paths, in order.
pub fn selected_blocks(doc: &Node, sel: Selection) -> Vec<Path> {
    let (lo, hi) = if sel.is_empty() { (0, pm::content_size(doc)) } else { (sel.from(), sel.to()) };
    fn walk(node: &Node, start: usize, lo: usize, hi: usize, path: &mut Path, out: &mut Vec<Path>) {
        let mut pos = start;
        for (i, child) in node.children().iter().enumerate() {
            let size = pm::node_size(child);
            // `nodesBetween(lo, hi)`: the nodes overlapping [lo, hi).
            if pos < hi && pos + size > lo || (lo == hi && pos == lo) {
                if matches!(child.node_type(), Some("paragraph" | "heading")) {
                    path.push(i);
                    out.push(path.clone());
                    path.pop();
                } else if pm::is_container(child) {
                    path.push(i);
                    walk(child, pos + 1, lo, hi, path, out);
                    path.pop();
                }
            }
            pos += size;
        }
    }
    let mut out = Vec::new();
    walk(doc, 0, lo, hi, &mut Vec::new(), &mut out);
    out
}

/// A copy of `doc` with the node at each path replaced by the paired node.
fn with_nodes_at(doc: &Node, swaps: Vec<(Path, Node)>) -> Node {
    let mut d = doc.clone();
    for (path, node) in swaps {
        if let Some((last, parent)) = path.split_last() {
            if let Some(kids) = pm::node_at_mut(&mut d, parent).and_then(|p| p.children_mut()) {
                if let Some(slot) = kids.get_mut(*last) {
                    *slot = node;
                }
            }
        }
    }
    d
}

/// `removeEmptyParagraphs`: the empty top-level paragraphs go (at least one block stays).
pub fn remove_empty_paragraphs(state: &mut EditState) -> Result<Change, EditError> {
    let kids = state.doc.children();
    let empty = |n: &Node| n.node_type() == Some("paragraph") && pm::content_size(n) == 0;
    let mut drop: Vec<usize> = kids.iter().enumerate().filter(|(_, n)| empty(n)).map(|(i, _)| i).collect();
    if drop.len() >= kids.len() {
        drop.pop();
    }
    if drop.is_empty() {
        return Err(EditError::NotApplicable);
    }
    let kept: Vec<Node> = kids.iter().enumerate().filter(|(i, _)| !drop.contains(i)).map(|(_, n)| n.clone()).collect();
    replace_body(state, kept, None)
}

/// `transformTextNodes`: every text node's text through `f` (an emptied node goes).
pub fn transform_text(state: &mut EditState, f: &dyn Fn(&str) -> String) -> Result<Change, EditError> {
    fn walk(node: &Node, f: &dyn Fn(&str) -> String, changed: &mut bool) -> Option<Node> {
        if node.node_type() == Some("text") {
            let t = node.text().unwrap_or_default();
            let r = f(&t);
            if r == t {
                return Some(node.clone());
            }
            *changed = true;
            if r.is_empty() {
                return None;
            }
            let mut n = node.clone();
            n.set_text(&r);
            return Some(n);
        }
        if !node.has_child_array() {
            return Some(node.clone());
        }
        let mut n = node.clone();
        let kids: Vec<Node> = node.children().iter().filter_map(|c| walk(c, f, changed)).collect();
        n.set_children(kids);
        Some(n)
    }
    let mut changed = false;
    let kids: Vec<Node> = state.doc.children().iter().filter_map(|c| walk(c, f, &mut changed)).collect();
    if !changed {
        return Err(EditError::NotApplicable);
    }
    replace_body(state, kids, None)
}

/// « Supprimer les espaces en trop »: runs of two spaces or more become one.
pub fn collapse_spaces(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut run = 0;
    for c in s.chars() {
        if c == ' ' {
            run += 1;
            if run == 1 {
                out.push(' ');
            }
        } else {
            run = 0;
            out.push(c);
        }
    }
    out
}

/// « Tabulations en espaces »: a tab becomes four spaces.
pub fn tabs_to_spaces(s: &str) -> String {
    s.replace('\t', "    ")
}

/// `smartQuotes`: `"x"` → `« x »` (pairs, left to right), every `'` → `’`.
pub fn smart_quotes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('"') {
        let after = &rest[i + 1..];
        match after.find('"') {
            Some(j) => {
                out.push_str(&rest[..i]);
                out.push_str("« ");
                out.push_str(&after[..j]);
                out.push_str(" »");
                rest = &after[j + 1..];
            }
            None => break,
        }
    }
    out.push_str(rest);
    out.replace('\'', "\u{2019}")
}

/// `numberParagraphs`: « 1. », « 2. »… at the start of each non-empty selected block (with the
/// marks of its first text).
pub fn number_paragraphs(state: &mut EditState) -> Result<Change, EditError> {
    let paths: Vec<Path> = selected_blocks(&state.doc, state.sel).into_iter().filter(|p| pm::node_at(&state.doc, p).is_some_and(|n| !text_of(n).trim().is_empty())).collect();
    if paths.is_empty() {
        return Err(EditError::NotApplicable);
    }
    let swaps: Vec<(Path, Node)> = paths
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            let block = pm::node_at(&state.doc, p)?;
            let marks = super::inline::marks_at(block.children(), 0);
            let kids = super::inline::insert_text(block.children(), 0, &format!("{}. ", i + 1), &marks).ok()?;
            Some((p.clone(), super::structure::with_children(block, kids)))
        })
        .collect();
    let doc = with_nodes_at(&state.doc, swaps);
    replace_body(state, doc.children().to_vec(), None)
}

/// `reverseParagraphs`: the selected blocks in the opposite order (whole nodes move: marks kept).
pub fn reverse_paragraphs(state: &mut EditState) -> Result<Change, EditError> {
    let paths = selected_blocks(&state.doc, state.sel);
    if paths.len() < 2 {
        return Err(EditError::NotApplicable);
    }
    let nodes: Vec<Node> = paths.iter().filter_map(|p| pm::node_at(&state.doc, p).cloned()).collect();
    let swaps: Vec<(Path, Node)> = paths.iter().cloned().zip(nodes.into_iter().rev()).collect();
    let doc = with_nodes_at(&state.doc, swaps);
    replace_body(state, doc.children().to_vec(), None)
}

/// `sortParagraphs`: the selected blocks (the whole document when the selection is empty) by their
/// lower-cased text, ascending or descending, equal keys keeping their order.
pub fn sort_paragraphs(state: &mut EditState, ascending: bool) -> Result<Change, EditError> {
    let paths = selected_blocks(&state.doc, state.sel);
    if paths.len() < 2 {
        return Err(EditError::NotApplicable);
    }
    let nodes: Vec<Node> = paths.iter().filter_map(|p| pm::node_at(&state.doc, p).cloned()).collect();
    let mut order: Vec<(usize, String)> = nodes.iter().enumerate().map(|(i, n)| (i, text_of(n).to_lowercase())).collect();
    order.sort_by(|a, b| if ascending { a.1.cmp(&b.1) } else { b.1.cmp(&a.1) });
    let swaps: Vec<(Path, Node)> = paths.iter().cloned().zip(order.iter().map(|(i, _)| nodes[*i].clone())).collect();
    let doc = with_nodes_at(&state.doc, swaps);
    replace_body(state, doc.children().to_vec(), None)
}

/// `dedupeParagraphs`: of consecutive selected blocks with the same (non-blank) text, the first
/// stays. A block that is its container's only child stays too (the container would be invalid).
pub fn dedupe_paragraphs(state: &mut EditState) -> Result<Change, EditError> {
    let paths = selected_blocks(&state.doc, state.sel);
    let texts: Vec<String> = paths.iter().map(|p| pm::node_at(&state.doc, p).map(text_of).unwrap_or_default()).collect();
    let mut drop: Vec<Path> = Vec::new();
    for i in 1..paths.len() {
        if !texts[i].trim().is_empty() && texts[i] == texts[i - 1] {
            drop.push(paths[i].clone());
        }
    }
    if drop.is_empty() {
        return Err(EditError::NotApplicable);
    }
    let mut doc = state.doc.clone();
    for path in drop.iter().rev() {
        let Some((last, parent)) = path.split_last() else { continue };
        if let Some(kids) = pm::node_at_mut(&mut doc, parent).and_then(|p| p.children_mut()) {
            if kids.len() > 1 && *last < kids.len() {
                kids.remove(*last);
            }
        }
    }
    replace_body(state, doc.children().to_vec(), None)
}

/// `textToTable`: the selected top-level blocks become a table (one row each; columns split on a
/// tab, else `;`, else `,`, trimmed), then an empty paragraph.
pub fn text_to_table(state: &mut EditState) -> Result<Change, EditError> {
    let paths = selected_blocks(&state.doc, state.sel);
    if paths.is_empty() || paths.iter().any(|p| p.len() != 1) {
        return Err(EditError::NotApplicable);
    }
    let lines: Vec<String> = paths.iter().map(|p| pm::node_at(&state.doc, p).map(text_of).unwrap_or_default()).collect();
    let sep = if lines.iter().any(|l| l.contains('\t')) { '\t' } else if lines.iter().any(|l| l.contains(';')) { ';' } else { ',' };
    let rows: Vec<Vec<String>> = lines.iter().map(|l| l.split(sep).map(|c| c.trim().to_string()).collect()).collect();
    let cols = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
    let table = json!({
        "type": "table",
        "content": rows.iter().map(|r| json!({
            "type": "tableRow",
            "content": (0..cols).map(|i| {
                let text = r.get(i).filter(|s| !s.is_empty());
                json!({ "type": "tableCell", "content": [ match text {
                    Some(t) => json!({ "type": "paragraph", "content": [{ "type": "text", "text": t }] }),
                    None => json!({ "type": "paragraph", "content": [] }),
                } ] })
            }).collect::<Vec<Value>>(),
        })).collect::<Vec<Value>>(),
    });
    let (first, last) = (paths[0][0], paths[paths.len() - 1][0]);
    let mut kids = state.doc.children().to_vec();
    let new: Vec<Node> = [table, json!({ "type": "paragraph" })].into_iter().filter_map(|v| serde_json::from_value(v).ok()).collect();
    kids.splice(first..=last, new);
    let start: usize = state.doc.children()[..first].iter().map(pm::node_size).sum();
    replace_body(state, kids, Some(start + 4))
}

/// `tableToText`: the table holding the caret becomes one paragraph per row, its cells joined by
/// tabs.
pub fn table_to_text(state: &mut EditState) -> Result<Change, EditError> {
    let r = pm::resolve(&state.doc, state.sel.head).ok_or(EditError::BadPosition)?;
    let mut table_path = None;
    for d in (1..=r.path.len()).rev() {
        if pm::node_at(&state.doc, &r.path[..d]).and_then(Node::node_type) == Some("table") {
            table_path = Some(r.path[..d].to_vec());
            break;
        }
    }
    let path = table_path.ok_or(EditError::NotApplicable)?;
    let table = pm::node_at(&state.doc, &path).ok_or(EditError::NoSuchNode)?;
    let paras: Vec<Node> = table
        .children()
        .iter()
        .map(|row| {
            let line = row.children().iter().map(text_of).collect::<Vec<_>>().join("\t");
            let v = if line.is_empty() { json!({ "type": "paragraph", "content": [] }) } else { json!({ "type": "paragraph", "content": [{ "type": "text", "text": line }] }) };
            serde_json::from_value(v).unwrap_or_default()
        })
        .collect();
    let (last, parent) = path.split_last().ok_or(EditError::BadPosition)?;
    let mut doc = state.doc.clone();
    if let Some(kids) = pm::node_at_mut(&mut doc, parent).and_then(|p| p.children_mut()) {
        kids.splice(*last..=*last, paras);
    }
    let start = pm::pos_before(&state.doc, &path).unwrap_or(0);
    replace_body(state, doc.children().to_vec(), Some(start + 1))
}

/// `handleInsertCaption`: « Figure N : » (N = the largest figure number + 1), centred, italic 9 pt.
pub fn caption_blocks(doc: &Node) -> Vec<Node> {
    let mut max = 0u64;
    for (path, _) in pm::textblocks(doc) {
        if let Some(n) = pm::node_at(doc, &path) {
            let t = text_of(n);
            let lower = t.to_lowercase();
            if let Some(rest) = lower.strip_prefix("figure") {
                let digits: String = rest.trim_start().chars().take_while(char::is_ascii_digit).collect();
                if rest.starts_with(char::is_whitespace) {
                    if let Ok(v) = digits.parse::<u64>() {
                        max = max.max(v);
                    }
                }
            }
        }
    }
    let v = json!({ "type": "paragraph", "attrs": { "textAlign": "center" }, "content": [{ "type": "text", "marks": [{ "type": "italic" }, { "type": "textStyle", "attrs": { "fontSize": "9pt" } }], "text": format!("Figure {} : ", max + 1) }] });
    serde_json::from_value(v).into_iter().collect()
}

/// `handleSignatureLine`: a long rule of underscores, then a small grey « Signature ».
pub fn signature_blocks(label: &str) -> Vec<Node> {
    [
        json!({ "type": "paragraph", "attrs": { "spaceBefore": 36 }, "content": [{ "type": "text", "text": "________________________" }] }),
        json!({ "type": "paragraph", "content": [{ "type": "text", "marks": [{ "type": "textStyle", "attrs": { "fontSize": "9pt", "color": "#5f6368" } }], "text": label }] }),
    ]
    .into_iter()
    .filter_map(|v| serde_json::from_value(v).ok())
    .collect()
}

/// `handleInsertCoverPage`: the two cover pages, inserted at the very start (`insertContentAt(0)`).
/// `long_date` is « 7 octobre 2026 », `short_date` « 07/10/2026 » (the platform's locale).
pub fn cover_blocks(variant: u8, title: &str, subtitle: &str, author: &str, long_date: &str, short_date: &str) -> Vec<Node> {
    let v = if variant == 1 {
        vec![
            json!({ "type": "paragraph", "attrs": { "spaceBefore": 120 } }),
            json!({ "type": "heading", "attrs": { "level": 1, "textAlign": "center" }, "content": [{ "type": "text", "text": title }] }),
            json!({ "type": "paragraph", "attrs": { "textAlign": "center", "spaceBefore": 8 }, "content": [{ "type": "text", "marks": [{ "type": "italic" }, { "type": "textStyle", "attrs": { "fontSize": "14pt" } }], "text": subtitle }] }),
            json!({ "type": "paragraph", "attrs": { "textAlign": "center", "spaceBefore": 160 }, "content": [{ "type": "text", "text": long_date }] }),
            json!({ "type": "pageBreak" }),
        ]
    } else {
        vec![
            json!({ "type": "paragraph", "attrs": { "spaceBefore": 200 } }),
            json!({ "type": "heading", "attrs": { "level": 1 }, "content": [{ "type": "text", "text": title }] }),
            json!({ "type": "paragraph", "content": [{ "type": "text", "marks": [{ "type": "textStyle", "attrs": { "color": "#5f6368" } }], "text": subtitle }] }),
            json!({ "type": "horizontalRule" }),
            json!({ "type": "paragraph", "content": [{ "type": "text", "marks": [{ "type": "bold" }], "text": format!("{author} · ") }, { "type": "text", "text": short_date }] }),
            json!({ "type": "pageBreak" }),
        ]
    };
    v.into_iter().filter_map(|v| serde_json::from_value(v).ok()).collect()
}

/// `insertContentAt(0, blocks)`: blocks before everything.
pub fn insert_at_start(state: &mut EditState, blocks: Vec<Node>) -> Result<Change, EditError> {
    let mut kids = blocks;
    kids.extend(state.doc.children().iter().cloned());
    replace_body(state, kids, Some(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(json: &str) -> EditState {
        EditState { doc: serde_json::from_str(json).expect("doc"), sel: Selection::caret(0), stored_marks: None }
    }

    fn texts(s: &EditState) -> Vec<String> {
        s.doc.children().iter().map(text_of).collect()
    }

    const PARAS: &str = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"banane"}]},{"type":"paragraph"},{"type":"paragraph","content":[{"type":"text","text":"Abricot"}]},{"type":"paragraph","content":[{"type":"text","text":"Abricot"}]},{"type":"paragraph","content":[{"type":"text","text":"cerise  \t\"rouge\" l'été"}]}]}"#;

    #[test]
    fn text_tools_do_what_the_web_menu_does() {
        let mut s = state(PARAS);
        remove_empty_paragraphs(&mut s).expect("removed");
        assert_eq!(texts(&s).len(), 4);
        dedupe_paragraphs(&mut s).expect("deduped");
        assert_eq!(texts(&s), ["banane", "Abricot", "cerise  \t\"rouge\" l'été"]);
        transform_text(&mut s, &collapse_spaces).expect("spaces");
        transform_text(&mut s, &tabs_to_spaces).expect("tabs");
        transform_text(&mut s, &smart_quotes).expect("quotes");
        assert_eq!(texts(&s)[2], "cerise     « rouge » l\u{2019}été");
        sort_paragraphs(&mut s, true).expect("sorted");
        assert_eq!(texts(&s)[..2], ["Abricot", "banane"], "case-insensitive");
        reverse_paragraphs(&mut s).expect("reversed");
        assert_eq!(texts(&s)[0], "cerise     « rouge » l\u{2019}été");
        number_paragraphs(&mut s).expect("numbered");
        assert_eq!(texts(&s), ["1. cerise     « rouge » l\u{2019}été", "2. banane", "3. Abricot"]);
        assert_eq!(remove_empty_paragraphs(&mut s).map(|_| ()), Err(EditError::NotApplicable));
    }

    #[test]
    fn text_and_tables_convert_both_ways() {
        let mut s = state(r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"a;b"}]},{"type":"paragraph","content":[{"type":"text","text":"c"}]}]}"#);
        text_to_table(&mut s).expect("to a table");
        let t = &s.doc.children()[0];
        assert_eq!(t.node_type(), Some("table"));
        assert_eq!(t.children().len(), 2);
        assert_eq!(t.children()[1].children().len(), 2, "every row has the widest row's columns");
        assert_eq!(s.doc.children()[1].node_type(), Some("paragraph"));
        s.sel = Selection::caret(4);
        table_to_text(&mut s).expect("to text");
        assert_eq!(texts(&s)[..2], ["a\tb", "c\t"]);
    }

    #[test]
    fn a_caption_continues_the_figure_numbering() {
        let s = state(r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Figure 4 : un graphe"}]}]}"#);
        assert_eq!(caption_blocks(&s.doc).first().map(text_of).as_deref(), Some("Figure 5 : "));
        assert_eq!(cover_blocks(1, "T", "S", "A", "7 octobre 2026", "07/10/2026").len(), 5);
        assert_eq!(signature_blocks("Signature").len(), 2);
    }
}
