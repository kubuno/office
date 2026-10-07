//! The server session of an open project, on the UI thread: it feeds the worker ([`crate::api::worker`]) the
//! editor's outbox one call at a time, applies what comes back to the editor, re-reads the project when the
//! queue is empty (the web's `refresh()`), keeps the journal up to date, retries after a lost connection and
//! re-reads an idle project periodically (until the real-time room — a Yjs client — is shared by the Office
//! apps, it is how edits made on the web show up).
//!
//! Portable and synchronous: the window calls [`ProjectSession::on_event`] with each worker event (posted to
//! its thread), [`ProjectSession::after_command`] after each edit and [`ProjectSession::tick`] on a timer.

use kubuno_office_projects_core::date::Date;
use kubuno_office_projects_core::editor::{Editor, Outgoing};

use crate::api::journal::FileJournal;
use crate::api::worker::{Event, Job};

/// How often an idle project is re-read, and how soon a lost connection is retried (ms).
pub const POLL_MS: i64 = 30_000;
pub const RETRY_MS: i64 = 10_000;

/// What the status bar says about the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncState {
    /// Waiting for the first read.
    Opening,
    /// Everything is on the server.
    Saved,
    /// Calls are on their way.
    Saving,
    /// The server cannot be reached: changes wait in the journal.
    Offline,
    /// Edits made on stale data meet newer values: the user must choose.
    Conflict,
    /// The project could not be opened.
    Failed,
}

/// What the window must do after an event.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Outcome {
    /// The editor changed: repaint, re-bind.
    pub changed: bool,
    /// Messages for the status bar (refusals, failures).
    pub messages: Vec<String>,
    /// Ask the user (keep mine / take theirs) about these tasks.
    pub conflict: Option<Vec<String>>,
}

/// The session (see the module doc).
pub struct ProjectSession {
    pub id: String,
    post: Box<dyn Fn(Job)>,
    journal: Option<FileJournal>,
    pub state: SyncState,
    connected: bool,
    sending: bool,
    fetching: bool,
    last_fetch_ms: i64,
    offline_at: Option<i64>,
    asked_conflict: bool,
    /// Back from an offline spell: the project is re-read (and the unsent edits checked against it) before
    /// anything is sent again.
    resync: bool,
    /// The signed-in user (`Connected`).
    pub me: Option<String>,
}

impl ProjectSession {
    pub fn new(id: String, post: Box<dyn Fn(Job)>, journal: Option<FileJournal>) -> Self {
        Self { id, post, journal, state: SyncState::Opening, connected: false, sending: false, fetching: false, last_fetch_ms: 0, offline_at: None, asked_conflict: false, resync: false, me: None }
    }

    /// The project as the journal kept it (shown at once, before the server answers).
    pub fn restore(&self, today: Date) -> Option<Editor> {
        let j = self.journal.as_ref()?.read(&self.id)?;
        Some(Editor::restore(j, today, false))
    }

    fn write_journal(&self, ed: &Editor) {
        let Some(j) = &self.journal else { return };
        let r = if ed.has_pending() { j.write(&self.id, &ed.journal()) } else {
            j.clear(&self.id);
            Ok(())
        };
        if let Err(e) = r {
            tracing::warn!("[projects] {e}");
        }
    }

    fn refresh_state(&mut self, ed: &Editor) {
        if self.state == SyncState::Failed && !self.connected {
            return;
        }
        self.state = if self.offline_at.is_some() {
            SyncState::Offline
        } else if self.asked_conflict {
            SyncState::Conflict
        } else if !self.connected && ed.has_pending() {
            SyncState::Offline
        } else if ed.has_pending() || self.sending {
            SyncState::Saving
        } else if self.fetching && self.last_fetch_ms == 0 {
            SyncState::Opening
        } else {
            SyncState::Saved
        };
    }

    /// Sends the next call of the outbox, if the connection is up and nothing is on its way.
    pub fn pump(&mut self, ed: &mut Editor, out: &mut Outcome) {
        if !self.connected || self.sending || self.offline_at.is_some() || self.asked_conflict || self.resync {
            self.refresh_state(ed);
            return;
        }
        match ed.next_outgoing() {
            Outgoing::Send { seq, call } => {
                self.sending = true;
                (self.post)(Job::Send { seq, call });
            }
            Outgoing::Conflict(names) => {
                self.asked_conflict = true;
                out.conflict = Some(names);
            }
            Outgoing::Idle | Outgoing::Busy => {}
        }
        self.refresh_state(ed);
    }

