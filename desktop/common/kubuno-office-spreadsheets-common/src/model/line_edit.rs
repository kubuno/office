//! A one-line text editor: the cell editor and the formula bar (`CellEditor`, `FormulaInput`), with the
//! formula helpers of the web - F4 cycles `$` (`applyF4`), brackets and quotes pair (`autoPair`), the
//! autocomplete list of functions and names (`refreshAssist`), point mode (`refExpecting`: a click on the
//! grid inserts a reference where an operand is expected).
//!
//! Positions are counted in `char`s; the formula helpers of the core count UTF-16 units and are converted.

use kubuno_office_sheets_core::catalog;
use kubuno_office_sheets_core::refs;

/// The editor's text, caret and selection.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LineEdit {
    chars: Vec<char>,
    /// The caret, in chars.
    pub caret: usize,
    /// The other end of the selection, when there is one.
    pub anchor: Option<usize>,
    /// The span (start, end) of the reference last inserted by point mode, replaced by the next click.
    pub pointed: Option<(usize, usize)>,
}

/// One autocomplete suggestion.
#[derive(Clone, Debug, PartialEq)]
pub struct Suggestion {
    pub name: String,
    pub syntax: String,
    /// A defined name rather than a function.
    pub is_name: bool,
}

fn utf16_of(chars: &[char], i: usize) -> usize {
    chars[..i.min(chars.len())].iter().map(|c| c.len_utf16()).sum()
}

fn char_of(chars: &[char], u: usize) -> usize {
    let mut n = 0;
    for (i, c) in chars.iter().enumerate() {
        if n >= u {
            return i;
        }
        n += c.len_utf16();
    }
    chars.len()
}

impl LineEdit {
    pub fn new(text: &str) -> Self {
        let chars: Vec<char> = text.chars().collect();
        let caret = chars.len();
        Self { chars, caret, anchor: None, pointed: None }
    }

    pub fn text(&self) -> String {
        self.chars.iter().collect()
    }

    pub fn len(&self) -> usize {
        self.chars.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }

    /// The selection as (start, end), empty when there is none.
    pub fn selection(&self) -> (usize, usize) {
        match self.anchor {
            Some(a) if a != self.caret => (a.min(self.caret), a.max(self.caret)),
            _ => (self.caret, self.caret),
        }
    }

    pub fn selected_text(&self) -> String {
        let (s, e) = self.selection();
        self.chars[s..e].iter().collect()
    }

    pub fn set_text(&mut self, text: &str) {
        *self = LineEdit::new(text);
    }

    /// The caret in UTF-16 units (for the core's formula helpers).
    pub fn caret16(&self) -> usize {
        utf16_of(&self.chars, self.caret)
    }

    fn replace_selection(&mut self, s: &str) {
        let (a, b) = self.selection();
        let ins: Vec<char> = s.chars().collect();
        let n = ins.len();
        self.chars.splice(a..b, ins);
        self.caret = a + n;
        self.anchor = None;
        self.pointed = None;
    }

    /// Types `s` (replacing the selection). In a formula, `(` and `"` pair and `)` steps over (`autoPair`).
    pub fn insert(&mut self, s: &str) {
        let text = self.text();
        let (a, b) = self.selection();
        if a == b && text.starts_with('=') && s.chars().count() == 1 {
            if let Some((next, caret16)) = refs::auto_pair(&text, self.caret16(), s) {
                self.chars = next.chars().collect();
                self.caret = char_of(&self.chars, caret16);
                self.anchor = None;
                self.pointed = None;
                return;
            }
        }
        self.replace_selection(s);
    }

    pub fn backspace(&mut self, word: bool) {
        let (a, b) = self.selection();
        if a != b {
            self.replace_selection("");
            return;
        }
        if self.caret == 0 {
            return;
        }
        let to = if word { self.word_left() } else { self.caret - 1 };
        self.chars.drain(to..self.caret);
        self.caret = to;
        self.pointed = None;
    }

    pub fn delete(&mut self, word: bool) {
        let (a, b) = self.selection();
        if a != b {
            self.replace_selection("");
            return;
        }
        if self.caret >= self.chars.len() {
            return;
        }
        let to = if word { self.word_right() } else { self.caret + 1 };
        self.chars.drain(self.caret..to);
        self.pointed = None;
    }

    fn word_left(&self) -> usize {
        let mut i = self.caret;
        while i > 0 && self.chars[i - 1].is_whitespace() {
            i -= 1;
        }
        while i > 0 && !self.chars[i - 1].is_whitespace() {
            i -= 1;
        }
        i
    }

    fn word_right(&self) -> usize {
        let mut i = self.caret;
        while i < self.chars.len() && !self.chars[i].is_whitespace() {
            i += 1;
        }
        while i < self.chars.len() && self.chars[i].is_whitespace() {
            i += 1;
        }
        i
    }

    fn move_to(&mut self, pos: usize, extend: bool) {
        if extend {
            if self.anchor.is_none() {
                self.anchor = Some(self.caret);
            }
        } else {
            self.anchor = None;
        }
        self.caret = pos.min(self.chars.len());
    }

    pub fn left(&mut self, word: bool, extend: bool) {
        let (a, b) = self.selection();
        if a != b && !extend {
            self.move_to(a, false);
            return;
        }
        let p = if word { self.word_left() } else { self.caret.saturating_sub(1) };
        self.move_to(p, extend);
    }

