//! The slideshow sequence — the web's `PresenterMode` (`PresentationEditorPage.tsx:4285-4515`) without its
//! window: which slide shows, which animated elements are revealed, the reveal in progress, the incoming
//! slide's transition, the black screen, the counter, autoplay and loop, the timer. Pure: the caller gives
//! the time (ms) and draws [`Show::frame`].

use std::collections::HashSet;

use serde_json::Value;

use crate::model::{as_f64, Deck, SlideData};

/// The animated elements of a slide in reveal order (`zIndex` ascending), those whose `anim.type` is set and
/// not `none`.
pub fn anim_ids(data: &SlideData) -> Vec<String> {
    let mut els: Vec<&crate::model::Element> = data
        .elements
        .iter()
        .filter(|e| e.get("anim").filter(|a| a.is_object()).and_then(|a| a.get("type")).and_then(Value::as_str).is_some_and(|t| t != "none"))
        .collect();
    els.sort_by(|a, b| a.z_index().total_cmp(&b.z_index()));
    els.into_iter().map(|e| e.id().to_string()).collect()
}

/// An element reveal in progress.
#[derive(Debug, Clone, PartialEq)]
pub struct Reveal {
    pub index: usize,
    pub id: String,
    /// When it starts (the press plus the element's delay) and how long it lasts.
    pub t0: f64,
    pub duration: f64,
}

/// What to draw now.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// Index in the deck of the slide shown.
    pub slide: usize,
    pub hidden: HashSet<String>,
    pub animating: Option<(String, f64)>,
    /// The incoming slide's transition and its eased progress (`None` once done or for `none`).
    pub transition: Option<(String, f64)>,
}

/// CSS `ease` (cubic-bezier(0.25, 0.1, 0.25, 1)) at linear progress `x`.
pub fn css_ease(x: f64) -> f64 {
    cubic_bezier(0.25, 0.1, 0.25, 1.0, x)
}

fn cubic_bezier(x1: f64, y1: f64, x2: f64, y2: f64, x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    let bez = |t: f64, a: f64, b: f64| 3.0 * (1.0 - t).powi(2) * t * a + 3.0 * (1.0 - t) * t * t * b + t * t * t;
    // Solve bez_x(t) = x by bisection (monotonic for these control points).
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        if bez(mid, x1, x2) < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    bez((lo + hi) / 2.0, y1, y2)
}

/// The slideshow's state.
#[derive(Debug, Clone)]
pub struct Show {
    /// Deck indices of the visible slides.
    pub visible: Vec<usize>,
    pub current: usize,
    pub step: usize,
    pub black: bool,
    pub show_num: bool,
    pub autoplay: bool,
    pub looping: bool,
    pub reveal: Option<Reveal>,
    /// When the current slide came in (its transition starts there).
    pub entered_at: f64,
    pub started_at: f64,
    autoplay_since: f64,
    pub closed: bool,
}

impl Show {
    /// Starts at deck slide `start` (or the first visible one after it).
    pub fn new(deck: &Deck, start: usize, now: f64) -> Show {
        let visible: Vec<usize> = deck.slides.iter().enumerate().filter(|(_, s)| !s.hidden).map(|(i, _)| i).collect();
        let current = visible.iter().position(|&i| i == start).unwrap_or(0);
        Show { visible, current, step: 0, black: false, show_num: false, autoplay: false, looping: false, reveal: None, entered_at: now, started_at: now, autoplay_since: now, closed: false }
    }