    /// After a command: the journal, then the outbox.
    pub fn after_command(&mut self, ed: &mut Editor) -> Outcome {
        let mut out = Outcome { changed: true, messages: ed.take_messages(), conflict: None };
        self.write_journal(ed);
        self.pump(ed, &mut out);
        out
    }

    /// The user's answer to a conflict.
    pub fn resolve(&mut self, ed: &mut Editor, keep_mine: bool) -> Outcome {
        ed.resolve_conflict(keep_mine);
        self.asked_conflict = false;
        let mut out = Outcome { changed: true, ..Outcome::default() };
        self.write_journal(ed);
        self.pump(ed, &mut out);
        out
    }

    fn fetch(&mut self, now_ms: i64) {
        if !self.fetching {
            self.fetching = true;
            self.last_fetch_ms = self.last_fetch_ms.max(1);
            let _ = now_ms;
            (self.post)(Job::Fetch);
        }
    }

    /// A worker event. `ed` is `None` until the project is known (no journal, first read pending).
    pub fn on_event(&mut self, ed: &mut Option<Editor>, ev: Event, today: Date, now_ms: i64) -> Outcome {
        let mut out = Outcome::default();
        match ev {
            Event::Connected { me } => {
                self.connected = true;
                self.resync = true;
                self.me = me;
                self.fetching = false;
                self.fetch(now_ms);
                (self.post)(Job::FetchSide);
            }
            Event::ConnectFailed(why) => {
                self.connected = false;
                out.messages.push(why);
                self.state = if ed.is_some() { SyncState::Offline } else { SyncState::Failed };
                return out;
            }
            Event::Fetched(d) => {
                self.fetching = false;
                self.resync = false;
                self.last_fetch_ms = now_ms.max(1);
                self.offline_at = None;
                match ed {
                    Some(e) => e.rebase(*d),
                    None => *ed = Some(Editor::new(*d, today, false)),
                }
                out.changed = true;
            }
            Event::Side(side) => {
                if let Some(e) = ed.as_mut() {
                    if let Some(w) = side.include_weekends {
                        e.ctx.include_weekends_default = w;
                    }
                    if let Some(s) = side.settings {
                        e.settings = s;
                    }
                    if let Some(b) = side.baselines {
                        e.baselines = b;
                    }
                    if let Some(v) = side.versions {
                        e.versions = v;
                    }
                    e.set_calendars(side.calendars);
                    out.changed = true;
                }
            }
            Event::Sent { seq, response } => {
                self.sending = false;
                if let Some(e) = ed.as_mut() {
                    let side = e.pending_change(seq).is_some_and(|c| c.edits_side_list());
                    let time_off = e.pending_change(seq).and_then(|c| c.edits_time_off().map(str::to_string));
                    let empty = e.acknowledge(seq, &response);
                    if side {
                        (self.post)(Job::FetchSide);
                    }
                    if let Some(resource) = time_off {
                        // The server minted the entry's id: its list is read again.
                        (self.post)(Job::FetchTimeOff { resource: e.resolve(&resource) });
                    }
                    if empty {
                        self.fetch(now_ms);
                    }
                    out.changed = true;
                }
            }
            Event::Refused { seq, message } => {
                self.sending = false;
                if let Some(e) = ed.as_mut() {
                    e.reject(seq, &message);
                    out.messages.extend(e.take_messages());
                    out.changed = true;
                    self.fetch(now_ms);
                }
            }
            Event::Offline { seq, why } => {
                if let (Some(s), Some(e)) = (seq, ed.as_mut()) {
                    e.offline(s);
                }
                self.sending = false;
                self.fetching = false;
                self.offline_at = Some(now_ms);
                tracing::info!("[projects] offline: {why}");
                if ed.is_none() {
                    // Nothing to show yet: say why instead of "opening" forever (the timer retries).
                    self.state = SyncState::Offline;
                    out.messages.push(why);
                    return out;
                }
            }
            Event::FetchFailed(m) => {
                self.fetching = false;
                out.messages.push(m);
                if ed.is_none() {
                    self.state = SyncState::Failed;
                    return out;
                }
            }
            Event::ResourceSide { members, currency, default_hourly_rate } => {
                if let Some(e) = ed.as_mut() {
                    if members.is_some() {
                        e.resources.members = members;
                    }
                    if currency.is_some() {
                        e.resources.currency = currency;
                        e.resources.default_hourly_rate = default_hourly_rate;
                    }
                    out.changed = true;
                }
            }
            Event::TimeOff { resource, entries } => {
                if let (Some(e), Some(list)) = (ed.as_mut(), entries) {
                    e.resources.set_time_off(&resource, list);
                    out.changed = true;
                }
            }
        }
        if let Some(e) = ed.as_mut() {
            self.write_journal(e);
            self.pump(e, &mut out);
        }
        out
    }

