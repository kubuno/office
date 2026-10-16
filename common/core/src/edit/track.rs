//! Track changes (« Suivi des modifications »), as `documents/track-changes.ts` runs it.
//!
//! While tracking is on, the web's `appendTransaction` re-expresses every replace step as revision
//! marks: inserted inline content gets an `insertion` mark, deleted inline content is put back
//! carrying a `deletion` mark - except the author's own pending insertions, which really go. A
//! removed range holding no inline content (a paragraph join, a block atom) is a real deletion; a
//! paragraph split inserts no inline content and stays untracked. Mark-only steps (formatting) and
//! node-attribute changes are never tracked.
//!
//! Here the same rules run at command level: [`tracked_insert`] wraps any command that inserts at
//! the selection (typing, paste, inline nodes), [`tracked_delete`] replaces Backspace / Delete.
//! [`decide`] is accept / reject, [`document_changes`] the review pane's list.

use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{json, Value};

use super::commands::{self, Change, EditState};
use super::history::Selection;
use super::inline;
use super::step::{EditError, Tr};
use super::structure;
use crate::marks;
use crate::model::Node;
use crate::pm::{self, node_at, textblock_at, textblocks};

/// Mark names - also the OOXML pair `w:ins` / `w:del`.
pub const MARK_INSERTION: &str = "insertion";
pub const MARK_DELETION: &str = "deletion";

/// The author stamped on new revisions (`TrackChangesUser`). Both fields may be empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrackUser {
    pub id: String,
    pub name: String,
}

/// What a new revision is stamped with: its author, the date (ISO-8601 UTC, second precision -
/// `nowIso`) and the id it takes when it does not continue a neighbouring revision.
#[derive(Clone, Debug)]
pub struct Stamp {
    pub user: TrackUser,
    pub date: String,
    pub id: String,
    /// The id a new insertion takes (the web draws a fresh `uid()` for every revision it starts, so a
    /// typed-over selection is two revisions).
    pub insertion_id: String,
}

impl Stamp {
    /// A stamp for an edit made now (`now_ms`: Unix time in milliseconds).
    pub fn now(user: &TrackUser, now_ms: u64) -> Self {
        Self { user: user.clone(), date: iso_utc(now_ms), id: fresh_id(now_ms), insertion_id: fresh_id(now_ms) }
    }
}

/// `ChangeKind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChangeKind {
    Insertion,
    Deletion,
}

impl ChangeKind {
    pub fn mark(self) -> &'static str {
        match self {
            ChangeKind::Insertion => MARK_INSERTION,
            ChangeKind::Deletion => MARK_DELETION,
        }
    }
}

/// One revision, as `documentChanges()` reports it.
#[derive(Clone, Debug, PartialEq)]
pub struct TrackedChange {
    pub id: String,
    pub kind: ChangeKind,
    pub author: String,
    pub author_id: String,
    pub date: String,
    pub from: usize,
    pub to: usize,
    /// The text of the change (a paragraph break inside it becomes "\n").
    pub text: String,
}

