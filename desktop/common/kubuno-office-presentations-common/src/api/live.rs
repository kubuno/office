//! A presentation opened from the server, kept in step with it by a background thread (Documents' `live.rs`,
//! for per-slide saves).
//!
//! One worker per open presentation owns the server copy it last read or wrote (`base`), the latest local deck
//! the window published, and the crash journal. It saves one second after the last change (the web's
//! `saveTimerRef`), on Ctrl+S and at close, pings the editing session every minute, retries a failed pass with a
//! growing delay, and hands what the window must show back through a sink. The window never waits on the
//! network, except when it closes ([`Live::close`], at most [`CLOSE_WAIT`]).

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use kubuno_office_slides_core::model::Deck;

use super::client::{Client, Error};
use super::journal::Journal;
use super::session::{self, Report, Resolution, Server};

/// The worker's clock when nothing happens.
pub const TICK_MS: u64 = 250;
/// Save this long after the last change (`saveTimerRef`: 1000 ms).
pub const DEBOUNCE_MS: u64 = 1000;
/// The editing session's ping (the server's presence window is 2 minutes).
pub const PING_MS: u64 = 60_000;
/// How long closing the window may wait for the last save and the leave.
pub const CLOSE_WAIT: Duration = Duration::from_secs(8);

/// Where the save stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// The server holds everything.
    Saved,
    /// Changes wait for the debounce.
    Dirty,
    Saving,
    /// The last pass failed (network…): it is retried; the journal keeps the work.
    Failed,
    /// Offline since the opening: the work is in the journal only.
    Offline,
}

/// What the window shows of the session.
#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    pub state: State,
    pub conflicts: Vec<String>,
    pub too_large: Vec<String>,
    pub editors: Vec<String>,
    pub message: String,
}

/// What the worker tells the window.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// The presentation is open: the deck (the server's, or offline the journal's), and a journal copy left by
    /// a previous run that differs from it, for the window to offer.
    Opened { deck: Box<Deck>, recovered: Option<Box<Deck>>, offline: bool },
    Failed(String),
    Status(Status),
    /// Slides the server changed under us: only the user can choose.
    Conflict(Vec<String>),
    /// « Take theirs »: the window replaces these slides' content (JSON text).
    Reload(Vec<(String, String)>),
    Closed,
}

pub type Sink = Arc<dyn Fn(Event) + Send + Sync>;

enum Cmd {
    Edited(Box<Deck>),
    SaveNow,
    Recovered(bool),
    Resolve(Vec<String>, Resolution),
    Close(Sender<()>),
}

/// The routes, through the REST client of one presentation.
pub struct Remote {
    pub client: Client,
    pub id: String,
}

impl Server for Remote {
    fn slide(&mut self, sid: &str) -> Result<String, Error> {
        self.client.slide(&self.id, sid)
    }
    fn put(&mut self, sid: &str, elements: Option<&str>, notes: Option<&str>) -> Result<(), Error> {
        self.client.put_slide(&self.id, sid, elements, notes)
    }
    fn patch(&mut self, sid: &str, background: Option<&str>, transition: Option<&str>, hidden: Option<bool>) -> Result<(), Error> {
        self.client.patch_slide(&self.id, sid, background, transition, hidden)
    }
    fn create(&mut self, sid: &str, position: usize) -> Result<(), Error> {
        self.client.create_slide(&self.id, sid, position)
    }
    fn delete(&mut self, sid: &str) -> Result<(), Error> {
        self.client.delete_slide(&self.id, sid)
    }
    fn reorder(&mut self, order: &[String]) -> Result<(), Error> {
        self.client.reorder(&self.id, order)
    }
}

/// What the worker needs of the outside world (the server and the journal), so the tests can run it.
pub trait Host: Server {
    /// Opens: the deck as the server holds it and the other editors (`None`: unreachable).
    fn open(&mut self) -> Result<(Deck, Vec<String>), Error>;
    fn ping(&mut self) -> Result<(), Error>;
    fn leave(&mut self) -> Result<(), Error>;
    fn journal(&mut self) -> &mut dyn Journal;
    fn id(&self) -> &str;
}

/// The real host: REST + the file journal.
pub struct Link {
    pub remote: Remote,
    pub journal: super::journal::FileJournal,
    pub me: String,
}