    /// The timer: retry after a lost connection, re-read an idle project.
    pub fn tick(&mut self, ed: &mut Option<Editor>, now_ms: i64) -> Outcome {
        let mut out = Outcome::default();
        if !self.connected {
            return out;
        }
        if let Some(at) = self.offline_at {
            if now_ms - at >= RETRY_MS {
                self.offline_at = None;
                self.resync = true;
                self.fetch(now_ms);
            }
        } else if !self.sending && !self.fetching && ed.as_ref().is_some_and(|e| !e.has_pending()) && now_ms - self.last_fetch_ms >= POLL_MS {
            self.fetch(now_ms);
        }
        if let Some(e) = ed.as_mut() {
            self.pump(e, &mut out);
        }
        out
    }

    /// Reads the resources workspace's side data (members, cost settings) — when the workspace opens.
    pub fn fetch_resource_side(&self) {
        if self.connected {
            (self.post)(Job::FetchResourceSide);
        }
    }

    /// Reads the time off of `resource` — when the availability sub-view shows it.
    pub fn fetch_time_off(&self, resource: &str) {
        if self.connected && !kubuno_office_projects_core::change::is_temp(resource) {
            (self.post)(Job::FetchTimeOff { resource: resource.to_string() });
        }
    }

    /// Closing: whether unsent changes remain (they stay in the journal for the next opening).
    pub fn close(&mut self, ed: Option<&Editor>) -> bool {
        if let Some(e) = ed {
            self.write_journal(e);
            return e.has_pending();
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kubuno_office_projects_core::model::{ProjectData, Task};
    use serde_json::json;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn project() -> ProjectData {
        let mut d = ProjectData::default();
        d.project.id = "p".into();
        d.project.start_date = Some("2026-10-05".into());
        d.tasks.push(Task::new("a", "p"));
        d
    }

    fn session(log: Rc<RefCell<Vec<Job>>>) -> ProjectSession {
        let dir = std::env::temp_dir().join(format!("kubuno-projects-session-{}-{}", std::process::id(), rand_tag()));
        ProjectSession::new("p".into(), Box::new(move |j| log.borrow_mut().push(j)), Some(FileJournal::at(dir)))
    }

    fn rand_tag() -> u128 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)
    }

    fn today() -> Date {
        Date::new(2026, 10, 7).expect("d")
    }