/// `nowIso()`: `YYYY-MM-DDTHH:MM:SSZ`.
pub fn iso_utc(epoch_ms: u64) -> String {
    let secs = epoch_ms / 1000;
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Civil date from days since 1970-01-01 (H. Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

fn base36(mut n: u64, width: usize) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = Vec::with_capacity(width);
    for _ in 0..width {
        out.push(DIGITS[(n % 36) as usize]);
        n /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// Unix milliseconds of an ISO-8601 UTC date (`Date.parse` of the `YYYY-MM-DDTHH:MM:SS[.sss]Z` form
/// the marks carry); `None` when it does not parse.
pub fn parse_iso_utc(iso: &str) -> Option<u64> {
    let b = iso.as_bytes();
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b' ') || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let n = |r: std::ops::Range<usize>| iso.get(r).and_then(|s| s.parse::<i64>().ok());
    let (y, mo, d, h, mi, s) = (n(0..4)?, n(5..7)?, n(8..10)?, n(11..13)?, n(14..16)?, n(17..19)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || s > 60 {
        return None;
    }
    // Days from civil (H. Hinnant).
    let yy = if mo <= 2 { y - 1 } else { y };
    let era = yy.div_euclid(400);
    let yoe = yy - era * 400;
    let mp = if mo > 2 { mo - 3 } else { mo + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let secs = days * 86_400 + h * 3600 + mi * 60 + s;
    u64::try_from(secs).ok().map(|v| v * 1000)
}

/// `relDate` of the review pane: how long ago a revision was made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Age {
    /// Under a minute: « à l'instant ».
    Now,
    Minutes(u64),
    Hours(u64),
    /// A day or more: the date (Unix milliseconds) the caller writes in the user's locale.
    Date(u64),
}

/// The age of a revision dated `iso` at `now_ms`; `None` for a date that does not parse (shown as
/// nothing).
pub fn age(iso: &str, now_ms: u64) -> Option<Age> {
    let ts = parse_iso_utc(iso)?;
    let s = now_ms.saturating_sub(ts) / 1000;
    Some(if s < 60 {
        Age::Now
    } else if s / 60 < 60 {
        Age::Minutes(s / 60)
    } else if s / 3600 < 24 {
        Age::Hours(s / 3600)
    } else {
        Age::Date(ts)
    })
}

/// `uid()`: eight pseudo-random base-36 characters then the last four of the time in base 36.
pub fn fresh_id(now_ms: u64) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let c = COUNTER.fetch_add(1, Ordering::Relaxed);
    // splitmix64 over the time and a process-wide counter: distinct ids within one millisecond.
    let mut z = now_ms.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ c.wrapping_add(0x632B_E59B_D9B4_E019);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    let tail = base36(now_ms, 9);
    format!("{}{}", base36(z, 8), &tail[tail.len() - 4..])
}

const AUTHOR_PALETTE: [&str; 10] = ["#1a73e8", "#d93025", "#1e8e3e", "#f9ab00", "#9334e6", "#e8710a", "#12b5cb", "#d01884", "#7cb342", "#3949ab"];

/// `authorColor(key)`: a stable colour per author (UTF-16 code units hashed `h * 31 + c`).
pub fn author_color(key: &str) -> &'static str {
    let mut h: u32 = 0;
    for u in key.encode_utf16() {
        h = h.wrapping_mul(31).wrapping_add(u32::from(u));
    }
    AUTHOR_PALETTE[(h as usize) % AUTHOR_PALETTE.len()]
}

fn attr(mark: &Value, key: &str) -> String {
    match mark.get("attrs").and_then(|a| a.get(key)) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

/// `sameAuthor`: empty ids fall back to the name.
fn same_author(mark: &Value, user: &TrackUser) -> bool {
    let aid = attr(mark, "authorId");
    if !aid.is_empty() || !user.id.is_empty() {
        return aid == user.id;
    }
    attr(mark, "author") == user.name
}

fn mark_of<'a>(marks: &'a [Value], kind: &str) -> Option<&'a Value> {
    marks.iter().find(|m| m.get("type").and_then(Value::as_str) == Some(kind))
}

/// `changeAttrs`.
fn change_mark(kind: &str, stamp: &Stamp, reuse: Option<(String, String)>) -> Value {
    let fresh = if kind == MARK_INSERTION { &stamp.insertion_id } else { &stamp.id };
    let (id, date) = reuse.unwrap_or_else(|| (fresh.clone(), stamp.date.clone()));
    json!({ "type": kind, "attrs": { "author": stamp.user.name, "authorId": stamp.user.id, "date": date, "id": id } })
}

/// The revision `(id, date)` of `kind` by the same author on one of `nodes` (`reuseAt`): consecutive
/// keystrokes form ONE revision.
fn reuse_of(nodes: &[Option<&Node>], kind: &str, user: &TrackUser) -> Option<(String, String)> {
    for n in nodes.iter().flatten() {
        let ms = marks::marks_of(n);
        if let Some(m) = mark_of(&ms, kind).filter(|m| same_author(m, user)) {
            let id = attr(m, "id");
            if !id.is_empty() {
                let date = attr(m, "date");
                return Some((id, date));
            }
        }
    }
    None
}

