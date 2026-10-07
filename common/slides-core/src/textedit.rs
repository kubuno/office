//! Editing the text of a text box or a shape in place — what the browser's `contentEditable` did for the
//! web editor (`PresentationEditorPage.tsx:2664-2766`): the caret and the selection over paragraphs of runs,
//! typing, Enter, deletion, the marks of the selection, and the caret geometry over the laid-out text
//! ([`crate::render::TextBox`]).
//!
//! Positions are (paragraph, character offset). The text is written back to the element as the web writes it
//! (`parasToDoc`), on every change.
//!
//! Web behaviour kept: a format command with a collapsed caret applies to the whole box (element-level keys);
//! paragraph attributes (lists, alignment, indents, line height) apply to every paragraph. Web bug not ported:
//! the web's editor dropped the paragraph attributes on every keystroke (its HTML did not carry them).

use crate::render::{PlacedLine, TextBox};
use crate::richtext::{merge_runs, Measure, Para, Run};

/// A position: paragraph and character offset in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Pos {
    pub para: usize,
    pub offset: usize,
}

impl Pos {
    pub fn new(para: usize, offset: usize) -> Pos {
        Pos { para, offset }
    }
}

/// A mark command.
#[derive(Debug, Clone, PartialEq)]
pub enum Mark {
    Bold,
    Italic,
    Underline,
    Strike,
    Superscript,
    Subscript,
    Color(String),
    Highlight(String),
    Size(f64),
}

/// The editing state of one element's text.
#[derive(Debug, Clone, PartialEq)]
pub struct TextEdit {
    pub id: String,
    pub paras: Vec<Para>,
    pub anchor: Pos,
    pub head: Pos,
    /// The x the caret keeps on ↑/↓.
    pub goal_x: Option<f64>,
}

