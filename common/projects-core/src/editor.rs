//! The project editor: the project as the server last sent it, the changes made since (shown at once, sent in
//! order, kept across a crash or an offline spell), the undo history, and the commands of the web editor
//! (`ProjectEditorPage.tsx`) — each one the same REST calls the web makes, with the same inverses.
//!
//! **Outbox.** A command applies its changes to the shown copy ([`Editor::data`]) and queues them. A platform
//! thread sends them one at a time ([`Editor::next_outgoing`]) and reports back ([`Editor::acknowledge`],
//! [`Editor::reject`], [`Editor::offline`]); when the queue is empty it re-reads the project
//! ([`Editor::rebase`]), as the web's `refresh()` does after every mutation. Rows created locally carry a
//! temporary id until the server answers with the real one.
//!
//! **History** (`history/useEditHistory.ts`). Entries are inverse REST calls, not snapshots: the web editor has
//! no document to snapshot. Ids are resolved through a remap table when an entry runs, because undoing a
//! deletion re-creates the rows under new ids. 100 entries; a failed inverse drops the whole history (the
//! web's rule); a continuous gesture on the same fields of the same task within 900 ms is one entry.
//!
//! **Conflicts** (a desktop addition — the web writes last-wins). Each queued field edit remembers the value it
//! replaced; when the project is re-read before the edit was sent (offline, or someone else edited
//! meanwhile) and that value changed on the server, the queue stops on [`Outgoing::Conflict`] until the user
//! keeps their version or takes the server's.

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::calendar::{self, WorkingCalendar};
use crate::change::{iso_from_ms, Change, HttpCall, Refused, ScheduleContext, TEMP_PREFIX};
use crate::date::Date;
use crate::gantt::BarCommit;
use crate::model::{Baseline, Calendars, ProjectData, Settings, Task, Version};
use crate::view;

/// Fields edited as a continuous stream (typing, a slider, a spinner): one history entry per gesture.
pub const COALESCED_FIELDS: [&str; 4] = ["name", "description", "progress", "duration_days"];
/// Idle time after which a continuous gesture is finished (`COALESCE_MS`).
pub const COALESCE_MS: i64 = 900;
/// History depth (`useEditHistory(limit = 100)`).
pub const HISTORY_LIMIT: usize = 100;

/// One step of a history entry.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// A REST call; an `optional` one may fail without failing the entry (the web's `try { } catch {}`).
    Call { change: Change, optional: bool },
    /// `saveProjectProps(patch)`: the project's dates, with the pins it computes when it runs.
    ProjectProps { patch: Map<String, Value> },
}

impl Step {
    fn call(change: Change) -> Self {
        Step::Call { change, optional: false }
    }
    fn optional(change: Change) -> Self {
        Step::Call { change, optional: true }
    }
}

/// One undoable action.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub label: String,
    pub undo: Vec<Step>,
    pub redo: Vec<Step>,
}

/// A change not yet acknowledged by the server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pending {
    pub seq: u64,
    /// The command it belongs to: a refused change drops the rest of its command.
    pub txn: u64,
    pub change: Change,
    /// The values its fields had when it was made (field edits only).
    #[serde(default)]
    pub expect: Map<String, Value>,
    #[serde(skip)]
    pub in_flight: bool,
}

/// What the outbox asks the platform to do next.
#[derive(Debug, Clone, PartialEq)]
pub enum Outgoing {
    /// Send this call; report with the same `seq`.
    Send { seq: u64, call: HttpCall },
    /// The queue is empty: nothing to send.
    Idle,
    /// A call is already on its way.
    Busy,
    /// Edits made on stale data meet newer server values: ask the user ([`Editor::resolve_conflict`]).
    Conflict(Vec<String>),
}

/// The journal of an open project: what was last read and what is not saved yet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Journal {
    pub base: ProjectData,
    #[serde(default)]
    pub pending: Vec<Pending>,
    #[serde(default)]
    pub remap: HashMap<String, String>,
    #[serde(default)]
    pub next_temp: u64,
}

/// Where a dragged row lands (`dropHint.place`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Before,
    After,
    Inside,
}

/// The project editor (see the module doc).
#[derive(Debug, Clone)]
pub struct Editor {
    /// The project as the server last sent it.
    pub base: ProjectData,
    /// What the user sees: `base` with the pending changes applied.
    data: ProjectData,
    pending: VecDeque<Pending>,
    undo_stack: Vec<Entry>,
    redo_stack: Vec<Entry>,
    remap: HashMap<String, String>,
    /// The running continuous gesture: its key and when it last fed the top entry.
    live: Option<(String, i64)>,
    /// The scheduler's context (today, the project calendar, the instance default).
    pub ctx: ScheduleContext,
    pub calendars: Option<Calendars>,
    pub settings: Settings,
    pub baselines: Vec<Baseline>,
    pub versions: Vec<Version>,
    next_seq: u64,
    next_txn: u64,
    next_temp: u64,
    now_ms: i64,
    revision: u64,
    /// Changes refused by the server or by the local checks, for the window to show.
    pub messages: Vec<String>,
    /// Pending edits the last rebase found in conflict (their `seq`).
    conflicts: HashSet<u64>,
}

impl Editor {
    pub fn new(data: ProjectData, today: Date, include_weekends_default: bool) -> Self {
        let mut e = Self {
            base: data.clone(),
            data,
            pending: VecDeque::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            remap: HashMap::new(),
            live: None,
            ctx: ScheduleContext { today, calendar: None, include_weekends_default },
            calendars: None,
            settings: Settings::default(),
            baselines: Vec::new(),
            versions: Vec::new(),
            next_seq: 1,
            next_txn: 1,
            next_temp: 1,
            now_ms: 0,
            revision: 1,
            messages: Vec::new(),
            conflicts: HashSet::new(),
        };
        e.refresh_calendar();
        e
    }

    /// Re-opens a journaled project: its last read copy and the changes not yet saved.
    pub fn restore(journal: Journal, today: Date, include_weekends_default: bool) -> Self {
        let mut e = Self::new(journal.base, today, include_weekends_default);
        e.remap = journal.remap;
        e.next_temp = journal.next_temp.max(1);
        e.next_seq = journal.pending.iter().map(|p| p.seq + 1).max().unwrap_or(1);
        e.next_txn = journal.pending.iter().map(|p| p.txn + 1).max().unwrap_or(1);
        e.pending = journal.pending.into_iter().map(|mut p| {
            p.in_flight = false;
            p
        }).collect();
        e.rebuild();
        e
    }

    /// The journal to keep on disk.
    pub fn journal(&self) -> Journal {
        Journal { base: self.base.clone(), pending: self.pending.iter().cloned().collect(), remap: self.remap.clone(), next_temp: self.next_temp }
    }

    /// The project as shown.
    pub fn data(&self) -> &ProjectData {
        &self.data
    }

    /// Bumped by every change of what is shown.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The clock, in Unix milliseconds (gesture coalescing, the creation stamp of new rows).
    pub fn set_now(&mut self, ms: i64) {
        self.now_ms = ms;
    }

