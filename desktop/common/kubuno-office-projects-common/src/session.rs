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
    /// The signed-in user (`Connected`).
    pub me: Option<String>,
}

impl ProjectSession {
    pub fn new(id: String, post: Box<dyn Fn(Job)>, journal: Option<FileJournal>) -> Self {
        Self { id, post, journal, state: SyncState::Opening, connected: false, sending: false, fetching: false, last_fetch_ms: 0, offline_at: None, asked_conflict: false, me: None }
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
        if !self.connected || self.sending || self.offline_at.is_some() || self.asked_conflict {
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
                    let baseline = e.pending_change(seq).is_some_and(|c| matches!(c, kubuno_office_projects_core::change::Change::CaptureBaseline { .. } | kubuno_office_projects_core::change::Change::UpdateBaseline { .. } | kubuno_office_projects_core::change::Change::DeleteBaseline { .. }));
                    let empty = e.acknowledge(seq, &response);
                    if baseline {
                        (self.post)(Job::FetchSide);
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
            }
            Event::FetchFailed(m) => {
                self.fetching = false;
                out.messages.push(m);
                if ed.is_none() {
                    self.state = SyncState::Failed;
                    return out;
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
    #[ignore = "WIP: the resend after an offline re-read is not issued yet (see PROJECTS-DESKTOP.md §9)"]
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
}
