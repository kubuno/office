//! A one-line text being edited in place (a task's name, its duration, its completion, its predecessors —
//! the inputs of the web's `TaskRow`): the text, the caret and the selection, and the keys that move them.

/// The edited text (chars, so the caret never splits a character).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LineEdit {
    chars: Vec<char>,
    /// Caret position, in chars.
    pub caret: usize,
    /// The other end of the selection (`None`: no selection).
    pub anchor: Option<usize>,
}

impl LineEdit {
    /// `text` with everything selected (what a click into a web `<input>` that selects on focus shows; the
    /// first key replaces it).
    pub fn new(text: &str) -> Self {
        let chars: Vec<char> = text.chars().collect();
        let n = chars.len();
        Self { chars, caret: n, anchor: Some(0) }
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

    /// The selected range `(start, end)`, empty when there is none.
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

    fn delete_selection(&mut self) -> bool {
        let (s, e) = self.selection();
        self.anchor = None;
        if s == e {
            return false;
        }
        self.chars.drain(s..e);
        self.caret = s;
        true
    }

    /// Types `s` over the selection.
    pub fn insert(&mut self, s: &str) {
        self.delete_selection();
        for c in s.chars().filter(|c| !c.is_control()) {
            self.chars.insert(self.caret, c);
            self.caret += 1;
        }
    }

    pub fn backspace(&mut self) {
        if !self.delete_selection() && self.caret > 0 {
            self.caret -= 1;
            self.chars.remove(self.caret);
        }
    }

    pub fn delete(&mut self) {
        if !self.delete_selection() && self.caret < self.chars.len() {
            self.chars.remove(self.caret);
        }
    }

    fn moved(&mut self, to: usize, extend: bool) {
        if extend {
            if self.anchor.is_none() {
                self.anchor = Some(self.caret);
            }
        } else {
            self.anchor = None;
        }
        self.caret = to.min(self.chars.len());
    }

    pub fn left(&mut self, extend: bool) {
        let (s, e) = self.selection();
        if !extend && s != e {
            self.moved(s, false);
        } else {
            self.moved(self.caret.saturating_sub(1), extend);
        }
    }

    pub fn right(&mut self, extend: bool) {
        let (s, e) = self.selection();
        if !extend && s != e {
            self.moved(e, false);
        } else {
            self.moved(self.caret + 1, extend);
        }
    }

    pub fn home(&mut self, extend: bool) {
        self.moved(0, extend);
    }

    pub fn end(&mut self, extend: bool) {
        self.moved(self.chars.len(), extend);
    }

    pub fn select_all(&mut self) {
        self.anchor = Some(0);
        self.caret = self.chars.len();
    }

    /// The first `n` chars (to measure the caret's x).
    pub fn prefix(&self, n: usize) -> String {
        self.chars[..n.min(self.chars.len())].iter().collect()
    }

    /// Cuts the selection: returns it, removed from the text.
    pub fn cut(&mut self) -> String {
        let s = self.selected_text();
        self.delete_selection();
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_replaces_the_selection_and_keys_move() {
        let mut e = LineEdit::new("Dig");
        e.insert("Pour");
        assert_eq!(e.text(), "Pour");
        e.left(false);
        e.left(true);
        assert_eq!(e.selected_text(), "u");
        e.backspace();
        assert_eq!(e.text(), "Por");
        e.home(false);
        e.delete();
        assert_eq!(e.text(), "or");
        e.end(false);
        e.insert("é\u{0007}");
        assert_eq!(e.text(), "oré");
        e.select_all();
        assert_eq!(e.cut(), "oré");
        assert!(e.is_empty());
    }
}