    pub fn right(&mut self, word: bool, extend: bool) {
        let (a, b) = self.selection();
        if a != b && !extend {
            self.move_to(b, false);
            return;
        }
        let p = if word { self.word_right() } else { (self.caret + 1).min(self.chars.len()) };
        self.move_to(p, extend);
    }

    pub fn home(&mut self, extend: bool) {
        self.move_to(0, extend);
    }

    pub fn end(&mut self, extend: bool) {
        self.move_to(self.chars.len(), extend);
    }

    pub fn select_all(&mut self) {
        self.anchor = Some(0);
        self.caret = self.chars.len();
    }

    /// Places the caret at char `i` (a click).
    pub fn set_caret(&mut self, i: usize, extend: bool) {
        self.move_to(i, extend);
    }

    /// F4: cycles the `$` of the reference at the caret (`applyF4`).
    pub fn f4(&mut self) -> bool {
        let text = self.text();
        match refs::apply_f4(&text, self.caret16()) {
            Some((next, caret16)) => {
                self.chars = next.chars().collect();
                self.caret = char_of(&self.chars, caret16);
                self.anchor = None;
                true
            }
            None => false,
        }
    }

    /// `refExpecting`: a formula whose caret follows `= + - * / ^ & < > % ( , :` (spaces ignored).
    pub fn expects_ref(&self) -> bool {
        if self.chars.first() != Some(&'=') {
            return false;
        }
        let before = self.chars[..self.caret].iter().rev().find(|c| !c.is_whitespace());
        matches!(before, Some('=' | '+' | '-' | '*' | '/' | '^' | '&' | '<' | '>' | '%' | '(' | ',' | ':' | ';'))
    }

    /// Point mode: inserts `r` at the caret, or replaces the reference pointed last when the caret is at
    /// its end (a second click moves it).
    pub fn point(&mut self, r: &str) -> bool {
        if let Some((s, e)) = self.pointed.filter(|(_, e)| *e == self.caret) {
            let ins: Vec<char> = r.chars().collect();
            let n = ins.len();
            self.chars.splice(s..e, ins);
            self.caret = s + n;
            self.pointed = Some((s, s + n));
            self.anchor = None;
            return true;
        }
        if !self.expects_ref() {
            return false;
        }
        let s = self.caret;
        self.replace_selection(r);
        self.pointed = Some((s, self.caret));
        true
    }

    /// The autocomplete list for the word before the caret (`refreshAssist`): defined names first (up to 6),
    /// then the functions ranked by `rankFunctions`; up to 16.
    pub fn suggestions(&self, names: &[String]) -> Vec<Suggestion> {
        let text = self.text();
        if !text.starts_with('=') {
            return Vec::new();
        }
        let token = refs::name_token_at(&text, self.caret16());
        if token.is_empty() {
            return Vec::new();
        }
        let up = token.to_uppercase();
        let mut out: Vec<Suggestion> = names.iter().filter(|n| n.to_uppercase().starts_with(&up)).take(6).map(|n| Suggestion { name: n.clone(), syntax: String::new(), is_name: true }).collect();
        let cat = catalog::function_catalog();
        for f in refs::rank_functions(&up, cat, 14, |f| f.name.as_str()) {
            if out.len() >= 16 {
                break;
            }
            out.push(Suggestion { name: f.name.clone(), syntax: f.syntax.clone(), is_name: false });
        }
        out
    }

    /// Applies a suggestion: the word before the caret becomes `NAME(` (or the bare name).
    pub fn accept(&mut self, s: &Suggestion) {
        let text = self.text();
        let token = refs::name_token_at(&text, self.caret16());
        let n = token.chars().count();
        let start = self.caret.saturating_sub(n);
        let ins = if s.is_name { s.name.clone() } else { format!("{}(", s.name) };
        let v: Vec<char> = ins.chars().collect();
        let len = v.len();
        self.chars.splice(start..self.caret, v);
        self.caret = start + len;
        self.anchor = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_and_moving() {
        let mut e = LineEdit::new("abc");
        e.left(false, false);
        e.insert("X");
        assert_eq!(e.text(), "abXc");
        e.home(true);
        e.backspace(false);
        assert_eq!(e.text(), "c");
        e.end(false);
        e.insert(" dé");
        e.backspace(true);
        assert_eq!(e.text(), "c ");
    }

    #[test]
    fn formula_helpers() {
        let mut e = LineEdit::new("=SUM");
        e.insert("(");
        assert_eq!(e.text(), "=SUM()");
        assert_eq!(e.caret, 5);
        assert!(e.expects_ref());
        assert!(e.point("A1"));
        assert!(e.point("B2"));
        assert_eq!(e.text(), "=SUM(B2)");
        assert!(e.f4());
        assert_eq!(e.text(), "=SUM($B$2)");
        let mut e = LineEdit::new("=som");
        let s = e.suggestions(&[]);
        assert!(s.iter().any(|x| x.name == "SOMME" || x.name.starts_with("SUM")) || !s.is_empty());
        let mut e2 = LineEdit::new("=VLO");
        let s = e2.suggestions(&[]);
        assert_eq!(s[0].name, "VLOOKUP");
        e2.accept(&s[0]);
        assert_eq!(e2.text(), "=VLOOKUP(");
        e.set_text("x");
        assert!(e.suggestions(&[]).is_empty());
    }
}
