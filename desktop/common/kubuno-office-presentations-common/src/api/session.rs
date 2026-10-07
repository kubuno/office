//! When to call which route — every rule that protects the user's work lives here.
//!
//! The web saves a slide with `PUT …/slides/:sid` one second after the last change and trusts Yjs for the rest.
//! The desktop has no Yjs (vskubuno docs/PRESENTATIONS-DESKTOP.md §6) and the server has no concurrency token,
//! so it keeps the server's copy it last read or wrote (`base`) and, before writing a slide, **re-reads that
//! slide and compares it with `base`**: equal → write; different → someone else changed it: a conflict only the
//! user resolves (keep mine / take theirs), never an overwrite. A 413 is terminal for that slide (it can never
//! fit); a network failure is retried and the crash journal keeps the work meanwhile.
//!
//! [`save`] makes the server equal the local deck, from `base`, in the order the server needs: deleted slides,
//! created slides (client-minted ids, so a retry is idempotent), contents and meta, then the order. Pure
//! (the network is the [`Server`] trait): the tests run it against an in-memory server.

use std::collections::{HashMap, HashSet};

use kubuno_office_slides_core::model::{Deck, Slide, SlideData};
use serde_json::Value;

use super::client::Error;

/// The routes the save path uses.
pub trait Server {
    /// The slide's content now (`GET …/slides/:sid`, `data`), JSON text.
    fn slide(&mut self, sid: &str) -> Result<String, Error>;
    fn put(&mut self, sid: &str, elements: Option<&str>, notes: Option<&str>) -> Result<(), Error>;
    fn patch(&mut self, sid: &str, background: Option<&str>, transition: Option<&str>, hidden: Option<bool>) -> Result<(), Error>;
    fn create(&mut self, sid: &str, position: usize) -> Result<(), Error>;
    fn delete(&mut self, sid: &str) -> Result<(), Error>;
    fn reorder(&mut self, order: &[String]) -> Result<(), Error>;
}

/// The user's answer for a slide in conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    KeepMine,
    TakeTheirs,
}

/// What a save pass did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    /// Slides the server changed since `base` (nothing written to them).
    pub conflicts: Vec<String>,
    /// Slides refused for their size (413), terminal until they change.
    pub too_large: Vec<String>,
    /// The first error that stopped the pass (network, rights…); what was saved before it stays saved.
    pub failed: Option<Error>,
    /// Number of requests sent.
    pub requests: usize,
}

impl Report {
    pub fn clean(&self) -> bool {
        self.conflicts.is_empty() && self.too_large.is_empty() && self.failed.is_none()
    }
}