    pub fn project_start(&self) -> Date {
        self.data.project_start(self.ctx.today)
    }

    /// Sets the side lists and the calendar the scheduler uses.
    pub fn set_calendars(&mut self, c: Option<Calendars>) {
        self.calendars = c;
        self.refresh_calendar();
    }

    fn refresh_calendar(&mut self) {
        let start = self.project_start();
        self.ctx.calendar = Some(calendar::for_schedule(self.calendars.as_ref(), start, self.ctx.include_weekends_default));
    }

    /// The calendar the Gantt predicts drops with (`workCal`).
    pub fn prediction_calendar(&self) -> WorkingCalendar {
        calendar::for_prediction(self.calendars.as_ref(), self.project_start(), self.ctx.include_weekends_default)
    }

    /// The queued change `seq`, if it is still queued.
    pub fn pending_change(&self, seq: u64) -> Option<&Change> {
        self.pending.iter().find(|p| p.seq == seq).map(|p| &p.change)
    }

    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// The current id of a row an undo or the server may have renamed (`ctx.resolve`).
    pub fn resolve(&self, id: &str) -> String {
        let mut cur = id.to_string();
        for _ in 0..16 {
            match self.remap.get(&cur) {
                Some(n) if *n != cur => cur = n.clone(),
                _ => break,
            }
        }
        cur
    }

    fn remap_id(&mut self, old: &str, new: &str) {
        if old != new {
            self.remap.insert(old.to_string(), new.to_string());
        }
    }

    fn mint(&mut self) -> String {
        let id = format!("{TEMP_PREFIX}{}", self.next_temp);
        self.next_temp += 1;
        id
    }

    fn now_iso(&self) -> String {
        iso_from_ms(self.now_ms)
    }

    // ── Applying ────────────────────────────────────────────────────────────

    /// The values a field edit replaces (`expect`).
    fn expectation(&self, change: &Change) -> Map<String, Value> {
        let (current, patch) = match change {
            Change::UpdateTask { id, patch } => (self.data.task(id).and_then(|t| serde_json::to_value(t).ok()), patch),
            Change::UpdateProject { patch } => (serde_json::to_value(&self.data.project).ok(), patch),
            _ => return Map::new(),
        };
        let Some(cur) = current else { return Map::new() };
        patch.keys().map(|k| (k.clone(), cur.get(k).cloned().unwrap_or(Value::Null))).collect()
    }

    /// Applies one change to the shown copy and queues it.
    fn push_change(&mut self, change: Change, txn: u64) -> Result<(), Refused> {
        let expect = self.expectation(&change);
        let now = self.now_iso();
        change.apply(&mut self.data, &self.ctx, &now)?;
        if matches!(change, Change::UpdateProject { .. }) {
            self.refresh_calendar();
        }
        let seq = self.next_seq;
        self.next_seq += 1;
        self.pending.push_back(Pending { seq, txn, change, expect, in_flight: false });
        self.revision += 1;
        Ok(())
    }

    /// `saveProjectProps(patch)` (`ProjectEditorPage.tsx:585-618`): moving the start EARLIER pins every free
    /// (ASAP/ALAP) leaf at its current date (SNET) so the plan stays where it is; a date change re-runs the
    /// scheduler.
    fn project_props_changes(&self, patch: &Map<String, Value>) -> Vec<Step> {
        let mut steps = vec![Step::call(Change::UpdateProject { patch: patch.clone() })];
        if let Some(new_start) = patch.get("start_date") {
            let old = self.project_start();
            let earlier = new_start.as_str().and_then(Date::parse).is_some_and(|d| d.days_since(old) < 0);
            if earlier {
                for t in &self.data.tasks {
                    if t.is_summary_type() || self.data.tasks.iter().any(|x| x.parent_id.as_deref() == Some(t.id.as_str())) {
                        continue;
                    }
                    if t.constraint_type == "ASAP" || t.constraint_type == "ALAP" {
                        let date = old.add_days(t.es() as i64).iso();
                        let p = obj(vec![("constraint_type", json!("SNET")), ("constraint_date", json!(date)), ("start_date", json!(date))]);
                        steps.push(Step::optional(Change::UpdateTask { id: t.id.clone(), patch: p }));
                    }
                }
            }
        }
        if patch.contains_key("start_date") || patch.contains_key("end_date") {
            steps.push(Step::optional(Change::ComputeCpm));
        }
        steps
    }

    /// Runs history steps: ids resolved, created rows given fresh temporary ids (remapped from the ids the
    /// steps knew), each change applied and queued. A non-optional refusal stops and fails.
    fn run_steps(&mut self, steps: &[Step], txn: u64) -> Result<(), Refused> {
        for step in steps {
            match step {
                Step::ProjectProps { patch } => {
                    let expanded = self.project_props_changes(patch);
                    self.run_steps(&expanded, txn)?;
                }
                Step::Call { change, optional } => {
                    let mut c = change.clone();
                    if let Some((own, _)) = c.created_id() {
                        let old = self.resolve(own);
                        let fresh = self.mint();
                        self.remap_id(&old, &fresh);
                        let own = own.to_string();
                        self.remap_id(&own, &fresh);
                    }
                    let remap = self.remap.clone();
                    let resolve = |id: &str| -> Option<String> {
                        let mut cur = id.to_string();
                        for _ in 0..16 {
                            match remap.get(&cur) {
                                Some(n) if *n != cur => cur = n.clone(),
                                _ => break,
                            }
                        }
                        (cur != id).then_some(cur)
                    };
                    let mut resolve = resolve;
                    c.map_ids(&mut resolve);
                    match self.push_change(c, txn) {
                        Ok(()) => {}
                        Err(_) if *optional => {}
                        Err(e) => return Err(e),
                    }
                }
            }
        }
        Ok(())
    }

    /// Runs a command: its steps now, its entry on the history (a fresh action clears the redo branch).
    fn perform(&mut self, label: &str, redo: Vec<Step>, undo: Vec<Step>) -> Result<(), Refused> {
        let txn = self.begin_txn();
        if let Err(e) = self.run_steps(&redo, txn) {
            self.abort_txn(txn);
            self.messages.push(e.to_string());
            return Err(e);
        }
        self.live = None;
        self.push_entry(Entry { label: label.to_string(), undo, redo });
        Ok(())
    }

