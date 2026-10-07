//! Undo and redo of element edits — the web's model (`PresentationEditorPage.tsx:4976-5010`): a stack of
//! per-slide element snapshots, depth 80; a change pushes the slide's PREVIOUS elements unless the last change
//! was on the same slide less than 500 ms ago (a sliding window: a gesture or a burst of typing is one step).
//! Slide-level operations (add, delete, reorder, hide, background, transition, notes) are not in it, like the
//! web. The time is given by the caller.

use crate::model::Element;

/// The web's coalescing window.
pub const COALESCE_MS: u64 = 500;
/// The web's depth.
pub const DEPTH: usize = 80;

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub slide: String,
    pub elements: Vec<Element>,
}

#[derive(Debug, Clone, Default)]
pub struct History {
    undo: Vec<Entry>,
    redo: Vec<Entry>,
    last: Option<(String, u64)>,
}

impl History {
    /// A change of `slide` at `now_ms`, whose elements were `before`.
    pub fn record(&mut self, slide: &str, before: &[Element], now_ms: u64) {
        let coalesce = matches!(&self.last, Some((s, t)) if s == slide && now_ms.saturating_sub(*t) < COALESCE_MS);
        if !coalesce {
            self.undo.push(Entry { slide: slide.to_string(), elements: before.to_vec() });
            if self.undo.len() > DEPTH {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
        self.last = Some((slide.to_string(), now_ms));
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Pops the step to undo; `current` gives the elements of a slide now (pushed to redo).
    pub fn undo(&mut self, current: impl FnOnce(&str) -> Vec<Element>) -> Option<Entry> {
        let e = self.undo.pop()?;
        self.redo.push(Entry { slide: e.slide.clone(), elements: current(&e.slide) });
        self.last = None;
        Some(e)
    }

    pub fn redo(&mut self, current: impl FnOnce(&str) -> Vec<Element>) -> Option<Entry> {
        let e = self.redo.pop()?;
        self.undo.push(Entry { slide: e.slide.clone(), elements: current(&e.slide) });
        self.last = None;
        Some(e)
    }

    /// Forgets the steps of a slide that no longer exists.
    pub fn forget(&mut self, slide: &str) {
        self.undo.retain(|e| e.slide != slide);
        self.redo.retain(|e| e.slide != slide);
    }

    /// Ends the current coalescing window (a new gesture starts a new step even within 500 ms).
    pub fn break_group(&mut self) {
        self.last = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn els(x: f64) -> Vec<Element> {
        vec![Element::from_value(json!({ "id": "a", "x": x })).unwrap_or_default()]
    }

    #[test]
    fn changes_within_500_ms_are_one_step_and_the_window_slides() {
        let mut h = History::default();
        h.record("s", &els(0.0), 1000);
        h.record("s", &els(0.1), 1400);
        h.record("s", &els(0.2), 1800); // 400 ms after the previous: still the same step
        h.record("s", &els(0.3), 2400);
        let e = h.undo(|_| els(0.4)).expect("undo");
        assert_eq!(e.elements, els(0.3));
        let e = h.undo(|_| els(0.3)).expect("undo");
        assert_eq!(e.elements, els(0.0));
        assert!(!h.can_undo());
        let r = h.redo(|_| els(0.0)).expect("redo");
        assert_eq!(r.elements, els(0.3));
    }

    #[test]
    fn another_slide_starts_a_step_and_the_depth_is_bounded() {
        let mut h = History::default();
        h.record("a", &els(0.0), 0);
        h.record("b", &els(0.0), 10);
        assert_eq!(h.undo(|_| Vec::new()).map(|e| e.slide), Some("b".to_string()));
        let mut h = History::default();
        for i in 0..100u64 {
            h.record("s", &els(i as f64), i * 1000);
        }
        let mut n = 0;
        while h.undo(|_| Vec::new()).is_some() {
            n += 1;
        }
        assert_eq!(n, DEPTH);
    }
}