impl Server for Link {
    fn slide(&mut self, sid: &str) -> Result<String, Error> {
        self.remote.slide(sid)
    }
    fn put(&mut self, sid: &str, e: Option<&str>, n: Option<&str>) -> Result<(), Error> {
        self.remote.put(sid, e, n)
    }
    fn patch(&mut self, sid: &str, b: Option<&str>, t: Option<&str>, h: Option<bool>) -> Result<(), Error> {
        self.remote.patch(sid, b, t, h)
    }
    fn create(&mut self, sid: &str, p: usize) -> Result<(), Error> {
        self.remote.create(sid, p)
    }
    fn delete(&mut self, sid: &str) -> Result<(), Error> {
        self.remote.delete(sid)
    }
    fn reorder(&mut self, o: &[String]) -> Result<(), Error> {
        self.remote.reorder(o)
    }
}

impl Host for Link {
    fn open(&mut self) -> Result<(Deck, Vec<String>), Error> {
        let meta = self.remote.client.meta(&self.remote.id)?;
        let (content, editors) = self.remote.client.join(&self.remote.id)?;
        let me = self.me.clone();
        let names = editors
            .as_array()
            .map(|a| {
                a.iter()
                    .filter(|e| e.get("user_id").and_then(|u| u.as_str()) != Some(me.as_str()))
                    .map(|e| e.get("display_name").and_then(|d| d.as_str()).or_else(|| e.get("user_id").and_then(|u| u.as_str())).unwrap_or("").to_string())
                    .collect()
            })
            .unwrap_or_default();
        Ok((Deck::from_server(&meta.value, &content), names))
    }
    fn ping(&mut self) -> Result<(), Error> {
        self.remote.client.ping(&self.remote.id)
    }
    fn leave(&mut self) -> Result<(), Error> {
        self.remote.client.leave(&self.remote.id)
    }
    fn journal(&mut self) -> &mut dyn Journal {
        &mut self.journal
    }
    fn id(&self) -> &str {
        &self.remote.id
    }
}

/// Connects to presentation `id` as the account the shell shows (its access token borrowed from the shell's
/// broker: this app never sees a password or a refresh token).
pub fn connect(id: &str) -> Result<Link, String> {
    let journal = super::journal::FileJournal::for_user().ok_or_else(|| "dossier de données introuvable".to_string())?;
    let account = kubuno_desktop_sync::current_account()
        .map_err(|e| format!("compte : {e}"))?
        .ok_or_else(|| "aucun compte connecté : connectez-vous dans Kubuno Desktop".to_string())?;
    let me = account.user_id.clone();
    Ok(Link { remote: Remote { client: Client::for_account(account), id: id.to_string() }, journal, me })
}

/// The window's handle on the worker.
pub struct Live {
    tx: Sender<Cmd>,
    pub id: String,
}