    fn push_entry(&mut self, entry: Entry) {
        self.undo_stack.push(entry);
        if self.undo_stack.len() > HISTORY_LIMIT {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    /// Runs changes that the web does not put on the history (project settings, resources, baselines…).
    fn run_unrecorded(&mut self, changes: Vec<Change>) -> Result<(), Refused> {
        let txn = self.begin_txn();
        for c in changes {
            if let Err(e) = self.push_change(c, txn) {
                self.abort_txn(txn);
                self.messages.push(e.to_string());
                return Err(e);
            }
        }
        Ok(())
    }

    fn begin_txn(&mut self) -> u64 {
        let t = self.next_txn;
        self.next_txn += 1;
        t
    }

    /// Drops what a failed command already queued and shows the rest again.
    fn abort_txn(&mut self, txn: u64) {
        self.pending.retain(|p| p.txn != txn || p.in_flight);
        self.rebuild();
    }

    /// `base` + the pending changes; those that no longer apply are dropped (the server would refuse them).
    fn rebuild(&mut self) {
        let mut d = self.base.clone();
        let now = self.now_iso();
        self.refresh_calendar_for(&d);
        let ctx = self.ctx.clone();
        let mut dropped = Vec::new();
        for p in self.pending.iter() {
            if let Err(e) = p.change.apply(&mut d, &ctx, &now) {
                if !p.in_flight {
                    dropped.push((p.seq, e));
                }
            }
        }
        if !dropped.is_empty() {
            let gone: HashSet<u64> = dropped.iter().map(|(s, _)| *s).collect();
            self.pending.retain(|p| !gone.contains(&p.seq));
            for (_, e) in dropped {
                self.messages.push(e.to_string());
            }
        }
        self.data = d;
        self.refresh_calendar();
        self.revision += 1;
    }

    fn refresh_calendar_for(&mut self, d: &ProjectData) {
        let start = d.project_start(self.ctx.today);
        self.ctx.calendar = Some(calendar::for_schedule(self.calendars.as_ref(), start, self.ctx.include_weekends_default));
    }

    // ── Outbox ──────────────────────────────────────────────────────────────

    /// The next call to send (one at a time, in order).
    pub fn next_outgoing(&mut self) -> Outgoing {
        let id = self.data.project.id.clone();
        let Some(front) = self.pending.front_mut() else { return Outgoing::Idle };
        if front.in_flight {
            return Outgoing::Busy;
        }
        if !self.conflicts.is_empty() {
            let names = self.conflict_names();
            return Outgoing::Conflict(names);
        }
        let Some(front) = self.pending.front_mut() else { return Outgoing::Idle };
        front.in_flight = true;
        Outgoing::Send { seq: front.seq, call: front.change.request(&id) }
    }

    fn conflict_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        for p in &self.pending {
            if !self.conflicts.contains(&p.seq) {
                continue;
            }
            let n = match &p.change {
                Change::UpdateTask { id, .. } => self.data.task(id).map(|t| t.name.clone()).unwrap_or_default(),
                Change::UpdateProject { .. } => self.data.project.title.clone(),
                _ => String::new(),
            };
            if !n.is_empty() && !names.contains(&n) {
                names.push(n);
            }
        }
        names
    }

    /// The server accepted call `seq` with `response`. Returns `true` when the queue is now empty (time to
    /// re-read the project).
    pub fn acknowledge(&mut self, seq: u64, response: &Value) -> bool {
        let Some(pos) = self.pending.iter().position(|p| p.seq == seq) else { return self.pending.is_empty() };
        let Some(p) = self.pending.remove(pos) else { return self.pending.is_empty() };
        if let Some((temp, key)) = p.change.created_id() {
            if let Some(real) = response.get(key).and_then(|o| o.get("id")).and_then(Value::as_str) {
                let (temp, real) = (temp.to_string(), real.to_string());
                self.remap_id(&temp, &real);
                for q in self.pending.iter_mut() {
                    q.change.rename_id(&temp, &real);
                }
                rename_in_data(&mut self.data, &temp, &real);
                self.revision += 1;
            }
        }
        if matches!(p.change, Change::CaptureBaseline { .. } | Change::UpdateBaseline { .. } | Change::DeleteBaseline { .. }) {
            self.revision += 1;
        }
        self.pending.is_empty()
    }

    /// The server refused call `seq` (a 4xx): it and the rest of its command are dropped, the history is
    /// cleared (a half-applied inverse would desync it), the message is kept for the window.
    pub fn reject(&mut self, seq: u64, message: &str) {
        let txn = self.pending.iter().find(|p| p.seq == seq).map(|p| p.txn);
        if let Some(txn) = txn {
            self.pending.retain(|p| p.txn != txn);
        }
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.live = None;
        self.messages.push(message.to_string());
        self.rebuild();
    }

    /// Call `seq` could not reach the server: it stays queued (sent again later, in order).
    pub fn offline(&mut self, seq: u64) {
        if let Some(p) = self.pending.iter_mut().find(|p| p.seq == seq) {
            p.in_flight = false;
        }
    }

    /// The project as the server now has it (the `refresh()` of the web): pending changes are re-applied on
    /// top; edits whose replaced value changed on the server meanwhile are flagged as conflicts.
    pub fn rebase(&mut self, fetched: ProjectData) {
        self.base = fetched;
        // Expectations are checked against the server copy with the earlier pending changes applied.
        let mut d = self.base.clone();
        let now = self.now_iso();
        let ctx = self.ctx.clone();
        self.conflicts.clear();
        for p in self.pending.iter() {
            if !p.expect.is_empty() && !p.in_flight {
                let cur = match &p.change {
                    Change::UpdateTask { id, .. } => d.task(id).and_then(|t| serde_json::to_value(t).ok()),
                    Change::UpdateProject { .. } => serde_json::to_value(&d.project).ok(),
                    _ => None,
                };
                if let Some(cur) = cur {
                    if p.expect.iter().any(|(k, v)| !same_value(cur.get(k).unwrap_or(&Value::Null), v)) {
                        self.conflicts.insert(p.seq);
                    }
                }
            }
            let _ = p.change.apply(&mut d, &ctx, &now);
        }
        self.rebuild();
    }

    /// The user's answer to [`Outgoing::Conflict`]: keep their edits (sent as they are), or take the server's
    /// (every unsent change dropped, the history cleared).
    pub fn resolve_conflict(&mut self, keep_mine: bool) {
        if keep_mine {
            // Accepted as they are: the same server values will not stop them again.
            let conflicts = std::mem::take(&mut self.conflicts);
            for p in self.pending.iter_mut().filter(|p| conflicts.contains(&p.seq)) {
                p.expect.clear();
            }
            return;
        }
        self.conflicts.clear();
        self.pending.retain(|p| p.in_flight);
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.rebuild();
    }

    /// Messages to show, emptied.
    pub fn take_messages(&mut self) -> Vec<String> {
        std::mem::take(&mut self.messages)
    }

    // ── History ─────────────────────────────────────────────────────────────

    pub fn undo(&mut self) {
        self.live = None;
        let Some(entry) = self.undo_stack.pop() else { return };
        let txn = self.begin_txn();
        match self.run_steps(&entry.undo, txn) {
            Ok(()) => self.redo_stack.push(entry),
            Err(e) => {
                self.undo_stack.clear();
                self.redo_stack.clear();
                self.messages.push(e.to_string());
            }
        }
    }

    pub fn redo(&mut self) {
        self.live = None;
        let Some(entry) = self.redo_stack.pop() else { return };
        let txn = self.begin_txn();
        match self.run_steps(&entry.redo, txn) {
            Ok(()) => self.undo_stack.push(entry),
            Err(e) => {
                self.undo_stack.clear();
                self.redo_stack.clear();
                self.messages.push(e.to_string());
            }
        }
    }

    /// Closes the running gesture (a discrete action, a focus change).
    pub fn end_gesture(&mut self) {
        self.live = None;
    }

    // ── Task commands ───────────────────────────────────────────────────────

    /// `patchTaskCmd`: edits fields of a task and stacks their restoration; a stream on the continuous
    /// fields of one task coalesces.
    pub fn patch_task(&mut self, task_id: &str, patch: Map<String, Value>, label: &str) -> Result<(), Refused> {
        let Some(cur) = self.data.task(task_id).and_then(|t| serde_json::to_value(t).ok()) else { return Err(Refused::NotFound) };
        if patch.is_empty() {
            return Ok(());
        }
        let before: Map<String, Value> = patch.keys().map(|k| (k.clone(), cur.get(k).cloned().unwrap_or(Value::Null))).collect();
        let coalesce = patch.keys().all(|k| COALESCED_FIELDS.contains(&k.as_str()));
        let key = coalesce.then(|| format!("task:{task_id}:{}", patch.keys().cloned().collect::<Vec<_>>().join(",")));
        let change = Change::UpdateTask { id: task_id.to_string(), patch: patch.clone() };
        if let (Some(k), Some((live_key, at))) = (&key, &self.live) {
            if k == live_key && self.now_ms - at < COALESCE_MS {
                let txn = self.begin_txn();
                if let Err(e) = self.push_change(change, txn) {
                    self.messages.push(e.to_string());
                    return Err(e);
                }
                if let Some(Entry { redo, .. }) = self.undo_stack.last_mut() {
                    if let Some(Step::Call { change: Change::UpdateTask { patch: after, .. }, .. }) = redo.first_mut() {
                        for (k, v) in patch {
                            after.insert(k, v);
                        }
                    }
                }
                self.live = Some((k.clone(), self.now_ms));
                return Ok(());
            }
        }
        self.perform(label, vec![Step::call(change)], vec![Step::call(Change::UpdateTask { id: task_id.to_string(), patch: before })])?;
        if let Some(k) = key {
            self.live = Some((k, self.now_ms));
        }
        Ok(())
    }

    /// One field (`patchTaskCmd(id, { field: value })`).
    pub fn set_task_field(&mut self, task_id: &str, field: &str, value: Value) -> Result<(), Refused> {
        self.patch_task(task_id, obj(vec![(field, value)]), "task-edit")
    }

    /// `createTaskCmd`: a new task (« Nouvelle tâche ») — under `parent`, of `task_type`, at `position` (an
    /// index the web takes from the table; 0 means « at the end »). Returns its (temporary) id.
    pub fn create_task(&mut self, parent: Option<&str>, task_type: Option<&str>, position: Option<i32>) -> Result<String, Refused> {
        let mut body = Map::new();
        if let Some(p) = parent {
            body.insert("parent_id".into(), json!(p));
        }
        if let Some(t) = task_type {
            body.insert("task_type".into(), json!(t));
        }
        if let Some(p) = position {
            body.insert("position".into(), json!(p));
        }
        let seed = self.mint();
        self.perform("task-create", vec![Step::call(Change::CreateTask { id: seed.clone(), body })], vec![Step::call(Change::DeleteTask { id: seed.clone() })])?;
        Ok(self.resolve(&seed))
    }

    /// The ribbon's « Tâche / Jalon / Récapitulatif » (`insertTask`): after the selected row of the stored
    /// order (`position: selIndex + 1`).
    pub fn insert_task(&mut self, selected: Option<&str>, task_type: &str) -> Result<String, Refused> {
        let pos = selected.and_then(|s| self.data.task_index(s)).map(|i| i as i32 + 1);
        self.create_task(None, Some(task_type), pos)
    }

    /// The fields `recreateTask` restores (`TaskSnapshot`).
    fn recreate_steps(t: &Task, parent: Option<&str>) -> Vec<Step> {
        let mut body = Map::new();
        body.insert("name".into(), json!(t.name));
        if let Some(p) = parent {
            body.insert("parent_id".into(), json!(p));
        }
        body.insert("position".into(), json!(t.position));
        body.insert("task_type".into(), json!(t.task_type));
        if let Some(s) = &t.start_date {
            body.insert("start_date".into(), json!(s));
        }
        body.insert("duration_days".into(), json!(t.duration_days));
        let mut patch = Map::new();
        patch.insert("description".into(), json!(t.description));
        patch.insert("status".into(), json!(t.status));
        patch.insert("priority".into(), json!(t.priority));
        if let Some(e) = &t.end_date {
            patch.insert("end_date".into(), json!(e));
        }
        patch.insert("progress".into(), json!(t.progress));
        patch.insert("position".into(), json!(t.position));
        patch.insert("wbs".into(), json!(t.wbs));
        vec![Step::call(Change::CreateTask { id: t.id.clone(), body }), Step::call(Change::UpdateTask { id: t.id.clone(), patch })]
    }

    /// `deleteTaskCmd`: the task, its subtree, the links touching them and their assignments; the undo
    /// re-creates everything (parents first, then links, then assignments) and re-runs the scheduler.
    pub fn delete_task(&mut self, task_id: &str) -> Result<(), Refused> {
        let Some(_) = self.data.task(task_id) else { return Err(Refused::NotFound) };
        // Pre-order: the task, then its children in stored order, recursively (`collect`).
        let mut subtree: Vec<Task> = Vec::new();
        fn collect(data: &ProjectData, id: &str, out: &mut Vec<Task>, depth: usize) {
            if depth > 10_000 {
                return;
            }
            let Some(t) = data.task(id) else { return };
            if out.iter().any(|x| x.id == t.id) {
                return;
            }
            out.push(t.clone());
            for c in data.tasks.iter().filter(|x| x.parent_id.as_deref() == Some(id)) {
                collect(data, &c.id, out, depth + 1);
            }
        }
        collect(&self.data, task_id, &mut subtree, 0);
        let gone: HashSet<&str> = subtree.iter().map(|t| t.id.as_str()).collect();
        let mut undo = Vec::new();
        for t in &subtree {
            undo.extend(Self::recreate_steps(t, t.parent_id.as_deref()));
        }
        for d in self.data.dependencies.iter().filter(|d| gone.contains(d.from_task_id.as_str()) || gone.contains(d.to_task_id.as_str())) {
            undo.push(Step::optional(Change::CreateDependency { id: d.id.clone(), from: d.from_task_id.clone(), to: d.to_task_id.clone(), dep_type: Some(d.dep_type.clone()), lag_days: Some(d.lag_days) }));
        }
        for a in self.data.assignments.iter().filter(|a| gone.contains(a.task_id.as_str())) {
            undo.push(Step::optional(Change::Assign { id: a.id.clone(), task: a.task_id.clone(), resource: a.resource_id.clone(), units: Some(a.units) }));
        }
        undo.push(Step::optional(Change::ComputeCpm));
        self.perform("task-delete", vec![Step::call(Change::DeleteTask { id: task_id.to_string() })], undo)
    }

    /// `duplicateTask`: a copy « (copie) » right after the task, without its outline code nor its dates.
    pub fn duplicate_task(&mut self, task_id: &str, copy_word: &str) -> Result<String, Refused> {
        let Some(src) = self.data.task(task_id).cloned() else { return Err(Refused::NotFound) };
        let idx = self.data.task_index(task_id).unwrap_or(0);
        let mut copy = src.clone();
        copy.id = self.mint();
        copy.name = format!("{} ({copy_word})", src.name);
        copy.position = idx as i32 + 1;
        copy.wbs = String::new();
        copy.start_date = None;
        copy.end_date = None;
        let redo = Self::recreate_steps(&copy, src.parent_id.as_deref());
        self.perform("task-duplicate", redo, vec![Step::call(Change::DeleteTask { id: copy.id.clone() })])?;
        Ok(self.resolve(&copy.id))
    }

    /// `setStatus`: « Terminé » also sets 100 % — but never on a parent, whose % is rolled up.
    pub fn set_status(&mut self, task_id: &str, status: &str) -> Result<(), Refused> {
        let mut patch = obj(vec![("status", json!(status))]);
        if status == "completed" && !self.is_parent(task_id) {
            patch.insert("progress".into(), json!(100));
        }
        self.patch_task(task_id, patch, "task-edit")
    }

    pub fn set_priority(&mut self, task_id: &str, priority: &str) -> Result<(), Refused> {
        self.patch_task(task_id, obj(vec![("priority", json!(priority))]), "task-edit")
    }

    pub fn set_progress(&mut self, task_id: &str, progress: i32) -> Result<(), Refused> {
        self.patch_task(task_id, obj(vec![("progress", json!(progress))]), "task-edit")
    }

    /// The completion ring of a row (`TaskRow`): done ↔ not started.
    pub fn toggle_done(&mut self, task_id: &str) -> Result<(), Refused> {
        let done = self.data.task(task_id).is_some_and(|t| t.progress >= 100);
        let patch = if done { obj(vec![("progress", json!(0)), ("status", json!("not_started"))]) } else { obj(vec![("progress", json!(100)), ("status", json!("completed"))]) };
        self.patch_task(task_id, patch, "task-edit")
    }

    pub fn is_parent(&self, task_id: &str) -> bool {
        self.data.tasks.iter().any(|t| t.parent_id.as_deref() == Some(task_id))
    }

    /// `indentTask`: under the nearest preceding sibling.
    pub fn indent(&mut self, task_id: &str) -> Result<(), Refused> {
        let Some(idx) = self.data.task_index(task_id) else { return Err(Refused::NotFound) };
        let parent = self.data.tasks[idx].parent_id.clone();
        for i in (0..idx).rev() {
            if self.data.tasks[i].parent_id == parent {
                let new_parent = self.data.tasks[i].id.clone();
                return self.patch_task(task_id, obj(vec![("parent_id", json!(new_parent))]), "task-indent");
            }
        }
        Ok(())
    }

    /// `outdentTask`: to the grandparent.
    pub fn outdent(&mut self, task_id: &str) -> Result<(), Refused> {
        let Some(me) = self.data.task(task_id) else { return Err(Refused::NotFound) };
        let Some(pid) = me.parent_id.clone() else { return Ok(()) };
        let gp = self.data.task(&pid).and_then(|p| p.parent_id.clone());
        self.patch_task(task_id, obj(vec![("parent_id", gp.map(Value::String).unwrap_or(Value::Null))]), "task-outdent")
    }

    /// `moveTask`: swaps the position with the neighbour in the stored order (one entry).
    pub fn move_task(&mut self, task_id: &str, up: bool) -> Result<(), Refused> {
        let Some(idx) = self.data.task_index(task_id) else { return Err(Refused::NotFound) };
        let j = if up { idx.checked_sub(1) } else { Some(idx + 1) };
        let Some(j) = j.filter(|j| *j < self.data.tasks.len()) else { return Ok(()) };
        let (me, other) = (self.data.tasks[idx].clone(), self.data.tasks[j].clone());
        let swap = |a: i32, b: i32| vec![Step::call(Change::UpdateTask { id: me.id.clone(), patch: obj(vec![("position", json!(a))]) }), Step::call(Change::UpdateTask { id: other.id.clone(), patch: obj(vec![("position", json!(b))]) })];
        self.perform("task-move", swap(other.position, me.position), swap(me.position, other.position))
    }

    /// `reorderTaskCmd`: moves a task with its subtree before/after a target, or inside it as its first child;
    /// the pre-order is renumbered (minimal position changes) and the parent set when it changes.
    pub fn reorder(&mut self, dragged: &str, target: &str, place: Place) -> Result<(), Refused> {
        if dragged == target {
            return Ok(());
        }
        let (Some(_), Some(t)) = (self.data.task(dragged), self.data.task(target).cloned()) else { return Err(Refused::NotFound) };
        let d = &self.data;
        let kids_of = |pid: Option<&str>| -> Vec<&Task> { d.tasks.iter().filter(|x| x.parent_id.as_deref() == pid).collect() };
        let mut flat: Vec<&Task> = Vec::new();
        fn walk<'a>(pid: Option<&str>, kids_of: &dyn Fn(Option<&str>) -> Vec<&'a Task>, flat: &mut Vec<&'a Task>, depth: usize) {
            if depth > 10_000 {
                return;
            }
            for t in kids_of(pid) {
                if flat.iter().any(|x| x.id == t.id) {
                    continue;
                }
                flat.push(t);
                walk(Some(&t.id), kids_of, flat, depth + 1);
            }
        }
        walk(None, &kids_of, &mut flat, 0);
        let block: HashSet<String> = crate::change::subtree(d, dragged).into_iter().collect();
        if block.contains(target) {
            return Ok(());
        }
        let moving: Vec<&Task> = flat.iter().copied().filter(|t| block.contains(&t.id)).collect();
        let rest: Vec<&Task> = flat.iter().copied().filter(|t| !block.contains(&t.id)).collect();
        let Some(ti) = rest.iter().position(|x| x.id == target) else { return Ok(()) };
        let insert_at = if place == Place::Before { ti } else { ti + 1 };
        let mut new_flat: Vec<&Task> = rest[..insert_at].to_vec();
        new_flat.extend(moving.iter().copied());
        new_flat.extend(rest[insert_at..].iter().copied());
        let new_parent = if place == Place::Inside { Some(target.to_string()) } else { t.parent_id.clone() };
        let mut redo = Vec::new();
        let mut undo = Vec::new();
        for (i, x) in new_flat.iter().enumerate() {
            let mut patch = Map::new();
            let mut before = Map::new();
            if x.position != i as i32 {
                patch.insert("position".into(), json!(i));
                before.insert("position".into(), json!(x.position));
            }
            if x.id == dragged && x.parent_id != new_parent {
                patch.insert("parent_id".into(), new_parent.clone().map(Value::String).unwrap_or(Value::Null));
                before.insert("parent_id".into(), x.parent_id.clone().map(Value::String).unwrap_or(Value::Null));
            }
            if !patch.is_empty() {
                redo.push(Step::call(Change::UpdateTask { id: x.id.clone(), patch }));
                undo.push(Step::call(Change::UpdateTask { id: x.id.clone(), patch: before }));
            }
        }
        if redo.is_empty() {
            return Ok(());
        }
        self.perform("task-reorder", redo, undo)
    }