fn char_split(s: &str, at: usize) -> (String, String) {
    let i = s.char_indices().nth(at).map(|(b, _)| b).unwrap_or(s.len());
    (s[..i].to_string(), s[i..].to_string())
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

impl TextEdit {
    /// Opens the element's text with the caret at the end (the web's `r.collapse(false)`).
    pub fn open(id: &str, mut paras: Vec<Para>) -> TextEdit {
        if paras.is_empty() {
            paras.push(Para::default());
        }
        let last = paras.len() - 1;
        let end = Pos::new(last, paras[last].len());
        TextEdit { id: id.to_string(), paras, anchor: end, head: end, goal_x: None }
    }

    pub fn doc(&self) -> serde_json::Value {
        crate::richtext::paras_to_doc(&self.paras)
    }

    pub fn is_collapsed(&self) -> bool {
        self.anchor == self.head
    }

    pub fn range(&self) -> (Pos, Pos) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    fn clamp(&self, p: Pos) -> Pos {
        let para = p.para.min(self.paras.len().saturating_sub(1));
        Pos::new(para, p.offset.min(self.paras[para].len()))
    }

    pub fn set_caret(&mut self, p: Pos, extend: bool) {
        let p = self.clamp(p);
        self.head = p;
        if !extend {
            self.anchor = p;
        }
    }

    pub fn select_all(&mut self) {
        self.anchor = Pos::new(0, 0);
        let last = self.paras.len() - 1;
        self.head = Pos::new(last, self.paras[last].len());
    }

    /// The paragraph's text as characters.
    fn chars(&self, para: usize) -> Vec<char> {
        self.paras[para].text().chars().collect()
    }

    /// Double click: the word (or the run of spaces) at `p`.
    pub fn select_word(&mut self, p: Pos) {
        let p = self.clamp(p);
        let cs = self.chars(p.para);
        if cs.is_empty() {
            self.set_caret(p, false);
            return;
        }
        let at = p.offset.min(cs.len() - 1);
        let kind = |c: char| if is_word(c) { 0 } else if c.is_whitespace() { 1 } else { 2 };
        let k = kind(cs[at]);
        let mut s = at;
        while s > 0 && kind(cs[s - 1]) == k {
            s -= 1;
        }
        let mut e = at;
        while e < cs.len() && kind(cs[e]) == k {
            e += 1;
        }
        self.anchor = Pos::new(p.para, s);
        self.head = Pos::new(p.para, e);
    }

    /// Triple click: the paragraph.
    pub fn select_para(&mut self, p: Pos) {
        let p = self.clamp(p);
        self.anchor = Pos::new(p.para, 0);
        self.head = Pos::new(p.para, self.paras[p.para].len());
    }

    /// The selected text (paragraphs joined by `\n`).
    pub fn selected_text(&self) -> String {
        let (a, b) = self.range();
        let mut out = String::new();
        for p in a.para..=b.para {
            let cs = self.chars(p);
            let s = if p == a.para { a.offset } else { 0 };
            let e = if p == b.para { b.offset } else { cs.len() };
            out.extend(cs[s.min(cs.len())..e.min(cs.len())].iter());
            if p != b.para {
                out.push('\n');
            }
        }
        out
    }

    /// Splits paragraph `p`'s runs at `offset`: (before, after).
    fn split_runs(runs: &[Run], offset: usize) -> (Vec<Run>, Vec<Run>) {
        let (mut before, mut after) = (Vec::new(), Vec::new());
        let mut at = 0;
        for r in runs {
            let n = r.text.chars().count();
            if at + n <= offset {
                before.push(r.clone());
            } else if at >= offset {
                after.push(r.clone());
            } else {
                let (l, rr) = char_split(&r.text, offset - at);
                before.push(r.with_text(l));
                after.push(r.with_text(rr));
            }
            at += n;
        }
        (before, after)
    }

    /// The marks typed text takes at `p`: the run before the caret, else the one after.
    fn marks_at(&self, p: Pos) -> Run {
        let runs = &self.paras[p.para].runs;
        let mut at = 0;
        let mut found: Option<&Run> = None;
        for r in runs {
            let n = r.text.chars().count();
            if p.offset > at && p.offset <= at + n {
                found = Some(r);
                break;
            }
            at += n;
        }
        found.or_else(|| runs.iter().find(|r| !r.text.is_empty())).map(|r| r.with_text(String::new())).unwrap_or_default()
    }

    /// Deletes the selection; returns whether anything was deleted.
    pub fn delete_selection(&mut self) -> bool {
        if self.is_collapsed() {
            return false;
        }
        let (a, b) = self.range();
        let (head_runs, _) = Self::split_runs(&self.paras[a.para].runs, a.offset);
        let (_, tail_runs) = Self::split_runs(&self.paras[b.para].runs, b.offset);
        let attrs = self.paras[a.para].attrs.clone();
        let mut runs = head_runs;
        runs.extend(tail_runs);
        self.paras.splice(a.para..=b.para, [Para { runs: merge_runs(&runs), attrs }]);
        self.anchor = a;
        self.head = a;
        true
    }

    /// Types `text` over the selection (`\n` splits paragraphs).
    pub fn insert_text(&mut self, text: &str) {
        let start = if self.is_collapsed() { self.head } else { self.range().0 };
        let marks = self.marks_at(start);
        self.delete_selection();
        let mut p = self.head;
        for (i, piece) in text.split('\n').enumerate() {
            if i > 0 {
                self.split_at(p);
                p = Pos::new(p.para + 1, 0);
            }
            if piece.is_empty() {
                continue;
            }
            let (mut before, after) = Self::split_runs(&self.paras[p.para].runs, p.offset);
            before.push(marks.with_text(piece.to_string()));
            before.extend(after);
            self.paras[p.para].runs = merge_runs(&before);
            p.offset += piece.chars().count();
        }
        self.anchor = p;
        self.head = p;
        self.goal_x = None;
    }

    fn split_at(&mut self, p: Pos) {
        let (before, after) = Self::split_runs(&self.paras[p.para].runs, p.offset);
        let attrs = self.paras[p.para].attrs.clone();
        self.paras[p.para].runs = merge_runs(&before);
        self.paras.insert(p.para + 1, Para { runs: merge_runs(&after), attrs });
    }

    /// Enter: a new paragraph (with the same attributes).
    pub fn enter(&mut self) {
        self.insert_text("\n");
    }

    fn word_left(&self, p: Pos) -> Pos {
        if p.offset == 0 {
            return if p.para > 0 { Pos::new(p.para - 1, self.paras[p.para - 1].len()) } else { p };
        }
        let cs = self.chars(p.para);
        let mut i = p.offset;
        while i > 0 && !is_word(cs[i - 1]) {
            i -= 1;
        }
        while i > 0 && is_word(cs[i - 1]) {
            i -= 1;
        }
        Pos::new(p.para, i)
    }

    fn word_right(&self, p: Pos) -> Pos {
        let cs = self.chars(p.para);
        if p.offset >= cs.len() {
            return if p.para + 1 < self.paras.len() { Pos::new(p.para + 1, 0) } else { p };
        }
        let mut i = p.offset;
        while i < cs.len() && is_word(cs[i]) {
            i += 1;
        }
        while i < cs.len() && !is_word(cs[i]) {
            i += 1;
        }
        Pos::new(p.para, i)
    }

    fn char_left(&self, p: Pos) -> Pos {
        if p.offset > 0 {
            Pos::new(p.para, p.offset - 1)
        } else if p.para > 0 {
            Pos::new(p.para - 1, self.paras[p.para - 1].len())
        } else {
            p
        }
    }

    fn char_right(&self, p: Pos) -> Pos {
        if p.offset < self.paras[p.para].len() {
            Pos::new(p.para, p.offset + 1)
        } else if p.para + 1 < self.paras.len() {
            Pos::new(p.para + 1, 0)
        } else {
            p
        }
    }

    /// Backspace (Ctrl: a word).
    pub fn delete_backward(&mut self, word: bool) {
        if !self.delete_selection() {
            let to = if word { self.word_left(self.head) } else { self.char_left(self.head) };
            self.anchor = to;
            self.delete_selection();
        }
        self.goal_x = None;
    }

    /// Delete (Ctrl: a word).
    pub fn delete_forward(&mut self, word: bool) {
        if !self.delete_selection() {
            let to = if word { self.word_right(self.head) } else { self.char_right(self.head) };
            self.anchor = to;
            self.delete_selection();
        }
        self.goal_x = None;
    }

    /// ← / → (Ctrl: by word); without Shift a selection collapses to its side.
    pub fn move_horizontal(&mut self, right: bool, word: bool, extend: bool) {
        self.goal_x = None;
        if !extend && !self.is_collapsed() && !word {
            let (a, b) = self.range();
            let p = if right { b } else { a };
            self.set_caret(p, false);
            return;
        }
        let p = match (right, word) {
            (true, true) => self.word_right(self.head),
            (true, false) => self.char_right(self.head),
            (false, true) => self.word_left(self.head),
            (false, false) => self.char_left(self.head),
        };
        self.set_caret(p, extend);
    }

    /// Ctrl+Home / Ctrl+End.
    pub fn move_doc(&mut self, end: bool, extend: bool) {
        let p = if end {
            let last = self.paras.len() - 1;
            Pos::new(last, self.paras[last].len())
        } else {
            Pos::new(0, 0)
        };
        self.set_caret(p, extend);
        self.goal_x = None;
    }

    /// Toggles or sets a mark over the selection (`execCommand`): a toggle removes the mark when every
    /// selected character has it, else adds it. Returns false with a collapsed selection (the caller then
    /// applies the format to the whole box, like the web).
    pub fn apply_mark(&mut self, mark: &Mark) -> bool {
        if self.is_collapsed() {
            return false;
        }
        let (a, b) = self.range();
        let has = |r: &Run| match mark {
            Mark::Bold => r.b,
            Mark::Italic => r.i,
            Mark::Underline => r.u,
            Mark::Strike => r.s,
            Mark::Superscript => r.sup,
            Mark::Subscript => r.sub,
            _ => false,
        };
        // Does every selected character carry the mark?
        let mut all = true;
        for p in a.para..=b.para {
            let s = if p == a.para { a.offset } else { 0 };
            let e = if p == b.para { b.offset } else { self.paras[p].len() };
            let (_, rest) = Self::split_runs(&self.paras[p].runs, s);
            let (mid, _) = Self::split_runs(&rest, e.saturating_sub(s));
            if mid.iter().any(|r| !r.text.is_empty() && !has(r)) {
                all = false;
            }
        }
        for p in a.para..=b.para {
            let s = if p == a.para { a.offset } else { 0 };
            let e = if p == b.para { b.offset } else { self.paras[p].len() };
            let (before, rest) = Self::split_runs(&self.paras[p].runs, s);
            let (mut mid, after) = Self::split_runs(&rest, e.saturating_sub(s));
            for r in &mut mid {
                match mark {
                    Mark::Bold => r.b = !all,
                    Mark::Italic => r.i = !all,
                    Mark::Underline => r.u = !all,
                    Mark::Strike => r.s = !all,
                    Mark::Superscript => {
                        r.sup = !all;
                        if r.sup {
                            r.sub = false;
                        }
                    }
                    Mark::Subscript => {
                        r.sub = !all;
                        if r.sub {
                            r.sup = false;
                        }
                    }
                    Mark::Color(c) => r.color = Some(c.clone()),
                    Mark::Highlight(c) => r.hl = Some(c.clone()),
                    Mark::Size(s) => r.size = Some(*s),
                }
            }
            let mut runs = before;
            runs.extend(mid);
            runs.extend(after);
            self.paras[p].runs = merge_runs(&runs);
        }
        true
    }

    /// `removeFormat` over the selection.
    pub fn clear_marks(&mut self) -> bool {
        if self.is_collapsed() {
            return false;
        }
        let (a, b) = self.range();
        for p in a.para..=b.para {
            let s = if p == a.para { a.offset } else { 0 };
            let e = if p == b.para { b.offset } else { self.paras[p].len() };
            let (before, rest) = Self::split_runs(&self.paras[p].runs, s);
            let (mid, after) = Self::split_runs(&rest, e.saturating_sub(s));
            let mut runs = before;
            runs.extend(mid.into_iter().map(|r| Run::plain(&r.text)));
            runs.extend(after);
            self.paras[p].runs = merge_runs(&runs);
        }
        true
    }

    /// The marks common to the selection (or at the caret), for the ribbon's checked states.
    pub fn marks_state(&self) -> Run {
        if self.is_collapsed() {
            return self.marks_at(self.head);
        }
        let (a, b) = self.range();
        let mut first: Option<Run> = None;
        let mut out = Run::default();
        for p in a.para..=b.para {
            let s = if p == a.para { a.offset } else { 0 };
            let e = if p == b.para { b.offset } else { self.paras[p].len() };
            let (_, rest) = Self::split_runs(&self.paras[p].runs, s);
            let (mid, _) = Self::split_runs(&rest, e.saturating_sub(s));
            for r in mid.iter().filter(|r| !r.text.is_empty()) {
                match &first {
                    None => {
                        out = r.with_text(String::new());
                        first = Some(r.clone());
                    }
                    Some(_) => {
                        out.b &= r.b;
                        out.i &= r.i;
                        out.u &= r.u;
                        out.s &= r.s;
                        out.sup &= r.sup;
                        out.sub &= r.sub;
                        if out.color != r.color {
                            out.color = None;
                        }
                        if out.size != r.size {
                            out.size = None;
                        }
                    }
                }
            }
        }
        out
    }

    // ── Geometry over the laid-out text ─────────────────────────────────────

    /// The line holding `p` (the next line at a wrap boundary, the paragraph's last line at its end).
    fn line_of<'b>(&self, tb: &'b TextBox, p: Pos) -> Option<(usize, &'b PlacedLine)> {
        let mut best: Option<(usize, &PlacedLine)> = None;
        for (i, l) in tb.lines.iter().enumerate() {
            if l.line.para != p.para {
                continue;
            }
            if p.offset >= l.line.start {
                best = Some((i, l));
            }
        }
        best.or_else(|| tb.lines.iter().enumerate().find(|(_, l)| l.line.para == p.para))
    }

    /// The x of offset `offset` on line `l`.
    fn x_on_line(l: &PlacedLine, offset: usize, m: &dyn Measure, ls: f64) -> f64 {
        let mut x = l.pen_x;
        for ps in &l.segs {
            let seg = &ps.seg;
            if offset <= seg.start {
                return if offset == seg.start { ps.x } else { x };
            }
            if offset <= seg.end {
                let n = offset - seg.start;
                let prefix: String = seg.text.chars().take(n).collect();
                return ps.x + m.width(&prefix, &seg.style, ls);
            }
            x = ps.x + seg.width;
        }
        x
    }

    /// The caret: `(x, top, height)` in the text box's frame.
    pub fn caret_rect(&self, tb: &TextBox, m: &dyn Measure) -> Option<(f64, f64, f64)> {
        let (_, l) = self.line_of(tb, self.head)?;
        Some((Self::x_on_line(l, self.head.offset, m, tb.letter_spacing), l.top, l.line.height))
    }

    /// The selection's rectangles (one per line it touches).
    pub fn selection_rects(&self, tb: &TextBox, m: &dyn Measure) -> Vec<(f64, f64, f64, f64)> {
        if self.is_collapsed() {
            return Vec::new();
        }
        let (a, b) = self.range();
        let mut out = Vec::new();
        for (i, l) in tb.lines.iter().enumerate() {
            let p = l.line.para;
            if p < a.para || p > b.para {
                continue;
            }
            let next_same = tb.lines.get(i + 1).is_some_and(|n| n.line.para == p);
            let (ls, le) = (l.line.start, if next_same { l.line.end } else { usize::MAX });
            let s = if p == a.para { a.offset.max(ls) } else { ls };
            let e = if p == b.para { b.offset.min(le) } else { le };
            if s > e || (p == a.para && a.offset > le) || (p == b.para && b.offset < ls) {
                continue;
            }
            let x0 = Self::x_on_line(l, s, m, tb.letter_spacing);
            let mut x1 = Self::x_on_line(l, e.min(l.line.end.max(s)), m, tb.letter_spacing);
            // A selection running past the line's end shows a little of the newline.
            if p < b.para && !next_same {
                x1 += l.line.height * 0.25;
            }
            if x1 > x0 {
                out.push((x0, l.top, x1 - x0, l.line.height));
            }
        }
        out
    }

    /// The position nearest `(x, y)` in the text box's frame.
    pub fn pos_at(&self, tb: &TextBox, x: f64, y: f64, m: &dyn Measure) -> Pos {
        let Some(line) = tb.lines.iter().find(|l| y < l.top + l.line.height).or_else(|| tb.lines.last()) else { return Pos::default() };
        let line = if y < tb.lines.first().map(|l| l.top).unwrap_or(0.0) { &tb.lines[0] } else { line };
        let para = line.line.para;
        let mut best = (f64::INFINITY, line.line.start);
        let last_of_para = !tb.lines.iter().any(|l| l.line.para == para && l.line.start > line.line.start);
        let end = if last_of_para { self.paras.get(para).map(Para::len).unwrap_or(line.line.end) } else { line.line.end.saturating_sub(1).max(line.line.start) };
        for off in line.line.start..=end {
            let cx = Self::x_on_line(line, off, m, tb.letter_spacing);
            let d = (cx - x).abs();
            if d < best.0 {
                best = (d, off);
            }
        }
        Pos::new(para, best.1)
    }

    /// ↑ / ↓: the next line's position at the goal x.
    pub fn move_vertical(&mut self, tb: &TextBox, down: bool, extend: bool, m: &dyn Measure) {
        let Some((i, l)) = self.line_of(tb, self.head) else { return };
        let gx = self.goal_x.unwrap_or_else(|| Self::x_on_line(l, self.head.offset, m, tb.letter_spacing));
        let target = if down { tb.lines.get(i + 1) } else { i.checked_sub(1).and_then(|j| tb.lines.get(j)) };
        let p = match target {
            Some(t) => self.pos_at(tb, gx, t.top + t.line.height / 2.0, m),
            None if down => {
                let last = self.paras.len() - 1;
                Pos::new(last, self.paras[last].len())
            }
            None => Pos::new(0, 0),
        };
        self.set_caret(p, extend);
        self.goal_x = Some(gx);
    }

    /// Home / End: the visual line's start or end.
    pub fn move_line_edge(&mut self, tb: &TextBox, end: bool, extend: bool) {
        let Some((i, l)) = self.line_of(tb, self.head) else { return };
        let next_same = tb.lines.get(i + 1).is_some_and(|n| n.line.para == l.line.para);
        let off = if !end {
            l.line.start
        } else if next_same {
            l.line.end.saturating_sub(1).max(l.line.start)
        } else {
            self.paras[l.line.para].len()
        };
        self.set_caret(Pos::new(l.line.para, off), extend);
        self.goal_x = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::richtext::{doc_to_paras, FixedMeasure};
    use serde_json::json;

    fn edit(text: &str) -> TextEdit {
        TextEdit::open("t", doc_to_paras(Some(&crate::richtext::doc_from_string(text))))
    }

    #[test]
    fn typing_enter_and_backspace() {
        let mut e = edit("Hello");
        e.insert_text(" world");
        e.enter();
        e.insert_text("x");
        assert_eq!(crate::richtext::paras_to_plain(&e.paras), "Hello world\nx");
        e.delete_backward(false);
        e.delete_backward(false);
        assert_eq!(crate::richtext::paras_to_plain(&e.paras), "Hello world");
        e.delete_backward(true);
        assert_eq!(crate::richtext::paras_to_plain(&e.paras), "Hello ");
    }

    #[test]
    fn marks_toggle_over_the_selection_and_typing_inherits() {
        let mut e = edit("abcdef");
        e.set_caret(Pos::new(0, 1), false);
        e.set_caret(Pos::new(0, 3), true);
        assert!(e.apply_mark(&Mark::Bold));
        assert_eq!(e.doc(), json!({ "type": "doc", "content": [{ "type": "paragraph", "content": [
            { "type": "text", "text": "a" }, { "type": "text", "text": "bc", "marks": [{ "type": "bold" }] }, { "type": "text", "text": "def" } ] }] }));
        e.set_caret(Pos::new(0, 3), false);
        e.insert_text("Z");
        assert!(e.paras[0].runs[1].b && e.paras[0].runs[1].text == "bcZ");
        e.select_all();
        assert!(e.apply_mark(&Mark::Bold), "not all bold: add");
        assert!(e.paras[0].runs.iter().all(|r| r.b));
        assert!(e.apply_mark(&Mark::Bold), "all bold: remove");
        assert!(e.paras[0].runs.iter().all(|r| !r.b));
        e.set_caret(Pos::new(0, 0), false);
        assert!(!e.apply_mark(&Mark::Italic), "collapsed: the box's own style");
    }

    #[test]
    fn deleting_across_paragraphs_joins_them() {
        let mut e = edit("one\ntwo\nthree");
        e.set_caret(Pos::new(0, 2), false);
        e.set_caret(Pos::new(2, 2), true);
        assert_eq!(e.selected_text(), "e\ntwo\nth");
        e.delete_selection();
        assert_eq!(crate::richtext::paras_to_plain(&e.paras), "onree");
        assert_eq!(e.head, Pos::new(0, 2));
    }

    #[test]
    fn words_and_paragraphs_select() {
        let mut e = edit("hello big world");
        e.select_word(Pos::new(0, 7));
        assert_eq!(e.selected_text(), "big");
        e.select_para(Pos::new(0, 0));
        assert_eq!(e.selected_text(), "hello big world");
    }

    #[test]
    fn the_caret_follows_the_layout() {
        let el = crate::model::Element::from_value(json!({ "type": "text", "x": 0, "y": 0, "w": 0.1, "h": 0.5, "padding": 0, "fontSize": 20,
            "content": crate::richtext::doc_from_string("aaa bbb") })).unwrap_or_default();
        let e = TextEdit::open("t", doc_to_paras(el.get("content")));
        // 96 px wide, 10 px a character: "aaa bbb" (70 px) fits on one line.
        let tb = crate::render::layout_text_box_with(&el, e.paras.clone(), 0.0, 0.0, 96.0, 270.0, &crate::model::Theme::default(), true, 1.0, &FixedMeasure, true).expect("box");
        let (x, top, h) = e.caret_rect(&tb, &FixedMeasure).expect("caret");
        assert_eq!((x, top, h), (70.0, 0.0, 26.0));
        let p = e.pos_at(&tb, 33.0, 5.0, &FixedMeasure);
        assert_eq!(p, Pos::new(0, 3));
        let mut e2 = e.clone();
        e2.move_line_edge(&tb, false, false);
        assert_eq!(e2.head, Pos::new(0, 0));
    }
}