    #[test]
    fn opens_sends_in_order_and_rereads() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut s = session(log.clone());
        let mut ed: Option<Editor> = None;
        s.on_event(&mut ed, Event::Connected { me: None }, today(), 1);
        assert_eq!(log.borrow()[..], [Job::Fetch, Job::FetchSide]);
        s.on_event(&mut ed, Event::Fetched(Box::new(project())), today(), 2);
        assert_eq!(s.state, SyncState::Saved);
        let e = ed.as_mut().expect("editor");
        e.set_task_field("a", "name", json!("X")).expect("edit");
        e.set_task_field("a", "priority", json!("high")).expect("edit");
        let out = s.after_command(e);
        assert!(out.changed);
        assert_eq!(s.state, SyncState::Saving);
        // One call at a time.
        assert_eq!(log.borrow().iter().filter(|j| matches!(j, Job::Send { .. })).count(), 1);
        let Some(Job::Send { seq, .. }) = log.borrow().last().cloned() else { panic!("send") };
        s.on_event(&mut ed, Event::Sent { seq, response: json!({}) }, today(), 3);
        let Some(Job::Send { seq, .. }) = log.borrow().last().cloned() else { panic!("send") };
        s.on_event(&mut ed, Event::Sent { seq, response: json!({}) }, today(), 4);
        // The queue is empty: the project is read again.
        assert_eq!(log.borrow().last(), Some(&Job::Fetch));
    }

    #[test]
    fn offline_keeps_the_journal_and_retries() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut s = session(log.clone());
        let mut ed: Option<Editor> = None;
        s.on_event(&mut ed, Event::Connected { me: None }, today(), 1);
        s.on_event(&mut ed, Event::Fetched(Box::new(project())), today(), 2);
        let e = ed.as_mut().expect("editor");
        e.set_task_field("a", "name", json!("Y")).expect("edit");
        s.after_command(e);
        let Some(Job::Send { seq, .. }) = log.borrow().last().cloned() else { panic!("send") };
        s.on_event(&mut ed, Event::Offline { seq: Some(seq), why: "réseau".into() }, today(), 100);
        assert_eq!(s.state, SyncState::Offline);
        // The journal holds the edit: a new session restores it.
        let restored = s.restore(today()).expect("journal");
        assert_eq!(restored.data().task("a").map(|t| t.name.as_str()), Some("Y"));
        // Retry after RETRY_MS: a read first, then the call again.
        s.tick(&mut ed, 100 + RETRY_MS);
        assert_eq!(log.borrow().last(), Some(&Job::Fetch));
        s.on_event(&mut ed, Event::Fetched(Box::new(project())), today(), 100 + RETRY_MS + 1);
        assert!(matches!(log.borrow().last(), Some(Job::Send { .. })));
    }

    #[test]
    fn a_first_read_that_fails_says_why_and_retries() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut s = session(log.clone());
        let mut ed: Option<Editor> = None;
        s.on_event(&mut ed, Event::Connected { me: None }, today(), 1);
        let out = s.on_event(&mut ed, Event::Offline { seq: None, why: "session expirée".into() }, today(), 2);
        assert_eq!(s.state, SyncState::Offline);
        assert_eq!(out.messages, vec!["session expirée".to_string()]);
        s.tick(&mut ed, 2 + RETRY_MS);
        assert_eq!(log.borrow().last(), Some(&Job::Fetch));
        s.on_event(&mut ed, Event::Fetched(Box::new(project())), today(), 3 + RETRY_MS);
        assert_eq!(s.state, SyncState::Saved);
    }

    #[test]
    fn time_off_is_sent_then_read_again() {
        use kubuno_office_projects_core::resources::TimeOff;
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut s = session(log.clone());
        let mut ed: Option<Editor> = None;
        s.on_event(&mut ed, Event::Connected { me: None }, today(), 1);
        let mut d = project();
        d.resources.push(serde_json::from_value(json!({ "id": "r", "name": "Alice" })).expect("resource"));
        s.on_event(&mut ed, Event::Fetched(Box::new(d)), today(), 2);
        let e = ed.as_mut().expect("editor");
        e.create_time_off("r", "2026-10-19", "2026-10-21", Some("Formation")).expect("time off");
        assert_eq!(e.resources.time_off.get("r").map(Vec::len), Some(1));
        s.after_command(e);
        let Some(Job::Send { seq, call }) = log.borrow().last().cloned() else { panic!("send") };
        assert_eq!(call.path, "/projects/p/resources/r/time-off");
        s.on_event(&mut ed, Event::Sent { seq, response: json!({ "entry": { "id": "t1" } }) }, today(), 3);
        assert!(log.borrow().contains(&Job::FetchTimeOff { resource: "r".into() }));
        let entry = TimeOff { id: "t1".into(), resource_id: "r".into(), from_date: "2026-10-19".into(), to_date: "2026-10-21".into(), reason: "Formation".into() };
        s.on_event(&mut ed, Event::TimeOff { resource: "r".into(), entries: Some(vec![entry]) }, today(), 4);
        assert_eq!(ed.as_ref().and_then(|e| e.resources.time_off.get("r")).map(|v| v[0].id.as_str()), Some("t1"));
        s.on_event(&mut ed, Event::ResourceSide { members: Some(Vec::new()), currency: Some("€".into()), default_hourly_rate: Some(50.0) }, today(), 5);
        assert_eq!(ed.as_ref().map(|e| e.resources.currency_code()), Some("EUR".to_string()));
    }
}