    // ── Links ───────────────────────────────────────────────────────────────

    /// `addDepCmd`: a finish-to-start link; with `cpm`, the scheduler re-runs (Gantt link gesture, network).
    pub fn add_dependency(&mut self, from: &str, to: &str, cpm: bool) -> Result<(), Refused> {
        let seed = self.mint();
        let mut redo = vec![Step::call(Change::CreateDependency { id: seed.clone(), from: from.into(), to: to.into(), dep_type: None, lag_days: None })];
        let mut undo = vec![Step::call(Change::DeleteDependency { id: seed })];
        if cpm {
            redo.push(Step::optional(Change::ComputeCpm));
            undo.push(Step::optional(Change::ComputeCpm));
        }
        self.perform("dep-add", redo, undo)
    }

    /// « Lier » (`linkSelectedToPrev`): the selected task after the previous one in the stored order.
    pub fn link_to_previous(&mut self, task_id: &str) -> Result<(), Refused> {
        let Some(i) = self.data.task_index(task_id).filter(|i| *i > 0) else { return Ok(()) };
        let prev = self.data.tasks[i - 1].id.clone();
        self.add_dependency(&prev, task_id, false)
    }

    /// `setDepCmd` / `setDep`: a link's type or lag (the upsert with the previous values is the inverse);
    /// the scheduler re-runs. The inspector's lag field coalesces per link.
    pub fn set_dependency(&mut self, from: &str, to: &str, dep_type: &str, lag_days: i32) -> Result<(), Refused> {
        let Some(prev) = self.data.dependencies.iter().find(|d| d.from_task_id == from && d.to_task_id == to).cloned() else { return Err(Refused::NotFound) };
        let mk = |ty: &str, lag: i32| vec![Step::call(Change::CreateDependency { id: prev.id.clone(), from: from.into(), to: to.into(), dep_type: Some(ty.into()), lag_days: Some(lag) }), Step::optional(Change::ComputeCpm)];
        let key = format!("dep:{from}:{to}");
        if let Some((live, at)) = &self.live {
            if *live == key && self.now_ms - at < COALESCE_MS {
                let txn = self.begin_txn();
                for s in mk(dep_type, lag_days) {
                    if let Step::Call { change, .. } = s {
                        let _ = self.push_change(change, txn);
                    }
                }
                if let Some(Entry { redo, .. }) = self.undo_stack.last_mut() {
                    *redo = mk(dep_type, lag_days);
                }
                self.live = Some((key, self.now_ms));
                return Ok(());
            }
        }
        // The upsert keeps the row's id: no re-creation, so the steps must not mint a new id.
        let txn = self.begin_txn();
        for s in mk(dep_type, lag_days) {
            if let Step::Call { change, optional } = s {
                if let Err(e) = self.push_change(change, txn) {
                    if !optional {
                        self.abort_txn(txn);
                        return Err(e);
                    }
                }
            }
        }
        self.push_entry(Entry { label: "dep-set".into(), undo: mk(&prev.dep_type, prev.lag_days), redo: mk(dep_type, lag_days) });
        self.live = Some((key, self.now_ms));
        Ok(())
    }