/// The inline nodes just before and just after `offset` in `children` (`$pos.nodeBefore/After`).
fn around(children: &[Node], offset: usize) -> (Option<&Node>, Option<&Node>) {
    let (mut before, mut after) = (None, None);
    let mut pos = 0usize;
    for c in children {
        let end = pos + inline::inline_len(c);
        if pos < offset && offset <= end {
            before = Some(c);
        }
        if pos <= offset && offset < end && after.is_none() {
            after = Some(c);
        }
        pos = end;
    }
    (before, after)
}

fn with_marks(node: &Node, ms: &[Value]) -> Node {
    let mut n = node.clone();
    match marks::to_raw(ms) {
        Some(raw) => n.set_raw("marks", raw),
        None => {
            if n.has("marks") {
                n.remove("marks");
            }
        }
    }
    n
}

fn welded(nodes: Vec<Node>) -> Vec<Node> {
    let mut out = Vec::with_capacity(nodes.len());
    for n in nodes {
        if pm::is_text(&n) && n.text().is_some_and(|t| t.is_empty()) {
            continue;
        }
        inline::push_merged(&mut out, n);
    }
    out
}

fn is_own_insertion(node: &Node, user: &TrackUser) -> bool {
    mark_of(&marks::marks_of(node), MARK_INSERTION).is_some_and(|m| same_author(m, user))
}