    fn data<'d>(&self, deck: &'d Deck) -> Option<&'d SlideData> {
        let i = *self.visible.get(self.current)?;
        deck.slides.get(i).map(|s| &s.data)
    }

    fn anim_ids(&self, deck: &Deck) -> Vec<String> {
        self.data(deck).map(anim_ids).unwrap_or_default()
    }

    fn go(&mut self, index: usize, now: f64) {
        self.current = index;
        self.step = 0;
        self.reveal = None;
        self.entered_at = now;
    }

    /// Next: reveals the next animated element, else the next slide, else (loop) the first. Ignored while a
    /// reveal plays.
    pub fn next(&mut self, deck: &Deck, now: f64) {
        if self.reveal.is_some() {
            return;
        }
        let ids = self.anim_ids(deck);
        if self.step < ids.len() {
            let id = ids[self.step].clone();
            let el = self.data(deck).and_then(|d| d.element(&id));
            let anim = el.and_then(|e| e.get("anim"));
            let duration = anim.and_then(|a| as_f64(a.get("duration"))).unwrap_or(450.0);
            let delay = anim.and_then(|a| as_f64(a.get("delay"))).unwrap_or(0.0);
            self.reveal = Some(Reveal { index: self.step, id, t0: now + delay, duration });
        } else if self.current + 1 < self.visible.len() {
            self.go(self.current + 1, now);
        } else if self.looping {
            self.go(0, now);
        }
    }

    /// Previous: cancels a reveal, hides the last revealed element, else the previous slide.
    pub fn prev(&mut self, now: f64) {
        self.reveal = None;
        if self.step > 0 {
            self.step -= 1;
        } else if self.current > 0 {
            self.go(self.current - 1, now);
        }
    }

    pub fn home(&mut self, now: f64) {
        self.go(0, now);
    }

    pub fn end(&mut self, now: f64) {
        self.go(self.visible.len().saturating_sub(1), now);
    }

    pub fn toggle_black(&mut self) {
        self.black = !self.black;
    }

    pub fn toggle_num(&mut self) {
        self.show_num = !self.show_num;
    }

    pub fn toggle_autoplay(&mut self, now: f64) {
        self.autoplay = !self.autoplay;
        self.autoplay_since = now;
    }

    pub fn toggle_loop(&mut self) {
        self.looping = !self.looping;
    }

    /// Advances time: ends a finished reveal, fires autoplay every 3.5 s. Returns true while something moves
    /// (the caller keeps repainting).
    pub fn tick(&mut self, deck: &Deck, now: f64) -> bool {
        if let Some(r) = &self.reveal {
            if now - r.t0 >= r.duration {
                self.step = r.index + 1;
                self.reveal = None;
            }
        }
        if self.autoplay && now - self.autoplay_since >= 3500.0 {
            self.autoplay_since += 3500.0;
            self.next(deck, now);
        }
        self.reveal.is_some() || self.transition_progress(deck, now).is_some() || self.autoplay
    }

    fn transition_progress(&self, deck: &Deck, now: f64) -> Option<(String, f64)> {
        let (kind, dur) = self.data(deck)?.transition();
        const KNOWN: [&str; 11] = ["fade", "slideL", "slideR", "slideU", "zoom", "flip", "pushU", "wipeR", "cover", "split", "rotate"];
        if !KNOWN.contains(&kind.as_str()) {
            return None;
        }
        let dur = dur.unwrap_or(500.0);
        let x = if dur <= 0.0 { 1.0 } else { (now - self.entered_at) / dur };
        (x < 1.0).then(|| (kind, css_ease(x.max(0.0))))
    }

    /// What to draw at `now`.
    pub fn frame(&self, deck: &Deck, now: f64) -> Option<Frame> {
        let slide = *self.visible.get(self.current)?;
        let ids = self.anim_ids(deck);
        let (hidden, animating) = match &self.reveal {
            Some(r) => (ids.iter().skip(r.index).cloned().collect(), Some((r.id.clone(), ((now - r.t0) / r.duration).clamp(0.0, 1.0)))),
            None => (ids.iter().skip(self.step).cloned().collect(), None),
        };
        Some(Frame { slide, hidden, animating, transition: self.transition_progress(deck, now) })
    }

    /// `mm:ss` since the show started.
    pub fn elapsed(&self, now: f64) -> String {
        let s = ((now - self.started_at) / 1000.0).max(0.0) as u64;
        format!("{:02}:{:02}", s / 60, s % 60)
    }

    /// The control bar's « n / N ».
    pub fn counter(&self) -> String {
        format!("{} / {}", self.current + 1, self.visible.len())
    }

    pub fn can_prev(&self) -> bool {
        !(self.current == 0 && self.step == 0)
    }

    pub fn can_next(&self, deck: &Deck) -> bool {
        !(self.current + 1 >= self.visible.len() && self.step >= self.anim_ids(deck).len())
    }

    /// The current slide's notes.
    pub fn notes<'d>(&self, deck: &'d Deck) -> &'d str {
        self.data(deck).map(|d| d.notes()).unwrap_or("")
    }
}