    /// `removeDepsCmd`: several links at once (« Délier »: every predecessor link of the task).
    pub fn remove_dependencies(&mut self, ids: &[String]) -> Result<(), Refused> {
        let list: Vec<_> = self.data.dependencies.iter().filter(|d| ids.contains(&d.id)).cloned().collect();
        if list.is_empty() {
            return Ok(());
        }
        let redo = list.iter().map(|d| Step::call(Change::DeleteDependency { id: d.id.clone() })).collect();
        let undo = list.iter().map(|d| Step::call(Change::CreateDependency { id: d.id.clone(), from: d.from_task_id.clone(), to: d.to_task_id.clone(), dep_type: Some(d.dep_type.clone()), lag_days: Some(d.lag_days) })).collect();
        self.perform("dep-remove", redo, undo)
    }

    /// « Délier » (`unlinkTask`).
    pub fn unlink(&mut self, task_id: &str) -> Result<(), Refused> {
        let ids: Vec<String> = self.data.dependencies.iter().filter(|d| d.to_task_id == task_id).map(|d| d.id.clone()).collect();
        self.remove_dependencies(&ids)
    }

    /// `setPredecessors`: the « 1;3 » field, one entry for the whole set.
    pub fn set_predecessors(&mut self, task_id: &str, text: &str) -> Result<(), Refused> {
        let (removed, added) = view::predecessors_plan(&self.data, task_id, text);
        if removed.is_empty() && added.is_empty() {
            return Ok(());
        }
        let gone: Vec<_> = self.data.dependencies.iter().filter(|d| removed.contains(&d.id)).cloned().collect();
        let born: Vec<(String, String)> = added.iter().map(|f| (f.clone(), self.mint())).collect();
        let mut redo: Vec<Step> = gone.iter().map(|d| Step::call(Change::DeleteDependency { id: d.id.clone() })).collect();
        redo.extend(born.iter().map(|(from, id)| Step::call(Change::CreateDependency { id: id.clone(), from: from.clone(), to: task_id.into(), dep_type: None, lag_days: None })));
        let mut undo: Vec<Step> = born.iter().map(|(_, id)| Step::optional(Change::DeleteDependency { id: id.clone() })).collect();
        undo.extend(gone.iter().map(|d| Step::call(Change::CreateDependency { id: d.id.clone(), from: d.from_task_id.clone(), to: d.to_task_id.clone(), dep_type: Some(d.dep_type.clone()), lag_days: Some(d.lag_days) })));
        self.perform("predecessors", redo, undo)
    }