impl Live {
    /// Starts the worker; `connect` runs on it.
    pub fn open<H, F>(id: String, sink: Sink, connect: F) -> Live
    where
        H: Host + Send + 'static,
        F: FnOnce() -> Result<H, String> + Send + 'static,
    {
        let (tx, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new().name(format!("presentations-live-{id}")).spawn(move || match connect() {
            Ok(host) => run(host, rx, sink),
            Err(why) => {
                sink(Event::Failed(why));
                while let Ok(cmd) = rx.recv() {
                    if let Cmd::Close(done) = cmd {
                        let _ = done.send(());
                        break;
                    }
                }
            }
        });
        if let Err(e) = spawned {
            tracing::error!("[presentations] live session thread: {e}");
        }
        Live { tx, id }
    }

    /// The deck changed: this is it now.
    pub fn edited(&self, deck: Deck) {
        let _ = self.tx.send(Cmd::Edited(Box::new(deck)));
    }

    pub fn save_now(&self) {
        let _ = self.tx.send(Cmd::SaveNow);
    }

    /// The answer to the recovery offer (`true`: the window loaded the journal's copy and publishes it).
    pub fn recovered(&self, kept: bool) {
        let _ = self.tx.send(Cmd::Recovered(kept));
    }

    pub fn resolve(&self, slides: Vec<String>, choice: Resolution) {
        let _ = self.tx.send(Cmd::Resolve(slides, choice));
    }

    /// Saves what can be, keeps the rest in the journal, leaves; waits at most [`CLOSE_WAIT`].
    pub fn close(&self, deck: Option<Deck>) -> bool {
        if let Some(d) = deck {
            let _ = self.tx.send(Cmd::Edited(Box::new(d)));
        }
        let (done_tx, done_rx) = mpsc::channel();
        if self.tx.send(Cmd::Close(done_tx)).is_err() {
            return true;
        }
        done_rx.recv_timeout(CLOSE_WAIT).is_ok()
    }
}

struct Worker<H: Host> {
    host: H,
    sink: Sink,
    base: Deck,
    local: Option<Deck>,
    editors: Vec<String>,
    offline: bool,
    joined: bool,
    dirty_since: Option<Instant>,
    save_now: bool,
    retry_at: Option<Instant>,
    backoff: Duration,
    last_ping: Instant,
    conflicts: HashSet<String>,
    too_large: HashSet<String>,
    resolved: HashMap<String, Resolution>,
    last_status: Option<Status>,
    journal_dirty: bool,
}

fn run<H: Host>(mut host: H, rx: Receiver<Cmd>, sink: Sink) {
    let id = host.id().to_string();
    let journaled = host.journal().read(&id);
    let opened = host.open();
    let (base, editors, offline) = match opened {
        Ok((deck, editors)) => (deck, editors, false),
        Err(Error::Transport(why)) => match journaled.as_ref() {
            Some(j) => (j.base.clone(), Vec::new(), true),
            None => {
                sink(Event::Failed(format!("réseau : {why}")));
                drain(rx);
                return;
            }
        },
        Err(e) => {
            sink(Event::Failed(e.to_string()));
            drain(rx);
            return;
        }
    };
    // A journal whose local copy differs from what is opened: offer it (offline, it IS what opens).
    let (shown, recovered) = match journaled {
        Some(j) if offline => (j.local, None),
        Some(j) if session::unsaved(&base, &j.local) => (base.clone(), Some(Box::new(j.local))),
        _ => (base.clone(), None),
    };
    sink(Event::Opened { deck: Box::new(shown.clone()), recovered, offline });
    let mut w = Worker {
        host,
        sink,
        base,
        // What the window shows now: the journal's copy offline, else the opened deck (published again on its first edit).
        local: Some(shown),
        editors,
        offline,
        joined: !offline,
        dirty_since: None,
        save_now: false,
        retry_at: None,
        backoff: Duration::from_secs(2),
        last_ping: Instant::now(),
        conflicts: HashSet::new(),
        too_large: HashSet::new(),
        resolved: HashMap::new(),
        last_status: None,
        journal_dirty: false,
    };
    w.status("");
    loop {
        match rx.recv_timeout(Duration::from_millis(TICK_MS)) {
            Ok(Cmd::Edited(d)) => {
                // A slide refused for its size gets another chance once it changes.
                if let Some(prev) = &w.local {
                    w.too_large.retain(|sid| prev.slide(sid).map(|s| s.data.to_json()) == d.slide(sid).map(|s| s.data.to_json()));
                }
                w.local = Some(*d);
                w.dirty_since = Some(Instant::now());
                w.journal_dirty = true;
            }
            Ok(Cmd::SaveNow) => w.save_now = true,
            Ok(Cmd::Recovered(kept)) => {
                if !kept {
                    let id = w.host.id().to_string();
                    w.host.journal().clear(&id);
                }
            }
            Ok(Cmd::Resolve(slides, choice)) => w.resolve(slides, choice),
            Ok(Cmd::Close(done)) => {
                w.flush_journal();
                w.pass();
                if w.joined {
                    let _ = w.host.leave();
                }
                let id = w.host.id().to_string();
                if !w.unsaved() {
                    w.host.journal().clear(&id);
                }
                (w.sink)(Event::Closed);
                let _ = done.send(());
                return;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        w.tick();
    }
}

fn drain(rx: Receiver<Cmd>) {
    while let Ok(cmd) = rx.recv() {
        if let Cmd::Close(done) = cmd {
            let _ = done.send(());
            break;
        }
    }
}

impl<H: Host> Worker<H> {
    fn unsaved(&self) -> bool {
        self.local.as_ref().is_some_and(|l| session::unsaved(&self.base, l))
    }

    fn flush_journal(&mut self) {
        if !self.journal_dirty {
            return;
        }
        self.journal_dirty = false;
        if let Some(local) = &self.local {
            let id = self.host.id().to_string();
            let entry = super::journal::Entry { local: local.clone(), base: self.base.clone() };
            if let Err(e) = self.host.journal().write(&id, &entry) {
                tracing::warn!("[presentations] journal: {e}");
            }
        }
    }

    fn tick(&mut self) {
        self.flush_journal();
        let due = self.save_now || self.dirty_since.is_some_and(|t| t.elapsed() >= Duration::from_millis(DEBOUNCE_MS));
        let retry = self.retry_at.is_some_and(|t| Instant::now() >= t);
        if (due || retry) && self.unsaved() {
            self.pass();
        } else if due {
            self.dirty_since = None;
            self.save_now = false;
            self.status("");
        }
        if self.joined && self.last_ping.elapsed() >= Duration::from_millis(PING_MS) {
            self.last_ping = Instant::now();
            let _ = self.host.ping();
        }
    }

    /// One save pass.
    fn pass(&mut self) {
        let Some(local) = self.local.clone() else { return };
        self.save_now = false;
        self.dirty_since = None;
        if !session::unsaved(&self.base, &local) {
            self.status("");
            return;
        }
        if self.offline {
            // Reopen to join first: the server copy may have moved while we were away (the guard re-reads anyway).
            match self.host.open() {
                Ok((_, editors)) => {
                    self.offline = false;
                    self.joined = true;
                    self.editors = editors;
                }
                Err(_) => {
                    self.retry_later();
                    self.status("");
                    return;
                }
            }
        }
        self.status_state(State::Saving);
        let skip: HashSet<String> = self.conflicts.union(&self.too_large).cloned().collect();
        let rep: Report = session::save(&mut self.host, &mut self.base, &local, &mut self.resolved, &skip);
        let mut fresh_conflicts = Vec::new();
        for c in &rep.conflicts {
            if self.conflicts.insert(c.clone()) {
                fresh_conflicts.push(c.clone());
            }
        }
        for t in &rep.too_large {
            self.too_large.insert(t.clone());
        }
        let message = match &rep.failed {
            Some(e) if e.retryable() => {
                self.retry_later();
                e.to_string()
            }
            Some(e) => {
                self.retry_at = None;
                e.to_string()
            }
            None => {
                self.retry_at = None;
                self.backoff = Duration::from_secs(2);
                String::new()
            }
        };
        self.journal_dirty = true;
        self.flush_journal();
        if !fresh_conflicts.is_empty() {
            (self.sink)(Event::Conflict(fresh_conflicts));
        }
        self.status(&message);
    }

    fn retry_later(&mut self) {
        self.retry_at = Some(Instant::now() + self.backoff);
        self.backoff = (self.backoff * 2).min(Duration::from_secs(30));
    }

    fn resolve(&mut self, slides: Vec<String>, choice: Resolution) {
        match choice {
            Resolution::KeepMine => {
                for s in slides {
                    self.conflicts.remove(&s);
                    self.resolved.insert(s, Resolution::KeepMine);
                }
                self.save_now = true;
            }
            Resolution::TakeTheirs => {
                let mut reload = Vec::new();
                for s in slides {
                    self.conflicts.remove(&s);
                    if let Ok(theirs) = self.host.slide(&s) {
                        if let Some(b) = self.base.slide_mut(&s) {
                            b.data = kubuno_office_slides_core::model::SlideData::parse(&theirs);
                        }
                        reload.push((s, theirs));
                    }
                }
                (self.sink)(Event::Reload(reload));
            }
        }
        self.status("");
    }

    fn status_state(&mut self, state: State) {
        let s = Status {
            state,
            conflicts: self.conflicts.iter().cloned().collect(),
            too_large: self.too_large.iter().cloned().collect(),
            editors: self.editors.clone(),
            message: String::new(),
        };
        if self.last_status.as_ref() != Some(&s) {
            self.last_status = Some(s.clone());
            (self.sink)(Event::Status(s));
        }
    }

    fn status(&mut self, message: &str) {
        let state = if self.offline {
            State::Offline
        } else if self.retry_at.is_some() {
            State::Failed
        } else if self.unsaved() {
            State::Dirty
        } else {
            State::Saved
        };
        let mut conflicts: Vec<String> = self.conflicts.iter().cloned().collect();
        conflicts.sort();
        let mut too_large: Vec<String> = self.too_large.iter().cloned().collect();
        too_large.sort();
        let s = Status { state, conflicts, too_large, editors: self.editors.clone(), message: message.to_string() };
        if self.last_status.as_ref() != Some(&s) {
            self.last_status = Some(s.clone());
            (self.sink)(Event::Status(s));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::journal::{Entry, MemJournal};
    use crate::api::session::tests::{deck, Mem};
    use std::sync::Mutex;

    struct TestHost {
        server: Mem,
        open_deck: Deck,
        journal: MemJournal,
        left: Arc<Mutex<u32>>,
        offline_open: bool,
    }

    impl Server for TestHost {
        fn slide(&mut self, sid: &str) -> Result<String, Error> {
            self.server.slide(sid)
        }
        fn put(&mut self, sid: &str, e: Option<&str>, n: Option<&str>) -> Result<(), Error> {
            self.server.put(sid, e, n)
        }
        fn patch(&mut self, sid: &str, b: Option<&str>, t: Option<&str>, h: Option<bool>) -> Result<(), Error> {
            self.server.patch(sid, b, t, h)
        }
        fn create(&mut self, sid: &str, p: usize) -> Result<(), Error> {
            self.server.create(sid, p)
        }
        fn delete(&mut self, sid: &str) -> Result<(), Error> {
            self.server.delete(sid)
        }
        fn reorder(&mut self, o: &[String]) -> Result<(), Error> {
            self.server.reorder(o)
        }
    }

    impl Host for TestHost {
        fn open(&mut self) -> Result<(Deck, Vec<String>), Error> {
            if self.offline_open {
                return Err(Error::Transport("offline".into()));
            }
            Ok((self.open_deck.clone(), vec!["Alice".into()]))
        }
        fn ping(&mut self) -> Result<(), Error> {
            Ok(())
        }
        fn leave(&mut self) -> Result<(), Error> {
            *self.left.lock().unwrap_or_else(|p| p.into_inner()) += 1;
            Ok(())
        }
        fn journal(&mut self) -> &mut dyn Journal {
            &mut self.journal
        }
        fn id(&self) -> &str {
            "p"
        }
    }

    fn collect() -> (Sink, Arc<Mutex<Vec<Event>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let e2 = events.clone();
        (Arc::new(move |e| e2.lock().unwrap_or_else(|p| p.into_inner()).push(e)), events)
    }

    #[test]
    fn edits_are_saved_after_the_debounce_and_the_session_leaves_once() {
        let d = deck(&["a"]);
        let left = Arc::new(Mutex::new(0));
        let server = Mem::from_deck(&d);
        let host = TestHost { server, open_deck: d.clone(), journal: MemJournal::default(), left: left.clone(), offline_open: false };
        let (sink, events) = collect();
        let live = Live::open("p".into(), sink, move || Ok(host));
        let mut edited = d.clone();
        edited.slides[0].data.set_notes("hello");
        live.edited(edited);
        std::thread::sleep(Duration::from_millis(1600));
        assert!(live.close(None));
        let ev = events.lock().unwrap_or_else(|p| p.into_inner()).clone();
        assert!(matches!(ev.first(), Some(Event::Opened { offline: false, .. })));
        assert!(ev.iter().any(|e| matches!(e, Event::Status(s) if s.state == State::Saved && s.editors == ["Alice"])), "{ev:?}");
        assert!(ev.iter().any(|e| matches!(e, Event::Status(s) if s.state == State::Saving)), "the save ran: {ev:?}");
        assert_eq!(*left.lock().unwrap_or_else(|p| p.into_inner()), 1);
        assert!(matches!(ev.last(), Some(Event::Closed)));
    }

    #[test]
    fn offline_opens_the_journal_and_keeps_it() {
        let d = deck(&["a"]);
        let mut local = d.clone();
        local.slides[0].data.set_notes("offline work");
        let mut journal = MemJournal::default();
        journal.write("p", &Entry { local: local.clone(), base: d.clone() }).expect("journal");
        let host = TestHost { server: Mem::from_deck(&d), open_deck: d, journal, left: Arc::new(Mutex::new(0)), offline_open: true };
        let (sink, events) = collect();
        let live = Live::open("p".into(), sink, move || Ok(host));
        std::thread::sleep(Duration::from_millis(300));
        live.close(None);
        let ev = events.lock().unwrap_or_else(|p| p.into_inner()).clone();
        match ev.first() {
            Some(Event::Opened { deck, offline: true, .. }) => assert_eq!(deck.slides[0].data.notes(), "offline work"),
            other => panic!("{other:?}"),
        }
    }
}