/// The same content, as JSON values (the server re-serialises with its own key order).
pub fn same_json(a: &str, b: &str) -> bool {
    match (serde_json::from_str::<Value>(a), serde_json::from_str::<Value>(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

fn key_json(d: &SlideData, key: &str) -> Option<String> {
    d.json_of(key)
}

fn differs(a: Option<String>, b: Option<String>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => !same_json(&a, &b),
        (None, None) => false,
        _ => true,
    }
}

/// Makes the server equal `local` (see the module doc). `base` is updated to what the server holds after each
/// successful write. `resolved` answers the slides the user resolved to « keep mine » (their re-read is skipped
/// once); `skip` are slides not to write (too large, unresolved conflicts).
pub fn save(server: &mut dyn Server, base: &mut Deck, local: &Deck, resolved: &mut HashMap<String, Resolution>, skip: &HashSet<String>) -> Report {
    let mut rep = Report::default();
    macro_rules! try_or_stop {
        ($e:expr) => {{
            rep.requests += 1;
            match $e {
                Ok(v) => v,
                Err(e) => {
                    rep.failed = Some(e);
                    return rep;
                }
            }
        }};
    }
    // 1. Deleted slides.
    let local_ids: HashSet<&str> = local.slides.iter().map(|s| s.id.as_str()).collect();
    let gone: Vec<String> = base.slides.iter().filter(|s| !local_ids.contains(s.id.as_str())).map(|s| s.id.clone()).collect();
    for sid in gone {
        rep.requests += 1;
        match server.delete(&sid) {
            Ok(()) | Err(Error::NotFound) => base.slides.retain(|s| s.id != sid),
            Err(e) => {
                rep.failed = Some(e);
                return rep;
            }
        }
    }
    // 2. Created slides (the server writes its empty slide; the contents follow in 3).
    let mut fresh: HashSet<String> = HashSet::new();
    for (i, s) in local.slides.iter().enumerate() {
        if base.slide(&s.id).is_some() {
            continue;
        }
        try_or_stop!(server.create(&s.id, i));
        let pos = i.min(base.slides.len());
        base.slides.insert(pos, Slide { id: s.id.clone(), hidden: false, data: SlideData::empty() });
        fresh.insert(s.id.clone());
    }
    // 3. Contents, meta and hidden flags.
    for s in &local.slides {
        if skip.contains(&s.id) {
            continue;
        }
        let Some(b) = base.slide(&s.id).cloned() else { continue };
        let (l, bd) = (&s.data, &b.data);
        let elements = differs(Some(l.elements_json()), Some(bd.elements_json()));
        let notes = l.notes() != bd.notes();
        let background = differs(key_json(l, "background"), key_json(bd, "background"));
        let transition = differs(key_json(l, "transition"), key_json(bd, "transition"));
        let hidden = s.hidden != b.hidden;
        if !(elements || notes || background || transition || hidden) {
            continue;
        }
        // The overwrite guard: the slide must still be what we last saw (a new slide is ours).
        if (elements || notes || background || transition) && !fresh.contains(&s.id) {
            let keep = resolved.remove(&s.id) == Some(Resolution::KeepMine);
            if !keep {
                let theirs = try_or_stop!(server.slide(&s.id));
                if !same_json(&theirs, &bd.to_json()) {
                    rep.conflicts.push(s.id.clone());
                    continue;
                }
            }
        }
        if elements || notes {
            let ej = elements.then(|| l.elements_json());
            rep.requests += 1;
            match server.put(&s.id, ej.as_deref(), notes.then(|| l.notes())) {
                Ok(()) => {}
                Err(Error::TooLarge) => {
                    rep.too_large.push(s.id.clone());
                    continue;
                }
                Err(e) => {
                    rep.failed = Some(e);
                    return rep;
                }
            }
        }
        if background || transition || hidden {
            let bg = background.then(|| key_json(l, "background")).flatten();
            let tr = transition.then(|| key_json(l, "transition")).flatten();
            try_or_stop!(server.patch(&s.id, bg.as_deref(), tr.as_deref(), hidden.then_some(s.hidden)));
        }
        if let Some(bs) = base.slide_mut(&s.id) {
            bs.data = s.data.clone();
            bs.hidden = s.hidden;
        }
    }
    // 4. The order.
    let order: Vec<String> = local.slides.iter().map(|s| s.id.clone()).collect();
    let base_order: Vec<String> = base.slides.iter().map(|s| s.id.clone()).collect();
    if order != base_order && base_order.len() == order.len() {
        try_or_stop!(server.reorder(&order));
        base.slides.sort_by_key(|s| order.iter().position(|o| *o == s.id).unwrap_or(usize::MAX));
    }
    rep
}

/// True when the local deck has something the server does not.
pub fn unsaved(base: &Deck, local: &Deck) -> bool {
    if base.slides.len() != local.slides.len() {
        return true;
    }
    base.slides.iter().zip(&local.slides).any(|(b, l)| b.id != l.id || b.hidden != l.hidden || !same_json(&b.data.to_json(), &l.data.to_json()))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    /// An in-memory presentation server: the content per slide, the order, and what was called.
    #[derive(Default)]
    pub struct Mem {
        pub slides: Vec<(String, bool, String)>,
        pub calls: Vec<String>,
        pub too_large: HashSet<String>,
        pub offline: bool,
    }

    impl Mem {
        pub fn from_deck(d: &Deck) -> Mem {
            Mem { slides: d.slides.iter().map(|s| (s.id.clone(), s.hidden, s.data.to_json())).collect(), ..Mem::default() }
        }
        fn find(&mut self, sid: &str) -> Result<&mut (String, bool, String), Error> {
            self.slides.iter_mut().find(|s| s.0 == sid).ok_or(Error::NotFound)
        }
        fn net(&self) -> Result<(), Error> {
            if self.offline {
                Err(Error::Transport("offline".into()))
            } else {
                Ok(())
            }
        }
        fn set_key(data: &str, key: &str, v: Value) -> String {
            let mut x: Value = serde_json::from_str(data).unwrap_or(json!({}));
            x[key] = v;
            x.to_string()
        }
    }

    impl Server for Mem {
        fn slide(&mut self, sid: &str) -> Result<String, Error> {
            self.net()?;
            self.calls.push(format!("GET {sid}"));
            Ok(self.find(sid)?.2.clone())
        }
        fn put(&mut self, sid: &str, elements: Option<&str>, notes: Option<&str>) -> Result<(), Error> {
            self.net()?;
            self.calls.push(format!("PUT {sid}"));
            if self.too_large.contains(sid) {
                return Err(Error::TooLarge);
            }
            let s = self.find(sid)?;
            if let Some(e) = elements {
                s.2 = Mem::set_key(&s.2, "elements", serde_json::from_str(e).unwrap_or(json!([])));
            }
            if let Some(n) = notes {
                s.2 = Mem::set_key(&s.2, "notes", json!(n));
            }
            Ok(())
        }
        fn patch(&mut self, sid: &str, background: Option<&str>, transition: Option<&str>, hidden: Option<bool>) -> Result<(), Error> {
            self.net()?;
            self.calls.push(format!("PATCH {sid}"));
            let s = self.find(sid)?;
            if let Some(b) = background {
                s.2 = Mem::set_key(&s.2, "background", serde_json::from_str(b).unwrap_or(Value::Null));
            }
            if let Some(t) = transition {
                s.2 = Mem::set_key(&s.2, "transition", serde_json::from_str(t).unwrap_or(Value::Null));
            }
            if let Some(h) = hidden {
                s.1 = h;
            }
            Ok(())
        }
        fn create(&mut self, sid: &str, position: usize) -> Result<(), Error> {
            self.net()?;
            self.calls.push(format!("POST {sid}@{position}"));
            if self.slides.iter().any(|s| s.0 == sid) {
                return Ok(());
            }
            let p = position.min(self.slides.len());
            self.slides.insert(p, (sid.to_string(), false, kubuno_office_slides_core::model::EMPTY_SLIDE.to_string()));
            Ok(())
        }
        fn delete(&mut self, sid: &str) -> Result<(), Error> {
            self.net()?;
            self.calls.push(format!("DELETE {sid}"));
            let n = self.slides.len();
            self.slides.retain(|s| s.0 != sid);
            if n == self.slides.len() {
                Err(Error::NotFound)
            } else {
                Ok(())
            }
        }
        fn reorder(&mut self, order: &[String]) -> Result<(), Error> {
            self.net()?;
            self.calls.push(format!("REORDER {}", order.join(",")));
            self.slides.sort_by_key(|s| order.iter().position(|o| *o == s.0).unwrap_or(usize::MAX));
            Ok(())
        }
    }

    pub fn deck(ids: &[&str]) -> Deck {
        let meta = json!({ "slides": ids.iter().enumerate().map(|(i, id)| json!({ "id": id, "position": i })).collect::<Vec<_>>() });
        let slides: serde_json::Map<String, Value> = ids.iter().map(|id| (id.to_string(), serde_json::from_str::<Value>(kubuno_office_slides_core::model::EMPTY_SLIDE).unwrap_or(Value::Null))).collect();
        Deck::from_server(&meta, &json!({ "version": 1, "slides": slides }).to_string())
    }

    fn el(id: &str) -> kubuno_office_slides_core::model::Element {
        kubuno_office_slides_core::model::Element::parse(&format!(r#"{{"id":"{id}","type":"shape","x":0.1}}"#)).unwrap_or_default()
    }

    #[test]
    fn an_edit_is_saved_after_checking_the_slide_did_not_move() {
        let mut base = deck(&["a", "b"]);
        let mut server = Mem::from_deck(&base);
        let mut local = base.clone();
        local.slides[1].data.elements.push(el("e1"));
        local.slides[1].data.set_notes("hello");
        let rep = save(&mut server, &mut base, &local, &mut HashMap::new(), &HashSet::new());
        assert!(rep.clean(), "{rep:?}");
        assert_eq!(server.calls, ["GET b", "PUT b"]);
        assert!(!unsaved(&base, &local));
        let saved: Value = serde_json::from_str(&server.slides[1].2).unwrap_or(Value::Null);
        assert_eq!(saved["notes"], "hello");
        assert_eq!(saved["elements"][0]["id"], "e1");
    }

    #[test]
    fn a_slide_changed_on_the_server_is_a_conflict_never_an_overwrite() {
        let mut base = deck(&["a"]);
        let mut server = Mem::from_deck(&base);
        server.slides[0].2 = Mem::set_key(&server.slides[0].2, "notes", json!("theirs"));
        let mut local = base.clone();
        local.slides[0].data.set_notes("mine");
        let rep = save(&mut server, &mut base, &local, &mut HashMap::new(), &HashSet::new());
        assert_eq!(rep.conflicts, ["a"]);
        assert!(!server.calls.iter().any(|c| c.starts_with("PUT")), "nothing written: {:?}", server.calls);
        // « Keep mine »: written without the re-read, once.
        let mut resolved = HashMap::from([("a".to_string(), Resolution::KeepMine)]);
        let rep = save(&mut server, &mut base, &local, &mut resolved, &HashSet::new());
        assert!(rep.clean());
        assert!(server.slides[0].2.contains("mine"));
    }

    #[test]
    fn created_deleted_and_moved_slides_reach_the_server_in_order() {
        let mut base = deck(&["a", "b", "c"]);
        let mut server = Mem::from_deck(&base);
        let mut local = base.clone();
        local.slides.retain(|s| s.id != "b");
        let mut n = SlideData::empty();
        n.elements.push(el("t"));
        local.slides.insert(0, Slide { id: "n".into(), hidden: true, data: n });
        local.slides.swap(1, 2); // n, c, a
        let rep = save(&mut server, &mut base, &local, &mut HashMap::new(), &HashSet::new());
        assert!(rep.clean(), "{rep:?}");
        assert_eq!(server.calls, ["DELETE b", "POST n@0", "PUT n", "PATCH n", "REORDER n,c,a"]);
        assert_eq!(server.slides.iter().map(|s| (s.0.as_str(), s.1)).collect::<Vec<_>>(), [("n", true), ("c", false), ("a", false)]);
        assert!(!unsaved(&base, &local));
    }

    #[test]
    fn too_large_is_terminal_for_its_slide_and_offline_keeps_the_work() {
        let mut base = deck(&["a", "b"]);
        let mut server = Mem::from_deck(&base);
        server.too_large.insert("a".into());
        let mut local = base.clone();
        local.slides[0].data.elements.push(el("big"));
        local.slides[1].data.elements.push(el("small"));
        let rep = save(&mut server, &mut base, &local, &mut HashMap::new(), &HashSet::new());
        assert_eq!(rep.too_large, ["a"]);
        assert!(server.slides[1].2.contains("small"), "the other slide still saves");
        let mut base2 = deck(&["a"]);
        let mut offline = Mem::from_deck(&base2);
        offline.offline = true;
        let mut l2 = base2.clone();
        l2.slides[0].data.set_notes("x");
        let rep = save(&mut offline, &mut base2, &l2, &mut HashMap::new(), &HashSet::new());
        assert!(matches!(rep.failed, Some(Error::Transport(_))));
        assert!(unsaved(&base2, &l2));
    }
}