    // ── Resources on tasks ──────────────────────────────────────────────────

    pub fn assign(&mut self, task_id: &str, resource: &str) -> Result<(), Refused> {
        let seed = self.mint();
        self.perform("res-assign", vec![Step::call(Change::Assign { id: seed, task: task_id.into(), resource: resource.into(), units: None })], vec![Step::call(Change::Unassign { task: task_id.into(), resource: resource.into() })])
    }

    pub fn unassign(&mut self, task_id: &str, resource: &str) -> Result<(), Refused> {
        let units = self.data.assignments.iter().find(|a| a.task_id == task_id && a.resource_id == resource).map(|a| a.units).unwrap_or(1.0);
        let seed = self.mint();
        self.perform("res-unassign", vec![Step::call(Change::Unassign { task: task_id.into(), resource: resource.into() })], vec![Step::call(Change::Assign { id: seed, task: task_id.into(), resource: resource.into(), units: Some(units) })])
    }

    // ── The Gantt's gestures ────────────────────────────────────────────────

    /// Commits a bar drag (`onGanttUp`): the patches, then the scheduler; one entry per gesture.
    pub fn commit_bar(&mut self, task_id: &str, commit: &BarCommit) -> Result<(), Refused> {
        match commit {
            BarCommit::Nothing => Ok(()),
            BarCommit::Task { before, after, .. } => {
                let redo = vec![Step::call(Change::UpdateTask { id: task_id.into(), patch: after.clone() }), Step::optional(Change::ComputeCpm)];
                let undo = vec![Step::call(Change::UpdateTask { id: task_id.into(), patch: before.clone() }), Step::optional(Change::ComputeCpm)];
                let label = if after.contains_key("start_date") && !after.contains_key("duration_days") { "gantt-move" } else { "gantt-resize" };
                self.perform(label, redo, undo)
            }
            BarCommit::Phase { changes, .. } => {
                let mut redo: Vec<Step> = changes.iter().map(|(id, after, _)| Step::call(Change::UpdateTask { id: id.clone(), patch: after.clone() })).collect();
                let mut undo: Vec<Step> = changes.iter().map(|(id, _, before)| Step::call(Change::UpdateTask { id: id.clone(), patch: before.clone() })).collect();
                redo.push(Step::optional(Change::ComputeCpm));
                undo.push(Step::optional(Change::ComputeCpm));
                self.perform("gantt-move-phase", redo, undo)
            }
        }
    }