/// The incoming slide's look at eased progress `p` (the web's `kbp_*` keyframes): opacity, translation
/// (fractions of the slide), scale, rotation (degrees), horizontal squeeze for `flip` (cos of the
/// `rotateY` angle: no perspective), and the visible horizontal band for the clip-path wipes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransitionLook {
    pub opacity: f64,
    pub dx: f64,
    pub dy: f64,
    pub scale: f64,
    pub rotate: f64,
    pub squeeze_x: f64,
    /// Visible part, fractions of the width: `[left, right]`.
    pub clip_x: (f64, f64),
}

pub fn transition_look(kind: &str, p: f64) -> TransitionLook {
    let mut l = TransitionLook { opacity: 1.0, dx: 0.0, dy: 0.0, scale: 1.0, rotate: 0.0, squeeze_x: 1.0, clip_x: (0.0, 1.0) };
    let q = 1.0 - p;
    match kind {
        "fade" => l.opacity = p,
        "slideL" | "cover" => l.dx = q,
        "slideR" => l.dx = -q,
        "slideU" | "pushU" => l.dy = q,
        "zoom" => {
            l.scale = 0.7 + 0.3 * p;
            l.opacity = p;
        }
        "flip" => {
            l.squeeze_x = (q * std::f64::consts::FRAC_PI_2).cos().max(0.0);
            l.opacity = p;
        }
        "wipeR" => l.clip_x = (0.0, p),
        "split" => l.clip_x = (0.5 - 0.5 * p, 0.5 + 0.5 * p),
        "rotate" => {
            l.rotate = -12.0 * q;
            l.scale = 0.85 + 0.15 * p;
            l.opacity = p;
        }
        _ => {}
    }
    l
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn deck() -> Deck {
        let meta = json!({ "slides": [{ "id": "a", "position": 0 }, { "id": "h", "position": 1, "is_hidden": true }, { "id": "b", "position": 2 }] });
        Deck::from_server(&meta, json!({ "version": 1, "slides": {
            "a": { "elements": [
                { "id": "x", "type": "shape", "zIndex": 2, "anim": { "type": "fade", "duration": 1000, "delay": 100 } },
                { "id": "y", "type": "shape", "zIndex": 1, "anim": { "type": "zoom" } },
                { "id": "z", "type": "shape", "anim": { "type": "none" } } ] },
            "b": { "elements": [], "transition": { "type": "fade", "duration": 500 } } } }))
    }

    #[test]
    fn hidden_slides_are_skipped_and_elements_reveal_in_z_order() {
        let d = deck();
        let mut s = Show::new(&d, 0, 0.0);
        assert_eq!(s.visible, [0, 2]);
        let f = s.frame(&d, 0.0).expect("frame");
        assert_eq!(f.hidden, ["x".to_string(), "y".to_string()].into());
        s.next(&d, 0.0);
        assert_eq!(s.reveal.as_ref().map(|r| r.id.as_str()), Some("y"));
        s.next(&d, 10.0); // ignored while revealing
        assert_eq!(s.step, 0);
        s.tick(&d, 450.0);
        assert_eq!(s.step, 1);
        s.next(&d, 500.0);
        let f = s.frame(&d, 600.0).expect("frame");
        assert_eq!(f.animating, Some(("x".to_string(), 0.0)), "the delay holds it at 0");
        s.tick(&d, 1700.0);
        s.next(&d, 1700.0);
        assert_eq!((s.current, s.step), (1, 0));
        assert!(s.frame(&d, 1800.0).and_then(|f| f.transition).is_some());
        assert!(s.frame(&d, 2300.0).and_then(|f| f.transition).is_none());
        s.next(&d, 3000.0);
        assert_eq!(s.current, 1, "no loop: stays on the last");
        s.toggle_loop();
        s.next(&d, 3000.0);
        assert_eq!(s.current, 0);
    }

    #[test]
    fn prev_hides_the_last_revealed_then_goes_back() {
        let d = deck();
        let mut s = Show::new(&d, 2, 0.0);
        assert_eq!(s.current, 1);
        s.prev(0.0);
        assert_eq!((s.current, s.step), (0, 0));
        assert!(!s.can_prev());
        assert_eq!(s.elapsed(65_500.0), "01:05");
    }

    #[test]
    fn ease_and_looks() {
        assert!((css_ease(0.5) - 0.8024).abs() < 1e-3);
        assert_eq!(transition_look("split", 0.5).clip_x, (0.25, 0.75));
        assert_eq!(transition_look("slideR", 0.0).dx, -1.0);
    }
}