/// Runs `f` on a transaction; rolls back on error.
fn transact(state: &mut EditState, f: impl FnOnce(&mut Tr<'_>) -> Result<Selection, EditError>) -> Result<Change, EditError> {
    let before = state.sel;
    let mut tr = Tr::new(&mut state.doc);
    match f(&mut tr) {
        Ok(after) => {
            let (steps, inverses) = (std::mem::take(&mut tr.steps), std::mem::take(&mut tr.inverses));
            drop(tr);
            state.sel = after;
            Ok(Change { steps, inverses, before, after, merge: true })
        }
        Err(e) => {
            tr.rollback();
            Err(e)
        }
    }
}

/// Undoes an applied change on the state (its inverses, last first).
fn revert(state: &mut EditState, change: &Change) {
    let mut tr = Tr::new(&mut state.doc);
    for inv in change.inverses.iter().rev() {
        if tr.step(inv.clone()).is_err() {
            break;
        }
    }
    state.sel = change.before;
}

fn join(acc: &mut Option<Change>, next: Change) {
    match acc {
        Some(a) => {
            a.steps.extend(next.steps);
            a.inverses.extend(next.inverses);
            a.after = next.after;
        }
        None => *acc = Some(next),
    }
}

/// Puts a deletion mark on the inline content of `from..to` (`markDeleted`): the author's own
/// pending insertions really go, already-deleted content is left as it is. `Ok(None)` when nothing
/// would survive - the deletion is then a real one, which the caller performs. The caret goes to
/// `from` (`TextSelection.near(map(rec.from, -1), -1)`).
pub fn mark_deleted(state: &mut EditState, from: usize, to: usize, stamp: &Stamp) -> Result<Option<Change>, EditError> {
    if from >= to {
        return Ok(None);
    }
    let blocks = commands::blocks_in(&state.doc, from, to);
    let user = &stamp.user;
    // Anything kept?
    let mut kept = false;
    for (path, _, s, e) in &blocks {
        let block = node_at(&state.doc, path).ok_or(EditError::NoSuchNode)?;
        let c = inline::cut(block.children(), *s, *e)?;
        if c.removed.iter().any(|n| !is_own_insertion(n, user)) {
            kept = true;
            break;
        }
    }
    if !kept {
        return Ok(None);
    }
    // The revision to continue: a deletion by the same author right before or after the range.
    let reuse = {
        let first = blocks.first().and_then(|(p, _, s, _)| node_at(&state.doc, p).map(|b| around(b.children(), *s).0));
        let last = blocks.last().and_then(|(p, _, _, e)| node_at(&state.doc, p).map(|b| around(b.children(), *e).1));
        reuse_of(&[first.flatten(), last.flatten()], MARK_DELETION, user)
    };
    let del = change_mark(MARK_DELETION, stamp, reuse);
    let change = transact(state, |tr| {
        for (path, _, s, e) in &blocks {
            let block = node_at(tr.doc, path).ok_or(EditError::NoSuchNode)?.clone();
            let c = inline::cut(block.children(), *s, *e)?;
            let mut nodes = c.prefix;
            for n in c.removed {
                if is_own_insertion(&n, user) {
                    continue;
                }
                let ms = marks::marks_of(&n);
                if marks::has_mark(&ms, MARK_DELETION) {
                    nodes.push(n);
                } else {
                    nodes.push(with_marks(&n, &marks::add_mark(&ms, del.clone())));
                }
            }
            nodes.extend(c.suffix);
            tr.replace_node(path, structure::with_children(&block, welded(nodes)))?;
        }
        Ok(Selection::caret(from))
    })?;
    Ok(Some(change))
}

/// Puts an insertion mark on the inline content of `from..to`, continuing the same author's
/// insertion just before `from` (`reuseAt(tr.doc, from)`: the node after is the new content).
pub fn mark_inserted(state: &mut EditState, from: usize, to: usize, stamp: &Stamp) -> Result<Option<Change>, EditError> {
    if from >= to {
        return Ok(None);
    }
    let blocks = commands::blocks_in(&state.doc, from, to);
    let reuse = blocks
        .first()
        .and_then(|(p, _, s, _)| node_at(&state.doc, p).and_then(|b| around(b.children(), *s).0))
        .and_then(|n| reuse_of(&[Some(n)], MARK_INSERTION, &stamp.user));
    let ins = change_mark(MARK_INSERTION, stamp, reuse);
    let sel = state.sel;
    let mut any = false;
    let change = transact(state, |tr| {
        for (path, _, s, e) in &blocks {
            if s >= e {
                continue;
            }
            let block = node_at(tr.doc, path).ok_or(EditError::NoSuchNode)?.clone();
            let c = inline::cut(block.children(), *s, *e)?;
            let mut nodes = c.prefix;
            for n in c.removed {
                nodes.push(with_marks(&n, &marks::add_mark(&marks::marks_of(&n), ins.clone())));
            }
            nodes.extend(c.suffix);
            tr.replace_node(path, structure::with_children(&block, welded(nodes)))?;
            any = true;
        }
        Ok(sel)
    })?;
    Ok(any.then_some(change))
}

/// A command inserting at the selection (typing, paste, an inline node), tracked: a selection is
/// first deleted as a revision (its struck text stays before the new content), then `f` runs at the
/// caret and what it inserted gets the insertion mark.
pub fn tracked_insert(state: &mut EditState, stamp: &Stamp, f: impl FnOnce(&mut EditState) -> Result<Change, EditError>) -> Result<Change, EditError> {
    let mut acc: Option<Change> = None;
    if !state.sel.is_empty() {
        let (from, to) = (state.sel.from(), state.sel.to());
        // The typed text takes the marks of the selection's start, read before it is struck.
        let typing = commands::typing_marks(&state.doc, from, state.stored_marks.as_ref());
        let size_before = pm::node_size(&state.doc);
        match mark_deleted(state, from, to, stamp)? {
            Some(ch) => {
                // The new content goes after the struck text (`fwd.map(rep.from, -1)` puts the
                // deletion before the insertion).
                let end = to - (size_before - pm::node_size(&state.doc));
                state.sel = Selection::caret(end);
                join(&mut acc, Change { after: state.sel, ..ch });
                let kept: Vec<Value> = typing.into_iter().filter(|m| !matches!(m.get("type").and_then(Value::as_str), Some(MARK_INSERTION | MARK_DELETION))).collect();
                state.stored_marks = Some(kept);
            }
            None => {
                let ch = commands::delete_backward(state, false, None)?;
                join(&mut acc, ch);
            }
        }
    }
    let at = state.sel.head;
    let inserted = match f(state) {
        Ok(ch) => ch,
        Err(e) => {
            if let Some(a) = &acc {
                revert(state, a);
            }
            return Err(e);
        }
    };
    let end = inserted.after.head;
    join(&mut acc, inserted);
    if end > at {
        let after = state.sel;
        if let Some(ch) = mark_inserted(state, at, end, stamp)? {
            join(&mut acc, Change { after, ..ch });
        }
    }
    acc.ok_or(EditError::NotApplicable)
}

/// The range Backspace / Delete removes inside one textblock (the selection, else a character or
/// a word), or `None` when the key works on the block structure (a join, an indent, a list item,
/// an atom block) - untracked on the web too, as such a step removes no inline content.
pub fn delete_range(state: &EditState, backward: bool, word: bool, word_pos: Option<usize>) -> Option<(usize, usize)> {
    if !state.sel.is_empty() {
        return Some((state.sel.from(), state.sel.to()));
    }
    let pos = state.sel.head;
    let (path, start) = textblock_at(&state.doc, pos)?;
    let len = inline::content_len(node_at(&state.doc, &path)?.children());
    if backward {
        if pos <= start {
            return None;
        }
        let target = if word { word_pos.filter(|&p| p >= start && p < pos).unwrap_or(start) } else { commands::prev_char_pos(&state.doc, pos).unwrap_or(start).max(start) };
        Some((target, pos))
    } else {
        if pos >= start + len {
            return None;
        }
        let target = if word { word_pos.filter(|&p| p > pos && p <= start + len).unwrap_or(start + len) } else { commands::next_char_pos(&state.doc, pos).unwrap_or(start + len).min(start + len) };
        Some((pos, target))
    }
}

/// Backspace (`backward`) / Delete while tracking. QUIRK (web): the caret goes before the struck
/// text in both directions, so Delete does not move past it.
pub fn tracked_delete(state: &mut EditState, stamp: &Stamp, backward: bool, word: bool, word_pos: Option<usize>) -> Result<Change, EditError> {
    if let Some((from, to)) = delete_range(state, backward, word, word_pos) {
        if let Some(ch) = mark_deleted(state, from, to, stamp)? {
            return Ok(ch);
        }
    }
    if backward {
        commands::delete_backward(state, word, word_pos)
    } else {
        commands::delete_forward(state, word, word_pos)
    }
}

/// `collectRuns` + `documentChanges`: every revision in document order, runs sharing an id merged.
pub fn document_changes(doc: &Node) -> Vec<TrackedChange> {
    struct Run {
        kind: ChangeKind,
        mark: Value,
        from: usize,
        to: usize,
        text: String,
    }
    let mut runs: Vec<Run> = Vec::new();
    // The run still growing per `kind:id`.
    let mut open: std::collections::HashMap<(ChangeKind, String), usize> = std::collections::HashMap::new();
    for (path, start) in textblocks(doc) {
        let Some(block) = node_at(doc, &path) else { continue };
        let mut pos = start;
        for child in block.children() {
            let size = inline::inline_len(child);
            let ms = marks::marks_of(child);
            for kind in [ChangeKind::Insertion, ChangeKind::Deletion] {
                let Some(m) = mark_of(&ms, kind.mark()) else { continue };
                let id = attr(m, "id");
                let text = if pm::is_text(child) { child.text().unwrap_or_default() } else { String::new() };
                let key = (kind, id);
                match open.get(&key).copied() {
                    Some(i) if runs[i].to == pos => {
                        runs[i].to = pos + size;
                        runs[i].text.push_str(&text);
                    }
                    _ => {
                        runs.push(Run { kind, mark: m.clone(), from: pos, to: pos + size, text });
                        open.insert(key, runs.len() - 1);
                    }
                }
            }
            pos += size;
        }
    }
    runs.sort_by_key(|r| r.from);
    let mut out: Vec<TrackedChange> = Vec::new();
    let mut by_key: std::collections::HashMap<(ChangeKind, String), usize> = std::collections::HashMap::new();
    for run in runs {
        let id = attr(&run.mark, "id");
        if !id.is_empty() {
            if let Some(&i) = by_key.get(&(run.kind, id.clone())) {
                let prev = &mut out[i];
                if run.from > prev.to {
                    prev.text.push('\n');
                }
                prev.text.push_str(&run.text);
                prev.to = run.to;
                continue;
            }
        }
        let change = TrackedChange {
            id: id.clone(),
            kind: run.kind,
            author: attr(&run.mark, "author"),
            author_id: attr(&run.mark, "authorId"),
            date: attr(&run.mark, "date"),
            from: run.from,
            to: run.to,
            text: run.text,
        };
        if !id.is_empty() {
            by_key.insert((run.kind, id), out.len());
        }
        out.push(change);
    }
    out
}

/// `hasChanges`.
pub fn has_changes(doc: &Node) -> bool {
    textblocks(doc).iter().any(|(p, _)| {
        node_at(doc, p).is_some_and(|b| {
            b.children().iter().any(|c| {
                let ms = marks::marks_of(c);
                marks::has_mark(&ms, MARK_INSERTION) || marks::has_mark(&ms, MARK_DELETION)
            })
        })
    })
}

/// `nextChange`: the first revision starting after `pos`, wrapping around.
pub fn next_change(doc: &Node, pos: usize) -> Option<TrackedChange> {
    let all = document_changes(doc);
    all.iter().find(|c| c.from > pos).cloned().or_else(|| all.first().cloned())
}

/// `prevChange`: the last revision starting before `pos`, wrapping around.
pub fn prev_change(doc: &Node, pos: usize) -> Option<TrackedChange> {
    let all = document_changes(doc);
    all.iter().rev().find(|c| c.from < pos).cloned().or_else(|| all.last().cloned())
}

/// `changesAt`: the revisions covering a position (a text can be inserted AND deleted).
pub fn changes_at(doc: &Node, pos: usize) -> Vec<TrackedChange> {
    document_changes(doc).into_iter().filter(|c| pos >= c.from && pos <= c.to).collect()
}

/// `removesText`: accepting a deletion or rejecting an insertion removes the text.
fn removes_text(accept: bool, kind: ChangeKind) -> bool {
    accept == (kind == ChangeKind::Deletion)
}

/// `applyDecision`: accepts (`accept`) or rejects the revisions whose id is in `ids` (all when
/// `None`). An inline node a decision removes goes; otherwise the decided marks come off.
pub fn decide(state: &mut EditState, ids: Option<&[String]>, accept: bool) -> Result<Change, EditError> {
    let wanted = |m: &Value| ids.is_none_or(|ids| ids.iter().any(|i| *i == attr(m, "id")));
    // The selection follows the text: positions after a removed node move back by its size.
    let map = |p: usize, removed: &[(usize, usize)]| -> usize {
        let mut out = p;
        for &(a, b) in removed {
            if b <= p {
                out -= b - a;
            } else if a < p {
                out -= p - a;
            }
        }
        out
    };
    let mut removed: Vec<(usize, usize)> = Vec::new();
    let mut edits: Vec<(Vec<usize>, Node)> = Vec::new();
    for (path, start) in textblocks(&state.doc) {
        let Some(block) = node_at(&state.doc, &path) else { continue };
        let mut changed = false;
        let mut nodes: Vec<Node> = Vec::with_capacity(block.children().len());
        let mut pos = start;
        for child in block.children() {
            let size = inline::inline_len(child);
            let at = pos;
            pos += size;
            let ms = marks::marks_of(child);
            let decided: Vec<ChangeKind> = [ChangeKind::Insertion, ChangeKind::Deletion].into_iter().filter(|k| mark_of(&ms, k.mark()).is_some_and(wanted)).collect();
            if decided.is_empty() {
                nodes.push(child.clone());
                continue;
            }
            changed = true;
            if decided.iter().any(|k| removes_text(accept, *k)) {
                removed.push((at, at + size));
                continue;
            }
            let mut kept = ms;
            for k in &decided {
                kept = marks::remove_mark(&kept, k.mark());
            }
            nodes.push(with_marks(child, &kept));
        }
        if changed {
            edits.push((path.clone(), structure::with_children(block, welded(nodes))));
        }
    }
    if edits.is_empty() {
        return Err(EditError::NotApplicable);
    }
    let sel = Selection::new(map(state.sel.anchor, &removed), map(state.sel.head, &removed));
    transact(state, |tr| {
        for (path, node) in edits {
            tr.replace_node(&path, node)?;
        }
        Ok(sel)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(json: &str, sel: Selection) -> EditState {
        EditState { doc: serde_json::from_str(json).expect("doc"), sel, stored_marks: None }
    }

    fn para(text: &str) -> String {
        format!(r#"{{"type":"doc","content":[{{"type":"paragraph","content":[{{"type":"text","text":"{text}"}}]}}]}}"#)
    }

    fn user(id: &str, name: &str) -> TrackUser {
        TrackUser { id: id.into(), name: name.into() }
    }

    fn stamp(u: &TrackUser, id: &str) -> Stamp {
        Stamp { user: u.clone(), date: "2026-10-07T09:00:00Z".into(), id: id.into(), insertion_id: format!("{id}+") }
    }

    /// `(text, kinds)` of each inline node of the first paragraph.
    fn runs(s: &EditState) -> Vec<(String, Vec<String>)> {
        s.doc.children()[0]
            .children()
            .iter()
            .map(|n| {
                let kinds = marks::marks_of(n)
                    .iter()
                    .filter_map(|m| {
                        let t = m.get("type").and_then(Value::as_str)?;
                        (t == MARK_INSERTION || t == MARK_DELETION).then(|| format!("{t}:{}", attr(m, "id")))
                    })
                    .collect();
                (n.text().unwrap_or_default(), kinds)
            })
            .collect()
    }

    fn type_text(s: &mut EditState, st: &Stamp, text: &str) {
        let t = text.to_string();
        tracked_insert(s, st, move |s| commands::insert_text(s, &t)).expect("typed");
    }

    #[test]
    fn typed_text_is_one_insertion_and_erasing_it_leaves_no_ghost() {
        let me = user("u1", "Lina");
        let mut s = state(&para("Bonjour"), Selection::caret(8));
        type_text(&mut s, &stamp(&me, "a"), " à");
        type_text(&mut s, &stamp(&me, "b"), " tous");
        // Consecutive keystrokes continue the revision before them: one id.
        assert_eq!(runs(&s), [("Bonjour".to_string(), vec![]), (" à tous".to_string(), vec!["insertion:a+".to_string()])]);
        let all = document_changes(&s.doc);
        assert_eq!(all.len(), 1);
        assert_eq!((all[0].kind, all[0].author.as_str(), all[0].text.as_str()), (ChangeKind::Insertion, "Lina", " à tous"));
        // Backspace over one's own pending insertion really removes it.
        for _ in 0..5 {
            tracked_delete(&mut s, &stamp(&me, "x"), true, false, None).expect("erased");
        }
        assert_eq!(runs(&s), [("Bonjour".to_string(), vec![]), (" à".to_string(), vec!["insertion:a+".to_string()])]);
    }

    #[test]
    fn deleting_someone_elses_text_strikes_it_and_backspace_grows_one_revision() {
        let me = user("u1", "Lina");
        let mut s = state(&para("Bonjour"), Selection::caret(8));
        tracked_delete(&mut s, &stamp(&me, "d1"), true, false, None).expect("struck");
        // The caret goes before the struck character.
        assert_eq!(s.sel, Selection::caret(7));
        tracked_delete(&mut s, &stamp(&me, "d2"), true, false, None).expect("struck");
        assert_eq!(runs(&s), [("Bonjo".to_string(), vec![]), ("ur".to_string(), vec!["deletion:d1".to_string()])]);
        assert_eq!(s.sel, Selection::caret(6));
        // Delete (forward) also leaves the caret before what it struck - the web's quirk.
        let mut f = state(&para("abc"), Selection::caret(1));
        tracked_delete(&mut f, &stamp(&me, "d3"), false, false, None).expect("struck");
        assert_eq!(f.sel, Selection::caret(1));
        assert_eq!(runs(&f)[0], ("a".to_string(), vec!["deletion:d3".to_string()]));
    }

    #[test]
    fn typing_over_a_selection_strikes_it_then_inserts_after_it() {
        let me = user("u1", "Lina");
        let mut s = state(&para("le chat"), Selection::new(4, 8));
        type_text(&mut s, &stamp(&me, "r1"), "chien");
        assert_eq!(
            runs(&s),
            [("le ".to_string(), vec![]), ("chat".to_string(), vec!["deletion:r1".to_string()]), ("chien".to_string(), vec!["insertion:r1+".to_string()])]
        );
        assert_eq!(s.sel, Selection::caret(13));
    }

    #[test]
    fn another_authors_insertion_is_struck_not_removed() {
        let other = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"ab","marks":[{"type":"insertion","attrs":{"author":"Paul","authorId":"u2","date":"2026-01-01T00:00:00Z","id":"p1"}}]}]}]}"#;
        let mut s = state(other, Selection::caret(3));
        tracked_delete(&mut s, &stamp(&user("u1", "Lina"), "d"), true, false, None).expect("struck");
        assert_eq!(runs(&s)[1], ("b".to_string(), vec!["insertion:p1".to_string(), "deletion:d".to_string()]));
    }

    #[test]
    fn a_paragraph_join_is_a_real_deletion() {
        let two = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"a"}]},{"type":"paragraph","content":[{"type":"text","text":"b"}]}]}"#;
        let mut s = state(two, Selection::caret(4));
        tracked_delete(&mut s, &stamp(&user("u1", "Lina"), "d"), true, false, None).expect("joined");
        assert_eq!(s.doc.children().len(), 1);
        assert!(!has_changes(&s.doc));
    }

    #[test]
    fn accept_and_reject_follow_the_web_rules() {
        let me = user("u1", "Lina");
        let mut s = state(&para("le chat"), Selection::new(4, 8));
        type_text(&mut s, &stamp(&me, "r1"), "chien");
        let mut accepted = s.clone();
        decide(&mut accepted, None, true).expect("accepted");
        assert_eq!(runs(&accepted), [("le chien".to_string(), vec![])]);
        let mut rejected = s.clone();
        decide(&mut rejected, None, false).expect("rejected");
        assert_eq!(runs(&rejected), [("le chat".to_string(), vec![])]);
        // One revision by id: typing over a selection made two (the web's fresh `uid()` each).
        let mut one = s;
        let ids = ["r1".to_string()];
        decide(&mut one, Some(&ids), true).expect("one");
        assert_eq!(runs(&one), [("le ".to_string(), vec![]), ("chien".to_string(), vec!["insertion:r1+".to_string()])]);
        let ids = ["r1+".to_string()];
        decide(&mut one, Some(&ids), true).expect("the other");
        assert!(!has_changes(&one.doc));
        assert_eq!(decide(&mut one, None, true).err(), Some(EditError::NotApplicable));
    }

    #[test]
    fn runs_of_one_id_across_paragraphs_are_one_change() {
        let m = r#"[{"type":"deletion","attrs":{"author":"Lina","authorId":"u1","date":"2026-10-07T09:00:00Z","id":"d"}}]"#;
        let doc = format!(r#"{{"type":"doc","content":[{{"type":"paragraph","content":[{{"type":"text","text":"un","marks":{m}}}]}},{{"type":"paragraph","content":[{{"type":"text","text":"deux","marks":{m}}}]}}]}}"#);
        let s = state(&doc, Selection::caret(1));
        let all = document_changes(&s.doc);
        assert_eq!(all.len(), 1);
        assert_eq!((all[0].from, all[0].to, all[0].text.as_str()), (1, 9, "un\ndeux"));
        assert_eq!(next_change(&s.doc, 5).map(|c| c.from), Some(1), "wraps around");
        assert_eq!(prev_change(&s.doc, 0).map(|c| c.from), Some(1));
    }

    #[test]
    fn stamps_look_like_the_webs() {
        assert_eq!(iso_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_utc(1_791_364_245_000), "2026-10-07T09:10:45Z");
        let id = fresh_id(1_791_364_245_000);
        assert_eq!(id.len(), 12);
        assert_ne!(id, fresh_id(1_791_364_245_000));
        // `authorColor('')` is the first hue; the palette index is the 31-hash modulo 10.
        assert_eq!(parse_iso_utc("2026-10-07T09:10:45Z"), Some(1_791_364_245_000));
        assert_eq!(parse_iso_utc("2026-10-07T09:10:45.123Z"), Some(1_791_364_245_000));
        assert_eq!(parse_iso_utc("hier"), None);
        let t = 1_791_364_245_000;
        assert_eq!(age("2026-10-07T09:10:45Z", t + 30_000), Some(Age::Now));
        assert_eq!(age("2026-10-07T09:10:45Z", t + 5 * 60_000), Some(Age::Minutes(5)));
        assert_eq!(age("2026-10-07T09:10:45Z", t + 3 * 3_600_000), Some(Age::Hours(3)));
        assert_eq!(age("2026-10-07T09:10:45Z", t + 30 * 3_600_000), Some(Age::Date(t)));
        assert_eq!(author_color(""), "#1a73e8");
        assert_eq!(author_color("a"), AUTHOR_PALETTE[97 % 10]);
    }
}