    /// Commits a boundary marker drag: the project's planned start or target end moves to `day` (an offset
    /// from the current start); undoable (`project-start` / `project-end`).
    pub fn commit_marker(&mut self, start: bool, day: i32) -> Result<(), Refused> {
        let date = self.project_start().add_days(day as i64).iso();
        let (key, before) = if start { ("start_date", self.data.project.start_date.clone()) } else { ("end_date", self.data.project.end_date.clone()) };
        let after = obj(vec![(key, json!(date))]);
        let before = obj(vec![(key, before.map(Value::String).unwrap_or(Value::Null))]);
        self.perform(if start { "project-start" } else { "project-end" }, vec![Step::ProjectProps { patch: after }], vec![Step::ProjectProps { patch: before }])
    }

    // ── Not on the history (the web uses plain mutations) ───────────────────

    /// The properties panel and the title (`saveProjectProps` / `updateProjectMut`).
    pub fn save_project_props(&mut self, patch: Map<String, Value>) -> Result<(), Refused> {
        let txn = self.begin_txn();
        let steps = self.project_props_changes(&patch);
        if let Err(e) = self.run_steps(&steps, txn) {
            self.abort_txn(txn);
            self.messages.push(e.to_string());
            return Err(e);
        }
        Ok(())
    }

    /// « Replanifier » (`computeCpmMut`).
    pub fn compute_cpm(&mut self) -> Result<(), Refused> {
        self.run_unrecorded(vec![Change::ComputeCpm])
    }

    pub fn set_starred(&mut self, starred: bool) -> Result<(), Refused> {
        self.run_unrecorded(vec![Change::UpdateProject { patch: obj(vec![("is_starred", json!(starred))]) }])
    }

    /// A resource: a directory member (`user_id`, the member's colour) or an external name.
    pub fn create_resource(&mut self, name: &str, user_id: Option<&str>, color: Option<&str>) -> Result<String, Refused> {
        let mut body = obj(vec![("name", json!(name))]);
        if let Some(u) = user_id {
            body.insert("user_id".into(), json!(u));
        }
        if let Some(c) = color {
            body.insert("color".into(), json!(c));
        }
        let id = self.mint();
        self.run_unrecorded(vec![Change::CreateResource { id: id.clone(), body }])?;
        Ok(id)
    }

    pub fn update_resource(&mut self, id: &str, patch: Map<String, Value>) -> Result<(), Refused> {
        self.run_unrecorded(vec![Change::UpdateResource { id: id.into(), patch }])
    }

    pub fn delete_resource(&mut self, id: &str) -> Result<(), Refused> {
        self.run_unrecorded(vec![Change::DeleteResource { id: id.into() }])
    }

    pub fn capture_baseline(&mut self, name: Option<&str>) -> Result<(), Refused> {
        self.run_unrecorded(vec![Change::CaptureBaseline { name: name.map(str::to_string) }])
    }

    pub fn update_baseline(&mut self, id: &str, patch: Map<String, Value>) -> Result<(), Refused> {
        if let Some(b) = self.baselines.iter_mut().find(|b| b.id == id) {
            if let Some(n) = patch.get("name").and_then(Value::as_str) {
                b.name = n.to_string();
            }
            if patch.get("is_primary").and_then(Value::as_bool) == Some(true) {
                let target = b.id.clone();
                for x in self.baselines.iter_mut() {
                    x.is_primary = x.id == target;
                }
            }
        }
        self.run_unrecorded(vec![Change::UpdateBaseline { id: id.into(), patch }])
    }

    pub fn delete_baseline(&mut self, id: &str) -> Result<(), Refused> {
        self.baselines.retain(|b| b.id != id);
        self.run_unrecorded(vec![Change::DeleteBaseline { id: id.into() }])
    }
}

/// Renames a temporary id in every row of `d`.
fn rename_in_data(d: &mut ProjectData, from: &str, to: &str) {
    let fix = |s: &mut String| {
        if s == from {
            *s = to.to_string();
        }
    };
    for t in d.tasks.iter_mut() {
        fix(&mut t.id);
        if let Some(p) = t.parent_id.as_mut() {
            fix(p);
        }
    }
    for x in d.dependencies.iter_mut() {
        fix(&mut x.id);
        fix(&mut x.from_task_id);
        fix(&mut x.to_task_id);
    }
    for a in d.assignments.iter_mut() {
        fix(&mut a.id);
        fix(&mut a.task_id);
        fix(&mut a.resource_id);
    }
    for r in d.resources.iter_mut() {
        fix(&mut r.id);
    }
}

/// JSON equality with numbers compared as numbers (`3` = `3.0`).
fn same_value(a: &Value, b: &Value) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => x == y,
        _ => a == b,
    }
}

fn obj(pairs: Vec<(&str, Value)>) -> Map<String, Value> {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor() -> Editor {
        let mut d = ProjectData::default();
        d.project.id = "p".into();
        d.project.start_date = Some("2026-09-07".into());
        for (i, id) in ["a", "b", "c"].iter().enumerate() {
            let mut t = Task::new(*id, "p");
            t.name = id.to_uppercase();
            t.position = i as i32;
            t.created_at = format!("2026-01-01T00:00:0{i}Z");
            d.tasks.push(t);
        }
        crate::wbs::renumber(&mut d.tasks);
        Editor::new(d, Date::new(2026, 9, 7).expect("d"), false)
    }

    /// Plays the server: acknowledges every queued call, minting `srv-<n>` ids for creations.
    fn drain(e: &mut Editor) -> Vec<String> {
        let mut log = Vec::new();
        let mut n = 0;
        while let Outgoing::Send { seq, call } = e.next_outgoing() {
            log.push(format!("{} {}", call.method, call.path));
            n += 1;
            let id = format!("srv-{n}");
            let resp = json!({"task": {"id": id}, "dependency": {"id": id}, "assignment": {"id": id}, "resource": {"id": id}});
            e.acknowledge(seq, &resp);
        }
        log
    }

    #[test]
    fn typing_coalesces_into_one_entry() {
        let mut e = editor();
        e.set_now(1_000);
        e.set_task_field("a", "name", json!("D")).expect("ok");
        e.set_now(1_500);
        e.set_task_field("a", "name", json!("Do")).expect("ok");
        e.set_now(3_000);
        e.set_task_field("a", "name", json!("Dog")).expect("ok");
        assert_eq!(e.undo_stack.len(), 2);
        e.undo();
        assert_eq!(e.data().task("a").map(|t| t.name.as_str()), Some("Do"));
        e.undo();
        assert_eq!(e.data().task("a").map(|t| t.name.as_str()), Some("A"));
        e.redo();
        assert_eq!(e.data().task("a").map(|t| t.name.as_str()), Some("Do"));
        // Every keystroke was a PATCH, as on the web.
        assert_eq!(drain(&mut e).len(), 6);
    }

    #[test]
    fn delete_then_undo_recreates_under_new_ids() {
        let mut e = editor();
        e.indent("b").expect("indent");
        e.add_dependency("a", "c", false).expect("link");
        drain(&mut e);
        e.delete_task("a").expect("deleted");
        assert_eq!(e.data().tasks.len(), 1);
        assert!(e.data().dependencies.is_empty());
        let log = drain(&mut e);
        assert_eq!(log, vec!["DELETE /projects/p/tasks/a"]);
        e.undo();
        let log = drain(&mut e);
        // a re-created, then its child b under the NEW a, the link, then the scheduler.
        assert_eq!(log[0], "POST /projects/p/tasks");
        assert!(log.iter().any(|l| l == "POST /projects/p/dependencies"));
        assert_eq!(log.last().map(String::as_str), Some("POST /projects/p/cpm"));
        let new_a = e.resolve("a");
        assert!(new_a.starts_with("srv-"));
        let b = e.data().tasks.iter().find(|t| t.name == "B").expect("b");
        assert_eq!(b.parent_id.as_deref(), Some(new_a.as_str()));
        // Redo deletes the re-created task through the remap.
        e.redo();
        assert_eq!(drain(&mut e), vec![format!("DELETE /projects/p/tasks/{new_a}")]);
    }

    #[test]
    fn a_create_waits_for_its_real_id() {
        let mut e = editor();
        let id = e.create_task(None, Some("milestone"), None).expect("created");
        assert!(id.starts_with(TEMP_PREFIX));
        e.set_task_field(&id, "priority", json!("high")).expect("edited");
        let Outgoing::Send { seq, call } = e.next_outgoing() else { panic!("send") };
        assert_eq!(call.body, Some(json!({"task_type": "milestone"})));
        assert_eq!(e.next_outgoing(), Outgoing::Busy);
        e.acknowledge(seq, &json!({"task": {"id": "T9"}}));
        let Outgoing::Send { call, .. } = e.next_outgoing() else { panic!("send") };
        assert_eq!(call.path, "/projects/p/tasks/T9");
        assert!(e.data().task("T9").is_some());
    }

    #[test]
    fn a_refused_call_drops_its_command_and_the_history() {
        let mut e = editor();
        e.add_dependency("a", "b", true).expect("link");
        let Outgoing::Send { seq, .. } = e.next_outgoing() else { panic!("send") };
        e.reject(seq, "boucle");
        assert!(!e.has_pending());
        assert!(e.data().dependencies.is_empty());
        assert!(!e.can_undo());
        assert_eq!(e.take_messages(), vec!["boucle".to_string()]);
        // A link closing a loop is refused locally, before any call.
        e.add_dependency("a", "b", false).expect("ok");
        assert!(e.add_dependency("b", "a", false).is_err());
    }

    #[test]
    fn offline_edits_conflict_with_newer_server_values() {
        let mut e = editor();
        e.set_task_field("a", "priority", json!("high")).expect("ok");
        e.set_task_field("b", "priority", json!("low")).expect("ok");
        let Outgoing::Send { seq, .. } = e.next_outgoing() else { panic!("send") };
        e.offline(seq);
        // Someone changed a's priority meanwhile; b is untouched.
        let mut server = e.base.clone();
        server.tasks[0].priority = "critical".into();
        e.rebase(server);
        assert_eq!(e.next_outgoing(), Outgoing::Conflict(vec!["A".into()]));
        e.resolve_conflict(true);
        assert!(matches!(e.next_outgoing(), Outgoing::Send { .. }));
        // Journal round trip.
        let j = e.journal();
        let back = Editor::restore(serde_json::from_slice(&serde_json::to_vec(&j).expect("json")).expect("journal"), Date::new(2026, 9, 7).expect("d"), false);
        assert_eq!(back.pending_count(), 2);
        assert_eq!(back.data().task("a").map(|t| t.priority.as_str()), Some("high"));
        // Taking theirs drops the unsent edits.
        let mut e2 = back;
        let mut server = e2.base.clone();
        server.tasks[1].priority = "critical".into();
        e2.rebase(server);
        assert!(matches!(e2.next_outgoing(), Outgoing::Conflict(_)));
        e2.resolve_conflict(false);
        assert!(!e2.has_pending());
        assert_eq!(e2.data().task("b").map(|t| t.priority.as_str()), Some("critical"));
    }

    #[test]
    fn moving_the_start_earlier_pins_free_leaves() {
        let mut e = editor();
        for t in e.data.tasks.iter_mut() {
            t.early_start = Some(2);
        }
        e.base = e.data.clone();
        e.commit_marker(true, -3).expect("moved");
        let log = drain(&mut e);
        assert_eq!(log[0], "PATCH /projects/p");
        assert_eq!(log.iter().filter(|l| l.starts_with("PATCH /projects/p/tasks/")).count(), 3);
        assert_eq!(log.last().map(String::as_str), Some("POST /projects/p/cpm"));
        assert_eq!(e.data().task("a").and_then(|t| t.constraint_date.clone()), Some("2026-09-09".into()));
    }

    #[test]
    fn reorder_and_predecessors() {
        let mut e = editor();
        e.reorder("c", "a", Place::Before).expect("moved");
        assert_eq!(e.data().tasks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["c", "a", "b"]);
        e.reorder("b", "c", Place::Inside).expect("nested");
        assert_eq!(e.data().task("b").and_then(|t| t.parent_id.clone()), Some("c".into()));
        assert_eq!(e.data().task("b").map(|t| t.wbs.as_str()), Some("1.1"));
        e.set_predecessors("a", "1").expect("linked");
        assert_eq!(view::predecessor_text(e.data(), "a"), "1");
        e.undo();
        assert!(e.data().dependencies.is_empty());
    }
}
